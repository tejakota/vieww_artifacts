//! Audio plugins: write one [`Plugin`], ship it as CLAP and VST3, and host
//! either format.
//!
//! A plugin here is a vieww-audio [`Parametric`] (its parameters, with
//! ids, ranges, tapers and text conversion) plus a [`Plugin::process`]
//! that renders planar buffers given timed [`Event`]s and the host's
//! [`Transport`]. Everything else is the wrappers' job:
//!
//! * [`clap`] — the CLAP ABI (entry, factory, plugin, and the params,
//!   state, audio-ports, note-ports, latency and tail extensions),
//!   transcribed from the CLAP headers, exported with [`export_clap!`];
//!   plus a CLAP host that loads any `.clap` file.
//! * [`vst3`] — the VST3 COM ABI (`IPluginFactory`, `IComponent`,
//!   `IAudioProcessor`, `IEditController`, the event, parameter-change and
//!   stream interfaces), transcribed from Steinberg's interface headers,
//!   exported with [`export_vst3!`]; plus a VST3 host.
//!
//! Neither format's SDK is used: the ABIs are C (and, for VST3, COM-style
//! vtables), and both are written out in `sys` modules. The CI builds the
//! example plugins as shared libraries and drives them from hosts compiled
//! in C and C++ against the official headers, which is what proves the
//! transcription is the ABI and not just consistent with itself.
//!
//! Parameter automation is **sample-accurate** whichever format delivers
//! it: the shared [`Driver`] splits each block at every parameter change,
//! so a plugin only ever sees parameters that are constant within the
//! slice it is rendering.

pub mod clap;
pub mod vst3;

pub use vieww_audio::preset::{ParamInfo, Parametric, State, Taper};

/// Most channels a port may have.
pub const MAX_CHANNELS: usize = 8;
/// Events buffered per block (more are dropped, never allocated for).
pub const MAX_EVENTS: usize = 4_096;

/// What a plugin is, for the host's browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Notes in, audio out.
    Instrument,
    /// Audio in, audio out.
    Effect,
    /// Notes in, notes out (audio passes through).
    NoteEffect,
}

/// A plugin's identity and shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginInfo {
    /// Reverse-DNS id (the CLAP id): `"com.example.gain"`.
    pub id: &'static str,
    /// Display name.
    pub name: &'static str,
    /// Vendor.
    pub vendor: &'static str,
    /// Home page.
    pub url: &'static str,
    /// Version, `"1.2.3"`.
    pub version: &'static str,
    /// One line.
    pub description: &'static str,
    /// Instrument or effect.
    pub kind: Kind,
    /// Channels of the main audio input (0: none).
    pub inputs: u32,
    /// Channels of the main audio output.
    pub outputs: u32,
    /// Accepts notes / MIDI.
    pub note_input: bool,
    /// The VST3 class id (any 16 bytes, unique to this plugin, forever).
    pub vst3_class_id: [u8; 16],
    /// Extra CLAP feature tags (`"synthesizer"`, `"equalizer"` …) and the
    /// VST3 subcategories derived from them.
    pub features: &'static [&'static str],
}

/// A note expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expression {
    /// Gain, 0..4 (linear).
    Volume,
    /// Pan, 0 (left) ..1 (right).
    Pan,
    /// Tuning in semitones.
    Tuning,
    /// Vibrato depth 0..1.
    Vibrato,
    /// Expression 0..1.
    Expression,
    /// Brightness (timbre) 0..1.
    Brightness,
    /// Pressure 0..1.
    Pressure,
}

