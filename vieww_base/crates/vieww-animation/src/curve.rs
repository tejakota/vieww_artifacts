//! Easing: the shape of the journey from 0 to 1.
//!
//! # Why a curve is not a nicety
//!
//! A linear animation is the one thing nothing in the physical world does. A
//! drawer that slides open at a constant speed and then stops dead reads as
//! *broken* rather than as fast, because nothing with mass moves like that. Every
//! platform's motion vocabulary — emphasised easing, the
//! `easeInOut` — is a cubic bézier for this reason, and matching those curves is
//! most of what makes an animation feel native rather than merely present.
//!
//! # Why this is hand-written rather than a crate
//!
//! `docs/ROADMAP.md` suggests the `keyframe` crate for the easing math. What is
//! actually needed here is one function — a unit cubic bézier solved for *y* at a
//! given *x* — and the interesting part of it is the root-find, which is thirty
//! lines and completely specified by the CSS `cubic-bezier()` definition. Taking a
//! dependency for that would buy a keyframe-sequence API this crate does not use
//! and would still leave [`Tween`](crate::Tween) to be written, since `keyframe`'s
//! interpolation does not know about [`Color`](vieww_foundation::Color) or
//! [`EdgeInsets`](vieww_foundation::EdgeInsets). See `docs/DESIGN.md` §15.

use std::fmt;

/// Iterations of Newton-Raphson before falling back to bisection.
///
/// Four is enough for a well-behaved curve to reach f32 precision; the fallback
/// exists for the flat spots where the derivative is near zero and Newton's
/// method steps off into nothing.
const NEWTON_STEPS: u32 = 4;
const NEWTON_MIN_SLOPE: f32 = 0.001;
const BISECTION_STEPS: u32 = 24;

/// How a value's progress maps onto its animation's progress.
///
/// A curve is a function on the unit interval with `f(0) == 0` and `f(1) == 1`.
/// Everything between is where the character lives.
#[derive(Debug, Clone, Copy, Default)]
pub enum Curve {
    /// Constant speed. Correct for a spinner or a colour cycle, wrong for
    /// anything that starts or stops on screen.
    #[default]
    Linear,
    /// A cubic bézier with its endpoints pinned at `(0, 0)` and `(1, 1)`, given
    /// by its two control points — the same parameterisation as CSS's
    /// `cubic-bezier()` and this crate's `Cubic`.
    Cubic { x1: f32, y1: f32, x2: f32, y2: f32 },
    /// Any easing at all, as a function — see [`custom`](Self::custom).
    ///
    /// A stepped curve, a bounce, an elastic overshoot or a table lookup, none
    /// of which a cubic bézier can express. Construct it through `custom`
    /// rather than by literal: `flipped` is bookkeeping this type owns.
    Custom {
        /// Maps progress to eased progress.
        f: fn(f32) -> f32,
        /// Read the function backwards — see [`flipped`](Self::flipped).
        ///
        /// **Why a flag and not a composed function.** `flipped` has to return a
        /// `Curve`, and the flip of `f` is `|t| 1 - f(1 - t)` — a closure, which
        /// is not a `fn` pointer. Composing would need an allocation, and being
        /// allocation-free is the property this enum exists to protect. One
        /// `bool` buys the same thing for nothing.
        flipped: bool,
    },
}

impl Curve {
    /// Slow to start, slow to stop. The default for anything that moves both
    /// into and out of view.
    pub const EASE_IN_OUT: Self = Self::cubic(0.42, 0.0, 0.58, 1.0);
    /// Slow to start. For something entering — it has to accelerate from rest.
    pub const EASE_IN: Self = Self::cubic(0.42, 0.0, 1.0, 1.0);
    /// Slow to stop. For something leaving, or arriving at a rest position.
    pub const EASE_OUT: Self = Self::cubic(0.0, 0.0, 0.58, 1.0);
    /// CSS's `ease`, which is not symmetric: it starts gently and finishes long.
    pub const EASE: Self = Self::cubic(0.25, 0.1, 0.25, 1.0);
    /// The standard emphasised easing — leaves quickly, arrives slowly.
    pub const FAST_OUT_SLOW_IN: Self = Self::cubic(0.4, 0.0, 0.2, 1.0);

    // ---------------------------------------------------------------------
    // The named library: GSAP's easing families (Power, Sine, Expo, Circ,
    // Back, Elastic, Bounce, Steps) — the curves every animation tool names
    // the same way, spelled here so "give me a bounce" is one identifier
    // rather than a cubic-bezier guess.
    //
    // Each family comes in `_IN`, `_OUT` and `_IN_OUT`, and each is built on
    // `custom` because none of them is a cubic bézier — a bounce and an
    // elastic overshoot leave the unit interval, and a bézier pinned to
    // `f(0)=0, f(1)=1` with monotone control x cannot. Where a family's out
    // really is the mirror of its in (the power family), the in is spelled
    // and the out is `.flipped()` — one function, two curves, no second copy
    // to drift. Where it is not (elastic, bounce), both are spelled.
    // ---------------------------------------------------------------------

