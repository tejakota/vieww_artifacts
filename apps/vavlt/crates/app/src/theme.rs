//! Adwaita, as tokens a vieww tree can read.
//!
//! This is `ui/theme.slint` and `android/ui/tokens.slint` merged into one Rust
//! definition. They were two files because Slint had no other way to share a
//! palette across two projects; here there is one palette and two *scales* over
//! it, chosen by form factor rather than by which crate you are in.
//!
//! Two things are published above the tree, not one:
//!
//! * [`vieww::ThemeData`] — so vieww's own controls (`Switch`, `TabBar`,
//!   `LinearProgress`, `Button`) come out Adwaita-coloured without a single
//!   call site restyling them. That is the whole reason to map onto the
//!   framework's roles instead of ignoring them.
//! * [`VavltTheme`] — the roles Adwaita has and Material does not: three text
//!   levels rather than two, a tint per semantic colour, a button surface
//!   distinct from a card, and the type/rhythm scale.
//!
//! # The one thing that did not survive the port, and now has
//!
//! The Slint build set every figure and numeric column in `DejaVu Sans Mono`.
//! vieww had no font-family selection at all, so the first pass separated the
//! figures by *weight and tracking* instead of by face — deliberate-looking,
//! and not the same thing. `TextStyle` carries a [`FontFamily`] now and
//! `vieww-text` embeds a real monospace face, so [`Numeric::style`] asks for
//! the typeface it always wanted.

use vieww::foundation::{Color, FontFamily, FontWeight, TextStyle};
use vieww::widget::{BuildContext, ColorScheme, Metrics, ThemeData, Typography};

/// Which set of metrics a surface gets.
///
/// Not "phone versus desktop" as two apps — one tree, and this decides the
/// numbers it lays itself out with. A tablet is a `Phone` that happens to be
/// wide; the breakpoint is the pointer, not the pixel count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormFactor {
    /// Touch. 48dp floors, a scale with air in it, one idea per screen.
    Phone,
    /// A pointer. Denser, smaller text, the same shapes.
    Desktop,
}

impl FormFactor {
    /// The form factor a surface this wide should use.
    ///
    /// 640 rather than a fashionable 768: below it, the plan screen's three
    /// tallies stop fitting on one line, which is the first thing that actually
    /// breaks rather than the first thing that looks tight.
    #[must_use]
    pub fn for_width(width: f32) -> Self {
        if width >= 640.0 {
            Self::Desktop
        } else {
            Self::Phone
        }
    }
}

/// The Adwaita palette, at one brightness.
///
/// Values are GNOME's own named colours, not approximations — `accent` is
/// Adwaita blue 3, `destructive` is red 4. The translucent ones are left
/// translucent rather than pre-composited so a divider over a card and the same
/// divider over the window read the way Adwaita intends, which is differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VavltColors {
    // --- surfaces ---------------------------------------------------------
    pub window: Color,
    pub view: Color,
    pub headerbar: Color,
    pub card: Color,
    pub dialog: Color,

    // --- text -------------------------------------------------------------
    /// Three levels, not two. A heading and its own caption rendering the same
    /// grey is what the desktop build's first pass got wrong.
    pub fg: Color,
    pub fg_2: Color,
    pub dim: Color,

    // --- lines ------------------------------------------------------------
    pub line: Color,
    pub line_strong: Color,
    pub hover: Color,

    // --- button surfaces --------------------------------------------------
    /// Adwaita's flat button chip. Distinct from `card`, or a button reads as a
    /// panel sitting next to the control rather than a control level with it.
    pub btn: Color,
    pub btn_hover: Color,
    pub btn_active: Color,

    // --- semantic ---------------------------------------------------------
    pub accent_bg: Color,
    pub accent_fg: Color,
    pub accent_txt: Color,
    pub accent_tint: Color,

    pub destructive_bg: Color,
    pub destructive_fg: Color,
    pub destructive_txt: Color,
    pub destructive_tint: Color,

    pub warning_txt: Color,
    pub warning_tint: Color,
    pub success_txt: Color,
}

