//! Shatter: every few seconds a table or picture on the page cracks, arcs
//! with blue-white electricity, breaks into shards and falls away – then
//! re-forms in place with a soft glow while its text decodes.
//!
//! Geometry (once per event, in the object's local px):
//!
//! * Seeds: ~60% clustered round a random impact point (r ∝ u^1.7), the rest
//!   uniform, so shards are small and many where it was hit, large far away.
//! * Cells: Voronoi by brute-force half-plane clipping – start from the
//!   bounding rectangle and Sutherland–Hodgman it against the perpendicular
//!   bisector of every other seed. O(n²·v), n ≤ 24: trivial.
//! * Cracks: every interior cell edge, kept once (owner = lower seed index of
//!   the pair, found by probing just across the edge), stamped with a delay
//!   proportional to its distance from the impact.
//!
//! Per frame:
//!
//! ```text
//!  charge  τ < 0.5     cracks light up outward from the impact as jagged
//!                      bolts (midpoint-displaced, re-rolled at 30 Hz); the
//!                      object flickers; its text rots to mojibake
//!  break   τ ≥ 0.5     shards blow out radially from the impact, hop, spin
//!                      about their own centroids and fall (g = 1250 px/s²)
//!  seep    τ ≥ 1.65    the original wells back up: blur 6→0, scale .92→1,
//!                      a soft white glow that rises and fades, text decodes
//!                      Greek → maths → true
//! ```
//!
//! JS owns the DOM: it clones the object once per shard, clips each clone to
//! its polygon and applies the transforms written here.

use crate::field;
use crate::math::{abs, cos, floor, max, min, sat, sin, sqrt, PI, TAU};
use crate::rng::{h01, hash32, hsym, Pcg};
use crate::Slot;

pub const MAXSH: usize = 24;
const MAXV: usize = 16;
const MAXE: usize = 96;
const ARCS: usize = 4;
const BOLT_PTS: usize = 9;
const POLY: usize = 3 + MAXV * 2; // nv, cx, cy, then xy pairs
const BOLT: usize = 1 + BOLT_PTS * 2; // alpha, then xy pairs

pub const CHARGE: f32 = 0.5;
const FALL: f32 = 1.25;
const SEEP_AT: f32 = CHARGE + 1.15;
const SEEP: f32 = 0.8;
const GRAVITY: f32 = 1250.0;

type V2 = (f32, f32);

struct Shatter {
    w: f32,
    h: f32,
    impact: V2,
    seeds: [V2; MAXSH],
    n: usize,
    poly: [f32; MAXSH * POLY],
    out: [f32; MAXSH * 4],
    edges: [(V2, V2, f32); MAXE],
    ne: usize,
    bolts: [f32; 1 + (MAXE + ARCS) * BOLT],
    fx: [f32; 4],
    g: (usize, usize),
    seed: u32,
    t0: f32,
    active: bool,
}

static RNG: Slot<Pcg> = Slot::new(Pcg::fixed());

static SH: Slot<Shatter> = Slot::new(Shatter {
    w: 0.0,
    h: 0.0,
    impact: (0.0, 0.0),
    seeds: [(0.0, 0.0); MAXSH],
    n: 0,
    poly: [0.0; MAXSH * POLY],
    out: [0.0; MAXSH * 4],
    edges: [((0.0, 0.0), (0.0, 0.0), 0.0); MAXE],
    ne: 0,
    bolts: [0.0; 1 + (MAXE + ARCS) * BOLT],
    fx: [0.0; 4],
    g: (0, 0),
    seed: 0,
    t0: 0.0,
    active: false,
});

#[no_mangle]
pub extern "C" fn shatter_poly() -> *const f32 {
    SH.get().poly.as_ptr()
}

#[no_mangle]
pub extern "C" fn shatter_out() -> *const f32 {
    SH.get().out.as_ptr()
}

#[no_mangle]
pub extern "C" fn shatter_bolts() -> *const f32 {
    SH.get().bolts.as_ptr()
}

/// [flash, glow, seep k, blur px]
#[no_mangle]
pub extern "C" fn shatter_fx() -> *const f32 {
    SH.get().fx.as_ptr()
}

/// Sutherland–Hodgman: keep the part of `poly` where (p − m)·d ≤ 0.
fn clip(poly: &mut [V2; 32], nv: &mut usize, m: V2, d: V2) {
    let mut out = [(0.0f32, 0.0f32); 32];
    let mut no = 0usize;
    let side = |p: V2| (p.0 - m.0) * d.0 + (p.1 - m.1) * d.1;
    for k in 0..*nv {
        let a = poly[k];
        let b = poly[(k + 1) % *nv];
        let (sa, sb) = (side(a), side(b));
        if sa <= 0.0 && no < 32 {
            out[no] = a;
            no += 1;
        }
        if (sa <= 0.0) != (sb <= 0.0) && no < 32 {
            let t = sa / (sa - sb);
            out[no] = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            no += 1;
        }
    }
    *poly = out;
    *nv = no;
}

