//! Integration tests for the reference viewer, driven through
//! `vieww-test-harness`: a real element tree, a real frame loop, a clock the
//! test owns, and the CPU rasterizer for pixel assertions. No window, no
//! GPU, no sleeping.
//!
//! Two oracles are used deliberately:
//!
//! * **State reads** — the mounted `PlaybackState` is read (and, where a test
//!   needs to *arrange* rather than *act*, written) through the element
//!   tree, exactly the channel the control handlers use. Asserting on state
//!   rather than on text keeps these tests independent of layout.
//! * **Pixels and the tree dump** — for the things only rendering can prove.

use std::rc::Rc;
use std::time::Duration;

use vieww::debug_tree;
use vieww::prelude::*;
use vieww_test_harness::visual;
use vieww_test_harness::TestHarness;

use three_format::LazyCapture;

use vieww_integration::{PlaybackState, ViewerScreen};

/// The window these tests mount into — tall, like the phone this viewer is
/// for, and the size every coordinate below is computed against.
const WINDOW: Size = Size {
    width: 480.0,
    height: 820.0,
};

/// The capture these tests mount: the synthetic sample, written and opened
/// lazily — the same path a downloaded feed asset takes.
fn sample() -> Rc<LazyCapture> {
    let capture = three_runtime::sample_capture();
    let bytes = three_runtime::save(&capture).unwrap();
    Rc::new(LazyCapture::open(bytes).unwrap())
}

/// A mounted viewer showing the sample capture.
///
/// The root is wrapped in the same dark `Theme` the real application's
/// `App::theme` publishes, so what the tests lay out and render is what the
/// window shows — without it, `ThemeData::of` falls back to the light theme
/// and every assertion would be made against a screen the app never draws.
fn mounted_viewer() -> TestHarness {
    let mut harness = TestHarness::new(WINDOW);
    harness.mount(
        Theme::new(ThemeData::dark()).child(WidgetNode::new(ViewerScreen { capture: sample() })),
    );
    harness
}

/// Draw at least one frame, without meaningfully advancing the clock.
fn pump(harness: &mut TestHarness) {
    harness.tick(Duration::from_millis(1));
}

/// The root element's state cell, the same handle the tree hands control
/// handlers.
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

/// Apply an edit to the mounted playback state — the exact channel a
/// gesture or control handler uses.
fn edit(harness: &mut TestHarness, edit: impl FnOnce(&mut PlaybackState)) {
    let cell = state_cell(harness);
    let mut state = cell.borrow_mut();
    let playback = state
        .as_any_mut()
        .downcast_mut::<PlaybackState>()
        .expect("the screen's state is the playback state");
    edit(playback);
}

/// A read of the mounted playback state.
fn with_state(harness: &mut TestHarness, read: impl FnOnce(&PlaybackState)) {
    let cell = state_cell(harness);
    let state = cell.borrow();
    let playback = state
        .as_any()
        .downcast_ref::<PlaybackState>()
        .expect("the screen's state is the playback state");
    read(playback);
}

#[test]
fn the_viewer_mounts_with_a_playing_state_and_real_pixels() {
    let mut harness = mounted_viewer();
    pump(&mut harness);

    with_state(&mut harness, |state| {
        assert!(state.playing(), "a fresh viewer plays");
        assert!(state.view().player().frame_index().is_some());
        // The rendered frame is the capture's own resolution.
        let (w, h) = (state.rendered().width, state.rendered().height);
        assert_eq!((w, h), (96, 64), "the sample's dimensions");
        assert_eq!(state.rendered().rgba.len(), (w * h * 4) as usize);
    });

    // The element tree contains the media surface and its image.
    let dump = debug_tree(WidgetNode::new(ViewerScreen { capture: sample() }));
    assert!(
        dump.contains("ThreeMediaView"),
        "the media surface is mounted:\n{dump}"
    );
    assert!(
        dump.contains("Image"),
        "the pixels are an Image widget:\n{dump}"
    );
}

#[test]
fn playback_advances_through_the_real_frame_loop() {
    let mut harness = mounted_viewer();
    pump(&mut harness);

    let first = {
        let cell = state_cell(&mut harness);
        let state = cell.borrow();
        state
            .as_any()
            .downcast_ref::<PlaybackState>()
            .unwrap()
            .view()
            .player()
            .time_ns()
    };

    harness.tick(Duration::from_millis(500));

    with_state(&mut harness, |state| {
        assert!(
            state.view().player().time_ns() > first,
            "half a second moved the clock"
        );
        assert!(state.view().player().time_ns() <= 600_000_000);
    });
}

#[test]
fn the_hold_gesture_recentres_the_camera_through_the_state() {
    let mut harness = mounted_viewer();
    pump(&mut harness);

    // Act through the same channel the gesture recognizer uses.
    edit(&mut harness, |state| state.orbit(120.0, 90.0));
    edit(&mut harness, |state| state.pinch_step(1.9));
    with_state(&mut harness, |state| {
        assert!(
            !state.view().camera().is_neutral(),
            "the gestures moved the camera"
        );
    });

    // Hold — the recenter spring — and let the loop settle it.
    edit(&mut harness, |state| state.hold());
    for _ in 0..600 {
        harness.tick(Duration::from_millis(16));
    }
    with_state(&mut harness, |state| {
        assert!(
            state.view().camera().is_neutral(),
            "hold returns the view to the filmed angle"
        );
    });
}

#[test]
fn an_orbit_changes_the_pixels_on_screen() {
    let mut harness = mounted_viewer();
    pump(&mut harness);

    let before = visual::render(harness.driver(), WINDOW, Color::WHITE);
    edit(&mut harness, |state| state.orbit(80.0, 60.0));
    pump(&mut harness);
    let after = visual::render(harness.driver(), WINDOW, Color::WHITE);

    let differences = before.differences(&after, 10);
    assert!(
        !differences.is_empty(),
        "orbiting the camera must re-photograph the frame"
    );
}

#[test]
fn pausing_freezes_the_moment_on_screen() {
    let mut harness = mounted_viewer();
    pump(&mut harness);

    edit(&mut harness, |state| state.set_playing(false));
    // Let the rebuild land first — the transport row relabels its button
    // (Pause -> Play), and the freeze must be a photograph of the settled
    // screen, not of the label mid-swap.
    pump(&mut harness);
    let frozen = visual::render(harness.driver(), WINDOW, Color::WHITE);

    harness.tick(Duration::from_millis(400));
    let still = visual::render(harness.driver(), WINDOW, Color::WHITE);

    assert!(
        frozen.differences(&still, 10).is_empty(),
        "a paused viewer is a photograph, not a video"
    );
}
