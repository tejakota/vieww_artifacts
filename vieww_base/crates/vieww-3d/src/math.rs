//! Vectors, quaternions and 4×4 matrices — Three.js' `Vector3`,
//! `Quaternion` and `Matrix4`.
//!
//! Matrices are **column-major** and act on **column vectors** (`M * v`),
//! the OpenGL/Three.js convention, so a world matrix is `parent * local` and
//! a model-view-projection is `projection * view * model`. The coordinate
//! system is right-handed with +y up and the camera looking down −z, again
//! as in Three.js and glTF, so imported scenes need no axis juggling.

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub};

/// A 3-vector.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    pub const ONE: Self = Self::new(1.0, 1.0, 1.0);
    pub const X: Self = Self::new(1.0, 0.0, 0.0);
    pub const Y: Self = Self::new(0.0, 1.0, 0.0);
    pub const Z: Self = Self::new(0.0, 0.0, 1.0);

    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    #[must_use]
    pub const fn splat(v: f32) -> Self {
        Self::new(v, v, v)
    }

    #[must_use]
    pub const fn from_array(a: [f32; 3]) -> Self {
        Self::new(a[0], a[1], a[2])
    }

    #[must_use]
    pub const fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }

    #[must_use]
    pub fn dot(self, o: Self) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    #[must_use]
    pub fn cross(self, o: Self) -> Self {
        Self::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    #[must_use]
    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    /// Unit length; zero stays zero rather than becoming NaN.
    #[must_use]
    pub fn normalize(self) -> Self {
        let l = self.length();
        if l > 1e-12 {
            self / l
        } else {
            Self::ZERO
        }
    }

    #[must_use]
    pub fn lerp(self, o: Self, t: f32) -> Self {
        self + (o - self) * t
    }

    /// Component-wise product.
    #[must_use]
    pub fn mul_elem(self, o: Self) -> Self {
        Self::new(self.x * o.x, self.y * o.y, self.z * o.z)
    }

    #[must_use]
    pub fn min(self, o: Self) -> Self {
        Self::new(self.x.min(o.x), self.y.min(o.y), self.z.min(o.z))
    }

    #[must_use]
    pub fn max(self, o: Self) -> Self {
        Self::new(self.x.max(o.x), self.y.max(o.y), self.z.max(o.z))
    }

    /// Mirror about a surface normal `n` (unit).
    #[must_use]
    pub fn reflect(self, n: Self) -> Self {
        self - n * (2.0 * self.dot(n))
    }

    #[must_use]
    pub fn max_elem(self) -> f32 {
        self.x.max(self.y).max(self.z)
    }
}

impl Add for Vec3 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl AddAssign for Vec3 {
    fn add_assign(&mut self, o: Self) {
        *self = *self + o;
    }
}
impl Sub for Vec3 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Mul<f32> for Vec3 {
    type Output = Self;
    fn mul(self, s: f32) -> Self {
        Self::new(self.x * s, self.y * s, self.z * s)
    }
}
impl Div<f32> for Vec3 {
    type Output = Self;
    fn div(self, s: f32) -> Self {
        Self::new(self.x / s, self.y / s, self.z / s)
    }
}
impl Neg for Vec3 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
}

