//! Image-based lighting — Three.js' `scene.environment` / `CubeTexture`,
//! drei's `<Environment>`, Unity's skybox + reflection probes, Blender's
//! World node.
//!
//! # What it is
//!
//! An [`EnvMap`] is the light arriving from every direction, stored as an
//! equirectangular grid of linear-light [`Rgb`] (so values above 1 — the sun
//! in an HDRI — survive). An [`Environment`] wraps one and precomputes what
//! a rasterizer needs to *light* with it rather than merely *show* it:
//!
//! * **diffuse irradiance** as nine spherical-harmonic coefficients
//!   (Ramamoorthi & Hanrahan 2001) — the cosine-convolved sky, evaluated per
//!   fragment as a handful of multiply-adds;
//! * **prefiltered specular** — a chain of maps, each convolved with a GGX
//!   lobe of increasing roughness (Karis 2013's split sum, first half),
//!   importance-sampled with a Hammersley sequence so the result is
//!   deterministic to the bit;
//! * the **environment BRDF** (the split sum's second half) from Karis'
//!   analytic fit, so no lookup texture is needed.
//!
//! The renderer adds both terms to every lit material and, when
//! [`Background::Map`] or [`Background::Blurred`] is chosen, draws the map
//! behind the geometry — the skybox.
//!
//! # Where maps come from
//!
//! * [`EnvMap::from_hdr`] reads Radiance `.hdr` (RGBE, flat and new-style
//!   run-length scanlines) — the format HDRI libraries publish;
//!   [`EnvMap::to_hdr`] writes it back.
//! * [`EnvMap::sky`] is a procedural daylight sky — a Preetham-flavoured
//!   gradient with Rayleigh-ish blue overhead, a warm horizon and a sun disc
//!   with a Mie halo — so a scene can be lit outdoors without any file.
//! * [`EnvMap::studio`] is a photographer's softbox room; [`EnvMap::gradient`]
//!   a three-colour dome.

use std::f32::consts::PI;
use std::sync::Arc;

use crate::math::Vec3;
use crate::scene::Rgb;

/// An equirectangular radiance map: `u` wraps longitude (−π..π around +y,
/// starting at −z), `v` runs from the zenith (0) to the nadir (1).
#[derive(Debug, Clone, PartialEq)]
pub struct EnvMap {
    width: usize,
    height: usize,
    data: Vec<Rgb>,
}

/// Direction → `(u, v)` in 0..1.
#[must_use]
pub fn dir_to_uv(d: Vec3) -> (f32, f32) {
    let d = d.normalize();
    let u = 0.5 + d.x.atan2(-d.z) / (2.0 * PI);
    let v = d.y.clamp(-1.0, 1.0).acos() / PI;
    (u, v)
}

/// `(u, v)` → unit direction (inverse of [`dir_to_uv`]).
#[must_use]
pub fn uv_to_dir(u: f32, v: f32) -> Vec3 {
    let phi = (u - 0.5) * 2.0 * PI;
    let theta = v * PI;
    let s = theta.sin();
    Vec3::new(s * phi.sin(), theta.cos(), -s * phi.cos())
}

impl EnvMap {
    /// From linear pixels, row-major, top row = zenith.
    ///
    /// # Panics
    /// If `data.len() != width * height` or either side is zero.
    #[must_use]
    pub fn new(width: usize, height: usize, data: Vec<Rgb>) -> Self {
        assert!(width > 0 && height > 0, "an environment map needs pixels");
        assert_eq!(data.len(), width * height, "pixel count must match size");
        Self {
            width,
            height,
            data,
        }
    }

