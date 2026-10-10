//! The Metal backend (spec §4.3, §11.3) — macOS and iOS.
//!
//! # What is here
//!
//! [`MetalDevice`] brings up the system device and a command queue, and
//! implements the same two headless entry points the Vulkan backend's suites
//! are built on:
//!
//! - [`HalDevice::render_clear_to_pixels`] — one render pass that clears a
//!   texture, read back as straight-alpha RGBA8;
//! - [`MetalDevice::render_mesh_to_pixels`] — caller-supplied triangles
//!   through `solid.wgsl`, the exact shader source the Vulkan mesh path
//!   compiles to SPIR-V, here compiled to MSL by `vieww-shaders`.
//!
//! Every frame renders into a `Private` texture and is copied by a blit
//! encoder into a `Shared` buffer for the CPU to read. That is the one
//! readback shape that is correct on every Apple GPU — a `Managed` texture
//! needs a synchronise step on Intel Macs and does not exist on iOS, and a
//! `Shared` render target is slower than either on discrete GPUs.
//!
//! # How shaders get here
//!
//! WGSL → naga → MSL text → `newLibraryWithSource` at pipeline creation.
//! The resource slots are the fixed convention
//! [`vieww_shaders::msl_slots`] documents: the WGSL binding number in the
//! resource type's own Metal index space, vertex data at
//! [`VERTEX_BUFFER`](vieww_shaders::msl_slots::VERTEX_BUFFER). So binding a
//! uniform at `@binding(0)` is `set_vertex_bytes(0, ..)` with no reflection
//! step in between.
//!
//! # Verification
//!
//! This module type-checks on Linux (`ci/check/cross-check.sh macos ios`)
//! and runs on the macOS leg of `.github/workflows/framework-checks.yml`:
//! `tests/metal_smoke.rs` is the same set of claims `vulkan_smoke.rs` and
//! `vulkan_mesh_smoke.rs` make, against the runner's GPU. There is no
//! software Metal, so the CI runner is the first place any of this executes.

use super::{sealed, AdapterInfo, Device as HalDevice};
use metal::{
    Buffer, CommandQueue, CompileOptions, Device, MTLBlendFactor, MTLBlendOperation, MTLBlitOption,
    MTLClearColor, MTLIndexType, MTLLoadAction, MTLOrigin, MTLPixelFormat, MTLPrimitiveType,
    MTLResourceOptions, MTLSize, MTLStorageMode, MTLStoreAction, MTLTextureUsage, MTLVertexFormat,
    MTLVertexStepFunction, RenderPassDescriptor, RenderPipelineDescriptor, RenderPipelineState,
    Texture, TextureDescriptor, TextureRef, VertexDescriptor,
};
use vieww_foundation::Color;
use vieww_shaders::{msl_slots, ParsedShader, ShaderStage};

/// What can go wrong on this backend.
#[derive(Debug)]
pub enum MetalError {
    /// `MTLCreateSystemDefaultDevice` returned nothing — no Metal-capable
    /// GPU is reachable (not Apple hardware, or a virtual machine without
    /// GPU passthrough).
    NoDevice,
    /// WGSL → MSL translation failed (a `vieww-shaders` error).
    Shader(String),
    /// Metal refused something: an MSL compile error, a pipeline the device
    /// cannot build, a command buffer that ended in error.
    Metal(String),
    /// A request this backend will not attempt (zero-sized target, an index
    /// out of range).
    Invalid(String),
}

impl std::fmt::Display for MetalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoDevice => write!(f, "no Metal-capable device found"),
            Self::Shader(e) => write!(f, "shader translation: {e}"),
            Self::Metal(e) => write!(f, "Metal: {e}"),
            Self::Invalid(e) => write!(f, "invalid request: {e}"),
        }
    }
}

impl std::error::Error for MetalError {}

/// The system's Metal device and one command queue.
pub struct MetalDevice {
    device: Device,
    queue: CommandQueue,
}

impl std::fmt::Debug for MetalDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MetalDevice")
            .field("name", &self.device.name())
            .finish_non_exhaustive()
    }
}

impl sealed::Sealed for MetalDevice {}

