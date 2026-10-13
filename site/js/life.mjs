// life.mjs — objects (bordered tables, picture surfaces) that come alive near
// the pointer: a membrane that undulates (SVG turbulence → displacement, one
// filter per object), a slow squash-and-stretch breath, sudden fibrillation
// bursts, and droplets that bud on the lower edge, neck out and fall.
// Computed in core/src/life.rs; nothing is applied while nobody is near, so
// a still page carries no filter, no scale, no drops.

const TARGETS = 'table[border]:not([border="0"]), canvas[data-surface]';
const OUT = 6;
const DROP = 7;
const SVGNS = 'http://www.w3.org/2000/svg';

export class Life {
  constructor(k, busy) {
    this.k = k;
    this.busy = busy; // (el) => true while another system owns el's style
    this.objs = [...document.querySelectorAll(TARGETS)].slice(0, 16);
    this.out = k.f32(k.x.life_out(), 16 * OUT);
    this.drops = k.f32(k.x.life_drops(), 1 + 64 * DROP);
    this.applied = new Uint8Array(this.objs.length);
    this.ink = this.objs.map(() => '#506c84');
    this.pointer = { x: -1e9, y: -1e9 };

    const svg = document.createElementNS(SVGNS, 'svg');
    svg.setAttribute('width', '0');
    svg.setAttribute('height', '0');
    svg.setAttribute('aria-hidden', 'true');
    svg.style.position = 'absolute';
    this.filters = this.objs.map((_, i) => {
      const f = document.createElementNS(SVGNS, 'filter');
      f.id = `cell-${i}`;
      for (const [a, v] of [['x', '-8%'], ['y', '-8%'], ['width', '116%'], ['height', '116%'], ['color-interpolation-filters', 'sRGB']]) f.setAttribute(a, v);
      const turb = document.createElementNS(SVGNS, 'feTurbulence');
      turb.setAttribute('type', 'fractalNoise');
      turb.setAttribute('numOctaves', '2');
      turb.setAttribute('result', 'n');
      const disp = document.createElementNS(SVGNS, 'feDisplacementMap');
      disp.setAttribute('in', 'SourceGraphic');
      disp.setAttribute('in2', 'n');
      disp.setAttribute('xChannelSelector', 'R');
      disp.setAttribute('yChannelSelector', 'G');
      f.append(turb, disp);
      svg.append(f);
      return { turb, disp };
    });
    document.body.append(svg);

    this.c = Object.assign(document.createElement('canvas'), { id: 'drops', hidden: true });
    document.documentElement.append(this.c);
    this.ctx = this.c.getContext('2d');

    const track = (x, y) => {
      this.pointer.x = x + scrollX;
      this.pointer.y = y + scrollY;
    };
    addEventListener('pointermove', (e) => track(e.clientX, e.clientY), { passive: true });
    addEventListener('touchstart', (e) => track(e.touches[0].clientX, e.touches[0].clientY), { passive: true });
    addEventListener('touchmove', (e) => track(e.touches[0].clientX, e.touches[0].clientY), { passive: true });
    addEventListener('touchend', () => (this.pointer.x = this.pointer.y = -1e9), { passive: true });
    setInterval(() => this.sampleInk(), 2000);
    this.sampleInk();
  }

  /** Droplet colour = what the object's lower edge is made of. */
  sampleInk() {
    this.objs.forEach((el, i) => {
      try {
        if (el instanceof HTMLCanvasElement) {
          // copy one pixel into our own read-optimised canvas, don't read back the surface
          this.probe ??= document.createElement('canvas').getContext('2d', { willReadFrequently: true });
          this.probe.clearRect(0, 0, 1, 1);
          this.probe.drawImage(el, (el.width / 2) | 0, Math.max(0, el.height - 3), 1, 1, 0, 0, 1, 1);
          const d = this.probe.getImageData(0, 0, 1, 1).data;
          if (d[3] > 0) this.ink[i] = `rgb(${d[0]} ${d[1]} ${d[2]})`;
        } else {
          const cs = getComputedStyle(el);
          this.ink[i] = cs.borderBottomColor || '#506c84';
        }
      } catch {}
    });
  }

