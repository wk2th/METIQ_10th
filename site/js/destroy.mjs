// destroy.mjs — every ~10 s one table or picture on screen is destroyed and
// re-formed. Five ways, picked at random (never the same twice running):
//
//   electric   blue-white arcs crawl the cracks, it chars black, bursts
//   rust       rust blooms and spreads, the text rots, it crumbles away
//   lightning  a bolt comes down from the top of the screen, it chars, blows apart
//   meteor     a fireball streaks in, shockwave, shake, it burns, explodes
//
// Pieces never fade: they keep charring / rusting on the way down and are
// removed only once they have left the viewport.
//   hack       green scanlines, text → hex, a terminal deletes it strip by strip
//
// All geometry, motion and skins come from core/src/destroy.rs. Here we pick
// the victim, clone it once per piece (each clone clipped to its polygon and
// carrying a snapshot of the skin), and paint bolts / fireball / shockwave on
// a viewport canvas. Open the page with #shatter (random) or #electric,
// #rust, #lightning, #meteor, #hack to trigger one two seconds after load.

const TARGETS = 'table[border]:not([border="0"]), canvas[data-surface]';
const MODES = ['electric', 'rust', 'lightning', 'meteor', 'hack'];
const MAXP = 160;
const POLY = 34;
const OUT = 7;
const LINE = 69;
const DEBRIS = [5, 9, 7, 7, 11]; // spark colour per mode when a piece lets go

export class Destroyer {
  constructor(k, field) {
    this.k = k;
    this.field = field;
    const x = k.x;
    this.polys = k.f32(x.destroy_polys(), MAXP * POLY);
    this.out = k.f32(x.destroy_out(), MAXP * OUT);
    this.lines = k.f32(x.destroy_lines(), 1 + 160 * LINE);
    this.fx = k.f32(x.destroy_fx(), 16);

    this.arc = Object.assign(document.createElement('canvas'), { id: 'arc', hidden: true });
    this.flash = Object.assign(document.createElement('div'), { id: 'flash', hidden: true });
    document.documentElement.append(this.arc, this.flash);
    this.actx = this.arc.getContext('2d');

    const hash = location.hash.slice(1);
    this.forced = MODES.indexOf(hash);
    this.next = this.forced >= 0 || hash === 'shatter' ? 2 : 6 + Math.random() * 4;
    this.el = null;
    this.lastMode = -1;
  }

  pick() {
    const vh = innerHeight;
    const seen = [...document.querySelectorAll(TARGETS)].filter((el) => {
      if (el === this.lastEl) return false;
      const r = el.getBoundingClientRect();
      const vis = Math.min(r.bottom, vh) - Math.max(r.top, 0);
      return r.width * r.height > 1500 && vis > r.height * 0.7;
    });
    return seen[(Math.random() * seen.length) | 0];
  }

  start(t) {
    const el = this.pick();
    if (!el) return;
    let mode = this.forced;
    this.forced = -1;
    if (mode < 0) do mode = (Math.random() * MODES.length) | 0; while (mode === this.lastMode);
    this.mode = this.lastMode = mode;

    // Everything below lives in the element's own CSS px ("local"). When the
    // page is shown scaled (CSS zoom on <body>, e.g. on a phone), local px ×
    // z = document px; vx()/vy() take local points to the viewport.
    const z = parseFloat(document.body.style.zoom) || 1;
    this.z = z;
    const r = el.getBoundingClientRect();
    const gs = el.querySelectorAll('.g');
    const g0 = gs.length ? this.field.index.get(gs[0]) : 0;
    const g1 = gs.length ? this.field.index.get(gs[gs.length - 1]) + 1 : 0;
    const box = { x: (r.left + scrollX) / z, y: (r.top + scrollY) / z, w: r.width / z, h: r.height / z };
    const ix = box.w * (0.2 + 0.6 * Math.random());
    const iy = box.h * (0.2 + 0.6 * Math.random());
    this.n = this.k.x.destroy_plan(mode, box.w, box.h, ix, iy, r.top / z, r.left + scrollX, r.top + scrollY, (Math.random() * 2 ** 32) >>> 0, g0, g1, t, z, innerHeight / z);
    this.el = this.lastEl = el;
    this.box = box;
    this.impact = { x: ix, y: iy };
    this.pieces = null;
    this.released = new Uint8Array(MAXP);

    // overlay host: skin (+ terminal) now, pieces later
    const host = document.createElement('div');
    Object.assign(host.style, {
      position: 'absolute',
      left: `${box.x}px`,
      top: `${box.y}px`,
      width: `${box.w}px`,
      height: `${box.h}px`,
      pointerEvents: 'none',
      zIndex: '2147483000',
      perspective: '900px',
    });
    this.host = host;
    this.scale = Math.max(2, Math.ceil(Math.max(box.w / 400, box.h / 240)));
    this.skin = null;
    {
      const sk = document.createElement('canvas');
      sk.width = this.k.x.destroy_skin_w(this.scale);
      sk.height = this.k.x.destroy_skin_h(this.scale);
      Object.assign(sk.style, {
        position: 'absolute',
        left: '0',
        top: '0',
        width: `${sk.width * this.scale}px`,
        height: `${sk.height * this.scale}px`,
        imageRendering: 'pixelated',
      });
      this.skin = sk;
      this.sctx = sk.getContext('2d');
      this.skinImg = null;
      host.append(sk);
    }
    this.term = null;
    if (MODES[mode] === 'hack') {
      const tc = document.createElement('canvas');
      tc.width = Math.ceil(box.w);
      tc.height = Math.ceil(box.h);
      Object.assign(tc.style, { position: 'absolute', left: '0', top: '0' });
      this.term = tc;
      this.target = el.dataset.surface ?? `table${[...document.querySelectorAll('table')].indexOf(el)}`;
      host.append(tc);
    }
    document.body.append(host);
  }

