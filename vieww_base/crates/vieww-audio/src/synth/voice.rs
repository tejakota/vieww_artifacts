//! Polyphony: turning keys into sounding voices.
//!
//! A synth engine (one [`Voice`]) plays one note. Playing *chords* needs a
//! pool of them and a policy for everything a keyboard can throw at it:
//! more keys than voices (steal one — which?), the sustain pedal (hold the
//! release), unison (several detuned voices per key, spread across the
//! stereo field), mono and legato modes (one voice; glide between notes;
//! don't retrigger while a key is still down), and MPE — MIDI Polyphonic
//! Expression, where every note lives on its own channel so each can bend,
//! press and brighten independently (the ROLI Seaboard, the Linnstrument,
//! and CLAP's per-note modulation).
//!
//! [`Poly`] is that pool. It is generic over the voice, so the FM, wavetable,
//! additive, sampler and physical-model engines all get the same keyboard
//! behaviour, and it speaks [`midi::Message`](crate::midi::Message)
//! directly.

use crate::midi::Message;

/// Per-note expression (MPE dimensions plus the channel's own controls).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Expression {
    /// Pitch offset in semitones (pitch bend).
    pub bend: f32,
    /// Pressure / aftertouch, 0..=1.
    pub pressure: f32,
    /// Timbre (MPE "slide", CC 74), 0..=1.
    pub timbre: f32,
}

/// One note's sound engine.
pub trait Voice {
    /// Start a note at `pitch` (MIDI note number, fractional allowed) and
    /// `velocity` 0..=1.
    fn note_on(&mut self, pitch: f32, velocity: f32);
    /// Release.
    fn note_off(&mut self);
    /// Change pitch while sounding (glide, bend, detune).
    fn set_pitch(&mut self, pitch: f32);
    /// Apply expression (bend is already folded into `set_pitch`).
    fn set_expression(&mut self, _e: Expression) {}
    /// Still producing sound?
    fn is_active(&self) -> bool;
    /// The next sample.
    fn next(&mut self) -> f32;
    /// A loudness estimate for quietest-voice stealing.
    fn level(&self) -> f32 {
        1.0
    }
    /// Stop immediately (a stolen voice). The default releases; engines
    /// that can should fade over a few milliseconds instead.
    fn kill(&mut self) {
        self.note_off();
    }
}

/// Which voice gives way when all are busy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Steal {
    /// The longest-sounding.
    #[default]
    Oldest,
    /// The quietest (by [`Voice::level`]); released voices first.
    Quietest,
    /// The lowest pitch.
    Lowest,
    /// The highest pitch.
    Highest,
    /// Never steal: drop the new note.
    None,
}

/// Keyboard mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Many notes at once.
    #[default]
    Poly,
    /// One note; every new key retriggers.
    Mono,
    /// One note; a new key while another is held glides without retrigger.
    Legato,
}

/// Unison stacking.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Unison {
    /// Voices per key (1 = off).
    pub voices: usize,
    /// Total detune spread in cents (outermost voices at ±half).
    pub detune_cents: f32,
    /// Stereo spread 0..=1.
    pub spread: f32,
}

impl Default for Unison {
    fn default() -> Self {
        Self {
            voices: 1,
            detune_cents: 0.0,
            spread: 0.0,
        }
    }
}

#[derive(Debug)]
struct Slot<V> {
    voice: V,
    channel: u8,
    key: Option<u8>,
    held: bool,
    sustained: bool,
    age: u64,
    detune: f32,
    pan: f32,
    /// Glide state in f64: an f32 exponential approach stalls short of its
    /// target once `diff × (1 − k)` drops under half an ULP.
    pitch: f64,
    target: f32,
}

