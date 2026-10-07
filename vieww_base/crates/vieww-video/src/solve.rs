//! Planar tracking, camera solving and corner pinning — After Effects'
//! *3D Camera Tracker* and *Corner Pin*, Mocha's planar tracker,
//! TouchDesigner's projection-mapping `Stoner`/`Kantan` (§2.15 L7, §2.8).
//!
//! * [`homography`] — the direct linear transform with Hartley
//!   normalisation, least-squares over any number of correspondences
//!   (solved by Gaussian elimination on the normal equations with the
//!   `h₃₃ = 1` gauge).
//! * [`ransac_homography`] — robust estimation: minimal 4-point samples,
//!   inlier consensus by reprojection error, final refit on the inliers.
//! * [`solve_planar_camera`] — Zhang's decomposition of a plane-to-image
//!   homography given intrinsics: the camera's rotation and translation
//!   relative to the tracked plane (orthonormalised), i.e. a 3D camera
//!   solved from a 2D track.
//! * [`corner_pin`] — warp an image so its corners land on four target
//!   points, by inverse mapping with bilinear sampling: projection mapping
//!   onto a surface, a screen replacement, a perspective insert.

use vieww_foundation::{Image, Offset};

/// A 3×3 projective transform, row-major, `h[8] = 1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Homography(pub [f64; 9]);

impl Homography {
    pub const IDENTITY: Self = Self([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);

    /// Map a point.
    #[must_use]
    pub fn apply(&self, p: Offset) -> Offset {
        let h = &self.0;
        let (x, y) = (f64::from(p.dx), f64::from(p.dy));
        let w = h[6] * x + h[7] * y + h[8];
        #[allow(clippy::cast_possible_truncation)]
        Offset::new(
            ((h[0] * x + h[1] * y + h[2]) / w) as f32,
            ((h[3] * x + h[4] * y + h[5]) / w) as f32,
        )
    }

    /// The inverse transform.
    #[must_use]
    pub fn inverse(&self) -> Option<Self> {
        let m = &self.0;
        let det = m[0] * (m[4] * m[8] - m[5] * m[7]) - m[1] * (m[3] * m[8] - m[5] * m[6])
            + m[2] * (m[3] * m[7] - m[4] * m[6]);
        if det.abs() < 1e-15 {
            return None;
        }
        let inv = [
            (m[4] * m[8] - m[5] * m[7]) / det,
            (m[2] * m[7] - m[1] * m[8]) / det,
            (m[1] * m[5] - m[2] * m[4]) / det,
            (m[5] * m[6] - m[3] * m[8]) / det,
            (m[0] * m[8] - m[2] * m[6]) / det,
            (m[2] * m[3] - m[0] * m[5]) / det,
            (m[3] * m[7] - m[4] * m[6]) / det,
            (m[1] * m[6] - m[0] * m[7]) / det,
            (m[0] * m[4] - m[1] * m[3]) / det,
        ];
        let s = inv[8];
        Some(Self(inv.map(|v| v / s)))
    }

    /// `self ∘ other` (apply `other`, then `self`).
    #[must_use]
    pub fn then_after(&self, other: &Self) -> Self {
        let (a, b) = (&self.0, &other.0);
        let mut r = [0.0; 9];
        for i in 0..3 {
            for j in 0..3 {
                r[i * 3 + j] = (0..3).map(|k| a[i * 3 + k] * b[k * 3 + j]).sum();
            }
        }
        let s = r[8];
        Self(r.map(|v| v / s))
    }
}

/// Similarity that moves points to zero mean and √2 mean distance.
fn normaliser(pts: &[Offset]) -> [f64; 9] {
    #[allow(clippy::cast_precision_loss)]
    let n = pts.len() as f64;
    let cx = pts.iter().map(|p| f64::from(p.dx)).sum::<f64>() / n;
    let cy = pts.iter().map(|p| f64::from(p.dy)).sum::<f64>() / n;
    let d = pts
        .iter()
        .map(|p| ((f64::from(p.dx) - cx).powi(2) + (f64::from(p.dy) - cy).powi(2)).sqrt())
        .sum::<f64>()
        / n;
    let s = if d > 1e-12 { std::f64::consts::SQRT_2 / d } else { 1.0 };
    [s, 0.0, -s * cx, 0.0, s, -s * cy, 0.0, 0.0, 1.0]
}

fn solve8(mut a: [[f64; 9]; 8]) -> Option<[f64; 8]> {
    for c in 0..8 {
        let p = (c..8).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))?;
        if a[p][c].abs() < 1e-12 {
            return None;
        }
        a.swap(c, p);
        for r in 0..8 {
            if r != c {
                let f = a[r][c] / a[c][c];
                for k in c..9 {
                    a[r][k] -= f * a[c][k];
                }
            }
        }
    }
    let mut x = [0.0; 8];
    for i in 0..8 {
        x[i] = a[i][8] / a[i][i];
    }
    Some(x)
}

