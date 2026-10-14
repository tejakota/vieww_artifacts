//! Implicit surfaces — Blender's Metaball objects, TouchDesigner's Metaball
//! SOP, Houdini's VDB-from-SDF, and the isosurface extraction under all of
//! them.
//!
//! A [`Field`] is any scalar function of space; its surface is where it
//! crosses `iso`. [`polygonize`] samples the field on a grid and extracts
//! that surface by **marching tetrahedra** (each cube cut into six tetrahedra
//! along its main diagonal, so neighbouring cubes share faces consistently
//! and the result is watertight without marching cubes' ambiguous cases);
//! vertices are shared between cells by edge key, and normals come from the
//! field's gradient, so the mesh shades smoothly.
//!
//! [`Metaballs`] is the classic field: a sum of Wyvill (soft-object)
//! kernels, which blend into each other as they approach — the "blobby"
//! look. [`Sdf`] gives signed distances for spheres, boxes, tori and
//! capsules with smooth unions (Inigo Quilez' polynomial smooth-min).

use crate::{Mesh, Point3};

/// A scalar field.
pub trait Field {
    fn value(&self, p: [f32; 3]) -> f32;

    /// Gradient by central differences (override when analytic).
    fn gradient(&self, p: [f32; 3]) -> [f32; 3] {
        let h = 1e-3;
        let d = |i: usize| {
            let mut a = p;
            let mut b = p;
            a[i] += h;
            b[i] -= h;
            (self.value(a) - self.value(b)) / (2.0 * h)
        };
        [d(0), d(1), d(2)]
    }
}

impl<F: Fn([f32; 3]) -> f32> Field for F {
    fn value(&self, p: [f32; 3]) -> f32 {
        self(p)
    }
}

/// One metaball.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ball {
    pub center: [f32; 3],
    pub radius: f32,
    /// Negative strength subtracts (Blender's "negative" metaball).
    pub strength: f32,
}

/// A sum of Wyvill kernels: each ball contributes
/// `s·(1 − r²/R²)³` inside its radius, 0 outside. The surface is the
/// `iso` crossing — 0.25 to 0.5 gives the familiar blobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Metaballs {
    pub balls: Vec<Ball>,
}

impl Metaballs {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, center: [f32; 3], radius: f32, strength: f32) -> &mut Self {
        self.balls.push(Ball {
            center,
            radius,
            strength,
        });
        self
    }

    /// The axis-aligned box enclosing every ball's influence.
    #[must_use]
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for b in &self.balls {
            for k in 0..3 {
                lo[k] = lo[k].min(b.center[k] - b.radius);
                hi[k] = hi[k].max(b.center[k] + b.radius);
            }
        }
        (lo, hi)
    }
}

impl Field for Metaballs {
    fn value(&self, p: [f32; 3]) -> f32 {
        let mut v = 0.0;
        for b in &self.balls {
            let d2 = (0..3).map(|k| (p[k] - b.center[k]).powi(2)).sum::<f32>();
            let r2 = b.radius * b.radius;
            if d2 < r2 {
                let t = 1.0 - d2 / r2;
                v += b.strength * t * t * t;
            }
        }
        v
    }

    fn gradient(&self, p: [f32; 3]) -> [f32; 3] {
        let mut g = [0.0; 3];
        for b in &self.balls {
            let d: [f32; 3] = [p[0] - b.center[0], p[1] - b.center[1], p[2] - b.center[2]];
            let d2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
            let r2 = b.radius * b.radius;
            if d2 < r2 {
                let t = 1.0 - d2 / r2;
                let k = b.strength * 3.0 * t * t * (-2.0 / r2);
                for i in 0..3 {
                    g[i] += k * d[i];
                }
            }
        }
        g
    }
}

/// Signed-distance primitives and combinators (negative inside).
#[derive(Debug, Clone, PartialEq)]
pub enum Sdf {
    Sphere {
        center: [f32; 3],
        radius: f32,
    },
    Box {
        center: [f32; 3],
        half: [f32; 3],
        round: f32,
    },
    /// Around the y axis through `center`.
    Torus {
        center: [f32; 3],
        major: f32,
        minor: f32,
    },
    Capsule {
        a: [f32; 3],
        b: [f32; 3],
        radius: f32,
    },
    Union(Vec<Sdf>),
    /// Union with polynomial blending over `k`.
    SmoothUnion(Vec<Sdf>, f32),
    /// `.0` minus `.1`.
    Subtract(Box<Sdf>, Box<Sdf>),
    Intersect(Box<Sdf>, Box<Sdf>),
}

