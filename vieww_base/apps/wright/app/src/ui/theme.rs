//! Colours: VS Code's "Dark Modern" and "Light Modern", by role.
//!
//! Every colour on screen comes from a [`Pal`] read through the theme
//! signal, so switching themes is one `set` and one rebuild. Nothing in the
//! UI names a hex value except this file (and the app-icon tiles, which are
//! the icon's own colours, not the theme's).

use vieww::prelude::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Dark,
    Light,
}

impl Mode {
    pub fn flip(self) -> Mode {
        match self {
            Mode::Dark => Mode::Light,
            Mode::Light => Mode::Dark,
        }
    }
}

/// VS Code's blue: the default accent, and the shipped app icon's colour.
pub const DEFAULT_ACCENT: Color = Color::hex(0x0078D4);

/// Accent swatches offered first in Settings (any colour can be picked).
pub const SWATCHES: [u32; 8] = [0x0078D4, 0x0E7C86, 0x5B5FC7, 0x8B5CF6, 0x2D9D4F, 0xD97706, 0xD6336C, 0x5F6B7A];

/// The saved look: `~/.wright/ui-theme` holds e.g. `dark #0078D4`.
pub fn load() -> (Mode, Color) {
    let text = std::fs::read_to_string(super::store::file("ui-theme")).unwrap_or_default();
    let mut parts = text.split_whitespace();
    let mode = if parts.next() == Some("light") { Mode::Light } else { Mode::Dark };
    let accent = parts.next().and_then(parse_hex).unwrap_or(DEFAULT_ACCENT);
    (mode, accent)
}

pub fn save(mode: Mode, accent: Color) {
    let m = if mode == Mode::Light { "light" } else { "dark" };
    let _ = std::fs::write(super::store::file("ui-theme"), format!("{m} {}\n", hex(accent)));
}

pub fn hex(c: Color) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b)
}

pub fn parse_hex(s: &str) -> Option<Color> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    u32::from_str_radix(h, 16).ok().map(Color::hex)
}

fn luminance(c: Color) -> f32 {
    0.2126 * c.r as f32 + 0.7152 * c.g as f32 + 0.0722 * c.b as f32
}

/// The palette for `mode`, with every accent role derived from `accent`.
///
/// One picked colour has to work as a button fill, as text on the ground,
/// as a soft selection wash and as a chat bubble — in both themes. So the
/// roles are mixed from it rather than all being it: text is pushed toward
/// white on dark (or toward black on light) until it reads, washes are the
/// ground with a little accent in it, and the label on a button is black or
/// white by the accent's own brightness.
pub fn palette(mode: Mode, accent: Color) -> Pal {
    let white = Color::hex(0xFFFFFF);
    let black = Color::hex(0x000000);
    let mut p = match mode {
        Mode::Dark => DARK,
        Mode::Light => LIGHT,
    };
    let lum = luminance(accent);
    p.accent = accent;
    p.on_accent = if lum > 165.0 { Color::hex(0x111111) } else { white };
    match mode {
        Mode::Dark => {
            p.accent_ink = if lum < 150.0 { accent.lerp(white, ((150.0 - lum) / 150.0).clamp(0.2, 0.55)) } else { accent };
            p.accent_soft = p.ground.lerp(accent, 0.26);
            p.bubble = p.ground.lerp(accent, 0.30);
        }
        Mode::Light => {
            p.accent_ink = if lum > 110.0 { accent.lerp(black, ((lum - 110.0) / 145.0).clamp(0.15, 0.5)) } else { accent };
            p.accent_soft = white.lerp(accent, 0.12);
            p.bubble = white.lerp(accent, 0.16);
        }
    }
    p
}

/// One theme, by role.
#[derive(Debug, Clone, Copy)]
pub struct Pal {
    /// The editor background: what everything sits on.
    pub ground: Color,
    /// Cards, menus, the composer.
    pub surface: Color,
    /// Inputs, code, quiet fills inside a card.
    pub surface2: Color,
    pub line: Color,
    pub line_strong: Color,
    pub ink: Color,
    pub muted: Color,
    /// Buttons and the logo.
    pub accent: Color,
    /// Accent used as text (lighter in dark mode, so it stays readable).
    pub accent_ink: Color,
    pub on_accent: Color,
    pub accent_soft: Color,
    pub good: Color,
    pub good_soft: Color,
    pub wait: Color,
    pub wait_soft: Color,
    pub bad: Color,
    /// The status bar along the bottom.
    pub status: Color,
    /// The user's own messages.
    pub bubble: Color,
    /// How visible the background logo is, out of 255.
    pub watermark: u8,
}

pub const DARK: Pal = Pal {
    ground: Color::hex(0x1F1F1F),
    surface: Color::hex(0x252526),
    surface2: Color::hex(0x2D2D2D),
    line: Color::hex(0x2B2B2B),
    line_strong: Color::hex(0x3C3C3C),
    ink: Color::hex(0xCCCCCC),
    muted: Color::hex(0x9D9D9D),
    accent: Color::hex(0x0078D4),
    accent_ink: Color::hex(0x4DAAFC),
    on_accent: Color::hex(0xFFFFFF),
    accent_soft: Color::hex(0x04395E),
    good: Color::hex(0x89D185),
    good_soft: Color::hex(0x1F3320),
    wait: Color::hex(0xCCA700),
    wait_soft: Color::hex(0x3A3217),
    bad: Color::hex(0xF14C4C),
    status: Color::hex(0x181818),
    bubble: Color::hex(0x04395E),
    watermark: 18,
};

pub const LIGHT: Pal = Pal {
    ground: Color::hex(0xF8F8F8),
    surface: Color::hex(0xFFFFFF),
    surface2: Color::hex(0xF3F3F3),
    line: Color::hex(0xE5E5E5),
    line_strong: Color::hex(0xCECECE),
    ink: Color::hex(0x3B3B3B),
    muted: Color::hex(0x6E6E6E),
    accent: Color::hex(0x005FB8),
    accent_ink: Color::hex(0x005FB8),
    on_accent: Color::hex(0xFFFFFF),
    accent_soft: Color::hex(0xE3EEF9),
    good: Color::hex(0x2D7D2A),
    good_soft: Color::hex(0xE5F2E4),
    wait: Color::hex(0x8A6100),
    wait_soft: Color::hex(0xFBF2D8),
    bad: Color::hex(0xC72E2E),
    status: Color::hex(0xF8F8F8),
    bubble: Color::hex(0xDCEBFA),
    watermark: 14,
};
