//! SwiftUI's `PhaseAnimator` and `KeyframeAnimator` (§2.10 L6).
//!
//! * [`PhaseAnimator`] cycles through a list of discrete phases, each
//!   arriving over its own [`Curve`] and duration — "pulse: rest → grow →
//!   glow → rest" written as data. It can loop forever or run once per
//!   trigger.
//! * [`KeyframeAnimator`] drives a value through *typed* keyframes the way
//!   SwiftUI's `KeyframeTrack` does: `Linear`, `Cubic` (a Catmull–Rom spline
//!   through neighbouring keys, so motion is smooth across them), `Spring`
//!   (critically-damped-to-underdamped arrival with the velocity carried in),
//!   and `Move` (a jump). Several tracks for different properties run on one
//!   clock with [`KeyframeAnimator::sample`].
//!
//! Both are pure functions of elapsed time.
//!
//! ```
//! use std::time::Duration;
//! use vieww_animation::phase::{Kind, KeyframeTrack};
//!
//! let ms = Duration::from_millis;
//! let scale = KeyframeTrack::new(1.0)
//!     .key(Kind::Spring { response: 0.3, damping: 0.5 }, 1.4, ms(400))
//!     .key(Kind::Cubic, 1.0, ms(300))
//!     .key(Kind::Move, 0.8, ms(100));
//! assert_eq!(scale.value(ms(0)), 1.0);
//! assert!(scale.value(ms(200)) > 1.0);
//! assert_eq!(scale.value(ms(800)), 0.8);
//! ```

use std::time::Duration;

use crate::{Curve, Lerp};

/// One phase: a target value and how it is reached.
#[derive(Debug, Clone)]
pub struct Phase<T> {
    pub value: T,
    pub duration: Duration,
    pub curve: Curve,
}

/// Cycles through phases.
#[derive(Debug, Clone)]
pub struct PhaseAnimator<T> {
    phases: Vec<Phase<T>>,
    repeat: bool,
}

impl<T: Lerp> PhaseAnimator<T> {
    /// Starts at `first`; each `then` animates to the next phase.
    #[must_use]
    pub fn new(first: T) -> Self {
        Self {
            phases: vec![Phase {
                value: first,
                duration: Duration::ZERO,
                curve: Curve::Linear,
            }],
            repeat: true,
        }
    }

    #[must_use]
    pub fn then(mut self, value: T, duration: Duration, curve: Curve) -> Self {
        self.phases.push(Phase {
            value,
            duration,
            curve,
        });
        self
    }

    /// Run the cycle once and hold the last phase (SwiftUI's `trigger:` form).
    #[must_use]
    pub fn once(mut self) -> Self {
        self.repeat = false;
        self
    }

    /// The length of one cycle, including the return to the first phase
    /// when looping.
    #[must_use]
    pub fn cycle(&self) -> Duration {
        let body: Duration = self.phases.iter().skip(1).map(|p| p.duration).sum();
        if self.repeat {
            body + self.return_duration()
        } else {
            body
        }
    }

    fn return_duration(&self) -> Duration {
        self.phases.get(1).map_or(Duration::ZERO, |p| p.duration)
    }

    /// Which phase is active (the one being animated towards) and the value.
    #[must_use]
    pub fn sample(&self, elapsed: Duration) -> (usize, T) {
        let n = self.phases.len();
        if n == 1 {
            return (0, self.phases[0].value.clone());
        }
        let cycle = self.cycle();
        let mut t = if self.repeat && !cycle.is_zero() {
            Duration::from_nanos(
                u64::try_from(elapsed.as_nanos() % cycle.as_nanos()).unwrap_or(0),
            )
        } else {
            elapsed
        };
        for i in 1..=n {
            let (from, to, dur, curve) = if i < n {
                let p = &self.phases[i];
                (&self.phases[i - 1].value, &p.value, p.duration, p.curve)
            } else if self.repeat {
                let p = &self.phases[1];
                (&self.phases[n - 1].value, &self.phases[0].value, p.duration, p.curve)
            } else {
                break;
            };
            if t < dur {
                let k = curve.transform(t.as_secs_f32() / dur.as_secs_f32());
                return (i % n, from.clone().lerp(to.clone(), k));
            }
            t -= dur;
        }
        (
            if self.repeat { 0 } else { n - 1 },
            self.phases[if self.repeat { 0 } else { n - 1 }].value.clone(),
        )
    }

    #[must_use]
    pub fn value(&self, elapsed: Duration) -> T {
        self.sample(elapsed).1
    }
}

