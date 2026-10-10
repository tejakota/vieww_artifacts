//! Small, dependency-free 3D mathematics used by ViewW's spatial renderer.
//!
//! The convention is right-handed world space, `+Y` up, and cameras looking
//! from their eye toward their target. Points and directions are kept separate
//! by naming (`transform_point` versus `transform_vector`) rather than by a
//! large family of nearly identical wrapper types.

use std::ops::{Add, Div, Mul, Neg, Sub};

/// A two-dimensional texture coordinate or projected point.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    #[must_use]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        self + (other - self) * t
    }
}
impl Add for Vec2 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}
impl Sub for Vec2 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}
impl Mul<f32> for Vec2 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs)
    }
}
impl Div<f32> for Vec2 {
    type Output = Self;
    fn div(self, rhs: f32) -> Self {
        Self::new(self.x / rhs, self.y / rhs)
    }
}

/// A three-dimensional vector. Operations intentionally use `f32`, matching
/// the scalar precision used by ViewW's layout and paint layers.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    pub const X: Self = Self::new(1.0, 0.0, 0.0);
    pub const Y: Self = Self::new(0.0, 1.0, 0.0);
    pub const Z: Self = Self::new(0.0, 0.0, 1.0);

    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
    #[must_use]
    pub fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }
    #[must_use]
    pub fn cross(self, rhs: Self) -> Self {
        Self::new(
            self.y * rhs.z - self.z * rhs.y,
            self.z * rhs.x - self.x * rhs.z,
            self.x * rhs.y - self.y * rhs.x,
        )
    }
    #[must_use]
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }
    #[must_use]
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }
    #[must_use]
    pub fn normalized(self) -> Self {
        let length = self.length();
        if length.is_finite() && length > 1.0e-8 {
            self / length
        } else {
            Self::ZERO
        }
    }
    #[must_use]
    pub fn component_mul(self, rhs: Self) -> Self {
        Self::new(self.x * rhs.x, self.y * rhs.y, self.z * rhs.z)
    }
    #[must_use]
    pub fn component_div(self, rhs: Self) -> Self {
        Self::new(
            safe_div(self.x, rhs.x),
            safe_div(self.y, rhs.y),
            safe_div(self.z, rhs.z),
        )
    }
    #[must_use]
    pub fn min(self, rhs: Self) -> Self {
        Self::new(self.x.min(rhs.x), self.y.min(rhs.y), self.z.min(rhs.z))
    }
    #[must_use]
    pub fn max(self, rhs: Self) -> Self {
        Self::new(self.x.max(rhs.x), self.y.max(rhs.y), self.z.max(rhs.z))
    }
    #[must_use]
    pub fn clamp(self, min: Self, max: Self) -> Self {
        self.max(min).min(max)
    }
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
    #[must_use]
    pub fn lerp(self, rhs: Self, t: f32) -> Self {
        self + (rhs - self) * t
    }
}

fn safe_div(a: f32, b: f32) -> f32 {
    if b.abs() > 1.0e-8 {
        a / b
    } else {
        0.0
    }
}
impl Add for Vec3 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}
impl Sub for Vec3 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}
impl Mul<f32> for Vec3 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}
impl Div<f32> for Vec3 {
    type Output = Self;
    fn div(self, rhs: f32) -> Self {
        Self::new(self.x / rhs, self.y / rhs, self.z / rhs)
    }
}
impl Neg for Vec3 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
}

