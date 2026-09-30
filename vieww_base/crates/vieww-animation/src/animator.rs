//! The animator graph — Unity's Mecanim, Rive's state machine, Godot's
//! `AnimationTree`, as one data-driven model.
//!
//! # What [`StateMachine`](crate::StateMachine) did not cover
//!
//! The existing state machine answers "states, events, cross-fades". The
//! document's layer tables for Rive (§2.6) and Unity (§2.7) name more, and
//! each is load-bearing in those tools:
//!
//! | layer | here |
//! |---|---|
//! | typed **inputs** — float, int, bool, trigger (Rive inputs, `animator.SetFloat`) | [`Param`], [`Animator::set_float`] … [`Animator::fire`] |
//! | **conditions** on inputs, all of which must hold | [`Condition`] |
//! | the **Any State** and **exit time** | [`Transition::from_any`], [`Transition::exit_time`] |
//! | **blend-tree states** driven by a parameter | [`Motion::Blend1D`] |
//! | **interruption** of a running cross-fade | handled: the in-flight blend is frozen and faded from |
//! | **layers** with weight, **Override / Additive** and an **avatar mask** | [`Layer`], [`LayerMode`], [`Layer::mask`] |
//! | **state events** (enter / exit) back to the app | [`AnimatorEvent`] from [`Animator::advance`] |
//! | **state machines as data** — Rive's `.riv` lesson | [`Animator::to_json`] / [`Animator::from_json`] |
//!
//! # Channels, not a pose type
//!
//! Everything an animator drives is a named `f32` [`Channels`] map —
//! `"arm.rotation"`, `"opacity"`, `"knob.x"`. Unity masks bones and Rive
//! drives arbitrary properties; a flat channel map serves both, a mask is a
//! set of channel-name prefixes, and a skeletal [`Pose`](crate::Pose) goes in
//! and out through [`retarget::pose_to_channels`](crate::retarget::pose_to_channels)
//! and its inverse. Additive layers are defined per channel exactly as Unity
//! defines them: the layer's value *minus its reference* (the current clip's
//! first frame), scaled by weight, added on top.

use std::collections::BTreeMap;
use std::time::Duration;

use vieww_foundation::json::Json;

use crate::curve::Curve;
use crate::keyframe::{Keyframe, Keyframes};

/// A snapshot of every driven channel.
pub type Channels = BTreeMap<String, f32>;

/// A clip: keyframed tracks, one per channel.
#[derive(Debug, Clone, Default)]
pub struct Clip {
    tracks: BTreeMap<String, Keyframes<f32>>,
    looping: bool,
}

impl Clip {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add (or replace) the track for `channel`.
    #[must_use]
    pub fn track(mut self, channel: &str, frames: Keyframes<f32>) -> Self {
        self.tracks.insert(channel.to_owned(), frames);
        self
    }

    /// Loop the clip — a walk cycle rather than a one-shot.
    #[must_use]
    pub const fn looping(mut self) -> Self {
        self.looping = true;
        self
    }

    /// Longest track, in seconds.
    #[must_use]
    pub fn length(&self) -> f32 {
        self.tracks
            .values()
            .map(|k| k.end().as_secs_f32())
            .fold(0.0, f32::max)
    }

    /// Every channel's value `t` seconds in.
    #[must_use]
    pub fn sample(&self, t: f32) -> Channels {
        let len = self.length();
        let t = if self.looping && len > 0.0 { t.rem_euclid(len) } else { t.min(len) };
        self.tracks
            .iter()
            .map(|(k, v)| (k.clone(), v.at(Duration::from_secs_f32(t.max(0.0)))))
            .collect()
    }
}

/// What a state plays.
#[derive(Debug, Clone)]
pub enum Motion {
    Clip(Clip),
    /// Children at thresholds of a float parameter, blended linearly between
    /// the two that bracket it — Unity's 1D blend tree.
    Blend1D { parameter: String, children: Vec<(f32, Clip)> },
}

impl Motion {
    fn length(&self) -> f32 {
        match self {
            Self::Clip(c) => c.length(),
            Self::Blend1D { children, .. } => children.iter().map(|(_, c)| c.length()).fold(0.0, f32::max),
        }
    }

