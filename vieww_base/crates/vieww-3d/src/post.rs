//! Post-processing — Three.js' `EffectComposer` + passes, drei's
//! `@react-three/postprocessing`, Unity's post stack, Blender EEVEE's
//! render settings (§2.2, §2.3 L6, §2.14 L6).
//!
//! A [`PostStack`] is an ordered list of [`Pass`]es run over a
//! [`Frame`](crate::render::Frame) — linear HDR colour plus view-space
//! position and normal — producing an sRGB [`Image`]. Every pass is a
//! readable CPU implementation:
//!
//! | pass | what |
//! |---|---|
//! | [`Pass::Ssao`] | screen-space ambient occlusion: hemisphere samples around the normal, re-projected and depth-compared, range-checked, then a 4×4 blur |
//! | [`Pass::Ssr`] | screen-space reflections: the view ray reflected about the normal, marched through the depth buffer, faded by Fresnel and screen edge |
//! | [`Pass::DepthOfField`] | thin-lens circle of confusion from depth, gathered with a disc kernel, foreground-aware |
//! | [`Pass::Bloom`] | bright-pass threshold, a mip chain of separable Gaussians, additive |
//! | [`Pass::DepthFog`] | exponential height-free fog by view distance |
//! | [`Pass::ToneMap`] | exposure + ACES (Narkowicz), Reinhard, filmic (Hable) or none |
//! | [`Pass::ColorGrade`] | lift / gamma / gain, saturation, contrast, temperature |
//! | [`Pass::ChromaticAberration`] | radial RGB split |
//! | [`Pass::Vignette`] | smooth radial darkening |
//! | [`Pass::Grain`] | seeded film grain, luminance-weighted |
//! | [`Pass::Fxaa`] | luma-edge antialiasing (FXAA 3.11's idea, simplified) |
//!
//! Passes before `ToneMap` work in linear HDR; `Grain`, `Vignette` and
//! `Fxaa` are typically placed after it. The output is sRGB-encoded.

use vieww_foundation::Image;

use crate::math::Vec3;
use crate::render::Frame;
use crate::scene::Rgb;

/// A tone-mapping curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToneCurve {
    /// Clamp only.
    None,
    Reinhard,
    /// Narkowicz's ACES fit.
    Aces,
    /// Hable's Uncharted 2 filmic curve.
    Filmic,
}

/// One post-processing step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pass {
    Ssao {
        /// World-space sampling radius.
        radius: f32,
        /// 0..1 darkening strength.
        strength: f32,
        samples: u32,
    },
    Ssr {
        /// 0..1 reflection strength at grazing angles.
        strength: f32,
        /// March steps.
        steps: u32,
        /// Max march distance in view units.
        max_distance: f32,
    },
    DepthOfField {
        /// Distance in focus.
        focus: f32,
        /// Circle-of-confusion pixels per unit of |1/focus − 1/depth|.
        aperture: f32,
        /// CoC cap in pixels.
        max_radius: f32,
    },
    Bloom {
        threshold: f32,
        intensity: f32,
        /// Blur radius in pixels at the first level.
        radius: f32,
    },
    DepthFog {
        color: Rgb,
        density: f32,
    },
    ToneMap {
        curve: ToneCurve,
        exposure: f32,
    },
    ColorGrade {
        lift: f32,
        gamma: f32,
        gain: f32,
        saturation: f32,
        contrast: f32,
        /// −1 cool … +1 warm.
        temperature: f32,
    },
    ChromaticAberration {
        /// Pixels of R/B offset at the frame corner.
        amount: f32,
    },
    Vignette {
        /// 0..1.
        strength: f32,
        /// Radius where darkening starts, 0..1 of the half-diagonal.
        radius: f32,
    },
    Grain {
        amount: f32,
        seed: u32,
    },
    Fxaa,
}

/// Passes in order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostStack {
    pub passes: Vec<Pass>,
}

impl PostStack {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with(mut self, p: Pass) -> Self {
        self.passes.push(p);
        self
    }