/// A timed event; `time` is the frame within the slice being rendered.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    /// A note starts.
    NoteOn {
        /// Frame.
        time: u32,
        /// MIDI channel 0..15.
        channel: u8,
        /// Key 0..127.
        key: u8,
        /// Velocity 0..1.
        velocity: f32,
        /// Host note id (−1 if none).
        note_id: i32,
    },
    /// A note is released.
    NoteOff {
        /// Frame.
        time: u32,
        /// MIDI channel.
        channel: u8,
        /// Key.
        key: u8,
        /// Release velocity.
        velocity: f32,
        /// Host note id (−1 if none).
        note_id: i32,
    },
    /// Stop a note now (no release).
    Choke {
        /// Frame.
        time: u32,
        /// MIDI channel.
        channel: u8,
        /// Key.
        key: u8,
        /// Host note id (−1 if none).
        note_id: i32,
    },
    /// A per-note expression.
    NoteExpression {
        /// Frame.
        time: u32,
        /// Which.
        expression: Expression,
        /// MIDI channel.
        channel: u8,
        /// Key.
        key: u8,
        /// Host note id (−1 if none).
        note_id: i32,
        /// Value (range by expression).
        value: f64,
    },
    /// A raw MIDI 1.0 message.
    Midi {
        /// Frame.
        time: u32,
        /// Status and data bytes.
        data: [u8; 3],
    },
}

impl Event {
    /// The event's frame.
    #[must_use]
    pub const fn time(&self) -> u32 {
        match *self {
            Self::NoteOn { time, .. }
            | Self::NoteOff { time, .. }
            | Self::Choke { time, .. }
            | Self::NoteExpression { time, .. }
            | Self::Midi { time, .. } => time,
        }
    }

    const fn with_time(mut self, t: u32) -> Self {
        match &mut self {
            Self::NoteOn { time, .. }
            | Self::NoteOff { time, .. }
            | Self::Choke { time, .. }
            | Self::NoteExpression { time, .. }
            | Self::Midi { time, .. } => *time = t,
        }
        self
    }
}

/// The host's transport, as far as it told us.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Transport {
    /// Playing.
    pub playing: bool,
    /// Recording.
    pub recording: bool,
    /// Looping.
    pub looping: bool,
    /// Tempo, BPM.
    pub tempo: Option<f64>,
    /// Song position in quarter notes.
    pub beats: Option<f64>,
    /// Song position in seconds.
    pub seconds: Option<f64>,
    /// Time signature.
    pub time_signature: Option<(u16, u16)>,
    /// Where the current bar started, in quarter notes.
    pub bar_start: Option<f64>,
    /// Loop range in quarter notes.
    pub loop_beats: Option<(f64, f64)>,
}

/// How processing went (the host may stop calling a sleeping plugin).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Keep calling.
    Continue,
    /// Keep calling while the input is not silent.
    ContinueIfNotQuiet,
    /// Ringing out a tail.
    Tail,
    /// Silent until new input or events.
    Sleep,
    /// Something went wrong.
    Error,
}

/// The audio a plugin renders: planar channels, a window into the host's
/// block.
#[derive(Debug)]
pub struct Buffers<'a, 'i, 'o> {
    inputs: &'a [&'i [f32]],
    outputs: &'a mut [&'o mut [f32]],
    start: usize,
    len: usize,
}

impl<'a, 'i, 'o> Buffers<'a, 'i, 'o> {
    /// A whole block.
    pub fn new(inputs: &'a [&'i [f32]], outputs: &'a mut [&'o mut [f32]], frames: usize) -> Self {
        Self {
            inputs,
            outputs,
            start: 0,
            len: frames,
        }
    }

    /// Frames in this slice.
    #[must_use]
    pub const fn frames(&self) -> usize {
        self.len
    }

    /// Input channels.
    #[must_use]
    pub fn input_channels(&self) -> usize {
        self.inputs.len()
    }

    /// Output channels.
    #[must_use]
    pub fn output_channels(&self) -> usize {
        self.outputs.len()
    }

    /// One input channel (empty if absent).
    #[must_use]
    pub fn input(&self, ch: usize) -> &[f32] {
        self.inputs
            .get(ch)
            .and_then(|c| c.get(self.start..self.start + self.len))
            .unwrap_or(&[])
    }

    /// One output channel (empty if absent).
    pub fn output(&mut self, ch: usize) -> &mut [f32] {
        let (s, n) = (self.start, self.len);
        match self.outputs.get_mut(ch) {
            Some(c) if c.len() >= s + n => &mut c[s..s + n],
            _ => &mut [],
        }
    }

    /// Copy inputs to outputs (an effect's bypass), zeroing extra outputs.
    pub fn pass_through(&mut self) {
        for ch in 0..self.outputs.len() {
            let (s, n) = (self.start, self.len);
            let src = self
                .inputs
                .get(ch)
                .and_then(|c| c.get(s..s + n))
                .unwrap_or(&[]);
            let dst = &mut self.outputs[ch];
            if dst.len() < s + n {
                continue;
            }
            if src.len() == n {
                dst[s..s + n].copy_from_slice(src);
            } else {
                dst[s..s + n].fill(0.0);
            }
        }
    }
}

