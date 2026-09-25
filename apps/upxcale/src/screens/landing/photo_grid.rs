//! The heterogeneous photo masonry.
//!
//! Mirrors `prototype/screens/landing/photo-grid.css`.
//!
//! vieww mapping: `Scrollable` + `Flex` + `Flexible` + `AspectRatio`, with
//! `EmptyState` in place of the grid when there is nothing to show.
//!
//! This file owns the scroll container, the two-column packing, and the
//! bottom inset that keeps the last row clear of the Render button. It does
//! not own the appearance of a tile ([`super::photo_tile`]).
//!
//! # Why this is not `GridView`
//!
//! `GridView` is a fixed column count at a *uniform* cell extent, and the whole
//! point of this screen is that the cells are not uniform — a 9:16 tile beside a
//! 4:3 one is what makes the grid read as photographs rather than as thumbnails.
//! `ListView::variable` is the other candidate and it is one-dimensional.
//!
//! So the packing is done here, in the way a masonry is always done: walk the
//! photographs in order and put each into whichever column is currently
//! shortest. The prototype leaned on CSS `column-count: 2`, which is the same
//! algorithm with the browser running it.
//!
//! That costs a full pass over the list per build, which is fine at a dozen
//! photographs and would not be at ten thousand. The honest note for a future
//! reader is that the fix at that size is not a cleverer pack — it is to
//! virtualise, and virtualising a masonry needs the column heights precomputed
//! and cached, which is a different piece of work than this one.

use std::rc::Rc;

use vieww_foundation::{Axis, DragDetails, EdgeInsets, Image as Pixels};
use vieww_widget::prelude::*;
// `Handler<T> = Rc<dyn Fn(T)>` is the shape every vieww callback that carries a
// value takes. It is at the crate root rather than in the prelude.
use vieww_widget::{widget_node_from, Handler};

use crate::icons;
use crate::photos::Photo;
use crate::state::GridItem;

use super::photo_tile::PhotoTile;

/// Columns in the masonry. Two, because a phone at 414 logical points gives
/// each column about 200 points and a photograph narrower than that stops being
/// a photograph and becomes a thumbnail.
pub const COLUMNS: usize = 2;

/// Clearance under the last row so the Render button never covers a tile.
///
/// The button is 52 high and sits 24 from the bottom; this is that plus a gap.
pub const FAB_CLEARANCE: f32 = 96.0;

/// What the grid needs about one photograph in order to draw it.
///
/// Resolved by the screen rather than looked up here, so this widget touches
/// neither the asset bundle nor the `RefCell`s inside
/// [`Library`](crate::photos::Library) — it is handed pixels and draws them.
pub struct Cell {
    pub item: GridItem,
    pub photo: &'static Photo,
    pub image: Option<Pixels>,
}

impl std::fmt::Debug for Cell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cell")
            .field("id", &self.item.id)
            .field("upscaled", &self.item.upscaled)
            .finish()
    }
}

