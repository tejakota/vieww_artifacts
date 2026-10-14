//! Sequencing: notes in clips, the edits a piano roll makes, step patterns,
//! a clip launcher, a linear arrangement, and an arpeggiator — everything
//! that turns musical time into note events.
//!
//! All positions are in quarter-note beats; turning beats into sample
//! offsets is the [`transport`](crate::transport) scheduler's job, so every
//! sequencer here produces [`Timed`] events and nothing here knows a sample
//! rate. That keeps the pieces composable: a step pattern, a launched clip
//! and an arranged clip all end up as the same event stream feeding a
//! [`Poly`](crate::synth::voice::Poly) or a [`Sampler`](crate::synth::sampler::Sampler).
//!
//! * [`Clip`] — notes with per-note expression curves (MPE: bend, pressure,
//!   timbre over the note's life), plus the editing operations a piano-roll
//!   UI performs: add, delete, move, resize, transpose, quantise (with
//!   strength and swing), humanise, legato, velocity scaling, and selection
//!   by region.
//! * [`StepSequencer`] — a grid of tracks × steps with velocity,
//!   probability, ratchets, per-track length (polymeter) and swing.
//! * [`Launcher`] — Ableton-style scenes and slots: quantised launches,
//!   stops, follow actions.
//! * [`Arrangement`] — tracks of clips placed on a timeline, with mutes.
//! * [`Arpeggiator`] — up/down/up-down/random/as-played over held notes,
//!   octave range, rate division and gate.

use crate::midi::{ClipNote, Message};
use crate::rng::Rng;
use crate::transport::Timed;

/// A per-note expression curve: `(beat offset into the note, value)`.
pub type Curve = Vec<(f64, f32)>;

/// A note.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    /// Start in beats (relative to the clip).
    pub start: f64,
    /// Length in beats.
    pub length: f64,
    /// MIDI key.
    pub key: u8,
    /// Velocity 1..=127.
    pub velocity: u8,
    /// Channel (MPE: each note may have its own).
    pub channel: u8,
    /// Pitch bend in semitones over the note.
    pub bend: Curve,
    /// Pressure 0..=1 over the note.
    pub pressure: Curve,
    /// Timbre (CC74) 0..=1 over the note.
    pub timbre: Curve,
    /// Selected in the editor.
    pub selected: bool,
}

impl Note {
    /// A plain note.
    #[must_use]
    pub const fn new(start: f64, length: f64, key: u8, velocity: u8) -> Self {
        Self {
            start,
            length,
            key,
            velocity,
            channel: 0,
            bend: Vec::new(),
            pressure: Vec::new(),
            timbre: Vec::new(),
            selected: false,
        }
    }

    /// End in beats.
    #[must_use]
    pub fn end(&self) -> f64 {
        self.start + self.length
    }
}

fn curve_at(c: &Curve, t: f64) -> Option<f32> {
    let first = c.first()?;
    if t <= first.0 {
        return Some(first.1);
    }
    for w in c.windows(2) {
        if t <= w[1].0 {
            #[expect(clippy::cast_possible_truncation, reason = "f32 value")]
            let f = ((t - w[0].0) / (w[1].0 - w[0].0).max(1e-12)) as f32;
            return Some(w[0].1 + (w[1].1 - w[0].1) * f);
        }
    }
    c.last().map(|p| p.1)
}

/// Note events a sequencer emits.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A MIDI message.
    Midi(Message),
}

/// A clip: notes over a length, optionally looped.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Clip {
    /// The notes.
    pub notes: Vec<Note>,
    /// Length in beats.
    pub length: f64,
    /// Loop when played past its length.
    pub looping: bool,
    /// Display name.
    pub name: String,
}

impl Clip {
    /// An empty clip of `length` beats.
    #[must_use]
    pub fn new(length: f64) -> Self {
        Self {
            length,
            ..Self::default()
        }
    }

    fn sort(&mut self) {
        self.notes
            .sort_by(|a, b| a.start.total_cmp(&b.start).then(a.key.cmp(&b.key)));
    }

    /// Add a note (kept sorted); returns its index.
    pub fn add(&mut self, note: Note) -> usize {
        self.notes.push(note);
        self.sort();
        self.notes.len() - 1
    }

    /// Select the notes whose start lies in `beats` and key in `keys`
    /// (a piano roll's rubber band); returns how many.
    pub fn select_region(&mut self, beats: (f64, f64), keys: (u8, u8)) -> usize {
        let mut n = 0;
        for note in &mut self.notes {
            note.selected = note.start >= beats.0
                && note.start < beats.1
                && (keys.0..=keys.1).contains(&note.key);
            n += usize::from(note.selected);
        }
        n
    }

