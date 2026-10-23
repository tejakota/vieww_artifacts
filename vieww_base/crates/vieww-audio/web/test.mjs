// Tests web/vieww-audio.js under Node with a simulated Web Audio: the
// worklet source runs in a sandbox with stand-ins for AudioWorkletProcessor
// and its message port, a fake wasm module renders a ramp into its memory,
// and the test drives the audio thread by calling process() for 128-frame
// quanta, checking that the output is the ramp, gap-free, with no
// underruns once primed — over each of the three transports: messages,
// the shared ring filled on the page, and the shared ring filled by a
// worker (simulated with a pair of message ports) while the page only
// hosts the context. Run: node web/test.mjs
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
  Int32Array,
  Atomics,
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
    assert.ok([5, 7, 9].includes(handle), `render for stream ${handle}`);
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

// ---- shared ring, filled on the page -------------------------------------------
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
globalThis.crossOriginIsolated = true;
counter = 0;
log.xrun = 0;
assert.equal(shim.vieww_audio_open(9, 48000, CH, 128, 0, SCRATCH, 256 * CH), 48000);
for (let k = 0; k < 6; k++) await tick();
const po = lastNode.opts.processorOptions;
assert.ok(po.sab instanceof SharedArrayBuffer, "the worklet got the ring");
assert.ok(po.cap >= 2048 && (po.cap & (po.cap - 1)) === 0, "power-of-two capacity");
expect = 0;
// 400 quanta: 51 200 frames through a 2048-frame ring.
for (let q = 0; q < 400; q++) {
  lastNode.processor.process([], [[left, right]]);
  for (let i = 0; i < 128; i++) {
    assert.equal(left[i], expect, `ring quantum ${q} frame ${i}`);
    assert.equal(right[i], -expect);
    expect++;
  }
  if (q % 4 === 3) await wait(2); // the pump's timer
}
await wait(5);
assert.equal(log.xrun, 0, "no underruns from the ring");
// A janky page: no pump for 20 quanta. The cushion (1024 frames) covers
// 8; the rest underrun, the worklet counts them, the pump reports them.
for (let q = 0; q < 20; q++) lastNode.processor.process([], [[left, right]]);
const lastBeforeGap = left;
assert.ok(lastBeforeGap.every((v) => v === 0), "silence once the cushion is spent");
await wait(5);
assert.ok(log.xrun >= 10, `underruns reported (${log.xrun})`);
// Recovery: nothing lost — the ramp continues where playback left it.
lastNode.processor.process([], [[left, right]]);
const resumed = left.find((v) => v !== 0);
assert.equal(resumed, expect + 1024, "resumes after the frames the cushion played");
shim.vieww_audio_close(9);

// ---- shared ring capture ---------------------------------------------------------
log.captured.length = 0;
assert.equal(shim.vieww_audio_open(4, 48000, CH, 128, 1, SCRATCH, 64 * CH), 48000);
for (let k = 0; k < 6; k++) await tick();
for (let q = 0; q < 3; q++) lastNode.processor.process([[micL, micR]], []);
await wait(5);
assert.equal(log.captured.length, 3 * 128 * CH, "ring capture delivered every frame");
assert.equal(log.captured[2 * 128 + 2 * 5], 5 / 128, "in order, interleaved");
assert.equal(log.captured[2 * 128 + 2 * 5 + 1], -5 / 128);
shim.vieww_audio_close(4);

// ---- worker mode: the wasm in a worker, the context on the page --------------------
function channel() {
  const make = () => ({
    listeners: [],
    other: null,
    addEventListener(_t, f) {
      this.listeners.push(f);
    },
    removeEventListener(_t, f) {
      this.listeners = this.listeners.filter((g) => g !== f);
    },
    postMessage(d) {
      const o = this.other;
      queueMicrotask(() => o.listeners.forEach((f) => f({ data: d })));
    },
  });
  const insideWorker = make();
  const workerHandle = make();
  insideWorker.other = workerHandle;
  workerHandle.other = insideWorker;
  return [insideWorker, workerHandle];
}
const [insideWorker, workerHandle] = channel();
const stopHosting = shim.hostAudio(workerHandle);
shim.attach(
  {
    memory,
    vieww_audio_render(handle, frames) {
      const v = new Float32Array(memory.buffer, SCRATCH, frames * CH);
      for (let i = 0; i < frames; i++) {
        v[i * CH] = counter;
        v[i * CH + 1] = -counter;
        counter++;
      }
      log.render += frames;
      return frames;
    },
    vieww_audio_capture() {},
    vieww_audio_xrun() {
      log.xrun++;
    },
    vieww_audio_ended() {
      log.ended++;
    },
  },
  { host: insideWorker },
);
counter = 0;
log.xrun = 0;
lastNode = null;
assert.equal(shim.vieww_audio_open(5, 48000, CH, 128, 0, SCRATCH, 256 * CH), 48000, "a worker grants the requested rate");
for (let k = 0; k < 8; k++) await tick();
assert.ok(lastNode, "the page opened the context for the worker");
assert.ok(lastNode.opts.processorOptions.sab instanceof SharedArrayBuffer);
expect = 0;
for (let q = 0; q < 200; q++) {
  lastNode.processor.process([], [[left, right]]);
  for (let i = 0; i < 128; i++) {
    assert.equal(left[i], expect, `worker quantum ${q} frame ${i}`);
    expect++;
  }
  if (q % 4 === 3) await wait(2);
}
await wait(5);
assert.equal(log.xrun, 0, "no underruns with the worker rendering");
// A rate the page's context will not run at ends the stream, honestly.
const endedBefore = log.ended;
assert.equal(shim.vieww_audio_open(6, 96000, CH, 128, 0, SCRATCH, 256 * CH), 96000);
for (let k = 0; k < 8; k++) await tick();
assert.equal(log.ended, endedBefore + 1, "mismatched rate → ended");
shim.vieww_audio_close(5);
shim.vieww_audio_close(6);
// Without isolation a worker cannot share memory: refused, not faked.
globalThis.crossOriginIsolated = false;
assert.equal(shim.vieww_audio_open(8, 48000, CH, 128, 0, SCRATCH, 512), 0, "worker without isolation → 0");
stopHosting();
shim.attach({ memory }); // back to page mode (no host) for the last check
globalThis.crossOriginIsolated = undefined;

// ---- unavailable ------------------------------------------------------------
delete globalThis.AudioContext;
assert.equal(shim.vieww_audio_open(1, 48000, 2, 128, 0, SCRATCH, 512), 0, "no Web Audio → 0");

console.log("web shim: all checks passed");
