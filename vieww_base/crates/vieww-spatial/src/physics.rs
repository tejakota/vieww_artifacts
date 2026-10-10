//! Small deterministic rigid-body stepping for UI scenes, interactive previews
//! and simple games. It is intentionally a compact built-in solver rather than
//! a binding to a third-party physics engine.

use crate::Vec3;

/// Collider primitives currently supported by the built-in broad/narrow phase.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColliderShape {
    Sphere {
        radius: f32,
    },
    /// Axis-aligned in world space; rotation is intentionally ignored.
    Box {
        half_extents: Vec3,
    },
}
impl ColliderShape {
    fn valid(self) -> bool {
        match self {
            Self::Sphere { radius } => radius.is_finite() && radius > 0.0,
            Self::Box { half_extents } => {
                half_extents.is_finite()
                    && half_extents.x > 0.0
                    && half_extents.y > 0.0
                    && half_extents.z > 0.0
            }
        }
    }
}

/// One body. `inverse_mass == 0` makes a static body without a second type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RigidBody {
    pub position: Vec3,
    pub velocity: Vec3,
    pub force: Vec3,
    pub shape: ColliderShape,
    pub inverse_mass: f32,
    pub restitution: f32,
    pub friction: f32,
    pub linear_damping: f32,
    pub enabled: bool,
}
impl RigidBody {
    pub fn dynamic(shape: ColliderShape, position: Vec3, mass: f32) -> Result<Self, PhysicsError> {
        if !position.is_finite() || !shape.valid() || !mass.is_finite() || mass <= 0.0 {
            return Err(PhysicsError::InvalidBody);
        }
        let inverse_mass = 1.0 / mass;
        if !inverse_mass.is_finite() || inverse_mass <= 0.0 {
            return Err(PhysicsError::InvalidBody);
        }
        Ok(Self {
            position,
            velocity: Vec3::ZERO,
            force: Vec3::ZERO,
            shape,
            inverse_mass,
            restitution: 0.05,
            friction: 0.6,
            linear_damping: 0.02,
            enabled: true,
        })
    }
    pub fn fixed(shape: ColliderShape, position: Vec3) -> Result<Self, PhysicsError> {
        if !position.is_finite() || !shape.valid() {
            return Err(PhysicsError::InvalidBody);
        }
        Ok(Self {
            position,
            velocity: Vec3::ZERO,
            force: Vec3::ZERO,
            shape,
            inverse_mass: 0.0,
            restitution: 0.05,
            friction: 0.6,
            linear_damping: 0.02,
            enabled: true,
        })
    }
    #[must_use]
    pub fn is_static(self) -> bool {
        self.inverse_mass <= 0.0
    }
    pub fn apply_force(&mut self, force: Vec3) -> Result<(), PhysicsError> {
        if !force.is_finite() {
            return Err(PhysicsError::InvalidBody);
        }
        if !self.is_static() {
            let accumulated = self.force + force;
            if !accumulated.is_finite() {
                return Err(PhysicsError::InvalidBody);
            }
            self.force = accumulated;
        }
        Ok(())
    }
}

/// Diagnostics from world construction/stepping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicsError {
    InvalidBody,
    InvalidStep,
}
impl std::fmt::Display for PhysicsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self { Self::InvalidBody => "body has invalid shape, mass, coefficient or state", Self::InvalidStep => "physics delta must be finite and non-negative, and fixed step must be in (0, 0.25] seconds" })
    }
}
impl std::error::Error for PhysicsError {}

fn body_is_valid(body: RigidBody) -> bool {
    body.position.is_finite()
        && body.velocity.is_finite()
        && body.force.is_finite()
        && body.shape.valid()
        && body.inverse_mass.is_finite()
        && body.inverse_mass >= 0.0
        && body.restitution.is_finite()
        && (0.0..=1.0).contains(&body.restitution)
        && body.friction.is_finite()
        && body.friction >= 0.0
        && body.linear_damping.is_finite()
        && (0.0..=1.0).contains(&body.linear_damping)
}

#[derive(Debug, Clone, Copy)]
struct Contact {
    a: usize,
    b: usize,
    normal: Vec3,
    penetration: f32,
}

/// Maximum accepted physics substep. Smaller fixed steps are recommended for
/// stable simulation of fast-moving bodies and contact-rich scenes.
pub const MAX_FIXED_STEP_SECONDS: f32 = 0.25;
/// Hard safety ceiling on stepping/solver work for caller-controlled config.
pub const MAX_SUBSTEPS: u32 = 64;

