//! A stream that survives its device: hot-plug and default-device changes.
//!
//! Unplug a USB interface, switch Bluetooth headphones off, restart the
//! sound server — and a plain [`DeviceStream`](super::DeviceStream) ends for good (its
//! [`StreamStats::ended`](super::StreamStats::ended) is set and it plays
//! nothing more). A [`ResilientStream`] instead watches its stream from a
//! supervisor thread and, when the stream ends or the system's default
//! output changes, opens a new one — on the new default, or on the same
//! named device when it comes back — and hands it the **same** render
//! closure, so the app's synth, sequencer position and effect tails carry
//! straight on.
//!
//! The closure lives in a shared slot. Each device stream gets a thin
//! forwarding closure that takes it out of the slot on its first callback
//! (one uncontended lock, at stream start) and puts it back when the
//! stream is dropped. The audio thread never waits on a lock in steady
//! state.
//!
//! Detection: a stream ending is universal (every backend sets `ended` on
//! device loss or server shutdown). Default-device changes come from
//! [`Backend::default_device_id`]: PulseAudio and PipeWire move streams to
//! a new default themselves (the server does it), WASAPI and Core Audio
//! report the default endpoint so a change can be followed, JACK and ALSA
//! have no notion of one.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use super::{Backend, BackendError, BlockInfo, RenderFn, StreamConfig, StreamStats};

/// How often the supervisor checks its stream.
const POLL: Duration = Duration::from_millis(100);
/// How often it asks for the default device (a round trip on some
/// systems, so less often).
const DEFAULT_POLL: Duration = Duration::from_millis(1_000);

/// The app's closure, shared across successive device streams.
type Slot = Arc<Mutex<Option<RenderFn>>>;

/// A forwarding closure: borrows the app's closure from `slot` for the
/// life of one device stream. If the app's closure panics, `poisoned` is
/// set and the closure is dropped instead of handed back: its state may be
/// half-updated, so no later stream runs it.
fn forwarder(slot: &Slot, poisoned: &Arc<AtomicBool>) -> RenderFn {
    struct Lease {
        slot: Slot,
        held: Option<RenderFn>,
        poisoned: Arc<AtomicBool>,
    }
    impl Drop for Lease {
        fn drop(&mut self) {
            if let Some(f) = self.held.take() {
                if self.poisoned.load(Ordering::Acquire) {
                    return; // dropped here, never reused
                }
                if let Ok(mut s) = self.slot.lock() {
                    *s = Some(f);
                }
            }
        }
    }
    /// Set `poisoned` if dropped during an unwind out of the app's closure.
    struct Unwinding<'a>(&'a AtomicBool);
    impl Drop for Unwinding<'_> {
        fn drop(&mut self) {
            if std::thread::panicking() {
                self.0.store(true, Ordering::Release);
            }
        }
    }
    let mut lease = Lease {
        slot: Arc::clone(slot),
        held: None,
        poisoned: Arc::clone(poisoned),
    };
    Box::new(move |out: &mut [f32], info: &BlockInfo| {
        if lease.held.is_none() && !lease.poisoned.load(Ordering::Acquire) {
            // First callback of this stream: take the closure (the previous
            // stream returned it when it was dropped).
            if let Ok(mut s) = lease.slot.try_lock() {
                lease.held = s.take();
            }
        }
        match &mut lease.held {
            Some(f) => {
                let _watch = Unwinding(&lease.poisoned);
                f(out, info);
            }
            None => out.fill(0.0),
        }
    })
}

/// Which device to follow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// The system default, wherever it moves.
    Default,
    /// A named device ([`super::DeviceInfo::id`]); when it disappears the
    /// stream waits for it to return (falling back to the default after
    /// `fallback_after`, if set), and moves back to it when it does —
    /// also when a sound server had moved the stream elsewhere meanwhile.
    Named {
        /// The device id.
        id: String,
        /// Switch to the default after this long without the device.
        fallback_after: Option<Duration>,
    },
}

