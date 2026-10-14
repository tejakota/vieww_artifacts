//! The vault as a treemap — what was handed over, by class, by bytes.
//!
//! The Activity tab is the audit log: every grant, run, skip and failure,
//! line by line. What it never said is *what those files were* — eleven
//! "Access granted" entries could be eleven 4 MB JPEGs or two hours of
//! 4K video, and the log reads the same either way. This card is that
//! answer, as a picture: every class in the selection gets a rect sized by
//! the bytes it actually occupies, laid out by the squarified treemap from
//! `vieww-dataviz::hierarchy` (`stratify` + `sum` + `sort_by_value` +
//! `treemap` — the D3 `d3.treemap` pipeline), so the shape of the vault's
//! storage is legible before any number is read.
//!
//! The numbers are the engine's own: `Item::bytes` as the picker reported
//! them, aggregated by `AssetClass`, formatted by the engine's
//! `fmt_bytes` — the same formatter every other figure on every other
//! screen goes through, so the card and the plan screen can never disagree
//! about what "2.1 MB" means.
//!
//! Drawn with plain positioned boxes rather than a painter: a treemap is
//! rectangles, and rectangles are exactly what the widget layer is for —
//! labels inside them can then wrap, scale and take focus like text
//! anywhere else, which a painted label cannot.

use vavlt_engine::{fmt_bytes, Item};
use vieww::foundation::{Color, Constraints, Rect, Size, TextStyle};
use vieww::prelude::*;

use vieww_dataviz::hierarchy;

use crate::theme::VavltTheme;
use crate::ui;

/// The treemap's height. Fixed rather than a function of the class count,
/// because a treemap's readability is a function of aspect ratio, not of
/// how many classes happen to be present.
const MAP_HEIGHT: f32 = 168.0;

/// How much width a label needs before it is drawn at all.
const LABEL_MIN: f32 = 64.0;

/// One class's share of the vault.
#[derive(Debug, Clone)]
pub struct ClassShare {
    pub label: &'static str,
    pub files: usize,
    pub bytes: u64,
}

/// Aggregate a selection by class, largest byte share first.
#[must_use]
pub fn by_class(items: &[Item]) -> Vec<ClassShare> {
    let mut shares: Vec<ClassShare> = Vec::new();
    for item in items {
        if let Some(existing) = shares.iter_mut().find(|s| s.label == item.class.label()) {
            existing.files += 1;
            existing.bytes += item.bytes;
        } else {
            shares.push(ClassShare {
                label: item.class.label(),
                files: 1,
                bytes: item.bytes,
            });
        }
    }
    shares.sort_by_key(|s| std::cmp::Reverse(s.bytes));
    shares
}

/// A categorical colour for a class rect.
///
/// Twelve muted steps around the wheel, ordered so adjacent list entries
/// (which are adjacent by *size*, thanks to the sort) are also visually
/// distinct. None of them are the theme's accent — the accent means "an
/// action lives here" everywhere else in this app, and a storage block is
/// not an action.
fn class_color(index: usize, theme: &VavltTheme) -> Color {
    const WHEEL: [Color; 12] = [
        Color::rgb(0x6e, 0x8e, 0xb8),
        Color::rgb(0xb8, 0x8f, 0x6e),
        Color::rgb(0x7f, 0xb0, 0x8a),
        Color::rgb(0xb0, 0x7f, 0x8a),
        Color::rgb(0x8a, 0x83, 0xb0),
        Color::rgb(0x8a, 0xb0, 0xae),
        Color::rgb(0xb0, 0xa1, 0x7f),
        Color::rgb(0x93, 0x7f, 0xb0),
        Color::rgb(0x7f, 0x9e, 0xb0),
        Color::rgb(0xb0, 0x7f, 0x9e),
        Color::rgb(0xa4, 0xb0, 0x7f),
        Color::rgb(0x7f, 0x8c, 0xb0),
    ];
    let base = WHEEL[index % WHEEL.len()];
    // Wash toward the card surface so the blocks sit *in* the card rather
    // than on it, in either theme; a heavier wash in the light theme where
    // the card is bright and raw wheel steps would shout.
    if theme.dark {
        base.lerp(theme.colors.card, 0.18)
    } else {
        base.lerp(theme.colors.card, 0.42)
    }
}

