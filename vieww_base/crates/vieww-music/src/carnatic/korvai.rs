//! Moras and korvais: cadences as arithmetic.
//!
//! A **mora** is a phrase played three times with two equal gaps between:
//! `3·p + 2·g` pulses. A **korvai** chains several sections and ends with
//! a mora, and its whole point is to finish exactly on *sam* — the first
//! beat of the cycle. Whether it does is pure arithmetic, so it is computed
//! rather than hoped for: [`fit_mora`] finds every mora that, started at
//! a given pulse, lands on sam after a given number of cycles.
//!
//! Syllables are spoken (konnakol) or drummed as *sollus*; [`sollu`] gives
//! the conventional syllables for groups of one to nine pulses.

use super::tala::Tala;

/// A mora: a phrase of `phrase` pulses, three times, `gap` pulses
/// between.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Mora {
    /// Pulses in the phrase.
    pub phrase: u32,
    /// Pulses of rest (or held note) between repetitions.
    pub gap: u32,
}

impl Mora {
    /// Total length: `3·phrase + 2·gap`.
    #[must_use]
    pub const fn span(self) -> u32 {
        3 * self.phrase + 2 * self.gap
    }

    /// Pulse offsets (from the mora's start) at which each repetition
    /// begins.
    #[must_use]
    pub const fn starts(self) -> [u32; 3] {
        let step = self.phrase + self.gap;
        [0, step, 2 * step]
    }

    /// The mora spoken: each phrase as sollus, gaps as `-`.
    #[must_use]
    pub fn spoken(self) -> String {
        let p = sollu(self.phrase);
        let g = vec!["-"; self.gap as usize].join(" ");
        let mut parts = vec![p.clone()];
        for _ in 0..2 {
            if !g.is_empty() {
                parts.push(g.clone());
            }
            parts.push(p.clone());
        }
        parts.join(" ")
    }
}

/// Every mora exactly `span` pulses long, phrase ≥ `min_phrase`, longest
/// phrase first.
#[must_use]
pub fn moras(span: u32, min_phrase: u32) -> Vec<Mora> {
    let mut out: Vec<Mora> = (min_phrase.max(1)..=span / 3)
        .filter_map(|phrase| {
            let rest = span.checked_sub(3 * phrase)?;
            rest.is_multiple_of(2).then_some(Mora {
                phrase,
                gap: rest / 2,
            })
        })
        .collect();
    out.reverse();
    out
}

/// Every mora that, starting `start_pulse` pulses after sam, ends exactly
/// on sam after `cycles` cycles of `tala` — the korvai's arithmetic. An odd
/// span takes an odd phrase and an even one an even phrase, so this is
/// empty only when the span is shorter than three phrases of `min_phrase`.
#[must_use]
pub fn fit_mora(tala: &Tala, start_pulse: u32, cycles: u32, min_phrase: u32) -> Vec<Mora> {
    let total = tala.pulses() * cycles;
    total
        .checked_sub(start_pulse)
        .map_or_else(Vec::new, |span| moras(span, min_phrase))
}

/// Whether a passage of `span` pulses, begun `start_pulse` after sam, ends
/// on sam.
#[must_use]
pub fn lands_on_sam(tala: &Tala, start_pulse: u32, span: u32) -> bool {
    (start_pulse + span).is_multiple_of(tala.pulses())
}

/// The conventional sollus for a group of `n` pulses: `ta`, `ta ka`,
/// `ta ki ta`, `ta ka di mi`, `ta di ki na thom`; longer groups combine
/// those (6 = 3 + 3, 7 = 3 + 4, 8 = 4 + 4, 9 = 4 + 5), beyond 9 by fives.
#[must_use]
pub fn sollu(n: u32) -> String {
    match n {
        0 => String::new(),
        1 => "ta".into(),
        2 => "ta ka".into(),
        3 => "ta ki ta".into(),
        4 => "ta ka di mi".into(),
        5 => "ta di ki na thom".into(),
        6 => format!("{} {}", sollu(3), sollu(3)),
        7 => format!("{} {}", sollu(3), sollu(4)),
        8 => format!("{} {}", sollu(4), sollu(4)),
        9 => format!("{} {}", sollu(4), sollu(5)),
        _ => format!("{} {}", sollu(5), sollu(n - 5)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mora_arithmetic() {
        let m = Mora { phrase: 5, gap: 2 };
        assert_eq!(m.span(), 19);
        assert_eq!(m.starts(), [0, 7, 14]);
        assert_eq!(
            m.spoken(),
            "ta di ki na thom - - ta di ki na thom - - ta di ki na thom"
        );
        assert_eq!(
            moras(19, 1),
            vec![
                Mora { phrase: 5, gap: 2 },
                Mora { phrase: 3, gap: 5 },
                Mora { phrase: 1, gap: 8 }
            ]
        );
        assert!(moras(19, 6).is_empty());
    }

    #[test]
    fn every_fitted_mora_lands_on_sam() {
        for tala in Tala::all_suladi().chain([Tala::misra_chapu(), Tala::khanda_chapu()]) {
            for nadai in [3u8, 4, 5, 7] {
                let t = tala.clone().in_nadai(nadai);
                for start in [0, 1, 6, 9] {
                    for m in fit_mora(&t, start, 2, 2) {
                        assert!(lands_on_sam(&t, start, m.span()), "{} {m:?}", t.name());
                    }
                }
            }
        }
    }

    #[test]
    fn a_classic_adi_mora_from_the_half_beat() {
        // Starting six pulses (1½ beats) after sam in Adi, chaturasra
        // nadai, one cycle: 26 pulses to fill.
        let t = Tala::adi();
        let fits = fit_mora(&t, 6, 1, 4);
        assert!(fits.contains(&Mora { phrase: 8, gap: 1 }));
        assert!(fits.contains(&Mora { phrase: 6, gap: 4 }));
        assert!(fits.iter().all(|m| m.span() == 26));
        // An odd span takes an odd phrase: 27 pulses = 3·9 + 2·0, 3·7 + 2·3 …
        assert!(fit_mora(&t, 5, 1, 1).iter().all(|m| m.phrase % 2 == 1));
        // Too little room left for three phrases.
        assert!(fit_mora(&t, 30, 1, 1).is_empty());
    }

    #[test]
    fn sollus_count_their_pulses() {
        for n in 1..=17 {
            assert_eq!(sollu(n).split(' ').count(), n as usize, "{n}");
        }
    }
}