fn nearest_other(s: &Shatter, p: V2, skip: usize) -> usize {
    let mut best = (f32::MAX, skip);
    for j in 0..s.n {
        if j == skip {
            continue;
        }
        let (dx, dy) = (p.0 - s.seeds[j].0, p.1 - s.seeds[j].1);
        let d = dx * dx + dy * dy;
        if d < best.0 {
            best = (d, j);
        }
    }
    best.1
}

/// Fracture a w×h object hit at (ix, iy). Glyphs [g0, g1) belong to it.
/// Returns the shard count.
#[no_mangle]
pub extern "C" fn shatter_plan(w: f32, h: f32, ix: f32, iy: f32, seed: u32, g0: u32, g1: u32, t: f32) -> u32 {
    let s = SH.get();
    shatter_cancel();
    let rng = RNG.get();
    rng.seed(seed as u64, 0x5A7);
    s.seed = seed;
    s.w = w;
    s.h = h;
    s.impact = (ix, iy);
    s.g = (g0 as usize, min(g1 as f32, field::CAP as f32) as usize);
    s.t0 = t;
    s.n = (min(max(w * h / 2200.0, 7.0), MAXSH as f32)) as usize;

    let diag = sqrt(w * w + h * h);
    s.seeds[0] = (ix, iy);
    for k in 1..s.n {
        s.seeds[k] = if (k as f32) < s.n as f32 * 0.6 {
            let r = rng.f01();
            let r = r * sqrt(r) * 0.45 * diag;
            let a = rng.f01() * TAU;
            (min(max(ix + r * cos(a), 1.0), w - 1.0), min(max(iy + r * sin(a), 1.0), h - 1.0))
        } else {
            (rng.f01() * w, rng.f01() * h)
        };
    }

    s.ne = 0;
    for i in 0..s.n {
        let mut poly = [(0.0f32, 0.0f32); 32];
        poly[..4].copy_from_slice(&[(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]);
        let mut nv = 4usize;
        let si = s.seeds[i];
        for j in 0..s.n {
            if j == i {
                continue;
            }
            let sj = s.seeds[j];
            let m = ((si.0 + sj.0) * 0.5, (si.1 + sj.1) * 0.5);
            clip(&mut poly, &mut nv, m, (sj.0 - si.0, sj.1 - si.1));
        }
        let nv = min(nv as f32, MAXV as f32) as usize;

        // area-weighted centroid
        let (mut a2, mut cx, mut cy) = (0.0, 0.0, 0.0);
        for k in 0..nv {
            let (p, q) = (poly[k], poly[(k + 1) % nv]);
            let cr = p.0 * q.1 - q.0 * p.1;
            a2 += cr;
            cx += (p.0 + q.0) * cr;
            cy += (p.1 + q.1) * cr;
        }
        let (cx, cy) = if abs(a2) > 1e-3 { (cx / (3.0 * a2), cy / (3.0 * a2)) } else { si };

        let b = i * POLY;
        s.poly[b] = nv as f32;
        s.poly[b + 1] = cx;
        s.poly[b + 2] = cy;
        for k in 0..nv {
            s.poly[b + 3 + k * 2] = poly[k].0;
            s.poly[b + 4 + k * 2] = poly[k].1;
        }

        // interior edges become cracks (kept once, by the lower index)
        for k in 0..nv {
            let (p, q) = (poly[k], poly[(k + 1) % nv]);
            let on_border = |a: V2, b: V2| {
                (abs(a.0) < 0.5 && abs(b.0) < 0.5)
                    || (abs(a.1) < 0.5 && abs(b.1) < 0.5)
                    || (abs(a.0 - w) < 0.5 && abs(b.0 - w) < 0.5)
                    || (abs(a.1 - h) < 0.5 && abs(b.1 - h) < 0.5)
            };
            if on_border(p, q) {
                continue;
            }
            let m = ((p.0 + q.0) * 0.5, (p.1 + q.1) * 0.5);
            if nearest_other(s, m, i) < i || s.ne >= MAXE {
                continue;
            }
            let (dx, dy) = (m.0 - ix, m.1 - iy);
            s.edges[s.ne] = (p, q, sqrt(dx * dx + dy * dy) / max(diag, 1.0) * 0.38);
            s.ne += 1;
        }
    }
    s.active = true;
    s.n as u32
}

#[no_mangle]
pub extern "C" fn shatter_cancel() {
    let s = SH.get();
    if s.active {
        let ext = field::ext();
        for e in ext.iter_mut().take(s.g.1).skip(s.g.0) {
            *e = 0.0;
        }
    }
    s.active = false;
    s.bolts[0] = 0.0;
    s.fx = [0.0; 4];
}

fn bolt(s: &mut Shatter, slot: usize, p: V2, q: V2, alpha: f32, salt: u32) {
    let b = 1 + slot * BOLT;
    s.bolts[b] = alpha;
    let (dx, dy) = (q.0 - p.0, q.1 - p.1);
    let len = sqrt(dx * dx + dy * dy);
    let (nx, ny) = if len > 1e-3 { (-dy / len, dx / len) } else { (0.0, 0.0) };
    let amp = min(7.0, len * 0.2);
    for k in 0..BOLT_PTS {
        let f = k as f32 / (BOLT_PTS - 1) as f32;
        let j = if k == 0 || k == BOLT_PTS - 1 { 0.0 } else { hsym(hash32(salt ^ (k as u32 * 0x9e37))) * amp * sin(PI * f) };
        s.bolts[b + 1 + k * 2] = p.0 + dx * f + nx * j;
        s.bolts[b + 2 + k * 2] = p.1 + dy * f + ny * j;
    }
}

/// Advance. Returns 0 done, 1 charging, 2 broken, 3 re-forming.
#[no_mangle]
pub extern "C" fn shatter_step(t: f32) -> u32 {
    let s = SH.get();
    if !s.active {
        return 0;
    }
    let tau = t - s.t0;
    let frame = floor(tau * 30.0) as u32;
    let diag = sqrt(s.w * s.w + s.h * s.h);

    // --- bolts -----------------------------------------------------------
    let mut nb = 0usize;
    let live_bolts = tau < CHARGE + 0.22;
    if live_bolts {
        for e in 0..s.ne {
            let (p, q, delay) = s.edges[e];
            if tau < delay {
                continue;
            }
            let hf = hash32(s.seed ^ (e as u32 * 0x2c1b_3c6d) ^ frame.wrapping_mul(0x51ed_270b));
            let fade = if tau > CHARGE { 1.0 - (tau - CHARGE) / 0.22 } else { 1.0 };
            let a = (0.45 + 0.55 * h01(hf)) * fade;
            bolt(s, nb, p, q, a, hf);
            nb += 1;
        }
        // a few free arcs jumping from the impact to random cell corners
        for k in 0..ARCS {
            let hk = hash32(s.seed ^ (k as u32 * 0x85eb) ^ (frame / 2).wrapping_mul(0x27d4_eb2d));
            if h01(hk) < 0.45 || tau > CHARGE {
                continue;
            }
            let j = (hk >> 8) as usize % s.n;
            bolt(s, nb, s.impact, s.seeds[j], 0.9, hk);
            nb += 1;
        }
    }
    s.bolts[0] = nb as f32;

    // --- shards ----------------------------------------------------------
    for i in 0..s.n {
        let b = i * POLY;
        let (cx, cy) = (s.poly[b + 1], s.poly[b + 2]);
        let (dx, dy) = (cx - s.impact.0, cy - s.impact.1);
        let d = sqrt(dx * dx + dy * dy) + 1e-3;
        let h = hash32(s.seed ^ (i as u32).wrapping_mul(0x9e37_79b9));
        let f = tau - CHARGE - d / max(diag, 1.0) * 0.18;
        let o = i * 4;
        if f <= 0.0 {
            s.out[o..o + 4].copy_from_slice(&[0.0, 0.0, 0.0, 1.0]);
            continue;
        }
        let push = 70.0 + 140.0 * h01(h) * (1.0 - min(d / diag, 1.0));
        s.out[o] = dx / d * push * f + hsym(h >> 5) * 20.0 * f;
        s.out[o + 1] = dy / d * push * 0.5 * f - (60.0 + 80.0 * h01(h >> 9)) * f + 0.5 * GRAVITY * f * f;
        s.out[o + 2] = hsym(h >> 13) * 260.0 * f;
        let k = sat(f / FALL);
        s.out[o + 3] = 1.0 - k * k;
    }

    // --- object-level fx + text ---------------------------------------------
    let (stage, phase) = if tau < CHARGE {
        let fl = h01(hash32(s.seed ^ frame.wrapping_mul(0x632b_e5ab)));
        s.fx = [sat(tau / CHARGE) * (0.4 + 0.6 * fl), 0.0, 0.0, 0.0];
        (1, if tau > 0.12 { 0.1 } else { -1.0 })
    } else if tau < SEEP_AT {
        s.fx = [0.0, 0.0, 0.0, 0.0];
        (2, 0.1)
    } else if tau < SEEP_AT + SEEP {
        let k = (tau - SEEP_AT) / SEEP;
        let glow = sin(PI * min(k * 1.25, 1.0));
        s.fx = [0.0, glow, k, 6.0 * (1.0 - k) * (1.0 - k)];
        (3, 0.38 + 0.62 * k)
    } else {
        s.fx = [0.0; 4];
        (0, -1.0)
    };
    let ext = field::ext();
    for e in ext.iter_mut().take(s.g.1).skip(s.g.0) {
        *e = if phase >= 0.0 { phase + 1.0 } else { 0.0 };
    }
    if stage == 0 {
        s.active = false;
    }
    stage
}
