//! The software rasterizer: scene + camera → pixels, with a depth buffer.
//!
//! # The pipeline, stage by stage (Part 4's "code to pixels")
//!
//! 1. **Transforms** — dirty world matrices recomputed ([`Scene::update_world`]).
//! 2. **Culling** — each object's bounding sphere is tested against the six
//!    frustum planes extracted from the view-projection matrix
//!    (Gribb–Hartmann); a LOD group picks its level by distance here.
//! 3. **Shadow pass** — the first shadow-casting directional light renders
//!    caster depth into an orthographic shadow map fitted to the casters.
//! 4. **Geometry** — vertices to world (normals by the inverse-transpose)
//!    and clip space; triangles are clipped against the near plane in
//!    homogeneous space, so geometry behind the camera never divides by a
//!    negative w; back faces are culled by screen-space winding.
//! 5. **Rasterization** — edge functions over the triangle's bounding box,
//!    a per-sample depth test, and **perspective-correct** interpolation of
//!    world position, normal and UV.
//! 6. **Shading** — per sample: Basic, Lambert, Blinn–Phong, a GGX
//!    metallic–roughness model, Toon or Normal; ambient, hemisphere,
//!    directional (with 3×3 PCF shadows), point and spot lights; texture
//!    maps; emission; linear fog.
//! 7. **Transparency** — opaque objects first, then transparent ones sorted
//!    back to front and blended.
//! 8. **Resolve** — supersamples averaged (`samples`² per pixel, the same
//!    antialiasing idea as the 2D rasterizer's 4×4), linear light converted
//!    to sRGB.
//!
//! Everything is computed in linear light and every stage is counted in
//! [`RenderStats`], the 3D counterpart of the 2D `SceneReport`.

use std::sync::Arc;

use vieww_foundation::Image;
use vieww_mesh::Mesh;

use crate::math::{Mat4, Vec3};
use crate::scene::{Camera, Content, Light, Material, Rgb, Scene, Shading};

/// What a render did — every stage counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RenderStats {
    /// Drawable objects (instances counted individually) considered.
    pub objects: usize,
    /// Of those, rejected by the frustum test.
    pub culled_objects: usize,
    /// Triangles submitted after culling.
    pub triangles: usize,
    /// Triangles dropped as back faces.
    pub backfaces: usize,
    /// Triangles cut by the near plane.
    pub near_clipped: usize,
    /// Samples that passed the depth test and were shaded.
    pub fragments: usize,
    /// Triangles drawn into the shadow map.
    pub shadow_triangles: usize,
    /// Objects drawn in the transparent pass.
    pub transparent: usize,
    /// Meshlets considered across clustered meshes.
    pub clusters: usize,
    /// Meshlets rejected by the frustum or the normal cone.
    pub clusters_culled: usize,
    /// Point-cloud splats drawn.
    pub points: usize,
    /// Vertices deformed by skinning or morphing this frame.
    pub skinned_vertices: usize,
    /// Fragments that ran a custom shader.
    pub shaded_custom: usize,
}

/// Renders scenes. Configure, then call [`render`](Self::render).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Renderer {
    pub width: u32,
    pub height: u32,
    /// Supersampling factor per axis (1..=4).
    pub samples: u32,
    /// Shadow map resolution (square); 0 disables shadows.
    pub shadow_size: u32,
    /// Multiplies every light (a camera's exposure).
    pub exposure: f32,
    /// Seconds, handed to custom fragment shaders.
    pub time: f32,
}

impl Renderer {
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            samples: 2,
            shadow_size: 1024,
            exposure: 1.0,
            time: 0.0,
        }
    }

    /// The time custom shaders see.
    #[must_use]
    pub const fn at_time(mut self, seconds: f32) -> Self {
        self.time = seconds;
        self
    }

    #[must_use]
    pub const fn samples(mut self, s: u32) -> Self {
        self.samples = s;
        self
    }
}

#[derive(Clone, Copy)]
struct VertexOut {
    clip: [f32; 4],
    world: Vec3,
    normal: Vec3,
    uv: [f32; 2],
}

fn lerp_v(a: &VertexOut, b: &VertexOut, t: f32) -> VertexOut {
    let mut clip = [0.0; 4];
    for (k, c) in clip.iter_mut().enumerate() {
        *c = a.clip[k] + (b.clip[k] - a.clip[k]) * t;
    }
    VertexOut {
        clip,
        world: a.world.lerp(b.world, t),
        normal: a.normal.lerp(b.normal, t),
        uv: [
            a.uv[0] + (b.uv[0] - a.uv[0]) * t,
            a.uv[1] + (b.uv[1] - a.uv[1]) * t,
        ],
    }
}

/// Clip a triangle against the near plane `z + w >= 0` (Sutherland–Hodgman).
fn clip_near(tri: [VertexOut; 3]) -> Vec<VertexOut> {
    let inside = |v: &VertexOut| v.clip[2] + v.clip[3] >= 1e-5;
    if tri.iter().all(inside) {
        return tri.to_vec();
    }
    let mut out = Vec::with_capacity(4);
    for i in 0..3 {
        let (a, b) = (&tri[i], &tri[(i + 1) % 3]);
        let (da, db) = (a.clip[2] + a.clip[3], b.clip[2] + b.clip[3]);
        if inside(a) {
            out.push(*a);
        }
        if inside(a) != inside(b) {
            let t = (da - 1e-5) / (da - db);
            out.push(lerp_v(a, b, t));
        }
    }
    out
}

