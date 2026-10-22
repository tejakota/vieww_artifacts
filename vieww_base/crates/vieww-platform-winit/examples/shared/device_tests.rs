//! Assertions that only a device can answer, run inside the demo app.
//!
//! Shared by the Android and iOS entry points, like the demo tree beside it.
//!
//! # Why this is not a `#[test]`
//!
//! Everything here needs a *window*: a surface with a real size, a display
//! density the platform chose, a cutout the hardware has, and frames that were
//! actually presented to a compositor. `cargo test --target aarch64-linux-android`
//! already runs the portable suite on the device through `ci/mobile/adb-runner.sh`, and
//! that covers every crate that can be tested without one — which is all of them
//! except this bridge. A pushed test binary has no activity, so it has no
//! window, so the interesting half is unreachable from there.
//!
//! So the harness is the application. It runs the checks against the same tree a
//! human is looking at, and reports on a sentinel line that `ci/mobile/device-suite.sh`
//! greps for. Same shape as `adb-runner`'s `__VIEWW_EXIT__`, for the same
//! reason: an exit status that has to survive `adb shell` cannot be trusted, and
//! a sentinel can.
//!
//! # What is checked here and what is checked by the script
//!
//! In here: invariants that must hold on **any** device, so a failure is a bug
//! rather than a fact about the hardware. Zero insets are legal — a phone with
//! no cutout is a real phone — so "the insets are non-zero" is *not* asserted
//! here. The measured values are logged instead, and the script asserts what the
//! operator knows about the device in front of them.
//!
//! That split is the whole design. An assertion that fails on a legitimate
//! device teaches people to ignore the suite.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use vieww_element::Signal;
use vieww_foundation::ViewMetrics;
use vieww_platform_winit::FrameLog;
use vieww_widget::prelude::*;

use crate::screen::{MENU_ITEMS, MENU_LABEL, TAP_LABEL};

/// How many frames to watch before reporting.
///
/// Ten seconds at 60Hz, and the number is load-bearing twice over.
///
/// **It outruns the warm-up.** `FrameLog` keeps the last 240 frames, so
/// reporting at 600 leaves a window containing none of the startup. That
/// matters more than it sounds: on an Adreno 612 the first frame measured
/// **480ms** — vello compiling its shaders on the device — against a median of
/// 5.3ms. Reporting at 120 put that frame inside the window and failed a budget
/// check on an application that was, from frame two onward, comfortably fast.
/// A first frame is expensive on every GPU and asserting otherwise is asserting
/// something false.
///
/// **It outruns the script.** `ci/mobile/device-suite.sh` has to tap the screen before
/// the report goes out, and it cannot tap until the app is up. At 120 frames
/// the report was already gone by the time the tap landed, so
/// `a_tap_reached_a_handler` was simply missing from a passing run — the worst
/// kind of absence, one that looks like a check nobody wrote.
const FRAMES: u64 = 600;

/// How many frames this run watches: [`FRAMES`], or more for the sustained
/// pass — `VIEWW_SUITE_FRAMES` (Android: `debug.vieww.frames`), never fewer
/// than [`FRAMES`]. 3600 is a minute at 60 Hz: long enough for a phone to
/// warm up and throttle, which ten seconds never is.
fn frames() -> u64 {
    static N: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *N.get_or_init(|| {
        std::env::var("VIEWW_SUITE_FRAMES")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map_or(FRAMES, |n| n.max(FRAMES))
    })
}

/// How long, at least, this run watches: `VIEWW_SUITE_SECONDS` (Android:
/// `debug.vieww.seconds`), 0 when unset. The sustained pass asks for a
/// minute by the clock rather than a frame count: 3600 frames was a minute
/// on a phone that holds 60 fps and five on one that draws the stress
/// screen at 12 (Device Farm, 2026-10-22: the pass outran its deadline on
/// every phone and reported nothing).
fn seconds() -> u64 {
    static N: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *N.get_or_init(|| {
        std::env::var("VIEWW_SUITE_SECONDS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(0)
    })
}

/// What the suite watches while frames go by, for the report: the process's
/// CPU time across the measured window, how hot the device got, and — on a
/// sustained run — how the first [`FRAMES`] frames compared with the last.
#[derive(Debug, Default)]
struct Watch {
    /// CPU time and frame count where the measured window (the frame log's
    /// last 240 frames) began.
    window_start: Option<(Duration, u64, std::time::Instant)>,
    /// The first thermal reading, the latest, and the worst seen.
    thermal_first: Option<vieww_platform_winit::ThermalReading>,
    thermal_last: Option<vieww_platform_winit::ThermalReading>,
    worst_status: Option<i32>,
    worst_headroom: Option<f32>,
    hottest: Option<f32>,
    last_sample: Option<std::time::Instant>,
    started: Option<std::time::Instant>,
    /// On a sustained run, the frame log at frame [`FRAMES`]: median work,
    /// frames over budget, samples.
    early: Option<(Duration, usize, usize)>,
}

impl Watch {
    /// Once per frame. Thermal readings are taken once a second, the rate
    /// Android answers `AThermal_getThermalHeadroom` at.
    fn frame(&mut self, log: &FrameLog) {
        let now = std::time::Instant::now();
        self.started.get_or_insert(now);
        if self
            .last_sample
            .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(1))
        {
            self.last_sample = Some(now);
            let reading = vieww_platform_winit::thermal_reading();
            if !reading.is_empty() {
                self.thermal_first.get_or_insert(reading);
                self.thermal_last = Some(reading);
                if let Some(status) = reading.status {
                    self.worst_status = Some(self.worst_status.map_or(status, |w| w.max(status)));
                }
                if let Some(headroom) = reading.headroom {
                    self.worst_headroom =
                        Some(self.worst_headroom.map_or(headroom, |w| w.max(headroom)));
                }
                if let Some(celsius) = reading.hottest_celsius {
                    self.hottest = Some(self.hottest.map_or(celsius, |w| w.max(celsius)));
                }
            }
        }
        let window = 240.min(frames());
        // The measured window is the run's last stretch: the last 240 frames
        // of a counted run, the last five seconds of a timed one.
        let window_open = if seconds() > 0 {
            self.elapsed() + Duration::from_secs(5) >= Duration::from_secs(seconds())
        } else {
            log.frames() >= frames() - window
        };
        if self.window_start.is_none() && window_open {
            if let Some(cpu) = vieww_platform_winit::process_cpu_time() {
                self.window_start = Some((cpu, log.frames(), now));
            }
        }
        if (frames() > FRAMES || seconds() > 0) && self.early.is_none() && log.frames() >= FRAMES {
            let mut samples = log.work_samples();
            if !samples.is_empty() {
                samples.sort_unstable();
                let over = samples.iter().filter(|w| **w > log.budget()).count();
                self.early = Some((samples[samples.len() / 2], over, samples.len()));
            }
        }
    }

    /// How long the run has been watched.
    fn elapsed(&self) -> Duration {
        self.started.map_or(Duration::ZERO, |s| s.elapsed())
    }

    /// Whether the run has watched long enough to report: its frames, and
    /// its seconds when it asked for some.
    fn done(&self, log: &FrameLog) -> bool {
        log.frames() >= frames() && self.elapsed() >= Duration::from_secs(seconds())
    }

    /// The report's lines about the window just measured.
    fn observe(&self, results: &mut Results, log: &FrameLog) {
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        // **CPU per frame, every thread** — what the battery pays on the CPU
        // side, which frame time does not say (four threads drawing a frame in
        // 5 ms cost more than one drawing it in 8). Device Farm phones are on
        // a charger, so this and `gpu_passes` stand in for battery drain.
        if let (Some((start_cpu, start_frames, start_at)), Some(cpu)) =
            (self.window_start, vieww_platform_winit::process_cpu_time())
        {
            let frames_in = log.frames().saturating_sub(start_frames).max(1);
            let used = cpu.saturating_sub(start_cpu);
            let wall = start_at.elapsed();
            #[expect(clippy::cast_precision_loss, reason = "frame counts")]
            let per_frame = ms(used) / frames_in as f64;
            results.observe(
                "cpu_time",
                format!(
                    "process CPU {per_frame:.2} ms per frame over the last {frames_in} frames, every \
                     thread ({:.0}% of one core)",
                    100.0 * used.as_secs_f64() / wall.as_secs_f64().max(1e-6),
                ),
            );
        }
        // **How hot it got.** A ten-second pass never warms a phone; the
        // sustained pass does, and this says how far.
        let mut thermal = Vec::new();
        if let (Some(first), Some(last)) = (self.thermal_first, self.thermal_last) {
            if let (Some(a), Some(b)) = (first.status, last.status) {
                thermal.push(format!(
                    "status {} -> {} (worst {})",
                    vieww_platform_winit::thermal_status_name(a),
                    vieww_platform_winit::thermal_status_name(b),
                    vieww_platform_winit::thermal_status_name(self.worst_status.unwrap_or(b)),
                ));
            }
            if let (Some(a), Some(b)) = (first.headroom, last.headroom) {
                thermal.push(format!(
                    "headroom {a:.2} -> {b:.2} (highest {:.2}; 1.0 is severe throttling)",
                    self.worst_headroom.unwrap_or(b)
                ));
            }
            if let (Some(a), Some(b)) = (first.hottest_celsius, last.hottest_celsius) {
                thermal.push(format!(
                    "hottest zone {a:.1} -> {b:.1} °C (peak {:.1})",
                    self.hottest.unwrap_or(b)
                ));
            }
        }
        let over = self
            .started
            .map_or(Duration::ZERO, |s| s.elapsed())
            .as_secs_f64();
        if thermal.is_empty() {
            results.observe("thermal", "not reported by this device");
        } else {
            results.observe(
                "thermal",
                format!("{} over {over:.0} s", thermal.join("; ")),
            );
        }
        // **Shadows blurred**, across both renderers
        // (`vieww_paint::native::shadow_blurs`): whether the shadow caches hold.
        // A steady screen blurs none; a scroll should blur only what the cache
        // has not seen — an animated shadow, a new shape.
        let (blurs, pixels) = vieww_paint::native::shadow_blurs();
        #[expect(clippy::cast_precision_loss, reason = "counts")]
        let per_frame = blurs as f64 / log.frames().max(1) as f64;
        #[expect(clippy::cast_precision_loss, reason = "counts")]
        let megapixels = pixels as f64 / 1.0e6;
        results.observe(
            "shadow_blurs",
            format!(
                "{blurs} over {} frames ({per_frame:.2} a frame), {megapixels:.1} Mpx in all",
                log.frames()
            ),
        );
        // **The first frames against the last**, on a run long enough for
        // that to differ: a phone that throttles shows it here as a slower,
        // later window.
        if let Some((early_median, early_over, early_n)) = self.early {
            let mut late = log.work_samples();
            late.sort_unstable();
            let late_median = late.get(late.len() / 2).copied().unwrap_or_default();
            let late_over = late.iter().filter(|w| **w > log.budget()).count();
            results.observe(
                "sustained",
                format!(
                    "frames {}-{}: median {:.2} ms, {early_over} of {early_n} over budget; last \
                     {} of {} frames: median {:.2} ms, {late_over} over; {over:.0} s",
                    FRAMES.saturating_sub(early_n as u64) + 1,
                    FRAMES,
                    ms(early_median),
                    late.len(),
                    log.frames(),
                    ms(late_median),
                ),
            );
        }
    }
}

