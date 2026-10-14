//! The reference Vieww viewer for `.3` depth-video captures.
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
//!   [`three_vieww::DepthView`] (lazy capture + player + camera), renders
//!   on the frame clock, and holds the pixels the last tick produced.
//! * [`widgets`] — `ViewerScreen` (the root), `ThreeMediaView` (the
//!   gesture-driven surface: an `Image` fitted into the chamber, inside the
//!   gesture recognizer that drives the camera) and the playback controls.
//! * `load_capture` — reads a `.3` file lazily, or falls back to a
//!   deterministic sample; returns it shared.
//!
//! Run it (from the workspace root, with a Vieww checkout alongside — see
//! the workspace `Cargo.toml`):
//!
//! ```text
//! cargo run -p vieww-integration                       # sample capture
//! cargo run -p vieww-integration -- ./imported.3        # a .3 file
//! ```
//!
//! Interaction — the gestures are the product:
//!
//! * **pan** — orbit the camera around the subject (real parallax)
//! * **pinch** — dolly in and out *in depth*
//! * **hold** — the recenter spring, back to the filmed viewpoint
//!
//! The tests in `tests/` run headlessly through `vieww-test-harness`: no
//! window, no GPU, a clock the test controls.

use std::fmt;
use std::rc::Rc;

use three_format::{FormatError, LazyCapture};

pub mod state;
pub mod widgets;

pub use state::PlaybackState;
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

/// Load a capture for viewing, shared and lazy.
///
/// With a path, the bytes are indexed (not decoded — frames decompress on
/// demand; see [`LazyCapture`]) and shared with the viewer state. Without
/// one, the deterministic sample capture is written and loaded, so
/// `cargo run -p vieww-integration` shows a real depth video with no file
/// to hand.
pub fn load_capture(path: Option<&str>) -> Result<Rc<LazyCapture>, LoadError> {
    match path {
        Some(path) => {
            let bytes = std::fs::read(path)?;
            Ok(Rc::new(LazyCapture::open(bytes)?))
        }
        None => {
            let capture = three_runtime::sample_capture();
            let bytes = three_runtime::save(&capture)?;
            Ok(Rc::new(LazyCapture::open(bytes)?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_path_the_sample_capture_loads() {
        let capture = load_capture(None).expect("the sample always loads");
        assert!(
            capture.meta().frame_count > 0,
            "the sample is a real capture"
        );
        assert_eq!(capture.meta().title, "sample moment");
    }

    #[test]
    fn a_real_file_round_trips_through_load() {
        let capture = three_runtime::sample_capture();
        let bytes = three_runtime::save(&capture).expect("encode");
        let dir = std::env::temp_dir().join("vieww-integration-load-test");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("round-trip.3");
        std::fs::write(&path, &bytes).expect("write");

        let loaded = load_capture(path.to_str()).expect("the file we just wrote loads");
        assert_eq!(loaded.meta().title, "sample moment");
        assert_eq!(loaded.meta().frame_count, capture.frames.len());
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
