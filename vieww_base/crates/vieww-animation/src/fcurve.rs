//! F-curves — Blender's and After Effects' value graph, with handles.
//!
//! # The difference from a keyframe track
//!
//! A [`Keyframes`](crate::Keyframes) segment is eased by a [`Curve`](crate::Curve):
//! a *time* remap from 0..1 to 0..1. The value can never leave the interval
//! between its two keys. That is CSS's and GSAP's model, and it cannot say
//! what an animator draws in Blender's Graph Editor or After Effects' value
//! graph every day:
//!
//! * an **overshoot** set by dragging a handle above the next key;
//! * an **asymmetric** ease — a long out-handle and a short in-handle;
//! * a **continuous tangent through a key** (auto handles), so a ball's arc
//!   does not kink at its apex;
//! * **extrapolation** past the last key — hold, or keep the slope going;
//! * **F-modifiers** stacked on the result: *Cycles* (repeat, repeat with
//!   offset, mirror), *Noise*, *Stepped*, *Limits*.
//!
//! Here each key is a point in (time, value) space with a left and right
//! handle, and a segment is the 2D cubic Bézier through key, right handle,
//! next key's left handle, next key. Sampling at time `t` solves the Bézier's
//! x(u) = t for u (Newton with a bisection fallback — x is monotone because
//! handles are clamped to their segment, as Blender does) and returns y(u).
//!
//! ```
//! use vieww_animation::fcurve::{FCurve, Handles};
//!
//! let mut c = FCurve::new();
//! c.insert(0.0, 0.0).insert(1.0, 10.0).insert(2.0, 0.0);
//! // Auto-clamped handles: flat at the peak, so no overshoot above 10.
//! assert!((c.evaluate(1.0) - 10.0).abs() < 1e-4);
//! assert!(c.evaluate(0.9) <= 10.0 && c.evaluate(1.1) <= 10.0);
//! ```

use crate::noise::Perlin;

/// How a key's handles are computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Handles {
    /// Smooth through the key, flattened at local extremes so the curve
    /// never overshoots a key's value — Blender's default.
    #[default]
    AutoClamped,
    /// Smooth through the key (Catmull-Rom tangent), overshoot allowed.
    Auto,
    /// Pointing straight at the neighbouring keys: sharp corners.
    Vector,
    /// Set by hand with [`FCurve::set_handles`], left and right kept
    /// independent ("free").
    Free,
}

/// How a segment is interpolated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Interpolation {
    #[default]
    Bezier,
    Linear,
    /// Hold the key's value until the next key.
    Constant,
}

/// What happens before the first key and after the last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Extrapolation {
    #[default]
    Constant,
    /// Continue along the end handle's slope.
    Linear,
}

/// One key: a point and its two handles (absolute (time, value) points).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Key {
    pub time: f32,
    pub value: f32,
    pub left: (f32, f32),
    pub right: (f32, f32),
    pub handles: Handles,
    /// Interpolation of the segment *leaving* this key.
    pub interpolation: Interpolation,
}

/// How a Cycles modifier repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleMode {
    /// Exactly the same each time.
    Repeat,
    /// Each repeat starts where the last ended (a walk cycle's root).
    RepeatWithOffset,
    /// Forward, then backward.
    Mirror,
}

/// A stacked post-process on the evaluated curve — Blender's F-modifiers.
#[derive(Debug, Clone, PartialEq)]
pub enum Modifier {
    /// Repeat the keyed range outside itself.
    Cycles { before: Option<CycleMode>, after: Option<CycleMode> },
    /// Add smooth noise.
    Noise { scale: f32, strength: f32, phase: f32, seed: u64 },
    /// Quantise time to steps of `step` seconds (the "on twos" look).
    Stepped { step: f32, offset: f32 },
    /// Clamp the value.
    Limits { min: f32, max: f32 },
}

