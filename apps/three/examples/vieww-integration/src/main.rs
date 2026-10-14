//! `three-viewer` — the reference Vieww application for `.3` depth videos.
//!
//! Opens one window showing one depth-video capture, playing on loop. Pass a
//! `.3` file to open it; with no argument the deterministic sample capture
//! plays, so the binary is useful the moment it builds.
//!
//! ```text
//! cargo run -p vieww-integration -- ./imported.3
//! ```
//!
//! Interaction — the gestures are the product:
//!
//! * **pan** — orbit the camera around the subject; depth makes the near
//!   geometry sweep past the far, because this is a re-photograph, not a
//!   stretch effect
//! * **pinch** — dolly in and out *in depth*: closer is genuinely closer
//! * **hold** — the recenter spring returns the view to the one angle the
//!   footage was filmed from
//! * Pause/Play, Loop, the slider — the control row
//!
//! Everything on screen is the stock widget catalogue plus one `Image`
//! whose pixels the playback state renders each tick; see `src/state.rs`.

use std::rc::Rc;

use vieww::prelude::*;
use vieww_platform_winit::App;

use vieww_integration::{load_capture, ViewerScreen};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1);
    let capture: Rc<three_format::LazyCapture> = match load_capture(path.as_deref()) {
        Ok(capture) => capture,
        Err(error) => {
            eprintln!("three-viewer: {error}");
            eprintln!("falling back to the built-in sample capture");
            load_capture(None)?
        }
    };

    let title = if path.is_some() {
        "3 — depth viewer"
    } else {
        "3 — depth viewer (sample)"
    };

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
/// draws into a fixed dark chamber and a dark surface around it keeps the
/// screen from framing a bright rectangle of content in a glare of chrome.
/// `ThemeData::dark()` keeps the stock controls — the buttons, the slider —
/// consistent with that choice on every platform.
fn viewer_theme() -> ThemeData {
    ThemeData::dark()
}
