//! The Sequencer — Unreal's `LevelSequence` with a camera-cuts track (§2.17 L7),
//! and the shot list every film editor and Blender's VSE camera markers keep.
//!
//! A [`Sequencer`] is an ordered list of [`Shot`]s. Each shot owns a camera
//! move: a start and end [`CameraState`] (eye, target, field of view, roll,
//! focus distance and aperture for depth of field) reached over a
//! [`Curve`], an optional Catmull–Rom **dolly path** for the eye, and
//! seeded **handheld shake**. Between shots sits a [`Transition`]: a hard
//! **cut** (the camera-cuts track) or a **blend** that interpolates the two
//! cameras over a window, as Unreal's blend-in does.
//!
//! [`Sequencer::evaluate`] is a pure function of the sequence time and
//! answers which shot is live, how far through it, the camera, and — inside a
//! blend — the outgoing shot and weight. Named **event markers** (Unreal's
//! event track; Motion Canvas' `waitUntil`) are queried with
//! [`Sequencer::events_between`], so a frame-stepped renderer fires each
//! exactly once.
//!
//! ```
//! use std::time::Duration;
//! use vieww_animation::cinematic::{CameraState, Sequencer, Shot, Transition};
//! use vieww_animation::Curve;
//!
//! let s = |x| Duration::from_secs_f32(x);
//! let wide = CameraState::looking(([0.0, 2.0, 10.0]), [0.0, 0.0, 0.0], 0.8);
//! let close = CameraState::looking(([0.0, 0.5, 3.0]), [0.0, 0.0, 0.0], 0.5);
//! let seq = Sequencer::new()
//!     .shot(Shot::new("establish", s(4.0), wide, wide.dolly(-2.0)).curve(Curve::EASE_IN_OUT))
//!     .transition(Transition::Cut)
//!     .shot(Shot::new("hero", s(3.0), close, close.orbit(0.6)))
//!     .event("logo", s(5.0));
//! assert_eq!(seq.duration(), s(7.0));
//! assert_eq!(seq.evaluate(s(4.5)).name, "hero");
//! assert_eq!(seq.cuts(), vec![s(4.0)]);
//! assert_eq!(seq.events_between(s(4.9), s(5.1)), vec!["logo"]);
//! ```

use std::time::Duration;

use crate::noise::Perlin;
use crate::{Curve, Lerp};

/// Everything a camera needs for one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraState {
    pub eye: [f32; 3],
    pub target: [f32; 3],
    /// Vertical field of view in radians.
    pub fov: f32,
    /// Rotation about the view axis, radians.
    pub roll: f32,
    /// Distance to the plane in focus.
    pub focus: f32,
    /// Aperture (0 = pinhole, everything sharp).
    pub aperture: f32,
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn len(a: [f32; 3]) -> f32 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

impl CameraState {
    /// A pinhole camera at `eye` looking at `target`, focused on it.
    #[must_use]
    pub fn looking(eye: [f32; 3], target: [f32; 3], fov: f32) -> Self {
        Self {
            eye,
            target,
            fov,
            roll: 0.0,
            focus: len(sub(target, eye)),
            aperture: 0.0,
        }
    }

    /// Distance from eye to target.
    #[must_use]
    pub fn distance(&self) -> f32 {
        len(sub(self.target, self.eye))
    }

    /// Move the eye `amount` along the view direction (positive = closer).
    #[must_use]
    pub fn dolly(mut self, amount: f32) -> Self {
        let d = sub(self.target, self.eye);
        let l = len(d).max(1e-6);
        self.eye = add(self.eye, scale(d, amount / l));
        self.focus = self.distance();
        self
    }

    /// Orbit the eye about the target's vertical axis by `radians`.
    #[must_use]
    pub fn orbit(mut self, radians: f32) -> Self {
        let r = sub(self.eye, self.target);
        let (s, c) = radians.sin_cos();
        self.eye = add(self.target, [r[0] * c + r[2] * s, r[1], -r[0] * s + r[2] * c]);
        self
    }

