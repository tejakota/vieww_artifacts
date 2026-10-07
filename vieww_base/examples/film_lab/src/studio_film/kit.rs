//! kit — the icons and the devices, both quoted from the product.
//!
//! Two notes from the review land here. *"The device slides can mock real
//! devices which you can find in the studio code"*, and *"enrich the
//! widgets or icons; check the alignment deeply"*. Both have the same
//! answer: stop inventing, and take the shapes from the application the
//! film is about.
//!
//! * **The icons** are `viewwstudio::ui::icons` — the studio's own set,
//!   authored as SVG path data on a 24×24 grid. The film does not redraw
//!   them; it asks for the same [`IconData`] the activity bar asks for and
//!   fits it to a box. Alignment stops being something to check, because
//!   [`IconData::fitted`] centres uniformly inside the box it is given:
//!   an icon is centred *by construction*, and two icons at the same box
//!   size agree on scale whether or not their ink fills the grid.
//!
//! * **The devices** are `viewwstudio::state::Platform` — the same screen
//!   sizes, pixel ratios and safe-area insets the studio hands a previewed
//!   tree. iOS is 393×852 at dpr 3 with 47 pt of top inset and 34 of
//!   bottom; Android is 412×915 at 2.625 with 30 and 24; Desktop is
//!   1280×800 with none. The film draws the bezel those numbers imply —
//!   the aspect ratio is the device's own, the notch sits where the inset
//!   says it does, and the home indicator is the height the bottom inset
//!   reserves. A phone in this film is 393:852 because the studio says a
//!   phone is 393:852.

use vieww_foundation::{Color, IconData, Offset, Rect, Sketchbook};
use viewwstudio::state::Platform;
use viewwstudio::ui::icons;

use crate::film_lib::clamp01;
use crate::product_film as pf;
use super::{ACCENT, INK, MUTED};

// ── The icons ───────────────────────────────────────────────────────────────

/// The film's icon vocabulary, by name — every one the studio's own.
///
/// Named rather than passed as a function pointer so a scene can carry a
/// `const` table of `(label, Ico)` and the icon is resolved at draw time.
#[derive(Clone, Copy, PartialEq)]
pub enum Ico {
    Folder,
    File,
    Search,
    Code,
    Warning,
    Dashboard,
    Tune,
    Swatch,
    Gear,
    Play,
    Phone,
    Clock,
    Shield,
    Error,
    Moon,
    Panel,
    Preview,
    Split,
    Book,
    Save,
    Branch,
    Export,
    SafeArea,
    Lightbulb,
    Check,
    Plus,
    ChevronDown,
    ChevronRight,
}

impl Ico {
    /// The studio's own [`IconData`] for this name.
    #[must_use]
    pub fn data(self) -> IconData {
        match self {
            Self::Folder => icons::folder(),
            Self::File => icons::file(),
            Self::Search => icons::search(),
            Self::Code => icons::code(),
            Self::Warning => icons::warning(),
            Self::Dashboard => icons::dashboard(),
            Self::Tune => icons::tune(),
            Self::Swatch => icons::swatch(),
            Self::Gear => icons::gear(),
            Self::Play => icons::play(),
            Self::Phone => icons::phone(),
            Self::Clock => icons::clock(),
            Self::Shield => icons::shield(),
            Self::Error => icons::error(),
            Self::Moon => icons::moon(),
            Self::Panel => icons::panel(),
            Self::Preview => icons::preview(),
            Self::Split => icons::split(),
            Self::Book => icons::book(),
            Self::Save => icons::save(),
            Self::Branch => icons::branch(),
            Self::Export => icons::export(),
            Self::SafeArea => icons::safe_area(),
            Self::Lightbulb => icons::lightbulb(),
            Self::Check => icons::check(),
            Self::Plus => icons::plus(),
            Self::ChevronDown => icons::chevron_down(),
            Self::ChevronRight => icons::chevron_right(),
        }
    }
}

/// Draw an icon centred in `box_`, stroked at `width`.
///
/// This is the only way the film draws an icon. `IconData::fitted` scales
/// the 24×24 grid uniformly into the box and centres what it finds there,
/// so alignment is a property of the call rather than of the caller's
/// arithmetic — which is exactly the class of bug the review asked to be
/// gone through deeply.
pub fn icon(book: &mut Sketchbook, ico: Ico, box_: Rect, color: Color, width: f32, a: f32) {
    if a <= 0.01 {
        return;
    }
    book.stroke(ico.data().fitted(box_), pf::alpha(color, a), width);
}

/// An icon centred on a point, at `side` square — the common case, so the
/// caller never builds the box itself and never gets it off by a half.
pub fn icon_at(book: &mut Sketchbook, ico: Ico, at: Offset, side: f32, color: Color, width: f32, a: f32) {
    let h = side * 0.5;
    icon(
        book,
        ico,
        Rect::new(at.dx - h, at.dy - h, at.dx + h, at.dy + h),
        color,
        width,
        a,
    );
}

