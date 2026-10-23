//! A CLAP host: load a `.clap`, list its plugins, instantiate and drive
//! them.
//!
//! [`Library::open`] loads a plugin binary (`dlopen` on Unix, including the
//! `Contents/MacOS` binary of a macOS bundle; `LoadLibraryW` on Windows)
//! and finds its `clap_entry`; [`Library::from_entry`] wraps an entry
//! already in this process. An [`Instance`] owns one plugin and exposes
//! the operations a DAW performs: activation, processing with timed
//! [`HostEvent`]s and a transport, parameters (info, values, text), state
//! save/load, ports and latency.

use std::ffi::{c_char, c_void, CStr, CString};
use std::path::Path;
use std::ptr::{null, null_mut};

use super::sys::*;

/// Why a host operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    /// The library could not be loaded.
    Load(String),
    /// It has no `clap_entry`.
    NoEntry,
    /// Its CLAP version is incompatible, or `init` failed.
    Init,
    /// It has no plugin factory.
    NoFactory,
    /// No plugin with that id, or creation failed.
    Create(String),
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Load(e) => write!(f, "could not load the plugin library: {e}"),
            Self::NoEntry => f.write_str("the library exports no clap_entry"),
            Self::Init => f.write_str("the plugin library refused to initialise"),
            Self::NoFactory => f.write_str("the library has no plugin factory"),
            Self::Create(id) => write!(f, "could not create plugin {id}"),
        }
    }
}

impl std::error::Error for HostError {}

/// A plugin a library offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginEntry {
    /// Id.
    pub id: String,
    /// Name.
    pub name: String,
    /// Vendor.
    pub vendor: String,
    /// Version.
    pub version: String,
    /// Description.
    pub description: String,
    /// Feature tags.
    pub features: Vec<String>,
}

