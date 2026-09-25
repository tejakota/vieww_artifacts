//! Depth 1 — the plan. One screen, one decision.
//!
//! The figure sits on the photographs it is about; the choice that changes it
//! sits directly under; then the photographs themselves, full size and every
//! one of them tappable.
//!
//! **The mosaic is inline and not a rail, deliberately.** Excluding a photo is
//! *the* task here, and a rail showing four of a hundred puts the task behind a
//! scroll — that was a real regression in an earlier pass of this design.
//! Rails survive on this screen only for "Never re-encoded", where a two-item
//! list is the whole content.

use std::rc::Rc;

use vavlt_core::Tier;
use vieww::element::ScrollController;
use vieww::prelude::*;

use crate::state::{VavltState, RETENTION_DAYS};
use crate::theme::VavltTheme;
use crate::ui;

pub fn plan(theme: &VavltTheme, state: &Rc<VavltState>, scroll: &ScrollController) -> WidgetNode {
    let colors = theme.colors;
    let m = theme.metrics;
    let tier = state.tier.get();
    let deep = tier != Tier::Move;
    let estimate = state.estimate.get();
    let included = state.included_count();
    let excluded = state.excluded_count();

    let ratio = estimate.selected_ratio;

    let hero = state.photos.with(|photos| {
        let art = state
            .billboard_pick
            .get()
            .and_then(|i| photos.get(i))
            .cloned();
        ui::billboard(
            theme,
            ui::Height::Plan.for_form(theme.form),
            art.as_ref(),
            deep,
            children![
                ui::eyebrow(
                    theme,
                    if deep {
                        "irreversible after 14 days"
                    } else {
                        "you would get back"
                    },
                    deep.then_some(colors.destructive_txt)
                ),
                ui::figure(
                    theme,
                    estimate.selected_label.clone(),
                    50.0,
                    if deep {
                        colors.destructive_txt
                    } else {
                        colors.fg
                    }
                ),
                ui::meter(theme, ratio, deep),
                ui::lede(
                    theme,
                    format!(
                        "{included} photos · {} handed over{}",
                        state.total_label(),
                        if excluded > 0 {
                            format!(" · {excluded} left out")
                        } else {
                            String::new()
                        }
                    )
                ),
            ],
        )
    });

    let set_tier = state.clone();
    let mut body = children![
        hero,
        ui::inset(
            theme,
            16.0,
            ui::tier_control(
                theme,
                tier,
                &estimate.move_label,
                &estimate.deep_label,
                move |t| { set_tier.set_tier(t) }
            )
        ),
    ];

    if deep {
        body.push(ui::inset(
            theme,
            14.0,
            ui::tinted_card(
                theme,
                colors.destructive_tint,
                format!(
                    "Not reversible once the {RETENTION_DAYS}-day window closes. A photo is only \
                     re-encoded if it scores SSIMULACRA2 at or above 90 against your original — \
                     anything below the floor is kept losslessly instead."
                ),
                colors.destructive_txt,
            ),
        ));
    } else {
        body.push(ui::inset(
            theme,
            14.0,
            ui::caption(
                theme,
                "Byte-exact. Every original stays reconstructable from the vavlt copy, and the \
                 app proves it by decoding each result and comparing.",
            ),
        ));
    }

    let toggle = state.clone();
    body.push(state.photos.with(|photos| {
        ui::section(
            theme,
            "In this plan",
            Some(format!("{included} photos")),
            Some("Tap a photo to leave it out. Excluded photos leave the manifest, so the plan hash changes."),
            ui::inset(theme, 0.0, ui::mosaic(theme, photos, Some(Rc::new(move |i| toggle.toggle_photo(i))))),
        )
    }));

    let grant = state.grant_delete.get();
    let set_grant = state.clone();
    body.push(ui::section(
        theme,
        "Permissions",
        None,
        None,
        ui::inset(
            theme,
            0.0,
            Flex::column()
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .main_axis_size(MainAxisSize::Min)
                .children(children![
                    ui::row(
                        theme,
                        ui::Row::new("Reading these files — granted by the picker")
                            .subtitle(format!(
                                "These {} files and nothing else. vavlt declares no media permission.",
                                included + excluded
                            ))
                            .icon(ui::icons::check(), colors.success_txt),
                        None,
                    ),
                    ui::divider(theme),
                    ui::switch_row(
                        theme,
                        "Delete originals later",
                        &format!(
                            "Off. Nothing is deleted by this run either way — this only marks them \
                             eligible after {RETENTION_DAYS} days, and the system asks again first."
                        ),
                        grant,
                        true,
                        move |v| set_grant.set_grant_delete(v),
                    ),
                ]),
        ),
    ));

    body.push(ui::inset(
        theme,
        20.0,
        Text::new(format!("plan {}", state.manifest.get()))
            .style(theme.meta().weight(FontWeight::Regular)),
    ));
    body.push(ui::gap(150.0));

    let start = state.clone();
    let action = ui::action_bar(
        theme,
        children![
            ui::button(
                theme,
                if included == 0 {
                    "Nothing selected".to_string()
                } else if deep {
                    format!("Deep Move {included} photos")
                } else {
                    format!("Move {included} photos")
                },
                if deep {
                    ui::ButtonKind::Destructive
                } else {
                    ui::ButtonKind::Suggested
                },
                included > 0,
                move || start.start_run(),
            ),
            ui::caption(
                theme,
                if deep {
                    "Re-encodes each photo and scores it. Your originals are not touched by this run."
                } else {
                    "Encodes each photo and verifies it byte for byte. Your originals are not touched."
                }
            ),
        ],
    );

    let _ = m;
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
