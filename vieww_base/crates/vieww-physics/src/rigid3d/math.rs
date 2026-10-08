//! The 3D vector types the rigid-body engine speaks: [`V3`], [`Q`] (a unit
//! quaternion) and [`M3`] (a 3×3 matrix, row-major, acting on column
//! vectors). Self-contained so the physics crate keeps its one dependency.

use std::ops::{Add, AddAssign, Div, Mul, MulAssign, Neg, Sub, SubAssign};

/// A 3-vector.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct V3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl V3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
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
    pub fn length_sq(self) -> f32 {
        self.dot(self)
    }
    #[must_use]
    pub fn length(self) -> f32 {
        self.length_sq().sqrt()
    }
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
    pub fn min(self, o: Self) -> Self {
        Self::new(self.x.min(o.x), self.y.min(o.y), self.z.min(o.z))
    }
    #[must_use]
    pub fn max(self, o: Self) -> Self {
        Self::new(self.x.max(o.x), self.y.max(o.y), self.z.max(o.z))
    }
    #[must_use]
    pub fn abs(self) -> Self {
        Self::new(self.x.abs(), self.y.abs(), self.z.abs())
    }
    #[must_use]
    pub fn mul_elem(self, o: Self) -> Self {
        Self::new(self.x * o.x, self.y * o.y, self.z * o.z)
    }
    #[must_use]
    pub fn lerp(self, o: Self, t: f32) -> Self {
        self + (o - self) * t
    }
    #[must_use]
    pub fn get(self, i: usize) -> f32 {
        match i {
            0 => self.x,
            1 => self.y,
            _ => self.z,
        }
    }
    /// Two unit vectors completing an orthonormal basis with `self` (unit).
    #[must_use]
    pub fn basis(self) -> (Self, Self) {
        let a = if self.x.abs() > 0.57 {
            Self::new(self.y, -self.x, 0.0)
        } else {
            Self::new(0.0, self.z, -self.y)
        }
        .normalize();
        (a, self.cross(a))
    }
}

impl Add for V3 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl Sub for V3 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Mul<f32> for V3 {
    type Output = Self;
    fn mul(self, s: f32) -> Self {
        Self::new(self.x * s, self.y * s, self.z * s)
    }
}
impl Mul<V3> for f32 {
    type Output = V3;
    fn mul(self, v: V3) -> V3 {
        v * self
    }
}
impl Div<f32> for V3 {
    type Output = Self;
    fn div(self, s: f32) -> Self {
        Self::new(self.x / s, self.y / s, self.z / s)
    }
}
impl Neg for V3 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
}
impl AddAssign for V3 {
    fn add_assign(&mut self, o: Self) {
        *self = *self + o;
    }
}
impl SubAssign for V3 {
    fn sub_assign(&mut self, o: Self) {
        *self = *self - o;
    }
}
impl MulAssign<f32> for V3 {
    fn mul_assign(&mut self, s: f32) {
        *self = *self * s;
    }
}

/// A rotation quaternion `w + xi + yj + zk`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Q {
    pub w: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Default for Q {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Q {
    pub const IDENTITY: Self = Self {
        w: 1.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    #[must_use]
    pub fn from_axis_angle(axis: V3, angle: f32) -> Self {
        let a = axis.normalize();
        let (s, c) = (angle * 0.5).sin_cos();
        Self {
            w: c,
            x: a.x * s,
            y: a.y * s,
            z: a.z * s,
        }
    }

    /// The shortest rotation taking unit `a` to unit `b`.
    #[must_use]
    pub fn between(a: V3, b: V3) -> Self {
        let d = a.dot(b);
        if d < -0.999_999 {
            let (axis, _) = a.basis();
            return Self::from_axis_angle(axis, std::f32::consts::PI);
        }
        let c = a.cross(b);
        Self {
            w: 1.0 + d,
            x: c.x,
            y: c.y,
            z: c.z,
        }
        .normalize()
    }

    #[must_use]
    pub fn normalize(self) -> Self {
        let l = (self.w * self.w + self.x * self.x + self.y * self.y + self.z * self.z).sqrt();
        if l < 1e-12 {
            return Self::IDENTITY;
        }
        Self {
            w: self.w / l,
            x: self.x / l,
            y: self.y / l,
            z: self.z / l,
        }
    }

    #[must_use]
    pub fn conjugate(self) -> Self {
        Self {
            w: self.w,
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }

    #[must_use]
    pub fn rotate(self, v: V3) -> V3 {
        let u = V3::new(self.x, self.y, self.z);
        let t = u.cross(v) * 2.0;
        v + t * self.w + u.cross(t)
    }

    /// Integrate angular velocity `w` over `dt`.
    #[must_use]
    pub fn integrate(self, w: V3, dt: f32) -> Self {
        let dq = Self {
            w: 0.0,
            x: w.x,
            y: w.y,
            z: w.z,
        } * self;
        Self {
            w: self.w + 0.5 * dt * dq.w,
            x: self.x + 0.5 * dt * dq.x,
            y: self.y + 0.5 * dt * dq.y,
            z: self.z + 0.5 * dt * dq.z,
        }
        .normalize()
    }

    #[must_use]
    pub fn to_m3(self) -> M3 {
        let (w, x, y, z) = (self.w, self.x, self.y, self.z);
        M3 {
            m: [
                [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - w * z), 2.0 * (x * z + w * y)],
                [2.0 * (x * y + w * z), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - w * x)],
                [2.0 * (x * z - w * y), 2.0 * (y * z + w * x), 1.0 - 2.0 * (x * x + y * y)],
            ],
        }
    }

    /// Rotation vector (axis × angle) of this rotation, angle in −π..π.
    #[must_use]
    pub fn to_rotation_vector(self) -> V3 {
        let q = if self.w < 0.0 {
            Self {
                w: -self.w,
                x: -self.x,
                y: -self.y,
                z: -self.z,
            }
        } else {
            self
        };
        let v = V3::new(q.x, q.y, q.z);
        let s = v.length();
        if s < 1e-7 {
            return v * 2.0;
        }
        let angle = 2.0 * s.atan2(q.w);
        v * (angle / s)
    }
}

impl Mul for Q {
    type Output = Self;
    fn mul(self, o: Self) -> Self {
        Self {
            w: self.w * o.w - self.x * o.x - self.y * o.y - self.z * o.z,
            x: self.w * o.x + self.x * o.w + self.y * o.z - self.z * o.y,
            y: self.w * o.y - self.x * o.z + self.y * o.w + self.z * o.x,
            z: self.w * o.z + self.x * o.y - self.y * o.x + self.z * o.w,
        }
    }
}

/// A 3×3 matrix, `m[row][col]`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct M3 {
    pub m: [[f32; 3]; 3],
}

