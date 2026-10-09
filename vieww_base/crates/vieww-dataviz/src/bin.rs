//! `d3-array`'s statistics: histogram binning, quantiles, extent.

/// One histogram bin `[x0, x1)` (the last bin is closed).
#[derive(Debug, Clone, PartialEq)]
pub struct Bin {
    pub x0: f64,
    pub x1: f64,
    pub count: usize,
    /// Indices of the values that fell in.
    pub members: Vec<usize>,
}

/// How many bins.
#[derive(Debug, Clone, PartialEq)]
pub enum Thresholds {
    /// Sturges' rule: ⌈log₂ n⌉ + 1, on "nice" boundaries.
    Sturges,
    /// Freedman–Diaconis: width 2·IQR·n^(−1/3), on nice boundaries.
    FreedmanDiaconis,
    /// About this many, on nice boundaries.
    Count(usize),
    /// Exactly these boundaries.
    Explicit(Vec<f64>),
}

/// `(min, max)` ignoring NaN.
#[must_use]
pub fn extent(v: &[f64]) -> Option<(f64, f64)> {
    v.iter()
        .filter(|x| !x.is_nan())
        .fold(None, |acc, &x| match acc {
            None => Some((x, x)),
            Some((a, b)) => Some((a.min(x), b.max(x))),
        })
}

/// The `p`-quantile (R-7 / d3 definition: linear between order statistics).
#[must_use]
pub fn quantile(v: &[f64], p: f64) -> Option<f64> {
    let mut s: Vec<f64> = v.iter().copied().filter(|x| !x.is_nan()).collect();
    if s.is_empty() {
        return None;
    }
    s.sort_by(f64::total_cmp);
    #[allow(clippy::cast_precision_loss)]
    let h = (s.len() - 1) as f64 * p.clamp(0.0, 1.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let i = h.floor() as usize;
    let j = (i + 1).min(s.len() - 1);
    #[allow(clippy::cast_precision_loss)]
    Some(s[i] + (s[j] - s[i]) * (h - i as f64))
}

/// Bin `values`.
#[must_use]
pub fn histogram(values: &[f64], thresholds: &Thresholds) -> Vec<Bin> {
    let Some((lo, hi)) = extent(values) else {
        return Vec::new();
    };
    let n = values.iter().filter(|x| !x.is_nan()).count();
    let edges: Vec<f64> = match thresholds {
        Thresholds::Explicit(e) => {
            let mut e = e.clone();
            e.sort_by(f64::total_cmp);
            e
        }
        _ => {
            let count = match thresholds {
                #[allow(
                    clippy::cast_precision_loss,
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss
                )]
                Thresholds::Sturges => ((n.max(1) as f64).log2().ceil() as usize) + 1,
                Thresholds::FreedmanDiaconis => {
                    let iqr = quantile(values, 0.75).unwrap_or(0.0)
                        - quantile(values, 0.25).unwrap_or(0.0);
                    #[allow(clippy::cast_precision_loss)]
                    let width = 2.0 * iqr * (n.max(1) as f64).powf(-1.0 / 3.0);
                    if width > 0.0 {
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let c = ((hi - lo) / width).ceil().max(1.0) as usize;
                        c
                    } else {
                        1
                    }
                }
                Thresholds::Count(c) => (*c).max(1),
                Thresholds::Explicit(_) => unreachable!(),
            };
            let step = crate::scale::tick_step(lo, hi, count);
            if !(step.is_finite() && step > 0.0) {
                vec![lo, hi]
            } else {
                let start = (lo / step).floor() * step;
                let mut e = vec![start];
                while *e.last().expect("non-empty") < hi {
                    let next = e.last().expect("non-empty") + step;
                    e.push(next);
                }
                if e.len() == 1 {
                    e.push(start + step);
                }
                e
            }
        }
    };
    let mut bins: Vec<Bin> = edges
        .windows(2)
        .map(|w| Bin {
            x0: w[0],
            x1: w[1],
            count: 0,
            members: Vec::new(),
        })
        .collect();
    let last = bins.len().saturating_sub(1);
    for (i, &v) in values.iter().enumerate() {
        if v.is_nan() {
            continue;
        }
        let k = bins.partition_point(|b| b.x1 <= v).min(last);
        if !bins.is_empty() && v >= bins[k].x0 && (v < bins[k].x1 || k == last && v <= bins[k].x1) {
            bins[k].count += 1;
            bins[k].members.push(i);
        }
    }
    bins
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_add_up_and_edges_are_nice() {
        let v: Vec<f64> = (0..1000).map(|i| f64::from(i % 97) * 1.37).collect();
        let b = histogram(&v, &Thresholds::Sturges);
        assert_eq!(b.iter().map(|x| x.count).sum::<usize>(), 1000);
        assert!(b[0].x0 <= 0.0 && b.last().unwrap().x1 >= 96.0 * 1.37);
        for x in &b {
            assert!((x.x0 / (b[0].x1 - b[0].x0)).fract().abs() < 1e-9);
        }
    }

    #[test]
    fn explicit_thresholds_and_the_closed_last_bin() {
        let b = histogram(
            &[0.0, 1.0, 1.5, 2.0, f64::NAN],
            &Thresholds::Explicit(vec![0.0, 1.0, 2.0]),
        );
        assert_eq!(b.iter().map(|x| x.count).collect::<Vec<_>>(), vec![1, 3]);
    }

    #[test]
    fn quantiles_match_d3() {
        let v = [3.0, 1.0, 4.0, 1.0, 5.0, 9.0, 2.0, 6.0];
        assert_eq!(quantile(&v, 0.0), Some(1.0));
        assert_eq!(quantile(&v, 1.0), Some(9.0));
        assert!((quantile(&v, 0.5).unwrap() - 3.5).abs() < 1e-12);
        assert!((quantile(&v, 0.25).unwrap() - 1.75).abs() < 1e-12);
        assert_eq!(extent(&[]), None);
    }

    #[test]
    fn freedman_diaconis_adapts_to_spread() {
        let tight: Vec<f64> = (0..500).map(|i| f64::from(i) * 0.001).collect();
        let b = histogram(&tight, &Thresholds::FreedmanDiaconis);
        assert!(b.len() >= 5);
    }
}
