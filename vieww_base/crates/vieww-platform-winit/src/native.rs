//! Windowed presentation on `vieww`'s own stack, end to end: scenes are
//! rasterised on the CPU by `vieww_paint::native::NativeRenderer` (the
//! ground-up engine that replaced `vello_cpu`) and the resulting pixels are
//! presented through `vieww_hal::vulkan`'s real `VK_KHR_swapchain` — raw
//! Vulkan, no `wgpu`, no vello anywhere in this path — this is this crate's
//! one and only renderer.
//!
//! # What this is
//!
//! [`NativeRenderer::for_window`] opens a window's surface via
//! `vieww_hal::vulkan::VulkanDevice::for_window` and
//! [`NativeRenderer::present`] rasterises a [`vieww_paint::Scene`] to
//! straight-alpha RGBA8 pixels, then hands them to
//! `VulkanDevice::present_pixels`, which uploads them and copies them onto
//! the next swapchain image with `vkCmdCopyBufferToImage` — see
//! `vieww-hal`'s `vulkan::swapchain` module for the Vulkan half of this and
//! why it is correct rather than fast. **Nothing in this module, or anything
//! it calls, touches vello, `vello_cpu`, `vello_hybrid` or `wgpu`.**
//!
//! # Two ways a frame reaches the screen
//!
//! A window presents through one of two [`PresentPath`]s, chosen when it
//! opens:
//!
//! - **[`PresentPath::Cpu`]** — the default. The CPU rasterizer's pixels go to
//!   the OS compositor through `softbuffer` (X11 SHM, `wl_shm`, GDI,
//!   `ANativeWindow_lock`, CoreGraphics), and **no GPU driver is loaded at
//!   all**. Only the rows a frame's damage touched are converted, on platforms
//!   whose buffers say how old they are (see `CpuPresenter`).
//! - **[`PresentPath::Vulkan`]** — a window that asked for the GPU
//!   (`prefer_gpu`), or set `VIEWW_PRESENT=vulkan`, or whose CPU present could
//!   not open. Everything below describes this path.
//!
//! Why the split: a window that never draws on the GPU was loading the Vulkan
//! loader, every installed ICD and the GPU driver just to copy a CPU buffer
//! onto the screen. On the reference laptop that was most of the window's
//! resident memory, ~400 ms of Windows start-up (device 215 ms, first present
//! ~200 ms), and on an LG Stylo 5 half of each CPU frame (present 19.8 ms of
//! 41 ms) — `PENDING.md` §3.5 and §4.2 have the measurements.
//!
//! # What this is not, yet
//!
//! - **Damage-region rasterisation, whole-buffer upload.**
//!   [`present_damaged`](NativeRenderer::present_damaged) repaints only the
//!   regions a frame's `Damage` names, onto pixels the renderer retains
//!   between frames; [`present`](NativeRenderer::present) is the
//!   repaint-everything case a resize and a first frame want. What is *not*
//!   yet incremental is the upload: a swapchain hands out a different image
//!   each frame, so the whole buffer is still copied onto it. Narrowing that
//!   needs `VK_KHR_incremental_present` and a per-image record of what each
//!   already holds — see `present_damaged`'s own docs.
//! - **No multi-window device sharing.** Every
//!   [`NativeRenderer::for_window`] builds its own `VulkanDevice`; there is
//!   no shared-instance equivalent to what `vieww_paint::gpu`'s `GpuContext`
//!   used to give the vello path. One `vieww-hal` device per window is
//!   correct, just not the cheapest shape — a follow-up, not a defect.
//! - **GPU rasterisation goes through host memory.**
//!   [`present_planned`](NativeRenderer::present_planned) is the GPU path:
//!   scenes the [`Planner`](vieww_gpu::Planner) can express are executed by
//!   `vieww-hal`'s `SceneRenderer` — one batched draw per frame — and their
//!   pixels are presented through this same swapchain, with a CPU fallback
//!   for plans with gaps. But the GPU's frame is read back into host memory
//!   and uploaded like the CPU's, because `SceneRenderer` renders into its
//!   own framebuffer, not into the swapchain image directly. Rendering
//!   straight onto the presentable image is the follow-up that makes the
//!   GPU path a *fast* windowed path rather than a correct one.
//!
//! What *is* resolved: HiDPI rasterisation. A frame whose logical size does
//! not match the swapchain's extent is lifted into physical pixels through
//! [`Scene::scaled`](vieww_paint::Scene::scaled) — scan-converting glyph
//! outlines and hairlines at the device's own resolution — rather than
//! rasterised small and magnified. See [`NativeRenderer::present_damaged`].
//!
//! This module *is* `crate::app`'s one and only renderer — vello,
//! `vello_cpu`, `vello_hybrid` and `wgpu` are gone from this crate and from
//! `vieww-paint` entirely, not merely unused by default. See
//! `docs/RENDERER-MIGRATION.md` for the full account of the migration.
//!
//! # Verification
//!
//! This sandbox has a real (virtual, via `Xvfb`) X server, so this path is
//! proven the direct way: this crate's examples (`hello`, `gallery`, `grid`,
//! …) and `apps/viewwstudio` are run for real under it, through this exact
//! `App` → `crate::app` → [`NativeRenderer`] code path, and the live
//! window is screenshotted (ImageMagick's `import`) rather than checked
//! through an offscreen stand-in. `vieww-hal`'s own
//! `tests/vulkan_smoke.rs` separately proves the headless half (device,
//! pipeline, render-to-texture-then-readback) against `lavapipe`.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt;
use std::io::Write as _;
// Only the softbuffer present (not Android, which locks the ANativeWindow) uses it.
#[cfg(not(target_os = "android"))]
use std::num::NonZeroU32;
use std::path::Path;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::window::Window;

use vieww_foundation::{Color, Rect};
use vieww_gpu::{PixelRect, Planner as GpuPlanner};
use vieww_hal::vulkan::{
    GpuFrameTimings, GpuPassTimes, Redrawn, SceneRenderer, VulkanDevice, VulkanError,
    VulkanSwapchain,
};
use vieww_paint::native::{
    NativeRenderer as CpuRenderer, RasterKindTimes, RendererError, SceneReport,
};
use vieww_paint::{Damage, Scene};
use vieww_render_planner::{DeviceProfile, Executor, GpuKind, ShadowPlanner};

use crate::scale::{Physical, Scale};

/// Why a windowed present through [`NativeRenderer`] could not happen.
#[derive(Debug)]
pub enum NativeError {
    /// No Vulkan loader was found, or no physical device offers both a
    /// graphics queue and presentation support on this window's surface.
    /// The one variant worth matching on: `vieww-hardware`'s capability
    /// probe and `tests/wait_loop.rs` both recognise a headless runner this
    /// way, exactly as they used to recognise `vieww_paint::gpu::GpuError::NoAdapter`.
    NoAdapter,
    /// A Vulkan call other than adapter/queue selection failed while
    /// opening the device or surface.
    Device(String),
    /// The swapchain image could not be acquired or presented, even after
    /// one rebuild attempt. Ordinary during a resize race; not ordinary
    /// otherwise.
    Present(String),
    /// The CPU rasterizer itself failed — an unbalanced `PushLayer`/
    /// `PopLayer` pair in the scene, the one way
    /// [`vieww_paint::native::NativeRenderer`] can fail at all.
    Render(String),
}

impl fmt::Display for NativeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAdapter => f.write_str("no Vulkan adapter available"),
            Self::Device(message) => write!(f, "opening a Vulkan device/surface: {message}"),
            Self::Present(message) => write!(f, "presenting a frame: {message}"),
            Self::Render(message) => write!(f, "rasterising a scene: {message}"),
        }
    }
}

impl std::error::Error for NativeError {}

impl From<RendererError> for NativeError {
    fn from(error: RendererError) -> Self {
        match error {
            RendererError::Render(message) => Self::Render(message),
        }
    }
}

impl From<VulkanError> for NativeError {
    fn from(error: VulkanError) -> Self {
        match error {
            VulkanError::NoAdapter => Self::NoAdapter,
            other => Self::Device(other.to_string()),
        }
    }
}

/// The core this thread is running on, where the OS can say.
fn current_core() -> Option<u16> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        // SAFETY: takes no arguments; a negative return is an error.
        u16::try_from(unsafe { libc::sched_getcpu() }).ok()
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        None
    }
}

/// How a window's frames reach the screen. See the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentPath {
    /// CPU pixels straight to the OS compositor, no GPU driver loaded.
    Cpu,
    /// A Vulkan swapchain, which the GPU rasterizer can draw into.
    Vulkan,
}

impl PresentPath {
    /// The path a window should take: Vulkan when it wants the GPU, CPU
    /// otherwise — with `VIEWW_PRESENT=vulkan` or `=cpu` overriding either way,
    /// for comparing the two on one machine.
    #[must_use]
    pub fn choose(prefer_gpu: bool) -> Self {
        match std::env::var("VIEWW_PRESENT").ok().as_deref() {
            Some("vulkan") => Self::Vulkan,
            Some("cpu") => Self::Cpu,
            _ if prefer_gpu => Self::Vulkan,
            _ => Self::Cpu,
        }
    }
}

/// A window's presentable surface: a Vulkan swapchain, or a CPU buffer the OS
/// composites. See [`PresentPath`].
#[derive(Debug)]
pub struct NativeSurface {
    kind: SurfaceKind,
}

enum SurfaceKind {
    /// See [`vieww_hal::vulkan::VulkanSwapchain`] for the Vulkan object this
    /// owns.
    Vulkan(VulkanSwapchain),
    Cpu(CpuPresenter),
}

impl fmt::Debug for SurfaceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vulkan(swapchain) => f.debug_tuple("Vulkan").field(swapchain).finish(),
            Self::Cpu(cpu) => f.debug_tuple("Cpu").field(&cpu.extent).finish(),
        }
    }
}

impl NativeSurface {
    /// Destroy the swapchain and its `VkSurfaceKHR` while `renderer`'s device
    /// and the window are both still alive. Must run before either is dropped;
    /// see `VulkanSwapchain::release` for the crash that ordering caused.
    /// Idempotent. The surface cannot present afterwards. A CPU surface holds
    /// no driver objects and has nothing to release early.
    pub fn release(&mut self, renderer: &NativeRenderer) {
        if let (SurfaceKind::Vulkan(swapchain), Some(device)) = (&mut self.kind, &renderer.device) {
            swapchain.release(device);
        }
    }