/// A rotation as a unit quaternion (x, y, z, w).
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

    /// `angle` radians about `axis` (need not be unit).
    #[must_use]
    pub fn from_axis_angle(axis: Vec3, angle: f32) -> Self {
        let a = axis.normalize();
        let (s, c) = (angle * 0.5).sin_cos();
        Self::new(a.x * s, a.y * s, a.z * s, c)
    }

    /// Euler angles in Three.js' default `'XYZ'` order for
    /// `Object3D.rotation`: the matrix is `Rx · Ry · Rz`.
    #[must_use]
    pub fn from_euler(x: f32, y: f32, z: f32) -> Self {
        Self::from_axis_angle(Vec3::X, x)
            * Self::from_axis_angle(Vec3::Y, y)
            * Self::from_axis_angle(Vec3::Z, z)
    }

    #[must_use]
    pub fn normalize(self) -> Self {
        let l = (self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w).sqrt();
        if l < 1e-12 {
            Self::IDENTITY
        } else {
            Self::new(self.x / l, self.y / l, self.z / l, self.w / l)
        }
    }

    #[must_use]
    pub fn conjugate(self) -> Self {
        Self::new(-self.x, -self.y, -self.z, self.w)
    }

    /// Rotate a vector.
    #[must_use]
    pub fn rotate(self, v: Vec3) -> Vec3 {
        let q = Vec3::new(self.x, self.y, self.z);
        let t = q.cross(v) * 2.0;
        v + t * self.w + q.cross(t)
    }

    /// Spherical interpolation along the shorter arc.
    #[must_use]
    pub fn slerp(self, mut o: Self, t: f32) -> Self {
        let mut d = self.x * o.x + self.y * o.y + self.z * o.z + self.w * o.w;
        if d < 0.0 {
            d = -d;
            o = Self::new(-o.x, -o.y, -o.z, -o.w);
        }
        if d > 0.9995 {
            return Self::new(
                self.x + (o.x - self.x) * t,
                self.y + (o.y - self.y) * t,
                self.z + (o.z - self.z) * t,
                self.w + (o.w - self.w) * t,
            )
            .normalize();
        }
        let theta = d.acos();
        let s = theta.sin();
        let a = ((1.0 - t) * theta).sin() / s;
        let b = (t * theta).sin() / s;
        Self::new(
            self.x * a + o.x * b,
            self.y * a + o.y * b,
            self.z * a + o.z * b,
            self.w * a + o.w * b,
        )
    }
}

impl Mul for Quat {
    type Output = Self;
    /// `self * o` rotates by `o` first, then `self`.
    fn mul(self, o: Self) -> Self {
        Self::new(
            self.w * o.x + self.x * o.w + self.y * o.z - self.z * o.y,
            self.w * o.y - self.x * o.z + self.y * o.w + self.z * o.x,
            self.w * o.z + self.x * o.y - self.y * o.x + self.z * o.w,
            self.w * o.w - self.x * o.x - self.y * o.y - self.z * o.z,
        )
    }
}

/// A 4×4 matrix, column-major: `m[col][row]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4 {
    pub m: [[f32; 4]; 4],
}

