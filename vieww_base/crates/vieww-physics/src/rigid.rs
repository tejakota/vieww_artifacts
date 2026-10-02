//! Rigid bodies that rotate — the game-engine physics layer.
//!
//! # What the translation-only [`World`](crate::World) could not do
//!
//! The crate root is a UI's physics: circles and axis-aligned boxes that
//! translate. Its own docs named what that leaves out — rotation, revolute
//! and motor joints, continuous collision — "the moment this crate stops
//! being a UI's physics and starts being an engine". The comparison
//! document asks for the engine: Unity's PhysX and Unreal's Chaos layers
//! (§2.16 L4, §2.17 L4: rigid bodies, colliders including capsules,
//! `Physics.Raycast`, joints, `CharacterController`), Godot's
//! `RigidBody2D`/`CharacterBody2D` and its `move_and_slide`, and the
//! ragdolls the capability matrix lists. This module is that engine, in the
//! Box2D lineage:
//!
//! * **Shapes** — circles, convex polygons (boxes included) and capsules,
//!   all with rotation. Polygons carry a *radius* (Box2D v3's rounded
//!   polygons), which is how a capsule is simply a two-vertex polygon with a
//!   radius, and why one polygon–polygon routine covers box/box,
//!   box/capsule and capsule/capsule.
//! * **Narrowphase** — SAT with reference/incident face clipping (up to two
//!   contact points per pair), plus closest-feature tests for circles.
//! * **Solver** — sequential impulses with accumulated-impulse clamping,
//!   **Coulomb friction**, restitution above a velocity threshold, Baumgarte
//!   stabilisation with slop, and **warm starting** across steps by contact
//!   feature id — the stability that makes stacks stand.
//! * **Joints** — [`JointKind::Revolute`] (with limits and a **motor**),
//!   [`JointKind::Distance`] (rigid or a damped spring), [`JointKind::Weld`]
//!   and [`JointKind::Mouse`] (drag a body toward a point).
//! * **Continuous collision** for bodies flagged `bullet`: a swept time of
//!   impact against static geometry, so a fast ball cannot tunnel through a
//!   thin wall.
//! * **Sleeping**, **raycasts**, point queries, contact begin/end events.
//! * A kinematic [`CharacterController`] with Godot's `move_and_slide`.
//!
//! Screen convention throughout: Y points down, so gravity is `+y` and a
//! floor's normal points toward `−y`. Angles are radians, clockwise on
//! screen.

use std::collections::HashMap;

use vieww_foundation::{Offset, Transform};

/// A 2-vector (the foundation's `Offset`).
pub type V = Offset;

fn v(x: f32, y: f32) -> V {
    Offset::new(x, y)
}
fn dot(a: V, b: V) -> f32 {
    a.dx * b.dx + a.dy * b.dy
}
fn cross(a: V, b: V) -> f32 {
    a.dx * b.dy - a.dy * b.dx
}
/// `s × v` for scalar s (angular velocity × radius vector).
fn cross_sv(s: f32, a: V) -> V {
    v(-s * a.dy, s * a.dx)
}
fn perp(a: V) -> V {
    v(-a.dy, a.dx)
}
fn rot(a: V, c: f32, s: f32) -> V {
    v(c * a.dx - s * a.dy, s * a.dx + c * a.dy)
}
fn inv_rot(a: V, c: f32, s: f32) -> V {
    v(c * a.dx + s * a.dy, -s * a.dx + c * a.dy)
}
fn norm(a: V) -> V {
    let l = a.distance();
    if l > 1e-12 {
        a.scale(1.0 / l)
    } else {
        V::ZERO
    }
}

/// A collision shape in body-local coordinates.
#[derive(Debug, Clone, PartialEq)]
pub enum Collider {
    Circle {
        radius: f32,
        center: V,
    },
    /// Convex, any winding (normalised to counter-clockwise on build), with
    /// a rounding `radius` (0 for sharp corners).
    Polygon {
        vertices: Vec<V>,
        normals: Vec<V>,
        radius: f32,
    },
}

impl Collider {
    #[must_use]
    pub const fn circle(radius: f32) -> Self {
        Self::Circle {
            radius,
            center: V::ZERO,
        }
    }

    /// A `w × h` box centred on the body.
    #[must_use]
    pub fn rect(w: f32, h: f32) -> Self {
        let (x, y) = (w / 2.0, h / 2.0);
        Self::polygon(&[v(-x, -y), v(x, -y), v(x, y), v(-x, y)], 0.0)
    }

    /// A capsule: a segment of `length` along local x, thickened by
    /// `radius`.
    #[must_use]
    pub fn capsule(length: f32, radius: f32) -> Self {
        let h = length / 2.0;
        Self::polygon(&[v(-h, 0.0), v(h, 0.0)], radius)
    }

    /// A convex polygon (points in any order are hulled).
    #[must_use]
    pub fn polygon(points: &[V], radius: f32) -> Self {
        let mut pts = hull(points);
        if pts.len() == 2 {
            // A segment: two opposite normals.
            let n = norm(perp(pts[1] - pts[0]));
            return Self::Polygon {
                vertices: pts,
                normals: vec![v(-n.dx, -n.dy), n],
                radius,
            };
        }
        // Ensure counter-clockwise in the math sense (cross > 0).
        let area: f32 = (0..pts.len())
            .map(|i| cross(pts[i], pts[(i + 1) % pts.len()]))
            .sum();
        if area < 0.0 {
            pts.reverse();
        }
        let n = pts.len();
        let normals = (0..n)
            .map(|i| {
                let e = pts[(i + 1) % n] - pts[i];
                norm(v(e.dy, -e.dx))
            })
            .collect();
        Self::Polygon {
            vertices: pts,
            normals,
            radius,
        }
    }

    /// Mass and rotational inertia at unit density, and the centroid.
    fn mass_data(&self) -> (f32, f32) {
        match self {
            Self::Circle { radius, center } => {
                let m = std::f32::consts::PI * radius * radius;
                (m, m * (0.5 * radius * radius + dot(*center, *center)))
            }
            Self::Polygon {
                vertices, radius, ..
            } if vertices.len() == 2 => {
                // Capsule: rectangle plus two half discs.
                let len = (vertices[1] - vertices[0]).distance();
                let r = *radius;
                let rect_m = len * 2.0 * r;
                let disc_m = std::f32::consts::PI * r * r;
                let m = rect_m + disc_m;
                let i_rect = rect_m * (len * len + 4.0 * r * r) / 12.0;
                let i_disc = disc_m * (0.5 * r * r + len * len / 4.0);
                (m, i_rect + i_disc)
            }
            Self::Polygon {
                vertices, radius, ..
            } => {
                let n = vertices.len();
                let (mut area, mut inertia) = (0.0, 0.0);
                for i in 0..n {
                    let (a, b) = (vertices[i], vertices[(i + 1) % n]);
                    let c = cross(a, b);
                    area += c / 2.0;
                    inertia += c * (dot(a, a) + dot(a, b) + dot(b, b)) / 12.0;
                }
                // Rounding adds a thin shell; approximate by perimeter × r.
                let perim: f32 = (0..n)
                    .map(|i| (vertices[(i + 1) % n] - vertices[i]).distance())
                    .sum();
                let extra = perim * radius;
                (
                    area.abs() + extra,
                    inertia.abs() * (1.0 + extra / area.abs().max(1e-6)),
                )
            }
        }
    }

    /// World-space AABB `(min, max)` at `(position, c, s)`.
    fn aabb(&self, p: V, c: f32, s: f32) -> (V, V) {
        match self {
            Self::Circle { radius, center } => {
                let q = p + rot(*center, c, s);
                (
                    v(q.dx - radius, q.dy - radius),
                    v(q.dx + radius, q.dy + radius),
                )
            }
            Self::Polygon {
                vertices, radius, ..
            } => {
                let mut lo = v(f32::INFINITY, f32::INFINITY);
                let mut hi = v(f32::NEG_INFINITY, f32::NEG_INFINITY);
                for &q in vertices {
                    let w = p + rot(q, c, s);
                    lo = v(lo.dx.min(w.dx), lo.dy.min(w.dy));
                    hi = v(hi.dx.max(w.dx), hi.dy.max(w.dy));
                }
                (
                    v(lo.dx - radius, lo.dy - radius),
                    v(hi.dx + radius, hi.dy + radius),
                )
            }
        }
    }

    /// Smallest half-extent — the tunnelling scale for CCD.
    fn min_extent(&self) -> f32 {
        match self {
            Self::Circle { radius, .. } => *radius,
            Self::Polygon {
                vertices, radius, ..
            } => {
                if vertices.len() == 2 {
                    return *radius;
                }
                let (lo, hi) = self.aabb(V::ZERO, 1.0, 0.0);
                ((hi.dx - lo.dx).min(hi.dy - lo.dy) / 2.0).max(*radius)
            }
        }
    }
}