    fn sample(&self, t: f32, params: &BTreeMap<String, Param>) -> Channels {
        match self {
            Self::Clip(c) => c.sample(t),
            Self::Blend1D { parameter, children } => {
                let x = params.get(parameter).map_or(0.0, Param::as_f32);
                let mut sorted: Vec<&(f32, Clip)> = children.iter().collect();
                sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
                match sorted.len() {
                    0 => Channels::new(),
                    1 => sorted[0].1.sample(t),
                    _ => {
                        let i = sorted.partition_point(|c| c.0 <= x).clamp(1, sorted.len() - 1);
                        let (lo, hi) = (sorted[i - 1], sorted[i]);
                        let w = if hi.0 > lo.0 { ((x - lo.0) / (hi.0 - lo.0)).clamp(0.0, 1.0) } else { 0.0 };
                        mix(&lo.1.sample(t), &hi.1.sample(t), w)
                    }
                }
            }
        }
    }
}

/// Linear mix of two channel maps; a channel present in only one holds.
#[must_use]
pub fn mix(a: &Channels, b: &Channels, t: f32) -> Channels {
    let mut out = a.clone();
    for (k, vb) in b {
        let va = a.get(k).copied().unwrap_or(*vb);
        out.insert(k.clone(), va + (vb - va) * t);
    }
    out
}

/// A typed input.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Param {
    Float(f32),
    Int(i32),
    Bool(bool),
    /// Set by [`Animator::fire`], consumed by the first transition that
    /// reads it.
    Trigger(bool),
}

impl Param {
    #[allow(clippy::cast_precision_loss)]
    fn as_f32(&self) -> f32 {
        match *self {
            Self::Float(v) => v,
            Self::Int(v) => v as f32,
            Self::Bool(b) | Self::Trigger(b) => f32::from(u8::from(b)),
        }
    }
}

/// One test a transition makes.
#[derive(Debug, Clone, PartialEq)]
pub enum Condition {
    Greater(String, f32),
    Less(String, f32),
    Equals(String, i32),
    NotEquals(String, i32),
    If(String),
    IfNot(String),
    Trigger(String),
}

impl Condition {
    fn holds(&self, params: &BTreeMap<String, Param>) -> bool {
        let get = |n: &str| params.get(n).copied();
        match self {
            Self::Greater(n, v) => get(n).is_some_and(|p| p.as_f32() > *v),
            Self::Less(n, v) => get(n).is_some_and(|p| p.as_f32() < *v),
            Self::Equals(n, v) => matches!(get(n), Some(Param::Int(i)) if i == *v),
            Self::NotEquals(n, v) => matches!(get(n), Some(Param::Int(i)) if i != *v),
            Self::If(n) => matches!(get(n), Some(Param::Bool(true))),
            Self::IfNot(n) => matches!(get(n), Some(Param::Bool(false))),
            Self::Trigger(n) => matches!(get(n), Some(Param::Trigger(true))),
        }
    }
}

/// An edge between states.
#[derive(Debug, Clone)]
pub struct Transition {
    /// `None` is the Any State.
    from: Option<String>,
    to: String,
    conditions: Vec<Condition>,
    /// Normalised time (0..=1 of the source clip) before which the
    /// transition may not fire; `None` means any time.
    exit_time: Option<f32>,
    duration: f32,
    /// Any-State transitions may re-enter the state they target only if set.
    to_self: bool,
}

impl Transition {
    /// From `from` to `to`, cross-fading over `duration` seconds.
    #[must_use]
    pub fn new(from: &str, to: &str, duration: f32) -> Self {
        Self {
            from: Some(from.to_owned()),
            to: to.to_owned(),
            conditions: Vec::new(),
            exit_time: None,
            duration,
            to_self: false,
        }
    }

    /// From the Any State.
    #[must_use]
    pub fn from_any(to: &str, duration: f32) -> Self {
        Self {
            from: None,
            ..Self::new("", to, duration)
        }
    }

    #[must_use]
    pub fn when(mut self, condition: Condition) -> Self {
        self.conditions.push(condition);
        self
    }

    /// Only after the source state has played this fraction of its clip.
    #[must_use]
    pub const fn exit_time(mut self, normalized: f32) -> Self {
        self.exit_time = Some(normalized);
        self
    }

