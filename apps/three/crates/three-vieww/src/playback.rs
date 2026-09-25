//! Playback state for a temporal 3D asset.

use std::rc::Rc;
use three_core::{Capture3D, MeshFrame, PointFrame};

/// A Vieww-friendly controller for a temporal 3D asset.
///
/// Owns the decoded capture behind an [`Rc`] so the widget that displays it,
/// the painter that draws it, and this controller can all share one copy —
/// a capture can carry megabytes of mesh data, and cloning it per rebuild
/// would make playback stutter for no gain.
///
/// Time is an integer count of nanoseconds since the capture started, the
/// same units `three-core` timestamps use, so no conversion sits between the
/// media clock and the frame timestamps.
pub struct ThreeView {
    capture: Rc<Capture3D>,
    time_ns: u64,
    looping: bool,
}

impl std::fmt::Debug for ThreeView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Not the capture's own Debug: its geometry would drown any tree
        // dump this appears in. Identity and position are what a dump wants.
        f.debug_struct("ThreeView")
            .field("title", &self.capture.title)
            .field("time_ns", &self.time_ns)
            .field("looping", &self.looping)
            .finish()
    }
}

impl ThreeView {
    pub fn new(capture: Capture3D) -> Self {
        Self { capture: Rc::new(capture), time_ns: 0, looping: true }
    }

    /// Build a view over a capture that is already shared — for an
    /// application state that hands the same [`Rc`] to a painter, a feed list
    /// and a player without any of them copying the geometry.
    pub fn from_shared(capture: Rc<Capture3D>) -> Self {
        Self { capture, time_ns: 0, looping: true }
    }

    /// The capture being played.
    pub fn capture(&self) -> &Capture3D {
        &self.capture
    }

