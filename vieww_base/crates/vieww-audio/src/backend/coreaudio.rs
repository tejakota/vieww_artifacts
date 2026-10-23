//! Core Audio: macOS's and iOS's audio system, through Audio Units.
//!
//! On iOS both directions use the `RemoteIO` unit — there is no device to
//! choose, and the HAL property API does not exist. Bus 0 is the speaker,
//! bus 1 the microphone, exactly as on the macOS `HALOutput` unit, so the
//! setup below is the same apart from the device lookup. Capture on iOS also
//! needs the app's `AVAudioSession` in a record category and the user's
//! microphone permission; without them the unit refuses to start and the
//! backend reports [`BackendError::Refused`].
//!
//! Playback uses the `DefaultOutput` unit (it follows the user's chosen
//! output device), capture the `HALOutput` unit with input enabled and the
//! default input device selected. The stream format is set to packed,
//! interleaved 32-bit float at the requested rate and channels — the unit
//! converts to the hardware — and the render (or input) callback runs on
//! Core Audio's real-time I/O thread. Playback renders straight into the
//! unit's buffer; capture pulls with `AudioUnitRender` into a buffer
//! preallocated for the unit's maximum slice.

use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::Arc;

use super::{
    isolate_capture, isolate_render, Backend, BackendError, BlockInfo, CaptureFn, Counters,
    DeviceInfo, DeviceStream, RenderFn, StreamConfig, StreamImpl,
};

type OsStatus = i32;
type AudioUnit = *mut c_void;

const fn fourcc(s: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*s)
}

const K_AUDIO_UNIT_TYPE_OUTPUT: u32 = fourcc(b"auou");
const K_AUDIO_UNIT_SUBTYPE_DEFAULT_OUTPUT: u32 = fourcc(b"def ");
const K_AUDIO_UNIT_SUBTYPE_HAL_OUTPUT: u32 = fourcc(b"ahal");
const K_AUDIO_UNIT_SUBTYPE_REMOTE_IO: u32 = fourcc(b"rioc");
const K_AUDIO_UNIT_MANUFACTURER_APPLE: u32 = fourcc(b"appl");
const K_AUDIO_FORMAT_LINEAR_PCM: u32 = fourcc(b"lpcm");
const K_AUDIO_FORMAT_FLAG_IS_FLOAT: u32 = 1;
const K_AUDIO_FORMAT_FLAG_IS_PACKED: u32 = 8;
const K_AUDIO_UNIT_PROPERTY_STREAM_FORMAT: u32 = 8;
const K_AUDIO_UNIT_PROPERTY_MAXIMUM_FRAMES_PER_SLICE: u32 = 14;
const K_AUDIO_UNIT_PROPERTY_SET_RENDER_CALLBACK: u32 = 23;
#[cfg(not(target_os = "ios"))]
const K_AUDIO_OUTPUT_UNIT_PROPERTY_CURRENT_DEVICE: u32 = 2000;
const K_AUDIO_OUTPUT_UNIT_PROPERTY_ENABLE_IO: u32 = 2003;
const K_AUDIO_OUTPUT_UNIT_PROPERTY_SET_INPUT_CALLBACK: u32 = 2005;
const K_AUDIO_UNIT_SCOPE_GLOBAL: u32 = 0;
const K_AUDIO_UNIT_SCOPE_INPUT: u32 = 1;
const K_AUDIO_UNIT_SCOPE_OUTPUT: u32 = 2;
#[cfg(not(target_os = "ios"))]
const K_AUDIO_OBJECT_SYSTEM_OBJECT: u32 = 1;
#[cfg(not(target_os = "ios"))]
const K_AUDIO_HARDWARE_PROPERTY_DEFAULT_INPUT_DEVICE: u32 = fourcc(b"dIn ");
#[cfg(not(target_os = "ios"))]
const K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL: u32 = fourcc(b"glob");
const MAX_SLICE: u32 = 4_096;

#[repr(C)]
struct AudioComponentDescription {
    component_type: u32,
    component_sub_type: u32,
    component_manufacturer: u32,
    component_flags: u32,
    component_flags_mask: u32,
}

#[repr(C)]
struct AudioStreamBasicDescription {
    sample_rate: f64,
    format_id: u32,
    format_flags: u32,
    bytes_per_packet: u32,
    frames_per_packet: u32,
    bytes_per_frame: u32,
    channels_per_frame: u32,
    bits_per_channel: u32,
    reserved: u32,
}

