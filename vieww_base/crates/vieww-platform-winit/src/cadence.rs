//! Which frame rate a window draws at: the panel's, or 60 Hz when the panel's
//! cannot be held.
//!
//! # Why
//!
//! The frame budget used to be whatever the panel reported. On a 120 Hz phone
//! that is 8.33 ms whether or not anything can meet it, and what a person sees
//! when it is missed is not "a bit under 120" but **uneven pacing**: frames
//! shown for one vsync, then two, then one. Device Farm run 39402544760, the
//! Galaxy A73: the shipping GPU path's median was inside 8.33 ms and 52 of 240
//! frames (22%) were not. A steady 60 looks better than that ragged 120, and
//! every native toolkit and game engine that runs on these panels makes the
//! same trade.
//!
//! So a window holds one of two rates — the panel's, or 60 Hz — and moves
//! between them on evidence:
//!
//! - **Down** to 60 when, over about a second of continuous frames at the
//!   panel's rate, more than [`MAX_OVER_SHARE`] of them miss its budget.
//! - **Up** to the panel's rate when, over [`UP_WINDOWS`] such windows in a
//!   row at 60 Hz, the 95th-percentile frame would fit the panel's budget
//!   with [`UP_HEADROOM`] to spare. The gap between the two thresholds is the
//!   hysteresis that stops a phone near the line from flapping.
//!
//! What is *never* traded is resolution: rendering the interface below the
//! panel's pixel density softens text, and a framework that wants to be the
//! best one does not do that to its UI to win a frame rate. The resolution
//! scale stays an explicit opt-in (`crate::quality`).
//!
//! # What "work" is here
//!
//! The frame's own work, without the time spent waiting for a buffer to
//! present into (`acquire`): at 60 Hz on a 120 Hz panel that wait is the
//! pacing itself, and counting it would make every frame look like it needs
//! the whole 16.7 ms — the rate could never come back up.
//!
//! # Holding the rate
//!
//! The policy only decides. The window asks the platform for the rate
//! ([`request`]: `ANativeWindow_setFrameRate` on Android 11+, which lets the
//! compositor switch the panel to 60 Hz) and paces its own frames at it
//! regardless, so the cadence is steady even where the platform keeps the
//! panel at 120 — each frame is then simply shown for two vsyncs.
//!
//! An application that states its rate (`App::refresh_rate`) is never
//! adapted, and `VIEWW_CADENCE=panel` (Android: `debug.vieww.cadence`) turns
//! adaptation off for a measurement that wants the panel's rate regardless.

use std::time::Duration;

/// The rate a window falls back to.
pub(crate) const FALLBACK_HZ: f32 = 60.0;

/// More than this share of a window's frames over the panel's budget, and it
/// is not being held. One in twenty: the same share the Device Farm gate
/// allows (`ci/mobile`'s `frames_inside_their_budget`).
pub(crate) const MAX_OVER_SHARE: f64 = 0.05;

/// At 60 Hz, the 95th-percentile frame must fit this fraction of the panel's
/// budget before the panel's rate is tried again.
pub(crate) const UP_HEADROOM: f64 = 0.75;

/// Consecutive windows that must clear [`UP_HEADROOM`] before going back up.
pub(crate) const UP_WINDOWS: u32 = 3;

/// A gap between frames this long means the app went idle; the evidence so
/// far describes a different stretch of work.
///
/// A second, not a few frame intervals. Interfaces draw in bursts — a tap,
/// a flick, a keyboard sliding in — with pauses of a few hundred
/// milliseconds between them, and a reset on every pause meant the policy
/// almost never saw a whole window: Device Farm run 39437190809's S26 CPU
/// pass drew 600 frames in 5.1 s, only 34 of its last 240 back to back, had
/// 17 of them late at 120 Hz, and dropped to 60 Hz 1.3 s after the suite
/// had already judged it. The stutter was there the whole time.
const IDLE_RESET: Duration = Duration::from_secs(1);

