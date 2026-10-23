//! Synthesis — the instrument half of the crate.
//!
//! The crate root's [`Tone`](crate::Tone) is a *pulse*: a UI click, a chime.
//! This module is for *notes*: things held, released, played in chords and
//! shaped while they sound. It is what Serum, Vital, a DX7, a Prophet VS and
//! Web Audio's oscillator and periodic-wave nodes provide, built on the same
//! rule as everything else here — every generator is a state machine stepped
//! one sample at a time at a fixed rate, with no clock, no allocation in the
//! step, and no randomness that is not seeded. Render the same patch twice
//! and the buffers are equal.
//!
//! | Module | What it is | Reference point |
//! |---|---|---|
//! | [`envelope`] | ADSR, DAHDSR, any-stage envelopes with curves, loops and velocity | every synth |
//! | [`osc`] | band-limited (PolyBLEP/BLAMP) oscillators, pulse width, hard and soft sync | analog-modelled VAs |
//! | [`noise`] | white, pink, brown, blue/violet, filtered and crackle noise | `WhiteNoise`/`PinkNoise` UGens |
//! | [`wavetable`] | mip-mapped band-limited tables, frame morphing, periodic waves from Fourier coefficients | Serum, Vital, `PeriodicWave` |
//! | [`additive`] | partials with their own ratios, amplitudes and envelopes | Kawai K5, Harmor |
//! | [`vector`] | four-source joystick morphing with a path | Prophet VS, Wavestation |
//! | [`fm`] | six-operator phase/frequency modulation, the 32 DX7 algorithms, feedback | Yamaha DX7, FM8 |
//! | [`voice`] | polyphony, voice stealing, unison, legato/glide, MPE | every poly synth |
//!
//! | [`granular`] | grain clouds, sync/async scheduling, four windows, time-stretch and pitch-shift | Granulator, Quanta |
//! | [`physical`] | Karplus–Strong, clarinet and bowed waveguides, modal bars, mass–spring chains | STK, Modalys |
//! | [`sampler`] | multisampled zones, loops with crossfades, round robin, keyswitches, release triggers, choke groups | Kontakt, sfizz |
//! | [`sfz`], [`sf2`] | SFZ and SoundFont 2/3 loaders | — |

pub mod additive;
pub mod envelope;
pub mod fm;
pub mod granular;
pub mod noise;
pub mod osc;
pub mod physical;
pub mod sampler;
pub mod sf2;
pub mod sfz;
pub mod tuning;
pub mod vector;
pub mod voice;
pub mod wavetable;

use crate::Samples;

/// Anything that produces one sample per call at a fixed rate.
///
/// The trait every source in [`synth`](self) implements, so a vector
/// synth's four corners, a granulator's source or a modulation input can be
/// any of them.
pub trait Generator {
    /// The next sample.
    fn next_sample(&mut self) -> f32;

    /// Fill `out` with the next `out.len()` samples.
    fn fill(&mut self, out: &mut [f32]) {
        for s in out {
            *s = self.next_sample();
        }
    }

    /// Render `frames` samples into a mono buffer at `rate`.
    fn render(&mut self, frames: usize, rate: u32) -> Samples {
        let mut data = vec![0.0; frames];
        self.fill(&mut data);
        Samples::mono(data, rate)
    }
}

impl<G: Generator + ?Sized> Generator for Box<G> {
    fn next_sample(&mut self) -> f32 {
        (**self).next_sample()
    }
}

/// A constant — the simplest generator, and a useful corner for a vector
/// synth or a DC offset into a modulation slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Constant(pub f32);

impl Generator for Constant {
    fn next_sample(&mut self) -> f32 {
        self.0
    }
}

/// A generator from a closure.
pub struct FnGenerator<F: FnMut() -> f32>(pub F);

impl<F: FnMut() -> f32> std::fmt::Debug for FnGenerator<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FnGenerator")
    }
}

impl<F: FnMut() -> f32> Generator for FnGenerator<F> {
    fn next_sample(&mut self) -> f32 {
        (self.0)()
    }
}

/// Seconds to samples at `rate`, rounded, never negative.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped non-negative and far below usize::MAX"
)]
pub(crate) fn secs_to_samples(seconds: f32, rate: f32) -> usize {
    (seconds.max(0.0) * rate).round() as usize
}

/// Equal-power pan of a mono value: `p` in `-1..=1`.
#[must_use]
pub fn equal_power(p: f32) -> (f32, f32) {
    let a = (p.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    (a.cos(), a.sin())
}

#[cfg(test)]
pub(crate) mod testing {
    //! Spectral helpers the synthesis tests share.
    use crate::fft::{next_pow2, Plan};

    /// Magnitude spectrum (Hann-windowed, normalised so a full-scale sine
    /// at a bin centre reads ~0.5 there) of `x`, with the bin width in Hz.
    pub(crate) fn spectrum(x: &[f32], rate: f32) -> (Vec<f64>, f64) {
        let n = next_pow2(x.len()) / if x.len().is_power_of_two() { 1 } else { 2 };
        let n = n.max(2);
        let plan = Plan::new(n);
        #[expect(clippy::cast_precision_loss, reason = "test sizes")]
        let mut buf: Vec<crate::fft::Complex> = (0..n)
            .map(|i| {
                let w = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos();
                crate::fft::Complex::new(f64::from(x[i]) * w, 0.0)
            })
            .collect();
        plan.forward(&mut buf);
        #[expect(clippy::cast_precision_loss, reason = "test sizes")]
        let norm = 2.0 / (n as f64 * 0.5);
        let mags = buf[..n / 2].iter().map(|z| z.norm() * norm / 2.0).collect();
        #[expect(clippy::cast_precision_loss, reason = "test sizes")]
        (mags, f64::from(rate) / n as f64)
    }

    /// Peak magnitude within ±`width` bins of `hz`.
    pub(crate) fn level_near(mags: &[f64], bin_hz: f64, hz: f64, width: usize) -> f64 {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "test"
        )]
        let c = (hz / bin_hz).round() as usize;
        let lo = c.saturating_sub(width);
        let hi = (c + width).min(mags.len() - 1);
        mags[lo..=hi].iter().copied().fold(0.0, f64::max)
    }

    /// RMS of a slice.
    pub(crate) fn rms(x: &[f32]) -> f64 {
        #[expect(clippy::cast_precision_loss, reason = "test")]
        let n = x.len().max(1) as f64;
        (x.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() / n).sqrt()
    }
}
