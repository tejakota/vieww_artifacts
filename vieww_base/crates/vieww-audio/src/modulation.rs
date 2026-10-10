//! Modulation: LFOs (free or tempo-synced), sample-and-hold, macros, a
//! modulation matrix, and MIDI learn.
//!
//! # LFOs are functions of position, not counters
//!
//! [`Lfo::at_seconds`] and [`Lfo::at_beats`] compute the LFO's value from a
//! position — never from "how many times was I stepped". That is the crate's
//! rule applied to modulation, and it is what makes tempo sync *exact*: an
//! LFO locked to a 1/8-note dotted grid is a function of the transport's beat
//! position, so seeking the transport, looping a bar or rendering a section
//! twice lands the LFO in the same place every time. Even the random shapes
//! obey it — sample-and-hold's value in cycle `k` is a seeded hash of `k`.
//!
//! # The matrix
//!
//! [`ModMatrix`] routes any source to any destination with a depth and an
//! optional *via* source scaling it (velocity-scaled vibrato, mod-wheel
//! amount). Sources and destinations are plain indices, so the matrix serves
//! any engine; [`Destination`] carries each target's range and how
//! modulation applies to it (linear, or in octaves for pitches/cutoffs).

use crate::rng::hash;

/// LFO waveform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LfoShape {
    /// Sine.
    #[default]
    Sine,
    /// Triangle.
    Triangle,
    /// Rising saw.
    SawUp,
    /// Falling saw.
    SawDown,
    /// Square.
    Square,
    /// A new random value each cycle, held (stepped).
    SampleHold,
    /// A new random value each cycle, cosine-interpolated (smooth).
    SmoothRandom,
}

/// A note-length division for tempo sync.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Division {
    /// Length in quarter-note beats (1/4 note = 1.0, 1/8 = 0.5, 1 bar of
    /// 4/4 = 4.0).
    pub beats: f64,
}

impl Division {
    /// A straight division of a whole note: `straight(8)` is an eighth.
    #[must_use]
    pub fn straight(denominator: u32) -> Self {
        Self {
            beats: 4.0 / f64::from(denominator.max(1)),
        }
    }

    /// Dotted: 1.5× length.
    #[must_use]
    pub fn dotted(denominator: u32) -> Self {
        Self {
            beats: Self::straight(denominator).beats * 1.5,
        }
    }

    /// Triplet: 2/3 length.
    #[must_use]
    pub fn triplet(denominator: u32) -> Self {
        Self {
            beats: Self::straight(denominator).beats * 2.0 / 3.0,
        }
    }

    /// Bars of 4/4.
    #[must_use]
    pub fn bars(n: f64) -> Self {
        Self { beats: 4.0 * n }
    }
}

/// How an LFO runs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LfoRate {
    /// Free-running, in Hz.
    Hz(f64),
    /// One cycle per division, locked to the transport.
    Sync(Division),
}

/// A low-frequency oscillator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lfo {
    /// Shape.
    pub shape: LfoShape,
    /// Rate.
    pub rate: LfoRate,
    /// Phase offset in turns.
    pub phase: f64,
    /// Output in `0..=1` instead of `-1..=1`.
    pub unipolar: bool,
    /// Seconds before the LFO starts fading in (for `note_seconds`).
    pub delay: f64,
    /// Seconds of fade-in after the delay.
    pub fade: f64,
    /// Seed for the random shapes.
    pub seed: u64,
}

impl Lfo {
    /// A free LFO at `hz`.
    #[must_use]
    pub const fn hz(shape: LfoShape, hz: f64) -> Self {
        Self {
            shape,
            rate: LfoRate::Hz(hz),
            phase: 0.0,
            unipolar: false,
            delay: 0.0,
            fade: 0.0,
            seed: 0,
        }
    }

    /// A tempo-synced LFO.
    #[must_use]
    pub const fn synced(shape: LfoShape, division: Division) -> Self {
        Self {
            shape,
            rate: LfoRate::Sync(division),
            phase: 0.0,
            unipolar: false,
            delay: 0.0,
            fade: 0.0,
            seed: 0,
        }
    }

