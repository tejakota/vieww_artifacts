//! Upscaling — the "AI upscale" row (Topaz, After Effects' Detail-preserving
//! Upscale, DLSS's spatial half), answered with a learned method that fits
//! in a file.
//!
//! * [`resize_lanczos`] / [`resize_bicubic`] — the classical separable
//!   resamplers (Lanczos-3, Catmull–Rom bicubic), the baselines every
//!   learned upscaler is measured against.
//! * [`Raisr`] — **RAISR** (Romano, Isidoro & Milanfar, Google 2016): a
//!   *learned* upscaler. Training pairs are made by downscaling
//!   high-resolution images; each pixel of the cheap (bicubic) upscale is
//!   **hashed** by its local gradient geometry — angle (24 bins), strength
//!   (3) and coherence (3) from the structure tensor's eigen-decomposition —
//!   and per bucket a 7×7 filter is fitted by regularised least squares
//!   mapping the cheap patch to the true pixel. At inference each pixel is
//!   hashed the same way and filtered by its bucket's learned kernel.
//!
//! It is a real trained model — 216 filters × 49 weights learned from data
//! at run time, no external weights — and the tests hold it to beating
//! Lanczos on images it was not trained on.

use vieww_foundation::Image;

use crate::cv::Plane;

fn lanczos(x: f32, a: f32) -> f32 {
    if x.abs() < 1e-6 {
        1.0
    } else if x.abs() >= a {
        0.0
    } else {
        let px = std::f32::consts::PI * x;
        a * px.sin() * (px / a).sin() / (px * px)
    }
}

fn cubic(x: f32) -> f32 {
    // Catmull–Rom (a = −0.5).
    let x = x.abs();
    if x < 1.0 {
        1.5 * x * x * x - 2.5 * x * x + 1.0
    } else if x < 2.0 {
        -0.5 * x * x * x + 2.5 * x * x - 4.0 * x + 2.0
    } else {
        0.0
    }
}

fn resize_plane(
    p: &Plane,
    nw: usize,
    nh: usize,
    kernel: &dyn Fn(f32) -> f32,
    support: f32,
) -> Plane {
    #[allow(clippy::cast_precision_loss)]
    let (sx, sy) = (p.w as f32 / nw as f32, p.h as f32 / nh as f32);
    let pass = |src: &Plane, out_w: usize, out_h: usize, horizontal: bool, scale: f32| {
        let mut o = Plane::new(out_w, out_h);
        let filt = scale.max(1.0);
        let r = support * filt;
        for y in 0..out_h {
            for x in 0..out_w {
                #[allow(clippy::cast_precision_loss)]
                let c = (if horizontal { x } else { y }) as f32 + 0.5;
                let center = c * scale - 0.5;
                #[allow(clippy::cast_possible_truncation)]
                let (lo, hi) = ((center - r).floor() as i64, (center + r).ceil() as i64);
                let (mut acc, mut wsum) = (0.0, 0.0);
                for k in lo..=hi {
                    #[allow(clippy::cast_precision_loss)]
                    let wgt = kernel((k as f32 - center) / filt);
                    if wgt == 0.0 {
                        continue;
                    }
                    let n = if horizontal { src.w } else { src.h };
                    #[allow(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        clippy::cast_possible_wrap
                    )]
                    let kk = k.clamp(0, n as i64 - 1) as usize;
                    let v = if horizontal {
                        src.at(kk, y)
                    } else {
                        src.at(x, kk)
                    };
                    acc += wgt * v;
                    wsum += wgt;
                }
                o.data[y * out_w + x] = if wsum.abs() > 1e-9 { acc / wsum } else { 0.0 };
            }
        }
        o
    };
    let t = pass(p, nw, p.h, true, sx);
    pass(&t, nw, nh, false, sy)
}

