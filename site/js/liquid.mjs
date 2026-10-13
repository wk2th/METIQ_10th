// liquid.mjs — real liquid on the home page, simulated on the GPU (WebGPU).
//
// A transparent canvas over the page. Water seeps out of objects that are
// alive (life.mjs: near the pointer) while the pointer is moving, and rain
// falls in showers while you are active; it runs over the page's real tables and pictures (measured from the
// live DOM), clings, drips and pools. After a few still seconds it evaporates,
// and the capture guard clears it at once — a still page is still 1996.
//
// Physics and splatting are shared with the lab page (js/liquid-shaders.mjs).
// The page behind the water is refracted from a snapshot of this very page
// (lab/page.jpg, tools/lab_snapshot.cjs) aligned to the live layout through
// the logo, which is enough because refraction only shifts a few pixels.

import { shaders, STRIDE } from './liquid-shaders.mjs';

const H = 12;
const SPACING = H * 0.45;
const SUBSTEPS = 3;
const DT = 1 / 180;
const MAX_OBS = 16;
const TARGETS = 'table[border]:not([border="0"]), canvas[data-surface]';
const IDLE = 2.5; // s of stillness before the water evaporates
const FADE = 1.5; // s it takes to go

const COMPOSITE = /* wgsl */ `
struct C { px: vec2f, fpx: vec2f, bgpx: vec2f, scroll: vec2f, thresh: f32, dpr: f32, fade: f32, _p: f32 };
@group(0) @binding(0) var samp: sampler;
@group(0) @binding(1) var field: texture_2d<f32>;
@group(0) @binding(2) var bg: texture_2d<f32>;
@group(0) @binding(3) var<uniform> U: C;
@vertex fn vs(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4f {
  var p = array<vec2f, 3>(vec2f(-1.0, -3.0), vec2f(3.0, 1.0), vec2f(-1.0, 1.0));
  return vec4f(p[vi], 0.0, 1.0);
}
fn F(uv: vec2f) -> vec2f { return textureSampleLevel(field, samp, uv, 0.0).rg; }
fn B(px: vec2f) -> vec3f {
  let uv = (px + U.scroll) * U.bgpx;
  return textureSampleLevel(bg, samp, 1.0 - abs(1.0 - abs(uv)), 0.0).rgb;
}
@fragment fn fs(@builtin(position) fc: vec4f) -> @location(0) vec4f {
  let uv = fc.xy * U.px;
  let fr = F(uv);
  let f = fr.x;
  let e = U.fpx * 1.5;
  let gx = F(uv + vec2f(e.x, 0.0)).x - F(uv - vec2f(e.x, 0.0)).x;
  let gy = F(uv + vec2f(0.0, e.y)).x - F(uv - vec2f(0.0, e.y)).x;
  let t = U.thresh;
  let sh = smoothstep(t * 0.6, t * 2.0, F(uv - vec2f(-6.0, -9.0) * U.px * U.dpr).x);
  let inside = smoothstep(t - 0.18, t + 0.18, f);
  var liq = vec3f(0.0);
  if (inside > 0.0) {
    let thick = clamp((f - t) / 4.0, 0.0, 1.0);
    let n = normalize(vec3f(-gx * 0.8, -gy * 0.8, 1.0));
    let off = n.xy * (10.0 + 22.0 * thick) * U.dpr;
    var base = B(fc.xy + off) * exp(-thick * vec3f(0.28, 0.10, 0.04));
    let fres = 0.03 + 0.97 * pow(1.0 - n.z, 5.0);
    let env = mix(vec3f(0.78, 0.84, 0.92), vec3f(1.0), clamp(-n.y * 0.5 + 0.5, 0.0, 1.0));
    liq = mix(base, env, clamp(fres * (0.3 + 0.7 * smoothstep(t, t + 1.5, f)), 0.0, 1.0));
    let L = normalize(vec3f(-0.45, -0.65, 0.62));
    let nh = max(dot(n, normalize(L + vec3f(0.0, 0.0, 1.0))), 0.0);
    liq += vec3f(pow(nh, 180.0) * 1.8 + pow(nh, 24.0) * 0.12);
    liq += vec3f(0.18) * clamp(dot(n.xy, vec2f(0.45, 0.65)), 0.0, 1.0) * (1.0 - thick);
    liq *= 1.0 - 0.16 * (1.0 - smoothstep(t, t + 0.35, f));
    liq = mix(liq, vec3f(1.0), clamp(fr.y / max(f, 1e-3), 0.0, 1.0) * 0.18);
  }
  // premultiplied: the water itself, plus its soft shadow on the page
  let shA = 0.22 * sh * (1.0 - inside);
  return vec4f(liq * inside, inside + shA) * U.fade;
}
`;

