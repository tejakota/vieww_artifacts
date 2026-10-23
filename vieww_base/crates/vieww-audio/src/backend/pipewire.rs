//! PipeWire, natively: the audio (and video) server of current Linux
//! desktops.
//!
//! `libpipewire-0.3` is loaded at run time and a `pw_stream` runs on a
//! `pw_thread_loop`. The format offered is raw interleaved `F32` at the
//! requested rate and channel count, as an SPA POD object encoded here —
//! SPA's builders are header-only inline functions, so there is nothing to
//! call; the encoding is tested byte-for-byte against what
//! `spa_format_audio_raw_build` produces. The `process` callback runs on
//! PipeWire's real-time thread and renders straight into the dequeued
//! buffer, sized by the server's request (the graph's quantum), so nothing
//! is copied or allocated there. `node.latency` asks for
//! [`StreamConfig::block`] frames per quantum.
//!
//! Opening waits for the stream to reach `PAUSED` or `STREAMING` (or an
//! error): a machine without a running daemon gets
//! [`BackendError::Unavailable`] promptly instead of a silent stream.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::ptr::null_mut;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::dl::Library;
use super::{
    isolate_capture, isolate_render, Backend, BackendError, BlockInfo, CaptureFn, Counters,
    DeviceInfo, DeviceStream, RenderFn, StreamConfig, StreamImpl,
};

// SPA constants (spa/utils/type.h, spa/param/*.h), printed from the headers.
const SPA_TYPE_ID: u32 = 3;
const SPA_TYPE_INT: u32 = 4;
const SPA_TYPE_ARRAY: u32 = 13;
const SPA_TYPE_OBJECT: u32 = 15;
const SPA_TYPE_OBJECT_FORMAT: u32 = 0x40003;
const SPA_PARAM_ENUM_FORMAT: u32 = 3;
const SPA_FORMAT_MEDIA_TYPE: u32 = 1;
const SPA_FORMAT_MEDIA_SUBTYPE: u32 = 2;
const SPA_FORMAT_AUDIO_FORMAT: u32 = 0x10001;
const SPA_FORMAT_AUDIO_RATE: u32 = 0x10003;
const SPA_FORMAT_AUDIO_CHANNELS: u32 = 0x10004;
const SPA_FORMAT_AUDIO_POSITION: u32 = 0x10005;
const SPA_MEDIA_TYPE_AUDIO: u32 = 1;
const SPA_MEDIA_SUBTYPE_RAW: u32 = 1;
const SPA_AUDIO_FORMAT_F32: u32 = 0x11b;
/// FL FR FC LFE RL RR SL SR (spa/param/audio/raw.h channel ids).
const POSITIONS: [u32; 8] = [3, 4, 5, 6, 8, 9, 10, 11];
const PW_ID_ANY: u32 = 0xffff_ffff;
const PW_DIRECTION_INPUT: c_int = 0;
const PW_DIRECTION_OUTPUT: c_int = 1;
const FLAG_AUTOCONNECT: u32 = 1 << 0;
const FLAG_MAP_BUFFERS: u32 = 1 << 2;
const FLAG_RT_PROCESS: u32 = 1 << 4;
const STATE_ERROR: c_int = -1;
const STATE_PAUSED: c_int = 2;
const STATE_STREAMING: c_int = 3;

/// An SPA POD encoder for the one object audio streams need.
#[derive(Debug, Default)]
struct Pod(Vec<u8>);