    /// Which path this surface presents through.
    #[must_use]
    pub const fn path(&self) -> PresentPath {
        match self.kind {
            SurfaceKind::Vulkan(_) => PresentPath::Vulkan,
            SurfaceKind::Cpu(_) => PresentPath::Cpu,
        }
    }

    /// The surface's size in physical pixels.
    #[must_use]
    pub fn extent(&self) -> (u32, u32) {
        match &self.kind {
            SurfaceKind::Vulkan(swapchain) => swapchain.extent(),
            SurfaceKind::Cpu(cpu) => cpu.extent,
        }
    }
}

/// Which pixels of the CPU rasterizer's output one frame changed, in physical
/// pixels: integer, half-open `(left, top, right, bottom)` — exactly the
/// bounds `render_retained_in_place` writes (it floors and ceils each damage
/// region the same way).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Changed {
    All,
    Regions(Vec<(u32, u32, u32, u32)>),
}

impl Changed {
    fn from_damage(damage: &Damage, (width, height): (u32, u32)) -> Self {
        if damage.is_everything() {
            return Self::All;
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "a surface extent is a small number of pixels"
        )]
        let full = Rect::new(0.0, 0.0, width as f32, height as f32);
        let mut regions = Vec::new();
        for region in damage.regions() {
            let region = region.intersect(full);
            if region.is_empty() {
                continue;
            }
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "clamped to the surface above"
            )]
            let bounds = (
                region.left.floor().max(0.0) as u32,
                region.top.floor().max(0.0) as u32,
                (region.right.ceil() as u32).min(width),
                (region.bottom.ceil() as u32).min(height),
            );
            if bounds.0 < bounds.2 && bounds.1 < bounds.3 {
                regions.push(bounds);
            }
        }
        Self::Regions(regions)
    }
}

/// A CPU frame's way to the screen without a GPU: `softbuffer`, everywhere
/// but Android (see `native_android.rs` for that one).
///
/// # Converting only what changed
///
/// The rasterizer's output is straight-alpha RGBA8 bytes; the OS wants
/// `0x00RRGGBB` words. Converting a whole 1080x2340 frame is ~10 MB of
/// traffic, so the conversion follows damage — but a compositor hands back
/// one of several buffers, each holding whatever frame it last showed.
/// `softbuffer`'s [`age`](softbuffer::Buffer::age) says which: 1 is the
/// previous frame, 2 the one before, 0 unknown. So this keeps the last few
/// frames' changed regions and converts the union of the ones the buffer
/// missed; age 0, or older than the history, converts everything. Platforms
/// that do not track age (Android, macOS) always report 0 and always convert
/// in full, which is still one pass with no GPU upload.
#[cfg(not(target_os = "android"))]
struct CpuPresenter {
    // Field order is drop order: the surface before the context it came from.
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    _context: softbuffer::Context<Arc<Window>>,
    extent: (u32, u32),
    /// Changed regions of recent presented frames, newest first.
    history: VecDeque<Changed>,
}

/// How many frames of change history a buffer age can use. Triple buffering
/// is the deepest swap any of `softbuffer`'s backends does.
#[cfg(not(target_os = "android"))]
const CPU_HISTORY: usize = 3;

#[cfg(target_os = "android")]
#[path = "native_android.rs"]
mod android;
#[cfg(target_os = "android")]
use android::CpuPresenter;

#[cfg(not(target_os = "android"))]
impl CpuPresenter {
    fn new(window: &Arc<Window>, width: u32, height: u32) -> Result<Self, NativeError> {
        let context = softbuffer::Context::new(Arc::clone(window))
            .map_err(|error| NativeError::Device(format!("CPU present: {error}")))?;
        let surface = softbuffer::Surface::new(&context, Arc::clone(window))
            .map_err(|error| NativeError::Device(format!("CPU present: {error}")))?;
        let mut presenter = Self {
            surface,
            _context: context,
            extent: (0, 0),
            history: VecDeque::with_capacity(CPU_HISTORY),
        };
        presenter.resize(width, height)?;
        Ok(presenter)
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), NativeError> {
        let (Some(w), Some(h)) = (NonZeroU32::new(width), NonZeroU32::new(height)) else {
            return Ok(());
        };
        if self.extent == (width, height) {
            return Ok(());
        }
        self.surface
            .resize(w, h)
            .map_err(|error| NativeError::Present(error.to_string()))?;
        self.extent = (width, height);
        self.history.clear();
        Ok(())
    }

    /// Put `frame` (straight RGBA8, `extent`) on screen, converting at least
    /// `changed` and whatever the buffer handed back is missing. Returns how
    /// long getting a buffer took (`FrameStages::acquire`).
    fn present(
        &mut self,
        frame: &[u8],
        extent: (u32, u32),
        changed: Changed,
    ) -> Result<Duration, NativeError> {
        self.resize(extent.0, extent.1)?;
        let (width, height) = self.extent;
        let acquiring = Instant::now();
        let mut buffer = self
            .surface
            .buffer_mut()
            .map_err(|error| NativeError::Present(error.to_string()))?;
        let acquired = acquiring.elapsed();
        if buffer.len() != frame.len() / 4 {
            // The surface and the frame disagree about the size (a resize the
            // rasterizer has not caught up with). The next frame repaints.
            return Ok(acquired);
        }
        let plan = conversion_plan(usize::from(buffer.age()), &self.history, &changed);
        match &plan {
            None => convert_rows(frame, &mut buffer, width, (0, 0, width, height)),
            Some(regions) => {
                for &region in regions {
                    convert_rows(frame, &mut buffer, width, region);
                }
            }
        }
        let full = plan.is_none();
        let presented = match &changed {
            Changed::Regions(regions) if !full => {
                let rects: Vec<softbuffer::Rect> = regions
                    .iter()
                    .filter_map(|&(l, t, r, b)| {
                        Some(softbuffer::Rect {
                            x: l,
                            y: t,
                            width: NonZeroU32::new(r - l)?,
                            height: NonZeroU32::new(b - t)?,
                        })
                    })
                    .collect();
                buffer.present_with_damage(&rects)
            }
            _ => buffer.present(),
        };
        presented.map_err(|error| NativeError::Present(error.to_string()))?;
        self.history.push_front(changed);
        self.history.truncate(CPU_HISTORY);
        Ok(acquired)
    }
}

/// Which window buffer holds which frame, for a platform that hands buffers
/// back without saying how old they are (Android's `ANativeWindow_lock`).
///
/// A buffer is known by its address. Each present records which frame it
/// now holds; when the same address comes round again, the regions changed
/// in every frame since are what it is missing. An address not seen before,
/// a history too short to cover the gap, or a full repaint in it, means
/// "copy everything" — so if a platform maps a buffer at a new address each
/// time, this degrades to a full copy per frame and is never wrong.
#[derive(Debug, Default)]
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
struct BufferAges {
    /// The number of the frame being presented now.
    frame: u64,
    /// Buffer address -> the frame it last showed.
    holds: Vec<(usize, u64)>,
    /// Changes of recent frames, newest first: (frame number, change).
    history: VecDeque<(u64, Changed)>,
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
impl BufferAges {
    /// Frames of history kept: more than any buffer queue rotates through.
    const DEPTH: usize = 6;

    /// Forget everything — after a resize, when every buffer is new.
    fn reset(&mut self) {
        self.holds.clear();
        self.history.clear();
    }

