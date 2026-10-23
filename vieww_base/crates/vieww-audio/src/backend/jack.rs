//! JACK: the low-latency audio server of Linux studios (and of macOS and
//! Windows setups that use it), and the API PipeWire also serves.
//!
//! `libjack` is loaded at run time. A stream is a JACK client with one
//! port per channel; JACK calls the process callback on its real-time
//! thread with planar port buffers, the render callback fills an
//! interleaved scratch block (preallocated, rendered in chunks if the
//! server's period ever exceeds it — nothing allocates on that thread),
//! and the block is split into the ports. The server sets the rate and
//! period: [`DeviceStream::config`] reports them. New ports are connected
//! to the server's physical ports unless [`Jack::autoconnect`] is off.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::ptr::{null, null_mut};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use super::dl::Library;
use super::{
    isolate_capture, isolate_render, Backend, BackendError, BlockInfo, CaptureFn, Counters,
    DeviceInfo, DeviceStream, RenderFn, StreamConfig, StreamImpl,
};

const PORT_IS_INPUT: u64 = 0x1;
const PORT_IS_OUTPUT: u64 = 0x2;
const PORT_IS_PHYSICAL: u64 = 0x4;
const NO_START_SERVER: c_int = 0x01;
const AUDIO_TYPE: &CStr = c"32 bit float mono audio";
const CHUNK: usize = 8_192;

type ProcessCb = unsafe extern "C" fn(nframes: u32, arg: *mut c_void) -> c_int;
type XrunCb = unsafe extern "C" fn(arg: *mut c_void) -> c_int;
type ShutdownCb = unsafe extern "C" fn(arg: *mut c_void);

struct Api {
    _lib: Library,
    client_open: unsafe extern "C" fn(*const c_char, c_int, *mut c_int, ...) -> *mut c_void,
    client_close: unsafe extern "C" fn(*mut c_void) -> c_int,
    get_sample_rate: unsafe extern "C" fn(*mut c_void) -> u32,
    get_buffer_size: unsafe extern "C" fn(*mut c_void) -> u32,
    port_register:
        unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char, u64, u64) -> *mut c_void,
    port_get_buffer: unsafe extern "C" fn(*mut c_void, u32) -> *mut f32,
    port_name: unsafe extern "C" fn(*mut c_void) -> *const c_char,
    set_process_callback: unsafe extern "C" fn(*mut c_void, ProcessCb, *mut c_void) -> c_int,
    set_xrun_callback: unsafe extern "C" fn(*mut c_void, XrunCb, *mut c_void) -> c_int,
    on_shutdown: unsafe extern "C" fn(*mut c_void, ShutdownCb, *mut c_void),
    activate: unsafe extern "C" fn(*mut c_void) -> c_int,
    deactivate: unsafe extern "C" fn(*mut c_void) -> c_int,
    get_ports:
        unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char, u64) -> *mut *const c_char,
    connect: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int,
    free: unsafe extern "C" fn(*mut c_void),
    /// `jack_port_get_latency_range` (JACK ≥ 0.120 / 1.9.8).
    latency_range: Option<unsafe extern "C" fn(*mut c_void, c_int, *mut LatencyRange)>,
}

/// `jack_latency_range_t`.
#[repr(C)]
#[derive(Default)]
struct LatencyRange {
    min: u32,
    max: u32,
}

const PLAYBACK_LATENCY: c_int = 1;
const CAPTURE_LATENCY: c_int = 0;

