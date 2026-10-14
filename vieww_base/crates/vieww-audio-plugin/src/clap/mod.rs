//! CLAP: export a [`Plugin`] as a `.clap`, and host `.clap` files.
//!
//! ```ignore
//! vieww_audio_plugin::export_clap!(MyGain, MySynth);
//! ```
//!
//! in a `cdylib` crate defines the `clap_entry` symbol a CLAP host looks
//! for. Each plugin gets a descriptor built from its [`PluginInfo`](crate::PluginInfo),
//! a main stereo (or mono, or none) audio port pair, a note port for
//! instruments (CLAP and MIDI dialects), and the params, state, latency
//! and tail extensions.
//!
//! # Threads
//!
//! CLAP calls a plugin from a main thread and an audio thread, sometimes
//! at once. The wrapper keeps them apart without blocking the audio thread:
//! parameter values are mirrored in atomics (so the host's main thread
//! reads them without touching the plugin), and the plugin itself sits
//! behind a lock that the audio thread only ever `try_lock`s — when the
//! main thread holds it (loading a preset) the block renders silence
//! rather than waiting.
//!
//! [`host`] is the other side: load a `.clap`, list and instantiate its
//! plugins, drive them.

pub mod host;
pub mod sys;

use std::any::TypeId;
use std::ffi::{c_char, c_void, CStr, CString};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::{
    Driver, Event, Expression, Kind, ParamChange, Plugin, Status, Transport, MAX_CHANNELS,
};
use sys::*;

/// A plugin's descriptor and the C strings it points into.
#[derive(Debug)]
pub struct Descriptor {
    raw: clap_plugin_descriptor,
    _strings: Vec<CString>,
    _features: Vec<*const c_char>,
}

// SAFETY: the raw pointers point into `_strings`/`_features`, owned by the
// descriptor and never mutated after construction.
unsafe impl Send for Descriptor {}
// SAFETY: as above — immutable after construction.
unsafe impl Sync for Descriptor {}