  vx(lx) {
    return (this.box.x + lx) * this.z - scrollX;
  }

  vy(ly) {
    return (this.box.y + ly) * this.z - scrollY;
  }

  /** Swap the element for one clipped clone per piece. */
  burst() {
    const { el, box, polys, host } = this;
    this.pieces = [];
    for (let i = 0; i < this.n; i++) {
      const b = i * POLY;
      const nv = polys[b];
      const pts = [];
      for (let v = 0; v < nv; v++) pts.push(`${polys[b + 6 + v * 2].toFixed(1)}px ${polys[b + 7 + v * 2].toFixed(1)}px`);
      const wrap = document.createElement('div');
      Object.assign(wrap.style, {
        position: 'absolute',
        left: '0',
        top: '0',
        width: `${box.w}px`,
        height: `${box.h}px`,
        clipPath: `polygon(${pts.join(',')})`,
        transformOrigin: `${polys[b + 1].toFixed(1)}px ${polys[b + 2].toFixed(1)}px`,
        willChange: 'transform, opacity, filter',
      });
      const c = el.cloneNode(true);
      c.removeAttribute('id');
      c.querySelectorAll('[id]').forEach((e) => e.removeAttribute('id'));
      Object.assign(c.style, { position: 'absolute', left: '0', top: '0', margin: '0', width: `${box.w}px`, height: `${box.h}px` });
      if (el instanceof HTMLCanvasElement) {
        c.width = el.width;
        c.height = el.height;
        c.getContext('2d').drawImage(el, 0, 0);
      }
      wrap.append(c);
      for (const layer of [this.skin, this.term]) {
        if (!layer) continue;
        const copy = layer.cloneNode();
        copy.getContext('2d').drawImage(layer, 0, 0);
        wrap.append(copy);
      }
      host.append(wrap);
      this.pieces.push(wrap);
    }
    this.skin?.remove();
    this.term?.remove();
    this.skin = this.term = null;
    el.style.visibility = 'hidden';
    el.style.filter = '';
  }

  drawTerminal(p) {
    const c = this.term.getContext('2d');
    const { width: w, height: h } = this.term;
    c.clearRect(0, 0, w, h);
    const fs = h < 60 || w < 160 ? 9 : 11;
    c.font = `bold ${fs}px "Courier New", monospace`;
    const pct = Math.min(100, Math.floor(Math.max(0, p - 0.45) / 0.55 * 100));
    const bar = '█'.repeat(Math.round(pct / 10)) + '░'.repeat(10 - Math.round(pct / 10));
    const lines = [
      'C:\\> telnet 203.0.113.66 31337',
      'Connected. ROOT ACCESS GRANTED',
      `> del /f /s /q ${this.target}`,
      `DELETING [${bar}] ${pct}%`,
    ];
    let budget = Math.floor(p * 120);
    const shown = [];
    for (const l of lines) {
      if (budget <= 0) break;
      shown.push(l.slice(0, budget));
      budget -= l.length;
    }
    const lh = fs + 2;
    const bw = Math.min(w, Math.max(...shown.map((l) => c.measureText(l).width), 0) + 8);
    c.fillStyle = 'rgb(0 0 0 / .78)';
    c.fillRect(0, 0, bw, shown.length * lh + 4);
    c.fillStyle = '#00FF66';
    shown.forEach((l, i) => c.fillText(l, 4, (i + 1) * lh));
    if (((p * 20) | 0) % 2) c.fillRect(4 + c.measureText(shown.at(-1) ?? '').width, shown.length * lh - fs + 2, fs * 0.6, fs);
  }

