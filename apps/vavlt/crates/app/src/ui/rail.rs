//! A horizontal row of photographs, and the heading over it.
//!
//! # Where a rail is right, and where it is not
//!
//! A rail is for a set whose *grouping is the information* — Proven, Not
//! counted, Failed, Left alone. One mixed grid of those hides exactly what the
//! outcome screen exists to show.
//!
//! It is the wrong tool where every member has to be reachable. The plan screen
//! is that case: excluding a photograph is the task, and a rail showing four of
//! a hundred puts the task behind a scroll. That screen uses
//! [`mosaic`](super::mosaic) instead. Netflix draws the same line — rows to
//! browse, a grid for search.
//!
//! # The last tile is cut on purpose
//!
//! A rail whose contents end flush with the gutter reads as a finished row
//! rather than one you can push. Letting the last one run under the screen edge
//! is the cheapest signal that it scrolls, and it costs nothing.

use std::rc::Rc;

use vieww::element::ScrollController;
use vieww::prelude::*;

use crate::model::Photo;
use crate::theme::VavltTheme;

const GAP: f32 = 8.0;
const TILE: f32 = 108.0;
const RADIUS: f32 = 22.0;

/// A section: a heading, an optional count on the right, an optional note, and
/// a body.
pub fn section(
    theme: &VavltTheme,
    title: &str,
    meta: Option<String>,
    note: Option<&str>,
    body: WidgetNode,
) -> WidgetNode {
    let m = theme.metrics;
    let mut head = children![];

    if !title.is_empty() {
        head.push(
            Flexible::expanded(1)
                .child(Text::new(title.to_string()).style(theme.group()))
                .into(),
        );
        if let Some(meta) = meta {
            head.push(Text::new(meta).style(theme.caption()).into());
        }
    }

    let mut column = children![];
    if !head.is_empty() {
        column.push(
            Padding::new(EdgeInsets::symmetric(m.gutter, 0.0))
                .child(
                    Flex::row()
                        .cross_axis_alignment(CrossAxisAlignment::Center)
                        .spacing(8.0)
                        .children(head),
                )
                .into(),
        );
    }
    if let Some(note) = note {
        column.push(
            Padding::new(EdgeInsets::only(m.gutter, 2.0, m.gutter, 0.0))
                .child(Text::new(note.to_string()).style(theme.caption()))
                .into(),
        );
    }
    column.push(
        Padding::new(EdgeInsets::only(0.0, 12.0, 0.0, 0.0))
            .child(body)
            .into(),
    );

    Padding::new(EdgeInsets::only(0.0, 26.0, 0.0, 0.0))
        .child(
            Flex::column()
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .main_axis_size(MainAxisSize::Min)
                .children(column),
        )
        .into()
}

/// The row of tiles itself.
pub fn rail(
    theme: &VavltTheme,
    photos: &[Photo],
    scroll: &ScrollController,
    on_tap: Option<Rc<dyn Fn(usize)>>,
) -> WidgetNode {
    if photos.is_empty() {
        return SizedBox::shrink().into();
    }

    let theme = *theme;
    let gutter = theme.metrics.gutter;
    let tiles: Vec<WidgetNode> = photos
        .iter()
        .enumerate()
        .map(|(index, photo)| {
            let handler = on_tap
                .clone()
                .map(|tap| Box::new(move || tap(index)) as Box<dyn Fn()>);
            super::tile(&theme, photo, TILE, TILE, RADIUS, handler)
        })
        .collect();

    Container::new()
        .height(TILE)
        .child(
            Scrollable::horizontal(scroll.offset())
                .on_drag(scroll.on_drag())
                .on_drag_end(scroll.on_drag_end())
                .on_extents(scroll.on_extents())
                .child(
                    // Padding only on the leading edge. A trailing gutter would
                    // let the row come to rest flush, which is the reading this
                    // deliberately avoids — see the module note.
                    Padding::new(EdgeInsets::only(gutter, 0.0, 0.0, 0.0)).child(
                        Flex::row()
                            .main_axis_size(MainAxisSize::Min)
                            .spacing(GAP)
                            .children(tiles),
                    ),
                ),
        )
        .into()
}