    /// GSAP `power1` / CSS `quad` — squared progress. Gentle, and the
    /// softest curve that still reads as accelerating.
    pub const POWER1_IN: Self = Self::custom(power1_in);
    /// `power1` out: fast, then a long gentle landing.
    pub const POWER1_OUT: Self = Self::custom(power1_in).flipped();
    /// `power1` both ways.
    pub const POWER1_IN_OUT: Self = Self::custom(power1_in_out);

    /// GSAP `power2` / CSS `cubic` — cubed progress. The default "emphatic"
    /// family for content that should feel deliberate.
    pub const POWER2_IN: Self = Self::custom(power2_in);
    /// `power2` out.
    pub const POWER2_OUT: Self = Self::custom(power2_in).flipped();
    /// `power2` both ways.
    pub const POWER2_IN_OUT: Self = Self::custom(power2_in_out);

    /// GSAP `power3` / CSS `quart`.
    pub const POWER3_IN: Self = Self::custom(power3_in);
    /// `power3` out.
    pub const POWER3_OUT: Self = Self::custom(power3_in).flipped();
    /// `power3` both ways.
    pub const POWER3_IN_OUT: Self = Self::custom(power3_in_out);

    /// GSAP `power4` / CSS `quint` — the sharpest power curve that does not
    /// look broken; `power5`+ is GSAP's own advice to stop at.
    pub const POWER4_IN: Self = Self::custom(power4_in);
    /// `power4` out.
    pub const POWER4_OUT: Self = Self::custom(power4_in).flipped();
    /// `power4` both ways.
    pub const POWER4_IN_OUT: Self = Self::custom(power4_in_out);

    /// Sine — the softest in the library; the shape of a pendulum's speed at
    /// the bottom of its swing.
    pub const SINE_IN: Self = Self::custom(sine_in);
    /// Sine out.
    pub const SINE_OUT: Self = Self::custom(sine_in).flipped();
    /// Sine both ways — a breath, a pulse, anything organic.
    pub const SINE_IN_OUT: Self = Self::custom(sine_in_out);

    /// Exponential — starts at nothing, arrives at full speed. For exits that
    /// should feel like being pulled off stage.
    pub const EXPO_IN: Self = Self::custom(expo_in);
    /// Exponential out — the classic "reveal": instant momentum, long glide.
    pub const EXPO_OUT: Self = Self::custom(expo_in).flipped();
    /// Exponential both ways.
    pub const EXPO_IN_OUT: Self = Self::custom(expo_in_out);

    /// Circular — an arc rather than a ramp; snappier than `sine`, softer
    /// than `expo`.
    pub const CIRC_IN: Self = Self::custom(circ_in);
    /// Circular out.
    pub const CIRC_OUT: Self = Self::custom(circ_in).flipped();
    /// Circular both ways.
    pub const CIRC_IN_OUT: Self = Self::custom(circ_in_out);

    /// Back — overshoots, then settles back. The constant is Penner's
    /// 1.70158, tuned once and kept: changing it changes the *personality*,
    /// not the family.
    pub const BACK_IN: Self = Self::custom(back_in);
    /// Back out — overshoots on arrival, which is the direction a "pop"
    /// entrance wants.
    pub const BACK_OUT: Self = Self::custom(back_out);
    /// Back both ways.
    pub const BACK_IN_OUT: Self = Self::custom(back_in_out);

    /// Elastic — wobbles like a released spring. `ELASTIC_OUT` is the one a
    /// playful entrance wants; the others complete the family.
    pub const ELASTIC_IN: Self = Self::custom(elastic_in);
    /// Elastic out: the bounce-in of a slingshot.
    pub const ELASTIC_OUT: Self = Self::custom(elastic_out);
    /// Elastic both ways.
    pub const ELASTIC_IN_OUT: Self = Self::custom(elastic_in_out);

    /// Bounce — a ball landing: N diminishing parabolic hops.
    pub const BOUNCE_IN: Self = Self::custom(bounce_out).flipped();
    /// Bounce out — the standard direction: arrives by falling.
    pub const BOUNCE_OUT: Self = Self::custom(bounce_out);
    /// Bounce both ways.
    pub const BOUNCE_IN_OUT: Self = Self::custom(bounce_in_out);

    /// Two discrete jumps — GSAP `steps(2)`. A binary state, a half-filled
    /// tally.
    pub const STEPS_2: Self = Self::custom(steps_2);
    /// Four discrete jumps — `steps(4)`. A quartered progress bar.
    pub const STEPS_4: Self = Self::custom(steps_4);
    /// Five discrete jumps — `steps(5)`. A star filling point by point.
    pub const STEPS_5: Self = Self::custom(steps_5);
    /// Eight discrete jumps — `steps(8)`. A loading bar that ticks.
    pub const STEPS_8: Self = Self::custom(steps_8);