const asset = (f) => new URL(f, import.meta.url);

export class Liquid {
  /** Resolves to null where WebGPU is unavailable (the 2D drops stay). */
  static async create() {
    try {
      if (!navigator.gpu) return null;
      const adapter = await navigator.gpu.requestAdapter({ powerPreference: 'high-performance' });
      if (!adapter) return null;
      const device = await adapter.requestDevice();
      const [page, img, marble] = await Promise.all([
        fetch(asset('../lab/page.json')).then((r) => r.json()),
        fetch(asset('../lab/page.jpg')).then((r) => r.blob()).then(createImageBitmap),
        fetch(asset('../lab/marble.png')).then((r) => r.blob()).then(createImageBitmap),
      ]);
      return new Liquid(adapter, device, page, img, marble);
    } catch (e) {
      console.warn('[METIQ] liquid offline:', e);
      return null;
    }
  }

  constructor(adapter, device, page, img, marble) {
    this.adapter = adapter;
    this.device = device;
    this.page = page;
    this.img = img;
    this.marble = marble;
    this.ok = true;
    device.lost.then(() => {
      this.ok = false;
      this.canvas.hidden = true;
    });

    this.canvas = Object.assign(document.createElement('canvas'), { id: 'liquid', hidden: true });
    document.documentElement.append(this.canvas);
    this.ctx = this.canvas.getContext('webgpu');
    this.format = navigator.gpu.getPreferredCanvasFormat();

    const { SIM, SPLAT } = shaders(MAX_OBS);
    const simModule = device.createShaderModule({ code: SIM });
    this.simLayout = device.createBindGroupLayout({
      entries: [
        { binding: 0, visibility: GPUShaderStage.COMPUTE, buffer: { type: 'uniform' } },
        ...[1, 2, 3, 4, 5].map((b) => ({ binding: b, visibility: GPUShaderStage.COMPUTE, buffer: { type: 'storage' } })),
      ],
    });
    const pl = device.createPipelineLayout({ bindGroupLayouts: [this.simLayout] });
    this.K = Object.fromEntries(
      ['predict', 'clearCounts', 'count', 'scan', 'scatter', 'density', 'relax', 'finalize'].map((e) => [
        e,
        device.createComputePipeline({ layout: pl, compute: { module: simModule, entryPoint: e } }),
      ]),
    );
    const splatModule = device.createShaderModule({ code: SPLAT });
    this.splatLayout = device.createBindGroupLayout({
      entries: [
        { binding: 0, visibility: GPUShaderStage.VERTEX, buffer: { type: 'uniform' } },
        { binding: 1, visibility: GPUShaderStage.VERTEX, buffer: { type: 'read-only-storage' } },
      ],
    });
    const add = { srcFactor: 'one', dstFactor: 'one', operation: 'add' };
    this.splatPipe = device.createRenderPipeline({
      layout: device.createPipelineLayout({ bindGroupLayouts: [this.splatLayout] }),
      vertex: { module: splatModule, entryPoint: 'vs' },
      fragment: { module: splatModule, entryPoint: 'fs', targets: [{ format: 'rgba16float', blend: { color: add, alpha: add } }] },
      primitive: { topology: 'triangle-strip' },
    });
    const compModule = device.createShaderModule({ code: COMPOSITE });
    this.compPipe = device.createRenderPipeline({
      layout: 'auto',
      vertex: { module: compModule, entryPoint: 'vs' },
      fragment: { module: compModule, entryPoint: 'fs', targets: [{ format: this.format }] },
      primitive: { topology: 'triangle-list' },
    });
    this.sampler = device.createSampler({ magFilter: 'linear', minFilter: 'linear' });

    // rest density of a lattice at SPACING under the same kernel
    let r = 0;
    for (let y = -3; y <= 3; y++)
      for (let x = -3; x <= 3; x++) {
        const d = Math.hypot(x, y) * SPACING;
        if (d < H) r += (1 - d / H) ** 2;
      }
    this.rho0 = r;

    this.mouse = { x: 0, y: 0, vx: 0, vy: 0, t: 0, on: 0 };
    addEventListener(
      'pointermove',
      (e) => {
        const m = this.mouse;
        const dt = Math.max(1, e.timeStamp - m.t) / 1000;
        m.vx = (e.clientX - m.x) / dt;
        m.vy = (e.clientY - m.y) / dt;
        Object.assign(m, { x: e.clientX, y: e.clientY, t: e.timeStamp });
        m.on = Math.hypot(m.vx, m.vy) > 150 ? 1 : 0;
      },
      { passive: true },
    );
    let rt = 0;
    addEventListener('resize', () => {
      clearTimeout(rt);
      rt = setTimeout(() => (this.S = null), 200);
    });
    this.S = null;
    this.rain = { next: 0, until: 0, acc: 0, wind: 0 };
    this.lastActive = -1e9;
  }

