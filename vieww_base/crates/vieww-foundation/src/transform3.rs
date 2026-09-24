//! A 3D transform with a perspective divide — the one thing
//! [`Transform`](crate::Transform) deliberately is not.
//!
//! `Transform` is a 2×3 affine, and the module docs there explain why:
//! perspective would make hit testing non-invertible in the general case, and
//! nothing in the *widget* layer asks for it. But the film does — a card that
//! tilts back, a fan of planes receding, a reveal that comes at the camera —
//! and before this existed every one of those was forty lines of hand-rolled
//! projection arithmetic in an example (todo-upgrades U-04).
//!
//! So this is the smallest honest version of the capability: a 4×4 matrix, a
//! perspective divide, and `project_rect` — enough to make a tilted plane a
//! shape you can fill, and no more. It is a *shape* generator, not a widget
//! property: the result is an ordinary [`Path`], and everything downstream
//! (fills, gradients, clips, blurs) works on it unchanged.
//!
//! # Conventions
//!
//! * **Y grows down** (screen convention), as everywhere in vieww.
//! * **Z grows away from the viewer**: a point at positive `z` is further
//!   away, and [`perspective`](Transform3::perspective) shrinks it.
//! * Row-major 4×4, applied to column vectors: `p' = M·p`. [`then`](Transform3::then)
//!   matches [`Transform::then`](crate::Transform::then): `a.then(b)` applies
//!   `a` first, as `b(a(p))`.

use crate::{Offset, Path, Rect};

/// A 3D transform: 4×4 row-major, with the perspective row carried honestly.
///
/// Build one from the constructors ([`translation`](Self::translation),
/// [`rotation_x`](Self::rotation_x), [`rotation_y`](Self::rotation_y),
/// [`perspective`](Self::perspective)), compose with
/// [`then`](Self::then), and consume with
/// [`project`](Self::project) or [`project_rect`](Self::project_rect).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform3 {
    /// Row-major: `m[row * 4 + col]`. The last row is the perspective row —
    /// `[0, 0, 0, 1]` for an affine transform, and `w`-producing for a
    /// projection.
    pub m: [f32; 16],
}

impl Transform3 {
    /// The transform that changes nothing.
    pub const IDENTITY: Self = Self {
        m: [
            1.0, 0.0, 0.0, 0.0, //
            0.0, 1.0, 0.0, 0.0, //
            0.0, 0.0, 1.0, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ],
    };

    /// A pure 3D translation.
    #[must_use]
    pub const fn translation(dx: f32, dy: f32, dz: f32) -> Self {
        Self {
            m: [
                1.0, 0.0, 0.0, dx, //
                0.0, 1.0, 0.0, dy, //
                0.0, 0.0, 1.0, dz, //
                0.0, 0.0, 0.0, 1.0,
            ],
        }
    }

    /// A per-axis scale.
    #[must_use]
    pub const fn scale(sx: f32, sy: f32, sz: f32) -> Self {
        Self {
            m: [
                sx, 0.0, 0.0, 0.0, //
                0.0, sy, 0.0, 0.0, //
                0.0, 0.0, sz, 0.0, //
                0.0, 0.0, 0.0, 1.0,
            ],
        }
    }

    /// Rotation about the X axis (the horizontal axis on screen), radians.
    ///
    /// With Y down and Z away, a **positive** angle moves the top of a shape
    /// (negative Y) *toward* the viewer and its bottom away — the sign a
    /// "tilt back" wants is negative. The sign is easy to get backwards, which
    /// is why it is written here rather than left to geometry intuition.
    #[must_use]
    pub fn rotation_x(radians: f32) -> Self {
        let (s, c) = radians.sin_cos();
        Self {
            m: [
                1.0, 0.0, 0.0, 0.0, //
                0.0, c, -s, 0.0, //
                0.0, s, c, 0.0, //
                0.0, 0.0, 0.0, 1.0,
            ],
        }
    }

    /// Rotation about the Y axis (the vertical axis on screen), radians.
    ///
    /// A positive angle moves the right edge (positive X) toward the viewer —
    /// the turntable direction that reads as spinning left-to-right.
    #[must_use]
    pub fn rotation_y(radians: f32) -> Self {
        let (s, c) = radians.sin_cos();
        Self {
            m: [
                c, 0.0, s, 0.0, //
                0.0, 1.0, 0.0, 0.0, //
                -s, 0.0, c, 0.0, //
                0.0, 0.0, 0.0, 1.0,
            ],
        }
    }

