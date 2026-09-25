//! Tab 0 — the vavlt tab. A billboard, then rails for what you might do next.
//!
//! Before a pick the billboard has no photograph to show and must not invent
//! one: the app declares no media permission on any platform, so it genuinely
//! cannot see a picture it was not handed. `ui::billboard` renders its ghost
//! state — outlined slots — and the headline sits on those.

use std::rc::Rc;

use vieww::element::ScrollController;
use vieww::prelude::*;

use crate::model::Screen;
use crate::state::VavltState;
use crate::theme::VavltTheme;
use crate::ui;

pub fn vault(theme: &VavltTheme, state: &Rc<VavltState>, scroll: &ScrollController) -> WidgetNode {
    let colors = theme.colors;
    let has_run = state.has_run.get();
    let picking = state.picking.get();
    let height = ui::Height::Home.for_form(theme.form);

    let hero = state.photos.with(|photos| {
        let art = state.billboard_pick.get().and_then(|i| photos.get(i)).cloned();
        let lower = if has_run {
            let result = state.result.get();
            children![
                ui::eyebrow(theme, "proven saving", None),
                ui::figure(theme, state.lifetime_saved.get(), 58.0, colors.fg),
                ui::meter(theme, result.pct, false),
                ui::lede(
                    theme,
                    format!(
                        "{:.0}% of the {} you handed over · {} files · your originals are still on this device",
                        result.pct * 100.0,
                        result.before_label,
                        state.lifetime_files.get()
                    )
                ),
            ]
        } else {
            children![
                ui::eyebrow(theme, "local · no network", None),
                ui::figure(theme, "Get space back", 38.0, colors.fg),
                ui::figure(theme, "without losing anything", 38.0, colors.fg.with_alpha(128)),
                ui::gap(10.0),
                ui::lede(
                    theme,
                    "Hand vavlt some photos. It shows you exactly what it can save on those \
                     files, proves every claim, and does nothing until you say so."
                ),
            ]
        };
        ui::billboard(theme, height, art.as_ref(), false, lower)
    });

    let picker = state.clone();
    let mut body = children![
        hero,
        Padding::new(EdgeInsets::only(
            theme.metrics.gutter,
            16.0,
            theme.metrics.gutter,
            0.0
        ))
        .child(ui::button_with(
            theme,
            if picking {
                "Waiting for the picker…"
            } else {
                "Choose photos"
            },
            // Dropped while the picker is open: a photo glyph beside
            // "Waiting for the picker…" invites the tap already in flight.
            (!picking).then(ui::icons::photo),
            ui::ButtonKind::Solid,
            !picking,
            move || picker.pick(),
        )),
    ];

    let error = state.pick_error.get();
    if !error.is_empty() {
        body.push(
            Padding::new(EdgeInsets::only(
                theme.metrics.gutter,
                14.0,
                theme.metrics.gutter,
                0.0,
            ))
            .child(ui::tinted_card(
                theme,
                colors.destructive_tint,
                error,
                colors.destructive_txt,
            ))
            .into(),
        );
    }

    // A rail per thing you might do next, and each is a *group* rather than a
    // page of the same set — which is the one shape a rail is right for.
    let waiting = state
        .photos
        .with(|p| p.iter().filter(|x| x.included).cloned().collect::<Vec<_>>());
    if !waiting.is_empty() {
        let go = state.clone();
        body.push(ui::section(
            theme,
            "Waiting to be planned",
            Some(format!("{} photos", waiting.len())),
            None,
            ui::rail(
                theme,
                &waiting,
                scroll,
                Some(Rc::new(move |_| go.go(Screen::Plan))),
            ),
        ));
    }

    if has_run {
        let proven = state.photos.with(|p| {
            p.iter()
                .filter(|x| x.state.has_result())
                .cloned()
                .collect::<Vec<_>>()
        });
        if !proven.is_empty() {
            let go = state.clone();
            body.push(ui::section(
                theme,
                "The last run",
                Some(format!("{} files", proven.len())),
                None,
                ui::rail(
                    theme,
                    &proven,
                    scroll,
                    Some(Rc::new(move |_| go.go(Screen::Outcome))),
                ),
            ));
        }
    }

    body.push(note(
        theme,
        ui::icons::shield(),
        "vavlt declares no media permission and no network permission, on any platform. It \
         cannot see a photo you did not hand it, and it cannot send one anywhere.",
    ));
    body.push(note(
        theme,
        ui::icons::info(),
        "This build measures and verifies but does not store yet: nothing is written, moved or \
         deleted. Every saving shown was proven by encoding your file and decoding it back.",
    ));
    body.push(ui::gap(110.0));

    ui::bleed_body(theme, scroll, body)
}

/// An icon and a paragraph, aligned at the cap height rather than the box.
pub fn note(theme: &VavltTheme, icon: IconData, text: &str) -> WidgetNode {
    Padding::new(EdgeInsets::only(
        theme.metrics.gutter,
        20.0,
        theme.metrics.gutter,
        0.0,
    ))
    .child(
        Flex::row()
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .spacing(12.0)
            .children(children![
                Padding::new(EdgeInsets::only(0.0, 1.0, 0.0, 0.0))
                    .child(Icon::new(icon).size(19.0).color(theme.colors.dim)),
                Flexible::expanded(1).child(ui::caption(theme, text.to_string())),
            ]),
    )
    .into()
}