  /** (Re)build everything for the current layout. */
  layout() {
    const { device } = this;
    const dpr = Math.min(2, devicePixelRatio || 1);
    const W = document.documentElement.clientWidth;
    const Hh = innerHeight;
    const PH = Math.max(document.documentElement.scrollHeight, Hh);
    this.canvas.width = Math.round(W * dpr);
    this.canvas.height = Math.round(Hh * dpr);
    this.canvas.style.width = `${W}px`;
    this.canvas.style.height = `${Hh}px`;
    this.ctx.configure({ device, format: this.format, alphaMode: 'premultiplied' });

    // align the snapshot to the live page through the logo
    const live = document.querySelector('canvas[data-surface="logo"]')?.getBoundingClientRect();
    const snap = this.page.objs.find((o) => o.s === 'logo')?.r;
    let k = 1;
    let ox = (W - this.page.width) / 2;
    let oy = 0;
    if (live && snap && live.width) {
      k = live.width / (snap[2] - snap[0]);
      ox = live.left + scrollX - snap[0] * k;
      oy = live.top + scrollY - snap[1] * k;
    }
    const bgW = Math.round(W * dpr);
    const bgH = Math.min(8192, Math.round(PH * dpr));
    const c = new OffscreenCanvas(bgW, bgH);
    const g = c.getContext('2d');
    g.scale(dpr, dpr);
    const pat = g.createPattern(this.marble, 'repeat');
    pat.setTransform(new DOMMatrix().translate(ox, oy).scale(k));
    g.fillStyle = pat;
    g.fillRect(0, 0, W, PH);
    g.drawImage(this.img, ox, oy, this.page.width * k, this.page.height * k);
    const bgTex = device.createTexture({
      size: [bgW, bgH],
      format: 'rgba8unorm',
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST | GPUTextureUsage.RENDER_ATTACHMENT,
    });
    device.queue.copyExternalImageToTexture({ source: c }, { texture: bgTex }, [bgW, bgH]);

    const n = Math.max(1500, Math.min(4000, Math.round((W * Hh) / 260)));
    const gridW = Math.ceil(W / H) + 1;
    const gridH = Math.ceil(PH / H) + 1;
    const cells = gridW * gridH;
    const mk = (size, usage) => device.createBuffer({ size, usage });
    const ST = GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST;
    const parts = mk(n * STRIDE * 4, ST);
    const params = mk(512, GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST);
    const simBG = device.createBindGroup({
      layout: this.simLayout,
      entries: [params, parts, mk(n * 8, ST), mk(cells * 4, ST), mk(cells * 4, ST), mk(n * 4, ST)].map((buffer, binding) => ({
        binding,
        resource: { buffer },
      })),
    });
    const FW = Math.ceil(this.canvas.width / 2);
    const FH = Math.ceil(this.canvas.height / 2);
    const field = device.createTexture({ size: [FW, FH], format: 'rgba16float', usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.TEXTURE_BINDING });
    const splatU = mk(16, GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST);
    const compU = mk(48, GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST);
    this.S = {
      W, Hh, PH, dpr, n, gridW, gridH, cells, parts, params, simBG, field, splatU, compU, bgW, bgH,
      splatBG: device.createBindGroup({ layout: this.splatLayout, entries: [{ binding: 0, resource: { buffer: splatU } }, { binding: 1, resource: { buffer: parts } }] }),
      compBG: device.createBindGroup({
        layout: this.compPipe.getBindGroupLayout(0),
        entries: [
          { binding: 0, resource: this.sampler },
          { binding: 1, resource: field.createView() },
          { binding: 2, resource: bgTex.createView() },
          { binding: 3, resource: { buffer: compU } },
        ],
      }),
      active: 0,
      next: 0,
      seep: 0,
    };
  }

