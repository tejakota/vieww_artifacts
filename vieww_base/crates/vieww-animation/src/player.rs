//! Godot's `AnimationPlayer` (§2.18 L3: "keyframe any property of any node
//! … `Track` types (Property, Method, Audio, Bezier)") — named animations
//! made of typed tracks, played, queued, seeked, cross-blended and looped
//! by one player.
//!
//! * **Value** tracks keyframe a named property with continuous, discrete
//!   or nearest update modes (Godot's `UPDATE_*`).
//! * **Bezier** tracks are 2D Bézier curves with explicit in/out handles —
//!   the curve editor's track kind.
//! * **Method** tracks call named methods (with arguments) when the
//!   playhead crosses their keys, in both directions, once per crossing.
//! * **Audio** tracks start clips at a time with an offset; the player
//!   reports a start event carrying where in the clip to begin, so a seek
//!   into the middle of a sound resumes it mid-sound.
//! * **Animation** tracks play other animations of the same player (a
//!   nested sequence).
//!
//! The player is advanced by deltas, keeps no wall clock, and returns the
//! property values and the events of that advance.

use std::collections::BTreeMap;

/// How a value track updates between keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UpdateMode {
    /// Interpolated.
    #[default]
    Continuous,
    /// Holds the last key.
    Discrete,
    /// Snaps to the nearest key.
    Nearest,
}

/// One Bézier key: value plus in/out handles as (Δtime, Δvalue).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BezierKey {
    pub time: f32,
    pub value: f32,
    pub in_handle: (f32, f32),
    pub out_handle: (f32, f32),
}

/// A track.
#[derive(Debug, Clone, PartialEq)]
pub enum Track {
    Value {
        path: String,
        keys: Vec<(f32, f32)>,
        mode: UpdateMode,
    },
    Bezier {
        path: String,
        keys: Vec<BezierKey>,
    },
    Method {
        target: String,
        keys: Vec<(f32, String, Vec<f32>)>,
    },
    Audio {
        keys: Vec<(f32, String, f32)>,
    },
    Animation {
        keys: Vec<(f32, String)>,
    },
}

/// A named animation.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Animation {
    pub length: f32,
    pub looping: bool,
    pub tracks: Vec<Track>,
}

/// Something that happened during an advance.
#[derive(Debug, Clone, PartialEq)]
pub enum PlayerEvent {
    Call {
        target: String,
        method: String,
        args: Vec<f32>,
    },
    /// Start `clip` at `offset` seconds into it.
    Audio {
        clip: String,
        offset: f32,
    },
    Started(String),
    Finished(String),
    Looped(String),
}

fn sample_value(keys: &[(f32, f32)], mode: UpdateMode, t: f32) -> Option<f32> {
    if keys.is_empty() {
        return None;
    }
    if t <= keys[0].0 {
        return Some(keys[0].1);
    }
    let i = keys.partition_point(|k| k.0 <= t);
    if i >= keys.len() {
        return Some(keys[keys.len() - 1].1);
    }
    let (a, b) = (keys[i - 1], keys[i]);
    Some(match mode {
        UpdateMode::Discrete => a.1,
        UpdateMode::Nearest => {
            if t - a.0 < b.0 - t {
                a.1
            } else {
                b.1
            }
        }
        UpdateMode::Continuous => {
            let u = (t - a.0) / (b.0 - a.0).max(1e-9);
            a.1 + (b.1 - a.1) * u
        }
    })
}

