//! Before and after, with a divider you drag.
//!
//! Mirrors `prototype/screens/landing/compare-sheet.css`.
//!
//! vieww mapping: `BottomSheet`-shaped composition — `Stack` + `Positioned` +
//! `Clip` + `Image` + `LayoutBuilder` + `GestureDetector` + `Button`.
//!
//! # How the reveal actually works
//!
//! Both photographs are laid out at the full size of the stage, one on top of
//! the other. The top one — the upscaled result — is wrapped in a
//! `Clip::path` whose path is a rectangle covering only the left `t` of the
//! stage. So the two images stay in register no matter where the divider is,
//! and moving it changes a clip rather than a layout. That is the property the
//! whole control depends on: if the top image were *resized* to `t` of the
//! width, the two halves would show different parts of the picture and the
//! comparison would be meaningless.
//!
//! The clip path needs the stage's real size, which is not known when `build`
//! runs — so the stage is wrapped in a `LayoutBuilder`, which reports the
//! constraints its child was offered and rebuilds when they change. That is
//! also what makes the drag arithmetic correct: `local.dx / width` is a
//! fraction only if `width` is the width the stage was actually given.
//!
//! # Why this is not `vieww_widget::Slider`
//!
//! `Slider` is a track and a thumb with its own 20pt geometry and its own
//! gesture handling, and none of that is wanted here — the draggable thing is
//! the full height of the stage, and the "track" is the photograph. The
//! prototype described the divider as a `Slider`; building it that way would
//! mean fighting the control for its appearance. A `GestureDetector` over the
//! stage is the honest spelling.

use std::rc::Rc;

use vieww_foundation::{
    Alignment, BoxFit, Color, Constraints, EdgeInsets, Image as Pixels, Offset, Path, Rect, Size,
};
use vieww_widget::prelude::*;
use vieww_widget::widget_node_from;

use crate::icons;
use crate::photos::{Photo, UPSCALE_FACTOR};
use crate::theme;

/// How much of the screen height the sheet takes.
const SHEET_FRACTION: f32 = 0.82;

/// The diameter of the round drag handle.
const HANDLE: f32 = 44.0;

/// The before/after sheet.
pub struct CompareSheet {
    pub photo: &'static Photo,
    /// The source, drawn underneath and revealed on the right.
    pub before: Option<Pixels>,
    /// The upscaled result, clipped to the left of the divider.
    pub after: Option<Pixels>,
    /// Divider position, 0..=1.
    pub divider: f32,
    /// Called with a new fraction as the finger moves.
    pub on_divider: Rc<dyn Fn(f32)>,
    pub on_close: Rc<dyn Fn()>,
    pub on_save: Rc<dyn Fn()>,
}

impl Widget for CompareSheet {
    fn debug_name(&self) -> &'static str {
        "CompareSheet"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, ctx: &BuildContext) -> WidgetNode {
        let theme_data = *ThemeData::of(ctx);
        let gap = theme_data.metrics.gap;
        let corner = theme_data.metrics.corner * 2.4;

        let sheet = Container::new()
            .decoration(
                BoxDecoration::new()
                    .color(theme_data.colors.surface_variant)
                    .radius(corner),
            )
            .padding(EdgeInsets::only(gap * 1.5, gap * 1.5, gap * 1.5, gap * 2.0))
            .child(
                Flex::column()
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .spacing(gap * 1.5)
                    .children(children![
                        grabber(&theme_data),
                        header(&theme_data, self.photo, &self.on_close),
                        // The stage takes everything left over.
                        Flexible::expanded(1).child(self.stage(&theme_data)),
                        footer(&theme_data, &self.on_save, &self.on_close),
                    ]),
            );

        // Pinned to the bottom edge at a fixed fraction of the height. `Align`
        // with a height factor is vieww's spelling of "as tall as this much of
        // the parent, at the bottom".
        Align::new(Alignment::BOTTOM_CENTER)
            .child(
                SizedBox::from_size(Size::new(
                    crate::SURFACE.width,
                    crate::SURFACE.height * SHEET_FRACTION,
                ))
                .child(Clip::rounded(corner).child(sheet)),
            )
            .into()
    }
}

