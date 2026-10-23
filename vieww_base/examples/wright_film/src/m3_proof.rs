//! Movement IV · THE PROOF — the receipt, and the delivery it proves.
//!
//! W09 is the film's own drawing of `wright verify`'s report for the
//! run the window just showed — the six checks, each with its own
//! sentence read from the store at film start, ticking green as they
//! pass. W10 returns to the real window: "Delivered and verified", the
//! artifacts, and the receipt overlay rising over the session page —
//! the glance the UI constitution promises.

use vieww_foundation::{Offset, Rect, Sketchbook, TextAlign};
use vieww_widget::prelude::*;

use crate::film::{ease, ACCENT, BAD, GOOD, H, INK, LINE, MUTED, SURFACE, W};
use crate::kit::{alpha, dlabel, mlabel};
use crate::wright_app::Script;

/// W09 · the receipt — six checks, in verify's own words.
///
/// The film loads the run's receipt at startup (`wright_app::demo()`)
/// and draws exactly what `wright verify` prints: each check's name,
/// its own sentence, its verdict. The signature seals the card. The
/// last line is the sentence the CLI ends with — the film's only
/// quotation longer than a label, and the one that matters.
pub(crate) fn the_receipt(ctx: &crate::film::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let mut stack = Stack::new();
    let clock = sec;

    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(W, H),
        PaintWith::new(move |book: &mut Sketchbook, s| {
            crate::kit::room(book, s.width, s.height, clock);
        }),
    )));

    // The receipt card.
    let a = ease(sec, 0.7);
    let card = Rect::new(480.0, 130.0, 1440.0, 880.0);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(W, H),
        PaintWith::new(move |book: &mut Sketchbook, _s| {
            if a <= 0.01 {
                return;
            }
            book.rrect(card, 22.0, alpha(SURFACE, a));
            book.stroke(
                vieww_foundation::Path::rounded_rect(card, 22.0),
                alpha(GOOD, 0.4 * a),
                1.4,
            );
        }),
    )));

    stack = stack.push(dlabel(
        card.left + 48.0,
        card.top + 34.0,
        900.0,
        44.0,
        "The receipt — wright verify, in its own words",
        30.0,
        INK,
        TextAlign::Left,
        ease(sec - 0.4, 0.7),
    ));
    stack = stack.push(mlabel(
        card.left + 48.0,
        card.top + 80.0,
        900.0,
        30.0,
        format!("run {} · the same store the CLI reads", ctx.demo.run),
        16.0,
        MUTED,
        TextAlign::Left,
        ease(sec - 0.7, 0.7),
    ));

    // The checks — the real ones, in the real order, each with its real
    // sentence. Passed checks tick in sequence; a failed one would wear
    // the bad colour and say so (the film does not skip it). The rows
    // are pitched at 76 px so all six clear the verdict with room to
    // breathe: the first cut of this scene stacked the verdict on the
    // sixth check's own sentence, and two greens on one line is the
    // film's worst kind of lie — one that is hard to read.
    let checks = &ctx.demo.receipt.checks;
    for (i, (name, ok, note)) in checks.iter().enumerate() {
        let at = 1.3 + i as f32 * 0.8;
        let ca = ease(sec - at, 0.55);
        if ca <= 0.01 {
            continue;
        }
        let y = card.top + 132.0 + i as f32 * 76.0;
        let color = if *ok { GOOD } else { BAD };
        let (name_s, note_s, ok_v, at_pt, frac) = (
            name.clone(),
            note.clone(),
            *ok,
            Offset::new(card.left + 76.0, y + 26.0),
            ease(sec - at, 0.5),
        );
        stack = stack.push(Positioned::fill().child(Painting::sized(
            vieww_foundation::Size::new(W, H),
            PaintWith::new(move |book: &mut Sketchbook, _s| {
                crate::kit::check_ring(book, at_pt, 15.0, frac, color);
                let _ = ok_v;
            }),
        )));
        stack = stack.push(mlabel(
            card.left + 110.0,
            y,
            320.0,
            32.0,
            name_s,
            19.0,
            color,
            TextAlign::Left,
            ca,
        ));
        // The sentence, one line, ellipsised by the box as the window's
        // own copyable lines are.
        stack = stack.push(dlabel(
            card.left + 110.0,
            y + 32.0,
            card.width() - 300.0,
            34.0,
            note_s,
            17.0,
            alpha(MUTED, 0.95),
            TextAlign::Left,
            ca * 0.9,
        ));
    }

    // The verdict — the report's own last line, below every check
    // (the card's floor, not the checks' ceiling: the two must never
    // share a pixel).
    let verdict = ctx
        .demo
        .receipt
        .report
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("verified")
        .to_string();
    let va = ease(sec - (1.3 + checks.len() as f32 * 0.8 + 0.5), 1.0);
    stack = stack.push(dlabel(
        card.left + 48.0,
        card.bottom - 132.0,
        card.width() - 360.0,
        84.0,
        verdict,
        21.0,
        GOOD,
        TextAlign::Left,
        va,
    ));

    // The seal: the Ed25519 signature, as a stamped ring.
    let seal_a = va;
    let seal_at = Offset::new(card.right - 96.0, card.bottom - 96.0);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(W, H),
        PaintWith::new(move |book: &mut Sketchbook, _s| {
            if seal_a <= 0.01 {
                return;
            }
            book.ring(seal_at, 34.0, 2.2, alpha(ACCENT, 0.9 * seal_a));
            book.ring(seal_at, 26.0, 1.2, alpha(ACCENT, 0.5 * seal_a));
        }),
    )));
    stack = stack.push(mlabel(
        card.right - 168.0,
        card.bottom - 66.0,
        150.0,
        28.0,
        "Ed25519",
        14.0,
        alpha(MUTED, 0.9),
        TextAlign::Center,
        seal_a,
    ));

    // The caption, under the card: the sentence the whole movement owns.
    stack = stack.push(dlabel(
        480.0,
        940.0,
        960.0,
        40.0,
        "A tampered byte fails by name. A receipt is evidence, not a promise.",
        22.0,
        alpha(INK, 0.95),
        TextAlign::Center,
        ease(sec - 8.6, 1.0),
    ));
    let _ = LINE;
    stack.into()
}

