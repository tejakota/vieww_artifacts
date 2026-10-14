//! A seekable timeline of tweens — GSAP's `gsap.timeline()`.
//!
//! # What was missing
//!
//! `vieww-element`'s `Timeline` sequences *springs*, and a spring cannot be
//! scrubbed: its settle time is not known in advance, so it can be played
//! but not seeked, reversed or dragged by a scroll position. [`keyframe`](crate::keyframe)'s
//! `Timeline` is seekable but is a set of independent tracks with delays.
//! Neither is what the comparison document means by "timeline sequencing" in
//! GSAP, After Effects or Motion Canvas:
//!
//! * a **position parameter** — "at 2 s", "0.5 s after the end", "with the
//!   previous one", "0.2 s after the label `intro`";
//! * **labels** to jump to and position against;
//! * **nesting** — a timeline added to another at a position;
//! * **stagger** across a set of targets, from the start, centre, end or
//!   edges;
//! * a **playhead** with `seek`, `progress`, `reverse`, `time_scale`,
//!   `repeat`, `yoyo` and `repeat_delay`;
//! * **callbacks** that fire exactly once when the playhead crosses them, in
//!   whichever direction it is moving.
//!
//! [`Sequence`] is that. It animates named **channels** of [`Value`]s — a
//! scalar, a point or a colour — because a GSAP timeline animates many
//! properties of many targets at once, and a single generic `T` could not.
//!
//! # Deterministic by construction
//!
//! The layout (where every tween starts and ends) is resolved at insertion
//! time, and sampling is a pure function of the playhead. Playing forward in
//! sixty small steps and seeking straight to the end produce the same
//! values; the only difference is which callbacks were crossed on the way,
//! and those are returned from [`Sequence::advance`], never run behind the
//! caller's back.
//!
//! ```
//! use vieww_animation::sequence::{Sequence, Value};
//! use vieww_animation::Curve;
//!
//! let mut tl = Sequence::new();
//! tl.to("x", 100.0, 1.0, Curve::Linear, ">")
//!     .label("half", ">")
//!     .to("y", 50.0, 0.5, Curve::Linear, "<0.25") // 0.25 s after the previous starts
//!     .call("done", "half+=0.5");
//!
//! assert_eq!(tl.duration(), 1.5);
//! tl.seek(0.5);
//! assert_eq!(tl.value("x"), Some(Value::Scalar(50.0)));
//! let events = tl.advance(1.2);
//! assert!(events.iter().any(|e| e.name == "done"));
//! ```

use std::collections::BTreeMap;

use vieww_foundation::{Color, Offset};

use crate::curve::Curve;
use crate::tween::Lerp;

/// What a channel holds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Scalar(f32),
    Point(Offset),
    Color(Color),
}

impl Value {
    /// The scalar, or `None` for another kind.
    #[must_use]
    pub const fn scalar(self) -> Option<f32> {
        match self {
            Self::Scalar(v) => Some(v),
            _ => None,
        }
    }

    /// The point, or `None`.
    #[must_use]
    pub const fn point(self) -> Option<Offset> {
        match self {
            Self::Point(v) => Some(v),
            _ => None,
        }
    }

    /// The colour, or `None`.
    #[must_use]
    pub const fn color(self) -> Option<Color> {
        match self {
            Self::Color(v) => Some(v),
            _ => None,
        }
    }

    /// The value a channel of this kind holds before anything touched it,
    /// when no [`Sequence::initial`] says otherwise: zero, the origin, or
    /// transparent.
    #[must_use]
    pub const fn zero(self) -> Self {
        match self {
            Self::Scalar(_) => Self::Scalar(0.0),
            Self::Point(_) => Self::Point(Offset::ZERO),
            Self::Color(_) => Self::Color(Color::TRANSPARENT),
        }
    }

    fn mix(self, other: Self, t: f32) -> Self {
        match (self, other) {
            (Self::Scalar(a), Self::Scalar(b)) => Self::Scalar(a.lerp(b, t)),
            (Self::Point(a), Self::Point(b)) => Self::Point(a.lerp(b, t)),
            (Self::Color(a), Self::Color(b)) => Self::Color(a.lerp(b, t)),
            // Mismatched kinds cannot interpolate; the tween jumps at its end.
            (a, b) => {
                if t < 1.0 {
                    a
                } else {
                    b
                }
            }
        }
    }
}