/// Shared state between the supervisor and its owner.
#[derive(Debug, Default)]
struct State {
    stop: AtomicBool,
    reopens: AtomicU64,
    failed_opens: AtomicU64,
    following_default: AtomicBool,
    /// The app's closure panicked: nothing more is opened.
    poisoned: Arc<AtomicBool>,
    current: Mutex<Option<StreamStats>>,
}

/// A device stream that reopens itself on device loss or default change.
#[derive(Debug)]
pub struct ResilientStream {
    state: Arc<State>,
    thread: Option<JoinHandle<()>>,
}

impl ResilientStream {
    /// Open `render` on `backend` (following `target`), and keep it
    /// playing through device changes.
    ///
    /// # Errors
    /// The first open failed — the device (or the audio system) is not
    /// there now, so there is nothing to keep alive.
    pub fn open(
        backend: Box<dyn Backend>,
        target: Target,
        config: StreamConfig,
        render: RenderFn,
    ) -> Result<Self, BackendError> {
        let slot: Slot = Arc::new(Mutex::new(Some(render)));
        let poisoned = Arc::new(AtomicBool::new(false));
        let p2 = Arc::clone(&poisoned);
        let open_on = move |b: &dyn Backend, t: &Target, slot: &Slot| match t {
            Target::Default => b.open_output(config, forwarder(slot, &p2)),
            Target::Named { id, .. } => b.open_output_on(id, config, forwarder(slot, &p2)),
        };
        let first = open_on(backend.as_ref(), &target, &slot)?;
        let state = Arc::new(State {
            following_default: AtomicBool::new(target == Target::Default),
            poisoned,
            ..State::default()
        });
        let st = Arc::clone(&state);
        let thread = std::thread::Builder::new()
            .name("vieww-audio-supervisor".into())
            .spawn(move || {
                let mut stream = Some(first);
                let mut default = backend.default_device_id();
                let mut since_default = Duration::ZERO;
                let mut lost_for = Duration::ZERO;
                let mut backoff = POLL;
                let mut named_gone = false;
                while !st.stop.load(Ordering::Acquire) {
                    std::thread::sleep(POLL);
                    since_default += POLL;
                    let ended = stream.as_ref().is_none_or(|s| s.stats().ended);
                    if let Some(s) = &stream {
                        if let Ok(mut c) = st.current.lock() {
                            *c = Some(s.stats());
                        }
                    }
                    let mut moved = false;
                    if since_default >= DEFAULT_POLL {
                        since_default = Duration::ZERO;
                        match &target {
                            Target::Default => {
                                let now = backend.default_device_id();
                                if now.is_some() && now != default {
                                    moved = default.is_some();
                                    default = now;
                                }
                            }
                            // A named device that left and came back: go
                            // back to it. (Sound servers move a stream whose
                            // sink vanished to their fallback rather than
                            // ending it — audio keeps going meanwhile — so
                            // the return has to be watched for.)
                            Target::Named { id, .. } => {
                                if let Ok(list) = backend.devices() {
                                    let present = list.iter().any(|d| &d.id == id);
                                    if !present {
                                        named_gone = true;
                                    } else if named_gone
                                        || st.following_default.load(Ordering::Relaxed)
                                    {
                                        moved = true;
                                    }
                                }
                            }
                        }
                    }
                    if st.poisoned.load(Ordering::Acquire) {
                        // The closure panicked: the stream stays ended
                        // (silent), and nothing is reopened with it.
                        continue;
                    }
                    if !(ended || moved) {
                        lost_for = Duration::ZERO;
                        backoff = POLL;
                        continue;
                    }
                    // Drop the old stream first: it returns the closure.
                    drop(stream.take());
                    lost_for += backoff;
                    let t = match &target {
                        Target::Named {
                            fallback_after: Some(after),
                            ..
                        } if lost_for >= *after => {
                            st.following_default.store(true, Ordering::Relaxed);
                            Target::Default
                        }
                        t => t.clone(),
                    };
                    match open_on(backend.as_ref(), &t, &slot) {
                        Ok(s) => {
                            stream = Some(s);
                            st.reopens.fetch_add(1, Ordering::Relaxed);
                            st.following_default
                                .store(t == Target::Default, Ordering::Relaxed);
                            named_gone = false;
                            lost_for = Duration::ZERO;
                            backoff = POLL;
                        }
                        Err(_) => {
                            st.failed_opens.fetch_add(1, Ordering::Relaxed);
                            // Back off to a second between attempts while
                            // the device is gone.
                            std::thread::sleep(backoff);
                            backoff = (backoff * 2).min(Duration::from_secs(1));
                        }
                    }
                }
                drop(stream);
            })
            .map_err(|e| BackendError::Refused(format!("supervisor thread: {e}")))?;
        Ok(Self {
            state,
            thread: Some(thread),
        })
    }

