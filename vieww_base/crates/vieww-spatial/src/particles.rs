//! Deterministic particle emission and simulation for effects, UI accents and
//! scene previews. Rendering particles as camera-facing quads is exposed as
//! data, so callers can choose the existing paint/image route or spatial mesh
//! route without a hidden backend dependency.

use std::time::Duration;

use vieww_foundation::{Color, ColorSpace};

use crate::{Camera3, Material3, Mesh3, MeshInstance3, Scene3, Transform3, Vec2, Vec3, Vertex3};

/// Hard storage ceiling even if public emitter configuration is corrupted.
pub const MAX_PARTICLES_PER_EMITTER: usize = 262_144;
/// Hard per-update creation budget to prevent a catch-up spike from monopolizing a frame.
pub const MAX_PARTICLE_SPAWNS_PER_UPDATE: usize = 16_384;

/// One particle's simulation state. `age / lifetime` is its normalized age.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Particle {
    pub position: Vec3,
    pub velocity: Vec3,
    pub age: f32,
    pub lifetime: f32,
    pub start_size: f32,
    pub end_size: f32,
    pub start_color: Color,
    pub end_color: Color,
}
impl Particle {
    #[must_use]
    pub fn is_valid(self) -> bool {
        self.position.is_finite()
            && self.velocity.is_finite()
            && self.age.is_finite()
            && self.age >= 0.0
            && self.lifetime.is_finite()
            && self.lifetime > 0.0
            && self.start_size.is_finite()
            && self.start_size >= 0.0
            && self.end_size.is_finite()
            && self.end_size >= 0.0
    }
    #[must_use]
    pub fn normalized_age(self) -> f32 {
        if !self.age.is_finite() || !self.lifetime.is_finite() || self.lifetime <= 0.0 {
            1.0
        } else {
            (self.age / self.lifetime).clamp(0.0, 1.0)
        }
    }
    #[must_use]
    pub fn size(self) -> f32 {
        if !self.start_size.is_finite() || !self.end_size.is_finite() {
            return 0.0;
        }
        (self.start_size + (self.end_size - self.start_size) * self.normalized_age()).max(0.0)
    }
    #[must_use]
    pub fn color(self) -> Color {
        lerp_color(self.start_color, self.end_color, self.normalized_age())
    }
    #[must_use]
    pub fn is_alive(self) -> bool {
        self.is_valid() && self.age < self.lifetime
    }
}

/// Data required to billboard a particle in the active camera's plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BillboardParticle {
    pub center: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    pub half_size: f32,
    pub color: Color,
}
impl BillboardParticle {
    #[must_use]
    pub fn corners(self) -> [Vec3; 4] {
        let right = self.right * self.half_size;
        let up = self.up * self.half_size;
        [
            self.center - right - up,
            self.center + right - up,
            self.center + right + up,
            self.center - right + up,
        ]
    }
}

