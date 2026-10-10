//! Sample playback — the multisampled instrument.
//!
//! A [`Zone`] maps a region of the keyboard and of velocity to a recorded
//! sample: which key it was recorded at (so other keys repitch it), where it
//! loops, how loud and where it sits, and which *articulation* and *round-
//! robin* slot it belongs to. An [`Instrument`] is a set of zones; the
//! [`Sampler`] plays one, choosing zones per note the way SFZ and SoundFont
//! players do:
//!
//! * **key and velocity ranges** select the layer;
//! * **round-robin** (`seq_length`/`seq_position`) cycles through
//!   alternative takes so repeated notes do not machine-gun;
//! * **keyswitches** (`sw_lokey`..`sw_hikey`, `sw_last`) switch articulation
//!   from keys outside the playing range;
//! * **release triggers** play a zone when the key comes *up* (a piano's
//!   damper, a guitar's fret noise);
//! * **choke groups** (`group`/`off_by`) let one sound cut another — the
//!   closed hi-hat silencing the open one.
//!
//! Loops are forward or ping-pong, continuous or only while the key is held
//! (`loop_sustain`), with an optional crossfade so a loop point that does
//! not match exactly does not click. Repitching is cubic-interpolated.
//! [`sfz`](super::sfz) and [`sf2`](super::sf2) load instruments from files.

use std::sync::Arc;

use super::envelope::{Adsr, EnvelopeGen};
use super::voice::pitch_to_hz;
use crate::midi::Message;

/// Audio for one zone.
#[derive(Debug, Clone, PartialEq)]
pub struct SampleData {
    /// Interleaved samples.
    pub data: Vec<f32>,
    /// 1 or 2.
    pub channels: usize,
    /// Sample rate.
    pub rate: f32,
}

impl SampleData {
    /// Mono data.
    #[must_use]
    pub const fn mono(data: Vec<f32>, rate: f32) -> Self {
        Self {
            data,
            channels: 1,
            rate,
        }
    }

    /// Frames.
    #[must_use]
    pub fn frames(&self) -> usize {
        self.data.len() / self.channels.max(1)
    }

    fn frame(&self, i: usize, c: usize) -> f32 {
        let ch = self.channels.max(1);
        self.data
            .get(i * ch + c.min(ch - 1))
            .copied()
            .unwrap_or(0.0)
    }
}

/// Loop behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoopMode {
    /// Play once to the end (or until released).
    #[default]
    None,
    /// Play once to the end regardless of note-off (drums).
    OneShot,
    /// Loop forever, through the release.
    Continuous,
    /// Loop while held; on release play on past the loop end.
    Sustain,
}

/// When a zone triggers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Trigger {
    /// On note-on.
    #[default]
    Attack,
    /// On note-off.
    Release,
}

/// One mapped sample.
#[derive(Debug, Clone, PartialEq)]
pub struct Zone {
    /// The audio.
    pub sample: Arc<SampleData>,
    /// Lowest key.
    pub lo_key: u8,
    /// Highest key.
    pub hi_key: u8,
    /// Lowest velocity (1..=127).
    pub lo_vel: u8,
    /// Highest velocity.
    pub hi_vel: u8,
    /// The key the sample sounds at unshifted.
    pub root: f32,
    /// Fine tune in cents.
    pub tune: f32,
    /// How much pitch follows the key (100 = normal, 0 = fixed).
    pub keytrack: f32,
    /// Gain in dB.
    pub volume: f32,
    /// Pan −1..=1.
    pub pan: f32,
    /// Start offset in frames.
    pub offset: usize,
    /// End frame (exclusive; 0 = sample end).
    pub end: usize,
    /// Loop mode.
    pub loop_mode: LoopMode,
    /// Loop start frame.
    pub loop_start: usize,
    /// Loop end frame (exclusive).
    pub loop_end: usize,
    /// Ping-pong instead of forward.
    pub ping_pong: bool,
    /// Loop crossfade in frames.
    pub crossfade: usize,
    /// Round-robin cycle length (1 = off).
    pub seq_length: u32,
    /// This zone's slot in the cycle (1-based).
    pub seq_position: u32,
    /// Keyswitch this zone needs (the last keyswitch pressed must equal it).
    pub keyswitch: Option<u8>,
    /// Attack or release trigger.
    pub trigger: Trigger,
    /// Choke group this zone belongs to (0 = none).
    pub group: u32,
    /// Starting this zone silences voices in this group (0 = none).
    pub off_by: u32,
    /// Amplitude envelope.
    pub amp: Adsr,
    /// Velocity → amplitude amount (0..=1).
    pub vel_track: f32,
}

