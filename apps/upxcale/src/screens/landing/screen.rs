//! The landing screen: the grid, the button, and the routes over them.
//!
//! Mirrors `prototype/screens/landing/screen.css` and the controller half of
//! `app.js`.
//!
//! vieww mapping: `Theme` + `SafeArea` + `Stack` + `Navigator`.
//!
//! This file owns the root of the screen — the theme published to the subtree,
//! the safe-area inset, the layering of body against popups, and which route is
//! on top. It owns none of their appearance.
//!
//! # The route stack
//!
//! `landing` → `picker` → `progress` → `compare`, exactly the prototype's
//! order. The picker and the compare sheet are `Route::modal`, so the grid
//! behind them keeps painting; the progress overlay is modal too but its
//! barrier takes no dismiss handler, which is how "you cannot tap this away"
//! is expressed.
//!
//! # Scrolling
//!
//! Two `ScrollController`s, not one: the masonry and the picker's grid scroll
//! independently, and sharing a controller would mean opening the picker jumps
//! it to wherever the grid happened to be. Both are attached to the frame's
//! `Tickers` in `main.rs` — without that a fling stops dead when the finger
//! lifts.

use std::rc::Rc;

use vieww_foundation::{Alignment, EdgeInsets};
use vieww_widget::prelude::*;
use vieww_widget::widget_node_from;

use crate::photos::{self, Photo};
use crate::state::{routes, AppState};
use crate::theme;

use super::backdrop::Backdrop;
use super::compare_sheet::CompareSheet;
use super::photo_grid::{Cell, PhotoGrid};
use super::picker_dialog::{Choice, PickerDialog};
use super::progress_overlay::ProgressOverlay;
use super::render_button::RenderButton;

/// The screen's root widget.
///
/// Holds nothing but [`AppState`], which is itself a bundle of handles — the
/// scroll controllers included. Routes are built from clones of it, so no route
/// builder is ever the only thing keeping a controller alive.
#[derive(Clone)]
pub struct Landing {
    pub state: AppState,
}

impl std::fmt::Debug for Landing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Landing").finish()
    }
}

impl Landing {
    /// The body: the masonry with the Render button floating over it.
    ///
    /// A `Stack` rather than a `Scaffold` — vieww has no `Scaffold`; pinning a
    /// floating action button to a corner of a stack is how the framework's own
    /// `fab.rs` says to do it.
    #[must_use]
    pub fn body(&self) -> WidgetNode {
        let state = self.state.clone();
        let scroll = state.grid_scroll.clone();

        // Resolve pixels here, once, rather than inside each tile: the tile is
        // then a pure function of what it is handed, and the `Library`'s
        // `RefCell`s are never borrowed inside a `build` that could re-enter.
        let cells: Vec<Cell> = state
            .grid
            .get()
            .into_iter()
            .filter_map(|item| {
                let photo = item.photo()?;
                let image = if item.upscaled {
                    state.library.result(photo)
                } else {
                    state.library.source(photo).ok()
                };
                Some(Cell { item, photo, image })
            })
            .collect();

        let tap_state = state.clone();
        let on_tap: Rc<dyn Fn(&'static Photo)> = Rc::new(move |photo: &'static Photo| {
            // Tapping an upscaled tile compares it. Tapping a source tile opens
            // the picker with that photograph already ticked — the shortcut the
            // prototype had, kept because it is the obvious gesture.
            if tap_state.library.is_upscaled(photo) {
                tap_state.open_compare(photo.id);
                tap_state.nav.push(compare_route(&tap_state));
            } else {
                tap_state.clear_picks();
                tap_state.toggle_pick(photo.id);
                tap_state.nav.push(picker_route(tap_state.clone()));
            }
        });

        let busy = state.is_rendering();
        let open_state = state.clone();

        Stack::new()
            .alignment(Alignment::TOP_LEFT)
            .push(Positioned::fill().child(PhotoGrid {
                cells,
                offset: scroll.offset(),
                on_drag: scroll.on_drag(),
                on_drag_end: scroll.on_drag_end(),
                on_extents: scroll.on_extents(),
                on_tap,
            }))
            .push(
                Positioned::new().left(0.0).right(0.0).bottom(0.0).child(
                    Container::new()
                        .padding(EdgeInsets::only(16.0, 0.0, 16.0, 24.0))
                        // A centred row whose only child is not `Flexible`
                        // gets *loose* constraints, so the button sizes to
                        // its label instead of stretching edge to edge. A
                        // `Container` with a `BOTTOM_CENTER` alignment does
                        // not do this — it aligns the child inside a box
                        // that is itself still full width, and the button's
                        // own decoration then paints across all of it.
                        .child(
                            Flex::row()
                                .main_axis_alignment(MainAxisAlignment::Center)
                                .children(children![RenderButton {
                                    busy,
                                    on_pressed: Rc::new(move || {
                                        // Opening the picker always starts from a
                                        // clean selection: the previous batch's
                                        // ticks are not what the user means now.
                                        open_state.clear_picks();
                                        open_state.nav.push(picker_route(open_state.clone()));
                                    }),
                                }]),
                        ),
                ),
            )
            .into()
    }
}

