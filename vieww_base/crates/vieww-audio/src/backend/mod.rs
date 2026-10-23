//! Audio devices: one interface over every platform's audio system.
//!
//! An audio system is reached through the library the platform ships — a
//! C ABI loaded at run time (JACK, PipeWire, PulseAudio, AAudio), a COM
//! interface (WASAPI), a framework (Core Audio) or the kernel itself
//! (ALSA) — never a binding crate, so this crate keeps its single
//! dependency, and a machine without a given system gets an honest
//! [`BackendError::Unavailable`] instead of a link failure.
//!
//! Every backend has the same shape. [`Backend::open_output`] takes a
//! render callback — handed interleaved `f32` frames to fill, on the audio
//! system's own thread where it has one — and returns a [`DeviceStream`]
//! that reports the configuration the system actually granted (a JACK
//! server's rate is the server's, not the caller's), counts callbacks,
//! frames and xruns, and stops when dropped. [`Backend::open_input`] is
//! the same for capture.
//!
//! [`backends`] lists what this platform offers in preference order, and
//! [`open_output`] / [`open_input`] take the first that works:
//!
//! | Platform | Order |
//! |---|---|
//! | Linux | PipeWire, JACK, PulseAudio, ALSA |
//! | macOS | Core Audio, JACK |
//! | Windows | WASAPI, JACK |
//! | Android | AAudio |
//! | Web | Web Audio |

#[cfg(all(any(unix, windows), not(target_os = "ios")))]
pub(crate) mod dl;
pub mod resilient;

#[cfg(any(target_os = "android", all(unix, vieww_check_all)))]
pub mod aaudio;
#[cfg(all(target_os = "linux", not(target_os = "android")))]
pub mod alsa;
#[cfg(any(target_os = "macos", target_os = "ios", vieww_check_all))]
pub mod coreaudio;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub mod jack;
#[cfg(all(target_os = "linux", not(target_os = "android")))]
pub mod pipewire;
#[cfg(all(target_os = "linux", not(target_os = "android")))]
pub mod pulse;
#[cfg(any(windows, vieww_check_all))]
pub mod wasapi;
#[cfg(any(target_arch = "wasm32", vieww_check_all))]
pub mod web;

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;

/// What to ask an audio system for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamConfig {
    /// Sample rate (a request: see [`DeviceStream::config`]).
    pub rate: u32,
    /// Channels.
    pub channels: u16,
    /// Frames per callback, as a latency target.
    pub block: u32,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            rate: 48_000,
            channels: 2,
            block: 256,
        }
    }
}

/// Facts about the block a callback is handed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockInfo {
    /// Frames in the block.
    pub frames: usize,
    /// Channels (interleaved).
    pub channels: usize,
    /// Sample rate.
    pub rate: u32,
    /// Stream time of the block's first frame, in seconds.
    pub time: f64,
}

/// A device a backend can open: an output, an input, or both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    /// What [`Backend::open_output_on`] takes to open this device — a
    /// backend's own spelling (`hw:1,0`, a sink name, an endpoint id).
    pub id: String,
    /// A name to show a person.
    pub name: String,
    /// Output channels, if known (0 = not an output).
    pub outputs: Option<u16>,
    /// Input channels, if known (0 = not an input).
    pub inputs: Option<u16>,
    /// Whether the system uses it when no device is named.
    pub is_default: bool,
}

/// A render callback: fill the interleaved block (it arrives zeroed).
pub type RenderFn = Box<dyn FnMut(&mut [f32], &BlockInfo) + Send + 'static>;
/// A capture callback: read the interleaved block.
pub type CaptureFn = Box<dyn FnMut(&[f32], &BlockInfo) + Send + 'static>;

/// Why a stream did not open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendError {
    /// The audio system is not on this machine, or not running.
    Unavailable(String),
    /// It is there but refused the stream.
    Refused(String),
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(e) => write!(f, "audio system unavailable: {e}"),
            Self::Refused(e) => write!(f, "audio system refused the stream: {e}"),
        }
    }
}

