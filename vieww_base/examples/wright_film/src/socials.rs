//! socials — the short cuts, one per place a film full of film won't fit.
//!
//! The keynote is the film; these are its **doors**: a 41-second ask for
//! X, a 61-second proof for LinkedIn (the trust buyer's platform — the
//! gate, the bounds, the receipt, the refire), a 33-second square for
//! the Instagram feed, and a native 9:16 Reels/Shorts cut where the real
//! product gets a legible portrait-safe panel.
//!
//! # The rules the keynote set, held here too
//!
//! - **A frame is a pure function of its index.** The 16:9 cuts reuse
//!   the keynote's own scene builders, unretimed — a social cut is the
//!   same film, shorter, not a different one rendered cheaper.
//! - **Movement III is not mocked.** The Instagram and Reels/Shorts window
//!   scenes mount the real `wright::ui` shell on the film's own driver,
//!   the same as the keynote's Wright act — each places it for its canvas.
//! - **The mark is centred where a mark belongs**: both square and vertical
//!   opening/closing cards place it on their canvas's horizontal centre,
//!   with the wordmark directly below.
//!
//! # Why these four shapes
//!
//! | cut | canvas | the door |
//! |---|---|---|
//! | `wright_twitter.mp4` | 1920×1080 · 0:41 | X's autoplay feed: the ask, the gate, the mark |
//! | `wright_linkedin.mp4` | 1920×1080 · 1:01 | the buyer's cut: every trust artifact, end to end |
//! | `wright_instagram.mp4` | 1080×1080 · 0:33 | the feed's square: the product, framed like a print |
//! | `wright_reels.mp4` | 1080×1920 · 0:33 | a native vertical story for Reels and Shorts |
//!
//! The 9:16 cut is composed natively, not cropped from the keynote. Its real
//! 1280×800 Wright window is scaled and placed as a readable product panel;
//! the title, proof and close each have their own vertical safe area.

use vieww_foundation::{Offset, Rect, Sketchbook, TextAlign};
use vieww_widget::prelude::*;

use crate::film::{ease, Ctx, Kind, SceneDef, ACCENT, GOOD, INK, LINE, MUTED, SURFACE, WAIT};
use crate::kit::{alpha, dlabel, mlabel, typed_label};

/// The square cut's canvas.
pub(crate) const SQ: f32 = 1080.0;
pub(crate) const VERT_W: f32 = 1080.0;
pub(crate) const VERT_H: f32 = 1920.0;

// ── the cuts ─────────────────────────────────────────────────────────────────

/// One social cut: a scene list, a canvas, and where the window sits on
/// it (the 16:9 cuts quote the keynote's placement; the square has its
/// own, computed for its own frame).
pub(crate) struct Cut {
    pub id: &'static str,
    /// The output file stem — `wright_twitter` → `wright_twitter.mp4`.
    pub out: &'static str,
    pub width: f32,
    pub height: f32,
    /// (scale, dx, dy) for the mounted window, the Wright act's scenes.
    pub window: (f32, f32, f32),
    /// Draw the movement label chrome (the keynote's own framing). The
    /// social cuts go quieter: no act labels, just the footer's receipt.
    pub act_chrome: bool,
    pub scenes: Vec<SceneDef>,
}

/// The named cut, or `None` for a word that isn't one.
pub(crate) fn cut(which: &str) -> Option<Cut> {
    match which {
        "twitter" => Some(Cut {
            id: "twitter",
            out: "wright_twitter",
            width: crate::film::W,
            height: crate::film::H,
            window: crate::wright_app::window_place(),
            act_chrome: false,
            scenes: pick(&["W00", "W01", "W03", "W12"]),
        }),
        "linkedin" => Some(Cut {
            id: "linkedin",
            out: "wright_linkedin",
            width: crate::film::W,
            height: crate::film::H,
            window: crate::wright_app::window_place(),
            act_chrome: false,
            scenes: pick(&["W03", "W05", "W09", "W10", "W11", "W12"]),
        }),
        "instagram" => Some(Cut {
            id: "instagram",
            out: "wright_instagram",
            width: SQ,
            height: SQ,
            window: square_window(),
            act_chrome: false,
            scenes: square_scenes(),
        }),
        "reels" | "shorts" => Some(Cut {
            id: "reels",
            out: "wright_reels",
            width: VERT_W,
            height: VERT_H,
            window: vertical_window(),
            act_chrome: false,
            scenes: vertical_scenes(),
        }),
        _ => None,
    }
}

