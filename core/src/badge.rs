//! 10th ANNIVERSARY medal – the kind of gold seal a 1996 home page would
//! proudly paste next to its logo.
//!
//! Layers, back to front, all evaluated per pixel (no sprites):
//!
//!   ribbon tails   two capsules with V-notched ends, swaying on hover
//!   seal           24-point starburst rim r(θ) = R − 4·|saw(12θ/π)| with
//!                  radial sunburst rays, a bevelled inner ring and a
//!                  top-left-lit gold disc
//!   "10" + "TH"    5x7 font at 3x / 1x, debossed (shadow + highlight)
//!   banner         notched red ribbon across the seal, "ANNIVERSARY"
//!   glints         4-point sparkles hopping round the rim
//!
//! Idle it behaves like a GIF: a sparkle every ~2.5 s, stepped at 12.5 fps.
//! Hover: the sunburst turns, a specular band sweeps the metal, the tails
//! swing and the sparkles come thick and fast.

use crate::font::{mask, width};
use crate::math::{abs, atan2, cos, damp, floor, fract, lerp, max, min, sat, sin, sqrt, TAU};
use crate::palette::{clear, put};
use crate::rng::{h01, hash32};
use crate::Slot;

pub const W: usize = 96;
pub const H: usize = 104;
const CX: f32 = 48.0;
const CY: f32 = 42.0;
const R: f32 = 37.0;
const RING: f32 = 28.5;

const GOLD: (f32, f32, f32) = (0.86, 0.64, 0.12);
const GOLD_HI: (f32, f32, f32) = (1.0, 0.94, 0.55);
const GOLD_LO: (f32, f32, f32) = (0.50, 0.33, 0.04);
const RED: (f32, f32, f32) = (0.80, 0.06, 0.08);
const RED_LO: (f32, f32, f32) = (0.48, 0.02, 0.04);

static BUF: Slot<[u8; W * H * 4]> = Slot::new([0; W * H * 4]);

struct Badge {
    spin: f32,
    clock: f32,
    sweep: f32,
}

static BG: Slot<Badge> = Slot::new(Badge { spin: 0.0, clock: 0.0, sweep: -60.0 });

#[no_mangle]
pub extern "C" fn badge_w() -> u32 {
    W as u32
}

#[no_mangle]
pub extern "C" fn badge_h() -> u32 {
    H as u32
}

#[inline]
fn mix(a: (f32, f32, f32), b: (f32, f32, f32), t: f32) -> (f32, f32, f32) {
    (lerp(a.0, b.0, t), lerp(a.1, b.1, t), lerp(a.2, b.2, t))
}

#[inline]
fn scale(c: (f32, f32, f32), k: f32) -> (f32, f32, f32) {
    (c.0 * k, c.1 * k, c.2 * k)
}

/// Ribbon tail from (ax, ay) along unit (ux, uy), half-width 5, V-notched end.
/// Returns the signed across-coordinate if inside.
fn tail(px: f32, py: f32, ax: f32, ay: f32, ux: f32, uy: f32, len: f32) -> Option<f32> {
    let (wx, wy) = (px - ax, py - ay);
    let along = wx * ux + wy * uy;
    let across = wx * -uy + wy * ux;
    if along < 0.0 || abs(across) > 5.0 {
        return None;
    }
    if along > len - 6.0 + abs(across) {
        return None;
    }
    Some(across)
}

/// Gold metal at a seal point: lit from the upper left, plus the sweep band.
fn metal(dx: f32, dy: f32, sweep: f32) -> (f32, f32, f32) {
    let k = sat(0.5 + (-dx - dy) / (R * 2.2));
    let mut c = if k > 0.5 { mix(GOLD, GOLD_HI, (k - 0.5) * 2.0) } else { mix(GOLD_LO, GOLD, k * 2.0) };
    let band = sat(1.0 - abs(dx + dy * 0.5 - sweep) / 6.0) * 0.8;
    c.0 += band;
    c.1 += band;
    c.2 += band * 0.7;
    c
}