    /// Allow an Any-State transition into the state already playing.
    #[must_use]
    pub const fn can_transition_to_self(mut self) -> Self {
        self.to_self = true;
        self
    }
}

/// Something the app hears about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnimatorEvent {
    Enter { layer: usize, state: String },
    Exit { layer: usize, state: String },
}

/// How a layer combines with the layers below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayerMode {
    #[default]
    Override,
    Additive,
}

/// One state machine plus how its output lands.
#[derive(Debug, Clone)]
pub struct Layer {
    name: String,
    states: BTreeMap<String, Motion>,
    default_state: String,
    transitions: Vec<Transition>,
    weight: f32,
    mode: LayerMode,
    mask: Option<Vec<String>>,
    // runtime
    current: String,
    time: f32,
    fade: Option<Fade>,
}

#[derive(Debug, Clone)]
struct Fade {
    /// What we are fading from — frozen if the fade interrupted another.
    from: FadeSource,
    elapsed: f32,
    duration: f32,
}

#[derive(Debug, Clone)]
enum FadeSource {
    State { name: String, time: f32 },
    Frozen(Channels),
}

impl Layer {
    /// A layer whose machine starts in `state`.
    #[must_use]
    pub fn new(name: &str, state: &str, motion: Motion) -> Self {
        let mut states = BTreeMap::new();
        states.insert(state.to_owned(), motion);
        Self {
            name: name.to_owned(),
            states,
            default_state: state.to_owned(),
            transitions: Vec::new(),
            weight: 1.0,
            mode: LayerMode::Override,
            mask: None,
            current: state.to_owned(),
            time: 0.0,
            fade: None,
        }
    }

    #[must_use]
    pub fn state(mut self, name: &str, motion: Motion) -> Self {
        self.states.insert(name.to_owned(), motion);
        self
    }

    #[must_use]
    pub fn transition(mut self, t: Transition) -> Self {
        self.transitions.push(t);
        self
    }

    #[must_use]
    pub const fn weight(mut self, w: f32) -> Self {
        self.weight = w;
        self
    }

    #[must_use]
    pub const fn mode(mut self, mode: LayerMode) -> Self {
        self.mode = mode;
        self
    }

    /// Restrict this layer to channels starting with any of `prefixes` —
    /// the avatar mask ("upper body only").
    #[must_use]
    pub fn mask(mut self, prefixes: &[&str]) -> Self {
        self.mask = Some(prefixes.iter().map(|p| (*p).to_owned()).collect());
        self
    }

    /// The state playing (the target, during a fade).
    #[must_use]
    pub fn current(&self) -> &str {
        &self.current
    }

    /// Whether a cross-fade is in progress.
    #[must_use]
    pub const fn is_fading(&self) -> bool {
        self.fade.is_some()
    }

    fn allowed(&self, channel: &str) -> bool {
        self.mask
            .as_ref()
            .is_none_or(|m| m.iter().any(|p| channel.starts_with(p.as_str())))
    }

    fn sample_state(&self, name: &str, t: f32, params: &BTreeMap<String, Param>) -> Channels {
        self.states.get(name).map_or_else(Channels::new, |m| m.sample(t, params))
    }

    fn output(&self, params: &BTreeMap<String, Param>) -> Channels {
        let target = self.sample_state(&self.current, self.time, params);
        match &self.fade {
            None => target,
            Some(f) => {
                let from = match &f.from {
                    FadeSource::State { name, time } => self.sample_state(name, *time, params),
                    FadeSource::Frozen(c) => c.clone(),
                };
                let w = if f.duration > 0.0 { (f.elapsed / f.duration).clamp(0.0, 1.0) } else { 1.0 };
                mix(&from, &target, w)
            }
        }
    }

    fn normalized(&self) -> f32 {
        let len = self.states.get(&self.current).map_or(0.0, Motion::length);
        if len > 0.0 {
            self.time / len
        } else {
            1.0
        }
    }

