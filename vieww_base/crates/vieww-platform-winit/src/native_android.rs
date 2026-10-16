//! CPU frames on Android: straight into the `ANativeWindow`'s own buffer.
//!
//! `softbuffer` works here too, but its Android backend hands out a fresh
//! `Vec<u32>` per frame, asks for `0x00RRGGBB` words and then swizzles them
//! back into the window's RGBA bytes on present — an allocation and three
//! full-frame passes, on the phones where a frame is 2.5 million pixels. The
//! rasterizer's output is already RGBA8, which is the window's own
//! `R8G8B8X8` layout byte for byte, so this copies rows and nothing else.
//!
//! **Only what each buffer is missing — and never Android's copy-back.** The
//! first version passed the frame's damage to `ANativeWindow_lock`, which then
//! copies the rest of the previous frame into the new buffer itself. Measured
//! on Device Farm (2026-10-16, same build, same phone, presented this way and
//! through Vulkan): present 7.1 ms here against 3.2 ms through Vulkan on a
//! 1440x3200 Galaxy S20. A window buffer is typically uncached memory, and
//! copy-back *reads* nearly all of it every frame. Now the whole buffer is
//! locked (no copy-back), each buffer is recognised by its address
//! (`BufferAges`), and only the regions changed since that buffer last held a
//! frame are written — writes only, which is what uncached memory is good at.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ndk::hardware_buffer_format::HardwareBufferFormat;
use ndk::native_window::NativeWindow;
use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
use winit::window::Window;

use super::{BufferAges, Changed, NativeError};

pub(super) struct CpuPresenter {
    native: NativeWindow,
    pub(super) extent: (u32, u32),
    ages: BufferAges,
}

fn device_error(message: impl std::fmt::Display) -> NativeError {
    NativeError::Device(format!("CPU present: {message}"))
}

fn present_error(message: impl std::fmt::Display) -> NativeError {
    NativeError::Present(format!("CPU present: {message}"))
}

impl CpuPresenter {
    pub(super) fn new(window: &Arc<Window>, width: u32, height: u32) -> Result<Self, NativeError> {
        let handle = window.window_handle().map_err(device_error)?;
        let RawWindowHandle::AndroidNdk(handle) = handle.as_raw() else {
            return Err(device_error("not an Android window"));
        };
        // SAFETY: winit's handle points at this window's live `ANativeWindow`;
        // `clone_from_ptr` takes a reference of its own (`ANativeWindow_acquire`)
        // and `NativeWindow`'s `Drop` releases it.
        let native = unsafe { NativeWindow::clone_from_ptr(handle.a_native_window.cast()) };
        let mut presenter = Self {
            native,
            extent: (0, 0),
            ages: BufferAges::default(),
        };
        presenter.resize(width, height)?;
        // The copy below is byte for byte, so anything but a 4-byte RGBA
        // layout goes back to Vulkan rather than showing wrong colours.
        match presenter.native.format() {
            HardwareBufferFormat::R8G8B8X8_UNORM | HardwareBufferFormat::R8G8B8A8_UNORM => {
                Ok(presenter)
            }
            other => Err(device_error(format!(
                "window buffers are {other:?}, not RGBA8"
            ))),
        }
    }

    pub(super) fn resize(&mut self, width: u32, height: u32) -> Result<(), NativeError> {
        if width == 0 || height == 0 || self.extent == (width, height) {
            return Ok(());
        }
        let w = i32::try_from(width).map_err(present_error)?;
        let h = i32::try_from(height).map_err(present_error)?;
        self.native
            .set_buffers_geometry(w, h, Some(HardwareBufferFormat::R8G8B8X8_UNORM))
            .map_err(present_error)?;
        self.extent = (width, height);
        self.ages.reset();
        Ok(())
    }

    pub(super) fn present(
        &mut self,
        frame: &[u8],
        extent: (u32, u32),
        changed: Changed,
    ) -> Result<Duration, NativeError> {
        self.resize(extent.0, extent.1)?;
        let (width, height) = (self.extent.0 as usize, self.extent.1 as usize);
        if frame.len() != width * height * 4 {
            // The rasterizer has not caught up with a resize; the next frame
            // repaints in full.
            return Ok(Duration::ZERO);
        }
        // Nothing changed: every buffer already shows this frame's pixels
        // where it matters, and posting nothing keeps the queue as it is.
        if matches!(&changed, Changed::Regions(regions) if regions.is_empty()) {
            return Ok(Duration::ZERO);
        }
        // The whole buffer: no copy-back (see the module docs). Timed: this
        // is where a full buffer queue makes the frame wait.
        let locking = Instant::now();
        let mut buffer = self.native.lock(None).map_err(present_error)?;
        let acquired = locking.elapsed();
        let cols = width.min(buffer.width());
        let rows = height.min(buffer.height());
        let stride = buffer.stride();
        let bits = buffer.bits().cast::<u8>();
        if cols != width || rows != height {
            // A buffer from before a resize: unknown contents, and this frame
            // fills what overlaps.
            self.ages.reset();
        }
        let plan = self.ages.plan(bits as usize, changed);
        let full = [(0, 0, cols as u32, rows as u32)];
        let regions: &[(u32, u32, u32, u32)] = plan.as_deref().unwrap_or(&full);
        for &(l, t, r, b) in regions {
            let (left, right) = ((l as usize).min(cols), (r as usize).min(cols));
            let (top, bottom) = ((t as usize).min(rows), (b as usize).min(rows));
            if left >= right {
                continue;
            }
            for y in top..bottom {
                let src = &frame[(y * width + left) * 4..(y * width + right) * 4];
                // SAFETY: `bits` is `stride * height * 4` bytes (a 4-byte
                // format, checked at creation), `y < buffer.height()` and
                // `right <= buffer.width() <= stride`, so the row's span is in
                // bounds; the source is our own frame, which cannot overlap
                // the window's buffer.
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        src.as_ptr(),
                        bits.add((y * stride + left) * 4),
                        src.len(),
                    );
                }
            }
        }
        // Dropping the guard unlocks and posts.
        drop(buffer);
        Ok(acquired)
    }
}
