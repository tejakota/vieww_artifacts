//! Where start-up time goes, in stages.
//!
//! # Why this exists
//!
//! Device Farm measured cold start at 217–285 ms on a Galaxy A36 and 270–370 ms
//! on three iPhones (2026-10-17), against the audit's 120 ms on a mid-tier
//! phone — and the only number was the total. `VIEWW_TRACE_FRAMES` has printed
//! a `startup:` line per step for a long time, but only when tracing is on, and
//! a cold start measured with tracing on is not a cold start.
//!
//! So the steps are recorded unconditionally: a handful of `(label, time)`
//! pairs, pushed once per process up to the first presented frame and never
//! again. [`report`] turns them into one line the device suite prints.
//!
//! # What "time" is
//!
//! Every mark is relative to [`begin`], which `App::run` calls first — the same
//! instant as the app's frame epoch. What happened **before** `App::run` (the
//! process being created, the dynamic linker loading the library, `main`'s own
//! setup) is measured separately where the OS can say when the process
//! started: on Linux and Android from `/proc/self/stat`, at the kernel's
//! 10 ms clock-tick resolution. Elsewhere it is reported as unknown rather
//! than guessed.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// One step: what had just finished, and when, since [`begin`].
type Mark = (&'static str, Duration);

struct Marks {
    start: Instant,
    /// Process age at [`begin`], where the OS can say.
    before_run: Option<Duration>,
    steps: Vec<Mark>,
    /// Set by the first presented frame; no marks are taken after it.
    done: bool,
}

static MARKS: OnceLock<Mutex<Marks>> = OnceLock::new();

/// Start the clock. Idempotent: a second `App::run` in one process keeps the
/// first start-up, which is the only one that was a start-up.
pub(crate) fn begin() {
    MARKS.get_or_init(|| {
        Mutex::new(Marks {
            start: Instant::now(),
            before_run: process_age(),
            steps: Vec::with_capacity(8),
            done: false,
        })
    });
}

/// Record that `label` has just finished, the first time it does. Free after
/// the first frame.
pub(crate) fn mark(label: &'static str) {
    let Some(marks) = MARKS.get() else { return };
    let Ok(mut marks) = marks.lock() else { return };
    // Once per label: the first frame's steps run on every `draw` until one
    // presents, and only the first of each is the start-up.
    if marks.done || marks.steps.iter().any(|(seen, _)| *seen == label) {
        return;
    }
    let at = marks.start.elapsed();
    marks.steps.push((label, at));
}

/// Record the first presented frame and stop recording.
pub(crate) fn first_frame() {
    mark("first raster and present");
    if let Some(Ok(mut marks)) = MARKS.get().map(Mutex::lock) {
        marks.done = true;
    }
}

/// The start-up, as one line: the time before `App::run`, then each step as
/// the time it took since the previous one, then the total from process start
/// (or from `App::run` when the process start is unknown).
///
/// `None` until the first frame has been presented.
///
/// ```text
/// before run 40 ms | event loop 3.0 | window created 12.1 | present ready 35.4 | … | first build and layout 9.2 | first raster and present 30.1 = 171.5 ms from process start
/// ```
#[must_use]
pub fn report() -> Option<String> {
    let marks = MARKS.get()?.lock().ok()?;
    if !marks.done {
        return None;
    }
    Some(format_steps(marks.before_run, &marks.steps))
}

fn format_steps(before_run: Option<Duration>, steps: &[Mark]) -> String {
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    let mut line = match before_run {
        Some(age) => format!("before run {:.0} ms", ms(age)),
        None => "before run unknown".to_string(),
    };
    let mut previous = Duration::ZERO;
    for (label, at) in steps {
        line.push_str(&format!(" | {label} {:.1}", ms(at.saturating_sub(previous))));
        previous = *at;
    }
    let total = previous + before_run.unwrap_or_default();
    line.push_str(&format!(
        " = {:.1} ms from {}",
        ms(total),
        if before_run.is_some() {
            "process start"
        } else {
            "App::run"
        }
    ));
    line
}

/// How long ago this process started, where the OS says.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn process_age() -> Option<Duration> {
    // Field 22 of /proc/self/stat is the start time in clock ticks since boot;
    // /proc/uptime is seconds since boot. Both count from the same boot clock.
    // The second field (the command) may contain spaces, so fields are counted
    // after its closing parenthesis.
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let after = stat.rsplit_once(')')?.1;
    // `after` starts at field 3, so field 22 is index 19.
    let ticks: f64 = after.split_whitespace().nth(19)?.parse().ok()?;
    let uptime: f64 = std::fs::read_to_string("/proc/uptime")
        .ok()?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    // USER_HZ is 100 on every Linux and Android ABI; it is part of the
    // userspace ABI and does not follow the kernel's internal HZ.
    let age = uptime - ticks / 100.0;
    (0.0..3600.0).contains(&age).then(|| Duration::from_secs_f64(age))
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
const fn process_age() -> Option<Duration> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_are_reported_as_the_time_each_one_took() {
        let ms = Duration::from_millis;
        let line = format_steps(
            Some(ms(40)),
            &[("window created", ms(12)), ("present ready", ms(47)), ("first frame", ms(100))],
        );
        assert_eq!(
            line,
            "before run 40 ms | window created 12.0 | present ready 35.0 | first frame 53.0 \
             = 140.0 ms from process start"
        );
        assert!(format_steps(None, &[("first frame", ms(5))]).ends_with("= 5.0 ms from App::run"));
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[test]
    fn a_running_process_knows_roughly_how_old_it_is() {
        let age = process_age().expect("/proc is readable");
        assert!(age < Duration::from_secs(3600), "{age:?}");
    }
}
