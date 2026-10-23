//! Talas: rhythmic cycles built from angas.
//!
//! A suladi tala is a sequence of *angas* (limbs): the **laghu**, whose
//! length is set by the tala's *jati*; the **drutam**, two beats; and the
//! **anudrutam**, one. Seven families of anga patterns × five jatis give
//! the 35 suladi talas, all generated here. The *chapu* talas are short
//! asymmetric cycles counted in groups instead.
//!
//! Each beat (*akshara*) is divided into *nadai* (gati) pulses — 4 by
//! default, 3, 5, 7 or 9 for tisra, khanda, misra and sankeerna nadai.
//!
//! The common names: chaturasra-jati triputa is **Adi** (8 beats);
//! chaturasra-jati rupaka is usually just **Rupaka** (6); misra-jati
//! jhampa is the usual **Jhampa** (10); khanda-jati ata is the usual
//! **Ata** (14).

use std::fmt;

/// The five jatis: how many beats a laghu takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Jati {
    /// 3.
    Tisra,
    /// 4.
    Chaturasra,
    /// 5.
    Khanda,
    /// 7.
    Misra,
    /// 9.
    Sankeerna,
}

impl Jati {
    /// All five, by count.
    pub const ALL: [Self; 5] = [
        Self::Tisra,
        Self::Chaturasra,
        Self::Khanda,
        Self::Misra,
        Self::Sankeerna,
    ];

    /// The count: 3, 4, 5, 7 or 9.
    #[must_use]
    pub const fn count(self) -> u8 {
        match self {
            Self::Tisra => 3,
            Self::Chaturasra => 4,
            Self::Khanda => 5,
            Self::Misra => 7,
            Self::Sankeerna => 9,
        }
    }

    /// The name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Tisra => "Tisra",
            Self::Chaturasra => "Chaturasra",
            Self::Khanda => "Khanda",
            Self::Misra => "Misra",
            Self::Sankeerna => "Sankeerna",
        }
    }
}

/// A limb of a tala.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Anga {
    /// A clap and finger counts: as many beats as the jati.
    Laghu,
    /// A clap and a wave: two beats.
    Drutam,
    /// A clap: one beat.
    Anudrutam,
}

/// The seven suladi families, by their anga pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Family {
    /// L D L L.
    Dhruva,
    /// L D L.
    Matya,
    /// D L.
    Rupaka,
    /// L U D.
    Jhampa,
    /// L D D.
    Triputa,
    /// L L D D.
    Ata,
    /// L.
    Eka,
}

impl Family {
    /// All seven, in the traditional order.
    pub const ALL: [Self; 7] = [
        Self::Dhruva,
        Self::Matya,
        Self::Rupaka,
        Self::Jhampa,
        Self::Triputa,
        Self::Ata,
        Self::Eka,
    ];

    /// The anga pattern.
    #[must_use]
    pub const fn angas(self) -> &'static [Anga] {
        use Anga::{Anudrutam as U, Drutam as D, Laghu as L};
        match self {
            Self::Dhruva => &[L, D, L, L],
            Self::Matya => &[L, D, L],
            Self::Rupaka => &[D, L],
            Self::Jhampa => &[L, U, D],
            Self::Triputa => &[L, D, D],
            Self::Ata => &[L, L, D, D],
            Self::Eka => &[L],
        }
    }

    /// The name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Dhruva => "Dhruva",
            Self::Matya => "Matya",
            Self::Rupaka => "Rupaka",
            Self::Jhampa => "Jhampa",
            Self::Triputa => "Triputa",
            Self::Ata => "Ata",
            Self::Eka => "Eka",
        }
    }
}

/// What the hand does on a beat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kriya {
    /// A clap (the first beat of every anga).
    Clap,
    /// A wave of the hand (a drutam's second beat).
    Wave,
    /// A finger count (a laghu's later beats): little finger is 1.
    Finger(u8),
}

impl fmt::Display for Kriya {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Clap => f.write_str("x"),
            Self::Wave => f.write_str("o"),
            Self::Finger(n) => write!(f, "{n}"),
        }
    }
}

