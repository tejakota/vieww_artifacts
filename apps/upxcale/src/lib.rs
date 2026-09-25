//! **upxcale** — upscale photographs, built on [`vieww`].
//!
//! A `vieww` application, structured so that each module answers to exactly one
//! file of the HTML prototype it was drawn from. That is not a filing
//! convention: the prototype's whole premise was that a component's appearance,
//! its role, and what it delegates to its neighbours could be pinned down in
//! CSS and then carried across to Rust without re-deciding any of it. Keeping
//! the boundaries identical is what makes the two readable side by side.
//!
//! | prototype file | module |
//! |---|---|
//! | `shared/theme.css` | [`theme`] |
//! | `screens/landing/screen.css` | [`screens::landing::screen`] |
//! | `screens/landing/backdrop-blur.css` | [`screens::landing::backdrop`] |
//! | `screens/landing/photo-grid.css` | [`screens::landing::photo_grid`] |
//! | `screens/landing/photo-tile.css` | [`screens::landing::photo_tile`] |
//! | `screens/landing/render-button.css` | [`screens::landing::render_button`] |
//! | `screens/landing/picker-dialog.css` | [`screens::landing::picker_dialog`] |
//! | `screens/landing/progress-overlay.css` | [`screens::landing::progress_overlay`] |
//! | `screens/landing/compare-sheet.css` | [`screens::landing::compare_sheet`] |
//! | `app.js` | [`state`] |
//!
//! `shared/foundation.css`, `shared/widgets.css` and `shared/controls.css` have
//! no module here on purpose — they were the prototype's stand-in for
//! `vieww-foundation` and `vieww-widget`, and the real thing has replaced them.
//!
//! Three modules have no prototype counterpart at all, because the prototype
//! did not do the work: [`upscale`] is the resampler, [`render`] runs it off
//! the frame thread, and [`icons`] draws the glyphs the framework does not
//! ship.

pub mod export;
pub mod icons;
pub mod photos;
pub mod render;
pub mod screens;
pub mod state;
pub mod theme;
pub mod upscale;

pub use state::AppState;

/// The logical size the desktop harness opens at — an iPhone 14 Pro Max in
/// logical points, so the layout is exercised at the shape it ships in.
pub const SURFACE: vieww_foundation::Size = vieww_foundation::Size {
    width: 414.0,
    height: 896.0,
};
