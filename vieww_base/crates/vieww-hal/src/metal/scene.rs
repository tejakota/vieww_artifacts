//! The Metal scene renderer — `vulkan::scene::SceneRenderer`'s frame, on
//! [`MetalDevice`].
//!
//! # What this is
//!
//! PENDING 1.2's port, built the way the audit recommends: not a copy of the
//! Vulkan renderer, but an interpreter of the *shared* plan traversal
//! ([`crate::scene_exec::frame_passes`]) over Metal primitives that already
//! existed — [`MetalDevice::pipeline`] (WGSL → MSL through `vieww-shaders`,
//! the vertex descriptor, the blend states), `render_target`, `buffer_with`
//! and the readback shape. What a frame *is* — the pass order, the scissored
//! regions, the damage a retained frame may keep, the immediates every post
//! pass pushes — is one definition in `scene_exec`, consumed by this backend
//! and Vulkan's alike; only the resource management is Metal's.
//!
//! # The contract, and what it means here
//!
//! Exactly `vulkan::scene`'s: targets are premultiplied RGBA16F (`L[n]` and
//! `S[n]` per nesting level, a 1x1 transparent `Dummy`), the four atlases
//! (R8 glyphs, RGBA8 images, R8 masks, RGBA32F ramps) sit at the WGSL
//! bindings the scene shader fetches them through, a frame is drawn over a
//! clear colour and read back as straight-alpha RGBA8 through the shared
//! conversion ([`crate::scene_exec::straight_rgba8_from_rgba16f`]). An
//! incomplete plan is an error, never a frame with holes.
//!
//! # What is deliberately simpler than Vulkan's
//!
//! * **No swapchain path.** `render_planned` / `render_planned_damaged` and
//!   their readback only — no `render_planned_to`, no present history, no
//!   partial-copy probing, no frame overlap: every frame waits for its own
//!   command buffer. The window path on macOS still goes through MoltenVK
//!   until a winit surface lands for this backend.
//! * **Whole-atlas uploads.** A changed atlas version uploads the whole
//!   texture; the Vulkan backend's changed-rectangle mask upload is a
//!   follow-up with the same shape.
//! * **Host timing.** [`FrameTimings`] is `Instant`-based; there are no
//!   Metal timestamp queries in v1, so there is no `GpuPassTimes` here.
//!   The numbers say where the CPU spent and waited, not where the GPU
//!   spent cycles — the module docs' own rule for what such numbers may
//!   claim.
//! * **One encoder per target segment.** Consecutive ops on one target
//!   share a render encoder (and its pipeline binds, deduplicated); every
//!   target switch ends it and the next segment rebinds. Vulkan's render
//!   passes achieve the same sharing; Metal has no `cmd_clear_attachments`
//!   inside an encoder, so a damaged frame's region clears draw
//!   `clear.wgsl` under the scissor instead.
//!
//! # Verification
//!
//! The traversal's tests run everywhere (they are `scene_exec`'s, on every
//! platform with no GPU at all). This backend type-checks from Linux
//! (`ci/check/cross-check.sh macos ios`) and executes for the first time on
//! the `framework-checks` macOS runner, in `tests/metal_scene.rs` — the
//! Vulkan parity suites' scenes against `vieww_paint`'s CPU rasterizer. No
//! number from a paravirtual GPU is a performance claim.

use std::time::{Duration, Instant};

use metal::{
    CommandQueue, Device, MTLBlitOption, MTLClearColor, MTLIndexType, MTLLoadAction, MTLOrigin,
    MTLPixelFormat, MTLPrimitiveType, MTLResourceOptions, MTLScissorRect, MTLSize, MTLStorageMode,
    MTLStoreAction, MTLTextureUsage, MTLViewport, RenderPassDescriptor, RenderPipelineState,
    Texture, TextureDescriptor,
};
use vieww_foundation::Color;
use vieww_gpu::{PixelRect, Planner, ScenePlan, Vertex};

use crate::scene_exec::{frame_passes, Redrawn, Retained, SceneOp, TargetId};

use super::{check_size, Attribute, Blend, MetalDevice, MetalError};

