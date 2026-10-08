//! Dynamic diffuse global illumination — the Lumen row of the document
//! (§2.17 L5: "real-time global illumination … without baked lighting"),
//! in the shape the industry converged on for it: an **irradiance probe
//! volume** (DDGI, Majercik et al. 2019; Lumen's radiance cache is the same
//! idea with screen-space traces in front of it).
//!
//! # How it works
//!
//! A regular grid of probes fills a box. Each probe shoots `rays_per_probe`
//! rays through the scene's BVH (the path tracer's own [`TraceScene`]),
//! each ray carrying real path-traced radiance — direct light by next-event
//! estimation at the hit, emissive surfaces, a bounce, and the environment
//! when it escapes — and projects what it sees into nine spherical-harmonic
//! coefficients ([`Sh9`]). Shading then reads the volume: trilinear over the
//! eight surrounding probes, each weighted by how much it faces the shaded
//! point, giving colour bleeding and sky occlusion the flat ambient term
//! cannot.
//!
//! **Dynamic** is [`IrradianceVolume::update`]: a frame budget of probes is
//! retraced round-robin and blended into the stored value with hysteresis,
//! so a moving light or object propagates through the volume over a few
//! frames instead of requiring a rebake — Lumen's and DDGI's update model.
//!
//! **Leaks** are the classic probe failure (a probe buried in a wall sees
//! the wall's back faces). A probe whose rays mostly strike back faces is
//! marked dead and excluded from interpolation, DDGI's "probe
//! classification".
//!
//! Every ray is seeded from `(probe, ray, generation)`, so a bake is
//! deterministic to the bit.

use crate::environment::Sh9;
use crate::math::Vec3;
use crate::pathtrace::{PathTracer, Rng, TraceScene};
use crate::raycast::Ray;
use crate::scene::{Rgb, Scene};

/// The probe grid. See the [module docs](self).
#[derive(Debug, Clone)]
pub struct IrradianceVolume {
    /// Minimum corner.
    pub origin: Vec3,
    /// Size of the box.
    pub extent: Vec3,
    /// Probes along x, y, z (each ≥ 2).
    pub dims: [usize; 3],
    pub rays_per_probe: u32,
    /// Bounces each probe ray follows (1 = direct light at the hit).
    pub bounces: u32,
    /// 0 = replace on update, 0.97 = slow and stable.
    pub hysteresis: f32,
    /// Multiplier at shading time.
    pub intensity: f32,
    probes: Vec<Sh9>,
    alive: Vec<bool>,
    cursor: usize,
    generation: u64,
}

/// What an update did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GiStats {
    pub probes_traced: usize,
    pub rays: u64,
    pub dead_probes: usize,
}

/// Fibonacci-sphere direction `i` of `n` — a low-discrepancy, deterministic
/// set rotated per generation by `twist`.
fn sphere_dir(i: u32, n: u32, twist: f32) -> Vec3 {
    let golden = std::f32::consts::PI * (3.0 - 5f32.sqrt());
    #[allow(clippy::cast_precision_loss)]
    let y = 1.0 - (i as f32 + 0.5) / n as f32 * 2.0;
    let r = (1.0 - y * y).max(0.0).sqrt();
    #[allow(clippy::cast_precision_loss)]
    let phi = golden * i as f32 + twist;
    Vec3::new(r * phi.cos(), y, r * phi.sin())
}

impl IrradianceVolume {
    /// A volume over the box `origin .. origin + extent`.
    #[must_use]
    pub fn new(origin: Vec3, extent: Vec3, dims: [usize; 3]) -> Self {
        let dims = dims.map(|d| d.max(2));
        let n = dims[0] * dims[1] * dims[2];
        Self {
            origin,
            extent,
            dims,
            rays_per_probe: 128,
            bounces: 2,
            hysteresis: 0.0,
            intensity: 1.0,
            probes: vec![Sh9::default(); n],
            alive: vec![true; n],
            cursor: 0,
            generation: 0,
        }
    }

