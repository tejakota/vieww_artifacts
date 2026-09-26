//! The photo picker.
//!
//! Mirrors `prototype/screens/landing/picker-dialog.css`.
//!
//! vieww mapping: `Dialog`-shaped composition — `Container` + `Flex` +
//! `Scrollable` + `Grid` + `Button`, over a [`Backdrop`](super::backdrop).
//!
//! # Why not `vieww_widget::Dialog`
//!
//! `Dialog` is title + content + actions and it builds its own `ModalBarrier`.
//! Two of those get in the way here: the barrier would be a second scrim on top
//! of the route's own, and `Dialog` caps its width at 400 and centres — this
//! card wants to be 92% of a phone's width and to cap its *height* at 78% so the
//! grid inside it scrolls rather than the card growing off-screen.
//!
//! So this is a composition rather than a use, which is the same call the
//! prototype made and for the same stated reason: a `Dialog` with a `GridView`
//! in it is not a reusable control, it is this screen's route.
//!
//! # When it appears
//!
//! Pushed by the Render button, not on app open. The app opens to photographs
//! with the button waiting; this is what the button opens.

use std::rc::Rc;

use vieww_foundation::{Alignment, Axis, EdgeInsets, Image as Pixels};
use vieww_widget::prelude::*;
use vieww_widget::{widget_node_from, Handler};

use crate::photos::Photo;
use crate::theme;

use super::photo_tile::PickerCell;

/// Columns in the picker. Three rather than the grid's two: picking is the task
/// here, so density beats fidelity.
const COLUMNS: usize = 3;

/// One selectable photograph, resolved by the screen.
pub struct Choice {
    pub photo: &'static Photo,
    pub image: Option<Pixels>,
    pub selected: bool,
}

impl std::fmt::Debug for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Choice")
            .field("id", &self.photo.id)
            .field("selected", &self.selected)
            .finish()
    }
}

/// The picker card.
pub struct PickerDialog {
    pub choices: Vec<Choice>,
    /// How many are ticked — drives the confirm button's label and whether it
    /// is enabled.
    pub picked: usize,
    pub offset: f32,
    pub on_drag: Handler<vieww_foundation::DragDetails>,
    pub on_drag_end: Handler<vieww_foundation::DragDetails>,
    pub on_extents: Handler<ScrollExtents>,
    pub on_toggle: Rc<dyn Fn(&'static Photo)>,
    pub on_cancel: Rc<dyn Fn()>,
    pub on_confirm: Rc<dyn Fn()>,
}

impl Widget for PickerDialog {
    fn debug_name(&self) -> &'static str {
        "PickerDialog"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, ctx: &BuildContext) -> WidgetNode {
        let theme_data = *ThemeData::of(ctx);
        let gap = theme_data.metrics.gap;
        let corner = theme_data.metrics.corner * 2.8;

        let cells: Vec<WidgetNode> = self
            .choices
            .iter()
            .map(|choice| {
                let photo = choice.photo;
                let on_toggle = Rc::clone(&self.on_toggle);
                PickerCell {
                    photo,
                    image: choice.image.clone(),
                    selected: choice.selected,
                    on_tap: Rc::new(move || on_toggle(photo)),
                }
                .into()
            })
            .collect();

        // `Grid::columns` is the non-scrolling CSS-grid-like layout; the
        // `Scrollable` around it is what moves. `GridView` would virtualise,
        // which at a dozen cells buys nothing and costs a fixed cell extent.
        let grid = Grid::columns(COLUMNS).gap(2.0).children(cells);

        let card = Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min)
            .children(children![
                header(&theme_data, &self.on_cancel),
                // The grid takes whatever height is left after the header and
                // the actions, and scrolls inside it. Without `Flexible` the
                // column would size to the grid and overflow the card.
                Flexible::expanded(1).child(
                    Scrollable::new(Axis::Vertical, self.offset)
                        .on_drag(Rc::clone(&self.on_drag))
                        .on_drag_end(Rc::clone(&self.on_drag_end))
                        .on_extents(Rc::clone(&self.on_extents))
                        .child(
                            Container::new()
                                .padding(EdgeInsets::symmetric(gap * 1.5, 0.0))
                                .child(grid)
                        )
                ),
                actions(&theme_data, self.picked, &self.on_cancel, &self.on_confirm),
            ]);

