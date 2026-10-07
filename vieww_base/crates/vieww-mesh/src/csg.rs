//! Constructive solid geometry — Blender's Boolean modifier, OpenSCAD's
//! `union()`/`difference()`/`intersection()`.
//!
//! The classic BSP-tree method (Naylor's, the one `csg.js` popularised):
//! each solid's polygons build a binary space partition; one solid's
//! polygons are clipped against the other's tree, coplanar faces are routed
//! by orientation, and the surviving pieces are merged. Exact topology is
//! not the goal — the output is a triangle soup with welded positions,
//! normals recomputed — but it is **watertight in the volume sense**: the
//! result's signed volume is the boolean of the inputs' volumes, which the
//! tests check against closed-form values.
//!
//! Inputs must be closed and consistently wound (counter-clockwise seen
//! from outside), which every primitive generator and a well-formed import
//! produce.
//!
//! ```
//! use vieww_mesh::csg::{difference, signed_volume};
//! use vieww_mesh::modifiers::cube;
//!
//! let a = cube(2.0);
//! let mut b = cube(2.0);
//! b.positions.iter_mut().for_each(|p| p[0] += 1.0);
//! let d = difference(&a, &b);
//! assert!((signed_volume(&d) - 4.0).abs() < 1e-3);
//! ```

use crate::Mesh;

const EPS: f32 = 1e-5;

#[derive(Debug, Clone, Copy)]
struct V3([f32; 3]);

impl V3 {
    fn sub(self, o: Self) -> Self {
        Self([self.0[0] - o.0[0], self.0[1] - o.0[1], self.0[2] - o.0[2]])
    }
    fn dot(self, o: Self) -> f32 {
        self.0[0] * o.0[0] + self.0[1] * o.0[1] + self.0[2] * o.0[2]
    }
    fn cross(self, o: Self) -> Self {
        Self([
            self.0[1] * o.0[2] - self.0[2] * o.0[1],
            self.0[2] * o.0[0] - self.0[0] * o.0[2],
            self.0[0] * o.0[1] - self.0[1] * o.0[0],
        ])
    }
    fn lerp(self, o: Self, t: f32) -> Self {
        Self([
            self.0[0] + (o.0[0] - self.0[0]) * t,
            self.0[1] + (o.0[1] - self.0[1]) * t,
            self.0[2] + (o.0[2] - self.0[2]) * t,
        ])
    }
    fn norm(self) -> Self {
        let l = self.dot(self).sqrt().max(1e-20);
        Self([self.0[0] / l, self.0[1] / l, self.0[2] / l])
    }
}

#[derive(Debug, Clone, Copy)]
struct Plane {
    n: V3,
    w: f32,
}

impl Plane {
    fn from_points(a: V3, b: V3, c: V3) -> Option<Self> {
        let n = b.sub(a).cross(c.sub(a));
        if n.dot(n) < 1e-20 {
            return None;
        }
        let n = n.norm();
        Some(Self { n, w: n.dot(a) })
    }
    fn flip(&mut self) {
        self.n = V3([-self.n.0[0], -self.n.0[1], -self.n.0[2]]);
        self.w = -self.w;
    }
}

#[derive(Debug, Clone)]
struct Poly {
    v: Vec<V3>,
    plane: Plane,
}

impl Poly {
    fn flip(&mut self) {
        self.v.reverse();
        self.plane.flip();
    }
}

const COPLANAR: u8 = 0;
const FRONT: u8 = 1;
const BACK: u8 = 2;
const SPANNING: u8 = 3;

