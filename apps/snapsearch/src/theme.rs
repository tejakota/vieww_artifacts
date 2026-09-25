//! SnapSearch's theme.
//!
//! This is a **direct port of the HTML prototype's `theme.css`**, not
//! `vieww`'s stock palette. An earlier pass used `ThemeData::adaptive(..)`,
//! which is the framework's own dark scheme — a light-blue primary on
//! neutral grey — and the result was a recognisably different app from the
//! prototype. Every colour below is the same value the prototype's
//! `--vw-*` custom properties carried, so the two read as one design.
//!
//! Platform adaptation is kept where it belongs: `Metrics::adaptive` still
//! decides the touch target (48 on Android, 44 on iOS). Only the *palette*
//! is pinned, because the brand is the brand on both platforms.

use vieww::prelude::*;

/// App tokens `ThemeData` has no field for.
#[derive(Debug, Clone, Copy)]
pub struct FluidTokens {
    pub violet: Color,
    pub coral: Color,
    pub amber: Color,
    /// The screen behind the surfaces — the prototype's `--vw-background`.
    /// `ColorScheme` has no background field of its own, so the root
    /// container paints this explicitly.
    pub background: Color,
    pub surface_raised: Color,
    pub scrim: Color,
    /// The tint over an already-blurred backdrop. Much lighter than
    /// [`Self::scrim`], which had to do the whole separation job alone.
    pub scrim_over_blur: Color,
    /// The FAB's corner radius, on a [`Self::fab_size`] box. Radius is
    /// clamped at paint time to half the shorter side, so this sits just
    /// under that half — the roundest a circular-arc corner can be while
    /// still reading as a squircle rather than as a plain circle.
    pub fab_curve: f32,
    pub fab_size: f32,
    /// Large surfaces (photo tiles, the sheet) carry their own radius
    /// rather than deriving it from `metrics.corner`: that token is tuned
    /// so *controls* come out as stadiums, and a 200px-tall card at the
    /// same radius would be a capsule.
    pub tile_curve: f32,
    pub sheet_curve: f32,
}

impl FluidTokens {
    pub const fn new() -> Self {
        Self {
            violet: Color::rgb(0x7c, 0x3a, 0xed),
            coral: Color::rgb(0xff, 0x6b, 0x81),
            amber: Color::rgb(0xff, 0xb5, 0x45),
            background: Color::rgb(0x12, 0x10, 0x20),
            surface_raised: Color::rgb(0x24, 0x1f, 0x33),
            scrim: Color::rgba(0x06, 0x05, 0x0c, 0xad),
            scrim_over_blur: Color::rgba(0x06, 0x05, 0x0c, 0x59),
            fab_curve: 27.0,
            fab_size: 64.0,
            tile_curve: 30.0,
            sheet_curve: 36.0,
        }
    }

    /// The fluid ramp — the prototype's
    /// `linear-gradient(135deg, violet, coral, amber)`.
    pub fn fluid_gradient(&self) -> Gradient {
        Gradient::linear(Offset::new(0.0, 0.0), Offset::new(1.0, 1.0)).with_stops(&[
            (0.0, self.violet),
            (0.55, self.coral),
            (1.0, self.amber),
        ])
    }
}

impl Default for FluidTokens {
    fn default() -> Self {
        Self::new()
    }
}

/// The prototype's dark scheme, field for field.
fn colors() -> ColorScheme {
    let t = FluidTokens::new();
    ColorScheme {
        primary: t.violet,
        on_primary: Color::WHITE,
        surface: Color::rgb(0x1b, 0x18, 0x26),
        on_surface: Color::rgb(0xf2, 0xef, 0xfa),
        surface_variant: Color::rgb(0x2c, 0x27, 0x40),
        on_surface_variant: Color::rgb(0xb9, 0xb2, 0xcc),
        outline: Color::rgb(0x3a, 0x34, 0x50),
        error: Color::rgb(0xef, 0x44, 0x44),
        on_error: Color::WHITE,
        success: Color::rgb(0x35, 0xd6, 0xa9),
        on_success: Color::rgb(0x0b, 0x0b, 0x12),
    }
}

/// How round every stock control comes out.
///
/// `vieww` derives each control's radius from this one token —
/// `Button`, `SegmentedControl`, `Dropdown`, `Menu`, `TextField` all read
/// `theme.metrics.corner` — and the painter clamps a radius to half the
/// shorter side. A button is at least `touch_target` (48px) tall, so
/// anything at or above 24 comes out as a full stadium. 26 puts every
/// control comfortably at that ceiling.
const CONTROL_CURVE: f32 = 26.0;

pub fn build_theme(platform: TargetPlatform) -> ThemeData {
    let colors = colors();
    let mut theme = ThemeData::from_colors(colors);
    theme.metrics = Metrics {
        corner: CONTROL_CURVE,
        ..Metrics::adaptive(platform)
    };
    // `expressive`, not `standard`: this is a consumer photo app, and the
    // whole design language is the fluid, overshooting FAB. The important
    // half is that `expressive` overshoots only the **spatial** springs —
    // its effects springs are deliberately identical to `standard`, because
    // a 0→1 opacity spring that overshoots asks for alpha 1.06, clamps, and
    // stalls at full opacity for ~80ms, which reads as a dropped frame.
    // `Motion::effects_never_overshoot()` is an invariant with a test behind
    // it; the test below is this app asserting it holds for its own theme.
    theme.motion = Motion::expressive();
    theme
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_are_round_enough_to_come_out_as_stadiums() {
        let theme = build_theme(TargetPlatform::Android);
        assert!(
            theme.metrics.corner * 2.0 >= theme.metrics.touch_target,
            "a {}px radius on a {}px-tall control is not yet a full stadium",
            theme.metrics.corner,
            theme.metrics.touch_target
        );
    }

    #[test]
    fn the_palette_is_the_prototypes_violet_not_viewws_stock_blue() {
        let theme = build_theme(TargetPlatform::Android);
        assert_eq!(theme.colors.primary, Color::rgb(0x7c, 0x3a, 0xed));
    }

    #[test]
    fn nothing_that_merely_fades_is_allowed_to_overshoot() {
        let motion = build_theme(TargetPlatform::Android).motion;
        assert!(
            motion.effects_never_overshoot(),
            "an opacity spring that overshoots clamps and stalls at full alpha"
        );
        assert!(
            motion.spatial_default.spec().damping_ratio < 1.0,
            "but spatial motion should still have life in it"
        );
    }

    #[test]
    fn the_platform_still_decides_the_touch_target() {
        assert_eq!(build_theme(TargetPlatform::Android).metrics.touch_target, 48.0);
        assert_eq!(build_theme(TargetPlatform::IOS).metrics.touch_target, 44.0);
    }
}
