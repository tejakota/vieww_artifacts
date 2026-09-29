//! The physics layer — 2D bodies, collisions, and an impulse solver.
//!
//! # What this is, and what it is for
//!
//! The framework's comparison tables list "no physics" as a gap against
//! Unity and Godot, and the honest answer to *that* gap is not a game
//! engine: it is the physics a **UI** wants — the spring that a scroll
//! already has, taken further. A card that can be thrown and slides to
//! rest against a wall. A drag that lets a panel bump others aside. A
//! stack of notification chips that settle. Two dimensions, circles and
//! boxes, gravity as a default rather than a world-building system.
//!
//! What is here: [`Body`] (a shape, a position, a velocity, a mass),
//! [`World`] (the list, gravity, one [`step`](World::step)), collision
//! detection for circle/circle, circle/box and box/box, and resolution by
//! impulses plus positional correction — the classical formulation, from
//! the classical papers, in about four hundred lines with no dependencies.
//!
//! What is not: joints, continuous collision (a fast body can tunnel
//! through a thin one — the caller's fixed `dt` is the guard), rotation
//! (bodies are translated only; a tumbling box is a rigid-body rotation
//! system, and torque brings orientation, inertia tensors and a whole
//! contact manifold model with it), and 3D. Each is a real, named gap
//! rather than an oversight, and each is the moment this crate stops
//! being a UI's physics and starts being an engine.
//!
//! # The one rule
//!
//! **`World::step(dt)` is the only thing that moves time, and it is a pure
//! function of its arguments.** Same bodies, same `dt`, same result, every
//! time — the determinism the animation and video crates draw from the
//! same well. The caller owns the clock (fixed `dt`, accumulated to the
//! frame rate, the classic "fixed timestep with an accumulator" pattern),
//! because a physics step interpolated to the frame's exact delta is a
//! physics step whose outcome depends on when the frames happened to
//! land, and a replay of the same gestures would diverge.
//!
//! ```
//! use vieww_foundation::Offset;
//! use vieww_physics::{Body, Shape, World};
//!
//! // A ball, dropped onto a floor.
//! let mut world = World::with_gravity(Offset::new(0.0, 600.0));
//! let ball = world.add_body(Body::dynamic(Offset::new(0.0, 0.0), Shape::circle(8.0), 1.0));
//! let floor = world.add_body(Body::fixed(Offset::new(0.0, 100.0), Shape::box_shape(200.0, 4.0)));
//!
//! // A quarter second of fixed steps.
//! for _ in 0..15 {
//!     world.step(1.0 / 60.0);
//! }
//!
//! let rest = world.body(ball).position;
//! assert!(
//!     rest.dy < 96.0,
//!     "the ball came to rest above the floor, not inside it: {rest:?}"
//! );
//! let _ = floor;
//! ```

use vieww_foundation::Offset;

pub use vieww_foundation as foundation;

/// A two-dimensional vector — [`Offset`] under the name physics reads
/// better in.
///
/// The reuse is deliberate: a physics position, velocity or normal is the
/// same pair the rest of the framework moves geometry in, and a `Vec2` of
/// this crate's own would fork the arithmetic of "add two vectors" into
/// two implementations to keep in agreement forever.
pub type Vec2 = Offset;

/// What a body is shaped like.
///
/// Circles and boxes, because those are the two shapes a UI's physics
/// asks for — a chip is a box, a cursor-follower is a circle — and because
/// every further shape (a capsule, a polygon) brings its own narrowphase
/// and its own edge cases, each worth adding only when something needs it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    /// A circle of `radius`, centred on the body's position.
    Circle { radius: f32 },
    /// An axis-aligned box, given by its *half* extents.
    ///
    /// Half, because every collision formula wants the centre-to-edge
    /// distance and writing `width / 2.0` at every one of them is how a
    /// factor-of-two bug gets into a solver.
    Box { half_width: f32, half_height: f32 },
}

impl Shape {
    /// A circle, the common spelling.
    #[must_use]
    pub const fn circle(radius: f32) -> Self {
        Self::Circle { radius }
    }

