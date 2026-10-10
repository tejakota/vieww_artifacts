//! VST3: export a [`Plugin`] as a VST3 module, and host VST3 modules.
//!
//! ```ignore
//! vieww_audio_plugin::export_vst3!(MyGain, MySynth);
//! ```
//!
//! in a `cdylib` crate defines `GetPluginFactory` (plus `ModuleEntry`/
//! `ModuleExit` on Linux, `bundleEntry`/`bundleExit` on macOS and
//! `InitDll`/`ExitDll` on Windows). Each plugin is one **single-component**
//! class: one object implementing `IComponent`, `IAudioProcessor`,
//! `IEditController`, `IProcessContextRequirements` and, for instruments,
//! `IMidiMapping` — the arrangement the VST3 SDK calls a plugin "without
//! a separate controller", which every host supports.
//!
//! VST3 never sends MIDI controllers as events: hosts deliver them to
//! parameters a plugin maps with `IMidiMapping`. So an instrument gets
//! hidden parameters for every channel's 128 controllers, channel pressure
//! and pitch bend (ids from [`MIDI_PARAM_BASE`]), and the wrapper turns
//! their changes back into [`Event::Midi`] at the sample they arrive on —
//! the plugin sees the same MIDI under VST3 as under CLAP.
//!
//! Threads are handled as in [`clap`](crate::clap): atomics mirror
//! parameter values for the controller side, and the audio thread only
//! `try_lock`s the plugin.

#![allow(
    non_upper_case_globals,
    reason = "VST3 constants keep their C names in patterns"
)]

pub mod host;
pub mod sys;

use std::ffi::{c_char, c_void};
use std::mem::offset_of;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;

use crate::vcall;
use crate::{
    Driver, Event, Expression, Kind, ParamChange, ParamInfo, Plugin, Status, Taper, Transport,
    MAX_CHANNELS,
};
use sys::*;

/// First id of the hidden MIDI-controller parameters (instruments only):
/// `MIDI_PARAM_BASE + channel × 130 + controller`, controller 0..=127, 128
/// channel pressure, 129 pitch bend. Plugin parameter ids must stay below.
pub const MIDI_PARAM_BASE: u32 = 0x7F00_0000;
const MIDI_PER_CHANNEL: u32 = 130;
const MIDI_PARAMS: u32 = 16 * MIDI_PER_CHANNEL;

/// The factory object a module hands out (one per library, static).
#[repr(C)]
#[derive(Debug)]
pub struct FactoryObject(pub *const IPluginFactory3Vtbl);
// SAFETY: points at an immutable static vtable.
unsafe impl Sync for FactoryObject {}

