//! `CanvasView` — a [`Stage`] in the widget tree (Konva's `<Stage>` in
//! react-konva).
//!
//! The stage's vector content is replayed through a
//! [`Painting`](vieww_widget::Painting); text shapes become real
//! [`Text`](vieww_widget::Text) widgets placed with the node's full
//! transform, so they are shaped and rasterised by the text stack like any
//! other text. An optional [`Transformer`] draws its handles on top.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use vieww_foundation::{Size, Sketchbook};
use vieww_widget::prelude::*;

use crate::{Stage, Transformer};

/// See the [module docs](self).
#[derive(Clone)]
pub struct CanvasView {
    stage: Rc<RefCell<Stage>>,
    transformer: Option<Transformer>,
}

impl fmt::Debug for CanvasView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CanvasView").field("stage", &self.stage.borrow()).finish_non_exhaustive()
    }
}

impl CanvasView {
    #[must_use]
    pub fn new(stage: Rc<RefCell<Stage>>) -> Self {
        Self { stage, transformer: None }
    }

    /// Draw a transformer's handles over the stage.
    #[must_use]
    pub fn with_transformer(mut self, t: Transformer) -> Self {
        self.transformer = Some(t);
        self
    }
}

#[derive(Debug)]
struct Recorded(Sketchbook);

impl Painter for Recorded {
    fn paint(&self, book: &mut Sketchbook, _size: Size) {
        for item in self.0.items() {
            book.push(item.clone());
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Widget for CanvasView {
    fn debug_name(&self) -> &'static str {
        "CanvasView"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let stage = self.stage.borrow();
        let size = Size::new(stage.width, stage.height);
        let (mut book, texts) = stage.render();
        if let Some(t) = &self.transformer {
            for item in t.draw(&stage).into_items() {
                book.push(item);
            }
        }
        let mut layers: Vec<WidgetNode> = vec![Painting::sized(size, Recorded(book)).into()];
        for t in texts {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let color = t.color.with_alpha((f32::from(t.color.a) * t.opacity).round().clamp(0.0, 255.0) as u8);
            layers.push(
                Positioned::new()
                    .left(0.0)
                    .top(0.0)
                    .child(Transformed::new(t.transform).child(Text::new(t.text).size(t.size).color(color)))
                    .into(),
            );
        }
        SizedBox::from_size(size)
            .child(Clip::new(ClipShape::Rect).child(Stack::new().children(layers)))
            .into()
    }
}

widget_node_from!(CanvasView);
