// boot.mjs — entry point. Wires the wasm kernel to the DOM and runs the one
// requestAnimationFrame loop everything hangs off.
//
//   pointer on a glyph ──► field_spread    (percolating decode: 縺→Σ→∑→字)
//   pointer ──► speed ──► fx_energy ──► E ─┬─► field_step   (glyph springs)
//                                          ├─► crt_render   (noise overlay)
//                                          ├─► *_render     (GIF surfaces)
//                                          └─► fx_glitch    (tear events)
//   ~10 s ──► destroy_*       (electric / rust / lightning / meteor / hack, re-form)
//   capture precursor ──► fx_calm + DOM reset, synchronously

import { boot } from './kernel.mjs';
import { GlyphField } from './glyphs.mjs';
import { Destroyer } from './destroy.mjs';
import { mountSurfaces, installBackground } from './surfaces.mjs';
import { Overlay } from './overlay.mjs';
import { armStealth } from './stealth.mjs';

const $ = (s) => document.querySelector(s);
const reduced = matchMedia('(prefers-reduced-motion: reduce)');

async function visitorCount() {
  // A real hit counter needs a server. Point data-endpoint at anything that
  // returns {"count": n} (and increments it) and that number is used instead.
  const el = $('#counter');
  const ep = el?.dataset.endpoint;
  if (ep) {
    try {
      const r = await fetch(ep, { method: 'POST' });
      const { count } = await r.json();
      if (Number.isFinite(count)) return count;
    } catch {}
  }
  let n = 0;
  try {
    n = (Number(localStorage.getItem('metiq.hits')) || 0) + 1;
    localStorage.setItem('metiq.hits', String(n));
  } catch {}
  return Number(el?.dataset.base ?? 0) + n;
}

function isKiriban(n) {
  // キリ番: ゾロ目 (11111) or a round/lucky tail (…000, …777)
  const s = String(n);
  return n >= 100 && (/^(\d)\1+$/.test(s) || /(000|777)$/.test(s));
}

async function main() {
  const k = await boot();
  const x = k.x;
  document.documentElement.classList.add('wasm');

  installBackground(k);
  const field = new GlyphField(k, document.body);
  const destroyer = new Destroyer(k, field);
  const { list: surfaces, byName } = mountSurfaces(k);
  const overlay = new Overlay(k);

  // Counter: roll up into place like the drum is catching up with history.
  const count = await visitorCount();
  x.odo_snap(Math.max(0, count - 48));
  x.odo_target(count);
  if (isKiriban(count)) $('#kiriban')?.removeAttribute('hidden');
  for (const s of byName.counter ?? []) s.canvas.title = `${count} hits`;

  const glitched = [];
  let energy = 0;
  const ptr = { cx: 0, cy: 0, t: 0, speed: 0, kick: 0, drag: false };

  const calm = () => {
    x.fx_calm(2.5);
    destroyer.restore();
    field.calm();
    overlay.calm();
    document.body.classList.remove('glitch');
    for (const el of glitched) el.style.translate = '';
    glitched.length = 0;
    ptr.speed = ptr.kick = 0;
  };
  armStealth(calm);

  addEventListener(
    'pointermove',
    (e) => {
      const now = e.timeStamp;
      const dt = Math.max(1, now - ptr.t) / 1000;
      const vx = (e.clientX - ptr.cx) / dt;
      const vy = (e.clientY - ptr.cy) / dt;
      if (ptr.t) ptr.speed = Math.max(ptr.speed, Math.min(4000, Math.hypot(vx, vy)));
      ptr.t = now;
      ptr.cx = e.clientX;
      ptr.cy = e.clientY;
      if (!reduced.matches && e.target.classList?.contains('g') && field.touch(e.target)) ptr.kick += 0.05;
      if (energy > 0.15) x.spark_emit(e.clientX, e.clientY, vx, vy, energy > 0.5 ? 2 : 1, 255);
      if (ptr.drag && e.target.dataset?.surface === 'globe') x.globe_push(e.movementX * 0.08);
    },
    { passive: true },
  );
  addEventListener('pointerover', (e) => {
    if (e.target.closest?.('a[href],button,canvas,[data-fx]')) ptr.kick += 0.18;
  });
  addEventListener('pointerdown', (e) => {
    ptr.kick += 0.3;
    ptr.drag = true;
    if (e.target.dataset?.surface === 'counter') x.odo_kick(30);
    if (e.target.dataset?.surface === 'logo') x.logo_kick(9);
  });
  addEventListener('pointerup', () => (ptr.drag = false));
  addEventListener('scroll', () => (ptr.speed = Math.max(ptr.speed, 600)), { passive: true });

  let raf = 0;
  const remeasure = () => {
    cancelAnimationFrame(raf);
    raf = requestAnimationFrame(() => field.measure());
  };
  addEventListener('resize', remeasure);
  document.fonts?.ready.then(remeasure);
  addEventListener('load', remeasure);

  // Glitch: briefly shear a couple of the page's tables sideways.
  const blocks = [...document.querySelectorAll('body > center > table, body > center > hr, body > center > canvas')];
  let wasGlitch = 0;
  let last = performance.now();
  let t = 0;

  const frame = (now) => {
    const dt = Math.min(0.05, (now - last) / 1000);
    last = now;
    t += dt;

    const drive = reduced.matches ? 0 : ptr.speed;
    energy = x.fx_energy(dt, drive, reduced.matches ? 0 : ptr.kick);
    ptr.kick = 0;
    ptr.speed *= Math.exp(-dt * 10);

    const glitch = x.fx_glitch(dt);
    if (glitch && !wasGlitch) {
      document.body.classList.add('glitch');
      for (let i = 0; i < 2; i++) {
        const el = blocks[(Math.random() * blocks.length) | 0];
        el.style.translate = `${(Math.random() < 0.5 ? -1 : 1) * (4 + Math.random() * 12)}px 0`;
        glitched.push(el);
      }
    } else if (!glitch && wasGlitch) {
      document.body.classList.remove('glitch');
      for (const el of glitched) el.style.translate = '';
      glitched.length = 0;
    }
    wasGlitch = glitch;

    if (!reduced.matches) destroyer.step(t);
    field.step(dt, t, energy);
    overlay.frame(dt, t, energy, glitch);
    for (const s of surfaces) s.tick(dt, t, energy);

    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);

  const lm = $('#lastmod');
  if (lm) lm.textContent = new Date(document.lastModified).toLocaleDateString('ja-JP');

  console.log(
    '%c METIQ %c rendering kernel 0x%s · %d glyphs · simd128 ',
    'background:#000080;color:#ff0;font:bold 14px serif',
    'background:#c0c0c0;color:#000',
    x.metiq_build().toString(16),
    field.n,
  );
}

main().catch((e) => {
  // Nothing breaks: without the kernel this is simply a static 1996 page.
  console.warn('[METIQ] kernel offline:', e);
});
