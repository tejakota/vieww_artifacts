//! 2D physics, falling and settling — the gap-closure pass's
//! `vieww-physics` crate, photographed.
//!
//! A world stepped at a fixed 60 Hz from a clock a ticker owns: two balls
//! and a box dropped onto a floor, a box dropped onto the pile. The strip
//! shows the fall, the bounce (restitution split by the pair minimum), and
//! the rest — every position a pure function of the step count.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use vieww_animation::Ticker;
use vieww_element::Signal;
use vieww_foundation::{Color, Offset, Rect, Size};
use vieww_physics::{Body, Shape, World};
use vieww_widget::prelude::*;

/// Positions and sizes of every body, published per frame: the world stays
/// in the ticker (nothing steps it twice), and the painter only reads.
#[derive(Clone, Debug, PartialEq)]
struct Snapshot {
    circles: Vec<(Offset, f32, Color)>,
    boxes: Vec<(Offset, (f32, f32), Color)>,
}

#[derive(Debug)]
struct PhysicsClock {
    world: World,
    snapshot: Signal<Snapshot>,
}

impl Ticker for PhysicsClock {
    fn tick(&mut self, _now: Duration) -> bool {
        // Fixed timestep, always: the determinism this crate promises is a
        // function of *steps*, and tying the step count to the wall clock
        // would make a dropped frame a different simulation.
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
    let mut circles = Vec::new();
    let mut boxes = Vec::new();
    for body in world.bodies() {
        match body.shape {
            Shape::Circle { radius } => circles.push((
                body.position,
                radius,
                if body.is_dynamic() {
                    Color::rgb(58, 122, 246)
                } else {
                    Color::rgb(150, 158, 175)
                },
            )),
            Shape::Box {
                half_width,
                half_height,
            } => boxes.push((
                body.position,
                (half_width, half_height),
                if body.is_dynamic() {
                    Color::rgb(255, 171, 64)
                } else {
                    Color::rgb(150, 158, 175)
                },
            )),
        }
    }
    Snapshot { circles, boxes }
}

#[derive(Debug)]
struct Scene {
    /// The clock's registration is weak (see example 05's comment): this
    /// field is the strong reference that keeps it ticking, because the
    /// tree holds the widget and the widget holds the clock.
    _clock: Rc<RefCell<PhysicsClock>>,
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
        CustomPaint::sized(Size::new(560.0, 400.0), ScenePainter { snapshot }).into()
    }
}

widget_node_from!(Scene);

struct ScenePainter {
    snapshot: Snapshot,
}

impl CustomPainter for ScenePainter {
    fn paint(&self, size: Size) -> Vec<DrawInstruction> {
        let to = |position: &Offset| Offset::new(position.dx + size.width / 2.0, position.dy);
        let mut out = Vec::new();
        for (position, radius, color) in &self.snapshot.circles {
            out.push(DrawInstruction::FillCircle {
                center: to(position),
                radius: *radius,
                color: *color,
            });
        }
        for (position, (hw, hh), color) in &self.snapshot.boxes {
            let center = to(position);
            out.push(DrawInstruction::FillRoundedRect {
                rect: Rect::new(
                    center.dx - hw,
                    center.dy - hh,
                    center.dx + hw,
                    center.dy + hh,
                ),
                radius: 3.0,
                color: *color,
            });
        }
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
    feature_harness::launch("68 — physics", Size::new(620.0, 460.0), |d| {
        let runtime = d.elements().runtime().clone();
        let snapshot = runtime.signal(Snapshot {
            circles: Vec::new(),
            boxes: Vec::new(),
        });

        let mut world = World::with_gravity(Offset::new(0.0, 900.0));
        // The floor and walls, fixed: the scenery the pile lands in.
        world.add_body(Body::fixed(
            Offset::new(0.0, 196.0),
            Shape::box_shape(600.0, 6.0),
        ));
        world.add_body(Body::fixed(
            Offset::new(-250.0, 100.0),
            Shape::box_shape(6.0, 300.0),
        ));
        world.add_body(Body::fixed(
            Offset::new(250.0, 100.0),
            Shape::box_shape(6.0, 300.0),
        ));
        // The payload: two balls and two boxes, staggered so the pile
        // forms in the air rather than all at once on the floor.
        world.add_body(Body::dynamic(
            Offset::new(-60.0, -350.0),
            Shape::circle(16.0),
            1.0,
        ));
        world.add_body(Body::dynamic(
            Offset::new(40.0, -280.0),
            Shape::circle(12.0),
            1.0,
        ));
        world.add_body(Body::dynamic(
            Offset::new(0.0, -420.0),
            Shape::box_shape(30.0, 18.0),
            1.2,
        ));
        world.add_body(Body::dynamic(
            Offset::new(-30.0, -520.0),
            Shape::box_shape(24.0, 14.0),
            0.8,
        ));

        snapshot.set(read_out(&world));
        // `world` moves into the clock here; `read_out` above already ran.
        let clock = Rc::new(RefCell::new(PhysicsClock {
            world,
            snapshot: snapshot.clone(),
        }));
        d.tickers().add(&clock);
        feature_harness::set_page(
            d,
            Container::new()
                .color(Color::WHITE)
                .padding(EdgeInsets::all(20.0))
                .child(Scene { snapshot, _clock: clock }),
        );
    })
}