/// The masonry.
pub struct PhotoGrid {
    pub cells: Vec<Cell>,
    /// Current scroll offset, owned by the screen —`Scrollable` is controlled.
    pub offset: f32,
    /// Handed straight to `Scrollable`; these come off the screen's
    /// `ScrollController`.
    pub on_drag: Handler<DragDetails>,
    pub on_drag_end: Handler<DragDetails>,
    pub on_extents: Handler<ScrollExtents>,
    /// Tapping a tile. The screen decides what that means.
    pub on_tap: Rc<dyn Fn(&'static Photo)>,
}

impl Widget for PhotoGrid {
    fn debug_name(&self) -> &'static str {
        "PhotoGrid"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, ctx: &BuildContext) -> WidgetNode {
        let theme_data = *ThemeData::of(ctx);
        let gap = theme_data.metrics.gap;

        if self.cells.is_empty() {
            // vieww paints an `EmptyState` in place of the grid, exactly as the
            // prototype's `[data-empty="true"]` rule hid the masonry and
            // revealed the empty block.
            return Container::new()
                .padding(EdgeInsets::all(theme_data.metrics.gap * 3.0))
                .alignment(vieww_foundation::Alignment::CENTER)
                .child(
                    EmptyState::new(icons::photo(), "No photos yet")
                        .description("Pick a few photos to upscale. They'll show up here."),
                )
                .into();
        }

        // ── pack ────────────────────────────────────────────────────────────
        // Shortest-column-first. `height` is tracked in units of column width
        // — a tile of ratio r occupies 1/r of its column's width in height —
        // so the packing never needs to know how wide a column actually is,
        // and stays correct when the phone rotates.
        let mut columns: [Vec<WidgetNode>; COLUMNS] = std::array::from_fn(|_| Vec::new());
        let mut heights = [0.0_f32; COLUMNS];

        for cell in &self.cells {
            let shortest = heights
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.total_cmp(b))
                .map_or(0, |(index, _)| index);

            let photo = cell.photo;
            let on_tap = Rc::clone(&self.on_tap);
            columns[shortest].push(
                PhotoTile {
                    photo,
                    image: cell.image.clone(),
                    upscaled: cell.item.upscaled,
                    fresh: cell.item.fresh,
                    selected: false,
                    on_tap: Rc::new(move || on_tap(photo)),
                }
                .into(),
            );
            heights[shortest] += 1.0 / photo.extent.ratio();
        }

        let packed = Flex::row()
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .spacing(gap)
            .children(
                columns
                    .into_iter()
                    .map(|children| {
                        // `Flexible::expanded(1)` on each column is what makes
                        // them equal width regardless of what is in them; a
                        // plain child would size to its content.
                        Flexible::expanded(1)
                            .child(
                                Flex::column()
                                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                                    .spacing(gap)
                                    .children(children),
                            )
                            .into()
                    })
                    .collect::<Vec<WidgetNode>>(),
            );

        Scrollable::new(Axis::Vertical, self.offset)
            .on_drag(Rc::clone(&self.on_drag))
            .on_drag_end(Rc::clone(&self.on_drag_end))
            .on_extents(Rc::clone(&self.on_extents))
            .child(
                Container::new()
                    .padding(EdgeInsets::only(gap, gap, gap, FAB_CLEARANCE))
                    .child(packed),
            )
            .into()
    }
}

widget_node_from!(PhotoGrid);

// `Widget` requires `Debug`, and these structs hold `Rc<dyn Fn>` callbacks,
// which are not. Hand-written rather than derived, and each prints the fields
// that identify *which* instance this is — a tree dump saying `PhotoTile` a
// dozen times is no use, one saying `PhotoTile { id: "a4", upscaled: true }`
// is.
impl std::fmt::Debug for PhotoGrid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PhotoGrid")
            .field("cells", &self.cells.len())
            .field("offset", &self.offset)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::photos::{Extent, CATALOGUE};

    /// The pack must put every photograph somewhere, and must not leave one
    /// column obviously taller than the other — that is the whole visual
    /// contract of a masonry.
    #[test]
    fn packing_balances_the_columns() {
        let mut heights = [0.0_f32; COLUMNS];
        let mut placed = 0;
        for photo in CATALOGUE {
            let shortest = heights
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.total_cmp(b))
                .map_or(0, |(i, _)| i);
            heights[shortest] += 1.0 / photo.extent.ratio();
            placed += 1;
        }
        assert_eq!(placed, CATALOGUE.len());

        let tallest = heights.iter().copied().fold(f32::MIN, f32::max);
        let shortest = heights.iter().copied().fold(f32::MAX, f32::min);
        // One tile's worth of slack is the most a greedy pack can leave.
        let tallest_tile = 1.0 / Extent::Tall.ratio();
        assert!(
            tallest - shortest <= tallest_tile,
            "columns differ by {} which is more than one tile ({tallest_tile})",
            tallest - shortest
        );
    }
}
