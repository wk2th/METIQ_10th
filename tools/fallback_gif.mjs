// tools/fallback_gif.mjs — renders the logo surface once through the real
// kernel and writes it as site/img/metiq_logo.gif, the image shown when
// JavaScript or WebAssembly is unavailable. The kernel already emits only
// web-safe colours with 1-bit alpha, so the frame maps losslessly onto a GIF
// palette; the LZW encoder below is the plain GIF89a variable-width one.
//
// Usage: node tools/fallback_gif.mjs   (run after build.sh)

import { readFileSync, writeFileSync } from 'node:fs';

const root = new URL('../', import.meta.url);
const { instance } = await WebAssembly.instantiate(readFileSync(new URL('site/wasm/metiq_core.wasm', root)), {});
const x = instance.exports;
x.init(1);
const w = x.logo_w();
const h = x.logo_h();
const rgba = new Uint8Array(x.memory.buffer, x.logo_render(0, 0, 0, 0), w * h * 4);

// palette: index 0 = transparent, then every distinct opaque colour
const index = new Map();
const pal = [[255, 255, 255]];
const pix = new Uint8Array(w * h);
for (let i = 0; i < w * h; i++) {
  if (rgba[i * 4 + 3] === 0) continue;
  const key = (rgba[i * 4] << 16) | (rgba[i * 4 + 1] << 8) | rgba[i * 4 + 2];
  if (!index.has(key)) {
    index.set(key, pal.length);
    pal.push([rgba[i * 4], rgba[i * 4 + 1], rgba[i * 4 + 2]]);
  }
  pix[i] = index.get(key);
}
if (pal.length > 256) throw new Error(`${pal.length} colours`);
const bits = Math.max(2, Math.ceil(Math.log2(pal.length)));

function lzw(data, minSize) {
  const out = [];
  let cur = 0;
  let nbits = 0;
  const clear = 1 << minSize;
  const eoi = clear + 1;
  let size = minSize + 1;
  let dict = new Map();
  let next = eoi + 1;
  const emit = (code) => {
    cur |= code << nbits;
    nbits += size;
    while (nbits >= 8) {
      out.push(cur & 0xff);
      cur >>>= 8;
      nbits -= 8;
    }
  };
  emit(clear);
  let prefix = data[0];
  for (let i = 1; i < data.length; i++) {
    const k = data[i];
    const key = prefix * 4096 + k;
    if (dict.has(key)) {
      prefix = dict.get(key);
      continue;
    }
    emit(prefix);
    if (next < 4096) {
      dict.set(key, next++);
      if (next > 1 << size && size < 12) size++;
    } else {
      emit(clear);
      dict = new Map();
      next = eoi + 1;
      size = minSize + 1;
    }
    prefix = k;
  }
  emit(prefix);
  emit(eoi);
  if (nbits > 0) out.push(cur & 0xff);
  return out;
}

const bytes = [];
const u16 = (v) => bytes.push(v & 0xff, v >> 8);
bytes.push(...Buffer.from('GIF89a'));
u16(w);
u16(h);
bytes.push(0x80 | ((bits - 1) << 4) | (bits - 1), 0, 0);
for (let i = 0; i < 1 << bits; i++) bytes.push(...(pal[i] ?? [0, 0, 0]));
bytes.push(0x21, 0xf9, 4, 0x01, 0, 0, 0, 0); // GCE: transparent index 0
bytes.push(0x2c);
u16(0);
u16(0);
u16(w);
u16(h);
bytes.push(0);
bytes.push(bits);
const lz = lzw(pix, bits);
for (let i = 0; i < lz.length; i += 255) {
  const chunk = lz.slice(i, i + 255);
  bytes.push(chunk.length, ...chunk);
}
bytes.push(0, 0x3b);

const out = new URL('site/img/metiq_logo.gif', root);
writeFileSync(out, Buffer.from(bytes));
console.log(`site/img/metiq_logo.gif: ${w}x${h}, ${pal.length - 1} colours + transparency, ${bytes.length} bytes`);