    /// A cubic bézier from its two control points.
    ///
    /// # Panics
    ///
    /// If either control point's *x* lies outside `0..=1`. Such a curve is not a
    /// function of *x* — it doubles back, so one progress value maps to several
    /// outputs and there is no answer to give. Const-evaluated for the
    /// associated constants above, so a bad one fails to compile.
    #[must_use]
    pub const fn cubic(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        assert!(
            x1 >= 0.0 && x1 <= 1.0 && x2 >= 0.0 && x2 <= 1.0,
            "a curve's control points must have x within 0..=1, or the curve \
             doubles back and is not a function of progress"
        );
        Self::Cubic { x1, y1, x2, y2 }
    }

    /// Any easing at all, from a function.
    ///
    /// For the curves a cubic bézier cannot express — a stepped curve, a bounce,
    /// an elastic overshoot, or anything sampled from a table.
    ///
    /// ```
    /// use vieww_animation::Curve;
    ///
    /// // Four discrete steps, which no bézier can do.
    /// let stepped = Curve::custom(|t: f32| (t * 4.0).floor() / 4.0);
    /// assert_eq!(stepped.transform(0.6), 0.5);
    /// assert_eq!(stepped.transform(1.0), 1.0);
    /// ```
    ///
    /// # The contract is yours to keep
    ///
    /// `f(0.0)` must be `0.0` and `f(1.0)` must be `1.0`. Neither is checked —
    /// checking would mean calling the function at construction, and a `const fn`
    /// cannot. A curve that breaks it makes an animation jump at one end.
    ///
    /// **It may leave the unit interval in between**, and that is the point of
    /// having it: an overshoot returning `1.2` is what a bounce *is*.
    /// [`Tween`](crate::Tween) extrapolates rather than clamping, so the
    /// overshoot survives all the way to the value.
    ///
    /// # Why `fn` and not a closure
    ///
    /// A bare function pointer keeps `Curve` `Copy`, comparable and
    /// allocation-free, which is why it is an enum rather than a trait object.
    /// The cost is that a curve cannot capture state — a table has to be a
    /// `static`, not a `Vec` — and that is the whole of what this does not cover.
    #[must_use]
    pub const fn custom(f: fn(f32) -> f32) -> Self {
        Self::Custom { f, flipped: false }
    }

    /// The eased value of `t`, which is clamped to `0..=1` first.
    ///
    /// Clamping rather than extrapolating: a bézier evaluated outside its
    /// interval says nothing useful, and the callers that legitimately go out of
    /// range — a spring overshooting its target — do not go through a curve at
    /// all. See [`AnimationController`](crate::AnimationController).
    #[must_use]
    pub fn transform(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::Cubic { x1, y1, x2, y2 } => {
                // The endpoints are exact by definition; the solver would land
                // within a rounding error of them and look like a bug.
                if t == 0.0 || t == 1.0 {
                    return t;
                }
                bezier(y1, y2, solve_x(t, x1, x2))
            }
            // No endpoint short-circuit and no clamp of the result. The
            // short-circuit exists above because the *solver* lands a rounding
            // error away from an exact endpoint; a function has no solver, and
            // hiding a wrong endpoint would hide the implementor's bug. The
            // result is unclamped because an overshoot is the reason to be here.
            Self::Custom { f, flipped } => {
                if flipped {
                    1.0 - f(1.0 - t)
                } else {
                    f(t)
                }
            }
        }
    }

    /// The same curve read backwards: `flipped().transform(t) == 1 - transform(1 - t)`.
    ///
    /// What a reversing animation wants. An ease-in played backwards is not an
    /// ease-in — it is an ease-out, and reusing the forward curve is why a
    /// dismissal so often looks subtly wrong next to the entrance it undoes.
    #[must_use]
    pub const fn flipped(self) -> Self {
        match self {
            Self::Linear => Self::Linear,
            Self::Cubic { x1, y1, x2, y2 } => Self::Cubic {
                x1: 1.0 - x2,
                y1: 1.0 - y2,
                x2: 1.0 - x1,
                y2: 1.0 - y1,
            },
            Self::Custom { f, flipped } => Self::Custom {
                f,
                flipped: !flipped,
            },
        }
    }
}

/// # Hand-written because a derive cannot compare a function pointer honestly
///
/// `#[derive(PartialEq)]` over [`Custom`](Self::Custom) compares `f` with `==`,
/// which rustc warns about (`unpredictable_function_pointer_comparisons`) and is
/// right to: the same function can have different addresses in different codegen
/// units, and distinct functions can share one after the linker merges
/// identical bodies.
///
/// [`std::ptr::fn_addr_eq`] is the sanctioned form of the same question. It
/// carries the same caveats — this is **identity, approximately** — and that is
/// all a `Curve` can offer, because comparing two easings by behaviour would
/// mean sampling them and two curves agreeing at every sample are still not the
/// same curve. `flipped` is compared normally: two references to one function,
/// read in opposite directions, are different curves.
impl PartialEq for Curve {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Linear, Self::Linear) => true,
            (
                Self::Cubic {
                    x1: ax1,
                    y1: ay1,
                    x2: ax2,
                    y2: ay2,
                },
                Self::Cubic {
                    x1: bx1,
                    y1: by1,
                    x2: bx2,
                    y2: by2,
                },
            ) => ax1 == bx1 && ay1 == by1 && ax2 == bx2 && ay2 == by2,
            (
                Self::Custom {
                    f: af,
                    flipped: aflipped,
                },
                Self::Custom {
                    f: bf,
                    flipped: bflipped,
                },
            ) => std::ptr::fn_addr_eq(*af, *bf) && aflipped == bflipped,
            _ => false,
        }
    }
}

