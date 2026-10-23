//! Web Audio: an `AudioContext` and an `AudioWorklet`, through a small
//! JavaScript shim instead of a bindings crate.
//!
//! The Rust side imports three functions from the wasm import module
//! `"vieww_audio"` and exports three; `web/vieww-audio.js` (shipped with
//! this crate) provides the imports. The worklet keeps a queue of blocks
//! and, when it runs low, asks the page thread for more; the page thread
//! calls [`vieww_audio_render`], which runs the stream's render callback
//! into a preallocated buffer in wasm memory, and transfers the block to
//! the worklet. Capture is the mirror image ([`vieww_audio_capture`]).
//! Browsers choose the context's real rate when asked for one they do not
//! support; [`DeviceStream::config`] reports it.
//!
//! Where the callbacks run depends on the page (the shim picks):
//!
//! - **In a Web Worker** — the app instantiates its wasm in a worker and
//!   calls the shim's `attach` there, and `hostAudio(worker)` on the page.
//!   The worker renders into a `SharedArrayBuffer` ring on its own timer;
//!   the page only owns the `AudioContext`. Page layout, scripts and GC
//!   cannot starve the audio. Needs cross-origin isolation (COOP/COEP).
//! - **On the page, through the shared ring** (isolated, single-threaded):
//!   a pump keeps a cushion of frames ahead of the worklet, which reads the
//!   ring without allocating — no GC on the audio thread, no per-block
//!   message round trip.
//! - **On the page, through messages** (not isolated): blocks are
//!   transferred to the worklet as it asks for them.
//!
//! In the page modes a render callback shares the page's thread, so it
//! should finish well inside a block's duration and the page should not
//! block for longer than the cushion; the worker mode has neither limit.
//! `web/test.mjs` drives all three under Node with a simulated Web Audio.

use std::cell::RefCell;
use std::sync::Arc;

use super::{
    isolate_capture, isolate_render, Backend, BackendError, BlockInfo, CaptureFn, Counters,
    DeviceStream, RenderFn, StreamConfig, StreamImpl,
};

#[cfg_attr(target_arch = "wasm32", link(wasm_import_module = "vieww_audio"))]
unsafe extern "C" {
    /// Create a context and worklet node; returns the granted rate, or 0.
    fn vieww_audio_open(
        handle: u32,
        rate: u32,
        channels: u32,
        block: u32,
        capture: u32,
        scratch: *mut f32,
        scratch_len: u32,
    ) -> u32;
    /// Close a context.
    fn vieww_audio_close(handle: u32);
}

enum Callback {
    Render(RenderFn),
    Capture(CaptureFn),
}

struct Entry {
    callback: Callback,
    scratch: Vec<f32>,
    channels: usize,
    rate: u32,
    counters: Arc<Counters>,
    frames_done: u64,
}

thread_local! {
    static STREAMS: RefCell<Vec<Option<Entry>>> = const { RefCell::new(Vec::new()) };
}

const MAX_BLOCK: usize = 4_096;

/// Called by the shim: render `frames` frames of stream `handle` into its
/// scratch buffer (whose address the shim was given at open).
#[unsafe(no_mangle)]
pub extern "C" fn vieww_audio_render(handle: u32, frames: u32) -> u32 {
    STREAMS.with_borrow_mut(|s| {
        let Some(Some(e)) = s.get_mut(handle as usize) else {
            return 0;
        };
        let n = (frames as usize).min(MAX_BLOCK);
        let len = n * e.channels;
        let out = &mut e.scratch[..len];
        out.fill(0.0);
        #[expect(clippy::cast_precision_loss, reason = "stream time")]
        let info = BlockInfo {
            frames: n,
            channels: e.channels,
            rate: e.rate,
            time: e.frames_done as f64 / f64::from(e.rate),
        };
        if let Callback::Render(r) = &mut e.callback {
            r(out, &info);
        }
        e.frames_done += n as u64;
        e.counters.block(n);
        u32::try_from(n).unwrap_or(0)
    })
}

/// Called by the shim after copying `frames` captured frames into stream
/// `handle`'s scratch buffer.
#[unsafe(no_mangle)]
pub extern "C" fn vieww_audio_capture(handle: u32, frames: u32) {
    STREAMS.with_borrow_mut(|s| {
        let Some(Some(e)) = s.get_mut(handle as usize) else {
            return;
        };
        let n = (frames as usize).min(MAX_BLOCK);
        #[expect(clippy::cast_precision_loss, reason = "stream time")]
        let info = BlockInfo {
            frames: n,
            channels: e.channels,
            rate: e.rate,
            time: e.frames_done as f64 / f64::from(e.rate),
        };
        let (scratch, channels) = (&e.scratch, e.channels);
        if let Callback::Capture(c) = &mut e.callback {
            c(&scratch[..n * channels], &info);
        }
        e.frames_done += n as u64;
        e.counters.block(n);
    });
}

