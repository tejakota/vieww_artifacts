//! Real-time thread priority for the threads this crate owns.
//!
//! An audio thread at ordinary priority competes with everything else on
//! the machine: a build, a browser, a garbage collector. When the
//! scheduler preempts it for longer than one buffer, the device underruns
//! and the listener hears a click — however fast the DSP is. The audit's
//! timing probe measured exactly that: an 8-voice render averaging 0.12 ms
//! per 2.67 ms block still overran the block once in ten seconds.
//!
//! Core Audio, JACK and AAudio already call back on real-time threads they
//! own. Everything else here runs on a thread this crate spawns — the
//! [`Stream`](crate::stream::Stream) render thread, the ALSA, PulseAudio
//! and WASAPI device threads, the multi-core voice workers — and each of
//! those calls [`promote_audio_thread`] as its first act:
//!
//! * **Linux / Android**: `SCHED_FIFO` (with `SCHED_RESET_ON_FORK`, so a
//!   child process never inherits it) at priority 70 of 1–99, below the
//!   kernel's own threads; failing that (no `CAP_SYS_NICE`, no `rtprio`
//!   limit — the common desktop case), the thread's nice value to −11,
//!   failing that, unchanged.
//! * **Windows**: MMCSS, the "Pro Audio" task at high priority — the
//!   scheduler class Windows built for exactly this — loaded from
//!   `avrt.dll` at run time like every other system library here; failing
//!   that, `THREAD_PRIORITY_TIME_CRITICAL`.
//! * **macOS / iOS**: a Mach time-constraint policy sized to the block
//!   period — the policy Core Audio's own I/O threads use.
//!
//! Nothing fails loudly: a thread that cannot be promoted keeps running at
//! the priority it has, and the [`Promotion`] it got back says what
//! happened, so the stream can report it (an app can then tell the user
//! to add themselves to the `audio` group, say). Set `VIEWW_AUDIO_NO_RT=1`
//! to skip promotion entirely (for profiling, or a shared CI box).

use std::time::Duration;

/// The scheduling class a thread ended up in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    /// Ordinary time-sharing; promotion was refused or skipped.
    Normal,
    /// Above normal but not real-time (a negative nice value, a raised
    /// Windows thread priority, a high QoS class).
    Elevated,
    /// A real-time class (`SCHED_FIFO`, MMCSS Pro Audio, Mach time
    /// constraint).
    Realtime,
}

impl Level {
    /// As a byte, for an atomic counter: 0, 1, 2.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::Elevated => 1,
            Self::Realtime => 2,
        }
    }

    /// From [`as_u8`](Self::as_u8).
    #[must_use]
    pub const fn from_u8(v: u8) -> Self {
        match v {
            2 => Self::Realtime,
            1 => Self::Elevated,
            _ => Self::Normal,
        }
    }
}

/// What promoting the calling thread achieved, and why if not all.
#[derive(Debug)]
pub struct Promotion {
    /// The class the thread is now in.
    pub level: Level,
    /// The mechanism used, or the reason none worked.
    pub detail: String,
    /// Undoes the promotion when dropped, where the platform needs it
    /// (MMCSS registrations are per-thread handles).
    _guard: Guard,
}

impl Promotion {
    fn new(level: Level, detail: impl Into<String>) -> Self {
        Self {
            level,
            detail: detail.into(),
            _guard: Guard::None,
        }
    }
}

/// Platform state to release on drop.
#[derive(Debug)]
enum Guard {
    None,
    #[cfg(windows)]
    Mmcss {
        lib: crate::backend::dl::Library,
        handle: *mut std::ffi::c_void,
    },
}

// SAFETY: the MMCSS handle is only used from the thread that created it —
// the guard is created and dropped on that thread — and the library handle
// is thread-safe.
unsafe impl Send for Guard {}

#[cfg(windows)]
impl Drop for Guard {
    fn drop(&mut self) {
        if let Self::Mmcss { lib, handle } = self {
            type Revert = unsafe extern "system" fn(*mut std::ffi::c_void) -> i32;
            // SAFETY: the symbol's true type; a handle from
            // AvSetMmThreadCharacteristicsW on this thread.
            unsafe {
                if let Ok(revert) = lib.get::<Revert>(c"AvRevertMmThreadCharacteristics") {
                    revert(*handle);
                }
            }
        }
    }
}

fn disabled() -> bool {
    std::env::var_os("VIEWW_AUDIO_NO_RT").is_some_and(|v| !v.is_empty() && v != "0")
}

