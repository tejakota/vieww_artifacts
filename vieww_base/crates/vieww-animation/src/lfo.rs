//! The low-frequency oscillator — the LFO CHOP of TouchDesigner, the
//! `Math.sin(t * speed)` of every After Effects expression, the "make it
//! breathe" of every motion designer.
//!
//! # Why a waveform deserves a type
//!
//! The expression `amplitude * sin(tau * f * t)` is one line, so why not let
//! every caller write it? Because the line is only the *sine* case, and the
//! other four waveforms — square, triangle, saw, reverse-saw — are four more
//! lines each with their own edge cases (a square that is 51% high is visibly
//! lopsided at 0.5 Hz; a triangle that does not start at zero clicks). One
//! type with five spellings is one test suite instead of five copy-pastes,
//! and the waveforms a caller can *name* are the waveforms a caller *uses*.
//!
//! # The one rule
//!
//! Like everything in this crate, an LFO is a **function of time with no
//! state**: sampling it twice at the same timestamp gives the same value, and
//! sampling it out of order is as legal as in order. A breath that must sync
//! to a beat samples the shared clock, not a counter it increments itself.
//!
//! # As a `Simulation`
//!
//! [`Lfo`](struct.Lfo.html) implements [`Simulation`](crate::Simulation) so
//! it can drive an [`AnimationController`](crate::AnimationController) — with
//! one honest wrinkle: an oscillator never finishes, so
//! [`duration`](crate::Simulation::duration) reports "forever" and
//! [`is_done`](crate::Simulation::is_done) is always false. A controller left
//! on an LFO animates forever, which is the point, and worth saying out loud
//! so nobody debugs a "leak" that is a breathing panel.
//!
//! ```
//! use std::time::Duration;
//! use vieww_animation::lfo::{Lfo, Wave};
//!
//! let breath = Lfo::new(Wave::Sine, 0.4).amplitude(0.5).center(1.0);
//!
//! // A full period at 0.4 Hz is 2.5 s, so a quarter in is the crest.
//! let crest = breath.at(Duration::from_secs_f32(0.625));
//! assert!((crest - 1.5).abs() < 1e-3, "the top of the breath: {crest}");
//!
//! // And the valley, half a period later.
//! let valley = breath.at(Duration::from_secs_f32(1.875));
//! assert!((valley - 0.5).abs() < 1e-3, "the bottom: {valley}");
//! ```

use std::time::Duration;

use crate::simulation::Simulation;

/// The shape of one period.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wave {
    /// Smooth both ways. The breathing panel, the drifting cloud.
    Sine,
    /// Half high, half low, and the *rise is instantaneous*. Switches and
    /// clocks; anything that wants to look digital rather than organic.
    Square,
    /// Up then down, evenly. The pendulum readout, the level meter sweep.
    Triangle,
    /// Rising ramp that resets instantly. Sweep generators, radar sweeps,
    /// and the "loading" shimmer.
    Saw,
    /// Falling ramp that resets instantly. The saw, played backwards.
    ReverseSaw,
}

impl Wave {
    /// The waveform's value over one period, `turns` in `0..=1`.
    ///
    /// The public spelling of the maths each shape needs, so a caller with a
    /// clock of their own can share waveforms with this module without
    /// reconstructing them from scratch — and so the tests can pin each
    /// shape's half-period landmarks in one place.
    pub fn at(self, turns: f32) -> f32 {
        let t = turns.fract();
        match self {
            Wave::Sine => (std::f32::consts::TAU * t).sin(),
            // A strict 50/50 split: `t < 0.5` rather than `t <= 0.5` makes
            // exactly half the period high, and the choice of `<` is only
            // observable at the single sample that lands on the boundary —
            // where either answer is correct and only consistency matters.
            Wave::Square => {
                if t < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            // Two linear halves meeting at zero on the quarter — the
            // triangle's crease is at the *middle of each half*, not at the
            // period boundary, which is what makes it look like a pendulum
            // rather than a saw that got tired.
            Wave::Triangle => {
                if t < 0.25 {
                    4.0 * t
                } else if t < 0.75 {
                    2.0 - 4.0 * t
                } else {
                    4.0 * t - 4.0
                }
            }
            Wave::Saw => 2.0 * t - 1.0,
            Wave::ReverseSaw => 1.0 - 2.0 * t,
        }
    }
}

/// An oscillator: `center + amplitude × wave(2π·f·t + phase)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lfo {
    /// The shape of one period.
    pub wave: Wave,
    /// Cycles per second. The scale of the effect: 0.2 Hz breathes, 0.5 Hz
    /// drifts, 2 Hz is busy enough to read as a flicker.
    pub frequency: f32,
    /// Half the peak-to-peak swing, added to and subtracted from `center`.
    pub amplitude: f32,
    /// The value the oscillation is centred on — the opacity a pulse pulses
    /// around, the scale a hover grows and shrinks around.
    pub center: f32,
    /// A fraction of a period to start ahead by, in `0..1`.
    ///
    /// Phase is the difference between two things oscillating in step and
    /// two things alternating — the second is two LFOs a half-period apart,
    /// and no amount of frequency fiddling will fake it.
    pub phase: f32,
}