/// A plugin.
pub trait Plugin: Parametric + Send + Sized + 'static {
    /// Identity and shape.
    fn info() -> &'static PluginInfo;

    /// A fresh instance.
    fn create() -> Self;

    /// Prepare for processing at `rate`, blocks of at most `max_block`.
    fn activate(&mut self, rate: f64, max_block: u32) -> bool {
        let _ = (rate, max_block);
        true
    }

    /// Processing has stopped.
    fn deactivate(&mut self) {}

    /// Clear internal state (tails, voices) without changing parameters.
    fn reset(&mut self) {}

    /// Render. Parameters are constant within the call (the driver splits
    /// blocks at changes); `events` are sorted by time.
    fn process(
        &mut self,
        buffers: &mut Buffers<'_, '_, '_>,
        events: &[Event],
        transport: &Transport,
    ) -> Status;

    /// Latency in samples.
    fn latency(&self) -> u32 {
        0
    }

    /// Tail length in samples after input stops (`u32::MAX`: infinite).
    fn tail(&self) -> u32 {
        0
    }

    /// The plugin's state (default: its parameters as a vieww-audio
    /// [`State`]).
    fn save_state(&self) -> Vec<u8> {
        State::capture(self, Self::info().name).to_bytes()
    }

    /// Restore a state; false if it is not one of ours.
    fn load_state(&mut self, data: &[u8]) -> bool {
        match State::from_bytes(data) {
            Ok(s) => {
                s.apply(self);
                true
            }
            Err(_) => false,
        }
    }
}

/// A parameter change at a frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParamChange {
    /// Frame.
    pub time: u32,
    /// Parameter id.
    pub id: u32,
    /// Plain value.
    pub value: f64,
}

/// The engine both wrappers share: a plugin, its parameter table, and
/// preallocated event scratch.
#[derive(Debug)]
pub struct Driver<P: Plugin> {
    /// The plugin.
    pub plugin: P,
    /// Its parameters (cached: `param_info` may allocate).
    pub params: Vec<ParamInfo>,
    events: Vec<Event>,
    changes: Vec<ParamChange>,
    slice: Vec<Event>,
    /// The latest transport.
    pub transport: Transport,
    /// The rate it was activated at.
    pub rate: f64,
    /// The largest block it was activated for.
    pub max_block: u32,
}

impl<P: Plugin> Driver<P> {
    /// A driver around a new instance.
    #[must_use]
    pub fn new() -> Self {
        let plugin = P::create();
        let params = plugin.param_info();
        Self {
            plugin,
            params,
            events: Vec::with_capacity(MAX_EVENTS),
            changes: Vec::with_capacity(MAX_EVENTS),
            slice: Vec::with_capacity(MAX_EVENTS),
            transport: Transport::default(),
            rate: 48_000.0,
            max_block: 4_096,
        }
    }

    /// A parameter's description by id.
    #[must_use]
    pub fn param(&self, id: u32) -> Option<&ParamInfo> {
        self.params.iter().find(|p| p.id == id)
    }

    /// Queue an event for the next [`run`](Self::run). Never allocates.
    pub fn push_event(&mut self, e: Event) {
        if self.events.len() < self.events.capacity() {
            self.events.push(e);
        }
    }

    /// Queue a parameter change (plain value) for the next run.
    pub fn push_change(&mut self, c: ParamChange) {
        if self.changes.len() < self.changes.capacity() {
            self.changes.push(c);
        }
    }

    /// Apply a parameter change immediately (outside processing).
    pub fn set(&mut self, id: u32, value: f64) {
        if let Some(info) = self.params.iter().find(|p| p.id == id) {
            let v = info.clamp(value);
            self.plugin.set_param(id, v);
        }
    }