/// How a pipeline blends into its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    /// Write the fragment as it is.
    Replace,
    /// Straight-alpha source-over: `rgb = src.rgb * src.a + dst.rgb * (1 -
    /// src.a)`, `a = src.a + dst.a * (1 - src.a)`. The Vulkan mesh path's.
    StraightOver,
    /// Premultiplied source-over, `src + dst * (1 - src.a)` on all four
    /// channels. The scene renderer's.
    PremultipliedOver,
}

/// One vertex attribute of an interleaved vertex buffer.
#[derive(Debug, Clone, Copy)]
pub struct Attribute {
    /// The WGSL `@location`.
    pub location: u64,
    /// Number of `f32` components, 1 to 4.
    pub components: u8,
    /// Byte offset inside one vertex.
    pub offset: u64,
}

impl MetalDevice {
    /// Bring up the system's default Metal device and one command queue.
    ///
    /// # Errors
    ///
    /// [`MetalError::NoDevice`] when there is no Metal device.
    pub fn new() -> Result<Self, MetalError> {
        let device = Device::system_default().ok_or(MetalError::NoDevice)?;
        let queue = device.new_command_queue();
        Ok(Self { device, queue })
    }

    /// The underlying `metal::Device`, for code in this crate that builds
    /// on it (the scene renderer).
    #[must_use]
    pub fn raw(&self) -> &Device {
        &self.device
    }

    /// The command queue every frame is submitted on.
    #[must_use]
    pub fn queue(&self) -> &CommandQueue {
        &self.queue
    }

    /// Compile one WGSL module's `vs_main`/`fs_main` into a render pipeline
    /// targeting `format`.
    ///
    /// `attributes` describes one interleaved vertex buffer of `stride`
    /// bytes, bound at [`msl_slots::VERTEX_BUFFER`]; an empty slice means
    /// the vertex stage takes no buffer (a full-screen pass that derives
    /// positions from the vertex index).
    ///
    /// # Errors
    ///
    /// Translation, MSL compilation or pipeline creation failing.
    pub fn pipeline(
        &self,
        wgsl: &str,
        attributes: &[Attribute],
        stride: u64,
        format: MTLPixelFormat,
        blend: Blend,
    ) -> Result<RenderPipelineState, MetalError> {
        let parsed = ParsedShader::parse(wgsl).map_err(|e| MetalError::Shader(e.to_string()))?;
        let vs = self.function(&parsed, "vs_main", ShaderStage::Vertex)?;
        let fs = self.function(&parsed, "fs_main", ShaderStage::Fragment)?;

        let desc = RenderPipelineDescriptor::new();
        desc.set_vertex_function(Some(&vs));
        desc.set_fragment_function(Some(&fs));
        if !attributes.is_empty() {
            let vd = VertexDescriptor::new();
            for a in attributes {
                let attr = vd
                    .attributes()
                    .object_at(a.location)
                    .ok_or_else(|| MetalError::Invalid(format!("location {}", a.location)))?;
                attr.set_format(match a.components {
                    1 => MTLVertexFormat::Float,
                    2 => MTLVertexFormat::Float2,
                    3 => MTLVertexFormat::Float3,
                    _ => MTLVertexFormat::Float4,
                });
                attr.set_offset(a.offset);
                attr.set_buffer_index(u64::from(msl_slots::VERTEX_BUFFER));
            }
            let layout = vd
                .layouts()
                .object_at(u64::from(msl_slots::VERTEX_BUFFER))
                .ok_or_else(|| MetalError::Invalid("vertex buffer layout".into()))?;
            layout.set_stride(stride);
            layout.set_step_function(MTLVertexStepFunction::PerVertex);
            layout.set_step_rate(1);
            desc.set_vertex_descriptor(Some(vd));
        }
        let color = desc
            .color_attachments()
            .object_at(0)
            .ok_or_else(|| MetalError::Invalid("color attachment 0".into()))?;
        color.set_pixel_format(format);
        match blend {
            Blend::Replace => color.set_blending_enabled(false),
            Blend::StraightOver | Blend::PremultipliedOver => {
                color.set_blending_enabled(true);
                color.set_rgb_blend_operation(MTLBlendOperation::Add);
                color.set_alpha_blend_operation(MTLBlendOperation::Add);
                color.set_source_rgb_blend_factor(if blend == Blend::StraightOver {
                    MTLBlendFactor::SourceAlpha
                } else {
                    MTLBlendFactor::One
                });
                color.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
                color.set_source_alpha_blend_factor(MTLBlendFactor::One);
                color.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
            }
        }
        self.device
            .new_render_pipeline_state(&desc)
            .map_err(MetalError::Metal)
    }