/// A line of the report. `ok: None` is an observation rather than a check.
#[derive(Debug)]
struct Line {
    name: &'static str,
    ok: Option<bool>,
    detail: String,
}

#[derive(Debug, Default)]
struct Results {
    lines: Vec<Line>,
    reported: bool,
}

impl Results {
    /// Record a check, replacing any earlier answer under the same name.
    ///
    /// **The latest answer wins, and the first version of this got it backwards.**
    /// A widget's `build` runs on every frame that touches it, so keeping the
    /// first answer looked like the way to stop the report filling with
    /// duplicates. It is wrong, because the values a probe reads legitimately
    /// start as placeholders: `FrameDriver::new` seeds `ViewMetrics` from the
    /// surface with a density of 1 and no safe area, and the real ones arrive
    /// from `sync_view_metrics` afterwards. First-wins froze that placeholder
    /// and reported a 2.75x phone as `1x` with no cutout — a suite confidently
    /// describing a device that does not exist.
    ///
    /// The report is printed once, at the end, so last-wins costs nothing and
    /// says what was true when it was asked.
    fn check(&mut self, name: &'static str, ok: bool, detail: impl Into<String>) {
        self.put(name, Some(ok), detail.into());
    }

    /// Record a check that, once true, stays true.
    ///
    /// For things that *happen* rather than things that *are*: a tap arrives on
    /// one particular rebuild, and every later build would otherwise overwrite
    /// it with whatever is true then.
    fn latch(&mut self, name: &'static str, detail: impl Into<String>) {
        if self.lines.iter().any(|line| line.name == name) {
            return;
        }
        self.put(name, Some(true), detail.into());
    }

    /// Record a measured value that no assertion is made about.
    fn observe(&mut self, name: &'static str, detail: impl Into<String>) {
        self.put(name, None, detail.into());
    }

    /// Upsert, keeping the original position so the report's order is stable.
    fn put(&mut self, name: &'static str, ok: Option<bool>, detail: String) {
        if let Some(line) = self.lines.iter_mut().find(|line| line.name == name) {
            line.ok = ok;
            line.detail = detail;
            return;
        }
        self.lines.push(Line { name, ok, detail });
    }

    fn failures(&self) -> usize {
        self.lines.iter().filter(|l| l.ok == Some(false)).count()
    }

    fn checks(&self) -> usize {
        self.lines.iter().filter(|l| l.ok.is_some()).count()
    }
}

/// Print one line of the report where the platform can be seen to print.
///
/// **Three platforms, three answers, and only the desktop one is the obvious one.**
/// Android discards stdout, so it needs `log` into logcat. iOS *accepts* stdout and
/// then shows it to nobody but an attached Xcode — see [`emit`] for how that was
/// found — so it needs `NSLog` into the unified log. A desktop has a console and
/// needs neither.
///
/// A `macro_rules!` rather than a function because `log::info!` and `println!` are
/// macros and neither can be passed as a value.
macro_rules! report {
    ($($arg:tt)*) => {{
        #[cfg(target_os = "android")]
        log::info!($($arg)*);
        #[cfg(target_os = "ios")]
        emit(&format!($($arg)*));
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        println!($($arg)*);
    }};
}

/// Put one line where a device log stream will see it, on iOS.
///
/// # Why `println!` is not enough, and how that was established
///
/// A real iPhone 14 ran this app for thirty seconds and the downloaded device log
/// held **61 lines, every one of them `vieww(UIKitCore) <Notice>`** — os_log output
/// from a system library inside our own process. Capture was working perfectly. Not
/// one line of ours appeared, because `println!` writes to **stdout**, which Xcode's
/// debugger captures and the OS unified log never sees.
///
/// So the whole device suite — every check, the tap targets, the frame report — was
/// mute on the one platform where nothing has ever been measured. Android only works
/// because `android_logger` routes `log` into logcat; iOS installed nothing.
///
/// `NSLog` writes to the unified log, which is what `adb`'s counterpart on iOS and
/// every device cloud streams.
///
/// # Why not a `log` backend
///
/// It would be tidier and it is the right shape for an *application* — and that is
/// exactly why it is not being decided here, under a deadline, for a test harness.
/// vieww has no logging story yet (it is on the P1 list); inventing one as a side
/// effect of fixing a device script would be the wrong place to make that choice.
/// This is fifteen lines confined to the example that needs it.
#[cfg(target_os = "ios")]
fn emit(line: &str) {
    use objc2_foundation::NSString;

    extern "C" {
        fn NSLog(format: *const NSString, ...);
    }

    // `"%@"` with the message as an **argument**, never the message as the format
    // string itself. A report carrying a percent sign — and these carry
    // `EdgeInsets`, percentages and file paths — would otherwise be read as a
    // format specifier and pull arbitrary values off the varargs stack.
    let format = NSString::from_str("%@");
    let message = NSString::from_str(line);
    unsafe { NSLog(&*format as *const NSString, &*message as *const NSString) }
    // And stderr: `xcrun devicectl device process launch --console` forwards
    // the process's own stdio, which is how AWS Device Farm's macOS host reads
    // the suite (`ci/mobile/device-farm/run-ios.sh`). The unified log stays the
    // channel for every other device cloud.
    eprintln!("{line}");
}

