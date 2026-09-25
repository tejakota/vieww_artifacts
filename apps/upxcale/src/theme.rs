//! The application's [`ThemeData`], and the few tokens the theme has no field
//! for.
//!
//! Mirrors `prototype/shared/theme.css`. That file's whole argument was: no
//! control hard-codes a colour or a size, every value comes from one place, and
//! swapping that place restyles the app. vieww enforces the same rule
//! structurally — a control calls `ThemeData::of(ctx)` once per build and reads
//! a role off it — so this module is just where the numbers live.
//!
//! # Why `apple_dark` and not a scheme of our own
//!
//! `ColorScheme::apple_dark()` is already systemBlue on true black with
//! systemGreen for success, which is exactly what the prototype's
//! `[data-theme="apple-dark"]` block spelled out by hand. Reaching for it
//! rather than restating eleven hex values means the app follows the framework
//! if those conventions are ever corrected.

use vieww_foundation::Color;
use vieww_foundation::TargetPlatform;
use vieww_widget::{ColorScheme, Metrics, Motion, ThemeData, Typography};

/// The dark scheme the app ships in.
///
/// A photo app is a dark app: the grid is full of images, and a light chrome
/// around them changes how every one of them reads. This is the one place that
/// decision lives.
#[must_use]
pub fn scheme() -> ColorScheme {
    ColorScheme::apple_dark()
}

/// The whole theme, assembled.
///
/// `Metrics::apple()` is the 44pt touch target and the 10pt corner the
/// prototype's `Metrics` block named. `Motion::standard()` rather than
/// `expressive()` because the one moment in this app that should feel
/// theatrical — the render — has its own overlay doing the work, and springy
/// chrome around it would compete.
#[must_use]
pub fn theme() -> ThemeData {
    let colors = scheme();
    ThemeData {
        colors,
        text: Typography::scale(colors.on_surface),
        metrics: Metrics::apple(),
        motion: Motion::standard(),
    }
}

/// The same, but following the host platform's conventions for corner radius
/// and touch target while keeping our colours.
///
/// Unused by the desktop harness, which wants one look it can screenshot. It is
/// here because `main.rs` on a phone should call it: a 44pt target on Android
/// is wrong, and `Metrics::adaptive` is the framework's answer.
#[must_use]
pub fn adaptive(platform: TargetPlatform) -> ThemeData {
    let colors = scheme();
    ThemeData {
        colors,
        text: Typography::scale(colors.on_surface),
        metrics: Metrics::adaptive(platform),
        motion: Motion::adaptive(platform),
    }
}

// ── Tokens with no `ThemeData` field ────────────────────────────────────────
//
// `ColorScheme` carries eleven roles and none of them is "the wash a scrim
// paints" or "the ink on a photograph". Those are real tokens — the prototype
// declared them as `--vw-scrim` and friends — so they live here beside the
// theme rather than as literals at the call sites. The rule the prototype set
// was that a *screen* may introduce a token; it may not introduce a literal.

/// The barrier behind the picker. Shallower than the others: the grid stays
/// legible through it, which is the point of blurring rather than hiding.
pub const SCRIM_PICKER: Color = Color::rgba(0, 0, 0, 115);

/// The barrier behind the progress card. Deeper — nothing behind it is
/// actionable while a render is in flight.
pub const SCRIM_PROGRESS: Color = Color::rgba(0, 0, 0, 158);

/// The barrier behind the compare sheet. Deepest, so the two images are the
/// only things on screen with any luminance.
pub const SCRIM_COMPARE: Color = Color::rgba(0, 0, 0, 217);

/// Ink on top of a photograph, where the surface roles do not apply because the
/// surface is an image. Always white, always on a dark plate.
pub const ON_PHOTO: Color = Color::rgb(255, 255, 255);

/// The plate that ink sits on: a badge over a tile, a Before/After label.
pub const PHOTO_PLATE: Color = Color::rgba(0, 0, 0, 140);

/// The wash a selected tile takes, so selection reads on a bright photo as
/// well as a dark one.
pub const SELECTION_WASH: Color = Color::rgba(255, 255, 255, 41);

/// The divider line in the compare stage, and the ring on a fresh tile.
pub const DIVIDER: Color = Color::rgb(255, 255, 255);

/// `DISABLED_ALPHA` as a multiplier, for the places that dim by opacity rather
/// than by compositing a colour.
pub const DISABLED_OPACITY: f32 = 0.38;
