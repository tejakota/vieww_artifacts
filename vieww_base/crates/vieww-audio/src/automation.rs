//! Parameter automation — Web Audio's `AudioParam` timeline, and DAW
//! automation lanes built on it.
//!
//! A [`Param`] is a value with a schedule: set it at a time, ramp it
//! linearly or exponentially to a value by a time, let it approach a target
//! with a time constant, or play a curve of values over a duration — the six
//! `AudioParam` methods, with `cancelScheduledValues` and
//! `cancelAndHoldAtTime`. [`Param::value_at`] is a pure function of time, so
//! a parameter can be rendered sample-accurately for any block, in any
//! order, and the same automation renders the same way twice.
//!
//! A [`Lane`] is the DAW view of the same idea: points in *beats*, each
//! segment with its own curve (linear, exponential, smooth, step, or a
//! bezier tension), recorded from a controller with thinning so a knob
//! twist becomes a handful of points rather than thousands.
//!
//! # Semantics of ramps
//!
//! As in Web Audio, a ramp event *ends* at its time and starts from the
//! value and time of the event before it. When the event before it is a
//! `set_target`, the ramp starts from where that approach had got to by the
//! `set_target`'s own time — that is, the ramp replaces the approach rather
//! than continuing from part-way along it.

/// One scheduled event.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Jump to `value` at `time`.
    Set {
        /// Seconds.
        time: f64,
        /// Value.
        value: f32,
    },
    /// Arrive at `value` at `time`, linearly.
    Linear {
        /// Seconds.
        time: f64,
        /// Value.
        value: f32,
    },
    /// Arrive at `value` at `time`, exponentially (geometric). Both ends must
    /// have the same sign and be non-zero; otherwise it holds, as the spec
    /// does.
    Exponential {
        /// Seconds.
        time: f64,
        /// Value.
        value: f32,
    },
    /// From `time`, approach `target` with time constant `tau`.
    Target {
        /// Seconds.
        time: f64,
        /// Target value.
        target: f32,
        /// Time constant in seconds.
        tau: f64,
    },
    /// Play `values` linearly interpolated over `duration` from `time`.
    Curve {
        /// Seconds.
        time: f64,
        /// Seconds.
        duration: f64,
        /// Values (at least one).
        values: Vec<f32>,
    },
}