    /// A map computed per direction.
    #[must_use]
    pub fn from_fn(width: usize, height: usize, f: impl Fn(Vec3) -> Rgb) -> Self {
        let mut data = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                #[allow(clippy::cast_precision_loss)]
                let (u, v) = (
                    (x as f32 + 0.5) / width as f32,
                    (y as f32 + 0.5) / height as f32,
                );
                data.push(f(uv_to_dir(u, v)));
            }
        }
        Self::new(width, height, data)
    }

    /// One colour everywhere.
    #[must_use]
    pub fn uniform(c: Rgb) -> Self {
        Self::new(4, 2, vec![c; 8])
    }

    /// A dome: `top` at the zenith, `horizon` at the equator, `bottom` at
    /// the nadir, blended smoothly.
    #[must_use]
    pub fn gradient(top: Rgb, horizon: Rgb, bottom: Rgb) -> Self {
        Self::from_fn(64, 32, |d| {
            if d.y >= 0.0 {
                horizon.lerp(top, d.y.powf(0.6))
            } else {
                horizon.lerp(bottom, (-d.y).powf(0.4))
            }
        })
    }

    /// A procedural daylight sky (see the module docs). `sun` points *at*
    /// the sun; `turbidity` (1 = crisp, 10 = hazy) widens the halo and
    /// warms the horizon. The sun disc carries `sun_intensity` radiance so it
    /// dominates specular highlights the way a real sun does.
    #[must_use]
    pub fn sky(sun: Vec3, turbidity: f32, sun_intensity: f32) -> Self {
        let sun = sun.normalize();
        let haze = (turbidity.clamp(1.0, 12.0) - 1.0) / 11.0;
        let zenith = Rgb::new(0.13, 0.32, 0.85).lerp(Rgb::new(0.45, 0.55, 0.75), haze);
        let horizon = Rgb::new(0.75, 0.82, 0.95).lerp(Rgb::new(0.95, 0.85, 0.75), haze);
        let ground = Rgb::new(0.18, 0.16, 0.14);
        // Low suns redden the whole sky.
        let elevation = sun.y.clamp(-0.2, 1.0);
        let dusk = (1.0 - (elevation * 3.0).clamp(0.0, 1.0)).powi(2);
        let dusk_tint = Rgb::new(1.0, 0.55, 0.3);
        Self::from_fn(256, 128, move |d| {
            let cos_sun = d.dot(sun).clamp(-1.0, 1.0);
            let base = if d.y >= 0.0 {
                let k = d.y.powf(0.45);
                horizon.lerp(zenith, k)
            } else {
                horizon.scale(0.6).lerp(ground, (-d.y).powf(0.3))
            };
            let base = base.lerp(base.mul(dusk_tint), dusk * (1.0 - d.y.abs()).powi(2));
            // Rayleigh-like brightening near the sun, Mie halo, disc.
            let mie = (1.0 - haze * 0.5) * 0.25 * ((cos_sun - 1.0) * (6.0 - 4.0 * haze)).exp();
            let halo = 2.5 * ((cos_sun - 1.0) * 120.0).exp();
            let disc = if cos_sun > 0.9995 { sun_intensity } else { 0.0 };
            let sun_col = Rgb::new(1.0, 0.95, 0.85).lerp(dusk_tint, dusk);
            let above = if d.y > -0.02 { 1.0 } else { 0.0 };
            base.add(sun_col.scale((mie + halo + disc) * above))
        })
    }

    /// A studio: dark walls, a big soft key light overhead-front, a strip
    /// rim light behind, a warm bounce card low on one side.
    #[must_use]
    pub fn studio(key: f32) -> Self {
        let soft = |d: Vec3, c: Vec3, size: f32| {
            let x = d.dot(c.normalize()).clamp(-1.0, 1.0);
            let t = ((x - (1.0 - size)) / size).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        Self::from_fn(256, 128, move |d| {
            let wall =
                Rgb::new(0.03, 0.03, 0.035).lerp(Rgb::new(0.08, 0.08, 0.09), (d.y + 1.0) * 0.5);
            let k = soft(d, Vec3::new(0.3, 0.8, 0.6), 0.25) * key * 6.0;
            let rim = soft(d, Vec3::new(-0.2, 0.3, -1.0), 0.08) * key * 9.0;
            let fill = soft(d, Vec3::new(-1.0, -0.1, 0.4), 0.2) * key * 1.5;
            wall.add(Rgb::new(1.0, 1.0, 1.0).scale(k))
                .add(Rgb::new(0.8, 0.9, 1.0).scale(rim))
                .add(Rgb::new(1.0, 0.8, 0.6).scale(fill))
        })
    }

    #[must_use]
    pub const fn width(&self) -> usize {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> usize {
        self.height
    }

    #[must_use]
    pub fn pixels(&self) -> &[Rgb] {
        &self.data
    }

    fn texel(&self, x: isize, y: isize) -> Rgb {
        #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
        let x = x.rem_euclid(self.width as isize) as usize;
        #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
        let y = y.clamp(0, self.height as isize - 1) as usize;
        self.data[y * self.width + x]
    }

    /// Bilinear sample at `(u, v)`; `u` wraps, `v` clamps.
    #[must_use]
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    pub fn sample_uv(&self, u: f32, v: f32) -> Rgb {
        let fx = u * self.width as f32 - 0.5;
        let fy = v * self.height as f32 - 0.5;
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let (x0, y0) = (x0 as isize, y0 as isize);
        let a = self.texel(x0, y0).lerp(self.texel(x0 + 1, y0), tx);
        let b = self.texel(x0, y0 + 1).lerp(self.texel(x0 + 1, y0 + 1), tx);
        a.lerp(b, ty)
    }

    /// Radiance arriving from direction `d`.
    #[must_use]
    pub fn sample(&self, d: Vec3) -> Rgb {
        let (u, v) = dir_to_uv(d);
        self.sample_uv(u, v)
    }

    /// Half resolution, box-filtered (used to build the specular chain).
    #[must_use]
    pub fn downsample(&self) -> Self {
        let (w, h) = ((self.width / 2).max(1), (self.height / 2).max(1));
        let mut data = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                #[allow(clippy::cast_possible_wrap)]
                let (sx, sy) = ((x * 2) as isize, (y * 2) as isize);
                let s = self
                    .texel(sx, sy)
                    .add(self.texel(sx + 1, sy))
                    .add(self.texel(sx, sy + 1))
                    .add(self.texel(sx + 1, sy + 1));
                data.push(s.scale(0.25));
            }
        }
        Self::new(w, h, data)
    }

    /// Rotate about +y by `angle` radians (an HDRI's "rotation" slider).
    #[must_use]
    pub fn rotated(&self, angle: f32) -> Self {
        let (s, c) = angle.sin_cos();
        Self::from_fn(self.width, self.height, |d| {
            self.sample(Vec3::new(c * d.x + s * d.z, d.y, -s * d.x + c * d.z))
        })
    }

    /// Mean radiance weighted by solid angle — the map's average brightness.
    #[must_use]
    pub fn mean(&self) -> Rgb {
        let mut acc = Rgb::BLACK;
        let mut wsum = 0.0;
        for y in 0..self.height {
            #[allow(clippy::cast_precision_loss)]
            let w = ((y as f32 + 0.5) / self.height as f32 * PI).sin();
            for x in 0..self.width {
                acc = acc.add(self.data[y * self.width + x].scale(w));
                wsum += w;
            }
        }
        acc.scale(1.0 / wsum)
    }

    /// Parse a Radiance `.hdr` (`#?RADIANCE` / `#?RGBE`, `FORMAT=32-bit_rle_rgbe`,
    /// `-Y h +X w` orientation; flat and run-length scanlines).
    ///
    /// # Errors
    /// A message naming what was malformed.
    pub fn from_hdr(bytes: &[u8]) -> Result<Self, String> {
        let mut pos = 0;
        let line = |pos: &mut usize| -> Result<String, String> {
            let start = *pos;
            while *pos < bytes.len() && bytes[*pos] != b'\n' {
                *pos += 1;
            }
            if *pos >= bytes.len() {
                return Err("hdr: header ended without a resolution line".into());
            }
            let s = String::from_utf8_lossy(&bytes[start..*pos]).into_owned();
            *pos += 1;
            Ok(s)
        };
        let magic = line(&mut pos)?;
        if !magic.starts_with("#?") {
            return Err("hdr: missing #? magic".into());
        }
        loop {
            let l = line(&mut pos)?;
            if l.is_empty() {
                break;
            }
            if let Some(f) = l.strip_prefix("FORMAT=") {
                if f.trim() != "32-bit_rle_rgbe" {
                    return Err(format!("hdr: unsupported format {f}"));
                }
            }
        }
        let res = line(&mut pos)?;
        let parts: Vec<&str> = res.split_whitespace().collect();
        if parts.len() != 4 || parts[0] != "-Y" || parts[2] != "+X" {
            return Err(format!("hdr: unsupported orientation '{res}'"));
        }
        let h: usize = parts[1].parse().map_err(|_| "hdr: bad height")?;
        let w: usize = parts[3].parse().map_err(|_| "hdr: bad width")?;
        if w == 0 || h == 0 || w > 1 << 15 || h > 1 << 15 {
            return Err("hdr: implausible size".into());
        }
        let mut rgbe = vec![[0u8; 4]; w * h];
        for y in 0..h {
            let row = &mut rgbe[y * w..(y + 1) * w];
            let rle = (8..0x8000).contains(&w)
                && pos + 4 <= bytes.len()
                && bytes[pos] == 2
                && bytes[pos + 1] == 2
                && bytes[pos + 2] & 0x80 == 0;
            if rle {
                let sw = (usize::from(bytes[pos + 2]) << 8) | usize::from(bytes[pos + 3]);
                if sw != w {
                    return Err("hdr: scanline width mismatch".into());
                }
                pos += 4;
                for ch in 0..4 {
                    let mut x = 0;
                    while x < w {
                        let n = *bytes.get(pos).ok_or("hdr: truncated run")? as usize;
                        pos += 1;
                        if n > 128 {
                            let n = n - 128;
                            let v = *bytes.get(pos).ok_or("hdr: truncated run")?;
                            pos += 1;
                            if x + n > w {
                                return Err("hdr: run overflows scanline".into());
                            }
                            for px in &mut row[x..x + n] {
                                px[ch] = v;
                            }
                            x += n;
                        } else {
                            if n == 0 || x + n > w || pos + n > bytes.len() {
                                return Err("hdr: bad literal run".into());
                            }
                            for (i, px) in row[x..x + n].iter_mut().enumerate() {
                                px[ch] = bytes[pos + i];
                            }
                            pos += n;
                            x += n;
                        }
                    }
                }
            } else {
                for px in row.iter_mut() {
                    let q = bytes.get(pos..pos + 4).ok_or("hdr: truncated pixels")?;
                    px.copy_from_slice(q);
                    pos += 4;
                }
            }
        }
        let data = rgbe
            .iter()
            .map(|p| {
                if p[3] == 0 {
                    Rgb::BLACK
                } else {
                    let f = 2f32.powi(i32::from(p[3]) - 136);
                    Rgb::new(
                        f32::from(p[0]) * f,
                        f32::from(p[1]) * f,
                        f32::from(p[2]) * f,
                    )
                }
            })
            .collect();
        Ok(Self::new(w, h, data))
    }

    /// Write as Radiance `.hdr` (flat scanlines).
    #[must_use]
    pub fn to_hdr(&self) -> Vec<u8> {
        let mut out = format!(
            "#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y {} +X {}\n",
            self.height, self.width
        )
        .into_bytes();
        for c in &self.data {
            out.extend_from_slice(&rgbe(*c));
        }
        out
    }
}

