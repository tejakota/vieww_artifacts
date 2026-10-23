//! Scales and modes as step patterns.
//!
//! A mode is a rotation of a step pattern: the seven church modes are the
//! seven rotations of the major scale's `2 2 1 2 2 2 1`. Storing the steps
//! (not seven lookup tables) makes "the third mode of melodic minor" a
//! one-liner, and makes the overlap with the melakartas checkable: Dorian
//! *is* Kharaharapriya.

use crate::carnatic::melakarta::Melakarta;

/// A scale: semitone steps summing to 12, from a tonic pitch class.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Scale {
    /// The tonic's pitch class (0 = C).
    pub tonic: u8,
    /// Steps between successive degrees, summing to 12.
    pub steps: Vec<u8>,
}

/// The major scale's steps.
pub const MAJOR: [u8; 7] = [2, 2, 1, 2, 2, 2, 1];
/// Harmonic minor.
pub const HARMONIC_MINOR: [u8; 7] = [2, 1, 2, 2, 1, 3, 1];
/// Melodic minor (ascending, "jazz minor").
pub const MELODIC_MINOR: [u8; 7] = [2, 1, 2, 2, 2, 2, 1];
/// Major pentatonic.
pub const MAJOR_PENTATONIC: [u8; 5] = [2, 2, 3, 2, 3];
/// Minor pentatonic.
pub const MINOR_PENTATONIC: [u8; 5] = [3, 2, 2, 3, 2];
/// The blues scale: minor pentatonic with the flat fifth.
pub const BLUES: [u8; 6] = [3, 2, 1, 1, 3, 2];

/// The seven modes of the major scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Rotation 0: major.
    Ionian,
    /// Rotation 1.
    Dorian,
    /// Rotation 2.
    Phrygian,
    /// Rotation 3.
    Lydian,
    /// Rotation 4.
    Mixolydian,
    /// Rotation 5: natural minor.
    Aeolian,
    /// Rotation 6.
    Locrian,
}

impl Mode {
    /// All seven, by rotation.
    pub const ALL: [Self; 7] = [
        Self::Ionian,
        Self::Dorian,
        Self::Phrygian,
        Self::Lydian,
        Self::Mixolydian,
        Self::Aeolian,
        Self::Locrian,
    ];

    /// Its rotation of the major scale.
    #[must_use]
    pub const fn rotation(self) -> usize {
        self as usize
    }
}

impl Scale {
    /// A scale from steps.
    ///
    /// # Panics
    /// Steps that do not sum to an octave.
    #[must_use]
    pub fn new(tonic: u8, steps: &[u8]) -> Self {
        assert_eq!(
            steps.iter().map(|&s| u32::from(s)).sum::<u32>(),
            12,
            "steps make an octave"
        );
        Self {
            tonic: tonic % 12,
            steps: steps.to_vec(),
        }
    }

    /// The `rotation`-th mode of a step pattern.
    #[must_use]
    pub fn rotated(tonic: u8, steps: &[u8], rotation: usize) -> Self {
        let mut s = steps.to_vec();
        s.rotate_left(rotation % steps.len());
        Self::new(tonic, &s)
    }

    /// A church mode on `tonic`.
    #[must_use]
    pub fn mode(tonic: u8, mode: Mode) -> Self {
        Self::rotated(tonic, &MAJOR, mode.rotation())
    }

    /// Major on `tonic`.
    #[must_use]
    pub fn major(tonic: u8) -> Self {
        Self::mode(tonic, Mode::Ionian)
    }

    /// Natural minor on `tonic`.
    #[must_use]
    pub fn minor(tonic: u8) -> Self {
        Self::mode(tonic, Mode::Aeolian)
    }

    /// A melakarta as a Western scale on `tonic`.
    #[must_use]
    pub fn from_melakarta(tonic: u8, m: Melakarta) -> Self {
        let s = m.semitones();
        let mut steps: Vec<u8> = s.windows(2).map(|w| w[1] - w[0]).collect();
        steps.push(12 - s[6]);
        Self::new(tonic, &steps)
    }