impl VavltColors {
    /// The cinematic dark.
    ///
    /// **Deeper than Adwaita's `#242424`, and that is the one token this app
    /// took from GNOME and then moved.** A billboard has to fade *into* the
    /// page, and a six-stop gradient reaching `#242424` still reads as a grey
    /// band under a photograph — the ramp never disappears, so the band has a
    /// visible bottom edge. `#0E0F11` is dark enough that the fade ends in
    /// nothing. Every other value here is still Adwaita's, including both
    /// accents, so the consent language is unchanged.
    #[must_use]
    pub const fn dark() -> Self {
        Self {
            window: Color::hex(0x0E_0F11),
            view: Color::hex(0x14_1619),
            headerbar: Color::hex(0x1B_1E22),
            card: Color::hex(0x1B_1E22),
            dialog: Color::hex(0x22_262B),

            fg: Color::WHITE,
            fg_2: Color::rgba(255, 255, 255, 199),
            dim: Color::rgba(255, 255, 255, 133),

            line: Color::rgba(255, 255, 255, 23),
            line_strong: Color::rgba(255, 255, 255, 46),
            hover: Color::rgba(255, 255, 255, 18),

            // The frosted surfaces. vieww has no backdrop blur, so "glass" is a
            // flat translucency — over a photograph that still reads as a panel
            // you can see through, which is most of what the effect was for.
            btn: Color::rgba(255, 255, 255, 33),
            btn_hover: Color::rgba(255, 255, 255, 46),
            btn_active: Color::rgba(255, 255, 255, 64),

            accent_bg: Color::hex(0x35_84E4),
            accent_fg: Color::WHITE,
            accent_txt: Color::hex(0x8A_B8F0),
            accent_tint: Color::rgba(53, 132, 228, 46),

            // Adwaita red 4 is `#C01C28`, which is muddy on a near-black
            // ground. Red 3 carries the same meaning and survives the darker
            // page.
            destructive_bg: Color::hex(0xE0_313E),
            destructive_fg: Color::WHITE,
            destructive_txt: Color::hex(0xFF_8272),
            destructive_tint: Color::rgba(224, 49, 62, 41),

            warning_txt: Color::hex(0xF8_E45C),
            warning_tint: Color::rgba(248, 228, 92, 31),
            success_txt: Color::hex(0x8F_F0A4),
        }
    }

    #[must_use]
    pub const fn light() -> Self {
        Self {
            window: Color::hex(0xF6_F6F7),
            view: Color::WHITE,
            headerbar: Color::hex(0xEB_EBEB),
            card: Color::WHITE,
            dialog: Color::WHITE,

            fg: Color::hex(0x1E_1E1E),
            fg_2: Color::rgba(30, 30, 30, 209),
            dim: Color::rgba(30, 30, 30, 153),

            line: Color::rgba(0, 0, 0, 26),
            line_strong: Color::rgba(0, 0, 0, 41),
            hover: Color::rgba(0, 0, 0, 13),

            btn: Color::WHITE,
            btn_hover: Color::hex(0xF0_F0F0),
            btn_active: Color::hex(0xE2_E2E2),

            accent_bg: Color::hex(0x35_84E4),
            accent_fg: Color::WHITE,
            accent_txt: Color::hex(0x1C_71D8),
            accent_tint: Color::rgba(53, 132, 228, 31),

            destructive_bg: Color::hex(0xC0_1C28),
            destructive_fg: Color::WHITE,
            destructive_txt: Color::hex(0xC0_1C28),
            destructive_tint: Color::rgba(192, 28, 40, 26),

            warning_txt: Color::hex(0x9C_6E03),
            warning_tint: Color::rgba(156, 110, 3, 26),
            success_txt: Color::hex(0x16_803C),
        }
    }

    #[must_use]
    pub const fn of(dark: bool) -> Self {
        if dark {
            Self::dark()
        } else {
            Self::light()
        }
    }

    /// The same palette expressed in vieww's roles.
    ///
    /// This is what makes `Switch`, `TabBar` and `LinearProgress` come out
    /// Adwaita-coloured with nothing restyling them at the call site. Where a
    /// role has no Adwaita equivalent the *nearest honest* one is used rather
    /// than an invented colour: `surface_variant` is the button chip, because
    /// that is what a control's own recessed area is here.
    #[must_use]
    pub const fn scheme(self) -> ColorScheme {
        ColorScheme {
            primary: self.accent_bg,
            on_primary: self.accent_fg,
            surface: self.window,
            on_surface: self.fg,
            surface_variant: self.btn,
            on_surface_variant: self.dim,
            outline: self.line_strong,
            error: self.destructive_bg,
            on_error: self.destructive_fg,
            // Adwaita publishes a green for *text* only. Filled success
            // surfaces do not exist in this app, so `on_success` is the window
            // — legible against the only background the green is ever drawn on.
            success: self.success_txt,
            on_success: self.window,
        }
    }
}

