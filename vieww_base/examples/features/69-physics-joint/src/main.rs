//! A pendulum chain on distance joints — the `vieww-physics` joint
//! support, photographed.
//!
//! One fixed anchor, three bobs, three rods: a chain that swings under
//! gravity and keeps every link at its rest length while it does. Beside
//! it, a soft tether converging on its rest length — the two stiffness
//! personalities the `Joint` type has, in one frame.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use vieww_animation::Ticker;
use vieww_element::Signal;
use vieww_foundation::{Color, Offset, Size};
use vieww_physics::{Body, Joint, Shape, World};
use vieww_widget::prelude::*;

#[derive(Clone, Debug, PartialEq)]
struct Snapshot {
    /// Anchors (fixed), drawn as pins.
    anchors: Vec<Offset>,
    /// Bob positions, in chain order.
    bobs: Vec<Offset>,
    /// The tether's end, converging on its rest circle.
    tether_end: Offset,
}

#[derive(Debug)]
struct JointClock {
    world: World,
    snapshot: Signal<Snapshot>,
}

impl Ticker for JointClock {
    fn tick(&mut self, _now: Duration) -> bool {
        self.world.step(1.0 / 60.0);
        let snapshot = read_out(&self.world);
        self.snapshot.set(snapshot);
        true
    }

    fn is_animating(&self) -> bool {
        true
    }
}

fn read_out(world: &World) -> Snapshot {
    Snapshot {
        anchors: vec![world.body(0).position, world.body(4).position],
        bobs: (1..=3).map(|i| world.body(i).position).collect(),
        tether_end: world.body(5).position,
    }
}

#[derive(Debug)]
struct Scene {
    /// The clock's registration is weak (see example 05's comment): this
    /// field is the strong reference that keeps it ticking, because the
    /// tree holds the widget and the widget holds the clock.
    _clock: Rc<RefCell<JointClock>>,
    snapshot: Signal<Snapshot>,
}

impl Widget for Scene {
    fn debug_name(&self) -> &'static str {
        "Scene"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let snapshot = self.snapshot.get();
        CustomPaint::sized(Size::new(560.0, 380.0), ScenePainter { snapshot }).into()
    }
}

widget_node_from!(Scene);

struct ScenePainter {
    snapshot: Snapshot,
}

impl CustomPainter for ScenePainter {
    fn paint(&self, size: Size) -> Vec<DrawInstruction> {
        let origin = Offset::new(size.width * 0.32, 40.0);
        let to = |p: &Offset| Offset::new(origin.dx + p.dx, origin.dy + p.dy);
        let mut out = Vec::new();

        // The chain: anchor → bob → bob → bob, as rods and pins.
        let mut from = self.snapshot.anchors[0];
        for bob in &self.snapshot.bobs {
            out.push(DrawInstruction::DrawLine {
                from: to(&from),
                to: to(bob),
                color: Color::rgb(24, 28, 40),
                width: 2.5,
            });
            out.push(DrawInstruction::FillCircle {
                center: to(bob),
                radius: 11.0,
                color: Color::rgb(58, 122, 246),
            });
            from = *bob;
        }
        out.push(DrawInstruction::FillCircle {
            center: to(&self.snapshot.anchors[0]),
            radius: 5.0,
            color: Color::rgb(24, 28, 40),
        });

        // The tether: a pin, a rest-length circle, the converging end.
        let pin = self.snapshot.anchors[1];
        let pin_at = to(&pin);
        out.push(DrawInstruction::FillCircle {
            center: pin_at,
            radius: 5.0,
            color: Color::rgb(24, 28, 40),
        });
        out.push(DrawInstruction::FillCircle {
            center: Offset::new(pin_at.dx + 90.0, pin_at.dy),
            radius: 90.0,
            color: Color::rgb(220, 228, 240),
        });
        let end = to(&self.snapshot.tether_end);
        out.push(DrawInstruction::DrawLine {
            from: pin_at,
            to: end,
            color: Color::rgb(200, 60, 110),
            width: 1.5,
        });
        out.push(DrawInstruction::FillCircle {
            center: end,
            radius: 9.0,
            color: Color::rgb(200, 60, 110),
        });
        out
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn should_repaint(&self, previous: &dyn CustomPainter) -> bool {
        match previous.as_any().downcast_ref::<Self>() {
            Some(prev) => self.snapshot != prev.snapshot,
            None => true,
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    feature_harness::launch("69 — physics joints", Size::new(620.0, 440.0), |d| {
        let runtime = d.elements().runtime().clone();
        let snapshot = runtime.signal(Snapshot {
            anchors: Vec::new(),
            bobs: Vec::new(),
            tether_end: Offset::new(0.0, 0.0),
        });

        let mut world = World::with_gravity(Offset::new(0.0, 700.0));
        // The chain: anchor at the origin, three bobs each 70 below the
        // last, joined by 70-length rods — swinging starts from the
        // staggered initial x, and gravity does the rest.
        world.add_body(Body::fixed(Offset::new(0.0, 0.0), Shape::circle(2.0)));
        let mut previous = 0;
        let mut x = 90.0;
        for i in 1..=3 {
            let bob = world.add_body(Body::dynamic(
                Offset::new(x, f32::from(i as u16) * 70.0),
                Shape::circle(6.0),
                1.0,
            ));
            world.add_joint(Joint::rod(previous, bob, 70.0));
            previous = bob;
            x *= 0.4;
        }
        // The tether: a pin 300 right of the chain, a bob 80 out at 45°,
        // a soft joint pulling it onto the 90-length rest circle.
        let pin = world.add_body(Body::fixed(Offset::new(300.0, 40.0), Shape::circle(2.0)));
        let drifting = world.add_body(Body::dynamic(
            Offset::new(390.0, 110.0),
            Shape::circle(5.0),
            1.0,
        ));
        world.add_joint(Joint::tether(pin, drifting, 90.0, 0.08));

        snapshot.set(read_out(&world));
        // `world` moves into the clock here; `read_out` above already ran.
        let clock = Rc::new(RefCell::new(JointClock {
            world,
            snapshot: snapshot.clone(),
        }));
        d.tickers().add(&clock);
        feature_harness::set_page(
            d,
            Container::new()
                .color(Color::WHITE)
                .padding(EdgeInsets::all(20.0))
                .child(Scene {
                    snapshot,
                    _clock: clock,
                }),
        );
    })
}