/// Encode one linear colour as RGBE.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn rgbe(c: Rgb) -> [u8; 4] {
    let m = c.r.max(c.g).max(c.b);
    if m < 1e-32 {
        return [0, 0, 0, 0];
    }
    // frexp: m = f * 2^e with f in [0.5, 1).
    let e = m.log2().floor() as i32 + 1;
    let scale = 256.0 / 2f32.powi(e);
    [
        (c.r * scale).clamp(0.0, 255.0) as u8,
        (c.g * scale).clamp(0.0, 255.0) as u8,
        (c.b * scale).clamp(0.0, 255.0) as u8,
        (e + 128).clamp(0, 255) as u8,
    ]
}

/// Nine spherical-harmonic coefficients per channel (bands 0–2).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Sh9 {
    pub c: [Rgb; 9],
}

fn sh_basis(d: Vec3) -> [f32; 9] {
    let (x, y, z) = (d.x, d.y, d.z);
    [
        0.282_095,
        0.488_603 * y,
        0.488_603 * z,
        0.488_603 * x,
        1.092_548 * x * y,
        1.092_548 * y * z,
        0.315_392 * (3.0 * z * z - 1.0),
        1.092_548 * x * z,
        0.546_274 * (x * x - y * y),
    ]
}

impl Sh9 {
    /// Accumulate one radiance sample from `dir` with weight `w`.
    pub fn add_sample(&mut self, dir: Vec3, radiance: Rgb, w: f32) {
        let b = sh_basis(dir);
        for (c, k) in self.c.iter_mut().zip(b) {
            *c = c.add(radiance.scale(k * w));
        }
    }

