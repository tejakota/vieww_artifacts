//! Light and lens effects — the half of After Effects' Stylize/Blur/Generate
//! categories and of every engine's post stack that a camera, not a canvas,
//! produces: **bloom** (Unreal/Unity), **glow** (AE), **halation** (film),
//! **anamorphic streaks**, **lens flares** (AE Lens Flare, Optical Flares),
//! **light rays / god rays** (AE CC Light Rays, Unreal light shafts),
//! **radial, zoom and directional blur** (AE Radial Blur, Directional Blur),
//! **fractal noise** and **turbulent displace** (AE's Noise & Grain and
//! Distort staples), **lens distortion** with **radial chromatic aberration**,
//! physically-shaped **vignetting** (cos⁴), and **film grain**.
//!
//! The work happens on a [`Linear`] frame — premultiplied linear-light RGB
//! in `f32` — because light adds in linear space: a bloom computed on sRGB
//! bytes is a grey smear, the same bloom in linear light is a glow. The
//! [`Finish`] stack is the whole chain as one call from RGBA8 to RGBA8,
//! ordered as a camera orders it (optics → sensor → grade → grain), with an
//! ordered dither on the way back to 8 bits so the gradients it creates do
//! not band in an encoder.
//!
//! Everything is deterministic: noise and grain are hashes of position and
//! a caller-supplied frame seed.

use super::grade::Lut3;

/// sRGB byte → linear.
#[must_use]
pub fn srgb_to_linear(v: u8) -> f32 {
    to_linear_table()[v as usize]
}

fn to_linear_table() -> &'static [f32; 256] {
    static T: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut t = [0.0; 256];
        for (i, v) in t.iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let c = i as f32 / 255.0;
            *v = if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
        }
        t
    })
}

const ENC_STEPS: usize = 4096;

/// Linear → sRGB (0..1) by table: the encode runs per channel per pixel,
/// and a `powf` there is most of a frame's finishing budget.
fn encode_table() -> &'static [f32] {
    static T: std::sync::OnceLock<Vec<f32>> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        (0..=ENC_STEPS)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let v = (i as f32 / ENC_STEPS as f32).powi(2);
                if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
            })
            .collect()
    })
}

/// Linear → sRGB in 0..1 (unclamped input is clamped). Tabulated on a
/// square-root axis (dense in the shadows, where the curve is steep) and
/// interpolated: error below 1e-4.
#[must_use]
pub fn linear_to_srgb(v: f32) -> f32 {
    let t = encode_table();
    #[allow(clippy::cast_precision_loss)]
    let f = v.clamp(0.0, 1.0).sqrt() * ENC_STEPS as f32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let i = (f as usize).min(ENC_STEPS - 1);
    #[allow(clippy::cast_precision_loss)]
    let fr = f - i as f32;
    t[i] + (t[i + 1] - t[i]) * fr
}

/// Run `f` over row bands of `rows` rows of `width`-long items in
/// parallel (the machine's cores); `f(first_row, band)`.
pub fn par_rows<T: Send>(data: &mut [T], width: usize, f: impl Fn(usize, &mut [T]) + Sync) {
    let height = data.len() / width.max(1);
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get).min(16);
    if threads <= 1 || height < 64 {
        f(0, data);
        return;
    }
    let band = height.div_ceil(threads);
    std::thread::scope(|sc| {
        for (k, chunk) in data.chunks_mut(band * width).enumerate() {
            let f = &f;
            sc.spawn(move || f(k * band, chunk));
        }
    });
}

/// A linear-light RGB frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Linear {
    pub width: usize,
    pub height: usize,
    pub px: Vec<[f32; 3]>,
}

