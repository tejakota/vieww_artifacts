//! Keying and mattes — After Effects' Keylight, Luma Key, Difference
//! Matte and track mattes (§2.15 L2/L7).
//!
//! A key turns footage into alpha: [`chroma_key`] measures each pixel's
//! distance from the key colour in a **luma-normalised CbCr plane** (so a shadow on
//! the green screen, darker but just as green, still keys), with a
//! tolerance and a soft edge, then suppresses spill by clamping the key
//! channel to the others. [`luma_key`] keys by brightness, and
//! [`difference_key`] against a clean plate. [`track_matte`] cuts one layer
//! by another's alpha or luminance, inverted or not — the four AE track
//! matte modes.

use vieww_foundation::{Color, Image};

/// Chroma normalised by luma (scaled to the 0..255 range), so a shadowed
/// patch of screen — same hue, less light — lands where the lit screen
/// does. Very dark pixels are floored so sensor noise is not amplified.
fn cbcr(r: f32, g: f32, b: f32) -> (f32, f32) {
    let y = (0.299 * r + 0.587 * g + 0.114 * b).max(24.0);
    let k = 128.0 / y;
    (
        (-0.168_736 * r - 0.331_264 * g + 0.5 * b) * k,
        (0.5 * r - 0.418_688 * g - 0.081_312 * b) * k,
    )
}

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0).max(1e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn q(v: f32) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

/// Key out `key`: pixels within `tolerance` (chroma distance, 0..~180)
/// become transparent, fading to opaque over `softness`; green/blue spill
/// on the survivors is clamped.
#[must_use]
pub fn chroma_key(frame: &Image, key: Color, tolerance: f32, softness: f32, despill: bool) -> Image {
    let (kb, kr) = cbcr(f32::from(key.r), f32::from(key.g), f32::from(key.b));
    let spill = if key.g >= key.b && key.g >= key.r { 1 } else if key.b >= key.r { 2 } else { 0 };
    let mut out = frame.pixels().to_vec();
    for px in out.chunks_exact_mut(4) {
        let (r, g, b) = (f32::from(px[0]), f32::from(px[1]), f32::from(px[2]));
        let (cb, cr) = cbcr(r, g, b);
        let d = ((cb - kb).powi(2) + (cr - kr).powi(2)).sqrt();
        let a = smooth(tolerance, tolerance + softness, d);
        px[3] = q(f32::from(px[3]) * a);
        if despill {
            let others = match spill {
                1 => (r + b) / 2.0,
                2 => (r + g) / 2.0,
                _ => (g + b) / 2.0,
            };
            if f32::from(px[spill]) > others {
                px[spill] = q(others);
            }
        }
    }
    Image::from_rgba8(out, frame.width(), frame.height())
}

/// Key by luminance: darker than `low` (or brighter than `high`, when
/// `invert`) goes transparent, with `softness` either side.
#[must_use]
pub fn luma_key(frame: &Image, threshold: f32, softness: f32, invert: bool) -> Image {
    let mut out = frame.pixels().to_vec();
    for px in out.chunks_exact_mut(4) {
        let l = 0.2126 * f32::from(px[0]) + 0.7152 * f32::from(px[1]) + 0.0722 * f32::from(px[2]);
        let mut a = smooth(threshold - softness, threshold + softness, l);
        if invert {
            a = 1.0 - a;
        }
        px[3] = q(f32::from(px[3]) * a);
    }
    Image::from_rgba8(out, frame.width(), frame.height())
}

/// Alpha from the difference against a clean plate (what is *new* in the
/// shot survives).
#[must_use]
pub fn difference_key(frame: &Image, plate: &Image, tolerance: f32, softness: f32) -> Image {
    let mut out = frame.pixels().to_vec();
    for (px, pl) in out.chunks_exact_mut(4).zip(plate.pixels().chunks_exact(4)) {
        let d = (0..3).map(|k| (f32::from(px[k]) - f32::from(pl[k])).abs()).fold(0.0, f32::max);
        px[3] = q(f32::from(px[3]) * smooth(tolerance, tolerance + softness, d));
    }
    Image::from_rgba8(out, frame.width(), frame.height())
}

/// After Effects' four track-matte modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatteMode {
    Alpha,
    AlphaInverted,
    Luma,
    LumaInverted,
}

/// Multiply `layer`'s alpha by `matte`'s alpha or luminance.
#[must_use]
pub fn track_matte(layer: &Image, matte: &Image, mode: MatteMode) -> Image {
    let mut out = layer.pixels().to_vec();
    for (px, m) in out.chunks_exact_mut(4).zip(matte.pixels().chunks_exact(4)) {
        let ma = f32::from(m[3]) / 255.0;
        let luma = (0.2126 * f32::from(m[0]) + 0.7152 * f32::from(m[1]) + 0.0722 * f32::from(m[2])) / 255.0 * ma;
        let k = match mode {
            MatteMode::Alpha => ma,
            MatteMode::AlphaInverted => 1.0 - ma,
            MatteMode::Luma => luma,
            MatteMode::LumaInverted => 1.0 - luma,
        };
        px[3] = q(f32::from(px[3]) * k);
    }
    Image::from_rgba8(out, layer.width(), layer.height())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(colors: &[[u8; 4]]) -> Image {
        #[allow(clippy::cast_possible_truncation)]
        Image::from_rgba8(colors.iter().flatten().copied().collect(), colors.len() as u32, 1)
    }

    #[test]
    fn green_screen_keys_including_its_shadow_and_keeps_the_subject() {
        let frame = img(&[[40, 200, 50, 255], [20, 100, 25, 255], [220, 180, 160, 255]]);
        let out = chroma_key(&frame, Color::rgb(40, 200, 50), 20.0, 10.0, true);
        let a: Vec<u8> = out.pixels().chunks(4).map(|p| p[3]).collect();
        assert_eq!(a[0], 0, "the screen");
        assert!(a[1] < 128, "the screen in shadow is still green: {}", a[1]);
        assert_eq!(a[2], 255, "skin survives");
    }

    #[test]
    fn despill_clamps_green_fringes() {
        let frame = img(&[[150, 200, 140, 255]]);
        let out = chroma_key(&frame, Color::rgb(0, 255, 0), 1.0, 1.0, true);
        assert!(out.pixels()[1] <= 145);
    }

    #[test]
    fn luma_and_difference_keys() {
        let frame = img(&[[10, 10, 10, 255], [250, 250, 250, 255]]);
        let out = luma_key(&frame, 128.0, 10.0, false);
        assert_eq!((out.pixels()[3], out.pixels()[7]), (0, 255));
        let plate = img(&[[10, 10, 10, 255], [10, 10, 10, 255]]);
        let d = difference_key(&frame, &plate, 5.0, 5.0);
        assert_eq!((d.pixels()[3], d.pixels()[7]), (0, 255));
    }

    #[test]
    fn track_mattes_in_all_four_modes() {
        let layer = img(&[[255, 0, 0, 255], [255, 0, 0, 255]]);
        let matte = img(&[[255, 255, 255, 255], [0, 0, 0, 0]]);
        let a = |m| track_matte(&layer, &matte, m).pixels().chunks(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(a(MatteMode::Alpha), [255, 0]);
        assert_eq!(a(MatteMode::AlphaInverted), [0, 255]);
        assert_eq!(a(MatteMode::Luma), [255, 0]);
        assert_eq!(a(MatteMode::LumaInverted), [0, 255]);
    }
}
