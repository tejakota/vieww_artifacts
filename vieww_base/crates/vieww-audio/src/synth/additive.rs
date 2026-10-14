//! Additive synthesis — a sound as a sum of sine partials, each with its own
//! frequency ratio, amplitude and envelope.
//!
//! This is how bells, organs and the "evolving pad" of a Kawai K5 or a
//! Harmor patch are built: a bell's partials are *inharmonic* (ratios like
//! 2.76 and 5.40, not 2 and 3) and the high ones die first, which is
//! exactly two fields per partial here. Partials whose frequency would pass
//! Nyquist are skipped, so additive synthesis is band-limited by
//! construction — the property that makes it the reference every other
//! oscillator is compared against.

use super::envelope::{EnvelopeGen, Stages};

/// One sine component.
#[derive(Debug, Clone)]
pub struct Partial {
    /// Frequency as a multiple of the note's fundamental (`2.0` = octave).
    pub ratio: f32,
    /// Fixed offset in Hz added after the ratio (beating, chorus-like detune).
    pub offset_hz: f32,
    /// Peak amplitude.
    pub amplitude: f32,
    /// Starting phase in turns.
    pub phase: f32,
    /// This partial's own envelope (`None`: constant at `amplitude`).
    pub envelope: Option<Stages>,
}

impl Partial {
    /// A harmonic partial at `ratio`, constant amplitude.
    #[must_use]
    pub const fn new(ratio: f32, amplitude: f32) -> Self {
        Self {
            ratio,
            offset_hz: 0.0,
            amplitude,
            phase: 0.0,
            envelope: None,
        }
    }

    /// Give the partial an envelope.
    #[must_use]
    pub fn envelope(mut self, stages: Stages) -> Self {
        self.envelope = Some(stages);
        self
    }
}

#[derive(Debug, Clone)]
struct Running {
    phase: f64,
    inc: f64,
    amplitude: f32,
    env: Option<EnvelopeGen>,
    audible: bool,
}

/// An additive voice: partials over one fundamental.
#[derive(Debug, Clone)]
pub struct Additive {
    partials: Vec<Partial>,
    running: Vec<Running>,
    rate: f32,
    fundamental: f32,
}

impl Additive {
    /// A voice over `partials` at `rate`.
    #[must_use]
    pub fn new(partials: Vec<Partial>, rate: f32) -> Self {
        let running = partials
            .iter()
            .map(|p| Running {
                phase: f64::from(p.phase),
                inc: 0.0,
                amplitude: p.amplitude,
                env: p.envelope.clone().map(|s| EnvelopeGen::new(s, rate)),
                audible: true,
            })
            .collect();
        let mut a = Self {
            partials,
            running,
            rate,
            fundamental: 0.0,
        };
        a.set_frequency(440.0);
        a
    }

    /// The classic church-bell spectrum (Hibberd's measured partials), each
    /// decaying faster the higher it sits.
    #[must_use]
    pub fn bell(rate: f32) -> Self {
        use super::envelope::{Curve, Stage};
        let table = [
            (0.5, 0.6, 6.0), // hum
            (1.0, 1.0, 4.0), // prime
            (1.2, 0.5, 3.5), // tierce (minor third)
            (1.5, 0.4, 3.0), // quint
            (2.0, 0.35, 2.5),
            (2.5, 0.25, 2.0),
            (2.66, 0.2, 1.6),
            (3.0, 0.15, 1.2),
            (4.0, 0.1, 0.9),
        ];
        let partials = table
            .iter()
            .map(|&(r, a, d)| {
                Partial::new(r, a).envelope(Stages::one_shot(vec![
                    Stage::linear(1.0, 0.002),
                    Stage::curved(0.0, d, Curve::Exp(-6.0)),
                ]))
            })
            .collect();
        Self::new(partials, rate)
    }

    /// A drawbar organ: the nine Hammond footages at the given drawbar
    /// settings (0..=8 each).
    #[must_use]
    pub fn drawbars(settings: [u8; 9], rate: f32) -> Self {
        let ratios = [0.5, 1.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0];
        let partials = ratios
            .iter()
            .zip(settings)
            .map(|(r, s)| Partial::new(*r, f32::from(s.min(8)) / 8.0 / 3.0))
            .collect();
        Self::new(partials, rate)
    }

