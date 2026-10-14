//! Throws — GSAP's `InertiaPlugin` and `Physics2DPlugin` (§2.1 L6).
//!
//! * [`VelocityTracker`] — `InertiaPlugin.track()`: a least-squares fit over
//!   the last few pointer samples, so one jittery sample does not decide the
//!   throw.
//! * [`Inertia`] — the glide after release: exponential decay from the
//!   release velocity, with GSAP's `resistance`, an **end snap** (to the
//!   nearest of a list, a grid, or a function of the natural end) and
//!   `min`/`max` bounds. The decay is re-aimed so it *arrives exactly* at the
//!   snapped end — the throw never stops short and then jumps.
//! * [`Physics2D`] — velocity, angle, gravity, acceleration and friction as a
//!   closed form in `t`, the `physics2D: {velocity, angle, gravity}` tween.
//!
//! All three keep the crate's rule: position is a function of elapsed time.
//!
//! ```
//! use std::time::Duration;
//! use vieww_animation::inertia::{Inertia, Snap};
//!
//! // Released at x = 0 moving 1000 px/s; cards sit every 300 px.
//! let throw = Inertia::new(0.0, 1000.0).snap(Snap::Grid(300.0));
//! assert_eq!(throw.end(), 300.0);
//! let done = throw.position(throw.duration());
//! assert!((done - 300.0).abs() < 0.01);
//! ```

use std::time::Duration;

use crate::Simulation;

/// Recent `(seconds, value)` samples fitted by least squares.
#[derive(Debug, Clone, Default)]
pub struct VelocityTracker {
    samples: Vec<(f32, f32)>,
    window: f32,
}

impl VelocityTracker {
    /// Keeps samples from the last `window` seconds (GSAP uses ~0.1–0.2 s).
    #[must_use]
    pub fn new(window: f32) -> Self {
        Self {
            samples: Vec::new(),
            window: window.max(1e-3),
        }
    }

    pub fn add(&mut self, seconds: f32, value: f32) {
        self.samples.push((seconds, value));
        let cutoff = seconds - self.window;
        self.samples.retain(|(t, _)| *t >= cutoff);
    }

    /// The slope of the best-fit line (units per second); 0 with < 2 samples.
    #[must_use]
    pub fn velocity(&self) -> f32 {
        let n = self.samples.len();
        if n < 2 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        let nf = n as f32;
        let mt = self.samples.iter().map(|s| s.0).sum::<f32>() / nf;
        let mv = self.samples.iter().map(|s| s.1).sum::<f32>() / nf;
        let (mut num, mut den) = (0.0, 0.0);
        for (t, v) in &self.samples {
            num += (t - mt) * (v - mv);
            den += (t - mt) * (t - mt);
        }
        if den <= f32::EPSILON {
            0.0
        } else {
            num / den
        }
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }
}

/// Where a throw may end.
#[derive(Debug, Clone)]
pub enum Snap {
    /// Wherever friction leaves it.
    Free,
    /// The nearest multiple of the step.
    Grid(f32),
    /// The nearest listed value.
    Values(Vec<f32>),
    /// A function of the natural end.
    With(fn(f32) -> f32),
}

impl Snap {
    fn apply(&self, natural: f32) -> f32 {
        match self {
            Self::Free => natural,
            Self::Grid(step) if *step > 0.0 => (natural / step).round() * step,
            Self::Grid(_) => natural,
            Self::Values(v) => v
                .iter()
                .copied()
                .min_by(|a, b| (a - natural).abs().total_cmp(&(b - natural).abs()))
                .unwrap_or(natural),
            Self::With(f) => f(natural),
        }
    }
}

/// The glide after a release.
#[derive(Debug, Clone)]
pub struct Inertia {
    start: f32,
    velocity: f32,
    resistance: f32,
    snap: Snap,
    min: f32,
    max: f32,
    end: f32,
    rate: f32,
}

impl Inertia {
    /// Released at `start` moving `velocity` units/s, GSAP's default resistance.
    #[must_use]
    pub fn new(start: f32, velocity: f32) -> Self {
        let mut s = Self {
            start,
            velocity,
            resistance: 3.0,
            snap: Snap::Free,
            min: f32::NEG_INFINITY,
            max: f32::INFINITY,
            end: start,
            rate: 3.0,
        };
        s.solve();
        s
    }