impl CompareSheet {
    /// The two photographs, the divider, and the finger that moves it.
    fn stage(&self, theme_data: &ThemeData) -> WidgetNode {
        let corner = theme_data.metrics.corner * 1.5;
        let before = self.before.clone();
        let after = self.after.clone();
        let divider = self.divider.clamp(0.0, 1.0);
        let on_divider = Rc::clone(&self.on_divider);
        let title = self.photo.title;
        let label_style = theme_data.text.label;

        let stage = LayoutBuilder::new(move |constraints: Constraints| {
            // `max_width` can be infinite if this ever ends up somewhere
            // unbounded. Falling back to the surface width keeps the clip
            // arithmetic finite instead of producing a NaN path.
            let width = if constraints.has_bounded_width() {
                constraints.max_width
            } else {
                crate::SURFACE.width
            };
            let height = if constraints.has_bounded_height() {
                constraints.max_height
            } else {
                crate::SURFACE.height * 0.5
            };
            let split = (width * divider).clamp(0.0, width);

            let mut layers = Stack::new().alignment(Alignment::CENTER);

            // ── before: the whole stage, underneath ──────────────────────────
            layers = match &before {
                Some(pixels) => layers.push(
                    Positioned::fill().child(
                        Image::new(pixels.clone())
                            .fit(BoxFit::Cover)
                            .label(format!("{title}, before")),
                    ),
                ),
                None => layers.push(Positioned::fill().child(Skeleton::new().corner(corner))),
            };

            // ── after: same size, same position, clipped to the left ─────────
            if let Some(pixels) = &after {
                layers = layers.push(
                    Positioned::fill().child(
                        Clip::path(Path::rect(Rect::new(0.0, 0.0, split, height))).child(
                            Image::new(pixels.clone())
                                .fit(BoxFit::Cover)
                                .label(format!("{title}, upscaled")),
                        ),
                    ),
                );
            }

            // ── the two chips ────────────────────────────────────────────────
            layers = layers.push(
                Positioned::new()
                    .left(8.0)
                    .top(8.0)
                    .child(stage_label("AFTER", label_style)),
            );
            layers = layers.push(
                Positioned::new()
                    .right(8.0)
                    .top(8.0)
                    .child(stage_label("BEFORE", label_style)),
            );

            // ── the divider line ─────────────────────────────────────────────
            layers = layers.push(
                Positioned::new()
                    .left(split - 1.0)
                    .top(0.0)
                    .bottom(0.0)
                    .width(2.0)
                    .child(ColoredBox::new(theme::DIVIDER)),
            );

            // ── the handle ───────────────────────────────────────────────────
            layers = layers.push(
                Positioned::new()
                    .left(split - HANDLE / 2.0)
                    .top(height / 2.0 - HANDLE / 2.0)
                    .width(HANDLE)
                    .height(HANDLE)
                    .child(handle()),
            );

            // ── the finger ───────────────────────────────────────────────────
            // One detector over the whole stage rather than one on the handle:
            // a 44pt circle is a small target on a phone, and dragging anywhere
            // on the picture is what people try first. `local` is the position
            // within this detector, which is the stage, so the division is a
            // fraction of the stage.
            let set = Rc::clone(&on_divider);
            let set_move = Rc::clone(&on_divider);
            GestureDetector::new()
                .drag_axis(vieww_foundation::Axis::Horizontal)
                .on_tap(move |tap| set(fraction(tap.local.dx, width)))
                .on_drag_update(move |drag| set_move(fraction(drag.local.dx, width)))
                .child(layers)
                .into()
        });

        Clip::rounded(corner)
            .child(
                Container::new()
                    .color(Color::BLACK)
                    .radius(corner)
                    .child(stage),
            )
            .into()
    }
}

