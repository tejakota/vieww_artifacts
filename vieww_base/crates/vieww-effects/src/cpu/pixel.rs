//! Per-pixel and neighbourhood effects — the stock filters of After
//! Effects, TouchDesigner TOPs, Processing's `filter()` and shader-toy style
//! pixel programs (§2.10–§2.12, §2.15 L5).
//!
//! All operate on tightly packed, straight-alpha RGBA8 with explicit
//! width/height, like the rest of [`cpu`](super):
//!
//! - point ops: [`posterize`], [`threshold`], [`invert`], [`grain`],
//!   [`vignette`];
//! - neighbourhood ops: [`convolve`] with any kernel, and the kernels
//!   [`EMBOSS`], [`SHARPEN`], [`EDGE`]; [`sobel`] edge magnitude;
//!   [`pixelate`];
//! - resampling ops: [`displace`] by a map, [`chromatic_aberration`],
//!   [`kaleidoscope`], [`halftone`];
//! - [`shader`]: an arbitrary per-pixel closure over uv and the source,
//!   the CPU version of a fragment shader;
//! - [`Feedback`]: TouchDesigner's Feedback TOP / Processing's "don't
//!   clear the background" trail — last frame, transformed and faded, under
//!   the new one.

/// Sample with clamped edges, bilinear, straight alpha → `[f32; 4]` 0..255.
#[must_use]
pub fn sample(px: &[u8], w: usize, h: usize, x: f32, y: f32) -> [f32; 4] {
    let fx = (x - 0.5).clamp(0.0, (w - 1) as f32);
    let fy = (y - 0.5).clamp(0.0, (h - 1) as f32);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
    let mut out = [0.0; 4];
    for (xx, yy, wt) in [
        (x0, y0, (1.0 - tx) * (1.0 - ty)),
        (x1, y0, tx * (1.0 - ty)),
        (x0, y1, (1.0 - tx) * ty),
        (x1, y1, tx * ty),
    ] {
        let i = (yy * w + xx) * 4;
        for (c, o) in out.iter_mut().enumerate() {
            *o += f32::from(px[i + c]) * wt;
        }
    }
    out
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn q(v: f32) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

/// Reduce each channel to `levels` steps.
pub fn posterize(px: &mut [u8], levels: u8) {
    let l = f32::from(levels.max(2) - 1);
    for p in px.as_chunks_mut::<4>().0 {
        for c in &mut p[..3] {
            *c = q((f32::from(*c) / 255.0 * l).round() / l * 255.0);
        }
    }
}

/// Black or white by luminance.
pub fn threshold(px: &mut [u8], level: u8) {
    for p in px.as_chunks_mut::<4>().0 {
        let l = 0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2]);
        let v = if l >= f32::from(level) { 255 } else { 0 };
        p[..3].fill(v);
    }
}

/// Invert colour.
pub fn invert(px: &mut [u8]) {
    for p in px.as_chunks_mut::<4>().0 {
        for c in &mut p[..3] {
            *c = 255 - *c;
        }
    }
}

/// Film grain: deterministic per `seed`, `amount` 0..1.
pub fn grain(px: &mut [u8], amount: f32, seed: u32) {
    let mut s = seed.wrapping_mul(2_654_435_761).max(1);
    for p in px.as_chunks_mut::<4>().0 {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        #[allow(clippy::cast_precision_loss)]
        let n = (s as f32 / u32::MAX as f32 - 0.5) * 2.0 * amount * 255.0;
        for c in &mut p[..3] {
            *c = q(f32::from(*c) + n);
        }
    }
}

/// Darken towards the corners.
pub fn vignette(px: &mut [u8], w: usize, h: usize, strength: f32) {
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let r = (cx * cx + cy * cy).sqrt();
    for y in 0..h {
        for x in 0..w {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt() / r;
            let k = 1.0 - strength * d * d;
            let i = (y * w + x) * 4;
            for c in 0..3 {
                px[i + c] = q(f32::from(px[i + c]) * k);
            }
        }
    }
}