impl Zone {
    /// A zone over the whole keyboard and velocity range, rooted at `root`.
    #[must_use]
    pub fn new(sample: Arc<SampleData>, root: f32) -> Self {
        Self {
            sample,
            lo_key: 0,
            hi_key: 127,
            lo_vel: 1,
            hi_vel: 127,
            root,
            tune: 0.0,
            keytrack: 100.0,
            volume: 0.0,
            pan: 0.0,
            offset: 0,
            end: 0,
            loop_mode: LoopMode::None,
            loop_start: 0,
            loop_end: 0,
            ping_pong: false,
            crossfade: 0,
            seq_length: 1,
            seq_position: 1,
            keyswitch: None,
            trigger: Trigger::Attack,
            group: 0,
            off_by: 0,
            amp: Adsr::new(0.001, 0.0, 1.0, 0.05),
            vel_track: 1.0,
        }
    }

    /// Restrict to keys.
    #[must_use]
    pub const fn keys(mut self, lo: u8, hi: u8) -> Self {
        self.lo_key = lo;
        self.hi_key = hi;
        self
    }

    /// Restrict to velocities.
    #[must_use]
    pub const fn vels(mut self, lo: u8, hi: u8) -> Self {
        self.lo_vel = lo;
        self.hi_vel = hi;
        self
    }

    /// Set a loop.
    #[must_use]
    pub const fn looped(mut self, mode: LoopMode, start: usize, end: usize) -> Self {
        self.loop_mode = mode;
        self.loop_start = start;
        self.loop_end = end;
        self
    }

    fn matches(&self, key: u8, vel: u8, sw: Option<u8>, trig: Trigger) -> bool {
        self.trigger == trig
            && (self.lo_key..=self.hi_key).contains(&key)
            && (self.lo_vel..=self.hi_vel).contains(&vel)
            && self.keyswitch.is_none_or(|k| sw == Some(k))
    }

    fn end_frame(&self) -> usize {
        let n = self.sample.frames();
        if self.end == 0 || self.end > n {
            n
        } else {
            self.end
        }
    }
}

/// A set of zones plus keyswitch configuration.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Instrument {
    /// The zones.
    pub zones: Vec<Zone>,
    /// Keys that act as keyswitches (and do not sound).
    pub keyswitch_range: Option<(u8, u8)>,
    /// The keyswitch active before any is pressed.
    pub default_keyswitch: Option<u8>,
}

#[derive(Debug, Clone)]
struct SVoice {
    zone: usize,
    key: u8,
    channel: u8,
    pos: f64,
    inc: f64,
    forward: bool,
    held: bool,
    sustained: bool,
    env: EnvelopeGen,
    gain: f32,
    gl: f32,
    gr: f32,
    active: bool,
    age: u64,
    bend: f32,
}

/// Plays an [`Instrument`].
#[derive(Debug)]
pub struct Sampler {
    /// The instrument.
    pub instrument: Instrument,
    voices: Vec<SVoice>,
    rate: f32,
    keyswitch: Option<u8>,
    rr: std::collections::HashMap<(u8, u32), u32>,
    note_vel: [[u8; 128]; 16],
    clock: u64,
    pedal: [bool; 16],
    bend: [f32; 16],
    /// Pitch-bend range in semitones.
    pub bend_range: f32,
}

