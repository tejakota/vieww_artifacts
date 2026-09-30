//! frame — the film's one frame grammar (v5).
//!
//! The v4 review, in the reviewer's words: *the header and footer need not
//! be a dark patch; the theme is the same for the whole screen. Titles in
//! the centre of the header. Padding left, right, top and bottom. Nothing
//! cropped.* Every one of those is a property of the **frame**, not of a
//! scene, so this module owns all of them and no scene can get them wrong:
//!
//! * **One ground.** A scene's room (its gradient, its stars, its
//!   vignette) is registered here with [`ground`] and painted full-bleed in
//!   screen space, *under* everything — header, body and footer are one
//!   surface. There is no matte and no band colour.
//!
//! * **A centred header.** Scenes no longer place their own captions.
//!   [`caption`] and [`headline`] *register* lines; the master lays the
//!   header out once: the movement label, the title, the subtitle — all
//!   centred, all at one size per role.
//!
//! * **A padded body.** Every scene declares the box its content occupies
//!   in its own coordinates ([`content_box`]); the master fits that box
//!   *uniformly* into [`BODY`], which is inset from the header, the footer
//!   and both sides. Content is scaled to the space it is given — up when
//!   the scene is small, never past the padding. There is no camera zoom
//!   left in the film to crop anything: the only transform a scene rides
//!   is the fit, and a fit cannot cut.
//!
//! * **A footer.** The receipts row ([`receipts`]) and the progress rail,
//!   laid out by the master.
//!
//! * **Plates.** The studio act quotes the *real* application. A plate is
//!   a rectangle of the live studio's own draw list, re-composited at a
//!   world rectangle — the same commands, re-rasterised, so text stays
//!   text at any size. The explanation scenes take the product apart with
//!   plates instead of redrawing it.

use std::cell::RefCell;

use vieww_foundation::{Color, Offset, Rect, Size, Sketchbook, TextAlign};
use vieww_widget::prelude::*;
use vieww_widget::WidgetNode;

use crate::film_lib::{clamp01, ease_out_cubic};
use crate::product_film as pf;

// ── The bands ───────────────────────────────────────────────────────────────

/// The header's height: movement label, title, subtitle.
pub const HEADER_H: f32 = 146.0;
/// Where the footer begins: receipts and the rail below.
pub const FOOTER_Y: f32 = 1000.0;
/// The side padding every band respects.
pub const PAD_X: f32 = 128.0;
/// The body's vertical breathing room inside the header/footer seams.
pub const PAD_Y: f32 = 16.0;

/// The body as the scenes were authored against it — the studio scenes
/// place their plates relative to this rectangle and declare it as their
/// content box, so it stays fixed.
pub const BODY: Rect = Rect {
    left: 72.0,
    top: HEADER_H + PAD_Y,
    right: pf::W - 72.0,
    bottom: FOOTER_Y - PAD_Y,
};

/// The safe area every scene is fitted into: the same generous padding on
/// the left and right, and clear air under the header and above the
/// footer — nothing a scene draws reaches past it.
pub const SAFE: Rect = Rect {
    left: PAD_X,
    top: HEADER_H + 36.0,
    right: pf::W - PAD_X,
    bottom: FOOTER_Y - 36.0,
};

// ── Registries ──────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Line {
    text: String,
    appear: f32,
    older: bool,
}

/// A rectangle of the live studio, re-composited into the world.
#[derive(Clone, Debug)]
pub struct Plate {
    /// The region of the application's own 1920×1080 draw list.
    pub src: Rect,
    /// Where it lands in world space (uniform scale: `dst.width / src.width`).
    pub dst: Rect,
    /// Group opacity.
    pub alpha: f32,
    /// Corner radius of the plate's card (and its backdrop).
    pub radius: f32,
    /// A named snapshot instead of the live frame (see `master::snapshots`).
    pub snap: Option<&'static str>,
    /// Lift the plate off the ground: a card, a shadow and a hairline.
    pub card: bool,
}

#[derive(Default)]
struct Registry {
    lines: Vec<Line>,
    headline: Option<(String, f32)>,
    receipts: Vec<(Vec<(String, Color)>, f32)>,
    ground: Vec<WidgetNode>,
    plates: Vec<Plate>,
    over: Vec<WidgetNode>,
    under: Vec<WidgetNode>,
    boxed: Option<Rect>,
}

thread_local! {
    static REG: RefCell<Registry> = RefCell::new(Registry::default());
}

/// Clear the frame's registrations — the master calls this before a build.
pub fn reset() {
    REG.with(|r| *r.borrow_mut() = Registry::default());
}