/// How a keyframe is reached.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// A straight line.
    Linear,
    /// Catmull–Rom through the neighbouring keys.
    Cubic,
    /// A spring with `response` (seconds per oscillation) and `damping`
    /// fraction (1 = critical); the velocity at entry is carried in.
    Spring { response: f32, damping: f32 },
    /// Jump at the start of the key's span.
    Move,
}

#[derive(Debug, Clone, Copy)]
struct Key {
    kind: Kind,
    value: f32,
    duration: Duration,
}

/// A keyframe track for one `f32` property.
#[derive(Debug, Clone)]
pub struct KeyframeTrack {
    initial: f32,
    keys: Vec<Key>,
}

impl KeyframeTrack {
    #[must_use]
    pub fn new(initial: f32) -> Self {
        Self {
            initial,
            keys: Vec::new(),
        }
    }

    /// Append a keyframe reached over `duration`.
    #[must_use]
    pub fn key(mut self, kind: Kind, value: f32, duration: Duration) -> Self {
        self.keys.push(Key {
            kind,
            value,
            duration,
        });
        self
    }

    #[must_use]
    pub fn duration(&self) -> Duration {
        self.keys.iter().map(|k| k.duration).sum()
    }

    fn value_before(&self, i: usize) -> f32 {
        if i == 0 {
            self.initial
        } else {
            self.keys[i - 1].value
        }
    }

    /// The value `elapsed` into the track (holds the last key afterwards).
    #[must_use]
    pub fn value(&self, elapsed: Duration) -> f32 {
        self.sample(elapsed).0
    }

    /// `(value, velocity)` at `elapsed`.
    #[must_use]
    pub fn sample(&self, elapsed: Duration) -> (f32, f32) {
        let mut start = Duration::ZERO;
        let mut entry_v = 0.0;
        for (i, k) in self.keys.iter().enumerate() {
            let end = start + k.duration;
            let from = self.value_before(i);
            if elapsed < end {
                let local = (elapsed - start).as_secs_f32();
                return self.eval(i, from, local, entry_v);
            }
            // Velocity at the end of this key, carried into the next.
            entry_v = self.eval(i, from, k.duration.as_secs_f32(), entry_v).1;
            start = end;
        }
        (self.keys.last().map_or(self.initial, |k| k.value), 0.0)
    }