/// Sizes, spacing and radii, at one form factor.
///
/// The phone scale is the second attempt at one. The first took the desktop
/// steps and nudged them — 13px body to 15, 30px hero to 44 — which produced a
/// phone screen with desktop *density*. A phone screen carries one idea at the
/// size of the idea, so this scale is more extreme at both ends with more air
/// between them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VavltMetrics {
    // --- touch ------------------------------------------------------------
    /// The floor for anything a finger has to hit. A floor, not a target.
    pub tap: f32,
    /// A primary action. Taller than the floor because missing one costs a step.
    pub tap_lg: f32,

    // --- type -------------------------------------------------------------
    /// The one figure a screen exists for.
    pub t_hero: f32,
    /// Secondary figures — a tally, a count.
    pub t_figure: f32,
    pub t_title: f32,
    pub t_group: f32,
    pub t_body: f32,
    pub t_desc: f32,
    pub t_meta: f32,

    // --- rhythm -----------------------------------------------------------
    /// Screen-edge inset. Everything else is a gap *inside* it.
    pub gutter: f32,
    pub sp_1: f32,
    pub sp_2: f32,
    pub sp_3: f32,
    pub sp_4: f32,

    pub r_card: f32,
    pub r_row: f32,
    pub r_thumb: f32,

    pub topbar: f32,
    pub tabbar: f32,

    /// Photo tile edge, as a floor. The grid computes the real number from the
    /// width it is given; this only decides how many columns fit.
    pub tile: f32,
}

impl VavltMetrics {
    /// Android's numbers over the Adwaita palette.
    #[must_use]
    pub const fn phone() -> Self {
        Self {
            tap: 48.0,
            tap_lg: 58.0,

            t_hero: 56.0,
            t_figure: 30.0,
            t_title: 24.0,
            t_group: 17.0,
            t_body: 16.0,
            t_desc: 14.0,
            t_meta: 12.5,

            // 20, not 16: 16 ran text to within a thumb's width of the screen
            // edge on every screen.
            gutter: 20.0,
            sp_1: 8.0,
            sp_2: 14.0,
            sp_3: 22.0,
            sp_4: 34.0,

            r_card: 18.0,
            r_row: 14.0,
            r_thumb: 10.0,

            topbar: 60.0,
            tabbar: 66.0,
            tile: 96.0,
        }
    }

    /// Adwaita's own desktop rhythm: a 6px unit, a 34px control height, and a
    /// type scale with real steps rather than the 11 / 12.5 / 15 the first
    /// desktop pass used — those were too close to separate a heading from its
    /// own caption, so everything read as one weight.
    #[must_use]
    pub const fn desktop() -> Self {
        Self {
            tap: 34.0,
            tap_lg: 38.0,

            t_hero: 30.0,
            t_figure: 19.0,
            t_title: 19.0,
            t_group: 14.0,
            t_body: 13.0,
            t_desc: 12.0,
            t_meta: 11.0,

            gutter: 18.0,
            sp_1: 6.0,
            sp_2: 12.0,
            sp_3: 18.0,
            sp_4: 24.0,

            r_card: 12.0,
            r_row: 8.0,
            r_thumb: 6.0,

            topbar: 46.0,
            tabbar: 44.0,
            tile: 88.0,
        }
    }

    #[must_use]
    pub const fn of(form: FormFactor) -> Self {
        match form {
            FormFactor::Phone => Self::phone(),
            FormFactor::Desktop => Self::desktop(),
        }
    }

    /// The same numbers as vieww's own metrics, so built-in controls size
    /// themselves to this form factor rather than to Material's defaults.
    #[must_use]
    pub const fn framework(self) -> Metrics {
        Metrics {
            corner: self.r_row,
            gap: self.sp_1,
            touch_target: self.tap,
        }
    }
}

/// Everything above the tree that is not vieww's own.
///
/// Read it with [`VavltTheme::of`]. Published as one value rather than three so
/// a widget takes one lookup and cannot see a palette from one frame beside a
/// scale from another.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VavltTheme {
    pub colors: VavltColors,
    pub metrics: VavltMetrics,
    pub form: FormFactor,
    pub dark: bool,
}

impl VavltTheme {
    #[must_use]
    pub const fn new(dark: bool, form: FormFactor) -> Self {
        Self {
            colors: VavltColors::of(dark),
            metrics: VavltMetrics::of(form),
            form,
            dark,
        }
    }

