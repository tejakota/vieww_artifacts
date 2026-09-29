//! shell — the film's own drawing of `viewwstudio`, for the scenes that
//! **explain** it.
//!
//! The brief's rule, in the reviewer's words: *the first frame shows the
//! exact studio; from the next frame, where you explain the features, you
//! can animate.* The v2 cut kept the real app mounted for the whole act
//! and drew annotations over it, which meant every explanation competed
//! with a live code buffer for the same pixels — and lost.
//!
//! So the act splits. **Z07 is the product**: the real `viewwstudio`, on
//! the film's own driver, unmatted and un-annotated on its first frames.
//! **Z08–Z10 are the explanation**: this module's shell, which is a
//! drawing — every pane addressable, every line liftable, the whole thing
//! free to explode, re-frame, recolour and re-assemble in a way a running
//! application cannot be asked to do on cue.
//!
//! The drawing is not a different product. Its proportions, its palette,
//! its gutter, its device pane and its file tree are the real app's,
//! taken from the same theme constants Z07 renders with, so the cut from
//! the photograph to the diagram is a change of register and not a change
//! of subject.
//!
//! Everything here lives inside the body band ([`super::HEADER_H`] to
//! [`super::FOOTER_Y`]). Nothing in this module may draw above or below
//! it — the matte would cut it off anyway, but the geometry is written so
//! it never has to.

use vieww_foundation::{Color, Offset, Rect, Sketchbook};

use crate::film_lib::{clamp01, ease_out_cubic, ease_out_expo};
use crate::product_film as pf;
use super::{ACCENT, ACCENT_DEEP, BRAND_NEAR, INK, MUTED, SYN_COMMENT, SYN_KEYWORD, SYN_NUMBER,
    SYN_STRING, SYN_TYPE};

// ── The shell's geometry ────────────────────────────────────────────────────

/// The shell's frame, inside the body band with a margin on every side.
pub const SHELL: Rect = Rect { left: 118.0, top: 326.0, right: 1802.0, bottom: 874.0 };

/// The file tree's right edge.
pub const SIDEBAR_R: f32 = 372.0;

/// The editor's right edge — where the preview pane begins.
pub const EDITOR_R: f32 = 1332.0;

/// The preview device's rect inside the preview pane.
pub const DEVICE: Rect = Rect { left: 1440.0, top: 366.0, right: 1700.0, bottom: 834.0 };

/// The editor's first code line's baseline, and the line pitch.
pub const CODE_Y0: f32 = 392.0;
pub const CODE_DY: f32 = 30.0;

/// The gutter's x — where line numbers sit.
pub const GUTTER_X: f32 = 396.0;

/// The code's left margin.
pub const CODE_X: f32 = 452.0;

// ── The parts ───────────────────────────────────────────────────────────────

/// The shell's chrome: body, the three panes' grounds, the pane seams and
/// the title strip. `p` assembles it — the panes slide in from their own
/// edges, so the shell builds itself rather than fading up.
pub fn frame(book: &mut Sketchbook, p: f32, a: f32) {
    let p = ease_out_expo(clamp01(p));
    if p <= 0.01 || a <= 0.01 {
        return;
    }
    let r = SHELL;
    book.shadow(
        Rect::new(r.left, r.top + 14.0, r.right, r.bottom + 14.0),
        20.0,
        vieww_foundation::Shadow::new(pf::alpha(Color::BLACK, 0.6 * a), Offset::new(0.0, 18.0), 48.0),
    );
    book.rrect(r, 18.0, pf::alpha(Color::rgb(0x11, 0x0F, 0x14), 0.985 * a));
    book.stroke_rrect(r, 18.0, pf::alpha(Color::WHITE, 0.075 * a), 1.1);

    // The sidebar, arriving from the left.
    let sx = r.left + (SIDEBAR_R - r.left) * p;
    book.rrect(
        Rect::new(r.left, r.top, sx, r.bottom),
        18.0,
        pf::alpha(Color::rgb(0x0C, 0x0B, 0x0F), 0.96 * a),
    );
    // The preview pane, arriving from the right.
    let px = r.right - (r.right - EDITOR_R) * p;
    book.rrect(
        Rect::new(px, r.top, r.right, r.bottom),
        18.0,
        pf::alpha(Color::rgb(0x0C, 0x0B, 0x0F), 0.96 * a),
    );
    // The seams.
    for x in [sx, px] {
        book.rect(Rect::new(x - 0.5, r.top, x + 0.5, r.bottom), pf::alpha(Color::WHITE, 0.06 * a));
    }
    // The title strip — three dots and a breadcrumb rule, the app's own.
    book.rect(
        Rect::new(r.left, r.top, r.right, r.top + 36.0),
        pf::alpha(Color::rgb(0x16, 0x13, 0x1A), 0.9 * a * p),
    );
    for d in 0..3 {
        book.circle(
            Offset::new(r.left + 26.0 + d as f32 * 17.0, r.top + 18.0),
            4.2,
            pf::alpha(pf::FAINT, 0.75 * a * p),
        );
    }
    book.rect(
        Rect::new(r.left + 104.0, r.top + 16.0, r.left + 104.0 + 250.0 * p, r.top + 20.0),
        pf::alpha(pf::FAINT, 0.30 * a),
    );
}