impl Sampler {
    /// A sampler with `polyphony` voices at `rate`.
    #[must_use]
    pub fn new(instrument: Instrument, polyphony: usize, rate: f32) -> Self {
        let ks = instrument.default_keyswitch;
        let proto = SVoice {
            zone: 0,
            key: 0,
            channel: 0,
            pos: 0.0,
            inc: 0.0,
            forward: true,
            held: false,
            sustained: false,
            env: EnvelopeGen::new(Adsr::new(0.0, 0.0, 1.0, 0.0).stages(), rate),
            gain: 0.0,
            gl: 0.0,
            gr: 0.0,
            active: false,
            age: 0,
            bend: 0.0,
        };
        Self {
            instrument,
            voices: vec![proto; polyphony.max(1)],
            rate,
            keyswitch: ks,
            rr: std::collections::HashMap::new(),
            note_vel: [[0; 128]; 16],
            clock: 0,
            pedal: [false; 16],
            bend: [0.0; 16],
            bend_range: 2.0,
        }
    }

    /// Voices sounding.
    #[must_use]
    pub fn active(&self) -> usize {
        self.voices.iter().filter(|v| v.active).count()
    }

    /// The current keyswitch.
    #[must_use]
    pub const fn keyswitch(&self) -> Option<u8> {
        self.keyswitch
    }

    /// Feed MIDI.
    pub fn midi(&mut self, m: &Message) {
        match m.clone().normalized() {
            Message::NoteOn {
                channel,
                key,
                velocity,
            } => self.note_on(channel, key, velocity),
            Message::NoteOff { channel, key, .. } => self.note_off(channel, key),
            Message::ControlChange {
                channel,
                controller: 64,
                value,
            } => {
                let down = value >= 64;
                self.pedal[usize::from(channel & 15)] = down;
                if !down {
                    for v in &mut self.voices {
                        if v.sustained && v.channel == channel {
                            v.sustained = false;
                            v.held = false;
                            v.env.gate_off();
                        }
                    }
                }
            }
            Message::PitchBend { channel, value } => {
                self.bend[usize::from(channel & 15)] = f32::from(value) / 8192.0 * self.bend_range;
            }
            _ => {}
        }
    }