/// A value graph. See the [module docs](self).
#[derive(Debug, Clone, Default)]
pub struct FCurve {
    keys: Vec<Key>,
    pub extrapolation: Extrapolation,
    modifiers: Vec<Modifier>,
}

impl FCurve {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or replace) a key with automatic clamped handles.
    pub fn insert(&mut self, time: f32, value: f32) -> &mut Self {
        self.insert_key(time, value, Handles::AutoClamped, Interpolation::Bezier)
    }

    /// Insert with explicit handle type and interpolation.
    pub fn insert_key(&mut self, time: f32, value: f32, handles: Handles, interpolation: Interpolation) -> &mut Self {
        let key = Key {
            time,
            value,
            left: (time, value),
            right: (time, value),
            handles,
            interpolation,
        };
        match self.keys.iter().position(|k| (k.time - time).abs() < 1e-6) {
            Some(i) => self.keys[i] = key,
            None => {
                let i = self.keys.partition_point(|k| k.time < time);
                self.keys.insert(i, key);
            }
        }
        self.recalc();
        self
    }

    /// Set a key's handles by hand (makes it [`Handles::Free`]). Handles are
    /// absolute (time, value) points.
    pub fn set_handles(&mut self, index: usize, left: (f32, f32), right: (f32, f32)) -> &mut Self {
        if let Some(k) = self.keys.get_mut(index) {
            k.handles = Handles::Free;
            k.left = left;
            k.right = right;
        }
        self.recalc();
        self
    }

    /// Stack a modifier; they apply in insertion order.
    pub fn modifier(&mut self, m: Modifier) -> &mut Self {
        self.modifiers.push(m);
        self
    }

    #[must_use]
    pub fn keys(&self) -> &[Key] {
        &self.keys
    }

    /// Recompute automatic handles and clamp every handle into its segment.
    fn recalc(&mut self) {
        let n = self.keys.len();
        for i in 0..n {
            let k = self.keys[i];
            let prev = (i > 0).then(|| self.keys[i - 1]);
            let next = (i + 1 < n).then(|| self.keys[i + 1]);
            match k.handles {
                Handles::Free => {}
                Handles::Vector => {
                    let (l, r) = (prev, next);
                    self.keys[i].left = l.map_or((k.time, k.value), |p| {
                        (k.time + (p.time - k.time) / 3.0, k.value + (p.value - k.value) / 3.0)
                    });
                    self.keys[i].right = r.map_or((k.time, k.value), |q| {
                        (k.time + (q.time - k.time) / 3.0, k.value + (q.value - k.value) / 3.0)
                    });
                }
                Handles::Auto | Handles::AutoClamped => {
                    let slope = match (prev, next) {
                        (Some(p), Some(q)) => {
                            let is_extreme = (k.value >= p.value && k.value >= q.value)
                                || (k.value <= p.value && k.value <= q.value);
                            if k.handles == Handles::AutoClamped && is_extreme {
                                0.0
                            } else {
                                (q.value - p.value) / (q.time - p.time).max(1e-6)
                            }
                        }
                        // An end key: auto-clamped lies flat (Blender), plain
                        // auto points along the chord to its one neighbour.
                        (Some(p), None) if k.handles == Handles::Auto => {
                            (k.value - p.value) / (k.time - p.time).max(1e-6)
                        }
                        (None, Some(q)) if k.handles == Handles::Auto => {
                            (q.value - k.value) / (q.time - k.time).max(1e-6)
                        }
                        _ => 0.0,
                    };
                    let dl = prev.map_or(0.0, |p| (k.time - p.time) / 3.0);
                    let dr = next.map_or(0.0, |q| (q.time - k.time) / 3.0);
                    self.keys[i].left = (k.time - dl, k.value - slope * dl);
                    self.keys[i].right = (k.time + dr, k.value + slope * dr);
                }
            }
        }
        // Clamp handle times into their segments so x(u) stays monotone.
        for i in 0..n {
            let t = self.keys[i].time;
            let lo = if i > 0 { self.keys[i - 1].time } else { f32::NEG_INFINITY };
            let hi = if i + 1 < n { self.keys[i + 1].time } else { f32::INFINITY };
            let k = &mut self.keys[i];
            k.left.0 = k.left.0.clamp(lo, t);
            k.right.0 = k.right.0.clamp(t, hi);
        }
        // If a segment's two handles together reach past each other, scale
        // both back (keeping their slopes) until they meet — Blender's
        // correction, and what keeps x(u) monotone.
        for i in 0..n.saturating_sub(1) {
            let (a, b) = (self.keys[i], self.keys[i + 1]);
            let span = b.time - a.time;
            let reach = (a.right.0 - a.time) + (b.time - b.left.0);
            if reach > span && reach > 0.0 {
                let f = span / reach;
                let k = &mut self.keys[i];
                k.right = (k.time + (k.right.0 - k.time) * f, k.value + (k.right.1 - k.value) * f);
                let k = &mut self.keys[i + 1];
                k.left = (k.time - (k.time - k.left.0) * f, k.value - (k.value - k.left.1) * f);
            }
        }
    }

    /// The raw curve (no modifiers) at `t`.
    #[must_use]
    pub fn evaluate_raw(&self, t: f32) -> f32 {
        let (Some(first), Some(last)) = (self.keys.first(), self.keys.last()) else {
            return 0.0;
        };
        if t <= first.time {
            return match self.extrapolation {
                Extrapolation::Constant => first.value,
                Extrapolation::Linear => {
                    let (dt, dv) = (first.right.0 - first.time, first.right.1 - first.value);
                    let slope = if dt.abs() > 1e-6 { dv / dt } else { 0.0 };
                    first.value + slope * (t - first.time)
                }
            };
        }
        if t >= last.time {
            return match self.extrapolation {
                Extrapolation::Constant => last.value,
                Extrapolation::Linear => {
                    let (dt, dv) = (last.time - last.left.0, last.value - last.left.1);
                    let slope = if dt.abs() > 1e-6 { dv / dt } else { 0.0 };
                    last.value + slope * (t - last.time)
                }
            };
        }
        let i = self.keys.partition_point(|k| k.time <= t) - 1;
        let (a, b) = (self.keys[i], self.keys[i + 1]);
        match a.interpolation {
            Interpolation::Constant => a.value,
            Interpolation::Linear => a.value + (b.value - a.value) * (t - a.time) / (b.time - a.time),
            Interpolation::Bezier => {
                let xs = [a.time, a.right.0, b.left.0, b.time];
                let ys = [a.value, a.right.1, b.left.1, b.value];
                let u = solve_u(xs, t);
                bez(ys, u)
            }
        }
    }

    /// The curve at `t`, modifiers applied.
    #[must_use]
    pub fn evaluate(&self, t: f32) -> f32 {
        let mut time = t;
        // Time-remapping modifiers first (Cycles, Stepped), then value ones.
        let mut offset = 0.0;
        if let (Some(first), Some(last)) = (self.keys.first(), self.keys.last()) {
            let span = last.time - first.time;
            for m in &self.modifiers {
                match *m {
                    Modifier::Cycles { before, after } if span > 0.0 => {
                        let mode = if time > last.time {
                            after
                        } else if time < first.time {
                            before
                        } else {
                            None
                        };
                        if let Some(mode) = mode {
                            let rel = time - first.time;
                            let cycle = (rel / span).floor();
                            let mut local = rel - cycle * span;
                            #[allow(clippy::cast_possible_truncation)]
                            if mode == CycleMode::Mirror && (cycle as i64).rem_euclid(2) == 1 {
                                local = span - local;
                            }
                            if mode == CycleMode::RepeatWithOffset {
                                offset += cycle * (last.value - first.value);
                            }
                            time = first.time + local;
                        }
                    }
                    Modifier::Stepped { step, offset: o } if step > 0.0 => {
                        time = ((time - o) / step).floor() * step + o;
                    }
                    _ => {}
                }
            }
        }
        let mut v = self.evaluate_raw(time) + offset;
        for m in &self.modifiers {
            match *m {
                Modifier::Noise {
                    scale,
                    strength,
                    phase,
                    seed,
                } => {
                    let p = Perlin::from_seed(seed);
                    v += p.noise2(t / scale.max(1e-4), phase) * strength;
                }
                Modifier::Limits { min, max } => v = v.clamp(min, max),
                _ => {}
            }
        }
        v
    }
}