    /// A pinhole perspective with the camera `focal` pixels behind the z=0
    /// plane, looking down +Z.
    ///
    /// The z=0 plane — the plane a widget's box lives in — projects to itself
    /// **exactly**: this is what makes the transform usable as a tilt rather
    /// than a camera, because an un-tilted rect comes back pixel-identical.
    /// A point at depth `z` scales by `focal / (focal + z)`: `z = focal`
    /// halves it, `z → -focal` is the camera plane itself, where
    /// [`project`](Self::project) returns `None`.
    ///
    /// `focal` is in the same units as the coordinates (logical pixels); a
    /// larger value is a longer lens — less distortion, gentler recession.
    /// 800–1200 covers the photographic range for a 1280×720 stage.
    #[must_use]
    pub const fn perspective(focal: f32) -> Self {
        Self {
            m: [
                focal, 0.0, 0.0, 0.0, //
                0.0, focal, 0.0, 0.0, //
                0.0, 0.0, 1.0, 0.0, //
                0.0, 0.0, 1.0, focal,
            ],
        }
    }

    /// `self` followed by `other`, as in [`Transform::then`](crate::Transform::then):
    /// the result applies `self` first — `a.then(b)` transforms a point as
    /// `b(a(point))`.
    #[must_use]
    pub fn then(self, other: Self) -> Self {
        let mut m = [0.0f32; 16];
        for (row, out) in m.iter_mut().enumerate() {
            let (i, j) = (row / 4, row % 4);
            *out = (0..4).map(|k| other.m[i * 4 + k] * self.m[k * 4 + j]).sum();
        }
        Self { m }
    }

    /// The full four-vector `M·[x, y, z, 1]`, w row included — for callers
    /// doing their own divide (fog, depth sorting).
    #[must_use]
    pub fn apply(self, x: f32, y: f32, z: f32) -> [f32; 4] {
        let p = [x, y, z, 1.0];
        let mut out = [0.0f32; 4];
        for (row, o) in out.iter_mut().enumerate() {
            *o = (0..4).map(|k| self.m[row * 4 + k] * p[k]).sum();
        }
        out
    }

    /// Project a point through the perspective divide.
    ///
    /// `None` if the point is on or behind the camera plane (`w ≤ 0`) or the
    /// arithmetic went non-finite — the two cases a 10,800-frame master hits
    /// once and spends an evening on (todo-upgrades U-16).
    #[must_use]
    pub fn project(self, point: Offset, z: f32) -> Option<Offset> {
        let [x, y, _, w] = self.apply(point.dx, point.dy, z);
        if w.is_finite() && w > 0.0 {
            let out = Offset::new(x / w, y / w);
            (out.dx.is_finite() && out.dy.is_finite()).then_some(out)
        } else {
            None
        }
    }

