//! Loading a platform library at run time (`dlopen`, `LoadLibraryW`).

#[cfg(unix)]
use std::ffi::c_int;
#[cfg(unix)]
use std::ffi::CString;
use std::ffi::{c_char, c_void, CStr};

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
}

#[cfg(unix)]
unsafe extern "C" {
    fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
    fn dlerror() -> *const c_char;
}

#[cfg(unix)]
const RTLD_NOW: c_int = 2;

/// A loaded shared library.
#[derive(Debug)]
pub(crate) struct Library(*mut c_void);

// SAFETY: a library handle may be used from any thread (dlsym/dlclose are
// thread-safe).
unsafe impl Send for Library {}
// SAFETY: as above.
unsafe impl Sync for Library {}

impl Library {
    /// The first of `names` that loads.
    pub(crate) fn open(names: &[&str]) -> Result<Self, String> {
        let mut last = String::new();
        for n in names {
            #[cfg(unix)]
            {
                let c = CString::new(*n).map_err(|e| e.to_string())?;
                // SAFETY: a valid C string; loading a system library.
                let h = unsafe { dlopen(c.as_ptr(), RTLD_NOW) };
                if !h.is_null() {
                    return Ok(Self(h));
                }
                // SAFETY: dlerror returns a thread-local string or null.
                last = unsafe {
                    let e = dlerror();
                    if e.is_null() {
                        format!("{n}: not found")
                    } else {
                        CStr::from_ptr(e).to_string_lossy().into_owned()
                    }
                };
            }
            #[cfg(windows)]
            {
                let wide: Vec<u16> = n.encode_utf16().chain(Some(0)).collect();
                // SAFETY: a valid wide string; loading a system library.
                let h = unsafe { LoadLibraryW(wide.as_ptr()) };
                if !h.is_null() {
                    return Ok(Self(h));
                }
                last = format!("{n}: {}", std::io::Error::last_os_error());
            }
        }
        Err(last)
    }

    /// A symbol as a function pointer of type `T`.
    ///
    /// # Safety
    /// `T` must be the symbol's true C type (a function pointer).
    pub(crate) unsafe fn get<T: Copy>(&self, name: &CStr) -> Result<T, String> {
        // SAFETY: a live handle and C string.
        #[cfg(unix)]
        let p = unsafe { dlsym(self.0, name.as_ptr()) };
        // SAFETY: as above.
        #[cfg(windows)]
        let p = unsafe { GetProcAddress(self.0, name.as_ptr()) };
        if p.is_null() {
            return Err(format!("missing symbol {}", name.to_string_lossy()));
        }
        debug_assert_eq!(size_of::<T>(), size_of::<*mut c_void>());
        // SAFETY: the caller guarantees `T` is the symbol's pointer type.
        Ok(unsafe { std::mem::transmute_copy::<*mut c_void, T>(&p) })
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        // SAFETY: a handle from dlopen/LoadLibraryW, closed once.
        #[cfg(unix)]
        unsafe {
            dlclose(self.0);
        }
        // SAFETY: as above.
        #[cfg(windows)]
        unsafe {
            FreeLibrary(self.0);
        }
    }
}