/// Gift-wrapped convex hull (Andrew's monotone chain).
fn hull(points: &[V]) -> Vec<V> {
    let mut p: Vec<V> = points.to_vec();
    p.sort_by(|a, b| a.dx.total_cmp(&b.dx).then(a.dy.total_cmp(&b.dy)));
    p.dedup_by(|a, b| (*a - *b).distance() < 1e-6);
    if p.len() <= 2 {
        return p;
    }
    let mut lower: Vec<V> = Vec::new();
    for &q in &p {
        while lower.len() >= 2
            && cross(
                lower[lower.len() - 1] - lower[lower.len() - 2],
                q - lower[lower.len() - 2],
            ) <= 0.0
        {
            lower.pop();
        }
        lower.push(q);
    }
    let mut upper: Vec<V> = Vec::new();
    for &q in p.iter().rev() {
        while upper.len() >= 2
            && cross(
                upper[upper.len() - 1] - upper[upper.len() - 2],
                q - upper[upper.len() - 2],
            ) <= 0.0
        {
            upper.pop();
        }
        upper.push(q);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// How a body moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    /// Forces, gravity and collisions move it.
    Dynamic,
    /// Never moves.
    Static,
    /// Moves only by the velocity you set; pushes dynamic bodies, is not
    /// pushed.
    Kinematic,
}

/// A body.
#[derive(Debug, Clone)]
pub struct RigidBody {
    pub kind: BodyKind,
    pub collider: Collider,
    pub position: V,
    pub angle: f32,
    pub velocity: V,
    pub angular_velocity: f32,
    pub friction: f32,
    pub restitution: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    /// Continuous collision against static bodies.
    pub bullet: bool,
    /// Rotation locked (a character).
    pub fixed_rotation: bool,
    pub gravity_scale: f32,
    /// A number the application owns (an entity id, a colour index).
    pub tag: u64,
    pub force: V,
    pub torque: f32,
    inv_mass: f32,
    inv_inertia: f32,
    sleep_time: f32,
    pub sleeping: bool,
}

impl RigidBody {
    fn new(kind: BodyKind, collider: Collider, position: V, density: f32) -> Self {
        let (m, i) = collider.mass_data();
        let dynamic = kind == BodyKind::Dynamic;
        Self {
            kind,
            collider,
            position,
            angle: 0.0,
            velocity: V::ZERO,
            angular_velocity: 0.0,
            friction: 0.5,
            restitution: 0.0,
            linear_damping: 0.0,
            angular_damping: 0.01,
            bullet: false,
            fixed_rotation: false,
            gravity_scale: 1.0,
            tag: 0,
            force: V::ZERO,
            torque: 0.0,
            inv_mass: if dynamic && m * density > 0.0 {
                1.0 / (m * density)
            } else {
                0.0
            },
            inv_inertia: if dynamic && i * density > 0.0 {
                1.0 / (i * density)
            } else {
                0.0
            },
            sleep_time: 0.0,
            sleeping: false,
        }
    }

    /// A dynamic body of `density` (mass and inertia follow from the shape).
    #[must_use]
    pub fn dynamic(collider: Collider, position: V, density: f32) -> Self {
        Self::new(BodyKind::Dynamic, collider, position, density)
    }

    #[must_use]
    pub fn fixed(collider: Collider, position: V) -> Self {
        Self::new(BodyKind::Static, collider, position, 0.0)
    }

    #[must_use]
    pub fn kinematic(collider: Collider, position: V) -> Self {
        Self::new(BodyKind::Kinematic, collider, position, 0.0)
    }

    #[must_use]
    pub const fn angle(mut self, a: f32) -> Self {
        self.angle = a;
        self
    }
    #[must_use]
    pub const fn friction(mut self, f: f32) -> Self {
        self.friction = f;
        self
    }
    #[must_use]
    pub const fn restitution(mut self, r: f32) -> Self {
        self.restitution = r;
        self
    }
    #[must_use]
    pub const fn bullet(mut self) -> Self {
        self.bullet = true;
        self
    }
    #[must_use]
    pub const fn velocity(mut self, vel: V) -> Self {
        self.velocity = vel;
        self
    }
    #[must_use]
    pub const fn tag(mut self, t: u64) -> Self {
        self.tag = t;
        self
    }
    #[must_use]
    pub const fn fixed_rotation(mut self) -> Self {
        self.fixed_rotation = true;
        self.inv_inertia = 0.0;
        self
    }

    #[must_use]
    pub fn mass(&self) -> f32 {
        if self.inv_mass > 0.0 {
            1.0 / self.inv_mass
        } else {
            0.0
        }
    }

    /// Body-local point to world.
    #[must_use]
    pub fn world_point(&self, local: V) -> V {
        let (s, c) = self.angle.sin_cos();
        self.position + rot(local, c, s)
    }

    /// World point to body-local.
    #[must_use]
    pub fn local_point(&self, world: V) -> V {
        let (s, c) = self.angle.sin_cos();
        inv_rot(world - self.position, c, s)
    }

    /// The paint transform that places body-local artwork.
    #[must_use]
    pub fn transform(&self) -> Transform {
        Transform::rotate(self.angle).then(Transform::translate(self.position))
    }

    /// World-space outline vertices (polygons) — for drawing.
    #[must_use]
    pub fn world_vertices(&self) -> Vec<V> {
        match &self.collider {
            Collider::Polygon { vertices, .. } => {
                vertices.iter().map(|p| self.world_point(*p)).collect()
            }
            Collider::Circle { center, .. } => vec![self.world_point(*center)],
        }
    }

    /// Apply an impulse at a world point.
    pub fn apply_impulse(&mut self, impulse: V, at: V) {
        self.velocity = self.velocity + impulse.scale(self.inv_mass);
        self.angular_velocity += self.inv_inertia * cross(at - self.position, impulse);
        self.wake();
    }

    pub fn wake(&mut self) {
        self.sleeping = false;
        self.sleep_time = 0.0;
    }

    /// Whether the collider covers a world point.
    #[must_use]
    pub fn contains(&self, p: V) -> bool {
        let q = self.local_point(p);
        match &self.collider {
            Collider::Circle { radius, center } => (q - *center).distance() <= *radius,
            Collider::Polygon {
                vertices,
                normals,
                radius,
            } => {
                if vertices.len() == 2 {
                    return segment_distance(q, vertices[0], vertices[1]) <= *radius;
                }
                let sep = vertices
                    .iter()
                    .zip(normals)
                    .map(|(a, n)| dot(*n, q - *a))
                    .fold(f32::NEG_INFINITY, f32::max);
                if sep <= 0.0 {
                    return true;
                }
                let n = vertices.len();
                (0..n).any(|i| segment_distance(q, vertices[i], vertices[(i + 1) % n]) <= *radius)
            }
        }
    }
}

fn closest_on_segment(p: V, a: V, b: V) -> V {
    let ab = b - a;
    let l2 = dot(ab, ab);
    let t = if l2 > 0.0 {
        (dot(p - a, ab) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    a + ab.scale(t)
}

fn segment_distance(p: V, a: V, b: V) -> f32 {
    (p - closest_on_segment(p, a, b)).distance()
}

/// One contact point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactPoint {
    pub point: V,
    /// Positive when overlapping.
    pub penetration: f32,
    id: u32,
    normal_impulse: f32,
    tangent_impulse: f32,
    normal_mass: f32,
    tangent_mass: f32,
    bias: f32,
}

/// All contacts between two bodies; `normal` points from `a` to `b`.
#[derive(Debug, Clone, PartialEq)]
pub struct Manifold {
    pub a: usize,
    pub b: usize,
    pub normal: V,
    pub points: Vec<ContactPoint>,
}

fn point(p: V, pen: f32, id: u32) -> ContactPoint {
    ContactPoint {
        point: p,
        penetration: pen,
        id,
        normal_impulse: 0.0,
        tangent_impulse: 0.0,
        normal_mass: 0.0,
        tangent_mass: 0.0,
        bias: 0.0,
    }
}

struct Posed<'a> {
    c: &'a Collider,
    p: V,
    cos: f32,
    sin: f32,
}

impl Posed<'_> {
    fn verts(&self) -> (Vec<V>, Vec<V>, f32) {
        match self.c {
            Collider::Polygon {
                vertices,
                normals,
                radius,
            } => (
                vertices
                    .iter()
                    .map(|q| self.p + rot(*q, self.cos, self.sin))
                    .collect(),
                normals
                    .iter()
                    .map(|n| rot(*n, self.cos, self.sin))
                    .collect(),
                *radius,
            ),
            Collider::Circle { .. } => (Vec::new(), Vec::new(), 0.0),
        }
    }
}

fn collide_circles(pa: V, ra: f32, pb: V, rb: f32) -> Option<(V, Vec<ContactPoint>)> {
    let d = pb - pa;
    let dist = d.distance();
    if dist > ra + rb {
        return None;
    }
    let n = if dist > 1e-9 {
        d.scale(1.0 / dist)
    } else {
        v(0.0, -1.0)
    };
    let mid = pa + n.scale(ra - (ra + rb - dist) / 2.0);
    Some((n, vec![point(mid, ra + rb - dist, 0)]))
}

