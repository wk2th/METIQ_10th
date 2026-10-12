//! Table collapse.
//!
//! Every minute or so JS picks a visible bordered table and hands us its cells
//! ("shards": rest centre + the contiguous glyph range inside). Timeline per
//! shard, τ seconds after the start:
//!
//! ```text
//!  crack   τ < delay₁           cracks run out from a random shard at 300 px/s;
//!                               the shard shivers and its text starts rotting
//!  fall    0 ≤ τ−delay₁ < 1.1   ballistic: hop, drift, spin, g = 1150 px/s²,
//!                               fading out, text is pure mojibake
//!  void    …until REBUILD
//!  seep    0 ≤ τ−REBUILD−delay₂ < 0.8
//!                               the cell wells up from *behind the page*:
//!                               scale 0.35 → 1 (overshoot), blur 8 → 0 px,
//!                               opacity 0 → 1, text decodes Greek → maths → true
//! ```
//!
//! delay₂ grows with distance from the table's centroid, so it re-forms from
//! the middle outwards. Glyph text is driven through `field::ext`, the same
//! decode path the hover ripples use.

use crate::field;
use crate::math::{max, min, sat, sin, sqrt};
use crate::rng::{h01, hash32, hsym};
use crate::Slot;

pub const MAXS: usize = 64;
const IN: usize = 4; // x, y, g0, g1
const OUT: usize = 6; // dx, dy, rot, scale, alpha, blur

const CRACK_SPEED: f32 = 300.0;
const FALL: f32 = 1.1;
const GRAVITY: f32 = 1150.0;
const SEEP: f32 = 0.8;

struct Collapse {
    inp: [f32; MAXS * IN],
    out: [f32; MAXS * OUT],
    n: usize,
    t0: f32,
    seed: u32,
    rebuild: f32,
    origin: (f32, f32),
    active: bool,
}

static C: Slot<Collapse> = Slot::new(Collapse {
    inp: [0.0; MAXS * IN],
    out: [0.0; MAXS * OUT],
    n: 0,
    t0: 0.0,
    seed: 0,
    rebuild: 0.0,
    origin: (0.0, 0.0),
    active: false,
});

#[no_mangle]
pub extern "C" fn shard_cap() -> u32 {
    MAXS as u32
}

#[no_mangle]
pub extern "C" fn shard_in() -> *mut f32 {
    C.get().inp.as_mut_ptr()
}

#[no_mangle]
pub extern "C" fn shard_out() -> *const f32 {
    C.get().out.as_ptr()
}

fn clear_ext(c: &Collapse) {
    let ext = field::ext();
    for s in 0..c.n {
        let (g0, g1) = (c.inp[s * IN + 2] as usize, c.inp[s * IN + 3] as usize);
        for e in ext.iter_mut().take(min(g1 as f32, field::CAP as f32) as usize).skip(g0) {
            *e = 0.0;
        }
    }
}

#[no_mangle]
pub extern "C" fn shard_start(n: u32, t: f32, seed: u32) {
    let c = C.get();
    clear_ext(c);
    c.n = min(n as f32, MAXS as f32) as usize;
    c.t0 = t;
    c.seed = seed;
    c.active = c.n > 0;
    // slowest crack arrival + fall + a beat of emptiness
    let o = (hash32(seed) as usize) % max(c.n as f32, 1.0) as usize;
    let (ox, oy) = (c.inp[o * IN], c.inp[o * IN + 1]);
    let mut far = 0.0f32;
    for s in 0..c.n {
        let (dx, dy) = (c.inp[s * IN] - ox, c.inp[s * IN + 1] - oy);
        far = max(far, sqrt(dx * dx + dy * dy));
    }
    c.rebuild = far / CRACK_SPEED + 0.25 + FALL + 0.45;
    c.origin = (ox, oy);
}

#[no_mangle]
pub extern "C" fn shard_cancel() {
    let c = C.get();
    clear_ext(c);
    c.active = false;
    c.out.fill(0.0);
}

/// Advance. Returns 0 when finished, 1 while falling, 2 while rebuilding.
#[no_mangle]
pub extern "C" fn shard_step(t: f32) -> u32 {
    let c = C.get();
    if !c.active {
        return 0;
    }
    let tau = t - c.t0;
    let (ox, oy) = c.origin;
    let (mut cx, mut cy) = (0.0, 0.0);
    for s in 0..c.n {
        cx += c.inp[s * IN];
        cy += c.inp[s * IN + 1];
    }
    cx /= c.n as f32;
    cy /= c.n as f32;

    let ext = field::ext();
    let mut running = false;
    for s in 0..c.n {
        let b = s * IN;
        let (x, y) = (c.inp[b], c.inp[b + 1]);
        let (g0, g1) = (c.inp[b + 2] as usize, min(c.inp[b + 3], field::CAP as f32) as usize);
        let h = hash32(c.seed ^ (s as u32).wrapping_mul(0x9e37_79b9));
        let (dx, dy) = (x - ox, y - oy);
        let delay1 = sqrt(dx * dx + dy * dy) / CRACK_SPEED + h01(h) * 0.25;
        let (ex, ey) = (x - cx, y - cy);
        let delay2 = sqrt(ex * ex + ey * ey) / 500.0 + h01(h >> 7) * 0.35;

        let (mut ddx, mut ddy, mut rot, mut sc, mut al, mut bl) = (0.0, 0.0, 0.0, 1.0, 1.0, 0.0);
        let phase: f32; // decode phase for the shard's glyphs, <0 = untouched
        let f = tau - delay1;
        let sp = tau - c.rebuild - delay2;

        if f < 0.0 {
            let pre = sat(1.0 + f / 0.35);
            ddx = sin(tau * 90.0 + s as f32) * 1.2 * pre;
            phase = if pre > 0.0 { 0.15 } else { -1.0 };
            running = true;
        } else if f < FALL {
            ddx = hsym(h >> 3) * 90.0 * f;
            ddy = -(40.0 + 90.0 * h01(h >> 9)) * f + 0.5 * GRAVITY * f * f;
            rot = hsym(h >> 13) * 220.0 * f;
            sc = 1.0 - 0.15 * f;
            let k = f / FALL;
            al = 1.0 - k * k;
            phase = 0.1;
            running = true;
        } else if sp < 0.0 {
            al = 0.0;
            phase = 0.1;
            running = true;
        } else if sp < SEEP {
            let k = sp / SEEP;
            // easeOutBack: overshoot a touch, like it surfaced too fast
            let k1 = k - 1.0;
            let ease = 1.0 + 2.7 * k1 * k1 * k1 + 1.7 * k1 * k1;
            sc = 0.35 + 0.65 * ease;
            al = k;
            bl = 8.0 * (1.0 - k) * (1.0 - k);
            rot = hsym(h >> 21) * 6.0 * (1.0 - k);
            phase = 0.38 + 0.62 * k;
            running = true;
        } else {
            phase = -1.0;
        }

        for e in ext.iter_mut().take(g1).skip(g0) {
            *e = if phase >= 0.0 { phase + 1.0 } else { 0.0 };
        }
        let o = s * OUT;
        c.out[o] = ddx;
        c.out[o + 1] = ddy;
        c.out[o + 2] = rot;
        c.out[o + 3] = sc;
        c.out[o + 4] = al;
        c.out[o + 5] = bl;
    }

    if !running {
        c.active = false;
        return 0;
    }
    if tau < c.rebuild { 1 } else { 2 }
}
