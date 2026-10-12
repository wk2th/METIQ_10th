//! BACKGROUND="marble.gif", procedurally. A seamless 128x128 tile:
//! tileable fBm warps a sine that completes exactly two periods across the
//! tile, then it is ordered-dithered to 32 grey levels so it carries the
//! grain of an 8-bit original.

use crate::math::{floor, sat, sin, TAU};
use crate::noise::fbm2t;
use crate::palette::bayer;
use crate::rng::{h01, hash32};
use crate::Slot;

pub const T: usize = 128;

static TX: Slot<[u8; T * T * 4]> = Slot::new([0; T * T * 4]);

#[no_mangle]
pub extern "C" fn texture_size() -> u32 {
    T as u32
}

#[no_mangle]
pub extern "C" fn texture_render(seed: u32) -> *const u8 {
    let buf = TX.get();
    for y in 0..T {
        for x in 0..T {
            let (fx, fy) = (x as f32 / T as f32, y as f32 / T as f32);
            let n = fbm2t(fx * 4.0, fy * 4.0, 4, 5, seed);
            let m = 0.5 + 0.5 * sin((fx + fy) * TAU * 2.0 + n * 5.5);
            let grain = (h01(hash32((y * T + x) as u32 ^ seed)) - 0.5) * 0.025;
            let v = sat(0.80 + 0.13 * m * m + grain);
            let q = floor(v * 31.0 + bayer(x as u32, y as u32)) / 31.0;
            let i4 = (y * T + x) * 4;
            buf[i4] = (sat(q * 0.975) * 255.0) as u8;
            buf[i4 + 1] = (sat(q * 0.985) * 255.0) as u8;
            buf[i4 + 2] = (sat(q) * 255.0) as u8;
            buf[i4 + 3] = 255;
        }
    }
    buf.as_ptr()
}