    /// A box, the common spelling — full extents, halved here once.
    #[must_use]
    pub const fn box_shape(width: f32, height: f32) -> Self {
        Self::Box {
            half_width: width / 2.0,
            half_height: height / 2.0,
        }
    }

    /// The body's bounding half-extents: the circle's radius in both axes,
    /// the box's own. What the broadphase needs.
    #[must_use]
    pub const fn bounds(&self) -> (f32, f32) {
        match *self {
            Self::Circle { radius } => (radius, radius),
            Self::Box {
                half_width,
                half_height,
            } => (half_width, half_height),
        }
    }
}

/// One thing that moves (or does not) in a [`World`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Body {
    /// Where the body's centre is.
    pub position: Vec2,
    /// How fast, per second, in world axes.
    pub velocity: Vec2,
    /// The shape, at the position.
    pub shape: Shape,
    /// Mass in abstract units. Zero for a fixed body — see
    /// [`inv_mass`](Self::inv_mass) for why that is the whole story.
    pub mass: f32,
    /// Bounciness in `0..=1`: 0 lands dead, 1 would bounce forever. The
    /// 0.3 default is "rubber on wood", the value a UI wants — full
    /// restitution is a physics demonstration, not an interface.
    pub restitution: f32,
}

impl Body {
    /// A body that moves, of `mass` kilograms-in-the-abstract.
    #[must_use]
    pub const fn dynamic(position: Vec2, shape: Shape, mass: f32) -> Self {
        Self {
            position,
            velocity: Offset::new(0.0, 0.0),
            shape,
            mass,
            restitution: 0.3,
        }
    }

    /// A body that never moves: a floor, a wall.
    ///
    /// Mass zero rather than `f32::INFINITY`, because the inverse mass —
    /// the number every impulse formula actually uses — is then zero, and
    /// zero is exact, total, and free of the `inf × 0 = NaN` that an
    /// infinite mass drags into every product it touches.
    #[must_use]
    pub const fn fixed(position: Vec2, shape: Shape) -> Self {
        Self {
            position,
            velocity: Offset::new(0.0, 0.0),
            shape,
            mass: 0.0,
            restitution: 0.3,
        }
    }

    /// Set the restitution, `0..=1`.
    #[must_use]
    pub const fn restitution(mut self, restitution: f32) -> Self {
        self.restitution = restitution;
        self
    }

    /// The inverse mass: `0.0` for fixed bodies, `1/mass` for moving ones.
    ///
    /// The one number the solver uses — a fixed body's inverse mass of
    /// zero makes every impulse term it appears in vanish, which is the
    /// algebra of "unmovable" without a single branch.
    #[must_use]
    pub fn inv_mass(&self) -> f32 {
        if self.mass > 0.0 {
            self.mass.recip()
        } else {
            0.0
        }
    }

    /// Whether this body can move at all.
    #[must_use]
    pub fn is_dynamic(&self) -> bool {
        self.mass > 0.0
    }
}

/// One collision, as the solver needs it.
///
/// The convention throughout: `normal` points **from `a` to `b`**, and
/// `depth` is how far `a` overlaps `b` along it. Every formula that uses
/// a contact states which side it stands on, because a normal with an
/// unwritten direction is the classic contact-bug generator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Contact {
    /// The first body's index in the world.
    pub a: usize,
    /// The second body's index in the world.
    pub b: usize,
    /// From `a` to `b`, unit length.
    pub normal: Vec2,
    /// How far they overlap along the normal.
    pub depth: f32,
}

/// Two shapes, one collision question.
///
/// # Panics
///
/// Nothing — non-overlap is `None`, and the caller's "did they touch?" is
/// the whole answer. Panicking here would make a contact-heavy frame a
/// minefield, and there is no input whose correct response is a crash.
#[must_use]
pub fn collide(a: &Body, b: &Body) -> Option<Contact> {
    let (an, bn) = (a.shape, b.shape);
    match (an, bn) {
        (Shape::Circle { .. }, Shape::Circle { .. }) => circle_circle(a, b),
        // Both mixed cases are computed circle-first and, when the circle
        // was the second argument, the normal is negated — so the
        // a-to-b convention holds for every pair, and every formula that
        // depends on it can be written once.
        (Shape::Circle { .. }, Shape::Box { .. }) => circle_box(a, b),
        (Shape::Box { .. }, Shape::Circle { .. }) => {
            circle_box(b, a).map(negated)
        }
        (Shape::Box { .. }, Shape::Box { .. }) => box_box(a, b),
    }
}