/// Polygon (with radius) against a circle; normal from polygon to circle.
fn collide_polygon_circle(
    verts: &[V],
    normals: &[V],
    pr: f32,
    c: V,
    cr: f32,
) -> Option<(V, Vec<ContactPoint>)> {
    let n = verts.len();
    let total = pr + cr;
    if n == 2 {
        let q = closest_on_segment(c, verts[0], verts[1]);
        return collide_circles(q, pr, c, cr);
    }
    // Deepest face.
    let (mut best, mut sep) = (0, f32::NEG_INFINITY);
    for i in 0..n {
        let s = dot(normals[i], c - verts[i]);
        if s > sep {
            sep = s;
            best = i;
        }
    }
    if sep > total {
        return None;
    }
    if sep <= 0.0 {
        // Centre inside: push out through the deepest face.
        let nn = normals[best];
        let pen = total - sep;
        return Some((nn, vec![point(c - nn.scale(cr - pen / 2.0), pen, 0)]));
    }
    let (a, b) = (verts[best], verts[(best + 1) % n]);
    let q = closest_on_segment(c, a, b);
    let d = c - q;
    let dist = d.distance();
    if dist > total {
        return None;
    }
    let nn = if dist > 1e-9 {
        d.scale(1.0 / dist)
    } else {
        normals[best]
    };
    let pen = total - dist;
    Some((nn, vec![point(q + nn.scale(pr - pen / 2.0), pen, 0)]))
}

/// The face of `a` with the greatest separation from `b`.
fn max_separation(va: &[V], na: &[V], vb: &[V]) -> (usize, f32) {
    let mut best = (0, f32::NEG_INFINITY);
    for i in 0..va.len() {
        let s = vb
            .iter()
            .map(|q| dot(na[i], *q - va[i]))
            .fold(f32::INFINITY, f32::min);
        if s > best.1 {
            best = (i, s);
        }
    }
    best
}

#[allow(clippy::too_many_lines)]
fn collide_polygons(a: &Posed<'_>, b: &Posed<'_>) -> Option<(V, Vec<ContactPoint>)> {
    let (va, na, ra) = a.verts();
    let (vb, nb, rb) = b.verts();
    let radius = ra + rb;
    // Segment vs segment: closest points (one contact, two when parallel).
    if va.len() == 2 && vb.len() == 2 {
        let candidates = [
            (va[0], closest_on_segment(va[0], vb[0], vb[1])),
            (va[1], closest_on_segment(va[1], vb[0], vb[1])),
            (closest_on_segment(vb[0], va[0], va[1]), vb[0]),
            (closest_on_segment(vb[1], va[0], va[1]), vb[1]),
        ];
        // Proper crossing: treat as deep overlap at the intersection.
        let d1 = va[1] - va[0];
        let d2 = vb[1] - vb[0];
        let den = cross(d1, d2);
        if den.abs() > 1e-9 {
            let t = cross(vb[0] - va[0], d2) / den;
            let u = cross(vb[0] - va[0], d1) / den;
            if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
                let x = va[0] + d1.scale(t);
                let n = norm(perp(d1));
                let n = if dot(n, (vb[0] + vb[1]).scale(0.5) - (va[0] + va[1]).scale(0.5)) < 0.0 {
                    v(-n.dx, -n.dy)
                } else {
                    n
                };
                return Some((n, vec![point(x, radius, 0)]));
            }
        }
        let mut sorted = candidates;
        sorted.sort_by(|x, y| (x.1 - x.0).distance().total_cmp(&(y.1 - y.0).distance()));
        let (p, q) = sorted[0];
        let dist = (q - p).distance();
        if dist > radius {
            return None;
        }
        let n = if dist > 1e-9 {
            (q - p).scale(1.0 / dist)
        } else {
            norm(perp(d1))
        };
        let mut pts = vec![point(
            p + n.scale(ra - (radius - dist) / 2.0),
            radius - dist,
            0,
        )];
        // Parallel overlap: add the second-closest pair if nearly as close.
        let (p2, q2) = sorted[1];
        let dist2 = (q2 - p2).distance();
        if dist2 <= radius
            && (dist2 - dist).abs() < 0.05 * radius.max(1.0)
            && (p2 - p).distance() > 1e-3
        {
            pts.push(point(
                p2 + n.scale(ra - (radius - dist2) / 2.0),
                radius - dist2,
                1,
            ));
        }
        return Some((n, pts));
    }
    // SAT over both polygons' face normals.
    let (fa, sa) = max_separation(&va, &na, &vb);
    if sa > radius {
        return None;
    }
    let (fb, sb) = max_separation(&vb, &nb, &va);
    if sb > radius {
        return None;
    }
    // Reference face on the polygon with the greater separation.
    let flip = sb > sa + 1e-4;
    let (rv, rn, iv, inn, face) = if flip {
        (&vb, &nb, &va, &na, fb)
    } else {
        (&va, &na, &vb, &nb, fa)
    };
    let rnorm = rn[face];
    // Incident face: most anti-parallel to the reference normal.
    let mut inc = 0;
    let mut min_dot = f32::INFINITY;
    for (i, n) in inn.iter().enumerate() {
        let d = dot(rnorm, *n);
        if d < min_dot {
            min_dot = d;
            inc = i;
        }
    }
    let mut incident = [
        (iv[inc], inc),
        (iv[(inc + 1) % iv.len()], (inc + 1) % iv.len()),
    ];
    let (r1, r2) = (rv[face], rv[(face + 1) % rv.len()]);
    let tangent = norm(r2 - r1);
    // Clip incident segment to the reference face's side planes.
    let clip = |pts: &mut [(V, usize); 2], n: V, off: f32| -> bool {
        let d0 = dot(n, pts[0].0) - off;
        let d1 = dot(n, pts[1].0) - off;
        if d0 > 0.0 && d1 > 0.0 {
            return false;
        }
        if d0 > 0.0 {
            let t = d0 / (d0 - d1);
            pts[0].0 = pts[0].0 + (pts[1].0 - pts[0].0).scale(t);
        } else if d1 > 0.0 {
            let t = d0 / (d0 - d1);
            pts[1].0 = pts[0].0 + (pts[1].0 - pts[0].0).scale(t);
        }
        true
    };
    if !clip(
        &mut incident,
        v(-tangent.dx, -tangent.dy),
        -dot(tangent, r1),
    ) {
        return None;
    }
    if !clip(&mut incident, tangent, dot(tangent, r2)) {
        return None;
    }
    let normal = if flip { v(-rnorm.dx, -rnorm.dy) } else { rnorm };
    let mut pts = Vec::new();
    for (k, (p, idx)) in incident.iter().enumerate() {
        let sep = dot(rnorm, *p - r1);
        if sep <= radius {
            // Midway between the rounded surfaces.
            let on_ref = *p - rnorm.scale(sep);
            let (pr, pi) = if flip { (rb, ra) } else { (ra, rb) };
            let mid = (on_ref + rnorm.scale(pr) + (*p - rnorm.scale(pi))).scale(0.5);
            #[allow(clippy::cast_possible_truncation)]
            let id = ((face as u32) << 16)
                | ((*idx as u32) << 1)
                | k as u32
                | if flip { 1 << 31 } else { 0 };
            pts.push(point(mid, radius - sep, id));
        }
    }
    if pts.is_empty() {
        return None;
    }
    Some((normal, pts))
}

fn collide(a: &RigidBody, b: &RigidBody) -> Option<(V, Vec<ContactPoint>)> {
    let (sa, ca) = a.angle.sin_cos();
    let (sb, cb) = b.angle.sin_cos();
    let pa = Posed {
        c: &a.collider,
        p: a.position,
        cos: ca,
        sin: sa,
    };
    let pb = Posed {
        c: &b.collider,
        p: b.position,
        cos: cb,
        sin: sb,
    };
    match (&a.collider, &b.collider) {
        (
            Collider::Circle {
                radius: r1,
                center: c1,
            },
            Collider::Circle {
                radius: r2,
                center: c2,
            },
        ) => collide_circles(
            a.position + rot(*c1, ca, sa),
            *r1,
            b.position + rot(*c2, cb, sb),
            *r2,
        ),
        (Collider::Polygon { .. }, Collider::Circle { radius, center }) => {
            let (vs, ns, r) = pa.verts();
            collide_polygon_circle(&vs, &ns, r, b.position + rot(*center, cb, sb), *radius)
        }
        (Collider::Circle { radius, center }, Collider::Polygon { .. }) => {
            let (vs, ns, r) = pb.verts();
            collide_polygon_circle(&vs, &ns, r, a.position + rot(*center, ca, sa), *radius)
                .map(|(n, p)| (v(-n.dx, -n.dy), p))
        }
        (Collider::Polygon { .. }, Collider::Polygon { .. }) => collide_polygons(&pa, &pb),
    }
}

/// What a joint constrains.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JointKind {
    /// A pin: the anchors coincide; free rotation, optional limits on the
    /// relative angle and an optional motor (target speed, max torque).
    Revolute {
        limits: Option<(f32, f32)>,
        motor: Option<(f32, f32)>,
    },
    /// Anchors kept `length` apart; `frequency` > 0 makes it a spring with
    /// damping ratio `damping`.
    Distance {
        length: f32,
        frequency: f32,
        damping: f32,
    },
    /// Anchors coincide and the relative angle is frozen.
    Weld,
    /// Pull body `b`'s anchor toward `target` with a soft spring (`a` is
    /// ignored) — dragging with the pointer.
    Mouse { target: V, max_force: f32 },
}