impl std::error::Error for BackendError {}

/// Counters a stream keeps (shared with its audio thread).
#[derive(Debug, Default)]
pub struct Counters {
    /// Callbacks run.
    pub callbacks: AtomicU64,
    /// Frames rendered or captured.
    pub frames: AtomicU64,
    /// Over- and underruns the audio system reported.
    pub xruns: AtomicU64,
    /// Set when the stream ended on its own (server shut down, device lost).
    pub ended: AtomicBool,
    /// The scheduling class the callback thread runs in, as the OS
    /// reports it on the first callback ([`crate::rt::Level`] as a byte).
    pub priority: AtomicU8,
    /// Whether `priority` has been recorded yet.
    pub priority_known: AtomicBool,
    /// Total nanoseconds spent inside the render callback.
    pub render_ns: AtomicU64,
    /// The longest single render, nanoseconds.
    pub render_max_ns: AtomicU64,
    /// The highest load seen: render time over the block's duration, in
    /// thousandths (1000 = the whole budget used).
    pub load_peak_milli: AtomicU64,
    /// Renders that took longer than their block lasts — each a certain
    /// underrun on a device that waits for nobody.
    pub late: AtomicU64,
    /// Output latency as the audio system reports it, nanoseconds: how
    /// long a sample rendered now takes to reach the device. 0 = not
    /// reported.
    pub latency_ns: AtomicU64,
}

/// A snapshot of [`Counters`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StreamStats {
    /// Callbacks run.
    pub callbacks: u64,
    /// Frames rendered or captured.
    pub frames: u64,
    /// Xruns.
    pub xruns: u64,
    /// Ended on its own.
    pub ended: bool,
    /// The callback thread's scheduling class (`None` before the first
    /// callback).
    pub priority: Option<crate::rt::Level>,
    /// Mean render time per callback.
    pub render_mean: std::time::Duration,
    /// Longest render.
    pub render_max: std::time::Duration,
    /// Peak load: the longest render as a fraction of its block's
    /// duration (1.0 = the whole budget).
    pub load_peak: f64,
    /// Renders that overran their block's duration.
    pub late: u64,
    /// Output latency as the audio system reports it, if it does.
    pub latency: Option<std::time::Duration>,
}

impl Counters {
    /// Count one callback of `frames`.
    pub fn block(&self, frames: usize) {
        self.callbacks.fetch_add(1, Ordering::Relaxed);
        self.frames.fetch_add(frames as u64, Ordering::Relaxed);
    }

    fn snapshot(&self) -> StreamStats {
        let callbacks = self.callbacks.load(Ordering::Relaxed);
        let total = self.render_ns.load(Ordering::Relaxed);
        #[expect(clippy::cast_precision_loss, reason = "a ratio")]
        let load_peak = self.load_peak_milli.load(Ordering::Relaxed) as f64 / 1000.0;
        StreamStats {
            callbacks,
            frames: self.frames.load(Ordering::Relaxed),
            xruns: self.xruns.load(Ordering::Relaxed),
            ended: self.ended.load(Ordering::Relaxed),
            priority: self
                .priority_known
                .load(Ordering::Acquire)
                .then(|| crate::rt::Level::from_u8(self.priority.load(Ordering::Relaxed))),
            render_mean: std::time::Duration::from_nanos(total.checked_div(callbacks).unwrap_or(0)),
            render_max: std::time::Duration::from_nanos(self.render_max_ns.load(Ordering::Relaxed)),
            load_peak,
            late: self.late.load(Ordering::Relaxed),
            latency: match self.latency_ns.load(Ordering::Relaxed) {
                0 => None,
                ns => Some(std::time::Duration::from_nanos(ns)),
            },
        }
    }

    /// Record the latency the audio system reports, in frames at `rate`.
    pub fn set_latency_frames(&self, frames: u64, rate: u32) {
        if rate > 0 {
            self.latency_ns.store(
                frames.saturating_mul(1_000_000_000) / u64::from(rate),
                Ordering::Relaxed,
            );
        }
    }

