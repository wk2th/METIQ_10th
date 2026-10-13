//! Corruption spread: first-passage percolation over the page's glyphs.
//!
//! A hover is not a ripple. The corruption *infects* the glyph under the
//! pointer and then hops glyph-to-glyph, neighbours only (same line and the
//! lines above/below), along edges whose cost is randomised per event:
//!
//! ```text
//!   w(u,v) = |uv| · (0.3 + 1.8·ξ³)       ξ ~ U(0,1) per (event, edge)
//!   w(u,v) ×= 5                           with p = 0.1    (an edge that holds)
//! ```
//!
//! Arrival = shortest path under those weights (Dijkstra on a lazy binary
//! heap), cut off at a random budget. Shortest paths through a random medium
//! are fractal: the front grows fingers, stalls, routes around "solid" edges
//! and leaves islands – which is what something actually breaking looks like.
//! The search runs once per event; per frame each glyph just reads its own
//! arrival time from a table.
//!
//! Neighbour queries go through a spatial hash (44 px cells, 4096 buckets,
//! intrusive chains) rebuilt whenever JS re-measures the layout.

use crate::math::{floor, max, min, rsqrt};
use crate::rng::{h01, hash32, Pcg};
use crate::Slot;

pub const CAP: usize = crate::field::CAP;
pub const EVENTS: usize = 12;
const CELL: f32 = 44.0;
const BUCKETS: usize = 4096;
const HEAP: usize = 16384;
const INF: f32 = 1e30;

// Chains store index+1 so that 0 means "empty": every static here is
// all-zero, lives in .bss and costs nothing in the binary.
struct Grid {
    head: [u32; BUCKETS],
    next: [u32; CAP],
    x: [f32; CAP],
    y: [f32; CAP],
    n: usize,
}

static G: Slot<Grid> = Slot::new(Grid { head: [0; BUCKETS], next: [0; CAP], x: [0.0; CAP], y: [0.0; CAP], n: 0 });

struct Events {
    /// per event, per glyph: arrival time (s) – INF if never reached
    arr: [[f32; CAP]; EVENTS],
    /// per event, per glyph: how deep into the budget it was reached, 0..1
    depth: [[f32; CAP]; EVENTS],
    t0: [f32; EVENTS],
    seed: [u32; EVENTS],
    head: usize,
}

static E: Slot<Events> = Slot::new(Events {
    arr: [[0.0; CAP]; EVENTS],
    depth: [[0.0; CAP]; EVENTS],
    t0: [0.0; EVENTS],
    seed: [0; EVENTS],
    head: 0,
});

struct Scratch {
    heap: [(f32, u32); HEAP],
    cost: [f32; CAP],
    ready: bool,
}

static S: Slot<Scratch> = Slot::new(Scratch { heap: [(0.0, 0); HEAP], cost: [0.0; CAP], ready: false });
static RNG: Slot<Pcg> = Slot::new(Pcg::fixed());

#[inline]
fn bucket(cx: i32, cy: i32) -> usize {
    ((cx as u32).wrapping_mul(0x8da6_b343) ^ (cy as u32).wrapping_mul(0xd816_3841)) as usize & (BUCKETS - 1)
}

/// Rebuild the spatial hash from the field's rest positions. Glyphs of kind 3
/// (marquee) move on their own and are left out.
pub fn index(inp: &[f32], stride: usize, n: usize) {
    let g = G.get();
    g.head.fill(0);
    g.n = n;
    for i in 0..n {
        let (x, y, kind) = (inp[i * stride], inp[i * stride + 1], inp[i * stride + 2]);
        g.x[i] = x;
        g.y[i] = y;
        g.next[i] = 0;
        if kind >= 3.0 {
            continue;
        }
        let b = bucket(floor(x / CELL) as i32, floor(y / CELL) as i32);
        g.next[i] = g.head[b];
        g.head[b] = i as u32 + 1;
    }
    let s = S.get();
    if !s.ready {
        for e in E.get().arr.iter_mut() {
            e.fill(INF);
        }
        s.ready = true;
    }
}

fn push(heap: &mut [(f32, u32); HEAP], len: &mut usize, item: (f32, u32)) {
    if *len >= HEAP {
        return;
    }
    let mut i = *len;
    heap[i] = item;
    *len += 1;
    while i > 0 {
        let p = (i - 1) / 2;
        if heap[p].0 <= heap[i].0 {
            break;
        }
        heap.swap(p, i);
        i = p;
    }
}

fn pop(heap: &mut [(f32, u32); HEAP], len: &mut usize) -> (f32, u32) {
    let top = heap[0];
    *len -= 1;
    heap[0] = heap[*len];
    let mut i = 0;
    loop {
        let (l, r) = (2 * i + 1, 2 * i + 2);
        let mut m = i;
        if l < *len && heap[l].0 < heap[m].0 {
            m = l;
        }
        if r < *len && heap[r].0 < heap[m].0 {
            m = r;
        }
        if m == i {
            break;
        }
        heap.swap(m, i);
        i = m;
    }
    top
}

