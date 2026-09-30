//! Non-linear animation: strips on tracks, event tracks, montages.
//!
//! # The three tools this answers
//!
//! * **Blender's NLA editor** (§2.14 L4, "NLA (Non-Linear Animation)") —
//!   actions become *strips* placed on *tracks*; a strip can be moved,
//!   repeated, time-scaled, reversed, faded in and out, and blended onto the
//!   tracks below it by **replace**, **add** or **multiply**. It is how a
//!   walk cycle and a wave get layered without re-keying either.
//! * **Godot's `AnimationPlayer` method tracks** and **Unity's animation
//!   events** / **Unreal's anim notifies** — named events at clip times that
//!   call back into game code ("footstep" at 0.4 s). [`EventTrack`].
//! * **Unreal's `AnimMontage`** (§2.17 L3) — one clip cut into named
//!   *sections* with next-section links, so gameplay can jump to "Attack2"
//!   and have "Recover" follow, or loop "Hold" until released. [`Montage`].
//!
//! Everything samples [`Clip`](crate::animator::Clip)s into
//! [`Channels`](crate::animator::Channels), the same currency the
//! [`Animator`](crate::animator::Animator) uses, so an NLA stack can feed a
//! layer and vice versa.

use crate::animator::{Channels, Clip};

/// How a strip combines with what is below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StripBlend {
    #[default]
    Replace,
    Add,
    Multiply,
}

/// What a strip does outside its own span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StripExtend {
    /// Contribute nothing outside the strip.
    #[default]
    Nothing,
    /// Hold the last frame after the strip ends.
    HoldForward,
    /// Hold the first frame before and the last frame after.
    Hold,
}

/// One action placed on a track.
#[derive(Debug, Clone)]
pub struct Strip {
    pub name: String,
    pub clip: Clip,
    /// Where the strip begins on the scene clock.
    pub start: f32,
    /// How many times the clip plays (fractional allowed).
    pub repeat: f32,
    /// Playback speed; 2.0 plays the clip in half the time.
    pub speed: f32,
    pub reversed: bool,
    pub blend_in: f32,
    pub blend_out: f32,
    pub blend: StripBlend,
    pub influence: f32,
    pub extend: StripExtend,
    pub muted: bool,
}

impl Strip {
    #[must_use]
    pub fn new(name: &str, clip: Clip, start: f32) -> Self {
        Self {
            name: name.to_owned(),
            clip,
            start,
            repeat: 1.0,
            speed: 1.0,
            reversed: false,
            blend_in: 0.0,
            blend_out: 0.0,
            blend: StripBlend::Replace,
            influence: 1.0,
            extend: StripExtend::Nothing,
            muted: false,
        }
    }

    #[must_use]
    pub const fn repeat(mut self, times: f32) -> Self {
        self.repeat = times;
        self
    }
    #[must_use]
    pub const fn speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }
    #[must_use]
    pub const fn reversed(mut self) -> Self {
        self.reversed = true;
        self
    }
    #[must_use]
    pub const fn fades(mut self, blend_in: f32, blend_out: f32) -> Self {
        self.blend_in = blend_in;
        self.blend_out = blend_out;
        self
    }
    #[must_use]
    pub const fn blend(mut self, blend: StripBlend) -> Self {
        self.blend = blend;
        self
    }
    #[must_use]
    pub const fn influence(mut self, w: f32) -> Self {
        self.influence = w;
        self
    }
    #[must_use]
    pub const fn extend(mut self, e: StripExtend) -> Self {
        self.extend = e;
        self
    }

    /// Length on the scene clock.
    #[must_use]
    pub fn length(&self) -> f32 {
        self.clip.length() * self.repeat / self.speed.max(1e-6)
    }

    #[must_use]
    pub fn end(&self) -> f32 {
        self.start + self.length()
    }

    /// Clip time and weight at scene time `t`, or `None` if silent.
    fn local(&self, t: f32) -> Option<(f32, f32)> {
        if self.muted {
            return None;
        }
        let len = self.length();
        let clip_len = self.clip.length();
        let (inside, rel) = if t < self.start {
            match self.extend {
                StripExtend::Hold => (false, 0.0),
                _ => return None,
            }
        } else if t > self.end() {
            match self.extend {
                StripExtend::Nothing => return None,
                _ => (false, len),
            }
        } else {
            (true, t - self.start)
        };
        let clip_time_total = rel * self.speed;
        let mut clip_time = if clip_len > 0.0 {
            if clip_time_total >= clip_len * self.repeat {
                // The very end of the last repeat, not the wrapped start.
                let frac = self.repeat.fract();
                if frac == 0.0 {
                    clip_len
                } else {
                    frac * clip_len
                }
            } else {
                clip_time_total.rem_euclid(clip_len)
            }
        } else {
            0.0
        };
        if self.reversed {
            clip_time = clip_len - clip_time;
        }
        let mut w = self.influence;
        if inside {
            if self.blend_in > 0.0 {
                w *= (rel / self.blend_in).clamp(0.0, 1.0);
            }
            if self.blend_out > 0.0 {
                w *= ((len - rel) / self.blend_out).clamp(0.0, 1.0);
            }
        }
        Some((clip_time, w))
    }
}