/// Map feature tags to VST3 subcategories.
#[must_use]
pub fn subcategories(kind: Kind, features: &[&str]) -> String {
    let mut out = vec![match kind {
        Kind::Instrument => "Instrument",
        Kind::Effect | Kind::NoteEffect => "Fx",
    }];
    for f in features {
        let s = match *f {
            "synthesizer" => "Synth",
            "sampler" => "Sampler",
            "drum" | "drum-machine" => "Drum",
            "equalizer" => "EQ",
            "filter" => "Filter",
            "delay" => "Delay",
            "reverb" => "Reverb",
            "distortion" => "Distortion",
            "compressor" | "limiter" | "dynamics" | "gate" => "Dynamics",
            "chorus" | "flanger" | "phaser" => "Modulation",
            "pitch-shifter" => "Pitch Shift",
            "analyzer" => "Analyzer",
            "utility" => "Tools",
            "spatial" | "surround" => "Spatial",
            "mono" => "Mono",
            "stereo" => "Stereo",
            _ => continue,
        };
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out.join("|")
}

fn copy_c(dst: &mut [c_char], s: &str) {
    crate::write_c_str(dst, s);
}

/// Fill a `PClassInfo` for `P`.
pub fn class_info<P: Plugin>(out: &mut PClassInfo) {
    let i = P::info();
    out.cid = i.vst3_class_id;
    out.cardinality = kManyInstances;
    copy_c(&mut out.category, "Audio Module Class");
    copy_c(&mut out.name, i.name);
}

/// Fill a `PClassInfo2` for `P`.
pub fn class_info2<P: Plugin>(out: &mut PClassInfo2) {
    let i = P::info();
    out.cid = i.vst3_class_id;
    out.cardinality = kManyInstances;
    copy_c(&mut out.category, "Audio Module Class");
    copy_c(&mut out.name, i.name);
    out.classFlags = 0;
    copy_c(&mut out.subCategories, &subcategories(i.kind, i.features));
    copy_c(&mut out.vendor, i.vendor);
    copy_c(&mut out.version, i.version);
    copy_c(&mut out.sdkVersion, kVstVersionString);
}

/// Fill a `PClassInfoW` for `P`.
pub fn class_info_w<P: Plugin>(out: &mut PClassInfoW) {
    let i = P::info();
    out.cid = i.vst3_class_id;
    out.cardinality = kManyInstances;
    copy_c(&mut out.category, "Audio Module Class");
    write_tchar(&mut out.name, i.name);
    out.classFlags = 0;
    copy_c(&mut out.subCategories, &subcategories(i.kind, i.features));
    write_tchar(&mut out.vendor, i.vendor);
    write_tchar(&mut out.version, i.version);
    write_tchar(&mut out.sdkVersion, kVstVersionString);
}

/// What the audio thread owns, behind one lock.
struct Audio<P: Plugin> {
    driver: Driver<P>,
    scratch: Vec<Vec<f32>>,
    /// Sounding notes: host note id → (channel, key).
    notes: [(i32, u8, u8); 128],
}

#[repr(C)]
struct Object<P: Plugin> {
    i_component: *const IComponentVtbl,
    i_processor: *const IAudioProcessorVtbl,
    i_controller: *const IEditControllerVtbl,
    i_context: *const IProcessContextRequirementsVtbl,
    i_midi: *const IMidiMappingVtbl,
    vt_component: IComponentVtbl,
    vt_processor: IAudioProcessorVtbl,
    vt_controller: IEditControllerVtbl,
    vt_context: IProcessContextRequirementsVtbl,
    vt_midi: IMidiMappingVtbl,
    refs: AtomicU32,
    audio: Mutex<Audio<P>>,
    params: Vec<ParamInfo>,
    /// Plain values (plugin parameters), for the controller side.
    values: Vec<AtomicU64>,
    /// Normalised values of the hidden MIDI parameters.
    midi: Vec<AtomicU64>,
    rate_bits: AtomicU64,
    max_block: AtomicU32,
    latency: AtomicU32,
    handler: AtomicPtr<c_void>,
}

macro_rules! base {
    ($p:ty, $this:expr, $field:ident) => {{
        // SAFETY: `$this` is the address of the object's `$field`, an
        // interface pointer handed out by `query`; subtracting its offset
        // recovers the object, alive while any reference is held.
        unsafe {
            &*($this
                .cast::<u8>()
                .sub(offset_of!(Object<$p>, $field))
                .cast::<Object<$p>>())
        }
    }};
}

impl<P: Plugin> Object<P> {
    fn new() -> Box<Self> {
        let driver = Driver::<P>::new();
        let params = driver.params.clone();
        let values = params
            .iter()
            .map(|p| AtomicU64::new(driver.plugin.get_param(p.id).unwrap_or(p.default).to_bits()))
            .collect();
        let midi = (0..if P::info().note_input { MIDI_PARAMS } else { 0 })
            .map(|i| {
                AtomicU64::new(if i % MIDI_PER_CHANNEL == 129 {
                    0.5f64.to_bits()
                } else {
                    0
                })
            })
            .collect();
        let latency = AtomicU32::new(driver.plugin.latency());
        let mut o = Box::new(Self {
            i_component: std::ptr::null(),
            i_processor: std::ptr::null(),
            i_controller: std::ptr::null(),
            i_context: std::ptr::null(),
            i_midi: std::ptr::null(),
            vt_component: IComponentVtbl {
                unknown: FUnknownVtbl {
                    queryInterface: c_query::<P>,
                    addRef: c_add_ref::<P>,
                    release: c_release::<P>,
                },
                initialize,
                terminate,
                getControllerClassId: get_controller_class_id,
                setIoMode: set_io_mode,
                getBusCount: get_bus_count::<P>,
                getBusInfo: get_bus_info::<P>,
                getRoutingInfo: get_routing_info,
                activateBus: activate_bus::<P>,
                setActive: set_active::<P>,
                setState: set_state::<P>,
                getState: get_state::<P>,
            },
            vt_processor: IAudioProcessorVtbl {
                unknown: FUnknownVtbl {
                    queryInterface: p_query::<P>,
                    addRef: p_add_ref::<P>,
                    release: p_release::<P>,
                },
                setBusArrangements: set_bus_arrangements::<P>,
                getBusArrangement: get_bus_arrangement::<P>,
                canProcessSampleSize: can_process_sample_size,
                getLatencySamples: get_latency::<P>,
                setupProcessing: setup_processing::<P>,
                setProcessing: set_processing::<P>,
                process: process::<P>,
                getTailSamples: get_tail::<P>,
            },
            vt_controller: IEditControllerVtbl {
                unknown: FUnknownVtbl {
                    queryInterface: e_query::<P>,
                    addRef: e_add_ref::<P>,
                    release: e_release::<P>,
                },
                initialize,
                terminate,
                setComponentState: set_component_state::<P>,
                setState: controller_set_state,
                getState: controller_get_state,
                getParameterCount: get_parameter_count::<P>,
                getParameterInfo: get_parameter_info::<P>,
                getParamStringByValue: get_param_string::<P>,
                getParamValueByString: get_param_value::<P>,
                normalizedParamToPlain: normalized_to_plain::<P>,
                plainParamToNormalized: plain_to_normalized::<P>,
                getParamNormalized: get_param_normalized::<P>,
                setParamNormalized: set_param_normalized::<P>,
                setComponentHandler: set_component_handler::<P>,
                createView: create_view,
            },
            vt_context: IProcessContextRequirementsVtbl {
                unknown: FUnknownVtbl {
                    queryInterface: x_query::<P>,
                    addRef: x_add_ref::<P>,
                    release: x_release::<P>,
                },
                getProcessContextRequirements: context_requirements,
            },
            vt_midi: IMidiMappingVtbl {
                unknown: FUnknownVtbl {
                    queryInterface: m_query::<P>,
                    addRef: m_add_ref::<P>,
                    release: m_release::<P>,
                },
                getMidiControllerAssignment: midi_assignment::<P>,
            },
            refs: AtomicU32::new(1),
            audio: Mutex::new(Audio {
                driver,
                scratch: Vec::new(),
                notes: [(-1, 0, 0); 128],
            }),
            params,
            values,
            midi,
            rate_bits: AtomicU64::new(48_000f64.to_bits()),
            max_block: AtomicU32::new(4_096),
            latency,
            handler: AtomicPtr::new(null_mut()),
        });
        o.i_component = &raw const o.vt_component;
        o.i_processor = &raw const o.vt_processor;
        o.i_controller = &raw const o.vt_controller;
        o.i_context = &raw const o.vt_context;
        o.i_midi = &raw const o.vt_midi;
        o
    }

    fn query(&self, iid: &TUID) -> *mut c_void {
        let me = std::ptr::from_ref(self).cast_mut();
        // SAFETY: field addresses of a live object.
        unsafe {
            let p: *mut c_void =
                if *iid == FUnknown_iid || *iid == IPluginBase_iid || *iid == IComponent_iid {
                    (&raw mut (*me).i_component).cast()
                } else if *iid == IAudioProcessor_iid {
                    (&raw mut (*me).i_processor).cast()
                } else if *iid == IEditController_iid {
                    (&raw mut (*me).i_controller).cast()
                } else if *iid == IProcessContextRequirements_iid {
                    (&raw mut (*me).i_context).cast()
                } else if *iid == IMidiMapping_iid && P::info().note_input {
                    (&raw mut (*me).i_midi).cast()
                } else {
                    return null_mut();
                };
            self.refs.fetch_add(1, Ordering::AcqRel);
            p
        }
    }

    fn release(&self) -> u32 {
        let n = self.refs.fetch_sub(1, Ordering::AcqRel) - 1;
        if n == 0 {
            let h = self.handler.swap(null_mut(), Ordering::AcqRel);
            if !h.is_null() {
                // SAFETY: we hold a reference taken in `setComponentHandler`.
                unsafe { vcall!(h, FUnknownVtbl, release) };
            }
            // SAFETY: the last reference: reclaim the box made in `create`.
            drop(unsafe { Box::from_raw(std::ptr::from_ref(self).cast_mut()) });
        }
        n
    }

    fn mirror(&self, d: &Driver<P>) {
        for (info, slot) in self.params.iter().zip(&self.values) {
            if let Some(v) = d.plugin.get_param(info.id) {
                slot.store(v.to_bits(), Ordering::Relaxed);
            }
        }
    }

    fn param_index(&self, id: ParamID) -> Option<usize> {
        self.params.iter().position(|p| p.id == id)
    }

    fn midi_index(&self, id: ParamID) -> Option<usize> {
        let i = id.checked_sub(MIDI_PARAM_BASE)? as usize;
        (i < self.midi.len()).then_some(i)
    }

    fn restart(&self, flags: i32) {
        let h = self.handler.load(Ordering::Acquire);
        if !h.is_null() {
            // SAFETY: a live component handler (we hold a reference).
            unsafe { vcall!(h, IComponentHandlerVtbl, restartComponent, flags) };
        }
    }
}

/// Create an instance of `P` and query it for `iid`.
///
/// # Safety
/// `iid` points at 16 bytes and `obj` is writable.
pub unsafe fn create<P: Plugin>(iid: *const c_char, obj: *mut *mut c_void) -> tresult {
    if iid.is_null() || obj.is_null() {
        return kInvalidArgument;
    }
    let o = Object::<P>::new();
    // SAFETY: the caller's contract.
    let iid = unsafe { &*iid.cast::<TUID>() };
    let p = o.query(iid);
    let raw = Box::into_raw(o);
    // SAFETY: `raw` is live; drop our creation reference (the query added one).
    unsafe {
        (*raw).release();
        *obj = p;
    }
    if p.is_null() {
        kNoInterface
    } else {
        kResultOk
    }
}

macro_rules! unknown_for {
    ($q:ident, $a:ident, $r:ident, $field:ident) => {
        unsafe extern "system" fn $q<P: Plugin>(
            this: *mut c_void,
            iid: *const TUID,
            obj: *mut *mut c_void,
        ) -> tresult {
            if iid.is_null() || obj.is_null() {
                return kInvalidArgument;
            }
            let o = base!(P, this, $field);
            // SAFETY: valid pointers (COM contract).
            let p = o.query(unsafe { &*iid });
            unsafe { *obj = p };
            if p.is_null() {
                kNoInterface
            } else {
                kResultOk
            }
        }
        unsafe extern "system" fn $a<P: Plugin>(this: *mut c_void) -> u32 {
            base!(P, this, $field).refs.fetch_add(1, Ordering::AcqRel) + 1
        }
        unsafe extern "system" fn $r<P: Plugin>(this: *mut c_void) -> u32 {
            base!(P, this, $field).release()
        }
    };
}

unknown_for!(c_query, c_add_ref, c_release, i_component);
unknown_for!(p_query, p_add_ref, p_release, i_processor);
unknown_for!(e_query, e_add_ref, e_release, i_controller);
unknown_for!(x_query, x_add_ref, x_release, i_context);
unknown_for!(m_query, m_add_ref, m_release, i_midi);

unsafe extern "system" fn initialize(_: *mut c_void, _: *mut c_void) -> tresult {
    kResultOk
}

unsafe extern "system" fn terminate(_: *mut c_void) -> tresult {
    kResultOk
}

unsafe extern "system" fn get_controller_class_id(_: *mut c_void, _: *mut TUID) -> tresult {
    // Single component: the controller is this object.
    kNotImplemented
}

unsafe extern "system" fn set_io_mode(_: *mut c_void, _: i32) -> tresult {
    kResultOk
}

fn channels<P: Plugin>(dir: i32) -> u32 {
    let i = P::info();
    if dir == kInput {
        i.inputs
    } else {
        i.outputs
    }
}

unsafe extern "system" fn get_bus_count<P: Plugin>(_: *mut c_void, ty: i32, dir: i32) -> i32 {
    match ty {
        t if t == kAudio => i32::from(channels::<P>(dir) > 0),
        t if t == kEvent => i32::from(dir == kInput && P::info().note_input),
        _ => 0,
    }
}

unsafe extern "system" fn get_bus_info<P: Plugin>(
    _: *mut c_void,
    ty: i32,
    dir: i32,
    index: i32,
    bus: *mut BusInfo,
) -> tresult {
    // SAFETY: plain function call.
    if bus.is_null() || index != 0 || unsafe { get_bus_count::<P>(null_mut(), ty, dir) } == 0 {
        return kInvalidArgument;
    }
    // SAFETY: a writable bus info.
    let b = unsafe { &mut *bus };
    b.mediaType = ty;
    b.direction = dir;
    b.busType = kMain;
    b.flags = kDefaultActive;
    if ty == kAudio {
        b.channelCount = i32::try_from(channels::<P>(dir)).unwrap_or(0);
        write_tchar(&mut b.name, if dir == kInput { "Input" } else { "Output" });
    } else {
        b.channelCount = 16;
        write_tchar(&mut b.name, "Notes");
    }
    kResultOk
}

unsafe extern "system" fn get_routing_info(
    _: *mut c_void,
    _: *mut RoutingInfo,
    _: *mut RoutingInfo,
) -> tresult {
    kNotImplemented
}

unsafe extern "system" fn activate_bus<P: Plugin>(
    _: *mut c_void,
    ty: i32,
    dir: i32,
    index: i32,
    _: TBool,
) -> tresult {
    // SAFETY: plain function call.
    if index == 0 && unsafe { get_bus_count::<P>(null_mut(), ty, dir) } > 0 {
        kResultOk
    } else {
        kInvalidArgument
    }
}

unsafe extern "system" fn set_active<P: Plugin>(this: *mut c_void, state: TBool) -> tresult {
    let o = base!(P, this, i_component);
    let Ok(mut a) = o.audio.lock() else {
        return kInternalError;
    };
    if state != 0 {
        let rate = f64::from_bits(o.rate_bits.load(Ordering::Relaxed));
        let max = o.max_block.load(Ordering::Relaxed);
        a.driver.rate = rate;
        a.driver.max_block = max;
        let ok = a.driver.plugin.activate(rate, max);
        let frames = (max as usize).max(1);
        a.scratch = vec![vec![0.0; frames]; MAX_CHANNELS];
        let lat = a.driver.plugin.latency();
        o.mirror(&a.driver);
        drop(a);
        if o.latency.swap(lat, Ordering::AcqRel) != lat {
            o.restart(kLatencyChanged);
        }
        if ok {
            kResultOk
        } else {
            kResultFalse
        }
    } else {
        a.driver.plugin.deactivate();
        kResultOk
    }
}

/// Read a whole `IBStream`.
///
/// # Safety
/// `s` is a valid stream.
unsafe fn read_stream(s: *mut c_void) -> Option<Vec<u8>> {
    if s.is_null() {
        return None;
    }
    let mut out = Vec::new();
    let mut buf = [0u8; 4_096];
    loop {
        let mut n = 0i32;
        // SAFETY: a valid stream and buffer.
        let r = unsafe {
            vcall!(
                s,
                IBStreamVtbl,
                read,
                buf.as_mut_ptr().cast(),
                4_096,
                &raw mut n
            )
        };
        if r != kResultOk || n <= 0 {
            break;
        }
        out.extend_from_slice(&buf[..usize::try_from(n).unwrap_or(0)]);
    }
    Some(out)
}

/// Write all of `data` to an `IBStream`.
///
/// # Safety
/// `s` is a valid stream.
unsafe fn write_stream(s: *mut c_void, data: &[u8]) -> bool {
    if s.is_null() {
        return false;
    }
    let mut at = 0;
    while at < data.len() {
        let chunk = (data.len() - at).min(1 << 20);
        let mut n = 0i32;
        // SAFETY: a valid stream; the buffer is only read.
        let r = unsafe {
            vcall!(
                s,
                IBStreamVtbl,
                write,
                data[at..].as_ptr().cast_mut().cast(),
                i32::try_from(chunk).unwrap_or(i32::MAX),
                &raw mut n
            )
        };
        if r != kResultOk || n <= 0 {
            return false;
        }
        at += usize::try_from(n).unwrap_or(0);
    }
    true
}

unsafe extern "system" fn set_state<P: Plugin>(this: *mut c_void, s: *mut c_void) -> tresult {
    let o = base!(P, this, i_component);
    // SAFETY: the host's stream.
    let Some(data) = (unsafe { read_stream(s) }) else {
        return kInvalidArgument;
    };
    let Ok(mut a) = o.audio.lock() else {
        return kInternalError;
    };
    let ok = a.driver.plugin.load_state(&data);
    o.mirror(&a.driver);
    if ok {
        kResultOk
    } else {
        kResultFalse
    }
}

unsafe extern "system" fn get_state<P: Plugin>(this: *mut c_void, s: *mut c_void) -> tresult {
    let o = base!(P, this, i_component);
    let data = match o.audio.lock() {
        Ok(a) => a.driver.plugin.save_state(),
        Err(_) => return kInternalError,
    };
    // SAFETY: the host's stream.
    if unsafe { write_stream(s, &data) } {
        kResultOk
    } else {
        kResultFalse
    }
}

fn arrangement(ch: u32) -> SpeakerArrangement {
    match ch {
        1 => kMono,
        2 => kStereo,
        n => (1u64 << n.min(63)) - 1,
    }
}

unsafe extern "system" fn set_bus_arrangements<P: Plugin>(
    _: *mut c_void,
    ins: *mut SpeakerArrangement,
    n_in: i32,
    outs: *mut SpeakerArrangement,
    n_out: i32,
) -> tresult {
    let i = P::info();
    let want_in = i32::from(i.inputs > 0);
    let want_out = i32::from(i.outputs > 0);
    if n_in != want_in || n_out != want_out {
        return kResultFalse;
    }
    // SAFETY: the host passes arrays of the counts it gives.
    let ok_in = n_in == 0 || (!ins.is_null() && unsafe { *ins } == arrangement(i.inputs));
    // SAFETY: as above.
    let ok_out = n_out == 0 || (!outs.is_null() && unsafe { *outs } == arrangement(i.outputs));
    if ok_in && ok_out {
        kResultOk
    } else {
        kResultFalse
    }
}

unsafe extern "system" fn get_bus_arrangement<P: Plugin>(
    _: *mut c_void,
    dir: i32,
    index: i32,
    arr: *mut SpeakerArrangement,
) -> tresult {
    let ch = channels::<P>(dir);
    if index != 0 || ch == 0 || arr.is_null() {
        return kInvalidArgument;
    }
    // SAFETY: a writable arrangement.
    unsafe { *arr = arrangement(ch) };
    kResultOk
}

unsafe extern "system" fn can_process_sample_size(_: *mut c_void, size: i32) -> tresult {
    if size == kSample32 {
        kResultOk
    } else {
        kResultFalse
    }
}

unsafe extern "system" fn get_latency<P: Plugin>(this: *mut c_void) -> u32 {
    base!(P, this, i_processor).latency.load(Ordering::Relaxed)
}

unsafe extern "system" fn get_tail<P: Plugin>(this: *mut c_void) -> u32 {
    let o = base!(P, this, i_processor);
    o.audio.lock().map_or(kNoTail, |a| a.driver.plugin.tail())
}

unsafe extern "system" fn setup_processing<P: Plugin>(
    this: *mut c_void,
    setup: *mut ProcessSetup,
) -> tresult {
    if setup.is_null() {
        return kInvalidArgument;
    }
    let o = base!(P, this, i_processor);
    // SAFETY: a valid setup.
    let s = unsafe { &*setup };
    if s.symbolicSampleSize != kSample32 || s.sampleRate <= 0.0 || s.maxSamplesPerBlock <= 0 {
        return kResultFalse;
    }
    o.rate_bits.store(s.sampleRate.to_bits(), Ordering::Relaxed);
    o.max_block.store(
        u32::try_from(s.maxSamplesPerBlock).unwrap_or(4_096),
        Ordering::Relaxed,
    );
    kResultOk
}

unsafe extern "system" fn set_processing<P: Plugin>(this: *mut c_void, state: TBool) -> tresult {
    let o = base!(P, this, i_processor);
    if state == 0 {
        if let Ok(mut a) = o.audio.lock() {
            a.driver.plugin.reset();
            a.notes = [(-1, 0, 0); 128];
        }
    }
    kResultOk
}

fn transport(c: &ProcessContext) -> Transport {
    let has = |f: u32| c.state & f != 0;
    Transport {
        playing: has(kPlaying),
        recording: has(kRecording),
        looping: has(kCycleActive),
        tempo: has(kTempoValid).then_some(c.tempo),
        beats: has(kProjectTimeMusicValid).then_some(c.projectTimeMusic),
        #[expect(clippy::cast_precision_loss, reason = "sample position to seconds")]
        seconds: (c.sampleRate > 0.0).then(|| c.projectTimeSamples as f64 / c.sampleRate),
        time_signature: has(kTimeSigValid).then(|| {
            (
                u16::try_from(c.timeSigNumerator).unwrap_or(4),
                u16::try_from(c.timeSigDenominator).unwrap_or(4),
            )
        }),
        bar_start: has(kBarPositionValid).then_some(c.barPositionMusic),
        loop_beats: (has(kCycleValid) && has(kCycleActive))
            .then_some((c.cycleStartMusic, c.cycleEndMusic)),
    }
}

/// Queue the host's parameter changes.
///
/// # Safety
/// `changes` is null or a valid `IParameterChanges`.
unsafe fn take_changes<P: Plugin>(o: &Object<P>, a: &mut Audio<P>, changes: *mut c_void) {
    if changes.is_null() {
        return;
    }
    // SAFETY: valid COM objects provided by the host for this call.
    unsafe {
        let n = vcall!(changes, IParameterChangesVtbl, getParameterCount);
        for qi in 0..n {
            let q = vcall!(changes, IParameterChangesVtbl, getParameterData, qi);
            if q.is_null() {
                continue;
            }
            let id = vcall!(q, IParamValueQueueVtbl, getParameterId);
            let points = vcall!(q, IParamValueQueueVtbl, getPointCount);
            for pi in 0..points {
                let (mut offset, mut value) = (0i32, 0.0f64);
                if vcall!(
                    q,
                    IParamValueQueueVtbl,
                    getPoint,
                    pi,
                    &raw mut offset,
                    &raw mut value
                ) != kResultOk
                {
                    continue;
                }
                let time = u32::try_from(offset.max(0)).unwrap_or(0);
                if let Some(i) = o.param_index(id) {
                    a.driver.push_change(ParamChange {
                        time,
                        id,
                        value: o.params[i].denormalize(value),
                    });
                } else if let Some(m) = o.midi_index(id) {
                    o.midi[m].store(value.to_bits(), Ordering::Relaxed);
                    a.driver.push_event(Event::Midi {
                        time,
                        data: midi_bytes(m, value),
                    });
                }
            }
        }
    }
}

/// The MIDI message a hidden controller parameter's value stands for.
fn midi_bytes(index: usize, value: f64) -> [u8; 3] {
    let ch = u8::try_from(index / MIDI_PER_CHANNEL as usize).unwrap_or(0) & 15;
    let ctrl = index % MIDI_PER_CHANNEL as usize;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to 7/14 bits"
    )]
    let seven = (value.clamp(0.0, 1.0) * 127.0).round() as u8;
    match ctrl {
        128 => [0xD0 | ch, seven, 0],
        129 => {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "14 bits"
            )]
            let v = (value.clamp(0.0, 1.0) * 16_383.0).round() as u16;
            [0xE0 | ch, (v & 0x7F) as u8, (v >> 7) as u8]
        }
        c => [0xB0 | ch, u8::try_from(c).unwrap_or(0), seven],
    }
}

