//! `d3-delaunay`: Delaunay triangulation and the Voronoi diagram.
//!
//! Bowyer–Watson incremental insertion in `f64` with a super-triangle,
//! points inserted in a spatially sorted (Hilbert-free, grid-binned) order
//! so the walk stays local. The Voronoi cell of each site is the polygon of
//! the circumcentres of its incident triangles, in angular order, clipped to
//! a bounding rectangle (Sutherland–Hodgman), so cells on the hull close.
//! (Cells are computed as the intersection of the bisector half-planes of a
//! site's Delaunay neighbours, which is the same polygon and stays exact
//! for hull sites whose circumcentres run to infinity.)
//! [`Delaunay::find`] returns the nearest site — the hover hit-test every
//! scatter plot with thousands of points uses.

use vieww_foundation::{Offset, Path};

/// A triangulation.
#[derive(Debug, Clone, PartialEq)]
pub struct Delaunay {
    pub points: Vec<(f64, f64)>,
    /// Triangles as point-index triples, counter-clockwise.
    pub triangles: Vec<[usize; 3]>,
}

fn circum(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> ((f64, f64), f64) {
    let d = 2.0 * (a.0 * (b.1 - c.1) + b.0 * (c.1 - a.1) + c.0 * (a.1 - b.1));
    if d.abs() < 1e-18 {
        return ((f64::INFINITY, f64::INFINITY), f64::INFINITY);
    }
    let (a2, b2, c2) = (a.0 * a.0 + a.1 * a.1, b.0 * b.0 + b.1 * b.1, c.0 * c.0 + c.1 * c.1);
    let ux = (a2 * (b.1 - c.1) + b2 * (c.1 - a.1) + c2 * (a.1 - b.1)) / d;
    let uy = (a2 * (c.0 - b.0) + b2 * (a.0 - c.0) + c2 * (b.0 - a.0)) / d;
    ((ux, uy), (a.0 - ux).powi(2) + (a.1 - uy).powi(2))
}

fn ccw(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
}

impl Delaunay {
    /// Triangulate `points` (duplicates are kept as sites but add no triangles).
    #[must_use]
    pub fn new(points: &[(f64, f64)]) -> Self {
        let n = points.len();
        if n < 3 {
            return Self {
                points: points.to_vec(),
                triangles: Vec::new(),
            };
        }
        let (mut lo, mut hi) = ((f64::INFINITY, f64::INFINITY), (f64::NEG_INFINITY, f64::NEG_INFINITY));
        for p in points {
            lo = (lo.0.min(p.0), lo.1.min(p.1));
            hi = (hi.0.max(p.0), hi.1.max(p.1));
        }
        let span = (hi.0 - lo.0).max(hi.1 - lo.1).max(1e-9);
        let mid = ((lo.0 + hi.0) * 0.5, (lo.1 + hi.1) * 0.5);
        let mut pts = points.to_vec();
        pts.push((mid.0 - 20.0 * span, mid.1 - span));
        pts.push((mid.0 + 20.0 * span, mid.1 - span));
        pts.push((mid.0, mid.1 + 20.0 * span));
        let (s0, s1, s2) = (n, n + 1, n + 2);
        // (triangle, circumcentre, r²)
        let mut tris: Vec<([usize; 3], (f64, f64), f64)> = {
            let (c, r) = circum(pts[s0], pts[s1], pts[s2]);
            vec![([s0, s1, s2], c, r)]
        };
        // Insert in grid-binned order.
        let mut order: Vec<usize> = (0..n).collect();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let cell = |p: (f64, f64)| -> (i64, i64) {
            let g = 16.0;
            (((p.0 - lo.0) / span * g) as i64, ((p.1 - lo.1) / span * g) as i64)
        };
        order.sort_by_key(|&i| {
            let (cx, cy) = cell(points[i]);
            (cy, if cy % 2 == 0 { cx } else { -cx })
        });
        let mut seen = std::collections::HashSet::new();
        for i in order {
            let p = pts[i];
            #[allow(clippy::cast_possible_truncation)]
            if !seen.insert(((p.0 * 1e9) as i64, (p.1 * 1e9) as i64)) {
                continue;
            }
            let mut bad = Vec::new();
            let mut keep = Vec::with_capacity(tris.len());
            for t in tris {
                if (p.0 - t.1 .0).powi(2) + (p.1 - t.1 .1).powi(2) <= t.2 * (1.0 + 1e-12) {
                    bad.push(t.0);
                } else {
                    keep.push(t);
                }
            }
            tris = keep;
            // Boundary edges of the cavity: edges that appear once.
            let mut edges: Vec<(usize, usize)> = Vec::new();
            for t in &bad {
                for k in 0..3 {
                    let e = (t[k], t[(k + 1) % 3]);
                    if let Some(pos) = edges.iter().position(|&(a, b)| a == e.1 && b == e.0) {
                        edges.swap_remove(pos);
                    } else {
                        edges.push(e);
                    }
                }
            }
            for (a, b) in edges {
                let (c, r) = circum(pts[a], pts[b], p);
                if r.is_finite() {
                    tris.push(([a, b, i], c, r));
                }
            }
        }
        let triangles = tris
            .into_iter()
            .map(|t| t.0)
            .filter(|t| t.iter().all(|&v| v < n))
            .map(|t| {
                if ccw(points[t[0]], points[t[1]], points[t[2]]) < 0.0 {
                    [t[0], t[2], t[1]]
                } else {
                    t
                }
            })
            .collect();
        Self {
            points: points.to_vec(),
            triangles,
        }
    }

    /// Index of the site nearest `(x, y)`.
    #[must_use]
    pub fn find(&self, x: f64, y: f64) -> Option<usize> {
        self.points
            .iter()
            .enumerate()
            .min_by(|a, b| {
                let d = |p: &(f64, f64)| (p.0 - x).powi(2) + (p.1 - y).powi(2);
                d(a.1).total_cmp(&d(b.1))
            })
            .map(|(i, _)| i)
    }

    /// Unique undirected edges.
    #[must_use]
    pub fn edges(&self) -> Vec<(usize, usize)> {
        let mut e: Vec<(usize, usize)> = self
            .triangles
            .iter()
            .flat_map(|t| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])])
            .map(|(a, b)| (a.min(b), a.max(b)))
            .collect();
        e.sort_unstable();
        e.dedup();
        e
    }

    /// Voronoi cells clipped to `bounds = (x0, y0, x1, y1)`, one per site.
    #[must_use]
    pub fn voronoi(&self, bounds: (f64, f64, f64, f64)) -> Vec<Vec<(f64, f64)>> {
        let n = self.points.len();
        let mut inc: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (ti, t) in self.triangles.iter().enumerate() {
            for &v in t {
                inc[v].push(ti);
            }
        }
        let (x0, y0, x1, y1) = bounds;
        (0..n)
            .map(|i| {
                let p = self.points[i];
                // The cell is the intersection of half-planes towards p from
                // every Delaunay neighbour — robust for hull sites too.
                let mut poly = vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
                let mut nbrs: Vec<usize> = inc[i]
                    .iter()
                    .flat_map(|&t| self.triangles[t])
                    .filter(|&v| v != i)
                    .collect();
                nbrs.sort_unstable();
                nbrs.dedup();
                if inc[i].is_empty() {
                    // Degenerate input: fall back to all sites.
                    nbrs = (0..n).filter(|&v| v != i).collect();
                }
                for q in nbrs.iter().map(|&j| self.points[j]) {
                    let m = ((p.0 + q.0) * 0.5, (p.1 + q.1) * 0.5);
                    let nrm = (q.0 - p.0, q.1 - p.1);
                    poly = clip(&poly, m, nrm);
                    if poly.is_empty() {
                        break;
                    }
                }
                poly
            })
            .collect()
    }
}

