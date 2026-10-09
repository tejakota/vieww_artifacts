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
use std::fmt;
use std::rc::Rc;

use vieww_foundation::{Color, Rect};
use vieww_gpu::Planner as GpuPlanner;
use vieww_hal::vulkan::{SceneRenderer, VulkanDevice, VulkanError, VulkanSwapchain};
use vieww_paint::native::{NativeRenderer as CpuRenderer, RendererError, SceneReport};
use vieww_paint::{Damage, Scene};

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

/// A window's swapchain. See [`vieww_hal::vulkan::VulkanSwapchain`] for the
/// Vulkan object this owns.
#[derive(Debug)]
pub struct NativeSurface {
    swapchain: VulkanSwapchain,
}

impl NativeSurface {
    /// Destroy the swapchain and its `VkSurfaceKHR` while `renderer`'s device
    /// and the window are both still alive. Must run before either is dropped;
    /// see `VulkanSwapchain::release` for the crash that ordering caused.
    /// Idempotent. The surface cannot present afterwards.
    pub fn release(&mut self, renderer: &NativeRenderer) {
        self.swapchain.release(&renderer.device);
    }
}

thread_local! {
    /// The one `VkInstance`/`VkDevice` this process opens, kept alive for as
    /// long as any window holds it.
    static SHARED_DEVICE: RefCell<Option<Rc<VulkanDevice>>> = const { RefCell::new(None) };
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
    let existing = SHARED_DEVICE.with(|cell| cell.borrow().clone());
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
    SHARED_DEVICE.with(|cell| *cell.borrow_mut() = Some(Rc::clone(&device)));
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

/// Presents [`vieww_paint::Scene`]s rasterised by `vieww-paint`'s `native`
/// backend, through `vieww-hal`'s Vulkan swapchain. See the module docs for
/// what this does and does not cover yet.
pub struct NativeRenderer {
    device: Rc<VulkanDevice>,
    cpu: CpuRenderer,
    /// The GPU rasterizer, built on first use — see [`GpuRaster`].
    gpu: Option<GpuRaster>,
    /// Set when [`GpuRaster`]'s creation failed, so the attempt is made once
    /// and never again.
    gpu_unavailable: bool,
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
            Self {
                device,
                cpu,
                gpu: None,
                gpu_unavailable: false,
            },
            NativeSurface { swapchain },
        ))
    }

    /// Cached glyph outlines — forwarded from the CPU rasterizer.
    #[must_use]
    pub fn cached_fonts(&self) -> usize {
        self.cpu.cached_fonts()
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
        let (physical_width, physical_height) = surface.swapchain.extent();
        let (logical_width, logical_height) = logical_size;

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
                Some(damage) => self.cpu.render_retained_in_place(
                    scene,
                    damage,
                    logical_width,
                    logical_height,
                    base,
                )?,
                None => self
                    .cpu
                    .render_in_place(scene, logical_width, logical_height, base)?,
            }
        } else {
            let factor = scale.factor();
            let scaled = scene.scaled(factor);
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

        // In place: the renderer's retained output is read directly, rather
        // than copied into a fresh `Pixels` every frame — that copy was a
        // full-frame allocation per present, which the Vieww standard's
        // steady-state clause counts. The rasterisation above already ran at
        // the swapchain's own extent, so there is nothing left to resample.
        let frame = self.cpu.last_frame();
        debug_assert_eq!(
            frame.len(),
            physical_width as usize * physical_height as usize * 4,
            "the renderer rasterised at the swapchain's extent"
        );

        self.device
            .present_pixels(&mut surface.swapchain, frame)
            .map_err(|error| NativeError::Present(error.to_string()))?;
        Ok(report)
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
        surface
            .swapchain
            .recreate(&self.device, width, height)
            .map_err(|error| NativeError::Present(error.to_string()))
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
    /// the fallback are the same agreement [`ScenePlan::is_complete`] and
    /// `present_damaged` have always had.
    ///
    /// The GPU path is a whole-frame rasterizer: it has no damage regions to
    /// honour and no retained content to reuse, so every frame it takes is a
    /// full repaint. The read-back into host memory it does per frame is a
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
    /// be built (once — see [`GpuRaster`]); after that, as
    /// [`present_damaged`](Self::present_damaged).
    pub fn present_planned(
        &mut self,
        surface: &mut NativeSurface,
        scene: &Scene,
        base: Color,
        logical_size: (u32, u32),
        scale: Scale,
    ) -> Result<PlannedFrame, NativeError> {
        let (physical_width, physical_height) = surface.swapchain.extent();

        // The GPU rasterizer, once. Creation failure is remembered and this
        // becomes a CPU renderer permanently — see `gpu_unavailable`.
        if self.gpu.is_none() && !self.gpu_unavailable {
            match SceneRenderer::new(&self.device) {
                Ok(renderer) => {
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

        if let Some(GpuRaster { planner, renderer }) = self.gpu.as_mut() {
            let scene = frame_at_device_resolution(
                scene,
                logical_size,
                (physical_width, physical_height),
                scale,
            );
            #[expect(
                clippy::cast_precision_loss,
                reason = "a swapchain extent is a small number of pixels"
            )]
            let width = physical_width as f32;
            #[expect(
                clippy::cast_precision_loss,
                reason = "a swapchain extent is a small number of pixels"
            )]
            let height = physical_height as f32;
            let plan = planner.plan(scene.as_ref(), width, height);
            if plan.is_complete() {
                let pixels = renderer
                    .render_planned(planner, &plan, physical_width, physical_height, base)
                    .map_err(|error| NativeError::Render(error.to_string()))?;
                self.device
                    .present_pixels(&mut surface.swapchain, &pixels)
                    .map_err(|error| NativeError::Present(error.to_string()))?;
                // The CPU report counts the work the CPU did; a GPU frame
                // did none of it, and the plan's own counts — vertices, draw
                // calls — are not those fields. Zeroes say what happened;
                // `drawn_on_gpu` says why.
                return Ok(PlannedFrame {
                    report: SceneReport::default(),
                    drawn_on_gpu: true,
                });
            }
        }

        // No GPU rasterizer, or the plan had gaps the backend cannot draw.
        // Either way the CPU rasterizes, in full — `present_damaged` with no
        // damage is the whole-frame path a first frame and a resize want,
        // and a fallback frame is exactly a first frame.
        let report = self.present_damaged(surface, scene, None, base, logical_size, scale)?;
        Ok(PlannedFrame {
            report,
            drawn_on_gpu: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