impl fmt::Display for Curve {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Linear => f.write_str("linear"),
            Self::Cubic { x1, y1, x2, y2 } => {
                write!(f, "cubic({x1}, {y1}, {x2}, {y2})")
            }
            Self::Custom { flipped, .. } => {
                // Nothing useful to print about a function pointer, and its
                // address would make this output differ between runs.
                f.write_str(if *flipped {
                    "custom (flipped)"
                } else {
                    "custom"
                })
            }
        }
    }
}

/// One coordinate of a unit cubic bézier at parameter `s`.
///
/// The first and last control points are 0 and 1, so the usual four-term form
/// collapses to this.
fn bezier(a: f32, b: f32, s: f32) -> f32 {
    let inv = 1.0 - s;
    3.0 * inv * inv * s * a + 3.0 * inv * s * s * b + s * s * s
}

/// The derivative of [`bezier`] with respect to `s`.
fn bezier_slope(a: f32, b: f32, s: f32) -> f32 {
    let inv = 1.0 - s;
    3.0 * inv * inv * a + 6.0 * inv * s * (b - a) + 3.0 * s * s * (1.0 - b)
}

/// The bézier parameter at which the curve's *x* equals `x`.
///
/// A bézier is parameterised by `s`, not by *x*, and the two are only equal for
/// a straight line — so asking "what is *y* when progress is `t`" means solving
/// for `s` first. Newton-Raphson converges in a few steps wherever the curve is
/// steep enough to have a direction; bisection finishes the flat spots, where
/// Newton would divide by nearly zero and leave the interval entirely.
fn solve_x(x: f32, x1: f32, x2: f32) -> f32 {
    /// Close enough that the difference is invisible at any pixel density.
    const TOLERANCE: f32 = 1e-6;

    let mut guess = x;
    for _ in 0..NEWTON_STEPS {
        let error = bezier(x1, x2, guess) - x;
        if error.abs() < TOLERANCE {
            return guess;
        }
        let slope = bezier_slope(x1, x2, guess);
        if slope.abs() < NEWTON_MIN_SLOPE {
            break;
        }
        guess -= error / slope;
        if !(0.0..=1.0).contains(&guess) {
            break;
        }
    }
    if (0.0..=1.0).contains(&guess) && (bezier(x1, x2, guess) - x).abs() < TOLERANCE {
        return guess;
    }

    // Newton stalled on a flat spot or stepped out of the interval. `x` is
    // monotonic in `s` for any curve `Curve::cubic` accepts, so halving the
    // interval always converges — just more slowly.
    let (mut low, mut high) = (0.0_f32, 1.0_f32);
    let mut guess = x;
    for _ in 0..BISECTION_STEPS {
        let value = bezier(x1, x2, guess);
        if (value - x).abs() < TOLERANCE {
            break;
        }
        if value < x {
            low = guess;
        } else {
            high = guess;
        }
        guess = (low + high) / 2.0;
    }
    guess
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Samples of a curve across the unit interval, endpoints included.
    fn samples(curve: Curve, count: u32) -> Vec<f32> {
        (0..=count)
            .map(|step| curve.transform(step as f32 / count as f32))
            .collect()
    }

    #[test]
    fn every_curve_starts_at_zero_and_ends_at_one() {
        for curve in [
            Curve::Linear,
            Curve::EASE,
            Curve::EASE_IN,
            Curve::EASE_OUT,
            Curve::EASE_IN_OUT,
            Curve::FAST_OUT_SLOW_IN,
        ] {
            assert_eq!(curve.transform(0.0), 0.0, "{curve}");
            assert_eq!(curve.transform(1.0), 1.0, "{curve}");
        }
    }

    #[test]
    fn every_curve_is_monotonic_so_an_animation_never_backs_up() {
        for curve in [
            Curve::EASE,
            Curve::EASE_IN,
            Curve::EASE_OUT,
            Curve::EASE_IN_OUT,
            Curve::FAST_OUT_SLOW_IN,
        ] {
            let mut previous = 0.0;
            for value in samples(curve, 200) {
                assert!(
                    value >= previous - 1e-4,
                    "{curve} went backwards: {previous} then {value}"
                );
                previous = value;
            }
        }
    }

    #[test]
    fn progress_outside_the_interval_is_clamped_rather_than_extrapolated() {
        assert_eq!(Curve::EASE_IN_OUT.transform(-2.0), 0.0);
        assert_eq!(Curve::EASE_IN_OUT.transform(4.0), 1.0);
        assert_eq!(Curve::Linear.transform(-0.5), 0.0);
    }

    /// Four discrete steps — the simplest thing a bézier cannot be.
    ///
    /// CSS's `steps(4, end)`: dividing by the step count rather than by one less
    /// is what lands `f(1.0)` on exactly `1.0`.
    fn stepped(t: f32) -> f32 {
        (t * 4.0).floor() / 4.0
    }

    /// Overshoots one before coming back, which is what a bounce *is*.
    fn overshooting(t: f32) -> f32 {
        if t >= 1.0 {
            1.0
        } else {
            1.0 - (1.0 - t).powi(2) * (1.0 - 3.0 * t)
        }
    }

    #[test]
    fn a_custom_curve_can_be_something_no_bezier_can() {
        // A bézier is continuous and a function of one solve; a step is neither.
        let curve = Curve::custom(stepped);
        assert_eq!(curve.transform(0.0), 0.0);
        assert_eq!(
            curve.transform(0.6),
            0.5,
            "held at the step rather than tracking progress"
        );
        assert_eq!(curve.transform(1.0), 1.0);
    }

    #[test]
    fn a_custom_curve_may_leave_the_unit_interval() {
        // **The reason this variant exists.** `transform` clamps its input and
        // must not clamp its output: an overshoot returning more than one is a
        // bounce, and clamping it here would flatten exactly the curves a bézier
        // could not express in the first place.
        let peak = samples(Curve::custom(overshooting), 200)
            .into_iter()
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(peak > 1.0, "the overshoot survived: {peak}");
    }

    #[test]
    fn a_flipped_custom_curve_is_the_mirror_of_the_original() {
        // The property `flipped` promises, and the one a `bool` in the variant
        // exists to keep: the flip of `f` is `1 - f(1 - t)`, which is a closure
        // and so cannot be a `fn` pointer.
        let curve = Curve::custom(stepped);
        let flipped = curve.flipped();
        for i in 0..=20 {
            let t = i as f32 / 20.0;
            let expected = 1.0 - curve.transform(1.0 - t);
            assert!(
                (flipped.transform(t) - expected).abs() < 1e-6,
                "at {t}: {} vs {expected}",
                flipped.transform(t)
            );
        }
    }

    #[test]
    fn flipping_a_custom_curve_twice_returns_the_original() {
        let curve = Curve::custom(stepped);
        assert_eq!(curve.flipped().flipped(), curve);
    }

    #[test]
    fn a_custom_curve_still_starts_at_zero_and_ends_at_one() {
        // Not enforced at construction — a `const fn` cannot call its argument —
        // so it is the implementor's contract. Both shipped examples keep it.
        for curve in [Curve::custom(stepped), Curve::custom(overshooting)] {
            assert_eq!(curve.transform(0.0), 0.0, "{curve}");
            assert_eq!(curve.transform(1.0), 1.0, "{curve}");
            assert_eq!(curve.flipped().transform(0.0), 0.0, "{curve} flipped");
            assert_eq!(curve.flipped().transform(1.0), 1.0, "{curve} flipped");
        }
    }

    #[test]
    fn easing_in_starts_slower_than_linear_and_catches_up() {
        let quarter = Curve::EASE_IN.transform(0.25);
        assert!(
            quarter < 0.25 * 0.6,
            "an ease-in must visibly hold back at the start, not merely differ \
             from linear: {quarter}"
        );
        assert!(Curve::EASE_IN.transform(0.9) > 0.8, "and then arrive");
    }

    #[test]
    fn easing_out_leaves_immediately_and_arrives_slowly() {
        let quarter = Curve::EASE_OUT.transform(0.25);
        assert!(
            quarter > 0.35,
            "an ease-out must already be well ahead of linear a quarter of the \
             way through: {quarter}"
        );
        let last_tenth = 1.0 - Curve::EASE_OUT.transform(0.9);
        assert!(
            last_tenth < 0.05,
            "the last tenth of the time must cover very little ground: {last_tenth}"
        );
    }

    #[test]
    fn ease_in_out_is_symmetric_about_its_midpoint() {
        for step in 0..=50 {
            let t = step as f32 / 50.0;
            let forward = Curve::EASE_IN_OUT.transform(t);
            let backward = Curve::EASE_IN_OUT.transform(1.0 - t);
            assert!(
                (forward + backward - 1.0).abs() < 1e-3,
                "an ease-in-out that is not symmetric decelerates differently \
                 than it accelerated: f({t})={forward}, f({})={backward}",
                1.0 - t
            );
        }
        assert!((Curve::EASE_IN_OUT.transform(0.5) - 0.5).abs() < 1e-3);
    }

    #[test]
    fn flipping_a_curve_plays_it_backwards() {
        let curve = Curve::EASE_IN;
        let flipped = curve.flipped();
        for step in 0..=20 {
            let t = step as f32 / 20.0;
            assert!(
                (flipped.transform(t) - (1.0 - curve.transform(1.0 - t))).abs() < 1e-3,
                "at {t}: {} vs {}",
                flipped.transform(t),
                1.0 - curve.transform(1.0 - t)
            );
        }
        assert!(
            (Curve::EASE_IN.flipped().transform(0.3) - Curve::EASE_OUT.transform(0.3)).abs() < 1e-6,
            "an ease-in read backwards is an ease-out"
        );
        assert_eq!(Curve::Linear.flipped(), Curve::Linear);
    }

    #[test]
    fn the_solver_inverts_x_rather_than_treating_the_parameter_as_time() {
        // A curve whose control points are far from the diagonal is where
        // "y at parameter t" and "y at progress t" differ most: reading the
        // parameter directly would give 0.5 here, and the answer is not 0.5.
        let curve = Curve::cubic(1.0, 0.0, 0.0, 1.0);
        let mid = curve.transform(0.5);
        assert!(
            (mid - 0.5).abs() < 1e-3,
            "this curve is antisymmetric, so its midpoint is 0.5: {mid}"
        );
        let quarter = curve.transform(0.25);
        assert!(
            quarter < 0.1,
            "and it must hold hard at the start: {quarter}"
        );
    }

    #[test]
    fn a_curve_with_a_flat_spot_still_resolves() {
        // Zero slope at both ends: Newton's method divides by nearly nothing
        // here, and without the bisection fallback this returns garbage.
        let curve = Curve::cubic(1.0, 0.0, 0.0, 1.0);
        for value in samples(curve, 100) {
            assert!(value.is_finite() && (0.0..=1.0).contains(&value), "{value}");
        }
    }

    #[test]
    #[should_panic(expected = "control points")]
    fn a_curve_that_doubles_back_is_rejected() {
        let _ = Curve::cubic(1.5, 0.0, 0.2, 1.0);
    }

    #[test]
    fn the_default_curve_is_linear() {
        assert_eq!(Curve::default(), Curve::Linear);
    }
}