    /// What buffer `address` must have copied into it for this frame, whose
    /// own change is `changed`; `None` means everything. Records the answer:
    /// after this call the buffer holds this frame.
    fn plan(&mut self, address: usize, changed: Changed) -> Option<Vec<(u32, u32, u32, u32)>> {
        self.frame += 1;
        let now = self.frame;
        let held = self
            .holds
            .iter()
            .find(|(a, _)| *a == address)
            .map(|&(_, f)| f);
        let mut plan = Some(Vec::new());
        match held {
            Some(last) if now - last - 1 <= self.history.len() as u64 => {
                for change in std::iter::once((now, &changed))
                    .chain(self.history.iter().map(|(f, c)| (*f, c)))
                    .take_while(|(f, _)| *f > last)
                    .map(|(_, c)| c)
                {
                    match (change, plan.as_mut()) {
                        (Changed::All, _) => plan = None,
                        (Changed::Regions(r), Some(p)) => p.extend_from_slice(r),
                        (Changed::Regions(_), None) => {}
                    }
                }
            }
            _ => plan = None,
        }
        match self.holds.iter_mut().find(|(a, _)| *a == address) {
            Some(slot) => slot.1 = now,
            None => {
                self.holds.push((address, now));
                if self.holds.len() > Self::DEPTH {
                    self.holds.remove(0);
                }
            }
        }
        self.history.push_front((now, changed));
        self.history.truncate(Self::DEPTH);
        plan
    }
}

/// Which regions a CPU present must convert into a buffer that last showed
/// the frame `age` frames ago (`softbuffer`'s convention: 1 is the previous
/// frame, 0 unknown), given the changes of recent frames (`history`, newest
/// first) and this frame's. `None` means everything.
#[cfg_attr(target_os = "android", allow(dead_code))]
fn conversion_plan(
    age: usize,
    history: &VecDeque<Changed>,
    changed: &Changed,
) -> Option<Vec<(u32, u32, u32, u32)>> {
    // The buffer holds frame N-age; frames N-age+1 .. N-1 (the newest
    // `age - 1` entries of the history) and this one changed it since.
    let missed = age.checked_sub(1)?;
    if missed > history.len() {
        return None;
    }
    let mut regions = Vec::new();
    for change in std::iter::once(changed).chain(history.iter().take(missed)) {
        match change {
            Changed::All => return None,
            Changed::Regions(r) => regions.extend_from_slice(r),
        }
    }
    Some(regions)
}

/// Straight RGBA8 bytes to `0x00RRGGBB` words, for the rows and columns of
/// `region` (half-open). Alpha is dropped: the window is opaque, exactly as
/// the Vulkan path's opaque composite ignores it.
#[cfg_attr(target_os = "android", allow(dead_code))]
fn convert_rows(rgba: &[u8], out: &mut [u32], width: u32, region: (u32, u32, u32, u32)) {
    let (left, top, right, bottom) = region;
    let width = width as usize;
    for y in top as usize..bottom as usize {
        let row = y * width;
        let src = &rgba[(row + left as usize) * 4..(row + right as usize) * 4];
        let dst = &mut out[row + left as usize..row + right as usize];
        for (px, d) in src.as_chunks::<4>().0.iter().zip(dst) {
            *d = (u32::from(px[0]) << 16) | (u32::from(px[1]) << 8) | u32::from(px[2]);
        }
    }
}

thread_local! {
    /// The one `VkInstance`/`VkDevice` this process opens, kept alive for as
    /// long as any window holds it — and **only** that long.
    ///
    /// # Why `Weak`
    ///
    /// This was an `Rc`, which kept the device alive for as long as the
    /// *thread* lived rather than as long as a window held it. Every window's
    /// renderer let go when it closed, and the device was finally destroyed
    /// by this thread-local's destructor — at process exit. On Windows that
    /// runs during the loader's process-detach phase, and NVIDIA's driver
    /// faults there: `tests/wait_loop.rs`' `self_close` and `gpu_frame` exited
    /// with `0xc0000005` *after* passing, on a GeForce 920MX (driver
    /// 32.0.15.7652, 2026-10-15), on the CPU path as well as the GPU one, and
    /// passed with `VIEWW_KEEP_VULKAN=1` — which leaks the device, so nothing
    /// was ever destroyed at exit. A `Weak` here destroys the device when the
    /// last window that uses it closes, inside the event loop, where the
    /// driver is fully alive. Sharing between windows is unchanged: while any
    /// window holds the device, this upgrades to it.
    static SHARED_DEVICE: RefCell<Weak<VulkanDevice>> = const { RefCell::new(Weak::new()) };
}

/// The shared device, and a swapchain for this window on it.
///
/// **One driver per process, not one per window.** Opening a `VkInstance` and
/// a `VkDevice` per window meant that closing any window destroyed a driver
/// out from under the windows that were still open, and the next window's
/// `vkDestroySwapchainKHR` crashed inside it — a null jump on NVIDIA, a double
/// free on Mesa's Intel driver, both reproduced by the desktop suite the
/// moment it closed a second window. See
/// [`VulkanDevice::swapchain_for_window`] for the measurements.
///
/// The event loop is single-threaded, so the cache is thread-local rather than
/// a lock, and it is only ever read on the thread that opens windows.
///
/// If the existing adapter cannot present to the new window — a second GPU
/// driving a second monitor is the case — this opens a device for that window
/// alone rather than failing, which is the old behaviour for the one situation
/// that needed it.
fn shared_device_for(
    window: &(impl raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle),
    width: u32,
    height: u32,
) -> Result<(Rc<VulkanDevice>, VulkanSwapchain), NativeError> {
    let existing = SHARED_DEVICE.with(|cell| cell.borrow().upgrade());
    if let Some(device) = existing {
        match device.swapchain_for_window(window, width, height) {
            Ok(swapchain) => return Ok((device, swapchain)),
            // Not this adapter's window. Fall through to a device of its own.
            Err(VulkanError::NoAdapter) => {}
            Err(error) => return Err(error.into()),
        }
        let (device, swapchain) = VulkanDevice::for_window(window, width, height)?;
        return Ok((Rc::new(device), swapchain));
    }

    let (device, swapchain) = VulkanDevice::for_window(window, width, height)?;
    let device = Rc::new(device);
    SHARED_DEVICE.with(|cell| *cell.borrow_mut() = Rc::downgrade(&device));
    Ok((device, swapchain))
}

/// The GPU rasterizer half of a windowed renderer: the planner that turns a
/// scene into a batched plan, and the renderer that executes it on the same
/// [`VulkanDevice`] the swapchain presents through.
///
/// Created lazily by [`NativeRenderer::present_planned`], once, on the first
/// frame that asks for the GPU path — a window that never does never pays
/// for the pipelines. A creation failure is remembered by
/// `gpu_unavailable` on the renderer rather than retried per frame: a
/// pipeline that could not build once will not build better forty times a
/// second, and retrying would turn every present into a failed build.
struct GpuRaster {
    planner: GpuPlanner,
    renderer: SceneRenderer,
}

/// Where one presented frame's time went, stage by stage.
///
/// # Why
///
/// The audit's first GPU finding was a single number — a 21 ms median frame on
/// an Adreno 612 — and its first ask was to break that number down. A frame on
/// this path is up to six different kinds of work, each with a different fix:
/// lifting the scene to device resolution, planning it for the GPU,
/// rasterising it on the CPU, the GPU's own upload/record/submit/execute/
/// readback (see [`GpuFrameTimings`]), acquiring a swapchain image, and
/// presenting. Every frame now records all of them, for the price of a few
/// `Instant::now()` calls and no allocation.
///
/// A stage the frame did not use is zero. The stages are sequential, so
/// [`total`](Self::total) is the frame's whole cost on this thread.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameStages {
    /// `Scene::scaled`, lifting an off-1x frame to device pixels.
    pub scale: Duration,
    /// `vieww_gpu::Planner::plan` — only [`NativeRenderer::present_planned`]
    /// plans.
    pub plan: Duration,
    /// The CPU rasterizer, on a CPU frame.
    pub cpu_raster: Duration,
    /// The GPU rasterizer, on a GPU frame, by stage. Its `wait` on the direct
    /// path includes waiting for the swapchain image to become free, because
    /// the frame's final blit waits on that semaphore.
    pub gpu: GpuFrameTimings,
    /// The GPU's own time for a GPU frame, by kind of work, from timestamp
    /// queries (see [`GpuPassTimes`]). `None` on a CPU frame or where the
    /// queue records no timestamps. **Not a stage of this thread**: it is the
    /// GPU's clock, overlapping `gpu.wait`, so [`total`](Self::total) leaves
    /// it out.
    pub gpu_passes: Option<GpuPassTimes>,
    /// Acquiring a swapchain image to render straight into (direct path).
    pub acquire: Duration,
    /// Handing the frame to the swapchain: `present_pixels` (staging upload,
    /// copy and present) for CPU and readback frames, `present_acquired` on
    /// the direct path.
    pub present: Duration,
    /// The CPU core the thread was on when a CPU raster finished
    /// (`sched_getcpu`, Linux and Android only), `None` on a GPU frame or
    /// elsewhere. Not a stage: it is here so the frame log can say which
    /// core a slow raster ran on — the LG Stylo 5 drew the same frame in
    /// 31 ms presented to the window and 21 ms through Vulkan (2026-10-17),
    /// and a core or its clock is the remaining suspect.
    pub cpu_core: Option<u16>,
    /// The CPU raster by kind of command — text, shadows, layer composites
    /// and the rest ([`RasterKindTimes`]) — on a CPU frame, when
    /// `VIEWW_RASTER_PROFILE=1` asked for it. Not a stage: it divides
    /// [`cpu_raster`](Self::cpu_raster) rather than adding to it.
    pub cpu_kinds: Option<RasterKindTimes>,
}

impl FrameStages {
    /// Every stage, summed.
    #[must_use]
    pub fn total(&self) -> Duration {
        self.scale + self.plan + self.cpu_raster + self.gpu.total() + self.acquire + self.present
    }
}

/// Shadow mode for the hybrid planner on a live window — see
/// [`NativeRenderer::enable_planner_shadow`].
struct PlannerShadow {
    planner: ShadowPlanner,
    out: std::io::BufWriter<std::fs::File>,
    /// Set after the first failed write, so a full disk is reported once and
    /// then shadow mode quietly stops rather than failing every frame.
    broken: bool,
    /// `VIEWW_PLANNER_AUTHORITATIVE=1`: the mode machine inside
    /// [`ShadowPlanner`] may promote past shadow and, once its gate
    /// clears, hand whole-frame executor decisions back. The shadow log
    /// stays the observation record either way — the gate is fed by the
    /// same frames it.
    authoritative: bool,
    /// The plan computed before this frame drew (`plan_for_frame`), so
    /// [`Self::record_shadow`] observes the prediction the decision was
    /// actually made on instead of planning the frame a second time.
    /// `None` in plain shadow mode, where the plan is computed at record
    /// time exactly as it always was.
    pending: Option<(Option<Executor>, vieww_render_planner::ShadowPlan)>,
}

/// Presents [`vieww_paint::Scene`]s rasterised by `vieww-paint`'s `native`
/// backend, through `vieww-hal`'s Vulkan swapchain. See the module docs for
/// what this does and does not cover yet.
pub struct NativeRenderer {
    /// The Vulkan device, on [`PresentPath::Vulkan`]; `None` on the CPU path,
    /// which loads no GPU driver.
    device: Option<Rc<VulkanDevice>>,
    cpu: CpuRenderer,
    /// The GPU rasterizer, built on first use — see [`GpuRaster`].
    gpu: Option<GpuRaster>,
    /// Set when [`GpuRaster`]'s creation failed, so the attempt is made once
    /// and never again.
    gpu_unavailable: bool,
    /// The last presented frame's stage timings — see [`FrameStages`].
    stages: FrameStages,
    /// The hybrid planner in shadow mode, when enabled.
    shadow: Option<PlannerShadow>,
    /// The resolution scale of the GPU rasterizer's last frame, when that
    /// frame is what is on screen — the condition for the next GPU frame at
    /// the same scale redrawing only its damage. Any CPU frame or failure
    /// clears it: the GPU's kept frame no longer matches what the damage is
    /// measured against. (A change of scale is a new target size, which the
    /// renderer would refuse to keep anyway.)
    gpu_frame_current: Option<f32>,
}

/// The GPU rasterizer goes before the device it was built on.
///
/// Fields drop in declaration order, and `device` is first — so without this,
/// the last window to close would destroy the `VkDevice` and *then* run
/// `SceneRenderer`'s `Drop`, which waits on and destroys its targets through
/// that device. While `SHARED_DEVICE` held a strong reference the device always
/// outlived every renderer and the order never mattered; since it holds a
/// `Weak`, a renderer can hold the last reference, and the order is the
/// difference between a clean teardown and a use-after-destroy.
impl Drop for NativeRenderer {
    fn drop(&mut self) {
        self.gpu = None;
    }
}