fn bez(p: [f32; 4], u: f32) -> f32 {
    let w = 1.0 - u;
    w * w * w * p[0] + 3.0 * w * w * u * p[1] + 3.0 * w * u * u * p[2] + u * u * u * p[3]
}

fn bez_d(p: [f32; 4], u: f32) -> f32 {
    let w = 1.0 - u;
    3.0 * w * w * (p[1] - p[0]) + 6.0 * w * u * (p[2] - p[1]) + 3.0 * u * u * (p[3] - p[2])
}

/// The Bézier parameter at which x reaches `t`.
fn solve_u(xs: [f32; 4], t: f32) -> f32 {
    let span = xs[3] - xs[0];
    let mut u = if span > 0.0 { ((t - xs[0]) / span).clamp(0.0, 1.0) } else { 0.0 };
    for _ in 0..8 {
        let x = bez(xs, u) - t;
        if x.abs() < 1e-6 {
            return u;
        }
        let d = bez_d(xs, u);
        if d.abs() < 1e-6 {
            break;
        }
        u = (u - x / d).clamp(0.0, 1.0);
    }
    // Bisection: always converges since x(u) is monotone.
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if bez(xs, mid) < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, e: f32) -> bool {
        (a - b).abs() < e
    }

    #[test]
    fn keys_are_hit_exactly_and_sorted() {
        let mut c = FCurve::new();
        c.insert(2.0, 5.0).insert(0.0, 1.0).insert(1.0, 3.0);
        assert_eq!(c.keys().iter().map(|k| k.time).collect::<Vec<_>>(), [0.0, 1.0, 2.0]);
        for (t, v) in [(0.0, 1.0), (1.0, 3.0), (2.0, 5.0)] {
            assert!(close(c.evaluate(t), v, 1e-4));
        }
        c.insert(1.0, 4.0);
        assert!(close(c.evaluate(1.0), 4.0, 1e-4), "same time replaces");
    }

    #[test]
    fn auto_handles_pass_smoothly_through_monotone_keys() {
        let mut c = FCurve::new();
        for (t, v) in [(0.0, 0.0), (1.0, 1.0), (2.0, 2.0)] {
            c.insert_key(t, v, Handles::Auto, Interpolation::Bezier);
        }
        // A straight line of keys gives a straight line.
        assert!(close(c.evaluate(1.5), 1.5, 1e-3), "{}", c.evaluate(1.5));
        // Slope continuity at the middle key.
        let d = |t: f32| (c.evaluate(t + 1e-3) - c.evaluate(t - 1e-3)) / 2e-3;
        assert!(close(d(0.999), d(1.001), 0.05));
    }

    #[test]
    fn auto_clamped_never_overshoots_but_auto_can() {
        let mut clamped = FCurve::new();
        clamped.insert(0.0, 0.0).insert(1.0, 10.0).insert(1.2, 0.0);
        let max_c = (0..=120).map(|i| clamped.evaluate(i as f32 / 100.0)).fold(f32::MIN, f32::max);
        assert!(max_c <= 10.0 + 1e-4, "{max_c}");
        let mut auto = FCurve::new();
        auto.insert_key(0.0, 0.0, Handles::Auto, Interpolation::Bezier)
            .insert_key(1.0, 5.0, Handles::Auto, Interpolation::Bezier)
            .insert_key(1.1, 10.0, Handles::Auto, Interpolation::Bezier)
            .insert_key(3.0, 10.5, Handles::Auto, Interpolation::Bezier);
        let max_a = (0..=300).map(|i| auto.evaluate(i as f32 / 100.0)).fold(f32::MIN, f32::max);
        assert!(max_a > 10.5, "auto overshoots on a sharp rise: {max_a}");
    }

    #[test]
    fn free_handles_make_an_overshoot() {
        let mut c = FCurve::new();
        c.insert(0.0, 0.0).insert(1.0, 1.0);
        c.set_handles(0, (0.0, 0.0), (0.5, 2.0));
        c.set_handles(1, (0.8, 1.5), (1.0, 1.0));
        let peak = (0..=100).map(|i| c.evaluate(i as f32 / 100.0)).fold(f32::MIN, f32::max);
        assert!(peak > 1.1, "{peak}");
    }

    #[test]
    fn interpolation_modes_and_extrapolation() {
        let mut c = FCurve::new();
        c.insert_key(0.0, 0.0, Handles::Vector, Interpolation::Linear)
            .insert_key(1.0, 2.0, Handles::Vector, Interpolation::Constant)
            .insert_key(2.0, 4.0, Handles::Vector, Interpolation::Bezier);
        assert!(close(c.evaluate(0.5), 1.0, 1e-4));
        assert!(close(c.evaluate(1.5), 2.0, 1e-4), "constant holds");
        assert!(close(c.evaluate(5.0), 4.0, 1e-4));
        c.extrapolation = Extrapolation::Linear;
        assert!(close(c.evaluate(3.0), 6.0, 1e-3), "{}", c.evaluate(3.0));
    }

    #[test]
    fn cycles_modifier_repeats_offsets_and_mirrors() {
        let base = || {
            let mut c = FCurve::new();
            c.insert_key(0.0, 0.0, Handles::Vector, Interpolation::Linear)
                .insert_key(1.0, 1.0, Handles::Vector, Interpolation::Linear);
            c
        };
        let mut r = base();
        r.modifier(Modifier::Cycles { before: None, after: Some(CycleMode::Repeat) });
        assert!(close(r.evaluate(2.25), 0.25, 1e-4));
        let mut o = base();
        o.modifier(Modifier::Cycles { before: None, after: Some(CycleMode::RepeatWithOffset) });
        assert!(close(o.evaluate(2.25), 2.25, 1e-4));
        let mut m = base();
        m.modifier(Modifier::Cycles { before: Some(CycleMode::Mirror), after: Some(CycleMode::Mirror) });
        assert!(close(m.evaluate(1.25), 0.75, 1e-4));
        assert!(close(m.evaluate(-0.25), 0.25, 1e-4));
    }

    #[test]
    fn stepped_noise_and_limits() {
        let mut c = FCurve::new();
        c.insert_key(0.0, 0.0, Handles::Vector, Interpolation::Linear)
            .insert_key(1.0, 1.0, Handles::Vector, Interpolation::Linear);
        c.modifier(Modifier::Stepped { step: 0.25, offset: 0.0 });
        assert!(close(c.evaluate(0.6), 0.5, 1e-4));
        c.modifier(Modifier::Limits { min: 0.0, max: 0.3 });
        assert!(close(c.evaluate(0.9), 0.3, 1e-4));
        let mut n = FCurve::new();
        n.insert(0.0, 0.0).modifier(Modifier::Noise { scale: 0.5, strength: 2.0, phase: 0.3, seed: 7 });
        let vals: Vec<f32> = (0..50).map(|i| n.evaluate(i as f32 * 0.1)).collect();
        assert!(vals.iter().any(|v| v.abs() > 0.1) && vals.iter().all(|v| v.abs() <= 2.0));
    }
}
