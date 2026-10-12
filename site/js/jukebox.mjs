// jukebox.mjs — the "MIDI" player. A second instance of metiq_core.wasm runs
// inside an AudioWorklet and renders 128-frame blocks on the audio thread.

export class Jukebox {
  constructor(bytes, ui) {
    this.bytes = bytes;
    this.ui = ui;
    this.ctx = null;
    this.t0 = 0;
    ui.play.addEventListener('click', () => this.play());
    ui.stop.addEventListener('click', () => this.stop());
  }

  async play() {
    if (!this.ctx) {
      this.ctx = new AudioContext({ latencyHint: 'playback' });
      await this.ctx.audioWorklet.addModule(new URL('./worklet.mjs', import.meta.url));
      this.node = new AudioWorkletNode(this.ctx, 'metiq-sid', { outputChannelCount: [2] });
      this.node.port.postMessage({ bytes: this.bytes.slice(0) });
      this.node.connect(this.ctx.destination);
    }
    await this.ctx.resume();
    this.t0 = this.ctx.currentTime - (this.paused ?? 0);
    this.ui.state.textContent = '再生中';
    clearInterval(this.timer);
    this.timer = setInterval(() => this.clock(), 250);
  }

  async stop() {
    if (!this.ctx) return;
    this.paused = this.ctx.currentTime - this.t0;
    await this.ctx.suspend();
    clearInterval(this.timer);
    this.ui.state.textContent = '停止';
  }

  clock() {
    const s = Math.floor(this.ctx.currentTime - this.t0);
    this.ui.time.textContent = `${String((s / 60) | 0).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`;
  }
}
