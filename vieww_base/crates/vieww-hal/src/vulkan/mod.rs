//! The Vulkan backend — spec §4.3's first-landing HAL implementation.
//!
//! Headless only in this build: instance, device, a WGSL shader translated
//! through `naga` to SPIR-V, one graphics pipeline, and a
//! render-to-texture-then-copy-to-buffer readback. No swapchain, no window —
//! spec §14.1's M0/M1 slice ("Vulkan device, swapchain, upload ring, first
//! quad + clear") minus the swapchain, which needs a live surface this
//! sandbox has neither the display nor the windowing to create. What is here
//! is real and runs: `tests/vulkan_smoke.rs` exercises it against
//! `lavapipe` (Mesa's software Vulkan implementation — installed for this
//! build specifically so the HAL could be *tested*, not just compiled) and
//! checks the read-back pixels against `clear_color`, exactly.

use std::ffi::{c_char, CStr, CString};

use ash::vk;
use vieww_foundation::Color;

use super::{AdapterInfo, Device as HalDevice};

/// [`scene::SceneRenderer`] — a persistent pipeline that executes a whole
/// `vieww_gpu::ScenePlan` per frame, rather than building and tearing down a
/// pipeline per shape the way [`VulkanDevice::render_mesh_to_pixels`] does.
/// See that module's own doc for the difference and why it matters at 16.7ms.
///
/// Needs the `shader-compiler` feature: a kernel is the caller's own WGSL,
/// translated at run time.
#[cfg(feature = "shader-compiler")]
pub mod compute;
pub mod pipeline_cache;
mod present_history;
pub mod scene;
#[cfg(feature = "shader-compiler")]
pub use compute::{ComputeBuffer, ComputeKernel};
pub use pipeline_cache::set_pipeline_cache_file;
pub use present_history::{partial_blit_stats, PartialBlitStats};
pub use scene::{GpuFrameTimings, GpuPassTimes, Redrawn, SceneRenderer};

/// [`VulkanDevice::for_window`] and [`swapchain::VulkanSwapchain`] — real
/// windowed presentation. See that module's own docs.
#[cfg(feature = "vulkan-swapchain")]
pub mod swapchain;
#[cfg(feature = "vulkan-swapchain")]
pub use swapchain::{AcquiredImage, VulkanSwapchain};

#[derive(Debug)]
pub enum VulkanError {
    Loading(String),
    NoAdapter,
    Vulkan(String),
}

impl std::fmt::Display for VulkanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Loading(m) => write!(f, "loading Vulkan: {m}"),
            Self::NoAdapter => write!(f, "no Vulkan-capable adapter"),
            Self::Vulkan(m) => write!(f, "Vulkan: {m}"),
        }
    }
}

impl std::error::Error for VulkanError {}

impl From<vk::Result> for VulkanError {
    fn from(result: vk::Result) -> Self {
        Self::Vulkan(result.to_string())
    }
}

pub struct VulkanDevice {
    entry: ash::Entry,
    instance: ash::Instance,
    physical_device: vk::PhysicalDevice,
    device: ash::Device,
    queue: vk::Queue,
    queue_family: u32,
    info: AdapterInfo,
    /// Every pipeline this device's renderers create goes through this — see
    /// [`pipeline_cache`](self::pipeline_cache).
    pipeline_cache: vk::PipelineCache,
    /// Where it is kept between launches, if anywhere.
    pipeline_cache_file: Option<std::path::PathBuf>,
    /// How many bytes it started with: zero on a first launch, or when the
    /// file was another GPU's or another driver's.
    pipeline_cache_seed: usize,
}

impl std::fmt::Debug for VulkanDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanDevice")
            .field("info", &self.info)
            .finish_non_exhaustive()
    }
}

impl super::sealed::Sealed for VulkanDevice {}

/// Load the Vulkan loader (or, on macOS, MoltenVK itself).
///
/// # Why not just `ash::Entry::load()`
///
/// On Linux and Windows the loader is a system library on the default search
/// path, and `Entry::load` is the whole story. On macOS it never is:
///
/// * Homebrew's `vulkan-loader` lives in `/opt/homebrew/lib` (Apple silicon) or
///   `/usr/local/lib` (Intel), and neither is on `dlopen`'s default path — the
///   first macOS CI run found a working MoltenVK device with `vulkaninfo` and
///   then failed every Vulkan stage with `dlopen(libvulkan.dylib): no such
///   file`, because `vulkaninfo` is linked against the loader by path and this
///   crate asked for it by name.
/// * The LunarG SDK sets `VULKAN_SDK` and installs into `$VULKAN_SDK/lib`.
/// * A shipped `.app` can only rely on what it carries in
///   `Contents/Frameworks` (blocker B1): either the loader, or MoltenVK
///   loaded directly — MoltenVK exports the Vulkan entry points itself.
///
/// So on macOS this tries the default name first (which honours
/// `DYLD_LIBRARY_PATH` / `DYLD_FALLBACK_LIBRARY_PATH`), then those places in
/// that order, and the error names every path it tried.
///
/// # Errors
///
/// [`VulkanError::Loading`] when no candidate loads.
pub fn load_entry() -> Result<ash::Entry, VulkanError> {
    // **The iOS simulator's GPU has no Metal argument buffers.** It is a
    // paravirtualised device (`AppleParavirtDevice`), and MoltenVK uses
    // argument buffers by default: the first simulator run with MoltenVK
    // bundled loaded it, listed its extensions, and then died on
    // `-[AppleParavirtDevice newArgumentEncoderWithLayout:]: unrecognized
    // selector`. MoltenVK reads `MVK_CONFIG_USE_METAL_ARGUMENT_BUFFERS` when
    // it initialises, so it has to be set before the library is loaded — here.
    // Only on the simulator, and never over a value someone set on purpose.
    #[cfg(all(target_os = "ios", target_abi = "sim"))]
    if std::env::var_os("MVK_CONFIG_USE_METAL_ARGUMENT_BUFFERS").is_none() {
        std::env::set_var("MVK_CONFIG_USE_METAL_ARGUMENT_BUFFERS", "0");
    }
    match unsafe { ash::Entry::load() } {
        Ok(entry) => Ok(entry),
        Err(error) => load_entry_fallback(error.to_string()),
    }
}

#[cfg(not(any(target_os = "macos", target_os = "ios")))]
#[allow(
    clippy::unnecessary_wraps,
    reason = "same signature as the macOS fallback"
)]
fn load_entry_fallback(first: String) -> Result<ash::Entry, VulkanError> {
    Err(VulkanError::Loading(first))
}

/// On iOS there is no system loader and no `$VULKAN_SDK`: MoltenVK has to
/// travel inside the app. An iOS bundle is flat — the executable sits at the
/// `.app` root and `Frameworks/` beside it, not at `../Frameworks` as on macOS —
/// so those paths are tried as well. `ci/mobile/ios-app.sh` puts it there when
/// `VIEWW_MOLTENVK` names a copy. Without one, the error below now reaches the
/// screen's owner instead of a silent black window (see `vieww-platform-winit`'s
/// `fail_to_open`).
#[cfg(any(target_os = "macos", target_os = "ios"))]
fn load_entry_fallback(first: String) -> Result<ash::Entry, VulkanError> {
    use std::path::{Path, PathBuf};

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        let frameworks = dir.join("../Frameworks");
        candidates.push(frameworks.join("libvulkan.1.dylib"));
        candidates.push(frameworks.join("libMoltenVK.dylib"));
        candidates.push(dir.join("libvulkan.1.dylib"));
        // iOS: `<App>.app/Frameworks/…`.
        let flat = dir.join("Frameworks");
        candidates.push(flat.join("libMoltenVK.dylib"));
        candidates.push(flat.join("MoltenVK.framework/MoltenVK"));
        candidates.push(flat.join("libvulkan.1.dylib"));
    }
    if let Some(sdk) = std::env::var_os("VULKAN_SDK") {
        let sdk = PathBuf::from(sdk);
        candidates.push(sdk.join("lib/libvulkan.1.dylib"));
        candidates.push(sdk.join("lib/libvulkan.dylib"));
    }
    for prefix in ["/opt/homebrew", "/usr/local"] {
        candidates.push(Path::new(prefix).join("lib/libvulkan.1.dylib"));
        candidates.push(Path::new(prefix).join("lib/libMoltenVK.dylib"));
    }
    let mut tried = Vec::new();
    for path in candidates.into_iter().filter(|p| p.is_file()) {
        match unsafe { ash::Entry::load_from(&path) } {
            Ok(entry) => return Ok(entry),
            Err(error) => tried.push(format!("{}: {error}", path.display())),
        }
    }
    let detail = if tried.is_empty() {
        "no Vulkan loader or MoltenVK found in the app bundle, $VULKAN_SDK, /opt/homebrew/lib \
         or /usr/local/lib (install the Vulkan SDK, or `brew install vulkan-loader molten-vk`)"
            .to_owned()
    } else {
        format!("also tried {}", tried.join("; "))
    };
    Err(VulkanError::Loading(format!("{first}; {detail}")))
}

/// The dependency every offscreen render pass here needs, because each one
/// ends in `TRANSFER_SRC_OPTIMAL` and is then read back with
/// `vkCmdCopyImageToBuffer`: without it nothing orders the copy after the
/// pass's colour writes and its final layout transition. Vulkan's
/// synchronisation validation reports exactly that (`SYNC-HAZARD-READ-AFTER-
/// WRITE`, `vkCmdCopyImageToBuffer` after `vkCmdEndRenderPass`); a lenient
/// driver such as lavapipe hides it, and a tiled GPU may hand the copy pixels
/// that are not written yet.
const COPY_AFTER_RENDER: [vk::SubpassDependency; 1] = [vk::SubpassDependency {
    src_subpass: 0,
    dst_subpass: vk::SUBPASS_EXTERNAL,
    src_stage_mask: vk::PipelineStageFlags::from_raw(
        vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT.as_raw()
            | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS.as_raw(),
    ),
    dst_stage_mask: vk::PipelineStageFlags::TRANSFER,
    src_access_mask: vk::AccessFlags::from_raw(
        vk::AccessFlags::COLOR_ATTACHMENT_WRITE.as_raw()
            | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE.as_raw(),
    ),
    dst_access_mask: vk::AccessFlags::TRANSFER_READ,
    dependency_flags: vk::DependencyFlags::empty(),
}];

impl VulkanDevice {
    /// Bring up a headless Vulkan 1.1 device on the first adapter that
    /// offers a graphics queue — spec §4.3's "Vulkan 1.1 ... Timeline
    /// semaphores mandatory" (the instance is created at 1.2 so timeline
    /// semaphores, a core 1.2 feature, are always available; nothing here
    /// uses them yet — see the module docs on scope).
    pub fn new() -> Result<Self, VulkanError> {
        let entry = load_entry()?;
        let instance = Self::create_instance(&entry, &[])?;

        let (physical_device, queue_family) =
            Self::pick_physical_device(&instance, |pd| Self::find_graphics_family(&instance, pd))
                .ok_or_else(|| {
                unsafe { instance.destroy_instance(None) };
                VulkanError::NoAdapter
            })?;

        let info = Self::adapter_info(&instance, physical_device);
        let (device, queue) = Self::create_device(&instance, physical_device, queue_family, &[])?;

        Ok(Self::assemble(
            entry,
            instance,
            physical_device,
            (device, queue, queue_family),
            info,
        ))
    }