    /// Clear the selection.
    pub fn select_none(&mut self) {
        for n in &mut self.notes {
            n.selected = false;
        }
    }

    fn selected_mut(&mut self) -> impl Iterator<Item = &mut Note> {
        let any = self.notes.iter().any(|n| n.selected);
        self.notes.iter_mut().filter(move |n| !any || n.selected)
    }

    /// Delete the selection (everything if nothing is selected).
    pub fn delete_selected(&mut self) {
        let any = self.notes.iter().any(|n| n.selected);
        self.notes.retain(|n| any && !n.selected);
    }

    /// Move the selection by `beats` and `semitones`.
    pub fn nudge(&mut self, beats: f64, semitones: i32) {
        for n in self.selected_mut() {
            n.start = (n.start + beats).max(0.0);
            n.key = u8::try_from((i32::from(n.key) + semitones).clamp(0, 127)).unwrap_or(n.key);
        }
        self.sort();
    }

    /// Resize the selection's lengths by `delta` beats (minimum a 1/64).
    pub fn resize(&mut self, delta: f64) {
        for n in self.selected_mut() {
            n.length = (n.length + delta).max(1.0 / 16.0);
        }
    }

    /// Scale the selection's velocities by `factor`, clamped to 1..=127.
    pub fn scale_velocity(&mut self, factor: f64) {
        for n in self.selected_mut() {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "clamped"
            )]
            let v = (f64::from(n.velocity) * factor).round().clamp(1.0, 127.0) as u8;
            n.velocity = v;
        }
    }

    /// Quantise starts to `grid` beats with `strength` 0..=1 and `swing`
    /// 0..=1 (delaying every other grid line by up to half a grid).
    pub fn quantize(&mut self, grid: f64, strength: f64, swing: f64) {
        if grid <= 0.0 {
            return;
        }
        for n in self.selected_mut() {
            let k = (n.start / grid).round();
            #[expect(clippy::cast_possible_truncation, reason = "grid index parity")]
            let odd = (k as i64).rem_euclid(2) == 1;
            let target = k * grid
                + if odd {
                    swing.clamp(0.0, 1.0) * grid * 0.5
                } else {
                    0.0
                };
            n.start += (target - n.start) * strength.clamp(0.0, 1.0);
        }
        self.sort();
    }

    /// Randomise timing (± `beats`) and velocity (± `vel`), seeded.
    pub fn humanize(&mut self, beats: f64, vel: f64, seed: u64) {
        let mut r = Rng::new(seed);
        for n in self.selected_mut() {
            n.start = (n.start + r.range(-beats, beats)).max(0.0);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "clamped"
            )]
            let v = (f64::from(n.velocity) + r.range(-vel, vel))
                .round()
                .clamp(1.0, 127.0) as u8;
            n.velocity = v;
        }
        self.sort();
    }

    /// Extend each selected note to the next note's start (legato).
    pub fn legato(&mut self) {
        self.sort();
        let starts: Vec<f64> = self.notes.iter().map(|n| n.start).collect();
        let any = self.notes.iter().any(|n| n.selected);
        for i in 0..self.notes.len() {
            if any && !self.notes[i].selected {
                continue;
            }
            if let Some(next) = starts
                .iter()
                .skip(i + 1)
                .find(|s| **s > self.notes[i].start)
            {
                self.notes[i].length = next - self.notes[i].start;
            }
        }
    }

    /// The events this clip produces in `[from, to)` beats (clip time,
    /// looped if the clip loops), offset by `at` beats on the timeline.
    /// Note-offs for notes straddling `to` are emitted when their end
    /// falls in a later window, so windows can be any size.
    #[must_use]
    pub fn events(&self, from: f64, to: f64, at: f64) -> Vec<Timed<Event>> {
        let mut out = Vec::new();
        if to <= from || self.length <= 0.0 {
            return out;
        }
        let (mut a, b) = (from, to);
        if self.looping {
            let mut cycle = (a / self.length).floor();
            while a < b {
                let cycle_start = cycle * self.length;
                let cycle_end = cycle_start + self.length;
                let end = b.min(cycle_end);
                self.window(
                    a - cycle_start,
                    end - cycle_start,
                    at + cycle_start,
                    &mut out,
                );
                a = end;
                cycle += 1.0;
            }
        } else {
            self.window(a, b.min(self.length.max(b)), at, &mut out);
        }
        out.sort_by(|x, y| x.beat.total_cmp(&y.beat));
        out
    }

    fn window(&self, a: f64, b: f64, at: f64, out: &mut Vec<Timed<Event>>) {
        for n in &self.notes {
            if !self.looping && n.start >= self.length {
                continue;
            }
            if n.start >= a && n.start < b {
                out.push(Timed {
                    beat: at + n.start,
                    event: Event::Midi(Message::NoteOn {
                        channel: n.channel,
                        key: n.key,
                        velocity: n.velocity,
                    }),
                });
                // Expression at the onset (later points are the caller's
                // via `expression_at`).
                if let Some(v) = curve_at(&n.timbre, 0.0) {
                    out.push(Timed {
                        beat: at + n.start,
                        event: Event::Midi(Message::ControlChange {
                            channel: n.channel,
                            controller: 74,
                            value: to7(v),
                        }),
                    });
                }
            }
            let end = if self.looping {
                n.end().min(self.length)
            } else {
                n.end()
            };
            if end >= a && end < b {
                out.push(Timed {
                    beat: at + end,
                    event: Event::Midi(Message::NoteOff {
                        channel: n.channel,
                        key: n.key,
                        velocity: 64,
                    }),
                });
            }
        }
    }

    /// The MPE expression messages for every note sounding at clip time
    /// `t` (bend in semitones over a ±48 range, pressure, timbre).
    #[must_use]
    pub fn expression_at(&self, t: f64) -> Vec<Message> {
        let mut out = Vec::new();
        for n in self.notes.iter().filter(|n| n.start <= t && t < n.end()) {
            let rel = t - n.start;
            if let Some(b) = curve_at(&n.bend, rel) {
                #[expect(clippy::cast_possible_truncation, reason = "14-bit bend")]
                let value = ((b / 48.0).clamp(-1.0, 1.0) * 8191.0).round() as i16;
                out.push(Message::PitchBend {
                    channel: n.channel,
                    value,
                });
            }
            if let Some(p) = curve_at(&n.pressure, rel) {
                out.push(Message::ChannelPressure {
                    channel: n.channel,
                    pressure: to7(p),
                });
            }
            if let Some(v) = curve_at(&n.timbre, rel) {
                out.push(Message::ControlChange {
                    channel: n.channel,
                    controller: 74,
                    value: to7(v),
                });
            }
        }
        out
    }

    /// Export as clip notes (for [`Smf::from_notes`](crate::midi::Smf::from_notes)).
    #[must_use]
    pub fn to_clip_notes(&self) -> Vec<ClipNote> {
        self.notes
            .iter()
            .map(|n| ClipNote {
                start: n.start,
                length: n.length,
                channel: n.channel,
                key: n.key,
                velocity: n.velocity,
            })
            .collect()
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped 7-bit"
)]
fn to7(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 127.0).round() as u8
}