struct Target {
    w: usize,
    h: usize,
    color: Vec<Rgb>,
    depth: Vec<f32>,
    /// View-space position and normal of the nearest opaque surface.
    position: Vec<Vec3>,
    normal: Vec<Vec3>,
}

/// A rendered frame before display encoding — the G-buffer post-processing
/// reads (`EffectComposer`'s render target, EEVEE's buffers).
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    /// Linear HDR radiance (not clamped).
    pub color: Vec<Rgb>,
    /// View-space position of the nearest opaque surface (`z < 0` in front
    /// of the camera); background pixels have `z = −∞`.
    pub position: Vec<Vec3>,
    /// View-space unit normal (zero on background).
    pub normal: Vec<Vec3>,
    /// The projection used, for screen-space effects that re-project.
    pub projection: Mat4,
}

impl Frame {
    /// Distance along the view axis (`−z`), `∞` for background.
    #[must_use]
    pub fn depth(&self, x: usize, y: usize) -> f32 {
        -self.position[y * self.width + x].z
    }

    /// sRGB-encode (clamped) into an image — what `Renderer::render` returns.
    #[must_use]
    pub fn to_image(&self) -> Image {
        let mut px = Vec::with_capacity(self.width * self.height * 4);
        for c in &self.color {
            px.extend_from_slice(&[srgb(c.r), srgb(c.g), srgb(c.b), 255]);
        }
        #[allow(clippy::cast_possible_truncation)]
        Image::from_rgba8(px, self.width as u32, self.height as u32)
    }
}

struct LightW {
    light: Light,
    position: Vec3,
    direction: Vec3,
}

struct ShadowMap {
    vp: Mat4,
    size: usize,
    depth: Vec<f32>,
}

impl ShadowMap {
    /// Fraction of light reaching `p` (1 = lit), 3×3 PCF.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    fn visibility(&self, p: Vec3, bias: f32) -> f32 {
        let c = self.vp.mul_vec4([p.x, p.y, p.z, 1.0]);
        let (x, y, z) = (c[0], c[1], c[2]);
        if !(-1.0..=1.0).contains(&x) || !(-1.0..=1.0).contains(&y) || z > 1.0 {
            return 1.0;
        }
        let s = self.size as f32;
        let px = (x * 0.5 + 0.5) * s;
        let py = (0.5 - y * 0.5) * s;
        let mut lit = 0.0;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let sx = (px as i64 + dx).clamp(0, self.size as i64 - 1) as usize;
                let sy = (py as i64 + dy).clamp(0, self.size as i64 - 1) as usize;
                if z - bias <= self.depth[sy * self.size + sx] {
                    lit += 1.0;
                }
            }
        }
        lit / 9.0
    }
}

struct Item {
    model: Mat4,
    mesh: Arc<Mesh>,
    material: Arc<Material>,
    view_depth: f32,
}

/// Six planes `(a, b, c, d)` with inside `a x + b y + c z + d >= 0`.
fn frustum(vp: &Mat4) -> [[f32; 4]; 6] {
    let row = |r: usize| [vp.m[0][r], vp.m[1][r], vp.m[2][r], vp.m[3][r]];
    let (r0, r1, r2, r3) = (row(0), row(1), row(2), row(3));
    let add = |a: [f32; 4], b: [f32; 4]| [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]];
    let sub = |a: [f32; 4], b: [f32; 4]| [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]];
    let mut planes = [
        add(r3, r0),
        sub(r3, r0),
        add(r3, r1),
        sub(r3, r1),
        add(r3, r2),
        sub(r3, r2),
    ];
    for p in &mut planes {
        let l = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt().max(1e-12);
        for v in p.iter_mut() {
            *v /= l;
        }
    }
    planes
}

fn sphere_visible(planes: &[[f32; 4]; 6], c: Vec3, r: f32) -> bool {
    planes
        .iter()
        .all(|p| p[0] * c.x + p[1] * c.y + p[2] * c.z + p[3] >= -r)
}

fn srgb(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let s = if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let b = (s * 255.0 + 0.5) as u8;
    b
}

impl Renderer {
    /// Render `scene` from `camera` into an RGBA8 image.
    #[must_use]
    pub fn render(&self, scene: &mut Scene, camera: &Camera) -> (Image, RenderStats) {
        let (frame, stats) = self.render_frame(scene, camera);
        (frame.to_image(), stats)
    }

    /// Render into a linear [`Frame`] (colour + view-space position and
    /// normal), for [`post`](crate::post) processing.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn render_frame(&self, scene: &mut Scene, camera: &Camera) -> (Frame, RenderStats) {
        let mut stats = RenderStats::default();
        scene.update_world();
        let s = self.samples.clamp(1, 4) as usize;
        let (w, h) = (self.width as usize * s, self.height as usize * s);
        #[allow(clippy::cast_precision_loss)]
        let aspect = self.width as f32 / self.height.max(1) as f32;
        let view = camera.view();
        let proj = camera.projection(aspect);
        let vp = proj * view;
        let planes = frustum(&vp);

        // Lights, in world space.
        let mut lights = Vec::new();
        for id in scene.traverse() {
            if !scene.effectively_visible(id) {
                continue;
            }
            let n = scene.node(id);
            if let Content::Light(l) = n.content {
                let world = n.world();
                let direction = match l {
                    Light::Directional { direction, .. } | Light::Spot { direction, .. } => {
                        world.transform_vector(direction).normalize()
                    }
                    _ => Vec3::ZERO,
                };
                lights.push(LightW {
                    light: l,
                    position: world.get_translation(),
                    direction,
                });
            }
        }