    /// How many times the stream has been reopened after a loss or a
    /// default-device change.
    #[must_use]
    pub fn reopens(&self) -> u64 {
        self.state.reopens.load(Ordering::Relaxed)
    }

    /// Whether the render closure panicked. The stream is then silent for
    /// good — the closure's state may be half-updated, so it is never run
    /// again; open a new stream with a fresh closure.
    #[must_use]
    pub fn panicked(&self) -> bool {
        self.state.poisoned.load(Ordering::Acquire)
    }

    /// Attempts to reopen that failed (the device was still gone).
    #[must_use]
    pub fn failed_opens(&self) -> u64 {
        self.state.failed_opens.load(Ordering::Relaxed)
    }

    /// Whether the stream is on the system default (from the start, or
    /// while a named device's fallback lasts).
    #[must_use]
    pub fn following_default(&self) -> bool {
        self.state.following_default.load(Ordering::Relaxed)
    }

    /// The current device stream's statistics (refreshed every 100 ms;
    /// `None` until the first refresh).
    #[must_use]
    pub fn stats(&self) -> Option<StreamStats> {
        self.state.current.lock().ok().and_then(|c| *c)
    }
}

impl Drop for ResilientStream {
    fn drop(&mut self) {
        self.state.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            t.join().ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{CaptureFn, Counters, DeviceStream, StreamImpl};
    use super::*;
    use std::sync::atomic::AtomicU32;

    /// A fake backend whose "device" can be unplugged: each stream runs a
    /// thread calling the render closure until the device generation it
    /// opened on is gone.
    #[derive(Default)]
    struct Fake {
        generation: Arc<AtomicU32>,
        present: Arc<AtomicBool>,
        default_id: Arc<Mutex<String>>,
    }

    struct FakeStream {
        stop: Arc<AtomicBool>,
        t: Option<JoinHandle<()>>,
    }

    impl StreamImpl for FakeStream {
        fn stop(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(t) = self.t.take() {
                t.join().ok();
            }
        }
    }

    impl Backend for Fake {
        fn name(&self) -> &'static str {
            "fake"
        }
        fn open_output(
            &self,
            config: StreamConfig,
            render: RenderFn,
        ) -> Result<DeviceStream, BackendError> {
            if !self.present.load(Ordering::Acquire) {
                return Err(BackendError::Unavailable("unplugged".into()));
            }
            let counters = Arc::new(Counters::default());
            // As every real backend does: a panic is caught, then silence
            // and `ended`.
            let mut render = super::super::isolate_render(render, &counters);
            let stop = Arc::new(AtomicBool::new(false));
            let (c2, s2, g, gen) = (
                counters.clone(),
                stop.clone(),
                self.generation.clone(),
                self.generation.load(Ordering::Acquire),
            );
            let t = std::thread::spawn(move || {
                let mut buf = vec![0.0f32; 128];
                let info = BlockInfo {
                    frames: 64,
                    channels: 2,
                    rate: 48_000,
                    time: 0.0,
                };
                while !s2.load(Ordering::Acquire) {
                    if g.load(Ordering::Acquire) != gen {
                        c2.ended.store(true, Ordering::Release);
                        return;
                    }
                    render(&mut buf, &info);
                    c2.block(64);
                    std::thread::sleep(Duration::from_millis(1));
                }
            });
            Ok(DeviceStream::new(
                "fake",
                config,
                counters,
                Box::new(FakeStream { stop, t: Some(t) }),
            ))
        }
        fn open_input(&self, _: StreamConfig, _: CaptureFn) -> Result<DeviceStream, BackendError> {
            Err(BackendError::Refused("no input".into()))
        }
        fn default_device_id(&self) -> Option<String> {
            self.default_id.lock().ok().map(|s| s.clone())
        }
    }

