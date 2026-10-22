//! Movement III · THE RUN — the actual wright window.
//!
//! Three scenes, all one mounting of the real shell ([`wright_app`]):
//! the session page with the run pouring in (the run card is a
//! projection of the real event spine, fed as a growing prefix), the
//! Mailer's approval card arriving and being dragged right to allow,
//! and the engine view open on the live graph with the inspector on a
//! node. The scenes' `build` functions return only the chrome that
//! rides on top — labels, callouts, receipts — because the window
//! beneath is the product.

use vieww_foundation::{Rect, Sketchbook, TextAlign};
use vieww_widget::prelude::*;

use crate::film::{ease, INK, MUTED, W};
use crate::kit::{alpha, mlabel};
use crate::wright_app::Script;

/// The script for W06 — the run pours in over the whole scene: the
/// spine walks from empty to whole, the thread following its own end.
pub(crate) fn w06_script() -> Script {
    Script {
        // The spine walks from nothing to everything over the scene's
        // middle; the last 3 s hold the finished card.
        events_walk: Some((0.6, 9.8)),
        follow_from: Some(0.0),
        ..Default::default()
    }
}

/// W06 · the window — the run, whole.
///
/// The real session page: the request above, the plan card, the run
/// card's steps lighting one by one as the spine grows — Shipwright
/// first, the Mailer last — the spend climbing in the status bar. The
/// film's chrome says what cannot be said loudly enough: this is the
/// app, and the events are real.
pub(crate) fn the_window(ctx: &crate::film::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let mut stack = Stack::new();
    let d = ctx.demo;

    // The walk through the spine, eased so the steps read.
    let walk = (sec / 9.5).clamp(0.0, 1.0);
    let _ = d.events_at(walk);

    stack = stack.push(chrome_note(
        sec,
        1.2,
        "the run card is a projection of the run's own event spine — every step, every second, every cent",
    ));
    stack = stack.push(chrome_receipt(
        ctx,
        &format!("{} · {} events", d.headline, d.events.len()),
    ));
    stack.into()
}

/// The script for W07 — the approval, then the spine continues.
pub(crate) fn w07_script() -> Script {
    Script {
        // The spine holds most of the way through — the Mailer's stage —
        // while the ask waits and is answered.
        events_walk: Some((0.0, 0.0)),
        approval: Some((
            2.0,
            "Mailer".into(),
            "the Mailer wants to send the digest — it asks before it touches the world".into(),
        )),
        follow_from: Some(0.0),
        ..Default::default()
    }
}

/// W07 · the approval — the gate, asking.
///
/// The real approval card arrives over the thread; it waits; then the
/// film drags it right — the card's own drag grammar, the ±120 px
/// threshold — and the run continues. One sentence of chrome: nothing
/// sends without a person.
pub(crate) fn the_approval(ctx: &crate::film::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let mut stack = Stack::new();
    stack = stack.push(chrome_note(
        sec,
        1.6,
        "drag the card right to allow, left to refuse — the buttons do the same",
    ));
    stack = stack.push(chrome_note(
        sec,
        6.4,
        "the gate asked. the person answered. the run continues.",
    ));
    stack = stack.push(chrome_receipt(
        ctx,
        "an outbound step of a 3-leg agent asks every time — the Rule of Two",
    ));
    stack.into()
}

/// The script for W08 — the engine view, open on the live graph.
pub(crate) fn w08_script() -> Script {
    Script {
        // The spine keeps moving under the open engine: the graph is
        // alive while the inspector reads a node.
        events_walk: Some((0.2, 10.5)),
        engine_from: Some(1.2),
        inspect_from: Some((5.4, "Summarizer".into())),
        follow_from: Some(0.0),
        ..Default::default()
    }
}

/// W08 · the engine — hold Space, and keep it held.
///
/// The engine view fades in over the page (the page behind blurring
/// down, exactly as the shell draws it): the graph live, edges marching
// into working nodes, tools lighting as they are called. At 5.4 s the
/// inspector opens on the Summarizer: what it may touch, in words; its
/// log, in the log's own lines.
pub(crate) fn the_engine(ctx: &crate::film::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let mut stack = Stack::new();
    stack = stack.push(chrome_note(
        sec,
        1.4,
        "hold Space to peek — E keeps it: the agents and the tools, live",
    ));
    stack = stack.push(chrome_note(
        sec,
        5.6,
        "what it may touch, in words — the inspector's own sentences",
    ));
    stack = stack.push(chrome_receipt(
        ctx,
        "Space peeks · E keeps it · the graph is the plan, alive",
    ));
    stack.into()
}

// ── the chrome ──────────────────────────────────────────────────────────────

/// One sentence of film chrome, top centre — what the window cannot say
/// about itself.
fn chrome_note(sec: f32, at: f32, text: &str) -> WidgetNode {
    let a = ease(sec - at, 0.9) * (1.0 - ease(sec - (at + 3.4), 0.7));
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

/// The footer's receipt line, bottom right — the scene's evidence.
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

/// The film's ground under the window: the room, the window's shadow.
/// Called by the master before the window's scene is appended.
pub(crate) fn ground_under_window(book: &mut Sketchbook, w: f32, h: f32, clock: f32, win: Rect) {
    crate::kit::room(book, w, h, clock);
    // The shadow the window casts on the room.
    book.rrect(
        Rect::new(
            win.left + 8.0,
            win.top + 16.0,
            win.right + 8.0,
            win.bottom + 16.0,
        ),
        20.0,
        alpha(vieww_foundation::Color::rgb(0, 0, 0), 0.45),
    );
}

/// The hairline the room draws around the window, over it — the master
/// calls this on the chrome driver.
pub(crate) fn window_edge(book: &mut Sketchbook, win: Rect) {
    book.stroke(
        vieww_foundation::Path::rounded_rect(win, 14.0),
        alpha(vieww_foundation::Color::rgb(70, 70, 78), 0.9),
        1.5,
    );
}
