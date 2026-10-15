//! CPU frames on Android: straight into the `ANativeWindow`'s own buffer.
//!
//! `softbuffer` works here too, but its Android backend hands out a fresh
//! `Vec<u32>` per frame, asks for `0x00RRGGBB` words and then swizzles them
//! back into the window's RGBA bytes on present — an allocation and three
//! full-frame passes, on the phones where a frame is 2.5 million pixels. The
//! rasterizer's output is already RGBA8, which is the window's own
//! `R8G8B8X8` layout byte for byte, so this copies rows and nothing else.
//!
//! **Only what changed.** `ANativeWindow_lock` takes the region about to be
//! drawn, copies the rest of the previous frame into the new buffer itself
//! when it can, and answers with the region the caller must actually fill —
//! the whole buffer when it could not copy back. So a frame whose damage is a
//! button copies the button's rows, and the platform keeps the rest.

use std::sync::Arc;

use ndk::hardware_buffer_format::HardwareBufferFormat;
use ndk::native_window::{NativeWindow, Rect as ARect};
use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
use winit::window::Window;

use super::{Changed, NativeError};

pub(super) struct CpuPresenter {
    native: NativeWindow,
    pub(super) extent: (u32, u32),
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
        Ok(())
    }

    pub(super) fn present(
        &mut self,
        frame: &[u8],
        extent: (u32, u32),
        changed: Changed,
    ) -> Result<(), NativeError> {
        self.resize(extent.0, extent.1)?;
        let (width, height) = (self.extent.0 as usize, self.extent.1 as usize);
        if frame.len() != width * height * 4 {
            // The rasterizer has not caught up with a resize; the next frame
            // repaints in full.
            return Ok(());
        }
        #[expect(
            clippy::cast_possible_wrap,
            clippy::cast_possible_truncation,
            reason = "a window extent fits an i32 (checked in resize)"
        )]
        let mut dirty = match &changed {
            Changed::All => ARect {
                left: 0,
                top: 0,
                right: width as i32,
                bottom: height as i32,
            },
            // Nothing changed: the window already shows this frame.
            Changed::Regions(regions) if regions.is_empty() => return Ok(()),
            Changed::Regions(regions) => {
                let (mut l, mut t, mut r, mut b) = (u32::MAX, u32::MAX, 0, 0);
                for &(rl, rt, rr, rb) in regions {
                    l = l.min(rl);
                    t = t.min(rt);
                    r = r.max(rr);
                    b = b.max(rb);
                }
                ARect {
                    left: l as i32,
                    top: t as i32,
                    right: r as i32,
                    bottom: b as i32,
                }
            }
        };
        let mut buffer = self.native.lock(Some(&mut dirty)).map_err(present_error)?;
        // What the platform asked to have filled, clamped to what both the
        // buffer and the frame hold (a buffer from before a resize can be
        // the other size for one frame).
        let cols = width.min(buffer.width());
        let rows = height.min(buffer.height());
        let clamp = |v: i32, max: usize| usize::try_from(v.max(0)).unwrap_or(0).min(max);
        let (left, right) = (clamp(dirty.left, cols), clamp(dirty.right, cols));
        let (top, bottom) = (clamp(dirty.top, rows), clamp(dirty.bottom, rows));
        let stride = buffer.stride();
        let bits = buffer.bits().cast::<u8>();
        if left < right {
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
        Ok(())
    }
}