/// Fixed-step, semi-implicit Euler world with bounded catch-up work.
#[derive(Debug, Clone)]
pub struct PhysicsWorld {
    pub gravity: Vec3,
    pub fixed_delta_seconds: f32,
    pub max_substeps: u32,
    pub solver_iterations: u32,
    accumulator: f32,
    bodies: Vec<RigidBody>,
}
impl Default for PhysicsWorld {
    fn default() -> Self {
        Self {
            gravity: Vec3::new(0.0, -9.81, 0.0),
            fixed_delta_seconds: 1.0 / 60.0,
            max_substeps: 8,
            solver_iterations: 5,
            accumulator: 0.0,
            bodies: Vec::new(),
        }
    }
}
impl PhysicsWorld {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add_body(&mut self, body: RigidBody) -> Result<usize, PhysicsError> {
        if !body_is_valid(body) {
            return Err(PhysicsError::InvalidBody);
        }
        let index = self.bodies.len();
        self.bodies.push(body);
        Ok(index)
    }
    #[must_use]
    pub fn bodies(&self) -> &[RigidBody] {
        &self.bodies
    }
    pub fn bodies_mut(&mut self) -> &mut [RigidBody] {
        &mut self.bodies
    }
    pub fn clear(&mut self) {
        self.bodies.clear();
        self.accumulator = 0.0;
    }

    /// Advance by elapsed wall time. Excess backlog is intentionally discarded
    /// after a bounded number of substeps so an application cannot enter a
    /// spiral of death. A substep above 250 ms is rejected as numerically unsafe.
    pub fn step(&mut self, delta_seconds: f32) -> Result<u32, PhysicsError> {
        if !delta_seconds.is_finite()
            || delta_seconds < 0.0
            || !self.fixed_delta_seconds.is_finite()
            || self.fixed_delta_seconds <= 0.0
            || self.fixed_delta_seconds > MAX_FIXED_STEP_SECONDS
        {
            return Err(PhysicsError::InvalidStep);
        }
        if !self.gravity.is_finite() || self.bodies.iter().any(|body| !body_is_valid(*body)) {
            return Err(PhysicsError::InvalidBody);
        }
        // Keep work bounded even if a public configuration field is corrupted.
        // The whole call is transactional: overflow in integration/contact math
        // restores the prior state instead of leaving NaNs in the public world.
        let previous_bodies = self.bodies.clone();
        let previous_accumulator = self.accumulator;
        let max_substeps = self.max_substeps.clamp(1, MAX_SUBSTEPS);
        let max_accumulator = f64::from(self.fixed_delta_seconds) * f64::from(max_substeps);
        self.accumulator =
            (f64::from(self.accumulator) + f64::from(delta_seconds)).min(max_accumulator) as f32;
        let mut steps = 0;
        while self.accumulator >= self.fixed_delta_seconds && steps < max_substeps {
            self.integrate(self.fixed_delta_seconds);
            if self.bodies.iter().any(|body| !body_is_valid(*body)) {
                self.bodies = previous_bodies;
                self.accumulator = previous_accumulator;
                return Err(PhysicsError::InvalidBody);
            }
            self.accumulator -= self.fixed_delta_seconds;
            steps += 1;
        }
        Ok(steps)
    }

    fn integrate(&mut self, dt: f32) {
        for body in &mut self.bodies {
            if !body.enabled || body.is_static() {
                body.force = Vec3::ZERO;
                continue;
            }
            let acceleration = self.gravity + body.force * body.inverse_mass;
            body.velocity = body.velocity + acceleration * dt;
            body.velocity =
                body.velocity * (1.0 - body.linear_damping.clamp(0.0, 1.0) * dt).max(0.0);
            body.position = body.position + body.velocity * dt;
            body.force = Vec3::ZERO;
        }
        for _ in 0..self.solver_iterations.clamp(1, MAX_SUBSTEPS) {
            let mut contacts = Vec::new();
            for (a, body_a) in self.bodies.iter().copied().enumerate() {
                if !body_a.enabled {
                    continue;
                }
                for (b, body_b) in self.bodies.iter().copied().enumerate().skip(a + 1) {
                    if !body_b.enabled || (body_a.is_static() && body_b.is_static()) {
                        continue;
                    }
                    if let Some(contact) = collide(a, body_a, b, body_b) {
                        contacts.push(contact);
                    }
                }
            }
            if contacts.is_empty() {
                break;
            }
            for contact in contacts {
                self.solve_contact(contact);
            }
        }
    }