#[repr(C)]
struct AudioBuffer {
    number_channels: u32,
    data_byte_size: u32,
    data: *mut c_void,
}

#[repr(C)]
struct AudioBufferList {
    number_buffers: u32,
    buffers: [AudioBuffer; 1],
}

#[cfg(not(target_os = "ios"))]
#[repr(C)]
struct AudioObjectPropertyAddress {
    selector: u32,
    scope: u32,
    element: u32,
}

type RenderProc = unsafe extern "C" fn(
    *mut c_void,
    *mut u32,
    *const c_void,
    u32,
    u32,
    *mut AudioBufferList,
) -> OsStatus;

#[repr(C)]
struct AuRenderCallbackStruct {
    input_proc: RenderProc,
    input_proc_ref_con: *mut c_void,
}

#[cfg_attr(
    any(target_os = "macos", target_os = "ios"),
    link(name = "AudioToolbox", kind = "framework")
)]
unsafe extern "C" {
    fn AudioComponentFindNext(
        component: *mut c_void,
        desc: *const AudioComponentDescription,
    ) -> *mut c_void;
    fn AudioComponentInstanceNew(component: *mut c_void, out: *mut AudioUnit) -> OsStatus;
    fn AudioComponentInstanceDispose(unit: AudioUnit) -> OsStatus;
    fn AudioUnitSetProperty(
        unit: AudioUnit,
        id: u32,
        scope: u32,
        element: u32,
        data: *const c_void,
        size: u32,
    ) -> OsStatus;
    fn AudioUnitInitialize(unit: AudioUnit) -> OsStatus;
    fn AudioUnitUninitialize(unit: AudioUnit) -> OsStatus;
    fn AudioOutputUnitStart(unit: AudioUnit) -> OsStatus;
    fn AudioOutputUnitStop(unit: AudioUnit) -> OsStatus;
    fn AudioUnitRender(
        unit: AudioUnit,
        flags: *mut u32,
        timestamp: *const c_void,
        bus: u32,
        frames: u32,
        data: *mut AudioBufferList,
    ) -> OsStatus;
}

#[cfg(not(target_os = "ios"))]
#[cfg_attr(target_os = "macos", link(name = "CoreAudio", kind = "framework"))]
unsafe extern "C" {
    fn AudioObjectGetPropertyData(
        object: u32,
        address: *const AudioObjectPropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
        data: *mut c_void,
    ) -> OsStatus;
}

enum Callback {
    Render(RenderFn),
    Capture(CaptureFn),
}

struct Shared {
    callback: Callback,
    unit: AudioUnit,
    channels: usize,
    rate: u32,
    counters: Arc<Counters>,
    frames_done: u64,
    scratch: Vec<f32>,
}

unsafe extern "C" fn render(
    refcon: *mut c_void,
    _flags: *mut u32,
    _ts: *const c_void,
    _bus: u32,
    frames: u32,
    data: *mut AudioBufferList,
) -> OsStatus {
    // SAFETY: `refcon` is the boxed `Shared`, alive until the unit is
    // disposed; Core Audio calls one callback at a time.
    let s = unsafe { &mut *refcon.cast::<Shared>() };
    if data.is_null() {
        return 0;
    }
    // SAFETY: an interleaved stream has one buffer of `frames` frames.
    let b = unsafe { &mut (*data).buffers[0] };
    let n = frames as usize;
    let want = n * s.channels;
    let bytes = b.data_byte_size as usize; // a byte count; widening on every target here
    if b.data.is_null() {
        return 0;
    }
    if bytes < want * 4 {
        // An undersized buffer cannot be rendered into, but returning
        // without touching it hands the speaker whatever was in it — a
        // garbage burst. Zero what there is room for and return cleanly.
        // SAFETY: `data` is non-null and owns `data_byte_size` bytes.
        unsafe { std::ptr::write_bytes(b.data.cast::<u8>(), 0, bytes) };
        return 0;
    }
    // SAFETY: as above.
    let out = unsafe { std::slice::from_raw_parts_mut(b.data.cast::<f32>(), want) };
    out.fill(0.0);
    #[expect(clippy::cast_precision_loss, reason = "stream time")]
    let info = BlockInfo {
        frames: n,
        channels: s.channels,
        rate: s.rate,
        time: s.frames_done as f64 / f64::from(s.rate),
    };
    if let Callback::Render(r) = &mut s.callback {
        r(out, &info);
    }
    s.frames_done += n as u64;
    s.counters.block(n);
    0
}

