//! The computer-vision toolkit the compositing tools stand on — the
//! OpenCV-shaped row of the comparison (Processing's OpenCV library,
//! openFrameworks' `ofxOpenCv`, After Effects' trackers).
//!
//! * [`Plane`] — a single-channel `f32` image with bilinear sampling,
//!   Sobel gradients and a Gaussian [`pyramid`].
//! * [`good_features`] — Shi–Tomasi corners (minimum eigenvalue of the
//!   structure tensor) with non-maximum suppression and spacing.
//! * [`lucas_kanade`] — pyramidal, iterative Lucas–Kanade sparse optical
//!   flow (Bouguet's formulation) with a per-point status.
//! * [`horn_schunck`] — dense optical flow by Horn–Schunck's global
//!   smoothness, multi-scale.
//! * [`blobs`] — connected components (two-pass union–find) with area,
//!   centroid and bounding box.

use std::collections::BTreeMap;

use vieww_foundation::{Image, Offset};

/// A single-channel float image.
#[derive(Debug, Clone, PartialEq)]
pub struct Plane {
    pub w: usize,
    pub h: usize,
    pub data: Vec<f32>,
}

impl Plane {
    #[must_use]
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            data: vec![0.0; w * h],
        }
    }

    /// Rec. 709 luma in `0..1`.
    #[must_use]
    pub fn luma(img: &Image) -> Self {
        let (w, h) = (img.width() as usize, img.height() as usize);
        let data = img
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| {
                (0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2]))
                    / 255.0
            })
            .collect();
        Self { w, h, data }
    }

    /// Grey RGBA image of the plane (clamped to `0..1`).
    #[must_use]
    pub fn to_image(&self) -> Image {
        let px = self
            .data
            .iter()
            .flat_map(|v| {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let g = (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                [g, g, g, 255]
            })
            .collect();
        #[allow(clippy::cast_possible_truncation)]
        Image::from_rgba8(px, self.w as u32, self.h as u32)
    }

    #[must_use]
    pub fn at(&self, x: usize, y: usize) -> f32 {
        self.data[y.min(self.h - 1) * self.w + x.min(self.w - 1)]
    }

    /// Bilinear sample with clamped edges.
    #[must_use]
    pub fn sample(&self, x: f32, y: f32) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        let (x, y) = (
            x.clamp(0.0, (self.w - 1) as f32),
            y.clamp(0.0, (self.h - 1) as f32),
        );
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (x0, y0) = (x as usize, y as usize);
        #[allow(clippy::cast_precision_loss)]
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let a = self.at(x0, y0) + (self.at(x0 + 1, y0) - self.at(x0, y0)) * fx;
        let b = self.at(x0, y0 + 1) + (self.at(x0 + 1, y0 + 1) - self.at(x0, y0 + 1)) * fx;
        a + (b - a) * fy
    }

    /// Sobel gradients `(gx, gy)`, scaled to units per pixel.
    #[must_use]
    pub fn gradients(&self) -> (Self, Self) {
        let mut gx = Self::new(self.w, self.h);
        let mut gy = Self::new(self.w, self.h);
        for y in 0..self.h {
            for x in 0..self.w {
                let p = |dx: isize, dy: isize| {
                    let xx = (x as isize + dx).clamp(0, self.w as isize - 1);
                    let yy = (y as isize + dy).clamp(0, self.h as isize - 1);
                    #[allow(clippy::cast_sign_loss)]
                    self.data[yy as usize * self.w + xx as usize]
                };
                gx.data[y * self.w + x] =
                    (p(1, -1) + 2.0 * p(1, 0) + p(1, 1) - p(-1, -1) - 2.0 * p(-1, 0) - p(-1, 1))
                        / 8.0;
                gy.data[y * self.w + x] =
                    (p(-1, 1) + 2.0 * p(0, 1) + p(1, 1) - p(-1, -1) - 2.0 * p(0, -1) - p(1, -1))
                        / 8.0;
            }
        }
        (gx, gy)
    }

    /// Separable 5-tap binomial blur (≈ Gaussian σ = 1).
    #[must_use]
    pub fn blur(&self) -> Self {
        const K: [f32; 5] = [1.0 / 16.0, 4.0 / 16.0, 6.0 / 16.0, 4.0 / 16.0, 1.0 / 16.0];
        let mut t = Self::new(self.w, self.h);
        for y in 0..self.h {
            for x in 0..self.w {
                t.data[y * self.w + x] = (0..5)
                    .map(|k| K[k] * self.at((x + k).saturating_sub(2), y))
                    .sum();
            }
        }
        let mut o = Self::new(self.w, self.h);
        for y in 0..self.h {
            for x in 0..self.w {
                o.data[y * self.w + x] = (0..5)
                    .map(|k| K[k] * t.at(x, (y + k).saturating_sub(2)))
                    .sum();
            }
        }
        o
    }

    /// Blur, then keep every other pixel.
    #[must_use]
    pub fn half(&self) -> Self {
        let b = self.blur();
        let (w, h) = (self.w.div_ceil(2).max(1), self.h.div_ceil(2).max(1));
        let mut o = Self::new(w, h);
        for y in 0..h {
            for x in 0..w {
                o.data[y * w + x] = b.at(x * 2, y * 2);
            }
        }
        o
    }
}