impl Pod {
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_ne_bytes());
    }

    fn pad(&mut self) {
        while !self.0.len().is_multiple_of(8) {
            self.0.push(0);
        }
    }

    fn prop_id(&mut self, key: u32, value: u32) {
        self.u32(key);
        self.u32(0);
        self.u32(4);
        self.u32(SPA_TYPE_ID);
        self.u32(value);
        self.pad();
    }

    fn prop_int(&mut self, key: u32, value: i32) {
        self.u32(key);
        self.u32(0);
        self.u32(4);
        self.u32(SPA_TYPE_INT);
        self.0.extend_from_slice(&value.to_ne_bytes());
        self.pad();
    }

    fn prop_id_array(&mut self, key: u32, values: &[u32]) {
        self.u32(key);
        self.u32(0);
        // Array body: the child pod header (size 4, Id), then the values.
        self.u32(8 + 4 * u32::try_from(values.len()).unwrap_or(0));
        self.u32(SPA_TYPE_ARRAY);
        self.u32(4);
        self.u32(SPA_TYPE_ID);
        for v in values {
            self.u32(*v);
        }
        self.pad();
    }

    /// The `EnumFormat` object for raw interleaved `F32` audio.
    fn audio_format(rate: u32, channels: u32) -> Vec<u8> {
        let mut body = Self::default();
        body.u32(SPA_TYPE_OBJECT_FORMAT);
        body.u32(SPA_PARAM_ENUM_FORMAT);
        body.prop_id(SPA_FORMAT_MEDIA_TYPE, SPA_MEDIA_TYPE_AUDIO);
        body.prop_id(SPA_FORMAT_MEDIA_SUBTYPE, SPA_MEDIA_SUBTYPE_RAW);
        body.prop_id(SPA_FORMAT_AUDIO_FORMAT, SPA_AUDIO_FORMAT_F32);
        body.prop_int(SPA_FORMAT_AUDIO_RATE, i32::try_from(rate).unwrap_or(48_000));
        body.prop_int(
            SPA_FORMAT_AUDIO_CHANNELS,
            i32::try_from(channels).unwrap_or(2),
        );
        if (1..=8).contains(&channels) && channels != 1 {
            body.prop_id_array(SPA_FORMAT_AUDIO_POSITION, &POSITIONS[..channels as usize]);
        }
        let mut pod = Self::default();
        pod.u32(u32::try_from(body.0.len()).unwrap_or(0));
        pod.u32(SPA_TYPE_OBJECT);
        pod.0.extend_from_slice(&body.0);
        pod.0
    }
}

#[repr(C)]
struct StreamEvents {
    version: u32,
    destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    state_changed: Option<unsafe extern "C" fn(*mut c_void, c_int, c_int, *const c_char)>,
    control_info: Option<unsafe extern "C" fn(*mut c_void, u32, *const c_void)>,
    io_changed: Option<unsafe extern "C" fn(*mut c_void, u32, *mut c_void, u32)>,
    param_changed: Option<unsafe extern "C" fn(*mut c_void, u32, *const c_void)>,
    add_buffer: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
    remove_buffer: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
    process: Option<unsafe extern "C" fn(*mut c_void)>,
    drained: Option<unsafe extern "C" fn(*mut c_void)>,
    command: Option<unsafe extern "C" fn(*mut c_void, *const c_void)>,
    trigger_done: Option<unsafe extern "C" fn(*mut c_void)>,
}

#[repr(C)]
struct PwBuffer {
    buffer: *mut SpaBuffer,
    user_data: *mut c_void,
    size: u64,
    requested: u64,
}

#[repr(C)]
struct SpaBuffer {
    n_metas: u32,
    n_datas: u32,
    metas: *mut c_void,
    datas: *mut SpaData,
}

#[repr(C)]
struct SpaData {
    type_: u32,
    flags: u32,
    fd: i64,
    mapoffset: u32,
    maxsize: u32,
    data: *mut c_void,
    chunk: *mut SpaChunk,
}

#[repr(C)]
struct SpaChunk {
    offset: u32,
    size: u32,
    stride: i32,
    flags: i32,
}

