//! A VST3 host: load a module, list its classes, instantiate and drive
//! them.
//!
//! [`Module::open`] loads a `.vst3` (a bundle directory — the binary for
//! this OS and architecture is found inside — or the binary itself), runs
//! the module entry (`ModuleEntry` on Linux, `bundleEntry` with a real
//! `CFBundleRef` on macOS, `InitDll` on Windows) and takes its factory. An
//! [`Instance`] owns one component: it connects the edit controller —
//! the component itself for single-component plugins, or a separately
//! created controller, synchronised with the component's state, for split
//! ones — and provides the host side of every interface a plugin calls
//! back into: `IHostApplication`, `IBStream`, `IParameterChanges` /
//! `IParamValueQueue`, `IEventList` and `IComponentHandler`.

#![allow(
    non_upper_case_globals,
    reason = "VST3 constants keep their C names in patterns"
)]

#[cfg(unix)]
use std::ffi::CString;
use std::ffi::{c_char, c_void};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};

use super::sys::*;
use crate::vcall;

/// Why a host operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    /// The library could not be loaded.
    Load(String),
    /// No `GetPluginFactory`, or it returned nothing.
    NoFactory,
    /// The module entry refused.
    Entry,
    /// No such class, or creation/initialisation failed.
    Create(String),
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Load(e) => write!(f, "could not load the module: {e}"),
            Self::NoFactory => f.write_str("the module has no plugin factory"),
            Self::Entry => f.write_str("the module's entry function failed"),
            Self::Create(e) => write!(f, "could not create the component: {e}"),
        }
    }
}

impl std::error::Error for HostError {}

/// A class a module offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassEntry {
    /// Class id.
    pub cid: TUID,
    /// Category (`"Audio Module Class"` for processors).
    pub category: String,
    /// Name.
    pub name: String,
    /// Subcategories (`"Fx|EQ"`).
    pub subcategories: String,
    /// Vendor.
    pub vendor: String,
    /// Version.
    pub version: String,
    /// SDK version it was built with.
    pub sdk_version: String,
}

fn c_string(b: &[c_char]) -> String {
    let bytes: Vec<u8> = b
        .iter()
        .take_while(|c| **c != 0)
        .map(|c| {
            // `c_char` is signed on some targets and unsigned on others.
            #[allow(clippy::cast_sign_loss, clippy::unnecessary_cast)]
            let b = *c as u8;
            b
        })
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(unix)]
mod dl {
    use std::ffi::{c_char, c_int, c_void};
    unsafe extern "C" {
        pub(super) fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
        pub(super) fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        pub(super) fn dlclose(handle: *mut c_void) -> c_int;
        pub(super) fn dlerror() -> *const c_char;
    }
}

#[cfg(windows)]
mod dl {
    use std::ffi::{c_char, c_void};
    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub(super) fn LoadLibraryW(name: *const u16) -> *mut c_void;
        pub(super) fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
        pub(super) fn FreeLibrary(module: *mut c_void) -> i32;
    }
}

#[cfg(target_os = "macos")]
mod cf {
    use std::ffi::c_void;
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        pub(super) fn CFURLCreateFromFileSystemRepresentation(
            alloc: *const c_void,
            buf: *const u8,
            len: isize,
            is_dir: u8,
        ) -> *const c_void;
        pub(super) fn CFBundleCreate(alloc: *const c_void, url: *const c_void) -> *mut c_void;
        pub(super) fn CFRelease(cf: *const c_void);
    }
}

/// The binary inside a `.vst3` bundle for this platform.
#[must_use]
pub fn bundle_binary(bundle: &Path) -> PathBuf {
    let stem = bundle
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let contents = bundle.join("Contents");
    if cfg!(target_os = "macos") {
        contents.join("MacOS").join(stem)
    } else if cfg!(windows) {
        let arch = if cfg!(target_arch = "aarch64") {
            "arm64-win"
        } else {
            "x86_64-win"
        };
        contents.join(arch).join(format!("{stem}.vst3"))
    } else {
        let arch = if cfg!(target_arch = "aarch64") {
            "aarch64-linux"
        } else {
            "x86_64-linux"
        };
        contents.join(arch).join(format!("{stem}.so"))
    }
}

/// A loaded VST3 module.
#[derive(Debug)]
pub struct Module {
    handle: *mut c_void,
    factory: *mut c_void,
    exit: Option<unsafe extern "C" fn() -> bool>,
    bundle_ref: *mut c_void,
    host: Box<HostApplication>,
}

type FactoryFn = unsafe extern "system" fn() -> *mut c_void;
type EntryFn = unsafe extern "C" fn(*mut c_void) -> bool;
type ExitFn = unsafe extern "C" fn() -> bool;