/// `levels` planes, finest first.
#[must_use]
pub fn pyramid(p: &Plane, levels: usize) -> Vec<Plane> {
    let mut v = vec![p.clone()];
    while v.len() < levels.max(1) {
        let last = v.last().expect("non-empty");
        if last.w < 16 || last.h < 16 {
            break;
        }
        v.push(last.half());
    }
    v
}

/// Shi–Tomasi corners: up to `max` points whose structure-tensor minimum
/// eigenvalue exceeds `quality ×` the best, at least `min_distance` apart.
#[must_use]
pub fn good_features(p: &Plane, max: usize, quality: f32, min_distance: f32) -> Vec<Offset> {
    let (gx, gy) = p.gradients();
    let mut score = Plane::new(p.w, p.h);
    let r = 2usize;
    for y in r..p.h.saturating_sub(r) {
        for x in r..p.w.saturating_sub(r) {
            let (mut a, mut b, mut c) = (0.0, 0.0, 0.0);
            for yy in y - r..=y + r {
                for xx in x - r..=x + r {
                    let (ix, iy) = (gx.at(xx, yy), gy.at(xx, yy));
                    a += ix * ix;
                    b += ix * iy;
                    c += iy * iy;
                }
            }
            score.data[y * p.w + x] = 0.5 * ((a + c) - ((a - c).powi(2) + 4.0 * b * b).sqrt());
        }
    }
    let best = score.data.iter().copied().fold(0.0f32, f32::max);
    let mut cands: Vec<(f32, usize, usize)> = Vec::new();
    for y in 1..p.h.saturating_sub(1) {
        for x in 1..p.w.saturating_sub(1) {
            let s = score.at(x, y);
            if s <= quality * best || s <= 0.0 {
                continue;
            }
            let local_max = (0..3).all(|dy| {
                (0..3).all(|dx| (dx == 1 && dy == 1) || score.at(x + dx - 1, y + dy - 1) <= s)
            });
            if local_max {
                cands.push((s, x, y));
            }
        }
    }
    cands.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut out: Vec<Offset> = Vec::new();
    for (_, x, y) in cands {
        #[allow(clippy::cast_precision_loss)]
        let o = Offset::new(x as f32, y as f32);
        if out
            .iter()
            .all(|q| (q.dx - o.dx).powi(2) + (q.dy - o.dy).powi(2) >= min_distance * min_distance)
        {
            out.push(o);
            if out.len() == max {
                break;
            }
        }
    }
    out
}

/// One tracked point's result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlowPoint {
    pub from: Offset,
    pub to: Offset,
    /// False when the tracker lost it (singular tensor or left the frame).
    pub ok: bool,
    /// Mean absolute intensity residual in the final window.
    pub error: f32,
}

