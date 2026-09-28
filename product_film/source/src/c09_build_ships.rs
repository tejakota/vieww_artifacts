//! C09 · BUILD SHIPS — from the studio onto the devices. 3:40–3:52.
//!
//! The last studio beat: the **Build and Run** side. The film opens the
//! toolchain view (the compilers the studio carries), then the export
//! view — and the package lands: Desktop, Windows, an Android `.apk`,
//! an iOS build waiting for its Mac. The devices list arrives with
//! each row's honest note: the `.apk` is debug-signed and says so; iOS
//! needs a Mac and says *that* too. The studio carries its own
//! compiler — **you do not need Rust installed** — which is the whole
//! reason this list can be honest: host and guest are one compilation
//! by construction.
//!
//! The studio act ends here. What the film owes now is proof.

use vieww_foundation::TextAlign;
use vieww_widget::prelude::*;
use vieww_widget::WidgetNode;

use super::{
    ACCENT, Ctx, INK, LEDGER, MUTED, SYN_FUNCTION, SYN_TYPE, W, alpha, caption, chip_row,
    clamp01, studio_chrome, xywh,
};

/// The export view's arrival (the script's own).
const EXPORT_AT: f32 = 223.3;
/// The devices list's landing.
const DEVICES_AT: f32 = 228.8;

/// The export rows — the studio's own packaging table, quoted.
const TARGETS: [(&str, &str); 4] = [
    ("Desktop", "this machine's binary"),
    ("Windows", "an .exe, cross-compiled"),
    ("Android", "debug-signed .apk — named as such"),
    ("iOS", "requires a Mac — Apple's rule, said plainly"),
];

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;

    let mut stack = studio_chrome(ctx);

    // The headline.
    let head_a = clamp01((t - 0.04) / 0.12);
    if head_a > 0.01 {
        stack = stack.push(
            vieww_widget::Positioned::new()
                .left(0.0)
                .top(186.0)
                .width(W)
                .height(40.0)
                .child(
                    vieww_widget::Opacity::new(head_a).child(
                        vieww_widget::Text::new("from the studio, onto the devices")
                            .style(super::geist(26.0).letter_spacing(1.6).color(alpha(INK, 0.95)))
                            .align(TextAlign::Center),
                    ),
                ),
        );
    }

    // The toolchain claim — the studio carries its own compiler.
    let tool_a = clamp01((t - 0.15) / 0.14);
    if tool_a > 0.01 {
        stack = stack.push(
            vieww_widget::Positioned::new()
                .left(0.0)
                .top(240.0)
                .width(W)
                .height(30.0)
                .child(
                    vieww_widget::Opacity::new(tool_a).child(
                        vieww_widget::Text::new(
                            "the bundle carries the rustc it was built by — you do not need Rust installed",
                        )
                        .style(super::geist_mono(15.0).letter_spacing(1.3).color(alpha(SYN_TYPE, 0.95)))
                        .align(TextAlign::Center),
                    ),
                ),
        );
    }

    // The export rows — arriving with the export view, staggered, as
    // receipt rows over the right pane.
    let rows_a = clamp01((ctx.abs - EXPORT_AT) / 0.4);
    if rows_a > 0.01 {
        for (i, (name, note)) in TARGETS.iter().enumerate() {
            let row_a = clamp01((rows_a - i as f32 * 0.12) / 0.5);
            if row_a <= 0.01 {
                continue;
            }
            let rise = (1.0 - crate::film_lib::ease_out_cubic(row_a)) * 12.0;
            stack = stack.push(
                vieww_widget::Positioned::new()
                    .left(W - 700.0)
                    .top(300.0 + i as f32 * 52.0 + rise)
                    .width(640.0)
                    .height(44.0)
                    .child(
                        vieww_widget::Opacity::new(row_a).child(Painting::sized(
                            Size::new(640.0, 44.0),
                            PaintWith::new(move |book: &mut vieww_foundation::Sketchbook, _s: Size| {
                                book.rrect(xywh(0.0, 0.0, 640.0, 40.0), 8.0, alpha(super::SURFACE_2, 0.92));
                                book.stroke_rrect(xywh(0.0, 0.0, 640.0, 40.0), 8.0, alpha(LEDGER, 0.25), 1.1);
                                // The row's status dot.
                                book.circle(Offset::new(24.0, 20.0), 5.0, alpha(LEDGER, 0.85));
                            }),
                        )),
                    ),
            );
            stack = stack.push(
                vieww_widget::Positioned::new()
                    .left(W - 700.0 + 44.0)
                    .top(308.0 + i as f32 * 52.0 + rise)
                    .width(150.0)
                    .height(24.0)
                    .child(
                        vieww_widget::Opacity::new(row_a).child(
                            vieww_widget::Text::new(name.to_string())
                                .style(super::geist_mono(15.0).letter_spacing(1.4).color(alpha(INK, 0.95)))
                                .align(TextAlign::Left),
                        ),
                    ),
            );
            stack = stack.push(
                vieww_widget::Positioned::new()
                    .left(W - 700.0 + 190.0)
                    .top(310.0 + i as f32 * 52.0 + rise)
                    .width(440.0)
                    .height(22.0)
                    .child(
                        vieww_widget::Opacity::new(row_a * 0.9).child(
                            vieww_widget::Text::new(note.to_string())
                                .style(super::geist_mono(12.5).letter_spacing(1.0).color(alpha(MUTED, 0.9)))
                                .align(TextAlign::Left),
                        ),
                    ),
            );
        }
    }

    // The devices-landed callout.
    if ctx.abs >= DEVICES_AT {
        let a = clamp01((ctx.abs - DEVICES_AT) / 0.3);
        stack = stack.push(
            vieww_widget::Positioned::new()
                .left(0.0)
                .top(560.0)
                .width(W)
                .height(30.0)
                .child(
                    vieww_widget::Opacity::new(a).child(
                        vieww_widget::Text::new("the package lands — four targets, honest labels, no secrets held")
                            .style(super::geist_mono(16.0).letter_spacing(1.5).color(alpha(LEDGER, 0.95)))
                            .align(TextAlign::Center),
                    ),
                ),
        );
    }

    // The captions.
    stack = stack.push(caption(
        "build and run: the whole project, compiled as the real thing",
        1002.0,
        clamp01((t - 0.05) / 0.12),
    ));
    stack = stack.push(caption(
        "an apk installs with developer mode on — and says exactly that",
        966.0,
        clamp01((t - 0.6) / 0.12),
    ));

    // The receipt chips.
    stack = stack.push(chip_row(
        &[
            ("4 targets", SYN_FUNCTION),
            ("1 toolchain, carried", SYN_TYPE),
            ("0 secrets held", ACCENT),
        ],
        W - 640.0,
        936.0,
        clamp01((sec - 1.2) / 0.5),
    ));

    stack.into()
}