    fn solve_contact(&mut self, contact: Contact) {
        let (left, right) = self.bodies.split_at_mut(contact.b);
        let a = &mut left[contact.a];
        let b = &mut right[0];
        let inv_a = if a.is_static() { 0.0 } else { a.inverse_mass };
        let inv_b = if b.is_static() { 0.0 } else { b.inverse_mass };
        let inv_sum = inv_a + inv_b;
        if inv_sum <= 0.0 {
            return;
        }
        let correction = contact.normal * ((contact.penetration - 0.001).max(0.0) * 0.65 / inv_sum);
        if inv_a > 0.0 {
            a.position = a.position - correction * inv_a;
        }
        if inv_b > 0.0 {
            b.position = b.position + correction * inv_b;
        }
        let relative = b.velocity - a.velocity;
        let normal_velocity = relative.dot(contact.normal);
        if normal_velocity > 0.0 {
            return;
        }
        let restitution = a.restitution.min(b.restitution).clamp(0.0, 1.0);
        let impulse_magnitude = -(1.0 + restitution) * normal_velocity / inv_sum;
        let impulse = contact.normal * impulse_magnitude;
        if inv_a > 0.0 {
            a.velocity = a.velocity - impulse * inv_a;
        }
        if inv_b > 0.0 {
            b.velocity = b.velocity + impulse * inv_b;
        }
        let relative_after = b.velocity - a.velocity;
        let tangent_raw = relative_after - contact.normal * relative_after.dot(contact.normal);
        let tangent = tangent_raw.normalized();
        if tangent != Vec3::ZERO {
            let jt = -relative_after.dot(tangent) / inv_sum;
            let friction = (a.friction * b.friction).max(0.0).sqrt();
            let tangent_impulse =
                tangent * jt.clamp(-impulse_magnitude * friction, impulse_magnitude * friction);
            if inv_a > 0.0 {
                a.velocity = a.velocity - tangent_impulse * inv_a;
            }
            if inv_b > 0.0 {
                b.velocity = b.velocity + tangent_impulse * inv_b;
            }
        }
    }
}