/// The keynote's own scenes by id, in the order given — same builders,
/// same clocks: a social cut is the film, shorter.
fn pick(ids: &[&str]) -> Vec<SceneDef> {
    let all = crate::film::scenes();
    ids.iter()
        .map(|id| {
            all.iter()
                .find(|s| s.id == *id)
                .unwrap_or_else(|| panic!("no scene {id}"))
                .clone()
        })
        .collect()
}

/// The square's window: 1280×800 at 0.82× is 1050×656 — the square's own
/// width less a 15-px margin each side, sitting in the frame's middle
/// band so the caption has the floor and the receipt the ceiling.
pub(crate) fn square_window() -> (f32, f32, f32) {
    let scale = (SQ - 30.0) / crate::wright_app::APP_W;
    let w = crate::wright_app::APP_W * scale;
    (scale, (SQ - w) * 0.5, 190.0)
}

/// The vertical cut keeps the real app legible instead of cropping it into
/// a tall slit: 0.72× gives the 1280×800 window a 922×576 product panel.
pub(crate) fn vertical_window() -> (f32, f32, f32) {
    let scale = ((VERT_W - 96.0) / crate::wright_app::APP_W).min(0.72);
    let w = crate::wright_app::APP_W * scale;
    (scale, (VERT_W - w) * 0.5, 650.0)
}

// ── the square's own scenes ──────────────────────────────────────────────────

/// The square's four scenes: question → words → window → end card.
/// 8 + 9 + 10 + 6 = 33 s.
fn square_scenes() -> Vec<SceneDef> {
    vec![
        SceneDef {
            id: "P1",
            name: "the_question",
            seconds: 8.0,
            kind: Kind::Pure,
            build: p1_question,
        },
        SceneDef {
            id: "P2",
            name: "the_words",
            seconds: 9.0,
            kind: Kind::Pure,
            build: p2_words,
        },
        SceneDef {
            id: "P3",
            name: "the_window",
            seconds: 10.0,
            kind: Kind::Wright,
            build: p3_window,
        },
        SceneDef {
            id: "P4",
            name: "the_endcard",
            seconds: 6.0,
            kind: Kind::Pure,
            build: p4_endcard,
        },
    ]
}

/// The script for the square's window scene: the finished run, whole —
/// the result on screen, the thread at its end, nothing asked of the
/// viewer in ten seconds except to look.
pub(crate) fn p3_script() -> crate::wright_app::Script {
    crate::wright_app::Script {
        events_walk: None,
        follow_from: Some(0.0),
        ..Default::default()
    }
}

/// A native vertical cut: question → safeguards → the real app → close.
/// The 33-second cadence matches the square cut, but all positions are
/// designed for 1080×1920 rather than enlarging or cropping the keynote.
fn vertical_scenes() -> Vec<SceneDef> {
    vec![
        SceneDef {
            id: "R1",
            name: "vertical_question",
            seconds: 7.0,
            kind: Kind::Pure,
            build: r1_question,
        },
        SceneDef {
            id: "R2",
            name: "vertical_trust",
            seconds: 7.0,
            kind: Kind::Pure,
            build: r2_trust,
        },
        SceneDef {
            id: "R3",
            name: "vertical_window",
            seconds: 12.0,
            kind: Kind::Wright,
            build: r3_window,
        },
        SceneDef {
            id: "R4",
            name: "vertical_endcard",
            seconds: 7.0,
            kind: Kind::Pure,
            build: r4_endcard,
        },
    ]
}

