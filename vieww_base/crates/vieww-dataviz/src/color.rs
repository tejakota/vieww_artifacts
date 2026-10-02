//! Colour schemes and interpolators — `d3-scale-chromatic`,
//! `d3-interpolate`.

use vieww_foundation::Color;

/// Tableau 10, D3's `schemeTableau10`.
pub const TABLEAU10: [Color; 10] = [
    Color::rgb(0x4e, 0x79, 0xa7),
    Color::rgb(0xf2, 0x8e, 0x2c),
    Color::rgb(0xe1, 0x57, 0x59),
    Color::rgb(0x76, 0xb7, 0xb2),
    Color::rgb(0x59, 0xa1, 0x4f),
    Color::rgb(0xed, 0xc9, 0x49),
    Color::rgb(0xaf, 0x7a, 0xa1),
    Color::rgb(0xff, 0x9d, 0xa7),
    Color::rgb(0x9c, 0x75, 0x5f),
    Color::rgb(0xba, 0xb0, 0xab),
];

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn rgb(r: f32, g: f32, b: f32) -> Color {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color::rgb(q(r), q(g), q(b))
}

/// Viridis — perceptually uniform, colour-blind safe (a polynomial fit to
/// the matplotlib table: within a few units per channel of the table).
#[must_use]
pub fn viridis(t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let c0 = [0.277_727_3, 0.005_407_344, 0.334_099_8];
    let c1 = [0.105_093_04, 1.404_613_5, 1.384_590_1];
    let c2 = [-0.330_861_8, 0.214_847_56, 0.095_095_16];
    let c3 = [-4.634_230_5, -5.799_101, -19.332_441];
    let c4 = [6.228_27, 14.179_934, 56.690_55];
    let c5 = [4.776_385, -13.745_146, -65.353_03];
    let c6 = [-5.435_456, 4.645_852_6, 26.312_435];
    let ch = |k: usize| {
        c0[k] + t * (c1[k] + t * (c2[k] + t * (c3[k] + t * (c4[k] + t * (c5[k] + t * c6[k])))))
    };
    rgb(ch(0), ch(1), ch(2))
}

/// Turbo — Google's improved rainbow (Mikhailov's polynomial fit).
#[must_use]
pub fn turbo(t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let r = 0.135_721_38
        + t * (4.615_392_6
            + t * (-42.660_32 + t * (132.131_08 + t * (-152.942_4 + t * 59.286_38))));
    let g = 0.091_402_61
        + t * (2.194_188_4
            + t * (4.842_966_6 + t * (-14.185_033 + t * (4.277_298_6 + t * 2.829_566))));
    let b = 0.106_673_3
        + t * (12.641_946 + t * (-60.582_05 + t * (110.362_77 + t * (-89.903_11 + t * 27.348_25))));
    rgb(r, g, b)
}

/// Blue → white → red, for signed data (`interpolateRdBu` reversed).
#[must_use]
pub fn diverging(t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let (a, m, b) = (
        Color::rgb(33, 102, 172),
        Color::rgb(247, 247, 247),
        Color::rgb(178, 24, 43),
    );
    if t < 0.5 {
        a.lerp_oklab(m, t * 2.0)
    } else {
        m.lerp_oklab(b, t * 2.0 - 1.0)
    }
}

/// Interpolate through `stops` evenly (in Oklab), like
/// `d3.interpolateRgbBasis` but perceptual.
#[must_use]
pub fn ramp(stops: &[Color], t: f32) -> Color {
    match stops.len() {
        0 => Color::TRANSPARENT,
        1 => stops[0],
        n => {
            #[allow(clippy::cast_precision_loss)]
            let x = t.clamp(0.0, 1.0) * (n - 1) as f32;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let i = (x.floor() as usize).min(n - 2);
            #[allow(clippy::cast_precision_loss)]
            stops[i].lerp_oklab(stops[i + 1], x - i as f32)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viridis_endpoints_match_the_table() {
        let a = viridis(0.0);
        let b = viridis(1.0);
        assert!(
            (i32::from(a.r) - 68).abs() <= 4
                && (i32::from(a.g) - 1).abs() <= 3
                && (i32::from(a.b) - 84).abs() <= 3,
            "{a:?}"
        );
        assert!(
            (i32::from(b.r) - 253).abs() <= 3
                && (i32::from(b.g) - 231).abs() <= 3
                && (i32::from(b.b) - 37).abs() <= 4,
            "{b:?}"
        );
    }

    #[test]
    fn viridis_lightness_increases() {
        let l: Vec<f32> = (0..=10)
            .map(|i| viridis(i as f32 / 10.0).lightness())
            .collect();
        assert!(l.windows(2).all(|w| w[1] > w[0]), "{l:?}");
    }

    #[test]
    fn diverging_is_white_in_the_middle_and_ramps_hit_their_stops() {
        let m = diverging(0.5);
        assert!(m.r > 240 && m.g > 240 && m.b > 240);
        let stops = [Color::RED, Color::GREEN, Color::BLUE];
        assert_eq!(ramp(&stops, 0.5), Color::GREEN);
        assert_eq!(ramp(&stops, 1.0), Color::BLUE);
        let t = turbo(0.5);
        assert!(t.g > 180, "turbo is green-ish mid-ramp: {t:?}");
    }
}
