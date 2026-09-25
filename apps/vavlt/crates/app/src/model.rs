//! What a screen reads. One type per thing the user can see.
//!
//! These are view models, not engine types: every byte count is already a
//! string, because a figure formatted at two different call sites is a figure
//! that eventually disagrees with itself. The engine owns the arithmetic and
//! this owns the wording.

use std::sync::Arc;

use vavlt_engine::Proof;

/// Where the user is.
///
/// Depth, not an id. The flow is linear, so the screen you are on *is* how far
/// you have got — which makes the transition derivable rather than
/// choreographed per edge, and makes it structurally impossible to reach the
/// run without passing the plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Screen {
    /// The tabs: vavlt, Activity, Settings.
    Root = 0,
    Plan = 1,
    Working = 2,
    Outcome = 3,
    /// One photograph's proof. Reachable from any tile.
    Photo = 4,
}

impl Screen {
    #[must_use]
    pub const fn depth(self) -> usize {
        self as usize
    }

    /// One step back out. `Root` is the floor — there is nothing under it, and
    /// a back gesture there belongs to the OS.
    #[must_use]
    pub const fn back(self) -> Self {
        match self {
            Self::Root | Self::Plan => Self::Root,
            Self::Working => Self::Plan,
            Self::Outcome => Self::Root,
            // A photo was opened *from* the outcome, so back is the run rather
            // than the root — the only edge in the flow that is not "one less".
            Self::Photo => Self::Outcome,
        }
    }
}

/// Which tab, at the root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Vavlt = 0,
    Activity = 1,
    Settings = 2,
}

impl Tab {
    pub const ALL: [Self; 3] = [Self::Vavlt, Self::Activity, Self::Settings];

    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Vavlt => "vavlt",
            Self::Activity => "Activity",
            Self::Settings => "Settings",
        }
    }

    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    #[must_use]
    pub const fn from_index(index: usize) -> Self {
        match index {
            1 => Self::Activity,
            2 => Self::Settings,
            _ => Self::Vavlt,
        }
    }
}

/// How a tile should draw itself.
///
/// The first version had three parallel row types — one for the selection, one
/// for the run, one for the result — which meant three list widgets, three
/// layouts, and a photo that changed identity every time it crossed a screen. A
/// photo is one thing; this is the only difference between the plan screen and
/// the outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PhotoState {
    /// Picked, not yet run.
    #[default]
    Waiting,
    /// In the codec right now.
    Working,
    Proven,
    PixelLossless,
    Unqualified,
    Failed,
    Skipped,
}

impl PhotoState {
    #[must_use]
    pub const fn from_proof(proof: Proof) -> Self {
        match proof {
            Proof::Proven => Self::Proven,
            Proof::PixelLossless => Self::PixelLossless,
            Proof::Unqualified => Self::Unqualified,
            Proof::Failed => Self::Failed,
            Proof::Skipped => Self::Skipped,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Waiting => "not run yet",
            Self::Working => "working",
            Self::Proven => "proven",
            Self::PixelLossless => "pixel-lossless",
            Self::Unqualified => "unqualified",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }

    /// Whether a result badge belongs on the tile yet.
    #[must_use]
    pub const fn has_result(self) -> bool {
        !matches!(self, Self::Waiting | Self::Working)
    }

    /// The long-form explanation on the photo screen.
    ///
    /// Here rather than in the screen because it is the app's *claim* about
    /// each state, and a claim belongs next to the state it is about.
    #[must_use]
    pub const fn explanation(self) -> &'static str {
        match self {
            Self::Proven => {
                "The encoded file was decoded back and compared against your original byte for \
                 byte. They matched. This is the only condition under which vavlt will call a \
                 file reversible."
            }
            Self::PixelLossless => {
                "Every pixel survives, but the original file cannot be reproduced from the \
                 result — oxipng rewrites the container. So the optimised copy becomes the \
                 canonical one, and this is recorded as pixel-lossless rather than as proven."
            }
            Self::Unqualified => {
                "The encode succeeded, but the quality gate could not run on this device, so the \
                 ratio is not quoted as a saving and is excluded from the total."
            }
            Self::Failed => {
                "Your original is untouched and remains the only copy. The failure is logged as \
                 loudly as a success — see Activity."
            }
            Self::Skipped => {
                "This one was not eligible for re-encoding. It was counted, hashed for duplicate \
                 detection, and left exactly as it was."
            }
            Self::Waiting | Self::Working => "This photo has not been run yet.",
        }
    }
}