    /// Number of degrees.
    #[must_use]
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Whether the scale has no degrees (never, for a valid scale).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Semitones above the tonic of each degree: `[0, 2, 4, 5, 7, 9, 11]`
    /// for major.
    #[must_use]
    pub fn intervals(&self) -> Vec<u8> {
        let mut acc = 0u8;
        let mut v = vec![0];
        for &s in &self.steps[..self.steps.len() - 1] {
            acc += s;
            v.push(acc);
        }
        v
    }

    /// The pitch classes, from the tonic.
    #[must_use]
    pub fn pitch_classes(&self) -> Vec<u8> {
        self.intervals()
            .iter()
            .map(|i| (self.tonic + i) % 12)
            .collect()
    }

    /// Whether pitch class `pc` is in the scale.
    #[must_use]
    pub fn contains(&self, pc: u8) -> bool {
        self.pitch_classes().contains(&(pc % 12))
    }

    /// The MIDI pitch of scale degree `degree` (0-based, any integer:
    /// negative and beyond the octave wrap), the tonic of octave 0 at
    /// `tonic_pitch`.
    #[must_use]
    pub fn degree_pitch(&self, tonic_pitch: i32, degree: i32) -> i32 {
        let n = i32::try_from(self.len()).unwrap_or(7);
        let oct = degree.div_euclid(n);
        let idx = usize::try_from(degree.rem_euclid(n)).unwrap_or(0);
        tonic_pitch + oct * 12 + i32::from(self.intervals()[idx])
    }

    /// The melakarta with the same seven notes, if this is a seven-note
    /// scale containing a perfect fifth and one of each letter.
    #[must_use]
    pub fn melakarta(&self) -> Option<Melakarta> {
        let iv = self.intervals();
        let arr: [u8; 7] = iv.try_into().ok()?;
        Melakarta::from_semitones(arr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_are_rotations() {
        assert_eq!(Scale::major(0).intervals(), [0, 2, 4, 5, 7, 9, 11]);
        assert_eq!(
            Scale::mode(2, Mode::Dorian).pitch_classes(),
            [2, 4, 5, 7, 9, 11, 0],
            "D Dorian: white keys"
        );
        assert_eq!(
            Scale::minor(9).pitch_classes(),
            [9, 11, 0, 2, 4, 5, 7],
            "A minor: white keys"
        );
        assert_eq!(
            Scale::mode(0, Mode::Lydian).intervals(),
            [0, 2, 4, 6, 7, 9, 11]
        );
        assert_eq!(
            Scale::mode(0, Mode::Locrian).intervals(),
            [0, 1, 3, 5, 6, 8, 10]
        );
    }

    #[test]
    fn the_two_systems_meet() {
        let pairs = [
            (Mode::Ionian, 29),
            (Mode::Dorian, 22),
            (Mode::Phrygian, 8),
            (Mode::Lydian, 65),
            (Mode::Mixolydian, 28),
            (Mode::Aeolian, 20),
        ];
        for (mode, mela) in pairs {
            assert_eq!(
                Scale::mode(0, mode).melakarta().map(Melakarta::number),
                Some(mela),
                "{mode:?}"
            );
        }
        // Locrian has no perfect fifth, so no melakarta holds it.
        assert_eq!(Scale::mode(0, Mode::Locrian).melakarta(), None);
        assert_eq!(
            Scale::rotated(0, &HARMONIC_MINOR, 0)
                .melakarta()
                .map(Melakarta::name),
            Some("Keeravani")
        );
        let mm = Scale::from_melakarta(0, Melakarta::new(15).unwrap());
        assert_eq!(mm.steps, [1, 3, 1, 2, 1, 3, 1]);
    }

    #[test]
    fn degrees_wrap_octaves() {
        let c = Scale::major(0);
        assert_eq!(c.degree_pitch(60, 0), 60);
        assert_eq!(c.degree_pitch(60, 7), 72);
        assert_eq!(c.degree_pitch(60, -1), 59);
        assert_eq!(c.degree_pitch(60, 9), 76);
        let blues = Scale::new(9, &BLUES);
        assert_eq!(blues.pitch_classes(), [9, 0, 2, 3, 4, 7]);
    }
}