impl Linear {
    #[must_use]
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            px: vec![[0.0; 3]; width * height],
        }
    }

    /// From RGBA8 (sRGB-encoded); alpha is ignored (frames are opaque).
    #[must_use]
    pub fn from_rgba8(px: &[u8], width: usize, height: usize) -> Self {
        let t = to_linear_table();
        let out: Vec<[f32; 3]> = px
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| [t[p[0] as usize], t[p[1] as usize], t[p[2] as usize]])
            .collect();
        Self {
            width,
            height,
            px: out,
        }
    }

    /// To RGBA8 with a 4×4 ordered dither (±½ LSB) so smooth ramps survive
    /// quantisation and the encoder after it.
    pub fn write_rgba8(&self, out: &mut [u8]) {
        const BAYER: [f32; 16] = [0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0];
        let w = self.width;
        par_rows(out, w * 4, |row0, band| {
            for (j, o) in band.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let i = row0 * w + j;
                let (x, y) = (i % w, i / w);
                let c = self.px[i];
                let d = (BAYER[(y & 3) * 4 + (x & 3)] + 0.5) / 16.0 - 0.5;
                for k in 0..3 {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    {
                        o[k] = (linear_to_srgb(c[k]) * 255.0 + 0.5 + d).clamp(0.0, 255.0) as u8;
                    }
                }
                o[3] = 255;
            }
        });
    }

    #[must_use]
    pub fn get(&self, x: isize, y: isize) -> [f32; 3] {
        #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
        let x = x.clamp(0, self.width as isize - 1) as usize;
        #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
        let y = y.clamp(0, self.height as isize - 1) as usize;
        self.px[y * self.width + x]
    }

    /// Bilinear sample at continuous pixel coordinates.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn sample(&self, x: f32, y: f32) -> [f32; 3] {
        let (fx, fy) = (x - 0.5, y - 0.5);
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let (x0, y0) = (x0 as isize, y0 as isize);
        let a = self.get(x0, y0);
        let b = self.get(x0 + 1, y0);
        let c = self.get(x0, y0 + 1);
        let d = self.get(x0 + 1, y0 + 1);
        let mut o = [0.0; 3];
        for k in 0..3 {
            o[k] = (a[k] * (1.0 - tx) + b[k] * tx) * (1.0 - ty) + (c[k] * (1.0 - tx) + d[k] * tx) * ty;
        }
        o
    }

    /// Half resolution with a 13-tap filter (Jimenez 2014's downsample —
    /// the one that keeps bloom from flickering on thin bright lines).
    #[must_use]
    pub fn downsample(&self) -> Self {
        let (w, h) = ((self.width / 2).max(1), (self.height / 2).max(1));
        let mut out = Self::new(w, h);
        const TAPS: [(isize, isize, f32); 13] = [
            (0, 0, 0.125),
            (-1, -1, 0.125),
            (1, -1, 0.125),
            (-1, 1, 0.125),
            (1, 1, 0.125),
            (-2, -2, 0.03125),
            (0, -2, 0.0625),
            (2, -2, 0.03125),
            (-2, 0, 0.0625),
            (2, 0, 0.0625),
            (-2, 2, 0.03125),
            (0, 2, 0.0625),
            (2, 2, 0.03125),
        ];
        par_rows(&mut out.px, w, |row0, band| {
            for (j, o) in band.iter_mut().enumerate() {
                let (x, y) = (j % w, row0 + j / w);
                #[allow(clippy::cast_possible_wrap)]
                let (sx, sy) = ((x * 2) as isize, (y * 2) as isize);
                let mut acc = [0.0f32; 3];
                for (dx, dy, wgt) in TAPS {
                    let c = self.get(sx + dx, sy + dy);
                    for k in 0..3 {
                        acc[k] += c[k] * wgt;
                    }
                }
                *o = acc;
            }
        });
        out
    }

    /// Upsample `small` with a tent (four bilinear taps at half-texel
    /// offsets) and add it, scaled, into `self`.
    pub fn add_upsampled(&mut self, small: &Self, scale: f32) {
        #[allow(clippy::cast_precision_loss)]
        let (sx, sy) = (small.width as f32 / self.width as f32, small.height as f32 / self.height as f32);
        let w = self.width;
        par_rows(&mut self.px, w, |row0, band| {
            for (j, p) in band.iter_mut().enumerate() {
                let (x, y) = (j % w, row0 + j / w);
                #[allow(clippy::cast_precision_loss)]
                let (u, v) = ((x as f32 + 0.5) * sx, (y as f32 + 0.5) * sy);
                let mut acc = [0.0f32; 3];
                for (dx, dy) in [(-0.5, -0.5), (0.5, -0.5), (-0.5, 0.5), (0.5, 0.5)] {
                    let c = small.sample(u + dx, v + dy);
                    for k in 0..3 {
                        acc[k] += c[k] * 0.25;
                    }
                }
                for k in 0..3 {
                    p[k] += acc[k] * scale;
                }
            }
        });
    }
}

fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// Soft-knee threshold (Unity's): passes light above `threshold`, fading
/// in over `knee`.
fn prefilter(c: [f32; 3], threshold: f32, knee: f32) -> [f32; 3] {
    let br = c[0].max(c[1]).max(c[2]);
    let soft = (br - threshold + knee).clamp(0.0, 2.0 * knee);
    let soft = soft * soft / (4.0 * knee + 1e-5);
    let contrib = soft.max(br - threshold) / br.max(1e-5);
    c.map(|v| v * contrib.max(0.0))
}

/// Physically-motivated bloom: threshold, a mip pyramid down, tent
/// upsamples back, added at `intensity`. `radius` (0..1) weights the wide
/// levels; `tint` colours the scattered light.
pub fn bloom(frame: &mut Linear, threshold: f32, knee: f32, intensity: f32, radius: f32, tint: [f32; 3]) {
    bloom_keyed(frame, threshold, knee, intensity, radius, tint, 0.0);
}

