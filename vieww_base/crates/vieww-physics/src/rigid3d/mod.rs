//! 3D rigid-body dynamics — the PhysX / Chaos / Godot Physics / Rapier row
//! of the document (Unity L4, Unreal L4, Godot L5, R3F's
//! `@react-three/rapier`), on vieww's own arithmetic.
//!
//! # What is in it
//!
//! * **Shapes** ([`Shape3`]): spheres, capsules, arbitrary convex hulls
//!   (boxes are one), and static half-spaces.
//! * **Bodies** ([`Body3`]): dynamic, static or kinematic; mass and full
//!   inertia tensors computed from the shape and a density (hulls by exact
//!   polyhedral integration); friction, restitution, damping, gravity scale.
//! * **Broadphase**: sweep-and-prune on the x axis of world AABBs.
//! * **Narrowphase** ([`collide`](collide::collide)): analytic
//!   sphere/capsule pairs, GJK distance for round-vs-polyhedron, SAT with
//!   reference/incident face clipping for polyhedron pairs (up to four
//!   points per manifold), planes against everything.
//! * **Solver**: sequential impulses (Catto) with warm starting matched by
//!   body-local contact position, two friction directions per point,
//!   restitution above a bounce threshold, and Baumgarte stabilisation with
//!   slop — the formulation Box2D and Bullet share.
//! * **Joints** ([`Joint3`]): ball-and-socket with an optional swing cone
//!   (ragdoll shoulders and hips), hinges with limits and a motor, fixed
//!   welds, and distance springs.
//! * **Sleeping**, **continuous collision** for fast bodies (a swept ray
//!   against static geometry, so a bullet cannot tunnel a wall),
//!   **ray casts**, **contact events** (begin/end), and a kinematic
//!   **character controller** ([`CharacterController3`]) with
//!   move-and-slide, slope limits and ground detection.
//!
//! Like the 2D engine: `step(dt)` is the only thing that moves time and
//! the world is a pure function of its inputs — no hash-map iteration
//! order, no wall clock, no threads.

pub mod collide;
pub mod fracture;
pub mod hull;
pub mod math;

use std::collections::BTreeMap;

pub use collide::{collide, ray_shape, Contact, Iso, Shape3};
pub use hull::{Hull, MassProps};
pub use math::{M3, Q, V3};

/// How a body moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    Dynamic,
    /// Never moves.
    Static,
    /// Moved by its velocity, unaffected by forces or contacts (platforms,
    /// characters).
    Kinematic,
}

/// A rigid body.
#[derive(Debug, Clone)]
pub struct Body3 {
    pub shape: Shape3,
    pub pose: Iso,
    pub lin_vel: V3,
    pub ang_vel: V3,
    pub kind: BodyKind,
    pub inv_mass: f32,
    /// Body-local inverse inertia.
    pub inv_inertia: M3,
    pub friction: f32,
    pub restitution: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub gravity_scale: f32,
    /// Sweep against static geometry each step.
    pub ccd: bool,
    pub sleeping: bool,
    sleep_time: f32,
    /// Free for the caller (an entity id, say).
    pub user: u64,
}

impl Body3 {
    /// A dynamic body of `density` (kg/m³).
    #[must_use]
    pub fn dynamic(shape: Shape3, density: f32) -> Self {
        let (mass, inertia) = mass_of(&shape, density);
        Self {
            inv_mass: if mass > 0.0 { 1.0 / mass } else { 0.0 },
            inv_inertia: inertia.inverse().unwrap_or(M3::ZERO),
            kind: BodyKind::Dynamic,
            ..Self::fixed(shape)
        }
    }

    /// A static body.
    #[must_use]
    pub fn fixed(shape: Shape3) -> Self {
        Self {
            shape,
            pose: Iso::default(),
            lin_vel: V3::ZERO,
            ang_vel: V3::ZERO,
            kind: BodyKind::Static,
            inv_mass: 0.0,
            inv_inertia: M3::ZERO,
            friction: 0.5,
            restitution: 0.1,
            linear_damping: 0.01,
            angular_damping: 0.05,
            gravity_scale: 1.0,
            ccd: false,
            sleeping: false,
            sleep_time: 0.0,
            user: 0,
        }
    }

    /// A kinematic body.
    #[must_use]
    pub fn kinematic(shape: Shape3) -> Self {
        Self {
            kind: BodyKind::Kinematic,
            ..Self::fixed(shape)
        }
    }

    #[must_use]
    pub fn at(mut self, p: V3) -> Self {
        self.pose.p = p;
        self
    }
    #[must_use]
    pub fn rotated(mut self, q: Q) -> Self {
        self.pose.q = q;
        self
    }
    #[must_use]
    pub fn with_velocity(mut self, v: V3) -> Self {
        self.lin_vel = v;
        self
    }
    #[must_use]
    pub fn with_spin(mut self, w: V3) -> Self {
        self.ang_vel = w;
        self
    }
    #[must_use]
    pub fn with_friction(mut self, f: f32) -> Self {
        self.friction = f;
        self
    }
    #[must_use]
    pub fn with_restitution(mut self, r: f32) -> Self {
        self.restitution = r;
        self
    }

    #[must_use]
    pub fn mass(&self) -> f32 {
        if self.inv_mass > 0.0 {
            1.0 / self.inv_mass
        } else {
            f32::INFINITY
        }
    }

    /// World-space inverse inertia.
    #[must_use]
    pub fn inv_inertia_world(&self) -> M3 {
        let r = self.pose.q.to_m3();
        r.mul(&self.inv_inertia).mul(&r.transpose())
    }

    fn is_dynamic(&self) -> bool {
        self.kind == BodyKind::Dynamic
    }

    /// Velocity of the material point at world `p`.
    #[must_use]
    pub fn velocity_at(&self, p: V3) -> V3 {
        self.lin_vel + self.ang_vel.cross(p - self.pose.p)
    }

    /// Apply an impulse at world point `p`.
    pub fn apply_impulse(&mut self, j: V3, p: V3) {
        if !self.is_dynamic() {
            return;
        }
        self.lin_vel += j * self.inv_mass;
        self.ang_vel += self.inv_inertia_world().mul_v((p - self.pose.p).cross(j));
        self.sleeping = false;
        self.sleep_time = 0.0;
    }

