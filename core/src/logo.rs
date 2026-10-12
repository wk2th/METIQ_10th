//! The METIQ logo, drawn analytically – shape only, on a transparent ground.
//!
//! The triangle mark is not a bitmap: it is the difference of two polygons
//! traced from the master artwork (IoU 0.975 against it), evaluated per pixel
//! as an exact signed distance field (Quílez' winding-number sdPolygon). From
//! that one d(p) we get:
//!
//!   * coverage, turned into 1-bit GIF alpha by ordered dithering
//!   * a chiselled bevel: the SDF gradient is the outward normal, lit from
//!     the upper left like every 1996 "Bevel and Emboss"
//!   * a 3-D turn about the apex axis (pre-divide x by cos θ)
//!   * the drop shadow (the same field sampled 2px up-left)
//!
//! The wordmark is baked coverage (tools/logo2rs.py), since its typeface
//! cannot ship. Fill is the brand slate, dithered into the 216 cube.
//!
//! Idle: static, plus an apex glint every few seconds at GIF frame rate.
//! Hover: the mark turns on its axis with a specular sweep; on leave it
//! completes the revolution and lands exactly face-on.

use crate::logo_data::{AXIS, H, S, SHADOW, SX0, SY0, W, WM, WM_H, WM_W, WM_X, WM_Y};
use crate::math::{abs, ceil_div, clamp, cos, damp, floor, lerp, max, min, sat, sin, sqrt, TAU};
use crate::palette::{bayer, clear, put};
use crate::rng::hash32;
use crate::Slot;

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

/// Light from the upper left, image coordinates (y down), unit length.
const LIGHT: (f32, f32) = (-0.6, -0.8);
const BEVEL: f32 = 2.4 * S;

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

/// Turn state shared by every sample in a frame.
#[derive(Clone, Copy)]
struct View {
    ac: f32,
    sign: f32,
    aa: f32,
}

impl View {
    /// Surface pixel → (distance, source-space x, source-space y) of the mark.
    #[inline]
    fn sample(&self, x: i32, y: i32) -> (f32, f32, f32) {
        let sx = (x as f32 + 0.5) * S + SX0;
        let sy = (y as f32 + 0.5) * S + SY0;
        let ux = AXIS + (sx - AXIS) / self.ac * self.sign;
        (mark(ux, sy), ux, sy)
    }

    #[inline]
    fn cov(&self, d: f32) -> f32 {
        sat(0.5 - d / self.aa)
    }
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

    let (c, sn) = (cos(l.theta), sin(l.theta));
    let ac = max(abs(c), 1e-3);
    let face = c >= 0.0;
    // Pixel footprint in source units, widened as the card turns edge-on.
    let v = View { ac, sign: if face { 1.0 } else { -1.0 }, aa: S / sqrt(ac) };
    let spinning = l.theta != 0.0;

    // Apex glint: 0.6 s every 4.5 s, quantised to 12.5 fps like a GIF.
    let gclk = floor(l.clock * 12.5) / 12.5;
    let gphase = gclk - floor(gclk / 4.5) * 4.5;
    let glint = if gphase < 0.6 { sat(1.0 - abs(gphase - 0.3) / 0.3) } else { 0.0 };
    let gx = (AXIS - SX0) / S;
    let gy = (TRI[0].1 - SY0) / S + 1.5;

    let sweep = if spinning { lerp(-200.0, 1100.0, sat((sn + 1.0) * 0.5)) } else { -1e4 };
    let frame = (t * 30.0) as u32;
    let (y_top, y_bot) = (TRI[0].1, TRI[1].1);
    let eps = S * 0.5;

    for y in 0..H as i32 {
        for x in 0..W as i32 {
            let i4 = (y as usize * W + x as usize) * 4;
            let th = bayer(x as u32, y as u32);

            let (d, ux, sy) = v.sample(x, y);
            let wm = wordmark(x, y);
            let mut col: Option<(f32, f32, f32)> = None;

            if v.cov(d) > th {
                // brand slate, lighter at the apex, deeper at the base
                let k = sat((sy - y_top) / (y_bot - y_top));
                let (mut r, mut g, mut b) =
                    (lerp(0.36, 0.13, k), lerp(0.52, 0.24, k), lerp(0.60, 0.31, k));
                if !face {
                    r *= 0.6;
                    g *= 0.6;
                    b *= 0.6;
                }
                // bevel: outward normal from the SDF gradient
                if d > -BEVEL {
                    let nx = mark(ux + eps, sy) - mark(ux - eps, sy);
                    let ny = mark(ux, sy + eps) - mark(ux, sy - eps);
                    let inv = 1.0 / max(sqrt(nx * nx + ny * ny), 1e-4);
                    let lit = (nx * v.sign * LIGHT.0 + ny * LIGHT.1) * inv;
                    let k = (1.0 - (-d) / BEVEL) * lit * 0.5;
                    r += k;
                    g += k;
                    b += k;
                }
                let spec = sat(1.0 - abs(ux + sy - sweep) / 90.0) * 0.7;
                col = Some((r + spec, g + spec, b + spec));
            } else if wm > th {
                let k = sat((y - WM_Y) as f32 / WM_H as f32);
                col = Some((lerp(0.24, 0.12, k), lerp(0.36, 0.22, k), lerp(0.44, 0.29, k)));
            } else {
                // drop shadow: same field, sampled up-left
                let (ds, _, _) = v.sample(x - SHADOW, y - SHADOW);
                if v.cov(ds) > th || wordmark(x - SHADOW, y - SHADOW) > th {
                    col = Some((0.42, 0.42, 0.46));
                }
            }

            if glint > 0.0 && !spinning {
                let (dx, dy) = (abs(x as f32 + 0.5 - gx), abs(y as f32 + 0.5 - gy));
                let arm = 9.0 * glint;
                let k = if dy < 0.6 && dx < arm {
                    1.0 - dx / arm
                } else if dx < 0.6 && dy < arm {
                    1.0 - dy / arm
                } else if dx < 1.5 && dy < 1.5 {
                    1.0
                } else {
                    0.0
                } * glint;
                if k > 0.3 {
                    let (r, g, b) = col.unwrap_or((1.0, 1.0, 1.0));
                    col = Some((r + k, g + k, b + k * 0.8));
                }
            }

            match col {
                Some((mut r, mut g, mut b)) => {
                    if energy > 0.0 {
                        let hz = hash32((y as u32 * W as u32 + x as u32) ^ frame.wrapping_mul(0x632b_e5ab));
                        if ((hz & 0xffff) as f32) < energy * 3000.0 {
                            let n = (hz >> 24) as f32 / 255.0;
                            r = n;
                            g = n;
                            b = n;
                        }
                    }
                    put(&mut *buf, i4, x as u32, y as u32, r, g, b);
                }
                None => clear(&mut *buf, i4),
            }
        }
    }
    buf.as_ptr()
}
