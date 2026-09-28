//! Movement III · THE STUDIO — the actual `viewwstudio`, doing real
//! work, with this film's voice annotating it.
//!
//! The studio's pixels come from the mounted app (`script::mount`);
//! these scenes contribute **only the overlay**: the headline, the
//! beat chips, the tap rings, the receipts. The honesty rule holds —
//! nothing here grades the app, and every chip describes what the
//! session is actually doing at that moment (see `super::script`).

use vieww_foundation::{Offset, Size, Sketchbook};
use vieww_widget::prelude::*;

use crate::film_lib::{ease_out_cubic, ease_out_expo};
use crate::product_film as pf;
use crate::product_film::script::{TAP_ADD_ONE, TAP_LIVE_ROW};
use super::{ACCENT, CANVAS, ENGINE, INK, LEDGER, MUTED, SYN_MACRO, SYN_STRING, SYN_TYPE};

/// The studio act's headline — one line, Geist, centered the way the
/// house centres: a full-width box and `TextAlign::Center`, so the
/// words land where the box says and never where a flex wants them.
fn headline(text: &str, sub: &str, ctx: &pf::Ctx) -> vieww_widget::WidgetNode {
    let a = ease_out_expo(pf::clamp01((ctx.t - 0.04) / 0.10));
    if a <= 0.01 {
        return Stack::new().into();
    }
    Stack::new()
        .push(
            Positioned::new()
                .left(0.0)
                .top(0.0)
                .width(pf::W)
                .height(52.0)
                .child(
                    Text::new(text.to_string())
                        .style(pf::geist(40.0).bold().letter_spacing(2.4).color(pf::alpha(INK, 0.97 * a)))
                        .align(TextAlign::Center),
                ),
        )
        .push(
            Positioned::new()
                .left(0.0)
                .top(58.0)
                .width(pf::W)
                .height(26.0)
                .child(
                    Text::new(sub.to_string())
                        .style(pf::geist_mono(15.0).letter_spacing(2.0).color(pf::alpha(ENGINE, 0.9 * a)))
                        .align(TextAlign::Center),
                ),
        )
        .into()
}

/// The headline plate — centered, on the band the studio scenes share.
fn headline_plate(ctx: &pf::Ctx, text: &str, sub: &str) -> vieww_widget::WidgetNode {
    pf::chrome(Stack::new().push(
        Positioned::new()
            .left(0.0)
            .top(pf::BAND_H + 40.0 + (1.0 - ease_out_expo(pf::clamp01((ctx.t - 0.04) / 0.10))) * 14.0)
            .width(pf::W)
            .height(120.0)
            .child(headline(text, sub, ctx)),
    ).into())
}

/// A beat chip — a mono chip that fires at one session beat, with its
/// own landing envelope.
fn beat_chip(text: &str, color: vieww_foundation::Color, x: f32, y: f32, since: f32) -> vieww_widget::WidgetNode {
    if since <= 0.0 {
        return Stack::new().into();
    }
    let a = ease_out_cubic(pf::clamp01(since / 0.24));
    let rise = (1.0 - a) * 10.0;
    pf::chrome(Stack::new().push(
        Positioned::new()
            .left(x)
            .top(y + rise)
            .width(pf::gmono_tw(14.0, text.chars().count(), 1.1) + pf::CHIP_PAD_X * 2.0 + 2.0)
            .height(30.0)
            .child(Opacity::new(a).child(pf::chip(text, 14.0, color))),
    ).into())
}

// ── Z07 · studio_opens ──────────────────────────────────────────────────────