impl Api {
    fn load() -> Result<Self, String> {
        let lib = Library::open(&[
            "libjack.so.0",
            "libjack.so",
            "libjack.0.dylib",
            "/usr/local/lib/libjack.0.dylib",
            "/opt/homebrew/lib/libjack.0.dylib",
            "libjack64.dll",
            "libjack.dll",
        ])?;
        // SAFETY: the types are libjack's documented signatures.
        unsafe {
            Ok(Self {
                client_open: lib.get(c"jack_client_open")?,
                client_close: lib.get(c"jack_client_close")?,
                get_sample_rate: lib.get(c"jack_get_sample_rate")?,
                get_buffer_size: lib.get(c"jack_get_buffer_size")?,
                port_register: lib.get(c"jack_port_register")?,
                port_get_buffer: lib.get(c"jack_port_get_buffer")?,
                port_name: lib.get(c"jack_port_name")?,
                set_process_callback: lib.get(c"jack_set_process_callback")?,
                set_xrun_callback: lib.get(c"jack_set_xrun_callback")?,
                on_shutdown: lib.get(c"jack_on_shutdown")?,
                activate: lib.get(c"jack_activate")?,
                deactivate: lib.get(c"jack_deactivate")?,
                get_ports: lib.get(c"jack_get_ports")?,
                connect: lib.get(c"jack_connect")?,
                free: lib.get(c"jack_free")?,
                latency_range: lib.get(c"jack_port_get_latency_range").ok(),
                _lib: lib,
            })
        }
    }
}

/// The JACK backend.
#[derive(Debug, Clone)]
pub struct Jack {
    /// Client name shown in the server's graph.
    pub client_name: String,
    /// Connect new ports to the physical ports.
    pub autoconnect: bool,
}

impl Default for Jack {
    fn default() -> Self {
        Self {
            client_name: "vieww".into(),
            autoconnect: true,
        }
    }
}

enum Callback {
    Render(RenderFn),
    Capture(CaptureFn),
}

struct Shared {
    api: Arc<Api>,
    ports: Vec<*mut c_void>,
    callback: Callback,
    scratch: Vec<f32>,
    counters: Arc<Counters>,
    rate: u32,
    frames_done: u64,
}

unsafe extern "C" fn process(nframes: u32, arg: *mut c_void) -> c_int {
    // SAFETY: `arg` is the `Shared` boxed in `open`, alive until the client
    // is closed (after which JACK makes no further calls).
    let s = unsafe { &mut *arg.cast::<Shared>() };
    let n = nframes as usize;
    let ch = s.ports.len();
    let mut bufs = [null_mut::<f32>(); 64];
    for (b, &p) in bufs.iter_mut().zip(&s.ports) {
        // SAFETY: a registered port; valid for this cycle.
        *b = unsafe { (s.api.port_get_buffer)(p, nframes) };
    }
    if bufs[..ch].iter().any(|b| b.is_null()) {
        return 0;
    }
    let mut done = 0;
    while done < n {
        let k = (n - done).min(CHUNK);
        let block = &mut s.scratch[..k * ch];
        #[expect(clippy::cast_precision_loss, reason = "stream time")]
        let info = BlockInfo {
            frames: k,
            channels: ch,
            rate: s.rate,
            time: (s.frames_done + done as u64) as f64 / f64::from(s.rate),
        };
        match &mut s.callback {
            Callback::Render(r) => {
                block.fill(0.0);
                r(block, &info);
                for (c, b) in bufs[..ch].iter().enumerate() {
                    // SAFETY: the port buffer holds `nframes` samples.
                    let dst = unsafe { std::slice::from_raw_parts_mut(b.add(done), k) };
                    for (i, d) in dst.iter_mut().enumerate() {
                        *d = block[i * ch + c];
                    }
                }
            }
            Callback::Capture(cap) => {
                for (c, b) in bufs[..ch].iter().enumerate() {
                    // SAFETY: as above.
                    let src = unsafe { std::slice::from_raw_parts(b.add(done), k) };
                    for (i, v) in src.iter().enumerate() {
                        block[i * ch + c] = *v;
                    }
                }
                cap(block, &info);
            }
        }
        done += k;
    }
    s.frames_done += n as u64;
    s.counters.block(n);
    // Latency: the port's own range (the graph downstream of it, i.e. the
    // driver's buffering) plus this cycle. The server recomputes ranges
    // after graph changes, so it is read every 64 cycles, not once.
    if let (Some(lr), Some(&p)) = (s.api.latency_range, s.ports.first()) {
        if s.counters.callbacks.load(Ordering::Relaxed) % 64 == 1 {
            let mut r = LatencyRange::default();
            let mode = if matches!(s.callback, Callback::Capture(_)) {
                CAPTURE_LATENCY
            } else {
                PLAYBACK_LATENCY
            };
            // SAFETY: a registered port and a valid out-pointer; allowed
            // from the process callback.
            unsafe { lr(p, mode, &raw mut r) };
            s.counters
                .set_latency_frames(u64::from(r.max) + u64::from(nframes), s.rate);
        }
    }
    0
}

