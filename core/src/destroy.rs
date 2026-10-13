//! Destruction director.
//!
//! Every ~10 s one table or picture on screen is destroyed in one of five
//! ways and then re-forms in place. Everything that moves is computed here;
//! JS only clones the element once per piece and applies what we write.
//!
//! ```text
//!  mode       pre-phase (element intact)            break                   pieces
//!  ─────────  ────────────────────────────────────  ──────────────────────  ───────────────
//!  ELECTRIC   blue-white arcs crawl the cracks       burst from impact       spider-web glass
//!  RUST       rust blooms from nuclei, text rots     crumbles in rust order  irregular chunks
//!  LIGHTNING  bolt from the top of the screen,       blast + scorch          spider-web glass
//!             white-out flash
//!  METEOR     fireball in on a diagonal              shockwave, shake,       dense web, huge
//!                                                    explosion + crater      blast velocities
//!  HACK       scanlines, glitch blocks, text →       strips shear sideways   horizontal strips
//!             hex, terminal + progress bar           and dissolve
//! ```
//!
//! Fracture: seeds are laid on a jittered polar web around the impact
//! (rings rₖ = r₀·g^k, 5 + 2.2k spokes each) so pieces are fine near the hit
//! and coarse at the rim, then cut into Voronoi cells by Sutherland–Hodgman
//! clipping of the bounding box against every perpendicular bisector.
//! Pieces tumble about a random 3-D axis (JS: `rotate3d` under perspective)
//! with a glint term on their brightness. Outer pieces may hang on for a
//! moment before they let go.
//!
//! Skins (rust, scorch, hack) are rendered here at reduced resolution with
//! 1-bit ordered-dither alpha – the same GIF grammar as the rest of the page
//! – and travel with the pieces.

use crate::field;
use crate::math::{abs, cos, floor, lerp, max, min, sat, sin, sqrt, PI, TAU};
use crate::noise::fbm3;
use crate::palette::{bayer, clear, put};
use crate::rng::{h01, hash32, hsym, Pcg};
use crate::Slot;

pub const MAXP: usize = 160;
const MAXV: usize = 14;
const POLY: usize = 6 + MAXV * 2; // nv, cx, cy, ax, ay, az, then xy pairs
const OUT: usize = 7; // dx, dy, dz, angle°, decay 0..1, brightness, 0 attached / 1 falling / 2 gone
const MAXL: usize = 160;
const LPTS: usize = 33;
const LINE: usize = 3 + LPTS * 2; // style, alpha, n, then xy pairs
const MAXE: usize = 260;
const SKW: usize = 400;
const SKH: usize = 240;
const FXN: usize = 16;
const SEEP: f32 = 0.8;

const ELECTRIC: u32 = 0;
const RUST: u32 = 1;
const LIGHTNING: u32 = 2;
const METEOR: u32 = 3;
const HACK: u32 = 4;

/// (pre-phase, time from break until re-form starts) per mode
const TIMING: [(f32, f32); 5] = [(1.0, 1.4), (2.4, 1.9), (1.05, 1.4), (1.45, 1.5), (1.8, 1.1)];
const STRIKE: f32 = 0.3; // lightning hits at this τ
const METEOR_HIT: f32 = 0.9; // meteor lands at this τ
/// longest we wait for the last debris to leave the screen after re-forming
const DEBRIS_GRACE: f32 = 3.0;

type V2 = (f32, f32);

struct Plan {
    mode: u32,
    w: f32,
    h: f32,
    impact: V2,
    sky: f32,
    vh: f32,
    origin: V2,
    zoom: f32,
    seed: u32,
    t0: f32,
    tau: f32,
    n: usize,
    g: (usize, usize),
    nuclei: [V2; 6],
    nn: usize,
    edges: [(V2, V2, f32); MAXE],
    ne: usize,
    rows: usize,
    active: bool,
}

static PL: Slot<Plan> = Slot::new(Plan {
    mode: 0,
    w: 0.0,
    h: 0.0,
    impact: (0.0, 0.0),
    sky: 0.0,
    vh: 800.0,
    origin: (0.0, 0.0),
    zoom: 1.0,
    seed: 0,
    t0: 0.0,
    tau: 0.0,
    n: 0,
    g: (0, 0),
    nuclei: [(0.0, 0.0); 6],
    nn: 0,
    edges: [((0.0, 0.0), (0.0, 0.0), 0.0); MAXE],
    ne: 0,
    rows: 0,
    active: false,
});