pub fn studio_opens(ctx: &pf::Ctx) -> vieww_widget::WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;
    let mut stack = super::studio_plate(ctx, "MOVEMENT III", "THE STUDIO");

    // The headline — the app's name and the film's claim about it.
    pf::chrome(Stack::new().push(
        Positioned::new()
            .left(0.0)
            .top(pf::BAND_H + 40.0 + (1.0 - ease_out_expo(pf::clamp01((t - 0.04) / 0.10))) * 14.0)
            .width(pf::W)
            .height(130.0)
            .child(headline("viewwstudio", "the actual app — not a mock", ctx)),
    ).into());

    // The beats, exactly as the session plays them.
    beat_chip(
        "open: counter.say",
        SYN_TYPE,
        360.0,
        330.0,
        abs - 56.4,
    );
    beat_chip("panel closes — the editor breathes", MUTED, 360.0, 372.0, abs - 60.0);
    beat_chip("panel returns — the tree is the map", MUTED, 360.0, 414.0, abs - 64.0);

    // The receipts.
    pf::chrome(pf::chip_row(
        &[
            ("the real Shell", ACCENT),
            ("the real editor", SYN_TYPE),
            ("the real preview", SYN_STRING),
        ],
        360.0,
        924.0,
        pf::clamp01((t - 0.30) / 0.20),
    ));

    pf::caption("one shell: editor, preview, build, ship.", 1002.0, pf::clamp01((t - 0.14) / 0.12));
    pf::caption("every pixel below this line is vieww's.", 966.0, pf::clamp01((t - 0.48) / 0.12));
    stack.into()
}

// ── Z08 · live_compose ──────────────────────────────────────────────────────

pub fn live_compose(ctx: &pf::Ctx) -> vieww_widget::WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;
    let mut stack = super::studio_plate(ctx, "MOVEMENT III", "THE STUDIO");

    headline_plate(ctx, "edit a line — see the picture change", "the preview is the tree, not a texture");

    // The beats — the session's own times, one chip each.
    beat_chip("live preview: accepted", SYN_STRING, 360.0, 330.0, abs - 69.2);
    beat_chip("damage overlay: on", pf::BREAK_RED, 360.0, 372.0, abs - 71.0);
    beat_chip("edit: title → “My own inbox”", ACCENT, 360.0, 414.0, abs - 73.0);
    beat_chip("edit: row → “Shipped the beta today”", ACCENT, 360.0, 456.0, abs - 75.6);
    beat_chip("damage overlay: off", MUTED, 360.0, 498.0, abs - 76.8);

    // The receipts — what one edit cost, as the app itself reported it.
    pf::chrome(pf::chip_row(
        &[
            ("damage: one rect", ACCENT),
            ("rebuild: one subtree", SYN_TYPE),
            ("budget: kept", LEDGER),
        ],
        360.0,
        924.0,
        pf::clamp01((t - 0.55) / 0.20),
    ));

    pf::caption("no relaunch. no reload. the line lands while you watch.", 1002.0, pf::clamp01((t - 0.16) / 0.12));
    pf::caption("what you see repaint itself is exactly what the planner repainted.", 966.0, pf::clamp01((t - 0.60) / 0.12));
    stack.into()
}

// ── Z09 · say_to_rust ───────────────────────────────────────────────────────

pub fn say_to_rust(ctx: &pf::Ctx) -> vieww_widget::WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;
    let mut stack = super::studio_plate(ctx, "MOVEMENT III", "THE STUDIO");

    headline_plate(ctx, ".say → rust → cdylib → preview", "one file of english — a real widget tree");

    // The pipeline beats, one per stage of the real compile.
    beat_chip("say-codegen: parses the program", SYN_MACRO, 360.0, 330.0, abs - 78.0);
    beat_chip("rustc: compiles the generated rust", SYN_TYPE, 360.0, 372.0, abs - 83.2);
    beat_chip("dlopen: the preview loads the cdylib", pf::SYN_FUNCTION, 360.0, 414.0, abs - 83.4);
    beat_chip("pixels: the screen is the counter", SYN_STRING, 360.0, 456.0, abs - 84.2);

    // The two real taps — the film's annotation of real input.
    for at in [85.6, 87.2] {
        let since = abs - at;
        if (0.0..=1.0).contains(&since) {
            stack = stack.push(pf::tap_ring_at(TAP_ADD_ONE, since));
        }
    }

    // The receipts.
    pf::chrome(pf::chip_row(
        &[
            ("counter.say → counter.rs", SYN_MACRO),
            ("rustc: settled off-clock", MUTED),
            ("2 taps, real input", ACCENT),
        ],
        360.0,
        924.0,
        pf::clamp01((t - 0.62) / 0.20),
    ));

    pf::caption("the language compiles to the framework — no bridge, no interpreter.", 1002.0, pf::clamp01((t - 0.14) / 0.12));
    pf::caption("and the counter counts, because the taps are real.", 966.0, pf::clamp01((t - 0.66) / 0.12));
    stack.into()
}

