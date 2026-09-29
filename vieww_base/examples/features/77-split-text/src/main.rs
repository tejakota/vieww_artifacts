//! Per-character and per-word cascades — the `SplitText` widget,
//! photographed.
//!
//! Two reveals on one clock: a character cascade above ("the letters
//! arrive one at a time") and a word cascade below (the calmer variant
//! long copy needs). The progress signal is the same for both — the
//! difference is entirely the split — and the strip shows the wave
//! travelling through each.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use vieww_animation::Ticker;
use vieww_element::Signal;
use vieww_foundation::{Color, Size, TextStyle};
use vieww_widget::prelude::*;

const SPAN: f32 = 2.6;

#[derive(Debug)]
struct LoopClock {
    progress: Signal<f32>,
    span: f32,
}

impl Ticker for LoopClock {
    fn tick(&mut self, now: Duration) -> bool {
        // A triangle wave: reveal forwards, then unreveal backwards, so
        // the loop shows both halves of the cascade's life.
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

/// The caller's own widget: reads the signal in `build` and hands the
/// number to `SplitText` — the same split every value widget makes.
#[derive(Debug)]
struct Reveals {
    /// The clock's registration is *weak*: the frame driver keeps a
    /// `Weak` to it, so somebody must own it for the frames to advance.
    /// This field is that somebody — the tree holds the widget, the
    /// widget holds the clock, the clock keeps ticking. The same trick
    /// example 05's comment explains for `TimelinePlayer`.
    _clock: Rc<RefCell<LoopClock>>,
    progress: Signal<f32>,
}

impl Widget for Reveals {
    fn debug_name(&self) -> &'static str {
        "Reveals"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let progress = self.progress.get();
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .spacing(30.0)
            .children(children![
                Text::new("per character:").style(TextStyle {
                    size: 12.0,
                    color: Color::rgb(130, 140, 160),
                    ..TextStyle::default()
                }),
                SplitText::new("Design is intelligence made visible", progress)
                    .stagger(0.75)
                    .rise(10.0)
                    .size(22.0)
                    .color(Color::rgb(24, 28, 40)),
                Text::new("per word:").style(TextStyle {
                    size: 12.0,
                    color: Color::rgb(130, 140, 160),
                    ..TextStyle::default()
                }),
                SplitText::new("A user interface is like a joke — if you have to explain it, it is not that good.", progress)
                    .words()
                    .stagger(0.6)
                    .rise(14.0)
                    .size(16.0)
                    .color(Color::rgb(58, 122, 246)),
            ])
            .into()
    }
}

widget_node_from!(Reveals);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    feature_harness::launch("77 — split text", Size::new(640.0, 340.0), |d| {
        let runtime = d.elements().runtime().clone();
        let progress = runtime.signal(0.0_f32);
        let clock = Rc::new(RefCell::new(LoopClock {
            progress: progress.clone(),
            span: SPAN,
        }));
        d.tickers().add(&clock);
        feature_harness::set_page(
            d,
            Container::new()
                .color(Color::WHITE)
                .padding(EdgeInsets::all(24.0))
                .child(Reveals { progress, _clock: clock }),
        );
    })
}
