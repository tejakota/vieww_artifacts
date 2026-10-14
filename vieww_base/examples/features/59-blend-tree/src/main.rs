//! A 1D blend tree, driven by a parameter that itself moves — the
//! `vieww-animation::blend_tree` module, photographed.
//!
//! Three gaits (idle, walk, run) as keyframed tracks, blended by one
//! parameter that ping-pongs across its range. The three children are
//! drawn as ghosts under the blended pose, so the picture shows *which
//! children are winning* — the blend, not just its result.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use vieww_animation::blend_tree::BlendTree1;
use vieww_animation::{Keyframe, Keyframes, Ticker};
use vieww_element::Signal;
use vieww_foundation::{Color, Offset, Rect, Size};
use vieww_widget::prelude::*;

const SPAN: f32 = 3.0;

#[derive(Debug)]
struct LoopClock {
    time: Signal<f32>,
    span: f32,
}

impl Ticker for LoopClock {
    fn tick(&mut self, now: Duration) -> bool {
        self.time.set(now.as_secs_f32().rem_euclid(self.span));
        true
    }

    fn is_animating(&self) -> bool {
        true
    }
}

/// The parameter track: 0 → 1 → 0 over the whole span, so the strip sweeps
/// across every blend the tree can produce.
fn parameter() -> Keyframes<f32> {
    Keyframes::new(0.0)
        .with(Keyframe::to(1.5, 1.0))
        .with(Keyframe::to(2.99, 0.0))
}

/// The tree: idle (small, slow), walk (mid), run (large, fast) — the
/// classic gait-blend. Each child's track loops at its own tempo, which is
/// why the ghosts move differently from each other.
fn tree() -> BlendTree1<f32> {
    let idle = Keyframes::new(6.0)
        .with(Keyframe::to(0.9, 10.0))
        .with(Keyframe::to(1.8, 6.0))
        .looping();
    let walk = Keyframes::new(16.0)
        .with(Keyframe::to(0.45, 34.0))
        .with(Keyframe::to(0.9, 16.0))
        .looping();
    let run = Keyframes::new(40.0)
        .with(Keyframe::to(0.28, 92.0))
        .with(Keyframe::to(0.56, 40.0))
        .looping();
    BlendTree1::new(0.0, idle).child(0.45, walk).child(1.0, run)
}

#[derive(Debug)]
struct Gaits {
    /// The clock's registration is *weak*: the frame driver keeps a
    /// `Weak` to it, so somebody must own it for the frames to advance.
    /// This field is that somebody — the tree holds the widget, the
    /// widget holds the clock, the clock keeps ticking. The same trick
    /// example 05's comment explains for `TimelinePlayer`.
    _clock: Rc<RefCell<LoopClock>>,
    time: Signal<f32>,
}

impl Widget for Gaits {
    fn debug_name(&self) -> &'static str {
        "Gaits"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let t = self.time.get();
        CustomPaint::sized(Size::new(560.0, 240.0), GaitPainter { t }).into()
    }
}

widget_node_from!(Gaits);

struct GaitPainter {
    t: f32,
}

impl CustomPainter for GaitPainter {
    fn paint(&self, size: Size) -> Vec<DrawInstruction> {
        let elapsed = Duration::from_secs_f32(self.t);
        let p = parameter().at(elapsed).clamp(0.0, 1.0);
        let tree = tree();
        let blended = tree.at(p, elapsed);

        let track_top = 60.0;
        let track_height = size.height - track_top - 24.0;
        let bar = |value: f32, x: f32, color: Color, alpha: u8| DrawInstruction::FillRoundedRect {
            rect: Rect::new(x, track_top, x + 26.0, track_top + value * track_height),
            radius: 3.0,
            color: color.with_alpha(alpha),
        };

        let mut out = Vec::new();
        // The three children as ghosts, at their own sampled poses.
        for (i, child) in tree.children().iter().enumerate() {
            let value = child.track.at(elapsed).clamp(0.0, 100.0);
            let x = 40.0 + i as f32 * 150.0;
            out.push(bar(value, x, Color::rgb(160, 170, 190), 0x70));
            // The child's threshold, marked on the parameter gauge.
            let gauge_x = 20.0 + child.threshold * (size.width - 40.0);
            out.push(DrawInstruction::FillRect {
                rect: Rect::new(gauge_x - 1.0, 26.0, gauge_x + 1.0, 34.0),
                color: Color::rgb(160, 170, 190),
            });
        }
        // The parameter gauge, swept by the moving parameter.
        out.push(DrawInstruction::DrawLine {
            from: Offset::new(20.0, 30.0),
            to: Offset::new(size.width - 20.0, 30.0),
            color: Color::rgb(200, 210, 225),
            width: 1.0,
        });
        let marker_x = 20.0 + p * (size.width - 40.0);
        out.push(DrawInstruction::FillCircle {
            center: Offset::new(marker_x, 30.0),
            radius: 6.0,
            color: Color::rgb(24, 28, 40),
        });
        // The blended pose — the answer, over the ghosts it came from.
        out.push(bar(
            blended.clamp(0.0, 100.0),
            size.width - 60.0,
            Color::rgb(58, 122, 246),
            0xFF,
        ));
        out
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn should_repaint(&self, previous: &dyn CustomPainter) -> bool {
        match previous.as_any().downcast_ref::<Self>() {
            Some(prev) => self.t != prev.t,
            None => true,
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    feature_harness::launch("59 — blend tree", Size::new(620.0, 300.0), |d| {
        let runtime = d.elements().runtime().clone();
        let time = runtime.signal(0.0_f32);
        let clock = Rc::new(RefCell::new(LoopClock {
            time: time.clone(),
            span: SPAN,
        }));
        d.tickers().add(&clock);
        feature_harness::set_page(
            d,
            Container::new()
                .color(Color::WHITE)
                .padding(EdgeInsets::all(20.0))
                .child(Gaits {
                    time,
                    _clock: clock,
                }),
        );
    })
}