fn string(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        // SAFETY: CLAP strings are NUL-terminated.
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
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
    pub(super) const RTLD_NOW: c_int = 2;
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

/// A loaded plugin library.
#[derive(Debug)]
pub struct Library {
    handle: *mut c_void,
    entry: *const clap_plugin_entry,
    factory: *const clap_plugin_factory,
}

impl Library {
    /// Load the library at `path` (a `.clap` file, or a macOS `.clap`
    /// bundle directory) and initialise it.
    ///
    /// # Errors
    /// Loading, a missing entry, init, or a missing factory.
    ///
    /// # Safety
    /// Loading a library runs its initialisers: it must be trusted code.
    pub unsafe fn open(path: impl AsRef<Path>) -> Result<Self, HostError> {
        let mut path = path.as_ref().to_path_buf();
        if path.is_dir() {
            // A macOS bundle: Contents/MacOS/<name without extension>.
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            path = path.join("Contents").join("MacOS").join(stem);
        }
        let plugin_path = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|e| HostError::Load(e.to_string()))?;
        // SAFETY: the caller trusts the library.
        let (handle, sym) = unsafe { load(&path)? };
        if sym.is_null() {
            // SAFETY: a handle from `load`.
            unsafe { unload(handle) };
            return Err(HostError::NoEntry);
        }
        // SAFETY: `clap_entry` is a `clap_plugin_entry` by the CLAP contract.
        let r = unsafe { Self::init(handle, sym.cast(), plugin_path.as_ptr()) };
        if r.is_err() {
            // SAFETY: a handle from `load`.
            unsafe { unload(handle) };
        }
        r
    }

    /// Wrap an entry in this process (no loading, no unloading).
    ///
    /// # Errors
    /// Init or a missing factory.
    pub fn from_entry(entry: &'static clap_plugin_entry, path: &str) -> Result<Self, HostError> {
        let p = CString::new(path).unwrap_or_default();
        // SAFETY: a valid entry with a 'static lifetime.
        unsafe { Self::init(null_mut(), entry, p.as_ptr()) }
    }

    unsafe fn init(
        handle: *mut c_void,
        entry: *const clap_plugin_entry,
        path: *const c_char,
    ) -> Result<Self, HostError> {
        // SAFETY: a valid entry (the caller's contract).
        let e = unsafe { &*entry };
        if e.clap_version.major < 1 {
            return Err(HostError::Init);
        }
        let ok = e.init.is_some_and(|f| {
            // SAFETY: the entry's own init with a valid path.
            unsafe { f(path) }
        });
        if !ok {
            return Err(HostError::Init);
        }
        let factory = e.get_factory.map_or(null(), |f| {
            // SAFETY: the entry's own accessor with a valid id.
            unsafe { f(CLAP_PLUGIN_FACTORY_ID.as_ptr().cast()) }
        });
        if factory.is_null() {
            if let Some(d) = e.deinit {
                // SAFETY: paired with the successful init.
                unsafe { d() };
            }
            return Err(HostError::NoFactory);
        }
        Ok(Self {
            handle,
            entry,
            factory: factory.cast(),
        })
    }

    /// The plugins on offer.
    #[must_use]
    pub fn plugins(&self) -> Vec<PluginEntry> {
        // SAFETY: a valid factory for the library's lifetime.
        unsafe {
            let f = &*self.factory;
            let (Some(count), Some(get)) = (f.get_plugin_count, f.get_plugin_descriptor) else {
                return Vec::new();
            };
            (0..count(self.factory))
                .filter_map(|i| {
                    let d = get(self.factory, i);
                    if d.is_null() {
                        return None;
                    }
                    let d = &*d;
                    let mut features = Vec::new();
                    let mut fp = d.features;
                    while !fp.is_null() && !(*fp).is_null() {
                        features.push(string(*fp));
                        fp = fp.add(1);
                    }
                    Some(PluginEntry {
                        id: string(d.id),
                        name: string(d.name),
                        vendor: string(d.vendor),
                        version: string(d.version),
                        description: string(d.description),
                        features,
                    })
                })
                .collect()
        }
    }

    /// Create and initialise a plugin by id.
    ///
    /// # Errors
    /// No such plugin, or it failed to create or initialise.
    pub fn instantiate(&self, id: &str) -> Result<Instance<'_>, HostError> {
        let host = Box::new(HostData::new());
        let cid = CString::new(id).map_err(|_| HostError::Create(id.into()))?;
        // SAFETY: a valid factory; the host box outlives the instance.
        let plugin = unsafe {
            let f = &*self.factory;
            f.create_plugin.map_or(null(), |c| {
                c(self.factory, &raw const host.clap, cid.as_ptr())
            })
        };
        if plugin.is_null() {
            return Err(HostError::Create(id.into()));
        }
        // SAFETY: a fresh plugin.
        let ok = unsafe { (*plugin).init.is_some_and(|i| i(plugin)) };
        if !ok {
            // SAFETY: destroying a plugin that failed init is allowed.
            unsafe {
                if let Some(d) = (*plugin).destroy {
                    d(plugin);
                }
            }
            return Err(HostError::Create(id.into()));
        }
        let ext = |name: &[u8]| -> *const c_void {
            // SAFETY: a live plugin and a NUL-terminated id.
            unsafe {
                (*plugin)
                    .get_extension
                    .map_or(null(), |g| g(plugin, name.as_ptr().cast()))
            }
        };
        Ok(Instance {
            _lib: self,
            _host: host,
            plugin,
            params: ext(CLAP_EXT_PARAMS).cast(),
            state: ext(CLAP_EXT_STATE).cast(),
            audio_ports: ext(CLAP_EXT_AUDIO_PORTS).cast(),
            note_ports: ext(CLAP_EXT_NOTE_PORTS).cast(),
            latency: ext(CLAP_EXT_LATENCY).cast(),
            active: false,
            processing: false,
            steady: 0,
        })
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        // SAFETY: the entry stays valid until unloaded; deinit pairs init.
        unsafe {
            if let Some(d) = (*self.entry).deinit {
                d();
            }
            if !self.handle.is_null() {
                unload(self.handle);
            }
        }
    }
}

#[cfg(unix)]
unsafe fn load(path: &Path) -> Result<(*mut c_void, *mut c_void), HostError> {
    let p = CString::new(path.to_string_lossy().as_bytes())
        .map_err(|e| HostError::Load(e.to_string()))?;
    // SAFETY: plain libc calls with valid C strings.
    unsafe {
        let h = dl::dlopen(p.as_ptr(), dl::RTLD_NOW);
        if h.is_null() {
            return Err(HostError::Load(string(dl::dlerror())));
        }
        Ok((h, dl::dlsym(h, c"clap_entry".as_ptr())))
    }
}

#[cfg(windows)]
unsafe fn load(path: &Path) -> Result<(*mut c_void, *mut c_void), HostError> {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: kernel32 calls with valid strings.
    unsafe {
        let h = dl::LoadLibraryW(wide.as_ptr());
        if h.is_null() {
            return Err(HostError::Load(std::io::Error::last_os_error().to_string()));
        }
        Ok((h, dl::GetProcAddress(h, c"clap_entry".as_ptr())))
    }
}

