//! Particle systems — the capability every engine in the comparison tables
//! carries (Unity's Particle System, Unreal's Niagara, Godot's particles,
//! TouchDesigner's POPs, Three.js' points) and the one a UI framework tends
//! to fake with a loop that mutates a `Vec`.
//!
//! # The one rule this module keeps
//!
//! **A particle field is a function of time, not a machine you step.** No
//! `update(dt)`, no internal clock, no order-of-sampling effects: asking
//! `sample(t)` for the state of the field at second three gives the same
//! answer whether you asked it first, last, or twice. That is the same
//! contract the rest of this crate makes ([`crate::simulation`], the
//! keyframes, the LFO), and it is what makes a field *testable* — a test
//! hands it a `Duration` and asserts on positions, exactly like a spring.
//!
//! # How a stateful thing is made a pure one
//!
//! The trick is that the physics of a particle after it is born is
//! **closed-form**: constant velocity plus constant gravity integrates to
//! `origin + v₀·age + ½·g·age²`. So the state of particle *k* at time *t*
//! needs only two ingredients — the random numbers that decided its birth
//! (spawn time, direction, speed, lifetime, jitter), which are a deterministic
//! hash of `(seed, k)`, and the arithmetic above. Nothing has to remember
//! anything between frames, and a dropped frame costs the frames it dropped
//! and nothing else.
//!
//! The capacity question answers itself the same way: if `rate` particles are
//! born per second and none outlives `lifetime_max`, then no more than
//! `rate × lifetime_max` are ever alive at once, and the alive set at time
//! *t* is the contiguous window of birth indices `[newest − capacity + 1,
//! newest]`. A field therefore costs nothing to own and nothing to idle.
//!
//! ```
//! use std::time::Duration;
//! use vieww_animation::particles::ParticleField;
//! use vieww_foundation::Offset;
//!
//! let fountain = ParticleField::new(120.0, 1.5) // 120/s, living 1.5 s
//!     .origin(Offset::new(0.0, 0.0))
//!     .direction(-std::f32::consts::FRAC_PI_2) // up
//!     .spread(0.4)
//!     .speed(180.0, 260.0)
//!     .gravity(Offset::new(0.0, 300.0));
//!
//! let mid = fountain.sample(Duration::from_secs_f32(0.75));
//!
//! // Alive and bounded: at most rate × lifetime, always.
//! assert!(!mid.is_empty());
//! assert!(mid.len() <= 120 * 2, "{} exceeds capacity", mid.len());
//!
//! // Deterministic: the same ask, the same particles, in the same order.
//! assert_eq!(mid, fountain.sample(Duration::from_secs_f32(0.75)));
//! ```

use vieww_foundation::{Color, Offset};


/// The deterministic hash behind every per-particle decision.
///
/// splitmix64 again, deliberately the same primitive the noise module uses:
/// one shuffler, understood once. The inputs are the field's seed and the
/// particle's *birth index* — not its slot in a ring buffer — so recycling
/// slots cannot change a particle's identity, and the same seed is the same
/// field forever.
fn birth_rng(seed: u64, index: u64) -> u64 {
    let mut state = seed ^ index.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    // Two mixing rounds: the first folds the index in, the second avalanches.
    // One round would leave neighbouring indices correlated in their low
    // bits, which is exactly where the speed and angle are read from.
    state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A unit interval from the stream, in `0..1`.
#[allow(clippy::cast_precision_loss)] // 24-bit mantissa: exact in f32
fn unit(stream: &mut u64) -> f32 {
    *stream = (*stream).wrapping_mul(0x2545_F491_4F6C_DD1D).wrapping_add(1);
    // 24 bits of mantissa: enough resolution for a spread, and immune to the
    // low-bit pathologies of `% f32`.
    (((*stream >> 40) & 0xFF_FFFF) as f32) / 16_777_215.0
}

/// One particle's state at a moment — everything a painter needs and nothing
/// it does not.
///
/// `age01` is the normalised age in `0..1` (birth to death), which is the
/// natural parameter for everything that varies across a life: size, colour,
/// alpha. Handing the painter the raw age would push the field's lifetime
/// arithmetic into every painter that ever draws one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Particle {
    /// Where the particle is, in the field's own coordinate space.
    pub position: Offset,
    /// Birth to death, in `0..=1`.
    pub age01: f32,
    /// Radius in logical pixels at this moment of its life.
    pub size: f32,
    /// The colour at this moment of its life — the lerp of the field's two
    /// endpoint colours, alpha included.
    pub color: Color,
}

