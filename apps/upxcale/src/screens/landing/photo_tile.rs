//! One photograph in the masonry.
//!
//! Mirrors `prototype/screens/landing/photo-tile.css`.
//!
//! vieww mapping: `Image` + `DecoratedBox` + `Clip` + `Pressable` +
//! `AspectRatio` + `AnimatedContainer` + `Stack` + `Positioned`.
//!
//! This file owns the photograph's cover fit and corner clipping, the press
//! wash, the selection overlay, the "4x upscaled" badge, and the ring a
//! freshly-rendered tile wears for a beat.
//!
//! It does not own the masonry layout ([`super::photo_grid`]) or the button at
//! the bottom ([`super::render_button`]).

use std::rc::Rc;

// `Image` is two types in this framework: the widget (`vieww_widget::Image`,
// arriving through the prelude) and the pixels it draws
// (`vieww_foundation::Image`). Aliasing the pixels to `Pixels` is the
// convention vieww's own examples use, and it keeps `Image::new(...)` at the
// call site meaning the widget, as it does everywhere else.
use vieww_foundation::Image as Pixels;
use vieww_foundation::{Alignment, BoxFit, Color, EdgeInsets, Offset, Shadow};
use vieww_widget::prelude::*;
use vieww_widget::widget_node_from;

use crate::icons;
use crate::photos::{Extent, Photo, UPSCALE_FACTOR};
use crate::theme;

/// How rounded a tile is, as a multiple of the theme's corner.
///
/// A photograph wants a softer corner than a button does — the prototype used
/// `calc(var(--vw-corner) * 1.5)` and this is that number, kept as a named
/// multiple of the token rather than as 15.0 so a theme change still reaches
/// it.
const CORNER_SCALE: f32 = 1.5;

/// A tile in the landing masonry.
pub struct PhotoTile {
    pub photo: &'static Photo,
    /// The pixels to draw: the upscaled result when there is one, else the
    /// source. `None` when the file could not be decoded.
    pub image: Option<Pixels>,
    /// Whether `image` is the upscaled result — drives the badge.
    pub upscaled: bool,
    /// Whether the post-render ring is showing.
    pub fresh: bool,
    /// Whether the tile is ticked. The landing grid never sets this; the picker
    /// reuses the same visual language through
    /// [`super::picker_dialog`](super::picker_dialog).
    pub selected: bool,
    pub on_tap: Rc<dyn Fn()>,
}

