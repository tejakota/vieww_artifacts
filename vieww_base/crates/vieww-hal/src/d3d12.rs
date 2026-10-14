//! The Direct3D 12 backend (spec §4.3, §11.3) — Windows.
//!
//! # What is here
//!
//! [`D3d12Device`] picks an adapter, creates a device and a direct queue, and
//! implements the headless pair the Vulkan suites are built on:
//!
//! - [`HalDevice::render_clear_to_pixels`] — clear a render target, copy it
//!   to a readback buffer, return straight-alpha RGBA8;
//! - [`D3d12Device::render_mesh_to_pixels`] — caller triangles through
//!   `solid.wgsl`, the same source the Vulkan path compiles to SPIR-V and the
//!   Metal path to MSL, here translated to HLSL by `vieww-shaders` and
//!   compiled with the system's `d3dcompiler_47.dll` (shader model 5.1,
//!   which every Windows 10 and later ships — no DXC redistributable).
//!
//! # Adapters, and WARP
//!
//! The first hardware adapter that accepts feature level 11.0 wins. WARP —
//! Microsoft's CPU implementation of D3D12 — is skipped unless
//! `VIEWW_D3D12_ALLOW_WARP=1`, because a capability probe that says "found a
//! GPU" about a software rasteriser is lying. CI sets it: GitHub's Windows
//! runners have no GPU, and WARP is a conformant D3D12 driver, which makes it
//! the D3D12 counterpart of the lavapipe runs on Linux — correctness
//! evidence, never performance evidence. [`AdapterInfo::device_type`] says
//! `"software"` when it is in use.
//!
//! # Resource binding
//!
//! One root signature for the mesh path: eight 32-bit root constants at
//! register `b0`, which is where naga puts `solid.wgsl`'s
//! `@group(0) @binding(0) var<uniform>` (see
//! [`vieww_shaders::hlsl_slots`]). Root constants back a constant buffer
//! directly, so the uniform needs no heap, no descriptor and no upload.
//!
//! # Verification
//!
//! Type-checked from Linux by `ci/check/cross-check.sh windows`, and run by
//! `tests/d3d12_smoke.rs` on the Windows leg of
//! `.github/workflows/framework-checks.yml`.

use std::mem::ManuallyDrop;
use std::sync::Mutex;

use super::{sealed, AdapterInfo, Device as HalDevice};
use vieww_foundation::Color;
use vieww_shaders::{hlsl_slots, ParsedShader, ShaderStage};
use windows::core::{Interface, PCSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Graphics::Direct3D::Fxc::{
    D3DCompile, D3DCOMPILE_ENABLE_STRICTNESS, D3DCOMPILE_OPTIMIZATION_LEVEL3,
};
use windows::Win32::Graphics::Direct3D::{
    ID3DBlob, D3D_FEATURE_LEVEL_11_0, D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
};
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_R32G32_FLOAT, DXGI_FORMAT_R32_UINT, DXGI_FORMAT_R8G8B8A8_UNORM,
    DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory4, DXGI_ADAPTER_FLAG_SOFTWARE,
};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject, INFINITE};

/// What can go wrong on this backend.
#[derive(Debug)]
pub enum D3d12Error {
    /// No adapter accepted feature level 11.0 (WARP included only when
    /// `VIEWW_D3D12_ALLOW_WARP=1`).
    NoAdapter,
    /// A DXGI/D3D12 call returned a failing `HRESULT`; the text includes it.
    Api(String),
    /// WGSL → HLSL translation, or `D3DCompile`, failed.
    Shader(String),
    /// A request this backend will not attempt.
    Invalid(String),
}

impl std::fmt::Display for D3d12Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAdapter => write!(f, "no usable D3D12 adapter found"),
            Self::Api(message) => write!(f, "D3D12/DXGI call failed: {message}"),
            Self::Shader(message) => write!(f, "shader: {message}"),
            Self::Invalid(message) => write!(f, "invalid request: {message}"),
        }
    }
}