    fn advance(&mut self, dt: f32, index: usize, params: &mut BTreeMap<String, Param>, events: &mut Vec<AnimatorEvent>) {
        self.time += dt;
        if let Some(f) = &mut self.fade {
            f.elapsed += dt;
            if let FadeSource::State { time, .. } = &mut f.from {
                *time += dt;
            }
            if f.elapsed >= f.duration {
                self.fade = None;
            }
        }
        // Any-State transitions first, then the current state's — Unity's
        // order.
        let normalized = self.normalized();
        let chosen = self
            .transitions
            .iter()
            .filter(|t| t.from.is_none())
            .chain(self.transitions.iter().filter(|t| t.from.as_deref() == Some(self.current.as_str())))
            .find(|t| {
                (t.from.is_some() || t.to_self || t.to != self.current)
                    && t.exit_time.is_none_or(|e| normalized >= e)
                    && t.conditions.iter().all(|c| c.holds(params))
            })
            .cloned();
        if let Some(t) = chosen {
            for c in &t.conditions {
                if let Condition::Trigger(n) = c {
                    params.insert(n.clone(), Param::Trigger(false));
                }
            }
            let from = if self.fade.is_some() {
                FadeSource::Frozen(self.output(params))
            } else {
                FadeSource::State {
                    name: self.current.clone(),
                    time: self.time,
                }
            };
            events.push(AnimatorEvent::Exit {
                layer: index,
                state: self.current.clone(),
            });
            events.push(AnimatorEvent::Enter {
                layer: index,
                state: t.to.clone(),
            });
            self.current = t.to;
            self.time = 0.0;
            self.fade = (t.duration > 0.0).then_some(Fade {
                from,
                elapsed: 0.0,
                duration: t.duration,
            });
        }
    }
}

/// Layers, parameters and the clock that drives them.
#[derive(Debug, Clone, Default)]
pub struct Animator {
    layers: Vec<Layer>,
    params: BTreeMap<String, Param>,
}

impl Animator {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a layer; later layers sit on top.
    #[must_use]
    pub fn layer(mut self, layer: Layer) -> Self {
        self.layers.push(layer);
        self
    }

    /// Declare a parameter with its default.
    #[must_use]
    pub fn param(mut self, name: &str, value: Param) -> Self {
        self.params.insert(name.to_owned(), value);
        self
    }

