//! Swaras: the seven notes, their sixteen names on twelve positions, and
//! notes placed in an octave.

use std::fmt;

use crate::pitch::{Ratio, JUST_CHROMATIC};

/// A swara name. Sa and Pa are fixed; Ri, Ga, Dha and Ni have three names
/// each and Ma two, sixteen names on the twelve positions of the octave.
///
/// Four positions carry two names each (`R2 = G1`, `R3 = G2`, `D2 = N1`,
/// `D3 = N2`): which name a raga uses depends on which other swaras it
/// contains, and a raga never uses both names of one position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Swara {
    /// Shadjam, the tonic.
    S,
    /// Shuddha rishabham.
    R1,
    /// Chatushruti rishabham.
    R2,
    /// Shatshruti rishabham.
    R3,
    /// Shuddha gandharam.
    G1,
    /// Sadharana gandharam.
    G2,
    /// Antara gandharam.
    G3,
    /// Shuddha madhyamam.
    M1,
    /// Prati madhyamam.
    M2,
    /// Panchamam, the fifth.
    P,
    /// Shuddha dhaivatam.
    D1,
    /// Chatushruti dhaivatam.
    D2,
    /// Shatshruti dhaivatam.
    D3,
    /// Shuddha nishadam.
    N1,
    /// Kaishiki nishadam.
    N2,
    /// Kakali nishadam.
    N3,
}

impl Swara {
    /// All sixteen names, in order.
    pub const ALL: [Self; 16] = [
        Self::S,
        Self::R1,
        Self::R2,
        Self::R3,
        Self::G1,
        Self::G2,
        Self::G3,
        Self::M1,
        Self::M2,
        Self::P,
        Self::D1,
        Self::D2,
        Self::D3,
        Self::N1,
        Self::N2,
        Self::N3,
    ];

    /// Semitones above Sa in the 12-position octave.
    #[must_use]
    pub const fn semitone(self) -> u8 {
        match self {
            Self::S => 0,
            Self::R1 => 1,
            Self::R2 | Self::G1 => 2,
            Self::R3 | Self::G2 => 3,
            Self::G3 => 4,
            Self::M1 => 5,
            Self::M2 => 6,
            Self::P => 7,
            Self::D1 => 8,
            Self::D2 | Self::N1 => 9,
            Self::D3 | Self::N2 => 10,
            Self::N3 => 11,
        }
    }

    /// Which of the seven notes (0 = Sa … 6 = Ni) this name belongs to.
    #[must_use]
    pub const fn letter(self) -> u8 {
        match self {
            Self::S => 0,
            Self::R1 | Self::R2 | Self::R3 => 1,
            Self::G1 | Self::G2 | Self::G3 => 2,
            Self::M1 | Self::M2 => 3,
            Self::P => 4,
            Self::D1 | Self::D2 | Self::D3 => 5,
            Self::N1 | Self::N2 | Self::N3 => 6,
        }
    }

    /// The name as written in notation: `S`, `R2`, `M1`, `P` …
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::S => "S",
            Self::R1 => "R1",
            Self::R2 => "R2",
            Self::R3 => "R3",
            Self::G1 => "G1",
            Self::G2 => "G2",
            Self::G3 => "G3",
            Self::M1 => "M1",
            Self::M2 => "M2",
            Self::P => "P",
            Self::D1 => "D1",
            Self::D2 => "D2",
            Self::D3 => "D3",
            Self::N1 => "N1",
            Self::N2 => "N2",
            Self::N3 => "N3",
        }
    }

    /// Read a name: `S`, `R1` … `N3` (also `Sa`, `Pa`, and `M` / `R` / `G`
    /// / `D` / `N` with no number are refused as ambiguous).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "Sa" | "s" => Some(Self::S),
            "Pa" | "p" => Some(Self::P),
            _ => Self::ALL.iter().copied().find(|w| w.name() == s),
        }
    }

    /// The default just ratio for this position — the 5-limit chromatic
    /// scale in [`JUST_CHROMATIC`]. One convention: a swara's intonation
    /// varies with the raga and the phrase, which [`raga::Raga`]'s own
    /// cents offsets express.
    ///
    /// [`raga::Raga`]: super::raga::Raga
    #[must_use]
    pub const fn just_ratio(self) -> Ratio {
        match self.semitone() {
            0 => Ratio::UNISON,
            s => JUST_CHROMATIC[s as usize - 1],
        }
    }

    /// Whether two names share a position (`R2` and `G1`, say).
    #[must_use]
    pub const fn same_position(self, other: Self) -> bool {
        self.semitone() == other.semitone()
    }
}