/// A spawn rate and a lifetime: the two numbers that size everything else.
///
/// These are constructor arguments because a field without them has no
/// capacity, and a builder that can build "no field" is a runtime branch in
/// every sample.
#[derive(Debug, Clone)]
pub struct ParticleField {
    /// Births per second.
    rate: f32,
    /// The lifetime range — a span, not one number, because a field where
    /// every particle dies on the same tick pulses visibly. `(min, max)`.
    lifetime: (f32, f32),
    /// Where every particle is born.
    origin: Offset,
    /// The centre of the launch cone, in radians. Screen convention: zero is
    /// +x, y grows down, so "up" is `-π/2`.
    direction: f32,
    /// Half the width of the launch cone, in radians. Zero is a jet;
    /// `π` is a full circle.
    spread: f32,
    /// The speed range at birth, units per second. `(min, max)`.
    speed: (f32, f32),
    /// Constant acceleration, in units per second². The closed form in
    /// [`sample`](Self::sample) is why this must be constant — an
    /// acceleration that varies with time turns the position into an
    /// integral this module would have to integrate instead of evaluate.
    gravity: Offset,
    /// Radius at birth, and at death, lerped across the life.
    size: (f32, f32),
    /// Colour at birth and at death, lerped across the life (alpha included —
    /// fading out *is* a colour change here, because that is what a painter
    /// does with it).
    color: (Color, Color),
    /// Two fields with the same seed are the same field.
    seed: u64,
}

impl ParticleField {
    /// A field of `rate` births per second whose particles live
    /// `lifetime` seconds (the span is `±10%` of it — enough stagger that a
    /// constant-rate field does not pulse, little enough that the rate means
    /// what it says).
    ///
    /// Everything else has a default: a jet straight up, a modest speed, one
    /// gravity, a dot that shrinks and fades. The constructor is the two
    /// numbers that change a field's *cost*; the builder is everything that
    /// changes its *look*.
    pub fn new(rate: f32, lifetime: f32) -> Self {
        let stagger = lifetime * 0.1;
        Self {
            rate,
            lifetime: (lifetime - stagger, lifetime + stagger),
            origin: Offset::new(0.0, 0.0),
            direction: -std::f32::consts::FRAC_PI_2,
            spread: 0.2,
            speed: (120.0, 180.0),
            gravity: Offset::new(0.0, 200.0),
            size: (3.0, 0.5),
            color: (Color::rgba(255, 140, 0, 255), Color::rgba(255, 0, 0, 0)),
            seed: 0,
        }
    }

    /// Where particles are born.
    #[must_use]
    pub const fn origin(mut self, origin: Offset) -> Self {
        self.origin = origin;
        self
    }

    /// The launch direction, in radians (screen convention: `-π/2` is up).
    #[must_use]
    pub const fn direction(mut self, direction: f32) -> Self {
        self.direction = direction;
        self
    }

    /// Half the launch cone's width, in radians.
    #[must_use]
    pub const fn spread(mut self, spread: f32) -> Self {
        self.spread = spread;
        self
    }

    /// The birth speed range `(min, max)` in units per second.
    #[must_use]
    pub const fn speed(mut self, min: f32, max: f32) -> Self {
        self.speed = (min, max);
        self
    }

    /// Constant acceleration. Zero for sparks in space; a few hundred for
    /// anything that should fall.
    #[must_use]
    pub const fn gravity(mut self, gravity: Offset) -> Self {
        self.gravity = gravity;
        self
    }

    /// Radius at birth and at death.
    #[must_use]
    pub const fn size(mut self, birth: f32, death: f32) -> Self {
        self.size = (birth, death);
        self
    }

    /// Colour at birth and at death — the whole "over lifetime" ramp, alpha
    /// included, because a particle that fades without cooling (or cools
    /// without fading) is two lines of builder rather than a third concept.
    #[must_use]
    pub const fn color(mut self, birth: Color, death: Color) -> Self {
        self.color = (birth, death);
        self
    }

    /// The seed. Two fields with the same seed and parameters are the same
    /// field on every machine; this is the reproducibility knob.
    #[must_use]
    pub const fn seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// How many particles can ever be alive at once: `rate × lifetime_max`,
    /// rounded up.
    ///
    /// Worth publishing because it is the memory answer a caller is really
    /// asking for when they ask what a field costs — and because the bound is
    /// a *theorem* here (births are spaced `1/rate` apart, none outlives
    /// `lifetime_max`), not a tuned guess.
    pub fn capacity(&self) -> usize {
        (self.rate * self.lifetime.1).ceil().max(0.0) as usize
    }