    /// The value at phase `cycles` (whole cycles elapsed, fractional).
    #[must_use]
    pub fn at_cycles(&self, cycles: f64) -> f32 {
        let c = cycles + self.phase;
        let t = c.rem_euclid(1.0);
        #[expect(clippy::cast_possible_truncation, reason = "cycle indices")]
        let k = c.floor() as i64;
        #[expect(clippy::cast_sign_loss, reason = "hash input, sign irrelevant")]
        let rnd = |i: i64| hash(self.seed, i as u64) * 2.0 - 1.0;
        let v = match self.shape {
            LfoShape::Sine => (std::f64::consts::TAU * t).sin(),
            LfoShape::Triangle => 4.0 * ((t - 0.25).rem_euclid(1.0) - 0.5).abs() - 1.0,
            LfoShape::SawUp => 2.0 * t - 1.0,
            LfoShape::SawDown => 1.0 - 2.0 * t,
            LfoShape::Square => {
                if t < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            LfoShape::SampleHold => rnd(k),
            LfoShape::SmoothRandom => {
                let a = rnd(k);
                let b = rnd(k + 1);
                let w = 0.5 - 0.5 * (std::f64::consts::PI * t).cos();
                a + (b - a) * w
            }
        };
        let v = if self.unipolar { (v + 1.0) / 2.0 } else { v };
        #[expect(clippy::cast_possible_truncation, reason = "modulation is f32")]
        let out = v as f32;
        out
    }

    /// The value at `seconds` (free LFOs), or at `seconds` converted through
    /// `bpm` (synced LFOs at a constant tempo).
    #[must_use]
    pub fn at_seconds(&self, seconds: f64, bpm: f64) -> f32 {
        match self.rate {
            LfoRate::Hz(hz) => self.at_cycles(seconds * hz),
            LfoRate::Sync(_) => self.at_beats(seconds * bpm / 60.0, seconds),
        }
    }

    /// The value at transport position `beats` (quarter notes). Free LFOs
    /// use `seconds` instead.
    #[must_use]
    pub fn at_beats(&self, beats: f64, seconds: f64) -> f32 {
        match self.rate {
            LfoRate::Hz(hz) => self.at_cycles(seconds * hz),
            LfoRate::Sync(d) => self.at_cycles(beats / d.beats.max(1e-9)),
        }
    }

    /// The value for a note `note_seconds` old, including delay and fade-in
    /// (the classic delayed vibrato).
    #[must_use]
    pub fn at_note(&self, note_seconds: f64, bpm: f64) -> f32 {
        let t = note_seconds - self.delay;
        if t <= 0.0 {
            return 0.0;
        }
        #[expect(clippy::cast_possible_truncation, reason = "gain")]
        let g = if self.fade > 0.0 {
            (t / self.fade).min(1.0) as f32
        } else {
            1.0
        };
        self.at_seconds(t, bpm) * g
    }
}

/// How modulation applies to a destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scale {
    /// `value + depth × source × (max − min)`.
    #[default]
    Linear,
    /// `value × 2^(depth × source)` — octaves, for frequencies.
    Octaves,
}

/// A modulation destination's range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Destination {
    /// Lowest value.
    pub min: f32,
    /// Highest value.
    pub max: f32,
    /// How modulation applies.
    pub scale: Scale,
}

/// One routing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Route {
    /// Source index.
    pub source: usize,
    /// Destination index.
    pub dest: usize,
    /// Depth (−1..1 typical; octaves for [`Scale::Octaves`]).
    pub depth: f32,
    /// A second source multiplying this route (mod wheel, velocity).
    pub via: Option<usize>,
}

/// A modulation matrix.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ModMatrix {
    /// Destination ranges, by index.
    pub destinations: Vec<Destination>,
    /// The routings.
    pub routes: Vec<Route>,
}

impl ModMatrix {
    /// Apply the matrix: `base[d]` modulated by `sources`, clamped to each
    /// destination's range.
    #[must_use]
    pub fn apply(&self, base: &[f32], sources: &[f32]) -> Vec<f32> {
        let mut lin = vec![0.0f32; self.destinations.len()];
        let mut oct = vec![0.0f32; self.destinations.len()];
        for r in &self.routes {
            let (Some(d), Some(s)) = (self.destinations.get(r.dest), sources.get(r.source)) else {
                continue;
            };
            let via = r.via.and_then(|v| sources.get(v)).copied().unwrap_or(1.0);
            let amt = r.depth * s * via;
            match d.scale {
                Scale::Linear => lin[r.dest] += amt * (d.max - d.min),
                Scale::Octaves => oct[r.dest] += amt,
            }
        }
        self.destinations
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let b = base.get(i).copied().unwrap_or(d.min);
                let v = match d.scale {
                    Scale::Linear => b + lin[i],
                    Scale::Octaves => b * 2f32.powf(oct[i]),
                };
                v.clamp(d.min.min(d.max), d.max.max(d.min))
            })
            .collect()
    }
}