    /// The last step of every constructor: the parts, plus the pipeline cache
    /// (seeded from [`set_pipeline_cache_file`]'s file when it fits).
    pub(crate) fn assemble(
        entry: ash::Entry,
        instance: ash::Instance,
        physical_device: vk::PhysicalDevice,
        (device, queue, queue_family): (ash::Device, vk::Queue, u32),
        info: AdapterInfo,
    ) -> Self {
        let file = pipeline_cache::configured_file();
        let props = unsafe { instance.get_physical_device_properties(physical_device) };
        let (pipeline_cache, pipeline_cache_seed) =
            pipeline_cache::create(&device, &props, file.as_deref());
        Self {
            entry,
            instance,
            physical_device,
            device,
            queue,
            queue_family,
            info,
            pipeline_cache,
            pipeline_cache_file: file,
            pipeline_cache_seed,
        }
    }

    /// The cache every pipeline on this device is created through.
    pub(crate) const fn pipeline_cache(&self) -> vk::PipelineCache {
        self.pipeline_cache
    }

    /// How many bytes of an earlier launch's pipeline cache this device
    /// started with — zero on a first launch, or after a driver update.
    #[must_use]
    pub const fn pipeline_cache_seed(&self) -> usize {
        self.pipeline_cache_seed
    }

    /// Write the pipeline cache to its file, if it has one. Returns the bytes
    /// written. Call it once the pipelines that matter exist: a phone app is
    /// killed rather than dropped, so `Drop`'s own save is a second chance,
    /// not the plan.
    ///
    /// # Errors
    ///
    /// The file could not be written, or the driver would not hand the data
    /// over. Either costs the next launch its head start and nothing else.
    pub fn save_pipeline_cache(&self) -> std::io::Result<usize> {
        match &self.pipeline_cache_file {
            Some(file) => pipeline_cache::save(&self.device, self.pipeline_cache, file),
            None => Ok(0),
        }
    }

    /// The instance every constructor shares: application/engine identity and
    /// API version are fixed, `extra_extensions` is the only thing that
    /// varies — empty for the headless path, `VK_KHR_surface` plus a
    /// platform surface extension (via `ash_window`) for
    /// [`swapchain`]'s `for_window`.
    fn create_instance(
        entry: &ash::Entry,
        extra_extensions: &[*const c_char],
    ) -> Result<ash::Instance, VulkanError> {
        let app_name = CString::new("vieww-hal").unwrap();
        let app_info = vk::ApplicationInfo::default()
            .application_name(&app_name)
            .application_version(0)
            .engine_name(&app_name)
            .engine_version(0)
            .api_version(vk::API_VERSION_1_2);

        // **Portability drivers (MoltenVK on macOS) are invisible without this.**
        //
        // Since Vulkan loader 1.3.216 a "portability" driver — one that is not
        // fully conformant, which is every Vulkan-on-Metal layer — is only
        // enumerated when the instance enables `VK_KHR_portability_enumeration`
        // and sets `ENUMERATE_PORTABILITY`. Without it a Mac with the Vulkan
        // SDK installed reports no adapter at all, and every window
        // (`vieww-platform-winit` presents through this module on every
        // desktop) fails to open. Enabled only when the loader offers it, so
        // conformant drivers elsewhere see no change.
        let mut extensions: Vec<*const c_char> = extra_extensions.to_vec();
        let portability = unsafe { entry.enumerate_instance_extension_properties(None) }
            .unwrap_or_default()
            .iter()
            .any(|ext| {
                ext.extension_name_as_c_str()
                    .is_ok_and(|name| name == ash::khr::portability_enumeration::NAME)
            });
        if portability {
            extensions.push(ash::khr::portability_enumeration::NAME.as_ptr());
        }

        let mut instance_info = vk::InstanceCreateInfo::default().application_info(&app_info);
        if !extensions.is_empty() {
            instance_info = instance_info.enabled_extension_names(&extensions);
        }
        if portability {
            instance_info = instance_info.flags(vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR);
        }
        unsafe { entry.create_instance(&instance_info, None) }
            .map_err(|e| VulkanError::Vulkan(e.to_string()))
    }

    /// Pick the best physical device, rather than whichever the loader listed
    /// first.
    ///
    /// # Why this is not "the first one with a graphics queue"
    ///
    /// Because a machine can have more than one, and the difference is not
    /// cosmetic. A laptop with switchable graphics enumerates its integrated
    /// part first; a machine with Mesa installed can enumerate `llvmpipe`
    /// first, and a frame drawn there is a CPU frame that every measurement
    /// will record as a GPU one. Neither case announces itself, and the second
    /// invalidates any performance claim made on that machine.
    ///
    /// So: discrete, then integrated, then virtual, then anything else, with
    /// CPU/software implementations last and only if nothing else will do.
    /// `VIEWW_VK_DEVICE` overrides the lot with a case-insensitive substring of
    /// the device name, because a developer with two real GPUs needs to be able
    /// to say which one without editing this function.
    fn pick_physical_device(
        instance: &ash::Instance,
        mut accept: impl FnMut(vk::PhysicalDevice) -> Option<u32>,
    ) -> Option<(vk::PhysicalDevice, u32)> {
        let physical_devices = unsafe { instance.enumerate_physical_devices() }.ok()?;
        let wanted = std::env::var("VIEWW_VK_DEVICE")
            .ok()
            .map(|s| s.to_lowercase());

        // `VIEWW_TRACE_FRAMES` — the platform layer's startup/frame trace —
        // also names each device as it is asked whether it can serve: on a
        // hybrid laptop, a GPU that is powered down can take its time
        // answering a surface query, and a stalled startup should say which
        // device it was waiting on. Read per call; this runs once per window.
        let trace = std::env::var_os("VIEWW_TRACE_FRAMES").is_some();
        let mut best: Option<(u32, vk::PhysicalDevice, u32)> = None;
        for pd in physical_devices {
            let props = unsafe { instance.get_physical_device_properties(pd) };
            let name = unsafe { CStr::from_ptr(props.device_name.as_ptr()) }
                .to_string_lossy()
                .to_lowercase();
            if trace {
                eprintln!("vulkan: asking {name} ({:?})", props.device_type);
            }
            let Some(family) = accept(pd) else {
                if trace {
                    eprintln!("vulkan: {name} cannot serve this request");
                }
                continue;
            };
            if let Some(wanted) = wanted.as_deref() {
                if name.contains(wanted) {
                    return Some((pd, family));
                }
                continue;
            }
            // Higher is better. Software last, whatever it calls itself.
            let software = name.contains("llvmpipe")
                || name.contains("lavapipe")
                || name.contains("swiftshader");
            let rank = if software {
                0
            } else {
                match props.device_type {
                    vk::PhysicalDeviceType::DISCRETE_GPU => 4,
                    vk::PhysicalDeviceType::INTEGRATED_GPU => 3,
                    vk::PhysicalDeviceType::VIRTUAL_GPU => 2,
                    _ => 1,
                }
            };
            if best.is_none_or(|(seen, _, _)| rank > seen) {
                best = Some((rank, pd, family));
            }
        }
        best.map(|(_, pd, family)| (pd, family))
    }