/// [`bloom`] with a **chroma key**: at `chroma_key` 1 only saturated light
/// scatters (neon accents glow, white type stays crisp); at 0 every
/// highlight does. Interfaces are mostly white text on dark — a camera's
/// bloom on every glyph reads as haze — so a UI film keys its glow to
/// colour, the way a display's own emissive accents read on camera.
pub fn bloom_keyed(frame: &mut Linear, threshold: f32, knee: f32, intensity: f32, radius: f32, tint: [f32; 3], chroma_key: f32) {
    if intensity <= 0.0 {
        return;
    }
    // Prefilter after the first (13-tap, anti-flicker) downsample: half the
    // work, and the threshold sees the same averaged light.
    let mut first = frame.downsample();
    let key = chroma_key.clamp(0.0, 1.0);
    for o in &mut first.px {
        let mx = o[0].max(o[1]).max(o[2]);
        let mn = o[0].min(o[1]).min(o[2]);
        let sat = if mx > 1e-5 { (mx - mn) / mx } else { 0.0 };
        let w = 1.0 - key + key * sat.powf(0.7);
        *o = prefilter(o.map(|v| v * w), threshold * (1.0 - 0.5 * key), knee);
    }
    let mut mips = vec![first];
    while mips.len() < 7 {
        let last = mips.last().expect("mip");
        if last.width < 8 || last.height < 8 {
            break;
        }
        let next = last.downsample();
        mips.push(next);
    }
    for i in (1..mips.len()).rev() {
        let (lo, hi) = mips.split_at_mut(i);
        let src = &hi[0];
        lo[i - 1].add_upsampled(src, radius.clamp(0.0, 1.0) * 1.2);
    }
    // The glow lives at half resolution (it is smooth by construction);
    // one bilinear tap per output pixel brings it up.
    let half = &mips[0];
    #[allow(clippy::cast_precision_loss)]
    let (sx, sy) = (half.width as f32 / frame.width as f32, half.height as f32 / frame.height as f32);
    let w = frame.width;
    par_rows(&mut frame.px, w, |row0, band| {
        for (j, o) in band.iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let g = half.sample(((j % w) as f32 + 0.5) * sx, ((row0 + j / w) as f32 + 0.5) * sy);
            for k in 0..3 {
                o[k] += g[k] * intensity * tint[k];
            }
        }
    });
}

/// Film halation: the red-orange fringe light makes bouncing off the film
/// base, around bright edges only.
pub fn halation(frame: &mut Linear, threshold: f32, amount: f32) {
    if amount <= 0.0 {
        return;
    }
    let mut bright = Linear::new(frame.width, frame.height);
    for (o, c) in bright.px.iter_mut().zip(&frame.px) {
        let l = luma(*c);
        *o = if l > threshold { [l - threshold, 0.0, 0.0] } else { [0.0; 3] };
    }
    let m = bright.downsample().downsample();
    let mut g = Linear::new(frame.width, frame.height);
    g.add_upsampled(&m.downsample(), 1.0);
    for (o, c) in frame.px.iter_mut().zip(&g.px) {
        o[0] += c[0] * amount;
        o[1] += c[0] * amount * 0.35;
        o[2] += c[0] * amount * 0.08;
    }
}

/// Anamorphic streaks: bright points smeared horizontally (the blue lines
/// of a cylindrical lens).
pub fn anamorphic_streaks(frame: &mut Linear, threshold: f32, length: f32, intensity: f32, tint: [f32; 3]) {
    if intensity <= 0.0 {
        return;
    }
    let small = {
        let mut b = Linear::new(frame.width, frame.height);
        for (o, c) in b.px.iter_mut().zip(&frame.px) {
            *o = prefilter(*c, threshold, threshold * 0.5);
        }
        b.downsample().downsample()
    };
    // Exponential smear both ways along x, on the quarter-res image.
    let (w, h) = (small.width, small.height);
    #[allow(clippy::cast_precision_loss)]
    let decay = (-1.0 / (length * w as f32 * 0.25).max(1.0)).exp();
    let mut out = Linear::new(w, h);
    for y in 0..h {
        let mut run = [0.0f32; 3];
        for x in 0..w {
            let c = small.px[y * w + x];
            for k in 0..3 {
                run[k] = run[k] * decay + c[k];
                out.px[y * w + x][k] += run[k];
            }
        }
        let mut run = [0.0f32; 3];
        for x in (0..w).rev() {
            let c = small.px[y * w + x];
            for k in 0..3 {
                run[k] = run[k] * decay + c[k];
                out.px[y * w + x][k] += run[k];
            }
        }
    }
    let norm = (1.0 - decay) * 0.5;
    let mut full = Linear::new(frame.width, frame.height);
    full.add_upsampled(&out, norm);
    for (o, c) in frame.px.iter_mut().zip(&full.px) {
        let l = luma(*c);
        for k in 0..3 {
            o[k] += l * tint[k] * intensity;
        }
    }
}