impl From<f32> for Value {
    fn from(v: f32) -> Self {
        Self::Scalar(v)
    }
}
impl From<Offset> for Value {
    fn from(v: Offset) -> Self {
        Self::Point(v)
    }
}
impl From<Color> for Value {
    fn from(v: Color) -> Self {
        Self::Color(v)
    }
}

/// Where an insertion lands — GSAP's position parameter, parsed.
#[derive(Debug, Clone, PartialEq)]
pub enum Position {
    /// At this absolute time (`"2.5"`).
    At(f32),
    /// Relative to the end of the timeline (`">"` is `End(0)`, `"+=1"`,
    /// `"-=0.5"`).
    End(f32),
    /// Relative to the **start** of the most recent insertion (`"<"`,
    /// `"<0.2"`).
    PreviousStart(f32),
    /// Relative to the **end** of the most recent insertion (`">0.3"`).
    PreviousEnd(f32),
    /// Relative to a label (`"intro"`, `"intro+=0.5"`, `"intro-=1"`). An
    /// unknown label is created at the end, as GSAP does.
    Label(String, f32),
}

impl Position {
    /// Parse GSAP's spelling. Unparseable text is treated as a label name.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let t = text.trim();
        let num = |s: &str| s.trim().parse::<f32>().ok();
        if t.is_empty() || t == ">" {
            return Self::End(0.0);
        }
        if let Some(rest) = t.strip_prefix("+=") {
            return Self::End(num(rest).unwrap_or(0.0));
        }
        if let Some(rest) = t.strip_prefix("-=") {
            return Self::End(-num(rest).unwrap_or(0.0));
        }
        if let Some(rest) = t.strip_prefix('<') {
            return Self::PreviousStart(if rest.is_empty() {
                0.0
            } else {
                num(rest).unwrap_or(0.0)
            });
        }
        if let Some(rest) = t.strip_prefix('>') {
            return Self::PreviousEnd(num(rest).unwrap_or(0.0));
        }
        if let Some(v) = num(t) {
            return Self::At(v);
        }
        for (op, sign) in [("+=", 1.0), ("-=", -1.0)] {
            if let Some((label, off)) = t.split_once(op) {
                return Self::Label(label.to_owned(), sign * num(off).unwrap_or(0.0));
            }
        }
        Self::Label(t.to_owned(), 0.0)
    }
}

impl From<&str> for Position {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}
impl From<f32> for Position {
    fn from(t: f32) -> Self {
        Self::At(t)
    }
}

/// Where the stagger cascade begins — GSAP's `stagger.from`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StaggerFrom {
    #[default]
    Start,
    End,
    Center,
    /// Both ends first, the middle last.
    Edges,
}

impl StaggerFrom {
    /// How many `each` steps item `i` of `n` waits.
    #[allow(clippy::cast_precision_loss)]
    fn rank(self, i: usize, n: usize) -> f32 {
        let last = n.saturating_sub(1) as f32;
        let i = i as f32;
        match self {
            Self::Start => i,
            Self::End => last - i,
            Self::Center => (i - last / 2.0).abs(),
            Self::Edges => last / 2.0 - (i - last / 2.0).abs(),
        }
    }
}

#[derive(Debug, Clone)]
struct Tween {
    channel: String,
    start: f32,
    duration: f32,
    from: Option<Value>,
    to: Value,
    curve: Curve,
}

/// A callback crossed by the playhead.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// The name given to [`Sequence::call`], or one of the lifecycle names
    /// `"start"`, `"repeat"`, `"complete"`, `"reverse-complete"`.
    pub name: String,
    /// When, on the sequence's own (un-repeated) clock.
    pub time: f32,
}

/// A GSAP-style timeline. See the [module docs](self).
#[derive(Debug, Clone, Default)]
pub struct Sequence {
    tweens: Vec<Tween>,
    calls: Vec<(f32, String)>,
    labels: BTreeMap<String, f32>,
    initial: BTreeMap<String, Value>,
    duration: f32,
    last: (f32, f32),
    // playback
    playhead: f32,
    time_scale: f32,
    reversed: bool,
    paused: bool,
    repeat: i32,
    yoyo: bool,
    repeat_delay: f32,
    started: bool,
}

impl Sequence {
    #[must_use]
    pub fn new() -> Self {
        Self {
            time_scale: 1.0,
            ..Self::default()
        }
    }

