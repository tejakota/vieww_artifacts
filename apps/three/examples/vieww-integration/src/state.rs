//! Playback state: the `ElementState` that owns the media clock and camera.

use std::rc::Rc;
use std::time::Duration;

use three_core::Capture3D;
use three_vieww::{OrbitCamera, Perspective, ThreeView};
use vieww::widget::ElementState;

/// How much wall time one frame may add to the media clock.
///
/// A window being dragged, a laptop sleeping, a debugger sitting on a
/// breakpoint: all of them stall the frame loop, and when it resumes the
/// naive `now - last` hands playback a multi-second jump. The capture is
/// three seconds long — one stall would skip most of it. Catching up at most
/// a quarter second keeps the timeline honest without lurching.
const MAX_STEP: Duration = Duration::from_millis(250);

/// Where playback starts, and where "reset view" returns to.
const INITIAL_YAW: f32 = 0.6;
const INITIAL_PITCH: f32 = 0.35;

/// The camera every new capture is framed with, shared by the state's
/// constructor and the no-state fallback so both agree by construction.
fn initial_camera(capture: &Capture3D) -> OrbitCamera {
    let mut camera = OrbitCamera {
        yaw: INITIAL_YAW,
        pitch: INITIAL_PITCH,
        distance: 5.0,
        target: three_core::Vec3::new(0.0, 0.0, 0.0),
    };
    if let Some(bounds) = capture.bounds() {
        camera.fit(bounds, Perspective::default().fov_y, 1.15);
    }
    camera
}

/// The durable state of the viewer: media clock, camera, and draw options.
///
/// This lives on the root screen's *element*, so it survives every rebuild —
/// which is the whole point: the widgets above it are descriptions that are
/// thrown away constantly, while the playback position, the orbit the user
/// dragged, and the wireframe toggle persist.
///
/// Two things drive it:
///
/// * **The frame clock** — [`ElementState::tick`] advances the media time by
///   the wall delta while playing. Returning `true` marks the element for
///   rebuild, which is how the next frame of the capture reaches the screen.
///   [`ElementState::is_animating`] answers `playing`, so a paused viewer
///   costs the loop nothing.
/// * **Input** — gesture and control handlers reach this state through the
///   shared [`StateHandle`](crate::StateHandle) and set the pending flag;
///   [`ElementState::take_pending`] turns that into a rebuild.
#[derive(Debug)]
pub struct PlaybackState {
    view: ThreeView,
    camera: OrbitCamera,
    playing: bool,
    wireframe: bool,
    show_points: bool,
    last_now: Option<Duration>,
    pending: bool,
    /// The previous pinch scale, so one scale gesture becomes incremental
    /// zoom steps rather than re-applying the total spread every update.
    pinch_anchor: f32,
}

impl PlaybackState {
    /// A state that starts playing the capture on loop, camera auto-fitted.
    pub fn new(capture: Rc<Capture3D>) -> Self {
        let camera = initial_camera(&capture);
        Self {
            view: ThreeView::from_shared(capture),
            camera,
            playing: true,
            wireframe: false,
            show_points: true,
            last_now: None,
            pending: false,
            pinch_anchor: 1.0,
        }
    }

    // -- reads, for a build taking a snapshot ------------------------------

    /// The media controller.
    pub fn view(&self) -> &ThreeView {
        &self.view
    }

    /// The orbit camera.
    pub fn camera(&self) -> &OrbitCamera {
        &self.camera
    }

    /// Whether the media clock is running.
    pub fn playing(&self) -> bool {
        self.playing
    }

    /// Whether the mesh is drawn as a wireframe.
    pub fn wireframe(&self) -> bool {
        self.wireframe
    }

    /// Whether the point motes are drawn.
    pub fn show_points(&self) -> bool {
        self.show_points
    }

    // -- writes, from handlers ----------------------------------------------

