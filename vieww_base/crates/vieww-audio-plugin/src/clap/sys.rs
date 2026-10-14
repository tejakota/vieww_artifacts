//! The CLAP ABI, transcribed from the CLAP 1.2 headers (MIT,
//! github.com/free-audio/clap). Field order, types and constants are the
//! headers'; names keep their C spelling so the two can be read side by
//! side.

#![allow(non_camel_case_types, missing_docs, reason = "C ABI names")]

use std::ffi::{c_char, c_void};

pub type clap_id = u32;
pub const CLAP_INVALID_ID: clap_id = u32::MAX;
pub const CLAP_NAME_SIZE: usize = 256;
pub const CLAP_PATH_SIZE: usize = 1024;
pub type clap_beattime = i64;
pub type clap_sectime = i64;
pub const CLAP_BEATTIME_FACTOR: i64 = 1 << 31;
pub const CLAP_SECTIME_FACTOR: i64 = 1 << 31;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct clap_version {
    pub major: u32,
    pub minor: u32,
    pub revision: u32,
}

pub const CLAP_VERSION: clap_version = clap_version {
    major: 1,
    minor: 2,
    revision: 2,
};

#[repr(C)]
#[derive(Debug)]
pub struct clap_plugin_entry {
    pub clap_version: clap_version,
    pub init: Option<unsafe extern "C" fn(plugin_path: *const c_char) -> bool>,
    pub deinit: Option<unsafe extern "C" fn()>,
    pub get_factory: Option<unsafe extern "C" fn(factory_id: *const c_char) -> *const c_void>,
}

pub const CLAP_PLUGIN_FACTORY_ID: &[u8] = b"clap.plugin-factory\0";

#[repr(C)]
#[derive(Debug)]
pub struct clap_plugin_factory {
    pub get_plugin_count: Option<unsafe extern "C" fn(factory: *const clap_plugin_factory) -> u32>,
    pub get_plugin_descriptor: Option<
        unsafe extern "C" fn(
            factory: *const clap_plugin_factory,
            index: u32,
        ) -> *const clap_plugin_descriptor,
    >,
    pub create_plugin: Option<
        unsafe extern "C" fn(
            factory: *const clap_plugin_factory,
            host: *const clap_host,
            plugin_id: *const c_char,
        ) -> *const clap_plugin,
    >,
}

#[repr(C)]
#[derive(Debug)]
pub struct clap_plugin_descriptor {
    pub clap_version: clap_version,
    pub id: *const c_char,
    pub name: *const c_char,
    pub vendor: *const c_char,
    pub url: *const c_char,
    pub manual_url: *const c_char,
    pub support_url: *const c_char,
    pub version: *const c_char,
    pub description: *const c_char,
    pub features: *const *const c_char,
}

#[repr(C)]
#[derive(Debug)]
pub struct clap_plugin {
    pub desc: *const clap_plugin_descriptor,
    pub plugin_data: *mut c_void,
    pub init: Option<unsafe extern "C" fn(plugin: *const clap_plugin) -> bool>,
    pub destroy: Option<unsafe extern "C" fn(plugin: *const clap_plugin)>,
    pub activate: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin,
            sample_rate: f64,
            min_frames_count: u32,
            max_frames_count: u32,
        ) -> bool,
    >,
    pub deactivate: Option<unsafe extern "C" fn(plugin: *const clap_plugin)>,
    pub start_processing: Option<unsafe extern "C" fn(plugin: *const clap_plugin) -> bool>,
    pub stop_processing: Option<unsafe extern "C" fn(plugin: *const clap_plugin)>,
    pub reset: Option<unsafe extern "C" fn(plugin: *const clap_plugin)>,
    pub process: Option<
        unsafe extern "C" fn(plugin: *const clap_plugin, process: *const clap_process) -> i32,
    >,
    pub get_extension: Option<
        unsafe extern "C" fn(plugin: *const clap_plugin, id: *const c_char) -> *const c_void,
    >,
    pub on_main_thread: Option<unsafe extern "C" fn(plugin: *const clap_plugin)>,
}

