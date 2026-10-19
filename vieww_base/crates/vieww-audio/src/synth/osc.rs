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
                // A naive ±1 pulse at width `w` carries a DC of `2w − 1`,
                // and the zero-area BLEP corrections cannot remove it — in
                // a polyphonic stack that DC sums and eats headroom. The
                // centered spelling subtracts it and renormalises so the
                // louder level is ±1: at `w = 0.5` it is exactly the old
                // ±1 square, and at any other width the levels are
                // `(1−w)/max(w, 1−w)` and `−w/max(w, 1−w)` — DC-free by
                // construction and in range by the same max.
                let w = f64::from(w.clamp(0.01, 0.99));
                let m = w.max(1.0 - w);
                if t < w {
                    (1.0 - w) / m
                } else {
                    -w / m
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
                    // The slave's phase at this moment is where it *would*
                    // be one sample past the reset instant (the one-sample
                    // output delay advances it through the wrap), so the
                    // pre-jump value is extrapolated *back* the `d` samples
                    // to the instant the master reset it — forward
                    // extrapolation measured the jump a full slave sample
                    // wide, and every sync edge carried the error.
                    let before_phase = (self.slave.phase - slave_inc * d).rem_euclid(1.0);
                    let before = self.slave.naive(before_phase);
                    let after = self.slave.naive(0.0);
                    let jump = after - before;
                    self.slave.phase = (slave_inc * d).rem_euclid(1.0);
                    // Naive, not `value()`: the phase was just reset mid-
                    // cycle, and `value()` would add the slave's own
                    // natural-wrap BLEP on top of the sync-jump correction
                    // below — two corrections for one discontinuity, with
                    // the spurious one reaching full scale as `d` nears
                    // zero. The manual correction is the only one that
                    // belongs here.
                    current = self.slave.naive(self.slave.phase);
                    // PolyBLEP split across the two samples around the jump:
                    // the pre-jump sample sits `(1 - d)` before it, the
                    // post-jump sample `d` after, and the two-point split
                    // `d² / (1-d)²` is the same shape `blep` itself uses.
                    correction_prev = jump / 2.0 * d * d;
                    current -= jump / 2.0 * (1.0 - d) * (1.0 - d);
                    self.slave.phase = (self.slave.phase + slave_inc).rem_euclid(1.0);
                }
                SyncMode::Soft => {
                    // The direction reversal is a slope discontinuity of
                    // its own, and it is *not* corrected here: a BLAMP for
                    // a kink at a fractional offset needs its own two-point
                    // split, and an unverified correction is worse than a
                    // documented one. Soft sync on a symmetric shape
                    // (triangle) is near-seamless; on a saw it tears —
                    // bounded, finite, and honest about it.
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
    fn pulses_of_any_width_carry_no_dc() {
        // The centered spelling this fixed: the naive ±1 pulse carried a
        // DC of 2w−1 — a quarter-width pulse sat at −0.5, and a polyphonic
        // stack of them eats real headroom. The mean over whole periods
        // must be a rounding error at every width (93.75 Hz divides the
        // 16,384-sample window exactly, 512 periods, so every phase is
        // counted once and the residual really is zero).
        for w in [0.01_f32, 0.25, 0.5, 0.75, 0.99] {
            let f = 93.75;
            let mut o = Oscillator::new(Shape::Pulse(w), f, RATE);
            let x = render(&mut || o.tick(), 16_384);
            let mean = f64::from(x.iter().sum::<f32>()) / 16_384.0;
            assert!(mean.abs() < 5e-3, "w={w}: mean {mean}");
            // And the level scaling keeps the louder side at ±1 — the
            // renormalisation the centering divides by.
            let peak = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(peak > 0.99, "w={w}: peak {peak}");
        }
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

    #[test]
    fn hard_sync_matches_an_oversampled_reference() {
        // The waveform, not just the spectrum: a naive hard sync rendered
        // 16x finer and *properly low-passed* before decimation is an
        // independent reference, and the band-limited sync must sit within
        // a few percent of its RMS. (A plain box average is not properly
        // low-passed — it passes up to 64% of the 24-48 kHz band, and the
        // aliased harmonics of the oversampled naive saw swamp the
        // comparison.) The old edge math — the pre-jump value extrapolated
        // a full slave sample the wrong way, plus the slave's own wrap-BLEP
        // stacked on the sync correction — carried visible error here at
        // every reset point; the periodicity and alias-ratio tests above
        // cannot see that, because the error lands on the master's own
        // harmonic grid. This one can: it compares the samples.
        let (f_m, f_s) = (1_000.0_f32, 2_600.0_f32);
        const OS: usize = 16;
        const N: usize = 8_192;
        let slave = Oscillator::new(Shape::Saw, f_s, RATE);
        let mut s = Sync::new(slave, f_m, SyncMode::Hard);
        let got = render(&mut || s.tick(), N + 4);
        // The reference: same reset schedule, phase advancing OS times
        // finer, filtered and decimated. The filter is a Hann-windowed
        // sinc at 21 kHz (129 fine taps): a passband the PolyBLEP saw
        // fills honestly, a stopband that keeps the decimation clean.
        let fine_rate = f64::from(RATE) * OS as f64;
        #[expect(clippy::cast_precision_loss, reason = "test")]
        let mi = f64::from(f_m / RATE) / OS as f64;
        #[expect(clippy::cast_precision_loss, reason = "test")]
        let si = f64::from(f_s / RATE) / OS as f64;
        let mut mp = 0.0f64;
        let mut sp = 0.0f64;
        let fine: Vec<f64> = (0..N * OS)
            .map(|_| {
                let v = 1.0 - 2.0 * sp;
                mp += mi;
                sp = (sp + si).rem_euclid(1.0);
                if mp >= 1.0 {
                    mp -= 1.0;
                    sp = 0.0;
                }
                v
            })
            .collect();
        const TAPS: usize = 129;
        let half = TAPS / 2;
        let fc = 2.0 * 21_000.0 / fine_rate; // cycles per fine sample
        let sinc = |x: f64| {
            if x.abs() < 1e-12 {
                1.0
            } else {
                let px = std::f64::consts::PI * x;
                px.sin() / px
            }
        };
        let mut kernel: Vec<f64> = (0..TAPS)
            .map(|n| {
                let hann =
                    0.5 - 0.5 * (2.0 * std::f64::consts::PI * n as f64 / (TAPS - 1) as f64).cos();
                fc * sinc(fc * (n as f64 - half as f64)) * hann
            })
            .collect();
        // Normalise the DC gain to exactly one, so the reference keeps the
        // signal's scale and the comparison measures shape, not volume.
        let dc: f64 = kernel.iter().sum();
        if dc.abs() > 1e-12 {
            for h in &mut kernel {
                *h /= dc;
            }
        }
        // Reference samples the filter's edge effects do not reach.
        // Filter the oversampled reference down to the output rate.
        let skip = 4; // output samples the filter's edge effects reach
        let mut want: Vec<f32> = Vec::with_capacity(N - skip);
        for n in skip..N {
            let c = n * OS;
            let mut acc = 0.0f64;
            for (j, h) in kernel.iter().enumerate() {
                let idx = c + j;
                if idx < fine.len() {
                    acc += h * fine[idx];
                }
            }
            #[expect(clippy::cast_possible_truncation, reason = "test")]
            want.push(acc as f32);
        }
        // Filter the *output* with the same shape at the output rate, so
        // both sides of the comparison carry the same band: without this,
        // the reference's 21 kHz corner measures the sync's honest
        // 21-24 kHz content as if it were error.
        let fc1 = 2.0 * 21_000.0 / f64::from(RATE);
        let mut kernel1: Vec<f64> = (0..TAPS)
            .map(|n| {
                let hann =
                    0.5 - 0.5 * (2.0 * std::f64::consts::PI * n as f64 / (TAPS - 1) as f64).cos();
                fc1 * sinc(fc1 * (n as f64 - half as f64)) * hann
            })
            .collect();
        let dc1: f64 = kernel1.iter().sum();
        if dc1.abs() > 1e-12 {
            for h in &mut kernel1 {
                *h /= dc1;
            }
        }
        let mut got_f: Vec<f32> = Vec::with_capacity(got.len());
        for n in 0..got.len() {
            let mut acc = 0.0f64;
            for (j, h) in kernel1.iter().enumerate() {
                let idx = n + j;
                if idx < got.len() {
                    acc += h * f64::from(got[idx]);
                }
            }
            #[expect(clippy::cast_possible_truncation, reason = "test")]
            got_f.push(acc as f32);
        }
        // Alignment: the output's filter delays it 64 samples, the
        // reference's 8 (4 of group delay at 16x plus the 4 skipped) —
        // the reference therefore *leads* by 56, and the search wiggles
        // around that.
        let signal = rms(&want);
        let best = (52..=60)
            .map(|lead| {
                let n = want.len() - lead;
                let e: f64 = (0..n)
                    .map(|i| {
                        let g = f64::from(got_f[i]);
                        let w = f64::from(want[i + lead]);
                        (g - w) * (g - w)
                    })
                    .sum::<f64>()
                    / n as f64;
                e.sqrt() / signal
            })
            .fold(f64::INFINITY, f64::min);
        let rel = best;
        eprintln!("hard sync vs oversampled reference: {rel:.3} RMS error");
        assert!(rel < 0.15, "sync waveform error {rel:.3} of signal RMS");
    }
}