unsafe extern "C" fn input(
    refcon: *mut c_void,
    flags: *mut u32,
    ts: *const c_void,
    bus: u32,
    frames: u32,
    _data: *mut AudioBufferList,
) -> OsStatus {
    // SAFETY: as in `render`.
    let s = unsafe { &mut *refcon.cast::<Shared>() };
    let n = (frames.min(MAX_SLICE)) as usize;
    let bytes = n * s.channels * 4;
    let mut list = AudioBufferList {
        number_buffers: 1,
        buffers: [AudioBuffer {
            number_channels: u32::try_from(s.channels).unwrap_or(1),
            data_byte_size: u32::try_from(bytes).unwrap_or(0),
            data: s.scratch.as_mut_ptr().cast(),
        }],
    };
    // SAFETY: pulling the captured frames into our preallocated buffer.
    let st =
        unsafe { AudioUnitRender(s.unit, flags, ts, bus, frames.min(MAX_SLICE), &raw mut list) };
    if st != 0 {
        s.counters
            .xruns
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        return st;
    }
    #[expect(clippy::cast_precision_loss, reason = "stream time")]
    let info = BlockInfo {
        frames: n,
        channels: s.channels,
        rate: s.rate,
        time: s.frames_done as f64 / f64::from(s.rate),
    };
    if let Callback::Capture(c) = &mut s.callback {
        c(&s.scratch[..n * s.channels], &info);
    }
    s.frames_done += n as u64;
    s.counters.block(n);
    0
}

struct CaStream {
    unit: AudioUnit,
    shared: *mut Shared,
}

// SAFETY: Audio Unit handles may be stopped and disposed from any thread.
unsafe impl Send for CaStream {}

impl StreamImpl for CaStream {
    fn stop(&mut self) {
        if self.unit.is_null() {
            return;
        }
        // SAFETY: stop, uninitialise and dispose the unit we created; no
        // callback runs after dispose, so the shared state can go.
        unsafe {
            AudioOutputUnitStop(self.unit);
            AudioUnitUninitialize(self.unit);
            AudioComponentInstanceDispose(self.unit);
            drop(Box::from_raw(self.shared));
        }
        self.unit = null_mut();
    }
}

fn status(st: OsStatus, what: &str) -> Result<(), String> {
    if st == 0 {
        Ok(())
    } else {
        Err(format!("{what} failed (OSStatus {st})"))
    }
}

unsafe fn set<T>(
    unit: AudioUnit,
    id: u32,
    scope: u32,
    element: u32,
    value: &T,
    what: &str,
) -> Result<(), String> {
    // SAFETY: a live unit and a property value of the property's type.
    status(
        unsafe {
            AudioUnitSetProperty(
                unit,
                id,
                scope,
                element,
                std::ptr::from_ref(value).cast(),
                u32::try_from(size_of::<T>()).unwrap_or(0),
            )
        },
        what,
    )
}

/// The output unit for a direction on this platform.
const fn subtype(capture: bool) -> u32 {
    if cfg!(target_os = "ios") {
        K_AUDIO_UNIT_SUBTYPE_REMOTE_IO
    } else if capture {
        K_AUDIO_UNIT_SUBTYPE_HAL_OUTPUT
    } else {
        K_AUDIO_UNIT_SUBTYPE_DEFAULT_OUTPUT
    }
}

/// macOS: point a `HALOutput` unit at the system's default input device.
#[cfg(not(target_os = "ios"))]
unsafe fn select_default_input(unit: AudioUnit) -> Result<(), String> {
    let addr = AudioObjectPropertyAddress {
        selector: K_AUDIO_HARDWARE_PROPERTY_DEFAULT_INPUT_DEVICE,
        scope: K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
        element: 0,
    };
    let mut device: u32 = 0;
    let mut size = 4u32;
    // SAFETY: reading a `u32` property of the system object.
    status(
        unsafe {
            AudioObjectGetPropertyData(
                K_AUDIO_OBJECT_SYSTEM_OBJECT,
                &raw const addr,
                0,
                std::ptr::null(),
                &raw mut size,
                (&raw mut device).cast(),
            )
        },
        "default input device",
    )?;
    if device == 0 {
        return Err("no default input device".into());
    }
    // SAFETY: a live unit; `CurrentDevice` takes an `AudioObjectID`.
    unsafe {
        set(
            unit,
            K_AUDIO_OUTPUT_UNIT_PROPERTY_CURRENT_DEVICE,
            K_AUDIO_UNIT_SCOPE_GLOBAL,
            0,
            &device,
            "CurrentDevice",
        )
    }
}