struct Api {
    _lib: Library,
    init: unsafe extern "C" fn(*mut c_int, *mut *mut *mut c_char),
    thread_loop_new: unsafe extern "C" fn(*const c_char, *const c_void) -> *mut c_void,
    thread_loop_get_loop: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    thread_loop_start: unsafe extern "C" fn(*mut c_void) -> c_int,
    thread_loop_stop: unsafe extern "C" fn(*mut c_void),
    thread_loop_lock: unsafe extern "C" fn(*mut c_void),
    thread_loop_unlock: unsafe extern "C" fn(*mut c_void),
    thread_loop_destroy: unsafe extern "C" fn(*mut c_void),
    properties_new: unsafe extern "C" fn(*const c_char, ...) -> *mut c_void,
    stream_new_simple: unsafe extern "C" fn(
        *mut c_void,
        *const c_char,
        *mut c_void,
        *const StreamEvents,
        *mut c_void,
    ) -> *mut c_void,
    stream_connect:
        unsafe extern "C" fn(*mut c_void, c_int, u32, u32, *mut *const c_void, u32) -> c_int,
    stream_get_state: unsafe extern "C" fn(*mut c_void, *mut *const c_char) -> c_int,
    stream_dequeue_buffer: unsafe extern "C" fn(*mut c_void) -> *mut PwBuffer,
    stream_queue_buffer: unsafe extern "C" fn(*mut c_void, *mut PwBuffer) -> c_int,
    stream_destroy: unsafe extern "C" fn(*mut c_void),
    /// `pw_stream_get_time_n` (PipeWire ≥ 0.3.50): the stream's timing,
    /// sized by the caller so older and newer `pw_time` layouts both work.
    stream_get_time_n: Option<unsafe extern "C" fn(*mut c_void, *mut PwTime, usize) -> c_int>,
}

/// The head of `struct pw_time` — the fields every 0.3/1.x version lays
/// out first, in this order.
#[repr(C)]
#[derive(Default)]
struct PwTime {
    now: i64,
    rate_num: u32,
    rate_denom: u32,
    ticks: u64,
    /// Delay to the device, in `rate` units.
    delay: i64,
    /// Samples queued in the stream, not yet handed to the graph.
    queued: u64,
    /// Samples buffered in the stream's converter/resampler (≥ 0.3.50).
    buffered: u64,
}

impl Api {
    fn load() -> Result<Self, String> {
        let lib = Library::open(&["libpipewire-0.3.so.0", "libpipewire-0.3.so"])?;
        // SAFETY: the types are libpipewire's documented signatures.
        unsafe {
            Ok(Self {
                init: lib.get(c"pw_init")?,
                thread_loop_new: lib.get(c"pw_thread_loop_new")?,
                thread_loop_get_loop: lib.get(c"pw_thread_loop_get_loop")?,
                thread_loop_start: lib.get(c"pw_thread_loop_start")?,
                thread_loop_stop: lib.get(c"pw_thread_loop_stop")?,
                thread_loop_lock: lib.get(c"pw_thread_loop_lock")?,
                thread_loop_unlock: lib.get(c"pw_thread_loop_unlock")?,
                thread_loop_destroy: lib.get(c"pw_thread_loop_destroy")?,
                properties_new: lib.get(c"pw_properties_new")?,
                stream_new_simple: lib.get(c"pw_stream_new_simple")?,
                stream_connect: lib.get(c"pw_stream_connect")?,
                stream_get_state: lib.get(c"pw_stream_get_state")?,
                stream_dequeue_buffer: lib.get(c"pw_stream_dequeue_buffer")?,
                stream_queue_buffer: lib.get(c"pw_stream_queue_buffer")?,
                stream_get_time_n: lib.get(c"pw_stream_get_time_n").ok(),
                stream_destroy: lib.get(c"pw_stream_destroy")?,
                _lib: lib,
            })
        }
    }
}

enum Callback {
    Render(RenderFn),
    Capture(CaptureFn),
}

struct Shared {
    api: Arc<Api>,
    stream: *mut c_void,
    callback: Callback,
    channels: usize,
    rate: u32,
    counters: Arc<Counters>,
    frames_done: u64,
    state: AtomicI32,
}

unsafe extern "C" fn on_state(data: *mut c_void, _old: c_int, state: c_int, _err: *const c_char) {
    // SAFETY: `data` is the boxed `Shared` (alive until the stream is
    // destroyed); only atomics are touched.
    let s = unsafe { &*data.cast::<Shared>() };
    s.state.store(state, Ordering::Release);
    if state == STATE_ERROR {
        s.counters.ended.store(true, Ordering::Release);
    }
}

