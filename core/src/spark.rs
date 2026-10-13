//! Cursor trail. A ring buffer of ballistic particles; we integrate and emit a
//! compact draw list (x, y, size, colour) and JS blits 1-3px "+" stars.

use crate::math::{exp, min};
use crate::rng::{hash32, Pcg};
use crate::Slot;

const CAP: usize = 384;

struct Sparks {
    x: [f32; CAP],
    y: [f32; CAP],
    vx: [f32; CAP],
    vy: [f32; CAP],
    life: [f32; CAP],
    max: [f32; CAP],
    col: [f32; CAP],
    head: usize,
    frame: u32,
    out: [f32; CAP * 4],
    rng: Pcg,
}

static SP: Slot<Sparks> = Slot::new(Sparks {
    x: [0.0; CAP],
    y: [0.0; CAP],
    vx: [0.0; CAP],
    vy: [0.0; CAP],
    life: [0.0; CAP],
    max: [0.0; CAP],
    col: [0.0; CAP],
    head: 0,
    frame: 0,
    out: [0.0; CAP * 4],
    rng: Pcg::fixed(),
});

pub fn reseed(seed: u32) {
    SP.get().rng.seed(seed as u64, 0x5AA);
}

pub fn clear() {
    SP.get().life.fill(0.0);
}

#[no_mangle]
pub extern "C" fn spark_out() -> *const f32 {
    SP.get().out.as_ptr()
}

#[no_mangle]
/// `col`: palette index (see overlay.mjs), 5 = electric mix, 7 = fire mix, 255 = random.
pub extern "C" fn spark_emit(x: f32, y: f32, vx: f32, vy: f32, n: u32, col: u32) {
    let s = SP.get();
    for _ in 0..min(n as f32, 32.0) as u32 {
        let i = s.head;
        s.head = (s.head + 1) % CAP;
        s.x[i] = x + s.rng.sym() * 4.0;
        s.y[i] = y + s.rng.sym() * 4.0;
        s.vx[i] = vx * 0.12 + s.rng.sym() * 70.0;
        s.vy[i] = vy * 0.12 + s.rng.sym() * 70.0 - 50.0;
        s.max[i] = 0.45 + s.rng.f01() * 0.6;
        s.life[i] = s.max[i];
        s.col[i] = match col {
            255 => (s.rng.next_u32() % 5) as f32,
            5 => (5 + s.rng.next_u32() % 2) as f32,     // electric blue-whites
            7 => (7 + s.rng.next_u32() % 2) as f32,     // fire: orange / yellow
            c => min(c as f32, 11.0),
        };
    }
}

/// Integrate and write the draw list. Returns number of entries.
#[no_mangle]
pub extern "C" fn spark_step(dt: f32) -> u32 {
    let s = SP.get();
    s.frame = s.frame.wrapping_add(1);
    let drag = exp(-dt * 1.6);
    let mut n = 0;
    for i in 0..CAP {
        if s.life[i] <= 0.0 {
            continue;
        }
        s.life[i] -= dt;
        s.vy[i] += 240.0 * dt;
        s.vx[i] *= drag;
        s.vy[i] *= drag;
        s.x[i] += s.vx[i] * dt;
        s.y[i] += s.vy[i] * dt;
        if s.life[i] <= 0.0 {
            continue;
        }
        // twinkle: drop ~1 frame in 4, per particle, deterministically
        if hash32(i as u32 ^ s.frame.wrapping_mul(0x51ed_270b)) & 3 == 0 {
            continue;
        }
        let k = s.life[i] / s.max[i];
        let size = if k > 0.6 { 3.0 } else if k > 0.3 { 2.0 } else { 1.0 };
        let o = n * 4;
        s.out[o] = s.x[i];
        s.out[o + 1] = s.y[i];
        s.out[o + 2] = size;
        s.out[o + 3] = s.col[i];
        n += 1;
    }
    n as u32
}
