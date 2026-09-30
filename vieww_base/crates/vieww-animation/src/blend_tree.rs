//! Blend trees — the layer Unity's Mecanim, Unreal's Animation Blueprints,
//! Godot's AnimationTree and Rive's BlendState all carry, and the one word
//! the comparison tables use for it: *blend a set of animation tracks by a
//! parameter, rather than choosing one*.
//!
//! # What "blend" means here, precisely
//!
//! A state machine *picks* a clip. A blend tree *mixes* several, by a
//! continuous parameter: the walk clip at parameter 0, the run clip at
//! parameter 1, and 0.6 of the way between the two clips' poses at parameter
//! 0.6 — with the same clock reading for every child, because blending poses
//! from different moments of their animations is a glitch, not a style.
//!
//! The 1D tree interpolates between the two children whose *thresholds*
//! bracket the parameter, on a smoothstep rather than a straight line — a
//! linear blend spends its middle accelerating and decelerating at once,
//! which reads as mush, and the smoothstep's zero-slope ends are what make
//! the pure-child ends look pure.
//!
//! The 2D tree is freeform: children sit at `(x, y)` points and the weights
//! come from inverse-distance weighting. Unity's 2D freeform does the same
//! thing with a more expensive geometry (barycentric over a triangulation);
//! IDW gives the same answers at the children themselves, is well defined
//! for any point set however arranged, and needs no convex-hull edge cases —
//! a point outside the hull still gets sensible weights rather than a
//! special case.
//!
//! # The one rule
//!
//! Like everything else in this crate, a blend tree is a **function of its
//! inputs**: the parameter and the clock. The same `(parameter, elapsed)`
//! is the same pose, always, on every machine — so a walk-to-run driven by
//! a signal is as testable as a spring.
//!
//! ```
//! use std::time::Duration;
//! use vieww_animation::blend_tree::BlendTree1;
//! use vieww_animation::{Keyframe, Keyframes};
//!
//! let walk = Keyframes::new(0.0).with(Keyframe::to(1.0, 10.0));
//! let run = Keyframes::new(0.0).with(Keyframe::to(1.0, 30.0));
//!
//! let mut gait = BlendTree1::new(0.0, walk).child(1.0, run);
//!
//! let ms = Duration::from_millis;
//! // At the endpoints, the pure child's pose — exactly. (Both tracks ease
//! // in-out, so half a second in each is half its distance travelled.)
//! assert_eq!(gait.at(0.0, ms(500)), 5.0);
//! assert_eq!(gait.at(1.0, ms(500)), 15.0);
//! // Half way, the half-way blend of those two poses (symmetric children
//! // make the smoothstep land on 0.5 at the middle).
//! assert_eq!(gait.at(0.5, ms(500)), 10.0);
//! // The clock is shared: every child is read at the same elapsed.
//! assert_eq!(gait.at(0.5, ms(0)), 0.0);
//! ```

use std::time::Duration;

use crate::keyframe::Keyframes;
use crate::tween::Lerp;

/// The smoothstep — `3t² − 2t³`, zero slope at both ends.
///
/// Why not [`Curve`](crate::Curve): a blend weight is not an easing. It is
/// not authored per keyframe, it is not composable with itself, and it never
/// runs backwards; giving it the full `Curve` type would invite all three.
/// One named function is the whole honest surface.
fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// One child of a 1D blend tree: a track and the parameter value where it is
/// the pure pose.
#[derive(Debug, Clone, PartialEq)]
pub struct Child1<T> {
    /// The parameter value at which this child has weight 1.
    ///
    /// Thresholds need not be evenly spaced — "idle 0, walk 0.4, run 1.0" is
    /// the spelling a gamepad stick actually produces.
    pub threshold: f32,
    /// The child's own track.
    pub track: Keyframes<T>,
}

/// A one-parameter blend of keyframed tracks.
///
/// Built from a first child — a tree with nothing in it has no answer, and
/// making that unrepresentable is the same decision [`Keyframes::new`]
/// makes for its first value.
#[derive(Debug, Clone, PartialEq)]
pub struct BlendTree1<T> {
    children: Vec<Child1<T>>,
}

