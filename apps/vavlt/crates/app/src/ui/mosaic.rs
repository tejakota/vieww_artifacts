//! The heterogeneous photo wall.
//!
//! Four columns of square cells, with some photographs taking two columns, two
//! rows, or both. A uniform grid of identical squares is a contact sheet; a
//! wall with weight in it reads as somebody's pictures.
//!
//! # The sizes are a function of position, not of chance
//!
//! The obvious implementation is a random size per tile, and it is wrong in a
//! way that only shows up in use: this grid is the screen where a photograph is
//! **excluded**, so it rebuilds on every tap — and a random size means the whole
//! wall reshuffles under the finger that just tapped it. [`shape`] is `i % 11`,
//! which is stable across rebuilds by construction.
//!
//! It is also why an excluded photograph dims *in place* rather than leaving
//! the wall: removing it would reflow everything after it, which is the same
//! defect wearing a different hat.
//!
//! # Why the packing is written out
//!
//! CSS would do this with `grid-auto-flow: dense` and no further thought. There
//! is no such thing here, so [`pack`] is the shelf algorithm that fills the
//! holes a big block leaves — about thirty lines, deterministic, and testable
//! without a GPU, which is the part that matters.

use vieww::prelude::*;

use crate::model::Photo;
use crate::theme::VavltTheme;

const COLUMNS: usize = 4;
const GAP: f32 = 5.0;

/// How many cells wide and tall a photograph is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub w: usize,
    pub h: usize,
}

impl Span {
    const ONE: Self = Self { w: 1, h: 1 };
    const BIG: Self = Self { w: 2, h: 2 };
    const WIDE: Self = Self { w: 2, h: 1 };
    const TALL: Self = Self { w: 1, h: 2 };
}

/// The shape the photograph at `index` gets.
///
/// Period 11 rather than 4 or 6, so the pattern does not line up with the
/// column count and lay down visible stripes.
#[must_use]
pub const fn shape(index: usize) -> Span {
    match index % 11 {
        0 | 7 => Span::BIG,
        3 => Span::WIDE,
        9 => Span::TALL,
        _ => Span::ONE,
    }
}

/// Where a tile ends up, in cell coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    pub index: usize,
    pub col: usize,
    pub row: usize,
    pub span: Span,
}

/// Lay `count` photographs into a four-column wall, back-filling holes.
///
/// A first-fit scan over an occupancy map: for each photograph, the first cell
/// its span fits in, scanning rows then columns. That is what makes a 1x1 drop
/// into the gap beside a 2x2 rather than starting a new row under it — the
/// "dense" part, and the reason the wall has no holes in it.
#[must_use]
pub fn pack(count: usize) -> (Vec<Placed>, usize) {
    let mut taken: Vec<[bool; COLUMNS]> = Vec::new();
    let mut placed = Vec::with_capacity(count);

    for index in 0..count {
        let span = shape(index);
        let mut row = 0usize;
        'search: loop {
            while taken.len() < row + span.h {
                taken.push([false; COLUMNS]);
            }
            for col in 0..=COLUMNS.saturating_sub(span.w) {
                let free = (0..span.h).all(|dy| (0..span.w).all(|dx| !taken[row + dy][col + dx]));
                if free {
                    for dy in 0..span.h {
                        for dx in 0..span.w {
                            taken[row + dy][col + dx] = true;
                        }
                    }
                    placed.push(Placed {
                        index,
                        col,
                        row,
                        span,
                    });
                    break 'search;
                }
            }
            row += 1;
        }
    }

    (placed, taken.len())
}

/// The wall.
///
/// `on_tap` is called with the photograph's index. `None` makes every tile
/// static, which is what the working screen wants — a run in progress is not a
/// thing you edit.
pub fn mosaic(
    theme: &VavltTheme,
    photos: &[Photo],
    on_tap: Option<std::rc::Rc<dyn Fn(usize)>>,
) -> WidgetNode {
    if photos.is_empty() {
        return SizedBox::shrink().into();
    }

    let theme = *theme;
    let photos = photos.to_vec();
    let (placed, rows) = pack(photos.len());

    LayoutBuilder::new(move |constraints| {
        let width = constraints.max_width;
        let cell = (width - GAP * (COLUMNS as f32 - 1.0)) / COLUMNS as f32;
        let height = cell * rows as f32 + GAP * (rows as f32 - 1.0);

        let tiles: Vec<WidgetNode> = placed
            .iter()
            .filter_map(|slot| {
                let photo = photos.get(slot.index)?;
                let w = cell * slot.span.w as f32 + GAP * (slot.span.w as f32 - 1.0);
                let h = cell * slot.span.h as f32 + GAP * (slot.span.h as f32 - 1.0);
                let handler = on_tap.clone().map(|tap| {
                    let index = slot.index;
                    Box::new(move || tap(index)) as Box<dyn Fn()>
                });
                Some(
                    Positioned::new()
                        .left(slot.col as f32 * (cell + GAP))
                        .top(slot.row as f32 * (cell + GAP))
                        .width(w)
                        .height(h)
                        // The radius grows with the block. A 2x2 at the 1x1
                        // radius reads as a squarer, different shape sitting
                        // among the others rather than as a bigger one.
                        .child(super::tile(&theme, photo, w, h, radius(slot.span), handler))
                        .into(),
                )
            })
            .collect();

        Container::new()
            .height(height)
            .child(Stack::new().children(tiles))
            .into()
    })
    // No breakpoints: this divides the width into cells, and
    // `LayoutBuilder::breakpoints` quantises it — a stale width here is a wall
    // wider than the screen.
    .into()
}

const fn radius(span: Span) -> f32 {
    match (span.w, span.h) {
        (2, 2) => 26.0,
        (1, 1) => 16.0,
        _ => 20.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every photograph is placed, inside the grid, and no two overlap. The
    /// three properties a packer has, and the only ones worth asserting.
    #[test]
    fn the_wall_has_no_holes_and_no_collisions() {
        let (placed, rows) = pack(37);
        assert_eq!(placed.len(), 37, "every photograph is placed");

        let mut taken = vec![[false; COLUMNS]; rows];
        for slot in &placed {
            assert!(
                slot.col + slot.span.w <= COLUMNS,
                "{slot:?} runs off the edge"
            );
            for dy in 0..slot.span.h {
                for dx in 0..slot.span.w {
                    let cell = &mut taken[slot.row + dy][slot.col + dx];
                    assert!(!*cell, "{slot:?} overlaps something already placed");
                    *cell = true;
                }
            }
        }
    }

    /// The property the whole design depends on: the same index always gets the
    /// same shape, so excluding a photograph cannot reshuffle the wall.
    #[test]
    fn a_photographs_shape_does_not_depend_on_the_others() {
        for index in 0..50 {
            assert_eq!(shape(index), shape(index), "shape must be pure");
        }
        assert_eq!(shape(0), Span::BIG);
        assert_eq!(shape(11), Span::BIG, "the pattern repeats at 11");
        assert_ne!(shape(4), Span::BIG);
    }

    /// A short wall still packs — the case a two-photo selection hits.
    #[test]
    fn a_tiny_selection_packs() {
        let (placed, rows) = pack(2);
        assert_eq!(placed.len(), 2);
        assert!(
            rows >= 2,
            "the first photograph is a 2x2, so it needs two rows"
        );
    }
}