    /// Kinetic energy.
    #[must_use]
    pub fn kinetic_energy(&self) -> f32 {
        if !self.is_dynamic() {
            return 0.0;
        }
        let r = self.pose.q.to_m3();
        let inertia = r
            .mul(&self.inv_inertia.inverse().unwrap_or(M3::ZERO))
            .mul(&r.transpose());
        0.5 * self.mass() * self.lin_vel.length_sq()
            + 0.5 * self.ang_vel.dot(inertia.mul_v(self.ang_vel))
    }

    fn aabb(&self) -> (V3, V3) {
        match &self.shape {
            Shape3::Sphere { radius } => (
                self.pose.p - V3::splat(*radius),
                self.pose.p + V3::splat(*radius),
            ),
            Shape3::Capsule {
                half_height,
                radius,
            } => {
                let a = self.pose.apply(V3::new(0.0, -half_height, 0.0));
                let b = self.pose.apply(V3::new(0.0, *half_height, 0.0));
                (a.min(b) - V3::splat(*radius), a.max(b) + V3::splat(*radius))
            }
            Shape3::Convex { hull } => {
                let mut lo = V3::splat(f32::INFINITY);
                let mut hi = V3::splat(f32::NEG_INFINITY);
                for v in &hull.verts {
                    let w = self.pose.apply(*v);
                    lo = lo.min(w);
                    hi = hi.max(w);
                }
                (lo, hi)
            }
            Shape3::Plane { .. } => (V3::splat(f32::NEG_INFINITY), V3::splat(f32::INFINITY)),
        }
    }
}

/// Mass and body-local inertia of `shape` at `density`.
#[must_use]
pub fn mass_of(shape: &Shape3, density: f32) -> (f32, M3) {
    match shape {
        Shape3::Sphere { radius } => {
            let m = density * 4.0 / 3.0 * std::f32::consts::PI * radius.powi(3);
            (m, M3::diagonal(V3::splat(0.4 * m * radius * radius)))
        }
        Shape3::Capsule {
            half_height,
            radius,
        } => {
            let pi = std::f32::consts::PI;
            let (r, h) = (*radius, 2.0 * half_height);
            let mc = density * pi * r * r * h;
            let ms = density * 4.0 / 3.0 * pi * r.powi(3);
            let iy = mc * r * r / 2.0 + ms * 2.0 * r * r / 5.0;
            let ixz = mc * (h * h / 12.0 + r * r / 4.0)
                + ms * (2.0 * r * r / 5.0 + h * h / 4.0 + 3.0 * h * r / 8.0);
            (mc + ms, M3::diagonal(V3::new(ixz, iy, ixz)))
        }
        Shape3::Convex { hull } => {
            let mp = hull.mass_props();
            (mp.volume * density, mp.inertia.scale(density))
        }
        Shape3::Plane { .. } => (0.0, M3::ZERO),
    }
}

/// A joint between bodies `a` and `b`.
#[derive(Debug, Clone, PartialEq)]
pub enum Joint3 {
    /// Anchors coincide. With `cone`, the angle between the bodies' local
    /// `axis` vectors is limited to `cone.1` radians (a swing limit).
    Ball {
        a: usize,
        b: usize,
        anchor_a: V3,
        anchor_b: V3,
        cone: Option<(V3, V3, f32)>,
    },
    /// Anchors coincide and the local axes stay parallel. Optional angle
    /// limits (radians, about the axis) and a velocity motor
    /// `(target speed, max impulse per step)`.
    Hinge {
        a: usize,
        b: usize,
        anchor_a: V3,
        anchor_b: V3,
        axis_a: V3,
        axis_b: V3,
        /// A vector perpendicular to the axis, in each body's frame — the
        /// pair the hinge angle is measured between (0 at creation).
        ref_a: V3,
        ref_b: V3,
        limits: Option<(f32, f32)>,
        motor: Option<(f32, f32)>,
    },
    /// Welded: anchors coincide and the relative rotation is frozen.
    Fixed {
        a: usize,
        b: usize,
        anchor_a: V3,
        anchor_b: V3,
        rel: Q,
    },
    /// The anchors stay `rest` apart (a rod), or pull together like a
    /// spring when `stiffness` < 1.
    Distance {
        a: usize,
        b: usize,
        anchor_a: V3,
        anchor_b: V3,
        rest: f32,
        stiffness: f32,
    },
}

/// A contact-pair transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContactEvent {
    Began(usize, usize),
    Ended(usize, usize),
}

#[derive(Debug, Clone, Copy)]
struct Cached {
    local_a: V3,
    normal_impulse: f32,
    tangent_impulse: [f32; 2],
}

#[derive(Debug, Clone, Copy)]
struct Point {
    ra: V3,
    rb: V3,
    normal: V3,
    t: [V3; 2],
    mass_n: f32,
    mass_t: [f32; 2],
    bias: f32,
    jn: f32,
    jt: [f32; 2],
    local_a: V3,
}

#[derive(Debug, Clone)]
struct Manifold {
    a: usize,
    b: usize,
    friction: f32,
    points: Vec<Point>,
}

/// The 3D world. See the [module docs](self).
#[derive(Debug, Clone)]
pub struct World3 {
    pub bodies: Vec<Body3>,
    pub joints: Vec<Joint3>,
    pub gravity: V3,
    pub velocity_iterations: usize,
    /// Baumgarte factor and allowed penetration.
    pub baumgarte: f32,
    pub slop: f32,
    /// Below this approach speed contacts do not bounce.
    pub bounce_threshold: f32,
    pub sleep_linear: f32,
    pub sleep_angular: f32,
    pub time_to_sleep: f32,
    cache: BTreeMap<(usize, usize), Vec<Cached>>,
    touching: std::collections::BTreeSet<(usize, usize)>,
    pub events: Vec<ContactEvent>,
    /// Manifold count and contact points of the last step.
    pub last_pairs: usize,
    pub last_points: usize,
}

impl Default for World3 {
    fn default() -> Self {
        Self::new()
    }
}