impl Default for Mat4 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mat4 {
    pub const IDENTITY: Self = Self {
        m: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    /// From 16 values in column-major order (glTF's `matrix` layout).
    #[must_use]
    pub fn from_cols_array(a: &[f32; 16]) -> Self {
        let mut m = [[0.0; 4]; 4];
        for (c, col) in m.iter_mut().enumerate() {
            col.copy_from_slice(&a[c * 4..c * 4 + 4]);
        }
        Self { m }
    }

    #[must_use]
    pub fn translation(t: Vec3) -> Self {
        let mut r = Self::IDENTITY;
        r.m[3] = [t.x, t.y, t.z, 1.0];
        r
    }

    #[must_use]
    pub fn scale(s: Vec3) -> Self {
        let mut r = Self::IDENTITY;
        r.m[0][0] = s.x;
        r.m[1][1] = s.y;
        r.m[2][2] = s.z;
        r
    }

    #[must_use]
    pub fn rotation(q: Quat) -> Self {
        let q = q.normalize();
        let (x, y, z, w) = (q.x, q.y, q.z, q.w);
        Self {
            m: [
                [
                    1.0 - 2.0 * (y * y + z * z),
                    2.0 * (x * y + z * w),
                    2.0 * (x * z - y * w),
                    0.0,
                ],
                [
                    2.0 * (x * y - z * w),
                    1.0 - 2.0 * (x * x + z * z),
                    2.0 * (y * z + x * w),
                    0.0,
                ],
                [
                    2.0 * (x * z + y * w),
                    2.0 * (y * z - x * w),
                    1.0 - 2.0 * (x * x + y * y),
                    0.0,
                ],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    /// Translation · rotation · scale — `Object3D.matrix`.
    #[must_use]
    pub fn compose(t: Vec3, r: Quat, s: Vec3) -> Self {
        Self::translation(t) * Self::rotation(r) * Self::scale(s)
    }

    /// A right-handed perspective projection (`fov_y` radians), depth
    /// mapped to −1..1 as in OpenGL.
    #[must_use]
    pub fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Self {
        let f = 1.0 / (fov_y / 2.0).tan();
        let mut m = [[0.0; 4]; 4];
        m[0][0] = f / aspect;
        m[1][1] = f;
        m[2][2] = (far + near) / (near - far);
        m[2][3] = -1.0;
        m[3][2] = 2.0 * far * near / (near - far);
        Self { m }
    }

    /// An orthographic projection of the box `left..right`, `bottom..top`,
    /// `near..far` (distances in front of the camera).
    #[must_use]
    pub fn orthographic(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> Self {
        let mut r = Self::IDENTITY;
        r.m[0][0] = 2.0 / (right - left);
        r.m[1][1] = 2.0 / (top - bottom);
        r.m[2][2] = -2.0 / (far - near);
        r.m[3] = [
            -(right + left) / (right - left),
            -(top + bottom) / (top - bottom),
            -(far + near) / (far - near),
            1.0,
        ];
        r
    }

    /// A view matrix: the camera at `eye` looking at `target`.
    #[must_use]
    pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Self {
        let f = (target - eye).normalize();
        let mut s = f.cross(up).normalize();
        if s.length() < 1e-6 {
            // Looking straight along `up`: pick any perpendicular.
            s = f
                .cross(if f.x.abs() < 0.9 { Vec3::X } else { Vec3::Z })
                .normalize();
        }
        let u = s.cross(f);
        Self {
            m: [
                [s.x, u.x, -f.x, 0.0],
                [s.y, u.y, -f.y, 0.0],
                [s.z, u.z, -f.z, 0.0],
                [-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0],
            ],
        }
    }

    /// `M * [v, 1]`, divided by w.
    #[must_use]
    pub fn transform_point(&self, v: Vec3) -> Vec3 {
        let r = self.mul_vec4([v.x, v.y, v.z, 1.0]);
        if r[3].abs() > 1e-12 && (r[3] - 1.0).abs() > 1e-9 {
            Vec3::new(r[0] / r[3], r[1] / r[3], r[2] / r[3])
        } else {
            Vec3::new(r[0], r[1], r[2])
        }
    }

    /// `M * [v, 0]` — directions ignore translation.
    #[must_use]
    pub fn transform_vector(&self, v: Vec3) -> Vec3 {
        let r = self.mul_vec4([v.x, v.y, v.z, 0.0]);
        Vec3::new(r[0], r[1], r[2])
    }

    #[must_use]
    pub fn mul_vec4(&self, v: [f32; 4]) -> [f32; 4] {
        let mut out = [0.0; 4];
        for (row, o) in out.iter_mut().enumerate() {
            *o = self.m[0][row] * v[0]
                + self.m[1][row] * v[1]
                + self.m[2][row] * v[2]
                + self.m[3][row] * v[3];
        }
        out
    }

    #[must_use]
    pub fn transpose(&self) -> Self {
        let mut r = [[0.0; 4]; 4];
        for (c, col) in r.iter_mut().enumerate() {
            for (row, v) in col.iter_mut().enumerate() {
                *v = self.m[row][c];
            }
        }
        Self { m: r }
    }

    /// The translation column.
    #[must_use]
    pub fn get_translation(&self) -> Vec3 {
        Vec3::new(self.m[3][0], self.m[3][1], self.m[3][2])
    }

    /// Largest axis scale — for transforming a bounding radius.
    #[must_use]
    pub fn max_scale(&self) -> f32 {
        let len = |c: usize| Vec3::new(self.m[c][0], self.m[c][1], self.m[c][2]).length();
        len(0).max(len(1)).max(len(2))
    }

    /// General inverse (cofactor expansion); `None` if singular.
    #[must_use]
    pub fn inverse(&self) -> Option<Self> {
        let a: [f32; 16] = {
            let mut a = [0.0; 16];
            for c in 0..4 {
                for r in 0..4 {
                    a[c * 4 + r] = self.m[c][r];
                }
            }
            a
        };
        let mut inv = [0.0f32; 16];
        inv[0] = a[5] * a[10] * a[15] - a[5] * a[11] * a[14] - a[9] * a[6] * a[15]
            + a[9] * a[7] * a[14]
            + a[13] * a[6] * a[11]
            - a[13] * a[7] * a[10];
        inv[4] = -a[4] * a[10] * a[15] + a[4] * a[11] * a[14] + a[8] * a[6] * a[15]
            - a[8] * a[7] * a[14]
            - a[12] * a[6] * a[11]
            + a[12] * a[7] * a[10];
        inv[8] = a[4] * a[9] * a[15] - a[4] * a[11] * a[13] - a[8] * a[5] * a[15]
            + a[8] * a[7] * a[13]
            + a[12] * a[5] * a[11]
            - a[12] * a[7] * a[9];
        inv[12] = -a[4] * a[9] * a[14] + a[4] * a[10] * a[13] + a[8] * a[5] * a[14]
            - a[8] * a[6] * a[13]
            - a[12] * a[5] * a[10]
            + a[12] * a[6] * a[9];
        inv[1] = -a[1] * a[10] * a[15] + a[1] * a[11] * a[14] + a[9] * a[2] * a[15]
            - a[9] * a[3] * a[14]
            - a[13] * a[2] * a[11]
            + a[13] * a[3] * a[10];
        inv[5] = a[0] * a[10] * a[15] - a[0] * a[11] * a[14] - a[8] * a[2] * a[15]
            + a[8] * a[3] * a[14]
            + a[12] * a[2] * a[11]
            - a[12] * a[3] * a[10];
        inv[9] = -a[0] * a[9] * a[15] + a[0] * a[11] * a[13] + a[8] * a[1] * a[15]
            - a[8] * a[3] * a[13]
            - a[12] * a[1] * a[11]
            + a[12] * a[3] * a[9];
        inv[13] = a[0] * a[9] * a[14] - a[0] * a[10] * a[13] - a[8] * a[1] * a[14]
            + a[8] * a[2] * a[13]
            + a[12] * a[1] * a[10]
            - a[12] * a[2] * a[9];
        inv[2] = a[1] * a[6] * a[15] - a[1] * a[7] * a[14] - a[5] * a[2] * a[15]
            + a[5] * a[3] * a[14]
            + a[13] * a[2] * a[7]
            - a[13] * a[3] * a[6];
        inv[6] = -a[0] * a[6] * a[15] + a[0] * a[7] * a[14] + a[4] * a[2] * a[15]
            - a[4] * a[3] * a[14]
            - a[12] * a[2] * a[7]
            + a[12] * a[3] * a[6];
        inv[10] = a[0] * a[5] * a[15] - a[0] * a[7] * a[13] - a[4] * a[1] * a[15]
            + a[4] * a[3] * a[13]
            + a[12] * a[1] * a[7]
            - a[12] * a[3] * a[5];
        inv[14] = -a[0] * a[5] * a[14] + a[0] * a[6] * a[13] + a[4] * a[1] * a[14]
            - a[4] * a[2] * a[13]
            - a[12] * a[1] * a[6]
            + a[12] * a[2] * a[5];
        inv[3] = -a[1] * a[6] * a[11] + a[1] * a[7] * a[10] + a[5] * a[2] * a[11]
            - a[5] * a[3] * a[10]
            - a[9] * a[2] * a[7]
            + a[9] * a[3] * a[6];
        inv[7] = a[0] * a[6] * a[11] - a[0] * a[7] * a[10] - a[4] * a[2] * a[11]
            + a[4] * a[3] * a[10]
            + a[8] * a[2] * a[7]
            - a[8] * a[3] * a[6];
        inv[11] = -a[0] * a[5] * a[11] + a[0] * a[7] * a[9] + a[4] * a[1] * a[11]
            - a[4] * a[3] * a[9]
            - a[8] * a[1] * a[7]
            + a[8] * a[3] * a[5];
        inv[15] = a[0] * a[5] * a[10] - a[0] * a[6] * a[9] - a[4] * a[1] * a[10]
            + a[4] * a[2] * a[9]
            + a[8] * a[1] * a[6]
            - a[8] * a[2] * a[5];
        let det = a[0] * inv[0] + a[1] * inv[4] + a[2] * inv[8] + a[3] * inv[12];
        if det.abs() < 1e-20 {
            return None;
        }
        let d = 1.0 / det;
        let mut m = [[0.0; 4]; 4];
        for c in 0..4 {
            for r in 0..4 {
                m[c][r] = inv[c * 4 + r] * d;
            }
        }
        Some(Self { m })
    }
}

impl Mul for Mat4 {
    type Output = Self;
    fn mul(self, o: Self) -> Self {
        let mut r = [[0.0; 4]; 4];
        for (c, col) in r.iter_mut().enumerate() {
            for (row, v) in col.iter_mut().enumerate() {
                *v = (0..4).map(|k| self.m[k][row] * o.m[c][k]).sum();
            }
        }
        Self { m: r }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-4
    }

    #[test]
    fn quaternion_rotation_matches_the_matrix() {
        let q = Quat::from_axis_angle(Vec3::Y, FRAC_PI_2);
        assert!(close(q.rotate(Vec3::X), Vec3::new(0.0, 0.0, -1.0)));
        assert!(close(
            Mat4::rotation(q).transform_vector(Vec3::X),
            Vec3::new(0.0, 0.0, -1.0)
        ));
    }

    #[test]
    fn compose_then_inverse_is_identity() {
        let m = Mat4::compose(
            Vec3::new(1.0, 2.0, 3.0),
            Quat::from_euler(0.3, 0.7, -0.2),
            Vec3::new(2.0, 1.0, 0.5),
        );
        let id = m * m.inverse().unwrap();
        for c in 0..4 {
            for r in 0..4 {
                let e = if c == r { 1.0 } else { 0.0 };
                assert!((id.m[c][r] - e).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn look_at_puts_the_target_on_negative_z() {
        let v = Mat4::look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
        assert!(close(
            v.transform_point(Vec3::ZERO),
            Vec3::new(0.0, 0.0, -5.0)
        ));
    }

    #[test]
    fn perspective_maps_near_and_far_to_the_depth_range() {
        let p = Mat4::perspective(1.0, 1.0, 1.0, 10.0);
        assert!((p.transform_point(Vec3::new(0.0, 0.0, -1.0)).z + 1.0).abs() < 1e-5);
        assert!((p.transform_point(Vec3::new(0.0, 0.0, -10.0)).z - 1.0).abs() < 1e-4);
    }

    #[test]
    fn slerp_goes_halfway() {
        let a = Quat::IDENTITY;
        let b = Quat::from_axis_angle(Vec3::Z, FRAC_PI_2);
        let h = a.slerp(b, 0.5);
        let expect = Quat::from_axis_angle(Vec3::Z, FRAC_PI_2 / 2.0);
        assert!((h.z - expect.z).abs() < 1e-5 && (h.w - expect.w).abs() < 1e-5);
    }
}
