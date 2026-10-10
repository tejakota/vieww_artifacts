//! WASAPI: Windows' native audio API (Vista and later; the automatic
//! format conversion used here needs Windows 10).
//!
//! Shared mode, event driven: the default endpoint's `IAudioClient` is
//! initialised with our format (32-bit float, the requested rate and
//! channels — `AUTOCONVERTPCM | SRC_DEFAULT_QUALITY` has the audio engine
//! convert to the device), and the stream thread waits on the client's
//! event, then fills `buffer size − padding` frames through
//! `IAudioRenderClient` (or drains `IAudioCaptureClient` packets). Every
//! COM object is created, used and released on that one thread, which
//! initialises COM as multithreaded. The interfaces' vtables, GUIDs and
//! `WAVEFORMATEXTENSIBLE` are transcribed from the Windows SDK headers (as
//! the MinGW-w64 headers reproduce them).

use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;

use super::{
    Backend, BackendError, BlockInfo, CaptureFn, Counters, DeviceStream, RenderFn, StreamConfig,
    StreamImpl,
};

#[repr(C)]
#[derive(Clone, Copy)]
struct Guid(u32, u16, u16, [u8; 8]);

const CLSID_MM_DEVICE_ENUMERATOR: Guid = Guid(
    0xbcde_0395,
    0xe52f,
    0x467c,
    [0x8e, 0x3d, 0xc4, 0x57, 0x92, 0x91, 0x69, 0x2e],
);
const IID_IMM_DEVICE_ENUMERATOR: Guid = Guid(
    0xa956_64d2,
    0x9614,
    0x4f35,
    [0xa7, 0x46, 0xde, 0x8d, 0xb6, 0x36, 0x17, 0xe6],
);
const IID_IAUDIO_CLIENT: Guid = Guid(
    0x1cb9_ad4c,
    0xdbfa,
    0x4c32,
    [0xb1, 0x78, 0xc2, 0xf5, 0x68, 0xa7, 0x03, 0xb2],
);
const IID_IAUDIO_RENDER_CLIENT: Guid = Guid(
    0xf294_acfc,
    0x3146,
    0x4483,
    [0xa7, 0xbf, 0xad, 0xdc, 0xa7, 0xc2, 0x60, 0xe2],
);
const IID_IAUDIO_CAPTURE_CLIENT: Guid = Guid(
    0xc8ad_bd64,
    0xe71e,
    0x48a0,
    [0xa4, 0xde, 0x18, 0x5c, 0x39, 0x5c, 0xd3, 0x17],
);
const KSDATAFORMAT_SUBTYPE_IEEE_FLOAT: Guid = Guid(
    0x0000_0003,
    0x0000,
    0x0010,
    [0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71],
);

const CLSCTX_ALL: u32 = 0x17;
const COINIT_MULTITHREADED: u32 = 0;
const E_RENDER: i32 = 0;
const E_CAPTURE: i32 = 1;
const E_CONSOLE: i32 = 0;
const SHAREMODE_SHARED: i32 = 0;
const STREAMFLAGS_EVENTCALLBACK: u32 = 0x0004_0000;
const STREAMFLAGS_SRC_DEFAULT_QUALITY: u32 = 0x0800_0000;
const STREAMFLAGS_AUTOCONVERTPCM: u32 = 0x8000_0000;
const BUFFERFLAGS_SILENT: u32 = 0x2;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;
const WAIT_OBJECT_0: u32 = 0;

/// `WAVEFORMATEXTENSIBLE` (byte-packed, 40 bytes).
#[repr(C, packed(1))]
#[derive(Clone, Copy)]
struct WaveFormatExtensible {
    format_tag: u16,
    channels: u16,
    samples_per_sec: u32,
    avg_bytes_per_sec: u32,
    block_align: u16,
    bits_per_sample: u16,
    cb_size: u16,
    valid_bits_per_sample: u16,
    channel_mask: u32,
    sub_format: Guid,
}