/// A voice pool.
#[derive(Debug)]
pub struct Poly<V: Voice> {
    slots: Vec<Slot<V>>,
    /// Stealing policy.
    pub steal: Steal,
    /// Keyboard mode.
    pub mode: Mode,
    /// Unison stacking.
    pub unison: Unison,
    /// Portamento time constant in seconds (0 = off; always on in legato
    /// transitions when set).
    pub glide: f32,
    /// Pitch-bend range in semitones (48 is the MPE default on member
    /// channels; 2 is the classic keyboard default).
    pub bend_range: f32,
    /// MPE lower zone: channel 0 is the master, 1..=15 are per-note.
    pub mpe: bool,
    rate: f32,
    clock: u64,
    pedal: [bool; 16],
    channel_expr: [Expression; 16],
    /// Held keys in mono modes: `(channel, key, pitch, velocity)`.
    mono_stack: Vec<(u8, u8, f32, f32)>,
    /// The tuning notes are played in; `None` is 12-TET at A4 = 440.
    tuning: Option<Box<super::tuning::TuningTable>>,
}

impl<V: Voice> Poly<V> {
    /// A pool of the given voices at `rate`.
    #[must_use]
    pub fn new(voices: Vec<V>, rate: f32) -> Self {
        let slots = voices
            .into_iter()
            .map(|voice| Slot {
                voice,
                channel: 0,
                key: None,
                held: false,
                sustained: false,
                age: 0,
                detune: 0.0,
                pan: 0.0,
                pitch: 60.0,
                target: 60.0,
            })
            .collect();
        Self {
            slots,
            steal: Steal::default(),
            mode: Mode::default(),
            unison: Unison::default(),
            glide: 0.0,
            bend_range: 2.0,
            mpe: false,
            rate,
            clock: 0,
            pedal: [false; 16],
            channel_expr: [Expression::default(); 16],
            // Room for every key on every channel's worth of a hand: a
            // push on the audio thread must not grow the stack.
            mono_stack: Vec::with_capacity(128),
            tuning: None,
        }
    }

    /// Voices currently producing sound.
    #[must_use]
    pub fn active(&self) -> usize {
        self.slots.iter().filter(|s| s.voice.is_active()).count()
    }