unsafe extern "C" fn on_process(data: *mut c_void) {
    // SAFETY: as in `on_state`; PipeWire calls `process` from one thread
    // at a time, so the exclusive borrow is sound.
    let s = unsafe { &mut *data.cast::<Shared>() };
    // SAFETY: a live stream, from its process callback.
    let b = unsafe { (s.api.stream_dequeue_buffer)(s.stream) };
    if b.is_null() {
        // No buffer to trade is the data thread arriving too late for the
        // driver's deadline — the one underrun signal this API offers, and
        // until now the one this backend silently swallowed. Every other
        // backend counts its misses; this one now does too.
        s.counters.xruns.fetch_add(1, Ordering::Relaxed);
        return;
    }
    // SAFETY: a dequeued buffer is valid until queued again.
    unsafe {
        let buf = (*b).buffer;
        if buf.is_null() || (*buf).n_datas == 0 {
            (s.api.stream_queue_buffer)(s.stream, b);
            return;
        }
        let d = &mut *(*buf).datas;
        if d.data.is_null() || d.chunk.is_null() {
            (s.api.stream_queue_buffer)(s.stream, b);
            return;
        }
        let stride = s.channels * 4;
        let ch = s.channels;
        #[expect(clippy::cast_precision_loss, reason = "stream time")]
        let time = s.frames_done as f64 / f64::from(s.rate);
        let frames = match &mut s.callback {
            Callback::Render(r) => {
                let mut n = d.maxsize as usize / stride;
                if (*b).requested > 0 {
                    n = n.min(usize::try_from((*b).requested).unwrap_or(n));
                }
                let out = std::slice::from_raw_parts_mut(d.data.cast::<f32>(), n * ch);
                out.fill(0.0);
                r(
                    out,
                    &BlockInfo {
                        frames: n,
                        channels: ch,
                        rate: s.rate,
                        time,
                    },
                );
                let c = &mut *d.chunk;
                c.offset = 0;
                c.stride = i32::try_from(stride).unwrap_or(8);
                c.size = u32::try_from(n * stride).unwrap_or(0);
                (*b).size = n as u64;
                n
            }
            Callback::Capture(cap) => {
                let c = &*d.chunk;
                let off = (c.offset % d.maxsize.max(1)) as usize;
                let size = (c.size as usize).min(d.maxsize as usize - off);
                let n = size / stride;
                let input =
                    std::slice::from_raw_parts(d.data.cast::<u8>().add(off).cast::<f32>(), n * ch);
                cap(
                    input,
                    &BlockInfo {
                        frames: n,
                        channels: ch,
                        rate: s.rate,
                        time,
                    },
                );
                n
            }
        };
        (s.api.stream_queue_buffer)(s.stream, b);
        s.frames_done += frames as u64;
        s.counters.block(frames);
        // Latency: the graph's delay to the device plus what this stream
        // still holds, every 64 cycles (a lock-free read of shared state,
        // documented as callable from `process`).
        if let Some(get_time) = s.api.stream_get_time_n {
            if s.counters.callbacks.load(Ordering::Relaxed) % 64 == 1 {
                let mut t = PwTime::default();
                if get_time(s.stream, &raw mut t, size_of::<PwTime>()) == 0 && t.rate_denom > 0 {
                    #[expect(clippy::cast_sign_loss, reason = "clamped non-negative")]
                    let delay_ns = (t.delay.max(0) as u128 * u128::from(t.rate_num) * 1_000_000_000
                        / u128::from(t.rate_denom)) as u64;
                    let held = t
                        .queued
                        .saturating_add(t.buffered)
                        .saturating_add(frames as u64);
                    let held_ns = held.saturating_mul(1_000_000_000) / u64::from(s.rate.max(1));
                    s.counters
                        .latency_ns
                        .store(delay_ns.saturating_add(held_ns), Ordering::Relaxed);
                }
            }
        }
    }
}

static EVENTS: StreamEvents = StreamEvents {
    version: 2,
    destroy: None,
    state_changed: Some(on_state),
    control_info: None,
    io_changed: None,
    param_changed: None,
    add_buffer: None,
    remove_buffer: None,
    process: Some(on_process),
    drained: None,
    command: None,
    trigger_done: None,
};

