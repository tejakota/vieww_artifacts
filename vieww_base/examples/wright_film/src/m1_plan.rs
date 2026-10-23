//! Movement II · THE WORKFORCE — the plan as a picture.
//!
//! Two scenes: the agent cards settling into the staged graph (the
//! compiler's own layout, the system workforce injected beside yours),
//! and the bounds every card runs in (budgets, permissions, the
//! sandbox). The movement's job is the sentence the architecture doc
//! owns: *the user composes the visible tier; the compiler silently
//! wraps it in the system tier.*

use vieww_foundation::{Offset, Rect, Sketchbook, TextAlign};
use vieww_widget::prelude::*;

use crate::film::{ease, ACCENT, GOOD, H, INK, LINE, MUTED, SURFACE, W, WAIT};
use crate::kit::{alpha, dlabel, mlabel};

/// One agent card, drawn: name, role line, the tools it may use, and
/// the pills the plan wears ("built in", "asks you first"). Returns
/// nothing — the caller owns the layout.
fn agent_card(book: &mut Sketchbook, at: Offset, w: f32, a: f32, accent: bool) {
    let h = 132.0;
    let r = Rect::new(at.dx, at.dy, at.dx + w, at.dy + h);
    book.rrect(r, 16.0, alpha(SURFACE, a));
    book.stroke(
        vieww_foundation::Path::rounded_rect(r, 16.0),
        alpha(
            if accent { ACCENT } else { LINE },
            if accent { 0.85 } else { 0.9 } * a,
        ),
        if accent { 1.6 } else { 1.0 },
    );
    if accent {
        // The working glow, the app's own grammar: an accent edge.
        book.rrect(
            Rect::new(r.left - 3.0, r.top - 3.0, r.right + 3.0, r.bottom + 3.0),
            18.0,
            alpha(ACCENT, 0.12 * a),
        );
    }
}

/// The card's text, as one positioned column over the canvas card.
/// The meta and tools lines are mono at 13–14 px in a card 208 wide:
/// the box keeps 18 px of air each side, so a line has ~172 px — about
/// 21 characters. Every line below is written to that budget, because
/// a mono line that runs past its card's edge reads as a bug in the
/// card, not in the sentence.
fn card_text(at: Offset, w: f32, a: f32, name: &str, meta: &str, tools: &str) -> WidgetNode {
    if a <= 0.01 {
        return Stack::new().into();
    }
    Stack::new()
        .push(dlabel(
            at.dx + 18.0,
            at.dy + 14.0,
            w - 36.0,
            34.0,
            name,
            21.0,
            INK,
            TextAlign::Left,
            a,
        ))
        .push(mlabel(
            at.dx + 18.0,
            at.dy + 50.0,
            w - 36.0,
            28.0,
            meta,
            14.0,
            MUTED,
            TextAlign::Left,
            a,
        ))
        .push(mlabel(
            at.dx + 18.0,
            at.dy + 84.0,
            w - 36.0,
            28.0,
            tools,
            13.0,
            alpha(MUTED, 0.85),
            TextAlign::Left,
            a,
        ))
        .into()
}

