// life.mjs — objects (bordered tables, picture surfaces) that come alive near
// the pointer: a membrane that undulates (SVG turbulence → displacement, one
// filter per object), a slow squash-and-stretch breath, corners that soften
// and round off, sudden fibrillation bursts, and clear glossy droplets that
// bud on the lower edge, neck out and fall.
// Computed in core/src/life.rs; nothing is applied while nobody is near, so
// a still page carries no filter, no scale, no drops.

const TARGETS = 'table[border]:not([border="0"]), canvas[data-surface]';
const OUT = 10;
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
    this.rects = [];
    this.dropsEnabled = true; // off once the WebGPU liquid takes over
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
  }

  step(dt, t, energy) {
    const x = this.k.x;
    const n = this.objs.length;
    this.objs.forEach((el, i) => {
      const r = el.getBoundingClientRect();
      this.rects[i] = [r.left + scrollX, r.top + scrollY, r.right + scrollX, r.bottom + scrollY];
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
          st.filter = st.scale = st.borderRadius = st.clipPath = '';
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
      // softened corners: radius draws the border round, the clip keeps cell
      // backgrounds from poking out past it
      const rr = [6, 7, 8, 9].map((k) => `${o[b + k].toFixed(1)}px`).join(' ');
      st.borderRadius = rr;
      st.clipPath = `inset(0 round ${rr})`;
      this.applied[i] = 1;
    });

    if (this.dropsEnabled) this.drawDrops();
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
      const r = Math.max(0.6, D[b + 2]);
      const neck = D[b + 3];
      const stretch = D[b + 4];
      const hanging = D[b + 5] > 0;
      const rx = hanging ? r : r / Math.sqrt(stretch);
      const ry = hanging ? r : r * stretch;

      g.beginPath();
      if (hanging) {
        // a neck from the edge down to the bulb
        const ey = y - r - neck;
        const w0 = r * 1.5;
        g.moveTo(x - w0, ey);
        g.bezierCurveTo(x - w0 * 0.3, ey + neck * 0.5, x - r, y - r * 0.7, x - r, y);
        g.arc(x, y, r, Math.PI, 0, true);
        g.bezierCurveTo(x + r, y - r * 0.7, x + w0 * 0.3, ey + neck * 0.5, x + w0, ey);
        g.closePath();
      } else {
        // falling: a teardrop pulled long by its speed
        g.moveTo(x, y - ry * 2.2);
        g.bezierCurveTo(x + rx * 0.4, y - ry * 1.2, x + rx, y - ry * 0.4, x + rx, y);
        g.arc(x, y, rx, 0, Math.PI, false);
        g.bezierCurveTo(x - rx, y - ry * 0.4, x - rx * 0.4, y - ry * 1.2, x, y - ry * 2.2);
      }

      // clear body: almost nothing in the middle, thicker (darker) at the rim
      const body = g.createRadialGradient(x - rx * 0.25, y - ry * 0.3, 0, x, y, Math.max(rx, ry) * 1.25);
      body.addColorStop(0, 'rgb(255 255 255 / .05)');
      body.addColorStop(0.62, 'rgb(170 200 225 / .10)');
      body.addColorStop(0.9, 'rgb(70 105 140 / .32)');
      body.addColorStop(1, 'rgb(40 70 100 / .45)');
      g.fillStyle = body;
      g.fill();
      g.lineWidth = 0.7;
      g.strokeStyle = 'rgb(30 55 85 / .5)';
      g.stroke();

      // caustic: light focused through the drop onto its lower inside edge
      g.beginPath();
      g.ellipse(x, y + ry * 0.12, rx * 0.72, ry * 0.72, 0, Math.PI * 0.18, Math.PI * 0.82);
      g.lineWidth = Math.max(0.8, r * 0.2);
      g.lineCap = 'round';
      g.strokeStyle = 'rgb(255 255 255 / .6)';
      g.stroke();

      // specular highlight + glint
      g.save();
      g.translate(x - rx * 0.36, y - ry * 0.4);
      g.rotate(-0.6);
      g.beginPath();
      g.ellipse(0, 0, rx * 0.3, ry * 0.17, 0, 0, Math.PI * 2);
      g.fillStyle = 'rgb(255 255 255 / .9)';
      g.fill();
      g.restore();
      g.beginPath();
      g.arc(x + rx * 0.3, y + ry * 0.05, Math.max(0.5, r * 0.08), 0, Math.PI * 2);
      g.fillStyle = 'rgb(255 255 255 / .8)';
      g.fill();
    }
    g.globalAlpha = 1;
  }

  /** Rectangles (document px) of the objects that are alive right now. */
  alive() {
    return this.rects.filter((_, i) => this.applied[i]);
  }

  calm() {
    this.k.x.life_reset();
    this.objs.forEach((el, i) => {
      if (this.applied[i] && !this.busy(el)) el.style.filter = el.style.scale = el.style.borderRadius = el.style.clipPath = '';
      this.applied[i] = 0;
    });
    this.ctx.setTransform(1, 0, 0, 1, 0, 0);
    this.ctx.clearRect(0, 0, this.c.width, this.c.height);
    this.c.hidden = true;
  }
}