/// Called by the shim when the worklet ran out of audio (an underrun).
#[unsafe(no_mangle)]
pub extern "C" fn vieww_audio_xrun(handle: u32) {
    STREAMS.with_borrow(|s| {
        if let Some(Some(e)) = s.get(handle as usize) {
            e.counters
                .xruns
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    });
}

/// Called by the shim when the context fails or is closed by the page.
#[unsafe(no_mangle)]
pub extern "C" fn vieww_audio_ended(handle: u32) {
    STREAMS.with_borrow(|s| {
        if let Some(Some(e)) = s.get(handle as usize) {
            e.counters
                .ended
                .store(true, std::sync::atomic::Ordering::Release);
        }
    });
}

struct WebStream(u32);

impl StreamImpl for WebStream {
    fn stop(&mut self) {
        if self.0 == u32::MAX {
            return;
        }
        // SAFETY: the shim's close for a handle it opened.
        unsafe { vieww_audio_close(self.0) };
        STREAMS.with_borrow_mut(|s| {
            if let Some(slot) = s.get_mut(self.0 as usize) {
                *slot = None;
            }
        });
        self.0 = u32::MAX;
    }
}

fn open(config: StreamConfig, callback: Callback) -> Result<DeviceStream, BackendError> {
    let capture = matches!(callback, Callback::Capture(_));
    let ch = config.channels.clamp(1, 32);
    let counters = Arc::new(Counters::default());
    // The callback runs from the shim's exported functions — an FFI
    // boundary for a panic, even though the thread is the page's own.
    // Isolating it here keeps the failure mode the same as every other
    // backend: one caught panic, then silence and `ended`.
    let callback = match callback {
        Callback::Render(r) => Callback::Render(isolate_render(r, &counters)),
        Callback::Capture(c) => Callback::Capture(isolate_capture(c, &counters)),
    };
    let (handle, scratch) = STREAMS.with_borrow_mut(|s| {
        let i = s.iter().position(Option::is_none).unwrap_or_else(|| {
            s.push(None);
            s.len() - 1
        });
        let mut scratch = vec![0.0f32; MAX_BLOCK * usize::from(ch)];
        let ptr = scratch.as_mut_ptr();
        s[i] = Some(Entry {
            callback,
            scratch,
            channels: usize::from(ch),
            rate: config.rate,
            counters: counters.clone(),
            frames_done: 0,
        });
        (u32::try_from(i).unwrap_or(0), ptr)
    });
    // SAFETY: the shim's open; the scratch buffer lives in the registry
    // until `stop`, and wasm memory addresses are stable.
    let rate = unsafe {
        vieww_audio_open(
            handle,
            config.rate,
            u32::from(ch),
            config.block,
            u32::from(capture),
            scratch,
            u32::try_from(MAX_BLOCK * usize::from(ch)).unwrap_or(0),
        )
    };
    if rate == 0 {
        STREAMS.with_borrow_mut(|s| s[handle as usize] = None);
        return Err(BackendError::Unavailable(
            "Web Audio is not available (or the shim is not loaded)".into(),
        ));
    }
    STREAMS.with_borrow_mut(|s| {
        if let Some(Some(e)) = s.get_mut(handle as usize) {
            e.rate = rate;
        }
    });
    Ok(DeviceStream::new(
        "webaudio",
        StreamConfig {
            rate,
            channels: ch,
            block: config.block,
        },
        counters,
        Box::new(WebStream(handle)),
    ))
}

/// The Web Audio backend.
#[derive(Debug, Clone, Copy, Default)]
pub struct WebAudio;

impl Backend for WebAudio {
    fn name(&self) -> &'static str {
        "webaudio"
    }

    fn open_output(
        &self,
        config: StreamConfig,
        render: RenderFn,
    ) -> Result<DeviceStream, BackendError> {
        open(config, Callback::Render(render))
    }

    fn open_input(
        &self,
        config: StreamConfig,
        capture: CaptureFn,
    ) -> Result<DeviceStream, BackendError> {
        open(config, Callback::Capture(capture))
    }
}