    /// Scale every coefficient.
    #[must_use]
    pub fn scaled(mut self, s: f32) -> Self {
        for c in &mut self.c {
            *c = c.scale(s);
        }
        self
    }

    /// Weighted blend (for probe interpolation and hysteresis).
    #[must_use]
    pub fn lerp(mut self, o: &Self, t: f32) -> Self {
        for (a, b) in self.c.iter_mut().zip(o.c) {
            *a = a.lerp(b, t);
        }
        self
    }

    /// Project a whole map (solid-angle weighted).
    #[must_use]
    pub fn project(map: &EnvMap) -> Self {
        let mut sh = Self::default();
        let (w, h) = (map.width(), map.height());
        #[allow(clippy::cast_precision_loss)]
        let d_omega_base = (2.0 * PI / w as f32) * (PI / h as f32);
        for y in 0..h {
            #[allow(clippy::cast_precision_loss)]
            let v = (y as f32 + 0.5) / h as f32;
            let s = (v * PI).sin();
            for x in 0..w {
                #[allow(clippy::cast_precision_loss)]
                let u = (x as f32 + 0.5) / w as f32;
                sh.add_sample(uv_to_dir(u, v), map.pixels()[y * w + x], d_omega_base * s);
            }
        }
        sh
    }

    /// Irradiance (cosine-convolved radiance) arriving at a surface with
    /// normal `n`, divided by π — i.e. the Lambertian outgoing radiance for
    /// unit albedo.
    #[must_use]
    pub fn irradiance(&self, n: Vec3) -> Rgb {
        // Convolution with the clamped cosine: A0 = π, A1 = 2π/3, A2 = π/4.
        const A: [f32; 9] = [
            PI,
            2.0 * PI / 3.0,
            2.0 * PI / 3.0,
            2.0 * PI / 3.0,
            PI / 4.0,
            PI / 4.0,
            PI / 4.0,
            PI / 4.0,
            PI / 4.0,
        ];
        let b = sh_basis(n);
        let mut e = Rgb::BLACK;
        for i in 0..9 {
            e = e.add(self.c[i].scale(A[i] * b[i]));
        }
        Rgb::new(e.r.max(0.0), e.g.max(0.0), e.b.max(0.0)).scale(1.0 / PI)
    }
}

