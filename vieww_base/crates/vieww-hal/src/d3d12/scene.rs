//! The D3D12 scene renderer — `vulkan::scene::SceneRenderer`'s frame, on
//! [`D3d12Device`].
//!
//! # What this is
//!
//! PENDING 1.3's port, and the same shape as the Metal one: an interpreter
//! of the *shared* plan traversal ([`crate::scene_exec::frame_passes`]) over
//! this backend's own primitives — WGSL → HLSL through `vieww-shaders` and
//! `d3dcompiler_47`, root signatures, committed resources, the fenced
//! submit, the placed-footprint readback. What a frame *is* is one
//! definition in `scene_exec`, consumed here, by Vulkan and by Metal alike;
//! only the resource management is D3D12's.
//!
//! # The binding model, stated once
//!
//! naga maps the WGSL bindings to HLSL registers (see
//! [`vieww_shaders::hlsl_slots`]): the scene shader's uniform is `b0` and its
//! four atlases are `t1..t4`; `post.wgsl`'s immediates are root constants at
//! `b15` and its three textures are `t0..t2`; `clear.wgsl`'s colour uniform
//! is `b0`. So:
//!
//! * **One shader-visible SRV heap**, laid out once: slots 0-3 the four
//!   atlases, one slot per render target's resource, a three-slot "post
//!   triplet" that every post pass rewrites with its `src`, `aux` and the
//!   mask atlas, and one slot for the dummy.
//! * **Three root signatures**: the scene's (root constants at `b0`, one
//!   descriptor table `t1..t4`), the post's (root constants at `b15`, one
//!   table `t0..t2`), and the clear's (root constants at `b0`). Root
//!   constants back the constant buffers directly — no CBV heap, no upload,
//!   the same trick the mesh path uses for `solid.wgsl`.
//! * **Transitions, tracked per target**: a target rests in
//!   `PIXEL_SHADER_RESOURCE` (what a post pass reads), moves to
//!   `RENDER_TARGET` for the encoders that draw into it, and the frame
//!   target moves on to `COPY_SOURCE` for the readback.
//!
//! # What is deliberately simpler than Vulkan's
//!
//! As the Metal port's, and for the same reasons: readback frames only (no
//! swapchain present), whole-atlas uploads, `Instant`-based [`FrameTimings`]
//! with no timestamp queries, and a region clear drawn through `clear.wgsl`
//! because D3D12's `ClearRenderTargetView` clears a whole target or nothing.
//! This backend owns its own direct queue and fence, so it submits and
//! waits independently of the [`D3d12Device`] that built it — no frame is
//! ever left in flight.
//!
//! # Verification
//!
//! The traversal's tests run everywhere. This backend type-checks from Linux
//! (`ci/check/cross-check.sh windows`) and executes on the Windows leg of
//! `framework-checks.yml` — `tests/d3d12_scene.rs`, the Vulkan parity
//! suites' scenes against `vieww_paint`'s CPU rasterizer, on WARP
//! (`VIEWW_D3D12_ALLOW_WARP=1`) or a hardware adapter. Correctness evidence
//! is never performance evidence, most of all on WARP.

use std::mem::ManuallyDrop;
use std::time::{Duration, Instant};

use vieww_foundation::Color;
use vieww_gpu::{PixelRect, Planner, ScenePlan, Vertex};
use windows::core::Interface;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_FORMAT_R32G32B32A32_FLOAT,
    DXGI_FORMAT_R32G32_FLOAT, DXGI_FORMAT_R32_FLOAT, DXGI_FORMAT_R8G8B8A8_UNORM,
    DXGI_FORMAT_R8_UNORM,
};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject, INFINITE};

use crate::scene_exec::{frame_passes, Redrawn, Retained, SceneOp, TargetId};

use super::{check_size, D3d12Device, D3d12Error};

/// Where one frame's wall time went, on the CPU's clock — this backend's
/// counterpart of `vulkan::scene`'s
/// [`GpuFrameTimings`](crate::vulkan::GpuFrameTimings), with the fields a
/// backend without timestamp queries can honestly fill. `submit` includes
/// waiting for the queue's fence, because this backend never leaves a frame
/// in flight.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameTimings {
    /// Atlas and geometry uploads, and any target (re)allocation.
    pub upload: Duration,
    /// Building the command list.
    pub record: Duration,
    /// Executing it and waiting for the fence.
    pub submit: Duration,
    /// Converting the finished frame to straight-alpha RGBA8.
    pub readback: Duration,
}

/// A persistent D3D12 executor for [`ScenePlan`]s — see the module doc.
pub struct D3d12SceneRenderer {
    device: ID3D12Device,
    queue: ID3D12CommandQueue,
    fence: ID3D12Fence,
    fence_value: u64,
    scene_root: ID3D12RootSignature,
    post_root: ID3D12RootSignature,
    clear_root: ID3D12RootSignature,
    scene_pso: ID3D12PipelineState,
    post_replace_pso: ID3D12PipelineState,
    post_over_pso: ID3D12PipelineState,
    clear_pso: ID3D12PipelineState,
    /// The one shader-visible heap: atlases, target SRVs, the post triplet,
    /// the dummy.
    heap: ID3D12DescriptorHeap,
    increment: u32,
    /// The targets' RTVs. Replaced with the targets, so no RTV ever outlives
    /// its heap.
    rtv_heap: ID3D12DescriptorHeap,
    atlases: [AtlasResource; 4],
    levels: Vec<TargetResource>,
    scratch: Vec<TargetResource>,
    /// 1x1 transparent, its SRV at slot [`DUMMY_SLOT`].
    dummy: ID3D12Resource,
    size: (u32, u32),
    retained: Option<Retained>,
    timings: FrameTimings,
}

/// One atlas: the resource and the version last uploaded. Its SRV lives in
/// the heap at its own slot, written whenever the resource is (re)created.
struct AtlasResource {
    resource: ID3D12Resource,
    version: Option<u64>,
    width: u32,
    height: u32,
}

/// One render target: the resource, its RTV handle (in the renderer's own
/// heap) and its tracked resource state. Targets have no heap slots of
/// their own — every post pass rewrites the triplet with its `src` and
/// `aux` instead, which is why the heap stays eight descriptors wide.
struct TargetResource {
    resource: ID3D12Resource,
    rtv: D3D12_CPU_DESCRIPTOR_HANDLE,
    state: D3D12_RESOURCE_STATES,
}