impl std::error::Error for D3d12Error {}

impl From<windows::core::Error> for D3d12Error {
    fn from(error: windows::core::Error) -> Self {
        Self::Api(error.to_string())
    }
}

/// The queue and its fence: submission is serialised through one lock, so
/// the device is `Sync` without the queue having to be.
struct Submit {
    queue: ID3D12CommandQueue,
    fence: ID3D12Fence,
    value: u64,
}

/// A D3D12 device and a direct command queue.
pub struct D3d12Device {
    device: ID3D12Device,
    submit: Mutex<Submit>,
    description: String,
    software: bool,
}

// SAFETY: `ID3D12Device` is free-threaded (`Send + Sync` in `windows`); the
// queue and fence are only ever touched under `submit`'s lock.
unsafe impl Sync for D3d12Device {}

impl std::fmt::Debug for D3d12Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("D3d12Device")
            .field("description", &self.description)
            .field("software", &self.software)
            .finish_non_exhaustive()
    }
}

impl sealed::Sealed for D3d12Device {}

/// Decode a DXGI adapter description's fixed UTF-16 buffer.
fn decode_description(buffer: [u16; 128]) -> String {
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..len])
}

fn warp_allowed() -> bool {
    std::env::var("VIEWW_D3D12_ALLOW_WARP").is_ok_and(|v| v == "1")
}

/// `D3D12CreateDevice` at feature level 11.0 on `adapter`.
fn create_device(adapter: &IDXGIAdapter1) -> Option<ID3D12Device> {
    let mut device: Option<ID3D12Device> = None;
    // SAFETY: a live adapter and a valid out-pointer.
    let created = unsafe { D3D12CreateDevice(adapter, D3D_FEATURE_LEVEL_11_0, &mut device) };
    created.ok().and(device)
}

