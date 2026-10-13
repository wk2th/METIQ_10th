// glyphs.mjs — splits the document's text into per-grapheme inline-blocks and
// drives them from the wasm glyph field (springs + decode ripples).
//
// Layout rules we have to respect so a still frame is pixel-identical to the
// unsplit page:
//   * graphemes, not code units (Intl.Segmenter) — ★ and 〜 survive intact
//   * Latin runs are wrapped in a nowrap span so words never break mid-word
//     (inline-blocks are each a break opportunity); CJK keeps per-glyph breaks
//   * text-decoration does not propagate into atomic inlines, so links get
//     their underline re-applied per glyph in CSS
//   * a glyph showing a substitute codepoint is pinned to its measured box, so
//     a ripple of corrupted text never reflows a single line
//   * we only ever write `translate` / `rotate` and clear them to '' at rest,
//     so an idle glyph carries no transform and no substitute at all

const SKIP = 'script,style,textarea,select,option,canvas,noscript,input,[data-fx="none"]';
const WIDE = /[ᄀ-ᅟ⺀-꓏가-힣豈-﫿︰-﹏＀-｠￠-￦　-〿☀-➿]/u;
const CJK = /[⺀-鿿豈-﫿＀-￯　-〿]/u;
const IN = 4;
const OUT = 4;

export class GlyphField {
  constructor(k, root) {
    this.k = k;
    this.cap = k.x.field_cap();
    this.inp = k.f32(k.x.field_in(), this.cap * IN);
    this.out = k.f32(k.x.field_out(), this.cap * OUT);
    this.codes = k.u32(k.x.field_codes(), this.cap);
    this.dirty = k.u32(k.x.field_dirty(), this.cap);
    this.els = [];
    this.text = [];
    this.index = new Map();
    this.applied = new Float32Array(this.cap * 2);
    this.box = new Float32Array(this.cap * 2);
    this.shown = new Uint32Array(this.cap);
    this.t = 0;
    this.split(root);
    this.measure();
  }

  kindOf(el) {
    if (el.closest('[data-fx="title"]')) return 2;
    if (el.closest('marquee')) return 3;
    if (el.closest('a[href],button')) return 1;
    return 0;
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
      const kind = this.kindOf(node.parentElement);
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
        this.text.push(s);
        this.index.set(g, i);
        this.inp[i * IN + 2] = kind;
        this.inp[i * IN + 3] = WIDE.test(s) ? 1 : 0;
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
      if (this.shown[i]) continue; // pinned glyph: keep the box it had
      const r = this.els[i].getBoundingClientRect();
      this.inp[i * IN] = r.left + r.width / 2 + sx - this.applied[i * 2];
      this.inp[i * IN + 1] = r.top + r.height / 2 + sy - this.applied[i * 2 + 1];
      this.box[i * 2] = r.width;
      this.box[i * 2 + 1] = r.height;
    }
    this.k.x.field_index(this.n);
  }

  /**
   * Pointer touched a glyph element: infect it, and let the corruption spread
   * from there — but only once the pointer has moved a couple of characters
   * from the last infection, so resting on a word doesn't keep re-scrambling.
   */
  touch(el) {
    const i = this.index.get(el);
    if (i === undefined || this.inp[i * IN + 2] === 3) return false;
    const x = this.inp[i * IN];
    const y = this.inp[i * IN + 1];
    if (this.last) {
      const reach = Math.max(24, this.box[i * 2] * 1.8);
      if (Math.hypot(x - this.last.x, y - this.last.y) < reach) return false;
    }
    this.last = { x, y };
    this.k.x.field_spread(i, this.t);
    return true;
  }

  step(dt, t, energy) {
    this.t = t;
    const nd = this.k.x.field_step(this.n, dt, t, energy);
    const { out, codes, shown, dirty, els, applied } = this;
    for (let k = 0; k < nd; k++) {
      const i = dirty[k];
      const el = els[i];
      const s = el.style;

      const code = codes[i];
      if (code !== shown[i]) {
        if (code) {
          if (!shown[i]) {
            s.width = `${this.box[i * 2]}px`;
            s.height = `${this.box[i * 2 + 1]}px`;
            el.classList.add('z');
          }
          el.textContent = String.fromCodePoint(code);
        } else {
          el.textContent = this.text[i];
          s.width = s.height = '';
          el.classList.remove('z');
        }
        shown[i] = code;
      }

      const o = i * OUT;
      const dx = out[o];
      const dy = out[o + 1];
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
    for (let i = 0; i < this.n; i++) {
      const el = this.els[i];
      const s = el.style;
      s.translate = s.rotate = s.textShadow = '';
      if (this.shown[i]) {
        el.textContent = this.text[i];
        s.width = s.height = '';
        el.classList.remove('z');
        this.shown[i] = 0;
      }
    }
  }
}