/// A constraint between two bodies' local anchor points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RigidJoint {
    pub a: usize,
    pub b: usize,
    pub local_a: V,
    pub local_b: V,
    pub kind: JointKind,
    reference_angle: f32,
    impulse: V,
    angular_impulse: f32,
    motor_impulse: f32,
}

impl RigidJoint {
    /// A joint at a world `anchor` point (converted to both bodies' local
    /// frames now — the standard "create at the current pose").
    #[must_use]
    pub fn at_anchor(world: &RigidWorld, a: usize, b: usize, anchor: V, kind: JointKind) -> Self {
        let (ba, bb) = (&world.bodies[a], &world.bodies[b]);
        Self {
            a,
            b,
            local_a: ba.local_point(anchor),
            local_b: bb.local_point(anchor),
            kind,
            reference_angle: bb.angle - ba.angle,
            impulse: V::ZERO,
            angular_impulse: 0.0,
            motor_impulse: 0.0,
        }
    }

    /// A distance joint between two world anchors; the length is their
    /// current separation unless `kind` says otherwise.
    #[must_use]
    pub fn between(
        world: &RigidWorld,
        a: usize,
        anchor_a: V,
        b: usize,
        anchor_b: V,
        frequency: f32,
        damping: f32,
    ) -> Self {
        let (ba, bb) = (&world.bodies[a], &world.bodies[b]);
        Self {
            a,
            b,
            local_a: ba.local_point(anchor_a),
            local_b: bb.local_point(anchor_b),
            kind: JointKind::Distance {
                length: (anchor_b - anchor_a).distance(),
                frequency,
                damping,
            },
            reference_angle: bb.angle - ba.angle,
            impulse: V::ZERO,
            angular_impulse: 0.0,
            motor_impulse: 0.0,
        }
    }

    /// Change a revolute joint's motor speed.
    pub fn set_motor_speed(&mut self, speed: f32) {
        if let JointKind::Revolute {
            motor: Some((s, _)),
            ..
        } = &mut self.kind
        {
            *s = speed;
        }
    }

    /// Move a mouse joint's target.
    pub fn set_target(&mut self, p: V) {
        if let JointKind::Mouse { target, .. } = &mut self.kind {
            *target = p;
        }
    }
}

/// What a raycast hit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit {
    pub body: usize,
    pub point: V,
    pub normal: V,
    /// Distance along the (unit) ray.
    pub distance: f32,
}

/// Contact began or ended between two bodies this step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContactEvent {
    Begin(usize, usize),
    End(usize, usize),
}

/// Counters from the last step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StepStats {
    pub broadphase_pairs: usize,
    pub manifolds: usize,
    pub contact_points: usize,
    pub sleeping: usize,
    pub toi_events: usize,
}

/// The simulation.
#[derive(Debug, Clone)]
pub struct RigidWorld {
    pub bodies: Vec<RigidBody>,
    pub joints: Vec<RigidJoint>,
    pub gravity: V,
    pub velocity_iterations: u32,
    pub position_slop: f32,
    pub baumgarte: f32,
    pub restitution_threshold: f32,
    pub sleep_enabled: bool,
    manifolds: Vec<Manifold>,
    events: Vec<ContactEvent>,
    pub stats: StepStats,
}

impl Default for RigidWorld {
    fn default() -> Self {
        Self::new(v(0.0, 9.8 * 60.0))
    }
}

impl RigidWorld {
    /// A world with `gravity` (units / s², Y down).
    #[must_use]
    pub fn new(gravity: V) -> Self {
        Self {
            bodies: Vec::new(),
            joints: Vec::new(),
            gravity,
            velocity_iterations: 10,
            position_slop: 0.5,
            baumgarte: 0.2,
            restitution_threshold: 30.0,
            sleep_enabled: true,
            manifolds: Vec::new(),
            events: Vec::new(),
            stats: StepStats::default(),
        }
    }

    pub fn add(&mut self, body: RigidBody) -> usize {
        self.bodies.push(body);
        self.bodies.len() - 1
    }

    pub fn add_joint(&mut self, joint: RigidJoint) -> usize {
        self.joints.push(joint);
        self.joints.len() - 1
    }

    /// Current contact manifolds (after the last step).
    #[must_use]
    pub fn manifolds(&self) -> &[Manifold] {
        &self.manifolds
    }

    /// Begin/end events from the last step.
    #[must_use]
    pub fn events(&self) -> &[ContactEvent] {
        &self.events
    }

