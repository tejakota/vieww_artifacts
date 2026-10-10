//! Band-limited oscillators, pulse-width modulation, and oscillator sync.
//!
//! The crate root's [`Waveform`](crate::Waveform) is *naive*: a saw is a
//! straight line that jumps, and a jump contains every frequency at once.
//! Above a few hundred hertz the harmonics the jump implies pass the Nyquist
//! rate and fold back down as inharmonic tones — aliasing, the metallic
//! whine of a cheap synth. That is fine for a 5 ms click and wrong for a
//! held note.
//!
//! [`Oscillator`] removes it with PolyBLEP (a polynomial band-limited step
//! subtracted at every jump) for saw, square and pulse, and PolyBLAMP (the
//! integrated version, for every *kink*) for the triangle. Both are two-sample
//! corrections: cheap, stateless across cycles, and good to roughly −60 dB of
//! alias suppression across the musical range — the technique Välimäki and
//! Huovilainen describe and every virtual-analog synth since has used.
//!
//! [`Sync`] pairs two oscillators: a *master* whose every cycle restarts the
//! *slave* (hard sync — the tearing lead sound), or reverses it (soft sync).
//! The restart is a jump of its own, and it is corrected with the same
//! PolyBLEP, placed at the sub-sample instant the master wrapped.

/// An oscillator shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    /// A sine — no harmonics, nothing to band-limit.
    Sine,
    /// Falling saw from +1 to −1, every harmonic at `1/n`.
    Saw,
    /// Square — a pulse at 50% duty, odd harmonics.
    Square,
    /// Pulse at the given duty cycle in `0.01..=0.99`.
    Pulse(f32),
    /// Triangle, odd harmonics at `1/n²`.
    Triangle,
}

/// A PolyBLEP/BLAMP band-limited oscillator.
#[derive(Debug, Clone)]
pub struct Oscillator {
    shape: Shape,
    rate: f64,
    phase: f64,
    inc: f64,
    direction: f64,
}

/// The two-sample polynomial band-limited step residual for a *falling*
/// unit-height (2.0 peak-to-peak) discontinuity at phase 0.
fn blep(t: f64, dt: f64) -> f64 {
    if dt <= 0.0 {
        return 0.0;
    }
    if t < dt {
        let x = t / dt;
        2.0 * x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + 2.0 * x + 1.0
    } else {
        0.0
    }
}

/// The integrated residual — for a slope discontinuity at phase 0.
fn blamp(t: f64, dt: f64) -> f64 {
    if dt <= 0.0 {
        return 0.0;
    }
    if t < dt {
        let x = t / dt - 1.0;
        -x * x * x / 3.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt + 1.0;
        x * x * x / 3.0
    } else {
        0.0
    }
}

impl Oscillator {
    /// An oscillator at `hz`, rendering at `rate`.
    #[must_use]
    pub fn new(shape: Shape, hz: f32, rate: f32) -> Self {
        let mut o = Self {
            shape,
            rate: f64::from(rate),
            phase: 0.0,
            inc: 0.0,
            direction: 1.0,
        };
        o.set_frequency(hz);
        o
    }

    /// Change pitch without resetting phase (so glides are continuous).
    pub fn set_frequency(&mut self, hz: f32) {
        self.inc = (f64::from(hz) / self.rate).clamp(0.0, 0.5);
    }

    /// The current frequency in Hz.
    #[must_use]
    #[expect(clippy::cast_possible_truncation, reason = "audio frequencies fit f32")]
    pub fn frequency(&self) -> f32 {
        (self.inc * self.rate) as f32
    }

    /// Change shape (pulse width modulation is `set_shape(Shape::Pulse(w))`).
    pub const fn set_shape(&mut self, shape: Shape) {
        self.shape = shape;
    }

    /// Set the phase in turns, `0..1`.
    pub fn set_phase(&mut self, turns: f64) {
        self.phase = turns.rem_euclid(1.0);
    }

    /// The phase in turns.
    #[must_use]
    pub const fn phase(&self) -> f64 {
        self.phase
    }

    /// The per-sample phase increment.
    #[must_use]
    pub const fn increment(&self) -> f64 {
        self.inc
    }