    fn resolve(&mut self, position: &Position) -> f32 {
        let t = match position {
            Position::At(t) => *t,
            Position::End(off) => self.duration + off,
            Position::PreviousStart(off) => self.last.0 + off,
            Position::PreviousEnd(off) => self.last.1 + off,
            Position::Label(name, off) => {
                let at = match self.labels.get(name) {
                    Some(&t) => t,
                    None => {
                        let t = self.duration;
                        self.labels.insert(name.clone(), t);
                        t
                    }
                };
                at + off
            }
        };
        t.max(0.0)
    }

    fn insert(&mut self, tween: Tween) {
        self.last = (tween.start, tween.start + tween.duration);
        self.duration = self.duration.max(tween.start + tween.duration);
        self.tweens.push(tween);
    }

    /// The value a channel holds before any tween touches it — GSAP's
    /// "current value" of the target, recorded here because a sequence owns
    /// no target to read it from.
    pub fn initial(&mut self, channel: &str, value: impl Into<Value>) -> &mut Self {
        self.initial.insert(channel.to_owned(), value.into());
        self
    }

    /// Tween `channel` from wherever it is at that moment to `to`.
    pub fn to(
        &mut self,
        channel: &str,
        to: impl Into<Value>,
        duration: f32,
        curve: Curve,
        position: impl Into<Position>,
    ) -> &mut Self {
        let start = self.resolve(&position.into());
        self.insert(Tween {
            channel: channel.to_owned(),
            start,
            duration: duration.max(0.0),
            from: None,
            to: to.into(),
            curve,
        });
        self
    }

    /// Tween `channel` from `from` to `to` — GSAP's `fromTo`.
    pub fn from_to(
        &mut self,
        channel: &str,
        from: impl Into<Value>,
        to: impl Into<Value>,
        duration: f32,
        curve: Curve,
        position: impl Into<Position>,
    ) -> &mut Self {
        let start = self.resolve(&position.into());
        self.insert(Tween {
            channel: channel.to_owned(),
            start,
            duration: duration.max(0.0),
            from: Some(from.into()),
            to: to.into(),
            curve,
        });
        self
    }

    /// Jump `channel` to `value` at a point — a zero-length tween.
    pub fn set(
        &mut self,
        channel: &str,
        value: impl Into<Value>,
        position: impl Into<Position>,
    ) -> &mut Self {
        self.to(channel, value, 0.0, Curve::Linear, position)
    }

    /// Name the moment at `position`.
    pub fn label(&mut self, name: &str, position: impl Into<Position>) -> &mut Self {
        let t = self.resolve(&position.into());
        self.labels.insert(name.to_owned(), t);
        self.duration = self.duration.max(t);
        self
    }

    /// A callback at `position`, returned from [`advance`](Self::advance)
    /// when the playhead crosses it.
    pub fn call(&mut self, name: &str, position: impl Into<Position>) -> &mut Self {
        let t = self.resolve(&position.into());
        self.calls.push((t, name.to_owned()));
        self.duration = self.duration.max(t);
        self.last = (t, t);
        self
    }

    /// The same tween on each of `channels`, `each` seconds apart in the
    /// order `from` dictates.
    #[allow(clippy::too_many_arguments)]
    pub fn stagger(
        &mut self,
        channels: &[&str],
        from: impl Into<Value> + Copy,
        to: impl Into<Value> + Copy,
        duration: f32,
        each: f32,
        order: StaggerFrom,
        curve: Curve,
        position: impl Into<Position>,
    ) -> &mut Self {
        let base = self.resolve(&position.into());
        let n = channels.len();
        let mut span = (base, base);
        for (i, ch) in channels.iter().enumerate() {
            let start = base + order.rank(i, n) * each;
            self.insert(Tween {
                channel: (*ch).to_owned(),
                start,
                duration,
                from: Some(from.into()),
                to: to.into(),
                curve,
            });
            span.1 = span.1.max(start + duration);
        }
        self.last = span;
        self
    }

