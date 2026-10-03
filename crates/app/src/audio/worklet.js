// AudioWorkletProcessor hosting the Frekostasis machine (frekostasis-worklet.wasm).
//
// The main thread compiles the wasm module and hands it over through
// processorOptions; the module has no imports, so instantiation is synchronous.
//
// Messages on `port`, main thread → worklet, are small arrays:
//   [0, paramId, value]  set a parameter
//   [1, on]              play (1) / stop (0)
//   [2]                  snap smoothed values to their targets
//   [3]                  strike now
// Worklet → main thread: a Float32Array of telemetry every TELEMETRY_BLOCKS.

const TELEMETRY_BLOCKS = 12;

class FrekostasisProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    const o = options.processorOptions;
    this.dsp = new WebAssembly.Instance(o.module, {}).exports;
    this.handle = this.dsp.fs_new(sampleRate, o.seed >>> 0);
    this.telemetryLen = this.dsp.fs_telemetry_len();
    this.views = null;
    this.blocks = 0;
    this.port.onmessage = (e) => this.onMessage(e.data);
  }

  onMessage(m) {
    const d = this.dsp;
    switch (m[0]) {
      case 0:
        d.fs_set_param(this.handle, m[1], m[2]);
        break;
      case 1:
        d.fs_play(this.handle, m[1] ? 1 : 0);
        break;
      case 2:
        d.fs_snap(this.handle);
        break;
      case 3:
        d.fs_strike(this.handle);
        break;
    }
  }

  // (Re)create the Float32Array views if wasm memory was replaced by a grow.
  outputViews(frames) {
    const buffer = this.dsp.memory.buffer;
    const v = this.views;
    if (v === null || v.buffer !== buffer || v.frames !== frames) {
      this.views = {
        buffer,
        frames,
        left: new Float32Array(buffer, this.dsp.fs_left(this.handle), frames),
        right: new Float32Array(buffer, this.dsp.fs_right(this.handle), frames),
      };
    }
    return this.views;
  }

  process(_inputs, outputs) {
    const out = outputs[0];
    const frames = out[0].length;
    this.dsp.fs_render(this.handle, frames);
    const v = this.outputViews(frames);
    out[0].set(v.left);
    if (out.length > 1) out[1].set(v.right);
    if (++this.blocks % TELEMETRY_BLOCKS === 0) {
      const ptr = this.dsp.fs_telemetry(this.handle);
      const t = new Float32Array(this.dsp.memory.buffer, ptr, this.telemetryLen).slice();
      this.port.postMessage(t, [t.buffer]);
    }
    return true;
  }
}

registerProcessor("frekostasis", FrekostasisProcessor);
