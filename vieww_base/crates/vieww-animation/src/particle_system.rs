//! Simulated particles: emitter shapes, forces, collisions, bursts.
//!
//! # Why a second particle module
//!
//! [`ParticleField`](crate::ParticleField) is a *closed form*: every particle
//! is a function of time, which is why it can be sampled at any instant and
//! why its only force is constant gravity. That is the right tool for a
//! fountain in a UI. It cannot express what the document's particle rows
//! mean by Unreal's Niagara, Unity's Shuriken, Godot's `GPUParticles`,
//! TouchDesigner's POPs or Blender's particle system:
//!
//! * **forces that depend on position** — turbulence from a noise field, an
//!   attractor that pulls, a vortex that swirls, drag that grows with speed;
//! * **collision** with the world — a floor to bounce on, with restitution
//!   and friction, or particles that die on contact;
//! * **emitter shapes** — a point, a circle's edge or area, a line, a box;
//! * **bursts** as well as a continuous rate;
//! * **over-life curves** for size, colour and alpha.
//!
//! Those need integration, so [`ParticleSystem`] is stateful and stepped
//! with a fixed `dt` (semi-implicit Euler). It stays deterministic: births
//! draw from a seeded generator, so the same seed and the same steps give
//! the same particles, bit for bit.

use std::f32::consts::TAU;

use vieww_foundation::{Color, Offset, Rect};

use crate::noise::Perlin;
use crate::particles::Particle;

/// Where new particles appear.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EmitterShape {
    Point(Offset),
    /// On the circle's circumference (`edge`) or anywhere inside it.
    Circle { center: Offset, radius: f32, edge: bool },
    Line(Offset, Offset),
    Rect(Rect),
}

/// Something that pushes particles around.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Force {
    /// Constant acceleration.
    Gravity(Offset),
    /// Velocity-proportional drag (1/s).
    Drag(f32),
    /// Curl-free noise push: `strength` at spatial `scale`, drifting over
    /// time at `speed`.
    Turbulence { strength: f32, scale: f32, speed: f32 },
    /// Pull toward (positive) or push from (negative) a point, falling off
    /// with distance beyond `radius`.
    Attractor { at: Offset, strength: f32, radius: f32 },
    /// Swirl around a point.
    Vortex { at: Offset, strength: f32 },
}

/// What happens when a particle crosses the floor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Collision {
    /// Bounce off `y = floor` (Y-down) keeping `restitution` of the normal
    /// speed and `1 - friction` of the tangential.
    Bounce { floor: f32, restitution: f32, friction: f32 },
    /// Die on touching the floor.
    Kill { floor: f32 },
}

#[derive(Debug, Clone, Copy)]
struct Live {
    position: Offset,
    velocity: Offset,
    age: f32,
    life: f32,
    seed: f32,
}

/// A stepped particle simulation. See the [module docs](self).
#[derive(Debug, Clone)]
pub struct ParticleSystem {
    pub shape: EmitterShape,
    /// Births per second (fractional births accumulate).
    pub rate: f32,
    pub lifetime: (f32, f32),
    /// Launch direction (radians, 0 = +x, Y-down) and half-spread.
    pub direction: f32,
    pub spread: f32,
    pub speed: (f32, f32),
    pub forces: Vec<Force>,
    pub collision: Option<Collision>,
    pub size: (f32, f32),
    pub color: (Color, Color),
    pub max: usize,
    live: Vec<Live>,
    carry: f32,
    rng: u64,
    time: f32,
    noise: Perlin,
    births: u64,
}

impl ParticleSystem {
    #[must_use]
    pub fn new(shape: EmitterShape, rate: f32, seed: u64) -> Self {
        Self {
            shape,
            rate,
            lifetime: (1.5, 2.5),
            direction: -std::f32::consts::FRAC_PI_2,
            spread: 0.4,
            speed: (80.0, 140.0),
            forces: vec![Force::Gravity(Offset::new(0.0, 120.0))],
            collision: None,
            size: (4.0, 1.0),
            color: (Color::rgb(255, 200, 80), Color::rgba(255, 60, 30, 0)),
            max: 10_000,
            live: Vec::new(),
            carry: 0.0,
            rng: seed | 1,
            time: 0.0,
            noise: Perlin::from_seed(seed),
            births: 0,
        }
    }

