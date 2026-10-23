//! Scala `.scl` scale files, read and written.
//!
//! Scala's format is the shared language of microtonal music: thousands of
//! historical, ethnic and experimental tunings are published as `.scl`.
//! The format: `!` lines are comments; the first non-comment line is a
//! description, the second the number of notes, then one pitch per line —
//! a **ratio** (`3/2`, or a bare integer `2`) or **cents** (anything with
//! a `.`). The unison is implied; the last pitch is the period (usually
//! the octave). Text after the number on a pitch line is ignored.

use std::fmt::Write as _;

use vieww_audio::synth::tuning::ScaleTuning;

use crate::pitch::Ratio;

/// One degree of a Scala scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Degree {
    /// An exact ratio.
    Ratio(Ratio),
    /// A size in cents.
    Cents(f64),
}

impl Degree {
    /// The degree in cents.
    #[must_use]
    pub fn cents(self) -> f64 {
        match self {
            Self::Ratio(r) => r.cents(),
            Self::Cents(c) => c,
        }
    }
}

/// A Scala scale: a description and its degrees, the last being the period.
#[derive(Debug, Clone, PartialEq)]
pub struct Scale {
    /// The description line.
    pub description: String,
    /// Degrees above the unison, ascending; the last is the period.
    pub degrees: Vec<Degree>,
}

/// Why a `.scl` file was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SclError {
    /// No description line or no count line.
    MissingHeader,
    /// The count line was not a number.
    BadCount(String),
    /// A pitch line could not be read.
    BadPitch(String),
    /// Fewer pitch lines than the count promised.
    TooFewPitches {
        /// The count line's number.
        expected: usize,
        /// Lines found.
        found: usize,
    },
}

impl std::fmt::Display for SclError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingHeader => f.write_str("a .scl file needs a description and a count"),
            Self::BadCount(s) => write!(f, "not a note count: {s:?}"),
            Self::BadPitch(s) => write!(f, "not a pitch: {s:?}"),
            Self::TooFewPitches { expected, found } => {
                write!(f, "{expected} pitches promised, {found} found")
            }
        }
    }
}

impl std::error::Error for SclError {}

fn parse_degree(line: &str) -> Result<Degree, SclError> {
    let tok = line.split_whitespace().next().unwrap_or("");
    let bad = || SclError::BadPitch(line.to_owned());
    if tok.contains('.') {
        return tok.parse::<f64>().map(Degree::Cents).map_err(|_| bad());
    }
    let (n, d) = match tok.split_once('/') {
        Some((n, d)) => (n, d),
        None => (tok, "1"),
    };
    let n: u64 = n.parse().map_err(|_| bad())?;
    let d: u64 = d.parse().map_err(|_| bad())?;
    if n == 0 || d == 0 {
        return Err(bad());
    }
    Ok(Degree::Ratio(Ratio::new(n, d)))
}

impl Scale {
    /// Read `.scl` text.
    ///
    /// # Errors
    /// [`SclError`] for a missing header, a bad count or pitch, or too few
    /// pitches.
    pub fn parse(text: &str) -> Result<Self, SclError> {
        let mut lines = text
            .lines()
            .map(str::trim_end)
            .filter(|l| !l.trim_start().starts_with('!'));
        let description = lines
            .next()
            .ok_or(SclError::MissingHeader)?
            .trim()
            .to_owned();
        let count_line = lines.next().ok_or(SclError::MissingHeader)?;
        let count: usize = count_line
            .split_whitespace()
            .next()
            .unwrap_or("")
            .parse()
            .map_err(|_| SclError::BadCount(count_line.to_owned()))?;
        let degrees = lines
            .filter(|l| !l.trim().is_empty())
            .take(count)
            .map(|l| parse_degree(l.trim()))
            .collect::<Result<Vec<_>, _>>()?;
        if degrees.len() < count {
            return Err(SclError::TooFewPitches {
                expected: count,
                found: degrees.len(),
            });
        }
        Ok(Self {
            description,
            degrees,
        })
    }