impl fmt::Debug for NativeRenderer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeRenderer")
            .field("cached_fonts", &self.cpu.cached_fonts())
            .finish_non_exhaustive()
    }
}

/// The outcome of a [`NativeRenderer::present_planned`] frame.
#[derive(Debug, Clone, Copy)]
pub struct PlannedFrame {
    /// The CPU rasterizer's report. Zeroed on GPU frames — see
    /// [`present_planned`](NativeRenderer::present_planned) for why zeroes
    /// are the honest value rather than a partial one.
    pub report: SceneReport,
    /// Whether the GPU rasterizer drew this frame. `false` means the planner
    /// had gaps (or the pipelines could not be built), and the CPU drew it.
    pub drawn_on_gpu: bool,
    /// Whether the GPU frame went straight into the swapchain image, with no
    /// readback through host memory. `false` for a CPU frame and for a GPU
    /// frame that took the readback path (a translucent window, or a driver
    /// without the blit support `VulkanDevice::can_blit_scene_frames` asks
    /// for).
    pub presented_directly: bool,
    /// Where this frame's time went. See [`FrameStages`].
    pub stages: FrameStages,
    /// Device pixels the GPU redrew: the frame's damage when it could keep
    /// the rest of its previous frame, the whole render extent when it could
    /// not. `None` for a CPU frame.
    pub gpu_redrawn: Option<u64>,
}

/// The scene to rasterise at the swapchain's extent: the frame itself when
/// logical and physical sizes agree, the frame lifted into physical pixels
/// through [`Scene::scaled`] when they do not — the same decision, for the
/// same reason, as [`present_damaged`](NativeRenderer::present_damaged)
/// makes (glyph outlines and hairlines scan-converted at the device's own
/// resolution, never magnified).
fn frame_at_device_resolution<'a>(
    scene: &'a Scene,
    logical_size: (u32, u32),
    physical: (u32, u32),
    scale: Scale,
) -> std::borrow::Cow<'a, Scene> {
    if logical_size == physical {
        std::borrow::Cow::Borrowed(scene)
    } else {
        std::borrow::Cow::Owned(scene.scaled(scale.factor()))
    }
}

impl NativeRenderer {
    /// Bytes of an earlier launch's pipeline cache the GPU device started
    /// with (`None` on the CPU present, which has no device).
    #[must_use]
    pub fn pipeline_cache_seed(&self) -> Option<usize> {
        self.device
            .as_ref()
            .map(|device| device.pipeline_cache_seed())
    }

    /// Write the GPU device's pipeline cache to disk — see
    /// `vieww_hal::vulkan::pipeline_cache`. `None` on the CPU present.
    pub fn save_pipeline_cache(&self) -> Option<std::io::Result<usize>> {
        self.device
            .as_ref()
            .map(|device| device.save_pipeline_cache())
    }

    /// Open a window and a renderer to present into it.
    ///
    /// # Errors
    ///
    /// [`NativeError::Device`] if no Vulkan loader is present, or no adapter
    /// supports both a graphics queue and presenting to this window.
    ///
    /// # Panics
    ///
    /// If `width` or `height` is zero.
    /// The name of the GPU this renderer presents through, as the driver
    /// reports it — for the startup trace and for logs, where "which of a
    /// hybrid laptop's GPUs did it pick" is the first question.
    #[must_use]
    pub fn adapter_name(&self) -> String {
        use vieww_hal::Device as _;
        self.device.as_ref().map_or_else(
            || "none (CPU present: no GPU driver loaded)".to_owned(),
            |device| device.info().name,
        )
    }

    pub fn for_window(
        window: &(impl raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle),
        width: u32,
        height: u32,
    ) -> Result<(Self, NativeSurface), NativeError> {
        Self::for_window_with(window, width, height, CpuRenderer::new())
    }

    /// The same, presenting through a rasterizer the caller configured.
    ///
    /// # Why this exists
    ///
    /// [`for_window`](Self::for_window) constructed `CpuRenderer::new()`, and
    /// that was the only rasterizer a windowed application could ever have. The
    /// consequence was not theoretical: `vieww_paint::native::NativeRenderer`
    /// offers `with_color_pipeline` — the linear-light compositing path, built,
    /// tested and documented at length in `native/linear.rs` as the
    /// colorimetrically correct way to blend a gradient or a translucent
    /// overlay — and `with_glyph_outline_budget_bytes`, for a
    /// memory-constrained target. **Neither could be reached from a real
    /// window.** They were configurable in a unit test and fixed in every
    /// application, which is a worse position than not having them.
    ///
    /// So the rasterizer is a parameter. `for_window` stays as the short form
    /// with the default, which is what almost every caller wants; this is the
    /// one that makes the crate's own options usable.
    ///
    /// # Errors
    ///
    /// As [`for_window`](Self::for_window).
    ///
    /// # Panics
    ///
    /// If `width` or `height` is zero.
    pub fn for_window_with(
        window: &(impl raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle),
        width: u32,
        height: u32,
        cpu: CpuRenderer,
    ) -> Result<(Self, NativeSurface), NativeError> {
        let (device, swapchain) = shared_device_for(window, width, height)?;
        Ok((
            Self::with_device(Some(device), cpu),
            NativeSurface {
                kind: SurfaceKind::Vulkan(swapchain),
            },
        ))
    }

    /// Open a window's renderer on `path`: [`PresentPath::Cpu`] presents
    /// without a GPU driver, falling back to Vulkan if the platform's CPU
    /// present cannot open (the error is returned only if both fail).
    ///
    /// # Errors
    ///
    /// As [`for_window`](Self::for_window).
    ///
    /// # Panics
    ///
    /// If `width` or `height` is zero.
    pub fn for_window_on(
        window: &Arc<Window>,
        width: u32,
        height: u32,
        path: PresentPath,
    ) -> Result<(Self, NativeSurface), NativeError> {
        if path == PresentPath::Cpu {
            match CpuPresenter::new(window, width, height) {
                Ok(cpu) => {
                    return Ok((
                        Self::with_device(None, CpuRenderer::new()),
                        NativeSurface {
                            kind: SurfaceKind::Cpu(cpu),
                        },
                    ));
                }
                Err(error) => {
                    eprintln!("vieww: {error}; presenting through Vulkan instead");
                }
            }
        }
        Self::for_window(window.as_ref(), width, height)
    }

    fn with_device(device: Option<Rc<VulkanDevice>>, mut cpu: CpuRenderer) -> Self {
        // Time the CPU raster by kind of command (`FrameStages::cpu_kinds`)
        // when asked: two clock reads a command, so off unless measuring.
        if std::env::var("VIEWW_RASTER_PROFILE").is_ok_and(|v| v == "1") {
            cpu.profile_kinds(true);
        }
        Self {
            device,
            cpu,
            gpu: None,
            gpu_unavailable: false,
            stages: FrameStages::default(),
            shadow: None,
            gpu_frame_current: None,
        }
    }

    /// Cached glyph outlines — forwarded from the CPU rasterizer.
    #[must_use]
    pub fn cached_fonts(&self) -> usize {
        self.cpu.cached_fonts()
    }

    /// Where the last presented frame's time went, from either present path.
    #[must_use]
    pub const fn last_stages(&self) -> FrameStages {
        self.stages
    }