// The heap, laid out once: [0..4) the atlases, then the post triplet, then
// the dummy — eight descriptors in all, because targets carry no slots of
// their own (see `TargetResource`).
const ATLAS_SLOTS: u32 = 4;
const TRIPLET: u32 = ATLAS_SLOTS;
const DUMMY_SLOT: u32 = TRIPLET + 3;
const HEAP_DESCRIPTORS: u32 = DUMMY_SLOT + 1;

const TARGET_FORMAT: DXGI_FORMAT = DXGI_FORMAT_R16G16B16A16_FLOAT;

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

    const fn heap_slot(self) -> u32 {
        self.index() as u32
    }
}

const fn atlas_format(slot: Slot) -> DXGI_FORMAT {
    match slot {
        Slot::Glyphs => DXGI_FORMAT_R8_UNORM,
        Slot::Images => DXGI_FORMAT_R8G8B8A8_UNORM,
        Slot::Masks => DXGI_FORMAT_R8_UNORM,
        Slot::Ramps => DXGI_FORMAT_R32G32B32A32_FLOAT,
    }
}

const fn bytes_per_texel(format: DXGI_FORMAT) -> u32 {
    match format {
        DXGI_FORMAT_R8_UNORM => 1,
        DXGI_FORMAT_R8G8B8A8_UNORM => 4,
        DXGI_FORMAT_R32G32B32A32_FLOAT => 16,
        _ => 8,
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "lengths are checked; scissor coordinates are in-frame"
)]
fn len32(bytes: &[u8]) -> u32 {
    u32::try_from(bytes.len()).unwrap_or(u32::MAX)
}

/// `bytes` rounded up to D3D12's 256-byte row-pitch alignment.
const fn row_pitch(bytes: u32) -> u32 {
    (bytes + 255) / 256 * 256
}

/// An SRV over a whole 2D resource, written into `slot` of the renderer's
/// heap.
fn srv(
    device: &ID3D12Device,
    heap: &ID3D12DescriptorHeap,
    increment: u32,
    slot: u32,
    resource: &ID3D12Resource,
    format: DXGI_FORMAT,
) {
    let mut desc = D3D12_SHADER_RESOURCE_VIEW_DESC {
        Format: format,
        ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2D,
        Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
        ..Default::default()
    };
    // A union write of a `Copy` field is safe; only the Texture2D arm of a
    // Texture2D-dimensional view is ever read.
    desc.Anonymous.Texture2D = D3D12_TEX2D_SRV {
        MostDetailedMip: 0,
        MipLevels: 1,
        PlaneSlice: 0,
        ResourceMinLODClamp: 0.0,
    };
    let at = D3D12_CPU_DESCRIPTOR_HANDLE {
        ptr: unsafe { heap.GetCPUDescriptorHandleForHeapStart() }.ptr
            + slot as usize * increment as usize,
    };
    // SAFETY: a live resource, a fully initialised view desc, and a handle
    // inside a heap this renderer owns.
    unsafe { device.CreateShaderResourceView(resource, Some(&desc), at) };
}

/// A GPU handle `slot` into the renderer's heap.
fn gpu_handle(
    heap: &ID3D12DescriptorHeap,
    increment: u32,
    slot: u32,
) -> D3D12_GPU_DESCRIPTOR_HANDLE {
    D3D12_GPU_DESCRIPTOR_HANDLE {
        ptr: unsafe { heap.GetGPUDescriptorHandleForHeapStart() }.ptr
            + u64::from(slot) * u64::from(increment),
    }
}

/// Premultiplied source-over — the scene pipeline's and the blending post
/// pipeline's blend, `ONE / INV_SRC_ALPHA` on every channel.
fn premultiplied_over() -> D3D12_RENDER_TARGET_BLEND_DESC {
    D3D12_RENDER_TARGET_BLEND_DESC {
        BlendEnable: true.into(),
        LogicOpEnable: false.into(),
        SrcBlend: D3D12_BLEND_ONE,
        DestBlend: D3D12_BLEND_INV_SRC_ALPHA,
        BlendOp: D3D12_BLEND_OP_ADD,
        SrcBlendAlpha: D3D12_BLEND_ONE,
        DestBlendAlpha: D3D12_BLEND_INV_SRC_ALPHA,
        BlendOpAlpha: D3D12_BLEND_OP_ADD,
        LogicOp: D3D12_LOGIC_OP_NOOP,
        #[expect(clippy::cast_possible_truncation, reason = "the mask is 0xF")]
        RenderTargetWriteMask: D3D12_COLOR_WRITE_ENABLE_ALL.0 as u8,
    }
}

/// No blending at all — the replacing post pipeline's and the clear's.
fn replace() -> D3D12_RENDER_TARGET_BLEND_DESC {
    D3D12_RENDER_TARGET_BLEND_DESC {
        BlendEnable: false.into(),
        LogicOpEnable: false.into(),
        SrcBlend: D3D12_BLEND_ONE,
        DestBlend: D3D12_BLEND_ZERO,
        BlendOp: D3D12_BLEND_OP_ADD,
        SrcBlendAlpha: D3D12_BLEND_ONE,
        DestBlendAlpha: D3D12_BLEND_ZERO,
        BlendOpAlpha: D3D12_BLEND_OP_ADD,
        LogicOp: D3D12_LOGIC_OP_NOOP,
        #[expect(clippy::cast_possible_truncation, reason = "the mask is 0xF")]
        RenderTargetWriteMask: D3D12_COLOR_WRITE_ENABLE_ALL.0 as u8,
    }
}

/// Serialise and create a root signature from `parameters` on `device`.
fn root_signature(
    device: &D3d12Device,
    parameters: &[D3D12_ROOT_PARAMETER],
) -> Result<ID3D12RootSignature, D3d12Error> {
    let desc = D3D12_ROOT_SIGNATURE_DESC {
        NumParameters: u32::try_from(parameters.len()).unwrap_or(0),
        pParameters: parameters.as_ptr(),
        NumStaticSamplers: 0,
        pStaticSamplers: std::ptr::null(),
        Flags: D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT,
    };
    device.root_signature(&desc)
}

/// A 2D texture descriptor.
fn texture_desc(format: DXGI_FORMAT, width: u32, height: u32) -> D3D12_RESOURCE_DESC {
    D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
        Alignment: 0,
        Width: u64::from(width.max(1)),
        Height: height.max(1),
        DepthOrArraySize: 1,
        MipLevels: 1,
        Format: format,
        SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
        Flags: D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
    }
}