/// W04 · the workforce — the graph, staged.
///
/// The plan's eight agents settle into their stages, left to right, the
/// way the engine view lays them out: your three agents (Fetcher,
/// Summarizer, Mailer) among the five the compiler injected (Shipwright,
/// Logwright, Testwright, Reviewer, Operator). The edges draw
/// themselves; the data flows; the Mailer wears its ask.
pub(crate) fn the_workforce(ctx: &crate::film::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let mut stack = Stack::new();
    let clock = sec;

    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(W, H),
        PaintWith::new(move |book: &mut Sketchbook, s| {
            kit::room(book, s.width, s.height, clock);
        }),
    )));

    // The stage columns: (x, label, cards).
    // Stage 1 lays the skeleton; stage 2 reads; 3 summarizes; 4 checks;
    // 5 reviews; 6 sends; 7 registers.
    let col_w = 208.0;
    let gap = 22.0;
    let x0 = 150.0;
    let y0 = 300.0;
    let cards: [(f32, &str, &str, &str, bool); 8] = [
        // (stage, name, meta, tools, yours) — every meta line inside the
        // 21-character mono budget the card's width gives it.
        (0.0, "Shipwright", "built-in · skeleton", "", false),
        (1.0, "Logwright", "chip: logging", "write-output", false),
        (1.0, "Fetcher", "yours · tier cheap", "http-read", true),
        (
            2.0,
            "Summarizer",
            "yours · tier quality",
            "write-output",
            true,
        ),
        (
            3.0,
            "Testwright",
            "chip: tests · checks",
            "run-tests",
            false,
        ),
        (4.0, "Reviewer", "APPROVE or REJECT", "", false),
        (5.0, "Mailer", "yours · asks first", "http-post", true),
        (6.0, "Operator", "built-in · schedule", "", false),
    ];
    let card_at = move |stage: f32, row: usize| -> Offset {
        Offset::new(x0 + stage * (col_w + gap), y0 + row as f32 * 176.0)
    };
    // Where each card sits (stage, row-in-stage).
    let place = [
        (0.0, 0usize),
        (1.0, 0),
        (1.0, 1),
        (2.0, 0),
        (3.0, 0),
        (4.0, 0),
        (5.0, 0),
        (6.0, 0),
    ];

    // The edges: from each card to the next stage's cards — the film
    // draws the three that carry the story.
    let edges = [
        (2usize, 3usize),
        (3usize, 4usize),
        (5usize, 6usize),
        (4usize, 6usize),
    ];
    let edge_a = ease(sec - 4.0, 1.0);
    let flow_t = sec;
    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(W, H),
        PaintWith::new(move |book: &mut Sketchbook, _s| {
            for (from, to) in edges {
                let (fs, fr) = place[from];
                let (ts, tr) = place[to];
                let a = card_at(fs, fr);
                let b = card_at(ts, tr);
                let p0 = Offset::new(a.dx + col_w, a.dy + 66.0);
                let p1 = Offset::new(b.dx, b.dy + 66.0);
                let pts = kit::thread_pts(p0, p1, 0.0);
                if edge_a > 0.01 {
                    kit::march_thread(book, &pts, flow_t, ACCENT, 2.0, edge_a);
                    kit::rider(book, &pts, (flow_t * 0.35) % 1.0, ACCENT, 3.2, edge_a);
                }
            }
        }),
    )));

    // The cards, arriving stage by stage.
    for (i, c) in cards.iter().enumerate() {
        let (name, meta, tools, yours) = (c.1, c.2, c.3, c.4);
        let (stage, row) = place[i];
        let at = card_at(stage, row);
        let arrive = 0.4 + stage * 0.55 + row as f32 * 0.2;
        let ca = ease(sec - arrive, 0.6);
        if ca <= 0.01 {
            continue;
        }
        let accent = yours;
        stack = stack.push(Positioned::fill().child(Painting::sized(
            vieww_foundation::Size::new(W, H),
            PaintWith::new(move |book: &mut Sketchbook, _s| {
                agent_card(book, at, col_w, ca, accent);
            }),
        )));
        stack = stack.push(card_text(at, col_w, ca, name, meta, tools));
        let tag = if yours && name == "Mailer" {
            ("asks you first", WAIT)
        } else if yours {
            ("yours", alpha(MUTED, 0.9))
        } else {
            ("added by Wright", ACCENT)
        };
        stack = stack.push(mlabel(
            at.dx + 12.0,
            at.dy + 108.0,
            col_w - 24.0,
            24.0,
            tag.0,
            12.0,
            tag.1,
            TextAlign::Left,
            ease(sec - arrive - 0.3, 0.6),
        ));
    }

    // The stage floor line.
    stack = stack.push(dlabel(
        x0,
        y0 - 74.0,
        1400.0,
        40.0,
        "independent cards run at the same time — stages, not a queue",
        19.0,
        alpha(MUTED, 0.95),
        TextAlign::Left,
        ease(sec - 1.0, 0.9),
    ));
    stack = stack.push(dlabel(
        x0,
        920.0,
        1400.0,
        44.0,
        "You compose the three. The compiler wraps them in the five. Nine prompts, all contracts.",
        22.0,
        INK,
        TextAlign::Left,
        ease(sec - 5.6, 1.0),
    ));
    stack.into()
}

