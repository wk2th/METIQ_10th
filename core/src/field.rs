//! Glyph field.
//!
//! JS splits the document's text into one inline-block per grapheme and hands
//! us their rest positions. Each frame, per glyph, we compute two things:
//!
//! 1. **Motion** – a damped spring toward a target offset made of
//!    tremble (1-D value noise, ∝ energy^1.5) plus the bob of any ripple
//!    wavefront currently passing under it.
//!
//! 2. **Decoding** – hovering a glyph drops a ripple at it. The ring expands
//!    at `SPEED` px/s through every glyph within `RADIUS`, above and below
//!    included. When the front reaches a glyph its text is "corrupted" and
//!    then recomputed back to the truth through three passes:
//!
//!    ```text
//!    p ∈ [0, 0.38)  mojibake   縺繧繝 / ｮｯﾃ Ã© �   (bytes read as the wrong codec)
//!    p ∈ [0.38,0.66) greek     ΣΦΨ / αβγ           (re-deriving: symbol space)
//!    p ∈ [0.66,0.92) maths     ∑∫∂∇≈≡             (solving; truth leaks through)
//!    p ≥ 0.92        original
//!    ```
//!
//!    Glyphs further from the impact start later in that schedule (they get
//!    "less damaged") and decode faster. The candidate codepoint re-rolls at
//!    24 Hz, so the text visibly churns while it converges.
//!
//! Only glyphs whose transform or codepoint actually changed are reported
//! (`dirty`), so the DOM sees zero writes once everything has settled.

use crate::math::{abs, clamp, exp, min, rsqrt, sqrt};
use crate::noise::n1;
use crate::rng::{h01, hash32};
use crate::Slot;

pub const CAP: usize = 4096;
const IN: usize = 4; // x, y, kind, wide
const OUT: usize = 4; // dx, dy, rot(deg), chroma(px)

/// Tremble px and rotation gain per kind: 0 text, 1 link, 2 headline, 3 marquee.
const KIND: [(f32, f32); 4] = [(0.75, 0.35), (1.0, 0.8), (2.6, 1.4), (0.9, 0.6)];

// ζ ≈ 0.30 – deliberately underdamped so letters overshoot and wobble home.
const K: f32 = 420.0;
const C: f32 = 12.3;
const KR: f32 = 300.0;
const CR: f32 = 10.0;

const RIPPLES: usize = 16;
const SPEED: f32 = 380.0;
const RADIUS: f32 = 150.0;
const LIFE: f32 = RADIUS / SPEED + 1.5;

#[rustfmt::skip]
const MOJI_WIDE: [char; 32] = [
    '縺', '繧', '繝', '蜷', '荳', '譁', '螟', '鬮', '髢', '驕', '蛻', '遘', '豁', '莉', '逕', '譛',
    '謨', '諢', '縲', '繼', '譌', '霑', '蟄', '鄒', '迺', '隱', '箕', '凵', '﨑', '炊', '�', '■',
];
#[rustfmt::skip]
const MOJI_NARROW: [char; 32] = [
    'ｮ', 'ｯ', 'ﾃ', 'ｦ', 'ｱ', 'ｼ', 'ﾏ', 'ｹ', 'ﾘ', 'ﾂ', 'ｿ', '｢', '｣', '･', 'Ã', 'â',
    '©', '€', '™', 'Â', '¢', '¤', 'þ', 'ð', 'Ø', 'µ', '¿', '§', '¶', 'ﾟ', '�', '¦',
];
#[rustfmt::skip]
const GREEK_WIDE: [char; 16] = [
    'Α', 'Β', 'Γ', 'Δ', 'Θ', 'Λ', 'Ξ', 'Π', 'Σ', 'Φ', 'Ψ', 'Ω', 'Ε', 'Η', 'Κ', 'Μ',
];
#[rustfmt::skip]
const GREEK_NARROW: [char; 16] = [
    'α', 'β', 'γ', 'δ', 'ε', 'ζ', 'η', 'θ', 'λ', 'μ', 'ξ', 'π', 'ρ', 'σ', 'φ', 'ψ',
];
#[rustfmt::skip]
const MATHS: [char; 32] = [
    '∀', '∂', '∃', '∅', '∇', '∈', '∉', '∏', '∑', '√', '∝', '∞', '∠', '∧', '∨', '∩',
    '∪', '∫', '∮', '∴', '≅', '≈', '≠', '≡', '≤', '≥', '⊂', '⊃', '⊕', '⊗', '⊥', 'ℵ',
];

struct Field {
    inp: [f32; CAP * IN],
    out: [f32; CAP * OUT],
    code: [u32; CAP],
    dirty: [u32; CAP],
    ox: [f32; CAP],
    oy: [f32; CAP],
    or: [f32; CAP],
    vx: [f32; CAP],
    vy: [f32; CAP],
    vr: [f32; CAP],
    live: [u8; CAP],
}

// Kept apart from `Field` so its non-zero initialiser doesn't drag ~250 KB of
// zeroed glyph state from .bss into the binary's data segment.
struct Ripples {
    rip: [(f32, f32, f32); RIPPLES],
    head: usize,
}

static R: Slot<Ripples> = Slot::new(Ripples { rip: [(0.0, 0.0, -1e9); RIPPLES], head: 0 });

