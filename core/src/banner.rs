//! 88x31 link banner ("当サイトのバナーです。お持ち帰りOK!").
//! Raised bevel, navy gradient, chrome-lettered METIQ with a drop shadow and a
//! diagonal specular sweep that only runs while hovered.

use crate::font::{mask, width};
use crate::math::{abs, lerp, sat};
use crate::palette::put;
use crate::rng::hash32;
use crate::Slot;

pub const W: usize = 88;
pub const H: usize = 31;
const TITLE: &[u8] = b"METIQ";
const SUB: &[u8] = b"OFFICIAL SITE";

static BUF: Slot<[u8; W * H * 4]> = Slot::new([0; W * H * 4]);

struct Banner {
    sweep: f32,
}

static BN: Slot<Banner> = Slot::new(Banner { sweep: -40.0 });

#[no_mangle]
pub extern "C" fn banner_w() -> u32 {
    W as u32
}

#[no_mangle]
pub extern "C" fn banner_h() -> u32 {
    H as u32
}

#[no_mangle]
pub extern "C" fn banner_render(dt: f32, t: f32, energy: f32, hover: f32) -> *const u8 {
    let bn = BN.get();
    let buf = BUF.get();
    if hover > 0.01 {
        bn.sweep += dt * 110.0;
        if bn.sweep > 140.0 {
            bn.sweep = -40.0;
        }
    } else {
        bn.sweep = -40.0;
    }

    let tx = (W as i32 - width(TITLE.len(), 2)) / 2;
    let ty = 3;
    let sx = (W as i32 - width(SUB.len(), 1)) / 2;
    let sy = 21;
    let frame = (t * 30.0) as u32;

    for y in 0..H {
        for x in 0..W {
            let i4 = (y * W + x) * 4;
            let (xi, yi) = (x as i32, y as i32);
            let (w1, h1) = (W as i32 - 1, H as i32 - 1);
            let fy = y as f32 / H as f32;

            let (mut r, mut g, mut b) = if xi == 0 || yi == 0 {
                (1.0, 1.0, 1.0)
            } else if xi == w1 || yi == h1 {
                (0.0, 0.0, 0.0)
            } else if xi == 1 || yi == 1 {
                (0.75, 0.75, 0.75)
            } else if xi == w1 - 1 || yi == h1 - 1 {
                (0.4, 0.4, 0.4)
            } else {
                (lerp(0.0, 0.1, fy), lerp(0.05, 0.3, fy), lerp(0.4, 0.95, fy))
            };

            let inner = xi > 1 && yi > 1 && xi < w1 - 1 && yi < h1 - 1;
            if inner {
                if mask(TITLE, 2, xi - tx - 1, yi - ty - 1) {
                    r = 0.0;
                    g = 0.0;
                    b = 0.1;
                }
                if mask(TITLE, 2, xi - tx, yi - ty) {
                    let k = sat((yi - ty) as f32 / 14.0);
                    // chrome: hot yellow top, white band, orange bottom
                    if k < 0.45 {
                        r = 1.0;
                        g = lerp(0.85, 1.0, k / 0.45);
                        b = lerp(0.1, 1.0, k / 0.45);
                    } else {
                        let q = (k - 0.45) / 0.55;
                        r = 1.0;
                        g = lerp(0.75, 0.45, q);
                        b = lerp(0.2, 0.0, q);
                    }
                }
                if mask(SUB, 1, xi - sx, yi - sy) {
                    r = 0.8;
                    g = 1.0;
                    b = 1.0;
                }
                let d = abs(x as f32 - bn.sweep + y as f32 * 0.6);
                if d < 4.0 {
                    let k = (1.0 - d / 4.0) * 0.6;
                    r += k;
                    g += k;
                    b += k;
                }
            }

            if energy > 0.0 {
                let hz = hash32((y * W + x) as u32 ^ frame.wrapping_mul(0x7a3d_1e55));
                if ((hz & 0xffff) as f32) < energy * 4000.0 {
                    r = 1.0;
                    g = 0.0;
                    b = 1.0;
                }
            }
            put(&mut *buf, i4, x as u32, y as u32, r, g, b);
        }
    }
    buf.as_ptr()
}
