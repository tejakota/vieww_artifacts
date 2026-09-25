//! Depth 2 — the run.
//!
//! **The one screen with a small billboard.** Here the grid is the content, not
//! the decoration: a photo still in the codec is veiled and one whose result is
//! in wears its figure, so progress is your photographs clearing rather than an
//! abstract bar. A tall header would push the thing you are watching off the
//! fold, which is why `Height::Working` is 210 where the others are 330–440.

use std::rc::Rc;

use vavlt_core::Tier;
use vieww::element::ScrollController;
use vieww::prelude::*;

use crate::model::Screen;
use crate::state::VavltState;
use crate::theme::VavltTheme;
use crate::ui;

pub fn working(
    theme: &VavltTheme,
    state: &Rc<VavltState>,
    scroll: &ScrollController,
) -> WidgetNode {
    let colors = theme.colors;
    let running = state.running.get();
    let progress = state.progress.get();
    let deep = state.tier.get() != Tier::Move;
    let done = state.done_count.get();
    let total = state.included_count();

    let hero = state.photos.with(|photos| {
        let art = photos
            .iter()
            .find(|p| p.state == crate::model::PhotoState::Working)
            .cloned();
        ui::billboard(
            theme,
            ui::Height::Working.for_form(theme.form),
            art.as_ref(),
            deep,
            children![
                ui::eyebrow(
                    theme,
                    if running { "encoding now" } else { "finished" },
                    None
                ),
                ui::figure(
                    theme,
                    if running && !state.current_file.get().is_empty() {
                        state.current_file.get()
                    } else {
                        format!("{done} of {total}")
                    },
                    32.0,
                    colors.fg
                ),
                ui::meter(theme, progress, deep),
                ui::lede(
                    theme,
                    format!(
                        "{done} of {total} · decoding each result and comparing it byte for byte"
                    )
                ),
            ],
        )
    });

    let mut body = children![hero];
    body.push(state.photos.with(|photos| {
        ui::section(
            theme,
            "",
            None,
            Some("The grid is the progress bar. A veiled photograph is still in the codec; a badge means its result is in."),
            ui::inset(theme, 0.0, ui::mosaic(theme, photos, None)),
        )
    }));
    body.push(super::vault::note(
        theme,
        ui::icons::shield(),
        "One that cannot prove its roundtrip is reported as failed, not as saved. The run never \
         ships an encode it could not verify.",
    ));
    body.push(ui::gap(150.0));

    let cancel = state.clone();
    let onward = state.clone();
    let action = ui::action_bar(
        theme,
        children![if running {
            ui::button(theme, "Stop", ui::ButtonKind::Glass, true, move || {
                cancel.cancel_run()
            })
        } else {
            ui::button(
                theme,
                "See what changed",
                ui::ButtonKind::Solid,
                true,
                move || onward.go(Screen::Outcome),
            )
        }],
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