        // Drawables.
        let mut items = Vec::new();
        let mut clouds = Vec::new();
        for id in scene.traverse() {
            if !scene.effectively_visible(id) {
                continue;
            }
            let n = scene.node(id);
            let world = n.world();
            let mut push = |model: Mat4,
                            mesh: &Arc<Mesh>,
                            material: &Arc<Material>,
                            bounds: (Vec3, f32),
                            stats: &mut RenderStats| {
                stats.objects += 1;
                let c = model.transform_point(bounds.0);
                let r = bounds.1 * model.max_scale();
                if !sphere_visible(&planes, c, r) {
                    stats.culled_objects += 1;
                    return;
                }
                let vd = -view.transform_point(c).z;
                items.push(Item {
                    model,
                    mesh: mesh.clone(),
                    material: material.clone(),
                    view_depth: vd,
                });
            };
            match &n.content {
                Content::Mesh {
                    mesh,
                    material,
                    bounds,
                } => push(world, mesh, material, *bounds, &mut stats),
                Content::Instanced {
                    mesh,
                    material,
                    bounds,
                    instances,
                } => {
                    for inst in instances {
                        push(world * *inst, mesh, material, *bounds, &mut stats);
                    }
                }
                Content::Lod {
                    levels,
                    material,
                    bounds,
                } => {
                    let d = (world.transform_point(bounds.0) - camera.position).length();
                    if let Some((_, mesh)) = levels
                        .iter()
                        .find(|(max, _)| d <= *max)
                        .or_else(|| levels.last())
                    {
                        push(world, mesh, material, *bounds, &mut stats);
                    }
                }
                Content::Skinned {
                    skin,
                    material,
                    morph_weights,
                } => {
                    let bones: Vec<Mat4> =
                        skin.bones.iter().map(|b| scene.node(*b).world()).collect();
                    let jm = crate::skin::joint_matrices(&world, &bones, &skin.inverse_bind);
                    let mesh = crate::skin::deform(skin, &jm, morph_weights);
                    stats.skinned_vertices += mesh.positions.len();
                    let bounds = crate::geometry::bounding_sphere(&mesh);
                    push(world, &Arc::new(mesh), material, bounds, &mut stats);
                }
                Content::Clustered {
                    lods,
                    material,
                    bounds,
                    threshold_px,
                } => {
                    let c = world.transform_point(bounds.0);
                    let d = (c - camera.position).length();
                    let fov = match camera.projection {
                        crate::scene::Projection::Perspective { fov_y, .. } => fov_y,
                        crate::scene::Projection::Orthographic { .. } => 0.8,
                    };
                    #[allow(clippy::cast_precision_loss)]
                    let level =
                        lods.select(d, fov, self.height as f32, world.max_scale(), *threshold_px);
                    let (mesh, cs) = lods.gather(level, &world, camera.position, &planes);
                    stats.clusters += cs.clusters;
                    stats.clusters_culled += cs.frustum_culled + cs.cone_culled;
                    if !mesh.indices.is_empty() {
                        push(world, &Arc::new(mesh), material, *bounds, &mut stats);
                    }
                }
                Content::Points { .. } => {
                    clouds.push(id);
                }
                Content::Empty | Content::Light(_) => {}
            }
        }

        // Shadow map for the first shadow-casting directional light.
        let shadow = self.shadow_pass(&lights, &items, &mut stats);

        let mut target = Target {
            w,
            h,
            color: vec![scene.background; w * h],
            depth: vec![f32::INFINITY; w * h],
            position: vec![Vec3::new(0.0, 0.0, f32::NEG_INFINITY); w * h],
            normal: vec![Vec3::ZERO; w * h],
        };
        let (opaque, mut transparent): (Vec<&Item>, Vec<&Item>) =
            items.iter().partition(|i| i.material.opacity >= 1.0);
        transparent.sort_by(|a, b| b.view_depth.total_cmp(&a.view_depth));
        stats.transparent = transparent.len();
        let ctx = ShadeCtx {
            lights: &lights,
            shadow: shadow.as_ref(),
            eye: camera.position,
            fog: scene.fog,
            view,
            exposure: self.exposure,
            time: self.time,
        };
        for item in &opaque {
            draw_item(item, &vp, &mut target, &ctx, &mut stats);
        }
        for id in clouds {
            let n = scene.node(id);
            if let Content::Points {
                points,
                colors,
                size,
                ..
            } = &n.content
            {
                draw_points(
                    &n.world(),
                    points,
                    colors,
                    *size,
                    &view,
                    &proj,
                    &mut target,
                    &mut stats,
                );
            }
        }
        for item in &transparent {
            draw_item(item, &vp, &mut target, &ctx, &mut stats);
        }