/// The on-device suite: a probe that lives in the tree, and a per-frame hook.
///
/// Created *before* `App::run`, because `on_frame` is installed before the
/// window exists. It holds no runtime handle of its own — the probe reads what
/// it needs from its `BuildContext` — which is what lets it exist that early.
#[derive(Debug, Clone, Default)]
pub(crate) struct DeviceSuite {
    results: Rc<RefCell<Results>>,
    /// The demo's tap counter, once the tree exists.
    ///
    /// The hooks are installed on the `App` builder, which runs *before* the
    /// build closure that creates the `Demo` and therefore before the signal
    /// exists at all. `probe` is called from inside that closure and is handed
    /// the signal, so it fills this in on the way past — the same shape
    /// `screen::PerfLink` uses for the frame graph, and for the same reason.
    taps: Rc<RefCell<Option<Signal<u32>>>>,
    /// The demo's drawer latch, filled in the same way and for the same reason
    /// as [`taps`](Self::taps).
    ///
    /// It exists so a device run can put a modal up **without a finger**. The
    /// question `BlockSemantics` raises is answerable mechanically — is the
    /// screen behind still in the semantics tree — and only the last step of it
    /// ("does TalkBack *say* nothing") needs ears.
    drawer: Rc<RefCell<Option<Signal<bool>>>>,
    /// Who else gets the report, once it is written — see
    /// [`when_reported`](Self::when_reported).
    sink: ReportSink,
}

/// A callback for the finished report: the same text the log gets.
///
/// Its own type only so [`DeviceSuite`] can keep deriving `Debug`, `Clone` and
/// `Default`.
type Deliver = Box<dyn FnOnce(&str)>;

#[derive(Clone, Default)]
struct ReportSink(Rc<RefCell<Option<Deliver>>>);

impl std::fmt::Debug for ReportSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReportSink")
    }
}

impl DeviceSuite {
    /// Hand the finished report to `deliver` as well as to the log.
    ///
    /// For a device cloud: Firebase Test Lab's Game Loop collects a results
    /// file the app writes (Android: the launch intent's data URI; iOS:
    /// `Documents/GameLoopResults/`), and a log line nobody downloads is not a
    /// result. Called once, on the frame the report is printed.
    #[allow(dead_code)]
    pub(crate) fn when_reported(&self, deliver: impl FnOnce(&str) + 'static) {
        *self.sink.0.borrow_mut() = Some(Box::new(deliver));
    }

    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The widget to put in the tree.
    ///
    /// It has to be *in* the tree rather than called from `build`, because the
    /// view metrics are published to the tree **after** the build closure runs —
    /// `resumed` creates the driver, calls `build`, and only then syncs the
    /// metrics. Anything asking about the safe area from inside the build
    /// closure reads the defaults and concludes, wrongly, that the device has no
    /// cutout. Reading it from a widget is also how an application would.
    pub(crate) fn probe(&self, taps: &Signal<u32>) -> WidgetNode {
        // Also the moment the activation hook gets its handle on the counter;
        // see the field.
        *self.taps.borrow_mut() = Some(taps.clone());
        Probe {
            results: Rc::clone(&self.results),
            taps: taps.clone(),
        }
        .into()
    }

    /// Hand the suite the drawer latch, so it can raise a modal itself.
    ///
    /// Separate from [`probe`](Self::probe) rather than a second parameter to
    /// it, because `probe` returns a widget that goes *in* the tree and this
    /// returns nothing — folding them together would make every call site read
    /// as though the drawer were something being mounted.
    ///
    /// No `dead_code` allowance, unlike most of this file: **both** entry points
    /// call this one.
    pub(crate) fn watch_drawer(&self, drawer: &Signal<bool>) {
        *self.drawer.borrow_mut() = Some(drawer.clone());
    }

