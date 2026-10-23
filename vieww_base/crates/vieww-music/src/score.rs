//! The score: where every tradition meets.
//!
//! A [`Score`] is parts of timed [`Event`]s — onset and length in beats, a
//! fractional pitch number, a velocity and an optional [`Gamaka`] — plus a
//! tempo and a [`ScoreTuning`]. A kriti and a rock song compile to the same
//! structure; the score owns the tuning, so re-tuning a whole piece is one
//! field, and no event changes.
//!
//! From here a score goes to sound ([`crate::render`]), to the engine's
//! sequencer ([`Score::to_clips`]) or to a Standard MIDI File
//! ([`Score::to_smf`]), where fractional pitches and gamakas travel as
//! MPE pitch bend on a channel per sounding note.

use vieww_audio::midi::{Message, Smf, TrackEvent};
use vieww_audio::sequencer::{Clip, Note};
use vieww_audio::synth::tuning::{EqualTemperament, ScaleTuning, Tuning, TuningTable};

use crate::carnatic::gamaka::Gamaka;

/// One sounding note.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// Onset, beats from the start of the score.
    pub start: f64,
    /// Length in beats.
    pub length: f64,
    /// Pitch number (60 = middle C under 12-TET; fractional allowed).
    pub pitch: f32,
    /// Loudness 0..=1.
    pub velocity: f32,
    /// Pitch movement within the note.
    pub gamaka: Gamaka,
}

impl Event {
    /// A plain note.
    #[must_use]
    pub const fn new(start: f64, length: f64, pitch: f32, velocity: f32) -> Self {
        Self {
            start,
            length,
            pitch,
            velocity,
            gamaka: Gamaka::Plain,
        }
    }

    /// The same note with a gamaka.
    #[must_use]
    pub fn with_gamaka(mut self, gamaka: Gamaka) -> Self {
        self.gamaka = gamaka;
        self
    }

    /// End, in beats.
    #[must_use]
    pub fn end(&self) -> f64 {
        self.start + self.length
    }
}

/// What a part is played on (see [`crate::render`] for the sounds).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Instrument {
    /// A plucked string (Karplus–Strong): guitar, veena-like melody.
    Pluck,
    /// A bowed string: sustained melody with expression.
    Bowed,
    /// A clarinet-like reed.
    Reed,
    /// A struck bar (marimba).
    Mallet,
    /// A two-oscillator subtractive synth, warm and sustained.
    Pad,
    /// A subtractive bass.
    Bass,
    /// A distorted lead (subtractive through saturation).
    Lead,
    /// A synthesised drum kit on General MIDI keys (36 kick, 38 snare, 42
    /// closed hat, 46 open hat, 49 crash).
    Drums,
    /// The tambura drone: long-ringing, buzzing plucked strings.
    Tambura,
    /// Mridangam strokes (see [`crate::render::Stroke`]).
    Mridangam,
}

/// A part: one instrument's line.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    /// Its name.
    pub name: String,
    /// What plays it.
    pub instrument: Instrument,
    /// The notes, in any order.
    pub events: Vec<Event>,
    /// Level in dB.
    pub gain_db: f32,
    /// Pan, −1 (left) to 1 (right).
    pub pan: f32,
    /// Reverb send in dB (−∞ = dry; −12 is a gentle room).
    pub reverb_db: f32,
}

impl Part {
    /// An empty part.
    #[must_use]
    pub fn new(name: &str, instrument: Instrument) -> Self {
        Self {
            name: name.to_owned(),
            instrument,
            events: Vec::new(),
            gain_db: 0.0,
            pan: 0.0,
            reverb_db: -14.0,
        }
    }

    /// Add a note.
    pub fn push(&mut self, e: Event) -> &mut Self {
        self.events.push(e);
        self
    }

    /// When the last note ends, in beats.
    #[must_use]
    pub fn end(&self) -> f64 {
        self.events.iter().map(Event::end).fold(0.0, f64::max)
    }
}

/// The tuning a score plays in.
#[derive(Debug, Clone, PartialEq)]
pub enum ScoreTuning {
    /// Equal temperament (the default: 12 steps, A4 = 440).
    Equal(EqualTemperament),
    /// A repeating scale of degrees in cents (just, raga, Scala …).
    Scale(ScaleTuning),
}

impl Default for ScoreTuning {
    fn default() -> Self {
        Self::Equal(EqualTemperament::default())
    }
}

impl ScoreTuning {
    /// The tuning as a realtime-safe table for the engine.
    #[must_use]
    pub fn table(&self) -> TuningTable {
        match self {
            Self::Equal(t) => TuningTable::from_tuning(t),
            Self::Scale(t) => TuningTable::from_tuning(t),
        }
    }

    /// The frequency pitch number `p` sounds at.
    #[must_use]
    pub fn hz(&self, p: f32) -> f32 {
        match self {
            Self::Equal(t) => t.hz(p),
            Self::Scale(t) => t.hz(p),
        }
    }
}

