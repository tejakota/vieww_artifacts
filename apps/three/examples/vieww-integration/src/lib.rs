//! The reference Vieww viewer for `.3` captures.
//!
//! This crate is the **Vieww side** of the boundary described in
//! `docs/VIEWW-INTEGRATION.md`: it depends on Vieww and on the `three` media
//! stack, and is the only place in the workspace where those two meet. The
//! media stack stays free of UI-framework dependencies; everything here could
//! be lifted into any other Vieww application unchanged.
//!
//! Layout:
//!
//! * [`state`] — `PlaybackState`, the `ElementState` that owns the
//!   [`three_vieww::ThreeView`] media controller and the orbit camera, and
//!   advances playback from the frame clock.
//! * [`painter`] — `MeshPainter`, a `vieww` `Painter` that records the
//!   current frame's projected triangles, wireframe or point motes into a
//!   `Sketchbook`.
//! * [`widgets`] — `ViewerScreen` (the root), `ThreeMediaView` (the
//!   gesture-driven 3D surface) and the playback controls.
//! * `load_capture` — reads a `.3` file through the `three-app` layer, or
//!   falls back to the deterministic demo capture; returns it shared.
//!
//! Run it (from the workspace root, with a Vieww checkout alongside — see
//! the workspace `Cargo.toml`):
//!
//! ```text
//! cargo run -p vieww-integration                       # demo capture
//! cargo run -p vieww-integration -- ./sample.3         # a .3 file
//! ```
//!
//! The tests in `tests/` run headlessly through `vieww-test-harness`: no
//! window, no GPU, a clock the test controls.

use std::fmt;
use std::rc::Rc;

use three_core::Capture3D;
use three_format::FormatError;

pub mod painter;
pub mod state;
pub mod widgets;

pub use painter::{MeshPainter, ViewPalette};
pub use state::{PlaybackState, Snapshot};
pub use widgets::{ControlsBar, StateHandle, ThreeMediaView, ViewerScreen};

/// Why a capture could not be loaded.
#[derive(Debug)]
pub enum LoadError {
    /// Reading the file failed.
    Io(std::io::Error),
    /// The bytes were not a valid `.3` file.
    Format(FormatError),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "could not read the file: {error}"),
            Self::Format(error) => write!(f, "not a loadable capture: {error}"),
        }
    }
}

impl std::error::Error for LoadError {}

impl From<std::io::Error> for LoadError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<FormatError> for LoadError {
    fn from(error: FormatError) -> Self {
        Self::Format(error)
    }
}

/// Load a capture for viewing, shared.
///
/// With a path, the bytes go through the canonical `.3` decoder and validation
/// path before being shared with the Vieww presentation layer. Without one, the
/// deterministic [`demo capture`](three_runtime::demo_capture) is used, so
/// `cargo run -p vieww-integration` shows a real temporal capture with no
/// file to hand.
///
/// The result is one [`Rc`]`<Capture3D>`: the screen, the playback state and
/// the painter all share it, and nothing downstream copies the geometry.
pub fn load_capture(path: Option<&str>) -> Result<Rc<Capture3D>, LoadError> {
    match path {
        Some(path) => {
            let bytes = std::fs::read(path)?;
            let capture = three_format::decode(&bytes)?;
            Ok(Rc::new(capture))
        }
        None => Ok(Rc::new(three_runtime::demo_capture())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_path_the_demo_capture_loads() {
        let capture = load_capture(None).expect("the demo always loads");
        assert!(!capture.meshes.is_empty(), "the demo is a temporal capture");
        assert_eq!(capture.title, "3 demo capture");
    }

    #[test]
    fn a_real_file_round_trips_through_open_asset() {
        let capture = three_runtime::demo_capture();
        let bytes = three_runtime::save(&capture).expect("encode");
        let dir = std::env::temp_dir().join("vieww-integration-load-test");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("round-trip.3");
        std::fs::write(&path, &bytes).expect("write");

        let loaded = load_capture(path.to_str()).expect("the file we just wrote loads");
        assert_eq!(*loaded, capture);
    }

    #[test]
    fn garbage_bytes_report_a_format_error() {
        let dir = std::env::temp_dir().join("vieww-integration-load-test");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("garbage.3");
        std::fs::write(&path, b"definitely not a dot3 file").expect("write");

        let error = load_capture(path.to_str()).expect_err("not a .3 file");
        assert!(
            matches!(error, LoadError::Format(_)),
            "bad magic must surface as a format error, got {error:?}"
        );
    }

    #[test]
    fn a_missing_file_reports_io() {
        let error = load_capture(Some("/nonexistent/nope.3")).expect_err("no such file");
        assert!(matches!(error, LoadError::Io(_)), "got {error:?}");
    }
}