/// Keep the part of `poly` with `(x − m)·n ≤ 0`.
fn clip(poly: &[(f64, f64)], m: (f64, f64), n: (f64, f64)) -> Vec<(f64, f64)> {
    let side = |p: (f64, f64)| (p.0 - m.0) * n.0 + (p.1 - m.1) * n.1;
    let mut out = Vec::with_capacity(poly.len() + 1);
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let (sa, sb) = (side(a), side(b));
        if sa <= 0.0 {
            out.push(a);
        }
        if (sa <= 0.0) != (sb <= 0.0) {
            let t = sa / (sa - sb);
            out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
        }
    }
    out
}

/// A cell as a closed path.
#[must_use]
pub fn cell_path(cell: &[(f64, f64)]) -> Path {
    let mut p = Path::new();
    for (k, &(x, y)) in cell.iter().enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        let o = Offset::new(x as f32, y as f32);
        if k == 0 {
            p.move_to(o);
        } else {
            p.line_to(o);
        }
    }
    if !cell.is_empty() {
        p.close();
    }
    p
}

/// Shoelace area.
#[must_use]
pub fn polygon_area(poly: &[(f64, f64)]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            a.0 * b.1 - b.0 * a.1
        })
        .sum::<f64>()
        * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cloud(n: usize) -> Vec<(f64, f64)> {
        let mut s = 7u64;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                #[allow(clippy::cast_precision_loss)]
                let a = (s >> 11) as f64 / (1u64 << 53) as f64;
                s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                #[allow(clippy::cast_precision_loss)]
                let b = (s >> 11) as f64 / (1u64 << 53) as f64;
                (a * 100.0, b * 80.0)
            })
            .collect()
    }

    #[test]
    fn delaunay_has_the_empty_circle_property() {
        let pts = cloud(150);
        let d = Delaunay::new(&pts);
        // Euler: a triangulation of n points with h on the hull has 2n − 2 − h triangles.
        assert!(d.triangles.len() > 250 && d.triangles.len() < 300, "{}", d.triangles.len());
        for t in &d.triangles {
            let (c, r) = circum(pts[t[0]], pts[t[1]], pts[t[2]]);
            for (i, p) in pts.iter().enumerate() {
                if t.contains(&i) {
                    continue;
                }
                assert!((p.0 - c.0).powi(2) + (p.1 - c.1).powi(2) >= r * (1.0 - 1e-9));
            }
            assert!(ccw(pts[t[0]], pts[t[1]], pts[t[2]]) > 0.0);
        }
    }

    #[test]
    fn voronoi_cells_tile_the_bounds() {
        let pts = cloud(60);
        let d = Delaunay::new(&pts);
        let cells = d.voronoi((0.0, 0.0, 100.0, 80.0));
        let total: f64 = cells.iter().map(|c| polygon_area(c).abs()).sum();
        assert!((total - 8000.0).abs() < 1e-6 * 8000.0 + 1e-6, "{total}");
        // Every cell contains its own site, and `find` agrees with the cell.
        for (i, c) in cells.iter().enumerate() {
            let p = pts[i];
            let inside = (0..c.len()).all(|k| ccw(c[k], c[(k + 1) % c.len()], p) >= -1e-9);
            assert!(inside, "site {i}");
            assert_eq!(d.find(p.0, p.1), Some(i));
        }
        assert!(!cell_path(&cells[0]).is_empty());
    }

    #[test]
    fn small_and_degenerate_inputs() {
        assert!(Delaunay::new(&[(0.0, 0.0), (1.0, 1.0)]).triangles.is_empty());
        let sq = Delaunay::new(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]);
        assert_eq!(sq.triangles.len(), 2);
        assert_eq!(sq.edges().len(), 5);
        let col = Delaunay::new(&[(0.0, 0.0), (1.0, 0.0), (2.0, 0.0)]);
        assert!(col.triangles.is_empty(), "collinear points have no triangles");
    }
}