/// One parameter a macro drives.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MacroTarget {
    /// The destination index.
    pub dest: usize,
    /// Value at macro 0.
    pub from: f32,
    /// Value at macro 1.
    pub to: f32,
    /// Response curve exponent (1 = linear, 2 = slow start, 0.5 = fast).
    pub curve: f32,
}

/// One knob driving many parameters, each over its own range and curve.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Macro {
    /// Name shown on the knob.
    pub name: String,
    /// The targets.
    pub targets: Vec<MacroTarget>,
}

impl Macro {
    /// Write the macro at `value` (0..=1) into `params`.
    pub fn apply(&self, value: f32, params: &mut [f32]) {
        let v = value.clamp(0.0, 1.0);
        for t in &self.targets {
            if let Some(p) = params.get_mut(t.dest) {
                let x = v.powf(t.curve.max(1e-3));
                *p = t.from + (t.to - t.from) * x;
            }
        }
    }
}

/// MIDI learn: "wiggle a knob, then click a parameter" bindings from CCs to
/// parameter indices.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MidiLearn {
    /// (channel, controller) → (parameter, min, max).
    pub bindings: std::collections::BTreeMap<(u8, u8), (usize, f32, f32)>,
    armed: Option<(usize, f32, f32)>,
}

impl MidiLearn {
    /// Arm learning for `param` over `min..=max`: the next CC binds to it.
    pub const fn learn(&mut self, param: usize, min: f32, max: f32) {
        self.armed = Some((param, min, max));
    }

    /// Whether learning is armed.
    #[must_use]
    pub const fn is_armed(&self) -> bool {
        self.armed.is_some()
    }

    /// Remove any binding to `param`.
    pub fn forget(&mut self, param: usize) {
        self.bindings.retain(|_, v| v.0 != param);
    }