/// Strips that do not overlap in time, and whether the track plays.
#[derive(Debug, Clone, Default)]
pub struct Track {
    pub name: String,
    pub strips: Vec<Strip>,
    pub muted: bool,
    pub solo: bool,
}

impl Track {
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn strip(mut self, s: Strip) -> Self {
        self.strips.push(s);
        self.strips.sort_by(|a, b| a.start.total_cmp(&b.start));
        self
    }
}

/// Tracks bottom to top.
#[derive(Debug, Clone, Default)]
pub struct Nla {
    pub tracks: Vec<Track>,
}

impl Nla {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn track(mut self, t: Track) -> Self {
        self.tracks.push(t);
        self
    }

    /// End of the last strip.
    #[must_use]
    pub fn length(&self) -> f32 {
        self.tracks
            .iter()
            .flat_map(|t| t.strips.iter().map(Strip::end))
            .fold(0.0, f32::max)
    }

    /// Every channel at scene time `t`, starting from `base`.
    #[must_use]
    pub fn evaluate(&self, t: f32, base: &Channels) -> Channels {
        let any_solo = self.tracks.iter().any(|t| t.solo);
        let mut out = base.clone();
        for track in &self.tracks {
            if track.muted || (any_solo && !track.solo) {
                continue;
            }
            // Of a track's strips, the one covering `t`, else the nearest
            // one extending over it.
            let active = track
                .strips
                .iter()
                .filter_map(|s| s.local(t).map(|l| (s, l)))
                .min_by(|a, b| {
                    let da = if t >= a.0.start && t <= a.0.end() { 0.0 } else { (t - a.0.end()).abs() };
                    let db = if t >= b.0.start && t <= b.0.end() { 0.0 } else { (t - b.0.end()).abs() };
                    da.total_cmp(&db)
                });
            let Some((strip, (clip_t, w))) = active else { continue };
            for (k, v) in strip.clip.sample(clip_t) {
                let below = out.get(&k).copied();
                let next = match strip.blend {
                    StripBlend::Replace => {
                        let b = below.unwrap_or(v);
                        b + (v - b) * w
                    }
                    StripBlend::Add => below.unwrap_or(0.0) + v * w,
                    StripBlend::Multiply => {
                        let b = below.unwrap_or(1.0);
                        b * (1.0 + (v - 1.0) * w)
                    }
                };
                out.insert(k, next);
            }
        }
        out
    }
}

/// Named moments on a clip's clock — method tracks, animation events,
/// notifies.
#[derive(Debug, Clone, Default)]
pub struct EventTrack {
    events: Vec<(f32, String)>,
}

impl EventTrack {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn at(mut self, time: f32, name: &str) -> Self {
        self.events.push((time, name.to_owned()));
        self.events.sort_by(|a, b| a.0.total_cmp(&b.0));
        self
    }