/// What the renderer paints behind the geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Background {
    /// The scene's flat background colour (the map only lights).
    Color,
    /// The map itself — a skybox.
    Map,
    /// The map blurred to roughly this roughness (0..1) — the "background
    /// blurriness" slider.
    Blurred(f32),
}

/// A lighting environment: the map plus its precomputed integrals.
#[derive(Debug, Clone)]
pub struct Environment {
    pub map: Arc<EnvMap>,
    pub irradiance: Sh9,
    /// Prefiltered radiance: level `k` is convolved for roughness
    /// `k / (levels − 1)`.
    pub specular: Vec<EnvMap>,
    /// Multiplier on both lighting terms.
    pub intensity: f32,
    pub background: Background,
    /// Multiplier on the drawn background only.
    pub background_intensity: f32,
}

fn hammersley(i: u32, n: u32) -> (f32, f32) {
    #[allow(clippy::cast_precision_loss)]
    (
        i as f32 / n as f32,
        i.reverse_bits() as f32 * 2.328_306_4e-10,
    )
}

fn onb(n: Vec3) -> (Vec3, Vec3) {
    let up = if n.y.abs() < 0.999 { Vec3::Y } else { Vec3::X };
    let t = up.cross(n).normalize();
    (t, n.cross(t))
}