/// A contact with its normal reversed — the box-first spelling of a
/// circle-first result.
fn negated(contact: Contact) -> Contact {
    Contact {
        normal: Offset::new(-contact.normal.dx, -contact.normal.dy),
        ..contact
    }
}

/// Circle–circle: the textbook one, and the reason the contact convention
/// is stated once at [`Contact`] and used everywhere.
fn circle_circle(a: &Body, b: &Body) -> Option<Contact> {
    let Shape::Circle { radius: ra } = a.shape else {
        return None;
    };
    let Shape::Circle { radius: rb } = b.shape else {
        return None;
    };
    let between = Offset::new(b.position.dx - a.position.dx, b.position.dy - a.position.dy);
    let distance = (between.dx * between.dx + between.dy * between.dy).sqrt();
    let depth = ra + rb - distance;
    if depth <= 0.0 {
        return None;
    }
    // Perfectly concentric circles have no direction; the solver needs
    // *one*, and "straight up" is the choice that never launches a body
    // horizontally off a stack it was dropped dead-centre on.
    let normal = if distance > f32::EPSILON {
        Offset::new(between.dx / distance, between.dy / distance)
    } else {
        Offset::new(0.0, -1.0)
    };
    Some(Contact {
        a: 0,
        b: 0,
        normal,
        depth,
    })
}

/// Circle–box, computed as circle-vs-box with the *circle first*; the
/// caller flips the normal when the box was first. Working in one order
/// and flipping is one narrowphase to test rather than two.
///
/// The returned contact's normal points from the circle to the box.
fn circle_box(circle: &Body, a_box: &Body) -> Option<Contact> {
    let Shape::Circle { radius } = circle.shape else {
        return None;
    };
    let Shape::Box {
        half_width,
        half_height,
    } = a_box.shape
    else {
        return None;
    };

    // The circle's centre clamped to the box: the box's closest point.
    let dx = circle.position.dx - a_box.position.dx;
    let dy = circle.position.dy - a_box.position.dy;
    let closest = Offset::new(
        dx.clamp(-half_width, half_width),
        dy.clamp(-half_height, half_height),
    );

    // Centre-to-closest distance, and the overlap against the radius.
    let offset = Offset::new(dx - closest.dx, dy - closest.dy);
    let distance_sq = offset.dx * offset.dx + offset.dy * offset.dy;
    let depth = radius - distance_sq.sqrt();
    if depth <= 0.0 {
        return None;
    }

    // Outside the box: the closest point is on the surface, and the
    // normal (circle to box — the a-to-b convention) is
    // *toward* the closest point, the negation of the centre-to-closest
    // offset.
    if distance_sq > f32::EPSILON {
        let distance = distance_sq.sqrt();
        return Some(Contact {
            a: 0,
            b: 0,
            normal: Offset::new(-offset.dx / distance, -offset.dy / distance),
            depth,
        });
    }

    // Inside the box (the clamp landed on the centre itself): the exit is
    // along the shallower axis — the face the circle is least far through,
    // and the only choice that does not fire a body through a wall it had
    // barely entered. The normal points *opposite* the exit (circle toward
    // the box's centre), which is what the convention asks for and what
    // makes the correction push the circle out of that face.
    let overlap_x = half_width - dx.abs();
    let overlap_y = half_height - dy.abs();
    if overlap_x < overlap_y {
        Some(Contact {
            a: 0,
            b: 0,
            normal: Offset::new(-dx.signum(), 0.0),
            // Out past the face, plus the radius: the full push-out.
            depth: overlap_x + radius,
        })
    } else {
        Some(Contact {
            a: 0,
            b: 0,
            normal: Offset::new(0.0, -dy.signum()),
            depth: overlap_y + radius,
        })
    }
}