    fn find_graphics_family(instance: &ash::Instance, pd: vk::PhysicalDevice) -> Option<u32> {
        let families = unsafe { instance.get_physical_device_queue_family_properties(pd) };
        families
            .iter()
            .position(|f| f.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            .map(|idx| idx as u32)
    }

    fn adapter_info(instance: &ash::Instance, physical_device: vk::PhysicalDevice) -> AdapterInfo {
        let props = unsafe { instance.get_physical_device_properties(physical_device) };
        let name = unsafe { CStr::from_ptr(props.device_name.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        let device_type = format!("{:?}", props.device_type);
        AdapterInfo {
            name,
            backend: "vulkan",
            device_type,
        }
    }

    /// One graphics queue, `extra_extensions` enabled (empty for headless,
    /// `VK_KHR_swapchain` for [`swapchain`]'s windowed path).
    fn create_device(
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        queue_family: u32,
        extra_extensions: &[*const c_char],
    ) -> Result<(ash::Device, vk::Queue), VulkanError> {
        let priorities = [1.0f32];
        let queue_info = vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family)
            .queue_priorities(&priorities);
        let queue_infos = [queue_info];
        // A portability device (MoltenVK) *must* have `VK_KHR_portability_subset`
        // enabled when it advertises it — the spec makes that a validity rule.
        let mut extensions: Vec<*const c_char> = extra_extensions.to_vec();
        let subset = unsafe { instance.enumerate_device_extension_properties(physical_device) }
            .unwrap_or_default()
            .iter()
            .any(|ext| {
                ext.extension_name_as_c_str()
                    .is_ok_and(|name| name == ash::khr::portability_subset::NAME)
            });
        if subset {
            extensions.push(ash::khr::portability_subset::NAME.as_ptr());
        }
        let mut device_info = vk::DeviceCreateInfo::default().queue_create_infos(&queue_infos);
        if !extensions.is_empty() {
            device_info = device_info.enabled_extension_names(&extensions);
        }
        let device = unsafe { instance.create_device(physical_device, &device_info, None) }
            .map_err(|e| VulkanError::Vulkan(e.to_string()))?;
        let queue = unsafe { device.get_device_queue(queue_family, 0) };
        Ok((device, queue))
    }

    pub(crate) fn find_memory_type(
        &self,
        filter: u32,
        flags: vk::MemoryPropertyFlags,
    ) -> Result<u32, VulkanError> {
        let mem_props = unsafe {
            self.instance
                .get_physical_device_memory_properties(self.physical_device)
        };
        (0..mem_props.memory_type_count)
            .find(|&i| {
                (filter & (1 << i)) != 0
                    && mem_props.memory_types[i as usize]
                        .property_flags
                        .contains(flags)
            })
            .ok_or_else(|| VulkanError::Vulkan("no suitable memory type".into()))
    }

    /// The physical device's memory properties, so a renderer built on this one
    /// can choose a memory type per allocation instead of once at construction.
    ///
    /// Resolving a single index up front is what
    /// `VUID-vkBindImageMemory-memory-01047` fires on: an image accepts only the
    /// types listed in its own `memoryTypeBits`, and a discrete GPU's sets do
    /// not coincide the way a unified-memory part's do.
    pub(crate) fn memory_properties(&self) -> vk::PhysicalDeviceMemoryProperties {
        unsafe {
            self.instance
                .get_physical_device_memory_properties(self.physical_device)
        }
    }

    /// The loaded device handle, for a renderer built on top of this one.
    ///
    /// `pub(crate)`, not `pub`: an `ash::Device` in a caller's hands is a
    /// licence to destroy objects this type owns, and the sealing on
    /// [`Device`](crate::Device) exists precisely so that the set of things
    /// which can do that is a list inside this crate.
    /// [`scene::SceneRenderer`] is on that list; nothing outside it is.
    pub(crate) const fn raw_device(&self) -> &ash::Device {
        &self.device
    }

    /// The graphics queue this device submits on.
    pub(crate) const fn raw_queue(&self) -> vk::Queue {
        self.queue
    }

    /// The queue family [`raw_queue`](Self::raw_queue) belongs to, which a
    /// command pool has to be created against.
    pub(crate) const fn queue_family_index(&self) -> u32 {
        self.queue_family
    }

    /// Whether [`raw_queue`](Self::raw_queue) can record timestamps, and
    /// what one tick is: `(nanoseconds per tick, valid bits)`. `None` when the
    /// queue family reports no valid timestamp bits or the device a zero
    /// period — then a renderer measures nothing on the GPU's clock rather
    /// than reporting zeroes as if it had.
    pub(crate) fn timestamp_support(&self) -> Option<(f32, u32)> {
        // SAFETY: queries on a live physical device.
        let (families, props) = unsafe {
            (
                self.instance
                    .get_physical_device_queue_family_properties(self.physical_device),
                self.instance
                    .get_physical_device_properties(self.physical_device),
            )
        };
        let bits = families
            .get(self.queue_family as usize)?
            .timestamp_valid_bits;
        let period = props.limits.timestamp_period;
        (bits > 0 && period > 0.0).then_some((period, bits))
    }
}

impl Drop for VulkanDevice {
    fn drop(&mut self) {
        let _ = self.save_pipeline_cache();
        unsafe {
            if self.pipeline_cache != vk::PipelineCache::null() {
                self.device
                    .destroy_pipeline_cache(self.pipeline_cache, None);
            }
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
        let _ = &self.entry;
    }
}

/// WGSL to SPIR-V for one entry point.
///
/// Spec §4.4: "All shaders are authored once in WGSL ... naga validates and
/// translates every WGSL module into SPIR-V ... artifacts that are embedded in
/// the binary". That is now literally so for every built-in shader: they are
/// translated when `vieww-shaders` compiles ([`vieww_shaders::baked`]), and
/// this returns the embedded words. Only WGSL that is *not* built in — a
/// [`ComputeKernel`](compute::ComputeKernel)'s — is translated here at run
/// time, and only with the `shader-compiler` feature; without it, such WGSL
/// is an error that says which feature to turn on.
pub(crate) fn translate_wgsl_to_spirv(
    source: &str,
    entry_point: &str,
    stage: Stage,
) -> Result<std::borrow::Cow<'static, [u32]>, VulkanError> {
    if let Some(words) = vieww_shaders::baked::spirv(source, entry_point, stage) {
        return Ok(std::borrow::Cow::Borrowed(words));
    }
    #[cfg(feature = "shader-compiler")]
    {
        let stage = match stage {
            Stage::Vertex => vieww_shaders::ShaderStage::Vertex,
            Stage::Fragment => vieww_shaders::ShaderStage::Fragment,
            Stage::Compute => vieww_shaders::ShaderStage::Compute,
        };
        vieww_shaders::ParsedShader::parse(source)
            .and_then(|parsed| parsed.to_spirv(entry_point, stage))
            .map(std::borrow::Cow::Owned)
            .map_err(|e| VulkanError::Vulkan(format!("vieww-shaders: {e}")))
    }
    #[cfg(not(feature = "shader-compiler"))]
    {
        Err(VulkanError::Vulkan(
            match vieww_shaders::baked::find(source) {
                Some(baked) => format!(
                "vieww-shaders: built-in shader {:?} has no {stage:?} entry point {entry_point:?}",
                baked.name
            ),
                None => {
                    "vieww-shaders: this WGSL is not a built-in shader, and vieww-hal was built \
                     without `shader-compiler` to translate it at run time"
                        .to_string()
                }
            },
        ))
    }
}

pub(crate) use vieww_shaders::baked::Stage;

impl HalDevice for VulkanDevice {
    type Error = VulkanError;

    fn info(&self) -> AdapterInfo {
        self.info.clone()
    }

    fn render_clear_to_pixels(
        &self,
        width: u32,
        height: u32,
        clear_color: Color,
    ) -> Result<Vec<u8>, VulkanError> {
        let device = &self.device;
        let format = vk::Format::R8G8B8A8_UNORM;

        // --- color attachment image -------------------------------------------------
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let image = unsafe { device.create_image(&image_info, None) }?;
        let mem_req = unsafe { device.get_image_memory_requirements(image) };
        let mem_type = self.find_memory_type(
            mem_req.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;
        let image_mem_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_req.size)
            .memory_type_index(mem_type);
        let image_memory = unsafe { device.allocate_memory(&image_mem_info, None) }?;
        unsafe { device.bind_image_memory(image, image_memory, 0) }?;

        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        let image_view = unsafe { device.create_image_view(&view_info, None) }?;

        // --- render pass + framebuffer -----------------------------------------------
        let attachment = vk::AttachmentDescription::default()
            .format(format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL);
        let attachments = [attachment];
        let color_ref = [vk::AttachmentReference {
            attachment: 0,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        }];
        let subpass = vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_ref);
        let subpasses = [subpass];
        let render_pass_info = vk::RenderPassCreateInfo::default()
            .attachments(&attachments)
            .subpasses(&subpasses)
            .dependencies(&COPY_AFTER_RENDER);
        let render_pass = unsafe { device.create_render_pass(&render_pass_info, None) }?;

        let fb_attachments = [image_view];
        let fb_info = vk::FramebufferCreateInfo::default()
            .render_pass(render_pass)
            .attachments(&fb_attachments)
            .width(width)
            .height(height)
            .layers(1);
        let framebuffer = unsafe { device.create_framebuffer(&fb_info, None) }?;

        // --- uniform buffer -----------------------------------------------------------
        let straight = [
            f32::from(clear_color.r) / 255.0,
            f32::from(clear_color.g) / 255.0,
            f32::from(clear_color.b) / 255.0,
            f32::from(clear_color.a) / 255.0,
        ];
        let ubo_size = std::mem::size_of::<[f32; 4]>() as u64;
        let ubo_info = vk::BufferCreateInfo::default()
            .size(ubo_size)
            .usage(vk::BufferUsageFlags::UNIFORM_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let ubo = unsafe { device.create_buffer(&ubo_info, None) }?;
        let ubo_req = unsafe { device.get_buffer_memory_requirements(ubo) };
        let ubo_mem_type = self.find_memory_type(
            ubo_req.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        let ubo_alloc = vk::MemoryAllocateInfo::default()
            .allocation_size(ubo_req.size)
            .memory_type_index(ubo_mem_type);
        let ubo_memory = unsafe { device.allocate_memory(&ubo_alloc, None) }?;
        unsafe { device.bind_buffer_memory(ubo, ubo_memory, 0) }?;
        unsafe {
            let ptr = device.map_memory(ubo_memory, 0, ubo_size, vk::MemoryMapFlags::empty())?
                as *mut f32;
            ptr.copy_from_nonoverlapping(straight.as_ptr(), 4);
            device.unmap_memory(ubo_memory);
        }

        // --- descriptor set -------------------------------------------------------------
        let binding = vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT);
        let bindings = [binding];
        let layout_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
        let set_layout = unsafe { device.create_descriptor_set_layout(&layout_info, None) }?;

        let pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: 1,
        };
        let pool_sizes = [pool_size];
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(1);
        let descriptor_pool = unsafe { device.create_descriptor_pool(&pool_info, None) }?;
        let set_layouts = [set_layout];
        let set_alloc = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&set_layouts);
        let descriptor_set = unsafe { device.allocate_descriptor_sets(&set_alloc) }?[0];

        let buffer_info = vk::DescriptorBufferInfo {
            buffer: ubo,
            offset: 0,
            range: ubo_size,
        };
        let buffer_infos = [buffer_info];
        let write = vk::WriteDescriptorSet::default()
            .dst_set(descriptor_set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .buffer_info(&buffer_infos);
        unsafe { device.update_descriptor_sets(&[write], &[]) };

        // --- shaders + pipeline -----------------------------------------------------------
        let vs_spirv =
            translate_wgsl_to_spirv(vieww_shaders::library::CLEAR_WGSL, "vs_main", Stage::Vertex)?;
        let fs_spirv = translate_wgsl_to_spirv(
            vieww_shaders::library::CLEAR_WGSL,
            "fs_main",
            Stage::Fragment,
        )?;
        let vs_info = vk::ShaderModuleCreateInfo::default().code(&vs_spirv);
        let fs_info = vk::ShaderModuleCreateInfo::default().code(&fs_spirv);
        let vs_module = unsafe { device.create_shader_module(&vs_info, None) }?;
        let fs_module = unsafe { device.create_shader_module(&fs_info, None) }?;

        let entry_point = CString::new("vs_main").unwrap();
        let fs_entry_point = CString::new("fs_main").unwrap();
        let stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vs_module)
                .name(&entry_point),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(fs_module)
                .name(&fs_entry_point),
        ];

        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: width as f32,
            height: height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        let scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D { width, height },
        };
        let viewports = [viewport];
        let scissors = [scissor];
        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewports(&viewports)
            .scissors(&scissors);
        let raster = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .line_width(1.0);
        let multisample = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let blend_attachment = vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA);
        let blend_attachments = [blend_attachment];
        let color_blend =
            vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend_attachments);

        let pipeline_layout_info =
            vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts);
        let pipeline_layout =
            unsafe { device.create_pipeline_layout(&pipeline_layout_info, None) }?;

        let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&raster)
            .multisample_state(&multisample)
            .color_blend_state(&color_blend)
            .layout(pipeline_layout)
            .render_pass(render_pass)
            .subpass(0);
        let pipelines = unsafe {
            device.create_graphics_pipelines(self.pipeline_cache(), &[pipeline_info], None)
        }
        .map_err(|(_, e)| VulkanError::Vulkan(e.to_string()))?;
        let pipeline = pipelines[0];

        // --- readback buffer -----------------------------------------------------------
        let readback_size = (width as u64) * (height as u64) * 4;
        let readback_info = vk::BufferCreateInfo::default()
            .size(readback_size)
            .usage(vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let readback_buffer = unsafe { device.create_buffer(&readback_info, None) }?;
        let readback_req = unsafe { device.get_buffer_memory_requirements(readback_buffer) };
        let readback_mem_type = self.find_memory_type(
            readback_req.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        let readback_alloc = vk::MemoryAllocateInfo::default()
            .allocation_size(readback_req.size)
            .memory_type_index(readback_mem_type);
        let readback_memory = unsafe { device.allocate_memory(&readback_alloc, None) }?;
        unsafe { device.bind_buffer_memory(readback_buffer, readback_memory, 0) }?;

        // --- command buffer -----------------------------------------------------------
        let pool_info = vk::CommandPoolCreateInfo::default().queue_family_index(self.queue_family);
        let command_pool = unsafe { device.create_command_pool(&pool_info, None) }?;
        let cmd_alloc = vk::CommandBufferAllocateInfo::default()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let command_buffer = unsafe { device.allocate_command_buffers(&cmd_alloc) }?[0];

        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe { device.begin_command_buffer(command_buffer, &begin_info) }?;

        let clear_value = vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.0, 0.0, 0.0, 0.0],
            },
        };
        let clear_values = [clear_value];
        let rp_begin = vk::RenderPassBeginInfo::default()
            .render_pass(render_pass)
            .framebuffer(framebuffer)
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: vk::Extent2D { width, height },
            })
            .clear_values(&clear_values);
        unsafe {
            device.cmd_begin_render_pass(command_buffer, &rp_begin, vk::SubpassContents::INLINE);
            device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, pipeline);
            device.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline_layout,
                0,
                &[descriptor_set],
                &[],
            );
            device.cmd_draw(command_buffer, 3, 1, 0, 0);
            device.cmd_end_render_pass(command_buffer);

            let copy_region = vk::BufferImageCopy {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                image_subresource: vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
                image_extent: vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                },
            };
            device.cmd_copy_image_to_buffer(
                command_buffer,
                image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                readback_buffer,
                &[copy_region],
            );
        }
        unsafe { device.end_command_buffer(command_buffer) }?;

        let fence_info = vk::FenceCreateInfo::default();
        let fence = unsafe { device.create_fence(&fence_info, None) }?;
        let command_buffers = [command_buffer];
        let submit_info = vk::SubmitInfo::default().command_buffers(&command_buffers);
        unsafe {
            device.queue_submit(self.queue, &[submit_info], fence)?;
            device.wait_for_fences(&[fence], true, u64::MAX)?;
        }

        let mut pixels = vec![0u8; readback_size as usize];
        unsafe {
            let ptr = device.map_memory(
                readback_memory,
                0,
                readback_size,
                vk::MemoryMapFlags::empty(),
            )? as *const u8;
            std::ptr::copy_nonoverlapping(ptr, pixels.as_mut_ptr(), readback_size as usize);
            device.unmap_memory(readback_memory);
        }

        unsafe {
            device.destroy_fence(fence, None);
            device.destroy_command_pool(command_pool, None);
            device.destroy_pipeline(pipeline, None);
            device.destroy_pipeline_layout(pipeline_layout, None);
            device.destroy_shader_module(vs_module, None);
            device.destroy_shader_module(fs_module, None);
            device.destroy_descriptor_pool(descriptor_pool, None);
            device.destroy_descriptor_set_layout(set_layout, None);
            device.free_memory(readback_memory, None);
            device.destroy_buffer(readback_buffer, None);
            device.free_memory(ubo_memory, None);
            device.destroy_buffer(ubo, None);
            device.destroy_framebuffer(framebuffer, None);
            device.destroy_render_pass(render_pass, None);
            device.destroy_image_view(image_view, None);
            device.free_memory(image_memory, None);
            device.destroy_image(image, None);
        }

        Ok(pixels)
    }
}