/// 3×3 emboss.
pub const EMBOSS: [f32; 9] = [-2.0, -1.0, 0.0, -1.0, 1.0, 1.0, 0.0, 1.0, 2.0];
/// 3×3 sharpen.
pub const SHARPEN: [f32; 9] = [0.0, -1.0, 0.0, -1.0, 5.0, -1.0, 0.0, -1.0, 0.0];
/// 3×3 Laplacian edge detect.
pub const EDGE: [f32; 9] = [-1.0, -1.0, -1.0, -1.0, 8.0, -1.0, -1.0, -1.0, -1.0];

/// Convolve RGB with a square `kernel` (odd side), plus `bias`; alpha kept.
#[must_use]
pub fn convolve(px: &[u8], w: usize, h: usize, kernel: &[f32], bias: f32) -> Vec<u8> {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let side = (kernel.len() as f32).sqrt() as usize;
    let r = (side / 2) as isize;
    let mut out = px.to_vec();
    for y in 0..h {
        for x in 0..w {
            let mut acc = [bias; 3];
            for ky in -r..=r {
                for kx in -r..=r {
                    let sx = (x as isize + kx).clamp(0, w as isize - 1) as usize;
                    let sy = (y as isize + ky).clamp(0, h as isize - 1) as usize;
                    let k = kernel[((ky + r) as usize) * side + (kx + r) as usize];
                    let i = (sy * w + sx) * 4;
                    for (c, a) in acc.iter_mut().enumerate() {
                        *a += f32::from(px[i + c]) * k;
                    }
                }
            }
            let o = (y * w + x) * 4;
            for c in 0..3 {
                out[o + c] = q(acc[c]);
            }
        }
    }
    out
}

/// Sobel gradient magnitude as greyscale.
#[must_use]
pub fn sobel(px: &[u8], w: usize, h: usize) -> Vec<u8> {
    let lum = |x: isize, y: isize| {
        let (x, y) = (
            x.clamp(0, w as isize - 1) as usize,
            y.clamp(0, h as isize - 1) as usize,
        );
        let i = (y * w + x) * 4;
        0.2126 * f32::from(px[i]) + 0.7152 * f32::from(px[i + 1]) + 0.0722 * f32::from(px[i + 2])
    };
    let mut out = px.to_vec();
    for y in 0..h as isize {
        for x in 0..w as isize {
            let gx = lum(x + 1, y - 1) + 2.0 * lum(x + 1, y) + lum(x + 1, y + 1)
                - lum(x - 1, y - 1)
                - 2.0 * lum(x - 1, y)
                - lum(x - 1, y + 1);
            let gy = lum(x - 1, y + 1) + 2.0 * lum(x, y + 1) + lum(x + 1, y + 1)
                - lum(x - 1, y - 1)
                - 2.0 * lum(x, y - 1)
                - lum(x + 1, y - 1);
            let m = q((gx * gx + gy * gy).sqrt() / 4.0);
            let o = (y as usize * w + x as usize) * 4;
            out[o..o + 3].fill(m);
        }
    }
    out
}

/// Mosaic into `block`-pixel cells (each the cell's average).
pub fn pixelate(px: &mut [u8], w: usize, h: usize, block: usize) {
    let b = block.max(1);
    for by in (0..h).step_by(b) {
        for bx in (0..w).step_by(b) {
            let (ex, ey) = ((bx + b).min(w), (by + b).min(h));
            let mut acc = [0u32; 4];
            for y in by..ey {
                for x in bx..ex {
                    let i = (y * w + x) * 4;
                    for c in 0..4 {
                        acc[c] += u32::from(px[i + c]);
                    }
                }
            }
            #[allow(clippy::cast_possible_truncation)]
            let n = ((ex - bx) * (ey - by)) as u32;
            for y in by..ey {
                for x in bx..ex {
                    let i = (y * w + x) * 4;
                    for c in 0..4 {
                        #[allow(clippy::cast_possible_truncation)]
                        {
                            px[i + c] = ((acc[c] + n / 2) / n) as u8;
                        }
                    }
                }
            }
        }
    }
}