    pub fn set_float(&mut self, name: &str, v: f32) {
        self.params.insert(name.to_owned(), Param::Float(v));
    }
    pub fn set_int(&mut self, name: &str, v: i32) {
        self.params.insert(name.to_owned(), Param::Int(v));
    }
    pub fn set_bool(&mut self, name: &str, v: bool) {
        self.params.insert(name.to_owned(), Param::Bool(v));
    }
    /// Fire a trigger: it stays set until a transition consumes it.
    pub fn fire(&mut self, name: &str) {
        self.params.insert(name.to_owned(), Param::Trigger(true));
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<Param> {
        self.params.get(name).copied()
    }

    #[must_use]
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    /// Set a layer's weight at runtime (Unity's `SetLayerWeight`).
    pub fn set_layer_weight(&mut self, index: usize, weight: f32) {
        if let Some(l) = self.layers.get_mut(index) {
            l.weight = weight.clamp(0.0, 1.0);
        }
    }

    /// Move time forward and evaluate transitions; returns enter/exit
    /// events in order.
    pub fn advance(&mut self, dt: f32) -> Vec<AnimatorEvent> {
        let mut events = Vec::new();
        for (i, layer) in self.layers.iter_mut().enumerate() {
            layer.advance(dt, i, &mut self.params, &mut events);
        }
        events
    }

    /// The blended output of every layer.
    #[must_use]
    pub fn output(&self) -> Channels {
        let mut out = Channels::new();
        for (i, layer) in self.layers.iter().enumerate() {
            let values = layer.output(&self.params);
            if i == 0 {
                for (k, v) in values {
                    if layer.allowed(&k) {
                        out.insert(k, v);
                    }
                }
                continue;
            }
            let reference = match layer.mode {
                LayerMode::Additive => layer.sample_state(&layer.current, 0.0, &self.params),
                LayerMode::Override => Channels::new(),
            };
            for (k, v) in values {
                if !layer.allowed(&k) {
                    continue;
                }
                let base = out.get(&k).copied().unwrap_or(0.0);
                let next = match layer.mode {
                    LayerMode::Override => base + (v - base) * layer.weight,
                    LayerMode::Additive => base + (v - reference.get(&k).copied().unwrap_or(0.0)) * layer.weight,
                };
                out.insert(k, next);
            }
        }
        out
    }

    // ------------------------------------------------------------ as data

    /// The whole graph — layers, states, clips, transitions, parameters —
    /// as JSON. Runtime state (playheads, fades) is not saved.
    #[must_use]
    pub fn to_json(&self) -> Json {
        let params = self
            .params
            .iter()
            .map(|(k, p)| {
                let v = match *p {
                    Param::Float(f) => Json::object([("float", Json::from(f))]),
                    Param::Int(i) => Json::object([("int", Json::from(f64::from(i)))]),
                    Param::Bool(b) => Json::object([("bool", Json::from(b))]),
                    Param::Trigger(_) => Json::object([("trigger", Json::Bool(false))]),
                };
                (k.clone(), v)
            })
            .collect::<Vec<_>>();
        let layers = self.layers.iter().map(layer_to_json).collect::<Vec<_>>();
        Json::object([("params", Json::Object(params)), ("layers", Json::Array(layers))])
    }

    /// Rebuild from [`to_json`](Self::to_json)'s format.
    ///
    /// # Errors
    ///
    /// A message naming the first malformed part.
    pub fn from_json(doc: &Json) -> Result<Self, String> {
        let mut animator = Self::new();
        if let Some(params) = doc.get("params").and_then(Json::as_object) {
            for (name, v) in params {
                let p = if let Some(f) = v.get("float").and_then(Json::as_f32) {
                    Param::Float(f)
                } else if let Some(i) = v.get("int").and_then(Json::as_f64) {
                    #[allow(clippy::cast_possible_truncation)]
                    Param::Int(i as i32)
                } else if let Some(b) = v.get("bool").and_then(Json::as_bool) {
                    Param::Bool(b)
                } else if v.get("trigger").is_some() {
                    Param::Trigger(false)
                } else {
                    return Err(format!("param {name}: unknown type"));
                };
                animator.params.insert(name.clone(), p);
            }
        }
        for l in doc.get("layers").and_then(Json::as_array).ok_or("missing layers")? {
            animator.layers.push(layer_from_json(l)?);
        }
        Ok(animator)
    }
}

const CURVES: &[(&str, Curve)] = &[
    ("linear", Curve::Linear),
    ("ease", Curve::EASE),
    ("ease-in", Curve::EASE_IN),
    ("ease-out", Curve::EASE_OUT),
    ("ease-in-out", Curve::EASE_IN_OUT),
    ("sine-in-out", Curve::SINE_IN_OUT),
    ("back-out", Curve::BACK_OUT),
    ("elastic-out", Curve::ELASTIC_OUT),
    ("bounce-out", Curve::BOUNCE_OUT),
    ("power2-out", Curve::POWER2_OUT),
    ("power2-in-out", Curve::POWER2_IN_OUT),
];

/// The name a curve is saved under, if it is one of the named ones.
#[must_use]
pub fn curve_name(c: Curve) -> &'static str {
    CURVES.iter().find(|(_, k)| *k == c).map_or("linear", |(n, _)| n)
}

/// A named curve; `linear` for unknown names.
#[must_use]
pub fn curve_named(name: &str) -> Curve {
    CURVES.iter().find(|(n, _)| *n == name).map_or(Curve::Linear, |(_, c)| *c)
}

fn clip_to_json(c: &Clip) -> Json {
    let tracks = c
        .tracks
        .iter()
        .map(|(k, frames)| {
            let list = frames
                .frames()
                .iter()
                .map(|f| {
                    Json::Array(vec![
                        Json::from(f.time),
                        Json::from(f.value),
                        Json::from(f.curve.map_or("linear", curve_name)),
                        Json::Bool(f.curve.is_none()),
                    ])
                })
                .collect();
            (k.clone(), Json::Array(list))
        })
        .collect::<Vec<_>>();
    Json::object([("loop", Json::Bool(c.looping)), ("tracks", Json::Object(tracks))])
}

