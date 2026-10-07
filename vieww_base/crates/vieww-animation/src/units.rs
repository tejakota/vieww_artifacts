//! Units — the `px`/`%`/`em`/`vw`/`deg` conversion GSAP does behind every tween.
//!
//! GSAP's property interpolation (§2.1 L4) animates `"50%"` to `"200px"` by
//! converting both ends into one unit first, against the element they belong
//! to. This module is that conversion, as a value type plus a context: a
//! [`Length`] carries its unit, a [`UnitContext`] knows what a percent, an em
//! and a viewport unit are worth for the element being animated, and
//! [`Length::to_px`] resolves. [`Angle`] does the same for rotations.
//!
//! Parsing (`"12.5vw".parse::<Length>()`) accepts the CSS spellings, so a
//! timeline authored as data can carry units as text.
//!
//! ```
//! use vieww_animation::units::{Length, UnitContext};
//!
//! let ctx = UnitContext { parent: 400.0, font_size: 16.0, root_font_size: 16.0, viewport: (1280.0, 720.0) };
//! let from: Length = "50%".parse().unwrap();
//! let to: Length = "2em".parse().unwrap();
//! assert_eq!(from.to_px(&ctx), 200.0);
//! // Interpolate in pixels, then report in the destination's unit, as GSAP does.
//! let mid = from.lerp_px(to, 0.5, &ctx);
//! assert_eq!(mid, 116.0);
//! assert_eq!(Length::from_px(mid, to.unit, &ctx).value, 7.25);
//! ```

use std::fmt;
use std::str::FromStr;

/// A length unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LengthUnit {
    Px,
    /// Of the parent's size along the animated axis.
    Percent,
    /// Of the element's font size.
    Em,
    /// Of the root font size.
    Rem,
    /// Of the viewport width.
    Vw,
    /// Of the viewport height.
    Vh,
    /// Of the smaller viewport side.
    Vmin,
    /// Of the larger viewport side.
    Vmax,
    /// Points (1/72 in at 96 dpi).
    Pt,
}

impl LengthUnit {
    #[must_use]
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::Px => "px",
            Self::Percent => "%",
            Self::Em => "em",
            Self::Rem => "rem",
            Self::Vw => "vw",
            Self::Vh => "vh",
            Self::Vmin => "vmin",
            Self::Vmax => "vmax",
            Self::Pt => "pt",
        }
    }
}

/// What the relative units are worth for one element.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitContext {
    /// The parent's size along the axis being animated.
    pub parent: f32,
    pub font_size: f32,
    pub root_font_size: f32,
    /// `(width, height)` of the viewport.
    pub viewport: (f32, f32),
}

impl Default for UnitContext {
    fn default() -> Self {
        Self {
            parent: 0.0,
            font_size: 16.0,
            root_font_size: 16.0,
            viewport: (0.0, 0.0),
        }
    }
}

impl UnitContext {
    /// How many pixels one of `unit` is.
    #[must_use]
    pub fn px_per(&self, unit: LengthUnit) -> f32 {
        let (vw, vh) = self.viewport;
        match unit {
            LengthUnit::Px => 1.0,
            LengthUnit::Percent => self.parent / 100.0,
            LengthUnit::Em => self.font_size,
            LengthUnit::Rem => self.root_font_size,
            LengthUnit::Vw => vw / 100.0,
            LengthUnit::Vh => vh / 100.0,
            LengthUnit::Vmin => vw.min(vh) / 100.0,
            LengthUnit::Vmax => vw.max(vh) / 100.0,
            LengthUnit::Pt => 96.0 / 72.0,
        }
    }
}

/// A number with a length unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Length {
    pub value: f32,
    pub unit: LengthUnit,
}

impl Length {
    #[must_use]
    pub const fn new(value: f32, unit: LengthUnit) -> Self {
        Self { value, unit }
    }

    #[must_use]
    pub const fn px(value: f32) -> Self {
        Self::new(value, LengthUnit::Px)
    }

    /// Resolve to pixels.
    #[must_use]
    pub fn to_px(self, ctx: &UnitContext) -> f32 {
        self.value * ctx.px_per(self.unit)
    }

    /// Express `px` pixels in `unit`. A zero-sized unit (a percent of a
    /// zero-sized parent) yields zero rather than infinity.
    #[must_use]
    pub fn from_px(px: f32, unit: LengthUnit, ctx: &UnitContext) -> Self {
        let per = ctx.px_per(unit);
        Self::new(if per == 0.0 { 0.0 } else { px / per }, unit)
    }

    /// Convert to another unit.
    #[must_use]
    pub fn convert(self, unit: LengthUnit, ctx: &UnitContext) -> Self {
        Self::from_px(self.to_px(ctx), unit, ctx)
    }

    /// Interpolate two lengths of any units, in pixels.
    #[must_use]
    pub fn lerp_px(self, other: Self, t: f32, ctx: &UnitContext) -> f32 {
        let a = self.to_px(ctx);
        a + (other.to_px(ctx) - a) * t
    }
}

impl fmt::Display for Length {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.value, self.unit.suffix())
    }
}

/// Why a unit string did not parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitError(pub String);

impl fmt::Display for UnitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not a value with a unit: {}", self.0)
    }
}

impl std::error::Error for UnitError {}

fn split_number(s: &str) -> Result<(f32, &str), UnitError> {
    let s = s.trim();
    let end = s
        .char_indices()
        .find(|(i, c)| {
            !(c.is_ascii_digit()
                || *c == '.'
                || ((*c == '-' || *c == '+') && *i == 0)
                || ((*c == 'e' || *c == 'E')
                    && s[i + 1..].starts_with(|n: char| n.is_ascii_digit() || n == '-')))
        })
        .map_or(s.len(), |(i, _)| i);
    let v: f32 = s[..end].parse().map_err(|_| UnitError(s.to_owned()))?;
    Ok((v, s[end..].trim()))
}