    /// Render `frames` frames, applying queued changes at their frames and
    /// delivering queued events to the slices they fall in. Never
    /// allocates.
    pub fn run(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], frames: usize) -> Status {
        let _ftz = DenormalGuard::new();
        // Sorted, stable: same-frame events keep the host's order.
        self.events.sort_by_key(Event::time);
        self.changes.sort_by_key(|c| c.time);
        let n = u32::try_from(frames).unwrap_or(u32::MAX);
        let mut cursor = 0u32;
        let mut ev = 0usize;
        let mut status = Status::Sleep;
        let mut ci = 0usize;
        loop {
            // The next boundary: the next change's frame, or the end.
            let next = self.changes.get(ci).map_or(n, |c| c.time.min(n));
            if next > cursor {
                self.slice.clear();
                while ev < self.events.len() && self.events[ev].time() < next {
                    let e = self.events[ev];
                    self.slice
                        .push(e.with_time(e.time().saturating_sub(cursor)));
                    ev += 1;
                }
                let mut b = Buffers {
                    inputs,
                    outputs,
                    start: cursor as usize,
                    len: (next - cursor) as usize,
                };
                let s = self.plugin.process(&mut b, &self.slice, &self.transport);
                status = merge(status, s);
                cursor = next;
            }
            if ci >= self.changes.len() {
                break;
            }
            // Apply every change at this frame.
            while ci < self.changes.len() && self.changes[ci].time.min(n) == next {
                let c = self.changes[ci];
                self.set(c.id, c.value);
                ci += 1;
            }
        }
        // Events past the end (a misbehaving host) go in the last slice's
        // place: deliver them at the final frame rather than lose them.
        if ev < self.events.len() && frames > 0 {
            self.slice.clear();
            for e in &self.events[ev..] {
                self.slice.push(e.with_time(0));
            }
            let mut b = Buffers {
                inputs,
                outputs,
                start: frames - 1,
                len: 1,
            };
            let _ = self.plugin.process(&mut b, &self.slice, &self.transport);
        }
        self.events.clear();
        self.changes.clear();
        if frames == 0 {
            Status::Continue
        } else {
            status
        }
    }
}

impl<P: Plugin> Default for Driver<P> {
    fn default() -> Self {
        Self::new()
    }
}

/// Flush-to-zero and denormals-are-zero for as long as it lives.
///
/// Recursive filters decaying towards silence produce subnormal floats,
/// which many CPUs process tens of times slower; every plugin host
/// expects a plugin to switch them off while it processes. The previous
/// mode is restored on drop, so the host's own code is unaffected.
#[derive(Debug)]
pub struct DenormalGuard {
    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    saved: u64,
}

impl DenormalGuard {
    /// Switch denormals off on this thread.
    #[must_use]
    pub fn new() -> Self {
        #[cfg(target_arch = "x86_64")]
        {
            let mut csr: u32 = 0;
            // SAFETY: reading and writing MXCSR affects only this thread's
            // SSE rounding/denormal modes; FTZ (bit 15) and DAZ (bit 6) are
            // defined on every x86-64 CPU.
            unsafe {
                std::arch::asm!("stmxcsr [{}]", in(reg) &raw mut csr, options(nostack));
                let new = csr | (1 << 15) | (1 << 6);
                std::arch::asm!("ldmxcsr [{}]", in(reg) &raw const new, options(nostack, readonly));
            }
            Self {
                saved: u64::from(csr),
            }
        }
        #[cfg(target_arch = "aarch64")]
        {
            let fpcr: u64;
            // SAFETY: FPCR.FZ (bit 24) flushes subnormals for this thread
            // only; the register exists on every AArch64 CPU.
            unsafe {
                std::arch::asm!("mrs {}, fpcr", out(reg) fpcr, options(nomem, nostack));
                std::arch::asm!("msr fpcr, {}", in(reg) fpcr | (1 << 24), options(nomem, nostack));
            }
            Self { saved: fpcr }
        }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            Self {}
        }
    }
}