type Hresult = i32;
type This = *mut c_void;

#[repr(C)]
struct UnknownVtbl {
    query_interface: unsafe extern "system" fn(This, *const Guid, *mut *mut c_void) -> Hresult,
    add_ref: unsafe extern "system" fn(This) -> u32,
    release: unsafe extern "system" fn(This) -> u32,
}

#[repr(C)]
struct DeviceEnumeratorVtbl {
    unknown: UnknownVtbl,
    enum_audio_endpoints: unsafe extern "system" fn(This, i32, u32, *mut *mut c_void) -> Hresult,
    get_default_audio_endpoint: unsafe extern "system" fn(This, i32, i32, *mut This) -> Hresult,
}

#[repr(C)]
struct DeviceVtbl {
    unknown: UnknownVtbl,
    activate: unsafe extern "system" fn(This, *const Guid, u32, *mut c_void, *mut This) -> Hresult,
}

#[repr(C)]
struct AudioClientVtbl {
    unknown: UnknownVtbl,
    initialize: unsafe extern "system" fn(
        This,
        i32,
        u32,
        i64,
        i64,
        *const WaveFormatExtensible,
        *const Guid,
    ) -> Hresult,
    get_buffer_size: unsafe extern "system" fn(This, *mut u32) -> Hresult,
    get_stream_latency: unsafe extern "system" fn(This, *mut i64) -> Hresult,
    get_current_padding: unsafe extern "system" fn(This, *mut u32) -> Hresult,
    is_format_supported:
        unsafe extern "system" fn(This, i32, *const c_void, *mut *mut c_void) -> Hresult,
    get_mix_format: unsafe extern "system" fn(This, *mut *mut c_void) -> Hresult,
    get_device_period: unsafe extern "system" fn(This, *mut i64, *mut i64) -> Hresult,
    start: unsafe extern "system" fn(This) -> Hresult,
    stop: unsafe extern "system" fn(This) -> Hresult,
    reset: unsafe extern "system" fn(This) -> Hresult,
    set_event_handle: unsafe extern "system" fn(This, *mut c_void) -> Hresult,
    get_service: unsafe extern "system" fn(This, *const Guid, *mut This) -> Hresult,
}

#[repr(C)]
struct RenderClientVtbl {
    unknown: UnknownVtbl,
    get_buffer: unsafe extern "system" fn(This, u32, *mut *mut u8) -> Hresult,
    release_buffer: unsafe extern "system" fn(This, u32, u32) -> Hresult,
}

#[repr(C)]
struct CaptureClientVtbl {
    unknown: UnknownVtbl,
    get_buffer: unsafe extern "system" fn(
        This,
        *mut *mut u8,
        *mut u32,
        *mut u32,
        *mut u64,
        *mut u64,
    ) -> Hresult,
    release_buffer: unsafe extern "system" fn(This, u32) -> Hresult,
    get_next_packet_size: unsafe extern "system" fn(This, *mut u32) -> Hresult,
}

#[cfg_attr(windows, link(name = "ole32"))]
unsafe extern "system" {
    fn CoInitializeEx(reserved: *mut c_void, co_init: u32) -> Hresult;
    fn CoUninitialize();
    fn CoCreateInstance(
        clsid: *const Guid,
        outer: *mut c_void,
        ctx: u32,
        iid: *const Guid,
        out: *mut This,
    ) -> Hresult;
}

#[cfg_attr(windows, link(name = "kernel32"))]
unsafe extern "system" {
    fn CreateEventW(
        attrs: *mut c_void,
        manual_reset: i32,
        initial: i32,
        name: *const u16,
    ) -> *mut c_void;
    fn WaitForSingleObject(handle: *mut c_void, ms: u32) -> u32;
    fn CloseHandle(handle: *mut c_void) -> i32;
}

/// `vtable!(ptr, Vtbl)`: the vtable of a COM pointer.
macro_rules! vt {
    ($p:expr, $t:ty) => {
        &**$p.cast::<*const $t>()
    };
}

