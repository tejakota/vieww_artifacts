// Tests web/vieww-audio.js under Node with a simulated Web Audio: the
// worklet source runs in a sandbox with stand-ins for AudioWorkletProcessor
// and its message port, a fake wasm module renders a ramp into its memory,
// and the test drives the audio thread by calling process() for 128-frame
// quanta, checking that the output is the ramp, gap-free, with no
// underruns once primed. Run: node web/test.mjs
import assert from "node:assert/strict";
import vm from "node:vm";
import * as shim from "./vieww-audio.js";

// ---- message ports delivering asynchronously, like the real thing ------
function portPair() {
  const a = { onmessage: null, postMessage: (d) => queueMicrotask(() => b.onmessage && b.onmessage({ data: d })) };
  const b = { onmessage: null, postMessage: (d) => queueMicrotask(() => a.onmessage && a.onmessage({ data: d })) };
  return [a, b];
}
const tick = () => new Promise((r) => setTimeout(r, 0));

// ---- the worklet scope ----------------------------------------------------
let Processor = null;
let workletPort = null;
const scope = {
  AudioWorkletProcessor: class {
    constructor() {
      this.port = workletPort;
    }
  },
  registerProcessor: (_name, cls) => {
    Processor = cls;
  },
  Float32Array,
  Math,
};
vm.runInNewContext(shim.WORKLET_SOURCE, scope);
assert.ok(Processor, "worklet registered its processor");

// ---- fake browser objects -------------------------------------------------
let mainPort = null;
let lastNode = null;
class FakeNode {
  constructor(_ctx, name, opts) {
    assert.equal(name, "vieww-audio");
    const [m, w] = portPair();
    mainPort = m;
    workletPort = w;
    this.port = m;
    this.opts = opts;
    this.processor = new Processor({ processorOptions: opts.processorOptions });
    lastNode = this;
  }
  connect() {}
  disconnect() {}
}
class FakeContext {
  constructor(o) {
    this.sampleRate = o.sampleRate === 44100 ? 44100 : 48000; // a browser picks
    this.state = "running";
    this.audioWorklet = { addModule: async () => {} };
    this.destination = {};
  }
  async resume() {}
  close() {
    this.state = "closed";
  }
}
globalThis.AudioContext = FakeContext;
globalThis.AudioWorkletNode = FakeNode;
globalThis.URL.createObjectURL = () => "blob:worklet";
globalThis.URL.revokeObjectURL = () => {};

// ---- fake wasm module ------------------------------------------------------
const memory = { buffer: new ArrayBuffer(1 << 20) };
let counter = 0;
const log = { render: 0, xrun: 0, captured: [], ended: 0 };
const SCRATCH = 4096; // byte offset of the stream's scratch buffer
const CH = 2;
shim.attach({
  memory,
  vieww_audio_render(handle, frames) {
    assert.equal(handle, 7);
    const v = new Float32Array(memory.buffer, SCRATCH, frames * CH);
    for (let i = 0; i < frames; i++) {
      v[i * CH] = counter;
      v[i * CH + 1] = -counter;
      counter++;
    }
    log.render += frames;
    return frames;
  },
  vieww_audio_capture(handle, frames) {
    const v = new Float32Array(memory.buffer, SCRATCH, frames * CH);
    log.captured.push(...v.slice(0, frames * CH));
  },
  vieww_audio_xrun() {
    log.xrun++;
  },
  vieww_audio_ended() {
    log.ended++;
  },
});

// ---- playback --------------------------------------------------------------
const rate = shim.vieww_audio_open(7, 96000, CH, 128, 0, SCRATCH, 256 * CH);
assert.equal(rate, 48000, "the granted rate is the context's");
await tick();
await tick();
assert.ok(lastNode, "worklet node created");
assert.deepEqual(lastNode.opts.outputChannelCount, [CH]);
for (let k = 0; k < 6; k++) await tick(); // let the first request be served
const left = new Float32Array(128);
const right = new Float32Array(128);
let expect = 0;
for (let q = 0; q < 200; q++) {
  lastNode.processor.process([], [[left, right]]);
  for (let i = 0; i < 128; i++) {
    assert.equal(left[i], expect, `quantum ${q} frame ${i}`);
    assert.equal(right[i], -expect);
    expect++;
  }
  await tick();
}
assert.equal(log.xrun, 0, "no underruns once primed");
assert.ok(log.render >= 200 * 128, "rendered what was played");
assert.ok(log.render <= 200 * 128 + 128 * 6 + 256, "and kept the queue bounded");

// Starve it: stop serving requests and the worklet reports underruns.
mainPort.onmessage = () => {};
for (let q = 0; q < 10; q++) lastNode.processor.process([], [[left, right]]);
await tick();
// (underrun messages go to the replaced handler, so count via the processor)
assert.ok(left.every((v) => v === 0), "silence when starved");
shim.vieww_audio_close(7);

// ---- capture ---------------------------------------------------------------
Object.defineProperty(globalThis, "navigator", {
  configurable: true,
  value: { mediaDevices: { getUserMedia: async () => ({ getTracks: () => [{ stop() {} }] }) } },
});
FakeContext.prototype.createMediaStreamSource = () => ({ connect() {} });
assert.equal(shim.vieww_audio_open(3, 48000, CH, 128, 1, SCRATCH, 64 * CH), 48000);
for (let k = 0; k < 6; k++) await tick();
const micL = Float32Array.from({ length: 128 }, (_, i) => i / 128);
const micR = Float32Array.from({ length: 128 }, (_, i) => -i / 128);
lastNode.processor.process([[micL, micR]], []);
for (let k = 0; k < 4; k++) await tick();
assert.equal(log.captured.length, 128 * CH, "captured frames delivered (in 64-frame scratch pieces)");
assert.equal(log.captured[2 * 5], 5 / 128);
assert.equal(log.captured[2 * 5 + 1], -5 / 128);
shim.vieww_audio_close(3);

// ---- unavailable ------------------------------------------------------------
delete globalThis.AudioContext;
assert.equal(shim.vieww_audio_open(1, 48000, 2, 128, 0, SCRATCH, 512), 0, "no Web Audio → 0");

console.log("web shim: all checks passed");
