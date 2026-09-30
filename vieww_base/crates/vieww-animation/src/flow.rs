//! Flow scripting — Motion Canvas' generators and Manim's `play`/`wait`.
//!
//! # The capability
//!
//! Motion Canvas describes an animation as a *script*: `yield* all(a, b)`,
//! `yield* chain(c, d)`, `yield* waitFor(1)`, `yield* waitUntil('drop')`,
//! `spawn(background)`, `yield* loop(3, () => …)`. Manim's `construct()` is
//! the same shape — `self.play(A, B, run_time=2)`, `self.wait()`. The
//! control flow reads top to bottom like the narration it accompanies, and
//! `waitUntil` binds a beat to a named **time event** set against the
//! voiceover, so re-timing the audio re-times the film without touching the
//! code.
//!
//! Rust has no stable generators, and does not need them for this: a script
//! whose every step has a known duration *is* a schedule. [`Flow`] is the
//! script as a value; [`Flow::compile`] walks it with a cursor and lays every
//! tween and call onto a [`Sequence`], which then gives the playback half —
//! seek, scrub, reverse, repeat — for free. The generator's "resume after
//! the animation ends" becomes "the cursor is at the animation's end", which
//! is the same moment, computed instead of awaited.
//!
//! ```
//! use vieww_animation::flow::{Flow, TimeEvents};
//! use vieww_animation::Curve;
//!
//! let script = Flow::chain([
//!     Flow::all([
//!         Flow::tween("x", 100.0, 1.0, Curve::EASE_IN_OUT),
//!         Flow::tween("opacity", 1.0, 0.5, Curve::Linear),
//!     ]),
//!     Flow::wait_until("drop"),
//!     Flow::call("boom"),
//!     Flow::tween("y", 200.0, 0.4, Curve::BOUNCE_OUT),
//! ]);
//! let mut events = TimeEvents::new();
//! events.set("drop", 2.0); // from the voiceover's cue sheet
//! let seq = script.compile(&events);
//! assert_eq!(seq.duration(), 2.4);
//! ```

use std::collections::BTreeMap;

use crate::curve::Curve;
use crate::sequence::{Position, Sequence, Value};

/// Named moments on the master clock — Motion Canvas' time events, set from
/// an editor or a cue sheet rather than written into the script.
#[derive(Debug, Clone, Default)]
pub struct TimeEvents {
    times: BTreeMap<String, f32>,
}

impl TimeEvents {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Place `name` at `time` seconds.
    pub fn set(&mut self, name: &str, time: f32) -> &mut Self {
        self.times.insert(name.to_owned(), time.max(0.0));
        self
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<f32> {
        self.times.get(name).copied()
    }
}

/// One step of a script. Build with the constructor functions.
#[derive(Debug, Clone)]
pub enum Flow {
    /// Animate a channel to a value (from wherever it is).
    Tween {
        channel: String,
        from: Option<Value>,
        to: Value,
        duration: f32,
        curve: Curve,
    },
    /// Jump a channel.
    Set { channel: String, value: Value },
    /// `waitFor(seconds)` / Manim's `wait()`.
    Wait(f32),
    /// `waitUntil(event)`: resume at the event's time, or immediately if it
    /// is unset or already past.
    WaitUntil(String),
    /// Run together; finish when all have.
    All(Vec<Flow>),
    /// Run together; finish when the **first** has (the rest keep playing
    /// in the background, as in Motion Canvas).
    Any(Vec<Flow>),
    /// Run one after another.
    Chain(Vec<Flow>),
    /// Start each `interval` after the previous *starts*; finish when all
    /// have — Motion Canvas' `sequence(delay, …)`.
    Stagger(f32, Vec<Flow>),
    /// Start after a pause.
    Delay(f32, Box<Flow>),
    /// Repeat a body.
    Loop(usize, Box<Flow>),
    /// Start in the background and move on at once.
    Spawn(Box<Flow>),
    /// Stretch or squeeze a body to take exactly this long — Manim's
    /// `run_time`.
    RunTime(f32, Box<Flow>),
    /// A named callback at the cursor.
    Call(String),
    /// Name the cursor's moment, for seeking.
    Label(String),
}

impl Flow {
    #[must_use]
    pub fn tween(channel: &str, to: impl Into<Value>, duration: f32, curve: Curve) -> Self {
        Self::Tween {
            channel: channel.to_owned(),
            from: None,
            to: to.into(),
            duration,
            curve,
        }
    }

