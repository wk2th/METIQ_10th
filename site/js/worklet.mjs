// worklet.mjs — AudioWorkletGlobalScope. Instantiates metiq_core.wasm from
// bytes posted by the main thread and pulls synth_render() per quantum.

class MetiqSID extends AudioWorkletProcessor {
  constructor() {
    super();
    this.x = null;
    this.port.onmessage = async ({ data }) => {
      if (!data.bytes) return;
      const { instance } = await WebAssembly.instantiate(data.bytes, {});
      this.x = instance.exports;
      this.x.synth_init(sampleRate);
    };
  }

  process(_inputs, outputs) {
    const out = outputs[0];
    if (!this.x) return true;
    const n = out[0].length;
    const v = new Float32Array(this.x.memory.buffer, this.x.synth_render(n), n);
    for (const ch of out) ch.set(v);
    return true;
  }
}

registerProcessor('metiq-sid', MetiqSID);
