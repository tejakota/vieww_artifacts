//! The viewer's virtual camera: orbit, dolly, and the spring home.

use three_core::Vec3;

/// How far a drag of one pixel rotates the view, in radians.
const DRAG_GAIN: f32 = 0.008;

/// How far the orbit may swing horizontally, radians (~28 degrees).
///
/// A monocular-with-depth capture is one viewpoint: past roughly this angle
/// there is more hole than image, because the camera is asking to see
/// around surfaces it never photographed. The clamp stops the gesture
/// before it starts lying.
const YAW_LIMIT: f32 = 0.5;

/// Vertical orbit limit (~20 degrees) — see [`YAW_LIMIT`].
const PITCH_LIMIT: f32 = 0.35;

/// Dolly limits as multiples of the subject distance: half as close,
/// twice as far.
const DOLLY_MIN: f32 = 0.5;
const DOLLY_MAX: f32 = 2.0;

/// How fast the recenter spring settles: the fraction of remaining distance
/// covered per millisecond. 0.012/ms ≈ 98% gone in 325 ms — quick enough to
/// feel like a release, slow enough to read as a movement, not a cut.
const RECENTRE_RATE_MS: f32 = 0.012;

/// The viewer's camera state: where around the subject it sits.
///
/// This is deliberately *not* a free 6-DOF camera. A depth video has one
/// real viewpoint and geometry measured from it; the two axes and a dolly
/// are the honest envelope of that data, and the clamps keep the warp inside
/// what was actually filmed.
///
/// All values are relative to the capture's own camera, which sits at the
/// neutral pose `(yaw 0, pitch 0, dolly 1)` — exactly where the footage was
/// recorded from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewCamera {
    /// Orbit around the subject's vertical axis, radians. Positive orbits
    /// toward the capture's left side (drag right, subject turns right).
    pub yaw: f32,
    /// Orbit above the horizon, radians. Positive is above, looking down.
    pub pitch: f32,
    /// Distance to the subject as a multiple of its true distance: 1.0 is
    /// where the camera stood, 0.5 half as close, 2.0 twice as far.
    pub dolly: f32,
    /// True while a recenter is settling; advanced by [`step_recentre`].
    pub(crate) recentring: bool,
}

impl Default for ViewCamera {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
            dolly: 1.0,
            recentring: false,
        }
    }
}

impl ViewCamera {
    /// Whether this pose is the neutral one, within the epsilon the warp
    /// uses to skip re-projection entirely. Feeds autoplay on this answer:
    /// the common case must cost nothing.
    pub fn is_neutral(&self) -> bool {
        self.yaw.abs() < 1e-4 && self.pitch.abs() < 1e-4 && (self.dolly - 1.0).abs() < 1e-4
    }

    /// Pan/drag: orbit around the subject, in pixel deltas straight from a
    /// gesture recognizer.
    pub fn drag(&mut self, dx: f32, dy: f32) {
        self.recentring = false;
        self.yaw = (self.yaw + dx * DRAG_GAIN).clamp(-YAW_LIMIT, YAW_LIMIT);
        self.pitch = (self.pitch + dy * DRAG_GAIN).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    /// Pinch/scroll: `factor > 1` (fingers spread) moves closer; `factor <
    /// 1` pulls back. `scroll` deltas arrive as pixels and can be handed
    /// over as `pinch(1.0 - delta.dy * 0.001)`.
    pub fn pinch(&mut self, factor: f32) {
        if factor.is_finite() && factor > 0.0 {
            self.recentring = false;
            self.dolly = (self.dolly / factor).clamp(DOLLY_MIN, DOLLY_MAX);
        }
    }

    /// Begin the recenter spring — the "hold" gesture.
    ///
    /// The animation itself is deterministic: [`step_recentre`] moves the
    /// pose an exponential fraction of the remaining distance per tick, so
    /// the same tick sequence always lands the same way, and a test can
    /// drive it with `Duration`s instead of sleeping.
    pub fn begin_recentre(&mut self) {
        self.recentring = true;
    }

    /// Advance the recenter spring by `delta`. Returns whether it is still
    /// moving, so the caller can keep requesting frames until it lands.
    pub fn step_recentre(&mut self, delta: std::time::Duration) -> bool {
        if !self.recentring {
            return false;
        }
        let ms = delta.as_secs_f32() * 1000.0;
        let remaining = 1.0 - (-RECENTRE_RATE_MS * ms).exp();
        let arrived = self.is_neutral();
        self.yaw *= 1.0 - remaining;
        self.pitch *= 1.0 - remaining;
        self.dolly = 1.0 + (self.dolly - 1.0) * (1.0 - remaining);
        if self.is_neutral() || arrived {
            self.yaw = 0.0;
            self.pitch = 0.0;
            self.dolly = 1.0;
            self.recentring = false;
            return false;
        }
        true
    }

    // ── the pose, for the renderer ───────────────────────────────────────

    /// The virtual camera's eye position in the capture's own coordinates.
    ///
    /// The subject sits on the optical axis at `subject_distance`; the eye
    /// is `dolly * subject_distance` back from it along the view direction.
    /// At the neutral pose this is the origin — the capture's own camera.
    pub fn eye(&self, subject_distance: f32) -> Vec3 {
        let forward = self.forward();
        let distance = self.dolly * subject_distance;
        Vec3::new(
            -forward.x * distance,
            -forward.y * distance,
            subject_distance - forward.z * distance,
        )
    }

    /// The unit vector from the eye toward the subject.
    ///
    /// Built so that `(yaw, pitch) = (0, 0)` gives `(0, 0, 1)` — the capture
    /// camera's own forward — and positive pitch looks downward from above
    /// (the Y-down convention's "up").
    pub fn forward(&self) -> Vec3 {
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        Vec3::new(sin_yaw * cos_pitch, sin_pitch, cos_yaw * cos_pitch)
    }

    /// The unit vector of the image's horizontal axis in capture space.
    pub fn right(&self) -> Vec3 {
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        Vec3::new(cos_yaw, 0.0, -sin_yaw)
    }

    /// The unit vector of the image's vertical axis (downward, matching
    /// pixel rows).
    pub fn down(&self) -> Vec3 {
        cross(self.forward(), self.right())
    }
}

/// Cross product — small, local, and used by exactly one caller.
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(a: Vec3, b: Vec3, what: &str) {
        let close =
            (a.x - b.x).abs() < 1e-5 && (a.y - b.y).abs() < 1e-5 && (a.z - b.z).abs() < 1e-5;
        assert!(close, "{what}: {a:?} vs {b:?}");
    }