    /// Advance by `dt` (use a fixed step).
    #[allow(clippy::too_many_lines)]
    pub fn step(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        let inv_dt = 1.0 / dt;
        // Integrate forces.
        for b in &mut self.bodies {
            if b.kind != BodyKind::Dynamic || b.sleeping {
                continue;
            }
            b.velocity = b.velocity
                + (self.gravity.scale(b.gravity_scale) + b.force.scale(b.inv_mass)).scale(dt);
            b.angular_velocity += b.torque * b.inv_inertia * dt;
            b.velocity = b.velocity.scale(1.0 / (1.0 + dt * b.linear_damping));
            b.angular_velocity /= 1.0 + dt * b.angular_damping;
            b.force = V::ZERO;
            b.torque = 0.0;
        }

        // Broadphase: sort and sweep on x.
        let aabbs: Vec<(V, V)> = self
            .bodies
            .iter()
            .map(|b| {
                let (s, c) = b.angle.sin_cos();
                b.collider.aabb(b.position, c, s)
            })
            .collect();
        let mut order: Vec<usize> = (0..self.bodies.len()).collect();
        order.sort_by(|&i, &j| aabbs[i].0.dx.total_cmp(&aabbs[j].0.dx));
        let mut pairs = Vec::new();
        for (k, &i) in order.iter().enumerate() {
            for &j in &order[k + 1..] {
                if aabbs[j].0.dx > aabbs[i].1.dx {
                    break;
                }
                if aabbs[i].0.dy > aabbs[j].1.dy || aabbs[j].0.dy > aabbs[i].1.dy {
                    continue;
                }
                let (a, b) = (i.min(j), i.max(j));
                let (ba, bb) = (&self.bodies[a], &self.bodies[b]);
                if ba.kind != BodyKind::Dynamic && bb.kind != BodyKind::Dynamic {
                    continue;
                }
                if (ba.sleeping || ba.kind != BodyKind::Dynamic)
                    && (bb.sleeping || bb.kind != BodyKind::Dynamic)
                {
                    continue;
                }
                // Jointed bodies do not collide with each other.
                if self
                    .joints
                    .iter()
                    .any(|jn| (jn.a == a && jn.b == b) || (jn.a == b && jn.b == a))
                {
                    continue;
                }
                pairs.push((a, b));
            }
        }
        self.stats = StepStats {
            broadphase_pairs: pairs.len(),
            ..StepStats::default()
        };

        // Narrowphase, warm-started from last step's impulses.
        let old: HashMap<(usize, usize), Manifold> =
            self.manifolds.drain(..).map(|m| ((m.a, m.b), m)).collect();
        let mut manifolds = Vec::new();
        for (a, b) in pairs {
            if let Some((normal, mut points)) = collide(&self.bodies[a], &self.bodies[b]) {
                if let Some(prev) = old.get(&(a, b)) {
                    for p in &mut points {
                        if let Some(q) = prev.points.iter().find(|q| q.id == p.id) {
                            p.normal_impulse = q.normal_impulse;
                            p.tangent_impulse = q.tangent_impulse;
                        }
                    }
                }
                manifolds.push(Manifold {
                    a,
                    b,
                    normal,
                    points,
                });
            }
        }
        // Events.
        self.events.clear();
        for m in &manifolds {
            if !old.contains_key(&(m.a, m.b)) {
                self.events.push(ContactEvent::Begin(m.a, m.b));
            }
        }
        for key in old.keys() {
            if !manifolds.iter().any(|m| (m.a, m.b) == *key) {
                self.events.push(ContactEvent::End(key.0, key.1));
            }
        }
        // Wake sleepers touched by moving bodies.
        for m in &manifolds {
            let (ma, mb) = (self.bodies[m.a].sleeping, self.bodies[m.b].sleeping);
            if ma != mb {
                let mover = if ma { m.b } else { m.a };
                let bm = &self.bodies[mover];
                if bm.velocity.distance() > 5.0 || bm.angular_velocity.abs() > 0.2 {
                    self.bodies[m.a].wake();
                    self.bodies[m.b].wake();
                }
            }
        }

        // Pre-step: effective masses, bias, restitution; warm start.
        for m in &mut manifolds {
            let (ba, bb) = (&self.bodies[m.a], &self.bodies[m.b]);
            let tangent = v(-m.normal.dy, m.normal.dx);
            let restitution = ba.restitution.max(bb.restitution);
            for p in &mut m.points {
                let ra = p.point - ba.position;
                let rb = p.point - bb.position;
                let rna = cross(ra, m.normal);
                let rnb = cross(rb, m.normal);
                let kn = ba.inv_mass
                    + bb.inv_mass
                    + ba.inv_inertia * rna * rna
                    + bb.inv_inertia * rnb * rnb;
                p.normal_mass = if kn > 0.0 { 1.0 / kn } else { 0.0 };
                let rta = cross(ra, tangent);
                let rtb = cross(rb, tangent);
                let kt = ba.inv_mass
                    + bb.inv_mass
                    + ba.inv_inertia * rta * rta
                    + bb.inv_inertia * rtb * rtb;
                p.tangent_mass = if kt > 0.0 { 1.0 / kt } else { 0.0 };
                let dv = bb.velocity + cross_sv(bb.angular_velocity, rb)
                    - ba.velocity
                    - cross_sv(ba.angular_velocity, ra);
                let vn = dot(dv, m.normal);
                p.bias = self.baumgarte * inv_dt * (p.penetration - self.position_slop).max(0.0);
                if vn < -self.restitution_threshold {
                    p.bias = p.bias.max(-restitution * vn);
                }
            }
        }
        for m in &manifolds {
            let tangent = v(-m.normal.dy, m.normal.dx);
            for p in &m.points {
                let imp = m.normal.scale(p.normal_impulse) + tangent.scale(p.tangent_impulse);
                self.apply_pair(m.a, m.b, p.point, imp);
            }
        }
        for j in &mut self.joints {
            j.impulse = V::ZERO;
            j.angular_impulse = 0.0;
            j.motor_impulse = 0.0;
        }

        // Velocity iterations.
        for _ in 0..self.velocity_iterations {
            for ji in 0..self.joints.len() {
                self.solve_joint(ji, dt);
            }
            for m in &mut manifolds {
                let tangent = v(-m.normal.dy, m.normal.dx);
                let friction = (self.bodies[m.a].friction * self.bodies[m.b].friction).sqrt();
                for p in &mut m.points {
                    let (ba, bb) = (&self.bodies[m.a], &self.bodies[m.b]);
                    let ra = p.point - ba.position;
                    let rb = p.point - bb.position;
                    let dv = bb.velocity + cross_sv(bb.angular_velocity, rb)
                        - ba.velocity
                        - cross_sv(ba.angular_velocity, ra);
                    // Tangent.
                    let vt = dot(dv, tangent);
                    let max_f = friction * p.normal_impulse;
                    let new_t = (p.tangent_impulse - p.tangent_mass * vt).clamp(-max_f, max_f);
                    let dt_imp = new_t - p.tangent_impulse;
                    p.tangent_impulse = new_t;
                    Self::apply_pair_raw(
                        &mut self.bodies,
                        m.a,
                        m.b,
                        p.point,
                        tangent.scale(dt_imp),
                    );
                    // Normal, against the velocity friction just changed.
                    let (ba, bb) = (&self.bodies[m.a], &self.bodies[m.b]);
                    let dv2 = bb.velocity + cross_sv(bb.angular_velocity, rb)
                        - ba.velocity
                        - cross_sv(ba.angular_velocity, ra);
                    let vn = dot(dv2, m.normal);
                    let new_n = (p.normal_impulse - p.normal_mass * (vn - p.bias)).max(0.0);
                    let dn = new_n - p.normal_impulse;
                    p.normal_impulse = new_n;
                    let imp = m.normal.scale(dn);
                    Self::apply_pair_raw(&mut self.bodies, m.a, m.b, p.point, imp);
                }
            }
        }

        // Continuous collision for bullets, then integrate positions.
        let statics: Vec<usize> = (0..self.bodies.len())
            .filter(|&i| self.bodies[i].kind == BodyKind::Static)
            .collect();
        for i in 0..self.bodies.len() {
            let b = &self.bodies[i];
            if b.kind == BodyKind::Static || b.sleeping {
                continue;
            }
            let mut step = dt;
            if b.bullet {
                let travel = b.velocity.distance() * dt;
                let ext = b.collider.min_extent().max(1e-3);
                if travel > ext * 0.5 {
                    if let Some(toi) = self.time_of_impact(i, &statics, dt) {
                        step = toi;
                        self.stats.toi_events += 1;
                    }
                }
            }
            let b = &mut self.bodies[i];
            b.position = b.position + b.velocity.scale(step);
            if !b.fixed_rotation {
                b.angle += b.angular_velocity * dt;
            }
        }

        // Sleep.
        if self.sleep_enabled {
            for b in &mut self.bodies {
                if b.kind != BodyKind::Dynamic {
                    continue;
                }
                if b.velocity.distance() < 3.0 && b.angular_velocity.abs() < 0.05 {
                    b.sleep_time += dt;
                    if b.sleep_time > 0.6 {
                        b.sleeping = true;
                        b.velocity = V::ZERO;
                        b.angular_velocity = 0.0;
                    }
                } else {
                    b.sleep_time = 0.0;
                    b.sleeping = false;
                }
            }
        }
        self.stats.manifolds = manifolds.len();
        self.stats.contact_points = manifolds.iter().map(|m| m.points.len()).sum();
        self.stats.sleeping = self.bodies.iter().filter(|b| b.sleeping).count();
        self.manifolds = manifolds;
    }

    /// Earliest time within `dt` at which body `i`, moving linearly, first
    /// touches a static body; `None` if it does not.
    fn time_of_impact(&self, i: usize, statics: &[usize], dt: f32) -> Option<f32> {
        let body = &self.bodies[i];
        let ext = body.collider.min_extent().max(1e-3);
        let travel = body.velocity.distance() * dt;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let samples = ((travel / (ext * 0.5)).ceil() as usize).clamp(2, 512);
        let touching = |t: f32| {
            let mut probe = body.clone();
            probe.position = body.position + body.velocity.scale(t);
            statics.iter().any(|&s| {
                collide(&probe, &self.bodies[s])
                    .is_some_and(|(_, p)| p.iter().any(|q| q.penetration > 0.0))
            })
        };
        if touching(0.0) {
            return None;
        }
        #[allow(clippy::cast_precision_loss)]
        for k in 1..=samples {
            let t = dt * k as f32 / samples as f32;
            if touching(t) {
                let (mut lo, mut hi) = (dt * (k - 1) as f32 / samples as f32, t);
                for _ in 0..20 {
                    let mid = 0.5 * (lo + hi);
                    if touching(mid) {
                        hi = mid;
                    } else {
                        lo = mid;
                    }
                }
                return Some(lo);
            }
        }
        None
    }

    fn apply_pair(&mut self, a: usize, b: usize, at: V, imp: V) {
        Self::apply_pair_raw(&mut self.bodies, a, b, at, imp);
    }

    fn apply_pair_raw(bodies: &mut [RigidBody], a: usize, b: usize, at: V, imp: V) {
        {
            let ba = &mut bodies[a];
            ba.velocity = ba.velocity - imp.scale(ba.inv_mass);
            ba.angular_velocity -= ba.inv_inertia * cross(at - ba.position, imp);
        }
        let bb = &mut bodies[b];
        bb.velocity = bb.velocity + imp.scale(bb.inv_mass);
        bb.angular_velocity += bb.inv_inertia * cross(at - bb.position, imp);
    }