impl World3 {
    #[must_use]
    pub fn new() -> Self {
        Self {
            bodies: Vec::new(),
            joints: Vec::new(),
            gravity: V3::new(0.0, -9.81, 0.0),
            velocity_iterations: 12,
            baumgarte: 0.2,
            slop: 0.005,
            bounce_threshold: 1.0,
            sleep_linear: 0.05,
            sleep_angular: 0.08,
            time_to_sleep: 0.5,
            cache: BTreeMap::new(),
            touching: std::collections::BTreeSet::new(),
            events: Vec::new(),
            last_pairs: 0,
            last_points: 0,
        }
    }

    /// Add a body; returns its index.
    pub fn add(&mut self, b: Body3) -> usize {
        self.bodies.push(b);
        self.bodies.len() - 1
    }

    /// Add a joint; returns its index.
    pub fn join(&mut self, j: Joint3) -> usize {
        // Anything jointed starts awake.
        let (a, b) = joint_bodies(&j);
        self.bodies[a].sleeping = false;
        self.bodies[b].sleeping = false;
        self.joints.push(j);
        self.joints.len() - 1
    }

    /// A welded joint at world point `at`, keeping the current relative pose.
    #[must_use]
    pub fn fixed_joint(&self, a: usize, b: usize, at: V3) -> Joint3 {
        let (pa, pb) = (self.bodies[a].pose, self.bodies[b].pose);
        Joint3::Fixed {
            a,
            b,
            anchor_a: pa.inverse_apply(at),
            anchor_b: pb.inverse_apply(at),
            rel: pa.q.conjugate() * pb.q,
        }
    }

    /// A ball joint at world point `at`.
    #[must_use]
    pub fn ball_joint(&self, a: usize, b: usize, at: V3) -> Joint3 {
        Joint3::Ball {
            a,
            b,
            anchor_a: self.bodies[a].pose.inverse_apply(at),
            anchor_b: self.bodies[b].pose.inverse_apply(at),
            cone: None,
        }
    }

    /// A hinge at world point `at` about world `axis`.
    #[must_use]
    pub fn hinge_joint(&self, a: usize, b: usize, at: V3, axis: V3) -> Joint3 {
        let (pa, pb) = (self.bodies[a].pose, self.bodies[b].pose);
        let (r, _) = axis.normalize().basis();
        Joint3::Hinge {
            a,
            b,
            anchor_a: pa.inverse_apply(at),
            anchor_b: pb.inverse_apply(at),
            axis_a: pa.q.conjugate().rotate(axis.normalize()),
            axis_b: pb.q.conjugate().rotate(axis.normalize()),
            ref_a: pa.q.conjugate().rotate(r),
            ref_b: pb.q.conjugate().rotate(r),
            limits: None,
            motor: None,
        }
    }

    /// Total kinetic energy.
    #[must_use]
    pub fn kinetic_energy(&self) -> f32 {
        self.bodies.iter().map(Body3::kinetic_energy).sum()
    }

    /// Nearest ray hit: `(body, distance, normal)`.
    #[must_use]
    pub fn raycast(&self, origin: V3, dir: V3, max_t: f32) -> Option<(usize, f32, V3)> {
        let dir = dir.normalize();
        let mut best: Option<(usize, f32, V3)> = None;
        for (i, b) in self.bodies.iter().enumerate() {
            let limit = best.map_or(max_t, |h| h.1);
            if let Some((t, n)) = ray_shape(origin, dir, limit, &b.shape, &b.pose) {
                if best.is_none_or(|h| t < h.1) {
                    best = Some((i, t, n));
                }
            }
        }
        best
    }

    fn broadphase(&self) -> Vec<(usize, usize)> {
        // Jointed bodies do not collide with each other (every engine's
        // default — a hinge's pin sits inside both parts).
        let jointed: std::collections::BTreeSet<(usize, usize)> = self
            .joints
            .iter()
            .map(|j| {
                let (a, b) = joint_bodies(j);
                (a.min(b), a.max(b))
            })
            .collect();
        let boxes: Vec<(V3, V3)> = self.bodies.iter().map(Body3::aabb).collect();
        let mut order: Vec<usize> = (0..self.bodies.len()).collect();
        order.sort_by(|a, b| boxes[*a].0.x.total_cmp(&boxes[*b].0.x).then(a.cmp(b)));
        let mut pairs = Vec::new();
        for (k, &i) in order.iter().enumerate() {
            for &j in &order[k + 1..] {
                if boxes[j].0.x > boxes[i].1.x {
                    break;
                }
                let (bi, bj) = (&self.bodies[i], &self.bodies[j]);
                let active = |b: &Body3| b.kind != BodyKind::Static && !b.sleeping;
                if !active(bi) && !active(bj) {
                    continue;
                }
                if !bi.is_dynamic() && !bj.is_dynamic() {
                    continue;
                }
                let (lo1, hi1) = boxes[i];
                let (lo2, hi2) = boxes[j];
                if lo1.y > hi2.y || lo2.y > hi1.y || lo1.z > hi2.z || lo2.z > hi1.z {
                    continue;
                }
                if jointed.contains(&(i.min(j), i.max(j))) {
                    continue;
                }
                pairs.push((i.min(j), i.max(j)));
            }
        }
        pairs.sort_unstable();
        pairs
    }