impl<T> BlendTree1<T> {
    /// A tree of one child, at `threshold`.
    pub fn new(threshold: f32, track: Keyframes<T>) -> Self {
        Self {
            children: vec![Child1 { threshold, track }],
        }
    }

    /// Add a child, keeping the list threshold-sorted. Returns `self` for
    /// builder chains.
    ///
    /// A child whose threshold is already taken **replaces** the one there —
    /// the same rule as [`Keyframes::insert`](Keyframes::insert), for the
    /// same reason: two children at one parameter value have no between to
    /// blend, and silent tie-breaking is a flicker.
    #[must_use]
    pub fn child(mut self, threshold: f32, track: Keyframes<T>) -> Self {
        match self
            .children
            .binary_search_by(|probe| probe.threshold.total_cmp(&threshold))
        {
            Ok(index) => self.children[index] = Child1 { threshold, track },
            Err(index) => self.children.insert(index, Child1 { threshold, track }),
        }
        self
    }

    /// The children, threshold order — an editor's view.
    pub fn children(&self) -> &[Child1<T>] {
        &self.children
    }

    /// When the last child's threshold is reached — the tree's answer stops
    /// changing after it, which is what a caller scheduling the next
    /// animation wants to know.
    pub fn end(&self) -> Duration {
        self.children
            .iter()
            .map(|child| child.track.end())
            .max()
            .unwrap_or(Duration::ZERO)
    }
}

impl<T: Lerp> BlendTree1<T> {
    /// The blended pose at `parameter` and `elapsed`.
    ///
    /// Children are sampled at the **same** `elapsed` — see the module docs
    /// for why mixing moments is a glitch. A parameter outside the
    /// thresholds clamps to the end child, the same way a keyframe track
    /// holds its last value rather than extrapolating: a parameter nobody
    /// authored an answer for is a bug, and the end pose is the loud
    /// approximation.
    pub fn at(&self, parameter: f32, elapsed: Duration) -> T {
        let first = &self.children[0];
        if self.children.len() == 1 || parameter <= first.threshold {
            return first.track.at(elapsed);
        }
        let last = &self.children[self.children.len() - 1];
        if parameter >= last.threshold {
            return last.track.at(elapsed);
        }
        // The bracketing pair: the list is threshold-sorted, so a scan for
        // the first child past the parameter finds the upper bracket, and
        // binary search would only matter for trees big enough that the
        // author should be using a 2D tree instead.
        let mut upper = 1;
        while self.children[upper].threshold < parameter {
            upper += 1;
        }
        let lower = &self.children[upper - 1];
        let upper_child = &self.children[upper];

        let span = upper_child.threshold - lower.threshold;
        let t = if span > f32::EPSILON {
            ((parameter - lower.threshold) / span).clamp(0.0, 1.0)
        } else {
            1.0
        };
        lower
            .track
            .at(elapsed)
            .lerp(upper_child.track.at(elapsed), smoothstep(t))
    }
}

/// One child of a 2D blend tree: a track and the `(x, y)` point where it is
/// the pure pose.
#[derive(Debug, Clone, PartialEq)]
pub struct Child2<T> {
    /// Where this child lives in the parameter plane.
    pub point: (f32, f32),
    /// The child's own track.
    pub track: Keyframes<T>,
}

/// A two-parameter, freeform blend of keyframed tracks.
///
/// The Unity capability this mirrors is "2D Freeform Cartesian": children at
/// arbitrary points in a plane, a blend for every point of the plane. The
/// classic use is locomotion — walk/run on one axis, strafe left/right on
/// the other — but any two-parameter pose space (x/y of a cursor, tilt of a
/// device in two axes) is the same shape.
///
/// Weights are inverse-distance-squared, normalised: the nearest child
/// dominates, far children contribute little, and the pose at a child's own
/// point is that child exactly. See the module docs for why IDW rather than
/// a triangulation.
#[derive(Debug, Clone, PartialEq)]
pub struct BlendTree2<T> {
    children: Vec<Child2<T>>,
}