/// Where one frame's wall time went, on the CPU's clock — this backend's
/// counterpart of `vulkan::scene`'s
/// [`GpuFrameTimings`](crate::vulkan::GpuFrameTimings), with the fields a
/// backend without Metal timestamp queries can honestly fill. `submit`
/// includes waiting for the GPU, because this backend never leaves a frame
/// in flight.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameTimings {
    /// Atlas and geometry uploads, and any target (re)allocation.
    pub upload: Duration,
    /// Building the command buffer's encoders.
    pub record: Duration,
    /// Committing it and waiting for the GPU to finish it.
    pub submit: Duration,
    /// Converting the finished frame to straight-alpha RGBA8.
    pub readback: Duration,
}

/// A persistent Metal executor for [`ScenePlan`]s — see the module doc.
pub struct MetalSceneRenderer {
    device: Device,
    queue: CommandQueue,
    scene_pipeline: RenderPipelineState,
    post_replace: RenderPipelineState,
    post_over: RenderPipelineState,
    clear_pipeline: RenderPipelineState,
    levels: Vec<Texture>,
    scratch: Vec<Texture>,
    dummy: Texture,
    atlases: [AtlasTexture; 4],
    size: (u32, u32),
    retained: Option<Retained>,
    timings: FrameTimings,
}

/// One atlas texture, and the version last uploaded.
struct AtlasTexture {
    texture: Texture,
    version: Option<u64>,
    width: u32,
    height: u32,
}

/// The four atlas slots, in the scene shader's binding order 1..=4.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Slot {
    Glyphs,
    Images,
    Masks,
    Ramps,
}

impl Slot {
    const ALL: [Self; 4] = [Self::Glyphs, Self::Images, Self::Masks, Self::Ramps];

    const fn index(self) -> usize {
        match self {
            Self::Glyphs => 0,
            Self::Images => 1,
            Self::Masks => 2,
            Self::Ramps => 3,
        }
    }

    /// The WGSL `@binding` the scene shader fetches this atlas through.
    const fn binding(self) -> u64 {
        match self {
            Self::Glyphs => 1,
            Self::Images => 2,
            Self::Masks => 3,
            Self::Ramps => 4,
        }
    }
}

/// `vieww_gpu::Vertex`, field by field — the WGSL locations' offsets in
/// bytes. Location 7 is `mask` (floats 23..27) and location 8 is `shape`
/// (floats 19..23): the Rust field order and the shader's location order
/// differ, exactly as `vulkan::scene`'s own attribute table documents.
const SCENE_ATTRIBUTES: [Attribute; 9] = [
    Attribute {
        location: 0,
        components: 2,
        offset: 0,
    }, // position
    Attribute {
        location: 1,
        components: 4,
        offset: 8,
    }, // color
    Attribute {
        location: 2,
        components: 2,
        offset: 24,
    }, // uv
    Attribute {
        location: 3,
        components: 1,
        offset: 32,
    }, // kind
    Attribute {
        location: 4,
        components: 4,
        offset: 36,
    }, // params
    Attribute {
        location: 5,
        components: 2,
        offset: 52,
    }, // local
    Attribute {
        location: 6,
        components: 4,
        offset: 60,
    }, // extra
    Attribute {
        location: 7,
        components: 4,
        offset: 92,
    }, // mask
    Attribute {
        location: 8,
        components: 4,
        offset: 76,
    }, // shape
];

/// Every render target and the dummy are premultiplied RGBA16F.
const TARGET_FORMAT: MTLPixelFormat = MTLPixelFormat::RGBA16Float;

/// One atlas slot's own format — the scene shader's four bindings, as
/// `vulkan::scene`'s placeholder uploads size them.
const fn atlas_format(slot: Slot) -> MTLPixelFormat {
    match slot {
        Slot::Glyphs => MTLPixelFormat::R8Unorm,
        Slot::Images => MTLPixelFormat::RGBA8Unorm,
        Slot::Masks => MTLPixelFormat::R8Unorm,
        Slot::Ramps => MTLPixelFormat::RGBA32Float,
    }
}

const fn bytes_per_texel(format: MTLPixelFormat) -> u64 {
    match format {
        MTLPixelFormat::R8Unorm => 1,
        MTLPixelFormat::RGBA8Unorm => 4,
        MTLPixelFormat::RGBA32Float => 16,
        _ => 8,
    }
}