/// One step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Step {
    /// On?
    pub on: bool,
    /// Velocity.
    pub velocity: u8,
    /// Probability 0..=1 of playing.
    pub probability: f32,
    /// Repeats within the step (1 = normal).
    pub ratchet: u8,
    /// Gate as a fraction of the step.
    pub gate: f32,
}

impl Default for Step {
    fn default() -> Self {
        Self {
            on: false,
            velocity: 100,
            probability: 1.0,
            ratchet: 1,
            gate: 0.5,
        }
    }
}

/// A step-sequencer track.
#[derive(Debug, Clone, PartialEq)]
pub struct StepTrack {
    /// The key it plays.
    pub key: u8,
    /// Channel.
    pub channel: u8,
    /// Steps (its own length: tracks of different lengths are polymeter).
    pub steps: Vec<Step>,
}

/// A grid of tracks × steps.
#[derive(Debug, Clone, PartialEq)]
pub struct StepSequencer {
    /// Tracks.
    pub tracks: Vec<StepTrack>,
    /// Beats per step (0.25 = sixteenths).
    pub step_beats: f64,
    /// Swing 0..=1: delays odd steps by up to half a step.
    pub swing: f64,
    /// Seed for probabilities.
    pub seed: u64,
}

impl StepSequencer {
    /// Sixteenth-note steps.
    #[must_use]
    pub const fn new(tracks: Vec<StepTrack>) -> Self {
        Self {
            tracks,
            step_beats: 0.25,
            swing: 0.0,
            seed: 0,
        }
    }

