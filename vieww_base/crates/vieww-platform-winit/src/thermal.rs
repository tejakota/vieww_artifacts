//! How hot the device is running, as the platform reports it — for a run long
//! enough to warm a phone up.
//!
//! # Why
//!
//! Every Device Farm pass used to last ten seconds. A phone holds its clocks
//! for that long and throttles later, so a frame time measured cold says
//! nothing about the minute after: the cross-framework record (Lynx's 4.2 °C
//! lower peak under sustained GPU load, against React Native) is about
//! exactly that minute. The sustained pass (`debug.vieww.frames`) runs long
//! enough to reach it, and this says how warm the phone got meanwhile.
//!
//! # What is read
//!
//! - **Android 11+ (API 30):** `AThermal_getCurrentThermalStatus` — the
//!   system's own throttling verdict, `none` to `shutdown`.
//! - **Android 12+ (API 31):** `AThermal_getThermalHeadroom` — how close the
//!   device is to severe throttling, `1.0` being there. The system answers at
//!   most once a second; asked sooner it returns NaN, which reads as `None`.
//! - **Linux (and Android, where readable):** the hottest
//!   `/sys/class/thermal/thermal_zone*/temp`, in °C.
//!
//! The NDK functions are looked up at run time, as `perf_hint` does: linked
//! directly, a symbol the phone's `libandroid.so` lacks stops the library
//! loading at all (the LG Stylo 5 is Android 9).

/// One reading. Every field is `None` where the platform does not say.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ThermalReading {
    /// `AThermalStatus`: 0 none, 1 light, 2 moderate, 3 severe, 4 critical,
    /// 5 emergency, 6 shutdown.
    pub status: Option<i32>,
    /// `AThermal_getThermalHeadroom(0)`: 1.0 is severe throttling.
    pub headroom: Option<f32>,
    /// The hottest thermal zone the process may read, in °C.
    pub hottest_celsius: Option<f32>,
}

impl ThermalReading {
    /// Whether the reading says anything at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.status.is_none() && self.headroom.is_none() && self.hottest_celsius.is_none()
    }
}

/// The name Android gives a thermal status.
#[must_use]
pub fn status_name(status: i32) -> &'static str {
    match status {
        0 => "none",
        1 => "light",
        2 => "moderate",
        3 => "severe",
        4 => "critical",
        5 => "emergency",
        6 => "shutdown",
        _ => "unknown",
    }
}

/// How hot the device is now. Cheap enough to call once a second; the
/// headroom is `None` if asked more often than the system allows.
#[must_use]
pub fn thermal_reading() -> ThermalReading {
    let (status, headroom) = imp::platform();
    ThermalReading {
        status,
        headroom,
        hottest_celsius: hottest_zone(),
    }
}

/// The hottest readable `/sys/class/thermal/thermal_zone*/temp`, in °C.
/// Zones report millidegrees; a value outside -40..150 °C is a zone that is
/// not a temperature (some report a state or a raw ADC count) and is skipped.
fn hottest_zone() -> Option<f32> {
    let entries = std::fs::read_dir("/sys/class/thermal").ok()?;
    let mut hottest: Option<f32> = None;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with("thermal_zone") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path().join("temp")) else {
            continue;
        };
        let Ok(milli) = text.trim().parse::<f32>() else {
            continue;
        };
        let celsius = milli / 1000.0;
        if (-40.0..150.0).contains(&celsius) {
            hottest = Some(hottest.map_or(celsius, |h: f32| h.max(celsius)));
        }
    }
    hottest
}

#[cfg(target_os = "android")]
mod imp {
    use std::ffi::c_void;
    use std::sync::OnceLock;

    type Acquire = unsafe extern "C" fn() -> *mut c_void;
    type Status = unsafe extern "C" fn(*mut c_void) -> i32;
    type Headroom = unsafe extern "C" fn(*mut c_void, i32) -> f32;

    struct Api {
        manager: usize,
        status: Status,
        headroom: Option<Headroom>,
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

    fn api() -> Option<&'static Api> {
        static API: OnceLock<Option<Api>> = OnceLock::new();
        API.get_or_init(|| {
            // SAFETY: `dlopen` of a system library by name; never closed, so
            // the symbols stay valid for the process.
            let library = unsafe { libc::dlopen(c"libandroid.so".as_ptr(), libc::RTLD_NOW) };
            if library.is_null() {
                return None;
            }
            // SAFETY: the NDK's signatures (android/thermal.h).
            let (acquire, status, headroom) = unsafe {
                (
                    symbol::<Acquire>(library, c"AThermal_acquireManager")?,
                    symbol::<Status>(library, c"AThermal_getCurrentThermalStatus")?,
                    symbol::<Headroom>(library, c"AThermal_getThermalHeadroom"),
                )
            };
            // SAFETY: returns a manager owned by the caller; kept for the
            // process's life, as the NDK allows.
            let manager = unsafe { acquire() };
            (!manager.is_null()).then_some(Api {
                manager: manager as usize,
                status,
                headroom,
            })
        })
        .as_ref()
    }

    pub(super) fn platform() -> (Option<i32>, Option<f32>) {
        let Some(api) = api() else {
            return (None, None);
        };
        let manager = api.manager as *mut c_void;
        // SAFETY: a live manager, and the NDK's signatures.
        let status = unsafe { (api.status)(manager) };
        let headroom = api
            .headroom
            // SAFETY: as above; 0 asks about now rather than a forecast.
            .map(|headroom| unsafe { headroom(manager, 0) })
            .filter(|h| h.is_finite());
        ((status >= 0).then_some(status), headroom)
    }
}

#[cfg(not(target_os = "android"))]
mod imp {
    pub(super) const fn platform() -> (Option<i32>, Option<f32>) {
        (None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_have_android_names() {
        assert_eq!(status_name(0), "none");
        assert_eq!(status_name(3), "severe");
        assert_eq!(status_name(6), "shutdown");
        assert_eq!(status_name(42), "unknown");
    }

    #[test]
    fn a_reading_never_panics_and_says_whether_it_said_anything() {
        let reading = thermal_reading();
        assert_eq!(
            reading.is_empty(),
            reading.status.is_none()
                && reading.headroom.is_none()
                && reading.hottest_celsius.is_none()
        );
    }
}