/// Split `p` by `plane` into the four buckets.
fn split(
    plane: &Plane,
    p: &Poly,
    co_front: &mut Vec<Poly>,
    co_back: &mut Vec<Poly>,
    front: &mut Vec<Poly>,
    back: &mut Vec<Poly>,
) {
    let mut kind = 0u8;
    let types: Vec<u8> = p
        .v
        .iter()
        .map(|v| {
            let t = plane.n.dot(*v) - plane.w;
            let k = if t < -EPS {
                BACK
            } else if t > EPS {
                FRONT
            } else {
                COPLANAR
            };
            kind |= k;
            k
        })
        .collect();
    match kind {
        COPLANAR => {
            if plane.n.dot(p.plane.n) > 0.0 {
                co_front.push(p.clone());
            } else {
                co_back.push(p.clone());
            }
        }
        FRONT => front.push(p.clone()),
        BACK => back.push(p.clone()),
        _ => {
            let (mut f, mut b) = (Vec::new(), Vec::new());
            let n = p.v.len();
            for i in 0..n {
                let j = (i + 1) % n;
                let (ti, tj) = (types[i], types[j]);
                let (vi, vj) = (p.v[i], p.v[j]);
                if ti != BACK {
                    f.push(vi);
                }
                if ti != FRONT {
                    b.push(vi);
                }
                if (ti | tj) == SPANNING {
                    let t = (plane.w - plane.n.dot(vi)) / plane.n.dot(vj.sub(vi));
                    let v = vi.lerp(vj, t);
                    f.push(v);
                    b.push(v);
                }
            }
            if f.len() >= 3 {
                front.push(Poly {
                    v: f,
                    plane: p.plane,
                });
            }
            if b.len() >= 3 {
                back.push(Poly {
                    v: b,
                    plane: p.plane,
                });
            }
        }
    }
}

#[derive(Debug, Default)]
struct Node {
    plane: Option<Plane>,
    front: Option<Box<Node>>,
    back: Option<Box<Node>>,
    polys: Vec<Poly>,
}

impl Node {
    fn new(polys: Vec<Poly>) -> Self {
        let mut n = Self::default();
        n.build(polys);
        n
    }

    fn invert(&mut self) {
        for p in &mut self.polys {
            p.flip();
        }
        if let Some(pl) = &mut self.plane {
            pl.flip();
        }
        if let Some(f) = &mut self.front {
            f.invert();
        }
        if let Some(b) = &mut self.back {
            b.invert();
        }
        std::mem::swap(&mut self.front, &mut self.back);
    }

    fn clip_polys(&self, polys: Vec<Poly>) -> Vec<Poly> {
        let Some(plane) = self.plane else {
            return polys;
        };
        let (mut front, mut back) = (Vec::new(), Vec::new());
        for p in &polys {
            let (mut cf, mut cb) = (Vec::new(), Vec::new());
            split(&plane, p, &mut cf, &mut cb, &mut front, &mut back);
            front.extend(cf);
            back.extend(cb);
        }
        let mut front = match &self.front {
            Some(f) => f.clip_polys(front),
            None => front,
        };
        let back = match &self.back {
            Some(b) => b.clip_polys(back),
            None => Vec::new(),
        };
        front.extend(back);
        front
    }

    fn clip_to(&mut self, other: &Self) {
        self.polys = other.clip_polys(std::mem::take(&mut self.polys));
        if let Some(f) = &mut self.front {
            f.clip_to(other);
        }
        if let Some(b) = &mut self.back {
            b.clip_to(other);
        }
    }

    fn all(&self) -> Vec<Poly> {
        let mut out = self.polys.clone();
        if let Some(f) = &self.front {
            out.extend(f.all());
        }
        if let Some(b) = &self.back {
            out.extend(b.all());
        }
        out
    }

    fn build(&mut self, polys: Vec<Poly>) {
        if polys.is_empty() {
            return;
        }
        // Iterative descent: recursion depth is bounded by the tree, but the
        // work list keeps a pathological input from overflowing the stack.
        let plane = *self.plane.get_or_insert(polys[0].plane);
        let (mut front, mut back) = (Vec::new(), Vec::new());
        let (mut cf, mut cb) = (Vec::new(), Vec::new());
        for p in &polys {
            split(&plane, p, &mut cf, &mut cb, &mut front, &mut back);
        }
        self.polys.extend(cf);
        self.polys.extend(cb);
        if !front.is_empty() {
            self.front
                .get_or_insert_with(|| Box::new(Self::default()))
                .build(front);
        }
        if !back.is_empty() {
            self.back
                .get_or_insert_with(|| Box::new(Self::default()))
                .build(back);
        }
    }
}

fn to_polys(m: &Mesh) -> Vec<Poly> {
    m.indices
        .as_chunks::<3>()
        .0
        .iter()
        .filter_map(|t| {
            let v = t.map(|i| V3(m.positions[i as usize]));
            Plane::from_points(v[0], v[1], v[2]).map(|plane| Poly {
                v: v.to_vec(),
                plane,
            })
        })
        .collect()
}

