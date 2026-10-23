//! socials — the short cuts, one per place a film full of film won't fit.
//!
//! The keynote is the film; these are its **doors**: a 41-second ask for
//! X, a 61-second proof for LinkedIn (the trust buyer's platform — the
//! gate, the bounds, the receipt, the refire), and a 33-second square
//! for the Instagram feed, where the product itself is the hero frame.
//!
//! # The rules the keynote set, held here too
//!
//! - **A frame is a pure function of its index.** The 16:9 cuts reuse
//!   the keynote's own scene builders, unretimed — a social cut is the
//!   same film, shorter, not a different one rendered cheaper.
//! - **Movement III is not mocked.** The Instagram cut's window scene
//!   mounts the real `wright::ui` shell on the film's own driver, the
//!   same as the keynote's Wright act — the square just places it
//!   smaller.
//! - **The mark is centred where a mark belongs**: the square's opening
//!   and close are the same composition as the keynote's — the mark on
//!   the frame's own axis, the wordmark under it.
//!
//! # Why these three shapes
//!
//! | cut | canvas | the door |
//! |---|---|---|
//! | `wright_twitter.mp4` | 1920×1080 · 0:41 | X's autoplay feed: the ask, the gate, the mark |
//! | `wright_linkedin.mp4` | 1920×1080 · 1:01 | the buyer's cut: every trust artifact, end to end |
//! | `wright_instagram.mp4` | 1080×1080 · 0:33 | the feed's square: the product, framed like a print |
//!
//! 9:16 (Reels/Shorts) is deliberately not here: a vertical frame has no
//! room for a 1280×800 window, and cropping the keynote into a tall slit
//! would break the one thing this film holds — that what you see is the
//! product. A vertical cut wants its own scenes, written for the frame.

use vieww_foundation::{Offset, Rect, Sketchbook, TextAlign};
use vieww_widget::prelude::*;

use crate::film::{ease, Ctx, Kind, SceneDef, ACCENT, GOOD, INK, LINE, MUTED, SURFACE, WAIT};
use crate::kit::{alpha, dlabel, mlabel, typed_label};

/// The square cut's canvas.
pub(crate) const SQ: f32 = 1080.0;

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
            let at = Offset::new(SQ * 0.5 - side * 0.5, 220.0);
            crate::kit::draw_mark(book, at, side, mark_p, ACCENT, 8.0);
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
        format!("{}", d.headline),
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
            let at = Offset::new(SQ * 0.5 - side * 0.5, 240.0);
            crate::kit::draw_mark(book, at, side, mark_p, ACCENT, 8.0);
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