    #[allow(clippy::too_many_lines)]
    fn solve_joint(&mut self, ji: usize, dt: f32) {
        let j = self.joints[ji];
        let (ia, ib) = (j.a, j.b);
        let (ba, bb) = (&self.bodies[ia], &self.bodies[ib]);
        if ba.sleeping && bb.sleeping {
            return;
        }
        let (sa, ca) = ba.angle.sin_cos();
        let (sb, cb) = bb.angle.sin_cos();
        let ra = rot(j.local_a, ca, sa);
        let rb = rot(j.local_b, cb, sb);
        let pa = ba.position + ra;
        let pb = bb.position + rb;
        let (ma, mb, iia, iib) = (ba.inv_mass, bb.inv_mass, ba.inv_inertia, bb.inv_inertia);
        let inv_dt = 1.0 / dt;
        // The 2×2 point-to-point effective mass.
        let point_constraint = |bodies: &mut [RigidBody], stiffness_bias: V, soft: f32| {
            let (ba, bb) = (&bodies[ia], &bodies[ib]);
            let dv = bb.velocity + cross_sv(bb.angular_velocity, rb)
                - ba.velocity
                - cross_sv(ba.angular_velocity, ra);
            let k11 = ma + mb + iia * ra.dy * ra.dy + iib * rb.dy * rb.dy + soft;
            let k12 = -iia * ra.dx * ra.dy - iib * rb.dx * rb.dy;
            let k22 = ma + mb + iia * ra.dx * ra.dx + iib * rb.dx * rb.dx + soft;
            let det = k11 * k22 - k12 * k12;
            if det.abs() < 1e-12 {
                return V::ZERO;
            }
            let rhs = v(-(dv.dx + stiffness_bias.dx), -(dv.dy + stiffness_bias.dy));
            let imp = v(
                (k22 * rhs.dx - k12 * rhs.dy) / det,
                (k11 * rhs.dy - k12 * rhs.dx) / det,
            );
            Self::apply_pair_raw(bodies, ia, ib, pb, imp);
            let _ = pa;
            imp
        };
        match j.kind {
            JointKind::Revolute { limits, motor } => {
                // Motor.
                if let Some((speed, max_torque)) = motor {
                    let (ba, bb) = (&self.bodies[ia], &self.bodies[ib]);
                    let k = iia + iib;
                    if k > 0.0 {
                        let cdot = bb.angular_velocity - ba.angular_velocity - speed;
                        let old = self.joints[ji].motor_impulse;
                        let max = max_torque * dt;
                        let new = (old - cdot / k).clamp(-max, max);
                        self.joints[ji].motor_impulse = new;
                        let d = new - old;
                        self.bodies[ia].angular_velocity -= iia * d;
                        self.bodies[ib].angular_velocity += iib * d;
                    }
                }
                // Limits (a simple clamp impulse with positional bias).
                if let Some((lo, hi)) = limits {
                    let (ba, bb) = (&self.bodies[ia], &self.bodies[ib]);
                    let angle = bb.angle - ba.angle - j.reference_angle;
                    let k = iia + iib;
                    if k > 0.0 {
                        let rel = bb.angular_velocity - ba.angular_velocity;
                        let mut d = 0.0;
                        if angle <= lo {
                            let c = angle - lo;
                            let new = (self.joints[ji].angular_impulse
                                - (rel + 0.2 * inv_dt * c) / k)
                                .max(0.0);
                            d = new - self.joints[ji].angular_impulse;
                            self.joints[ji].angular_impulse = new;
                        } else if angle >= hi {
                            let c = angle - hi;
                            let new = (self.joints[ji].angular_impulse
                                - (rel + 0.2 * inv_dt * c) / k)
                                .min(0.0);
                            d = new - self.joints[ji].angular_impulse;
                            self.joints[ji].angular_impulse = new;
                        }
                        self.bodies[ia].angular_velocity -= iia * d;
                        self.bodies[ib].angular_velocity += iib * d;
                    }
                }
                let c = pb - pa;
                let bias = c.scale(0.2 * inv_dt);
                let imp = point_constraint(&mut self.bodies, bias, 0.0);
                self.joints[ji].impulse = self.joints[ji].impulse + imp;
            }
            JointKind::Weld => {
                let (ba, bb) = (&self.bodies[ia], &self.bodies[ib]);
                let k = iia + iib;
                if k > 0.0 {
                    let angle = bb.angle - ba.angle - j.reference_angle;
                    let rel = bb.angular_velocity - ba.angular_velocity;
                    let d = -(rel + 0.2 * inv_dt * angle) / k;
                    self.bodies[ia].angular_velocity -= iia * d;
                    self.bodies[ib].angular_velocity += iib * d;
                }
                let bias = (pb - pa).scale(0.2 * inv_dt);
                point_constraint(&mut self.bodies, bias, 0.0);
            }
            JointKind::Distance {
                length,
                frequency,
                damping,
            } => {
                let d = pb - pa;
                let len = d.distance();
                if len < 1e-6 {
                    return;
                }
                let u = d.scale(1.0 / len);
                let (ba, bb) = (&self.bodies[ia], &self.bodies[ib]);
                let cra = cross(ra, u);
                let crb = cross(rb, u);
                let mut k = ma + mb + iia * cra * cra + iib * crb * crb;
                if k <= 0.0 {
                    return;
                }
                let c = len - length;
                let dv = bb.velocity + cross_sv(bb.angular_velocity, rb)
                    - ba.velocity
                    - cross_sv(ba.angular_velocity, ra);
                let cdot = dot(u, dv);
                let (bias, gamma) = if frequency > 0.0 {
                    let mass = 1.0 / k;
                    let omega = std::f32::consts::TAU * frequency;
                    let dc = 2.0 * mass * damping * omega;
                    let kk = mass * omega * omega;
                    let gamma = 1.0 / (dt * (dc + dt * kk));
                    (c * dt * kk * gamma, gamma)
                } else {
                    (0.2 * inv_dt * c, 0.0)
                };
                k += gamma;
                let imp = -(cdot + bias + gamma * self.joints[ji].angular_impulse) / k;
                self.joints[ji].angular_impulse += imp;
                Self::apply_pair_raw(&mut self.bodies, ia, ib, pb, u.scale(imp));
            }
            JointKind::Mouse { target, max_force } => {
                let body = &self.bodies[ib];
                let r = rb;
                let c = pb - target;
                let mass = body.mass().max(1e-6);
                let omega = std::f32::consts::TAU * 5.0;
                let dc = 2.0 * mass * 0.7 * omega;
                let kk = mass * omega * omega;
                let gamma = 1.0 / (dt * (dc + dt * kk));
                let beta = dt * kk * gamma;
                let k11 = ma * 0.0 + body.inv_mass + body.inv_inertia * r.dy * r.dy + gamma;
                let k12 = -body.inv_inertia * r.dx * r.dy;
                let k22 = body.inv_mass + body.inv_inertia * r.dx * r.dx + gamma;
                let det = k11 * k22 - k12 * k12;
                if det.abs() < 1e-12 {
                    return;
                }
                let vel = body.velocity + cross_sv(body.angular_velocity, r);
                let old = self.joints[ji].impulse;
                let rhs = v(
                    -(vel.dx + c.dx * beta + gamma * old.dx),
                    -(vel.dy + c.dy * beta + gamma * old.dy),
                );
                let mut imp = v(
                    (k22 * rhs.dx - k12 * rhs.dy) / det,
                    (k11 * rhs.dy - k12 * rhs.dx) / det,
                );
                let mut total = old + imp;
                let max = max_force * dt;
                if total.distance() > max {
                    total = total.scale(max / total.distance());
                }
                imp = total - old;
                self.joints[ji].impulse = total;
                let b = &mut self.bodies[ib];
                b.velocity = b.velocity + imp.scale(b.inv_mass);
                b.angular_velocity += b.inv_inertia * cross(r, imp);
                b.wake();
            }
        }
    }

    /// The nearest body hit by a ray from `origin` along `direction`
    /// within `max` — `Physics.Raycast`.
    #[must_use]
    pub fn raycast(&self, origin: V, direction: V, max: f32) -> Option<RayHit> {
        let d = norm(direction);
        let mut best: Option<RayHit> = None;
        for (i, b) in self.bodies.iter().enumerate() {
            if let Some((t, n)) = ray_body(b, origin, d, max) {
                if best.is_none_or(|h| t < h.distance) {
                    best = Some(RayHit {
                        body: i,
                        point: origin + d.scale(t),
                        normal: n,
                        distance: t,
                    });
                }
            }
        }
        best
    }

    /// Every body whose collider covers `p`.
    #[must_use]
    pub fn query_point(&self, p: V) -> Vec<usize> {
        (0..self.bodies.len())
            .filter(|&i| self.bodies[i].contains(p))
            .collect()
    }

    /// Total kinetic energy (translational + rotational).
    #[must_use]
    pub fn kinetic_energy(&self) -> f32 {
        self.bodies
            .iter()
            .filter(|b| b.inv_mass > 0.0)
            .map(|b| {
                let m = 1.0 / b.inv_mass;
                let i = if b.inv_inertia > 0.0 {
                    1.0 / b.inv_inertia
                } else {
                    0.0
                };
                0.5 * m * dot(b.velocity, b.velocity)
                    + 0.5 * i * b.angular_velocity * b.angular_velocity
            })
            .sum()
    }
}

fn ray_circle(o: V, d: V, c: V, r: f32, max: f32) -> Option<(f32, V)> {
    let oc = o - c;
    let b = dot(oc, d);
    let cc = dot(oc, oc) - r * r;
    let disc = b * b - cc;
    if disc < 0.0 {
        return None;
    }
    let t = -b - disc.sqrt();
    if t < 0.0 || t > max {
        return None;
    }
    let p = o + d.scale(t);
    Some((t, norm(p - c)))
}

fn ray_segment(o: V, d: V, a: V, b: V, max: f32) -> Option<(f32, V)> {
    let e = b - a;
    let den = cross(d, e);
    if den.abs() < 1e-12 {
        return None;
    }
    let t = cross(a - o, e) / den;
    let u = cross(a - o, d) / den;
    if t < 0.0 || t > max || !(0.0..=1.0).contains(&u) {
        return None;
    }
    let mut n = norm(perp(e));
    if dot(n, d) > 0.0 {
        n = v(-n.dx, -n.dy);
    }
    Some((t, n))
}