/// Least-squares homography mapping `src[i]` to `dst[i]` (≥ 4 pairs).
#[must_use]
pub fn homography(src: &[Offset], dst: &[Offset]) -> Option<Homography> {
    if src.len() < 4 || src.len() != dst.len() {
        return None;
    }
    let (ts, td) = (normaliser(src), normaliser(dst));
    let tf = |t: &[f64; 9], p: Offset| {
        (
            t[0] * f64::from(p.dx) + t[2],
            t[4] * f64::from(p.dy) + t[5],
        )
    };
    // Normal equations AᵀA h = Aᵀb for the 8 unknowns.
    let mut ata = [[0.0f64; 9]; 8];
    for (p, q) in src.iter().zip(dst) {
        let (x, y) = tf(&ts, *p);
        let (u, v) = tf(&td, *q);
        let rows = [
            ([x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y], u),
            ([0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y], v),
        ];
        for (r, b) in rows {
            for i in 0..8 {
                for j in 0..8 {
                    ata[i][j] += r[i] * r[j];
                }
                ata[i][8] += r[i] * b;
            }
        }
    }
    let h = solve8(ata)?;
    let hn = Homography([h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7], 1.0]);
    // Denormalise: H = Td⁻¹ · Hn · Ts.
    let tdi = Homography(td).inverse()?;
    Some(tdi.then_after(&hn).then_after(&Homography(ts)))
}

/// RANSAC result.
#[derive(Debug, Clone, PartialEq)]
pub struct RansacFit {
    pub h: Homography,
    pub inliers: Vec<bool>,
}

/// Robust homography: `iterations` minimal samples (seeded), inliers within
/// `threshold` pixels of reprojection, refit on the best consensus.
#[must_use]
pub fn ransac_homography(
    src: &[Offset],
    dst: &[Offset],
    threshold: f32,
    iterations: usize,
    seed: u64,
) -> Option<RansacFit> {
    let n = src.len();
    if n < 4 || n != dst.len() {
        return None;
    }
    let mut state = seed | 1;
    let mut rnd = |m: usize| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        #[allow(clippy::cast_possible_truncation)]
        let v = (state % m as u64) as usize;
        v
    };
    let score = |h: &Homography| -> Vec<bool> {
        src.iter()
            .zip(dst)
            .map(|(p, q)| {
                let r = h.apply(*p);
                (r.dx - q.dx).powi(2) + (r.dy - q.dy).powi(2) <= threshold * threshold
            })
            .collect()
    };
    let mut best: Option<(usize, Homography, Vec<bool>)> = None;
    for _ in 0..iterations {
        let mut idx = [0usize; 4];
        for k in 0..4 {
            loop {
                let c = rnd(n);
                if !idx[..k].contains(&c) {
                    idx[k] = c;
                    break;
                }
            }
        }
        let (s4, d4): (Vec<Offset>, Vec<Offset>) = idx.iter().map(|&i| (src[i], dst[i])).unzip();
        let Some(h) = homography(&s4, &d4) else { continue };
        let inl = score(&h);
        let c = inl.iter().filter(|&&b| b).count();
        if best.as_ref().is_none_or(|b| c > b.0) {
            best = Some((c, h, inl));
        }
    }
    let (_, h, inl) = best?;
    let (s, d): (Vec<Offset>, Vec<Offset>) = src
        .iter()
        .zip(dst)
        .zip(&inl)
        .filter(|(_, &k)| k)
        .map(|((a, b), _)| (*a, *b))
        .unzip();
    let h = homography(&s, &d).unwrap_or(h);
    let inliers = score(&h);
    Some(RansacFit { h, inliers })
}

/// Pinhole intrinsics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Intrinsics {
    pub fx: f64,
    pub fy: f64,
    pub cx: f64,
    pub cy: f64,
}

impl Intrinsics {
    /// Square pixels, principal point at the centre, from a vertical fov.
    #[must_use]
    pub fn from_fov(width: u32, height: u32, fov_y: f64) -> Self {
        let f = f64::from(height) / (2.0 * (fov_y * 0.5).tan());
        Self {
            fx: f,
            fy: f,
            cx: f64::from(width) * 0.5,
            cy: f64::from(height) * 0.5,
        }
    }
}

/// A camera pose: `x_cam = R · x_plane + t`, with the plane at `z = 0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    /// Row-major rotation.
    pub r: [[f64; 3]; 3],
    pub t: [f64; 3],
}

