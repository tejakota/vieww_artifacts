//! State machines for animation: named states, guarded transitions, blending.
//!
//! A controller plays one value. A **state machine** plays one of several,
//! chosen by events — "walk", "hit", "celebrate" — and cross-fades between
//! them over a chosen duration. That shape is Unity's Animator, Godot's
//! `AnimationTree` and Rive's state machine, and it is the pattern the
//! framework's own comparison tables list as missing: a signal can hold
//! *which* animation should play, but something still has to own the
//! in-between — the frame where "walk" is 60% gone and "hit" is 40% arrived.
//!
//! # Why this is not a `match` statement
//!
//! ```text
//! match event { Event::Hit => play_hit(), Event::Walk => play_walk() }
//! ```
//!
//! A match owns *selection* and leaves *blending* to whoever calls it, every
//! frame, forever. It also cannot express "the walk→run transition takes
//! 200 ms but run→walk takes 50", because a match has no notion of the state
//! it came *from*. Both are properties of the transition, and a transition is
//! a thing this type has:
//!
//! * [`StateMachine::state`] declares a named state and what it plays;
//! * [`StateMachine::transition`] declares from→to, the cross-fade, and the
//!   guard that decides whether an event may take it;
//! * [`StateMachine::event`] offers an event, which either fires one
//!   transition, or does not — and both are results a caller can act on.
//!
//! # One clock, driven by deltas
//!
//! The machine owns its own clock and advances it by frame deltas through
//! `advance`; `sample` reads it. No method
//! here reads a wall clock, for the same reason as every other type in this
//! crate — the frame scheduler owns time, and a type that sampled its own
//! would drift from the frame it was drawn on. The clock also drives each
//! state's track: a state's animation begins when it is entered, and during a
//! cross-fade **both** tracks play — the target's clock starts at the fade's
//! start, so what lands is an animation already in motion, not one that
//! begins from zero the instant the fade completes.
//!
//! # The blending rule
//!
//! During a transition the sampled value is `from.lerp(to, alpha)` over the
//! transition's duration with [`Curve::EASE_IN_OUT`] — one rule, the same one
//! every cross-fade in this crate uses, rather than a per-transition
//! interpolator that has to be specified four times to be consistent once.
//!
//! # Why transitions do not auto-reverse
//!
//! A state machine that came from A into B and "just fades back" when B ends
//! is a machine with an edge nobody authored. Every transition here is
//! declared, in both directions if both are wanted, because the machine that
//! invents its own edges is the one whose behaviour cannot be read from its
//! definition.
//!
//! ```
//! use std::time::Duration;
//! use vieww_animation::{Keyframe, Keyframes, StateMachine};
//!
//! fn ms(ms: u64) -> Duration {
//!     Duration::from_millis(ms)
//! }
//!
//! // A button's glow: idle is dark, pressed is bright, and the press itself
//! // is a fast cross-fade between them.
//! let mut machine: StateMachine<f32> = StateMachine::new("idle", Keyframes::new(0.0));
//! machine.state("pressed", Keyframes::new(1.0));
//! machine.transition("idle", "pressed", ms(80), |event| event == "press");
//! machine.transition("pressed", "idle", ms(200), |event| event == "release");
//!
//! // The press lands: the fade to 1.0 begins.
//! assert_eq!(machine.event("press"), Some("pressed"));
//! machine.advance(ms(40));
//! assert_eq!(machine.sample(), 0.5, "half way through an 80 ms cross-fade");
//!
//! // An event with no matching transition is refused, not ignored-as-success.
//! assert_eq!(machine.event("release"), None, "not in a state that releases");
//! ```
//!
//! # What an animation *is* here
//!
//! Each state plays a [`Keyframes<T>`](crate::Keyframes) — the timeline this
//! crate grew in the module next door — looped by default, because a machine
//! state ("walk", "breathe", "spin") is a *mode*, and a mode that runs out
//! and stops is a state that silently ends. A one-shot state is
//! [`state_once`](StateMachine::state_once), whose last value holds when its
//! track ends, for the hit-flash that must not loop.

use std::time::Duration;

use crate::curve::Curve;
use crate::keyframe::Keyframes;
use crate::tween::Lerp;

