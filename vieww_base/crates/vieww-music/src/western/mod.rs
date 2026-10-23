//! Western common-practice and popular theory: scales and modes, chords
//! and roman numerals, voice leading, rhythm.

pub mod chord;
pub mod rhythm;
pub mod scale;
pub mod voice_leading;

/// The name of pitch class `pc` (0 = C), sharps for black keys.
#[must_use]
pub const fn pc_name(pc: u8) -> &'static str {
    match pc % 12 {
        0 => "C",
        1 => "C#",
        2 => "D",
        3 => "D#",
        4 => "E",
        5 => "F",
        6 => "F#",
        7 => "G",
        8 => "G#",
        9 => "A",
        10 => "A#",
        _ => "B",
    }
}

/// A note name with octave for a MIDI pitch: 60 → `"C4"`.
#[must_use]
pub fn note_name(pitch: u8) -> String {
    format!("{}{}", pc_name(pitch % 12), i32::from(pitch / 12) - 1)
}

/// Read a note name (`C4`, `F#3`, `Bb2`, `E-1`) as a MIDI pitch.
#[must_use]
pub fn parse_note(name: &str) -> Option<u8> {
    let mut chars = name.chars();
    let base: i32 = match chars.next()?.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let rest: String = chars.collect();
    let (acc, octave) = if let Some(r) = rest.strip_prefix('#') {
        (1, r)
    } else if let Some(r) = rest.strip_prefix('b') {
        (-1, r)
    } else {
        (0, rest.as_str())
    };
    let octave: i32 = octave.parse().ok()?;
    u8::try_from((octave + 1) * 12 + base + acc)
        .ok()
        .filter(|p| *p < 128)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_names_round_trip() {
        assert_eq!(note_name(60), "C4");
        assert_eq!(note_name(69), "A4");
        assert_eq!(parse_note("A4"), Some(69));
        assert_eq!(parse_note("Bb3"), Some(58));
        assert_eq!(parse_note("F#2"), Some(42));
        assert_eq!(parse_note("C-1"), Some(0));
        assert_eq!(parse_note("H2"), None);
        for p in 0..128u8 {
            assert_eq!(parse_note(&note_name(p)), Some(p));
        }
    }
}
