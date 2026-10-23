//! Ragas as grammars: what a melody may do, not just which notes exist.
//!
//! A raga is more than its scale. Two ragas can share every swara and
//! still differ in how a melody moves through them — the ascent
//! (*arohana*) and descent (*avarohana*) may omit different swaras, may
//! zigzag (*vakra*), and each raga has characteristic phrases (*prayogas*)
//! and resting notes. [`Raga`] holds that grammar and
//! [`Raga::check`] tests a phrase against it.
//!
//! The movement rule implemented: an ascending step from note `a` to note
//! `b` is allowed when `b` comes after `a` on the arohana path (taken
//! across octaves); a descending step likewise along the avarohana. For a
//! plain (krama) raga that reduces to "only use the raga's notes in each
//! direction"; for a vakra raga it enforces the zigzag. Repeated notes are
//! always allowed. Leaps are allowed when they follow the path forward —
//! the tradition permits them, and judging which leaps *sound* right is a
//! matter of style, not grammar.

use super::melakarta::Melakarta;
use super::swara::{parse_phrase, Note, Swara};

/// A raga: its parent scale, its paths, and its phrases.
#[derive(Debug, Clone, PartialEq)]
pub struct Raga {
    /// Its name.
    pub name: String,
    /// The parent melakarta.
    pub melakarta: Melakarta,
    /// The ascent, one octave, Sa to upper Sa inclusive (may zigzag).
    pub arohana: Vec<Note>,
    /// The descent, upper Sa to Sa inclusive (may zigzag).
    pub avarohana: Vec<Note>,
    /// Characteristic phrases.
    pub prayogas: Vec<Vec<Note>>,
    /// Swaras a phrase likes to rest on.
    pub nyasa: Vec<Swara>,
    /// Per-swara intonation, cents from the 12-TET position — how this
    /// raga tunes its notes (empty: equal temperament; see
    /// [`Raga::just_offsets`]).
    pub cents: Vec<(Swara, f32)>,
}

/// Why a phrase breaks a raga's grammar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    /// The note is not in the raga at all.
    Foreign {
        /// Index in the phrase.
        at: usize,
        /// The note.
        note: Note,
    },
    /// The step `from → to` does not follow the ascent or descent path.
    Path {
        /// Index of `to` in the phrase.
        at: usize,
        /// Where the step starts.
        from: Note,
        /// Where it lands.
        to: Note,
        /// Whether the step rises.
        rising: bool,
    },
}

/// Absolute positions (semitones from middle Sa) of a one-octave path,
/// repeated across `-2..=2` octaves, in path order.
fn unrolled(path: &[Note], rising: bool) -> Vec<i32> {
    // Drop the closing Sa: the next octave's own Sa replaces it.
    let core: Vec<i32> = path[..path.len().saturating_sub(1)]
        .iter()
        .map(|n| n.semitones())
        .collect();
    let octaves: Vec<i32> = if rising {
        (-2..=2).collect()
    } else {
        (-2..=2).rev().collect()
    };
    octaves
        .into_iter()
        .flat_map(|o| core.iter().map(move |s| s + 12 * o))
        .collect()
}

impl Raga {
    /// A raga from notation: `arohana` and `avarohana` as swara strings
    /// (see [`parse_phrase`]).
    ///
    /// # Errors
    /// A bad swara name, or a path that does not begin and end on Sa.
    pub fn new(name: &str, melakarta: u8, arohana: &str, avarohana: &str) -> Result<Self, String> {
        let melakarta = Melakarta::new(melakarta).ok_or("no such melakarta")?;
        let arohana = parse_phrase(arohana)?;
        let avarohana = parse_phrase(avarohana)?;
        let sa = |n: Option<&Note>, oct| n.is_some_and(|n| n.swara == Swara::S && n.octave == oct);
        if !(sa(arohana.first(), 0) && sa(arohana.last(), 1)) {
            return Err("an arohana runs S … S'".into());
        }
        if !(sa(avarohana.first(), 1) && sa(avarohana.last(), 0)) {
            return Err("an avarohana runs S' … S".into());
        }
        let scale = melakarta.swaras();
        for n in arohana.iter().chain(&avarohana) {
            if !scale.iter().any(|s| s.same_position(n.swara)) {
                return Err(format!(
                    "{} is not in melakarta {}",
                    n.swara,
                    melakarta.name()
                ));
            }
        }
        Ok(Self {
            name: name.to_owned(),
            melakarta,
            arohana,
            avarohana,
            prayogas: Vec::new(),
            nyasa: Vec::new(),
            cents: Vec::new(),
        })
    }

    /// A melakarta played as a raga: the full scale both ways.
    #[must_use]
    pub fn sampurna(m: Melakarta) -> Self {
        let up: Vec<Note> = m
            .swaras()
            .iter()
            .map(|s| Note::mid(*s))
            .chain([Note {
                swara: Swara::S,
                octave: 1,
            }])
            .collect();
        let down: Vec<Note> = up.iter().rev().copied().collect();
        Self {
            name: m.name().to_owned(),
            melakarta: m,
            arohana: up,
            avarohana: down,
            prayogas: Vec::new(),
            nyasa: Vec::new(),
            cents: Vec::new(),
        }
    }