    /// The nearest enclosing theme, or the dark phone default.
    ///
    /// A default rather than a panic, for the reason `ThemeData::of` has one: a
    /// widget rendered in a tree dump or a unit test with no provider above it
    /// should draw, not abort.
    #[must_use]
    pub fn of(ctx: &BuildContext) -> Self {
        ctx.inherit::<Self>()
            .map(|theme| *theme)
            .unwrap_or_else(|| Self::new(true, FormFactor::Phone))
    }

    /// What vieww's controls should read.
    #[must_use]
    pub fn framework(&self) -> ThemeData {
        ThemeData {
            colors: self.colors.scheme(),
            text: Typography::scale(self.colors.fg),
            metrics: self.metrics.framework(),
        }
    }

    // --- the app's own type styles ---------------------------------------
    // Named for what they are *for*, not for how big they are: a caller asking
    // for `caption()` cannot pick the wrong grey, which is the failure the
    // three-level text hierarchy exists to prevent.

    /// The one figure a screen is about.
    #[must_use]
    pub fn hero(&self) -> TextStyle {
        Numeric::style(self.metrics.t_hero, self.colors.fg)
    }

    /// A secondary figure: a tally, a count, a per-file size.
    #[must_use]
    pub fn figure(&self) -> TextStyle {
        Numeric::style(self.metrics.t_figure, self.colors.fg)
    }

    /// A screen title.
    #[must_use]
    pub fn title(&self) -> TextStyle {
        TextStyle::new(self.metrics.t_title)
            .color(self.colors.fg)
            .weight(FontWeight::Bold)
            .line_height(1.2)
    }

    /// A section heading, sentence case.
    #[must_use]
    pub fn group(&self) -> TextStyle {
        TextStyle::new(self.metrics.t_group)
            .color(self.colors.fg)
            .weight(FontWeight::Bold)
            .line_height(1.25)
    }

    /// A row's primary line, and running text.
    #[must_use]
    pub fn body(&self) -> TextStyle {
        TextStyle::new(self.metrics.t_body)
            .color(self.colors.fg)
            .line_height(1.35)
    }

    /// The caption under a heading. Dim by construction — this is the style
    /// that stops explanatory text competing with what it explains.
    #[must_use]
    pub fn caption(&self) -> TextStyle {
        TextStyle::new(self.metrics.t_desc)
            .color(self.colors.dim)
            .line_height(1.4)
    }

    /// Secondary running text that is still content rather than commentary.
    #[must_use]
    pub fn secondary(&self) -> TextStyle {
        TextStyle::new(self.metrics.t_desc)
            .color(self.colors.fg_2)
            .line_height(1.4)
    }

    /// A label, a badge, a table header.
    #[must_use]
    pub fn meta(&self) -> TextStyle {
        TextStyle::new(self.metrics.t_meta)
            .color(self.colors.dim)
            .weight(FontWeight::Bold)
            .line_height(1.2)
            .letter_spacing(0.2)
    }
}

/// How a number is set, now that there is no monospace face to set it in.
///
/// Fixed-pitch, which is not a stylistic preference here.
///
/// Every figure in this app is a number that *changes while you watch it* — a
/// megabyte count ticking down during a run, a percentage, a photo count. On a
/// proportional face `1` is narrower than `8`, so those numbers jitter
/// horizontally as they update, and a figure that shifts left and right reads
/// as unstable no matter how correct it is.
///
/// The tracking is gone with the face: it was there to stop digits crowding at
/// hero sizes, and a monospace face has that spacing built into its advances.
/// The weight stays, because a run of figures wants a little more colour than
/// the prose beside it.
///
/// **This is the single place every figure in the app is styled through.**
pub struct Numeric;

impl Numeric {
    #[must_use]
    pub const fn style(size: f32, color: Color) -> TextStyle {
        TextStyle::new(size)
            .color(color)
            .family(FontFamily::Monospace)
            .weight(FontWeight::Medium)
            .line_height(1.1)
    }
}

/// Motion, as durations. Adwaita specifies statics only, so this is where the
/// app gets a point of view, and the rule is **mass, not bounce**: the things
/// this app moves are heavy — a library, a decision — and heavy things settle.
pub mod motion {
    use std::time::Duration;

    /// Hover, press.
    pub const FAST: Duration = Duration::from_millis(140);
    /// Selection, reveal.
    pub const BASE: Duration = Duration::from_millis(260);
    /// A change of depth.
    pub const NAV: Duration = Duration::from_millis(320);
    /// One tile completing. Short, because dozens fire in sequence and a long
    /// curve turns a run into a light show.
    pub const TILE: Duration = Duration::from_millis(220);
}
