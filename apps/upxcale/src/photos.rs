//! The photo catalogue: what the app has, and how a file becomes pixels.
//!
//! In the HTML prototype this was a `PHOTOS` array of picsum.photos seeds and a
//! `src()` that built a URL. Here it is the same list against real files in
//! `assets/photos/`, decoded through `vieww-asset` and cached, because
//! `vieww_foundation::Image` compares by identity rather than by pixels: decode
//! the same file twice and the two are unequal, so every frame repaints the
//! whole picture. The cache is what makes a scroll cost nothing.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use vieww_asset::{decode, AssetBundle, AssetError};
use vieww_foundation::Image;

use crate::upscale;

/// The shape a tile takes in the masonry.
///
/// The prototype called this the tile's "extent" and drove it from a CSS
/// `aspect-ratio`. It is kept as an enum rather than a bare `f32` because the
/// grid packs by it and the picker ignores it — a name survives that refactor,
/// a magic ratio does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Extent {
    Portrait,
    Square,
    Landscape,
    Tall,
    Wide,
}

impl Extent {
    /// Width over height, the way [`AspectRatio`](vieww_widget::AspectRatio)
    /// wants it.
    #[must_use]
    pub const fn ratio(self) -> f32 {
        match self {
            Self::Portrait => 3.0 / 4.0,
            Self::Square => 1.0,
            Self::Landscape => 4.0 / 3.0,
            Self::Tall => 9.0 / 16.0,
            Self::Wide => 16.0 / 9.0,
        }
    }
}

/// One entry in the catalogue. Cheap to clone — it is an id and a path, not
/// pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Photo {
    pub id: &'static str,
    /// The file under `assets/photos/`, without the directory or the extension.
    pub slug: &'static str,
    /// What the compare sheet puts in its title.
    pub title: &'static str,
    pub extent: Extent,
}

impl Photo {
    /// The bundle-relative path, slash-separated with no leading slash — the
    /// only form `AssetBundle::open` accepts.
    #[must_use]
    pub fn asset_path(&self) -> String {
        format!("photos/{}.png", self.slug)
    }
}

/// The twelve photographs the app ships with.
///
/// A real build would populate this from the device's photo library; the
/// picker's job and the grid's job do not change when it does, which is why
/// every screen below takes `&[Photo]` rather than reaching for this constant.
pub const CATALOGUE: &[Photo] = &[
    Photo {
        id: "a1",
        slug: "mountain-ridge",
        title: "Mountain ridge",
        extent: Extent::Portrait,
    },
    Photo {
        id: "a2",
        slug: "still-lake",
        title: "Still lake",
        extent: Extent::Square,
    },
    Photo {
        id: "a3",
        slug: "pine-forest",
        title: "Pine forest",
        extent: Extent::Landscape,
    },
    Photo {
        id: "a4",
        slug: "dune-field",
        title: "Dune field",
        extent: Extent::Tall,
    },
    Photo {
        id: "a5",
        slug: "city-dusk",
        title: "City at dusk",
        extent: Extent::Square,
    },
    Photo {
        id: "a6",
        slug: "aurora-night",
        title: "Aurora",
        extent: Extent::Portrait,
    },
    Photo {
        id: "a7",
        slug: "north-coast",
        title: "North coast",
        extent: Extent::Landscape,
    },
    Photo {
        id: "a8",
        slug: "summer-meadow",
        title: "Summer meadow",
        extent: Extent::Square,
    },
    Photo {
        id: "a9",
        slug: "red-canyon",
        title: "Red canyon",
        extent: Extent::Portrait,
    },
    Photo {
        id: "a10",
        slug: "morning-fog",
        title: "Morning fog",
        extent: Extent::Wide,
    },
    Photo {
        id: "a11",
        slug: "high-peak",
        title: "High peak",
        extent: Extent::Square,
    },
    Photo {
        id: "a12",
        slug: "slow-river",
        title: "Slow river",
        extent: Extent::Landscape,
    },
];

/// Look one up by the id the signals carry.
#[must_use]
pub fn by_id(id: &str) -> Option<&'static Photo> {
    CATALOGUE.iter().find(|p| p.id == id)
}

/// The factor the Upscale button applies, and what the badge says.
pub const UPSCALE_FACTOR: u32 = 4;

/// Decoded sources and their upscaled results, kept for the life of the app.
///
/// `RefCell` rather than `&mut` throughout: a widget's `build` takes `&self`,
/// and the whole point of this type is to be read during one. Every method here
/// takes `&self` for that reason.
pub struct Library {
    bundle: Rc<dyn AssetBundle>,
    sources: RefCell<HashMap<String, Image>>,
    upscaled: RefCell<HashMap<String, Image>>,
}

/// Hand-written because `dyn AssetBundle` is not `Debug` and the orphan rule
/// puts an `impl Debug for dyn AssetBundle` out of reach from here. Every
/// `Widget` in this crate needs `Debug`, and one that holds a `Library` needs
/// this — printing the counts is also more use in a tree dump than a bundle's
/// address would be.
impl std::fmt::Debug for Library {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Library")
            .field("sources", &self.sources.borrow().len())
            .field("upscaled", &self.upscaled.borrow().len())
            .finish()
    }
}

impl Library {
    #[must_use]
    pub fn new(bundle: Rc<dyn AssetBundle>) -> Self {
        Self {
            bundle,
            sources: RefCell::new(HashMap::new()),
            upscaled: RefCell::new(HashMap::new()),
        }
    }

    /// The photo as shot: decoded once, then handed out by clone.
    ///
    /// `Image` clones share their pixel buffer through an `Arc`
    /// (`Image::from_shared_rgba8`), so this is a refcount bump rather than a
    /// copy of the bitmap.
    pub fn source(&self, photo: &Photo) -> Result<Image, AssetError> {
        let path = photo.asset_path();
        if let Some(image) = self.sources.borrow().get(&path) {
            return Ok(image.clone());
        }
        let bytes = self.bundle.open(&path)?;
        let (image, _format) = decode(&bytes)?;
        self.sources.borrow_mut().insert(path, image.clone());
        Ok(image)
    }

    /// The upscaled result, if this photo has been through the renderer.
    #[must_use]
    pub fn result(&self, photo: &Photo) -> Option<Image> {
        self.upscaled.borrow().get(photo.id).cloned()
    }

    /// Whether this photo already has a result.
    #[must_use]
    pub fn is_upscaled(&self, photo: &Photo) -> bool {
        self.upscaled.borrow().contains_key(photo.id)
    }

    /// Run the upscaler and keep the result.
    ///
    /// This is synchronous and it is the expensive call in the app — a 300x300
    /// source at 4x is a 1200x1200 Lanczos resample plus an unsharp pass. The
    /// progress route is what stands in front of it; see `render.rs` for how it
    /// is driven off the main thread.
    pub fn render(&self, photo: &Photo) -> Result<Image, AssetError> {
        if let Some(done) = self.result(photo) {
            return Ok(done);
        }
        let source = self.source(photo)?;
        let output = upscale::upscale(&source, UPSCALE_FACTOR);
        self.upscaled
            .borrow_mut()
            .insert(photo.id.to_string(), output.clone());
        Ok(output)
    }

    /// Insert an already-computed result — the path a background render takes
    /// when it hands its work back to the tree.
    pub fn adopt(&self, photo: &Photo, image: Image) {
        self.upscaled
            .borrow_mut()
            .insert(photo.id.to_string(), image);
    }

    /// Forget every result. The app never calls this; a "clear cache" setting
    /// would.
    pub fn clear_results(&self) {
        self.upscaled.borrow_mut().clear();
    }
}