impl Event {
    /// When the event happens (its end for ramps, its start otherwise).
    #[must_use]
    pub const fn time(&self) -> f64 {
        match self {
            Self::Set { time, .. }
            | Self::Linear { time, .. }
            | Self::Exponential { time, .. }
            | Self::Target { time, .. }
            | Self::Curve { time, .. } => *time,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Hold {
    Constant,
    Target(f32, f64),
}

/// An automatable parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    /// The value before any event.
    pub default: f32,
    /// Lowest allowed value.
    pub min: f32,
    /// Highest allowed value.
    pub max: f32,
    events: Vec<Event>,
}

impl Param {
    /// A parameter with a default and range.
    #[must_use]
    pub const fn new(default: f32, min: f32, max: f32) -> Self {
        Self {
            default,
            min,
            max,
            events: Vec::new(),
        }
    }

    /// The scheduled events, in time order.
    #[must_use]
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    fn insert(&mut self, e: Event) -> &mut Self {
        // Stable insert after any events at the same time, like the spec.
        let t = e.time();
        let at = self.events.partition_point(|x| x.time() <= t);
        self.events.insert(at, e);
        self
    }

    /// `setValueAtTime`.
    pub fn set_at(&mut self, value: f32, time: f64) -> &mut Self {
        self.insert(Event::Set { time, value })
    }

    /// `linearRampToValueAtTime`.
    pub fn linear_to(&mut self, value: f32, time: f64) -> &mut Self {
        self.insert(Event::Linear { time, value })
    }

    /// `exponentialRampToValueAtTime`.
    pub fn exponential_to(&mut self, value: f32, time: f64) -> &mut Self {
        self.insert(Event::Exponential { time, value })
    }

    /// `setTargetAtTime`.
    pub fn target_at(&mut self, target: f32, time: f64, tau: f64) -> &mut Self {
        self.insert(Event::Target { time, target, tau })
    }

    /// `setValueCurveAtTime`.
    pub fn curve_at(&mut self, values: Vec<f32>, time: f64, duration: f64) -> &mut Self {
        if values.is_empty() {
            return self;
        }
        self.insert(Event::Curve {
            time,
            duration,
            values,
        })
    }

    /// `cancelScheduledValues`: remove every event at or after `time`.
    pub fn cancel(&mut self, time: f64) -> &mut Self {
        self.events.retain(|e| e.time() < time);
        self
    }

    /// `cancelAndHoldAtTime`: freeze at whatever the value is at `time`.
    pub fn cancel_and_hold(&mut self, time: f64) -> &mut Self {
        let v = self.value_at(time);
        // Ramps ending after `time` would change the held value; drop them
        // along with everything later, then pin the value.
        self.events.retain(|e| e.time() < time);
        self.set_at(v, time)
    }

    /// The value at `t` seconds.
    #[must_use]
    pub fn value_at(&self, t: f64) -> f32 {
        let mut prev_t = 0.0f64;
        let mut prev_v = self.default;
        let mut hold = Hold::Constant;
        let held = |hold: Hold, pt: f64, pv: f32, at: f64| -> f32 {
            match hold {
                Hold::Constant => pv,
                Hold::Target(target, tau) => {
                    if tau <= 0.0 {
                        target
                    } else {
                        #[expect(clippy::cast_possible_truncation, reason = "f32 param")]
                        let k = (-(at - pt).max(0.0) / tau).exp() as f32;
                        target + (pv - target) * k
                    }
                }
            }
        };
        for e in &self.events {
            match e {
                Event::Set { time, value } => {
                    if t < *time {
                        break;
                    }
                    prev_t = *time;
                    prev_v = *value;
                    hold = Hold::Constant;
                }
                Event::Linear { time, value } => {
                    if t < *time {
                        if t < prev_t || *time <= prev_t {
                            break;
                        }
                        #[expect(clippy::cast_possible_truncation, reason = "f32 param")]
                        let f = ((t - prev_t) / (*time - prev_t)) as f32;
                        return self.clamp(prev_v + (*value - prev_v) * f);
                    }
                    prev_t = *time;
                    prev_v = *value;
                    hold = Hold::Constant;
                }
                Event::Exponential { time, value } => {
                    let ok = prev_v != 0.0 && *value != 0.0 && (prev_v > 0.0) == (*value > 0.0);
                    if t < *time {
                        if !ok || t < prev_t || *time <= prev_t {
                            break;
                        }
                        let f = (t - prev_t) / (*time - prev_t);
                        #[expect(clippy::cast_possible_truncation, reason = "f32 param")]
                        let v = (f64::from(prev_v)
                            * (f64::from(*value) / f64::from(prev_v)).powf(f))
                            as f32;
                        return self.clamp(v);
                    }
                    if ok {
                        prev_v = *value;
                    }
                    prev_t = *time;
                    hold = Hold::Constant;
                }
                Event::Target { time, target, tau } => {
                    if t < *time {
                        break;
                    }
                    prev_v = held(hold, prev_t, prev_v, *time);
                    prev_t = *time;
                    hold = Hold::Target(*target, *tau);
                }
                Event::Curve {
                    time,
                    duration,
                    values,
                } => {
                    if t < *time {
                        break;
                    }
                    let end = time + duration.max(0.0);
                    if t < end && values.len() > 1 {
                        #[expect(clippy::cast_precision_loss, reason = "curve lengths")]
                        let x = (t - time) / duration * (values.len() - 1) as f64;
                        #[expect(
                            clippy::cast_possible_truncation,
                            clippy::cast_sign_loss,
                            reason = "in range"
                        )]
                        let i = (x.floor() as usize).min(values.len() - 2);
                        #[expect(clippy::cast_possible_truncation, reason = "fraction")]
                        let f = (x - x.floor()) as f32;
                        return self.clamp(values[i] + (values[i + 1] - values[i]) * f);
                    }
                    prev_t = end;
                    prev_v = *values.last().unwrap_or(&prev_v);
                    hold = Hold::Constant;
                }
            }
        }
        self.clamp(held(hold, prev_t, prev_v, t))
    }

    fn clamp(&self, v: f32) -> f32 {
        v.clamp(self.min.min(self.max), self.max.max(self.min))
    }

    /// Fill `out` with sample-accurate values starting at `start` seconds.
    pub fn fill(&self, start: f64, rate: f64, out: &mut [f32]) {
        if self.events.is_empty() {
            out.fill(self.clamp(self.default));
            return;
        }
        for (i, o) in out.iter_mut().enumerate() {
            #[expect(clippy::cast_precision_loss, reason = "block offsets")]
            let t = start + i as f64 / rate;
            *o = self.value_at(t);
        }
    }
}

