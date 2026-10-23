//! Rhythm as patterns on a grid: Euclidean rhythms, rotation, swing.
//!
//! A pattern is a cycle of steps, each a hit or a rest. The Euclidean
//! rhythm `E(k, n)` spreads `k` hits as evenly as possible over `n` steps
//! — and turns out to be the skeleton of rhythms worldwide: `E(3, 8)` is
//! the Cuban tresillo, `E(5, 8)` the cinquillo, `E(7, 12)` a common West
//! African bell pattern (Toussaint, *The Euclidean Algorithm Generates
//! Traditional Musical Rhythms*, 2005).

/// A cycle of hits (`true`) and rests.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Pattern(pub Vec<bool>);

impl Pattern {
    /// `k` hits spread evenly over `n` steps, starting on a hit.
    /// (Bresenham's line, which yields the same necklaces as Bjorklund's
    /// algorithm, rotated to begin with a hit.)
    #[must_use]
    pub fn euclid(k: usize, n: usize) -> Self {
        if n == 0 {
            return Self(Vec::new());
        }
        let k = k.min(n);
        let steps: Vec<bool> = (0..n).map(|i| (i * k) % n < k).collect();
        Self(steps)
    }

    /// Parse `x` (hit) and `.` (rest): `"x..x..x."`.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        Self(
            s.chars()
                .filter(|c| !c.is_whitespace())
                .map(|c| c == 'x')
                .collect(),
        )
    }

    /// Steps in the cycle.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the cycle has no steps.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Number of hits.
    #[must_use]
    pub fn hits(&self) -> usize {
        self.0.iter().filter(|h| **h).count()
    }

    /// Rotated `by` steps to the left.
    #[must_use]
    pub fn rotated(&self, by: usize) -> Self {
        let mut v = self.0.clone();
        if !v.is_empty() {
            let r = by % v.len();
            v.rotate_left(r);
        }
        Self(v)
    }

    /// Indices of the hits.
    #[must_use]
    pub fn onsets(&self) -> Vec<usize> {
        self.0
            .iter()
            .enumerate()
            .filter(|(_, h)| **h)
            .map(|(i, _)| i)
            .collect()
    }

    /// The hits as beat positions, each step `step_beats` long, swung:
    /// odd steps are delayed by `swing × step_beats / 2` (0 = straight,
    /// 1 = full triplet feel at `swing` = 2/3 of the pair).
    #[must_use]
    pub fn beats(&self, step_beats: f64, swing: f64) -> Vec<f64> {
        self.onsets()
            .into_iter()
            .map(|i| {
                #[expect(clippy::cast_precision_loss, reason = "step index")]
                let straight = i as f64 * step_beats;
                if i % 2 == 1 {
                    straight + swing.clamp(0.0, 1.0) * step_beats * 0.5
                } else {
                    straight
                }
            })
            .collect()
    }
}

impl std::fmt::Display for Pattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for h in &self.0 {
            f.write_str(if *h { "x" } else { "." })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn euclidean_rhythms_of_the_world() {
        assert_eq!(Pattern::euclid(3, 8).to_string(), "x..x..x.", "tresillo");
        assert_eq!(Pattern::euclid(5, 8).to_string(), "x.x.xx.x");
        assert_eq!(
            Pattern::euclid(5, 8).rotated(2).to_string(),
            "x.xx.xx.",
            "the cinquillo"
        );
        assert_eq!(
            Pattern::euclid(4, 16).to_string(),
            "x...x...x...x...",
            "four on the floor"
        );
        assert_eq!(Pattern::euclid(7, 12).hits(), 7);
        assert_eq!(Pattern::euclid(0, 4).to_string(), "....");
        assert_eq!(Pattern::euclid(9, 4).to_string(), "xxxx");
        for n in 1..=16 {
            for k in 0..=n {
                let p = Pattern::euclid(k, n);
                assert_eq!(p.hits(), k);
                // Evenness: gaps between hits differ by at most one.
                let on = p.onsets();
                if on.len() > 1 {
                    let mut gaps: Vec<usize> = on.windows(2).map(|w| w[1] - w[0]).collect();
                    gaps.push(n - on[on.len() - 1] + on[0]);
                    let (lo, hi) = (gaps.iter().min().unwrap(), gaps.iter().max().unwrap());
                    assert!(hi - lo <= 1, "E({k},{n}) = {p}");
                }
            }
        }
    }

    #[test]
    fn rotation_parsing_and_swing() {
        let p = Pattern::parse("x..x ..x.");
        assert_eq!(p, Pattern::euclid(3, 8));
        assert_eq!(p.rotated(3).to_string(), "x..x.x..");
        let eighths = Pattern::parse("xxxx");
        assert_eq!(eighths.beats(0.5, 0.0), [0.0, 0.5, 1.0, 1.5]);
        let swung = eighths.beats(0.5, 2.0 / 3.0);
        assert!(
            (swung[1] - (0.5 + 1.0 / 6.0)).abs() < 1e-12,
            "triplet swing"
        );
        assert_eq!(swung[2], 1.0);
    }
}