/// Box–box: the separating-axis theorem on the two axes an
/// axis-aligned pair has, which is the whole theorem here — no rotated
/// boxes exist in this crate, so the "any of infinitely many axes" case
/// does not either.
fn box_box(a: &Body, b: &Body) -> Option<Contact> {
    let Shape::Box {
        half_width: aw,
        half_height: ah,
    } = a.shape
    else {
        return None;
    };
    let Shape::Box {
        half_width: bw,
        half_height: bh,
    } = b.shape
    else {
        return None;
    };

    let dx = b.position.dx - a.position.dx;
    let dy = b.position.dy - a.position.dy;
    let overlap_x = aw + bw - dx.abs();
    let overlap_y = ah + bh - dy.abs();
    if overlap_x <= 0.0 || overlap_y <= 0.0 {
        return None;
    }

    // Push out along the shallower overlap: the axis they are least
    // interpenetrated on is the axis they were arriving along.
    let (normal, depth) = if overlap_x < overlap_y {
        (Offset::new(dx.signum(), 0.0), overlap_x)
    } else {
        (Offset::new(0.0, dy.signum()), overlap_y)
    };
    Some(Contact {
        a: 0,
        b: 0,
        normal,
        depth,
    })
}

/// The world: bodies, gravity, one step at a time.
#[derive(Debug, Clone)]
pub struct World {
    bodies: Vec<Body>,
    /// Constant acceleration applied to every dynamic body, per second.
    ///
    /// A default of zero — "top-down" physics, chips on a desk — because
    /// gravity is a *choice* about the simulation's orientation, and
    /// `World::with_gravity` is where a falling world says so.
    pub gravity: Vec2,
}

impl Default for World {
    fn default() -> Self {
        Self {
            bodies: Vec::new(),
            gravity: Offset::new(0.0, 0.0),
        }
    }
}

