//! The 216-colour "Netscape" cube and a 4x4 Bayer matrix.
//!
//! Every surface is shaded in continuous colour and only quantised at the very
//! end, so a still frame reads as an honest 8-bit GIF while the motion between
//! frames stays sub-pixel smooth.

use crate::math::{floor, min, sat};

const BAYER: [u8; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];

#[inline(always)]
pub fn bayer(x: u32, y: u32) -> f32 {
    (BAYER[(((y & 3) << 2) | (x & 3)) as usize] as f32 + 0.5) * (1.0 / 16.0)
}

/// One channel to the 6-level cube {00,33,66,99,CC,FF}.
#[inline(always)]
pub fn cube(c: f32, t: f32) -> u8 {
    (min(floor(sat(c) * 5.0 + t), 5.0) as u8) * 51
}

/// Write an opaque, dithered, web-safe pixel.
#[inline(always)]
pub fn put(buf: &mut [u8], i: usize, x: u32, y: u32, r: f32, g: f32, b: f32) {
    let t = bayer(x, y);
    buf[i] = cube(r, t);
    buf[i + 1] = cube(g, t);
    buf[i + 2] = cube(b, t);
    buf[i + 3] = 255;
}

/// GIF transparency is 1-bit. So is ours.
#[inline(always)]
pub fn clear(buf: &mut [u8], i: usize) {
    buf[i] = 0;
    buf[i + 1] = 0;
    buf[i + 2] = 0;
    buf[i + 3] = 0;
}