/// Unit-quaternion rotation. Multiplication order is `self * rhs` (apply `rhs`
/// first, then `self`) which is the usual active-rotation convention.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Default for Quat {
    fn default() -> Self {
        Self::IDENTITY
    }
}
impl Quat {
    pub const IDENTITY: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 1.0,
    };
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }
    #[must_use]
    pub fn from_axis_angle(axis: Vec3, radians: f32) -> Self {
        let axis = axis.normalized();
        if axis == Vec3::ZERO || !radians.is_finite() {
            return Self::IDENTITY;
        }
        let half = radians * 0.5;
        let sine = half.sin();
        Self::new(axis.x * sine, axis.y * sine, axis.z * sine, half.cos()).normalized()
    }
    #[must_use]
    pub fn length_squared(self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w
    }
    #[must_use]
    pub fn normalized(self) -> Self {
        let length = self.length_squared().sqrt();
        if length.is_finite() && length > 1.0e-8 {
            Self::new(
                self.x / length,
                self.y / length,
                self.z / length,
                self.w / length,
            )
        } else {
            Self::IDENTITY
        }
    }
    #[must_use]
    pub fn conjugate(self) -> Self {
        Self::new(-self.x, -self.y, -self.z, self.w)
    }
    #[must_use]
    pub fn rotate(self, v: Vec3) -> Vec3 {
        let rotation = self.normalized();
        let qv = Vec3::new(rotation.x, rotation.y, rotation.z);
        let t = qv.cross(v) * 2.0;
        v + t * rotation.w + qv.cross(t)
    }
    #[must_use]
    pub fn slerp(self, other: Self, t: f32) -> Self {
        let a = self.normalized();
        let mut b = other.normalized();
        let mut dot = a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w;
        if dot < 0.0 {
            b = Self::new(-b.x, -b.y, -b.z, -b.w);
            dot = -dot;
        }
        if dot > 0.9995 {
            return Self::new(
                a.x + (b.x - a.x) * t,
                a.y + (b.y - a.y) * t,
                a.z + (b.z - a.z) * t,
                a.w + (b.w - a.w) * t,
            )
            .normalized();
        }
        let theta = dot.clamp(-1.0, 1.0).acos();
        let sin_theta = theta.sin();
        if sin_theta.abs() < 1.0e-8 {
            return a;
        }
        let wa = ((1.0 - t) * theta).sin() / sin_theta;
        let wb = (t * theta).sin() / sin_theta;
        Self::new(
            a.x * wa + b.x * wb,
            a.y * wa + b.y * wb,
            a.z * wa + b.z * wb,
            a.w * wa + b.w * wb,
        )
        .normalized()
    }
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite() && self.w.is_finite()
    }
}

impl Mul for Quat {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self::new(
            self.w * rhs.x + self.x * rhs.w + self.y * rhs.z - self.z * rhs.y,
            self.w * rhs.y - self.x * rhs.z + self.y * rhs.w + self.z * rhs.x,
            self.w * rhs.z + self.x * rhs.y - self.y * rhs.x + self.z * rhs.w,
            self.w * rhs.w - self.x * rhs.x - self.y * rhs.y - self.z * rhs.z,
        )
        .normalized()
    }
}

/// Position, orientation and scale of one spatial object.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform3 {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}
impl Default for Transform3 {
    fn default() -> Self {
        Self::IDENTITY
    }
}
impl Transform3 {
    pub const IDENTITY: Self = Self {
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        },
    };
    #[must_use]
    pub const fn new(translation: Vec3, rotation: Quat, scale: Vec3) -> Self {
        Self {
            translation,
            rotation,
            scale,
        }
    }
    #[must_use]
    pub fn transform_point(self, point: Vec3) -> Vec3 {
        self.translation + self.rotation.rotate(point.component_mul(self.scale))
    }
    #[must_use]
    pub fn transform_vector(self, vector: Vec3) -> Vec3 {
        self.rotation.rotate(vector.component_mul(self.scale))
    }
    /// Transform a normal by the inverse-transpose of the scale/rotation
    /// portion; zero scale components safely contribute zero rather than NaN.
    #[must_use]
    pub fn transform_normal(self, normal: Vec3) -> Vec3 {
        self.rotation
            .rotate(normal.component_div(self.scale))
            .normalized()
    }
    #[must_use]
    pub fn interpolate(self, rhs: Self, t: f32) -> Self {
        Self::new(
            self.translation.lerp(rhs.translation, t),
            self.rotation.slerp(rhs.rotation, t),
            self.scale.lerp(rhs.scale, t),
        )
    }
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.translation.is_finite() && self.rotation.is_finite() && self.scale.is_finite()
    }
    /// A usable transform must have a finite, non-degenerate quaternion.
    /// Scale components may be zero (to collapse geometry), but rotation may
    /// not be a zero quaternion because normalizing it would silently become an
    /// identity rotation.
    #[must_use]
    pub fn is_valid(self) -> bool {
        self.is_finite()
            && self.rotation.length_squared().is_finite()
            && self.rotation.length_squared() > 1.0e-12
    }
}