    /// Record one render of `ns` nanoseconds for a block of `frames` at
    /// `rate`. Lock-free and allocation-free: called on the audio thread.
    pub fn render_timed(&self, ns: u64, frames: usize, rate: u32) {
        self.render_ns.fetch_add(ns, Ordering::Relaxed);
        self.render_max_ns.fetch_max(ns, Ordering::Relaxed);
        let budget_ns = (frames as u64).saturating_mul(1_000_000_000) / u64::from(rate.max(1));
        if let Some(milli) = ns.saturating_mul(1000).checked_div(budget_ns) {
            self.load_peak_milli.fetch_max(milli, Ordering::Relaxed);
            if ns > budget_ns {
                self.late.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

/// Wrap a render callback in panic isolation for the boundary it crosses.
///
/// A panic unwinding out of an `extern "C"` audio callback is a process
/// abort — the audio system owns that thread, and Rust refuses to let an
/// unwind cross the boundary it was called through. The closure behind a
/// [`RenderFn`] is user code doing user things on a thread the user does
/// not control, so every backend wraps its callback here before the first
/// call. A panicking block is caught and rendered as silence, and the
/// stream is flagged ended so [`DeviceStream::wait_for`] releases and
/// `stop` runs; the slot poisons itself, so later blocks cost one atomic
/// load instead of a re-panicked closure per block until teardown.
///
/// **Under `panic = "abort"` there is nothing to catch.** The workspace's
/// `dist` profile — what ships, and what Device Farm measures — aborts on
/// panic, so there a panicking render closure still ends the process; this
/// isolation protects `dev`, `release` and `dist-unwind` builds. An
/// application that wants a misbehaving callback to cost a stream rather
/// than the process builds with `dist-unwind`.
pub(crate) fn isolate_render(render: RenderFn, counters: &Arc<Counters>) -> RenderFn {
    let counters = Arc::clone(counters);
    let dead = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&dead);
    let mut render = render;
    // A promotion made from inside the callback, kept for the stream's
    // life (on Windows, dropping it would revert MMCSS).
    let mut _held: Option<crate::rt::Promotion> = None;
    Box::new(move |out, info| {
        // Subnormals off for the block, the thread's own mode back after —
        // see `crate::denormal`. Two register moves per callback.
        let _ftz = crate::denormal::FlushDenormals::enable();
        if flag.load(Ordering::Acquire) {
            out.fill(0.0);
            return;
        }
        if !counters.priority_known.load(Ordering::Relaxed) {
            // Once, on the callback's own thread: what the OS says it is —
            // an OS-owned real-time thread (Core Audio, JACK, AAudio) and
            // one this crate promoted alike. A library-owned thread left
            // at normal priority (PipeWire's data thread when neither
            // RTKit nor an rtprio limit granted it) is promoted here, once;
            // the promotion's small setup allocation happens on this first
            // callback only, never in steady state.
            let mut level = crate::rt::current_level();
            if level == crate::rt::Level::Normal && info.rate > 0 {
                #[expect(clippy::cast_precision_loss, reason = "a block length")]
                let period =
                    std::time::Duration::from_secs_f64(info.frames as f64 / f64::from(info.rate));
                let p = crate::rt::promote_audio_thread(period);
                level = crate::rt::current_level();
                _held = Some(p);
            }
            counters.priority.store(level.as_u8(), Ordering::Relaxed);
            counters.priority_known.store(true, Ordering::Release);
        }
        let started = std::time::Instant::now();
        let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render(out, info)));
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a render takes far less than 584 years"
        )]
        let ns = started.elapsed().as_nanos() as u64;
        counters.render_timed(ns, info.frames, info.rate);
        match ok {
            Ok(()) => {
                // The device boundary is the last place a non-finite sample
                // can be stopped: a NaN or infinity handed to a float sink
                // is, depending on the driver, silence, a full-scale burst
                // or a muted endpoint. Zero it here, on every backend.
                for s in out.iter_mut() {
                    if !s.is_finite() {
                        *s = 0.0;
                    }
                }
            }
            Err(_) => {
                flag.store(true, Ordering::Release);
                out.fill(0.0);
                counters.ended.store(true, Ordering::Release);
            }
        }
    })
}