    fn start_zone(&mut self, zi: usize, channel: u8, key: u8, vel: u8) {
        let z = &self.instrument.zones[zi];
        if z.off_by != 0 {
            let group = z.off_by;
            let zones = &self.instrument.zones;
            for v in self.voices.iter_mut().filter(|v| v.active) {
                if zones[v.zone].group == group {
                    v.env.gate_off();
                    v.held = false;
                }
            }
        }
        let z = &self.instrument.zones[zi];
        let slot = self
            .voices
            .iter()
            .position(|v| !v.active)
            .or_else(|| {
                // Steal the oldest released voice, else the oldest.
                self.voices
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| !v.held)
                    .min_by_key(|(_, v)| v.age)
                    .or_else(|| self.voices.iter().enumerate().min_by_key(|(_, v)| v.age))
                    .map(|(i, _)| i)
            })
            .unwrap_or(0);
        self.clock += 1;
        let semis = (f32::from(key) - z.root) * z.keytrack / 100.0 + z.tune / 100.0;
        let inc =
            f64::from(2f32.powf(semis / 12.0)) * f64::from(z.sample.rate) / f64::from(self.rate);
        let v01 = f32::from(vel) / 127.0;
        let vel_gain = 1.0 - z.vel_track * (1.0 - v01 * v01);
        let (gl, gr) = super::equal_power(z.pan);
        let mut env = EnvelopeGen::new(z.amp.stages(), self.rate);
        env.gate_on();
        #[expect(clippy::cast_precision_loss, reason = "frame offsets")]
        let pos = z.offset as f64;
        self.voices[slot] = SVoice {
            zone: zi,
            key,
            channel,
            pos,
            inc,
            forward: true,
            held: z.trigger == Trigger::Attack,
            sustained: false,
            env,
            gain: 10f32.powf(z.volume / 20.0) * vel_gain,
            gl,
            gr,
            active: true,
            age: self.clock,
            bend: 0.0,
        };
    }

    fn select(&mut self, key: u8, vel: u8, trig: Trigger) -> Vec<usize> {
        let sw = self.keyswitch;
        let candidates: Vec<usize> = self
            .instrument
            .zones
            .iter()
            .enumerate()
            .filter(|(_, z)| z.matches(key, vel, sw, trig))
            .map(|(i, _)| i)
            .collect();
        // Round robin: per (key, cycle length), pick zones whose position
        // matches the counter.
        let mut out = Vec::new();
        let mut advanced = std::collections::HashSet::new();
        for &i in &candidates {
            let z = &self.instrument.zones[i];
            if z.seq_length <= 1 {
                out.push(i);
                continue;
            }
            let ctr = *self.rr.get(&(key, z.seq_length)).unwrap_or(&0);
            if ctr % z.seq_length + 1 == z.seq_position {
                out.push(i);
            }
            advanced.insert(z.seq_length);
        }
        for len in advanced {
            *self.rr.entry((key, len)).or_insert(0) += 1;
        }
        out
    }

    /// Key down.
    pub fn note_on(&mut self, channel: u8, key: u8, vel: u8) {
        if vel == 0 {
            self.note_off(channel, key);
            return;
        }
        if let Some((lo, hi)) = self.instrument.keyswitch_range {
            if (lo..=hi).contains(&key) {
                self.keyswitch = Some(key);
                return;
            }
        }
        self.note_vel[usize::from(channel & 15)][usize::from(key & 127)] = vel;
        for zi in self.select(key, vel, Trigger::Attack) {
            self.start_zone(zi, channel, key, vel);
        }
    }

    /// Key up.
    pub fn note_off(&mut self, channel: u8, key: u8) {
        if let Some((lo, hi)) = self.instrument.keyswitch_range {
            if (lo..=hi).contains(&key) {
                return;
            }
        }
        let ped = self.pedal[usize::from(channel & 15)];
        let zones = &self.instrument.zones;
        for v in &mut self.voices {
            if v.active && v.held && v.key == key && v.channel == channel {
                if ped {
                    v.sustained = true;
                } else {
                    v.held = false;
                    if zones[v.zone].loop_mode != LoopMode::OneShot {
                        v.env.gate_off();
                    }
                }
            }
        }
        let vel = self.note_vel[usize::from(channel & 15)][usize::from(key & 127)].max(1);
        for zi in self.select(key, vel, Trigger::Release) {
            self.start_zone(zi, channel, key, vel);
        }
    }

    /// The next stereo frame.
    pub fn next_stereo(&mut self) -> (f32, f32) {
        let mut l = 0.0;
        let mut r = 0.0;
        let zones = &self.instrument.zones;
        for v in &mut self.voices {
            if !v.active {
                continue;
            }
            let z = &zones[v.zone];
            let bend = self.bend[usize::from(v.channel & 15)];
            if (bend - v.bend).abs() > 1e-6 {
                v.bend = bend;
            }
            let inc = v.inc * f64::from(2f32.powf(v.bend / 12.0));
            let e = v.env.next_level();
            let (sl, sr) = read(z, v.pos);
            let (sl, sr) = crossfaded(z, v, sl, sr);
            l += sl * e * v.gain * v.gl * std::f32::consts::SQRT_2;
            r += sr * e * v.gain * v.gr * std::f32::consts::SQRT_2;
            advance(z, v, inc);
            if !v.env.is_active() {
                v.active = false;
            }
        }
        (l, r)
    }

    /// Render interleaved stereo.
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