// ---------------------------------------------------------------------
// The named-library functions behind the constants above.
//
// These are Penner's equations — the 2002 set that GSAP, CSS, jQuery and
// every other easing library derive from, which is why the shapes here will
// match what a designer already knows by name. The `in` of each family is
// spelled; where the out is the mirror (powers, sine, expo, circ), the
// constant uses `.flipped()` instead of a second function.
// ---------------------------------------------------------------------

/// The `in` half of an in-out curve, scaled — the standard construction for
/// every symmetric family: run the in at double speed over the first half,
/// and its mirror over the second.
fn in_out_of(in_fn: fn(f32) -> f32, t: f32) -> f32 {
    if t < 0.5 {
        in_fn(2.0 * t) / 2.0
    } else {
        (2.0 - in_fn(2.0 - 2.0 * t)) / 2.0
    }
}

fn power1_in(t: f32) -> f32 {
    t * t
}
fn power1_in_out(t: f32) -> f32 {
    in_out_of(power1_in, t)
}

fn power2_in(t: f32) -> f32 {
    t * t * t
}
fn power2_in_out(t: f32) -> f32 {
    in_out_of(power2_in, t)
}

fn power3_in(t: f32) -> f32 {
    t * t * t * t
}
fn power3_in_out(t: f32) -> f32 {
    in_out_of(power3_in, t)
}