/// Wrap a capture callback in panic isolation — the read-side twin of
/// [`isolate_render`]: a panicking capture is caught and the stream
/// flagged ended; later blocks return without invoking the closure.
pub(crate) fn isolate_capture(capture: CaptureFn, counters: &Arc<Counters>) -> CaptureFn {
    let counters = Arc::clone(counters);
    let dead = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&dead);
    let mut capture = capture;
    Box::new(move |data, info| {
        let _ftz = crate::denormal::FlushDenormals::enable();
        if flag.load(Ordering::Acquire) {
            return;
        }
        let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| capture(data, info)));
        match ok {
            Ok(()) => {}
            Err(_) => {
                flag.store(true, Ordering::Release);
                counters.ended.store(true, Ordering::Release);
            }
        }
    })
}

/// What a backend's stream implementation provides.
pub trait StreamImpl: Send {
    /// Stop (idempotent).
    fn stop(&mut self);
}

/// A running stream. Dropping it stops it.
pub struct DeviceStream {
    backend: &'static str,
    config: StreamConfig,
    counters: Arc<Counters>,
    inner: Box<dyn StreamImpl>,
}

impl std::fmt::Debug for DeviceStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceStream")
            .field("backend", &self.backend)
            .field("config", &self.config)
            .field("stats", &self.stats())
            .finish_non_exhaustive()
    }
}

impl DeviceStream {
    /// Wrap a backend's stream.
    #[must_use]
    pub fn new(
        backend: &'static str,
        config: StreamConfig,
        counters: Arc<Counters>,
        inner: Box<dyn StreamImpl>,
    ) -> Self {
        Self {
            backend,
            config,
            counters,
            inner,
        }
    }

