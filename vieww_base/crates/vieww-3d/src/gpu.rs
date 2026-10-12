//! The GPU seam — a `GpuRenderer` behind the CPU renderer's
//! `(scene, camera, settings) → RGBA8` contract.
//!
//! # Why this crate needed a seam at all
//!
//! The software rasterizer is this crate's reference implementation, and the
//! promise that keeps it honest is that a 3D render is a pure function,
//! deterministic to the byte, testable without a GPU. But "the CPU is the
//! oracle" was always meant to be the first half of a sentence whose second
//! half is "and a GPU backend is held to its pixels" — `lib.rs` has said so
//! since the rasterizer landed. This module is that second half beginning to
//! exist: [`GpuRenderer`] takes the same inputs as
//! [`Renderer`](crate::render::Renderer), returns the same `Image`, and on a
//! machine with a Vulkan loader its pixels come from
//! [`vieww_hal::vulkan::VulkanDevice`] instead of the CPU loops.
//!
//! # The reach, and the fall
//!
//! Two passes, chosen per frame:
//!
//! - **Unlit** — every visible material `Shading::Basic`: flat base colour
//!   plus emissive, one depth-tested draw (`tests/gpu_unlit.rs`).
//! - **Lit** — any `Shading::Lambert` material in the frame: Lambert (and
//!   Basic, in the same draw) under ambient, hemisphere and directional
//!   lights, with the first shadow-casting directional light's shadow map
//!   (`tests/gpu_lit.rs`). The shadow map is the CPU renderer's own
//!   `shadow_pass` output, uploaded — so the two renderers make the same
//!   shadow decision per point — and back faces are decided per triangle
//!   here while packing, exactly as the CPU rasterizer culls or flips them.
//!
//! Both are held to the CPU oracle: interiors match, disagreements hug
//! edges. Everything outside the reach — Phong/Standard/Toon/Normal shading,
//! textures, transparency, wireframe, custom shaders, point and spot lights,
//! image-based lighting and GI probes, fog, skyboxes, point clouds, more than
//! eight lights — **falls back to the CPU renderer for the whole frame**, so
//! `render` never returns anything the CPU would not have returned. The
//! fallback is per frame, not per object: mixing half a GPU frame with half a
//! CPU one would be neither oracle-clean nor cheaper to reason about.
//!
//! The reach grows the way the 2D path's did: Phong and point/spot lights,
//! textures, then the transparent pass, each with its own parity assertions.
//!
//! # What the stats mean here
//!
//! `RenderStats` reports what the pass actually knows: objects considered
//! and frustum-culled are counted on the CPU during packing, triangles
//! submitted is the index list's length. The counters that live inside
//! rasterization — fragments, backfaces, near-clips — stay zero: the GPU
//! does not report them, and inventing numbers would make the stats lie.
//! Compare against the CPU renderer when a rasterizer-level count matters.

use std::sync::Arc;

use vieww_foundation::Image;

use crate::math::{Mat4, Vec3};
use crate::render::{frustum, sphere_visible, world_lights, Item, RenderStats, Renderer};
use crate::scene::{Camera, Content, Light, Material, Projection, Scene, Shading};

/// A `(scene, camera) → Image` renderer whose pixels come from Vulkan when
/// the scene is within the GPU path's reach, and from the CPU renderer
/// otherwise — see the [module docs](self) for what "reach" means today.
///
/// ```
/// # use vieww_3d::gpu::GpuRenderer;
/// # use vieww_3d::math::Vec3;
/// # use vieww_3d::scene::{Camera, Scene};
/// // Probes Vulkan once; on a machine without a loader this is simply the
/// // CPU renderer behind the same call.
/// let renderer = GpuRenderer::new(320, 240);
/// let mut scene = Scene::new();
/// let camera = Camera::perspective(Vec3::new(0.0, 2.0, 5.0), Vec3::ZERO, 0.8);
/// let (image, _stats) = renderer.render(&mut scene, &camera);
/// assert_eq!((image.width(), image.height()), (320, 240));
/// ```
#[derive(Debug)]
pub struct GpuRenderer {
    width: u32,
    height: u32,
    device: Option<vieww_hal::vulkan::VulkanDevice>,
}

