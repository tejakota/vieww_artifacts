//! The product page, rasterised to PNGs on the host.
//!
//! ```console
//! cargo run -p wrightsite --example render            # writes ./site-shots/*.png
//! cargo run -p wrightsite --example render -- /tmp    # somewhere else
//! ```
//!
//! # Why this exists rather than a browser
//!
//! The page's only real target is wasm, and a wasm build plus a page reload
//! is two minutes and a browser. The rasteriser that draws it is the same one
//! on both targets — `vieww_paint::native` — so the picture this writes is
//! the picture the browser presents, modulo the device-pixel ratio. That
//! makes it the fast way to review a layout change, and the only way to
//! review one on a machine that cannot install the `wasm32-unknown-unknown`
//! target.
//!
//! It renders three widths, because the page has one breakpoint and a
//! screenshot at one width says nothing about the other side of it.

use std::path::PathBuf;

use std::time::Duration;

use vieww::element::Animation;
use vieww::foundation::{Color, Size};
use vieww::paint::native::NativeRenderer;
use vieww::{FrameDriver, ScrollController, ScrollPhysics};

use wrightsite::{Art, Os};

/// The ground the page is drawn on, which the tree does not paint itself: a
/// scrollable's viewport shows the surface behind it wherever the content is
/// shorter than the window.
const GROUND: Color = Color::rgb(0x1F, 0x1F, 0x1F);

/// The widths to render, and how tall a window at each one is taken to be.
///
/// The tall heights are not window heights. They are how much of the page to
/// draw in one picture: the viewport is what the scrollable is *told* it is,
/// so a tall one renders the page as one long image rather than one
/// screenful — which is what a reviewer wants and what a browser can never
/// show at once.
const SHOTS: &[(&str, f32, f32)] = &[
    ("desktop", 1440.0, 10600.0),
    ("tablet", 800.0, 10600.0),
    ("phone", 390.0, 13600.0),
];

fn main() {
    let out: PathBuf = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "site-shots".into())
        .into();
    std::fs::create_dir_all(&out).expect("creating the output directory");

    for (name, width, height) in SHOTS {
        let surface = Size::new(*width, *height);
        let mut driver = FrameDriver::new(surface);
        // The same faces the browser gets, so these PNGs measure text the way
        // the page will — see `wrightsite::fonts`.
        driver.set_fonts(wrightsite::fonts());
        let runtime = driver.elements().runtime().clone();
        let scroll = ScrollController::new(&runtime, ScrollPhysics::android());

        // **Held at the end, not run.** The page's entrance is a one-second
        // fade and lift, and a still of a page that is 40% faded in is a
        // picture of nothing anybody will ever see. `set_progress(1.0)` is
        // the finished state, which is the state a layout review is about.
        let entrance = Animation::new(
            &runtime,
            vieww::prelude::Tween::new(0.0_f32, 1.0),
            Duration::from_millis(1000),
        );
        entrance.attach(driver.tickers());
        entrance.set_progress(1.0);

        scroll.attach(driver.tickers());

        // The demo's firing, also held at the gate rather than mid-flight: a
        // still of the demo at the gate is the honest picture, because that
        // is the state the reader first meets and the state they answer.
        let (fire, deliver, gate) = wrightsite::start_demo(&runtime, driver.tickers());
        fire.set_progress(1.0);
        deliver.set_progress(0.0);
        deliver.stop();
        gate.set(0);

        driver.set_root(wrightsite::page(
            scroll,
            entrance,
            Some(Os::Linux),
            fire,
            deliver,
            gate,
            Art::embedded(),
            wrightsite::Anchors::new(&runtime),
        ));

        // Four frames, not one. The first build runs against the seed
        // constraints; `LayoutBuilder` rebuilds against the real ones on the
        // frame after, and the scrollable reports its extents out of layout
        // after that. Four is slack, not a requirement.
        for _ in 0..4 {
            driver.draw_frame();
        }

        let mut renderer = NativeRenderer::new();
        let (png, report) = renderer
            .render_to_png(driver.scene(), *width as u32, *height as u32, GROUND)
            .expect("rasterising through vieww's own renderer");
        let path = out.join(format!("{name}.png"));
        std::fs::write(&path, png).expect("writing the PNG");
        println!(
            "{name}: {width}x{height} — {} shapes, {} glyph runs ({} glyphs), {} clips",
            report.shapes, report.glyph_runs, report.glyphs, report.clips,
        );
    }
}