/// Seeded emitter with cone spread and bounded per-frame spawning.
#[derive(Debug, Clone)]
pub struct ParticleEmitter {
    pub origin: Vec3,
    pub direction: Vec3,
    pub spread_radians: f32,
    pub rate_per_second: f32,
    pub lifetime_seconds: (f32, f32),
    pub speed: (f32, f32),
    pub size: (f32, f32),
    pub start_color: Color,
    pub end_color: Color,
    pub gravity: Vec3,
    pub max_particles: usize,
    pub seed: u64,
    spawn_remainder: f32,
    particles: Vec<Particle>,
}
impl Default for ParticleEmitter {
    fn default() -> Self {
        Self {
            origin: Vec3::ZERO,
            direction: Vec3::Y,
            spread_radians: 0.25,
            rate_per_second: 24.0,
            lifetime_seconds: (0.5, 1.5),
            speed: (0.5, 2.0),
            size: (0.02, 0.12),
            start_color: Color::rgba(255, 210, 100, 255),
            end_color: Color::rgba(255, 50, 20, 0),
            gravity: Vec3::new(0.0, -1.0, 0.0),
            max_particles: 4096,
            seed: 0x5649_4557_5750_4152,
            spawn_remainder: 0.0,
            particles: Vec::new(),
        }
    }
}
impl ParticleEmitter {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            ..Self::default()
        }
    }
    #[must_use]
    pub fn particles(&self) -> &[Particle] {
        &self.particles
    }
    pub fn particles_mut(&mut self) -> &mut [Particle] {
        &mut self.particles
    }
    pub fn clear(&mut self) {
        self.particles.clear();
        self.spawn_remainder = 0.0;
    }

    /// Advance with caller-supplied time. Returns the number of particles born.
    pub fn update(&mut self, delta: Duration) -> usize {
        let dt = delta.as_secs_f32();
        if !dt.is_finite() || dt <= 0.0 || !self.configuration_is_valid() {
            return 0;
        }
        let particle_limit = self.max_particles.min(MAX_PARTICLES_PER_EMITTER);
        self.particles.retain(|particle| particle.is_valid());
        self.particles.truncate(particle_limit);
        for particle in &mut self.particles {
            particle.velocity = particle.velocity + self.gravity * dt;
            particle.position = particle.position + particle.velocity * dt;
            particle.age += dt;
        }
        self.particles.retain(|particle| particle.is_alive());
        if !self.rate_per_second.is_finite() || self.rate_per_second <= 0.0 {
            self.spawn_remainder = 0.0;
            return 0;
        }
        self.spawn_remainder += (self.rate_per_second * dt).min(particle_limit as f32);
        let requested = self.spawn_remainder.floor() as usize;
        self.spawn_remainder -= requested as f32;
        let capacity = particle_limit.saturating_sub(self.particles.len());
        let spawn_count = requested.min(capacity).min(MAX_PARTICLE_SPAWNS_PER_UPDATE);
        for _ in 0..spawn_count {
            self.spawn_one();
        }
        spawn_count
    }

    fn configuration_is_valid(&self) -> bool {
        self.origin.is_finite()
            && self.direction.is_finite()
            && self.spread_radians.is_finite()
            && (0.0..=std::f32::consts::PI).contains(&self.spread_radians)
            && self.rate_per_second.is_finite()
            && self.rate_per_second >= 0.0
            && self.lifetime_seconds.0.is_finite()
            && self.lifetime_seconds.1.is_finite()
            && self.lifetime_seconds.0 > 0.0
            && self.lifetime_seconds.1 > 0.0
            && self.speed.0.is_finite()
            && self.speed.1.is_finite()
            && self.size.0.is_finite()
            && self.size.1.is_finite()
            && self.size.0 >= 0.0
            && self.size.1 >= 0.0
            && self.gravity.is_finite()
    }

    fn spawn_one(&mut self) {
        let mut direction = self.direction.normalized();
        if direction == Vec3::ZERO {
            direction = Vec3::Y;
        }
        let axis = if direction.dot(Vec3::Y).abs() > 0.98 {
            Vec3::X
        } else {
            Vec3::Y
        };
        let tangent = direction.cross(axis).normalized();
        let bitangent = direction.cross(tangent).normalized();
        let angle = self.random_unit() * std::f32::consts::TAU;
        let cosine = (1.0
            - self.random_unit()
                * (1.0 - self.spread_radians.clamp(0.0, std::f32::consts::PI).cos()))
        .clamp(-1.0, 1.0);
        let sine = (1.0 - cosine * cosine).max(0.0).sqrt();
        let sampled = (direction * cosine
            + tangent * (sine * angle.cos())
            + bitangent * (sine * angle.sin()))
        .normalized();
        let speed = random_range(&mut self.seed, self.speed);
        let lifetime = random_range(&mut self.seed, self.lifetime_seconds).max(0.001);
        let size = random_range(&mut self.seed, self.size).max(0.0);
        self.particles.push(Particle {
            position: self.origin,
            velocity: sampled * speed,
            age: 0.0,
            lifetime,
            start_size: size,
            end_size: 0.0,
            start_color: self.start_color,
            end_color: self.end_color,
        });
    }

    fn random_unit(&mut self) -> f32 {
        random_unit(&mut self.seed)
    }

    /// Return current particle data as camera-aligned quads for a renderer.
    #[must_use]
    pub fn billboards(&self, camera: Camera3) -> Vec<BillboardParticle> {
        let right = camera.right();
        let up = camera.true_up();
        self.particles
            .iter()
            .copied()
            .filter(|particle| particle.is_alive())
            .map(|particle| BillboardParticle {
                center: particle.position,
                right,
                up,
                half_size: particle.size() * 0.5,
                color: particle.color(),
            })
            .collect()
    }

    /// Append current particles to a scene as unlit, camera-facing quads. Each
    /// instance owns its material tint; this is a deliberately simple path for
    /// UI-scale effects rather than a GPU instancing implementation.
    pub fn append_billboards(&self, scene: &mut Scene3, camera: Camera3) {
        for billboard in self.billboards(camera) {
            if billboard.half_size <= 0.0 {
                continue;
            }
            let corners = billboard.corners();
            let normal = -camera.forward();
            let vertices = vec![
                Vertex3::new(corners[0], normal, Vec2::new(0.0, 1.0)),
                Vertex3::new(corners[1], normal, Vec2::new(1.0, 1.0)),
                Vertex3::new(corners[2], normal, Vec2::new(1.0, 0.0)),
                Vertex3::new(corners[3], normal, Vec2::new(0.0, 0.0)),
            ];
            let mesh = Mesh3 {
                vertices,
                indices: vec![0, 1, 2, 0, 2, 3],
            };
            let material = Material3::new(billboard.color).unlit().double_sided(true);
            scene.add(MeshInstance3::new(mesh, material).with_transform(Transform3::IDENTITY));
        }
    }
}

