//! Depth 3 — what actually happened.
//!
//! **The one screen where rails beat a mosaic outright**: the grouping *is* the
//! information. Proven, Not counted, Failed, Left alone — one mixed wall of all
//! four hides exactly what you came here to see, and no amount of per-tile
//! badging fixes that.

use std::rc::Rc;

use vieww::element::ScrollController;
use vieww::prelude::*;

use crate::model::{PhotoState, Screen};
use crate::state::VavltState;
use crate::theme::VavltTheme;
use crate::ui;

pub fn outcome(
    theme: &VavltTheme,
    state: &Rc<VavltState>,
    scroll: &ScrollController,
) -> WidgetNode {
    let colors = theme.colors;
    let result = state.result.get();

    let hero = state.photos.with(|photos| {
        let art = state
            .billboard_pick
            .get()
            .and_then(|i| photos.get(i))
            .cloned();
        ui::billboard(
            theme,
            ui::Height::Outcome.for_form(theme.form),
            art.as_ref(),
            false,
            children![
                ui::eyebrow(theme, "measured, net of vavlt overhead", None),
                ui::figure(theme, result.saved_label.clone(), 50.0, colors.fg),
                ui::meter(theme, result.pct, false),
                ui::lede(
                    theme,
                    format!(
                        "{} → {} · every figure proven on your own files",
                        result.before_label, result.after_label
                    )
                ),
            ],
        )
    });

    let mut body = children![
        hero,
        ui::inset(
            theme,
            20.0,
            Flex::row()
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .spacing(8.0)
                .children(children![
                    stat(
                        theme,
                        ui::icons::check(),
                        result.proven,
                        "proven by decode + compare",
                        colors.success_txt
                    ),
                    stat(
                        theme,
                        ui::icons::info(),
                        result.unqualified,
                        "unqualified — not counted",
                        colors.warning_txt
                    ),
                    stat(
                        theme,
                        ui::icons::cross(),
                        result.failed,
                        "failed, original untouched",
                        if result.failed > 0 {
                            colors.destructive_txt
                        } else {
                            colors.dim
                        }
                    ),
                ])
        ),
    ];

    // One rail per state, and each is skipped when empty — a heading over
    // nothing is worse than no heading.
    for (states, title, note) in [
        (
            vec![PhotoState::Proven, PhotoState::PixelLossless],
            "Proven",
            Some("Decoded back and compared byte for byte. The only basis for calling a file reversible."),
        ),
        (
            vec![PhotoState::Unqualified],
            "Not counted",
            Some("A ratio with no perceptual score is not a saving you can have, so it is excluded from the figure above."),
        ),
        (
            vec![PhotoState::Failed],
            "Failed",
            Some("Your original is untouched and remains the only copy."),
        ),
        (vec![PhotoState::Skipped], "Left alone", None),
    ] {
        let group = state.photos.with(|photos| {
            photos.iter().filter(|p| states.contains(&p.state)).cloned().collect::<Vec<_>>()
        });
        if group.is_empty() {
            continue;
        }
        let open = state.clone();
        let indices = state.photos.with(|photos| {
            photos
                .iter()
                .enumerate()
                .filter(|(_, p)| states.contains(&p.state))
                .map(|(i, _)| i)
                .collect::<Vec<_>>()
        });
        body.push(ui::section(
            theme,
            title,
            Some(format!("{} {}", group.len(), if group.len() == 1 { "file" } else { "files" })),
            note,
            ui::rail(
                theme,
                &group,
                scroll,
                Some(Rc::new(move |i| open.open_photo(indices.get(i).copied().unwrap_or(i)))),
            ),
        ));
    }

    body.push(ui::gap(160.0));

    let done = state.clone();
    let restore = state.clone();
    let action = ui::action_bar(
        theme,
        children![Flex::row().spacing(10.0).children(children![
            // "Done" takes no glyph on purpose. It is the button that
            // ends the flow and it should be the quiet one; two icons side
            // by side would make the pair read as equally weighted, and
            // they are not — one of these undoes a run.
            Flexible::expanded(1).child(ui::button(
                theme,
                "Done",
                ui::ButtonKind::Solid,
                true,
                move || { done.go(Screen::Root) }
            )),
            Flexible::expanded(1).child(ui::button_with(
                theme,
                "Restore",
                Some(ui::icons::restore()),
                ui::ButtonKind::Glass,
                true,
                move || restore.restore_all()
            )),
        ])],
    );

    Stack::new()
        .fit(StackFit::Expand)
        .children(children![
            ui::bleed_body(theme, scroll, body),
            Positioned::new()
                .left(0.0)
                .right(0.0)
                .bottom(0.0)
                .child(action),
        ])
        .into()
}

/// One of the three counts. A shape as well as a colour, so proven / unqualified
/// / failed survive the commonest form of colour blindness.
fn stat(theme: &VavltTheme, icon: IconData, figure: usize, label: &str, tint: Color) -> WidgetNode {
    Flexible::expanded(1)
        .child(
            Flex::column()
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min)
                .spacing(2.0)
                .children(children![
                    Flex::row()
                        .cross_axis_alignment(CrossAxisAlignment::Center)
                        .spacing(6.0)
                        .children(children![
                            Icon::new(icon).size(20.0).color(tint),
                            ui::figure(theme, figure.to_string(), 30.0, tint),
                        ]),
                    Text::new(label.to_string())
                        .style(theme.meta().weight(FontWeight::Regular).line_height(1.3)),
                ]),
        )
        .into()
}
