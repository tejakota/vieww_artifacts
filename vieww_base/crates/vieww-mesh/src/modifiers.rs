//! The modifier stack — Blender's L5 (§2.14): "Base mesh → modifiers applied
//! in stack order → each modifier reads input mesh, outputs modified mesh".
//!
//! Blender ships 180+ modifiers; this is the working set every modelling
//! session reaches for, each a pure `Mesh → Mesh` function so a stack is
//! a fold and every intermediate is inspectable:
//!
//! | modifier | what it does |
//! |---|---|
//! | [`Modifier::Transform`] | translate / scale / rotate about Y |
//! | [`Modifier::Mirror`] | reflect across an axis plane and keep both halves, welding the seam |
//! | [`Modifier::Array`] | repeat with a constant offset |
//! | [`Modifier::Subdivide`] | split every triangle in four; `smooth` runs **Loop subdivision** (Loop 1987, with Warren's weights and crease-free boundary rules) |
//! | [`Modifier::Displace`] | push vertices along their normals by seeded 3D value noise |
//! | [`Modifier::Twist`] / [`Modifier::Bend`] / [`Modifier::Taper`] | the Simple Deform family, along Y |
//! | [`Modifier::Smooth`] | Laplacian relaxation |
//! | [`Modifier::Decimate`] | vertex-clustering simplification to a grid cell size |
//! | [`Modifier::Weld`] | merge vertices closer than a distance |
//! | [`Modifier::FlipNormals`] | reverse winding and normals |
//!
//! Topological modifiers (subdivide-smooth, smooth, decimate, weld) work on
//! **welded** positions, so a mesh whose vertices were split for UV seams
//! still subdivides as one surface; they drop UVs, which is stated rather
//! than faked. Normals are recomputed after any modifier that moves
//! geometry.

use std::collections::HashMap;

use crate::Mesh;

/// An axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    const fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
            Self::Z => 2,
        }
    }
}

/// One step of a stack.
#[derive(Debug, Clone, PartialEq)]
pub enum Modifier {
    Transform {
        translate: [f32; 3],
        scale: [f32; 3],
        rotate_y: f32,
    },
    Mirror {
        axis: Axis,
        merge: f32,
    },
    Array {
        count: usize,
        offset: [f32; 3],
    },
    Subdivide {
        levels: u32,
        smooth: bool,
    },
    Displace {
        strength: f32,
        scale: f32,
        seed: u32,
    },
    /// Rotate about Y by `angle` radians per unit of height.
    Twist {
        angle: f32,
    },
    /// Bend the Y axis into an arc of `angle` radians over the mesh's height.
    Bend {
        angle: f32,
    },
    /// Scale XZ from 1 at the bottom to `factor` at the top.
    Taper {
        factor: f32,
    },
    Smooth {
        factor: f32,
        iterations: u32,
    },
    Decimate {
        cell: f32,
    },
    Weld {
        distance: f32,
    },
    FlipNormals,
}

/// Modifiers in order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModifierStack {
    pub modifiers: Vec<Modifier>,
}

impl ModifierStack {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with(mut self, m: Modifier) -> Self {
        self.modifiers.push(m);
        self
    }

    /// Evaluate the stack over `base`.
    #[must_use]
    pub fn apply(&self, base: &Mesh) -> Mesh {
        self.modifiers
            .iter()
            .fold(base.clone(), |m, md| md.apply(&m))
    }
}

fn map_positions(mesh: &Mesh, f: impl Fn([f32; 3]) -> [f32; 3]) -> Mesh {
    let mut out = mesh.clone();
    for p in &mut out.positions {
        *p = f(*p);
    }
    out.compute_normals();
    out
}

fn y_range(mesh: &Mesh) -> (f32, f32) {
    mesh.bounds().map_or((0.0, 1.0), |(lo, hi)| (lo[1], hi[1]))
}