/// Frames in a window's first second are not evidence. Start-up is the
/// slowest stretch an app has — the first raster, pipelines, fonts — and
/// counting it would drop nearly every launch on a 120 Hz phone to 60 Hz,
/// to climb back three seconds later.
const WARM_UP: Duration = Duration::from_secs(1);

/// The rate policy for one window. See the module docs.
#[derive(Debug, Clone)]
pub(crate) struct Cadence {
    panel_hz: f32,
    target_hz: f32,
    /// Work of the frames in the current window, reused between windows.
    window: Vec<Duration>,
    window_len: usize,
    /// When the last frame was observed, for gap detection.
    last: Option<Duration>,
    /// When the first frame was observed, for [`WARM_UP`].
    first: Option<Duration>,
    /// Windows in a row at the fallback that cleared the headroom.
    clear_windows: u32,
    switches: u32,
}

impl Cadence {
    /// A policy for a panel at `panel_hz`, or `None` when there is nothing to
    /// adapt: a panel at or below the fallback rate.
    #[must_use]
    pub(crate) fn new(panel_hz: f32) -> Option<Self> {
        if !panel_hz.is_finite() || panel_hz <= FALLBACK_HZ + 0.5 {
            return None;
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a refresh rate is a small positive number"
        )]
        let window_len = (panel_hz.round() as usize).max(30);
        Some(Self {
            panel_hz,
            target_hz: panel_hz,
            window: Vec::with_capacity(window_len),
            window_len,
            last: None,
            first: None,
            clear_windows: 0,
            switches: 0,
        })
    }

    /// The rate the window should draw at now.
    #[must_use]
    pub(crate) const fn target_hz(&self) -> f32 {
        self.target_hz
    }

    /// The panel's own rate.
    #[must_use]
    pub(crate) const fn panel_hz(&self) -> f32 {
        self.panel_hz
    }

    /// How many times the rate has changed.
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn switches(&self) -> u32 {
        self.switches
    }

    fn budget(hz: f32) -> Duration {
        Duration::from_secs_f32(1.0 / hz)
    }

    /// One drawn frame's `work` (without its wait for a buffer), finished at
    /// `now`. Returns the new rate when this frame decided a change.
    pub(crate) fn observe(&mut self, work: Duration, now: Duration) -> Option<f32> {
        if let Some(last) = self.last {
            if now.saturating_sub(last) >= IDLE_RESET {
                self.window.clear();
            }
        }
        self.last = Some(now);
        if now.saturating_sub(*self.first.get_or_insert(now)) < WARM_UP {
            return None;
        }
        self.window.push(work);
        if self.window.len() < self.window_len {
            return None;
        }
        let panel_budget = Self::budget(self.panel_hz);
        let decision = if self.target_hz >= self.panel_hz {
            let over = self.window.iter().filter(|&&w| w > panel_budget).count();
            #[expect(clippy::cast_precision_loss, reason = "frame counts")]
            let share = over as f64 / self.window.len() as f64;
            (share > MAX_OVER_SHARE).then_some(FALLBACK_HZ)
        } else {
            self.window.sort_unstable();
            let p95 = self.window[(self.window.len() * 95 / 100).min(self.window.len() - 1)];
            if p95.as_secs_f64() <= panel_budget.as_secs_f64() * UP_HEADROOM {
                self.clear_windows += 1;
            } else {
                self.clear_windows = 0;
            }
            (self.clear_windows >= UP_WINDOWS).then_some(self.panel_hz)
        };
        self.window.clear();
        let hz = decision?;
        self.target_hz = hz;
        self.clear_windows = 0;
        self.switches += 1;
        Some(hz)
    }
}

/// Whether adaptation is switched off for this process (`VIEWW_CADENCE=panel`).
#[must_use]
pub(crate) fn disabled() -> bool {
    std::env::var("VIEWW_CADENCE").is_ok_and(|v| v.trim().eq_ignore_ascii_case("panel"))
}

