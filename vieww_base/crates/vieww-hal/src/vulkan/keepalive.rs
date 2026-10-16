//! A little GPU work every frame, for frames that otherwise have none.
//!
//! # Why this exists — an experiment, stated as one
//!
//! On three Qualcomm phones (Device Farm, 2026-10-16) the CPU rasterizer ran
//! the *same* frames 25-70% slower when they were presented straight to the
//! window (`ANativeWindow_lock`) than when they went through Vulkan: LG Stylo 5
//! 30.6 vs 17.7 ms, Galaxy S20 9.1 vs 7.4, S26 Ultra 4.9 vs 3.3. Same build,
//! same phone, same run. The raster is memory-bandwidth-bound, and the leading
//! explanation is that these SoCs clock the memory bus from bandwidth votes
//! that GPU activity raises: with the GPU idle every frame, the bus runs slow.
//!
//! [`GpuKeepAlive`] tests that directly: once per frame it asks the GPU to fill
//! a small buffer — real memory traffic, a few hundred microseconds of GPU
//! time — and never waits for it. If the CPU raster on the window-present path
//! drops toward the Vulkan path's figure with this on, the explanation holds
//! and this is the (cheap) fix; if it does not, the cause is elsewhere.
//!
//! Off unless asked for (`debug.vieww.keepalive=1` on the Android example).

use ash::vk;

use super::{VulkanDevice, VulkanError};

/// One reusable command buffer that fills a device buffer, submitted at most
/// once per frame and never waited on.
#[derive(Debug)]
pub struct GpuKeepAlive {
    pool: vk::CommandPool,
    commands: vk::CommandBuffer,
    fence: vk::Fence,
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    submitted: bool,
}

impl GpuKeepAlive {
    /// Allocate a `bytes`-sized device buffer and record the fill once.
    ///
    /// # Errors
    ///
    /// Any Vulkan object could not be created.
    pub fn new(gpu: &VulkanDevice, bytes: u64) -> Result<Self, VulkanError> {
        let device = gpu.raw_device();
        let buffer = unsafe {
            device.create_buffer(
                &vk::BufferCreateInfo::default()
                    .size(bytes)
                    .usage(vk::BufferUsageFlags::TRANSFER_DST)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                None,
            )
        }?;
        let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
        let memory_type = gpu.find_memory_type(
            requirements.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;
        let memory = unsafe {
            device.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(requirements.size)
                    .memory_type_index(memory_type),
                None,
            )
        }?;
        unsafe { device.bind_buffer_memory(buffer, memory, 0) }?;
        let pool = unsafe {
            device.create_command_pool(
                &vk::CommandPoolCreateInfo::default().queue_family_index(gpu.queue_family_index()),
                None,
            )
        }?;
        let commands = unsafe {
            device.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )
        }?[0];
        unsafe {
            device.begin_command_buffer(commands, &vk::CommandBufferBeginInfo::default())?;
            device.cmd_fill_buffer(commands, buffer, 0, vk::WHOLE_SIZE, 0x5a5a_5a5a);
            device.end_command_buffer(commands)?;
        }
        let fence = unsafe { device.create_fence(&vk::FenceCreateInfo::default(), None) }?;
        Ok(Self {
            pool,
            commands,
            fence,
            buffer,
            memory,
            submitted: false,
        })
    }

    /// Submit the fill unless the previous one is still running. Never blocks.
    ///
    /// # Errors
    ///
    /// The submit or the fence query failed.
    pub fn tick(&mut self, gpu: &VulkanDevice) -> Result<(), VulkanError> {
        let device = gpu.raw_device();
        if self.submitted {
            if !unsafe { device.get_fence_status(self.fence) }? {
                return Ok(());
            }
            unsafe { device.reset_fences(&[self.fence]) }?;
        }
        let buffers = [self.commands];
        let submit = vk::SubmitInfo::default().command_buffers(&buffers);
        unsafe { device.queue_submit(gpu.raw_queue(), &[submit], self.fence) }?;
        self.submitted = true;
        Ok(())
    }

    /// Wait for the last fill and free everything. Must run before `gpu` is
    /// dropped.
    pub fn destroy(self, gpu: &VulkanDevice) {
        let device = gpu.raw_device();
        unsafe {
            if self.submitted {
                let _ = device.wait_for_fences(&[self.fence], true, u64::MAX);
            }
            device.destroy_fence(self.fence, None);
            device.destroy_command_pool(self.pool, None);
            device.destroy_buffer(self.buffer, None);
            device.free_memory(self.memory, None);
        }
    }
}
