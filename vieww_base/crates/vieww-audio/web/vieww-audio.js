// The browser side of vieww-audio's Web Audio backend (src/backend/web.rs).
//
// The Rust code imports `vieww_audio_open` / `vieww_audio_close` from the
// wasm import module "vieww_audio" and exports `vieww_audio_render`,
// `vieww_audio_capture`, `vieww_audio_xrun` and `vieww_audio_ended`. Map
// the module name to this file (an import map, or your bundler's alias),
// and after instantiating the wasm call `attach(instance.exports)`:
//
//   <script type="importmap">{ "imports": { "vieww_audio": "./vieww-audio.js" } }</script>
//   import init from "./app.js";            // wasm-bindgen output, say
//   import { attach } from "vieww_audio";
//   const wasm = await init();
//   attach(wasm);
//
// Playback: an AudioWorklet keeps a queue of interleaved blocks; when it
// falls below three blocks it asks this page thread for frames, which are
// rendered by the wasm module into its own buffer, copied out and
// transferred to the worklet. Capture runs the other way, from a
// getUserMedia source. Browsers require a user gesture before audio
// starts: open streams from an input handler, or the context waits
// suspended until one happens.

export const WORKLET_SOURCE = `
class ViewwAudioProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    const o = options.processorOptions;
    this.ch = o.channels;
    this.capture = o.capture;
    this.block = o.block;
    this.queue = [];
    this.head = 0;
    this.queued = 0;
    this.requested = 0;
    this.low = o.block * 3;
    this.started = false;
    this.port.onmessage = (e) => {
      const b = e.data.block;
      if (b) {
        this.queue.push(b);
        const frames = b.length / this.ch;
        this.queued += frames;
        this.requested = Math.max(0, this.requested - frames);
      }
    };
    if (!this.capture) this.top();
  }
  top() {
    const want = this.low * 2 - this.queued - this.requested;
    if (want > 0) {
      this.requested += want;
      this.port.postMessage({ need: want });
    }
  }
  process(inputs, outputs) {
    if (this.capture) {
      const inp = inputs[0];
      if (!inp || inp.length === 0) return true;
      const n = inp[0].length;
      const blk = new Float32Array(n * this.ch);
      for (let c = 0; c < this.ch; c++) {
        const src = inp[Math.min(c, inp.length - 1)];
        for (let i = 0; i < n; i++) blk[i * this.ch + c] = src[i];
      }
      this.port.postMessage({ captured: blk }, [blk.buffer]);
      return true;
    }
    const out = outputs[0];
    const n = out[0].length;
    let i = 0;
    while (i < n && this.queue.length) {
      const b = this.queue[0];
      const frames = b.length / this.ch;
      const take = Math.min(n - i, frames - this.head);
      for (let c = 0; c < out.length; c++) {
        const dst = out[c];
        const sc = Math.min(c, this.ch - 1);
        for (let k = 0; k < take; k++) dst[i + k] = b[(this.head + k) * this.ch + sc];
      }
      i += take;
      this.head += take;
      this.queued -= take;
      if (this.head >= frames) {
        this.queue.shift();
        this.head = 0;
      }
    }
    if (i < n) {
      for (const dst of out) dst.fill(0, i);
      if (this.started) this.port.postMessage({ underrun: 1 });
    } else {
      this.started = true;
    }
    this.top();
    return true;
  }
}
registerProcessor("vieww-audio", ViewwAudioProcessor);
`;

let wasm = null;
const streams = new Map();

/** Give the shim the wasm instance's exports (memory and the callbacks). */
export function attach(exports) {
  wasm = exports;
}

function fill(handle, st, frames) {
  const ch = st.channels;
  const max = Math.floor(st.scratchLen / ch);
  let left = frames;
  while (left > 0 && !st.closed) {
    const n = wasm.vieww_audio_render(handle, Math.min(left, max));
    if (!n) break;
    const blk = new Float32Array(wasm.memory.buffer, st.scratch, n * ch).slice();
    st.node.port.postMessage({ block: blk }, [blk.buffer]);
    left -= n;
  }
}

function captured(handle, st, blk) {
  const ch = st.channels;
  const max = Math.floor(st.scratchLen / ch);
  for (let at = 0; at < blk.length && !st.closed; at += max * ch) {
    const part = blk.subarray(at, Math.min(blk.length, at + max * ch));
    new Float32Array(wasm.memory.buffer, st.scratch, part.length).set(part);
    wasm.vieww_audio_capture(handle, part.length / ch);
  }
}

function ended(handle) {
  if (wasm) wasm.vieww_audio_ended(handle);
}

/** Import: open a context for stream `handle`; returns its rate or 0. */
export function vieww_audio_open(handle, rate, channels, block, capture, scratch, scratchLen) {
  try {
    const Ctx = globalThis.AudioContext || globalThis.webkitAudioContext;
    if (!Ctx || !wasm) return 0;
    const ctx = new Ctx({ sampleRate: rate, latencyHint: "interactive" });
    const st = { ctx, node: null, channels, scratch, scratchLen, closed: false, media: null };
    streams.set(handle, st);
    const url = URL.createObjectURL(new Blob([WORKLET_SOURCE], { type: "application/javascript" }));
    ctx.audioWorklet
      .addModule(url)
      .then(async () => {
        if (st.closed) return;
        const node = new AudioWorkletNode(ctx, "vieww-audio", {
          numberOfInputs: capture ? 1 : 0,
          numberOfOutputs: capture ? 0 : 1,
          outputChannelCount: capture ? undefined : [channels],
          processorOptions: { channels, capture: !!capture, block },
        });
        st.node = node;
        node.port.onmessage = (e) => {
          const d = e.data;
          if (d.need) fill(handle, st, d.need);
          else if (d.captured) captured(handle, st, d.captured);
          else if (d.underrun) wasm.vieww_audio_xrun(handle);
        };
        if (capture) {
          st.media = await navigator.mediaDevices.getUserMedia({ audio: { channelCount: channels } });
          ctx.createMediaStreamSource(st.media).connect(node);
        } else {
          node.connect(ctx.destination);
        }
        await ctx.resume();
      })
      .catch(() => ended(handle))
      .finally(() => URL.revokeObjectURL(url));
    ctx.onstatechange = () => {
      if (ctx.state === "closed" && !st.closed) ended(handle);
    };
    return ctx.sampleRate;
  } catch (_) {
    return 0;
  }
}

/** Import: close stream `handle`. */
export function vieww_audio_close(handle) {
  const st = streams.get(handle);
  if (!st) return;
  st.closed = true;
  streams.delete(handle);
  try {
    if (st.node) st.node.disconnect();
    if (st.media) st.media.getTracks().forEach((t) => t.stop());
    st.ctx.close();
  } catch (_) {
    /* already closed */
  }
}
