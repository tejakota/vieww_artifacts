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
// Three ways audio moves, chosen by what the page allows:
//
// 1. Worker (render off the page thread). Run the app's wasm in a Web
//    Worker and `attach(wasm)` there; on the page, `hostAudio(worker)`.
//    The worker renders into a SharedArrayBuffer ring on its own timer;
//    the page only owns the AudioContext (which workers cannot create)
//    and the worklet reads the ring. Layout, scripts and GC on the page
//    cannot starve the audio. Needs cross-origin isolation (COOP
//    `same-origin` + COEP `require-corp`), which SharedArrayBuffer needs.
// 2. Shared ring on the page. Cross-origin isolated but single-threaded:
//    the same ring, filled by a pump on the page that runs ahead of the
//    worklet by a cushion (default four blocks, at least 1024 frames;
//    `attach(wasm, { cushion })`). The worklet never allocates — no GC on
//    the audio thread — and needs no message round trip per block.
// 3. Messages (no isolation). The worklet keeps a queue of transferred
//    blocks and asks the page for frames when it falls below three blocks.
//
// Capture runs the other way in each, from a getUserMedia source. Browsers
// require a user gesture before audio starts: open streams from an input
// handler, or the context waits suspended until one happens.

export const WORKLET_SOURCE = `
class ViewwAudioProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    const o = options.processorOptions;
    this.ch = o.channels;
    this.capture = o.capture;
    this.block = o.block;
    this.started = false;
    this.low = o.block * 3;
    if (o.sab) {
      // Ring: [write, read, xruns, spare] as Int32, then interleaved
      // frames. Indices count frames and wrap at 2^32; capacity is a power
      // of two.
      this.hdr = new Int32Array(o.sab, 0, 4);
      this.ring = new Float32Array(o.sab, 16, o.cap * o.channels);
      this.cap = o.cap;
      this.mask = o.cap - 1;
      this.poked = false;
      return;
    }
    this.queue = [];
    this.head = 0;
    this.queued = 0;
    this.requested = 0;
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
  ringPlay(out) {
    const h = this.hdr, ring = this.ring, mask = this.mask, ch = this.ch;
    const n = out[0].length;
    const r = Atomics.load(h, 1);
    const avail = (Atomics.load(h, 0) - r) | 0;
    const take = Math.min(n, avail);
    for (let c = 0; c < out.length; c++) {
      const dst = out[c];
      const sc = Math.min(c, ch - 1);
      for (let k = 0; k < take; k++) dst[k] = ring[((r + k) & mask) * ch + sc];
      if (take < n) dst.fill(0, take);
    }
    Atomics.store(h, 1, (r + take) | 0);
    if (take < n) {
      if (this.started) Atomics.add(h, 2, 1);
    } else {
      this.started = true;
    }
    // Wake a page-side pump early when running low (a worker's pump runs
    // on its own timer and ignores this).
    if (avail - take < this.low) {
      if (!this.poked) {
        this.poked = true;
        this.port.postMessage({ need: 1 });
      }
    } else {
      this.poked = false;
    }
  }
  ringCapture(inp) {
    const h = this.hdr, ring = this.ring, mask = this.mask, ch = this.ch;
    const n = inp[0].length;
    const w = Atomics.load(h, 0);
    const used = (w - Atomics.load(h, 1)) | 0;
    if (this.cap - used < n) {
      Atomics.add(h, 2, 1); // overrun: the consumer fell behind
      return;
    }
    for (let c = 0; c < ch; c++) {
      const src = inp[Math.min(c, inp.length - 1)];
      for (let i = 0; i < n; i++) ring[((w + i) & mask) * ch + c] = src[i];
    }
    Atomics.store(h, 0, (w + n) | 0);
  }
  process(inputs, outputs) {
    if (this.capture) {
      const inp = inputs[0];
      if (!inp || inp.length === 0) return true;
      if (this.hdr) {
        this.ringCapture(inp);
        return true;
      }
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
    if (this.hdr) {
      this.ringPlay(out);
      return true;
    }
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
let host = null; // worker mode: the channel to the page that owns the audio
let cushion = 0;
const streams = new Map();

/** Whether SharedArrayBuffer may be used (the page is cross-origin isolated). */
function sharedOk() {
  return typeof SharedArrayBuffer === "function" && globalThis.crossOriginIsolated === true;
}

function workerScope() {
  return typeof globalThis.WorkerGlobalScope === "function" && globalThis instanceof globalThis.WorkerGlobalScope;
}

function listen(port, on) {
  if (port.addEventListener) port.addEventListener("message", on);
  else port.onmessage = on;
}

/**
 * Give the shim the wasm instance's exports (memory and the callbacks).
 *
 * Options: `cushion` — frames the ring pump keeps ahead of the worklet
 * (default four blocks, at least 1024); `host` — in worker mode, the port
 * to the page (default: the worker's own global, when attached inside a
 * worker).
 */
export function attach(exports, options = {}) {
  wasm = exports;
  cushion = options.cushion || 0;
  host = options.host || (workerScope() ? globalThis : null);
  if (host) {
    listen(host, (e) => {
      const d = e.data;
      if (d && d.vieww_audio === "ended") {
        const st = streams.get(d.handle);
        if (st && !st.closed) {
          stopPump(st);
          ended(d.handle);
        }
      }
    });
  }
}

/** Open an AudioContext with a vieww-audio worklet node. */
function openContext(o, events) {
  const Ctx = globalThis.AudioContext || globalThis.webkitAudioContext;
  if (!Ctx) throw new Error("no Web Audio");
  const ctx = new Ctx({ sampleRate: o.rate, latencyHint: "interactive" });
  const c = { ctx, node: null, media: null, closed: false };
  const url = URL.createObjectURL(new Blob([WORKLET_SOURCE], { type: "application/javascript" }));
  ctx.audioWorklet
    .addModule(url)
    .then(async () => {
      if (c.closed) return;
      const node = new AudioWorkletNode(ctx, "vieww-audio", {
        numberOfInputs: o.capture ? 1 : 0,
        numberOfOutputs: o.capture ? 0 : 1,
        outputChannelCount: o.capture ? undefined : [o.channels],
        processorOptions: { channels: o.channels, capture: !!o.capture, block: o.block, sab: o.sab, cap: o.cap },
      });
      c.node = node;
      node.port.onmessage = (e) => events.message(e.data);
      if (o.capture) {
        c.media = await navigator.mediaDevices.getUserMedia({ audio: { channelCount: o.channels } });
        ctx.createMediaStreamSource(c.media).connect(node);
      } else {
        node.connect(ctx.destination);
      }
      await ctx.resume();
    })
    .catch(() => {
      if (!c.closed) events.ended();
    })
    .finally(() => URL.revokeObjectURL(url));
  ctx.onstatechange = () => {
    if (ctx.state === "closed" && !c.closed) events.ended();
  };
  return c;
}

function closeContext(c) {
  c.closed = true;
  try {
    if (c.node) c.node.disconnect();
    if (c.media) c.media.getTracks().forEach((t) => t.stop());
    c.ctx.close();
  } catch (_) {
    /* already closed */
  }
}

/**
 * On the page, for an app whose wasm runs in `worker`: own the audio
 * contexts the worker's streams ask for. Returns a function that stops
 * hosting (closing every context).
 */
export function hostAudio(worker) {
  const open = new Map();
  const on = (e) => {
    const d = e.data;
    if (!d || !d.vieww_audio) return;
    if (d.vieww_audio === "open") {
      const st = { closed: false, c: null };
      open.set(d.handle, st);
      const tell = () => {
        if (st.closed) return;
        st.closed = true;
        open.delete(d.handle);
        if (st.c) closeContext(st.c);
        worker.postMessage({ vieww_audio: "ended", handle: d.handle });
      };
      try {
        st.c = openContext(d, { message() {}, ended: tell });
        // The worker told the Rust side this rate; a context that would
        // not run at it ends the stream rather than play at the wrong one.
        if (st.c.ctx.sampleRate !== d.rate) tell();
      } catch (_) {
        tell();
      }
    } else if (d.vieww_audio === "close") {
      const st = open.get(d.handle);
      if (st) {
        st.closed = true;
        open.delete(d.handle);
        if (st.c) closeContext(st.c);
      }
    }
  };
  listen(worker, on);
  return () => {
    if (worker.removeEventListener) worker.removeEventListener("message", on);
    for (const st of open.values()) {
      st.closed = true;
      if (st.c) closeContext(st.c);
    }
    open.clear();
  };
}

// ---- the ring pump (runs wherever the wasm runs) ---------------------------

function ringStream(rate, channels, block, capture, scratch, scratchLen) {
  const target = Math.max(cushion || block * 4, 1024, block * 2);
  let cap = 1;
  while (cap < target * 2) cap *= 2;
  const sab = new SharedArrayBuffer(16 + cap * channels * 4);
  return {
    ring: true,
    sab,
    cap,
    hdr: new Int32Array(sab, 0, 4),
    data: new Float32Array(sab, 16, cap * channels),
    target,
    rate,
    channels,
    block,
    capture: !!capture,
    scratch,
    scratchLen,
    closed: false,
    timer: null,
    xseen: 0,
    period: Math.max(1, Math.floor((500 * block) / Math.max(rate, 1))),
  };
}

function pumpOnce(handle, st) {
  const h = st.hdr, data = st.data, mask = st.cap - 1, ch = st.channels;
  const max = Math.floor(st.scratchLen / ch);
  if (!st.capture) {
    let free = st.target - ((Atomics.load(h, 0) - Atomics.load(h, 1)) | 0);
    while (free > 0 && !st.closed) {
      const n = wasm.vieww_audio_render(handle, Math.min(free, max));
      if (!n) break;
      const src = new Float32Array(wasm.memory.buffer, st.scratch, n * ch);
      const w = Atomics.load(h, 0);
      const at = w & mask;
      const first = Math.min(n, st.cap - at);
      data.set(src.subarray(0, first * ch), at * ch);
      if (n > first) data.set(src.subarray(first * ch), 0);
      Atomics.store(h, 0, (w + n) | 0);
      free -= n;
    }
  } else {
    let r = Atomics.load(h, 1);
    let avail = (Atomics.load(h, 0) - r) | 0;
    while (avail > 0 && !st.closed) {
      const n = Math.min(avail, max);
      const dst = new Float32Array(wasm.memory.buffer, st.scratch, n * ch);
      const at = r & mask;
      const first = Math.min(n, st.cap - at);
      dst.set(data.subarray(at * ch, (at + first) * ch));
      if (n > first) dst.set(data.subarray(0, (n - first) * ch), first * ch);
      r = (r + n) | 0;
      Atomics.store(h, 1, r);
      wasm.vieww_audio_capture(handle, n);
      avail -= n;
    }
  }
  const x = Atomics.load(h, 2);
  const missed = Math.min((x - st.xseen) | 0, 1000);
  for (let i = 0; i < missed; i++) wasm.vieww_audio_xrun(handle);
  st.xseen = x;
}

function startPump(handle, st) {
  const loop = () => {
    st.timer = null;
    if (st.closed) return;
    pumpOnce(handle, st);
    st.timer = setTimeout(loop, st.period);
  };
  loop();
}

function stopPump(st) {
  st.closed = true;
  if (st.timer !== null) clearTimeout(st.timer);
  st.timer = null;
}

// ---- the message path --------------------------------------------------------

function fill(handle, st, frames) {
  const ch = st.channels;
  const max = Math.floor(st.scratchLen / ch);
  let left = frames;
  while (left > 0 && !st.closed) {
    const n = wasm.vieww_audio_render(handle, Math.min(left, max));
    if (!n) break;
    const blk = new Float32Array(wasm.memory.buffer, st.scratch, n * ch).slice();
    st.c.node.port.postMessage({ block: blk }, [blk.buffer]);
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
    if (!wasm) return 0;
    if (host) {
      // Worker: the page opens the context; every browser runs one at any
      // rate from 8 to 96 kHz, so the requested rate is the granted one.
      if (!sharedOk() || rate < 8000 || rate > 96000) return 0;
      const st = ringStream(rate, channels, block, capture, scratch, scratchLen);
      st.worker = true;
      streams.set(handle, st);
      host.postMessage({ vieww_audio: "open", handle, rate, channels, block, capture: !!capture, sab: st.sab, cap: st.cap });
      startPump(handle, st);
      return rate;
    }
    if (sharedOk()) {
      const st = ringStream(rate, channels, block, capture, scratch, scratchLen);
      streams.set(handle, st);
      st.c = openContext(
        { rate, channels, block, capture, sab: st.sab, cap: st.cap },
        {
          message: (d) => {
            if (d.need && !st.closed) pumpOnce(handle, st);
          },
          ended: () => {
            if (!st.closed) {
              stopPump(st);
              ended(handle);
            }
          },
        },
      );
      st.rate = st.c.ctx.sampleRate;
      startPump(handle, st);
      return st.c.ctx.sampleRate;
    }
    const st = { c: null, channels, scratch, scratchLen, closed: false };
    streams.set(handle, st);
    st.c = openContext(
      { rate, channels, block, capture },
      {
        message: (d) => {
          if (d.need) fill(handle, st, d.need);
          else if (d.captured) captured(handle, st, d.captured);
          else if (d.underrun) wasm.vieww_audio_xrun(handle);
        },
        ended: () => {
          if (!st.closed) ended(handle);
        },
      },
    );
    return st.c.ctx.sampleRate;
  } catch (_) {
    streams.delete(handle);
    return 0;
  }
}

/** Import: close stream `handle`. */
export function vieww_audio_close(handle) {
  const st = streams.get(handle);
  if (!st) return;
  streams.delete(handle);
  if (st.ring) stopPump(st);
  st.closed = true;
  if (st.worker) {
    host.postMessage({ vieww_audio: "close", handle });
  } else if (st.c) {
    closeContext(st.c);
  }
}