impl Module {
    /// Load the module at `path` (a `.vst3` bundle or binary).
    ///
    /// # Errors
    /// Loading, the entry function, or a missing factory.
    ///
    /// # Safety
    /// Loading runs the library's code: it must be trusted.
    pub unsafe fn open(path: impl AsRef<Path>) -> Result<Self, HostError> {
        let bundle = path.as_ref().to_path_buf();
        let binary = if bundle.is_dir() {
            bundle_binary(&bundle)
        } else {
            bundle.clone()
        };
        // SAFETY: the caller trusts the library.
        let handle = unsafe { load(&binary)? };
        // SAFETY: symbol lookups on a live handle; the types are the
        // VST3 module ABI's.
        unsafe {
            let sym = |name: &std::ffi::CStr| symbol(handle, name);
            #[allow(unused_mut, reason = "assigned on macOS")]
            let mut bundle_ref: *mut c_void = null_mut();
            let entry = sym(c"ModuleEntry");
            let exit = sym(c"ModuleExit");
            let (entry, exit) = if entry.is_null() {
                (sym(c"bundleEntry"), sym(c"bundleExit"))
            } else {
                (entry, exit)
            };
            if !entry.is_null() {
                #[cfg(target_os = "macos")]
                {
                    let p = bundle.to_string_lossy();
                    let url = cf::CFURLCreateFromFileSystemRepresentation(
                        null(),
                        p.as_ptr(),
                        isize::try_from(p.len()).unwrap_or(0),
                        1,
                    );
                    if !url.is_null() {
                        bundle_ref = cf::CFBundleCreate(null(), url);
                        cf::CFRelease(url);
                    }
                }
                let arg = if bundle_ref.is_null() {
                    handle
                } else {
                    bundle_ref
                };
                let f: EntryFn = std::mem::transmute(entry);
                if !f(arg) {
                    unload(handle);
                    return Err(HostError::Entry);
                }
            }
            let init = sym(c"InitDll");
            if !init.is_null() {
                let f: ExitFn = std::mem::transmute(init);
                f();
            }
            let gf = sym(c"GetPluginFactory");
            if gf.is_null() {
                unload(handle);
                return Err(HostError::NoFactory);
            }
            let gf: FactoryFn = std::mem::transmute(gf);
            let factory = gf();
            if factory.is_null() {
                unload(handle);
                return Err(HostError::NoFactory);
            }
            let exit = if exit.is_null() {
                sym(c"ExitDll")
            } else {
                exit
            };
            let mut m = Self {
                handle,
                factory,
                exit: (!exit.is_null()).then(|| std::mem::transmute::<*mut c_void, ExitFn>(exit)),
                bundle_ref,
                host: HostApplication::new(),
            };
            m.set_host_context();
            Ok(m)
        }
    }

    /// Wrap a factory already in this process.
    ///
    /// # Errors
    /// A null factory.
    ///
    /// # Safety
    /// `factory` is a valid `IPluginFactory` that outlives the module.
    pub unsafe fn from_factory(factory: *mut c_void, _name: &str) -> Result<Self, HostError> {
        if factory.is_null() {
            return Err(HostError::NoFactory);
        }
        let mut m = Self {
            handle: null_mut(),
            factory,
            exit: None,
            bundle_ref: null_mut(),
            host: HostApplication::new(),
        };
        m.set_host_context();
        Ok(m)
    }

    fn factory3(&self) -> *mut c_void {
        let mut p = null_mut();
        // SAFETY: a valid factory; QI with a valid out pointer.
        let r = unsafe {
            vcall!(
                self.factory,
                FUnknownVtbl,
                queryInterface,
                &IPluginFactory3_iid,
                &raw mut p
            )
        };
        if r == kResultOk {
            p
        } else {
            null_mut()
        }
    }

    fn factory2(&self) -> *mut c_void {
        let mut p = null_mut();
        // SAFETY: as above.
        let r = unsafe {
            vcall!(
                self.factory,
                FUnknownVtbl,
                queryInterface,
                &IPluginFactory2_iid,
                &raw mut p
            )
        };
        if r == kResultOk {
            p
        } else {
            null_mut()
        }
    }

    fn set_host_context(&mut self) {
        let f3 = self.factory3();
        if !f3.is_null() {
            // SAFETY: a valid IPluginFactory3 and our host object.
            unsafe {
                vcall!(
                    f3,
                    IPluginFactory3Vtbl,
                    setHostContext,
                    self.host.as_unknown()
                );
                vcall!(f3, FUnknownVtbl, release);
            }
        }
    }

    /// `(vendor, flags)` from the factory info.
    #[must_use]
    pub fn factory_info(&self) -> (String, i32) {
        // SAFETY: a zeroed POD out-struct and a valid factory.
        unsafe {
            let mut info: PFactoryInfo = std::mem::zeroed();
            vcall!(
                self.factory,
                IPluginFactory3Vtbl,
                getFactoryInfo,
                &raw mut info
            );
            (c_string(&info.vendor), info.flags)
        }
    }

    /// The classes on offer (with the richest info the factory provides).
    #[must_use]
    pub fn classes(&self) -> Vec<ClassEntry> {
        // SAFETY: a valid factory; zeroed POD out-structs.
        unsafe {
            let n = vcall!(self.factory, IPluginFactory3Vtbl, countClasses);
            let f2 = self.factory2();
            let out = (0..n)
                .filter_map(|i| {
                    if f2.is_null() {
                        let mut c: PClassInfo = std::mem::zeroed();
                        (vcall!(
                            self.factory,
                            IPluginFactory3Vtbl,
                            getClassInfo,
                            i,
                            &raw mut c
                        ) == kResultOk)
                            .then(|| ClassEntry {
                                cid: c.cid,
                                category: c_string(&c.category),
                                name: c_string(&c.name),
                                subcategories: String::new(),
                                vendor: String::new(),
                                version: String::new(),
                                sdk_version: String::new(),
                            })
                    } else {
                        let mut c: PClassInfo2 = std::mem::zeroed();
                        (vcall!(f2, IPluginFactory3Vtbl, getClassInfo2, i, &raw mut c) == kResultOk)
                            .then(|| ClassEntry {
                                cid: c.cid,
                                category: c_string(&c.category),
                                name: c_string(&c.name),
                                subcategories: c_string(&c.subCategories),
                                vendor: c_string(&c.vendor),
                                version: c_string(&c.version),
                                sdk_version: c_string(&c.sdkVersion),
                            })
                    }
                })
                .collect();
            if !f2.is_null() {
                vcall!(f2, FUnknownVtbl, release);
            }
            out
        }
    }