    /// Play or pause the media clock.
    pub fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
        self.pending = true;
    }

    /// Toggle the media clock.
    pub fn toggle_play(&mut self) {
        self.set_playing(!self.playing);
    }

    /// Set looping; turning it on resumes a capture paused at its end.
    pub fn set_looping(&mut self, looping: bool) {
        self.view.set_looping(looping);
        self.pending = true;
    }

    /// Toggle wireframe rendering.
    pub fn toggle_wireframe(&mut self) {
        self.wireframe = !self.wireframe;
        self.pending = true;
    }

    /// Toggle the point motes.
    pub fn toggle_points(&mut self) {
        self.show_points = !self.show_points;
        self.pending = true;
    }

    /// Drag-orbit the camera, in pixels.
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.camera.drag(dx, dy);
        self.pending = true;
    }

    /// Wheel/trackpad zoom, in pixels of travel.
    pub fn zoom(&mut self, dy: f32) {
        // A wheel notch is ~100 px; a comfortable step is ~10% of the
        // distance, hence the 0.001 gain.
        self.camera.zoom(dy * 0.001 * self.camera.distance);
        self.pending = true;
    }

    /// One incremental pinch step: `scale` is the gesture's current spread
    /// relative to its start.
    pub fn pinch_step(&mut self, scale: f32) {
        if scale.is_finite() && scale > 0.0 && self.pinch_anchor > 0.0 {
            self.camera.pinch(scale / self.pinch_anchor);
        }
        self.pinch_anchor = scale;
        self.pending = true;
    }

    /// A pinch gesture ended; the next one starts a fresh anchor.
    pub fn pinch_end(&mut self) {
        self.pinch_anchor = 1.0;
    }

    /// Scrub to a fraction of the duration.
    pub fn scrub(&mut self, progress: f32) {
        self.view.set_progress(progress);
        self.pending = true;
    }

    /// Refit the camera to the capture, keeping the user's viewing angle.
    pub fn reset_camera(&mut self) {
        self.camera = initial_camera(self.view.capture());
        self.pending = true;
    }

    /// Advance the media clock from a wall-clock step, applying the
    /// [`MAX_STEP`] catch-up cap.
    fn advance(&mut self, step: Duration) {
        let capped = step.min(MAX_STEP);
        self.view.tick(capped.as_nanos() as u64);
    }
}

impl ElementState for PlaybackState {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn tick(&mut self, now: Duration) -> bool {
        let step = self.last_now.map(|last| now.saturating_sub(last));
        self.last_now = Some(now);
        match (self.playing, step) {
            // The first frame after mount has no delta to advance from; the
            // media clock starts at 0 and the *next* frame moves it.
            (true, Some(step)) => {
                self.advance(step);
                true
            }
            (true, None) => true,
            (false, _) => false,
        }
    }

    /// Only while playing: a paused viewer must let the loop sleep, the same
    /// discipline an indeterminate progress bar pays for its animation.
    fn is_animating(&self) -> bool {
        self.playing
    }

    fn take_pending(&mut self) -> bool {
        std::mem::take(&mut self.pending)
    }
}

/// An immutable read of the playback state, taken during a build.
///
/// Widgets are cheap descriptions that hold nothing durable, so the root's
/// build takes one of these and hands copies of the relevant fields down to
/// the media view and the controls. It is *not* shared state: it is the value
/// the state had at build time, and the next rebuild takes a fresh one.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub capture: Rc<Capture3D>,
    pub mesh_index: Option<usize>,
    pub points_index: Option<usize>,
    pub progress: f32,
    pub time_seconds: f32,
    pub duration_seconds: f32,
    pub frame_count: usize,
    pub playing: bool,
    pub looping: bool,
    pub wireframe: bool,
    pub show_points: bool,
    pub camera: OrbitCamera,
}

impl Snapshot {
    /// Read the live state during a build.
    pub fn from_state(state: &PlaybackState) -> Self {
        let view = state.view();
        let nanos_per_second = 1_000_000_000.0;
        Self {
            mesh_index: view.current_mesh_index(),
            points_index: view.current_points_index(),
            progress: view.progress(),
            time_seconds: view.time_ns() as f32 / nanos_per_second,
            duration_seconds: view.capture().duration_ns as f32 / nanos_per_second,
            frame_count: view.mesh_frame_count(),
            playing: state.playing(),
            looping: view.is_looping(),
            wireframe: state.wireframe(),
            show_points: state.show_points(),
            camera: *state.camera(),
            capture: view.shared_capture(),
        }
    }