/// Cubic read of both channels at a fractional frame.
fn read(z: &Zone, pos: f64) -> (f32, f32) {
    let s = &z.sample;
    let n = s.frames();
    if n == 0 || pos < 0.0 {
        return (0.0, 0.0);
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "non-negative"
    )]
    let i = pos.floor() as usize;
    if i >= n {
        return (0.0, 0.0);
    }
    #[expect(clippy::cast_possible_truncation, reason = "fraction")]
    let f = (pos - pos.floor()) as f32;
    let get = |k: isize, c: usize| -> f32 {
        let idx = i.wrapping_add_signed(k);
        if idx < n {
            s.frame(idx, c)
        } else {
            0.0
        }
    };
    let cubic = |c: usize| {
        let (y0, y1, y2, y3) = (get(-1, c), get(0, c), get(1, c), get(2, c));
        let c1 = 0.5 * (y2 - y0);
        let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
        let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
        ((c3 * f + c2) * f + c1) * f + y1
    };
    if s.channels >= 2 {
        (cubic(0), cubic(1))
    } else {
        let m = cubic(0);
        (m, m)
    }
}

fn looping(z: &Zone, v: &SVoice) -> bool {
    let mode_loops = match z.loop_mode {
        LoopMode::Continuous => true,
        LoopMode::Sustain => v.held || v.sustained,
        _ => false,
    };
    mode_loops && z.loop_end > z.loop_start + 1
}

/// Forward-loop crossfade: as the read head approaches the loop end, blend
/// in the material just before the loop start so the jump is seamless.
fn crossfaded(z: &Zone, v: &SVoice, l: f32, r: f32) -> (f32, f32) {
    if z.crossfade == 0 || z.ping_pong || !looping(z, v) {
        return (l, r);
    }
    #[expect(clippy::cast_precision_loss, reason = "frames")]
    let (ls, le, xf) = (
        z.loop_start as f64,
        z.loop_end as f64,
        z.crossfade.min(z.loop_start) as f64,
    );
    if xf <= 0.0 || v.pos < le - xf {
        return (l, r);
    }
    #[expect(clippy::cast_possible_truncation, reason = "fade weight")]
    let w = ((v.pos - (le - xf)) / xf) as f32;
    let (al, ar) = read(z, v.pos - (le - ls));
    (l * (1.0 - w) + al * w, r * (1.0 - w) + ar * w)
}

fn advance(z: &Zone, v: &mut SVoice, inc: f64) {
    let lp = looping(z, v);
    #[expect(clippy::cast_precision_loss, reason = "frames")]
    let (ls, le, end) = (z.loop_start as f64, z.loop_end as f64, z.end_frame() as f64);
    if v.forward {
        v.pos += inc;
    } else {
        v.pos -= inc;
    }
    if lp {
        if z.ping_pong {
            if v.forward && v.pos >= le - 1.0 {
                v.pos = 2.0 * (le - 1.0) - v.pos;
                v.forward = false;
            } else if !v.forward && v.pos <= ls {
                v.pos = 2.0 * ls - v.pos;
                v.forward = true;
            }
        } else if v.pos >= le {
            v.pos -= le - ls;
        }
    } else {
        v.forward = true;
        if v.pos >= end {
            v.active = false;
        }
    }
}