    fn create(&self, cid: &TUID, iid: &TUID) -> *mut c_void {
        let mut obj = null_mut();
        // SAFETY: a valid factory, 16-byte ids and an out pointer.
        let r = unsafe {
            vcall!(
                self.factory,
                IPluginFactory3Vtbl,
                createInstance,
                cid.as_ptr().cast(),
                iid.as_ptr().cast(),
                &raw mut obj
            )
        };
        if r == kResultOk {
            obj
        } else {
            null_mut()
        }
    }

    /// Create, initialise and connect a component by class id.
    ///
    /// # Errors
    /// No such class, or it failed to initialise.
    pub fn instantiate(&self, cid: &TUID) -> Result<Instance<'_>, HostError> {
        let component = self.create(cid, &IComponent_iid);
        if component.is_null() {
            return Err(HostError::Create("no such class".into()));
        }
        let ctx = self.host.as_unknown();
        // SAFETY: a fresh component and a valid host context.
        unsafe {
            if vcall!(component, IComponentVtbl, initialize, ctx) != kResultOk {
                vcall!(component, FUnknownVtbl, release);
                return Err(HostError::Create("initialize failed".into()));
            }
        }
        let query = |obj: *mut c_void, iid: &TUID| -> *mut c_void {
            let mut p = null_mut();
            // SAFETY: a live object and an out pointer.
            let r = unsafe { vcall!(obj, FUnknownVtbl, queryInterface, iid, &raw mut p) };
            if r == kResultOk {
                p
            } else {
                null_mut()
            }
        };
        let processor = query(component, &IAudioProcessor_iid);
        if processor.is_null() {
            // SAFETY: tear down what we made.
            unsafe {
                vcall!(component, IComponentVtbl, terminate);
                vcall!(component, FUnknownVtbl, release);
            }
            return Err(HostError::Create("not an audio processor".into()));
        }
        let mut controller = query(component, &IEditController_iid);
        let mut split = false;
        if controller.is_null() {
            let mut ccid: TUID = [0; 16];
            // SAFETY: a valid component and out buffer.
            if unsafe {
                vcall!(
                    component,
                    IComponentVtbl,
                    getControllerClassId,
                    &raw mut ccid
                )
            } == kResultOk
            {
                controller = self.create(&ccid, &IEditController_iid);
                if !controller.is_null() {
                    // SAFETY: a fresh controller.
                    unsafe { vcall!(controller, IEditControllerVtbl, initialize, ctx) };
                    split = true;
                }
            }
        }
        let handler = ComponentHandler::new();
        let inst = Instance {
            _module: self,
            component,
            processor,
            controller,
            split,
            handler,
            active: false,
            processing: false,
            rate: 48_000.0,
        };
        if !controller.is_null() {
            // SAFETY: a live controller and our handler object.
            unsafe {
                vcall!(
                    controller,
                    IEditControllerVtbl,
                    setComponentHandler,
                    inst.handler.as_unknown()
                )
            };
            if split {
                // Bring the controller in line with the component's state
                // and connect the two message channels.
                if let Some(state) = inst.get_state() {
                    let mut s = MemoryStream::new(state);
                    // SAFETY: a live controller and stream.
                    unsafe {
                        vcall!(
                            controller,
                            IEditControllerVtbl,
                            setComponentState,
                            s.as_ptr()
                        )
                    };
                }
                let a = query(component, &IConnectionPoint_iid);
                let b = query(controller, &IConnectionPoint_iid);
                if !a.is_null() && !b.is_null() {
                    // SAFETY: two live connection points.
                    unsafe {
                        vcall!(a, IConnectionPointVtbl, connect, b);
                        vcall!(b, IConnectionPointVtbl, connect, a);
                    }
                }
                for p in [a, b] {
                    if !p.is_null() {
                        // SAFETY: references from `query`.
                        unsafe { vcall!(p, FUnknownVtbl, release) };
                    }
                }
            }
        }
        Ok(inst)
    }
}

impl Drop for Module {
    fn drop(&mut self) {
        // SAFETY: the reverse of `open`.
        unsafe {
            if !self.handle.is_null() {
                if let Some(e) = self.exit {
                    e();
                }
                unload(self.handle);
            }
            #[cfg(target_os = "macos")]
            if !self.bundle_ref.is_null() {
                cf::CFRelease(self.bundle_ref);
            }
        }
        let _ = self.bundle_ref;
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct IConnectionPointVtbl {
    unknown: FUnknownVtbl,
    connect: unsafe extern "system" fn(this: *mut c_void, other: *mut c_void) -> tresult,
    disconnect: unsafe extern "system" fn(this: *mut c_void, other: *mut c_void) -> tresult,
    notify: unsafe extern "system" fn(this: *mut c_void, message: *mut c_void) -> tresult,
}

#[cfg(unix)]
unsafe fn load(path: &Path) -> Result<*mut c_void, HostError> {
    let p = CString::new(path.to_string_lossy().as_bytes())
        .map_err(|e| HostError::Load(e.to_string()))?;
    // SAFETY: libc with a valid C string.
    unsafe {
        let h = dl::dlopen(p.as_ptr(), 2);
        if h.is_null() {
            let e = dl::dlerror();
            let msg = if e.is_null() {
                String::new()
            } else {
                std::ffi::CStr::from_ptr(e).to_string_lossy().into_owned()
            };
            return Err(HostError::Load(msg));
        }
        Ok(h)
    }
}

#[cfg(windows)]
unsafe fn load(path: &Path) -> Result<*mut c_void, HostError> {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: kernel32 with a valid string.
    let h = unsafe { dl::LoadLibraryW(wide.as_ptr()) };
    if h.is_null() {
        return Err(HostError::Load(std::io::Error::last_os_error().to_string()));
    }
    Ok(h)
}

#[cfg(not(any(unix, windows)))]
unsafe fn load(_: &Path) -> Result<*mut c_void, HostError> {
    Err(HostError::Load("no dynamic loader on this platform".into()))
}

unsafe fn symbol(h: *mut c_void, name: &std::ffi::CStr) -> *mut c_void {
    #[cfg(unix)]
    // SAFETY: a live handle and C string.
    return unsafe { dl::dlsym(h, name.as_ptr()) };
    #[cfg(windows)]
    // SAFETY: as above.
    return unsafe { dl::GetProcAddress(h, name.as_ptr()) };
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (h, name);
        null_mut()
    }
}

unsafe fn unload(h: *mut c_void) {
    #[cfg(unix)]
    // SAFETY: a handle from dlopen.
    unsafe {
        dl::dlclose(h);
    }
    #[cfg(windows)]
    // SAFETY: a handle from LoadLibraryW.
    unsafe {
        dl::FreeLibrary(h);
    }
    #[cfg(not(any(unix, windows)))]
    let _ = h;
}

// ---- host-side COM objects ---------------------------------------------
//
// Each is a `#[repr(C)]` struct whose first field is its vtable pointer,
// owned (boxed) by the host for longer than any plugin call that sees it.
// Reference counts are kept honestly but never free: the owner does.

unsafe extern "system" fn nominal_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: every host object starts with (vtable, refcount).
    unsafe {
        let rc = &*this
            .cast::<u8>()
            .add(size_of::<*const c_void>())
            .cast::<std::sync::atomic::AtomicU32>();
        rc.fetch_add(1, std::sync::atomic::Ordering::AcqRel) + 1
    }
}