    /// Decay rate per second: higher stops sooner.
    #[must_use]
    pub fn resistance(mut self, r: f32) -> Self {
        self.resistance = r.max(1e-3);
        self.solve();
        self
    }

    #[must_use]
    pub fn snap(mut self, snap: Snap) -> Self {
        self.snap = snap;
        self.solve();
        self
    }

    #[must_use]
    pub fn bounds(mut self, min: f32, max: f32) -> Self {
        self.min = min.min(max);
        self.max = max.max(min);
        self.solve();
        self
    }

    fn solve(&mut self) {
        // x(t) = start + (v / k)(1 − e^(−k t)); the natural end is start + v/k.
        let natural = self.start + self.velocity / self.resistance;
        self.end = self.snap.apply(natural).clamp(self.min, self.max);
        // Re-aim: keep the release speed's sign, choose k so the glide lands
        // exactly on `end`. With no travel there is nothing to re-aim.
        let travel = self.end - self.start;
        self.rate = if travel.abs() < 1e-6 || self.velocity.abs() < 1e-6 {
            self.resistance
        } else {
            (self.velocity / travel).abs().max(0.25)
        };
    }

    /// Where the throw comes to rest.
    #[must_use]
    pub const fn end(&self) -> f32 {
        self.end
    }

    /// Time to within 0.1% of the end (≈ 6.9 / rate).
    #[must_use]
    pub fn duration(&self) -> Duration {
        Duration::from_secs_f32(6.9 / self.rate)
    }

    #[must_use]
    pub fn position(&self, elapsed: Duration) -> f32 {
        let t = elapsed.as_secs_f32();
        let k = 1.0 - (-self.rate * t).exp();
        if t >= self.duration().as_secs_f32() {
            return self.end;
        }
        self.start + (self.end - self.start) * k
    }

    #[must_use]
    pub fn velocity_at(&self, elapsed: Duration) -> f32 {
        let t = elapsed.as_secs_f32();
        (self.end - self.start) * self.rate * (-self.rate * t).exp()
    }
}

impl Simulation for Inertia {
    fn position(&self, elapsed: Duration) -> f32 {
        Self::position(self, elapsed)
    }
    fn velocity_at(&self, elapsed: Duration) -> f32 {
        Self::velocity_at(self, elapsed)
    }
    fn duration(&self) -> Duration {
        Self::duration(self)
    }
    fn final_position(&self) -> f32 {
        self.end
    }
}

/// `physics2D`: a projectile with friction, as a closed form.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Physics2D {
    pub start: (f32, f32),
    /// Initial speed (units/s).
    pub velocity: f32,
    /// Launch angle in degrees, 0 = +x, 90 = +y (screen-down if y is down).
    pub angle: f32,
    /// Constant acceleration along `acceleration_angle`.
    pub acceleration: f32,
    pub acceleration_angle: f32,
    /// Added to y acceleration (GSAP `gravity`).
    pub gravity: f32,
    /// Linear drag coefficient per second (0 = none).
    pub friction: f32,
}

impl Physics2D {
    #[must_use]
    pub const fn new(start: (f32, f32), velocity: f32, angle: f32) -> Self {
        Self {
            start,
            velocity,
            angle,
            acceleration: 0.0,
            acceleration_angle: 0.0,
            gravity: 0.0,
            friction: 0.0,
        }
    }

    #[must_use]
    pub const fn gravity(mut self, g: f32) -> Self {
        self.gravity = g;
        self
    }

    #[must_use]
    pub const fn friction(mut self, f: f32) -> Self {
        self.friction = f;
        self
    }

    #[must_use]
    pub const fn acceleration(mut self, a: f32, angle: f32) -> Self {
        self.acceleration = a;
        self.acceleration_angle = angle;
        self
    }

    fn accel(&self) -> (f32, f32) {
        let r = self.acceleration_angle.to_radians();
        (
            self.acceleration * r.cos(),
            self.acceleration * r.sin() + self.gravity,
        )
    }

    /// Position at `t` seconds. With linear drag `c`:
    /// `x(t) = x0 + (v0 − a/c)(1 − e^(−ct))/c + a t / c`.
    #[must_use]
    pub fn position(&self, t: f32) -> (f32, f32) {
        let r = self.angle.to_radians();
        let (v0x, v0y) = (self.velocity * r.cos(), self.velocity * r.sin());
        let (ax, ay) = self.accel();
        let c = self.friction;
        let axis = |x0: f32, v0: f32, a: f32| {
            if c.abs() < 1e-6 {
                x0 + v0 * t + 0.5 * a * t * t
            } else {
                let e = 1.0 - (-c * t).exp();
                x0 + (v0 - a / c) * e / c + a * t / c
            }
        };
        (axis(self.start.0, v0x, ax), axis(self.start.1, v0y, ay))
    }

