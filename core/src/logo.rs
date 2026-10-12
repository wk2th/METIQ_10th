//! The METIQ logo, drawn analytically.
//!
//! The triangle mark is not a bitmap: it is the difference of two polygons
//! traced from the master artwork (IoU 0.975 against it), evaluated per pixel
//! as an exact signed distance field (Quílez' winding-number sdPolygon). One
//! SDF gives us, for free:
//!
//!   * resolution-independent anti-aliasing  (coverage = ½ − d/px)
//!   * the soft halo of the original           (glow = e^(−d/r))
//!   * a 3-D turn about the apex axis          (pre-divide x by cos θ)
//!
//! The wordmark *is* baked (4-bit coverage, see tools/logo2rs.py) because it
//! is set in a typeface we cannot ship. Background is the original's slate →
//! sage gradient, ordered-dithered to the 216 cube like a 1996 GIF would be.
//!
//! Idle: static, plus an apex glint every few seconds at GIF frame rate.
//! Hover: the mark turns on its axis with a specular sweep; on leave it
//! completes the revolution and lands exactly face-on.

use crate::logo_data::{WM, WM_H, WM_W, WM_X, WM_Y};
use crate::math::{abs, ceil_div, clamp, damp, exp, floor, lerp, max, min, sat, sqrt, TAU};
use crate::palette::put;
use crate::rng::hash32;
use crate::Slot;

pub const W: usize = 240;
pub const H: usize = 124;

/// Source artwork px per surface px.
const S: f32 = 767.0 / 240.0;
/// Rotation axis (source x of the apex).
const AXIS: f32 = 414.4;

const TRI: [(f32, f32); 3] = [(414.4, 76.55), (726.8, 282.5), (103.2, 282.5)];

/// The dark zig-zag channel cut out of the triangle. Its first edge pokes past
/// the right flank on purpose – that is the notch in the right-hand side.
const CUT: [(f32, f32); 10] = [
    (415.5, 122.5),
    (490.0, 122.5),
    (560.0, 162.5),
    (475.5, 162.5),
    (593.1, 242.5),
    (234.25, 242.5),
    (300.25, 198.5),
    (472.5, 198.5),
    (418.5, 162.5),
    (356.0, 162.5),
];

static BUF: Slot<[u8; W * H * 4]> = Slot::new([0; W * H * 4]);

struct Logo {
    theta: f32,
    omega: f32,
    clock: f32,
}

static LG: Slot<Logo> = Slot::new(Logo { theta: 0.0, omega: 0.0, clock: 0.0 });

pub fn settle() {
    let l = LG.get();
    l.theta = 0.0;
    l.omega = 0.0;
}

/// Exact signed distance to a simple polygon (negative inside).
fn sd_poly<const N: usize>(px: f32, py: f32, v: &[(f32, f32); N]) -> f32 {
    let (dx, dy) = (px - v[0].0, py - v[0].1);
    let mut d = dx * dx + dy * dy;
    let mut s = 1.0;
    let mut j = N - 1;
    for i in 0..N {
        let (ex, ey) = (v[j].0 - v[i].0, v[j].1 - v[i].1);
        let (wx, wy) = (px - v[i].0, py - v[i].1);
        let h = clamp((wx * ex + wy * ey) / (ex * ex + ey * ey), 0.0, 1.0);
        let (bx, by) = (wx - ex * h, wy - ey * h);
        d = min(d, bx * bx + by * by);
        let c0 = py >= v[i].1;
        let c1 = py < v[j].1;
        let c2 = ex * wy > ey * wx;
        if (c0 && c1 && c2) || (!c0 && !c1 && !c2) {
            s = -s;
        }
        j = i;
    }
    s * sqrt(d)
}

#[inline]
fn mark(px: f32, py: f32) -> f32 {
    max(sd_poly(px, py, &TRI), -sd_poly(px, py, &CUT))
}

#[inline]
fn wordmark(x: i32, y: i32) -> f32 {
    let (lx, ly) = (x - WM_X, y - WM_Y);
    if lx < 0 || ly < 0 || lx >= WM_W || ly >= WM_H {
        return 0.0;
    }
    let i = (ly * WM_W + lx) as usize;
    let b = WM[i >> 1];
    let q = if i & 1 == 0 { b >> 4 } else { b & 15 };
    q as f32 * (1.0 / 15.0)
}

#[no_mangle]
pub extern "C" fn logo_w() -> u32 {
    W as u32
}

#[no_mangle]
pub extern "C" fn logo_h() -> u32 {
    H as u32
}

/// Click: throw the mark into a fast spin.
#[no_mangle]
pub extern "C" fn logo_kick(dv: f32) {
    LG.get().omega += dv;
}