    /// Advance by `dt` seconds.
    #[allow(clippy::too_many_lines)]
    pub fn step(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        self.events.clear();
        let g = self.gravity;
        // 1. Forces → velocities.
        for b in &mut self.bodies {
            if b.is_dynamic() && !b.sleeping {
                b.lin_vel += g * (b.gravity_scale * dt);
                b.lin_vel *= 1.0 / (1.0 + dt * b.linear_damping);
                b.ang_vel *= 1.0 / (1.0 + dt * b.angular_damping);
            }
        }
        // 2–3. Collision.
        let pairs = self.broadphase();
        let mut manifolds = Vec::new();
        let mut now_touching = std::collections::BTreeSet::new();
        for (a, b) in pairs {
            let (ba, bb) = (&self.bodies[a], &self.bodies[b]);
            let contacts = collide(&ba.shape, &ba.pose, &bb.shape, &bb.pose);
            if contacts.is_empty() {
                continue;
            }
            now_touching.insert((a, b));
            // Waking: a moving body touching a sleeping one wakes it.
            let cached = self.cache.get(&(a, b)).cloned().unwrap_or_default();
            let ia = ba.inv_inertia_world();
            let ib = bb.inv_inertia_world();
            let restitution = ba.restitution.max(bb.restitution);
            let mut points = Vec::new();
            for c in contacts {
                let ra = c.point - ba.pose.p;
                let rb = c.point - bb.pose.p;
                let n = c.normal;
                let (t0, t1) = n.basis();
                let eff = |dir: V3| {
                    let k = ba.inv_mass
                        + bb.inv_mass
                        + ra.cross(dir).dot(ia.mul_v(ra.cross(dir)))
                        + rb.cross(dir).dot(ib.mul_v(rb.cross(dir)));
                    if k > 0.0 {
                        1.0 / k
                    } else {
                        0.0
                    }
                };
                let rel = bb.velocity_at(c.point) - ba.velocity_at(c.point);
                let vn = rel.dot(n);
                let mut bias = self.baumgarte / dt * (c.depth - self.slop).max(0.0);
                if vn < -self.bounce_threshold {
                    bias = bias.max(-restitution * vn);
                }
                let local_a = ba.pose.inverse_apply(c.point);
                let warm = cached
                    .iter()
                    .find(|k| (k.local_a - local_a).length() < 0.05)
                    .copied();
                let (jn, jt) =
                    warm.map_or((0.0, [0.0, 0.0]), |w| (w.normal_impulse, w.tangent_impulse));
                points.push(Point {
                    ra,
                    rb,
                    normal: n,
                    t: [t0, t1],
                    mass_n: eff(n),
                    mass_t: [eff(t0), eff(t1)],
                    bias,
                    jn,
                    jt,
                    local_a,
                });
            }
            manifolds.push(Manifold {
                a,
                b,
                friction: (ba.friction * bb.friction).sqrt(),
                points,
            });
        }
        for p in &now_touching {
            if !self.touching.contains(p) {
                self.events.push(ContactEvent::Began(p.0, p.1));
            }
            let (a, b) = *p;
            let moving = |x: &Body3| {
                !x.sleeping
                    && x.kind != BodyKind::Static
                    && (x.lin_vel.length() > self.sleep_linear * 2.0
                        || x.ang_vel.length() > self.sleep_angular * 2.0)
            };
            if moving(&self.bodies[a]) && self.bodies[b].sleeping {
                self.bodies[b].sleeping = false;
                self.bodies[b].sleep_time = 0.0;
            }
            if moving(&self.bodies[b]) && self.bodies[a].sleeping {
                self.bodies[a].sleeping = false;
                self.bodies[a].sleep_time = 0.0;
            }
        }
        for p in &self.touching {
            if !now_touching.contains(p) {
                self.events.push(ContactEvent::Ended(p.0, p.1));
            }
        }
        self.touching = now_touching;
        self.last_pairs = manifolds.len();
        self.last_points = manifolds.iter().map(|m| m.points.len()).sum();

        // 4. Warm start.
        for m in &manifolds {
            for p in &m.points {
                let j = p.normal * p.jn + p.t[0] * p.jt[0] + p.t[1] * p.jt[1];
                self.impulse_pair(m.a, m.b, j, p.ra, p.rb);
            }
        }
        // 5–6. Iterate.
        for _ in 0..self.velocity_iterations {
            for ji in 0..self.joints.len() {
                self.solve_joint(ji, dt);
            }
            for m in &mut manifolds {
                for k in 0..m.points.len() {
                    let p = m.points[k];
                    // Friction first (Box2D order), bounded by the normal.
                    let mut p = p;
                    for d in 0..2 {
                        let rel = self.bodies[m.b].velocity_at(self.bodies[m.b].pose.p + p.rb)
                            - self.bodies[m.a].velocity_at(self.bodies[m.a].pose.p + p.ra);
                        let vt = rel.dot(p.t[d]);
                        let max = m.friction * p.jn;
                        let new = (p.jt[d] - vt * p.mass_t[d]).clamp(-max, max);
                        let dj = new - p.jt[d];
                        p.jt[d] = new;
                        self.impulse_pair(m.a, m.b, p.t[d] * dj, p.ra, p.rb);
                    }
                    let rel = self.bodies[m.b].velocity_at(self.bodies[m.b].pose.p + p.rb)
                        - self.bodies[m.a].velocity_at(self.bodies[m.a].pose.p + p.ra);
                    let vn = rel.dot(p.normal);
                    let new = (p.jn + (p.bias - vn) * p.mass_n).max(0.0);
                    let dj = new - p.jn;
                    p.jn = new;
                    self.impulse_pair(m.a, m.b, p.normal * dj, p.ra, p.rb);
                    m.points[k] = p;
                }
            }
        }
        // Remember impulses for next step's warm start.
        self.cache.clear();
        for m in &manifolds {
            self.cache.insert(
                (m.a, m.b),
                m.points
                    .iter()
                    .map(|p| Cached {
                        local_a: p.local_a,
                        normal_impulse: p.jn,
                        tangent_impulse: p.jt,
                    })
                    .collect(),
            );
        }
        // 7. Integrate positions (with CCD against static shapes).
        let statics: Vec<usize> = (0..self.bodies.len())
            .filter(|i| self.bodies[*i].kind == BodyKind::Static)
            .collect();
        for i in 0..self.bodies.len() {
            let b = &self.bodies[i];
            if b.kind == BodyKind::Static || b.sleeping {
                continue;
            }
            let mut motion = b.lin_vel * dt;
            if b.ccd && b.is_dynamic() {
                let len = motion.length();
                let r = match &b.shape {
                    Shape3::Sphere { radius } => *radius,
                    Shape3::Capsule { radius, .. } => *radius,
                    s => s.bounding_radius() * 0.5,
                };
                if len > r * 0.5 {
                    let dir = motion / len;
                    let mut hit: Option<(f32, V3)> = None;
                    for &s in &statics {
                        let sb = &self.bodies[s];
                        if let Some((t, n)) = ray_shape(b.pose.p, dir, len + r, &sb.shape, &sb.pose)
                        {
                            if hit.is_none_or(|h| t < h.0) {
                                hit = Some((t, n));
                            }
                        }
                    }
                    if let Some((t, n)) = hit {
                        let allowed = (t - r).max(0.0);
                        if allowed < len {
                            motion = dir * allowed;
                            let b = &mut self.bodies[i];
                            let vn = b.lin_vel.dot(n);
                            if vn < 0.0 {
                                b.lin_vel -= n * (vn * (1.0 + b.restitution));
                            }
                        }
                    }
                }
            }
            let b = &mut self.bodies[i];
            b.pose.p += motion;
            b.pose.q = b.pose.q.integrate(b.ang_vel, dt);
        }
        // 8. Sleep.
        for b in &mut self.bodies {
            if !b.is_dynamic() || b.sleeping {
                continue;
            }
            if b.lin_vel.length() < self.sleep_linear && b.ang_vel.length() < self.sleep_angular {
                b.sleep_time += dt;
                if b.sleep_time >= self.time_to_sleep {
                    b.sleeping = true;
                    b.lin_vel = V3::ZERO;
                    b.ang_vel = V3::ZERO;
                }
            } else {
                b.sleep_time = 0.0;
            }
        }
    }

