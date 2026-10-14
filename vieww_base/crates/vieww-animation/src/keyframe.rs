//! Keyframes: more than two points, on a timeline.
//!
//! [`Tween`](crate::Tween) is an endpoint pair, and an endpoint pair is the
//! whole story only when a value has somewhere to be and one way to get there.
//! The moment it has **several** places to be — three colours a brand cycles
//! through, a knob that detents at 0%, 25% and 100%, a caption that grows,
//! holds, then shrinks — a chain of tweens has to be scheduled by hand, and
//! the hand that does it is the application's, forever.
//!
//! A [`Keyframes<T>`](Keyframes) is that scheduling, done once, sampled by
//! time. This module is what "keyframe timeline" means in Motion Canvas, After
//! Effects and Rive — the one gap in this crate's own comparison table that
//! this file closes.
//!
//! # The three shapes a keyframe can take
//!
//! | shape | spelling | what it is for |
//! |---|---|---|
//! | eased | [`Keyframe::to`](Keyframe::to) | arrive at the value along a curve |
//! | held | [`Keyframe::hold`](Keyframe::hold) | step there instantly — a toggle, a cut |
//! | arrival-only | [`Keyframe::at`](Keyframe::at) | the curve is decided by the *next* keyframe |
//!
//! The distinction that matters is that **the curve belongs to the segment**,
//! and a segment is named by the keyframe that ends it. That is the spelling
//! every timeline editor uses — After Effects puts the interpolator on the
//! keyframe the segment leaves, Motion Canvas puts the easing on the value at
//! the end of the segment — and it means a keyframe list reads as
//! "by 200 ms be here, eased like this".
//!
//! # Two clocks, one rule
//!
//! Keyframes are placed in **seconds** and sampled in **seconds**, because a
//! timeline is authored by a human against a ruler ("the logo holds for half a
//! second") and the ruler a human reads is seconds. The rest of this crate
//! works in `Duration`, so [`Keyframes::at`](Keyframes::at) takes a `Duration`
//! and the seconds stay inside, where an editor UI can keep using them.
//!
//! ```
//! use std::time::Duration;
//! use vieww_animation::{Curve, Keyframe, Keyframes};
//! use vieww_foundation::Color;
//!
//! fn ms(ms: u64) -> Duration {
//!     Duration::from_millis(ms)
//! }
//!
//! // A colour that sits on red, eases to blue, holds, then steps to green.
//! let colour = Keyframes::new(Color::RED)
//!     .with(Keyframe::to(1.0, Color::BLUE).curve(Curve::EASE_IN_OUT))
//!     .with(Keyframe::hold(2.0, Color::GREEN));
//!
//! assert_eq!(colour.at(ms(0)), Color::RED);
//! assert_eq!(colour.at(ms(500)), Color::rgb(128, 0, 128), "mid-way is the mix");
//! assert_eq!(colour.at(ms(1000)), Color::BLUE);
//! assert_eq!(colour.at(ms(1500)), Color::BLUE); // held, not eased on
//! assert_eq!(colour.at(ms(2500)), Color::GREEN); // stepped
//! ```
//!
//! # Why this is not a `Vec<(f32, T)>`
//!
//! Because the empty case, the one-keyframe case, out-of-order insertion and
//! duplicate times are all *states*, and leaving them to a naked vector makes
//! each of them somebody's runtime surprise:
//!
//! * empty is unrepresentable here — [`Keyframes::new`](Keyframes::new) takes
//!   the first value, so the type cannot say "no timeline" and nothing
//!   downstream has to handle it;
//! * insertion sorts, so out-of-order authoring is fine;
//! * a keyframe at the same time as one already there **replaces** it, the way
//!   every timeline editor does, rather than making `at` return either.
//!
//! [`Timeline`] then lifts one of these to a *set of named tracks*
//! with per-track delays — the staggered reveal pattern, in one type.

use std::time::Duration;

use crate::curve::Curve;
use crate::tween::Lerp;