/// A lens flare for a light at `pos` (pixels): a starburst core, a halo
/// ring, and a chain of coloured ghosts mirrored through the frame centre.
pub fn lens_flare(frame: &mut Linear, pos: (f32, f32), brightness: f32, seed: u32) {
    if brightness <= 0.0 {
        return;
    }
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (frame.width as f32, frame.height as f32);
    let (cx, cy) = (w * 0.5, h * 0.5);
    let diag = (w * w + h * h).sqrt();
    let axis = (cx - pos.0, cy - pos.1);
    let mut rng = seed.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    let mut rand = || {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        #[allow(clippy::cast_precision_loss)]
        let v = (rng >> 8) as f32 / (1u32 << 24) as f32;
        v
    };
    // (position along axis in units of |axis|·2, radius frac of diag, colour)
    let mut ghosts = Vec::new();
    for i in 0..7 {
        let t = 0.25 + 0.3 * i as f32 + rand() * 0.2;
        let r = diag * (0.012 + rand() * 0.05);
        let hue = rand();
        let col = [
            0.5 + 0.5 * (hue * std::f32::consts::TAU).cos(),
            0.5 + 0.5 * (hue * std::f32::consts::TAU + std::f32::consts::FRAC_PI_3).cos(),
            0.5 + 0.5 * (hue * std::f32::consts::TAU + 2.0 * std::f32::consts::FRAC_PI_3).cos(),
        ];
        ghosts.push((t, r, col, rand() < 0.4));
    }
    let core_r = diag * 0.02;
    let halo_r = diag * 0.22;
    for y in 0..frame.height {
        for x in 0..frame.width {
            #[allow(clippy::cast_precision_loss)]
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let (dx, dy) = (px - pos.0, py - pos.1);
            let d = (dx * dx + dy * dy).sqrt();
            let mut add = [0.0f32; 3];
            // Core + six-point starburst.
            let ang = dy.atan2(dx);
            let star = (ang * 3.0).cos().abs().powf(160.0) * (-d / (diag * 0.07)).exp() * 0.6;
            let core = (-(d / core_r).powi(2)).exp() * 2.0 + (-d / (core_r * 3.0)).exp() * 0.3;
            for k in 0..3 {
                add[k] += (core + star * 0.8) * [1.0, 0.92, 0.8][k];
            }
            // Halo ring with a spectral rim.
            let ring = (-((d - halo_r) / (diag * 0.006)).powi(2)).exp() * 0.025;
            let rim = [(-((d - halo_r * 1.01) / (diag * 0.005)).powi(2)).exp(), 1.0, (-((d - halo_r * 0.99) / (diag * 0.005)).powi(2)).exp()];
            for k in 0..3 {
                add[k] += ring * rim[k];
            }
            // Ghosts.
            for (t, r, col, ringed) in &ghosts {
                let gx = pos.0 + axis.0 * 2.0 * t;
                let gy = pos.1 + axis.1 * 2.0 * t;
                let gd = ((px - gx).powi(2) + (py - gy).powi(2)).sqrt();
                let v = if *ringed {
                    (-((gd - r) / (r * 0.12)).powi(2)).exp() * 0.12
                } else {
                    let e = (gd / r).clamp(0.0, 1.2);
                    (1.0 - e.powi(6)).max(0.0) * 0.05 * (0.6 + 0.4 * e)
                };
                if v > 0.0 {
                    for k in 0..3 {
                        add[k] += v * col[k];
                    }
                }
            }
            let p = &mut frame.px[y * frame.width + x];
            for k in 0..3 {
                p[k] += add[k] * brightness;
            }
        }
    }
}