/// A ray with a normalized direction. `t` in intersections is world distance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray3 {
    pub origin: Vec3,
    pub direction: Vec3,
}
impl Ray3 {
    #[must_use]
    pub fn new(origin: Vec3, direction: Vec3) -> Self {
        Self {
            origin,
            direction: direction.normalized(),
        }
    }
    #[must_use]
    pub fn is_valid(self) -> bool {
        self.origin.is_finite()
            && self.direction.is_finite()
            && self.direction.length_squared().is_finite()
            && self.direction.length_squared() > 1.0e-12
    }
    #[must_use]
    pub fn at(self, t: f32) -> Vec3 {
        self.origin + self.direction.normalized() * t
    }
    /// Möller–Trumbore ray/triangle hit; returns world distance, or `None`.
    #[must_use]
    pub fn intersect_triangle(self, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
        if !self.is_valid() || !a.is_finite() || !b.is_finite() || !c.is_finite() {
            return None;
        }
        let direction = self.direction.normalized();
        let edge1 = b - a;
        let edge2 = c - a;
        let p = direction.cross(edge2);
        let determinant = edge1.dot(p);
        if determinant.abs() < 1.0e-7 {
            return None;
        }
        let inv = 1.0 / determinant;
        let tvec = self.origin - a;
        let u = tvec.dot(p) * inv;
        if !(0.0..=1.0).contains(&u) {
            return None;
        }
        let q = tvec.cross(edge1);
        let v = direction.dot(q) * inv;
        if v < 0.0 || u + v > 1.0 {
            return None;
        }
        let t = edge2.dot(q) * inv;
        (t >= 0.0 && t.is_finite()).then_some(t)
    }
}

