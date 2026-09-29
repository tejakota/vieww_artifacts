//! A state machine animation, driven by events on a clock — the
//! gap-closure pass's `vieww-animation::state_machine` module,
//! photographed.
//!
//! A chip that rests, *pops* when "alerted", holds, and settles back:
//! states with guarded transitions and cross-fades where the outgoing
//! state keeps playing. The events fire from a schedule keyed to the same
//! clock the frames own, so the whole demo is deterministic and the strip
//! shows every state it passes through.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use vieww_animation::{Keyframe, Keyframes, StateMachine, Ticker};
use vieww_element::Signal;
use vieww_foundation::{Color, Size, TextStyle};
use vieww_widget::prelude::*;

/// One full schedule, in seconds.
const SPAN: f32 = 3.6;

/// When each event fires. The schedule *is* the demo's input: the machine
/// is only ever advanced by deltas and poked by these.
const SCHEDULE: &[(f32, &str)] = &[(0.5, "alert"), (1.7, "settle"), (2.7, "alert")];

/// The machine: rest (flat) → pop (overshoot, then hold) → hold (settle
/// onto the resting scale). Transitions cross-fade, so the machine's own
/// blending is part of the picture.
fn machine() -> StateMachine<f32> {
    let mut machine = StateMachine::new(
        "rest",
        Keyframes::new(0.0).with(Keyframe::to(0.4, 0.0)),
    );
    machine.state(
        "pop",
        Keyframes::new(0.0)
            .with(Keyframe::to(0.18, 1.6))
            .with(Keyframe::to(0.55, 0.9)),
    );
    machine.state_once(
        "hold",
        Keyframes::new(0.0).with(Keyframe::to(0.6, 0.55)),
    );
    // The guard is the event's name: "alert" pops, "settle" calms. The
    // `Duration` is the cross-fade, where both states keep playing.
    machine.transition("rest", "pop", Duration::from_millis(120), |event| event == "alert");
    machine.transition("pop", "hold", Duration::from_millis(250), |event| event == "settle");
    machine.transition("hold", "pop", Duration::from_millis(250), |event| event == "alert");
    machine
}

/// The ticker that owns the machine: advances it by frame deltas, fires
/// the schedule's events at their instants, publishes the sampled pose and
/// the current state name. The widget never steps anything — the module's
/// design, kept in the demo that shows it.
#[derive(Debug)]
struct MachineRelay {
    machine: StateMachine<f32>,
    pose: Signal<f32>,
    state: Signal<String>,
    /// Tracked from `event`'s return value: the machine itself does not
    /// publish its current state, and a demo that *guesses* it would be
    /// lying about the one thing it set out to show.
    current: &'static str,
    last: f32,
    fired: Vec<f32>,
}

impl Ticker for MachineRelay {
    fn tick(&mut self, now: Duration) -> bool {
        let t = now.as_secs_f32().rem_euclid(SPAN);
        // A wrap in `t` means the demo restarted: rebuild the machine so
        // the next cycle is identical — determinism across loops, not just
        // within one.
        if t < self.last {
            self.machine = machine();
            self.fired.clear();
            self.current = "rest";
        }
        self.machine
            .advance(Duration::from_secs_f32((t - self.last).max(0.0)));
        for &(at, event) in SCHEDULE {
            if self.last < at && at <= t && !self.fired.contains(&at) {
                if let Some(moved_to) = self.machine.event(event) {
                    self.current = moved_to;
                }
                self.fired.push(at);
            }
        }
        self.last = t;
        self.pose.set(self.machine.sample().max(0.0));
        self.state.set(self.current.to_string());
        true
    }

    fn is_animating(&self) -> bool {
        true
    }
}

#[derive(Debug)]
struct Chip {
    /// The clock's registration is weak (see example 05's comment): this
    /// field is the strong reference that keeps it ticking, because the
    /// tree holds the widget and the widget holds the clock.
    _clock: Rc<RefCell<MachineRelay>>,
    pose: Signal<f32>,
    state: Signal<String>,
}

impl Widget for Chip {
    fn debug_name(&self) -> &'static str {
        "Chip"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let scale = self.pose.get().max(0.0);
        let state = self.state.get();
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .spacing(26.0)
            .children(children![
                Text::new(format!("state: {state}")).style(TextStyle {
                    size: 13.0,
                    color: Color::rgb(90, 100, 120),
                    ..TextStyle::default()
                }),
                Container::new()
                    .color(Color::rgb(58, 122, 246))
                    .radius(8.0)
                    .child(SizedBox::from_size(Size::new(
                        150.0 + 70.0 * scale,
                        60.0 + 26.0 * scale,
                    ))),
                Text::new(format!("pose: {scale:.2}")).style(TextStyle {
                    size: 12.0,
                    color: Color::rgb(130, 140, 160),
                    ..TextStyle::default()
                }),
            ])
            .into()
    }
}

widget_node_from!(Chip);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    feature_harness::launch("57 — state machine", Size::new(560.0, 300.0), |d| {
        let runtime = d.elements().runtime().clone();
        let pose = runtime.signal(0.0_f32);
        let state = runtime.signal(String::from("rest"));
        let clock = Rc::new(RefCell::new(MachineRelay {
            machine: machine(),
            pose: pose.clone(),
            state: state.clone(),
            current: "rest",
            last: 0.0,
            fired: Vec::new(),
        }));
        d.tickers().add(&clock);
        feature_harness::set_page(
            d,
            Container::new()
                .color(Color::WHITE)
                .padding(EdgeInsets::all(20.0))
                .child(Chip { pose, state, _clock: clock }),
        );
    })
}