/// Ask the platform to show this window at `hz`.
///
/// Android 11+ (`ANativeWindow_setFrameRate`, looked up at run time so the
/// library still loads on older phones, where this does nothing and the
/// window's own pacing holds the rate). Returns what happened, for the log.
#[cfg(target_os = "android")]
pub(crate) fn request(window: &winit::window::Window, hz: f32) -> &'static str {
    use std::ffi::c_void;
    use std::sync::OnceLock;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    type SetFrameRate = unsafe extern "C" fn(*mut c_void, f32, i8) -> i32;
    static FUNCTION: OnceLock<Option<SetFrameRate>> = OnceLock::new();
    let function = FUNCTION.get_or_init(|| {
        // SAFETY: `dlopen` of a system library by name, never closed.
        let library = unsafe { libc::dlopen(c"libandroid.so".as_ptr(), libc::RTLD_NOW) };
        if library.is_null() {
            return None;
        }
        // SAFETY: `library` is live and the name is NUL-terminated.
        let address = unsafe { libc::dlsym(library, c"ANativeWindow_setFrameRate".as_ptr()) };
        if address.is_null() {
            return None;
        }
        // SAFETY: the NDK's signature (android/native_window.h, API 30).
        Some(unsafe { std::mem::transmute::<*mut c_void, SetFrameRate>(address) })
    });
    let Some(set_frame_rate) = function else {
        return "not asked (Android 10 or older: no ANativeWindow_setFrameRate)";
    };
    let Ok(handle) = window.window_handle() else {
        return "not asked (no window handle)";
    };
    let RawWindowHandle::AndroidNdk(handle) = handle.as_raw() else {
        return "not asked (not an Android window)";
    };
    // ANATIVEWINDOW_FRAME_RATE_COMPATIBILITY_DEFAULT: the app's own content
    // rate, which lets the compositor pick a panel mode that divides it.
    const COMPATIBILITY_DEFAULT: i8 = 0;
    // SAFETY: a live `ANativeWindow*` from winit, owned by the window that
    // is borrowed for this call.
    let status =
        unsafe { set_frame_rate(handle.a_native_window.as_ptr(), hz, COMPATIBILITY_DEFAULT) };
    if status == 0 {
        "asked (ANativeWindow_setFrameRate)"
    } else {
        "refused (ANativeWindow_setFrameRate returned an error)"
    }
}