    /// The voices, to adjust their settings (a filter cutoff, say) in
    /// place.
    pub fn voices_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.slots.iter_mut().map(|s| &mut s.voice)
    }

    /// Stop every note: released, or silenced at once if `immediate`.
    pub fn all_notes_off(&mut self, immediate: bool) {
        self.all_off(immediate);
    }

    /// Keys currently held (by any voice).
    #[must_use]
    pub fn held_keys(&self) -> Vec<u8> {
        let mut k: Vec<u8> = self
            .slots
            .iter()
            .filter(|s| s.held)
            .filter_map(|s| s.key)
            .collect();
        k.sort_unstable();
        k.dedup();
        k
    }

    /// Feed one MIDI message.
    pub fn midi(&mut self, m: &Message) {
        match m.clone().normalized() {
            Message::NoteOn {
                channel,
                key,
                velocity,
            } => self.note_on(channel, key, f32::from(velocity) / 127.0),
            Message::NoteOff { channel, key, .. } => self.note_off(channel, key),
            Message::PitchBend { channel, value } => {
                let range = if self.mpe && channel != 0 {
                    self.bend_range
                } else if self.mpe {
                    2.0
                } else {
                    self.bend_range
                };
                self.channel_expr[usize::from(channel & 15)].bend =
                    f32::from(value) / 8192.0 * range;
            }
            Message::ChannelPressure { channel, pressure } => {
                self.channel_expr[usize::from(channel & 15)].pressure = f32::from(pressure) / 127.0;
            }
            Message::ControlChange {
                channel,
                controller,
                value,
            } => match controller {
                64 => self.pedal(channel, value >= 64),
                74 => {
                    self.channel_expr[usize::from(channel & 15)].timbre = f32::from(value) / 127.0;
                }
                120 | 123 => self.all_off(controller == 120),
                _ => {}
            },
            _ => {}
        }
    }

    /// Sustain pedal down/up on `channel` (all channels in MPE mode).
    pub fn pedal(&mut self, channel: u8, down: bool) {
        let chans: Vec<usize> = if self.mpe {
            (0..16).collect()
        } else {
            vec![usize::from(channel & 15)]
        };
        for c in chans {
            self.pedal[c] = down;
        }
        if !down {
            for s in &mut self.slots {
                if s.sustained && !s.held {
                    s.sustained = false;
                    s.key = None;
                    s.voice.note_off();
                }
            }
        }
    }

    fn all_off(&mut self, immediate: bool) {
        for s in &mut self.slots {
            if immediate {
                s.voice.kill();
            } else {
                s.voice.note_off();
            }
            s.held = false;
            s.sustained = false;
            s.key = None;
        }
        self.mono_stack.clear();
    }

    /// The slot for the next voice of a press: a free one, else one to
    /// steal by policy, released voices before held ones. Slots already
    /// claimed by this press (`age == clock`) are skipped. Allocates
    /// nothing — it runs on the audio thread at every note-on.
    fn pick(&self) -> Option<usize> {
        let now = self.clock;
        let open = |s: &Slot<V>| s.age != now;
        if let Some(i) = self
            .slots
            .iter()
            .position(|s| open(s) && !s.voice.is_active())
        {
            return Some(i);
        }
        let any_released = self.slots.iter().any(|s| open(s) && !s.held);
        let in_pool = |s: &Slot<V>| open(s) && (!any_released || !s.held);
        #[expect(clippy::cast_precision_loss, reason = "age ordering only")]
        let score = |s: &Slot<V>| -> f32 {
            match self.steal {
                Steal::None | Steal::Oldest => s.age as f32,
                Steal::Quietest => s.voice.level(),
                Steal::Lowest => s.target,
                Steal::Highest => -s.target,
            }
        };
        if self.steal == Steal::None {
            return None;
        }
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, s)| in_pool(s))
            .min_by(|(_, a), (_, b)| score(a).total_cmp(&score(b)))
            .map(|(i, _)| i)
    }

    /// Play every note in `tuning` (see [`super::tuning`]); `None` returns
    /// to 12-TET at A4 = 440. Takes effect on sounding notes at once.
    /// Call off the audio thread — or hand over a table built there.
    pub fn set_tuning(&mut self, tuning: Option<super::tuning::TuningTable>) {
        self.tuning = tuning.map(Box::new);
    }

    /// The tuning in use, if not the default.
    #[must_use]
    pub fn tuning(&self) -> Option<&super::tuning::TuningTable> {
        self.tuning.as_deref()
    }

    /// Press a note at a fractional `pitch` — a microtone, a swara between
    /// keys — identified by `key` for its [`note_off`](Self::note_off).
    /// The pitch passes through the tuning like any other.
    pub fn note_on_pitch(&mut self, channel: u8, key: u8, pitch: f32, velocity: f32) {
        if velocity <= 0.0 {
            self.note_off(channel, key);
            return;
        }
        self.press(channel, key, pitch, velocity);
    }

    /// Press a key.
    pub fn note_on(&mut self, channel: u8, key: u8, velocity: f32) {
        if velocity <= 0.0 {
            self.note_off(channel, key);
            return;
        }
        self.press(channel, key, f32::from(key), velocity);
    }

    fn press(&mut self, channel: u8, key: u8, pitch: f32, velocity: f32) {
        match self.mode {
            Mode::Poly => self.start(channel, key, pitch, velocity, false),
            Mode::Mono | Mode::Legato => {
                let legato = self.mode == Mode::Legato && !self.mono_stack.is_empty();
                self.mono_stack
                    .retain(|(c, k, _, _)| !(*c == channel && *k == key));
                self.mono_stack.push((channel, key, pitch, velocity));
                self.start(channel, key, pitch, velocity, legato);
            }
        }
    }

    fn start(&mut self, channel: u8, key: u8, pitch: f32, velocity: f32, legato: bool) {
        self.clock += 1;
        let n = self.unison.voices.max(1);
        if self.mode != Mode::Poly {
            // Mono: reuse the first n voices.
            for (u, s) in self.slots.iter_mut().take(n).enumerate() {
                s.channel = channel;
                s.key = Some(key);
                s.held = true;
                s.sustained = false;
                s.age = self.clock;
                s.target = pitch;
                let (d, p) = unison_slot(u, n, self.unison);
                s.detune = d;
                s.pan = p;
                if !legato || !s.voice.is_active() {
                    if self.glide <= 0.0 || !s.voice.is_active() {
                        s.pitch = f64::from(pitch);
                    }
                    #[expect(clippy::cast_possible_truncation, reason = "pitch in f32 is ample")]
                    let tuned = retuned(self.tuning.as_deref(), s.pitch as f32);
                    s.voice.note_on(tuned + s.detune, velocity);
                }
            }
            return;
        }
        // Slots claimed by this press carry `age == clock` (set below), so
        // `pick` skips them without a list — no allocation on note-on.
        for u in 0..n {
            let Some(i) = self.pick() else {
                return;
            };
            let (d, p) = unison_slot(u, n, self.unison);
            let tuned = retuned(self.tuning.as_deref(), pitch);
            let s = &mut self.slots[i];
            if s.voice.is_active() {
                s.voice.kill();
            }
            s.channel = channel;
            s.key = Some(key);
            s.held = true;
            s.sustained = false;
            s.age = self.clock;
            s.detune = d;
            s.pan = p;
            s.target = pitch;
            s.pitch = f64::from(pitch);
            s.voice.note_on(tuned + d, velocity);
        }
    }

    /// Release a key.
    pub fn note_off(&mut self, channel: u8, key: u8) {
        if self.mode != Mode::Poly {
            self.mono_stack
                .retain(|(c, k, _, _)| !(*c == channel && *k == key));
            if let Some(&(c, k, p, v)) = self.mono_stack.last() {
                // Return to the previous held key, at its own pitch.
                let legato = self.mode == Mode::Legato;
                self.start(c, k, p, v, legato);
                return;
            }
        }
        let ped = self.pedal[usize::from(channel & 15)];
        for s in &mut self.slots {
            if s.held && s.key == Some(key) && s.channel == channel {
                s.held = false;
                if ped {
                    s.sustained = true;
                } else {
                    s.key = None;
                    s.voice.note_off();
                }
            }
        }
    }

    /// The next stereo frame.
    pub fn next_stereo(&mut self) -> (f32, f32) {
        let glide_k = if self.glide > 0.0 {
            (-1.0 / (f64::from(self.glide) * f64::from(self.rate))).exp()
        } else {
            0.0
        };
        let master = self.channel_expr[0];
        let tuning = self.tuning.as_deref();
        let mut l = 0.0;
        let mut r = 0.0;
        for s in &mut self.slots {
            if !s.voice.is_active() {
                continue;
            }
            let target = f64::from(s.target);
            s.pitch = target + (s.pitch - target) * glide_k;
            #[expect(clippy::cast_possible_truncation, reason = "pitch in f32 is ample")]
            let pitch = s.pitch as f32;
            let ch = self.channel_expr[usize::from(s.channel & 15)];
            let expr = if self.mpe && s.channel != 0 {
                Expression {
                    bend: ch.bend + master.bend,
                    pressure: ch.pressure,
                    timbre: ch.timbre,
                }
            } else {
                ch
            };
            s.voice
                .set_pitch(retuned(tuning, pitch) + s.detune + expr.bend);
            s.voice.set_expression(expr);
            let v = s.voice.next();
            let (gl, gr) = super::equal_power(s.pan);
            l += v * gl;
            r += v * gr;
        }
        #[expect(clippy::cast_precision_loss, reason = "voice counts")]
        let norm = 1.0 / (self.unison.voices.max(1) as f32).sqrt();
        (l * norm, r * norm)
    }

    /// Render into `out`, interleaved stereo (`out.len() / 2` frames).
    /// Allocates nothing: the realtime spelling of [`render`](Self::render).
    pub fn render_into(&mut self, out: &mut [f32]) {
        for f in out.as_chunks_mut::<2>().0 {
            let (l, r) = self.next_stereo();
            f[0] = l;
            f[1] = r;
        }
    }

    /// Render `frames` stereo frames, interleaved, into a new buffer.
    /// Allocates: for offline use. On an audio thread use
    /// [`render_into`](Self::render_into).
    pub fn render(&mut self, frames: usize) -> Vec<f32> {
        let mut out = Vec::with_capacity(frames * 2);
        for _ in 0..frames {
            let (l, r) = self.next_stereo();
            out.push(l);
            out.push(r);
        }
        out
    }
}