fn vertical_background(sec: f32) -> WidgetNode {
    Positioned::fill()
        .child(Painting::sized(
            vieww_foundation::Size::new(VERT_W, VERT_H),
            PaintWith::new(move |book: &mut Sketchbook, size| {
                crate::kit::room(book, size.width, size.height, sec);
            }),
        ))
        .into()
}

fn r1_question(ctx: &Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let mut stack = Stack::new().push(vertical_background(sec));
    let mark_p = ease(sec, 1.8);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(VERT_W, VERT_H),
        PaintWith::new(move |book: &mut Sketchbook, _| {
            crate::kit::centered_mark(book, VERT_W, 420.0, 142.0, mark_p, ACCENT, 8.0);
        }),
    )));
    stack = stack.push(dlabel(
        64.0,
        665.0,
        VERT_W - 128.0,
        84.0,
        "What should exist?",
        48.0,
        INK,
        TextAlign::Center,
        ease(sec - 1.4, 0.9),
    ));
    stack = stack.push(dlabel(
        72.0,
        770.0,
        VERT_W - 144.0,
        52.0,
        "Describe it. Agents wright it.",
        25.0,
        MUTED,
        TextAlign::Center,
        ease(sec - 2.5, 0.9),
    ));
    stack = stack.push(mlabel(
        72.0,
        1030.0,
        VERT_W - 144.0,
        40.0,
        "NOT A TERMINAL. A FACTORY FLOOR.",
        17.0,
        alpha(MUTED, 0.9),
        TextAlign::Center,
        ease(sec - 4.2, 0.7),
    ));
    stack.into()
}

fn r2_trust(ctx: &Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let mut stack = Stack::new().push(vertical_background(sec));
    let cards = [
        (
            Rect::new(72.0, 520.0, VERT_W - 72.0, 735.0),
            "01  ·  PLAN BEFORE EXECUTION",
            "The request becomes an inspectable, bounded plan.",
            ACCENT,
        ),
        (
            Rect::new(72.0, 790.0, VERT_W - 72.0, 1005.0),
            "02  ·  PERMISSIONS BEFORE REACH",
            "Approvals gate consequential actions and delivery.",
            WAIT,
        ),
        (
            Rect::new(72.0, 1060.0, VERT_W - 72.0, 1275.0),
            "03  ·  RECEIPT AFTER THE RUN",
            "Checks, costs and artifacts stay reviewable.",
            GOOD,
        ),
    ];
    let card_alpha = ease(sec, 0.6);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(VERT_W, VERT_H),
        PaintWith::new(move |book: &mut Sketchbook, _| {
            for (rect, _, _, color) in cards {
                book.rrect(rect, 22.0, alpha(SURFACE, card_alpha));
                book.stroke(
                    vieww_foundation::Path::rounded_rect(rect, 22.0),
                    alpha(color, card_alpha),
                    1.5,
                );
            }
        }),
    )));
    stack = stack.push(dlabel(
        64.0,
        300.0,
        VERT_W - 128.0,
        64.0,
        "Power, with boundaries.",
        43.0,
        INK,
        TextAlign::Center,
        ease(sec - 0.2, 0.7),
    ));
    stack = stack.push(dlabel(
        74.0,
        385.0,
        VERT_W - 148.0,
        42.0,
        "A harness should make trust visible.",
        21.0,
        MUTED,
        TextAlign::Center,
        ease(sec - 0.8, 0.7),
    ));
    for (i, (rect, title, body, color)) in cards.iter().enumerate() {
        let a = ease(sec - (0.9 + i as f32 * 0.6), 0.6);
        stack = stack.push(mlabel(
            rect.left + 24.0,
            rect.top + 28.0,
            rect.width() - 48.0,
            32.0,
            *title,
            17.0,
            *color,
            TextAlign::Left,
            a,
        ));
        stack = stack.push(dlabel(
            rect.left + 24.0,
            rect.top + 82.0,
            rect.width() - 48.0,
            72.0,
            *body,
            21.0,
            INK,
            TextAlign::Left,
            a,
        ));
    }
    stack = stack.push(mlabel(
        72.0,
        1370.0,
        VERT_W - 144.0,
        38.0,
        "PLAN  /  APPROVE  /  VERIFY",
        18.0,
        alpha(GOOD, 0.95),
        TextAlign::Center,
        ease(sec - 4.4, 0.8),
    ));
    stack.into()
}