        // Resolve.
        let (ow, oh) = (self.width as usize, self.height as usize);
        let mut color = Vec::with_capacity(ow * oh);
        let mut position = Vec::with_capacity(ow * oh);
        let mut normal = Vec::with_capacity(ow * oh);
        #[allow(clippy::cast_precision_loss)]
        let norm = 1.0 / (s * s) as f32;
        for y in 0..oh {
            for x in 0..ow {
                let mut acc = Rgb::BLACK;
                let mut best = (y * s) * w + x * s;
                for sy in 0..s {
                    for sx in 0..s {
                        let k = (y * s + sy) * w + x * s + sx;
                        acc = acc.add(target.color[k]);
                        // Geometry: the nearest sample of the block (no
                        // averaging across silhouettes).
                        if target.position[k].z > target.position[best].z {
                            best = k;
                        }
                    }
                }
                color.push(acc.scale(norm));
                position.push(target.position[best]);
                normal.push(target.normal[best]);
            }
        }
        (
            Frame {
                width: ow,
                height: oh,
                color,
                position,
                normal,
                projection: proj,
            },
            stats,
        )
    }

    fn shadow_pass(
        &self,
        lights: &[LightW],
        items: &[Item],
        stats: &mut RenderStats,
    ) -> Option<ShadowMap> {
        if self.shadow_size == 0 {
            return None;
        }
        let light = lights
            .iter()
            .find(|l| matches!(l.light, Light::Directional { shadow: true, .. }))?;
        let casters: Vec<&Item> = items.iter().filter(|i| i.material.cast_shadow).collect();
        // Fit to all drawables so receivers are covered too.
        let mut lo = Vec3::splat(f32::INFINITY);
        let mut hi = Vec3::splat(f32::NEG_INFINITY);
        for it in items {
            for p in &it.mesh.positions {
                let q = it.model.transform_point(Vec3::from_array(*p));
                lo = lo.min(q);
                hi = hi.max(q);
            }
        }
        if lo.x > hi.x {
            return None;
        }
        let center = (lo + hi) * 0.5;
        let radius = ((hi - lo).length() * 0.5).max(1e-3);
        let dir = light.direction.normalize();
        let eye = center - dir * (radius * 2.0);
        let up = if dir.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
        let lv = Mat4::look_at(eye, center, up);
        let lp = Mat4::orthographic(-radius, radius, -radius, radius, 0.01, radius * 4.0);
        let vp = lp * lv;
        let size = self.shadow_size as usize;
        let mut depth = vec![f32::INFINITY; size * size];
        for it in casters {
            let m = &it.mesh;
            let clip: Vec<[f32; 4]> = m
                .positions
                .iter()
                .map(|p| {
                    let wpos = it.model.transform_point(Vec3::from_array(*p));
                    vp.mul_vec4([wpos.x, wpos.y, wpos.z, 1.0])
                })
                .collect();
            for t in m.indices.as_chunks::<3>().0 {
                let v = [
                    clip[t[0] as usize],
                    clip[t[1] as usize],
                    clip[t[2] as usize],
                ];
                stats.shadow_triangles += 1;
                #[allow(clippy::cast_precision_loss)]
                let sp = v.map(|c| {
                    [
                        (c[0] * 0.5 + 0.5) * size as f32,
                        (0.5 - c[1] * 0.5) * size as f32,
                        c[2],
                    ]
                });
                raster_depth(&sp, size, &mut depth);
            }
        }
        Some(ShadowMap { vp, size, depth })
    }
}

/// Depth-only rasterization for the shadow map (both faces).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn raster_depth(v: &[[f32; 3]; 3], size: usize, depth: &mut [f32]) {
    let area =
        (v[1][0] - v[0][0]) * (v[2][1] - v[0][1]) - (v[2][0] - v[0][0]) * (v[1][1] - v[0][1]);
    if area.abs() < 1e-12 {
        return;
    }
    let minx = v
        .iter()
        .map(|p| p[0])
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0) as usize;
    let maxx = (v
        .iter()
        .map(|p| p[0])
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil() as i64)
        .min(size as i64 - 1);
    let miny = v
        .iter()
        .map(|p| p[1])
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0) as usize;
    let maxy = (v
        .iter()
        .map(|p| p[1])
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil() as i64)
        .min(size as i64 - 1);
    if maxx < 0 || maxy < 0 {
        return;
    }
    for y in miny..=maxy as usize {
        for x in minx..=maxx as usize {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let e = |a: [f32; 3], b: [f32; 3]| {
                (b[0] - a[0]) * (py - a[1]) - (b[1] - a[1]) * (px - a[0])
            };
            let (w0, w1, w2) = (
                e(v[1], v[2]) / area,
                e(v[2], v[0]) / area,
                e(v[0], v[1]) / area,
            );
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }
            let z = w0 * v[0][2] + w1 * v[1][2] + w2 * v[2][2];
            let d = &mut depth[y * size + x];
            if z < *d {
                *d = z;
            }
        }
    }
}

struct ShadeCtx<'a> {
    lights: &'a [LightW],
    shadow: Option<&'a ShadowMap>,
    eye: Vec3,
    fog: Option<(Rgb, f32, f32)>,
    view: Mat4,
    exposure: f32,
    time: f32,
}

