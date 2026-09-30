//! Shared values and a UI-thread animator — Reanimated's worklets, Core
//! Animation's render server.
//!
//! # The problem the document names
//!
//! §2.12 and §5.2: React Native's old `Animated` API computed every frame on
//! the JavaScript thread, so a busy JS thread dropped frames. Reanimated
//! moves the animation *off* that thread: a **worklet** runs on the UI
//! thread and writes a **shared value** that both threads can read. The
//! 2026 SwiftUI finding the document quotes is the same lesson from the
//! other side — animations advanced in-process halt when the main thread
//! blocks; Core Animation's, advanced in the render server, do not.
//!
//! vieww's frame loop is single-threaded by design (the element tree is not
//! `Send`), so this module gives the Reanimated shape *beside* it:
//!
//! * [`SharedValue`] — an `f32` in an atomic, cloneable across threads,
//!   lock-free to read and write from either side;
//! * [`UiThread`] — a thread that ticks registered **worklets** (`Send`
//!   closures of elapsed time) at a fixed frame interval against its own
//!   monotonic clock, independent of whatever the build thread is doing;
//! * [`with_timing`] / [`with_spring`] — the two Reanimated animation
//!   factories, as worklets;
//! * [`UiThread::run_on_js`]'s channel — worklets send messages back
//!   (`runOnJS`) that the build thread drains when it is free.
//!
//! The build thread reads the shared values in `build` or in a painter, and
//! a compositor-only property (a transform, an opacity) keeps moving even
//! while a rebuild is stuck — which is the whole point, and what the test
//! below demonstrates by *blocking* the calling thread.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::curve::Curve;

/// An `f32` both threads can read and write without a lock.
#[derive(Debug, Clone, Default)]
pub struct SharedValue(Arc<AtomicU32>);

impl SharedValue {
    #[must_use]
    pub fn new(v: f32) -> Self {
        Self(Arc::new(AtomicU32::new(v.to_bits())))
    }

    #[must_use]
    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Acquire))
    }

    pub fn set(&self, v: f32) {
        self.0.store(v.to_bits(), Ordering::Release);
    }
}

/// A worklet: given time since it was registered, write shared values;
/// return `false` when finished.
pub type Worklet = Box<dyn FnMut(Duration) -> bool + Send>;

/// Move `value` from its current reading to `target` over `duration` along
/// `curve` — Reanimated's `withTiming`.
#[must_use]
pub fn with_timing(value: &SharedValue, target: f32, duration: Duration, curve: Curve) -> Worklet {
    let v = value.clone();
    let mut from: Option<f32> = None;
    Box::new(move |elapsed| {
        let start = *from.get_or_insert_with(|| v.get());
        let t = if duration.is_zero() { 1.0 } else { (elapsed.as_secs_f32() / duration.as_secs_f32()).min(1.0) };
        v.set(start + (target - start) * curve.transform(t));
        t < 1.0
    })
}

/// A damped spring toward `target` — Reanimated's `withSpring`. Integrated
/// in fixed 1 ms substeps so the result does not depend on the frame rate.
#[must_use]
pub fn with_spring(value: &SharedValue, target: f32, stiffness: f32, damping: f32) -> Worklet {
    let v = value.clone();
    let mut state: Option<(f32, f32, Duration)> = None;
    Box::new(move |elapsed| {
        let (mut x, mut vel, mut done) = state.unwrap_or((v.get(), 0.0, Duration::ZERO));
        let step = Duration::from_millis(1);
        while done + step <= elapsed {
            let a = -stiffness * (x - target) - damping * vel;
            vel += a * 0.001;
            x += vel * 0.001;
            done += step;
        }
        state = Some((x, vel, done));
        v.set(x);
        !((x - target).abs() < 1e-3 && vel.abs() < 1e-3)
    })
}

struct Registered {
    worklet: Worklet,
    started: Option<Instant>,
}

/// A dedicated animation thread. See the [module docs](self).
pub struct UiThread {
    pending: Arc<Mutex<Vec<Worklet>>>,
    running: Arc<AtomicBool>,
    frames: Arc<AtomicU64>,
    active: Arc<AtomicU64>,
    to_js: Sender<String>,
    from_ui: Receiver<String>,
    handle: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for UiThread {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UiThread")
            .field("frames", &self.frames())
            .field("active", &self.active())
            .finish_non_exhaustive()
    }
}