/// One photograph, carried through the whole flow.
#[derive(Debug, Clone)]
pub struct Photo {
    pub name: String,
    /// The decoded tile, RGBA8. `None` until the loader thread gets to it, so
    /// tiles fade in rather than blocking the grid.
    pub thumb: Option<Thumb>,
    pub size_label: String,
    pub class_label: String,
    /// RAW/DNG. Never re-encoded in any tier — marked so you can watch the
    /// engine refuse rather than be told that it did.
    pub protected: bool,
    /// Excluded by tapping the tile. Excluded photos leave the manifest, so the
    /// hash changes and any grant against the old plan lapses.
    pub included: bool,

    // --- filled in as the run progresses ---------------------------------
    pub state: PhotoState,
    pub after_label: String,
    pub pct_label: String,
    pub note: String,
}

impl Photo {
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        size_label: impl Into<String>,
        class_label: impl Into<String>,
        protected: bool,
    ) -> Self {
        Self {
            name: name.into(),
            thumb: None,
            size_label: size_label.into(),
            class_label: class_label.into(),
            protected,
            included: true,
            state: PhotoState::Waiting,
            after_label: String::new(),
            pct_label: String::new(),
            note: String::new(),
        }
    }
}

/// Decoded pixels, sized.
///
/// A plain struct rather than `vieww`'s `Image` so this crate's model layer
/// stays free of framework types and can be built in a test with no tree.
#[derive(Debug, Clone)]
pub struct Thumb {
    /// Shared because the UI tree is rebuilt as thumbnail batches arrive.
    /// Cloning this handle must not clone hundreds of kilobytes per tile.
    pub pixels: Arc<Vec<u8>>,
    pub edge: u32,
}

impl Thumb {
    #[must_use]
    pub fn new(pixels: Vec<u8>, edge: u32) -> Self {
        Self {
            pixels: Arc::new(pixels),
            edge,
        }
    }
}

/// One line in the audit log, as the Activity screen reads it.
#[derive(Debug, Clone)]
pub struct AuditRow {
    pub time: String,
    pub title: String,
    pub detail: String,
    /// The chain link. Shown rather than tucked behind a developer setting — it
    /// is what makes the log worth having.
    pub hash: String,
    pub kind: AuditKind,
}

/// What colour the rule beside an entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditKind {
    Grant,
    Run,
    Skip,
    Fail,
    Restore,
}

impl AuditKind {
    #[must_use]
    pub fn parse(kind: &str) -> Self {
        match kind {
            "grant" => Self::Grant,
            "skip" => Self::Skip,
            "fail" => Self::Fail,
            "restore" => Self::Restore,
            _ => Self::Run,
        }
    }
}

/// The two tiers, priced.
///
/// Both figures are computed, not just the selected one: choosing a tier
/// without being shown what it is worth is not a choice.
#[derive(Debug, Clone, Default)]
pub struct Estimate {
    pub move_label: String,
    pub deep_label: String,
    /// The selected tier's figure.
    pub selected_label: String,
    /// The same figure as a fraction of what was handed over.
    ///
    /// Carried rather than derived at the call site: the meter under the hero
    /// needs a ratio, and the alternative is the UI parsing back the string it
    /// was just given — which is how a bar and the number above it end up
    /// disagreeing.
    pub selected_ratio: f32,
    pub codec_label: String,
    pub dedup_label: String,
    /// vavlt overhead — thumbnails and previews. A cost the user is entitled to
    /// see as a negative line rather than as a footnote.
    pub overhead_label: String,
}

/// What a finished run proved.
#[derive(Debug, Clone, Default)]
pub struct RunResult {
    pub saved_label: String,
    pub before_label: String,
    pub after_label: String,
    pub pct: f32,
    pub proven: usize,
    pub unqualified: usize,
    pub failed: usize,
}