#[repr(C)]
#[derive(Debug)]
pub struct clap_host {
    pub clap_version: clap_version,
    pub host_data: *mut c_void,
    pub name: *const c_char,
    pub vendor: *const c_char,
    pub url: *const c_char,
    pub version: *const c_char,
    pub get_extension: Option<
        unsafe extern "C" fn(host: *const clap_host, extension_id: *const c_char) -> *const c_void,
    >,
    pub request_restart: Option<unsafe extern "C" fn(host: *const clap_host)>,
    pub request_process: Option<unsafe extern "C" fn(host: *const clap_host)>,
    pub request_callback: Option<unsafe extern "C" fn(host: *const clap_host)>,
}

pub const CLAP_PROCESS_ERROR: i32 = 0;
pub const CLAP_PROCESS_CONTINUE: i32 = 1;
pub const CLAP_PROCESS_CONTINUE_IF_NOT_QUIET: i32 = 2;
pub const CLAP_PROCESS_TAIL: i32 = 3;
pub const CLAP_PROCESS_SLEEP: i32 = 4;

#[repr(C)]
#[derive(Debug)]
pub struct clap_process {
    pub steady_time: i64,
    pub frames_count: u32,
    pub transport: *const clap_event_transport,
    pub audio_inputs: *const clap_audio_buffer,
    pub audio_outputs: *mut clap_audio_buffer,
    pub audio_inputs_count: u32,
    pub audio_outputs_count: u32,
    pub in_events: *const clap_input_events,
    pub out_events: *const clap_output_events,
}

