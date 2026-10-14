//! FM synthesis — six operators, the DX7's 32 algorithms, and feedback.
//!
//! An *operator* is a sine oscillator with its own frequency ratio, output
//! level and envelope. An *algorithm* says which operators modulate which:
//! a modulator's output bends its target's phase, and the target's spectrum
//! grows sidebands at `f_c ± k·f_m` with Bessel-function amplitudes — bells,
//! electric pianos, metallic basses, sounds subtractive synthesis cannot
//! reach. Operators that modulate nothing are *carriers* and are heard.
//!
//! # Units
//!
//! An operator's output is `level × envelope × sin(…)`. As a **carrier** that
//! is an audio sample. As a **modulator** the same number is the
//! *modulation index* β in radians: a modulator at level 1.5 produces
//! `sin(ω_c t + 1.5 sin(ω_m t))`, whose spectrum is the textbook
//! `Σ J_k(1.5)`. One unit, both roles — the tests check the Bessel
//! amplitudes directly.
//!
//! # Phase vs frequency modulation
//!
//! The DX7 (and almost every "FM" synth) is really phase modulation:
//! [`Modulation::Phase`] adds the modulator to the carrier's phase.
//! [`Modulation::Frequency`] adds it to the carrier's *frequency* (scaled by
//! the modulator's own frequency, so β means the same thing). For a sine
//! modulator the two give the same magnitudes with different sideband
//! phases; they differ for complex modulators and in how pitch drifts under
//! a DC offset, which is why both exist.
//!
//! # The algorithms
//!
//! [`Algorithm::dx7`] returns the 32 routings from the DX7's algorithm
//! chart, operators numbered 1–6 as on the panel. In every one a modulator
//! has a higher number than its target, so evaluating 6 → 1 always sees a
//! modulator's *current* output; feedback loops that run "backwards"
//! (algorithms 4 and 6) use the previous sample, as the hardware did.

use super::envelope::{Adsr, EnvelopeGen, Stages};

/// Operator count.
pub const OPS: usize = 6;

/// Which kind of modulation operators apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Modulation {
    /// Add to phase (DX7-style).
    #[default]
    Phase,
    /// Add to instantaneous frequency.
    Frequency,
}

/// Routing between six operators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Algorithm {
    /// `mods[target]` lists the 0-based sources modulating `target`.
    pub mods: [Vec<usize>; OPS],
    /// Which operators are heard.
    pub carriers: [bool; OPS],
    /// `(from, to)`: `from`'s recent output feeds `to`'s phase.
    pub feedback: Option<(usize, usize)>,
}

impl Algorithm {
    /// Build from 1-based `(source, target)` pairs, 1-based carriers and a
    /// 1-based feedback pair.
    #[must_use]
    pub fn from_pairs(
        pairs: &[(usize, usize)],
        carriers: &[usize],
        feedback: (usize, usize),
    ) -> Self {
        let mut mods: [Vec<usize>; OPS] = Default::default();
        for &(s, t) in pairs {
            mods[t - 1].push(s - 1);
        }
        let mut c = [false; OPS];
        for &k in carriers {
            c[k - 1] = true;
        }
        Self {
            mods,
            carriers: c,
            feedback: Some((feedback.0 - 1, feedback.1 - 1)),
        }
    }