/// A list of platform-appropriate C-string extension names — kept as a
/// helper for the day a window surface is added (spec's Phase M1 swapchain);
/// unused by the headless path above, hence `#[allow(dead_code)]`.
#[allow(dead_code)]
fn required_extensions() -> Vec<*const c_char> {
    Vec::new()
}

/// [`VulkanDevice::render_mesh_to_pixels`]'s shader —
/// [`vieww_shaders::library::SOLID_WGSL`] — is vertex-buffer-driven: real
/// per-vertex positions in, a solid fragment colour out. This is the piece
/// `render_clear_to_pixels`'s fullscreen-triangle trick deliberately has no
/// use for (spec's M0/M1 clear-only slice) and that a real GPU compositing
/// path needs first — `docs/RENDERER-V2-NOTES.md`'s "GPU
/// tessellation/GPU-driven execution" gap starts exactly here: a pipeline
/// that actually reads a vertex buffer `vieww-gpu::tessellate` produced,
/// rather than three hardcoded clip-space corners.
impl VulkanDevice {
    /// Render a real tessellated mesh — `vieww_gpu::tessellate`'s
    /// output, though this function takes plain slices rather than
    /// depending on that crate, so `vieww-hal` gains no new dependency for
    /// it — as one solid-coloured draw call, and read the result back as
    /// straight-alpha RGBA8.
    ///
    /// This is the direct continuation of `render_clear_to_pixels`'s
    /// pipeline (same instance/device, same descriptor/shader/pipeline
    /// shape) with the one piece that function deliberately omits: a real
    /// vertex buffer, index buffer, and indexed draw call, so this is
    /// genuinely a GPU rasterizing caller-supplied geometry rather than a
    /// hardcoded clip-space triangle.
    ///
    /// `positions` are in the same pixel space `width`/`height` describe
    /// (origin top-left); `indices` index into `positions` as a triangle
    /// list (three indices per triangle, matching
    /// `vieww_gpu::tessellate::Mesh`'s own layout).
    ///
    /// # Errors
    ///
    /// Any Vulkan call failing — most commonly, in an environment with no
    /// ICD, at instance/device creation before this method is even
    /// reachable (see [`VulkanDevice::new`]).
    pub fn render_mesh_to_pixels(
        &self,
        width: u32,
        height: u32,
        positions: &[[f32; 2]],
        indices: &[u32],
        color: Color,
    ) -> Result<Vec<u8>, VulkanError> {
        if positions.is_empty() || indices.is_empty() {
            // A degenerate mesh draws nothing — return a fully transparent
            // frame rather than asking Vulkan to bind a zero-length buffer,
            // which some drivers reject outright.
            return Ok(vec![0u8; (width as usize) * (height as usize) * 4]);
        }

        let device = &self.device;
        let format = vk::Format::R8G8B8A8_UNORM;

        // --- color attachment image -------------------------------------------------
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let image = unsafe { device.create_image(&image_info, None) }?;
        let mem_req = unsafe { device.get_image_memory_requirements(image) };
        let mem_type = self.find_memory_type(
            mem_req.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;
        let image_mem_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_req.size)
            .memory_type_index(mem_type);
        let image_memory = unsafe { device.allocate_memory(&image_mem_info, None) }?;
        unsafe { device.bind_image_memory(image, image_memory, 0) }?;

        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        let image_view = unsafe { device.create_image_view(&view_info, None) }?;

        // --- render pass + framebuffer -----------------------------------------------
        let attachment = vk::AttachmentDescription::default()
            .format(format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL);
        let attachments = [attachment];
        let color_ref = [vk::AttachmentReference {
            attachment: 0,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        }];
        let subpass = vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_ref);
        let subpasses = [subpass];
        let render_pass_info = vk::RenderPassCreateInfo::default()
            .attachments(&attachments)
            .subpasses(&subpasses)
            .dependencies(&COPY_AFTER_RENDER);
        let render_pass = unsafe { device.create_render_pass(&render_pass_info, None) }?;

        let fb_attachments = [image_view];
        let fb_info = vk::FramebufferCreateInfo::default()
            .render_pass(render_pass)
            .attachments(&fb_attachments)
            .width(width)
            .height(height)
            .layers(1);
        let framebuffer = unsafe { device.create_framebuffer(&fb_info, None) }?;

        // --- vertex + index buffers ----------------------------------------------------
        let (vertex_buffer, vertex_memory) = self.create_host_visible_buffer(
            vk::BufferUsageFlags::VERTEX_BUFFER,
            bytemuck_cast_positions(positions),
        )?;
        let (index_buffer, index_memory) = self.create_host_visible_buffer(
            vk::BufferUsageFlags::INDEX_BUFFER,
            bytemuck_cast_u32(indices),
        )?;

        // --- uniform buffer (color + viewport) ------------------------------------------
        let straight = [
            f32::from(color.r) / 255.0,
            f32::from(color.g) / 255.0,
            f32::from(color.b) / 255.0,
            f32::from(color.a) / 255.0,
            width as f32,
            height as f32,
            0.0,
            0.0,
        ];
        let (ubo, ubo_memory) = self.create_host_visible_buffer(
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            bytemuck_cast_f32(&straight),
        )?;
        let ubo_size = std::mem::size_of_val(&straight) as u64;