widget_node_from!(Landing);

impl Widget for Landing {
    fn debug_name(&self) -> &'static str {
        "Landing"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        // One `Theme` at the root publishes `ThemeData` to everything below;
        // every control reads it with `ThemeData::of(ctx)` exactly once per
        // build. `SafeArea` insets from the notch and the home indicator.
        Theme::new(theme::theme())
            .child(
                Container::new().color(theme::scheme().surface).child(
                    Stack::new().fit(StackFit::Expand).push(
                        Navigator::new(self.state.nav.routes())
                            .leaving(self.state.nav.leaving())
                            .surface(crate::SURFACE),
                    ),
                ),
            )
            .into()
    }
}

// ── the routes ──────────────────────────────────────────────────────────────
//
// A `Route` is a name and a `Rc<dyn Fn(&BuildContext) -> WidgetNode>`. Building
// them as free functions rather than methods keeps the closure's captures
// visible at the definition, which matters because every one of them is a
// clone that must not accidentally be the *only* handle to something.

/// The home route: the grid and the button, inside a `SafeArea`.
#[must_use]
pub fn landing_route(landing: Landing) -> Route {
    Route::new(
        routes::LANDING,
        Rc::new(move |_ctx| SafeArea::new().child(landing.body()).into()),
    )
}

/// The picker, over a dismissable scrim.
#[must_use]
pub fn picker_route(state: AppState) -> Route {
    let scroll = state.picker_scroll.clone();
    Route::modal(
        routes::PICKER,
        Rc::new(move |_ctx| {
            let choices: Vec<Choice> = photos::CATALOGUE
                .iter()
                .map(|photo| Choice {
                    photo,
                    image: state.library.source(photo).ok(),
                    selected: state.is_picked(photo.id),
                })
                .collect();

            let toggle = state.clone();
            let cancel = state.clone();
            let confirm = state.clone();
            let dismiss = state.clone();

            Stack::new()
                .fit(StackFit::Expand)
                .push(Backdrop::dismissable(
                    theme::SCRIM_PICKER,
                    Rc::new(move || {
                        dismiss.clear_picks();
                        dismiss.nav.pop();
                    }),
                ))
                .push(PickerDialog {
                    choices,
                    picked: state.pick_count(),
                    offset: scroll.offset(),
                    on_drag: scroll.on_drag(),
                    on_drag_end: scroll.on_drag_end(),
                    on_extents: scroll.on_extents(),
                    on_toggle: Rc::new(move |photo: &'static Photo| toggle.toggle_pick(photo.id)),
                    on_cancel: Rc::new(move || {
                        cancel.clear_picks();
                        cancel.nav.pop();
                    }),
                    on_confirm: Rc::new(move || {
                        // Replace rather than push: the picker has done its job
                        // and the user should not land back on it when the
                        // render finishes.
                        if confirm.start_render() {
                            confirm.nav.replace(progress_route(confirm.clone()));
                        }
                    }),
                })
                .into()
        }),
    )
}

/// The progress overlay, over a scrim that cannot be tapped away.
#[must_use]
pub fn progress_route(state: AppState) -> Route {
    Route::modal(
        routes::PROGRESS,
        Rc::new(move |_ctx| {
            Stack::new()
                .fit(StackFit::Expand)
                .push(Backdrop::blocking(theme::SCRIM_PROGRESS))
                .push(ProgressOverlay {
                    state: state.progress.get(),
                })
                .into()
        }),
    )
}

/// The before/after sheet.
#[must_use]
pub fn compare_route(state: &AppState) -> Route {
    let state = state.clone();
    Route::modal(
        routes::COMPARE,
        Rc::new(move |_ctx| {
            let Some(photo) = state.comparing_photo() else {
                // Nothing to compare — an empty node rather than a panic. This
                // is reachable only if the signal is cleared while the route is
                // still on the stack, which the app does not do, but a route
                // builder that can panic is a route builder that will.
                return SizedBox::shrink().into();
            };

            let close = state.clone();
            let save = state.clone();
            let dismiss = state.clone();
            let divider = state.clone();

            Stack::new()
                .fit(StackFit::Expand)
                .push(Backdrop::dismissable(
                    theme::SCRIM_COMPARE,
                    Rc::new(move || {
                        dismiss.nav.pop();
                    }),
                ))
                .push(CompareSheet {
                    photo,
                    before: state.library.source(photo).ok(),
                    after: state.library.result(photo),
                    divider: state.divider.get(),
                    on_divider: Rc::new(move |fraction| divider.set_divider(fraction)),
                    on_close: Rc::new(move || {
                        close.nav.pop();
                    }),
                    on_save: Rc::new(move || {
                        // The path is logged inside; the sheet has nothing to do
                        // with it, so the closure discards it and stays `Fn()`.
                        save.save_current();
                    }),
                })
                .into()
        }),
    )
}