    fn impulse_pair(&mut self, a: usize, b: usize, j: V3, ra: V3, rb: V3) {
        {
            let ba = &mut self.bodies[a];
            if ba.is_dynamic() {
                ba.lin_vel -= j * ba.inv_mass;
                let iw = ba.inv_inertia_world();
                ba.ang_vel -= iw.mul_v(ra.cross(j));
            }
        }
        let bb = &mut self.bodies[b];
        if bb.is_dynamic() {
            bb.lin_vel += j * bb.inv_mass;
            let iw = bb.inv_inertia_world();
            bb.ang_vel += iw.mul_v(rb.cross(j));
        }
    }

    fn angular_impulse(&mut self, a: usize, b: usize, j: V3) {
        if self.bodies[a].is_dynamic() {
            let iw = self.bodies[a].inv_inertia_world();
            self.bodies[a].ang_vel -= iw.mul_v(j);
        }
        if self.bodies[b].is_dynamic() {
            let iw = self.bodies[b].inv_inertia_world();
            self.bodies[b].ang_vel += iw.mul_v(j);
        }
    }

    /// The 3×3 point-constraint mass matrix.
    fn point_k(&self, a: usize, b: usize, ra: V3, rb: V3) -> M3 {
        let (ba, bb) = (&self.bodies[a], &self.bodies[b]);
        let ma = if ba.is_dynamic() { ba.inv_mass } else { 0.0 };
        let mb = if bb.is_dynamic() { bb.inv_mass } else { 0.0 };
        let ia = if ba.is_dynamic() {
            ba.inv_inertia_world()
        } else {
            M3::ZERO
        };
        let ib = if bb.is_dynamic() {
            bb.inv_inertia_world()
        } else {
            M3::ZERO
        };
        let sa = M3::skew(ra);
        let sb = M3::skew(rb);
        M3::diagonal(V3::splat(ma + mb))
            .add(&sa.mul(&ia).mul(&sa).scale(-1.0))
            .add(&sb.mul(&ib).mul(&sb).scale(-1.0))
    }

    fn solve_point(&mut self, a: usize, b: usize, anchor_a: V3, anchor_b: V3, dt: f32) {
        let ra = self.bodies[a].pose.rotate(anchor_a);
        let rb = self.bodies[b].pose.rotate(anchor_b);
        let pa = self.bodies[a].pose.p + ra;
        let pb = self.bodies[b].pose.p + rb;
        let err = pb - pa;
        let rel = self.bodies[b].velocity_at(pb) - self.bodies[a].velocity_at(pa);
        let k = self.point_k(a, b, ra, rb);
        if let Some(ki) = k.inverse() {
            let j = ki.mul_v(-(rel + err * (self.baumgarte / dt)));
            self.impulse_pair(a, b, j, ra, rb);
        }
    }

    fn angular_mass(&self, a: usize, b: usize, axis: V3) -> f32 {
        let (ba, bb) = (&self.bodies[a], &self.bodies[b]);
        let ia = if ba.is_dynamic() {
            ba.inv_inertia_world()
        } else {
            M3::ZERO
        };
        let ib = if bb.is_dynamic() {
            bb.inv_inertia_world()
        } else {
            M3::ZERO
        };
        let k = axis.dot(ia.mul_v(axis)) + axis.dot(ib.mul_v(axis));
        if k > 1e-12 {
            1.0 / k
        } else {
            0.0
        }
    }