/// The file tree — rows that land one after another, with the open file
/// held in the accent. `open` is the row index the session has open.
pub fn tree(book: &mut Sketchbook, p: f32, open: usize, a: f32) {
    if p <= 0.01 || a <= 0.01 {
        return;
    }
    let widths = [128.0f32, 96.0, 150.0, 112.0, 138.0, 104.0, 126.0, 92.0, 144.0];
    for (i, w) in widths.iter().enumerate() {
        let rp = ease_out_cubic(clamp01(p * 1.5 - i as f32 * 0.07));
        if rp <= 0.01 {
            continue;
        }
        let y = SHELL.top + 66.0 + i as f32 * 34.0;
        let indent = if i % 3 == 0 { 0.0 } else { 18.0 };
        if i == open {
            book.rrect(
                pf::xywh(SHELL.left + 12.0, y - 7.0, SIDEBAR_R - SHELL.left - 24.0, 26.0),
                7.0,
                pf::alpha(ACCENT_DEEP, 0.34 * a * rp),
            );
        }
        book.rrect(
            pf::xywh(SHELL.left + 30.0 + indent, y, w * rp, 9.0),
            4.5,
            pf::alpha(if i == open { INK } else { MUTED }, (if i == open { 0.9 } else { 0.42 }) * a),
        );
    }
}

/// One token of a drawn code line — a coloured bar of a given width.
pub struct Tok(pub f32, pub Color);

