//! S12 · FIRST PAINT — the hero moment: the live preview opens, and a
//! keystroke repaints the screen with no build. 1:37–1:49.
//!
//! The script runs the real command — `LivePreview` — and the studio
//! answers with its real caution dialog (honest software warns before
//! it improvises). Accept, and the inbox demo mounts in the device
//! frame, **uncompiled** — the studio's live preview, parsing live.rs
//! and drawing it. Then the film types one real edit into the buffer,
//! and the preview repaints in the same breath: no save, no build, no
//! reload. The receipt — **alive in N seconds** — is the census's own
//! probe of exactly this keystroke, edit to pixels, at master
//! resolution, through the real studio.
//!
//! Witness taps 1 (the demo mounts, 103.2 s) and 2 (the keystroke,
//! 106.4 s) fire here.

use super::{
    caption, chip, clamp01, glow, spring_out, studio_chrome, tint, Ctx, ACCENT, MINT, SYN_TYPE,
    VIOLET_SOFT,
};
use vieww_widget::WidgetNode;

/// The caution dialog's beat (scene fraction ≈ 102.0–103.2 s → 0.42–0.52).
const CAUTION_T: f32 = 0.42;
/// The acceptance — tap 1.
const ACCEPT_T: f32 = 0.52;
/// The keystroke — tap 2.
const EDIT_T: f32 = 0.78;

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;

    let mut stack = studio_chrome(ctx);

    // The spark — arrives with the demo and takes its post beside the
    // preview pane's top: the light is IN the product now. It blooms at
    // the acceptance and breathes through the edit.
    let spark_a = clamp01((t - ACCEPT_T) / 0.10);
    if spark_a > 0.02 {
        let sx = 1780.0;
        let sy = 150.0;
        let orb_abs = ctx.abs;
        let orb = super::Painting::sized(
            super::Size::new(120.0, 120.0),
            super::PaintWith::new(move |book: &mut super::Sketchbook, _s: super::Size| {
                super::spark(book, 60.0, 60.0, 11.0, orb_abs, spark_a, ACCENT);
            }),
        );
        stack = stack.push(
            super::Positioned::new()
                .left(sx - 60.0)
                .top(sy - 60.0)
                .width(120.0)
                .height(120.0)
                .child(orb),
        );
    }

    // The first-paint bloom — a soft light over the preview pane's area
    // at the acceptance beat (the film annotating the moment; the
    // product's pixels beneath are its own).
    let paint = clamp01((t - ACCEPT_T) / 0.06);
    let paint_fade = clamp01((t - (ACCEPT_T + 0.10)) / 0.30);
    if paint > 0.0 && paint_fade < 1.0 {
        let a = paint * (1.0 - paint_fade);
        let bloom = super::Painting::sized(
            super::CANVAS,
            super::PaintWith::new(move |book: &mut super::Sketchbook, s: super::Size| {
                let _ = s;
                glow(book, 1440.0, 520.0, 420.0, ACCENT, 0.14 * a);
            }),
        );
        stack = stack.push(super::Positioned::fill().child(bloom));
    }

    // The alive receipt — pops after the keystroke lands: the census's
    // own measurement of this exact edit, edit to pixels.
    if t > EDIT_T + 0.04 {
        let pop = spring_out(clamp01((t - EDIT_T - 0.04) / 0.30), 10.0, 0.55);
        let a = clamp01((t - (EDIT_T + 0.08)) / 0.10);
        let rise = (1.0 - pop) * 26.0;
        let text = if ctx.probe.alive_seconds > 0.0 {
            format!("alive in {:.3} s", ctx.probe.alive_seconds)
        } else {
            "alive in — s".to_string()
        };
        let y = 860.0 + rise - 120.0;
        stack = stack.push(
            super::Positioned::new()
                .left(1010.0)
                .top(y)
                .width(360.0)
                .height(40.0)
                .child(super::Opacity::new(a.max(0.01)).child(chip(text, 16.0, tint(MINT, 0.1)))),
        );
        stack = stack.push(
            super::Positioned::new()
                .left(1010.0)
                .top(y + 44.0)
                .width(430.0)
                .height(22.0)
                .child(super::Opacity::new(a.max(0.01)).child(super::mono_tracked(
                    "the census's probe: edit → pixels, this bench",
                    12.0,
                    super::MUTED,
                    1.0,
                ))),
        );
    }

    // The captions — the hero moment's beats.
    stack = stack.push(caption(
        "run: live preview — the studio asks before it improvises",
        1002.0,
        clamp01((t - (CAUTION_T - 0.06)) / 0.10),
    ));
    if t > ACCEPT_T + 0.04 {
        stack = stack.push(caption(
            "accept — the demo mounts. no compile: the studio draws live.rs",
            966.0,
            clamp01((t - ACCEPT_T - 0.04) / 0.10),
        ));
    }
    if t > EDIT_T + 0.02 {
        stack = stack.push(caption(
            "and a keystroke repaints it — no save, no build, no reload",
            930.0,
            clamp01((t - EDIT_T - 0.02) / 0.10),
        ));
    }

    // The command receipt — what ran, staggered.
    stack = stack.push(super::receipt_row(
        &[
            ("command: LivePreview", VIOLET_SOFT),
            ("the real caution dialog", SYN_TYPE),
            ("the real inbox demo", ACCENT),
        ],
        clamp01((sec - (CAUTION_T * 12.0 + 2.0)) / 0.5),
    ));

    stack.into()
}