impl Lfo {
    /// A unit-amplitude, zero-centre oscillator at `frequency` Hz — the
    /// spelling that chains well.
    pub fn new(wave: Wave, frequency: f32) -> Self {
        Self {
            wave,
            frequency,
            amplitude: 1.0,
            center: 0.0,
            phase: 0.0,
        }
    }

    /// Set the half-swing.
    #[must_use]
    pub const fn amplitude(mut self, amplitude: f32) -> Self {
        self.amplitude = amplitude;
        self
    }

    /// Set what the swing is *around*.
    #[must_use]
    pub const fn center(mut self, center: f32) -> Self {
        self.center = center;
        self
    }

    /// Start a fraction of a period ahead. Values outside `0..1` wrap, which
    /// is the only honest answer for a fraction of a cycle.
    #[must_use]
    pub const fn phase(mut self, phase: f32) -> Self {
        self.phase = phase;
        self
    }

    /// The oscillator's value at `t`, in one line, for callers with their own
    /// clock — the same maths [`Simulation::position`](crate::Simulation::position)
    /// hands a controller.
    pub fn at(&self, t: Duration) -> f32 {
        let turns = t.as_secs_f32() * self.frequency + self.phase;
        self.center + self.amplitude * self.wave.at(turns)
    }
}

impl Simulation for Lfo {
    fn position(&self, elapsed: Duration) -> f32 {
        self.at(elapsed)
    }

    /// The analytic derivative, evaluated at the phase; the square and the
    /// saws are technically unbounded at their jumps, and zero is reported
    /// there rather than infinity — a caller using this to hand a velocity to
    /// a spring wants "not moving towards anything" at a discontinuity, not
    /// NaN.
    fn velocity_at(&self, elapsed: Duration) -> f32 {
        let turns = elapsed.as_secs_f32() * self.frequency + self.phase;
        let d_per_turn = match self.wave {
            Wave::Sine => std::f32::consts::TAU * (std::f32::consts::TAU * turns).cos(),
            Wave::Square => 0.0,
            Wave::Triangle => {
                let t = turns.fract();
                if (t - 0.25).abs() < 1e-6 || (t - 0.75).abs() < 1e-6 {
                    0.0
                } else if !(0.25..=0.75).contains(&t) {
                    4.0
                } else {
                    -4.0
                }
            }
            Wave::Saw => 2.0,
            Wave::ReverseSaw => -2.0,
        };
        self.amplitude * self.frequency * d_per_turn
    }

    /// Forever, and meant: an oscillator does not run its course. The
    /// controller that adopts one animates until it is swapped for something
    /// else, and the callers that find that surprising have found the
    /// documented behaviour.
    fn duration(&self) -> Duration {
        Duration::MAX
    }

    fn is_done(&self, _elapsed: Duration) -> bool {
        false
    }