    /// A shared handle to the capture, for a painter that draws at paint time
    /// rather than holding a clone.
    pub fn shared_capture(&self) -> Rc<Capture3D> {
        Rc::clone(&self.capture)
    }

    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }

    pub fn is_looping(&self) -> bool {
        self.looping
    }

    pub fn set_time_ns(&mut self, time_ns: u64) {
        self.time_ns = time_ns.min(self.capture.duration_ns);
    }

    pub fn time_ns(&self) -> u64 {
        self.time_ns
    }

    /// How far through the capture, `0.0 ..= 1.0`.
    pub fn progress(&self) -> f32 {
        if self.capture.duration_ns == 0 {
            return 0.0;
        }
        self.time_ns as f32 / self.capture.duration_ns as f32
    }

    /// Jump to a fraction of the duration, as reported by [`progress`](Self::progress).
    pub fn set_progress(&mut self, progress: f32) {
        self.set_time_ns((progress.clamp(0.0, 1.0) * self.capture.duration_ns as f32) as u64);
    }

    /// Advance playback by `delta_ns`.
    ///
    /// While looping, time wraps at the duration. While not looping, it stops
    /// at the end and [`is_finished`](Self::is_finished) reports true — a
    /// paused-at-end frame the user can scrub back from, rather than a time
    /// that keeps growing past every frame timestamp.
    pub fn tick(&mut self, delta_ns: u64) {
        if self.capture.duration_ns == 0 {
            return;
        }
        if self.looping {
            self.time_ns = self.time_ns.saturating_add(delta_ns) % self.capture.duration_ns;
        } else {
            self.time_ns = self.time_ns.saturating_add(delta_ns).min(self.capture.duration_ns);
        }
    }

    /// Whether playback has run to the end without looping.
    pub fn is_finished(&self) -> bool {
        !self.looping && self.capture.duration_ns > 0 && self.time_ns >= self.capture.duration_ns
    }

    /// How many mesh frames the capture carries.
    pub fn mesh_frame_count(&self) -> usize {
        self.capture.meshes.len()
    }

    /// The mesh frame at or after the current time, or the last frame when
    /// the time is past every timestamp.
    ///
    /// This matches `three_runtime::seek_mesh`: "at or after" is the frame a
    /// player would **next show** at this time — the same rule seeking a
    /// keyframe-coded stream uses, since the frame before the seek point
    /// cannot be decoded without its keyframe. A time past every timestamp
    /// falls back to the final frame so the end of a capture never shows
    /// nothing.
    pub fn current_mesh(&self) -> Option<&MeshFrame> {
        self.mesh_at_ns(self.time_ns)
    }

    /// [`current_mesh`](Self::current_mesh) at an arbitrary timestamp.
    pub fn mesh_at_ns(&self, time_ns: u64) -> Option<&MeshFrame> {
        self.capture
            .meshes
            .iter()
            .find(|m| m.timestamp.0 >= time_ns)
            .or_else(|| self.capture.meshes.last())
    }

    /// The index [`current_mesh`](Self::current_mesh) would return.
    pub fn current_mesh_index(&self) -> Option<usize> {
        self.capture
            .meshes
            .iter()
            .position(|m| m.timestamp.0 >= self.time_ns)
            .or(Some(self.capture.meshes.len().wrapping_sub(1)))
            .filter(|_| !self.capture.meshes.is_empty())
    }

    /// The index [`current_points`](Self::current_points) would return.
    pub fn current_points_index(&self) -> Option<usize> {
        self.capture
            .points
            .iter()
            .position(|p| p.timestamp.0 >= self.time_ns)
            .or(Some(self.capture.points.len().wrapping_sub(1)))
            .filter(|_| !self.capture.points.is_empty())
    }

    /// The nearest point frame at or after the current time, or the last.
    pub fn current_points(&self) -> Option<&PointFrame> {
        self.capture
            .points
            .iter()
            .find(|p| p.timestamp.0 >= self.time_ns)
            .or_else(|| self.capture.points.last())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::wave_capture;
    use three_core::TimestampNs;

    #[test]
    fn ticking_loops_at_the_duration() {
        let mut view = ThreeView::new(wave_capture());
        view.tick(600_000_000);
        assert_eq!(view.time_ns(), 600_000_000);
        view.tick(600_000_000);
        assert_eq!(view.time_ns(), 200_000_000, "1.2s wraps at 1s");
        assert!((view.progress() - 0.2).abs() < 1e-6);
    }

    #[test]
    fn ticking_without_looping_stops_at_the_end() {
        let mut view = ThreeView::new(wave_capture());
        view.set_looping(false);
        view.tick(5_000_000_000);
        assert_eq!(view.time_ns(), 1_000_000_000);
        assert!(view.is_finished());
    }

    #[test]
    fn looping_is_never_finished() {
        let mut view = ThreeView::new(wave_capture());
        view.tick(5_000_000_000);
        assert!(!view.is_finished());
    }

    #[test]
    fn the_next_frame_at_or_after_the_time_wins() {
        let view = ThreeView::new(wave_capture());
        // Frames start at 0s and 1s (the duration). At t=0 the frame at 0s
        // is the next one at or after the time...
        assert_eq!(view.current_mesh_index(), Some(0));
        // ...and at 999ms the at-or-after rule has already moved to the
        // frame that starts at 1s, the same rule `seek_mesh` applies.
        let mut late = ThreeView::new(wave_capture());
        late.set_time_ns(999_999_999);
        assert_eq!(late.current_mesh_index(), Some(1));
        late.set_time_ns(1_000_000_000);
        assert_eq!(late.current_mesh_index(), Some(1));
    }

    #[test]
    fn a_time_past_every_frame_shows_the_last_frame() {
        let mut view = ThreeView::new(wave_capture());
        view.set_looping(false);
        view.tick(5_000_000_000);
        let mesh = view.current_mesh().expect("frames exist");
        assert_eq!(mesh.timestamp, TimestampNs(1_000_000_000));
        assert_eq!(view.current_mesh_index(), Some(1));
    }

    #[test]
    fn set_time_is_clamped_and_progress_round_trips() {
        let mut view = ThreeView::new(wave_capture());
        view.set_time_ns(u64::MAX);
        assert_eq!(view.time_ns(), 1_000_000_000);
        view.set_progress(0.5);
        assert_eq!(view.time_ns(), 500_000_000);
        assert!((view.progress() - 0.5).abs() < 1e-6);
        view.set_progress(7.0);
        assert_eq!(view.time_ns(), 1_000_000_000, "out-of-range progress clamps");
    }

    #[test]
    fn the_capture_is_shared_not_copied() {
        let view = ThreeView::new(wave_capture());
        let shared = view.shared_capture();
        assert!(Rc::ptr_eq(&shared, &view.shared_capture()));
        assert_eq!(shared.meshes.len(), 2);
        // The handle is one more reference, not a copy of the data...
        assert_eq!(Rc::strong_count(&shared), 2, "the view and this handle");
        // ...and releasing it leaves the view playing from its own reference.
        drop(shared);
        assert_eq!(view.current_mesh_index(), Some(0));
    }

    #[test]
    fn point_frames_follow_the_same_selection_rule() {
        let view = ThreeView::new(wave_capture());
        let points = view.current_points().expect("points exist");
        assert_eq!(points.points.len(), 1);
    }

    #[test]
    fn an_empty_capture_degrades_quietly() {
        let mut capture = wave_capture();
        capture.meshes.clear();
        capture.points.clear();
        let mut view = ThreeView::new(capture);
        view.tick(1_000);
        assert!(view.current_mesh().is_none());
        assert!(view.current_mesh_index().is_none());
        assert!(view.current_points().is_none());
        assert!((0.0..=1.0).contains(&view.progress()), "progress stays finite");
    }
}
