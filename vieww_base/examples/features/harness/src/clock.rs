//! A looping clock and a widget rebuilt from it.
//!
//! Most animated feature examples are "a picture that is a function of
//! time". Each used to carry its own `LoopClock` ticker and its own
//! composed widget holding the ticker alive; [`clocked`] is that pattern
//! once: it owns a signal, a ticker that writes `now mod span` into it every
//! frame, and a widget whose `build` calls the example's closure with the
//! time. The widget keeps the ticker alive (the frame driver only holds a
//! weak reference to it).

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
use std::time::Duration;

use vieww_animation::Ticker;
use vieww_element::Signal;
use vieww_render::FrameDriver;
use vieww_widget::prelude::*;

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

/// A widget rebuilt every frame from the looping time. See [`clocked`].
#[derive(Clone)]
pub struct Clocked {
    _clock: Rc<RefCell<LoopClock>>,
    time: Signal<f32>,
    build: Rc<dyn Fn(f32) -> WidgetNode>,
}

impl fmt::Debug for Clocked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Clocked").field("time", &self.time.peek()).finish()
    }
}

impl Widget for Clocked {
    fn debug_name(&self) -> &'static str {
        "Clocked"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        (self.build)(self.time.get())
    }
}

widget_node_from!(Clocked);

/// A widget built by `build(t)` with `t` looping over `0..span` seconds.
pub fn clocked(driver: &mut FrameDriver, span: f32, build: impl Fn(f32) -> WidgetNode + 'static) -> Clocked {
    let time = driver.elements().runtime().signal(0.0_f32);
    let clock = Rc::new(RefCell::new(LoopClock {
        time: time.clone(),
        span,
    }));
    driver.tickers().add(&clock);
    Clocked {
        _clock: clock,
        time,
        build: Rc::new(build),
    }
}

/// A heading and a caption line, in the examples' house style.
#[must_use]
pub fn caption(title: &str, detail: &str, dark: bool) -> WidgetNode {
    let (a, b) = if dark {
        (Color::rgb(236, 238, 245), Color::rgb(150, 156, 175))
    } else {
        (Color::rgb(24, 28, 40), Color::rgb(96, 102, 120))
    };
    Flex::column()
        .children(children![Text::new(title).size(17.0).bold().color(a), Text::new(detail).size(12.0).color(b),])
        .into()
}