struct PwStream {
    api: Arc<Api>,
    lp: *mut c_void,
    stream: *mut c_void,
    shared: *mut Shared,
}

// SAFETY: the loop and stream are only touched under the loop lock, and
// `shared` is freed after the stream is destroyed.
unsafe impl Send for PwStream {}

impl StreamImpl for PwStream {
    fn stop(&mut self) {
        if self.lp.is_null() {
            return;
        }
        // SAFETY: the teardown order the thread-loop documentation gives:
        // destroy the stream under the lock, stop the loop without it.
        unsafe {
            (self.api.thread_loop_lock)(self.lp);
            if !self.stream.is_null() {
                (self.api.stream_destroy)(self.stream);
            }
            (self.api.thread_loop_unlock)(self.lp);
            (self.api.thread_loop_stop)(self.lp);
            (self.api.thread_loop_destroy)(self.lp);
            drop(Box::from_raw(self.shared));
        }
        self.lp = null_mut();
    }
}

/// The PipeWire backend.
#[derive(Debug, Clone, Copy, Default)]
pub struct PipeWire;

fn daemon_socket_present() -> bool {
    if std::env::var_os("PIPEWIRE_REMOTE").is_some() {
        return true;
    }
    let Some(dir) =
        std::env::var_os("PIPEWIRE_RUNTIME_DIR").or_else(|| std::env::var_os("XDG_RUNTIME_DIR"))
    else {
        return false;
    };
    std::path::Path::new(&dir).join("pipewire-0").exists()
}

