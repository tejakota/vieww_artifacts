//! PulseAudio — and every PipeWire desktop, through `pipewire-pulse`.
//!
//! The blocking "simple" API (`libpulse-simple`, loaded at run time): a
//! stream thread renders a block and writes it, and the write blocks until
//! the server has room, so the server's clock paces the callback. The
//! server's buffer is sized from [`StreamConfig::block`] (target length
//! four blocks, minimum request one block), which is what sets latency.
//! Samples are `float32le` at the requested rate and channel count; the
//! server converts to the device.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use super::dl::Library;
use super::{
    Backend, BackendError, BlockInfo, CaptureFn, Counters, DeviceStream, RenderFn, StreamConfig,
    StreamImpl,
};

const PA_STREAM_PLAYBACK: c_int = 1;
const PA_STREAM_RECORD: c_int = 2;
const PA_SAMPLE_FLOAT32LE: c_int = 5;

#[repr(C)]
struct SampleSpec {
    format: c_int,
    rate: u32,
    channels: u8,
}

#[repr(C)]
struct BufferAttr {
    maxlength: u32,
    tlength: u32,
    prebuf: u32,
    minreq: u32,
    fragsize: u32,
}

struct Api {
    _lib: Library,
    new: unsafe extern "C" fn(
        *const c_char,
        *const c_char,
        c_int,
        *const c_char,
        *const c_char,
        *const SampleSpec,
        *const c_void,
        *const BufferAttr,
        *mut c_int,
    ) -> *mut c_void,
    write: unsafe extern "C" fn(*mut c_void, *const c_void, usize, *mut c_int) -> c_int,
    read: unsafe extern "C" fn(*mut c_void, *mut c_void, usize, *mut c_int) -> c_int,
    flush: unsafe extern "C" fn(*mut c_void, *mut c_int) -> c_int,
    free: unsafe extern "C" fn(*mut c_void),
    strerror: Option<unsafe extern "C" fn(c_int) -> *const c_char>,
}

impl Api {
    fn load() -> Result<Self, String> {
        let lib = Library::open(&["libpulse-simple.so.0", "libpulse-simple.so"])?;
        let errlib = Library::open(&["libpulse.so.0", "libpulse.so"]).ok();
        // SAFETY: the types are libpulse-simple's documented signatures.
        unsafe {
            let strerror = errlib.as_ref().and_then(|l| l.get(c"pa_strerror").ok());
            Ok(Self {
                new: lib.get(c"pa_simple_new")?,
                write: lib.get(c"pa_simple_write")?,
                read: lib.get(c"pa_simple_read")?,
                flush: lib.get(c"pa_simple_flush")?,
                free: lib.get(c"pa_simple_free")?,
                strerror,
                _lib: lib,
            })
        }
    }

    fn error(&self, code: c_int) -> String {
        match self.strerror {
            Some(f) => {
                // SAFETY: pa_strerror returns a static string.
                let p = unsafe { f(code) };
                if p.is_null() {
                    format!("error {code}")
                } else {
                    // SAFETY: a static NUL-terminated string.
                    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
                }
            }
            None => format!("error {code}"),
        }
    }
}

/// The PulseAudio backend.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pulse;

struct Handle(*mut c_void);
// SAFETY: a pa_simple is used by one thread at a time (the stream thread;
// `stop` only touches it after joining that thread).
unsafe impl Send for Handle {}

struct PulseStream {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl StreamImpl for PulseStream {
    fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            t.join().ok();
        }
    }
}

enum Callback {
    Render(RenderFn),
    Capture(CaptureFn),
}

