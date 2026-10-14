//! Playback state: the `ElementState` that owns the view, the clock and the
//! camera.

use std::rc::Rc;
use std::time::Duration;

use three_format::LazyCapture;
use three_vieww::{DepthView, RenderedFrame};
use vieww::widget::ElementState;

/// How much wall time one frame may add to the media clock.
///
/// A window being dragged, a laptop sleeping, a debugger sitting on a
/// breakpoint: all of them stall the frame loop, and when it resumes the
/// naive `now - last` hands playback a multi-second jump. The capture is
/// three seconds long — one stall would skip most of it. Catching up at most
/// a quarter second keeps the timeline honest without lurching.
const MAX_STEP: Duration = Duration::from_millis(250);

/// The durable state of the viewer: the depth view, and the rendered frame
/// its last tick produced.
///
/// This lives on the root screen's *element*, so it survives every rebuild —
/// which is the whole point: the widgets above it are descriptions that are
/// thrown away constantly, while the media clock, the orbit the user
/// dragged, and the pixels currently on screen persist.
///
/// Two things drive it:
///
/// * **The frame clock** — [`ElementState::tick`] advances the media time,
///   steps the recenter spring, renders, and returns `true` while anything
///   is moving. A paused, neutral viewer renders nothing and lets the loop
///   sleep.
/// * **Input** — gesture and control handlers reach this state through the
///   shared [`StateHandle`](crate::widgets::StateHandle) and set the pending
///   flag; [`ElementState::take_pending`] turns that into a rebuild.
impl std::fmt::Debug for PlaybackState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Not the view's own Debug — a dump wants identity and position,
        // and the pixels would drown it. The type is what a dump needs.
        f.debug_struct("PlaybackState")
            .field("playing", &self.playing)
            .field("rendered", &self.rendered)
            .field("pinch_anchor", &self.pinch_anchor)
            .finish()
    }
}

/// The durable state of the viewer: the depth view, and the rendered frame
/// its last tick produced.
///
/// This lives on the root screen's *element*, so it survives every rebuild —
/// which is the whole point: the widgets above it are descriptions that are
/// thrown away constantly, while the media clock, the orbit the user
/// dragged, and the pixels currently on screen persist.
///
/// Two things drive it:
///
/// * **The frame clock** — [`ElementState::tick`] advances the media time,
///   steps the recenter spring, renders, and returns `true` while anything
///   is moving. A paused, neutral viewer renders nothing and lets the loop
///   sleep.
/// * **Input** — gesture and control handlers reach this state through the
///   shared [`StateHandle`](crate::widgets::StateHandle) and set the pending
///   flag; [`ElementState::take_pending`] turns that into a rebuild.
pub struct PlaybackState {
    view: DepthView,
    playing: bool,
    rendered: RenderedFrame,
    last_now: Option<Duration>,
    pending: bool,
    /// The previous pinch scale, so one scale gesture becomes incremental
    /// dolly steps rather than re-applying the total spread every update.
    pinch_anchor: f32,
}

impl PlaybackState {
    /// A state that starts playing the capture on loop from its own first
    /// frame.
    pub fn new(capture: Rc<LazyCapture>) -> Self {
        let mut view = DepthView::new(capture);
        view.player_mut().play();
        let rendered = view.render(Duration::ZERO).unwrap_or_else(|| {
            RenderedFrame::blank(view.meta().intrinsics.width, view.meta().intrinsics.height)
        });
        Self {
            view,
            playing: true,
            rendered,
            last_now: None,
            pending: true,
            pinch_anchor: 1.0,
        }
    }

    // -- reads, for a build taking a snapshot ------------------------------

    /// The media view (player + camera).
    pub fn view(&self) -> &DepthView {
        &self.view
    }

    /// The pixels to show right now.
    pub fn rendered(&self) -> &RenderedFrame {
        &self.rendered
    }

    /// Whether the media clock is running.
    pub fn playing(&self) -> bool {
        self.playing
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
        self.view.player_mut().set_looping(looping);
        self.pending = true;
    }

