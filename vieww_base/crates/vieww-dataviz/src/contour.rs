//! Contours — `d3-contour`: iso-lines of a sampled field by marching
//! squares, with linear interpolation along cell edges and the two saddle
//! cases resolved by the cell's centre value.

use vieww_foundation::{Offset, Path};

/// Iso-line segments of `values` (row-major, `cols × rows`, one sample per
/// grid point) at `threshold`, in grid units (multiply by your cell size).
#[must_use]
pub fn isoline(values: &[f32], cols: usize, rows: usize, threshold: f32) -> Vec<(Offset, Offset)> {
    let mut out = Vec::new();
    if cols < 2 || rows < 2 || values.len() < cols * rows {
        return out;
    }
    let v = |x: usize, y: usize| values[y * cols + x];
    #[allow(clippy::cast_precision_loss)]
    for y in 0..rows - 1 {
        for x in 0..cols - 1 {
            let (a, b, c, d) = (v(x, y), v(x + 1, y), v(x + 1, y + 1), v(x, y + 1));
            let case = usize::from(a >= threshold)
                | usize::from(b >= threshold) << 1
                | usize::from(c >= threshold) << 2
                | usize::from(d >= threshold) << 3;
            if case == 0 || case == 15 {
                continue;
            }
            let (fx, fy) = (x as f32, y as f32);
            let lerp = |p: f32, q: f32| {
                if (q - p).abs() < 1e-12 {
                    0.5
                } else {
                    (threshold - p) / (q - p)
                }
            };
            let top = Offset::new(fx + lerp(a, b), fy);
            let right = Offset::new(fx + 1.0, fy + lerp(b, c));
            let bottom = Offset::new(fx + lerp(d, c), fy + 1.0);
            let left = Offset::new(fx, fy + lerp(a, d));
            let centre_high = (a + b + c + d) / 4.0 >= threshold;
            let segs: &[(Offset, Offset)] = match case {
                1 | 14 => &[(left, top)],
                2 | 13 => &[(top, right)],
                3 | 12 => &[(left, right)],
                4 | 11 => &[(right, bottom)],
                6 | 9 => &[(top, bottom)],
                7 | 8 => &[(left, bottom)],
                5 => {
                    if centre_high {
                        &[(left, bottom), (top, right)]
                    } else {
                        &[(left, top), (right, bottom)]
                    }
                }
                10 => {
                    if centre_high {
                        &[(left, top), (right, bottom)]
                    } else {
                        &[(left, bottom), (top, right)]
                    }
                }
                _ => &[],
            };
            out.extend_from_slice(segs);
        }
    }
    out
}

/// The segments as one path, scaled by `cell` and offset by `origin`.
#[must_use]
pub fn isoline_path(
    values: &[f32],
    cols: usize,
    rows: usize,
    threshold: f32,
    cell: f32,
    origin: Offset,
) -> Path {
    let mut p = Path::new();
    for (a, b) in isoline(values, cols, rows, threshold) {
        p.move_to(origin + a.scale(cell));
        p.line_to(origin + b.scale(cell));
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cone_gives_a_ring_near_the_right_radius() {
        let n = 41;
        let mut vals = Vec::new();
        for y in 0..n {
            for x in 0..n {
                #[allow(clippy::cast_precision_loss)]
                let (dx, dy) = (x as f32 - 20.0, y as f32 - 20.0);
                vals.push((dx * dx + dy * dy).sqrt());
            }
        }
        let segs = isoline(&vals, n, n, 10.0);
        assert!(!segs.is_empty());
        for (a, b) in segs {
            for p in [a, b] {
                let r = (p - Offset::new(20.0, 20.0)).distance();
                assert!((r - 10.0).abs() < 0.1, "{r}");
            }
        }
    }

    #[test]
    fn flat_fields_have_no_contour() {
        assert!(isoline(&[1.0; 16], 4, 4, 0.5).is_empty());
        assert!(isoline(&[0.0; 16], 4, 4, 0.5).is_empty());
    }
}