/// An icon in a rounded tile — the activity bar's unit, and the film's.
/// The tile is square, the icon is 55% of it, and both are centred on the
/// same point, so a row of these is aligned by construction.
#[allow(clippy::too_many_arguments)]
pub fn icon_tile(
    book: &mut Sketchbook,
    ico: Ico,
    at: Offset,
    side: f32,
    color: Color,
    selected: bool,
    a: f32,
) {
    if a <= 0.01 {
        return;
    }
    let h = side * 0.5;
    let tile = Rect::new(at.dx - h, at.dy - h, at.dx + h, at.dy + h);
    if selected {
        book.rrect(tile, side * 0.28, pf::alpha(color, 0.18 * a));
        book.stroke_rrect(tile, side * 0.28, pf::alpha(color, 0.42 * a), 1.2);
    } else {
        book.rrect(tile, side * 0.28, pf::alpha(Color::WHITE, 0.035 * a));
    }
    icon_at(
        book,
        ico,
        at,
        side * 0.55,
        if selected { color } else { MUTED },
        1.7,
        a,
    );
}

// ── The devices ─────────────────────────────────────────────────────────────

/// A device to draw, quoted from the studio's `Platform`.
#[derive(Clone, Copy, PartialEq)]
pub struct Device {
    pub platform: Platform,
}

impl Device {
    pub const IOS: Device = Device { platform: Platform::Ios };
    pub const ANDROID: Device = Device { platform: Platform::Android };
    pub const DESKTOP: Device = Device { platform: Platform::Desktop };

    /// The screen's logical size — the studio's own numbers.
    #[must_use]
    pub fn screen(self) -> (f32, f32) {
        self.platform.screen()
    }

    /// The safe-area insets, `(top, bottom)` — the studio's own numbers.
    #[must_use]
    pub fn insets(self) -> (f32, f32) {
        self.platform.insets()
    }

    /// `"iOS · 393×852"` — the studio's own status-bar line.
    #[must_use]
    pub fn describe(self) -> String {
        self.platform.describe()
    }

    /// The bezel this device's screen sits in, for a screen `h` tall.
    /// Phones get a thick bezel and big corners; a desktop window gets a
    /// title bar and small ones.
    #[must_use]
    pub fn frame_for_height(self, h: f32) -> (f32, f32, f32, f32) {
        let (sw, sh) = self.screen();
        let scale = h / sh;
        let w = sw * scale;
        match self.platform {
            Platform::Desktop => (w, h, 10.0, 6.0),
            Platform::Ios => (w, h, sw * scale * 0.115, 12.0 * scale.max(0.35)),
            Platform::Android => (w, h, sw * scale * 0.085, 11.0 * scale.max(0.35)),
        }
    }
}

/// What the device's screen is showing.
#[derive(Clone, Copy, PartialEq)]
pub enum Screen {
    /// The studio's own empty state — nothing rendered yet.
    Empty,
    /// A list app: an app bar and `rows` rows. `lit` marks the row that
    /// just repainted.
    List { rows: usize, title_lit: bool, lit_row: Option<usize> },
    /// The counter: a number and a button.
    Counter { count: u32, pressed: f32 },
}