/// A guard: does this event allow the transition it is offered to?
///
/// A closure rather than an enum of conditions, because every condition
/// language (Unity's "exit time", Rive's input comparisons) is a subset of
/// `Fn(&str) -> bool` with a parser attached, and the parse can be added
/// later without the type moving.
pub type Guard = Box<dyn Fn(&str) -> bool>;

/// One declared state of a [`StateMachine`]: a name and what it plays.
struct MachineState<T> {
    name: &'static str,
    frames: Keyframes<T>,
    /// `true` to loop the track (the default for a mode), `false` to hold the
    /// last value once the track ends (a one-shot).
    looped: bool,
}

/// A declared edge: from one state, to another, over a cross-fade, on events
/// a guard accepts.
struct MachineTransition {
    from: &'static str,
    to: &'static str,
    duration: Duration,
    guard: Guard,
}

/// A cross-fade in flight.
struct Flight {
    target: &'static str,
    /// When the fade began — also when the target state's clock began, which
    /// is what makes the landing land on an animation in motion.
    started: Duration,
}

/// A state machine that samples one `T`, cross-fading between states.
///
/// The machine's clock advances by [`advance`](Self::advance) and is read by
/// [`sample`](Self::sample). Nothing here reads a wall clock.
pub struct StateMachine<T> {
    states: Vec<MachineState<T>>,
    transitions: Vec<MachineTransition>,
    current: &'static str,
    /// When `current` was (effectively) entered: its track's zero.
    entered_at: Duration,
    in_flight: Option<Flight>,
    now: Duration,
}

impl<T> std::fmt::Debug for StateMachine<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Deliberately shallow: the states' keyframes and the guards are not
        // printable — a `Box<dyn Fn>` has no `Debug` — and a machine's useful
        // debugging identity is *where it is*, not what it is carrying.
        f.debug_struct("StateMachine")
            .field("states", &self.states.iter().map(|state| state.name).collect::<Vec<_>>())
            .field("current", &self.current)
            .field("destination", &self.destination())
            .field("now", &self.now)
            .finish()
    }
}

impl<T: Lerp> StateMachine<T> {
    /// A machine with one state: the start state.
    ///
    /// More states and edges are declarations, added in place, because a
    /// machine that could only be built whole (a builder that finishes) could
    /// not be edited by a tool that loads one edge at a time.
    #[must_use]
    pub fn new(start: &'static str, frames: Keyframes<T>) -> Self {
        Self {
            states: vec![MachineState {
                name: start,
                frames,
                looped: true,
            }],
            transitions: Vec::new(),
            current: start,
            entered_at: Duration::ZERO,
            in_flight: None,
            now: Duration::ZERO,
        }
    }

    /// Declare (or replace) a state, looped — a mode: walk, breathe, spin.
    pub fn state(&mut self, name: &'static str, frames: Keyframes<T>) -> &mut Self {
        self.set_state(name, frames, true)
    }

    /// Declare (or replace) a state that plays once and holds its last value.
    ///
    /// The hit-flash, the door-open: a state whose track ending is a
    /// *completion*, not a reason to loop.
    pub fn state_once(&mut self, name: &'static str, frames: Keyframes<T>) -> &mut Self {
        self.set_state(name, frames, false)
    }

    fn set_state(&mut self, name: &'static str, frames: Keyframes<T>, looped: bool) -> &mut Self {
        if let Some(existing) = self.states.iter_mut().find(|state| state.name == name) {
            existing.frames = frames;
            existing.looped = looped;
        } else {
            self.states.push(MachineState {
                name,
                frames,
                looped,
            });
        }
        self
    }

    /// Declare (or replace) the edge `from` → `to`.
    ///
    /// A later declaration of the same edge replaces the earlier one — the
    /// same replacement rule as a keyframe at a taken time, and for the same
    /// reason: two edges between the same pair of states is a choice whose
    /// winner should be the last thing declared, not whichever a `Vec` happens
    /// to yield first.
    pub fn transition(
        &mut self,
        from: &'static str,
        to: &'static str,
        duration: Duration,
        guard: impl Fn(&str) -> bool + 'static,
    ) -> &mut Self {
        let guard: Guard = Box::new(guard);
        if let Some(existing) = self
            .transitions
            .iter_mut()
            .find(|edge| edge.from == from && edge.to == to)
        {
            existing.duration = duration;
            existing.guard = guard;
        } else {
            self.transitions.push(MachineTransition {
                from,
                to,
                duration,
                guard,
            });
        }
        self
    }