    /// Set the fundamental, phase-continuously.
    pub fn set_frequency(&mut self, hz: f32) {
        self.fundamental = hz;
        let nyq = self.rate / 2.0;
        for (p, r) in self.partials.iter().zip(&mut self.running) {
            let f = hz * p.ratio + p.offset_hz;
            r.audible = f > 0.0 && f < nyq;
            r.inc = f64::from(f) / f64::from(self.rate);
        }
    }

    /// Start every partial's envelope.
    pub fn note_on(&mut self, velocity: f32) {
        for r in &mut self.running {
            if let Some(e) = &mut r.env {
                e.gate_on_velocity(velocity);
            }
        }
    }

    /// Release every partial's envelope.
    pub fn note_off(&mut self) {
        for r in &mut self.running {
            if let Some(e) = &mut r.env {
                e.gate_off();
            }
        }
    }

    /// Whether any enveloped partial is still sounding (constant partials
    /// count as sounding).
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.running
            .iter()
            .any(|r| r.env.as_ref().is_none_or(EnvelopeGen::is_active))
    }

    /// The number of partials currently below Nyquist.
    #[must_use]
    pub fn audible_partials(&self) -> usize {
        self.running.iter().filter(|r| r.audible).count()
    }

    /// The next sample.
    pub fn tick(&mut self) -> f32 {
        let mut sum = 0.0f64;
        for r in &mut self.running {
            let env = r.env.as_mut().map_or(1.0, EnvelopeGen::next_level);
            if r.audible {
                sum += f64::from(r.amplitude * env) * (std::f64::consts::TAU * r.phase).sin();
            }
            r.phase = (r.phase + r.inc).rem_euclid(1.0);
        }
        #[expect(clippy::cast_possible_truncation, reason = "f32 audio")]
        let out = sum as f32;
        out
    }
}

impl super::Generator for Additive {
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

    #[test]
    fn partials_appear_at_their_ratios_and_amplitudes() {
        let mut a = Additive::new(vec![Partial::new(1.0, 1.0), Partial::new(2.76, 0.5)], RATE);
        a.set_frequency(375.0);
        let x = a.render(16_384, 48_000).data;
        let (m, bin) = spectrum(&x, RATE);
        let f = level_near(&m, bin, 375.0, 2);
        let p = level_near(&m, bin, 375.0 * 2.76, 2);
        assert!((p / f - 0.5).abs() < 0.05, "{}", p / f);
        assert!((f - 0.5).abs() < 0.05, "full-scale sine reads 0.5: {f}");
    }

    #[test]
    fn partials_above_nyquist_are_dropped() {
        let mut a = Additive::new(vec![Partial::new(1.0, 1.0), Partial::new(10.0, 1.0)], RATE);
        a.set_frequency(3_000.0);
        assert_eq!(a.audible_partials(), 1);
        let x = a.render(16_384, 48_000).data;
        let (m, bin) = spectrum(&x, RATE);
        // 30 kHz would alias to 18 kHz.
        assert!(level_near(&m, bin, 18_000.0, 3) < 1e-4);
    }

    #[test]
    fn bell_high_partials_die_first() {
        let mut b = Additive::bell(RATE);
        b.set_frequency(400.0);
        b.note_on(1.0);
        let x = b.render(48_000 * 2, 48_000).data;
        let early = &x[2_000..2_000 + 16_384];
        let late = &x[60_000..60_000 + 16_384];
        let (me, bin) = spectrum(early, RATE);
        let (ml, _) = spectrum(late, RATE);
        let ratio_e = level_near(&me, bin, 1600.0, 2) / level_near(&me, bin, 400.0, 2);
        let ratio_l = level_near(&ml, bin, 1600.0, 2) / level_near(&ml, bin, 400.0, 2);
        assert!(ratio_l < ratio_e * 0.5, "{ratio_e} -> {ratio_l}");
    }

    #[test]
    fn drawbars_888000000_is_three_partials() {
        let mut o = Additive::drawbars([8, 8, 8, 0, 0, 0, 0, 0, 0], RATE);
        o.set_frequency(375.0);
        let x = o.render(16_384, 48_000).data;
        let (m, bin) = spectrum(&x, RATE);
        for hz in [187.5, 375.0, 562.5] {
            assert!(level_near(&m, bin, hz, 2) > 0.05, "{hz}");
        }
        assert!(level_near(&m, bin, 750.0, 2) < 1e-3);
    }
}