/// Pyramidal Lucas–Kanade: track `points` from `prev` to `next`.
#[must_use]
pub fn lucas_kanade(
    prev: &Plane,
    next: &Plane,
    points: &[Offset],
    window: usize,
    levels: usize,
) -> Vec<FlowPoint> {
    let (pa, pb) = (pyramid(prev, levels), pyramid(next, levels));
    let grads: Vec<(Plane, Plane)> = pa.iter().map(Plane::gradients).collect();
    let r = (window / 2).max(1) as isize;
    points
        .iter()
        .map(|&pt| {
            let mut g = (0.0f32, 0.0f32);
            let mut ok = true;
            let mut err = 0.0;
            for lvl in (0..pa.len()).rev() {
                #[allow(
                    clippy::cast_precision_loss,
                    clippy::cast_possible_truncation,
                    clippy::cast_possible_wrap
                )]
                let s = (1u32 << lvl) as f32;
                let (ux, uy) = (pt.dx / s, pt.dy / s);
                let (a, b) = (&pa[lvl], &pb[lvl]);
                let (gx, gy) = (&grads[lvl].0, &grads[lvl].1);
                let (mut sxx, mut sxy, mut syy) = (0.0f32, 0.0f32, 0.0f32);
                for dy in -r..=r {
                    for dx in -r..=r {
                        #[allow(clippy::cast_precision_loss)]
                        let (x, y) = (ux + dx as f32, uy + dy as f32);
                        let (ix, iy) = (gx.sample(x, y), gy.sample(x, y));
                        sxx += ix * ix;
                        sxy += ix * iy;
                        syy += iy * iy;
                    }
                }
                let det = sxx * syy - sxy * sxy;
                if det.abs() < 1e-9 {
                    ok = false;
                    break;
                }
                let mut v = (0.0f32, 0.0f32);
                for _ in 0..20 {
                    let (mut bx, mut by) = (0.0f32, 0.0f32);
                    err = 0.0;
                    for dy in -r..=r {
                        for dx in -r..=r {
                            #[allow(clippy::cast_precision_loss)]
                            let (x, y) = (ux + dx as f32, uy + dy as f32);
                            let it = a.sample(x, y) - b.sample(x + g.0 + v.0, y + g.1 + v.1);
                            bx += it * gx.sample(x, y);
                            by += it * gy.sample(x, y);
                            err += it.abs();
                        }
                    }
                    let step = ((syy * bx - sxy * by) / det, (sxx * by - sxy * bx) / det);
                    v = (v.0 + step.0, v.1 + step.1);
                    if step.0.abs() + step.1.abs() < 0.01 {
                        break;
                    }
                }
                g = if lvl > 0 {
                    ((g.0 + v.0) * 2.0, (g.1 + v.1) * 2.0)
                } else {
                    (g.0 + v.0, g.1 + v.1)
                };
            }
            let to = Offset::new(pt.dx + g.0, pt.dy + g.1);
            #[allow(clippy::cast_precision_loss)]
            let inside =
                to.dx >= 0.0 && to.dy >= 0.0 && to.dx < next.w as f32 && to.dy < next.h as f32;
            #[allow(clippy::cast_precision_loss)]
            let n = ((2 * r + 1) * (2 * r + 1)) as f32;
            FlowPoint {
                from: pt,
                to,
                ok: ok && inside,
                error: err / n,
            }
        })
        .collect()
}

/// A dense flow field: `(u, v)` per pixel.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowField {
    pub w: usize,
    pub h: usize,
    pub u: Vec<f32>,
    pub v: Vec<f32>,
}

impl FlowField {
    #[must_use]
    pub fn at(&self, x: usize, y: usize) -> (f32, f32) {
        let i = y.min(self.h - 1) * self.w + x.min(self.w - 1);
        (self.u[i], self.v[i])
    }