/// The frequency a zone sounds for `key` — for players that display it.
#[must_use]
pub fn zone_hz(z: &Zone, key: u8) -> f32 {
    pitch_to_hz(z.root + (f32::from(key) - z.root) * z.keytrack / 100.0 + z.tune / 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::testing::{level_near, spectrum};

    const RATE: f32 = 48_000.0;

    fn sine(hz: f32, frames: usize) -> Arc<SampleData> {
        Arc::new(SampleData::mono(
            (0..frames)
                .map(|i| {
                    #[expect(clippy::cast_precision_loss, reason = "test")]
                    let t = i as f32 / RATE;
                    (std::f32::consts::TAU * hz * t).sin()
                })
                .collect(),
            RATE,
        ))
    }

    fn mono(s: &mut Sampler, n: usize) -> Vec<f32> {
        s.render(n).chunks_exact(2).map(|f| f[0]).collect()
    }

    #[test]
    fn repitches_from_the_root() {
        let inst = Instrument {
            zones: vec![Zone::new(sine(375.0, 48_000), 60.0)],
            ..Instrument::default()
        };
        let mut s = Sampler::new(inst, 8, RATE);
        s.note_on(0, 72, 127);
        let x = mono(&mut s, 16_384);
        let (m, bin) = spectrum(&x, RATE);
        assert!(level_near(&m, bin, 750.0, 2) > 0.3, "an octave up");
        assert!(level_near(&m, bin, 375.0, 2) < 0.01);
    }

    #[test]
    fn key_and_velocity_ranges_pick_layers() {
        let soft = Zone::new(sine(375.0, 9_600), 60.0).vels(1, 63);
        let loud = Zone::new(sine(750.0, 9_600), 60.0).vels(64, 127);
        let high = Zone::new(sine(1125.0, 9_600), 72.0).keys(72, 127);
        let low_soft = soft.keys(0, 71);
        let low_loud = loud.keys(0, 71);
        let inst = Instrument {
            zones: vec![low_soft, low_loud, high],
            ..Instrument::default()
        };
        let probe = |key, vel| {
            let mut s = Sampler::new(inst.clone(), 4, RATE);
            s.note_on(0, key, vel);
            let x = mono(&mut s, 8_192);
            let (m, bin) = spectrum(&x, RATE);
            [375.0, 750.0, 1125.0].map(|hz| level_near(&m, bin, hz, 2))
        };
        // Velocity 30 with full velocity tracking is quiet by design; what
        // matters is which layer sounds.
        let a = probe(60, 30);
        assert!(a[0] > 0.01 && a[1] < a[0] * 0.01, "{a:?}");
        let b = probe(60, 100);
        assert!(b[1] > 0.1 && b[0] < 0.01);
        let c = probe(72, 100);
        assert!(c[2] > 0.1 && c[0] < 0.01 && c[1] < 0.01);
    }

    #[test]
    fn round_robin_cycles_takes() {
        let mk = |hz, pos| Zone {
            seq_length: 3,
            seq_position: pos,
            ..Zone::new(sine(hz, 4_800), 60.0)
        };
        let inst = Instrument {
            zones: vec![mk(375.0, 1), mk(750.0, 2), mk(1125.0, 3)],
            ..Instrument::default()
        };
        let mut s = Sampler::new(inst, 8, RATE);
        let mut picked = Vec::new();
        for _ in 0..6 {
            s.note_on(0, 60, 100);
            let x = mono(&mut s, 4_096);
            s.note_off(0, 60);
            mono(&mut s, 4_800);
            let (m, bin) = spectrum(&x, RATE);
            let lv = [375.0, 750.0, 1125.0].map(|hz| level_near(&m, bin, hz, 3));
            let best = (0..3).max_by(|a, b| lv[*a].total_cmp(&lv[*b])).unwrap();
            picked.push(best);
        }
        assert_eq!(picked, vec![0, 1, 2, 0, 1, 2]);
    }

    #[test]
    fn keyswitches_choose_articulation_and_are_silent() {
        let legato = Zone {
            keyswitch: Some(24),
            ..Zone::new(sine(375.0, 9_600), 60.0)
        };
        let stacc = Zone {
            keyswitch: Some(25),
            ..Zone::new(sine(750.0, 9_600), 60.0)
        };
        let inst = Instrument {
            zones: vec![legato, stacc],
            keyswitch_range: Some((24, 25)),
            default_keyswitch: Some(24),
        };
        let mut s = Sampler::new(inst, 4, RATE);
        s.note_on(0, 25, 100);
        assert_eq!(s.active(), 0, "keyswitch keys do not sound");
        assert_eq!(s.keyswitch(), Some(25));
        s.note_on(0, 60, 100);
        let x = mono(&mut s, 8_192);
        let (m, bin) = spectrum(&x, RATE);
        assert!(level_near(&m, bin, 750.0, 2) > 0.1 && level_near(&m, bin, 375.0, 2) < 0.01);
    }

    #[test]
    fn sustain_loop_holds_then_plays_out() {
        let z = Zone::new(sine(375.0, 4_800), 60.0).looped(LoopMode::Sustain, 1_280, 3_840);
        let z = Zone {
            amp: Adsr::new(0.001, 0.0, 1.0, 2.0),
            ..z
        };
        let inst = Instrument {
            zones: vec![z],
            ..Instrument::default()
        };
        let mut s = Sampler::new(inst, 2, RATE);
        s.note_on(0, 60, 127);
        let held = mono(&mut s, 48_000);
        assert!(
            held[40_000..].iter().any(|v| v.abs() > 0.5),
            "still looping after 1 s"
        );
        s.note_off(0, 60);
        mono(&mut s, 4_800);
        assert_eq!(s.active(), 0, "played past the loop end and stopped");
    }

    #[test]
    fn crossfaded_loop_has_no_click() {
        // A loop whose end does not match its start (sine at a non-integer
        // number of cycles): with a crossfade the seam is smooth.
        let z = Zone {
            crossfade: 480,
            ..Zone::new(sine(377.0, 9_600), 60.0).looped(LoopMode::Continuous, 2_000, 6_000)
        };
        let inst = Instrument {
            zones: vec![z.clone()],
            ..Instrument::default()
        };
        let worst_step = |zone: Zone| {
            let mut s = Sampler::new(
                Instrument {
                    zones: vec![zone],
                    ..Instrument::default()
                },
                2,
                RATE,
            );
            s.note_on(0, 60, 127);
            let x = mono(&mut s, 24_000);
            x[100..]
                .windows(2)
                .map(|w| (w[1] - w[0]).abs())
                .fold(0.0f32, f32::max)
        };
        let smooth = worst_step(inst.zones[0].clone());
        let rough = worst_step(Zone { crossfade: 0, ..z });
        let natural = std::f32::consts::TAU * 377.0 / RATE * 1.42;
        assert!(smooth < natural * 1.1, "{smooth} vs {natural}");
        assert!(rough > smooth * 2.0, "{rough}");
    }

    #[test]
    fn ping_pong_reverses_at_the_ends() {
        let ramp = Arc::new(SampleData::mono(
            (0..1_000).map(|i| i as f32 / 1_000.0).collect(),
            RATE,
        ));
        let z = Zone {
            ping_pong: true,
            ..Zone::new(ramp, 60.0).looped(LoopMode::Continuous, 200, 400)
        };
        let mut s = Sampler::new(
            Instrument {
                zones: vec![z],
                ..Instrument::default()
            },
            1,
            RATE,
        );
        s.note_on(0, 60, 127);
        let x = mono(&mut s, 2_000);
        let tail = &x[600..];
        let hi = tail.iter().fold(0.0f32, |m, v| m.max(*v));
        let lo = tail.iter().fold(1.0f32, |m, v| m.min(*v));
        assert!(
            hi < 0.42 && lo > 0.18,
            "bounces between the loop points: {lo}..{hi}"
        );
    }

    #[test]
    fn release_triggers_and_choke_groups() {
        let open_hat = Zone {
            group: 1,
            ..Zone::new(sine(375.0, 48_000), 42.0).keys(46, 46)
        };
        let closed_hat = Zone {
            off_by: 1,
            ..Zone::new(sine(750.0, 4_800), 42.0).keys(42, 42)
        };
        let rel = Zone {
            trigger: Trigger::Release,
            ..Zone::new(sine(1125.0, 4_800), 60.0).keys(60, 60)
        };
        let body = Zone::new(sine(375.0, 48_000), 60.0).keys(60, 60);
        let mut s = Sampler::new(
            Instrument {
                zones: vec![open_hat, closed_hat, rel, body],
                ..Instrument::default()
            },
            8,
            RATE,
        );
        s.note_on(9, 46, 100);
        mono(&mut s, 1_000);
        s.note_on(9, 42, 100);
        mono(&mut s, 4_800);
        assert_eq!(s.active(), 0, "closed hat choked the open one and ended");
        s.note_on(0, 60, 100);
        mono(&mut s, 1_000);
        assert_eq!(s.active(), 1);
        s.note_off(0, 60);
        assert_eq!(s.active(), 2, "release sample started");
    }
}