    /// The backend's name.
    #[must_use]
    pub const fn backend(&self) -> &'static str {
        self.backend
    }

    /// The configuration actually granted.
    #[must_use]
    pub const fn config(&self) -> StreamConfig {
        self.config
    }

    /// The counters.
    #[must_use]
    pub fn stats(&self) -> StreamStats {
        self.counters.snapshot()
    }

    /// Output latency as the audio system reports it — what an app shows
    /// as "7.4 ms", and what it compensates by when it lines audio up with
    /// pictures or recorded input. `None` until the backend has a figure,
    /// or on a backend that cannot report one.
    #[must_use]
    pub fn latency(&self) -> Option<std::time::Duration> {
        self.stats().latency
    }

    /// Wait until at least `callbacks` callbacks have run, up to `timeout`.
    #[must_use]
    pub fn wait_for(&self, callbacks: u64, timeout: std::time::Duration) -> bool {
        let t0 = std::time::Instant::now();
        while self.stats().callbacks < callbacks {
            if t0.elapsed() > timeout || self.stats().ended {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        true
    }

    /// Stop now (also done on drop).
    pub fn stop(&mut self) {
        self.inner.stop();
    }
}

impl Drop for DeviceStream {
    fn drop(&mut self) {
        self.inner.stop();
    }
}

/// An audio system.
pub trait Backend: Send + Sync {
    /// The devices this backend can open, for a settings page or to pick
    /// a particular interface. Empty when the backend cannot list them
    /// (its default device still opens through
    /// [`open_output`](Self::open_output)).
    ///
    /// # Errors
    /// The audio system is unavailable.
    fn devices(&self) -> Result<Vec<DeviceInfo>, BackendError> {
        Ok(Vec::new())
    }

    /// The id of the device the system currently uses by default, when the
    /// backend can tell — what [`resilient::ResilientStream`] watches to
    /// follow a default that moves (headphones plugged in, an output picked
    /// in the system's settings). `None`: no notion of a default here.
    fn default_device_id(&self) -> Option<String> {
        None
    }

    /// Open playback on a particular device — a [`DeviceInfo::id`] from
    /// [`devices`](Self::devices) — instead of the default.
    ///
    /// # Errors
    /// No such device, the device refused, or this backend cannot address
    /// devices (the default answer).
    fn open_output_on(
        &self,
        device: &str,
        config: StreamConfig,
        render: RenderFn,
    ) -> Result<DeviceStream, BackendError> {
        let _ = (device, config, render);
        Err(BackendError::Refused(format!(
            "{} cannot open a named device",
            self.name()
        )))
    }

    /// Its name (`"jack"`, `"wasapi"` …).
    fn name(&self) -> &'static str;

    /// Open a playback stream.
    ///
    /// # Errors
    /// The system is unavailable or refuses.
    fn open_output(
        &self,
        config: StreamConfig,
        render: RenderFn,
    ) -> Result<DeviceStream, BackendError>;

    /// Open a capture stream.
    ///
    /// # Errors
    /// As [`open_output`](Self::open_output).
    fn open_input(
        &self,
        config: StreamConfig,
        capture: CaptureFn,
    ) -> Result<DeviceStream, BackendError>;
}

/// The backends this platform offers, in preference order.
#[must_use]
#[allow(
    clippy::vec_init_then_push,
    reason = "which pushes run is per-platform cfg; on wasm it is exactly one"
)]
pub fn backends() -> Vec<Box<dyn Backend>> {
    #[allow(unused_mut, reason = "empty on some platforms")]
    let mut v: Vec<Box<dyn Backend>> = Vec::new();
    #[cfg(all(target_os = "linux", not(target_os = "android")))]
    {
        v.push(Box::new(pipewire::PipeWire));
        v.push(Box::new(jack::Jack::default()));
        v.push(Box::new(pulse::Pulse));
        v.push(Box::new(alsa::Alsa::default()));
    }
    #[cfg(target_os = "macos")]
    {
        v.push(Box::new(coreaudio::CoreAudio));
        v.push(Box::new(jack::Jack::default()));
    }
    #[cfg(windows)]
    {
        v.push(Box::new(wasapi::Wasapi));
        v.push(Box::new(jack::Jack::default()));
    }
    #[cfg(target_os = "ios")]
    v.push(Box::new(coreaudio::CoreAudio));
    #[cfg(target_os = "android")]
    v.push(Box::new(aaudio::AAudio));
    #[cfg(target_arch = "wasm32")]
    v.push(Box::new(web::WebAudio));
    v
}

/// A backend by name.
#[must_use]
pub fn backend(name: &str) -> Option<Box<dyn Backend>> {
    backends().into_iter().find(|b| b.name() == name)
}

/// Open playback on the first backend that accepts it. The render
/// callback is built per attempt by `make` (a refused attempt drops it).
///
/// # Errors
/// Every backend failed (the errors, in order).
pub fn open_output(
    config: StreamConfig,
    mut make: impl FnMut() -> RenderFn,
) -> Result<DeviceStream, Vec<(&'static str, BackendError)>> {
    let mut errors = Vec::new();
    for b in backends() {
        match b.open_output(config, make()) {
            Ok(s) => return Ok(s),
            Err(e) => errors.push((b.name(), e)),
        }
    }
    Err(errors)
}

/// Every device every available backend can list, with the backend's name.
/// Backends that are unavailable or cannot list are skipped.
#[must_use]
pub fn devices() -> Vec<(&'static str, DeviceInfo)> {
    backends()
        .into_iter()
        .flat_map(|b| {
            let name = b.name();
            b.devices()
                .unwrap_or_default()
                .into_iter()
                .map(move |d| (name, d))
        })
        .collect()
}

/// Open capture on the first backend that accepts it.
///
/// # Errors
/// Every backend failed (the errors, in order).
pub fn open_input(
    config: StreamConfig,
    mut make: impl FnMut() -> CaptureFn,
) -> Result<DeviceStream, Vec<(&'static str, BackendError)>> {
    let mut errors = Vec::new();
    for b in backends() {
        match b.open_input(config, make()) {
            Ok(s) => return Ok(s),
            Err(e) => errors.push((b.name(), e)),
        }
    }
    Err(errors)
}

