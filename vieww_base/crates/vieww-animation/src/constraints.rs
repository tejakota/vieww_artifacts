//! Constraints — Blender's constraint stack, Unity's constraint components,
//! After Effects' parenting-with-a-twist, in two dimensions.
//!
//! # The capability
//!
//! The document's Blender table (§2.14, L4) lists *constraints* beside
//! keyframes and drivers: rules evaluated **after** animation that make one
//! object's transform depend on another's — "look at the ball", "stay on
//! this path", "never go below the floor", "follow the hand but only half
//! as much". Unity ships the same set as `LookAtConstraint`,
//! `PositionConstraint`, `ParentConstraint`; Unreal as control rig nodes.
//! They are what makes a rig *procedural*: the eyes track by rule instead of
//! by keyframe.
//!
//! # The model
//!
//! Each object is a [`BoneTransform`] (position, rotation, scale). A
//! [`ConstraintStack`] is an ordered list of [`Constraint`]s, each with an
//! **influence** in 0..=1; evaluation runs them top to bottom, and each
//! result is blended with its input by the influence — Blender's semantics
//! exactly, including that later constraints see earlier ones' output.
//! Targets are looked up by name in a [`Targets`] map the caller refreshes
//! each frame (the "scene"), so constraints hold names, not references, and
//! the stack stays plain data.

use std::collections::BTreeMap;

use vieww_foundation::{Offset, Path, PathMeasure};

use crate::skeletal::BoneTransform;
use crate::tween::Lerp;

/// Named world transforms constraints can refer to.
pub type Targets = BTreeMap<String, BoneTransform>;

/// Which way a distance limit pushes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistanceMode {
    /// Keep at most the distance away (a leash).
    Inside,
    /// Keep at least the distance away (a repeller).
    Outside,
    /// Exactly on the circle.
    OnSurface,
}

/// One rule. See each variant.
#[derive(Debug, Clone)]
pub enum Constraint {
    /// Take the target's position (per axis), optionally *added* to our own.
    CopyLocation { target: String, x: bool, y: bool, offset: bool },
    /// Take the target's rotation, optionally added.
    CopyRotation { target: String, offset: bool },
    /// Take the target's scale.
    CopyScale { target: String },
    /// Clamp position into a box.
    LimitLocation { min: Offset, max: Offset },
    /// Clamp rotation (radians).
    LimitRotation { min: f32, max: f32 },
    /// Keep a distance from the target.
    LimitDistance { target: String, distance: f32, mode: DistanceMode },
    /// Rotate so the +x axis points at the target (plus `offset` radians) —
    /// Track To / Damped Track / LookAt.
    TrackTo { target: String, offset: f32 },
    /// Rotate toward the target *and* stretch along x so the end reaches it;
    /// `rest` is the length at scale 1 — Stretch To.
    StretchTo { target: String, rest: f32 },
    /// Ride a path at `fraction` of its length; with `follow`, turn with it.
    FollowPath { path: Box<PathMeasure>, fraction: f32, follow: bool },
    /// Be carried by the target as if parented to it, from the relative
    /// placement `inverse` records (Blender's "Set Inverse").
    ChildOf { target: String, inverse: BoneTransform },
    /// Stay above (y ≤ `height`, Y-down) a floor.
    Floor { height: f32 },
}

impl Constraint {
    /// A Follow Path constraint over `path`.
    #[must_use]
    pub fn follow_path(path: &Path, fraction: f32, follow: bool) -> Self {
        Self::FollowPath {
            path: Box::new(path.measure()),
            fraction,
            follow,
        }
    }