    /// Hitchcock's dolly zoom: keep the subject's size at `target` constant
    /// while moving to `new_distance` — the fov that does it.
    #[must_use]
    pub fn dolly_zoom(self, new_distance: f32) -> Self {
        let d0 = self.distance().max(1e-6);
        let width = 2.0 * d0 * (self.fov * 0.5).tan();
        let dir = scale(sub(self.eye, self.target), 1.0 / d0);
        Self {
            eye: add(self.target, scale(dir, new_distance)),
            fov: 2.0 * (width / (2.0 * new_distance.max(1e-6))).atan(),
            focus: new_distance,
            ..self
        }
    }

    #[must_use]
    pub const fn aperture(mut self, a: f32) -> Self {
        self.aperture = a;
        self
    }

    #[must_use]
    pub const fn roll(mut self, r: f32) -> Self {
        self.roll = r;
        self
    }
}

impl Lerp for CameraState {
    fn lerp(self, o: Self, t: f32) -> Self {
        let l3 = |a: [f32; 3], b: [f32; 3]| add(a, scale(sub(b, a), t));
        Self {
            eye: l3(self.eye, o.eye),
            target: l3(self.target, o.target),
            fov: self.fov.lerp(o.fov, t),
            roll: self.roll.lerp(o.roll, t),
            focus: self.focus.lerp(o.focus, t),
            aperture: self.aperture.lerp(o.aperture, t),
        }
    }
}

/// One camera shot.
#[derive(Debug, Clone)]
pub struct Shot {
    pub name: String,
    pub duration: Duration,
    pub from: CameraState,
    pub to: CameraState,
    pub curve: Curve,
    /// Eye path control points (Catmull–Rom, through `from.eye` … `to.eye`).
    pub path: Vec<[f32; 3]>,
    /// Handheld shake amplitude (world units) and frequency (Hz).
    pub shake: (f32, f32),
    pub seed: u64,
}

impl Shot {
    #[must_use]
    pub fn new(name: &str, duration: Duration, from: CameraState, to: CameraState) -> Self {
        Self {
            name: name.to_owned(),
            duration,
            from,
            to,
            curve: Curve::Linear,
            path: Vec::new(),
            shake: (0.0, 0.0),
            seed: 1,
        }
    }

    #[must_use]
    pub fn curve(mut self, c: Curve) -> Self {
        self.curve = c;
        self
    }

    /// Route the eye through intermediate points.
    #[must_use]
    pub fn through(mut self, points: Vec<[f32; 3]>) -> Self {
        self.path = points;
        self
    }

    #[must_use]
    pub fn shake(mut self, amplitude: f32, hz: f32, seed: u64) -> Self {
        self.shake = (amplitude, hz);
        self.seed = seed;
        self
    }

    /// The camera `local` into this shot.
    #[must_use]
    pub fn camera(&self, local: Duration) -> CameraState {
        let u = if self.duration.is_zero() {
            1.0
        } else {
            (local.as_secs_f32() / self.duration.as_secs_f32()).clamp(0.0, 1.0)
        };
        let k = self.curve.transform(u);
        let mut cam = self.from.lerp(self.to, k);
        if !self.path.is_empty() {
            let mut pts = vec![self.from.eye];
            pts.extend(&self.path);
            pts.push(self.to.eye);
            cam.eye = catmull_rom(&pts, k);
        }
        if self.shake.0 > 0.0 {
            let n = Perlin::from_seed(self.seed);
            let x = local.as_secs_f32() * self.shake.1;
            let j = [n.noise1(x), n.noise1(x + 31.7), n.noise1(x + 77.1)];
            cam.eye = add(cam.eye, scale(j, self.shake.0));
            cam.target = add(cam.target, scale(j, self.shake.0 * 0.5));
        }
        cam
    }
}

/// Uniform Catmull–Rom through `pts` at `u ∈ [0,1]`.
fn catmull_rom(pts: &[[f32; 3]], u: f32) -> [f32; 3] {
    let n = pts.len();
    if n == 1 {
        return pts[0];
    }
    #[allow(clippy::cast_precision_loss)]
    let f = u.clamp(0.0, 1.0) * (n - 1) as f32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let i = (f.floor() as usize).min(n - 2);
    #[allow(clippy::cast_precision_loss)]
    let t = f - i as f32;
    let p = |k: isize| {
        let idx = (i as isize + k).clamp(0, n as isize - 1);
        #[allow(clippy::cast_sign_loss)]
        pts[idx as usize]
    };
    let (p0, p1, p2, p3) = (p(-1), p(0), p(1), p(2));
    let (t2, t3) = (t * t, t * t * t);
    let mut out = [0.0; 3];
    for k in 0..3 {
        out[k] = 0.5
            * (2.0 * p1[k]
                + (-p0[k] + p2[k]) * t
                + (2.0 * p0[k] - 5.0 * p1[k] + 4.0 * p2[k] - p3[k]) * t2
                + (-p0[k] + 3.0 * p1[k] - 3.0 * p2[k] + p3[k]) * t3);
    }
    out
}