/// Tests: whether the environment promises this backend is reachable
/// (`VIEWW_EXPECT_BACKENDS=jack,pulse`), so "unavailable" must fail.
#[cfg(test)]
#[cfg_attr(
    any(target_os = "android", target_os = "ios"),
    allow(dead_code, reason = "no backend test on these targets asks")
)]
pub(crate) fn expected(name: &str) -> bool {
    std::env::var("VIEWW_EXPECT_BACKENDS").is_ok_and(|v| v.split(',').any(|n| n.trim() == name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_platform_lists_its_backends() {
        let names: Vec<&str> = backends().iter().map(|b| b.name()).collect();
        #[cfg(all(target_os = "linux", not(target_os = "android")))]
        assert_eq!(names, ["pipewire", "jack", "pulse", "alsa"]);
        assert!(backend("no-such-system").is_none());
        let _ = names;
    }

    #[test]
    fn opening_with_nothing_available_reports_every_attempt() {
        // With no servers reachable every backend says why; with some
        // running (the CI machine with JACK, say) the first one wins.
        match open_output(StreamConfig::default(), || Box::new(|_, _| {})) {
            Ok(s) => assert!(!s.backend().is_empty()),
            Err(errors) => {
                assert_eq!(errors.len(), backends().len());
                assert!(errors.iter().all(|(_, e)| !e.to_string().is_empty()));
            }
        }
    }

    #[test]
    fn an_isolated_render_callback_panics_once_then_goes_silent() {
        // The contract every backend now wraps its callback in: a panic is
        // caught (not an abort out of `extern "C"`), the block is zeroed,
        // the stream is flagged ended, and the poisoned slot never invokes
        // the closure again — one message on stderr, not one per block.
        use std::sync::atomic::AtomicU64;
        let counters = Arc::new(Counters::default());
        let calls = Arc::new(AtomicU64::new(0));
        let seen = Arc::clone(&calls);
        let mut render = isolate_render(
            Box::new(move |_out: &mut [f32], _info: &BlockInfo| {
                seen.fetch_add(1, Ordering::Relaxed);
                panic!("user code misbehaved on the audio thread");
            }),
            &counters,
        );
        // Keep the expected-panic message out of the test log.
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let mut out = [0.5f32; 8];
        let info = BlockInfo {
            frames: 4,
            channels: 2,
            rate: 48_000,
            time: 0.0,
        };
        render(&mut out, &info);
        std::panic::set_hook(prev);
        assert_eq!(calls.load(Ordering::Relaxed), 1, "the closure ran once");
        assert!(out.iter().all(|&s| s == 0.0), "the block was zeroed");
        assert!(
            counters.ended.load(Ordering::Acquire),
            "the stream is flagged ended"
        );
        render(&mut out, &info);
        assert_eq!(
            calls.load(Ordering::Relaxed),
            1,
            "a poisoned slot never re-invokes the closure"
        );
        assert!(
            out.iter().all(|&s| s == 0.0),
            "a poisoned slot renders silence"
        );
    }

    #[test]
    fn an_isolated_capture_callback_ends_the_stream() {
        use std::sync::atomic::AtomicU64;
        let counters = Arc::new(Counters::default());
        let calls = Arc::new(AtomicU64::new(0));
        let seen = Arc::clone(&calls);
        let mut capture = isolate_capture(
            Box::new(move |_data: &[f32], _info: &BlockInfo| {
                seen.fetch_add(1, Ordering::Relaxed);
                panic!("capture misbehaved");
            }),
            &counters,
        );
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let data = [0.0f32; 4];
        let info = BlockInfo {
            frames: 2,
            channels: 2,
            rate: 48_000,
            time: 0.0,
        };
        capture(&data, &info);
        std::panic::set_hook(prev);
        capture(&data, &info);
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert!(counters.ended.load(Ordering::Acquire));
    }
}