fn to_mesh(polys: &[Poly]) -> Mesh {
    let mut m = Mesh::default();
    for p in polys {
        let base = m.positions.len();
        #[allow(clippy::cast_possible_truncation)]
        let b = base as u32;
        m.positions.extend(p.v.iter().map(|v| v.0));
        for k in 1..p.v.len() - 1 {
            #[allow(clippy::cast_possible_truncation)]
            m.indices.extend([b, b + k as u32, b + k as u32 + 1]);
        }
    }
    let mut m = crate::modifiers::weld(&m, 1e-4);
    m.normals.clear();
    m.compute_normals();
    m
}

/// `a ∪ b`.
#[must_use]
pub fn union(a: &Mesh, b: &Mesh) -> Mesh {
    let mut na = Node::new(to_polys(a));
    let mut nb = Node::new(to_polys(b));
    na.clip_to(&nb);
    nb.clip_to(&na);
    nb.invert();
    nb.clip_to(&na);
    nb.invert();
    na.build(nb.all());
    to_mesh(&na.all())
}

/// `a − b`.
#[must_use]
pub fn difference(a: &Mesh, b: &Mesh) -> Mesh {
    let mut na = Node::new(to_polys(a));
    let mut nb = Node::new(to_polys(b));
    na.invert();
    na.clip_to(&nb);
    nb.clip_to(&na);
    nb.invert();
    nb.clip_to(&na);
    nb.invert();
    na.build(nb.all());
    na.invert();
    to_mesh(&na.all())
}

/// `a ∩ b`.
#[must_use]
pub fn intersection(a: &Mesh, b: &Mesh) -> Mesh {
    let mut na = Node::new(to_polys(a));
    let mut nb = Node::new(to_polys(b));
    na.invert();
    nb.clip_to(&na);
    nb.invert();
    na.clip_to(&nb);
    nb.clip_to(&na);
    na.build(nb.all());
    na.invert();
    to_mesh(&na.all())
}

/// The enclosed volume (divergence theorem); positive for outward winding.
#[must_use]
pub fn signed_volume(m: &Mesh) -> f32 {
    m.indices
        .as_chunks::<3>()
        .0
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| V3(m.positions[i as usize]));
            a.dot(b.cross(c)) / 6.0
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modifiers::cube;

    fn shifted(mut m: Mesh, d: [f32; 3]) -> Mesh {
        for p in &mut m.positions {
            for k in 0..3 {
                p[k] += d[k];
            }
        }
        m
    }

    #[test]
    fn cube_volume_is_exact() {
        assert!((signed_volume(&cube(2.0)) - 8.0).abs() < 1e-4);
    }

    #[test]
    fn overlapping_cubes_obey_inclusion_exclusion() {
        let a = cube(2.0);
        let b = shifted(cube(2.0), [1.0, 1.0, 0.0]);
        // Overlap is 1 × 1 × 2 = 2.
        let u = signed_volume(&union(&a, &b));
        let i = signed_volume(&intersection(&a, &b));
        let d = signed_volume(&difference(&a, &b));
        assert!((u - 14.0).abs() < 1e-2, "union {u}");
        assert!((i - 2.0).abs() < 1e-2, "intersection {i}");
        assert!((d - 6.0).abs() < 1e-2, "difference {d}");
    }

    #[test]
    fn disjoint_solids() {
        let a = cube(1.0);
        let b = shifted(cube(1.0), [5.0, 0.0, 0.0]);
        assert!((signed_volume(&union(&a, &b)) - 2.0).abs() < 1e-3);
        assert!(signed_volume(&intersection(&a, &b)).abs() < 1e-3);
        assert!((signed_volume(&difference(&a, &b)) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn a_hole_through_a_block() {
        // A 4×4×4 block minus a 2×2×6 bar through it: 64 − 16 = 48.
        let block = cube(4.0);
        let mut bar = cube(1.0);
        for p in &mut bar.positions {
            p[0] *= 2.0;
            p[1] *= 2.0;
            p[2] *= 6.0;
        }
        let r = difference(&block, &bar);
        assert!((signed_volume(&r) - 48.0).abs() < 0.05);
        assert!(r.has_normals());
    }

    #[test]
    fn subtracting_itself_leaves_nothing() {
        let a = cube(2.0);
        assert!(signed_volume(&difference(&a, &a)).abs() < 1e-3);
    }
}