/// macOS device discovery and timing, through the HAL's property API.
#[cfg(target_os = "macos")]
mod hal {
    use super::{fourcc, K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL, K_AUDIO_OBJECT_SYSTEM_OBJECT};
    use super::{status, AudioObjectGetPropertyData, AudioObjectPropertyAddress, DeviceInfo};
    use std::ffi::{c_char, c_void};

    const DEVICES: u32 = fourcc(b"dev#");
    const DEFAULT_OUTPUT: u32 = fourcc(b"dOut");
    const NAME: u32 = fourcc(b"lnam");
    const UID: u32 = fourcc(b"uid ");
    const STREAM_CONFIGURATION: u32 = fourcc(b"slay");
    const LATENCY: u32 = fourcc(b"ltnc");
    const SAFETY_OFFSET: u32 = fourcc(b"saft");
    const BUFFER_FRAME_SIZE: u32 = fourcc(b"fsiz");
    const NOMINAL_SAMPLE_RATE: u32 = fourcc(b"nsrt");
    const SCOPE_OUTPUT: u32 = fourcc(b"outp");
    const SCOPE_INPUT: u32 = fourcc(b"inpt");
    const UTF8: u32 = 0x0800_0100;

    #[link(name = "CoreAudio", kind = "framework")]
    unsafe extern "C" {
        fn AudioObjectGetPropertyDataSize(
            object: u32,
            address: *const AudioObjectPropertyAddress,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
        ) -> i32;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringGetCString(s: *const c_void, buf: *mut c_char, len: isize, encoding: u32) -> u8;
        fn CFRelease(p: *const c_void);
    }

    fn addr(selector: u32, scope: u32) -> AudioObjectPropertyAddress {
        AudioObjectPropertyAddress {
            selector,
            scope,
            element: 0,
        }
    }

    /// A fixed-size property.
    fn get<T: Copy>(object: u32, selector: u32, scope: u32, zero: T) -> Option<T> {
        let a = addr(selector, scope);
        let mut v = zero;
        let mut size = u32::try_from(size_of::<T>()).ok()?;
        // SAFETY: a valid address and an out-buffer of `size` bytes.
        let st = unsafe {
            AudioObjectGetPropertyData(
                object,
                &raw const a,
                0,
                std::ptr::null(),
                &raw mut size,
                (&raw mut v).cast(),
            )
        };
        status(st, "property").ok().map(|()| v)
    }

    /// A variable-size property, as bytes.
    fn get_bytes(object: u32, selector: u32, scope: u32) -> Option<Vec<u8>> {
        let a = addr(selector, scope);
        let mut size = 0u32;
        // SAFETY: a valid address and size out-pointer.
        let st = unsafe {
            AudioObjectGetPropertyDataSize(object, &raw const a, 0, std::ptr::null(), &raw mut size)
        };
        status(st, "property size").ok()?;
        let mut buf = vec![0u8; size as usize];
        // SAFETY: a buffer of `size` bytes.
        let st = unsafe {
            AudioObjectGetPropertyData(
                object,
                &raw const a,
                0,
                std::ptr::null(),
                &raw mut size,
                buf.as_mut_ptr().cast(),
            )
        };
        status(st, "property").ok()?;
        buf.truncate(size as usize);
        Some(buf)
    }