/// A segment curve in an automation lane.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Shape {
    /// Straight line.
    #[default]
    Linear,
    /// Geometric (for gains and frequencies).
    Exponential,
    /// Cosine ease in-out.
    Smooth,
    /// Hold until the next point.
    Step,
    /// Power curve with tension −1..1 (negative bows early, positive late).
    Tension(f32),
}

/// A point in an automation lane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    /// Position in beats.
    pub beat: f64,
    /// Value.
    pub value: f32,
    /// The curve *from* this point to the next.
    pub shape: Shape,
}

/// A DAW automation lane: points in beats with per-segment curves.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Lane {
    /// Points, sorted by beat.
    pub points: Vec<Point>,
}

impl Lane {
    /// Add or replace a point.
    pub fn add(&mut self, beat: f64, value: f32, shape: Shape) {
        self.points.retain(|p| (p.beat - beat).abs() > 1e-9);
        let at = self.points.partition_point(|p| p.beat < beat);
        self.points.insert(at, Point { beat, value, shape });
    }

    /// The value at `beat` (holds the first/last value outside the points).
    #[must_use]
    pub fn value_at(&self, beat: f64) -> Option<f32> {
        let first = self.points.first()?;
        if beat <= first.beat {
            return Some(first.value);
        }
        let i = self.points.partition_point(|p| p.beat <= beat);
        if i >= self.points.len() {
            return self.points.last().map(|p| p.value);
        }
        let a = self.points[i - 1];
        let b = self.points[i];
        #[expect(clippy::cast_possible_truncation, reason = "f32 value")]
        let x = ((beat - a.beat) / (b.beat - a.beat)) as f32;
        let f = match a.shape {
            Shape::Linear => x,
            Shape::Step => 0.0,
            Shape::Smooth => 0.5 - 0.5 * (std::f32::consts::PI * x).cos(),
            Shape::Tension(k) => {
                let k = k.clamp(-0.99, 0.99);
                if k >= 0.0 {
                    x.powf(1.0 + 4.0 * k)
                } else {
                    1.0 - (1.0 - x).powf(1.0 - 4.0 * k)
                }
            }
            Shape::Exponential => {
                if a.value > 0.0 && b.value > 0.0 {
                    return Some(a.value * (b.value / a.value).powf(x));
                }
                x
            }
        };
        Some(a.value + (b.value - a.value) * f)
    }