pub(crate) fn r3_script() -> crate::wright_app::Script {
    crate::wright_app::Script {
        events_walk: None,
        follow_from: Some(0.0),
        ..Default::default()
    }
}

fn r3_window(ctx: &Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let mut stack = Stack::new();
    stack = stack.push(dlabel(
        56.0,
        330.0,
        VERT_W - 112.0,
        55.0,
        "A real run. A visible receipt.",
        32.0,
        INK,
        TextAlign::Center,
        ease(sec - 0.2, 0.7),
    ));
    stack = stack.push(dlabel(
        60.0,
        405.0,
        VERT_W - 120.0,
        44.0,
        "Not a mockup — the actual Wright shell.",
        20.0,
        MUTED,
        TextAlign::Center,
        ease(sec - 0.8, 0.7),
    ));
    let (scale, _, dy) = vertical_window();
    let under = dy + crate::wright_app::APP_H * scale + 24.0;
    stack = stack.push(mlabel(
        56.0,
        under,
        VERT_W - 112.0,
        40.0,
        format!(
            "{} events  ·  ${:.4}  ·  verified",
            ctx.demo.events.len(),
            ctx.demo.result.cost
        ),
        17.0,
        alpha(GOOD, 0.95),
        TextAlign::Center,
        ease(sec - 2.0, 0.8),
    ));
    stack = stack.push(dlabel(
        64.0,
        under + 58.0,
        VERT_W - 128.0,
        66.0,
        "Every action is bounded. Every result is reviewable.",
        21.0,
        INK,
        TextAlign::Center,
        ease(sec - 3.5, 0.8),
    ));
    stack.into()
}

fn r4_endcard(ctx: &Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let mut stack = Stack::new().push(vertical_background(sec));
    let mark_p = ease(sec, 1.7);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(VERT_W, VERT_H),
        PaintWith::new(move |book: &mut Sketchbook, _| {
            crate::kit::centered_mark(book, VERT_W, 620.0, 142.0, mark_p, ACCENT, 8.0);
        }),
    )));
    stack = stack.push(dlabel(
        80.0,
        805.0,
        VERT_W - 160.0,
        94.0,
        "wright",
        76.0,
        INK,
        TextAlign::Center,
        ease(sec - 1.0, 0.8),
    ));
    stack = stack.push(dlabel(
        80.0,
        935.0,
        VERT_W - 160.0,
        56.0,
        "You write it. Agents wright it.",
        24.0,
        MUTED,
        TextAlign::Center,
        ease(sec - 2.0, 0.8),
    ));
    stack = stack.push(mlabel(
        72.0,
        1085.0,
        VERT_W - 144.0,
        42.0,
        "TRUST BEFORE REACH",
        19.0,
        alpha(GOOD, 0.95),
        TextAlign::Center,
        ease(sec - 3.4, 0.8),
    ));
    stack = stack.push(mlabel(
        72.0,
        1170.0,
        VERT_W - 144.0,
        40.0,
        "WRIGHT 1.0  ·  BUILT WITH VIEWW",
        15.0,
        alpha(MUTED, 0.9),
        TextAlign::Center,
        ease(sec - 4.2, 0.8),
    ));
    stack.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reels_cut_is_native_vertical_and_keeps_the_real_window_inside_safe_area() {
        let cut = cut("reels").expect("native vertical cut");
        assert_eq!((cut.width, cut.height), (1080.0, 1920.0));
        assert_eq!(cut.out, "wright_reels");
        assert_eq!(cut.scenes.len(), 4);
        assert_eq!(cut.scenes[2].kind, Kind::Wright);
        let (scale, x, y) = cut.window;
        assert!(x >= 0.0 && y >= 0.0);
        assert!(x + crate::wright_app::APP_W * scale <= cut.width);
        assert!(y + crate::wright_app::APP_H * scale <= cut.height);
    }
}