impl Widget for PhotoTile {
    fn debug_name(&self) -> &'static str {
        "PhotoTile"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, ctx: &BuildContext) -> WidgetNode {
        let theme_data = *ThemeData::of(ctx);
        let corner = theme_data.metrics.corner * CORNER_SCALE;

        let photo = self.photo;
        let upscaled = self.upscaled;
        let selected = self.selected;
        let image = self.image.clone();
        let on_tap = Rc::clone(&self.on_tap);

        // `Pressable::themed` reads CONTROL_DURATION off the theme, so the
        // press fade here is the same length as every other control's.
        let body = Pressable::themed(ctx, move |press| {
            let mut layers = Stack::new().alignment(Alignment::CENTER);

            // ── the photograph ───────────────────────────────────────────────
            // `BoxFit::Cover` fills the tile and crops the overflow, which is
            // what a grid of mixed aspect ratios needs; `Contain` would letterbox
            // and `Fill` would distort. `Cover` deliberately paints outside its
            // bounds, so the `Clip` below is load-bearing rather than cosmetic.
            layers = match &image {
                Some(pixels) => layers.push(
                    Image::new(pixels.clone())
                        .fit(BoxFit::Cover)
                        .alignment(Alignment::CENTER)
                        .label(photo.title),
                ),
                // A tile whose file would not decode shows the placeholder
                // shimmer rather than a hole. `Skeleton` is the framework's own
                // control for exactly this.
                None => layers.push(Skeleton::new().corner(corner)),
            };

            // ── press wash ───────────────────────────────────────────────────
            // vieww's own controls composite `on_surface` into the fill at
            // `PRESSED_ALPHA * press`; over a photograph the equivalent is a
            // scrim that deepens, because there is no fill to wash.
            if press > 0.0 {
                let alpha = (press * 46.0) as u8;
                layers = layers
                    .push(Positioned::fill().child(ColoredBox::new(Color::rgba(0, 0, 0, alpha))));
            }

            // ── selection ────────────────────────────────────────────────────
            if selected {
                layers =
                    layers.push(Positioned::fill().child(ColoredBox::new(theme::SELECTION_WASH)));
                layers = layers.push(
                    Positioned::new()
                        .top(theme_data.metrics.gap)
                        .right(theme_data.metrics.gap)
                        .child(check_mark(&theme_data)),
                );
            }

            // ── the "4x upscaled" badge ──────────────────────────────────────
            if upscaled {
                layers = layers.push(
                    Positioned::new()
                        .left(theme_data.metrics.gap)
                        .bottom(theme_data.metrics.gap)
                        .child(upscaled_badge(&theme_data)),
                );
            }

            layers.into()
        })
        .on_tap(move || on_tap());

        // ── the frame ────────────────────────────────────────────────────────
        // Order matters and reads outward: aspect ratio decides the box, the
        // clip keeps the cover-fitted photograph inside the corner, and the
        // decoration draws the ring outside it. Putting the clip outside the
        // decoration would clip the ring off.
        let framed = AspectRatio::new(photo.extent.ratio()).child(
            Clip::rounded(corner).child(
                Container::new()
                    .color(theme_data.colors.surface_variant)
                    .radius(corner)
                    .child(body),
            ),
        );

        if self.fresh {
            // The accent ring, drawn as a decorated box *behind* the clipped
            // photograph, inflated by the border width. `AnimatedContainer`
            // would be the tool if this faded on its own; it does not — the
            // grid drops `fresh` a beat later and the tile rebuilds without it.
            Container::new()
                .decoration(
                    BoxDecoration::new()
                        .radius(corner + 2.0)
                        .border(Border::new(theme_data.colors.primary, 2.0))
                        .shadow(Shadow::new(
                            Color::rgba(10, 132, 255, 64),
                            Offset::new(0.0, 6.0),
                            16.0,
                        )),
                )
                .padding(EdgeInsets::all(2.0))
                .child(framed)
                .into()
        } else {
            framed.into()
        }
    }
}

widget_node_from!(PhotoTile);

/// The circular tick on a selected tile.
fn check_mark(theme_data: &ThemeData) -> WidgetNode {
    Container::new()
        .decoration(
            BoxDecoration::new()
                .color(theme_data.colors.primary)
                .stadium()
                .border(Border::new(theme::ON_PHOTO, 1.5)),
        )
        .size(24.0, 24.0)
        .alignment(Alignment::CENTER)
        .child(
            Icon::new(vieww_widget::icons::check())
                .size(14.0)
                .color(theme_data.colors.on_primary),
        )
        .into()
}

/// The small chip in the corner of a rendered tile.
fn upscaled_badge(theme_data: &ThemeData) -> WidgetNode {
    Container::new()
        .decoration(BoxDecoration::new().color(theme::PHOTO_PLATE).stadium())
        .padding(EdgeInsets::symmetric(8.0, 4.0))
        .child(
            Flex::row()
                .main_axis_size(MainAxisSize::Min)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .spacing(4.0)
                .children(children![
                    Icon::new(icons::sparkles())
                        .size(11.0)
                        .color(theme::ON_PHOTO),
                    Text::new(format!("{UPSCALE_FACTOR}x UPSCALED"))
                        .style(theme_data.text.label)
                        .color(theme::ON_PHOTO)
                        .size(10.0)
                        .bold(),
                ]),
        )
        .into()
}

