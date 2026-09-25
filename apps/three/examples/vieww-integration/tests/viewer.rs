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

use vieww::prelude::*;
use vieww::debug_tree;
use vieww_test_harness::TestHarness;
use vieww_test_harness::visual;

use three_runtime::demo_capture;

use vieww_integration::{PlaybackState, ViewerScreen};

/// The window these tests mount into — tall, like the phone this viewer is
/// for, and the size every coordinate below is computed against.
const WINDOW: Size = Size { width: 480.0, height: 820.0 };

/// A mounted viewer showing the deterministic demo capture.
///
/// The root is wrapped in the same dark `Theme` the real application's
/// `App::theme` publishes, so what the tests lay out and render is what the
/// window shows — without it, `ThemeData::of` falls back to the light theme
/// and every assertion would be made against a screen the app never draws.
fn mounted_viewer() -> TestHarness {
    let mut harness = TestHarness::new(WINDOW);
    harness.mount(Theme::new(ThemeData::dark()).child(WidgetNode::new(ViewerScreen {
        capture: Rc::new(demo_capture()),
    })));
    harness
}

/// Draw at least one frame, without meaningfully advancing the clock.
///
/// `tick(1ms)` steps the loop once: a requested frame draws, and the loop
/// then stops at the deadline. One vsync (~16.7 ms) of clock passes, which
/// the range assertions below tolerate.
fn pump(harness: &mut TestHarness) {
    harness.tick(Duration::from_millis(1));
}

/// Read the mounted playback state.
fn read<R>(harness: &mut TestHarness, read: impl FnOnce(&PlaybackState) -> R) -> R {
    let driver = harness.driver();
    let tree = driver.elements();
    let root = tree
        .iter()
        .into_iter()
        .find(|element| element.debug_name() == "ViewerScreen")
        .expect("the root screen is mounted");
    root.state_as::<PlaybackState, R>(read)
        .expect("the screen owns a playback state")
}

/// Write the mounted playback state — the same channel a control handler
/// writes through, used here to *arrange* a state before testing behaviour.
///
/// Requests a frame afterwards: a state write alone does not wake the loop
/// (a real control handler is invoked from input dispatch, which requests
/// one), so without this the change would sit pending forever and the test
/// would assert on a stale tree.
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
    {
        let mut state = cell.borrow_mut();
        let playback = state
            .as_any_mut()
            .downcast_mut::<PlaybackState>()
            .expect("the screen's state is playback state");
        edit(playback);
    }
    harness.request_frame();
}

/// A pixel count of "content" — mesh, wireframe, motes — in the media area.
///
/// The chamber gradient never rises above blue 36 and the theme's primary is
/// blue 255, so `blue > 60` separates content from chamber with a wide
/// margin on both sides. Sampled every 4th pixel in both axes; the media
/// area is hundreds of pixels tall and wide, so sampling still sees
/// thousands of pixels.
fn content_pixels(harness: &mut TestHarness) -> usize {
    let frame = visual::render(harness.driver(), WINDOW, Color::WHITE);
    let mut content = 0;
    for y in (96..500).step_by(4) {
        for x in (96..384).step_by(4) {
            let (_, _, blue, _) = frame.at(x, y);
            if blue > 60 {
                content += 1;
            }
        }
    }
    content
}

// --------------------------------------------------------------------- tree

#[test]
fn an_unmounted_tree_builds_from_the_initial_snapshot() {
    // `debug_tree` builds the whole screen without mounting it: the widgets
    // fall back to `Snapshot::initial`, which must agree with the state a
    // mount would create. This is also the cheapest proof that every label
    // resolves.
    let tree = debug_tree(WidgetNode::new(ViewerScreen { capture: Rc::new(demo_capture()) }));

    assert!(tree.contains("3 demo capture"), "the title is shown");
    assert!(tree.contains("30 frames"), "the frame count is shown");
    assert!(tree.contains("frame 1 / 30"), "the initial frame is frame 1");
    assert!(tree.contains("0.0 s / 3.0 s"), "the timeline starts at zero");
    assert!(tree.contains("Pause"), "the clock starts playing");
    assert!(tree.contains("Loop on"), "the clock starts looping");
    assert!(tree.contains("Reset view"));
    assert!(tree.contains("ViewerScreen"), "widget names appear in dumps");
}

#[test]
fn a_capture_without_spatial_data_says_so() {
    let mut capture = demo_capture();
    capture.meshes.clear();
    capture.points.clear();
    let tree = debug_tree(WidgetNode::new(ViewerScreen { capture: Rc::new(capture) }));
    assert!(tree.contains("no spatial data"));
    assert!(tree.contains("frame - / 0"), "no frame to count");
}