    #[must_use]
    pub fn from_to(
        channel: &str,
        from: impl Into<Value>,
        to: impl Into<Value>,
        duration: f32,
        curve: Curve,
    ) -> Self {
        Self::Tween {
            channel: channel.to_owned(),
            from: Some(from.into()),
            to: to.into(),
            duration,
            curve,
        }
    }

    #[must_use]
    pub fn set(channel: &str, value: impl Into<Value>) -> Self {
        Self::Set {
            channel: channel.to_owned(),
            value: value.into(),
        }
    }

    #[must_use]
    pub const fn wait(seconds: f32) -> Self {
        Self::Wait(seconds)
    }

    #[must_use]
    pub fn wait_until(event: &str) -> Self {
        Self::WaitUntil(event.to_owned())
    }

    #[must_use]
    pub fn all(items: impl IntoIterator<Item = Self>) -> Self {
        Self::All(items.into_iter().collect())
    }

    #[must_use]
    pub fn any(items: impl IntoIterator<Item = Self>) -> Self {
        Self::Any(items.into_iter().collect())
    }

    #[must_use]
    pub fn chain(items: impl IntoIterator<Item = Self>) -> Self {
        Self::Chain(items.into_iter().collect())
    }

    #[must_use]
    pub fn stagger(interval: f32, items: impl IntoIterator<Item = Self>) -> Self {
        Self::Stagger(interval, items.into_iter().collect())
    }

    #[must_use]
    pub fn delay(seconds: f32, body: Self) -> Self {
        Self::Delay(seconds, Box::new(body))
    }

    #[must_use]
    pub fn repeat(times: usize, body: Self) -> Self {
        Self::Loop(times, Box::new(body))
    }

    #[must_use]
    pub fn spawn(body: Self) -> Self {
        Self::Spawn(Box::new(body))
    }

    /// Manim's `self.play(*animations, run_time=…)`: all together, fitted
    /// into `run_time`.
    #[must_use]
    pub fn play(run_time: f32, items: impl IntoIterator<Item = Self>) -> Self {
        Self::RunTime(run_time, Box::new(Self::all(items)))
    }

    #[must_use]
    pub fn call(name: &str) -> Self {
        Self::Call(name.to_owned())
    }

    #[must_use]
    pub fn label(name: &str) -> Self {
        Self::Label(name.to_owned())
    }

    /// How long this step holds the cursor, at unit scale, with events.
    #[must_use]
    pub fn duration(&self, events: &TimeEvents) -> f32 {
        let mut probe = Sequence::new();
        self.lay(&mut probe, 0.0, 1.0, events)
    }

    /// Lay the script onto a fresh [`Sequence`].
    #[must_use]
    pub fn compile(&self, events: &TimeEvents) -> Sequence {
        let mut seq = Sequence::new();
        let end = self.lay(&mut seq, 0.0, 1.0, events);
        // A trailing wait extends the sequence even with nothing in it.
        seq.label("__end", Position::At(end));
        seq
    }