/// Draw a device: bezel, safe areas, and the screen inside it.
///
/// `at` is the **top-left of the bezel**; `h` is the bezel's height, and
/// the width follows from the platform's own aspect ratio — a caller
/// cannot draw a phone at the wrong shape. `p` lands it, `a` fades it.
#[allow(clippy::too_many_arguments)]
pub fn device(
    book: &mut Sketchbook,
    dev: Device,
    at: Offset,
    h: f32,
    screen: Screen,
    accent: Color,
    damage: Option<(Rect, f32)>,
    p: f32,
    a: f32,
) -> Rect {
    let p = clamp01(p);
    let (w, hh, radius, bezel) = dev.frame_for_height(h);
    let body = Rect::new(at.dx, at.dy, at.dx + w, at.dy + hh);
    if p <= 0.01 || a <= 0.01 {
        return body;
    }
    let a = a * p;

    book.shadow(
        Rect::new(body.left, body.top + 10.0, body.right, body.bottom + 10.0),
        radius,
        vieww_foundation::Shadow::new(pf::alpha(Color::BLACK, 0.55 * a), Offset::new(0.0, 16.0), 38.0),
    );
    // The bezel — a dark shell with a bright hairline, the way a device
    // catches a room light along its edge.
    book.rrect(body, radius, pf::alpha(Color::rgb(0x16, 0x14, 0x1A), 0.995 * a));
    book.stroke_rrect(body, radius, pf::alpha(Color::WHITE, 0.16 * a), 1.3);
    book.stroke_rrect(
        Rect::new(body.left + 1.6, body.top + 1.6, body.right - 1.6, body.bottom - 1.6),
        radius - 1.6,
        pf::alpha(Color::BLACK, 0.5 * a),
        1.2,
    );

    let (sw, sh) = dev.screen();
    let scale = hh / sh;
    let screen_rect = match dev.platform {
        Platform::Desktop => Rect::new(body.left + bezel, body.top + 26.0, body.right - bezel, body.bottom - bezel),
        _ => Rect::new(body.left + bezel, body.top + bezel, body.right - bezel, body.bottom - bezel),
    };
    book.rrect(screen_rect, (radius - bezel).max(2.0), pf::alpha(Color::rgb(0x0B, 0x0A, 0x0F), a));

    // The chrome each platform actually has.
    let (top_inset, bottom_inset) = dev.insets();
    match dev.platform {
        Platform::Ios => {
            // The pill sits inside the top inset, centred — its height is
            // the inset's own proportion, not a number picked to look right.
            let ph = (top_inset * scale * 0.52).clamp(6.0, 26.0);
            let pw = ph * 3.1;
            let cx = screen_rect.left + screen_rect.width() * 0.5;
            book.rrect(
                pf::xywh(cx - pw * 0.5, screen_rect.top + ph * 0.55, pw, ph),
                ph * 0.5,
                pf::alpha(Color::BLACK, 0.92 * a),
            );
            // The home indicator, in the bottom inset.
            let iw = screen_rect.width() * 0.34;
            let ih = (bottom_inset * scale * 0.14).clamp(2.0, 5.0);
            book.rrect(
                pf::xywh(cx - iw * 0.5, screen_rect.bottom - bottom_inset * scale * 0.45, iw, ih),
                ih * 0.5,
                pf::alpha(Color::WHITE, 0.5 * a),
            );
        }
        Platform::Android => {
            // A punch-hole camera, top-centre, inside the status inset.
            let r = (top_inset * scale * 0.22).clamp(3.0, 9.0);
            book.circle(
                Offset::new(screen_rect.left + screen_rect.width() * 0.5, screen_rect.top + top_inset * scale * 0.46),
                r,
                pf::alpha(Color::BLACK, 0.92 * a),
            );
            // The gesture bar.
            let iw = screen_rect.width() * 0.30;
            book.rrect(
                pf::xywh(
                    screen_rect.left + (screen_rect.width() - iw) * 0.5,
                    screen_rect.bottom - bottom_inset * scale * 0.5,
                    iw,
                    3.0,
                ),
                1.5,
                pf::alpha(Color::WHITE, 0.45 * a),
            );
        }
        Platform::Desktop => {
            book.rrect(
                pf::xywh(body.left, body.top, body.width(), 26.0),
                radius,
                pf::alpha(Color::rgb(0x21, 0x1D, 0x27), a),
            );
            for d in 0..3 {
                book.circle(
                    Offset::new(body.left + 17.0 + d as f32 * 14.0, body.top + 13.0),
                    3.8,
                    pf::alpha(pf::FAINT, 0.85 * a),
                );
            }
        }
    }

    // The screen's content, laid inside the safe area the platform reports.
    let safe = Rect::new(
        screen_rect.left,
        screen_rect.top + top_inset * scale,
        screen_rect.right,
        screen_rect.bottom - bottom_inset * scale,
    );
    draw_screen(book, screen, safe, accent, scale, a);

    if let Some((d, da)) = damage {
        if da > 0.01 {
            book.stroke_rrect(d, 7.0, pf::alpha(pf::BREAK_RED, 0.92 * da * a), 1.8);
            book.rrect(d, 7.0, pf::alpha(pf::BREAK_RED, 0.12 * da * a));
        }
    }
    body
}