impl M3 {
    pub const IDENTITY: Self = Self {
        m: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };
    pub const ZERO: Self = Self { m: [[0.0; 3]; 3] };

    #[must_use]
    pub fn diagonal(d: V3) -> Self {
        Self {
            m: [[d.x, 0.0, 0.0], [0.0, d.y, 0.0], [0.0, 0.0, d.z]],
        }
    }

    #[must_use]
    pub fn mul_v(&self, v: V3) -> V3 {
        let m = &self.m;
        V3::new(
            m[0][0] * v.x + m[0][1] * v.y + m[0][2] * v.z,
            m[1][0] * v.x + m[1][1] * v.y + m[1][2] * v.z,
            m[2][0] * v.x + m[2][1] * v.y + m[2][2] * v.z,
        )
    }

    #[must_use]
    pub fn transpose(&self) -> Self {
        let m = &self.m;
        Self {
            m: [
                [m[0][0], m[1][0], m[2][0]],
                [m[0][1], m[1][1], m[2][1]],
                [m[0][2], m[1][2], m[2][2]],
            ],
        }
    }

    #[must_use]
    pub fn mul(&self, o: &Self) -> Self {
        let mut r = Self::ZERO;
        for i in 0..3 {
            for j in 0..3 {
                r.m[i][j] = (0..3).map(|k| self.m[i][k] * o.m[k][j]).sum();
            }
        }
        r
    }

    #[must_use]
    pub fn add(&self, o: &Self) -> Self {
        let mut r = *self;
        for i in 0..3 {
            for j in 0..3 {
                r.m[i][j] += o.m[i][j];
            }
        }
        r
    }

    #[must_use]
    pub fn scale(&self, s: f32) -> Self {
        let mut r = *self;
        for row in &mut r.m {
            for v in row {
                *v *= s;
            }
        }
        r
    }

    #[must_use]
    pub fn determinant(&self) -> f32 {
        let m = &self.m;
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    }

    #[must_use]
    pub fn inverse(&self) -> Option<Self> {
        let d = self.determinant();
        if d.abs() < 1e-20 {
            return None;
        }
        let m = &self.m;
        let inv = 1.0 / d;
        Some(Self {
            m: [
                [
                    (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * inv,
                    (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * inv,
                    (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * inv,
                ],
                [
                    (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * inv,
                    (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * inv,
                    (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * inv,
                ],
                [
                    (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * inv,
                    (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * inv,
                    (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * inv,
                ],
            ],
        })
    }

    /// The skew matrix `[v]×` with `[v]× w = v × w`.
    #[must_use]
    pub fn skew(v: V3) -> Self {
        Self {
            m: [[0.0, -v.z, v.y], [v.z, 0.0, -v.x], [-v.y, v.x, 0.0]],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quaternion_rotation_matches_its_matrix() {
        let q = Q::from_axis_angle(V3::new(1.0, 2.0, 3.0), 0.7);
        let v = V3::new(0.3, -1.2, 2.0);
        let a = q.rotate(v);
        let b = q.to_m3().mul_v(v);
        assert!((a - b).length() < 1e-5);
    }

    #[test]
    fn between_takes_a_to_b() {
        let a = V3::new(1.0, 0.0, 0.0);
        let b = V3::new(0.0, 0.6, 0.8);
        assert!((Q::between(a, b).rotate(a) - b).length() < 1e-5);
        assert!((Q::between(a, -a).rotate(a) + a).length() < 1e-5);
    }

    #[test]
    fn matrix_inverse_inverts() {
        let m = Q::from_axis_angle(V3::Y, 0.4).to_m3().mul(&M3::diagonal(V3::new(2.0, 3.0, 4.0)));
        let i = m.inverse().expect("invertible");
        let p = m.mul(&i);
        for r in 0..3 {
            for c in 0..3 {
                let e = if r == c { 1.0 } else { 0.0 };
                assert!((p.m[r][c] - e).abs() < 1e-5);
            }
        }
    }

    #[test]
    fn integrating_spin_turns_by_omega_dt() {
        let mut q = Q::IDENTITY;
        for _ in 0..1000 {
            q = q.integrate(V3::new(0.0, 1.0, 0.0), 0.001);
        }
        let rv = q.to_rotation_vector();
        assert!((rv.y - 1.0).abs() < 1e-3, "{rv:?}");
    }
}
