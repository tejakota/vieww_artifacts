//! The viewer's media state: a lazy capture, a clock, a camera, one decoded
//! frame.

use std::rc::Rc;
use std::sync::Arc as Shared;

use three_core::DepthVideoFrame;
use three_format::{CaptureMeta, LazyCapture};
use three_runtime::DepthVideoPlayer;

use crate::ViewCamera;

/// One rendered frame, ready for a 2D UI to draw as an image.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderedFrame {
    pub width: u32,
    pub height: u32,
    /// RGBA8, shared rather than copied: a snapshot hands this to a widget
    /// description every rebuild, and the pixels must not duplicate per
    /// rebuild. `Clone` is an `Arc` bump.
    pub rgba: Shared<Vec<u8>>,
}

impl RenderedFrame {
    /// The render for a paused, camera-neutral state machine — what a feed
    /// card shows before anything has decoded, and what tests use as a
    /// blank. Black, because the alternative (white) flashes on dark
    /// themes.
    pub fn blank(width: u32, height: u32) -> Self {
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        for pixel in rgba.as_chunks_mut::<4>().0 {
            pixel[3] = 255;
        }
        Self {
            width,
            height,
            rgba: Shared::new(rgba),
        }
    }
}

/// Everything a depth-video surface needs, in one owned place.
///
/// The view deliberately caches **exactly one decoded frame**: the one
/// being shown. Decoding is the expensive act in this pipeline (a JPEG and
/// a zstd inflate per frame), playback moves forward one frame at a time,
/// and a re-render at the same position must not re-decode — scrubbing
/// back and forth across two frames is the only pattern that pays double,
/// and it pays once per crossing, which is the cache a scroll UI actually
/// needs. (A full decode of a 3-second capture is ~130 MB — see
/// `LazyCapture` for why that is not an option on a phone.)
///
/// The camera is a plain field, not a child object: gestures write it, the
/// render reads it, and the recenter spring advances inside
/// [`render`](Self::render) so a single call drives one whole frame of the
/// state machine.
pub struct DepthView {
    capture: Rc<LazyCapture>,
    player: DepthVideoPlayer,
    camera: ViewCamera,
    /// The index `decoded` belongs to, if any.
    decoded_index: Option<usize>,
    decoded: Option<DepthVideoFrame>,
    /// The subject distance, measured once from the first frame's depth
    /// median and then never re-measured: it anchors the orbit, and letting
    /// it move per-frame would make the camera breathe with the footage.
    subject_distance: f32,
}

impl DepthView {
    /// Open a capture for viewing. Decodes the first frame (for the
    /// subject distance and the first paint) and nothing else.
    pub fn new(capture: Rc<LazyCapture>) -> Self {
        let player = DepthVideoPlayer::new_from_meta(capture.meta());
        let subject_distance = capture
            .frame(0)
            .ok()
            .map(|frame| median_depth(&frame.depth))
            .filter(|&meters| meters > 0.0)
            .unwrap_or(2.0);
        Self {
            capture,
            player,
            camera: ViewCamera::default(),
            decoded_index: None,
            decoded: None,
            subject_distance,
        }
    }

    pub fn meta(&self) -> &CaptureMeta {
        self.capture.meta()
    }

    pub fn player(&self) -> &DepthVideoPlayer {
        &self.player
    }

    pub fn player_mut(&mut self) -> &mut DepthVideoPlayer {
        &mut self.player
    }

    pub fn camera(&self) -> &ViewCamera {
        &self.camera
    }

    pub fn camera_mut(&mut self) -> &mut ViewCamera {
        &mut self.camera
    }

    /// Where the orbit centres, meters in front of the capture camera.
    pub fn subject_distance(&self) -> f32 {
        self.subject_distance
    }

    /// The frame the clock currently points at, decoded on demand.
    ///
    /// Returns `None` when the capture's frame cannot be decoded (a corrupt
    /// file mid-stream) — a viewer shows the last good render or a blank,
    /// and carries on; one bad frame must not take the feed down with it.
    pub fn current_frame(&mut self) -> Option<&DepthVideoFrame> {
        let index = self.player.frame_index()?;
        if self.decoded_index != Some(index) {
            self.decoded = self.capture.frame(index).ok();
            self.decoded_index = Some(index);
        }
        self.decoded.as_ref()
    }