/// `pitch` through `tuning`, or unchanged under the default 12-TET.
#[inline]
fn retuned(tuning: Option<&super::tuning::TuningTable>, pitch: f32) -> f32 {
    tuning.map_or(pitch, |t| t.retune(pitch))
}

fn unison_slot(u: usize, n: usize, uni: Unison) -> (f32, f32) {
    if n <= 1 {
        return (0.0, 0.0);
    }
    #[expect(clippy::cast_precision_loss, reason = "voice counts")]
    let x = u as f32 / (n - 1) as f32 * 2.0 - 1.0;
    (x * uni.detune_cents / 200.0, x * uni.spread)
}

/// A ready-made subtractive voice: two band-limited oscillators, a
/// resonant state-variable filter with its own envelope, and an amp ADSR —
/// the Minimoog/Juno signal path, and the voice the polyphony tests use.
#[derive(Debug, Clone)]
pub struct SubtractiveVoice {
    osc1: super::osc::Oscillator,
    osc2: super::osc::Oscillator,
    /// Osc 2 detune in semitones.
    pub osc2_semis: f32,
    /// Osc 2 mix 0..=1.
    pub osc2_mix: f32,
    filter: crate::fx::filter::Svf,
    /// Filter base cutoff in Hz.
    pub cutoff: f32,
    /// Filter resonance (Q).
    pub resonance: f32,
    /// Filter envelope depth in octaves.
    pub env_amount: f32,
    /// Keyboard tracking 0..=1.
    pub key_track: f32,
    amp: super::envelope::EnvelopeGen,
    fenv: super::envelope::EnvelopeGen,
    pitch: f32,
    velocity: f32,
    timbre: f32,
}