    /// The closure to hand to `App::before_frame`, which raises a modal and
    /// asks whether the screen behind it is still reachable.
    ///
    /// # What this closes
    ///
    /// Every `ModalBarrier` in the framework wraps itself in `BlockSemantics`
    /// **unconditionally** — decided in code on 2026-08-14, tested on a host,
    /// and never once checked on a device or by ear, because no entry point that
    /// builds for Android had anything modal on it. `screen::NavMenu` is now
    /// that something, and this is the half of the claim a machine can settle.
    ///
    /// It asserts **two things together**, and neither is sufficient alone: the
    /// panel's own items are reachable, *and* [`TAP_LABEL`] is not. The first
    /// alone passes on a drawer that opened over a screen it never silenced; the
    /// second alone passes on a drawer that failed to open at all, which is the
    /// more likely failure and the one that would look like success.
    ///
    /// # What it does not close
    ///
    /// Whether TalkBack *announces* the difference, and whether a finger on the
    /// strip of live scrim beside the panel reaches anything. Both need ears and
    /// a hand; see `ci/mobile/a11y-android.sh --activate` and `docs/HANDOFF.md`. The
    /// value of this check is that it fails on every run where the mechanism
    /// broke, so the listening session is spent on the question that needs a
    /// person rather than on one a machine could have answered.
    ///
    /// # Why it is a three-stage machine and not a boolean
    ///
    /// `before_frame` runs ahead of this frame's build, so `driver.semantics()`
    /// is the tree the **previous** frame produced. Opening the drawer and
    /// reading the tree in one pass would read the screen as it was before the
    /// panel existed and report the blocking as broken on a working build. So:
    /// wait for the screen, open, read on the next frame, close.
    ///
    /// It closes the drawer afterwards on purpose. Leaving a modal up would
    /// change what every later check and every screenshot sees, and a suite that
    /// alters the app it is measuring is the failure this project keeps writing
    /// down.
    ///
    /// # Why this one carries no `dead_code` allowance
    ///
    /// Every other hook here is Android-only because it needs something the
    /// platform provides — a pasteboard, an accessibility bridge — and iOS will
    /// install it the day a device cloud runs one. This hook needs **nothing**:
    /// it reads vieww's own `SemanticsTree` and writes a signal. So both entry
    /// points install it, and `examples/ios.rs` is the desktop preview, which
    /// makes the `BlockSemantics` claim checkable on a workstation rather than
    /// only on a phone.
    pub(crate) fn modal_semantics(&self) -> impl FnMut(&mut vieww_render::FrameDriver) + 'static {
        let results = Rc::clone(&self.results);
        let slot = Rc::clone(&self.drawer);
        let mut stage = 0_u8;
        move |driver| {
            if stage > 1 {
                return;
            }
            let Some(drawer) = slot.borrow().clone() else {
                return;
            };

            let labels: Vec<String> = driver
                .semantics()
                .nodes()
                .iter()
                .filter_map(|node| node.label.clone())
                .collect();
            let reachable = |text: &str| labels.iter().any(|label| label == text);

            if stage == 0 {
                // Not frame one: the tree has to be built and laid out before
                // anything has a published node. Waiting for the button is also
                // what proves the screen was up *before* the drawer went over
                // it, which is what makes its absence below meaningful.
                if reachable(TAP_LABEL) {
                    drawer.set(true);
                    stage = 1;
                }
                return;
            }

            stage = 2;
            let panel = MENU_ITEMS.iter().filter(|item| reachable(item)).count();
            let behind = reachable(TAP_LABEL);
            drawer.set(false);

            results.borrow_mut().check(
                "a_modal_silences_the_screen_behind_it",
                panel == MENU_ITEMS.len() && !behind,
                format!(
                    "{panel} of {} panel item(s) reachable, {} behind the scrim \
                     ({} labelled node(s) in the tree)",
                    MENU_ITEMS.len(),
                    if behind {
                        format!("and {TAP_LABEL} is STILL")
                    } else {
                        String::from("and nothing is")
                    },
                    labels.len(),
                ),
            );
        }
    }

    /// Record a check from outside the tree.
    ///
    /// The probe covers everything a *widget* can see. Some things are only
    /// answerable before the window exists — whether the platform gave the app
    /// a writable directory, for one — and those are asked by the entry point
    /// and reported here, so they land in the same report on the same sentinel
    /// lines rather than in a second mechanism nobody greps for.
    ///
    /// `dead_code` is allowed on both of these because this file is one module
    /// included by two entry points, and only the Android one has a
    /// platform-specific question to ask so far. The iOS one will, the day it
    /// runs anywhere.
    #[allow(dead_code)]
    pub(crate) fn check(&self, name: &'static str, ok: bool, detail: impl Into<String>) {
        self.results.borrow_mut().check(name, ok, detail);
    }

    /// Record a measured value from outside the tree, asserting nothing.
    #[allow(dead_code)]
    pub(crate) fn observe(&self, name: &'static str, detail: impl Into<String>) {
        self.results.borrow_mut().observe(name, detail);
    }

    /// The closure to hand to `App::after_frame`, which reports where the things
    /// a script has to tap **actually are**.
    ///
    /// # Why this exists, and why it is not a convenience
    ///
    /// `ci/mobile/device-suite.sh` used to tap `190,536`. Those numbers were correct for
    /// one 1080x2340 phone at 2.75x and silently wrong everywhere else, and the
    /// script said so itself: "elsewhere the tap misses and the check is absent
    /// rather than failing". **A cross-platform framework cannot verify itself with
    /// a constant that suits one handset** — a second device would report a pass
    /// that had checked less, with nothing to say it had.
    ///
    /// It also froze the demo's layout. `screen.rs` forbids adding anything except
    /// *below* the frame graph, because everything above it feeds those constants,
    /// which is why the phone demo has five controls and not twenty-one.
    ///
    /// So the app answers instead. Nothing here knows a resolution, a density or a
    /// device: [`SemanticsNode::bounds`] is already global logical pixels, and
    /// [`ViewMetrics::device_pixel_ratio`] is the only conversion needed. The same
    /// code reports correctly on a tablet, a low-density phone, or a desktop
    /// window resized by hand.
    ///
    /// # Found by role and label, not by position
    ///
    /// The button by its label, because a screen may grow more buttons; the field
    /// by its role, because a text field's `label` is empty and its `value` is the
    /// text being edited. Both are what a screen reader would use to find them,
    /// which is the point — a target a11y cannot locate is one no script should be
    /// able to either.
    ///
    /// Reported **once**, on the first frame that has both, because the numbers do
    /// not move and a line per frame at 60Hz would bury the report.
    ///
    /// [`SemanticsNode::bounds`]: vieww_render::SemanticsNode::bounds
    pub(crate) fn tap_targets(&self) -> impl FnMut(&vieww_render::FrameDriver) + 'static {
        let mut reported = false;
        move |driver| {
            if reported {
                return;
            }
            let scale = driver.view_metrics().device_pixel_ratio;
            // A ratio of zero or worse would put every target at the origin, which
            // reads as "the layout is broken" rather than "the metrics have not
            // arrived yet". Wait for a real one.
            if !scale.is_finite() || scale <= 0.0 {
                return;
            }

            let semantics = driver.semantics();
            let centre = |node: &vieww_render::SemanticsNode| {
                let rect = node.bounds;
                let x = (rect.origin().dx + rect.size().width / 2.0) * scale;
                let y = (rect.origin().dy + rect.size().height / 2.0) * scale;
                (x.round() as i32, y.round() as i32)
            };

            let button = semantics
                .nodes()
                .iter()
                .find(|n| n.label.as_deref() == Some(TAP_LABEL))
                .map(centre);
            let field = semantics
                .nodes()
                .iter()
                .find(|n| n.role == vieww_render::Role::TextField)
                .map(centre);

            // **Optional, and deliberately not part of the guard below.** The
            // menu button sits at the bottom of a column that is not scrollable,
            // so a short enough screen could lay it out past the edge. Requiring
            // it would then suppress the whole line and take the button and field
            // coordinates with it — turning "the drawer is off screen" into
            // "`ci/mobile/device-suite.sh` taps nothing and reports nothing", which is
            // exactly the failure this function's own comment warns about.
            let menu = semantics
                .nodes()
                .iter()
                .find(|n| n.label.as_deref() == Some(MENU_LABEL))
                .map(centre);

            let (Some((bx, by)), Some((fx, fy))) = (button, field) else {
                return;
            };
            reported = true;
            // One greppable line, in physical pixels, which is the unit
            // `adb shell input tap` speaks.
            let menu = match menu {
                Some((mx, my)) => format!(" menu={mx},{my}"),
                None => String::new(),
            };
            report!("__VIEWW_TAPS__ button={bx},{by} field={fx},{fy}{menu}");
            // The same targets in **logical points**, which is what iOS input
            // speaks (XCUITest/WebDriverAgent taps through Appium, in
            // `ci/mobile/device-farm/run-ios.sh`). A separate line so the
            // Android parser of the one above is untouched.
            let point = |(x, y): (i32, i32)| {
                let (x, y) = (x as f32 / scale, y as f32 / scale);
                format!("{},{}", x.round() as i32, y.round() as i32)
            };
            report!(
                "__VIEWW_TAPS_PT__ scale={scale} button={} field={}",
                point((bx, by)),
                point((fx, fy))
            );
        }
    }

    /// The closure to hand to `App::before_frame`, which activates the button
    /// the way a screen reader does and checks that something happened.
    ///
    /// # What this closes, and what it does not
    ///
    /// The remaining accessibility question on Android was whether a TalkBack
    /// **double-tap activates the right thing** — `handle_semantic_action` end
    /// to end. That path has two halves: `accesskit_android` delivering the
    /// action into the app, and the app routing it to the render object that
    /// can do it. This checks **the second half**, on every device, with nobody
    /// listening: it finds the button the way a screen reader finds it (by
    /// label), dispatches `SemanticAction::Click` at its node, and asserts the
    /// tap counter moved.
    ///
    /// The first half needs TalkBack actually running and is what
    /// `ci/mobile/a11y-android.sh --activate` drives. Neither is sufficient alone —
    /// a working router behind a broken bridge announces nothing, and a working
    /// bridge into a router that drops the action does nothing.
    ///
    /// `before_frame` rather than `after_frame` because dispatching needs
    /// `&mut FrameDriver`, and because the rebuild that follows the signal write
    /// is then *this* frame's rather than the next one's.
    ///
    /// Runs once, on the first frame where the semantics tree has the button in
    /// it — which is not frame one: the tree has to be laid out before it has
    /// bounds or a published node.
    /// `dead_code` for the same reason `check` and `observe` carry it: this file
    /// is one module included by two entry points, and only `android.rs`
    /// installs this hook. iOS will, the day a device cloud runs it.
    #[allow(dead_code)]
    pub(crate) fn semantic_activation(
        &self,
    ) -> impl FnMut(&mut vieww_render::FrameDriver) + 'static {
        let results = Rc::clone(&self.results);
        let slot = Rc::clone(&self.taps);
        let mut done = false;
        move |driver| {
            if done {
                return;
            }
            // Not yet built on the first frames; `probe` fills it in.
            let Some(taps) = slot.borrow().clone() else {
                return;
            };
            let Some(id) = driver
                .semantics()
                .nodes()
                .iter()
                .find(|node| node.label.as_deref() == Some(TAP_LABEL))
                .map(|node| node.id)
            else {
                return;
            };
            done = true;

            let before = taps.get();
            let handled = driver.handle_semantic_action(id, vieww_render::SemanticAction::Activate);
            let after = taps.get();

            // Two assertions in one, deliberately. `handled` alone would pass if
            // some ancestor claimed the action and did nothing with it, and the
            // counter alone would pass if the button had been tapped by a finger
            // in the same frame. Together they say this dispatch did this work.
            results.borrow_mut().check(
                "a_semantic_click_activates_the_button",
                handled && after == before + 1,
                format!("handled={handled}, taps {before} -> {after}"),
            );
        }
    }

    /// The closure to hand to `App::after_frame`, which round-trips the real
    /// platform pasteboard.
    ///
    /// # Why this is a device check
    ///
    /// The JNI and `objc2` clipboard paths were written on 2026-08-13 and have
    /// only ever been through a compiler. The desktop path has five tests
    /// through the real services seam; these two have none, and compiling says
    /// nothing about whether `ClipboardManager` was found, whether the JNI
    /// signature matches, or whether the returned `CharSequence` survives being
    /// turned into a `String`.
    ///
    /// **Not at startup, and that is the whole reason this is a frame hook.**
    /// From API 29 Android returns an empty pasteboard to a *background* read,
    /// so a round trip run from `android_main` before the activity is in front
    /// would report a broken clipboard on a working one. `after_frame` runs
    /// after a frame has been presented, so the app is by definition on screen.
    ///
    /// **It puts back what it found.** A device suite that silently ate the
    /// user's pasteboard would be a bad citizen on a phone somebody also uses.
    /// The restore is best-effort: if the read failed there is nothing to put
    /// back, which is reported rather than hidden.
    ///
    /// What it still does not prove is that *another application* sees the
    /// write. That needs a second app and a person, and it is the one clipboard
    /// question left after this.
    /// `dead_code` as above — installed by `android.rs` only, for now. The iOS
    /// pasteboard is written and equally unrun, and this is the hook that will
    /// check it.
    #[allow(dead_code)]
    pub(crate) fn clipboard_round_trip(
        &self,
        clipboard: Option<Rc<dyn vieww_foundation::Clipboard>>,
    ) -> impl FnMut(&vieww_render::FrameDriver) + 'static {
        let results = Rc::clone(&self.results);
        let mut done = false;
        move |_driver| {
            if done {
                return;
            }
            done = true;

            let Some(clipboard) = clipboard.clone() else {
                results.borrow_mut().check(
                    "the_platform_clipboard_round_trips",
                    false,
                    "no Clipboard service is registered on this platform",
                );
                return;
            };

            // Distinctive, so that a stale value from a previous run cannot be
            // mistaken for this one's write succeeding.
            const MARKER: &str = "vieww device clipboard check 8f3a";

            let previous = clipboard.read_text();
            let written = clipboard.write_text(MARKER);
            let read_back = clipboard.read_text();

            // Best effort, and only when there was something to restore.
            if let Ok(Some(text)) = &previous {
                let _ = clipboard.write_text(text);
            }

            let detail = match (&written, &read_back) {
                (Err(error), _) => format!("write failed: {error}"),
                (Ok(()), Err(error)) => format!("wrote, but read failed: {error}"),
                (Ok(()), Ok(Some(text))) if text == MARKER => {
                    let restored: String = match &previous {
                        Ok(Some(_)) => "restored what was there".to_owned(),
                        Ok(None) => "the pasteboard had been empty".to_owned(),
                        Err(error) => format!("could not read the old value back ({error})"),
                    };
                    format!("wrote and read back {} bytes; {restored}", MARKER.len())
                }
                (Ok(()), Ok(Some(text))) => {
                    format!(
                        "read back {} bytes, and they are not the marker",
                        text.len()
                    )
                }
                (Ok(()), Ok(None)) => "wrote, and read back an empty pasteboard — the API 29 \
                     background-read rule looks like this"
                    .to_owned(),
            };

            let ok = matches!((&written, &read_back), (Ok(()), Ok(Some(text))) if text == MARKER);
            results
                .borrow_mut()
                .check("the_platform_clipboard_round_trips", ok, detail);
        }
    }

    /// The closure to hand to `App::on_frame`.
    pub(crate) fn hook(&self) -> impl FnMut(&FrameLog) + 'static {
        let results = Rc::clone(&self.results);
        let sink = self.sink.clone();
        let mut watch = Watch::default();
        move |log| {
            if results.borrow().reported {
                return;
            }
            watch.frame(log);
            // Progress, so `ci/mobile/device-suite.sh` waits as long as frames
            // keep coming rather than for a fixed time: a fixed deadline is
            // either too short for the slowest screen on the slowest phone
            // (the LG Stylo 5 drew the stress screen slower than 4 fps,
            // 2026-10-22) or wasted on every other.
            if log.frames().is_multiple_of(60) {
                report!("__VIEWW_PROGRESS__ frames={}", log.frames());
            }
            if !watch.done(log) {
                return;
            }
            let text = {
                let mut results = results.borrow_mut();
                frame_checks(&mut results, log);
                watch.observe(&mut results, log);
                results.reported = true;
                print(&results)
            };
            let deliver = sink.0.borrow_mut().take();
            if let Some(deliver) = deliver {
                deliver(&text);
            }
        }
    }
}