    /// The naive (un-band-limited) value at phase `t`.
    fn naive(&self, t: f64) -> f64 {
        match self.shape {
            Shape::Sine => (std::f64::consts::TAU * t).sin(),
            Shape::Saw => 1.0 - 2.0 * t,
            Shape::Square => {
                if t < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            Shape::Pulse(w) => {
                if t < f64::from(w.clamp(0.01, 0.99)) {
                    1.0
                } else {
                    -1.0
                }
            }
            Shape::Triangle => 4.0 * (t - 0.5).abs() - 1.0,
        }
    }

    /// The band-limited value at the current phase.
    fn value(&self) -> f64 {
        let t = self.phase;
        let dt = self.inc;
        let v = self.naive(t);
        match self.shape {
            Shape::Sine => v,
            // The saw rises back from −1 to +1 at t=0: an upward step of 2.
            Shape::Saw => v + blep(t, dt),
            Shape::Square => v + blep(t, dt) - blep((t + 0.5).rem_euclid(1.0), dt),
            Shape::Pulse(w) => {
                let w = f64::from(w.clamp(0.01, 0.99));
                v + blep(t, dt) - blep((t + 1.0 - w).rem_euclid(1.0), dt)
            }
            // Triangle: the slope changes by +8 per cycle at its minimum
            // (t = 0.5) and by −8 at its maximum (t = 0). A BLAMP residual
            // for a slope change Δ is Δ·dt/2 × `blamp`.
            Shape::Triangle => v + 4.0 * dt * (blamp((t + 0.5).rem_euclid(1.0), dt) - blamp(t, dt)),
        }
    }

    /// Advance one sample, returning the value.
    #[expect(clippy::cast_possible_truncation, reason = "output is f32 audio")]
    pub fn tick(&mut self) -> f32 {
        let v = self.value();
        self.advance();
        v as f32
    }

    /// Advance one sample with a phase offset (phase modulation, in turns).
    #[expect(clippy::cast_possible_truncation, reason = "output is f32 audio")]
    pub fn next_pm(&mut self, offset_turns: f64) -> f32 {
        let saved = self.phase;
        self.phase = (self.phase + offset_turns).rem_euclid(1.0);
        let v = self.value();
        self.phase = saved;
        self.advance();
        v as f32
    }

    fn advance(&mut self) {
        self.phase += self.inc * self.direction;
        self.phase = self.phase.rem_euclid(1.0);
    }
}

impl super::Generator for Oscillator {
    fn next_sample(&mut self) -> f32 {
        self.tick()
    }
}

/// How a master restarts its slave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncMode {
    /// Reset the slave's phase to 0 on every master cycle.
    Hard,
    /// Reverse the slave's direction on every master cycle.
    Soft,
}

/// Two oscillators in sync: the slave is heard; the master only clocks it.
///
/// Output is one sample late, because a PolyBLEP for a jump between two
/// samples has to correct the sample *before* the jump too.
#[derive(Debug, Clone)]
pub struct Sync {
    /// The heard oscillator — its pitch is the *timbre* knob.
    pub slave: Oscillator,
    master_phase: f64,
    master_inc: f64,
    mode: SyncMode,
    pending: f64,
}

impl Sync {
    /// A synced pair: `master_hz` sets the pitch heard, `slave` the timbre.
    #[must_use]
    pub fn new(slave: Oscillator, master_hz: f32, mode: SyncMode) -> Self {
        let master_inc = (f64::from(master_hz) / slave.rate).clamp(0.0, 0.5);
        Self {
            slave,
            master_phase: 0.0,
            master_inc,
            mode,
            pending: 0.0,
        }
    }

    /// Change the master pitch.
    pub fn set_master_frequency(&mut self, hz: f32) {
        self.master_inc = (f64::from(hz) / self.slave.rate).clamp(0.0, 0.5);
    }

    /// The next sample.
    #[expect(clippy::cast_possible_truncation, reason = "output is f32 audio")]
    pub fn tick(&mut self) -> f32 {
        self.master_phase += self.master_inc;
        let wrapped = self.master_phase >= 1.0;
        let mut current;
        let mut correction_prev = 0.0;
        if wrapped {
            self.master_phase -= 1.0;
            // `d`: how far past the wrap (in samples) this sample sits.
            let d = if self.master_inc > 0.0 {
                (self.master_phase / self.master_inc).clamp(0.0, 1.0)
            } else {
                0.0
            };
            match self.mode {
                SyncMode::Hard => {
                    let slave_inc = self.slave.inc;
                    let before_phase = (self.slave.phase + slave_inc * (1.0 - d)).rem_euclid(1.0);
                    let before = self.slave.naive(before_phase);
                    let after = self.slave.naive(0.0);
                    let jump = after - before;
                    self.slave.phase = (slave_inc * d).rem_euclid(1.0);
                    current = self.slave.value();
                    // PolyBLEP split across the two samples around the jump.
                    correction_prev = jump / 2.0 * d * d;
                    current -= jump / 2.0 * (1.0 - d) * (1.0 - d);
                    self.slave.phase = (self.slave.phase + slave_inc).rem_euclid(1.0);
                }
                SyncMode::Soft => {
                    self.slave.direction = -self.slave.direction;
                    current = self.slave.value();
                    self.slave.advance();
                }
            }
        } else {
            current = self.slave.value();
            self.slave.advance();
        }
        let out = self.pending + correction_prev;
        self.pending = current;
        out as f32
    }
}