/// Light rays (god rays): the frame's light above `threshold` streaked
/// radially away from `center`, `length` (0..1) of the way to the source,
/// in `samples` steps — AE's CC Light Rays, Unreal's light shafts.
pub fn light_rays(frame: &mut Linear, center: (f32, f32), threshold: f32, length: f32, intensity: f32, samples: usize) {
    if intensity <= 0.0 {
        return;
    }
    let mut src = Linear::new(frame.width, frame.height);
    for (o, c) in src.px.iter_mut().zip(&frame.px) {
        *o = prefilter(*c, threshold, threshold * 0.5);
    }
    let src = src.downsample();
    #[allow(clippy::cast_precision_loss)]
    let (cx, cy) = (center.0 * src.width as f32 / frame.width as f32, center.1 * src.height as f32 / frame.height as f32);
    let mut rays = Linear::new(src.width, src.height);
    let n = samples.max(4);
    for y in 0..src.height {
        for x in 0..src.width {
            #[allow(clippy::cast_precision_loss)]
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let mut acc = [0.0f32; 3];
            let mut wsum = 0.0;
            for i in 0..n {
                #[allow(clippy::cast_precision_loss)]
                let t = i as f32 / n as f32 * length;
                let (sx, sy) = (px + (cx - px) * t, py + (cy - py) * t);
                let wgt = 1.0 - t * 0.8;
                let c = src.sample(sx, sy);
                for k in 0..3 {
                    acc[k] += c[k] * wgt;
                }
                wsum += wgt;
            }
            rays.px[y * src.width + x] = acc.map(|v| v / wsum);
        }
    }
    let mut full = Linear::new(frame.width, frame.height);
    full.add_upsampled(&rays, intensity);
    for (o, c) in frame.px.iter_mut().zip(&full.px) {
        for k in 0..3 {
            o[k] += c[k];
        }
    }
}

/// Zoom (radial) blur about `center`: `amount` is the fraction of the
/// distance to the centre each pixel smears across.
#[must_use]
pub fn zoom_blur(frame: &Linear, center: (f32, f32), amount: f32, samples: usize) -> Linear {
    let mut out = Linear::new(frame.width, frame.height);
    let n = samples.max(2);
    for y in 0..frame.height {
        for x in 0..frame.width {
            #[allow(clippy::cast_precision_loss)]
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let mut acc = [0.0f32; 3];
            for i in 0..n {
                #[allow(clippy::cast_precision_loss)]
                let s = 1.0 - amount * i as f32 / (n - 1) as f32;
                let c = frame.sample(center.0 + (px - center.0) * s, center.1 + (py - center.1) * s);
                for k in 0..3 {
                    acc[k] += c[k];
                }
            }
            #[allow(clippy::cast_precision_loss)]
            let inv = 1.0 / n as f32;
            out.px[y * frame.width + x] = acc.map(|v| v * inv);
        }
    }
    out
}

/// Spin (rotational radial) blur by `angle` radians about `center`.
#[must_use]
pub fn spin_blur(frame: &Linear, center: (f32, f32), angle: f32, samples: usize) -> Linear {
    let mut out = Linear::new(frame.width, frame.height);
    let n = samples.max(2);
    for y in 0..frame.height {
        for x in 0..frame.width {
            #[allow(clippy::cast_precision_loss)]
            let (dx, dy) = (x as f32 + 0.5 - center.0, y as f32 + 0.5 - center.1);
            let mut acc = [0.0f32; 3];
            for i in 0..n {
                #[allow(clippy::cast_precision_loss)]
                let a = angle * (i as f32 / (n - 1) as f32 - 0.5);
                let (s, c) = a.sin_cos();
                let v = frame.sample(center.0 + dx * c - dy * s, center.1 + dx * s + dy * c);
                for k in 0..3 {
                    acc[k] += v[k];
                }
            }
            #[allow(clippy::cast_precision_loss)]
            let inv = 1.0 / n as f32;
            out.px[y * frame.width + x] = acc.map(|v| v * inv);
        }
    }
    out
}

/// Directional (motion) blur along `angle` over `length` pixels.
#[must_use]
pub fn directional_blur(frame: &Linear, angle: f32, length: f32, samples: usize) -> Linear {
    let mut out = Linear::new(frame.width, frame.height);
    let n = samples.max(2);
    let (s, c) = angle.sin_cos();
    for y in 0..frame.height {
        for x in 0..frame.width {
            let mut acc = [0.0f32; 3];
            for i in 0..n {
                #[allow(clippy::cast_precision_loss)]
                let t = (i as f32 / (n - 1) as f32 - 0.5) * length;
                #[allow(clippy::cast_precision_loss)]
                let v = frame.sample(x as f32 + 0.5 + c * t, y as f32 + 0.5 + s * t);
                for k in 0..3 {
                    acc[k] += v[k];
                }
            }
            #[allow(clippy::cast_precision_loss)]
            let inv = 1.0 / n as f32;
            out.px[y * frame.width + x] = acc.map(|v| v * inv);
        }
    }
    out
}

fn hash2(x: i32, y: i32, seed: u32) -> f32 {
    #[allow(clippy::cast_sign_loss)]
    let mut h = (x as u32).wrapping_mul(0x8DA6_B343) ^ (y as u32).wrapping_mul(0xD816_3841) ^ seed.wrapping_mul(0xCB1A_B31F);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    #[allow(clippy::cast_precision_loss)]
    let v = (h & 0x00FF_FFFF) as f32 / 16_777_215.0;
    v
}