    /// A `CFStringRef` property, as a Rust string (the reference released).
    fn get_string(object: u32, selector: u32) -> Option<String> {
        let cf: *const c_void = get(
            object,
            selector,
            K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
            std::ptr::null(),
        )?;
        if cf.is_null() {
            return None;
        }
        let mut buf = [0 as c_char; 512];
        // SAFETY: a live CFString we own (released below) and a buffer of
        // the stated length.
        let ok = unsafe {
            let ok = CFStringGetCString(cf, buf.as_mut_ptr(), 512, UTF8);
            CFRelease(cf);
            ok
        };
        if ok == 0 {
            return None;
        }
        // SAFETY: NUL-terminated by CFStringGetCString.
        Some(
            unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }
                .to_string_lossy()
                .into_owned(),
        )
    }

    /// Channels in a scope: the sum over the `AudioBufferList` the stream
    /// configuration property returns (u32 count, then {u32 channels,
    /// u32 bytes, pointer} per buffer).
    fn channels(device: u32, scope: u32) -> u16 {
        let Some(b) = get_bytes(device, STREAM_CONFIGURATION, scope) else {
            return 0;
        };
        let word = |at: usize| {
            b.get(at..at + 4)
                .map_or(0, |w| u32::from_ne_bytes([w[0], w[1], w[2], w[3]]))
        };
        let n = word(0) as usize;
        let stride = 8 + size_of::<usize>();
        let first = 8; // the count, padded to the buffers' pointer alignment
        let total: u32 = (0..n).map(|i| word(first + i * stride)).sum();
        u16::try_from(total).unwrap_or(u16::MAX)
    }

    fn ids() -> Vec<u32> {
        get_bytes(
            K_AUDIO_OBJECT_SYSTEM_OBJECT,
            DEVICES,
            K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
        )
        .map(|b| {
            b.as_chunks::<4>()
                .0
                .iter()
                .map(|w| u32::from_ne_bytes(*w))
                .collect()
        })
        .unwrap_or_default()
    }

    pub(super) fn default_output() -> Option<u32> {
        get(
            K_AUDIO_OBJECT_SYSTEM_OBJECT,
            DEFAULT_OUTPUT,
            K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
            0u32,
        )
        .filter(|d| *d != 0)
    }

    pub(super) fn devices() -> Vec<DeviceInfo> {
        let default = default_output().unwrap_or(0);
        ids()
            .into_iter()
            .filter_map(|d| {
                let outputs = channels(d, SCOPE_OUTPUT);
                if outputs == 0 {
                    return None;
                }
                let uid = get_string(d, UID)?;
                Some(DeviceInfo {
                    name: get_string(d, NAME).unwrap_or_else(|| uid.clone()),
                    id: uid,
                    outputs: Some(outputs),
                    inputs: Some(channels(d, SCOPE_INPUT)),
                    is_default: d == default,
                })
            })
            .collect()
    }

    /// The default output's UID.
    pub(super) fn default_uid() -> Option<String> {
        default_output().and_then(|d| get_string(d, UID))
    }

    /// The device whose UID is `uid`.
    pub(super) fn by_uid(uid: &str) -> Option<u32> {
        ids()
            .into_iter()
            .find(|&d| get_string(d, UID).as_deref() == Some(uid))
    }

    /// Output latency in nanoseconds: the device's own latency, its
    /// safety offset and its buffer, at its nominal rate.
    pub(super) fn latency_ns(device: u32) -> Option<u64> {
        let frames = u64::from(get(device, LATENCY, SCOPE_OUTPUT, 0u32).unwrap_or(0))
            + u64::from(get(device, SAFETY_OFFSET, SCOPE_OUTPUT, 0u32).unwrap_or(0))
            + u64::from(get(
                device,
                BUFFER_FRAME_SIZE,
                K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
                0u32,
            )?);
        let rate = get(
            device,
            NOMINAL_SAMPLE_RATE,
            K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
            0f64,
        )?;
        (rate > 0.0).then(|| {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_precision_loss,
                reason = "a latency"
            )]
            let ns = (frames as f64 / rate * 1e9) as u64;
            ns
        })
    }
}

