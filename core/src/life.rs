//! Life: every object on the page (bordered tables, picture surfaces) behaves
//! a little like a cell when you are near it.
//!
//! Per object, per frame:
//!
//! ```text
//!  activity  a ← max(0.9·energy, proximity to the pointer), fast attack,
//!            slow release – zero when nobody is around, so a still page is
//!            exactly a still page
//!  membrane  SVG feTurbulence → feDisplacementMap on the element; we drive
//!            the displacement scale and slowly drift the noise frequency so
//!            the whole thing undulates like cytoplasm
//!  breath    anti-phase squash/stretch (sx = 1 + ε·sin ωt, sy = 1 − ε·sin ωt)
//!  softening corners round off, each on its own slow phase, like a cell
//!            losing its box shape
//!  fibrillation  Poisson-scheduled bursts (∝ a) of fast, irregular tremor:
//!            displacement spikes and the noise seed re-rolled every frame
//!  droplets  clear, glossy bulbs swell on the lower edge, neck out, pinch
//!            off and fall (g = 1100 px/s²), stretching with speed
//! ```

use crate::math::{abs, damp, max, min, sat, sin};
use crate::rng::{h01, Pcg};
use crate::Slot;

pub const MAXO: usize = 16;
const MAXD: usize = 64;
const OUT: usize = 10; // displacement, freq x, freq y, seed, sx, sy, radius tl/tr/br/bl
const DROP: usize = 7; // x, y, r, neck, stretch, attached, object

#[derive(Clone, Copy)]
struct Obj {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    act: f32,
    fib: f32,
    drip: f32,
    phase: f32,
}

const O0: Obj = Obj { x: 0.0, y: 0.0, w: 0.0, h: 0.0, act: 0.0, fib: 0.0, drip: 0.0, phase: 0.0 };

#[derive(Clone, Copy)]
struct Drop {
    obj: usize,
    u: f32, // anchor along the lower edge, 0..1
    x: f32,
    y: f32,
    vy: f32,
    r: f32,
    rmax: f32,
    age: f32,
    grow: f32,
    falling: bool,
    live: bool,
}

const D0: Drop = Drop { obj: 0, u: 0.0, x: 0.0, y: 0.0, vy: 0.0, r: 0.0, rmax: 0.0, age: 0.0, grow: 1.0, falling: false, live: false };

struct Life {
    o: [Obj; MAXO],
    d: [Drop; MAXD],
    out: [f32; MAXO * OUT],
    dout: [f32; 1 + MAXD * DROP],
    frame: u32,
}

static L: Slot<Life> = Slot::new(Life { o: [O0; MAXO], d: [D0; MAXD], out: [0.0; MAXO * OUT], dout: [0.0; 1 + MAXD * DROP], frame: 0 });
static RNG: Slot<Pcg> = Slot::new(Pcg::fixed());

#[no_mangle]
pub extern "C" fn life_out() -> *const f32 {
    L.get().out.as_ptr()
}

#[no_mangle]
pub extern "C" fn life_drops() -> *const f32 {
    L.get().dout.as_ptr()
}

/// Object `i`'s rectangle in document px (call every frame; it may move).
#[no_mangle]
pub extern "C" fn life_set(i: u32, x: f32, y: f32, w: f32, h: f32) {
    let l = L.get();
    if let Some(o) = l.o.get_mut(i as usize) {
        o.x = x;
        o.y = y;
        o.w = w;
        o.h = h;
        if o.phase == 0.0 {
            o.phase = 1.0 + i as f32 * 2.399;
        }
    }
}

#[no_mangle]
pub extern "C" fn life_reset() {
    let l = L.get();
    for o in l.o.iter_mut() {
        o.act = 0.0;
        o.fib = 0.0;
        o.drip = 0.0;
    }
    for d in l.d.iter_mut() {
        d.live = false;
    }
    l.out.fill(0.0);
    l.dout[0] = 0.0;
}

