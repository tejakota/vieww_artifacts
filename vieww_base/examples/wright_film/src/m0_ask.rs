//! Movement I · THE ASK — the question, the words, the blueprint, the
//! gate.
//!
//! Four scenes, one sentence of story: you say what should exist; the
//! words become a typed blueprint you never write; and nothing runs
//! until you have read, in plain words generated from that blueprint,
//! exactly what will exist. The movement ends on the Confirm Gate —
//! trust before reach, the release's whole thesis in one card.

use vieww_foundation::{Color, Offset, Rect, Sketchbook, TextAlign};
use vieww_widget::prelude::*;

use crate::film::{ease, ACCENT, GOOD, H, INK, LINE, MUTED, SURFACE, W, WAIT};
use crate::kit::{alpha, dlabel, mlabel, typed_label};

/// W00 · the question — the cold open.
///
/// A dark room. The W draws itself. One question arrives, and under it
/// the film's thesis in the product's own words: describe it, and it's
/// wrought. Nothing else asks to be read for ten seconds.
pub(crate) fn the_question(ctx: &crate::film::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let mut stack = Stack::new();
    let clock = sec;

    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(W, H),
        PaintWith::new(move |book: &mut Sketchbook, s| {
            kit::room(book, s.width, s.height, clock);
        }),
    )));

    // The mark, centred high, drawing itself over the first 2.4 s and
    // then settling upward as the words arrive.
    let mark_p = ease(sec, 2.4);
    let lift = ease(sec - 2.6, 1.4) * 96.0;
    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(W, H),
        PaintWith::new(move |book: &mut Sketchbook, _s| {
            let side = 168.0;
            kit::centered_mark(book, W, 300.0 - lift, side, mark_p, ACCENT, 11.0);
        }),
    )));

    // The question.
    stack = stack.push(dlabel(
        W * 0.5 - 800.0,
        560.0,
        1600.0,
        96.0,
        "What should exist?",
        64.0,
        INK,
        TextAlign::Center,
        ease(sec - 2.2, 1.2),
    ));
    // The thesis, under it, quieter.
    stack = stack.push(dlabel(
        W * 0.5 - 800.0,
        680.0,
        1600.0,
        48.0,
        "Describe it. Agents wright it. The artifact is wrought.",
        26.0,
        MUTED,
        TextAlign::Center,
        ease(sec - 4.2, 1.4),
    ));
    // Where the others start — the film's one competitive word.
    stack = stack.push(mlabel(
        W * 0.5 - 800.0,
        856.0,
        1600.0,
        36.0,
        "Not a terminal. Not a workflow editor. A factory floor.",
        20.0,
        alpha(MUTED, 0.9),
        TextAlign::Center,
        ease(sec - 7.6, 1.4),
    ));
    stack.into()
}

/// W01 · the words — the request, typed.
///
/// The request types itself letter by letter, exactly as a person would
/// type it into Home's composer; once it is whole, the three parts the
/// plan keeps lift out — when, what, and the deliverable.
pub(crate) fn the_words(ctx: &crate::film::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let request = "Every morning at 8, fetch the top Hacker News stories, summarize them, and email me the digest.";
    let type_end = 6.2;
    let typed_frac = (sec / type_end).clamp(0.0, 1.0);
    let mut stack = Stack::new();
    let clock = sec;

    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(W, H),
        PaintWith::new(move |book: &mut Sketchbook, s| {
            kit::room(book, s.width, s.height, clock);
        }),
    )));

    // The composer card, as Home wears it: surface, hairline, the prompt
    // line, the caret — the film's own drawing of the real composer. The
    // card holds three lines of prompt: the request at this size wraps
    // to three in the box it has, and a box that fits two was what
    // clipped the request's last words at the card's right edge.
    let card_a = ease(sec, 0.8);
    let lift = ease(sec - type_end, 1.2) * 120.0;
    let card = Rect::new(W * 0.5 - 620.0, 360.0 - lift, W * 0.5 + 620.0, 580.0 - lift);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(W, H),
        PaintWith::new(move |book: &mut Sketchbook, _s| {
            if card_a <= 0.01 {
                return;
            }
            book.rrect(card, 20.0, alpha(SURFACE, card_a));
            book.stroke(
                vieww_foundation::Path::rounded_rect(card, 20.0),
                alpha(LINE, card_a),
                1.0,
            );
        }),
    )));

    // The typed request, in the composer.
    stack = stack.push(typed_label(
        card.left + 32.0,
        card.top + 28.0,
        1160.0,
        120.0,
        request,
        26.0,
        INK,
        typed_frac,
        card_a,
    ));

    // The three lifts, once the sentence is whole: when, what, and to
    // whom — the parts the plan keeps. The film's first receipt.
    let lifts = [
        ("0 8 * * *", " — when", WAIT),
        ("the top stories", " — what", ACCENT),
        ("email me the digest", " — the deliverable", GOOD),
    ];
    for (i, (text, tag, color)) in lifts.iter().enumerate() {
        let a = ease(sec - (type_end + 0.6 + i as f32 * 0.55), 0.9);
        if a <= 0.01 {
            continue;
        }
        let y = 680.0 + i as f32 * 66.0;
        let text = format!("{text}{tag}");
        stack = stack.push(mlabel(
            W * 0.5 - 500.0,
            y,
            1000.0,
            40.0,
            text,
            22.0,
            *color,
            TextAlign::Center,
            a,
        ));
    }
    stack.into()
}