/// A picker cell: the same photograph, square, denser, and always tickable.
///
/// Shares this file with [`PhotoTile`] rather than living in
/// `picker_dialog.rs` because it is the *same component* at a different size —
/// the prototype made the same call, giving the picker cell the landing tile's
/// selected-state language in a square variant.
pub struct PickerCell {
    pub photo: &'static Photo,
    pub image: Option<Pixels>,
    pub selected: bool,
    pub on_tap: Rc<dyn Fn()>,
}

impl Widget for PickerCell {
    fn debug_name(&self) -> &'static str {
        "PickerCell"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, ctx: &BuildContext) -> WidgetNode {
        let theme_data = *ThemeData::of(ctx);
        let corner = theme_data.metrics.corner;
        let selected = self.selected;
        let image = self.image.clone();
        let title = self.photo.title;
        let on_tap = Rc::clone(&self.on_tap);

        let body = Pressable::themed(ctx, move |press| {
            let mut layers = Stack::new().alignment(Alignment::CENTER);
            layers = match &image {
                Some(pixels) => {
                    layers.push(Image::new(pixels.clone()).fit(BoxFit::Cover).label(title))
                }
                None => layers.push(Skeleton::new().corner(corner)),
            };
            if press > 0.0 {
                let alpha = (press * 46.0) as u8;
                layers = layers
                    .push(Positioned::fill().child(ColoredBox::new(Color::rgba(0, 0, 0, alpha))));
            }
            if selected {
                layers =
                    layers.push(Positioned::fill().child(ColoredBox::new(theme::SELECTION_WASH)));
                layers = layers.push(
                    Positioned::new()
                        .top(4.0)
                        .right(4.0)
                        .child(small_check(&theme_data)),
                );
            }
            layers.into()
        })
        .on_tap(move || on_tap());

        let cell = Clip::rounded(corner).child(
            Container::new()
                .color(theme_data.colors.surface_variant)
                .radius(corner)
                .child(body),
        );

        // The accent ring on a ticked cell sits outside the clip, so it is not
        // shaved off by its own corner.
        if selected {
            Container::new()
                .decoration(
                    BoxDecoration::new()
                        .radius(corner + 2.5)
                        .border(Border::new(theme_data.colors.primary, 2.5)),
                )
                .padding(EdgeInsets::all(2.5))
                .child(AspectRatio::new(Extent::Square.ratio()).child(cell))
                .into()
        } else {
            Container::new()
                .padding(EdgeInsets::all(2.5))
                .child(AspectRatio::new(Extent::Square.ratio()).child(cell))
                .into()
        }
    }
}

widget_node_from!(PickerCell);

fn small_check(theme_data: &ThemeData) -> WidgetNode {
    Container::new()
        .decoration(
            BoxDecoration::new()
                .color(theme_data.colors.primary)
                .stadium(),
        )
        .size(20.0, 20.0)
        .alignment(Alignment::CENTER)
        .child(
            Icon::new(vieww_widget::icons::check())
                .size(12.0)
                .color(theme_data.colors.on_primary),
        )
        .into()
}

// `Widget` requires `Debug`, and these structs hold `Rc<dyn Fn>` callbacks,
// which are not. Hand-written rather than derived, and each prints the fields
// that identify *which* instance this is — a tree dump saying `PhotoTile` a
// dozen times is no use, one saying `PhotoTile { id: "a4", upscaled: true }`
// is.
impl std::fmt::Debug for PhotoTile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PhotoTile")
            .field("id", &self.photo.id)
            .field("upscaled", &self.upscaled)
            .field("fresh", &self.fresh)
            .field("selected", &self.selected)
            .finish()
    }
}

impl std::fmt::Debug for PickerCell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PickerCell")
            .field("id", &self.photo.id)
            .field("selected", &self.selected)
            .finish()
    }
}
