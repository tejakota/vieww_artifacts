//! Compute shaders — the WebGPU row's "compute shaders for GPU computation
//! … GPU physics and particle systems previously impossible", TouchDesigner's
//! POPs, Unity's compute buffers, Unreal Niagara's GPU simulation stage.
//!
//! A [`ComputeKernel`] is one WGSL `@compute` entry point, translated through
//! the same `vieww-shaders` → naga → SPIR-V path as every graphics shader,
//! bound to `@group(0)` storage buffers at consecutive `@binding`s. A
//! [`ComputeBuffer`] is host-visible, coherent device memory — upload,
//! dispatch, read back, no staging — which is right for the sizes a UI
//! framework's simulations use and keeps the API to three calls:
//!
//! ```no_run
//! # use vieww_hal::vulkan::{VulkanDevice, compute::{ComputeKernel, ComputeBuffer}};
//! # fn f() -> Result<(), Box<dyn std::error::Error>> {
//! let gpu = VulkanDevice::new()?;
//! let wgsl = "@group(0) @binding(0) var<storage, read_write> v: array<f32>;
//!             @compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
//!                 v[id.x] = v[id.x] * 2.0;
//!             }";
//! let kernel = ComputeKernel::new(&gpu, wgsl, "main", 1)?;
//! let buf = ComputeBuffer::from_f32(&gpu, &[1.0, 2.0, 3.0])?;
//! kernel.dispatch(&gpu, &[&buf], [1, 1, 1])?;
//! assert_eq!(buf.read_f32(&gpu)?, vec![2.0, 4.0, 6.0]);
//! # Ok(()) }
//! ```
//!
//! Dispatches are synchronous (submit, wait on a fence): a frame-graph
//! integration that overlaps them with rendering is the render graph's job,
//! and the [`ComputeKernel::dispatch_many`] form records several dispatches
//! with barriers between them in one submission, which is what an iterative
//! simulation needs.

use ash::vk;

use super::{translate_wgsl_to_spirv, VulkanDevice, VulkanError};

fn err(e: vk::Result) -> VulkanError {
    VulkanError::Vulkan(e.to_string())
}

/// A host-visible storage buffer.
pub struct ComputeBuffer {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    size: u64,
    device: ash::Device,
}

impl std::fmt::Debug for ComputeBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComputeBuffer")
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

impl ComputeBuffer {
    /// An uninitialised buffer of `size` bytes (rounded up to 4).
    ///
    /// # Errors
    /// Allocation failures.
    pub fn new(gpu: &VulkanDevice, size: u64) -> Result<Self, VulkanError> {
        let size = size.max(4).div_ceil(4) * 4;
        let device = gpu.raw_device();
        let info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(vk::BufferUsageFlags::STORAGE_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer = unsafe { device.create_buffer(&info, None) }.map_err(err)?;
        let req = unsafe { device.get_buffer_memory_requirements(buffer) };
        let ty = gpu.find_memory_type(
            req.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        let alloc = vk::MemoryAllocateInfo::default()
            .allocation_size(req.size)
            .memory_type_index(ty);
        let memory = unsafe { device.allocate_memory(&alloc, None) }.map_err(|e| {
            unsafe { device.destroy_buffer(buffer, None) };
            err(e)
        })?;
        unsafe { device.bind_buffer_memory(buffer, memory, 0) }.map_err(err)?;
        Ok(Self {
            buffer,
            memory,
            size,
            device: device.clone(),
        })
    }

    /// A buffer holding `bytes`.
    ///
    /// # Errors
    /// Allocation or mapping failures.
    pub fn from_bytes(gpu: &VulkanDevice, bytes: &[u8]) -> Result<Self, VulkanError> {
        let b = Self::new(gpu, bytes.len() as u64)?;
        b.write(bytes)?;
        Ok(b)
    }

    /// A buffer holding `data` (little-endian `f32`s).
    ///
    /// # Errors
    /// As [`from_bytes`](Self::from_bytes).
    pub fn from_f32(gpu: &VulkanDevice, data: &[f32]) -> Result<Self, VulkanError> {
        let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();
        Self::from_bytes(gpu, &bytes)
    }

    /// A buffer holding `data` (little-endian `u32`s).
    ///
    /// # Errors
    /// As [`from_bytes`](Self::from_bytes).
    pub fn from_u32(gpu: &VulkanDevice, data: &[u32]) -> Result<Self, VulkanError> {
        let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();
        Self::from_bytes(gpu, &bytes)
    }

    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }

    /// Overwrite the start of the buffer.
    ///
    /// # Errors
    /// If `bytes` is longer than the buffer, or mapping fails.
    pub fn write(&self, bytes: &[u8]) -> Result<(), VulkanError> {
        if bytes.len() as u64 > self.size {
            return Err(VulkanError::Vulkan("write larger than the buffer".into()));
        }
        unsafe {
            let p = self
                .device
                .map_memory(self.memory, 0, self.size, vk::MemoryMapFlags::empty())
                .map_err(err)?;
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), p.cast::<u8>(), bytes.len());
            self.device.unmap_memory(self.memory);
        }
        Ok(())
    }

    /// The buffer's bytes.
    ///
    /// # Errors
    /// Mapping failures.
    pub fn read(&self) -> Result<Vec<u8>, VulkanError> {
        #[allow(clippy::cast_possible_truncation)]
        let mut out = vec![0u8; self.size as usize];
        unsafe {
            let p = self
                .device
                .map_memory(self.memory, 0, self.size, vk::MemoryMapFlags::empty())
                .map_err(err)?;
            std::ptr::copy_nonoverlapping(p.cast::<u8>(), out.as_mut_ptr(), out.len());
            self.device.unmap_memory(self.memory);
        }
        Ok(out)
    }

    /// The buffer as `f32`s.
    ///
    /// # Errors
    /// Mapping failures.
    pub fn read_f32(&self, _gpu: &VulkanDevice) -> Result<Vec<f32>, VulkanError> {
        Ok(self
            .read()?
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect())
    }

    /// The buffer as `u32`s.
    ///
    /// # Errors
    /// Mapping failures.
    pub fn read_u32(&self, _gpu: &VulkanDevice) -> Result<Vec<u32>, VulkanError> {
        Ok(self
            .read()?
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect())
    }
}

