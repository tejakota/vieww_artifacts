//! Statistical marks — Matplotlib's `boxplot`, `violinplot`, `imshow`/
//! `pcolormesh` heatmaps and `errorbar`, Observable Plot's box and density
//! marks — as geometry, ready for any renderer.

/// Five-number summary plus outliers (Tukey's 1.5 IQR fences).
#[derive(Debug, Clone, PartialEq)]
pub struct BoxStats {
    pub min: f64,
    pub q1: f64,
    pub median: f64,
    pub q3: f64,
    pub max: f64,
    /// Whisker ends: the most extreme data within the fences.
    pub whisker_low: f64,
    pub whisker_high: f64,
    pub outliers: Vec<f64>,
    pub mean: f64,
}

/// The `p`-quantile (0..1) of sorted data, linear interpolation (numpy's
/// default, Hyndman–Fan type 7).
#[must_use]
pub fn quantile_sorted(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    #[allow(clippy::cast_precision_loss)]
    let h = (sorted.len() - 1) as f64 * p.clamp(0.0, 1.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let i = h.floor() as usize;
    let f = h - h.floor();
    if i + 1 < sorted.len() {
        sorted[i] + (sorted[i + 1] - sorted[i]) * f
    } else {
        sorted[i]
    }
}

/// Box-plot statistics for `data` (NaNs ignored); `None` when empty.
#[must_use]
pub fn box_stats(data: &[f64]) -> Option<BoxStats> {
    let mut v: Vec<f64> = data.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let q1 = quantile_sorted(&v, 0.25);
    let q3 = quantile_sorted(&v, 0.75);
    let iqr = q3 - q1;
    let (lo_f, hi_f) = (q1 - 1.5 * iqr, q3 + 1.5 * iqr);
    let whisker_low = v.iter().copied().find(|x| *x >= lo_f).unwrap_or(v[0]);
    let whisker_high = v
        .iter()
        .rev()
        .copied()
        .find(|x| *x <= hi_f)
        .unwrap_or(v[v.len() - 1]);
    #[allow(clippy::cast_precision_loss)]
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    Some(BoxStats {
        min: v[0],
        q1,
        median: quantile_sorted(&v, 0.5),
        q3,
        max: v[v.len() - 1],
        whisker_low,
        whisker_high,
        outliers: v
            .iter()
            .copied()
            .filter(|x| *x < lo_f || *x > hi_f)
            .collect(),
        mean,
    })
}

/// Scott's rule bandwidth.
#[must_use]
pub fn scott_bandwidth(data: &[f64]) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    let n = data.len().max(2) as f64;
    let mean = data.iter().sum::<f64>() / n;
    let var = data.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0);
    (var.sqrt() * n.powf(-0.2)).max(1e-9)
}

/// Gaussian kernel density estimate at `points` evenly spaced over
/// `[lo, hi]`; `bandwidth` defaults to Scott's rule. Integrates to ~1.
#[must_use]
pub fn kde(
    data: &[f64],
    lo: f64,
    hi: f64,
    points: usize,
    bandwidth: Option<f64>,
) -> Vec<(f64, f64)> {
    let h = bandwidth.unwrap_or_else(|| scott_bandwidth(data));
    #[allow(clippy::cast_precision_loss)]
    let n = data.len().max(1) as f64;
    let norm = 1.0 / (n * h * (2.0 * std::f64::consts::PI).sqrt());
    (0..points.max(2))
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let x = lo + (hi - lo) * i as f64 / (points.max(2) - 1) as f64;
            let d: f64 = data
                .iter()
                .map(|v| (-0.5 * ((x - v) / h).powi(2)).exp())
                .sum();
            (x, d * norm)
        })
        .collect()
}

/// A violin's outline: the KDE mirrored around `center`, scaled so the
/// widest point is `half_width`. Returns the polygon (value axis = y).
#[must_use]
pub fn violin(data: &[f64], center: f64, half_width: f64, points: usize) -> Vec<(f64, f64)> {
    let Some(b) = box_stats(data) else {
        return Vec::new();
    };
    let pad = (b.max - b.min).max(1e-9) * 0.1;
    let d = kde(data, b.min - pad, b.max + pad, points, None);
    let peak = d.iter().map(|p| p.1).fold(0.0, f64::max).max(1e-12);
    let mut poly: Vec<(f64, f64)> = d
        .iter()
        .map(|(y, w)| (center + w / peak * half_width, *y))
        .collect();
    poly.extend(
        d.iter()
            .rev()
            .map(|(y, w)| (center - w / peak * half_width, *y)),
    );
    poly
}

/// One heatmap cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    pub row: usize,
    pub col: usize,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub value: f64,
    /// `value` normalised to 0..1 over the matrix's finite range.
    pub t: f64,
}