/// Axis-aligned bounds, useful for broad-phase collision and editor picking.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb3 {
    pub min: Vec3,
    pub max: Vec3,
}
impl Aabb3 {
    #[must_use]
    pub const fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }
    #[must_use]
    pub fn from_center_half_extents(center: Vec3, half: Vec3) -> Self {
        Self::new(center - half, center + half)
    }
    #[must_use]
    pub fn is_valid(self) -> bool {
        self.min.is_finite()
            && self.max.is_finite()
            && self.min.x <= self.max.x
            && self.min.y <= self.max.y
            && self.min.z <= self.max.z
    }
    #[must_use]
    pub fn overlaps(self, rhs: Self) -> bool {
        self.is_valid()
            && rhs.is_valid()
            && self.min.x <= rhs.max.x
            && self.max.x >= rhs.min.x
            && self.min.y <= rhs.max.y
            && self.max.y >= rhs.min.y
            && self.min.z <= rhs.max.z
            && self.max.z >= rhs.min.z
    }
    #[must_use]
    pub fn center(self) -> Vec3 {
        self.min * 0.5 + self.max * 0.5
    }
    #[must_use]
    pub fn half_extents(self) -> Vec3 {
        self.max * 0.5 - self.min * 0.5
    }
    #[must_use]
    pub fn union(self, rhs: Self) -> Self {
        Self::new(self.min.min(rhs.min), self.max.max(rhs.max))
    }
    #[must_use]
    pub fn ray_intersection(self, ray: Ray3) -> Option<f32> {
        if !self.is_valid() || !ray.is_valid() {
            return None;
        }
        let direction = ray.direction.normalized();
        let mut t_min = 0.0_f32;
        let mut t_max = f32::INFINITY;
        for (origin, direction, min, max) in [
            (ray.origin.x, direction.x, self.min.x, self.max.x),
            (ray.origin.y, direction.y, self.min.y, self.max.y),
            (ray.origin.z, direction.z, self.min.z, self.max.z),
        ] {
            if direction.abs() < 1.0e-8 {
                if origin < min || origin > max {
                    return None;
                }
            } else {
                let inverse = 1.0 / direction;
                let mut near = (min - origin) * inverse;
                let mut far = (max - origin) * inverse;
                if near > far {
                    std::mem::swap(&mut near, &mut far);
                }
                t_min = t_min.max(near);
                t_max = t_max.min(far);
                if t_min > t_max {
                    return None;
                }
            }
        }
        (t_max >= 0.0 && t_min.is_finite()).then_some(t_min.max(0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quaternion_rotation_preserves_length() {
        let rotation = Quat::from_axis_angle(Vec3::Y, std::f32::consts::FRAC_PI_2);
        let rotated = rotation.rotate(Vec3::Z);
        assert!((rotated.length() - 1.0).abs() < 1.0e-5);
        assert!((rotated.x - 1.0).abs() < 1.0e-5);
    }

    #[test]
    fn transform_normal_handles_non_uniform_scale() {
        let transform = Transform3::new(Vec3::ZERO, Quat::IDENTITY, Vec3::new(2.0, 1.0, 0.5));
        let normal = transform.transform_normal(Vec3::new(1.0, 1.0, 0.0));
        assert!(normal.is_finite());
        assert!((normal.length() - 1.0).abs() < 1.0e-5);
    }

    #[test]
    fn ray_hits_triangle_and_misses_parallel_triangle() {
        let ray = Ray3::new(Vec3::new(0.25, 0.25, 1.0), -Vec3::Z);
        assert!(
            (ray.intersect_triangle(Vec3::ZERO, Vec3::X, Vec3::Y)
                .unwrap()
                - 1.0)
                .abs()
                < 1.0e-5
        );
        assert!(Ray3::new(Vec3::ZERO, Vec3::X)
            .intersect_triangle(Vec3::ZERO, Vec3::X, Vec3::Y)
            .is_none());
        assert!(Ray3 {
            origin: Vec3::new(f32::NAN, 0.0, 0.0),
            direction: -Vec3::Z
        }
        .intersect_triangle(Vec3::ZERO, Vec3::X, Vec3::Y)
        .is_none());
    }

    #[test]
    fn slab_ray_test_handles_parallel_axes() {
        let bounds = Aabb3::new(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
        assert_eq!(
            bounds.ray_intersection(Ray3::new(Vec3::new(0.0, 0.0, 3.0), -Vec3::Z)),
            Some(2.0)
        );
        assert!(bounds
            .ray_intersection(Ray3::new(Vec3::new(3.0, 0.0, 3.0), -Vec3::Z))
            .is_none());
        assert!(
            Aabb3::new(Vec3::new(1.0, 1.0, 1.0), Vec3::new(-1.0, -1.0, -1.0))
                .ray_intersection(Ray3::new(Vec3::ZERO, Vec3::Z))
                .is_none()
        );
    }

    #[test]
    fn aabb_center_and_extents_avoid_intermediate_overflow() {
        let bounds = Aabb3::new(
            Vec3::new(-f32::MAX, -f32::MAX, -f32::MAX),
            Vec3::new(f32::MAX, f32::MAX, f32::MAX),
        );
        assert_eq!(bounds.center(), Vec3::ZERO);
        assert!(bounds.half_extents().is_finite());
    }
}