/// A drawn code line: indent in characters, then its tokens. Widths are in
/// pixels at the shell's own 15 px mono, so the lines read as code at a
/// glance and never as a bar chart.
pub struct Line(pub f32, pub &'static [(f32, u8)]);

/// The token palette, indexed by [`Line`]'s `u8` — kept as an index rather
/// than a colour so a line is a `const` and costs nothing per frame.
pub fn tok_color(k: u8) -> Color {
    match k {
        0 => SYN_KEYWORD,
        1 => SYN_TYPE,
        2 => SYN_STRING,
        3 => SYN_NUMBER,
        4 => SYN_COMMENT,
        5 => ACCENT,
        _ => MUTED,
    }
}

/// The editor's code — `n_typed` lines already there, the line at
/// `typing` drawn to `frac` of its width with a caret at the end, and
/// `highlight` (if any) carrying the accent wash of the line being edited.
#[allow(clippy::too_many_arguments)]
pub fn code(
    book: &mut Sketchbook,
    lines: &[Line],
    n_typed: usize,
    typing: Option<(usize, f32)>,
    highlight: Option<(usize, f32)>,
    caret_on: bool,
    a: f32,
) {
    if a <= 0.01 {
        return;
    }
    for (i, Line(indent, toks)) in lines.iter().enumerate() {
        let full = i < n_typed;
        let (frac, is_typing) = match typing {
            Some((ti, f)) if ti == i => (f, true),
            _ => (if full { 1.0 } else { 0.0 }, false),
        };
        if frac <= 0.001 {
            continue;
        }
        let y = CODE_Y0 + i as f32 * CODE_DY;
        // The gutter's line number — a short bar, the app's own dim.
        book.rect(pf::xywh(GUTTER_X, y + 4.0, 18.0, 2.0), pf::alpha(pf::FAINT, 0.35 * a));
        if let Some((hi, ha)) = highlight {
            if hi == i && ha > 0.01 {
                book.rrect(
                    pf::xywh(CODE_X - 16.0, y - 7.0, EDITOR_R - CODE_X - 40.0, 24.0),
                    6.0,
                    pf::alpha(ACCENT_DEEP, 0.28 * ha * a),
                );
            }
        }
        // The line's tokens, laid left to right, revealed to `frac`.
        let total: f32 = toks.iter().map(|(w, _)| w + 9.0).sum();
        let shown = total * frac;
        let mut x = CODE_X + indent * 13.0;
        let mut used = 0.0f32;
        for (w, k) in toks.iter() {
            if used >= shown {
                break;
            }
            let draw_w = (shown - used).min(*w);
            if draw_w > 0.5 {
                book.rrect(pf::xywh(x, y, draw_w, 10.0), 4.0, pf::alpha(tok_color(*k), 0.82 * a));
            }
            x += w + 9.0;
            used += w + 9.0;
        }
        if is_typing && caret_on && frac < 1.0 {
            book.rect(pf::xywh(x.min(EDITOR_R - 60.0), y - 5.0, 2.4, 20.0), pf::alpha(ACCENT, 0.95 * a));
        }
    }
}

/// What the preview device is showing.
#[derive(Clone, Copy, PartialEq)]
pub enum Preview {
    /// Nothing rendered yet — the app's own empty state.
    Empty,
    /// A list app: a title bar and rows. `title_lit` tints the title.
    List { rows: usize, title_lit: bool, row_lit: bool },
    /// The counter: a number and a button. `count` is what it reads.
    Counter { count: u32, pressed: f32 },
}

/// The preview device — a phone, a window, or a tablet, at `r`, with
/// `accent` as the app's own token colour. `p` lands it; `damage` draws
/// the repaint rectangle the planner actually reported.
#[allow(clippy::too_many_arguments)]
pub fn device(
    book: &mut Sketchbook,
    r: Rect,
    kind: Kind,
    state: Preview,
    accent: Color,
    damage: Option<(Rect, f32)>,
    p: f32,
    a: f32,
) {
    let p = ease_out_expo(clamp01(p));
    if p <= 0.01 || a <= 0.01 {
        return;
    }
    let radius = match kind {
        Kind::Phone => 28.0,
        Kind::Window => 12.0,
    };
    book.shadow(
        Rect::new(r.left, r.top + 8.0, r.right, r.bottom + 8.0),
        radius,
        vieww_foundation::Shadow::new(pf::alpha(Color::BLACK, 0.5 * a), Offset::new(0.0, 12.0), 30.0),
    );
    book.rrect(r, radius, pf::alpha(Color::rgb(0x1A, 0x17, 0x1E), 0.99 * a));
    book.stroke_rrect(r, radius, pf::alpha(Color::WHITE, 0.14 * a), 1.2);
    // The screen — deliberately *not* white. The real app previews a
    // light-themed program, which in Z07 is the honest picture and in a
    // dark 1920×1080 frame is also the loudest thing in it. The drawing
    // previews the same program in the film's own register, so the
    // explanation is legible next to what it explains.
    let inset = match kind {
        Kind::Phone => 10.0,
        Kind::Window => 8.0,
    };
    let s = Rect::new(r.left + inset, r.top + inset + if kind == Kind::Phone { 16.0 } else { 26.0 },
                      r.right - inset, r.bottom - inset - if kind == Kind::Phone { 16.0 } else { 8.0 });
    book.rrect(s, radius * 0.6, pf::alpha(Color::rgb(0x0D, 0x0C, 0x11), a));
    match kind {
        Kind::Phone => {
            book.rrect(
                pf::xywh(r.left + r.width() * 0.5 - 28.0, r.top + 14.0, 56.0, 8.0),
                4.0,
                pf::alpha(Color::BLACK, 0.7 * a),
            );
        }
        Kind::Window => {
            book.rect(Rect::new(r.left, r.top, r.right, r.top + 26.0), pf::alpha(Color::rgb(0x22, 0x1E, 0x28), a));
            for d in 0..3 {
                book.circle(Offset::new(r.left + 16.0 + d as f32 * 13.0, r.top + 13.0), 3.4, pf::alpha(pf::FAINT, 0.8 * a));
            }
        }
    }

    match state {
        Preview::Empty => {
            let cx = s.left + s.width() * 0.5;
            let cy = s.top + s.height() * 0.5;
            book.ring(Offset::new(cx, cy), 20.0, 1.4, pf::alpha(MUTED, 0.30 * a));
            book.rrect(pf::xywh(cx - 54.0, cy + 36.0, 108.0, 7.0), 3.5, pf::alpha(MUTED, 0.22 * a));
        }
        Preview::List { rows, title_lit, row_lit } => {
            // The app bar, in the app's token colour.
            book.rrect(
                pf::xywh(s.left, s.top, s.width(), 62.0),
                radius * 0.6,
                pf::alpha(accent, 0.90 * a),
            );
            book.rrect(
                pf::xywh(s.left + 20.0, s.top + 24.0, if title_lit { 132.0 } else { 86.0 }, 11.0),
                5.0,
                pf::alpha(Color::WHITE, (if title_lit { 0.95 } else { 0.65 }) * a),
            );
            for i in 0..rows {
                let y = s.top + 82.0 + i as f32 * 46.0;
                if y + 34.0 > s.bottom {
                    break;
                }
                let lit = row_lit && i == 0;
                book.rrect(pf::xywh(s.left + 14.0, y, s.width() - 28.0, 36.0), 9.0,
                           pf::alpha(Color::rgb(0x1A, 0x17, 0x1F), 0.95 * a));
                book.circle(Offset::new(s.left + 34.0, y + 18.0), 6.0, pf::alpha(accent, 0.8 * a));
                book.rrect(
                    pf::xywh(s.left + 52.0, y + 13.0, if lit { s.width() * 0.62 } else { s.width() * 0.40 }, 9.0),
                    4.5,
                    pf::alpha(if lit { INK } else { MUTED }, (if lit { 0.92 } else { 0.5 }) * a),
                );
            }
        }
        Preview::Counter { count, pressed } => {
            let cx = s.left + s.width() * 0.5;
            // The count — drawn as a numeral by the caller (a widget), so
            // this draws only its plate and the button.
            // Laid out against the screen's own box: the v1 of this
            // used constants and put the button 40 px below the phone.
            let sh_h = s.height();
            let plate_y = s.top + sh_h * 0.16;
            let btn_y = s.top + sh_h * 0.66;
            book.rrect(pf::xywh(cx - 92.0, plate_y, 184.0, sh_h * 0.30), 16.0,
                       pf::alpha(Color::rgb(0x18, 0x15, 0x1D), 0.95 * a));
            let bw = (s.width() * 0.76).min(196.0);
            let bh = (sh_h * 0.16).min(54.0);
            let press = 1.0 - 0.04 * pressed;
            book.rrect(
                pf::xywh(cx - bw * 0.5 * press, btn_y, bw * press, bh),
                14.0,
                pf::alpha(accent, (0.82 + 0.18 * pressed) * a),
            );
            if pressed > 0.02 {
                book.ring(Offset::new(cx, btn_y + bh * 0.5), 58.0 + 72.0 * (1.0 - pressed), 1.6,
                          pf::alpha(accent, 0.5 * pressed * a));
            }
            let _ = count;
        }
    }

    // The damage rectangle — what the planner repainted, and nothing else.
    if let Some((d, da)) = damage {
        if da > 0.01 {
            book.stroke_rrect(d, 7.0, pf::alpha(pf::BREAK_RED, 0.9 * da * a), 1.6);
            book.rrect(d, 7.0, pf::alpha(pf::BREAK_RED, 0.10 * da * a));
        }
    }
}

/// The device shapes the fleet can take.
#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Phone,
    Window,
}

/// A tidy label under a drawn part — the diagram's own voice, painted
/// rather than laid out, so it rides the same transform its subject does.
pub fn tag(book: &mut Sketchbook, at: Offset, w: f32, color: Color, a: f32) {
    if a <= 0.01 {
        return;
    }
    book.rrect(pf::xywh(at.dx, at.dy, w, 3.0), 1.5, pf::alpha(color, 0.75 * a));
}

/// Silence the lints for the palette entries the scenes reach for by name.
#[allow(unused)]
fn _reserved() {
    let _ = (SYN_STRING, BRAND_NEAR, SYN_TYPE, tag);
}
