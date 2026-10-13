// touch.mjs — fingers. Phones have no hover, so every touch point does two
// things, independently per finger (multi-touch):
//
//   1. infects the nearest glyph within reach and lets the corruption spread
//      from there (dragging re-infects every couple of characters);
//   2. throws a burst of loose characters out from under the fingertip that
//      decode mojibake → Greek → maths as they fly, and dissolve.
//
// Touch events rather than pointer events: pointer streams are cancelled the
// moment the browser starts scrolling, touchmove keeps reporting every finger.

const MAX = 160;
const INK = ['#000080', '#800000', '#006600', '#000000', '#660099'];

export class TouchFx {
  constructor(k, field, now, reduced) {
    this.k = k;
    this.field = field;
    this.now = now;
    this.reduced = reduced;
    this.c = Object.assign(document.createElement('canvas'), { id: 'touchfx', hidden: true });
    document.documentElement.append(this.c);
    this.ctx = this.c.getContext('2d');
    this.p = [];
    this.trail = new Map(); // finger id → last burst point (client px)

    const opts = { passive: true };
    addEventListener('touchstart', (e) => this.touches(e, true), opts);
    addEventListener('touchmove', (e) => this.touches(e, false), opts);
    const end = (e) => {
      for (const tc of e.changedTouches) {
        this.trail.delete(tc.identifier);
        this.field.touchEnd(tc.identifier);
      }
    };
    addEventListener('touchend', end, opts);
    addEventListener('touchcancel', end, opts);
  }

  touches(e, down) {
    if (this.reduced()) return;
    for (const tc of e.changedTouches) {
      const { clientX: x, clientY: y, identifier: id } = tc;
      this.field.touchAt(id, x + scrollX, y + scrollY);
      const last = this.trail.get(id);
      if (down || !last || Math.hypot(x - last.x, y - last.y) > 28) {
        this.trail.set(id, { x, y });
        this.burst(x, y, down ? 10 : 3, down ? 1 : 0.55);
      }
    }
  }

  burst(x, y, n, power) {
    for (let i = 0; i < n && this.p.length < MAX; i++) {
      const a = Math.random() * Math.PI * 2;
      const v = (50 + Math.random() * 130) * power;
      this.p.push({
        x,
        y,
        vx: Math.cos(a) * v,
        vy: Math.sin(a) * v - 40 * power,
        age: 0,
        life: 0.7 + Math.random() * 0.5,
        size: 12 + Math.random() * 8,
        wide: Math.random() < 0.6 ? 1 : 0,
        ink: INK[(Math.random() * INK.length) | 0],
        seed: (Math.random() * 2 ** 32) >>> 0,
        rot: (Math.random() - 0.5) * 0.6,
      });
    }
  }

  step(dt) {
    const c = this.c;
    if (!this.p.length) {
      if (!c.hidden) {
        this.ctx.clearRect(0, 0, c.width, c.height);
        c.hidden = true;
      }
      return;
    }
    const dpr = Math.min(2, devicePixelRatio || 1);
    if (c.width !== Math.round(innerWidth * dpr) || c.height !== Math.round(innerHeight * dpr)) {
      c.width = Math.round(innerWidth * dpr);
      c.height = Math.round(innerHeight * dpr);
    }
    c.hidden = false;
    const g = this.ctx;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, innerWidth, innerHeight);
    g.textAlign = 'center';
    g.textBaseline = 'middle';
    const tick = (this.now() * 24) | 0;
    const drag = Math.exp(-dt * 2.2);

    this.p = this.p.filter((q) => {
      q.age += dt;
      if (q.age >= q.life) return false;
      q.vx *= drag;
      q.vy = q.vy * drag + 160 * dt;
      q.x += q.vx * dt;
      q.y += q.vy * dt;
      const k = q.age / q.life;
      // the decode schedule, compressed into the particle's life
      const stage = k < 0.35 ? 0 : k < 0.62 ? 1 : 2;
      const code = this.k.x.field_glyph(stage, q.wide, q.seed ^ Math.imul(tick, 0x2c1b3c6d));
      const alpha = k < 0.75 ? 1 : 1 - (k - 0.75) / 0.25;
      g.save();
      g.translate(q.x, q.y);
      g.rotate(q.rot * k * 3);
      g.font = `bold ${(q.size * (1 + 0.25 * k)).toFixed(1)}px "MS PGothic", "Hiragino Kaku Gothic ProN", sans-serif`;
      // a little chromatic split while it is still raw mojibake
      if (stage === 0) {
        g.globalAlpha = alpha * 0.55;
        g.fillStyle = '#FF0000';
        g.fillText(String.fromCodePoint(code), 1.5, 0);
        g.fillStyle = '#00FFFF';
        g.fillText(String.fromCodePoint(code), -1.5, 0);
      }
      g.globalAlpha = alpha;
      g.fillStyle = q.ink;
      g.fillText(String.fromCodePoint(code), 0, 0);
      g.restore();
      return true;
    });
  }

  calm() {
    this.p = [];
    this.ctx.setTransform(1, 0, 0, 1, 0, 0);
    this.ctx.clearRect(0, 0, this.c.width, this.c.height);
    this.c.hidden = true;
  }
}