impl D3d12SceneRenderer {
    /// Build the root signatures, the pipelines, the heap and the
    /// placeholder atlases.
    ///
    /// # Errors
    ///
    /// Any D3D12 call failing, or a shader failing to translate or compile.
    pub fn new(device: &D3d12Device) -> Result<Self, D3d12Error> {
        let raw = device.raw().clone();
        let queue_desc = D3D12_COMMAND_QUEUE_DESC {
            Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
            Priority: 0,
            Flags: D3D12_COMMAND_QUEUE_FLAG_NONE,
            NodeMask: 0,
        };
        // SAFETY: creation calls on a live device.
        let queue: ID3D12CommandQueue = unsafe { raw.CreateCommandQueue(&queue_desc)? };
        // SAFETY: as above.
        let fence: ID3D12Fence = unsafe { raw.CreateFence(0, D3D12_FENCE_FLAG_NONE)? };

        // ── the one heap ──────────────────────────────────────────────────
        let heap_desc = D3D12_DESCRIPTOR_HEAP_DESC {
            Type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
            NumDescriptors: HEAP_DESCRIPTORS,
            Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
            NodeMask: 0,
        };
        // SAFETY: as above.
        let heap: ID3D12DescriptorHeap = unsafe { raw.CreateDescriptorHeap(&heap_desc)? };
        let increment =
            unsafe { raw.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV) };