    /// Events in `[from, to)` beats.
    #[must_use]
    pub fn events(&self, from: f64, to: f64) -> Vec<Timed<Event>> {
        let mut out = Vec::new();
        #[expect(clippy::cast_possible_truncation, reason = "step indices")]
        let first = (from / self.step_beats).floor() as i64 - 1;
        #[expect(clippy::cast_possible_truncation, reason = "step indices")]
        let last = (to / self.step_beats).ceil() as i64;
        for (ti, t) in self.tracks.iter().enumerate() {
            if t.steps.is_empty() {
                continue;
            }
            for k in first.max(0)..=last {
                let len = i64::try_from(t.steps.len()).unwrap_or(1);
                let s = t.steps[usize::try_from(k.rem_euclid(len)).unwrap_or(0)];
                if !s.on {
                    continue;
                }
                // Deterministic probability: hash of (seed, track, step).
                #[expect(clippy::cast_sign_loss, reason = "hash input")]
                let roll = crate::rng::hash(self.seed ^ (ti as u64).wrapping_mul(0x9E37), k as u64);
                if roll >= f64::from(s.probability) {
                    continue;
                }
                #[expect(clippy::cast_precision_loss, reason = "step indices")]
                let mut start = k as f64 * self.step_beats;
                if k % 2 == 1 {
                    start += self.swing.clamp(0.0, 1.0) * self.step_beats * 0.5;
                }
                let r = f64::from(s.ratchet.max(1));
                let sub = self.step_beats / r;
                for j in 0..s.ratchet.max(1) {
                    let on = start + f64::from(j) * sub;
                    let off = on + sub * f64::from(s.gate.clamp(0.01, 1.0));
                    if on >= from && on < to {
                        out.push(Timed {
                            beat: on,
                            event: Event::Midi(Message::NoteOn {
                                channel: t.channel,
                                key: t.key,
                                velocity: s.velocity,
                            }),
                        });
                    }
                    if off >= from && off < to {
                        out.push(Timed {
                            beat: off,
                            event: Event::Midi(Message::NoteOff {
                                channel: t.channel,
                                key: t.key,
                                velocity: 64,
                            }),
                        });
                    }
                }
            }
        }
        out.sort_by(|a, b| a.beat.total_cmp(&b.beat));
        out
    }
}

/// What a clip slot does when it finishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Follow {
    /// Keep looping (or stop, for a one-shot).
    #[default]
    None,
    /// Play the next slot down.
    Next,
    /// Stop.
    Stop,
    /// Play this slot again.
    Again,
}

/// A clip in a launcher slot.
#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    /// The clip.
    pub clip: Clip,
    /// Follow action after `follow_after` plays of the clip.
    pub follow: Follow,
    /// Plays before the follow action fires.
    pub follow_after: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Playing {
    slot: usize,
    started: f64,
}

/// Ableton-style clip launcher: tracks of slots, scenes across them,
/// launches quantised to a grid.
#[derive(Debug, Clone, PartialEq)]
pub struct Launcher {
    /// `tracks[t][s]` — slot `s` of track `t`.
    pub tracks: Vec<Vec<Option<Slot>>>,
    /// Launch quantisation in beats (4 = next bar of 4/4).
    pub quantize: f64,
    playing: Vec<Option<Playing>>,
    pending: Vec<Option<(Option<usize>, f64)>>,
}

impl Launcher {
    /// A launcher.
    #[must_use]
    pub fn new(tracks: Vec<Vec<Option<Slot>>>) -> Self {
        let n = tracks.len();
        Self {
            tracks,
            quantize: 4.0,
            playing: vec![None; n],
            pending: vec![None; n],
        }
    }

    fn next_grid(&self, now: f64) -> f64 {
        if self.quantize <= 0.0 {
            now
        } else {
            (now / self.quantize).ceil() * self.quantize
        }
    }

    /// Launch slot `s` of track `t` at the next quantisation point.
    pub fn launch(&mut self, t: usize, s: usize, now: f64) {
        let at = self.next_grid(now);
        if let Some(p) = self.pending.get_mut(t) {
            *p = Some((Some(s), at));
        }
    }

    /// Launch scene `s` (the same slot on every track).
    pub fn launch_scene(&mut self, s: usize, now: f64) {
        for t in 0..self.tracks.len() {
            if self.tracks[t].get(s).is_some_and(Option::is_some) {
                self.launch(t, s, now);
            } else {
                self.stop(t, now);
            }
        }
    }

    /// Stop track `t` at the next quantisation point.
    pub fn stop(&mut self, t: usize, now: f64) {
        let at = self.next_grid(now);
        if let Some(p) = self.pending.get_mut(t) {
            *p = Some((None, at));
        }
    }