  paintOverlay() {
    const n = this.lines[0];
    const fx = this.fx;
    const meteor = fx[10] > 0;
    const ring = fx[12] > 0.01;
    const c = this.actx;
    if (!n && !meteor && !ring && this.arc.hidden) return;
    if (this.arc.width !== innerWidth || this.arc.height !== innerHeight) {
      this.arc.width = innerWidth;
      this.arc.height = innerHeight;
    }
    c.clearRect(0, 0, this.arc.width, this.arc.height);
    this.arc.hidden = !n && !meteor && !ring;
    const z = this.z;
    c.globalCompositeOperation = 'lighter';
    c.lineJoin = c.lineCap = 'round';

    // bolts: style 0 = crack arcs, 1 = sky bolt, 2 = sky bolt branch
    const L = this.lines;
    const STYLES = [
      [[6, 'rgb(70 140 255 / %)', 0.35], [2.5, 'rgb(150 215 255 / %)', 1], [1, 'rgb(255 255 255 / %)', 1]],
      [[18, 'rgb(130 120 255 / %)', 0.25], [7, 'rgb(190 200 255 / %)', 0.7], [2.5, 'rgb(255 255 255 / %)', 1]],
      [[8, 'rgb(130 120 255 / %)', 0.2], [3, 'rgb(200 210 255 / %)', 0.7], [1.2, 'rgb(255 255 255 / %)', 1]],
    ];
    for (let pass = 0; pass < 3; pass++) {
      for (let i = 0; i < n; i++) {
        const b = 1 + i * LINE;
        const [w, col, k] = STYLES[L[b]][pass];
        c.lineWidth = w * Math.max(z, 0.6);
        c.strokeStyle = col.replace('%', (L[b + 1] * k).toFixed(3));
        c.beginPath();
        c.moveTo(this.vx(L[b + 3]), this.vy(L[b + 4]));
        for (let p = 1; p < L[b + 2]; p++) c.lineTo(this.vx(L[b + 3 + p * 2]), this.vy(L[b + 4 + p * 2]));
        c.stroke();
      }
    }

    if (meteor) {
      const mx = this.vx(fx[8]);
      const my = this.vy(fx[9]);
      const r = fx[10] * z;
      const g = c.createRadialGradient(mx, my, 0, mx, my, r * 3.2);
      g.addColorStop(0, 'rgb(255 255 230 / 1)');
      g.addColorStop(0.25, 'rgb(255 210 80 / .95)');
      g.addColorStop(0.55, 'rgb(255 90 0 / .55)');
      g.addColorStop(1, 'rgb(255 40 0 / 0)');
      c.fillStyle = g;
      c.beginPath();
      c.arc(mx, my, r * 3.2, 0, Math.PI * 2);
      c.fill();
      const vx = mx - (this.lastMeteor?.x ?? mx);
      const vy = my - (this.lastMeteor?.y ?? my);
      this.k.x.spark_emit(mx, my, -vx * 8, -vy * 8, 4, 7);
      this.k.x.spark_emit(mx, my, -vx * 4, -vy * 4, 1, 10);
      this.lastMeteor = { x: mx, y: my };
    }

    if (ring) {
      const cx = this.vx(this.impact.x);
      const cy = this.vy(this.impact.y);
      c.lineWidth = (this.mode === 3 ? 6 : 2) * z;
      c.strokeStyle = this.mode === 3 ? `rgb(255 220 160 / ${fx[12].toFixed(3)})` : `rgb(200 220 255 / ${fx[12].toFixed(3)})`;
      c.beginPath();
      c.ellipse(cx, cy, fx[11] * z, fx[11] * z * 0.55, 0, 0, Math.PI * 2);
      c.stroke();
    }
    c.globalCompositeOperation = 'source-over';
  }

