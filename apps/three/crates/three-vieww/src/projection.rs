//! Perspective projection from world space to a top-left-origin viewport.

use crate::OrbitCamera;
use three_core::Vec3;

/// A vertical-field-of-view perspective frustum.
///
/// Angles in radians; `near` and `far` in meters, both positive. The defaults
/// (50°, 5 cm, 1 km) suit a hand-held capture viewer: wide enough to frame a
/// room at arm's length, near plane tight enough that orbiting close to a
/// subject does not clip it away.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Perspective {
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for Perspective {
    fn default() -> Self {
        Self { fov_y: 50.0_f32.to_radians(), near: 0.05, far: 1_000.0 }
    }
}

/// The rectangle being drawn into, in logical pixels.
///
/// This is Vieww's own unit — the same `Size` a painter is handed — so the
/// numbers a [`Projector`] produces need no further scaling before being
/// recorded as drawing coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
}

impl Viewport {
    pub fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// One world point after projection.
///
/// `x`/`y` are meaningful only while [`in_front`](Self::in_front) holds; a
/// point behind the camera has no honest screen position, and the fields are
/// left at zero rather than carrying `NaN` into a drawing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenPoint {
    /// Pixels from the left edge of the viewport.
    pub x: f32,
    /// Pixels from the top edge of the viewport (Y grows downward).
    pub y: f32,
    /// Distance along the view direction, meters. Positive in front.
    ///
    /// This is view-space depth rather than eye distance: it preserves the
    /// painter's-algorithm ordering even for wide lenses, where two points at
    /// equal eye distance but different angles must not compare equal.
    pub depth: f32,
    /// `depth > 0`: the point is in front of the camera plane.
    pub in_front: bool,
    /// Inside the frustum: in front, past the near plane, before the far
    /// plane, and within the viewport rectangle.
    pub on_screen: bool,
}

/// A fixed camera + lens + viewport, ready to project points.
///
/// Built once per paint from the values a widget already holds; projecting is
/// then a handful of dot products per vertex, with no per-point allocation.
pub struct Projector {
    eye: Vec3,
    right: Vec3,
    up: Vec3,
    forward: Vec3,
    focal_x: f32,
    focal_y: f32,
    near: f32,
    far: f32,
    width: f32,
    height: f32,
}

impl Projector {
    /// Precompute the view basis and lens for `camera`, `lens`, and `viewport`.
    pub fn new(camera: &OrbitCamera, lens: Perspective, viewport: Viewport) -> Self {
        let eye = camera.eye();
        let forward = sub(camera.target(), eye);
        let forward = normalize(forward);
        // Right-handed basis: right = forward x world-up, up = right x forward.
        // Degenerate when looking straight down the world-up axis, so the
        // world-up is nudged there — the standard trick that keeps pitch at
        // the pole from collapsing the basis.
        let world_up = if forward.y.abs() > 0.999 {
            Vec3::new(0.0, 0.0, -forward.y.signum())
        } else {
            Vec3::new(0.0, 1.0, 0.0)
        };
        let right = normalize(cross(forward, world_up));
        let up = cross(right, forward);

        let width = viewport.width.max(1e-6);
        let height = viewport.height.max(1e-6);
        let focal_y = 1.0 / (0.5 * lens.fov_y).tan();
        let aspect = width / height;

        Self {
            eye,
            right,
            up,
            forward,
            focal_x: focal_y / aspect,
            focal_y,
            near: lens.near,
            far: lens.far,
            width,
            height,
        }
    }

    /// The near-plane distance the projector was built with.
    ///
    /// Exposed so callers sharing one projector can apply the same near-plane
    /// rule to their own geometry (the presentation layer drops triangles
    /// crossing it rather than clipping them).
    pub fn near(&self) -> f32 {
        self.near
    }