  /** Colliders: the live page's objects, in document px. */
  obstacles() {
    return [...document.querySelectorAll(TARGETS)]
      .filter((e) => e.dataset.surface !== 'rainbow' && e.style.visibility !== 'hidden')
      .map((e) => {
        const r = e.getBoundingClientRect();
        return [r.left + scrollX, r.top + scrollY, r.right + scrollX, r.bottom + scrollY];
      })
      .filter((o) => o[2] - o[0] > 4)
      .slice(0, MAX_OBS);
  }

  emit(list) {
    const S = this.S;
    const count = Math.min(list.length, S.n);
    if (!count) return;
    const buf = new Float32Array(count * STRIDE);
    for (let k = 0; k < count; k++) {
      const [x, y, vx, vy] = list[k];
      const px = Math.min(S.W - 4, Math.max(4, x));
      const py = Math.min(S.PH - 4, Math.max(4, y));
      buf.set([px, py, px - vx * DT, py - vy * DT, vx, vy, px, py, 0, 0, 0, 0], k * STRIDE);
    }
    const first = Math.min(count, S.n - S.next);
    this.device.queue.writeBuffer(S.parts, S.next * STRIDE * 4, buf, 0, first * STRIDE);
    if (first < count) this.device.queue.writeBuffer(S.parts, 0, buf, first * STRIDE, (count - first) * STRIDE);
    S.next = (S.next + count) % S.n;
    S.active = Math.min(S.n, S.active + count);
  }

