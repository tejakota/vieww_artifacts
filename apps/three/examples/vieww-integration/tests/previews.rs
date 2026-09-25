//! Renders PNG previews of the viewer — run on demand with
//!
//! ```text
//! cargo test -p vieww-integration --test previews -- --ignored --nocapture
//! ```
//!
//! Ignored by default because it writes files; the regular test suite never
//! touches the disk beyond `target/`. The same headless path the viewer
//! tests use — real element tree, real frame loop, the CPU rasterizer — so
//! what lands in the PNG is what the window shows.

use std::rc::Rc;
use std::time::Duration;

use vieww::prelude::*;
use vieww_test_harness::TestHarness;
use vieww_test_harness::visual;

use three_runtime::demo_capture;

use vieww_integration::{PlaybackState, ViewerScreen};

const WINDOW: Size = Size { width: 480.0, height: 820.0 };
const OUT_DIR: &str = "target/previews";

fn mounted_viewer() -> TestHarness {
    let mut harness = TestHarness::new(WINDOW);
    // The same dark theme `App::theme` publishes in the real application.
    harness.mount(Theme::new(ThemeData::dark()).child(WidgetNode::new(ViewerScreen {
        capture: Rc::new(demo_capture()),
    })));
    harness
}

fn save(harness: &mut TestHarness, name: &str) {
    let frame = visual::render(harness.driver(), WINDOW, Color::WHITE);
    let dir = std::path::Path::new(OUT_DIR);
    std::fs::create_dir_all(dir).expect("create the previews directory");
    let path = dir.join(name);
    frame.write_png(&path).expect("write the preview");
    println!("wrote {}", path.display());
}

fn edit(harness: &mut TestHarness, edit: impl FnOnce(&mut PlaybackState)) {
    let cell = {
        let driver = harness.driver();
        let tree = driver.elements();
        let root = tree
            .iter()
            .into_iter()
            .find(|element| element.debug_name() == "ViewerScreen")
            .expect("the root screen is mounted");
        root.state().expect("the screen owns a playback state").clone()
    };
    let mut state = cell.borrow_mut();
    let playback = state
        .as_any_mut()
        .downcast_mut::<PlaybackState>()
        .expect("the screen's state is playback state");
    edit(playback);
    drop(state);
    harness.request_frame();
}

#[test]
#[ignore = "writes PNG files; run with -- --ignored"]
fn render_the_reference_previews() {
    // 1. The viewer as it opens: playing the demo capture, shaded, motes on.
    let mut harness = mounted_viewer();
    harness.tick(Duration::from_millis(100));
    save(&mut harness, "viewer-initial.png");

    // 2. Halfway through the timeline: the wave surface has rippled on.
    edit(&mut harness, |state| {
        state.set_playing(false);
        state.scrub(0.5);
    });
    harness.tick(Duration::from_millis(50));
    save(&mut harness, "viewer-halfway.png");

    // 3. Wireframe from a fresh angle: a dragged camera, wire mode, motes
    //    left on so the two layers read together.
    edit(&mut harness, |state| {
        state.orbit(160.0, 60.0);
        state.zoom(-40.0);
        state.toggle_wireframe();
    });
    harness.tick(Duration::from_millis(50));
    save(&mut harness, "viewer-wireframe.png");
}
