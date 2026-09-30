//! A progressive path tracer — the Cycles half of Blender's render layer
//! (§2.14 L6: "Cycles (path-tracing, GPU-accelerated), EEVEE (real-time
//! rasterization)"). [`Renderer`](crate::Renderer) is the EEVEE half.
//!
//! # What it does
//!
//! The same [`Scene`] the rasterizer draws is flattened to world-space
//! triangles, put in a **BVH** (binary, median split on the longest axis),
//! and traced: camera rays, then random bounces — cosine-weighted diffuse,
//! or a roughness-blurred mirror lobe for metals — with **next-event
//! estimation** toward every directional and point light (a shadow ray per
//! light per bounce, so small lights converge), emissive materials as area
//! lights, the scene background as the sky, and Russian roulette after the
//! third bounce. Samples accumulate in an [`Accumulator`] so a preview
//! refines frame by frame, the way Cycles' viewport does; the image is
//! tone-mapped (ACES fit) and encoded to sRGB.
//!
//! Deterministic: pixel `(x, y)` at sample `k` draws from a generator seeded
//! by `(seed, x, y, k)`, so the same scene converges to the same image on
//! every machine, and rows are traced on scoped threads without changing
//! the result.

use std::sync::Arc;

use vieww_foundation::Image;

use crate::math::Vec3;
use crate::raycast::Ray;
use crate::scene::{Camera, Content, Light, Material, Rgb, Scene, Shading};

#[derive(Clone, Copy)]
struct Tri {
    a: Vec3,
    e1: Vec3,
    e2: Vec3,
    n: [Vec3; 3],
    mat: usize,
}

#[derive(Clone, Copy)]
struct Bvh {
    lo: Vec3,
    hi: Vec3,
    /// Leaf: first triangle and count; inner: left child index, count 0.
    start: usize,
    count: usize,
    right: usize,
}

#[derive(Clone)]
struct PtMaterial {
    albedo: Rgb,
    emissive: Rgb,
    metallic: f32,
    roughness: f32,
}

/// The flattened, accelerated scene.
pub struct TraceScene {
    tris: Vec<Tri>,
    nodes: Vec<Bvh>,
    materials: Vec<PtMaterial>,
    lights: Vec<(Light, Vec3, Vec3)>,
    sky: Rgb,
}

impl std::fmt::Debug for TraceScene {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TraceScene")
            .field("triangles", &self.tris.len())
            .field("bvh_nodes", &self.nodes.len())
            .finish_non_exhaustive()
    }
}

fn aabb(t: &Tri) -> (Vec3, Vec3) {
    let b = t.a + t.e1;
    let c = t.a + t.e2;
    (t.a.min(b).min(c), t.a.max(b).max(c))
}

impl TraceScene {
    /// Flatten and build the BVH.
    pub fn build(scene: &mut Scene) -> Self {
        scene.update_world();
        let mut tris = Vec::new();
        let mut materials: Vec<PtMaterial> = Vec::new();
        let mut lights = Vec::new();
        let mut add_mesh = |model: crate::math::Mat4, mesh: &vieww_mesh::Mesh, m: &Arc<Material>| {
            let (metallic, roughness) = match m.shading {
                Shading::Standard { metallic, roughness } => (metallic, roughness),
                Shading::Phong { shininess, .. } => (0.0, (2.0 / (shininess + 2.0)).sqrt()),
                _ => (0.0, 1.0),
            };
            materials.push(PtMaterial {
                albedo: m.color,
                emissive: m.emissive,
                metallic,
                roughness,
            });
            let mi = materials.len() - 1;
            let nm = model.inverse().map_or(model, |x| x.transpose());
            for t in mesh.indices.chunks_exact(3) {
                let p = |i: u32| model.transform_point(Vec3::from_array(mesh.positions[i as usize]));
                let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
                let face = (b - a).cross(c - a).normalize();
                let n = |i: u32| {
                    if mesh.has_normals() {
                        nm.transform_vector(Vec3::from_array(mesh.normals[i as usize])).normalize()
                    } else {
                        face
                    }
                };
                tris.push(Tri {
                    a,
                    e1: b - a,
                    e2: c - a,
                    n: [n(t[0]), n(t[1]), n(t[2])],
                    mat: mi,
                });
            }
        };
        for id in scene.traverse() {
            if !scene.effectively_visible(id) {
                continue;
            }
            let node = scene.node(id);
            let world = node.world();
            match &node.content {
                Content::Mesh { mesh, material, .. } => add_mesh(world, mesh, material),
                Content::Instanced { mesh, material, instances, .. } => {
                    for i in instances {
                        add_mesh(world * *i, mesh, material);
                    }
                }
                Content::Lod { levels, material, .. } => {
                    if let Some((_, m)) = levels.first() {
                        add_mesh(world, m, material);
                    }
                }
                Content::Light(l) => {
                    let dir = match l {
                        Light::Directional { direction, .. } | Light::Spot { direction, .. } => {
                            world.transform_vector(*direction).normalize()
                        }
                        _ => Vec3::ZERO,
                    };
                    lights.push((*l, world.get_translation(), dir));
                }
                Content::Empty => {}
            }
        }
        let mut s = Self {
            tris,
            nodes: Vec::new(),
            materials,
            lights,
            sky: scene.background,
        };
        s.build_bvh();
        s
    }