/// A COM pointer released on drop.
struct Com(This);

impl Drop for Com {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: a live COM pointer we own one reference to.
            unsafe { (vt!(self.0, UnknownVtbl).release)(self.0) };
        }
    }
}

fn check(hr: Hresult, what: &str) -> Result<(), String> {
    if hr < 0 {
        Err(format!(
            "{what} failed (HRESULT {:#010x})",
            hr.cast_unsigned()
        ))
    } else {
        Ok(())
    }
}

fn channel_mask(ch: u16) -> u32 {
    match ch {
        1 => 0x4,   // FC
        2 => 0x3,   // FL FR
        4 => 0x33,  // FL FR BL BR
        6 => 0x3F,  // 5.1
        8 => 0x63F, // 7.1 (side)
        n => (1u32 << n.min(18)) - 1,
    }
}

enum Callback {
    Render(RenderFn),
    Capture(CaptureFn),
}

struct WasapiStream {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl StreamImpl for WasapiStream {
    fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            t.join().ok();
        }
    }
}

/// Everything set up on the stream thread.
struct Session {
    _enumerator: Com,
    _device: Com,
    client: Com,
    service: Com,
    event: *mut c_void,
    buffer_frames: u32,
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: a live client; stopping an unstarted client is harmless.
        unsafe { (vt!(self.client.0, AudioClientVtbl).stop)(self.client.0) };
        if !self.event.is_null() {
            // SAFETY: an event handle we created.
            unsafe { CloseHandle(self.event) };
        }
    }
}

unsafe fn session(config: StreamConfig, capture: bool) -> Result<Session, String> {
    // SAFETY: the WASAPI setup sequence, each pointer checked before use.
    unsafe {
        let mut e = null_mut();
        check(
            CoCreateInstance(
                &CLSID_MM_DEVICE_ENUMERATOR,
                null_mut(),
                CLSCTX_ALL,
                &IID_IMM_DEVICE_ENUMERATOR,
                &raw mut e,
            ),
            "CoCreateInstance(MMDeviceEnumerator)",
        )?;
        let enumerator = Com(e);
        let mut d = null_mut();
        check(
            (vt!(e, DeviceEnumeratorVtbl).get_default_audio_endpoint)(
                e,
                if capture { E_CAPTURE } else { E_RENDER },
                E_CONSOLE,
                &raw mut d,
            ),
            "GetDefaultAudioEndpoint",
        )?;
        let device = Com(d);
        let mut c = null_mut();
        check(
            (vt!(d, DeviceVtbl).activate)(
                d,
                &IID_IAUDIO_CLIENT,
                CLSCTX_ALL,
                null_mut(),
                &raw mut c,
            ),
            "Activate(IAudioClient)",
        )?;
        let client = Com(c);
        let ch = config.channels.clamp(1, 18);
        let fmt = WaveFormatExtensible {
            format_tag: WAVE_FORMAT_EXTENSIBLE,
            channels: ch,
            samples_per_sec: config.rate,
            avg_bytes_per_sec: config.rate * u32::from(ch) * 4,
            block_align: ch * 4,
            bits_per_sample: 32,
            cb_size: 22,
            valid_bits_per_sample: 32,
            channel_mask: channel_mask(ch),
            sub_format: KSDATAFORMAT_SUBTYPE_IEEE_FLOAT,
        };
        // Buffer: three blocks, in 100 ns units.
        let hns = i64::from(config.block.max(64)) * 3 * 10_000_000 / i64::from(config.rate.max(1));
        check(
            (vt!(c, AudioClientVtbl).initialize)(
                c,
                SHAREMODE_SHARED,
                STREAMFLAGS_EVENTCALLBACK
                    | STREAMFLAGS_AUTOCONVERTPCM
                    | STREAMFLAGS_SRC_DEFAULT_QUALITY,
                hns,
                0,
                &raw const fmt,
                std::ptr::null(),
            ),
            "IAudioClient::Initialize",
        )?;
        let event = CreateEventW(null_mut(), 0, 0, std::ptr::null());
        if event.is_null() {
            return Err("CreateEventW failed".into());
        }
        let mut s = Session {
            _enumerator: enumerator,
            _device: device,
            client,
            service: Com(null_mut()),
            event,
            buffer_frames: 0,
        };
        check(
            (vt!(c, AudioClientVtbl).set_event_handle)(c, event),
            "SetEventHandle",
        )?;
        check(
            (vt!(c, AudioClientVtbl).get_buffer_size)(c, &raw mut s.buffer_frames),
            "GetBufferSize",
        )?;
        let mut svc = null_mut();
        let iid = if capture {
            &IID_IAUDIO_CAPTURE_CLIENT
        } else {
            &IID_IAUDIO_RENDER_CLIENT
        };
        check(
            (vt!(c, AudioClientVtbl).get_service)(c, iid, &raw mut svc),
            "GetService",
        )?;
        s.service = Com(svc);
        Ok(s)
    }
}