    /// Write `.scl` text: ratios as ratios, cents with six decimals.
    #[must_use]
    pub fn to_scl(&self, file_name: &str) -> String {
        let mut s = format!(
            "! {file_name}\n!\n{}\n {}\n!\n",
            self.description,
            self.degrees.len()
        );
        for d in &self.degrees {
            match d {
                Degree::Ratio(r) => {
                    let _ = writeln!(s, " {r}");
                }
                Degree::Cents(c) => {
                    let _ = writeln!(s, " {c:.6}");
                }
            }
        }
        s
    }

    /// A scale from exact ratios above the unison, the period last.
    #[must_use]
    pub fn from_ratios(description: &str, ratios: &[Ratio]) -> Self {
        Self {
            description: description.to_owned(),
            degrees: ratios.iter().copied().map(Degree::Ratio).collect(),
        }
    }

    /// Play it: `tonic_pitch` sounds at `tonic_hz`, and each following
    /// pitch number is the next degree, repeating every period.
    ///
    /// # Panics
    /// An empty scale (no period).
    #[must_use]
    pub fn tuning(&self, tonic_pitch: i32, tonic_hz: f32) -> ScaleTuning {
        let (period, inner) = self
            .degrees
            .split_last()
            .expect("a scale has at least its period");
        #[expect(clippy::cast_possible_truncation, reason = "cents in f32")]
        ScaleTuning {
            tonic_pitch,
            tonic_hz,
            cents: inner.iter().map(|d| d.cents() as f32).collect(),
            period_cents: period.cents() as f32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHRUTI_22: &str = "! shruti22.scl
!
22 shrutis: a common tabulation of the Indian octave
 22
!
 256/243
 16/15
 10/9
 9/8
 32/27
 6/5
 5/4
 81/64
 4/3
 27/20
 45/32
 729/512
 3/2
 128/81
 8/5
 5/3
 27/16
 16/9
 9/5
 15/8
 243/128
 2/1
";

    #[test]
    fn the_22_shrutis_read_and_round_trip() {
        let s = Scale::parse(SHRUTI_22).unwrap();
        assert_eq!(s.degrees.len(), 22);
        assert_eq!(s.degrees[12], Degree::Ratio(Ratio::new(3, 2)));
        assert!((s.degrees[12].cents() - 701.955).abs() < 0.001);
        let back = Scale::parse(&s.to_scl("shruti22.scl")).unwrap();
        assert_eq!(back, s, "lossless");
    }

    #[test]
    fn cents_ratios_and_integers_all_read() {
        let s = Scale::parse("! x\nmixed\n3\n100.0 a semitone\n3/2\n2\n").unwrap();
        assert_eq!(s.degrees[0], Degree::Cents(100.0));
        assert_eq!(s.degrees[2], Degree::Ratio(Ratio::OCTAVE));
        let t = s.tuning(60, 261.63);
        assert_eq!(t.cents.len(), 2);
        assert!((t.period_cents - 1200.0).abs() < 1e-3);
    }

    #[test]
    fn bad_files_are_refused() {
        assert_eq!(
            Scale::parse("only a description"),
            Err(SclError::MissingHeader)
        );
        assert!(matches!(
            Scale::parse("d\nmany\n"),
            Err(SclError::BadCount(_))
        ));
        assert!(matches!(
            Scale::parse("d\n2\n3/0\n2\n"),
            Err(SclError::BadPitch(_))
        ));
        assert_eq!(
            Scale::parse("d\n3\n3/2\n2\n"),
            Err(SclError::TooFewPitches {
                expected: 3,
                found: 2
            })
        );
    }

    #[test]
    fn a_19_edo_file_tunes_19_keys_per_octave() {
        let mut text = String::from("19-EDO\n19\n");
        for i in 1..=19 {
            let _ = writeln!(text, "{:.5}", f64::from(i) * 1200.0 / 19.0);
        }
        let t = Scale::parse(&text).unwrap().tuning(60, 261.63);
        let table = vieww_audio::synth::tuning::TuningTable::from_tuning(&t);
        let octave = 1200.0 * (table.hz(79.0) / table.hz(60.0)).log2();
        assert!((octave - 1200.0).abs() < 0.05);
    }
}