    /// Events in `(from, to]`, in order; for a looping clip of `length`,
    /// wrap-around crossings are included.
    #[must_use]
    pub fn crossed(&self, from: f32, to: f32, length: Option<f32>) -> Vec<&str> {
        let mut out = Vec::new();
        match length {
            Some(len) if len > 0.0 && to - from >= 0.0 => {
                let mut a = from;
                while a < to {
                    let cycle = (a / len).floor();
                    let cycle_end = (cycle + 1.0) * len;
                    let b = to.min(cycle_end);
                    let (la, lb) = (a - cycle * len, b - cycle * len);
                    out.extend(
                        self.events
                            .iter()
                            .filter(|(t, _)| *t > la && *t <= lb)
                            .map(|(_, n)| n.as_str()),
                    );
                    if b <= a {
                        break;
                    }
                    a = b;
                }
            }
            _ => out.extend(
                self.events
                    .iter()
                    .filter(|(t, _)| *t > from && *t <= to)
                    .map(|(_, n)| n.as_str()),
            ),
        }
        out
    }
}

/// A named slice of a montage's clip.
#[derive(Debug, Clone)]
pub struct Section {
    pub name: String,
    pub start: f32,
    pub end: f32,
    /// What plays after this section; itself for a loop; `None` to stop.
    pub next: Option<String>,
}

/// One clip in sections — Unreal's `AnimMontage`.
#[derive(Debug, Clone)]
pub struct Montage {
    clip: Clip,
    sections: Vec<Section>,
    events: EventTrack,
    current: Option<usize>,
    time: f32,
}

impl Montage {
    #[must_use]
    pub fn new(clip: Clip) -> Self {
        Self {
            clip,
            sections: Vec::new(),
            events: EventTrack::new(),
            current: None,
            time: 0.0,
        }
    }

    #[must_use]
    pub fn section(mut self, name: &str, start: f32, end: f32, next: Option<&str>) -> Self {
        self.sections.push(Section {
            name: name.to_owned(),
            start,
            end,
            next: next.map(str::to_owned),
        });
        self
    }

    #[must_use]
    pub fn events(mut self, events: EventTrack) -> Self {
        self.events = events;
        self
    }

    /// Start (or jump) to a section.
    pub fn play(&mut self, section: &str) -> bool {
        match self.sections.iter().position(|s| s.name == section) {
            Some(i) => {
                self.current = Some(i);
                self.time = self.sections[i].start;
                true
            }
            None => false,
        }
    }

    /// Change what follows `section` at runtime (Unreal's
    /// `Montage_SetNextSection`).
    pub fn set_next(&mut self, section: &str, next: Option<&str>) {
        if let Some(s) = self.sections.iter_mut().find(|s| s.name == section) {
            s.next = next.map(str::to_owned);
        }
    }

    /// The playing section's name.
    #[must_use]
    pub fn current(&self) -> Option<&str> {
        self.current.map(|i| self.sections[i].name.as_str())
    }

    #[must_use]
    pub const fn time(&self) -> f32 {
        self.time
    }

    #[must_use]
    pub const fn is_playing(&self) -> bool {
        self.current.is_some()
    }

    /// Advance, following section links; returns events crossed and
    /// section changes (`"section:<name>"`).
    pub fn advance(&mut self, mut dt: f32) -> Vec<String> {
        let mut out = Vec::new();
        let mut guard = 0;
        while let Some(i) = self.current {
            guard += 1;
            if guard > 64 || dt <= 0.0 {
                break;
            }
            let s = self.sections[i].clone();
            let room = s.end - self.time;
            let step = dt.min(room);
            out.extend(self.events.crossed(self.time, self.time + step, None).into_iter().map(str::to_owned));
            self.time += step;
            dt -= step;
            if self.time >= s.end - 1e-6 {
                match s.next.as_deref().and_then(|n| self.sections.iter().position(|x| x.name == n)) {
                    Some(n) => {
                        self.current = Some(n);
                        self.time = self.sections[n].start;
                        out.push(format!("section:{}", self.sections[n].name));
                    }
                    None => {
                        self.current = None;
                        out.push("end".into());
                    }
                }
            }
        }
        out
    }