fn open(config: StreamConfig, callback: Callback) -> Result<DeviceStream, BackendError> {
    let capture = matches!(callback, Callback::Capture(_));
    let counters = Arc::new(Counters::default());
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel::<Result<u32, String>>();
    let (c2, s2) = (counters.clone(), stop.clone());
    let ch = usize::from(config.channels.clamp(1, 18));
    let rate = config.rate;
    let thread = std::thread::Builder::new()
        .name("vieww-wasapi".into())
        .spawn(move || {
            // SAFETY: COM initialised for this thread and uninitialised at
            // its end; every object lives inside `session`.
            unsafe {
                let hr = CoInitializeEx(null_mut(), COINIT_MULTITHREADED);
                if hr < 0 {
                    tx.send(Err(format!("CoInitializeEx failed ({hr:#x})")))
                        .ok();
                    return;
                }
                let sess = match session(config, capture) {
                    Ok(s) => s,
                    Err(e) => {
                        tx.send(Err(e)).ok();
                        CoUninitialize();
                        return;
                    }
                };
                let c = sess.client.0;
                let svc = sess.service.0;
                let mut callback = callback;
                let mut done = 0u64;
                if (vt!(c, AudioClientVtbl).start)(c) < 0 {
                    tx.send(Err("IAudioClient::Start failed".into())).ok();
                    drop(sess);
                    CoUninitialize();
                    return;
                }
                tx.send(Ok(sess.buffer_frames)).ok();
                while !s2.load(Ordering::Acquire) {
                    if WaitForSingleObject(sess.event, 2_000) != WAIT_OBJECT_0 {
                        c2.xruns.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    #[expect(clippy::cast_precision_loss, reason = "stream time")]
                    let time = done as f64 / f64::from(rate);
                    match &mut callback {
                        Callback::Render(r) => {
                            let mut pad = 0u32;
                            if (vt!(c, AudioClientVtbl).get_current_padding)(c, &raw mut pad) < 0 {
                                c2.ended.store(true, Ordering::Release);
                                break;
                            }
                            let avail = sess.buffer_frames.saturating_sub(pad);
                            if avail == 0 {
                                continue;
                            }
                            let mut data = null_mut();
                            if (vt!(svc, RenderClientVtbl).get_buffer)(svc, avail, &raw mut data)
                                < 0
                                || data.is_null()
                            {
                                c2.ended.store(true, Ordering::Release);
                                break;
                            }
                            let n = avail as usize;
                            let out = std::slice::from_raw_parts_mut(data.cast::<f32>(), n * ch);
                            out.fill(0.0);
                            r(
                                out,
                                &BlockInfo {
                                    frames: n,
                                    channels: ch,
                                    rate,
                                    time,
                                },
                            );
                            (vt!(svc, RenderClientVtbl).release_buffer)(svc, avail, 0);
                            done += n as u64;
                            c2.block(n);
                        }
                        Callback::Capture(cap) => loop {
                            let mut packet = 0u32;
                            if (vt!(svc, CaptureClientVtbl).get_next_packet_size)(
                                svc,
                                &raw mut packet,
                            ) < 0
                            {
                                c2.ended.store(true, Ordering::Release);
                                break;
                            }
                            if packet == 0 {
                                break;
                            }
                            let (mut data, mut frames, mut flags) = (null_mut(), 0u32, 0u32);
                            if (vt!(svc, CaptureClientVtbl).get_buffer)(
                                svc,
                                &raw mut data,
                                &raw mut frames,
                                &raw mut flags,
                                null_mut(),
                                null_mut(),
                            ) < 0
                            {
                                c2.ended.store(true, Ordering::Release);
                                break;
                            }
                            let n = frames as usize;
                            if flags & BUFFERFLAGS_SILENT != 0 || data.is_null() {
                                let zeros = [0.0f32; 4_096];
                                let mut left = n * ch;
                                while left > 0 {
                                    let k = left.min(zeros.len() - zeros.len() % ch);
                                    cap(
                                        &zeros[..k],
                                        &BlockInfo {
                                            frames: k / ch,
                                            channels: ch,
                                            rate,
                                            time,
                                        },
                                    );
                                    left -= k;
                                }
                            } else {
                                let input = std::slice::from_raw_parts(data.cast::<f32>(), n * ch);
                                cap(
                                    input,
                                    &BlockInfo {
                                        frames: n,
                                        channels: ch,
                                        rate,
                                        time,
                                    },
                                );
                            }
                            (vt!(svc, CaptureClientVtbl).release_buffer)(svc, frames);
                            done += n as u64;
                            c2.block(n);
                        },
                    }
                }
                drop(sess);
                CoUninitialize();
            }
        })
        .map_err(|e| BackendError::Refused(e.to_string()))?;
    match rx.recv() {
        Ok(Ok(buffer)) => Ok(DeviceStream::new(
            "wasapi",
            StreamConfig {
                rate: config.rate,
                channels: config.channels.clamp(1, 18),
                block: buffer / 3,
            },
            counters,
            Box::new(WasapiStream {
                stop,
                thread: Some(thread),
            }),
        )),
        Ok(Err(e)) => {
            thread.join().ok();
            // No endpoint at all (a server with no audio device) is
            // "unavailable"; anything later is a refusal.
            if e.contains("GetDefaultAudioEndpoint") || e.contains("CoCreateInstance") {
                Err(BackendError::Unavailable(e))
            } else {
                Err(BackendError::Refused(e))
            }
        }
        Err(_) => Err(BackendError::Refused("the WASAPI thread ended".into())),
    }
}

/// The WASAPI backend.
#[derive(Debug, Clone, Copy, Default)]
pub struct Wasapi;

impl Backend for Wasapi {
    fn name(&self) -> &'static str {
        "wasapi"
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

    #[test]
    fn the_format_struct_has_the_sdk_layout() {
        assert_eq!(size_of::<WaveFormatExtensible>(), 40);
        assert_eq!(std::mem::offset_of!(WaveFormatExtensible, cb_size), 16);
        assert_eq!(std::mem::offset_of!(WaveFormatExtensible, channel_mask), 20);
        assert_eq!(std::mem::offset_of!(WaveFormatExtensible, sub_format), 24);
        assert_eq!(size_of::<Guid>(), 16);
        assert_eq!(channel_mask(2), 3);
        assert_eq!(channel_mask(6), 0x3F);
    }

    #[test]
    #[cfg(windows)]
    fn plays_on_the_default_endpoint_or_refuses_cleanly() {
        // CI runners usually have no audio endpoint: then the refusal is
        // the check.
        match Wasapi.open_output(StreamConfig::default(), Box::new(|b, _| b.fill(0.0))) {
            Ok(s) => assert!(s.wait_for(5, std::time::Duration::from_secs(10))),
            Err(e) => assert!(!e.to_string().is_empty()),
        }
    }
}