impl fmt::Display for Swara {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A swara in an octave: `octave` 0 is the middle (madhya) octave, −1 the
/// lower (mandra), +1 the upper (tara).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Note {
    /// The swara.
    pub swara: Swara,
    /// Octave relative to the tonic's.
    pub octave: i8,
}

impl Note {
    /// A swara in the middle octave.
    #[must_use]
    pub const fn mid(swara: Swara) -> Self {
        Self { swara, octave: 0 }
    }

    /// Semitones above the middle-octave Sa (negative below it).
    #[must_use]
    pub const fn semitones(self) -> i32 {
        self.octave as i32 * 12 + self.swara.semitone() as i32
    }

    /// The pitch number this note plays when Sa is `tonic_pitch`.
    #[must_use]
    #[expect(clippy::cast_precision_loss, reason = "small integers")]
    pub fn pitch(self, tonic_pitch: f32) -> f32 {
        tonic_pitch + self.semitones() as f32
    }
}

/// Read swara notation: names separated by spaces; a trailing `'` raises a
/// note an octave (tara), a trailing `.` lowers it (mandra), repeatable.
/// `"S R2 G3 P D2 S'"` is Mohanam's ascent.
///
/// # Errors
/// The first token that is not a swara name.
pub fn parse_phrase(text: &str) -> Result<Vec<Note>, String> {
    text.split_whitespace()
        .map(|tok| {
            let base = tok.trim_end_matches(['\'', '.']);
            let ups = tok[base.len()..].matches('\'').count();
            let downs = tok[base.len()..].matches('.').count();
            let swara = Swara::parse(base).ok_or_else(|| format!("not a swara: {tok:?}"))?;
            #[expect(clippy::cast_possible_truncation, reason = "a few octaves")]
            let octave = ups as i8 - downs as i8;
            Ok(Note { swara, octave })
        })
        .collect()
}

/// Write notes back as swara notation (`'` and `.` for octaves).
#[must_use]
pub fn write_phrase(notes: &[Note]) -> String {
    notes
        .iter()
        .map(|n| {
            let mark = if n.octave >= 0 { "'" } else { "." };
            format!(
                "{}{}",
                n.swara.name(),
                mark.repeat(n.octave.unsigned_abs().into())
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixteen_names_on_twelve_positions() {
        let mut positions: Vec<u8> = Swara::ALL.iter().map(|s| s.semitone()).collect();
        positions.sort_unstable();
        positions.dedup();
        assert_eq!(positions, (0..12).collect::<Vec<u8>>());
        assert!(Swara::R2.same_position(Swara::G1));
        assert!(Swara::R3.same_position(Swara::G2));
        assert!(Swara::D2.same_position(Swara::N1));
        assert!(Swara::D3.same_position(Swara::N2));
        assert!(!Swara::G1.same_position(Swara::G2));
    }

    #[test]
    fn just_ratios_by_position() {
        assert_eq!(Swara::P.just_ratio(), Ratio::new(3, 2));
        assert_eq!(Swara::G3.just_ratio(), Ratio::new(5, 4));
        assert_eq!(Swara::M2.just_ratio(), Ratio::new(45, 32));
        assert_eq!(Swara::S.just_ratio(), Ratio::UNISON);
    }

    #[test]
    fn notation_round_trips() {
        let p = parse_phrase("S. R2 G3 P D2 S' R2''").unwrap();
        assert_eq!(
            p[0],
            Note {
                swara: Swara::S,
                octave: -1
            }
        );
        assert_eq!(p[5].semitones(), 12);
        assert_eq!(p[6].semitones(), 26);
        assert_eq!(write_phrase(&p), "S. R2 G3 P D2 S' R2''");
        assert!(parse_phrase("S X P").is_err());
    }
}
