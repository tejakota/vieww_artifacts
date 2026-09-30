//! A two-bone arm, reaching — the `solve_two_bone` IK, photographed.
//!
//! One arm (upper 90, forearm 80), a target that walks a fixed loop of
//! three points with eased keyframes, and the analytic solution drawn
//! every frame: the arm, the joint, the effector on the target, and the
//! target itself. When the target leaves the reachable ring the arm
//! extends toward it and the marker says so — the honest unreachable
//! answer, not an error.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use vieww_animation::skeletal::{solve_two_bone, Bend};
use vieww_animation::{Keyframe, Keyframes, Ticker};
use vieww_element::Signal;
use vieww_foundation::{Color, Offset, Size};
use vieww_widget::prelude::*;

const SPAN: f32 = 3.0;
const LENGTHS: (f32, f32) = (95.0, 80.0);

/// The target's loop: near, far (out of reach), and high — the reachable
/// and the unreachable case, alternated, so the extension behaviour is on
/// the same strip as the tracking.
fn target_track() -> Keyframes<(f32, f32)> {
    Keyframes::new((40.0, -40.0))
        .with(Keyframe::to(0.8, (200.0, 60.0)))
        .with(Keyframe::to(1.6, (30.0, 110.0)))
        .with(Keyframe::to(2.99, (40.0, -40.0)))
}

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

#[derive(Debug)]
struct Arm {
    /// The clock's registration is *weak*: the frame driver keeps a
    /// `Weak` to it, so somebody must own it for the frames to advance.
    /// This field is that somebody — the tree holds the widget, the
    /// widget holds the clock, the clock keeps ticking. The same trick
    /// example 05's comment explains for `TimelinePlayer`.
    _clock: Rc<RefCell<LoopClock>>,
    time: Signal<f32>,
}

impl Widget for Arm {
    fn debug_name(&self) -> &'static str {
        "Arm"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let t = self.time.get();
        CustomPaint::sized(Size::new(420.0, 320.0), ArmPainter { t }).into()
    }
}

widget_node_from!(Arm);

struct ArmPainter {
    t: f32,
}

impl CustomPainter for ArmPainter {
    fn paint(&self, size: Size) -> Vec<DrawInstruction> {
        let root = Offset::new(50.0, size.height * 0.55);
        let (tx, ty) = target_track().at(Duration::from_secs_f32(self.t));
        let target = Offset::new(root.dx + tx, root.dy - ty);

        let solution = solve_two_bone(root, LENGTHS, target, Bend::CounterClockwise);
        let elbow = solution.joint(root, LENGTHS);
        let effector = solution.effector;

        let mut out = Vec::new();
        // The reachable ring: where the arm's tip can actually get to.
        out.push(DrawInstruction::FillCircle {
            center: root,
            radius: LENGTHS.0 + LENGTHS.1,
            color: Color::rgb(58, 122, 246).with_alpha(0x0A),
        });
        // The arm: two bones, thick.
        out.push(DrawInstruction::DrawLine {
            from: root,
            to: elbow,
            color: Color::rgb(24, 28, 40),
            width: 6.0,
        });
        out.push(DrawInstruction::DrawLine {
            from: elbow,
            to: effector,
            color: Color::rgb(24, 28, 40),
            width: 6.0,
        });
        // The joints.
        out.push(DrawInstruction::FillCircle {
            center: root,
            radius: 6.0,
            color: Color::rgb(24, 28, 40),
        });
        out.push(DrawInstruction::FillCircle {
            center: elbow,
            radius: 5.0,
            color: Color::rgb(24, 28, 40),
        });
        // The effector, and the target: on-target they coincide; out of
        // reach the effector sits on the ring and the marker stays where
        // the target actually is.
        out.push(DrawInstruction::FillCircle {
            center: effector,
            radius: 5.0,
            color: Color::rgb(58, 122, 246),
        });
        out.push(DrawInstruction::DrawLine {
            from: Offset::new(target.dx - 8.0, target.dy - 8.0),
            to: Offset::new(target.dx + 8.0, target.dy + 8.0),
            color: if solution.reachable {
                Color::rgb(46, 145, 80)
            } else {
                Color::rgb(200, 60, 90)
            },
            width: 3.0,
        });
        out.push(DrawInstruction::DrawLine {
            from: Offset::new(target.dx - 8.0, target.dy + 8.0),
            to: Offset::new(target.dx + 8.0, target.dy - 8.0),
            color: if solution.reachable {
                Color::rgb(46, 145, 80)
            } else {
                Color::rgb(200, 60, 90)
            },
            width: 3.0,
        });
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
    feature_harness::launch("79 — two-bone ik", Size::new(460.0, 380.0), |d| {
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
                .child(Arm { time, _clock: clock }),
        );
    })
}