/// A piece: tempo, tuning and parts.
#[derive(Debug, Clone, PartialEq)]
pub struct Score {
    /// Its title.
    pub title: String,
    /// Tempo in beats per minute.
    pub bpm: f64,
    /// The tuning every pitched part plays in (drum parts ignore it).
    pub tuning: ScoreTuning,
    /// The parts.
    pub parts: Vec<Part>,
    /// Metadata written into rendered files besides the title (which is
    /// always written): `GENRE`, `RAGA`, `TALA`, `COMPOSER`, anything.
    pub tags: vieww_audio::codec::tags::Tags,
}

/// The MPE member channels: 1–15 (channel 0 is the zone's master).
const MPE_CHANNELS: u8 = 15;

impl Score {
    /// An empty score at `bpm` in 12-TET.
    #[must_use]
    pub fn new(title: &str, bpm: f64) -> Self {
        Self {
            title: title.to_owned(),
            bpm,
            tuning: ScoreTuning::default(),
            parts: Vec::new(),
            tags: vieww_audio::codec::tags::Tags::new(),
        }
    }

    /// Add a part, returning it for filling.
    pub fn part(&mut self, name: &str, instrument: Instrument) -> &mut Part {
        self.parts.push(Part::new(name, instrument));
        self.parts.last_mut().expect("just pushed")
    }

    /// Seconds per beat.
    #[must_use]
    pub fn seconds_per_beat(&self) -> f64 {
        60.0 / self.bpm.max(1e-6)
    }

    /// Length in beats (to the end of the last note).
    #[must_use]
    pub fn beats(&self) -> f64 {
        self.parts.iter().map(Part::end).fold(0.0, f64::max)
    }

    /// Length in seconds.
    #[must_use]
    pub fn seconds(&self) -> f64 {
        self.beats() * self.seconds_per_beat()
    }