  step(t) {
    if (!this.el) {
      if (t >= this.next && !document.hidden) {
        this.next = t + 8 + Math.random() * 5;
        this.start(t);
      }
      if (!this.el) return;
    }
    const x = this.k.x;
    const stage = x.destroy_step(t);
    if (stage === 0) return this.restore();
    const fx = this.fx;
    const st = this.el.style;

    // screen-level: white-out, shake
    this.flash.hidden = fx[4] < 0.01;
    this.flash.style.opacity = fx[4].toFixed(3);
    document.body.style.translate = fx[5] || fx[6] ? `${fx[5].toFixed(1)}px ${fx[6].toFixed(1)}px` : '';
    this.paintOverlay();

    // skin + terminal ride on the intact element until it breaks
    if (this.skin) {
      const ptr = x.destroy_skin(this.scale);
      if (ptr) {
        this.skinImg ??= this.k.pixels(ptr, this.skin.width, this.skin.height);
        this.sctx.putImageData(this.skinImg, 0, 0);
      }
      this.skin.style.opacity = fx[7].toFixed(2);
    }
    if (this.term) this.drawTerminal(fx[13]);

    if (stage === 1) {
      const f = fx[0];
      st.filter =
        f > 0.05
          ? this.mode === 2
            ? `brightness(${(1 + 1.2 * f).toFixed(2)}) contrast(${(1 + 0.5 * f).toFixed(2)})`
            : `brightness(${(1 + 0.7 * f).toFixed(2)}) saturate(${(1 - 0.6 * f).toFixed(2)}) drop-shadow(0 0 ${(2 + 6 * f).toFixed(1)}px rgb(140 210 255 / ${(0.9 * f).toFixed(2)}))`
          : '';
      return;
    }

    if (!this.pieces) this.burst();
    const o = this.out;
    const pl = this.polys;
    const hack = this.mode === 4;
    const rust = this.mode === 1;
    this.pieces.forEach((wrap, i) => {
      if (!wrap) return;
      const b = i * OUT;
      const pb = i * POLY;
      if (o[b + 6] === 2) {
        // fell (or slid) out of view: drop it, no fading
        wrap.remove();
        this.pieces[i] = null;
        return;
      }
      const s = wrap.style;
      if (hack) {
        s.transform = `translate(${o[b].toFixed(1)}px, ${o[b + 1].toFixed(1)}px)`;
      } else {
        s.transform = `translate3d(${o[b].toFixed(1)}px, ${o[b + 1].toFixed(1)}px, ${o[b + 2].toFixed(1)}px) rotate3d(${pl[pb + 3].toFixed(3)}, ${pl[pb + 4].toFixed(3)}, ${pl[pb + 5].toFixed(3)}, ${o[b + 3].toFixed(1)}deg)`;
      }
      // pieces keep rotting / charring as they fall
      const d = o[b + 4];
      const br = o[b + 5];
      s.filter = hack
        ? 'drop-shadow(0 0 2px #00ff66) brightness(1.2)'
        : rust
          ? `sepia(${(0.75 * d).toFixed(2)}) saturate(${(1 + 0.9 * d).toFixed(2)}) hue-rotate(${(-14 * d).toFixed(1)}deg) brightness(${(br * (1 - 0.4 * d)).toFixed(2)})`
          : `brightness(${(br * (1 - 0.66 * d)).toFixed(2)}) contrast(${(1 + 0.3 * d).toFixed(2)}) sepia(${(0.4 * d).toFixed(2)})`;
      if (o[b + 6] && !this.released[i]) {
        this.released[i] = 1;
        const cx = this.vx(pl[pb + 1]);
        const cy = this.vy(pl[pb + 2]);
        x.spark_emit(cx, cy, (Math.random() - 0.5) * 300, -100 - Math.random() * 200, this.mode === 1 ? 4 : 2, DEBRIS[this.mode]);
        if (this.mode === 3 || this.mode === 2) x.spark_emit(cx, cy, 0, -60, 1, 10);
      }
    });

    if (stage === 4) {
      st.visibility = st.opacity = st.scale = st.filter = '';
    } else if (stage === 3) {
      st.visibility = '';
      st.opacity = Math.min(1, fx[2] * 1.6).toFixed(3);
      st.scale = (0.92 + 0.08 * fx[2]).toFixed(4);
      st.filter = `blur(${fx[3].toFixed(2)}px) brightness(${(1 + 0.45 * fx[1]).toFixed(2)}) drop-shadow(0 0 ${(12 * fx[1]).toFixed(1)}px rgb(235 250 255 / ${(0.95 * fx[1]).toFixed(2)}))`;
    }
  }

  /** Put everything back exactly as authored. Synchronous. */
  restore() {
    this.k.x.destroy_cancel();
    this.host?.remove();
    this.host = this.pieces = this.skin = this.term = null;
    if (this.el) {
      const st = this.el.style;
      st.visibility = st.opacity = st.scale = st.filter = '';
    }
    this.el = null;
    document.body.style.translate = '';
    this.flash.hidden = true;
    this.actx.clearRect(0, 0, this.arc.width, this.arc.height);
    this.arc.hidden = true;
    this.lastMeteor = null;
  }
}