/// Queue the host's note events.
///
/// # Safety
/// `events` is null or a valid `IEventList`.
unsafe fn take_events<P: Plugin>(a: &mut Audio<P>, events: *mut c_void) {
    if events.is_null() {
        return;
    }
    let ch = |c: i16| u8::try_from(c.clamp(0, 15)).unwrap_or(0);
    let key = |k: i16| u8::try_from(k.clamp(0, 127)).unwrap_or(0);
    // SAFETY: a valid event list; each union arm read matches `type_`.
    unsafe {
        let n = vcall!(events, IEventListVtbl, getEventCount);
        for i in 0..n {
            let mut e: sys::Event = std::mem::zeroed();
            if vcall!(events, IEventListVtbl, getEvent, i, &raw mut e) != kResultOk {
                continue;
            }
            let time = u32::try_from(e.sampleOffset.max(0)).unwrap_or(0);
            match e.type_ {
                kNoteOnEvent => {
                    let n = e.body.noteOn;
                    if n.velocity == 0.0 {
                        // A velocity-0 note-on is a note-off (MIDI's convention).
                        a.driver.push_event(Event::NoteOff {
                            time,
                            channel: ch(n.channel),
                            key: key(n.pitch),
                            velocity: 0.0,
                            note_id: n.noteId,
                        });
                        continue;
                    }
                    if n.noteId >= 0 {
                        if let Some(slot) = a.notes.iter_mut().find(|s| s.0 < 0 || s.0 == n.noteId)
                        {
                            *slot = (n.noteId, ch(n.channel), key(n.pitch));
                        }
                    }
                    a.driver.push_event(Event::NoteOn {
                        time,
                        channel: ch(n.channel),
                        key: key(n.pitch),
                        velocity: n.velocity,
                        note_id: n.noteId,
                    });
                }
                kNoteOffEvent => {
                    let n = e.body.noteOff;
                    if let Some(slot) = a
                        .notes
                        .iter_mut()
                        .find(|s| s.0 == n.noteId && n.noteId >= 0)
                    {
                        slot.0 = -1;
                    }
                    a.driver.push_event(Event::NoteOff {
                        time,
                        channel: ch(n.channel),
                        key: key(n.pitch),
                        velocity: n.velocity,
                        note_id: n.noteId,
                    });
                }
                kPolyPressureEvent => {
                    let p = e.body.polyPressure;
                    a.driver.push_event(Event::NoteExpression {
                        time,
                        expression: Expression::Pressure,
                        channel: ch(p.channel),
                        key: key(p.pitch),
                        note_id: p.noteId,
                        value: f64::from(p.pressure),
                    });
                }
                kNoteExpressionValueEvent => {
                    let x = e.body.noteExpressionValue;
                    let (expression, value) = match x.typeId {
                        // VST3 volume: 0..1 normalised maps to gain 0..4 (×1 at 0.25).
                        kVolumeTypeID => (Expression::Volume, x.value * 4.0),
                        kPanTypeID => (Expression::Pan, x.value),
                        // Tuning: 0.5 is centre, the range ±120 semitones.
                        kTuningTypeID => (Expression::Tuning, (x.value - 0.5) * 240.0),
                        kVibratoTypeID => (Expression::Vibrato, x.value),
                        kExpressionTypeID => (Expression::Expression, x.value),
                        kBrightnessTypeID => (Expression::Brightness, x.value),
                        _ => continue,
                    };
                    let Some(&(_, c, k)) = a.notes.iter().find(|s| s.0 == x.noteId) else {
                        continue;
                    };
                    a.driver.push_event(Event::NoteExpression {
                        time,
                        expression,
                        channel: c,
                        key: k,
                        note_id: x.noteId,
                        value,
                    });
                }
                _ => {}
            }
        }
    }
}

