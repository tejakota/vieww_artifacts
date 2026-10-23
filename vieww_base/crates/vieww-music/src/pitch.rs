//! Exact intervals, cents, and the step from ratios to a tuning.
//!
//! A just fifth is 3/2, not 701.955 cents rounded somewhere: stacking two
//! gives exactly 9/4, and cents are computed once, at the boundary, as
//! `1200 · log₂(n/d)`. That keeps every tuning table here checkable to the
//! last digit against the tradition it encodes.

use std::fmt;

use vieww_audio::synth::tuning::ScaleTuning;

/// A frequency ratio `num/den`, always in lowest terms and positive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ratio {
    num: u64,
    den: u64,
}

const fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

impl Ratio {
    /// The unison, 1/1.
    pub const UNISON: Self = Self { num: 1, den: 1 };
    /// The octave, 2/1.
    pub const OCTAVE: Self = Self { num: 2, den: 1 };

    /// `num/den` in lowest terms.
    ///
    /// # Panics
    /// A zero numerator or denominator: no frequency ratio is zero or
    /// infinite.
    #[must_use]
    pub const fn new(num: u64, den: u64) -> Self {
        assert!(
            num != 0 && den != 0,
            "a frequency ratio is positive and finite"
        );
        let g = gcd(num, den);
        Self {
            num: num / g,
            den: den / g,
        }
    }

    /// Numerator (lowest terms).
    #[must_use]
    pub const fn num(self) -> u64 {
        self.num
    }

    /// Denominator (lowest terms).
    #[must_use]
    pub const fn den(self) -> u64 {
        self.den
    }

    /// The ratio as a float.
    #[must_use]
    #[expect(clippy::cast_precision_loss, reason = "musical ratios are small")]
    pub fn value(self) -> f64 {
        self.num as f64 / self.den as f64
    }

    /// The interval's size in cents: `1200 · log₂(num/den)`.
    #[must_use]
    pub fn cents(self) -> f64 {
        1200.0 * self.value().log2()
    }

    /// Stack two intervals (multiply the ratios), exactly.
    ///
    /// # Panics
    /// If the product overflows `u64` — far beyond any musical interval.
    #[must_use]
    pub const fn stack(self, other: Self) -> Self {
        let g1 = gcd(self.num, other.den);
        let g2 = gcd(other.num, self.den);
        Self::new(
            (self.num / g1) * (other.num / g2),
            (self.den / g2) * (other.den / g1),
        )
    }

    /// The inverse interval (`den/num`).
    #[must_use]
    pub const fn invert(self) -> Self {
        Self {
            num: self.den,
            den: self.num,
        }
    }

    /// Fold into one octave: `1/1 ≤ r < 2/1`.
    #[must_use]
    pub const fn octave_reduced(self) -> Self {
        let (mut n, mut d) = (self.num, self.den);
        while n >= 2 * d {
            d *= 2;
        }
        while n < d {
            n *= 2;
        }
        Self::new(n, d)
    }
}

impl fmt::Display for Ratio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.num, self.den)
    }
}

impl From<(u64, u64)> for Ratio {
    fn from((n, d): (u64, u64)) -> Self {
        Self::new(n, d)
    }
}

/// Cents between two frequencies.
#[must_use]
pub fn cents_between(from_hz: f64, to_hz: f64) -> f64 {
    1200.0 * (to_hz / from_hz).log2()
}

/// A repeating scale tuning built from exact ratios above a tonic: the
/// tonic sounds at `tonic_hz` on pitch number `tonic_pitch`, and each
/// following pitch number is the next ratio, repeating every octave.
///
/// `ratios` are the degrees *above* the tonic, ascending, within the
/// octave (1/1 itself is implied).
///
/// # Panics
/// A ratio outside `(1/1, 2/1)`, or ratios not ascending.
#[must_use]
pub fn ratio_scale(tonic_pitch: i32, tonic_hz: f32, ratios: &[Ratio]) -> ScaleTuning {
    let mut last = 0.0;
    let cents = ratios
        .iter()
        .map(|r| {
            let c = r.cents();
            assert!(
                c > last && c < 1200.0,
                "degrees ascend strictly within one octave"
            );
            last = c;
            #[expect(
                clippy::cast_possible_truncation,
                reason = "cents in f32 hold 0.0001 c"
            )]
            let c = c as f32;
            c
        })
        .collect();
    ScaleTuning {
        tonic_pitch,
        tonic_hz,
        cents,
        period_cents: 1200.0,
    }
}

/// The twelve pitch classes as a chromatic just scale: each semitone above
/// the tonic given its ratio (`ratios[0]` is the minor second … `[10]` the
/// major seventh), mapped onto the twelve keys of each octave — a just
/// keyboard, on which raga and Western scales alike sound in their ratios.
///
/// # Panics
/// As [`ratio_scale`].
#[must_use]
pub fn chromatic_just(tonic_pitch: i32, tonic_hz: f32, ratios: &[Ratio; 11]) -> ScaleTuning {
    ratio_scale(tonic_pitch, tonic_hz, ratios)
}

/// The 5-limit just chromatic scale most raga and harmony textbooks print:
/// 16/15 9/8 6/5 5/4 4/3 45/32 3/2 8/5 5/3 9/5 15/8.
pub const JUST_CHROMATIC: [Ratio; 11] = [
    Ratio::new(16, 15),
    Ratio::new(9, 8),
    Ratio::new(6, 5),
    Ratio::new(5, 4),
    Ratio::new(4, 3),
    Ratio::new(45, 32),
    Ratio::new(3, 2),
    Ratio::new(8, 5),
    Ratio::new(5, 3),
    Ratio::new(9, 5),
    Ratio::new(15, 8),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratios_are_exact() {
        let fifth = Ratio::new(3, 2);
        assert_eq!(fifth.stack(fifth), Ratio::new(9, 4));
        assert_eq!(fifth.stack(fifth).octave_reduced(), Ratio::new(9, 8));
        assert_eq!(Ratio::new(6, 4), fifth);
        assert_eq!(fifth.stack(fifth.invert()), Ratio::UNISON);
        // The syntonic comma: four fifths against a third two octaves up.
        let four_fifths = fifth.stack(fifth).stack(fifth).stack(fifth);
        let comma = four_fifths.stack(Ratio::new(5, 4).stack(Ratio::new(4, 1)).invert());
        assert_eq!(comma, Ratio::new(81, 80));
        assert!((comma.cents() - 21.506).abs() < 0.001);
    }

    #[test]
    fn cents_match_the_textbook() {
        assert!((Ratio::new(3, 2).cents() - 701.955).abs() < 0.001);
        assert!((Ratio::new(5, 4).cents() - 386.314).abs() < 0.001);
        assert!((Ratio::OCTAVE.cents() - 1200.0).abs() < 1e-9);
        assert_eq!(Ratio::new(256, 243).to_string(), "256/243");
    }

    #[test]
    fn a_just_keyboard_is_a_tuning() {
        let t = chromatic_just(60, 261.63, &JUST_CHROMATIC);
        assert_eq!(t.cents.len(), 11);
        assert!((f64::from(t.cents[6]) - 701.955).abs() < 0.001);
    }

    #[test]
    #[should_panic(expected = "ascend")]
    fn degrees_must_ascend() {
        let _ = ratio_scale(60, 261.63, &[Ratio::new(5, 4), Ratio::new(9, 8)]);
    }
}