impl SubtractiveVoice {
    /// A voice at `rate` with two saws, a 12 dB low-pass and a plucky
    /// filter envelope.
    #[must_use]
    pub fn new(rate: f32) -> Self {
        use super::envelope::Adsr;
        use super::osc::{Oscillator, Shape};
        Self {
            osc1: Oscillator::new(Shape::Saw, 440.0, rate),
            osc2: Oscillator::new(Shape::Saw, 440.0, rate),
            osc2_semis: 0.07,
            osc2_mix: 0.5,
            filter: crate::fx::filter::Svf::new(
                crate::fx::filter::SvfMode::LowPass,
                1200.0,
                1.2,
                rate,
            ),
            cutoff: 800.0,
            resonance: 1.2,
            env_amount: 3.0,
            key_track: 0.5,
            amp: super::envelope::EnvelopeGen::new(Adsr::new(0.005, 0.3, 0.7, 0.25).analog(), rate),
            fenv: super::envelope::EnvelopeGen::new(Adsr::new(0.002, 0.4, 0.2, 0.3).analog(), rate),
            pitch: 69.0,
            velocity: 1.0,
            timbre: 0.0,
        }
    }
}

/// MIDI note number to Hz (A4 = 69 = 440 Hz).
#[must_use]
pub fn pitch_to_hz(pitch: f32) -> f32 {
    440.0 * 2f32.powf((pitch - 69.0) / 12.0)
}

impl Voice for SubtractiveVoice {
    fn note_on(&mut self, pitch: f32, velocity: f32) {
        self.velocity = velocity;
        self.set_pitch(pitch);
        self.amp.gate_on_velocity(velocity);
        self.fenv.gate_on_velocity(velocity);
    }

    fn note_off(&mut self) {
        self.amp.gate_off();
        self.fenv.gate_off();
    }