impl Pose {
    /// Project a plane point `(X, Y, 0)` to pixels.
    #[must_use]
    pub fn project(&self, k: &Intrinsics, x: f64, y: f64) -> Offset {
        let c = [
            self.r[0][0] * x + self.r[0][1] * y + self.t[0],
            self.r[1][0] * x + self.r[1][1] * y + self.t[1],
            self.r[2][0] * x + self.r[2][1] * y + self.t[2],
        ];
        #[allow(clippy::cast_possible_truncation)]
        Offset::new(
            (k.fx * c[0] / c[2] + k.cx) as f32,
            (k.fy * c[1] / c[2] + k.cy) as f32,
        )
    }

    /// The camera centre in plane coordinates: `−Rᵀ t`.
    #[must_use]
    pub fn center(&self) -> [f64; 3] {
        let mut c = [0.0; 3];
        for i in 0..3 {
            c[i] = -(0..3).map(|j| self.r[j][i] * self.t[j]).sum::<f64>();
        }
        c
    }
}

/// Zhang's decomposition: the pose of a camera with intrinsics `k` that sees
/// the plane `z = 0` through homography `h` (plane units → pixels).
#[must_use]
pub fn solve_planar_camera(h: &Homography, k: &Intrinsics) -> Option<Pose> {
    let m = &h.0;
    // K⁻¹ H, column by column.
    let kinv = |c: [f64; 3]| [(c[0] - k.cx * c[2]) / k.fx, (c[1] - k.cy * c[2]) / k.fy, c[2]];
    let h1 = kinv([m[0], m[3], m[6]]);
    let h2 = kinv([m[1], m[4], m[7]]);
    let h3 = kinv([m[2], m[5], m[8]]);
    let n = |v: [f64; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let lambda = 2.0 / (n(h1) + n(h2));
    if !lambda.is_finite() {
        return None;
    }
    let mut r1 = h1.map(|v| v * lambda);
    let mut r2 = h2.map(|v| v * lambda);
    let mut t = h3.map(|v| v * lambda);
    if t[2] < 0.0 {
        // The plane must be in front of the camera.
        r1 = r1.map(|v| -v);
        r2 = r2.map(|v| -v);
        t = t.map(|v| -v);
    }
    // Orthonormalise (Gram–Schmidt on r1, r2; r3 = r1 × r2).
    let nr1 = n(r1);
    r1 = r1.map(|v| v / nr1);
    let d = r1[0] * r2[0] + r1[1] * r2[1] + r1[2] * r2[2];
    r2 = [r2[0] - d * r1[0], r2[1] - d * r1[1], r2[2] - d * r1[2]];
    let nr2 = n(r2);
    r2 = r2.map(|v| v / nr2);
    let r3 = [
        r1[1] * r2[2] - r1[2] * r2[1],
        r1[2] * r2[0] - r1[0] * r2[2],
        r1[0] * r2[1] - r1[1] * r2[0],
    ];
    Some(Pose {
        r: [[r1[0], r2[0], r3[0]], [r1[1], r2[1], r3[1]], [r1[2], r2[2], r3[2]]],
        t,
    })
}

/// Warp `src` so its corners land on `quad` (TL, TR, BR, BL) in an output of
/// `out_w × out_h`; pixels outside the quad are transparent.
#[must_use]
pub fn corner_pin(src: &Image, quad: [Offset; 4], out_w: u32, out_h: u32) -> Image {
    #[allow(clippy::cast_precision_loss)]
    let (sw, sh) = (src.width() as f32, src.height() as f32);
    let corners = [
        Offset::new(0.0, 0.0),
        Offset::new(sw, 0.0),
        Offset::new(sw, sh),
        Offset::new(0.0, sh),
    ];
    let mut px = vec![0u8; out_w as usize * out_h as usize * 4];
    let Some(inv) = homography(&corners, &quad).and_then(|h| h.inverse()) else {
        return Image::from_rgba8(px, out_w, out_h);
    };
    let sp = src.pixels();
    let (iw, ih) = (src.width() as usize, src.height() as usize);
    for y in 0..out_h as usize {
        for x in 0..out_w as usize {
            #[allow(clippy::cast_precision_loss)]
            let s = inv.apply(Offset::new(x as f32 + 0.5, y as f32 + 0.5));
            let (fx, fy) = (s.dx - 0.5, s.dy - 0.5);
            if fx < -0.5 || fy < -0.5 || fx > sw - 0.5 || fy > sh - 0.5 {
                continue;
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let (x0, y0) = (fx.floor().max(0.0) as usize, fy.floor().max(0.0) as usize);
            let (x1, y1) = ((x0 + 1).min(iw - 1), (y0 + 1).min(ih - 1));
            #[allow(clippy::cast_precision_loss)]
            let (ax, ay) = ((fx - x0 as f32).clamp(0.0, 1.0), (fy - y0 as f32).clamp(0.0, 1.0));
            let o = (y * out_w as usize + x) * 4;
            for c in 0..4 {
                let g = |xx: usize, yy: usize| f32::from(sp[(yy * iw + xx) * 4 + c]);
                let v = (g(x0, y0) * (1.0 - ax) + g(x1, y0) * ax) * (1.0 - ay)
                    + (g(x0, y1) * (1.0 - ax) + g(x1, y1) * ax) * ay;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                {
                    px[o + c] = (v + 0.5).clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
    Image::from_rgba8(px, out_w, out_h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn o(x: f32, y: f32) -> Offset {
        Offset::new(x, y)
    }

    #[test]
    fn four_points_define_the_homography_exactly() {
        let src = [o(0.0, 0.0), o(100.0, 0.0), o(100.0, 100.0), o(0.0, 100.0)];
        let dst = [o(10.0, 20.0), o(130.0, 5.0), o(120.0, 140.0), o(-5.0, 110.0)];
        let h = homography(&src, &dst).unwrap();
        for (s, d) in src.iter().zip(&dst) {
            let r = h.apply(*s);
            assert!((r.dx - d.dx).abs() < 1e-3 && (r.dy - d.dy).abs() < 1e-3);
        }
        let back = h.inverse().unwrap().apply(dst[2]);
        assert!((back.dx - 100.0).abs() < 1e-3);
    }

    #[test]
    fn ransac_ignores_outliers() {
        let truth = Homography([1.1, 0.05, 12.0, -0.03, 0.95, -7.0, 0.0004, -0.0002, 1.0]);
        let mut src = Vec::new();
        let mut dst = Vec::new();
        for i in 0..60 {
            #[allow(clippy::cast_precision_loss)]
            let p = o((i % 10) as f32 * 30.0, (i / 10) as f32 * 40.0);
            src.push(p);
            dst.push(truth.apply(p));
        }
        for i in (0..60).step_by(5) {
            dst[i] = o(dst[i].dx + 80.0, dst[i].dy - 50.0);
        }
        let fit = ransac_homography(&src, &dst, 1.0, 300, 9).unwrap();
        assert_eq!(fit.inliers.iter().filter(|&&b| b).count(), 48);
        let p = fit.h.apply(o(77.0, 33.0));
        let q = truth.apply(o(77.0, 33.0));
        assert!((p.dx - q.dx).abs() < 0.05 && (p.dy - q.dy).abs() < 0.05);
    }

    #[test]
    fn a_planar_track_solves_the_camera() {
        let k = Intrinsics::from_fov(640, 480, 0.9);
        // A known camera: rotated about x and y, 5 units from the plane.
        let (ax, ay) = (0.4f64, -0.3f64);
        let rx = [[1.0, 0.0, 0.0], [0.0, ax.cos(), -ax.sin()], [0.0, ax.sin(), ax.cos()]];
        let ry = [[ay.cos(), 0.0, ay.sin()], [0.0, 1.0, 0.0], [-ay.sin(), 0.0, ay.cos()]];
        let mut r = [[0.0; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                r[i][j] = (0..3).map(|q| rx[i][q] * ry[q][j]).sum();
            }
        }
        let truth = Pose { r, t: [0.3, -0.2, 5.0] };
        let plane: Vec<Offset> = (0..16)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                o((i % 4) as f32 - 1.5, (i / 4) as f32 - 1.5)
            })
            .collect();
        let img: Vec<Offset> = plane
            .iter()
            .map(|p| truth.project(&k, f64::from(p.dx), f64::from(p.dy)))
            .collect();
        let h = homography(&plane, &img).unwrap();
        let pose = solve_planar_camera(&h, &k).unwrap();
        for i in 0..3 {
            assert!((pose.t[i] - truth.t[i]).abs() < 1e-3, "{:?}", pose.t);
            for j in 0..3 {
                assert!((pose.r[i][j] - truth.r[i][j]).abs() < 1e-3);
            }
        }
        let c = pose.center();
        let ct = truth.center();
        assert!((c[2] - ct[2]).abs() < 1e-3);
    }

    #[test]
    fn corner_pin_lands_corners() {
        let px: Vec<u8> = (0..16 * 16).flat_map(|_| [200u8, 100, 50, 255]).collect();
        let src = Image::from_rgba8(px, 16, 16);
        let quad = [o(10.0, 10.0), o(50.0, 14.0), o(46.0, 52.0), o(8.0, 44.0)];
        let out = corner_pin(&src, quad, 64, 64);
        let p = |x: usize, y: usize| &out.pixels()[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
        assert_eq!(p(28, 30), &[200, 100, 50, 255], "inside the quad");
        assert_eq!(p(2, 2)[3], 0, "outside is transparent");
        assert_eq!(p(60, 60)[3], 0);
    }
}