/// One point on a keyframe track.
///
/// Construct through the helpers — [`to`](Self::to) for an eased arrival,
/// [`hold`](Self::hold) for a step — or [`at`](Self::at) when the easing is
/// none of this keyframe's business (a builder-style `.curve()` on it decides).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Keyframe<T> {
    /// When this keyframe is reached, in seconds from the track's start.
    pub time: f32,
    /// The value in force from this keyframe's time until the next one's.
    pub value: T,
    /// How the *segment ending here* travels.
    ///
    /// `None` is a step: the value jumps at `time`. That is [`hold`]'s
    /// behaviour, and `at`'s default, because a keyframe that says nothing
    /// about how it was reached should not invent a smooth one — a cut is the
    /// honest reading of "be this value at this time".
    ///
    /// [`hold`]: Self::hold
    pub curve: Option<Curve>,
}

impl<T> Keyframe<T> {
    /// Arrive at `value` at `time` (seconds), along `curve`.
    #[must_use]
    pub fn to(time: f32, value: T) -> Self {
        Self {
            time,
            value,
            // EASE_IN_OUT is the default the rest of this crate uses for
            // motion that both starts and stops on screen; `Linear` is one
            // `.curve()` away for the spinners that want it.
            curve: Some(Curve::EASE_IN_OUT),
        }
    }

    /// **Step** to `value` at `time` (seconds). No interpolation before it.
    ///
    /// A cut, a toggle, a discrete detent — the value is whatever the previous
    /// keyframe said, right up to this instant.
    #[must_use]
    pub fn hold(time: f32, value: T) -> Self {
        Self {
            time,
            value,
            curve: None,
        }
    }

    /// Reach `value` at `time` (seconds), deciding *how* later.
    ///
    /// The easing of the segment that ends here is whatever this keyframe
    /// ends up carrying — a step, unless a `.curve()` call sets one. This is
    /// the constructor for timelines authored in data, where the curve
    /// arrives from a file rather than from the next line of code.
    #[must_use]
    pub fn at(time: f32, value: T) -> Self {
        Self {
            time,
            value,
            curve: None,
        }
    }

    /// Set the curve of the segment that *ends* at this keyframe.
    #[must_use]
    pub const fn curve(mut self, curve: Curve) -> Self {
        self.curve = Some(curve);
        self
    }
}

/// What a track does when the clock passes its last keyframe.
///
/// The default is [`Clamp`](LoopMode::Clamp) — hold the last value — because
/// that is the only answer that is always a *finished* animation. The other
/// two are the spellings every animation tool names: **loop** (GSAP `repeat:
/// -1`, Godot and Unity clips, a spinner, a breathing light) wraps the clock
/// back to the first keyframe and plays again; **ping-pong** (GSAP `yoyo`,
/// a shuttle, a sweeping radar) plays it backwards from the end instead.
///
/// ```text
/// Clamp:    0 ──▶ 1 ──────▶ (holds 1)
/// Loop:     0 ──▶ 1 ──▶ 0 ──▶ 1 ──▶ ...
/// PingPong: 0 ──▶ 1 ──▶ 0 ──▶ 1 ──▶ ... (the way back is the same track)
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoopMode {
    /// Hold the last keyframe's value forever. The default.
    #[default]
    Clamp,
    /// Wrap to the first keyframe and play again, forever.
    Loop,
    /// Play to the end, then backwards to the start, forever.
    PingPong,
}

/// A keyframed value: any number of points, in any order, sampled by time.
///
/// The first value is the value at `t = 0`, and it is a constructor argument
/// rather than a list entry because a track with nothing at time zero still
/// *has* a value at time zero — the first one — and making that unspeakable
/// moves the decision to a runtime branch.
///
/// Before the first keyframe and after the last, the track **holds** rather
/// than extrapolates. A tween extrapolating past its end is an overshoot,
/// which is motion; a keyframe track extrapolating past its end is a value
/// nobody authored, which is a bug. The last keyframe is where a timeline
/// *finishes*.
#[derive(Debug, Clone, PartialEq)]
pub struct Keyframes<T> {
    /// Sorted by `time`, always — [`insert`](Self::insert) keeps it so.
    /// Never empty: the constructor's first value sits at `t = 0`.
    frames: Vec<Keyframe<T>>,
    /// What happens past the end — see [`LoopMode`]. It changes only how the
    /// clock is *read*, never the keyframes themselves, so a looping track
    /// and its clamped twin share one authored list.
    mode: LoopMode,
}

