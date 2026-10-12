//! Rainbow divider. Static when calm (it was a GIF, after all); on hover the
//! hue cycles and the bar ripples like a skipping rope.

use crate::math::{damp, floor, hsv, sin};
use crate::palette::{clear, put};
use crate::Slot;

pub const W: usize = 480;
pub const H: usize = 8;

static BUF: Slot<[u8; W * H * 4]> = Slot::new([0; W * H * 4]);

struct Rainbow {
    phase: f32,
    amp: f32,
}

static RB: Slot<Rainbow> = Slot::new(Rainbow { phase: 0.0, amp: 0.0 });

#[no_mangle]
pub extern "C" fn rainbow_w() -> u32 {
    W as u32
}

#[no_mangle]
pub extern "C" fn rainbow_h() -> u32 {
    H as u32
}

#[no_mangle]
pub extern "C" fn rainbow_render(dt: f32, t: f32, energy: f32, hover: f32) -> *const u8 {
    let rb = RB.get();
    let buf = BUF.get();
    let drive = if hover > energy { hover } else { energy };
    rb.amp += (drive * 2.4 - rb.amp) * damp(6.0, dt);
    rb.phase += dt * drive * 0.6;
    if rb.amp < 0.01 {
        rb.amp = 0.0;
    }
    for x in 0..W {
        let dy = sin(x as f32 * 0.045 - t * 9.0) * rb.amp;
        let hue = x as f32 / W as f32 * 0.85 + rb.phase;
        let (r, g, b) = hsv(hue, 1.0, 1.0);
        for y in 0..H {
            let i4 = (y * W + x) * 4;
            let yy = floor(y as f32 - dy + 0.5) as i32;
            if !(2..6).contains(&yy) {
                clear(&mut *buf, i4);
                continue;
            }
            let k = match yy {
                2 => 1.25,
                5 => 0.6,
                _ => 1.0,
            };
            put(&mut *buf, i4, x as u32, y as u32, r * k, g * k, b * k);
        }
    }
    buf.as_ptr()
}