/// W02 · the blueprint — the words become a file you never write.
///
/// The request dissolves into the typed blueprint: agent cards as KDL
/// blocks assembling line by line, the edges and chips arriving in the
/// accent, the right rail saying what makes this file different from a
/// prompt: typed, versioned, diffable.
pub(crate) fn the_blueprint(ctx: &crate::film::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let mut stack = Stack::new();
    let clock = sec;

    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(W, H),
        PaintWith::new(move |book: &mut Sketchbook, s| {
            kit::room(book, s.width, s.height, clock);
        }),
    )));

    // The file card — the blueprint on the page.
    let a = ease(sec, 0.7);
    let card = Rect::new(420.0, 150.0, 1500.0, 830.0);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(W, H),
        PaintWith::new(move |book: &mut Sketchbook, _s| {
            if a <= 0.01 {
                return;
            }
            book.rrect(card, 22.0, alpha(SURFACE, a));
            book.stroke(
                vieww_foundation::Path::rounded_rect(card, 22.0),
                alpha(LINE, a),
                1.0,
            );
            // The file's own header bar, the editor's three dots.
            let head = Rect::new(card.left, card.top, card.right, card.top + 52.0);
            book.rect(head, alpha(Color::rgb(0x2D, 0x2D, 0x2D), a));
            book.ring(
                Offset::new(card.left + 26.0, card.top + 26.0),
                5.0,
                1.6,
                alpha(Color::rgb(0xF1, 0x4C, 0x4C), 0.8 * a),
            );
            book.ring(
                Offset::new(card.left + 48.0, card.top + 26.0),
                5.0,
                1.6,
                alpha(WAIT, 0.8 * a),
            );
            book.ring(
                Offset::new(card.left + 70.0, card.top + 26.0),
                5.0,
                1.6,
                alpha(GOOD, 0.8 * a),
            );
        }),
    )));
    stack = stack.push(mlabel(
        card.left + 96.0,
        card.top + 10.0,
        600.0,
        32.0,
        "blueprint.kdl",
        17.0,
        MUTED,
        TextAlign::Left,
        a,
    ));

    // The KDL, assembling line by line.
    let lines: [(&str, Color, f32); 9] = [
        ("meta { name \"HN Daily Digest\" }", MUTED, 0.0),
        ("agent Fetcher { goal \"Fetch the top stories\"", INK, 0.6),
        (
            "    tools \"http-read\"  permissions network=true }",
            INK,
            1.2,
        ),
        (
            "agent Summarizer { goal \"Two punchy sentences each\"",
            INK,
            1.8,
        ),
        (
            "    in \"stories.json\"  out \"summaries.json\" }",
            INK,
            2.4,
        ),
        ("agent Mailer { goal \"Send the digest\"", INK, 3.0),
        (
            "    approval \"before-send\"  permissions network=true }",
            INK,
            3.6,
        ),
        ("edge from=\"Fetcher\" to=\"Summarizer\"", ACCENT, 4.2),
        (
            "chips { scheduling cron=\"0 8 * * *\"  tests on }",
            ACCENT,
            4.8,
        ),
    ];
    for (i, line) in lines.iter().enumerate() {
        let (text, color, at) = (line.0, line.1, line.2);
        let la = ease(sec - at, 0.5);
        if la <= 0.01 {
            continue;
        }
        stack = stack.push(mlabel(
            card.left + 40.0,
            card.top + 84.0 + i as f32 * 44.0,
            1020.0,
            36.0,
            text,
            19.0,
            color,
            TextAlign::Left,
            la,
        ));
    }

    // The right rail: what makes this file different from a prompt.
    let notes = [
        "typed — a parser, not a prompt",
        "versioned — every change is a commit",
        "diffable — only what changed refires",
    ];
    for (i, note) in notes.iter().enumerate() {
        let note = *note;
        let na = ease(sec - (5.6 + i as f32 * 0.5), 0.8);
        if na <= 0.01 {
            continue;
        }
        stack = stack.push(dlabel(
            1560.0,
            320.0 + i as f32 * 56.0,
            330.0,
            40.0,
            note,
            19.0,
            alpha(MUTED, 0.95),
            TextAlign::Left,
            na,
        ));
    }

    // The caption.
    stack = stack.push(dlabel(
        420.0,
        880.0,
        1080.0,
        44.0,
        "One file. The whole project. The source of truth — you never write it.",
        24.0,
        INK,
        TextAlign::Center,
        ease(sec - 6.6, 1.0),
    ));
    stack.into()
}

