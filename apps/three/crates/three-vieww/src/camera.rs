//! Orbit camera state suitable for wiring to Vieww gestures.

use three_core::{Bounds, Vec3};

/// A tiny camera orbit state suitable for wiring to Vieww gestures.
///
/// The camera sits at [`eye`](Self::eye), `distance` meters from
/// [`target`](Self::target), at `pitch` radians above the horizon and `yaw`
/// radians around the +Y axis (measured from +Z, so yaw 0 looks from +Z
/// toward −Z, and yaw π/2 looks from +X).
///
/// Drag and zoom take pixel deltas rather than angles because that is what a
/// gesture recognizer hands over: [`drag`](Self::drag) receives the same
/// `dx`/`dy` a `DragDetails::delta` carries, and [`zoom`](Self::zoom) the
/// `dy` of a scroll wheel or the scale factor of a pinch.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OrbitCamera {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: Vec3,
}

/// How far a drag of one pixel rotates the camera, in radians.
const DRAG_GAIN: f32 = 0.01;

/// The pitch beyond which the camera would flip over the pole.
const PITCH_LIMIT: f32 = 1.5;

/// The closest the camera may approach the target.
///
/// Not zero, because a viewer sitting inside its subject is degenerate: the
/// projection divides by depth, triangles cross the near plane, and the
/// picture dissolves. A hard floor keeps zoom gesturing safe.
const MIN_DISTANCE: f32 = 0.05;

impl OrbitCamera {
    /// The world-space position of the eye.
    pub fn eye(&self) -> Vec3 {
        let horizontal = self.pitch.cos();
        Vec3::new(
            self.target.x + self.distance * horizontal * self.yaw.sin(),
            self.target.y + self.distance * self.pitch.sin(),
            self.target.z + self.distance * horizontal * self.yaw.cos(),
        )
    }

    /// The point the camera orbits around.
    pub fn target(&self) -> Vec3 {
        self.target
    }