/// A GPU-only texture this backend can render into and sample from —
/// `MetalDevice::render_target`'s creation, on the raw device the renderer
/// holds (it outlives the `MetalDevice` that built it, so it is kept by
/// handle, not by reference to the wrapper).
fn new_target(device: &Device, width: u32, height: u32, format: MTLPixelFormat) -> Texture {
    let desc = TextureDescriptor::new();
    desc.set_width(u64::from(width.max(1)));
    desc.set_height(u64::from(height.max(1)));
    desc.set_pixel_format(format);
    desc.set_storage_mode(MTLStorageMode::Private);
    desc.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
    device.new_texture(&desc)
}

/// The one place `T`'s bytes come from: a plain-data slice, as
/// `vulkan::scene`'s `as_bytes` — every type it is used with here (`Vertex`,
/// `u32`) is a struct of `f32`/`u32` fields with no padding.
fn as_bytes<T: Copy>(values: &[T]) -> &[u8] {
    // SAFETY: `T` is `Copy` with no padding — see the doc above; the slice
    // is the same memory read as bytes.
    unsafe { std::slice::from_raw_parts(values.as_ptr().cast(), std::mem::size_of_val(values)) }
}

fn len64(bytes: &[u8]) -> u64 {
    bytes.len() as u64
}

/// A scissored rectangle, clamped to the frame — every rect the shared
/// traversal emits is already inside it, so this is a cast with the frame
/// as a backstop.
fn scissor(rect: PixelRect, width: u32, height: u32) -> MTLScissorRect {
    let x = rect.x0.clamp(0, width as i32) as u64;
    let y = rect.y0.clamp(0, height as i32) as u64;
    let x1 = rect.x1.clamp(0, width as i32) as u64;
    let y1 = rect.y1.clamp(0, height as i32) as u64;
    MTLScissorRect {
        x,
        y,
        width: x1.saturating_sub(x),
        height: y1.saturating_sub(y),
    }
}

impl MetalSceneRenderer {
    /// Build the pipelines and the placeholder atlases.
    ///
    /// # Errors
    ///
    /// Shader translation (a `vieww-shaders` error) or Metal refusing a
    /// pipeline — see [`MetalDevice::pipeline`].
    pub fn new(device: &MetalDevice) -> Result<Self, MetalError> {
        let scene_pipeline = device.pipeline(
            vieww_shaders::library::SCENE_WGSL,
            &SCENE_ATTRIBUTES,
            u64::try_from(std::mem::size_of::<Vertex>())
                .map_err(|_| MetalError::Invalid("vertex stride".into()))?,
            TARGET_FORMAT,
            Blend::PremultipliedOver,
        )?;
        let post_replace = device.pipeline(
            vieww_shaders::library::POST_WGSL,
            &[],
            0,
            TARGET_FORMAT,
            Blend::Replace,
        )?;
        let post_over = device.pipeline(
            vieww_shaders::library::POST_WGSL,
            &[],
            0,
            TARGET_FORMAT,
            Blend::PremultipliedOver,
        )?;
        // The damaged frame's region clears: `clear.wgsl` is the same
        // full-screen-triangle-with-a-uniform-colour shape `post.wgsl` is,
        // with `Replace` blending so the clear wins over anything beneath.
        let clear_pipeline = device.pipeline(
            vieww_shaders::library::CLEAR_WGSL,
            &[],
            0,
            TARGET_FORMAT,
            Blend::Replace,
        )?;

        let mut renderer = Self {
            device: device.raw().clone(),
            queue: device.queue().clone(),
            scene_pipeline,
            post_replace,
            post_over,
            clear_pipeline,
            levels: Vec::new(),
            scratch: Vec::new(),
            dummy: new_target(device.raw(), 1, 1, TARGET_FORMAT),
            // Each placeholder in its own slot's format: the 1x1 upload below
            // blits one texel of *that* format, and `upload_atlas` only
            // recreates a texture whose size changed — a placeholder in the
            // target format would take an R8 or RGBA8 row into an RGBA16F
            // texture (a bytes-per-row below the texel size Metal requires).
            atlases: Slot::ALL.map(|slot| AtlasTexture {
                texture: new_target(device.raw(), 1, 1, atlas_format(slot)),
                version: None,
                width: 1,
                height: 1,
            }),
            size: (0, 0),
            retained: None,
            timings: FrameTimings::default(),
        };
        // New textures hold nothing: clear the dummy to transparent, and
        // give each atlas slot one zeroed texel of its own format.
        renderer.clear_to_transparent(renderer.dummy.as_ref())?;
        for slot in Slot::ALL {
            let format = atlas_format(slot);
            let zeros = vec![0u8; bytes_per_texel(format) as usize];
            renderer.upload_atlas(slot, &zeros, 1, 1, None)?;
        }
        Ok(renderer)
    }