    /// Visualise as an HSV wheel (hue = direction, value = magnitude / `max`).
    #[must_use]
    pub fn to_image(&self, max: f32) -> Image {
        let mut px = Vec::with_capacity(self.w * self.h * 4);
        for (u, v) in self.u.iter().zip(&self.v) {
            let mag = ((u * u + v * v).sqrt() / max.max(1e-6)).min(1.0);
            let hue = (v.atan2(*u) / std::f32::consts::TAU).rem_euclid(1.0) * 6.0;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let (i, f) = (hue as u32 % 6, hue.fract());
            let (r, g, b) = match i {
                0 => (1.0, f, 0.0),
                1 => (1.0 - f, 1.0, 0.0),
                2 => (0.0, 1.0, f),
                3 => (0.0, 1.0 - f, 1.0),
                4 => (f, 0.0, 1.0),
                _ => (1.0, 0.0, 1.0 - f),
            };
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            px.extend([
                (r * mag * 255.0) as u8,
                (g * mag * 255.0) as u8,
                (b * mag * 255.0) as u8,
                255,
            ]);
        }
        #[allow(clippy::cast_possible_truncation)]
        Image::from_rgba8(px, self.w as u32, self.h as u32)
    }
}

/// Dense Horn–Schunck flow, coarse-to-fine over `levels`.
#[must_use]
pub fn horn_schunck(
    prev: &Plane,
    next: &Plane,
    alpha: f32,
    iterations: usize,
    levels: usize,
) -> FlowField {
    let (pa, pb) = (pyramid(prev, levels), pyramid(next, levels));
    let top = pa.len() - 1;
    let mut u = vec![0.0f32; pa[top].w * pa[top].h];
    let mut v = u.clone();
    for lvl in (0..pa.len()).rev() {
        let (a, b) = (&pa[lvl], &pb[lvl]);
        let (w, h) = (a.w, a.h);
        if u.len() != w * h {
            // Upsample the coarser field ×2.
            let (pw, ph) = (pa[lvl + 1].w, pa[lvl + 1].h);
            let (ou, ov) = (u.clone(), v.clone());
            u = vec![0.0; w * h];
            v = vec![0.0; w * h];
            for y in 0..h {
                for x in 0..w {
                    let (sx, sy) = ((x / 2).min(pw - 1), (y / 2).min(ph - 1));
                    u[y * w + x] = ou[sy * pw + sx] * 2.0;
                    v[y * w + x] = ov[sy * pw + sx] * 2.0;
                }
            }
        }
        // Warp `b` by the current flow, then solve for the increment.
        let mut warped = Plane::new(w, h);
        for y in 0..h {
            for x in 0..w {
                #[allow(clippy::cast_precision_loss)]
                {
                    warped.data[y * w + x] =
                        b.sample(x as f32 + u[y * w + x], y as f32 + v[y * w + x]);
                }
            }
        }
        let (ax, ay) = a.gradients();
        let (wx, wy) = warped.gradients();
        let ix: Vec<f32> = ax
            .data
            .iter()
            .zip(&wx.data)
            .map(|(p, q)| (p + q) * 0.5)
            .collect();
        let iy: Vec<f32> = ay
            .data
            .iter()
            .zip(&wy.data)
            .map(|(p, q)| (p + q) * 0.5)
            .collect();
        let it: Vec<f32> = warped
            .data
            .iter()
            .zip(&a.data)
            .map(|(p, q)| p - q)
            .collect();
        let (mut du, mut dv) = (vec![0.0f32; w * h], vec![0.0f32; w * h]);
        let a2 = alpha * alpha;
        for _ in 0..iterations {
            let (pu, pv) = (du.clone(), dv.clone());
            for y in 0..h {
                for x in 0..w {
                    let avg = |f: &[f32]| {
                        let g = |dx: isize, dy: isize| {
                            let xx = (x as isize + dx).clamp(0, w as isize - 1);
                            let yy = (y as isize + dy).clamp(0, h as isize - 1);
                            #[allow(clippy::cast_sign_loss)]
                            f[yy as usize * w + xx as usize]
                        };
                        (g(-1, 0) + g(1, 0) + g(0, -1) + g(0, 1)) / 6.0
                            + (g(-1, -1) + g(1, -1) + g(-1, 1) + g(1, 1)) / 12.0
                    };
                    let i = y * w + x;
                    let (ua, va) = (avg(&pu), avg(&pv));
                    let k =
                        (ix[i] * ua + iy[i] * va + it[i]) / (a2 + ix[i] * ix[i] + iy[i] * iy[i]);
                    du[i] = ua - ix[i] * k;
                    dv[i] = va - iy[i] * k;
                }
            }
        }
        for i in 0..w * h {
            u[i] += du[i];
            v[i] += dv[i];
        }
    }
    FlowField {
        w: prev.w,
        h: prev.h,
        u,
        v,
    }
}

