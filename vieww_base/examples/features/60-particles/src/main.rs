//! A particle fountain — the `vieww-animation::particles` module,
//! photographed.
//!
//! 160 births per second, 2.2-second lives, gravity, a launch cone, a
//! size ramp and a colour ramp: every knob the type has, on one field.
//! The whole field is a *function of the clock* — nothing steps, nothing
//! accumulates — so the strip is five honest instants of one deterministic
//! fountain.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use vieww_animation::particles::ParticleField;
use vieww_animation::Ticker;
use vieww_element::Signal;
use vieww_foundation::{Color, Offset, Size};
use vieww_widget::prelude::*;

const SPAN: f32 = 2.4;

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
struct Fountain {
    /// The clock's registration is *weak*: the frame driver keeps a
    /// `Weak` to it, so somebody must own it for the frames to advance.
    /// This field is that somebody — the tree holds the widget, the
    /// widget holds the clock, the clock keeps ticking. The same trick
    /// example 05's comment explains for `TimelinePlayer`.
    _clock: Rc<RefCell<LoopClock>>,
    time: Signal<f32>,
}

impl Widget for Fountain {
    fn debug_name(&self) -> &'static str {
        "Fountain"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let t = self.time.get();
        CustomPaint::sized(Size::new(560.0, 400.0), FountainPainter { t }).into()
    }
}

widget_node_from!(Fountain);

struct FountainPainter {
    t: f32,
}

impl CustomPainter for FountainPainter {
    fn paint(&self, size: Size) -> Vec<DrawInstruction> {
        let field = ParticleField::new(160.0, 2.2)
            .seed(42)
            .origin(Offset::new(0.0, 0.0))
            .direction(-std::f32::consts::FRAC_PI_2)
            .spread(0.55)
            .speed(140.0, 330.0)
            .gravity(Offset::new(0.0, 260.0))
            .size(5.0, 0.8)
            .color(
                Color::rgba(255, 214, 64, 255),
                Color::rgba(255, 61, 61, 0),
            );

        let origin = Offset::new(size.width / 2.0, size.height - 36.0);
        let particles = field.sample(Duration::from_secs_f32(self.t));

        let mut out = Vec::with_capacity(particles.len() + 2);
        // The emitter, so the fountain's source is visible as geometry.
        out.push(DrawInstruction::FillRoundedRect {
            rect: vieww_foundation::Rect::new(
                origin.dx - 22.0,
                origin.dy - 6.0,
                origin.dx + 22.0,
                origin.dy + 8.0,
            ),
            radius: 5.0,
            color: Color::rgb(40, 44, 58),
        });
        for particle in &particles {
            out.push(DrawInstruction::FillCircle {
                center: Offset::new(
                    origin.dx + particle.position.dx,
                    origin.dy + particle.position.dy,
                ),
                radius: particle.size.max(0.5),
                color: particle.color,
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
    feature_harness::launch("60 — particles", Size::new(620.0, 460.0), |d| {
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
                .color(Color::rgb(16, 18, 28))
                .padding(EdgeInsets::all(20.0))
                .child(Fountain { time, _clock: clock }),
        );
    })
}