// ----------------------------------------------------------------- playback

#[test]
fn playback_follows_the_controlled_clock() {
    let mut harness = mounted_viewer();

    // A fifth of a second of wall time passes; the loop draws ~12 vsync
    // frames, each advancing the media clock one frame's worth.
    harness.tick(Duration::from_millis(200));

    let time_ns = read(&mut harness, |state| state.view().time_ns());
    assert!(
        (100_000_000..=220_000_000).contains(&time_ns),
        "200 ms of wall time advances the media clock by roughly 200 ms, got {time_ns} ns"
    );

    // The frame selection follows the clock: the at-or-after rule has
    // already moved on from frame 1.
    let frame = read(&mut harness, |state| state.view().current_mesh_index());
    assert!(frame.unwrap_or(0) >= 2, "the shown frame moved on, got {frame:?}");
}

#[test]
fn playback_loops_at_the_end_of_the_capture() {
    let mut harness = mounted_viewer();

    // 3.5 s of wall time for a 3 s looping capture: the media clock must
    // wrap, not run past the duration.
    harness.tick(Duration::from_millis(3_500));

    let time_ns = read(&mut harness, |state| state.view().time_ns());
    assert!(
        time_ns < 700_000_000,
        "the clock wraps at the 3 s duration, got {time_ns} ns"
    );
    let progress = read(&mut harness, |state| state.view().progress());
    assert!((0.0..0.7).contains(&progress), "progress restarts, got {progress}");
}

#[test]
fn pausing_freezes_the_media_clock() {
    let mut harness = mounted_viewer();
    harness.tick(Duration::from_millis(100));

    // Arrange: pause through the state, as the Pause button's handler does.
    edit(&mut harness, |state| state.set_playing(false));
    pump(&mut harness);
    assert!(!read(&mut harness, |state| state.playing()), "the state is paused");

    let frozen = read(&mut harness, |state| state.view().time_ns());
    // The paused loop sleeps: half a second of "wall time" draws nothing
    // (there is nothing pending), and the media clock must not move.
    harness.tick(Duration::from_millis(500));
    let after = read(&mut harness, |state| state.view().time_ns());
    assert_eq!(frozen, after, "a paused viewer is frozen");

    // And resuming continues from exactly where it stopped, not from where
    // the wall clock went.
    edit(&mut harness, |state| state.set_playing(true));
    pump(&mut harness);
    harness.tick(Duration::from_millis(100));
    let resumed = read(&mut harness, |state| state.view().time_ns());
    assert!(
        (frozen..frozen + 120_000_000).contains(&resumed),
        "resuming continues from {frozen} ns, got {resumed} ns"
    );
}

#[test]
fn the_pause_button_toggles_the_clock() {
    let mut harness = mounted_viewer();
    // Draw the first frames so there is a laid-out tree to hit-test.
    harness.tick(Duration::from_millis(50));

    // The transport row sits at the bottom of the window; the Pause button
    // is its first control. The tap lands well inside both the button's
    // 48 px height and its (now equal-width) slot.
    harness.tap(Offset::new(60.0, 681.0));
    pump(&mut harness);

    assert!(!read(&mut harness, |state| state.playing()), "the tap paused playback");

    // And back.
    harness.tap(Offset::new(60.0, 681.0));
    pump(&mut harness);
    assert!(read(&mut harness, |state| state.playing()), "the tap resumed playback");
}

#[test]
fn scrubbing_the_timeline_selects_the_frame_at_that_time() {
    let mut harness = mounted_viewer();
    edit(&mut harness, |state| state.set_playing(false));
    pump(&mut harness);

    // 90% into a 3 s capture is 2.7 s; the frame at or after 2.7 s is index
    // 27 (frames every 100 ms).
    edit(&mut harness, |state| state.scrub(0.9));
    pump(&mut harness);

    assert_eq!(
        read(&mut harness, |state| state.view().time_ns()),
        2_700_000_000,
        "the media clock reads the scrubbed time exactly"
    );
    assert_eq!(
        read(&mut harness, |state| state.view().current_mesh_index()),
        Some(27),
        "the frame at or after 2.7 s is index 27"
    );
}

// ----------------------------------------------------------------- gestures