impl D3d12Device {
    /// The first hardware adapter that accepts feature level 11.0 — or WARP,
    /// when no hardware adapter does and `VIEWW_D3D12_ALLOW_WARP=1`.
    ///
    /// # Errors
    ///
    /// [`D3d12Error::NoAdapter`], or a failing DXGI/D3D12 call.
    pub fn new() -> Result<Self, D3d12Error> {
        // SAFETY: an argument-free COM factory.
        let factory: IDXGIFactory4 = unsafe { CreateDXGIFactory1()? };
        let mut chosen = None;
        let mut index = 0u32;
        // SAFETY: enumeration ends at the first failing index.
        while let Ok(adapter) = unsafe { factory.EnumAdapters1(index) } {
            index += 1;
            // SAFETY: fills a plain struct from a live adapter.
            let Ok(desc) = (unsafe { adapter.GetDesc1() }) else {
                continue;
            };
            if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                continue;
            }
            if let Some(device) = create_device(&adapter) {
                chosen = Some((device, decode_description(desc.Description), false));
                break;
            }
        }
        if chosen.is_none() && warp_allowed() {
            // SAFETY: as above.
            let warp: IDXGIAdapter1 = unsafe { factory.EnumWarpAdapter()? };
            // SAFETY: as above.
            let desc = unsafe { warp.GetDesc1()? };
            if let Some(device) = create_device(&warp) {
                chosen = Some((device, decode_description(desc.Description), true));
            }
        }
        let (device, description, software) = chosen.ok_or(D3d12Error::NoAdapter)?;
        let queue_desc = D3D12_COMMAND_QUEUE_DESC {
            Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
            Priority: 0,
            Flags: D3D12_COMMAND_QUEUE_FLAG_NONE,
            NodeMask: 0,
        };
        // SAFETY: a valid descriptor on a live device.
        let queue: ID3D12CommandQueue = unsafe { device.CreateCommandQueue(&queue_desc)? };
        // SAFETY: as above.
        let fence: ID3D12Fence = unsafe { device.CreateFence(0, D3D12_FENCE_FLAG_NONE)? };
        Ok(Self {
            device,
            submit: Mutex::new(Submit {
                queue,
                fence,
                value: 0,
            }),
            description,
            software,
        })
    }

    /// Whether this device is WARP rather than a GPU.
    #[must_use]
    pub const fn is_software(&self) -> bool {
        self.software
    }

    /// The underlying device.
    #[must_use]
    pub const fn raw(&self) -> &ID3D12Device {
        &self.device
    }

    /// A fresh allocator and an open direct command list on it.
    fn begin(&self) -> Result<(ID3D12CommandAllocator, ID3D12GraphicsCommandList), D3d12Error> {
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
            Ok((allocator, list))
        }
    }

    /// Close `list`, execute it, and block until the GPU has finished it.
    fn finish(&self, list: &ID3D12GraphicsCommandList) -> Result<(), D3d12Error> {
        let mut submit = self
            .submit
            .lock()
            .map_err(|_| D3d12Error::Invalid("queue lock poisoned".into()))?;
        submit.value += 1;
        let value = submit.value;
        // SAFETY: a recorded list on our queue; the fence wait below keeps
        // every resource it references alive until the GPU is done.
        unsafe {
            list.Close()?;
            let lists = [Some(list.cast::<ID3D12CommandList>()?)];
            submit.queue.ExecuteCommandLists(&lists);
            submit.queue.Signal(&submit.fence, value)?;
            if submit.fence.GetCompletedValue() < value {
                let event: HANDLE = CreateEventW(None, false, false, None)?;
                let set = submit.fence.SetEventOnCompletion(value, event);
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

    /// A committed resource in `heap` of `desc`, created in `state`.
    fn committed(
        &self,
        heap: D3D12_HEAP_TYPE,
        desc: &D3D12_RESOURCE_DESC,
        state: D3D12_RESOURCE_STATES,
        clear: Option<&D3D12_CLEAR_VALUE>,
    ) -> Result<ID3D12Resource, D3d12Error> {
        let props = D3D12_HEAP_PROPERTIES {
            Type: heap,
            ..Default::default()
        };
        let mut resource: Option<ID3D12Resource> = None;
        // SAFETY: valid descriptors and out-pointer.
        unsafe {
            self.device.CreateCommittedResource(
                &props,
                D3D12_HEAP_FLAG_NONE,
                desc,
                state,
                clear.map(std::ptr::from_ref),
                &mut resource,
            )?;
        }
        resource.ok_or_else(|| D3d12Error::Api("CreateCommittedResource returned nothing".into()))
    }

    /// An upload-heap buffer holding `bytes`.
    fn upload_buffer(&self, bytes: &[u8]) -> Result<ID3D12Resource, D3d12Error> {
        let resource = self.committed(
            D3D12_HEAP_TYPE_UPLOAD,
            &buffer_desc(bytes.len().max(1) as u64),
            D3D12_RESOURCE_STATE_GENERIC_READ,
            None,
        )?;
        // SAFETY: mapping subresource 0 of an upload buffer of at least
        // `bytes.len()` bytes; unmapped before anything else sees it.
        unsafe {
            let mut ptr = std::ptr::null_mut();
            resource.Map(0, None, Some(&mut ptr))?;
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.cast::<u8>(), bytes.len());
            resource.Unmap(0, None);
        }
        Ok(resource)
    }

    /// A render target of `width x height` RGBA8, its RTV heap and handle,
    /// created in the render-target state.
    fn target(
        &self,
        width: u32,
        height: u32,
        clear: [f32; 4],
    ) -> Result<
        (
            ID3D12Resource,
            ID3D12DescriptorHeap,
            D3D12_CPU_DESCRIPTOR_HANDLE,
        ),
        D3d12Error,
    > {
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Alignment: 0,
            Width: u64::from(width),
            Height: height,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
            Flags: D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
        };
        let clear_value = D3D12_CLEAR_VALUE {
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            Anonymous: D3D12_CLEAR_VALUE_0 { Color: clear },
        };
        let texture = self.committed(
            D3D12_HEAP_TYPE_DEFAULT,
            &desc,
            D3D12_RESOURCE_STATE_RENDER_TARGET,
            Some(&clear_value),
        )?;
        let heap_desc = D3D12_DESCRIPTOR_HEAP_DESC {
            Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
            NumDescriptors: 1,
            Flags: D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
            NodeMask: 0,
        };
        // SAFETY: creation calls on a live device with valid descriptors.
        unsafe {
            let heap: ID3D12DescriptorHeap = self.device.CreateDescriptorHeap(&heap_desc)?;
            let rtv = heap.GetCPUDescriptorHandleForHeapStart();
            self.device.CreateRenderTargetView(&texture, None, rtv);
            Ok((texture, heap, rtv))
        }
    }

    /// Record: transition `texture` from render target to copy source and
    /// copy it into a fresh readback buffer. Returns the buffer and the
    /// row pitch the copy used.
    fn record_readback(
        &self,
        list: &ID3D12GraphicsCommandList,
        texture: &ID3D12Resource,
        width: u32,
        height: u32,
    ) -> Result<(ID3D12Resource, u32), D3d12Error> {
        // D3D12 requires 256-byte-aligned rows in a buffer copy.
        let pitch = (width * 4).div_ceil(D3D12_TEXTURE_DATA_PITCH_ALIGNMENT)
            * D3D12_TEXTURE_DATA_PITCH_ALIGNMENT;
        let readback = self.committed(
            D3D12_HEAP_TYPE_READBACK,
            &buffer_desc(u64::from(pitch) * u64::from(height)),
            D3D12_RESOURCE_STATE_COPY_DEST,
            None,
        )?;
        let barrier = transition(
            texture,
            D3D12_RESOURCE_STATE_RENDER_TARGET,
            D3D12_RESOURCE_STATE_COPY_SOURCE,
        );
        let src = D3D12_TEXTURE_COPY_LOCATION {
            pResource: ManuallyDrop::new(Some(texture.clone())),
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
                        Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                        Width: width,
                        Height: height,
                        Depth: 1,
                        RowPitch: pitch,
                    },
                },
            },
        };
        // SAFETY: recording onto an open list; the copy locations' resource
        // references are released right after.
        unsafe {
            list.ResourceBarrier(std::slice::from_ref(&barrier));
            list.CopyTextureRegion(&dst, 0, 0, 0, &src, None);
        }
        release_barrier(barrier);
        drop(ManuallyDrop::into_inner(src.pResource));
        drop(ManuallyDrop::into_inner(dst.pResource));
        Ok((readback, pitch))
    }

    /// Read a finished readback buffer into tight RGBA8 rows.
    fn read_rows(
        readback: &ID3D12Resource,
        width: u32,
        height: u32,
        pitch: u32,
    ) -> Result<Vec<u8>, D3d12Error> {
        let row = width as usize * 4;
        let mut out = vec![0u8; row * height as usize];
        // SAFETY: mapping a readback buffer of `pitch * height` bytes that
        // the GPU has finished writing (the fence wait in `finish`).
        unsafe {
            let mut ptr = std::ptr::null_mut();
            let range = D3D12_RANGE {
                Begin: 0,
                End: pitch as usize * height as usize,
            };
            readback.Map(0, Some(&range), Some(&mut ptr))?;
            let base = ptr.cast::<u8>();
            for y in 0..height as usize {
                std::ptr::copy_nonoverlapping(
                    base.add(y * pitch as usize),
                    out.as_mut_ptr().add(y * row),
                    row,
                );
            }
            let nothing_written = D3D12_RANGE { Begin: 0, End: 0 };
            readback.Unmap(0, Some(&nothing_written));
        }
        Ok(out)
    }

    /// Translate and compile one entry point of `wgsl` to DXBC.
    fn compile(wgsl: &str, entry: &str, stage: ShaderStage) -> Result<ID3DBlob, D3d12Error> {
        let parsed = ParsedShader::parse(wgsl).map_err(|e| D3d12Error::Shader(e.to_string()))?;
        let hlsl = parsed
            .to_hlsl(entry, stage)
            .map_err(|e| D3d12Error::Shader(e.to_string()))?;
        let target: &[u8] = if stage == ShaderStage::Vertex {
            b"vs_5_1\0"
        } else {
            b"ps_5_1\0"
        };
        let entry_c = std::ffi::CString::new(entry)
            .map_err(|_| D3d12Error::Invalid("entry point name".into()))?;
        let mut code: Option<ID3DBlob> = None;
        let mut errors: Option<ID3DBlob> = None;
        // SAFETY: the source pointer and length describe `hlsl`; every
        // string argument is NUL-terminated and outlives the call.
        let compiled = unsafe {
            D3DCompile(
                hlsl.as_ptr().cast(),
                hlsl.len(),
                PCSTR(c"vieww.hlsl".as_ptr().cast()),
                None,
                None,
                PCSTR(entry_c.as_ptr().cast()),
                PCSTR(target.as_ptr()),
                D3DCOMPILE_ENABLE_STRICTNESS | D3DCOMPILE_OPTIMIZATION_LEVEL3,
                0,
                &mut code,
                Some(&mut errors),
            )
        };
        if let Err(error) = compiled {
            let log = errors.map(|b| blob_text(&b)).unwrap_or_default();
            return Err(D3d12Error::Shader(format!(
                "D3DCompile({entry}): {error}: {log}"
            )));
        }
        code.ok_or_else(|| D3d12Error::Shader("D3DCompile produced no code".into()))
    }

    /// The mesh path's root signature: eight root constants at `b0`.
    fn solid_root_signature(&self) -> Result<ID3D12RootSignature, D3d12Error> {
        let parameter = D3D12_ROOT_PARAMETER {
            ParameterType: D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS,
            Anonymous: D3D12_ROOT_PARAMETER_0 {
                Constants: D3D12_ROOT_CONSTANTS {
                    ShaderRegister: 0,
                    RegisterSpace: 0,
                    Num32BitValues: 8,
                },
            },
            ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
        };
        let desc = D3D12_ROOT_SIGNATURE_DESC {
            NumParameters: 1,
            pParameters: &parameter,
            NumStaticSamplers: 0,
            pStaticSamplers: std::ptr::null(),
            Flags: D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT,
        };
        self.root_signature(&desc)
    }

    /// Serialise and create a root signature.
    ///
    /// # Errors
    ///
    /// Serialisation or creation failing.
    pub fn root_signature(
        &self,
        desc: &D3D12_ROOT_SIGNATURE_DESC,
    ) -> Result<ID3D12RootSignature, D3d12Error> {
        let mut blob: Option<ID3DBlob> = None;
        let mut errors: Option<ID3DBlob> = None;
        // SAFETY: a valid descriptor whose pointers outlive the call.
        let serialised = unsafe {
            D3D12SerializeRootSignature(
                desc,
                D3D_ROOT_SIGNATURE_VERSION_1,
                &mut blob,
                Some(&mut errors),
            )
        };
        if let Err(error) = serialised {
            let log = errors.map(|b| blob_text(&b)).unwrap_or_default();
            return Err(D3d12Error::Api(format!("root signature: {error}: {log}")));
        }
        let blob = blob.ok_or_else(|| D3d12Error::Api("empty root signature".into()))?;
        // SAFETY: the blob's bytes are a serialised root signature.
        unsafe { Ok(self.device.CreateRootSignature(0, blob_bytes(&blob))?) }
    }

    /// Rasterise a triangle mesh in one flat colour — the D3D12 twin of
    /// `vulkan::VulkanDevice::render_mesh_to_pixels`: same shader source,
    /// same straight-alpha blend, same transparent clear.
    ///
    /// # Errors
    ///
    /// A zero-sized frame, an index out of range, or D3D12 failing.
    pub fn render_mesh_to_pixels(
        &self,
        width: u32,
        height: u32,
        positions: &[[f32; 2]],
        indices: &[u32],
        color: Color,
    ) -> Result<Vec<u8>, D3d12Error> {
        check_size(width, height)?;
        if positions.is_empty() || indices.is_empty() {
            return Ok(vec![0u8; width as usize * height as usize * 4]);
        }
        if let Some(bad) = indices.iter().find(|&&i| i as usize >= positions.len()) {
            return Err(D3d12Error::Invalid(format!(
                "index {bad} with {} positions",
                positions.len()
            )));
        }
        let root = self.solid_root_signature()?;
        let vs = Self::compile(
            vieww_shaders::library::SOLID_WGSL,
            "vs_main",
            ShaderStage::Vertex,
        )?;
        let ps = Self::compile(
            vieww_shaders::library::SOLID_WGSL,
            "fs_main",
            ShaderStage::Fragment,
        )?;
        let semantic = std::ffi::CString::new(hlsl_slots::VERTEX_SEMANTIC)
            .map_err(|_| D3d12Error::Invalid("semantic".into()))?;
        let elements = [D3D12_INPUT_ELEMENT_DESC {
            SemanticName: PCSTR(semantic.as_ptr().cast()),
            SemanticIndex: 0,
            Format: DXGI_FORMAT_R32G32_FLOAT,
            InputSlot: 0,
            AlignedByteOffset: 0,
            InputSlotClass: D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
            InstanceDataStepRate: 0,
        }];
        let pso = self.pipeline(
            &root,
            &vs,
            &ps,
            &elements,
            DXGI_FORMAT_R8G8B8A8_UNORM,
            straight_over(),
        )?;

        let vertex_bytes: Vec<u8> = positions
            .iter()
            .flat_map(|p| p.iter().flat_map(|v| v.to_le_bytes()))
            .collect();
        let index_bytes: Vec<u8> = indices.iter().flat_map(|i| i.to_le_bytes()).collect();
        let vertices = self.upload_buffer(&vertex_bytes)?;
        let index_buffer = self.upload_buffer(&index_bytes)?;
        let (texture, _heap, rtv) = self.target(width, height, [0.0; 4])?;
        #[expect(clippy::cast_precision_loss, reason = "frame sizes < 2^24")]
        let constants: [f32; 8] = [
            f32::from(color.r) / 255.0,
            f32::from(color.g) / 255.0,
            f32::from(color.b) / 255.0,
            f32::from(color.a) / 255.0,
            width as f32,
            height as f32,
            0.0,
            0.0,
        ];

        let (_allocator, list) = self.begin()?;
        #[expect(clippy::cast_precision_loss, reason = "frame sizes < 2^24")]
        let viewport = D3D12_VIEWPORT {
            TopLeftX: 0.0,
            TopLeftY: 0.0,
            Width: width as f32,
            Height: height as f32,
            MinDepth: 0.0,
            MaxDepth: 1.0,
        };
        let scissor = windows::Win32::Foundation::RECT {
            left: 0,
            top: 0,
            right: i32::try_from(width).unwrap_or(i32::MAX),
            bottom: i32::try_from(height).unwrap_or(i32::MAX),
        };
        // SAFETY: recording onto an open list with live resources; every
        // resource is kept alive until `finish` has waited for the GPU.
        unsafe {
            list.ClearRenderTargetView(rtv, &[0.0; 4], None);
            list.OMSetRenderTargets(1, Some(&rtv), false, None);
            list.SetGraphicsRootSignature(&root);
            list.SetPipelineState(&pso);
            list.SetGraphicsRoot32BitConstants(0, 8, constants.as_ptr().cast(), 0);
            list.RSSetViewports(&[viewport]);
            list.RSSetScissorRects(&[scissor]);
            list.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            list.IASetVertexBuffers(
                0,
                Some(&[D3D12_VERTEX_BUFFER_VIEW {
                    BufferLocation: vertices.GetGPUVirtualAddress(),
                    SizeInBytes: u32::try_from(vertex_bytes.len())
                        .map_err(|_| D3d12Error::Invalid("vertex buffer too large".into()))?,
                    StrideInBytes: 8,
                }]),
            );
            list.IASetIndexBuffer(Some(&D3D12_INDEX_BUFFER_VIEW {
                BufferLocation: index_buffer.GetGPUVirtualAddress(),
                SizeInBytes: u32::try_from(index_bytes.len())
                    .map_err(|_| D3d12Error::Invalid("index buffer too large".into()))?,
                Format: DXGI_FORMAT_R32_UINT,
            }));
            list.DrawIndexedInstanced(
                u32::try_from(indices.len())
                    .map_err(|_| D3d12Error::Invalid("too many indices".into()))?,
                1,
                0,
                0,
                0,
            );
        }
        let (readback, pitch) = self.record_readback(&list, &texture, width, height)?;
        self.finish(&list)?;
        Self::read_rows(&readback, width, height, pitch)
    }

    /// A graphics pipeline: triangle lists, no culling, no depth, one
    /// render target of `format` blended by `blend`.
    ///
    /// # Errors
    ///
    /// Pipeline creation failing.
    pub fn pipeline(
        &self,
        root: &ID3D12RootSignature,
        vs: &ID3DBlob,
        ps: &ID3DBlob,
        elements: &[D3D12_INPUT_ELEMENT_DESC],
        format: DXGI_FORMAT,
        blend: D3D12_RENDER_TARGET_BLEND_DESC,
    ) -> Result<ID3D12PipelineState, D3d12Error> {
        let mut rtv_formats = [DXGI_FORMAT_UNKNOWN; 8];
        rtv_formats[0] = format;
        let mut blend_desc = D3D12_BLEND_DESC {
            AlphaToCoverageEnable: false.into(),
            IndependentBlendEnable: false.into(),
            RenderTarget: [D3D12_RENDER_TARGET_BLEND_DESC::default(); 8],
        };
        blend_desc.RenderTarget[0] = blend;
        let desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
            pRootSignature: ManuallyDrop::new(Some(root.clone())),
            VS: bytecode(vs),
            PS: bytecode(ps),
            BlendState: blend_desc,
            SampleMask: u32::MAX,
            RasterizerState: D3D12_RASTERIZER_DESC {
                FillMode: D3D12_FILL_MODE_SOLID,
                CullMode: D3D12_CULL_MODE_NONE,
                FrontCounterClockwise: false.into(),
                DepthBias: 0,
                DepthBiasClamp: 0.0,
                SlopeScaledDepthBias: 0.0,
                DepthClipEnable: true.into(),
                MultisampleEnable: false.into(),
                AntialiasedLineEnable: false.into(),
                ForcedSampleCount: 0,
                ConservativeRaster: D3D12_CONSERVATIVE_RASTERIZATION_MODE_OFF,
            },
            DepthStencilState: D3D12_DEPTH_STENCIL_DESC::default(),
            InputLayout: D3D12_INPUT_LAYOUT_DESC {
                pInputElementDescs: if elements.is_empty() {
                    std::ptr::null()
                } else {
                    elements.as_ptr()
                },
                NumElements: u32::try_from(elements.len()).unwrap_or(0),
            },
            PrimitiveTopologyType: D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
            NumRenderTargets: 1,
            RTVFormats: rtv_formats,
            DSVFormat: DXGI_FORMAT_UNKNOWN,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            ..Default::default()
        };
        // SAFETY: a fully initialised descriptor whose pointers outlive the
        // call.
        let pso = unsafe { self.device.CreateGraphicsPipelineState(&desc) };
        drop(ManuallyDrop::into_inner(desc.pRootSignature));
        Ok(pso?)
    }
}