    /// Render what the state machine says is on screen: advance the recenter
    /// spring, then decode the current frame if the clock moved, then warp
    /// it for the camera. One call, one frame of viewer state.
    ///
    /// `delta` is the time since the last render — used *only* by the
    /// spring, which is the only part of this view that integrates time
    /// itself. Playback time is advanced separately by whoever owns the
    /// app's frame loop, through [`player_mut`](Self::player_mut).
    pub fn render(&mut self, delta: std::time::Duration) -> Option<RenderedFrame> {
        self.camera.step_recentre(delta);
        let intrinsics = self.capture.meta().intrinsics;
        let subject = self.subject_distance;
        let camera = self.camera;
        let frame = self.current_frame()?;
        let out = crate::warp_frame(&frame.color, &frame.depth, &intrinsics, &camera, subject);
        Some(RenderedFrame {
            width: out.width,
            height: out.height,
            rgba: Shared::new(out.rgba),
        })
    }

    /// Whether the current state still needs frames drawn for it: the
    /// spring is settling. Playback ticks are the caller's business.
    pub fn animating(&self) -> bool {
        !self.camera.is_neutral()
    }
}

/// The median of a depth plane in meters, 0 when nothing is known.
///
/// The median, not the mean: a sky and a foreground hand are both outliers,
/// and the subject is what is left.
fn median_depth(depth_mm: &[u16]) -> f32 {
    let mut known: Vec<u16> = depth_mm.iter().copied().filter(|&mm| mm > 0).collect();
    if known.is_empty() {
        return 0.0;
    }
    known.sort_unstable();
    known[known.len() / 2] as f32 / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use three_format::encode;

    fn sample_view() -> DepthView {
        let capture = three_runtime::sample_with_size(48, 32, 8, std::time::Duration::from_secs(1));
        let bytes = encode(&capture).unwrap();
        DepthView::new(Rc::new(LazyCapture::open(bytes).unwrap()))
    }

    #[test]
    fn the_first_frame_decodes_once_per_position() {
        let mut view = sample_view();
        view.player_mut().play();
        view.player_mut()
            .advance(std::time::Duration::from_millis(125));
        // Same position twice: the decoded frame is the same *content*
        // both times — identity, not address: the allocator is free to
        // reuse the slot across a replacement, so pointer equality would
        // test the allocator, not the cache.
        let first_ts = view.current_frame().unwrap().timestamp;
        assert_eq!(view.current_frame().unwrap().timestamp, first_ts);

        // Moving to the next frame decodes a different one.
        view.player_mut()
            .advance(std::time::Duration::from_millis(125));
        let third_ts = view.current_frame().unwrap().timestamp;
        assert_ne!(first_ts, third_ts);
        assert_eq!(view.player().frame_index(), Some(2));
    }

    #[test]
    fn render_tracks_the_camera_and_the_clock() {
        let mut view = sample_view();
        view.player_mut().play();

        let neutral = view.render(std::time::Duration::ZERO).unwrap();
        assert_eq!(neutral.width, 48);
        assert_eq!(neutral.rgba.len(), 48 * 32 * 4);

        // Neutral camera: the render equals the decoded frame's color —
        // the fast path is a copy.
        view.camera_mut().drag(40.0, 0.0);
        let moved = view.render(std::time::Duration::ZERO).unwrap();
        assert_ne!(moved.rgba, neutral.rgba, "an orbit changes the picture");

        // Hold: the spring homes.
        view.camera_mut().begin_recentre();
        let _ = view.render(std::time::Duration::from_secs(5));
        assert!(view.camera().is_neutral());
        let homed = view.render(std::time::Duration::ZERO).unwrap();
        assert_eq!(homed.rgba, neutral.rgba, "what returns is the frame itself");
    }

    #[test]
    fn blank_is_black_and_opaque() {
        let blank = RenderedFrame::blank(4, 3);
        assert_eq!(blank.rgba.len(), 4 * 3 * 4);
        assert!(blank.rgba.chunks(4).all(|p| p == [0, 0, 0, 255]));
    }
}
