//! The five LFO waveforms, plotted and breathing — the
//! `vieww-animation::lfo` module, photographed.
//!
//! Each of the five shapes as a plotted line (sine, square, triangle, saw,
//! reverse-saw), and one circle whose radius follows a 0.4 Hz sine — the
//! "make it breathe" that is the whole reason the type exists. All of it
//! is a function of the frame clock, and the strip shows the phase
//! advancing.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use vieww_animation::lfo::{Lfo, Wave};
use vieww_animation::Ticker;
use vieww_element::Signal;
use vieww_foundation::{Color, Offset, Size};
use vieww_widget::prelude::*;

const SPAN: f32 = 2.5;

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
struct Waves {
    /// The clock's registration is *weak*: the frame driver keeps a
    /// `Weak` to it, so somebody must own it for the frames to advance.
    /// This field is that somebody — the tree holds the widget, the
    /// widget holds the clock, the clock keeps ticking. The same trick
    /// example 05's comment explains for `TimelinePlayer`.
    _clock: Rc<RefCell<LoopClock>>,
    time: Signal<f32>,
}

impl Widget for Waves {
    fn debug_name(&self) -> &'static str {
        "Waves"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let t = self.time.get();
        CustomPaint::sized(Size::new(560.0, 360.0), WavePainter { t }).into()
    }
}

widget_node_from!(Waves);

struct WavePainter {
    t: f32,
}

impl CustomPainter for WavePainter {
    fn paint(&self, size: Size) -> Vec<DrawInstruction> {
        let mut out = Vec::new();
        let waves = [
            (Wave::Sine, "sine"),
            (Wave::Square, "square"),
            (Wave::Triangle, "triangle"),
            (Wave::Saw, "saw"),
            (Wave::ReverseSaw, "reverse saw"),
        ];
        let row_height = (size.height - 90.0) / 5.0;
        let left = 56.0;
        let width = size.width - left - 20.0;

        for (i, (wave, name)) in waves.iter().enumerate() {
            let mid = 12.0 + f32::from(i as u16) * row_height + row_height / 2.0;
            // The axis the wave oscillates around.
            out.push(DrawInstruction::DrawLine {
                from: Offset::new(left, mid),
                to: Offset::new(left + width, mid),
                color: Color::rgb(225, 230, 240),
                width: 0.5,
            });
            let lfo = Lfo::new(*wave, 1.0);
            let mut previous = Offset::new(left, mid - lfo.at(Duration::ZERO) * row_height * 0.42);
            for step in 1..=180 {
                let x = left + width * step as f32 / 180.0;
                let at = Duration::from_secs_f32(step as f32 / 180.0 * SPAN);
                let y = mid - lfo.at(at) * row_height * 0.42;
                let point = Offset::new(x, y);
                out.push(DrawInstruction::DrawLine {
                    from: previous,
                    to: point,
                    color: Color::rgb(58, 122, 246),
                    width: 2.0,
                });
                previous = point;
            }
            // The phase marker: where on this wave *now* is.
            let now_x = left + width * (self.t / SPAN);
            let now_y = mid - lfo.at(Duration::from_secs_f32(self.t)) * row_height * 0.42;
            out.push(DrawInstruction::FillCircle {
                center: Offset::new(now_x, now_y),
                radius: 4.0,
                color: Color::rgb(220, 40, 90),
            });
            let _ = name;
        }

        // The breathing circle: radius on a 0.4 Hz sine, centred at the
        // bottom-right — the demo's whole point, made once as a picture.
        let breath = Lfo::new(Wave::Sine, 0.4).amplitude(18.0).center(30.0);
        let radius = breath.at(Duration::from_secs_f32(self.t));
        out.push(DrawInstruction::FillCircle {
            center: Offset::new(size.width - 64.0, size.height - 64.0),
            radius: radius.max(4.0),
            color: Color::rgb(58, 122, 246).with_alpha(0xB0),
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
    feature_harness::launch("62 — lfo", Size::new(620.0, 420.0), |d| {
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
                .child(Waves { time, _clock: clock }),
        );
    })
}
