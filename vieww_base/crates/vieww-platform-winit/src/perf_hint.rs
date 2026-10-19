//! Telling Android how long a frame should take, so it schedules the UI thread
//! to make it (ADPF, `APerformanceHint`, Android 13+).
//!
//! # Why this exists
//!
//! Device Farm (2026-10-17): the CPU rasterizer drew the same frames in
//! 31.0 ms presented to the window and 20.7 ms presented through the Vulkan
//! swapchain on the LG Stylo 5, and 5.3 against 2.3 ms on the Galaxy S26
//! Ultra. Keeping the GPU busy changed nothing, which leaves where the thread
//! runs and at what clock. A Vulkan swapchain is a frame producer the system
//! recognises and boosts; a CPU-drawn window is not.
//!
//! `APerformanceHint` is the supported way for an app to say "this thread has
//! a frame budget": a session over the UI thread, given the target, fed each
//! frame's actual work. The system then picks cores and clocks to meet it.
//!
//! # What it is not
//!
//! Not on by default yet: `VIEWW_PERF_HINT=1` turns it on, and Device Farm runs
//! it as its own diagnostic pass beside the plain CPU path. Below Android 13
//! the functions do not exist — the LG Stylo 5 is Android 9 — and they are
//! looked up at run time rather than linked, because a link against a symbol
//! the phone's `libandroid.so` does not have stops the library from loading at
//! all.

use std::sync::Mutex;
use std::time::Duration;

static STATUS: Mutex<Option<String>> = Mutex::new(None);

fn set_status(text: String) {
    log::info!("vieww: performance hint {text}");
    if let Ok(mut status) = STATUS.lock() {
        *status = Some(text);
    }
}

/// What `VIEWW_PERF_HINT` did in this process — `None` when it was not asked
/// for (or off Android).
#[must_use]
pub fn perf_hint_status() -> Option<String> {
    STATUS.lock().ok()?.clone()
}

/// Whether the application asked for a performance-hint session.
pub(crate) fn requested() -> bool {
    std::env::var_os("VIEWW_PERF_HINT").is_some_and(|v| v == "1")
}

#[cfg(target_os = "android")]
mod imp {
    use std::ffi::c_void;
    use std::time::Duration;

    type GetManager = unsafe extern "C" fn() -> *mut c_void;
    type CreateSession = unsafe extern "C" fn(*mut c_void, *const i32, usize, i64) -> *mut c_void;
    type SetDuration = unsafe extern "C" fn(*mut c_void, i64) -> libc::c_int;
    type CloseSession = unsafe extern "C" fn(*mut c_void);

    /// One session over the calling thread.
    pub(crate) struct PerfHint {
        session: *mut c_void,
        report: SetDuration,
        update: SetDuration,
        close: CloseSession,
        target: i64,
        reported: u64,
    }

    impl std::fmt::Debug for PerfHint {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("PerfHint")
                .field("target_ns", &self.target)
                .field("reported", &self.reported)
                .finish_non_exhaustive()
        }
    }

    fn nanos(duration: Duration) -> i64 {
        i64::try_from(duration.as_nanos())
            .unwrap_or(i64::MAX)
            .max(1)
    }

    /// Look a function up in `libandroid.so`.
    ///
    /// # Safety
    ///
    /// `T` must be the function's real signature.
    unsafe fn symbol<T: Copy>(library: *mut c_void, name: &std::ffi::CStr) -> Option<T> {
        // SAFETY: `library` is a live handle; `name` is NUL-terminated.
        let address = unsafe { libc::dlsym(library, name.as_ptr()) };
        if address.is_null() {
            return None;
        }
        // SAFETY: the caller vouches for `T`; a data pointer and a function
        // pointer have the same size on every Android ABI.
        Some(unsafe { std::mem::transmute_copy::<*mut c_void, T>(&address) })
    }

    impl PerfHint {
        /// A session over this thread with `target` as its frame budget, or
        /// why there is none.
        pub(crate) fn new(target: Duration) -> Result<Self, String> {
            // SAFETY: `dlopen` of a system library by name; the handle is
            // never closed, so symbols taken from it stay valid.
            let library = unsafe { libc::dlopen(c"libandroid.so".as_ptr(), libc::RTLD_NOW) };
            if library.is_null() {
                return Err("unavailable (libandroid.so did not open)".to_string());
            }
            // SAFETY: the NDK's signatures for these five functions
            // (android/performance_hint.h).
            let functions = unsafe {
                (
                    symbol::<GetManager>(library, c"APerformanceHint_getManager"),
                    symbol::<CreateSession>(library, c"APerformanceHint_createSession"),
                    symbol::<SetDuration>(library, c"APerformanceHint_reportActualWorkDuration"),
                    symbol::<SetDuration>(library, c"APerformanceHint_updateTargetWorkDuration"),
                    symbol::<CloseSession>(library, c"APerformanceHint_closeSession"),
                )
            };
            let (Some(get_manager), Some(create), Some(report), Some(update), Some(close)) =
                functions
            else {
                return Err("unavailable (Android 12 or older: no APerformanceHint)".to_string());
            };
            let target = nanos(target);
            // SAFETY: the manager is a process singleton; the thread id array
            // outlives the call.
            let session = unsafe {
                let manager = get_manager();
                if manager.is_null() {
                    return Err("unavailable (no performance hint manager)".to_string());
                }
                let thread = libc::gettid();
                create(manager, &thread, 1, target)
            };
            if session.is_null() {
                return Err("unavailable (the system declined a session)".to_string());
            }
            Ok(Self {
                session,
                report,
                update,
                close,
                target,
                reported: 0,
            })
        }

        /// One frame's actual work, against `budget` (updated when the
        /// refresh rate changed it).
        pub(crate) fn frame(&mut self, work: Duration, budget: Duration) {
            let budget = nanos(budget);
            // SAFETY: `session` is live until `drop`.
            unsafe {
                if budget != self.target {
                    self.target = budget;
                    (self.update)(self.session, budget);
                }
                (self.report)(self.session, nanos(work));
            }
            self.reported += 1;
        }
    }

    impl Drop for PerfHint {
        fn drop(&mut self) {
            // SAFETY: closed exactly once, here.
            unsafe { (self.close)(self.session) };
        }
    }
}

#[cfg(not(target_os = "android"))]
mod imp {
    use std::time::Duration;

    /// Nothing outside Android.
    #[derive(Debug)]
    pub(crate) struct PerfHint;

    impl PerfHint {
        #[allow(clippy::unnecessary_wraps)]
        pub(crate) fn new(_target: Duration) -> Result<Self, String> {
            Err("unavailable (Android only)".to_string())
        }

        pub(crate) fn frame(&mut self, _work: Duration, _budget: Duration) {}
    }
}

pub(crate) use imp::PerfHint;

/// A session for the calling thread when one was asked for, recording why
/// not otherwise.
pub(crate) fn start(target: Duration) -> Option<PerfHint> {
    if !requested() {
        return None;
    }
    match PerfHint::new(target) {
        Ok(hint) => {
            set_status(format!(
                "on (target {:.2} ms)",
                target.as_secs_f64() * 1000.0
            ));
            Some(hint)
        }
        Err(why) => {
            set_status(why);
            None
        }
    }
}