#[no_mangle]
pub extern "C" fn badge_render(dt: f32, t: f32, energy: f32, hover: f32) -> *const u8 {
    let b = BG.get();
    let buf = BUF.get();

    b.spin = fract(b.spin + dt * hover * 0.35);
    b.clock += dt;
    if hover > 0.01 {
        b.sweep += dt * 70.0;
        if b.sweep > 70.0 {
            b.sweep = -70.0;
        }
    } else {
        b.sweep += (-70.0 - b.sweep) * damp(4.0, dt);
    }

    // GIF-style clock for idle sparkles; continuous when hovered
    let clk = if hover > 0.01 { b.clock } else { floor(b.clock * 12.5) / 12.5 };
    let period = lerp(2.5, 0.35, hover);
    let slot = floor(clk / period);
    let sp_phase = (clk - slot * period) / min(period, 0.6);
    let hs = hash32(slot as u32 ^ 0xB4D6);
    let sa = h01(hs) * TAU;
    let (sx, sy) = (CX + (R - 2.0) * cos(sa), CY + (R - 2.0) * sin(sa));
    let spark = if sp_phase < 1.0 { sat(1.0 - abs(sp_phase - 0.5) * 2.0) } else { 0.0 };

    let sway = sin(t * 5.0) * 0.12 * hover;
    // tails hang ±0.36 rad off vertical: direction (sin a, cos a)
    let (l_ux, l_uy) = (sin(-0.36 + sway), cos(-0.36 + sway));
    let (r_ux, r_uy) = (sin(0.36 + sway), cos(0.36 + sway));

    let ten_w = width(2, 3); // "10" at 3x
    let tx0 = (CX as i32) - ten_w / 2 - 6;
    let ty0 = 22;
    let th_x0 = tx0 + ten_w + 2;
    let ann = b"ANNIVERSARY";
    let ax0 = (CX as i32) - width(ann.len(), 1) / 2;
    let frame = (t * 30.0) as u32;

    for y in 0..H {
        for x in 0..W {
            let i4 = (y * W + x) * 4;
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let (dx, dy) = (px - CX, py - CY);
            let r = sqrt(dx * dx + dy * dy);
            let mut col: Option<(f32, f32, f32)> = None;

            // ribbon tails (behind)
            for (ux, uy, ox) in [(l_ux, l_uy, -6.0f32), (r_ux, r_uy, 6.0f32)] {
                if let Some(a) = tail(px, py, CX + ox, CY + 22.0, ux, uy, 40.0) {
                    let c = if a < -3.5 { mix(RED, (1.0, 0.5, 0.5), 0.5) } else if a > 3.5 { RED_LO } else { RED };
                    col = Some(c);
                }
            }

            // seal
            let ang = atan2(dy, dx);
            let saw = fract(ang / TAU * 24.0 + b.spin);
            let edge = R - 4.0 * abs(saw - 0.5) * 2.0;
            if r < edge {
                let mut c = metal(dx, dy, b.sweep);
                if r > RING + 1.5 {
                    // sunburst rays
                    let ray = fract(ang / TAU * 48.0 - b.spin * 2.0);
                    c = scale(c, if ray < 0.5 { 1.08 } else { 0.86 });
                } else if r > RING - 0.5 {
                    c = scale(GOLD_LO, 0.9); // groove
                } else if r > RING - 1.6 {
                    c = mix(c, GOLD_HI, 0.6); // bevel highlight
                }
                if r > edge - 1.0 {
                    c = scale(c, 0.7); // rim outline
                }

                let (xi, yi) = (x as i32, y as i32);
                // "10" debossed: dark fill, light lower-right lip
                // the font's 0 is slashed (it's a counter font); a medal wants a round O
                let ten = |xx: i32, yy: i32| mask(b"1O", 3, xx - tx0, yy - ty0);
                let th = |xx: i32, yy: i32| mask(b"TH", 1, xx - th_x0, yy - ty0);
                if ten(xi, yi) || th(xi, yi) {
                    c = (0.36, 0.17, 0.0);
                } else if ten(xi - 1, yi - 1) || th(xi - 1, yi - 1) {
                    c = GOLD_HI;
                }
                // three little stars above the numerals
                for k in 0..3 {
                    let (cx, cy) = (CX - 9.0 + k as f32 * 9.0, 15.5);
                    let (ex, ey) = (abs(px - cx), abs(py - cy));
                    if (ex < 0.6 && ey < 2.6) || (ey < 0.6 && ex < 2.6) || (ex < 1.1 && ey < 1.1) {
                        c = (0.55, 0.05, 0.05);
                    }
                }
                col = Some(c);
            }

            // banner across the seal, notched ends
            let (xi, yi) = (x as i32, y as i32);
            if (48..60).contains(&yi) && (1..95).contains(&xi) {
                let v = abs(py - 54.0);
                let left_notch = px - 1.0 < 6.0 - v;
                let right_notch = 95.0 - px < 6.0 - v;
                if !left_notch && !right_notch {
                    let end = !(11..85).contains(&xi);
                    let mut c = if end { RED_LO } else { RED };
                    if yi == 48 && !end {
                        c = mix(RED, (1.0, 0.6, 0.6), 0.6);
                    } else if yi == 59 {
                        c = scale(RED_LO, 0.7);
                    }
                    if mask(ann, 1, xi - ax0, yi - 50) {
                        c = (1.0, 0.92, 0.35);
                    } else if mask(ann, 1, xi - ax0 - 1, yi - 51) {
                        c = (0.25, 0.0, 0.0);
                    }
                    col = Some(c);
                }
            }

            // sparkle
            if spark > 0.0 {
                let (ex, ey) = (abs(px - sx), abs(py - sy));
                let arm = 7.0 * spark;
                let k = if ey < 0.6 && ex < arm {
                    1.0 - ex / arm
                } else if ex < 0.6 && ey < arm {
                    1.0 - ey / arm
                } else if ex < 1.2 && ey < 1.2 {
                    1.0
                } else {
                    0.0
                };
                if k > 0.25 {
                    col = Some((1.0, 1.0, max(0.7, k)));
                }
            }

            match col {
                Some((mut r, mut g, mut bb)) => {
                    if energy > 0.0 {
                        let hz = hash32((y * W + x) as u32 ^ frame.wrapping_mul(0x51ed_270b));
                        if ((hz & 0xffff) as f32) < energy * 3000.0 {
                            let v = (hz >> 24) as f32 / 255.0;
                            r = v;
                            g = v;
                            bb = v;
                        }
                    }
                    put(&mut *buf, i4, x as u32, y as u32, r, g, bb);
                }
                None => clear(&mut *buf, i4),
            }
        }
    }
    buf.as_ptr()
}