    /// Add a characteristic phrase.
    ///
    /// # Errors
    /// A bad swara name, or a phrase that breaks the raga's own grammar.
    pub fn with_prayoga(mut self, phrase: &str) -> Result<Self, String> {
        let p = parse_phrase(phrase)?;
        if let Err(v) = self.check(&p) {
            return Err(format!("prayoga {phrase:?} breaks the grammar: {v:?}"));
        }
        self.prayogas.push(p);
        Ok(self)
    }

    /// Tune each swara to its default just ratio (see
    /// [`Swara::just_ratio`]), as cents offsets from 12-TET.
    #[must_use]
    pub fn just_offsets(mut self) -> Self {
        let mut swaras: Vec<Swara> = self
            .arohana
            .iter()
            .chain(&self.avarohana)
            .map(|n| n.swara)
            .collect();
        swaras.sort();
        swaras.dedup();
        #[expect(clippy::cast_possible_truncation, reason = "cents in f32")]
        let cents = swaras
            .into_iter()
            .map(|s| {
                (
                    s,
                    (s.just_ratio().cents() - f64::from(s.semitone()) * 100.0) as f32,
                )
            })
            .collect();
        self.cents = cents;
        self
    }

    /// The swaras the raga uses (either direction).
    #[must_use]
    pub fn swaras(&self) -> Vec<Swara> {
        let mut v: Vec<Swara> = self
            .arohana
            .iter()
            .chain(&self.avarohana)
            .map(|n| n.swara)
            .collect();
        v.sort();
        v.dedup();
        v
    }

    /// The cents offset this raga gives `swara` (0 if none set).
    #[must_use]
    pub fn offset(&self, swara: Swara) -> f32 {
        self.cents
            .iter()
            .find(|(s, _)| *s == swara)
            .map_or(0.0, |(_, c)| *c)
    }

    /// The pitch number `note` plays at, Sa on `tonic_pitch`, intonation
    /// included.
    #[must_use]
    pub fn pitch(&self, note: Note, tonic_pitch: f32) -> f32 {
        note.pitch(tonic_pitch) + self.offset(note.swara) / 100.0
    }

    /// Whether the raga is vakra (either path zigzags).
    #[must_use]
    pub fn is_vakra(&self) -> bool {
        let mono = |p: &[Note], up: bool| {
            p.windows(2).all(|w| {
                let (a, b) = (w[0].semitones(), w[1].semitones());
                if up {
                    a < b
                } else {
                    a > b
                }
            })
        };
        !(mono(&self.arohana, true) && mono(&self.avarohana, false))
    }

    /// Check a phrase against the grammar: every note in the raga, every
    /// rising step along the arohana path, every falling step along the
    /// avarohana.
    ///
    /// # Errors
    /// The first [`Violation`].
    pub fn check(&self, phrase: &[Note]) -> Result<(), Violation> {
        let known = self.swaras();
        for (i, n) in phrase.iter().enumerate() {
            if !known.contains(&n.swara) {
                return Err(Violation::Foreign { at: i, note: *n });
            }
        }
        let up = unrolled(&self.arohana, true);
        let down = unrolled(&self.avarohana, false);
        // `b` follows `a` on `path` along a stretch that moves one way:
        // a leap may skip notes, but never a zigzag's turn.
        let follows = |path: &[i32], a: i32, b: i32, rising: bool| {
            path.iter()
                .enumerate()
                .filter(|(_, &x)| x == a)
                .any(|(i, _)| {
                    let mut j = i;
                    while j + 1 < path.len() {
                        let step_ok = if rising {
                            path[j + 1] > path[j]
                        } else {
                            path[j + 1] < path[j]
                        };
                        if !step_ok {
                            return false;
                        }
                        j += 1;
                        if path[j] == b {
                            return true;
                        }
                    }
                    false
                })
        };
        for (i, w) in phrase.windows(2).enumerate() {
            let (a, b) = (w[0].semitones(), w[1].semitones());
            if a == b {
                continue;
            }
            let rising = b > a;
            let ok = if rising {
                follows(&up, a, b, true)
            } else {
                follows(&down, a, b, false)
            };
            if !ok {
                return Err(Violation::Path {
                    at: i + 1,
                    from: w[0],
                    to: w[1],
                    rising,
                });
            }
        }
        Ok(())
    }
}

/// A small library of well-documented ragas, for examples and tests.
/// Each is defined by its arohana and avarohana as most textbooks give
/// them; schools differ on details of some ragas, so treat these as
/// starting points.
pub mod library {
    use super::{Melakarta, Raga};

    fn r(name: &str, mela: u8, up: &str, down: &str) -> Raga {
        Raga::new(name, mela, up, down).expect("library ragas are well-formed")
    }

    /// Mayamalavagowla (15), the lesson scale, sampurna.
    #[must_use]
    pub fn mayamalavagowla() -> Raga {
        Raga::sampurna(Melakarta::new(15).expect("15 exists"))
    }