impl Environment {
    /// Precompute irradiance and a six-level specular chain.
    #[must_use]
    pub fn new(map: EnvMap) -> Self {
        Self::with_levels(map, 6, 64)
    }

    /// Precompute with `levels` specular levels and `samples` GGX samples
    /// per texel.
    #[must_use]
    pub fn with_levels(map: EnvMap, levels: usize, samples: u32) -> Self {
        let levels = levels.max(2);
        // Work from a ≤ 256-wide copy; prefiltering a 4K HDRI per texel is
        // wasted precision at the roughnesses involved.
        let mut base = map.clone();
        while base.width() > 256 {
            base = base.downsample();
        }
        let irradiance = Sh9::project(&EnvMap::from_fn(64, 32, |d| base.sample(d)));
        // Source mips for filtered importance sampling.
        let mut mips = vec![base.clone()];
        while mips.last().is_some_and(|m| m.width() > 8) {
            let next = mips
                .last()
                .map(EnvMap::downsample)
                .unwrap_or_else(|| base.clone());
            mips.push(next);
        }
        let mut specular = vec![base.clone()];
        for k in 1..levels {
            #[allow(clippy::cast_precision_loss)]
            let roughness = k as f32 / (levels - 1) as f32;
            let w = (base.width() >> k.min(4)).max(16);
            let h = (w / 2).max(8);
            let a = roughness * roughness;
            // Pick the source mip whose texel roughly matches the lobe.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let mip =
                ((roughness * (mips.len() as f32 - 1.0)).round() as usize).min(mips.len() - 1);
            let src = &mips[mip];
            let level = EnvMap::from_fn(w, h, |r| {
                let (t, b) = onb(r);
                let mut acc = Rgb::BLACK;
                let mut wsum = 0.0;
                for i in 0..samples {
                    let (xi1, xi2) = hammersley(i, samples);
                    // GGX half-vector around N = V = R.
                    let phi = 2.0 * PI * xi1;
                    let cos_t = ((1.0 - xi2) / (1.0 + (a * a - 1.0) * xi2)).sqrt();
                    let sin_t = (1.0 - cos_t * cos_t).max(0.0).sqrt();
                    let hv =
                        (t * (sin_t * phi.cos()) + b * (sin_t * phi.sin()) + r * cos_t).normalize();
                    let l = (hv * (2.0 * r.dot(hv)) - r).normalize();
                    let ndl = r.dot(l);
                    if ndl > 0.0 {
                        acc = acc.add(src.sample(l).scale(ndl));
                        wsum += ndl;
                    }
                }
                if wsum > 0.0 {
                    acc.scale(1.0 / wsum)
                } else {
                    src.sample(r)
                }
            });
            specular.push(level);
        }
        Self {
            map: Arc::new(map),
            irradiance,
            specular,
            intensity: 1.0,
            background: Background::Map,
            background_intensity: 1.0,
        }
    }

    #[must_use]
    pub const fn intensity(mut self, i: f32) -> Self {
        self.intensity = i;
        self
    }

    #[must_use]
    pub const fn background(mut self, b: Background) -> Self {
        self.background = b;
        self
    }

    #[must_use]
    pub const fn background_intensity(mut self, i: f32) -> Self {
        self.background_intensity = i;
        self
    }