/// A connected region.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blob {
    pub label: u32,
    pub area: usize,
    pub centroid: Offset,
    /// `(x0, y0, x1, y1)` inclusive.
    pub bounds: (usize, usize, usize, usize),
}

/// Per-label accumulation while a `blobs` pass walks the plane.
#[derive(Debug, Clone, Copy)]
struct BlobStats {
    area: usize,
    sum_x: f64,
    sum_y: f64,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

/// 8-connected components of `mask > threshold`, largest first, with each
/// pixel's label (0 = background).
#[must_use]
pub fn blobs(mask: &Plane, threshold: f32) -> (Vec<Blob>, Vec<u32>) {
    let (w, h) = (mask.w, mask.h);
    let mut label = vec![0u32; w * h];
    let mut parent: Vec<u32> = vec![0];
    fn find(p: &mut [u32], mut x: u32) -> u32 {
        while p[x as usize] != x {
            p[x as usize] = p[p[x as usize] as usize];
            x = p[x as usize];
        }
        x
    }
    for y in 0..h {
        for x in 0..w {
            if mask.data[y * w + x] <= threshold {
                continue;
            }
            let mut ns = Vec::with_capacity(4);
            if x > 0 && label[y * w + x - 1] != 0 {
                ns.push(label[y * w + x - 1]);
            }
            if y > 0 {
                for dx in [-1isize, 0, 1] {
                    let xx = x as isize + dx;
                    if xx >= 0 && (xx as usize) < w {
                        #[allow(clippy::cast_sign_loss)]
                        let l = label[(y - 1) * w + xx as usize];
                        if l != 0 {
                            ns.push(l);
                        }
                    }
                }
            }
            if ns.is_empty() {
                #[allow(clippy::cast_possible_truncation)]
                let n = parent.len() as u32;
                parent.push(n);
                label[y * w + x] = n;
            } else {
                let m = ns.iter().map(|&l| find(&mut parent, l)).min().unwrap_or(0);
                label[y * w + x] = m;
                for &l in &ns {
                    let r = find(&mut parent, l);
                    parent[r as usize] = m;
                }
            }
        }
    }
    let mut stats: BTreeMap<u32, BlobStats> = BTreeMap::new();
    for y in 0..h {
        for x in 0..w {
            let l = label[y * w + x];
            if l == 0 {
                continue;
            }
            let r = find(&mut parent, l);
            label[y * w + x] = r;
            let e = stats.entry(r).or_insert(BlobStats {
                area: 0,
                sum_x: 0.0,
                sum_y: 0.0,
                x0: x,
                y0: y,
                x1: x,
                y1: y,
            });
            e.area += 1;
            #[allow(clippy::cast_precision_loss)]
            {
                e.sum_x += x as f64;
                e.sum_y += y as f64;
            }
            e.x0 = e.x0.min(x);
            e.y0 = e.y0.min(y);
            e.x1 = e.x1.max(x);
            e.y1 = e.y1.max(y);
        }
    }
    let mut out: Vec<Blob> = stats
        .into_iter()
        .map(|(l, s)| Blob {
            label: l,
            area: s.area,
            #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
            centroid: Offset::new(
                (s.sum_x / s.area as f64) as f32,
                (s.sum_y / s.area as f64) as f32,
            ),
            bounds: (s.x0, s.y0, s.x1, s.y1),
        })
        .collect();
    out.sort_by_key(|b| std::cmp::Reverse(b.area));
    (out, label)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A smooth textured plane shifted by `(sx, sy)`.
    fn texture(w: usize, h: usize, sx: f32, sy: f32) -> Plane {
        let mut p = Plane::new(w, h);
        for y in 0..h {
            for x in 0..w {
                #[allow(clippy::cast_precision_loss)]
                let (fx, fy) = (x as f32 - sx, y as f32 - sy);
                p.data[y * w + x] = 0.5
                    + 0.2 * (fx * 0.21).sin() * (fy * 0.17).cos()
                    + 0.15 * ((fx + fy) * 0.11).sin()
                    + 0.1 * (fx * 0.05 - fy * 0.08).cos();
            }
        }
        p
    }

    #[test]
    fn lucas_kanade_recovers_a_translation() {
        let (a, b) = (texture(96, 80, 0.0, 0.0), texture(96, 80, 3.4, -2.2));
        let pts = good_features(&a, 30, 0.05, 6.0);
        assert!(pts.len() > 10);
        let res = lucas_kanade(&a, &b, &pts, 9, 3);
        let good: Vec<_> = res
            .iter()
            .filter(|r| {
                r.ok && r.from.dx > 12.0 && r.from.dx < 84.0 && r.from.dy > 12.0 && r.from.dy < 68.0
            })
            .collect();
        assert!(good.len() > 5);
        for r in good {
            assert!((r.to.dx - r.from.dx - 3.4).abs() < 0.15, "{r:?}");
            assert!((r.to.dy - r.from.dy + 2.2).abs() < 0.15, "{r:?}");
        }
    }

    #[test]
    fn horn_schunck_flow_points_the_right_way() {
        let (a, b) = (texture(64, 64, 0.0, 0.0), texture(64, 64, 2.0, 1.0));
        let f = horn_schunck(&a, &b, 0.05, 60, 3);
        let (mut su, mut sv, mut n) = (0.0, 0.0, 0.0);
        for y in 16..48 {
            for x in 16..48 {
                let (u, v) = f.at(x, y);
                su += u;
                sv += v;
                n += 1.0;
            }
        }
        let (mu, mv) = (su / n, sv / n);
        assert!(
            (mu - 2.0).abs() < 0.4 && (mv - 1.0).abs() < 0.4,
            "({mu}, {mv})"
        );
        assert_eq!(f.to_image(4.0).width(), 64);
    }

    #[test]
    fn corners_of_a_square_are_found() {
        let mut p = Plane::new(40, 40);
        for y in 10..30 {
            for x in 10..30 {
                p.data[y * 40 + x] = 1.0;
            }
        }
        let c = good_features(&p, 4, 0.1, 5.0);
        assert_eq!(c.len(), 4);
        for q in c {
            let near = |v: f32| (v - 10.0).abs() < 3.0 || (v - 29.0).abs() < 3.0;
            assert!(near(q.dx) && near(q.dy), "{q:?}");
        }
    }

    #[test]
    fn blobs_are_labelled_and_measured() {
        let mut p = Plane::new(20, 10);
        for (x0, y0, s) in [(1, 1, 3), (10, 2, 5)] {
            for y in y0..y0 + s {
                for x in x0..x0 + s {
                    p.data[y * 20 + x] = 1.0;
                }
            }
        }
        // A diagonal touch joins under 8-connectivity.
        p.data[6 * 20 + 15] = 1.0;
        let (b, labels) = blobs(&p, 0.5);
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].area, 26);
        assert_eq!(b[1].area, 9);
        assert!((b[1].centroid.dx - 2.0).abs() < 1e-4);
        assert_eq!(b[1].bounds, (1, 1, 3, 3));
        assert_ne!(labels[20 + 1], labels[2 * 20 + 10]);
    }

    #[test]
    fn pyramid_halves() {
        let p = pyramid(&texture(100, 60, 0.0, 0.0), 4);
        assert_eq!(p.len(), 3, "stops before a side drops under 16");
        assert_eq!((p[1].w, p[1].h), (50, 30));
    }
}
