//! Voice leading: voicing a progression so the voices move least.
//!
//! Good voice leading is mostly one rule — move each voice as little as
//! possible — plus constraints: keep every chord tone, keep voices in
//! order (no crossing), stay in range, and (optionally) put the root in the
//! bass. That is a shortest-path problem: each chord has a set of legal
//! voicings, the cost of a step is the total semitones the voices move, and
//! [`lead`] finds the cheapest path through the whole progression (dynamic
//! programming over every candidate, so the answer is optimal, not greedy).

use super::chord::Chord;

/// Constraints on a voicing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Voicing {
    /// Number of voices.
    pub voices: usize,
    /// Lowest allowed pitch (MIDI).
    pub low: u8,
    /// Highest allowed pitch (MIDI).
    pub high: u8,
    /// Whether the bass must be the chord's root.
    pub root_in_bass: bool,
}

impl Default for Voicing {
    /// Four voices, C3 to G5, any inversion.
    fn default() -> Self {
        Self {
            voices: 4,
            low: 48,
            high: 79,
            root_in_bass: false,
        }
    }
}

/// Every legal voicing of `chord`: ascending pitches, every chord tone
/// present (when there are enough voices), root in the bass if asked.
#[must_use]
pub fn candidates(chord: &Chord, v: Voicing) -> Vec<Vec<u8>> {
    let pcs = chord.pitch_classes();
    let pool: Vec<u8> = (v.low..=v.high)
        .filter(|p| pcs.contains(&(p % 12)))
        .collect();
    let mut out = Vec::new();
    let mut cur = Vec::with_capacity(v.voices);
    fn rec(pool: &[u8], start: usize, need: usize, cur: &mut Vec<u8>, out: &mut Vec<Vec<u8>>) {
        if cur.len() == need {
            out.push(cur.clone());
            return;
        }
        for i in start..pool.len() {
            cur.push(pool[i]);
            rec(pool, i + 1, need, cur, out);
            cur.pop();
        }
    }
    rec(&pool, 0, v.voices, &mut cur, &mut out);
    let need_all = v.voices >= pcs.len();
    out.retain(|c| {
        (!need_all || pcs.iter().all(|pc| c.iter().any(|p| p % 12 == *pc)))
            && (!v.root_in_bass || c[0] % 12 == chord.root)
    });
    out
}

/// Semitones moved between two voicings, voice by voice.
#[must_use]
pub fn motion(a: &[u8], b: &[u8]) -> u32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| u32::from(x.abs_diff(*y)))
        .sum()
}

/// The voicing of each chord that minimises total motion across the whole
/// progression. `start`, when given, fixes the first voicing. Empty when
/// some chord has no legal voicing under `v`.
#[must_use]
pub fn lead(chords: &[Chord], v: Voicing, start: Option<&[u8]>) -> Vec<Vec<u8>> {
    let mut layers: Vec<Vec<Vec<u8>>> = chords.iter().map(|c| candidates(c, v)).collect();
    if let (Some(s), Some(first)) = (start, layers.first_mut()) {
        *first = vec![s.to_vec()];
    }
    if layers.is_empty() || layers.iter().any(Vec::is_empty) {
        return Vec::new();
    }
    // cost[i][k]: cheapest total motion ending in candidate k of chord i.
    let mut cost: Vec<Vec<u32>> = vec![vec![0; layers[0].len()]];
    let mut back: Vec<Vec<usize>> = vec![vec![0; layers[0].len()]];
    for i in 1..layers.len() {
        let (prev, here) = (&layers[i - 1], &layers[i]);
        let mut c = Vec::with_capacity(here.len());
        let mut b = Vec::with_capacity(here.len());
        for cand in here {
            let (best_k, best) = prev
                .iter()
                .enumerate()
                .map(|(k, p)| (k, cost[i - 1][k] + motion(p, cand)))
                .min_by_key(|&(_, c)| c)
                .unwrap_or((0, 0));
            c.push(best);
            b.push(best_k);
        }
        cost.push(c);
        back.push(b);
    }
    let last = layers.len() - 1;
    let mut k = cost[last]
        .iter()
        .enumerate()
        .min_by_key(|&(_, c)| c)
        .map_or(0, |(k, _)| k);
    let mut path = vec![Vec::new(); layers.len()];
    for i in (0..layers.len()).rev() {
        path[i] = std::mem::take(&mut layers[i][k]);
        k = back[i][k];
    }
    path
}

#[cfg(test)]
mod tests {
    use super::super::chord::progression;
    use super::*;

    #[test]
    fn a_ii_v_i_moves_by_step() {
        let chords = progression(0, "ii7 V7 Imaj7").unwrap();
        let v = lead(&chords, Voicing::default(), None);
        assert_eq!(v.len(), 3);
        for (a, b) in v.iter().zip(v.iter().skip(1)) {
            for (x, y) in a.iter().zip(b) {
                assert!(x.abs_diff(*y) <= 2, "{a:?} → {b:?}: a voice leaps");
            }
        }
        let total: u32 = v.windows(2).map(|w| motion(&w[0], &w[1])).sum();
        assert!(total <= 6, "{total} semitones in all");
    }

    #[test]
    fn constraints_hold() {
        let chords = progression(7, "I IV V7 I").unwrap();
        let rules = Voicing {
            root_in_bass: true,
            ..Voicing::default()
        };
        let v = lead(&chords, rules, None);
        for (voicing, chord) in v.iter().zip(&chords) {
            assert_eq!(voicing[0] % 12, chord.root, "root in bass");
            assert!(voicing.windows(2).all(|w| w[0] < w[1]), "no crossing");
            assert!(voicing.iter().all(|p| (48..=79).contains(p)));
            for pc in chord.pitch_classes() {
                assert!(
                    voicing.iter().any(|p| p % 12 == pc),
                    "{chord} keeps every tone"
                );
            }
        }
    }

    #[test]
    fn an_impossible_range_gives_nothing() {
        let chords = progression(0, "I").unwrap();
        let tiny = Voicing {
            low: 60,
            high: 62,
            ..Voicing::default()
        };
        assert!(lead(&chords, tiny, None).is_empty());
    }
}