    /// Run the hybrid planner in **shadow mode** beside every
    /// [`present_planned`](Self::present_planned) frame, appending one JSON
    /// line per frame to `path`.
    ///
    /// Each line is a `vieww_render_planner::ShadowFrame`: the per-pass
    /// placement the planner would have chosen, its prediction, which
    /// rasterizer actually drew the frame, how long that took
    /// ([`FrameStages::total`]), and why the live path took that rasterizer
    /// when it was not a free choice. Nothing about what is drawn changes.
    /// This is the evidence an authoritative hybrid rollout needs — see that
    /// module's docs.
    ///
    /// The planner plans as this device: a discrete GPU as
    /// `DeviceProfile::DESKTOP_DISCRETE` (an integrated one with its `gpu`
    /// kind set to match), a mobile target as `MOBILE_TILE_BASED`, a CPU
    /// "GPU" (lavapipe, SwiftShader) as `CPU_ONLY_FALLBACK`.
    ///
    /// Only `present_planned` frames are recorded: they are whole-frame
    /// repaints, which is what the planner predicts. A
    /// [`present_damaged`](Self::present_damaged) frame repaints a damaged
    /// region, and comparing it to a whole-frame prediction would teach the
    /// calibration that the CPU is ten times faster than it is.
    ///
    /// **Not free:** planning builds two graphs and allocates per frame, so
    /// leave this off when measuring allocations or frame time for a budget.
    /// Applications enable it with `VIEWW_PLANNER_SHADOW=<file>` (see
    /// `app.rs`).
    ///
    /// # Errors
    ///
    /// The file could not be opened for appending.
    pub fn enable_planner_shadow(&mut self, path: &Path) -> std::io::Result<()> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        self.shadow = Some(PlannerShadow {
            planner: ShadowPlanner::new(self.planner_profile()),
            out: std::io::BufWriter::new(file),
            broken: false,
            authoritative: false,
            pending: None,
        });
        Ok(())
    }

    /// Let the hybrid planner's mode machine run to completion: shadow,
    /// then shadow on the calibrated profile, then — if its gate (the
    /// advisory's "RMSE < 5% over 1000+ frames") clears — whole-frame
    /// executor decisions on frames the planner recommends the CPU for.
    /// The GPU side needs no actuation: the live path already prefers the
    /// GPU whenever `ScenePlan::is_complete` passes, and a `Mixed`
    /// recommendation stays unactionable until a backend can execute a
    /// split frame.
    ///
    /// Requires [`Self::enable_planner_shadow`] first — the gate is fed by
    /// the shadow log's own observations, and an authoritative planner
    /// with no observation loop would be a decision with no evidence
    /// behind it.
    ///
    /// # Errors
    ///
    /// Shadow mode was never enabled, with the reason as the message.
    pub fn enable_planner_authoritative(&mut self) -> Result<(), String> {
        if self.shadow.is_none() {
            return Err(
                "planner shadow is not enabled; set VIEWW_PLANNER_SHADOW to a log path \
                 so the authoritative gate has something to read"
                    .to_owned(),
            );
        }
        // The profile is read before the mutable borrow of the shadow
        // state, which `planner_profile`'s `&self` would collide with.
        let profile = self.planner_profile();
        let shadow = self.shadow.as_mut().expect("checked above");
        shadow.authoritative = true;
        shadow.planner = ShadowPlanner::with_authoritative(profile);
        Ok(())
    }

    /// The planner's assumed profile for this window's device.
    ///
    /// `VIEWW_PLANNER_CALIBRATE=1` (read once per process) additionally runs
    /// [`vieww_render_planner::probe::calibrate_cpu_fill`]: a one-time,
    /// single-digit-millisecond startup micro-benchmark that replaces the
    /// profile's round-number CPU fill throughput with a measurement of this
    /// machine's own rasterizer, and fills `cpu_cores` from the scheduler.
    /// The GPU fields stay as `profile_for` guessed them — the shadow log's
    /// own `Calibration` is the measured path for those, and this function
    /// is called once per window, not per frame.
    fn planner_profile(&self) -> DeviceProfile {
        use vieww_hal::Device as _;
        // On the CPU present path no device was opened to ask; plan as this
        // kind of machine's usual GPU, which is what the log is for.
        let mut profile = self.device.as_ref().map_or_else(
            || profile_for(""),
            |device| profile_for(&device.info().device_type),
        );
        if planner_calibration() {
            profile = vieww_render_planner::probe::calibrate_cpu_fill(profile);
        }
        profile
    }

    /// Plan `scene` in shadow and log what actually happened to it.
    fn record_shadow(
        &mut self,
        scene: &Scene,
        extent: (u32, u32),
        executor: Executor,
        fallback: Option<String>,
    ) {
        let Some(shadow) = self.shadow.as_mut().filter(|s| !s.broken) else {
            return;
        };
        // A pending plan (authoritative mode planned this frame *before*
        // it drew, to decide who drew it) is the prediction to record —
        // replanning would measure the model against a different answer
        // than the one it acted on.
        let plan = match shadow.pending.take() {
            Some((_, plan)) => plan,
            None => match shadow.planner.plan(scene, extent.0, extent.1) {
                Some(plan) => plan,
                None => return,
            },
        };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a frame time in ms fits an f32"
        )]
        let actual_ms = (self.stages.total().as_secs_f64() * 1000.0) as f32;
        let frame = shadow
            .planner
            .observe(plan, "window", extent, executor, actual_ms, fallback);
        if let Err(error) =
            writeln!(shadow.out, "{}", frame.to_json_line()).and_then(|()| shadow.out.flush())
        {
            eprintln!("vieww: planner shadow log stopped: {error}");
            shadow.broken = true;
        }
    }

    /// Rasterise `scene` on the CPU and present it to `surface`. Always a
    /// full repaint (see the module docs on why there is no damage path
    /// yet).
    ///
    /// `scene`'s commands are recorded in *logical* pixels, against
    /// `logical_size`; `surface` is measured in *physical* ones (see
    /// [`crate::Scale`]). At 1:1 the two match and this rasterises straight
    /// into the swapchain's own resolution. Off 1:1 the frame — scene and
    /// damage alike — is lifted into physical pixels through
    /// [`Scene::scaled`](vieww_paint::Scene::scaled) before rasterising, so
    /// glyph outlines and hairlines are scan-converted at device resolution
    /// and never magnified. That is the same door `film_lab`'s `SCALE_FACTOR`
    /// receipts render 4K through, and it replaced the old behaviour —
    /// rasterise at `logical_size`, then nearest-neighbour-upscale — whose
    /// cost was not only the soft edges a HiDPI display made visible but a
    /// full-frame buffer walk and allocation per present.
    ///
    /// # Errors
    ///
    /// [`NativeError::Render`] if rasterising failed, or
    /// [`NativeError::Present`] if the swapchain image could not be acquired
    /// or presented, even after one rebuild attempt.
    pub fn present(
        &mut self,
        surface: &mut NativeSurface,
        scene: &Scene,
        base: Color,
        logical_size: (u32, u32),
        scale: Scale,
    ) -> Result<SceneReport, NativeError> {
        self.present_damaged(surface, scene, None, base, logical_size, scale)
    }

    /// [`present`](Self::present), repainting only what `damage` says changed.
    ///
    /// # What this changes, and what it deliberately does not
    ///
    /// The **rasterisation** becomes incremental: the CPU renderer keeps the
    /// pixels it produced last frame and clears, redraws and converts only
    /// the damaged regions — see
    /// `vieww_paint::native::NativeRenderer::render_retained`, which also
    /// documents why the regions are a write mask rather than a smaller
    /// surface. Measured on the Studio shell at 1440x900: a full frame is
    /// 123 ms, the same frame through `render_damaged` (which culls commands
    /// but still rebuilds the whole framebuffer) is 13 ms, and through the
    /// retained path with nothing changed it is **0.5 ms**.
    ///
    /// The **upload** stays whole. A swapchain hands out a different image
    /// each frame, so the pixels this renderer kept are not the pixels
    /// already on the image being presented into; copying the full buffer is
    /// correct and costs a memcpy of a few megabytes. Narrowing that to the
    /// damaged rectangles needs `VK_KHR_incremental_present` and a per-image
    /// record of what each one already holds — a real follow-up, and a much
    /// smaller one than this, because the expensive half is now gone.
    ///
    /// Passing `None` repaints in full, which is what a resize and a first
    /// frame want.
    ///
    /// # Errors
    ///
    /// As [`present`](Self::present).
    pub fn present_damaged(
        &mut self,
        surface: &mut NativeSurface,
        scene: &Scene,
        damage: Option<&Damage>,
        base: Color,
        logical_size: (u32, u32),
        scale: Scale,
    ) -> Result<SceneReport, NativeError> {
        let (physical_width, physical_height) = surface.extent();
        let (logical_width, logical_height) = logical_size;
        self.stages = FrameStages::default();
        self.gpu_frame_current = None;
        let rasterising = Instant::now();
        // What this frame changes, for a CPU surface that converts only that.
        // A retained render falls back to a full one when the size changed
        // since the last frame; so does this.
        let resized =
            self.cpu.last_frame().len() != physical_width as usize * physical_height as usize * 4;
        let mut changed = Changed::All;

        // **Rasterise at the device's own resolution whenever the two differ.**
        //
        // The old path rasterised at `logical_size` and nearest-neighbour-
        // upscaled into the swapchain: on a phone — where the ratio is ~3, not
        // the 2 a laptop reports — every glyph was drawn at a third of its
        // linear resolution and then tripled, which reads on screen as text
        // nobody sharpened. `Scene::scaled` lifts the whole frame into
        // physical pixels *after* compositing, where there are no repaint
        // boundaries left to lose the ratio, so glyph outlines and hairlines
        // are scan-converted at device resolution rather than magnified —
        // the film lab's `SCALE_FACTOR` receipts render 4K through exactly
        // this door.
        //
        // The comparison is against the sizes rather than `scale.is_one()`
        // alone: a scale that rounds back to the same extent (a 1.25 window
        // dragged onto a 1x monitor mid-transition, say) has nothing to
        // scale and a borrowed buffer to present, and the truncated
        // `logical_size` a caller derived from `physical / scale` can differ
        // from the swapchain by a rounding pixel — that case scales, and the
        // renderer clips the sub-pixel remainder.
        let report = if (logical_width, logical_height) == (physical_width, physical_height) {
            match damage {
                Some(damage) => {
                    if !resized {
                        changed = Changed::from_damage(damage, (physical_width, physical_height));
                    }
                    self.cpu.render_retained_in_place(
                        scene,
                        damage,
                        logical_width,
                        logical_height,
                        base,
                    )?
                }
                None => self
                    .cpu
                    .render_in_place(scene, logical_width, logical_height, base)?,
            }
        } else {
            let factor = scale.factor();
            let scaled = scene.scaled(factor);
            self.stages.scale = rasterising.elapsed();
            #[expect(
                clippy::cast_precision_loss,
                reason = "a swapchain extent is a small number of pixels"
            )]
            let surface_width = physical_width as f32;
            #[expect(
                clippy::cast_precision_loss,
                reason = "a swapchain extent is a small number of pixels"
            )]
            let surface_height = physical_height as f32;
            let physical_surface = Rect::new(0.0, 0.0, surface_width, surface_height);
            match damage {
                // The damage is logical, against a logical surface — `Physical`
                // rebuilds it against the physical one, scaling each region
                // (and its antialiasing bleed) exactly as it scales the ink.
                Some(logical) => {
                    let physical = Physical::new(scene, logical, scale, physical_surface);
                    if !resized {
                        changed = Changed::from_damage(
                            physical.damage(),
                            (physical_width, physical_height),
                        );
                    }
                    self.cpu.render_retained_in_place(
                        &scaled,
                        physical.damage(),
                        physical_width,
                        physical_height,
                        base,
                    )?
                }
                None => self
                    .cpu
                    .render_in_place(&scaled, physical_width, physical_height, base)?,
            }
        };

        self.stages.cpu_raster = rasterising.elapsed() - self.stages.scale;
        self.stages.cpu_core = current_core();
        self.stages.cpu_kinds = self.cpu.last_kind_times();
        self.present_last_cpu_frame(surface, (physical_width, physical_height), changed)?;
        Ok(report)
    }

    /// Present the CPU rasterizer's retained output, timing it into
    /// [`FrameStages::present`].
    fn present_last_cpu_frame(
        &mut self,
        surface: &mut NativeSurface,
        (physical_width, physical_height): (u32, u32),
        changed: Changed,
    ) -> Result<(), NativeError> {
        // In place: the renderer's retained output is read directly, rather
        // than copied into a fresh `Pixels` every frame — that copy was a
        // full-frame allocation per present, which the Vieww standard's
        // steady-state clause counts. The rasterisation already ran at the
        // swapchain's own extent, so there is nothing left to resample.
        // A GPU frame may still be in flight, holding a wait on the swapchain's
        // acquire semaphore that `present_pixels` is about to signal again —
        // see `SceneRenderer::settle`.
        if let Some(gpu) = &mut self.gpu {
            gpu.renderer
                .settle()
                .map_err(|error| NativeError::Render(error.to_string()))?;
        }
        let frame = self.cpu.last_frame();
        debug_assert_eq!(
            frame.len(),
            physical_width as usize * physical_height as usize * 4,
            "the renderer rasterised at the swapchain's extent"
        );

        let presenting = Instant::now();
        // On a CPU surface, getting a buffer from the OS is timed apart from
        // filling and posting it, into `acquire` — the same split the Vulkan
        // path's direct GPU frames have. Waiting for a free buffer (the
        // compositor's pacing) and copying pixels are different problems,
        // and one `present` number could not tell them apart (Galaxy S20,
        // 2026-10-16: 7.1 ms of "present").
        let mut acquired = Duration::ZERO;
        match (&mut surface.kind, &self.device) {
            (SurfaceKind::Cpu(cpu), _) => {
                acquired = cpu.present(frame, (physical_width, physical_height), changed)?;
            }
            (SurfaceKind::Vulkan(swapchain), Some(device)) => device
                .present_pixels(swapchain, frame)
                .map_err(|error| NativeError::Present(error.to_string()))?,
            (SurfaceKind::Vulkan(_), None) => {
                return Err(NativeError::Present(
                    "a Vulkan surface on a renderer with no device".to_owned(),
                ));
            }
        }
        self.stages.acquire = acquired;
        self.stages.present = presenting.elapsed().saturating_sub(acquired);
        Ok(())
    }

    /// Match the surface to a window that changed size. Contents are gone
    /// after this — the next [`present`](Self::present) repaints in full,
    /// which today it always does anyway (see the module docs).
    ///
    /// # Errors
    ///
    /// [`NativeError::Present`] if the swapchain could not be rebuilt at the
    /// new size (the surface was likely invalidated — e.g. the window
    /// closed mid-resize).
    pub fn resize(
        &self,
        surface: &mut NativeSurface,
        width: u32,
        height: u32,
    ) -> Result<(), NativeError> {
        if width == 0 || height == 0 {
            return Ok(());
        }
        match (&mut surface.kind, &self.device) {
            (SurfaceKind::Cpu(cpu), _) => cpu.resize(width, height),
            (SurfaceKind::Vulkan(swapchain), Some(device)) => swapchain
                .recreate(device, width, height)
                .map_err(|error| NativeError::Present(error.to_string())),
            (SurfaceKind::Vulkan(_), None) => Ok(()),
        }
    }

    /// Rasterise on the GPU when the scene plans complete, on the CPU when it
    /// does not, and present either way through the same swapchain.
    ///
    /// This is the join the worklist called §2.6 and closed on 2026-10-09
    /// (evidence in `TRACKER.md`): `SceneRenderer` could
    /// execute a whole frame as one draw call, and `VulkanSwapchain` could
    /// put pixels on a screen, and nothing connected them — the swapchain
    /// was presenting the CPU rasterizer's buffer, whatever the GPU could
    /// do. Now a frame is planned first ([`GpuPlanner`]), drawn with
    /// [`SceneRenderer`] when the plan is complete, and its pixels take the
    /// same `present_pixels` path the CPU's always took. A frame the planner
    /// cannot express falls back to a full CPU repaint — the plan's gaps and
    /// the fallback are the same agreement [`vieww_gpu::ScenePlan::is_complete`] and
    /// `present_damaged` have always had.
    ///
    /// The GPU path redraws only `damage` when the screen holds its own
    /// previous frame at full resolution — the frame target is kept between
    /// frames (see `SceneRenderer`'s module docs, and `gpu_frame_current`);
    /// otherwise, and for a first frame, a resize or `None`, it repaints in
    /// full. The read-back into host memory it does per frame is a
    /// parity-harness cost a real present would not pay — see
    /// `SceneRenderer::render_planned`'s own docs; the follow-up is rendering
    /// straight into the swapchain image, which needs the scene pipeline to
    /// target `VK_KHR_swapchain` images rather than its own framebuffer.
    ///
    /// Which half drew the frame is returned rather than implied, because
    /// the caller is the one reporting frame stats and pacing, and "the GPU
    /// is doing it" changes what those numbers mean.
    ///
    /// # Errors
    ///
    /// [`NativeError::Device`] if the GPU rasterizer's pipelines could not
    /// be built (once — see `GpuRaster`); after that, as
    /// [`present_damaged`](Self::present_damaged).
    pub fn present_planned(
        &mut self,
        surface: &mut NativeSurface,
        scene: &Scene,
        damage: Option<&Damage>,
        base: Color,
        logical_size: (u32, u32),
        scale: Scale,
    ) -> Result<PlannedFrame, NativeError> {
        let (physical_width, physical_height) = surface.extent();
        let extent = (physical_width, physical_height);
        self.stages = FrameStages::default();
        // Whether the GPU may redraw only this frame's damage is decided
        // from the state *before* this frame; whatever happens below, the
        // frame it leaves on screen sets it again.
        let previous_gpu_scale = self.gpu_frame_current.take();

        // The GPU rasterizer, once. Creation failure is remembered and this
        // becomes a CPU renderer permanently — see `gpu_unavailable`. A CPU
        // surface has no device to build one on, and draws every frame on the
        // CPU.
        let device = match (&surface.kind, &self.device) {
            (SurfaceKind::Vulkan(_), Some(device)) => Some(Rc::clone(device)),
            _ => None,
        };
        if let (None, false, Some(device)) = (&self.gpu, self.gpu_unavailable, &device) {
            match SceneRenderer::new(device) {
                Ok(mut renderer) => {
                    if !gpu_overlap() {
                        renderer
                            .set_overlap(false)
                            .map_err(|error| NativeError::Render(error.to_string()))?;
                    }
                    self.gpu = Some(GpuRaster {
                        planner: GpuPlanner::new(),
                        renderer,
                    });
                }
                Err(error) => {
                    self.gpu_unavailable = true;
                    return Err(NativeError::Device(error.to_string()));
                }
            }
        }

        // **Lifted to device pixels once, for whichever rasterizer draws.**
        // Until 2026-10-15 a CPU fallback frame was lifted here for the
        // planner and then lifted again inside `present_damaged` — two
        // `Scene::scaled` calls per fallback frame on every HiDPI display.
        // Measuring the stages is what made the second one visible.
        //
        // The resolution scale (`VIEWW_GPU_SCALE`, `crate::quality`) applies
        // only to the direct GPU route: it renders the plan into targets a
        // fraction of the swapchain's size and the final blit enlarges, which
        // needs an opaque frame and a format that filters linearly. The lift
        // goes to the *scaled* extent — the plan's geometry lives in the space
        // the targets render — and the scale multiplies the display's own, so
        // text rasters at the reduced density too. That reduced density is
        // the point; the CPU fallback below re-lifts to full size if the plan
        // comes back with gaps.
        let direct_route = match (&surface.kind, &self.device) {
            (SurfaceKind::Vulkan(swapchain), Some(device)) => {
                base.a == 255 && device.can_scale_scene_frames(swapchain)
            }
            _ => false,
        };
        let render_scale = if direct_route {
            crate::quality::gpu_scale().unwrap_or(1.0)
        } else {
            1.0
        };
        let full_scene = scene;
        // **The damage, for a GPU frame that redraws only it** — see
        // `SceneRenderer`'s module docs. In device pixels, rounded out, then
        // grown by `GPU_DAMAGE_MARGIN`: the damage is measured against the
        // CPU rasterizer's antialiasing, and the GPU's edge coverage is a
        // different computation that may reach a little further. At a reduced
        // resolution the rectangles shrink with it, rounded outwards. Only
        // when the screen shows the GPU's own last frame at this same scale;
        // anything else redraws everything, as before.
        let gpu_damage: Option<Vec<PixelRect>> = match damage {
            Some(logical)
                if previous_gpu_scale.map(f32::to_bits) == Some(render_scale.to_bits()) =>
            {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "a swapchain extent is a small number of pixels"
                )]
                let surface_rect =
                    Rect::new(0.0, 0.0, physical_width as f32, physical_height as f32);
                let physical = Physical::new(full_scene, logical, scale, surface_rect);
                match Changed::from_damage(physical.damage(), extent) {
                    Changed::All => None,
                    Changed::Regions(regions) => Some(
                        regions
                            .into_iter()
                            .map(|region| grown_for_gpu(scaled_for_gpu(region, render_scale)))
                            .collect(),
                    ),
                }
            }
            _ => None,
        };
        let lifting = Instant::now();
        let (lifted_extent, lifted_scale) = if render_scale < 1.0 {
            #[expect(
                clippy::cast_precision_loss,
                reason = "frame extents are a small number of pixels"
            )]
            let scaled = (
                (physical_width as f32 * render_scale).round().max(1.0) as u32,
                (physical_height as f32 * render_scale).round().max(1.0) as u32,
            );
            let factor = scale.factor() * render_scale;
            (scaled, Scale::new(factor as f64))
        } else {
            (extent, scale)
        };
        let mut scene =
            frame_at_device_resolution(full_scene, logical_size, lifted_extent, lifted_scale);
        self.stages.scale = lifting.elapsed();

        // Why the CPU drew this frame, for the shadow log. Only ever built
        // when shadow mode is on: the frame path does not allocate otherwise.
        let mut fallback: Option<String> = None;

        // **`VIEWW_PLANNER_AUTHORITATIVE`: the hybrid planner, promoted,
        // deciding whole-frame placement.** The one place a planner opinion
        // can change what draws: planned *before* the GPU attempt, and a
        // CPU recommendation (only ever CPU — see `plan_for_frame`) skips
        // the GPU rasterizer entirely. Deliberately not on a
        // resolution-scaled frame: the plan would describe the scaled
        // extent while a CPU fallback draws full size, and feeding the
        // gate a prediction for a picture nobody drew is how a gate rots.
        // The plan is stashed so `record_shadow` records the prediction
        // the decision was made on, not a second one.
        let mut planner_cpu = false;
        if render_scale == 1.0 {
            if let Some(shadow) = self
                .shadow
                .as_mut()
                .filter(|s| !s.broken && s.authoritative)
            {
                let (act, plan) = shadow
                    .planner
                    .plan_for_frame(scene.as_ref(), extent.0, extent.1);
                if let Some(plan) = plan {
                    planner_cpu = act == Some(Executor::Cpu);
                    shadow.pending = Some((act, plan));
                }
            }
        }

        if planner_cpu {
            // The planner's decision, not a plan with gaps: the GPU
            // rasterizer is healthy, it was simply told this frame is not
            // worth its overhead. The fallback string says which, so a
            // log analysis can tell a decision from a limitation.
            if self.shadow.is_some() {
                fallback = Some("planner authoritative: cpu".to_owned());
            }
        } else if let (
            Some(GpuRaster { planner, renderer }),
            Some(device),
            SurfaceKind::Vulkan(swapchain),
        ) = (self.gpu.as_mut(), device.as_ref(), &mut surface.kind)
        {
            #[expect(
                clippy::cast_precision_loss,
                reason = "a swapchain extent is a small number of pixels"
            )]
            let (width, height) = (lifted_extent.0 as f32, lifted_extent.1 as f32);
            let planning = Instant::now();
            let plan = planner.plan(scene.as_ref(), width, height);
            self.stages.plan = planning.elapsed();
            if plan.is_complete() {
                // Straight into the swapchain image when the frame is opaque
                // and the driver can blit the scene's RGBA16F into the
                // swapchain's format: the whole frame stays on the GPU. A
                // translucent window keeps the readback path, because the
                // scene targets are premultiplied and a blit cannot divide.
                // (`direct_route` above checked both, plus the linear filter
                // the resolution scale's enlarging blit needs.)
                if base.a == 255 && device.can_blit_scene_frames(swapchain) {
                    // The previous frame, if still in flight, is done with the
                    // acquire semaphore and every resource this one writes.
                    // Planning above ran while it drew; this is the remainder.
                    renderer
                        .settle()
                        .map_err(|error| NativeError::Render(error.to_string()))?;
                    let acquiring = Instant::now();
                    let acquired = device
                        .acquire_image(swapchain, extent)
                        .map_err(|error| NativeError::Present(error.to_string()))?;
                    self.stages.acquire = acquiring.elapsed();
                    let Some(image) = acquired else {
                        // The swapchain was rebuilt to a new size under this
                        // frame; the matching resize repaints.
                        return Ok(PlannedFrame {
                            report: SceneReport::default(),
                            drawn_on_gpu: true,
                            presented_directly: true,
                            stages: self.stages,
                            gpu_redrawn: None,
                        });
                    };
                    let redrawn = match renderer.render_planned_to(
                        planner,
                        &plan,
                        base,
                        &image,
                        render_scale,
                        gpu_damage.as_deref(),
                    ) {
                        Ok(redrawn) => redrawn,
                        Err(error) => {
                            let _ = device.abandon_acquired(swapchain, image);
                            return Err(NativeError::Render(error.to_string()));
                        }
                    };
                    self.stages.gpu = renderer.last_timings();
                    self.stages.gpu_passes = renderer.last_pass_times();
                    let presenting = Instant::now();
                    device
                        .present_acquired(swapchain, image)
                        .map_err(|error| NativeError::Present(error.to_string()))?;
                    self.stages.present = presenting.elapsed();
                    self.record_shadow(&scene, extent, Executor::Gpu, None);
                    self.gpu_frame_current = Some(render_scale);
                    return Ok(PlannedFrame {
                        report: SceneReport::default(),
                        drawn_on_gpu: true,
                        presented_directly: true,
                        stages: self.stages,
                        gpu_redrawn: Some(surface_pixels(
                            redrawn,
                            lifted_extent,
                            extent,
                            render_scale,
                        )),
                    });
                }
                let (pixels, redrawn) = renderer
                    .render_planned_damaged(
                        planner,
                        &plan,
                        physical_width,
                        physical_height,
                        base,
                        gpu_damage.as_deref(),
                    )
                    .map_err(|error| NativeError::Render(error.to_string()))?;
                self.stages.gpu = renderer.last_timings();
                self.stages.gpu_passes = renderer.last_pass_times();
                let presenting = Instant::now();
                device
                    .present_pixels(swapchain, &pixels)
                    .map_err(|error| NativeError::Present(error.to_string()))?;
                self.stages.present = presenting.elapsed();
                let reason = self
                    .shadow
                    .is_some()
                    .then(|| "gpu frame read back through host memory".to_owned());
                self.record_shadow(&scene, extent, Executor::Gpu, reason);
                // The CPU report counts the work the CPU did; a GPU frame
                // did none of it, and the plan's own counts — vertices, draw
                // calls — are not those fields. Zeroes say what happened;
                // `drawn_on_gpu` says why.
                self.gpu_frame_current = Some(1.0);
                return Ok(PlannedFrame {
                    report: SceneReport::default(),
                    drawn_on_gpu: true,
                    presented_directly: false,
                    stages: self.stages,
                    gpu_redrawn: Some(redrawn.pixels(physical_width, physical_height)),
                });
            }
            if self.shadow.is_some() {
                let mut kinds: Vec<&str> = plan.unsupported.keys().map(|k| k.name()).collect();
                kinds.sort_unstable();
                fallback = Some(format!("plan incomplete: {}", kinds.join(", ")));
            }
        } else if self.shadow.is_some() {
            fallback = Some("no GPU rasterizer".to_owned());
        }

        // No GPU rasterizer, or the plan had gaps the backend cannot draw.
        // Either way the CPU rasterizes, in full, the frame already lifted to
        // device pixels above — exactly what `present_damaged` with no damage
        // did, minus its second lift. A fallback frame is a first frame.
        // A resolution-scaled lift only made sense for the GPU route, so it is
        // undone here: the CPU renders at the surface's full extent. The
        // extra lift is the scale's own fallback cost, paid on frames the
        // planner could not draw anyway.
        if render_scale < 1.0 {
            let lifting = Instant::now();
            scene = frame_at_device_resolution(full_scene, logical_size, extent, scale);
            self.stages.scale += lifting.elapsed();
        }
        let rasterising = Instant::now();
        let report =
            self.cpu
                .render_in_place(scene.as_ref(), physical_width, physical_height, base)?;
        self.stages.cpu_raster = rasterising.elapsed();
        self.stages.cpu_core = current_core();
        self.present_last_cpu_frame(surface, extent, Changed::All)?;
        self.record_shadow(&scene, extent, Executor::Cpu, fallback);
        Ok(PlannedFrame {
            report,
            drawn_on_gpu: false,
            presented_directly: false,
            stages: self.stages,
            gpu_redrawn: None,
        })
    }
}

