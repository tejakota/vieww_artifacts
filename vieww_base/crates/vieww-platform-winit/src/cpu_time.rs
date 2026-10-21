//! CPU time this process has spent, every thread together — what a frame
//! costs the battery on the CPU side, rather than how long it took.
//!
//! # Why
//!
//! Frame time says whether a frame was on time, not what it cost: a frame
//! drawn in 5 ms by four threads costs more battery than one drawn in 8 ms by
//! one. The cross-framework record compares stacks by exactly that (Lynx's
//! 23% lower CPU under sustained load). Device Farm phones run on a charger,
//! so battery drain itself cannot be read there; CPU time per frame, with the
//! GPU's own time per frame (`GpuPassTimes`), is the measurable stand-in.
//!
//! Linux and Android: `utime + stime` from `/proc/self/stat`, in clock ticks
//! (`sysconf(_SC_CLK_TCK)`, 100 on every kernel either ships with). `None`
//! elsewhere.

use std::time::Duration;

/// CPU time the whole process has used since it started, user and system,
/// every thread. `None` where the platform does not say.
#[must_use]
pub fn process_cpu_time() -> Option<Duration> {
    imp::read()
}

/// `utime + stime`, in ticks, from the text of `/proc/<pid>/stat`.
///
/// The command name (field 2) is in parentheses and may itself contain
/// spaces or parentheses, so the fields are counted from the **last** `)`:
/// after it come field 3 (state) onward, and `utime`, `stime` are fields 14
/// and 15.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn ticks_from_stat(stat: &str) -> Option<u64> {
    let after = &stat[stat.rfind(')')? + 1..];
    let fields: Vec<&str> = after.split_whitespace().collect();
    // fields[0] is field 3, so field n is fields[n - 3].
    let utime: u64 = fields.get(14 - 3)?.parse().ok()?;
    let stime: u64 = fields.get(15 - 3)?.parse().ok()?;
    Some(utime + stime)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
mod imp {
    use std::time::Duration;

    pub(super) fn read() -> Option<Duration> {
        let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
        let ticks = super::ticks_from_stat(&stat)?;
        // SAFETY: a plain query with no arguments to validate.
        let per_second = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
        let per_second = u64::try_from(per_second).ok().filter(|t| *t > 0)?;
        Some(Duration::from_nanos(
            ticks.saturating_mul(1_000_000_000) / per_second,
        ))
    }
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
mod imp {
    use std::time::Duration;

    pub(super) const fn read() -> Option<Duration> {
        None
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "android")))]
mod tests {
    use super::*;

    #[test]
    fn utime_and_stime_are_read_past_a_command_name_with_spaces_and_parens() {
        // pid (comm) state ppid pgrp session tty tpgid flags minflt cminflt
        // majflt cmajflt utime stime …
        let stat = "4242 (a (weird) name) R 1 4242 4242 0 -1 4194560 100 0 0 0 37 5 0 0 20 0";
        assert_eq!(ticks_from_stat(stat), Some(42));
        assert_eq!(ticks_from_stat("garbage"), None);
    }

    #[test]
    fn this_process_has_used_some_cpu_and_more_after_working() {
        let before = process_cpu_time().expect("Linux has /proc/self/stat");
        let start = std::time::Instant::now();
        let mut x = 0u64;
        while start.elapsed() < Duration::from_millis(120) {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
            std::hint::black_box(x);
        }
        let after = process_cpu_time().expect("still there");
        // Ticks are 10 ms; 120 ms of spinning is several of them.
        assert!(
            after >= before + Duration::from_millis(40),
            "{before:?} -> {after:?}"
        );
    }
}