    /// Oscillators have no resting place. This is the mean (`center`), the
    /// value a caller parking one wants to land on so the hand-off does not
    /// jump.
    fn final_position(&self) -> f32 {
        self.center
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-4;
    fn ms(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    #[test]
    fn sine_hits_both_extremes() {
        let lfo = Lfo::new(Wave::Sine, 1.0); // 1 Hz: period 1 s
        let crest = lfo.at(ms(250));
        let valley = lfo.at(ms(750));
        assert!((crest - 1.0).abs() < EPS, "crest {crest}");
        assert!((valley + 1.0).abs() < EPS, "valley {valley}");
    }

    #[test]
    fn centre_and_amplitude_rescale_the_swing() {
        let pulse = Lfo::new(Wave::Sine, 1.0).amplitude(0.25).center(0.75);
        assert!((pulse.at(ms(250)) - 1.0).abs() < EPS);
        assert!((pulse.at(ms(750)) - 0.5).abs() < EPS);
    }

    #[test]
    fn square_is_half_high_half_low() {
        let lfo = Lfo::new(Wave::Square, 1.0);
        assert_eq!(lfo.at(ms(100)), 1.0);
        assert_eq!(lfo.at(ms(600)), -1.0);
    }

    #[test]
    fn triangle_is_a_pendulum_not_a_saw() {
        let lfo = Lfo::new(Wave::Triangle, 1.0);
        // Starts at zero, crests at the quarter, back through zero at the
        // half, valley at three quarters. A saw would be *rising* at the
        // boundary; a triangle is *home*.
        assert!((lfo.at(ms(0)) - 0.0).abs() < EPS);
        assert!((lfo.at(ms(250)) - 1.0).abs() < EPS);
        assert!((lfo.at(ms(500)) - 0.0).abs() < EPS);
        assert!((lfo.at(ms(750)) + 1.0).abs() < EPS);
    }

    #[test]
    fn saws_are_mirror_images() {
        let up = Lfo::new(Wave::Saw, 1.0);
        let down = Lfo::new(Wave::ReverseSaw, 1.0);
        // Mid-period: the rising saw is at zero crossing up, the falling one
        // crossing down — both zero, opposite slopes.
        assert!((up.at(ms(500)) - 0.0).abs() < EPS);
        assert!((down.at(ms(500)) - 0.0).abs() < EPS);
        assert!(up.velocity_at(ms(500)) > 0.0);
        assert!(down.velocity_at(ms(500)) < 0.0);
    }

    #[test]
    fn phase_offsets_two_equal_lfos_into_alternation() {
        let a = Lfo::new(Wave::Sine, 1.0);
        let b = Lfo::new(Wave::Sine, 1.0).phase(0.5);
        // Where a is at its crest, b is at its valley — the alternation that
        // no frequency change reproduces.
        assert!((a.at(ms(250)) - 1.0).abs() < EPS);
        assert!((b.at(ms(250)) + 1.0).abs() < EPS);
    }

    #[test]
    fn frequency_scales_the_period() {
        let slow = Lfo::new(Wave::Sine, 0.5); // 2 s period
        // 0.5 s is a quarter of the slow period — its crest.
        assert!((slow.at(ms(500)) - 1.0).abs() < EPS);
    }

    #[test]
    fn sampling_out_of_order_is_legal() {
        let lfo = Lfo::new(Wave::Sine, 0.3).amplitude(2.0).center(4.0);
        let late = lfo.at(ms(2000));
        let early = lfo.at(ms(10));
        let late_again = lfo.at(ms(2000));
        assert_eq!(late, late_again);
        assert_ne!(late, early);
    }

    #[test]
    fn as_a_simulation_it_never_finishes() {
        let lfo = Lfo::new(Wave::Sine, 1.0);
        assert_eq!(Simulation::duration(&lfo), Duration::MAX);
        assert!(!Simulation::is_done(&lfo, Duration::from_secs(10_000)));
        // The parking value is the mean, so a hand-off lands smoothly.
        assert_eq!(Simulation::final_position(&lfo), 0.0);
        // And the controller path answers the same maths.
        let via_trait = Simulation::position(&lfo, ms(250));
        assert!((via_trait - 1.0).abs() < EPS);
    }

    #[test]
    fn sine_velocity_is_the_cosine() {
        let lfo = Lfo::new(Wave::Sine, 1.0).amplitude(2.0);
        // Crest: velocity zero. Zero crossing: velocity maximum.
        assert!(lfo.velocity_at(ms(250)).abs() < EPS);
        assert!((lfo.velocity_at(ms(0)) - 2.0 * std::f32::consts::TAU).abs() < 1e-3);
    }

    #[test]
    fn wave_shapes_are_pinned_at_their_landmarks() {
        let cases: [(Wave, f32, f32); 5] = [
            (Wave::Sine, 0.25, 1.0),
            (Wave::Square, 0.25, 1.0),
            (Wave::Triangle, 0.25, 1.0),
            (Wave::Saw, 0.25, -0.5),
            (Wave::ReverseSaw, 0.25, 0.5),
        ];
        for (wave, t, want) in cases {
            assert!((wave.at(t) - want).abs() < EPS, "{wave:?} at {t}");
        }
    }
}