#[repr(C)]
#[derive(Debug)]
pub struct clap_audio_buffer {
    pub data32: *mut *mut f32,
    pub data64: *mut *mut f64,
    pub channel_count: u32,
    pub latency: u32,
    pub constant_mask: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct clap_event_header {
    pub size: u32,
    pub time: u32,
    pub space_id: u16,
    pub type_: u16,
    pub flags: u32,
}

pub const CLAP_CORE_EVENT_SPACE_ID: u16 = 0;
pub const CLAP_EVENT_IS_LIVE: u32 = 1;
pub const CLAP_EVENT_DONT_RECORD: u32 = 2;

pub const CLAP_EVENT_NOTE_ON: u16 = 0;
pub const CLAP_EVENT_NOTE_OFF: u16 = 1;
pub const CLAP_EVENT_NOTE_CHOKE: u16 = 2;
pub const CLAP_EVENT_NOTE_END: u16 = 3;
pub const CLAP_EVENT_NOTE_EXPRESSION: u16 = 4;
pub const CLAP_EVENT_PARAM_VALUE: u16 = 5;
pub const CLAP_EVENT_PARAM_MOD: u16 = 6;
pub const CLAP_EVENT_PARAM_GESTURE_BEGIN: u16 = 7;
pub const CLAP_EVENT_PARAM_GESTURE_END: u16 = 8;
pub const CLAP_EVENT_TRANSPORT: u16 = 9;
pub const CLAP_EVENT_MIDI: u16 = 10;
pub const CLAP_EVENT_MIDI_SYSEX: u16 = 11;
pub const CLAP_EVENT_MIDI2: u16 = 12;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct clap_event_note {
    pub header: clap_event_header,
    pub note_id: i32,
    pub port_index: i16,
    pub channel: i16,
    pub key: i16,
    pub velocity: f64,
}

pub const CLAP_NOTE_EXPRESSION_VOLUME: i32 = 0;
pub const CLAP_NOTE_EXPRESSION_PAN: i32 = 1;
pub const CLAP_NOTE_EXPRESSION_TUNING: i32 = 2;
pub const CLAP_NOTE_EXPRESSION_VIBRATO: i32 = 3;
pub const CLAP_NOTE_EXPRESSION_EXPRESSION: i32 = 4;
pub const CLAP_NOTE_EXPRESSION_BRIGHTNESS: i32 = 5;
pub const CLAP_NOTE_EXPRESSION_PRESSURE: i32 = 6;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct clap_event_note_expression {
    pub header: clap_event_header,
    pub expression_id: i32,
    pub note_id: i32,
    pub port_index: i16,
    pub channel: i16,
    pub key: i16,
    pub value: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct clap_event_param_value {
    pub header: clap_event_header,
    pub param_id: clap_id,
    pub cookie: *mut c_void,
    pub note_id: i32,
    pub port_index: i16,
    pub channel: i16,
    pub key: i16,
    pub value: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct clap_event_param_mod {
    pub header: clap_event_header,
    pub param_id: clap_id,
    pub cookie: *mut c_void,
    pub note_id: i32,
    pub port_index: i16,
    pub channel: i16,
    pub key: i16,
    pub amount: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct clap_event_param_gesture {
    pub header: clap_event_header,
    pub param_id: clap_id,
}

pub const CLAP_TRANSPORT_HAS_TEMPO: u32 = 1 << 0;
pub const CLAP_TRANSPORT_HAS_BEATS_TIMELINE: u32 = 1 << 1;
pub const CLAP_TRANSPORT_HAS_SECONDS_TIMELINE: u32 = 1 << 2;
pub const CLAP_TRANSPORT_HAS_TIME_SIGNATURE: u32 = 1 << 3;
pub const CLAP_TRANSPORT_IS_PLAYING: u32 = 1 << 4;
pub const CLAP_TRANSPORT_IS_RECORDING: u32 = 1 << 5;
pub const CLAP_TRANSPORT_IS_LOOP_ACTIVE: u32 = 1 << 6;
pub const CLAP_TRANSPORT_IS_WITHIN_PRE_ROLL: u32 = 1 << 7;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct clap_event_transport {
    pub header: clap_event_header,
    pub flags: u32,
    pub song_pos_beats: clap_beattime,
    pub song_pos_seconds: clap_sectime,
    pub tempo: f64,
    pub tempo_inc: f64,
    pub loop_start_beats: clap_beattime,
    pub loop_end_beats: clap_beattime,
    pub loop_start_seconds: clap_sectime,
    pub loop_end_seconds: clap_sectime,
    pub bar_start: clap_beattime,
    pub bar_number: i32,
    pub tsig_num: u16,
    pub tsig_denom: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct clap_event_midi {
    pub header: clap_event_header,
    pub port_index: u16,
    pub data: [u8; 3],
}

#[repr(C)]
#[derive(Debug)]
pub struct clap_input_events {
    pub ctx: *mut c_void,
    pub size: Option<unsafe extern "C" fn(list: *const clap_input_events) -> u32>,
    pub get: Option<
        unsafe extern "C" fn(
            list: *const clap_input_events,
            index: u32,
        ) -> *const clap_event_header,
    >,
}

#[repr(C)]
#[derive(Debug)]
pub struct clap_output_events {
    pub ctx: *mut c_void,
    pub try_push: Option<
        unsafe extern "C" fn(
            list: *const clap_output_events,
            event: *const clap_event_header,
        ) -> bool,
    >,
}

pub const CLAP_EXT_PARAMS: &[u8] = b"clap.params\0";
pub const CLAP_PARAM_IS_STEPPED: u32 = 1 << 0;
pub const CLAP_PARAM_IS_PERIODIC: u32 = 1 << 1;
pub const CLAP_PARAM_IS_HIDDEN: u32 = 1 << 2;
pub const CLAP_PARAM_IS_READONLY: u32 = 1 << 3;
pub const CLAP_PARAM_IS_BYPASS: u32 = 1 << 4;
pub const CLAP_PARAM_IS_AUTOMATABLE: u32 = 1 << 5;
pub const CLAP_PARAM_IS_MODULATABLE: u32 = 1 << 10;
pub const CLAP_PARAM_REQUIRES_PROCESS: u32 = 1 << 15;
pub const CLAP_PARAM_IS_ENUM: u32 = 1 << 16;

#[repr(C)]
pub struct clap_param_info {
    pub id: clap_id,
    pub flags: u32,
    pub cookie: *mut c_void,
    pub name: [c_char; CLAP_NAME_SIZE],
    pub module: [c_char; CLAP_PATH_SIZE],
    pub min_value: f64,
    pub max_value: f64,
    pub default_value: f64,
}

impl std::fmt::Debug for clap_param_info {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("clap_param_info")
            .field("id", &self.id)
            .field("flags", &self.flags)
            .finish_non_exhaustive()
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct clap_plugin_params {
    pub count: Option<unsafe extern "C" fn(plugin: *const clap_plugin) -> u32>,
    pub get_info: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin,
            index: u32,
            info: *mut clap_param_info,
        ) -> bool,
    >,
    pub get_value: Option<
        unsafe extern "C" fn(plugin: *const clap_plugin, id: clap_id, out: *mut f64) -> bool,
    >,
    pub value_to_text: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin,
            id: clap_id,
            value: f64,
            out: *mut c_char,
            capacity: u32,
        ) -> bool,
    >,
    pub text_to_value: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin,
            id: clap_id,
            text: *const c_char,
            out: *mut f64,
        ) -> bool,
    >,
    pub flush: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin,
            input: *const clap_input_events,
            output: *const clap_output_events,
        ),
    >,
}

pub const CLAP_PARAM_RESCAN_VALUES: u32 = 1 << 0;
pub const CLAP_PARAM_RESCAN_TEXT: u32 = 1 << 1;
pub const CLAP_PARAM_RESCAN_INFO: u32 = 1 << 2;
pub const CLAP_PARAM_RESCAN_ALL: u32 = 1 << 3;

#[repr(C)]
#[derive(Debug)]
pub struct clap_host_params {
    pub rescan: Option<unsafe extern "C" fn(host: *const clap_host, flags: u32)>,
    pub clear: Option<unsafe extern "C" fn(host: *const clap_host, param_id: clap_id, flags: u32)>,
    pub request_flush: Option<unsafe extern "C" fn(host: *const clap_host)>,
}

#[repr(C)]
#[derive(Debug)]
pub struct clap_host_latency {
    pub changed: Option<unsafe extern "C" fn(host: *const clap_host)>,
}

#[repr(C)]
#[derive(Debug)]
pub struct clap_host_state {
    pub mark_dirty: Option<unsafe extern "C" fn(host: *const clap_host)>,
}

pub const CLAP_EXT_STATE: &[u8] = b"clap.state\0";

#[repr(C)]
#[derive(Debug)]
pub struct clap_istream {
    pub ctx: *mut c_void,
    pub read: Option<
        unsafe extern "C" fn(stream: *const clap_istream, buffer: *mut c_void, size: u64) -> i64,
    >,
}

#[repr(C)]
#[derive(Debug)]
pub struct clap_ostream {
    pub ctx: *mut c_void,
    pub write: Option<
        unsafe extern "C" fn(stream: *const clap_ostream, buffer: *const c_void, size: u64) -> i64,
    >,
}

#[repr(C)]
#[derive(Debug)]
pub struct clap_plugin_state {
    pub save: Option<
        unsafe extern "C" fn(plugin: *const clap_plugin, stream: *const clap_ostream) -> bool,
    >,
    pub load: Option<
        unsafe extern "C" fn(plugin: *const clap_plugin, stream: *const clap_istream) -> bool,
    >,
}

pub const CLAP_EXT_AUDIO_PORTS: &[u8] = b"clap.audio-ports\0";
pub const CLAP_PORT_MONO: &[u8] = b"mono\0";
pub const CLAP_PORT_STEREO: &[u8] = b"stereo\0";
pub const CLAP_AUDIO_PORT_IS_MAIN: u32 = 1;

#[repr(C)]
pub struct clap_audio_port_info {
    pub id: clap_id,
    pub name: [c_char; CLAP_NAME_SIZE],
    pub flags: u32,
    pub channel_count: u32,
    pub port_type: *const c_char,
    pub in_place_pair: clap_id,
}

impl std::fmt::Debug for clap_audio_port_info {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("clap_audio_port_info")
            .field("id", &self.id)
            .field("channel_count", &self.channel_count)
            .finish_non_exhaustive()
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct clap_plugin_audio_ports {
    pub count: Option<unsafe extern "C" fn(plugin: *const clap_plugin, is_input: bool) -> u32>,
    pub get: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin,
            index: u32,
            is_input: bool,
            info: *mut clap_audio_port_info,
        ) -> bool,
    >,
}

pub const CLAP_EXT_NOTE_PORTS: &[u8] = b"clap.note-ports\0";
pub const CLAP_NOTE_DIALECT_CLAP: u32 = 1 << 0;
pub const CLAP_NOTE_DIALECT_MIDI: u32 = 1 << 1;
pub const CLAP_NOTE_DIALECT_MIDI_MPE: u32 = 1 << 2;
pub const CLAP_NOTE_DIALECT_MIDI2: u32 = 1 << 3;

#[repr(C)]
pub struct clap_note_port_info {
    pub id: clap_id,
    pub supported_dialects: u32,
    pub preferred_dialect: u32,
    pub name: [c_char; CLAP_NAME_SIZE],
}

impl std::fmt::Debug for clap_note_port_info {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("clap_note_port_info")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct clap_plugin_note_ports {
    pub count: Option<unsafe extern "C" fn(plugin: *const clap_plugin, is_input: bool) -> u32>,
    pub get: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin,
            index: u32,
            is_input: bool,
            info: *mut clap_note_port_info,
        ) -> bool,
    >,
}

pub const CLAP_EXT_LATENCY: &[u8] = b"clap.latency\0";

#[repr(C)]
#[derive(Debug)]
pub struct clap_plugin_latency {
    pub get: Option<unsafe extern "C" fn(plugin: *const clap_plugin) -> u32>,
}

pub const CLAP_EXT_TAIL: &[u8] = b"clap.tail\0";

#[repr(C)]
#[derive(Debug)]
pub struct clap_plugin_tail {
    pub get: Option<unsafe extern "C" fn(plugin: *const clap_plugin) -> u32>,
}

/// Layout checks: every number here was printed by a C program compiled
/// against the CLAP headers (x86-64 Linux); `tools/check-plugins.sh`
/// recompiles that program so CI re-derives them on each platform.
#[cfg(all(test, target_pointer_width = "64"))]
mod layout {
    use super::*;
    use std::mem::{offset_of, size_of};

    #[test]
    fn sizes_and_offsets_match_the_headers() {
        assert_eq!(size_of::<clap_event_header>(), 16);
        assert_eq!(size_of::<clap_event_note>(), 40);
        assert_eq!(offset_of!(clap_event_note, velocity), 32);
        assert_eq!(size_of::<clap_event_param_value>(), 56);
        assert_eq!(offset_of!(clap_event_param_value, value), 48);
        assert_eq!(size_of::<clap_event_note_expression>(), 40);
        assert_eq!(size_of::<clap_note_port_info>(), 268);
        assert_eq!(size_of::<clap_host>(), 88);
        assert_eq!(size_of::<clap_plugin_entry>(), 40);
        assert_eq!(size_of::<clap_event_transport>(), 104);
        assert_eq!(size_of::<clap_event_midi>(), 24);
        assert_eq!(size_of::<clap_param_info>(), 8 + 8 + 256 + 1024 + 24);
        assert_eq!(size_of::<clap_audio_buffer>(), 32);
        assert_eq!(size_of::<clap_process>(), 64);
        assert_eq!(size_of::<clap_plugin>(), 16 + 10 * 8);
        assert_eq!(size_of::<clap_plugin_descriptor>(), 16 + 9 * 8);
        assert_eq!(
            size_of::<clap_audio_port_info>(),
            4 + 256 + 4 + 4 + 4 + 8 + 4 + 4
        );
    }
}