    /// Record a controller: `samples` of `(beat, value)` thinned so that
    /// linear segments reproduce every sample within `tolerance`
    /// (Ramer–Douglas–Peucker).
    pub fn record(&mut self, samples: &[(f64, f32)], tolerance: f32) {
        if samples.is_empty() {
            return;
        }
        let (b0, b1) = (samples[0].0, samples[samples.len() - 1].0);
        self.points.retain(|p| p.beat < b0 || p.beat > b1);
        let mut keep = vec![false; samples.len()];
        keep[0] = true;
        let last = samples.len() - 1;
        keep[last] = true;
        let mut stack = vec![(0usize, last)];
        while let Some((a, b)) = stack.pop() {
            if b <= a + 1 {
                continue;
            }
            let (pa, pb) = (samples[a], samples[b]);
            let mut worst = (0.0f32, a);
            for (k, s) in samples.iter().enumerate().take(b).skip(a + 1) {
                #[expect(clippy::cast_possible_truncation, reason = "f32 value")]
                let x = ((s.0 - pa.0) / (pb.0 - pa.0)) as f32;
                let d = (pa.1 + (pb.1 - pa.1) * x - s.1).abs();
                if d > worst.0 {
                    worst = (d, k);
                }
            }
            if worst.0 > tolerance {
                keep[worst.1] = true;
                stack.push((a, worst.1));
                stack.push((worst.1, b));
            }
        }
        for (s, k) in samples.iter().zip(keep) {
            if k {
                self.add(s.0, s.1, Shape::Linear);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_and_linear_ramp() {
        let mut p = Param::new(0.0, -10.0, 10.0);
        p.set_at(1.0, 1.0).linear_to(3.0, 2.0);
        assert_eq!(p.value_at(0.5), 0.0);
        assert_eq!(p.value_at(1.0), 1.0);
        assert!((p.value_at(1.5) - 2.0).abs() < 1e-6);
        assert_eq!(p.value_at(5.0), 3.0);
    }

    #[test]
    fn exponential_ramp_is_geometric() {
        let mut p = Param::new(100.0, 0.0, 20_000.0);
        p.set_at(100.0, 0.0).exponential_to(10_000.0, 2.0);
        assert!(
            (p.value_at(1.0) - 1000.0).abs() < 1e-2,
            "{}",
            p.value_at(1.0)
        );
        // Invalid (crosses zero): holds the previous value.
        let mut q = Param::new(1.0, -5.0, 5.0);
        q.set_at(1.0, 0.0).exponential_to(-1.0, 1.0);
        assert_eq!(q.value_at(0.5), 1.0);
    }

    #[test]
    fn set_target_approaches_with_its_time_constant() {
        let mut p = Param::new(0.0, 0.0, 1.0);
        p.set_at(0.0, 0.0).target_at(1.0, 1.0, 0.5);
        assert_eq!(p.value_at(0.9), 0.0);
        let one_tau = p.value_at(1.5);
        assert!((one_tau - (1.0 - (-1.0f32).exp())).abs() < 1e-5);
    }

    #[test]
    fn value_curve_interpolates_then_holds_its_last() {
        let mut p = Param::new(0.0, -1.0, 1.0);
        p.curve_at(vec![0.0, 1.0, -1.0], 1.0, 2.0);
        assert!((p.value_at(1.5) - 0.5).abs() < 1e-6);
        assert!((p.value_at(2.5) - 0.0).abs() < 1e-6);
        assert_eq!(p.value_at(4.0), -1.0);
    }

    #[test]
    fn cancel_and_hold_freezes_mid_ramp() {
        let mut p = Param::new(0.0, 0.0, 10.0);
        p.set_at(0.0, 0.0).linear_to(10.0, 10.0);
        p.cancel_and_hold(4.0);
        assert!((p.value_at(4.0) - 4.0).abs() < 1e-5);
        assert!((p.value_at(8.0) - 4.0).abs() < 1e-5);
        p.cancel(0.0);
        assert_eq!(p.value_at(3.0), 0.0);
    }

    #[test]
    fn values_are_clamped_and_blocks_are_sample_accurate() {
        let mut p = Param::new(0.0, 0.0, 1.0);
        p.set_at(0.0, 0.0).linear_to(2.0, 1.0);
        assert_eq!(p.value_at(0.9), 1.0);
        let mut out = [0.0; 4];
        p.fill(0.0, 8.0, &mut out);
        assert_eq!(out, [0.0, 0.25, 0.5, 0.75]);
    }

    #[test]
    fn lane_shapes() {
        let mut l = Lane::default();
        l.add(0.0, 0.0, Shape::Linear);
        l.add(4.0, 1.0, Shape::Step);
        l.add(8.0, 0.0, Shape::Linear);
        assert!((l.value_at(2.0).unwrap() - 0.5).abs() < 1e-6);
        assert_eq!(l.value_at(6.0), Some(1.0));
        assert_eq!(l.value_at(100.0), Some(0.0));
        let mut e = Lane::default();
        e.add(0.0, 100.0, Shape::Exponential);
        e.add(2.0, 10_000.0, Shape::Linear);
        assert!((e.value_at(1.0).unwrap() - 1000.0).abs() < 0.1);
        let mut s = Lane::default();
        s.add(0.0, 0.0, Shape::Tension(0.5));
        s.add(1.0, 1.0, Shape::Linear);
        assert!(s.value_at(0.5).unwrap() < 0.5);
    }

    #[test]
    fn recording_thins_a_knob_twist() {
        // A ramp up then hold: 1000 samples become a few points.
        let samples: Vec<(f64, f32)> = (0..1000)
            .map(|i| {
                let b = f64::from(i) / 100.0;
                #[expect(clippy::cast_possible_truncation, reason = "test")]
                let v = if b < 5.0 { (b / 5.0) as f32 } else { 1.0 };
                (b, v)
            })
            .collect();
        let mut l = Lane::default();
        l.record(&samples, 0.001);
        assert!(l.points.len() <= 4, "{} points", l.points.len());
        for (b, v) in samples.iter().step_by(37) {
            assert!((l.value_at(*b).unwrap() - v).abs() < 0.002);
        }
    }
}