    /// Project one world-space point.
    pub fn project(&self, point: Vec3) -> ScreenPoint {
        let d = sub(point, self.eye);
        let depth = dot(d, self.forward);
        if depth <= 0.0 {
            return ScreenPoint { x: 0.0, y: 0.0, depth, in_front: false, on_screen: false };
        }

        let x_v = dot(d, self.right);
        let y_v = dot(d, self.up);
        let x_ndc = x_v * self.focal_x / depth;
        let y_ndc = y_v * self.focal_y / depth;
        // NDC to top-left pixels: x mirrors, y flips — NDC +Y is up, the
        // viewport's +Y is down.
        let x = (0.5 + 0.5 * x_ndc) * self.width;
        let y = (0.5 - 0.5 * y_ndc) * self.height;

        let on_screen = depth > self.near
            && depth < self.far
            && x >= 0.0
            && x <= self.width
            && y >= 0.0
            && y <= self.height;

        ScreenPoint { x, y, depth, in_front: true, on_screen }
    }
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

fn normalize(v: Vec3) -> Vec3 {
    let length = dot(v, v).sqrt();
    if length > 1e-9 {
        Vec3::new(v.x / length, v.y / length, v.z / length)
    } else {
        // A zero-length forward means eye on target: keep the basis finite so
        // a degenerate camera degrades to "everything at the near plane"
        // rather than NaN coordinates.
        Vec3::new(0.0, 0.0, -1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OrbitCamera;

    /// A camera at (0, 0, 5) looking at the origin through a 90-degree lens.
    fn setup() -> (Projector, Perspective, Viewport) {
        let camera = OrbitCamera {
            yaw: 0.0,
            pitch: 0.0,
            distance: 5.0,
            target: Vec3::new(0.0, 0.0, 0.0),
        };
        let lens = Perspective { fov_y: std::f32::consts::FRAC_PI_2, near: 0.05, far: 100.0 };
        let viewport = Viewport::new(200.0, 100.0);
        (Projector::new(&camera, lens, viewport), lens, viewport)
    }

    #[test]
    fn the_center_of_the_view_is_the_center_of_the_viewport() {
        let (projector, _, _) = setup();
        let p = projector.project(Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(p.x, 100.0);
        assert_eq!(p.y, 50.0);
        assert!((p.depth - 5.0).abs() < 1e-5);
        assert!(p.in_front && p.on_screen);
    }

    #[test]
    fn x_grows_rightward_on_screen() {
        let (projector, _, _) = setup();
        let p = projector.project(Vec3::new(1.0, 0.0, 0.0));
        // focal_y = 1 at 90 deg, aspect = 2, so focal_x = 0.5;
        // x_ndc = 1 * 0.5 / 5 = 0.1 -> x = 110.
        assert!((p.x - 110.0).abs() < 1e-4);
        assert!((p.y - 50.0).abs() < 1e-4);
    }

    #[test]
    fn y_up_in_the_world_is_up_on_the_screen() {
        let (projector, _, _) = setup();
        let p = projector.project(Vec3::new(0.0, 1.0, 0.0));
        // y_ndc = 1 * 1.0 / 5 = 0.2 -> y = (0.5 - 0.1) * 100 = 40.
        assert!((p.y - 40.0).abs() < 1e-4);
        assert!(p.y < 50.0, "up in the world is above the viewport middle");
    }

    #[test]
    fn behind_the_camera_has_no_screen_position() {
        let (projector, _, _) = setup();
        let p = projector.project(Vec3::new(0.0, 0.0, 10.0));
        assert!(!p.in_front && !p.on_screen);
        assert_eq!((p.x, p.y), (0.0, 0.0));
    }

    #[test]
    fn off_screen_but_in_front_is_in_front_not_on_screen() {
        let (projector, _, _) = setup();
        let p = projector.project(Vec3::new(20.0, 0.0, 0.0));
        assert!(p.in_front);
        assert!(!p.on_screen, "x_ndc = 2 is outside the frustum");
        assert!(p.x > 200.0);
    }

    #[test]
    fn near_plane_points_report_on_screen_false() {
        let camera = OrbitCamera {
            yaw: 0.0,
            pitch: 0.0,
            distance: 5.0,
            target: Vec3::new(0.0, 0.0, 0.0),
        };
        let lens = Perspective { fov_y: std::f32::consts::FRAC_PI_2, near: 2.0, far: 100.0 };
        let projector = Projector::new(&camera, lens, Viewport::new(200.0, 100.0));
        // depth 1 < near 2
        let p = projector.project(Vec3::new(0.0, 0.0, 4.0));
        assert!(p.in_front);
        assert!(!p.on_screen);
    }

    #[test]
    fn the_basis_survives_looking_straight_down() {
        let camera = OrbitCamera {
            yaw: 0.0,
            pitch: std::f32::consts::FRAC_PI_2,
            distance: 5.0,
            target: Vec3::new(0.0, 0.0, 0.0),
        };
        let projector =
            Projector::new(&camera, Perspective::default(), Viewport::new(100.0, 100.0));
        let p = projector.project(Vec3::new(0.0, 0.0, 0.0));
        assert!(p.in_front && p.on_screen);
        assert!((p.x - 50.0).abs() < 1e-4);
        assert!((p.y - 50.0).abs() < 1e-4);
    }
}