/// Whether a GPU frame into the swapchain is left in flight while the
/// application builds the next one (`SceneRenderer::settle`): on unless
/// `VIEWW_GPU_OVERLAP=0` (Android: `debug.vieww.overlap=0`), the switch for
/// comparing a phone whose driver is in doubt. Read once per process.
#[must_use]
pub fn gpu_overlap() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("VIEWW_GPU_OVERLAP").map_or(true, |v| v.trim() != "0"))
}

/// Device pixels a GPU frame's damage is grown by on every side — see
/// `present_planned`. Two, not one: a rounded-out rectangle already covers
/// the CPU's half-pixel antialiasing bleed, and the margin is for the GPU's
/// coverage reaching a pixel past it. Redrawing a pixel that did not change
/// is always correct; missing one that did leaves a stale edge.
const GPU_DAMAGE_MARGIN: i32 = 2;

/// Whether the planner's profile is calibrated against this machine at
/// startup (`VIEWW_PLANNER_CALIBRATE=1`): a one-time CPU fill micro-benchmark
/// plus core-count detection, replacing the profile's round-number guesses
/// with measured numbers — see `planner_profile`. Off by default, like every
/// planner diagnostic, because the benchmark costs time and the guesses are
/// what every existing log was recorded against. Read once per process.
#[must_use]
fn planner_calibration() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("VIEWW_PLANNER_CALIBRATE").is_ok_and(|v| v.trim() == "1")
    })
}