static POLYS: Slot<[f32; MAXP * POLY]> = Slot::new([0.0; MAXP * POLY]);
static OUTB: Slot<[f32; MAXP * OUT]> = Slot::new([0.0; MAXP * OUT]);
static LINES: Slot<[f32; 1 + MAXL * LINE]> = Slot::new([0.0; 1 + MAXL * LINE]);
static FXB: Slot<[f32; FXN]> = Slot::new([0.0; FXN]);
static SKIN: Slot<[u8; SKW * SKH * 4]> = Slot::new([0; SKW * SKH * 4]);
static RNG: Slot<Pcg> = Slot::new(Pcg::fixed());

#[no_mangle]
pub extern "C" fn destroy_polys() -> *const f32 {
    POLYS.get().as_ptr()
}
#[no_mangle]
pub extern "C" fn destroy_out() -> *const f32 {
    OUTB.get().as_ptr()
}
#[no_mangle]
pub extern "C" fn destroy_lines() -> *const f32 {
    LINES.get().as_ptr()
}
/// [flicker, glow, seep k, blur, screen flash, shake x, shake y, skin alpha,
///  meteor x, meteor y, meteor r, ring r, ring alpha, hack progress, τ, mode]
#[no_mangle]
pub extern "C" fn destroy_fx() -> *const f32 {
    FXB.get().as_ptr()
}

// ───────────────────────────────── geometry ─────────────────────────────────

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

fn write_poly(i: usize, pts: &[V2], centroid: V2, rng: &mut Pcg) {
    let pb = POLYS.get();
    let b = i * POLY;
    let nv = min(pts.len() as f32, MAXV as f32) as usize;
    // random tumble axis, biased off the screen plane so pieces flip visibly
    let (ax, ay, az) = (rng.sym(), rng.sym(), rng.sym() * 0.4);
    let l = sqrt(ax * ax + ay * ay + az * az) + 1e-4;
    pb[b] = nv as f32;
    pb[b + 1] = centroid.0;
    pb[b + 2] = centroid.1;
    pb[b + 3] = ax / l;
    pb[b + 4] = ay / l;
    pb[b + 5] = az / l;
    for k in 0..nv {
        pb[b + 6 + k * 2] = pts[k].0;
        pb[b + 7 + k * 2] = pts[k].1;
    }
}