    #[must_use]
    pub fn force(mut self, f: Force) -> Self {
        self.forces.push(f);
        self
    }

    #[must_use]
    pub fn forces(mut self, f: Vec<Force>) -> Self {
        self.forces = f;
        self
    }

    #[must_use]
    pub const fn collision(mut self, c: Collision) -> Self {
        self.collision = Some(c);
        self
    }

    #[must_use]
    pub const fn launch(mut self, direction: f32, spread: f32, speed: (f32, f32)) -> Self {
        self.direction = direction;
        self.spread = spread;
        self.speed = speed;
        self
    }

    #[must_use]
    pub const fn lifetime(mut self, lo: f32, hi: f32) -> Self {
        self.lifetime = (lo, hi);
        self
    }

    #[must_use]
    pub const fn look(mut self, size: (f32, f32), color: (Color, Color)) -> Self {
        self.size = size;
        self.color = color;
        self
    }

    fn random(&mut self) -> f32 {
        // xorshift64*
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        let x = self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D);
        #[allow(clippy::cast_precision_loss)]
        let r = (x >> 40) as f32 / (1u64 << 24) as f32;
        r
    }

    fn spawn_point(&mut self) -> Offset {
        match self.shape {
            EmitterShape::Point(p) => p,
            EmitterShape::Circle { center, radius, edge } => {
                let a = self.random() * TAU;
                let r = if edge { radius } else { radius * self.random().sqrt() };
                Offset::new(center.dx + a.cos() * r, center.dy + a.sin() * r)
            }
            EmitterShape::Line(a, b) => {
                let t = self.random();
                Offset::new(a.dx + (b.dx - a.dx) * t, a.dy + (b.dy - a.dy) * t)
            }
            EmitterShape::Rect(r) => {
                let (u, v) = (self.random(), self.random());
                Offset::new(r.left + r.width() * u, r.top + r.height() * v)
            }
        }
    }

    fn spawn(&mut self) {
        if self.live.len() >= self.max {
            return;
        }
        let position = self.spawn_point();
        let angle = self.direction + (self.random() * 2.0 - 1.0) * self.spread;
        let speed = self.speed.0 + (self.speed.1 - self.speed.0) * self.random();
        let life = self.lifetime.0 + (self.lifetime.1 - self.lifetime.0) * self.random();
        let seed = self.random();
        self.live.push(Live {
            position,
            velocity: Offset::new(angle.cos() * speed, angle.sin() * speed),
            age: 0.0,
            life,
            seed,
        });
        self.births += 1;
    }

    /// Emit `count` particles at once.
    pub fn burst(&mut self, count: usize) {
        for _ in 0..count {
            self.spawn();
        }
    }

    /// Advance one fixed step.
    pub fn step(&mut self, dt: f32) {
        self.time += dt;
        self.carry += self.rate * dt;
        while self.carry >= 1.0 {
            self.carry -= 1.0;
            self.spawn();
        }
        let forces = self.forces.clone();
        let time = self.time;
        let noise = &self.noise;
        let collision = self.collision;
        self.live.retain_mut(|p| {
            p.age += dt;
            if p.age >= p.life {
                return false;
            }
            let mut acc = Offset::ZERO;
            for f in &forces {
                acc = acc
                    + match *f {
                        Force::Gravity(g) => g,
                        Force::Drag(k) => p.velocity.scale(-k),
                        Force::Turbulence { strength, scale, speed } => {
                            let (x, y) = (p.position.dx / scale, p.position.dy / scale);
                            let z = time * speed;
                            // Curl of a scalar potential: divergence-free swirl.
                            let e = 0.01;
                            let n = |x: f32, y: f32| noise.noise3(x, y, z);
                            let dndx = (n(x + e, y) - n(x - e, y)) / (2.0 * e);
                            let dndy = (n(x, y + e) - n(x, y - e)) / (2.0 * e);
                            Offset::new(dndy, -dndx).scale(strength)
                        }
                        Force::Attractor { at, strength, radius } => {
                            let d = at - p.position;
                            let dist = d.distance().max(1e-3);
                            let fall = (radius / dist.max(radius)).powi(2);
                            d.scale(strength * fall / dist)
                        }
                        Force::Vortex { at, strength } => {
                            let d = p.position - at;
                            let dist = d.distance().max(1.0);
                            Offset::new(-d.dy, d.dx).scale(strength / dist)
                        }
                    };
            }
            p.velocity = p.velocity + acc.scale(dt);
            p.position = p.position + p.velocity.scale(dt);
            match collision {
                Some(Collision::Bounce { floor, restitution, friction }) if p.position.dy > floor => {
                    p.position.dy = floor - (p.position.dy - floor) * restitution;
                    p.velocity.dy = -p.velocity.dy.abs() * restitution;
                    p.velocity.dx *= 1.0 - friction;
                }
                Some(Collision::Kill { floor }) if p.position.dy > floor => return false,
                _ => {}
            }
            true
        });
    }

    /// Advance `seconds` in fixed steps of `dt`.
    pub fn run(&mut self, seconds: f32, dt: f32) {
        let mut t = 0.0;
        while t + 1e-6 < seconds {
            self.step(dt);
            t += dt;
        }
    }

    /// How many are alive.
    #[must_use]
    pub fn len(&self) -> usize {
        self.live.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.live.is_empty()
    }

    /// How many were ever born.
    #[must_use]
    pub const fn births(&self) -> u64 {
        self.births
    }

    /// The particles as drawable [`Particle`]s (over-life size and colour).
    #[must_use]
    pub fn particles(&self) -> Vec<Particle> {
        self.live
            .iter()
            .map(|p| {
                let age01 = (p.age / p.life).clamp(0.0, 1.0);
                Particle {
                    position: p.position,
                    age01,
                    size: self.size.0 + (self.size.1 - self.size.0) * age01,
                    color: self.color.0.lerp(self.color.1, age01),
                }
            })
            .collect()
    }

    /// Velocities, for tests and motion-blurred drawing.
    #[must_use]
    pub fn velocities(&self) -> Vec<Offset> {
        self.live.iter().map(|p| p.velocity).collect()
    }

    /// A per-particle random in 0..1, stable over its life.
    #[must_use]
    pub fn seeds(&self) -> Vec<f32> {
        self.live.iter().map(|p| p.seed).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_times_time_births_and_lifetimes_cap_the_population() {
        let mut s = ParticleSystem::new(EmitterShape::Point(Offset::ZERO), 100.0, 1).lifetime(1.0, 1.0);
        s.run(3.0, 0.01);
        assert!((299..=301).contains(&s.births()), "{}", s.births());
        assert!((99..=101).contains(&s.len()), "{}", s.len());
    }

    #[test]
    fn same_seed_same_particles() {
        let run = || {
            let mut s = ParticleSystem::new(EmitterShape::Circle { center: Offset::ZERO, radius: 10.0, edge: true }, 50.0, 42)
                .force(Force::Turbulence { strength: 50.0, scale: 30.0, speed: 1.0 });
            s.run(1.0, 1.0 / 60.0);
            s.particles()
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn emitter_shapes_place_births() {
        let mut s = ParticleSystem::new(EmitterShape::Circle { center: Offset::new(5.0, 5.0), radius: 10.0, edge: true }, 0.0, 3)
            .forces(vec![]);
        s.speed = (0.0, 0.0);
        s.burst(50);
        for p in s.particles() {
            assert!(((p.position - Offset::new(5.0, 5.0)).distance() - 10.0).abs() < 1e-3);
        }
        let mut r = ParticleSystem::new(EmitterShape::Rect(Rect::new(0.0, 0.0, 4.0, 2.0)), 0.0, 3).forces(vec![]);
        r.speed = (0.0, 0.0);
        r.burst(50);
        assert!(r.particles().iter().all(|p| Rect::new(0.0, 0.0, 4.0, 2.0).contains(p.position)));
    }

    #[test]
    fn floor_bounce_keeps_particles_above_it() {
        let mut s = ParticleSystem::new(EmitterShape::Point(Offset::ZERO), 0.0, 5)
            .launch(std::f32::consts::FRAC_PI_2, 0.0, (100.0, 100.0))
            .lifetime(10.0, 10.0)
            .forces(vec![Force::Gravity(Offset::new(0.0, 400.0))])
            .collision(Collision::Bounce { floor: 50.0, restitution: 0.5, friction: 0.1 });
        s.burst(1);
        let mut max_after_bounce = 0.0f32;
        for i in 0..300 {
            s.step(1.0 / 120.0);
            let y = s.particles()[0].position.dy;
            assert!(y <= 50.0 + 1e-3, "step {i}: {y}");
            if i > 100 {
                max_after_bounce = max_after_bounce.max(50.0 - y);
            }
        }
        assert!(max_after_bounce < 20.0, "restitution loses height");
    }

    #[test]
    fn kill_floor_removes_particles() {
        let mut s = ParticleSystem::new(EmitterShape::Point(Offset::ZERO), 0.0, 5)
            .launch(std::f32::consts::FRAC_PI_2, 0.0, (100.0, 100.0))
            .lifetime(10.0, 10.0)
            .collision(Collision::Kill { floor: 10.0 });
        s.burst(5);
        s.run(0.5, 0.01);
        assert!(s.is_empty());
    }

    #[test]
    fn attractors_pull_vortices_swirl_drag_slows() {
        let mut a = ParticleSystem::new(EmitterShape::Point(Offset::new(100.0, 0.0)), 0.0, 1)
            .forces(vec![Force::Attractor { at: Offset::ZERO, strength: 500.0, radius: 10.0 }])
            .lifetime(5.0, 5.0);
        a.speed = (0.0, 0.0);
        a.burst(1);
        a.run(0.5, 0.01);
        assert!(a.particles()[0].position.dx < 100.0);
        let mut v = ParticleSystem::new(EmitterShape::Point(Offset::new(50.0, 0.0)), 0.0, 1)
            .forces(vec![Force::Vortex { at: Offset::ZERO, strength: 1000.0 }])
            .lifetime(5.0, 5.0);
        v.speed = (0.0, 0.0);
        v.burst(1);
        v.run(0.1, 0.01);
        assert!(v.particles()[0].position.dy > 0.0, "swirls clockwise in Y-down");
        let mut d = ParticleSystem::new(EmitterShape::Point(Offset::ZERO), 0.0, 1)
            .forces(vec![Force::Drag(2.0)])
            .launch(0.0, 0.0, (100.0, 100.0))
            .lifetime(5.0, 5.0);
        d.burst(1);
        d.run(1.0, 0.001);
        let speed = d.velocities()[0].distance();
        assert!((speed - 100.0 * (-2.0f32).exp()).abs() < 1.0, "{speed}");
    }

    #[test]
    fn over_life_size_and_colour() {
        let mut s = ParticleSystem::new(EmitterShape::Point(Offset::ZERO), 0.0, 1)
            .lifetime(1.0, 1.0)
            .look((10.0, 0.0), (Color::WHITE, Color::BLACK));
        s.burst(1);
        s.run(0.5, 0.01);
        let p = s.particles()[0];
        assert!((p.size - 5.0).abs() < 0.2);
        assert!((i32::from(p.color.r) - 128).abs() < 5);
    }
}