unsafe extern "system" fn process<P: Plugin>(this: *mut c_void, data: *mut ProcessData) -> tresult {
    if data.is_null() {
        return kInvalidArgument;
    }
    let o = base!(P, this, i_processor);
    // SAFETY: a valid process data block for this call.
    let d = unsafe { &mut *data };
    if d.symbolicSampleSize != kSample32 {
        return kInvalidArgument;
    }
    let frames = usize::try_from(d.numSamples.max(0)).unwrap_or(0);
    let chan_ptrs = |bufs: *mut AudioBusBuffers, n: i32| -> ([*mut f32; MAX_CHANNELS], usize) {
        let mut out = [null_mut::<f32>(); MAX_CHANNELS];
        if n <= 0 || bufs.is_null() {
            return (out, 0);
        }
        // SAFETY: the host provides `n` buses; bus 0 is the main bus.
        let b = unsafe { &*bufs };
        let c = usize::try_from(b.numChannels.max(0))
            .unwrap_or(0)
            .min(MAX_CHANNELS);
        if b.channelBuffers.is_null() {
            return (out, 0);
        }
        for (i, slot) in out.iter_mut().enumerate().take(c) {
            // SAFETY: `channelBuffers` has `numChannels` entries.
            *slot = unsafe { (*b.channelBuffers.add(i)).cast() };
        }
        (out, c)
    };
    let (in_ptrs, n_in) = chan_ptrs(d.inputs, d.numInputs);
    let (out_ptrs, n_out) = chan_ptrs(d.outputs, d.numOutputs);
    if frames > 0
        && (in_ptrs[..n_in]
            .iter()
            .chain(&out_ptrs[..n_out])
            .any(|p| p.is_null()))
    {
        return kInvalidArgument;
    }
    let Ok(mut a) = o.audio.try_lock() else {
        for &p in &out_ptrs[..n_out] {
            // SAFETY: a host output channel of `frames` samples.
            unsafe { std::slice::from_raw_parts_mut(p, frames).fill(0.0) };
        }
        return kResultOk;
    };
    if !d.processContext.is_null() {
        // SAFETY: a valid context when non-null.
        a.driver.transport = transport(unsafe { &*d.processContext });
    }
    // SAFETY: the host's change and event lists.
    unsafe {
        take_changes(o, &mut a, d.inputParameterChanges);
        take_events(&mut a, d.inputEvents);
    }
    let a = &mut *a;
    let aliased = in_ptrs[..n_in]
        .iter()
        .any(|i| out_ptrs[..n_out].contains(i));
    let mut ins: [&[f32]; MAX_CHANNELS] = [&[]; MAX_CHANNELS];
    if aliased {
        if a.scratch.len() < n_in || a.scratch.iter().take(n_in).any(|s| s.len() < frames) {
            return kInvalidArgument;
        }
        for (dst, &src) in a.scratch.iter_mut().zip(&in_ptrs[..n_in]) {
            // SAFETY: a host input channel of `frames` samples.
            dst[..frames].copy_from_slice(unsafe { std::slice::from_raw_parts(src, frames) });
        }
        for (c, slot) in ins.iter_mut().enumerate().take(n_in) {
            *slot = &a.scratch[c][..frames];
        }
    } else {
        for (c, slot) in ins.iter_mut().enumerate().take(n_in) {
            // SAFETY: a distinct host input channel of `frames` samples.
            *slot = unsafe { std::slice::from_raw_parts(in_ptrs[c], frames) };
        }
    }
    let mut outs: [&mut [f32]; MAX_CHANNELS] = Default::default();
    for (c, slot) in outs.iter_mut().enumerate().take(n_out) {
        // SAFETY: distinct host output channels of `frames` samples.
        *slot = unsafe { std::slice::from_raw_parts_mut(out_ptrs[c], frames) };
    }
    let status = a.driver.run(&ins[..n_in], &mut outs[..n_out], frames);
    o.mirror(&a.driver);
    if n_out > 0 {
        // SAFETY: the main output bus.
        unsafe { (*d.outputs).silenceFlags = 0 };
    }
    if status == Status::Error {
        kInternalError
    } else {
        kResultOk
    }
}