/// The script for W10 — the delivery, whole, then the receipt overlay.
pub(crate) fn w10_script() -> Script {
    Script {
        // The finished run whole; the receipt rises at 4.6 s.
        events_walk: None,
        receipt_from: Some(4.6),
        follow_from: Some(0.0),
        ..Default::default()
    }
}

/// W10 · the delivered — the result card, and the receipt over it.
///
/// The finished run in the real window: "Delivered and verified", the
/// artifacts with their sizes, the Review's pill. Then the film clicks
/// what a person would: "See the receipt" — and the overlay rises over
/// the page (v1.0.1's own feature, shown where it lives). The last
/// beat flips the theme to light: the evidence holds in daylight.
pub(crate) fn the_delivered(ctx: &crate::film::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let mut stack = Stack::new();
    stack = stack.push(chrome_note(
        sec,
        0.8,
        "the run's own result card — the review's verdict, the artifacts, the receipt saved",
    ));
    stack = stack.push(chrome_note(
        sec,
        4.8,
        "See the receipt — the CLI's evidence, inside the window where the work happened",
    ));
    stack = stack.push(chrome_receipt(ctx, "Esc backs out of anything"));
    stack.into()
}

// ── the chrome (the movement's own, quieter than III's) ─────────────────────

fn chrome_note(sec: f32, at: f32, text: &str) -> WidgetNode {
    let a = ease(sec - at, 0.9) * (1.0 - ease(sec - (at + 3.2), 0.7));
    if a <= 0.01 {
        return Stack::new().into();
    }
    let text = text.to_string();
    vieww_widget::Positioned::new()
        .left(0.0)
        .top(14.0)
        .width(W)
        .height(40.0)
        .child(
            Opacity::new(a.min(1.0)).child(
                Container::new()
                    .width(W)
                    .height(40.0)
                    .alignment(vieww_foundation::Alignment::CENTER)
                    .child(
                        vieww_widget::Text::new(text)
                            .size(19.0)
                            .color(alpha(INK, 0.95))
                            .align(TextAlign::Center),
                    ),
            ),
        )
        .into()
}

fn chrome_receipt(_ctx: &crate::film::Ctx, text: &str) -> WidgetNode {
    mlabel(
        W - 1050.0,
        1030.0,
        980.0,
        32.0,
        text,
        15.0,
        alpha(MUTED, 0.8),
        TextAlign::Right,
        1.0,
    )
}