    fn eval(&self, i: usize, from: f32, t: f32, v0: f32) -> (f32, f32) {
        let k = self.keys[i];
        let d = k.duration.as_secs_f32().max(1e-6);
        let u = (t / d).clamp(0.0, 1.0);
        match k.kind {
            Kind::Move => (k.value, 0.0),
            Kind::Linear => (from + (k.value - from) * u, (k.value - from) / d),
            Kind::Cubic => {
                let p0 = if i == 0 {
                    from
                } else {
                    self.value_before(i - 1)
                };
                let p3 = self.keys.get(i + 1).map_or(k.value, |n| n.value);
                let (p1, p2) = (from, k.value);
                let u2 = u * u;
                let u3 = u2 * u;
                let v = 0.5
                    * ((2.0 * p1)
                        + (-p0 + p2) * u
                        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * u2
                        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * u3);
                let dv = 0.5
                    * ((-p0 + p2)
                        + 2.0 * (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * u
                        + 3.0 * (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * u2);
                (v, dv / d)
            }
            Kind::Spring { response, damping } => {
                spring(from, k.value, v0, response, damping, t)
            }
        }
    }
}

/// A damped harmonic oscillator from `x0` (velocity `v0`) to `target`.
fn spring(x0: f32, target: f32, v0: f32, response: f32, zeta: f32, t: f32) -> (f32, f32) {
    let w0 = std::f32::consts::TAU / response.max(1e-3);
    let zeta = zeta.max(0.0);
    let a = x0 - target;
    if zeta < 1.0 {
        let wd = w0 * (1.0 - zeta * zeta).sqrt();
        let b = (v0 + zeta * w0 * a) / wd;
        let e = (-zeta * w0 * t).exp();
        let (s, c) = (wd * t).sin_cos();
        let x = e * (a * c + b * s);
        let v = e * ((-zeta * w0) * (a * c + b * s) + (-a * wd * s + b * wd * c));
        (target + x, v)
    } else {
        let b = v0 + w0 * a;
        let e = (-w0 * t).exp();
        (target + (a + b * t) * e, (b - w0 * (a + b * t)) * e)
    }
}

/// Several named tracks on one clock — SwiftUI's `KeyframeAnimator`
/// with a struct of animatable properties.
#[derive(Debug, Clone, Default)]
pub struct KeyframeAnimator {
    tracks: Vec<(String, KeyframeTrack)>,
}

impl KeyframeAnimator {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn track(mut self, name: &str, track: KeyframeTrack) -> Self {
        self.tracks.push((name.to_owned(), track));
        self
    }

    /// The longest track.
    #[must_use]
    pub fn duration(&self) -> Duration {
        self.tracks
            .iter()
            .map(|(_, t)| t.duration())
            .max()
            .unwrap_or_default()
    }

    /// Every property at `elapsed`.
    #[must_use]
    pub fn sample(&self, elapsed: Duration) -> Vec<(&str, f32)> {
        self.tracks
            .iter()
            .map(|(n, t)| (n.as_str(), t.value(elapsed)))
            .collect()
    }

    /// One property by name.
    #[must_use]
    pub fn get(&self, name: &str, elapsed: Duration) -> Option<f32> {
        self.tracks
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, t)| t.value(elapsed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(v: u64) -> Duration {
        Duration::from_millis(v)
    }

    #[test]
    fn phases_cycle_and_return() {
        let p = PhaseAnimator::new(0.0_f32)
            .then(1.0, ms(100), Curve::Linear)
            .then(3.0, ms(200), Curve::Linear);
        assert_eq!(p.cycle(), ms(400));
        assert!((p.value(ms(50)) - 0.5).abs() < 1e-4);
        assert!((p.value(ms(200)) - 2.0).abs() < 1e-4);
        // The return leg 3 → 0 over the first phase's duration.
        assert!((p.value(ms(350)) - 1.5).abs() < 1e-4);
        assert!((p.value(ms(450)) - 0.5).abs() < 1e-4, "second cycle");
        assert_eq!(p.sample(ms(150)).0, 2);
    }

    #[test]
    fn once_holds_the_last_phase() {
        let p = PhaseAnimator::new(0.0_f32)
            .then(10.0, ms(100), Curve::Linear)
            .once();
        assert_eq!(p.value(ms(5000)), 10.0);
    }

    #[test]
    fn linear_and_move_keys() {
        let t = KeyframeTrack::new(0.0)
            .key(Kind::Linear, 10.0, ms(100))
            .key(Kind::Move, 50.0, ms(100));
        assert!((t.value(ms(50)) - 5.0).abs() < 1e-4);
        assert_eq!(t.value(ms(150)), 50.0);
    }

    #[test]
    fn cubic_passes_through_keys_and_is_smooth() {
        let t = KeyframeTrack::new(0.0)
            .key(Kind::Cubic, 10.0, ms(100))
            .key(Kind::Cubic, 0.0, ms(100))
            .key(Kind::Cubic, 10.0, ms(100));
        assert!((t.value(ms(100)) - 10.0).abs() < 1e-3);
        assert!((t.value(ms(200)) - 0.0).abs() < 1e-3);
        // Continuous across the key boundary.
        let a = t.value(ms(99));
        let b = t.value(ms(101));
        assert!((a - b).abs() < 0.5);
    }

    #[test]
    fn underdamped_spring_overshoots_then_settles() {
        let t = KeyframeTrack::new(0.0).key(
            Kind::Spring {
                response: 0.4,
                damping: 0.3,
            },
            1.0,
            ms(3000),
        );
        let peak = (0..300)
            .map(|i| t.value(ms(i * 10)))
            .fold(f32::MIN, f32::max);
        assert!(peak > 1.1, "overshoot {peak}");
        assert!((t.value(ms(2990)) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn critical_spring_does_not_overshoot() {
        let t = KeyframeTrack::new(0.0).key(
            Kind::Spring {
                response: 0.5,
                damping: 1.0,
            },
            1.0,
            ms(2000),
        );
        for i in 0..200 {
            assert!(t.value(ms(i * 10)) <= 1.0 + 1e-5);
        }
    }

    #[test]
    fn spring_carries_entry_velocity() {
        let moving = KeyframeTrack::new(0.0)
            .key(Kind::Linear, 1.0, ms(100))
            .key(
                Kind::Spring {
                    response: 0.5,
                    damping: 1.0,
                },
                1.0,
                ms(1000),
            );
        // Arriving at the target already moving, the spring overshoots it.
        assert!(moving.value(ms(150)) > 1.0);
    }

    #[test]
    fn animator_runs_tracks_on_one_clock() {
        let a = KeyframeAnimator::new()
            .track("x", KeyframeTrack::new(0.0).key(Kind::Linear, 1.0, ms(100)))
            .track("y", KeyframeTrack::new(5.0).key(Kind::Linear, 0.0, ms(500)));
        assert_eq!(a.duration(), ms(500));
        assert_eq!(a.get("x", ms(300)), Some(1.0));
        assert!((a.get("y", ms(250)).unwrap() - 2.5).abs() < 1e-4);
        assert_eq!(a.sample(ms(0)).len(), 2);
    }
}