fn planes(img: &Image) -> [Plane; 4] {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let mut ps = [
        Plane::new(w, h),
        Plane::new(w, h),
        Plane::new(w, h),
        Plane::new(w, h),
    ];
    for (i, p) in img.pixels().as_chunks::<4>().0.iter().enumerate() {
        for c in 0..4 {
            ps[c].data[i] = f32::from(p[c]) / 255.0;
        }
    }
    ps
}

fn join(ps: &[Plane; 4]) -> Image {
    let (w, h) = (ps[0].w, ps[0].h);
    let mut px = Vec::with_capacity(w * h * 4);
    for i in 0..w * h {
        for p in ps {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            px.push((p.data[i].clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
        }
    }
    #[allow(clippy::cast_possible_truncation)]
    Image::from_rgba8(px, w as u32, h as u32)
}

/// Lanczos-3 resize to `w × h`.
#[must_use]
pub fn resize_lanczos(img: &Image, w: u32, h: u32) -> Image {
    let ps = planes(img);
    join(&ps.map(|p| resize_plane(&p, w as usize, h as usize, &|x| lanczos(x, 3.0), 3.0)))
}

/// Catmull–Rom bicubic resize to `w × h`.
#[must_use]
pub fn resize_bicubic(img: &Image, w: u32, h: u32) -> Image {
    let ps = planes(img);
    join(&ps.map(|p| resize_plane(&p, w as usize, h as usize, &cubic, 2.0)))
}

const PATCH: usize = 7;
const TAPS: usize = PATCH * PATCH;
const ANGLES: usize = 24;
const STRENGTHS: usize = 3;
const COHERENCES: usize = 3;
const BUCKETS: usize = ANGLES * STRENGTHS * COHERENCES;

/// The hash of one pixel's neighbourhood.
fn hash(gx: &Plane, gy: &Plane, x: usize, y: usize) -> usize {
    let (mut a, mut b, mut d) = (0.0f32, 0.0f32, 0.0f32);
    let r = PATCH / 2;
    for yy in y.saturating_sub(r)..=(y + r).min(gx.h - 1) {
        for xx in x.saturating_sub(r)..=(x + r).min(gx.w - 1) {
            let (ix, iy) = (gx.at(xx, yy), gy.at(xx, yy));
            a += ix * ix;
            b += ix * iy;
            d += iy * iy;
        }
    }
    let tr = a + d;
    let disc = ((a - d).powi(2) + 4.0 * b * b).sqrt();
    let (l1, l2) = ((tr + disc) * 0.5, (tr - disc) * 0.5);
    let theta = (2.0 * b).atan2(a - d) * 0.5; // dominant gradient angle
    let angle = ((theta.rem_euclid(std::f32::consts::PI) / std::f32::consts::PI) * ANGLES as f32)
        as usize
        % ANGLES;
    let strength = l1.max(0.0).sqrt();
    let (s1, s2) = (l1.max(0.0).sqrt(), l2.max(0.0).sqrt());
    let coherence = if s1 + s2 > 1e-9 {
        (s1 - s2) / (s1 + s2)
    } else {
        0.0
    };
    let si = if strength < 0.004 {
        0
    } else if strength < 0.016 {
        1
    } else {
        2
    };
    let ci = if coherence < 0.25 {
        0
    } else if coherence < 0.5 {
        1
    } else {
        2
    };
    (angle * STRENGTHS + si) * COHERENCES + ci
}

fn patch(p: &Plane, x: usize, y: usize) -> [f32; TAPS] {
    let r = PATCH / 2;
    let mut out = [0f32; TAPS];
    for dy in 0..PATCH {
        for dx in 0..PATCH {
            out[dy * PATCH + dx] = p.at((x + dx).saturating_sub(r), (y + dy).saturating_sub(r));
        }
    }
    out
}

/// A trained RAISR model for one integer scale.
#[derive(Debug, Clone)]
pub struct Raisr {
    pub scale: u32,
    filters: Vec<[f32; TAPS]>,
    /// Training pixels seen per bucket.
    pub counts: Vec<usize>,
}

fn luma_plane(img: &Image) -> Plane {
    Plane::luma(img)
}

impl Raisr {
    /// Learn filters from high-resolution `examples` for upscaling by `scale`.
    #[must_use]
    pub fn train(examples: &[Image], scale: u32) -> Self {
        let mut ata = vec![[[0f64; TAPS]; TAPS]; BUCKETS];
        let mut atb = vec![[0f64; TAPS]; BUCKETS];
        let mut counts = vec![0usize; BUCKETS];
        for hr in examples {
            let (w, h) = (hr.width(), hr.height());
            let (lw, lh) = ((w / scale).max(1), (h / scale).max(1));
            let lr = resize_bicubic(hr, lw, lh);
            let cheap_img = resize_bicubic(&lr, w, h);
            // Hash on luma structure; learn from every colour channel.
            let (gx, gy) = luma_plane(&cheap_img).gradients();
            let (cheap, truth) = (planes(&cheap_img), planes(hr));
            for y in 0..h as usize {
                for x in 0..w as usize {
                    let k = hash(&gx, &gy, x, y);
                    for c in 0..3 {
                        let a = patch(&cheap[c], x, y);
                        let b = f64::from(truth[c].at(x, y));
                        for i in 0..TAPS {
                            let ai = f64::from(a[i]);
                            atb[k][i] += ai * b;
                            for j in i..TAPS {
                                ata[k][i][j] += ai * f64::from(a[j]);
                            }
                        }
                        counts[k] += 1;
                    }
                }
            }
        }
        let identity = {
            let mut f = [0f32; TAPS];
            f[TAPS / 2] = 1.0;
            f
        };
        let filters = (0..BUCKETS)
            .map(|k| {
                if counts[k] < TAPS * 4 {
                    return identity;
                }
                let mut m = ata[k];
                // Index loop: symmetrisation reads column i while writing
                // row i of the same matrix, which no iterator over `m` can
                // express.
                #[allow(clippy::needless_range_loop)]
                for i in 0..TAPS {
                    for j in 0..i {
                        m[i][j] = m[j][i];
                    }
                    m[i][i] += 1e-3 * (m[i][i] + 1e-6); // ridge, relative
                }
                solve(m, atb[k]).map_or(identity, |x| {
                    x.map(|v| {
                        #[allow(clippy::cast_possible_truncation)]
                        let f = v as f32;
                        f
                    })
                })
            })
            .collect();
        Self {
            scale,
            filters,
            counts,
        }
    }

    /// How many buckets learned a filter (rather than falling back to identity).
    #[must_use]
    pub fn trained_buckets(&self) -> usize {
        self.counts.iter().filter(|&&c| c >= TAPS * 4).count()
    }

    /// Upscale by the trained scale: bicubic, then every colour channel is
    /// re-filtered by the kernel its pixel's luma geometry hashes to.
    #[must_use]
    pub fn upscale(&self, img: &Image) -> Image {
        let (w, h) = (img.width() * self.scale, img.height() * self.scale);
        let cheap_img = resize_bicubic(img, w, h);
        let (gx, gy) = luma_plane(&cheap_img).gradients();
        let mut ps = planes(&cheap_img);
        let src = ps.clone();
        for y in 0..h as usize {
            for x in 0..w as usize {
                let f = &self.filters[hash(&gx, &gy, x, y)];
                for c in 0..3 {
                    let a = patch(&src[c], x, y);
                    ps[c].data[y * w as usize + x] = f.iter().zip(&a).map(|(p, q)| p * q).sum();
                }
            }
        }
        join(&ps)
    }
}

/// Solve the symmetric positive-definite system by Cholesky.
fn solve(m: [[f64; TAPS]; TAPS], b: [f64; TAPS]) -> Option<[f64; TAPS]> {
    let mut l = [[0f64; TAPS]; TAPS];
    for i in 0..TAPS {
        for j in 0..=i {
            let s: f64 = (0..j).map(|k| l[i][k] * l[j][k]).sum();
            if i == j {
                let d = m[i][i] - s;
                if d <= 0.0 {
                    return None;
                }
                l[i][j] = d.sqrt();
            } else {
                l[i][j] = (m[i][j] - s) / l[j][j];
            }
        }
    }
    let mut y = [0f64; TAPS];
    for i in 0..TAPS {
        y[i] = (b[i] - (0..i).map(|k| l[i][k] * y[k]).sum::<f64>()) / l[i][i];
    }
    let mut x = [0f64; TAPS];
    for i in (0..TAPS).rev() {
        x[i] = (y[i] - (i + 1..TAPS).map(|k| l[k][i] * x[k]).sum::<f64>()) / l[i][i];
    }
    Some(x)
}

/// PSNR in dB between two equal-size images (RGB).
#[must_use]
pub fn psnr(a: &Image, b: &Image) -> f64 {
    let mut se = 0.0;
    let mut n = 0.0;
    for (p, q) in a
        .pixels()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.pixels().as_chunks::<4>().0)
    {
        for c in 0..3 {
            se += (f64::from(p[c]) - f64::from(q[c])).powi(2);
            n += 1.0;
        }
    }
    10.0 * (255.0f64.powi(2) / (se / n).max(1e-9)).log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shapes with hard edges at varied angles — the content RAISR learns on.
    fn art(w: usize, h: usize, seed: u32) -> Image {
        let mut px = Vec::with_capacity(w * h * 4);
        #[allow(clippy::cast_precision_loss)]
        let fs = seed as f32;
        for y in 0..h {
            for x in 0..w {
                #[allow(clippy::cast_precision_loss)]
                let (fx, fy) = (x as f32, y as f32);
                let mut v = 40.0f32;
                for k in 0..6 {
                    #[allow(clippy::cast_precision_loss)]
                    let a = fs * 0.7 + k as f32 * 0.53;
                    let d = fx * a.cos() + fy * a.sin() - (k as f32 * 9.0 + fs * 3.0) % w as f32;
                    if d > 0.0 && d < 6.0 + k as f32 {
                        v += 35.0;
                    }
                }
                let cx = fx - (w as f32 * 0.5 + fs * 2.0);
                let cy = fy - h as f32 * 0.45;
                if cx * cx + cy * cy < (h as f32 * 0.25).powi(2) {
                    v = 255.0 - v * 0.5;
                }
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let g = v.clamp(0.0, 255.0) as u8;
                px.extend([g, g / 2 + 60, 255 - g, 255]);
            }
        }
        #[allow(clippy::cast_possible_truncation)]
        Image::from_rgba8(px, w as u32, h as u32)
    }

    #[test]
    fn resamplers_preserve_flat_fields_and_size() {
        let flat = Image::from_rgba8([90, 120, 150, 255].repeat(20 * 10), 20, 10);
        for img in [resize_lanczos(&flat, 47, 23), resize_bicubic(&flat, 7, 31)] {
            assert!(img
                .pixels()
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[..3] == [90, 120, 150]));
        }
    }

    #[test]
    fn raisr_learns_and_beats_lanczos_on_unseen_images() {
        let train: Vec<Image> = (0..4).map(|s| art(96, 96, s)).collect();
        let model = Raisr::train(&train, 2);
        assert!(model.trained_buckets() > 20, "{}", model.trained_buckets());
        let test_hr = art(96, 96, 11);
        let lr = resize_bicubic(&test_hr, 48, 48);
        let up = model.upscale(&lr);
        let base = resize_lanczos(&lr, 96, 96);
        let (pr, pl) = (psnr(&up, &test_hr), psnr(&base, &test_hr));
        assert!(pr > pl + 0.3, "RAISR {pr:.2} dB vs Lanczos {pl:.2} dB");
    }
}