/// P1 · the question — the cold open, squared.
///
/// The mark centred high, the question under it, the thesis under that.
/// Nothing else asks to be read: a feed stops for a question it can
/// finish in its head.
pub(crate) fn p1_question(ctx: &Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let mut stack = Stack::new();
    let clock = sec;

    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(SQ, SQ),
        PaintWith::new(move |book: &mut Sketchbook, s| {
            crate::kit::room(book, s.width, s.height, clock);
        }),
    )));

    // The mark, centred on the square's own axis, drawing itself.
    let mark_p = ease(sec, 1.8);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(SQ, SQ),
        PaintWith::new(move |book: &mut Sketchbook, _s| {
            let side = 120.0;
            crate::kit::centered_mark(book, SQ, 220.0, side, mark_p, ACCENT, 8.0);
        }),
    )));

    // The question.
    stack = stack.push(dlabel(
        90.0,
        430.0,
        SQ - 180.0,
        96.0,
        "What should exist?",
        54.0,
        INK,
        TextAlign::Center,
        ease(sec - 1.6, 1.0),
    ));
    // The thesis, quieter.
    stack = stack.push(dlabel(
        90.0,
        540.0,
        SQ - 180.0,
        48.0,
        "Describe it. Agents wright it.",
        24.0,
        MUTED,
        TextAlign::Center,
        ease(sec - 3.2, 1.0),
    ));
    // The one competitive word, in mono.
    stack = stack.push(mlabel(
        90.0,
        700.0,
        SQ - 180.0,
        36.0,
        "Not a terminal. A factory floor.",
        18.0,
        alpha(MUTED, 0.9),
        TextAlign::Center,
        ease(sec - 5.0, 1.0),
    ));
    stack.into()
}

/// P2 · the words — the request, typed, squared.
///
/// The composer card fills the square's width; the request types itself
/// at a size a phone can read; the three lifts land under it.
pub(crate) fn p2_words(ctx: &Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let request = "Every morning at 8, email me the top Hacker News stories, summarized.";
    let type_end = 5.2;
    let typed_frac = (sec / type_end).clamp(0.0, 1.0);
    let mut stack = Stack::new();
    let clock = sec;

    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(SQ, SQ),
        PaintWith::new(move |book: &mut Sketchbook, s| {
            crate::kit::room(book, s.width, s.height, clock);
        }),
    )));

    // The composer card.
    let card_a = ease(sec, 0.7);
    let card = Rect::new(60.0, 300.0, SQ - 60.0, 560.0);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(SQ, SQ),
        PaintWith::new(move |book: &mut Sketchbook, _s| {
            if card_a <= 0.01 {
                return;
            }
            book.rrect(card, 18.0, alpha(SURFACE, card_a));
            book.stroke(
                vieww_foundation::Path::rounded_rect(card, 18.0),
                alpha(LINE, card_a),
                1.0,
            );
        }),
    )));

    // The typed request — three lines' room, the square's own width.
    stack = stack.push(typed_label(
        card.left + 26.0,
        card.top + 26.0,
        card.width() - 52.0,
        130.0,
        request,
        25.0,
        INK,
        typed_frac,
        card_a,
    ));

    // The three lifts: when, what, to whom.
    let lifts = [
        ("0 8 * * *", " — when", WAIT),
        ("the top stories", " — what", ACCENT),
        ("email me", " — the deliverable", GOOD),
    ];
    for (i, (text, tag, color)) in lifts.iter().enumerate() {
        let a = ease(sec - (type_end + 0.5 + i as f32 * 0.5), 0.8);
        if a <= 0.01 {
            continue;
        }
        let y = 640.0 + i as f32 * 54.0;
        let text = format!("{text}{tag}");
        stack = stack.push(mlabel(
            90.0,
            y,
            SQ - 180.0,
            34.0,
            text,
            19.0,
            *color,
            TextAlign::Center,
            a,
        ));
    }
    stack.into()
}