/// Straight-alpha source-over — the Vulkan mesh pipeline's blend.
#[must_use]
pub fn straight_over() -> D3D12_RENDER_TARGET_BLEND_DESC {
    D3D12_RENDER_TARGET_BLEND_DESC {
        BlendEnable: true.into(),
        LogicOpEnable: false.into(),
        SrcBlend: D3D12_BLEND_SRC_ALPHA,
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

fn bytecode(blob: &ID3DBlob) -> D3D12_SHADER_BYTECODE {
    // SAFETY: a live blob; the pointer is used only while `blob` lives.
    unsafe {
        D3D12_SHADER_BYTECODE {
            pShaderBytecode: blob.GetBufferPointer(),
            BytecodeLength: blob.GetBufferSize(),
        }
    }
}

fn blob_bytes(blob: &ID3DBlob) -> &[u8] {
    // SAFETY: a blob's buffer is `GetBufferSize` bytes for its lifetime.
    unsafe {
        std::slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize())
    }
}

fn blob_text(blob: &ID3DBlob) -> String {
    String::from_utf8_lossy(blob_bytes(blob))
        .trim_end_matches('\0')
        .trim()
        .to_string()
}

fn buffer_desc(size: u64) -> D3D12_RESOURCE_DESC {
    D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
        Alignment: 0,
        Width: size,
        Height: 1,
        DepthOrArraySize: 1,
        MipLevels: 1,
        Format: DXGI_FORMAT_UNKNOWN,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        Flags: D3D12_RESOURCE_FLAG_NONE,
    }
}