/// W03 · the gate — the Confirm Gate, in words.
///
/// The blueprint's own summary, code-generated — never model-written —
/// check-marked line by line: what will exist, what runs, what it may
/// touch, what it may cost. The approval waits. This is the gate every
/// incident report wishes had existed.
pub(crate) fn the_gate(ctx: &crate::film::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let mut stack = Stack::new();
    let clock = sec;

    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(W, H),
        PaintWith::new(move |book: &mut Sketchbook, s| {
            kit::room(book, s.width, s.height, clock);
        }),
    )));

    // The gate card.
    let a = ease(sec, 0.7);
    let card = Rect::new(500.0, 170.0, 1420.0, 850.0);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(W, H),
        PaintWith::new(move |book: &mut Sketchbook, _s| {
            if a <= 0.01 {
                return;
            }
            book.rrect(card, 22.0, alpha(SURFACE, a));
            book.stroke(
                vieww_foundation::Path::rounded_rect(card, 22.0),
                alpha(ACCENT, 0.55 * a),
                1.6,
            );
        }),
    )));

    stack = stack.push(dlabel(
        card.left + 48.0,
        card.top + 36.0,
        820.0,
        44.0,
        "This is exactly what will exist",
        30.0,
        INK,
        TextAlign::Left,
        ease(sec - 0.4, 0.7),
    ));
    stack = stack.push(dlabel(
        card.left + 48.0,
        card.top + 82.0,
        820.0,
        32.0,
        "Written from the blueprint by code, not by a model.",
        17.0,
        MUTED,
        TextAlign::Left,
        ease(sec - 0.8, 0.7),
    ));

    // The gate's sentences, checking in one by one — the shape of
    // `wright_core::confirm`'s own summary.
    let checks: [(String, &str, Color); 6] = [
        (
            "A daily digest of the top HN stories, emailed at 8.".into(),
            "the deliverable",
            INK,
        ),
        (
            "8 stages run, Fetcher → Summarizer → Mailer among them.".into(),
            "the work",
            INK,
        ),
        (
            "Fetcher and Mailer may use the network. Nothing else may.".into(),
            "the reach",
            WAIT,
        ),
        (
            "The Mailer asks before it sends anything at all.".into(),
            "the gate's ask",
            WAIT,
        ),
        (
            "Never more than $0.50 a run, $3.00 a day.".into(),
            "the cost",
            GOOD,
        ),
        (
            "Delivered only after Testwright and the Reviewer pass.".into(),
            "the bar",
            GOOD,
        ),
    ];
    for (i, (text, tag, color)) in checks.iter().enumerate() {
        let (text, tag) = (text.as_str(), *tag);
        let at = 1.4 + i as f32 * 0.75;
        let ca = ease(sec - at, 0.6);
        if ca <= 0.01 {
            continue;
        }
        let y = card.top + 150.0 + i as f32 * 76.0;
        let (color, frac, at_pt) = (
            *color,
            ease(sec - at, 0.45),
            Offset::new(card.left + 74.0, y + 26.0),
        );
        stack = stack.push(Positioned::fill().child(Painting::sized(
            vieww_foundation::Size::new(W, H),
            PaintWith::new(move |book: &mut Sketchbook, _s| {
                kit::check_ring(book, at_pt, 15.0, frac, alpha(color, 1.0));
            }),
        )));
        stack = stack.push(dlabel(
            card.left + 108.0,
            y,
            700.0,
            52.0,
            text,
            21.0,
            color,
            TextAlign::Left,
            ca,
        ));
        stack = stack.push(mlabel(
            card.right - 280.0,
            y + 8.0,
            230.0,
            36.0,
            tag,
            15.0,
            alpha(MUTED, 0.9),
            TextAlign::Right,
            ca,
        ));
    }

    // The buttons, held: the gate waits for a person.
    let ba = ease(sec - 6.4, 0.8);
    let btn_y = card.bottom - 96.0;
    let (yes_r, no_r) = (
        Rect::new(card.left + 48.0, btn_y, card.left + 268.0, btn_y + 52.0),
        Rect::new(card.left + 292.0, btn_y, card.left + 512.0, btn_y + 52.0),
    );
    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(W, H),
        PaintWith::new(move |book: &mut Sketchbook, _s| {
            if ba <= 0.01 {
                return;
            }
            book.rrect(yes_r, 14.0, alpha(ACCENT, ba));
            book.stroke(
                vieww_foundation::Path::rounded_rect(no_r, 14.0),
                alpha(LINE, ba),
                1.2,
            );
        }),
    )));
    stack = stack.push(dlabel(
        yes_r.left,
        btn_y,
        yes_r.width(),
        52.0,
        "Build this",
        19.0,
        Color::WHITE,
        TextAlign::Center,
        ba,
    ));
    stack = stack.push(dlabel(
        no_r.left,
        btn_y,
        no_r.width(),
        52.0,
        "Change something",
        17.0,
        MUTED,
        TextAlign::Center,
        ba,
    ));

    // The movement's last word.
    stack = stack.push(dlabel(
        500.0,
        900.0,
        920.0,
        40.0,
        "Nothing runs until you have read this. Trust before reach.",
        22.0,
        alpha(INK, 0.95),
        TextAlign::Center,
        ease(sec - 7.8, 0.9),
    ));
    stack.into()
}

use crate::kit;