impl World {
    /// A world with no gravity: the top-down one.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            bodies: Vec::new(),
            gravity: Offset::new(0.0, 0.0),
        }
    }

    /// A world where everything falls, at `gravity` per second per second.
    #[must_use]
    pub const fn with_gravity(gravity: Vec2) -> Self {
        Self {
            bodies: Vec::new(),
            gravity,
        }
    }

    /// Add a body, returning its index — the handle every later question
    /// about it takes.
    pub fn add_body(&mut self, body: Body) -> usize {
        self.bodies.push(body);
        self.bodies.len() - 1
    }

    /// A body, by index.
    #[must_use]
    pub fn body(&self, index: usize) -> &Body {
        &self.bodies[index]
    }

    /// A body, mutably, by index — for a throw, a fling, a teleport.
    pub fn body_mut(&mut self, index: usize) -> &mut Body {
        &mut self.bodies[index]
    }

    /// Every body, in index order.
    #[must_use]
    pub fn bodies(&self) -> &[Body] {
        &self.bodies
    }

    /// How many bodies the world holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bodies.len()
    }

    /// Whether the world is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bodies.is_empty()
    }

    /// Advance the world by `dt` seconds.
    ///
    /// Integration, then every pair's collision, then every contact's
    /// resolution — in that order, because resolving on the pre-integration
    /// positions is resolving a world that no longer exists, and because
    /// one resolution pass over all contacts (rather than
    /// resolve-and-recheck) is the classical formulation this crate
    /// deliberately sticks to: it is stable at UI scales, and its failure
    /// mode — a deep stack settling over a few frames — reads as *settle*,
    /// which is what a stack is supposed to do.
    ///
    /// A `dt` of zero is a legal no-op, so a caller accumulating fixed
    /// steps against a frame budget needs no special case for the frame
    /// that had nothing left to spend.
    pub fn step(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }

        // Integrate: gravity for the dynamic, nothing for the fixed, and
        // position from velocity — semi-implicit Euler, the integration
        // that uses the *new* velocity, which damps the oscillation an
        // explicit Euler shows on a bouncing contact.
        for body in &mut self.bodies {
            if body.is_dynamic() {
                body.velocity = Offset::new(
                    body.velocity.dx + self.gravity.dx * dt,
                    body.velocity.dy + self.gravity.dy * dt,
                );
            }
            body.position = Offset::new(
                body.position.dx + body.velocity.dx * dt,
                body.position.dy + body.velocity.dy * dt,
            );
        }

        // Broadphase: the naive all-pairs walk with an AABB reject. An
        // all-pairs narrowphase is O(n²) and a UI's n is tens; the
        // sweep-and-prune that a game's thousands want is a named, known
        // upgrade that would make this loop's worst case irrelevant — and
        // is not this loop's average case today.
        let mut contacts = Vec::new();
        for a in 0..self.bodies.len() {
            for b in (a + 1)..self.bodies.len() {
                let (one, two) = (&self.bodies[a], &self.bodies[b]);
                // The AABB reject: bounds sums, per axis. Cheap enough to
                // run before any shape arithmetic at all.
                let (aw, ah) = one.shape.bounds();
                let (bw, bh) = two.shape.bounds();
                let (dx, dy) = (
                    (two.position.dx - one.position.dx).abs(),
                    (two.position.dy - one.position.dy).abs(),
                );
                if dx > aw + bw || dy > ah + bh {
                    continue;
                }
                if let Some(mut contact) = collide(one, two) {
                    contact.a = a;
                    contact.b = b;
                    contacts.push(contact);
                }
            }
        }

        // Resolve: impulses for velocity, correction for position.
        for contact in &contacts {
            self.resolve_impulse(contact);
        }
        for contact in &contacts {
            self.correct_positions(contact);
        }
    }

    /// Apply one contact's impulse: the velocities change along the
    /// normal, by the classical impulse magnitude, split by inverse mass.
    fn resolve_impulse(&mut self, contact: &Contact) {
        let (a, b) = (self.bodies[contact.a], self.bodies[contact.b]);
        let inv_a = a.inv_mass();
        let inv_b = b.inv_mass();
        let inv_total = inv_a + inv_b;
        if inv_total <= 0.0 {
            return; // Two fixed bodies "colliding" is scenery, not physics.
        }

        // The bodies' closing speed along the normal. Negative (separating)
        // contacts are skipped: an impulse that accelerates an
        // already-separating pair is the jitter a resting stack shows when
        // the solver cannot tell "leaving" from "arriving".
        let relative = Offset::new(
            b.velocity.dx - a.velocity.dx,
            b.velocity.dy - a.velocity.dy,
        );
        let closing = relative.dx * contact.normal.dx + relative.dy * contact.normal.dy;
        if closing > 0.0 {
            return;
        }

        // Impulse magnitude with restitution, split by inverse mass. The
        // `min(restitution, 1.0)` is the standard guard: a caller-set 1.4
        // is a *gain*, and each bounce would arrive faster than it left —
        // a simulation that explodes politely, one frame at a time.
        let restitution = a.restitution.min(b.restitution).min(1.0);
        let magnitude = -(1.0 + restitution) * closing / inv_total;

        let impulse = Offset::new(
            contact.normal.dx * magnitude,
            contact.normal.dy * magnitude,
        );
        self.bodies[contact.a].velocity = Offset::new(
            a.velocity.dx - impulse.dx * inv_a,
            a.velocity.dy - impulse.dy * inv_a,
        );
        self.bodies[contact.b].velocity = Offset::new(
            b.velocity.dx + impulse.dx * inv_b,
            b.velocity.dy + impulse.dy * inv_b,
        );
    }

    /// Push two overlapping bodies apart, along the contact normal, by a
    /// fraction of the depth — the Baumgarte positional correction.
    ///
    /// The fraction is 0.8 rather than 1.0 on purpose: correcting the whole
    /// depth at once is what makes a resting stack shiver (each correction
    /// overshoots the other body's next overlap), and 80% per contact per
    /// step converges fast without the overshoot.
    fn correct_positions(&mut self, contact: &Contact) {
        const CORRECTION: f32 = 0.8;
        let (a, b) = (self.bodies[contact.a], self.bodies[contact.b]);
        let inv_a = a.inv_mass();
        let inv_b = b.inv_mass();
        let inv_total = inv_a + inv_b;
        if inv_total <= 0.0 {
            return;
        }
        let move_by = contact.depth * CORRECTION / inv_total;
        let shift = Offset::new(
            contact.normal.dx * move_by,
            contact.normal.dy * move_by,
        );
        self.bodies[contact.a].position = Offset::new(
            a.position.dx - shift.dx * inv_a,
            a.position.dy - shift.dy * inv_a,
        );
        self.bodies[contact.b].position = Offset::new(
            b.position.dx + shift.dx * inv_b,
            b.position.dy + shift.dy * inv_b,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steps(world: &mut World, dt: f32, count: usize) {
        for _ in 0..count {
            world.step(dt);
        }
    }

    #[test]
    fn gravity_accelerates_a_dynamic_body() {
        let mut world = World::with_gravity(Offset::new(0.0, 10.0));
        let ball = world.add_body(Body::dynamic(Offset::new(0.0, 0.0), Shape::circle(1.0), 1.0));

        world.step(1.0);
        assert_eq!(world.body(ball).velocity.dy, 10.0, "one second of g");
        assert_eq!(world.body(ball).position.dy, 10.0, "semi-implicit: new velocity, then position");

        world.step(1.0);
        assert_eq!(world.body(ball).velocity.dy, 20.0);
        assert_eq!(world.body(ball).position.dy, 30.0);
    }

    #[test]
    fn a_fixed_body_never_moves() {
        let mut world = World::with_gravity(Offset::new(0.0, 100.0));
        let floor = world.add_body(Body::fixed(Offset::new(0.0, 50.0), Shape::box_shape(100.0, 4.0)));
        let ball = world.add_body(Body::dynamic(Offset::new(0.0, 0.0), Shape::circle(2.0), 1.0));

        steps(&mut world, 1.0 / 60.0, 60);
        assert_eq!(world.body(floor).position, Offset::new(0.0, 50.0));
        assert!(world.body(floor).velocity.dy.abs() < f32::EPSILON);
        assert!(world.body(ball).position.dy > 40.0, "the ball fell");
    }

    #[test]
    fn gravity_is_opt_in() {
        let mut world = World::new();
        let puck = world.add_body(Body::dynamic(Offset::new(0.0, 0.0), Shape::circle(1.0), 1.0));

        world.step(1.0);
        assert_eq!(world.body(puck).position, Offset::new(0.0, 0.0), "top-down: nothing falls");
    }

    #[test]
    fn a_ball_comes_to_rest_above_a_floor() {
        let mut world = World::with_gravity(Offset::new(0.0, 600.0));
        let ball = world.add_body(Body::dynamic(Offset::new(0.0, 0.0), Shape::circle(8.0), 1.0));
        world.add_body(Body::fixed(Offset::new(0.0, 100.0), Shape::box_shape(200.0, 4.0)));

        // A second of settling.
        steps(&mut world, 1.0 / 60.0, 60);

        let rest = world.body(ball).position;
        // The floor's top edge is at 98; a resting ball's centre sits at
        // 90 (radius 8) — within a small tolerance for the correction's
        // convergence.
        assert!(
            (rest.dy - 90.0).abs() < 1.5,
            "resting on the surface, got {rest:?}"
        );
        assert!(
            world.body(ball).velocity.dy.abs() < 10.0,
            "and not still accelerating: {:?}",
            world.body(ball).velocity
        );
    }

    #[test]
    fn restitution_controls_the_bounce() {
        // Dead vs lively, on floors that agree with them: the coefficient a
        // pair uses is the *minimum* of the two, so a lively ball needs a
        // lively floor.
        let make = |ball_restitution: f32, floor_restitution: f32| {
            let mut world = World::with_gravity(Offset::new(0.0, 600.0));
            let ball = world.add_body(
                Body::dynamic(Offset::new(0.0, 0.0), Shape::circle(4.0), 1.0)
                    .restitution(ball_restitution),
            );
            world.add_body(
                Body::fixed(Offset::new(0.0, 100.0), Shape::box_shape(200.0, 4.0))
                    .restitution(floor_restitution),
            );
            (world, ball)
        };

        // Two seconds: both have hit; the dead one stays down, the lively
        // one rises at some point in the run.
        let (mut dead, dead_ball) = make(0.0, 0.0);
        let (mut lively, lively_ball) = make(0.9, 0.9);
        let mut lively_rose = false;
        for _ in 0..120 {
            dead.step(1.0 / 60.0);
            lively.step(1.0 / 60.0);
            if lively.body(lively_ball).velocity.dy < -50.0 {
                lively_rose = true;
            }
            assert!(
                dead.body(dead_ball).velocity.dy >= -1.0,
                "dead never rises: {:?}",
                dead.body(dead_ball).velocity
            );
        }
        assert!(lively_rose, "the lively ball bounced back up");
    }

    #[test]
    fn two_circles_collide_head_on_and_stop_dead() {
        // Equal masses, opposite velocities, zero restitution: the textbook
        // full stop — both rebound to (almost) zero, momentum conserved at
        // zero, and the correction separates them.
        let mut world = World::new();
        let left = world.add_body(
            Body::dynamic(Offset::new(-5.0, 0.0), Shape::circle(2.0), 1.0)
                .restitution(0.0),
        );
        let right = world.add_body(
            Body::dynamic(Offset::new(5.0, 0.0), Shape::circle(2.0), 1.0)
                .restitution(0.0),
        );
        world.body_mut(left).velocity = Offset::new(10.0, 0.0);
        world.body_mut(right).velocity = Offset::new(-10.0, 0.0);

        // 0.31 s: they have just interpenetrated (10 apart, 4 of radii,
        // closing at 20/s — contact from 0.3 s, depth at 0.31).
        world.step(0.31);

        let (a, b) = (world.body(left), world.body(right));
        assert!(
            a.velocity.dx.abs() < 0.5,
            "the left body stopped: {:?}",
            a.velocity
        );
        assert!(
            b.velocity.dx.abs() < 0.5,
            "the right body stopped: {:?}",
            b.velocity
        );
        // And the correction pushed them apart to (nearly) touching — 80%
        // of the depth per step, by design, so a hair under is the
        // converged state, not a failure.
        let gap = (a.position.dx - b.position.dx).abs();
        assert!(gap >= 3.9, "separated, gap {gap}");
    }

    #[test]
    fn a_heavy_body_barely_notices_a_light_one() {
        // A barge drifting along, and a pebble flung at it: the impulse is
        // split by inverse mass, so the pebble's rebound is the barge's
        // nudge — however fast the pebble arrived.
        let mut world = World::new();
        let barge = world.add_body(
            Body::dynamic(Offset::new(-3.0, 0.0), Shape::circle(4.0), 100.0).restitution(1.0),
        );
        let pebble = world.add_body(
            Body::dynamic(Offset::new(3.0, 0.0), Shape::circle(1.0), 1.0).restitution(1.0),
        );
        world.body_mut(barge).velocity = Offset::new(2.0, 0.0);
        world.body_mut(pebble).velocity = Offset::new(-20.0, 0.0);

        // A step that reaches contact (closing at 22/s over a ~1-unit gap).
        world.step(0.1);

        let barge_after = world.body(barge).velocity.dx;
        let pebble_after = world.body(pebble).velocity.dx;
        assert!(
            (barge_after - 2.0).abs() < 0.5,
            "the barge sailed on: {barge_after}"
        );
        assert!(pebble_after > 10.0, "the pebble was sent flying: {pebble_after}");
    }

    #[test]
    fn far_apart_bodies_never_touch() {
        let a = Body::dynamic(Offset::new(0.0, 0.0), Shape::circle(1.0), 1.0);
        let b = Body::dynamic(Offset::new(100.0, 0.0), Shape::circle(1.0), 1.0);
        assert!(collide(&a, &b).is_none());
    }

    #[test]
    fn the_contact_normal_points_from_a_to_b() {
        let a = Body::dynamic(Offset::new(0.0, 0.0), Shape::circle(1.0), 1.0);
        let b = Body::dynamic(Offset::new(2.5, 0.0), Shape::circle(2.0), 1.0);
        let contact = collide(&a, &b).expect("they overlap");
        assert!(contact.normal.dx > 0.9, "from a toward b: {:?}", contact.normal);
        assert!((contact.depth - 0.5).abs() < 0.01, "3 of radii, 2.5 apart");

        // And exactly touching — depth zero — is *not* a contact.
        let touching = Body::dynamic(Offset::new(3.0, 0.0), Shape::circle(2.0), 1.0);
        assert!(collide(&a, &touching).is_none(), "touching, not overlapping");
    }

    #[test]
    fn circle_box_collides_from_inside_and_out() {
        let a_box = Body::fixed(Offset::new(0.0, 0.0), Shape::box_shape(10.0, 10.0));

        // Outside, to the right: the normal points back toward the box.
        let outside = Body::dynamic(Offset::new(7.0, 0.0), Shape::circle(3.0), 1.0);
        let contact = collide(&outside, &a_box).expect("overlapping the edge");
        // Convention: from a (the circle) to b (the box) — toward the box.
        assert!(contact.normal.dx < -0.9, "toward the box: {:?}", contact.normal);

        // From the other argument order, the normal flips with it: a-to-b
        // now points from the box at the circle.
        let contact = collide(&a_box, &outside).expect("overlapping the edge");
        assert!(contact.normal.dx > 0.9, "at the circle: {:?}", contact.normal);

        // Deep inside: pushed out along the shallower axis, and the depth
        // is the whole push — out past the face plus the radius.
        let inside = Body::dynamic(Offset::new(1.0, 2.0), Shape::circle(1.0), 1.0);
        let contact = collide(&a_box, &inside).expect("inside is a collision");
        // x overlap: 5 - 1 = 4; y overlap: 5 - 2 = 3. The shallow axis is
        // y: the circle exits the +y face, so the box-first normal (a to
        // b) points +y — at the face the exit happens through — and the
        // depth is 3 + 1 (radius) = 4.
        assert!(contact.normal.dy > 0.9, "at the exit face: {:?}", contact.normal);
        assert!((contact.depth - 4.0).abs() < 0.01, "out past the face: {}", contact.depth);
    }

    #[test]
    fn boxes_collide_on_the_shallow_axis() {
        let a = Body::dynamic(Offset::new(0.0, 0.0), Shape::box_shape(10.0, 4.0), 1.0);
        let b = Body::dynamic(Offset::new(8.0, 0.5), Shape::box_shape(10.0, 4.0), 1.0);
        let contact = collide(&a, &b).expect("overlapping");
        // x overlap: 10 - 8 = 2; y overlap: 4 - 0.5 = 3.5. The shallow one
        // is x, so the normal is ±x.
        assert!(contact.normal.dx.abs() > 0.9, "the shallow axis: {:?}", contact.normal);
        assert!((contact.depth - 2.0).abs() < 0.01, "by its overlap: {}", contact.depth);
    }

    #[test]
    fn a_step_of_zero_is_a_no_op() {
        let mut world = World::with_gravity(Offset::new(0.0, 100.0));
        let ball = world.add_body(Body::dynamic(Offset::new(0.0, 0.0), Shape::circle(1.0), 1.0));
        world.step(0.0);
        assert_eq!(world.body(ball).position, Offset::new(0.0, 0.0));
        assert_eq!(world.body(ball).velocity, Offset::new(0.0, 0.0));
    }

    #[test]
    fn bodies_are_indexed_and_counted() {
        let mut world = World::new();
        assert!(world.is_empty());
        let a = world.add_body(Body::fixed(Offset::new(0.0, 0.0), Shape::circle(1.0)));
        let b = world.add_body(Body::fixed(Offset::new(5.0, 0.0), Shape::circle(1.0)));
        assert_eq!(world.len(), 2);
        assert_eq!(world.body(a).position.dx, 0.0);
        assert_eq!(world.body(b).position.dx, 5.0);

        world.body_mut(a).position = Offset::new(1.0, 0.0);
        assert_eq!(world.body(a).position.dx, 1.0);
    }
}
