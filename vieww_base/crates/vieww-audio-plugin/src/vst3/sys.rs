//! The VST3 ABI, transcribed from Steinberg's interface headers (MIT,
//! github.com/steinbergmedia/vst3_pluginterfaces). VST3 is COM-shaped: an
//! interface pointer points at a pointer to a vtable whose first three
//! entries are `queryInterface`, `addRef`, `release`, followed by the
//! interface's own methods in declaration order (base interfaces first).
//! Calling convention: `PLUGIN_API` is `__stdcall` on Windows — Rust's
//! `extern "system"` — and the C convention elsewhere.

#![allow(
    non_camel_case_types,
    non_snake_case,
    missing_docs,
    reason = "C ABI names"
)]

use std::ffi::{c_char, c_void};

pub type tresult = i32;
pub type TUID = [u8; 16];
pub type ParamID = u32;
pub type ParamValue = f64;
pub type TChar = u16;
pub type String128 = [TChar; 128];
pub type SpeakerArrangement = u64;
pub type TBool = u8;

// Result codes: COM's on Windows, a small enum elsewhere (funknown.h).
#[cfg(windows)]
mod codes {
    pub const kNoInterface: i32 = 0x8000_4002_u32 as i32;
    pub const kResultOk: i32 = 0;
    pub const kResultFalse: i32 = 1;
    pub const kInvalidArgument: i32 = 0x8007_0057_u32 as i32;
    pub const kNotImplemented: i32 = 0x8000_4001_u32 as i32;
    pub const kInternalError: i32 = 0x8000_4005_u32 as i32;
    pub const kNotInitialized: i32 = 0x8000_FFFF_u32 as i32;
    pub const kOutOfMemory: i32 = 0x8007_000E_u32 as i32;
}
#[cfg(not(windows))]
mod codes {
    pub const kNoInterface: i32 = -1;
    pub const kResultOk: i32 = 0;
    pub const kResultFalse: i32 = 1;
    pub const kInvalidArgument: i32 = 2;
    pub const kNotImplemented: i32 = 3;
    pub const kInternalError: i32 = 4;
    pub const kNotInitialized: i32 = 5;
    pub const kOutOfMemory: i32 = 6;
}
pub use codes::*;
pub const kResultTrue: i32 = kResultOk;

/// `INLINE_UID(l1, l2, l3, l4)`: COM byte order on Windows (the first
/// three fields little-endian), plain big-endian elsewhere.
#[must_use]
pub const fn uid(l1: u32, l2: u32, l3: u32, l4: u32) -> TUID {
    let (a, b, c, d) = (
        l1.to_be_bytes(),
        l2.to_be_bytes(),
        l3.to_be_bytes(),
        l4.to_be_bytes(),
    );
    if cfg!(windows) {
        [
            a[3], a[2], a[1], a[0], b[1], b[0], b[3], b[2], c[0], c[1], c[2], c[3], d[0], d[1],
            d[2], d[3],
        ]
    } else {
        [
            a[0], a[1], a[2], a[3], b[0], b[1], b[2], b[3], c[0], c[1], c[2], c[3], d[0], d[1],
            d[2], d[3],
        ]
    }
}

pub const FUnknown_iid: TUID = uid(0x0000_0000, 0x0000_0000, 0xC000_0000, 0x0000_0046);
pub const IPluginBase_iid: TUID = uid(0x2288_8DDB, 0x156E_45AE, 0x8358_B348, 0x0819_0625);
pub const IPluginFactory_iid: TUID = uid(0x7A4D_811C, 0x5211_4A1F, 0xAED9_D2EE, 0x0B43_BF9F);
pub const IPluginFactory2_iid: TUID = uid(0x0007_B650, 0xF24B_4C0B, 0xA464_EDB9, 0xF00B_2ABB);
pub const IPluginFactory3_iid: TUID = uid(0x4555_A2AB, 0xC123_4E57, 0x9B12_2910, 0x3687_8931);
pub const IBStream_iid: TUID = uid(0xC3BF_6EA2, 0x3099_4752, 0x9B6B_F990, 0x1EE3_3E9B);
pub const IComponent_iid: TUID = uid(0xE831_FF31, 0xF2D5_4301, 0x928E_BBEE, 0x2569_7802);
pub const IAudioProcessor_iid: TUID = uid(0x4204_3F99, 0xB7DA_453C, 0xA569_E79D, 0x9AAE_C33D);
pub const IProcessContextRequirements_iid: TUID =
    uid(0x2A65_4303, 0xEF76_4E3D, 0x95B5_FE83, 0x730E_F6D0);
