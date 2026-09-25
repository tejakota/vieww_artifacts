//! One photograph, in a square.
//!
//! The tile is the interface. Everything the app knows about a file that can be
//! shown as a mark on its picture is shown as a mark on its picture: excluded,
//! protected, working, and the result. Text is what explains the things a mark
//! cannot.
//!
//! Four layers, bottom to top: the class label as a placeholder, the picture,
//! the working veil, then the badges. The placeholder is under the picture
//! rather than instead of it, so a tile whose thumbnail has not arrived is
//! still a tile rather than a hole.

use vieww::foundation::Image as ImageData;
use vieww::prelude::*;

use crate::model::{Photo, PhotoState};
use crate::theme::{motion, VavltTheme};

/// A square tile for `photo`. `on_tap` makes it excludable.
/// A photograph, at whatever size and curvature the caller needs.
///
/// Width, height and radius are all arguments rather than metrics, because the
/// mosaic's whole point is that they differ per tile — a 2x2 block and a 1x1
/// block are the same widget with different numbers, and the radius has to grow
/// with the block or the big one reads as a squarer, different shape.
pub fn tile(
    theme: &VavltTheme,
    photo: &Photo,
    width: f32,
    height: f32,
    radius: f32,
    on_tap: Option<Box<dyn Fn()>>,
) -> WidgetNode {
    let theme = *theme;
    let colors = theme.colors;
    let photo = photo.clone();

    let body = move |press: f32| -> WidgetNode {
        let mut layers = children![
            // The placeholder. Always present, always underneath.
            Container::new()
                .color(colors.hover)
                .alignment(Alignment::CENTER)
                .child(
                    // Centred by the `Container`, not by itself — see the
                    // note in `ui::button`.
                    Text::new(photo.class_label.clone()).style(theme.meta())
                )
        ];

        if let Some(thumb) = &photo.thumb {
            // Excluded photos stay legible at 30%. Fading them to nothing would
            // hide the thing the user is deciding about.
            let alpha = if photo.included { 1.0 } else { 0.30 };
            layers.push(
                Opacity::new(alpha)
                    .child(
                        Image::new(ImageData::from_shared_rgba8(
                            thumb.pixels.clone(),
                            thumb.edge,
                            thumb.edge,
                        ))
                        .fit(BoxFit::Cover)
                        .label(photo.name.clone()),
                    )
                    .into(),
            );
        }

        // The run's own progress, per tile: a photo still in the codec is
        // veiled, and the veil lifts when its result is in. This is why the
        // working screen needs no progress bar to be legible — the grid is one.
        if photo.state == PhotoState::Working {
            layers.push(Container::new().color(colors.window.with_alpha(140)).into());
        }

        if photo.state.has_result() {
            layers.push(result_badge(&theme, &photo).into());
        }

        // The exclusion mark and the protected mark share a corner and are
        // mutually exclusive, so a tile never has to be read twice.
        if !photo.included {
            layers.push(corner_mark(super::icons::cross(), Color::WHITE).into());
        } else if photo.protected {
            layers.push(corner_mark(super::icons::lock(), colors.warning_txt).into());
        }

        if press > 0.0 {
            layers.push(
                Container::new()
                    .color(colors.fg.with_alpha((press * 46.0) as u8))
                    .into(),
            );
        }

        Container::new()
            .size(width, height)
            .radius(radius)
            .child(Clip::rounded(radius).child(Stack::new().fit(StackFit::Expand).children(layers)))
            .into()
    };

    match on_tap {
        Some(handler) => Pressable::new(body).on_tap(handler).into(),
        None => body(0.0),
    }
}

/// Bottom-left. Carries the measured percentage, or the word the engine used
/// when there is no percentage to quote.
fn result_badge(theme: &VavltTheme, photo: &Photo) -> Positioned {
    let colors = theme.colors;
    let fill = match photo.state {
        PhotoState::Failed => colors.destructive_bg,
        PhotoState::Skipped => Color::rgba(0, 0, 0, 136),
        PhotoState::Unqualified => colors.warning_txt,
        _ => colors.accent_bg,
    };
    let ink = if photo.state == PhotoState::Unqualified {
        colors.window
    } else {
        Color::WHITE
    };

    Positioned::new().left(7.0).bottom(7.0).child(
        Container::new()
            .height(22.0)
            .color(fill)
            .radius(11.0)
            .padding(EdgeInsets::symmetric(7.0, 0.0))
            .alignment(Alignment::CENTER)
            .child(Text::new(photo.pct_label.clone()).style(theme.meta().color(ink))),
    )
}

/// Top-right. A round chip over the picture, dark enough to read on anything.
fn corner_mark(icon: IconData, tint: Color) -> Positioned {
    Positioned::new().right(7.0).top(7.0).child(
        Container::new()
            .size(24.0, 24.0)
            .color(Color::rgba(0, 0, 0, 170))
            .radius(12.0)
            .alignment(Alignment::CENTER)
            .child(Icon::new(icon).size(15.0).color(tint)),
    )
}

#[allow(dead_code)]
const _: std::time::Duration = motion::TILE;