    fn apply(&self, input: BoneTransform, targets: &Targets) -> BoneTransform {
        let mut out = input;
        let target = |name: &str| targets.get(name).copied();
        match self {
            Self::CopyLocation { target: t, x, y, offset } => {
                if let Some(tt) = target(t) {
                    if *x {
                        out.translation.dx = tt.translation.dx + if *offset { input.translation.dx } else { 0.0 };
                    }
                    if *y {
                        out.translation.dy = tt.translation.dy + if *offset { input.translation.dy } else { 0.0 };
                    }
                }
            }
            Self::CopyRotation { target: t, offset } => {
                if let Some(tt) = target(t) {
                    out.rotation = tt.rotation + if *offset { input.rotation } else { 0.0 };
                }
            }
            Self::CopyScale { target: t } => {
                if let Some(tt) = target(t) {
                    out.scale = tt.scale;
                }
            }
            Self::LimitLocation { min, max } => {
                out.translation = Offset::new(
                    input.translation.dx.clamp(min.dx, max.dx),
                    input.translation.dy.clamp(min.dy, max.dy),
                );
            }
            Self::LimitRotation { min, max } => out.rotation = input.rotation.clamp(*min, *max),
            Self::LimitDistance { target: t, distance, mode } => {
                if let Some(tt) = target(t) {
                    let d = input.translation - tt.translation;
                    let len = d.distance();
                    let push = match mode {
                        DistanceMode::Inside => len > *distance,
                        DistanceMode::Outside => len < *distance,
                        DistanceMode::OnSurface => true,
                    };
                    if push && len > 1e-6 {
                        out.translation = tt.translation + d.scale(distance / len);
                    }
                }
            }
            Self::TrackTo { target: t, offset } => {
                if let Some(tt) = target(t) {
                    let d = tt.translation - input.translation;
                    if d.distance() > 1e-6 {
                        out.rotation = d.dy.atan2(d.dx) + offset;
                    }
                }
            }
            Self::StretchTo { target: t, rest } => {
                if let Some(tt) = target(t) {
                    let d = tt.translation - input.translation;
                    let len = d.distance();
                    if len > 1e-6 && *rest > 0.0 {
                        out.rotation = d.dy.atan2(d.dx);
                        let s = len / rest;
                        // Volume preservation, as Blender's default does.
                        out.scale = (input.scale.0 * s, input.scale.1 / s.sqrt());
                    }
                }
            }
            Self::FollowPath { path, fraction, follow } => {
                if let Some(p) = path.point_at_fraction(*fraction) {
                    out.translation = p.position + input.translation;
                    if *follow {
                        out.rotation = p.angle + input.rotation;
                    }
                }
            }
            Self::ChildOf { target: t, inverse } => {
                if let Some(parent) = target(t) {
                    // world = parent ∘ inverse ∘ local
                    let rel = compose(*inverse, input);
                    out = compose(parent, rel);
                }
            }
            Self::Floor { height } => out.translation.dy = input.translation.dy.min(*height),
        }
        out
    }
}

/// `outer ∘ inner`: place `inner` inside `outer`'s frame.
#[must_use]
pub fn compose(outer: BoneTransform, inner: BoneTransform) -> BoneTransform {
    let m = outer.to_transform();
    BoneTransform {
        translation: m.apply(inner.translation),
        rotation: outer.rotation + inner.rotation,
        scale: (outer.scale.0 * inner.scale.0, outer.scale.1 * inner.scale.1),
    }
}

/// The inverse placement for [`Constraint::ChildOf`]: what `child` looks
/// like in `parent`'s frame right now, so parenting does not jump.
#[must_use]
pub fn set_inverse(parent: BoneTransform, child: BoneTransform) -> BoneTransform {
    let inv = parent.to_transform().invert().unwrap_or(vieww_foundation::Transform::IDENTITY);
    BoneTransform {
        translation: inv.apply(child.translation),
        rotation: child.rotation - parent.rotation,
        scale: (
            child.scale.0 / parent.scale.0.max(1e-6),
            child.scale.1 / parent.scale.1.max(1e-6),
        ),
    }
}

/// An ordered stack of constraints with influences.
#[derive(Debug, Clone, Default)]
pub struct ConstraintStack {
    items: Vec<(Constraint, f32)>,
}

impl ConstraintStack {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a constraint at full influence.
    #[must_use]
    pub fn with(mut self, c: Constraint) -> Self {
        self.items.push((c, 1.0));
        self
    }

    /// Push with an influence (0..=1).
    #[must_use]
    pub fn with_influence(mut self, c: Constraint, influence: f32) -> Self {
        self.items.push((c, influence.clamp(0.0, 1.0)));
        self
    }

    /// Change the influence of constraint `i`.
    pub fn set_influence(&mut self, i: usize, influence: f32) {
        if let Some(item) = self.items.get_mut(i) {
            item.1 = influence.clamp(0.0, 1.0);
        }
    }

    /// Run the stack over `input`.
    #[must_use]
    pub fn evaluate(&self, input: BoneTransform, targets: &Targets) -> BoneTransform {
        self.items.iter().fold(input, |acc, (c, w)| acc.lerp(c.apply(acc, targets), *w))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, PI};

    fn at(x: f32, y: f32) -> BoneTransform {
        BoneTransform::from_translation(Offset::new(x, y))
    }