fn open(
    config: StreamConfig,
    callback: Callback,
    device: Option<u32>,
) -> Result<DeviceStream, BackendError> {
    let capture = matches!(callback, Callback::Capture(_));
    let ch = config.channels.clamp(1, 32);
    // A named output device needs the HAL unit (the default-output unit
    // follows the system default and cannot be pointed elsewhere).
    let sub = if device.is_some() && !capture && !cfg!(target_os = "ios") {
        K_AUDIO_UNIT_SUBTYPE_HAL_OUTPUT
    } else {
        subtype(capture)
    };
    let desc = AudioComponentDescription {
        component_type: K_AUDIO_UNIT_TYPE_OUTPUT,
        component_sub_type: sub,
        component_manufacturer: K_AUDIO_UNIT_MANUFACTURER_APPLE,
        component_flags: 0,
        component_flags_mask: 0,
    };
    // SAFETY: the Audio Unit setup sequence; every status is checked and
    // the unit disposed on failure.
    unsafe {
        let comp = AudioComponentFindNext(null_mut(), &raw const desc);
        if comp.is_null() {
            return Err(BackendError::Unavailable("no output Audio Unit".into()));
        }
        let mut unit = null_mut();
        status(
            AudioComponentInstanceNew(comp, &raw mut unit),
            "AudioComponentInstanceNew",
        )
        .map_err(BackendError::Unavailable)?;
        let counters = Arc::new(Counters::default());
        // The callback crosses an `extern "C"` boundary on Core Audio's
        // realtime I/O thread: isolate it before it is stored, so a
        // panicking closure ends the stream instead of the process.
        let callback = match callback {
            Callback::Render(r) => Callback::Render(isolate_render(r, &counters)),
            Callback::Capture(c) => Callback::Capture(isolate_capture(c, &counters)),
        };
        let shared = Box::into_raw(Box::new(Shared {
            callback,
            unit,
            channels: usize::from(ch),
            rate: config.rate,
            counters: counters.clone(),
            frames_done: 0,
            scratch: vec![0.0; MAX_SLICE as usize * usize::from(ch)],
        }));
        let fmt = AudioStreamBasicDescription {
            sample_rate: f64::from(config.rate),
            format_id: K_AUDIO_FORMAT_LINEAR_PCM,
            format_flags: K_AUDIO_FORMAT_FLAG_IS_FLOAT | K_AUDIO_FORMAT_FLAG_IS_PACKED,
            bytes_per_packet: u32::from(ch) * 4,
            frames_per_packet: 1,
            bytes_per_frame: u32::from(ch) * 4,
            channels_per_frame: u32::from(ch),
            bits_per_channel: 32,
            reserved: 0,
        };
        let setup = || -> Result<(), String> {
            if capture {
                let on: u32 = 1;
                let off: u32 = 0;
                set(
                    unit,
                    K_AUDIO_OUTPUT_UNIT_PROPERTY_ENABLE_IO,
                    K_AUDIO_UNIT_SCOPE_INPUT,
                    1,
                    &on,
                    "EnableIO(input)",
                )?;
                set(
                    unit,
                    K_AUDIO_OUTPUT_UNIT_PROPERTY_ENABLE_IO,
                    K_AUDIO_UNIT_SCOPE_OUTPUT,
                    0,
                    &off,
                    "EnableIO(output)",
                )?;
                #[cfg(not(target_os = "ios"))]
                select_default_input(unit)?;
                set(
                    unit,
                    K_AUDIO_UNIT_PROPERTY_STREAM_FORMAT,
                    K_AUDIO_UNIT_SCOPE_OUTPUT,
                    1,
                    &fmt,
                    "StreamFormat",
                )?;
                set(
                    unit,
                    K_AUDIO_UNIT_PROPERTY_MAXIMUM_FRAMES_PER_SLICE,
                    K_AUDIO_UNIT_SCOPE_GLOBAL,
                    0,
                    &MAX_SLICE,
                    "MaximumFramesPerSlice",
                )?;
                let cb = AuRenderCallbackStruct {
                    input_proc: input,
                    input_proc_ref_con: shared.cast(),
                };
                set(
                    unit,
                    K_AUDIO_OUTPUT_UNIT_PROPERTY_SET_INPUT_CALLBACK,
                    K_AUDIO_UNIT_SCOPE_GLOBAL,
                    0,
                    &cb,
                    "SetInputCallback",
                )?;
            } else {
                #[cfg(not(target_os = "ios"))]
                if let Some(d) = device {
                    set(
                        unit,
                        K_AUDIO_OUTPUT_UNIT_PROPERTY_CURRENT_DEVICE,
                        K_AUDIO_UNIT_SCOPE_GLOBAL,
                        0,
                        &d,
                        "CurrentDevice",
                    )?;
                }
                set(
                    unit,
                    K_AUDIO_UNIT_PROPERTY_STREAM_FORMAT,
                    K_AUDIO_UNIT_SCOPE_INPUT,
                    0,
                    &fmt,
                    "StreamFormat",
                )?;
                // The output path sizes its scratch and its callback
                // contract off MAX_SLICE too, so the unit must be told the
                // same ceiling it tells the capture path — without this a
                // device that hands the render callback more frames than
                // the default exercises the undersized-buffer path above.
                set(
                    unit,
                    K_AUDIO_UNIT_PROPERTY_MAXIMUM_FRAMES_PER_SLICE,
                    K_AUDIO_UNIT_SCOPE_GLOBAL,
                    0,
                    &MAX_SLICE,
                    "MaximumFramesPerSlice",
                )?;
                let cb = AuRenderCallbackStruct {
                    input_proc: render,
                    input_proc_ref_con: shared.cast(),
                };
                set(
                    unit,
                    K_AUDIO_UNIT_PROPERTY_SET_RENDER_CALLBACK,
                    K_AUDIO_UNIT_SCOPE_INPUT,
                    0,
                    &cb,
                    "SetRenderCallback",
                )?;
            }
            status(AudioUnitInitialize(unit), "AudioUnitInitialize")?;
            status(AudioOutputUnitStart(unit), "AudioOutputUnitStart")
        };
        if let Err(e) = setup() {
            AudioComponentInstanceDispose(unit);
            drop(Box::from_raw(shared));
            return Err(if e.contains("default input") {
                BackendError::Unavailable(e)
            } else {
                BackendError::Refused(e)
            });
        }
        #[cfg(target_os = "macos")]
        if !capture {
            if let Some(ns) = device
                .or_else(hal::default_output)
                .and_then(hal::latency_ns)
            {
                counters
                    .latency_ns
                    .store(ns, std::sync::atomic::Ordering::Relaxed);
            }
        }
        Ok(DeviceStream::new(
            "coreaudio",
            StreamConfig {
                rate: config.rate,
                channels: ch,
                block: config.block,
            },
            counters,
            Box::new(CaStream { unit, shared }),
        ))
    }
}