/// The insight card: title, the treemap, and the one-line footer.
///
/// `None` when there is nothing to draw — no selection, or a selection whose
/// byte sizes are all zero (a picker that reports nothing is not a vault).
#[must_use]
pub fn vault_map(theme: &VavltTheme, items: &[Item]) -> Option<WidgetNode> {
    let shares = by_class(items);
    let total: u64 = shares.iter().map(|s| s.bytes).sum();
    if shares.is_empty() || total == 0 {
        return None;
    }

    let m = theme.metrics;

    // Stacked rather than shared-row: the figure style is the one the hero
    // numbers use, and a heading row that has to fit beside it wraps the
    // title one word per line — the title is the sentence, it gets the
    // width.
    let heading = Flex::column()
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .spacing(2.0)
        .push(Text::new("WHAT WAS HANDED OVER").style(theme.caption().bold()))
        .push(
            Text::new(format!(
                "{} files · {}",
                shares.iter().map(|s| s.files).sum::<usize>(),
                fmt_bytes(total)
            ))
            .style(theme.figure()),
        );

    let map = {
        // `LayoutBuilder`'s closure outlives this function, so it takes the
        // theme by value — `VavltTheme` is `Copy`, like every other screen
        // that closes over one.
        let theme = *theme;
        LayoutBuilder::new(move |constraints: Constraints| {
        let width = if constraints.has_bounded_width() {
            constraints.max_width
        } else {
            320.0
        };
        let area = Rect::from_origin_size(
            vieww::foundation::Offset::ZERO,
            Size::new(width.max(0.0), MAP_HEIGHT),
        );

        // The stratified rows: one root, one child per class. `sum` folds
        // the root's total; `sort_by_value` orders the squarify pass.
        #[allow(clippy::cast_precision_loss)]
        let rows: Vec<(&str, Option<&str>, f64)> = std::iter::once(("vault", None, 0.0))
            .chain(shares.iter().map(|s| (s.label, Some("vault"), s.bytes as f64)))
            .collect();
        let Ok(mut tree) = hierarchy::Hierarchy::stratify(&rows) else {
            return SizedBox::shrink().into();
        };
        tree.sum();
        tree.sort_by_value();
        let rects = hierarchy::treemap(&tree, area, 2.0);

        // Node 0 is the root (stratify puts it first); children follow in
        // row order, which is `shares` order — the hierarchy's internal sort
        // changes the drawing, not the indices.
        let mut map = Stack::new().fit(StackFit::Expand);
        for (index, share) in shares.iter().enumerate() {
            let Some(rect) = rects.get(index + 1) else { continue };
            let (w, h) = (rect.width(), rect.height());
            if w <= 0.0 || h <= 0.0 {
                continue;
            }
            let color = class_color(index, &theme);
            let label = if w >= LABEL_MIN && h >= 52.0 {
                format!("{}\n{} · {}", share.label, share.files, fmt_bytes(share.bytes))
            } else if w >= LABEL_MIN && h >= 30.0 {
                format!("{}\n{}", share.label, fmt_bytes(share.bytes))
            } else if w >= 30.0 && h >= 16.0 {
                share.label.to_string()
            } else {
                String::new()
            };

            let block: WidgetNode = if label.is_empty() {
                ColoredBox::new(color).into()
            } else {
                Container::new()
                    .color(color)
                    .padding(EdgeInsets::all(6.0))
                    .alignment(vieww::foundation::Alignment::TOP_LEFT)
                    .child(
                        Text::new(label)
                            .style(
                                TextStyle::new(if h >= 52.0 { 11.0 } else { 10.0 })
                                    .color(theme.colors.fg)
                                    .line_height(1.3),
                            ),
                    )
                    .into()
            };

            map = map.push(
                Positioned::new()
                    .left(rect.origin().dx)
                    .top(rect.origin().dy)
                    .width(w)
                    .height(h)
                    .child(block),
            );
        }
        map.into()
        })
    };

    Some(
        ui::card(
            theme,
            Flex::column()
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .spacing(m.sp_2)
                .push(heading)
                .push(SizedBox::height(MAP_HEIGHT).child(map))
                .push(
                    Text::new(
                        "sized by bytes as the picker reported them — the plan screen says what \
                         the tiers would do to them",
                    )
                    .style(theme.caption()),
                ),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::FormFactor;

    fn item(name: &str, bytes: u64) -> Item {
        Item::new("content://test", name.to_string(), bytes)
    }

    #[test]
    fn an_empty_selection_aggregates_to_nothing() {
        assert!(by_class(&[]).is_empty());
        assert!(vault_map(&VavltTheme::new(true, FormFactor::Phone), &[]).is_none());
    }

    #[test]
    fn files_of_one_class_fold_into_one_share() {
        let shares = by_class(&[item("a.jpg", 100), item("b.jpg", 150)]);
        assert_eq!(shares.len(), 1);
        assert_eq!(shares[0].label, "JPEG");
        assert_eq!(shares[0].files, 2);
        assert_eq!(shares[0].bytes, 250);
    }

    #[test]
    fn classes_are_sorted_largest_byte_share_first() {
        let shares = by_class(&[
            item("a.jpg", 100),
            item("b.dng", 900),
            item("c.png", 400),
        ]);
        let labels: Vec<_> = shares.iter().map(|s| s.label).collect();
        assert_eq!(labels, ["RAW/DNG", "PNG", "JPEG"]);
    }

    #[test]
    fn the_treemap_covers_the_area_it_is_given() {
        // The layout itself is the dataviz crate's to test; what this app
        // owes is that the pipeline runs and produces one rect per class,
        // inside the given area.
        let shares = by_class(&[
            item("a.jpg", 100),
            item("b.dng", 900),
            item("c.png", 400),
            item("d.heic", 200),
        ]);
        let rows: Vec<(&str, Option<&str>, f64)> = std::iter::once(("vault", None, 0.0))
            .chain(shares.iter().map(|s| (s.label, Some("vault"), s.bytes as f64)))
            .collect();
        let mut tree = hierarchy::Hierarchy::stratify(&rows).expect("valid rows");
        tree.sum();
        tree.sort_by_value();
        let area = Rect::from_origin_size(
            vieww::foundation::Offset::ZERO,
            Size::new(320.0, MAP_HEIGHT),
        );
        let rects = hierarchy::treemap(&tree, area, 2.0);
        assert_eq!(rects.len(), shares.len() + 1, "root plus one per class");
        for (index, rect) in rects.iter().enumerate().skip(1) {
            assert!(rect.width() > 0.0, "class {index} has area");
            assert!(rect.height() > 0.0, "class {index} has area");
            assert!(rect.origin().dx >= 0.0 && rect.origin().dy >= 0.0);
        }
    }

    #[test]
    fn a_selection_of_zero_bytes_draws_no_card() {
        let theme = VavltTheme::new(true, FormFactor::Phone);
        assert!(vault_map(&theme, &[item("a.jpg", 0)]).is_none());
    }
}