fn power4_in(t: f32) -> f32 {
    t * t * t * t * t
}
fn power4_in_out(t: f32) -> f32 {
    in_out_of(power4_in, t)
}

fn sine_in(t: f32) -> f32 {
    1.0 - (t * std::f32::consts::FRAC_PI_2).cos()
}
fn sine_in_out(t: f32) -> f32 {
    (1.0 - (std::f32::consts::PI * t).cos()) / 2.0
}

fn expo_in(t: f32) -> f32 {
    // t = 0 exactly: 2^-10 is 0.0009765, not 0, and the jump from nothing to
    // something in one frame is what an expo *is* — but t = 0 is the value
    // every animation starts from, and 0.001 there is a visible snap. The
    // endpoint is exact by special case, the way easings.net spells it.
    if t <= 0.0 {
        0.0
    } else {
        2.0_f32.powf(10.0 * (t - 1.0))
    }
}
fn expo_in_out(t: f32) -> f32 {
    if t <= 0.0 {
        0.0
    } else if t >= 1.0 {
        1.0
    } else if t < 0.5 {
        2.0_f32.powf(20.0 * t - 10.0) / 2.0
    } else {
        (2.0 - 2.0_f32.powf(-20.0 * t + 10.0)) / 2.0
    }
}

fn circ_in(t: f32) -> f32 {
    1.0 - (1.0 - t * t).sqrt()
}
fn circ_in_out(t: f32) -> f32 {
    if t < 0.5 {
        (1.0 - (1.0 - 4.0 * t * t).sqrt()) / 2.0
    } else {
        let u = -2.0 * t + 2.0;
        ((1.0 - u * u).sqrt() + 1.0) / 2.0
    }
}

/// Penner's back constant — the overshoot amount, tuned by hand in 2002 and
/// copied by every library since.
const BACK_K: f32 = 1.70158;

fn back_in(t: f32) -> f32 {
    let c3 = BACK_K + 1.0;
    c3 * t * t * t - BACK_K * t * t
}
fn back_out(t: f32) -> f32 {
    let c3 = BACK_K + 1.0;
    let u = t - 1.0;
    1.0 + c3 * u * u * u + BACK_K * u * u
}
fn back_in_out(t: f32) -> f32 {
    // Penner's in-out uses a larger constant (k·1.525) because the halves run
    // at double speed and would otherwise overshoot half as far each.
    let c2 = BACK_K * 1.525;
    if t < 0.5 {
        let u = 2.0 * t;
        (u * u * ((c2 + 1.0) * u - c2)) / 2.0
    } else {
        let u = 2.0 * t - 2.0;
        (u * u * ((c2 + 1.0) * u + c2)) / 2.0 + 1.0
    }
}