        // --- descriptor set -------------------------------------------------------------
        let binding = vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT);
        let bindings = [binding];
        let layout_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
        let set_layout = unsafe { device.create_descriptor_set_layout(&layout_info, None) }?;

        let pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: 1,
        };
        let pool_sizes = [pool_size];
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(1);
        let descriptor_pool = unsafe { device.create_descriptor_pool(&pool_info, None) }?;
        let set_layouts = [set_layout];
        let set_alloc = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&set_layouts);
        let descriptor_set = unsafe { device.allocate_descriptor_sets(&set_alloc) }?[0];

        let buffer_info = vk::DescriptorBufferInfo {
            buffer: ubo,
            offset: 0,
            range: ubo_size,
        };
        let buffer_infos = [buffer_info];
        let write = vk::WriteDescriptorSet::default()
            .dst_set(descriptor_set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .buffer_info(&buffer_infos);
        unsafe { device.update_descriptor_sets(&[write], &[]) };

        // --- shaders + pipeline -----------------------------------------------------------
        let vs_spirv =
            translate_wgsl_to_spirv(vieww_shaders::library::SOLID_WGSL, "vs_main", Stage::Vertex)?;
        let fs_spirv = translate_wgsl_to_spirv(
            vieww_shaders::library::SOLID_WGSL,
            "fs_main",
            Stage::Fragment,
        )?;
        let vs_info = vk::ShaderModuleCreateInfo::default().code(&vs_spirv);
        let fs_info = vk::ShaderModuleCreateInfo::default().code(&fs_spirv);
        let vs_module = unsafe { device.create_shader_module(&vs_info, None) }?;
        let fs_module = unsafe { device.create_shader_module(&fs_info, None) }?;

        let entry_point = CString::new("vs_main").unwrap();
        let fs_entry_point = CString::new("fs_main").unwrap();
        let stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vs_module)
                .name(&entry_point),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(fs_module)
                .name(&fs_entry_point),
        ];

        let binding_desc = [vk::VertexInputBindingDescription {
            binding: 0,
            stride: std::mem::size_of::<[f32; 2]>() as u32,
            input_rate: vk::VertexInputRate::VERTEX,
        }];
        let attr_desc = [vk::VertexInputAttributeDescription {
            location: 0,
            binding: 0,
            format: vk::Format::R32G32_SFLOAT,
            offset: 0,
        }];
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(&binding_desc)
            .vertex_attribute_descriptions(&attr_desc);
        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: width as f32,
            height: height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        let scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D { width, height },
        };
        let viewports = [viewport];
        let scissors = [scissor];
        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewports(&viewports)
            .scissors(&scissors);
        let raster = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .line_width(1.0);
        let multisample = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);
        // Straight (non-premultiplied) alpha blend — matches this crate's
        // own `render_clear_to_pixels`/readback contract ("straight-alpha
        // RGBA8"), so a caller does not have to remember two different
        // alpha conventions depending on which method it called.
        let blend_attachment = vk::PipelineColorBlendAttachmentState::default()
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD)
            .color_write_mask(vk::ColorComponentFlags::RGBA);
        let blend_attachments = [blend_attachment];
        let color_blend =
            vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend_attachments);

        let pipeline_layout_info =
            vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts);
        let pipeline_layout =
            unsafe { device.create_pipeline_layout(&pipeline_layout_info, None) }?;

        let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&raster)
            .multisample_state(&multisample)
            .color_blend_state(&color_blend)
            .layout(pipeline_layout)
            .render_pass(render_pass)
            .subpass(0);
        let pipelines = unsafe {
            device.create_graphics_pipelines(self.pipeline_cache(), &[pipeline_info], None)
        }
        .map_err(|(_, e)| VulkanError::Vulkan(e.to_string()))?;
        let pipeline = pipelines[0];

        // --- readback buffer -----------------------------------------------------------
        let readback_size = (width as u64) * (height as u64) * 4;
        let readback_info = vk::BufferCreateInfo::default()
            .size(readback_size)
            .usage(vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let readback_buffer = unsafe { device.create_buffer(&readback_info, None) }?;
        let readback_req = unsafe { device.get_buffer_memory_requirements(readback_buffer) };
        let readback_mem_type = self.find_memory_type(
            readback_req.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        let readback_alloc = vk::MemoryAllocateInfo::default()
            .allocation_size(readback_req.size)
            .memory_type_index(readback_mem_type);
        let readback_memory = unsafe { device.allocate_memory(&readback_alloc, None) }?;
        unsafe { device.bind_buffer_memory(readback_buffer, readback_memory, 0) }?;

        // --- command buffer -----------------------------------------------------------
        let pool_info = vk::CommandPoolCreateInfo::default().queue_family_index(self.queue_family);
        let command_pool = unsafe { device.create_command_pool(&pool_info, None) }?;
        let cmd_alloc = vk::CommandBufferAllocateInfo::default()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let command_buffer = unsafe { device.allocate_command_buffers(&cmd_alloc) }?[0];

        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe { device.begin_command_buffer(command_buffer, &begin_info) }?;

        let clear_value = vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.0, 0.0, 0.0, 0.0],
            },
        };
        let clear_values = [clear_value];
        let rp_begin = vk::RenderPassBeginInfo::default()
            .render_pass(render_pass)
            .framebuffer(framebuffer)
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: vk::Extent2D { width, height },
            })
            .clear_values(&clear_values);
        unsafe {
            device.cmd_begin_render_pass(command_buffer, &rp_begin, vk::SubpassContents::INLINE);
            device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, pipeline);
            device.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline_layout,
                0,
                &[descriptor_set],
                &[],
            );
            device.cmd_bind_vertex_buffers(command_buffer, 0, &[vertex_buffer], &[0]);
            device.cmd_bind_index_buffer(command_buffer, index_buffer, 0, vk::IndexType::UINT32);
            device.cmd_draw_indexed(command_buffer, indices.len() as u32, 1, 0, 0, 0);
            device.cmd_end_render_pass(command_buffer);

            let copy_region = vk::BufferImageCopy {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                image_subresource: vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
                image_extent: vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                },
            };
            device.cmd_copy_image_to_buffer(
                command_buffer,
                image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                readback_buffer,
                &[copy_region],
            );
        }
        unsafe { device.end_command_buffer(command_buffer) }?;

        let fence_info = vk::FenceCreateInfo::default();
        let fence = unsafe { device.create_fence(&fence_info, None) }?;
        let command_buffers = [command_buffer];
        let submit_info = vk::SubmitInfo::default().command_buffers(&command_buffers);
        unsafe {
            device.queue_submit(self.queue, &[submit_info], fence)?;
            device.wait_for_fences(&[fence], true, u64::MAX)?;
        }

        let mut pixels = vec![0u8; readback_size as usize];
        unsafe {
            let ptr = device.map_memory(
                readback_memory,
                0,
                readback_size,
                vk::MemoryMapFlags::empty(),
            )? as *const u8;
            std::ptr::copy_nonoverlapping(ptr, pixels.as_mut_ptr(), readback_size as usize);
            device.unmap_memory(readback_memory);
        }

        unsafe {
            device.destroy_fence(fence, None);
            device.destroy_command_pool(command_pool, None);
            device.destroy_pipeline(pipeline, None);
            device.destroy_pipeline_layout(pipeline_layout, None);
            device.destroy_shader_module(vs_module, None);
            device.destroy_shader_module(fs_module, None);
            device.destroy_descriptor_pool(descriptor_pool, None);
            device.destroy_descriptor_set_layout(set_layout, None);
            device.free_memory(readback_memory, None);
            device.destroy_buffer(readback_buffer, None);
            device.free_memory(ubo_memory, None);
            device.destroy_buffer(ubo, None);
            device.free_memory(index_memory, None);
            device.destroy_buffer(index_buffer, None);
            device.free_memory(vertex_memory, None);
            device.destroy_buffer(vertex_buffer, None);
            device.destroy_framebuffer(framebuffer, None);
            device.destroy_render_pass(render_pass, None);
            device.destroy_image_view(image_view, None);
            device.free_memory(image_memory, None);
            device.destroy_image(image, None);
        }

        Ok(pixels)
    }

    /// Render the **unlit 3D forward pass** — `vieww-3d`'s `GpuRenderer`'s
    /// first increment — and read the result back as straight-alpha RGBA8.
    ///
    /// What this draws: `positions` (world space, metres, whatever the caller's
    /// `vp` expects) as an indexed triangle list, each vertex carrying its
    /// item's flat linear `colors` entry, transformed by the 16-float
    /// column-major `vp` (vieww-3d's `Mat4` flattens to exactly this layout),
    /// depth-tested against a depth attachment, and written sRGB-encoded with
    /// alpha 1 — the byte-for-byte contract of `vieww_3d::render::Renderer`
    /// for an all-`Basic`-materials scene, which is the parity oracle. No
    /// lights, no shadows, no textures, no blending: that is what "unlit
    /// forward pass, first increment" means; the shader is
    /// [`vieww_shaders::library::UNLIT3D_WGSL`].
    ///
    /// `background` is the scene's clear colour in **linear** light — encoded
    /// here with the same sRGB curve the fragment shader uses, so empty
    /// regions match the CPU's `to_image` background bytes.
    ///
    /// Like [`render_mesh_to_pixels`](Self::render_mesh_to_pixels), this takes
    /// plain slices so `vieww-hal` gains no new dependency (the 3D types stay
    /// in `vieww-3d`, the pipeline here), and like it this brings the whole
    /// pipeline up per call and tears it down after — a one-shot shape whose
    /// cached-pipeline successor is the same step `SceneRenderer` was for the
    /// 2D path. Error returns before the manual teardown leak that call's
    /// objects, exactly as in `render_mesh_to_pixels`; the success path (the
    /// only path a healthy device ever takes) frees everything.
    ///
    /// One deliberate choice to name: **back-face culling is off.** The CPU
    /// rasterizer culls per-triangle by screen-space winding; the GPU pass
    /// lets the depth buffer arbitrate instead, which produces identical
    /// pixels for closed opaque geometry — the only kind the unlit pass's
    /// contract covers today — and defers the winding-convention question to
    /// the increment that adds lighting (where `double_sided` starts to
    /// matter).
    ///
    /// # Errors
    ///
    /// Any Vulkan call failing — as with the other render methods, most
    /// commonly at instance/device creation on a machine with no ICD, which
    /// fails before this method is reachable.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::too_many_arguments,
        clippy::too_many_lines
    )]
    pub fn render_unlit3d_to_pixels(
        &self,
        width: u32,
        height: u32,
        positions: &[[f32; 3]],
        colors: &[[f32; 3]],
        indices: &[u32],
        vp: [f32; 16],
        background: [f32; 3],
    ) -> Result<Vec<u8>, VulkanError> {
        assert_eq!(
            positions.len(),
            colors.len(),
            "one colour per vertex — the caller packs them together"
        );
        // Interleaved: position at offset 0, colour at 12, stride 24.
        let mut vertices = Vec::with_capacity(positions.len() * 6);
        for (p, c) in positions.iter().zip(colors) {
            vertices.extend_from_slice(p);
            vertices.extend_from_slice(c);
        }
        let batches = [Batch3d {
            first_index: 0,
            index_count: indices.len() as u32,
            depth_write: true,
            blend: false,
            alpha_pass: 0.0,
            texture: None,
        }];
        self.render_3d_pass(
            width,
            height,
            &Pass3d {
                wgsl: vieww_shaders::library::UNLIT3D_WGSL,
                vertices: bytemuck_cast_f32(&vertices),
                stride: 24,
                attributes: &[
                    (0, vk::Format::R32G32B32_SFLOAT, 0),
                    (1, vk::Format::R32G32B32_SFLOAT, 12),
                ],
                indices,
                uniforms: bytemuck_cast_f32(&vp),
                storage: None,
                sampled: false,
                push_constants: false,
                batches: &batches,
            },
            background,
        )
    }

    /// Render the **lit 3D forward pass** — `vieww-3d`'s `GpuRenderer`'s
    /// second increment, textures and transparency included — and read the
    /// result back as straight-alpha RGBA8.
    ///
    /// `vertices` is a de-indexed triangle list (three per triangle, in
    /// order), each vertex twenty floats: world position, world normal,
    /// linear albedo, linear emissive, each `xyz` then one float — the first
    /// of those four is 1 for Lambert and 0 for Basic, the second 1 if the
    /// material receives shadows, the last two Phong's `shininess` and
    /// `specular` — then the material's texture coordinates `uv`, its
    /// `opacity`, and 1 if a texture is bound for its batch (the
    /// transparent-split inputs; see `lit3d.wgsl`).
    ///
    /// `batches` draws the geometry in order, each batch through its own
    /// texture (the CPU oracle's linear-light texels, uploaded as f16), its
    /// own sampler (nearest or bilinear), its own depth-write and blend
    /// state, and its own half of the transparent split (`alpha_pass`) —
    /// `vieww-3d/src/gpu.rs` builds them from the CPU renderer's own
    /// opaque/transparent pass order.
    ///
    /// `uniforms` is the shader's `Uniforms` block, std140, as
    /// [`vieww_shaders::library::LIT3D_WGSL`] documents it; `shadow_depth` the
    /// shadow map's depths (empty when there is none). `background` is
    /// linear, as in [`Self::render_unlit3d_to_pixels`], and so are the
    /// lifetime and error notes there.
    ///
    /// # Errors
    ///
    /// Any Vulkan call failing.
    #[allow(
        clippy::too_many_arguments,
        reason = "the pass's own contract: geometry, uniforms, shadow map, batches and background"
    )]
    pub fn render_lit3d_to_pixels(
        &self,
        width: u32,
        height: u32,
        vertices: &[[f32; 20]],
        uniforms: &[f32],
        shadow_depth: &[f32],
        batches: &[Batch3d<'_>],
        background: [f32; 3],
    ) -> Result<Vec<u8>, VulkanError> {
        let flat: &[f32] = vertices.as_flattened();
        let indices: Vec<u32> = (0..u32::try_from(vertices.len()).unwrap_or(u32::MAX)).collect();
        self.render_3d_pass(
            width,
            height,
            &Pass3d {
                wgsl: vieww_shaders::library::LIT3D_WGSL,
                vertices: bytemuck_cast_f32(flat),
                stride: 80,
                attributes: &[
                    (0, vk::Format::R32G32B32_SFLOAT, 0),
                    (1, vk::Format::R32G32B32_SFLOAT, 16),
                    (2, vk::Format::R32G32B32_SFLOAT, 32),
                    (3, vk::Format::R32G32B32_SFLOAT, 48),
                    (4, vk::Format::R32_SFLOAT, 12),
                    (5, vk::Format::R32_SFLOAT, 28),
                    (6, vk::Format::R32_SFLOAT, 44),
                    (7, vk::Format::R32_SFLOAT, 60),
                    (8, vk::Format::R32G32_SFLOAT, 64),
                    (9, vk::Format::R32_SFLOAT, 72),
                    (10, vk::Format::R32_SFLOAT, 76),
                ],
                indices: &indices,
                uniforms: bytemuck_cast_f32(uniforms),
                storage: Some(bytemuck_cast_f32(shadow_depth)),
                sampled: true,
                push_constants: true,
                batches,
            },
            background,
        )
    }

    /// The one-shot depth-tested 3D pass every 3D entry point draws through:
    /// an interleaved vertex buffer described by `pass.attributes`, one
    /// index list drawn as `pass.batches` says — each batch its own texture,
    /// sampler, depth-write and blend state — into a cleared linear RGBA16F
    /// colour + depth target, read back and sRGB-encoded on the CPU exactly
    /// as `vieww-3d`'s `to_image` does. Extracted from the unlit pass so the
    /// 3D entry points cannot drift apart.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::too_many_lines
    )]
    fn render_3d_pass(
        &self,
        width: u32,
        height: u32,
        pass: &Pass3d<'_>,
        background: [f32; 3],
    ) -> Result<Vec<u8>, VulkanError> {
        let indices = pass.indices;
        let vertex_count = pass.vertices.len() / pass.stride as usize;
        // The CPU oracle's background, encoded the way its `to_image`
        // encodes every pixel — including the shaded ones, at the readback.
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        fn srgb(v: f32) -> f32 {
            let v = v.clamp(0.0, 1.0);
            if v <= 0.003_130_8 {
                v * 12.92
            } else {
                1.055 * v.powf(1.0 / 2.4) - 0.055
            }
        }
        let bg = [
            srgb(background[0]),
            srgb(background[1]),
            srgb(background[2]),
        ];
        let bg_byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;

        let total_indices: u64 = pass.batches.iter().map(|b| u64::from(b.index_count)).sum();
        if vertex_count == 0 || indices.is_empty() || total_indices == 0 {
            // Nothing to draw: the frame is the encoded background, exactly
            // what the CPU renderer returns for an empty scene.
            let bg_px = [bg_byte(bg[0]), bg_byte(bg[1]), bg_byte(bg[2]), 255];
            #[allow(clippy::cast_possible_truncation)]
            return Ok(bg_px.repeat(width as usize * height as usize));
        }
        assert_eq!(
            pass.vertices.len() % pass.stride as usize,
            0,
            "the vertex bytes are a whole number of vertices"
        );
        assert!(
            indices.iter().all(|i| (*i as usize) < vertex_count),
            "indices out of range"
        );

        let device = &self.device;
        // Linear RGBA16F: the CPU oracle lerps linear colours and encodes
        // once at `to_image`, so the target the GPU blends into holds linear
        // values too, and the readback encodes them the same way. (The
        // target was UNORM RGBA8 with the encode in the fragment shader
        // until transparency arrived: blending encoded bytes is not the
        // CPU's arithmetic, so a translucent surface's colour would have
        // been blended in the wrong space.)
        let format = vk::Format::R16G16B16A16_SFLOAT;

        // --- colour attachment image (as in render_mesh_to_pixels) -----------
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let image = unsafe { device.create_image(&image_info, None) }?;
        let mem_req = unsafe { device.get_image_memory_requirements(image) };
        let mem_type = self.find_memory_type(
            mem_req.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;
        let image_mem_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_req.size)
            .memory_type_index(mem_type);
        let image_memory = unsafe { device.allocate_memory(&image_mem_info, None) }?;
        unsafe { device.bind_image_memory(image, image_memory, 0) }?;

        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        let image_view = unsafe { device.create_image_view(&view_info, None) }?;

        // --- depth attachment --------------------------------------------------
        // D32_SFLOAT where the device supports it as a depth attachment
        // (virtually everywhere), else the spec-mandatory D16_UNORM.
        let d32 = unsafe {
            self.instance
                .get_physical_device_format_properties(self.physical_device, vk::Format::D32_SFLOAT)
        };
        let depth_format = if d32
            .optimal_tiling_features
            .contains(vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT)
        {
            vk::Format::D32_SFLOAT
        } else {
            vk::Format::D16_UNORM
        };
        let depth_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(depth_format)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let depth_image = unsafe { device.create_image(&depth_info, None) }?;
        let depth_req = unsafe { device.get_image_memory_requirements(depth_image) };
        let depth_mem_type = self.find_memory_type(
            depth_req.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;
        let depth_mem_info = vk::MemoryAllocateInfo::default()
            .allocation_size(depth_req.size)
            .memory_type_index(depth_mem_type);
        let depth_memory = unsafe { device.allocate_memory(&depth_mem_info, None) }?;
        unsafe { device.bind_image_memory(depth_image, depth_memory, 0) }?;
        let depth_view_info = vk::ImageViewCreateInfo::default()
            .image(depth_image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(depth_format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::DEPTH,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        let depth_view = unsafe { device.create_image_view(&depth_view_info, None) }?;

        // --- render pass + framebuffer -----------------------------------------
        let color_attachment = vk::AttachmentDescription::default()
            .format(format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL);
        let depth_attachment = vk::AttachmentDescription::default()
            .format(depth_format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);
        let attachments = [color_attachment, depth_attachment];
        let color_ref = [vk::AttachmentReference {
            attachment: 0,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        }];
        let depth_ref = vk::AttachmentReference {
            attachment: 1,
            layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        };
        let subpass = vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_ref)
            .depth_stencil_attachment(&depth_ref);
        let subpasses = [subpass];
        let render_pass_info = vk::RenderPassCreateInfo::default()
            .attachments(&attachments)
            .subpasses(&subpasses)
            .dependencies(&COPY_AFTER_RENDER);
        let render_pass = unsafe { device.create_render_pass(&render_pass_info, None) }?;

        let fb_attachments = [image_view, depth_view];
        let fb_info = vk::FramebufferCreateInfo::default()
            .render_pass(render_pass)
            .attachments(&fb_attachments)
            .width(width)
            .height(height)
            .layers(1);
        let framebuffer = unsafe { device.create_framebuffer(&fb_info, None) }?;

        // --- vertex (one interleaved binding), index, uniform, storage -------
        let (vertex_buffer, vertex_memory) =
            self.create_host_visible_buffer(vk::BufferUsageFlags::VERTEX_BUFFER, pass.vertices)?;
        let (index_buffer, index_memory) = self.create_host_visible_buffer(
            vk::BufferUsageFlags::INDEX_BUFFER,
            bytemuck_cast_u32(indices),
        )?;
        let (ubo, ubo_memory) =
            self.create_host_visible_buffer(vk::BufferUsageFlags::UNIFORM_BUFFER, pass.uniforms)?;
        // Bytes, not elements — a float count here hid three quarters of the
        // view-projection matrix from the shader once (observed on lavapipe
        // as every column but the first reading as zero).
        let ubo_size = pass.uniforms.len() as u64;
        // A storage buffer may not be empty; an absent or empty one is a
        // single zero word the shader never reads.
        let storage_bytes: &[u8] = match pass.storage {
            Some(bytes) if !bytes.is_empty() => bytes,
            _ => &[0; 4],
        };
        let (ssbo, ssbo_memory) =
            self.create_host_visible_buffer(vk::BufferUsageFlags::STORAGE_BUFFER, storage_bytes)?;
        let ssbo_size = storage_bytes.len() as u64;
        let with_storage = pass.storage.is_some();

        // --- textures and samplers ---------------------------------------------
        // One sampled image per distinct texture, plus a 1x1 transparent
        // dummy so every set binds *something* at bindings 2 and 3 (the
        // shader statically reads them; a set with a hole is invalid).
        // Uploaded as RGBA16F from the oracle's own linear texels, via a
        // staging buffer copied on this pass's command buffer.
        #[derive(Clone, Copy)]
        struct Uploaded {
            image: vk::Image,
            memory: vk::DeviceMemory,
            view: vk::ImageView,
        }
        // Every upload, in the order it happens: the image, the staging
        // buffer, the copy region — the command buffer records these
        // directly, so the image-to-staging pairing can never drift.
        let mut upload_jobs: Vec<(vk::Image, vk::Buffer, vk::BufferImageCopy)> = Vec::new();
        // The staging buffers to free afterwards.
        let mut staging_uploads: Vec<(vk::Buffer, vk::DeviceMemory)> = Vec::new();
        // The textures themselves, for the teardown.
        let mut uploaded_textures: Vec<Uploaded> = Vec::new();
        let mut samplers: Vec<(bool, vk::Sampler)> = Vec::new();
        let sampler_for = |nearest: bool,
                           samplers: &mut Vec<(bool, vk::Sampler)>|
         -> Result<vk::Sampler, VulkanError> {
            if let Some((_, existing)) = samplers.iter().find(|(n, _)| *n == nearest) {
                return Ok(*existing);
            }
            let filter = if nearest {
                vk::Filter::NEAREST
            } else {
                vk::Filter::LINEAR
            };
            let info = vk::SamplerCreateInfo::default()
                .mag_filter(filter)
                .min_filter(filter)
                .address_mode_u(vk::SamplerAddressMode::REPEAT)
                .address_mode_v(vk::SamplerAddressMode::REPEAT)
                .address_mode_w(vk::SamplerAddressMode::REPEAT)
                .max_lod(0.0);
            let sampler = unsafe { device.create_sampler(&info, None) }?;
            samplers.push((nearest, sampler));
            Ok(sampler)
        };
        let mut upload_texture = |texels: &[[f32; 4]],
                                  tex_width: u32,
                                  tex_height: u32,
                                  nearest: bool,
                                  samplers: &mut Vec<(bool, vk::Sampler)>|
         -> Result<(Uploaded, vk::Sampler), VulkanError> {
            let info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(vk::Format::R16G16B16A16_SFLOAT)
                .extent(vk::Extent3D {
                    width: tex_width,
                    height: tex_height,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED);
            let image = unsafe { device.create_image(&info, None) }?;
            let req = unsafe { device.get_image_memory_requirements(image) };
            let memory_type =
                self.find_memory_type(req.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL)?;
            let alloc = vk::MemoryAllocateInfo::default()
                .allocation_size(req.size)
                .memory_type_index(memory_type);
            let memory = unsafe { device.allocate_memory(&alloc, None) }?;
            unsafe { device.bind_image_memory(image, memory, 0) }?;
            let view_info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(vk::Format::R16G16B16A16_SFLOAT)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            let view = unsafe { device.create_image_view(&view_info, None) }?;
            // The staging bytes: the oracle's f32 texels as f16 — the same
            // numbers to both renderers' sampling, within half precision.
            let mut half = Vec::with_capacity(texels.len() * 8);
            for texel in texels {
                for value in texel {
                    half.extend_from_slice(&crate::scene_exec::f32_to_half(*value).to_le_bytes());
                }
            }
            let (staging, staging_memory) =
                self.create_host_visible_buffer(vk::BufferUsageFlags::TRANSFER_SRC, &half)?;
            let copy = vk::BufferImageCopy {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                image_subresource: vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
                image_extent: vk::Extent3D {
                    width: tex_width,
                    height: tex_height,
                    depth: 1,
                },
            };
            upload_jobs.push((image, staging, copy));
            staging_uploads.push((staging, staging_memory));
            let sampler = sampler_for(nearest, samplers)?;
            let uploaded = Uploaded {
                image,
                memory,
                view,
            };
            uploaded_textures.push(Uploaded {
                image,
                memory,
                view,
            });
            Ok((uploaded, sampler))
        };
        // The dummy: 1x1 transparent, its own sampler, first in the upload
        // order.
        let (dummy, dummy_sampler) = if pass.sampled {
            let (dummy, sampler) = upload_texture(&[[0.0; 4]], 1, 1, false, &mut samplers)?;
            (Some(dummy), Some(sampler))
        } else {
            (None, None)
        };
        // One image + sampler per distinct texture, shared by the batches
        // that use it — a texture is identified by its texels' address,
        // which is how `vieww-3d` shares the `Arc`d texels behind it.
        let mut batch_textures: Vec<Option<(Uploaded, vk::Sampler)>> = Vec::new();
        let mut texel_ptrs: Vec<*const [f32; 4]> = Vec::new();
        for batch in pass.batches {
            let uploaded = match &batch.texture {
                Some(texture) => {
                    if let Some(slot) = texel_ptrs
                        .iter()
                        .position(|p| std::ptr::eq(*p, texture.texels.as_ptr()))
                    {
                        batch_textures[slot]
                    } else {
                        texel_ptrs.push(texture.texels.as_ptr());
                        let (uploaded, sampler) = upload_texture(
                            texture.texels,
                            texture.width,
                            texture.height,
                            texture.nearest,
                            &mut samplers,
                        )?;
                        Some((uploaded, sampler))
                    }
                }
                None => None,
            };
            batch_textures.push(uploaded);
        }

        // --- descriptor sets: one per batch --------------------------------------
        let set_count = pass.batches.len().max(1) as u32;
        let mut bindings = vec![vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)];
        if with_storage {
            bindings.push(
                vk::DescriptorSetLayoutBinding::default()
                    .binding(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            );
        }
        if pass.sampled {
            bindings.push(
                vk::DescriptorSetLayoutBinding::default()
                    .binding(2)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            );
            bindings.push(
                vk::DescriptorSetLayoutBinding::default()
                    .binding(3)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            );
        }
        let layout_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
        let set_layout = unsafe { device.create_descriptor_set_layout(&layout_info, None) }?;

        let mut pool_sizes = vec![
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: set_count,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: set_count,
            },
        ];
        if pass.sampled {
            pool_sizes.push(vk::DescriptorPoolSize {
                ty: vk::DescriptorType::SAMPLED_IMAGE,
                descriptor_count: set_count,
            });
            pool_sizes.push(vk::DescriptorPoolSize {
                ty: vk::DescriptorType::SAMPLER,
                descriptor_count: set_count,
            });
        }
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(set_count);
        let descriptor_pool = unsafe { device.create_descriptor_pool(&pool_info, None) }?;
        let set_layouts = [set_layout];
        let set_alloc = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&set_layouts);
        let descriptor_sets = unsafe { device.allocate_descriptor_sets(&set_alloc)? };

        let buffer_infos = [vk::DescriptorBufferInfo {
            buffer: ubo,
            offset: 0,
            range: ubo_size,
        }];
        let storage_infos = [vk::DescriptorBufferInfo {
            buffer: ssbo,
            offset: 0,
            range: ssbo_size,
        }];
        // The per-batch image/sampler infos live in their own vectors so
        // every write can borrow from them and all of it outlives the one
        // `update_descriptor_sets` call that reads it — collected first, so
        // the write-building pass only borrows.
        let mut batch_image_infos: Vec<[vk::DescriptorImageInfo; 1]> = Vec::new();
        let mut batch_sampler_infos: Vec<[vk::DescriptorImageInfo; 1]> = Vec::new();
        if pass.sampled {
            for (index, _) in descriptor_sets.iter().enumerate() {
                let (view, sampler) = match &batch_textures[index] {
                    Some((uploaded, sampler)) => (uploaded.view, *sampler),
                    None => (
                        dummy
                            .as_ref()
                            .expect("a sampled pass has a dummy texture")
                            .view,
                        dummy_sampler.expect("a sampled pass has a dummy sampler"),
                    ),
                };
                batch_image_infos.push([vk::DescriptorImageInfo {
                    sampler: vk::Sampler::null(),
                    image_view: view,
                    image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                }]);
                batch_sampler_infos.push([vk::DescriptorImageInfo {
                    sampler,
                    image_view: vk::ImageView::null(),
                    image_layout: vk::ImageLayout::UNDEFINED,
                }]);
            }
        }
        let mut writes: Vec<vk::WriteDescriptorSet> = Vec::new();
        for (index, set) in descriptor_sets.iter().enumerate() {
            writes.push(
                vk::WriteDescriptorSet::default()
                    .dst_set(*set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&buffer_infos),
            );
            if with_storage {
                writes.push(
                    vk::WriteDescriptorSet::default()
                        .dst_set(*set)
                        .dst_binding(1)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(&storage_infos),
                );
            }
            if pass.sampled {
                writes.push(
                    vk::WriteDescriptorSet::default()
                        .dst_set(*set)
                        .dst_binding(2)
                        .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                        .image_info(&batch_image_infos[index]),
                );
                writes.push(
                    vk::WriteDescriptorSet::default()
                        .dst_set(*set)
                        .dst_binding(3)
                        .descriptor_type(vk::DescriptorType::SAMPLER)
                        .image_info(&batch_sampler_infos[index]),
                );
            }
        }
        unsafe { device.update_descriptor_sets(&writes, &[]) };

        // --- shaders + the pipeline variants -------------------------------------
        let vs_spirv = translate_wgsl_to_spirv(pass.wgsl, "vs_main", Stage::Vertex)?;
        let fs_spirv = translate_wgsl_to_spirv(pass.wgsl, "fs_main", Stage::Fragment)?;
        let vs_module = unsafe {
            device
                .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&vs_spirv), None)
        }?;
        let fs_module = unsafe {
            device
                .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&fs_spirv), None)
        }?;

        let entry_point = CString::new("vs_main").unwrap();
        let fs_entry_point = CString::new("fs_main").unwrap();
        let stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vs_module)
                .name(&entry_point),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(fs_module)
                .name(&fs_entry_point),
        ];

        // One interleaved binding, laid out by the caller's attribute list.
        let binding_desc = [vk::VertexInputBindingDescription {
            binding: 0,
            stride: pass.stride,
            input_rate: vk::VertexInputRate::VERTEX,
        }];
        let attr_desc: Vec<vk::VertexInputAttributeDescription> = pass
            .attributes
            .iter()
            .map(
                |&(location, format, offset)| vk::VertexInputAttributeDescription {
                    location,
                    binding: 0,
                    format,
                    offset,
                },
            )
            .collect();
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(&binding_desc)
            .vertex_attribute_descriptions(&attr_desc);
        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: width as f32,
            height: height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        let scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D { width, height },
        };
        let viewports = [viewport];
        let scissors = [scissor];
        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewports(&viewports)
            .scissors(&scissors);
        // Culling deliberately off — see the method doc.
        let raster = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .line_width(1.0);
        let multisample = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);

        // The transparent split's state pairs: depth writes for the
        // `a >= 1.0` half, straight-alpha-over blending in linear light for
        // the `a < 1.0` one — the CPU oracle's write-or-lerp, expressed as
        // pipeline state. One variant per distinct pair the batches use.
        let mut variants: Vec<((bool, bool), vk::Pipeline)> = Vec::new();
        let mut push_ranges = Vec::new();
        if pass.push_constants {
            push_ranges.push(
                vk::PushConstantRange::default()
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                    .offset(0)
                    .size(4),
            );
        }
        let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&set_layouts)
            .push_constant_ranges(&push_ranges);
        let pipeline_layout =
            unsafe { device.create_pipeline_layout(&pipeline_layout_info, None) }?;
        for batch in pass.batches {
            let key = (batch.depth_write, batch.blend);
            if variants.iter().any(|(k, _)| *k == key) {
                continue;
            }
            // The depth state is the point of the 3D path: LESS keeps the
            // nearest surface, the same contest the CPU's per-sample depth
            // test runs; the write is the variant's half of the split.
            let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(true)
                .depth_write_enable(batch.depth_write)
                .depth_compare_op(vk::CompareOp::LESS);
            // Straight-alpha source-over on a linear target: the CPU's
            // `lerp(color, a)` — `rgb = src.rgb * a + dst.rgb * (1 - a)`,
            // `alpha = a + dst.a * (1 - a)`.
            let blend_attachment = if batch.blend {
                vk::PipelineColorBlendAttachmentState::default()
                    .blend_enable(true)
                    .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
                    .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                    .color_blend_op(vk::BlendOp::ADD)
                    .src_alpha_blend_factor(vk::BlendFactor::ONE)
                    .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                    .alpha_blend_op(vk::BlendOp::ADD)
                    .color_write_mask(vk::ColorComponentFlags::RGBA)
            } else {
                vk::PipelineColorBlendAttachmentState::default()
                    .blend_enable(false)
                    .color_write_mask(vk::ColorComponentFlags::RGBA)
            };
            let blend_attachments = [blend_attachment];
            let color_blend =
                vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend_attachments);
            let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
                .stages(&stages)
                .vertex_input_state(&vertex_input)
                .input_assembly_state(&input_assembly)
                .viewport_state(&viewport_state)
                .rasterization_state(&raster)
                .multisample_state(&multisample)
                .depth_stencil_state(&depth_stencil)
                .color_blend_state(&color_blend)
                .layout(pipeline_layout)
                .render_pass(render_pass)
                .subpass(0);
            let pipelines = unsafe {
                device.create_graphics_pipelines(self.pipeline_cache(), &[pipeline_info], None)
            }
            .map_err(|(_, e)| VulkanError::Vulkan(e.to_string()))?;
            variants.push((key, pipelines[0]));
        }

        // --- readback buffer ------------------------------------------------------
        // f16 in, four bytes out — the readback holds the raw linear target.
        let readback_size = (width as u64) * (height as u64) * 8;
        let readback_info = vk::BufferCreateInfo::default()
            .size(readback_size)
            .usage(vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let readback_buffer = unsafe { device.create_buffer(&readback_info, None) }?;
        let readback_req = unsafe { device.get_buffer_memory_requirements(readback_buffer) };
        let readback_mem_type = self.find_memory_type(
            readback_req.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        let readback_alloc = vk::MemoryAllocateInfo::default()
            .allocation_size(readback_req.size)
            .memory_type_index(readback_mem_type);
        let readback_memory = unsafe { device.allocate_memory(&readback_alloc, None) }?;
        unsafe { device.bind_buffer_memory(readback_buffer, readback_memory, 0) }?;

        // --- command buffer ---------------------------------------------------------
        let pool_info = vk::CommandPoolCreateInfo::default().queue_family_index(self.queue_family);
        let command_pool = unsafe { device.create_command_pool(&pool_info, None) }?;
        let cmd_alloc = vk::CommandBufferAllocateInfo::default()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let command_buffer = unsafe { device.allocate_command_buffers(&cmd_alloc) }?[0];

        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe { device.begin_command_buffer(command_buffer, &begin_info)? };

        // The texture uploads, before the render pass: every staged f16
        // buffer into its image, UNDEFINED -> TRANSFER_DST -> SHADER_READ.
        for (image, staging, copy) in &upload_jobs {
            unsafe {
                device.cmd_pipeline_barrier(
                    command_buffer,
                    vk::PipelineStageFlags::ALL_COMMANDS,
                    vk::PipelineStageFlags::ALL_COMMANDS,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[vk::ImageMemoryBarrier::default()
                        .old_layout(vk::ImageLayout::UNDEFINED)
                        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .image(*image)
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            base_mip_level: 0,
                            level_count: 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        })],
                );
                device.cmd_copy_buffer_to_image(
                    command_buffer,
                    *staging,
                    *image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    std::slice::from_ref(copy),
                );
                device.cmd_pipeline_barrier(
                    command_buffer,
                    vk::PipelineStageFlags::ALL_COMMANDS,
                    vk::PipelineStageFlags::ALL_COMMANDS,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[vk::ImageMemoryBarrier::default()
                        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .image(*image)
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            base_mip_level: 0,
                            level_count: 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        })],
                );
            }
        }

        let clear_values = [
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [background[0], background[1], background[2], 1.0],
                },
            },
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            },
        ];
        let rp_begin = vk::RenderPassBeginInfo::default()
            .render_pass(render_pass)
            .framebuffer(framebuffer)
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: vk::Extent2D { width, height },
            })
            .clear_values(&clear_values);
        unsafe {
            device.cmd_begin_render_pass(command_buffer, &rp_begin, vk::SubpassContents::INLINE);
            device.cmd_bind_vertex_buffers(command_buffer, 0, &[vertex_buffer], &[0]);
            device.cmd_bind_index_buffer(command_buffer, index_buffer, 0, vk::IndexType::UINT32);
            for (index, batch) in pass.batches.iter().enumerate() {
                if batch.index_count == 0 {
                    continue;
                }
                let pipeline = &variants
                    .iter()
                    .find(|(key, _)| *key == (batch.depth_write, batch.blend))
                    .expect("every batch's state pair has a pipeline")
                    .1;
                device.cmd_bind_pipeline(
                    command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    *pipeline,
                );
                device.cmd_bind_descriptor_sets(
                    command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    pipeline_layout,
                    0,
                    &[descriptor_sets[index]],
                    &[],
                );
                if pass.push_constants {
                    let alpha_pass = [batch.alpha_pass];
                    // Four bytes of an `f32`, read as bytes for the push
                    // constant write.
                    let alpha_bytes: &[u8] = std::slice::from_raw_parts(
                        alpha_pass.as_ptr().cast::<u8>(),
                        std::mem::size_of_val(&alpha_pass),
                    );
                    device.cmd_push_constants(
                        command_buffer,
                        pipeline_layout,
                        vk::ShaderStageFlags::FRAGMENT,
                        0,
                        alpha_bytes,
                    );
                }
                device.cmd_draw_indexed(
                    command_buffer,
                    batch.index_count,
                    1,
                    batch.first_index,
                    0,
                    0,
                );
            }
            device.cmd_end_render_pass(command_buffer);

            let copy_region = vk::BufferImageCopy {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                image_subresource: vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
                image_extent: vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                },
            };
            device.cmd_copy_image_to_buffer(
                command_buffer,
                image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                readback_buffer,
                &[copy_region],
            );
        }
        unsafe { device.end_command_buffer(command_buffer) }?;

        let fence_info = vk::FenceCreateInfo::default();
        let fence = unsafe { device.create_fence(&fence_info, None) }?;
        let command_buffers = [command_buffer];
        let submit_info = vk::SubmitInfo::default().command_buffers(&command_buffers);
        unsafe {
            device.queue_submit(self.queue, &[submit_info], fence)?;
            device.wait_for_fences(&[fence], true, u64::MAX)?;
        }

        // The readback, encoded: linear f16 -> sRGB bytes + alpha 255, the
        // CPU oracle's own `to_image` resolve, on the CPU.
        let mut half = vec![0u16; width as usize * height as usize * 4];
        unsafe {
            let ptr = device.map_memory(
                readback_memory,
                0,
                readback_size,
                vk::MemoryMapFlags::empty(),
            )? as *const u16;
            std::ptr::copy_nonoverlapping(ptr, half.as_mut_ptr(), half.len());
            device.unmap_memory(readback_memory);
        }
        let mut pixels = vec![0u8; half.len()];
        for (texel, out) in half
            .as_chunks::<4>()
            .0
            .iter()
            .zip(pixels.as_chunks_mut::<4>().0)
        {
            let rgb = [
                crate::scene_exec::half_to_f32(texel[0]),
                crate::scene_exec::half_to_f32(texel[1]),
                crate::scene_exec::half_to_f32(texel[2]),
            ];
            out.copy_from_slice(&[
                bg_byte(srgb(rgb[0])),
                bg_byte(srgb(rgb[1])),
                bg_byte(srgb(rgb[2])),
                255,
            ]);
        }

        unsafe {
            device.destroy_fence(fence, None);
            device.destroy_command_pool(command_pool, None);
            for (_, pipeline) in variants {
                device.destroy_pipeline(pipeline, None);
            }
            device.destroy_pipeline_layout(pipeline_layout, None);
            device.destroy_shader_module(vs_module, None);
            device.destroy_shader_module(fs_module, None);
            for (_, sampler) in samplers {
                device.destroy_sampler(sampler, None);
            }
            for uploaded in uploaded_textures {
                device.destroy_image_view(uploaded.view, None);
                device.free_memory(uploaded.memory, None);
                device.destroy_image(uploaded.image, None);
            }
            for (staging, memory) in staging_uploads {
                device.free_memory(memory, None);
                device.destroy_buffer(staging, None);
            }
            device.destroy_descriptor_pool(descriptor_pool, None);
            device.destroy_descriptor_set_layout(set_layout, None);
            device.free_memory(readback_memory, None);
            device.destroy_buffer(readback_buffer, None);
            device.free_memory(ubo_memory, None);
            device.destroy_buffer(ubo, None);
            device.free_memory(index_memory, None);
            device.destroy_buffer(index_buffer, None);
            device.free_memory(ssbo_memory, None);
            device.destroy_buffer(ssbo, None);
            device.free_memory(vertex_memory, None);
            device.destroy_buffer(vertex_buffer, None);
            device.destroy_framebuffer(framebuffer, None);
            device.destroy_render_pass(render_pass, None);
            device.destroy_image_view(depth_view, None);
            device.free_memory(depth_memory, None);
            device.destroy_image(depth_image, None);
            device.destroy_image_view(image_view, None);
            device.free_memory(image_memory, None);
            device.destroy_image(image, None);
        }

        Ok(pixels)
    }

    /// Allocate a host-visible, host-coherent buffer of `usage`, upload
    /// `data` into it immediately, and return the buffer and its backing
    /// memory (owned by the caller — freed alongside everything else at the
    /// end of whichever `render_*` call created it, matching the existing
    /// uniform/readback buffers' lifetime pattern above).
    ///
    /// Host-visible rather than staged through a device-local buffer via a
    /// transfer queue: correct and simple, at the cost of being the wrong
    /// choice for a large, frequently-updated vertex buffer on a discrete
    /// GPU (a real per-frame path would stage through
    /// `vieww_gpu::resources::UploadQueue` instead) — exactly the kind of
    /// tradeoff `vieww-gpu`'s own resource-system docs call out as this
    /// crate's job to eventually replace, not this one-shot render
    /// function's.
    fn create_host_visible_buffer(
        &self,
        usage: vk::BufferUsageFlags,
        data: &[u8],
    ) -> Result<(vk::Buffer, vk::DeviceMemory), VulkanError> {
        let device = &self.device;
        let size = data.len() as u64;
        let info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer = unsafe { device.create_buffer(&info, None) }?;
        let req = unsafe { device.get_buffer_memory_requirements(buffer) };
        let mem_type = self.find_memory_type(
            req.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        let alloc = vk::MemoryAllocateInfo::default()
            .allocation_size(req.size)
            .memory_type_index(mem_type);
        let memory = unsafe { device.allocate_memory(&alloc, None) }?;
        unsafe { device.bind_buffer_memory(buffer, memory, 0) }?;
        unsafe {
            let ptr = device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty())? as *mut u8;
            ptr.copy_from_nonoverlapping(data.as_ptr(), data.len());
            device.unmap_memory(memory);
        }
        Ok((buffer, memory))
    }
}