/// `n` objects; pointer at document (mx, my); `floor` = document y of the
/// viewport's bottom edge (drops below it are dropped).
#[no_mangle]
pub extern "C" fn life_step(n: u32, dt: f32, t: f32, mx: f32, my: f32, energy: f32, floor: f32) {
    let l = L.get();
    let rng = RNG.get();
    l.frame = l.frame.wrapping_add(1);
    let n = min(n as f32, MAXO as f32) as usize;

    for i in 0..n {
        let o = &mut l.o[i];
        // distance from pointer to the rectangle (0 inside)
        let dx = max(max(o.x - mx, mx - (o.x + o.w)), 0.0);
        let dy = max(max(o.y - my, my - (o.y + o.h)), 0.0);
        let prox = sat(1.0 - (dx * dx + dy * dy) / (170.0 * 170.0));
        // global motion wakes every cell a little; the pointer wakes its neighbours fully
        let target = max(if energy > 0.08 { energy * 0.55 } else { 0.0 }, prox);
        o.act += (target - o.act) * damp(if target > o.act { 6.0 } else { 1.6 }, dt);
        if o.act < 0.03 {
            o.act = 0.0;
        }

        if o.fib > 0.0 {
            o.fib -= dt;
        } else if o.act > 0.15 && rng.f01() < dt * 0.9 * o.act * o.act {
            o.fib = 0.18 + rng.f01() * 0.45;
        }
        let fib = if o.fib > 0.0 { 1.0 } else { 0.0 };

        let a = o.act;
        let breath = 0.5 + 0.5 * sin(t * 1.7 + o.phase);
        let b = i * OUT;
        if a <= 0.0 && fib == 0.0 {
            l.out[b..b + OUT].fill(0.0);
        } else {
            let tremor = fib * (3.0 + 6.0 * h01(rng.next_u32()));
            l.out[b] = a * (1.5 + 3.5 * breath) + tremor;
            l.out[b + 1] = 0.010 + 0.004 * sin(t * 0.53 + o.phase);
            l.out[b + 2] = 0.018 + 0.006 * sin(t * 0.41 + o.phase * 1.7);
            l.out[b + 3] = if fib > 0.0 { (l.frame % 97) as f32 } else { (i * 7) as f32 };
            let sq = a * 0.010 * sin(t * 2.3 + o.phase) + fib * 0.006 * sin(t * 61.0 + o.phase);
            l.out[b + 4] = 1.0 + sq;
            l.out[b + 5] = 1.0 - sq;
            let rmax = min(min(o.w, o.h) * 0.32, 22.0) * a;
            for k in 0..4 {
                let wob = 0.55 + 0.45 * sin(t * (0.7 + 0.13 * k as f32) + o.phase + k as f32 * 1.9);
                l.out[b + 6 + k] = rmax * wob;
            }
        }

        // droplets bud while the cell is active
        o.drip -= dt * a * (0.6 + o.w / 260.0);
        if o.drip <= 0.0 && a > 0.2 {
            o.drip = 0.8 + rng.f01() * 1.6;
            if let Some(d) = l.d.iter_mut().find(|d| !d.live) {
                let rmax = 3.0 + rng.f01() * 5.0;
                *d = Drop { obj: i, u: 0.08 + 0.84 * rng.f01(), x: 0.0, y: 0.0, vy: 0.0, r: 0.0, rmax, age: 0.0, grow: 0.6 + rng.f01() * 0.9, falling: false, live: true };
            }
        }
    }

    let mut nd = 0usize;
    for d in l.d.iter_mut() {
        if !d.live {
            continue;
        }
        d.age += dt;
        let o = l.o[d.obj];
        let (neck, stretch, attached);
        if !d.falling {
            let k = sat(d.age / d.grow);
            d.r = d.rmax * (k * (2.0 - k)); // ease-out swell
            d.x = o.x + d.u * o.w;
            let nk = d.r * (0.4 + 1.6 * k * k);
            d.y = o.y + o.h + nk + d.r;
            neck = nk;
            stretch = 1.0;
            attached = 1.0;
            if k >= 1.0 {
                d.falling = true;
                d.vy = 30.0;
            }
        } else {
            d.vy += 1100.0 * dt;
            d.y += d.vy * dt;
            neck = 0.0;
            stretch = min(1.9, 1.0 + abs(d.vy) / 900.0);
            attached = 0.0;
            if d.y - d.r * 3.0 > floor {
                d.live = false;
                continue;
            }
        }
        let b = 1 + nd * DROP;
        l.dout[b..b + DROP].copy_from_slice(&[d.x, d.y, d.r, neck, stretch, attached, d.obj as f32]);
        nd += 1;
    }
    l.dout[0] = nd as f32;
}