impl<T> Keyframes<T> {
    /// A track that is `first`, and nothing else yet.
    #[must_use]
    pub fn new(first: T) -> Self {
        Self {
            frames: vec![Keyframe::hold(0.0, first)],
            mode: LoopMode::Clamp,
        }
    }

    /// Loop forever: at the end, wrap to the start. GSAP's `repeat: -1`,
    /// every engine's clip loop. Returns `self` for builder chains.
    ///
    /// A one-keyframe track loops as itself — the mode is harmless there,
    /// and refusing it would be a special case a builder does not need.
    #[must_use]
    pub const fn looping(mut self) -> Self {
        self.mode = LoopMode::Loop;
        self
    }

    /// Loop by bouncing: play forwards, then backwards, forever. GSAP's
    /// `yoyo`, a shuttle, a radar sweep. Returns `self` for builder chains.
    #[must_use]
    pub const fn ping_pong(mut self) -> Self {
        self.mode = LoopMode::PingPong;
        self
    }

    /// The track's loop mode — an editor's view of how it was authored.
    #[must_use]
    pub const fn mode(&self) -> LoopMode {
        self.mode
    }

    /// Add a keyframe, keeping the list time-sorted. Returns `self`, for
    /// builder chains.
    ///
    /// A keyframe whose time is already taken **replaces** the one there.
    /// That is what every timeline editor does when you drag one point onto
    /// another, and the alternative — two keyframes at one instant, with the
    /// sample order between them decided by a sort's tie-break — is a
    /// one-frame flicker that survives every test that does not hit the exact
    /// boundary.
    #[must_use]
    pub fn with(mut self, frame: Keyframe<T>) -> Self {
        self.insert(frame);
        self
    }