  /**
   * One frame. `alive` = rectangles (document px) of objects that are alive
   * right now; they seep. `energy` = the page's activity envelope.
   */
  step(dt, t, energy, alive) {
    if (!this.ok) return;
    if (!this.S) this.layout();
    const S = this.S;
    // only movement counts: a resting cursor must not keep the water coming
    if (energy > 0.02) this.lastActive = t;
    const idle = t - this.lastActive;

    if (idle < IDLE && dt < 0.25) {
      // seep from living objects
      S.seep += dt * 6 * alive.length;
      const list = [];
      while (S.seep >= 1 && alive.length) {
        S.seep -= 1;
        const [x0, y0, x1, y1] = alive[(Math.random() * alive.length) | 0];
        const u = Math.random();
        if (u < 0.75) list.push([x0 + 4 + Math.random() * (x1 - x0 - 8), y1 + 2, 0, 15]);
        else if (u < 0.875) list.push([x0 - 2, y0 + Math.random() * (y1 - y0), -10, 0]);
        else list.push([x1 + 2, y0 + Math.random() * (y1 - y0), 10, 0]);
      }
      // a shower now and then, only while someone is around
      const R = this.rain;
      if (energy > 0.25 && t > R.next && t > R.until) {
        R.until = t + 4 + Math.random() * 4;
        R.next = R.until + 15 + Math.random() * 20;
        R.wind = (Math.random() - 0.5) * 160;
      }
      if (t < R.until) {
        R.acc += dt * 32;
        while (R.acc >= 1) {
          R.acc -= 1;
          const x = Math.random() * S.W;
          const y = scrollY - 6 - Math.random() * 30;
          const v = 650 + Math.random() * 250;
          for (let k = 0; k < 3; k++) list.push([x - R.wind * k * 0.004, y - k * 4, R.wind, v]);
        }
      }
      this.emit(list);
    }

    let fade = 1;
    if (idle > IDLE) {
      fade = 1 - (idle - IDLE) / FADE;
      if (fade <= 0) return this.calm();
    }
    if (!S.active) {
      this.canvas.hidden = true;
      return;
    }
    this.canvas.hidden = false;

    const { device } = this;
    const obs = this.obstacles();
    const m = this.mouse;
    m.vx *= 0.85;
    m.vy *= 0.85;
    const f = new Float32Array(128);
    const u = new Uint32Array(f.buffer);
    f.set([S.W, S.PH, H, this.rho0, 0.18, 0.18, DT, 1400], 0);
    f[8] = 0.015;
    u[9] = S.active;
    u[10] = S.gridW;
    u[11] = S.gridH;
    f.set([m.x + scrollX, m.y + scrollY, m.vx, m.vy], 12);
    f[16] = 46;
    f[17] = m.on;
    u[18] = obs.length;
    u[19] = S.cells;
    f[20] = 250;
    obs.forEach((o, i) => f.set(o, 24 + i * 4));
    device.queue.writeBuffer(S.params, 0, f);
    device.queue.writeBuffer(S.splatU, 0, new Float32Array([S.W, S.Hh, H * 1.25, scrollY]));
    device.queue.writeBuffer(
      S.compU,
      0,
      new Float32Array([1 / this.canvas.width, 1 / this.canvas.height, 1 / S.field.width, 1 / S.field.height, 1 / S.bgW, 1 / S.bgH, scrollX * S.dpr, scrollY * S.dpr, 0.75, S.dpr, Math.max(0, fade), 0]),
    );

    const enc = device.createCommandEncoder();
    const cp = enc.beginComputePass();
    cp.setBindGroup(0, S.simBG);
    const pg = Math.ceil(S.active / 64);
    const cg = Math.ceil(S.cells / 64);
    for (let s = 0; s < SUBSTEPS; s++) {
      for (const [k, g] of [['predict', pg], ['clearCounts', cg], ['count', pg], ['scan', 1], ['scatter', pg], ['density', pg], ['relax', pg], ['finalize', pg]]) {
        cp.setPipeline(this.K[k]);
        cp.dispatchWorkgroups(g);
      }
    }
    cp.end();
    const sp = enc.beginRenderPass({ colorAttachments: [{ view: S.field.createView(), loadOp: 'clear', clearValue: [0, 0, 0, 0], storeOp: 'store' }] });
    sp.setPipeline(this.splatPipe);
    sp.setBindGroup(0, S.splatBG);
    sp.draw(4, S.active);
    sp.end();
    const rp = enc.beginRenderPass({ colorAttachments: [{ view: this.ctx.getCurrentTexture().createView(), loadOp: 'clear', clearValue: [0, 0, 0, 0], storeOp: 'store' }] });
    rp.setPipeline(this.compPipe);
    rp.setBindGroup(0, S.compBG);
    rp.draw(3);
    rp.end();
    device.queue.submit([enc.finish()]);
  }

  /** Synchronous: all water gone, canvas hidden. */
  calm() {
    if (this.S) {
      this.S.active = 0;
      this.S.next = 0;
      this.S.seep = 0;
    }
    this.rain.until = 0;
    this.lastActive = -1e9;
    this.canvas.hidden = true;
  }
}
