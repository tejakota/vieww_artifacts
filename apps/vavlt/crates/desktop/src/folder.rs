//! The desktop's stand-in for a photo picker.
//!
//! # This is the one host that does not honour SPEC §1.1
//!
//! Said plainly rather than buried. The invariant is that the engine receives
//! OS-issued handles and can open nothing it was not given; on Android and iOS
//! that is literally true — a content URI or a security-scoped bookmark from an
//! out-of-process picker, with no media permission declared, so the app
//! *cannot* enumerate storage even if it wanted to.
//!
//! A desktop has no equivalent. There is no out-of-process picker that issues a
//! revocable handle, and a Linux binary can read `$HOME` because it is a Linux
//! binary. So this walks a directory the user named on the command line, and
//! the handles it issues are paths.
//!
//! What that costs is real and is confined to this file: the engine still sees
//! only handles it was given, `Source` is still the only way to bytes, and the
//! run is byte-for-byte the same. What it does not get is the *capability*
//! property — nothing stops this process reading a file it was never handed.
//!
//! That is why this host is the harness and the phones are the product.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use vavlt_core::classify_path;
use vavlt_engine::{Item, Source};

/// Opens paths under a root.
#[derive(Debug)]
pub struct FolderSource {
    root: PathBuf,
}

impl FolderSource {
    #[must_use]
    pub const fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl Source for FolderSource {
    fn read(&self, handle: &str) -> Result<Vec<u8>> {
        let path = Path::new(handle);

        // Refuse anything outside the root even though the process could read
        // it. The capability property is unavailable here; the *discipline* is
        // not, and a handle that escaped the scan is a bug worth catching
        // rather than serving.
        let canonical = path
            .canonicalize()
            .with_context(|| format!("resolving {handle}"))?;
        let root = self
            .root
            .canonicalize()
            .with_context(|| format!("resolving {}", self.root.display()))?;
        anyhow::ensure!(
            canonical.starts_with(&root),
            "{handle} is outside the folder that was handed over"
        );

        std::fs::read(&canonical).with_context(|| format!("reading {handle}"))
    }
}

/// Walk `root` and return what the app should treat as a selection.
///
/// Metadata only — nothing is opened. That mirrors the phone flow, where the
/// tier and estimate screens genuinely run before any descriptor is used, and
/// it means a folder of ten thousand photographs costs ten thousand `stat`
/// calls rather than ten thousand decodes.
pub fn scan(root: &Path, max: usize) -> Result<Vec<Item>> {
    anyhow::ensure!(root.is_dir(), "{} is not a folder", root.display());

    let mut items = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .max_depth(4)
        .into_iter()
        // A permission error on one subdirectory must not cost the whole scan.
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        if items.len() >= max {
            break;
        }

        // `classify_path` reads the header, so it is the one place the walk
        // touches a file — and it reads a handful of bytes rather than the
        // picture. A name-only classification would be cheaper and would put a
        // `.jpg` that is really a PNG in front of the wrong codec.
        let Ok(meta) = classify_path(entry.path()) else {
            continue;
        };
        if meta.bytes == 0 {
            continue;
        }

        let name = entry
            .file_name()
            .to_string_lossy()
            .to_string();
        items.push(Item::new(
            entry.path().to_string_lossy().to_string(),
            name,
            meta.bytes,
        ));
    }

    log::info!("scan: {} files under {}", items.len(), root.display());
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one guard this file adds over `std::fs::read`, so it is the one
    /// worth a test.
    #[test]
    fn a_handle_outside_the_root_is_refused() {
        let dir = std::env::temp_dir().join(format!("vavlt-folder-{}", std::process::id()));
        let inside = dir.join("inside.txt");
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(&inside, b"ok").expect("write");

        let source = FolderSource::new(dir.clone());
        assert!(source.read(&inside.to_string_lossy()).is_ok());

        let outside = std::env::temp_dir().join("vavlt-outside.txt");
        std::fs::write(&outside, b"no").expect("write");
        let error = source
            .read(&outside.to_string_lossy())
            .expect_err("a path outside the root must be refused");
        assert!(error.to_string().contains("outside the folder"));

        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_file(&outside).ok();
    }
}