#[cfg(not(any(unix, windows)))]
unsafe fn load(_: &Path) -> Result<(*mut c_void, *mut c_void), HostError> {
    Err(HostError::Load("no dynamic loader on this platform".into()))
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

#[repr(C)]
struct HostData {
    clap: clap_host,
    _strings: [CString; 4],
}

impl HostData {
    fn new() -> Self {
        let s = [c"vieww", c"vieww", c"https://vieww.dev", c"0.1.0"].map(CString::from);
        Self {
            clap: clap_host {
                clap_version: CLAP_VERSION,
                host_data: null_mut(),
                name: s[0].as_ptr(),
                vendor: s[1].as_ptr(),
                url: s[2].as_ptr(),
                version: s[3].as_ptr(),
                get_extension: Some(host_get_extension),
                request_restart: Some(host_request),
                request_process: Some(host_request),
                request_callback: Some(host_request),
            },
            _strings: s,
        }
    }
}

unsafe extern "C" fn host_get_extension(_: *const clap_host, _: *const c_char) -> *const c_void {
    null()
}

unsafe extern "C" fn host_request(_: *const clap_host) {}

/// An event a host sends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HostEvent {
    /// Note on.
    NoteOn {
        /// Frame.
        time: u32,
        /// Channel.
        channel: i16,
        /// Key.
        key: i16,
        /// Velocity 0..1.
        velocity: f64,
    },
    /// Note off.
    NoteOff {
        /// Frame.
        time: u32,
        /// Channel.
        channel: i16,
        /// Key.
        key: i16,
        /// Velocity.
        velocity: f64,
    },
    /// Parameter value (plain).
    Param {
        /// Frame.
        time: u32,
        /// Id.
        id: u32,
        /// Value.
        value: f64,
    },
    /// Raw MIDI.
    Midi {
        /// Frame.
        time: u32,
        /// Bytes.
        data: [u8; 3],
    },
}

/// The transport a host reports.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HostTransport {
    /// Playing.
    pub playing: bool,
    /// Tempo.
    pub tempo: f64,
    /// Position in quarter notes.
    pub beats: f64,
    /// Time signature.
    pub time_signature: (u16, u16),
}

/// The C events of one block, owned so the list's pointers stay valid.
#[repr(C)]
#[derive(Clone, Copy)]
union CEvent {
    note: clap_event_note,
    param: clap_event_param_value,
    midi: clap_event_midi,
}

fn header(size: usize, time: u32, type_: u16) -> clap_event_header {
    clap_event_header {
        size: u32::try_from(size).unwrap_or(0),
        time,
        space_id: CLAP_CORE_EVENT_SPACE_ID,
        type_,
        flags: CLAP_EVENT_IS_LIVE,
    }
}

fn to_c(e: &HostEvent) -> CEvent {
    match *e {
        HostEvent::NoteOn {
            time,
            channel,
            key,
            velocity,
        }
        | HostEvent::NoteOff {
            time,
            channel,
            key,
            velocity,
        } => CEvent {
            note: clap_event_note {
                header: header(
                    size_of::<clap_event_note>(),
                    time,
                    if matches!(e, HostEvent::NoteOn { .. }) {
                        CLAP_EVENT_NOTE_ON
                    } else {
                        CLAP_EVENT_NOTE_OFF
                    },
                ),
                note_id: -1,
                port_index: 0,
                channel,
                key,
                velocity,
            },
        },
        HostEvent::Param { time, id, value } => CEvent {
            param: clap_event_param_value {
                header: header(
                    size_of::<clap_event_param_value>(),
                    time,
                    CLAP_EVENT_PARAM_VALUE,
                ),
                param_id: id,
                cookie: null_mut(),
                note_id: -1,
                port_index: -1,
                channel: -1,
                key: -1,
                value,
            },
        },
        HostEvent::Midi { time, data } => CEvent {
            midi: clap_event_midi {
                header: header(size_of::<clap_event_midi>(), time, CLAP_EVENT_MIDI),
                port_index: 0,
                data,
            },
        },
    }
}

unsafe extern "C" fn list_size(l: *const clap_input_events) -> u32 {
    // SAFETY: `ctx` is the `Vec<CEvent>` set up by `with_events`.
    u32::try_from(unsafe { &*(*l).ctx.cast::<Vec<CEvent>>() }.len()).unwrap_or(0)
}