/// Displace by a map: the map's red channel shifts x, green shifts y, by
/// `(value − 128) / 128 × amount` pixels (AE's Displacement Map).
#[must_use]
pub fn displace(px: &[u8], map: &[u8], w: usize, h: usize, amount: f32) -> Vec<u8> {
    shader(px, w, h, |x, y, _uv, src| {
        let i = (y * w + x) * 4;
        let dx = (f32::from(map[i]) - 128.0) / 128.0 * amount;
        let dy = (f32::from(map[i + 1]) - 128.0) / 128.0 * amount;
        src(x as f32 + 0.5 + dx, y as f32 + 0.5 + dy)
    })
}

/// Split R and B outward from the centre by up to `amount` pixels.
#[must_use]
pub fn chromatic_aberration(px: &[u8], w: usize, h: usize, amount: f32) -> Vec<u8> {
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    shader(px, w, h, |x, y, _uv, src| {
        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
        let (dx, dy) = ((fx - cx) / cx, (fy - cy) / cy);
        let r = src(fx + dx * amount, fy + dy * amount);
        let g = src(fx, fy);
        let b = src(fx - dx * amount, fy - dy * amount);
        [r[0], g[1], b[2], g[3]]
    })
}

/// Mirror the image into `segments` wedges around the centre.
#[must_use]
pub fn kaleidoscope(px: &[u8], w: usize, h: usize, segments: u32, rotation: f32) -> Vec<u8> {
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    #[allow(clippy::cast_precision_loss)]
    let wedge = std::f32::consts::TAU / segments.max(1) as f32;
    shader(px, w, h, |x, y, _uv, src| {
        let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
        let r = (dx * dx + dy * dy).sqrt();
        let mut a = (dy.atan2(dx) - rotation).rem_euclid(wedge);
        if a > wedge / 2.0 {
            a = wedge - a;
        }
        let a = a + rotation;
        src(cx + r * a.cos(), cy + r * a.sin())
    })
}

/// Print-style halftone: dots on a `cell` grid rotated by `angle`, sized
/// by darkness; ink colour `ink` on white.
#[must_use]
pub fn halftone(px: &[u8], w: usize, h: usize, cell: f32, angle: f32, ink: [u8; 3]) -> Vec<u8> {
    let (s, c) = angle.sin_cos();
    shader(px, w, h, |x, y, _uv, src| {
        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
        let (u, v) = (fx * c + fy * s, -fx * s + fy * c);
        let (cu, cv) = (
            ((u / cell).floor() + 0.5) * cell,
            ((v / cell).floor() + 0.5) * cell,
        );
        let centre = (cu * c - cv * s, cu * s + cv * c);
        let p = src(centre.0, centre.1);
        let dark = 1.0 - (0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]) / 255.0;
        let radius = cell * 0.5 * dark.sqrt() * std::f32::consts::SQRT_2;
        let d = ((u - cu).powi(2) + (v - cv).powi(2)).sqrt();
        let k = (radius - d + 0.5).clamp(0.0, 1.0);
        [
            255.0 + (f32::from(ink[0]) - 255.0) * k,
            255.0 + (f32::from(ink[1]) - 255.0) * k,
            255.0 + (f32::from(ink[2]) - 255.0) * k,
            p[3],
        ]
    })
}

/// A CPU "fragment shader": `f(x, y, uv, sample)` returns RGBA (0..255)
/// for each output pixel; `sample(x, y)` reads the source bilinearly (pixel
/// coordinates, edges clamped).
#[must_use]
pub fn shader<F>(px: &[u8], w: usize, h: usize, f: F) -> Vec<u8>
where
    F: Fn(usize, usize, (f32, f32), &dyn Fn(f32, f32) -> [f32; 4]) -> [f32; 4],
{
    let src = |x: f32, y: f32| sample(px, w, h, x, y);
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let uv = ((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
            let v = f(x, y, uv, &src);
            let o = (y * w + x) * 4;
            for c in 0..4 {
                out[o + c] = q(v[c]);
            }
        }
    }
    out
}

