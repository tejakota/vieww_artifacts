//! The depth-warp presentation layer: how a `.3` depth video becomes pixels.
//!
//! # What this crate does
//!
//! A depth video is footage plus per-pixel distance. To let a viewer *look
//! at it from somewhere else*, you re-project: take every source pixel,
//! place it in 3D at its measured depth, and photograph the result from the
//! virtual camera the viewer's gestures control. [`warp_frame`] is that
//! photograph — a z-buffered, hole-filled splat renderer, pure float math,
//! no GPU required, which is the same choice the framework's own software
//! rasterizer makes and for the same reason: it works on every backend.
//!
//! # The camera contract
//!
//! * **Pan/drag** orbits the virtual camera around the capture's subject
//!   ([`ViewCamera::drag`]) — the parallax you see is real geometry moving.
//! * **Pinch** dollies in and out ([`ViewCamera::pinch`]): closer is bigger,
//!   and with depth it is *genuinely* closer — foreground objects overtake
//!   the background exactly as walking forward would show.
//! * **Hold** recentres ([`ViewCamera::begin_recentre`]): a press-and-hold
//!   springs the view back to where the footage was filmed from, the one
//!   angle where every pixel is ground truth.
//!
//! At the neutral pose the warp is the identity — the renderer returns the
//! source frame untouched — so autoplay in a feed costs no re-projection at
//! all, and the warp runs only while a finger is on the glass.
//!
//! # Conventions
//!
//! Camera space is the pixel-aligned pinhole: +X right, +Y down, +Z forward,
//! in meters — the same axes [`CameraIntrinsics`] unprojects to. Output is
//! RGBA, top-left origin, the format every 2D UI (Vieww included) paints.
//!
//! # Determinism
//!
//! Source pixels splat in row-major order, z-ties break the same way every
//! run, and hole fill sweeps fixed directions: a given `(frame, camera)`
//! always produces identical bytes — which is what the viewer's tests
//! assert on.

mod camera;
mod playback;
mod warp;

pub use camera::ViewCamera;
pub use playback::{DepthView, RenderedFrame};
pub use warp::{warp_frame, WarpOutput};

/// Near plane for the warp, in meters.
///
/// Splats closer than this to the virtual camera are dropped rather than
/// projected to absurd screen positions — the same rule the old mesh viewer
/// applied for the same reason, at a distance that keeps an arm's-length
/// subject intact while the camera dollies halfway into it.
pub const WARP_NEAR: f32 = 0.05;

/// A camera at an explicit pose — for tests in this crate's modules, keeping
/// the `recentring` field's one writer honest.
#[cfg(test)]
pub(crate) fn test_camera(yaw: f32, pitch: f32, dolly: f32) -> ViewCamera {
    ViewCamera {
        yaw,
        pitch,
        dolly,
        recentring: false,
    }
}