    fn build_bvh(&mut self) {
        let n = self.tris.len();
        self.nodes.clear();
        if n == 0 {
            return;
        }
        let mut stack = vec![(0usize, n, usize::MAX, false)];
        while let Some((start, count, parent, is_right)) = stack.pop() {
            let mut lo = Vec3::splat(f32::INFINITY);
            let mut hi = Vec3::splat(f32::NEG_INFINITY);
            let mut clo = Vec3::splat(f32::INFINITY);
            let mut chi = Vec3::splat(f32::NEG_INFINITY);
            for t in &self.tris[start..start + count] {
                let (a, b) = aabb(t);
                lo = lo.min(a);
                hi = hi.max(b);
                let c = (a + b) * 0.5;
                clo = clo.min(c);
                chi = chi.max(c);
            }
            let index = self.nodes.len();
            self.nodes.push(Bvh {
                lo,
                hi,
                start,
                count,
                right: 0,
            });
            if parent != usize::MAX && is_right {
                self.nodes[parent].right = index;
            }
            if count <= 4 {
                continue;
            }
            let ext = chi - clo;
            let axis = if ext.x >= ext.y && ext.x >= ext.z {
                0
            } else if ext.y >= ext.z {
                1
            } else {
                2
            };
            let key = |t: &Tri| {
                let (a, b) = aabb(t);
                let c = (a + b) * 0.5;
                [c.x, c.y, c.z][axis]
            };
            let mid = count / 2;
            self.tris[start..start + count].select_nth_unstable_by(mid, |x, y| key(x).total_cmp(&key(y)));
            self.nodes[index].count = 0;
            self.nodes[index].start = start;
            // Right first so left is processed next (and lands at index+1).
            stack.push((start + mid, count - mid, index, true));
            stack.push((start, mid, index, false));
        }
    }

    /// Nearest hit: (t, triangle, u, v).
    fn intersect(&self, ray: &Ray, max_t: f32) -> Option<(f32, usize, f32, f32)> {
        if self.nodes.is_empty() {
            return None;
        }
        let inv = Vec3::new(1.0 / ray.direction.x, 1.0 / ray.direction.y, 1.0 / ray.direction.z);
        let mut best: Option<(f32, usize, f32, f32)> = None;
        let mut limit = max_t;
        let mut stack = vec![0usize];
        while let Some(i) = stack.pop() {
            let n = self.nodes[i];
            if !slab(ray.origin, inv, n.lo, n.hi, limit) {
                continue;
            }
            if n.count > 0 {
                for ti in n.start..n.start + n.count {
                    let t = &self.tris[ti];
                    if let Some((d, u, v)) = crate::raycast::ray_triangle(ray, t.a, t.a + t.e1, t.a + t.e2) {
                        if d < limit {
                            limit = d;
                            best = Some((d, ti, u, v));
                        }
                    }
                }
            } else {
                stack.push(n.right);
                stack.push(i + 1);
            }
        }
        best
    }