unsafe extern "system" fn nominal_release(this: *mut c_void) -> u32 {
    // SAFETY: as above.
    unsafe {
        let rc = &*this
            .cast::<u8>()
            .add(size_of::<*const c_void>())
            .cast::<std::sync::atomic::AtomicU32>();
        rc.fetch_sub(1, std::sync::atomic::Ordering::AcqRel)
            .saturating_sub(1)
    }
}

macro_rules! simple_query {
    ($name:ident, $($iid:expr),+) => {
        unsafe extern "system" fn $name(this: *mut c_void, iid: *const TUID, obj: *mut *mut c_void) -> tresult {
            if iid.is_null() || obj.is_null() {
                return kInvalidArgument;
            }
            // SAFETY: valid pointers (COM contract).
            unsafe {
                if *iid == FUnknown_iid $(|| *iid == $iid)+ {
                    nominal_add_ref(this);
                    *obj = this;
                    kResultOk
                } else {
                    *obj = null_mut();
                    kNoInterface
                }
            }
        }
    };
}

#[repr(C)]
#[derive(Debug)]
struct HostApplication {
    vtbl: *const IHostApplicationVtbl,
    refs: std::sync::atomic::AtomicU32,
}

simple_query!(host_query, IHostApplication_iid);

unsafe extern "system" fn host_name(_: *mut c_void, name: *mut TChar) -> tresult {
    if name.is_null() {
        return kInvalidArgument;
    }
    // SAFETY: a String128 buffer.
    write_tchar(
        unsafe { std::slice::from_raw_parts_mut(name, 128) },
        "vieww",
    );
    kResultOk
}

unsafe extern "system" fn host_create(
    _: *mut c_void,
    _: *mut TUID,
    _: *mut TUID,
    obj: *mut *mut c_void,
) -> tresult {
    if !obj.is_null() {
        // SAFETY: a writable out pointer.
        unsafe { *obj = null_mut() };
    }
    kResultFalse
}

static HOST_VTBL: IHostApplicationVtbl = IHostApplicationVtbl {
    unknown: FUnknownVtbl {
        queryInterface: host_query,
        addRef: nominal_add_ref,
        release: nominal_release,
    },
    getName: host_name,
    createInstance: host_create,
};

impl HostApplication {
    fn new() -> Box<Self> {
        Box::new(Self {
            vtbl: &raw const HOST_VTBL,
            refs: std::sync::atomic::AtomicU32::new(1),
        })
    }

    fn as_unknown(&self) -> *mut c_void {
        std::ptr::from_ref(self).cast_mut().cast()
    }
}

#[repr(C)]
#[derive(Debug)]
struct ComponentHandler {
    vtbl: *const IComponentHandlerVtbl,
    refs: std::sync::atomic::AtomicU32,
    restarts: std::sync::atomic::AtomicI32,
}

simple_query!(handler_query, IComponentHandler_iid);

unsafe extern "system" fn handler_edit(_: *mut c_void, _: ParamID) -> tresult {
    kResultOk
}

unsafe extern "system" fn handler_perform(_: *mut c_void, _: ParamID, _: ParamValue) -> tresult {
    kResultOk
}

unsafe extern "system" fn handler_restart(this: *mut c_void, flags: i32) -> tresult {
    // SAFETY: `this` is a ComponentHandler.
    let h = unsafe { &*this.cast::<ComponentHandler>() };
    h.restarts
        .fetch_or(flags, std::sync::atomic::Ordering::AcqRel);
    kResultOk
}

static HANDLER_VTBL: IComponentHandlerVtbl = IComponentHandlerVtbl {
    unknown: FUnknownVtbl {
        queryInterface: handler_query,
        addRef: nominal_add_ref,
        release: nominal_release,
    },
    beginEdit: handler_edit,
    performEdit: handler_perform,
    endEdit: handler_edit,
    restartComponent: handler_restart,
};

impl ComponentHandler {
    fn new() -> Box<Self> {
        Box::new(Self {
            vtbl: &raw const HANDLER_VTBL,
            refs: std::sync::atomic::AtomicU32::new(1),
            restarts: std::sync::atomic::AtomicI32::new(0),
        })
    }