  step(dt, t, energy) {
    const x = this.k.x;
    const n = this.objs.length;
    this.objs.forEach((el, i) => {
      const r = el.getBoundingClientRect();
      x.life_set(i, r.left + scrollX, r.top + scrollY, r.width, r.height);
    });
    x.life_step(n, dt, t, this.pointer.x, this.pointer.y, energy, scrollY + innerHeight);

    const o = this.out;
    this.objs.forEach((el, i) => {
      if (this.busy(el)) {
        this.applied[i] = 0;
        return;
      }
      const b = i * OUT;
      const st = el.style;
      if (o[b] < 0.12) {
        if (this.applied[i]) {
          st.filter = st.scale = '';
          this.applied[i] = 0;
        }
        return;
      }
      const f = this.filters[i];
      f.disp.setAttribute('scale', o[b].toFixed(2));
      f.turb.setAttribute('baseFrequency', `${o[b + 1].toFixed(4)} ${o[b + 2].toFixed(4)}`);
      f.turb.setAttribute('seed', String(o[b + 3] | 0));
      st.filter = `url(#cell-${i})`;
      st.scale = `${o[b + 4].toFixed(4)} ${o[b + 5].toFixed(4)}`;
      this.applied[i] = 1;
    });

    this.drawDrops();
  }

  drawDrops() {
    const D = this.drops;
    const n = D[0];
    const c = this.c;
    if (!n) {
      if (!c.hidden) {
        this.ctx.setTransform(1, 0, 0, 1, 0, 0);
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
    for (let i = 0; i < n; i++) {
      const b = 1 + i * DROP;
      const x = D[b] - scrollX;
      const y = D[b + 1] - scrollY;
      const r = Math.max(0.5, D[b + 2]);
      const neck = D[b + 3];
      const stretch = D[b + 4];
      g.fillStyle = this.ink[D[b + 6] | 0];
      g.globalAlpha = 0.92;
      g.beginPath();
      if (D[b + 5]) {
        // still hanging: a neck from the edge down to the bulb
        const ey = y - r - neck;
        const w0 = r * 1.5;
        g.moveTo(x - w0, ey);
        g.bezierCurveTo(x - w0 * 0.3, ey + neck * 0.5, x - r, y - r * 0.7, x - r, y);
        g.arc(x, y, r, Math.PI, 0, true);
        g.bezierCurveTo(x + r, y - r * 0.7, x + w0 * 0.3, ey + neck * 0.5, x + w0, ey);
        g.closePath();
      } else {
        // falling: a teardrop pulled long by its speed
        const rx = r / Math.sqrt(stretch);
        const ry = r * stretch;
        g.moveTo(x, y - ry * 2.2);
        g.bezierCurveTo(x + rx * 0.4, y - ry * 1.2, x + rx, y - ry * 0.4, x + rx, y);
        g.arc(x, y, rx, 0, Math.PI, false);
        g.bezierCurveTo(x - rx, y - ry * 0.4, x - rx * 0.4, y - ry * 1.2, x, y - ry * 2.2);
      }
      g.fill();
      g.globalAlpha = 0.75;
      g.fillStyle = '#FFFFFF';
      g.beginPath();
      g.arc(x - r * 0.35, y - r * 0.35, r * 0.28, 0, Math.PI * 2);
      g.fill();
    }
    g.globalAlpha = 1;
  }

  calm() {
    this.k.x.life_reset();
    this.objs.forEach((el, i) => {
      if (this.applied[i] && !this.busy(el)) el.style.filter = el.style.scale = '';
      this.applied[i] = 0;
    });
    this.ctx.setTransform(1, 0, 0, 1, 0, 0);
    this.ctx.clearRect(0, 0, this.c.width, this.c.height);
    this.c.hidden = true;
  }
}
