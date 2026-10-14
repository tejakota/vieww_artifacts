//! C07 · THREE FRAMES — the platform switch. 3:18–3:30.
//!
//! The same compiled screen, moved between its real device frames:
//! **Android** — the phone's metrics, its safe areas, its own motion
//! feel; **iOS** — the notch and the insets; **Desktop** — the window.
//! `ThemeData::of(ctx)` reflects each platform's conventions into the
//! widgets without one `if platform ==` in the screen's source: the
//! platform switch is the studio's job, not the screen's. What the
//! frame showed you in A04 — three devices, three truths — is here
//! three devices, **one** truth: the same tree, laid out at each
//! frame's real size, wrapped wrong nowhere.

use vieww_foundation::TextAlign;
use vieww_widget::WidgetNode;

use super::{
    alpha, caption, chip_row, clamp01, studio_chrome, Ctx, ACCENT, LEDGER, MUTED, SYN_TYPE, W,
};

/// The platforms' arrival times (the script's own).
const ANDROID_AT: f32 = 200.5;
const IOS_AT: f32 = 203.0;
const DESKTOP_AT: f32 = 205.5;

pub(super) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;

    let mut stack = studio_chrome(ctx);

    // The headline — the claim, over the studio.
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
                        vieww_widget::Text::new("one screen, three truths reconciled")
                            .style(
                                super::geist(26.0)
                                    .letter_spacing(1.6)
                                    .color(alpha(super::INK, 0.95)),
                            )
                            .align(TextAlign::Center),
                    ),
                ),
        );
    }

    // The platform tracker — which frame is current, as a chip rail
    // that moves with the script's platform switches.
    let platforms = [
        ("android", ANDROID_AT, SYN_TYPE),
        ("ios", IOS_AT, LEDGER),
        ("desktop", DESKTOP_AT, ACCENT),
    ];
    let rail_a = clamp01((t - 0.10) / 0.1);
    if rail_a > 0.01 {
        let active = platforms
            .iter()
            .filter(|(_, at, _)| ctx.abs >= *at)
            .count()
            .max(1);
        let label = platforms
            .get(active - 1)
            .map(|(name, _, _)| *name)
            .unwrap_or("android");
        for (i, (name, at, color)) in platforms.iter().enumerate() {
            let lit = ctx.abs >= *at;
            let is_current = i + 1 == active;
            stack = stack.push(
                vieww_widget::Positioned::new()
                    .left(W * 0.5 - 240.0 + i as f32 * 170.0)
                    .top(240.0)
                    .width(160.0)
                    .height(36.0)
                    .child(
                        vieww_widget::Opacity::new(rail_a).child(
                            vieww_widget::Container::new()
                                .color(alpha(
                                    if is_current {
                                        super::WASH
                                    } else {
                                        super::SURFACE_2
                                    },
                                    0.9,
                                ))
                                .radius(9.0)
                                .border(vieww_foundation::Border::new(
                                    alpha(*color, if is_current { 0.6 } else { 0.2 }),
                                    if is_current { 1.8 } else { 1.0 },
                                ))
                                .padding(vieww_foundation::EdgeInsets::symmetric(7.0, 13.0))
                                .child(
                                    vieww_widget::Text::new(name.to_string())
                                        .style(super::geist_mono(14.0).letter_spacing(1.6).color(
                                            alpha(
                                                if lit { *color } else { MUTED },
                                                if is_current { 1.0 } else { 0.7 },
                                            ),
                                        ))
                                        .align(TextAlign::Center),
                                ),
                        ),
                    ),
            );
        }
        // The current-frame callout — the studio's own metrics line.
        let metrics = match label {
            "ios" => "393 × 852 · notch insets · ios conventions",
            "desktop" => "window-sized · desktop conventions",
            _ => "360 × 800 · gesture insets · android conventions",
        };
        if ctx.abs >= ANDROID_AT {
            let a = clamp01((ctx.abs - ANDROID_AT) / 0.3);
            stack = stack.push(
                vieww_widget::Positioned::new()
                    .left(0.0)
                    .top(296.0)
                    .width(W)
                    .height(28.0)
                    .child(
                        vieww_widget::Opacity::new(a).child(
                            vieww_widget::Text::new(metrics)
                                .style(
                                    super::geist_mono(15.0)
                                        .letter_spacing(1.4)
                                        .color(alpha(MUTED, 0.95)),
                                )
                                .align(TextAlign::Center),
                        ),
                    ),
            );
        }
    }

    // The captions — the drift act's answer.
    stack = stack.push(caption(
        "the frame changes the metrics, the insets, the platform's own feel",
        1002.0,
        clamp01((t - 0.05) / 0.12),
    ));
    stack = stack.push(caption(
        "not one `if platform ==` in the screen's source — the theme carries it",
        966.0,
        clamp01((t - 0.55) / 0.12),
    ));

    // The receipt chips.
    stack = stack.push(chip_row(
        &[
            ("a flex that overflows here, overflows there", LEDGER),
            ("a text that wraps here, wraps there", LEDGER),
        ],
        // The editor's empty lower band — see `receipt_row`. Pinned
        // right, these rows rendered straight through the preview
        // pane's description paragraph in every studio scene.
        360.0,
        924.0,
        clamp01((sec - 1.0) / 0.5),
    ));

    stack.into()
}