/// What sits between two shots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    /// The camera-cuts track: switch on the frame.
    Cut,
    /// Interpolate from the outgoing camera to the incoming one over the
    /// first `Duration` of the incoming shot.
    Blend(Duration),
}

/// The state of the sequence at one time.
#[derive(Debug, Clone, PartialEq)]
pub struct ShotSample {
    pub index: usize,
    pub name: String,
    /// Time into the live shot.
    pub local: Duration,
    /// `local / duration` in `[0, 1]`.
    pub progress: f32,
    pub camera: CameraState,
    /// Inside a blend: `(outgoing shot index, weight of the incoming shot)`.
    pub blend: Option<(usize, f32)>,
}

/// An ordered list of shots, transitions and event markers.
#[derive(Debug, Clone, Default)]
pub struct Sequencer {
    shots: Vec<Shot>,
    /// `transitions[i]` sits before `shots[i + 1]`.
    transitions: Vec<Transition>,
    pending: Option<Transition>,
    events: Vec<(Duration, String)>,
}

impl Sequencer {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn shot(mut self, shot: Shot) -> Self {
        if !self.shots.is_empty() {
            self.transitions
                .push(self.pending.take().unwrap_or(Transition::Cut));
        }
        self.shots.push(shot);
        self
    }

    /// The transition into the next shot added.
    #[must_use]
    pub fn transition(mut self, t: Transition) -> Self {
        self.pending = Some(t);
        self
    }

    /// A named marker at an absolute sequence time.
    #[must_use]
    pub fn event(mut self, name: &str, at: Duration) -> Self {
        self.events.push((at, name.to_owned()));
        self.events.sort_by_key(|e| e.0);
        self
    }

    #[must_use]
    pub fn shots(&self) -> &[Shot] {
        &self.shots
    }

    #[must_use]
    pub fn duration(&self) -> Duration {
        self.shots.iter().map(|s| s.duration).sum()
    }

    /// Start time of each shot.
    #[must_use]
    pub fn starts(&self) -> Vec<Duration> {
        let mut t = Duration::ZERO;
        self.shots
            .iter()
            .map(|s| {
                let start = t;
                t += s.duration;
                start
            })
            .collect()
    }

    /// The times of every hard cut.
    #[must_use]
    pub fn cuts(&self) -> Vec<Duration> {
        let starts = self.starts();
        self.transitions
            .iter()
            .enumerate()
            .filter(|(_, t)| **t == Transition::Cut)
            .map(|(i, _)| starts[i + 1])
            .collect()
    }

    /// Event names with `from < time <= to` — fire each once when stepping.
    #[must_use]
    pub fn events_between(&self, from: Duration, to: Duration) -> Vec<&str> {
        self.events
            .iter()
            .filter(|(t, _)| *t > from && *t <= to)
            .map(|(_, n)| n.as_str())
            .collect()
    }