pub const IEditController_iid: TUID = uid(0xDCD7_BBE3, 0x7742_448D, 0xA874_AACC, 0x979C_759E);
pub const IMidiMapping_iid: TUID = uid(0xDF0F_F9F7, 0x49B7_4669, 0xB63A_B732, 0x7ADB_F5E5);
pub const IComponentHandler_iid: TUID = uid(0x93A0_BEA3, 0x0BD0_45DB, 0x8E89_0B0C, 0xC1E4_6AC6);
pub const IEventList_iid: TUID = uid(0x3A2C_4214, 0x3463_49FE, 0xB2C4_F397, 0xB969_5A44);
pub const IParamValueQueue_iid: TUID = uid(0x0126_3A18, 0xED07_4F6F, 0x98C9_D356, 0x4686_F9BA);
pub const IParameterChanges_iid: TUID = uid(0xA477_9663, 0x0BB6_4A56, 0xB443_84A8, 0x466F_EB9D);
pub const IHostApplication_iid: TUID = uid(0x58E5_95CC, 0xDB2D_4969, 0x8B6A_AF8C, 0x36A6_64E5);
pub const IConnectionPoint_iid: TUID = uid(0x70A4_156F, 0x6E6E_4026, 0x9891_48BF, 0xAA60_D8D1);