fn open(config: StreamConfig, callback: Callback) -> Result<DeviceStream, BackendError> {
    let api = Arc::new(Api::load().map_err(BackendError::Unavailable)?);
    let ch = config.channels.clamp(1, 32);
    let spec = SampleSpec {
        format: PA_SAMPLE_FLOAT32LE,
        rate: config.rate,
        channels: u8::try_from(ch).unwrap_or(2),
    };
    let block_bytes = config.block.max(16) * u32::from(ch) * 4;
    let capture = matches!(callback, Callback::Capture(_));
    let attr = BufferAttr {
        maxlength: u32::MAX,
        tlength: if capture { u32::MAX } else { block_bytes * 4 },
        prebuf: if capture { u32::MAX } else { block_bytes * 2 },
        minreq: if capture { u32::MAX } else { block_bytes },
        fragsize: if capture { block_bytes } else { u32::MAX },
    };
    let name = CString::new("vieww").unwrap_or_default();
    let stream_name =
        CString::new(if capture { "capture" } else { "playback" }).unwrap_or_default();
    let mut err = 0;
    // SAFETY: valid strings, spec and attributes; null server/device mean
    // the defaults.
    let s = unsafe {
        (api.new)(
            null(),
            name.as_ptr(),
            if capture {
                PA_STREAM_RECORD
            } else {
                PA_STREAM_PLAYBACK
            },
            null(),
            stream_name.as_ptr(),
            &raw const spec,
            null(),
            &raw const attr,
            &raw mut err,
        )
    };
    if s.is_null() {
        return Err(BackendError::Unavailable(api.error(err)));
    }
    let counters = Arc::new(Counters::default());
    let stop = Arc::new(AtomicBool::new(false));
    let (c2, s2, a2, handle) = (counters.clone(), stop.clone(), api.clone(), Handle(s));
    let frames = config.block.max(16) as usize;
    let rate = config.rate;
    let thread = std::thread::Builder::new()
        .name("vieww-pulse".into())
        .spawn(move || {
            let h = handle;
            let chn = usize::from(ch);
            let mut buf = vec![0.0f32; frames * chn];
            let mut callback = callback;
            let mut done = 0u64;
            while !s2.load(Ordering::Acquire) {
                #[expect(clippy::cast_precision_loss, reason = "stream time")]
                let info = BlockInfo {
                    frames,
                    channels: chn,
                    rate,
                    time: done as f64 / f64::from(rate),
                };
                let mut err = 0;
                let ok = match &mut callback {
                    Callback::Render(r) => {
                        buf.fill(0.0);
                        r(&mut buf, &info);
                        // SAFETY: a live stream and a buffer of that many bytes.
                        unsafe {
                            (a2.write)(h.0, buf.as_ptr().cast(), buf.len() * 4, &raw mut err) == 0
                        }
                    }
                    Callback::Capture(cap) => {
                        // SAFETY: as above.
                        let ok = unsafe {
                            (a2.read)(h.0, buf.as_mut_ptr().cast(), buf.len() * 4, &raw mut err)
                                == 0
                        };
                        if ok {
                            cap(&buf, &info);
                        }
                        ok
                    }
                };
                if !ok {
                    c2.ended.store(true, Ordering::Release);
                    break;
                }
                done += frames as u64;
                c2.block(frames);
            }
            // SAFETY: the stream thread owns the handle; free it once.
            unsafe {
                let mut e = 0;
                (a2.flush)(h.0, &raw mut e);
                (a2.free)(h.0);
            }
        })
        .map_err(|e| BackendError::Refused(e.to_string()))?;
    let _ = null_mut::<c_void>();
    Ok(DeviceStream::new(
        "pulse",
        StreamConfig {
            rate: config.rate,
            channels: ch,
            block: config.block.max(16),
        },
        counters,
        Box::new(PulseStream {
            stop,
            thread: Some(thread),
        }),
    ))
}

impl Backend for Pulse {
    fn name(&self) -> &'static str {
        "pulse"
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    // Against a real server when one is running (CI starts PulseAudio with
    // a null sink), else the refusal.
    #[test]
    fn plays_at_the_servers_pace_or_refuses_cleanly() {
        let cfg = StreamConfig {
            rate: 48_000,
            channels: 2,
            block: 480,
        };
        match Pulse.open_output(
            cfg,
            Box::new(|buf, _| {
                for (i, v) in buf.iter_mut().enumerate() {
                    *v = (i as f32 * 0.01).sin() * 0.1;
                }
            }),
        ) {
            Ok(stream) => {
                assert_eq!(stream.config(), cfg);
                // Let the initial buffer fill, then measure the pace: the
                // server consumes 48 000 frames a second, so ~100 blocks.
                assert!(
                    stream.wait_for(20, Duration::from_secs(10)),
                    "{:?}",
                    stream.stats()
                );
                let (f0, t0) = (stream.stats().frames, Instant::now());
                std::thread::sleep(Duration::from_millis(1_000));
                let rate = (stream.stats().frames - f0) as f64 / t0.elapsed().as_secs_f64();
                assert!(
                    (rate - 48_000.0).abs() < 4_800.0,
                    "consumed {rate} frames/s"
                );
                assert!(!stream.stats().ended);
            }
            Err(e) => {
                assert!(
                    !super::super::expected("pulse"),
                    "pulse was expected to be running: {e}"
                );
                assert!(matches!(e, BackendError::Unavailable(_)), "{e}");
            }
        }
    }

    #[test]
    fn records_or_refuses_cleanly() {
        match Pulse.open_input(
            StreamConfig {
                rate: 16_000,
                channels: 1,
                block: 160,
            },
            Box::new(|buf, info| assert_eq!(buf.len(), info.frames)),
        ) {
            Ok(stream) => assert!(stream.wait_for(5, Duration::from_secs(10))),
            Err(e) => {
                assert!(
                    !super::super::expected("pulse"),
                    "pulse was expected to be running: {e}"
                );
                assert!(matches!(e, BackendError::Unavailable(_)), "{e}");
            }
        }
    }
}