/// A caption line. The `legacy_y` is the scenes' old slot literal: `>= 990`
/// is the scene's first statement (its **title**), anything else its second
/// (its **subtitle**). `appear` fades it in.
pub fn caption(text: &str, legacy_y: f32, appear: f32) -> WidgetNode {
    let older = legacy_y >= 990.0;
    let text = text.to_string();
    REG.with(|r| r.borrow_mut().lines.push(Line { text, appear, older }));
    Stack::new().into()
}

/// The scene's headline — when a scene has one, it is the title, and both
/// captions become the subtitle (the newer replacing the older).
pub fn headline(text: &str, appear: f32) {
    let text = text.to_string();
    REG.with(|r| r.borrow_mut().headline = Some((text, appear)));
}

/// The receipts row — chips in the footer, centred.
pub fn receipts(chips: &[(&str, Color)], _x: f32, _y: f32, appear: f32) -> WidgetNode {
    let chips: Vec<(String, Color)> = chips.iter().map(|(t, c)| ((*t).to_string(), *c)).collect();
    REG.with(|r| r.borrow_mut().receipts.push((chips, appear)));
    Stack::new().into()
}

/// Paint a scene's room full-bleed in screen space — the one surface the
/// header, the body and the footer share.
pub fn ground(node: impl Into<WidgetNode>) -> WidgetNode {
    let node = node.into();
    REG.with(|r| r.borrow_mut().ground.push(node));
    Stack::new().into()
}

/// A world-space node drawn over the plates (the studio act's overlay).
pub fn over(node: impl Into<WidgetNode>) -> WidgetNode {
    let node = node.into();
    REG.with(|r| r.borrow_mut().over.push(node));
    Stack::new().into()
}

/// A world-space node drawn *under* the plates (a card a plate sits on).
pub fn under(node: impl Into<WidgetNode>) -> WidgetNode {
    let node = node.into();
    REG.with(|r| r.borrow_mut().under.push(node));
    Stack::new().into()
}

pub fn take_under() -> Vec<WidgetNode> {
    REG.with(|r| std::mem::take(&mut r.borrow_mut().under))
}

/// Composite a region of the live studio at a world rectangle.
pub fn plate(p: Plate) {
    REG.with(|r| r.borrow_mut().plates.push(p));
}

/// Override the scene's content box for this frame (normally the table in
/// [`content_box`] decides).
pub fn boxed(r: Rect) {
    REG.with(|reg| reg.borrow_mut().boxed = Some(r));
}

pub fn take_ground() -> Vec<WidgetNode> {
    REG.with(|r| std::mem::take(&mut r.borrow_mut().ground))
}
pub fn take_plates() -> Vec<Plate> {
    REG.with(|r| std::mem::take(&mut r.borrow_mut().plates))
}
pub fn take_over() -> Vec<WidgetNode> {
    REG.with(|r| std::mem::take(&mut r.borrow_mut().over))
}
pub fn take_boxed() -> Option<Rect> {
    REG.with(|r| r.borrow_mut().boxed.take())
}

// ── The fit ─────────────────────────────────────────────────────────────────

/// The box each scene's content occupies in its own coordinates. Measured
/// from the scene (`sfmeasure`), written down here, and fitted into
/// [`BODY`] by [`fit`].
pub fn content_box(id: &str) -> Rect {
    super::layout::content_box(id)
}

/// The uniform transform that puts `content` in the middle of [`BODY`] as
/// large as the body allows, capped at `max_scale`.
pub fn fit(content: Rect, max_scale: f32) -> (f32, Offset) {
    let s = (SAFE.width() / content.width())
        .min(SAFE.height() / content.height())
        .min(max_scale);
    let cx = (content.left + content.right) * 0.5;
    let cy = (content.top + content.bottom) * 0.5;
    let bx = (SAFE.left + SAFE.right) * 0.5;
    let by = (SAFE.top + SAFE.bottom) * 0.5;
    (s, Offset::new(bx - cx * s, by - cy * s))
}

// ── The header and footer ───────────────────────────────────────────────────