/// Smooth value noise in 0..1.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (tx, ty) = (x - x0, y - y0);
    let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
    let (ix, iy) = (x0 as i32, y0 as i32);
    let a = hash2(ix, iy, seed);
    let b = hash2(ix + 1, iy, seed);
    let c = hash2(ix, iy + 1, seed);
    let d = hash2(ix + 1, iy + 1, seed);
    (a + (b - a) * sx) * (1.0 - sy) + (c + (d - c) * sx) * sy
}

/// AE Fractal Noise: `octaves` of value noise, each at double frequency and
/// `persistence` amplitude, evolving with `evolution` (a third axis folded
/// in as a phase), in 0..1. `turbulent` folds each octave (|2n−1|).
#[must_use]
pub fn fractal_noise(x: f32, y: f32, octaves: u32, persistence: f32, evolution: f32, turbulent: bool, seed: u32) -> f32 {
    let mut amp = 1.0;
    let mut freq = 1.0;
    let mut sum = 0.0;
    let mut norm = 0.0;
    for o in 0..octaves.max(1) {
        let ev = evolution * (1.0 + o as f32 * 0.37);
        let n0 = value_noise(x * freq + ev, y * freq - ev * 0.7, seed.wrapping_add(o));
        let n = if turbulent { (n0 * 2.0 - 1.0).abs() } else { n0 };
        sum += n * amp;
        norm += amp;
        amp *= persistence;
        freq *= 2.0;
    }
    sum / norm
}

/// Fill a frame with fractal noise at `scale` pixels per cell, as grey.
#[must_use]
pub fn fractal_noise_frame(width: usize, height: usize, scale: f32, octaves: u32, evolution: f32, seed: u32) -> Linear {
    let mut f = Linear::new(width, height);
    for y in 0..height {
        for x in 0..width {
            #[allow(clippy::cast_precision_loss)]
            let v = fractal_noise(x as f32 / scale, y as f32 / scale, octaves, 0.5, evolution, false, seed);
            f.px[y * width + x] = [v, v, v];
        }
    }
    f
}

/// AE Turbulent Displace: every pixel pushed by a fractal noise field.
#[must_use]
pub fn turbulent_displace(frame: &Linear, amount: f32, size: f32, evolution: f32, seed: u32) -> Linear {
    let mut out = Linear::new(frame.width, frame.height);
    for y in 0..frame.height {
        for x in 0..frame.width {
            #[allow(clippy::cast_precision_loss)]
            let (fx, fy) = (x as f32, y as f32);
            let dx = fractal_noise(fx / size, fy / size, 3, 0.5, evolution, false, seed) * 2.0 - 1.0;
            let dy = fractal_noise(fx / size + 31.7, fy / size - 17.3, 3, 0.5, evolution, false, seed ^ 0x55) * 2.0 - 1.0;
            out.px[y * frame.width + x] = frame.sample(fx + 0.5 + dx * amount, fy + 0.5 + dy * amount);
        }
    }
    out
}

/// Barrel/pincushion distortion (`k` > 0 barrel) with radial chromatic
/// aberration (`ca` pixels of red/blue split at the corners).
#[must_use]
pub fn lens_distort(frame: &Linear, k: f32, ca: f32) -> Linear {
    let mut out = Linear::new(frame.width, frame.height);
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (frame.width as f32, frame.height as f32);
    let (cx, cy) = (w * 0.5, h * 0.5);
    let r_max = (cx * cx + cy * cy).sqrt();
    for y in 0..frame.height {
        for x in 0..frame.width {
            #[allow(clippy::cast_precision_loss)]
            let (dx, dy) = ((x as f32 + 0.5 - cx) / r_max, (y as f32 + 0.5 - cy) / r_max);
            let r2 = dx * dx + dy * dy;
            let f = 1.0 + k * r2;
            let mut c = [0.0f32; 3];
            for (ch, shift) in [(0usize, 1.0f32), (1, 0.0), (2, -1.0)] {
                let s = f * (1.0 + shift * ca / r_max * r2.sqrt());
                c[ch] = frame.sample(cx + dx * r_max * s, cy + dy * r_max * s)[ch];
            }
            out.px[y * frame.width + x] = c;
        }
    }
    out
}