    /// DX7 algorithm `n` (1..=32).
    ///
    /// # Panics
    /// If `n` is outside 1..=32.
    #[must_use]
    pub fn dx7(n: u8) -> Self {
        let p = Self::from_pairs;
        match n {
            1 => p(&[(6, 5), (5, 4), (4, 3), (2, 1)], &[1, 3], (6, 6)),
            2 => p(&[(6, 5), (5, 4), (4, 3), (2, 1)], &[1, 3], (2, 2)),
            3 => p(&[(6, 5), (5, 4), (3, 2), (2, 1)], &[1, 4], (6, 6)),
            4 => p(&[(6, 5), (5, 4), (3, 2), (2, 1)], &[1, 4], (4, 6)),
            5 => p(&[(6, 5), (4, 3), (2, 1)], &[1, 3, 5], (6, 6)),
            6 => p(&[(6, 5), (4, 3), (2, 1)], &[1, 3, 5], (5, 6)),
            7 => p(&[(6, 5), (5, 3), (4, 3), (2, 1)], &[1, 3], (6, 6)),
            8 => p(&[(6, 5), (5, 3), (4, 3), (2, 1)], &[1, 3], (4, 4)),
            9 => p(&[(6, 5), (5, 3), (4, 3), (2, 1)], &[1, 3], (2, 2)),
            10 => p(&[(3, 2), (2, 1), (6, 4), (5, 4)], &[1, 4], (3, 3)),
            11 => p(&[(3, 2), (2, 1), (6, 4), (5, 4)], &[1, 4], (6, 6)),
            12 => p(&[(2, 1), (6, 3), (5, 3), (4, 3)], &[1, 3], (2, 2)),
            13 => p(&[(2, 1), (6, 3), (5, 3), (4, 3)], &[1, 3], (6, 6)),
            14 => p(&[(2, 1), (6, 4), (5, 4), (4, 3)], &[1, 3], (6, 6)),
            15 => p(&[(2, 1), (6, 4), (5, 4), (4, 3)], &[1, 3], (2, 2)),
            16 => p(&[(2, 1), (3, 1), (5, 1), (4, 3), (6, 5)], &[1], (6, 6)),
            17 => p(&[(2, 1), (3, 1), (5, 1), (4, 3), (6, 5)], &[1], (2, 2)),
            18 => p(&[(2, 1), (3, 1), (4, 1), (5, 4), (6, 5)], &[1], (3, 3)),
            19 => p(&[(3, 2), (2, 1), (6, 4), (6, 5)], &[1, 4, 5], (6, 6)),
            20 => p(&[(3, 1), (3, 2), (5, 4), (6, 4)], &[1, 2, 4], (3, 3)),
            21 => p(&[(3, 1), (3, 2), (6, 4), (6, 5)], &[1, 2, 4, 5], (3, 3)),
            22 => p(&[(2, 1), (6, 3), (6, 4), (6, 5)], &[1, 3, 4, 5], (6, 6)),
            23 => p(&[(3, 2), (6, 4), (6, 5)], &[1, 2, 4, 5], (6, 6)),
            24 => p(&[(6, 3), (6, 4), (6, 5)], &[1, 2, 3, 4, 5], (6, 6)),
            25 => p(&[(6, 4), (6, 5)], &[1, 2, 3, 4, 5], (6, 6)),
            26 => p(&[(3, 2), (5, 4), (6, 4)], &[1, 2, 4], (6, 6)),
            27 => p(&[(3, 2), (5, 4), (6, 4)], &[1, 2, 4], (3, 3)),
            28 => p(&[(2, 1), (5, 4), (4, 3)], &[1, 3, 6], (5, 5)),
            29 => p(&[(4, 3), (6, 5)], &[1, 2, 3, 5], (6, 6)),
            30 => p(&[(5, 4), (4, 3)], &[1, 2, 3, 6], (5, 5)),
            31 => p(&[(6, 5)], &[1, 2, 3, 4, 5], (6, 6)),
            32 => p(&[], &[1, 2, 3, 4, 5, 6], (6, 6)),
            _ => panic!("DX7 algorithms are numbered 1..=32, not {n}"),
        }
    }
}

/// One operator's settings.
#[derive(Debug, Clone, PartialEq)]
pub struct Operator {
    /// Frequency as a multiple of the note (ignored if `fixed_hz` is set).
    pub ratio: f32,
    /// A fixed frequency, independent of the note.
    pub fixed_hz: Option<f32>,
    /// Detune in cents.
    pub detune_cents: f32,
    /// Output level: carrier amplitude, or modulation index in radians.
    pub level: f32,
    /// The operator's envelope (with its own velocity response).
    pub envelope: Stages,
}