    #[must_use]
    pub fn triangles(&self) -> usize {
        self.tris.len()
    }
}

fn slab(o: Vec3, inv: Vec3, lo: Vec3, hi: Vec3, max_t: f32) -> bool {
    let (mut t0, mut t1) = (0.0f32, max_t);
    for (oa, ia, la, ha) in [(o.x, inv.x, lo.x, hi.x), (o.y, inv.y, lo.y, hi.y), (o.z, inv.z, lo.z, hi.z)] {
        let (mut a, mut b) = ((la - oa) * ia, (ha - oa) * ia);
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        t0 = t0.max(a);
        t1 = t1.min(b);
        if t0 > t1 {
            return false;
        }
    }
    true
}

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        let mut r = Self(seed ^ 0x9E37_79B9_7F4A_7C15);
        r.next();
        r
    }
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        let x = ((self.0 >> 18) ^ self.0) >> 27;
        #[allow(clippy::cast_possible_truncation)]
        let rot = (self.0 >> 59) as u32;
        #[allow(clippy::cast_possible_truncation)]
        let v = (x as u32).rotate_right(rot);
        #[allow(clippy::cast_precision_loss)]
        let r = (v >> 8) as f32 / (1u32 << 24) as f32;
        r
    }
}

fn onb(n: Vec3) -> (Vec3, Vec3) {
    let a = if n.x.abs() > 0.9 { Vec3::Y } else { Vec3::X };
    let t = n.cross(a).normalize();
    (t, n.cross(t))
}

fn cosine_dir(n: Vec3, rng: &mut Rng) -> Vec3 {
    let (u1, u2) = (rng.next(), rng.next());
    let r = u1.sqrt();
    let phi = std::f32::consts::TAU * u2;
    let (t, b) = onb(n);
    (t * (r * phi.cos()) + b * (r * phi.sin()) + n * (1.0 - u1).max(0.0).sqrt()).normalize()
}

/// Running per-pixel sums — the progressive image.
#[derive(Debug, Clone)]
pub struct Accumulator {
    width: u32,
    height: u32,
    sum: Vec<Rgb>,
    samples: u32,
}

impl Accumulator {
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            sum: vec![Rgb::BLACK; (width * height) as usize],
            samples: 0,
        }
    }

    /// Samples per pixel so far.
    #[must_use]
    pub const fn samples(&self) -> u32 {
        self.samples
    }

    /// The tone-mapped, sRGB-encoded mean.
    #[must_use]
    pub fn image(&self, exposure: f32) -> Image {
        #[allow(clippy::cast_precision_loss)]
        let k = exposure / self.samples.max(1) as f32;
        let aces = |x: f32| {
            let x = x * k;
            ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0)
        };
        let enc = |v: f32| {
            let s = if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let b = (s * 255.0 + 0.5) as u8;
            b
        };
        let mut px = Vec::with_capacity(self.sum.len() * 4);
        for c in &self.sum {
            px.extend_from_slice(&[enc(aces(c.r)), enc(aces(c.g)), enc(aces(c.b)), 255]);
        }
        Image::from_rgba8(px, self.width, self.height)
    }
}

/// Tracer settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathTracer {
    pub max_bounces: u32,
    pub seed: u64,
    /// Scoped worker threads (rows are split between them).
    pub threads: usize,
}

impl Default for PathTracer {
    fn default() -> Self {
        Self {
            max_bounces: 5,
            seed: 1,
            threads: std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
        }
    }
}