#[allow(
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn draw_item(
    item: &Item,
    vp: &Mat4,
    target: &mut Target,
    ctx: &ShadeCtx<'_>,
    stats: &mut RenderStats,
) {
    let mesh = &item.mesh;
    let mat = &item.material;
    let normal_m = item.model.inverse().map_or(item.model, |m| m.transpose());
    let has_n = mesh.has_normals();
    let has_uv = mesh.has_uvs();
    let verts: Vec<VertexOut> = (0..mesh.positions.len())
        .map(|i| {
            let world = item
                .model
                .transform_point(Vec3::from_array(mesh.positions[i]));
            let normal = if has_n {
                normal_m
                    .transform_vector(Vec3::from_array(mesh.normals[i]))
                    .normalize()
            } else {
                Vec3::ZERO
            };
            VertexOut {
                clip: vp.mul_vec4([world.x, world.y, world.z, 1.0]),
                world,
                normal,
                uv: if has_uv { mesh.uvs[i] } else { [0.0, 0.0] },
            }
        })
        .collect();
    let (w, h) = (target.w as f32, target.h as f32);
    for t in mesh.indices.as_chunks::<3>().0 {
        stats.triangles += 1;
        let tri = [
            verts[t[0] as usize],
            verts[t[1] as usize],
            verts[t[2] as usize],
        ];
        let face_n = (tri[1].world - tri[0].world)
            .cross(tri[2].world - tri[0].world)
            .normalize();
        let poly = clip_near(tri);
        if poly.len() < 3 {
            continue;
        }
        if poly.len() != 3 {
            stats.near_clipped += 1;
        }
        // Fan-triangulate the clipped polygon.
        for k in 1..poly.len() - 1 {
            let v = [poly[0], poly[k], poly[k + 1]];
            let sp: [[f32; 3]; 3] = v.map(|p| {
                let iw = 1.0 / p.clip[3];
                [
                    (p.clip[0] * iw * 0.5 + 0.5) * w,
                    (0.5 - p.clip[1] * iw * 0.5) * h,
                    p.clip[2] * iw,
                ]
            });
            let area = (sp[1][0] - sp[0][0]) * (sp[2][1] - sp[0][1])
                - (sp[2][0] - sp[0][0]) * (sp[1][1] - sp[0][1]);
            // Screen Y is down, so a counter-clockwise front face has
            // negative area here.
            let back = area > 0.0;
            if back && !mat.double_sided {
                stats.backfaces += 1;
                continue;
            }
            if area.abs() < 1e-12 {
                continue;
            }
            if mat.wireframe {
                for e in 0..3 {
                    draw_line(
                        sp[e],
                        sp[(e + 1) % 3],
                        target,
                        mat.color.add(mat.emissive),
                        stats,
                    );
                }
                continue;
            }
            let inv_w = [1.0 / v[0].clip[3], 1.0 / v[1].clip[3], 1.0 / v[2].clip[3]];
            let minx = sp
                .iter()
                .map(|p| p[0])
                .fold(f32::INFINITY, f32::min)
                .floor()
                .max(0.0) as usize;
            let maxx = (sp
                .iter()
                .map(|p| p[0])
                .fold(f32::NEG_INFINITY, f32::max)
                .ceil())
            .min(w - 1.0);
            let miny = sp
                .iter()
                .map(|p| p[1])
                .fold(f32::INFINITY, f32::min)
                .floor()
                .max(0.0) as usize;
            let maxy = (sp
                .iter()
                .map(|p| p[1])
                .fold(f32::NEG_INFINITY, f32::max)
                .ceil())
            .min(h - 1.0);
            if maxx < 0.0 || maxy < 0.0 {
                continue;
            }
            for y in miny..=maxy as usize {
                let py = y as f32 + 0.5;
                for x in minx..=maxx as usize {
                    let px = x as f32 + 0.5;
                    let e = |a: [f32; 3], b: [f32; 3]| {
                        (b[0] - a[0]) * (py - a[1]) - (b[1] - a[1]) * (px - a[0])
                    };
                    let (b0, b1, b2) = (
                        e(sp[1], sp[2]) / area,
                        e(sp[2], sp[0]) / area,
                        e(sp[0], sp[1]) / area,
                    );
                    if b0 < 0.0 || b1 < 0.0 || b2 < 0.0 {
                        continue;
                    }
                    let z = b0 * sp[0][2] + b1 * sp[1][2] + b2 * sp[2][2];
                    if !(-1.0..=1.0).contains(&z) {
                        continue;
                    }
                    let idx = y * target.w + x;
                    if z >= target.depth[idx] {
                        continue;
                    }
                    // Perspective-correct weights.
                    let (p0, p1, p2) = (b0 * inv_w[0], b1 * inv_w[1], b2 * inv_w[2]);
                    let s = 1.0 / (p0 + p1 + p2);
                    let (q0, q1, q2) = (p0 * s, p1 * s, p2 * s);
                    let world = v[0].world * q0 + v[1].world * q1 + v[2].world * q2;
                    let mut n = if mat.flat || !has_n {
                        face_n
                    } else {
                        (v[0].normal * q0 + v[1].normal * q1 + v[2].normal * q2).normalize()
                    };
                    if back {
                        n = -n;
                    }
                    let uv = [
                        v[0].uv[0] * q0 + v[1].uv[0] * q1 + v[2].uv[0] * q2,
                        v[0].uv[1] * q0 + v[1].uv[1] * q1 + v[2].uv[1] * q2,
                    ];
                    let (mut color, mut alpha) = shade(mat, world, n, uv, ctx);
                    if let Some(prog) = &mat.shader {
                        let mut albedo = mat.color;
                        if let Some(t) = &mat.map {
                            let s = t.sample(uv[0], uv[1]);
                            albedo = albedo.mul(Rgb::new(s[0], s[1], s[2]));
                        }
                        let out = prog.run(&crate::shader::Fragment {
                            world,
                            normal: n,
                            uv,
                            view: (ctx.eye - world).normalize(),
                            screen: [px, py],
                            time: ctx.time,
                            lit: color,
                            albedo,
                        });
                        color = Rgb::new(out[0], out[1], out[2]);
                        alpha *= out[3];
                        stats.shaded_custom += 1;
                    }
                    stats.fragments += 1;
                    let a = alpha * mat.opacity;
                    if a >= 1.0 {
                        target.color[idx] = color;
                        target.depth[idx] = z;
                        target.position[idx] = ctx.view.transform_point(world);
                        target.normal[idx] = ctx.view.transform_vector(n).normalize();
                    } else {
                        target.color[idx] = target.color[idx].lerp(color, a.clamp(0.0, 1.0));
                        // Transparent surfaces do not write depth.
                    }
                }
            }
        }
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn draw_line(a: [f32; 3], b: [f32; 3], target: &mut Target, color: Rgb, stats: &mut RenderStats) {
    let steps = (b[0] - a[0]).abs().max((b[1] - a[1]).abs()).ceil().max(1.0) as usize;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let (x, y, z) = (
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
        );
        if x < 0.0
            || y < 0.0
            || x >= target.w as f32
            || y >= target.h as f32
            || !(-1.0..=1.0).contains(&z)
        {
            continue;
        }
        let idx = y as usize * target.w + x as usize;
        if z - 1e-4 <= target.depth[idx] {
            target.color[idx] = color;
            target.depth[idx] = z - 1e-4;
            stats.fragments += 1;
        }
    }
}

/// Round, depth-tested splats of world `size`, perspective-scaled.
#[allow(
    clippy::too_many_arguments,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap
)]
fn draw_points(
    model: &Mat4,
    points: &[Vec3],
    colors: &[Rgb],
    size: f32,
    view: &Mat4,
    proj: &Mat4,
    target: &mut Target,
    stats: &mut RenderStats,
) {
    let (w, h) = (target.w as f32, target.h as f32);
    let scale = model.max_scale();
    for (i, p) in points.iter().enumerate() {
        let vpos = view.transform_point(model.transform_point(*p));
        if vpos.z >= -1e-4 {
            continue;
        }
        let c = proj.mul_vec4([vpos.x, vpos.y, vpos.z, 1.0]);
        let iw = 1.0 / c[3];
        let (sx, sy, z) = (
            (c[0] * iw * 0.5 + 0.5) * w,
            (0.5 - c[1] * iw * 0.5) * h,
            c[2] * iw,
        );
        if !(-1.0..=1.0).contains(&z) {
            continue;
        }
        // Radius in pixels: project a view-space offset of `size`.
        let edge = proj.mul_vec4([vpos.x + size * scale, vpos.y, vpos.z, 1.0]);
        let r = ((edge[0] / edge[3] - c[0] * iw).abs() * 0.5 * w).max(0.5);
        let color = colors
            .get(i)
            .or_else(|| colors.last())
            .copied()
            .unwrap_or(Rgb::WHITE);
        let (x0, x1) = (
            (sx - r).floor().max(0.0) as i64,
            (sx + r).ceil().min(w - 1.0) as i64,
        );
        let (y0, y1) = (
            (sy - r).floor().max(0.0) as i64,
            (sy + r).ceil().min(h - 1.0) as i64,
        );
        let mut drew = false;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (dx, dy) = (x as f32 + 0.5 - sx, y as f32 + 0.5 - sy);
                if dx * dx + dy * dy > r * r {
                    continue;
                }
                let idx = y as usize * target.w + x as usize;
                if z < target.depth[idx] {
                    target.depth[idx] = z;
                    target.color[idx] = color;
                    target.position[idx] = vpos;
                    target.normal[idx] = Vec3::Z;
                    stats.fragments += 1;
                    drew = true;
                }
            }
        }
        if drew {
            stats.points += 1;
        }
    }
}

