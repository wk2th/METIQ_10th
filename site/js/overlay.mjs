// overlay.mjs — the two full-viewport layers that never exist in a still:
//   #crt     low-res SIMD noise / scanlines / tear bands, upscaled pixelated
//   #sparks  cursor-trail stars from the wasm particle system
// Both are display:none whenever energy is zero.

// 0-4 cursor trail · 5-6 electric · 7-8 fire · 9 rust dust · 10 smoke · 11 hack green
const STAR = ['#FFFF00', '#FF00FF', '#00FFFF', '#FFFFFF', '#FF9900', '#CCF2FF', '#99CCFF', '#FF6600', '#FFCC00', '#996633', '#999999', '#00FF66'];

export class Overlay {
  constructor(k) {
    this.k = k;
    this.crt = Object.assign(document.createElement('canvas'), { id: 'crt' });
    this.sp = Object.assign(document.createElement('canvas'), { id: 'sparks' });
    this.crt.hidden = this.sp.hidden = true;
    // Children of <html>, not <body>: the glitch transforms <body>, and a
    // transformed ancestor would turn position:fixed into position:absolute.
    document.documentElement.append(this.crt, this.sp);
    this.cctx = this.crt.getContext('2d');
    this.sctx = this.sp.getContext('2d');
    this.sparkView = k.f32(k.x.spark_out(), 384 * 4);
    this.resize();
    addEventListener('resize', () => this.resize());
  }

  resize() {
    this.cw = 240;
    this.ch = Math.min(320, Math.max(4, Math.round((240 * innerHeight) / innerWidth)));
    this.crt.width = this.cw;
    this.crt.height = this.ch;
    this.sp.width = innerWidth;
    this.sp.height = innerHeight;
    this.img = null;
  }

  frame(dt, t, energy, glitch) {
    const x = this.k.x;
    const on = energy > 0 || glitch > 0;
    this.crt.hidden = !on;
    if (on) {
      const ptr = x.crt_render(this.cw, this.ch, t, energy, glitch);
      this.img ??= this.k.pixels(ptr, this.cw, this.ch);
      this.cctx.putImageData(this.img, 0, 0);
    }

    const n = x.spark_step(dt);
    this.sp.hidden = n === 0;
    if (n === 0 && !this.dirty) return;
    const c = this.sctx;
    c.clearRect(0, 0, this.sp.width, this.sp.height);
    const v = this.sparkView;
    for (let i = 0; i < n; i++) {
      const px = v[i * 4] | 0;
      const py = v[i * 4 + 1] | 0;
      const s = v[i * 4 + 2];
      c.fillStyle = STAR[v[i * 4 + 3] | 0];
      c.fillRect(px, py, 1, 1);
      if (s >= 2) c.fillRect(px - 1, py, 3, 1), c.fillRect(px, py - 1, 1, 3);
      if (s >= 3) c.fillRect(px - 2, py, 5, 1), c.fillRect(px, py - 2, 1, 5);
    }
    this.dirty = n > 0;
  }

  calm() {
    this.crt.hidden = this.sp.hidden = true;
    this.sctx.clearRect(0, 0, this.sp.width, this.sp.height);
    this.dirty = false;
  }
}