    /// Diffuse irradiance / π at normal `n` (multiply by albedo).
    #[must_use]
    pub fn diffuse(&self, n: Vec3) -> Rgb {
        self.irradiance.irradiance(n).scale(self.intensity)
    }

    /// Prefiltered radiance along reflection direction `r` at `roughness`.
    #[must_use]
    pub fn specular(&self, r: Vec3, roughness: f32) -> Rgb {
        #[allow(clippy::cast_precision_loss)]
        let f = roughness.clamp(0.0, 1.0) * (self.specular.len() - 1) as f32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let i = (f.floor() as usize).min(self.specular.len() - 2);
        #[allow(clippy::cast_precision_loss)]
        let t = f - i as f32;
        let a = self.specular[i].sample(r);
        let b = self.specular[i + 1].sample(r);
        a.lerp(b, t).scale(self.intensity)
    }

    /// What the camera sees along `d` when nothing is in the way, or `None`
    /// for [`Background::Color`].
    #[must_use]
    pub fn background_sample(&self, d: Vec3) -> Option<Rgb> {
        let c = match self.background {
            Background::Color => return None,
            Background::Map => self.map.sample(d),
            Background::Blurred(r) => self.specular(d, r).scale(1.0 / self.intensity.max(1e-6)),
        };
        Some(c.scale(self.background_intensity))
    }
}