    #[allow(clippy::too_many_lines)]
    fn solve_joint(&mut self, ji: usize, dt: f32) {
        let joint = self.joints[ji].clone();
        match joint {
            Joint3::Ball {
                a,
                b,
                anchor_a,
                anchor_b,
                cone,
            } => {
                self.solve_point(a, b, anchor_a, anchor_b, dt);
                if let Some((axis_a, axis_b, angle)) = cone {
                    let wa = self.bodies[a].pose.rotate(axis_a);
                    let wb = self.bodies[b].pose.rotate(axis_b);
                    let cos = wa.dot(wb).clamp(-1.0, 1.0);
                    let current = cos.acos();
                    if current > angle {
                        let n = wa.cross(wb).normalize();
                        if n.length_sq() > 0.0 {
                            let rel = (self.bodies[b].ang_vel - self.bodies[a].ang_vel).dot(n);
                            let bias = (current - angle) * self.baumgarte / dt;
                            let m = self.angular_mass(a, b, n);
                            let j = (-(rel + bias) * m).min(0.0);
                            self.angular_impulse(a, b, n * j);
                        }
                    }
                }
            }
            Joint3::Hinge {
                a,
                b,
                anchor_a,
                anchor_b,
                axis_a,
                axis_b,
                ref_a,
                ref_b,
                limits,
                motor,
            } => {
                self.solve_point(a, b, anchor_a, anchor_b, dt);
                let wa = self.bodies[a].pose.rotate(axis_a);
                let wb = self.bodies[b].pose.rotate(axis_b);
                // Keep the axes parallel: two angular rows perpendicular to wa.
                let err = wa.cross(wb);
                let (p0, p1) = wa.basis();
                for d in [p0, p1] {
                    let rel = (self.bodies[b].ang_vel - self.bodies[a].ang_vel).dot(d);
                    let c = err.dot(d);
                    let m = self.angular_mass(a, b, d);
                    let j = -(rel + c * self.baumgarte / dt) * m;
                    self.angular_impulse(a, b, d * j);
                }
                // The hinge angle about wa, between the two references.
                let ra_w = self.bodies[a].pose.rotate(ref_a);
                let rb_w = self.bodies[b].pose.rotate(ref_b);
                let angle = wa.dot(ra_w.cross(rb_w)).atan2(ra_w.dot(rb_w));
                if let Some((speed, max)) = motor {
                    let rel = (self.bodies[b].ang_vel - self.bodies[a].ang_vel).dot(wa);
                    let m = self.angular_mass(a, b, wa);
                    let j = ((speed - rel) * m).clamp(-max, max);
                    self.angular_impulse(a, b, wa * j);
                }
                if let Some((lo, hi)) = limits {
                    let rel = (self.bodies[b].ang_vel - self.bodies[a].ang_vel).dot(wa);
                    let m = self.angular_mass(a, b, wa);
                    if angle < lo {
                        let j = (-(rel - (lo - angle) * self.baumgarte / dt) * m).max(0.0);
                        self.angular_impulse(a, b, wa * j);
                    } else if angle > hi {
                        let j = (-(rel + (angle - hi) * self.baumgarte / dt) * m).min(0.0);
                        self.angular_impulse(a, b, wa * j);
                    }
                }
            }
            Joint3::Fixed {
                a,
                b,
                anchor_a,
                anchor_b,
                rel,
            } => {
                self.solve_point(a, b, anchor_a, anchor_b, dt);
                // Rotation error: qa * rel vs qb.
                let target = self.bodies[a].pose.q * rel;
                let err_q = self.bodies[b].pose.q * target.conjugate();
                let err = err_q.to_rotation_vector();
                for d in [V3::X, V3::Y, V3::Z] {
                    let relw = (self.bodies[b].ang_vel - self.bodies[a].ang_vel).dot(d);
                    let m = self.angular_mass(a, b, d);
                    let j = -(relw + err.dot(d) * self.baumgarte / dt) * m;
                    self.angular_impulse(a, b, d * j);
                }
            }
            Joint3::Distance {
                a,
                b,
                anchor_a,
                anchor_b,
                rest,
                stiffness,
            } => {
                let ra = self.bodies[a].pose.rotate(anchor_a);
                let rb = self.bodies[b].pose.rotate(anchor_b);
                let pa = self.bodies[a].pose.p + ra;
                let pb = self.bodies[b].pose.p + rb;
                let d = pb - pa;
                let len = d.length();
                if len < 1e-9 {
                    return;
                }
                let n = d / len;
                let rel = (self.bodies[b].velocity_at(pb) - self.bodies[a].velocity_at(pa)).dot(n);
                let (ba, bb) = (&self.bodies[a], &self.bodies[b]);
                let ia = if ba.is_dynamic() {
                    ba.inv_inertia_world()
                } else {
                    M3::ZERO
                };
                let ib = if bb.is_dynamic() {
                    bb.inv_inertia_world()
                } else {
                    M3::ZERO
                };
                let ma = if ba.is_dynamic() { ba.inv_mass } else { 0.0 };
                let mb = if bb.is_dynamic() { bb.inv_mass } else { 0.0 };
                let k = ma
                    + mb
                    + ra.cross(n).dot(ia.mul_v(ra.cross(n)))
                    + rb.cross(n).dot(ib.mul_v(rb.cross(n)));
                if k <= 0.0 {
                    return;
                }
                let c = len - rest;
                let j = -(rel + c * self.baumgarte * stiffness.clamp(0.0, 1.0) / dt) / k
                    * stiffness.clamp(0.0, 1.0).max(0.05);
                self.impulse_pair(a, b, n * j, ra, rb);
            }
        }
    }
}

fn joint_bodies(j: &Joint3) -> (usize, usize) {
    match j {
        Joint3::Ball { a, b, .. }
        | Joint3::Hinge { a, b, .. }
        | Joint3::Fixed { a, b, .. }
        | Joint3::Distance { a, b, .. } => (*a, *b),
    }
}

/// A kinematic capsule that walks: move-and-slide against the world's
/// non-dynamic bodies (and pushes nothing — Unity's `CharacterController`,
/// Godot's `CharacterBody3D`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharacterController3 {
    pub position: V3,
    pub half_height: f32,
    pub radius: f32,
    /// Steepest walkable slope, radians.
    pub max_slope: f32,
    pub grounded: bool,
    pub ground_normal: V3,
}

impl CharacterController3 {
    #[must_use]
    pub fn new(position: V3, half_height: f32, radius: f32) -> Self {
        Self {
            position,
            half_height,
            radius,
            max_slope: 0.8,
            grounded: false,
            ground_normal: V3::Y,
        }
    }

    /// Try to move by `motion`; slide along whatever is in the way.
    pub fn move_and_slide(&mut self, world: &World3, motion: V3) -> V3 {
        let shape = Shape3::capsule(self.half_height, self.radius);
        let start = self.position;
        let mut pos = self.position + motion;
        self.grounded = false;
        let cos_slope = self.max_slope.cos();
        for _ in 0..6 {
            let mut pushed = false;
            for b in &world.bodies {
                if b.is_dynamic() {
                    continue;
                }
                let me = Iso::new(pos, Q::IDENTITY);
                for c in collide(&b.shape, &b.pose, &shape, &me) {
                    // Normal points from the obstacle to us.
                    pos += c.normal * (c.depth + 1e-4);
                    pushed = true;
                    if c.normal.y >= cos_slope {
                        self.grounded = true;
                        self.ground_normal = c.normal;
                    }
                }
            }
            if !pushed {
                break;
            }
        }
        // A walkable slope should not slide us sideways down it: keep only
        // the vertical correction there.
        if self.grounded {
            let d = pos - start;
            let lateral_drift = V3::new(d.x - motion.x, 0.0, d.z - motion.z);
            if lateral_drift.length() < 1e-3 + motion.length() * 0.05 {
                pos = V3::new(start.x + motion.x, pos.y, start.z + motion.z);
            }
        }
        self.position = pos;
        pos - start
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ground(w: &mut World3) {
        w.add(Body3::fixed(Shape3::plane(V3::Y)));
    }

    fn run(w: &mut World3, seconds: f32) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        for _ in 0..(seconds * 120.0) as usize {
            w.step(1.0 / 120.0);
        }
    }

