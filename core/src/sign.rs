//! UNDER CONSTRUCTION. Diagonal barrier tape top and bottom, two rotating
//! amber beacons, 5x7 lettering. Calm: tape advances in whole pixels at 8 fps
//! (GIF). Hover: tape accelerates on a spring and the beacons strobe.

use crate::font::{mask, width};
use crate::math::{damp, floor, sat, sin, PI};
use crate::palette::put;
use crate::rng::hash32;
use crate::Slot;

pub const W: usize = 136;
pub const H: usize = 32;
const TEXT: &[u8] = b"UNDER CONSTRUCTION";

static BUF: Slot<[u8; W * H * 4]> = Slot::new([0; W * H * 4]);

struct Sign {
    off: f32,
    speed: f32,
    clock: f32,
}

static SG: Slot<Sign> = Slot::new(Sign { off: 0.0, speed: 8.0, clock: 0.0 });

#[no_mangle]
pub extern "C" fn sign_w() -> u32 {
    W as u32
}

#[no_mangle]
pub extern "C" fn sign_h() -> u32 {
    H as u32
}

#[no_mangle]
pub extern "C" fn sign_render(dt: f32, t: f32, energy: f32, hover: f32) -> *const u8 {
    let s = SG.get();
    let buf = BUF.get();
    s.speed += (8.0 + hover * 48.0 - s.speed) * damp(5.0, dt);
    s.off += s.speed * dt;
    s.clock += dt * (1.0 + hover * 1.6);

    let live = hover > 0.01 || energy > 0.04;
    let off = if live { s.off } else { floor(s.off) };
    let clk = if live { s.clock } else { floor(s.clock * 8.0) / 8.0 };

    let tx0 = (W as i32 - width(TEXT.len(), 1)) / 2;
    let ty0 = 12;
    let lamp_a = sat(sin(clk * 6.0) * 1.4);
    let lamp_b = sat(sin(clk * 6.0 + PI) * 1.4);
    let frame = (t * 30.0) as u32;

    for y in 0..H {
        for x in 0..W {
            let i4 = (y * W + x) * 4;
            let (xi, yi) = (x as i32, y as i32);
            let (mut r, mut g, mut b);

            if xi == 0 || yi == 0 || xi == W as i32 - 1 || yi == H as i32 - 1 {
                r = 0.0;
                g = 0.0;
                b = 0.0;
            } else if !(8..24).contains(&yi) {
                let k = floor((x as f32 + y as f32 + off) / 5.0) as i32;
                if k & 1 == 0 {
                    r = 1.0;
                    g = 0.82;
                    b = 0.0;
                } else {
                    r = 0.06;
                    g = 0.06;
                    b = 0.06;
                }
            } else {
                r = 1.0;
                g = 0.9;
                b = 0.12;
                if mask(TEXT, 1, xi - tx0, yi - ty0) {
                    r = 0.0;
                    g = 0.0;
                    b = 0.0;
                }
                for (cx, on) in [(7.0f32, lamp_a), (128.0f32, lamp_b)] {
                    let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - 16.0);
                    let d2 = dx * dx + dy * dy;
                    if d2 < 4.2 * 4.2 {
                        if d2 > 3.3 * 3.3 {
                            r = 0.0;
                            g = 0.0;
                            b = 0.0;
                        } else {
                            let hl = if dx < -0.5 && dy < -0.5 && on > 0.5 { 0.35 } else { 0.0 };
                            r = 0.45 + 0.55 * on + hl;
                            g = 0.18 + 0.4 * on + hl;
                            b = 0.0 + hl;
                        }
                    }
                }
            }

            if energy > 0.0 {
                let hz = hash32((y * W + x) as u32 ^ frame.wrapping_mul(0x2c1b_3c6d));
                if ((hz & 0xffff) as f32) < energy * 4000.0 {
                    let v = (hz >> 24) as f32 / 255.0;
                    r = v;
                    g = v;
                    b = v;
                }
            }
            put(&mut *buf, i4, x as u32, y as u32, r, g, b);
        }
    }
    buf.as_ptr()
}
