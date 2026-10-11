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
//! The GPU path's first increment is an **unlit forward pass**: opaque
//! geometry, `Shading::Basic` materials, flat base colour (plus emissive),
//! one depth-tested draw. Scenes within that reach are rendered on the GPU
//! and held to the CPU oracle byte-for-byte in the parity suite
//! (`tests/gpu_unlit.rs`). Everything outside the reach — lit materials,
//! textures, transparency, fog, skyboxes, point clouds — **falls back to the
//! CPU renderer for the whole frame**, so `render` never returns anything
//! the CPU would not have returned. The fallback is per frame, not per
//! object: mixing half a GPU frame with half a CPU one would be neither
//! oracle-clean nor cheaper to reason about, and a scene that is 90% lit is
//! not a scene the unlit pass should be touching at all.
//!
//! The reach grows the way the 2D path's did (`SceneRenderer` started as a
//! clear-colour smoke test): lighting, textures and transparency are the
//! increments, each landing with its own parity assertions.
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
use crate::render::{frustum, sphere_visible, RenderStats, Renderer};
use crate::scene::{Camera, Content, Material, Scene, Shading};

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
        let mut positions: Vec<[f32; 3]> = Vec::new();
        let mut colors: Vec<[f32; 3]> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();

        // One packing step per push site, mirroring `render_frame`'s: true =
        // packed (or culled), false = the material or content is outside the
        // GPU pass's reach and the whole frame goes to the CPU.
        #[allow(clippy::too_many_arguments)]
        let pack = |positions: &mut Vec<[f32; 3]>,
                    colors: &mut Vec<[f32; 3]>,
                    indices: &mut Vec<u32>,
                    model: &Mat4,
                    mesh: &vieww_mesh::Mesh,
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
            let base = positions.len();
            for p in &mesh.positions {
                let world = model.transform_point(Vec3::from_array(*p));
                positions.push(world.to_array());
            }
            // The CPU's Basic output is `albedo + emissive`, exposure-free.
            let col = material.color.add(material.emissive);
            for _ in 0..mesh.positions.len() {
                colors.push([col.r, col.g, col.b]);
            }
            for i in &mesh.indices {
                indices.push(base as u32 + *i);
            }
            stats.triangles += mesh.indices.len() / 3;
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
                } => pack(
                    &mut positions,
                    &mut colors,
                    &mut indices,
                    &world,
                    mesh,
                    material,
                    *bounds,
                    &mut stats,
                ),
                Content::Instanced {
                    mesh,
                    material,
                    bounds,
                    instances,
                } => instances.iter().all(|inst| {
                    pack(
                        &mut positions,
                        &mut colors,
                        &mut indices,
                        &(world * *inst),
                        mesh,
                        material,
                        *bounds,
                        &mut stats,
                    )
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
                        Some((_, mesh)) => pack(
                            &mut positions,
                            &mut colors,
                            &mut indices,
                            &world,
                            mesh,
                            material,
                            *bounds,
                            &mut stats,
                        ),
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
                    pack(
                        &mut positions,
                        &mut colors,
                        &mut indices,
                        &world,
                        &mesh,
                        material,
                        bounds,
                        &mut stats,
                    )
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
                    let level =
                        lods.select(d, fov, self.height as f32, world.max_scale(), *threshold_px);
                    let (mesh, cs) = lods.gather(level, &world, camera.position, &planes);
                    stats.clusters += cs.clusters;
                    stats.clusters_culled += cs.frustum_culled + cs.cone_culled;
                    mesh.indices.is_empty()
                        || pack(
                            &mut positions,
                            &mut colors,
                            &mut indices,
                            &world,
                            &mesh,
                            material,
                            *bounds,
                            &mut stats,
                        )
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

        // Mat4 is column-major (`m[col][row]`), which flattens to exactly the
        // WGSL `mat4x4` layout `render_unlit3d_to_pixels` uploads.
        let vp16 = std::array::from_fn(|i| vp.m[i / 4][i % 4]);
        let bg = [scene.background.r, scene.background.g, scene.background.b];

        match gpu.render_unlit3d_to_pixels(
            self.width,
            self.height,
            &positions,
            &colors,
            &indices,
            vp16,
            bg,
        ) {
            Ok(pixels) => (Image::from_rgba8(pixels, self.width, self.height), stats),
            Err(error) => {
                // A device that fails mid-frame loses the GPU path for that
                // frame, not the render contract: the CPU oracle answers.
                eprintln!("vieww-3d: GPU render failed, falling back ({error})");
                self.cpu().render(scene, camera)
            }
        }
    }
}

/// Whether a material is inside the unlit pass's reach. `double_sided` and
/// `flat` are deliberately absent: with culling off and no lighting, neither
/// changes a pixel the CPU produces.
fn material_reachable(mat: &Material) -> bool {
    mat.shading == Shading::Basic
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
