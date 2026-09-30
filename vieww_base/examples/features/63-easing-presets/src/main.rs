//! The named easing library, plotted — GSAP's families as vieww `Curve`
//! constants, photographed.
//!
//! Twelve cards, twelve curves: the power family's in-out, sine, expo,
//! circ, back's overshoot, elastic's wobble and bounce's hops, plus a
//! stepped curve for the digital cases. A still is the right form for
//! this one — an easing curve is a *function*, and the picture of a
//! function is its graph.

use vieww_foundation::{Color, Offset, Size, TextStyle};
use vieww_widget::prelude::*;
use vieww_animation::Curve;

/// One card: the curve's graph, on a unit square.
#[derive(Clone)]
struct EaseCard {
    name: &'static str,
    curve: Curve,
}

impl EaseCard {
    const fn new(name: &'static str, curve: Curve) -> Self {
        Self { name, curve }
    }
}

impl std::fmt::Debug for EaseCard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EaseCard").field("name", &self.name).finish()
    }
}

impl Widget for EaseCard {
    fn debug_name(&self) -> &'static str {
        "EaseCard"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .spacing(6.0)
            .children(children![
                CustomPaint::sized(Size::new(120.0, 90.0), EaseGraph {
                    curve: self.curve,
                }),
                Text::new(self.name).style(TextStyle {
                    size: 11.0,
                    color: Color::rgb(90, 100, 120),
                    ..TextStyle::default()
                }),
            ])
            .into()
    }
}

widget_node_from!(EaseCard);

struct EaseGraph {
    curve: Curve,
}

impl CustomPainter for EaseGraph {
    fn paint(&self, size: Size) -> Vec<DrawInstruction> {
        let mut out = Vec::new();
        // The unit frame, so overshoots and undershoots read as leaving it.
        out.push(DrawInstruction::DrawLine {
            from: Offset::new(0.0, size.height),
            to: Offset::new(size.width, 0.0),
            color: Color::rgb(232, 236, 244),
            width: 1.0,
        });
        let mut previous = Offset::new(0.0, size.height);
        for step in 1..=80 {
            let t = step as f32 / 80.0;
            let eased = self.curve.transform(t);
            // Overshoot is the point of back and elastic: the eased value
            // may leave 0..=1, and the graph shows it leaving the frame.
            let x = t * size.width;
            let y = size.height - eased * size.height;
            let point = Offset::new(x, y);
            out.push(DrawInstruction::DrawLine {
                from: previous,
                to: point,
                color: Color::rgb(58, 122, 246),
                width: 2.0,
            });
            previous = point;
        }
        out
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn should_repaint(&self, previous: &dyn CustomPainter) -> bool {
        // Two cards of the same curve draw the same graph; the name is
        // outside the painter.
        match previous.as_any().downcast_ref::<Self>() {
            Some(prev) => {
                matches!((self.curve, prev.curve), (Curve::Custom { f: a, .. }, Curve::Custom { f: b, .. }) if std::ptr::eq(a as *const (), b as *const ()))
                    && format!("{:?}", self.curve) == format!("{:?}", prev.curve)
            }
            None => true,
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    feature_harness::launch("63 — easing library", Size::new(620.0, 460.0), |d| {
        let cards = [
            EaseCard::new("POWER2_IN_OUT", Curve::POWER2_IN_OUT),
            EaseCard::new("POWER4_OUT", Curve::POWER4_OUT),
            EaseCard::new("SINE_IN_OUT", Curve::SINE_IN_OUT),
            EaseCard::new("EXPO_OUT", Curve::EXPO_OUT),
            EaseCard::new("CIRC_IN_OUT", Curve::CIRC_IN_OUT),
            EaseCard::new("BACK_OUT", Curve::BACK_OUT),
            EaseCard::new("BACK_IN", Curve::BACK_IN),
            EaseCard::new("ELASTIC_OUT", Curve::ELASTIC_OUT),
            EaseCard::new("ELASTIC_IN", Curve::ELASTIC_IN),
            EaseCard::new("BOUNCE_OUT", Curve::BOUNCE_OUT),
            EaseCard::new("BOUNCE_IN_OUT", Curve::BOUNCE_IN_OUT),
            EaseCard::new("STEPS_8", Curve::STEPS_8),
        ];
        let mut grid = Flex::column().spacing(10.0);
        for row in cards.chunks(4) {
            let mut r = Flex::row().spacing(10.0);
            for card in row {
                r = r.push(card.clone());
            }
            grid = grid.push(r);
        }
        feature_harness::set_page(
            d,
            Container::new()
                .color(Color::WHITE)
                .padding(EdgeInsets::all(20.0))
                .child(grid),
        );
    })
}