/// How the cycle is built.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Structure {
    /// A suladi tala: a family in a jati.
    Suladi(Family, Jati),
    /// A chapu tala: beats grouped (Misra chapu: 3 + 2 + 2).
    Chapu(Vec<u8>),
}

/// A tala and its nadai.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Tala {
    /// How the cycle is built.
    pub structure: Structure,
    /// Pulses per beat (4 = chaturasra nadai).
    pub nadai: u8,
}

impl Tala {
    /// A suladi tala in chaturasra nadai.
    #[must_use]
    pub const fn suladi(family: Family, jati: Jati) -> Self {
        Self {
            structure: Structure::Suladi(family, jati),
            nadai: 4,
        }
    }

    /// All 35 suladi talas.
    pub fn all_suladi() -> impl Iterator<Item = Self> {
        Family::ALL
            .into_iter()
            .flat_map(|f| Jati::ALL.into_iter().map(move |j| Self::suladi(f, j)))
    }

    /// Adi: chaturasra-jati triputa, 8 beats.
    #[must_use]
    pub const fn adi() -> Self {
        Self::suladi(Family::Triputa, Jati::Chaturasra)
    }

    /// Rupaka as usually meant: chaturasra-jati rupaka, 6 beats.
    #[must_use]
    pub const fn rupaka() -> Self {
        Self::suladi(Family::Rupaka, Jati::Chaturasra)
    }

    /// Jhampa as usually meant: misra-jati jhampa, 10 beats.
    #[must_use]
    pub const fn jhampa() -> Self {
        Self::suladi(Family::Jhampa, Jati::Misra)
    }

    /// Ata as usually meant: khanda-jati ata, 14 beats.
    #[must_use]
    pub const fn ata() -> Self {
        Self::suladi(Family::Ata, Jati::Khanda)
    }

    /// Misra chapu: 7 beats, 3 + 2 + 2.
    #[must_use]
    pub fn misra_chapu() -> Self {
        Self {
            structure: Structure::Chapu(vec![3, 2, 2]),
            nadai: 4,
        }
    }

    /// Khanda chapu: 5 beats, 2 + 3.
    #[must_use]
    pub fn khanda_chapu() -> Self {
        Self {
            structure: Structure::Chapu(vec![2, 3]),
            nadai: 4,
        }
    }

    /// The same tala in another nadai (pulses per beat).
    #[must_use]
    pub fn in_nadai(mut self, nadai: u8) -> Self {
        self.nadai = nadai.max(1);
        self
    }

    /// Beats (aksharas) in one cycle.
    #[must_use]
    pub fn aksharas(&self) -> u32 {
        match &self.structure {
            Structure::Suladi(f, j) => f
                .angas()
                .iter()
                .map(|a| match a {
                    Anga::Laghu => u32::from(j.count()),
                    Anga::Drutam => 2,
                    Anga::Anudrutam => 1,
                })
                .sum(),
            Structure::Chapu(groups) => groups.iter().map(|&g| u32::from(g)).sum(),
        }
    }

    /// Pulses in one cycle (aksharas × nadai).
    #[must_use]
    pub fn pulses(&self) -> u32 {
        self.aksharas() * u32::from(self.nadai)
    }

    /// The hand's action on each beat of the cycle. Chapu talas clap the
    /// first beat of each group and count the rest.
    #[must_use]
    pub fn kriyas(&self) -> Vec<Kriya> {
        let mut out = Vec::new();
        match &self.structure {
            Structure::Suladi(f, j) => {
                for a in f.angas() {
                    match a {
                        Anga::Laghu => {
                            out.push(Kriya::Clap);
                            out.extend((1..j.count()).map(Kriya::Finger));
                        }
                        Anga::Drutam => out.extend([Kriya::Clap, Kriya::Wave]),
                        Anga::Anudrutam => out.push(Kriya::Clap),
                    }
                }
            }
            Structure::Chapu(groups) => {
                for &g in groups {
                    out.push(Kriya::Clap);
                    out.extend((1..g).map(Kriya::Finger));
                }
            }
        }
        out
    }

