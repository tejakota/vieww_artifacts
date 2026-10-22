//! Movement V · THE REFIRE — the moat, and the end card.
//!
//! W11 is the product's differentiator in one gesture: change one
//! requirement, and only what the change touches runs again — the rest
//! is served from the cache, free. The graph remembers what W04 built;
//! two nodes light, the rest wear "reused from cache"; the sentence the
//! architecture doc owns is typed out in full. W12 is the end card: the
//! mark, the wordmark, the thesis, the hold.

use vieww_foundation::{Offset, Rect, Sketchbook, TextAlign};
use vieww_widget::prelude::*;

use crate::film::{ease, ACCENT, GOOD, H, INK, LINE, MUTED, SURFACE, W};
use crate::kit::{alpha, dlabel, mlabel, typed_label};

/// W11 · the change — "make the summaries punchier."
///
/// The request arrives as a small reply under the graph; the graph
/// answers: Summarizer and Testwright and Reviewer light up (they
/// refire), everything else dims to "reused from cache" — and the
/// sentence that no competitor can say lands beneath:
/// *This change refires three steps. The rest is cached — free.*
pub(crate) fn the_change(ctx: &crate::film::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let mut stack = Stack::new();
    let clock = sec;

    // The graph, held over from movement II — the same layout language,
    // one row, so the audience knows exactly what changed.
    let cols: [(&str, bool); 8] = [
        ("Shipwright", false),
        ("Logwright", false),
        ("Fetcher", false),
        ("Summarizer", true),
        ("Testwright", true),
        ("Reviewer", true),
        ("Mailer", false),
        ("Operator", false),
    ];
    let refire = ease(sec - 3.0, 1.2);

    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(W, H),
        PaintWith::new(move |book: &mut Sketchbook, s| {
            crate::kit::room(book, s.width, s.height, clock);
            let n = cols.len();
            let cw = 178.0;
            let gap = 20.0;
            let total = n as f32 * cw + (n as f32 - 1.0) * gap;
            let x0 = (W - total) * 0.5;
            for (i, (name, hot)) in cols.iter().enumerate() {
                let name = *name;
                let _ = name;
                let x = x0 + i as f32 * (cw + gap);
                let y = 330.0;
                let r = Rect::new(x, y, x + cw, y + 118.0);
                let dim = if *hot {
                    1.0
                } else {
                    0.45 + 0.55 * (1.0 - refire)
                };
                book.rrect(r, 14.0, alpha(SURFACE, dim));
                book.stroke(
                    vieww_foundation::Path::rounded_rect(r, 14.0),
                    alpha(
                        if *hot { ACCENT } else { LINE },
                        if *hot { 0.9 } else { 0.8 } * dim,
                    ),
                    if *hot { 1.6 } else { 1.0 },
                );
                // The cached pill on the cold cards, once the change lands.
                if !*hot && refire > 0.5 {
                    let pill_y = y + 84.0;
                    book.rrect(
                        Rect::new(x + 14.0, pill_y, x + cw - 14.0, pill_y + 22.0),
                        11.0,
                        alpha(GOOD, 0.14),
                    );
                }
            }
        }),
    )));

    // The card names + their state.
    let n = cols.len();
    let cw = 178.0;
    let gap = 20.0;
    let total = n as f32 * cw + (n as f32 - 1.0) * gap;
    let x0 = (W - total) * 0.5;
    for (i, (name, hot)) in cols.iter().enumerate() {
        let name = *name;
        let x = x0 + i as f32 * (cw + gap);
        let dim = if *hot {
            1.0
        } else {
            0.45 + 0.55 * (1.0 - refire)
        };
        let appear = ease(sec - 0.4 - i as f32 * 0.08, 0.5);
        if appear <= 0.01 {
            continue;
        }
        stack = stack.push(dlabel(
            x + 16.0,
            348.0,
            cw - 32.0,
            30.0,
            name,
            18.0,
            alpha(INK, dim),
            TextAlign::Left,
            appear,
        ));
        if *hot {
            stack = stack.push(mlabel(
                x + 16.0,
                382.0,
                cw - 32.0,
                24.0,
                "refires",
                13.0,
                ACCENT,
                TextAlign::Left,
                ease(sec - 3.2, 0.7),
            ));
        } else if refire > 0.5 {
            stack = stack.push(mlabel(
                x + 26.0,
                386.0,
                cw - 40.0,
                20.0,
                "cached — free",
                12.0,
                GOOD,
                TextAlign::Left,
                ease(sec - 3.8, 0.7),
            ));
        }
    }

    // The reply that asked for the change, above the graph.
    let ask_a = ease(sec - 1.0, 0.8);
    let bubble = Rect::new(W * 0.5 - 420.0, 190.0, W * 0.5 + 420.0, 268.0);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(W, H),
        PaintWith::new(move |book: &mut Sketchbook, _s| {
            if ask_a <= 0.01 {
                return;
            }
            book.rrect(bubble, 16.0, alpha(crate::film::BUBBLE, ask_a));
        }),
    )));
    stack = stack.push(typed_label(
        bubble.left + 28.0,
        bubble.top + 18.0,
        780.0,
        40.0,
        "Make the summaries punchier.",
        21.0,
        vieww_foundation::Color::WHITE,
        (sec - 1.2) / 1.6,
        ask_a,
    ));

    // The sentence the moat owns, typed in full.
    let moat_at = 5.2;
    stack = stack.push(typed_label(
        240.0,
        640.0,
        1500.0,
        44.0,
        "This change refires Summarizer + Testwright + Reviewer — est. $0.03, 45 seconds. Fetcher is cached: free.",
        24.0,
        INK,
        (sec - moat_at) / 3.0,
        ease(sec - moat_at, 0.6),
    ));

    // The contrast line: the competitor's second run costs the same as
    // its first. Ours is a cache hit.
    stack = stack.push(dlabel(
        240.0,
        730.0,
        1500.0,
        40.0,
        "Every competitor re-runs everything. The second run here is a cache hit.",
        21.0,
        alpha(MUTED, 0.95),
        TextAlign::Left,
        ease(sec - 8.0, 0.9),
    ));
    stack = stack.push(chrome_receipt(ctx));
    stack.into()
}

