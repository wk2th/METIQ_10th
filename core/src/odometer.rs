//! The access counter.
//!
//! Real 1996 counters were a server-side CGI that stitched digit GIFs together.
//! This one is a mechanical drum odometer: each wheel's position is derived
//! from a single continuous count `c` so that a wheel only turns during the
//! last unit of the wheel to its right – i.e. carries propagate exactly like
//! the brass original:
//!
//! ```text
//!   wheel_i(c) = ⌊c/10^i⌋ mod 10 + max(0, (c mod 10^i) − (10^i − 1))
//! ```
//!
//! The drum is rendered as a vertical tape (no digit is ever "drawn at" a
//! position – rows sample the tape), with cylindrical falloff shading. Hover
//! kicks inject angular velocity into per-wheel springs on top of that.

use crate::font::digit_mask;
use crate::math::{abs, damp, floor, floor64, min};
use crate::noise::n1;
use crate::palette::put;
use crate::rng::{hash32, Pcg};
use crate::Slot;

pub const DIGITS: usize = 7;
const CW: usize = 14;
const CH: usize = 22;
pub const W: usize = DIGITS * CW + 4;
pub const H: usize = CH + 4;

const POW10: [f64; 8] = [1.0, 10.0, 100.0, 1e3, 1e4, 1e5, 1e6, 1e7];

static BUF: Slot<[u8; W * H * 4]> = Slot::new([0; W * H * 4]);

struct Odo {
    shown: f64,
    target: f64,
    kick: [f32; DIGITS],
    kv: [f32; DIGITS],
    rng: Pcg,
}

static ODO: Slot<Odo> = Slot::new(Odo {
    shown: 0.0,
    target: 0.0,
    kick: [0.0; DIGITS],
    kv: [0.0; DIGITS],
    rng: Pcg::fixed(),
});

pub fn reseed(seed: u32) {
    ODO.get().rng.seed(seed as u64, 0x0D0);
}

pub fn settle() {
    let o = ODO.get();
    o.kick = [0.0; DIGITS];
    o.kv = [0.0; DIGITS];
    o.shown = o.target;
}

fn wheel(c: f64, i: usize) -> f64 {
    let p = POW10[i];
    let q = floor64(c / p);
    let d = q - 10.0 * floor64(q / 10.0);
    let roll = (c - q * p) - (p - 1.0);
    d + if roll > 0.0 { roll } else { 0.0 }
}

#[no_mangle]
pub extern "C" fn odo_w() -> u32 {
    W as u32
}

#[no_mangle]
pub extern "C" fn odo_h() -> u32 {
    H as u32
}

#[no_mangle]
pub extern "C" fn odo_snap(v: f64) {
    let o = ODO.get();
    o.shown = v;
    o.target = v;
}

#[no_mangle]
pub extern "C" fn odo_target(v: f64) {
    ODO.get().target = v;
}

/// Spin every wheel by a random angular impulse (in digits/s).
#[no_mangle]
pub extern "C" fn odo_kick(strength: f32) {
    let o = ODO.get();
    for i in 0..DIGITS {
        o.kv[i] += o.rng.sym() * strength * (1.0 + i as f32 * 0.25);
    }
}

/// Returns 1 while anything on the drum is still moving.
#[no_mangle]
pub extern "C" fn odo_busy() -> u32 {
    let o = ODO.get();
    let mut busy = abs((o.target - o.shown) as f32) > 1e-3;
    for i in 0..DIGITS {
        busy |= abs(o.kick[i]) > 1e-3 || abs(o.kv[i]) > 1e-3;
    }
    busy as u32
}

#[no_mangle]
pub extern "C" fn odo_render(dt: f32, t: f32, energy: f32, hover: f32) -> *const u8 {
    let o = ODO.get();
    let buf = BUF.get();

    o.shown += (o.target - o.shown) * damp(2.4, dt) as f64;
    if (o.target - o.shown) < 1e-3 {
        o.shown = o.target;
    }

    if hover > 0.0 && o.rng.f01() < dt * 2.5 * hover {
        let j = (o.rng.next_u32() % DIGITS as u32) as usize;
        o.kv[j] += o.rng.sym() * 22.0;
    }

    let mut pos = [0.0f64; DIGITS];
    for i in 0..DIGITS {
        // ζ ≈ 0.37: a kicked wheel spins, overshoots its detent once, settles.
        o.kv[i] += (-90.0 * o.kick[i] - 7.0 * o.kv[i]) * dt;
        o.kick[i] += o.kv[i] * dt;
        if abs(o.kick[i]) < 1e-3 && abs(o.kv[i]) < 1e-3 {
            o.kick[i] = 0.0;
            o.kv[i] = 0.0;
        }
        pos[i] = wheel(o.shown, i) + o.kick[i] as f64;
    }

    let frame = (t * 60.0) as u32;
    for y in 0..H {
        let wob = if energy > 0.0 { floor(n1(t * 25.0 + y as f32 * 1.7, 77) * energy * 1.6 + 0.5) as i32 } else { 0 };
        for x in 0..W {
            let i4 = (y * W + x) * 4;
            let (xi, yi) = (x as i32, y as i32);
            let (w1, h1) = (W as i32 - 1, H as i32 - 1);

            // Sunken 2px bevel, Win3.1 style.
            let bevel = if xi == 0 || yi == 0 {
                Some(0.5)
            } else if xi == w1 || yi == h1 {
                Some(1.0)
            } else if xi == 1 || yi == 1 {
                Some(0.0)
            } else if xi == w1 - 1 || yi == h1 - 1 {
                Some(0.75)
            } else {
                None
            };
            if let Some(v) = bevel {
                put(&mut *buf, i4, x as u32, y as u32, v, v, v);
                continue;
            }

            let ix = (xi - 2 + wob).clamp(0, (DIGITS * CW) as i32 - 1) as usize;
            let iy = y - 2;
            let j = ix / CW;
            let cx = (ix % CW) as i32;
            if cx == CW as i32 - 1 && j < DIGITS - 1 {
                put(&mut *buf, i4, x as u32, y as u32, 0.2, 0.2, 0.2);
                continue;
            }

            let wi = DIGITS - 1 - j;
            let s = floor64(iy as f64 + pos[wi] * CH as f64) as i64;
            let digit = s.div_euclid(CH as i64).rem_euclid(10) as u32;
            let ly = s.rem_euclid(CH as i64) as i32;
            let lit = digit_mask(digit, 2, cx - 2, ly - 4);

            let u = (iy as f32 + 0.5 - CH as f32 * 0.5) / (CH as f32 * 0.5);
            let shade = 1.0 - 0.38 * u * u;
            let (mut r, mut g, mut b) = if lit { (1.0, 0.95, 0.78) } else { (0.03, 0.03, 0.03) };
            r *= shade;
            g *= shade;
            b *= shade;

            if energy > 0.0 {
                let hz = hash32((y * W + x) as u32 ^ frame.wrapping_mul(0x1b87_3593));
                if ((hz & 0xffff) as f32) < energy * 5000.0 {
                    let v = (hz >> 24) as f32 / 255.0;
                    r = v;
                    g = v;
                    b = min(1.0, v * 1.2);
                }
            }
            put(&mut *buf, i4, x as u32, y as u32, r, g, b);
        }
    }
    buf.as_ptr()
}