    /// The alive particles at `t`, oldest first.
    ///
    /// Each is evaluated in closed form from its birth parameters — see the
    /// module docs for why that keeps a field a pure function of time. The
    /// count is the capacity bound or less, never more, and the order is
    /// stable so two samples are `==`-comparable.
    pub fn sample(&self, t: std::time::Duration) -> Vec<Particle> {
        let now = t.as_secs_f32().max(0.0);
        let alive_max = self.capacity();
        if self.rate <= 0.0 || alive_max == 0 {
            return Vec::new();
        }

        // The newest birth index at this instant: births are spaced exactly
        // `1/rate` apart, so the index born at or before `now` is the floor
        // of `now × rate`. The alive window is the `alive_max` indices
        // ending there; older ones are dead by the capacity argument and are
        // never even visited.
        let newest = (now * self.rate).floor() as i64;
        let mut out = Vec::with_capacity(alive_max);

        // Ascending birth index = oldest first, the order a painter stacking
        // additive glow wants: the oldest are at the back.
        for k in (newest - alive_max as i64 + 1)..=newest {
            if k < 0 {
                // Before the field's first birth.
                continue;
            }
            // The birth lottery for this index — the same five numbers every
            // time, on every machine, from (seed, index).
            let mut stream = birth_rng(self.seed, k as u64);
            let jitter = unit(&mut stream);
            let lifetime =
                self.lifetime.0 + unit(&mut stream) * (self.lifetime.1 - self.lifetime.0);
            let speed = self.speed.0 + unit(&mut stream) * (self.speed.1 - self.speed.0);
            let angle = self.direction + (unit(&mut stream) * 2.0 - 1.0) * self.spread;

            let spawn = (k as f64 + f64::from(jitter)) / f64::from(self.rate);
            let age = now - spawn as f32;
            if age < 0.0 {
                // Not born yet — its slot in the window is the future.
                continue;
            }
            if age > lifetime {
                // Dead; keeping it would be a painter drawing a ghost.
                continue;
            }
            let age01 = (age / lifetime).clamp(0.0, 1.0);

            // The closed form. Constant velocity, constant gravity: the one
            // ballistic world where the position is an evaluation rather
            // than an integral, and the reason this module can keep the
            // crate's contract.
            let vx = angle.cos() * speed;
            let vy = angle.sin() * speed;
            let position = Offset::new(
                self.origin.dx + vx * age + 0.5 * self.gravity.dx * age * age,
                self.origin.dy + vy * age + 0.5 * self.gravity.dy * age * age,
            );
            let size = self.size.0 + (self.size.1 - self.size.0) * age01;
            let color = self.color.0.lerp(self.color.1, age01);
            out.push(Particle {
                position,
                age01,
                size,
                color,
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const EPS: f32 = 1e-4;

    #[test]
    fn nothing_is_alive_before_the_first_birth() {
        let field = ParticleField::new(60.0, 1.0);
        assert!(field.sample(Duration::ZERO).is_empty());
        assert!(field.sample(Duration::from_secs_f32(0.001)).is_empty());
    }

    #[test]
    fn the_capacity_bound_is_respected_everywhere() {
        // The theorem of the module, checked empirically across a sweep: no
        // instant ever holds more than rate × lifetime_max. The constructor
        // staggers lifetime ±10%, so lifetime_max is 1.2 × 1.1 = 1.32 s and
        // the bound is 90 × 1.32 = 118.8 → 119, not 108.
        let field = ParticleField::new(90.0, 1.2).spread(2.5);
        assert_eq!(field.capacity(), 119);
        let mut worst = 0;
        for step in 0..600 {
            let t = Duration::from_secs_f32(step as f32 * 0.02);
            worst = worst.max(field.sample(t).len());
        }
        assert!(worst <= 119, "{worst} exceeded the capacity");
        // And it gets busy: a field that never comes near the bound is a
        // field whose lifetimes are being consumed by dead-slot iteration.
        assert!(worst > 100, "only {worst} alive at the busiest instant");
    }

    #[test]
    fn sampling_is_deterministic_and_order_stable() {
        let field = ParticleField::new(50.0, 1.0).seed(9);
        let a = field.sample(Duration::from_secs_f32(1.37));
        let b = field.sample(Duration::from_secs_f32(1.37));
        assert_eq!(a, b);
        assert!(!a.is_empty());
        // Out of order is legal: asking about the past does not change it.
        let earlier = field.sample(Duration::from_secs_f32(0.3));
        let a_again = field.sample(Duration::from_secs_f32(1.37));
        assert_eq!(a, a_again);
        assert!(earlier.len() < a.len());
    }

    #[test]
    fn the_same_seed_is_the_same_field() {
        let make = |seed: u64| ParticleField::new(50.0, 1.0).seed(seed);
        assert_eq!(
            make(4).sample(Duration::from_secs_f32(0.9)),
            make(4).sample(Duration::from_secs_f32(0.9))
        );
    }

    #[test]
    fn different_seeds_are_different_fields() {
        let a = ParticleField::new(50.0, 1.0).seed(1);
        let b = ParticleField::new(50.0, 1.0).seed(2);
        let sa = a.sample(Duration::from_secs_f32(0.9));
        let sb = b.sample(Duration::from_secs_f32(0.9));
        assert_eq!(sa.len(), sb.len());
        assert_ne!(sa, sb, "two seeds gave the identical field");
    }

    #[test]
    fn gravity_is_integrated_exactly() {
        // The closed form, pinned: particles launched straight up at a known
        // speed, after a known time, at the positions the algebra says. The
        // check is against each particle's *own* birth parameters, read out
        // through the same stream, so the test is about the ballistic
        // arithmetic and not about a lucky seed.
        let field = ParticleField::new(1.0, 10.0)
            .direction(-std::f32::consts::FRAC_PI_2)
            .spread(0.0)
            .speed(100.0, 100.0)
            .gravity(Offset::new(0.0, 50.0));
        let t = Duration::from_secs_f32(2.0);
        let alive = field.sample(t);
        // Rate 1, so births k = 0 and k = 1 are both two seconds old or
        // less and both alive; k = 2 is not born yet.
        assert_eq!(alive.len(), 2, "two births, two lives: {alive:?}");
        for (slot, k) in [0u64, 1].iter().enumerate() {
            let p = alive[slot];
            let mut stream = birth_rng(0, *k);
            let jitter = unit(&mut stream);
            // Rate 1: birth k spawns at k + jitter seconds.
            let age = 2.0 - (*k as f32 + jitter);
            let want_y = -100.0 * age + 0.5 * 50.0 * age * age;
            assert!(
                (p.position.dy - want_y).abs() < EPS,
                "k={k}: y {} vs closed form {want_y}",
                p.position.dy
            );
            assert!(p.position.dx.abs() < EPS, "straight up means no x");
        }
    }

    #[test]
    fn particles_age_then_die() {
        let field = ParticleField::new(4.0, 0.5); // ±10% → 0.45..0.55 s lives
        // The first particle (k=0, spawn ≤ 0.25 s) is alive mid-life and
        // dead well after every possible lifetime.
        let mid = field.sample(Duration::from_secs_f32(0.3));
        let late = field.sample(Duration::from_secs_f32(3.0));
        assert!(!mid.is_empty());
        // At t = 3 s the alive set is particles born after 2.45 s — k=0 is
        // long dead, which is the point: nothing survives its lifetime.
        let any_ancient = late.iter().any(|p| p.age01 > 0.999 && p.position.dy.abs() > 1000.0);
        assert!(!any_ancient);
        assert!(!late.is_empty(), "the field keeps emitting");
    }

    #[test]
    fn age_and_colour_advance_together() {
        let field = ParticleField::new(2.0, 1.0)
            .color(Color::rgba(255, 255, 255, 255), Color::rgba(0, 0, 0, 0));
        let alive = field.sample(Duration::from_secs_f32(0.75));
        // The first-born (oldest-first ordering) is the most aged: its alpha
        // must be the *lowest*, because it is the farthest down its ramp.
        let oldest = &alive[0];
        let newest = &alive[alive.len() - 1];
        assert!(oldest.age01 >= newest.age01, "ordering is oldest first");
        // Alpha tracks age on the whole linear ramp: older = fainter.
        assert!(
            oldest.color.a <= newest.color.a,
            "oldest alpha {} should trail newest {}",
            oldest.color.a,
            newest.color.a
        );
    }

    #[test]
    fn sizes_shrink_across_a_life() {
        let field = ParticleField::new(2.0, 1.0).size(8.0, 1.0);
        let alive = field.sample(Duration::from_secs_f32(0.75));
        let oldest = alive[0];
        let newest = alive[alive.len() - 1];
        assert!(oldest.size < newest.size, "birth is big, death is small");
        // And never outside the authored pair.
        for p in &alive {
            assert!((1.0..=8.0).contains(&p.size), "size {} out of range", p.size);
        }
    }

    #[test]
    fn zero_rate_is_an_empty_field_not_a_crash() {
        let field = ParticleField::new(0.0, 1.0);
        assert_eq!(field.capacity(), 0);
        assert!(field.sample(Duration::from_secs_f32(5.0)).is_empty());
    }

    #[test]
    fn spread_zero_is_a_jet() {
        // No gravity either — the test is about the launch cone, and default
        // gravity would curve every trajectory whatever the spread.
        let field = ParticleField::new(30.0, 1.0)
            .spread(0.0)
            .direction(0.0)
            .gravity(Offset::new(0.0, 0.0));
        // Every particle travels exactly along +x.
        let alive = field.sample(Duration::from_secs_f32(0.5));
        for p in &alive {
            assert!(p.position.dy.abs() < 1.0, "a jet has no sideways: {p:?}");
        }
    }
}
