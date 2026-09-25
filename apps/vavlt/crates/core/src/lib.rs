//! vavlt engine core: asset classification, metadata-only estimation, and reporting.
//!
//! This crate is deliberately free of platform and UI dependencies so the same
//! logic runs in the desktop measurement harness and on device.

pub mod classify;
pub mod estimate;
pub mod report;
pub mod tier;

pub use classify::{classify_bytes, classify_name, classify_path, AssetClass, AssetMeta};
pub use estimate::{estimate_library, Estimate, RATIOS};
pub use report::{Aggregate, FileResult, Measurement};
pub use tier::{PhotoPolicy, Policy, Tier, VideoPolicy};

/// Whole-file content hash. Used for dedup and for lossless roundtrip proof.
///
/// Note: the engine deliberately uses *whole-file* hashing for media rather than
/// content-defined chunking. Compressed media shares no chunk boundaries across
/// files, so CDC costs CPU and index rows without finding redundancy that
/// whole-file hashing misses. CDC is reserved for the document bucket.
pub fn content_hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}