/// The header, laid out from this frame's registrations.
///
/// Three roles, three sizes, one axis: the movement label (small, tracked,
/// accent), the title (the scene's statement), the subtitle (its
/// consequence). Everything is centred on the frame's own centre line.
pub fn header(movement: &str, name: &str, act_a: f32) -> WidgetNode {
    let (lines, headline) = REG.with(|r| {
        let r = r.borrow();
        (r.lines.clone(), r.headline.clone())
    });
    let mut stack = Stack::new();

    // No movement label: the audience does not need the film's own table
    // of contents on screen. (`movement`, `name` and `act_a` stay in the
    // signature so the master's call is unchanged.)
    let _ = (movement, name, act_a);

    // The title and the subtitle.
    let (title, subs): (Option<(String, f32)>, Vec<Line>) = match headline {
        Some((h, a)) => (Some((h, a)), lines.clone()),
        None => {
            let t = lines.iter().find(|l| l.older).map(|l| (l.text.clone(), l.appear));
            let rest: Vec<Line> = lines.iter().filter(|l| !l.older).cloned().collect();
            (t, rest)
        }
    };
    if let Some((text, appear)) = title {
        let a = ease_out_cubic(clamp01(appear));
        if a > 0.01 {
            stack = stack.push(
                Positioned::new().left(PAD_X).top(34.0 + (1.0 - a) * 10.0).width(pf::W - 2.0 * PAD_X).height(58.0).child(
                    Opacity::new(a).child(
                        Text::new(text)
                            .style(pf::geist(40.0).weight(vieww_foundation::FontWeight::Medium).letter_spacing(0.2).color(pf::alpha(pf::INK, 0.97)))
                            .align(TextAlign::Center),
                    ),
                ),
            );
        }
    }
    // Subtitles: with a headline, the newest caption shown replaces the
    // older one (a crossfade in place); without, the one subtitle.
    let shown: Vec<(String, f32)> = if subs.len() >= 2 {
        let older = subs.iter().find(|l| l.older).cloned();
        let newer = subs.iter().find(|l| !l.older).cloned();
        match (older, newer) {
            (Some(o), Some(n)) => {
                let na = ease_out_cubic(clamp01(n.appear));
                vec![(o.text, ease_out_cubic(clamp01(o.appear)) * (1.0 - na)), (n.text, na)]
            }
            _ => subs.iter().map(|l| (l.text.clone(), ease_out_cubic(clamp01(l.appear)))).collect(),
        }
    } else {
        subs.iter().map(|l| (l.text.clone(), ease_out_cubic(clamp01(l.appear)))).collect()
    };
    for (text, a) in shown {
        if a <= 0.01 {
            continue;
        }
        stack = stack.push(
            Positioned::new().left(PAD_X).top(96.0 + (1.0 - a) * 8.0).width(pf::W - 2.0 * PAD_X).height(38.0).child(
                Opacity::new(a).child(
                    Text::new(text)
                        .style(pf::geist(25.0).letter_spacing(0.3).color(pf::alpha(pf::MUTED, 0.98)))
                        .align(TextAlign::Center),
                ),
            ),
        );
    }
    stack.into()
}

/// The footer's receipts row, centred.
pub fn footer_receipts() -> WidgetNode {
    let rows = REG.with(|r| r.borrow().receipts.clone());
    let mut stack = Stack::new();
    for (chips, appear) in rows {
        let a = ease_out_cubic(clamp01(appear));
        if a <= 0.01 {
            continue;
        }
        let size = 16.0;
        let widths: Vec<f32> = chips.iter().map(|(t, _)| chip_w(t, size)).collect();
        let gap = 14.0;
        let total: f32 = widths.iter().sum::<f32>() + gap * (widths.len().saturating_sub(1)) as f32;
        let mut x = (pf::W - total) * 0.5;
        for (i, ((text, color), w)) in chips.iter().zip(widths.iter()).enumerate() {
            let ca = clamp01((a - i as f32 * 0.08) / 0.6);
            if ca > 0.01 {
                let rise = (1.0 - ease_out_cubic(ca)) * 8.0;
                stack = stack.push(chip_at(x, FOOTER_Y + 8.0 + rise, *w, 34.0, text.clone(), size, *color, ca));
            }
            x += w + gap;
        }
    }
    stack.into()
}