pub type QueryInterface = unsafe extern "system" fn(
    this: *mut c_void,
    iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult;
pub type AddRef = unsafe extern "system" fn(this: *mut c_void) -> u32;
pub type Release = unsafe extern "system" fn(this: *mut c_void) -> u32;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FUnknownVtbl {
    pub queryInterface: QueryInterface,
    pub addRef: AddRef,
    pub release: Release,
}

// ---- factory -------------------------------------------------------------

pub const kUnicode: i32 = 16;
pub const kManyInstances: i32 = 0x7FFF_FFFF;
pub const kVstAudioEffectClass: &[u8] = b"Audio Module Class\0";
pub const kVstComponentControllerClass: &[u8] = b"Component Controller Class\0";
pub const kVstVersionString: &str = "VST 3.8.1";

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PFactoryInfo {
    pub vendor: [c_char; 64],
    pub url: [c_char; 256],
    pub email: [c_char; 128],
    pub flags: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PClassInfo {
    pub cid: TUID,
    pub cardinality: i32,
    pub category: [c_char; 32],
    pub name: [c_char; 64],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PClassInfo2 {
    pub cid: TUID,
    pub cardinality: i32,
    pub category: [c_char; 32],
    pub name: [c_char; 64],
    pub classFlags: u32,
    pub subCategories: [c_char; 128],
    pub vendor: [c_char; 64],
    pub version: [c_char; 64],
    pub sdkVersion: [c_char; 64],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PClassInfoW {
    pub cid: TUID,
    pub cardinality: i32,
    pub category: [c_char; 32],
    pub name: [TChar; 64],
    pub classFlags: u32,
    pub subCategories: [c_char; 128],
    pub vendor: [TChar; 64],
    pub version: [TChar; 64],
    pub sdkVersion: [TChar; 64],
}

macro_rules! opaque_debug {
    ($($t:ty),*) => {$(
        impl std::fmt::Debug for $t {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(stringify!($t))
            }
        }
    )*};
}
opaque_debug!(
    PFactoryInfo,
    PClassInfo,
    PClassInfo2,
    PClassInfoW,
    BusInfo,
    ParameterInfo,
    Event,
    EventBody,
    ProcessContext
);

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IPluginFactory3Vtbl {
    pub unknown: FUnknownVtbl,
    pub getFactoryInfo:
        unsafe extern "system" fn(this: *mut c_void, info: *mut PFactoryInfo) -> tresult,
    pub countClasses: unsafe extern "system" fn(this: *mut c_void) -> i32,
    pub getClassInfo:
        unsafe extern "system" fn(this: *mut c_void, index: i32, info: *mut PClassInfo) -> tresult,
    pub createInstance: unsafe extern "system" fn(
        this: *mut c_void,
        cid: *const c_char,
        iid: *const c_char,
        obj: *mut *mut c_void,
    ) -> tresult,
    pub getClassInfo2:
        unsafe extern "system" fn(this: *mut c_void, index: i32, info: *mut PClassInfo2) -> tresult,
    pub getClassInfoUnicode:
        unsafe extern "system" fn(this: *mut c_void, index: i32, info: *mut PClassInfoW) -> tresult,
    pub setHostContext:
        unsafe extern "system" fn(this: *mut c_void, context: *mut c_void) -> tresult,
}

// ---- component ----------------------------------------------------------

pub const kAudio: i32 = 0;
pub const kEvent: i32 = 1;
pub const kInput: i32 = 0;
pub const kOutput: i32 = 1;
pub const kMain: i32 = 0;
pub const kAux: i32 = 1;
pub const kDefaultActive: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BusInfo {
    pub mediaType: i32,
    pub direction: i32,
    pub channelCount: i32,
    pub name: String128,
    pub busType: i32,
    pub flags: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RoutingInfo {
    pub mediaType: i32,
    pub busIndex: i32,
    pub channel: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IComponentVtbl {
    pub unknown: FUnknownVtbl,
    pub initialize: unsafe extern "system" fn(this: *mut c_void, context: *mut c_void) -> tresult,
    pub terminate: unsafe extern "system" fn(this: *mut c_void) -> tresult,
    pub getControllerClassId:
        unsafe extern "system" fn(this: *mut c_void, cid: *mut TUID) -> tresult,
    pub setIoMode: unsafe extern "system" fn(this: *mut c_void, mode: i32) -> tresult,
    pub getBusCount: unsafe extern "system" fn(this: *mut c_void, ty: i32, dir: i32) -> i32,
    pub getBusInfo: unsafe extern "system" fn(
        this: *mut c_void,
        ty: i32,
        dir: i32,
        index: i32,
        bus: *mut BusInfo,
    ) -> tresult,
    pub getRoutingInfo: unsafe extern "system" fn(
        this: *mut c_void,
        inInfo: *mut RoutingInfo,
        outInfo: *mut RoutingInfo,
    ) -> tresult,
    pub activateBus: unsafe extern "system" fn(
        this: *mut c_void,
        ty: i32,
        dir: i32,
        index: i32,
        state: TBool,
    ) -> tresult,
    pub setActive: unsafe extern "system" fn(this: *mut c_void, state: TBool) -> tresult,
    pub setState: unsafe extern "system" fn(this: *mut c_void, state: *mut c_void) -> tresult,
    pub getState: unsafe extern "system" fn(this: *mut c_void, state: *mut c_void) -> tresult,
}

// ---- processor ----------------------------------------------------------

pub const kRealtime: i32 = 0;
pub const kPrefetch: i32 = 1;
pub const kOffline: i32 = 2;
pub const kSample32: i32 = 0;
pub const kSample64: i32 = 1;
pub const kSpeakerL: u64 = 1;
pub const kSpeakerR: u64 = 2;
pub const kSpeakerM: u64 = 1 << 19;
pub const kStereo: u64 = kSpeakerL | kSpeakerR;
pub const kMono: u64 = kSpeakerM;
pub const kNoTail: u32 = 0;
pub const kInfiniteTail: u32 = u32::MAX;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ProcessSetup {
    pub processMode: i32,
    pub symbolicSampleSize: i32,
    pub maxSamplesPerBlock: i32,
    pub sampleRate: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AudioBusBuffers {
    pub numChannels: i32,
    pub silenceFlags: u64,
    /// The union of `Sample32**` and `Sample64**`.
    pub channelBuffers: *mut *mut c_void,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ProcessData {
    pub processMode: i32,
    pub symbolicSampleSize: i32,
    pub numSamples: i32,
    pub numInputs: i32,
    pub numOutputs: i32,
    pub inputs: *mut AudioBusBuffers,
    pub outputs: *mut AudioBusBuffers,
    pub inputParameterChanges: *mut c_void,
    pub outputParameterChanges: *mut c_void,
    pub inputEvents: *mut c_void,
    pub outputEvents: *mut c_void,
    pub processContext: *mut ProcessContext,
}

pub const kPlaying: u32 = 1 << 1;
pub const kCycleActive: u32 = 1 << 2;
pub const kRecording: u32 = 1 << 3;
pub const kSystemTimeValid: u32 = 1 << 8;
pub const kProjectTimeMusicValid: u32 = 1 << 9;
pub const kTempoValid: u32 = 1 << 10;
pub const kBarPositionValid: u32 = 1 << 11;
pub const kCycleValid: u32 = 1 << 12;
pub const kTimeSigValid: u32 = 1 << 13;
pub const kContTimeValid: u32 = 1 << 17;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Chord {
    pub keyNote: u8,
    pub rootNote: u8,
    pub chordMask: i16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameRate {
    pub framesPerSecond: u32,
    pub flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProcessContext {
    pub state: u32,
    pub sampleRate: f64,
    pub projectTimeSamples: i64,
    pub systemTime: i64,
    pub continousTimeSamples: i64,
    pub projectTimeMusic: f64,
    pub barPositionMusic: f64,
    pub cycleStartMusic: f64,
    pub cycleEndMusic: f64,
    pub tempo: f64,
    pub timeSigNumerator: i32,
    pub timeSigDenominator: i32,
    pub chord: Chord,
    pub smpteOffsetSubframes: i32,
    pub frameRate: FrameRate,
    pub samplesToNextClock: i32,
}

pub const kNeedTransportState: u32 = 1 << 10;
pub const kNeedTempo: u32 = 1 << 6;
pub const kNeedTimeSignature: u32 = 1 << 7;
pub const kNeedProjectTimeMusic: u32 = 1 << 2;
pub const kNeedBarPositionMusic: u32 = 1 << 3;
pub const kNeedCycleMusic: u32 = 1 << 4;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IAudioProcessorVtbl {
    pub unknown: FUnknownVtbl,
    pub setBusArrangements: unsafe extern "system" fn(
        this: *mut c_void,
        inputs: *mut SpeakerArrangement,
        numIns: i32,
        outputs: *mut SpeakerArrangement,
        numOuts: i32,
    ) -> tresult,
    pub getBusArrangement: unsafe extern "system" fn(
        this: *mut c_void,
        dir: i32,
        index: i32,
        arr: *mut SpeakerArrangement,
    ) -> tresult,
    pub canProcessSampleSize: unsafe extern "system" fn(this: *mut c_void, size: i32) -> tresult,
    pub getLatencySamples: unsafe extern "system" fn(this: *mut c_void) -> u32,
    pub setupProcessing:
        unsafe extern "system" fn(this: *mut c_void, setup: *mut ProcessSetup) -> tresult,
    pub setProcessing: unsafe extern "system" fn(this: *mut c_void, state: TBool) -> tresult,
    pub process: unsafe extern "system" fn(this: *mut c_void, data: *mut ProcessData) -> tresult,
    pub getTailSamples: unsafe extern "system" fn(this: *mut c_void) -> u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IProcessContextRequirementsVtbl {
    pub unknown: FUnknownVtbl,
    pub getProcessContextRequirements: unsafe extern "system" fn(this: *mut c_void) -> u32,
}

// ---- controller ---------------------------------------------------------

pub const kCanAutomate: i32 = 1 << 0;
pub const kIsReadOnly: i32 = 1 << 1;
pub const kIsWrapAround: i32 = 1 << 2;
pub const kIsList: i32 = 1 << 3;
pub const kIsHidden: i32 = 1 << 4;
pub const kIsProgramChange: i32 = 1 << 15;
pub const kIsBypass: i32 = 1 << 16;
pub const kRootUnitId: i32 = 0;

pub const kReloadComponent: i32 = 1 << 0;
pub const kIoChanged: i32 = 1 << 1;
pub const kParamValuesChanged: i32 = 1 << 2;
pub const kLatencyChanged: i32 = 1 << 3;
pub const kParamTitlesChanged: i32 = 1 << 4;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ParameterInfo {
    pub id: ParamID,
    pub title: String128,
    pub shortTitle: String128,
    pub units: String128,
    pub stepCount: i32,
    pub defaultNormalizedValue: ParamValue,
    pub unitId: i32,
    pub flags: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IEditControllerVtbl {
    pub unknown: FUnknownVtbl,
    pub initialize: unsafe extern "system" fn(this: *mut c_void, context: *mut c_void) -> tresult,
    pub terminate: unsafe extern "system" fn(this: *mut c_void) -> tresult,
    pub setComponentState:
        unsafe extern "system" fn(this: *mut c_void, state: *mut c_void) -> tresult,
    pub setState: unsafe extern "system" fn(this: *mut c_void, state: *mut c_void) -> tresult,
    pub getState: unsafe extern "system" fn(this: *mut c_void, state: *mut c_void) -> tresult,
    pub getParameterCount: unsafe extern "system" fn(this: *mut c_void) -> i32,
    pub getParameterInfo: unsafe extern "system" fn(
        this: *mut c_void,
        index: i32,
        info: *mut ParameterInfo,
    ) -> tresult,
    pub getParamStringByValue: unsafe extern "system" fn(
        this: *mut c_void,
        id: ParamID,
        value: ParamValue,
        string: *mut TChar,
    ) -> tresult,
    pub getParamValueByString: unsafe extern "system" fn(
        this: *mut c_void,
        id: ParamID,
        string: *mut TChar,
        value: *mut ParamValue,
    ) -> tresult,
    pub normalizedParamToPlain:
        unsafe extern "system" fn(this: *mut c_void, id: ParamID, value: ParamValue) -> ParamValue,
    pub plainParamToNormalized:
        unsafe extern "system" fn(this: *mut c_void, id: ParamID, value: ParamValue) -> ParamValue,
    pub getParamNormalized: unsafe extern "system" fn(this: *mut c_void, id: ParamID) -> ParamValue,
    pub setParamNormalized:
        unsafe extern "system" fn(this: *mut c_void, id: ParamID, value: ParamValue) -> tresult,
    pub setComponentHandler:
        unsafe extern "system" fn(this: *mut c_void, handler: *mut c_void) -> tresult,
    pub createView:
        unsafe extern "system" fn(this: *mut c_void, name: *const c_char) -> *mut c_void,
}

pub const kAfterTouch: i16 = 128;
pub const kPitchBend: i16 = 129;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IMidiMappingVtbl {
    pub unknown: FUnknownVtbl,
    pub getMidiControllerAssignment: unsafe extern "system" fn(
        this: *mut c_void,
        bus: i32,
        channel: i16,
        ctrl: i16,
        id: *mut ParamID,
    ) -> tresult,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IComponentHandlerVtbl {
    pub unknown: FUnknownVtbl,
    pub beginEdit: unsafe extern "system" fn(this: *mut c_void, id: ParamID) -> tresult,
    pub performEdit:
        unsafe extern "system" fn(this: *mut c_void, id: ParamID, value: ParamValue) -> tresult,
    pub endEdit: unsafe extern "system" fn(this: *mut c_void, id: ParamID) -> tresult,
    pub restartComponent: unsafe extern "system" fn(this: *mut c_void, flags: i32) -> tresult,
}

// ---- events and parameter changes ---------------------------------------

pub const kNoteOnEvent: u16 = 0;
pub const kNoteOffEvent: u16 = 1;
pub const kDataEvent: u16 = 2;
pub const kPolyPressureEvent: u16 = 3;
pub const kNoteExpressionValueEvent: u16 = 4;
pub const kIsLive: u16 = 1;

pub const kVolumeTypeID: u32 = 0;
pub const kPanTypeID: u32 = 1;
pub const kTuningTypeID: u32 = 2;
pub const kVibratoTypeID: u32 = 3;
pub const kExpressionTypeID: u32 = 4;
pub const kBrightnessTypeID: u32 = 5;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NoteOnEvent {
    pub channel: i16,
    pub pitch: i16,
    pub tuning: f32,
    pub velocity: f32,
    pub length: i32,
    pub noteId: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NoteOffEvent {
    pub channel: i16,
    pub pitch: i16,
    pub velocity: f32,
    pub noteId: i32,
    pub tuning: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PolyPressureEvent {
    pub channel: i16,
    pub pitch: i16,
    pub pressure: f32,
    pub noteId: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NoteExpressionValueEvent {
    pub typeId: u32,
    pub noteId: i32,
    pub value: f64,
}

/// The event payload union; its size is the largest member's
/// (`NoteExpressionTextEvent`, 24 bytes on 64-bit).
#[repr(C)]
#[derive(Clone, Copy)]
pub union EventBody {
    pub noteOn: NoteOnEvent,
    pub noteOff: NoteOffEvent,
    pub polyPressure: PolyPressureEvent,
    pub noteExpressionValue: NoteExpressionValueEvent,
    pub _size: [*const c_void; 3],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Event {
    pub busIndex: i32,
    pub sampleOffset: i32,
    pub ppqPosition: f64,
    pub flags: u16,
    pub type_: u16,
    pub body: EventBody,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IEventListVtbl {
    pub unknown: FUnknownVtbl,
    pub getEventCount: unsafe extern "system" fn(this: *mut c_void) -> i32,
    pub getEvent:
        unsafe extern "system" fn(this: *mut c_void, index: i32, e: *mut Event) -> tresult,
    pub addEvent: unsafe extern "system" fn(this: *mut c_void, e: *mut Event) -> tresult,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IParamValueQueueVtbl {
    pub unknown: FUnknownVtbl,
    pub getParameterId: unsafe extern "system" fn(this: *mut c_void) -> ParamID,
    pub getPointCount: unsafe extern "system" fn(this: *mut c_void) -> i32,
    pub getPoint: unsafe extern "system" fn(
        this: *mut c_void,
        index: i32,
        offset: *mut i32,
        value: *mut ParamValue,
    ) -> tresult,
    pub addPoint: unsafe extern "system" fn(
        this: *mut c_void,
        offset: i32,
        value: ParamValue,
        index: *mut i32,
    ) -> tresult,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IParameterChangesVtbl {
    pub unknown: FUnknownVtbl,
    pub getParameterCount: unsafe extern "system" fn(this: *mut c_void) -> i32,
    pub getParameterData: unsafe extern "system" fn(this: *mut c_void, index: i32) -> *mut c_void,
    pub addParameterData: unsafe extern "system" fn(
        this: *mut c_void,
        id: *const ParamID,
        index: *mut i32,
    ) -> *mut c_void,
}

// ---- streams and host ----------------------------------------------------

pub const kIBSeekSet: i32 = 0;
pub const kIBSeekCur: i32 = 1;
pub const kIBSeekEnd: i32 = 2;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IBStreamVtbl {
    pub unknown: FUnknownVtbl,
    pub read: unsafe extern "system" fn(
        this: *mut c_void,
        buffer: *mut c_void,
        n: i32,
        read: *mut i32,
    ) -> tresult,
    pub write: unsafe extern "system" fn(
        this: *mut c_void,
        buffer: *mut c_void,
        n: i32,
        written: *mut i32,
    ) -> tresult,
    pub seek: unsafe extern "system" fn(
        this: *mut c_void,
        pos: i64,
        mode: i32,
        result: *mut i64,
    ) -> tresult,
    pub tell: unsafe extern "system" fn(this: *mut c_void, pos: *mut i64) -> tresult,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IHostApplicationVtbl {
    pub unknown: FUnknownVtbl,
    pub getName: unsafe extern "system" fn(this: *mut c_void, name: *mut TChar) -> tresult,
    pub createInstance: unsafe extern "system" fn(
        this: *mut c_void,
        cid: *mut TUID,
        iid: *mut TUID,
        obj: *mut *mut c_void,
    ) -> tresult,
}

/// Call through an interface pointer's vtable: `vcall!(ptr, Vtbl, method, args…)`.
#[macro_export]
#[doc(hidden)]
macro_rules! vcall {
    ($p:expr, $vt:ty, $m:ident $(, $a:expr)* $(,)?) => {{
        let __p: *mut ::std::ffi::c_void = ($p).cast();
        let __vt = *__p.cast::<*const $vt>();
        ((*__vt).$m)(__p $(, $a)*)
    }};
}

/// UTF-16 into a fixed buffer, NUL-terminated.
pub fn write_tchar(dst: &mut [TChar], s: &str) {
    if dst.is_empty() {
        return;
    }
    let mut n = 0;
    for u in s.encode_utf16() {
        if n + 1 >= dst.len() {
            break;
        }
        dst[n] = u;
        n += 1;
    }
    // Never leave half a surrogate pair.
    if n > 0 && (0xD800..0xDC00).contains(&dst[n - 1]) {
        n -= 1;
    }
    dst[n] = 0;
}

/// A NUL-terminated UTF-16 buffer as a string.
#[must_use]
pub fn read_tchar(src: &[TChar]) -> String {
    let n = src.iter().position(|c| *c == 0).unwrap_or(src.len());
    String::from_utf16_lossy(&src[..n])
}

#[cfg(all(test, target_pointer_width = "64"))]
mod layout {
    //! Every number printed by a C++ program compiled against the VST3
    //! interface headers (x86-64 Linux); `tools/check-plugins.sh` reruns it.
    use super::*;
    use std::mem::{offset_of, size_of};

    #[test]
    fn sizes_and_offsets_match_the_headers() {
        assert_eq!(size_of::<PFactoryInfo>(), 452);
        assert_eq!(offset_of!(PFactoryInfo, flags), 448);
        assert_eq!(size_of::<PClassInfo>(), 116);
        assert_eq!(size_of::<PClassInfo2>(), 440);
        assert_eq!(offset_of!(PClassInfo2, subCategories), 120);
        assert_eq!(offset_of!(PClassInfo2, sdkVersion), 376);
        assert_eq!(size_of::<PClassInfoW>(), 696);
        assert_eq!(size_of::<BusInfo>(), 276);
        assert_eq!(offset_of!(BusInfo, busType), 268);
        assert_eq!(size_of::<RoutingInfo>(), 12);
        assert_eq!(size_of::<ProcessSetup>(), 24);
        assert_eq!(offset_of!(ProcessSetup, sampleRate), 16);
        assert_eq!(size_of::<AudioBusBuffers>(), 24);
        assert_eq!(offset_of!(AudioBusBuffers, channelBuffers), 16);
        assert_eq!(size_of::<ProcessData>(), 80);
        assert_eq!(offset_of!(ProcessData, inputs), 24);
        assert_eq!(offset_of!(ProcessData, inputEvents), 56);
        assert_eq!(offset_of!(ProcessData, processContext), 72);
        assert_eq!(size_of::<ParameterInfo>(), 792);
        assert_eq!(offset_of!(ParameterInfo, stepCount), 772);
        assert_eq!(offset_of!(ParameterInfo, defaultNormalizedValue), 776);
        assert_eq!(offset_of!(ParameterInfo, flags), 788);
        assert_eq!(size_of::<Event>(), 48);
        assert_eq!(offset_of!(Event, type_), 18);
        assert_eq!(offset_of!(Event, body), 24);
        assert_eq!(size_of::<NoteOnEvent>(), 20);
        assert_eq!(offset_of!(NoteOnEvent, noteId), 16);
        assert_eq!(size_of::<NoteOffEvent>(), 16);
        assert_eq!(offset_of!(NoteOffEvent, tuning), 12);
        assert_eq!(size_of::<PolyPressureEvent>(), 12);
        assert_eq!(size_of::<NoteExpressionValueEvent>(), 16);
        assert_eq!(size_of::<ProcessContext>(), 112);
        assert_eq!(offset_of!(ProcessContext, tempo), 72);
        assert_eq!(offset_of!(ProcessContext, chord), 88);
        assert_eq!(offset_of!(ProcessContext, frameRate), 96);
        assert_eq!(offset_of!(ProcessContext, samplesToNextClock), 104);
    }

    #[test]
    fn uids_use_the_platform_byte_order() {
        let u = uid(0x0102_0304, 0x0506_0708, 0x090A_0B0C, 0x0D0E_0F10);
        if cfg!(windows) {
            assert_eq!(u, [4, 3, 2, 1, 6, 5, 8, 7, 9, 10, 11, 12, 13, 14, 15, 16]);
        } else {
            assert_eq!(u, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
        }
    }

    #[test]
    fn utf16_buffers() {
        let mut b = [0u16; 4];
        write_tchar(&mut b, "a😀bc");
        assert_eq!(read_tchar(&b), "a😀");
        write_tchar(&mut b, "ab😀");
        assert_eq!(read_tchar(&b), "ab", "no half surrogate");
    }
}