unsafe extern "C" fn list_get(l: *const clap_input_events, i: u32) -> *const clap_event_header {
    // SAFETY: as in `list_size`; every union variant starts with a header.
    let v = unsafe { &*(*l).ctx.cast::<Vec<CEvent>>() };
    v.get(i as usize)
        .map_or(null(), |e| (e as *const CEvent).cast())
}

unsafe extern "C" fn out_push(_: *const clap_output_events, _: *const clap_event_header) -> bool {
    true
}

unsafe extern "C" fn ostream_write(s: *const clap_ostream, buf: *const c_void, n: u64) -> i64 {
    // SAFETY: `ctx` is the `Vec<u8>` set up by `save`; `buf` has `n` bytes.
    unsafe {
        let v = &mut *(*s).ctx.cast::<Vec<u8>>();
        let n = usize::try_from(n).unwrap_or(0);
        v.extend_from_slice(std::slice::from_raw_parts(buf.cast::<u8>(), n));
        i64::try_from(n).unwrap_or(0)
    }
}

unsafe extern "C" fn istream_read(s: *const clap_istream, buf: *mut c_void, n: u64) -> i64 {
    // SAFETY: `ctx` is the `(slice, position)` set up by `load`.
    unsafe {
        let (data, at) = &mut *(*s).ctx.cast::<(&[u8], usize)>();
        // Hand it out in small pieces: plugins must cope with short reads.
        let k = (data.len() - *at)
            .min(usize::try_from(n).unwrap_or(0))
            .min(37);
        std::ptr::copy_nonoverlapping(data[*at..].as_ptr(), buf.cast::<u8>(), k);
        *at += k;
        i64::try_from(k).unwrap_or(0)
    }
}

/// A parameter as the plugin described it.
#[derive(Debug, Clone, PartialEq)]
pub struct HostParam {
    /// Id.
    pub id: u32,
    /// Name.
    pub name: String,
    /// Minimum.
    pub min: f64,
    /// Maximum.
    pub max: f64,
    /// Default.
    pub default: f64,
    /// Stepped.
    pub stepped: bool,
    /// An enumeration.
    pub is_enum: bool,
    /// Automatable.
    pub automatable: bool,
}

/// A hosted plugin.
#[derive(Debug)]
pub struct Instance<'a> {
    _lib: &'a Library,
    _host: Box<HostData>,
    plugin: *const clap_plugin,
    params: *const clap_plugin_params,
    state: *const clap_plugin_state,
    audio_ports: *const clap_plugin_audio_ports,
    note_ports: *const clap_plugin_note_ports,
    latency: *const clap_plugin_latency,
    active: bool,
    processing: bool,
    steady: i64,
}

impl std::fmt::Debug for HostData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HostData")
    }
}