    /// Nest `child` at `position`: its tweens, labels (prefixed
    /// `child_name/`) and callbacks, shifted and scaled by its own
    /// `time_scale`.
    pub fn add(
        &mut self,
        child_name: &str,
        child: &Self,
        position: impl Into<Position>,
    ) -> &mut Self {
        let base = self.resolve(&position.into());
        let scale = if child.time_scale > 0.0 {
            1.0 / child.time_scale
        } else {
            1.0
        };
        for t in &child.tweens {
            let mut t = t.clone();
            t.start = base + t.start * scale;
            t.duration *= scale;
            self.tweens.push(t);
        }
        for (t, name) in &child.calls {
            self.calls.push((base + t * scale, name.clone()));
        }
        for (name, t) in &child.labels {
            self.labels
                .insert(format!("{child_name}/{name}"), base + t * scale);
        }
        for (ch, v) in &child.initial {
            self.initial.entry(ch.clone()).or_insert(*v);
        }
        let end = base + child.duration * scale;
        self.last = (base, end);
        self.duration = self.duration.max(end);
        self
    }

    /// Length of one pass, in seconds.
    #[must_use]
    pub const fn duration(&self) -> f32 {
        self.duration
    }

    /// Length of every pass including repeats and their delays; infinite
    /// for `repeat(-1)`.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn total_duration(&self) -> f32 {
        if self.repeat < 0 {
            f32::INFINITY
        } else {
            let r = self.repeat as f32;
            self.duration * (r + 1.0) + self.repeat_delay * r
        }
    }

    /// Where the label `name` sits.
    #[must_use]
    pub fn label_time(&self, name: &str) -> Option<f32> {
        self.labels.get(name).copied()
    }

    /// Every label, in time order.
    #[must_use]
    pub fn labels(&self) -> Vec<(&str, f32)> {
        let mut v: Vec<(&str, f32)> = self.labels.iter().map(|(k, v)| (k.as_str(), *v)).collect();
        v.sort_by(|a, b| a.1.total_cmp(&b.1));
        v
    }

    /// Every channel any tween touches.
    #[must_use]
    pub fn channels(&self) -> Vec<&str> {
        let mut v: Vec<&str> = self.tweens.iter().map(|t| t.channel.as_str()).collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    // ------------------------------------------------------------ playback

    /// Repeat `count` extra times; `-1` forever.
    pub fn repeat(&mut self, count: i32) -> &mut Self {
        self.repeat = count;
        self
    }

    /// Alternate direction on each repeat.
    pub fn yoyo(&mut self, yoyo: bool) -> &mut Self {
        self.yoyo = yoyo;
        self
    }

    /// Pause between repeats, in seconds.
    pub fn repeat_delay(&mut self, delay: f32) -> &mut Self {
        self.repeat_delay = delay.max(0.0);
        self
    }

    /// Playback rate: 2.0 is double speed.
    pub fn time_scale(&mut self, scale: f32) -> &mut Self {
        self.time_scale = scale.max(0.0);
        self
    }

    /// Play backwards from wherever the playhead is.
    pub fn reverse(&mut self) -> &mut Self {
        self.reversed = !self.reversed;
        self
    }

    #[must_use]
    pub const fn is_reversed(&self) -> bool {
        self.reversed
    }

    pub fn pause(&mut self) -> &mut Self {
        self.paused = true;
        self
    }

    pub fn play(&mut self) -> &mut Self {
        self.paused = false;
        self.reversed = false;
        self
    }

    /// Put the playhead at `time` (seconds on the repeated clock) without
    /// firing anything.
    pub fn seek(&mut self, time: f32) -> &mut Self {
        self.playhead = time.clamp(0.0, self.total_duration());
        self
    }

    /// Seek to a label.
    pub fn seek_label(&mut self, name: &str) -> &mut Self {
        if let Some(t) = self.label_time(name) {
            self.seek(t);
        }
        self
    }

    /// Seek to a fraction of one pass (GSAP's `progress()`).
    pub fn set_progress(&mut self, p: f32) -> &mut Self {
        self.seek(p.clamp(0.0, 1.0) * self.duration)
    }

    /// The playhead, on the repeated clock.
    #[must_use]
    pub const fn time(&self) -> f32 {
        self.playhead
    }

    /// The playhead folded into one pass — the time sampling reads.
    #[must_use]
    pub fn local_time(&self) -> f32 {
        self.fold(self.playhead).0
    }

    /// Where one pass is, 0..=1.
    #[must_use]
    pub fn progress(&self) -> f32 {
        if self.duration <= 0.0 {
            1.0
        } else {
            self.local_time() / self.duration
        }
    }

    /// Whether the playhead has reached the end (or the start, reversed).
    #[must_use]
    pub fn is_complete(&self) -> bool {
        if self.reversed {
            self.playhead <= 0.0
        } else {
            self.playhead >= self.total_duration()
        }
    }

    /// (local time, cycle) for a repeated-clock time.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    fn fold(&self, global: f32) -> (f32, i32) {
        if self.duration <= 0.0 {
            return (0.0, 0);
        }
        let cycle_len = self.duration + self.repeat_delay;
        let max_cycle = if self.repeat < 0 {
            i32::MAX
        } else {
            self.repeat
        };
        let mut cycle = (global / cycle_len).floor() as i32;
        cycle = cycle.clamp(0, max_cycle);
        let mut local = (global - cycle as f32 * cycle_len).clamp(0.0, self.duration);
        if global >= self.total_duration() {
            local = self.duration;
        }
        if self.yoyo && cycle % 2 == 1 {
            local = self.duration - local;
        }
        (local, cycle)
    }

    /// Advance by `dt` real seconds (scaled by `time_scale`, negated when
    /// reversed) and return every callback crossed, in crossing order.
    pub fn advance(&mut self, dt: f32) -> Vec<Event> {
        if self.paused || dt <= 0.0 {
            return Vec::new();
        }
        let before = self.playhead;
        let step = dt * self.time_scale * if self.reversed { -1.0 } else { 1.0 };
        let after = (before + step).clamp(0.0, self.total_duration());
        self.playhead = after;
        self.crossed(before, after)
    }

    #[allow(clippy::cast_precision_loss)]
    fn crossed(&mut self, before: f32, after: f32) -> Vec<Event> {
        let mut events = Vec::new();
        if before == after {
            return events;
        }
        let forward = after > before;
        if forward && !self.started && before <= 0.0 {
            self.started = true;
            events.push(Event {
                name: "start".into(),
                time: 0.0,
            });
        }
        let cycle_len = (self.duration + self.repeat_delay).max(f32::EPSILON);
        let (_, c0) = self.fold(before);
        let (_, c1) = self.fold(after);
        let (lo_c, hi_c) = if forward { (c0, c1) } else { (c1, c0) };
        let mut per_cycle = Vec::new();
        for cycle in lo_c..=hi_c {
            let cycle_start = cycle as f32 * cycle_len;
            let a = (before.min(after) - cycle_start).clamp(0.0, self.duration);
            let b = (before.max(after) - cycle_start).clamp(0.0, self.duration);
            // Local-time window covered in this cycle, and whether local
            // time runs the same way as the playhead in it.
            let flipped = self.yoyo && cycle % 2 == 1;
            let (la, lb) = if flipped {
                (self.duration - b, self.duration - a)
            } else {
                (a, b)
            };
            let local_forward = forward != flipped;
            let mut hits: Vec<&(f32, String)> = self
                .calls
                .iter()
                .filter(|(t, _)| {
                    if local_forward {
                        // (la, lb], plus la itself on the very first frame.
                        (*t > la || (la == 0.0 && before <= 0.0 && *t == 0.0)) && *t <= lb
                    } else {
                        *t >= la && *t < lb
                    }
                })
                .collect();
            hits.sort_by(|x, y| x.0.total_cmp(&y.0));
            if !local_forward {
                hits.reverse();
            }
            let mut list: Vec<Event> = hits
                .into_iter()
                .map(|(t, n)| Event {
                    name: n.clone(),
                    time: *t,
                })
                .collect();
            if forward && cycle > c0 && cycle > 0 {
                list.insert(
                    0,
                    Event {
                        name: "repeat".into(),
                        time: 0.0,
                    },
                );
            }
            per_cycle.push(list);
        }
        if !forward {
            per_cycle.reverse();
        }
        events.extend(per_cycle.into_iter().flatten());
        if forward && after >= self.total_duration() && before < self.total_duration() {
            events.push(Event {
                name: "complete".into(),
                time: self.duration,
            });
        }
        if !forward && after <= 0.0 && before > 0.0 {
            events.push(Event {
                name: "reverse-complete".into(),
                time: 0.0,
            });
        }
        events
    }

    /// The value of `channel` at local time `t` within one pass.
    #[must_use]
    pub fn value_at(&self, channel: &str, t: f32) -> Option<Value> {
        let mut tweens: Vec<&Tween> = self
            .tweens
            .iter()
            .filter(|w| w.channel == channel)
            .collect();
        if tweens.is_empty() {
            return self.initial.get(channel).copied();
        }
        tweens.sort_by(|a, b| a.start.total_cmp(&b.start));
        let base = self
            .initial
            .get(channel)
            .copied()
            .or(tweens[0].from)
            .unwrap_or_else(|| tweens[0].to.zero());
        // Each tween's implicit `from` is the value in force when it starts,
        // computed from the tweens before it only — GSAP records the start
        // value at the tween's first render, which is its start time.
        let mut froms: Vec<Value> = Vec::with_capacity(tweens.len());
        for i in 0..tweens.len() {
            let from = tweens[i]
                .from
                .unwrap_or_else(|| eval(&tweens[..i], &froms, base, tweens[i].start));
            froms.push(from);
        }
        Some(eval(&tweens, &froms, base, t))
    }

    /// The value of `channel` at the playhead.
    #[must_use]
    pub fn value(&self, channel: &str) -> Option<Value> {
        self.value_at(channel, self.local_time())
    }

    /// The scalar value of `channel` at the playhead, `0.0` if absent.
    #[must_use]
    pub fn scalar(&self, channel: &str) -> f32 {
        self.value(channel).and_then(Value::scalar).unwrap_or(0.0)
    }

    /// Every channel's value at the playhead.
    #[must_use]
    pub fn values(&self) -> BTreeMap<String, Value> {
        self.channels()
            .into_iter()
            .filter_map(|c| self.value(c).map(|v| (c.to_owned(), v)))
            .collect()
    }
}

