//! The spinning globe.
//!
//! Per pixel: ray–sphere hit (orthographic, so it is just the disc equation),
//! inverse-rotate the surface normal into planet space, sample 3-D fBm on the
//! unit sphere for continents (no UV seam, no pole pinch), Lambert + Blinn
//! specular on water, a lat/long graticule via atan2/asin, then ordered-dither
//! into the 216-colour cube with 1-bit alpha.
//!
//! When nobody is interacting, rotation snaps to 24 discrete frames per turn
//! at 12.5 fps – exactly what an animated GIF would do. Interact and it drops
//! the pretence and spins continuously at 60 fps.

use crate::math::{abs, asin, atan2, cos, damp, floor, fract, max, min, sat, sin, sqrt, TAU};
use crate::noise::fbm3;
use crate::palette::{clear, put};
use crate::rng::hash32;
use crate::Slot;

pub const S: usize = 88;
const R: f32 = 30.0;
const GIF_FRAMES: f32 = 24.0;
const TILT: f32 = 0.41; // 23.4°

static BUF: Slot<[u8; S * S * 4]> = Slot::new([0; S * S * 4]);

struct Globe {
    angle: f32,
    spin: f32,
    orbit: f32,
    trail: [(f32, f32, f32); 8],
}

static G: Slot<Globe> = Slot::new(Globe {
    angle: 0.0,
    spin: 0.7,
    orbit: 0.0,
    trail: [(0.0, 0.0, 0.0); 8],
});

pub fn settle() {
    G.get().spin = 0.7;
}

/// Add angular velocity (rad/s) – used for drag-to-spin.
#[no_mangle]
pub extern "C" fn globe_push(dv: f32) {
    let g = G.get();
    g.spin = min(max(g.spin + dv, -14.0), 14.0);
}

#[no_mangle]
pub extern "C" fn globe_size() -> u32 {
    S as u32
}

#[inline(always)]
fn pow32(x: f32) -> f32 {
    let x = x * x;
    let x = x * x;
    let x = x * x;
    let x = x * x;
    x * x
}

fn satellite(theta: f32) -> (f32, f32, f32) {
    let (x, z) = (cos(theta) * 1.42, sin(theta) * 1.42);
    // Inclined orbit: tip around X then around Z.
    let (ci, si) = (cos(0.5), sin(0.5));
    let (y1, z1) = (-z * si, z * ci);
    let (cz, sz) = (cos(0.3), sin(0.3));
    (x * cz - y1 * sz, x * sz + y1 * cz, z1)
}