/// The progress rail — one tick per scene, major ticks at movements, the
/// travelled part lit. It spans the padded width, at the frame's foot.
pub fn rail(abs: f32) -> WidgetNode {
    let list = super::scenes();
    let total = super::total_seconds();
    let starts: Vec<f32> = (0..list.len()).map(super::scene_start).collect();
    let acts = super::act_starts();
    let frac = (abs / total).clamp(0.0, 1.0);
    // The cold open is left alone: the rail arrives with the first scene.
    let open = super::scene_start(1);
    let ra = clamp01((abs - (open - 1.0)) / 1.5);
    Positioned::new()
        .left(0.0)
        .top(pf::H - 30.0)
        .width(pf::W)
        .height(24.0)
        .child(Painting::sized(
            Size::new(pf::W, 24.0),
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                if ra <= 0.01 {
                    return;
                }
                let (x0, x1, y) = (PAD_X, pf::W - PAD_X, 12.0);
                book.line(Offset::new(x0, y), Offset::new(x1, y), pf::alpha(Color::WHITE, 0.10 * ra), 1.0);
                for (i, st) in starts.iter().enumerate() {
                    let x = x0 + (x1 - x0) * (st / total);
                    let major = acts.contains(&i);
                    let h = if major { 6.0 } else { 3.5 };
                    book.line(
                        Offset::new(x, y - h),
                        Offset::new(x, y + h),
                        pf::alpha(Color::WHITE, (if major { 0.30 } else { 0.15 }) * ra),
                        1.0,
                    );
                }
                let px = x0 + (x1 - x0) * frac;
                book.line(Offset::new(x0, y), Offset::new(px, y), pf::alpha(super::ACCENT, 0.55 * ra), 1.6);
                book.circle(Offset::new(px, y), 4.0, pf::alpha(super::ACCENT, 0.95 * ra));
                book.circle(Offset::new(px, y), 8.0, pf::alpha(super::ACCENT, 0.18 * ra));
            }),
        ))
        .into()
}

/// The film's default room — used when a scene registers none. The same
/// dark the scenes' own rooms are built on, so a cut between a scene with a
/// room and one without is a change of light, not of surface.
pub fn default_ground(t: f32) -> WidgetNode {
    Positioned::fill()
        .child(Painting::sized(
            pf::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, s: Size| {
                let (w, h) = (s.width, s.height);
                book.rect(
                    Rect::new(0.0, 0.0, w, h),
                    vieww_foundation::Gradient::vertical().with_dither().with_stops(&[
                        (0.0, Color::rgb(14, 12, 17)),
                        (0.55, pf::BG_DEEP),
                        (1.0, Color::rgb(10, 9, 13)),
                    ]),
                );
                pf::stars(book, w, h, 0x5F11, 50, t, 0.05);
                pf::vignette(book, w, h, 0.5);
            }),
        ))
        .into()
}

// ── Type helpers ────────────────────────────────────────────────────────────

/// A line of type in a box, **vertically centred** in it and aligned
/// horizontally by `align`. The scenes' old `Positioned` + `Text` pairs
/// hung type from the top of its box, so every label sat low in the chip
/// or card it named; a box that centres its line cannot.
#[allow(clippy::too_many_arguments)]
pub fn label(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    text: impl Into<String>,
    style: vieww_foundation::TextStyle,
    align: TextAlign,
    a: f32,
) -> WidgetNode {
    if a <= 0.01 {
        return Stack::new().into();
    }
    // Vertical centring only: the paragraph is laid out at the box's width
    // and aligns itself horizontally. Aligning the child box as well would
    // apply the horizontal offset twice (a centred line lands flush right).
    let al = vieww_foundation::Alignment::CENTER_LEFT;
    Positioned::new()
        .left(x)
        .top(y)
        .width(w)
        .height(h)
        .child(Opacity::new(a.min(1.0)).child(
            Container::new()
                .width(w)
                .height(h)
                .alignment(al)
                .child(Text::new(text.into()).style(style).align(align)),
        ))
        .into()
}

/// A chip — a rounded, bordered label with its text centred both ways.
pub fn chip_at(x: f32, y: f32, w: f32, h: f32, text: impl Into<String>, size: f32, fg: Color, a: f32) -> WidgetNode {
    if a <= 0.01 {
        return Stack::new().into();
    }
    Positioned::new()
        .left(x)
        .top(y)
        .width(w)
        .height(h)
        .child(Opacity::new(a.min(1.0)).child(
            Container::new()
                .width(w)
                .height(h)
                .color(pf::alpha(pf::SURFACE, 0.9))
                .radius(8.0)
                .border(vieww_foundation::Border::new(pf::alpha(fg, 0.30), 1.0))
                .alignment(vieww_foundation::Alignment::CENTER)
                .child(Text::new(text.into()).style(pf::geist_mono(size).letter_spacing(1.0).color(fg))),
        ))
        .into()
}

/// A chip's width for `text` at `size` — measured the way `chip_at` draws.
pub fn chip_w(text: &str, size: f32) -> f32 {
    pf::gmono_tw(size, text.chars().count(), 1.0) + 2.0 * 14.0
}