impl FromStr for Length {
    type Err = UnitError;
    fn from_str(s: &str) -> Result<Self, UnitError> {
        let (v, suffix) = split_number(s)?;
        let unit = match suffix.to_ascii_lowercase().as_str() {
            "" | "px" => LengthUnit::Px,
            "%" => LengthUnit::Percent,
            "em" => LengthUnit::Em,
            "rem" => LengthUnit::Rem,
            "vw" => LengthUnit::Vw,
            "vh" => LengthUnit::Vh,
            "vmin" => LengthUnit::Vmin,
            "vmax" => LengthUnit::Vmax,
            "pt" => LengthUnit::Pt,
            _ => return Err(UnitError(s.to_owned())),
        };
        Ok(Self::new(v, unit))
    }
}

/// An angle with its unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Angle {
    Deg(f32),
    Rad(f32),
    Turn(f32),
    Grad(f32),
}

impl Angle {
    #[must_use]
    pub fn radians(self) -> f32 {
        match self {
            Self::Deg(d) => d.to_radians(),
            Self::Rad(r) => r,
            Self::Turn(t) => t * std::f32::consts::TAU,
            Self::Grad(g) => g * std::f32::consts::PI / 200.0,
        }
    }

    #[must_use]
    pub fn degrees(self) -> f32 {
        self.radians().to_degrees()
    }

    /// GSAP's directional rotation: the end angle chosen so the tween turns
    /// the `short` way (`_short`), always clockwise (`_cw`) or counter-clockwise
    /// (`_ccw`). Returns the destination in radians, relative to `from`.
    #[must_use]
    pub fn directional(from: f32, to: f32, dir: Direction) -> f32 {
        let tau = std::f32::consts::TAU;
        let mut d = (to - from).rem_euclid(tau);
        match dir {
            Direction::Short => {
                if d > std::f32::consts::PI {
                    d -= tau;
                }
            }
            Direction::Clockwise => {}
            Direction::CounterClockwise => {
                if d > 0.0 {
                    d -= tau;
                }
            }
        }
        from + d
    }
}

/// Which way a directional rotation turns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Short,
    Clockwise,
    CounterClockwise,
}

impl FromStr for Angle {
    type Err = UnitError;
    fn from_str(s: &str) -> Result<Self, UnitError> {
        let (v, suffix) = split_number(s)?;
        Ok(match suffix.to_ascii_lowercase().as_str() {
            "" | "deg" => Self::Deg(v),
            "rad" => Self::Rad(v),
            "turn" => Self::Turn(v),
            "grad" => Self::Grad(v),
            _ => return Err(UnitError(s.to_owned())),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> UnitContext {
        UnitContext {
            parent: 500.0,
            font_size: 20.0,
            root_font_size: 16.0,
            viewport: (1000.0, 800.0),
        }
    }

    #[test]
    fn every_unit_resolves() {
        let c = ctx();
        let cases = [
            ("10px", 10.0),
            ("10%", 50.0),
            ("2em", 40.0),
            ("2rem", 32.0),
            ("10vw", 100.0),
            ("10vh", 80.0),
            ("10vmin", 80.0),
            ("10vmax", 100.0),
            ("72pt", 96.0),
            ("-3", -3.0),
            ("1.5e1px", 15.0),
        ];
        for (s, px) in cases {
            let l: Length = s.parse().unwrap();
            assert!((l.to_px(&c) - px).abs() < 1e-4, "{s}");
        }
    }

    #[test]
    fn conversion_round_trips() {
        let c = ctx();
        let l = Length::new(37.0, LengthUnit::Vw);
        let back = l.convert(LengthUnit::Em, &c).convert(LengthUnit::Vw, &c);
        assert!((back.value - 37.0).abs() < 1e-4);
    }

    #[test]
    fn zero_parent_percent_is_zero_not_infinite() {
        let c = UnitContext::default();
        assert_eq!(Length::from_px(10.0, LengthUnit::Percent, &c).value, 0.0);
    }

    #[test]
    fn junk_is_refused() {
        assert!("12furlongs".parse::<Length>().is_err());
        assert!("px".parse::<Length>().is_err());
        assert!("3parsec".parse::<Angle>().is_err());
    }

    #[test]
    fn angles_convert() {
        let a: Angle = "0.5turn".parse().unwrap();
        assert!((a.degrees() - 180.0).abs() < 1e-3);
        let g: Angle = "100grad".parse().unwrap();
        assert!((g.degrees() - 90.0).abs() < 1e-3);
        assert!((Angle::Rad(std::f32::consts::PI).degrees() - 180.0).abs() < 1e-3);
    }

    #[test]
    fn directional_rotation_picks_the_requested_way() {
        let d = |deg: f32| deg.to_radians();
        let short = Angle::directional(d(350.0), d(10.0), Direction::Short);
        assert!((short.to_degrees() - 370.0).abs() < 1e-3, "{}", short.to_degrees());
        let cw = Angle::directional(d(10.0), d(350.0), Direction::Clockwise);
        assert!((cw.to_degrees() - 350.0).abs() < 1e-3);
        let ccw = Angle::directional(d(10.0), d(350.0), Direction::CounterClockwise);
        assert!((ccw.to_degrees() + 10.0).abs() < 1e-3);
    }

    #[test]
    fn display_keeps_the_unit() {
        assert_eq!(Length::new(2.5, LengthUnit::Rem).to_string(), "2.5rem");
    }
}