#[no_mangle]
pub extern "C" fn globe_render(dt: f32, t: f32, energy: f32, hover: f32) -> *const u8 {
    let g = G.get();
    let buf = BUF.get();
    let base = 0.7 + hover * 2.2;
    g.spin += (base - g.spin) * damp(0.9, dt);
    g.angle = fract((g.angle + g.spin * dt) / TAU) * TAU;
    g.orbit = fract((g.orbit + (1.1 + hover * 1.5) * dt) / TAU) * TAU;

    let live = energy > 0.04 || hover > 0.01 || abs(g.spin - 0.7) > 0.05;
    let step = TAU / GIF_FRAMES;
    let (ang, orb) = if live {
        (g.angle, g.orbit)
    } else {
        (floor(g.angle / step) * step, floor(g.orbit / step) * step)
    };

    let (ca, sa) = (cos(ang), sin(ang));
    let (ct, st) = (cos(TILT), sin(TILT));
    let l = (-0.55f32, 0.55f32, 0.63f32);
    let ln = 1.0 / sqrt(l.0 * l.0 + l.1 * l.1 + l.2 * l.2);
    let l = (l.0 * ln, l.1 * ln, l.2 * ln);
    let hv = (l.0, l.1, l.2 + 1.0);
    let hn = 1.0 / sqrt(hv.0 * hv.0 + hv.1 * hv.1 + hv.2 * hv.2);
    let hv = (hv.0 * hn, hv.1 * hn, hv.2 * hn);
    let grid = 0.22 + 0.6 * max(hover, energy);
    let c = S as f32 * 0.5;
    let frame = (t * 30.0) as u32;

    for y in 0..S {
        for x in 0..S {
            let i4 = (y * S + x) * 4;
            let nx = (x as f32 + 0.5 - c) / R;
            let ny = (c - (y as f32 + 0.5)) / R;
            let r2 = nx * nx + ny * ny;
            if r2 > 1.0 {
                clear(&mut *buf, i4);
                continue;
            }
            let nz = sqrt(1.0 - r2);

            // view -> planet: undo axial tilt (X), then spin (Y)
            let py = ny * ct - nz * st;
            let pz = ny * st + nz * ct;
            let qx = nx * ca + pz * sa;
            let qz = -nx * sa + pz * ca;
            let qy = py;

            let hgt = fbm3(qx * 1.9 + 11.3, qy * 1.9 + 4.1, qz * 1.9 - 7.7, 4, 0x6EA);
            let land = hgt > 0.06;
            let ice = abs(qy) > 0.86 - hgt * 0.25;

            let (mut r, mut gg, mut b) = if ice {
                (0.92, 0.96, 1.0)
            } else if land {
                let k = sat((hgt - 0.06) * 4.0);
                (0.12 + 0.45 * k, 0.55 - 0.1 * k, 0.12 + 0.05 * k)
            } else {
                let k = sat(-hgt * 3.0);
                (0.04, 0.22 - 0.08 * k, 0.78 - 0.2 * k)
            };

            let diff = max(0.0, nx * l.0 + ny * l.1 + nz * l.2);
            let lit = 0.16 + 0.95 * diff;
            r *= lit;
            gg *= lit;
            b *= lit;
            if !land && !ice {
                let sp = pow32(max(0.0, nx * hv.0 + ny * hv.1 + nz * hv.2)) * 0.8;
                r += sp;
                gg += sp;
                b += sp;
            }

            // graticule every 30°
            let lon = atan2(qx, qz);
            let lat = asin(qy);
            let gl = fract(lon / (TAU / 12.0));
            let gt = fract(lat / (TAU / 12.0));
            let wl = 0.035 / max(cos(lat), 0.25);
            if min(gl, 1.0 - gl) < wl || min(gt, 1.0 - gt) < 0.045 {
                r = r + (0.3 - r) * grid;
                gg = gg + (1.0 - gg) * grid;
                b = b + (1.0 - b) * grid;
            }

            let rim = 1.0 - r2 * r2 * r2 * 0.45;
            r *= rim;
            gg *= rim;
            b *= rim;

            if energy > 0.0 {
                let hz = hash32((y * S + x) as u32 ^ frame.wrapping_mul(0x9e37_79b9));
                if ((hz & 0xffff) as f32) < energy * 3500.0 {
                    r = 1.0;
                    gg = 1.0;
                    b = 1.0;
                }
            }
            put(&mut *buf, i4, x as u32, y as u32, r, gg, b);
        }
    }

    // Satellite + 8-point fading trail, depth-tested against the disc.
    for k in 0..8 {
        g.trail[k] = satellite(orb - k as f32 * 0.07);
    }
    for k in (0..8).rev() {
        let (sx, sy, sz) = g.trail[k];
        if sz < 0.0 && sx * sx + sy * sy < 1.0 {
            continue;
        }
        let px = (c + sx * R) as i32;
        let py = (c - sy * R) as i32;
        let fade = 1.0 - k as f32 / 8.0;
        let size = if k == 0 { 2 } else { 1 };
        for oy in 0..size {
            for ox in 0..size {
                let (xx, yy) = (px + ox, py + oy);
                if xx < 0 || yy < 0 || xx >= S as i32 || yy >= S as i32 {
                    continue;
                }
                let i4 = (yy as usize * S + xx as usize) * 4;
                put(&mut *buf, i4, xx as u32, yy as u32, fade, fade, 0.4 * fade + 0.2);
            }
        }
    }
    buf.as_ptr()
}
