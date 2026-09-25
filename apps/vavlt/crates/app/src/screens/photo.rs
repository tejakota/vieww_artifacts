//! Depth 4 — one photograph's proof.
//!
//! Reachable from any tile. It exists so that "proven" is a claim you can open
//! and inspect rather than a badge you have to accept: the picture, what ran,
//! what came back, and what was compared against what.

use std::rc::Rc;

use vieww::element::ScrollController;
use vieww::prelude::*;

use crate::model::{Photo, PhotoState, Screen};
use crate::state::VavltState;
use crate::theme::VavltTheme;
use crate::ui;

pub fn photo(theme: &VavltTheme, state: &Rc<VavltState>, scroll: &ScrollController) -> WidgetNode {
    let index = state.detail.get();
    let Some(photo) = state.photos.with(|photos| photos.get(index).cloned()) else {
        // The selection was cleared while this screen was up. An empty state
        // rather than a panic — the index is a signal and signals race.
        return ui::status_page(
            theme,
            icons::close(),
            "Nothing to show",
            "That photo is no longer part of the selection.",
        );
    };

    let m = theme.metrics;
    let mut body = children![];

    // The picture is the point of this screen, so it gets a full billboard —
    // the chips and the name ride over the fade rather than sitting under a
    // framed thumbnail.
    body.push(ui::billboard(
        theme,
        ui::Height::Proof.for_form(theme.form),
        Some(&photo),
        false,
        children![
            Flex::row()
                .main_axis_size(MainAxisSize::Min)
                .spacing(m.sp_1)
                .children(children![
                    ui::chip(theme, photo.state.label(), tone_for(photo.state)),
                    ui::chip(theme, photo.class_label.clone(), ui::ChipTone::Neutral),
                ]),
            ui::gap(10.0),
            ui::figure(theme, photo.name.clone(), 32.0, theme.colors.fg),
            ui::gap(8.0),
            ui::lede(theme, photo.state.explanation().to_string()),
        ],
    ));

    // --- the arithmetic ---------------------------------------------------
    let saved_tint = if photo.state == PhotoState::Failed {
        theme.colors.destructive_txt
    } else {
        theme.colors.success_txt
    };

    // Rows against hairlines rather than a card. No boxes on this screen.
    body.push(ui::inset(
        theme,
        20.0,
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min)
            .children(children![
                measure_line(theme, "before", &photo.size_label, theme.colors.fg),
                ui::divider(theme),
                measure_line(
                    theme,
                    "after",
                    &or_dash(&photo.after_label),
                    theme.colors.fg
                ),
                ui::divider(theme),
                measure_line(theme, "saved", &or_dash(&photo.pct_label), saved_tint),
                ui::divider(theme),
                measure_line(
                    theme,
                    "codec",
                    &or_dash(&photo.class_label),
                    theme.colors.fg_2
                ),
            ]),
    ));

    if !photo.note.is_empty() {
        body.push(ui::inset(
            theme,
            16.0,
            ui::tinted_card(
                theme,
                theme.colors.warning_tint,
                photo.note.clone(),
                theme.colors.warning_txt,
            ),
        ));
    }
    body.push(ui::gap(150.0));

    let back = state.clone();
    let action = ui::action_bar(
        theme,
        children![ui::button_with(
            theme,
            "Back to the run",
            Some(ui::icons::back()),
            ui::ButtonKind::Glass,
            true,
            move || back.go(Screen::Outcome)
        )],
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

fn tone_for(state: PhotoState) -> ui::ChipTone {
    match state {
        PhotoState::Proven | PhotoState::PixelLossless => ui::ChipTone::Accent,
        PhotoState::Failed => ui::ChipTone::Destructive,
        PhotoState::Unqualified => ui::ChipTone::Warning,
        _ => ui::ChipTone::Neutral,
    }
}

/// An em dash beats an empty cell: a blank reads as a missing row rather than
/// as a value that does not exist yet.
fn or_dash(value: &str) -> String {
    if value.is_empty() {
        "—".to_string()
    } else {
        value.to_string()
    }
}

fn measure_line(theme: &VavltTheme, label: &str, figure: &str, tint: Color) -> WidgetNode {
    Padding::new(EdgeInsets::symmetric(0.0, 15.0))
        .child(
            Flex::row()
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .children(children![
                    Flexible::expanded(1)
                        .child(Text::new(label.to_string()).style(theme.caption())),
                    Text::new(figure.to_string())
                        .style(crate::theme::Numeric::style(theme.metrics.t_body, tint)),
                ]),
        )
        .into()
}

/// Silences the unused-import warning when `Photo` is only named in a doc.
#[allow(dead_code)]
fn _photo_type(_: &Photo) {}