/// Each core's current and maximum clock, from sysfs: `cpu0 1.10/1.80 GHz | …`.
/// `None` where there is no such file (not Linux or Android, or a phone that
/// hides it).
fn cpu_clocks() -> Option<String> {
    let read = |core: usize, file: &str| {
        std::fs::read_to_string(format!("/sys/devices/system/cpu/cpu{core}/cpufreq/{file}"))
            .ok()
            .and_then(|text| text.trim().parse::<f64>().ok())
    };
    let mut parts = Vec::new();
    for core in 0..16 {
        let max = read(core, "cpuinfo_max_freq");
        let current = read(core, "scaling_cur_freq");
        match (current, max) {
            (Some(current), Some(max)) => parts.push(format!(
                "cpu{core} {:.2}/{:.2} GHz",
                current / 1e6,
                max / 1e6
            )),
            // An offline core keeps its directory but not its clock.
            (None, Some(max)) => parts.push(format!("cpu{core} off/{:.2} GHz", max / 1e6)),
            _ => {}
        }
    }
    (!parts.is_empty()).then(|| parts.join(" | "))
}

/// What the frame log says after [`frames`] frames.
fn frame_checks(results: &mut Results, log: &FrameLog) {
    let samples = log.work_samples();
    let budget = log.budget();

    results.check(
        "frames_were_presented",
        log.frames() >= frames(),
        format!("{} frames", log.frames()),
    );

    if samples.is_empty() {
        results.check("frame_times_were_recorded", false, "no samples");
        return;
    }

    let mut sorted = samples.clone();
    sorted.sort_unstable();
    let worst = *sorted.last().unwrap_or(&Duration::ZERO);
    let median = sorted[sorted.len() / 2];

    // The check the raster fix made meaningful. Before it, `work` contained the
    // vsync wait and this was pinned at the budget on a perfectly healthy app —
    // an assertion that could only ever have been written to pass.
    //
    // Measured over a window that excludes the warm-up, by construction: see
    // `FRAMES`. The occasional late frame is still allowed, because a phone
    // schedules other things and a suite that fails on one 20ms frame in six
    // hundred is a suite that gets muted. What it will not tolerate is a
    // *median* over budget, which is what "this app is slow" actually looks
    // like, or a single frame so late it drops several vsyncs.
    //
    // A stall is measured against the larger of four budgets and three medians.
    // Four budgets alone failed a phone twice for one slowness: the LG Stylo 5's
    // CPU pass (run 39382602099) had a 38 ms median — already a failure of the
    // check above — and a 98 ms worst frame, 2.6 medians, which is its normal
    // spread, not a stall. The A73's 131 ms worst over a 23 ms median (5.7
    // medians, and over four budgets) is a real one and still fails.
    let over = sorted.iter().filter(|work| **work > budget).count();
    let stall = (budget * 4).max(median * 3);
    results.check(
        "the_typical_frame_is_inside_its_budget",
        median <= budget,
        format!("median {median:?} against a budget of {budget:?}"),
    );
    // **Pacing, not just the median.** A median inside the budget with one
    // frame in five outside it is visible stutter on a 120 Hz panel — the
    // Galaxy A73's shipping path in run 39402544760 (52 of 240). One in
    // twenty is the allowance: the same share at which the app itself drops
    // to a steady 60 Hz (`vieww_platform_winit`'s `cadence`), so on a phone
    // that adapts, this judges the rate it chose.
    results.check(
        "frames_inside_their_budget",
        over * 20 <= sorted.len(),
        format!(
            "{over} of {} frames over a budget of {budget:?} (at most 1 in 20)",
            sorted.len()
        ),
    );
    results.check(
        "no_frame_stalled",
        worst <= stall,
        format!(
            "worst {worst:?} against a stall line of {stall:?} (max of 4x budget, 3x median), {over} of {} over budget",
            sorted.len()
        ),
    );

    // Deliberately not a check: how much headroom a device has is a fact about
    // the device, and a threshold here would be a guess about hardware nobody
    // has seen yet.
    results.observe(
        "frame_time",
        format!(
            "median {median:?}, worst {worst:?}, budget {budget:?} ({} samples)",
            sorted.len()
        ),
    );

    // Where that time went, per renderer stage (`FrameStages`): the breakdown
    // PENDING §1.5 asks for behind the Adreno 612's 21 ms GPU median. Observed,
    // not checked, for the reason `frame_time` above is: a stage's time is a
    // fact about the handset.
    // Where this launch's start-up went, step by step (`vieww_platform_winit::
    // startup_report`). Observed, not checked: the audit's 120 ms is a target
    // for a mid-tier phone, and a check that fails on every low-end one would
    // be muted rather than read.
    if let Some(startup) = vieww_platform_winit::startup_report() {
        results.observe("startup_stages", startup);
    }
    // What `App::auto_rasterizer` chose on this device, and from which
    // numbers — absent when the app fixed its rasterizer in advance.
    if let Some(choice) = vieww_platform_winit::rasterizer_choice() {
        results.observe("renderer_choice", choice);
    }

    // The same breakdown at the median, at p95, and for the single worst
    // frame: the A36's 13.9 ms median and 68.6 ms worst (Device Farm,
    // 2026-10-17) needed the last two to say which stage the stall was in.
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    let line = |stages: &vieww_platform_winit::native::FrameStages| {
        format!(
            "scale {:.2} plan {:.2} cpu {:.2} gpu_upload {:.2} gpu_record {:.2} \
             gpu_submit {:.2} gpu_wait {:.2} gpu_readback {:.2} acquire {:.2} present {:.2}",
            ms(stages.scale),
            ms(stages.plan),
            ms(stages.cpu_raster),
            ms(stages.gpu.upload),
            ms(stages.gpu.record),
            ms(stages.gpu.submit),
            ms(stages.gpu.wait),
            ms(stages.gpu.readback),
            ms(stages.acquire),
            ms(stages.present),
        )
    };
    let stages = log.stage_medians();
    if !stages.total().is_zero() {
        results.observe("frame_stages", format!("medians ms: {}", line(&stages)));
        results.observe(
            "frame_stages_p95",
            format!("p95 ms: {}", line(&log.stage_percentile(0.95))),
        );
    }
    // **Where the GPU's own time went**, by kind of work, from timestamp
    // queries — `gpu_wait` above is one number on the CPU's clock and cannot
    // say whether a slow GPU frame is geometry, layers, filters or shadows
    // (`vieww_hal::vulkan::GpuPassTimes`). Said outright when the queue
    // records no timestamps, rather than printed as zeroes.
    let passes = |t: &vieww_hal::vulkan::GpuPassTimes| {
        format!(
            "draw {:.2} layers {:.2} filters {:.2} shadows {:.2} blit {:.2} total {:.2}",
            ms(t.draw),
            ms(t.layers),
            ms(t.filters),
            ms(t.shadows),
            ms(t.blit),
            ms(t.total()),
        )
    };
    // **Input to present**, per input (`vieww_platform_winit::InputLatencyLog`):
    // from a tap or key reaching the window's event loop to the present call
    // that showed it. Observed, not gated, until phones have given numbers
    // to set a bar from. Not touch-panel to photon: winit carries no
    // platform event time, and scan-out comes after the present.
    if let Some(latency) = log.input_latency() {
        if latency.total_events > 0 {
            results.observe(
                "input_latency",
                format!(
                    "input to present ms: median {:.1} p95 {:.1} p99 {:.1} worst {:.1} \
                     over {} inputs in {} frames; {} unanswered within {} s",
                    ms(latency.p50),
                    ms(latency.p95),
                    ms(latency.p99),
                    ms(latency.worst),
                    latency.samples,
                    latency.frames_with_input,
                    latency.unanswered,
                    vieww_platform_winit::UNANSWERED_AFTER.as_secs(),
                ),
            );
        } else {
            results.observe("input_latency", "no input reached this window");
        }
    }
    // Which way the GPU frames ran: overlapping the app's next frame, or
    // waited for one by one (`debug.vieww.overlap=0`, the `gpuserial` pass).
    if !stages.gpu.total().is_zero() {
        results.observe(
            "gpu_overlap",
            if vieww_platform_winit::native::gpu_overlap() {
                "on: the app builds the next frame while the GPU draws this one"
            } else {
                "off: every GPU frame waited for before the next (VIEWW_GPU_OVERLAP=0)"
            },
        );
    }
    // **The copy into the swapchain image, partial or whole**
    // (`vieww_hal::vulkan::partial_blit_stats`): whether this device kept
    // its images between presents (probed before partial copies are
    // trusted), and how much of the screen the copies actually covered.
    // With `VIEWW_PARTIAL_BLIT_CHECK=1` every written image was also read
    // back and compared with the frame: a trail is a failed check here.
    let partial = vieww_hal::vulkan::partial_blit_stats();
    if let Some(state) = partial.state {
        #[expect(clippy::cast_precision_loss, reason = "a ratio of pixel counts")]
        let share = partial.copied_pixels as f64 / (partial.frame_pixels.max(1) as f64);
        results.observe(
            "partial_blit",
            format!(
                "{state}: {} partial and {} full copies, {:.1}% of the frames' pixels copied; \
                 {} image(s) kept their contents, {} did not",
                partial.partial_frames,
                partial.full_frames,
                share * 100.0,
                partial.probes_ok,
                partial.probes_bad,
            ),
        );
        if partial.checked_frames > 0 {
            results.check(
                "partial_copies_leave_no_trails",
                partial.check_bad_pixels == 0 && partial.probes_bad == 0,
                format!(
                    "{} frames compared with their images: {} pixel(s) differed; \
                     {} image(s) lost what was written to them",
                    partial.checked_frames, partial.check_bad_pixels, partial.probes_bad,
                ),
            );
        }
    }
    match (stages.gpu_passes, log.stage_percentile(0.95).gpu_passes) {
        (Some(median), Some(p95)) => {
            results.observe(
                "gpu_passes",
                format!("medians ms: {}; p95 ms: {}", passes(&median), passes(&p95)),
            );
        }
        _ if !stages.gpu.total().is_zero() => {
            results.observe(
                "gpu_passes",
                "unavailable: this GPU's queue records no timestamps",
            );
        }
        _ => {}
    }
    // **Where the CPU raster's time went**, by kind of command — the CPU
    // path's `gpu_passes` (`vieww_paint::native::RasterKindTimes`). The
    // Stylo 5 drew the demo's CPU frames in 34 ms with the raster nearly all
    // of it, and nothing said whether that was text, shadows or the layer
    // composites. Present when the entry point set `VIEWW_RASTER_PROFILE=1`
    // and frames ran on the CPU.
    if let (Some(median), Some(p95)) = (stages.cpu_kinds, log.stage_percentile(0.95).cpu_kinds) {
        let kinds = |t: &vieww_paint::native::RasterKindTimes| {
            format!(
                "text {:.2} shadows {:.2} paths {:.2} rects {:.2} images {:.2} \
                 layers {:.2} composite {:.2} total {:.2}",
                ms(t.text),
                ms(t.shadows),
                ms(t.paths),
                ms(t.rects),
                ms(t.images),
                ms(t.layers),
                ms(t.composite),
                ms(t.total()),
            )
        };
        results.observe(
            "raster_by_kind",
            format!("medians ms: {}; p95 ms: {}", kinds(&median), kinds(&p95)),
        );
    }
    if let Some((work, stages)) = log.worst_frame_stages() {
        // What the stages leave out of the frame's work is time spent outside
        // the renderer — build, layout, or the thread not running at all.
        let outside = work.saturating_sub(stages.total());
        let core = stages
            .cpu_core
            .map_or_else(String::new, |core| format!(", on cpu{core}"));
        results.observe(
            "worst_frame",
            format!(
                "work {:.2} ms, outside the renderer {:.2} ms{core}; stages ms: {}",
                ms(work),
                ms(outside),
                line(&stages)
            ),
        );
    }

    // Which core the CPU raster ran on, and how long it took there — the
    // question behind the LG Stylo 5 drawing the same frame in 31 ms to the
    // window and 21 ms through Vulkan (2026-10-17). With the clocks each core
    // was at when the report was taken, and the most it can reach, so a
    // little core and a big one at a low clock can be told apart.
    let cores = log.raster_by_core();
    if !cores.is_empty() {
        results.observe(
            "raster_by_core",
            cores
                .iter()
                .map(|(core, frames, median)| {
                    format!("cpu{core} {frames} frames {:.2} ms", ms(*median))
                })
                .collect::<Vec<_>>()
                .join(" | "),
        );
    }
    if let Some(clocks) = cpu_clocks() {
        results.observe("cpu_clocks", clocks);
    }
    if let Some(cache) = vieww_platform_winit::pipeline_cache_status() {
        results.observe("pipeline_cache", cache);
    }
    if let Some(hint) = vieww_platform_winit::perf_hint_status() {
        results.observe("perf_hint", hint);
    }
    // What this device is, and what that implies for quality: the tier the
    // policy resolved (`VIEWW_DEVICE_TIER` can override), the RAM and cores
    // it was resolved from, and the SoC as the device names itself. Observed
    // beside the shell's own `mem_total_kb`/`cpu_cores` so the two probes
    // can be cross-checked against each other.
    if let Some(device) = vieww_platform_winit::device_facts_status() {
        results.observe("device", device);
    }
    // The GPU path's resolution scale, when one is engaged
    // (`VIEWW_GPU_SCALE`, `crate::quality`); quiet at full resolution.
    if let Some(scale) = vieww_platform_winit::gpu_scale_status() {
        results.observe("gpu_scale", scale);
    }
    // The blur/fill cache budgets this process runs with — the tier's
    // (`quality::cache_budgets_for`) unless an env override says otherwise,
    // so a PSS number can be read against the caches it was measured with.
    results.observe(
        "cache_budgets",
        vieww_platform_winit::cache_budgets_status(),
    );
    // The same facts and the frame clock, on the one line the farm summary
    // prints beside each pass (`vieww-devtools`'s quality overlay). The
    // timeline is fed from the samples already measured above — the overlay
    // times nothing and sorts nothing itself — and its window is the whole
    // session, the same window `frame_time`'s median is taken over.
    let mut timeline = vieww_devtools::FrameTimeline::new(samples.len());
    for (index, work) in samples.iter().enumerate() {
        timeline.record(vieww_paint::FrameStats {
            number: index as u64,
            timestamp: Duration::ZERO,
            animate: Duration::ZERO,
            build: Duration::ZERO,
            layout: Duration::ZERO,
            paint: Duration::ZERO,
            composite: Duration::ZERO,
            total: *work,
            budget,
            damage_area: 0.0,
            damage_regions: 0,
        });
    }
    let overlay = vieww_devtools::QualityOverlay::build(
        vieww_platform_winit::device_facts().tier.name(),
        &vieww_platform_winit::device_facts_status()
            .unwrap_or_else(|| "probes said nothing".to_owned()),
        vieww_platform_winit::gpu_scale_status().as_deref(),
        &timeline,
        samples.len(),
    );
    results.observe("quality_overlay", overlay.summary_line());

    // The one performance claim that *can* be gated on a single run, and the
    // reason is that it is not a time.
    //
    // Every millisecond above moves with the handset and with how hot it is —
    // four runs of this suite on one phone produced medians of 4.09, 6.86, 6.56
    // and 10.84ms with no code change between them. A 2.65x spread is larger
    // than most regressions worth catching, so a threshold either passes
    // everything (which is what a 16.7ms ceiling does) or flakes.
    //
    // Rasterised pixels do not throttle. `full_repaints` is pixels vello
    // actually touched over pixels in the surface, so per frame it is a pure
    // property of damage tracking — independent of the device, the resolution
    // and the temperature.
    //
    // **The bound was 1.0 for weeks and it was measuring nothing.** That number
    // was chosen because more than one full repaint per frame is the one outcome
    // that makes damage tracking worse than not having it — true, and far too
    // loose, because the note beside it claimed this screen already sat at 1.0
    // (`rasterised 190.00 full repaints` over 190 frames) and so every frame was
    // repainting in full.
    //
    // **That claim does not reproduce.** Measured 2026-08-14 on the Redmi Note 7
    // Pro, over 600 frames: **0.066 full repaints per frame**, with only 2
    // individual frames costing a whole surface. Damage tracking was working the
    // whole time. The old figure has no run behind it that anyone can point at,
    // and it is the fourth item on a "what is left" list to dissolve on contact.
    //
    // So the bound is 0.25 — **fifteen times below the old one and four times
    // above what was measured.** The headroom is for the parts of this suite
    // that legitimately damage everything: the keyboard opening, a tap changing
    // a whole row, the scroll. What it will not tolerate is the failure the old
    // note described: a screen that repaints in full every frame lands at 1.0
    // and fails this by a factor of four.
    //
    // Safe to state tightly because it is **not a time**. Rasterised pixels do
    // not throttle, the value is a ratio so resolution divides out, and the
    // suite drives the same scripted sequence everywhere — unlike the
    // milliseconds above, which spread 2.65x across four runs on one handset.
    let report = log.report();
    let per_frame = if log.frames() == 0 {
        0.0
    } else {
        report.full_repaints() / log.frames() as f64
    };
    let damage = format!(
        "{per_frame:.3} full repaints per frame over {} frames, \
         against a {}px surface",
        log.frames(),
        report.surface_pixels,
    );
    // **Not on the stress screen** (`VIEWW_SCREEN=stress`), whose list
    // scrolls under a full-width backdrop blur every frame: there a whole
    // surface each frame is the right answer, and a bound of 0.25 would fail a
    // renderer for drawing what changed. Reported, so a stress run still says
    // what it rasterised; the bound stays where it means something.
    if std::env::var("VIEWW_SCREEN").is_ok_and(|screen| screen == "stress") {
        results.observe(
            "damage",
            format!("{damage} (stress screen: the whole list moves every frame)"),
        );
    } else {
        results.check(
            "damage_tracking_costs_a_fraction_of_a_full_repaint",
            report.surface_pixels > 0 && per_frame <= 0.25,
            damage,
        );
    }

    // Which frames those repaints were, which the ratio above cannot say.
    //
    // A ratio of 1.0 over 190 frames is equally consistent with every frame
    // repainting in full and with a third of them repainting three times over
    // while the rest cost nothing. Those are different bugs and one of them is
    // not a bug at all, so the ratio alone was never something to act on.
    //
    // `continuous` already draws the line that matters. A frame the loop *owed*
    // — mid-scroll, mid-animation — repaints everything because the viewport
    // moved everything, and that is correct. A frame the loop **slept before**
    // was one tap or one caret blink on a still screen, and rasterising the
    // whole surface for it is precisely what damage tracking exists to avoid.
    results.observe(
        "full_repaints",
        format!(
            "{} of {} frames cost a whole surface, {} of those after the loop slept",
            report.full_repaint_frames, report.frames, report.idle_full_repaint_frames,
        ),
    );

    // Gated now, because the number has been read: **1 idle full repaint in 600
    // frames** on 2026-08-14. The question this was built to answer — "startup
    // and idle frames repaint fully and nobody has worked out why" — turns out
    // to have had a false premise. They do not.
    //
    // 2% rather than the 0.17% measured, because one legitimate cause exists and
    // is rare: the surface being invalidated after a sleep, which forces a full
    // repaint by design. A handful of those in a session is correct behaviour.
    // Hundreds is the defect, and on a 600-frame run the two are three orders of
    // magnitude apart — there is no threshold between them worth arguing about.
    //
    // The floor keeps a short run honest: 2% of 30 frames is a bound of zero,
    // which the *first* frame would fail on its own if it happened to follow a
    // sleep.
    let idle_budget = 4.max(report.frames / 50);
    results.check(
        "idle_frames_do_not_repaint_the_whole_surface",
        report.idle_full_repaint_frames <= idle_budget,
        format!(
            "{} idle full repaint(s) over {} frames, against a budget of {idle_budget}",
            report.idle_full_repaint_frames, report.frames,
        ),
    );

    // A check on the *instrument*, not on the result. The first presented frame
    // has no previous contents to keep, so it is a full repaint by construction
    // — on every device, at every resolution. Zero here therefore does not mean
    // a clean run, it means the counter never saw a surface size and is
    // reporting silence as good news. That failure mode has cost this project a
    // day before; see `uiautomator dump` in docs/HANDOFF.md.
    //
    // It matters more now that the two bounds above are tight: both are
    // satisfied trivially by a counter that never counts.
    results.check(
        "the_full_repaint_counter_saw_the_first_frame",
        report.frames == 0 || report.full_repaint_frames >= 1,
        format!(
            "{} full repaint(s) over {} frames against a {}px surface",
            report.full_repaint_frames, report.frames, report.surface_pixels,
        ),
    );

    results.observe("frame_report", format!("{report}"));
    // Which rasterizer drew. With `App::prefer_gpu` a frame the planner cannot
    // express falls back to the CPU, and a GPU that would not build falls
    // back for good — so "GPU requested" is not "GPU measured", and the frame
    // times above mean nothing until this says which one they are.
    results.observe(
        "gpu_frames",
        format!(
            "{} of {} drawn by the GPU rasterizer ({} straight into the swapchain)",
            report.gpu_frames, report.frames, report.gpu_direct_frames
        ),
    );
}