        // ── root signatures ───────────────────────────────────────────────
        // The scene: four dwords at b0, the atlases as one table t1..t4.
        let scene_range = [D3D12_DESCRIPTOR_RANGE {
            RangeType: D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
            NumDescriptors: 4,
            BaseShaderRegister: 1,
            RegisterSpace: 0,
            OffsetInDescriptorsFromTableStart: 0,
        }];
        let scene_parameters = [
            constants_parameter(0, 4),
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                        NumDescriptorRanges: 1,
                        pDescriptorRanges: scene_range.as_ptr(),
                    },
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
            },
        ];
        let scene_root = root_signature(device, &scene_parameters)?;
        // The post: the immediates as 32 dwords at b15, one table t0..t2.
        let post_range = [D3D12_DESCRIPTOR_RANGE {
            RangeType: D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
            NumDescriptors: 3,
            BaseShaderRegister: 0,
            RegisterSpace: 0,
            OffsetInDescriptorsFromTableStart: 0,
        }];
        let post_parameters = [
            constants_parameter(vieww_shaders::hlsl_slots::IMMEDIATES_REGISTER, 32),
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                        NumDescriptorRanges: 1,
                        pDescriptorRanges: post_range.as_ptr(),
                    },
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
            },
        ];
        let post_root = root_signature(device, &post_parameters)?;
        // The clear: the colour at b0, four dwords.
        let clear_parameters = [constants_parameter(0, 4)];
        let clear_root = root_signature(device, &clear_parameters)?;

        // ── pipelines ─────────────────────────────────────────────────────
        let semantic = std::ffi::CString::new(vieww_shaders::hlsl_slots::VERTEX_SEMANTIC)
            .map_err(|_| D3d12Error::Invalid("semantic".into()))?;
        // `vieww_gpu::Vertex`'s WGSL locations, as HLSL LOCn semantics —
        // location 7 is `mask` (floats 23..27) and 8 is `shape` (19..23),
        // as the shared attribute table has it.
        let element = |index: u32, format: DXGI_FORMAT, offset: u32| D3D12_INPUT_ELEMENT_DESC {
            SemanticName: windows::core::PCSTR(semantic.as_ptr().cast()),
            SemanticIndex: index,
            Format: format,
            InputSlot: 0,
            AlignedByteOffset: offset,
            InputSlotClass: D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
            InstanceDataStepRate: 0,
        };
        let scene_elements = [
            element(0, DXGI_FORMAT_R32G32_FLOAT, 0),
            element(1, DXGI_FORMAT_R32G32B32A32_FLOAT, 8),
            element(2, DXGI_FORMAT_R32G32_FLOAT, 24),
            element(3, DXGI_FORMAT_R32_FLOAT, 32),
            element(4, DXGI_FORMAT_R32G32B32A32_FLOAT, 36),
            element(5, DXGI_FORMAT_R32G32_FLOAT, 52),
            element(6, DXGI_FORMAT_R32G32B32A32_FLOAT, 60),
            element(7, DXGI_FORMAT_R32G32B32A32_FLOAT, 92),
            element(8, DXGI_FORMAT_R32G32B32A32_FLOAT, 76),
        ];
        let scene_vs = D3d12Device::compile(
            vieww_shaders::library::SCENE_WGSL,
            "vs_main",
            vieww_shaders::ShaderStage::Vertex,
        )?;
        let scene_ps = D3d12Device::compile(
            vieww_shaders::library::SCENE_WGSL,
            "fs_main",
            vieww_shaders::ShaderStage::Fragment,
        )?;
        let scene_pso = device.pipeline(
            &scene_root,
            &scene_vs,
            &scene_ps,
            &scene_elements,
            TARGET_FORMAT,
            premultiplied_over(),
        )?;
        let post_vs = D3d12Device::compile(
            vieww_shaders::library::POST_WGSL,
            "vs_main",
            vieww_shaders::ShaderStage::Vertex,
        )?;
        let post_ps = D3d12Device::compile(
            vieww_shaders::library::POST_WGSL,
            "fs_main",
            vieww_shaders::ShaderStage::Fragment,
        )?;
        let post_replace_pso = device.pipeline(
            &post_root,
            &post_vs,
            &post_ps,
            &[],
            TARGET_FORMAT,
            replace(),
        )?;
        let post_over_pso = device.pipeline(
            &post_root,
            &post_vs,
            &post_ps,
            &[],
            TARGET_FORMAT,
            premultiplied_over(),
        )?;
        let clear_vs = D3d12Device::compile(
            vieww_shaders::library::CLEAR_WGSL,
            "vs_main",
            vieww_shaders::ShaderStage::Vertex,
        )?;
        let clear_ps = D3d12Device::compile(
            vieww_shaders::library::CLEAR_WGSL,
            "fs_main",
            vieww_shaders::ShaderStage::Fragment,
        )?;
        let clear_pso = device.pipeline(
            &clear_root,
            &clear_vs,
            &clear_ps,
            &[],
            TARGET_FORMAT,
            replace(),
        )?;

        // ── the atlases and the dummy ─────────────────────────────────────
        let slot_resource = |slot: Slot| -> Result<AtlasResource, D3d12Error> {
            Ok(AtlasResource {
                resource: committed(
                    &raw,
                    D3D12_HEAP_TYPE_DEFAULT,
                    &texture_desc(atlas_format(slot), 1, 1),
                    D3D12_RESOURCE_STATE_COPY_DEST,
                )?,
                version: None,
                width: 1,
                height: 1,
            })
        };
        let atlases: [AtlasResource; 4] = [
            slot_resource(Slot::Glyphs)?,
            slot_resource(Slot::Images)?,
            slot_resource(Slot::Masks)?,
            slot_resource(Slot::Ramps)?,
        ];
        let dummy = committed(
            &raw,
            D3D12_HEAP_TYPE_DEFAULT,
            &texture_desc(TARGET_FORMAT, 1, 1),
            D3D12_RESOURCE_STATE_COPY_DEST,
        )?;

        let mut renderer = Self {
            device: raw.clone(),
            queue,
            fence,
            fence_value: 0,
            scene_root,
            post_root,
            clear_root,
            scene_pso,
            post_replace_pso,
            post_over_pso,
            clear_pso,
            heap,
            increment,
            rtv_heap: {
                let desc = D3D12_DESCRIPTOR_HEAP_DESC {
                    Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                    NumDescriptors: 0,
                    Flags: D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
                    NodeMask: 0,
                };
                // SAFETY: a creation call on a live device; replaced by the
                // real one with the first frame's targets.
                unsafe { raw.CreateDescriptorHeap(&desc)? }
            },
            atlases,
            levels: Vec::new(),
            scratch: Vec::new(),
            dummy,
            size: (0, 0),
            retained: None,
            timings: FrameTimings::default(),
        };
        for slot in Slot::ALL {
            let format = atlas_format(slot);
            let zeros = vec![0u8; bytes_per_texel(format) as usize];
            renderer.upload_atlas(slot, &zeros, 1, 1, None)?;
        }
        // The dummy is never rendered into; give it one zeroed texel and the
        // rest state every post pass reads it in. One row of f16s, pitched
        // to the 256-byte alignment the copy reads.
        {
            let zeros = vec![0u8; row_pitch(8) as usize];
            let staging = renderer.upload_buffer(&zeros)?;
            let (list, allocator) = renderer.begin()?;
            renderer.copy_staging_to_texture(&list, &renderer.dummy, &staging, 1, 1, TARGET_FORMAT);
            renderer.transition_resource(
                &list,
                &renderer.dummy,
                D3D12_RESOURCE_STATE_COPY_DEST,
                D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
            );
            renderer.finish(&list)?;
            drop(allocator);
        }
        srv(
            &renderer.device,
            &renderer.heap,
            renderer.increment,
            DUMMY_SLOT,
            &renderer.dummy,
            TARGET_FORMAT,
        );
        Ok(renderer)
    }

    /// Execute `plan` at `width x height` over `clear`, with `planner`'s
    /// atlases uploaded first, and read the frame back as straight-alpha
    /// RGBA8 — `vulkan::scene::render_planned`'s contract.
    ///
    /// # Errors
    ///
    /// An incomplete plan (this backend cannot draw it either — render such
    /// a frame on the CPU rasterizer), or any D3D12 call failing.
    pub fn render_planned(
        &mut self,
        planner: &Planner,
        plan: &ScenePlan,
        width: u32,
        height: u32,
        clear: Color,
    ) -> Result<Vec<u8>, D3d12Error> {
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
    ) -> Result<(Vec<u8>, Redrawn), D3d12Error> {
        if !plan.is_complete() {
            return Err(D3d12Error::Invalid(format!(
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
    ) -> Result<(Vec<u8>, Redrawn), D3d12Error> {
        let width = width.max(1);
        let height = height.max(1);
        check_size(width, height)?;

        let exec = frame_passes(plan, width, height, clear, damage, self.retained);
        // Until this frame finishes, `L[0]` is half-drawn.
        self.retained = None;

        let started = Instant::now();
        self.ensure_targets(width, height, plan.target_depth + 1)?;
        let vertex_bytes: &[u8] = plain_bytes(&plan.vertices);
        let index_bytes: &[u8] = plain_bytes(&plan.indices);
        let vertices = self.upload_buffer(vertex_bytes)?;
        let indices = self.upload_buffer(index_bytes)?;
        #[expect(clippy::cast_precision_loss, reason = "frame extents")]
        let viewport: [f32; 4] = [width as f32, height as f32, 0.0, 0.0];
        self.timings.upload += started.elapsed();

        // ── the interpreter ─────────────────────────────────────────────
        //
        // `open` is which target OMSetRenderTargets last bound; `bound` is
        // which pipeline the list last bound — the same dedup the other two
        // backends' interpreters do, reset whenever the target switches
        // (every switch rebinds, and viewport/scissor come with each op).
        let started = Instant::now();
        let (list, allocator) = self.begin()?;
        let mut bound: Bound = Bound::None;
        let mut open: Option<TargetId> = None;
        let mut states = self.target_states();

        let viewport_rect = D3D12_VIEWPORT {
            TopLeftX: 0.0,
            TopLeftY: 0.0,
            #[expect(clippy::cast_precision_loss, reason = "frame extents")]
            Width: width as f32,
            #[expect(clippy::cast_precision_loss, reason = "frame extents")]
            Height: height as f32,
            MinDepth: 0.0,
            MaxDepth: 1.0,
        };
        let full = scissor_rect(
            vieww_gpu::PixelRect {
                x0: 0,
                y0: 0,
                x1: width as i32,
                y1: height as i32,
            },
            width,
            height,
        );
        // SAFETY below: recording onto an open list whose resources are all
        // kept alive until `finish` has waited for the GPU.
        unsafe {
            list.SetDescriptorHeaps(&[Some(self.heap.clone())]);
            list.RSSetViewports(std::slice::from_ref(&viewport_rect));
            list.RSSetScissorRects(std::slice::from_ref(&full));
            list.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        }

        // SAFETY: every raw command-list call from here to the end of the
        // loop records onto an open list whose resources are all kept alive
        // until `finish` has waited for the GPU.
        unsafe {
            for op in &exec.ops {
                match op {
                    SceneOp::Begin {
                        target,
                        clear,
                        kind: _,
                    } => {
                        self.bind_target(&list, &mut states, &mut open, *target);
                        if let Some(color) = clear {
                            let target = self.target_resource(*target);
                            list.ClearRenderTargetView(target.rtv, color, None);
                        }
                        bound = Bound::None;
                    }
                    SceneOp::ClearRegions { rects, color } => {
                        // Never a first op: the frame's begin precedes it. D3D12
                        // has no per-region clear inside a pass, so the regions
                        // draw through `clear.wgsl`, blending `Replace`.
                        if bound != Bound::Clear {
                            // SAFETY: as above.
                            list.SetGraphicsRootSignature(&self.clear_root);
                            list.SetPipelineState(&self.clear_pso);
                            bound = Bound::Clear;
                        }
                        // SAFETY: as above.
                        list.SetGraphicsRoot32BitConstants(0, 4, color.as_ptr().cast(), 0);
                        for rect in rects {
                            let sc = scissor_rect(*rect, width, height);
                            // SAFETY: as above.
                            list.RSSetScissorRects(std::slice::from_ref(&sc));
                            list.DrawInstanced(3, 1, 0, 0);
                        }
                        // SAFETY: as above.
                        list.RSSetScissorRects(std::slice::from_ref(&full));
                    }
                    SceneOp::DrawRun {
                        target,
                        first_index,
                        index_count,
                        scissor: rect,
                    } => {
                        self.bind_target(&list, &mut states, &mut open, *target);
                        if bound != Bound::Scene {
                            // SAFETY: as above.
                            list.SetGraphicsRootSignature(&self.scene_root);
                            list.SetPipelineState(&self.scene_pso);
                            list.SetGraphicsRoot32BitConstants(0, 4, viewport.as_ptr().cast(), 0);
                            list.SetGraphicsRootDescriptorTable(
                                1,
                                gpu_handle(&self.heap, self.increment, Slot::Glyphs.heap_slot()),
                            );
                            list.IASetVertexBuffers(
                                0,
                                Some(&[D3D12_VERTEX_BUFFER_VIEW {
                                    BufferLocation: vertices.GetGPUVirtualAddress(),
                                    SizeInBytes: len32(vertex_bytes).max(1),
                                    StrideInBytes: std::mem::size_of::<Vertex>() as u32,
                                }]),
                            );
                            list.IASetIndexBuffer(Some(&D3D12_INDEX_BUFFER_VIEW {
                                BufferLocation: indices.GetGPUVirtualAddress(),
                                SizeInBytes: len32(index_bytes).max(1),
                                Format:
                                    windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R32_UINT,
                            }));
                            bound = Bound::Scene;
                        }
                        let sc = scissor_rect(*rect, width, height);
                        // SAFETY: as above.
                        list.RSSetScissorRects(std::slice::from_ref(&sc));
                        list.DrawIndexedInstanced(*index_count, 1, *first_index, 0, 0);
                    }
                    SceneOp::Post {
                        pass,
                        scissors,
                        kind: _,
                    } => {
                        // The reads move to PIXEL_SHADER_RESOURCE; the write to
                        // RENDER_TARGET.
                        for read in [pass.src, pass.aux] {
                            self.transition_state(
                                &list,
                                &mut states,
                                read,
                                D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
                            );
                        }
                        self.bind_target(&list, &mut states, &mut open, pass.into);
                        // The triplet: src, aux, masks — written on the CPU,
                        // read by the GPU when the finished list executes.
                        srv(
                            &self.device,
                            &self.heap,
                            self.increment,
                            TRIPLET,
                            &self.target_resource(pass.src).resource,
                            TARGET_FORMAT,
                        );
                        srv(
                            &self.device,
                            &self.heap,
                            self.increment,
                            TRIPLET + 1,
                            &self.target_resource(pass.aux).resource,
                            TARGET_FORMAT,
                        );
                        srv(
                            &self.device,
                            &self.heap,
                            self.increment,
                            TRIPLET + 2,
                            &self.atlases[Slot::Masks.index()].resource,
                            atlas_format(Slot::Masks),
                        );
                        if bound != Bound::Post(pass.over) {
                            // SAFETY: as above.
                            list.SetGraphicsRootSignature(&self.post_root);
                            list.SetPipelineState(if pass.over {
                                &self.post_over_pso
                            } else {
                                &self.post_replace_pso
                            });
                            bound = Bound::Post(pass.over);
                        }
                        // SAFETY: as above.
                        list.SetGraphicsRootDescriptorTable(
                            1,
                            gpu_handle(&self.heap, self.increment, TRIPLET),
                        );
                        list.SetGraphicsRoot32BitConstants(
                            0,
                            32,
                            std::ptr::from_ref(&pass.push).cast(),
                            0,
                        );
                        for rect in scissors {
                            let sc = scissor_rect(*rect, width, height);
                            // SAFETY: as above.
                            list.RSSetScissorRects(std::slice::from_ref(&sc));
                            list.DrawInstanced(3, 1, 0, 0);
                        }
                    }
                }
            }
        }

        // ── the frame out: readback ─────────────────────────────────────
        let level0 = &self.levels[0];
        let pitch = row_pitch(width * 8);
        let readback = self.committed_readback(u64::from(pitch) * u64::from(height))?;
        self.transition_state(
            &list,
            &mut states,
            TargetId::Level(0),
            D3D12_RESOURCE_STATE_COPY_SOURCE,
        );
        {
            let src = D3D12_TEXTURE_COPY_LOCATION {
                pResource: ManuallyDrop::new(Some(level0.resource.clone())),
                Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    SubresourceIndex: 0,
                },
            };
            let dst = D3D12_TEXTURE_COPY_LOCATION {
                pResource: ManuallyDrop::new(Some(readback.clone())),
                Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    PlacedFootprint: D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
                        Offset: 0,
                        Footprint: D3D12_SUBRESOURCE_FOOTPRINT {
                            Format: TARGET_FORMAT,
                            Width: width,
                            Height: height,
                            Depth: 1,
                            RowPitch: pitch,
                        },
                    },
                },
            };
            // SAFETY: as above; the locations' resource references are
            // released right after.
            unsafe { list.CopyTextureRegion(&dst, 0, 0, 0, &src, None) };
            drop(ManuallyDrop::into_inner(src.pResource));
            drop(ManuallyDrop::into_inner(dst.pResource));
        }
        self.timings.record = started.elapsed();

        let submitting = Instant::now();
        self.finish(&list)?;
        drop(allocator);
        self.timings.submit = submitting.elapsed();

        let started = Instant::now();
        let row = width as usize * 4;
        let mut pixels = vec![0u8; row * height as usize];
        // SAFETY: mapping a readback buffer of `pitch * height` bytes that
        // the GPU finished writing (the fence wait in `finish`).
        unsafe {
            let mut ptr = std::ptr::null_mut();
            let range = D3D12_RANGE {
                Begin: 0,
                End: pitch as usize * height as usize,
            };
            readback.Map(0, Some(&range), Some(&mut ptr))?;
            let base = ptr.cast::<u8>();
            let nothing = D3D12_RANGE { Begin: 0, End: 0 };
            for y in 0..height as usize {
                // One row of f16s, straight-alpha RGBA8 — the shared
                // conversion, row by row past the pitch padding.
                let half: &[u16] = std::slice::from_raw_parts(
                    base.add(y * pitch as usize).cast::<u16>(),
                    width as usize * 4,
                );
                let converted = crate::scene_exec::straight_rgba8_from_rgba16f(half);
                pixels[y * row..(y + 1) * row].copy_from_slice(&converted);
            }
            readback.Unmap(0, Some(&nothing));
        }
        self.timings.readback = started.elapsed();

        self.store_target_states(states);
        self.retained = Some(exec.retained);
        Ok((pixels, exec.redrawn))
    }

    // ── the interpreter's helpers ─────────────────────────────────────────

    /// Transition `target` to `RENDER_TARGET` (if it is not already) and
    /// bind its RTV as the list's render target, tracking `open`.
    fn bind_target(
        &self,
        list: &ID3D12GraphicsCommandList,
        states: &mut Vec<D3D12_RESOURCE_STATES>,
        open: &mut Option<TargetId>,
        target: TargetId,
    ) {
        self.transition_state(list, states, target, D3D12_RESOURCE_STATE_RENDER_TARGET);
        if *open != Some(target) {
            let rtv = self.target_resource(target).rtv;
            // SAFETY: recording onto an open list; the heap outlives the
            // frame.
            unsafe { list.OMSetRenderTargets(1, Some(&rtv), false, None) };
            *open = Some(target);
        }
    }

    /// Record a transition of `target`'s resource to `state`, skipping it
    /// when the tracked state already is `state`.
    fn transition_state(
        &self,
        list: &ID3D12GraphicsCommandList,
        states: &mut Vec<D3D12_RESOURCE_STATES>,
        target: TargetId,
        state: D3D12_RESOURCE_STATES,
    ) {
        let index = self.target_index(target);
        if states[index] != state {
            let resource = self.target_resource(target);
            // SAFETY: the barrier describes the resource's tracked state.
            let barrier = super::transition(&resource.resource, states[index], state);
            // SAFETY: as above.
            unsafe { list.ResourceBarrier(std::slice::from_ref(&barrier)) };
            super::release_barrier(barrier);
            states[index] = state;
        }
    }

    /// The tracked state of every target: levels then scratch, dummy last.
    fn target_states(&self) -> Vec<D3D12_RESOURCE_STATES> {
        let mut states: Vec<D3D12_RESOURCE_STATES> = self
            .levels
            .iter()
            .chain(self.scratch.iter())
            .map(|t| t.state)
            .collect();
        states.push(D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE);
        states
    }

    fn store_target_states(&mut self, states: Vec<D3D12_RESOURCE_STATES>) {
        for (target, state) in self
            .levels
            .iter_mut()
            .chain(self.scratch.iter_mut())
            .zip(&states)
        {
            target.state = *state;
        }
    }

    /// The index of `target` in the tracked-state vector.
    fn target_index(&self, target: TargetId) -> usize {
        match target {
            TargetId::Level(n) => 2 * n,
            TargetId::Scratch(n) => 2 * n + 1,
            TargetId::Dummy => 2 * self.levels.len(),
        }
    }

    /// The target's resource, RTV and heap slot — the dummy has no RTV and
    /// never needs one (nothing renders into it).
    fn target_resource(&self, target: TargetId) -> &TargetResource {
        match target {
            TargetId::Level(n) => &self.levels[n],
            TargetId::Scratch(n) => &self.scratch[n],
            TargetId::Dummy => unreachable!("the dummy is only ever read, never bound"),
        }
    }

    // ── resources ─────────────────────────────────────────────────────────

    /// (Re)allocate the levels and scratch to `depth` targets of the frame
    /// size, cleared to transparent and resting in `PIXEL_SHADER_RESOURCE`.
    fn ensure_targets(&mut self, width: u32, height: u32, depth: usize) -> Result<(), D3d12Error> {
        if self.size != (width, height) {
            self.levels.clear();
            self.scratch.clear();
            self.size = (width, height);
        }
        if self.levels.len() >= depth {
            return Ok(());
        }
        let rtv_heap_desc = D3D12_DESCRIPTOR_HEAP_DESC {
            Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
            NumDescriptors: 2 * u32::try_from(depth).unwrap_or(0),
            Flags: D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
            NodeMask: 0,
        };
        // SAFETY: creation calls on a live device. The heap is stored — an
        // RTV must never outlive the heap it lives in.
        let rtv_heap: ID3D12DescriptorHeap =
            unsafe { self.device.CreateDescriptorHeap(&rtv_heap_desc)? };
        self.rtv_heap = rtv_heap.clone();
        let rtv_increment = unsafe {
            self.device
                .GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV)
        };
        let (list, allocator) = self.begin()?;
        let mut fresh = 0u32;
        while self.levels.len() < depth {
            let level = self.levels.len();
            let level_resource = committed(
                &self.device,
                D3D12_HEAP_TYPE_DEFAULT,
                &texture_desc(TARGET_FORMAT, width, height),
                D3D12_RESOURCE_STATE_COPY_DEST,
            )?;
            let scratch_resource = committed(
                &self.device,
                D3D12_HEAP_TYPE_DEFAULT,
                &texture_desc(TARGET_FORMAT, width, height),
                D3D12_RESOURCE_STATE_COPY_DEST,
            )?;
            // Zero-fill both (a committed resource's contents are
            // undefined), then rest them in the state a post pass reads.
            for resource in [&level_resource, &scratch_resource] {
                let bytes = vec![0u8; row_pitch(width * 8) as usize * height.max(1) as usize];
                let zeros = self.upload_buffer(&bytes)?;
                self.copy_staging_to_texture(&list, resource, &zeros, width, height, TARGET_FORMAT);
                self.transition_resource(
                    &list,
                    resource,
                    D3D12_RESOURCE_STATE_COPY_DEST,
                    D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
                );
            }
            let rtv = |offset: u32| D3D12_CPU_DESCRIPTOR_HANDLE {
                ptr: unsafe { rtv_heap.GetCPUDescriptorHandleForHeapStart() }.ptr
                    + offset as usize * rtv_increment as usize,
            };
            for resource in [&level_resource, &scratch_resource] {
                // SAFETY: a live resource and a handle inside this
                // renderer's RTV heap.
                unsafe {
                    self.device
                        .CreateRenderTargetView(resource, None, rtv(fresh));
                }
                fresh += 1;
            }
            self.levels.push(TargetResource {
                resource: level_resource,
                rtv: rtv(2 * level as u32),
                state: D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
            });
            self.scratch.push(TargetResource {
                resource: scratch_resource,
                rtv: rtv(2 * level as u32 + 1),
                state: D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
            });
        }
        self.finish(&list)?;
        drop(allocator);
        Ok(())
    }

    /// Upload every atlas `planner` holds that changed since the last
    /// upload — whole-texture uploads in v1; see the module doc.
    fn upload_planner(&mut self, planner: &Planner) -> Result<(), D3d12Error> {
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
            plain_bytes(ramps.texels()),
            vieww_gpu::RampAtlas::WIDTH,
            ramps.rows(),
            Some(ramps.version()),
        )?;
        Ok(())
    }

    /// Bring one atlas up to `bytes` at `version`: skipped when unchanged,
    /// recreated (and its SRV rewritten) when the size changed, copied into
    /// otherwise. Version `None` is the zeroed placeholder at bring-up.
    fn upload_atlas(
        &mut self,
        slot: Slot,
        bytes: &[u8],
        width: u32,
        height: u32,
        version: Option<u64>,
    ) -> Result<(), D3d12Error> {
        let index = slot.index();
        let format = atlas_format(slot);
        let (unchanged, resized) = {
            let atlas = &self.atlases[index];
            (
                version.is_some()
                    && atlas.version == version
                    && atlas.width == width
                    && atlas.height == height,
                atlas.width != width || atlas.height != height,
            )
        };
        if unchanged {
            return Ok(());
        }
        if resized {
            let fresh = committed(
                &self.device,
                D3D12_HEAP_TYPE_DEFAULT,
                &texture_desc(format, width, height),
                D3D12_RESOURCE_STATE_COPY_DEST,
            )?;
            self.atlases[index].resource = fresh;
            self.atlases[index].width = width;
            self.atlases[index].height = height;
        }
        let resource = self.atlases[index].resource.clone();
        let staging = self.upload_buffer(bytes)?;
        let (list, allocator) = self.begin()?;
        if !resized {
            // What the last upload left in PIXEL_SHADER_RESOURCE, back to
            // COPY_DEST for this one.
            self.transition_resource(
                &list,
                &resource,
                D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
                D3D12_RESOURCE_STATE_COPY_DEST,
            );
        }
        self.copy_staging_to_texture(&list, &resource, &staging, width, height, format);
        self.transition_resource(
            &list,
            &resource,
            D3D12_RESOURCE_STATE_COPY_DEST,
            D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
        );
        self.finish(&list)?;
        drop(allocator);
        if resized {
            srv(
                &self.device,
                &self.heap,
                self.increment,
                slot.heap_slot(),
                &resource,
                format,
            );
        }
        self.atlases[index].version = version;
        Ok(())
    }

    // ── the small submit/upload plumbing ──────────────────────────────────

    /// A fresh allocator and an open direct command list on this
    /// renderer's own queue.
    fn begin(&self) -> Result<(ID3D12GraphicsCommandList, ID3D12CommandAllocator), D3d12Error> {
        // SAFETY: creation calls on a live device.
        unsafe {
            let allocator: ID3D12CommandAllocator = self
                .device
                .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)?;
            let list: ID3D12GraphicsCommandList = self.device.CreateCommandList(
                0,
                D3D12_COMMAND_LIST_TYPE_DIRECT,
                &allocator,
                None,
            )?;
            Ok((list, allocator))
        }
    }

    /// Close `list`, execute it on this renderer's queue, and block until
    /// the GPU has finished it.
    fn finish(&mut self, list: &ID3D12GraphicsCommandList) -> Result<(), D3d12Error> {
        self.fence_value += 1;
        let value = self.fence_value;
        // SAFETY: a recorded list on our queue; the fence wait below keeps
        // every resource it references alive until the GPU is done.
        unsafe {
            list.Close()?;
            let lists = [Some(list.cast::<ID3D12CommandList>()?)];
            self.queue.ExecuteCommandLists(&lists);
            self.queue.Signal(&self.fence, value)?;
            if self.fence.GetCompletedValue() < value {
                let event: HANDLE = CreateEventW(None, false, false, None)?;
                let set = self.fence.SetEventOnCompletion(value, event);
                if set.is_ok() {
                    WaitForSingleObject(event, INFINITE);
                }
                CloseHandle(event)?;
                set?;
            }
            // A removed device (driver reset, TDR) reports here rather than
            // returning garbage pixels.
            self.device.GetDeviceRemovedReason()?;
        }
        Ok(())
    }

    /// An upload-heap buffer holding `bytes`.
    fn upload_buffer(&self, bytes: &[u8]) -> Result<ID3D12Resource, D3d12Error> {
        let resource = self.committed_readback_upload(bytes.len().max(1) as u64)?;
        // SAFETY: mapping subresource 0 of an upload buffer of at least
        // `bytes.len()` bytes; unmapped before anything else sees it.
        unsafe {
            let mut ptr = std::ptr::null_mut();
            resource.Map(0, None, Some(&mut ptr))?;
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.cast::<u8>(), bytes.len());
            let nothing = D3D12_RANGE { Begin: 0, End: 0 };
            resource.Unmap(0, Some(&nothing));
        }
        Ok(resource)
    }

    /// An upload-heap buffer of `size` bytes.
    fn committed_readback_upload(&self, size: u64) -> Result<ID3D12Resource, D3d12Error> {
        self.committed(
            D3D12_HEAP_TYPE_UPLOAD,
            size,
            D3D12_RESOURCE_STATE_GENERIC_READ,
        )
    }

    /// A readback-heap buffer of `size` bytes.
    fn committed_readback(&self, size: u64) -> Result<ID3D12Resource, D3d12Error> {
        self.committed(
            D3D12_HEAP_TYPE_READBACK,
            size,
            D3D12_RESOURCE_STATE_COPY_DEST,
        )
    }

    /// A committed buffer in `heap`.
    fn committed(
        &self,
        heap: D3D12_HEAP_TYPE,
        size: u64,
        state: D3D12_RESOURCE_STATES,
    ) -> Result<ID3D12Resource, D3d12Error> {
        let props = D3D12_HEAP_PROPERTIES {
            Type: heap,
            ..Default::default()
        };
        let desc = super::buffer_desc(size);
        let mut resource: Option<ID3D12Resource> = None;
        // SAFETY: valid descriptors and out-pointer.
        unsafe {
            self.device.CreateCommittedResource(
                &props,
                D3D12_HEAP_FLAG_NONE,
                &desc,
                state,
                None,
                &mut resource,
            )?;
        }
        resource.ok_or_else(|| D3d12Error::Api("CreateCommittedResource returned nothing".into()))
    }

    /// Record a transition of one raw resource (no tracked state).
    fn transition_resource(
        &self,
        list: &ID3D12GraphicsCommandList,
        resource: &ID3D12Resource,
        from: D3D12_RESOURCE_STATES,
        to: D3D12_RESOURCE_STATES,
    ) {
        // SAFETY: the barrier describes the caller's from-state.
        let barrier = super::transition(resource, from, to);
        // SAFETY: recording onto an open list.
        unsafe { list.ResourceBarrier(std::slice::from_ref(&barrier)) };
        super::release_barrier(barrier);
    }

    /// Record a copy of a whole staging buffer into `resource`, footprinted
    /// for `width x height` of `format` with 256-aligned rows — every
    /// upload path's one copy (atlas versions, the targets' zero-fill, the
    /// dummy's).
    fn copy_staging_to_texture(
        &self,
        list: &ID3D12GraphicsCommandList,
        resource: &ID3D12Resource,
        staging: &ID3D12Resource,
        width: u32,
        height: u32,
        format: DXGI_FORMAT,
    ) {
        let pitch = row_pitch(width.max(1) * bytes_per_texel(format));
        let src = D3D12_TEXTURE_COPY_LOCATION {
            pResource: ManuallyDrop::new(Some(staging.clone())),
            Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                PlacedFootprint: D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
                    Offset: 0,
                    Footprint: D3D12_SUBRESOURCE_FOOTPRINT {
                        Format: format,
                        Width: width.max(1),
                        Height: height.max(1),
                        Depth: 1,
                        RowPitch: pitch,
                    },
                },
            },
        };
        let dst = D3D12_TEXTURE_COPY_LOCATION {
            pResource: ManuallyDrop::new(Some(resource.clone())),
            Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                SubresourceIndex: 0,
            },
        };
        // SAFETY: recording onto an open list with live resources.
        unsafe { list.CopyTextureRegion(&dst, 0, 0, 0, &src, None) };
        drop(ManuallyDrop::into_inner(src.pResource));
        drop(ManuallyDrop::into_inner(dst.pResource));
    }
}

