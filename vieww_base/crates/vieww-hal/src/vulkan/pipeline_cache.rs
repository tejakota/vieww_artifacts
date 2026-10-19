//! A `VkPipelineCache` kept on disk between launches.
//!
//! # Why this exists
//!
//! Device Farm (2026-10-17) timed the GPU path's first frame at 1463 ms on the
//! LG Stylo 5, 341–352 ms on the Galaxy A36 and 82 ms on the S26 Ultra —
//! against 16–158 ms for the same first frame on the CPU path — and it cost
//! that on *every* launch, the fourth as much as the first. Every pipeline was
//! created with `VK_NULL_HANDLE` as its cache, so the driver compiled the
//! shaders to machine code from scratch each time. Android has no system
//! shader cache for Vulkan: an app that wants one keeps it itself.
//!
//! So a [`VulkanDevice`](super::VulkanDevice) opens one pipeline cache, seeded
//! from the file named by [`set_pipeline_cache_file`] if it exists and was
//! written by this same GPU and driver, and every pipeline the scene renderer
//! creates goes through it. [`VulkanDevice::save_pipeline_cache`](super::VulkanDevice::save_pipeline_cache) writes it
//! back; the platform layer calls it after the first GPU frame (a phone app is
//! killed, not dropped) and the device's `Drop` does it again.
//!
//! # Why the header is checked here
//!
//! The spec says an implementation must cope with incompatible data, and new
//! drivers do; old mobile drivers have been known to crash on it instead. The
//! 32-byte header names the vendor, device and a cache UUID that changes with
//! the driver, so data from another GPU or an updated driver is dropped here
//! and the cache starts empty — the cost is one slow first frame, never a
//! crash.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ash::vk;

static FILE: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Where devices created from now on keep their pipeline cache. `None` (the
/// default) keeps it in memory only.
pub fn set_pipeline_cache_file(file: Option<PathBuf>) {
    if let Ok(mut current) = FILE.lock() {
        *current = file;
    }
}

pub(crate) fn configured_file() -> Option<PathBuf> {
    FILE.lock().ok()?.clone()
}

/// The 32-byte header every pipeline cache starts with
/// (`VkPipelineCacheHeaderVersionOne`).
fn header_matches(data: &[u8], props: &vk::PhysicalDeviceProperties) -> bool {
    let word = |at: usize| {
        data.get(at..at + 4)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u32::from_le_bytes)
    };
    let (Some(length), Some(version), Some(vendor), Some(device)) =
        (word(0), word(4), word(8), word(12))
    else {
        return false;
    };
    length >= 32
        && (length as usize) <= data.len()
        && version == vk::PipelineCacheHeaderVersion::ONE.as_raw() as u32
        && vendor == props.vendor_id
        && device == props.device_id
        && data.get(16..32) == Some(&props.pipeline_cache_uuid[..])
}

/// Create the device's cache, seeded from `file` when it holds data for this
/// GPU and driver. Returns the cache and how many bytes seeded it.
pub(crate) fn create(
    device: &ash::Device,
    props: &vk::PhysicalDeviceProperties,
    file: Option<&Path>,
) -> (vk::PipelineCache, usize) {
    let seed = file
        .and_then(|file| std::fs::read(file).ok())
        .filter(|data| header_matches(data, props))
        .unwrap_or_default();
    let info = vk::PipelineCacheCreateInfo::default().initial_data(&seed);
    // SAFETY: `info` and `seed` outlive the call.
    match unsafe { device.create_pipeline_cache(&info, None) } {
        Ok(cache) => (cache, seed.len()),
        Err(_) => {
            // A driver that rejects the seed despite the header gets an empty
            // cache; one that cannot make any cache gets none at all, which is
            // exactly what every pipeline used before.
            let empty = vk::PipelineCacheCreateInfo::default();
            // SAFETY: as above.
            let cache = unsafe { device.create_pipeline_cache(&empty, None) }
                .unwrap_or(vk::PipelineCache::null());
            (cache, 0)
        }
    }
}

/// Write `cache` to `file`, atomically. Returns the bytes written.
pub(crate) fn save(device: &ash::Device, cache: vk::PipelineCache, file: &Path) -> std::io::Result<usize> {
    if cache == vk::PipelineCache::null() {
        return Ok(0);
    }
    // SAFETY: `cache` belongs to `device` and is alive.
    let data = unsafe { device.get_pipeline_cache_data(cache) }
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temporary = file.with_extension("tmp");
    std::fs::write(&temporary, &data)?;
    std::fs::rename(&temporary, file)?;
    Ok(data.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props() -> vk::PhysicalDeviceProperties {
        vk::PhysicalDeviceProperties {
            vendor_id: 0x5143,
            device_id: 0x0602,
            pipeline_cache_uuid: [7; 16],
            ..Default::default()
        }
    }

    fn header(vendor: u32, device: u32, uuid: [u8; 16]) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&32u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&vendor.to_le_bytes());
        data.extend_from_slice(&device.to_le_bytes());
        data.extend_from_slice(&uuid);
        data.extend_from_slice(b"driver data");
        data
    }

    #[test]
    fn only_this_gpu_and_driver_seed_the_cache() {
        assert!(header_matches(&header(0x5143, 0x0602, [7; 16]), &props()));
        // Another driver version: a different cache UUID.
        assert!(!header_matches(&header(0x5143, 0x0602, [8; 16]), &props()));
        // Another GPU.
        assert!(!header_matches(&header(0x10de, 0x0602, [7; 16]), &props()));
        // Truncated, empty, garbage.
        assert!(!header_matches(&header(0x5143, 0x0602, [7; 16])[..20], &props()));
        assert!(!header_matches(&[], &props()));
        assert!(!header_matches(b"not a pipeline cache at all, no", &props()));
    }
}