/// The elastic period constant — `(2π)/3`, the frequency Penner chose: fast
/// enough to read as a wobble, slow enough that the first swing is visible.
const ELASTIC_C: f32 = 2.0 * std::f32::consts::FRAC_PI_3;

fn elastic_in(t: f32) -> f32 {
    if t <= 0.0 {
        0.0
    } else if t >= 1.0 {
        1.0
    } else {
        -(2.0_f32.powf(10.0 * t - 10.0) * ((10.0 * t - 10.75) * ELASTIC_C).sin())
    }
}
fn elastic_out(t: f32) -> f32 {
    if t <= 0.0 {
        0.0
    } else if t >= 1.0 {
        1.0
    } else {
        2.0_f32.powf(-10.0 * t) * ((10.0 * t - 0.75) * ELASTIC_C).sin() + 1.0
    }
}
fn elastic_in_out(t: f32) -> f32 {
    // Built with the same `in_out_of` construction as every other symmetric
    // family — the halves run at double speed and mirror — rather than
    // Penner's separately-constant form: one construction guarantees the
    // in-out symmetry the test checks, where hand-tuned phase constants
    // have to be *checked* for it.
    if t <= 0.0 {
        0.0
    } else if t >= 1.0 {
        1.0
    } else {
        in_out_of(elastic_in, t)
    }
}

/// Penner's bounce: four parabolic arcs of geometrically shrinking width,
/// landing at 0.75, 0.9375, 0.984375 — each hop a quarter of the height of
/// the one before it.
fn bounce_out(t: f32) -> f32 {
    const N1: f32 = 7.5625;
    const D1: f32 = 2.75;
    if t < 1.0 / D1 {
        N1 * t * t
    } else if t < 2.0 / D1 {
        let u = t - 1.5 / D1;
        N1 * u * u + 0.75
    } else if t < 2.5 / D1 {
        let u = t - 2.25 / D1;
        N1 * u * u + 0.9375
    } else {
        let u = t - 2.625 / D1;
        N1 * u * u + 0.984375
    }
}
fn bounce_in_out(t: f32) -> f32 {
    if t < 0.5 {
        (1.0 - bounce_out(1.0 - 2.0 * t)) / 2.0
    } else {
        (bounce_out(2.0 * t - 1.0) + 1.0) / 2.0
    }
}

/// `n` discrete equal jumps, holding between — a closure can't become a `fn`
/// pointer, so each published count is its own named function. `min(n)` so
/// the last step's top edge lands on exactly 1 rather than 1 − ε.
fn step_of(n: f32) -> impl Fn(f32) -> f32 {
    move |t: f32| (t * n).floor().min(n) / n
}
fn steps_2(t: f32) -> f32 {
    step_of(2.0)(t)
}
fn steps_4(t: f32) -> f32 {
    step_of(4.0)(t)
}
fn steps_5(t: f32) -> f32 {
    step_of(5.0)(t)
}
fn steps_8(t: f32) -> f32 {
    step_of(8.0)(t)
}

#[cfg(test)]
mod preset_tests {
    use super::Curve;

    const EPS: f32 = 1e-4;

    #[test]
    fn every_preset_lands_on_both_endpoints() {
        // The contract `custom` documents but cannot check: f(0) = 0 and
        // f(1) = 1. A preset is this crate's own code, so here it *is*
        // checked — and the flipped variants need their mirror checked too,
        // because a flipped curve that was already landing on 1 at t = 1
        // lands on 0 there instead, and no caller can tell which end broke.
        let all = [
            Curve::POWER1_IN,
            Curve::POWER1_OUT,
            Curve::POWER1_IN_OUT,
            Curve::POWER2_IN,
            Curve::POWER2_OUT,
            Curve::POWER2_IN_OUT,
            Curve::POWER3_IN,
            Curve::POWER3_OUT,
            Curve::POWER3_IN_OUT,
            Curve::POWER4_IN,
            Curve::POWER4_OUT,
            Curve::POWER4_IN_OUT,
            Curve::SINE_IN,
            Curve::SINE_OUT,
            Curve::SINE_IN_OUT,
            Curve::EXPO_IN,
            Curve::EXPO_OUT,
            Curve::EXPO_IN_OUT,
            Curve::CIRC_IN,
            Curve::CIRC_OUT,
            Curve::CIRC_IN_OUT,
            Curve::BACK_IN,
            Curve::BACK_OUT,
            Curve::BACK_IN_OUT,
            Curve::ELASTIC_IN,
            Curve::ELASTIC_OUT,
            Curve::ELASTIC_IN_OUT,
            Curve::BOUNCE_IN,
            Curve::BOUNCE_OUT,
            Curve::BOUNCE_IN_OUT,
            Curve::STEPS_2,
            Curve::STEPS_4,
            Curve::STEPS_5,
            Curve::STEPS_8,
        ];
        for curve in all {
            assert!(
                curve.transform(0.0).abs() < EPS,
                "{curve:?} does not start at 0"
            );
            assert!(
                (curve.transform(1.0) - 1.0).abs() < EPS,
                "{curve:?} does not finish at 1"
            );
        }
    }