fn bytemuck_cast_positions(positions: &[[f32; 2]]) -> &[u8] {
    // SAFETY: `[f32; 2]` has no padding and no alignment requirement beyond
    // 4 bytes, which `u8` satisfies trivially — reinterpreting a `&[[f32;
    // 2]]` as `&[u8]` of `positions.len() * 8` bytes is exactly what
    // `bytemuck::cast_slice` does, inlined here so this module gains no new
    // dependency for one cast used in only two call sites.
    unsafe {
        std::slice::from_raw_parts(
            positions.as_ptr().cast::<u8>(),
            std::mem::size_of_val(positions),
        )
    }
}

/// One textured, blended slice of a 3D pass's geometry — see
/// [`VulkanDevice::render_lit3d_to_pixels`].
#[derive(Debug, Clone, Copy)]
pub struct Texture3d<'a> {
    /// Texel columns.
    pub width: u32,
    /// Texel rows.
    pub height: u32,
    /// Linear-light RGBA, row-major, origin top row — the CPU oracle's own
    /// texels ([`vieww_3d::scene::Texture::texels`]), uploaded as f16.
    pub texels: &'a [[f32; 4]],
    /// `true` for nearest-neighbour sampling, `false` for bilinear — the
    /// material's own `Filter`.
    pub nearest: bool,
}