#[test]
fn dragging_on_the_surface_orbits_the_camera() {
    let mut harness = mounted_viewer();
    harness.tick(Duration::from_millis(50));

    let yaw_before = read(&mut harness, |state| state.camera().yaw);
    let pitch_before = read(&mut harness, |state| state.camera().pitch);

    // A 100 px leftward drag across the middle of the media surface. The
    // gesture arena eats the first few moves while it disambiguates tap
    // from drag, so the rotation is slightly less than the raw
    // 0.01-rad-per-pixel arithmetic — direction and a wide magnitude band
    // are what a drag can honestly promise.
    harness.drag(
        Offset::new(240.0, 350.0),
        Offset::new(140.0, 350.0),
        10,
    );
    pump(&mut harness);

    let yaw_after = read(&mut harness, |state| state.camera().yaw);
    let pitch_after = read(&mut harness, |state| state.camera().pitch);

    let yaw_delta = yaw_before - yaw_after;
    assert!(
        (0.3..=1.0).contains(&yaw_delta),
        "100 px of drag orbits leftward: before {yaw_before}, after {yaw_after}"
    );
    assert!((pitch_after - pitch_before).abs() < 1e-6, "a horizontal drag does not tilt");
}

#[test]
fn dragging_vertically_tilts_the_camera() {
    let mut harness = mounted_viewer();
    harness.tick(Duration::from_millis(50));

    let pitch_before = read(&mut harness, |state| state.camera().pitch);
    harness.drag(Offset::new(240.0, 350.0), Offset::new(240.0, 300.0), 5);
    pump(&mut harness);

    let pitch_after = read(&mut harness, |state| state.camera().pitch);
    let pitch_delta = pitch_after - pitch_before;
    assert!(
        (-0.5..=-0.1).contains(&pitch_delta),
        "50 px of upward drag lowers the camera: before {pitch_before}, after {pitch_after}"
    );
}

#[test]
fn scrolling_on_the_surface_zooms_the_camera() {
    let mut harness = mounted_viewer();
    harness.tick(Duration::from_millis(50));

    let before = read(&mut harness, |state| state.camera().distance);
    // A wheel's worth of downward scroll.
    harness.scroll(Offset::new(240.0, 350.0), Offset::new(0.0, 120.0));
    pump(&mut harness);

    let after = read(&mut harness, |state| state.camera().distance);
    // 120 px at 0.1% of distance per pixel is 12% closer.
    assert!(
        (after / before - 0.88).abs() < 1e-3,
        "scrolling down 120 px pulls in 12%: before {before}, after {after}"
    );
}

#[test]
fn the_reset_control_refits_the_camera() {
    let mut harness = mounted_viewer();
    harness.tick(Duration::from_millis(50));

    let initial = read(&mut harness, |state| *state.camera());
    edit(&mut harness, |state| state.orbit(300.0, 200.0));
    pump(&mut harness);
    assert_ne!(read(&mut harness, |state| *state.camera()), initial);

    edit(&mut harness, |state| state.reset_camera());
    pump(&mut harness);
    assert_eq!(read(&mut harness, |state| *state.camera()), initial);
}

// ------------------------------------------------------------------- pixels

#[test]
fn the_mesh_lights_up_the_media_area() {
    let mut harness = mounted_viewer();
    harness.tick(Duration::from_millis(100));

    let content = content_pixels(&mut harness);
    assert!(
        content > 200,
        "the capture must cover a visible share of the media area; \
         {content} sampled pixels of content is too few"
    );
}

#[test]
fn orbiting_changes_what_is_drawn() {
    let mut harness = mounted_viewer();
    // Freeze the media clock so *only* the camera moves between renders.
    edit(&mut harness, |state| state.set_playing(false));
    harness.tick(Duration::from_millis(50));

    let before = visual::render(harness.driver(), WINDOW, Color::WHITE);
    harness.drag(Offset::new(240.0, 350.0), Offset::new(120.0, 420.0), 12);
    pump(&mut harness);
    let after = visual::render(harness.driver(), WINDOW, Color::WHITE);

    let differences = before.differences(&after, 10);
    assert!(
        !differences.is_empty(),
        "a 120 px orbit must move pixels on screen"
    );
}

#[test]
fn wireframe_mode_replaces_fills_with_lines() {
    let mut harness = mounted_viewer();
    edit(&mut harness, |state| {
        state.set_playing(false);
        state.toggle_wireframe();
    });
    pump(&mut harness);

    // In wireframe the same geometry draws as 1 px strokes, so the lit pixel
    // count collapses relative to shaded fills — while staying above zero,
    // because the wireframe is drawn from the same in-frustum triangles.
    let wireframe_content = content_pixels(&mut harness);

    edit(&mut harness, |state| state.toggle_wireframe());
    pump(&mut harness);
    let shaded_content = content_pixels(&mut harness);

    assert!(wireframe_content > 0, "the wireframe draws something");
    assert!(
        wireframe_content * 4 < shaded_content,
        "fills cover far more than strokes: wireframe {wireframe_content} vs shaded {shaded_content}"
    );
}