widget_node_from!(CompareSheet);

/// Convert a local x to a 0..=1 fraction, guarding the degenerate width.
fn fraction(dx: f32, width: f32) -> f32 {
    if width <= 0.0 {
        0.5
    } else {
        (dx / width).clamp(0.0, 1.0)
    }
}

fn grabber(theme_data: &ThemeData) -> WidgetNode {
    Align::new(Alignment::CENTER)
        .child(
            Container::new()
                .decoration(
                    BoxDecoration::new()
                        .color(theme_data.colors.outline)
                        .stadium(),
                )
                .size(40.0, 5.0),
        )
        .into()
}

fn header(theme_data: &ThemeData, photo: &'static Photo, on_close: &Rc<dyn Fn()>) -> WidgetNode {
    let on_close = Rc::clone(on_close);
    Flex::row()
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .children(children![
            Flexible::expanded(1).child(
                Flex::column()
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .spacing(2.0)
                    .children(children![
                        Text::new(photo.title)
                            .style(theme_data.text.title)
                            .color(theme_data.colors.on_surface)
                            .bold(),
                        Text::new(format!("{UPSCALE_FACTOR}x detail · Lanczos + unsharp"))
                            .style(theme_data.text.label)
                            .color(theme_data.colors.on_surface_variant)
                            .size(12.0),
                    ])
            ),
            Button::new("Done")
                .style(ButtonStyle::Text)
                .on_pressed(move || on_close()),
        ])
        .into()
}

fn footer(theme_data: &ThemeData, on_save: &Rc<dyn Fn()>, on_close: &Rc<dyn Fn()>) -> WidgetNode {
    let on_save = Rc::clone(on_save);
    let on_close = Rc::clone(on_close);
    Flex::row()
        .spacing(theme_data.metrics.gap)
        .children(children![
            Flexible::expanded(2).child(
                Button::new("Save to Photos")
                    .style(ButtonStyle::Filled)
                    .on_pressed(move || on_save())
            ),
            Flexible::expanded(1).child(
                Button::new("Close")
                    .style(ButtonStyle::Text)
                    .on_pressed(move || on_close())
            ),
        ])
        .into()
}

/// A "BEFORE" / "AFTER" chip on the photograph.
fn stage_label(text: &str, style: TextStyle) -> WidgetNode {
    Container::new()
        .decoration(BoxDecoration::new().color(theme::PHOTO_PLATE).stadium())
        .padding(EdgeInsets::symmetric(10.0, 4.0))
        .child(
            Text::new(text.to_string())
                .style(style)
                .color(theme::ON_PHOTO)
                .size(11.0)
                .bold(),
        )
        .into()
}

/// The round grip on the divider.
fn handle() -> WidgetNode {
    Container::new()
        .decoration(
            BoxDecoration::new()
                .color(theme::ON_PHOTO)
                .stadium()
                .shadow(vieww_foundation::Shadow::new(
                    Color::rgba(0, 0, 0, 102),
                    Offset::new(0.0, 2.0),
                    8.0,
                )),
        )
        .alignment(Alignment::CENTER)
        .child(
            Icon::new(icons::compare())
                .size(20.0)
                .color(Color::rgb(27, 27, 31)),
        )
        .into()
}

// `Widget` requires `Debug`, and these structs hold `Rc<dyn Fn>` callbacks,
// which are not. Hand-written rather than derived, and each prints the fields
// that identify *which* instance this is — a tree dump saying `PhotoTile` a
// dozen times is no use, one saying `PhotoTile { id: "a4", upscaled: true }`
// is.
impl std::fmt::Debug for CompareSheet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompareSheet")
            .field("photo", &self.photo.id)
            .field("divider", &self.divider)
            .field("has_after", &self.after.is_some())
            .finish()
    }
}