    /// Velocity at `t` seconds.
    #[must_use]
    pub fn velocity(&self, t: f32) -> (f32, f32) {
        let r = self.angle.to_radians();
        let (v0x, v0y) = (self.velocity * r.cos(), self.velocity * r.sin());
        let (ax, ay) = self.accel();
        let c = self.friction;
        let axis = |v0: f32, a: f32| {
            if c.abs() < 1e-6 {
                v0 + a * t
            } else {
                (v0 - a / c) * (-c * t).exp() + a / c
            }
        };
        (axis(v0x, ax), axis(v0y, ay))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracker_fits_a_line_and_ignores_one_outlier() {
        let mut v = VelocityTracker::new(0.2);
        for i in 0..10 {
            #[allow(clippy::cast_precision_loss)]
            let t = i as f32 * 0.016;
            v.add(t, 500.0 * t);
        }
        assert!((v.velocity() - 500.0).abs() < 1.0);
        v.add(0.16, 500.0 * 0.16 + 3.0);
        assert!((v.velocity() - 500.0).abs() < 60.0);
    }

    #[test]
    fn tracker_forgets_old_samples() {
        let mut v = VelocityTracker::new(0.1);
        v.add(0.0, 0.0);
        v.add(1.0, 100.0);
        v.add(1.05, 100.0);
        assert!(v.velocity().abs() < 1e-3);
    }

    #[test]
    fn free_throw_ends_at_v_over_k() {
        let t = Inertia::new(10.0, 300.0).resistance(3.0);
        assert!((t.end() - 110.0).abs() < 1e-4);
        assert!((t.position(t.duration()) - 110.0).abs() < 1e-4);
    }

    #[test]
    fn snap_to_values_lands_exactly() {
        let t = Inertia::new(0.0, 900.0).snap(Snap::Values(vec![0.0, 250.0, 500.0]));
        assert_eq!(t.end(), 250.0);
        let near = t.position(t.duration() - Duration::from_millis(1));
        assert!((near - 250.0).abs() < 1.0);
    }

    #[test]
    fn bounds_clamp_the_end() {
        let t = Inertia::new(0.0, 10_000.0).bounds(-50.0, 120.0);
        assert_eq!(t.end(), 120.0);
    }

    #[test]
    fn motion_is_monotonic_towards_the_end() {
        let t = Inertia::new(0.0, -800.0).snap(Snap::Grid(100.0));
        let mut prev = 0.0;
        for i in 1..200 {
            let p = t.position(Duration::from_millis(i * 10));
            assert!(p <= prev + 1e-4);
            prev = p;
        }
        assert_eq!(t.end(), -300.0);
    }

    #[test]
    fn with_snap_uses_the_function() {
        let t = Inertia::new(0.0, 100.0).snap(Snap::With(|x| x * 2.0));
        assert!((t.end() - 200.0 / 3.0).abs() < 1e-3);
    }

    #[test]
    fn projectile_matches_kinematics() {
        let p = Physics2D::new((0.0, 0.0), 100.0, 0.0).gravity(10.0);
        let (x, y) = p.position(2.0);
        assert!((x - 200.0).abs() < 1e-3);
        assert!((y - 20.0).abs() < 1e-3);
        let (_, vy) = p.velocity(2.0);
        assert!((vy - 20.0).abs() < 1e-3);
    }

    #[test]
    fn friction_reaches_terminal_velocity() {
        let p = Physics2D::new((0.0, 0.0), 0.0, 0.0)
            .gravity(10.0)
            .friction(2.0);
        let (_, vy) = p.velocity(20.0);
        assert!((vy - 5.0).abs() < 1e-3, "terminal = g / c");
        // Position is the integral of velocity.
        let (dt, t) = (1e-3, 1.3);
        let (_, y0) = p.position(t);
        let (_, y1) = p.position(t + dt);
        let (_, v) = p.velocity(t + dt / 2.0);
        assert!(((y1 - y0) / dt - v).abs() < 1e-2);
    }
}