    /// Shankarabharanam (29): the major scale, sampurna.
    #[must_use]
    pub fn shankarabharanam() -> Raga {
        Raga::sampurna(Melakarta::new(29).expect("29 exists"))
    }

    /// Kalyani (65): the Lydian scale, sampurna.
    #[must_use]
    pub fn kalyani() -> Raga {
        let mut k = Raga::sampurna(Melakarta::new(65).expect("65 exists"));
        "Kalyani".clone_into(&mut k.name);
        k
    }

    /// Kharaharapriya (22): Dorian, sampurna.
    #[must_use]
    pub fn kharaharapriya() -> Raga {
        Raga::sampurna(Melakarta::new(22).expect("22 exists"))
    }

    /// Mohanam (janya of 28): the major pentatonic, both ways.
    #[must_use]
    pub fn mohanam() -> Raga {
        r("Mohanam", 28, "S R2 G3 P D2 S'", "S' D2 P G3 R2 S")
    }

    /// Hamsadhwani (janya of 29): S R2 G3 P N3, both ways.
    #[must_use]
    pub fn hamsadhwani() -> Raga {
        r("Hamsadhwani", 29, "S R2 G3 P N3 S'", "S' N3 P G3 R2 S")
    }

    /// Hindolam (janya of 20): S G2 M1 D1 N2, both ways.
    #[must_use]
    pub fn hindolam() -> Raga {
        r("Hindolam", 20, "S G2 M1 D1 N2 S'", "S' N2 D1 M1 G2 S")
    }

    /// Madhyamavati (janya of 22): S R2 M1 P N2, both ways.
    #[must_use]
    pub fn madhyamavati() -> Raga {
        r("Madhyamavati", 22, "S R2 M1 P N2 S'", "S' N2 P M1 R2 S")
    }

    /// Abhogi (janya of 22): S R2 G2 M1 D2, both ways.
    #[must_use]
    pub fn abhogi() -> Raga {
        r("Abhogi", 22, "S R2 G2 M1 D2 S'", "S' D2 M1 G2 R2 S")
    }

    /// Bilahari (janya of 29): pentatonic ascent, full descent.
    #[must_use]
    pub fn bilahari() -> Raga {
        r("Bilahari", 29, "S R2 G3 P D2 S'", "S' N3 D2 P M1 G3 R2 S")
    }
}

#[cfg(test)]
mod tests {
    use super::library::*;
    use super::*;

    fn p(s: &str) -> Vec<Note> {
        parse_phrase(s).unwrap()
    }

    #[test]
    fn a_plain_raga_allows_its_notes_both_ways() {
        let m = mohanam();
        assert!(m.check(&p("S R2 G3 P D2 S' D2 P G3 R2 S")).is_ok());
        assert!(m.check(&p("G3 P D2 P G3")).is_ok());
        assert!(m.check(&p("S G3 D2 S'")).is_ok(), "leaps along the path");
        assert!(matches!(
            m.check(&p("S R2 M1")),
            Err(Violation::Foreign { at: 2, .. })
        ));
        assert!(!m.is_vakra());
    }

    #[test]
    fn asymmetric_ragas_differ_by_direction() {
        let b = bilahari();
        assert!(
            b.check(&p("S' N3 D2 P M1 G3 R2 S")).is_ok(),
            "M1 and N3 descending"
        );
        let err = b.check(&p("G3 M1 P")).unwrap_err();
        assert!(
            matches!(err, Violation::Path { rising: true, .. }),
            "no M1 on the way up: {err:?}"
        );
        assert!(b.check(&p("P D2 N3 S'")).is_err(), "no N3 on the way up");
    }

    #[test]
    fn a_vakra_path_is_enforced() {
        // An invented zigzag: up S R2 G3 R2 M1 P D2 S'.
        let v = Raga::new(
            "Vakra test",
            29,
            "S R2 G3 R2 M1 P D2 S'",
            "S' D2 P M1 G3 R2 S",
        )
        .unwrap();
        assert!(v.is_vakra());
        assert!(v.check(&p("S R2 G3 R2 M1 P")).is_ok());
        assert!(v.check(&p("G3 M1")).is_err(), "G3 to M1 must go through R2");
        assert!(v.check(&p("R2 M1")).is_ok());
    }

    #[test]
    fn construction_is_validated() {
        assert!(
            Raga::new("bad", 29, "R2 G3 S'", "S' S").is_err(),
            "starts on S"
        );
        assert!(
            Raga::new("bad", 28, "S R1 S'", "S' S").is_err(),
            "R1 not in 28"
        );
        assert!(mohanam().with_prayoga("S R2 G3 R2 S").is_ok());
        assert!(mohanam().with_prayoga("S M1 G3").is_err());
    }

    #[test]
    fn just_intonation_by_swara() {
        let m = mohanam().just_offsets();
        assert!(
            (m.offset(Swara::G3) + 13.686).abs() < 0.01,
            "5/4 is 13.7 c flat of 400"
        );
        assert!((m.offset(Swara::P) - 1.955).abs() < 0.01);
        assert!((m.pitch(Note::mid(Swara::G3), 60.0) - 63.863).abs() < 0.001);
        assert_eq!(m.offset(Swara::S), 0.0);
    }
}
