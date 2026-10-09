//! Movement III, two more beats of the real studio — the parts a person
//! only finds after a week with it, shown in the first minute.
//!
//! * **Z12B · make it yours.** The command palette opens over the real
//!   window — the studio's whole command registry, one list (its count on
//!   screen is `Command::ALL.len()`) — and the real filter narrows it as
//!   `>theme` is typed into it. Then the accent walks round the studio's
//!   own wheel (`theme::ACCENTS`): teal, amber, rose, green, home to
//!   purple — and every region of the window follows on the frame it is
//!   set, because every region reads the one signal.
//! * **Z13B · the inspector.** The right pane switches to the Inspector:
//!   the previewed screen's render tree, read out of the frame that drew
//!   it. Then the semantics overlay goes on over the whole window — what
//!   a screen reader is told about every control. Both are the studio's
//!   own views (`RightTab::Inspector`, `show_semantics`), not drawings.
//!
//! Both scenes work in the application's own 1920×1080 space (content box
//! `APP`) and register no plate: the master shows the whole window.

use vieww_foundation::{Rect, Size, Sketchbook};
use vieww_widget::prelude::*;

use super::m3_studio::app;
use super::{frame, ACCENT, CANVAS, LEDGER, SYN_STRING, SYN_TYPE};
use crate::film_lib::{clamp01, ease_in_out, ease_out_cubic};
use crate::product_film as pf;

fn beats(sec: f32, list: &[(f32, &str)]) {
    let cur = list.iter().rposition(|(at, _)| sec >= *at);
    let Some(i) = cur else { return };
    let since = sec - list[i].0;
    let a = clamp01(since / 0.45);
    if i > 0 && a < 1.0 {
        frame::caption(list[i - 1].1, 1002.0, 1.0);
    }
    frame::caption(
        list[i].1,
        966.0,
        if i == 0 {
            clamp01((sec - 0.1) / 0.4)
        } else {
            a
        },
    );
}

/// A focus ring around an application rectangle.
fn ring(r: Rect, color: vieww_foundation::Color, a: f32) -> WidgetNode {
    Positioned::fill()
        .child(Painting::sized(
            CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                if a > 0.01 {
                    book.stroke_rrect(r.inflate(3.0), 10.0, pf::alpha(color, 0.9 * a), 3.0);
                }
            }),
        ))
        .into()
}

/// The camera: the content box eased between the whole window and a
/// region of it — a real push-in on the real app, no plate cut out.
fn camera(sec: f32, close: Rect, push_in: f32, pull_out: f32) {
    let z = ease_in_out(clamp01((sec - push_in) / 0.9))
        * (1.0 - ease_in_out(clamp01((sec - pull_out) / 0.9)));
    let l = |a: f32, b: f32| a + (b - a) * z;
    let full = super::layout::APP;
    frame::boxed(Rect::new(
        l(full.left, close.left),
        l(full.top, close.top),
        l(full.right, close.right),
        l(full.bottom, close.bottom),
    ));
}

/// The command palette's neighbourhood in the application's space.
const PALETTE_ZONE: Rect = Rect {
    left: 560.0,
    top: 30.0,
    right: 1360.0,
    bottom: 480.0,
};
/// The right pane (preview / inspector) and a margin of editor.
const PANE_ZONE: Rect = Rect {
    left: 1180.0,
    top: 40.0,
    right: 1920.0,
    bottom: 840.0,
};

pub(crate) fn make_it_yours(ctx: &pf::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    camera(sec, PALETTE_ZONE, 0.5, 4.1);
    let commands = viewwstudio::command::Command::ALL.len();
    let accents = viewwstudio::theme::ACCENTS.len();
    frame::headline("Make it yours", clamp01((sec - 0.1) / 0.5));
    let n_line = format!("{commands} commands — every one in a single palette.");
    beats(
        sec,
        &[
            (0.0, "Every command, one keystroke away."),
            (1.0, &n_line),
            (
                2.6,
                "Type to narrow it — the real filter, running as you type.",
            ),
            (4.6, "Pick an accent — the whole studio follows, live."),
            (9.8, "No restart, no reload: every region reads one signal."),
        ],
    );
    let mut stack = Stack::new();
    // The accent walk, named as it happens.
    let walk: [(f32, &str); 5] = [
        (4.6, "Teal"),
        (5.9, "Amber"),
        (7.2, "Rose"),
        (8.5, "Green"),
        (9.8, "Purple"),
    ];
    if let Some(i) = walk.iter().rposition(|(at, _)| sec >= *at) {
        let (at, name) = walk[i];
        let a = ease_out_cubic(clamp01((sec - at) / 0.3)) * (1.0 - clamp01((sec - 11.2) / 0.5));
        stack = stack.push(frame::chip_at(
            1620.0,
            990.0,
            260.0,
            54.0,
            format!("accent · {name}"),
            26.0,
            ACCENT,
            a,
        ));
    }
    (frame::receipts(
        &[
            (&format!("{commands} commands"), ACCENT),
            (&format!("{accents} accents"), SYN_TYPE),
            ("0 restarts", LEDGER),
        ],
        0.0,
        0.0,
        clamp01((sec - 1.4) / 0.6),
    ));
    stack.into()
}

pub(crate) fn the_inspector(ctx: &pf::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    camera(sec, PANE_ZONE, 0.5, 5.1);
    frame::headline("See what you built", clamp01((sec - 0.1) / 0.5));
    beats(
        sec,
        &[
            (0.0, "The Inspector: the real render tree, read live."),
            (
                2.8,
                "Every box, read straight out of the frame that drew it.",
            ),
            (5.6, "Semantics on — what a screen reader will be told."),
            (8.6, "Accessibility you can see, before anyone has to ask."),
        ],
    );
    let mut stack = Stack::new();
    let pa = clamp01((sec - 0.6) / 0.35) * (1.0 - clamp01((sec - 5.4) / 0.35));
    stack = stack.push(ring(app::PREVIEW, SYN_TYPE, pa));
    let fa = clamp01((sec - 5.8) / 0.35) * (1.0 - clamp01((sec - 11.0) / 0.35));
    stack = stack.push(ring(app::FULL.inflate(-6.0), SYN_STRING, fa));
    (frame::receipts(
        &[
            ("render tree, live", SYN_TYPE),
            ("semantics overlay", SYN_STRING),
            ("the studio's own views", ACCENT),
        ],
        0.0,
        0.0,
        clamp01((sec - 1.2) / 0.6),
    ));
    stack.into()
}