fn sample_bezier(keys: &[BezierKey], t: f32) -> Option<f32> {
    if keys.is_empty() {
        return None;
    }
    if t <= keys[0].time {
        return Some(keys[0].value);
    }
    let i = keys.partition_point(|k| k.time <= t);
    if i >= keys.len() {
        return Some(keys[keys.len() - 1].value);
    }
    let (a, b) = (keys[i - 1], keys[i]);
    // Control points in (time, value); solve x(s) = t by bisection.
    let p = [
        (a.time, a.value),
        (a.time + a.out_handle.0, a.value + a.out_handle.1),
        (b.time + b.in_handle.0, b.value + b.in_handle.1),
        (b.time, b.value),
    ];
    let bez = |s: f32, k: usize| {
        let c = [p[0], p[1], p[2], p[3]].map(|q| if k == 0 { q.0 } else { q.1 });
        let m = 1.0 - s;
        m * m * m * c[0] + 3.0 * m * m * s * c[1] + 3.0 * m * s * s * c[2] + s * s * s * c[3]
    };
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..40 {
        let mid = (lo + hi) * 0.5;
        if bez(mid, 0) < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some(bez((lo + hi) * 0.5, 1))
}

impl Animation {
    #[must_use]
    pub fn new(length: f32) -> Self {
        Self {
            length,
            looping: false,
            tracks: Vec::new(),
        }
    }

    #[must_use]
    pub fn looping(mut self) -> Self {
        self.looping = true;
        self
    }

    #[must_use]
    pub fn track(mut self, t: Track) -> Self {
        self.tracks.push(t);
        self
    }

    /// Every value and Bézier track's value at `t`.
    #[must_use]
    pub fn sample(&self, t: f32) -> BTreeMap<String, f32> {
        let mut out = BTreeMap::new();
        for tr in &self.tracks {
            match tr {
                Track::Value { path, keys, mode } => {
                    if let Some(v) = sample_value(keys, *mode, t) {
                        out.insert(path.clone(), v);
                    }
                }
                Track::Bezier { path, keys } => {
                    if let Some(v) = sample_bezier(keys, t) {
                        out.insert(path.clone(), v);
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// Events whose keys lie in the half-open interval swept from `from` to
    /// `to` (either direction).
    fn events(&self, from: f32, to: f32, out: &mut Vec<PlayerEvent>) {
        let crossed = |k: f32| {
            if to >= from {
                k > from && k <= to
            } else {
                k < from && k >= to
            }
        };
        for tr in &self.tracks {
            match tr {
                Track::Method { target, keys } => {
                    for (k, m, args) in keys {
                        if crossed(*k) {
                            out.push(PlayerEvent::Call {
                                target: target.clone(),
                                method: m.clone(),
                                args: args.clone(),
                            });
                        }
                    }
                }
                Track::Audio { keys } => {
                    for (k, clip, offset) in keys {
                        if crossed(*k) {
                            out.push(PlayerEvent::Audio {
                                clip: clip.clone(),
                                offset: *offset + (to - k).max(0.0),
                            });
                        }
                    }
                }
                Track::Animation { keys } => {
                    for (k, name) in keys {
                        if crossed(*k) {
                            out.push(PlayerEvent::Started(name.clone()));
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// The player.
#[derive(Debug, Clone, Default)]
pub struct AnimationPlayer {
    pub library: BTreeMap<String, Animation>,
    current: Option<String>,
    position: f32,
    /// Playback speed (negative plays backwards).
    pub speed: f32,
    queue: Vec<String>,
    /// Cross-fade in progress: (from animation, its frozen position, elapsed, duration).
    blend: Option<(String, f32, f32, f32)>,
    /// Nested animations started by Animation tracks: name → position.
    nested: BTreeMap<String, f32>,
    playing: bool,
}

impl AnimationPlayer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            speed: 1.0,
            ..Self::default()
        }
    }

    pub fn add(&mut self, name: &str, a: Animation) -> &mut Self {
        self.library.insert(name.to_owned(), a);
        self
    }

    /// Play `name` from the start, cross-fading from the current animation
    /// over `blend` seconds.
    pub fn play(&mut self, name: &str, blend: f32) -> Vec<PlayerEvent> {
        let mut ev = Vec::new();
        if !self.library.contains_key(name) {
            return ev;
        }
        if let Some(cur) = self.current.take() {
            if blend > 0.0 {
                self.blend = Some((cur, self.position, 0.0, blend));
            }
        }
        self.current = Some(name.to_owned());
        self.position = if self.speed < 0.0 {
            self.library[name].length
        } else {
            0.0
        };
        self.playing = true;
        ev.push(PlayerEvent::Started(name.to_owned()));
        // Keys at exactly 0 fire on start.
        self.library[name].events(-1e-6, 0.0, &mut ev);
        ev
    }

    /// Play `name` when the current animation finishes.
    pub fn queue(&mut self, name: &str) {
        self.queue.push(name.to_owned());
    }

    pub fn stop(&mut self) {
        self.playing = false;
    }

    #[must_use]
    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }

    #[must_use]
    pub fn position(&self) -> f32 {
        self.position
    }

    /// Jump to `t`; with `update`, fire the method/audio keys between the
    /// old and new positions (Godot's `seek(t, update = true)`).
    pub fn seek(&mut self, t: f32, update: bool) -> Vec<PlayerEvent> {
        let mut ev = Vec::new();
        if let Some(name) = &self.current {
            let a = &self.library[name];
            let t = t.clamp(0.0, a.length);
            if update {
                a.events(self.position, t, &mut ev);
            }
            self.position = t;
        }
        ev
    }

    /// Advance by `dt`; returns the events of this step.
    pub fn advance(&mut self, dt: f32) -> Vec<PlayerEvent> {
        let mut ev = Vec::new();
        if let Some((_, _, el, dur)) = &mut self.blend {
            *el += dt;
            if *el >= *dur {
                self.blend = None;
            }
        }
        // Nested animations run on their own clocks.
        let nested: Vec<(String, f32)> = self.nested.iter().map(|(k, v)| (k.clone(), *v)).collect();
        for (name, pos) in nested {
            if let Some(a) = self.library.get(&name) {
                let np = pos + dt * self.speed.abs();
                a.events(pos, np.min(a.length), &mut ev);
                if np >= a.length {
                    self.nested.remove(&name);
                    ev.push(PlayerEvent::Finished(name));
                } else {
                    self.nested.insert(name, np);
                }
            }
        }
        if !self.playing {
            return ev;
        }
        let Some(name) = self.current.clone() else {
            return ev;
        };
        let a = self.library[&name].clone();
        let mut from = self.position;
        let mut remaining = dt * self.speed;
        // Loop as many times as the step covers.
        let mut guard = 0;
        loop {
            guard += 1;
            let to = from + remaining;
            if (0.0..=a.length).contains(&to) || guard > 64 || a.length <= 0.0 {
                a.events(from, to.clamp(0.0, a.length), &mut ev);
                self.position = to.clamp(0.0, a.length);
                break;
            }
            let edge = if remaining > 0.0 { a.length } else { 0.0 };
            a.events(from, edge, &mut ev);
            if a.looping {
                ev.push(PlayerEvent::Looped(name.clone()));
                remaining -= edge - from;
                from = if remaining > 0.0 { 0.0 } else { a.length };
                // Keys sitting exactly on the restart edge fire again.
                a.events(
                    if remaining > 0.0 {
                        -1e-6
                    } else {
                        a.length + 1e-6
                    },
                    from,
                    &mut ev,
                );
            } else {
                self.position = edge;
                self.playing = false;
                ev.push(PlayerEvent::Finished(name.clone()));
                if !self.queue.is_empty() {
                    let next = self.queue.remove(0);
                    ev.extend(self.play(&next, 0.0));
                }
                break;
            }
        }
        // Animation-track starts become nested runs.
        for e in &ev {
            if let PlayerEvent::Started(n) = e {
                if Some(n.as_str()) != self.current.as_deref() && self.library.contains_key(n) {
                    self.nested.insert(n.clone(), 0.0);
                }
            }
        }
        ev
    }

    /// The current property values, cross-faded with the outgoing animation
    /// during a blend, and nested animations layered on top.
    #[must_use]
    pub fn values(&self) -> BTreeMap<String, f32> {
        let mut out = match &self.current {
            Some(n) => self.library[n].sample(self.position),
            None => BTreeMap::new(),
        };
        if let Some((from, pos, el, dur)) = &self.blend {
            let w = (el / dur).clamp(0.0, 1.0);
            for (k, v) in self.library[from].sample(*pos) {
                let e = out.entry(k).or_insert(v);
                *e = v + (*e - v) * w;
            }
        }
        for (n, p) in &self.nested {
            out.extend(self.library[n].sample(*p));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walk() -> Animation {
        Animation::new(2.0)
            .track(Track::Value {
                path: "x".into(),
                keys: vec![(0.0, 0.0), (2.0, 10.0)],
                mode: UpdateMode::Continuous,
            })
            .track(Track::Value {
                path: "frame".into(),
                keys: vec![(0.0, 0.0), (0.5, 1.0), (1.0, 2.0)],
                mode: UpdateMode::Discrete,
            })
            .track(Track::Method {
                target: "feet".into(),
                keys: vec![
                    (0.5, "step".into(), vec![1.0]),
                    (1.5, "step".into(), vec![2.0]),
                ],
            })
            .track(Track::Audio {
                keys: vec![(1.0, "thud".into(), 0.0)],
            })
    }

    #[test]
    fn value_tracks_interpolate_and_hold() {
        let a = walk();
        let s = a.sample(0.75);
        assert!((s["x"] - 3.75).abs() < 1e-5);
        assert!((s["frame"] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn bezier_tracks_pass_through_their_keys_and_follow_handles() {
        let keys = vec![
            BezierKey {
                time: 0.0,
                value: 0.0,
                in_handle: (0.0, 0.0),
                out_handle: (0.5, 2.0),
            },
            BezierKey {
                time: 1.0,
                value: 1.0,
                in_handle: (-0.5, 0.0),
                out_handle: (0.0, 0.0),
            },
        ];
        assert!((sample_bezier(&keys, 0.0).unwrap()).abs() < 1e-5);
        assert!((sample_bezier(&keys, 1.0).unwrap() - 1.0).abs() < 1e-5);
        // A steep out handle overshoots early.
        assert!(sample_bezier(&keys, 0.3).unwrap() > 0.5);
    }

    #[test]
    fn method_and_audio_keys_fire_once_per_crossing() {
        let mut p = AnimationPlayer::new();
        p.add("walk", walk());
        p.play("walk", 0.0);
        let mut calls = 0;
        let mut audio = 0;
        for _ in 0..40 {
            for e in p.advance(0.05) {
                match e {
                    PlayerEvent::Call { .. } => calls += 1,
                    PlayerEvent::Audio { offset, .. } => {
                        audio += 1;
                        assert!(offset < 0.05 + 1e-5);
                    }
                    _ => {}
                }
            }
        }
        assert_eq!(calls, 2);
        assert_eq!(audio, 1);
    }

    #[test]
    fn seeking_into_a_sound_resumes_it_mid_clip() {
        let mut p = AnimationPlayer::new();
        p.add("walk", walk());
        p.play("walk", 0.0);
        let ev = p.seek(1.4, true);
        let off = ev.iter().find_map(|e| {
            if let PlayerEvent::Audio { offset, .. } = e {
                Some(*offset)
            } else {
                None
            }
        });
        assert!((off.expect("audio") - 0.4).abs() < 1e-5);
        assert!(ev
            .iter()
            .any(|e| matches!(e, PlayerEvent::Call { args, .. } if args == &vec![1.0])));
    }

    #[test]
    fn looping_wraps_and_queued_animations_follow() {
        let mut p = AnimationPlayer::new();
        p.add(
            "spin",
            Animation::new(1.0).looping().track(Track::Method {
                target: "t".into(),
                keys: vec![(0.25, "tick".into(), vec![])],
            }),
        );
        p.add("once", Animation::new(0.5));
        p.add("after", Animation::new(0.5));
        p.play("spin", 0.0);
        let ev = p.advance(3.1);
        let ticks = ev
            .iter()
            .filter(|e| matches!(e, PlayerEvent::Call { .. }))
            .count();
        let loops = ev
            .iter()
            .filter(|e| matches!(e, PlayerEvent::Looped(_)))
            .count();
        assert_eq!(ticks, 3);
        assert_eq!(loops, 3);
        p.play("once", 0.0);
        p.queue("after");
        let ev = p.advance(0.6);
        assert!(ev.contains(&PlayerEvent::Finished("once".into())));
        assert_eq!(p.current(), Some("after"));
    }

    #[test]
    fn backwards_playback_fires_keys_in_reverse() {
        let mut p = AnimationPlayer::new();
        p.add("walk", walk());
        p.speed = -1.0;
        p.play("walk", 0.0);
        assert!((p.position() - 2.0).abs() < 1e-6);
        let ev = p.advance(0.6);
        assert!(ev
            .iter()
            .any(|e| matches!(e, PlayerEvent::Call { args, .. } if args == &vec![2.0])));
    }

    #[test]
    fn cross_fades_blend_values_and_nested_animations_run() {
        let mut p = AnimationPlayer::new();
        p.add(
            "a",
            Animation::new(1.0).track(Track::Value {
                path: "v".into(),
                keys: vec![(0.0, 0.0)],
                mode: UpdateMode::Continuous,
            }),
        );
        p.add(
            "b",
            Animation::new(1.0)
                .track(Track::Value {
                    path: "v".into(),
                    keys: vec![(0.0, 10.0)],
                    mode: UpdateMode::Continuous,
                })
                .track(Track::Animation {
                    keys: vec![(0.1, "c".into())],
                }),
        );
        p.add(
            "c",
            Animation::new(0.5).track(Track::Value {
                path: "w".into(),
                keys: vec![(0.0, 1.0), (0.5, 2.0)],
                mode: UpdateMode::Continuous,
            }),
        );
        p.play("a", 0.0);
        p.advance(0.1);
        p.play("b", 0.5);
        p.advance(0.25);
        let v = p.values()["v"];
        assert!((v - 5.0).abs() < 1e-4, "{v}");
        p.advance(0.1);
        assert!(p.values().contains_key("w"));
    }
}
