//! Signal noise overlay: white noise, interlace scanlines, a rolling VHS
//! hum bar and, during glitches, magenta/cyan tear bands.
//!
//! The noise floor is generated four pixels at a time: four independent
//! xorshift32 generators live in the lanes of one `v128`, and the RGBA word for
//! each pixel is assembled with lane shifts – no scalar loop touches the
//! common path.

use core::arch::wasm32::*;

use crate::math::{exp, fract, min};
use crate::rng::{hash32, Pcg};
use crate::Slot;

pub const MAXW: usize = 320;
pub const MAXH: usize = 320;

// Pixel buffers live in their own all-zero statics so they land in .bss and
// cost nothing in the binary; only the small state structs go to .data.
static BUF: Slot<[u8; MAXW * MAXH * 4]> = Slot::new([0; MAXW * MAXH * 4]);

struct Crt {
    lanes: [u32; 4],
    frame: u32,
}

static CRT: Slot<Crt> = Slot::new(Crt {
    lanes: [0x1234_5678, 0x9abc_def0, 0x0fed_cba9, 0x8765_4321],
    frame: 0,
});

pub fn reseed(seed: u32) {
    let mut r = Pcg::fixed();
    r.seed(seed as u64, 0xC127);
    for l in CRT.get().lanes.iter_mut() {
        *l = r.next_u32() | 1; // xorshift must never be zero
    }
}

#[inline(always)]
fn xorshift4(s: v128) -> v128 {
    let s = v128_xor(s, u32x4_shl(s, 13));
    let s = v128_xor(s, u32x4_shr(s, 17));
    v128_xor(s, u32x4_shl(s, 5))
}

#[no_mangle]
pub extern "C" fn crt_render(w: u32, h: u32, t: f32, energy: f32, glitch: f32) -> *const u8 {
    let c = CRT.get();
    let buf = BUF.get();
    let w = (min(w as f32, MAXW as f32) as usize) & !3;
    let h = min(h as f32, MAXH as f32) as usize;
    c.frame = c.frame.wrapping_add(1);

    let mut s = u32x4(c.lanes[0], c.lanes[1], c.lanes[2], c.lanes[3]);
    let mask = u32x4_splat(0xff);
    let k_lo = f32x4_splat(0.35);
    let k_hi = f32x4_splat(0.65 / 255.0);

    let bar_c = fract(t * 0.11) * 1.5 - 0.25;
    let tear_seed = hash32((c.frame / 3) ^ 0x00C0_FFEE);
    let speck_p = (energy * energy * 48.0) as u32;

    for y in 0..h {
        let fy = y as f32 / h as f32;
        let d = (fy - bar_c) * (1.0 / 0.07);
        let bar = exp(-d * d);
        let scan = (y & 1) as f32;
        let base = min(energy * (14.0 + 46.0 * bar) + scan * energy * 20.0, 140.0);
        let row = &mut buf[y * w * 4..(y + 1) * w * 4];
        let hr = hash32((y as u32 / 2) ^ tear_seed);

        if glitch > 0.0 && hr & 7 == 0 {
            let shift = (hr >> 8) % 40;
            let a = (110.0 * glitch) as u8;
            for x in 0..w {
                let hx = hash32(x as u32 ^ hr);
                let p = x * 4;
                let (r, g, b) = if hx & 3 == 0 {
                    (255, 255, 255)
                } else if ((x as u32 + shift) / 6) & 1 == 1 {
                    (255, 0, 255)
                } else {
                    (0, 255, 255)
                };
                row[p] = r;
                row[p + 1] = g;
                row[p + 2] = b;
                row[p + 3] = a;
            }
            continue;
        }

        let vbase = f32x4_splat(base);
        let mut x = 0;
        while x < w {
            s = xorshift4(s);
            let g = v128_and(s, mask);
            let af = f32x4_mul(vbase, f32x4_add(k_lo, f32x4_mul(f32x4_convert_u32x4(g), k_hi)));
            let a = u32x4_trunc_sat_f32x4(af);
            let px = v128_or(
                v128_or(g, u32x4_shl(g, 8)),
                v128_or(u32x4_shl(g, 16), u32x4_shl(a, 24)),
            );
            // SAFETY: x + 4 <= w and the row slice is exactly w*4 bytes long.
            unsafe { v128_store(row.as_mut_ptr().add(x * 4) as *mut v128, px) };
            x += 4;
        }

        let hs = hash32(y as u32 ^ c.frame.wrapping_mul(0x2545_f491));
        if hs & 0xff < speck_p {
            let x0 = ((hs >> 8) as usize) % w;
            let len = 2 + ((hs >> 24) & 3) as usize;
            for x in x0..min((x0 + len) as f32, w as f32) as usize {
                row[x * 4..x * 4 + 4].copy_from_slice(&[255, 255, 255, 200]);
            }
        }
    }

    c.lanes = [
        u32x4_extract_lane::<0>(s),
        u32x4_extract_lane::<1>(s),
        u32x4_extract_lane::<2>(s),
        u32x4_extract_lane::<3>(s),
    ];
    buf.as_ptr()
}