fn chrome_receipt(_ctx: &crate::film::Ctx) -> WidgetNode {
    mlabel(
        W - 1050.0,
        1030.0,
        980.0,
        32.0,
        "the cache key: agent ∥ model ∥ inputs ∥ tools ∥ permissions ∥ version",
        15.0,
        alpha(MUTED, 0.8),
        TextAlign::Right,
        1.0,
    )
}

/// W12 · the end card — the mark, the thesis, the hold.
///
/// The mark draws itself one last time and settles; the wordmark beside
/// it; the thesis under both; the honest install line in mono; and a
/// long hold on the quiet — the studio film's own ending grammar, with
/// Wright's own one accent firing on the word that matters.
pub(crate) fn the_endcard(ctx: &crate::film::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let mut stack = Stack::new();
    let clock = sec;

    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(W, H),
        PaintWith::new(move |book: &mut Sketchbook, s| {
            crate::kit::room(book, s.width, s.height, clock);
        }),
    )));

    // The mark, centred, drawing itself over 2.2 s.
    let mark_p = ease(sec, 2.2);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(W, H),
        PaintWith::new(move |book: &mut Sketchbook, _s| {
            let side = 132.0;
            let at = Offset::new(W * 0.5 - 620.0 - side * 0.5, 380.0);
            crate::kit::draw_mark(book, at, side, mark_p, ACCENT, 9.0);
        }),
    )));

    // The wordmark beside it.
    stack = stack.push(dlabel(
        W * 0.5 - 440.0,
        396.0,
        600.0,
        110.0,
        "wright",
        88.0,
        INK,
        TextAlign::Left,
        ease(sec - 1.4, 1.0),
    ));

    // The thesis, one accent on the word that matters.
    let t_a = ease(sec - 2.8, 1.2);
    stack = stack.push(dlabel(
        W * 0.5 - 800.0,
        620.0,
        1600.0,
        48.0,
        "You write it. Agents wright it. The artifact is wrought.",
        30.0,
        INK,
        TextAlign::Center,
        t_a,
    ));

    // The honest line, in mono: where it lives, what it costs.
    let m_a = ease(sec - 4.6, 1.0);
    stack = stack.push(mlabel(
        W * 0.5 - 800.0,
        720.0,
        1600.0,
        40.0,
        "wright 1.0 · trust before reach · one dependency, and it is this repository's own",
        19.0,
        alpha(MUTED, 0.95),
        TextAlign::Center,
        m_a,
    ));

    // The one accent firing — the release.
    let fire_a = ease(sec - 6.4, 0.7);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(W, H),
        PaintWith::new(move |book: &mut Sketchbook, _s| {
            if fire_a <= 0.01 {
                return;
            }
            // A single hairline sweep under the thesis — the light rule
            // (no radial glows), one crisp accent line, drawn.
            let y = 700.0;
            let x = W * 0.5 + 330.0 + 60.0 * (1.0 - fire_a);
            book.line(
                Offset::new(x - 120.0 * fire_a, y),
                Offset::new(x, y),
                alpha(ACCENT, fire_a),
                2.0,
            );
        }),
    )));

    // The audience line, last, quietest.
    stack = stack.push(dlabel(
        W * 0.5 - 800.0,
        840.0,
        1600.0,
        36.0,
        "A film for everyone who has something that should exist.",
        20.0,
        alpha(MUTED, 0.85),
        TextAlign::Center,
        ease(sec - 7.6, 1.2),
    ));
    let _ = LINE;
    stack.into()
}
