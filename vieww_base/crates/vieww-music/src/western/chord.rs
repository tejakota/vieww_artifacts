//! Chords, their qualities, diatonic harmony and roman numerals.
//!
//! The diatonic chords of a scale are not looked up: they are built by
//! stacking every other degree, and their quality is read back from the
//! intervals. So the familiar table (major: I ii iii IV V vi vii°) is a
//! test result, and the same code harmonises any seven-note scale — a
//! melakarta included.

use std::fmt;

use super::scale::Scale;

/// A chord quality: its intervals above the root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Quality {
    /// 0 4 7.
    Major,
    /// 0 3 7.
    Minor,
    /// 0 3 6.
    Diminished,
    /// 0 4 8.
    Augmented,
    /// 0 2 7.
    Sus2,
    /// 0 5 7.
    Sus4,
    /// 0 7: the power chord (no third).
    Power,
    /// 0 4 7 11.
    Major7,
    /// 0 4 7 10: the dominant seventh.
    Dominant7,
    /// 0 3 7 10.
    Minor7,
    /// 0 3 6 10: half-diminished (ø7).
    HalfDiminished7,
    /// 0 3 6 9.
    Diminished7,
    /// 0 3 7 11.
    MinorMajor7,
    /// 0 4 8 11.
    Augmented7Major,
    /// 0 4 7 9.
    Major6,
    /// 0 3 7 9.
    Minor6,
    /// 0 4 7 10 14.
    Dominant9,
    /// 0 4 7 14 (add9).
    Add9,
}

impl Quality {
    /// Every quality.
    pub const ALL: [Self; 18] = [
        Self::Major,
        Self::Minor,
        Self::Diminished,
        Self::Augmented,
        Self::Sus2,
        Self::Sus4,
        Self::Power,
        Self::Major7,
        Self::Dominant7,
        Self::Minor7,
        Self::HalfDiminished7,
        Self::Diminished7,
        Self::MinorMajor7,
        Self::Augmented7Major,
        Self::Major6,
        Self::Minor6,
        Self::Dominant9,
        Self::Add9,
    ];

    /// Semitones above the root.
    #[must_use]
    pub const fn intervals(self) -> &'static [u8] {
        match self {
            Self::Major => &[0, 4, 7],
            Self::Minor => &[0, 3, 7],
            Self::Diminished => &[0, 3, 6],
            Self::Augmented => &[0, 4, 8],
            Self::Sus2 => &[0, 2, 7],
            Self::Sus4 => &[0, 5, 7],
            Self::Power => &[0, 7],
            Self::Major7 => &[0, 4, 7, 11],
            Self::Dominant7 => &[0, 4, 7, 10],
            Self::Minor7 => &[0, 3, 7, 10],
            Self::HalfDiminished7 => &[0, 3, 6, 10],
            Self::Diminished7 => &[0, 3, 6, 9],
            Self::MinorMajor7 => &[0, 3, 7, 11],
            Self::Augmented7Major => &[0, 4, 8, 11],
            Self::Major6 => &[0, 4, 7, 9],
            Self::Minor6 => &[0, 3, 7, 9],
            Self::Dominant9 => &[0, 4, 7, 10, 14],
            Self::Add9 => &[0, 4, 7, 14],
        }
    }

    /// The chord-symbol suffix: `""`, `"m"`, `"°"`, `"maj7"`, `"7"`,
    /// `"m7♭5"` …
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Major => "",
            Self::Minor => "m",
            Self::Diminished => "°",
            Self::Augmented => "+",
            Self::Sus2 => "sus2",
            Self::Sus4 => "sus4",
            Self::Power => "5",
            Self::Major7 => "maj7",
            Self::Dominant7 => "7",
            Self::Minor7 => "m7",
            Self::HalfDiminished7 => "m7♭5",
            Self::Diminished7 => "°7",
            Self::MinorMajor7 => "m(maj7)",
            Self::Augmented7Major => "+maj7",
            Self::Major6 => "6",
            Self::Minor6 => "m6",
            Self::Dominant9 => "9",
            Self::Add9 => "add9",
        }
    }

    /// The quality with exactly these intervals (root first), if any.
    #[must_use]
    pub fn from_intervals(iv: &[u8]) -> Option<Self> {
        Self::ALL.into_iter().find(|q| q.intervals() == iv)
    }
}

/// A chord: a root pitch class and a quality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Chord {
    /// Root pitch class (0 = C).
    pub root: u8,
    /// Quality.
    pub quality: Quality,
}

impl Chord {
    /// A chord.
    #[must_use]
    pub const fn new(root: u8, quality: Quality) -> Self {
        Self {
            root: root % 12,
            quality,
        }
    }

    /// Its pitch classes, root first.
    #[must_use]
    pub fn pitch_classes(&self) -> Vec<u8> {
        self.quality
            .intervals()
            .iter()
            .map(|i| (self.root + i) % 12)
            .collect()
    }

    /// The chord in close position from `root_pitch` (MIDI).
    #[must_use]
    pub fn pitches(&self, root_pitch: u8) -> Vec<u8> {
        self.quality
            .intervals()
            .iter()
            .map(|i| root_pitch.saturating_add(*i))
            .collect()
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", super::pc_name(self.root), self.quality.symbol())
    }
}

/// The chord built on each degree of a seven-note scale by stacking
/// thirds (`notes` = 3 for triads, 4 for sevenths). `None` for a degree
/// whose stack is not a named quality.
#[must_use]
pub fn diatonic(scale: &Scale, notes: usize) -> Vec<Option<Chord>> {
    let n = i32::try_from(scale.len()).unwrap_or(7);
    (0..n)
        .map(|d| {
            let root = scale.degree_pitch(0, d);
            let iv: Vec<u8> = (0..i32::try_from(notes).unwrap_or(3))
                .map(|k| {
                    let p = scale.degree_pitch(0, d + 2 * k) - root;
                    u8::try_from(p).unwrap_or(0)
                })
                .collect();
            let root_pc = u8::try_from((i32::from(scale.tonic) + root).rem_euclid(12)).unwrap_or(0);
            Quality::from_intervals(&iv).map(|q| Chord::new(root_pc, q))
        })
        .collect()
}