/// A feedback loop: each [`step`](Self::step) draws the previous output,
/// zoomed/rotated about the centre and faded, then composites the new
/// input over it.
#[derive(Debug, Clone, PartialEq)]
pub struct Feedback {
    pub width: usize,
    pub height: usize,
    /// Per-step multiply on the trail (0..1).
    pub decay: f32,
    pub zoom: f32,
    /// Radians per step.
    pub rotate: f32,
    buffer: Vec<u8>,
}

impl Feedback {
    #[must_use]
    pub fn new(width: usize, height: usize, decay: f32, zoom: f32, rotate: f32) -> Self {
        Self {
            width,
            height,
            decay,
            zoom,
            rotate,
            buffer: vec![0; width * height * 4],
        }
    }

    /// Advance one frame with `input` (straight-alpha RGBA) over the trail.
    pub fn step(&mut self, input: &[u8]) -> &[u8] {
        let (w, h) = (self.width, self.height);
        let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
        let (s, c) = self.rotate.sin_cos();
        let (z, decay) = (self.zoom.max(1e-3), self.decay);
        let prev = std::mem::take(&mut self.buffer);
        let mut trail = shader(&prev, w, h, |x, y, _uv, src| {
            let (dx, dy) = ((x as f32 + 0.5 - cx) / z, (y as f32 + 0.5 - cy) / z);
            let (rx, ry) = (dx * c + dy * s, -dx * s + dy * c);
            let (sx, sy) = (cx + rx, cy + ry);
            if sx < 0.0 || sy < 0.0 || sx > w as f32 || sy > h as f32 {
                return [0.0; 4];
            }
            let p = src(sx, sy);
            [p[0], p[1], p[2], p[3] * decay]
        });
        for (d, s) in trail
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(input.as_chunks::<4>().0)
        {
            let sa = f32::from(s[3]) / 255.0;
            let da = f32::from(d[3]) / 255.0;
            let oa = sa + da * (1.0 - sa);
            for k in 0..3 {
                let v = if oa > 0.0 {
                    (f32::from(s[k]) * sa + f32::from(d[k]) * da * (1.0 - sa)) / oa
                } else {
                    0.0
                };
                d[k] = q(v);
            }
            d[3] = q(oa * 255.0);
        }
        self.buffer = trail;
        &self.buffer
    }

