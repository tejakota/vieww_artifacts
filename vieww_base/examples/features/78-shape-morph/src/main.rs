//! Morphing stars and polygons — the `Star` and `Polygon` morph shapes,
//! photographed.
//!
//! Two `ShapeMorph`s on one triangle-wave clock: a five-point star into a
//! hexagon, and a circle into an eight-point star. Fixed-sample morphing
//! means any shape becomes any other — the point the new shapes exist to
//! make — and the ping-pong shows both directions.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use vieww_animation::Ticker;
use vieww_element::Signal;
use vieww_foundation::{Color, Size};
use vieww_widget::prelude::*;

const SPAN: f32 = 3.0;

#[derive(Debug)]
struct PingPong {
    progress: Signal<f32>,
    span: f32,
}

impl Ticker for PingPong {
    fn tick(&mut self, now: Duration) -> bool {
        let t = now.as_secs_f32().rem_euclid(self.span);
        let half = self.span / 2.0;
        let progress = if t < half {
            t / half
        } else {
            1.0 - (t - half) / half
        };
        self.progress.set(progress);
        true
    }

    fn is_animating(&self) -> bool {
        true
    }
}

/// The caller's widget: reads the signal, hands plain values to the
/// morphs — the split `SplitText` and every other driven widget makes.
#[derive(Debug)]
struct Morphs {
    /// The clock's registration is *weak*: the frame driver keeps a
    /// `Weak` to it, so somebody must own it for the frames to advance.
    /// This field is that somebody — the tree holds the widget, the
    /// widget holds the clock, the clock keeps ticking. The same trick
    /// example 05's comment explains for `TimelinePlayer`.
    _clock: Rc<RefCell<PingPong>>,
    progress: Signal<f32>,
}

impl Widget for Morphs {
    fn debug_name(&self) -> &'static str {
        "Morphs"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let progress = self.progress.get().clamp(0.0, 1.0);
        Flex::row()
            .spacing(28.0)
            .children(children![
                ShapeMorph::new(Star::new(5), Polygon::new(6))
                    .progress(progress)
                    .size(Size::new(160.0, 160.0))
                    .child(Container::new().color(Color::rgb(58, 122, 246))),
                ShapeMorph::new(Circle, Star::new(8).inner(0.62))
                    .progress(progress)
                    .size(Size::new(160.0, 160.0))
                    .child(Container::new().color(Color::rgb(46, 145, 80))),
            ])
            .into()
    }
}

widget_node_from!(Morphs);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    feature_harness::launch("78 — shape morph", Size::new(640.0, 280.0), |d| {
        let runtime = d.elements().runtime().clone();
        let progress = runtime.signal(0.0_f32);
        let clock = Rc::new(RefCell::new(PingPong {
            progress: progress.clone(),
            span: SPAN,
        }));
        d.tickers().add(&clock);
        feature_harness::set_page(
            d,
            Container::new()
                .color(Color::WHITE)
                .padding(EdgeInsets::all(24.0))
                .child(Morphs {
                    progress,
                    _clock: clock,
                }),
        );
    })
}