// ── Z10 · ships_everywhere ──────────────────────────────────────────────────

pub fn ships_everywhere(ctx: &pf::Ctx) -> vieww_widget::WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;
    let mut stack = super::studio_plate(ctx, "MOVEMENT III", "THE STUDIO");

    headline_plate(ctx, "one tree. every device.", "platforms and tokens, not forks");

    // The platform flips — three chips lighting as each lands.
    beat_chip("android — the frame re-chromes", SYN_TYPE, 360.0, 330.0, abs - 89.6);
    beat_chip("ios — same tree, new metrics", SYN_TYPE, 360.0, 372.0, abs - 91.2);
    beat_chip("desktop — same tree, a window", SYN_TYPE, 360.0, 414.0, abs - 92.8);
    beat_chip("tokens: the accent is data", LEDGER, 360.0, 456.0, abs - 94.4);
    beat_chip("teal → purple — the studio re-tints, live", ACCENT, 360.0, 498.0, abs - 95.6);

    // The accent swatch — the token flip, drawn as it happens.
    let teal_at = 95.6;
    let purple_at = 96.8;
    let swatch = if abs >= purple_at {
        ACCENT
    } else if abs >= teal_at {
        vieww_foundation::Color::rgb(0x2F, 0xBF, 0xAE)
    } else {
        vieww_foundation::Color::rgb(0x46, 0x4E, 0x5E)
    };
    let sw_a = ease_out_cubic(pf::clamp01((t - 0.72) / 0.14));
    if sw_a > 0.01 {
        stack = stack.push(Positioned::new().left(1420.0).top(320.0).width(300.0).height(200.0).child(
            Painting::sized(Size::new(300.0, 200.0), PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                book.rrect(pf::xywh(40.0, 20.0, 220.0, 68.0), 16.0, pf::alpha(swatch, sw_a));
                book.stroke_rrect(pf::xywh(40.0, 20.0, 220.0, 68.0), 16.0, pf::alpha(vieww_foundation::Color::WHITE, 0.10 * sw_a), 1.0);
                book.rrect(pf::xywh(40.0, 108.0, 220.0, 12.0), 6.0, pf::alpha(swatch, 0.4 * sw_a));
                book.rrect(pf::xywh(40.0, 132.0, 150.0, 12.0), 6.0, pf::alpha(swatch, 0.25 * sw_a));
            })),
        ));
    }

    // The receipts.
    pf::chrome(pf::chip_row(
        &[
            ("3 platforms, 0 forks", ACCENT),
            ("tokens: 1 line to a new brand", LEDGER),
        ],
        360.0,
        924.0,
        pf::clamp01((t - 0.70) / 0.20),
    ));

    pf::caption("the same widget tree — re-framed, not re-written.", 1002.0, pf::clamp01((t - 0.14) / 0.12));
    pf::caption("a brand is a token file, and the studio edits tokens.", 966.0, pf::clamp01((t - 0.62) / 0.12));
    stack.into()
}

/// Silence the lint until the studio act's next pass.
#[allow(unused)]
fn _unused() {
    let _ = (TAP_LIVE_ROW, CANVAS);
}