/// W05 · the bounds — guardrails as the plan's own geometry.
///
/// The same graph, pulled back; over it, the three bounds: the budget
/// (with the run's real number from the demo firing), the permissions
/// (two cards may touch the network; the rest may not), and the sandbox
/// every tool runs in. Least privilege, drawn.
pub(crate) fn the_bounds(ctx: &crate::film::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let mut stack = Stack::new();
    let clock = sec;

    // The demo's real cost, for the budget line.
    let cost = ctx.demo.result.cost;

    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(W, H),
        PaintWith::new(move |book: &mut Sketchbook, s| {
            kit::room(book, s.width, s.height, clock);
            // The graph, receding: three quiet columns of the workforce.
            let cols = [240.0, 640.0, 1040.0];
            let names: [&[&str]; 3] = [
                &["Shipwright", "Logwright"],
                &["Fetcher", "Summarizer", "Testwright"],
                &["Reviewer", "Mailer", "Operator"],
            ];
            for (ci, col) in cols.iter().enumerate() {
                for (ri, name) in names[ci].iter().enumerate() {
                    let _ = name;
                    let r = Rect::new(
                        *col,
                        240.0 + ri as f32 * 120.0,
                        *col + 150.0,
                        336.0 + ri as f32 * 120.0,
                    );
                    book.rrect(r, 12.0, alpha(SURFACE, 0.35));
                    book.stroke(
                        vieww_foundation::Path::rounded_rect(r, 12.0),
                        alpha(LINE, 0.4),
                        1.0,
                    );
                }
            }
        }),
    )));

    // Bound 1 — the budget.
    let b1 = ease(sec - 0.4, 0.8);
    let cap = 0.50;
    let frac = (cost / cap).clamp(0.0, 1.0) as f32;
    let bar = Rect::new(240.0, 700.0, 840.0, 716.0);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        vieww_foundation::Size::new(W, H),
        PaintWith::new(move |book: &mut Sketchbook, _s| {
            if b1 <= 0.01 {
                return;
            }
            kit::progress_line(book, bar, frac, GOOD);
        }),
    )));
    stack = stack.push(dlabel(
        240.0,
        620.0,
        700.0,
        44.0,
        "The budget — decided before, not after",
        26.0,
        INK,
        TextAlign::Left,
        b1,
    ));
    stack = stack.push(mlabel(
        240.0,
        736.0,
        700.0,
        34.0,
        format!("this run cost ${cost:.4} of the ${cap:.2} a run cap — the bar is the plan's own"),
        16.0,
        MUTED,
        TextAlign::Left,
        ease(sec - 1.2, 0.8),
    ));

    // Bound 2 — the permissions.
    let b2 = ease(sec - 2.6, 0.8);
    stack = stack.push(dlabel(
        980.0,
        620.0,
        700.0,
        44.0,
        "The permissions — off until the plan says",
        26.0,
        INK,
        TextAlign::Left,
        b2,
    ));
    let rows = [
        ("Fetcher", "network: read", true),
        ("Mailer", "network: send — asks first", true),
        ("Summarizer", "writes one file", false),
        ("everything else", "no network, no shell, no secrets", false),
    ];
    for (i, (who, what, on)) in rows.iter().enumerate() {
        let ra = ease(sec - (3.0 + i as f32 * 0.35), 0.6);
        if ra <= 0.01 {
            continue;
        }
        let y = 690.0 + i as f32 * 42.0;
        let color = if *on { WAIT } else { MUTED };
        let dot = if *on { WAIT } else { LINE };
        let (at_pt, dot_c) = (Offset::new(992.0, y + 14.0), dot);
        stack = stack.push(Positioned::fill().child(Painting::sized(
            vieww_foundation::Size::new(W, H),
            PaintWith::new(move |book: &mut Sketchbook, _s| {
                book.circle(at_pt, 5.0, dot_c);
            }),
        )));
        stack = stack.push(mlabel(
            1012.0,
            y,
            660.0,
            30.0,
            format!("{who} — {what}"),
            16.0,
            color,
            TextAlign::Left,
            ra,
        ));
    }

    // Bound 3 — the sandbox, and the rule of two.
    let b3 = ease(sec - 5.0, 0.9);
    stack = stack.push(dlabel(
        240.0,
        920.0,
        1440.0,
        44.0,
        "Every tool call passes one gate — budget, capability, approval — and a 3-leg agent asks before every send. The Rule of Two.",
        22.0,
        alpha(INK, 0.95),
        TextAlign::Left,
        b3,
    ));
    stack.into()
}

use crate::kit;
