//! S16 · THE SCREENS — another real compile, then the platform frames.
//! 2:20–2:30.
//!
//! The film switches to `card_grid.rs` and renders it — the same real
//! pipeline, a different screen: the card grid compiles, mounts, and
//! lives in the preview. Then the platform simulation: Android, iOS,
//! desktop — one preview, three frames, the studio's own device
//! dressing (safe areas, platform control shapes, appearance).

use super::{caption, clamp01, studio_chrome, Ctx, ACCENT, SYN_TYPE};
use vieww_widget::WidgetNode;

/// The script's moments, absolute film seconds.
const TAB_T: f32 = 140.5;
const RENDER_T: f32 = 142.0;
const ANDROID_T: f32 = 144.5;
const IOS_T: f32 = 146.5;
const DESKTOP_T: f32 = 148.5;

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let _t = ctx.t;
    let abs = ctx.abs;

    let mut stack = studio_chrome(ctx);

    // The platform badge — names the frame as the studio wears it.
    let platform = if abs >= DESKTOP_T {
        Some(("desktop", ACCENT))
    } else if abs >= IOS_T {
        Some(("ios", SYN_TYPE))
    } else if abs >= ANDROID_T {
        Some(("android", super::SYN_STRING))
    } else {
        None
    };
    if let Some((name, color)) = platform {
        let pop = clamp01(
            (abs - (if abs >= DESKTOP_T {
                DESKTOP_T
            } else if abs >= IOS_T {
                IOS_T
            } else {
                ANDROID_T
            }) - 0.0)
                / 0.18,
        );
        let badge = super::Painting::sized(
            super::Size::new(220.0, 46.0),
            super::PaintWith::new(move |book: &mut super::Sketchbook, _s: super::Size| {
                book.rrect(
                    super::xywh(0.0, 2.0, 220.0, 40.0),
                    9.0,
                    super::alpha(super::SURFACE_2, 0.94),
                );
                book.stroke_rrect(
                    super::xywh(0.0, 2.0, 220.0, 40.0),
                    9.0,
                    super::alpha(color, 0.35),
                    1.2,
                );
                // The frame's notch — a small mark of the platform.
                book.rrect(
                    super::xywh(14.0, 16.0, 12.0, 12.0),
                    3.0,
                    super::alpha(color, 0.8),
                );
            }),
        );
        stack = stack.push(
            super::Positioned::new()
                .left(1010.0)
                .top(852.0 - (1.0 - pop) * 10.0)
                .width(220.0)
                .height(46.0)
                .child(badge),
        );
        stack = stack.push(
            super::Positioned::new()
                .left(1036.0)
                .top(860.0 - (1.0 - pop) * 10.0)
                .width(180.0)
                .height(30.0)
                .child(
                    vieww_widget::Text::new(name)
                        .style(
                            super::geist_mono(16.0)
                                .letter_spacing(2.0)
                                .color(super::alpha(color, 1.0)),
                        )
                        .align(vieww_foundation::TextAlign::Left),
                ),
        );
    }

    // The captions — the screens' beats.
    stack = stack.push(caption(
        "the card grid — a different screen, the same real compile",
        1002.0,
        clamp01((abs - TAB_T) / 0.25),
    ));
    if abs >= RENDER_T + 0.5 {
        stack = stack.push(caption(
            "and one preview, three platform frames — android · ios · desktop",
            966.0,
            clamp01((abs - RENDER_T - 0.5) / 0.25),
        ));
    }
    if abs >= ANDROID_T + 0.3 {
        stack = stack.push(caption(
            "safe areas, control shapes, appearance — the studio simulates each",
            930.0,
            clamp01((abs - ANDROID_T - 0.3) / 0.2),
        ));
    }

    stack.into()
}