impl Operator {
    /// An operator at `ratio` and `level` with an organ-like envelope.
    #[must_use]
    pub fn new(ratio: f32, level: f32) -> Self {
        Self {
            ratio,
            fixed_hz: None,
            detune_cents: 0.0,
            level,
            envelope: Adsr::new(0.0, 0.0, 1.0, 0.0).stages(),
        }
    }

    /// Set the envelope.
    #[must_use]
    pub fn envelope(mut self, stages: Stages) -> Self {
        self.envelope = stages;
        self
    }

    /// Silent: level 0.
    #[must_use]
    pub fn off() -> Self {
        Self::new(1.0, 0.0)
    }
}

/// A full FM patch.
#[derive(Debug, Clone, PartialEq)]
pub struct Patch {
    /// The six operators, index 0 is operator 1.
    pub ops: [Operator; OPS],
    /// The routing.
    pub algorithm: Algorithm,
    /// Feedback amount (index in radians at full output).
    pub feedback: f32,
    /// Phase or frequency modulation.
    pub modulation: Modulation,
}

impl Patch {
    /// A two-operator patch on algorithm 5's first stack (2 → 1), every
    /// other operator silent — the textbook FM pair.
    #[must_use]
    pub fn pair(carrier_ratio: f32, modulator_ratio: f32, index: f32) -> Self {
        let mut ops: [Operator; OPS] = std::array::from_fn(|_| Operator::off());
        ops[0] = Operator::new(carrier_ratio, 1.0);
        ops[1] = Operator::new(modulator_ratio, index);
        Self {
            ops,
            algorithm: Algorithm::dx7(5),
            feedback: 0.0,
            modulation: Modulation::Phase,
        }
    }

    /// A DX7-style electric piano on algorithm 5.
    #[must_use]
    pub fn electric_piano() -> Self {
        use super::envelope::Curve;
        let decay = |t: f32, s: f32| {
            Adsr::new(0.001, t, s, 0.3)
                .with_curves(Curve::Linear, Curve::Exp(-5.0), Curve::Exp(-5.0))
                .velocity(super::envelope::Velocity {
                    to_level: 0.8,
                    to_attack: 0.0,
                })
        };
        let ops = [
            Operator::new(1.0, 0.7).envelope(decay(3.0, 0.0)),
            Operator::new(1.0, 1.6).envelope(decay(1.5, 0.0)),
            Operator::new(1.0, 0.5).envelope(decay(2.5, 0.0)),
            Operator::new(14.0, 0.6).envelope(decay(0.25, 0.0)),
            Operator::off(),
            Operator::off(),
        ];
        Self {
            ops,
            algorithm: Algorithm::dx7(5),
            feedback: 0.0,
            modulation: Modulation::Phase,
        }
    }
}

#[derive(Debug, Clone)]
struct OpState {
    phase: f64,
    inc: f64,
    env: EnvelopeGen,
    out: f64,
    prev: [f64; 2],
}

/// A playing FM voice.
#[derive(Debug, Clone)]
pub struct FmVoice {
    patch: Patch,
    ops: [OpState; OPS],
    rate: f64,
}

impl FmVoice {
    /// A voice for `patch` at `rate`.
    #[must_use]
    pub fn new(patch: Patch, rate: f32) -> Self {
        let ops = std::array::from_fn(|i| OpState {
            phase: 0.0,
            inc: 0.0,
            env: EnvelopeGen::new(patch.ops[i].envelope.clone(), rate),
            out: 0.0,
            prev: [0.0; 2],
        });
        let mut v = Self {
            patch,
            ops,
            rate: f64::from(rate),
        };
        v.set_frequency(440.0);
        v
    }

    /// Set the note frequency (fixed-frequency operators ignore it).
    pub fn set_frequency(&mut self, hz: f32) {
        for (o, p) in self.ops.iter_mut().zip(&self.patch.ops) {
            let f = p.fixed_hz.unwrap_or(hz * p.ratio) * 2f32.powf(p.detune_cents / 1200.0);
            o.inc = f64::from(f) / self.rate;
        }
    }