fn c(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

impl Descriptor {
    fn new<P: Plugin>() -> Self {
        let i = P::info();
        let kind = match i.kind {
            Kind::Instrument => "instrument",
            Kind::Effect => "audio-effect",
            Kind::NoteEffect => "note-effect",
        };
        let mut strings: Vec<CString> = [
            i.id,
            i.name,
            i.vendor,
            i.url,
            "",
            "",
            i.version,
            i.description,
        ]
        .iter()
        .map(|s| c(s))
        .collect();
        let first_feature = strings.len();
        strings.push(c(kind));
        strings.extend(i.features.iter().map(|f| c(f)));
        let mut features: Vec<*const c_char> = strings[first_feature..]
            .iter()
            .map(|s| s.as_ptr())
            .collect();
        features.push(null());
        let raw = clap_plugin_descriptor {
            clap_version: CLAP_VERSION,
            id: strings[0].as_ptr(),
            name: strings[1].as_ptr(),
            vendor: strings[2].as_ptr(),
            url: strings[3].as_ptr(),
            manual_url: strings[4].as_ptr(),
            support_url: strings[5].as_ptr(),
            version: strings[6].as_ptr(),
            description: strings[7].as_ptr(),
            features: features.as_ptr(),
        };
        Self {
            raw,
            _strings: strings,
            _features: features,
        }
    }
}

/// The descriptor for `P` (built once, valid for the life of the library).
#[must_use]
pub fn descriptor<P: Plugin>() -> *const clap_plugin_descriptor {
    type Registry = Mutex<Vec<(TypeId, &'static Descriptor)>>;
    static REG: OnceLock<Registry> = OnceLock::new();
    let reg = REG.get_or_init(|| Mutex::new(Vec::new()));
    let Ok(mut r) = reg.lock() else {
        return null();
    };
    let t = TypeId::of::<P>();
    if let Some((_, d)) = r.iter().find(|(k, _)| *k == t) {
        return &raw const d.raw;
    }
    let d: &'static Descriptor = Box::leak(Box::new(Descriptor::new::<P>()));
    r.push((t, d));
    &raw const d.raw
}

/// The C string a pointer holds, if it is one.
///
/// # Safety
/// `p` is null or a NUL-terminated string.
unsafe fn cstr<'a>(p: *const c_char) -> Option<&'a CStr> {
    // SAFETY: the caller's contract.
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

/// An instance: the C struct first, so a `clap_plugin*` is an `Instance*`.
#[repr(C)]
struct Instance<P: Plugin> {
    clap: clap_plugin,
    host: *const clap_host,
    driver: Mutex<Driver<P>>,
    /// Immutable copy of the parameter table for main-thread queries.
    params: Vec<crate::ParamInfo>,
    values: Vec<AtomicU64>,
    latency: AtomicU64,
    tail: AtomicU64,
    scratch: Mutex<Vec<Vec<f32>>>,
    ext_params: clap_plugin_params,
    ext_state: clap_plugin_state,
    ext_audio_ports: clap_plugin_audio_ports,
    ext_note_ports: clap_plugin_note_ports,
    ext_latency: clap_plugin_latency,
    ext_tail: clap_plugin_tail,
}

impl<P: Plugin> Instance<P> {
    /// # Safety
    /// `p` came from [`create`] for this `P` and has not been destroyed.
    unsafe fn get<'a>(p: *const clap_plugin) -> &'a Self {
        // SAFETY: `plugin_data` points at the instance (see `create`).
        unsafe { &*(*p).plugin_data.cast::<Self>() }
    }

    fn mirror(&self, d: &Driver<P>) {
        for (info, slot) in self.params.iter().zip(&self.values) {
            if let Some(v) = d.plugin.get_param(info.id) {
                slot.store(v.to_bits(), Ordering::Relaxed);
            }
        }
        self.latency
            .store(u64::from(d.plugin.latency()), Ordering::Relaxed);
        self.tail
            .store(u64::from(d.plugin.tail()), Ordering::Relaxed);
    }

    fn index(&self, id: u32) -> Option<usize> {
        self.params.iter().position(|p| p.id == id)
    }
}

/// Create an instance of `P` for `host` (what a factory's
/// `create_plugin` does).
///
/// # Safety
/// `host` must outlive the instance (the CLAP contract).
pub unsafe fn create<P: Plugin>(host: *const clap_host) -> *const clap_plugin {
    let driver = Driver::<P>::new();
    let params = driver.params.clone();
    let values = params
        .iter()
        .map(|p| AtomicU64::new(driver.plugin.get_param(p.id).unwrap_or(p.default).to_bits()))
        .collect();
    let inst = Box::new(Instance::<P> {
        clap: clap_plugin {
            desc: descriptor::<P>(),
            plugin_data: null_mut(),
            init: Some(init),
            destroy: Some(destroy::<P>),
            activate: Some(activate::<P>),
            deactivate: Some(deactivate::<P>),
            start_processing: Some(start_processing),
            stop_processing: Some(stop_processing),
            reset: Some(reset::<P>),
            process: Some(process::<P>),
            get_extension: Some(get_extension::<P>),
            on_main_thread: Some(on_main_thread),
        },
        host,
        latency: AtomicU64::new(u64::from(driver.plugin.latency())),
        tail: AtomicU64::new(u64::from(driver.plugin.tail())),
        driver: Mutex::new(driver),
        params,
        values,
        scratch: Mutex::new(Vec::new()),
        ext_params: clap_plugin_params {
            count: Some(params_count::<P>),
            get_info: Some(params_get_info::<P>),
            get_value: Some(params_get_value::<P>),
            value_to_text: Some(params_value_to_text::<P>),
            text_to_value: Some(params_text_to_value::<P>),
            flush: Some(params_flush::<P>),
        },
        ext_state: clap_plugin_state {
            save: Some(state_save::<P>),
            load: Some(state_load::<P>),
        },
        ext_audio_ports: clap_plugin_audio_ports {
            count: Some(audio_ports_count::<P>),
            get: Some(audio_ports_get::<P>),
        },
        ext_note_ports: clap_plugin_note_ports {
            count: Some(note_ports_count::<P>),
            get: Some(note_ports_get::<P>),
        },
        ext_latency: clap_plugin_latency {
            get: Some(latency_get::<P>),
        },
        ext_tail: clap_plugin_tail {
            get: Some(tail_get::<P>),
        },
    });
    let p = Box::into_raw(inst);
    // SAFETY: `p` is a fresh, valid allocation.
    unsafe {
        (*p).clap.plugin_data = p.cast();
        &raw const (*p).clap
    }
}

impl<P: Plugin> Instance<P> {
    /// A host extension, or null.
    fn host_ext<T>(&self, id: &[u8]) -> *const T {
        if self.host.is_null() {
            return null();
        }
        // SAFETY: the host outlives the plugin and `id` is NUL-terminated.
        unsafe {
            (*self.host)
                .get_extension
                .map_or(null(), |g| g(self.host, id.as_ptr().cast()))
                .cast()
        }
    }

    /// Tell the host every parameter value may have changed.
    fn rescan_values(&self) {
        let hp = self.host_ext::<clap_host_params>(CLAP_EXT_PARAMS);
        if !hp.is_null() {
            // SAFETY: a valid host extension (main thread, per the contract
            // of the state and activation calls this runs from).
            unsafe {
                if let Some(r) = (*hp).rescan {
                    r(self.host, CLAP_PARAM_RESCAN_VALUES);
                }
            }
        }
    }
}

unsafe extern "C" fn init(_: *const clap_plugin) -> bool {
    true
}

unsafe extern "C" fn destroy<P: Plugin>(p: *const clap_plugin) {
    if p.is_null() {
        return;
    }
    // SAFETY: created by `create::<P>` via `Box::into_raw`; destroyed once.
    drop(unsafe { Box::from_raw((*p).plugin_data.cast::<Instance<P>>()) });
}

unsafe extern "C" fn activate<P: Plugin>(
    p: *const clap_plugin,
    rate: f64,
    _min: u32,
    max: u32,
) -> bool {
    // SAFETY: a live instance (CLAP contract).
    let inst = unsafe { Instance::<P>::get(p) };
    let Ok(mut d) = inst.driver.lock() else {
        return false;
    };
    d.rate = rate;
    d.max_block = max;
    let before = inst.latency.load(Ordering::Relaxed);
    let ok = d.plugin.activate(rate, max);
    inst.mirror(&d);
    if inst.latency.load(Ordering::Relaxed) != before {
        // Latency may change during activation; the host must hear of it.
        let hl = inst.host_ext::<clap_host_latency>(CLAP_EXT_LATENCY);
        if !hl.is_null() {
            // SAFETY: a valid host extension, called from `activate`.
            unsafe {
                if let Some(c) = (*hl).changed {
                    c(inst.host);
                }
            }
        }
    }
    if let Ok(mut s) = inst.scratch.lock() {
        let frames = usize::try_from(max).unwrap_or(4_096).max(1);
        *s = vec![vec![0.0; frames]; MAX_CHANNELS];
    }
    ok
}

unsafe extern "C" fn deactivate<P: Plugin>(p: *const clap_plugin) {
    // SAFETY: a live instance.
    let inst = unsafe { Instance::<P>::get(p) };
    if let Ok(mut d) = inst.driver.lock() {
        d.plugin.deactivate();
    }
}

unsafe extern "C" fn start_processing(_: *const clap_plugin) -> bool {
    true
}

unsafe extern "C" fn stop_processing(_: *const clap_plugin) {}

unsafe extern "C" fn reset<P: Plugin>(p: *const clap_plugin) {
    // SAFETY: a live instance.
    let inst = unsafe { Instance::<P>::get(p) };
    if let Ok(mut d) = inst.driver.lock() {
        d.plugin.reset();
    }
}

unsafe extern "C" fn on_main_thread(_: *const clap_plugin) {}

/// Convert one host event into the driver's queues.
///
/// # Safety
/// `h` points at a complete event whose `size` and `type_` agree.
unsafe fn take_event<P: Plugin>(d: &mut Driver<P>, h: *const clap_event_header) {
    // SAFETY: the caller's contract; each arm reads the struct `type_` names.
    unsafe {
        let hd = &*h;
        if hd.space_id != CLAP_CORE_EVENT_SPACE_ID {
            return;
        }
        let ch = |c: i16| u8::try_from(c.clamp(0, 15)).unwrap_or(0);
        let key = |k: i16| u8::try_from(k.clamp(0, 127)).unwrap_or(0);
        match hd.type_ {
            CLAP_EVENT_NOTE_ON | CLAP_EVENT_NOTE_OFF | CLAP_EVENT_NOTE_CHOKE => {
                let n = &*h.cast::<clap_event_note>();
                #[expect(clippy::cast_possible_truncation, reason = "velocity 0..1")]
                let velocity = n.velocity as f32;
                let (time, channel, k, note_id) = (hd.time, ch(n.channel), key(n.key), n.note_id);
                d.push_event(match hd.type_ {
                    CLAP_EVENT_NOTE_ON => Event::NoteOn {
                        time,
                        channel,
                        key: k,
                        velocity,
                        note_id,
                    },
                    CLAP_EVENT_NOTE_OFF => Event::NoteOff {
                        time,
                        channel,
                        key: k,
                        velocity,
                        note_id,
                    },
                    _ => Event::Choke {
                        time,
                        channel,
                        key: k,
                        note_id,
                    },
                });
            }
            CLAP_EVENT_NOTE_EXPRESSION => {
                let n = &*h.cast::<clap_event_note_expression>();
                let expression = match n.expression_id {
                    CLAP_NOTE_EXPRESSION_VOLUME => Expression::Volume,
                    CLAP_NOTE_EXPRESSION_PAN => Expression::Pan,
                    CLAP_NOTE_EXPRESSION_TUNING => Expression::Tuning,
                    CLAP_NOTE_EXPRESSION_VIBRATO => Expression::Vibrato,
                    CLAP_NOTE_EXPRESSION_EXPRESSION => Expression::Expression,
                    CLAP_NOTE_EXPRESSION_BRIGHTNESS => Expression::Brightness,
                    CLAP_NOTE_EXPRESSION_PRESSURE => Expression::Pressure,
                    _ => return,
                };
                d.push_event(Event::NoteExpression {
                    time: hd.time,
                    expression,
                    channel: ch(n.channel),
                    key: key(n.key),
                    note_id: n.note_id,
                    value: n.value,
                });
            }
            CLAP_EVENT_PARAM_VALUE => {
                let e = &*h.cast::<clap_event_param_value>();
                d.push_change(ParamChange {
                    time: hd.time,
                    id: e.param_id,
                    value: e.value,
                });
            }
            CLAP_EVENT_MIDI => {
                let e = &*h.cast::<clap_event_midi>();
                d.push_event(Event::Midi {
                    time: hd.time,
                    data: e.data,
                });
            }
            CLAP_EVENT_TRANSPORT => {
                d.transport = transport(&*h.cast::<clap_event_transport>());
            }
            _ => {}
        }
    }
}

fn transport(t: &clap_event_transport) -> Transport {
    #[expect(clippy::cast_precision_loss, reason = "fixed point to float")]
    let beats = |b: i64| b as f64 / CLAP_BEATTIME_FACTOR as f64;
    #[expect(clippy::cast_precision_loss, reason = "fixed point to float")]
    let secs = |s: i64| s as f64 / CLAP_SECTIME_FACTOR as f64;
    let has = |f: u32| t.flags & f != 0;
    Transport {
        playing: has(CLAP_TRANSPORT_IS_PLAYING),
        recording: has(CLAP_TRANSPORT_IS_RECORDING),
        looping: has(CLAP_TRANSPORT_IS_LOOP_ACTIVE),
        tempo: has(CLAP_TRANSPORT_HAS_TEMPO).then_some(t.tempo),
        beats: has(CLAP_TRANSPORT_HAS_BEATS_TIMELINE).then(|| beats(t.song_pos_beats)),
        seconds: has(CLAP_TRANSPORT_HAS_SECONDS_TIMELINE).then(|| secs(t.song_pos_seconds)),
        time_signature: has(CLAP_TRANSPORT_HAS_TIME_SIGNATURE)
            .then_some((t.tsig_num, t.tsig_denom)),
        bar_start: has(CLAP_TRANSPORT_HAS_BEATS_TIMELINE).then(|| beats(t.bar_start)),
        loop_beats: (has(CLAP_TRANSPORT_IS_LOOP_ACTIVE) && has(CLAP_TRANSPORT_HAS_BEATS_TIMELINE))
            .then(|| (beats(t.loop_start_beats), beats(t.loop_end_beats))),
    }
}

/// # Safety
/// `list` is null or a valid host event list.
unsafe fn drain_events<P: Plugin>(d: &mut Driver<P>, list: *const clap_input_events) {
    if list.is_null() {
        return;
    }
    // SAFETY: a valid list (the caller's contract) and its own accessors.
    unsafe {
        let (Some(size), Some(get)) = ((*list).size, (*list).get) else {
            return;
        };
        for i in 0..size(list) {
            let h = get(list, i);
            if !h.is_null() {
                take_event(d, h);
            }
        }
    }
}

unsafe extern "C" fn process<P: Plugin>(p: *const clap_plugin, pr: *const clap_process) -> i32 {
    if p.is_null() || pr.is_null() {
        return CLAP_PROCESS_ERROR;
    }
    // SAFETY: a live instance and a valid process struct (CLAP contract).
    let (inst, pr) = unsafe { (Instance::<P>::get(p), &*pr) };
    let frames = pr.frames_count as usize;
    // Output channel pointers.
    let mut out_ptrs = [null_mut::<f32>(); MAX_CHANNELS];
    let mut n_out = 0;
    if pr.audio_outputs_count > 0 && !pr.audio_outputs.is_null() {
        // SAFETY: the host provides `audio_outputs_count` buffers.
        let b = unsafe { &*pr.audio_outputs };
        n_out = (b.channel_count as usize).min(MAX_CHANNELS);
        for (c, slot) in out_ptrs.iter_mut().enumerate().take(n_out) {
            // SAFETY: `data32` has `channel_count` entries.
            *slot = unsafe { *b.data32.add(c) };
        }
    }
    let mut in_ptrs = [null::<f32>(); MAX_CHANNELS];
    let mut n_in = 0;
    if pr.audio_inputs_count > 0 && !pr.audio_inputs.is_null() {
        // SAFETY: as for outputs.
        let b = unsafe { &*pr.audio_inputs };
        n_in = (b.channel_count as usize).min(MAX_CHANNELS);
        for (c, slot) in in_ptrs.iter_mut().enumerate().take(n_in) {
            // SAFETY: as for outputs.
            *slot = unsafe { (*b.data32.add(c)).cast_const() };
        }
    }
    if out_ptrs[..n_out].iter().any(|p| p.is_null()) || in_ptrs[..n_in].iter().any(|p| p.is_null())
    {
        return CLAP_PROCESS_ERROR;
    }
    let Ok(mut d) = inst.driver.try_lock() else {
        // The main thread is loading state: render silence, never wait.
        for &o in &out_ptrs[..n_out] {
            // SAFETY: the host's output channel of `frames` samples.
            unsafe { std::slice::from_raw_parts_mut(o, frames).fill(0.0) };
        }
        return CLAP_PROCESS_CONTINUE;
    };
    if !pr.transport.is_null() {
        // SAFETY: a valid transport event when non-null.
        d.transport = transport(unsafe { &*pr.transport });
    }
    // SAFETY: the host's event list.
    unsafe { drain_events(&mut d, pr.in_events) };

    // In-place (aliased) buffers would be `&` and `&mut` to the same
    // memory, which Rust forbids: copy such inputs aside first.
    let aliased = in_ptrs[..n_in]
        .iter()
        .any(|i| out_ptrs[..n_out].iter().any(|o| o.cast_const() == *i));
    let mut scratch_guard = if aliased {
        inst.scratch.try_lock().ok()
    } else {
        None
    };
    let mut ins: [&[f32]; MAX_CHANNELS] = [&[]; MAX_CHANNELS];
    if let Some(s) = scratch_guard.as_mut() {
        if s.len() < n_in || s.iter().take(n_in).any(|b| b.len() < frames) {
            return CLAP_PROCESS_ERROR;
        }
        for c in 0..n_in {
            // SAFETY: the host's input channel of `frames` samples.
            let src = unsafe { std::slice::from_raw_parts(in_ptrs[c], frames) };
            s[c][..frames].copy_from_slice(src);
        }
        for (c, slot) in ins.iter_mut().enumerate().take(n_in) {
            *slot = &s[c][..frames];
        }
    } else if aliased {
        return CLAP_PROCESS_ERROR;
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
    let status = d.run(&ins[..n_in], &mut outs[..n_out], frames);
    inst.mirror(&d);
    match status {
        Status::Continue => CLAP_PROCESS_CONTINUE,
        Status::ContinueIfNotQuiet => CLAP_PROCESS_CONTINUE_IF_NOT_QUIET,
        Status::Tail => CLAP_PROCESS_TAIL,
        Status::Sleep => CLAP_PROCESS_SLEEP,
        Status::Error => CLAP_PROCESS_ERROR,
    }
}

unsafe extern "C" fn get_extension<P: Plugin>(
    p: *const clap_plugin,
    id: *const c_char,
) -> *const c_void {
    // SAFETY: a live instance and a C string (CLAP contract).
    let (inst, Some(id)) = (unsafe { Instance::<P>::get(p) }, unsafe { cstr(id) }) else {
        return null();
    };
    let id = id.to_bytes_with_nul();
    let info = P::info();
    if id == CLAP_EXT_PARAMS {
        (&raw const inst.ext_params).cast()
    } else if id == CLAP_EXT_STATE {
        (&raw const inst.ext_state).cast()
    } else if id == CLAP_EXT_AUDIO_PORTS {
        (&raw const inst.ext_audio_ports).cast()
    } else if id == CLAP_EXT_NOTE_PORTS && info.note_input {
        (&raw const inst.ext_note_ports).cast()
    } else if id == CLAP_EXT_LATENCY {
        (&raw const inst.ext_latency).cast()
    } else if id == CLAP_EXT_TAIL {
        (&raw const inst.ext_tail).cast()
    } else {
        null()
    }
}

unsafe extern "C" fn params_count<P: Plugin>(p: *const clap_plugin) -> u32 {
    // SAFETY: a live instance.
    u32::try_from(unsafe { Instance::<P>::get(p) }.params.len()).unwrap_or(0)
}

unsafe extern "C" fn params_get_info<P: Plugin>(
    p: *const clap_plugin,
    index: u32,
    out: *mut clap_param_info,
) -> bool {
    // SAFETY: a live instance.
    let inst = unsafe { Instance::<P>::get(p) };
    let Some(info) = inst.params.get(index as usize) else {
        return false;
    };
    if out.is_null() {
        return false;
    }
    let mut flags = 0;
    if info.taper == crate::Taper::Stepped {
        flags |= CLAP_PARAM_IS_STEPPED;
        if !info.choices.is_empty() {
            flags |= CLAP_PARAM_IS_ENUM;
        }
    }
    if info.automatable {
        flags |= CLAP_PARAM_IS_AUTOMATABLE;
    }
    // SAFETY: `out` is a valid, writable param info (CLAP contract).
    unsafe {
        let o = &mut *out;
        o.id = info.id;
        o.flags = flags;
        o.cookie = null_mut();
        crate::write_c_str(&mut o.name, &info.name);
        crate::write_c_str(&mut o.module, "");
        o.min_value = info.min;
        o.max_value = info.max;
        o.default_value = info.default;
    }
    true
}

unsafe extern "C" fn params_get_value<P: Plugin>(
    p: *const clap_plugin,
    id: clap_id,
    out: *mut f64,
) -> bool {
    // SAFETY: a live instance.
    let inst = unsafe { Instance::<P>::get(p) };
    match (inst.index(id), out.is_null()) {
        (Some(i), false) => {
            // SAFETY: a valid out pointer.
            unsafe { *out = f64::from_bits(inst.values[i].load(Ordering::Relaxed)) };
            true
        }
        _ => false,
    }
}

unsafe extern "C" fn params_value_to_text<P: Plugin>(
    p: *const clap_plugin,
    id: clap_id,
    value: f64,
    out: *mut c_char,
    cap: u32,
) -> bool {
    // SAFETY: a live instance.
    let inst = unsafe { Instance::<P>::get(p) };
    let Some(i) = inst.index(id) else {
        return false;
    };
    if out.is_null() || cap == 0 {
        return false;
    }
    // SAFETY: `out` has `cap` writable chars.
    let buf = unsafe { std::slice::from_raw_parts_mut(out, cap as usize) };
    crate::write_c_str(buf, &inst.params[i].display(value));
    true
}

unsafe extern "C" fn params_text_to_value<P: Plugin>(
    p: *const clap_plugin,
    id: clap_id,
    text: *const c_char,
    out: *mut f64,
) -> bool {
    // SAFETY: a live instance and a C string.
    let (inst, text) = unsafe { (Instance::<P>::get(p), cstr(text)) };
    let (Some(i), Some(t)) = (inst.index(id), text) else {
        return false;
    };
    match (inst.params[i].parse(&t.to_string_lossy()), out.is_null()) {
        (Some(v), false) => {
            // SAFETY: a valid out pointer.
            unsafe { *out = v };
            true
        }
        _ => false,
    }
}

unsafe extern "C" fn params_flush<P: Plugin>(
    p: *const clap_plugin,
    input: *const clap_input_events,
    _out: *const clap_output_events,
) {
    // SAFETY: a live instance.
    let inst = unsafe { Instance::<P>::get(p) };
    let Ok(mut d) = inst.driver.lock() else {
        return;
    };
    // SAFETY: the host's event list.
    unsafe { drain_events(&mut d, input) };
    // Outside processing only parameter changes mean anything: apply them.
    let _ = d.run(&[], &mut [], 0);
    inst.mirror(&d);
}

unsafe extern "C" fn state_save<P: Plugin>(p: *const clap_plugin, s: *const clap_ostream) -> bool {
    // SAFETY: a live instance.
    let inst = unsafe { Instance::<P>::get(p) };
    if s.is_null() {
        return false;
    }
    let data = match inst.driver.lock() {
        Ok(d) => d.plugin.save_state(),
        Err(_) => return false,
    };
    // SAFETY: a valid stream; partial writes are retried.
    unsafe {
        let Some(write) = (*s).write else {
            return false;
        };
        let mut at = 0;
        while at < data.len() {
            let n = write(s, data[at..].as_ptr().cast(), (data.len() - at) as u64);
            if n <= 0 {
                return false;
            }
            at += usize::try_from(n).unwrap_or(0);
        }
    }
    true
}

unsafe extern "C" fn state_load<P: Plugin>(p: *const clap_plugin, s: *const clap_istream) -> bool {
    // SAFETY: a live instance.
    let inst = unsafe { Instance::<P>::get(p) };
    if s.is_null() {
        return false;
    }
    let mut data = Vec::new();
    // SAFETY: a valid stream, read until it reports the end.
    unsafe {
        let Some(read) = (*s).read else {
            return false;
        };
        let mut buf = [0u8; 4_096];
        loop {
            let n = read(s, buf.as_mut_ptr().cast(), buf.len() as u64);
            if n < 0 {
                return false;
            }
            if n == 0 {
                break;
            }
            data.extend_from_slice(&buf[..usize::try_from(n).unwrap_or(0)]);
        }
    }
    let Ok(mut d) = inst.driver.lock() else {
        return false;
    };
    let ok = d.plugin.load_state(&data);
    inst.mirror(&d);
    drop(d);
    if ok {
        inst.rescan_values();
    }
    ok
}

unsafe extern "C" fn audio_ports_count<P: Plugin>(_: *const clap_plugin, is_input: bool) -> u32 {
    let i = P::info();
    u32::from(if is_input {
        i.inputs > 0
    } else {
        i.outputs > 0
    })
}

unsafe extern "C" fn audio_ports_get<P: Plugin>(
    _: *const clap_plugin,
    index: u32,
    is_input: bool,
    out: *mut clap_audio_port_info,
) -> bool {
    let i = P::info();
    let channels = if is_input { i.inputs } else { i.outputs };
    if index != 0 || channels == 0 || out.is_null() {
        return false;
    }
    // SAFETY: a valid, writable port info.
    unsafe {
        let o = &mut *out;
        o.id = if is_input { 0 } else { 1 };
        crate::write_c_str(&mut o.name, if is_input { "Input" } else { "Output" });
        o.flags = CLAP_AUDIO_PORT_IS_MAIN;
        o.channel_count = channels;
        o.port_type = match channels {
            1 => CLAP_PORT_MONO.as_ptr().cast(),
            2 => CLAP_PORT_STEREO.as_ptr().cast(),
            _ => null(),
        };
        o.in_place_pair = CLAP_INVALID_ID;
    }
    true
}

unsafe extern "C" fn note_ports_count<P: Plugin>(_: *const clap_plugin, is_input: bool) -> u32 {
    u32::from(is_input && P::info().note_input)
}

unsafe extern "C" fn note_ports_get<P: Plugin>(
    _: *const clap_plugin,
    index: u32,
    is_input: bool,
    out: *mut clap_note_port_info,
) -> bool {
    if index != 0 || !is_input || !P::info().note_input || out.is_null() {
        return false;
    }
    // SAFETY: a valid, writable port info.
    unsafe {
        let o = &mut *out;
        o.id = 0;
        o.supported_dialects = CLAP_NOTE_DIALECT_CLAP | CLAP_NOTE_DIALECT_MIDI;
        o.preferred_dialect = CLAP_NOTE_DIALECT_CLAP;
        crate::write_c_str(&mut o.name, "Notes");
    }
    true
}

unsafe extern "C" fn latency_get<P: Plugin>(p: *const clap_plugin) -> u32 {
    // SAFETY: a live instance.
    u32::try_from(
        unsafe { Instance::<P>::get(p) }
            .latency
            .load(Ordering::Relaxed),
    )
    .unwrap_or(0)
}

unsafe extern "C" fn tail_get<P: Plugin>(p: *const clap_plugin) -> u32 {
    // SAFETY: a live instance.
    u32::try_from(
        unsafe { Instance::<P>::get(p) }
            .tail
            .load(Ordering::Relaxed),
    )
    .unwrap_or(u32::MAX)
}

/// Build a `clap_plugin_entry` exposing the listed plugin types (what
/// [`export_clap!`](crate::export_clap) exports under the `clap_entry`
/// name). Usable directly for an in-process entry (tests, a host
/// embedding its own plugins).
#[macro_export]
macro_rules! clap_entry {
    ($($t:ty),+ $(,)?) => {{
        use $crate::clap::sys as __s;
        unsafe extern "C" fn __init(_: *const ::std::ffi::c_char) -> bool { true }
        unsafe extern "C" fn __deinit() {}
        unsafe extern "C" fn __count(_: *const __s::clap_plugin_factory) -> u32 {
            [$( ::std::stringify!($t) ),+].len() as u32
        }
        unsafe extern "C" fn __desc(
            _: *const __s::clap_plugin_factory,
            index: u32,
        ) -> *const __s::clap_plugin_descriptor {
            let table: &[fn() -> *const __s::clap_plugin_descriptor] =
                &[$( $crate::clap::descriptor::<$t> ),+];
            table.get(index as usize).map_or(::std::ptr::null(), |f| f())
        }
        unsafe extern "C" fn __create(
            _: *const __s::clap_plugin_factory,
            host: *const __s::clap_host,
            id: *const ::std::ffi::c_char,
        ) -> *const __s::clap_plugin {
            if id.is_null() || host.is_null() {
                return ::std::ptr::null();
            }
            // SAFETY: a NUL-terminated id from the host.
            let id = unsafe { ::std::ffi::CStr::from_ptr(id) }.to_bytes();
            $(
                if <$t as $crate::Plugin>::info().id.as_bytes() == id {
                    // SAFETY: the host outlives its plugins (CLAP contract).
                    return unsafe { $crate::clap::create::<$t>(host) };
                }
            )+
            ::std::ptr::null()
        }
        static __FACTORY: __s::clap_plugin_factory = __s::clap_plugin_factory {
            get_plugin_count: Some(__count),
            get_plugin_descriptor: Some(__desc),
            create_plugin: Some(__create),
        };
        unsafe extern "C" fn __get_factory(id: *const ::std::ffi::c_char) -> *const ::std::ffi::c_void {
            if id.is_null() {
                return ::std::ptr::null();
            }
            // SAFETY: a NUL-terminated factory id from the host.
            let id = unsafe { ::std::ffi::CStr::from_ptr(id) }.to_bytes_with_nul();
            if id == __s::CLAP_PLUGIN_FACTORY_ID {
                (&raw const __FACTORY).cast()
            } else {
                ::std::ptr::null()
            }
        }
        __s::clap_plugin_entry {
            clap_version: __s::CLAP_VERSION,
            init: Some(__init),
            deinit: Some(__deinit),
            get_factory: Some(__get_factory),
        }
    }};
}

/// Export the listed plugin types as this library's `clap_entry`.
#[macro_export]
macro_rules! export_clap {
    ($($t:ty),+ $(,)?) => {
        #[allow(non_upper_case_globals, reason = "the symbol CLAP hosts look up")]
        #[unsafe(no_mangle)]
        pub static clap_entry: $crate::clap::sys::clap_plugin_entry = $crate::clap_entry!($($t),+);
    };
}

#[cfg(test)]
mod tests {
    use super::host::{HostEvent, Library};
    use crate::testing::{Gain, Synth};

    static ENTRY: crate::clap::sys::clap_plugin_entry = crate::clap_entry!(Gain, Synth);

    fn lib() -> Library {
        Library::from_entry(&ENTRY, "in-process").unwrap()
    }

    #[test]
    fn the_factory_lists_both_plugins_with_their_features() {
        let l = lib();
        let ps = l.plugins();
        assert_eq!(ps.len(), 2);
        assert_eq!(ps[0].id, "dev.vieww.test-gain");
        assert_eq!(ps[0].features, vec!["audio-effect", "utility", "stereo"]);
        assert_eq!(ps[1].name, "Test Synth");
        assert_eq!(ps[1].version, "0.3.1");
        assert_eq!(ps[1].features[0], "instrument");
        assert!(l.instantiate("nope").is_err());
    }

    #[test]
    fn an_effect_processes_automates_and_reports_through_extensions() {
        let l = lib();
        let mut p = l.instantiate("dev.vieww.test-gain").unwrap();
        let params = p.params();
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "Gain");
        assert_eq!(
            (params[0].min, params[0].max, params[0].default),
            (-60.0, 12.0, 0.0)
        );
        assert!(params[1].stepped && params[1].is_enum);
        assert_eq!(p.value_to_text(1, -6.0).as_deref(), Some("-6.000 dB"));
        assert_eq!(p.text_to_value(2, "On"), Some(1.0));
        assert_eq!(p.latency(), 64);
        assert_eq!(p.audio_ports(true), vec![("Input".to_owned(), 2)]);
        assert_eq!(p.audio_ports(false), vec![("Output".to_owned(), 2)]);
        assert_eq!(p.note_ports(), 0);
        assert!(p.activate(48_000.0, 32, 256));
        let input = vec![vec![1.0f32; 128]; 2];
        let events = [HostEvent::Param {
            time: 64,
            id: 1,
            value: -6.020_599_913_279_624,
        }];
        let (out, status) = p.process(&input, 2, 128, &events, None);
        assert_eq!(status, crate::clap::sys::CLAP_PROCESS_CONTINUE_IF_NOT_QUIET);
        assert_eq!(out[0][63], 1.0);
        assert!((out[1][64] - 0.5).abs() < 1e-6);
        assert!(
            (p.value(1).unwrap() + 6.0206).abs() < 1e-3,
            "mirrored to the main thread"
        );
        // Flushing outside processing applies changes too.
        p.flush(&[HostEvent::Param {
            time: 0,
            id: 2,
            value: 1.0,
        }]);
        assert_eq!(p.value(2), Some(1.0));
        let (out, _) = p.process(&input, 2, 16, &[], None);
        assert!((out[0][0] + 0.5).abs() < 1e-6);
    }

    #[test]
    fn state_round_trips_through_clap_streams() {
        let l = lib();
        let mut a = l.instantiate("dev.vieww.test-gain").unwrap();
        a.flush(&[HostEvent::Param {
            time: 0,
            id: 1,
            value: -18.0,
        }]);
        let saved = a.save().unwrap();
        let mut b = l.instantiate("dev.vieww.test-gain").unwrap();
        assert_eq!(b.value(1), Some(0.0));
        assert!(b.load(&saved));
        assert_eq!(b.value(1), Some(-18.0));
        assert!(!b.load(b"not a state"));
    }

    #[test]
    fn an_instrument_receives_notes_and_transport() {
        let l = lib();
        let mut p = l.instantiate("dev.vieww.test-synth").unwrap();
        assert_eq!(p.note_ports(), 1);
        assert!(p.audio_ports(true).is_empty());
        assert!(p.activate(48_000.0, 1, 512));
        let tr = super::host::HostTransport {
            playing: true,
            tempo: 128.0,
            beats: 4.5,
            time_signature: (7, 8),
        };
        let (out, st) = p.process(
            &[],
            2,
            256,
            &[
                HostEvent::NoteOn {
                    time: 100,
                    channel: 0,
                    key: 69,
                    velocity: 1.0,
                },
                HostEvent::Midi {
                    time: 200,
                    data: [0xB0, 1, 64],
                },
            ],
            Some(tr),
        );
        assert_eq!(st, crate::clap::sys::CLAP_PROCESS_CONTINUE);
        assert!(out[0][..100].iter().all(|v| *v == 0.0));
        assert!(out[0][101..].iter().any(|v| v.abs() > 0.1));
        assert_eq!(out[0], out[1]);
        let (out, st) = p.process(
            &[],
            2,
            64,
            &[HostEvent::NoteOff {
                time: 0,
                channel: 0,
                key: 69,
                velocity: 0.0,
            }],
            None,
        );
        assert_eq!(st, crate::clap::sys::CLAP_PROCESS_SLEEP);
        assert!(out[0].iter().all(|v| *v == 0.0));
    }
}