/// Start a corruption event at glyph `src`, time `t`. Returns glyphs reached.
pub fn start(src: usize, t: f32) -> u32 {
    let g = G.get();
    let s = S.get();
    let ev = E.get();
    if !s.ready || src >= g.n {
        return 0;
    }
    let e = ev.head;
    ev.head = (ev.head + 1) % EVENTS;
    let rng = RNG.get();
    let seed = rng.next_u32();
    ev.t0[e] = t;
    ev.seed[e] = seed;
    let arr = &mut ev.arr[e];
    let depth = &mut ev.depth[e];
    arr[..g.n].fill(INF);

    let budget = 62.0 + rng.f01() * 88.0; // px of "effective" distance
    let speed = 260.0 + rng.f01() * 220.0; // px/s along the cheapest path
    s.cost[..g.n].fill(INF);
    let mut len = 0usize;
    let mut reached = 0u32;
    s.cost[src] = 0.0;
    push(&mut s.heap, &mut len, (0.0, src as u32));

    while len > 0 {
        let (c, u) = pop(&mut s.heap, &mut len);
        let u = u as usize;
        if c > s.cost[u] {
            continue;
        }
        arr[u] = t + c / speed;
        depth[u] = c / budget;
        reached += 1;
        let (ux, uy) = (g.x[u], g.y[u]);
        let (cx, cy) = (floor(ux / CELL) as i32, floor(uy / CELL) as i32);
        for oy in -1..=1 {
            for ox in -1..=1 {
                let mut v = g.head[bucket(cx + ox, cy + oy)];
                while v != 0 {
                    let vi = (v - 1) as usize;
                    v = g.next[vi];
                    if vi == u {
                        continue;
                    }
                    let (dx, dy) = (g.x[vi] - ux, g.y[vi] - uy);
                    let d2 = dx * dx + dy * dy;
                    if d2 > CELL * CELL {
                        continue;
                    }
                    let (a, b) = (min(u as f32, vi as f32) as u32, max(u as f32, vi as f32) as u32);
                    let h = hash32(seed ^ a.wrapping_mul(0x9e37_79b1) ^ b.wrapping_mul(0x85eb_ca6b));
                    let xi = h01(h);
                    let mut w = d2 * rsqrt(d2 + 1e-6) * (0.3 + 1.8 * xi * xi * xi);
                    if h01(h >> 9) < 0.1 {
                        w *= 5.0;
                    }
                    let nc = c + w;
                    if nc < s.cost[vi] && nc <= budget {
                        s.cost[vi] = nc;
                        push(&mut s.heap, &mut len, (nc, vi as u32));
                    }
                }
            }
        }
    }
    reached
}

/// Closest indexed glyph to document point (x, y) within `maxd` (≤ one
/// cell), or -1. Used for touches, which land between glyphs as often as on
/// them.
pub fn nearest(x: f32, y: f32, maxd: f32) -> i32 {
    let g = G.get();
    let maxd = min(maxd, CELL);
    let (cx, cy) = (floor(x / CELL) as i32, floor(y / CELL) as i32);
    let mut best = (maxd * maxd, -1i32);
    for oy in -1..=1 {
        for ox in -1..=1 {
            let mut v = g.head[bucket(cx + ox, cy + oy)];
            while v != 0 {
                let vi = (v - 1) as usize;
                v = g.next[vi];
                let (dx, dy) = (g.x[vi] - x, g.y[vi] - y);
                let d2 = dx * dx + dy * dy;
                if d2 < best.0 {
                    best = (d2, vi as i32);
                }
            }
        }
    }
    best.1
}

pub fn clear() {
    let ev = E.get();
    if S.get().ready {
        for e in ev.arr.iter_mut() {
            e.fill(INF);
        }
    }
}

/// Earliest decode phase for glyph `i` across live events, and how hard the
/// arriving front should lift it. Phase < 0 = not reached yet / untouched.
#[inline]
pub fn sample(i: usize, t: f32) -> (f32, f32) {
    let ev = E.get();
    let mut phase = 2.0f32;
    let mut bob = 0.0f32;
    for e in 0..EVENTS {
        let a = ev.arr[e][i];
        if a >= INF {
            continue;
        }
        let local = t - a;
        if local > 1.5 {
            continue;
        }
        let q = ev.depth[e][i];
        let h = hash32(ev.seed[e] ^ (i as u32).wrapping_mul(0x2545_f491));
        // a hit lands as a jolt that dies in ~80 ms
        if (-0.02..0.08).contains(&local) {
            bob = max(bob, (1.0 - local / 0.08) * (1.0 - q));
        }
        if local < 0.0 {
            continue;
        }
        let dur = (0.34 + 0.3 * (1.0 - q)) * (0.6 + 0.8 * h01(h));
        let p = local / dur + q * 0.3;
        if p < phase {
            phase = p;
        }
    }
    (phase, bob)
}