/// Write the report out, ending with the sentinel the script greps for.
/// Returns the same lines, joined, for [`DeviceSuite::when_reported`].
fn print(results: &Results) -> String {
    let mut text = String::new();
    let mut line_out = |line: String| {
        report!("{}", line);
        text.push_str(&line);
        text.push('\n');
    };
    line_out("__VIEWW_SUITE__ begin".to_owned());
    for line in &results.lines {
        let mark = match line.ok {
            Some(true) => "ok  ",
            Some(false) => "FAIL",
            None => "--  ",
        };
        line_out(format!(
            "__VIEWW_SUITE__ {} {}: {}",
            mark, line.name, line.detail
        ));
    }
    let failures = results.failures();
    line_out(format!(
        "__VIEWW_SUITE__ end {} checks, {} failed",
        results.checks(),
        failures
    ));
    // A separate, greppable verdict. The script keys on this rather than
    // counting lines itself, so adding a check never changes the script.
    line_out(format!(
        "__VIEWW_SUITE__ result {}",
        if failures == 0 { "PASS" } else { "FAIL" }
    ));
    text
}

/// Reads what only a live window knows, from inside the tree.
///
/// Draws nothing. It is a `Composed` widget returning an empty box, so it costs
/// one element and no pixels — a probe that changed the layout would be
/// measuring a tree that only exists while it is being measured.
#[derive(Debug)]
struct Probe {
    results: Rc<RefCell<Results>>,
    taps: Signal<u32>,
}