impl Default for DenormalGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DenormalGuard {
    fn drop(&mut self) {
        #[cfg(target_arch = "x86_64")]
        {
            #[expect(clippy::cast_possible_truncation, reason = "MXCSR is 32 bits")]
            let csr = self.saved as u32;
            // SAFETY: restores the value read in `new`.
            unsafe {
                std::arch::asm!("ldmxcsr [{}]", in(reg) &raw const csr, options(nostack, readonly));
            }
        }
        #[cfg(target_arch = "aarch64")]
        // SAFETY: restores the value read in `new`.
        unsafe {
            std::arch::asm!("msr fpcr, {}", in(reg) self.saved, options(nomem, nostack));
        }
    }
}

fn merge(a: Status, b: Status) -> Status {
    use Status::{Continue, ContinueIfNotQuiet, Error, Sleep, Tail};
    match (a, b) {
        (Error, _) | (_, Error) => Error,
        (Continue, _) | (_, Continue) => Continue,
        (Tail, _) | (_, Tail) => Tail,
        (ContinueIfNotQuiet, _) | (_, ContinueIfNotQuiet) => ContinueIfNotQuiet,
        (Sleep, Sleep) => Sleep,
    }
}

/// Copy a Rust string into a fixed C buffer, NUL-terminated and
/// truncated at a character boundary.
pub(crate) fn write_c_str(dst: &mut [std::ffi::c_char], s: &str) {
    if dst.is_empty() {
        return;
    }
    let mut n = s.len().min(dst.len() - 1);
    while !s.is_char_boundary(n) {
        n -= 1;
    }
    for (d, b) in dst.iter_mut().zip(&s.as_bytes()[..n]) {
        *d = b.cast_signed();
    }
    dst[n] = 0;
}

#[cfg(test)]
pub(crate) mod testing {
    //! Two plugins the format tests share.
    use super::*;

    /// A gain/pan effect with a lookahead-like fixed latency.
    #[derive(Debug, Default)]
    pub(crate) struct Gain {
        pub(crate) gain_db: f64,
        pub(crate) invert: f64,
        /// Every (slice start-independent) gain change, for automation tests.
        pub(crate) slices: Vec<(usize, f64)>,
    }

    pub(crate) static GAIN_INFO: PluginInfo = PluginInfo {
        id: "dev.vieww.test-gain",
        name: "Test Gain",
        vendor: "vieww",
        url: "https://vieww.dev",
        version: "1.0.0",
        description: "Gain for the wrapper tests",
        kind: Kind::Effect,
        inputs: 2,
        outputs: 2,
        note_input: false,
        vst3_class_id: *b"vieww-test-gain!",
        features: &["utility", "stereo"],
    };

    impl Parametric for Gain {
        fn param_info(&self) -> Vec<ParamInfo> {
            vec![
                ParamInfo::new(1, "Gain", -60.0, 12.0, 0.0).unit("dB"),
                ParamInfo::toggle(2, "Invert", false),
            ]
        }
        fn get_param(&self, id: u32) -> Option<f64> {
            match id {
                1 => Some(self.gain_db),
                2 => Some(self.invert),
                _ => None,
            }
        }
        fn set_param(&mut self, id: u32, v: f64) {
            match id {
                1 => self.gain_db = v,
                2 => self.invert = v,
                _ => {}
            }
        }
    }