    /// A good-looking default: SSAO, bloom, ACES, a touch of vignette, FXAA.
    #[must_use]
    pub fn cinematic() -> Self {
        Self::new()
            .with(Pass::Ssao {
                radius: 0.5,
                strength: 0.8,
                samples: 12,
            })
            .with(Pass::Bloom {
                threshold: 1.0,
                intensity: 0.6,
                radius: 4.0,
            })
            .with(Pass::ToneMap {
                curve: ToneCurve::Aces,
                exposure: 1.0,
            })
            .with(Pass::Vignette {
                strength: 0.35,
                radius: 0.55,
            })
            .with(Pass::Fxaa)
    }

    /// Run every pass over `frame` (consumed) and encode to sRGB.
    #[must_use]
    pub fn apply(&self, mut frame: Frame) -> Image {
        for p in &self.passes {
            match *p {
                Pass::Ssao {
                    radius,
                    strength,
                    samples,
                } => ssao(&mut frame, radius, strength, samples),
                Pass::Ssr {
                    strength,
                    steps,
                    max_distance,
                } => ssr(&mut frame, strength, steps, max_distance),
                Pass::DepthOfField {
                    focus,
                    aperture,
                    max_radius,
                } => dof(&mut frame, focus, aperture, max_radius),
                Pass::Bloom {
                    threshold,
                    intensity,
                    radius,
                } => bloom(&mut frame, threshold, intensity, radius),
                Pass::DepthFog { color, density } => {
                    for (c, p) in frame.color.iter_mut().zip(&frame.position) {
                        let d = -p.z;
                        let f = if d.is_finite() {
                            1.0 - (-density * d).exp()
                        } else {
                            1.0
                        };
                        *c = c.lerp(color, f.clamp(0.0, 1.0));
                    }
                }
                Pass::ToneMap { curve, exposure } => {
                    for c in &mut frame.color {
                        *c = tone(c.scale(exposure), curve);
                    }
                }
                Pass::ColorGrade {
                    lift,
                    gamma,
                    gain,
                    saturation,
                    contrast,
                    temperature,
                } => {
                    for c in &mut frame.color {
                        *c = grade(*c, lift, gamma, gain, saturation, contrast, temperature);
                    }
                }
                Pass::ChromaticAberration { amount } => chroma(&mut frame, amount),
                Pass::Vignette { strength, radius } => vignette(&mut frame, strength, radius),
                Pass::Grain { amount, seed } => grain(&mut frame, amount, seed),
                Pass::Fxaa => fxaa(&mut frame),
            }
        }
        frame.to_image()
    }
}

fn luma(c: Rgb) -> f32 {
    0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b
}

/// Map an HDR colour through a curve into `0..1`.
#[must_use]
pub fn tone(c: Rgb, curve: ToneCurve) -> Rgb {
    let f: fn(f32) -> f32 = match curve {
        ToneCurve::None => |x| x.clamp(0.0, 1.0),
        ToneCurve::Reinhard => |x| x / (1.0 + x),
        ToneCurve::Aces => |x| {
            let (a, b, c, d, e) = (2.51, 0.03, 2.43, 0.59, 0.14);
            ((x * (a * x + b)) / (x * (c * x + d) + e)).clamp(0.0, 1.0)
        },
        ToneCurve::Filmic => |x| {
            let h = |v: f32| {
                let (a, b, c, d, e, f) = (0.15, 0.50, 0.10, 0.20, 0.02, 0.30);
                ((v * (a * v + c * b) + d * e) / (v * (a * v + b) + d * f)) - e / f
            };
            (h(x * 2.0) / h(11.2)).clamp(0.0, 1.0)
        },
    };
    Rgb::new(f(c.r.max(0.0)), f(c.g.max(0.0)), f(c.b.max(0.0)))
}

#[allow(clippy::too_many_arguments)]
fn grade(c: Rgb, lift: f32, gamma: f32, gain: f32, sat: f32, contrast: f32, temp: f32) -> Rgb {
    let ch = |v: f32| {
        let v = (v * gain + lift * (1.0 - v)).max(0.0);
        let v = v.powf(1.0 / gamma.max(1e-3));
        (v - 0.5) * contrast + 0.5
    };
    let mut o = Rgb::new(ch(c.r), ch(c.g), ch(c.b));
    let l = luma(o);
    o = Rgb::new(
        l + (o.r - l) * sat,
        l + (o.g - l) * sat,
        l + (o.b - l) * sat,
    );
    Rgb::new(
        (o.r * (1.0 + 0.1 * temp)).max(0.0),
        o.g.max(0.0),
        (o.b * (1.0 - 0.1 * temp)).max(0.0),
    )
}