fn collide(a_index: usize, a: RigidBody, b_index: usize, b: RigidBody) -> Option<Contact> {
    let (normal, penetration) = match (a.shape, b.shape) {
        (ColliderShape::Sphere { radius: ra }, ColliderShape::Sphere { radius: rb }) => {
            let delta = b.position - a.position;
            let distance = delta.length();
            let penetration = ra + rb - distance;
            if penetration <= 0.0 {
                return None;
            }
            (
                if distance > 1.0e-7 {
                    delta / distance
                } else {
                    Vec3::X
                },
                penetration,
            )
        }
        (ColliderShape::Box { half_extents: ea }, ColliderShape::Box { half_extents: eb }) => {
            let delta = b.position - a.position;
            let overlap = Vec3::new(
                ea.x + eb.x - delta.x.abs(),
                ea.y + eb.y - delta.y.abs(),
                ea.z + eb.z - delta.z.abs(),
            );
            if overlap.x <= 0.0 || overlap.y <= 0.0 || overlap.z <= 0.0 {
                return None;
            }
            if overlap.x <= overlap.y && overlap.x <= overlap.z {
                (
                    Vec3::new(if delta.x < 0.0 { -1.0 } else { 1.0 }, 0.0, 0.0),
                    overlap.x,
                )
            } else if overlap.y <= overlap.z {
                (
                    Vec3::new(0.0, if delta.y < 0.0 { -1.0 } else { 1.0 }, 0.0),
                    overlap.y,
                )
            } else {
                (
                    Vec3::new(0.0, 0.0, if delta.z < 0.0 { -1.0 } else { 1.0 }),
                    overlap.z,
                )
            }
        }
        (ColliderShape::Sphere { radius }, ColliderShape::Box { half_extents }) => {
            let local = a.position - b.position;
            let closest = local.clamp(-half_extents, half_extents);
            let delta_from_sphere_to_box = closest - local;
            let distance = delta_from_sphere_to_box.length();
            if distance > 1.0e-7 {
                let penetration = radius - distance;
                if penetration <= 0.0 {
                    return None;
                }
                (delta_from_sphere_to_box / distance, penetration)
            } else {
                // The sphere center is inside the box. Choose the nearest face
                // and point the contact normal inward so the solver moves the
                // sphere out through that face rather than deeper into the box.
                let margins = half_extents - Vec3::new(local.x.abs(), local.y.abs(), local.z.abs());
                let (normal, margin) = if margins.x <= margins.y && margins.x <= margins.z {
                    (
                        Vec3::new(if local.x >= 0.0 { -1.0 } else { 1.0 }, 0.0, 0.0),
                        margins.x,
                    )
                } else if margins.y <= margins.z {
                    (
                        Vec3::new(0.0, if local.y >= 0.0 { -1.0 } else { 1.0 }, 0.0),
                        margins.y,
                    )
                } else {
                    (
                        Vec3::new(0.0, 0.0, if local.z >= 0.0 { -1.0 } else { 1.0 }),
                        margins.z,
                    )
                };
                (normal, radius + margin.max(0.0))
            }
        }
        (ColliderShape::Box { .. }, ColliderShape::Sphere { .. }) => {
            let reverse = collide(b_index, b, a_index, a)?;
            (-reverse.normal, reverse.penetration)
        }
    };
    Some(Contact {
        a: a_index,
        b: b_index,
        normal,
        penetration,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_step_is_repeatable_and_bounded() {
        let mut world = PhysicsWorld::new();
        world
            .add_body(
                RigidBody::dynamic(ColliderShape::Sphere { radius: 0.25 }, Vec3::ZERO, 1.0)
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(world.step(1.0 / 60.0).unwrap(), 1);
        assert!(world.bodies()[0].position.y < 0.0);
        assert!(world.step(10.0).unwrap() <= world.max_substeps);
    }

    #[test]
    fn fixed_floor_resolves_sphere_penetration() {
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        world
            .add_body(
                RigidBody::fixed(
                    ColliderShape::Box {
                        half_extents: Vec3::new(4.0, 0.5, 4.0),
                    },
                    Vec3::new(0.0, -0.5, 0.0),
                )
                .unwrap(),
            )
            .unwrap();
        world
            .add_body(
                RigidBody::dynamic(
                    ColliderShape::Sphere { radius: 0.5 },
                    Vec3::new(0.0, 0.2, 0.0),
                    1.0,
                )
                .unwrap(),
            )
            .unwrap();
        world.step(1.0 / 60.0).unwrap();
        assert!(world.bodies()[1].position.y >= 0.48);
    }

    #[test]
    fn invalid_mass_is_not_silently_treated_as_a_static_body() {
        assert!(matches!(
            RigidBody::dynamic(ColliderShape::Sphere { radius: 1.0 }, Vec3::ZERO, -1.0),
            Err(PhysicsError::InvalidBody)
        ));
    }

    #[test]
    fn invalid_public_world_state_is_rejected_before_integration() {
        let mut world = PhysicsWorld::new();
        world.gravity.x = f32::NAN;
        assert_eq!(world.step(1.0 / 60.0), Err(PhysicsError::InvalidBody));
        world.gravity = Vec3::ZERO;
        world.fixed_delta_seconds = f32::MAX;
        assert_eq!(world.step(1.0 / 60.0), Err(PhysicsError::InvalidStep));
    }

    #[test]
    fn body_coefficients_and_forces_are_validated() {
        let mut body =
            RigidBody::dynamic(ColliderShape::Sphere { radius: 1.0 }, Vec3::ZERO, 1.0).unwrap();
        body.friction = f32::NAN;
        let mut world = PhysicsWorld::new();
        assert_eq!(world.add_body(body), Err(PhysicsError::InvalidBody));
        let mut body =
            RigidBody::dynamic(ColliderShape::Sphere { radius: 1.0 }, Vec3::ZERO, 1.0).unwrap();
        assert_eq!(
            body.apply_force(Vec3::new(f32::NAN, 0.0, 0.0)),
            Err(PhysicsError::InvalidBody)
        );
        body.apply_force(Vec3::new(0.0, 1.0, 0.0)).unwrap();
        assert_eq!(body.force, Vec3::Y);
    }

    #[test]
    fn sphere_inside_box_is_corrected_toward_a_nearest_face() {
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        world.solver_iterations = 8;
        world
            .add_body(
                RigidBody::fixed(
                    ColliderShape::Box {
                        half_extents: Vec3::new(2.0, 2.0, 2.0),
                    },
                    Vec3::ZERO,
                )
                .unwrap(),
            )
            .unwrap();
        world
            .add_body(
                RigidBody::dynamic(ColliderShape::Sphere { radius: 0.5 }, Vec3::ZERO, 1.0).unwrap(),
            )
            .unwrap();
        world.step(1.0 / 60.0).unwrap();
        assert!(world.bodies()[1].position.length() > 0.0);
    }

    #[test]
    fn overflow_during_integration_rolls_back_the_entire_step() {
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        world.fixed_delta_seconds = MAX_FIXED_STEP_SECONDS;
        let mut body = RigidBody::dynamic(
            ColliderShape::Sphere { radius: 1.0 },
            Vec3::new(f32::MAX, 0.0, 0.0),
            1.0,
        )
        .unwrap();
        body.velocity = Vec3::new(f32::MAX, 0.0, 0.0);
        world.add_body(body).unwrap();
        let before = world.bodies().to_vec();
        assert_eq!(
            world.step(MAX_FIXED_STEP_SECONDS),
            Err(PhysicsError::InvalidBody)
        );
        assert_eq!(world.bodies(), before.as_slice());
    }
}