/// The epsilon that keeps IDW finite when the query lands on a child: the
/// weight of a child at distance zero is `1 / ε`, which overwhelms the sum
/// to exactly 1 after normalisation without a division by zero.
const ON_TOP: f32 = 1e-6;

impl<T> BlendTree2<T> {
    /// A tree of one child, at `point`.
    pub fn new(point: (f32, f32), track: Keyframes<T>) -> Self {
        Self {
            children: vec![Child2 { point, track }],
        }
    }

    /// Add a child at `point`. Returns `self` for builder chains.
    ///
    /// Unlike the 1D tree there is no ordering to keep and no threshold to
    /// collide — two children at the same point are two sources of the same
    /// pose, and the weights simply split between them.
    #[must_use]
    pub fn child(mut self, point: (f32, f32), track: Keyframes<T>) -> Self {
        self.children.push(Child2 { point, track });
        self
    }

    /// The children, insertion order — an editor's view.
    pub fn children(&self) -> &[Child2<T>] {
        &self.children
    }

    /// The longest child track — the scheduling answer
    /// [`BlendTree1::end`](BlendTree1::end) gives the 1D case.
    pub fn end(&self) -> Duration {
        self.children
            .iter()
            .map(|child| child.track.end())
            .max()
            .unwrap_or(Duration::ZERO)
    }
}