    /// Rotate from pointer movement, in pixels.
    ///
    /// Yaw follows `dx` (drag right, subject rotates right) and pitch follows
    /// `dy` (drag down, look from higher up), each pixel worth
    /// [`DRAG_GAIN`](constant.DRAG_GAIN) radians — about 57 pixels per 10
    /// degrees, in the range a trackpad or a finger both feel natural at.
    pub fn drag(&mut self, dx: f32, dy: f32) {
        self.yaw += dx * DRAG_GAIN;
        self.pitch = (self.pitch + dy * DRAG_GAIN).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    /// Move the camera `delta` meters toward the target (positive = zoom in).
    pub fn zoom(&mut self, delta: f32) {
        self.distance = (self.distance - delta).max(MIN_DISTANCE);
    }

    /// Zoom by a multiplicative pinch factor.
    ///
    /// `factor > 1.0` spreads the fingers and moves the camera **in**;
    /// `factor < 1.0` pinches and moves it out. This is the inverse of how a
    /// `scale` gesture reports spread, so a viewer wires it as
    /// `camera.pinch(1.0 / details.scale)` per update, or
    /// `camera.pinch(previous_scale / details.scale)` for the incremental
    /// delta.
    pub fn pinch(&mut self, factor: f32) {
        if factor.is_finite() && factor > 0.0 {
            self.distance = (self.distance / factor).max(MIN_DISTANCE);
        }
    }

    /// Frame `bounds` entirely from any orbit direction.
    ///
    /// Sets the target to the bounds center and the distance so a sphere
    /// enclosing the capture fits the given vertical field of view with
    /// `margin` room to spare. Leaves yaw and pitch alone — framing says
    /// *where* to look from, not which side; the caller keeps the user's
    /// viewing angle, which is what "reset camera" should preserve and
    /// "fit to view" should not disturb.
    pub fn fit(&mut self, bounds: Bounds, fov_y: f32, margin: f32) {
        let radius = bounds.radius().max(1e-6);
        let fov = fov_y.clamp(0.01, std::f32::consts::FRAC_PI_2);
        self.target = bounds.center();
        // The enclosing sphere touches the frustum walls, so distance is the
        // sphere over sin(half-fov), with a margin so the silhouette never
        // kisses the viewport edge.
        self.distance = (radius / (0.5 * fov).sin()) * margin.max(1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eye_orbits_the_target() {
        let camera = OrbitCamera {
            yaw: 0.0,
            pitch: 0.0,
            distance: 2.0,
            target: Vec3::new(1.0, 2.0, 3.0),
        };
        // Yaw 0, pitch 0: on +Z looking toward -Z.
        assert_eq!(camera.eye(), Vec3::new(1.0, 2.0, 5.0));

        let quarter = OrbitCamera {
            yaw: std::f32::consts::FRAC_PI_2,
            pitch: 0.0,
            distance: 2.0,
            target: Vec3::new(0.0, 0.0, 0.0),
        };
        let eye = quarter.eye();
        assert!((eye.x - 2.0).abs() < 1e-6, "yaw pi/2 puts the eye on +X");
        assert!(eye.z.abs() < 1e-6);
    }

    #[test]
    fn pitch_is_sine_up() {
        let camera = OrbitCamera {
            yaw: 0.0,
            pitch: std::f32::consts::FRAC_PI_2,
            distance: 3.0,
            target: Vec3::new(0.0, 0.0, 0.0),
        };
        let eye = camera.eye();
        assert!((eye.y - 3.0).abs() < 1e-6, "pitch pi/2 is directly overhead");
        assert!(eye.x.abs() < 1e-6 && eye.z.abs() < 1e-6);
    }

    #[test]
    fn drag_clamps_pitch_but_not_yaw() {
        let mut camera = OrbitCamera { yaw: 0.0, pitch: 0.0, distance: 1.0, target: Vec3::new(0.0, 0.0, 0.0) };
        camera.drag(0.0, 100_000.0);
        assert_eq!(camera.pitch, 1.5, "pitch cannot exceed the pole");
        camera.drag(0.0, -100_000.0);
        assert_eq!(camera.pitch, -1.5, "pitch cannot go under the floor");
        camera.drag(100_000.0, 0.0);
        assert!(camera.yaw > 100.0, "yaw wraps freely");
    }

    #[test]
    fn zoom_and_pinch_respect_the_floor() {
        let mut camera = OrbitCamera { yaw: 0.0, pitch: 0.0, distance: 1.0, target: Vec3::new(0.0, 0.0, 0.0) };
        camera.zoom(100.0);
        assert_eq!(camera.distance, 0.05);
        camera.pinch(0.0);
        assert_eq!(camera.distance, 0.05, "a zero pinch factor is ignored, not NaN");
    }

    #[test]
    fn pinch_moves_in_when_fingers_spread() {
        let mut camera = OrbitCamera { yaw: 0.0, pitch: 0.0, distance: 4.0, target: Vec3::new(0.0, 0.0, 0.0) };
        camera.pinch(2.0);
        assert!((camera.distance - 2.0).abs() < 1e-6);
    }

    #[test]
    fn fit_centers_and_distances_for_the_frustum() {
        let bounds = Bounds {
            min: Vec3::new(-1.0, -1.0, -1.0),
            max: Vec3::new(1.0, 1.0, 1.0),
        };
        let mut camera = OrbitCamera { yaw: 0.4, pitch: 0.3, distance: 99.0, target: Vec3::new(0.0, 0.0, 0.0) };
        camera.fit(bounds, std::f32::consts::FRAC_PI_4, 1.2);

        assert_eq!(camera.target(), Vec3::new(0.0, 0.0, 0.0));
        // radius = sqrt(3), sin(22.5 deg) ~ 0.38268, margin 1.2
        let expected = 3.0_f32.sqrt() / (std::f32::consts::FRAC_PI_8).sin() * 1.2;
        assert!((camera.distance - expected).abs() < 1e-4);
        // The user's angle survives a fit.
        assert_eq!(camera.yaw, 0.4);
        assert_eq!(camera.pitch, 0.3);
    }
}
