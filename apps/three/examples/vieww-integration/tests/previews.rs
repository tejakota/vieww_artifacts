//! Renders PNG previews of the viewer — run on demand with
//!
//! ```text
//! cargo test -p vieww-integration --test previews -- --ignored --nocapture
//! ```
//!
//! Ignored by default because it writes files; the regular test suite never
//! touches the disk beyond `target/`. The same headless path the viewer
//! tests use — real element tree, real frame loop, the native rasteriser —
//! so what lands in the PNG is what the window shows.

use std::rc::Rc;
use std::time::Duration;

use vieww::prelude::*;
use vieww_test_harness::visual;
use vieww_test_harness::TestHarness;

use three_format::LazyCapture;

use vieww_integration::{PlaybackState, ViewerScreen};

const WINDOW: Size = Size {
    width: 480.0,
    height: 820.0,
};
const OUT_DIR: &str = "target/previews";

fn mounted_viewer() -> TestHarness {
    let capture = three_runtime::sample_capture();
    let bytes = three_runtime::save(&capture).unwrap();
    let mut harness = TestHarness::new(WINDOW);
    // The same dark theme `App::theme` publishes in the real application.
    harness.mount(
        Theme::new(ThemeData::dark()).child(WidgetNode::new(ViewerScreen {
            capture: Rc::new(LazyCapture::open(bytes).unwrap()),
        })),
    );
    harness
}

fn save(harness: &mut TestHarness, name: &str) {
    let frame = visual::render(harness.driver(), WINDOW, Color::WHITE);
    std::fs::create_dir_all(OUT_DIR).expect("create the previews directory");
    let path = std::path::Path::new(OUT_DIR).join(name);
    frame.write_png(&path).expect("write the frame");
    println!("wrote {}", path.display());
}

/// The root element's state cell — the same handle control handlers use.
fn state_cell(
    harness: &mut TestHarness,
) -> Rc<std::cell::RefCell<dyn vieww::widget::ElementState>> {
    let driver = harness.driver();
    let tree = driver.elements();
    let root = tree
        .iter()
        .into_iter()
        .find(|element| element.debug_name() == "ViewerScreen")
        .expect("the root screen is mounted");
    root.state()
        .expect("the screen owns a playback state")
        .clone()
}

fn edit(harness: &mut TestHarness, edit: impl FnOnce(&mut PlaybackState)) {
    let cell = state_cell(harness);
    let mut state = cell.borrow_mut();
    let playback = state
        .as_any_mut()
        .downcast_mut::<PlaybackState>()
        .expect("the screen's state is the playback state");
    edit(playback);
}

#[test]
#[ignore = "writes PNG files; run with -- --ignored --nocapture"]
fn render_the_viewer_states() {
    // 1. The moment as filmed: neutral camera, playing.
    let mut harness = mounted_viewer();
    harness.tick(Duration::from_millis(120));
    save(&mut harness, "1-neutral.png");

    // 2. Pan: the camera orbits the subject — parallax on a real
    //    re-photograph.
    edit(&mut harness, |state| state.orbit(150.0, -40.0));
    harness.tick(Duration::from_millis(60));
    save(&mut harness, "2-orbit.png");

    // 3. Pinch: dolly into the depth of the scene.
    edit(&mut harness, |state| state.pinch_step(1.8));
    harness.tick(Duration::from_millis(60));
    save(&mut harness, "3-dolly.png");

    // 4. Hold: the spring brings the view home.
    edit(&mut harness, |state| state.hold());
    for _ in 0..120 {
        harness.tick(Duration::from_millis(16));
    }
    save(&mut harness, "4-recentred.png");
}