    /// Place this step at `at` with durations multiplied by `scale`; return
    /// where the cursor is afterwards.
    fn lay(&self, seq: &mut Sequence, at: f32, scale: f32, events: &TimeEvents) -> f32 {
        match self {
            Self::Tween {
                channel,
                from,
                to,
                duration,
                curve,
            } => {
                let d = duration * scale;
                match from {
                    Some(f) => seq.from_to(channel, *f, *to, d, *curve, Position::At(at)),
                    None => seq.to(channel, *to, d, *curve, Position::At(at)),
                };
                at + d
            }
            Self::Set { channel, value } => {
                seq.set(channel, *value, Position::At(at));
                at
            }
            Self::Wait(s) => at + s * scale,
            Self::WaitUntil(name) => events.get(name).map_or(at, |t| t.max(at)),
            Self::All(items) => items
                .iter()
                .map(|f| f.lay(seq, at, scale, events))
                .fold(at, f32::max),
            Self::Any(items) => {
                let ends: Vec<f32> = items.iter().map(|f| f.lay(seq, at, scale, events)).collect();
                ends.into_iter().reduce(f32::min).unwrap_or(at)
            }
            Self::Chain(items) => items.iter().fold(at, |cursor, f| f.lay(seq, cursor, scale, events)),
            Self::Stagger(interval, items) => {
                let mut end = at;
                for (i, f) in items.iter().enumerate() {
                    #[allow(clippy::cast_precision_loss)]
                    let start = at + interval * scale * i as f32;
                    end = end.max(f.lay(seq, start, scale, events));
                }
                end
            }
            Self::Delay(s, body) => body.lay(seq, at + s * scale, scale, events),
            Self::Loop(n, body) => (0..*n).fold(at, |cursor, _| body.lay(seq, cursor, scale, events)),
            Self::Spawn(body) => {
                body.lay(seq, at, scale, events);
                at
            }
            Self::RunTime(run, body) => {
                let natural = body.duration(events);
                let factor = if natural > 0.0 { run / natural } else { 1.0 };
                body.lay(seq, at, scale * factor, events);
                at + run * scale
            }
            Self::Call(name) => {
                seq.call(name, Position::At(at));
                at
            }
            Self::Label(name) => {
                seq.label(name, Position::At(at));
                at
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn chain_all_and_any_combine_durations() {
        let ev = TimeEvents::new();
        let a = Flow::tween("a", 1.0, 1.0, Curve::Linear);
        let b = Flow::tween("b", 1.0, 2.0, Curve::Linear);
        assert!(close(Flow::chain([a.clone(), b.clone()]).duration(&ev), 3.0));
        assert!(close(Flow::all([a.clone(), b.clone()]).duration(&ev), 2.0));
        assert!(close(Flow::any([a.clone(), b.clone()]).duration(&ev), 1.0));
        // After `any`, the longer one is still running in the background.
        let seq = Flow::chain([Flow::any([a, b]), Flow::tween("c", 1.0, 1.0, Curve::Linear)]).compile(&ev);
        let mut s = seq;
        s.seek(1.5);
        assert!(close(s.scalar("b"), 0.75));
        assert!(close(s.scalar("c"), 0.5));
    }

    #[test]
    fn wait_until_binds_to_time_events_and_never_goes_back() {
        let mut ev = TimeEvents::new();
        ev.set("cue", 3.0).set("early", 0.5);
        let f = Flow::chain([
            Flow::wait(1.0),
            Flow::wait_until("early"),
            Flow::wait_until("cue"),
            Flow::call("hit"),
            Flow::wait_until("unset"),
        ]);
        let seq = f.compile(&ev);
        assert!(close(seq.duration(), 3.0));
    }

    #[test]
    fn run_time_scales_everything_inside() {
        let ev = TimeEvents::new();
        let f = Flow::play(
            1.0,
            [
                Flow::from_to("x", 0.0, 1.0, 2.0, Curve::Linear),
                Flow::from_to("y", 0.0, 1.0, 4.0, Curve::Linear),
            ],
        );
        let mut seq = f.compile(&ev);
        assert!(close(seq.duration(), 1.0));
        seq.seek(0.5);
        assert!(close(seq.scalar("x"), 1.0), "x took half the run time");
        assert!(close(seq.scalar("y"), 0.5));
    }

    #[test]
    fn stagger_loop_spawn_delay() {
        let ev = TimeEvents::new();
        let item = |c: &str| Flow::from_to(c, 0.0, 1.0, 1.0, Curve::Linear);
        assert!(close(Flow::stagger(0.25, [item("a"), item("b"), item("c")]).duration(&ev), 1.5));
        assert!(close(Flow::repeat(3, item("a")).duration(&ev), 3.0));
        assert!(close(Flow::spawn(item("a")).duration(&ev), 0.0));
        assert!(close(Flow::delay(0.5, item("a")).duration(&ev), 1.5));
    }

    #[test]
    fn calls_and_labels_land_at_the_cursor() {
        let ev = TimeEvents::new();
        let seq = Flow::chain([
            Flow::tween("x", 1.0, 0.75, Curve::Linear),
            Flow::label("hit"),
            Flow::call("bang"),
            Flow::wait(1.0),
        ])
        .compile(&ev);
        assert_eq!(seq.label_time("hit"), Some(0.75));
        let mut s = seq;
        let names: Vec<String> = (0..20).flat_map(|_| s.advance(0.1)).map(|e| e.name).collect();
        assert!(names.contains(&"bang".to_owned()));
        assert!(close(s.duration(), 1.75));
    }
}