fn centroid(poly: &[V2]) -> V2 {
    let n = poly.len();
    let (mut a2, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for k in 0..n {
        let (p, q) = (poly[k], poly[(k + 1) % n]);
        let cr = p.0 * q.1 - q.0 * p.1;
        a2 += cr;
        cx += (p.0 + q.0) * cr;
        cy += (p.1 + q.1) * cr;
    }
    if abs(a2) > 1e-3 {
        (cx / (3.0 * a2), cy / (3.0 * a2))
    } else {
        poly[0]
    }
}

fn voronoi(p: &mut Plan, seeds: &[V2], rng: &mut Pcg) {
    let (w, h) = (p.w, p.h);
    let n = seeds.len();
    p.ne = 0;
    let diag = sqrt(w * w + h * h);
    for i in 0..n {
        let mut poly = [(0.0f32, 0.0f32); 32];
        poly[..4].copy_from_slice(&[(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]);
        let mut nv = 4usize;
        let si = seeds[i];
        for (j, sj) in seeds.iter().enumerate() {
            if j == i {
                continue;
            }
            let m = ((si.0 + sj.0) * 0.5, (si.1 + sj.1) * 0.5);
            clip(&mut poly, &mut nv, m, (sj.0 - si.0, sj.1 - si.1));
        }
        let nv = min(nv as f32, MAXV as f32) as usize;
        let c = centroid(&poly[..nv]);
        write_poly(i, &poly[..nv], c, rng);

        // interior edges become crack lines (kept once, by the lower index)
        for k in 0..nv {
            let (a, b) = (poly[k], poly[(k + 1) % nv]);
            let border = |u: V2, v: V2| {
                (abs(u.0) < 0.5 && abs(v.0) < 0.5)
                    || (abs(u.1) < 0.5 && abs(v.1) < 0.5)
                    || (abs(u.0 - w) < 0.5 && abs(v.0 - w) < 0.5)
                    || (abs(u.1 - h) < 0.5 && abs(v.1 - h) < 0.5)
            };
            if border(a, b) || p.ne >= MAXE {
                continue;
            }
            let m = ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5);
            let mut best = (f32::MAX, i);
            for (j, sj) in seeds.iter().enumerate() {
                if j == i {
                    continue;
                }
                let d = (m.0 - sj.0) * (m.0 - sj.0) + (m.1 - sj.1) * (m.1 - sj.1);
                if d < best.0 {
                    best = (d, j);
                }
            }
            if best.1 < i {
                continue;
            }
            let (dx, dy) = (m.0 - p.impact.0, m.1 - p.impact.1);
            p.edges[p.ne] = (a, b, sqrt(dx * dx + dy * dy) / max(diag, 1.0) * 0.38);
            p.ne += 1;
        }
    }
    p.n = n;
}

/// Spider-web seeding round the impact: fine at the centre, coarse outside.
fn web(p: &Plan, rng: &mut Pcg, target: usize, r0: f32, growth: f32, out: &mut [V2; MAXP]) -> usize {
    let (w, h) = (p.w, p.h);
    let diag = sqrt(w * w + h * h);
    let mut n = 0usize;
    out[n] = p.impact;
    n += 1;
    let mut r = r0;
    let mut k = 0;
    while n < target && r < diag * 1.1 {
        let spokes = (5.0 + 2.2 * k as f32) as usize;
        let rot = rng.f01() * TAU;
        for j in 0..spokes {
            if n >= target {
                break;
            }
            let a = rot + (j as f32 + 0.5 * rng.sym()) / spokes as f32 * TAU;
            let rr = r * (1.0 + 0.28 * rng.sym());
            let (x, y) = (p.impact.0 + rr * cos(a), p.impact.1 + rr * sin(a));
            if x > 0.5 && y > 0.5 && x < w - 0.5 && y < h - 0.5 {
                out[n] = (x, y);
                n += 1;
            }
        }
        r *= growth;
        k += 1;
    }
    while n < target {
        out[n] = (rng.f01() * w, rng.f01() * h);
        n += 1;
    }
    n
}

/// Horizontal strips for HACK, each row cut into 1–4 segments.
fn strips(p: &mut Plan, rng: &mut Pcg) {
    let (w, h) = (p.w, p.h);
    let mut y = 0.0f32;
    let mut i = 0usize;
    let mut row = 0usize;
    while y < h - 0.01 && i < MAXP {
        let sh = min(3.0 + rng.f01() * 5.0, h - y);
        let cuts = 1 + (rng.next_u32() % 4) as usize;
        let mut xs = [0.0f32; 5];
        xs[cuts] = w;
        for c in 1..cuts {
            xs[c] = w * (c as f32 + 0.6 * rng.sym() * 0.5) / cuts as f32;
        }
        for c in 0..cuts {
            if i >= MAXP {
                break;
            }
            let r = [(xs[c], y), (xs[c + 1], y), (xs[c + 1], y + sh), (xs[c], y + sh)];
            write_poly(i, &r, ((xs[c] + xs[c + 1]) * 0.5, y + sh * 0.5), rng);
            // HACK pieces don't tumble; stash the row index in az
            POLYS.get()[i * POLY + 5] = row as f32;
            i += 1;
        }
        y += sh;
        row += 1;
    }
    p.n = i;
    p.rows = row;
    p.ne = 0;
}

/// Plan an event. Sizes are in the element's own CSS px; `sky` = its top edge
/// in the same units (bolts and meteors come from above the viewport).
/// (ox, oy) = its page origin in *document* px, and `zoom` the CSS zoom in
/// between, so glyph rest positions (document px) map into local space.
#[no_mangle]
pub extern "C" fn destroy_plan(
    mode: u32,
    w: f32,
    h: f32,
    ix: f32,
    iy: f32,
    sky: f32,
    ox: f32,
    oy: f32,
    seed: u32,
    g0: u32,
    g1: u32,
    t: f32,
    zoom: f32,
    vh: f32,
) -> u32 {
    destroy_cancel();
    let p = PL.get();
    let rng = RNG.get();
    rng.seed(seed as u64, 0xDE57);
    p.mode = min(mode as f32, 4.0) as u32;
    p.w = w;
    p.h = h;
    p.impact = (ix, iy);
    p.sky = sky;
    p.origin = (ox, oy);
    p.zoom = max(zoom, 0.05);
    p.vh = vh;
    p.seed = seed;
    p.t0 = t;
    p.tau = 0.0;
    p.g = (g0 as usize, min(g1 as f32, field::CAP as f32) as usize);

    let target = min(max(w * h / 240.0, 48.0), MAXP as f32) as usize;
    let mut seeds = [(0.0f32, 0.0f32); MAXP];
    match p.mode {
        HACK => strips(p, rng),
        RUST => {
            let n = (target as f32 * 0.7) as usize;
            for s in seeds.iter_mut().take(n) {
                *s = (rng.f01() * w, rng.f01() * h);
            }
            voronoi(p, &seeds[..n], rng);
        }
        METEOR => {
            let n = web(p, rng, target, 4.0, 1.42, &mut seeds);
            voronoi(p, &seeds[..n], rng);
        }
        _ => {
            let n = web(p, rng, target, 6.0, 1.5, &mut seeds);
            voronoi(p, &seeds[..n], rng);
        }
    }
    p.nn = 3 + (rng.next_u32() % 4) as usize;
    for k in 0..p.nn {
        p.nuclei[k] = (rng.f01() * w, rng.f01() * h);
    }
    p.active = true;
    p.n as u32
}

#[no_mangle]
pub extern "C" fn destroy_cancel() {
    let p = PL.get();
    if p.active {
        for e in field::ext().iter_mut().take(p.g.1).skip(p.g.0) {
            *e = 0.0;
        }
    }
    p.active = false;
    LINES.get()[0] = 0.0;
    *FXB.get() = [0.0; FXN];
}

// ───────────────────────────────── fields ───────────────────────────────────

/// Rust coverage at local (x, y) for progress `pr` ∈ [0, 1].
fn rust_at(p: &Plan, x: f32, y: f32, pr: f32) -> f32 {
    let diag = sqrt(p.w * p.w + p.h * p.h);
    let n = 0.5 + 0.5 * fbm3(x / 26.0, y / 26.0, 1.3, 4, p.seed);
    let mut nd = 1.0f32;
    for k in 0..p.nn {
        let (dx, dy) = (x - p.nuclei[k].0, y - p.nuclei[k].1);
        nd = min(nd, sqrt(dx * dx + dy * dy) / (0.55 * diag));
    }
    let v = 0.55 * n + 0.45 * (1.0 - nd);
    sat((v - (1.0 - pr * 1.15)) * 5.0)
}

/// Normalised char distance from the impact at time τ (≤ 1 is burnt).
/// The burn starts as a scorch round the impact and spreads, noise-warped,
/// until the whole object is blackened just before it breaks.
fn char_at(p: &Plan, x: f32, y: f32, tau: f32) -> f32 {
    let (pre, _) = TIMING[p.mode as usize];
    let start = impact_time(p);
    let cp = sat((tau - start) / max(pre - start, 0.05));
    let diag = sqrt(p.w * p.w + p.h * p.h);
    let rb = if p.mode == METEOR { 40.0 + 0.22 * min(p.w, p.h) } else { 18.0 + 0.1 * min(p.w, p.h) };
    let r = lerp(rb, diag * 1.15, cp * sqrt(cp));
    let (dx, dy) = (x - p.impact.0, y - p.impact.1);
    sqrt(dx * dx + dy * dy) / r + 0.3 * fbm3(x / 12.0, y / 12.0, 7.1, 3, p.seed ^ 0xB0)
}

fn impact_time(p: &Plan) -> f32 {
    match p.mode {
        ELECTRIC => 0.15,
        LIGHTNING => STRIKE,
        METEOR => METEOR_HIT,
        _ => 0.0,
    }
}

// ───────────────────────────────── lines ────────────────────────────────────

fn line(n: &mut usize, style: f32, alpha: f32, pts: &[V2]) {
    if *n >= MAXL {
        return;
    }
    let lb = LINES.get();
    let b = 1 + *n * LINE;
    let k = min(pts.len() as f32, LPTS as f32) as usize;
    lb[b] = style;
    lb[b + 1] = alpha;
    lb[b + 2] = k as f32;
    for (j, q) in pts.iter().take(k).enumerate() {
        lb[b + 3 + j * 2] = q.0;
        lb[b + 4 + j * 2] = q.1;
    }
    *n += 1;
}

/// Midpoint-displaced polyline from a to b with 2^levels segments.
fn jagged(a: V2, b: V2, levels: u32, rough: f32, salt: u32, out: &mut [V2; LPTS]) -> usize {
    let segs = 1usize << levels;
    out[0] = a;
    out[segs] = b;
    let mut step = segs / 2;
    let mut amp = rough * sqrt((b.0 - a.0) * (b.0 - a.0) + (b.1 - a.1) * (b.1 - a.1));
    while step >= 1 {
        let mut i = step;
        while i < segs {
            let (p, q) = (out[i - step], out[i + step]);
            let (dx, dy) = (q.0 - p.0, q.1 - p.1);
            let l = sqrt(dx * dx + dy * dy) + 1e-4;
            let j = hsym(hash32(salt ^ (i as u32 * 0x9e37) ^ (step as u32 * 0x85eb))) * amp;
            out[i] = ((p.0 + q.0) * 0.5 - dy / l * j, (p.1 + q.1) * 0.5 + dx / l * j);
            i += step * 2;
        }
        amp *= 0.55;
        step /= 2;
    }
    segs + 1
}

// ───────────────────────────────── step ─────────────────────────────────────

/// Advance. Returns 0 done, 1 pre-phase, 2 broken, 3 re-forming,
/// 4 re-formed but debris still on screen.
#[no_mangle]
pub extern "C" fn destroy_step(t: f32) -> u32 {
    let p = PL.get();
    if !p.active {
        return 0;
    }
    let tau = t - p.t0;
    p.tau = tau;
    let (pre, after) = TIMING[p.mode as usize];
    let reform = pre + after;
    let frame = floor(tau * 30.0) as u32;
    let diag = sqrt(p.w * p.w + p.h * p.h);
    let fx = FXB.get();
    *fx = [0.0; FXN];
    fx[14] = tau;
    fx[15] = p.mode as f32;

    // ── lines ──
    let mut nl = 0usize;
    let mut pts = [(0.0f32, 0.0f32); LPTS];
    let cracks = (p.mode == ELECTRIC && tau < pre + 0.22) || (p.mode == LIGHTNING && tau > STRIKE && tau < pre + 0.25);
    if cracks {
        for e in 0..p.ne {
            let (a, b, delay) = p.edges[e];
            let d0 = if p.mode == LIGHTNING { STRIKE + delay * 0.4 } else { delay };
            if tau < d0 {
                continue;
            }
            let hf = hash32(p.seed ^ (e as u32 * 0x2c1b_3c6d) ^ frame.wrapping_mul(0x51ed_270b));
            let fade = if tau > pre { 1.0 - (tau - pre) / 0.22 } else { 1.0 };
            let k = jagged(a, b, 3, 0.18, hf, &mut pts);
            line(&mut nl, 0.0, (0.45 + 0.55 * h01(hf)) * sat(fade), &pts[..k]);
        }
    }
    if p.mode == LIGHTNING && tau > STRIKE - 0.06 && tau < STRIKE + 0.4 {
        // three strokes: the leader, the return stroke, an afterflash
        let s = tau - (STRIKE - 0.06);
        let on = s < 0.1 || (s > 0.13 && s < 0.26) || (s > 0.3 && s < 0.38);
        if on {
            let stroke = if s < 0.1 { 0 } else if s < 0.26 { 1 } else { 2 };
            let hs = hash32(p.seed ^ 0x7E57 ^ (stroke as u32) ^ (frame / 2).wrapping_mul(0x632b_e5ab));
            let top = (p.impact.0 + hsym(p.seed) * 70.0, -p.sky - 20.0);
            let k = jagged(top, p.impact, 5, 0.16, hs, &mut pts);
            let main = pts;
            line(&mut nl, 1.0, 1.0, &main[..k]);
            for br in 0..5u32 {
                let hb = hash32(hs ^ br.wrapping_mul(0x27d4_eb2d));
                let from = main[6 + (hb as usize % 20)];
                let ang = (PI * 0.5) + hsym(hb >> 5) * 0.9;
                let len = 40.0 + 110.0 * h01(hb >> 11);
                let to = (from.0 + cos(ang) * len * if hb & 1 == 0 { 1.0 } else { -1.0 }, from.1 + sin(ang) * len);
                let kb = jagged(from, to, 4, 0.22, hb, &mut pts);
                line(&mut nl, 2.0, 0.8, &pts[..kb]);
            }
        }
    }
    LINES.get()[0] = nl as f32;

    // ── screen-level fx ──
    let hit = impact_time(p);
    match p.mode {
        ELECTRIC => {
            if tau < pre {
                fx[0] = sat(tau / pre) * (0.4 + 0.6 * h01(hash32(p.seed ^ frame.wrapping_mul(0x632b_e5ab))));
            }
        }
        LIGHTNING => {
            let s = tau - STRIKE;
            if (0.0..0.35).contains(&s) {
                fx[4] = if s < 0.08 { 0.9 } else if (0.14..0.2).contains(&s) { 0.55 } else { 0.0 };
                fx[0] = 1.0 - s / 0.35;
                let sh = (1.0 - s / 0.35) * 3.0;
                fx[5] = hsym(hash32(frame ^ 0x51)) * sh;
                fx[6] = hsym(hash32(frame ^ 0x52)) * sh;
                fx[11] = 10.0 + 160.0 * s / 0.35;
                fx[12] = 0.6 * (1.0 - s / 0.35);
            }
        }
        METEOR => {
            if tau < METEOR_HIT {
                let k = tau / METEOR_HIT;
                let k = k * sqrt(k); // accelerating in
                let dir = if hsym(p.seed >> 3) < 0.0 { -1.0 } else { 1.0 };
                let start = (p.impact.0 + dir * 650.0, -p.sky - 220.0);
                fx[8] = lerp(start.0, p.impact.0, k);
                fx[9] = lerp(start.1, p.impact.1, k);
                fx[10] = 7.0 + 5.0 * k;
            } else {
                let s = tau - METEOR_HIT;
                if s < 0.15 {
                    fx[4] = 1.0 - s / 0.15;
                }
                if s < 0.5 {
                    let k = s / 0.5;
                    fx[11] = 20.0 + 520.0 * sqrt(k);
                    fx[12] = 1.0 - k;
                }
                if s < 0.4 {
                    let sh = (1.0 - s / 0.4) * 9.0;
                    fx[5] = hsym(hash32(frame ^ 0x61)) * sh;
                    fx[6] = hsym(hash32(frame ^ 0x62)) * sh;
                }
            }
        }
        HACK => fx[13] = sat(tau / pre),
        _ => {}
    }
    fx[7] = match p.mode {
        RUST | HACK => 1.0,
        _ => {
            if tau > hit {
                1.0
            } else {
                0.0
            }
        }
    };

    // ── pieces ──
    let mut all_gone = true;
    let pb = POLYS.get();
    let ob = OUTB.get();
    for i in 0..p.n {
        let b = i * POLY;
        let (cx, cy) = (pb[b + 1], pb[b + 2]);
        let (dx, dy) = (cx - p.impact.0, cy - p.impact.1);
        let d = sqrt(dx * dx + dy * dy) + 1e-3;
        let (ux, uy) = (dx / d, dy / d);
        let near = 1.0 - min(d / diag, 1.0);
        let hsh = hash32(p.seed ^ (i as u32).wrapping_mul(0x9e37_79b9));
        let (u1, u2, u3, u4) = (h01(hsh), h01(hsh >> 7), h01(hsh >> 14), h01(hsh >> 21));
        let o = i * OUT;

        let delay = match p.mode {
            RUST => (1.0 - rust_at(p, cx, cy, 0.75)) * 1.3 + 0.35 * u1,
            HACK => pb[b + 5] / max(p.rows as f32, 1.0) * 0.55 + 0.12 * u1,
            _ => {
                let hang = if d / diag > 0.3 && u4 < 0.35 { 0.15 + 0.6 * u3 } else { 0.0 };
                d / diag * 0.2 + 0.12 * u1 + hang
            }
        };
        let f = tau - pre - delay;

        let (mut ddx, mut dz, mut ang, mut br) = (0.0, 0.0, 0.0, 1.0);
        let ddy: f32;
        let decay: f32;
        if f <= 0.0 {
            // still attached: shiver just before letting go
            if f > -0.3 && tau > pre {
                ddx = sin(tau * 70.0 + i as f32) * 0.9;
            }
            ob[o..o + OUT].copy_from_slice(&[ddx, 0.0, 0.0, 0.0, 0.0, br, 0.0]);
            continue;
        }
        match p.mode {
            RUST => {
                ddx = hsym(hsh) * 14.0 * f;
                ddy = 0.5 * 900.0 * f * f;
                ang = (40.0 + 140.0 * u2) * f * if u3 < 0.5 { -1.0 } else { 1.0 };
                dz = hsym(hsh >> 9) * 80.0 * f;
                decay = sat(f / 1.0);
                br = 0.85;
            }
            HACK => {
                let fq = floor(f * 15.0) / 15.0; // digital: 15 fps steps
                let dir = if u2 < 0.5 { -1.0 } else { 1.0 };
                ddx = dir * (60.0 * fq + 1100.0 * fq * fq) * (0.6 + 0.8 * u3);
                ddy = 0.5 * 600.0 * fq * fq * u4;
                decay = 0.0;
                br = 1.3;
            }
            _ => {
                let (vr, up, spin, g, kz) = match p.mode {
                    METEOR => (260.0 + 700.0 * near * sqrt(near), 220.0 + 260.0 * near, 400.0 + 700.0 * u2, 1450.0, 420.0),
                    LIGHTNING => (90.0 + 420.0 * near * near, 90.0 + 160.0 * near, 300.0 + 500.0 * u2, 1350.0, 260.0),
                    _ => (60.0 + 280.0 * near * near, 60.0 + 110.0 * near, 250.0 + 450.0 * u2, 1300.0, 220.0),
                };
                let v = vr * (0.5 + 1.0 * u1);
                ddx = ux * v * f + hsym(hsh >> 3) * 70.0 * f;
                ddy = uy * v * 0.6 * f - up * (0.4 + 0.9 * u3) * f + 0.5 * g * f * f;
                dz = hsym(hsh >> 9) * kz * f;
                ang = spin * f * if u4 < 0.5 { -1.0 } else { 1.0 };
                decay = sat(f / 0.9); // keeps charring on the way down
                br = 1.0 + 0.35 * sin(ang * (PI / 90.0)); // glint as it tumbles
            }
        }
        // gone once it has dropped (or slid) clean out of the viewport
        let gone = if p.mode == HACK { abs(ddx) > p.w + 300.0 } else { ddy > p.vh + p.h + 120.0 };
        if !gone {
            all_gone = false;
        }
        ob[o..o + OUT].copy_from_slice(&[ddx, ddy, dz, ang, decay, br, if gone { 2.0 } else { 1.0 }]);
    }

    // ── re-form ──
    let stage = if tau < pre {
        1
    } else if tau < reform {
        2
    } else if tau < reform + SEEP {
        let k = (tau - reform) / SEEP;
        fx[1] = sin(PI * min(k * 1.25, 1.0));
        fx[2] = k;
        fx[3] = 6.0 * (1.0 - k) * (1.0 - k);
        3
    } else if !all_gone && tau < reform + SEEP + DEBRIS_GRACE {
        4 // re-formed; the last debris is still falling
    } else {
        0
    };

    // ── glyph text ──
    let ext = field::ext();
    for i in p.g.0..p.g.1 {
        let (gx, gy) = field::pos(i);
        let (lx, ly) = ((gx - p.origin.0) / p.zoom, (gy - p.origin.1) / p.zoom);
        let v = match stage {
            0 | 4 => 0.0,
            3 => {
                let ph = 0.38 + 0.62 * fx[2];
                if p.mode == HACK { 10.0 + ph } else { 1.0 + ph }
            }
            2 => {
                if p.mode == HACK { 10.1 } else { 1.1 }
            }
            _ => match p.mode {
                RUST => {
                    if rust_at(p, lx, ly, sat(tau / pre)) > 0.45 { 1.1 } else { 0.0 }
                }
                ELECTRIC | LIGHTNING | METEOR => {
                    if tau > hit && char_at(p, lx, ly, tau) < 1.3 { 1.1 } else { 0.0 }
                }
                _ => {
                    if ly < sat(tau / (pre * 0.6)) * p.h { 10.1 } else { 0.0 }
                }
            },
        };
        ext[i] = v;
    }

    if stage == 0 {
        destroy_cancel();
    }
    stage
}

// ───────────────────────────────── skin ─────────────────────────────────────

/// Render the current skin (rust / scorch / hack) at 1/`scale` resolution.
/// Returns null when this mode has none.
#[no_mangle]
pub extern "C" fn destroy_skin(scale: f32) -> *const u8 {
    let p = PL.get();
    if !p.active {
        return core::ptr::null();
    }
    let buf = SKIN.get();
    let sw = min((p.w / scale) as f32 + 1.0, SKW as f32) as usize;
    let sh = min((p.h / scale) as f32 + 1.0, SKH as f32) as usize;
    let (pre, _) = TIMING[p.mode as usize];
    let tau = p.tau;
    let frame = floor(tau * 20.0) as u32;
    let hit = impact_time(p);

    for y in 0..sh {
        for x in 0..sw {
            let i4 = (y * sw + x) * 4;
            let (lx, ly) = ((x as f32 + 0.5) * scale, (y as f32 + 0.5) * scale);
            let th = bayer(x as u32, y as u32);
            let mut col: Option<(f32, f32, f32)> = None;
            match p.mode {
                RUST => {
                    let a = rust_at(p, lx, ly, sat(tau / pre));
                    if a > th {
                        let n = 0.5 + 0.5 * fbm3(lx / 5.0, ly / 5.0, 9.1, 2, p.seed ^ 0x55);
                        let fresh = sat((1.0 - a) * 3.0);
                        let (mut r, mut g, mut b) = (lerp(0.42, 0.74, n), lerp(0.17, 0.36, n), lerp(0.05, 0.10, n));
                        r = lerp(r, 0.9, fresh * 0.5);
                        g = lerp(g, 0.5, fresh * 0.4);
                        if a > 0.8 && n < 0.32 {
                            // pitting
                            r = 0.18;
                            g = 0.08;
                            b = 0.03;
                        }
                        col = Some((r, g, b));
                    }
                }
                ELECTRIC | LIGHTNING | METEOR => {
                    if tau > hit {
                        let d = char_at(p, lx, ly, min(tau, pre));
                        let alpha = if d < 0.6 { 1.0 } else { sat((1.0 - d) / 0.4) };
                        if alpha > th {
                            let k = sat(d);
                            let mut c = (lerp(0.04, 0.32, k), lerp(0.03, 0.15, k), lerp(0.02, 0.05, k));
                            let hz = hash32((y * sw + x) as u32 ^ frame.wrapping_mul(0x632b_e5ab));
                            if d > 0.72 && h01(hz) < 0.35 {
                                c = (1.0, 0.45 + 0.4 * h01(hz >> 8), 0.05); // embers
                            }
                            col = Some(c);
                        }
                    }
                }
                _ => {
                    // HACK
                    let pr = sat(tau / (pre * 0.5));
                    let scan = sat(tau / (pre * 0.6)) * p.h;
                    let row = (ly / 6.0) as u32;
                    let hb = hash32(row ^ (frame / 3).wrapping_mul(0x27d4_eb2d) ^ p.seed);
                    if abs(ly - scan) < scale * 0.8 && tau < pre * 0.6 {
                        col = Some((0.6, 1.0, 0.7));
                    } else if pr > 0.2 && hb & 7 == 0 {
                        let x0 = h01(hb >> 3) * p.w;
                        let len = 20.0 + 120.0 * h01(hb >> 11);
                        if lx > x0 && lx < x0 + len {
                            col = Some(if hb & 0x100 == 0 { (0.0, 0.9, 0.3) } else { (0.0, 0.1, 0.0) });
                        }
                    } else if y & 1 == 0 && pr * 0.45 > th {
                        col = Some((0.0, 0.75, 0.25));
                    }
                }
            }
            match col {
                Some((r, g, b)) => put(&mut *buf, i4, x as u32, y as u32, r, g, b),
                None => clear(&mut *buf, i4),
            }
        }
    }
    buf.as_ptr()
}

#[no_mangle]
pub extern "C" fn destroy_skin_w(scale: f32) -> u32 {
    min((PL.get().w / scale) + 1.0, SKW as f32) as u32
}

#[no_mangle]
pub extern "C" fn destroy_skin_h(scale: f32) -> u32 {
    min((PL.get().h / scale) + 1.0, SKH as f32) as u32
}