/// Karis' analytic environment BRDF: the `(scale, bias)` applied to F0 for
/// the split-sum specular term.
#[must_use]
pub fn env_brdf(roughness: f32, n_dot_v: f32) -> (f32, f32) {
    let c0 = [-1.0, -0.0275, -0.572, 0.022];
    let c1 = [1.0, 0.0425, 1.04, -0.04];
    let r = [
        roughness * c0[0] + c1[0],
        roughness * c0[1] + c1[1],
        roughness * c0[2] + c1[2],
        roughness * c0[3] + c1[3],
    ];
    let a004 = (r[0] * r[0]).min((-9.28 * n_dot_v).exp2()) * r[0] + r[1];
    (-1.04 * a004 + r[2], 1.04 * a004 + r[3])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn uv_and_direction_round_trip() {
        for d in [
            Vec3::new(0.3, 0.5, -0.8),
            Vec3::new(-0.9, -0.2, 0.1),
            Vec3::new(0.0, 0.99, 0.1),
            Vec3::new(0.7, 0.0, 0.7),
        ] {
            let d = d.normalize();
            let (u, v) = dir_to_uv(d);
            let e = uv_to_dir(u, v);
            assert!((d - e).length() < 1e-4, "{d:?} → {e:?}");
        }
    }

    #[test]
    fn a_uniform_sky_irradiates_every_normal_equally() {
        let env = Environment::new(EnvMap::uniform(Rgb::new(0.5, 0.25, 1.0)));
        for n in [
            Vec3::Y,
            -Vec3::Y,
            Vec3::X,
            Vec3::new(0.3, -0.4, 0.8).normalize(),
        ] {
            let e = env.diffuse(n);
            // Uniform radiance L gives E/π = L for every normal.
            assert!(close(e.r, 0.5, 0.02), "{e:?}");
            assert!(close(e.g, 0.25, 0.02), "{e:?}");
            assert!(close(e.b, 1.0, 0.03), "{e:?}");
        }
    }

    #[test]
    fn a_bright_upper_hemisphere_lights_up_facing_normals_more() {
        let map = EnvMap::from_fn(64, 32, |d| if d.y > 0.0 { Rgb::WHITE } else { Rgb::BLACK });
        let env = Environment::new(map);
        let up = env.diffuse(Vec3::Y).r;
        let side = env.diffuse(Vec3::X).r;
        let down = env.diffuse(-Vec3::Y).r;
        assert!(up > side && side > down, "{up} {side} {down}");
        // Exact: upward normal sees full hemisphere → 1.0; sideways 0.5.
        assert!(close(up, 1.0, 0.08), "{up}");
        assert!(close(side, 0.5, 0.05), "{side}");
    }

    #[test]
    fn rougher_specular_levels_spread_a_point_light() {
        let sun = Vec3::new(0.0, 1.0, 0.0);
        let map = EnvMap::from_fn(128, 64, |d| {
            if d.dot(sun) > 0.99 {
                Rgb::new(50.0, 50.0, 50.0)
            } else {
                Rgb::BLACK
            }
        });
        let env = Environment::new(map);
        let off = Vec3::new(0.5, 0.86, 0.0).normalize();
        let sharp_on = env.specular(sun, 0.0).r;
        let sharp_off = env.specular(off, 0.0).r;
        let rough_off = env.specular(off, 1.0).r;
        assert!(sharp_on > 10.0);
        assert!(sharp_off < 1e-3);
        assert!(rough_off > sharp_off, "{rough_off}");
    }

    #[test]
    fn hdr_round_trips_through_rgbe_within_its_mantissa() {
        let map = EnvMap::sky(Vec3::new(0.3, 0.6, -0.5), 3.0, 40.0);
        let bytes = map.to_hdr();
        let back = EnvMap::from_hdr(&bytes).expect("parse");
        assert_eq!((back.width(), back.height()), (map.width(), map.height()));
        for (a, b) in map.pixels().iter().zip(back.pixels()) {
            let m = a.r.max(a.g).max(a.b);
            assert!((a.r - b.r).abs() <= m / 64.0 + 1e-6);
            assert!((a.b - b.b).abs() <= m / 64.0 + 1e-6);
        }
    }

    #[test]
    fn hdr_reads_run_length_scanlines() {
        // One 8-wide row, each channel a single run.
        let mut f = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 1 +X 8\n".to_vec();
        f.extend_from_slice(&[2, 2, 0, 8]);
        for v in [128u8, 64, 32, 129] {
            f.extend_from_slice(&[128 + 8, v]);
        }
        let map = EnvMap::from_hdr(&f).expect("rle");
        let p = map.pixels()[5];
        // 128 * 2^(129-136) = 1.0
        assert!(close(p.r, 1.0, 1e-6) && close(p.g, 0.5, 1e-6) && close(p.b, 0.25, 1e-6));
    }

    #[test]
    fn malformed_hdr_is_refused_with_a_reason() {
        assert!(EnvMap::from_hdr(b"nope").is_err());
        assert!(EnvMap::from_hdr(b"#?RADIANCE\n\n+Y 1 +X 1\n").is_err());
        assert!(EnvMap::from_hdr(b"#?RADIANCE\n\n-Y 2 +X 2\n\x01\x02").is_err());
    }

    #[test]
    fn env_brdf_is_bounded_and_falls_with_roughness_at_grazing() {
        for r in [0.0, 0.3, 0.7, 1.0] {
            for nv in [0.05, 0.5, 1.0] {
                let (a, b) = env_brdf(r, nv);
                assert!((0.0..=1.05).contains(&(a + b)), "{r} {nv} {a} {b}");
            }
        }
        let smooth = env_brdf(0.05, 0.1);
        let rough = env_brdf(0.95, 0.1);
        assert!(smooth.0 + smooth.1 > rough.0 + rough.1);
    }

    #[test]
    fn the_sky_is_blue_overhead_and_brightest_at_the_sun() {
        let sun = Vec3::new(0.4, 0.5, -0.6).normalize();
        let sky = EnvMap::sky(sun, 2.0, 30.0);
        let up = sky.sample(Vec3::Y);
        assert!(up.b > up.r, "{up:?}");
        assert!(sky.sample(sun).r > 10.0);
        let rot = sky.rotated(1.0);
        assert!(rot.sample(sun).r < sky.sample(sun).r);
    }

    #[test]
    fn mean_of_a_uniform_map_is_its_colour() {
        let m = EnvMap::uniform(Rgb::new(0.2, 0.4, 0.6)).mean();
        assert!(close(m.r, 0.2, 1e-5) && close(m.b, 0.6, 1e-5));
    }
}