/// A damaged region from [`Changed::from_damage`], grown by
/// [`GPU_DAMAGE_MARGIN`]; the renderer clamps it to its frame.
#[expect(clippy::cast_possible_wrap, reason = "surface coordinates")]
const fn grown_for_gpu((left, top, right, bottom): (u32, u32, u32, u32)) -> PixelRect {
    PixelRect {
        x0: left as i32 - GPU_DAMAGE_MARGIN,
        y0: top as i32 - GPU_DAMAGE_MARGIN,
        x1: right as i32 + GPU_DAMAGE_MARGIN,
        y1: bottom as i32 + GPU_DAMAGE_MARGIN,
    }
}

/// A damaged region in surface pixels, as pixels of a frame drawn at
/// `scale` of the surface: shrunk with it and rounded outwards, so a
/// changed surface pixel is never left out of the smaller frame.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "surface coordinates: small, non-negative, and rounded on purpose"
)]
fn scaled_for_gpu(region: (u32, u32, u32, u32), scale: f32) -> (u32, u32, u32, u32) {
    if scale >= 1.0 {
        return region;
    }
    let (left, top, right, bottom) = region;
    let down = |v: u32| (v as f32 * scale).floor() as u32;
    let up = |v: u32| (v as f32 * scale).ceil() as u32;
    (down(left), down(top), up(right), up(bottom))
}

/// What a GPU frame redrew, in pixels *of the surface*: the frame log's
/// repaint figure is a fraction of the surface, so a frame drawn at a
/// reduced resolution counts its redraw at the share of the surface it
/// covers — a full redraw of a smaller frame is still a full repaint.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "a pixel count, rounded"
)]
fn surface_pixels(redrawn: Redrawn, frame: (u32, u32), surface: (u32, u32), scale: f32) -> u64 {
    match redrawn {
        Redrawn::Full => u64::from(surface.0) * u64::from(surface.1),
        Redrawn::Damage { pixels } if scale < 1.0 && frame != surface => {
            let surface_area = u64::from(surface.0) * u64::from(surface.1);
            let frame_area = (u64::from(frame.0) * u64::from(frame.1)).max(1);
            ((pixels as f64) * (surface_area as f64) / (frame_area as f64)).round() as u64
        }
        Redrawn::Damage { pixels } => pixels,
    }
}