/// Project a view-space point to pixel coordinates.
fn to_screen(f: &Frame, p: Vec3) -> Option<(f32, f32)> {
    let c = f.projection.mul_vec4([p.x, p.y, p.z, 1.0]);
    if c[3] <= 1e-6 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (f.width as f32, f.height as f32);
    Some(((c[0] / c[3] * 0.5 + 0.5) * w, (0.5 - c[1] / c[3] * 0.5) * h))
}

fn hash(x: u32, y: u32, k: u32) -> f32 {
    let mut h =
        x.wrapping_mul(0x8da6_b343) ^ y.wrapping_mul(0xd816_3841) ^ k.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    #[allow(clippy::cast_precision_loss)]
    let v = (h & 0xff_ffff) as f32 / 16_777_216.0;
    v
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn ssao(f: &mut Frame, radius: f32, strength: f32, samples: u32) {
    let (w, h) = (f.width, f.height);
    let mut ao = vec![1.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let p = f.position[i];
            let n = f.normal[i];
            if !p.z.is_finite() || n == Vec3::ZERO {
                continue;
            }
            // Tangent frame around the normal.
            let t = if n.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
            let t = (t - n * n.dot(t)).normalize();
            let b = n.cross(t);
            let mut occ = 0.0;
            for k in 0..samples {
                // Cosine-ish hemisphere, scaled towards the centre.
                let (u, v) = (
                    hash(x as u32, y as u32, 2 * k),
                    hash(x as u32, y as u32, 2 * k + 1),
                );
                let phi = std::f32::consts::TAU * u;
                let r = v.sqrt();
                let dir = t * (r * phi.cos()) + b * (r * phi.sin()) + n * (1.0 - v).sqrt();
                let s = (k + 1) as f32 / samples as f32;
                let q = p + dir * (radius * (0.1 + 0.9 * s * s));
                let Some((sx, sy)) = to_screen(f, q) else {
                    continue;
                };
                if sx < 0.0 || sy < 0.0 || sx >= w as f32 || sy >= h as f32 {
                    continue;
                }
                let scene_z = f.position[sy as usize * w + sx as usize].z;
                if scene_z >= q.z + 0.02 * radius {
                    let range = (radius / (p.z - scene_z).abs().max(1e-4)).min(1.0);
                    occ += range;
                }
            }
            ao[i] = 1.0 - strength * occ / samples as f32;
        }
    }
    // 4×4 box blur to remove the per-pixel noise pattern.
    for y in 0..h {
        for x in 0..w {
            let (mut s, mut c) = (0.0, 0.0);
            for dy in 0..4usize {
                for dx in 0..4usize {
                    let (xx, yy) = ((x + dx).saturating_sub(2), (y + dy).saturating_sub(2));
                    if xx < w && yy < h {
                        s += ao[yy * w + xx];
                        c += 1.0;
                    }
                }
            }
            let i = y * w + x;
            f.color[i] = f.color[i].scale((s / c).clamp(0.0, 1.0));
        }
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn ssr(f: &mut Frame, strength: f32, steps: u32, max_distance: f32) {
    let (w, h) = (f.width, f.height);
    let src = f.color.clone();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let p = f.position[i];
            let n = f.normal[i];
            if !p.z.is_finite() || n == Vec3::ZERO {
                continue;
            }
            let v = p.normalize();
            let r = v.reflect(n).normalize();
            if r.z > 0.3 {
                continue; // towards the camera: nothing in the buffer to find
            }
            let fres = (1.0 - (-v).dot(n).max(0.0)).powi(5);
            let step = max_distance / steps.max(1) as f32;
            let mut q = p + n * 0.01;
            for _ in 0..steps {
                q += r * step;
                let Some((sx, sy)) = to_screen(f, q) else {
                    break;
                };
                if sx < 0.0 || sy < 0.0 || sx >= w as f32 || sy >= h as f32 {
                    break;
                }
                let k = sy as usize * w + sx as usize;
                let z = f.position[k].z;
                if z.is_finite() && z >= q.z && z - q.z < step * 2.0 {
                    let edge = (sx.min(w as f32 - sx).min(sy).min(h as f32 - sy)
                        / (0.1 * w as f32))
                        .clamp(0.0, 1.0);
                    let amt = strength * (0.2 + 0.8 * fres) * edge;
                    f.color[i] = f.color[i].lerp(src[k], amt);
                    break;
                }
            }
        }
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn dof(f: &mut Frame, focus: f32, aperture: f32, max_r: f32) {
    let (w, h) = (f.width, f.height);
    let coc: Vec<f32> = f
        .position
        .iter()
        .map(|p| {
            let d = -p.z;
            let d = if d.is_finite() { d } else { 1e6 };
            (aperture * (1.0 / focus.max(1e-3) - 1.0 / d.max(1e-3)).abs()).min(max_r)
        })
        .collect();
    let src = f.color.clone();
    let rings = 3;
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let r = coc[i];
            if r < 0.5 {
                continue;
            }
            let mut acc = src[i];
            let mut wsum = 1.0;
            for ring in 1..=rings {
                let rr = r * ring as f32 / rings as f32;
                let taps = 6 * ring;
                for t in 0..taps {
                    let a = std::f32::consts::TAU * (t as f32 + 0.5 * ring as f32) / taps as f32;
                    let sx = (x as f32 + rr * a.cos()).round();
                    let sy = (y as f32 + rr * a.sin()).round();
                    if sx < 0.0 || sy < 0.0 || sx >= w as f32 || sy >= h as f32 {
                        continue;
                    }
                    let k = sy as usize * w + sx as usize;
                    // A sharp foreground sample does not bleed into a blurry
                    // background pixel's neighbourhood unless it is itself
                    // blurry enough to reach.
                    let wt = if f.position[k].z > f.position[i].z && coc[k] < rr {
                        0.0
                    } else {
                        1.0
                    };
                    acc = acc.add(src[k].scale(wt));
                    wsum += wt;
                }
            }
            f.color[i] = acc.scale(1.0 / wsum);
        }
    }
}