unsafe extern "C" fn xrun(arg: *mut c_void) -> c_int {
    // SAFETY: as in `process` (only the atomic counters are touched).
    let s = unsafe { &*arg.cast::<Shared>() };
    s.counters.xruns.fetch_add(1, Ordering::Relaxed);
    0
}

unsafe extern "C" fn shutdown(arg: *mut c_void) {
    // SAFETY: as in `xrun`.
    let s = unsafe { &*arg.cast::<Shared>() };
    s.counters.ended.store(true, Ordering::Release);
}

struct JackStream {
    api: Arc<Api>,
    client: *mut c_void,
    shared: *mut Shared,
}

// SAFETY: the client handle is usable from any thread; `shared` is only
// freed in `stop`, after the client is closed.
unsafe impl Send for JackStream {}

impl StreamImpl for JackStream {
    fn stop(&mut self) {
        if self.client.is_null() {
            return;
        }
        // SAFETY: a live client; after close, JACK calls nothing more, so
        // the shared state can be freed.
        unsafe {
            (self.api.deactivate)(self.client);
            (self.api.client_close)(self.client);
            drop(Box::from_raw(self.shared));
        }
        self.client = null_mut();
    }
}

fn cstr(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        // SAFETY: JACK port names are NUL-terminated.
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

impl Jack {
    fn open(
        &self,
        config: StreamConfig,
        callback: Callback,
        target: Option<&str>,
    ) -> Result<DeviceStream, BackendError> {
        let api = Arc::new(Api::load().map_err(BackendError::Unavailable)?);
        let name = CString::new(self.client_name.as_str())
            .map_err(|e| BackendError::Refused(e.to_string()))?;
        let mut status = 0;
        // SAFETY: a valid name and status pointer; no variadic arguments
        // follow (no JackServerName option).
        let client = unsafe { (api.client_open)(name.as_ptr(), NO_START_SERVER, &raw mut status) };
        if client.is_null() {
            return Err(BackendError::Unavailable(format!(
                "no JACK server running (status {status:#x})"
            )));
        }
        let input = matches!(callback, Callback::Capture(_));
        // A named device: an explicit list of ports, or a client whose
        // physical ports to use. Resolved before anything is registered,
        // so an unknown name refuses instead of playing somewhere else.
        let targets: Option<Vec<String>> = match target {
            None => None,
            Some(t) if t.contains(':') => Some(t.split(',').map(|p| p.trim().to_owned()).collect()),
            Some(t) => {
                let want = PORT_IS_PHYSICAL | if input { PORT_IS_OUTPUT } else { PORT_IS_INPUT };
                // SAFETY: a live client; the list is freed with jack_free.
                let all = unsafe { port_list(&api, client, want) };
                Some(
                    all.into_iter()
                        .filter(|p| p.split_once(':').is_some_and(|(c, _)| c == t))
                        .collect(),
                )
            }
        };
        if targets.as_ref().is_some_and(Vec::is_empty) {
            // SAFETY: a live client, closed once.
            unsafe { (api.client_close)(client) };
            return Err(BackendError::Refused(format!(
                "no JACK device {:?}",
                target.unwrap_or_default()
            )));
        }
        let ch = usize::from(config.channels.clamp(1, 64));
        let flag = if input { PORT_IS_INPUT } else { PORT_IS_OUTPUT };
        let mut ports = Vec::with_capacity(ch);
        for c in 0..ch {
            let pname = CString::new(format!("{}_{}", if input { "in" } else { "out" }, c + 1))
                .unwrap_or_default();
            // SAFETY: a live client and valid strings.
            let p = unsafe {
                (api.port_register)(client, pname.as_ptr(), AUDIO_TYPE.as_ptr(), flag, 0)
            };
            if p.is_null() {
                // SAFETY: closing the client we opened.
                unsafe { (api.client_close)(client) };
                return Err(BackendError::Refused("could not register a port".into()));
            }
            ports.push(p);
        }
        // SAFETY: a live client.
        let (rate, period) =
            unsafe { ((api.get_sample_rate)(client), (api.get_buffer_size)(client)) };
        let counters = Arc::new(Counters::default());
        // The callback crosses an `extern "C"` boundary on JACK's process
        // thread: isolate it before it is stored, so a panicking closure
        // ends the stream instead of the process.
        let callback = match callback {
            Callback::Render(r) => Callback::Render(isolate_render(r, &counters)),
            Callback::Capture(c) => Callback::Capture(isolate_capture(c, &counters)),
        };
        let shared = Box::into_raw(Box::new(Shared {
            api: api.clone(),
            ports: ports.clone(),
            callback,
            scratch: vec![0.0; CHUNK * ch],
            counters: counters.clone(),
            rate,
            frames_done: 0,
        }));
        // SAFETY: callbacks registered before activation, with the shared
        // state as their argument.
        let ok = unsafe {
            (api.set_process_callback)(client, process, shared.cast()) == 0
                && (api.set_xrun_callback)(client, xrun, shared.cast()) == 0
        };
        // SAFETY: as above.
        unsafe { (api.on_shutdown)(client, shutdown, shared.cast()) };
        // SAFETY: a configured client.
        if !ok || unsafe { (api.activate)(client) } != 0 {
            // SAFETY: never activated: close, then free.
            unsafe {
                (api.client_close)(client);
                drop(Box::from_raw(shared));
            }
            return Err(BackendError::Refused(
                "the server refused activation".into(),
            ));
        }
        if let Some(targets) = &targets {
            // SAFETY: a live, active client; valid port names.
            unsafe {
                for (&p, other) in ports.iter().zip(targets) {
                    let Ok(other) = CString::new(other.as_str()) else {
                        continue;
                    };
                    let mine = (api.port_name)(p);
                    if input {
                        (api.connect)(client, other.as_ptr(), mine);
                    } else {
                        (api.connect)(client, mine, other.as_ptr());
                    }
                }
            }
        } else if self.autoconnect {
            // Physical playback ports are inputs; capture ports outputs.
            let want = PORT_IS_PHYSICAL | if input { PORT_IS_OUTPUT } else { PORT_IS_INPUT };
            // SAFETY: a live client; the list is freed with jack_free.
            unsafe {
                let list = (api.get_ports)(client, null(), AUDIO_TYPE.as_ptr(), want);
                if !list.is_null() {
                    for (i, &p) in ports.iter().enumerate() {
                        let other = *list.add(i);
                        if other.is_null() {
                            break;
                        }
                        let mine = (api.port_name)(p);
                        if input {
                            (api.connect)(client, other, mine);
                        } else {
                            (api.connect)(client, mine, other);
                        }
                    }
                    (api.free)(list.cast());
                }
            }
        }
        let granted = StreamConfig {
            rate,
            channels: u16::try_from(ch).unwrap_or(2),
            block: period,
        };
        Ok(DeviceStream::new(
            "jack",
            granted,
            counters,
            Box::new(JackStream {
                api,
                client,
                shared,
            }),
        ))
    }

    /// The server's port names (for diagnostics and routing UIs).
    ///
    /// # Errors
    /// No JACK, or no server.
    pub fn ports(&self) -> Result<Vec<String>, BackendError> {
        let api = Api::load().map_err(BackendError::Unavailable)?;
        let name = CString::new(format!("{}-probe", self.client_name)).unwrap_or_default();
        let mut status = 0;
        // SAFETY: as in `open`.
        let client = unsafe { (api.client_open)(name.as_ptr(), NO_START_SERVER, &raw mut status) };
        if client.is_null() {
            return Err(BackendError::Unavailable("no JACK server running".into()));
        }
        let mut out = Vec::new();
        // SAFETY: a live client; the list is freed with jack_free.
        unsafe {
            let list = (api.get_ports)(client, null(), null(), 0);
            if !list.is_null() {
                let mut i = 0;
                while !(*list.add(i)).is_null() {
                    out.push(cstr(*list.add(i)));
                    i += 1;
                }
                (api.free)(list.cast());
            }
            (api.client_close)(client);
        }
        Ok(out)
    }
}

impl Backend for Jack {
    fn name(&self) -> &'static str {
        "jack"
    }

    fn open_output(
        &self,
        config: StreamConfig,
        render: RenderFn,
    ) -> Result<DeviceStream, BackendError> {
        self.open(config, Callback::Render(render), None)
    }

    fn open_input(
        &self,
        config: StreamConfig,
        capture: CaptureFn,
    ) -> Result<DeviceStream, BackendError> {
        self.open(config, Callback::Capture(capture), None)
    }

    /// One device per client that owns physical ports (`system` is the
    /// sound card), with its playback and capture port counts.
    fn devices(&self) -> Result<Vec<DeviceInfo>, BackendError> {
        let api = Api::load().map_err(BackendError::Unavailable)?;
        let name = CString::new(format!("{}-list", self.client_name)).unwrap_or_default();
        let mut status = 0;
        // SAFETY: as in `open`.
        let client = unsafe { (api.client_open)(name.as_ptr(), NO_START_SERVER, &raw mut status) };
        if client.is_null() {
            return Err(BackendError::Unavailable("no JACK server running".into()));
        }
        // SAFETY: a live client, closed once after both lists.
        let (play, rec) = unsafe {
            let play = port_list(&api, client, PORT_IS_PHYSICAL | PORT_IS_INPUT);
            let rec = port_list(&api, client, PORT_IS_PHYSICAL | PORT_IS_OUTPUT);
            (api.client_close)(client);
            (play, rec)
        };
        let mut out: Vec<DeviceInfo> = Vec::new();
        let client_of = |p: &str| p.split_once(':').map_or(p, |(c, _)| c).to_owned();
        for (list, playback) in [(&play, true), (&rec, false)] {
            for p in list {
                let c = client_of(p);
                let d = match out.iter_mut().find(|d| d.id == c) {
                    Some(d) => d,
                    None => {
                        out.push(DeviceInfo {
                            id: c.clone(),
                            name: c.clone(),
                            outputs: Some(0),
                            inputs: Some(0),
                            is_default: c == "system",
                        });
                        out.last_mut().expect("just pushed")
                    }
                };
                let slot = if playback {
                    &mut d.outputs
                } else {
                    &mut d.inputs
                };
                *slot = slot.map(|n| n.saturating_add(1));
            }
        }
        Ok(out)
    }

    /// `device` is a client from [`devices`](Backend::devices) (its
    /// physical ports in order), or explicit ports, comma-separated:
    /// `"system:playback_3,system:playback_4"`.
    fn open_output_on(
        &self,
        device: &str,
        config: StreamConfig,
        render: RenderFn,
    ) -> Result<DeviceStream, BackendError> {
        self.open(config, Callback::Render(render), Some(device))
    }
}

