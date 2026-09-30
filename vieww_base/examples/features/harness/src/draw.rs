//! Drawing helpers shared by the multi-panel feature examples: a closure
//! painter, captioned panels, a panel grid, and a few path shapes.

use std::f32::consts::TAU;
use std::fmt;
use std::rc::Rc;

use vieww_foundation::{Color, EdgeInsets, Offset, Path, Size, Sketchbook};
use vieww_widget::prelude::*;
use vieww_widget::{Painter, Painting};

/// A [`Painter`] from a closure.
#[derive(Clone)]
pub struct FnPainter(Rc<dyn Fn(&mut Sketchbook, Size)>);

impl fmt::Debug for FnPainter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FnPainter")
    }
}

impl Painter for FnPainter {
    fn paint(&self, book: &mut Sketchbook, size: Size) {
        (self.0)(book, size);
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// A sized painting from a closure.
pub fn paint(size: Size, f: impl Fn(&mut Sketchbook, Size) + 'static) -> WidgetNode {
    Painting::sized(size, FnPainter(Rc::new(f))).into()
}

/// Panel background.
pub const PANEL: Color = Color::rgb(24, 27, 36);
/// Page background.
pub const PAGE: Color = Color::rgb(14, 16, 24);
/// Primary ink.
pub const INK: Color = Color::rgb(230, 234, 245);
/// Secondary ink.
pub const DIM: Color = Color::rgb(130, 138, 158);
/// A palette for series.
pub const HUES: [Color; 6] = [
    Color::rgb(97, 175, 239),
    Color::rgb(240, 113, 120),
    Color::rgb(152, 195, 121),
    Color::rgb(229, 192, 123),
    Color::rgb(198, 120, 221),
    Color::rgb(86, 182, 194),
];

/// A dark panel: `body` above a two-line caption.
pub fn panel(title: &str, detail: &str, body: impl Into<WidgetNode>) -> WidgetNode {
    Container::new()
        .color(PANEL)
        .radius(8.0)
        .padding(EdgeInsets::all(8.0))
        .child(
            Flex::column()
                .spacing(4.0)
                .children(children![body.into(), Text::new(title).size(12.0).bold().color(INK), Text::new(detail).size(10.0).color(DIM),]),
        )
        .into()
}

/// A panel around a painting.
pub fn painted(title: &str, detail: &str, size: Size, f: impl Fn(&mut Sketchbook, Size) + 'static) -> WidgetNode {
    panel(title, detail, paint(size, f))
}

/// Rows of `cols` panels.
pub fn grid(cols: usize, items: Vec<WidgetNode>) -> WidgetNode {
    let mut rows = Vec::new();
    let mut it = items.into_iter().peekable();
    while it.peek().is_some() {
        let row: Vec<WidgetNode> = it.by_ref().take(cols).collect();
        rows.push(Flex::row().spacing(10.0).children(row).into());
    }
    Flex::column().spacing(10.0).children(rows).into()
}

/// A page: heading, subtitle, then `body`.
pub fn page(title: &str, subtitle: &str, body: WidgetNode) -> WidgetNode {
    Container::new()
        .color(PAGE)
        .padding(EdgeInsets::all(16.0))
        .child(Flex::column().spacing(10.0).children(children![crate::caption(title, subtitle, true), body]))
        .into()
}

/// A closed circle path.
#[must_use]
pub fn circle(c: Offset, r: f32) -> Path {
    polygon(c, r, 64, 0.0)
}

/// A regular polygon.
#[must_use]
pub fn polygon(c: Offset, r: f32, n: usize, rot: f32) -> Path {
    let mut p = Path::new();
    for i in 0..n {
        #[allow(clippy::cast_precision_loss)]
        let a = rot + i as f32 / n as f32 * TAU;
        let q = Offset::new(c.dx + r * a.cos(), c.dy + r * a.sin());
        if i == 0 {
            p.move_to(q);
        } else {
            p.line_to(q);
        }
    }
    p.close();
    p
}

/// A star.
#[must_use]
pub fn star(c: Offset, outer: f32, inner: f32, points: usize, rot: f32) -> Path {
    let mut p = Path::new();
    for i in 0..points * 2 {
        #[allow(clippy::cast_precision_loss)]
        let a = rot + i as f32 / (points * 2) as f32 * TAU;
        let r = if i % 2 == 0 { outer } else { inner };
        let q = Offset::new(c.dx + r * a.cos(), c.dy + r * a.sin());
        if i == 0 {
            p.move_to(q);
        } else {
            p.line_to(q);
        }
    }
    p.close();
    p
}

/// An open polyline.
#[must_use]
pub fn polyline(points: &[Offset]) -> Path {
    let mut p = Path::new();
    for (i, q) in points.iter().enumerate() {
        if i == 0 {
            p.move_to(*q);
        } else {
            p.line_to(*q);
        }
    }
    p
}

/// Plot `values` as a line inside `rect`-sized box at `origin`, scaled to
/// `lo..hi`.
#[must_use]
pub fn plot(values: &[f32], origin: Offset, size: Size, lo: f32, hi: f32) -> Path {
    let n = values.len().max(2) - 1;
    let pts: Vec<Offset> = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            #[allow(clippy::cast_precision_loss)]
            let x = origin.dx + i as f32 / n as f32 * size.width;
            let y = origin.dy + size.height - (v - lo) / (hi - lo).max(1e-6) * size.height;
            Offset::new(x, y)
        })
        .collect();
    polyline(&pts)
}

/// A rect from x, y, width, height (`Rect::new` takes left, top, right,
/// bottom).
#[must_use]
pub const fn xywh(x: f32, y: f32, w: f32, h: f32) -> vieww_foundation::Rect {
    vieww_foundation::Rect::new(x, y, x + w, y + h)
}