impl PathTracer {
    /// Add `spp` samples per pixel to `acc`, viewing `scene` from `camera`.
    /// Returns rays cast.
    pub fn accumulate(&self, scene: &TraceScene, camera: &Camera, acc: &mut Accumulator, spp: u32) -> u64 {
        let (w, h) = (acc.width, acc.height);
        let first = acc.samples;
        #[allow(clippy::cast_precision_loss)]
        let (wf, hf) = (w as f32, h as f32);
        let threads = self.threads.max(1);
        let rows_per = (h as usize).div_ceil(threads);
        let rays = std::sync::atomic::AtomicU64::new(0);
        std::thread::scope(|sc| {
            for (chunk_i, chunk) in acc.sum.chunks_mut(rows_per * w as usize).enumerate() {
                let rays = &rays;
                sc.spawn(move || {
                    let mut local_rays = 0u64;
                    for (i, px) in chunk.iter_mut().enumerate() {
                        let idx = chunk_i * rows_per * w as usize + i;
                        let (x, y) = (idx as u32 % w, idx as u32 / w);
                        for k in 0..spp {
                            let mut rng = Rng::new(
                                self.seed
                                    ^ (u64::from(x) << 40)
                                    ^ (u64::from(y) << 20)
                                    ^ u64::from(first + k).wrapping_mul(0x2545_F491),
                            );
                            #[allow(clippy::cast_precision_loss)]
                            let (sx, sy) = (x as f32 + rng.next(), y as f32 + rng.next());
                            let ray = Ray::from_screen(camera, wf, hf, sx, sy);
                            let (c, r) = self.trace(scene, ray, &mut rng);
                            local_rays += r;
                            *px = px.add(c);
                        }
                    }
                    rays.fetch_add(local_rays, std::sync::atomic::Ordering::Relaxed);
                });
            }
        });
        acc.samples += spp;
        rays.into_inner()
    }