impl GpuRenderer {
    /// Probe for a Vulkan device and keep it for the renderer's life. When
    /// the probe fails — no loader, no ICD — the renderer is the CPU one
    /// behind the same contract, and [`is_gpu`](Self::is_gpu) says so.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        let device = match vieww_hal::vulkan::VulkanDevice::new() {
            Ok(device) => Some(device),
            Err(error) => {
                eprintln!("vieww-3d: no Vulkan device, GPU path disabled ({error})");
                None
            }
        };
        Self {
            width,
            height,
            device,
        }
    }

    /// The CPU renderer only, without probing for a device — for callers
    /// that know their machine has no loader, and for tests that need the
    /// fallback path deterministically.
    #[must_use]
    pub const fn without_gpu(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            device: None,
        }
    }

    /// Whether frames render on the GPU (`false` = every frame is the CPU
    /// oracle's).
    #[must_use]
    pub const fn is_gpu(&self) -> bool {
        self.device.is_some()
    }

    /// The CPU renderer the fallback delegates to.
    fn cpu(&self) -> Renderer {
        Renderer::new(self.width, self.height)
    }

    /// Render `scene` from `camera` — the same contract as
    /// [`Renderer::render`](crate::render::Renderer::render).
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    pub fn render(&self, scene: &mut Scene, camera: &Camera) -> (Image, RenderStats) {
        let Some(gpu) = self.device.as_ref() else {
            return self.cpu().render(scene, camera);
        };

        // Scene-level reach: fog is applied to every shading the CPU raster
        // produces, and an environment with a background replaces the flat
        // clear colour with a per-pixel skybox — neither is unlit-pass
        // territory yet.
        if scene.fog.is_some() {
            return self.cpu().render(scene, camera);
        }
        if scene
            .environment
            .as_deref()
            .is_some_and(|e| e.background_sample(Vec3::Y).is_some())
        {
            return self.cpu().render(scene, camera);
        }

        scene.update_world();
        let aspect = self.width as f32 / self.height.max(1) as f32;
        let view = camera.view();
        let proj = camera.projection(aspect);
        let vp = proj * view;
        let planes = frustum(&vp);

        let mut stats = RenderStats::default();
        // The frustum-visible drawables, collected the way `render_frame`
        // collects them — the shadow pass is fitted to exactly this list.
        let mut items: Vec<Item> = Vec::new();

        // One step per push site, mirroring `render_frame`'s: true = kept
        // (or culled), false = the material is outside the GPU passes' reach
        // and the whole frame goes to the CPU.
        let push = |items: &mut Vec<Item>,
                    model: Mat4,
                    mesh: Arc<vieww_mesh::Mesh>,
                    material: &Arc<Material>,
                    bounds: (Vec3, f32),
                    stats: &mut RenderStats|
         -> bool {
            if !material_reachable(material) {
                return false;
            }
            stats.objects += 1;
            let c = model.transform_point(bounds.0);
            let r = bounds.1 * model.max_scale();
            if !sphere_visible(&planes, c, r) {
                stats.culled_objects += 1;
                return true;
            }
            stats.triangles += mesh.indices.len() / 3;
            items.push(Item {
                model,
                mesh,
                material: material.clone(),
                view_depth: 0.0,
            });
            true
        };

        for id in scene.traverse() {
            if !scene.effectively_visible(id) {
                continue;
            }
            let n = scene.node(id);
            let world = n.world();
            let ok = match &n.content {
                Content::Mesh {
                    mesh,
                    material,
                    bounds,
                } => push(&mut items, world, mesh.clone(), material, *bounds, &mut stats),
                Content::Instanced {
                    mesh,
                    material,
                    bounds,
                    instances,
                } => instances.iter().all(|inst| {
                    push(&mut items, world * *inst, mesh.clone(), material, *bounds, &mut stats)
                }),
                Content::Lod {
                    levels,
                    material,
                    bounds,
                } => {
                    let d = (world.transform_point(bounds.0) - camera.position).length();
                    match levels
                        .iter()
                        .find(|(max, _)| d <= *max)
                        .or_else(|| levels.last())
                    {
                        Some((_, mesh)) => {
                            push(&mut items, world, mesh.clone(), material, *bounds, &mut stats)
                        }
                        None => true,
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
                    push(&mut items, world, Arc::new(mesh), material, bounds, &mut stats)
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
                        Projection::Perspective { fov_y, .. } => fov_y,
                        Projection::Orthographic { .. } => 0.8,
                    };
                    let level =
                        lods.select(d, fov, self.height as f32, world.max_scale(), *threshold_px);
                    let (mesh, cs) = lods.gather(level, &world, camera.position, &planes);
                    stats.clusters += cs.clusters;
                    stats.clusters_culled += cs.frustum_culled + cs.cone_culled;
                    mesh.indices.is_empty()
                        || push(&mut items, world, Arc::new(mesh), material, *bounds, &mut stats)
                }
                // Point clouds are a splat pass the GPU path has no drawing
                // of yet; lights and empty nodes draw nothing.
                Content::Points { .. } => false,
                Content::Empty | Content::Light(_) => true,
            };
            if !ok {
                return self.cpu().render(scene, camera);
            }
        }

        let bg = [scene.background.r, scene.background.g, scene.background.b];
        let lit = items.iter().any(|it| it.material.shading == Shading::Lambert);
        let result = if lit {
            match self.lit_frame(gpu, scene, camera, &vp, &view, &items, &mut stats) {
                Some(result) => result,
                None => return self.cpu().render(scene, camera),
            }
        } else {
            Self::unlit_frame(gpu, self.width, self.height, &vp, &items, bg)
        };
        match result {
            Ok(pixels) => (Image::from_rgba8(pixels, self.width, self.height), stats),
            Err(error) => {
                // A device that fails mid-frame loses the GPU path for that
                // frame, not the render contract: the CPU oracle answers.
                eprintln!("vieww-3d: GPU render failed, falling back ({error})");
                self.cpu().render(scene, camera)
            }
        }
    }

    /// The unlit pass: every item `Basic`, flat colour, shared vertices.
    #[allow(clippy::cast_possible_truncation)]
    fn unlit_frame(
        gpu: &vieww_hal::vulkan::VulkanDevice,
        width: u32,
        height: u32,
        vp: &Mat4,
        items: &[Item],
        bg: [f32; 3],
    ) -> Result<Vec<u8>, vieww_hal::vulkan::VulkanError> {
        let mut positions: Vec<[f32; 3]> = Vec::new();
        let mut colors: Vec<[f32; 3]> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        for it in items {
            let base = positions.len();
            for p in &it.mesh.positions {
                positions.push(it.model.transform_point(Vec3::from_array(*p)).to_array());
            }
            // The CPU's Basic output is `albedo + emissive`, exposure-free.
            let col = it.material.color.add(it.material.emissive);
            colors.extend(std::iter::repeat_n([col.r, col.g, col.b], it.mesh.positions.len()));
            indices.extend(it.mesh.indices.iter().map(|i| base as u32 + *i));
        }
        // Mat4 is column-major (`m[col][row]`), which flattens to exactly the
        // WGSL `mat4x4` layout the pass uploads.
        let vp16 = std::array::from_fn(|i| vp.m[i / 4][i % 4]);
        gpu.render_unlit3d_to_pixels(width, height, &positions, &colors, &indices, vp16, bg)
    }

    /// The lit pass, or `None` when the scene needs something it does not
    /// have — a light kind other than ambient, hemisphere or directional,
    /// more than [`MAX_LIGHTS`], or image-based lighting / GI probes, which
    /// every lit material receives.
    #[allow(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        clippy::cast_precision_loss
    )]
    fn lit_frame(
        &self,
        gpu: &vieww_hal::vulkan::VulkanDevice,
        scene: &Scene,
        camera: &Camera,
        vp: &Mat4,
        view: &Mat4,
        items: &[Item],
        stats: &mut RenderStats,
    ) -> Option<Result<Vec<u8>, vieww_hal::vulkan::VulkanError>> {
        if scene.environment.is_some() || scene.gi.is_some() {
            return None;
        }
        let lights = world_lights(scene);
        if lights.len() > MAX_LIGHTS {
            return None;
        }
        let cpu = self.cpu();
        // The CPU renderer's own shadow map, from the same visible items, so
        // the two renderers make the same shadow decision per point.
        let shadow = cpu.shadow_pass(&lights, items, stats);

        // --- uniforms (std140, `lit3d.wgsl`'s `Uniforms`) ---------------------
        let mut u = Vec::with_capacity(40 + 12 * MAX_LIGHTS);
        let flat = |m: &Mat4| -> [f32; 16] { std::array::from_fn(|i| m.m[i / 4][i % 4]) };
        u.extend_from_slice(&flat(vp));
        u.extend_from_slice(&flat(&shadow.as_ref().map_or(Mat4::IDENTITY, |s| s.vp)));
        u.extend_from_slice(&[camera.position.x, camera.position.y, camera.position.z, cpu.exposure]);
        u.extend_from_slice(&[
            lights.len() as f32,
            shadow.as_ref().map_or(0.0, |s| s.size as f32),
            if shadow.is_some() { 1.0 } else { 0.0 },
            0.0,
        ]);
        for l in &lights {
            let d = l.direction;
            let packed = match l.light {
                Light::Ambient { color, intensity } => {
                    [color.r, color.g, color.b, 0.0, 0.0, 0.0, 0.0, intensity, 0.0, 0.0, 0.0, 0.0]
                }
                Light::Hemisphere {
                    sky,
                    ground,
                    intensity,
                } => [
                    sky.r, sky.g, sky.b, 1.0, 0.0, 0.0, 0.0, intensity, ground.r, ground.g,
                    ground.b, 0.0,
                ],
                Light::Directional {
                    color,
                    intensity,
                    shadow,
                    ..
                } => [
                    color.r,
                    color.g,
                    color.b,
                    2.0,
                    d.x,
                    d.y,
                    d.z,
                    intensity,
                    0.0,
                    0.0,
                    0.0,
                    if shadow { 1.0 } else { 0.0 },
                ],
                // Point and spot lights are not in the pass yet.
                _ => return None,
            };
            u.extend_from_slice(&packed);
        }
        u.resize(40 + 12 * MAX_LIGHTS, 0.0);

        // The shadow map's depths; `INFINITY` (nothing casts) as the largest
        // finite float, which compares the same against any NDC depth.
        let depth: Vec<f32> = shadow.as_ref().map_or_else(Vec::new, |s| {
            s.depth.iter().map(|d| d.min(f32::MAX)).collect()
        });

        // --- vertices: de-indexed, facing decided here --------------------------
        //
        // The CPU rasterizer culls a back face of a single-sided material and
        // flips the normal of a double-sided one; it decides by the sign of
        // the triangle's screen-space area. For a triangle in front of the
        // camera that sign is the side of the triangle's plane the eye is on
        // (perspective) or the side the view axis points to (orthographic),
        // which is what is tested here — per triangle, so the GPU pipeline
        // needs no culling state and no facing test.
        let view_axis = Vec3::new(view.m[0][2], view.m[1][2], view.m[2][2]);
        let perspective = matches!(camera.projection, Projection::Perspective { .. });
        let mut vertices: Vec<[f32; 16]> = Vec::new();
        for it in items {
            let mesh = &it.mesh;
            let mat = &it.material;
            let lambert = if mat.shading == Shading::Lambert { 1.0 } else { 0.0 };
            let receive = if mat.receive_shadow { 1.0 } else { 0.0 };
            let normal_m = it.model.inverse().map_or(it.model, |m| m.transpose());
            let has_n = mesh.has_normals();
            let world: Vec<Vec3> = mesh
                .positions
                .iter()
                .map(|p| it.model.transform_point(Vec3::from_array(*p)))
                .collect();
            let normals: Vec<Vec3> = if has_n {
                mesh.normals
                    .iter()
                    .map(|n| normal_m.transform_vector(Vec3::from_array(*n)).normalize())
                    .collect()
            } else {
                Vec::new()
            };
            for t in mesh.indices.as_chunks::<3>().0 {
                let [a, b, c] = t.map(|i| i as usize);
                let face_n = (world[b] - world[a]).cross(world[c] - world[a]).normalize();
                if !(face_n.x.is_finite() && face_n.y.is_finite() && face_n.z.is_finite()) {
                    continue; // degenerate: no area on screen either
                }
                let toward_eye = if perspective {
                    camera.position - world[a]
                } else {
                    view_axis
                };
                let back = face_n.dot(toward_eye) < 0.0;
                if back && !mat.double_sided {
                    continue;
                }
                let sign = if back { -1.0 } else { 1.0 };
                for i in [a, b, c] {
                    let n = if mat.flat || !has_n { face_n } else { normals[i] } * sign;
                    let p = world[i];
                    vertices.push([
                        p.x,
                        p.y,
                        p.z,
                        lambert,
                        n.x,
                        n.y,
                        n.z,
                        receive,
                        mat.color.r,
                        mat.color.g,
                        mat.color.b,
                        0.0,
                        mat.emissive.r,
                        mat.emissive.g,
                        mat.emissive.b,
                        0.0,
                    ]);
                }
            }
        }

        let bg = [scene.background.r, scene.background.g, scene.background.b];
        Some(gpu.render_lit3d_to_pixels(self.width, self.height, &vertices, &u, &depth, bg))
    }
}

