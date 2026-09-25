//! One file on its way through the flow, and what came back.

use vavlt_core::{classify_name, AssetClass, AssetMeta, Tier};

use crate::source::Handle;

/// A file the user handed over, before anything has been opened.
///
/// `class` is guessed from the name here and confirmed from the header during
/// the run — the extension was only ever a claim, and the tier screen has to
/// show *something* without reading pixels.
#[derive(Debug, Clone)]
pub struct Item {
    pub handle: Handle,
    pub name: String,
    pub bytes: u64,
    pub class: AssetClass,
}

impl Item {
    /// Build one from what a picker reported: a handle, a display name, a size.
    #[must_use]
    pub fn new(handle: impl Into<Handle>, name: impl Into<String>, bytes: u64) -> Self {
        let name = name.into();
        Self {
            class: classify_name(&name),
            handle: handle.into(),
            name,
            bytes,
        }
    }

    #[must_use]
    pub fn meta(&self) -> AssetMeta {
        AssetMeta {
            path: self.name.clone(),
            bytes: self.bytes,
            class: self.class,
        }
    }
}

/// Binds a grant to the exact plan the user was shown (SPEC §1.3).
///
/// Any drift — one file added, one excluded, the tier changed — produces a
/// different hash, and the run refuses to start against a grant that names the
/// old one. The hash is displayed on the plan screen for the same reason the
/// audit chain is displayed: a binding nobody can see is a binding nobody can
/// check.
#[must_use]
pub fn manifest_hash(items: &[Item], tier: Tier) -> String {
    let mut h = blake3::Hasher::new();
    h.update(match tier {
        Tier::Move => b"move",
        Tier::DeepMove => b"deep",
        Tier::DeepMovePlus => b"deep+",
    });
    for item in items {
        h.update(item.handle.as_bytes());
        h.update(&item.bytes.to_le_bytes());
    }
    h.finalize().to_hex()[..32].to_string()
}

/// What the engine is willing to say about a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Proof {
    /// Decoded back and byte-compared. The only basis for calling a file
    /// reversible.
    Proven,
    /// Pixels survive, bytes do not — the original file is not reconstructable.
    PixelLossless,
    /// Encoded, but no perceptual score, so the ratio is not a saving.
    Unqualified,
    Failed,
    /// The engine declined to touch it at all.
    Skipped,
}

impl Proof {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Proven => "proven",
            Self::PixelLossless => "pixel-lossless",
            Self::Unqualified => "unqualified",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }

    /// Whether this row's bytes may be counted toward the headline figure.
    ///
    /// Invariant 8: a ratio with no quality qualification is not a saving the
    /// user can have, so it is not one the app will quote.
    #[must_use]
    pub const fn countable(self) -> bool {
        matches!(self, Self::Proven | Self::PixelLossless)
    }
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub name: String,
    pub codec: String,
    pub before: u64,
    pub after: u64,
    pub proof: Proof,
    pub note: String,
    /// The chosen tier promised something this build cannot deliver, so the
    /// engine delivered the safe thing instead. Counted separately from the
    /// proof, because the result is still proven — it is the *promise* that was
    /// not kept.
    pub fell_back: bool,
}

impl Outcome {
    #[must_use]
    pub const fn saved(&self) -> u64 {
        self.before.saturating_sub(self.after)
    }

    #[must_use]
    pub fn pct(&self) -> f32 {
        if self.before == 0 || !self.proof.countable() {
            return 0.0;
        }
        self.saved() as f32 / self.before as f32 * 100.0
    }
}