/// Which pipeline (and its roots and tables) the list last bound — the
/// interpreter's bind dedup, reset whenever the bound target switches.
#[derive(Clone, Copy, PartialEq)]
enum Bound {
    None,
    Scene,
    /// The post pipeline, and whether it is the blending one.
    Post(bool),
    Clear,
}

impl std::fmt::Debug for D3d12SceneRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("D3d12SceneRenderer")
            .field("size", &self.size)
            .field("targets", &self.levels.len())
            .finish_non_exhaustive()
    }
}

/// A committed 2D texture in the default heap.
fn committed(
    device: &ID3D12Device,
    heap: D3D12_HEAP_TYPE,
    desc: &D3D12_RESOURCE_DESC,
    state: D3D12_RESOURCE_STATES,
) -> Result<ID3D12Resource, D3d12Error> {
    let props = D3D12_HEAP_PROPERTIES {
        Type: heap,
        ..Default::default()
    };
    let mut resource: Option<ID3D12Resource> = None;
    // SAFETY: valid descriptors and out-pointer.
    unsafe {
        device.CreateCommittedResource(
            &props,
            D3D12_HEAP_FLAG_NONE,
            desc,
            state,
            None,
            &mut resource,
        )?;
    }
    resource.ok_or_else(|| D3d12Error::Api("CreateCommittedResource returned nothing".into()))
}