/// Promote the calling thread to real-time audio priority, for a thread
/// that renders one block every `period`. Best effort; see the module
/// docs. Call it first thing on the thread; keep the returned value alive
/// for the thread's life.
#[must_use]
pub fn promote_audio_thread(period: Duration) -> Promotion {
    if disabled() {
        return Promotion::new(Level::Normal, "VIEWW_AUDIO_NO_RT is set");
    }
    imp::realtime(period)
}

/// Raise the calling thread above normal without making it real-time —
/// for helpers that feed the audio thread but block on I/O (a disk
/// reader, a MIDI input), where a real-time class would be a hazard.
#[must_use]
pub fn promote_helper_thread() -> Promotion {
    if disabled() {
        return Promotion::new(Level::Normal, "VIEWW_AUDIO_NO_RT is set");
    }
    imp::elevated()
}

/// The calling thread's scheduling class as the OS reports it now —
/// independent of what promotion claimed, for checking.
#[must_use]
pub fn current_level() -> Level {
    imp::current()
}

#[cfg(any(target_os = "linux", target_os = "android"))]
mod imp {
    use super::{Level, Promotion};
    use std::ffi::c_int;
    use std::time::Duration;

    /// `struct sched_param`: glibc's is one `int`; musl's carries reserved
    /// fields after it. Over-allocated and zeroed so either reads safely.
    #[repr(C)]
    struct SchedParam {
        sched_priority: c_int,
        _reserved: [u8; 60],
    }

    unsafe extern "C" {
        fn pthread_self() -> usize;
        fn pthread_setschedparam(thread: usize, policy: c_int, param: *const SchedParam) -> c_int;
        fn pthread_getschedparam(
            thread: usize,
            policy: *mut c_int,
            param: *mut SchedParam,
        ) -> c_int;
        fn sched_get_priority_min(policy: c_int) -> c_int;
        fn sched_get_priority_max(policy: c_int) -> c_int;
        fn setpriority(which: c_int, who: u32, prio: c_int) -> c_int;
        fn getpriority(which: c_int, who: u32) -> c_int;
    }

    const SCHED_OTHER: c_int = 0;
    const SCHED_FIFO: c_int = 1;
    const SCHED_RR: c_int = 2;
    const SCHED_RESET_ON_FORK: c_int = 0x4000_0000;
    const PRIO_PROCESS: c_int = 0;

    fn param(p: c_int) -> SchedParam {
        SchedParam {
            sched_priority: p,
            _reserved: [0; 60],
        }
    }

    pub(super) fn realtime(_period: Duration) -> Promotion {
        // SAFETY: plain libc calls with valid arguments.
        let (lo, hi) = unsafe {
            (
                sched_get_priority_min(SCHED_FIFO),
                sched_get_priority_max(SCHED_FIFO),
            )
        };
        let prio = 70.clamp(lo, hi);
        let mut why = String::new();
        for policy in [SCHED_FIFO | SCHED_RESET_ON_FORK, SCHED_FIFO] {
            // SAFETY: the calling thread; a valid, zeroed parameter.
            let rc = unsafe { pthread_setschedparam(pthread_self(), policy, &param(prio)) };
            if rc == 0 {
                return Promotion::new(Level::Realtime, format!("SCHED_FIFO priority {prio}"));
            }
            why = std::io::Error::from_raw_os_error(rc).to_string();
        }
        let mut p = elevated();
        p.detail = format!("SCHED_FIFO refused ({why}); {}", p.detail);
        p
    }

    pub(super) fn elevated() -> Promotion {
        // On Linux the nice value is per thread, and `who = 0` is the
        // calling thread (setpriority(2), NOTES).
        for nice in [-11, -5] {
            // SAFETY: plain libc call.
            if unsafe { setpriority(PRIO_PROCESS, 0, nice) } == 0 {
                return Promotion::new(Level::Elevated, format!("nice {nice}"));
            }
        }
        Promotion::new(
            Level::Normal,
            "no permission to raise priority (add the user to the `audio` group with an rtprio limit, or grant CAP_SYS_NICE)",
        )
    }

