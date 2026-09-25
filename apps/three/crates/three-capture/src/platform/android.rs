//! Android camera bridge.
//!
//! Production implementation notes:
//! - obtain camera frames through Camera2/CameraX;
//! - ingest ARCore camera pose and intrinsics when available;
//! - ingest depth when the device exposes it;
//! - copy only the minimum data needed by reconstruction;
//! - never block the Vieww/UI thread waiting for a camera frame.
//!
//! The concrete JNI/NDK implementation is intentionally left behind this
//! module boundary until the Android app crate is introduced.
