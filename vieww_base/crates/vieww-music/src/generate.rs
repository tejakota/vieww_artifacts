//! Seeded, reproducible generation.
//!
//! Everything here takes a seed and is deterministic in it: the same seed
//! gives the same music on every machine, so a generated piece can be
//! named by its seed and regenerated, and tests can pin what comes out.
//!
//! * [`raga_walk`] — a melody that **cannot** break its raga's grammar:
//!   every step is drawn only from the moves the raga allows, biased toward
//!   stepwise motion and toward the raga's resting notes at phrase ends.
//! * [`Markov`] — a first-order chain over any symbols, trained from
//!   examples (phrases, chord sequences) and sampled.

use std::collections::HashMap;
use std::hash::Hash;

use vieww_audio::rng::Rng;

use crate::carnatic::raga::Raga;
use crate::carnatic::swara::{Note, Swara};

/// Every note one step away from `from` that the raga allows: the next
/// notes up its arohana path and down its avarohana path, across the
/// lower, middle and upper octaves.
fn moves(raga: &Raga, from: Note) -> Vec<Note> {
    let mut out = Vec::new();
    for octave in -1..=1i8 {
        for s in raga.swaras() {
            let to = Note { swara: s, octave };
            if to == from || to.semitones().abs_diff(from.semitones()) > 7 {
                continue;
            }
            if raga.check(&[from, to]).is_ok() && !out.contains(&to) {
                out.push(to);
            }
        }
    }
    out
}

/// A melody of `len` notes in `raga`, from `start`, ending (when it can)
/// on one of the raga's resting notes (or Sa). Every adjacent pair obeys
/// [`Raga::check`], so the whole phrase does.
#[must_use]
pub fn raga_walk(raga: &Raga, seed: u64, len: usize, start: Note) -> Vec<Note> {
    let mut rng = Rng::new(seed);
    let mut out = vec![start];
    let rests: Vec<Swara> = if raga.nyasa.is_empty() {
        vec![Swara::S]
    } else {
        raga.nyasa.clone()
    };
    while out.len() < len {
        let cur = *out.last().expect("non-empty");
        let mut opts = moves(raga, cur);
        if opts.is_empty() {
            break;
        }
        // On the last note, prefer a resting note if one is reachable.
        if out.len() + 1 == len {
            let resting: Vec<Note> = opts
                .iter()
                .copied()
                .filter(|n| rests.contains(&n.swara))
                .collect();
            if !resting.is_empty() {
                opts = resting;
            }
        }
        // Weight by closeness: steps far likelier than leaps; stay near
        // the middle octave.
        let weights: Vec<f64> = opts
            .iter()
            .map(|n| {
                let d = f64::from(n.semitones().abs_diff(cur.semitones()));
                let height = f64::from(n.semitones().abs_diff(6));
                1.0 / (1.0 + d * d) / (1.0 + height / 12.0)
            })
            .collect();
        let total: f64 = weights.iter().sum();
        let mut pick = rng.next_f64() * total;
        let mut chosen = opts[opts.len() - 1];
        for (n, w) in opts.iter().zip(&weights) {
            if pick < *w {
                chosen = *n;
                break;
            }
            pick -= w;
        }
        out.push(chosen);
    }
    out
}

/// A first-order Markov chain over symbols of type `T`.
#[derive(Debug, Clone, Default)]
pub struct Markov<T: Eq + Hash + Clone> {
    next: HashMap<T, Vec<(T, u32)>>,
}

impl<T: Eq + Hash + Clone> Markov<T> {
    /// An untrained chain.
    #[must_use]
    pub fn new() -> Self {
        Self {
            next: HashMap::new(),
        }
    }

    /// Count every transition in `sequence`.
    pub fn train(&mut self, sequence: &[T]) {
        for w in sequence.windows(2) {
            let row = self.next.entry(w[0].clone()).or_default();
            match row.iter_mut().find(|(t, _)| *t == w[1]) {
                Some((_, n)) => *n += 1,
                None => row.push((w[1].clone(), 1)),
            }
        }
    }

    /// The observed successors of `from`, with counts.
    #[must_use]
    pub fn successors(&self, from: &T) -> &[(T, u32)] {
        self.next.get(from).map_or(&[], Vec::as_slice)
    }

    /// Walk `len` symbols from `start`, choosing successors in proportion
    /// to their counts; stops early at a symbol with no successor.
    #[must_use]
    pub fn walk(&self, start: T, len: usize, seed: u64) -> Vec<T> {
        let mut rng = Rng::new(seed);
        let mut out = vec![start];
        while out.len() < len {
            let row = self.successors(out.last().expect("non-empty"));
            let total: u32 = row.iter().map(|(_, n)| n).sum();
            if total == 0 {
                break;
            }
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "below total"
            )]
            let mut pick = (rng.next_f64() * f64::from(total)) as u32;
            let mut chosen = row[row.len() - 1].0.clone();
            for (t, n) in row {
                if pick < *n {
                    chosen = t.clone();
                    break;
                }
                pick -= n;
            }
            out.push(chosen);
        }
        out
    }
}

/// A chord-progression chain trained on common rock and pop progressions
/// (as roman numerals in a major key).
#[must_use]
pub fn pop_progressions() -> Markov<&'static str> {
    let mut m = Markov::new();
    for p in [
        "I V vi IV I V vi IV I",
        "I IV V IV I IV V I",
        "vi IV I V vi IV I V",
        "I bVII IV I bVII IV I",
        "I vi IV V I vi IV V I",
        "ii V I vi ii V I",
        "I IV I V IV I",
    ] {
        let seq: Vec<&'static str> = p.split(' ').collect();
        m.train(&seq);
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carnatic::raga::library;
    use crate::western::chord::progression;

    #[test]
    fn a_raga_walk_never_breaks_its_grammar() {
        for raga in [
            library::mohanam(),
            library::bilahari(),
            library::hindolam(),
            library::mayamalavagowla(),
            library::kalyani(),
        ] {
            for seed in 0..50 {
                let w = raga_walk(&raga, seed, 24, Note::mid(Swara::S));
                assert_eq!(w.len(), 24, "{}", raga.name);
                assert!(
                    raga.check(&w).is_ok(),
                    "{} seed {seed}: {:?}",
                    raga.name,
                    raga.check(&w)
                );
            }
        }
    }

    #[test]
    fn walks_are_reproducible_and_seed_dependent() {
        let r = library::mohanam();
        let a = raga_walk(&r, 7, 16, Note::mid(Swara::S));
        assert_eq!(a, raga_walk(&r, 7, 16, Note::mid(Swara::S)));
        assert_ne!(a, raga_walk(&r, 8, 16, Note::mid(Swara::S)));
        assert_eq!(a.last().map(|n| n.swara), Some(Swara::S), "ends at rest");
    }

    #[test]
    fn markov_follows_its_training() {
        let mut m = Markov::new();
        m.train(&["a", "b", "a", "c", "a", "b"]);
        assert_eq!(m.successors(&"a"), [("b", 2), ("c", 1)]);
        let w = m.walk("a", 20, 3);
        for pair in w.windows(2) {
            assert!(m.successors(&pair[0]).iter().any(|(t, _)| *t == pair[1]));
        }
        let pop = pop_progressions().walk("I", 8, 42);
        assert_eq!(pop.len(), 8);
        assert!(
            progression(0, &pop.join(" ")).is_some(),
            "every symbol is a numeral"
        );
    }
}