    /// Start a note.
    pub fn note_on(&mut self, velocity: f32) {
        for o in &mut self.ops {
            o.phase = 0.0;
            o.env.gate_on_velocity(velocity);
        }
    }

    /// Release.
    pub fn note_off(&mut self) {
        for o in &mut self.ops {
            o.env.gate_off();
        }
    }

    /// Whether any carrier is still sounding.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.ops
            .iter()
            .zip(self.patch.algorithm.carriers)
            .any(|(o, c)| c && o.env.is_active())
    }

    /// The next sample.
    pub fn tick(&mut self) -> f32 {
        use std::f64::consts::TAU;
        let alg = &self.patch.algorithm;
        let mut sum = 0.0;
        for i in (0..OPS).rev() {
            let mut pm = 0.0; // radians
            let mut fm = 0.0; // cycles/sample
            for &j in &alg.mods[i] {
                match self.patch.modulation {
                    Modulation::Phase => pm += self.ops[j].out,
                    Modulation::Frequency => fm += self.ops[j].out * self.ops[j].inc,
                }
            }
            if let Some((from, to)) = alg.feedback {
                if to == i && self.patch.feedback != 0.0 {
                    let prev = self.ops[from].prev;
                    pm += f64::from(self.patch.feedback) * (prev[0] + prev[1]) / 2.0;
                }
            }
            let level = f64::from(self.patch.ops[i].level);
            let env = f64::from(self.ops[i].env.next_level());
            let o = &mut self.ops[i];
            let out = level * env * (TAU * o.phase + pm).sin();
            o.prev = [out, o.prev[0]];
            o.out = out;
            o.phase = (o.phase + o.inc + fm).rem_euclid(1.0);
            if alg.carriers[i] {
                sum += out;
            }
        }
        #[expect(clippy::cast_possible_truncation, reason = "f32 audio")]
        let s = sum as f32;
        s
    }
}