/// The previewed program, drawn inside the safe area.
fn draw_screen(book: &mut Sketchbook, screen: Screen, safe: Rect, accent: Color, scale: f32, a: f32) {
    let u = scale.max(0.25);
    match screen {
        Screen::Empty => {
            let c = Offset::new(safe.left + safe.width() * 0.5, safe.top + safe.height() * 0.42);
            book.ring(c, 22.0 * u, 1.5, pf::alpha(MUTED, 0.30 * a));
            icon_at(book, Ico::Play, c, 22.0 * u, MUTED, 1.6, 0.5 * a);
            book.rrect(
                pf::xywh(c.dx - 58.0 * u, c.dy + 44.0 * u, 116.0 * u, 7.0 * u),
                3.5 * u,
                pf::alpha(MUTED, 0.22 * a),
            );
        }
        Screen::List { rows, title_lit, lit_row } => {
            // The app bar, in the app's token colour, with a real icon in
            // it — the bar is a widget, not a coloured rectangle.
            let bar_h = 56.0 * u;
            book.rect(pf::xywh(safe.left, safe.top, safe.width(), bar_h), pf::alpha(accent, 0.92 * a));
            icon_at(
                book,
                Ico::Panel,
                Offset::new(safe.left + 22.0 * u, safe.top + bar_h * 0.5),
                20.0 * u,
                Color::WHITE,
                1.8,
                0.9 * a,
            );
            book.rrect(
                pf::xywh(safe.left + 42.0 * u, safe.top + bar_h * 0.5 - 5.0 * u,
                         if title_lit { 118.0 * u } else { 76.0 * u }, 10.0 * u),
                5.0 * u,
                pf::alpha(Color::WHITE, (if title_lit { 0.96 } else { 0.66 }) * a),
            );
            icon_at(
                book,
                Ico::Search,
                Offset::new(safe.right - 24.0 * u, safe.top + bar_h * 0.5),
                19.0 * u,
                Color::WHITE,
                1.8,
                0.85 * a,
            );
            // The rows — an icon, two lines of text, a chevron. A list row
            // as a list row, not as a stack of bars.
            let row_h = 44.0 * u;
            for i in 0..rows {
                let y = safe.top + bar_h + 12.0 * u + i as f32 * (row_h + 8.0 * u);
                if y + row_h > safe.bottom {
                    break;
                }
                let lit = lit_row == Some(i);
                let r = pf::xywh(safe.left + 12.0 * u, y, safe.width() - 24.0 * u, row_h);
                book.rrect(r, 10.0 * u, pf::alpha(Color::rgb(0x1B, 0x18, 0x21), 0.96 * a));
                if lit {
                    book.stroke_rrect(r, 10.0 * u, pf::alpha(accent, 0.55 * a), 1.2);
                }
                icon_at(
                    book,
                    [Ico::File, Ico::Book, Ico::Clock, Ico::Shield, Ico::Branch][i % 5],
                    Offset::new(r.left + 22.0 * u, y + row_h * 0.5),
                    18.0 * u,
                    accent,
                    1.7,
                    0.9 * a,
                );
                book.rrect(
                    pf::xywh(r.left + 40.0 * u, y + row_h * 0.30, if lit { r.width() * 0.62 } else { r.width() * 0.44 }, 8.0 * u),
                    4.0 * u,
                    pf::alpha(if lit { INK } else { MUTED }, (if lit { 0.95 } else { 0.55 }) * a),
                );
                book.rrect(
                    pf::xywh(r.left + 40.0 * u, y + row_h * 0.60, r.width() * 0.30, 6.0 * u),
                    3.0 * u,
                    pf::alpha(MUTED, 0.32 * a),
                );
                icon_at(
                    book,
                    Ico::ChevronRight,
                    Offset::new(r.right - 18.0 * u, y + row_h * 0.5),
                    14.0 * u,
                    MUTED,
                    1.5,
                    0.6 * a,
                );
            }
        }
        Screen::Counter { count, pressed } => {
            let cx = safe.left + safe.width() * 0.5;
            let plate = pf::xywh(cx - 96.0 * u, safe.top + safe.height() * 0.16, 192.0 * u, safe.height() * 0.26);
            book.rrect(plate, 16.0 * u, pf::alpha(Color::rgb(0x1A, 0x16, 0x21), 0.96 * a));
            book.stroke_rrect(plate, 16.0 * u, pf::alpha(Color::WHITE, 0.06 * a), 1.0);
            let bw = (safe.width() * 0.62).min(210.0 * u);
            let bh = (safe.height() * 0.11).min(56.0 * u);
            let by = safe.top + safe.height() * 0.58;
            let press = 1.0 - 0.045 * pressed;
            let b = pf::xywh(cx - bw * 0.5 * press, by, bw * press, bh);
            book.rrect(b, bh * 0.30, pf::alpha(accent, (0.85 + 0.15 * pressed) * a));
            icon_at(book, Ico::Plus, Offset::new(b.left + 26.0 * u, by + bh * 0.5), 17.0 * u, Color::WHITE, 2.0, 0.95 * a);
            book.rrect(
                pf::xywh(b.left + 42.0 * u, by + bh * 0.5 - 5.0 * u, bw * 0.42, 10.0 * u),
                5.0 * u,
                pf::alpha(Color::WHITE, 0.92 * a),
            );
            if pressed > 0.02 {
                book.ring(
                    Offset::new(cx, by + bh * 0.5),
                    bw * 0.5 + 60.0 * (1.0 - pressed),
                    1.8,
                    pf::alpha(accent, 0.55 * pressed * a),
                );
            }
            let _ = count;
        }
    }
}

/// Silence the lints for the entries the scenes reach for by name.
#[allow(unused)]
fn _reserved() {
    let _ = (ACCENT, icon_tile, Ico::Moon, Ico::Split, Ico::Save, Ico::Export);
}