fn clip_from_json(j: &Json) -> Result<Clip, String> {
    let mut clip = Clip::new();
    clip.looping = j.get("loop").and_then(Json::as_bool).unwrap_or(false);
    for (name, list) in j.get("tracks").and_then(Json::as_object).ok_or("clip without tracks")? {
        let frames = list.as_array().ok_or("track is not an array")?;
        let mut track: Option<Keyframes<f32>> = None;
        for f in frames {
            let time = f.index(0).and_then(Json::as_f32).ok_or("keyframe time")?;
            let value = f.index(1).and_then(Json::as_f32).ok_or("keyframe value")?;
            let curve = curve_named(f.index(2).and_then(Json::as_str).unwrap_or("linear"));
            let hold = f.index(3).and_then(Json::as_bool).unwrap_or(false);
            let key = if hold { Keyframe::hold(time, value) } else { Keyframe::to(time, value).curve(curve) };
            track = Some(match track {
                None if time == 0.0 => Keyframes::new(value),
                None => Keyframes::new(value).with(key),
                Some(t) => t.with(key),
            });
        }
        clip.tracks.insert(name.clone(), track.ok_or("empty track")?);
    }
    Ok(clip)
}

fn motion_to_json(m: &Motion) -> Json {
    match m {
        Motion::Clip(c) => Json::object([("clip", clip_to_json(c))]),
        Motion::Blend1D { parameter, children } => Json::object([
            ("blend1d", Json::from(parameter.as_str())),
            (
                "children",
                Json::Array(
                    children
                        .iter()
                        .map(|(t, c)| Json::object([("threshold", Json::from(*t)), ("clip", clip_to_json(c))]))
                        .collect(),
                ),
            ),
        ]),
    }
}

fn motion_from_json(j: &Json) -> Result<Motion, String> {
    if let Some(c) = j.get("clip") {
        return Ok(Motion::Clip(clip_from_json(c)?));
    }
    let parameter = j.get("blend1d").and_then(Json::as_str).ok_or("unknown motion")?.to_owned();
    let mut children = Vec::new();
    for c in j.get("children").and_then(Json::as_array).ok_or("blend without children")? {
        children.push((
            c.get("threshold").and_then(Json::as_f32).ok_or("threshold")?,
            clip_from_json(c.get("clip").ok_or("child clip")?)?,
        ));
    }
    Ok(Motion::Blend1D { parameter, children })
}

fn condition_to_json(c: &Condition) -> Json {
    let (op, name, v) = match c {
        Condition::Greater(n, v) => ("greater", n, Json::from(*v)),
        Condition::Less(n, v) => ("less", n, Json::from(*v)),
        Condition::Equals(n, v) => ("equals", n, Json::from(f64::from(*v))),
        Condition::NotEquals(n, v) => ("not-equals", n, Json::from(f64::from(*v))),
        Condition::If(n) => ("if", n, Json::Null),
        Condition::IfNot(n) => ("if-not", n, Json::Null),
        Condition::Trigger(n) => ("trigger", n, Json::Null),
    };
    Json::Array(vec![Json::from(op), Json::from(name.as_str()), v])
}

#[allow(clippy::cast_possible_truncation)]
fn condition_from_json(j: &Json) -> Result<Condition, String> {
    let op = j.index(0).and_then(Json::as_str).ok_or("condition op")?;
    let n = j.index(1).and_then(Json::as_str).ok_or("condition param")?.to_owned();
    let f = || j.index(2).and_then(Json::as_f64).ok_or_else(|| format!("{op} needs a value"));
    Ok(match op {
        "greater" => Condition::Greater(n, f()? as f32),
        "less" => Condition::Less(n, f()? as f32),
        "equals" => Condition::Equals(n, f()? as i32),
        "not-equals" => Condition::NotEquals(n, f()? as i32),
        "if" => Condition::If(n),
        "if-not" => Condition::IfNot(n),
        "trigger" => Condition::Trigger(n),
        other => return Err(format!("unknown condition {other}")),
    })
}