    impl Plugin for Gain {
        fn info() -> &'static PluginInfo {
            &GAIN_INFO
        }
        fn create() -> Self {
            Self::default()
        }
        fn process(&mut self, b: &mut Buffers<'_, '_, '_>, _: &[Event], _: &Transport) -> Status {
            self.slices.push((b.frames(), self.gain_db));
            #[expect(clippy::cast_possible_truncation, reason = "audio")]
            let g = (10f64.powf(self.gain_db / 20.0) * if self.invert >= 0.5 { -1.0 } else { 1.0 })
                as f32;
            for ch in 0..b.output_channels() {
                let n = b.frames();
                let mut tmp = [0.0f32; 64];
                let mut done = 0;
                while done < n {
                    let k = (n - done).min(64);
                    let src = b.input(ch);
                    for (i, t) in tmp.iter_mut().enumerate().take(k) {
                        *t = src.get(done + i).copied().unwrap_or(0.0) * g;
                    }
                    b.output(ch)[done..done + k].copy_from_slice(&tmp[..k]);
                    done += k;
                }
            }
            Status::ContinueIfNotQuiet
        }
        fn latency(&self) -> u32 {
            64
        }
    }

    /// A sine instrument: one voice per key, logs the events it saw.
    #[derive(Debug, Default)]
    pub(crate) struct Synth {
        pub(crate) level: f64,
        pub(crate) voices: Vec<(u8, f64, f64)>, // key, phase, velocity
        pub(crate) seen: Vec<Event>,
        pub(crate) transport: Option<Transport>,
        rate: f64,
    }

    pub(crate) static SYNTH_INFO: PluginInfo = PluginInfo {
        id: "dev.vieww.test-synth",
        name: "Test Synth",
        vendor: "vieww",
        url: "https://vieww.dev",
        version: "0.3.1",
        description: "Sines for the wrapper tests",
        kind: Kind::Instrument,
        inputs: 0,
        outputs: 2,
        note_input: true,
        vst3_class_id: *b"vieww-test-synth",
        features: &["synthesizer", "stereo"],
    };

    impl Parametric for Synth {
        fn param_info(&self) -> Vec<ParamInfo> {
            vec![ParamInfo::new(10, "Level", 0.0, 1.0, 0.5)]
        }
        fn get_param(&self, id: u32) -> Option<f64> {
            (id == 10).then_some(self.level)
        }
        fn set_param(&mut self, id: u32, v: f64) {
            if id == 10 {
                self.level = v;
            }
        }
    }

    impl Plugin for Synth {
        fn info() -> &'static PluginInfo {
            &SYNTH_INFO
        }
        fn create() -> Self {
            Self {
                level: 0.5,
                ..Self::default()
            }
        }
        fn activate(&mut self, rate: f64, _: u32) -> bool {
            self.rate = rate;
            true
        }
        fn reset(&mut self) {
            self.voices.clear();
        }
        fn process(
            &mut self,
            b: &mut Buffers<'_, '_, '_>,
            events: &[Event],
            t: &Transport,
        ) -> Status {
            self.transport = Some(*t);
            let mut ei = 0;
            for f in 0..b.frames() {
                while ei < events.len() && events[ei].time() as usize <= f {
                    let e = events[ei];
                    self.seen.push(e);
                    match e {
                        Event::NoteOn { key, velocity, .. } => {
                            self.voices.push((key, 0.0, f64::from(velocity)));
                        }
                        Event::NoteOff { key, .. } | Event::Choke { key, .. } => {
                            self.voices.retain(|v| v.0 != key);
                        }
                        _ => {}
                    }
                    ei += 1;
                }
                let mut s = 0.0;
                for v in &mut self.voices {
                    let hz = 440.0 * 2f64.powf((f64::from(v.0) - 69.0) / 12.0);
                    s += v.1.sin() * v.2 * self.level;
                    v.1 += std::f64::consts::TAU * hz / self.rate.max(1.0);
                }
                #[expect(clippy::cast_possible_truncation, reason = "audio")]
                let s = s as f32;
                b.output(0)[f] = s;
                b.output(1)[f] = s;
            }
            if self.voices.is_empty() {
                Status::Sleep
            } else {
                Status::Continue
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{Gain, Synth};
    use super::*;

    #[test]
    fn automation_is_sample_accurate() {
        let mut d: Driver<Gain> = Driver::new();
        let input = vec![1.0f32; 100];
        let mut l = vec![0.0f32; 100];
        let mut r = vec![0.0f32; 100];
        d.push_change(ParamChange {
            time: 30,
            id: 1,
            value: -6.020_599_913_279_624,
        });
        d.push_change(ParamChange {
            time: 70,
            id: 2,
            value: 1.0,
        });
        d.push_change(ParamChange {
            time: 70,
            id: 1,
            value: 500.0, // clamped to +12
        });
        let ins: [&[f32]; 2] = [&input, &input];
        let st = {
            let mut outs: [&mut [f32]; 2] = [&mut l, &mut r];
            d.run(&ins, &mut outs, 100)
        };
        assert_eq!(st, Status::ContinueIfNotQuiet);
        assert_eq!(l[29], 1.0);
        assert!((l[30] - 0.5).abs() < 1e-6);
        assert!((l[69] - 0.5).abs() < 1e-6);
        assert!((l[70] + 3.981).abs() < 1e-3, "{}", l[70]);
        assert_eq!(r, l);
        assert_eq!(
            d.plugin.slices.iter().map(|s| s.0).collect::<Vec<_>>(),
            vec![30, 40, 30]
        );
    }

    #[test]
    fn events_arrive_relative_to_their_slice() {
        let mut d: Driver<Synth> = Driver::new();
        d.plugin.activate(48_000.0, 512);
        d.push_event(Event::NoteOn {
            time: 50,
            channel: 0,
            key: 69,
            velocity: 1.0,
            note_id: 7,
        });
        d.push_event(Event::NoteOff {
            time: 10,
            channel: 0,
            key: 60,
            velocity: 0.0,
            note_id: -1,
        });
        d.push_change(ParamChange {
            time: 40,
            id: 10,
            value: 0.25,
        });
        let mut l = vec![0.0f32; 64];
        let mut r = vec![0.0f32; 64];
        let st = {
            let mut outs: [&mut [f32]; 2] = [&mut l, &mut r];
            d.run(&[], &mut outs, 64)
        };
        assert_eq!(st, Status::Continue);
        // Sorted by time; the note-on at frame 50 is frame 10 of the slice
        // starting at the change (40).
        assert_eq!(d.plugin.seen[0].time(), 10);
        assert!(matches!(d.plugin.seen[0], Event::NoteOff { .. }));
        assert_eq!(d.plugin.seen[1].time(), 10);
        assert!(matches!(d.plugin.seen[1], Event::NoteOn { note_id: 7, .. }));
        assert!(l[..50].iter().all(|v| *v == 0.0));
        assert!(l[51].abs() > 0.0 && l[51].abs() <= 0.25);
    }

    #[test]
    fn buffers_window_and_pass_through() {
        let a = [1.0f32, 2.0, 3.0, 4.0];
        let mut o = [0.0f32; 4];
        let ins: [&[f32]; 1] = [&a];
        let mut outs: [&mut [f32]; 1] = [&mut o];
        let mut b = Buffers::new(&ins, &mut outs, 4);
        b.start = 1;
        b.len = 2;
        assert_eq!(b.input(0), &[2.0, 3.0]);
        assert!(b.input(5).is_empty());
        assert!(b.output(3).is_empty());
        b.pass_through();
        assert_eq!(o, [0.0, 2.0, 3.0, 0.0]);
    }

    #[test]
    fn denormals_flush_inside_the_guard_and_return_after() {
        let tiny = std::hint::black_box(1.0e-30_f32);
        let scale = std::hint::black_box(1.0e-10_f32);
        let sub = tiny * scale;
        assert!(
            sub != 0.0 && sub.is_subnormal(),
            "subnormals exist normally"
        );
        {
            let _g = DenormalGuard::new();
            let flushed = std::hint::black_box(tiny) * std::hint::black_box(scale);
            assert_eq!(flushed, 0.0, "flushed under the guard");
        }
        let again = std::hint::black_box(tiny) * std::hint::black_box(scale);
        assert!(again.is_subnormal(), "mode restored");
    }

    #[test]
    fn default_state_round_trips_parameters() {
        let mut g = Gain::create();
        g.set_param(1, -12.0);
        let s = g.save_state();
        let mut h = Gain::create();
        assert!(h.load_state(&s));
        assert_eq!(h.gain_db, -12.0);
        assert!(!h.load_state(b"junk"));
    }

    #[test]
    fn c_strings_truncate_on_char_boundaries() {
        let mut buf = [1 as std::ffi::c_char; 6];
        write_c_str(&mut buf, "héllo!");
        let bytes: Vec<u8> = buf.iter().map(|c| c.cast_unsigned()).collect();
        assert_eq!(&bytes[..], b"h\xc3\xa9ll\0");
        let mut tiny = [1 as std::ffi::c_char; 2];
        write_c_str(&mut tiny, "é");
        assert_eq!(tiny[0], 0);
    }
}