#[no_mangle]
pub extern "C" fn logo_render(dt: f32, t: f32, energy: f32, hover: f32) -> *const u8 {
    let l = LG.get();
    let buf = BUF.get();

    // Spin while hovered; once released, coast to the next whole turn and land.
    if hover > 0.05 {
        l.omega += (2.6 - l.omega) * damp(3.0, dt);
    } else if l.omega != 0.0 || l.theta != 0.0 {
        l.omega = max(l.omega * (1.0 - damp(1.2, dt)), 1.4);
    }
    let goal = ceil_div(l.theta, TAU) * TAU;
    let next = l.theta + l.omega * dt;
    if hover <= 0.05 && l.theta > 0.0 && next >= goal {
        l.theta = 0.0;
        l.omega = 0.0;
    } else {
        l.theta = next;
    }
    l.clock += dt;

    let (c, sn) = (crate::math::cos(l.theta), crate::math::sin(l.theta));
    let ac = max(abs(c), 1e-3);
    let face = c >= 0.0;
    // Pixel footprint in source units, widened as the card turns edge-on.
    let aa = S / sqrt(ac);
    let spinning = l.theta != 0.0;

    // Apex glint: 0.6 s every 4.5 s, quantised to 12.5 fps like a GIF.
    let gclk = floor(l.clock * 12.5) / 12.5;
    let gphase = gclk - floor(gclk / 4.5) * 4.5;
    let glint = if gphase < 0.6 { sat(1.0 - abs(gphase - 0.3) / 0.3) } else { 0.0 };
    let (gx, gy) = (AXIS / S, TRI[0].1 / S + 1.0);

    // Specular band position (source units along x+y), only while turning.
    let sweep = if spinning { lerp(-200.0, 1100.0, sat((sn + 1.0) * 0.5)) } else { -1e4 };
    let frame = (t * 30.0) as u32;

    for y in 0..H {
        let fy = y as f32 / (H - 1) as f32;
        for x in 0..W {
            let fx = x as f32 / (W - 1) as f32;

            // slate (39,66,84) → (70,98,106) → sage, warmer to the right
            let (mut r, mut g, mut b) = if fy < 0.5 {
                let k = fy * 2.0;
                (lerp(0.153, 0.275, k), lerp(0.259, 0.384, k), lerp(0.329, 0.416, k))
            } else {
                let k = (fy - 0.5) * 2.0;
                let br = (lerp(0.514, 0.714, fx), lerp(0.596, 0.725, fx), lerp(0.584, 0.663, fx));
                (lerp(0.275, br.0, k), lerp(0.384, br.1, k), lerp(0.416, br.2, k))
            };

            let sx = (x as f32 + 0.5) * S;
            let sy = (y as f32 + 0.5) * S;
            if sy < 300.0 {
                let ux = AXIS + (sx - AXIS) / ac * if face { 1.0 } else { -1.0 };
                let d = mark(ux, sy);
                let glow = exp(-max(d, 0.0) * (1.0 / 13.0)) * 0.55 * ac;
                let cov = sat(0.5 - d / aa);
                r += (1.0 - r) * glow;
                g += (1.0 - g) * glow;
                b += (1.0 - b) * glow;
                if cov > 0.0 {
                    let shade = if face { 0.82 + 0.18 * ac } else { 0.55 + 0.2 * ac };
                    let spec = sat(1.0 - abs(ux + sy - sweep) / 90.0) * 0.6;
                    let (mr, mg, mb) = (shade + spec, shade + spec, shade * 0.98 + spec);
                    r = lerp(r, mr, cov);
                    g = lerp(g, mg, cov);
                    b = lerp(b, mb, cov);
                }
            }

            let wm = wordmark(x as i32, y as i32);
            if wm > 0.0 {
                r = lerp(r, 1.0, wm);
                g = lerp(g, 1.0, wm);
                b = lerp(b, 1.0, wm);
            }

            if glint > 0.0 && !spinning {
                let (dx, dy) = (abs(x as f32 + 0.5 - gx), abs(y as f32 + 0.5 - gy));
                let arm = 9.0 * glint;
                let k = if dy < 0.6 && dx < arm {
                    1.0 - dx / arm
                } else if dx < 0.6 && dy < arm {
                    1.0 - dy / arm
                } else if dx < 1.5 && dy < 1.5 {
                    0.6
                } else {
                    0.0
                } * glint;
                r += k;
                g += k;
                b += k;
            }

            if energy > 0.0 {
                let hz = hash32((y * W + x) as u32 ^ frame.wrapping_mul(0x632b_e5ab));
                if ((hz & 0xffff) as f32) < energy * 3000.0 {
                    let v = (hz >> 24) as f32 / 255.0;
                    r = v;
                    g = v;
                    b = v;
                }
            }
            put(&mut *buf, (y * W + x) * 4, x as u32, y as u32, r, g, b);
        }
    }
    buf.as_ptr()
}