fn saturate(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

#[allow(clippy::cast_precision_loss)]
fn shade(mat: &Material, p: Vec3, n: Vec3, uv: [f32; 2], ctx: &ShadeCtx<'_>) -> (Rgb, f32) {
    let (mut albedo, mut alpha) = (mat.color, 1.0);
    if let Some(t) = &mat.map {
        let s = t.sample(uv[0], uv[1]);
        albedo = albedo.mul(Rgb::new(s[0], s[1], s[2]));
        alpha = s[3];
    }
    let out = match mat.shading {
        Shading::Basic => albedo,
        Shading::Normal => {
            let vn = ctx.view.transform_vector(n).normalize();
            Rgb::new(vn.x * 0.5 + 0.5, vn.y * 0.5 + 0.5, vn.z * 0.5 + 0.5)
        }
        _ => {
            let v = (ctx.eye - p).normalize();
            let mut total = Rgb::BLACK;
            for l in ctx.lights {
                let (radiance, dir, shadowed) = match l.light {
                    Light::Ambient { color, intensity } => {
                        total = total.add(albedo.mul(color).scale(intensity * ctx.exposure));
                        continue;
                    }
                    Light::Hemisphere {
                        sky,
                        ground,
                        intensity,
                    } => {
                        let k = 0.5 * (n.y + 1.0);
                        total = total.add(
                            albedo
                                .mul(ground.lerp(sky, k))
                                .scale(intensity * ctx.exposure),
                        );
                        continue;
                    }
                    Light::Directional {
                        color,
                        intensity,
                        shadow,
                        ..
                    } => {
                        let vis = match (shadow, ctx.shadow) {
                            (true, Some(sm)) if mat.receive_shadow => {
                                let ndl = n.dot(-l.direction).max(0.0);
                                sm.visibility(p, 0.002 + 0.01 * (1.0 - ndl))
                            }
                            _ => 1.0,
                        };
                        (color.scale(intensity), -l.direction, vis)
                    }
                    Light::Point {
                        color,
                        intensity,
                        range,
                    } => {
                        let d = l.position - p;
                        let dist = d.length();
                        let fall = if range > 0.0 {
                            saturate(1.0 - (dist / range).powi(4)).powi(2)
                        } else {
                            1.0
                        };
                        (
                            color.scale(intensity * fall / (1.0 + dist * dist * 0.02)),
                            d.normalize(),
                            1.0,
                        )
                    }
                    Light::Spot {
                        color,
                        intensity,
                        range,
                        direction: _,
                        angle,
                        penumbra,
                    } => {
                        let d = l.position - p;
                        let dist = d.length();
                        let ld = d.normalize();
                        let cos = (-ld).dot(l.direction);
                        let outer = angle.cos();
                        let inner = (angle * (1.0 - penumbra)).cos();
                        let cone = saturate((cos - outer) / (inner - outer).max(1e-4));
                        let fall = if range > 0.0 {
                            saturate(1.0 - (dist / range).powi(4)).powi(2)
                        } else {
                            1.0
                        };
                        (color.scale(intensity * fall * cone * cone), ld, 1.0)
                    }
                };
                let ndl = n.dot(dir).max(0.0);
                if ndl <= 0.0 || shadowed <= 0.0 {
                    continue;
                }
                let radiance = radiance.scale(ctx.exposure * shadowed);
                let contribution = match mat.shading {
                    Shading::Lambert => albedo.scale(ndl),
                    Shading::Toon { bands } => {
                        let b = bands.max(1) as f32;
                        albedo.scale(((ndl * b).ceil() / b).min(1.0))
                    }
                    Shading::Phong {
                        shininess,
                        specular,
                    } => {
                        let hv = (dir + v).normalize();
                        let spec = n.dot(hv).max(0.0).powf(shininess.max(1.0)) * specular;
                        albedo.scale(ndl).add(Rgb::WHITE.scale(spec))
                    }
                    Shading::Standard {
                        metallic,
                        roughness,
                    } => {
                        let hv = (dir + v).normalize();
                        let ndh = n.dot(hv).max(0.0);
                        let ndv = n.dot(v).max(1e-4);
                        let a = (roughness * roughness).max(0.002);
                        let a2 = a * a;
                        let dd = ndh * ndh * (a2 - 1.0) + 1.0;
                        let dist = a2 / (std::f32::consts::PI * dd * dd);
                        let k = (roughness + 1.0).powi(2) / 8.0;
                        let g = (ndv / (ndv * (1.0 - k) + k)) * (ndl / (ndl * (1.0 - k) + k));
                        let f0 = Rgb::new(0.04, 0.04, 0.04).lerp(albedo, metallic);
                        let fr = (1.0 - v.dot(hv).max(0.0)).powi(5);
                        let f = f0.add(Rgb::WHITE.add(f0.scale(-1.0)).scale(fr));
                        let spec =
                            f.scale(dist * g / (4.0 * ndl * ndv) * std::f32::consts::PI * ndl);
                        let kd = (1.0 - metallic) * (1.0 - f.luminance());
                        albedo.scale(kd * ndl).add(spec)
                    }
                    Shading::Basic | Shading::Normal => albedo,
                };
                total = total.add(radiance.mul(contribution));
            }
            total
        }
    };
    let mut out = out.add(mat.emissive);
    if let Some((fog, near, far)) = ctx.fog {
        let d = (p - ctx.eye).length();
        let f = saturate((d - near) / (far - near).max(1e-4));
        out = out.lerp(fog, f);
    }
    (out, alpha)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{box_mesh, plane, sphere};
    use crate::math::Quat;
    use crate::scene::{Node, Rgb};
    use vieww_foundation::Color;

    fn lit_scene() -> Scene {
        let mut s = Scene::new();
        s.background = Rgb::BLACK;
        s.add(
            Node::new(
                "amb",
                Content::Light(Light::Ambient {
                    color: Rgb::WHITE,
                    intensity: 0.1,
                }),
            ),
            None,
        );
        s.add(
            Node::new(
                "sun",
                Content::Light(Light::Directional {
                    color: Rgb::WHITE,
                    intensity: 1.0,
                    direction: Vec3::new(-0.3, -1.0, -0.5),
                    shadow: true,
                }),
            ),
            None,
        );
        s
    }

    fn pixel(img: &Image, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * img.width() + x) * 4) as usize;
        let p = img.pixels();
        [p[i], p[i + 1], p[i + 2], p[i + 3]]
    }

    #[test]
    fn a_red_box_in_front_of_the_camera_is_red_in_the_middle() {
        let mut s = lit_scene();
        s.add(
            Node::new(
                "box",
                Content::mesh(box_mesh(1.0, 1.0, 1.0), Material::basic(Color::RED)),
            ),
            None,
        );
        let cam = Camera::perspective(Vec3::new(0.0, 0.0, 4.0), Vec3::ZERO, 0.8);
        let (img, stats) = Renderer::new(64, 64).samples(1).render(&mut s, &cam);
        assert_eq!(pixel(&img, 32, 32), [255, 0, 0, 255]);
        assert_eq!(pixel(&img, 1, 1), [0, 0, 0, 255], "background");
        assert_eq!(stats.triangles, 12);
        assert_eq!(
            stats.backfaces, 10,
            "all but the front face point away or are edge-on"
        );
    }

    #[test]
    fn depth_testing_keeps_the_nearer_surface() {
        let mut s = lit_scene();
        s.add(
            Node::new(
                "far",
                Content::mesh(plane(4.0, 4.0, 1, 1), Material::basic(Color::BLUE)),
            )
            .at(Vec3::new(0.0, 0.0, -1.0)),
            None,
        );
        s.add(
            Node::new(
                "near",
                Content::mesh(plane(1.0, 1.0, 1, 1), Material::basic(Color::GREEN)),
            ),
            None,
        );
        let cam = Camera::perspective(Vec3::new(0.0, 0.0, 3.0), Vec3::ZERO, 0.8);
        let (img, _) = Renderer::new(40, 40).samples(1).render(&mut s, &cam);
        assert_eq!(pixel(&img, 20, 20), [0, 255, 0, 255]);
        assert_eq!(pixel(&img, 5, 20), [0, 0, 255, 255]);
    }

    #[test]
    fn objects_outside_the_frustum_are_culled() {
        let mut s = lit_scene();
        s.add(
            Node::new(
                "in",
                Content::mesh(box_mesh(1.0, 1.0, 1.0), Material::lambert(Color::WHITE)),
            ),
            None,
        );
        s.add(
            Node::new(
                "behind",
                Content::mesh(box_mesh(1.0, 1.0, 1.0), Material::lambert(Color::WHITE)),
            )
            .at(Vec3::new(0.0, 0.0, 20.0)),
            None,
        );
        s.add(
            Node::new(
                "left",
                Content::mesh(box_mesh(1.0, 1.0, 1.0), Material::lambert(Color::WHITE)),
            )
            .at(Vec3::new(-50.0, 0.0, 0.0)),
            None,
        );
        let cam = Camera::perspective(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, 0.8);
        let (_, stats) = Renderer::new(32, 32).samples(1).render(&mut s, &cam);
        assert_eq!((stats.objects, stats.culled_objects), (3, 2));
    }

    #[test]
    fn lambert_is_brighter_facing_the_light() {
        let mut s = lit_scene();
        s.add(
            Node::new(
                "ball",
                Content::mesh(sphere(1.0, 32, 16), Material::lambert(Color::WHITE)),
            ),
            None,
        );
        let cam = Camera::perspective(Vec3::new(0.0, 0.0, 4.0), Vec3::ZERO, 0.8);
        let (img, _) = Renderer::new(64, 64).samples(1).render(&mut s, &cam);
        let top = pixel(&img, 32, 18)[0];
        let bottom = pixel(&img, 32, 46)[0];
        assert!(
            top > bottom + 40,
            "lit from above: top {top} bottom {bottom}"
        );
    }

    #[test]
    fn shadows_darken_the_ground_under_a_caster() {
        let mut s = lit_scene();
        let ground = Node::new(
            "ground",
            Content::mesh(plane(8.0, 8.0, 1, 1), Material::lambert(Color::WHITE)),
        )
        .rotated(Quat::from_axis_angle(Vec3::X, -std::f32::consts::FRAC_PI_2))
        .at(Vec3::new(0.0, -1.0, 0.0));
        s.add(ground, None);
        s.add(
            Node::new(
                "box",
                Content::mesh(box_mesh(1.0, 1.0, 1.0), Material::lambert(Color::RED)),
            )
            .at(Vec3::new(0.0, 0.5, 0.0)),
            None,
        );
        let cam = Camera::perspective(Vec3::new(0.0, 6.0, 0.01), Vec3::ZERO, 1.2);
        let r = Renderer::new(64, 64).samples(1);
        let (with, stats) = r.render(&mut s, &cam);
        assert!(stats.shadow_triangles > 0);
        let mut off = r;
        off.shadow_size = 0;
        let (without, _) = off.render(&mut s, &cam);
        // Somewhere on the ground the shadow makes it darker than unshadowed.
        let darker = (0..64u32)
            .flat_map(|y| (0..64u32).map(move |x| (x, y)))
            .any(|(x, y)| {
                let a = pixel(&with, x, y);
                let b = pixel(&without, x, y);
                u16::from(a[1]) + 30 < u16::from(b[1])
            });
        assert!(darker);
    }

    #[test]
    fn the_near_plane_clips_rather_than_inverting() {
        let mut s = lit_scene();
        s.add(
            Node::new(
                "wall",
                Content::mesh(
                    plane(20.0, 20.0, 1, 1),
                    Material::basic(Color::WHITE).double_sided(),
                ),
            )
            .rotated(Quat::from_axis_angle(
                Vec3::Y,
                std::f32::consts::FRAC_PI_2 * 0.9,
            )),
            None,
        );
        let cam = Camera::perspective(Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, 0.0, -1.0), 1.2);
        let (_, stats) = Renderer::new(32, 32).samples(1).render(&mut s, &cam);
        assert!(stats.near_clipped > 0);
        assert!(stats.fragments > 0);
    }

    #[test]
    fn instances_and_lods_count_and_choose() {
        let mut s = lit_scene();
        let inst: Vec<Mat4> = (0..5)
            .map(|i| Mat4::translation(Vec3::new(i as f32 * 2.0 - 4.0, 0.0, 0.0)))
            .collect();
        s.add(
            Node::new(
                "many",
                Content::instanced(
                    box_mesh(0.5, 0.5, 0.5),
                    Material::lambert(Color::WHITE),
                    inst,
                ),
            ),
            None,
        );
        let lod = Content::lod(
            vec![(5.0, sphere(1.0, 32, 16)), (1e9, box_mesh(1.0, 1.0, 1.0))],
            Material::lambert(Color::WHITE),
        );
        s.add(Node::new("lod", lod).at(Vec3::new(0.0, 0.0, -20.0)), None);
        let cam = Camera::perspective(Vec3::new(0.0, 0.0, 8.0), Vec3::ZERO, 1.0);
        let (_, stats) = Renderer::new(32, 32).samples(1).render(&mut s, &cam);
        assert_eq!(stats.objects, 6);
        assert_eq!(stats.triangles, 5 * 12 + 12, "the far LOD level is the box");
    }
}
