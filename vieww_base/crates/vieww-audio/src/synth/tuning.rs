//! Tunings: what frequency a pitch number sounds at.
//!
//! Every voice in this crate turns a pitch into Hz through
//! [`pitch_to_hz`](super::voice::pitch_to_hz): twelve equal steps to the
//! octave, A4 = 440 Hz, MIDI key 69. That is one tuning among many — just
//! intonation, the Carnatic swara ratios, 22 shrutis, 19-, 31- or 53-EDO,
//! a Scala file — and none of the others can be reached from it.
//!
//! A [`Tuning`] is any function from pitch to Hz. A [`TuningTable`] is a
//! tuning made **realtime-safe**: built once off the audio thread, it stores
//! each of the 128 keys as the *equal-tempered pitch that sounds at the same
//! frequency*, and maps a fractional pitch by linear interpolation in that
//! log-frequency space — two loads and a multiply-add, no `log`, no `pow`.
//!
//! [`Poly`](super::voice::Poly) and [`Sampler`](super::sampler::Sampler)
//! take a table with `set_tuning` and retune every note through it before
//! the voice sees the pitch, so every voice — the built-in ones and any
//! implementor of [`Voice`](super::voice::Voice) — plays in the tuning
//! without a line of change. Pitch bend, unison detune and glide stay in
//! equal-tempered semitones on top of the tuned pitch, which is what a
//! player moving a bend wheel expects.

use super::voice::pitch_to_hz;

/// A map from pitch (MIDI-style: 60 = middle C, fractional allowed) to Hz.
///
/// Called off the audio thread (by [`TuningTable::from_tuning`]), so it may
/// be as slow as it likes.
pub trait Tuning {
    /// The frequency `pitch` sounds at.
    fn hz(&self, pitch: f32) -> f32;
}

/// `divisions` equal steps per octave, `reference_pitch` sounding at
/// `reference_hz`. The default is the crate's own tuning: 12 steps,
/// key 69 = 440 Hz.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EqualTemperament {
    /// Steps per octave (12 for the piano; 19, 22, 24, 31, 53 …).
    pub divisions: f32,
    /// The pitch number that sounds at `reference_hz`.
    pub reference_pitch: f32,
    /// Its frequency.
    pub reference_hz: f32,
}

impl Default for EqualTemperament {
    fn default() -> Self {
        Self {
            divisions: 12.0,
            reference_pitch: 69.0,
            reference_hz: 440.0,
        }
    }
}

impl Tuning for EqualTemperament {
    fn hz(&self, pitch: f32) -> f32 {
        self.reference_hz * ((pitch - self.reference_pitch) / self.divisions.max(1e-3)).exp2()
    }
}

/// A repeating scale: `tonic_pitch` sounds at `tonic_hz`, each following
/// pitch number is the next degree, and the degree list repeats every
/// `period_cents` (1200 for an octave).
///
/// `cents[0]` is the first step above the tonic; the tonic itself (0
/// cents) is implied. A 7-note scale of just ratios mapped onto seven keys
/// per octave, a 22-shruti octave on twenty-two, a Scala file's degrees —
/// all are this.
#[derive(Debug, Clone, PartialEq)]
pub struct ScaleTuning {
    /// The pitch number that sounds the tonic.
    pub tonic_pitch: i32,
    /// The tonic's frequency.
    pub tonic_hz: f32,
    /// Each degree above the tonic, in cents, ascending.
    pub cents: Vec<f32>,
    /// The repeat interval in cents (1200 = octave).
    pub period_cents: f32,
}

impl Tuning for ScaleTuning {
    fn hz(&self, pitch: f32) -> f32 {
        let n = self.cents.len() + 1;
        let degree_cents = |d: i64| -> f64 {
            #[expect(clippy::cast_possible_wrap, reason = "scales are short")]
            let len = n as i64;
            let period = d.div_euclid(len);
            let step = d.rem_euclid(len) as usize;
            let within = if step == 0 {
                0.0
            } else {
                f64::from(self.cents[step - 1])
            };
            #[expect(clippy::cast_precision_loss, reason = "small integers")]
            let p = period as f64;
            p * f64::from(self.period_cents) + within
        };
        // Fractional pitch: interpolate in cents between neighbours.
        let lo = f64::from(pitch).floor();
        let frac = f64::from(pitch) - lo;
        #[expect(clippy::cast_possible_truncation, reason = "pitch range")]
        let d = lo as i64 - i64::from(self.tonic_pitch);
        let c = degree_cents(d) + (degree_cents(d + 1) - degree_cents(d)) * frac;
        #[expect(clippy::cast_possible_truncation, reason = "audio precision")]
        let hz = (f64::from(self.tonic_hz) * (c / 1200.0).exp2()) as f32;
        hz
    }
}

/// A realtime-safe tuning: 128 keys precomputed as equivalent
/// equal-tempered pitches, interpolated in log frequency.
#[derive(Debug, Clone, PartialEq)]
pub struct TuningTable {
    /// `et[k]`: the 12-TET (A4 = 440) pitch that sounds where key `k` does.
    et: [f32; 128],
}