    /// Insert in place, time-sorted, replacing a same-time keyframe.
    ///
    /// A keyframe at or before the track's start *becomes* the start: the
    /// earliest authored time wins, which is also the only reading that keeps
    /// the list free of two frames at one instant.
    ///
    /// See [`with`](Self::with) for the replacement rule.
    pub fn insert(&mut self, frame: Keyframe<T>) {
        // The first frame is the track's start by construction, so a keyframe
        // authored at or before it takes the start position — replacing, not
        // stacking, because two frames at t = 0 is a one-frame flicker.
        if frame.time <= self.frames[0].time {
            let mut frame = frame;
            frame.time = self.frames[0].time;
            self.frames[0] = frame;
            return;
        }
        // The tail is sorted and strictly increasing in time (replacement
        // above and here both forbid duplicates), so binary search applies.
        let found = self.frames[1..].binary_search_by(|probe| {
            probe
                .time
                .partial_cmp(&frame.time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        match found {
            Ok(index) => self.frames[index + 1] = frame,
            Err(index) => self.frames.insert(index + 1, frame),
        }
    }

    /// When the track's last keyframe is — its end.
    ///
    /// A track is finished when its last authored value is reached, and this
    /// is that instant. [`Timeline::duration`](Timeline::duration) is the
    /// maximum of this over its tracks, plus their delays.
    #[must_use]
    pub fn end(&self) -> Duration {
        let last = self.frames[self.frames.len() - 1].time;
        Duration::from_secs_f32(last.max(0.0))
    }

    /// The keyframes, in time order, for an editor or a serialisation.
    #[must_use]
    pub fn frames(&self) -> &[Keyframe<T>] {
        &self.frames
    }
}

impl<T: Lerp> Keyframes<T> {
    /// The value at `elapsed`.
    ///
    /// Holds before the first and after the last keyframe — see the type's
    /// docs for why holding is the only defensible answer at both ends —
    /// unless the track is [`looping`](Self::looping) or
    /// [`ping_pong`](Self::ping_pong), in which case the clock is folded
    /// back into the track's span first and the answer never leaves it.
    #[must_use]
    pub fn at(&self, elapsed: Duration) -> T {
        let end = self.frames[self.frames.len() - 1].time;
        let t = fold(self.mode, elapsed.as_secs_f32(), end);
        self.at_inside(t)
    }

    /// The sampling body: `t` is already inside the track's span.
    fn at_inside(&self, t: f32) -> T {
        let last = self.frames.len() - 1;
        if t <= self.frames[0].time || last == 0 {
            return self.frames[0].value.clone();
        }
        if t >= self.frames[last].time {
            return self.frames[last].value.clone();
        }

        // `frames` is sorted, so a scan for the bracketing pair is correct;
        // binary search would be faster, and a timeline with enough keyframes
        // to notice has bigger problems than this scan.
        let mut upper = 1;
        while self.frames[upper].time < t {
            upper += 1;
        }
        let from = &self.frames[upper - 1];
        let to = &self.frames[upper];

        let span = to.time - from.time;
        let progress = ((t - from.time) / span).clamp(0.0, 1.0);
        let Some(curve) = to.curve else {
            // A **step**: the previous value holds right up to the keyframe's
            // own instant (progress 1), where the value becomes the
            // keyframe's. A segment that reaches its end keyframe before the
            // clock does is a hold, not a fade.
            return if progress >= 1.0 {
                to.value.clone()
            } else {
                from.value.clone()
            };
        };
        from.value
            .clone()
            .lerp(to.value.clone(), curve.transform(progress))
    }
}

/// A set of named keyframed tracks with per-track delays: one stagger, many
/// values.
///
/// The pattern this type exists for is the one every motion system
/// rediscovers: a reveal where the *title* fades in at 0 ms, the *subtitle* at
/// 120 ms and the *rule* at 240 ms. Three [`AnimationController`](crate::AnimationController)s
/// and a scheduler can do it; here the staggering is data, and
/// [`at`](Self::at) is the scheduler.
///
/// ```
/// use std::time::Duration;
/// use vieww_animation::{Keyframe, Keyframes, Timeline};
///
/// let mut reveal = Timeline::new();
/// reveal.track("title", 0.0, Keyframes::new(0.0).with(Keyframe::to(0.6, 1.0)));
/// reveal.track("subtitle", 0.12, Keyframes::new(0.0).with(Keyframe::to(0.6, 1.0)));
///
/// let halfway = reveal.at(Duration::from_millis(300));
/// // The title is further along than the subtitle, by exactly 120 ms.
/// assert!(halfway["title"] > halfway["subtitle"]);
/// ```
///
/// Delays are per-track *starts*; a track's own keyframe times count from its
/// delay, not from the timeline's zero. A negative delay would pull a track
/// before the timeline's start and is clamped at insertion — authored data is
/// corrected loudly once, at the editor, and silently never here.
#[derive(Debug, Clone, PartialEq)]
pub struct Timeline<T> {
    tracks: Vec<Track<T>>,
}

/// One named, delayed track of a [`Timeline`].
#[derive(Debug, Clone, PartialEq)]
struct Track<T> {
    name: &'static str,
    delay: f32,
    frames: Keyframes<T>,
}

impl<T> Timeline<T> {
    /// An empty timeline. Add tracks with [`track`](Self::track).
    #[must_use]
    pub const fn new() -> Self {
        Self { tracks: Vec::new() }
    }

    /// Add (or replace) a named track that starts `delay` seconds in.
    ///
    /// Replacing rather than appending a same-named track, for the same reason
    /// a keyframe at a taken time replaces: two tracks under one name is a
    /// `["title"]` lookup whose answer depends on insertion order, which is a
    /// decision made by a `Vec`'s internals rather than by anybody.
    pub fn track(&mut self, name: &'static str, delay: f32, frames: Keyframes<T>) -> &mut Self {
        if let Some(existing) = self.tracks.iter_mut().find(|track| track.name == name) {
            existing.delay = delay.max(0.0);
            existing.frames = frames;
        } else {
            self.tracks.push(Track {
                name,
                delay: delay.max(0.0),
                frames,
            });
        }
        self
    }

    /// When the last track finishes, delay included.
    ///
    /// The instant a "play the timeline" controller should report done, and
    /// the number a progress bar over a staggered reveal is a fraction of.
    /// An empty timeline lasts zero seconds rather than panicking, because
    /// "nothing to play" is a state an editor legitimately produces.
    #[must_use]
    pub fn duration(&self) -> Duration {
        let end = self
            .tracks
            .iter()
            .map(|track| track.delay + track.frames.end().as_secs_f32())
            .fold(0.0_f32, f32::max);
        Duration::from_secs_f32(end.max(0.0))
    }

    /// The track names, in insertion order.
    #[must_use]
    pub fn names(&self) -> Vec<&'static str> {
        self.tracks.iter().map(|track| track.name).collect()
    }
}

impl<T: Lerp> Timeline<T> {
    /// Sample every track at `elapsed`, as a frame of named values.
    ///
    /// A track before its delay reports its first value — a timeline is
    /// "not yet" there rather than "nothing", and `None` would make every
    /// caller write a fallback with no first value at hand.
    #[must_use]
    pub fn at(&self, elapsed: Duration) -> TimelineFrame<'_, T> {
        let t = elapsed.as_secs_f32();
        TimelineFrame {
            values: self
                .tracks
                .iter()
                .map(|track| {
                    let local = (t - track.delay).max(0.0);
                    let value = track.frames.at(Duration::from_secs_f32(local));
                    (track.name, value)
                })
                .collect(),
        }
    }
}

impl<T> Default for Timeline<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// One sampled instant of a [`Timeline`]: a name per value.
///
/// Index by name — `frame["title"]` — which is the spelling the stagger
/// pattern reads best in. An unknown name panics, the same way
/// `HashMap::index` does, because a typo'd track name is a programming error
/// and deserves the loudest answer available, not a silent fallback.
pub struct TimelineFrame<'a, T> {
    values: Vec<(&'a str, T)>,
}

impl<T: std::fmt::Debug> std::fmt::Debug for TimelineFrame<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map()
            .entries(self.values.iter().map(|(name, value)| (*name, value)))
            .finish()
    }
}