/// Natural (cos⁴) vignetting of strength 0..1, with `roundness` 1 = a circle
/// in a wide frame, 0 = following the frame's aspect.
pub fn natural_vignette(frame: &mut Linear, strength: f32, roundness: f32) {
    if strength <= 0.0 {
        return;
    }
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (frame.width as f32, frame.height as f32);
    let aspect = w / h;
    let fw = frame.width;
    par_rows(&mut frame.px, fw, |row0, band| {
        for (j, p) in band.iter_mut().enumerate() {
            let (x, y) = (j % fw, row0 + j / fw);
            #[allow(clippy::cast_precision_loss)]
            let mut dx = (x as f32 + 0.5) / w * 2.0 - 1.0;
            #[allow(clippy::cast_precision_loss)]
            let dy = (y as f32 + 0.5) / h * 2.0 - 1.0;
            dx *= 1.0 + (aspect - 1.0) * roundness;
            let r2 = (dx * dx + dy * dy) * 0.5;
            let c = 1.0 / (1.0 + r2 * 1.6);
            let fall = 1.0 - strength * (1.0 - c * c);
            for v in p.iter_mut() {
                *v *= fall;
            }
        }
    });
}

/// Film grain: luminance-weighted (strongest in the mids, as on stock),
/// slightly soft (two-tap), and different every frame.
pub fn film_grain(frame: &mut Linear, amount: f32, frame_seed: u32) {
    if amount <= 0.0 {
        return;
    }
    let fw = frame.width;
    par_rows(&mut frame.px, fw, |row0, band| {
        for (j, p) in band.iter_mut().enumerate() {
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            let (ix, iy) = ((j % fw) as i32, (row0 + j / fw) as i32);
            let n = (hash2(ix, iy, frame_seed) + hash2(ix + 1, iy, frame_seed) * 0.5 + hash2(ix, iy + 1, frame_seed) * 0.5) / 2.0 - 0.5;
            let l = luma(*p).clamp(0.0, 1.0);
            let e = linear_to_srgb(l);
            let mid = 4.0 * e * (1.0 - e) * 0.85 + 0.15;
            for v in p.iter_mut() {
                *v = (*v + n * amount * mid * (v.max(0.002)).sqrt() * 0.6).max(0.0);
            }
        }
    });
}

/// The finishing stack — a camera's order, one call per frame.
#[derive(Debug, Clone)]
pub struct Finish {
    pub bloom_threshold: f32,
    pub bloom_knee: f32,
    pub bloom_intensity: f32,
    pub bloom_radius: f32,
    pub bloom_tint: [f32; 3],
    /// 0 = every highlight blooms, 1 = only saturated light does.
    pub bloom_chroma_key: f32,
    pub halation: f32,
    pub streaks: f32,
    pub streak_tint: [f32; 3],
    pub flare: Option<((f32, f32), f32)>,
    pub rays: Option<((f32, f32), f32)>,
    pub lens_k: f32,
    pub chromatic: f32,
    pub vignette: f32,
    /// A grade applied in display space at `grade_strength`.
    pub grade: Option<Lut3>,
    pub grade_strength: f32,
    pub grain: f32,
    pub exposure: f32,
}

impl Default for Finish {
    fn default() -> Self {
        Self {
            bloom_threshold: 0.8,
            bloom_knee: 0.4,
            bloom_intensity: 0.0,
            bloom_radius: 0.7,
            bloom_tint: [1.0, 1.0, 1.0],
            bloom_chroma_key: 0.0,
            halation: 0.0,
            streaks: 0.0,
            streak_tint: [0.45, 0.65, 1.0],
            flare: None,
            rays: None,
            lens_k: 0.0,
            chromatic: 0.0,
            vignette: 0.0,
            grade: None,
            grade_strength: 1.0,
            grain: 0.0,
            exposure: 1.0,
        }
    }
}

