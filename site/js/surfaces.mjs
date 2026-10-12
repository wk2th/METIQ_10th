// surfaces.mjs — <CANVAS>es that pretend to be animated GIFs.
//
// Each surface is a wasm pixel shader writing into its own static buffer; we
// wrap that memory in an ImageData exactly once and putImageData() it every
// frame. Offscreen surfaces are skipped via IntersectionObserver; hover state
// is low-passed here so the shaders receive a continuous 0..1 signal.

const SPECS = {
  logo:    (x) => ({ w: x.logo_w(),      h: x.logo_h(),     render: x.logo_render }),
  globe:   (x) => ({ w: x.globe_size(),  h: x.globe_size(), render: x.globe_render }),
  counter: (x) => ({ w: x.odo_w(),       h: x.odo_h(),      render: x.odo_render }),
  sign:    (x) => ({ w: x.sign_w(),      h: x.sign_h(),     render: x.sign_render, scale: 2 }),
  banner:  (x) => ({ w: x.banner_w(),    h: x.banner_h(),   render: x.banner_render }),
  rainbow: (x) => ({ w: x.rainbow_w(),   h: x.rainbow_h(),  render: x.rainbow_render }),
};

class Surface {
  constructor(k, canvas, spec) {
    Object.assign(this, spec);
    this.k = k;
    this.canvas = canvas;
    this.hover = 0;
    this.want = 0;
    this.visible = true;
    this.img = null;
    canvas.width = spec.w;
    canvas.height = spec.h;
    canvas.style.width = `${spec.w * (spec.scale ?? 1)}px`;
    canvas.style.height = `${spec.h * (spec.scale ?? 1)}px`;
    this.ctx = canvas.getContext('2d', { alpha: true, desynchronized: true });
    canvas.addEventListener('pointerenter', () => (this.want = 1));
    canvas.addEventListener('pointerleave', () => (this.want = 0));
  }

  tick(dt, t, energy) {
    this.hover += (this.want - this.hover) * (1 - Math.exp(-dt * 8));
    if (this.hover < 1e-3) this.hover = 0;
    if (!this.visible) return;
    const ptr = this.render(dt, t, energy, this.hover);
    this.img ??= this.k.pixels(ptr, this.w, this.h);
    this.ctx.putImageData(this.img, 0, 0);
  }
}

export function mountSurfaces(k) {
  const list = [];
  const byName = {};
  const io = new IntersectionObserver((es) => es.forEach((e) => (e.target.__surface.visible = e.isIntersecting)));
  // <IMG DATA-SURFACE> is the no-JS fallback (a real dithered GIF); swap it
  // for a live canvas of the same size and name.
  for (const img of document.querySelectorAll('img[data-surface]')) {
    const c = document.createElement('canvas');
    c.dataset.surface = img.dataset.surface;
    c.title = img.alt;
    c.setAttribute('aria-label', img.alt);
    img.replaceWith(c);
  }
  for (const canvas of document.querySelectorAll('canvas[data-surface]')) {
    const name = canvas.dataset.surface;
    const s = new Surface(k, canvas, SPECS[name](k.x));
    canvas.__surface = s;
    io.observe(canvas);
    list.push(s);
    (byName[name] ??= []).push(s);
  }
  return { list, byName };
}

/** Bake the seamless background tile once and install it as BODY BACKGROUND. */
export function installBackground(k) {
  const n = k.x.texture_size();
  const ptr = k.x.texture_render(0x6d657469);
  const c = new OffscreenCanvas(n, n);
  c.getContext('2d').putImageData(k.pixels(ptr, n, n), 0, 0);
  return c.convertToBlob({ type: 'image/png' }).then((b) => {
    document.body.background = URL.createObjectURL(b);
  });
}