    /// Project a rectangle's four corners — at `z = 0`, so any depth comes
    /// from the transform itself (a rotation composed in front of a
    /// [`perspective`](Self::perspective)) — into a closed [`Path`].
    ///
    /// This is the perspective answer to [`Transform::apply_rect`](crate::Transform::apply_rect):
    /// the quad a tilted plane occupies on screen, as a shape any fill,
    /// gradient, clip or blur applies to unchanged. `None` when any corner
    /// falls behind the camera.
    ///
    /// ```no_run
    /// # use vieww_foundation::{Rect, Transform3};
    /// // A card that tilts back 30° around its bottom edge.
    /// let rect = Rect::new(200.0, 100.0, 600.0, 500.0);
    /// let bottom = rect.bottom;
    /// let tilt = Transform3::translation(0.0, -bottom, 0.0)
    ///     .then(Transform3::rotation_x(-0.52))
    ///     .then(Transform3::translation(0.0, bottom, 0.0))
    ///     .then(Transform3::perspective(1000.0));
    /// let quad = tilt.project_rect(rect).expect("on screen");
    /// // fill(quad, gradient) — the card, in perspective, as one shape.
    /// # let _ = quad;
    /// ```
    #[must_use]
    pub fn project_rect(self, rect: Rect) -> Option<Path> {
        let corners = [
            self.project(Offset::new(rect.left, rect.top), 0.0)?,
            self.project(Offset::new(rect.right, rect.top), 0.0)?,
            self.project(Offset::new(rect.right, rect.bottom), 0.0)?,
            self.project(Offset::new(rect.left, rect.bottom), 0.0)?,
        ];
        let mut path = Path::new();
        path.move_to(corners[0])
            .line_to(corners[1])
            .line_to(corners[2])
            .line_to(corners[3])
            .close();
        Some(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_identity_projects_a_rect_to_itself() {
        let rect = Rect::new(10.0, 20.0, 110.0, 70.0);
        let quad = Transform3::IDENTITY.project_rect(rect).unwrap();
        // The same four corners, in the same order — an exact round trip.
        let expected = [
            (10.0, 20.0),
            (110.0, 20.0),
            (110.0, 70.0),
            (10.0, 70.0),
        ];
        for (verb, want) in quad.verbs().iter().zip(expected) {
            let p = match verb {
                crate::PathVerb::MoveTo(p) | crate::PathVerb::LineTo(p) => *p,
                crate::PathVerb::CubicTo(_, _, p) => *p,
                crate::PathVerb::Close => continue,
            };
            assert!((p.dx - want.0).abs() < 1e-4, "{p:?} != {want:?}");
            assert!((p.dy - want.1).abs() < 1e-4, "{p:?} != {want:?}");
        }
    }

    #[test]
    fn the_z_zero_plane_is_the_screen() {
        let t = Transform3::perspective(800.0);
        let p = t.project(Offset::new(100.0, -40.0), 0.0).unwrap();
        assert!((p.dx - 100.0).abs() < 1e-4);
        assert!((p.dy + 40.0).abs() < 1e-4);
    }

    #[test]
    fn depth_shrinks_by_the_focal_ratio() {
        // z = focal halves; z = 3·focal quarters.
        let t = Transform3::perspective(800.0);
        let half = t.project(Offset::new(200.0, 0.0), 800.0).unwrap();
        assert!((half.dx - 100.0).abs() < 1e-3);
        let quarter = t.project(Offset::new(400.0, 0.0), 2400.0).unwrap();
        assert!((quarter.dx - 100.0).abs() < 1e-3);
    }

    #[test]
    fn the_camera_plane_is_none() {
        let t = Transform3::perspective(800.0);
        assert!(t.project(Offset::new(0.0, 0.0), -800.0).is_none());
        assert!(t.project(Offset::new(0.0, 0.0), -1600.0).is_none());
    }

    #[test]
    fn a_tilted_rect_comes_back_narrower_at_the_far_edge() {
        // Tilt back around the bottom edge: the top edge recedes, so it must
        // project *inward* on both sides, and the bottom edge must not move.
        let rect = Rect::new(100.0, 100.0, 500.0, 400.0);
        let bottom = rect.bottom;
        let tilt = Transform3::translation(0.0, -bottom, 0.0)
            .then(Transform3::rotation_x(-0.6))
            .then(Transform3::translation(0.0, bottom, 0.0))
            .then(Transform3::perspective(1000.0));
        let quad = tilt.project_rect(rect).unwrap();
        let pts: Vec<Offset> = quad
            .verbs()
            .iter()
            .filter_map(|v| match v {
                crate::PathVerb::MoveTo(p) | crate::PathVerb::LineTo(p) => Some(*p),
                _ => None,
            })
            .collect();
        let [tl, tr, br, bl] = [pts[0], pts[1], pts[2], pts[3]];
        // Bottom edge unmoved (z=0 there: the pivot).
        assert!((bl.dy - rect.bottom).abs() < 1e-3);
        assert!((br.dy - rect.bottom).abs() < 1e-3);
        // Top edge has moved toward the pivot (up in Y-down means smaller y
        // than the rotated-but-unprojected edge would be is hard to state
        // exactly; the robust assertion is the *pinch*: the top edge is
        // shorter than the bottom edge).
        let top = (tr.dx - tl.dx).abs();
        let bottom_width = (br.dx - bl.dx).abs();
        assert!(
            top < bottom_width,
            "tilting back must pinch the far edge: {top} !< {bottom_width}"
        );
    }

    #[test]
    fn then_applies_self_first() {
        // translate.then(scale) must scale the translation, not translate the
        // scale — the same order guarantee `Transform::then` pins.
        let t = Transform3::translation(10.0, 0.0, 0.0).then(Transform3::scale(2.0, 2.0, 2.0));
        let [x, y, z, w] = t.apply(1.0, 1.0, 1.0);
        assert!((x - 22.0).abs() < 1e-4); // (1 + 10) * 2
        assert!((y - 2.0).abs() < 1e-4);
        assert!((z - 2.0).abs() < 1e-4);
        assert!((w - 1.0).abs() < 1e-4);
    }
}