        // Centred, 92% of the width, capped at 78% of the height. `Align` with
        // factors is how vieww spells "shrink-wrap the child, then place it".
        Align::new(Alignment::CENTER)
            .child(
                Constrained::new(vieww_foundation::Constraints::new(
                    0.0,
                    380.0,
                    0.0,
                    crate::SURFACE.height * 0.78,
                ))
                .child(
                    Container::new()
                        .decoration(
                            BoxDecoration::new()
                                .color(theme_data.colors.surface_variant)
                                .radius(corner)
                                // One elevation language across every floating
                                // surface: the Render button carries the
                                // accent-tinted lift, the cards this neutral
                                // one. Over a frosted scrim the shadow is what
                                // keeps the card reading as *above* the blur —
                                // without it the frosted glass flattens
                                // everything behind it onto one plane, dialog
                                // included.
                                .shadow(theme::CARD_SHADOW),
                        )
                        .margin(EdgeInsets::symmetric(gap * 2.0, 0.0))
                        .child(Clip::rounded(corner).child(card)),
                ),
            )
            .into()
    }
}

widget_node_from!(PickerDialog);

fn header(theme_data: &ThemeData, on_close: &Rc<dyn Fn()>) -> WidgetNode {
    let on_close = Rc::clone(on_close);
    let gap = theme_data.metrics.gap;

    Container::new()
        .padding(EdgeInsets::only(gap * 2.5, gap * 2.5, gap * 1.5, gap * 1.5))
        .child(
            Flex::row()
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .children(children![
                    Flexible::expanded(1).child(
                        Flex::column()
                            .cross_axis_alignment(CrossAxisAlignment::Start)
                            .spacing(2.0)
                            .children(children![
                                Text::new("Upscale photos")
                                    .style(theme_data.text.headline)
                                    .color(theme_data.colors.on_surface)
                                    .size(22.0)
                                    .bold(),
                                Text::new("Pick one or more. We'll rebuild the detail.")
                                    .style(theme_data.text.body)
                                    .color(theme_data.colors.on_surface_variant),
                            ])
                    ),
                    // A round ghost button. `Button` would enforce a 44pt box
                    // and a label; this is an icon affordance, so it composes
                    // `Pressable` the way `controls/` invites.
                    close_button(theme_data, on_close),
                ]),
        )
        .into()
}

fn close_button(theme_data: &ThemeData, on_close: Rc<dyn Fn()>) -> WidgetNode {
    let colors = theme_data.colors;
    Semantics::new()
        .label("Dismiss")
        .role(SemanticRole::Button)
        .child(
            Pressable::new(move |press| {
                Container::new()
                    .decoration(
                        BoxDecoration::new()
                            .color(ColorScheme::pressed(
                                colors.surface,
                                colors.on_surface,
                                press,
                            ))
                            .stadium(),
                    )
                    .size(32.0, 32.0)
                    .alignment(Alignment::CENTER)
                    .child(
                        Icon::new(icons::close())
                            .size(18.0)
                            .color(colors.on_surface_variant),
                    )
                    .into()
            })
            .on_tap(move || on_close()),
        )
        .into()
}

fn actions(
    theme_data: &ThemeData,
    picked: usize,
    on_cancel: &Rc<dyn Fn()>,
    on_confirm: &Rc<dyn Fn()>,
) -> WidgetNode {
    let gap = theme_data.metrics.gap;
    let on_cancel = Rc::clone(on_cancel);
    let on_confirm = Rc::clone(on_confirm);

    // vieww's rule: a button with no handler is disabled — dimmed, and it
    // registers no recogniser. So "nothing picked" is expressed by *not
    // attaching* a handler rather than by attaching one that returns early.
    let confirm = if picked == 0 {
        Button::new("Select photos").style(ButtonStyle::Filled)
    } else {
        let label = if picked == 1 {
            "Upscale 1 photo".to_string()
        } else {
            format!("Upscale {picked} photos")
        };
        Button::new(label)
            .style(ButtonStyle::Filled)
            .on_pressed(move || on_confirm())
    };

    Container::new()
        .padding(EdgeInsets::only(gap * 2.0, gap * 1.5, gap * 2.0, gap * 2.0))
        .child(
            Flex::row()
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .spacing(gap)
                .children(children![
                    Flexible::expanded(1).child(
                        Button::new("Cancel")
                            .style(ButtonStyle::Text)
                            .on_pressed(move || on_cancel())
                    ),
                    Flexible::expanded(2).child(confirm),
                ]),
        )
        .into()
}

// `Widget` requires `Debug`, and these structs hold `Rc<dyn Fn>` callbacks,
// which are not. Hand-written rather than derived, and each prints the fields
// that identify *which* instance this is — a tree dump saying `PhotoTile` a
// dozen times is no use, one saying `PhotoTile { id: "a4", upscaled: true }`
// is.
impl std::fmt::Debug for PickerDialog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PickerDialog")
            .field("choices", &self.choices.len())
            .field("picked", &self.picked)
            .finish()
    }
}