/// An axis-aligned cube of edge `size` centred on the origin: 8 shared
/// corners, 12 triangles wound counter-clockwise seen from outside, normals
/// computed — the closed solid booleans and modifiers start from.
#[must_use]
pub fn cube(size: f32) -> Mesh {
    let h = size * 0.5;
    let mut m = Mesh {
        positions: vec![
            [-h, -h, -h],
            [h, -h, -h],
            [h, h, -h],
            [-h, h, -h],
            [-h, -h, h],
            [h, -h, h],
            [h, h, h],
            [-h, h, h],
        ],
        indices: vec![
            0, 3, 2, 0, 2, 1, // -z
            4, 5, 6, 4, 6, 7, // +z
            0, 1, 5, 0, 5, 4, // -y
            3, 7, 6, 3, 6, 2, // +y
            0, 4, 7, 0, 7, 3, // -x
            1, 2, 6, 1, 6, 5, // +x
        ],
        ..Mesh::default()
    };
    m.compute_normals();
    m
}

/// Merge vertices within `distance` (grid-hashed); attributes of the first
/// vertex win; UVs are dropped because merged vertices may disagree.
#[must_use]
pub fn weld(mesh: &Mesh, distance: f32) -> Mesh {
    let cell = distance.max(1e-6);
    let key = |p: [f32; 3]| {
        #[allow(clippy::cast_possible_truncation)]
        let k = |v: f32| (v / cell).round() as i64;
        (k(p[0]), k(p[1]), k(p[2]))
    };
    let mut map: HashMap<(i64, i64, i64), u32> = HashMap::new();
    let mut out = Mesh::default();
    let mut remap = Vec::with_capacity(mesh.positions.len());
    for p in &mesh.positions {
        let k = key(*p);
        #[allow(clippy::cast_possible_truncation)]
        let next = out.positions.len() as u32;
        let i = *map.entry(k).or_insert_with(|| {
            out.positions.push(*p);
            next
        });
        remap.push(i);
    }
    for t in mesh.indices.as_chunks::<3>().0 {
        let (a, b, c) = (
            remap[t[0] as usize],
            remap[t[1] as usize],
            remap[t[2] as usize],
        );
        if a != b && b != c && a != c {
            out.indices.extend_from_slice(&[a, b, c]);
        }
    }
    out.compute_normals();
    out
}

fn hash3(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    #[allow(clippy::cast_sign_loss)]
    let mut h = (x as u32).wrapping_mul(0x8da6_b343)
        ^ (y as u32).wrapping_mul(0xd816_3841)
        ^ (z as u32).wrapping_mul(0xcb1a_b31f)
        ^ seed;
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    #[allow(clippy::cast_precision_loss)]
    let r = (h & 0x00ff_ffff) as f32 / 16_777_215.0;
    r * 2.0 - 1.0
}

/// Trilinear value noise in −1..1.
fn value_noise(p: [f32; 3], seed: u32) -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    let f = p.map(|v| v.floor() as i32);
    let t = [
        p[0] - p[0].floor(),
        p[1] - p[1].floor(),
        p[2] - p[2].floor(),
    ]
    .map(|v| v * v * (3.0 - 2.0 * v));
    let mut acc = 0.0;
    for dz in 0..2 {
        for dy in 0..2 {
            for dx in 0..2 {
                let w = (if dx == 1 { t[0] } else { 1.0 - t[0] })
                    * (if dy == 1 { t[1] } else { 1.0 - t[1] })
                    * (if dz == 1 { t[2] } else { 1.0 - t[2] });
                acc += w * hash3(f[0] + dx, f[1] + dy, f[2] + dz, seed);
            }
        }
    }
    acc
}