/// Lay a `rows × cols` matrix (row-major) into a `width × height` box with
/// `gap` between cells.
#[must_use]
pub fn heatmap(
    values: &[f64],
    rows: usize,
    cols: usize,
    width: f64,
    height: f64,
    gap: f64,
) -> Vec<Cell> {
    let (lo, hi) = values
        .iter()
        .filter(|v| v.is_finite())
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
            (a.min(*v), b.max(*v))
        });
    let span = (hi - lo).max(1e-12);
    #[allow(clippy::cast_precision_loss)]
    let (cw, ch) = (
        (width - gap * (cols as f64 - 1.0)) / cols as f64,
        (height - gap * (rows as f64 - 1.0)) / rows as f64,
    );
    let mut out = Vec::with_capacity(rows * cols);
    for r in 0..rows {
        for c in 0..cols {
            let v = values.get(r * cols + c).copied().unwrap_or(f64::NAN);
            #[allow(clippy::cast_precision_loss)]
            out.push(Cell {
                row: r,
                col: c,
                x: c as f64 * (cw + gap),
                y: r as f64 * (ch + gap),
                w: cw,
                h: ch,
                value: v,
                t: if v.is_finite() {
                    (v - lo) / span
                } else {
                    f64::NAN
                },
            });
        }
    }
    out
}

/// Mean and a symmetric error (standard error × `z`) — `errorbar`'s data.
#[must_use]
pub fn mean_and_error(data: &[f64], z: f64) -> (f64, f64) {
    #[allow(clippy::cast_precision_loss)]
    let n = data.len().max(1) as f64;
    let mean = data.iter().sum::<f64>() / n;
    let sd = (data.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0)).sqrt();
    (mean, z * sd / n.sqrt())
}

/// Ordinary least squares `y = a + b x` with R².
#[must_use]
pub fn linear_fit(xs: &[f64], ys: &[f64]) -> (f64, f64, f64) {
    #[allow(clippy::cast_precision_loss)]
    let n = xs.len().min(ys.len()).max(1) as f64;
    let mx = xs.iter().sum::<f64>() / n;
    let my = ys.iter().sum::<f64>() / n;
    let sxy: f64 = xs.iter().zip(ys).map(|(x, y)| (x - mx) * (y - my)).sum();
    let sxx: f64 = xs.iter().map(|x| (x - mx).powi(2)).sum();
    let b = if sxx > 0.0 { sxy / sxx } else { 0.0 };
    let a = my - b * mx;
    let ss_tot: f64 = ys.iter().map(|y| (y - my).powi(2)).sum();
    let ss_res: f64 = xs
        .iter()
        .zip(ys)
        .map(|(x, y)| (y - (a + b * x)).powi(2))
        .sum();
    (
        a,
        b,
        if ss_tot > 0.0 {
            1.0 - ss_res / ss_tot
        } else {
            1.0
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_stats_match_numpy_and_find_outliers() {
        let d = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 100.0];
        let b = box_stats(&d).expect("stats");
        assert!((b.q1 - 3.25).abs() < 1e-12);
        assert!((b.median - 5.5).abs() < 1e-12);
        assert!((b.q3 - 7.75).abs() < 1e-12);
        assert_eq!(b.outliers, vec![100.0]);
        assert!((b.whisker_high - 9.0).abs() < 1e-12);
        assert!(box_stats(&[]).is_none());
    }

    #[test]
    fn kde_integrates_to_one() {
        let d: Vec<f64> = (0..200)
            .map(|i| (f64::from(i) * 0.37).sin() * 3.0)
            .collect();
        let k = kde(&d, -8.0, 8.0, 400, None);
        let dx = 16.0 / 399.0;
        let area: f64 = k.iter().map(|p| p.1 * dx).sum();
        assert!((area - 1.0).abs() < 0.02, "{area}");
    }

    #[test]
    fn a_violin_is_symmetric_and_as_wide_as_asked() {
        let d = [1.0, 1.2, 1.1, 2.0, 2.2, 3.0, 1.5];
        let v = violin(&d, 10.0, 2.0, 64);
        assert_eq!(v.len(), 128);
        let max_x = v.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
        let min_x = v.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
        assert!((max_x - 12.0).abs() < 1e-9 && (min_x - 8.0).abs() < 1e-9);
    }

    #[test]
    fn heatmap_cells_tile_the_box() {
        let cells = heatmap(&[0.0, 1.0, 2.0, 3.0, 4.0, 5.0], 2, 3, 100.0, 50.0, 2.0);
        assert_eq!(cells.len(), 6);
        let last = cells[5];
        assert!((last.x + last.w - 100.0).abs() < 1e-9 && (last.y + last.h - 50.0).abs() < 1e-9);
        assert!((cells[0].t).abs() < 1e-12 && (last.t - 1.0).abs() < 1e-12);
    }

    #[test]
    fn linear_fit_recovers_a_line() {
        let xs: Vec<f64> = (0..10).map(f64::from).collect();
        let ys: Vec<f64> = xs.iter().map(|x| 2.0 + 3.0 * x).collect();
        let (a, b, r2) = linear_fit(&xs, &ys);
        assert!((a - 2.0).abs() < 1e-9 && (b - 3.0).abs() < 1e-9 && (r2 - 1.0).abs() < 1e-12);
        let (m, e) = mean_and_error(&[1.0, 2.0, 3.0], 1.0);
        assert!((m - 2.0).abs() < 1e-12 && (e - 1.0 / 3f64.sqrt()).abs() < 1e-12);
    }
}
