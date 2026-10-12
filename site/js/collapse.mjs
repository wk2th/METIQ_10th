// collapse.mjs — roughly once a minute, a bordered table on screen cracks,
// its cells fall away as mojibake, and it seeps back out from behind the page
// and re-decodes. The choreography is computed by core/src/collapse.rs; here
// we only pick the table, describe its cells, and paint the result.

const CANDIDATES = 'table[border]:not([border="0"])';
const OUT = 6;

const opaque = (c) => c && c !== 'transparent' && !/rgba\(.*,\s*0\)$/.test(c);

export class Collapse {
  constructor(k, field) {
    this.k = k;
    this.field = field;
    this.cap = k.x.shard_cap();
    this.inp = k.f32(k.x.shard_in(), this.cap * 4);
    this.out = k.f32(k.x.shard_out(), this.cap * OUT);
    this.cells = null;
    // first one comes a little early; open the page with #collapse to see it now
    this.next = location.hash === '#collapse' ? 3 : 25 + Math.random() * 15;
  }

  pick() {
    const vh = innerHeight;
    const seen = [...document.querySelectorAll(CANDIDATES)].filter((t) => {
      const r = t.getBoundingClientRect();
      return r.bottom > 0 && r.top < vh && t.querySelector('.g');
    });
    return seen[(Math.random() * seen.length) | 0];
  }

  maybeStart(t) {
    if (this.cells || t < this.next || document.hidden) return;
    this.next = t + 50 + Math.random() * 20;
    const table = this.pick();
    if (!table) return;

    const cells = [...table.querySelectorAll('td,th')].filter((c) => c.closest('table') === table).slice(0, this.cap);
    const sx = scrollX;
    const sy = scrollY;
    const tableBg = getComputedStyle(table).backgroundColor;
    cells.forEach((cell, s) => {
      const r = cell.getBoundingClientRect();
      const gs = cell.querySelectorAll('.g');
      const first = gs.length ? this.field.index.get(gs[0]) : 0;
      const last = gs.length ? this.field.index.get(gs[gs.length - 1]) + 1 : 0;
      this.inp.set([r.left + r.width / 2 + sx, r.top + r.height / 2 + sy, first, last], s * 4);
      // cells must carry their own paint once they leave the table
      const own = getComputedStyle(cell).backgroundColor;
      const row = getComputedStyle(cell.parentElement).backgroundColor;
      cell.style.backgroundColor = opaque(own) ? own : opaque(row) ? row : tableBg;
      cell.style.willChange = 'translate, rotate, scale, opacity, filter';
    });
    this.table = table;
    this.cells = cells;
    table.style.background = 'transparent';
    table.style.transition = 'border-color .25s';
    table.style.borderColor = 'transparent';
    this.k.x.shard_start(cells.length, t, (Math.random() * 2 ** 32) >>> 0);
  }

  step(t) {
    this.maybeStart(t);
    if (!this.cells) return;
    const stage = this.k.x.shard_step(t);
    if (stage === 0) return this.restore();
    if (stage === 2) this.table.style.borderColor = '';
    const o = this.out;
    this.cells.forEach((cell, s) => {
      const b = s * OUT;
      const st = cell.style;
      st.translate = `${o[b].toFixed(1)}px ${o[b + 1].toFixed(1)}px`;
      st.rotate = `${o[b + 2].toFixed(1)}deg`;
      st.scale = o[b + 3].toFixed(3);
      st.opacity = o[b + 4].toFixed(3);
      st.filter = o[b + 5] > 0.05 ? `blur(${o[b + 5].toFixed(1)}px)` : '';
    });
  }

  /** Put the table back exactly as authored. Synchronous. */
  restore() {
    if (!this.cells) return;
    for (const cell of this.cells) {
      const st = cell.style;
      st.translate = st.rotate = st.scale = st.opacity = st.filter = st.backgroundColor = st.willChange = '';
    }
    const ts = this.table.style;
    ts.background = ts.transition = ts.borderColor = '';
    this.cells = this.table = null;
  }

  cancel() {
    this.k.x.shard_cancel();
    this.restore();
  }
}
