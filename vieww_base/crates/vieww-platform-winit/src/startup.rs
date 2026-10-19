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
//! started: on Linux and Android from `/proc/self/stat` against the boot
//! clock, at the kernel's 10 ms clock-tick resolution; on iOS and macOS from
//! `proc_pidinfo`. Elsewhere (Windows) it is reported as unknown rather than
//! guessed.

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

static PIPELINE_CACHE: Mutex<Option<String>> = Mutex::new(None);

/// Record what the GPU pipeline cache started from and what the first GPU
/// frame saved — the GPU path's first frame was 82–1463 ms on every launch
/// before the cache existed (Device Farm, 2026-10-17).
pub(crate) fn note_pipeline_cache(seed: usize, saved: Option<std::io::Result<usize>>) {
    let saved = match saved {
        Some(Ok(bytes)) => format!("saved {bytes} bytes after the first GPU frame"),
        Some(Err(error)) => format!("could not be saved ({error})"),
        None => "no GPU device".to_string(),
    };
    let text = format!("started from {seed} bytes; {saved}");
    log::info!("vieww: pipeline cache {text}");
    if let Ok(mut status) = PIPELINE_CACHE.lock() {
        *status = Some(text);
    }
}

/// What the GPU pipeline cache did this launch — `None` until a GPU frame
/// has been drawn.
#[must_use]
pub fn pipeline_cache_status() -> Option<String> {
    PIPELINE_CACHE.lock().ok()?.clone()
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
        line.push_str(&format!(
            " | {label} {:.1}",
            ms(at.saturating_sub(previous))
        ));
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
///
/// **Not `/proc/uptime`.** The first version read it, and on Device Farm
/// (2026-10-17) every Android phone reported "before run unknown": an app's
/// SELinux domain may not read it. The boot clock is a system call away
/// instead, and `/proc/self/stat` is always the process's own to read.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn process_age() -> Option<Duration> {
    // Field 22 of /proc/self/stat is the start time in clock ticks since boot.
    // The second field (the command) may contain spaces, so fields are counted
    // after its closing parenthesis.
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let after = stat.rsplit_once(')')?.1;
    // `after` starts at field 3, so field 22 is index 19.
    let ticks: f64 = after.split_whitespace().nth(19)?.parse().ok()?;
    // SAFETY: `sysconf` takes no pointers; `clock_gettime` writes one
    // `timespec`, which lives on this stack frame for the call.
    let (hz, now) = unsafe {
        let hz = libc::sysconf(libc::_SC_CLK_TCK);
        let mut now = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        if hz <= 0 || libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut now) != 0 {
            return None;
        }
        (hz, now)
    };
    #[expect(clippy::cast_precision_loss, reason = "seconds since boot")]
    let boot = now.tv_sec as f64 + now.tv_nsec as f64 / 1e9;
    #[expect(clippy::cast_precision_loss, reason = "clock ticks per second")]
    let age = boot - ticks / hz as f64;
    (0.0..3600.0)
        .contains(&age)
        .then(|| Duration::from_secs_f64(age))
}

/// How long ago this process started, from the kernel's own record of it
/// (`proc_pidinfo`, which a sandboxed iOS app may ask about itself), against
/// the wall clock — microsecond resolution.
// `allow`, not `expect`: this is built only for Apple targets, where clippy
// is not run here, and an unfulfilled expectation would fail the build.
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn process_age() -> Option<Duration> {
    // SAFETY: `proc_pidinfo` writes at most `size` bytes into `info`, a
    // zeroed plain-data struct on this stack frame; `gettimeofday` writes one
    // `timeval` the same way.
    let (info, now) = unsafe {
        let mut info: libc::proc_bsdinfo = std::mem::zeroed();
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // a small struct
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
        let written = libc::proc_pidinfo(
            libc::getpid(),
            libc::PROC_PIDTBSDINFO,
            0,
            std::ptr::addr_of_mut!(info).cast(),
            size,
        );
        let mut now = libc::timeval {
            tv_sec: 0,
            tv_usec: 0,
        };
        if written != size || libc::gettimeofday(&mut now, std::ptr::null_mut()) != 0 {
            return None;
        }
        (info, now)
    };
    #[allow(clippy::cast_precision_loss)] // seconds since the epoch
    let start = info.pbi_start_tvsec as f64 + info.pbi_start_tvusec as f64 / 1e6;
    #[allow(clippy::cast_precision_loss)] // seconds since the epoch
    let now = now.tv_sec as f64 + f64::from(now.tv_usec) / 1e6;
    let age = now - start;
    (0.0..3600.0)
        .contains(&age)
        .then(|| Duration::from_secs_f64(age))
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "ios",
    target_os = "macos"
)))]
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
            &[
                ("window created", ms(12)),
                ("present ready", ms(47)),
                ("first frame", ms(100)),
            ],
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