    /// Execute `plan` at `width x height` over `clear`, with `planner`'s
    /// atlases uploaded first, and read the frame back as straight-alpha
    /// RGBA8 — `vulkan::scene::render_planned`'s contract.
    ///
    /// # Errors
    ///
    /// An incomplete plan (this backend cannot draw it either — render such
    /// a frame on the CPU rasterizer), or Metal failing.
    pub fn render_planned(
        &mut self,
        planner: &Planner,
        plan: &ScenePlan,
        width: u32,
        height: u32,
        clear: Color,
    ) -> Result<Vec<u8>, MetalError> {
        self.render_planned_damaged(planner, plan, width, height, clear, None)
            .map(|(pixels, _)| pixels)
    }

    /// [`Self::render_planned`], redrawing only `damage` of the previous
    /// frame this renderer drew — the shared traversal's retained-frame
    /// rules, which are `vulkan::scene`'s. `None` redraws everything.
    ///
    /// # Errors
    ///
    /// As [`Self::render_planned`].
    pub fn render_planned_damaged(
        &mut self,
        planner: &Planner,
        plan: &ScenePlan,
        width: u32,
        height: u32,
        clear: Color,
        damage: Option<&[PixelRect]>,
    ) -> Result<(Vec<u8>, Redrawn), MetalError> {
        if !plan.is_complete() {
            return Err(MetalError::Invalid(format!(
                "incomplete plan: this backend cannot draw {}. Render this frame on the CPU \
                 rasterizer",
                plan.unsupported
                    .iter()
                    .map(|(kind, count)| format!("{count}x{}", kind.name()))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        self.timings = FrameTimings::default();
        let started = Instant::now();
        self.upload_planner(planner)?;
        self.timings.upload = started.elapsed();
        self.render_with(plan, width, height, clear, damage)
    }

    /// Where the last frame's wall time went — see [`FrameTimings`].
    #[must_use]
    pub const fn last_timings(&self) -> FrameTimings {
        self.timings
    }

    // ── the frame ─────────────────────────────────────────────────────────

    #[allow(clippy::too_many_lines)]
    fn render_with(
        &mut self,
        plan: &ScenePlan,
        width: u32,
        height: u32,
        clear: Color,
        damage: Option<&[PixelRect]>,
    ) -> Result<(Vec<u8>, Redrawn), MetalError> {
        let width = width.max(1);
        let height = height.max(1);
        check_size(width, height)?;

        let exec = frame_passes(plan, width, height, clear, damage, self.retained);
        // Until this frame finishes, `L[0]` is half-drawn.
        self.retained = None;

        let started = Instant::now();
        self.ensure_targets(width, height, plan.target_depth + 1)?;
        let vertex_bytes = as_bytes(&plan.vertices);
        let index_bytes = as_bytes(&plan.indices);
        let vertices = self.device.new_buffer_with_data(
            vertex_bytes.as_ptr().cast(),
            len64(vertex_bytes).max(1),
            MTLResourceOptions::StorageModeShared,
        );
        let indices = self.device.new_buffer_with_data(
            index_bytes.as_ptr().cast(),
            len64(index_bytes).max(1),
            MTLResourceOptions::StorageModeShared,
        );
        #[expect(clippy::cast_precision_loss, reason = "frame extents")]
        let viewport: [f32; 4] = [width as f32, height as f32, 0.0, 0.0];
        self.timings.upload += started.elapsed();

        // ── the interpreter ─────────────────────────────────────────────
        //
        // One encoder per contiguous run of ops on one target; a target
        // switch ends it and the next segment rebinds. `bound` is the
        // encoder's own pipeline state, reset by every switch — the same
        // dedup Vulkan's interpreter does per render pass.
        let started = Instant::now();
        let commands = self.queue.new_command_buffer();

        #[derive(Clone, Copy, PartialEq)]
        enum Bound {
            None,
            Scene,
            /// The post pipeline, and whether it is the blending one.
            Post(bool),
        }
        let mut bound = Bound::None;
        let mut open: Option<TargetId> = None;
        let mut encoder: Option<&metal::RenderCommandEncoderRef> = None;

        fn begin_on<'a>(
            renderer: &'a MetalSceneRenderer,
            commands: &'a metal::CommandBufferRef,
            target: TargetId,
            clear: Option<[f32; 4]>,
            width: u32,
            height: u32,
        ) -> &'a metal::RenderCommandEncoderRef {
            let pass = RenderPassDescriptor::new();
            let attachment = pass
                .color_attachments()
                .object_at(0)
                .expect("attachment 0 exists");
            attachment.set_texture(Some(renderer.target(target)));
            match clear {
                Some(color) => {
                    attachment.set_load_action(MTLLoadAction::Clear);
                    attachment.set_clear_color(MTLClearColor::new(
                        f64::from(color[0]),
                        f64::from(color[1]),
                        f64::from(color[2]),
                        f64::from(color[3]),
                    ));
                }
                None => attachment.set_load_action(MTLLoadAction::Load),
            }
            attachment.set_store_action(MTLStoreAction::Store);
            let encoder = commands.new_render_command_encoder(pass);
            encoder.set_viewport(MTLViewport {
                originX: 0.0,
                originY: 0.0,
                width: f64::from(width),
                height: f64::from(height),
                znear: 0.0,
                zfar: 1.0,
            });
            encoder
        }

        fn end_current(encoder: &mut Option<&metal::RenderCommandEncoderRef>) {
            if let Some(open) = encoder.take() {
                open.end_encoding();
            }
        }

        for op in &exec.ops {
            match op {
                SceneOp::Begin { target, clear, .. } => {
                    end_current(&mut encoder);
                    bound = Bound::None;
                    encoder = Some(begin_on(self, commands, *target, *clear, width, height));
                    open = Some(*target);
                }
                SceneOp::ClearRegions { rects, color } => {
                    // Never a first op: the frame's own begin precedes it.
                    // Metal has no `cmd_clear_attachments`; a region clear is
                    // a draw of `clear.wgsl` under the scissor, blending
                    // `Replace`.
                    let target = open.expect("a region clear follows a begin");
                    end_current(&mut encoder);
                    bound = Bound::None;
                    let enc = begin_on(self, commands, target, None, width, height);
                    enc.set_render_pipeline_state(&self.clear_pipeline);
                    enc.set_fragment_bytes(0, 16, color.as_ptr().cast());
                    for rect in rects {
                        enc.set_scissor_rect(scissor(*rect, width, height));
                        enc.draw_primitives(MTLPrimitiveType::Triangle, 0, 3);
                    }
                    encoder = Some(enc);
                }
                SceneOp::DrawRun {
                    target,
                    first_index,
                    index_count,
                    scissor: rect,
                } => {
                    if open != Some(*target) {
                        end_current(&mut encoder);
                        bound = Bound::None;
                        open = Some(*target);
                        encoder = Some(begin_on(self, commands, *target, None, width, height));
                    }
                    let enc = encoder.expect("a draw has a target to encode onto");
                    if bound != Bound::Scene {
                        enc.set_render_pipeline_state(&self.scene_pipeline);
                        enc.set_vertex_bytes(0, 16, viewport.as_ptr().cast());
                        enc.set_vertex_buffer(
                            u64::from(vieww_shaders::msl_slots::VERTEX_BUFFER),
                            Some(&vertices),
                            0,
                        );
                        let [glyphs, images, masks, ramps] = &self.atlases;
                        enc.set_fragment_textures(
                            Slot::Glyphs.binding(),
                            &[
                                Some(&glyphs.texture),
                                Some(&images.texture),
                                Some(&masks.texture),
                                Some(&ramps.texture),
                            ],
                        );
                        bound = Bound::Scene;
                    }
                    enc.set_scissor_rect(scissor(*rect, width, height));
                    enc.draw_indexed_primitives(
                        MTLPrimitiveType::Triangle,
                        u64::from(*index_count),
                        MTLIndexType::UInt32,
                        &indices,
                        u64::from(*first_index) * 4,
                    );
                }
                SceneOp::Post { pass, scissors, .. } => {
                    if open != Some(pass.into) {
                        end_current(&mut encoder);
                        bound = Bound::None;
                        open = Some(pass.into);
                        encoder = Some(begin_on(self, commands, pass.into, None, width, height));
                    }
                    let enc = encoder.expect("a post pass has a target to encode onto");
                    if bound != Bound::Post(pass.over) {
                        enc.set_render_pipeline_state(if pass.over {
                            &self.post_over
                        } else {
                            &self.post_replace
                        });
                        bound = Bound::Post(pass.over);
                    }
                    enc.set_fragment_textures(
                        0,
                        &[
                            Some(self.target(pass.src)),
                            Some(self.target(pass.aux)),
                            Some(&self.atlases[Slot::Masks.index()].texture),
                        ],
                    );
                    enc.set_fragment_bytes(
                        u64::from(vieww_shaders::msl_slots::IMMEDIATES_BUFFER),
                        128,
                        std::ptr::from_ref(&pass.push).cast(),
                    );
                    for rect in scissors {
                        enc.set_scissor_rect(scissor(*rect, width, height));
                        enc.draw_primitives(MTLPrimitiveType::Triangle, 0, 3);
                    }
                }
            }
        }
        end_current(&mut encoder);
        self.timings.record = started.elapsed();

        // ── the frame out: readback ─────────────────────────────────────
        let started = Instant::now();
        let row = u64::from(width) * 8;
        let len = row * u64::from(height);
        let staging = self
            .device
            .new_buffer(len.max(1), MTLResourceOptions::StorageModeShared);
        {
            let blit = commands.new_blit_command_encoder();
            blit.copy_from_texture_to_buffer(
                self.levels[0].as_ref(),
                0,
                0,
                MTLOrigin { x: 0, y: 0, z: 0 },
                MTLSize {
                    width: u64::from(width),
                    height: u64::from(height),
                    depth: 1,
                },
                &staging,
                0,
                row,
                len,
                MTLBlitOption::empty(),
            );
            blit.end_encoding();
        }
        let submitting = Instant::now();
        commands.commit();
        commands.wait_until_completed();
        super::check(commands)?;
        self.timings.submit = submitting.elapsed();

        // SAFETY: a `Shared` buffer of exactly `len` bytes, written by the
        // completed blit above and not touched by the GPU again.
        let half: Vec<u16> = unsafe {
            std::slice::from_raw_parts(staging.contents().cast::<u16>(), (len / 2) as usize)
        }
        .to_vec();
        // The shared readback conversion — see `scene_exec`'s module doc.
        let pixels = crate::scene_exec::straight_rgba8_from_rgba16f(&half);
        self.timings.readback = started.elapsed() - self.timings.submit;

        self.retained = Some(exec.retained);
        Ok((pixels, exec.redrawn))
    }

    // ── resources ─────────────────────────────────────────────────────────

    /// The texture of `target` — the level, its scratch twin, or the dummy.
    fn target(&self, target: TargetId) -> &Texture {
        match target {
            TargetId::Level(n) => &self.levels[n],
            TargetId::Scratch(n) => &self.scratch[n],
            TargetId::Dummy => &self.dummy,
        }
    }

    /// (Re)allocate the levels and scratch to `depth` targets of the frame
    /// size, clearing every fresh one to transparent — a scratch read
    /// before its first write must see transparent, not undefined bytes.
    fn ensure_targets(&mut self, width: u32, height: u32, depth: usize) -> Result<(), MetalError> {
        if self.size != (width, height) {
            self.levels.clear();
            self.scratch.clear();
            self.size = (width, height);
        }
        while self.levels.len() < depth {
            let level_texture = new_target(&self.device, width, height, TARGET_FORMAT);
            self.clear_to_transparent(level_texture.as_ref())?;
            self.levels.push(level_texture);
            let scratch_texture = new_target(&self.device, width, height, TARGET_FORMAT);
            self.clear_to_transparent(scratch_texture.as_ref())?;
            self.scratch.push(scratch_texture);
        }
        Ok(())
    }

    /// One command buffer that clears `texture` to fully transparent.
    fn clear_to_transparent(&self, texture: &metal::TextureRef) -> Result<(), MetalError> {
        let commands = self.queue.new_command_buffer();
        let pass = RenderPassDescriptor::new();
        let attachment = pass
            .color_attachments()
            .object_at(0)
            .expect("attachment 0 exists");
        attachment.set_texture(Some(texture));
        attachment.set_load_action(MTLLoadAction::Clear);
        attachment.set_clear_color(MTLClearColor::new(0.0, 0.0, 0.0, 0.0));
        attachment.set_store_action(MTLStoreAction::Store);
        let encoder = commands.new_render_command_encoder(pass);
        encoder.end_encoding();
        commands.commit();
        commands.wait_until_completed();
        super::check(commands)
    }

    /// Upload every atlas `planner` holds that changed since the last
    /// upload — whole-texture uploads in v1; see the module doc.
    fn upload_planner(&mut self, planner: &Planner) -> Result<(), MetalError> {
        let atlas = planner.atlas();
        self.upload_atlas(
            Slot::Glyphs,
            atlas.texels(),
            atlas.side(),
            atlas.side(),
            Some(atlas.version()),
        )?;
        let images = planner.image_atlas();
        self.upload_atlas(
            Slot::Images,
            images.texels(),
            images.side(),
            images.side(),
            Some(images.version()),
        )?;
        let masks = planner.mask_atlas();
        self.upload_atlas(
            Slot::Masks,
            masks.texels(),
            masks.side(),
            masks.side(),
            Some(masks.version()),
        )?;
        let ramps = planner.ramp_atlas();
        self.upload_atlas(
            Slot::Ramps,
            as_bytes(ramps.texels()),
            vieww_gpu::RampAtlas::WIDTH,
            ramps.rows(),
            Some(ramps.version()),
        )?;
        Ok(())
    }

    /// Bring one atlas up to `bytes` at `version`: skipped when unchanged,
    /// recreated when the size changed, blitted from a shared buffer
    /// otherwise. Version `None` is the zeroed placeholder at bring-up.
    fn upload_atlas(
        &mut self,
        slot: Slot,
        bytes: &[u8],
        width: u32,
        height: u32,
        version: Option<u64>,
    ) -> Result<(), MetalError> {
        let atlas = &mut self.atlases[slot.index()];
        if version.is_some()
            && atlas.version == version
            && atlas.width == width
            && atlas.height == height
        {
            return Ok(());
        }
        let format = atlas_format(slot);
        if atlas.texture.width() != u64::from(width) || atlas.texture.height() != u64::from(height)
        {
            atlas.texture = new_target(&self.device, width.max(1), height.max(1), format);
            atlas.width = width;
            atlas.height = height;
        }
        let staging = self.device.new_buffer_with_data(
            bytes.as_ptr().cast(),
            len64(bytes).max(bytes_per_texel(format)),
            MTLResourceOptions::StorageModeShared,
        );
        let commands = self.queue.new_command_buffer();
        let blit = commands.new_blit_command_encoder();
        blit.copy_from_buffer_to_texture(
            &staging,
            0,
            u64::from(width.max(1)) * bytes_per_texel(format),
            0,
            MTLSize {
                width: u64::from(width.max(1)),
                height: u64::from(height.max(1)),
                depth: 1,
            },
            &atlas.texture,
            0,
            0,
            MTLOrigin { x: 0, y: 0, z: 0 },
            MTLBlitOption::empty(),
        );
        blit.end_encoding();
        commands.commit();
        commands.wait_until_completed();
        super::check(commands)?;
        atlas.version = version;
        Ok(())
    }
}

impl std::fmt::Debug for MetalSceneRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MetalSceneRenderer")
            .field("size", &self.size)
            .field("targets", &self.levels.len())
            .finish_non_exhaustive()
    }
}