    pub(super) fn current() -> Level {
        let mut policy: c_int = 0;
        let mut p = param(0);
        // SAFETY: the calling thread; valid out-pointers.
        let rc = unsafe { pthread_getschedparam(pthread_self(), &mut policy, &mut p) };
        if rc == 0 && matches!(policy & !SCHED_RESET_ON_FORK, SCHED_FIFO | SCHED_RR) {
            return Level::Realtime;
        }
        if rc == 0 && policy & !SCHED_RESET_ON_FORK != SCHED_OTHER {
            return Level::Normal;
        }
        // SAFETY: plain libc call (errno ambiguity is irrelevant: any
        // negative value is a raised priority).
        if unsafe { getpriority(PRIO_PROCESS, 0) } < 0 {
            Level::Elevated
        } else {
            Level::Normal
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::{Guard, Level, Promotion};
    use crate::backend::dl::Library;
    use std::ffi::c_void;
    use std::time::Duration;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut c_void;
        fn SetThreadPriority(thread: *mut c_void, priority: i32) -> i32;
        fn GetThreadPriority(thread: *mut c_void) -> i32;
    }

    const THREAD_PRIORITY_ABOVE_NORMAL: i32 = 1;
    const THREAD_PRIORITY_HIGHEST: i32 = 2;
    const THREAD_PRIORITY_TIME_CRITICAL: i32 = 15;
    const AVRT_PRIORITY_HIGH: i32 = 1;

    type SetChars = unsafe extern "system" fn(*const u16, *mut u32) -> *mut c_void;
    type SetPrio = unsafe extern "system" fn(*mut c_void, i32) -> i32;

    pub(super) fn realtime(_period: Duration) -> Promotion {
        let mmcss = (|| -> Result<Promotion, String> {
            let lib = Library::open(&["avrt.dll"])?;
            // SAFETY: the symbols' true types (avrt.h).
            let (set, prio) = unsafe {
                (
                    lib.get::<SetChars>(c"AvSetMmThreadCharacteristicsW")?,
                    lib.get::<SetPrio>(c"AvSetMmThreadPriority")?,
                )
            };
            let task: Vec<u16> = "Pro Audio".encode_utf16().chain(Some(0)).collect();
            let mut index = 0u32;
            // SAFETY: a valid wide string and out-pointer.
            let handle = unsafe { set(task.as_ptr(), &mut index) };
            if handle.is_null() {
                return Err(format!(
                    "AvSetMmThreadCharacteristicsW: {}",
                    std::io::Error::last_os_error()
                ));
            }
            // SAFETY: a live MMCSS handle.
            unsafe { prio(handle, AVRT_PRIORITY_HIGH) };
            Ok(Promotion {
                level: Level::Realtime,
                detail: "MMCSS Pro Audio, high".into(),
                _guard: Guard::Mmcss { lib, handle },
            })
        })();
        match mmcss {
            Ok(p) => p,
            Err(why) => {
                // SAFETY: the pseudo-handle of the calling thread.
                if unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL) }
                    != 0
                {
                    Promotion::new(
                        Level::Realtime,
                        format!("MMCSS unavailable ({why}); THREAD_PRIORITY_TIME_CRITICAL"),
                    )
                } else {
                    Promotion::new(
                        Level::Normal,
                        format!("MMCSS unavailable ({why}); SetThreadPriority refused"),
                    )
                }
            }
        }
    }

    pub(super) fn elevated() -> Promotion {
        // SAFETY: the pseudo-handle of the calling thread.
        if unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_ABOVE_NORMAL) } != 0 {
            Promotion::new(Level::Elevated, "THREAD_PRIORITY_ABOVE_NORMAL")
        } else {
            Promotion::new(Level::Normal, "SetThreadPriority refused")
        }
    }

    pub(super) fn current() -> Level {
        // MMCSS shows up as a raised thread priority too.
        // SAFETY: the pseudo-handle of the calling thread.
        let p = unsafe { GetThreadPriority(GetCurrentThread()) };
        if p >= THREAD_PRIORITY_HIGHEST {
            Level::Realtime
        } else if p > 0 {
            Level::Elevated
        } else {
            Level::Normal
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod imp {
    use super::{Level, Promotion};
    use std::time::Duration;

    #[repr(C)]
    struct TimebaseInfo {
        numer: u32,
        denom: u32,
    }

    /// `thread_time_constraint_policy_data_t`.
    #[repr(C)]
    struct TimeConstraint {
        period: u32,
        computation: u32,
        constraint: u32,
        preemptible: i32,
    }

    unsafe extern "C" {
        fn mach_thread_self() -> u32;
        fn mach_timebase_info(info: *mut TimebaseInfo) -> i32;
        fn thread_policy_set(thread: u32, flavor: u32, info: *mut i32, count: u32) -> i32;
        fn thread_policy_get(
            thread: u32,
            flavor: u32,
            info: *mut i32,
            count: *mut u32,
            get_default: *mut i32,
        ) -> i32;
        fn mach_port_deallocate(task: u32, name: u32) -> i32;
        fn pthread_set_qos_class_self_np(class: u32, relative: i32) -> i32;
        static mach_task_self_: u32;
    }

    const THREAD_TIME_CONSTRAINT_POLICY: u32 = 2;
    const THREAD_TIME_CONSTRAINT_POLICY_COUNT: u32 = 4;
    const QOS_CLASS_USER_INTERACTIVE: u32 = 0x21;

    fn abs_time(d: Duration) -> u32 {
        let mut tb = TimebaseInfo { numer: 0, denom: 0 };
        // SAFETY: a valid out-pointer.
        unsafe { mach_timebase_info(&mut tb) };
        let ns = d.as_nanos();
        let t = ns * u128::from(tb.denom.max(1)) / u128::from(tb.numer.max(1));
        u32::try_from(t).unwrap_or(u32::MAX)
    }

    pub(super) fn realtime(period: Duration) -> Promotion {
        let period = period.clamp(Duration::from_micros(500), Duration::from_millis(100));
        // Compute for up to half the period, finish within the period —
        // the shape Core Audio's own I/O threads request.
        let mut policy = TimeConstraint {
            period: abs_time(period),
            computation: abs_time(period / 2),
            constraint: abs_time(period),
            preemptible: 1,
        };
        // SAFETY: Mach calls on the current thread with a correctly laid
        // out policy of the declared count; the thread port is released.
        let rc = unsafe {
            let thread = mach_thread_self();
            let rc = thread_policy_set(
                thread,
                THREAD_TIME_CONSTRAINT_POLICY,
                std::ptr::from_mut(&mut policy).cast(),
                THREAD_TIME_CONSTRAINT_POLICY_COUNT,
            );
            mach_port_deallocate(mach_task_self_, thread);
            rc
        };
        if rc == 0 {
            Promotion::new(
                Level::Realtime,
                format!("Mach time constraint, period {period:?}"),
            )
        } else {
            let mut p = elevated();
            p.detail = format!("thread_policy_set failed ({rc}); {}", p.detail);
            p
        }
    }

    pub(super) fn elevated() -> Promotion {
        // SAFETY: a plain libpthread call for the current thread.
        if unsafe { pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, 0) } == 0 {
            Promotion::new(Level::Elevated, "QoS user-interactive")
        } else {
            Promotion::new(Level::Normal, "QoS refused")
        }
    }

    pub(super) fn current() -> Level {
        let mut policy = TimeConstraint {
            period: 0,
            computation: 0,
            constraint: 0,
            preemptible: 0,
        };
        let mut count = THREAD_TIME_CONSTRAINT_POLICY_COUNT;
        let mut default = 0i32;
        // SAFETY: as in `realtime`.
        let rc = unsafe {
            let thread = mach_thread_self();
            let rc = thread_policy_get(
                thread,
                THREAD_TIME_CONSTRAINT_POLICY,
                std::ptr::from_mut(&mut policy).cast(),
                &mut count,
                &mut default,
            );
            mach_port_deallocate(mach_task_self_, thread);
            rc
        };
        if rc == 0 && default == 0 {
            Level::Realtime
        } else {
            Level::Normal
        }
    }
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    windows,
    target_os = "macos",
    target_os = "ios"
)))]
mod imp {
    use super::{Level, Promotion};
    use std::time::Duration;