    fn function(
        &self,
        parsed: &ParsedShader,
        entry: &str,
        stage: ShaderStage,
    ) -> Result<metal::Function, MetalError> {
        let msl = parsed
            .to_msl(entry, stage)
            .map_err(|e| MetalError::Shader(e.to_string()))?;
        let options = CompileOptions::new();
        let library = self
            .device
            .new_library_with_source(&msl, &options)
            .map_err(|e| MetalError::Metal(format!("compiling {entry}: {e}")))?;
        library
            .get_function(entry, None)
            .map_err(|e| MetalError::Metal(format!("{entry}: {e}")))
    }

    /// A GPU-only texture this backend can render into and sample from.
    #[must_use]
    pub fn render_target(&self, width: u32, height: u32, format: MTLPixelFormat) -> Texture {
        let desc = TextureDescriptor::new();
        desc.set_width(u64::from(width));
        desc.set_height(u64::from(height));
        desc.set_pixel_format(format);
        desc.set_storage_mode(MTLStorageMode::Private);
        desc.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
        self.device.new_texture(&desc)
    }

    /// A CPU-visible buffer holding `bytes`.
    #[must_use]
    pub fn buffer_with(&self, bytes: &[u8]) -> Buffer {
        self.device.new_buffer_with_data(
            bytes.as_ptr().cast(),
            bytes.len().max(1) as u64,
            MTLResourceOptions::StorageModeShared,
        )
    }