static F: Slot<Field> = Slot::new(Field {
    inp: [0.0; CAP * IN],
    out: [0.0; CAP * OUT],
    code: [0; CAP],
    dirty: [0; CAP],
    ox: [0.0; CAP],
    oy: [0.0; CAP],
    or: [0.0; CAP],
    vx: [0.0; CAP],
    vy: [0.0; CAP],
    vr: [0.0; CAP],
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

/// Codepoint each glyph should currently display; 0 = its own text.
#[no_mangle]
pub extern "C" fn field_codes() -> *const u32 {
    F.get().code.as_ptr()
}

#[no_mangle]
pub extern "C" fn field_dirty() -> *const u32 {
    F.get().dirty.as_ptr()
}

/// Drop a ripple at document position (x, y) at time t.
#[no_mangle]
pub extern "C" fn field_ripple(x: f32, y: f32, t: f32) {
    let r = R.get();
    r.rip[r.head] = (x, y, t);
    r.head = (r.head + 1) % RIPPLES;
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
    f.live.fill(0);
    f.out.fill(0.0);
    f.code.fill(0);
    R.get().rip = [(0.0, 0.0, -1e9); RIPPLES];
}

#[inline]
fn pick(pool: &[char], h: u32) -> u32 {
    pool[(h as usize) % pool.len()] as u32
}

/// Integrate `n` glyphs. Returns the dirty count.
#[no_mangle]
pub extern "C" fn field_step(n: u32, dt: f32, t: f32, energy: f32) -> u32 {
    let f = F.get();
    let n = min(n as f32, CAP as f32) as usize;
    let dt = clamp(dt, 0.0, 1.0 / 30.0);
    let h = dt * 0.5;
    let e15 = energy * sqrt(energy);
    let tick = (t * 24.0) as u32;

    // Cull spent ripples once per frame rather than per glyph.
    let mut active = [(0.0f32, 0.0f32, 0.0f32); RIPPLES];
    let mut na = 0;
    for r in R.get().rip.iter() {
        if t - r.2 < LIFE {
            active[na] = *r;
            na += 1;
        }
    }

    let mut nd = 0usize;
    for i in 0..n {
        let b = i * IN;
        let (x, y, k, wide) = (f.inp[b], f.inp[b + 1], f.inp[b + 2], f.inp[b + 3] > 0.5);
        let kind = min(k, 3.0) as usize;
        let (trem, rg) = KIND[kind];

        let seed = (i as u32).wrapping_mul(0x9e37_79b1);
        let ti = t * 9.0 + i as f32 * 0.37;
        let a = trem * e15;
        let tx = n1(ti, seed) * a;
        let mut ty = n1(ti, seed ^ 0x55) * a;
        let tr = n1(t * 5.0 + i as f32 * 0.21, seed ^ 0xAA) * a * rg * 3.0;

        // Ripples: earliest decode phase wins; the front also lifts the glyph.
        let mut phase = 2.0f32;
        let mut bob = 0.0f32;
        if kind != 3 {
            for r in active[..na].iter() {
                let (dx, dy) = (x - r.0, y - r.1);
                let d2 = dx * dx + dy * dy;
                if d2 > RADIUS * RADIUS {
                    continue;
                }
                let d = d2 * rsqrt(d2 + 1e-6);
                let q = d / RADIUS;
                let local = t - (r.2 + d / SPEED);
                let front = exp(-(local * 14.0) * (local * 14.0)) * (1.0 - q);
                bob = if front > bob { front } else { bob };
                if local < 0.0 {
                    continue;
                }
                let dur = 0.75 + 0.6 * (1.0 - q);
                let p = local / dur + q * 0.35;
                if p < phase {
                    phase = p;
                }
            }
        }
        ty -= bob * 3.5;

        let code = if phase < 0.92 {
            let hr = hash32(seed ^ tick.wrapping_mul(0x2c1b_3c6d) ^ (i as u32 * 7919));
            if phase < 0.38 {
                pick(if wide { &MOJI_WIDE } else { &MOJI_NARROW }, hr)
            } else if phase < 0.66 {
                pick(if wide { &GREEK_WIDE } else { &GREEK_NARROW }, hr)
            } else if h01(hr) < (phase - 0.66) / 0.26 {
                0 // the right answer flickers in more often as it converges
            } else {
                pick(&MATHS, hr >> 7)
            }
        } else {
            0
        };
        let code_changed = code != f.code[i];
        f.code[i] = code;

        for _ in 0..2 {
            f.vx[i] += (K * (tx - f.ox[i]) - C * f.vx[i]) * h;
            f.vy[i] += (K * (ty - f.oy[i]) - C * f.vy[i]) * h;
            f.vr[i] += (KR * (tr - f.or[i]) - CR * f.vr[i]) * h;
            f.ox[i] += f.vx[i] * h;
            f.oy[i] += f.vy[i] * h;
            f.or[i] += f.vr[i] * h;
        }

        let chroma = min(3.0, (abs(f.vx[i]) + abs(f.vy[i])) * 0.012);
        let live = abs(f.ox[i]) > 0.04 || abs(f.oy[i]) > 0.04 || abs(f.or[i]) > 0.08 || chroma > 0.2;

        if live || f.live[i] != 0 || code_changed {
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