    fn as_unknown(&self) -> *mut c_void {
        std::ptr::from_ref(self).cast_mut().cast()
    }
}

/// An in-memory `IBStream`.
#[repr(C)]
#[derive(Debug)]
pub struct MemoryStream {
    vtbl: *const IBStreamVtbl,
    refs: std::sync::atomic::AtomicU32,
    data: Vec<u8>,
    pos: usize,
}

simple_query!(stream_query, IBStream_iid);

unsafe extern "system" fn stream_read(
    this: *mut c_void,
    buf: *mut c_void,
    n: i32,
    read: *mut i32,
) -> tresult {
    // SAFETY: `this` is a MemoryStream; `buf` has `n` bytes.
    unsafe {
        let s = &mut *this.cast::<MemoryStream>();
        let k = (s.data.len() - s.pos).min(usize::try_from(n.max(0)).unwrap_or(0));
        std::ptr::copy_nonoverlapping(s.data[s.pos..].as_ptr(), buf.cast::<u8>(), k);
        s.pos += k;
        if !read.is_null() {
            *read = i32::try_from(k).unwrap_or(0);
        }
    }
    kResultOk
}

unsafe extern "system" fn stream_write(
    this: *mut c_void,
    buf: *mut c_void,
    n: i32,
    written: *mut i32,
) -> tresult {
    // SAFETY: as above.
    unsafe {
        let s = &mut *this.cast::<MemoryStream>();
        let k = usize::try_from(n.max(0)).unwrap_or(0);
        let src = std::slice::from_raw_parts(buf.cast::<u8>(), k);
        let end = s.pos + k;
        if s.data.len() < end {
            s.data.resize(end, 0);
        }
        s.data[s.pos..end].copy_from_slice(src);
        s.pos = end;
        if !written.is_null() {
            *written = n.max(0);
        }
    }
    kResultOk
}

unsafe extern "system" fn stream_seek(
    this: *mut c_void,
    pos: i64,
    mode: i32,
    result: *mut i64,
) -> tresult {
    // SAFETY: as above.
    unsafe {
        let s = &mut *this.cast::<MemoryStream>();
        let base = match mode {
            kIBSeekSet => 0,
            kIBSeekCur => i64::try_from(s.pos).unwrap_or(0),
            kIBSeekEnd => i64::try_from(s.data.len()).unwrap_or(0),
            _ => return kInvalidArgument,
        };
        let np = base + pos;
        if np < 0 {
            return kInvalidArgument;
        }
        s.pos = usize::try_from(np).unwrap_or(0).min(s.data.len());
        if !result.is_null() {
            *result = i64::try_from(s.pos).unwrap_or(0);
        }
    }
    kResultOk
}

unsafe extern "system" fn stream_tell(this: *mut c_void, pos: *mut i64) -> tresult {
    // SAFETY: as above.
    unsafe {
        if !pos.is_null() {
            *pos = i64::try_from((*this.cast::<MemoryStream>()).pos).unwrap_or(0);
        }
    }
    kResultOk
}

static STREAM_VTBL: IBStreamVtbl = IBStreamVtbl {
    unknown: FUnknownVtbl {
        queryInterface: stream_query,
        addRef: nominal_add_ref,
        release: nominal_release,
    },
    read: stream_read,
    write: stream_write,
    seek: stream_seek,
    tell: stream_tell,
};

impl MemoryStream {
    /// A stream over `data`, positioned at the start.
    #[must_use]
    pub fn new(data: Vec<u8>) -> Box<Self> {
        Box::new(Self {
            vtbl: &raw const STREAM_VTBL,
            refs: std::sync::atomic::AtomicU32::new(1),
            data,
            pos: 0,
        })
    }

    /// The interface pointer.
    #[must_use]
    pub fn as_ptr(&mut self) -> *mut c_void {
        std::ptr::from_mut(self).cast()
    }

    /// Take the bytes written (the stream is left empty).
    pub fn take_data(&mut self) -> Vec<u8> {
        self.pos = 0;
        std::mem::take(&mut self.data)
    }
}

#[repr(C)]
#[derive(Debug)]
struct ParamQueue {
    vtbl: *const IParamValueQueueVtbl,
    refs: std::sync::atomic::AtomicU32,
    id: ParamID,
    points: Vec<(i32, f64)>,
}

simple_query!(queue_query, IParamValueQueue_iid);

unsafe extern "system" fn queue_id(this: *mut c_void) -> ParamID {
    // SAFETY: `this` is a ParamQueue.
    unsafe { (*this.cast::<ParamQueue>()).id }
}

unsafe extern "system" fn queue_count(this: *mut c_void) -> i32 {
    // SAFETY: as above.
    i32::try_from(unsafe { &(*this.cast::<ParamQueue>()).points }.len()).unwrap_or(0)
}

unsafe extern "system" fn queue_get(
    this: *mut c_void,
    i: i32,
    offset: *mut i32,
    value: *mut f64,
) -> tresult {
    // SAFETY: as above, with writable outs.
    unsafe {
        let q = &*this.cast::<ParamQueue>();
        match usize::try_from(i).ok().and_then(|i| q.points.get(i)) {
            Some(&(o, v)) if !offset.is_null() && !value.is_null() => {
                *offset = o;
                *value = v;
                kResultOk
            }
            _ => kInvalidArgument,
        }
    }
}

unsafe extern "system" fn queue_add(_: *mut c_void, _: i32, _: f64, _: *mut i32) -> tresult {
    kResultFalse
}

