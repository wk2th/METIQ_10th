// glyphs.mjs — splits the document's text into per-grapheme inline-blocks and
// drives them from the wasm spring field.
//
// Layout rules we have to respect so a still frame is pixel-identical to the
// unsplit page:
//   * graphemes, not code units (Intl.Segmenter) — ★ and 〜 survive intact
//   * Latin runs are wrapped in a nowrap span so words never break mid-word
//     (inline-blocks are each a break opportunity); CJK keeps per-glyph breaks
//   * text-decoration does not propagate into atomic inlines, so links get
//     their underline re-applied per glyph in CSS
//   * we only ever write the individual `translate` / `rotate` properties and
//     clear them to '' at rest, so an idle glyph has no transform at all

const SKIP = 'script,style,textarea,select,option,canvas,noscript,input,[data-fx="none"]';
const CJK = /[⺀-鿿豈-﫿＀-￯　-〿]/u;
const IN = 5;
const OUT = 4;

export class GlyphField {
  constructor(k, root) {
    this.k = k;
    this.cap = k.x.field_cap();
    this.inp = k.f32(k.x.field_in(), this.cap * IN);
    this.out = k.f32(k.x.field_out(), this.cap * OUT);
    this.dirty = k.u32(k.x.field_dirty(), this.cap);
    this.els = [];
    this.applied = new Float32Array(this.cap * 2);
    this.groups = new Map();
    this.split(root);
    this.measure();
  }

  kindOf(el) {
    if (el.closest('[data-fx="title"]')) return 2;
    if (el.closest('marquee')) return 3;
    if (el.closest('a[href],button')) return 1;
    return 0;
  }

  groupOf(el) {
    const host = el.closest('a[href],button,[data-fx]');
    if (!host) return -1;
    if (!this.groups.has(host)) this.groups.set(host, { id: this.groups.size, n: 0 });
    return this.groups.get(host);
  }

  split(root) {
    const seg = new Intl.Segmenter('ja', { granularity: 'grapheme' });
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
      acceptNode: (n) =>
        !n.nodeValue.trim() || n.parentElement?.closest(SKIP) ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT,
    });
    const nodes = [];
    for (let n; (n = walker.nextNode()); ) nodes.push(n);

    for (const node of nodes) {
      if (this.els.length >= this.cap) break;
      const parent = node.parentElement;
      const kind = this.kindOf(parent);
      const group = this.groupOf(parent);
      const frag = document.createDocumentFragment();
      let latin = null;

      for (const { segment: s } of seg.segment(node.nodeValue)) {
        if (/^\s+$/u.test(s) || this.els.length >= this.cap) {
          latin = null;
          frag.append(s);
          continue;
        }
        const g = document.createElement('span');
        g.className = 'g';
        g.textContent = s;
        if (CJK.test(s)) {
          latin = null;
          frag.append(g);
        } else {
          if (!latin) {
            latin = document.createElement('span');
            latin.className = 'w';
            frag.append(latin);
          }
          latin.append(g);
        }
        const i = this.els.length;
        this.els.push(g);
        this.inp[i * IN + 2] = group === -1 ? -1 : group.id;
        this.inp[i * IN + 3] = kind;
        this.inp[i * IN + 4] = group === -1 ? 0 : group.n++;
      }
      node.replaceWith(frag);
    }
    this.n = this.els.length;
  }

  /** Rest positions in document space, with our own live offset removed. */
  measure() {
    const sx = scrollX;
    const sy = scrollY;
    for (let i = 0; i < this.n; i++) {
      const r = this.els[i].getBoundingClientRect();
      this.inp[i * IN] = r.left + r.width / 2 + sx - this.applied[i * 2];
      this.inp[i * IN + 1] = r.top + r.height / 2 + sy - this.applied[i * 2 + 1];
    }
  }

  hotGroup(target) {
    const host = target?.closest?.('a[href],button,[data-fx]');
    return host && this.groups.has(host) ? this.groups.get(host).id : -1;
  }

  step(dt, t, energy, mx, my, hot) {
    const nd = this.k.x.field_step(this.n, dt, t, mx, my, energy, hot);
    const { out, dirty, els, applied } = this;
    for (let k = 0; k < nd; k++) {
      const i = dirty[k];
      const o = i * OUT;
      const dx = out[o];
      const dy = out[o + 1];
      const s = els[i].style;
      applied[i * 2] = dx;
      applied[i * 2 + 1] = dy;
      if (dx === 0 && dy === 0 && out[o + 2] === 0) {
        s.translate = s.rotate = s.textShadow = '';
        continue;
      }
      s.translate = `${dx.toFixed(2)}px ${dy.toFixed(2)}px`;
      s.rotate = `${out[o + 2].toFixed(2)}deg`;
      const c = out[o + 3];
      s.textShadow = c > 0.25 ? `${c.toFixed(1)}px 0 rgb(255 0 0/.55),${(-c).toFixed(1)}px 0 rgb(0 255 255/.55)` : '';
    }
  }

  /** Synchronous: after this returns the DOM carries no trace of us. */
  calm() {
    this.k.x.field_reset();
    this.applied.fill(0);
    for (const el of this.els) el.style.translate = el.style.rotate = el.style.textShadow = '';
  }
}
