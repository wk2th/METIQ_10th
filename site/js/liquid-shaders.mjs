// liquid-shaders.mjs — the WGSL for the liquid, shared by the home page
// overlay (js/liquid.mjs) and the lab page (lab/liquid.html).
//
//   SIM    predict → grid (clear / count / scan / scatter) → density → relax
//          → finalize; viscoelastic particles (Clavet et al. 2005) with a GPU
//          counting-sort neighbour grid. See lab/liquid.html for the overview.
//   SPLAT  instanced gaussian quads → additive half-res density field.

export const STRIDE = 12; // floats per particle: pos, prev, vel, out, dens, vdel

export function shaders(MAX_OBS) {
  const COMMON = /* wgsl */ `
struct Particle { pos: vec2f, prev: vec2f, vel: vec2f, out: vec2f, dens: vec2f, vdel: vec2f };
struct Params {
  size: vec2f, h: f32, rho0: f32,
  k: f32, kn: f32, dt: f32, g: f32,
  visc: f32, n: u32, gridW: u32, gridH: u32,
  mouse: vec4f,
  mouseR: f32, mouseOn: f32, nObs: u32, cells: u32,
  adhesion: f32, _a: f32, _b: f32, _c: f32,
  obs: array<vec4f, ${MAX_OBS}>,
};
`;

  const SIM = /* wgsl */ `${COMMON}
@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var<storage, read_write> parts: array<Particle>;
@group(0) @binding(2) var<storage, read_write> cr: array<vec2u>;
@group(0) @binding(3) var<storage, read_write> counts: array<atomic<u32>>;
@group(0) @binding(4) var<storage, read_write> offsets: array<u32>;
@group(0) @binding(5) var<storage, read_write> sorted: array<u32>;

fn cellOf(p: vec2f) -> vec2i {
  return clamp(vec2i(floor(p / P.h)), vec2i(0), vec2i(i32(P.gridW) - 1, i32(P.gridH) - 1));
}
fn cellIdx(c: vec2i) -> u32 { return u32(c.y) * P.gridW + u32(c.x); }

@compute @workgroup_size(64) fn predict(@builtin(global_invocation_id) id: vec3u) {
  let i = id.x; if (i >= P.n) { return; }
  var p = parts[i];
  var v = p.vel + vec2f(0.0, P.g) * P.dt;
  if (P.mouseOn > 0.5) {
    let d = p.pos - P.mouse.xy;
    let r = length(d);
    if (r < P.mouseR) {
      let w = 1.0 - r / P.mouseR;
      v += (P.mouse.zw - v) * w * 0.2;
    }
  }
  p.prev = p.pos;
  p.pos += v * P.dt;
  p.vel = v;
  parts[i] = p;
}

@compute @workgroup_size(64) fn clearCounts(@builtin(global_invocation_id) id: vec3u) {
  if (id.x < P.cells) { atomicStore(&counts[id.x], 0u); }
}

@compute @workgroup_size(64) fn count(@builtin(global_invocation_id) id: vec3u) {
  let i = id.x; if (i >= P.n) { return; }
  let c = cellIdx(cellOf(parts[i].pos));
  cr[i] = vec2u(c, atomicAdd(&counts[c], 1u));
}

var<workgroup> wsum: array<u32, 256>;
@compute @workgroup_size(256) fn scan(@builtin(local_invocation_id) lid: vec3u) {
  let t = lid.x;
  let per = (P.cells + 255u) / 256u;
  let s0 = t * per;
  var acc = 0u;
  for (var k = 0u; k < per; k++) { let c = s0 + k; if (c < P.cells) { acc += atomicLoad(&counts[c]); } }
  wsum[t] = acc;
  workgroupBarrier();
  for (var off = 1u; off < 256u; off *= 2u) {
    var v = 0u;
    if (t >= off) { v = wsum[t - off]; }
    workgroupBarrier();
    wsum[t] += v;
    workgroupBarrier();
  }
  var run = 0u;
  if (t > 0u) { run = wsum[t - 1u]; }
  for (var k = 0u; k < per; k++) {
    let c = s0 + k;
    if (c < P.cells) { offsets[c] = run; run += atomicLoad(&counts[c]); }
  }
}

@compute @workgroup_size(64) fn scatter(@builtin(global_invocation_id) id: vec3u) {
  let i = id.x; if (i >= P.n) { return; }
  let c = cr[i];
  sorted[offsets[c.x] + c.y] = i;
}

@compute @workgroup_size(64) fn density(@builtin(global_invocation_id) id: vec3u) {
  let i = id.x; if (i >= P.n) { return; }
  let pi = parts[i].pos;
  let vi = parts[i].vel;
  let c = cellOf(pi);
  let h2 = P.h * P.h;
  var rho = 0.0; var rhon = 0.0; var vs = vec2f(0.0); var ws = 0.0;
  for (var dy = -1; dy <= 1; dy++) {
    for (var dx = -1; dx <= 1; dx++) {
      let cc = c + vec2i(dx, dy);
      if (cc.x < 0 || cc.y < 0 || cc.x >= i32(P.gridW) || cc.y >= i32(P.gridH)) { continue; }
      let ci = cellIdx(cc);
      let s0 = offsets[ci];
      let s1 = s0 + atomicLoad(&counts[ci]);
      for (var s = s0; s < s1; s++) {
        let j = sorted[s];
        let d = parts[j].pos - pi;
        let r2 = dot(d, d);
        if (r2 < h2) {
          let q = 1.0 - sqrt(r2) / P.h;
          rho += q * q;
          rhon += q * q * q;
          if (j != i) { vs += (parts[j].vel - vi) * q; ws += q; }
        }
      }
    }
  }
  parts[i].dens = vec2f(rho, rhon);
  parts[i].vdel = select(vec2f(0.0), vs / max(ws, 1e-4), ws > 0.0) * P.visc;
}

@compute @workgroup_size(64) fn relax(@builtin(global_invocation_id) id: vec3u) {
  let i = id.x; if (i >= P.n) { return; }
  let pi = parts[i].pos;
  // clamp the negative side: almost no cohesive pull, so water runs instead of beading
  let Pi = P.k * max(parts[i].dens.x - P.rho0, -0.04 * P.rho0);
  let Pni = P.kn * parts[i].dens.y;
  let c = cellOf(pi);
  let h2 = P.h * P.h;
  var disp = vec2f(0.0);
  for (var dy = -1; dy <= 1; dy++) {
    for (var dx = -1; dx <= 1; dx++) {
      let cc = c + vec2i(dx, dy);
      if (cc.x < 0 || cc.y < 0 || cc.x >= i32(P.gridW) || cc.y >= i32(P.gridH)) { continue; }
      let ci = cellIdx(cc);
      let s0 = offsets[ci];
      let s1 = s0 + atomicLoad(&counts[ci]);
      for (var s = s0; s < s1; s++) {
        let j = sorted[s];
        if (j == i) { continue; }
        let d = parts[j].pos - pi;
        let r2 = dot(d, d);
        if (r2 < h2 && r2 > 1e-6) {
          let r = sqrt(r2);
          let q = 1.0 - r / P.h;
          let Pj = P.k * max(parts[j].dens.x - P.rho0, -0.04 * P.rho0);
          let Pnj = P.kn * parts[j].dens.y;
          let D = (0.5 * (Pi + Pj) * q + 0.5 * (Pni + Pnj) * q * q) * P.h * 0.5;
          disp -= D * (d / r) * 0.5;
        }
      }
    }
  }
  let m = length(disp);
  parts[i].out = pi + select(disp, disp * (P.h * 0.15 / m), m > P.h * 0.15);
}

@compute @workgroup_size(64) fn finalize(@builtin(global_invocation_id) id: vec3u) {
  let i = id.x; if (i >= P.n) { return; }
  var p = parts[i];
  var x = p.out;
  let rad = P.h * 0.2;
  var touched = false;
  // walls
  let lo = vec2f(rad);
  let hi = P.size - vec2f(rad);
  if (x.x < lo.x || x.x > hi.x || x.y > hi.y) { touched = true; }
  x = vec2f(clamp(x.x, lo.x, hi.x), min(x.y, hi.y));
  // the page's tables are solid; liquid clings to them a little
  var adh = vec2f(0.0);
  for (var o = 0u; o < P.nObs; o++) {
    let b = P.obs[o];
    let q = clamp(x, b.xy, b.zw);
    let d = x - q;
    let dist = length(d);
    if (dist < rad) {
      if (dist > 1e-4) {
        x = q + d / dist * rad;
      } else {
        // inside: leave by the nearest face
        let dl = x.x - b.x; let dr = b.z - x.x; let dt = x.y - b.y; let db = b.w - x.y;
        let m = min(min(dl, dr), min(dt, db));
        if (m == dt) { x.y = b.y - rad; } else if (m == db) { x.y = b.w + rad; }
        else if (m == dl) { x.x = b.x - rad; } else { x.x = b.z + rad; }
      }
      touched = true;
    } else if (dist < P.h * 0.9) {
      adh -= d / dist * (1.0 - dist / (P.h * 0.9));
    }
  }
  var v = (x - p.prev) / P.dt + p.vdel + adh * P.adhesion * P.dt;
  if (touched) { v *= 0.996; }
  let sp = length(v);
  if (sp > 1600.0) { v *= 1600.0 / sp; }
  p.pos = x;
  p.vel = v;
  parts[i] = p;
}
`;

  const SPLAT = /* wgsl */ `${COMMON}
struct R { size: vec2f, radius: f32, scroll: f32 };
@group(0) @binding(0) var<uniform> U: R;
@group(0) @binding(1) var<storage, read> parts: array<Particle>;
struct VO { @builtin(position) pos: vec4f, @location(0) c: vec2f, @location(1) speed: f32 };
@vertex fn vs(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> VO {
  var corners = array<vec2f, 4>(vec2f(-1.0, -1.0), vec2f(1.0, -1.0), vec2f(-1.0, 1.0), vec2f(1.0, 1.0));
  let c = corners[vi];
  let w = parts[ii].pos + c * U.radius;
  var o: VO;
  o.pos = vec4f(w.x / U.size.x * 2.0 - 1.0, 1.0 - (w.y - U.scroll) / U.size.y * 2.0, 0.0, 1.0);
  o.c = c;
  o.speed = length(parts[ii].vel);
  return o;
}
@fragment fn fs(i: VO) -> @location(0) vec4f {
  let d2 = dot(i.c, i.c);
  if (d2 > 1.0) { discard; }
  let w = exp(-d2 * 3.0) - 0.0498;
  return vec4f(w, w * min(i.speed / 900.0, 1.0), 0.0, 0.0);
}
`;

  return { SIM, SPLAT };
}