impl<T> TimelineFrame<'_, T> {
    /// The value of `name` at this instant.
    ///
    /// # Panics
    ///
    /// If `name` is not a track of the timeline this frame came from.
    #[must_use]
    pub fn value(&self, name: &str) -> &T {
        self.values
            .iter()
            .find(|(track, _)| *track == name)
            .map(|(_, value)| value)
            .unwrap_or_else(|| panic!("no track named `{name}` on this timeline"))
    }

    /// The sampled values, as `(name, value)` pairs, in track order.
    #[must_use]
    pub fn values(&self) -> &[(&str, T)] {
        &self.values
    }
}

impl<T> std::ops::Index<&str> for TimelineFrame<'_, T> {
    type Output = T;

    fn index(&self, name: &str) -> &T {
        self.value(name)
    }
}

/// A keyframed duration — for animating the timings of things, which a
/// timeline editor does whenever an easing "speeds up" mid-segment.
impl Lerp for Duration {
    fn lerp(self, other: Self, t: f32) -> Self {
        Duration::from_secs_f32(self.as_secs_f32().lerp(other.as_secs_f32(), t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vieww_foundation::Color;

    fn ms(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    #[test]
    fn one_frame_is_a_constant() {
        let track: Keyframes<f32> = Keyframes::new(3.0);
        assert_eq!(track.at(ms(0)), 3.0);
        assert_eq!(track.at(ms(10_000)), 3.0);
        assert_eq!(track.end(), ms(0));
    }

    #[test]
    fn eased_segment_hits_its_endpoints() {
        let track = Keyframes::new(0.0).with(Keyframe::to(1.0, 10.0));
        assert_eq!(track.at(ms(0)), 0.0);
        assert_eq!(track.at(ms(500)), 5.0);
        assert_eq!(track.at(ms(1000)), 10.0);
    }

    #[test]
    fn holds_before_first_after_last() {
        let track = Keyframes::new(1.0).with(Keyframe::to(1.0, 2.0));
        assert_eq!(track.at(ms(500)), 1.5);
        assert_eq!(
            track.at(ms(5000)),
            2.0,
            "past the end: the last value holds"
        );
    }

    #[test]
    fn hold_steps_instead_of_easing() {
        let track = Keyframes::new(0.0)
            .with(Keyframe::to(1.0, 1.0))
            .with(Keyframe::hold(2.0, 5.0));
        assert_eq!(track.at(ms(1500)), 1.0, "right before the step");
        assert_eq!(track.at(ms(2500)), 5.0, "right after it");
        assert_eq!(track.at(ms(1999)), 1.0);
    }

    #[test]
    fn insertion_sorts_out_of_order_authoring() {
        let mut track = Keyframes::new(0.0);
        track.insert(Keyframe::to(3.0, 30.0));
        track.insert(Keyframe::to(1.0, 10.0));
        track.insert(Keyframe::to(2.0, 20.0));

        let times: Vec<f32> = track.frames().iter().map(|frame| frame.time).collect();
        assert_eq!(times, vec![0.0, 1.0, 2.0, 3.0]);
        assert_eq!(track.at(ms(2500)), 25.0);
    }

    #[test]
    fn same_time_replaces() {
        let mut track = Keyframes::new(0.0);
        track.insert(Keyframe::to(1.0, 10.0));
        track.insert(Keyframe::to(1.0, 20.0));

        assert_eq!(track.frames().len(), 2, "one keyframe at t = 1, not two");
        assert_eq!(track.at(ms(1000)), 20.0);
    }

    #[test]
    fn a_keyframe_at_or_before_zero_takes_the_start() {
        let mut track = Keyframes::new(5.0);
        track.insert(Keyframe::to(-1.0, 1.0));
        assert_eq!(
            track.frames().len(),
            1,
            "the start was replaced, not stacked"
        );
        assert_eq!(track.frames()[0].time, 0.0);
        assert_eq!(
            track.at(ms(0)),
            1.0,
            "the earliest authored value wins the start"
        );

        // And at exactly zero, which is the more common authoring slip.
        let mut track = Keyframes::new(5.0);
        track.insert(Keyframe::to(0.0, 2.0));
        assert_eq!(track.frames().len(), 1);
        assert_eq!(track.at(ms(0)), 2.0);
    }

    #[test]
    fn timeline_staggers_by_delay() {
        // Linear tracks, so the arithmetic a stagger claims to guarantee
        // ("exactly 120 ms behind") is the arithmetic the test reads.
        let mut timeline: Timeline<f32> = Timeline::new();
        timeline.track(
            "a",
            0.0,
            Keyframes::new(0.0).with(Keyframe::to(1.0, 1.0).curve(Curve::Linear)),
        );
        timeline.track(
            "b",
            0.5,
            Keyframes::new(0.0).with(Keyframe::to(1.0, 1.0).curve(Curve::Linear)),
        );

        let before = timeline.at(ms(250));
        assert_eq!(before["a"], 0.25);
        assert_eq!(before["b"], 0.0, "not started: first value, not a fraction");

        let after = timeline.at(ms(1250));
        assert_eq!(after["a"], 1.0);
        assert_eq!(after["b"], 0.75, "half a second behind, exactly");

        assert_eq!(timeline.duration(), ms(1500), "delay counts toward the end");
    }

    #[test]
    fn same_named_track_replaces() {
        let mut timeline: Timeline<f32> = Timeline::new();
        timeline.track("a", 0.0, Keyframes::new(0.0).with(Keyframe::to(1.0, 1.0)));
        timeline.track("a", 0.0, Keyframes::new(2.0));

        assert_eq!(timeline.names().len(), 1);
        assert_eq!(timeline.at(ms(0))["a"], 2.0);
    }

    #[test]
    fn empty_timeline_lasts_zero() {
        let timeline: Timeline<f32> = Timeline::new();
        assert_eq!(timeline.duration(), Duration::ZERO);
        assert!(timeline.names().is_empty());
    }

    #[test]
    fn negative_delay_is_clamped_not_rewound() {
        let mut timeline: Timeline<f32> = Timeline::new();
        timeline.track("a", -5.0, Keyframes::new(0.0).with(Keyframe::to(2.0, 1.0)));
        assert_eq!(timeline.at(ms(1000))["a"], 0.5);
    }

    #[test]
    fn colours_keyframe_like_everything_else() {
        let track = Keyframes::new(Color::RED)
            .with(Keyframe::to(1.0, Color::BLUE))
            .with(Keyframe::hold(2.0, Color::GREEN));

        // Half way to blue at an eased midpoint is the channel-wise mix.
        assert_eq!(track.at(ms(500)), Color::rgb(128, 0, 128));
        assert_eq!(track.at(ms(1000)), Color::BLUE);
        assert_eq!(track.at(ms(1500)), Color::BLUE);
        assert_eq!(track.at(ms(2500)), Color::GREEN);
    }

    #[test]
    fn durations_are_keyframable() {
        let track = Keyframes::new(Duration::ZERO).with(Keyframe::to(1.0, ms(1000)));
        assert_eq!(track.at(ms(500)), ms(500));
    }

    #[test]
    #[should_panic(expected = "no track named `nope`")]
    fn unknown_track_name_is_loud() {
        let mut timeline: Timeline<f32> = Timeline::new();
        timeline.track("a", 0.0, Keyframes::new(0.0));
        let _ = timeline.at(ms(0))["nope"];
    }

    #[test]
    fn frame_values_come_in_track_order() {
        let mut timeline: Timeline<f32> = Timeline::new();
        timeline.track("a", 0.0, Keyframes::new(1.0));
        timeline.track("b", 0.0, Keyframes::new(2.0));

        let frame = timeline.at(ms(0));
        let names: Vec<&str> = frame.values().iter().map(|(name, _)| *name).collect();
        assert_eq!(names, vec!["a", "b"]);
        assert_eq!(frame.values()[0].1, 1.0);
        assert_eq!(frame.values()[1].1, 2.0);
    }
}

/// Fold the clock into the track's span, per the mode.
///
/// Free-standing because the arithmetic is the whole of what "loop" and
/// "ping-pong" mean, and a named function says it once:
///
/// * `Clamp` — the clock, unchanged; the sampling body holds past the end.
/// * `Loop` — the clock modulo the span. Exactly at a multiple of the span
///   the remainder is zero, so the *first* keyframe is shown: a loop is a
///   cut, and cutting back to frame zero is what cutting back means.
/// * `PingPong` — the clock modulo *twice* the span, mirrored over the
///   second half. The mirror is `2·span − phase`, which is continuous at the
///   turn (phase = span gives span) and lands on zero at the far end.
///
/// A span of zero (a one-keyframe track) folds to zero under every mode —
/// there is no span to be inside, and the value is the only value either way.
fn fold(mode: LoopMode, t: f32, span: f32) -> f32 {
    if span <= 0.0 {
        return t.min(0.0);
    }
    match mode {
        LoopMode::Clamp => t,
        LoopMode::Loop => t.rem_euclid(span),
        LoopMode::PingPong => {
            let phase = t.rem_euclid(2.0 * span);
            if phase > span {
                2.0 * span - phase
            } else {
                phase
            }
        }
    }
}

#[cfg(test)]
mod loop_tests {
    use super::*;
    use std::time::Duration;

    fn ms(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    fn ramp() -> Keyframes<f32> {
        // 0 → 1 over one second, eased in-out.
        Keyframes::new(0.0).with(Keyframe::to(1.0, 1.0))
    }

    #[test]
    fn clamped_tracks_hold_their_last_value() {
        let track = ramp();
        assert_eq!(track.at(ms(1500)), 1.0);
        assert_eq!(track.at(ms(60_000)), 1.0);
        assert_eq!(track.mode(), LoopMode::Clamp);
    }

    #[test]
    fn looping_tracks_wrap_the_clock() {
        let track = ramp().looping();
        // A quarter past the end behaves like a quarter from the start.
        let over = track.at(ms(1250));
        let from = track.at(ms(250));
        assert!((over - from).abs() < 1e-4, "{over} vs {from}");
        assert_eq!(track.mode(), LoopMode::Loop);
    }

    #[test]
    fn a_loop_boundary_lands_on_the_first_keyframe() {
        let track = ramp().looping();
        // Exactly one span in: the cut back to frame zero.
        assert_eq!(track.at(ms(1000)), 0.0);
        assert_eq!(track.at(ms(2000)), 0.0);
        // And just *before* the cut, the last value.
        assert!((track.at(ms(999)) - 1.0).abs() < 1e-2);
    }

    #[test]
    fn ping_pong_plays_backwards_after_the_end() {
        let track = ramp().ping_pong();
        // 0.75 s past the start of the return journey is 0.25 s *into* it:
        // the value the forward pass had at 0.75 s... read back from the end.
        let returning = track.at(ms(1750));
        let forward_equivalent = track.at(ms(250));
        assert!(
            (returning - forward_equivalent).abs() < 1e-4,
            "{returning} vs {forward_equivalent}"
        );
        assert_eq!(track.mode(), LoopMode::PingPong);
    }

    #[test]
    fn ping_pong_is_continuous_at_the_turn() {
        let track = ramp().ping_pong();
        let before = track.at(ms(999));
        let at = track.at(ms(1000));
        let after = track.at(ms(1001));
        // No jump at the reversal: the values at ±1 ms straddle the endpoint
        // within a frame's worth of easing.
        assert!((before - 1.0).abs() < 2e-2);
        assert!((at - 1.0).abs() < 1e-3);
        assert!((after - 1.0).abs() < 2e-2);
    }

    #[test]
    fn ping_pong_returns_home() {
        let track = ramp().ping_pong();
        // Two spans in: back at the start.
        assert!(track.at(ms(2000)).abs() < 1e-3);
        assert!(track.at(ms(1999)) > 0.0, "approaching home from above");
    }

    #[test]
    fn looping_before_the_start_is_unchanged() {
        // The fold only affects the far end; before the first keyframe a
        // track holds its first value whatever its mode.
        let track = ramp().looping();
        assert_eq!(track.at(Duration::ZERO), 0.0);
        let _ = track.ping_pong().at(Duration::ZERO);
    }

    #[test]
    fn one_keyframe_tracks_survive_every_mode() {
        for track in [
            Keyframes::new(5.0),
            Keyframes::new(5.0).looping(),
            Keyframes::new(5.0).ping_pong(),
        ] {
            assert_eq!(track.at(ms(0)), 5.0);
            assert_eq!(track.at(ms(10_000)), 5.0);
        }
    }

    #[test]
    fn held_keyframes_loop_as_cuts() {
        // A hold-hold track is a square wave once looping: 0 for the first
        // half of the span, 1 for the second, cutting back to 0 at the wrap.
        // (A track that only *ends* on a hold has no second half — the value
        // 1 exists for one instant — so the track here returns to 0 at the
        // span's end, which is what a square wave actually is.)
        let track = Keyframes::new(0.0)
            .with(Keyframe::hold(0.25, 1.0))
            .with(Keyframe::hold(0.5, 0.0))
            .looping();
        assert_eq!(track.at(ms(100)), 0.0);
        assert_eq!(track.at(ms(300)), 1.0);
        assert_eq!(track.at(ms(600)), 0.0, "wrapped back to the hold");
        assert_eq!(track.at(ms(800)), 1.0);
        assert_eq!(track.at(ms(1100)), 0.0);
    }
}