    #[test]
    fn a_dropped_ball_lands_and_rests_on_the_ground() {
        let mut w = World3::new();
        ground(&mut w);
        let ball = w.add(Body3::dynamic(Shape3::sphere(0.5), 1000.0).at(V3::new(0.0, 3.0, 0.0)));
        run(&mut w, 3.0);
        let y = w.bodies[ball].pose.p.y;
        assert!((y - 0.5).abs() < 0.02, "{y}");
        assert!(w.bodies[ball].sleeping);
    }

    #[test]
    fn a_stack_of_five_boxes_stands() {
        let mut w = World3::new();
        ground(&mut w);
        let mut ids = vec![];
        for i in 0..5 {
            #[allow(clippy::cast_precision_loss)]
            let y = 0.5 + i as f32 * 1.0;
            ids.push(w.add(
                Body3::dynamic(Shape3::cuboid(V3::splat(0.5)), 500.0).at(V3::new(0.0, y, 0.0)),
            ));
        }
        run(&mut w, 4.0);
        for (i, id) in ids.iter().enumerate() {
            let p = w.bodies[*id].pose.p;
            #[allow(clippy::cast_precision_loss)]
            let expect = 0.5 + i as f32;
            assert!((p.y - expect).abs() < 0.05, "box {i}: {p:?}");
            assert!(
                p.x.abs() < 0.05 && p.z.abs() < 0.05,
                "box {i} drifted: {p:?}"
            );
        }
    }

    #[test]
    fn a_tilted_box_topples_flat() {
        let mut w = World3::new();
        ground(&mut w);
        let id = w.add(
            Body3::dynamic(Shape3::cuboid(V3::new(0.5, 1.0, 0.5)), 500.0)
                .at(V3::new(0.0, 1.3, 0.0))
                .rotated(Q::from_axis_angle(V3::Z, 0.6)),
        );
        run(&mut w, 5.0);
        let up = w.bodies[id].pose.q.rotate(V3::Y);
        // Lying down: its long axis is horizontal; resting at half-width.
        assert!(up.y.abs() < 0.1, "{up:?}");
        assert!((w.bodies[id].pose.p.y - 0.5).abs() < 0.05);
    }

    #[test]
    fn bounciness_follows_restitution() {
        let mut w = World3::new();
        w.add(Body3::fixed(Shape3::plane(V3::Y)).with_restitution(0.9));
        let ball = w.add(
            Body3::dynamic(Shape3::sphere(0.25), 1000.0)
                .at(V3::new(0.0, 2.0, 0.0))
                .with_restitution(0.9),
        );
        let mut peak_after_bounce = 0.0f32;
        let mut bounced = false;
        for _ in 0..240 {
            w.step(1.0 / 120.0);
            let b = &w.bodies[ball];
            if b.lin_vel.y > 0.0 {
                bounced = true;
            }
            if bounced {
                peak_after_bounce = peak_after_bounce.max(b.pose.p.y);
            }
        }
        // Drop of 1.75 m to contact; e = 0.9 → rebound ≈ 0.81 × 1.75 + 0.25.
        assert!(bounced);
        assert!(
            peak_after_bounce > 1.2 && peak_after_bounce < 1.9,
            "{peak_after_bounce}"
        );
    }

    #[test]
    fn a_pendulum_on_a_ball_joint_swings_and_keeps_its_length() {
        let mut w = World3::new();
        let pivot = w.add(Body3::fixed(Shape3::sphere(0.05)).at(V3::new(0.0, 5.0, 0.0)));
        let bob = w.add(Body3::dynamic(Shape3::sphere(0.2), 1000.0).at(V3::new(2.0, 5.0, 0.0)));
        let j = w.ball_joint(pivot, bob, V3::new(0.0, 5.0, 0.0));
        w.join(j);
        let mut lowest = f32::INFINITY;
        for _ in 0..240 {
            w.step(1.0 / 240.0);
            let p = w.bodies[bob].pose.p;
            lowest = lowest.min(p.y);
            let len = (p - V3::new(0.0, 5.0, 0.0)).length();
            assert!((len - 2.0).abs() < 0.05, "{len}");
        }
        assert!(lowest < 3.2, "{lowest}");
    }

    #[test]
    fn a_motorised_hinge_spins_at_its_target_speed() {
        let mut w = World3::new();
        w.gravity = V3::ZERO;
        let base = w.add(Body3::fixed(Shape3::sphere(0.1)));
        let wheel = w.add(Body3::dynamic(
            Shape3::cuboid(V3::new(1.0, 0.1, 0.1)),
            100.0,
        ));
        let mut j = w.hinge_joint(base, wheel, V3::ZERO, V3::Y);
        if let Joint3::Hinge { motor, .. } = &mut j {
            *motor = Some((3.0, 50.0));
        }
        w.join(j);
        run(&mut w, 1.0);
        let wv = w.bodies[wheel].ang_vel;
        assert!((wv.y - 3.0).abs() < 0.1, "{wv:?}");
        assert!(wv.x.abs() < 0.05 && wv.z.abs() < 0.05);
    }

    #[test]
    fn a_hinge_limit_stops_a_falling_door() {
        let mut w = World3::new();
        let post = w.add(Body3::fixed(Shape3::sphere(0.05)));
        let arm = w.add(
            Body3::dynamic(Shape3::cuboid(V3::new(0.5, 0.05, 0.05)), 500.0)
                .at(V3::new(0.5, 0.0, 0.0)),
        );
        let mut j = w.hinge_joint(post, arm, V3::ZERO, V3::Z);
        if let Joint3::Hinge { limits, .. } = &mut j {
            *limits = Some((-0.3, 0.3));
        }
        w.join(j);
        run(&mut w, 2.0);
        let dir = w.bodies[arm].pose.q.rotate(V3::X);
        let angle = dir.y.atan2(dir.x);
        assert!(angle > -0.4 && angle < -0.2, "{angle}");
    }

