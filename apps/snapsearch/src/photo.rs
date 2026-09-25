//! What a photo is, and where one comes from.
//!
//! Two sources, and the app is explicit about which it is showing:
//!
//! * [`Source::File`] — a real image decoded off disk. This is the app.
//! * [`Source::Sample`] — a generated gradient, used only when no photo
//!   directory was found, so a fresh checkout still opens onto something.
//!   The library header says so rather than passing them off as photos.

use std::path::PathBuf;
use std::rc::Rc;

use crate::concept::{Concept, CONCEPTS};
use crate::embed::local::gradient_image;

pub type Image_ = vieww::foundation::Image;

/// Thumbnail bound. Every photo is decoded down to fit inside this, which is
/// what keeps a thousand-photo library in tens of megabytes rather than
/// gigabytes — and it is plenty for both the grid and the embedder.
pub const THUMB_MAX: u32 = 256;

/// Sample tiles are generated at the size the embedder actually looks at.
const SAMPLE_W: u32 = 48;
const SAMPLE_H: u32 = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A real file. The path is also the photo's identity.
    File(PathBuf),
    /// A generated placeholder — see the module docs.
    Sample,
}

impl Source {
    pub fn path(&self) -> Option<&PathBuf> {
        match self {
            Self::File(path) => Some(path),
            Self::Sample => None,
        }
    }

    pub const fn is_sample(&self) -> bool {
        matches!(self, Self::Sample)
    }
}

#[derive(Debug, Clone)]
pub struct Photo {
    /// Stable across runs: the file's path, or `sample-N`. The embedding
    /// cache is keyed on it, so it must not change between launches.
    pub id: String,
    pub label: String,
    /// The thumbnail — what the grid draws and what the embedder reads.
    pub image: Image_,
    /// Height over width, from the *original* pixels.
    pub aspect: f32,
    pub source: Source,
}

impl Photo {
    /// What to show under a photo. A file's stem, tidied up.
    pub fn caption(&self) -> &str {
        &self.label
    }
}

/// A filename stem as a human would read it: `IMG_2024-06-03_sunset` →
/// `IMG 2024 06 03 sunset`.
pub fn label_from_path(path: &std::path::Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().replace(['_', '-'], " "))
        // A dotfile's "stem" is the whole name, so `.jpg` comes back as
        // `.jpg` rather than as nothing. Trim the dots before deciding
        // whether anything is left.
        .map(|s| s.trim_matches('.').trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "untitled".to_string())
}

/// A deterministic FNV-1a hash. No RNG dependency, and identical between
/// runs — which the sample library and the tests both rely on.
pub fn hash_str(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

// ===================================================== the sample library

/// One generated photo of `concept`, varied by `nth`.
pub fn photo_of(concept: &Concept, nth: u64) -> Photo {
    let skew = ((nth % 5) as f32 - 2.0) * 0.18;
    let aspect = 0.85 + ((nth.wrapping_mul(37)) % 100) as f32 / 100.0 * 0.75;
    Photo {
        id: format!("sample:{}-{nth}", concept.name.replace(' ', "-")),
        label: concept.name.to_string(),
        image: gradient_image(concept.top, concept.bottom, SAMPLE_W, SAMPLE_H, skew),
        aspect,
        source: Source::Sample,
    }
}

/// A photo for an arbitrary counter — what the masonry's live-replace reaches
/// for when a sample tile turns over.
pub fn photo_for_seed(seed: u64) -> Photo {
    photo_of(&CONCEPTS[(seed as usize) % CONCEPTS.len()], seed)
}

/// `count` generated photos, cycling through the concepts.
pub fn sample_library(count: usize) -> Rc<Vec<Photo>> {
    Rc::new(
        (0..count as u64)
            .map(|i| photo_of(&CONCEPTS[(i as usize) % CONCEPTS.len()], i))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_library_covers_every_concept() {
        let library = sample_library(CONCEPTS.len() * 2);
        for concept in CONCEPTS {
            assert!(
                library.iter().any(|p| p.label == concept.name),
                "no photo for {}",
                concept.name
            );
        }
    }

    #[test]
    fn ids_are_unique_so_the_index_and_the_cache_can_key_on_them() {
        let library = sample_library(48);
        let mut ids: Vec<_> = library.iter().map(|p| p.id.clone()).collect();
        ids.sort();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before, "duplicate photo ids");
    }

    #[test]
    fn a_label_comes_from_the_filename_and_is_readable() {
        assert_eq!(
            label_from_path(std::path::Path::new("/x/IMG_2024-06-03_sunset.jpg")),
            "IMG 2024 06 03 sunset"
        );
        // A dotfile's stem is its whole name; the leading dot is trimmed
        // rather than the name being thrown away. (Hidden files are skipped
        // by the scanner anyway — see `library::scan`.)
        assert_eq!(label_from_path(std::path::Path::new("/x/.jpg")), "jpg");
        // A path with no filename at all still gets something to draw.
        assert_eq!(label_from_path(std::path::Path::new("/")), "untitled");
    }
}
