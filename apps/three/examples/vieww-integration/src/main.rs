//! `three-viewer` — the reference Vieww application for `.3` captures.
//!
//! Opens one window showing one temporal 3D capture, playing on loop. Pass a
//! `.3` file to open it; with no argument the deterministic demo capture
//! plays, so the binary is useful the moment it builds.
//!
//! ```text
//! cargo run -p vieww-integration -- ./sample.3
//! ```
//!
//! Interaction:
//!
//! * drag on the capture — orbit the camera
//! * scroll / pinch — zoom
//! * Pause/Play, Wireframe, motes, Loop, Reset view — the control row
//! * the slider — scrub the timeline
//!
//! Everything on screen is the stock widget catalogue plus one `Painting`
//! whose painter projects the current mesh through `three-vieww`; see
//! `src/painter.rs`.

use std::rc::Rc;

use vieww::prelude::*;
use vieww_platform_winit::App;

use vieww_integration::{ViewerScreen, load_capture};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1);
    let capture: Rc<three_core::Capture3D> = match load_capture(path.as_deref()) {
        Ok(capture) => capture,
        Err(error) => {
            eprintln!("three-viewer: {error}");
            eprintln!("falling back to the built-in demo capture");
            load_capture(None)?
        }
    };

    let title = if path.is_some() { "3 — viewer" } else { "3 — viewer (demo capture)" };

    let report = App::new()
        .title(title)
        .size(Size::new(480.0, 820.0))
        .theme(viewer_theme())
        .run(|driver| {
            driver.set_root(WidgetNode::new(ViewerScreen { capture }));
        })?;
    println!("{report}");
    Ok(())
}

/// The viewer's theme.
///
/// Dark, and for the same reason every media viewer is dark: the capture
/// draws into a fixed dark chamber (see `ViewPalette`) and a dark surface
/// around it keeps the screen from framing a bright rectangle of content in
/// a glare of chrome. `ThemeData::dark()` keeps the stock controls — the
/// buttons, the slider — consistent with that choice on every platform.
fn viewer_theme() -> ThemeData {
    ThemeData::dark()
}