/// A transition barrier holding its own reference to `resource`; release
/// it with [`release_barrier`] once recorded.
#[must_use]
pub fn transition(
    resource: &ID3D12Resource,
    before: D3D12_RESOURCE_STATES,
    after: D3D12_RESOURCE_STATES,
) -> D3D12_RESOURCE_BARRIER {
    D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
        Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        Anonymous: D3D12_RESOURCE_BARRIER_0 {
            Transition: ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                pResource: ManuallyDrop::new(Some(resource.clone())),
                Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                StateBefore: before,
                StateAfter: after,
            }),
        },
    }
}

/// Drop the resource reference a [`transition`] barrier holds.
pub fn release_barrier(barrier: D3D12_RESOURCE_BARRIER) {
    // SAFETY: built by `transition`, so the union holds a transition.
    let transition = unsafe { ManuallyDrop::into_inner(barrier.Anonymous.Transition) };
    drop(ManuallyDrop::into_inner(transition.pResource));
}

fn check_size(width: u32, height: u32) -> Result<(), D3d12Error> {
    if width == 0 || height == 0 || width > 16_384 || height > 16_384 {
        return Err(D3d12Error::Invalid(format!("{width}x{height} frame")));
    }
    Ok(())
}

impl HalDevice for D3d12Device {
    type Error = D3d12Error;

    fn info(&self) -> AdapterInfo {
        AdapterInfo {
            name: self.description.clone(),
            backend: "d3d12",
            device_type: if self.software {
                "software"
            } else {
                "hardware"
            }
            .to_string(),
        }
    }

    fn render_clear_to_pixels(
        &self,
        width: u32,
        height: u32,
        clear_color: Color,
    ) -> Result<Vec<u8>, D3d12Error> {
        check_size(width, height)?;
        let clear = [
            f32::from(clear_color.r) / 255.0,
            f32::from(clear_color.g) / 255.0,
            f32::from(clear_color.b) / 255.0,
            f32::from(clear_color.a) / 255.0,
        ];
        let (texture, _heap, rtv) = self.target(width, height, clear)?;
        let (_allocator, list) = self.begin()?;
        // SAFETY: recording onto an open list.
        unsafe { list.ClearRenderTargetView(rtv, &clear, None) };
        let (readback, pitch) = self.record_readback(&list, &texture, width, height)?;
        self.finish(&list)?;
        Self::read_rows(&readback, width, height, pitch)
    }
}