    /// A volume fitted to everything visible in `scene`, padded by 5 %.
    #[must_use]
    pub fn fit(scene: &mut Scene, dims: [usize; 3]) -> Self {
        scene.update_world();
        let mut lo = Vec3::splat(f32::INFINITY);
        let mut hi = Vec3::splat(f32::NEG_INFINITY);
        for id in scene.traverse() {
            if !scene.effectively_visible(id) {
                continue;
            }
            let node = scene.node(id);
            if let crate::scene::Content::Mesh { mesh, .. } = &node.content {
                let w = node.world();
                for p in &mesh.positions {
                    let q = w.transform_point(Vec3::from_array(*p));
                    lo = lo.min(q);
                    hi = hi.max(q);
                }
            }
        }
        if !lo.x.is_finite() {
            lo = Vec3::splat(-1.0);
            hi = Vec3::splat(1.0);
        }
        let pad = (hi - lo) * 0.05 + Vec3::splat(1e-3);
        Self::new(lo - pad, hi - lo + pad * 2.0, dims)
    }

    #[must_use]
    pub fn probe_count(&self) -> usize {
        self.probes.len()
    }

    fn index(&self, x: usize, y: usize, z: usize) -> usize {
        (z * self.dims[1] + y) * self.dims[0] + x
    }

    /// World position of probe `i`.
    #[must_use]
    pub fn probe_position(&self, i: usize) -> Vec3 {
        let x = i % self.dims[0];
        let y = (i / self.dims[0]) % self.dims[1];
        let z = i / (self.dims[0] * self.dims[1]);
        let step = self.step();
        #[allow(clippy::cast_precision_loss)]
        let off = Vec3::new(x as f32 * step.x, y as f32 * step.y, z as f32 * step.z);
        self.origin + off
    }

    fn step(&self) -> Vec3 {
        #[allow(clippy::cast_precision_loss)]
        Vec3::new(
            self.extent.x / (self.dims[0] - 1) as f32,
            self.extent.y / (self.dims[1] - 1) as f32,
            self.extent.z / (self.dims[2] - 1) as f32,
        )
    }

    /// The SH of probe `i`.
    #[must_use]
    pub fn probe(&self, i: usize) -> &Sh9 {
        &self.probes[i]
    }

    /// Is probe `i` used for shading (not buried in geometry)?
    #[must_use]
    pub fn is_alive(&self, i: usize) -> bool {
        self.alive[i]
    }

    fn trace_probe(&self, trace: &TraceScene, i: usize) -> (Sh9, bool, u64) {
        let pos = self.probe_position(i);
        let n = self.rays_per_probe.max(8);
        let tracer = PathTracer {
            max_bounces: self.bounces.max(1),
            seed: 0,
            threads: 1,
        };
        #[allow(clippy::cast_precision_loss)]
        let twist = (self.generation as f32 * 2.399_963).rem_euclid(std::f32::consts::TAU);
        let mut sh = Sh9::default();
        let mut back = 0u32;
        let mut rays = 0u64;
        let w = 4.0 * std::f32::consts::PI / n as f32;
        for r in 0..n {
            let d = sphere_dir(r, n, twist);
            let ray = Ray::new(pos, d);
            if trace.hits_backface(&ray) == Some(true) {
                back += 1;
            }
            let mut rng = Rng::new(
                (i as u64).wrapping_mul(0x9E37_79B9) ^ (u64::from(r) << 32) ^ self.generation,
            );
            let (radiance, k) = tracer.trace(trace, ray, &mut rng);
            rays += k;
            sh.add_sample(d, radiance, w);
        }
        // DDGI's classification: mostly back faces = inside a wall.
        let alive = f64::from(back) / f64::from(n) < 0.3;
        (sh, alive, rays)
    }

    /// Trace every probe from scratch.
    pub fn bake(&mut self, scene: &mut Scene) -> GiStats {
        let trace = TraceScene::build(scene);
        let mut stats = GiStats::default();
        for i in 0..self.probes.len() {
            let (sh, alive, rays) = self.trace_probe(&trace, i);
            self.probes[i] = sh;
            self.alive[i] = alive;
            stats.rays += rays;
        }
        stats.probes_traced = self.probes.len();
        stats.dead_probes = self.alive.iter().filter(|a| !**a).count();
        self.generation += 1;
        stats
    }