    /// Which slot each track is playing.
    #[must_use]
    pub fn playing(&self) -> Vec<Option<usize>> {
        self.playing.iter().map(|p| p.map(|p| p.slot)).collect()
    }

    /// Advance through `[from, to)` beats, returning the events.
    pub fn events(&mut self, from: f64, to: f64) -> Vec<Timed<Event>> {
        let mut out = Vec::new();
        for t in 0..self.tracks.len() {
            let mut a = from;
            loop {
                // A pending change inside the window splits it.
                let change = self.pending[t].filter(|(_, at)| *at >= a && *at < to);
                let b = change.map_or(to, |(_, at)| at);
                if let Some(p) = self.playing[t] {
                    if let Some(Some(slot)) = self.tracks[t].get(p.slot) {
                        let clip = &slot.clip;
                        let rel_a = a - p.started;
                        let rel_b = b - p.started;
                        // Follow actions fire at the end of `follow_after` plays.
                        let span = clip.length * f64::from(slot.follow_after.max(1));
                        if slot.follow != Follow::None && rel_b > span && rel_a < span {
                            out.extend(clip_events_until(clip, rel_a, span, p.started));
                            let at = p.started + span;
                            let next = match slot.follow {
                                Follow::Next => Some(p.slot + 1).filter(|s| {
                                    self.tracks[t].get(*s).is_some_and(Option::is_some)
                                }),
                                Follow::Again => Some(p.slot),
                                _ => None,
                            };
                            self.playing[t] = next.map(|slot| Playing { slot, started: at });
                            a = at;
                            continue;
                        }
                        out.extend(clip_events_until(clip, rel_a, rel_b, p.started));
                    }
                }
                if let Some((next, at)) = change {
                    // Silence what was playing, then start the new clip.
                    if let Some(p) = self.playing[t] {
                        if let Some(Some(slot)) = self.tracks[t].get(p.slot) {
                            for n in &slot.clip.notes {
                                out.push(Timed {
                                    beat: at,
                                    event: Event::Midi(Message::NoteOff {
                                        channel: n.channel,
                                        key: n.key,
                                        velocity: 64,
                                    }),
                                });
                            }
                        }
                    }
                    self.playing[t] = next.map(|slot| Playing { slot, started: at });
                    self.pending[t] = None;
                    a = at;
                    continue;
                }
                break;
            }
        }
        out.sort_by(|x, y| x.beat.total_cmp(&y.beat));
        out
    }
}

fn clip_events_until(clip: &Clip, a: f64, b: f64, at: f64) -> Vec<Timed<Event>> {
    if clip.looping {
        clip.events(a, b, at)
    } else {
        clip.events(a, b.min(clip.length + 1e-9), at)
    }
}

/// A clip placed on the timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    /// Where it starts, in beats.
    pub at: f64,
    /// How long it plays (the clip loops if longer than itself and looping).
    pub length: f64,
    /// The clip.
    pub clip: Clip,
}

/// An arrangement track.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ArrTrack {
    /// Name.
    pub name: String,
    /// Placements.
    pub clips: Vec<Placement>,
    /// Muted.
    pub muted: bool,
}

/// A linear arrangement.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Arrangement {
    /// Tracks.
    pub tracks: Vec<ArrTrack>,
}

impl Arrangement {
    /// Events in `[from, to)` beats, tagged with their track index.
    #[must_use]
    pub fn events(&self, from: f64, to: f64) -> Vec<(usize, Timed<Event>)> {
        let mut out = Vec::new();
        for (ti, t) in self.tracks.iter().enumerate() {
            if t.muted {
                continue;
            }
            for p in &t.clips {
                let (a, b) = (from.max(p.at), to.min(p.at + p.length));
                if b <= a {
                    continue;
                }
                for e in p.clip.events(a - p.at, b - p.at, p.at) {
                    out.push((ti, e));
                }
            }
        }
        out.sort_by(|x, y| x.1.beat.total_cmp(&y.1.beat));
        out
    }

    /// The end of the last placement.
    #[must_use]
    pub fn length(&self) -> f64 {
        self.tracks
            .iter()
            .flat_map(|t| t.clips.iter().map(|p| p.at + p.length))
            .fold(0.0, f64::max)
    }
}

/// Arpeggiator note order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArpMode {
    /// Lowest to highest.
    #[default]
    Up,
    /// Highest to lowest.
    Down,
    /// Up then down, ends not repeated.
    UpDown,
    /// In the order keys were pressed.
    AsPlayed,
    /// Random (seeded).
    Random,
}