fn ray_body(b: &RigidBody, o: V, d: V, max: f32) -> Option<(f32, V)> {
    match &b.collider {
        Collider::Circle { radius, center } => {
            ray_circle(o, d, b.world_point(*center), *radius, max)
        }
        Collider::Polygon {
            vertices,
            normals,
            radius,
        } => {
            let w: Vec<V> = vertices.iter().map(|p| b.world_point(*p)).collect();
            let (s, c) = b.angle.sin_cos();
            let mut best: Option<(f32, V)> = None;
            let mut consider = |hit: Option<(f32, V)>| {
                if let Some(h) = hit {
                    if best.is_none_or(|x| h.0 < x.0) {
                        best = Some(h);
                    }
                }
            };
            let n = w.len();
            let edges = if n == 2 { 1 } else { n };
            for i in 0..edges {
                let (a, bb) = (w[i], w[(i + 1) % n]);
                if *radius > 0.0 {
                    let nn = if n == 2 {
                        rot(normals[1], c, s)
                    } else {
                        rot(normals[i], c, s)
                    };
                    for side in [nn, v(-nn.dx, -nn.dy)] {
                        consider(ray_segment(
                            o,
                            d,
                            a + side.scale(*radius),
                            bb + side.scale(*radius),
                            max,
                        ));
                    }
                    consider(ray_circle(o, d, a, *radius, max));
                    consider(ray_circle(o, d, bb, *radius, max));
                } else {
                    consider(ray_segment(o, d, a, bb, max));
                }
            }
            best
        }
    }
}

/// A kinematic capsule that walks — Godot's `CharacterBody2D`, Unity's
/// `CharacterController`.
#[derive(Debug, Clone)]
pub struct CharacterController {
    pub position: V,
    pub velocity: V,
    /// Capsule half-height of the straight part and radius (vertical).
    pub half_height: f32,
    pub radius: f32,
    /// A surface with normal.y below this (Y-down) counts as floor.
    pub floor_max: f32,
    pub on_floor: bool,
    pub on_wall: bool,
}

impl CharacterController {
    #[must_use]
    pub const fn new(position: V, half_height: f32, radius: f32) -> Self {
        Self {
            position,
            velocity: V::ZERO,
            half_height,
            radius,
            floor_max: -0.7,
            on_floor: false,
            on_wall: false,
        }
    }

    fn body(&self) -> RigidBody {
        RigidBody::kinematic(
            Collider::capsule(self.half_height * 2.0, self.radius),
            self.position,
        )
        .angle(std::f32::consts::FRAC_PI_2)
    }