    fn targets() -> Targets {
        let mut t = Targets::new();
        t.insert("ball".into(), at(10.0, 10.0));
        t.insert("hand".into(), BoneTransform { rotation: 0.5, ..at(5.0, 0.0) });
        t
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn track_to_points_at_the_target() {
        let s = ConstraintStack::new().with(Constraint::TrackTo { target: "ball".into(), offset: 0.0 });
        let r = s.evaluate(at(0.0, 0.0), &targets());
        assert!(close(r.rotation, PI / 4.0));
    }

    #[test]
    fn influence_blends_with_the_input() {
        let s = ConstraintStack::new().with_influence(
            Constraint::CopyLocation { target: "ball".into(), x: true, y: true, offset: false },
            0.5,
        );
        let r = s.evaluate(at(0.0, 0.0), &targets());
        assert_eq!(r.translation, Offset::new(5.0, 5.0));
    }

    #[test]
    fn later_constraints_see_earlier_results() {
        let s = ConstraintStack::new()
            .with(Constraint::CopyLocation { target: "ball".into(), x: true, y: true, offset: false })
            .with(Constraint::LimitLocation { min: Offset::new(0.0, 0.0), max: Offset::new(8.0, 100.0) });
        assert_eq!(s.evaluate(at(0.0, 0.0), &targets()).translation, Offset::new(8.0, 10.0));
    }

    #[test]
    fn distance_limits_leash_and_repel() {
        let t = targets();
        let leash = ConstraintStack::new().with(Constraint::LimitDistance {
            target: "ball".into(),
            distance: 5.0,
            mode: DistanceMode::Inside,
        });
        let r = leash.evaluate(at(10.0, 30.0), &t);
        assert!(close((r.translation - Offset::new(10.0, 10.0)).distance(), 5.0));
        let repel = ConstraintStack::new().with(Constraint::LimitDistance {
            target: "ball".into(),
            distance: 5.0,
            mode: DistanceMode::Outside,
        });
        assert_eq!(repel.evaluate(at(10.0, 30.0), &t).translation, Offset::new(10.0, 30.0));
    }

    #[test]
    fn stretch_to_reaches_and_preserves_volume() {
        let s = ConstraintStack::new().with(Constraint::StretchTo { target: "ball".into(), rest: 10.0 });
        let r = s.evaluate(at(10.0, 0.0), &targets());
        assert!(close(r.rotation, FRAC_PI_2));
        assert!(close(r.scale.0, 1.0));
        let r2 = s.evaluate(at(10.0, -30.0), &targets());
        assert!(close(r2.scale.0, 4.0) && close(r2.scale.1, 0.5));
    }

    #[test]
    fn follow_path_rides_and_turns() {
        let mut p = Path::new();
        p.move_to(Offset::ZERO).line_to(Offset::new(100.0, 0.0)).line_to(Offset::new(100.0, 100.0));
        let s = ConstraintStack::new().with(Constraint::follow_path(&p, 0.75, true));
        let r = s.evaluate(BoneTransform::default(), &Targets::new());
        assert!(close(r.translation.dx, 100.0) && close(r.translation.dy, 50.0));
        assert!(close(r.rotation, FRAC_PI_2));
    }

    #[test]
    fn child_of_with_set_inverse_does_not_jump_and_then_follows() {
        let mut t = targets();
        let child = at(20.0, 0.0);
        let inverse = set_inverse(t["hand"], child);
        let s = ConstraintStack::new().with(Constraint::ChildOf { target: "hand".into(), inverse });
        let r = s.evaluate(BoneTransform::default(), &t);
        assert!(close(r.translation.dx, 20.0) && close(r.translation.dy, 0.0), "{r:?}");
        // Move the hand: the child is carried.
        t.insert("hand".into(), BoneTransform { rotation: 0.5, ..at(15.0, 0.0) });
        let moved = s.evaluate(BoneTransform::default(), &t);
        assert!(close(moved.translation.dx, 30.0), "{moved:?}");
    }

    #[test]
    fn floor_and_limit_rotation() {
        let s = ConstraintStack::new()
            .with(Constraint::Floor { height: 50.0 })
            .with(Constraint::LimitRotation { min: -0.5, max: 0.5 });
        let r = s.evaluate(BoneTransform { rotation: 2.0, ..at(0.0, 80.0) }, &Targets::new());
        assert!(close(r.translation.dy, 50.0) && close(r.rotation, 0.5));
    }
}