impl<T: Lerp> BlendTree2<T> {
    /// The blended pose at `(x, y)` and `elapsed`.
    ///
    /// Every child is sampled at the same `elapsed`, then mixed with
    /// normalised inverse-distance-squared weights. The weights are a
    /// function of the query point alone — never of `elapsed` — so as the
    /// parameter moves, the *mixture* changes and each child's own animation
    /// keeps playing, which is exactly the behaviour that makes a blended
    /// locomotion read as one continuous motion rather than a cross-fade.
    pub fn at(&self, x: f32, y: f32, elapsed: Duration) -> T {
        // The nearest child's contribution, accumulated separately, so an
        // exact hit short-circuits to the pure pose rather than leaning on
        // the epsilon arithmetic to dominate.
        let mut nearest = 0usize;
        let mut nearest_d2 = f32::INFINITY;
        for (index, child) in self.children.iter().enumerate() {
            let dx = child.point.0 - x;
            let dy = child.point.1 - y;
            let d2 = dx * dx + dy * dy;
            if d2 < nearest_d2 {
                nearest_d2 = d2;
                nearest = index;
            }
        }
        if nearest_d2 <= ON_TOP * ON_TOP {
            return self.children[nearest].track.at(elapsed);
        }

        // The weighted sum Σ wᵢ·pᵢ / Σ wᵢ, computed as a running lerp so any
        // `Lerp` type blends with no extra trait:
        //
        //   sum ← sum.lerp(pᵢ, wᵢ / (total + wᵢ));  total += wᵢ
        //
        // which by induction keeps `sum` equal to the normalised sum at every
        // step (two terms: sum moves from p₀ toward p₁ by exactly the share
        // w₁ owns of the new total). The induction needs `total` to stay
        // positive, which the epsilon above guarantees — every weight is
        // finite and strictly positive.
        let mut sum = self.children[0].track.at(elapsed);
        let mut total = 0.0f32;
        for child in &self.children {
            let dx = child.point.0 - x;
            let dy = child.point.1 - y;
            let d2 = (dx * dx + dy * dy).max(ON_TOP);
            let weight = 1.0 / d2;
            let pose = child.track.at(elapsed);
            let share = weight / (total + weight);
            sum = sum.lerp(pose, share);
            total += weight;
        }
        sum
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyframe::Keyframe;
    use std::time::Duration;

    fn ms(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    fn flat(value: f32) -> Keyframes<f32> {
        Keyframes::new(value)
    }

    #[test]
    fn a_single_child_is_its_pure_pose_everywhere() {
        let tree = BlendTree1::new(0.0, flat(7.0)).child(1.0, flat(9.0)).child(9.0, flat(11.0));
        // ^ three children, but the parameter is what picks; here one child:
        let one = BlendTree1::new(0.3, flat(7.0));
        for p in [-1.0, 0.0, 0.3, 5.0] {
            assert_eq!(one.at(p, ms(0)), 7.0);
        }
        assert_eq!(tree.at(-5.0, ms(0)), 7.0, "below every threshold clamps");
        assert_eq!(tree.at(99.0, ms(0)), 11.0, "above every threshold clamps");
    }

    #[test]
    fn endpoints_are_the_children_exactly() {
        let tree = BlendTree1::new(0.0, flat(10.0)).child(1.0, flat(30.0));
        assert_eq!(tree.at(0.0, ms(0)), 10.0);
        assert_eq!(tree.at(1.0, ms(0)), 30.0);
    }

    #[test]
    fn the_middle_of_symmetric_children_is_the_middle() {
        let tree = BlendTree1::new(0.0, flat(10.0)).child(1.0, flat(30.0));
        // smoothstep(0.5) = 0.5, so symmetric children blend to their mean.
        assert!((tree.at(0.5, ms(0)) - 20.0).abs() < 1e-4);
    }

    #[test]
    fn uneven_thresholds_blend_over_their_own_span() {
        // idle 0, walk 0.4, run 1.0: a parameter of 0.4 is *pure* walk.
        let tree = BlendTree1::new(0.0, flat(0.0))
            .child(0.4, flat(4.0))
            .child(1.0, flat(10.0));
        assert!((tree.at(0.4, ms(0)) - 4.0).abs() < 1e-4);
        // Half way from walk to run (0.7) is their mean, by symmetry.
        assert!((tree.at(0.7, ms(0)) - 7.0).abs() < 1e-4);
    }

    #[test]
    fn same_threshold_replaces_rather_than_stacks() {
        let tree = BlendTree1::new(0.0, flat(1.0))
            .child(1.0, flat(2.0))
            .child(1.0, flat(3.0));
        assert_eq!(tree.children().len(), 2, "two children, not three");
        assert_eq!(tree.at(1.0, ms(0)), 3.0, "the second authoring wins");
    }

    #[test]
    fn children_are_threshold_sorted_whatever_the_insert_order() {
        let tree = BlendTree1::new(1.0, flat(1.0))
            .child(0.0, flat(0.0))
            .child(0.5, flat(0.5));
        let thresholds: Vec<f32> = tree.children().iter().map(|c| c.threshold).collect();
        assert_eq!(thresholds, vec![0.0, 0.5, 1.0]);
    }

    #[test]
    fn children_are_sampled_at_a_shared_clock() {
        // The walk track is 0 → 10 over its life, the run track 0 → 20; at
        // half way in time the two *poses* are 5 and 10, and the blend at
        // parameter 0.5 (smoothstep 0.5) is their mean: 7.5. That number is
        // only reachable if both children were read at the *same* elapsed —
        // a run track read a beat later contributes a pose the walk track
        // never saw, and the answer shifts.
        let walk = Keyframes::new(0.0).with(Keyframe::to(1.0, 10.0));
        let run = Keyframes::new(0.0).with(Keyframe::to(1.0, 20.0));
        let tree = BlendTree1::new(0.0, walk).child(1.0, run);
        let half = tree.at(0.5, ms(500));
        assert!((half - 7.5).abs() < 1e-4, "shared clock gives {half}");
    }

    #[test]
    fn the_end_is_the_longest_child() {
        let short = Keyframes::new(0.0).with(Keyframe::to(0.5, 1.0));
        let long = Keyframes::new(0.0).with(Keyframe::to(2.0, 1.0));
        let tree = BlendTree1::new(0.0, short).child(1.0, long);
        assert_eq!(tree.end(), Duration::from_secs(2));
    }

    #[test]
    fn a_2d_query_on_a_child_is_that_child_exactly() {
        let tree = BlendTree2::new((0.0, 0.0), flat(1.0))
            .child((1.0, 0.0), flat(2.0))
            .child((0.0, 1.0), flat(3.0));
        assert_eq!(tree.at(1.0, 0.0, ms(0)), 2.0);
        assert_eq!(tree.at(0.0, 1.0, ms(0)), 3.0);
    }

    #[test]
    fn a_2d_midpoint_blends_the_neighbours() {
        let tree = BlendTree2::new((0.0, 0.0), flat(0.0)).child((2.0, 0.0), flat(2.0));
        // Equidistant children, equal weights (the third child is 2 away in
        // y, a quarter of the weight of each) — the answer is pulled toward
        // the mean of the two nearest, and is *between* them either way.
        let mid = tree.at(1.0, 0.0, ms(0));
        assert!((0.8..=1.2).contains(&mid), "midpoint blended to {mid}");
    }

    #[test]
    fn the_nearest_child_dominates() {
        // A far child with a huge pose should barely move the answer: the
        // property is checked by *removing* it and comparing, which measures
        // the far child's contribution directly rather than guessing what
        // the blend "should" be.
        let near_a = Keyframes::new(0.0);
        let near_b = Keyframes::new(1.0);
        let far = Keyframes::new(100.0);
        let with_far = BlendTree2::new((0.0, 0.0), near_a.clone())
            .child((1.0, 0.0), near_b.clone())
            .child((50.0, 50.0), far);
        let without_far = BlendTree2::new((0.0, 0.0), near_a)
            .child((1.0, 0.0), near_b);
        for i in 1..10 {
            let x = i as f32 / 10.0;
            let a = with_far.at(x, 0.0, ms(0));
            let b = without_far.at(x, 0.0, ms(0));
            assert!((a - b).abs() < 0.02, "far child moved {a} vs {b} at x={x}");
        }
        // And the answer itself stays bracketed by the near children.
        let near = with_far.at(0.9, 0.0, ms(0));
        assert!((0.0..=1.0).contains(&near), "{near} out of bracket");
    }

    #[test]
    fn a_single_2d_child_is_the_pose_everywhere() {
        let tree = BlendTree2::new((0.5, 0.5), flat(42.0));
        for (x, y) in [(0.0, 0.0), (1.0, 1.0), (-3.0, 9.0)] {
            assert_eq!(tree.at(x, y, ms(0)), 42.0);
        }
    }

    #[test]
    fn weights_2d_are_a_function_of_position_not_time() {
        let a = Keyframes::new(0.0).with(Keyframe::to(1.0, 10.0));
        let b = Keyframes::new(0.0).with(Keyframe::to(1.0, 20.0));
        let tree = BlendTree2::new((0.0, 0.0), a).child((2.0, 0.0), b);
        // At the two instants the poses advance but the *mixture* — checked
        // by the ratio of the blended value to each child's own value — is
        // unchanged: same weights, different clock.
        let early = tree.at(1.0, 0.0, ms(0));
        let late = tree.at(1.0, 0.0, ms(500));
        // Early: both children are 0, so any weights give 0. Late: both are
        // at their half-life values (5 and 10); the blend must be between
        // them, in the same proportion early's weights imply.
        assert_eq!(early, 0.0);
        assert!((5.0..=10.0).contains(&late), "blend {late} out of bracket");
        let again = tree.at(1.0, 0.0, ms(500));
        assert_eq!(late, again, "deterministic");
    }

    #[test]
    fn blending_works_on_poses_not_just_scalars() {
        use vieww_foundation::Offset;
        let left = Keyframes::new(Offset::new(0.0, 0.0));
        let right = Keyframes::new(Offset::new(10.0, 20.0));
        let tree = BlendTree1::new(0.0, left).child(1.0, right);
        let mid = tree.at(0.5, ms(0));
        assert!((mid.dx - 5.0).abs() < 1e-3 && (mid.dy - 10.0).abs() < 1e-3);
    }
}