unsafe extern "system" fn context_requirements(_: *mut c_void) -> u32 {
    kNeedTransportState
        | kNeedTempo
        | kNeedTimeSignature
        | kNeedProjectTimeMusic
        | kNeedBarPositionMusic
        | kNeedCycleMusic
}

unsafe extern "system" fn set_component_state<P: Plugin>(
    this: *mut c_void,
    s: *mut c_void,
) -> tresult {
    let o = base!(P, this, i_controller);
    // SAFETY: the host's stream.
    let Some(data) = (unsafe { read_stream(s) }) else {
        return kInvalidArgument;
    };
    // Read the values out of a scratch instance: the controller side never
    // touches the processing one.
    let mut scratch = P::create();
    if !scratch.load_state(&data) {
        return kResultFalse;
    }
    for (info, slot) in o.params.iter().zip(&o.values) {
        if let Some(v) = scratch.get_param(info.id) {
            slot.store(v.to_bits(), Ordering::Relaxed);
        }
    }
    kResultOk
}

unsafe extern "system" fn controller_set_state(_: *mut c_void, _: *mut c_void) -> tresult {
    kResultOk
}

unsafe extern "system" fn controller_get_state(_: *mut c_void, _: *mut c_void) -> tresult {
    kResultOk
}

unsafe extern "system" fn get_parameter_count<P: Plugin>(this: *mut c_void) -> i32 {
    let o = base!(P, this, i_controller);
    i32::try_from(o.params.len() + o.midi.len()).unwrap_or(0)
}

