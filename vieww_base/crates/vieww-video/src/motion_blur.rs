//! Motion blur — After Effects' layer motion blur (shutter angle and
//! samples per frame), Blender's motion blur, and ReelSmart-style vector
//! blur driven by optical flow.
//!
//! * [`accumulate`] — the ground truth: render the scene at `samples`
//!   instants spread across the open shutter and average them (linear
//!   light, so a bright streak is as bright as it should be). A 180° shutter
//!   at 24 fps is open for 1/48 s, centred on the frame.
//! * [`vector_blur`] — the post-process: smear each pixel along its motion
//!   vector (e.g. a [`FlowField`](crate::cv::FlowField)), for footage
//!   that was not rendered with a shutter.

use vieww_foundation::Image;

use crate::cv::FlowField;

fn to_linear(v: u8) -> f32 {
    let c = f32::from(v) / 255.0;
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn to_srgb(v: f32) -> u8 {
    let c = v.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let b = (s * 255.0 + 0.5) as u8;
    b
}

/// A camera shutter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shutter {
    /// Degrees of the frame the shutter is open (180 = half the frame).
    pub angle: f64,
    /// −0.5 centres the exposure on the frame time (AE's default phase is
    /// −angle/2); 0 opens at the frame time.
    pub phase: f64,
    pub samples: u32,
}

impl Default for Shutter {
    fn default() -> Self {
        Self {
            angle: 180.0,
            phase: -0.5,
            samples: 8,
        }
    }
}

impl Shutter {
    /// The sample instants for the frame at `t` lasting `frame` seconds.
    #[must_use]
    pub fn instants(&self, t: f64, frame: f64) -> Vec<f64> {
        let open = frame * self.angle / 360.0;
        let start = t + open * self.phase;
        let n = self.samples.max(1);
        (0..n)
            .map(|i| start + open * (f64::from(i) + 0.5) / f64::from(n))
            .collect()
    }
}

/// Render at every shutter instant and average in linear light.
///
/// # Panics
/// If renders disagree in size.
pub fn accumulate(render: impl Fn(f64) -> Image, t: f64, frame: f64, shutter: Shutter) -> Image {
    let times = shutter.instants(t, frame);
    let first = render(times[0]);
    let (w, h) = (first.width(), first.height());
    let mut acc: Vec<f32> = first.pixels().iter().enumerate().map(|(i, &v)| if i % 4 == 3 { f32::from(v) } else { to_linear(v) }).collect();
    for &ti in &times[1..] {
        let img = render(ti);
        assert_eq!((img.width(), img.height()), (w, h), "render size changed");
        for (i, (a, &v)) in acc.iter_mut().zip(img.pixels()).enumerate() {
            *a += if i % 4 == 3 { f32::from(v) } else { to_linear(v) };
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let n = times.len() as f32;
    let px = acc
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            if i % 4 == 3 {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let a = (v / n + 0.5) as u8;
                a
            } else {
                to_srgb(v / n)
            }
        })
        .collect();
    Image::from_rgba8(px, w, h)
}

/// Smear `img` along `flow` (pixels per frame), scaled by `strength`
/// (the shutter fraction), with `samples` taps centred on each pixel.
#[must_use]
pub fn vector_blur(img: &Image, flow: &FlowField, strength: f32, samples: u32) -> Image {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let src = img.pixels();
    let mut out = vec![0u8; w * h * 4];
    let n = samples.max(1);
    for y in 0..h {
        for x in 0..w {
            let (u, v) = flow.at(x, y);
            let mut acc = [0f32; 4];
            for k in 0..n {
                #[allow(clippy::cast_precision_loss)]
                let s = ((k as f32 + 0.5) / n as f32 - 0.5) * strength;
                #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                let (sx, sy) = (
                    (x as f32 + u * s).round().clamp(0.0, (w - 1) as f32) as usize,
                    (y as f32 + v * s).round().clamp(0.0, (h - 1) as f32) as usize,
                );
                let o = (sy * w + sx) * 4;
                for c in 0..3 {
                    acc[c] += to_linear(src[o + c]);
                }
                acc[3] += f32::from(src[o + 3]);
            }
            #[allow(clippy::cast_precision_loss)]
            let nf = n as f32;
            let o = (y * w + x) * 4;
            for c in 0..3 {
                out[o + c] = to_srgb(acc[c] / nf);
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                out[o + 3] = (acc[3] / nf + 0.5) as u8;
            }
        }
    }
    #[allow(clippy::cast_possible_truncation)]
    Image::from_rgba8(out, w as u32, h as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A white 2-px bar at x = speed·t on black, 40 px wide.
    fn bar(t: f64) -> Image {
        let x0 = (100.0 * t) as usize;
        let px = (0..40 * 4)
            .flat_map(|i| {
                let x = i % 40;
                if x >= x0 && x < x0 + 2 {
                    [255, 255, 255, 255]
                } else {
                    [0, 0, 0, 255]
                }
            })
            .collect();
        Image::from_rgba8(px, 40, 4)
    }

    #[test]
    fn shutter_instants_are_centred_and_span_the_angle() {
        let s = Shutter {
            angle: 180.0,
            phase: -0.5,
            samples: 4,
        };
        let ts = s.instants(1.0, 1.0 / 24.0);
        let mean = ts.iter().sum::<f64>() / 4.0;
        assert!((mean - 1.0).abs() < 1e-12);
        assert!((ts[3] - ts[0] - 0.75 / 48.0).abs() < 1e-12);
    }

    #[test]
    fn a_moving_bar_streaks_and_keeps_its_energy() {
        let img = accumulate(bar, 0.1, 0.1, Shutter { angle: 360.0, phase: -0.5, samples: 20 });
        let row: Vec<u8> = (0..40).map(|x| img.pixels()[x * 4]).collect();
        let lit = row.iter().filter(|&&v| v > 10).count();
        assert!(lit > 6, "streak {row:?}");
        // Energy in linear light ≈ 2 px of white.
        let e: f32 = row.iter().map(|&v| to_linear(v)).sum();
        assert!((e - 2.0).abs() < 0.5, "{e}");
        // A still frame (zero-angle shutter) stays sharp.
        let sharp = accumulate(bar, 0.1, 0.1, Shutter { angle: 0.0, phase: 0.0, samples: 4 });
        assert_eq!((0..40).filter(|&x| sharp.pixels()[x * 4] > 10).count(), 2);
    }

    #[test]
    fn vector_blur_smears_only_where_things_move() {
        let img = bar(0.1);
        let mut flow = FlowField {
            w: 40,
            h: 4,
            u: vec![0.0; 160],
            v: vec![0.0; 160],
        };
        let still = vector_blur(&img, &flow, 1.0, 8);
        assert_eq!(still.pixels(), img.pixels());
        flow.u = vec![8.0; 160];
        let moved = vector_blur(&img, &flow, 1.0, 8);
        assert!((0..40).filter(|&x| moved.pixels()[x * 4] > 10).count() > 4);
    }
}
