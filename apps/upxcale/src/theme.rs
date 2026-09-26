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

use vieww_foundation::{Color, Offset, Shadow, TargetPlatform};
use vieww_widget::{ColorScheme, Metrics, Motion, ThemeData, Typography};

/// The platform the controls draw the shapes of, resolved at compile time the
/// same way `ThemeData::from_colors` does — so the same binary looks native on
/// the machine it runs on. (`with_platform` is the deterministic override a
/// test would use; the desktop harness here wants the host's answer.)
#[must_use]
pub const fn host_platform() -> TargetPlatform {
    TargetPlatform::current()
}

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
        platform: host_platform(),
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
        platform,
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
///
/// With the frosted backdrop now blurring for real, this is the *unblurred*
/// spelling — the depth the barrier alone paints when nothing frosts it.
pub const SCRIM_PICKER: Color = Color::rgba(0, 0, 0, 115);

/// The wash the picker's frosted route carries. Shallower than
/// [`SCRIM_PICKER`] because the blur beneath it is doing half the
/// pushing-back — stacked at full depth on top of a blur, the grid the user
/// is about to pick from would stop reading entirely, which is the opposite
/// of what `backdrop-blur.css` asked for.
pub const SCRIM_PICKER_BLURRED: Color = Color::rgba(0, 0, 0, 66);

/// The picker's backdrop-blur sigma. 22 is the top of the phone-dialog range
/// — deep enough that the grid reads as *behind glass* rather than merely
/// dimmed, shallow enough that photo colours still come through as colours,
/// which is what the user is choosing between.
pub const PICKER_BLUR: f32 = 22.0;

/// The barrier behind the progress card. Deeper — nothing behind it is
/// actionable while a render is in flight.
pub const SCRIM_PROGRESS: Color = Color::rgba(0, 0, 0, 158);

/// The shadow every floating card casts — the picker and the progress card,
/// the same one deliberately. Neutral rather than accent-tinted (the Render
/// button's [`RENDER_SHADOW`] owns the tint) because a card is a surface, not
/// an action, and its lift should not announce itself in colour. The offset
/// and blur are the “mid dialog” step of the elevation scale: far enough that
/// the card reads as floating, shallow enough that it does not halo.
pub const CARD_SHADOW: Shadow = Shadow::new(
    Color::rgba(0, 0, 0, 140),
    Offset::new(0.0, 16.0),
    40.0,
);

/// The shadow the **Render** button casts — the one accent-tinted shadow in
/// the app, because the one primary action is the one thing allowed to glow
/// in its own colour. Lifts it off a grid of photographs, where a neutral
/// shadow disappears against a dark tile.
pub const RENDER_SHADOW: Shadow = Shadow::new(
    Color::rgba(10, 132, 255, 107),
    Offset::new(0.0, 8.0),
    24.0,
);

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