fn layer_to_json(l: &Layer) -> Json {
    let states = l
        .states
        .iter()
        .map(|(k, m)| (k.clone(), motion_to_json(m)))
        .collect::<Vec<_>>();
    let transitions = l
        .transitions
        .iter()
        .map(|t| {
            let mut fields = vec![
                ("from", t.from.as_deref().map_or(Json::Null, Json::from)),
                ("to", Json::from(t.to.as_str())),
                ("duration", Json::from(t.duration)),
                ("conditions", Json::Array(t.conditions.iter().map(condition_to_json).collect())),
            ];
            if let Some(e) = t.exit_time {
                fields.push(("exit_time", Json::from(e)));
            }
            if t.to_self {
                fields.push(("to_self", Json::Bool(true)));
            }
            Json::object(fields)
        })
        .collect();
    let mut fields = vec![
        ("name", Json::from(l.name.as_str())),
        ("default", Json::from(l.default_state.as_str())),
        ("weight", Json::from(l.weight)),
        (
            "mode",
            Json::from(match l.mode {
                LayerMode::Override => "override",
                LayerMode::Additive => "additive",
            }),
        ),
        ("states", Json::Object(states)),
        ("transitions", Json::Array(transitions)),
    ];
    if let Some(m) = &l.mask {
        fields.push(("mask", Json::Array(m.iter().map(|s| Json::from(s.as_str())).collect())));
    }
    Json::object(fields)
}