    /// The state a capture would show before its element exists — the
    /// fallback an unmounted tree (a `debug_tree` dump, an error placeholder)
    /// builds from.
    ///
    /// Must agree with [`PlaybackState::new`]'s initial camera; the test
    /// `an_unmounted_snapshot_matches_initial_state` keeps the two honest.
    pub fn initial(capture: &Rc<Capture3D>) -> Self {
        Self {
            mesh_index: (!capture.meshes.is_empty()).then_some(0),
            points_index: (!capture.points.is_empty()).then_some(0),
            progress: 0.0,
            time_seconds: 0.0,
            duration_seconds: capture.duration_ns as f32 / 1_000_000_000.0,
            frame_count: capture.meshes.len(),
            playing: true,
            looping: true,
            wireframe: false,
            show_points: true,
            camera: initial_camera(capture),
            capture: Rc::clone(capture),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use three_core::{CaptureSource, MeshFrame, TimestampNs, Vec3};

    fn small_capture() -> Capture3D {
        Capture3D {
            duration_ns: 1_000_000_000,
            source: CaptureSource::Synthetic,
            cameras: vec![],
            depth: vec![],
            meshes: vec![
                MeshFrame {
                    timestamp: TimestampNs(0),
                    vertices: vec![Vec3::new(-1.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)],
                    indices: vec![0, 1, 2],
                },
                MeshFrame {
                    timestamp: TimestampNs(1_000_000_000),
                    vertices: vec![Vec3::new(-2.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 0.0), Vec3::new(0.0, 2.0, 0.0)],
                    indices: vec![0, 1, 2],
                },
            ],
            points: vec![],
            title: "small".into(),
        }
    }

    #[test]
    fn the_clock_advances_only_while_playing() {
        let mut state = PlaybackState::new(Rc::new(small_capture()));

        // First tick: no delta, but playing, so the element rebuilds.
        assert!(state.tick(Duration::from_millis(0)));
        state.tick(Duration::from_millis(100));
        assert_eq!(state.view().time_ns(), 100_000_000);

        state.set_playing(false);
        state.tick(Duration::from_millis(400));
        assert_eq!(state.view().time_ns(), 100_000_000, "a paused clock is frozen");
    }

    #[test]
    fn a_stall_catches_up_at_most_a_quarter_second() {
        let mut state = PlaybackState::new(Rc::new(small_capture()));
        state.tick(Duration::from_millis(0));
        // Five seconds of "window was dragged"...
        state.tick(Duration::from_millis(5_000));
        // ...advances the media by 250 ms, not 5 s.
        assert_eq!(state.view().time_ns(), 250_000_000);
    }

    #[test]
    fn writes_mark_the_element_pending() {
        let mut state = PlaybackState::new(Rc::new(small_capture()));
        assert!(!state.take_pending(), "a fresh state has nothing pending");

        state.toggle_wireframe();
        assert!(state.take_pending());
        state.orbit(10.0, 0.0);
        assert!(state.take_pending());
        state.scrub(0.5);
        assert_eq!(state.view().time_ns(), 500_000_000);
        assert!(state.take_pending());
        // Taking the flag clears it.
        assert!(!state.take_pending());
    }

    #[test]
    fn pinch_steps_are_incremental() {
        let mut state = PlaybackState::new(Rc::new(small_capture()));
        let start = state.camera().distance;
        state.pinch_step(2.0); // spread doubled: halfway in
        let halfway = state.camera().distance;
        assert!(halfway < start, "spreading moves the camera in");
        state.pinch_step(2.0); // the same total spread again: no further zoom
        assert!(
            (state.camera().distance - halfway).abs() < 1e-6,
            "re-reporting the same scale must not zoom again"
        );
        state.pinch_end();
        state.pinch_step(1.0); // a fresh gesture at rest zooms nothing
        assert!((state.camera().distance - halfway).abs() < 1e-6);
    }

    #[test]
    fn scrubbing_then_playing_continues_from_there() {
        let mut state = PlaybackState::new(Rc::new(small_capture()));
        state.scrub(0.75);
        state.tick(Duration::from_millis(0));
        state.tick(Duration::from_millis(100));
        assert_eq!(state.view().time_ns(), 850_000_000);
    }

    #[test]
    fn an_unmounted_snapshot_matches_initial_state() {
        let capture = Rc::new(small_capture());
        let from_state = Snapshot::from_state(&PlaybackState::new(Rc::clone(&capture)));
        let initial = Snapshot::initial(&capture);

        assert_eq!(from_state.camera, initial.camera);
        assert_eq!(from_state.playing, initial.playing);
        assert_eq!(from_state.wireframe, initial.wireframe);
        assert_eq!(from_state.show_points, initial.show_points);
        assert_eq!(from_state.mesh_index, initial.mesh_index);
        assert_eq!(from_state.progress, initial.progress);
    }

    #[test]
    fn reset_camera_returns_to_the_initial_framing() {
        let capture = Rc::new(small_capture());
        let mut state = PlaybackState::new(Rc::clone(&capture));
        let before = *state.camera();
        state.orbit(300.0, 200.0);
        state.zoom(2.0);
        assert_ne!(*state.camera(), before);
        state.reset_camera();
        assert_eq!(*state.camera(), before);
    }
}