fn random_range(seed: &mut u64, range: (f32, f32)) -> f32 {
    let min = range.0.min(range.1);
    let max = range.0.max(range.1);
    min + (max - min) * random_unit(seed)
}
fn random_unit(seed: &mut u64) -> f32 {
    // Xorshift64*: small, deterministic and explicitly non-cryptographic.
    let mut x = if *seed == 0 {
        0x9e37_79b9_7f4a_7c15
    } else {
        *seed
    };
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *seed = x;
    let value = x.wrapping_mul(0x2545_f491_4f6c_dd1d);
    ((value >> 40) as u32) as f32 / ((1_u32 << 24) as f32)
}
fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    // `Color::lerp` intentionally returns sRGB for UI tween compatibility. A
    // particle interpolation instead preserves the start color's declared
    // primaries and first converts the end point into that space; otherwise a
    // P3 start color would silently become sRGB halfway through its lifetime.
    let b = b.converted_to(a.space);
    let t = if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let channel = |from: u8, to: u8| {
        (f32::from(from) + (f32::from(to) - f32::from(from)) * t)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    let (r, g, b, alpha) = (
        channel(a.r, b.r),
        channel(a.g, b.g),
        channel(a.b, b.b),
        channel(a.a, b.a),
    );
    match a.space {
        ColorSpace::Srgb => Color::rgba(r, g, b, alpha),
        ColorSpace::DisplayP3 => Color::p3a(r, g, b, alpha),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_emitters_produce_the_same_particles() {
        let mut a = ParticleEmitter::new(7);
        let mut b = ParticleEmitter::new(7);
        a.rate_per_second = 20.0;
        b.rate_per_second = 20.0;
        assert_eq!(a.update(Duration::from_millis(250)), 5);
        assert_eq!(b.update(Duration::from_millis(250)), 5);
        assert_eq!(a.particles(), b.particles());
    }

    #[test]
    fn invalid_emitter_configuration_does_not_emit_nan_particles() {
        let mut emitter = ParticleEmitter::new(5);
        emitter.gravity.x = f32::NAN;
        assert_eq!(emitter.update(Duration::from_secs(1)), 0);
        assert!(emitter.particles().is_empty());
    }

    #[test]
    fn particles_expire_and_report_lifecycle_values() {
        let mut emitter = ParticleEmitter::new(3);
        emitter.rate_per_second = 2.0;
        emitter.lifetime_seconds = (0.1, 0.1);
        emitter.update(Duration::from_millis(500));
        assert_eq!(emitter.particles().len(), 1);
        emitter.update(Duration::from_millis(200));
        assert!(emitter.particles().is_empty());
    }

    #[test]
    fn excessive_configuration_is_bounded_per_update_and_in_storage() {
        let mut emitter = ParticleEmitter::new(11);
        emitter.max_particles = usize::MAX;
        emitter.rate_per_second = 1.0e12;
        let spawned = emitter.update(Duration::from_secs(100));
        assert!(spawned <= MAX_PARTICLE_SPAWNS_PER_UPDATE);
        assert!(emitter.particles().len() <= MAX_PARTICLE_SPAWNS_PER_UPDATE);
    }

    #[test]
    fn invalid_mutated_particles_do_not_reach_billboards() {
        let mut emitter = ParticleEmitter::new(13);
        emitter.rate_per_second = 1.0;
        emitter.update(Duration::from_secs(1));
        assert_eq!(emitter.particles().len(), 1);
        emitter.particles_mut()[0].position.x = f32::NAN;
        assert!(emitter
            .billboards(
                Camera3::look_at(
                    Vec3::new(0.0, 0.0, 3.0),
                    Vec3::ZERO,
                    Vec3::Y,
                    crate::Projection3::perspective(1.0, 0.1, 100.0).unwrap()
                )
                .unwrap()
            )
            .is_empty());
    }

    #[test]
    fn color_interpolation_keeps_the_start_color_space_when_endpoints_differ() {
        let p3 = Color::p3(0, 255, 0);
        let middle = lerp_color(p3, Color::rgba(255, 0, 0, 255), 0.5);
        assert_eq!(middle.space, ColorSpace::DisplayP3);
        assert_ne!(middle, Color::rgba(127, 127, 0, 255));

        let srgb = lerp_color(Color::RED, p3, 0.5);
        assert_eq!(srgb.space, ColorSpace::Srgb);
        assert_ne!(srgb, Color::RED);
        assert_ne!(srgb, p3);
    }

    #[test]
    fn same_space_particle_color_interpolation_matches_channel_lerp() {
        let start = Color::p3a(10, 20, 30, 40);
        let end = Color::p3a(110, 120, 130, 140);
        let middle = lerp_color(start, end, 0.5);
        assert_eq!(middle, Color::p3a(60, 70, 80, 90));
    }
}