fn gauss_blur(src: &[Rgb], w: usize, h: usize, sigma: f32) -> Vec<Rgb> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let r = (sigma * 3.0).ceil().max(1.0) as usize;
    #[allow(clippy::cast_precision_loss)]
    let k: Vec<f32> = (0..=r)
        .map(|i| (-((i * i) as f32) / (2.0 * sigma * sigma)).exp())
        .collect();
    let norm = k[0] + 2.0 * k[1..].iter().sum::<f32>();
    let mut tmp = vec![Rgb::BLACK; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut a = src[y * w + x].scale(k[0]);
            for i in 1..=r {
                a = a.add(src[y * w + x.saturating_sub(i)].scale(k[i]));
                a = a.add(src[y * w + (x + i).min(w - 1)].scale(k[i]));
            }
            tmp[y * w + x] = a.scale(1.0 / norm);
        }
    }
    let mut out = vec![Rgb::BLACK; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut a = tmp[y * w + x].scale(k[0]);
            for i in 1..=r {
                a = a.add(tmp[y.saturating_sub(i) * w + x].scale(k[i]));
                a = a.add(tmp[(y + i).min(h - 1) * w + x].scale(k[i]));
            }
            out[y * w + x] = a.scale(1.0 / norm);
        }
    }
    out
}

fn downsample(src: &[Rgb], w: usize, h: usize) -> (Vec<Rgb>, usize, usize) {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = vec![Rgb::BLACK; nw * nh];
    for y in 0..nh {
        for x in 0..nw {
            let mut a = Rgb::BLACK;
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                a = a.add(src[(2 * y + dy).min(h - 1) * w + (2 * x + dx).min(w - 1)]);
            }
            out[y * nw + x] = a.scale(0.25);
        }
    }
    (out, nw, nh)
}