/// The names of the ports matching `flags`, audio only.
///
/// # Safety
/// `client` must be a live client of `api`.
unsafe fn port_list(api: &Api, client: *mut c_void, flags: u64) -> Vec<String> {
    let mut out = Vec::new();
    // SAFETY: the caller's live client; the list is freed with jack_free.
    unsafe {
        let list = (api.get_ports)(client, null(), AUDIO_TYPE.as_ptr(), flags);
        if !list.is_null() {
            let mut i = 0;
            while !(*list.add(i)).is_null() {
                out.push(cstr(*list.add(i)));
                i += 1;
            }
            (api.free)(list.cast());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Duration;

    #[test]
    fn lists_devices_and_routes_to_named_ports_or_refuses_cleanly() {
        let cfg = StreamConfig {
            rate: 48_000,
            channels: 2,
            block: 256,
        };
        let jack = Jack::default();
        match jack.devices() {
            Ok(devs) => {
                let sys = devs
                    .iter()
                    .find(|d| d.id == "system")
                    .expect("the dummy driver's system ports");
                assert!(sys.is_default && sys.outputs.unwrap_or(0) >= 2, "{devs:?}");
                // By client, and by explicit ports (reversed: R then L).
                for target in ["system", "system:playback_2,system:playback_1"] {
                    let s = jack
                        .open_output_on(target, cfg, Box::new(|_, _| {}))
                        .expect("a listed device opens");
                    assert!(s.wait_for(5, Duration::from_secs(5)), "{target}");
                }
                let mut ports = jack.ports().expect("listing");
                ports.sort();
                assert!(ports.iter().any(|p| p.starts_with("system:playback")));
                assert!(jack
                    .open_output_on("no-such-card", cfg, Box::new(|_, _| {}))
                    .is_err());
            }
            Err(e) => assert!(matches!(e, BackendError::Unavailable(_)), "{e:?}"),
        }
    }

    // These run against a real server when one is reachable (the CI job
    // starts `jackd -d dummy`); otherwise they check the refusal.
    #[test]
    fn renders_through_a_running_server_or_refuses_cleanly() {
        let seen = Arc::new(Mutex::new((0usize, 0usize, 0.0f64)));
        let s2 = seen.clone();
        let jack = Jack {
            client_name: "vieww-test-out".into(),
            autoconnect: true,
        };
        let r = jack.open_output(
            StreamConfig::default(),
            Box::new(move |buf, info| {
                for (i, v) in buf.iter_mut().enumerate() {
                    *v = ((i % 64) as f32 / 64.0) - 0.5;
                }
                if let Ok(mut g) = s2.try_lock() {
                    *g = (info.frames, info.channels, info.time);
                }
            }),
        );
        match r {
            Ok(stream) => {
                assert_eq!(stream.backend(), "jack");
                let c = stream.config();
                assert!(c.rate >= 8_000 && c.block > 0 && c.channels == 2);
                assert!(
                    stream.wait_for(10, Duration::from_secs(10)),
                    "callbacks run: {:?}",
                    stream.stats()
                );
                let (frames, channels, time) = *seen.lock().unwrap();
                assert_eq!((frames, channels), (c.block as usize, 2));
                assert!(time > 0.0);
                let ports = jack.ports().unwrap();
                assert!(
                    ports.iter().any(|p| p == "vieww-test-out:out_1"),
                    "{ports:?}"
                );
                let st = stream.stats();
                assert_eq!(st.frames, st.callbacks * u64::from(c.block));
            }
            Err(e) => {
                assert!(
                    !super::super::expected("jack"),
                    "jack was expected to be running: {e}"
                );
                assert!(matches!(e, BackendError::Unavailable(_)), "{e}");
            }
        }
    }

    #[test]
    fn captures_from_a_running_server_or_refuses_cleanly() {
        let got = Arc::new(Mutex::new(0usize));
        let g2 = got.clone();
        let jack = Jack {
            client_name: "vieww-test-in".into(),
            autoconnect: true,
        };
        match jack.open_input(
            StreamConfig {
                channels: 1,
                ..StreamConfig::default()
            },
            Box::new(move |buf, info| {
                assert_eq!(buf.len(), info.frames);
                *g2.lock().unwrap() += buf.len();
            }),
        ) {
            Ok(stream) => {
                assert!(stream.wait_for(5, Duration::from_secs(10)));
                drop(stream);
                assert!(*got.lock().unwrap() > 0);
            }
            Err(e) => {
                assert!(
                    !super::super::expected("jack"),
                    "jack was expected to be running: {e}"
                );
                assert!(matches!(e, BackendError::Unavailable(_)), "{e}");
            }
        }
    }
}
