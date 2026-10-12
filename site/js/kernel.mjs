// kernel.mjs — loads metiq_core.wasm and exposes zero-copy views into it.
//
// The module has no imports and never calls memory.grow (every buffer is a
// static in .bss), so a typed-array view created once stays valid for the
// lifetime of the page. That is what lets every surface hand an ImageData
// that *aliases wasm linear memory* straight to putImageData.

const BUILD = 0x19961012;
export const WASM_URL = new URL('../wasm/metiq_core.wasm', import.meta.url);

/** @returns {Promise<{x: WebAssembly.Exports & Record<string, Function>, bytes: ArrayBuffer, pixels: Function, f32: Function, u32: Function}>} */
export async function boot() {
  const res = await fetch(WASM_URL, { cache: 'force-cache' });
  if (!res.ok) throw new Error(`metiq_core.wasm: HTTP ${res.status}`);
  const bytes = await res.arrayBuffer();
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const x = instance.exports;

  if (x.metiq_build() !== BUILD) throw new Error('metiq_core.wasm: build stamp mismatch');
  x.init(crypto.getRandomValues(new Uint32Array(1))[0]);

  const mem = () => x.memory.buffer;
  return {
    x,
    bytes,
    pixels: (ptr, w, h) => new ImageData(new Uint8ClampedArray(mem(), ptr, w * h * 4), w, h),
    f32: (ptr, len) => new Float32Array(mem(), ptr, len),
    u32: (ptr, len) => new Uint32Array(mem(), ptr, len),
  };
}