fn midpoint_subdivide(mesh: &Mesh) -> Mesh {
    let mut out = Mesh::default();
    let has_uv = mesh.has_uvs();
    let mid3 = |a: [f32; 3], b: [f32; 3]| {
        [
            (a[0] + b[0]) / 2.0,
            (a[1] + b[1]) / 2.0,
            (a[2] + b[2]) / 2.0,
        ]
    };
    let mid2 = |a: [f32; 2], b: [f32; 2]| [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
    for t in mesh.indices.as_chunks::<3>().0 {
        let v = [t[0] as usize, t[1] as usize, t[2] as usize];
        let p = v.map(|i| mesh.positions[i]);
        let pts = [
            p[0],
            p[1],
            p[2],
            mid3(p[0], p[1]),
            mid3(p[1], p[2]),
            mid3(p[2], p[0]),
        ];
        #[allow(clippy::cast_possible_truncation)]
        let base = out.positions.len() as u32;
        out.positions.extend_from_slice(&pts);
        if has_uv {
            let u = v.map(|i| mesh.uvs[i]);
            out.uvs.extend_from_slice(&[
                u[0],
                u[1],
                u[2],
                mid2(u[0], u[1]),
                mid2(u[1], u[2]),
                mid2(u[2], u[0]),
            ]);
        }
        for tri in [[0, 3, 5], [3, 1, 4], [5, 4, 2], [3, 4, 5]] {
            out.indices.extend(tri.iter().map(|k| base + k));
        }
    }
    out.compute_normals();
    out
}

/// One level of Loop subdivision on a welded mesh.
#[allow(clippy::cast_precision_loss)]
fn loop_subdivide(mesh: &Mesh) -> Mesh {
    let m = weld(mesh, 1e-5);
    let n = m.positions.len();
    // Edge → (new vertex index, opposite vertices).
    let mut edges: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
    for t in m.indices.as_chunks::<3>().0 {
        for k in 0..3 {
            let (a, b, c) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
            edges.entry((a.min(b), a.max(b))).or_default().push(c);
        }
    }
    let mut neighbours: Vec<Vec<u32>> = vec![Vec::new(); n];
    let mut boundary_nb: Vec<Vec<u32>> = vec![Vec::new(); n];
    for (&(a, b), opp) in &edges {
        neighbours[a as usize].push(b);
        neighbours[b as usize].push(a);
        if opp.len() == 1 {
            boundary_nb[a as usize].push(b);
            boundary_nb[b as usize].push(a);
        }
    }
    let p = |i: u32| m.positions[i as usize];
    let add = |acc: &mut [f32; 3], v: [f32; 3], w: f32| {
        for k in 0..3 {
            acc[k] += v[k] * w;
        }
    };
    let mut out = Mesh::default();
    // Even (old) vertices, repositioned.
    for i in 0..n {
        let mut acc = [0.0; 3];
        if boundary_nb[i].len() == 2 {
            add(&mut acc, m.positions[i], 0.75);
            for &j in &boundary_nb[i] {
                add(&mut acc, p(j), 0.125);
            }
        } else {
            let k = neighbours[i].len() as f32;
            let beta = if k > 3.0 { 3.0 / (8.0 * k) } else { 3.0 / 16.0 };
            add(&mut acc, m.positions[i], 1.0 - k * beta);
            for &j in &neighbours[i] {
                add(&mut acc, p(j), beta);
            }
        }
        out.positions.push(acc);
    }
    // Odd (edge) vertices.
    let mut edge_vertex: HashMap<(u32, u32), u32> = HashMap::new();
    let mut keys: Vec<&(u32, u32)> = edges.keys().collect();
    keys.sort_unstable();
    for &(a, b) in keys {
        let opp = &edges[&(a, b)];
        let mut acc = [0.0; 3];
        if opp.len() == 2 {
            add(&mut acc, p(a), 0.375);
            add(&mut acc, p(b), 0.375);
            add(&mut acc, p(opp[0]), 0.125);
            add(&mut acc, p(opp[1]), 0.125);
        } else {
            add(&mut acc, p(a), 0.5);
            add(&mut acc, p(b), 0.5);
        }
        #[allow(clippy::cast_possible_truncation)]
        edge_vertex.insert((a, b), out.positions.len() as u32);
        out.positions.push(acc);
    }
    for t in m.indices.as_chunks::<3>().0 {
        let e = |a: u32, b: u32| edge_vertex[&(a.min(b), a.max(b))];
        let (a, b, c) = (t[0], t[1], t[2]);
        let (ab, bc, ca) = (e(a, b), e(b, c), e(c, a));
        out.indices
            .extend_from_slice(&[a, ab, ca, ab, b, bc, ca, bc, c, ab, bc, ca]);
    }
    out.compute_normals();
    out
}

impl Modifier {
    /// Apply this one modifier.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn apply(&self, mesh: &Mesh) -> Mesh {
        match *self {
            Self::Transform {
                translate,
                scale,
                rotate_y,
            } => {
                let (s, c) = rotate_y.sin_cos();
                map_positions(mesh, |p| {
                    let q = [p[0] * scale[0], p[1] * scale[1], p[2] * scale[2]];
                    [
                        q[0] * c + q[2] * s + translate[0],
                        q[1] + translate[1],
                        -q[0] * s + q[2] * c + translate[2],
                    ]
                })
            }
            Self::Mirror { axis, merge } => {
                let a = axis.index();
                let mut out = mesh.clone();
                #[allow(clippy::cast_possible_truncation)]
                let base = out.positions.len() as u32;
                for (i, p) in mesh.positions.iter().enumerate() {
                    let mut q = *p;
                    q[a] = -q[a];
                    out.positions.push(q);
                    if mesh.has_normals() {
                        let mut n = mesh.normals[i];
                        n[a] = -n[a];
                        out.normals.push(n);
                    }
                    if mesh.has_uvs() {
                        out.uvs.push(mesh.uvs[i]);
                    }
                }
                for t in mesh.indices.as_chunks::<3>().0 {
                    // Reflection flips handedness: reverse the winding.
                    out.indices
                        .extend_from_slice(&[base + t[0], base + t[2], base + t[1]]);
                }
                if merge > 0.0 {
                    weld(&out, merge)
                } else {
                    out
                }
            }
            Self::Array { count, offset } => {
                let mut out = Mesh::default();
                for k in 0..count.max(1) {
                    #[allow(clippy::cast_precision_loss)]
                    let d = offset.map(|o| o * k as f32);
                    #[allow(clippy::cast_possible_truncation)]
                    let base = out.positions.len() as u32;
                    out.positions.extend(
                        mesh.positions
                            .iter()
                            .map(|p| [p[0] + d[0], p[1] + d[1], p[2] + d[2]]),
                    );
                    out.normals.extend_from_slice(&mesh.normals);
                    out.uvs.extend_from_slice(&mesh.uvs);
                    out.indices.extend(mesh.indices.iter().map(|i| i + base));
                }
                out
            }
            Self::Subdivide { levels, smooth } => (0..levels).fold(mesh.clone(), |m, _| {
                if smooth {
                    loop_subdivide(&m)
                } else {
                    midpoint_subdivide(&m)
                }
            }),
            Self::Displace {
                strength,
                scale,
                seed,
            } => {
                let mut src = mesh.clone();
                if !src.has_normals() {
                    src.compute_normals();
                }
                let mut out = src.clone();
                for (i, p) in out.positions.iter_mut().enumerate() {
                    let d =
                        value_noise([p[0] / scale, p[1] / scale, p[2] / scale], seed) * strength;
                    let n = src.normals[i];
                    *p = [p[0] + n[0] * d, p[1] + n[1] * d, p[2] + n[2] * d];
                }
                out.compute_normals();
                out
            }
            Self::Twist { angle } => map_positions(mesh, |p| {
                let (s, c) = (angle * p[1]).sin_cos();
                [p[0] * c - p[2] * s, p[1], p[0] * s + p[2] * c]
            }),
            Self::Bend { angle } => {
                let (lo, hi) = y_range(mesh);
                let h = (hi - lo).max(1e-6);
                if angle.abs() < 1e-6 {
                    return mesh.clone();
                }
                let r = h / angle;
                map_positions(mesh, |p| {
                    let theta = (p[1] - lo) / h * angle;
                    let rx = r - p[0];
                    [r - rx * theta.cos(), lo + rx * theta.sin(), p[2]]
                })
            }
            Self::Taper { factor } => {
                let (lo, hi) = y_range(mesh);
                let h = (hi - lo).max(1e-6);
                map_positions(mesh, |p| {
                    let s = 1.0 + (factor - 1.0) * (p[1] - lo) / h;
                    [p[0] * s, p[1], p[2] * s]
                })
            }
            Self::Smooth { factor, iterations } => {
                let mut m = weld(mesh, 1e-5);
                let n = m.positions.len();
                let mut nb: Vec<Vec<u32>> = vec![Vec::new(); n];
                for t in m.indices.as_chunks::<3>().0 {
                    for k in 0..3 {
                        let (a, b) = (t[k], t[(k + 1) % 3]);
                        if !nb[a as usize].contains(&b) {
                            nb[a as usize].push(b);
                        }
                        if !nb[b as usize].contains(&a) {
                            nb[b as usize].push(a);
                        }
                    }
                }
                for _ in 0..iterations {
                    let prev = m.positions.clone();
                    for (i, p) in m.positions.iter_mut().enumerate() {
                        if nb[i].is_empty() {
                            continue;
                        }
                        #[allow(clippy::cast_precision_loss)]
                        let k = nb[i].len() as f32;
                        let mut avg = [0.0; 3];
                        for &j in &nb[i] {
                            for a in 0..3 {
                                avg[a] += prev[j as usize][a] / k;
                            }
                        }
                        for a in 0..3 {
                            p[a] += (avg[a] - p[a]) * factor;
                        }
                    }
                }
                m.compute_normals();
                m
            }
            Self::Decimate { cell } => {
                // Vertex clustering: every vertex snaps to its cell's mean.
                let m = weld(mesh, 1e-5);
                let key = |p: [f32; 3]| {
                    #[allow(clippy::cast_possible_truncation)]
                    let k = |v: f32| (v / cell.max(1e-6)).floor() as i64;
                    (k(p[0]), k(p[1]), k(p[2]))
                };
                type Cell = (i64, i64, i64);
                // Per cell: position sum, count, output index.
                let mut sums: HashMap<Cell, ([f32; 3], f32, u32)> = HashMap::new();
                let mut out = Mesh::default();
                let mut remap = Vec::with_capacity(m.positions.len());
                for p in &m.positions {
                    #[allow(clippy::cast_possible_truncation)]
                    let next = sums.len() as u32;
                    let e = sums.entry(key(*p)).or_insert(([0.0; 3], 0.0, next));
                    for (acc, x) in e.0.iter_mut().zip(p) {
                        *acc += x;
                    }
                    e.1 += 1.0;
                    remap.push(e.2);
                }
                out.positions = vec![[0.0; 3]; sums.len()];
                for (sum, count, i) in sums.values() {
                    out.positions[*i as usize] = sum.map(|v| v / count);
                }
                for t in m.indices.as_chunks::<3>().0 {
                    let (a, b, c) = (
                        remap[t[0] as usize],
                        remap[t[1] as usize],
                        remap[t[2] as usize],
                    );
                    if a != b && b != c && a != c {
                        out.indices.extend_from_slice(&[a, b, c]);
                    }
                }
                out.compute_normals();
                out
            }
            Self::Weld { distance } => weld(mesh, distance),
            Self::FlipNormals => {
                let mut out = mesh.clone();
                for t in out.indices.as_chunks_mut::<3>().0.iter_mut() {
                    t.swap(1, 2);
                }
                for n in &mut out.normals {
                    *n = n.map(|v| -v);
                }
                out
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit cube with split vertices per face (like an exporter's).
    fn cube() -> Mesh {
        let mut m = Mesh::default();
        let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
            ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
            ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
            ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
            ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ];
        for (n, u, v) in faces {
            #[allow(clippy::cast_possible_truncation)]
            let base = m.positions.len() as u32;
            for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                m.positions
                    .push([0, 1, 2].map(|k| (n[k] + u[k] * a + v[k] * b) * 0.5));
                m.uvs.push([f32::midpoint(a, 1.0), f32::midpoint(b, 1.0)]);
            }
            m.indices
                .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        m.compute_normals();
        m
    }

    fn volume(m: &Mesh) -> f32 {
        m.indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| {
                let [a, b, c] = [0, 1, 2].map(|k| m.positions[t[k] as usize]);
                (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                    + a[2] * (b[0] * c[1] - b[1] * c[0]))
                    / 6.0
            })
            .sum()
    }

    #[test]
    fn weld_merges_seams() {
        let w = weld(&cube(), 1e-4);
        assert_eq!(w.positions.len(), 8);
        assert_eq!(w.triangles(), 12);
        assert!((volume(&w) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn midpoint_subdivision_quadruples_and_keeps_shape() {
        let s = Modifier::Subdivide {
            levels: 2,
            smooth: false,
        }
        .apply(&cube());
        assert_eq!(s.triangles(), 12 * 16);
        assert!((volume(&s) - 1.0).abs() < 1e-4);
        assert!(s.has_uvs());
    }

    #[test]
    fn loop_subdivision_rounds_a_cube_toward_a_sphere() {
        let s = Modifier::Subdivide {
            levels: 3,
            smooth: true,
        }
        .apply(&cube());
        assert_eq!(s.triangles(), 12 * 64);
        let v = volume(&s);
        assert!(v < 1.0 && v > 0.3, "shrinks inside its hull: {v}");
        // Distances from the centre even out.
        let d: Vec<f32> = s
            .positions
            .iter()
            .map(|p| (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt())
            .collect();
        let (lo, hi) = d
            .iter()
            .fold((f32::MAX, 0.0f32), |(a, b), &x| (a.min(x), b.max(x)));
        assert!(hi / lo < 1.35, "{lo} .. {hi}");
    }

    #[test]
    fn mirror_array_and_flip() {
        let half = Modifier::Transform {
            translate: [1.0, 0.0, 0.0],
            scale: [1.0; 3],
            rotate_y: 0.0,
        }
        .apply(&cube());
        let both = Modifier::Mirror {
            axis: Axis::X,
            merge: 0.0,
        }
        .apply(&half);
        assert_eq!(both.triangles(), 24);
        assert!(
            (volume(&both) - 2.0).abs() < 1e-4,
            "mirror keeps outward winding"
        );
        let row = Modifier::Array {
            count: 3,
            offset: [2.0, 0.0, 0.0],
        }
        .apply(&cube());
        assert!((volume(&row) - 3.0).abs() < 1e-4);
        assert!((volume(&Modifier::FlipNormals.apply(&cube())) + 1.0).abs() < 1e-5);
    }

    #[test]
    fn deforms_move_what_they_should() {
        let tall = Modifier::Transform {
            translate: [0.0, 0.5, 0.0],
            scale: [1.0, 1.0, 1.0],
            rotate_y: 0.0,
        }
        .apply(&cube());
        let tapered = Modifier::Taper { factor: 0.0 }.apply(&tall);
        let top = tapered
            .positions
            .iter()
            .filter(|p| p[1] > 0.99)
            .all(|p| p[0].abs() < 1e-5 && p[2].abs() < 1e-5);
        assert!(top, "tapered to a point");
        let twisted = Modifier::Twist {
            angle: std::f32::consts::FRAC_PI_2,
        }
        .apply(&tall);
        assert!(
            (volume(&twisted) - 1.0).abs() < 0.2,
            "twist roughly preserves volume"
        );
        let bent = Modifier::Bend {
            angle: std::f32::consts::PI,
        }
        .apply(
            &Modifier::Subdivide {
                levels: 2,
                smooth: false,
            }
            .apply(&tall),
        );
        let (lo, hi) = bent.bounds().unwrap();
        assert!(hi[0] - lo[0] > 1.2, "bent sideways: {lo:?} {hi:?}");
    }

    #[test]
    fn displace_is_seeded_and_bounded() {
        let base = Modifier::Subdivide {
            levels: 2,
            smooth: false,
        }
        .apply(&weld(&cube(), 1e-5));
        let a = Modifier::Displace {
            strength: 0.1,
            scale: 0.3,
            seed: 1,
        }
        .apply(&base);
        let b = Modifier::Displace {
            strength: 0.1,
            scale: 0.3,
            seed: 1,
        }
        .apply(&base);
        assert_eq!(a, b);
        for (p, q) in a.positions.iter().zip(&base.positions) {
            let d = ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt();
            assert!(d <= 0.1 + 1e-5);
        }
    }

    #[test]
    fn smooth_and_decimate_simplify() {
        let dense = Modifier::Subdivide {
            levels: 3,
            smooth: false,
        }
        .apply(&cube());
        let dec = Modifier::Decimate { cell: 0.26 }.apply(&dense);
        assert!(
            dec.triangles() < dense.triangles() / 4,
            "{} → {}",
            dense.triangles(),
            dec.triangles()
        );
        let sm = Modifier::Smooth {
            factor: 0.5,
            iterations: 10,
        }
        .apply(&dense);
        assert!(volume(&sm) < 1.0);
    }

    #[test]
    fn stacks_fold_in_order() {
        let s = ModifierStack::new()
            .with(Modifier::Array {
                count: 2,
                offset: [1.0, 0.0, 0.0],
            })
            .with(Modifier::Weld { distance: 1e-4 });
        let out = s.apply(&cube());
        assert_eq!(out.positions.len(), 12, "the shared face welded");
    }
}