impl super::Generator for Sync {
    fn next_sample(&mut self) -> f32 {
        self.tick()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::testing::{level_near, rms, spectrum};

    const RATE: f32 = 48_000.0;

    fn render(osc: &mut impl FnMut() -> f32, n: usize) -> Vec<f32> {
        (0..n).map(|_| osc()).collect()
    }

    /// Energy between harmonics — alias components land off the harmonic
    /// grid, so this is a direct alias measure.
    fn inharmonic_energy(x: &[f32], f0: f64) -> f64 {
        let (mags, bin) = spectrum(x, RATE);
        let mut e = 0.0;
        for (k, m) in mags.iter().enumerate() {
            #[expect(clippy::cast_precision_loss, reason = "test")]
            let hz = k as f64 * bin;
            let nearest = (hz / f0).round() * f0;
            if (hz - nearest).abs() > 4.0 * bin && hz > 100.0 {
                e += m * m;
            }
        }
        e
    }

    fn naive(shape: Shape, hz: f32, n: usize) -> Vec<f32> {
        let o = Oscillator::new(shape, hz, RATE);
        let mut t = 0.0f64;
        (0..n)
            .map(|_| {
                #[expect(clippy::cast_possible_truncation, reason = "test")]
                let v = o.naive(t) as f32;
                t = (t + o.inc).rem_euclid(1.0);
                v
            })
            .collect()
    }

    #[test]
    fn band_limited_saw_aliases_far_less_than_naive() {
        let f = 3_517.0;
        let mut o = Oscillator::new(Shape::Saw, f, RATE);
        let bl = render(&mut || o.tick(), 16_384);
        let nv = naive(Shape::Saw, f, 16_384);
        let ratio = inharmonic_energy(&bl, f64::from(f)) / inharmonic_energy(&nv, f64::from(f));
        assert!(ratio < 0.1, "alias energy ratio {ratio}");
    }

    #[test]
    fn band_limited_square_and_pulse_alias_less() {
        for shape in [Shape::Square, Shape::Pulse(0.3)] {
            let f = 2_903.0;
            let mut o = Oscillator::new(shape, f, RATE);
            let bl = render(&mut || o.tick(), 16_384);
            let nv = naive(shape, f, 16_384);
            let ratio = inharmonic_energy(&bl, f64::from(f)) / inharmonic_energy(&nv, f64::from(f));
            assert!(ratio < 0.15, "{shape:?}: {ratio}");
        }
    }

    #[test]
    fn band_limited_triangle_aliases_less() {
        let f = 5_011.0;
        let mut o = Oscillator::new(Shape::Triangle, f, RATE);
        let bl = render(&mut || o.tick(), 16_384);
        let nv = naive(Shape::Triangle, f, 16_384);
        let ratio = inharmonic_energy(&bl, f64::from(f)) / inharmonic_energy(&nv, f64::from(f));
        assert!(ratio < 0.5, "triangle alias ratio {ratio}");
    }

    #[test]
    fn saw_harmonics_fall_as_one_over_n() {
        let f = 375.0; // exact bins at 48k / 16384 * k? close enough with window
        let mut o = Oscillator::new(Shape::Saw, f, RATE);
        let x = render(&mut || o.tick(), 16_384);
        let (mags, bin) = spectrum(&x, RATE);
        let h1 = level_near(&mags, bin, 375.0, 2);
        let h2 = level_near(&mags, bin, 750.0, 2);
        let h3 = level_near(&mags, bin, 1125.0, 2);
        assert!((h1 / h2 - 2.0).abs() < 0.2, "{h1} {h2}");
        assert!((h1 / h3 - 3.0).abs() < 0.3, "{h1} {h3}");
    }

    #[test]
    fn square_has_only_odd_harmonics() {
        let f = 375.0;
        let mut o = Oscillator::new(Shape::Square, f, RATE);
        let x = render(&mut || o.tick(), 16_384);
        let (mags, bin) = spectrum(&x, RATE);
        let h1 = level_near(&mags, bin, 375.0, 2);
        let h2 = level_near(&mags, bin, 750.0, 2);
        let h3 = level_near(&mags, bin, 1125.0, 2);
        assert!(h2 < h1 * 0.01, "even harmonic present: {h2}");
        assert!((h1 / h3 - 3.0).abs() < 0.3);
    }

    #[test]
    fn outputs_stay_in_range_and_centered() {
        for shape in [
            Shape::Sine,
            Shape::Saw,
            Shape::Square,
            Shape::Pulse(0.5),
            Shape::Triangle,
        ] {
            let mut o = Oscillator::new(shape, 440.0, RATE);
            let x = render(&mut || o.tick(), 48_000);
            assert!(x.iter().all(|v| v.abs() <= 1.2), "{shape:?}");
            let mean: f64 = x.iter().map(|v| f64::from(*v)).sum::<f64>() / 48_000.0;
            assert!(mean.abs() < 0.01, "{shape:?} DC {mean}");
            assert!(rms(&x) > 0.4, "{shape:?}");
        }
    }

    #[test]
    fn hard_sync_follows_the_master_pitch() {
        let slave = Oscillator::new(Shape::Saw, 1_234.0, RATE);
        let mut s = Sync::new(slave, 300.0, SyncMode::Hard);
        let x = render(&mut || s.tick(), 32_768);
        // Hard sync makes the output periodic at the master frequency.
        let (mags, bin) = spectrum(&x, RATE);
        let on_grid = level_near(&mags, bin, 1200.0, 2);
        let off_grid = level_near(&mags, bin, 1234.0 + 60.0, 1);
        assert!(on_grid > 10.0 * off_grid, "{on_grid} vs {off_grid}");
        let per = (f64::from(RATE) / 300.0).round() as usize;
        let diff: f64 = (1000..3000)
            .map(|i| f64::from((x[i] - x[i + per]).abs()))
            .sum::<f64>()
            / 2000.0;
        assert!(diff < 0.05, "not periodic at the master: {diff}");
    }

    #[test]
    fn hard_sync_is_band_limited_better_than_naive_reset() {
        // Compare with a naive reset of a naive saw.
        let f_m = 2_311.0;
        let f_s = 5_333.0;
        let slave = Oscillator::new(Shape::Saw, f_s, RATE);
        let mut s = Sync::new(slave, f_m, SyncMode::Hard);
        let bl = render(&mut || s.tick(), 16_384);
        let mut mp = 0.0f64;
        let mut sp = 0.0f64;
        let mi = f64::from(f_m / RATE);
        let si = f64::from(f_s / RATE);
        #[expect(clippy::cast_possible_truncation, reason = "test")]
        let nv: Vec<f32> = (0..16_384)
            .map(|_| {
                let v = (1.0 - 2.0 * sp) as f32;
                mp += mi;
                sp = (sp + si).rem_euclid(1.0);
                if mp >= 1.0 {
                    mp -= 1.0;
                    sp = 0.0;
                }
                v
            })
            .collect();
        let ratio = inharmonic_energy(&bl, f64::from(f_m)) / inharmonic_energy(&nv, f64::from(f_m));
        assert!(ratio < 0.5, "sync alias ratio {ratio}");
    }

    #[test]
    fn soft_sync_runs_and_is_bounded() {
        let slave = Oscillator::new(Shape::Triangle, 700.0, RATE);
        let mut s = Sync::new(slave, 220.0, SyncMode::Soft);
        let x = render(&mut || s.tick(), 4_800);
        assert!(x.iter().all(|v| v.is_finite() && v.abs() <= 1.2));
        assert!(rms(&x) > 0.2);
    }

    #[test]
    fn frequency_changes_keep_phase() {
        let mut o = Oscillator::new(Shape::Sine, 440.0, RATE);
        for _ in 0..100 {
            o.tick();
        }
        let p = o.phase();
        o.set_frequency(880.0);
        assert!((o.phase() - p).abs() < 1e-12);
        assert!((o.frequency() - 880.0).abs() < 0.01);
    }
}