/// One draw of a 3D pass: `indices[first_index ..][.. index_count]` through
/// one texture, one depth-write and blend state, and one half of the
/// transparent split.
#[derive(Debug, Clone, Copy)]
pub struct Batch3d<'a> {
    /// Offset of the first index.
    pub first_index: u32,
    /// How many indices this batch draws.
    pub index_count: u32,
    /// Whether the depth test writes (the `a >= 1.0` half of the split).
    pub depth_write: bool,
    /// Whether the colour blends straight-alpha-over (the `a < 1.0` half).
    pub blend: bool,
    /// The batch's half of the transparent split: 0 keeps `a >= 1.0`
    /// fragments, 1 keeps `a < 1.0`. See `lit3d.wgsl`.
    pub alpha_pass: f32,
    /// The batch's texture, if it has one.
    pub texture: Option<Texture3d<'a>>,
}

/// What [`VulkanDevice::render_3d_pass`] draws.
struct Pass3d<'a> {
    wgsl: &'a str,
    /// Interleaved vertex bytes, `stride` per vertex.
    vertices: &'a [u8],
    stride: u32,
    /// `(location, format, byte offset)` per vertex attribute.
    attributes: &'a [(u32, vk::Format, u32)],
    indices: &'a [u32],
    /// Binding 0, visible to both stages.
    uniforms: &'a [u8],
    /// Binding 1, fragment stage, when present.
    storage: Option<&'a [u8]>,
    /// Whether the shader samples a texture at binding 2 (and a sampler at
    /// 3) — the lit pass does, the unlit pass does not.
    sampled: bool,
    /// Whether the shader reads `var<immediate>` push constants (the lit
    /// pass's transparent-split flag).
    push_constants: bool,
    /// The draws, in order.
    batches: &'a [Batch3d<'a>],
}

fn bytemuck_cast_u32(values: &[u32]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(values))
    }
}

fn bytemuck_cast_f32(values: &[f32]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(values))
    }
}