    /// Feed a message; returns `(param, value)` if it moves a bound
    /// parameter. A CC arriving while armed binds first (replacing any
    /// existing binding of that CC or that parameter) and then applies.
    pub fn handle(&mut self, m: &crate::midi::Message) -> Option<(usize, f32)> {
        let crate::midi::Message::ControlChange {
            channel,
            controller,
            value,
        } = *m
        else {
            return None;
        };
        if let Some(arm) = self.armed.take() {
            self.forget(arm.0);
            self.bindings.insert((channel, controller), arm);
        }
        let &(p, lo, hi) = self.bindings.get(&(channel, controller))?;
        Some((p, lo + (hi - lo) * f32::from(value) / 127.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::Message;

    #[test]
    fn shapes_hit_their_landmarks() {
        let at = |s: LfoShape, c: f64| Lfo::hz(s, 1.0).at_cycles(c);
        assert!((at(LfoShape::Sine, 0.25) - 1.0).abs() < 1e-6);
        assert!((at(LfoShape::Triangle, 0.25) - 1.0).abs() < 1e-6);
        assert!((at(LfoShape::Triangle, 0.75) + 1.0).abs() < 1e-6);
        assert!(at(LfoShape::Triangle, 0.0).abs() < 1e-6);
        assert!((at(LfoShape::SawUp, 0.0) + 1.0).abs() < 1e-6);
        assert!((at(LfoShape::SawDown, 0.0) - 1.0).abs() < 1e-6);
        assert!((at(LfoShape::Square, 0.1) - 1.0).abs() < 1e-6);
        assert!((at(LfoShape::Square, 0.6) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn sample_and_hold_is_constant_within_a_cycle_and_reproducible() {
        let l = Lfo {
            seed: 42,
            ..Lfo::hz(LfoShape::SampleHold, 1.0)
        };
        let a = l.at_cycles(3.1);
        assert_eq!(a, l.at_cycles(3.9));
        assert_ne!(a, l.at_cycles(4.1));
        assert!((-1.0..=1.0).contains(&a));
    }

    #[test]
    fn smooth_random_is_continuous_across_cycles() {
        let l = Lfo::hz(LfoShape::SmoothRandom, 1.0);
        let a = l.at_cycles(4.999_99);
        let b = l.at_cycles(5.000_01);
        assert!((a - b).abs() < 1e-3);
    }

    #[test]
    fn synced_lfo_locks_to_the_beat_grid() {
        // A quarter-note LFO completes a cycle every beat at any tempo.
        let l = Lfo::synced(LfoShape::SawUp, Division::straight(4));
        assert!((l.at_beats(2.0, 0.0) + 1.0).abs() < 1e-6);
        assert!(l.at_beats(2.5, 0.0).abs() < 1e-6);
        // At 120 BPM, a quarter note is 0.5 s.
        assert!(l.at_seconds(0.25, 120.0).abs() < 1e-6);
        // Dotted eighth is 0.75 beats; triplet quarter 2/3.
        assert!((Division::dotted(8).beats - 0.75).abs() < 1e-12);
        assert!((Division::triplet(4).beats - 2.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn delayed_vibrato_fades_in() {
        let l = Lfo {
            delay: 0.5,
            fade: 0.5,
            ..Lfo::hz(LfoShape::Square, 5.0)
        };
        assert_eq!(l.at_note(0.3, 120.0), 0.0);
        let half = l.at_note(0.75, 120.0).abs();
        let full = l.at_note(1.5, 120.0).abs();
        assert!((half - 0.5).abs() < 1e-6 && (full - 1.0).abs() < 1e-6);
    }

    #[test]
    fn matrix_routes_scales_and_clamps() {
        let m = ModMatrix {
            destinations: vec![
                Destination {
                    min: 20.0,
                    max: 20_000.0,
                    scale: Scale::Octaves,
                },
                Destination {
                    min: 0.0,
                    max: 1.0,
                    scale: Scale::Linear,
                },
            ],
            routes: vec![
                Route {
                    source: 0,
                    dest: 0,
                    depth: 2.0,
                    via: None,
                },
                Route {
                    source: 1,
                    dest: 1,
                    depth: 0.5,
                    via: Some(2),
                },
            ],
        };
        let out = m.apply(&[1000.0, 0.5], &[1.0, 1.0, 0.5]);
        assert!((out[0] - 4000.0).abs() < 1e-2, "two octaves up");
        assert!((out[1] - 0.75).abs() < 1e-6, "via scales depth");
        let out = m.apply(&[1000.0, 0.9], &[10.0, 1.0, 1.0]);
        assert!(
            (out[0] - 20_000.0).abs() < 1e-3 && (out[1] - 1.0).abs() < 1e-6,
            "clamped"
        );
    }

    #[test]
    fn macro_drives_several_ranges() {
        let mac = Macro {
            name: "Brightness".into(),
            targets: vec![
                MacroTarget {
                    dest: 0,
                    from: 200.0,
                    to: 8000.0,
                    curve: 2.0,
                },
                MacroTarget {
                    dest: 1,
                    from: 1.0,
                    to: 0.0,
                    curve: 1.0,
                },
            ],
        };
        let mut p = [0.0; 2];
        mac.apply(0.5, &mut p);
        assert!((p[0] - (200.0 + 7800.0 * 0.25)).abs() < 1e-3);
        assert!((p[1] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn midi_learn_binds_the_next_cc() {
        let mut ml = MidiLearn::default();
        let cc = |c, v| Message::ControlChange {
            channel: 0,
            controller: c,
            value: v,
        };
        assert_eq!(ml.handle(&cc(1, 64)), None);
        ml.learn(7, 100.0, 200.0);
        let (p, v) = ml.handle(&cc(21, 127)).unwrap();
        assert_eq!(p, 7);
        assert!((v - 200.0).abs() < 1e-4);
        assert!(!ml.is_armed());
        let (_, v) = ml.handle(&cc(21, 0)).unwrap();
        assert!((v - 100.0).abs() < 1e-4);
        // Relearning moves the binding.
        ml.learn(7, 0.0, 1.0);
        ml.handle(&cc(22, 0));
        assert!(ml.handle(&cc(21, 5)).is_none());
    }
}