    /// The kriyas as a string: `"x 1 2 3 x o x o"` for Adi.
    #[must_use]
    pub fn notation(&self) -> String {
        self.kriyas()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The tala's name: `"Chaturasra-jati Triputa"`, `"Misra chapu"` …
    #[must_use]
    pub fn name(&self) -> String {
        match &self.structure {
            Structure::Suladi(f, j) => format!("{}-jati {}", j.name(), f.name()),
            Structure::Chapu(g) => {
                let n: u32 = g.iter().map(|&x| u32::from(x)).sum();
                let jati = match n {
                    3 => "Tisra",
                    5 => "Khanda",
                    7 => "Misra",
                    9 => "Sankeerna",
                    _ => "",
                };
                format!("{jati} chapu").trim().to_owned()
            }
        }
    }

    /// The nearest Western meter: aksharas over quarter notes.
    #[must_use]
    pub fn western_meter(&self) -> (u32, u32) {
        (self.aksharas(), 4)
    }

    /// Where pulse `p` (counted from sam, any number of cycles in) falls:
    /// `(cycle, beat, pulse within beat)`.
    #[must_use]
    pub fn locate(&self, p: u64) -> (u64, u32, u32) {
        let per = u64::from(self.pulses());
        let n = u64::from(self.nadai);
        let within = p % per;
        #[expect(clippy::cast_possible_truncation, reason = "within one cycle")]
        let (beat, pulse) = ((within / n) as u32, (within % n) as u32);
        (p / per, beat, pulse)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adi_is_eight_beats_with_its_hand_pattern() {
        let a = Tala::adi();
        assert_eq!(a.aksharas(), 8);
        assert_eq!(a.notation(), "x 1 2 3 x o x o");
        assert_eq!(a.name(), "Chaturasra-jati Triputa");
        assert_eq!(a.western_meter(), (8, 4));
        assert_eq!(a.pulses(), 32);
        assert_eq!(a.clone().in_nadai(3).pulses(), 24, "tisra nadai");
    }

    #[test]
    fn the_common_talas_have_their_lengths() {
        assert_eq!(Tala::rupaka().aksharas(), 6);
        assert_eq!(Tala::rupaka().notation(), "x o x 1 2 3");
        assert_eq!(Tala::jhampa().aksharas(), 10);
        assert_eq!(Tala::jhampa().notation(), "x 1 2 3 4 5 6 x x o");
        assert_eq!(Tala::ata().aksharas(), 14);
        assert_eq!(Tala::suladi(Family::Triputa, Jati::Tisra).aksharas(), 7);
        assert_eq!(
            Tala::suladi(Family::Dhruva, Jati::Chaturasra).aksharas(),
            14
        );
        assert_eq!(Tala::suladi(Family::Eka, Jati::Chaturasra).aksharas(), 4);
        assert_eq!(Tala::misra_chapu().aksharas(), 7);
        assert_eq!(Tala::misra_chapu().name(), "Misra chapu");
        assert_eq!(Tala::khanda_chapu().aksharas(), 5);
    }

    #[test]
    fn thirty_five_suladi_talas_by_construction() {
        let all: Vec<Tala> = Tala::all_suladi().collect();
        assert_eq!(all.len(), 35);
        let names: std::collections::HashSet<String> = all.iter().map(Tala::name).collect();
        assert_eq!(names.len(), 35);
        // Shortest: tisra eka (3); longest: sankeerna dhruva (9+2+9+9 = 29).
        assert_eq!(all.iter().map(Tala::aksharas).min(), Some(3));
        assert_eq!(all.iter().map(Tala::aksharas).max(), Some(29));
        for t in &all {
            assert_eq!(t.kriyas().len(), t.aksharas() as usize);
            assert_eq!(t.kriyas()[0], Kriya::Clap, "every cycle opens on a clap");
        }
    }

    #[test]
    fn pulses_locate_in_the_cycle() {
        let a = Tala::adi();
        assert_eq!(a.locate(0), (0, 0, 0));
        assert_eq!(a.locate(31), (0, 7, 3));
        assert_eq!(a.locate(32), (1, 0, 0), "sam again");
        assert_eq!(a.locate(32 * 3 + 9), (3, 2, 1));
    }
}
