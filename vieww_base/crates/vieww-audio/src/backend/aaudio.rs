//! AAudio: Android's low-latency audio API (Android 8.0, API 26+).
//!
//! `libaaudio.so` is loaded at run time (so an APK built for older API
//! levels still starts, and reports the backend unavailable). A stream is
//! built for float samples in low-latency performance mode with a data
//! callback, which AAudio runs on its real-time thread with an
//! interleaved buffer to fill (or read). An error callback — a device
//! disconnected, typically — marks the stream ended. Signatures and
//! constants follow `<aaudio/AAudio.h>`.

use std::ffi::{c_char, c_void, CStr};
use std::ptr::null_mut;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;

use super::dl::Library;
use super::{
    isolate_capture, isolate_render, Backend, BackendError, BlockInfo, CaptureFn, Counters,
    DeviceStream, RenderFn, StreamConfig, StreamImpl,
};

type Result_ = i32;
const AAUDIO_OK: Result_ = 0;
const AAUDIO_DIRECTION_OUTPUT: i32 = 0;
const AAUDIO_DIRECTION_INPUT: i32 = 1;
const AAUDIO_FORMAT_PCM_FLOAT: i32 = 2;
const AAUDIO_SHARING_MODE_SHARED: i32 = 1;
const AAUDIO_PERFORMANCE_MODE_LOW_LATENCY: i32 = 12;
const AAUDIO_CALLBACK_RESULT_CONTINUE: i32 = 0;

type DataCb = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, i32) -> i32;
type ErrorCb = unsafe extern "C" fn(*mut c_void, *mut c_void, Result_);

struct Api {
    _lib: Library,
    create_builder: unsafe extern "C" fn(*mut *mut c_void) -> Result_,
    set_direction: unsafe extern "C" fn(*mut c_void, i32),
    set_sample_rate: unsafe extern "C" fn(*mut c_void, i32),
    set_channel_count: unsafe extern "C" fn(*mut c_void, i32),
    set_format: unsafe extern "C" fn(*mut c_void, i32),
    set_sharing_mode: unsafe extern "C" fn(*mut c_void, i32),
    set_performance_mode: unsafe extern "C" fn(*mut c_void, i32),
    set_frames_per_data_callback: unsafe extern "C" fn(*mut c_void, i32),
    set_data_callback: unsafe extern "C" fn(*mut c_void, DataCb, *mut c_void),
    set_error_callback: unsafe extern "C" fn(*mut c_void, ErrorCb, *mut c_void),
    open_stream: unsafe extern "C" fn(*mut c_void, *mut *mut c_void) -> Result_,
    builder_delete: unsafe extern "C" fn(*mut c_void) -> Result_,
    request_start: unsafe extern "C" fn(*mut c_void) -> Result_,
    request_stop: unsafe extern "C" fn(*mut c_void) -> Result_,
    close: unsafe extern "C" fn(*mut c_void) -> Result_,
    get_sample_rate: unsafe extern "C" fn(*mut c_void) -> i32,
    get_channel_count: unsafe extern "C" fn(*mut c_void) -> i32,
    get_frames_per_burst: unsafe extern "C" fn(*mut c_void) -> i32,
    convert_result_to_text: unsafe extern "C" fn(Result_) -> *const c_char,
}