impl UiThread {
    /// Start the thread, ticking every `frame` (e.g. 16.67 ms).
    #[must_use]
    pub fn start(frame: Duration) -> Self {
        let pending: Arc<Mutex<Vec<Worklet>>> = Arc::default();
        let running = Arc::new(AtomicBool::new(true));
        let frames = Arc::new(AtomicU64::new(0));
        let active = Arc::new(AtomicU64::new(0));
        let (to_js, from_ui) = channel();
        let handle = {
            let (pending, running, frames, active) = (pending.clone(), running.clone(), frames.clone(), active.clone());
            std::thread::Builder::new()
                .name("vieww-ui-worklets".into())
                .spawn(move || {
                    let mut live: Vec<Registered> = Vec::new();
                    let mut next = Instant::now();
                    while running.load(Ordering::Acquire) {
                        if let Ok(mut p) = pending.lock() {
                            live.extend(p.drain(..).map(|w| Registered { worklet: w, started: None }));
                        }
                        let now = Instant::now();
                        live.retain_mut(|r| {
                            let start = *r.started.get_or_insert(now);
                            (r.worklet)(now - start)
                        });
                        active.store(live.len() as u64, Ordering::Release);
                        frames.fetch_add(1, Ordering::AcqRel);
                        next += frame;
                        let now = Instant::now();
                        if next > now {
                            std::thread::sleep(next - now);
                        } else {
                            next = now;
                        }
                    }
                })
                .expect("spawning the worklet thread")
        };
        Self {
            pending,
            running,
            frames,
            active,
            to_js,
            from_ui,
            handle: Some(handle),
        }
    }

    /// Queue a worklet (`runOnUI`); it starts on the thread's next frame.
    pub fn run(&self, worklet: Worklet) {
        if let Ok(mut p) = self.pending.lock() {
            p.push(worklet);
        }
    }

    /// A sender worklets can capture to message the build thread
    /// (`runOnJS`).
    #[must_use]
    pub fn run_on_js(&self) -> Sender<String> {
        self.to_js.clone()
    }

    /// Messages sent back since the last drain.
    #[must_use]
    pub fn drain_messages(&self) -> Vec<String> {
        self.from_ui.try_iter().collect()
    }

    /// Frames the thread has ticked.
    #[must_use]
    pub fn frames(&self) -> u64 {
        self.frames.load(Ordering::Acquire)
    }

    /// Worklets still running.
    #[must_use]
    pub fn active(&self) -> u64 {
        self.active.load(Ordering::Acquire)
    }

    /// Stop and join the thread.
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Release);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for UiThread {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_values_cross_threads() {
        let v = SharedValue::new(1.5);
        let w = v.clone();
        std::thread::spawn(move || w.set(7.25)).join().unwrap();
        assert_eq!(v.get(), 7.25);
    }

    #[test]
    fn animation_advances_while_the_calling_thread_is_blocked() {
        let mut ui = UiThread::start(Duration::from_millis(5));
        let x = SharedValue::new(0.0);
        ui.run(with_timing(&x, 100.0, Duration::from_millis(150), Curve::Linear));
        // Block this thread outright — the stand-in for a stuck rebuild.
        std::thread::sleep(Duration::from_millis(80));
        let mid = x.get();
        assert!(mid > 10.0 && mid < 100.0, "moved while blocked: {mid}");
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(x.get(), 100.0);
        assert!(ui.frames() > 20);
        assert_eq!(ui.active(), 0, "finished worklets are dropped");
        ui.stop();
    }

    #[test]
    fn springs_settle_and_messages_come_back() {
        let mut ui = UiThread::start(Duration::from_millis(4));
        let x = SharedValue::new(0.0);
        let tx = ui.run_on_js();
        let mut spring = with_spring(&x, 1.0, 200.0, 20.0);
        let mut sent = false;
        ui.run(Box::new(move |t| {
            let going = spring(t);
            if !going && !sent {
                sent = true;
                let _ = tx.send("settled".into());
            }
            going
        }));
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline && ui.drain_messages().is_empty() {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!((x.get() - 1.0).abs() < 1e-2, "{}", x.get());
        ui.stop();
    }

    #[test]
    fn spring_is_frame_rate_independent() {
        let a = SharedValue::new(0.0);
        let b = SharedValue::new(0.0);
        let mut wa = with_spring(&a, 1.0, 150.0, 8.0);
        let mut wb = with_spring(&b, 1.0, 150.0, 8.0);
        for i in 1..=30 {
            wa(Duration::from_millis(i * 10));
        }
        for i in 1..=10 {
            wb(Duration::from_millis(i * 30));
        }
        assert!((a.get() - b.get()).abs() < 1e-5);
    }
}
