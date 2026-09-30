//! A skeletal rig, skinned and waving — the gap-closure pass's
//! `vieww-animation::skeletal` module, photographed.
//!
//! Two bones (an arm and a forearm, parented), one skin: a strip of
//! vertices bound across both, so bending the elbow *deforms* the strip
//! rather than replacing it. The clip animates the forearm's rotation with
//! eased keyframes; the frame clock owns the time.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use vieww_animation::skeletal::{BoneTransform, SkeletalClip, Skeleton, Skin};
use vieww_animation::{Keyframe, Keyframes, Ticker};
use vieww_element::Signal;
use vieww_foundation::{Color, Offset, Size};
use vieww_widget::prelude::*;

const SPAN: f32 = 2.0;

/// The rig: `arm` from the shoulder, `forearm` from the elbow.
fn skeleton() -> Skeleton {
    Skeleton::new()
        .bone("arm", None, BoneTransform::from_translation(Offset::new(0.0, 0.0)))
        .bone(
            "forearm",
            Some("arm"),
            BoneTransform::from_translation(Offset::new(90.0, 0.0)),
        )
}

/// The skin: a strip of nine vertices along the two bones, bound with
/// weights that cross over at the elbow, so the bend stretches the middle
/// of the strip rather than tearing it.
fn skin(rig: &Skeleton) -> Skin {
    let mut skin = Skin::bind(rig, &rig.bind_pose());
    for i in 0..9 {
        let t = i as f32 / 8.0;
        // x runs 0..=180 (the two bones' 90+90); the weight crosses from
        // the arm to the forearm across the elbow at x = 90.
        let x = t * 180.0;
        let (arm, forearm) = if x <= 90.0 {
            let w = x / 90.0;
            (1.0 - w * 0.5, w * 0.5)
        } else {
            let w = (x - 90.0) / 90.0;
            (0.5 - w * 0.5, 0.5 + w * 0.5)
        };
        skin = skin.vertex(
            vieww_animation::skeletal::Vertex::new(Offset::new(x, 0.0))
                .binding(0, arm)
                .binding(1, forearm),
        );
    }
    skin
}

/// The forearm's local transform at rotation `theta`: still offset 90 from
/// the arm's origin, because a clip track replaces the bone's *whole*
/// local transform — a track that sets only the rotation moves the elbow
/// onto the shoulder, and a demo that did that would be demonstrating a
/// bug.
fn forearm_local(theta: f32) -> BoneTransform {
    BoneTransform {
        translation: Offset::new(90.0, 0.0),
        rotation: theta,
        scale: (1.0, 1.0),
    }
}

/// The clip: the forearm swings with eased keyframes, loops forever.
fn clip() -> SkeletalClip {
    SkeletalClip::new().bone(
        "forearm",
        Keyframes::new(forearm_local(0.0))
            .with(Keyframe::to(0.9, forearm_local(-1.5)))
            .with(Keyframe::to(1.9, forearm_local(0.35)))
            .looping(),
    )
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
        CustomPaint::sized(Size::new(420.0, 300.0), RigPainter { t }).into()
    }
}

widget_node_from!(Arm);

struct RigPainter {
    t: f32,
}

impl CustomPainter for RigPainter {
    fn paint(&self, size: Size) -> Vec<DrawInstruction> {
        let rig = skeleton();
        let skin = skin(&rig);
        let pose = clip().pose(&rig, Duration::from_secs_f32(self.t));
        let world = rig.world(&pose);
        let points = skin.apply(&world);

        let origin = Offset::new(size.width * 0.22, size.height * 0.42);
        let to = |p: &Offset| Offset::new(origin.dx + p.dx, origin.dy + p.dy);

        let mut out = Vec::new();

        // The skinned strip, as its outline and its spine: deformation you
        // can *see* is the point of a skin, and a filled outline of a
        // 2-wide strip shows the stretching at the elbow directly.
        for thickness_sign in [18.0_f32, -18.0] {
            let edge: Vec<Offset> = points
                .iter()
                .map(|p| to(&Offset::new(p.dx, p.dy + thickness_sign)))
                .collect();
            for pair in edge.windows(2) {
                out.push(DrawInstruction::DrawLine {
                    from: pair[0],
                    to: pair[1],
                    color: Color::rgb(58, 122, 246),
                    width: 2.0,
                });
            }
        }
        // Connect the two edges, so the strip is closed.
        for point in &points {
            let x = point.dx;
            out.push(DrawInstruction::DrawLine {
                from: to(&Offset::new(x, 18.0)),
                to: to(&Offset::new(x, -18.0)),
                color: Color::rgb(58, 122, 246).with_alpha(0x80),
                width: 1.0,
            });
        }
        // The spine — the skinned midline, one line through the strip.
        for pair in points.windows(2) {
            out.push(DrawInstruction::DrawLine {
                from: to(&pair[0]),
                to: to(&pair[1]),
                color: Color::rgb(20, 60, 140),
                width: 1.5,
            });
        }

        // The bones, drawn over the skin they move: shoulder, elbow, tip.
        let joints = [
            world[0].apply(Offset::new(0.0, 0.0)),
            world[0].apply(Offset::new(90.0, 0.0)),
            world[1].apply(Offset::new(90.0, 0.0)),
        ];
        out.push(DrawInstruction::DrawLine {
            from: to(&joints[0]),
            to: to(&joints[1]),
            color: Color::rgb(24, 28, 40),
            width: 3.0,
        });
        out.push(DrawInstruction::DrawLine {
            from: to(&joints[1]),
            to: to(&joints[2]),
            color: Color::rgb(24, 28, 40),
            width: 3.0,
        });
        for joint in joints {
            out.push(DrawInstruction::FillCircle {
                center: to(&joint),
                radius: 5.0,
                color: Color::rgb(24, 28, 40),
            });
        }
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
    feature_harness::launch("58 — skeletal", Size::new(460.0, 340.0), |d| {
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