/// An arpeggiator over held keys.
#[derive(Debug, Clone, PartialEq)]
pub struct Arpeggiator {
    /// Order.
    pub mode: ArpMode,
    /// Octave range (1 = no extra octaves).
    pub octaves: u8,
    /// Beats per arp step.
    pub rate: f64,
    /// Gate fraction 0..=1.
    pub gate: f64,
    /// Channel.
    pub channel: u8,
    /// Seed for random mode.
    pub seed: u64,
    held: Vec<(u8, u8)>,
}

impl Arpeggiator {
    /// An arp at `rate` beats per step.
    #[must_use]
    pub const fn new(mode: ArpMode, rate: f64) -> Self {
        Self {
            mode,
            octaves: 1,
            rate,
            gate: 0.5,
            channel: 0,
            seed: 0,
            held: Vec::new(),
        }
    }

    /// A key went down.
    pub fn press(&mut self, key: u8, velocity: u8) {
        if !self.held.iter().any(|(k, _)| *k == key) {
            self.held.push((key, velocity));
        }
    }

    /// A key came up.
    pub fn release(&mut self, key: u8) {
        self.held.retain(|(k, _)| *k != key);
    }

    /// The note sequence one cycle plays.
    #[must_use]
    pub fn sequence(&self) -> Vec<(u8, u8)> {
        let mut base = self.held.clone();
        if !matches!(self.mode, ArpMode::AsPlayed) {
            base.sort_by_key(|(k, _)| *k);
        }
        let mut seq = Vec::new();
        for o in 0..self.octaves.max(1) {
            for &(k, v) in &base {
                let key = u16::from(k) + 12 * u16::from(o);
                if key <= 127 {
                    seq.push((u8::try_from(key).unwrap_or(127), v));
                }
            }
        }
        match self.mode {
            ArpMode::Down => seq.reverse(),
            ArpMode::UpDown if seq.len() > 2 => {
                let mut down: Vec<_> = seq[1..seq.len() - 1].to_vec();
                down.reverse();
                seq.extend(down);
            }
            _ => {}
        }
        seq
    }