fn step_count(p: &ParamInfo) -> i32 {
    if p.taper == Taper::Stepped {
        #[expect(clippy::cast_possible_truncation, reason = "a step count")]
        let n = (p.max - p.min).round() as i32;
        n.max(1)
    } else {
        0
    }
}

unsafe extern "system" fn get_parameter_info<P: Plugin>(
    this: *mut c_void,
    index: i32,
    info: *mut ParameterInfo,
) -> tresult {
    let o = base!(P, this, i_controller);
    if info.is_null() || index < 0 {
        return kInvalidArgument;
    }
    let i = index as usize;
    // SAFETY: a writable parameter info.
    let out = unsafe { &mut *info };
    if let Some(p) = o.params.get(i) {
        out.id = p.id;
        write_tchar(&mut out.title, &p.name);
        write_tchar(&mut out.shortTitle, &p.name);
        write_tchar(&mut out.units, p.unit);
        out.stepCount = step_count(p);
        out.defaultNormalizedValue = p.normalize(p.default);
        out.unitId = kRootUnitId;
        out.flags = if p.automatable { kCanAutomate } else { 0 }
            | if p.choices.is_empty() { 0 } else { kIsList };
        return kResultOk;
    }
    let m = i - o.params.len();
    if m >= o.midi.len() {
        return kInvalidArgument;
    }
    let (ch, ctrl) = (m / MIDI_PER_CHANNEL as usize, m % MIDI_PER_CHANNEL as usize);
    out.id = MIDI_PARAM_BASE + u32::try_from(m).unwrap_or(0);
    let title = match ctrl {
        128 => format!("Channel Pressure [{}]", ch + 1),
        129 => format!("Pitch Bend [{}]", ch + 1),
        c => format!("MIDI CC {c} [{}]", ch + 1),
    };
    write_tchar(&mut out.title, &title);
    write_tchar(&mut out.shortTitle, &title);
    write_tchar(&mut out.units, "");
    out.stepCount = 0;
    out.defaultNormalizedValue = if ctrl == 129 { 0.5 } else { 0.0 };
    out.unitId = kRootUnitId;
    out.flags = kIsHidden | kCanAutomate;
    kResultOk
}

unsafe extern "system" fn get_param_string<P: Plugin>(
    this: *mut c_void,
    id: ParamID,
    value: ParamValue,
    string: *mut TChar,
) -> tresult {
    let o = base!(P, this, i_controller);
    if string.is_null() {
        return kInvalidArgument;
    }
    let text = if let Some(i) = o.param_index(id) {
        // VST3 hosts append `ParameterInfo::units` themselves.
        let p = &o.params[i];
        let t = p.display(p.denormalize(value));
        match t.strip_suffix(p.unit) {
            Some(v) if !p.unit.is_empty() => v.trim_end().to_owned(),
            _ => t,
        }
    } else if o.midi_index(id).is_some() {
        format!("{:.3}", value.clamp(0.0, 1.0))
    } else {
        return kInvalidArgument;
    };
    // SAFETY: a String128 buffer.
    write_tchar(
        unsafe { std::slice::from_raw_parts_mut(string, 128) },
        &text,
    );
    kResultOk
}

unsafe extern "system" fn get_param_value<P: Plugin>(
    this: *mut c_void,
    id: ParamID,
    string: *mut TChar,
    value: *mut ParamValue,
) -> tresult {
    let o = base!(P, this, i_controller);
    if string.is_null() || value.is_null() {
        return kInvalidArgument;
    }
    // SAFETY: a NUL-terminated UTF-16 string from the host.
    let text = unsafe {
        let mut n = 0;
        while n < 4_096 && *string.add(n) != 0 {
            n += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(string, n))
    };
    let v = if let Some(i) = o.param_index(id) {
        match o.params[i].parse(&text) {
            Some(plain) => o.params[i].normalize(plain),
            None => return kResultFalse,
        }
    } else if o.midi_index(id).is_some() {
        match text.trim().parse::<f64>() {
            Ok(v) => v.clamp(0.0, 1.0),
            Err(_) => return kResultFalse,
        }
    } else {
        return kInvalidArgument;
    };
    // SAFETY: a writable value.
    unsafe { *value = v };
    kResultOk
}

unsafe extern "system" fn normalized_to_plain<P: Plugin>(
    this: *mut c_void,
    id: ParamID,
    value: ParamValue,
) -> ParamValue {
    let o = base!(P, this, i_controller);
    o.param_index(id)
        .map_or(value, |i| o.params[i].denormalize(value))
}

unsafe extern "system" fn plain_to_normalized<P: Plugin>(
    this: *mut c_void,
    id: ParamID,
    value: ParamValue,
) -> ParamValue {
    let o = base!(P, this, i_controller);
    o.param_index(id)
        .map_or(value, |i| o.params[i].normalize(value))
}

unsafe extern "system" fn get_param_normalized<P: Plugin>(
    this: *mut c_void,
    id: ParamID,
) -> ParamValue {
    let o = base!(P, this, i_controller);
    if let Some(i) = o.param_index(id) {
        o.params[i].normalize(f64::from_bits(o.values[i].load(Ordering::Relaxed)))
    } else if let Some(m) = o.midi_index(id) {
        f64::from_bits(o.midi[m].load(Ordering::Relaxed))
    } else {
        0.0
    }
}