    pub(super) fn realtime(_period: Duration) -> Promotion {
        Promotion::new(Level::Normal, "no real-time scheduling on this platform")
    }

    pub(super) fn elevated() -> Promotion {
        Promotion::new(Level::Normal, "no priority control on this platform")
    }

    pub(super) const fn current() -> Level {
        Level::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn promotion_reports_what_the_os_did() {
        // Whatever this machine allows, the claim must match the
        // scheduler's own answer, read back independently.
        let h = std::thread::spawn(|| {
            let p = promote_audio_thread(Duration::from_millis(3));
            (p.level, current_level(), p.detail.clone())
        });
        let (claimed, actual, detail) = h.join().expect("thread");
        assert_eq!(
            claimed, actual,
            "claimed {claimed:?} ({detail}), OS says {actual:?}"
        );
        assert!(!detail.is_empty());
        let h = std::thread::spawn(|| {
            let p = promote_helper_thread();
            (p.level, current_level())
        });
        let (claimed, actual) = h.join().expect("thread");
        assert_eq!(claimed, actual);
        assert_ne!(claimed, Level::Realtime, "a helper is never real-time");
    }

    #[test]
    fn levels_round_trip_through_bytes() {
        for l in [Level::Normal, Level::Elevated, Level::Realtime] {
            assert_eq!(Level::from_u8(l.as_u8()), l);
        }
        assert!(Level::Realtime > Level::Elevated && Level::Elevated > Level::Normal);
    }
}