    /// Offer an event. Returns the state it moved to, or `None` if no
    /// transition from the current state accepted it.
    ///
    /// `None` is a *refusal*, not an error — an event that nothing handles is
    /// the normal case for any machine wired to a busy input stream, and the
    /// caller that wants "did my click do anything" gets a real answer.
    ///
    /// A transition that fires while another is in flight **replaces it**: the
    /// new edge fades from the current state at its own clock, because the
    /// alternative — queueing the transition behind the first — is a machine
    /// that answers a click half a second after the click.
    ///
    /// Only edges *from the current state* are considered. An edge from a
    /// state a fade is heading toward is considered once that fade lands;
    /// a machine that could interrupt its own destination is a machine whose
    /// next state depends on how far through a fade it was when the second
    /// event arrived, which is a behaviour no author can see on paper.
    pub fn event(&mut self, event: &str) -> Option<&'static str> {
        let current = self.current;
        let accepted = self
            .transitions
            .iter()
            .filter(|edge| edge.from == current)
            .find(|edge| (edge.guard)(event))?;

        let target = accepted.to;
        if accepted.duration.is_zero() {
            // A cut, not a fade: the state changes now, in this call, and the
            // clock starts at the moment of the cut — a state whose clock ran
            // while it was unentered is a state that greets you mid-animation.
            self.current = target;
            self.entered_at = self.now;
            self.in_flight = None;
        } else {
            self.in_flight = Some(Flight {
                target,
                started: self.now,
            });
        }
        Some(target)
    }

    /// Move the machine's clock forward by `elapsed`.
    ///
    /// A zero-duration call is allowed and changes nothing, so a frame loop
    /// can advance by its real delta — including a dropped frame's zero —
    /// without a special case.
    pub fn advance(&mut self, elapsed: Duration) -> &mut Self {
        self.now += elapsed;
        // A finished cross-fade lands: the target becomes current. Its clock
        // has been running since the fade began, so `entered_at` is the fade's
        // start and the landing is seamless by construction.
        if let Some(flight) = self.in_flight.take() {
            let duration = self
                .transitions
                .iter()
                .find(|edge| edge.from == self.current && edge.to == flight.target)
                .map_or(Duration::ZERO, |edge| edge.duration);
            if self.now - flight.started >= duration {
                self.current = flight.target;
                self.entered_at = flight.started;
            } else {
                self.in_flight = Some(flight);
            }
        }
        self
    }

    /// The value now: the current state's track, cross-faded toward the
    /// destination's if a transition is in flight.
    #[must_use]
    pub fn sample(&self) -> T {
        let from = self.sample_state(self.current, self.now - self.entered_at);
        let Some(flight) = &self.in_flight else {
            return from;
        };

        let duration = self
            .transitions
            .iter()
            .find(|edge| edge.from == self.current && edge.to == flight.target)
            .map_or(Duration::ZERO, |edge| edge.duration);
        let to = self.sample_state(flight.target, self.now - flight.started);
        if duration.is_zero() {
            // An edge declared with no duration but reached while fading is a
            // contradiction this type resolves in favour of the destination.
            return to;
        }
        let alpha = ((self.now - flight.started).as_secs_f32() / duration.as_secs_f32())
            .clamp(0.0, 1.0);
        from.lerp(to, Curve::EASE_IN_OUT.transform(alpha))
    }

    /// A state's track at its own `elapsed`, with looping applied.
    fn sample_state(&self, name: &str, elapsed: Duration) -> T {
        let state = self
            .states
            .iter()
            .find(|state| state.name == name)
            .expect("a machine's states are declared before they are entered");
        if !state.looped {
            return state.frames.at(elapsed);
        }
        let end = state.frames.end();
        if end.is_zero() {
            return state.frames.at(elapsed);
        }
        // Loop by modulo: mode time is unbounded, and a wrap must not depend
        // on how many times the track has been through.
        let phase = elapsed.as_secs_f32() % end.as_secs_f32();
        state.frames.at(Duration::from_secs_f32(phase))
    }
}

