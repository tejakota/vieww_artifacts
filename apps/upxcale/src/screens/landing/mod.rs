//! The landing screen and the three routes that sit over it.
//!
//! One module per component file in `prototype/screens/landing/`, so a change
//! that was scoped to one CSS file there is scoped to one module here.

pub mod backdrop;
pub mod compare_sheet;
pub mod photo_grid;
pub mod photo_tile;
pub mod picker_dialog;
pub mod progress_overlay;
pub mod render_button;
pub mod screen;

pub use screen::{compare_route, landing_route, picker_route, progress_route, Landing};
