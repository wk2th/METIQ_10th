//! Value noise in 1, 2 (tileable) and 3 dimensions, quintic-faded.

use crate::math::{floor, lerp};
use crate::rng::{hash32, hsym};

#[inline(always)]
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline(always)]
fn lat1(i: i32, seed: u32) -> f32 {
    hsym(hash32((i as u32) ^ seed.wrapping_mul(0x9e37_79b9)))
}

/// 1-D value noise in [-1, 1].
pub fn n1(x: f32, seed: u32) -> f32 {
    let i = floor(x);
    let f = x - i;
    let i = i as i32;
    lerp(lat1(i, seed), lat1(i + 1, seed), fade(f))
}

#[inline(always)]
fn lat3(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    hsym(hash32(
        (x as u32).wrapping_mul(0x8da6_b343)
            ^ (y as u32).wrapping_mul(0xd816_3841)
            ^ (z as u32).wrapping_mul(0xcb1a_b31f)
            ^ seed,
    ))
}

/// 3-D value noise in [-1, 1].
pub fn n3(x: f32, y: f32, z: f32, seed: u32) -> f32 {
    let (ix, iy, iz) = (floor(x), floor(y), floor(z));
    let (u, v, w) = (fade(x - ix), fade(y - iy), fade(z - iz));
    let (x0, y0, z0) = (ix as i32, iy as i32, iz as i32);
    let c = |dx: i32, dy: i32, dz: i32| lat3(x0 + dx, y0 + dy, z0 + dz, seed);
    lerp(
        lerp(lerp(c(0, 0, 0), c(1, 0, 0), u), lerp(c(0, 1, 0), c(1, 1, 0), u), v),
        lerp(lerp(c(0, 0, 1), c(1, 0, 1), u), lerp(c(0, 1, 1), c(1, 1, 1), u), v),
        w,
    )
}

/// Fractal sum of `n3`. Each octave is offset to break lattice alignment.
pub fn fbm3(x: f32, y: f32, z: f32, octaves: u32, seed: u32) -> f32 {
    let (mut a, mut f, mut s, mut norm) = (0.5, 1.0, 0.0, 0.0);
    for o in 0..octaves {
        let k = o as f32 * 17.13;
        s += a * n3(x * f + k, y * f - k, z * f + k * 0.5, seed.wrapping_add(o));
        norm += a;
        a *= 0.5;
        f *= 2.03;
    }
    s / norm
}

#[inline(always)]
fn lat2t(x: i32, y: i32, p: i32, seed: u32) -> f32 {
    let (x, y) = (x.rem_euclid(p) as u32, y.rem_euclid(p) as u32);
    hsym(hash32(x.wrapping_mul(0x27d4_eb2d) ^ y.wrapping_mul(0x1656_67b1) ^ seed))
}

/// 2-D value noise that repeats every `period` lattice cells.
pub fn n2t(x: f32, y: f32, period: i32, seed: u32) -> f32 {
    let (ix, iy) = (floor(x), floor(y));
    let (u, v) = (fade(x - ix), fade(y - iy));
    let (x0, y0) = (ix as i32, iy as i32);
    lerp(
        lerp(lat2t(x0, y0, period, seed), lat2t(x0 + 1, y0, period, seed), u),
        lerp(lat2t(x0, y0 + 1, period, seed), lat2t(x0 + 1, y0 + 1, period, seed), u),
        v,
    )
}

/// Tileable fBm: frequency doubles exactly, so the period doubles with it and
/// every octave still wraps on the same tile boundary.
pub fn fbm2t(x: f32, y: f32, period: i32, octaves: u32, seed: u32) -> f32 {
    let (mut a, mut f, mut p, mut s, mut norm) = (0.5, 1.0, period, 0.0, 0.0);
    for o in 0..octaves {
        s += a * n2t(x * f, y * f, p, seed.wrapping_add(o * 101));
        norm += a;
        a *= 0.5;
        f *= 2.0;
        p *= 2;
    }
    s / norm
}