/// Read a roman numeral in a major key on `tonic`: `I`, `ii`, `iii7`,
/// `V7`, `vii°`, `viiø7`, `IVmaj7`, `bVII`, `#iv°`, `V9`, `Isus4`, `I5`.
///
/// Case gives the third (upper major, lower minor); `°` diminished, `+`
/// augmented, `ø` half-diminished; a `7` after an upper-case numeral is
/// dominant, `maj7` major-seventh; a leading `b` or `#` alters the root.
#[must_use]
pub fn roman(tonic: u8, numeral: &str) -> Option<Chord> {
    let (shift, rest) = match numeral.as_bytes().first()? {
        b'b' => (-1i32, &numeral[1..]),
        b'#' => (1, &numeral[1..]),
        _ => (0, numeral),
    };
    let upper = ["VII", "VI", "IV", "V", "III", "II", "I"];
    let (degree, is_upper, tail) = upper.iter().find_map(|u| {
        if let Some(t) = rest.strip_prefix(u) {
            Some((*u, true, t))
        } else {
            rest.strip_prefix(u.to_lowercase().as_str())
                .map(|t| (*u, false, t))
        }
    })?;
    let steps: i32 = match degree {
        "I" => 0,
        "II" => 2,
        "III" => 4,
        "IV" => 5,
        "V" => 7,
        "VI" => 9,
        _ => 11,
    };
    let quality = match (is_upper, tail) {
        (true, "") => Quality::Major,
        (false, "") => Quality::Minor,
        (_, "°") => Quality::Diminished,
        (_, "+") => Quality::Augmented,
        (_, "°7") => Quality::Diminished7,
        (_, "ø7" | "ø") => Quality::HalfDiminished7,
        (true, "7") => Quality::Dominant7,
        (false, "7") => Quality::Minor7,
        (true, "maj7") => Quality::Major7,
        (false, "maj7") => Quality::MinorMajor7,
        (true, "6") => Quality::Major6,
        (false, "6") => Quality::Minor6,
        (true, "9") => Quality::Dominant9,
        (_, "sus2") => Quality::Sus2,
        (_, "sus4") => Quality::Sus4,
        (_, "5") => Quality::Power,
        (true, "add9") => Quality::Add9,
        _ => return None,
    };
    let root = u8::try_from((i32::from(tonic) + steps + shift).rem_euclid(12)).ok()?;
    Some(Chord::new(root, quality))
}

/// A progression of roman numerals in a major key, space-separated.
#[must_use]
pub fn progression(tonic: u8, numerals: &str) -> Option<Vec<Chord>> {
    numerals
        .split_whitespace()
        .map(|n| roman(tonic, n))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::scale::{Mode, Scale, HARMONIC_MINOR};
    use super::*;

    fn symbols(chords: &[Option<Chord>]) -> Vec<String> {
        chords
            .iter()
            .map(|c| c.map_or_else(|| "?".into(), |c| c.quality.symbol().to_owned()))
            .collect()
    }

    #[test]
    fn diatonic_triads_and_sevenths_come_out_of_the_steps() {
        let c = Scale::major(0);
        assert_eq!(symbols(&diatonic(&c, 3)), ["", "m", "m", "", "", "m", "°"]);
        assert_eq!(
            symbols(&diatonic(&c, 4)),
            ["maj7", "m7", "m7", "maj7", "7", "m7", "m7♭5"]
        );
        let names: Vec<String> = diatonic(&c, 3)
            .iter()
            .flatten()
            .map(ToString::to_string)
            .collect();
        assert_eq!(names, ["C", "Dm", "Em", "F", "G", "Am", "B°"]);
        // Harmonic minor gives the augmented III and the diminished seventh.
        let hm = Scale::rotated(9, &HARMONIC_MINOR, 0);
        assert_eq!(symbols(&diatonic(&hm, 3))[2], "+");
        assert_eq!(symbols(&diatonic(&hm, 4))[6], "°7");
        // Dorian's IV is major.
        assert_eq!(symbols(&diatonic(&Scale::mode(2, Mode::Dorian), 3))[3], "");
    }

    #[test]
    fn roman_numerals_read() {
        let p = progression(0, "ii7 V7 Imaj7").unwrap();
        assert_eq!(
            p.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ["Dm7", "G7", "Cmaj7"]
        );
        assert_eq!(
            roman(7, "bVII"),
            Some(Chord::new(5, Quality::Major)),
            "F in G"
        );
        assert_eq!(
            roman(0, "viiø7"),
            Some(Chord::new(11, Quality::HalfDiminished7))
        );
        assert_eq!(roman(0, "#iv°"), Some(Chord::new(6, Quality::Diminished)));
        assert_eq!(roman(4, "I5"), Some(Chord::new(4, Quality::Power)));
        assert_eq!(roman(0, "VIII"), None);
        assert_eq!(roman(0, "Vx"), None);
    }

    #[test]
    fn qualities_round_trip() {
        for q in Quality::ALL {
            assert_eq!(Quality::from_intervals(q.intervals()), Some(q));
        }
        assert_eq!(Chord::new(9, Quality::Minor7).pitch_classes(), [9, 0, 4, 7]);
        assert_eq!(Chord::new(0, Quality::Major).pitches(60), [60, 64, 67]);
    }
}