impl Api {
    fn load() -> Result<Self, String> {
        let lib = Library::open(&["libaaudio.so"])?;
        // SAFETY: the types are <aaudio/AAudio.h>'s signatures.
        unsafe {
            Ok(Self {
                create_builder: lib.get(c"AAudio_createStreamBuilder")?,
                set_direction: lib.get(c"AAudioStreamBuilder_setDirection")?,
                set_sample_rate: lib.get(c"AAudioStreamBuilder_setSampleRate")?,
                set_channel_count: lib.get(c"AAudioStreamBuilder_setChannelCount")?,
                set_format: lib.get(c"AAudioStreamBuilder_setFormat")?,
                set_sharing_mode: lib.get(c"AAudioStreamBuilder_setSharingMode")?,
                set_performance_mode: lib.get(c"AAudioStreamBuilder_setPerformanceMode")?,
                set_frames_per_data_callback: lib
                    .get(c"AAudioStreamBuilder_setFramesPerDataCallback")?,
                set_data_callback: lib.get(c"AAudioStreamBuilder_setDataCallback")?,
                set_error_callback: lib.get(c"AAudioStreamBuilder_setErrorCallback")?,
                open_stream: lib.get(c"AAudioStreamBuilder_openStream")?,
                builder_delete: lib.get(c"AAudioStreamBuilder_delete")?,
                request_start: lib.get(c"AAudioStream_requestStart")?,
                request_stop: lib.get(c"AAudioStream_requestStop")?,
                close: lib.get(c"AAudioStream_close")?,
                get_sample_rate: lib.get(c"AAudioStream_getSampleRate")?,
                get_channel_count: lib.get(c"AAudioStream_getChannelCount")?,
                get_frames_per_burst: lib.get(c"AAudioStream_getFramesPerBurst")?,
                convert_result_to_text: lib.get(c"AAudio_convertResultToText")?,
                _lib: lib,
            })
        }
    }

    fn text(&self, r: Result_) -> String {
        // SAFETY: AAudio returns a static string.
        let p = unsafe { (self.convert_result_to_text)(r) };
        if p.is_null() {
            format!("AAudio error {r}")
        } else {
            // SAFETY: a static NUL-terminated string.
            unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
        }
    }
}

enum Callback {
    Render(RenderFn),
    Capture(CaptureFn),
}

struct Shared {
    callback: Callback,
    /// Granted values, written by `open` after the stream exists. They are
    /// atomic because the data callback — registered before those values
    /// are known, the builder API allows nothing else — reads them; a
    /// plain store would make the safety of the read depend on AAudio
    /// never invoking the callback before `requestStart`, which is true of
    /// the API's ordering but true of nothing else.
    channels: AtomicUsize,
    rate: AtomicU32,
    counters: Arc<Counters>,
    frames_done: u64,
}

unsafe extern "C" fn on_data(
    _stream: *mut c_void,
    user: *mut c_void,
    data: *mut c_void,
    frames: i32,
) -> i32 {
    // SAFETY: `user` is the boxed `Shared`, alive until the stream closes;
    // AAudio calls one data callback at a time.
    let s = unsafe { &mut *user.cast::<Shared>() };
    let n = usize::try_from(frames.max(0)).unwrap_or(0);
    let ch = s.channels.load(Ordering::Acquire);
    let rate = s.rate.load(Ordering::Acquire);
    let len = n * ch;
    #[expect(clippy::cast_precision_loss, reason = "stream time")]
    let info = BlockInfo {
        frames: n,
        channels: ch,
        rate,
        time: s.frames_done as f64 / f64::from(rate),
    };
    match &mut s.callback {
        Callback::Render(r) => {
            // SAFETY: AAudio hands `frames` interleaved float frames.
            let out = unsafe { std::slice::from_raw_parts_mut(data.cast::<f32>(), len) };
            out.fill(0.0);
            r(out, &info);
        }
        Callback::Capture(c) => {
            // SAFETY: as above.
            let input = unsafe { std::slice::from_raw_parts(data.cast::<f32>(), len) };
            c(input, &info);
        }
    }
    s.frames_done += n as u64;
    s.counters.block(n);
    AAUDIO_CALLBACK_RESULT_CONTINUE
}

unsafe extern "C" fn on_error(_stream: *mut c_void, user: *mut c_void, _error: Result_) {
    // SAFETY: as in `on_data`; only an atomic is touched.
    let s = unsafe { &*user.cast::<Shared>() };
    s.counters.ended.store(true, Ordering::Release);
}

struct AaStream {
    api: Arc<Api>,
    stream: *mut c_void,
    shared: *mut Shared,
}

// SAFETY: AAudio stream handles may be stopped and closed from any thread.
unsafe impl Send for AaStream {}

impl StreamImpl for AaStream {
    fn stop(&mut self) {
        if self.stream.is_null() {
            return;
        }
        // SAFETY: stop then close; no callback runs after close.
        unsafe {
            (self.api.request_stop)(self.stream);
            (self.api.close)(self.stream);
            drop(Box::from_raw(self.shared));
        }
        self.stream = null_mut();
    }
}