impl Widget for Probe {
    fn debug_name(&self) -> &'static str {
        "Probe"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, ctx: &BuildContext) -> WidgetNode {
        let metrics = ctx.inherit_or(ViewMetrics::default());
        let taps = self.taps.get();
        let mut results = self.results.borrow_mut();

        // The metrics reached the tree at all. `ViewMetrics::default` is a zero
        // surface, so a real size proves the publish happened — which is exactly
        // what the `set_root` footgun silently broke, and it broke it in a way
        // that looked like a layout bug rather than a plumbing one.
        let size = metrics.size;
        results.check(
            "view_metrics_reached_the_tree",
            size.width > 0.0 && size.height > 0.0,
            format!("{}x{} logical", size.width, size.height),
        );

        let ratio = metrics.device_pixel_ratio;
        results.check(
            "the_display_density_is_plausible",
            ratio.is_finite() && (0.5..=8.0).contains(&ratio),
            format!("{ratio}x"),
        );

        // Sane, not non-zero. A phone with no cutout reports zeros and is not
        // broken; what would be broken is an inset that is negative, or one that
        // eats the whole screen. See this module's header on why the "is there
        // actually a notch" question belongs to the script.
        let safe = metrics.safe_area;
        let sane = [safe.left, safe.top, safe.right, safe.bottom]
            .iter()
            .all(|inset| inset.is_finite() && *inset >= 0.0)
            && safe.top + safe.bottom < size.height
            && safe.left + safe.right < size.width;
        results.check(
            "the_safe_area_fits_inside_the_surface",
            sane,
            format!(
                "l{} t{} r{} b{}",
                safe.left, safe.top, safe.right, safe.bottom
            ),
        );

        // The numbers themselves, for the script and for a human. This is the
        // line that answers "did the cutout reach us", and it is an observation
        // because only the operator knows what the device has.
        results.observe(
            "safe_area",
            format!(
                "l{} t{} r{} b{} on a {}x{} surface at {}x",
                safe.left, safe.top, safe.right, safe.bottom, size.width, size.height, ratio
            ),
        );
        // A keyboard is a *bottom* inset and nothing else, and it can never be
        // taller than the surface it is covering. Both hold on any device with
        // or without one up, so this is a check rather than an observation —
        // and both are exactly what goes wrong if the platform hands back a
        // rectangle in the wrong coordinate space.
        let keyboard = metrics.view_insets;
        let plausible = keyboard.bottom.is_finite()
            && keyboard.bottom >= 0.0
            && keyboard.bottom < size.height
            && keyboard.left == 0.0
            && keyboard.top == 0.0
            && keyboard.right == 0.0;
        results.check(
            "the_keyboard_inset_is_a_plausible_bottom_inset",
            plausible,
            format!("{keyboard:?} on a {}-tall surface", size.height),
        );

        // Latched at the first non-zero reading, so it survives the keyboard
        // being dismissed again before the report goes out. Its *absence* from
        // a run is the interesting outcome, and it has already been interesting
        // once: it was missing because `sync_ime` was never called on a tap, so
        // no keyboard was ever raised to have an inset. Seen at **290.55
        // logical pixels** on a Redmi Note 7 Pro once that was fixed.
        if keyboard.bottom > 0.0 {
            results.latch(
                "the_keyboard_inset_appears_when_the_keyboard_does",
                format!("{} logical pixels", keyboard.bottom),
            );
        }

        results.observe(
            "keyboard_inset",
            format!("{keyboard:?} at the moment of the report"),
        );

        // Recorded on the *rebuild* a tap causes, which is the point: reaching
        // this line at all means a MotionEvent crossed winit, the translator,
        // the router, the arena, a recogniser and a handler, and then marked
        // this element pending. `ci/mobile/device-suite.sh` sends the tap.
        if taps > 0 {
            // Latched at the count it first saw, which is normally 1 even though
            // the script sends several — it cannot know the app felt one until
            // the report, so it keeps tapping. The number is not the point; that
            // this line exists at all is.
            let plural = if taps == 1 { "tap" } else { "taps" };
            results.latch("a_tap_reached_a_handler", format!("{taps} {plural}"));
        }

        SizedBox::shrink().into()
    }
}

widget_node_from!(Probe);