    /// The pose at the montage's playhead.
    #[must_use]
    pub fn sample(&self) -> Channels {
        self.clip.sample(self.time)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyframe::{Keyframe, Keyframes};
    use crate::Curve;

    fn ramp(ch: &str, a: f32, b: f32, len: f32) -> Clip {
        Clip::new().track(ch, Keyframes::new(a).with(Keyframe::to(len, b).curve(Curve::Linear)))
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn strips_repeat_scale_and_reverse() {
        let s = Strip::new("walk", ramp("x", 0.0, 10.0, 1.0), 2.0).repeat(2.0).speed(2.0);
        assert!(close(s.length(), 1.0));
        let nla = Nla::new().track(Track::new("t").strip(s));
        assert!(close(nla.evaluate(2.25, &Channels::new())["x"], 5.0));
        assert!(close(nla.evaluate(2.75, &Channels::new())["x"], 5.0), "second repeat");
        assert!(nla.evaluate(1.0, &Channels::new()).is_empty(), "silent before");
        let r = Nla::new().track(Track::new("t").strip(Strip::new("r", ramp("x", 0.0, 10.0, 1.0), 0.0).reversed()));
        assert!(close(r.evaluate(0.25, &Channels::new())["x"], 7.5));
    }

    #[test]
    fn replace_add_and_multiply_layer_up() {
        let base = Track::new("base").strip(Strip::new("b", ramp("x", 10.0, 10.0, 1.0), 0.0));
        let add = Track::new("add").strip(Strip::new("a", ramp("x", 5.0, 5.0, 1.0), 0.0).blend(StripBlend::Add));
        let mul = Track::new("mul").strip(Strip::new("m", ramp("x", 2.0, 2.0, 1.0), 0.0).blend(StripBlend::Multiply).influence(0.5));
        let nla = Nla::new().track(base).track(add).track(mul);
        // (10 + 5) * (1 + (2-1)*0.5) = 22.5
        assert!(close(nla.evaluate(0.5, &Channels::new())["x"], 22.5));
    }

    #[test]
    fn fades_and_hold_extension() {
        let s = Strip::new("s", ramp("x", 10.0, 10.0, 2.0), 0.0).fades(1.0, 0.0).extend(StripExtend::HoldForward);
        let mut base = Channels::new();
        base.insert("x".into(), 0.0);
        let nla = Nla::new().track(Track::new("t").strip(s));
        assert!(close(nla.evaluate(0.5, &base)["x"], 5.0), "half faded in");
        assert!(close(nla.evaluate(5.0, &base)["x"], 10.0), "held forward");
    }

    #[test]
    fn mute_and_solo() {
        let a = Track::new("a").strip(Strip::new("a", ramp("x", 1.0, 1.0, 1.0), 0.0));
        let mut b = Track::new("b").strip(Strip::new("b", ramp("x", 2.0, 2.0, 1.0), 0.0));
        b.muted = true;
        let nla = Nla::new().track(a.clone()).track(b.clone());
        assert!(close(nla.evaluate(0.5, &Channels::new())["x"], 1.0));
        let mut a2 = a;
        a2.solo = true;
        b.muted = false;
        let nla = Nla::new().track(a2).track(b);
        assert!(close(nla.evaluate(0.5, &Channels::new())["x"], 1.0), "solo silences others");
    }

    #[test]
    fn event_tracks_report_crossings_including_loops() {
        let ev = EventTrack::new().at(0.2, "step-l").at(0.7, "step-r");
        assert_eq!(ev.crossed(0.0, 0.5, None), ["step-l"]);
        assert_eq!(ev.crossed(0.5, 1.5, Some(1.0)), ["step-r", "step-l"]);
        assert!(ev.crossed(0.2, 0.2, None).is_empty());
    }

    #[test]
    fn montage_follows_and_rewires_sections() {
        let clip = ramp("x", 0.0, 30.0, 3.0);
        let mut m = Montage::new(clip)
            .section("wind", 0.0, 1.0, Some("hold"))
            .section("hold", 1.0, 2.0, Some("hold"))
            .section("release", 2.0, 3.0, None)
            .events(EventTrack::new().at(2.5, "hit"));
        assert!(m.play("wind"));
        let e = m.advance(1.5);
        assert_eq!(e, ["section:hold"]);
        m.advance(2.0);
        assert_eq!(m.current(), Some("hold"), "loops while held");
        m.set_next("hold", Some("release"));
        let e = m.advance(3.0);
        assert!(e.contains(&"section:release".to_owned()) && e.contains(&"hit".to_owned()) && e.last().unwrap() == "end");
        assert!(!m.is_playing());
    }
}