/// P3 · the window — the product, framed like a print.
///
/// The scene's own chrome is one line above and one below: what it is
/// ("the actual app"), and what it did (the demo's own headline). The
/// window between them is the real shell on the real spine.
pub(crate) fn p3_window(ctx: &Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let d = ctx.demo;
    let mut stack = Stack::new();

    // The title, above the window's band.
    stack = stack.push(dlabel(
        60.0,
        110.0,
        SQ - 120.0,
        44.0,
        "This is the app. The run is real.",
        26.0,
        INK,
        TextAlign::Center,
        ease(sec - 0.4, 0.8),
    ));
    stack = stack.push(dlabel(
        60.0,
        156.0,
        SQ - 120.0,
        30.0,
        "Every step, every cent — projected from the run's own event spine.",
        15.5,
        alpha(MUTED, 0.95),
        TextAlign::Center,
        ease(sec - 1.4, 0.8),
    ));

    // The floor, under the window: the demo's own headline, in mono.
    let (_, dy) = (square_window().0, square_window().2);
    let under = dy + crate::wright_app::APP_H * square_window().0 + 18.0;
    stack = stack.push(mlabel(
        60.0,
        under,
        SQ - 120.0,
        34.0,
        d.headline.to_string(),
        16.0,
        alpha(MUTED, 0.9),
        TextAlign::Center,
        ease(sec - 2.2, 0.9),
    ));
    stack.into()
}

/// P4 · the end card — the mark centred, the wordmark under it, the hold.
///
/// The same composition the keynote's own end card holds, squared: the
/// film ends the way it began, on the mark.
pub(crate) fn p4_endcard(_ctx: &Ctx) -> WidgetNode {
    let sec = _ctx.sec;
    let mut stack = Stack::new();
    let clock = sec;

    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(SQ, SQ),
        PaintWith::new(move |book: &mut Sketchbook, s| {
            crate::kit::room(book, s.width, s.height, clock);
        }),
    )));

    // The mark, centred, drawing itself.
    let mark_p = ease(sec, 1.6);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(SQ, SQ),
        PaintWith::new(move |book: &mut Sketchbook, _s| {
            let side = 110.0;
            crate::kit::centered_mark(book, SQ, 240.0, side, mark_p, ACCENT, 8.0);
        }),
    )));

    // The wordmark under it, centred.
    stack = stack.push(dlabel(
        90.0,
        420.0,
        SQ - 180.0,
        100.0,
        "wright",
        72.0,
        INK,
        TextAlign::Center,
        ease(sec - 1.0, 0.9),
    ));

    // The thesis.
    stack = stack.push(dlabel(
        90.0,
        560.0,
        SQ - 180.0,
        44.0,
        "You write it. Agents wright it.",
        24.0,
        MUTED,
        TextAlign::Center,
        ease(sec - 1.8, 0.9),
    ));

    // The one accent firing.
    let fire_a = ease(sec - 3.2, 0.6);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(SQ, SQ),
        PaintWith::new(move |book: &mut Sketchbook, _s| {
            if fire_a <= 0.01 {
                return;
            }
            let y = 540.0;
            let x = SQ * 0.5 + 170.0 + 40.0 * (1.0 - fire_a);
            book.line(
                Offset::new(x - 90.0 * fire_a, y),
                Offset::new(x, y),
                alpha(ACCENT, fire_a),
                2.0,
            );
        }),
    )));

    // The quiet close, in mono.
    stack = stack.push(mlabel(
        90.0,
        700.0,
        SQ - 180.0,
        34.0,
        "trust before reach",
        17.0,
        alpha(GOOD, 0.9),
        TextAlign::Center,
        ease(sec - 3.8, 1.0),
    ));
    stack.into()
}