/// The Core Audio backend.
#[derive(Debug, Clone, Copy, Default)]
pub struct CoreAudio;

impl Backend for CoreAudio {
    fn name(&self) -> &'static str {
        "coreaudio"
    }

    fn open_output(
        &self,
        config: StreamConfig,
        render: RenderFn,
    ) -> Result<DeviceStream, BackendError> {
        open(config, Callback::Render(render), None)
    }

    /// macOS: every device with output channels; ids are device UIDs,
    /// stable across reboots. iOS has no devices to choose (the session's
    /// route decides).
    fn devices(&self) -> Result<Vec<DeviceInfo>, BackendError> {
        #[cfg(target_os = "macos")]
        {
            Ok(hal::devices())
        }
        #[cfg(not(target_os = "macos"))]
        {
            Ok(Vec::new())
        }
    }

    /// macOS: the default output's UID. The default-output unit follows
    /// the default itself, but a stream on a named device does not, and a
    /// device that vanishes ends the stream; the supervisor watches this.
    fn default_device_id(&self) -> Option<String> {
        #[cfg(target_os = "macos")]
        {
            hal::default_uid()
        }
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    }

    fn open_output_on(
        &self,
        device: &str,
        config: StreamConfig,
        render: RenderFn,
    ) -> Result<DeviceStream, BackendError> {
        #[cfg(target_os = "macos")]
        {
            let id = hal::by_uid(device)
                .ok_or_else(|| BackendError::Refused(format!("no Core Audio device {device:?}")))?;
            open(config, Callback::Render(render), Some(id))
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (config, render);
            Err(BackendError::Refused(format!(
                "no device selection on this platform ({device:?})"
            )))
        }
    }

    fn open_input(
        &self,
        config: StreamConfig,
        capture: CaptureFn,
    ) -> Result<DeviceStream, BackendError> {
        open(config, Callback::Capture(capture), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_char_codes_and_layouts() {
        assert_eq!(K_AUDIO_UNIT_TYPE_OUTPUT, 0x6175_6f75);
        assert_eq!(K_AUDIO_FORMAT_LINEAR_PCM, 0x6c70_636d);
        assert_eq!(size_of::<AudioStreamBasicDescription>(), 40);
        assert_eq!(size_of::<AudioBufferList>(), 24);
        assert_eq!(size_of::<AudioComponentDescription>(), 20);
    }

    #[test]
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    fn plays_on_the_default_output_or_refuses_cleanly() {
        match CoreAudio.open_output(StreamConfig::default(), Box::new(|b, _| b.fill(0.0))) {
            Ok(s) => assert!(
                s.wait_for(5, std::time::Duration::from_secs(10)),
                "{:?}",
                s.stats()
            ),
            Err(e) => assert!(!e.to_string().is_empty()),
        }
    }
}