impl super::Generator for FmVoice {
    fn next_sample(&mut self) -> f32 {
        self.tick()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::testing::{level_near, spectrum};
    use crate::synth::Generator;

    const RATE: f32 = 48_000.0;

    /// Bessel function of the first kind, by its power series.
    fn bessel_j(n: i32, x: f64) -> f64 {
        let mut sum = 0.0;
        let mut fact_m = 1.0;
        for m in 0..30 {
            if m > 0 {
                fact_m *= f64::from(m);
            }
            let mut fact_mn = 1.0;
            for k in 1..=(m + n) {
                fact_mn *= f64::from(k);
            }
            let sign = if m % 2 == 0 { 1.0 } else { -1.0 };
            sum += sign / (fact_m * fact_mn) * (x / 2.0).powi(2 * m + n);
        }
        sum
    }

    fn sidebands(modulation: Modulation, beta: f32) -> [f64; 3] {
        let mut patch = Patch::pair(8.0, 1.0, beta);
        patch.modulation = modulation;
        let mut v = FmVoice::new(patch, RATE);
        v.set_frequency(375.0);
        v.note_on(1.0);
        let x = v.render(16_384, 48_000).data;
        let (m, bin) = spectrum(&x, RATE);
        [
            level_near(&m, bin, 3000.0, 1),
            level_near(&m, bin, 3375.0, 1),
            level_near(&m, bin, 3750.0, 1),
        ]
    }

    #[test]
    fn phase_modulation_sidebands_follow_bessel() {
        let beta = 1.5;
        let [c, s1, s2] = sidebands(Modulation::Phase, beta);
        let b = f64::from(beta);
        assert!((c / 0.5 - bessel_j(0, b).abs()).abs() < 0.02, "J0 {c}");
        assert!((s1 / 0.5 - bessel_j(1, b).abs()).abs() < 0.02, "J1 {s1}");
        assert!((s2 / 0.5 - bessel_j(2, b).abs()).abs() < 0.02, "J2 {s2}");
    }

    #[test]
    fn frequency_modulation_has_the_same_magnitudes() {
        let p = sidebands(Modulation::Phase, 2.0);
        let f = sidebands(Modulation::Frequency, 2.0);
        for (a, b) in p.iter().zip(&f) {
            assert!((a - b).abs() < 0.02, "{p:?} vs {f:?}");
        }
    }

    #[test]
    fn zero_index_is_a_pure_sine() {
        let [c, s1, _] = sidebands(Modulation::Phase, 0.0);
        assert!((c - 0.5).abs() < 0.02 && s1 < 1e-3);
    }

    #[test]
    fn every_algorithm_is_well_formed() {
        for n in 1..=32 {
            let a = Algorithm::dx7(n);
            assert!(a.carriers[0], "operator 1 is always a carrier ({n})");
            for (t, srcs) in a.mods.iter().enumerate() {
                for &s in srcs {
                    assert!(s > t, "alg {n}: modulator {s} below target {t}");
                    assert!(!a.carriers[s], "alg {n}: op {} is both", s + 1);
                }
            }
            // Every non-carrier modulates something.
            for k in 0..OPS {
                if !a.carriers[k] {
                    assert!(
                        a.mods.iter().any(|m| m.contains(&k)),
                        "alg {n}: op {} idle",
                        k + 1
                    );
                }
            }
            let carriers = a.carriers.iter().filter(|c| **c).count();
            let expected = match n {
                1..=4 | 7..=15 => 2,
                16..=18 => 1,
                5 | 6 | 19 | 20 | 26..=28 => 3,
                21..=23 | 29 | 30 => 4,
                24 | 25 | 31 => 5,
                32 => 6,
                _ => unreachable!(),
            };
            assert_eq!(carriers, expected, "algorithm {n}");
        }
    }

    #[test]
    fn every_algorithm_renders_bounded() {
        for n in 1..=32 {
            let mut patch = Patch::pair(1.0, 1.0, 1.0);
            patch.algorithm = Algorithm::dx7(n);
            for (k, op) in patch.ops.iter_mut().enumerate() {
                #[expect(clippy::cast_precision_loss, reason = "test")]
                let r = (k + 1) as f32;
                *op = Operator::new(r, 0.5);
            }
            patch.feedback = 1.0;
            let mut v = FmVoice::new(patch, RATE);
            v.set_frequency(220.0);
            v.note_on(1.0);
            let x = v.render(4_800, 48_000).data;
            assert!(
                x.iter().all(|s| s.is_finite() && s.abs() <= 3.01),
                "alg {n}"
            );
        }
    }

    #[test]
    fn feedback_adds_harmonics() {
        let harmonic2 = |fb: f32| {
            let mut patch = Patch::pair(1.0, 1.0, 0.0);
            patch.algorithm = Algorithm::dx7(2); // feedback on op 2 → op 2
            patch.ops[1] = Operator::new(1.0, 1.0);
            patch.ops[0] = Operator::new(1.0, 0.0);
            patch.algorithm.carriers = [false, true, false, false, false, false];
            patch.feedback = fb;
            let mut v = FmVoice::new(patch, RATE);
            v.set_frequency(375.0);
            v.note_on(1.0);
            let x = v.render(16_384, 48_000).data;
            let (m, bin) = spectrum(&x, RATE);
            level_near(&m, bin, 750.0, 2) / level_near(&m, bin, 375.0, 2)
        };
        assert!(harmonic2(0.0) < 1e-3);
        assert!(harmonic2(1.2) > 0.1, "{}", harmonic2(1.2));
    }

    #[test]
    fn electric_piano_decays_and_is_velocity_sensitive() {
        let render = |vel: f32| {
            let mut v = FmVoice::new(Patch::electric_piano(), RATE);
            v.set_frequency(261.6);
            v.note_on(vel);
            v.render(48_000, 48_000).data
        };
        let loud = render(1.0);
        let soft = render(0.3);
        let peak = |x: &[f32]| x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak(&loud) > peak(&soft) * 1.5);
        assert!(peak(&loud[40_000..]) < peak(&loud[..4_000]) * 0.5, "decays");
    }
}