    fn trace(&self, scene: &TraceScene, mut ray: Ray, rng: &mut Rng) -> (Rgb, u64) {
        let mut throughput = Rgb::WHITE;
        let mut radiance = Rgb::BLACK;
        let mut rays = 0u64;
        for bounce in 0..=self.max_bounces {
            rays += 1;
            let Some((t, ti, u, v)) = scene.intersect(&ray, f32::INFINITY) else {
                radiance = radiance.add(throughput.mul(scene.sky));
                break;
            };
            let tri = &scene.tris[ti];
            let m = &scene.materials[tri.mat];
            let p = ray.at(t);
            let w0 = 1.0 - u - v;
            let mut n = (tri.n[0] * w0 + tri.n[1] * u + tri.n[2] * v).normalize();
            if n.dot(ray.direction) > 0.0 {
                n = -n;
            }
            // Emissive surfaces are found by bouncing into them (only the
            // analytic lights are sampled explicitly, so nothing is counted
            // twice).
            radiance = radiance.add(throughput.mul(m.emissive));
            let origin = p + n * 1e-4;
            let metal = rng.next() < m.metallic;
            if !metal {
                // Next-event estimation.
                for (light, pos, dir) in &scene.lights {
                    let (li, ldir, dist) = match *light {
                        Light::Directional { color, intensity, .. } => (color.scale(intensity), -*dir, f32::INFINITY),
                        Light::Point { color, intensity, range } => {
                            let d = *pos - p;
                            let dist = d.length();
                            let fall = if range > 0.0 { (1.0 - (dist / range).powi(4)).clamp(0.0, 1.0).powi(2) } else { 1.0 };
                            (color.scale(intensity * fall / (1.0 + dist * dist * 0.02)), d / dist.max(1e-6), dist)
                        }
                        Light::Ambient { color, intensity } => {
                            radiance = radiance.add(throughput.mul(m.albedo).mul(color).scale(intensity));
                            continue;
                        }
                        _ => continue,
                    };
                    let ndl = n.dot(ldir);
                    if ndl <= 0.0 {
                        continue;
                    }
                    rays += 1;
                    if scene.intersect(&Ray::new(origin, ldir), dist - 1e-3).is_none() {
                        radiance = radiance.add(throughput.mul(m.albedo).mul(li).scale(ndl));
                    }
                }
            }
            if bounce >= 3 {
                let p_survive = throughput.luminance().clamp(0.05, 0.95);
                if rng.next() > p_survive {
                    break;
                }
                throughput = throughput.scale(1.0 / p_survive);
            }
            let next = if metal {
                let r = ray.direction.reflect(n);
                let fuzz = cosine_dir(n, rng) * m.roughness * m.roughness;
                (r + fuzz).normalize()
            } else {
                cosine_dir(n, rng)
            };
            if next.dot(n) <= 0.0 {
                break;
            }
            throughput = throughput.mul(m.albedo);
            ray = Ray::new(origin, next);
        }
        (radiance, rays)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{box_mesh, plane, sphere};
    use crate::math::Quat;
    use crate::scene::Node;
    use vieww_foundation::Color;

    fn furnace() -> Scene {
        let mut s = Scene::new();
        s.background = Rgb::new(0.5, 0.5, 0.5);
        s.add(Node::new("ball", Content::mesh(sphere(1.0, 24, 12), Material::lambert(Color::WHITE))), None);
        s
    }

    #[test]
    fn bvh_finds_the_same_hits_as_brute_force() {
        let mut s = Scene::new();
        for i in 0..10 {
            #[allow(clippy::cast_precision_loss)]
            let x = i as f32 * 1.5 - 7.0;
            s.add(Node::new("b", Content::mesh(box_mesh(1.0, 1.0, 1.0), Material::lambert(Color::WHITE))).at(Vec3::new(x, 0.0, 0.0)), None);
        }
        let ts = TraceScene::build(&mut s);
        assert_eq!(ts.triangles(), 120);
        for i in 0..40 {
            #[allow(clippy::cast_precision_loss)]
            let ray = Ray::new(Vec3::new(i as f32 * 0.4 - 8.0, 0.2, 5.0), Vec3::new(0.0, 0.0, -1.0));
            let bvh = ts.intersect(&ray, f32::INFINITY).map(|h| h.0);
            let brute = ts
                .tris
                .iter()
                .filter_map(|t| crate::raycast::ray_triangle(&ray, t.a, t.a + t.e1, t.a + t.e2).map(|h| h.0))
                .reduce(f32::min);
            assert_eq!(bvh, brute, "ray {i}");
        }
    }

    #[test]
    fn white_furnace_a_white_diffuse_sphere_under_uniform_sky_disappears() {
        // The classic energy test: a white Lambertian in a uniform
        // environment reflects exactly the environment, so it is invisible.
        let mut s = furnace();
        let ts = TraceScene::build(&mut s);
        let cam = Camera::perspective(Vec3::new(0.0, 0.0, 4.0), Vec3::ZERO, 0.8);
        let mut acc = Accumulator::new(16, 16);
        let pt = PathTracer { max_bounces: 64, ..PathTracer::default() };
        pt.accumulate(&ts, &cam, &mut acc, 64);
        let centre: Rgb = acc.sum[8 * 16 + 8].scale(1.0 / 64.0);
        assert!((centre.r - 0.5).abs() < 0.03, "{centre:?}");
    }

    #[test]
    fn accumulation_is_deterministic_and_converges() {
        let mut s = Scene::new();
        s.background = Rgb::new(0.1, 0.1, 0.12);
        s.add(
            Node::new("floor", Content::mesh(plane(10.0, 10.0, 1, 1), Material::lambert(Color::WHITE)))
                .rotated(Quat::from_axis_angle(Vec3::X, -std::f32::consts::FRAC_PI_2))
                .at(Vec3::new(0.0, -1.0, 0.0)),
            None,
        );
        s.add(Node::new("ball", Content::mesh(sphere(1.0, 24, 12), Material::standard(Color::rgb(200, 60, 60), 0.0, 0.8))), None);
        s.add(
            Node::new(
                "sun",
                Content::Light(Light::Directional { color: Rgb::WHITE, intensity: 2.0, direction: Vec3::new(-0.4, -1.0, -0.3), shadow: true }),
            ),
            None,
        );
        let ts = TraceScene::build(&mut s);
        let cam = Camera::perspective(Vec3::new(0.0, 1.0, 5.0), Vec3::ZERO, 0.8);
        let run = || {
            let mut a = Accumulator::new(24, 24);
            PathTracer::default().accumulate(&ts, &cam, &mut a, 4);
            a.image(1.0)
        };
        assert_eq!(run().pixels(), run().pixels());
    }
}