    #[test]
    fn welded_bodies_move_as_one() {
        let mut w = World3::new();
        ground(&mut w);
        let a = w
            .add(Body3::dynamic(Shape3::cuboid(V3::splat(0.25)), 500.0).at(V3::new(0.0, 2.0, 0.0)));
        let b = w
            .add(Body3::dynamic(Shape3::cuboid(V3::splat(0.25)), 500.0).at(V3::new(0.5, 2.0, 0.0)));
        let j = w.fixed_joint(a, b, V3::new(0.25, 2.0, 0.0));
        w.join(j);
        w.bodies[a].ang_vel = V3::new(0.0, 0.0, 2.0);
        run(&mut w, 2.0);
        let d = (w.bodies[b].pose.p - w.bodies[a].pose.p).length();
        assert!((d - 0.5).abs() < 0.03, "{d}");
    }

    #[test]
    fn ccd_stops_a_bullet_at_a_thin_wall() {
        let mut w = World3::new();
        w.gravity = V3::ZERO;
        w.add(Body3::fixed(Shape3::cuboid(V3::new(0.02, 2.0, 2.0))).at(V3::new(5.0, 0.0, 0.0)));
        let mut bullet =
            Body3::dynamic(Shape3::sphere(0.05), 8000.0).with_velocity(V3::new(400.0, 0.0, 0.0));
        bullet.ccd = true;
        let id = w.add(bullet);
        for _ in 0..10 {
            w.step(1.0 / 60.0);
        }
        assert!(
            w.bodies[id].pose.p.x < 5.0,
            "tunnelled: {:?}",
            w.bodies[id].pose.p
        );
    }

    #[test]
    fn raycasts_find_the_nearest_body() {
        let mut w = World3::new();
        ground(&mut w);
        let b =
            w.add(Body3::dynamic(Shape3::cuboid(V3::splat(0.5)), 1.0).at(V3::new(0.0, 2.0, 0.0)));
        let (hit, t, n) = w
            .raycast(V3::new(0.0, 10.0, 0.0), -V3::Y, 100.0)
            .expect("hit");
        assert_eq!(hit, b);
        assert!((t - 7.5).abs() < 1e-4 && n.y > 0.99);
    }

    #[test]
    fn contact_events_begin_and_end() {
        let mut w = World3::new();
        ground(&mut w);
        let ball = w.add(
            Body3::dynamic(Shape3::sphere(0.5), 1000.0)
                .at(V3::new(0.0, 1.0, 0.0))
                .with_restitution(0.0),
        );
        let mut began = false;
        for _ in 0..120 {
            w.step(1.0 / 120.0);
            if w.events.contains(&ContactEvent::Began(0, ball)) {
                began = true;
            }
        }
        assert!(began);
        w.bodies[ball].sleeping = false;
        w.bodies[ball].lin_vel = V3::new(0.0, 10.0, 0.0);
        w.step(1.0 / 120.0);
        w.step(1.0 / 120.0);
        let ended = w.events.contains(&ContactEvent::Ended(0, ball));
        assert!(ended, "{:?}", w.events);
    }

    #[test]
    fn energy_does_not_grow_in_a_resting_pile() {
        let mut w = World3::new();
        ground(&mut w);
        for i in 0..12 {
            #[allow(clippy::cast_precision_loss)]
            let f = i as f32;
            let shape = match i % 3 {
                0 => Shape3::sphere(0.3),
                1 => Shape3::cuboid(V3::new(0.3, 0.2, 0.25)),
                _ => Shape3::capsule(0.3, 0.2),
            };
            w.add(Body3::dynamic(shape, 600.0).at(V3::new(
                (f * 0.7).sin() * 0.6,
                0.5 + f * 0.7,
                (f * 1.3).cos() * 0.6,
            )));
        }
        run(&mut w, 6.0);
        // Spheres and capsules may keep rolling (there is no rolling
        // resistance — physically right); everything with corners must
        // have come to rest.
        let angular: f32 = w
            .bodies
            .iter()
            .filter(|b| matches!(b.shape, Shape3::Convex { .. }))
            .map(Body3::kinetic_energy)
            .sum();
        assert!(angular < 0.5, "{angular}");
        for b in &w.bodies[1..] {
            assert!(b.pose.p.y > 0.0 && b.pose.p.y < 4.0, "{:?}", b.pose.p);
        }
    }

    #[test]
    fn the_character_walks_on_the_floor_and_is_stopped_by_a_wall() {
        let mut w = World3::new();
        ground(&mut w);
        w.add(Body3::fixed(Shape3::cuboid(V3::new(0.5, 2.0, 5.0))).at(V3::new(3.0, 2.0, 0.0)));
        let mut c = CharacterController3::new(V3::new(0.0, 0.7, 0.0), 0.4, 0.3);
        for _ in 0..100 {
            c.move_and_slide(&w, V3::new(0.1, -0.05, 0.0));
        }
        assert!(c.grounded);
        assert!((c.position.y - 0.7).abs() < 0.02, "{:?}", c.position);
        // Wall face at x = 2.5; capsule radius 0.3.
        assert!(c.position.x <= 2.2 + 0.01, "{:?}", c.position);
    }

    #[test]
    fn convex_hull_mass_matches_a_box_of_the_same_shape() {
        let a = mass_of(&Shape3::cuboid(V3::new(0.5, 1.0, 1.5)), 2.0);
        assert!((a.0 - 12.0).abs() < 1e-3);
        let ixx = 12.0 / 12.0 * (4.0 + 9.0);
        assert!((a.1.m[0][0] - ixx).abs() < 0.01);
        let s = mass_of(&Shape3::sphere(1.0), 1.0);
        assert!((s.0 - 4.18879).abs() < 1e-3);
    }
}