impl Instance<'_> {
    fn with_events<R>(
        events: &[HostEvent],
        f: impl FnOnce(&clap_input_events, &clap_output_events) -> R,
    ) -> R {
        let mut v: Vec<CEvent> = events.iter().map(to_c).collect();
        let input = clap_input_events {
            ctx: (&raw mut v).cast(),
            size: Some(list_size),
            get: Some(list_get),
        };
        let output = clap_output_events {
            ctx: null_mut(),
            try_push: Some(out_push),
        };
        f(&input, &output)
    }

    /// The plugin's parameters.
    #[must_use]
    pub fn params(&self) -> Vec<HostParam> {
        if self.params.is_null() {
            return Vec::new();
        }
        // SAFETY: a valid params extension for the plugin's lifetime.
        unsafe {
            let e = &*self.params;
            let (Some(count), Some(get)) = (e.count, e.get_info) else {
                return Vec::new();
            };
            (0..count(self.plugin))
                .filter_map(|i| {
                    let mut info: clap_param_info = std::mem::zeroed();
                    get(self.plugin, i, &raw mut info).then(|| HostParam {
                        id: info.id,
                        name: string(info.name.as_ptr()),
                        min: info.min_value,
                        max: info.max_value,
                        default: info.default_value,
                        stepped: info.flags & CLAP_PARAM_IS_STEPPED != 0,
                        is_enum: info.flags & CLAP_PARAM_IS_ENUM != 0,
                        automatable: info.flags & CLAP_PARAM_IS_AUTOMATABLE != 0,
                    })
                })
                .collect()
        }
    }

    /// A parameter's current value.
    #[must_use]
    pub fn value(&self, id: u32) -> Option<f64> {
        if self.params.is_null() {
            return None;
        }
        let mut v = 0.0;
        // SAFETY: a valid extension and out pointer.
        unsafe {
            (*self.params)
                .get_value
                .is_some_and(|g| g(self.plugin, id, &raw mut v))
        }
        .then_some(v)
    }

    /// A value as the plugin displays it.
    #[must_use]
    pub fn value_to_text(&self, id: u32, value: f64) -> Option<String> {
        if self.params.is_null() {
            return None;
        }
        let mut buf = [0 as c_char; 256];
        // SAFETY: a valid extension and buffer.
        unsafe {
            (*self.params)
                .value_to_text
                .is_some_and(|f| f(self.plugin, id, value, buf.as_mut_ptr(), 256))
        }
        .then(|| string(buf.as_ptr()))
    }

    /// Text parsed by the plugin.
    #[must_use]
    pub fn text_to_value(&self, id: u32, text: &str) -> Option<f64> {
        let t = CString::new(text).ok()?;
        if self.params.is_null() {
            return None;
        }
        let mut v = 0.0;
        // SAFETY: a valid extension, C string and out pointer.
        unsafe {
            (*self.params)
                .text_to_value
                .is_some_and(|f| f(self.plugin, id, t.as_ptr(), &raw mut v))
        }
        .then_some(v)
    }

    /// Send parameter changes outside processing.
    pub fn flush(&mut self, events: &[HostEvent]) {
        if self.params.is_null() {
            return;
        }
        let (plugin, params) = (self.plugin, self.params);
        Self::with_events(events, |i, o| {
            // SAFETY: a valid extension and event lists.
            unsafe {
                if let Some(f) = (*params).flush {
                    f(plugin, i, o);
                }
            }
        });
    }

    /// Activate (and start processing).
    pub fn activate(&mut self, rate: f64, min: u32, max: u32) -> bool {
        // SAFETY: a live plugin.
        self.active = unsafe {
            (*self.plugin)
                .activate
                .is_some_and(|a| a(self.plugin, rate, min, max))
        };
        if self.active {
            // SAFETY: as above.
            self.processing = unsafe {
                (*self.plugin)
                    .start_processing
                    .is_some_and(|s| s(self.plugin))
            };
        }
        self.active
    }

    /// Process one block: planar `inputs`, `out_channels` outputs of
    /// `frames` frames. Returns the outputs and the plugin's status.
    pub fn process(
        &mut self,
        inputs: &[Vec<f32>],
        out_channels: usize,
        frames: u32,
        events: &[HostEvent],
        transport: Option<HostTransport>,
    ) -> (Vec<Vec<f32>>, i32) {
        let n = frames as usize;
        let mut ins: Vec<Vec<f32>> = inputs.iter().map(|c| c[..n].to_vec()).collect();
        let mut outs: Vec<Vec<f32>> = vec![vec![0.0; n]; out_channels];
        let mut in_ptrs: Vec<*mut f32> = ins.iter_mut().map(|c| c.as_mut_ptr()).collect();
        let mut out_ptrs: Vec<*mut f32> = outs.iter_mut().map(|c| c.as_mut_ptr()).collect();
        let in_buf = clap_audio_buffer {
            data32: in_ptrs.as_mut_ptr(),
            data64: null_mut(),
            channel_count: u32::try_from(in_ptrs.len()).unwrap_or(0),
            latency: 0,
            constant_mask: 0,
        };
        let mut out_buf = clap_audio_buffer {
            data32: out_ptrs.as_mut_ptr(),
            data64: null_mut(),
            channel_count: u32::try_from(out_ptrs.len()).unwrap_or(0),
            latency: 0,
            constant_mask: 0,
        };
        let tr = transport.map(|t| {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_precision_loss,
                reason = "fixed point"
            )]
            let fixed = |b: f64| (b * CLAP_BEATTIME_FACTOR as f64).round() as i64;
            clap_event_transport {
                header: header(size_of::<clap_event_transport>(), 0, CLAP_EVENT_TRANSPORT),
                flags: CLAP_TRANSPORT_HAS_TEMPO
                    | CLAP_TRANSPORT_HAS_BEATS_TIMELINE
                    | CLAP_TRANSPORT_HAS_TIME_SIGNATURE
                    | if t.playing {
                        CLAP_TRANSPORT_IS_PLAYING
                    } else {
                        0
                    },
                song_pos_beats: fixed(t.beats),
                song_pos_seconds: 0,
                tempo: t.tempo,
                tempo_inc: 0.0,
                loop_start_beats: 0,
                loop_end_beats: 0,
                loop_start_seconds: 0,
                loop_end_seconds: 0,
                bar_start: 0,
                bar_number: 0,
                tsig_num: t.time_signature.0,
                tsig_denom: t.time_signature.1,
            }
        });
        let plugin = self.plugin;
        let steady = self.steady;
        let status = Self::with_events(events, |i, o| {
            let p = clap_process {
                steady_time: steady,
                frames_count: frames,
                transport: tr.as_ref().map_or(null(), |t| t as *const _),
                audio_inputs: if in_ptrs.is_empty() {
                    null()
                } else {
                    &raw const in_buf
                },
                audio_outputs: if out_ptrs.is_empty() {
                    null_mut()
                } else {
                    &raw mut out_buf
                },
                audio_inputs_count: u32::from(!in_ptrs.is_empty()),
                audio_outputs_count: u32::from(!out_ptrs.is_empty()),
                in_events: i,
                out_events: o,
            };
            // SAFETY: a live, activated plugin and a fully valid process.
            unsafe {
                (*plugin)
                    .process
                    .map_or(CLAP_PROCESS_ERROR, |f| f(plugin, &raw const p))
            }
        });
        self.steady += i64::from(frames);
        crate::sanitize_outputs(&mut outs);
        (outs, status)
    }

    /// Save the plugin's state.
    #[must_use]
    pub fn save(&self) -> Option<Vec<u8>> {
        if self.state.is_null() {
            return None;
        }
        let mut data: Vec<u8> = Vec::new();
        let s = clap_ostream {
            ctx: (&raw mut data).cast(),
            write: Some(ostream_write),
        };
        // SAFETY: a valid extension and stream.
        let ok = unsafe {
            (*self.state)
                .save
                .is_some_and(|f| f(self.plugin, &raw const s))
        };
        ok.then_some(data)
    }

    /// Load a state.
    pub fn load(&mut self, data: &[u8]) -> bool {
        if self.state.is_null() {
            return false;
        }
        let mut ctx: (&[u8], usize) = (data, 0);
        let s = clap_istream {
            ctx: (&raw mut ctx).cast(),
            read: Some(istream_read),
        };
        // SAFETY: a valid extension and stream.
        unsafe {
            (*self.state)
                .load
                .is_some_and(|f| f(self.plugin, &raw const s))
        }
    }

    /// The plugin's latency in samples.
    #[must_use]
    pub fn latency(&self) -> u32 {
        if self.latency.is_null() {
            return 0;
        }
        // SAFETY: a valid extension.
        unsafe { (*self.latency).get.map_or(0, |g| g(self.plugin)) }
    }

    /// Audio ports: `(name, channels)`.
    #[must_use]
    pub fn audio_ports(&self, input: bool) -> Vec<(String, u32)> {
        if self.audio_ports.is_null() {
            return Vec::new();
        }
        // SAFETY: a valid extension.
        unsafe {
            let e = &*self.audio_ports;
            let (Some(count), Some(get)) = (e.count, e.get) else {
                return Vec::new();
            };
            (0..count(self.plugin, input))
                .filter_map(|i| {
                    let mut info: clap_audio_port_info = std::mem::zeroed();
                    get(self.plugin, i, input, &raw mut info)
                        .then(|| (string(info.name.as_ptr()), info.channel_count))
                })
                .collect()
        }
    }

    /// Number of note input ports.
    #[must_use]
    pub fn note_ports(&self) -> u32 {
        if self.note_ports.is_null() {
            return 0;
        }
        // SAFETY: a valid extension.
        unsafe { (*self.note_ports).count.map_or(0, |c| c(self.plugin, true)) }
    }

    /// Reset the plugin.
    pub fn reset(&mut self) {
        // SAFETY: a live plugin.
        unsafe {
            if let Some(r) = (*self.plugin).reset {
                r(self.plugin);
            }
        }
    }
}

impl Drop for Instance<'_> {
    fn drop(&mut self) {
        // SAFETY: the CLAP teardown order on a live plugin.
        unsafe {
            let p = &*self.plugin;
            if self.processing {
                if let Some(s) = p.stop_processing {
                    s(self.plugin);
                }
            }
            if self.active {
                if let Some(d) = p.deactivate {
                    d(self.plugin);
                }
            }
            if let Some(d) = p.destroy {
                d(self.plugin);
            }
        }
    }
}