static QUEUE_VTBL: IParamValueQueueVtbl = IParamValueQueueVtbl {
    unknown: FUnknownVtbl {
        queryInterface: queue_query,
        addRef: nominal_add_ref,
        release: nominal_release,
    },
    getParameterId: queue_id,
    getPointCount: queue_count,
    getPoint: queue_get,
    addPoint: queue_add,
};

#[repr(C)]
#[derive(Debug)]
struct ParamChanges {
    vtbl: *const IParameterChangesVtbl,
    refs: std::sync::atomic::AtomicU32,
    queues: Vec<ParamQueue>,
}

simple_query!(changes_query, IParameterChanges_iid);

unsafe extern "system" fn changes_count(this: *mut c_void) -> i32 {
    // SAFETY: `this` is a ParamChanges.
    i32::try_from(unsafe { &(*this.cast::<ParamChanges>()).queues }.len()).unwrap_or(0)
}

unsafe extern "system" fn changes_get(this: *mut c_void, i: i32) -> *mut c_void {
    // SAFETY: as above; the queue lives as long as the changes object.
    unsafe {
        let c = &mut *this.cast::<ParamChanges>();
        usize::try_from(i)
            .ok()
            .and_then(|i| c.queues.get_mut(i))
            .map_or(null_mut(), |q| std::ptr::from_mut(q).cast())
    }
}

unsafe extern "system" fn changes_add(
    _: *mut c_void,
    _: *const ParamID,
    _: *mut i32,
) -> *mut c_void {
    null_mut()
}

static CHANGES_VTBL: IParameterChangesVtbl = IParameterChangesVtbl {
    unknown: FUnknownVtbl {
        queryInterface: changes_query,
        addRef: nominal_add_ref,
        release: nominal_release,
    },
    getParameterCount: changes_count,
    getParameterData: changes_get,
    addParameterData: changes_add,
};

#[repr(C)]
struct EventList {
    vtbl: *const IEventListVtbl,
    refs: std::sync::atomic::AtomicU32,
    events: Vec<Event>,
}

simple_query!(events_query, IEventList_iid);

unsafe extern "system" fn events_count(this: *mut c_void) -> i32 {
    // SAFETY: `this` is an EventList.
    i32::try_from(unsafe { &(*this.cast::<EventList>()).events }.len()).unwrap_or(0)
}

unsafe extern "system" fn events_get(this: *mut c_void, i: i32, e: *mut Event) -> tresult {
    // SAFETY: as above, with a writable out event.
    unsafe {
        let l = &*this.cast::<EventList>();
        match (
            usize::try_from(i).ok().and_then(|i| l.events.get(i)),
            e.is_null(),
        ) {
            (Some(ev), false) => {
                *e = *ev;
                kResultOk
            }
            _ => kInvalidArgument,
        }
    }
}

unsafe extern "system" fn events_add(_: *mut c_void, _: *mut Event) -> tresult {
    kResultOk
}

static EVENTS_VTBL: IEventListVtbl = IEventListVtbl {
    unknown: FUnknownVtbl {
        queryInterface: events_query,
        addRef: nominal_add_ref,
        release: nominal_release,
    },
    getEventCount: events_count,
    getEvent: events_get,
    addEvent: events_add,
};

/// An event a host sends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HostEvent {
    /// Note on.
    NoteOn {
        /// Frame.
        time: i32,
        /// Channel.
        channel: i16,
        /// Key.
        pitch: i16,
        /// Velocity 0..1.
        velocity: f32,
        /// Note id.
        note_id: i32,
    },
    /// Note off.
    NoteOff {
        /// Frame.
        time: i32,
        /// Channel.
        channel: i16,
        /// Key.
        pitch: i16,
        /// Note id.
        note_id: i32,
    },
    /// A normalised parameter value.
    Param {
        /// Frame.
        time: i32,
        /// Id.
        id: ParamID,
        /// Value 0..1.
        normalized: f64,
    },
}

/// A parameter as the controller describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct HostParam {
    /// Id.
    pub id: ParamID,
    /// Title.
    pub title: String,
    /// Units.
    pub units: String,
    /// Steps (0: continuous).
    pub step_count: i32,
    /// Default, normalised.
    pub default_normalized: f64,
    /// Automatable.
    pub automatable: bool,
    /// Hidden.
    pub hidden: bool,
}

/// A hosted component.
#[derive(Debug)]
pub struct Instance<'a> {
    _module: &'a Module,
    component: *mut c_void,
    processor: *mut c_void,
    controller: *mut c_void,
    split: bool,
    handler: Box<ComponentHandler>,
    active: bool,
    processing: bool,
    rate: f64,
}

