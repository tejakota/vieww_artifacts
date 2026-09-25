//! Native platform integration points.
//!
//! The files in this module are intentionally conservative stubs. Camera APIs
//! are unsafe to fake: the Android implementation must use Camera2/CameraX or
//! an equivalent native path, while iOS should use AVFoundation/ARKit.

#[cfg(target_os = "android")]
pub mod android;

#[cfg(target_os = "ios")]
pub mod ios;