fn len(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// Quilez' polynomial smooth minimum.
#[must_use]
pub fn smin(a: f32, b: f32, k: f32) -> f32 {
    if k <= 0.0 {
        return a.min(b);
    }
    let h = (k - (a - b).abs()).max(0.0) / k;
    a.min(b) - h * h * k * 0.25
}

impl Field for Sdf {
    fn value(&self, p: [f32; 3]) -> f32 {
        match self {
            Self::Sphere { center, radius } => {
                len([p[0] - center[0], p[1] - center[1], p[2] - center[2]]) - radius
            }
            Self::Box {
                center,
                half,
                round,
            } => {
                let q: Vec<f32> = (0..3)
                    .map(|k| (p[k] - center[k]).abs() - (half[k] - round))
                    .collect();
                let outside = len([q[0].max(0.0), q[1].max(0.0), q[2].max(0.0)]);
                outside + q[0].max(q[1]).max(q[2]).min(0.0) - round
            }
            Self::Torus {
                center,
                major,
                minor,
            } => {
                let d = [p[0] - center[0], p[1] - center[1], p[2] - center[2]];
                let q = ((d[0] * d[0] + d[2] * d[2]).sqrt() - major, d[1]);
                (q.0 * q.0 + q.1 * q.1).sqrt() - minor
            }
            Self::Capsule { a, b, radius } => {
                let pa = [p[0] - a[0], p[1] - a[1], p[2] - a[2]];
                let ba = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let h = ((pa[0] * ba[0] + pa[1] * ba[1] + pa[2] * ba[2])
                    / (ba[0] * ba[0] + ba[1] * ba[1] + ba[2] * ba[2]).max(1e-12))
                .clamp(0.0, 1.0);
                len([pa[0] - ba[0] * h, pa[1] - ba[1] * h, pa[2] - ba[2] * h]) - radius
            }
            Self::Union(v) => v.iter().map(|s| s.value(p)).fold(f32::INFINITY, f32::min),
            Self::SmoothUnion(v, k) => v
                .iter()
                .map(|s| s.value(p))
                .reduce(|a, b| smin(a, b, *k))
                .unwrap_or(f32::INFINITY),
            Self::Subtract(a, b) => a.value(p).max(-b.value(p)),
            Self::Intersect(a, b) => a.value(p).max(b.value(p)),
        }
    }
}

/// Extract the `iso` surface of `field` over the box `lo..hi` at
/// `resolution` cells along the longest side. With `inside_positive`
/// (metaballs), "inside" is `value > iso`; otherwise (SDFs) `value < iso`.
/// Triangles wind counter-clockwise seen from outside.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn polygonize(
    field: &dyn Field,
    lo: [f32; 3],
    hi: [f32; 3],
    resolution: usize,
    iso: f32,
    inside_positive: bool,
) -> Mesh {
    let ext = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    let longest = ext[0].max(ext[1]).max(ext[2]).max(1e-6);
    #[allow(clippy::cast_precision_loss)]
    let step = longest / resolution.max(2) as f32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let dims = ext.map(|e| ((e / step).ceil() as usize).max(1) + 1);
    let idx = |x: usize, y: usize, z: usize| (z * dims[1] + y) * dims[0] + x;
    #[allow(clippy::cast_precision_loss)]
    let pos = |x: usize, y: usize, z: usize| {
        [
            lo[0] + x as f32 * step,
            lo[1] + y as f32 * step,
            lo[2] + z as f32 * step,
        ]
    };
    let sign = if inside_positive { 1.0 } else { -1.0 };
    let mut vals = vec![0.0f32; dims[0] * dims[1] * dims[2]];
    for z in 0..dims[2] {
        for y in 0..dims[1] {
            for x in 0..dims[0] {
                // Normalised so that "inside" is positive.
                vals[idx(x, y, z)] = sign * (field.value(pos(x, y, z)) - iso);
            }
        }
    }
    let mut mesh = Mesh::default();
    let mut edge_vertex: std::collections::HashMap<(usize, usize), u32> =
        std::collections::HashMap::new();
    // Cube corners (Bourke order) and the six tetrahedra around 0–6.
    const C: [(usize, usize, usize); 8] = [
        (0, 0, 0),
        (1, 0, 0),
        (1, 1, 0),
        (0, 1, 0),
        (0, 0, 1),
        (1, 0, 1),
        (1, 1, 1),
        (0, 1, 1),
    ];
    const TETS: [[usize; 4]; 6] = [
        [0, 5, 1, 6],
        [0, 1, 2, 6],
        [0, 2, 3, 6],
        [0, 3, 7, 6],
        [0, 7, 4, 6],
        [0, 4, 5, 6],
    ];
    let mut vertex =
        |a: usize, b: usize, mesh: &mut Mesh, vals: &[f32], ap: [f32; 3], bp: [f32; 3]| -> u32 {
            let key = (a.min(b), a.max(b));
            if let Some(v) = edge_vertex.get(&key) {
                return *v;
            }
            let (va, vb) = (vals[a], vals[b]);
            let t = (va / (va - vb)).clamp(0.0, 1.0);
            let p = [
                ap[0] + (bp[0] - ap[0]) * t,
                ap[1] + (bp[1] - ap[1]) * t,
                ap[2] + (bp[2] - ap[2]) * t,
            ];
            let g = field.gradient(p);
            let gl = len(g).max(1e-12);
            // Outward normal: against the gradient when inside is larger.
            let n: Point3 = if inside_positive {
                [-g[0] / gl, -g[1] / gl, -g[2] / gl]
            } else {
                [g[0] / gl, g[1] / gl, g[2] / gl]
            };
            #[allow(clippy::cast_possible_truncation)]
            let i = mesh.positions.len() as u32;
            mesh.positions.push(p);
            mesh.normals.push(n);
            edge_vertex.insert(key, i);
            i
        };
    let tri = |mesh: &mut Mesh, a: u32, b: u32, c: u32, outward_hint: [f32; 3]| {
        if a == b || b == c || a == c {
            return;
        }
        let (pa, pb, pc) = (
            mesh.positions[a as usize],
            mesh.positions[b as usize],
            mesh.positions[c as usize],
        );
        let e1 = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let e2 = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
        let n = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        if n[0] * outward_hint[0] + n[1] * outward_hint[1] + n[2] * outward_hint[2] >= 0.0 {
            mesh.indices.extend_from_slice(&[a, b, c]);
        } else {
            mesh.indices.extend_from_slice(&[a, c, b]);
        }
    };
    for z in 0..dims[2] - 1 {
        for y in 0..dims[1] - 1 {
            for x in 0..dims[0] - 1 {
                let corner = C.map(|(dx, dy, dz)| idx(x + dx, y + dy, z + dz));
                let cpos = C.map(|(dx, dy, dz)| pos(x + dx, y + dy, z + dz));
                if corner.iter().all(|i| vals[*i] > 0.0) || corner.iter().all(|i| vals[*i] <= 0.0) {
                    continue;
                }
                for t in TETS {
                    let ins: Vec<usize> = t
                        .iter()
                        .copied()
                        .filter(|k| vals[corner[*k]] > 0.0)
                        .collect();
                    let outs: Vec<usize> = t
                        .iter()
                        .copied()
                        .filter(|k| vals[corner[*k]] <= 0.0)
                        .collect();
                    // Outward = from inside corners toward outside corners.
                    let centroid = |s: &[usize]| {
                        let mut c = [0.0f32; 3];
                        for k in s {
                            for j in 0..3 {
                                c[j] += cpos[*k][j];
                            }
                        }
                        #[allow(clippy::cast_precision_loss)]
                        c.map(|v| v / s.len().max(1) as f32)
                    };
                    let (ci, co) = (centroid(&ins), centroid(&outs));
                    let hint = [co[0] - ci[0], co[1] - ci[1], co[2] - ci[2]];
                    let mut v = |a: usize, b: usize, mesh: &mut Mesh| {
                        vertex(corner[a], corner[b], mesh, &vals, cpos[a], cpos[b])
                    };
                    match (ins.len(), outs.len()) {
                        (1, 3) => {
                            let a = v(ins[0], outs[0], &mut mesh);
                            let b = v(ins[0], outs[1], &mut mesh);
                            let c = v(ins[0], outs[2], &mut mesh);
                            tri(&mut mesh, a, b, c, hint);
                        }
                        (3, 1) => {
                            let a = v(outs[0], ins[0], &mut mesh);
                            let b = v(outs[0], ins[1], &mut mesh);
                            let c = v(outs[0], ins[2], &mut mesh);
                            tri(&mut mesh, a, b, c, hint);
                        }
                        (2, 2) => {
                            let a = v(ins[0], outs[0], &mut mesh);
                            let b = v(ins[0], outs[1], &mut mesh);
                            let c = v(ins[1], outs[1], &mut mesh);
                            let d = v(ins[1], outs[0], &mut mesh);
                            tri(&mut mesh, a, b, c, hint);
                            tri(&mut mesh, a, c, d, hint);
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    mesh
}

impl Metaballs {
    /// The blob surface at `iso`, `resolution` cells along the longest side.
    #[must_use]
    pub fn mesh(&self, iso: f32, resolution: usize) -> Mesh {
        let (lo, hi) = self.bounds();
        let pad = 0.05 * (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]);
        polygonize(
            self,
            lo.map(|v| v - pad),
            hi.map(|v| v + pad),
            resolution,
            iso,
            true,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volume(m: &Mesh) -> f32 {
        let mut v = 0.0;
        for t in m.indices.as_chunks::<3>().0 {
            let (a, b, c) = (
                m.positions[t[0] as usize],
                m.positions[t[1] as usize],
                m.positions[t[2] as usize],
            );
            v += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.0;
        }
        v
    }

    fn is_closed(m: &Mesh) -> bool {
        let mut edges = std::collections::HashMap::new();
        for t in m.indices.as_chunks::<3>().0 {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *edges.entry((a, b)).or_insert(0i32) += 1;
                *edges.entry((b, a)).or_insert(0i32) -= 1;
            }
        }
        edges.values().all(|v| *v == 0)
    }

    #[test]
    fn a_sphere_sdf_polygonizes_closed_with_the_right_volume_and_outward_faces() {
        let s = Sdf::Sphere {
            center: [0.0; 3],
            radius: 1.0,
        };
        let m = polygonize(&s, [-1.3; 3], [1.3; 3], 40, 0.0, false);
        assert!(m.triangles() > 1000);
        assert!(is_closed(&m));
        let v = volume(&m);
        let exact = 4.0 / 3.0 * std::f32::consts::PI;
        assert!((v - exact).abs() / exact < 0.02, "{v}");
        for (p, n) in m.positions.iter().zip(&m.normals) {
            let r = len(*p);
            assert!((r - 1.0).abs() < 0.02);
            assert!(p[0] * n[0] + p[1] * n[1] + p[2] * n[2] > 0.9);
        }
    }

    #[test]
    fn two_metaballs_merge_when_close_and_part_when_far() {
        let count_components = |m: &Mesh| {
            let n = m.positions.len();
            let mut parent: Vec<usize> = (0..n).collect();
            fn find(p: &mut [usize], x: usize) -> usize {
                let mut r = x;
                while p[r] != r {
                    r = p[r];
                }
                p[x] = r;
                r
            }
            for t in m.indices.as_chunks::<3>().0 {
                for k in 1..3 {
                    let (a, b) = (
                        find(&mut parent, t[0] as usize),
                        find(&mut parent, t[k] as usize),
                    );
                    parent[a] = b;
                }
            }
            (0..n).filter(|i| find(&mut parent, *i) == *i).count()
        };
        let mut near = Metaballs::new();
        near.add([-0.6, 0.0, 0.0], 1.2, 1.0)
            .add([0.6, 0.0, 0.0], 1.2, 1.0);
        let mut far = Metaballs::new();
        far.add([-1.6, 0.0, 0.0], 1.2, 1.0)
            .add([1.6, 0.0, 0.0], 1.2, 1.0);
        assert_eq!(count_components(&near.mesh(0.3, 36)), 1);
        assert_eq!(count_components(&far.mesh(0.3, 36)), 2);
        assert!(is_closed(&near.mesh(0.3, 36)));
    }

    #[test]
    fn metaball_gradient_matches_finite_differences() {
        let mut m = Metaballs::new();
        m.add([0.1, 0.2, 0.0], 1.0, 1.0)
            .add([0.5, -0.1, 0.3], 0.8, -0.5);
        let p = [0.3, 0.1, 0.1];
        let g = m.gradient(p);
        let h = 1e-3;
        for k in 0..3 {
            let mut a = p;
            let mut b = p;
            a[k] += h;
            b[k] -= h;
            let fd = (m.value(a) - m.value(b)) / (2.0 * h);
            assert!((fd - g[k]).abs() < 1e-2, "{k}: {fd} vs {}", g[k]);
        }
    }

    #[test]
    fn sdf_combinators_behave() {
        let a = Sdf::Sphere {
            center: [0.0; 3],
            radius: 1.0,
        };
        let b = Sdf::Box {
            center: [1.0, 0.0, 0.0],
            half: [0.5; 3],
            round: 0.0,
        };
        let cut = Sdf::Subtract(Box::new(a.clone()), Box::new(b.clone()));
        assert!(cut.value([0.9, 0.0, 0.0]) > 0.0);
        assert!(cut.value([-0.5, 0.0, 0.0]) < 0.0);
        let smooth = Sdf::SmoothUnion(vec![a.clone(), b.clone()], 0.3);
        assert!(smooth.value([0.0, 0.95, 0.0]) <= Sdf::Union(vec![a, b]).value([0.0, 0.95, 0.0]));
        let t = Sdf::Torus {
            center: [0.0; 3],
            major: 1.0,
            minor: 0.25,
        };
        assert!(t.value([1.0, 0.0, 0.0]) < 0.0 && t.value([0.0; 3]) > 0.0);
        let c = Sdf::Capsule {
            a: [0.0, -1.0, 0.0],
            b: [0.0, 1.0, 0.0],
            radius: 0.3,
        };
        assert!((c.value([0.3, 0.5, 0.0])).abs() < 1e-5);
    }
}