    /// The live shot and camera at `t` (clamped to the sequence).
    ///
    /// # Panics
    /// If the sequence has no shots.
    #[must_use]
    pub fn evaluate(&self, t: Duration) -> ShotSample {
        assert!(!self.shots.is_empty(), "a sequence needs a shot");
        let starts = self.starts();
        let t = t.min(self.duration());
        let index = starts
            .iter()
            .rposition(|s| *s <= t)
            .unwrap_or(0)
            .min(self.shots.len() - 1);
        let shot = &self.shots[index];
        let local = (t - starts[index]).min(shot.duration);
        let progress = if shot.duration.is_zero() {
            1.0
        } else {
            local.as_secs_f32() / shot.duration.as_secs_f32()
        };
        let mut camera = shot.camera(local);
        let mut blend = None;
        if index > 0 {
            if let Transition::Blend(w) = self.transitions[index - 1] {
                if local < w && !w.is_zero() {
                    let prev = &self.shots[index - 1];
                    let out_cam = prev.camera(prev.duration);
                    let k = Curve::EASE_IN_OUT.transform(local.as_secs_f32() / w.as_secs_f32());
                    camera = out_cam.lerp(camera, k);
                    blend = Some((index - 1, k));
                }
            }
        }
        ShotSample {
            index,
            name: shot.name.clone(),
            local,
            progress,
            camera,
            blend,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: f32) -> Duration {
        Duration::from_secs_f32(x)
    }

    fn cam(z: f32) -> CameraState {
        CameraState::looking([0.0, 0.0, z], [0.0, 0.0, 0.0], 0.8)
    }

    #[test]
    fn shots_play_in_order_and_cut() {
        let seq = Sequencer::new()
            .shot(Shot::new("a", s(1.0), cam(10.0), cam(8.0)))
            .shot(Shot::new("b", s(2.0), cam(4.0), cam(2.0)));
        assert_eq!(seq.evaluate(s(0.5)).name, "a");
        assert!((seq.evaluate(s(0.5)).camera.eye[2] - 9.0).abs() < 1e-4);
        let b = seq.evaluate(s(1.0));
        assert_eq!(b.name, "b");
        assert!((b.camera.eye[2] - 4.0).abs() < 1e-4, "a cut is instant");
        assert_eq!(seq.cuts(), vec![s(1.0)]);
        assert_eq!(seq.evaluate(s(99.0)).index, 1);
    }

    #[test]
    fn blends_interpolate_cameras() {
        let seq = Sequencer::new()
            .shot(Shot::new("a", s(1.0), cam(10.0), cam(10.0)))
            .transition(Transition::Blend(s(1.0)))
            .shot(Shot::new("b", s(2.0), cam(2.0), cam(2.0)));
        let mid = seq.evaluate(s(1.5));
        let (out, w) = mid.blend.unwrap();
        assert_eq!(out, 0);
        assert!((w - 0.5).abs() < 1e-3);
        assert!((mid.camera.eye[2] - 6.0).abs() < 1e-2);
        assert!(seq.evaluate(s(2.5)).blend.is_none());
        assert!(seq.cuts().is_empty());
    }

    #[test]
    fn dolly_path_passes_through_points() {
        let shot = Shot::new("p", s(2.0), cam(10.0), cam(0.0)).through(vec![[5.0, 0.0, 5.0]]);
        let mid = shot.camera(s(1.0));
        assert!((mid.eye[0] - 5.0).abs() < 1e-3);
        assert!((mid.eye[2] - 5.0).abs() < 1e-3);
    }

    #[test]
    fn dolly_zoom_keeps_subject_width() {
        let c = cam(10.0);
        let z = c.dolly_zoom(4.0);
        let width = |c: CameraState| 2.0 * c.distance() * (c.fov * 0.5).tan();
        assert!((width(c) - width(z)).abs() < 1e-3);
        assert!(z.fov > c.fov);
    }

    #[test]
    fn orbit_preserves_distance() {
        let c = cam(10.0).orbit(1.0);
        assert!((c.distance() - 10.0).abs() < 1e-4);
        assert!(c.eye[0].abs() > 1.0);
    }

    #[test]
    fn shake_is_deterministic_and_bounded() {
        let shot = Shot::new("h", s(4.0), cam(5.0), cam(5.0)).shake(0.1, 3.0, 7);
        let a = shot.camera(s(1.3));
        let b = shot.camera(s(1.3));
        assert_eq!(a, b);
        assert!((a.eye[2] - 5.0).abs() <= 0.2);
        assert!(a != shot.camera(s(2.1)));
    }

    #[test]
    fn events_fire_once_per_step() {
        let seq = Sequencer::new()
            .shot(Shot::new("a", s(3.0), cam(1.0), cam(1.0)))
            .event("x", s(1.0))
            .event("y", s(2.0));
        let mut fired = Vec::new();
        let mut prev = Duration::ZERO;
        for f in 1..=180 {
            let now = Duration::from_secs_f64(f64::from(f) / 60.0);
            fired.extend(seq.events_between(prev, now).iter().map(|s| s.to_string()));
            prev = now;
        }
        assert_eq!(fired, vec!["x", "y"]);
    }
}