    #[must_use]
    pub fn frame(&self) -> &[u8] {
        &self.buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grad(w: usize, h: usize) -> Vec<u8> {
        let mut v = Vec::new();
        for y in 0..h {
            for x in 0..w {
                #[allow(clippy::cast_possible_truncation)]
                v.extend([
                    (x * 255 / (w - 1)) as u8,
                    (y * 255 / (h - 1)) as u8,
                    100,
                    255,
                ]);
            }
        }
        v
    }

    #[test]
    fn posterize_threshold_invert() {
        let mut p = vec![0, 100, 200, 255];
        posterize(&mut p, 2);
        assert_eq!(p, [0, 0, 255, 255]);
        let mut t = vec![10, 10, 10, 255, 200, 200, 200, 255];
        threshold(&mut t, 128);
        assert_eq!(t, [0, 0, 0, 255, 255, 255, 255, 255]);
        invert(&mut t);
        assert_eq!(t[0], 255);
    }

    #[test]
    fn pixelate_averages_cells() {
        let mut p = grad(4, 4);
        pixelate(&mut p, 4, 4, 2);
        assert_eq!(p[0..4], p[4..8]);
        assert_eq!(p[0..4], p[16..20]);
        assert_ne!(p[0..4], p[8..12]);
    }

    #[test]
    fn convolve_identity_and_edges() {
        let p = grad(8, 8);
        let id = [0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0];
        assert_eq!(convolve(&p, 8, 8, &id, 0.0), p);
        let flat = vec![90u8; 8 * 8 * 4];
        let e = convolve(&flat, 8, 8, &EDGE, 0.0);
        assert!(e.chunks(4).all(|c| c[0] == 0), "a flat image has no edges");
        let s = sobel(&flat, 8, 8);
        assert!(s.chunks(4).all(|c| c[0] == 0));
        let mut step = vec![0u8; 8 * 8 * 4];
        for y in 0..8 {
            for x in 4..8 {
                step[(y * 8 + x) * 4..(y * 8 + x) * 4 + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
        let s = sobel(&step, 8, 8);
        assert!(s[(3 * 8 + 4) * 4] > 200 && s[(3 * 8 + 1) * 4] == 0);
    }

    #[test]
    fn shader_identity_and_displace() {
        let p = grad(16, 8);
        assert_eq!(
            shader(&p, 16, 8, |x, y, _, s| s(x as f32 + 0.5, y as f32 + 0.5)),
            p
        );
        let neutral = vec![128u8; 16 * 8 * 4];
        assert_eq!(displace(&p, &neutral, 16, 8, 10.0), p);
        let mut right = neutral.clone();
        right.chunks_mut(4).for_each(|c| c[0] = 255);
        let d = displace(&p, &right, 16, 8, 2.0);
        // Sampling ~2 px to the right: brighter red.
        assert!(d[(4 * 16 + 5) * 4] > p[(4 * 16 + 5) * 4]);
    }

    #[test]
    fn kaleidoscope_is_symmetric() {
        let p = grad(32, 32);
        let k = kaleidoscope(&p, 32, 32, 4, 0.0);
        let at = |x: usize, y: usize| &k[(y * 32 + x) * 4..(y * 32 + x) * 4 + 3];
        // 4 mirrored wedges: a vertical mirror about the centre.
        assert_eq!(at(20, 10), at(20, 21));
    }

    #[test]
    fn halftone_dot_size_tracks_darkness() {
        let dark = [0u8, 0, 0, 255].repeat(32 * 32);
        let light = [230u8, 230, 230, 255].repeat(32 * 32);
        let ink = |v: &[u8]| v.chunks(4).filter(|c| c[0] < 128).count();
        let hd = halftone(&dark, 32, 32, 8.0, 0.3, [0, 0, 0]);
        let hl = halftone(&light, 32, 32, 8.0, 0.3, [0, 0, 0]);
        assert!(ink(&hd) > 3 * ink(&hl));
    }

    #[test]
    fn feedback_trails_fade() {
        let mut fb = Feedback::new(8, 8, 0.5, 1.0, 0.0);
        let mut dot = vec![0u8; 8 * 8 * 4];
        dot[(4 * 8 + 4) * 4..(4 * 8 + 4) * 4 + 4].copy_from_slice(&[255, 255, 255, 255]);
        fb.step(&dot);
        let empty = vec![0u8; 8 * 8 * 4];
        let a1 = fb.step(&empty)[(4 * 8 + 4) * 4 + 3];
        let a2 = fb.step(&empty)[(4 * 8 + 4) * 4 + 3];
        assert!(
            (i32::from(a1) - 128).abs() <= 1 && (i32::from(a2) - 64).abs() <= 1,
            "{a1} {a2}"
        );
    }

    #[test]
    fn grain_is_deterministic_and_vignette_darkens_corners() {
        let mut a = vec![128u8; 64];
        let mut b = a.clone();
        grain(&mut a, 0.2, 7);
        grain(&mut b, 0.2, 7);
        assert_eq!(a, b);
        let mut v = vec![200u8; 10 * 10 * 4];
        vignette(&mut v, 10, 10, 0.8);
        assert!(v[0] < v[(5 * 10 + 5) * 4]);
    }
}
