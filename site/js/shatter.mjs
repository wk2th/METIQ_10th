// shatter.mjs — every ~10 s a table or picture on screen is struck: blue-white
// lightning runs along its cracks, it bursts into Voronoi shards that fall
// away, and then it re-forms in place with a soft glow while its text
// decodes. Geometry, bolts and motion come from core/src/shatter.rs; here we
// clone the object once per shard, clip each clone to its polygon, and paint.

const TARGETS = 'table[border]:not([border="0"]), canvas[data-surface]';
const MAXSH = 24;
const POLY = 35;
const BOLT = 19;

export class Shatter {
  constructor(k, field) {
    this.k = k;
    this.field = field;
    const x = k.x;
    this.poly = k.f32(x.shatter_poly(), MAXSH * POLY);
    this.out = k.f32(x.shatter_out(), MAXSH * 4);
    this.bolts = k.f32(x.shatter_bolts(), 1 + 100 * BOLT);
    this.fx = k.f32(x.shatter_fx(), 4);
    this.arc = Object.assign(document.createElement('canvas'), { id: 'arc', hidden: true });
    document.documentElement.append(this.arc);
    this.actx = this.arc.getContext('2d');
    this.el = null;
    // open the page with #shatter to see one right away
    this.next = location.hash === "#shatter" ? 2 : 6 + Math.random() * 4;
  }

  pick() {
    const vh = innerHeight;
    const seen = [...document.querySelectorAll(TARGETS)].filter((el) => {
      if (el === this.last) return false;
      const r = el.getBoundingClientRect();
      const vis = Math.min(r.bottom, vh) - Math.max(r.top, 0);
      return r.width * r.height > 1500 && vis > r.height * 0.7;
    });
    return seen[(Math.random() * seen.length) | 0];
  }

  start(t) {
    const el = this.pick();
    if (!el) return;
    const r = el.getBoundingClientRect();
    const gs = el.querySelectorAll('.g');
    const g0 = gs.length ? this.field.index.get(gs[0]) : 0;
    const g1 = gs.length ? this.field.index.get(gs[gs.length - 1]) + 1 : 0;
    const ix = r.width * (0.15 + 0.7 * Math.random());
    const iy = r.height * (0.15 + 0.7 * Math.random());
    this.n = this.k.x.shatter_plan(r.width, r.height, ix, iy, (Math.random() * 2 ** 32) >>> 0, g0, g1, t);
    this.el = this.last = el;
    this.box = { x: r.left + scrollX, y: r.top + scrollY, w: r.width, h: r.height };
    this.host = null;
  }

  /** Swap the object for one clipped clone per shard. */
  burst() {
    const { el, box, poly } = this;
    const host = document.createElement('div');
    Object.assign(host.style, {
      position: 'absolute',
      left: `${box.x}px`,
      top: `${box.y}px`,
      width: `${box.w}px`,
      height: `${box.h}px`,
      pointerEvents: 'none',
      zIndex: '2147483000',
    });
    this.shards = [];
    for (let i = 0; i < this.n; i++) {
      const b = i * POLY;
      const nv = poly[b];
      const pts = [];
      for (let v = 0; v < nv; v++) pts.push(`${poly[b + 3 + v * 2].toFixed(1)}px ${poly[b + 4 + v * 2].toFixed(1)}px`);
      const c = el.cloneNode(true);
      c.removeAttribute('id');
      c.querySelectorAll('[id]').forEach((e) => e.removeAttribute('id'));
      Object.assign(c.style, {
        position: 'absolute',
        left: '0',
        top: '0',
        margin: '0',
        width: `${box.w}px`,
        height: `${box.h}px`,
        clipPath: `polygon(${pts.join(',')})`,
        transformOrigin: `${poly[b + 1].toFixed(1)}px ${poly[b + 2].toFixed(1)}px`,
        filter: 'brightness(1.25)',
      });
      if (el instanceof HTMLCanvasElement) {
        c.width = el.width;
        c.height = el.height;
        c.getContext('2d').drawImage(el, 0, 0);
      }
      host.append(c);
      this.shards.push(c);
    }
    document.body.append(host);
    this.host = host;
    el.style.visibility = 'hidden';

    // sparks off every crack
    const bl = this.bolts;
    const ox = box.x - scrollX;
    const oy = box.y - scrollY;
    for (let i = 0; i < bl[0]; i++) {
      const b = 1 + i * BOLT;
      const mx = (bl[b + 1] + bl[b + 17]) / 2 + ox;
      const my = (bl[b + 2] + bl[b + 18]) / 2 + oy;
      this.k.x.spark_emit(mx, my, (Math.random() - 0.5) * 600, -200 - Math.random() * 300, 3, 5);
    }
  }