/// The value `tweens` (sorted, with resolved `froms`) give at `t`: the last
/// tween to have started wins.
fn eval(tweens: &[&Tween], froms: &[Value], base: Value, t: f32) -> Value {
    let mut result = base;
    for (w, &from) in tweens.iter().zip(froms) {
        if w.start > t {
            break;
        }
        let end = w.start + w.duration;
        result = if t < end && w.duration > 0.0 {
            from.mix(w.to, w.curve.transform((t - w.start) / w.duration))
        } else {
            w.to
        };
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn positions_parse_like_gsap() {
        assert_eq!(Position::parse(">"), Position::End(0.0));
        assert_eq!(Position::parse("+=1.5"), Position::End(1.5));
        assert_eq!(Position::parse("-=0.5"), Position::End(-0.5));
        assert_eq!(Position::parse("<"), Position::PreviousStart(0.0));
        assert_eq!(Position::parse("<0.2"), Position::PreviousStart(0.2));
        assert_eq!(Position::parse(">0.3"), Position::PreviousEnd(0.3));
        assert_eq!(Position::parse("2"), Position::At(2.0));
        assert_eq!(
            Position::parse("intro+=1"),
            Position::Label("intro".into(), 1.0)
        );
        assert_eq!(
            Position::parse("intro"),
            Position::Label("intro".into(), 0.0)
        );
    }

    #[test]
    fn appended_tweens_chain_and_overlaps_resolve() {
        let mut tl = Sequence::new();
        tl.to("a", 1.0, 1.0, Curve::Linear, ">")
            .to("b", 1.0, 1.0, Curve::Linear, ">")
            .to("c", 1.0, 1.0, Curve::Linear, "-=0.5")
            .to("d", 1.0, 1.0, Curve::Linear, "<");
        assert!(close(tl.duration(), 2.5));
        tl.seek(2.0);
        assert!(close(tl.scalar("c"), 0.5));
        assert!(close(tl.scalar("d"), 0.5), "d starts with c");
    }

    #[test]
    fn to_starts_from_the_value_in_force() {
        let mut tl = Sequence::new();
        tl.initial("x", 10.0)
            .to("x", 20.0, 1.0, Curve::Linear, ">")
            .to("x", 0.0, 1.0, Curve::Linear, ">");
        tl.seek(0.0);
        assert!(close(tl.scalar("x"), 10.0));
        tl.seek(1.5);
        assert!(close(tl.scalar("x"), 10.0), "halfway from 20 to 0");
        tl.seek(2.0);
        assert!(close(tl.scalar("x"), 0.0));
    }

    #[test]
    fn labels_position_later_insertions() {
        let mut tl = Sequence::new();
        tl.to("x", 1.0, 2.0, Curve::Linear, ">")
            .label("mid", 1.0)
            .to("y", 1.0, 1.0, Curve::Linear, "mid+=0.5");
        tl.seek_label("mid");
        assert!(close(tl.time(), 1.0));
        tl.seek(1.5);
        assert!(close(tl.scalar("y"), 0.0));
        tl.seek(2.0);
        assert!(close(tl.scalar("y"), 0.5));
    }

    #[test]
    fn stagger_orders_by_from() {
        let mut tl = Sequence::new();
        let ch = ["a", "b", "c", "d", "e"];
        tl.stagger(
            &ch,
            0.0,
            1.0,
            1.0,
            0.1,
            StaggerFrom::Center,
            Curve::Linear,
            0.0,
        );
        tl.seek(0.2);
        // centre starts first
        assert!(tl.scalar("c") > tl.scalar("b"));
        assert!(close(tl.scalar("a"), tl.scalar("e")));
        assert!(close(tl.duration(), 1.2));
    }

    #[test]
    fn nesting_shifts_scales_and_prefixes_labels() {
        let mut child = Sequence::new();
        child
            .from_to("x", 0.0, 1.0, 1.0, Curve::Linear, 0.0)
            .label("end", ">");
        child.time_scale(2.0);
        let mut parent = Sequence::new();
        parent
            .to("y", 1.0, 1.0, Curve::Linear, ">")
            .add("child", &child, ">");
        assert!(close(parent.duration(), 1.5));
        assert_eq!(parent.label_time("child/end"), Some(1.5));
        parent.seek(1.25);
        assert!(close(parent.scalar("x"), 0.5));
    }

    #[test]
    fn calls_fire_once_in_either_direction() {
        let mut tl = Sequence::new();
        tl.to("x", 1.0, 2.0, Curve::Linear, ">").call("ping", 1.0);
        let fwd: Vec<String> = (0..30)
            .flat_map(|_| tl.advance(0.1))
            .map(|e| e.name)
            .collect();
        assert_eq!(fwd, ["start", "ping", "complete"]);
        tl.reverse();
        let back: Vec<String> = (0..30)
            .flat_map(|_| tl.advance(0.1))
            .map(|e| e.name)
            .collect();
        assert_eq!(back, ["ping", "reverse-complete"]);
    }

    #[test]
    fn repeat_yoyo_folds_the_clock_and_announces_repeats() {
        let mut tl = Sequence::new();
        tl.from_to("x", 0.0, 1.0, 1.0, Curve::Linear, 0.0)
            .call("mid", 0.5)
            .repeat(2)
            .yoyo(true);
        assert!(close(tl.total_duration(), 3.0));
        tl.seek(1.25);
        assert!(close(tl.scalar("x"), 0.75), "second pass runs backwards");
        tl.seek(0.0);
        let names: Vec<String> = (0..40)
            .flat_map(|_| tl.advance(0.1))
            .map(|e| e.name)
            .collect();
        assert_eq!(
            names,
            ["start", "mid", "repeat", "mid", "repeat", "mid", "complete"]
        );
        assert!(
            close(tl.scalar("x"), 1.0),
            "odd number of passes ends at the end"
        );
    }

    #[test]
    fn time_scale_and_pause() {
        let mut tl = Sequence::new();
        tl.from_to("x", 0.0, 1.0, 1.0, Curve::Linear, 0.0)
            .time_scale(2.0);
        tl.advance(0.25);
        assert!(close(tl.scalar("x"), 0.5));
        tl.pause();
        tl.advance(1.0);
        assert!(close(tl.scalar("x"), 0.5));
    }

    #[test]
    fn stepped_and_sought_agree() {
        let mut a = Sequence::new();
        a.from_to("x", 0.0, 5.0, 1.0, Curve::EASE_IN_OUT, 0.0).to(
            "x",
            -3.0,
            0.7,
            Curve::BOUNCE_OUT,
            ">0.2",
        );
        let mut b = a.clone();
        for _ in 0..100 {
            a.advance(0.013);
        }
        b.seek(1.3);
        assert!(close(a.scalar("x"), b.scalar("x")));
    }

    #[test]
    fn values_of_other_kinds_interpolate() {
        let mut tl = Sequence::new();
        tl.from_to(
            "p",
            Offset::new(0.0, 0.0),
            Offset::new(10.0, 20.0),
            1.0,
            Curve::Linear,
            0.0,
        )
        .from_to("c", Color::BLACK, Color::WHITE, 1.0, Curve::Linear, 0.0);
        tl.seek(0.5);
        assert_eq!(
            tl.value("p").and_then(Value::point),
            Some(Offset::new(5.0, 10.0))
        );
        assert_eq!(
            tl.value("c").and_then(Value::color),
            Some(Color::rgb(128, 128, 128))
        );
    }
}