    /// Retrace `budget` probes (round-robin) against an already-built
    /// [`TraceScene`] and blend them in with [`hysteresis`](Self::hysteresis).
    pub fn update(&mut self, trace: &TraceScene, budget: usize) -> GiStats {
        let mut stats = GiStats::default();
        let n = self.probes.len();
        for _ in 0..budget.min(n) {
            let i = self.cursor;
            self.cursor = (self.cursor + 1) % n;
            if self.cursor == 0 {
                self.generation += 1;
            }
            let (sh, alive, rays) = self.trace_probe(trace, i);
            self.probes[i] = sh.lerp(&self.probes[i], self.hysteresis.clamp(0.0, 0.999));
            self.alive[i] = alive;
            stats.rays += rays;
            stats.probes_traced += 1;
        }
        stats.dead_probes = self.alive.iter().filter(|a| !**a).count();
        stats
    }

    /// Diffuse irradiance / π at world point `p` with normal `n`.
    #[must_use]
    pub fn sample(&self, p: Vec3, n: Vec3) -> Rgb {
        let step = self.step();
        // Normal offset pushes the lookup off the surface (fewer leaks).
        let q = p + n * (step.x.min(step.y).min(step.z) * 0.25) - self.origin;
        let g = Vec3::new(q.x / step.x, q.y / step.y, q.z / step.z);
        let clampi = |v: f32, d: usize| {
            #[allow(clippy::cast_precision_loss)]
            let v = v.clamp(0.0, (d - 1) as f32 - 1e-4);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let i = v.floor() as usize;
            #[allow(clippy::cast_precision_loss)]
            (i.min(d - 2), v - i.min(d - 2) as f32)
        };
        let (x0, tx) = clampi(g.x, self.dims[0]);
        let (y0, ty) = clampi(g.y, self.dims[1]);
        let (z0, tz) = clampi(g.z, self.dims[2]);
        let mut acc = Rgb::BLACK;
        let mut wsum = 0.0;
        for dz in 0..2 {
            for dy in 0..2 {
                for dx in 0..2 {
                    let i = self.index(x0 + dx, y0 + dy, z0 + dz);
                    if !self.alive[i] {
                        continue;
                    }
                    let tri = (if dx == 1 { tx } else { 1.0 - tx })
                        * (if dy == 1 { ty } else { 1.0 - ty })
                        * (if dz == 1 { tz } else { 1.0 - tz });
                    // Backface weight: probes behind the surface count less.
                    let to = (self.probe_position(i) - p).normalize();
                    let facing = ((n.dot(to) + 1.0) * 0.5).powi(2) + 0.05;
                    let w = tri * facing;
                    if w <= 0.0 {
                        continue;
                    }
                    acc = acc.add(self.probes[i].irradiance(n).scale(w));
                    wsum += w;
                }
            }
        }
        if wsum <= 0.0 {
            return Rgb::BLACK;
        }
        acc.scale(self.intensity / wsum)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{box_mesh, plane};
    use crate::scene::{Content, Material, Node};
    use crate::Quat;
    use vieww_foundation::Color;

    fn cornell() -> Scene {
        let mut s = Scene::new();
        s.background = Rgb::BLACK;
        // Floor (white), left wall (red), right wall (green); a sun.
        s.add(
            {
                let mut floor =
                    Node::new("floor", Content::mesh(plane(4.0, 4.0, 1, 1), Material::lambert(Color::WHITE)));
                floor.rotation = Quat::from_axis_angle(Vec3::X, -std::f32::consts::FRAC_PI_2);
                floor
            },
            None,
        );
        s.add(
            Node::new(
                "red",
                Content::mesh(box_mesh(0.1, 4.0, 4.0), Material::lambert(Color::rgb(255, 0, 0))),
            )
            .at(Vec3::new(-2.0, 2.0, 0.0)),
            None,
        );
        s.add(
            Node::new(
                "green",
                Content::mesh(box_mesh(0.1, 4.0, 4.0), Material::lambert(Color::rgb(0, 255, 0))),
            )
            .at(Vec3::new(2.0, 2.0, 0.0)),
            None,
        );
        s.add(
            Node::new(
                "sun",
                Content::Light(crate::Light::Directional {
                    color: Rgb::WHITE,
                    intensity: 3.0,
                    direction: Vec3::new(0.0, -1.0, 0.0),
                    shadow: false,
                }),
            ),
            None,
        );
        s
    }

    #[test]
    fn colour_bleeds_from_the_walls_onto_the_floor() {
        let mut s = cornell();
        let mut gi = IrradianceVolume::new(Vec3::new(-1.9, 0.1, -1.9), Vec3::new(3.8, 3.0, 3.8), [5, 3, 5]);
        gi.rays_per_probe = 96;
        let stats = gi.bake(&mut s);
        assert_eq!(stats.probes_traced, 75);
        assert!(stats.rays > 75 * 96);
        let near_red = gi.sample(Vec3::new(-1.7, 0.0, 0.0), Vec3::Y);
        let near_green = gi.sample(Vec3::new(1.7, 0.0, 0.0), Vec3::Y);
        assert!(near_red.r > near_red.g, "{near_red:?}");
        assert!(near_green.g > near_green.r, "{near_green:?}");
    }

    #[test]
    fn bakes_are_deterministic() {
        let mut a = IrradianceVolume::new(Vec3::splat(-1.0), Vec3::splat(2.0), [2, 2, 2]);
        let mut b = a.clone();
        let mut s = cornell();
        a.rays_per_probe = 32;
        b.rays_per_probe = 32;
        a.bake(&mut s);
        b.bake(&mut s);
        for i in 0..a.probe_count() {
            assert_eq!(a.probe(i), b.probe(i));
        }
    }

    #[test]
    fn probes_buried_in_geometry_are_classified_dead() {
        let mut s = Scene::new();
        s.add(
            Node::new("block", Content::mesh(box_mesh(2.0, 2.0, 2.0), Material::lambert(Color::WHITE))),
            None,
        );
        // A 3×3×3 grid whose centre probe sits inside the block.
        let mut gi = IrradianceVolume::new(Vec3::splat(-3.0), Vec3::splat(6.0), [3, 3, 3]);
        gi.rays_per_probe = 32;
        let stats = gi.bake(&mut s);
        assert!(!gi.is_alive(13));
        assert_eq!(stats.dead_probes, 1);
        assert!(gi.is_alive(0));
    }

    #[test]
    fn dynamic_updates_converge_to_the_new_lighting() {
        let mut s = cornell();
        let mut gi = IrradianceVolume::new(Vec3::new(-1.5, 0.5, -1.5), Vec3::new(3.0, 2.0, 3.0), [2, 2, 2]);
        gi.rays_per_probe = 48;
        gi.bake(&mut s);
        let before = gi.sample(Vec3::new(0.0, 0.0, 0.0), Vec3::Y).luminance();
        // Turn the sun off: rebuild the trace, update the whole volume a
        // few times with hysteresis — it must darken.
        let sun = s.find("sun").expect("sun");
        s.node_mut(sun).visible = false;
        let trace = TraceScene::build(&mut s);
        gi.hysteresis = 0.5;
        for _ in 0..6 {
            gi.update(&trace, gi.probe_count());
        }
        let after = gi.sample(Vec3::new(0.0, 0.0, 0.0), Vec3::Y).luminance();
        assert!(after < before * 0.1, "{before} → {after}");
    }

    #[test]
    fn fit_encloses_the_scene() {
        let mut s = cornell();
        let gi = IrradianceVolume::fit(&mut s, [4, 4, 4]);
        assert!(gi.origin.x < -2.0 && gi.origin.x + gi.extent.x > 2.0);
        assert_eq!(gi.probe_count(), 64);
    }
}