#[allow(clippy::cast_precision_loss)]
fn sample_bilinear(src: &[Rgb], w: usize, h: usize, u: f32, v: f32) -> Rgb {
    let x = (u * w as f32 - 0.5).clamp(0.0, w as f32 - 1.0);
    let y = (v * h as f32 - 0.5).clamp(0.0, h as f32 - 1.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (x0, y0) = (x as usize, y as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let a = src[y0 * w + x0].lerp(src[y0 * w + x1], fx);
    let b = src[y1 * w + x0].lerp(src[y1 * w + x1], fx);
    a.lerp(b, fy)
}

#[allow(clippy::cast_precision_loss)]
fn bloom(f: &mut Frame, threshold: f32, intensity: f32, radius: f32) {
    let (w, h) = (f.width, f.height);
    // Soft-knee bright pass.
    let bright: Vec<Rgb> = f
        .color
        .iter()
        .map(|c| {
            let l = luma(*c);
            let k = ((l - threshold) / (l.max(1e-4))).max(0.0);
            c.scale(k)
        })
        .collect();
    let mut levels = Vec::new();
    let (mut cur, mut cw, mut ch) = (bright, w, h);
    for _ in 0..5 {
        let (d, dw, dh) = downsample(&cur, cw, ch);
        let blurred = gauss_blur(&d, dw, dh, radius * 0.5);
        levels.push((blurred.clone(), dw, dh));
        cur = blurred;
        cw = dw;
        ch = dh;
        if dw < 4 || dh < 4 {
            break;
        }
    }
    for y in 0..h {
        for x in 0..w {
            let (u, v) = ((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
            let mut add = Rgb::BLACK;
            for (lv, lw, lh) in &levels {
                add = add.add(sample_bilinear(lv, *lw, *lh, u, v));
            }
            let i = y * w + x;
            f.color[i] = f.color[i].add(add.scale(intensity / levels.len().max(1) as f32));
        }
    }
}

#[allow(clippy::cast_precision_loss)]
fn chroma(f: &mut Frame, amount: f32) {
    let (w, h) = (f.width, f.height);
    let src = f.color.clone();
    for y in 0..h {
        for x in 0..w {
            let (u, v) = ((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
            let (dx, dy) = (u - 0.5, v - 0.5);
            let k = amount / w as f32 * 2.0;
            let r = sample_bilinear(&src, w, h, u + dx * k, v + dy * k).r;
            let b = sample_bilinear(&src, w, h, u - dx * k, v - dy * k).b;
            let i = y * w + x;
            f.color[i] = Rgb::new(r, src[i].g, b);
        }
    }
}

#[allow(clippy::cast_precision_loss)]
fn vignette(f: &mut Frame, strength: f32, radius: f32) {
    let (w, h) = (f.width, f.height);
    let half_diag = ((w * w + h * h) as f32).sqrt() * 0.5;
    for y in 0..h {
        for x in 0..w {
            let d = ((x as f32 + 0.5 - w as f32 * 0.5).powi(2)
                + (y as f32 + 0.5 - h as f32 * 0.5).powi(2))
            .sqrt()
                / half_diag;
            let t = ((d - radius) / (1.0 - radius).max(1e-3)).clamp(0.0, 1.0);
            let s = t * t * (3.0 - 2.0 * t);
            let i = y * w + x;
            f.color[i] = f.color[i].scale(1.0 - strength * s);
        }
    }
}

#[allow(clippy::cast_possible_truncation)]
fn grain(f: &mut Frame, amount: f32, seed: u32) {
    let w = f.width;
    for (i, c) in f.color.iter_mut().enumerate() {
        let n = hash((i % w) as u32, (i / w) as u32, seed) - 0.5;
        let l = luma(*c);
        let k = amount * (1.0 - l).max(0.2);
        *c = Rgb::new(
            (c.r + n * k).max(0.0),
            (c.g + n * k).max(0.0),
            (c.b + n * k).max(0.0),
        );
    }
}

fn fxaa(f: &mut Frame) {
    let (w, h) = (f.width, f.height);
    if w < 3 || h < 3 {
        return;
    }
    let src = f.color.clone();
    let l: Vec<f32> = src.iter().map(|c| luma(*c).sqrt()).collect();
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = y * w + x;
            let (n, s, e, wv) = (l[i - w], l[i + w], l[i + 1], l[i - 1]);
            let m = l[i];
            let lo = m.min(n).min(s).min(e).min(wv);
            let hi = m.max(n).max(s).max(e).max(wv);
            let range = hi - lo;
            if range < (0.0312f32).max(hi * 0.125) {
                continue;
            }
            // Blend along the edge: horizontal edge → blend vertically.
            let horiz = (n + s - 2.0 * m).abs() >= (e + wv - 2.0 * m).abs();
            let (a, b) = if horiz {
                (i - w, i + w)
            } else {
                (i - 1, i + 1)
            };
            let blend = (range / hi.max(1e-4)).min(0.5) * 0.5;
            let avg = src[a].add(src[b]).scale(0.5);
            f.color[i] = src[i].lerp(avg, blend * 2.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{box_mesh, plane, sphere};
    use crate::math::Quat;
    use crate::scene::{Camera, Content, Light, Material, Node, Scene};
    use crate::Renderer;
    use vieww_foundation::Color;

    fn scene() -> Scene {
        let mut s = Scene::new();
        s.background = Rgb::BLACK;
        s.add(
            Node::new(
                "amb",
                Content::Light(Light::Ambient {
                    color: Rgb::WHITE,
                    intensity: 1.0,
                }),
            ),
            None,
        );
        s.add(
            Node::new(
                "floor",
                Content::mesh(plane(10.0, 10.0, 1, 1), Material::lambert(Color::WHITE)),
            )
            .rotated(Quat::from_axis_angle(Vec3::X, -std::f32::consts::FRAC_PI_2)),
            None,
        );
        s.add(
            Node::new(
                "box",
                Content::mesh(box_mesh(1.0, 1.0, 1.0), Material::lambert(Color::WHITE)),
            )
            .at(Vec3::new(0.0, 0.5, 0.0)),
            None,
        );
        s
    }

    fn cam() -> Camera {
        Camera::perspective(Vec3::new(2.0, 2.0, 3.0), Vec3::new(0.0, 0.3, 0.0), 0.9)
    }

    #[test]
    fn frame_matches_render_without_passes() {
        let r = Renderer::new(48, 32);
        let (img, _) = r.render(&mut scene(), &cam());
        let (frame, _) = r.render_frame(&mut scene(), &cam());
        assert_eq!(PostStack::new().apply(frame).pixels(), img.pixels());
    }

    #[test]
    fn gbuffer_has_geometry_where_drawn() {
        let (f, _) = Renderer::new(48, 32).render_frame(&mut scene(), &cam());
        let c = 16 * 48 + 24;
        assert!(f.position[c].z < 0.0 && f.position[c].z.is_finite());
        assert!((f.normal[c].length() - 1.0).abs() < 1e-3);
        assert!(f.depth(24, 16) > 1.0);
    }

    #[test]
    fn ssao_darkens_the_crease_but_not_open_floor() {
        let (f, _) = Renderer::new(96, 64)
            .samples(1)
            .render_frame(&mut scene(), &cam());
        let before = f.color.clone();
        let mut g = f.clone();
        ssao(&mut g, 0.6, 1.0, 16);
        let darkened = before
            .iter()
            .zip(&g.color)
            .filter(|(a, b)| luma(**b) < luma(**a) * 0.9)
            .count();
        assert!(darkened > 20, "{darkened}");
        // Background (no geometry) untouched.
        let bg = before
            .iter()
            .zip(&g.color)
            .filter(|(a, _)| luma(**a) == 0.0);
        for (a, b) in bg {
            assert_eq!(luma(*a), luma(*b));
        }
    }

    #[test]
    fn bloom_spreads_light_into_dark_neighbours() {
        let mut f = Frame {
            width: 32,
            height: 32,
            color: vec![Rgb::BLACK; 32 * 32],
            position: vec![Vec3::new(0.0, 0.0, f32::NEG_INFINITY); 32 * 32],
            normal: vec![Vec3::ZERO; 32 * 32],
            projection: crate::math::Mat4::IDENTITY,
        };
        f.color[16 * 32 + 16] = Rgb::new(50.0, 50.0, 50.0);
        bloom(&mut f, 1.0, 1.0, 2.0);
        assert!(luma(f.color[16 * 32 + 20]) > 0.01);
        assert!(luma(f.color[0]) < luma(f.color[16 * 32 + 18]));
    }

    #[test]
    fn tone_curves_are_monotonic_and_bounded() {
        for c in [ToneCurve::Reinhard, ToneCurve::Aces, ToneCurve::Filmic] {
            let mut prev = -1.0;
            for i in 0..200 {
                #[allow(clippy::cast_precision_loss)]
                let v = tone(Rgb::new(i as f32 * 0.1, 0.0, 0.0), c).r;
                assert!((0.0..=1.0).contains(&v));
                assert!(v >= prev);
                prev = v;
            }
        }
        assert_eq!(
            tone(Rgb::new(5.0, 0.5, -1.0), ToneCurve::None),
            Rgb::new(1.0, 0.5, 0.0)
        );
    }

    #[test]
    fn depth_of_field_blurs_out_of_focus_only() {
        let mut s = scene();
        s.add(
            Node::new(
                "ball",
                Content::mesh(sphere(0.3, 16, 8), Material::basic(Color::RED)),
            )
            .at(Vec3::new(-2.0, 0.3, -4.0)),
            None,
        );
        let (f, _) = Renderer::new(96, 64)
            .samples(1)
            .render_frame(&mut s, &cam());
        let focus = f.depth(48, 32);
        let mut g = f.clone();
        dof(&mut g, focus, 40.0, 6.0);
        assert_eq!(
            g.color[32 * 96 + 48],
            f.color[32 * 96 + 48],
            "in focus stays sharp"
        );
        let changed = f.color.iter().zip(&g.color).filter(|(a, b)| a != b).count();
        assert!(changed > 50);
    }

    #[test]
    fn ssr_reflects_a_box_in_a_mirror_floor() {
        let mut s = scene();
        s.add(
            Node::new(
                "red",
                Content::mesh(box_mesh(0.8, 0.8, 0.8), Material::basic(Color::RED)),
            )
            .at(Vec3::new(-0.2, 0.4, -1.2)),
            None,
        );
        let (f, _) = Renderer::new(96, 64)
            .samples(1)
            .render_frame(&mut s, &cam());
        let mut g = f.clone();
        ssr(&mut g, 1.0, 64, 6.0);
        let changed = f.color.iter().zip(&g.color).filter(|(a, b)| a != b).count();
        assert!(changed > 10, "{changed}");
    }

    #[test]
    fn grade_vignette_grain_chroma_fxaa_run_and_stay_finite() {
        let (f, _) = Renderer::new(64, 48).render_frame(&mut scene(), &cam());
        let img = PostStack::cinematic()
            .with(Pass::ColorGrade {
                lift: 0.02,
                gamma: 1.1,
                gain: 1.05,
                saturation: 1.2,
                contrast: 1.1,
                temperature: 0.3,
            })
            .with(Pass::ChromaticAberration { amount: 2.0 })
            .with(Pass::Grain {
                amount: 0.05,
                seed: 3,
            })
            .with(Pass::DepthFog {
                color: Rgb::new(0.1, 0.1, 0.2),
                density: 0.02,
            })
            .apply(f);
        assert_eq!((img.width(), img.height()), (64, 48));
        // Vignette: corners darker than the centre on a uniform image.
        let mut flat = Frame {
            width: 40,
            height: 40,
            color: vec![Rgb::new(0.5, 0.5, 0.5); 1600],
            position: vec![Vec3::new(0.0, 0.0, f32::NEG_INFINITY); 1600],
            normal: vec![Vec3::ZERO; 1600],
            projection: crate::math::Mat4::IDENTITY,
        };
        vignette(&mut flat, 0.5, 0.3);
        assert!(luma(flat.color[0]) < luma(flat.color[20 * 40 + 20]));
    }

    #[test]
    fn fxaa_softens_a_hard_edge() {
        let mut f = Frame {
            width: 8,
            height: 8,
            color: (0..64)
                .map(|i| if i % 8 < 4 { Rgb::BLACK } else { Rgb::WHITE })
                .collect(),
            position: vec![Vec3::ZERO; 64],
            normal: vec![Vec3::ZERO; 64],
            projection: crate::math::Mat4::IDENTITY,
        };
        fxaa(&mut f);
        let edge = f.color[3 * 8 + 4];
        assert!(edge.r < 1.0 && edge.r > 0.0);
    }
}