    #[test]
    fn the_closure_survives_unplug_and_replug() {
        let fake = Fake::default();
        fake.present.store(true, Ordering::Release);
        *fake.default_id.lock().unwrap() = "speakers".into();
        let (generation, present, default_id) = (
            fake.generation.clone(),
            fake.present.clone(),
            fake.default_id.clone(),
        );
        // The app's state: a counter only the closure advances.
        let calls = Arc::new(AtomicU32::new(0));
        let c2 = calls.clone();
        let rs = ResilientStream::open(
            Box::new(fake),
            Target::Default,
            StreamConfig::default(),
            Box::new(move |_, _| {
                c2.fetch_add(1, Ordering::Relaxed);
            }),
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(200));
        let before = calls.load(Ordering::Relaxed);
        assert!(before > 10, "playing");

        // Unplug: the stream ends; nothing to reopen on for a while.
        present.store(false, Ordering::Release);
        generation.fetch_add(1, Ordering::AcqRel);
        std::thread::sleep(Duration::from_millis(500));
        assert!(rs.failed_opens() > 0, "retrying while unplugged");
        let during = calls.load(Ordering::Relaxed);

        // Replug: playback resumes with the same closure.
        present.store(true, Ordering::Release);
        std::thread::sleep(Duration::from_millis(1_600));
        assert_eq!(rs.reopens(), 1);
        assert!(
            calls.load(Ordering::Relaxed) > during + 10,
            "the same closure plays again"
        );

        // A new default device: followed.
        *default_id.lock().unwrap() = "headphones".into();
        std::thread::sleep(Duration::from_millis(1_500));
        assert_eq!(rs.reopens(), 2, "followed the default");
        assert!(rs.following_default());
        assert!(rs.stats().is_some_and(|s| s.callbacks > 0));
    }

    #[test]
    fn a_panicked_closure_is_never_run_again() {
        let fake = Fake::default();
        fake.present.store(true, Ordering::Release);
        *fake.default_id.lock().unwrap() = "speakers".into();
        let default_id = fake.default_id.clone();
        /// Counts its own drop: the closure's state, freed or kept.
        struct Alive(Arc<AtomicU32>);
        impl Drop for Alive {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
        let dropped = Arc::new(AtomicU32::new(0));
        let alive = Alive(dropped.clone());
        let calls = Arc::new(AtomicU32::new(0));
        let c2 = calls.clone();
        let rs = ResilientStream::open(
            Box::new(fake),
            Target::Default,
            StreamConfig::default(),
            Box::new(move |_, _| {
                let _keep = &alive;
                assert!(
                    c2.fetch_add(1, Ordering::Relaxed) < 20,
                    "a bug in the app's render"
                );
            }),
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(rs.panicked());
        let after = calls.load(Ordering::Relaxed);
        // A default change would normally reopen; a poisoned stream stays
        // ended and silent.
        *default_id.lock().unwrap() = "headphones".into();
        std::thread::sleep(Duration::from_millis(1_400));
        assert_eq!(rs.reopens(), 0, "nothing reopened with a panicked closure");
        assert_eq!(calls.load(Ordering::Relaxed), after, "never called again");
        drop(rs);
        assert_eq!(
            dropped.load(Ordering::Relaxed),
            1,
            "its state was freed, once"
        );
    }

    #[test]
    fn a_first_open_that_fails_is_an_error() {
        let fake = Fake::default();
        assert!(ResilientStream::open(
            Box::new(fake),
            Target::Default,
            StreamConfig::default(),
            Box::new(|_, _| {})
        )
        .is_err());
    }
}