fn layer_from_json(j: &Json) -> Result<Layer, String> {
    let name = j.get("name").and_then(Json::as_str).unwrap_or("layer");
    let default = j.get("default").and_then(Json::as_str).ok_or("layer default")?;
    let states = j.get("states").and_then(Json::as_object).ok_or("layer states")?;
    let (_, first) = states
        .iter()
        .find(|(k, _)| k == default)
        .ok_or_else(|| format!("default state {default} missing"))?;
    let mut layer = Layer::new(name, default, motion_from_json(first)?);
    for (k, m) in states {
        layer.states.insert(k.clone(), motion_from_json(m)?);
    }
    layer.weight = j.get("weight").and_then(Json::as_f32).unwrap_or(1.0);
    layer.mode = match j.get("mode").and_then(Json::as_str) {
        Some("additive") => LayerMode::Additive,
        _ => LayerMode::Override,
    };
    layer.mask = j
        .get("mask")
        .and_then(Json::as_array)
        .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_owned)).collect());
    for t in j.get("transitions").and_then(Json::as_array).unwrap_or(&[]) {
        let to = t.get("to").and_then(Json::as_str).ok_or("transition to")?;
        let duration = t.get("duration").and_then(Json::as_f32).unwrap_or(0.0);
        let mut tr = match t.get("from").and_then(Json::as_str) {
            Some(from) => Transition::new(from, to, duration),
            None => Transition::from_any(to, duration),
        };
        for c in t.get("conditions").and_then(Json::as_array).unwrap_or(&[]) {
            tr.conditions.push(condition_from_json(c)?);
        }
        tr.exit_time = t.get("exit_time").and_then(Json::as_f32);
        tr.to_self = t.get("to_self").and_then(Json::as_bool).unwrap_or(false);
        layer.transitions.push(tr);
    }
    Ok(layer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constant(channel: &str, v: f32) -> Clip {
        Clip::new().track(channel, Keyframes::new(v).with(Keyframe::hold(1.0, v)))
    }

    fn ramp(channel: &str, a: f32, b: f32, len: f32) -> Clip {
        Clip::new().track(channel, Keyframes::new(a).with(Keyframe::to(len, b)))
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    fn locomotion() -> Animator {
        Animator::new()
            .param("speed", Param::Float(0.0))
            .param("jump", Param::Trigger(false))
            .param("grounded", Param::Bool(true))
            .layer(
                Layer::new(
                    "base",
                    "idle",
                    Motion::Clip(constant("body.y", 0.0).track("legs.swing", Keyframes::new(0.0))),
                )
                .state(
                    "move",
                    Motion::Blend1D {
                        parameter: "speed".into(),
                        children: vec![
                            (0.0, constant("legs.swing", 0.0)),
                            (1.0, constant("legs.swing", 10.0)),
                            (2.0, constant("legs.swing", 30.0)),
                        ],
                    },
                )
                .state("jump", Motion::Clip(ramp("body.y", 0.0, 50.0, 0.5)))
                .transition(Transition::new("idle", "move", 0.0).when(Condition::Greater("speed".into(), 0.1)))
                .transition(Transition::new("move", "idle", 0.0).when(Condition::Less("speed".into(), 0.1)))
                .transition(Transition::from_any("jump", 0.0).when(Condition::Trigger("jump".into())))
                .transition(Transition::new("jump", "idle", 0.0).exit_time(1.0)),
            )
    }

    #[test]
    fn float_conditions_and_blend_trees() {
        let mut a = locomotion();
        a.advance(0.016);
        assert_eq!(a.layers()[0].current(), "idle");
        a.set_float("speed", 1.5);
        let ev = a.advance(0.016);
        assert_eq!(
            ev,
            vec![
                AnimatorEvent::Exit { layer: 0, state: "idle".into() },
                AnimatorEvent::Enter { layer: 0, state: "move".into() }
            ]
        );
        assert!(close(a.output()["legs.swing"], 20.0), "halfway between 10 and 30");
    }

    #[test]
    fn triggers_are_consumed_and_exit_time_returns() {
        let mut a = locomotion();
        a.fire("jump");
        a.advance(0.01);
        assert_eq!(a.layers()[0].current(), "jump");
        assert_eq!(a.get("jump"), Some(Param::Trigger(false)), "consumed");
        a.advance(0.25);
        assert!(close(a.output()["body.y"], 25.0));
        a.advance(0.3);
        assert_eq!(a.layers()[0].current(), "idle", "exit time reached");
    }

    #[test]
    fn any_state_does_not_retrigger_itself_unless_allowed() {
        let mut a = Animator::new().param("go", Param::Bool(true)).layer(
            Layer::new("l", "a", Motion::Clip(constant("x", 0.0)))
                .state("b", Motion::Clip(ramp("x", 0.0, 1.0, 1.0)))
                .transition(Transition::from_any("b", 0.0).when(Condition::If("go".into()))),
        );
        a.advance(0.1);
        assert_eq!(a.layers()[0].current(), "b");
        a.advance(0.5);
        assert!(close(a.output()["x"], 0.5), "not restarted every frame");
    }

    #[test]
    fn cross_fades_and_interruptions_blend_without_pops() {
        let mut a = Animator::new().param("s", Param::Int(0)).layer(
            Layer::new("l", "a", Motion::Clip(constant("x", 0.0)))
                .state("b", Motion::Clip(constant("x", 100.0)))
                .state("c", Motion::Clip(constant("x", -100.0)))
                .transition(Transition::from_any("b", 1.0).when(Condition::Equals("s".into(), 1)))
                .transition(Transition::from_any("c", 1.0).when(Condition::Equals("s".into(), 2))),
        );
        a.set_int("s", 1);
        a.advance(0.0001);
        a.advance(0.5);
        let mid = a.output()["x"];
        assert!(close(mid, 50.0), "{mid}");
        a.set_int("s", 2);
        a.advance(0.0001);
        let after = a.output()["x"];
        assert!((after - mid).abs() < 1.0, "interruption starts from the frozen blend: {mid} → {after}");
        a.advance(1.0);
        assert!(close(a.output()["x"], -100.0));
    }

    #[test]
    fn layers_override_additively_and_through_masks() {
        let base = Layer::new(
            "base",
            "walk",
            Motion::Clip(constant("arm.r", 10.0).track("leg.r", Keyframes::new(5.0))),
        );
        let upper = Layer::new("upper", "wave", Motion::Clip(constant("arm.r", 90.0).track("leg.r", Keyframes::new(99.0))))
            .mask(&["arm."])
            .weight(0.5);
        let breathe = Layer::new("add", "b", Motion::Clip(ramp("arm.r", 0.0, 4.0, 1.0)))
            .mode(LayerMode::Additive);
        let mut a = Animator::new().layer(base).layer(upper).layer(breathe);
        a.advance(0.5);
        let out = a.output();
        assert!(close(out["leg.r"], 5.0), "masked out of the upper layer");
        // override 50%: 10 → 50, then additive +2 (value 2 minus reference 0)
        assert!(close(out["arm.r"], 52.0), "{}", out["arm.r"]);
    }

    #[test]
    fn the_graph_round_trips_as_data() {
        let a = locomotion();
        let text = a.to_json().pretty();
        let back = Animator::from_json(&Json::parse(&text).unwrap()).unwrap();
        assert_eq!(back.to_json(), a.to_json());
        let mut b = back;
        b.set_float("speed", 2.0);
        b.advance(0.01);
        assert!(close(b.output()["legs.swing"], 30.0));
    }
}