    /// Copy `texture` (`bytes_per_pixel` wide texels) into host memory.
    ///
    /// # Errors
    ///
    /// The command buffer finishing in error.
    pub fn read_texture(
        &self,
        texture: &TextureRef,
        width: u32,
        height: u32,
        bytes_per_pixel: u32,
    ) -> Result<Vec<u8>, MetalError> {
        let row = u64::from(width) * u64::from(bytes_per_pixel);
        let len = row * u64::from(height);
        let staging = self
            .device
            .new_buffer(len.max(1), MTLResourceOptions::StorageModeShared);
        let commands = self.queue.new_command_buffer();
        let blit = commands.new_blit_command_encoder();
        blit.copy_from_texture_to_buffer(
            texture,
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
        commands.commit();
        commands.wait_until_completed();
        check(commands)?;
        let len =
            usize::try_from(len).map_err(|_| MetalError::Invalid("frame too large".into()))?;
        // SAFETY: a `Shared` buffer of exactly `len` bytes, written by the
        // completed blit above and not touched by the GPU again.
        let bytes = unsafe { std::slice::from_raw_parts(staging.contents().cast::<u8>(), len) };
        Ok(bytes.to_vec())
    }

    /// Rasterise a triangle mesh in one flat colour — the Metal twin of
    /// `vulkan::VulkanDevice::render_mesh_to_pixels`, through the same
    /// `solid.wgsl`, with the same straight-alpha blend and the same
    /// transparent clear.
    ///
    /// `positions` are pixels, origin top-left; `indices` are a triangle
    /// list into them. Returns straight-alpha RGBA8, row-major.
    ///
    /// # Errors
    ///
    /// A zero-sized frame, an index out of range, or Metal failing.
    pub fn render_mesh_to_pixels(
        &self,
        width: u32,
        height: u32,
        positions: &[[f32; 2]],
        indices: &[u32],
        color: Color,
    ) -> Result<Vec<u8>, MetalError> {
        check_size(width, height)?;
        if positions.is_empty() || indices.is_empty() {
            return Ok(vec![0u8; width as usize * height as usize * 4]);
        }
        if let Some(bad) = indices.iter().find(|&&i| i as usize >= positions.len()) {
            return Err(MetalError::Invalid(format!(
                "index {bad} with {} positions",
                positions.len()
            )));
        }
        let format = MTLPixelFormat::RGBA8Unorm;
        let pipeline = self.pipeline(
            vieww_shaders::library::SOLID_WGSL,
            &[Attribute {
                location: 0,
                components: 2,
                offset: 0,
            }],
            8,
            format,
            Blend::StraightOver,
        )?;
        let target = self.render_target(width, height, format);

        let vertex_bytes: Vec<u8> = positions
            .iter()
            .flat_map(|p| p.iter().flat_map(|v| v.to_le_bytes()))
            .collect();
        let index_bytes: Vec<u8> = indices.iter().flat_map(|i| i.to_le_bytes()).collect();
        let vertices = self.buffer_with(&vertex_bytes);
        let index_buffer = self.buffer_with(&index_bytes);
        #[expect(clippy::cast_precision_loss, reason = "frame sizes < 2^24")]
        let uniforms: [f32; 8] = [
            f32::from(color.r) / 255.0,
            f32::from(color.g) / 255.0,
            f32::from(color.b) / 255.0,
            f32::from(color.a) / 255.0,
            width as f32,
            height as f32,
            0.0,
            0.0,
        ];
        let uniform_bytes: Vec<u8> = uniforms.iter().flat_map(|v| v.to_le_bytes()).collect();

        let pass = RenderPassDescriptor::new();
        let attachment = pass
            .color_attachments()
            .object_at(0)
            .ok_or_else(|| MetalError::Invalid("color attachment 0".into()))?;
        attachment.set_texture(Some(&target));
        attachment.set_load_action(MTLLoadAction::Clear);
        attachment.set_clear_color(MTLClearColor::new(0.0, 0.0, 0.0, 0.0));
        attachment.set_store_action(MTLStoreAction::Store);

        let commands = self.queue.new_command_buffer();
        let encoder = commands.new_render_command_encoder(pass);
        encoder.set_render_pipeline_state(&pipeline);
        encoder.set_vertex_buffer(u64::from(msl_slots::VERTEX_BUFFER), Some(&vertices), 0);
        encoder.set_vertex_bytes(0, uniform_bytes.len() as u64, uniform_bytes.as_ptr().cast());
        encoder.set_fragment_bytes(0, uniform_bytes.len() as u64, uniform_bytes.as_ptr().cast());
        encoder.draw_indexed_primitives(
            MTLPrimitiveType::Triangle,
            indices.len() as u64,
            MTLIndexType::UInt32,
            &index_buffer,
            0,
        );
        encoder.end_encoding();
        commands.commit();
        commands.wait_until_completed();
        check(commands)?;
        self.read_texture(&target, width, height, 4)
    }
}

/// A command buffer that completed in error is an error, not a frame.
fn check(commands: &metal::CommandBufferRef) -> Result<(), MetalError> {
    match commands.status() {
        metal::MTLCommandBufferStatus::Completed => Ok(()),
        status => Err(MetalError::Metal(format!(
            "command buffer ended as {status:?}"
        ))),
    }
}

fn check_size(width: u32, height: u32) -> Result<(), MetalError> {
    if width == 0 || height == 0 || width > 16_384 || height > 16_384 {
        return Err(MetalError::Invalid(format!("{width}x{height} frame")));
    }
    Ok(())
}

impl HalDevice for MetalDevice {
    type Error = MetalError;

    fn info(&self) -> AdapterInfo {
        AdapterInfo {
            name: self.device.name().to_string(),
            backend: "metal",
            device_type: if self.device.has_unified_memory() || self.device.is_low_power() {
                "integrated".to_string()
            } else {
                "discrete".to_string()
            },
        }
    }

    fn render_clear_to_pixels(
        &self,
        width: u32,
        height: u32,
        clear_color: Color,
    ) -> Result<Vec<u8>, MetalError> {
        check_size(width, height)?;
        let format = MTLPixelFormat::RGBA8Unorm;
        let target = self.render_target(width, height, format);
        let pass = RenderPassDescriptor::new();
        let attachment = pass
            .color_attachments()
            .object_at(0)
            .ok_or_else(|| MetalError::Invalid("color attachment 0".into()))?;
        attachment.set_texture(Some(&target));
        attachment.set_load_action(MTLLoadAction::Clear);
        attachment.set_clear_color(MTLClearColor::new(
            f64::from(clear_color.r) / 255.0,
            f64::from(clear_color.g) / 255.0,
            f64::from(clear_color.b) / 255.0,
            f64::from(clear_color.a) / 255.0,
        ));
        attachment.set_store_action(MTLStoreAction::Store);
        let commands = self.queue.new_command_buffer();
        let encoder = commands.new_render_command_encoder(pass);
        encoder.end_encoding();
        commands.commit();
        commands.wait_until_completed();
        check(commands)?;
        self.read_texture(&target, width, height, 4)
    }
}
