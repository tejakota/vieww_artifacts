//! iOS camera bridge.
//!
//! Production implementation notes:
//! - obtain frames through AVFoundation;
//! - ingest ARKit camera pose/intrinsics when available;
//! - use LiDAR/depth when present, but keep RGB-only capture supported;
//! - transfer frames through a bounded queue so capture backpressure cannot
//!   freeze the Vieww event loop.
//!
//! The concrete Objective-C/Swift bridge is intentionally kept out of the core
//! crates and should be added in the iOS application target.