fn open(
    config: StreamConfig,
    callback: Callback,
    target: Option<&str>,
) -> Result<DeviceStream, BackendError> {
    let target = target
        .map(CString::new)
        .transpose()
        .map_err(|_| BackendError::Refused("a device name with a NUL byte".into()))?;
    if !daemon_socket_present() {
        return Err(BackendError::Unavailable(
            "no PipeWire daemon socket".into(),
        ));
    }
    let api = Arc::new(Api::load().map_err(BackendError::Unavailable)?);
    let capture = matches!(callback, Callback::Capture(_));
    let ch = u32::from(config.channels.clamp(1, 64));
    let latency =
        CString::new(format!("{}/{}", config.block.max(16), config.rate)).unwrap_or_default();
    let counters = Arc::new(Counters::default());
    // The callback crosses into SPA's `on_process` on PipeWire's realtime
    // data thread: isolate it before it is stored, so a panicking closure
    // ends the stream instead of the process.
    let callback = match callback {
        Callback::Render(r) => Callback::Render(isolate_render(r, &counters)),
        Callback::Capture(c) => Callback::Capture(isolate_capture(c, &counters)),
    };
    let shared = Box::into_raw(Box::new(Shared {
        api: api.clone(),
        stream: null_mut(),
        callback,
        channels: ch as usize,
        rate: config.rate,
        counters: counters.clone(),
        frames_done: 0,
        state: AtomicI32::new(0),
    }));
    let pod = Pod::audio_format(config.rate, ch);
    // SAFETY: the pw_* calls follow the thread-loop protocol (create and
    // connect under the lock); strings and the POD outlive the calls.
    unsafe {
        (api.init)(null_mut(), null_mut());
        let lp = (api.thread_loop_new)(c"vieww-pipewire".as_ptr(), std::ptr::null());
        if lp.is_null() {
            drop(Box::from_raw(shared));
            return Err(BackendError::Unavailable(
                "pw_thread_loop_new failed".into(),
            ));
        }
        let props = (api.properties_new)(
            c"media.type".as_ptr(),
            c"Audio".as_ptr(),
            c"media.category".as_ptr(),
            if capture {
                c"Capture".as_ptr()
            } else {
                c"Playback".as_ptr()
            },
            c"media.role".as_ptr(),
            c"Music".as_ptr(),
            c"node.latency".as_ptr(),
            latency.as_ptr(),
            // A named device: `target.object` (PipeWire ≥ 0.3.44) and the
            // older `node.target`, both by node name. A null key ends the
            // list early when there is no target.
            if target.is_some() {
                c"target.object".as_ptr()
            } else {
                std::ptr::null::<c_char>()
            },
            target.as_ref().map_or(std::ptr::null(), |t| t.as_ptr()),
            c"node.target".as_ptr(),
            target.as_ref().map_or(std::ptr::null(), |t| t.as_ptr()),
            std::ptr::null::<c_char>(),
        );
        (api.thread_loop_lock)(lp);
        let stream = (api.stream_new_simple)(
            (api.thread_loop_get_loop)(lp),
            c"vieww".as_ptr(),
            props,
            &raw const EVENTS,
            shared.cast(),
        );
        if stream.is_null() {
            (api.thread_loop_unlock)(lp);
            (api.thread_loop_destroy)(lp);
            drop(Box::from_raw(shared));
            return Err(BackendError::Unavailable(
                "pw_stream_new_simple failed".into(),
            ));
        }
        (*shared).stream = stream;
        let mut params = [pod.as_ptr().cast::<c_void>()];
        let r = (api.stream_connect)(
            stream,
            if capture {
                PW_DIRECTION_INPUT
            } else {
                PW_DIRECTION_OUTPUT
            },
            PW_ID_ANY,
            FLAG_AUTOCONNECT | FLAG_MAP_BUFFERS | FLAG_RT_PROCESS,
            params.as_mut_ptr(),
            1,
        );
        let started = r >= 0 && (api.thread_loop_start)(lp) >= 0;
        (api.thread_loop_unlock)(lp);
        let mut handle = PwStream {
            api: api.clone(),
            lp,
            stream,
            shared,
        };
        if !started {
            handle.stop();
            let msg = format!("pw_stream_connect failed ({r})");
            // No daemon behind the socket (it crashed, or a stale socket
            // survived it): -EHOSTDOWN, -ECONNREFUSED, -ENOENT, -EPIPE,
            // -ECONNRESET. The audio system is not there — "unavailable",
            // as when the socket is missing — not a refusal of this stream.
            return Err(if matches!(r, -112 | -111 | -2 | -32 | -104) {
                BackendError::Unavailable(msg)
            } else {
                BackendError::Refused(msg)
            });
        }
        // Wait for the connection to settle.
        let t0 = Instant::now();
        loop {
            let st = (*shared).state.load(Ordering::Acquire);
            if st == STATE_PAUSED || st == STATE_STREAMING {
                break;
            }
            if st == STATE_ERROR || t0.elapsed() > Duration::from_secs(3) {
                (api.thread_loop_lock)(lp);
                let mut err = std::ptr::null();
                (api.stream_get_state)(stream, &raw mut err);
                let msg = if err.is_null() {
                    "the stream did not connect".to_owned()
                } else {
                    CStr::from_ptr(err).to_string_lossy().into_owned()
                };
                (api.thread_loop_unlock)(lp);
                handle.stop();
                return Err(BackendError::Unavailable(msg));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(DeviceStream::new(
            "pipewire",
            StreamConfig {
                rate: config.rate,
                channels: u16::try_from(ch).unwrap_or(2),
                block: config.block.max(16),
            },
            counters,
            Box::new(handle),
        ))
    }
}

impl Backend for PipeWire {
    fn name(&self) -> &'static str {
        "pipewire"
    }

    fn open_output(
        &self,
        config: StreamConfig,
        render: RenderFn,
    ) -> Result<DeviceStream, BackendError> {
        open(config, Callback::Render(render), None)
    }

    fn open_input(
        &self,
        config: StreamConfig,
        capture: CaptureFn,
    ) -> Result<DeviceStream, BackendError> {
        open(config, Callback::Capture(capture), None)
    }

    /// The sinks, as `pipewire-pulse` (PipeWire's PulseAudio server, on
    /// every PipeWire desktop) lists them: their ids are PipeWire node
    /// names, which is what [`open_output_on`](Backend::open_output_on)
    /// targets.
    fn devices(&self) -> Result<Vec<DeviceInfo>, BackendError> {
        if !daemon_socket_present() {
            return Err(BackendError::Unavailable(
                "no PipeWire daemon socket".into(),
            ));
        }
        super::pulse::Pulse.devices()
    }

    /// `device` is a node name (`alsa_output.pci-0000_00_1f.3.analog-stereo`).
    /// PipeWire treats the target as a preference: if the node is gone, the
    /// session manager routes to the default instead.
    fn open_output_on(
        &self,
        device: &str,
        config: StreamConfig,
        render: RenderFn,
    ) -> Result<DeviceStream, BackendError> {
        open(config, Callback::Render(render), Some(device))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `spa_format_audio_raw_build(EnumFormat, {F32, 48000, 2, [FL, FR]})`
    /// as printed by a C program compiled against the SPA headers.
    const REFERENCE: &str = "a00000000f000000 0300040003000000 0100000000000000 0400000003000000 \
        0100000000000000 0200000000000000 0400000003000000 0100000000000000 0100010000000000 \
        0400000003000000 1b01000000000000 0300010000000000 0400000004000000 80bb000000000000 \
        0400010000000000 0400000004000000 0200000000000000 0500010000000000 100000000d000000 \
        0400000003000000 0300000004000000";

    #[test]
    #[cfg(target_endian = "little")]
    fn the_format_pod_matches_spa_byte_for_byte() {
        let want: Vec<u8> = REFERENCE
            .split_whitespace()
            .flat_map(|w| {
                (0..w.len())
                    .step_by(2)
                    .map(move |i| u8::from_str_radix(&w[i..i + 2], 16).unwrap())
            })
            .collect();
        assert_eq!(Pod::audio_format(48_000, 2), want);
        // Mono has no position array; the size field follows.
        let mono = Pod::audio_format(44_100, 1);
        assert_eq!(
            u32::from_ne_bytes(mono[0..4].try_into().unwrap()) as usize,
            mono.len() - 8
        );
        assert_eq!(mono.len() % 8, 0);
    }

    #[test]
    fn the_buffer_structs_have_the_header_layouts() {
        assert_eq!(size_of::<StreamEvents>(), 96);
        // The C struct is 40 bytes (a later `time` field); PipeWire owns
        // it and this side only reads the first four fields through a
        // pointer, so declaring those is exact.
        assert_eq!(size_of::<PwBuffer>(), 32);
        assert_eq!(size_of::<SpaBuffer>(), 24);
        assert_eq!(size_of::<SpaData>(), 40);
        assert_eq!(std::mem::offset_of!(SpaData, data), 24);
        assert_eq!(std::mem::offset_of!(SpaData, chunk), 32);
        assert_eq!(size_of::<SpaChunk>(), 16);
    }

    #[test]
    fn plays_through_a_running_daemon_or_refuses_cleanly() {
        match PipeWire.open_output(
            StreamConfig::default(),
            Box::new(|buf, info| {
                assert_eq!(buf.len(), info.frames * info.channels);
                for v in buf.iter_mut() {
                    *v = 0.01;
                }
            }),
        ) {
            Ok(stream) => {
                assert_eq!(stream.backend(), "pipewire");
                assert!(
                    stream.wait_for(10, Duration::from_secs(10)),
                    "{:?}",
                    stream.stats()
                );
            }
            Err(e) => {
                assert!(
                    !super::super::expected("pipewire"),
                    "pipewire was expected to be running: {e}"
                );
                assert!(matches!(e, BackendError::Unavailable(_)), "{e}");
            }
        }
    }

    #[test]
    fn captures_through_a_running_daemon_or_refuses_cleanly() {
        match PipeWire.open_input(
            StreamConfig {
                channels: 1,
                ..StreamConfig::default()
            },
            Box::new(|buf, info| assert_eq!(buf.len(), info.frames * info.channels)),
        ) {
            Ok(stream) => assert!(
                stream.wait_for(5, Duration::from_secs(10)),
                "{:?}",
                stream.stats()
            ),
            Err(e) => {
                assert!(
                    !super::super::expected("pipewire"),
                    "pipewire was expected to be running: {e}"
                );
                assert!(matches!(e, BackendError::Unavailable(_)), "{e}");
            }
        }
    }
}