/// The planner's assumed profile for a Vulkan device type (`AdapterInfo`'s
/// `device_type`, the `Debug` of `VkPhysicalDeviceType`).
fn profile_for(device_type: &str) -> DeviceProfile {
    if device_type.contains("CPU") || device_type.contains("OTHER") {
        return DeviceProfile::CPU_ONLY_FALLBACK;
    }
    if cfg!(any(target_os = "android", target_os = "ios")) {
        return DeviceProfile::MOBILE_TILE_BASED;
    }
    let mut profile = DeviceProfile::DESKTOP_DISCRETE;
    if device_type.contains("INTEGRATED") {
        profile.gpu = GpuKind::Integrated;
    }
    profile
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaled_damage_rounds_outwards_and_full_scale_is_untouched() {
        assert_eq!(scaled_for_gpu((10, 10, 21, 33), 1.0), (10, 10, 21, 33));
        // 0.75: 7.5 -> 7, 15.75 -> 16; a changed pixel is never dropped.
        assert_eq!(scaled_for_gpu((10, 10, 21, 33), 0.75), (7, 7, 16, 25));
        assert_eq!(scaled_for_gpu((0, 0, 1, 1), 0.5), (0, 0, 1, 1));
    }

    #[test]
    fn a_scaled_frame_counts_its_redraw_as_a_share_of_the_surface() {
        let surface = (1080, 2400);
        let frame = (810, 1800); // 0.75
                                 // A full redraw of the smaller frame is a full repaint.
        assert_eq!(
            surface_pixels(Redrawn::Full, frame, surface, 0.75),
            1080 * 2400
        );
        // A tenth of the frame is a tenth of the surface.
        let tenth = 810 * 1800 / 10;
        assert_eq!(
            surface_pixels(Redrawn::Damage { pixels: tenth }, frame, surface, 0.75),
            1080 * 2400 / 10
        );
        // At full resolution, pixels are pixels.
        assert_eq!(
            surface_pixels(Redrawn::Damage { pixels: 500 }, surface, surface, 1.0),
            500
        );
    }

    /// The shadow planner plans as the device it is on: lavapipe is a CPU,
    /// and an integrated GPU is not a discrete one.
    #[test]
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    fn the_shadow_profile_follows_the_vulkan_device_type() {
        assert_eq!(profile_for("CPU"), DeviceProfile::CPU_ONLY_FALLBACK);
        assert_eq!(profile_for("DISCRETE_GPU"), DeviceProfile::DESKTOP_DISCRETE);
        assert_eq!(profile_for("INTEGRATED_GPU").gpu, GpuKind::Integrated);
    }

    fn regions(r: &[(u32, u32, u32, u32)]) -> Changed {
        Changed::Regions(r.to_vec())
    }

    /// The CPU present converts what the handed-back buffer is missing: this
    /// frame's change, plus every frame since that buffer was last shown.
    #[test]
    fn a_cpu_present_converts_what_the_buffer_missed() {
        let a = (0, 0, 10, 10);
        let b = (20, 20, 30, 30);
        let c = (40, 40, 50, 50);
        let history: VecDeque<Changed> = [regions(&[b]), regions(&[c])].into();

        // Unknown contents: everything.
        assert_eq!(conversion_plan(0, &history, &regions(&[a])), None);
        // The previous frame: only this frame's change.
        assert_eq!(conversion_plan(1, &history, &regions(&[a])), Some(vec![a]));
        // Two frames ago: this frame's and the previous frame's.
        assert_eq!(
            conversion_plan(2, &history, &regions(&[a])),
            Some(vec![a, b])
        );
        assert_eq!(
            conversion_plan(3, &history, &regions(&[a])),
            Some(vec![a, b, c])
        );
        // Older than the history remembers: everything.
        assert_eq!(conversion_plan(4, &history, &regions(&[a])), None);
        // A full repaint anywhere in what was missed: everything.
        let after_full: VecDeque<Changed> = [regions(&[b]), Changed::All].into();
        assert_eq!(
            conversion_plan(2, &after_full, &regions(&[a])),
            Some(vec![a, b])
        );
        assert_eq!(conversion_plan(3, &after_full, &regions(&[a])), None);
        assert_eq!(conversion_plan(1, &history, &Changed::All), None);
        // Nothing changed and the buffer is current: nothing to convert.
        assert_eq!(conversion_plan(1, &history, &regions(&[])), Some(vec![]));
    }

    /// Android hands buffers back by address with no age: a buffer seen two
    /// frames ago needs this frame's change and the previous one's; a new
    /// address, a gap longer than the history, or a full repaint in between
    /// means a full copy.
    #[test]
    fn buffer_ages_copy_what_each_buffer_missed() {
        let a = (0, 0, 10, 10);
        let b = (20, 20, 30, 30);
        let c = (40, 40, 50, 50);
        let mut ages = BufferAges::default();
        // Frame 1 into buffer X, frame 2 into Y: both new, so full copies.
        assert_eq!(ages.plan(0x1000, regions(&[a])), None);
        assert_eq!(ages.plan(0x2000, regions(&[b])), None);
        // Frame 3 back into X (which holds frame 1): frames 2 and 3 changed.
        let mut got = ages.plan(0x1000, regions(&[c])).expect("X is known");
        got.sort_unstable();
        assert_eq!(got, vec![b, c]);
        // Frame 4 into Y (holds frame 2): frames 3 and 4.
        let mut got = ages.plan(0x2000, regions(&[a])).expect("Y is known");
        got.sort_unstable();
        assert_eq!(got, vec![a, c]);
        // A full repaint in frame 5 (X); frame 6 into Y must copy everything.
        assert_eq!(ages.plan(0x1000, Changed::All), None);
        assert_eq!(ages.plan(0x2000, regions(&[a])), None);
        // Then steady again: frame 7 into X (holds 5): frames 6 and 7 only.
        assert_eq!(ages.plan(0x1000, regions(&[b])), Some(vec![b, a]));
        // A buffer at a new address is unknown, whatever came before.
        assert_eq!(ages.plan(0x3000, regions(&[a])), None);
        // After a reset, nothing is known.
        ages.reset();
        assert_eq!(ages.plan(0x1000, regions(&[a])), None);
    }

    /// One buffer, presented every frame (age 1 each time): only the frame's
    /// own change, and nothing at all when nothing changed.
    #[test]
    fn buffer_ages_single_buffer_copies_only_this_frame() {
        let a = (0, 0, 10, 10);
        let mut ages = BufferAges::default();
        assert_eq!(ages.plan(0x1000, regions(&[a])), None);
        assert_eq!(ages.plan(0x1000, regions(&[a])), Some(vec![a]));
        assert_eq!(ages.plan(0x1000, regions(&[])), Some(vec![]));
    }

    /// Damage rounds out to whole pixels, clamps to the surface, and drops
    /// what lies outside it — the same bounds the retained render writes.
    #[test]
    fn damage_becomes_the_pixels_the_retained_render_writes() {
        let surface = Rect::new(0.0, 0.0, 100.0, 50.0);
        let mut damage = Damage::new(surface);
        damage.add(Rect::new(10.2, 5.7, 20.1, 9.0));
        damage.add(Rect::new(90.0, 40.0, 130.0, 70.0));
        damage.add(Rect::new(200.0, 200.0, 210.0, 210.0));
        let Changed::Regions(mut got) = Changed::from_damage(&damage, (100, 50)) else {
            panic!("partial damage is regions");
        };
        got.sort_unstable();
        // `Damage::add` widens each rect by a pixel of antialiasing bleed
        // before this sees it; the rounding out is this function's.
        assert_eq!(got, vec![(9, 4, 22, 10), (89, 39, 100, 50)]);
        assert_eq!(
            Changed::from_damage(&Damage::everything(surface), (100, 50)),
            Changed::All
        );
    }

    #[test]
    fn conversion_is_rgba_to_0rgb_inside_the_region_only() {
        // 3x2: red, green, blue / white, black, 50%-alpha grey.
        let rgba = [
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, //
            255, 255, 255, 255, 0, 0, 0, 255, 128, 128, 128, 128,
        ];
        let mut out = [0xDEAD_BEEF_u32; 6];
        convert_rows(&rgba, &mut out, 3, (1, 0, 3, 2));
        assert_eq!(
            out,
            [
                0xDEAD_BEEF,
                0x00_00FF00,
                0x00_0000FF,
                0xDEAD_BEEF,
                0x00_000000,
                0x00_808080
            ]
        );
    }

    #[test]
    fn frame_stages_total_every_stage() {
        let ms = Duration::from_millis;
        let stages = FrameStages {
            scale: ms(1),
            plan: ms(2),
            cpu_raster: ms(3),
            gpu: GpuFrameTimings {
                upload: ms(4),
                record: ms(5),
                submit: ms(6),
                wait: ms(7),
                readback: ms(8),
            },
            acquire: ms(9),
            present: ms(10),
            cpu_core: Some(3),
            cpu_kinds: Some(RasterKindTimes {
                text: ms(200),
                ..RasterKindTimes::default()
            }),
            // The GPU's own clock, overlapping `wait`: not part of the total.
            gpu_passes: Some(GpuPassTimes {
                draw: ms(100),
                ..GpuPassTimes::default()
            }),
        };
        assert_eq!(stages.total(), ms(55));
        assert_eq!(FrameStages::default().total(), Duration::ZERO);
    }

    /// A scene is borrowed unchanged when logical and physical extents agree,
    /// and lifted through `Scene::scaled` when they do not — the same
    /// decision `present_damaged` makes, so the two paths agree about what a
    /// HiDPI frame is. Planning a scene with an open subpath at either
    /// resolution makes no difference to the assertion; what is checked is
    /// *which* frame reaches the planner.
    #[test]
    fn a_frame_is_borrowed_at_1x_and_lifted_off_1x() {
        let mut scene = Scene::default();
        scene.push_command(vieww_paint::Command::FillRect {
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            paint: vieww_paint::Paint::solid(Color::BLACK),
            transform: vieww_foundation::Transform::IDENTITY,
            clip: vieww_paint::Clip::NONE,
        });

        // 1:1 — the scene itself, not a copy of it.
        let frame = frame_at_device_resolution(&scene, (100, 80), (100, 80), Scale::new(1.0));
        assert!(matches!(frame, std::borrow::Cow::Borrowed(_)));

        // 2:1 — lifted, and the lift actually scaled: the fill's rect doubles.
        let frame = frame_at_device_resolution(&scene, (100, 80), (200, 160), Scale::new(2.0));
        match frame {
            std::borrow::Cow::Owned(_) => {}
            std::borrow::Cow::Borrowed(_) => panic!("an off-1x frame must be lifted, not borrowed"),
        }
    }
}