impl Drop for ComputeBuffer {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_buffer(self.buffer, None);
            self.device.free_memory(self.memory, None);
        }
    }
}

/// A compiled compute pipeline over `bindings` storage buffers.
pub struct ComputeKernel {
    set_layout: vk::DescriptorSetLayout,
    layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    module: vk::ShaderModule,
    bindings: u32,
    push_bytes: u32,
    device: ash::Device,
}

impl std::fmt::Debug for ComputeKernel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComputeKernel")
            .field("bindings", &self.bindings)
            .finish_non_exhaustive()
    }
}

impl ComputeKernel {
    /// Compile `entry` from `wgsl` with `bindings` storage buffers at
    /// `@group(0) @binding(0..bindings)`.
    ///
    /// # Errors
    /// WGSL that does not validate, or pipeline creation failures.
    pub fn new(
        gpu: &VulkanDevice,
        wgsl: &str,
        entry: &str,
        bindings: u32,
    ) -> Result<Self, VulkanError> {
        Self::with_push_constants(gpu, wgsl, entry, bindings, 0)
    }

    /// As [`new`](Self::new), with a `var<immediate>` block (push constants) of
    /// `push_bytes` bytes (a per-dispatch uniform: a time, a step size).
    ///
    /// # Errors
    /// As [`new`](Self::new).
    pub fn with_push_constants(
        gpu: &VulkanDevice,
        wgsl: &str,
        entry: &str,
        bindings: u32,
        push_bytes: u32,
    ) -> Result<Self, VulkanError> {
        let spirv = translate_wgsl_to_spirv(wgsl, entry, naga::ShaderStage::Compute)?;
        let device = gpu.raw_device();
        let module = unsafe {
            device.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&spirv), None)
        }
        .map_err(err)?;
        let binds: Vec<vk::DescriptorSetLayoutBinding> = (0..bindings)
            .map(|i| {
                vk::DescriptorSetLayoutBinding::default()
                    .binding(i)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
            })
            .collect();
        let set_layout = unsafe {
            device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&binds),
                None,
            )
        }
        .map_err(err)?;
        let ranges = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .offset(0)
            .size(push_bytes.max(4))];
        let set_layouts = [set_layout];
        let mut layout_info = vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts);
        if push_bytes > 0 {
            layout_info = layout_info.push_constant_ranges(&ranges);
        }
        let layout = unsafe { device.create_pipeline_layout(&layout_info, None) }.map_err(err)?;
        let name = std::ffi::CString::new(entry).map_err(|e| VulkanError::Vulkan(e.to_string()))?;
        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(module)
            .name(&name);
        let info = vk::ComputePipelineCreateInfo::default()
            .stage(stage)
            .layout(layout);
        let pipeline =
            unsafe { device.create_compute_pipelines(vk::PipelineCache::null(), &[info], None) }
                .map_err(|(_, e)| err(e))?[0];
        Ok(Self {
            set_layout,
            layout,
            pipeline,
            module,
            bindings,
            push_bytes,
            device: device.clone(),
        })
    }

    /// One dispatch of `groups` workgroups.
    ///
    /// # Errors
    /// Binding-count mismatches, submission failures.
    pub fn dispatch(
        &self,
        gpu: &VulkanDevice,
        buffers: &[&ComputeBuffer],
        groups: [u32; 3],
    ) -> Result<(), VulkanError> {
        self.dispatch_many(gpu, buffers, &[(groups, Vec::new())])
    }

    /// Several dispatches recorded into one submission with a
    /// shader-write → shader-read barrier between each — iterations of a
    /// simulation. Each entry carries its own push-constant bytes.
    ///
    /// # Errors
    /// As [`dispatch`](Self::dispatch).
    #[allow(clippy::too_many_lines)]
    pub fn dispatch_many(
        &self,
        gpu: &VulkanDevice,
        buffers: &[&ComputeBuffer],
        steps: &[([u32; 3], Vec<u8>)],
    ) -> Result<(), VulkanError> {
        if buffers.len() as u32 != self.bindings {
            return Err(VulkanError::Vulkan(format!(
                "kernel binds {} buffers, {} given",
                self.bindings,
                buffers.len()
            )));
        }
        let device = &self.device;
        let sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(self.bindings.max(1))];
        let pool = unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(1)
                    .pool_sizes(&sizes),
                None,
            )
        }
        .map_err(err)?;
        let cmd_pool = unsafe {
            device.create_command_pool(
                &vk::CommandPoolCreateInfo::default().queue_family_index(gpu.queue_family_index()),
                None,
            )
        };
        let cmd_pool = match cmd_pool {
            Ok(p) => p,
            Err(e) => {
                unsafe { device.destroy_descriptor_pool(pool, None) };
                return Err(err(e));
            }
        };
        let fence = unsafe { device.create_fence(&vk::FenceCreateInfo::default(), None) };
        let cleanup = |fence: Option<vk::Fence>| unsafe {
            if let Some(f) = fence {
                device.destroy_fence(f, None);
            }
            device.destroy_command_pool(cmd_pool, None);
            device.destroy_descriptor_pool(pool, None);
        };
        let fence = match fence {
            Ok(f) => f,
            Err(e) => {
                cleanup(None);
                return Err(err(e));
            }
        };
        let result = (|| -> Result<(), VulkanError> {
            let layouts = [self.set_layout];
            let set = unsafe {
                device.allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(pool)
                        .set_layouts(&layouts),
                )
            }
            .map_err(err)?[0];
            let infos: Vec<[vk::DescriptorBufferInfo; 1]> = buffers
                .iter()
                .map(|b| {
                    [vk::DescriptorBufferInfo::default()
                        .buffer(b.buffer)
                        .offset(0)
                        .range(vk::WHOLE_SIZE)]
                })
                .collect();
            let writes: Vec<vk::WriteDescriptorSet> = infos
                .iter()
                .enumerate()
                .map(|(i, info)| {
                    vk::WriteDescriptorSet::default()
                        .dst_set(set)
                        .dst_binding(i as u32)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(info)
                })
                .collect();
            unsafe { device.update_descriptor_sets(&writes, &[]) };
            let cmd = unsafe {
                device.allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(cmd_pool)
                        .level(vk::CommandBufferLevel::PRIMARY)
                        .command_buffer_count(1),
                )
            }
            .map_err(err)?[0];
            unsafe {
                device
                    .begin_command_buffer(
                        cmd,
                        &vk::CommandBufferBeginInfo::default()
                            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                    )
                    .map_err(err)?;
                device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
                device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::COMPUTE,
                    self.layout,
                    0,
                    &[set],
                    &[],
                );
                for (k, (groups, push)) in steps.iter().enumerate() {
                    if k > 0 {
                        let barrier = vk::MemoryBarrier::default()
                            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                            .dst_access_mask(
                                vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
                            );
                        device.cmd_pipeline_barrier(
                            cmd,
                            vk::PipelineStageFlags::COMPUTE_SHADER,
                            vk::PipelineStageFlags::COMPUTE_SHADER,
                            vk::DependencyFlags::empty(),
                            &[barrier],
                            &[],
                            &[],
                        );
                    }
                    if self.push_bytes > 0 && !push.is_empty() {
                        device.cmd_push_constants(
                            cmd,
                            self.layout,
                            vk::ShaderStageFlags::COMPUTE,
                            0,
                            push,
                        );
                    }
                    device.cmd_dispatch(cmd, groups[0], groups[1], groups[2]);
                }
                let host = vk::MemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                    .dst_access_mask(vk::AccessFlags::HOST_READ);
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::PipelineStageFlags::HOST,
                    vk::DependencyFlags::empty(),
                    &[host],
                    &[],
                    &[],
                );
                device.end_command_buffer(cmd).map_err(err)?;
                let cmds = [cmd];
                let submit = vk::SubmitInfo::default().command_buffers(&cmds);
                device
                    .queue_submit(gpu.raw_queue(), &[submit], fence)
                    .map_err(err)?;
                device
                    .wait_for_fences(&[fence], true, u64::MAX)
                    .map_err(err)?;
            }
            Ok(())
        })();
        cleanup(Some(fence));
        result
    }
}

impl Drop for ComputeKernel {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline(self.pipeline, None);
            self.device.destroy_pipeline_layout(self.layout, None);
            self.device
                .destroy_descriptor_set_layout(self.set_layout, None);
            self.device.destroy_shader_module(self.module, None);
        }
    }
}
