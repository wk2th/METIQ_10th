//! Glyph spring field.
//!
//! JS splits the document's text into one inline-block per grapheme and hands
//! us their rest positions. Each frame we integrate a damped spring per glyph
//! toward a target offset composed of:
//!
//!   tremble  – 1-D value noise, amplitude ∝ energy^1.5
//!   repel    – (1 - d/R)^2 falloff away from the pointer
//!   wave     – travelling sine through hovered link text
//!
//! and emit only the glyphs whose transform actually changed (`dirty`), so the
//! DOM sees zero writes once everything has settled.

use crate::math::{abs, clamp, cos, damp, min, rsqrt, sin, smooth, sqrt};
use crate::noise::n1;
use crate::Slot;

pub const CAP: usize = 4096;
const IN: usize = 5; // x, y, group, kind, phase
const OUT: usize = 4; // dx, dy, rot(deg), chroma(px)

/// (tremble px, repel radius, repel px, rotation gain) per glyph kind:
/// 0 body text, 1 link, 2 headline, 3 marquee (position unknown → no repel)
const KIND: [(f32, f32, f32, f32); 4] = [
    (0.75, 64.0, 5.0, 0.35),
    (1.0, 72.0, 7.0, 0.8),
    (2.6, 160.0, 34.0, 1.4),
    (0.9, 0.0, 0.0, 0.6),
];

// ζ ≈ 0.30 – deliberately underdamped so letters overshoot and wobble home.
const K: f32 = 420.0;
const C: f32 = 12.3;
const KR: f32 = 300.0;
const CR: f32 = 10.0;

struct Field {
    inp: [f32; CAP * IN],
    out: [f32; CAP * OUT],
    dirty: [u32; CAP],
    ox: [f32; CAP],
    oy: [f32; CAP],
    or: [f32; CAP],
    vx: [f32; CAP],
    vy: [f32; CAP],
    vr: [f32; CAP],
    hov: [f32; CAP],
    live: [u8; CAP],
}

static F: Slot<Field> = Slot::new(Field {
    inp: [0.0; CAP * IN],
    out: [0.0; CAP * OUT],
    dirty: [0; CAP],
    ox: [0.0; CAP],
    oy: [0.0; CAP],
    or: [0.0; CAP],
    vx: [0.0; CAP],
    vy: [0.0; CAP],
    vr: [0.0; CAP],
    hov: [0.0; CAP],
    live: [0; CAP],
});

#[no_mangle]
pub extern "C" fn field_cap() -> u32 {
    CAP as u32
}

#[no_mangle]
pub extern "C" fn field_in() -> *mut f32 {
    F.get().inp.as_mut_ptr()
}

#[no_mangle]
pub extern "C" fn field_out() -> *const f32 {
    F.get().out.as_ptr()
}

#[no_mangle]
pub extern "C" fn field_dirty() -> *const u32 {
    F.get().dirty.as_ptr()
}

#[no_mangle]
pub extern "C" fn field_reset() {
    let f = F.get();
    f.ox.fill(0.0);
    f.oy.fill(0.0);
    f.or.fill(0.0);
    f.vx.fill(0.0);
    f.vy.fill(0.0);
    f.vr.fill(0.0);
    f.hov.fill(0.0);
    f.live.fill(0);
    f.out.fill(0.0);
}

/// Integrate `n` glyphs. Pointer is in document coordinates; `hot` is the
/// group id under the pointer (-1 for none). Returns the dirty count.
#[no_mangle]
pub extern "C" fn field_step(n: u32, dt: f32, t: f32, mx: f32, my: f32, energy: f32, hot: i32) -> u32 {
    let f = F.get();
    let n = min(n as f32, CAP as f32) as usize;
    let dt = clamp(dt, 0.0, 1.0 / 30.0);
    let h = dt * 0.5;
    let e15 = energy * sqrt(energy);
    let wave_gain = 0.35 + 0.65 * smooth(energy * 2.0);
    let mut nd = 0usize;

    for i in 0..n {
        let b = i * IN;
        let (x, y, g, k, ph) = (f.inp[b], f.inp[b + 1], f.inp[b + 2], f.inp[b + 3], f.inp[b + 4]);
        let (trem, rad, push, rg) = KIND[min(k, 3.0) as usize];

        let is_hot = hot >= 0 && g as i32 == hot;
        let hv = &mut f.hov[i];
        *hv += ((is_hot as u32 as f32) - *hv) * damp(if is_hot { 16.0 } else { 6.0 }, dt);
        let hv = *hv;

        let seed = (i as u32).wrapping_mul(0x9e37_79b1);
        let ti = t * 9.0 + i as f32 * 0.37;
        let a = trem * e15;
        let mut tx = n1(ti, seed) * a;
        let mut ty = n1(ti, seed ^ 0x55) * a;
        let mut tr = n1(t * 5.0 + i as f32 * 0.21, seed ^ 0xAA) * a * rg * 3.0;

        if rad > 0.0 {
            let (dx, dy) = (x - mx, y - my);
            let d2 = dx * dx + dy * dy;
            if d2 < rad * rad {
                let inv = rsqrt(d2 + 1e-6);
                let q = 1.0 - d2 * inv / rad;
                let force = q * q * push * energy;
                let (ux, uy) = (dx * inv, dy * inv);
                tx += ux * force;
                ty += uy * force;
                tr += ux * force * rg;
            }
        }

        if hv > 0.001 {
            let w = t * 11.0 - ph * 0.8;
            let amp = hv * wave_gain;
            ty -= sin(w) * 3.2 * amp;
            tr += cos(w) * 5.0 * amp;
        }

        for _ in 0..2 {
            f.vx[i] += (K * (tx - f.ox[i]) - C * f.vx[i]) * h;
            f.vy[i] += (K * (ty - f.oy[i]) - C * f.vy[i]) * h;
            f.vr[i] += (KR * (tr - f.or[i]) - CR * f.vr[i]) * h;
            f.ox[i] += f.vx[i] * h;
            f.oy[i] += f.vy[i] * h;
            f.or[i] += f.vr[i] * h;
        }

        let chroma = min(3.0, (abs(f.vx[i]) + abs(f.vy[i])) * 0.012 + hv * energy * 1.5);
        let live = abs(f.ox[i]) > 0.04 || abs(f.oy[i]) > 0.04 || abs(f.or[i]) > 0.08 || chroma > 0.2;

        if live || f.live[i] != 0 {
            let o = i * OUT;
            if live {
                f.out[o] = f.ox[i];
                f.out[o + 1] = f.oy[i];
                f.out[o + 2] = f.or[i];
                f.out[o + 3] = chroma;
            } else {
                f.ox[i] = 0.0;
                f.oy[i] = 0.0;
                f.or[i] = 0.0;
                f.vx[i] = 0.0;
                f.vy[i] = 0.0;
                f.vr[i] = 0.0;
                f.out[o..o + OUT].fill(0.0);
            }
            f.dirty[nd] = i as u32;
            nd += 1;
        }
        f.live[i] = live as u8;
    }
    nd as u32
}