    /// Each part as an engine [`Clip`]: notes on MPE member channels, a
    /// channel per overlapping note (round-robin, so a channel's bend is
    /// its note's alone); fractional pitch as [`Note::cents`]; gamakas as
    /// bend curves (`curve_points` per note). The score's tuning is *not*
    /// in the clip — give the playing `Poly` the score's
    /// [`ScoreTuning::table`].
    #[must_use]
    pub fn to_clips(&self, curve_points: usize) -> Vec<Clip> {
        let spb = self.seconds_per_beat();
        self.parts
            .iter()
            .map(|part| {
                let mut clip = Clip::new(part.end().max(1.0));
                part.name.clone_into(&mut clip.name);
                let mut events: Vec<&Event> = part.events.iter().collect();
                events.sort_by(|a, b| a.start.total_cmp(&b.start));
                let mut busy_until = [f64::NEG_INFINITY; MPE_CHANNELS as usize];
                let mut next = 0usize;
                for e in events {
                    // The next free channel, round-robin; if all are busy,
                    // the one that frees soonest.
                    let ch = (0..usize::from(MPE_CHANNELS))
                        .map(|k| (next + k) % usize::from(MPE_CHANNELS))
                        .find(|&c| busy_until[c] <= e.start)
                        .unwrap_or_else(|| {
                            (0..usize::from(MPE_CHANNELS))
                                .min_by(|a, b| busy_until[*a].total_cmp(&busy_until[*b]))
                                .unwrap_or(0)
                        });
                    busy_until[ch] = e.end();
                    next = (ch + 1) % usize::from(MPE_CHANNELS);
                    let key = e.pitch.round().clamp(0.0, 127.0);
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "clamped"
                    )]
                    let mut n = Note::new(
                        e.start,
                        e.length,
                        key as u8,
                        (e.velocity.clamp(0.0, 1.0) * 126.0) as u8 + 1,
                    );
                    n.channel = u8::try_from(ch + 1).unwrap_or(1);
                    n.cents = (e.pitch - key) * 100.0;
                    #[expect(clippy::cast_possible_truncation, reason = "seconds in f32")]
                    let secs = (e.length * spb) as f32;
                    n.bend = e.gamaka.bend_points(e.length, secs, curve_points);
                    clip.notes.push(n);
                }
                clip
            })
            .collect()
    }

    /// The score as a type-1 Standard MIDI File (`ppq` ticks per quarter):
    /// a tempo track, then a track per part with note-ons, note-offs and
    /// MPE pitch bends (fractional pitch and gamaka curves sampled every
    /// 1/32 beat, over a ±48-semitone bend range — the receiving synth must
    /// be in MPE mode). The tuning travels as an MTS octave message on the
    /// tempo track when the score's tuning is a 12-step scale.
    #[must_use]
    pub fn to_smf(&self, ppq: u16) -> Smf {
        let mut smf = Smf::new(ppq);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "tempo in µs"
        )]
        let tempo = (60_000_000.0 / self.bpm.max(1e-6)) as u32;
        let mut conductor = vec![
            (0u64, TrackEvent::TrackName(self.title.clone())),
            (0, TrackEvent::Tempo(tempo)),
        ];
        if let Some(m) = self.mts_octave() {
            conductor.push((0, TrackEvent::Midi(m)));
        }
        smf.tracks.push(conductor);
        let tick = |beat: f64| {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "ticks"
            )]
            let t = (beat.max(0.0) * f64::from(ppq)).round() as u64;
            t
        };
        for (part, clip) in self.parts.iter().zip(self.to_clips(16)) {
            let mut track = vec![(0u64, TrackEvent::TrackName(part.name.clone()))];
            for ev in clip.events(0.0, clip.length + 1.0, 0.0) {
                let vieww_audio::sequencer::Event::Midi(m) = ev.event;
                track.push((tick(ev.beat), TrackEvent::Midi(m)));
            }
            // Curves: sample each bending note's expression.
            for n in clip.notes.iter().filter(|n| !n.bend.is_empty()) {
                let steps = (n.length * 32.0).ceil().max(1.0);
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "step count"
                )]
                for i in 1..steps as u32 {
                    let at = n.start + f64::from(i) / 32.0;
                    for m in clip.expression_at(at) {
                        if matches!(m, Message::PitchBend { channel, .. } if channel == n.channel) {
                            track.push((tick(at), TrackEvent::Midi(m)));
                        }
                    }
                }
            }
            track.sort_by_key(|(t, _)| *t);
            smf.tracks.push(track);
        }
        smf
    }

    /// The tuning as an MTS octave message, when it is a 12-degree scale.
    fn mts_octave(&self) -> Option<Message> {
        let ScoreTuning::Scale(s) = &self.tuning else {
            return None;
        };
        if s.cents.len() != 11 || (s.period_cents - 1200.0).abs() > 1e-3 {
            return None;
        }
        let mut from_tonic = [0.0f32; 12];
        from_tonic[1..].copy_from_slice(&s.cents);
        let tonic_class = u8::try_from(s.tonic_pitch.rem_euclid(12)).ok()?;
        Some(vieww_audio::mts::octave_from_scale(
            vieww_audio::mts::ALL_DEVICES,
            0xFFFF,
            tonic_class,
            &from_tonic,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pitch::{chromatic_just, JUST_CHROMATIC};

    fn two_parts() -> Score {
        let mut s = Score::new("test", 120.0);
        let p = s.part("melody", Instrument::Pluck);
        p.push(Event::new(0.0, 1.0, 60.0, 0.8));
        p.push(
            Event::new(0.5, 1.0, 64.14, 0.8).with_gamaka(Gamaka::Kampita {
                depth_cents: 100.0,
                rate_hz: 5.0,
            }),
        );
        s.part("bass", Instrument::Bass)
            .push(Event::new(0.0, 2.0, 36.0, 0.9));
        s
    }

    #[test]
    fn lengths_and_time() {
        let s = two_parts();
        assert!((s.beats() - 2.0).abs() < 1e-12);
        assert!((s.seconds() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn clips_carry_pitch_cents_channels_and_curves() {
        let clips = two_parts().to_clips(8);
        assert_eq!(clips.len(), 2);
        let m = &clips[0];
        assert_eq!(m.notes.len(), 2);
        assert_ne!(
            m.notes[0].channel, m.notes[1].channel,
            "overlapping notes, own channels"
        );
        assert!(m.notes.iter().all(|n| (1..=15).contains(&n.channel)));
        assert_eq!(m.notes[1].key, 64);
        assert!((m.notes[1].cents - 14.0).abs() < 1e-3);
        assert_eq!(m.notes[1].bend.len(), 8, "the kampita curve");
        assert!(m.notes[0].bend.is_empty());
    }

    #[test]
    fn retuning_touches_no_event() {
        let s = two_parts();
        let mut just = s.clone();
        just.tuning = ScoreTuning::Scale(chromatic_just(60, 261.63, &JUST_CHROMATIC));
        assert_eq!(s.parts, just.parts);
        let et = s.tuning.hz(64.0);
        let ji = just.tuning.hz(64.0);
        assert!(
            (1200.0 * (ji / et).log2() + 13.686).abs() < 0.05,
            "a just third"
        );
    }

    #[test]
    fn smf_export_round_trips_through_the_parser() {
        let mut s = two_parts();
        s.tuning = ScoreTuning::Scale(chromatic_just(60, 261.63, &JUST_CHROMATIC));
        let smf = s.to_smf(480);
        assert_eq!(smf.tracks.len(), 3);
        let back = Smf::parse(&smf.to_bytes()).unwrap();
        assert_eq!(back.tracks.len(), 3);
        let notes = back.notes();
        assert_eq!(notes.len(), 3);
        let has_mts = back.tracks[0]
            .iter()
            .any(|(_, e)| matches!(e, TrackEvent::Midi(Message::SysEx(d)) if vieww_audio::mts::parse(d).is_some()));
        assert!(has_mts, "the tuning travels");
        let bends = back.tracks[1]
            .iter()
            .filter(|(_, e)| matches!(e, TrackEvent::Midi(Message::PitchBend { .. })))
            .count();
        assert!(bends > 10, "cents and the kampita as bends: {bends}");
    }
}