    #[test]
    fn neutral_pose_is_the_capture_camera() {
        let camera = ViewCamera::default();
        assert!(camera.is_neutral());
        assert_close(camera.forward(), Vec3::new(0.0, 0.0, 1.0), "forward");
        assert_close(camera.right(), Vec3::new(1.0, 0.0, 0.0), "right");
        assert_close(camera.down(), Vec3::new(0.0, 1.0, 0.0), "down");
        assert_close(
            camera.eye(3.0),
            Vec3::new(0.0, 0.0, 0.0),
            "eye at the origin",
        );
    }

    #[test]
    fn the_eye_stays_on_the_subject_sphere() {
        // Whatever the angles, the eye is `dolly * distance` from the
        // subject center at (0, 0, z_s) — the orbit that makes parallax
        // behave like walking around the subject.
        let subject = 2.0;
        let camera = ViewCamera {
            yaw: 0.7,
            pitch: -0.3,
            dolly: 1.4,
            recentring: false,
        };
        let eye = camera.eye(subject);
        let to_center = Vec3::new(0.0 - eye.x, 0.0 - eye.y, subject - eye.z);
        let length =
            (to_center.x * to_center.x + to_center.y * to_center.y + to_center.z * to_center.z)
                .sqrt();
        assert!(
            (length - 1.4 * subject).abs() < 1e-5,
            "orbit radius {length}"
        );
    }

    #[test]
    fn positive_pitch_is_above_looking_down() {
        let camera = ViewCamera {
            yaw: 0.0,
            pitch: 0.5,
            dolly: 1.0,
            recentring: false,
        };
        let eye = camera.eye(2.0);
        assert!(
            eye.y < 0.0,
            "in Y-down space, above the subject is negative y"
        );
        assert!(camera.forward().y > 0.0, "looking down is +y");
    }

    #[test]
    fn drag_and_pinch_clamp_to_the_honest_envelope() {
        let mut camera = ViewCamera::default();
        camera.drag(100_000.0, 0.0);
        assert_eq!(camera.yaw, 0.5);
        camera.drag(0.0, -100_000.0);
        assert_eq!(camera.pitch, -0.35);
        camera.pinch(1e9);
        assert!((camera.dolly - 0.5).abs() < 1e-6);
        camera.pinch(0.0);
        assert!(
            (camera.dolly - 0.5).abs() < 1e-6,
            "a zero factor is ignored, not NaN"
        );
    }

    #[test]
    fn recentre_springs_home_and_lands_exactly() {
        let mut camera = ViewCamera {
            yaw: 0.4,
            pitch: -0.2,
            dolly: 1.7,
            recentring: false,
        };
        camera.begin_recentre();
        // A long idle tick lands it.
        assert!(!camera.step_recentre(std::time::Duration::from_secs(5)));
        assert!(
            camera.is_neutral(),
            "spring settles exactly, not asymptotically-on-screen"
        );
    }

    #[test]
    fn recentre_is_deterministic() {
        let mut a = ViewCamera {
            yaw: 0.3,
            pitch: 0.1,
            dolly: 1.5,
            recentring: false,
        };
        let mut b = a;
        a.begin_recentre();
        b.begin_recentre();
        for _ in 0..10 {
            let moving = a.step_recentre(std::time::Duration::from_millis(16));
            assert_eq!(
                b.step_recentre(std::time::Duration::from_millis(16)),
                moving
            );
            assert_eq!(a, b);
        }
    }
}