  drawBolts() {
    const n = this.bolts[0];
    const c = this.actx;
    if (!n && this.arc.hidden) return;
    if (this.arc.width !== innerWidth || this.arc.height !== innerHeight) {
      this.arc.width = innerWidth;
      this.arc.height = innerHeight;
    }
    c.clearRect(0, 0, this.arc.width, this.arc.height);
    this.arc.hidden = !n;
    if (!n) return;
    const ox = this.box.x - scrollX;
    const oy = this.box.y - scrollY;
    const bl = this.bolts;
    c.globalCompositeOperation = 'lighter';
    c.lineJoin = 'round';
    for (const [w, col] of [
      [6, 'rgb(70 140 255 / %)'],
      [2.5, 'rgb(150 215 255 / %)'],
      [1, 'rgb(255 255 255 / %)'],
    ]) {
      c.lineWidth = w;
      for (let i = 0; i < n; i++) {
        const b = 1 + i * BOLT;
        const a = bl[b] * (w > 4 ? 0.35 : 1);
        c.strokeStyle = col.replace('%', a.toFixed(3));
        c.beginPath();
        c.moveTo(bl[b + 1] + ox, bl[b + 2] + oy);
        for (let p = 1; p < 9; p++) c.lineTo(bl[b + 1 + p * 2] + ox, bl[b + 2 + p * 2] + oy);
        c.stroke();
      }
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
    const stage = this.k.x.shatter_step(t);
    const [flash, glow, k, blur] = this.fx;
    const st = this.el.style;
    this.drawBolts();

    if (stage === 1) {
      st.filter =
        flash > 0.05
          ? `brightness(${(1 + 0.7 * flash).toFixed(2)}) saturate(${(1 - 0.6 * flash).toFixed(2)}) drop-shadow(0 0 ${(2 + 6 * flash).toFixed(1)}px rgb(140 210 255 / ${(0.9 * flash).toFixed(2)}))`
          : '';
    } else if (stage >= 2) {
      if (!this.host) {
        st.filter = '';
        this.burst();
      }
      const o = this.out;
      this.shards.forEach((c, i) => {
        const s = c.style;
        s.translate = `${o[i * 4].toFixed(1)}px ${o[i * 4 + 1].toFixed(1)}px`;
        s.rotate = `${o[i * 4 + 2].toFixed(1)}deg`;
        s.opacity = o[i * 4 + 3].toFixed(3);
      });
      if (stage === 3) {
        st.visibility = '';
        st.opacity = Math.min(1, k * 1.6).toFixed(3);
        st.scale = (0.92 + 0.08 * k).toFixed(4);
        st.filter = `blur(${blur.toFixed(2)}px) brightness(${(1 + 0.45 * glow).toFixed(2)}) drop-shadow(0 0 ${(12 * glow).toFixed(1)}px rgb(235 250 255 / ${(0.95 * glow).toFixed(2)}))`;
      }
    } else {
      this.restore();
    }
  }

  /** Put everything back exactly as authored. Synchronous. */
  restore() {
    this.k.x.shatter_cancel();
    this.host?.remove();
    this.host = null;
    this.shards = null;
    if (this.el) {
      const st = this.el.style;
      st.visibility = st.opacity = st.scale = st.filter = '';
    }
    this.el = null;
    this.actx.clearRect(0, 0, this.arc.width, this.arc.height);
    this.arc.hidden = true;
  }
}