/// The most lights the lit pass takes; a scene with more falls back to the
/// CPU for the frame.
const MAX_LIGHTS: usize = 8;

/// Whether a material is inside the GPU passes' reach: `Basic` (the unlit
/// pass, or the lit pass's unlit branch) or `Lambert`, untextured, opaque,
/// filled, with no custom shader.
fn material_reachable(mat: &Material) -> bool {
    matches!(mat.shading, Shading::Basic | Shading::Lambert)
        && mat.map.is_none()
        && mat.opacity >= 1.0
        && !mat.wireframe
        && mat.shader.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Node;
    use vieww_foundation::Color;

    #[test]
    fn without_gpu_returns_the_cpu_renderer_s_image_exactly() {
        let mut scene = Scene::new();
        scene.background = crate::scene::Rgb::new(0.25, 0.5, 0.75);
        scene.add(
            Node::new(
                "box",
                crate::scene::Content::mesh(
                    crate::geometry::box_mesh(1.0, 1.0, 1.0),
                    Material::basic(Color::RED),
                ),
            ),
            None,
        );
        let camera = Camera::perspective(Vec3::new(0.0, 0.0, 3.0), Vec3::ZERO, 0.8);

        let gpu = GpuRenderer::without_gpu(64, 48);
        let (image, _) = gpu.render(&mut scene, &camera);
        let (cpu, _) = Renderer::new(64, 48).render(&mut scene, &camera);

        assert_eq!(image.width(), 64);
        assert_eq!(image.height(), 48);
        assert_eq!(image.pixels(), cpu.pixels());
    }
}