fn open(config: StreamConfig, callback: Callback) -> Result<DeviceStream, BackendError> {
    let api = Arc::new(Api::load().map_err(BackendError::Unavailable)?);
    let capture = matches!(callback, Callback::Capture(_));
    let ch = config.channels.clamp(1, 8);
    let counters = Arc::new(Counters::default());
    // The callback crosses an `extern "C"` boundary on AAudio's data
    // thread: isolate it before it is stored, so a panicking closure ends
    // the stream instead of the process.
    let callback = match callback {
        Callback::Render(r) => Callback::Render(isolate_render(r, &counters)),
        Callback::Capture(c) => Callback::Capture(isolate_capture(c, &counters)),
    };
    let shared = Box::into_raw(Box::new(Shared {
        callback,
        channels: AtomicUsize::new(usize::from(ch)),
        rate: AtomicU32::new(config.rate),
        counters: counters.clone(),
        frames_done: 0,
    }));
    // SAFETY: the AAudio builder protocol; every result checked.
    unsafe {
        let mut b = null_mut();
        let r = (api.create_builder)(&raw mut b);
        if r != AAUDIO_OK {
            drop(Box::from_raw(shared));
            return Err(BackendError::Unavailable(api.text(r)));
        }
        (api.set_direction)(
            b,
            if capture {
                AAUDIO_DIRECTION_INPUT
            } else {
                AAUDIO_DIRECTION_OUTPUT
            },
        );
        (api.set_sample_rate)(b, i32::try_from(config.rate).unwrap_or(48_000));
        (api.set_channel_count)(b, i32::from(ch));
        (api.set_format)(b, AAUDIO_FORMAT_PCM_FLOAT);
        (api.set_sharing_mode)(b, AAUDIO_SHARING_MODE_SHARED);
        (api.set_performance_mode)(b, AAUDIO_PERFORMANCE_MODE_LOW_LATENCY);
        (api.set_frames_per_data_callback)(b, i32::try_from(config.block).unwrap_or(0));
        (api.set_data_callback)(b, on_data, shared.cast());
        (api.set_error_callback)(b, on_error, shared.cast());
        let mut stream = null_mut();
        let r = (api.open_stream)(b, &raw mut stream);
        (api.builder_delete)(b);
        if r != AAUDIO_OK {
            drop(Box::from_raw(shared));
            return Err(BackendError::Refused(api.text(r)));
        }
        // The device may grant another rate or channel count.
        let rate = u32::try_from((api.get_sample_rate)(stream)).unwrap_or(config.rate);
        let channels = u16::try_from((api.get_channel_count)(stream)).unwrap_or(ch);
        (*shared).rate.store(rate, Ordering::Release);
        (*shared)
            .channels
            .store(usize::from(channels), Ordering::Release);
        let burst = u32::try_from((api.get_frames_per_burst)(stream)).unwrap_or(config.block);
        let r = (api.request_start)(stream);
        if r != AAUDIO_OK {
            (api.close)(stream);
            drop(Box::from_raw(shared));
            return Err(BackendError::Refused(api.text(r)));
        }
        Ok(DeviceStream::new(
            "aaudio",
            StreamConfig {
                rate,
                channels,
                block: burst,
            },
            counters,
            Box::new(AaStream {
                api,
                stream,
                shared,
            }),
        ))
    }
}

/// The AAudio backend.
#[derive(Debug, Clone, Copy, Default)]
pub struct AAudio;

impl Backend for AAudio {
    fn name(&self) -> &'static str {
        "aaudio"
    }

    fn open_output(
        &self,
        config: StreamConfig,
        render: RenderFn,
    ) -> Result<DeviceStream, BackendError> {
        open(config, Callback::Render(render))
    }

    fn open_input(
        &self,
        config: StreamConfig,
        capture: CaptureFn,
    ) -> Result<DeviceStream, BackendError> {
        open(config, Callback::Capture(capture))
    }
}