impl Instance<'_> {
    /// Whether the controller is a separate object.
    #[must_use]
    pub const fn split_controller(&self) -> bool {
        self.split
    }

    /// Channel counts of the buses of a media type and direction.
    #[must_use]
    pub fn bus_channels(&self, media: i32, dir: i32) -> Vec<i32> {
        // SAFETY: a live component and zeroed POD out-structs.
        unsafe {
            let n = vcall!(self.component, IComponentVtbl, getBusCount, media, dir);
            (0..n)
                .filter_map(|i| {
                    let mut b: BusInfo = std::mem::zeroed();
                    (vcall!(
                        self.component,
                        IComponentVtbl,
                        getBusInfo,
                        media,
                        dir,
                        i,
                        &raw mut b
                    ) == kResultOk)
                        .then_some(b.channelCount)
                })
                .collect()
        }
    }

    /// The controller's parameters.
    #[must_use]
    pub fn params(&self) -> Vec<HostParam> {
        if self.controller.is_null() {
            return Vec::new();
        }
        // SAFETY: a live controller and zeroed POD out-structs.
        unsafe {
            let n = vcall!(self.controller, IEditControllerVtbl, getParameterCount);
            (0..n)
                .filter_map(|i| {
                    let mut p: ParameterInfo = std::mem::zeroed();
                    (vcall!(
                        self.controller,
                        IEditControllerVtbl,
                        getParameterInfo,
                        i,
                        &raw mut p
                    ) == kResultOk)
                        .then(|| HostParam {
                            id: p.id,
                            title: read_tchar(&p.title),
                            units: read_tchar(&p.units),
                            step_count: p.stepCount,
                            default_normalized: p.defaultNormalizedValue,
                            automatable: p.flags & kCanAutomate != 0,
                            hidden: p.flags & kIsHidden != 0,
                        })
                })
                .collect()
        }
    }

    /// A parameter's normalised value, as the controller holds it.
    #[must_use]
    pub fn normalized(&self, id: ParamID) -> f64 {
        if self.controller.is_null() {
            return 0.0;
        }
        // SAFETY: a live controller.
        unsafe { vcall!(self.controller, IEditControllerVtbl, getParamNormalized, id) }
    }

    /// A normalised value as text.
    #[must_use]
    pub fn value_to_text(&self, id: ParamID, normalized: f64) -> Option<String> {
        if self.controller.is_null() {
            return None;
        }
        let mut buf: String128 = [0; 128];
        // SAFETY: a live controller and a String128 buffer.
        let r = unsafe {
            vcall!(
                self.controller,
                IEditControllerVtbl,
                getParamStringByValue,
                id,
                normalized,
                buf.as_mut_ptr()
            )
        };
        (r == kResultOk).then(|| read_tchar(&buf))
    }

    /// Text parsed to a normalised value.
    #[must_use]
    pub fn text_to_value(&self, id: ParamID, text: &str) -> Option<f64> {
        if self.controller.is_null() {
            return None;
        }
        let mut wide: Vec<TChar> = text.encode_utf16().chain(Some(0)).collect();
        let mut v = 0.0;
        // SAFETY: a live controller, a NUL-terminated string and an out value.
        let r = unsafe {
            vcall!(
                self.controller,
                IEditControllerVtbl,
                getParamValueByString,
                id,
                wide.as_mut_ptr(),
                &raw mut v
            )
        };
        (r == kResultOk).then_some(v)
    }

    /// The MIDI-mapped parameter for a controller, if the plugin maps it.
    #[must_use]
    pub fn midi_mapping(&self, bus: i32, channel: i16, ctrl: i16) -> Option<ParamID> {
        if self.controller.is_null() {
            return None;
        }
        let mut mm = null_mut();
        // SAFETY: a live controller; QI with an out pointer.
        unsafe {
            if vcall!(
                self.controller,
                FUnknownVtbl,
                queryInterface,
                &IMidiMapping_iid,
                &raw mut mm
            ) != kResultOk
            {
                return None;
            }
            let mut id = 0;
            let r = vcall!(
                mm,
                IMidiMappingVtbl,
                getMidiControllerAssignment,
                bus,
                channel,
                ctrl,
                &raw mut id
            );
            vcall!(mm, FUnknownVtbl, release);
            (r == kResultOk).then_some(id)
        }
    }

    /// Set up for processing, activate and start.
    pub fn setup(&mut self, rate: f64, max_block: i32) -> bool {
        let mut s = ProcessSetup {
            processMode: kRealtime,
            symbolicSampleSize: kSample32,
            maxSamplesPerBlock: max_block,
            sampleRate: rate,
        };
        self.rate = rate;
        // SAFETY: live interfaces and a valid setup.
        unsafe {
            if vcall!(
                self.processor,
                IAudioProcessorVtbl,
                setupProcessing,
                &raw mut s
            ) != kResultOk
            {
                return false;
            }
            self.active = vcall!(self.component, IComponentVtbl, setActive, 1) == kResultOk;
            if self.active {
                self.processing =
                    vcall!(self.processor, IAudioProcessorVtbl, setProcessing, 1) == kResultOk;
            }
        }
        self.active
    }

    /// Latency in samples.
    #[must_use]
    pub fn latency(&self) -> u32 {
        // SAFETY: a live processor.
        unsafe { vcall!(self.processor, IAudioProcessorVtbl, getLatencySamples) }
    }

    /// Restart flags the plugin has requested since creation.
    #[must_use]
    pub fn restart_requests(&self) -> i32 {
        self.handler
            .restarts
            .load(std::sync::atomic::Ordering::Acquire)
    }

    /// Process one block; `tempo_beats` sets a playing transport.
    pub fn process(
        &mut self,
        inputs: &[Vec<f32>],
        out_channels: usize,
        frames: i32,
        events: &[HostEvent],
        tempo_beats: Option<(f64, f64)>,
    ) -> Vec<Vec<f32>> {
        let n = usize::try_from(frames.max(0)).unwrap_or(0);
        let mut ins: Vec<Vec<f32>> = inputs.iter().map(|c| c[..n].to_vec()).collect();
        let mut outs: Vec<Vec<f32>> = vec![vec![0.0; n]; out_channels];
        let mut in_ptrs: Vec<*mut c_void> = ins.iter_mut().map(|c| c.as_mut_ptr().cast()).collect();
        let mut out_ptrs: Vec<*mut c_void> =
            outs.iter_mut().map(|c| c.as_mut_ptr().cast()).collect();
        let mut in_bus = AudioBusBuffers {
            numChannels: i32::try_from(in_ptrs.len()).unwrap_or(0),
            silenceFlags: 0,
            channelBuffers: in_ptrs.as_mut_ptr(),
        };
        let mut out_bus = AudioBusBuffers {
            numChannels: i32::try_from(out_ptrs.len()).unwrap_or(0),
            silenceFlags: 0,
            channelBuffers: out_ptrs.as_mut_ptr(),
        };
        let mut changes = ParamChanges {
            vtbl: &raw const CHANGES_VTBL,
            refs: std::sync::atomic::AtomicU32::new(1),
            queues: Vec::new(),
        };
        let mut list = EventList {
            vtbl: &raw const EVENTS_VTBL,
            refs: std::sync::atomic::AtomicU32::new(1),
            events: Vec::new(),
        };
        for e in events {
            // SAFETY: a zeroed POD event filled field by field.
            let mut ev: Event = unsafe { std::mem::zeroed() };
            ev.flags = kIsLive;
            match *e {
                HostEvent::Param {
                    time,
                    id,
                    normalized,
                } => {
                    match changes.queues.iter_mut().find(|q| q.id == id) {
                        Some(q) => q.points.push((time, normalized)),
                        None => changes.queues.push(ParamQueue {
                            vtbl: &raw const QUEUE_VTBL,
                            refs: std::sync::atomic::AtomicU32::new(1),
                            id,
                            points: vec![(time, normalized)],
                        }),
                    }
                    continue;
                }
                HostEvent::NoteOn {
                    time,
                    channel,
                    pitch,
                    velocity,
                    note_id,
                } => {
                    ev.sampleOffset = time;
                    ev.type_ = kNoteOnEvent;
                    ev.body.noteOn = NoteOnEvent {
                        channel,
                        pitch,
                        tuning: 0.0,
                        velocity,
                        length: 0,
                        noteId: note_id,
                    };
                }
                HostEvent::NoteOff {
                    time,
                    channel,
                    pitch,
                    note_id,
                } => {
                    ev.sampleOffset = time;
                    ev.type_ = kNoteOffEvent;
                    ev.body.noteOff = NoteOffEvent {
                        channel,
                        pitch,
                        velocity: 0.0,
                        noteId: note_id,
                        tuning: 0.0,
                    };
                }
            }
            list.events.push(ev);
        }
        // SAFETY: a zeroed POD context, filled below.
        let mut ctx: ProcessContext = unsafe { std::mem::zeroed() };
        ctx.sampleRate = self.rate;
        if let Some((tempo, beats)) = tempo_beats {
            ctx.state = kPlaying | kTempoValid | kProjectTimeMusicValid | kTimeSigValid;
            ctx.tempo = tempo;
            ctx.projectTimeMusic = beats;
            ctx.timeSigNumerator = 4;
            ctx.timeSigDenominator = 4;
        }
        let mut data = ProcessData {
            processMode: kRealtime,
            symbolicSampleSize: kSample32,
            numSamples: frames,
            numInputs: i32::from(!in_ptrs.is_empty()),
            numOutputs: i32::from(!out_ptrs.is_empty()),
            inputs: if in_ptrs.is_empty() {
                null_mut()
            } else {
                &raw mut in_bus
            },
            outputs: if out_ptrs.is_empty() {
                null_mut()
            } else {
                &raw mut out_bus
            },
            inputParameterChanges: (&raw mut changes).cast(),
            outputParameterChanges: null_mut(),
            inputEvents: (&raw mut list).cast(),
            outputEvents: null_mut(),
            processContext: &raw mut ctx,
        };
        // SAFETY: a live, active processor and a fully valid process block.
        unsafe { vcall!(self.processor, IAudioProcessorVtbl, process, &raw mut data) };
        // A host keeps the controller informed of automation it sent.
        if !self.controller.is_null() && self.split {
            for q in &changes.queues {
                if let Some(&(_, v)) = q.points.last() {
                    // SAFETY: a live controller.
                    unsafe {
                        vcall!(
                            self.controller,
                            IEditControllerVtbl,
                            setParamNormalized,
                            q.id,
                            v
                        )
                    };
                }
            }
        }
        let _ = null::<u8>();
        outs
    }

    /// The component's state.
    #[must_use]
    pub fn get_state(&self) -> Option<Vec<u8>> {
        let mut s = MemoryStream::new(Vec::new());
        // SAFETY: a live component and stream.
        let r = unsafe { vcall!(self.component, IComponentVtbl, getState, s.as_ptr()) };
        (r == kResultOk).then(|| s.take_data())
    }

    /// Restore a state into the component and the controller.
    pub fn set_state(&mut self, data: &[u8]) -> bool {
        let mut s = MemoryStream::new(data.to_vec());
        // SAFETY: a live component and stream.
        let ok =
            unsafe { vcall!(self.component, IComponentVtbl, setState, s.as_ptr()) } == kResultOk;
        if ok && !self.controller.is_null() {
            let mut s = MemoryStream::new(data.to_vec());
            // SAFETY: a live controller and stream.
            unsafe {
                vcall!(
                    self.controller,
                    IEditControllerVtbl,
                    setComponentState,
                    s.as_ptr()
                )
            };
        }
        ok
    }
}

impl Drop for Instance<'_> {
    fn drop(&mut self) {
        // SAFETY: the VST3 teardown order on live interfaces.
        unsafe {
            if self.processing {
                vcall!(self.processor, IAudioProcessorVtbl, setProcessing, 0);
            }
            if self.active {
                vcall!(self.component, IComponentVtbl, setActive, 0);
            }
            if !self.controller.is_null() {
                vcall!(
                    self.controller,
                    IEditControllerVtbl,
                    setComponentHandler,
                    null_mut()
                );
                if self.split {
                    vcall!(self.controller, IEditControllerVtbl, terminate);
                }
                vcall!(self.controller, FUnknownVtbl, release);
            }
            vcall!(self.processor, FUnknownVtbl, release);
            vcall!(self.component, IComponentVtbl, terminate);
            vcall!(self.component, FUnknownVtbl, release);
        }
    }
}