    #[test]
    fn in_and_out_sum_to_one_where_the_family_is_symmetric() {
        // The mirror relation: an ease-out at (1 − t) is 1 − the ease-in at
        // t. Checking the *sum* is checking the mirror; checking equality
        // would be checking that the curve is its own flip, which none of
        // these are.
        for family in [Curve::POWER2_IN, Curve::SINE_IN, Curve::EXPO_IN, Curve::CIRC_IN] {
            let out = family.flipped();
            for i in 1..10 {
                let t = i as f32 / 10.0;
                let a = family.transform(t);
                let b = out.transform(1.0 - t);
                assert!((a + b - 1.0).abs() < EPS, "{family:?} and its flip disagree at {t}");
            }
        }
    }

    #[test]
    fn back_and_elastic_leave_the_interval() {
        // The reason these exist as `custom` curves: they overshoot. A
        // bezier could not, and a curve that never leaves 0..=1 cannot be a
        // back or an elastic.
        let mut back_max: f32 = 0.0;
        for i in 0..=100 {
            let t = i as f32 / 100.0;
            back_max = back_max.max(Curve::BACK_OUT.transform(t));
        }
        assert!(back_max > 1.0, "BACK_OUT never overshot: {back_max}");
        let mut elastic_min: f32 = 0.0;
        for i in 0..=100 {
            let t = i as f32 / 100.0;
            elastic_min = elastic_min.min(Curve::ELASTIC_IN.transform(t));
        }
        assert!(elastic_min < 0.0, "ELASTIC_IN never undershot: {elastic_min}");
    }

    #[test]
    fn bounce_out_is_penneres_quarter_landing() {
        // The troughs of the classic bounce — the instants the parabolic
        // arcs bottom out — at 1.5/d, 2.25/d and 2.625/d for d = 2.75:
        // the values Penner's constants produce by construction.
        const D: f32 = 2.75;
        assert!((Curve::BOUNCE_OUT.transform(1.5 / D) - 0.75).abs() < 1e-3);
        assert!((Curve::BOUNCE_OUT.transform(2.25 / D) - 0.9375).abs() < 1e-3);
        assert!((Curve::BOUNCE_OUT.transform(2.625 / D) - 0.984375).abs() < 1e-3);
        assert!((Curve::BOUNCE_OUT.transform(1.0) - 1.0).abs() < EPS);
    }

    #[test]
    fn steps_jump_and_hold() {
        let four = Curve::STEPS_4;
        // Holds *at* the step value across each quarter, jumps at the edge.
        assert_eq!(four.transform(0.24), 0.0);
        assert_eq!(four.transform(0.26), 0.25);
        assert_eq!(four.transform(0.49), 0.25);
        assert_eq!(four.transform(0.51), 0.5);
        assert_eq!(four.transform(0.99), 0.75);
        assert_eq!(four.transform(1.0), 1.0);
    }

    #[test]
    fn in_out_symmetry() {
        // An in-out curve is mirror-symmetric about its centre: the value at
        // t and at 1 − t add to 1. Checked on every family, because it is
        // the property that makes "in-out" mean one thing rather than
        // "roughly in then roughly out".
        for curve in [
            Curve::POWER2_IN_OUT,
            Curve::SINE_IN_OUT,
            Curve::EXPO_IN_OUT,
            Curve::CIRC_IN_OUT,
            Curve::BACK_IN_OUT,
            Curve::ELASTIC_IN_OUT,
            Curve::BOUNCE_IN_OUT,
        ] {
            for i in 1..10 {
                let t = i as f32 / 10.0;
                let a = curve.transform(t);
                let b = curve.transform(1.0 - t);
                assert!(
                    (a + b - 1.0).abs() < 1e-3,
                    "{curve:?} not symmetric at {t}: {a} + {b}"
                );
            }
        }
    }

    #[test]
    fn monotone_families_actually_monotone() {
        // The families that promise monotone progress must deliver it — a
        // non-monotone "power" would be an elastic with the personality
        // filed off.
        for curve in [
            Curve::POWER1_IN,
            Curve::POWER2_IN,
            Curve::POWER3_IN,
            Curve::POWER4_IN,
            Curve::POWER2_OUT,
            Curve::SINE_IN,
            Curve::EXPO_IN,
            Curve::CIRC_IN,
            Curve::STEPS_8,
        ] {
            let mut last = curve.transform(0.0);
            for i in 1..=200 {
                let t = i as f32 / 200.0;
                let now = curve.transform(t);
                assert!(
                    now >= last - 1e-6,
                    "{curve:?} went backwards at {t}: {last} -> {now}"
                );
                last = now;
            }
        }
    }
}