impl<T> StateMachine<T> {
    /// The state the machine is in — the one a fade is *leaving*, if one is.
    #[must_use]
    pub const fn current(&self) -> &'static str {
        self.current
    }

    /// The state a fade is heading for, if one is in flight.
    #[must_use]
    pub const fn destination(&self) -> Option<&'static str> {
        match self.in_flight {
            Some(Flight { target, .. }) => Some(target),
            None => None,
        }
    }

    /// The names of every declared state.
    #[must_use]
    pub fn states(&self) -> Vec<&'static str> {
        self.states.iter().map(|state| state.name).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyframe::Keyframe;

    fn ms(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    /// A track rising 0 → 1 over 100 ms, linearly: the tests below assert
    /// "half way", and half of a linear track is a number that exists — half
    /// of an eased one is a function of the curve's shape.
    fn rising() -> Keyframes<f32> {
        Keyframes::new(0.0).with(Keyframe::to(0.1, 1.0).curve(Curve::Linear))
    }

    #[test]
    fn one_state_samples_its_track() {
        let mut machine: StateMachine<f32> = StateMachine::new("on", rising());
        machine.advance(ms(50));
        assert!((machine.sample() - 0.5).abs() < 0.001);
        assert_eq!(machine.current(), "on");
        assert_eq!(machine.destination(), None);
    }

    #[test]
    fn looped_state_wraps() {
        // Track: 0 → 1 over 100 ms, looping.
        let mut machine: StateMachine<f32> = StateMachine::new("walk", rising());
        machine.advance(ms(250));
        let sample = machine.sample();
        assert!((sample - 0.5).abs() < 0.001, "2.5 loops in: half way again, got {sample}");
        assert_eq!(machine.states().len(), 1);
    }

    #[test]
    fn once_state_holds_its_end() {
        let mut machine: StateMachine<f32> = StateMachine::new("idle", Keyframes::new(0.0));
        machine.state_once("hit", rising());
        machine.transition("idle", "hit", Duration::ZERO, |_| true);

        machine.event("go");
        machine.advance(ms(10_000));
        assert_eq!(machine.current(), "hit");
        assert_eq!(machine.sample(), 1.0, "one-shot: the end holds");
    }

    #[test]
    fn zero_duration_transition_is_a_cut() {
        let mut machine: StateMachine<f32> = StateMachine::new("idle", Keyframes::new(0.0));
        machine.state("on", rising());
        machine.transition("idle", "on", Duration::ZERO, |event| event == "tap");

        machine.advance(ms(37));
        assert_eq!(machine.event("tap"), Some("on"));
        assert_eq!(machine.current(), "on", "no fade: the state moved now");
        assert_eq!(machine.entered_at, ms(37), "the clock starts at the cut");
        machine.advance(ms(50));
        assert!((machine.sample() - 0.5).abs() < 0.001, "50 ms into the new state's track");
    }

    #[test]
    fn unaccepted_event_is_a_refusal() {
        let mut machine: StateMachine<f32> = StateMachine::new("idle", Keyframes::new(0.0));
        machine.transition("idle", "gone", ms(100), |event| event == "tap");

        assert_eq!(machine.event("poke"), None);
        assert_eq!(machine.current(), "idle");
    }

    #[test]
    fn cross_fade_eases_between_states() {
        let mut machine: StateMachine<f32> = StateMachine::new("low", Keyframes::new(0.0));
        machine.state("high", Keyframes::new(1.0));
        machine.transition("low", "high", ms(200), |event| event == "go");

        machine.event("go");
        machine.advance(ms(100));
        // EASE_IN_OUT at the half-way point is 0.5 exactly, by symmetry.
        assert_eq!(machine.sample(), 0.5);
        assert_eq!(machine.current(), "low", "still fading: not landed yet");
        assert_eq!(machine.destination(), Some("high"));

        machine.advance(ms(100));
        assert_eq!(machine.sample(), 1.0);
        assert_eq!(machine.current(), "high", "landed");
        assert_eq!(machine.destination(), None);
    }

    #[test]
    fn the_target_plays_through_the_fade() {
        // from = constant 0; to = rising track (0 → 1 over 100 ms); fade 100 ms.
        let mut machine: StateMachine<f32> = StateMachine::new("off", Keyframes::new(0.0));
        machine.state("on", rising());
        machine.transition("off", "on", ms(100), |event| event == "go");

        machine.event("go");
        machine.advance(ms(50));
        // from = 0. to = the target's own track at 50 ms = 0.5. alpha eased
        // at 0.5 = 0.5. So the sample is 0.25 — the target is *in motion*
        // during the fade, not frozen at its first frame.
        let value = machine.sample();
        assert!((0.2..0.3).contains(&value), "target mid-track, got {value}");
    }

    #[test]
    fn a_fade_leaving_a_looped_state_keeps_walking() {
        // Walk loops 0→1 over 100 ms; the fade out is long (1000 ms) toward a
        // constant the walk never reaches, so the samples track the walk's
        // own loop rather than the fade's alpha.
        let mut machine: StateMachine<f32> = StateMachine::new("walk", rising());
        machine.state("stop", Keyframes::new(2.0));
        machine.transition("walk", "stop", ms(1000), |event| event == "stop");

        machine.advance(ms(1000)); // walk is deep in its loop, phase 0
        machine.event("stop");
        machine.advance(ms(25)); // phase 0.25
        let first = machine.sample();
        machine.advance(ms(50)); // phase 0.75
        let second = machine.sample();
        // If the walk had frozen at the moment of the event, both samples
        // would be the fade's alpha apart (a sliver over 1000 ms). They are
        // half the walk's range apart, because the from-state kept playing.
        assert!((0.2..0.3).contains(&first), "walk at phase 0.25, got {first}");
        assert!((0.7..0.8).contains(&second), "walk at phase 0.75, got {second}");
    }

    #[test]
    fn a_second_event_replaces_the_fade_not_queues_it() {
        let mut machine: StateMachine<f32> = StateMachine::new("a", Keyframes::new(0.0));
        machine.state("b", Keyframes::new(1.0));
        machine.state("c", Keyframes::new(0.5));
        machine.transition("a", "b", ms(100), |event| event == "go-b");
        machine.transition("a", "c", ms(100), |event| event == "go-c");

        machine.event("go-b");
        machine.advance(ms(50));
        machine.event("go-c");
        assert_eq!(machine.destination(), Some("c"), "retargeted, not queued");
        assert_eq!(machine.current(), "a", "still fading from a");

        machine.advance(ms(50));
        // A fresh 100 ms fade from `a`, half done: 0 → 0.5 at eased 0.5.
        assert_eq!(machine.sample(), 0.25);
    }

    #[test]
    fn late_frame_overshoot_lands_cleanly() {
        let mut machine: StateMachine<f32> = StateMachine::new("a", Keyframes::new(0.0));
        machine.state("b", rising());
        machine.transition("a", "b", ms(100), |event| event == "go");

        machine.event("go");
        // One 350 ms frame: the 100 ms fade is over, 250 ms into the state.
        machine.advance(ms(350));
        assert_eq!(machine.current(), "b");
        // The target clock began at the fade's start: 350 ms in, wrapped once
        // past its 100 ms loop point — the state never "restarts" on landing.
        let sample = machine.sample();
        assert!((sample - 0.5).abs() < 0.001, "350 ms into a 100 ms loop: half way, got {sample}");
    }

    #[test]
    fn replacing_a_state_or_edge_updates_in_place() {
        let mut machine: StateMachine<f32> = StateMachine::new("a", Keyframes::new(0.0));
        machine.state("b", Keyframes::new(1.0));
        machine.state("b", Keyframes::new(2.0)); // replaced

        machine.transition("a", "b", ms(10), |_| true);
        machine.transition("a", "b", ms(20), |_| true); // replaced

        assert_eq!(machine.states().len(), 2);
        machine.event("go");
        machine.advance(ms(20));
        assert_eq!(machine.sample(), 2.0, "the second declaration won");
    }

    #[test]
    fn advance_by_zero_changes_nothing() {
        let mut machine: StateMachine<f32> = StateMachine::new("a", rising());
        machine.advance(ms(50));
        let before = machine.sample();
        machine.advance(Duration::ZERO);
        assert_eq!(machine.sample(), before);
    }
}