    fn set_pitch(&mut self, pitch: f32) {
        self.pitch = pitch;
        self.osc1.set_frequency(pitch_to_hz(pitch));
        self.osc2
            .set_frequency(pitch_to_hz(pitch + self.osc2_semis));
    }

    fn set_expression(&mut self, e: Expression) {
        self.timbre = e.timbre;
    }

    fn is_active(&self) -> bool {
        self.amp.is_active()
    }

    fn level(&self) -> f32 {
        self.amp.level()
    }

    fn kill(&mut self) {
        // A three-millisecond fade, not a reset: the reset this replaced
        // snapped both envelopes to zero at whatever amplitude the voice
        // had — the click voice stealing exists to avoid — and the trait's
        // own doc says engines "should fade over a few milliseconds
        // instead". The retrigger that follows a steal attacks from the
        // current level, so the fade crossfades into the new note rather
        // than being cut off by it.
        self.amp.kill(0.003);
        self.fenv.kill(0.003);
    }

    fn next(&mut self) -> f32 {
        use crate::dsp::Processor;
        let osc = self.osc1.tick() * (1.0 - self.osc2_mix) + self.osc2.tick() * self.osc2_mix;
        let track = (self.pitch - 60.0) / 12.0 * self.key_track;
        let octaves =
            track + self.fenv.next_level() * self.env_amount * self.velocity + self.timbre * 3.0;
        self.filter
            .set(self.cutoff * 2f32.powf(octaves), self.resonance);
        self.filter.process(osc) * self.amp.next_level() * 0.5
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A probe voice that reports what it was told.
    #[derive(Debug, Default)]
    struct Probe {
        on: bool,
        pitch: f32,
        vel: f32,
        tail: u32,
        killed: bool,
        expr: Expression,
    }

    impl Voice for Probe {
        fn note_on(&mut self, pitch: f32, velocity: f32) {
            self.on = true;
            self.pitch = pitch;
            self.vel = velocity;
            self.tail = 10;
            self.killed = false;
        }
        fn note_off(&mut self) {
            self.on = false;
        }
        fn set_pitch(&mut self, pitch: f32) {
            self.pitch = pitch;
        }
        fn set_expression(&mut self, e: Expression) {
            self.expr = e;
        }
        fn is_active(&self) -> bool {
            self.on || self.tail > 0
        }
        fn next(&mut self) -> f32 {
            if !self.on && self.tail > 0 {
                self.tail -= 1;
            }
            if self.is_active() {
                0.1
            } else {
                0.0
            }
        }
        fn level(&self) -> f32 {
            if self.on {
                self.vel
            } else {
                0.01
            }
        }
        fn kill(&mut self) {
            self.on = false;
            self.tail = 0;
            self.killed = true;
        }
    }

    fn pool(n: usize) -> Poly<Probe> {
        Poly::new((0..n).map(|_| Probe::default()).collect(), 48_000.0)
    }

    #[test]
    fn chords_get_one_voice_per_key() {
        let mut p = pool(8);
        for k in [60, 64, 67] {
            p.note_on(0, k, 1.0);
        }
        assert_eq!(p.active(), 3);
        assert_eq!(p.held_keys(), vec![60, 64, 67]);
        p.note_off(0, 64);
        assert_eq!(p.held_keys(), vec![60, 67]);
    }

    #[test]
    fn oldest_is_stolen_first() {
        let mut p = pool(2);
        p.note_on(0, 60, 1.0);
        p.note_on(0, 62, 1.0);
        p.note_on(0, 64, 1.0);
        assert_eq!(p.held_keys(), vec![62, 64]);
    }

    #[test]
    fn released_voices_are_stolen_before_held_ones() {
        let mut p = pool(2);
        p.note_on(0, 60, 1.0);
        p.note_on(0, 62, 1.0);
        p.note_off(0, 62); // released, still tailing
        p.note_on(0, 64, 1.0);
        assert_eq!(p.held_keys(), vec![60, 64]);
    }

    #[test]
    fn steal_policies() {
        let mut p = pool(2);
        p.steal = Steal::Lowest;
        p.note_on(0, 70, 1.0);
        p.note_on(0, 50, 1.0);
        p.note_on(0, 60, 1.0);
        assert_eq!(p.held_keys(), vec![60, 70]);
        let mut p = pool(2);
        p.steal = Steal::Quietest;
        p.note_on(0, 70, 1.0);
        p.note_on(0, 50, 0.2);
        p.note_on(0, 60, 1.0);
        assert_eq!(p.held_keys(), vec![60, 70]);
        let mut p = pool(1);
        p.steal = Steal::None;
        p.note_on(0, 70, 1.0);
        p.note_on(0, 60, 1.0);
        assert_eq!(p.held_keys(), vec![70]);
    }

    #[test]
    fn stealing_fades_the_voice_out_instead_of_cutting_it() {
        let rms = |x: &[f32]| crate::synth::testing::rms(x);
        let mut v = SubtractiveVoice::new(48_000.0);
        v.note_on(69.0, 1.0);
        let pre: Vec<f32> = (0..48_000).map(|_| v.next()).collect(); // 1 s
        let loud = rms(&pre[24_000..]);
        assert!(loud > 0.05, "sustaining: {loud}");
        v.kill();
        let post: Vec<f32> = (0..9_600).map(|_| v.next()).collect(); // 200 ms
                                                                     // The reset this replaced cut the voice dead at the kill — the
                                                                     // first 10 ms after it would be pure silence. A fade still sounds,
                                                                     // decaying on the kill's own 3 ms clock.
        let ten = |a: usize| rms(&post[a..a + 480]);
        assert!(
            ten(0) > loud * 0.25,
            "fading, not cut: {} vs {}",
            ten(0),
            loud
        );
        assert!(
            ten(480) < ten(0) * 0.2,
            "exponential-ish decay: {} -> {}",
            ten(0),
            ten(480)
        );
        assert!(rms(&post[4_800..]) < 1e-4, "silent by 100 ms");
        assert!(!v.is_active(), "voice reclaimable");
        // A retrigger into the faded voice attacks from the faded level and
        // reaches full voice — the fade is absorbed by the new note, not
        // left running underneath it (which would mute the retrigger).
        v.note_on(71.0, 1.0);
        let after: Vec<f32> = (0..48_000).map(|_| v.next()).collect();
        assert!(
            rms(&after[24_000..]) > loud * 0.5,
            "the new note rings at full voice"
        );
    }

    #[test]
    fn sustain_pedal_holds_released_notes() {
        let mut p = pool(4);
        p.midi(&Message::ControlChange {
            channel: 0,
            controller: 64,
            value: 127,
        });
        p.note_on(0, 60, 1.0);
        p.note_off(0, 60);
        for _ in 0..100 {
            p.next_stereo();
        }
        assert_eq!(p.active(), 1, "still sounding under the pedal");
        p.pedal(0, false);
        for _ in 0..100 {
            p.next_stereo();
        }
        assert_eq!(p.active(), 0);
    }

    #[test]
    fn unison_stacks_detuned_spread_voices() {
        let mut p = pool(8);
        p.unison = Unison {
            voices: 4,
            detune_cents: 30.0,
            spread: 1.0,
        };
        p.note_on(0, 60, 1.0);
        assert_eq!(p.active(), 4);
        p.next_stereo();
        let mut pitches: Vec<f32> = p
            .slots
            .iter()
            .filter(|s| s.voice.on)
            .map(|s| s.voice.pitch)
            .collect();
        pitches.sort_by(f32::total_cmp);
        assert!(
            (pitches[0] - 59.85).abs() < 1e-4 && (pitches[3] - 60.15).abs() < 1e-4,
            "{pitches:?}"
        );
        let pans: Vec<f32> = p
            .slots
            .iter()
            .filter(|s| s.voice.on)
            .map(|s| s.pan)
            .collect();
        assert!(pans.contains(&-1.0) && pans.contains(&1.0));
    }

    #[test]
    fn legato_glides_without_retrigger_and_returns_to_held_key() {
        let mut p = pool(2);
        p.mode = Mode::Legato;
        p.glide = 0.01;
        p.note_on(0, 60, 1.0);
        let vel_before = p.slots[0].voice.vel;
        p.slots[0].voice.vel = 0.123; // mark: a retrigger would overwrite
        p.note_on(0, 72, 1.0);
        assert!((p.slots[0].voice.vel - 0.123).abs() < 1e-6, "no retrigger");
        let _ = vel_before;
        for _ in 0..10 {
            p.next_stereo();
        }
        let mid = p.slots[0].voice.pitch;
        assert!(mid > 60.0 && mid < 72.0, "gliding: {mid}");
        for _ in 0..48_000 {
            p.next_stereo();
        }
        assert!(
            (p.slots[0].voice.pitch - 72.0).abs() < 1e-3,
            "{}",
            p.slots[0].voice.pitch
        );
        p.note_off(0, 72);
        for _ in 0..48_000 {
            p.next_stereo();
        }
        assert!(
            (p.slots[0].voice.pitch - 60.0).abs() < 1e-3,
            "back to the held key"
        );
        assert_eq!(p.active(), 1);
    }

    #[test]
    fn mono_retriggers() {
        let mut p = pool(2);
        p.mode = Mode::Mono;
        p.note_on(0, 60, 1.0);
        p.slots[0].voice.vel = 0.123;
        p.note_on(0, 62, 0.9);
        assert!((p.slots[0].voice.vel - 0.9).abs() < 1e-6);
        assert_eq!(p.active(), 1);
    }

    #[test]
    fn mpe_bends_one_note_not_its_neighbour() {
        let mut p = pool(4);
        p.mpe = true;
        p.bend_range = 48.0;
        p.note_on(1, 60, 1.0);
        p.note_on(2, 64, 1.0);
        p.midi(&Message::PitchBend {
            channel: 1,
            value: 4096,
        }); // +24 semitones on channel 1 only
        p.midi(&Message::ChannelPressure {
            channel: 2,
            pressure: 127,
        });
        p.next_stereo();
        let s1 = p.slots.iter().find(|s| s.channel == 1).unwrap();
        let s2 = p.slots.iter().find(|s| s.channel == 2).unwrap();
        assert!((s1.voice.pitch - 84.0).abs() < 1e-3, "{}", s1.voice.pitch);
        assert!((s2.voice.pitch - 64.0).abs() < 1e-3);
        assert!((s2.voice.expr.pressure - 1.0).abs() < 1e-6 && s1.voice.expr.pressure == 0.0);
        // The master channel's bend moves everything (±2).
        p.midi(&Message::PitchBend {
            channel: 0,
            value: 8191,
        });
        p.next_stereo();
        let s2 = p.slots.iter().find(|s| s.channel == 2).unwrap();
        assert!((s2.voice.pitch - 66.0).abs() < 0.01);
    }

    #[test]
    fn all_notes_off() {
        let mut p = pool(4);
        p.note_on(0, 60, 1.0);
        p.note_on(0, 61, 1.0);
        p.midi(&Message::ControlChange {
            channel: 0,
            controller: 120,
            value: 0,
        });
        assert_eq!(p.active(), 0);
    }

    #[test]
    fn subtractive_voice_plays_a_chord() {
        let mut p = Poly::new(
            (0..6).map(|_| SubtractiveVoice::new(48_000.0)).collect(),
            48_000.0,
        );
        for k in [48, 55, 60, 64] {
            p.note_on(0, k, 0.8);
        }
        let x = p.render(24_000);
        let peak = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.05 && peak < 4.0, "{peak}");
        for k in [48, 55, 60, 64] {
            p.note_off(0, k);
        }
        let tail = p.render(48_000);
        assert!(tail[tail.len() - 200..].iter().all(|v| v.abs() < 1e-3));
        assert_eq!(p.active(), 0);
    }
}