impl TuningTable {
    /// Precompute `tuning`. Off the audio thread: it calls `tuning.hz`
    /// 128 times and a `log2` each.
    #[must_use]
    pub fn from_tuning(tuning: &dyn Tuning) -> Self {
        let mut et = [0.0f32; 128];
        for (k, e) in et.iter_mut().enumerate() {
            #[expect(clippy::cast_precision_loss, reason = "0..128")]
            let hz = tuning.hz(k as f32).max(1e-6);
            *e = 69.0 + 12.0 * (hz / 440.0).log2();
        }
        Self { et }
    }

    /// The equal-tempered pitch that sounds where `pitch` does in this
    /// tuning — what a voice built on [`pitch_to_hz`] must be given.
    /// Linear between keys; beyond 0 and 127 the end step continues.
    #[must_use]
    pub fn retune(&self, pitch: f32) -> f32 {
        let p = pitch.clamp(-1_000.0, 1_000.0);
        #[expect(clippy::cast_possible_truncation, reason = "clamped above")]
        let i = (p.floor() as i32).clamp(0, 126);
        #[expect(clippy::cast_sign_loss, reason = "clamped to 0..=126")]
        let i = i as usize;
        #[expect(clippy::cast_precision_loss, reason = "0..127")]
        let t = p - i as f32;
        self.et[i] + (self.et[i + 1] - self.et[i]) * t
    }

    /// The equal-tempered pitch key `key` sounds at.
    #[must_use]
    pub fn key(&self, key: u8) -> f32 {
        self.et[usize::from(key & 127)]
    }

    /// Retune one key to sound at equal-tempered pitch `et_pitch` — what a
    /// MIDI Tuning Standard single-note change does (see [`crate::mts`]).
    pub fn set_key(&mut self, key: u8, et_pitch: f32) {
        self.et[usize::from(key & 127)] = et_pitch;
    }

    /// The frequency `pitch` sounds at in this tuning.
    #[must_use]
    pub fn hz(&self, pitch: f32) -> f32 {
        pitch_to_hz(self.retune(pitch))
    }
}

impl Default for TuningTable {
    fn default() -> Self {
        Self::from_tuning(&EqualTemperament::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cents(a: f32, b: f32) -> f32 {
        1200.0 * (a / b).log2()
    }

    #[test]
    fn the_default_table_is_the_crates_own_tuning() {
        let t = TuningTable::default();
        for p in [0.0, 21.0, 60.0, 60.37, 69.0, 100.5, 127.0] {
            assert!(cents(t.hz(p), pitch_to_hz(p)).abs() < 0.01, "pitch {p}");
            assert!((t.retune(p) - p).abs() < 1e-3);
        }
    }

    #[test]
    fn a_tonic_can_sit_anywhere() {
        // A Carnatic singer's shruti at C# = 277.18 Hz, say: retune the
        // concert pitch, keep the steps.
        let t = TuningTable::from_tuning(&EqualTemperament {
            reference_pitch: 61.0,
            reference_hz: 277.18,
            ..EqualTemperament::default()
        });
        assert!((t.hz(61.0) - 277.18).abs() < 0.01);
        assert!(cents(t.hz(73.0), t.hz(61.0)) - 1200.0 < 0.01);
    }

    #[test]
    fn a_just_scale_sounds_its_ratios() {
        // Seven just degrees on seven keys per period: S R G M P D N.
        let ratios: [f64; 6] = [
            9.0 / 8.0,
            5.0 / 4.0,
            4.0 / 3.0,
            3.0 / 2.0,
            5.0 / 3.0,
            15.0 / 8.0,
        ];
        #[expect(clippy::cast_possible_truncation, reason = "cents")]
        let scale = ScaleTuning {
            tonic_pitch: 60,
            tonic_hz: 261.63,
            cents: ratios.iter().map(|r| (1200.0 * r.log2()) as f32).collect(),
            period_cents: 1200.0,
        };
        let t = TuningTable::from_tuning(&scale);
        let base = t.hz(60.0);
        for (k, r) in ratios.iter().enumerate() {
            #[expect(clippy::cast_precision_loss, reason = "test")]
            let got = t.hz(61.0 + k as f32) / base;
            #[expect(clippy::cast_possible_truncation, reason = "test")]
            let want = *r as f32;
            assert!(cents(got, want).abs() < 0.05, "degree {k}: {got} vs {want}");
        }
        // The eighth key is the octave; the key below the tonic is N an
        // octave down.
        assert!(cents(t.hz(67.0), base * 2.0).abs() < 0.05);
        assert!(cents(t.hz(59.0), base * 15.0 / 16.0).abs() < 0.05);
    }

    #[test]
    fn many_steps_to_the_octave() {
        let t = TuningTable::from_tuning(&EqualTemperament {
            divisions: 53.0,
            reference_pitch: 60.0,
            reference_hz: 261.63,
        });
        assert!(cents(t.hz(113.0), t.hz(60.0)) - 1200.0 < 0.05);
        assert!((cents(t.hz(61.0), t.hz(60.0)) - 1200.0 / 53.0).abs() < 0.05);
    }

    #[test]
    fn retuning_is_continuous_between_keys() {
        let t = TuningTable::from_tuning(&EqualTemperament {
            divisions: 19.0,
            ..EqualTemperament::default()
        });
        let mut last = t.retune(40.0);
        for i in 1..=2000 {
            #[expect(clippy::cast_precision_loss, reason = "test")]
            let p = 40.0 + i as f32 * 0.01;
            let r = t.retune(p);
            assert!(r > last, "monotone");
            assert!(r - last < 0.02, "no jumps");
            last = r;
        }
    }
}