    /// Pan: orbit the camera, in pixels.
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.view.camera_mut().drag(dx, dy);
        self.pending = true;
    }

    /// Wheel/trackpad zoom, in pixels of travel.
    pub fn zoom(&mut self, dy: f32) {
        // A wheel notch is ~100 px; a comfortable step is ~10% of the
        // dolly, hence the 0.001 gain.
        self.view.camera_mut().pinch(1.0 - dy * 0.001);
        self.pending = true;
    }

    /// One incremental pinch step: `scale` is the gesture's current spread
    /// relative to its start.
    pub fn pinch_step(&mut self, scale: f32) {
        if scale.is_finite() && scale > 0.0 && self.pinch_anchor > 0.0 {
            self.view.camera_mut().pinch(scale / self.pinch_anchor);
        }
        self.pinch_anchor = scale;
        self.pending = true;
    }

    /// A pinch gesture ended; the next one starts a fresh anchor.
    pub fn pinch_end(&mut self) {
        self.pinch_anchor = 1.0;
    }

    /// **Hold**: start the recenter spring.
    pub fn hold(&mut self) {
        self.view.camera_mut().begin_recentre();
        self.pending = true;
    }

    /// Scrub to a fraction of the duration.
    pub fn scrub(&mut self, progress: f32) {
        self.view.player_mut().seek(progress);
        self.pending = true;
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
        let step = self
            .last_now
            .map(|last| now.saturating_sub(last))
            .unwrap_or(Duration::ZERO);
        self.last_now = Some(now);
        let capped = step.min(MAX_STEP);
        let moving = self.playing || self.view.animating();
        if moving {
            if self.playing {
                self.view.player_mut().advance(capped);
            }
            if let Some(frame) = self.view.render(capped) {
                self.rendered = frame;
            }
        } else if self.pending {
            // A write without motion still needs one render to show itself:
            // a scrub while paused, a loop toggle, a hold that lands instantly.
            if let Some(frame) = self.view.render(Duration::ZERO) {
                self.rendered = frame;
            }
        }
        moving
    }

    /// While playing, or while the recenter spring settles — a paused,
    /// neutral viewer must let the loop sleep, the same discipline an
    /// indeterminate progress bar pays for its animation.
    fn is_animating(&self) -> bool {
        self.playing || self.view.animating()
    }

    fn take_pending(&mut self) -> bool {
        std::mem::take(&mut self.pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use three_format::encode;

    fn small_view() -> Rc<LazyCapture> {
        let capture = three_runtime::sample_with_size(32, 24, 10, Duration::from_secs(1));
        Rc::new(LazyCapture::open(encode(&capture).unwrap()).unwrap())
    }

    #[test]
    fn the_clock_advances_only_while_playing() {
        let mut state = PlaybackState::new(small_view());

        assert!(state.tick(Duration::from_millis(0)));
        state.tick(Duration::from_millis(100));
        assert_eq!(state.view().player().time_ns(), 100_000_000);

        state.set_playing(false);
        state.tick(Duration::from_millis(400));
        assert_eq!(
            state.view().player().time_ns(),
            100_000_000,
            "a paused clock is frozen"
        );
    }

    #[test]
    fn a_stall_catches_up_at_most_a_quarter_second() {
        let mut state = PlaybackState::new(small_view());
        state.tick(Duration::from_millis(0));
        // Five seconds of "window was dragged"...
        state.tick(Duration::from_millis(5_000));
        // ...advances the media by 250 ms, not 5 s.
        assert_eq!(state.view().player().time_ns(), 250_000_000);
    }

    #[test]
    fn writes_mark_the_element_pending() {
        let mut state = PlaybackState::new(small_view());
        let _ = state.take_pending(); // the constructor's own pending

        assert!(!state.take_pending(), "a settled state has nothing pending");

        state.orbit(10.0, 0.0);
        assert!(state.take_pending());
        state.scrub(0.5);
        assert_eq!(state.view().player().time_ns(), 500_000_000);
        assert!(state.take_pending());
        assert!(!state.take_pending());
    }

    #[test]
    fn pinch_steps_are_incremental() {
        let mut state = PlaybackState::new(small_view());
        let start = state.view().camera().dolly;
        state.pinch_step(2.0); // spread doubled: closer
        let closer = state.view().camera().dolly;
        assert!(closer < start, "spreading moves the camera in");
        state.pinch_step(2.0); // the same total spread again: no further zoom
        assert!(
            (state.view().camera().dolly - closer).abs() < 1e-6,
            "re-reporting the same scale must not zoom again"
        );
        state.pinch_end();
        state.pinch_step(1.0); // a fresh gesture at rest zooms nothing
        assert!((state.view().camera().dolly - closer).abs() < 1e-6);
    }

    #[test]
    fn hold_springs_the_camera_home() {
        let mut state = PlaybackState::new(small_view());
        state.orbit(300.0, 200.0);
        state.pinch_step(1.8);
        assert!(!state.view().camera().is_neutral());

        state.hold();
        // The spring is animating, so ticks render until it lands. The
        // clock a real frame loop owns accumulates; feeding the state
        // non-accumulating "16ms" stamps would make every step zero.
        let mut ticks = 0;
        while ticks < 600 {
            state.tick(Duration::from_millis(16 * (ticks + 1)));
            ticks += 1;
            if state.view().camera().is_neutral() {
                break;
            }
        }
        assert!(
            state.view().camera().is_neutral(),
            "hold recentres (in {ticks} ticks)"
        );
        // ...and what it returns to is the frame itself.
        let (playing, _moving) = (state.playing(), false);
        let _ = playing;
    }

    #[test]
    fn scrubbing_then_playing_continues_from_there() {
        let mut state = PlaybackState::new(small_view());
        state.scrub(0.75);
        state.tick(Duration::from_millis(0));
        state.tick(Duration::from_millis(100));
        assert_eq!(state.view().player().time_ns(), 850_000_000);
    }

    #[test]
    fn a_scrub_while_paused_shows_the_scrubbed_frame() {
        let mut state = PlaybackState::new(small_view());
        state.tick(Duration::from_millis(0));
        state.set_playing(false);
        state.tick(Duration::from_millis(50));
        let before = state.rendered().rgba.clone();
        state.scrub(0.9);
        state.tick(Duration::from_millis(60));
        // The synthetic pattern differs per frame, so the pixels differ.
        assert_ne!(
            state.rendered().rgba,
            before,
            "a scrub while paused re-renders"
        );
    }
}