    /// Move by `velocity * dt`, sliding along whatever static geometry is
    /// hit (up to four slides), and update `on_floor` / `on_wall`.
    pub fn move_and_slide(&mut self, world: &RigidWorld, dt: f32) {
        self.on_floor = false;
        self.on_wall = false;
        let mut motion = self.velocity.scale(dt);
        // Sub-step long moves so thin geometry is not skipped.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let steps = ((motion.distance() / (self.radius * 0.5)).ceil() as usize).clamp(1, 32);
        #[allow(clippy::cast_precision_loss)]
        let part = motion.scale(1.0 / steps as f32);
        motion = V::ZERO;
        let _ = motion;
        for _ in 0..steps {
            self.position = self.position + part;
            for _ in 0..4 {
                let me = self.body();
                let mut pushed = false;
                for other in &world.bodies {
                    if other.kind == BodyKind::Dynamic {
                        continue;
                    }
                    if let Some((n, pts)) = collide(other, &me) {
                        let pen = pts.iter().map(|p| p.penetration).fold(0.0, f32::max);
                        if pen <= 1e-4 {
                            continue;
                        }
                        // `n` points from the obstacle to us.
                        self.position = self.position + n.scale(pen);
                        let into = dot(self.velocity, n);
                        if into < 0.0 {
                            self.velocity = self.velocity - n.scale(into);
                        }
                        if n.dy <= self.floor_max {
                            self.on_floor = true;
                        } else if n.dy.abs() < 0.3 {
                            self.on_wall = true;
                        }
                        pushed = true;
                    }
                }
                if !pushed {
                    break;
                }
            }
        }
        // A floor probe just below the feet, so standing still still
        // reports the floor.
        let feet = self.position + v(0.0, self.half_height + self.radius + 0.5);
        if world
            .bodies
            .iter()
            .any(|b| b.kind != BodyKind::Dynamic && b.contains(feet))
        {
            self.on_floor = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn floor(w: &mut RigidWorld) -> usize {
        w.add(RigidBody::fixed(
            Collider::rect(2000.0, 40.0),
            v(0.0, 520.0),
        ))
    }

    fn run(w: &mut RigidWorld, seconds: f32) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        for _ in 0..(seconds / DT) as usize {
            w.step(DT);
        }
    }

    #[test]
    fn polygons_hull_and_face_outward() {
        let c = Collider::polygon(
            &[
                v(0.0, 0.0),
                v(1.0, 1.0),
                v(1.0, 0.0),
                v(0.0, 1.0),
                v(0.5, 0.5),
            ],
            0.0,
        );
        let Collider::Polygon {
            vertices, normals, ..
        } = c
        else {
            panic!()
        };
        assert_eq!(vertices.len(), 4, "interior point dropped");
        for (i, n) in normals.iter().enumerate() {
            let centre = v(0.5, 0.5);
            assert!(dot(*n, vertices[i] - centre) > 0.0, "normal {i} points out");
        }
    }

    #[test]
    fn a_box_lands_flat_and_rests_on_the_floor() {
        let mut w = RigidWorld::default();
        floor(&mut w);
        let b =
            w.add(RigidBody::dynamic(Collider::rect(40.0, 40.0), v(0.0, 300.0), 1.0).angle(0.3));
        run(&mut w, 4.0);
        let body = &w.bodies[b];
        assert!(
            (body.position.dy - 480.0).abs() < 2.0,
            "resting on top: {:?}",
            body.position
        );
        let a = body.angle.rem_euclid(std::f32::consts::FRAC_PI_2);
        assert!(
            !(0.05..=std::f32::consts::FRAC_PI_2 - 0.05).contains(&a),
            "toppled flat: {}",
            body.angle
        );
        assert!(body.sleeping, "and went to sleep");
    }

    #[test]
    fn a_stack_of_boxes_stands() {
        let mut w = RigidWorld::default();
        floor(&mut w);
        let ids: Vec<usize> = (0..6)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let y = 480.0 - 40.5 * i as f32;
                w.add(RigidBody::dynamic(
                    Collider::rect(40.0, 40.0),
                    v(0.0, y),
                    1.0,
                ))
            })
            .collect();
        run(&mut w, 5.0);
        let top = &w.bodies[*ids.last().unwrap()];
        assert!(
            top.position.dx.abs() < 2.0,
            "the stack did not fall over: {:?}",
            top.position
        );
        assert!(
            top.position.dy > 270.0 && top.position.dy < 285.0,
            "{:?}",
            top.position
        );
    }

    #[test]
    fn friction_holds_a_box_on_a_slope_that_a_slippery_box_slides_down() {
        let make = |mu: f32| {
            let mut w = RigidWorld {
                sleep_enabled: false,
                ..RigidWorld::default()
            };
            let slope = 0.35;
            w.add(
                RigidBody::fixed(Collider::rect(1000.0, 20.0), v(0.0, 400.0))
                    .angle(slope)
                    .friction(1.0),
            );
            let n = v(slope.sin(), -slope.cos());
            let start = v(0.0, 400.0) + n.scale(10.0 + 15.0 + 0.5);
            let b = w.add(
                RigidBody::dynamic(Collider::rect(30.0, 30.0), start, 1.0)
                    .angle(slope)
                    .friction(mu),
            );
            run(&mut w, 1.5);
            (w.bodies[b].position - start).distance()
        };
        let sticky = make(1.0);
        let slippery = make(0.0);
        assert!(sticky < 5.0, "static friction holds: moved {sticky}");
        assert!(slippery > 100.0, "frictionless slides: moved {slippery}");
    }

    #[test]
    fn restitution_bounces_and_energy_does_not_grow() {
        let mut w = RigidWorld {
            sleep_enabled: false,
            ..RigidWorld::default()
        };
        floor(&mut w);
        let b =
            w.add(RigidBody::dynamic(Collider::circle(20.0), v(0.0, 200.0), 1.0).restitution(0.8));
        let mut max_after = 0.0f32;
        let mut hit = false;
        for i in 0..300 {
            w.step(DT);
            if w.bodies[b].velocity.dy < 0.0 {
                hit = true;
            }
            if hit && i > 60 {
                max_after = max_after.max(500.0 - w.bodies[b].position.dy);
            }
        }
        assert!(hit, "it bounced");
        assert!(
            max_after < 300.0 && max_after > 60.0,
            "lower than dropped, but up again: {max_after}"
        );
    }

    #[test]
    fn circles_roll_on_a_slope_because_rotation_exists() {
        let mut w = RigidWorld {
            sleep_enabled: false,
            ..RigidWorld::default()
        };
        w.add(
            RigidBody::fixed(Collider::rect(2000.0, 20.0), v(0.0, 400.0))
                .angle(0.2)
                .friction(1.0),
        );
        let b = w.add(RigidBody::dynamic(Collider::circle(20.0), v(0.0, 360.0), 1.0).friction(1.0));
        run(&mut w, 2.0);
        assert!(
            w.bodies[b].angular_velocity > 0.5,
            "rolling: {}",
            w.bodies[b].angular_velocity
        );
    }

    #[test]
    fn capsules_collide_with_boxes_and_each_other() {
        let mut w = RigidWorld::default();
        floor(&mut w);
        let c1 = w.add(RigidBody::dynamic(
            Collider::capsule(60.0, 10.0),
            v(0.0, 400.0),
            1.0,
        ));
        let c2 =
            w.add(RigidBody::dynamic(Collider::capsule(60.0, 10.0), v(5.0, 300.0), 1.0).angle(0.4));
        run(&mut w, 4.0);
        assert!(
            (w.bodies[c1].position.dy - 490.0).abs() < 3.0,
            "{:?}",
            w.bodies[c1].position
        );
        assert!(
            w.bodies[c2].position.dy < 485.0,
            "resting on the other: {:?}",
            w.bodies[c2].position
        );
    }

    #[test]
    fn revolute_pendulum_keeps_its_length_and_swings() {
        let mut w = RigidWorld {
            sleep_enabled: false,
            ..RigidWorld::default()
        };
        let anchor = w.add(RigidBody::fixed(Collider::circle(2.0), v(0.0, 0.0)));
        let bob = w.add(RigidBody::dynamic(
            Collider::circle(10.0),
            v(100.0, 0.0),
            1.0,
        ));
        let j = RigidJoint::between(&w, anchor, v(0.0, 0.0), bob, v(100.0, 0.0), 0.0, 0.0);
        w.add_joint(j);
        let mut max_x = 0.0f32;
        for i in 0..240 {
            w.step(DT);
            let d = w.bodies[bob].position.distance();
            assert!((d - 100.0).abs() < 3.0, "step {i}: length {d}");
            if i > 60 {
                max_x = max_x.min(w.bodies[bob].position.dx);
            }
        }
        assert!(max_x < -50.0, "swung to the other side: {max_x}");
    }

    #[test]
    fn revolute_motor_spins_and_limits_hold() {
        let mut w = RigidWorld::new(V::ZERO);
        w.sleep_enabled = false;
        let hub = w.add(RigidBody::fixed(Collider::circle(2.0), v(0.0, 0.0)));
        let wheel = w.add(RigidBody::dynamic(
            Collider::rect(80.0, 10.0),
            v(0.0, 0.0),
            1.0,
        ));
        let mut j = RigidJoint::at_anchor(
            &w,
            hub,
            wheel,
            v(0.0, 0.0),
            JointKind::Revolute {
                limits: None,
                motor: Some((3.0, 1e9)),
            },
        );
        j.set_motor_speed(3.0);
        w.add_joint(j);
        run(&mut w, 1.0);
        assert!((w.bodies[wheel].angular_velocity - 3.0).abs() < 0.05);
        assert!(w.bodies[wheel].position.distance() < 0.5, "stays pinned");

        let mut w = RigidWorld {
            sleep_enabled: false,
            ..RigidWorld::default()
        };
        let hub = w.add(RigidBody::fixed(Collider::circle(2.0), v(0.0, 0.0)));
        let arm = w.add(RigidBody::dynamic(
            Collider::rect(80.0, 10.0),
            v(40.0, 0.0),
            1.0,
        ));
        w.add_joint(RigidJoint::at_anchor(
            &w,
            hub,
            arm,
            v(0.0, 0.0),
            JointKind::Revolute {
                limits: Some((-0.3, 0.3)),
                motor: None,
            },
        ));
        run(&mut w, 2.0);
        assert!(
            w.bodies[arm].angle <= 0.36,
            "limited: {}",
            w.bodies[arm].angle
        );
    }

    #[test]
    fn weld_and_spring_joints() {
        let mut w = RigidWorld {
            sleep_enabled: false,
            ..RigidWorld::default()
        };
        let wall = w.add(RigidBody::fixed(Collider::rect(10.0, 10.0), v(0.0, 0.0)));
        let beam = w.add(RigidBody::dynamic(
            Collider::rect(100.0, 10.0),
            v(55.0, 0.0),
            1.0,
        ));
        w.add_joint(RigidJoint::at_anchor(
            &w,
            wall,
            beam,
            v(5.0, 0.0),
            JointKind::Weld,
        ));
        run(&mut w, 1.0);
        assert!(
            w.bodies[beam].angle.abs() < 0.2,
            "a welded cantilever barely droops: {}",
            w.bodies[beam].angle
        );

        let mut w = RigidWorld {
            sleep_enabled: false,
            ..RigidWorld::default()
        };
        let top = w.add(RigidBody::fixed(Collider::circle(2.0), v(0.0, 0.0)));
        let bob = w.add(RigidBody::dynamic(
            Collider::circle(10.0),
            v(0.0, 100.0),
            1.0,
        ));
        w.add_joint(RigidJoint::between(
            &w,
            top,
            v(0.0, 0.0),
            bob,
            v(0.0, 100.0),
            0.5,
            0.05,
        ));
        let ys: Vec<f32> = (0..120)
            .map(|_| {
                w.step(DT);
                w.bodies[bob].position.dy
            })
            .collect();
        let (lo, hi) = ys
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), &y| (a.min(y), b.max(y)));
        assert!(
            hi - lo > 20.0,
            "the spring stretches and oscillates: {lo}..{hi}"
        );
    }

    #[test]
    fn mouse_joint_drags_toward_the_target() {
        let mut w = RigidWorld::new(V::ZERO);
        let ground = w.add(RigidBody::fixed(Collider::circle(1.0), v(-1000.0, -1000.0)));
        let b = w.add(RigidBody::dynamic(
            Collider::rect(20.0, 20.0),
            v(0.0, 0.0),
            1.0,
        ));
        w.add_joint(RigidJoint::at_anchor(
            &w,
            ground,
            b,
            v(0.0, 0.0),
            JointKind::Mouse {
                target: v(100.0, 50.0),
                max_force: 1e7,
            },
        ));
        run(&mut w, 2.0);
        assert!(
            (w.bodies[b].position - v(100.0, 50.0)).distance() < 5.0,
            "{:?}",
            w.bodies[b].position
        );
    }

    #[test]
    fn bullets_do_not_tunnel_through_thin_walls() {
        let make = |bullet: bool| {
            let mut w = RigidWorld::new(V::ZERO);
            w.add(RigidBody::fixed(Collider::rect(4.0, 400.0), v(300.0, 0.0)));
            let mut ball = RigidBody::dynamic(Collider::circle(5.0), v(0.0, 0.0), 1.0)
                .velocity(v(12000.0, 0.0));
            if bullet {
                ball = ball.bullet();
            }
            let b = w.add(ball);
            run(&mut w, 0.1);
            (w.bodies[b].position.dx, w.stats.toi_events)
        };
        let (x_plain, _) = make(false);
        assert!(x_plain > 300.0, "without CCD the ball tunnels: {x_plain}");
        let (x_ccd, _) = make(true);
        assert!(x_ccd < 300.0, "with CCD it is stopped: {x_ccd}");
    }

    #[test]
    fn raycasts_and_point_queries() {
        let mut w = RigidWorld::default();
        let bx = w.add(RigidBody::fixed(Collider::rect(20.0, 20.0), v(100.0, 0.0)));
        let c = w.add(RigidBody::fixed(Collider::circle(10.0), v(200.0, 0.0)));
        let hit = w.raycast(v(0.0, 0.0), v(1.0, 0.0), 1000.0).unwrap();
        assert_eq!(hit.body, bx);
        assert!((hit.distance - 90.0).abs() < 1e-3);
        assert!((hit.normal - v(-1.0, 0.0)).distance() < 1e-4);
        let hit2 = w.raycast(v(150.0, 0.0), v(1.0, 0.0), 1000.0).unwrap();
        assert_eq!(hit2.body, c);
        assert!((hit2.distance - 40.0).abs() < 1e-3);
        assert!(w.raycast(v(0.0, 50.0), v(1.0, 0.0), 1000.0).is_none());
        assert_eq!(w.query_point(v(105.0, 5.0)), vec![bx]);
    }

    #[test]
    fn contact_events_begin_and_end() {
        let mut w = RigidWorld {
            sleep_enabled: false,
            ..RigidWorld::default()
        };
        floor(&mut w);
        w.add(RigidBody::dynamic(Collider::circle(10.0), v(0.0, 400.0), 1.0).restitution(0.0));
        let mut begins = 0;
        for _ in 0..120 {
            w.step(DT);
            begins += w
                .events()
                .iter()
                .filter(|e| matches!(e, ContactEvent::Begin(..)))
                .count();
        }
        assert_eq!(begins, 1);
    }

    #[test]
    fn character_walks_stands_and_is_blocked_by_walls() {
        let mut w = RigidWorld::default();
        floor(&mut w);
        w.add(RigidBody::fixed(
            Collider::rect(20.0, 200.0),
            v(200.0, 400.0),
        ));
        let mut ch = CharacterController::new(v(0.0, 420.0), 20.0, 12.0);
        for _ in 0..120 {
            ch.velocity = v(200.0, ch.velocity.dy + 980.0 * DT);
            ch.move_and_slide(&w, DT);
        }
        assert!(ch.on_floor, "standing");
        assert!(
            (ch.position.dy - (500.0 - 32.0)).abs() < 2.0,
            "{:?}",
            ch.position
        );
        assert!(
            ch.position.dx < 190.0 - 12.0 + 1.0,
            "stopped by the wall: {:?}",
            ch.position
        );
        assert!(ch.on_wall);
    }
}