    /// Events in `[from, to)` beats for the keys currently held.
    #[must_use]
    pub fn events(&self, from: f64, to: f64) -> Vec<Timed<Event>> {
        let seq = self.sequence();
        let mut out = Vec::new();
        if seq.is_empty() || self.rate <= 0.0 {
            return out;
        }
        #[expect(clippy::cast_possible_truncation, reason = "step indices")]
        let first = ((from / self.rate).floor() as i64 - 1).max(0);
        #[expect(clippy::cast_possible_truncation, reason = "step indices")]
        let last = (to / self.rate).ceil() as i64;
        let len = i64::try_from(seq.len()).unwrap_or(1);
        for k in first..=last {
            let idx = if self.mode == ArpMode::Random {
                #[expect(
                    clippy::cast_sign_loss,
                    clippy::cast_possible_truncation,
                    clippy::cast_precision_loss,
                    reason = "index"
                )]
                let i = (crate::rng::hash(self.seed, k as u64) * seq.len() as f64) as usize;
                i.min(seq.len() - 1)
            } else {
                usize::try_from(k.rem_euclid(len)).unwrap_or(0)
            };
            let (key, vel) = seq[idx];
            #[expect(clippy::cast_precision_loss, reason = "step indices")]
            let on = k as f64 * self.rate;
            let off = on + self.rate * self.gate.clamp(0.01, 1.0);
            if on >= from && on < to {
                out.push(Timed {
                    beat: on,
                    event: Event::Midi(Message::NoteOn {
                        channel: self.channel,
                        key,
                        velocity: vel,
                    }),
                });
            }
            if off >= from && off < to {
                out.push(Timed {
                    beat: off,
                    event: Event::Midi(Message::NoteOff {
                        channel: self.channel,
                        key,
                        velocity: 64,
                    }),
                });
            }
        }
        out.sort_by(|a, b| a.beat.total_cmp(&b.beat));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ons(ev: &[Timed<Event>]) -> Vec<(f64, u8)> {
        ev.iter()
            .filter_map(|e| match &e.event {
                Event::Midi(Message::NoteOn { key, .. }) => Some((e.beat, *key)),
                _ => None,
            })
            .collect()
    }

    fn clip() -> Clip {
        let mut c = Clip::new(4.0);
        c.add(Note::new(0.0, 1.0, 60, 100));
        c.add(Note::new(1.02, 0.5, 64, 90));
        c.add(Note::new(2.47, 1.0, 67, 80));
        c
    }

    #[test]
    fn clip_events_window_any_size() {
        let c = clip();
        let all = c.events(0.0, 4.0, 0.0);
        let mut pieces = Vec::new();
        let mut t = 0.0;
        while t < 4.0 {
            pieces.extend(c.events(t, t + 0.37, 0.0));
            t += 0.37;
        }
        assert_eq!(all.len(), 6);
        assert_eq!(all.len(), pieces.len());
        assert_eq!(ons(&all), ons(&pieces));
    }

    #[test]
    fn looping_clips_repeat_and_offset() {
        let mut c = clip();
        c.looping = true;
        let ev = c.events(0.0, 12.0, 100.0);
        assert_eq!(ons(&ev).len(), 9);
        assert!((ons(&ev)[3].0 - 104.0).abs() < 1e-9);
    }

    #[test]
    fn piano_roll_edits() {
        let mut c = clip();
        assert_eq!(c.select_region((1.0, 3.0), (60, 70)), 2);
        c.nudge(0.5, 2);
        assert_eq!(
            c.notes
                .iter()
                .filter(|n| n.selected)
                .map(|n| n.key)
                .collect::<Vec<_>>(),
            vec![66, 69]
        );
        c.select_none();
        c.quantize(0.5, 1.0, 0.0);
        assert!(c
            .notes
            .iter()
            .all(|n| (n.start * 2.0 - (n.start * 2.0).round()).abs() < 1e-9));
        c.scale_velocity(2.0);
        assert!(c.notes.iter().all(|n| n.velocity == 127));
        c.legato();
        assert!((c.notes[0].end() - c.notes[1].start).abs() < 1e-9);
        c.select_region((0.0, 0.5), (0, 127));
        c.delete_selected();
        assert_eq!(c.notes.len(), 2);
    }

    #[test]
    fn swing_and_partial_quantize() {
        let mut c = Clip::new(2.0);
        c.add(Note::new(0.5 + 0.1, 0.25, 60, 100));
        c.quantize(0.5, 0.5, 0.0);
        assert!((c.notes[0].start - 0.55).abs() < 1e-9, "half strength");
        c.quantize(0.5, 1.0, 1.0);
        assert!(
            (c.notes[0].start - 0.75).abs() < 1e-9,
            "odd grid line swung by half a grid"
        );
    }

    #[test]
    fn mpe_expression_curves_interpolate() {
        let mut n = Note::new(0.0, 2.0, 60, 100);
        n.channel = 3;
        n.bend = vec![(0.0, 0.0), (1.0, 12.0)];
        n.pressure = vec![(0.0, 0.2), (2.0, 1.0)];
        let mut c = Clip::new(2.0);
        c.add(n);
        let m = c.expression_at(0.5);
        assert!(
            m.contains(&Message::PitchBend {
                channel: 3,
                value: 1024
            }),
            "{m:?}"
        );
        assert!(m.contains(&Message::ChannelPressure {
            channel: 3,
            pressure: 51
        }));
        assert!(c.expression_at(3.0).is_empty());
    }

    #[test]
    fn step_sequencer_patterns_swing_ratchets_and_probability() {
        let mut steps = vec![Step::default(); 4];
        steps[0].on = true;
        steps[1].on = true;
        steps[2] = Step {
            on: true,
            ratchet: 3,
            ..Step::default()
        };
        let mut s = StepSequencer::new(vec![StepTrack {
            key: 36,
            channel: 9,
            steps,
        }]);
        s.swing = 1.0;
        let ev = s.events(0.0, 1.0);
        let o = ons(&ev);
        assert_eq!(o.len(), 5);
        assert!((o[1].0 - 0.375).abs() < 1e-9, "odd step swung");
        assert!((o[3].0 - (0.5 + 0.25 / 3.0)).abs() < 1e-9, "ratchet");
        let mut p = vec![
            Step {
                on: true,
                probability: 0.5,
                ..Step::default()
            };
            16
        ];
        p[0].probability = 1.0;
        let s = StepSequencer::new(vec![StepTrack {
            key: 42,
            channel: 9,
            steps: p,
        }]);
        let n = ons(&s.events(0.0, 64.0)).len();
        assert!((100..170).contains(&n), "{n} of 256");
        assert_eq!(s.events(0.0, 64.0), s.events(0.0, 64.0), "deterministic");
    }

    #[test]
    fn polymeter_tracks_drift_apart() {
        let three = StepTrack {
            key: 1,
            channel: 0,
            steps: vec![
                Step {
                    on: true,
                    ..Step::default()
                },
                Step::default(),
                Step::default(),
            ],
        };
        let four = StepTrack {
            key: 2,
            channel: 0,
            steps: vec![
                Step {
                    on: true,
                    ..Step::default()
                },
                Step::default(),
                Step::default(),
                Step::default(),
            ],
        };
        let s = StepSequencer::new(vec![three, four]);
        let o = ons(&s.events(0.0, 3.0));
        let t1: Vec<f64> = o.iter().filter(|e| e.1 == 1).map(|e| e.0).collect();
        let t2: Vec<f64> = o.iter().filter(|e| e.1 == 2).map(|e| e.0).collect();
        assert_eq!(t1, vec![0.0, 0.75, 1.5, 2.25]);
        assert_eq!(t2, vec![0.0, 1.0, 2.0]);
    }

    #[test]
    fn launcher_quantizes_and_follows() {
        let mut a = Clip::new(2.0);
        a.add(Note::new(0.0, 0.5, 60, 100));
        a.looping = true;
        let mut b = Clip::new(1.0);
        b.add(Note::new(0.0, 0.5, 72, 100));
        let mut l = Launcher::new(vec![vec![
            Some(Slot {
                clip: a,
                follow: Follow::Next,
                follow_after: 2,
            }),
            Some(Slot {
                clip: b,
                follow: Follow::Stop,
                follow_after: 1,
            }),
        ]]);
        l.launch(0, 0, 1.3);
        let ev = l.events(0.0, 12.0);
        let o = ons(&ev);
        assert_eq!(
            o,
            vec![(4.0, 60), (6.0, 60), (8.0, 72)],
            "starts on the bar, plays twice, follows, stops"
        );
        assert_eq!(l.playing(), vec![None]);
    }

    #[test]
    fn scenes_launch_across_tracks() {
        let mut c = Clip::new(1.0);
        c.looping = true;
        c.add(Note::new(0.0, 0.5, 50, 100));
        let slot = Some(Slot {
            clip: c,
            follow: Follow::None,
            follow_after: 1,
        });
        let mut l = Launcher::new(vec![vec![slot.clone()], vec![slot]]);
        l.quantize = 1.0;
        l.launch_scene(0, 0.2);
        let o = ons(&l.events(0.0, 3.0));
        assert_eq!(o.len(), 4);
        assert_eq!(l.playing(), vec![Some(0), Some(0)]);
    }

    #[test]
    fn arrangement_places_and_mutes() {
        let mut c = Clip::new(1.0);
        c.looping = true;
        c.add(Note::new(0.0, 0.5, 60, 100));
        let arr = Arrangement {
            tracks: vec![
                ArrTrack {
                    name: "a".into(),
                    clips: vec![Placement {
                        at: 4.0,
                        length: 3.0,
                        clip: c.clone(),
                    }],
                    muted: false,
                },
                ArrTrack {
                    name: "b".into(),
                    clips: vec![Placement {
                        at: 0.0,
                        length: 1.0,
                        clip: c,
                    }],
                    muted: true,
                },
            ],
        };
        let ev = arr.events(0.0, 10.0);
        let starts: Vec<f64> = ev
            .iter()
            .filter(|e| matches!(e.1.event, Event::Midi(Message::NoteOn { .. })))
            .map(|e| e.1.beat)
            .collect();
        assert_eq!(starts, vec![4.0, 5.0, 6.0]);
        assert!((arr.length() - 7.0).abs() < 1e-12);
    }

    #[test]
    fn arpeggiator_modes() {
        let mut a = Arpeggiator::new(ArpMode::UpDown, 0.25);
        for k in [64, 60, 67] {
            a.press(k, 100);
        }
        let keys: Vec<u8> = a.sequence().iter().map(|x| x.0).collect();
        assert_eq!(keys, vec![60, 64, 67, 64]);
        a.mode = ArpMode::AsPlayed;
        a.octaves = 2;
        let keys: Vec<u8> = a.sequence().iter().map(|x| x.0).collect();
        assert_eq!(keys, vec![64, 60, 67, 76, 72, 79]);
        a.mode = ArpMode::Up;
        a.octaves = 1;
        let o = ons(&a.events(0.0, 1.0));
        assert_eq!(o, vec![(0.0, 60), (0.25, 64), (0.5, 67), (0.75, 60)]);
        a.release(64);
        assert_eq!(a.sequence().len(), 2);
    }

    #[test]
    fn clips_export_to_midi_files() {
        let c = clip();
        let smf = crate::midi::Smf::from_notes(&c.to_clip_notes(), 960, 120.0);
        let back = crate::midi::Smf::parse(&smf.to_bytes()).unwrap();
        assert_eq!(back.notes().len(), 3);
    }
}
