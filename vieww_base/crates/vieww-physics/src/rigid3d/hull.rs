//! Convex polyhedra: construction (incremental hull with coplanar-face
//! merging), support mapping, mass properties, and plane clipping — the
//! geometry under the 3D engine's boxes, convex hulls and fracture pieces.

use super::math::{M3, V3};

/// A planar face: vertex indices counter-clockwise seen from outside, its
/// outward unit normal and plane offset (`normal · x = d` on the face).
#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    pub verts: Vec<usize>,
    pub normal: V3,
    pub d: f32,
}

/// A convex polyhedron.
#[derive(Debug, Clone, PartialEq)]
pub struct Hull {
    pub verts: Vec<V3>,
    pub faces: Vec<Face>,
    /// Unique undirected edges.
    pub edges: Vec<(usize, usize)>,
}

/// Mass properties about the centre of mass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MassProps {
    pub volume: f32,
    pub centroid: V3,
    /// Inertia tensor per unit density, about the centroid.
    pub inertia: M3,
}

impl Hull {
    /// An axis-aligned box with the given half extents, centred at origin.
    #[must_use]
    pub fn cuboid(h: V3) -> Self {
        let mut pts = Vec::with_capacity(8);
        for &x in &[-h.x, h.x] {
            for &y in &[-h.y, h.y] {
                for &z in &[-h.z, h.z] {
                    pts.push(V3::new(x, y, z));
                }
            }
        }
        Self::from_points(&pts).expect("a box is a solid")
    }

