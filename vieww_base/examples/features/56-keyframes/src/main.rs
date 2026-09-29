//! Keyframed tracks, staggered and looping — the gap-closure pass's
//! `vieww-animation::keyframe` module, photographed.
//!
//! Four bars, four [`Keyframes`] tracks, one clock. Each track holds its
//! own delay (staggered keyframe times), and every track loops, so the
//! strip shows the cascade arriving and then restarting — GSAP's
//! `repeat: -1` with `stagger`, in one type.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use vieww_animation::{Keyframe, Keyframes, Ticker};
use vieww_element::Signal;
use vieww_foundation::{Color, Rect, Size};
use vieww_widget::prelude::*;

/// One full cycle of the cascade, in seconds.
const SPAN: f32 = 2.4;

/// The clock every time-driven example shares: a signal that holds the
/// wrapped seconds, written by a ticker so the frame loop owns the timing
/// and the tree only ever reads it.
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

/// The four staggered tracks. Built once; sampled every frame.
fn tracks() -> Vec<Keyframes<f32>> {
    (0..4)
        .map(|i| {
            let delay = 0.18 * i as f32;
            Keyframes::new(0.06)
                .with(Keyframe::to(delay + 0.0, 0.06))
                .with(Keyframe::to(delay + 0.45, 0.55 + 0.12 * i as f32))
                .with(Keyframe::to(delay + 1.2, 0.1))
                .looping()
        })
        .collect()
}

#[derive(Debug)]
struct Cascade {
    /// The clock's registration is *weak*: the frame driver keeps a
    /// `Weak` to it, so somebody must own it for the frames to advance.
    /// This field is that somebody — the tree holds the widget, the
    /// widget holds the clock, the clock keeps ticking. The same trick
    /// example 05's comment explains for `TimelinePlayer`.
    _clock: Rc<RefCell<LoopClock>>,
    time: Signal<f32>,
}

impl Widget for Cascade {
    fn debug_name(&self) -> &'static str {
        "Cascade"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let t = self.time.get();
        CustomPaint::sized(
            Size::new(560.0, 260.0),
            CascadePainter {
                t,
                tracks: tracks(),
            },
        )
        .into()
    }
}

widget_node_from!(Cascade);

struct CascadePainter {
    t: f32,
    tracks: Vec<Keyframes<f32>>,
}

impl CustomPainter for CascadePainter {
    fn paint(&self, size: Size) -> Vec<DrawInstruction> {
        let bar_width = (size.width - 30.0 - 4.0 * 12.0) / 4.0;
        let mut out = Vec::new();
        for (i, track) in self.tracks.iter().enumerate() {
            let value = track.at(Duration::from_secs_f32(self.t));
            let height = value * (size.height - 30.0);
            let x = 30.0 + i as f32 * (bar_width + 12.0);
            out.push(DrawInstruction::FillRoundedRect {
                rect: Rect::new(
                    x,
                    size.height - 20.0 - height,
                    x + bar_width,
                    size.height - 20.0,
                ),
                radius: 4.0,
                color: Color::rgb(58, 122, 246),
            });
            // The track's own delay, on the baseline, so the stagger is
            // visible as data rather than only as timing.
            out.push(DrawInstruction::FillRect {
                rect: Rect::new(x, size.height - 18.0, x + bar_width, size.height - 16.0),
                color: Color::rgb(200, 210, 225),
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
    feature_harness::launch("56 — keyframes", Size::new(620.0, 320.0), |d| {
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
                .child(Cascade { time, _clock: clock }),
        );
    })
}
