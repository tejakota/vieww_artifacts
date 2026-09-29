//! Perlin noise and fBm — the `vieww-animation::noise` module,
//! photographed.
//!
//! The flowing-field trick the module exists for: a 2D slice of *3D* noise
//! with time as the third coordinate, so a coherent field drifts rather
//! than flickers (the z of `noise3(x, y, t)` is the clock, and neighbouring
//! instants are neighbouring slices). Below it, a 1D trace of fBm — the
//! banded, self-similar signal a terrain profile or a coastline has.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use vieww_animation::noise::Perlin;
use vieww_animation::Ticker;
use vieww_element::Signal;
use vieww_foundation::{Color, Offset, Rect, Size};
use vieww_widget::prelude::*;

const SPAN: f32 = 4.0;
const TILES: (usize, usize) = (56, 26);

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
struct Field {
    /// The clock's registration is *weak*: the frame driver keeps a
    /// `Weak` to it, so somebody must own it for the frames to advance.
    /// This field is that somebody — the tree holds the widget, the
    /// widget holds the clock, the clock keeps ticking. The same trick
    /// example 05's comment explains for `TimelinePlayer`.
    _clock: Rc<RefCell<LoopClock>>,
    time: Signal<f32>,
}

impl Widget for Field {
    fn debug_name(&self) -> &'static str {
        "Field"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let t = self.time.get();
        CustomPaint::sized(Size::new(560.0, 340.0), FieldPainter { t }).into()
    }
}

widget_node_from!(Field);

/// Map a −1..=1 noise value to a sea-to-sky ramp: blue below, pale above.
fn ramp(value: f32) -> Color {
    let v = (value * 0.5 + 0.5).clamp(0.0, 1.0);
    // Two anchor colours, lerped by hand — the ramp is the *picture* half
    // of this example and belongs here rather than in the module.
    let deep = (12.0_f32, 26.0_f32, 66.0_f32);
    let high = (208.0_f32, 226.0_f32, 245.0_f32);
    let mix = |a: f32, b: f32| (a + (b - a) * v) as u8;
    Color::rgb(mix(deep.0, high.0), mix(deep.1, high.1), mix(deep.2, high.2))
}

struct FieldPainter {
    t: f32,
}

impl CustomPainter for FieldPainter {
    fn paint(&self, size: Size) -> Vec<DrawInstruction> {
        let noise = Perlin::from_seed(7);
        let mut out = Vec::with_capacity(TILES.0 * TILES.1 + 64);

        // The flowing field: 3D noise, z = time.
        let tile_w = size.width / TILES.0 as f32;
        let tile_h = (size.height - 70.0) / TILES.1 as f32;
        for gy in 0..TILES.1 {
            for gx in 0..TILES.0 {
                let value = noise.noise3(
                    f32::from(gx as u16) * 0.11,
                    f32::from(gy as u16) * 0.13,
                    self.t * 0.35,
                );
                out.push(DrawInstruction::FillRect {
                    rect: Rect::new(
                        f32::from(gx as u16) * tile_w,
                        f32::from(gy as u16) * tile_h,
                        f32::from(gx as u16) * tile_w + tile_w + 0.6,
                        f32::from(gy as u16) * tile_h + tile_h + 0.6,
                    ),
                    color: ramp(value),
                });
            }
        }

        // The 1D fBm trace: five octaves, drawn as a line across a band
        // pinned to the bottom.
        let trace_top = size.height - 64.0;
        let mut previous = Offset::new(0.0, trace_top);
        for i in 0..=160 {
            let x = i as f32 / 160.0 * size.width;
            let value = noise.fbm(x * 0.02, 0.0, 5, 0.55, 2.1);
            let y = trace_top + 30.0 - value * 24.0;
            let point = Offset::new(x, y);
            out.push(DrawInstruction::DrawLine {
                from: previous,
                to: point,
                color: Color::rgb(255, 214, 64),
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
        match previous.as_any().downcast_ref::<Self>() {
            Some(prev) => self.t != prev.t,
            None => true,
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    feature_harness::launch("61 — noise", Size::new(620.0, 400.0), |d| {
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
                .color(Color::rgb(10, 12, 20))
                .padding(EdgeInsets::all(20.0))
                .child(Field { time, _clock: clock }),
        );
    })
}