/// One root-constants parameter at `register`.
fn constants_parameter(register: u32, dwords: u32) -> D3D12_ROOT_PARAMETER {
    D3D12_ROOT_PARAMETER {
        ParameterType: D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS,
        Anonymous: D3D12_ROOT_PARAMETER_0 {
            Constants: D3D12_ROOT_CONSTANTS {
                ShaderRegister: register,
                RegisterSpace: 0,
                Num32BitValues: dwords,
            },
        },
        ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
    }
}

/// A traversal scissor as a D3D12_RECT, clamped to the frame — everything
/// the traversal emits is already inside it, so this is a cast with the
/// frame as a backstop.
fn scissor_rect(rect: PixelRect, width: u32, height: u32) -> windows::Win32::Foundation::RECT {
    let x0 = rect.x0.clamp(0, width as i32);
    let y0 = rect.y0.clamp(0, height as i32);
    let x1 = rect.x1.clamp(0, width as i32);
    let y1 = rect.y1.clamp(0, height as i32);
    windows::Win32::Foundation::RECT {
        left: x0,
        top: y0,
        right: x1.max(x0),
        bottom: y1.max(y0),
    }
}

/// The bytes of a plain-data slice — `vulkan::scene`'s `as_bytes`, for the
/// same all-`f32`/`u32` no-padding types.
fn plain_bytes<T: Copy>(values: &[T]) -> &[u8] {
    // SAFETY: `T` is `Copy` with no padding; the slice is the same memory
    // read as bytes.
    unsafe { std::slice::from_raw_parts(values.as_ptr().cast(), std::mem::size_of_val(values)) }
}