/// Ask the platform to show this window at `hz`: nothing to ask off Android.
/// iOS follows the rate through its display link (`crate::ios`), desktops
/// present at the monitor's rate and the window paces itself.
#[cfg(not(target_os = "android"))]
pub(crate) const fn request(_window: &winit::window::Window, _hz: f32) -> &'static str {
    "not asked (no platform frame-rate request here; paced by the window)"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(v: f64) -> Duration {
        Duration::from_secs_f64(v / 1000.0)
    }

    /// Feed `frames` frames of `work`, one interval apart; return every
    /// decision made.
    fn run(
        c: &mut Cadence,
        start: &mut Duration,
        frames: usize,
        work: impl Fn(usize) -> f64,
    ) -> Vec<f32> {
        let mut out = Vec::new();
        for i in 0..frames {
            *start += Duration::from_secs_f32(1.0 / c.target_hz());
            if let Some(hz) = c.observe(ms(work(i)), *start) {
                out.push(hz);
            }
        }
        out
    }

    #[test]
    fn a_60_hz_panel_has_nothing_to_adapt() {
        assert!(Cadence::new(60.0).is_none());
        assert!(Cadence::new(59.94).is_none());
        assert!(Cadence::new(f32::NAN).is_none());
        assert!(Cadence::new(90.0).is_some());
    }

    /// Past [`WARM_UP`], where the evidence starts.
    fn warmed(c: &mut Cadence, t: &mut Duration) {
        run(c, t, 120, |_| 40.0);
        assert!(
            (c.target_hz() - c.panel_hz()).abs() < f32::EPSILON,
            "start-up is not evidence"
        );
    }

    #[test]
    fn a_slow_start_up_is_not_a_reason_to_drop() {
        let mut c = Cadence::new(120.0).unwrap();
        let mut t = Duration::ZERO;
        // A second of 40 ms frames — a first raster, pipelines — then a
        // phone that holds its rate.
        warmed(&mut c, &mut t);
        let decisions = run(&mut c, &mut t, 600, |_| 6.0);
        assert!(decisions.is_empty(), "{decisions:?}");
    }

    #[test]
    fn a_phone_that_holds_120_stays_at_120() {
        let mut c = Cadence::new(120.0).unwrap();
        let mut t = Duration::ZERO;
        warmed(&mut c, &mut t);
        // Every 25th frame over: 4%, inside the allowance.
        let decisions = run(
            &mut c,
            &mut t,
            1200,
            |i| if i % 25 == 0 { 9.5 } else { 6.0 },
        );
        assert!(decisions.is_empty(), "{decisions:?}");
        assert!((c.target_hz() - 120.0).abs() < f32::EPSILON);
    }

    #[test]
    fn the_a73s_ragged_120_falls_to_a_steady_60() {
        // Run 39402544760's auto pass: median 7.7 ms, 22% over 8.33 ms.
        let mut c = Cadence::new(120.0).unwrap();
        let mut t = Duration::ZERO;
        warmed(&mut c, &mut t);
        let decisions = run(&mut c, &mut t, 120, |i| if i % 9 < 2 { 10.0 } else { 7.5 });
        assert_eq!(decisions, vec![60.0]);
        assert_eq!(c.switches(), 1);
    }

    #[test]
    fn it_comes_back_up_only_with_room_to_spare_and_does_not_flap() {
        let mut c = Cadence::new(120.0).unwrap();
        let mut t = Duration::ZERO;
        warmed(&mut c, &mut t);
        run(&mut c, &mut t, 120, |_| 12.0);
        assert!((c.target_hz() - 60.0).abs() < f32::EPSILON);
        // Just under the panel's budget is not enough room: it would fall
        // straight back.
        let decisions = run(&mut c, &mut t, 120 * 6, |_| 8.0);
        assert!(
            decisions.is_empty(),
            "8 ms is not 25% under 8.33: {decisions:?}"
        );
        // Comfortably under for three windows: back to the panel.
        let decisions = run(&mut c, &mut t, 120 * 3, |_| 5.0);
        assert_eq!(decisions, vec![120.0]);
        // One noisy window at 60 resets the streak.
        let mut c = Cadence::new(120.0).unwrap();
        warmed(&mut c, &mut t);
        run(&mut c, &mut t, 120, |_| 12.0);
        let decisions = run(&mut c, &mut t, 120 * 3, |i| {
            if (120..240).contains(&i) {
                9.0
            } else {
                5.0
            }
        });
        assert!(
            decisions.is_empty(),
            "a bad window in the middle must reset the streak"
        );
    }

    #[test]
    fn evidence_carries_across_the_pauses_between_bursts() {
        let mut c = Cadence::new(120.0).unwrap();
        let mut t = Duration::ZERO;
        warmed(&mut c, &mut t);
        // Bursts of 30 frames with 250 ms between them — taps and flicks —
        // and 3 of every 30 late (10%, twice the allowance). The window
        // fills across bursts, so the rate drops within a few of them.
        let mut decisions = Vec::new();
        for _ in 0..8 {
            decisions.extend(run(&mut c, &mut t, 30, |i| {
                if i % 10 == 0 {
                    12.0
                } else {
                    6.0
                }
            }));
            t += Duration::from_millis(250);
        }
        assert_eq!(decisions.first().copied(), Some(60.0), "{decisions:?}");
    }

    #[test]
    fn a_pause_in_the_animation_starts_the_evidence_again() {
        let mut c = Cadence::new(120.0).unwrap();
        let mut t = Duration::ZERO;
        warmed(&mut c, &mut t);
        // Half a window of slow frames, a pause, then half a window of fast
        // ones: no single stretch of work was a whole window, so no verdict.
        run(&mut c, &mut t, 60, |_| 12.0);
        t += Duration::from_secs(2);
        let decisions = run(&mut c, &mut t, 60, |_| 5.0);
        assert!(decisions.is_empty(), "{decisions:?}");
        assert!((c.target_hz() - 120.0).abs() < f32::EPSILON);
    }
}