unsafe extern "system" fn set_param_normalized<P: Plugin>(
    this: *mut c_void,
    id: ParamID,
    value: ParamValue,
) -> tresult {
    let o = base!(P, this, i_controller);
    // The controller's copy only: the processor receives values through
    // `process` parameter changes, as VST3 requires.
    if let Some(i) = o.param_index(id) {
        let v = o.params[i].denormalize(value);
        o.values[i].store(v.to_bits(), Ordering::Relaxed);
        kResultOk
    } else if let Some(m) = o.midi_index(id) {
        o.midi[m].store(value.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
        kResultOk
    } else {
        kInvalidArgument
    }
}

unsafe extern "system" fn set_component_handler<P: Plugin>(
    this: *mut c_void,
    handler: *mut c_void,
) -> tresult {
    let o = base!(P, this, i_controller);
    if !handler.is_null() {
        // SAFETY: a valid handler; we keep a reference.
        unsafe { vcall!(handler, FUnknownVtbl, addRef) };
    }
    let old = o.handler.swap(handler, Ordering::AcqRel);
    if !old.is_null() {
        // SAFETY: releasing the reference we took earlier.
        unsafe { vcall!(old, FUnknownVtbl, release) };
    }
    kResultOk
}

unsafe extern "system" fn create_view(_: *mut c_void, _: *const c_char) -> *mut c_void {
    // No editor view: hosts show their generic parameter editor.
    null_mut()
}

unsafe extern "system" fn midi_assignment<P: Plugin>(
    this: *mut c_void,
    bus: i32,
    channel: i16,
    ctrl: i16,
    id: *mut ParamID,
) -> tresult {
    let o = base!(P, this, i_midi);
    if bus != 0
        || !(0..16).contains(&channel)
        || !(0..=kPitchBend).contains(&ctrl)
        || id.is_null()
        || o.midi.is_empty()
    {
        return kResultFalse;
    }
    let index =
        u32::try_from(channel).unwrap_or(0) * MIDI_PER_CHANNEL + u32::try_from(ctrl).unwrap_or(0);
    // SAFETY: a writable id.
    unsafe { *id = MIDI_PARAM_BASE + index };
    kResultOk
}

/// Build the static factory for the listed plugin types (what
/// [`export_vst3!`](crate::export_vst3) exports through
/// `GetPluginFactory`). Evaluates to a `&'static FactoryObject`.
#[macro_export]
macro_rules! vst3_factory {
    ($($t:ty),+ $(,)?) => {{
        use $crate::vst3::sys as __v;
        use ::std::ffi::{c_char, c_void};
        unsafe extern "system" fn __query(this: *mut c_void, iid: *const __v::TUID, obj: *mut *mut c_void) -> __v::tresult {
            if iid.is_null() || obj.is_null() {
                return __v::kInvalidArgument;
            }
            // SAFETY: valid pointers (COM contract).
            let iid = unsafe { &*iid };
            if *iid == __v::FUnknown_iid || *iid == __v::IPluginFactory_iid
                || *iid == __v::IPluginFactory2_iid || *iid == __v::IPluginFactory3_iid {
                unsafe { *obj = this };
                __v::kResultOk
            } else {
                unsafe { *obj = ::std::ptr::null_mut() };
                __v::kNoInterface
            }
        }
        // The factory is static: reference counting is a formality.
        unsafe extern "system" fn __add_ref(_: *mut c_void) -> u32 { 2 }
        unsafe extern "system" fn __release(_: *mut c_void) -> u32 { 1 }
        unsafe extern "system" fn __info(_: *mut c_void, info: *mut __v::PFactoryInfo) -> __v::tresult {
            if info.is_null() {
                return __v::kInvalidArgument;
            }
            let firsts: &[&'static $crate::PluginInfo] = &[$( <$t as $crate::Plugin>::info() ),+];
            // SAFETY: a writable info.
            let i = unsafe { &mut *info };
            $crate::vst3::factory_info(i, firsts[0]);
            __v::kResultOk
        }
        unsafe extern "system" fn __count(_: *mut c_void) -> i32 {
            [$( ::std::stringify!($t) ),+].len() as i32
        }
        unsafe extern "system" fn __class(_: *mut c_void, index: i32, info: *mut __v::PClassInfo) -> __v::tresult {
            let table: &[fn(&mut __v::PClassInfo)] = &[$( $crate::vst3::class_info::<$t> ),+];
            match (usize::try_from(index).ok().and_then(|i| table.get(i)), info.is_null()) {
                // SAFETY: a writable info.
                (Some(f), false) => { f(unsafe { &mut *info }); __v::kResultOk }
                _ => __v::kInvalidArgument,
            }
        }
        unsafe extern "system" fn __class2(_: *mut c_void, index: i32, info: *mut __v::PClassInfo2) -> __v::tresult {
            let table: &[fn(&mut __v::PClassInfo2)] = &[$( $crate::vst3::class_info2::<$t> ),+];
            match (usize::try_from(index).ok().and_then(|i| table.get(i)), info.is_null()) {
                // SAFETY: a writable info.
                (Some(f), false) => { f(unsafe { &mut *info }); __v::kResultOk }
                _ => __v::kInvalidArgument,
            }
        }
        unsafe extern "system" fn __classw(_: *mut c_void, index: i32, info: *mut __v::PClassInfoW) -> __v::tresult {
            let table: &[fn(&mut __v::PClassInfoW)] = &[$( $crate::vst3::class_info_w::<$t> ),+];
            match (usize::try_from(index).ok().and_then(|i| table.get(i)), info.is_null()) {
                // SAFETY: a writable info.
                (Some(f), false) => { f(unsafe { &mut *info }); __v::kResultOk }
                _ => __v::kInvalidArgument,
            }
        }
        unsafe extern "system" fn __create(_: *mut c_void, cid: *const c_char, iid: *const c_char, obj: *mut *mut c_void) -> __v::tresult {
            if cid.is_null() || obj.is_null() {
                return __v::kInvalidArgument;
            }
            // SAFETY: a 16-byte class id from the host.
            let cid = unsafe { &*cid.cast::<__v::TUID>() };
            $(
                if *cid == <$t as $crate::Plugin>::info().vst3_class_id {
                    // SAFETY: the caller's pointers (COM contract).
                    return unsafe { $crate::vst3::create::<$t>(iid, obj) };
                }
            )+
            // SAFETY: a writable out pointer.
            unsafe { *obj = ::std::ptr::null_mut() };
            __v::kInvalidArgument
        }
        unsafe extern "system" fn __context(_: *mut c_void, _: *mut c_void) -> __v::tresult {
            __v::kResultOk
        }
        static __VTBL: __v::IPluginFactory3Vtbl = __v::IPluginFactory3Vtbl {
            unknown: __v::FUnknownVtbl { queryInterface: __query, addRef: __add_ref, release: __release },
            getFactoryInfo: __info,
            countClasses: __count,
            getClassInfo: __class,
            createInstance: __create,
            getClassInfo2: __class2,
            getClassInfoUnicode: __classw,
            setHostContext: __context,
        };
        static __FACTORY: $crate::vst3::FactoryObject = $crate::vst3::FactoryObject(&raw const __VTBL);
        &__FACTORY
    }};
}

/// Fill the factory info from the first plugin's vendor.
pub fn factory_info(out: &mut PFactoryInfo, first: &crate::PluginInfo) {
    copy_c(&mut out.vendor, first.vendor);
    copy_c(&mut out.url, first.url);
    copy_c(&mut out.email, "");
    out.flags = kUnicode;
}

/// Export the listed plugin types as this library's VST3 module.
#[macro_export]
macro_rules! export_vst3 {
    ($($t:ty),+ $(,)?) => {
        #[unsafe(no_mangle)]
        #[allow(non_snake_case, reason = "the symbol VST3 hosts look up")]
        pub extern "system" fn GetPluginFactory() -> *mut ::std::ffi::c_void {
            let f: &'static $crate::vst3::FactoryObject = $crate::vst3_factory!($($t),+);
            ::std::ptr::from_ref(f).cast_mut().cast()
        }
        #[cfg(target_os = "linux")]
        #[unsafe(no_mangle)]
        #[allow(non_snake_case, reason = "the symbol VST3 hosts look up")]
        pub extern "C" fn ModuleEntry(_: *mut ::std::ffi::c_void) -> bool { true }
        #[cfg(target_os = "linux")]
        #[unsafe(no_mangle)]
        #[allow(non_snake_case, reason = "the symbol VST3 hosts look up")]
        pub extern "C" fn ModuleExit() -> bool { true }
        #[cfg(target_os = "macos")]
        #[unsafe(no_mangle)]
        #[allow(non_snake_case, reason = "the symbol VST3 hosts look up")]
        pub extern "C" fn bundleEntry(_: *mut ::std::ffi::c_void) -> bool { true }
        #[cfg(target_os = "macos")]
        #[unsafe(no_mangle)]
        #[allow(non_snake_case, reason = "the symbol VST3 hosts look up")]
        pub extern "C" fn bundleExit() -> bool { true }
        #[cfg(windows)]
        #[unsafe(no_mangle)]
        #[allow(non_snake_case, reason = "the symbol VST3 hosts look up")]
        pub extern "system" fn InitDll() -> bool { true }
        #[cfg(windows)]
        #[unsafe(no_mangle)]
        #[allow(non_snake_case, reason = "the symbol VST3 hosts look up")]
        pub extern "system" fn ExitDll() -> bool { true }
    };
}

#[cfg(test)]
mod tests {
    use super::host::{HostEvent, Module};
    use super::*;
    use crate::testing::{Gain, Synth};
    #[allow(unused_imports, reason = "Event is matched in some tests")]
    use crate::Event as _;

    fn module() -> Module {
        let f: &'static FactoryObject = crate::vst3_factory!(Gain, Synth);
        // SAFETY: a static, valid factory.
        unsafe { Module::from_factory(std::ptr::from_ref(f).cast_mut().cast(), "in-process") }
            .unwrap()
    }

    #[test]
    fn subcategories_follow_features() {
        assert_eq!(
            subcategories(Kind::Instrument, &["synthesizer", "stereo"]),
            "Instrument|Synth|Stereo"
        );
        assert_eq!(
            subcategories(Kind::Effect, &["reverb", "delay", "unknown"]),
            "Fx|Reverb|Delay"
        );
    }

    #[test]
    fn the_factory_describes_both_classes() {
        let m = module();
        let (vendor, flags) = m.factory_info();
        assert_eq!(vendor, "vieww");
        assert_eq!(flags & kUnicode, kUnicode);
        let classes = m.classes();
        assert_eq!(classes.len(), 2);
        assert_eq!(classes[0].name, "Test Gain");
        assert_eq!(classes[0].category, "Audio Module Class");
        assert_eq!(classes[0].subcategories, "Fx|Tools|Stereo");
        assert_eq!(classes[1].subcategories, "Instrument|Synth|Stereo");
        assert_eq!(classes[1].version, "0.3.1");
        assert_eq!(classes[1].sdk_version, kVstVersionString);
        assert_eq!(classes[0].cid, *b"vieww-test-gain!");
        assert!(m.instantiate(&[0; 16]).is_err());
    }

    #[test]
    fn an_effect_processes_with_sample_accurate_automation() {
        let m = module();
        let mut p = m.instantiate(b"vieww-test-gain!").unwrap();
        assert!(!p.split_controller(), "single component");
        assert_eq!(p.bus_channels(kAudio, kInput), vec![2]);
        assert_eq!(p.bus_channels(kAudio, kOutput), vec![2]);
        assert_eq!(p.bus_channels(kEvent, kInput), Vec::<i32>::new());
        let params = p.params();
        assert_eq!(params.len(), 2, "effects get no MIDI parameters");
        assert_eq!(params[0].title, "Gain");
        assert_eq!(params[0].units, "dB");
        assert_eq!(params[1].step_count, 1);
        assert!((params[0].default_normalized - 60.0 / 72.0).abs() < 1e-12);
        assert_eq!(
            p.value_to_text(1, 0.5).as_deref(),
            Some("-24.00"),
            "units come from ParameterInfo"
        );
        assert!((p.text_to_value(1, "-24 dB").unwrap() - 0.5).abs() < 1e-12);
        assert!(p.setup(48_000.0, 256));
        assert_eq!(p.latency(), 64);
        let input = vec![vec![1.0f32; 128]; 2];
        let half = (-6.020_599_913_279_624 + 60.0) / 72.0;
        let out = p.process(
            &input,
            2,
            128,
            &[HostEvent::Param {
                time: 32,
                id: 1,
                normalized: half,
            }],
            None,
        );
        assert_eq!(out[0][31], 1.0);
        assert!((out[1][32] - 0.5).abs() < 1e-5);
        assert!(
            (p.normalized(1) - half).abs() < 1e-12,
            "mirrored to the controller"
        );
    }

    #[test]
    fn state_moves_between_instances_and_the_controller_follows() {
        let m = module();
        let mut a = m.instantiate(b"vieww-test-gain!").unwrap();
        assert!(a.setup(48_000.0, 64));
        let _ = a.process(
            &[vec![0.0; 8], vec![0.0; 8]],
            2,
            8,
            &[HostEvent::Param {
                time: 0,
                id: 1,
                normalized: 0.25,
            }],
            None,
        );
        let state = a.get_state().unwrap();
        let mut b = m.instantiate(b"vieww-test-gain!").unwrap();
        assert!(b.set_state(&state));
        assert!((b.normalized(1) - 0.25).abs() < 1e-12);
        assert!(!b.set_state(b"junk"));
    }

    #[test]
    fn an_instrument_gets_notes_cc_parameters_and_context() {
        let m = module();
        let mut p = m.instantiate(b"vieww-test-synth").unwrap();
        assert_eq!(p.bus_channels(kEvent, kInput), vec![16]);
        assert!(p.bus_channels(kAudio, kInput).is_empty());
        let params = p.params();
        assert_eq!(params.len(), 1 + 16 * 130);
        let bend = params.iter().find(|q| q.title == "Pitch Bend [1]").unwrap();
        assert_eq!(bend.default_normalized, 0.5);
        assert!(bend.hidden);
        assert_eq!(p.midi_mapping(0, 0, 7), Some(MIDI_PARAM_BASE + 7));
        assert_eq!(
            p.midi_mapping(0, 2, 129),
            Some(MIDI_PARAM_BASE + 2 * 130 + 129)
        );
        assert_eq!(p.midi_mapping(0, 0, 200), None);
        assert!(p.setup(44_100.0, 512));
        let out = p.process(
            &[],
            2,
            256,
            &[
                HostEvent::NoteOn {
                    time: 64,
                    channel: 0,
                    pitch: 69,
                    velocity: 0.8,
                    note_id: 5,
                },
                HostEvent::Param {
                    time: 100,
                    id: MIDI_PARAM_BASE + 1,
                    normalized: 1.0,
                },
            ],
            Some((120.0, 8.0)),
        );
        assert!(out[0][..64].iter().all(|v| *v == 0.0));
        assert!(out[0][65..].iter().any(|v| v.abs() > 0.05));
        let out = p.process(
            &[],
            2,
            64,
            &[HostEvent::NoteOff {
                time: 0,
                channel: 0,
                pitch: 69,
                note_id: 5,
            }],
            None,
        );
        assert!(out[0].iter().all(|v| *v == 0.0));
    }

    #[test]
    fn process_context_becomes_transport() {
        let mut c: ProcessContext = unsafe { std::mem::zeroed() };
        c.state = kPlaying
            | kTempoValid
            | kProjectTimeMusicValid
            | kTimeSigValid
            | kCycleActive
            | kCycleValid;
        c.tempo = 98.0;
        c.projectTimeMusic = 16.5;
        c.timeSigNumerator = 6;
        c.timeSigDenominator = 8;
        c.cycleStartMusic = 8.0;
        c.cycleEndMusic = 24.0;
        c.sampleRate = 48_000.0;
        c.projectTimeSamples = 96_000;
        let t = transport(&c);
        assert!(t.playing && t.looping && !t.recording);
        assert_eq!(t.tempo, Some(98.0));
        assert_eq!(t.beats, Some(16.5));
        assert_eq!(t.time_signature, Some((6, 8)));
        assert_eq!(t.loop_beats, Some((8.0, 24.0)));
        assert_eq!(t.seconds, Some(2.0));
        assert_eq!(t.bar_start, None);
    }

    #[test]
    fn midi_parameter_values_become_midi_bytes() {
        assert_eq!(midi_bytes(7, 1.0), [0xB0, 7, 127]);
        assert_eq!(midi_bytes(130 * 3 + 128, 0.5), [0xD3, 64, 0]);
        assert_eq!(midi_bytes(129, 0.5), [0xE0, 0, 64]);
        assert_eq!(midi_bytes(129, 1.0), [0xE0, 0x7F, 0x7F]);
    }
}