    /// The convex hull of `points`, or `None` if they are (nearly) flat.
    #[must_use]
    pub fn from_points(points: &[V3]) -> Option<Self> {
        // Deduplicate close points.
        let mut lo = V3::splat(f32::INFINITY);
        let mut hi = V3::splat(f32::NEG_INFINITY);
        for p in points {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        let scale = (hi - lo).length().max(1e-6);
        let eps = scale * 1e-5;
        let mut pts: Vec<V3> = Vec::new();
        for p in points {
            if !pts.iter().any(|q| (*q - *p).length() < eps * 4.0) {
                pts.push(*p);
            }
        }
        if pts.len() < 4 {
            return None;
        }
        // Initial tetrahedron: two extreme points, the farthest from their
        // line, the farthest from their plane.
        let i0 = 0;
        let i1 = (0..pts.len()).max_by(|&a, &b| {
            (pts[a] - pts[i0]).length_sq().total_cmp(&(pts[b] - pts[i0]).length_sq())
        })?;
        let line = (pts[i1] - pts[i0]).normalize();
        let i2 = (0..pts.len()).max_by(|&a, &b| {
            let da = (pts[a] - pts[i0]).cross(line).length_sq();
            let db = (pts[b] - pts[i0]).cross(line).length_sq();
            da.total_cmp(&db)
        })?;
        let n = (pts[i1] - pts[i0]).cross(pts[i2] - pts[i0]);
        if n.length() < eps * scale {
            return None;
        }
        let n = n.normalize();
        let i3 = (0..pts.len()).max_by(|&a, &b| {
            (pts[a] - pts[i0]).dot(n).abs().total_cmp(&(pts[b] - pts[i0]).dot(n).abs())
        })?;
        if (pts[i3] - pts[i0]).dot(n).abs() < eps * 10.0 {
            return None;
        }
        // Triangles, oriented outward from the tetra's centroid.
        let c = (pts[i0] + pts[i1] + pts[i2] + pts[i3]) * 0.25;
        let mut tris: Vec<[usize; 3]> = Vec::new();
        let push = |tris: &mut Vec<[usize; 3]>, a: usize, b: usize, cc: usize| {
            let nn = (pts[b] - pts[a]).cross(pts[cc] - pts[a]);
            if nn.dot(pts[a] - c) < 0.0 {
                tris.push([a, cc, b]);
            } else {
                tris.push([a, b, cc]);
            }
        };
        push(&mut tris, i0, i1, i2);
        push(&mut tris, i0, i1, i3);
        push(&mut tris, i0, i2, i3);
        push(&mut tris, i1, i2, i3);
        let tri_plane = |t: &[usize; 3]| {
            let nn = (pts[t[1]] - pts[t[0]]).cross(pts[t[2]] - pts[t[0]]).normalize();
            (nn, nn.dot(pts[t[0]]))
        };
        for (pi, p) in pts.iter().enumerate() {
            if [i0, i1, i2, i3].contains(&pi) {
                continue;
            }
            let visible: Vec<bool> = tris
                .iter()
                .map(|t| {
                    let (nn, d) = tri_plane(t);
                    nn.dot(*p) - d > eps
                })
                .collect();
            if !visible.iter().any(|v| *v) {
                continue;
            }
            // Horizon: directed edges of visible triangles whose twin is in
            // a non-visible triangle.
            let mut horizon = Vec::new();
            for (ti, t) in tris.iter().enumerate() {
                if !visible[ti] {
                    continue;
                }
                for k in 0..3 {
                    let (a, b) = (t[k], t[(k + 1) % 3]);
                    let twin_visible = tris.iter().enumerate().any(|(tj, u)| {
                        visible[tj] && (0..3).any(|m| u[m] == b && u[(m + 1) % 3] == a)
                    });
                    if !twin_visible {
                        horizon.push((a, b));
                    }
                }
            }
            let mut kept: Vec<[usize; 3]> = tris
                .iter()
                .zip(&visible)
                .filter(|(_, v)| !**v)
                .map(|(t, _)| *t)
                .collect();
            for (a, b) in horizon {
                kept.push([a, b, pi]);
            }
            tris = kept;
        }
        Some(Self::from_triangles(&pts, &tris, eps))
    }

    fn from_triangles(pts: &[V3], tris: &[[usize; 3]], eps: f32) -> Self {
        // Group coplanar triangles.
        let planes: Vec<(V3, f32)> = tris
            .iter()
            .map(|t| {
                let n = (pts[t[1]] - pts[t[0]]).cross(pts[t[2]] - pts[t[0]]).normalize();
                (n, n.dot(pts[t[0]]))
            })
            .collect();
        let mut group = vec![usize::MAX; tris.len()];
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for i in 0..tris.len() {
            if group[i] != usize::MAX {
                continue;
            }
            let g = groups.len();
            group[i] = g;
            let mut members = vec![i];
            for j in i + 1..tris.len() {
                if group[j] == usize::MAX
                    && planes[i].0.dot(planes[j].0) > 1.0 - 1e-4
                    && (planes[i].1 - planes[j].1).abs() < eps * 20.0
                {
                    group[j] = g;
                    members.push(j);
                }
            }
            groups.push(members);
        }
        // Each group's boundary loop.
        let mut used = vec![false; pts.len()];
        let mut faces_raw: Vec<(Vec<usize>, V3)> = Vec::new();
        for members in &groups {
            let mut edges: Vec<(usize, usize)> = Vec::new();
            for &ti in members {
                let t = tris[ti];
                for k in 0..3 {
                    edges.push((t[k], t[(k + 1) % 3]));
                }
            }
            let boundary: Vec<(usize, usize)> = edges
                .iter()
                .filter(|(a, b)| !edges.contains(&(*b, *a)))
                .copied()
                .collect();
            let mut loop_v = Vec::new();
            if let Some(&(start, mut next)) = boundary.first() {
                loop_v.push(start);
                let mut guard = 0;
                while next != start && guard < boundary.len() + 2 {
                    loop_v.push(next);
                    next = boundary
                        .iter()
                        .find(|(a, _)| *a == next)
                        .map_or(start, |e| e.1);
                    guard += 1;
                }
            }
            // Drop collinear vertices.
            let mut simplified = Vec::new();
            let m = loop_v.len();
            for k in 0..m {
                let (p0, p1, p2) = (pts[loop_v[(k + m - 1) % m]], pts[loop_v[k]], pts[loop_v[(k + 1) % m]]);
                if (p1 - p0).cross(p2 - p1).length() > eps * eps * 10.0 {
                    simplified.push(loop_v[k]);
                }
            }
            if simplified.len() >= 3 {
                for &v in &simplified {
                    used[v] = true;
                }
                faces_raw.push((simplified, planes[members[0]].0));
            }
        }
        // Compact vertices.
        let mut remap = vec![usize::MAX; pts.len()];
        let mut verts = Vec::new();
        for (i, u) in used.iter().enumerate() {
            if *u {
                remap[i] = verts.len();
                verts.push(pts[i]);
            }
        }
        let mut faces = Vec::new();
        let mut edges = Vec::new();
        for (loop_v, _) in faces_raw {
            let idx: Vec<usize> = loop_v.iter().map(|v| remap[*v]).collect();
            // Newell normal for robustness.
            let mut n = V3::ZERO;
            for k in 0..idx.len() {
                let (a, b) = (verts[idx[k]], verts[idx[(k + 1) % idx.len()]]);
                n += V3::new(
                    (a.y - b.y) * (a.z + b.z),
                    (a.z - b.z) * (a.x + b.x),
                    (a.x - b.x) * (a.y + b.y),
                );
            }
            let n = n.normalize();
            let d = idx.iter().map(|i| n.dot(verts[*i])).sum::<f32>() / idx.len() as f32;
            for k in 0..idx.len() {
                let (a, b) = (idx[k], idx[(k + 1) % idx.len()]);
                let e = (a.min(b), a.max(b));
                if !edges.contains(&e) {
                    edges.push(e);
                }
            }
            faces.push(Face { verts: idx, normal: n, d });
        }
        Self {
            verts,
            faces,
            edges,
        }
    }

    /// The vertex furthest along `dir`.
    #[must_use]
    pub fn support(&self, dir: V3) -> V3 {
        let mut best = self.verts[0];
        let mut bd = best.dot(dir);
        for v in &self.verts[1..] {
            let d = v.dot(dir);
            if d > bd {
                bd = d;
                best = *v;
            }
        }
        best
    }

    /// Volume, centroid and unit-density inertia about the centroid.
    #[must_use]
    pub fn mass_props(&self) -> MassProps {
        let mut vol = 0.0;
        let mut first = V3::ZERO;
        let mut cov = M3::ZERO;
        // Canonical tetra covariance (Blow & Binstock).
        let canon = M3 {
            m: [[2.0, 1.0, 1.0], [1.0, 2.0, 1.0], [1.0, 1.0, 2.0]],
        }
        .scale(1.0 / 120.0);
        for f in &self.faces {
            let a = self.verts[f.verts[0]];
            for k in 1..f.verts.len() - 1 {
                let b = self.verts[f.verts[k]];
                let c = self.verts[f.verts[k + 1]];
                let am = M3 {
                    m: [[a.x, b.x, c.x], [a.y, b.y, c.y], [a.z, b.z, c.z]],
                };
                let det = am.determinant();
                vol += det / 6.0;
                first += (a + b + c) * (det / 24.0);
                cov = cov.add(&am.mul(&canon).mul(&am.transpose()).scale(det));
            }
        }
        let centroid = if vol.abs() > 1e-12 { first / vol } else { V3::ZERO };
        // Shift covariance to the centroid: C' = C − V c cᵀ.
        let cc = M3 {
            m: [
                [centroid.x * centroid.x, centroid.x * centroid.y, centroid.x * centroid.z],
                [centroid.y * centroid.x, centroid.y * centroid.y, centroid.y * centroid.z],
                [centroid.z * centroid.x, centroid.z * centroid.y, centroid.z * centroid.z],
            ],
        };
        let c = cov.add(&cc.scale(-vol));
        let tr = c.m[0][0] + c.m[1][1] + c.m[2][2];
        let inertia = M3::diagonal(V3::splat(tr)).add(&c.scale(-1.0));
        MassProps {
            volume: vol,
            centroid,
            inertia,
        }
    }

    /// Every vertex moved by `t`.
    #[must_use]
    pub fn translated(&self, t: V3) -> Self {
        let mut h = self.clone();
        for v in &mut h.verts {
            *v += t;
        }
        for f in &mut h.faces {
            f.d += f.normal.dot(t);
        }
        h
    }

    /// The part on the side `n · x ≤ d`, or `None` if nothing solid remains.
    #[must_use]
    pub fn clip(&self, n: V3, d: f32) -> Option<Self> {
        let side = |p: V3| n.dot(p) - d;
        let mut out: Vec<V3> = self.verts.iter().filter(|p| side(**p) <= 0.0).copied().collect();
        if out.len() == self.verts.len() {
            return Some(self.clone());
        }
        for &(a, b) in &self.edges {
            let (pa, pb) = (self.verts[a], self.verts[b]);
            let (sa, sb) = (side(pa), side(pb));
            if (sa < 0.0 && sb > 0.0) || (sa > 0.0 && sb < 0.0) {
                out.push(pa + (pb - pa) * (sa / (sa - sb)));
            }
        }
        Self::from_points(&out)
    }

    /// Is `p` inside (or on) the hull?
    #[must_use]
    pub fn contains(&self, p: V3) -> bool {
        self.faces.iter().all(|f| f.normal.dot(p) - f.d <= 1e-5)
    }

    /// Triangles (fan per face) for rendering: positions and indices.
    #[must_use]
    pub fn triangles(&self) -> (Vec<V3>, Vec<u32>) {
        let mut idx = Vec::new();
        for f in &self.faces {
            for k in 1..f.verts.len() - 1 {
                #[allow(clippy::cast_possible_truncation)]
                idx.extend_from_slice(&[f.verts[0] as u32, f.verts[k] as u32, f.verts[k + 1] as u32]);
            }
        }
        (self.verts.clone(), idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cube_has_eight_corners_six_quads_twelve_edges() {
        let h = Hull::cuboid(V3::new(1.0, 2.0, 3.0));
        assert_eq!(h.verts.len(), 8);
        assert_eq!(h.faces.len(), 6);
        assert!(h.faces.iter().all(|f| f.verts.len() == 4));
        assert_eq!(h.edges.len(), 12);
        let mp = h.mass_props();
        assert!((mp.volume - 48.0).abs() < 1e-3);
        assert!(mp.centroid.length() < 1e-4);
        // Box inertia per unit density: V/12 (b²+c²) with full sides 2,4,6.
        let ix = 48.0 / 12.0 * (16.0 + 36.0);
        assert!((mp.inertia.m[0][0] - ix).abs() < 0.05, "{:?}", mp.inertia);
        assert!(mp.inertia.m[0][1].abs() < 1e-3);
    }

    #[test]
    fn interior_points_do_not_change_the_hull() {
        let mut pts = vec![];
        for i in 0..50 {
            #[allow(clippy::cast_precision_loss)]
            let t = i as f32 * 0.37;
            pts.push(V3::new(t.sin() * 0.5, (t * 1.3).cos() * 0.5, (t * 0.7).sin() * 0.5));
        }
        pts.extend(Hull::cuboid(V3::splat(1.0)).verts);
        let h = Hull::from_points(&pts).expect("hull");
        assert_eq!(h.verts.len(), 8);
        assert_eq!(h.faces.len(), 6);
    }

    #[test]
    fn euler_holds_for_a_random_hull() {
        let mut pts = vec![];
        let mut s = 12345u32;
        for _ in 0..80 {
            let mut r = || {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                #[allow(clippy::cast_precision_loss)]
                let v = (s >> 8) as f32 / (1u32 << 24) as f32;
                v * 2.0 - 1.0
            };
            pts.push(V3::new(r(), r(), r()));
        }
        let h = Hull::from_points(&pts).expect("hull");
        let (v, e, f) = (h.verts.len() as i64, h.edges.len() as i64, h.faces.len() as i64);
        assert_eq!(v - e + f, 2);
        for p in &pts {
            assert!(h.contains(*p));
        }
    }

    #[test]
    fn clipping_a_cube_in_half_halves_its_volume() {
        let h = Hull::cuboid(V3::splat(1.0));
        let half = h.clip(V3::X, 0.0).expect("half");
        assert!((half.mass_props().volume - 4.0).abs() < 1e-3);
        let diag = h.clip(V3::new(1.0, 1.0, 1.0).normalize(), 0.0).expect("diag");
        assert!((diag.mass_props().volume - 4.0).abs() < 1e-3);
        assert!(h.clip(V3::X, -2.0).is_none());
    }

    #[test]
    fn flat_input_is_refused() {
        let pts = [V3::ZERO, V3::X, V3::Y, V3::new(1.0, 1.0, 0.0)];
        assert!(Hull::from_points(&pts).is_none());
    }
}