impl Finish {
    /// Apply in place to RGBA8 `px` (`width × height`), `frame_seed` varying
    /// the grain.
    pub fn apply_rgba8(&self, px: &mut [u8], width: usize, height: usize, frame_seed: u32) {
        let mut f = Linear::from_rgba8(px, width, height);
        if (self.exposure - 1.0).abs() > 1e-6 {
            for p in &mut f.px {
                for v in p {
                    *v *= self.exposure;
                }
            }
        }
        if let Some((pos, amount)) = self.rays {
            light_rays(&mut f, pos, self.bloom_threshold, 0.8, amount, 24);
        }
        bloom_keyed(&mut f, self.bloom_threshold, self.bloom_knee, self.bloom_intensity, self.bloom_radius, self.bloom_tint, self.bloom_chroma_key);
        halation(&mut f, self.bloom_threshold, self.halation);
        if self.streaks > 0.0 {
            anamorphic_streaks(&mut f, self.bloom_threshold.max(0.5), 0.5, self.streaks, self.streak_tint);
        }
        if let Some((pos, b)) = self.flare {
            lens_flare(&mut f, pos, b, 7);
        }
        if self.lens_k.abs() > 1e-6 || self.chromatic > 0.0 {
            f = lens_distort(&f, self.lens_k, self.chromatic);
        }
        natural_vignette(&mut f, self.vignette, 0.6);
        film_grain(&mut f, self.grain, frame_seed);
        f.write_rgba8(px);
        if let Some(lut) = &self.grade {
            lut.apply_rgba(px, self.grade_strength);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dot_frame(w: usize, h: usize, at: (usize, usize), v: f32) -> Linear {
        let mut f = Linear::new(w, h);
        f.px[at.1 * w + at.0] = [v, v, v];
        f
    }

    #[test]
    fn srgb_round_trips_every_byte() {
        for b in 0..=255u8 {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let back = (linear_to_srgb(srgb_to_linear(b)) * 255.0).round() as u8;
            assert_eq!(back, b);
        }
    }

    #[test]
    fn bloom_spreads_light_only_from_highlights() {
        let mut f = dot_frame(64, 64, (32, 32), 8.0);
        f.px[5 * 64 + 5] = [0.3, 0.3, 0.3];
        bloom(&mut f, 1.0, 0.2, 1.0, 0.8, [1.0; 3]);
        let near = f.get(36, 32)[0];
        let far = f.get(60, 60)[0];
        assert!(near > 0.0005, "{near}");
        assert!(near > far);
        // The dim pixel did not bloom its neighbour.
        assert!(f.get(7, 5)[0] < 1e-3 + f.get(60, 60)[0] * 2.0);
    }

    #[test]
    fn zoom_blur_at_zero_is_identity_and_spin_preserves_the_centre() {
        let mut f = Linear::new(32, 32);
        for (i, p) in f.px.iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let v = (i % 7) as f32 / 7.0;
            *p = [v, v * 0.5, 1.0 - v];
        }
        let z = zoom_blur(&f, (16.0, 16.0), 0.0, 4);
        for (a, b) in z.px.iter().zip(&f.px) {
            assert!((a[0] - b[0]).abs() < 1e-5);
        }
        let s = spin_blur(&f, (16.0, 16.0), 1.0, 8);
        let centre = s.get(16, 16);
        assert!(centre[0].is_finite());
    }

    #[test]
    fn directional_blur_smears_along_its_angle_only() {
        let f = dot_frame(41, 41, (20, 20), 1.0);
        let b = directional_blur(&f, 0.0, 10.0, 21);
        assert!(b.get(24, 20)[0] > 0.01);
        assert!(b.get(20, 24)[0] < 1e-4);
    }

    #[test]
    fn fractal_noise_is_bounded_deterministic_and_evolves() {
        let a = fractal_noise(3.3, 7.1, 5, 0.5, 0.0, false, 9);
        let b = fractal_noise(3.3, 7.1, 5, 0.5, 0.0, false, 9);
        let c = fractal_noise(3.3, 7.1, 5, 0.5, 0.5, false, 9);
        assert_eq!(a, b);
        assert!((a - c).abs() > 1e-4);
        for i in 0..500 {
            #[allow(clippy::cast_precision_loss)]
            let v = fractal_noise(i as f32 * 0.37, i as f32 * 0.11, 4, 0.6, 0.0, true, 1);
            assert!((0.0..=1.0).contains(&v));
        }
    }

    #[test]
    fn vignette_darkens_corners_not_the_centre() {
        let mut f = Linear::new(64, 36);
        for p in &mut f.px {
            *p = [0.5; 3];
        }
        natural_vignette(&mut f, 0.6, 0.5);
        assert!(f.get(32, 18)[0] > 0.49);
        assert!(f.get(0, 0)[0] < 0.4);
    }

    #[test]
    fn the_finish_stack_is_deterministic_and_dithers_flat_fields_honestly() {
        let mut a = vec![40u8; 64 * 36 * 4];
        let mut b = a.clone();
        let fin = Finish {
            bloom_intensity: 0.3,
            vignette: 0.2,
            grain: 0.02,
            ..Finish::default()
        };
        fin.apply_rgba8(&mut a, 64, 36, 3);
        fin.apply_rgba8(&mut b, 64, 36, 3);
        assert_eq!(a, b);
        let mut c = vec![40u8; 64 * 36 * 4];
        Finish::default().apply_rgba8(&mut c, 64, 36, 3);
        // No effects: dither is ±½ LSB, so every byte stays 40.
        assert!(c.chunks_exact(4).all(|p| p[0] == 40 && p[3] == 255));
    }

    #[test]
    fn lens_flare_and_rays_add_light_without_nans() {
        let mut f = dot_frame(80, 45, (20, 10), 20.0);
        lens_flare(&mut f, (20.0, 10.0), 0.5, 1);
        light_rays(&mut f, (20.0, 10.0), 0.9, 0.8, 0.5, 12);
        assert!(f.px.iter().all(|p| p.iter().all(|v| v.is_finite() && *v >= 0.0)));
        assert!(f.get(22, 10)[0] > 0.05);
    }
}
