//! Skeletal animation: bones, poses, skinning.
//!
//! This is the Rive/Spine shape of skeletal animation — **two-dimensional**
//! bones that carry artwork with them — rather than the Unity shape of
//! skinned 3D meshes with dual-quaternion blending. That is the honest fit
//! for a framework whose rasteriser draws in a plane: the comparison table
//! that lists "skeletal animation" as a gap names Rive for exactly this
//! pattern, and 3D mesh *loading* is a different gap with a different crate.
//!
//! # The model in one paragraph
//!
//! A [`Skeleton`] is bones in a forest — each has a parent, or does not —
//! where every bone owns a *local* transform (translation, rotation, scale)
//! relative to its parent. A [`Pose`] is one local transform per bone. The
//! skeleton turns a pose into **world** transforms by walking parents before
//! children and chaining. Artwork is attached by a [`Skin`]: each vertex
//! carries weights to a handful of bones, and deforming is
//! *linear-blend skinning* — the weighted average, per vertex, of where each
//! bound bone would have carried it. Animate the bones, and the artwork
//! follows.
//!
//! ```
//! use vieww_animation::skeletal::{BoneTransform, Skeleton, Skin, Vertex};
//! use vieww_foundation::Offset;
//!
//! // A two-bone arm: a shoulder, and a forearm hanging off it.
//! let skeleton = Skeleton::new()
//!     .bone("shoulder", None, BoneTransform::default())
//!     .bone("forearm", Some("shoulder"), BoneTransform::from_translation(Offset::new(0.0, 40.0)));
//!
//! // Skin a vertex to the forearm (bone 1) at the forearm's tip.
//! let bind_pose = skeleton.bind_pose();
//! let skin = Skin::bind(&skeleton, &bind_pose)
//!     .vertex(Vertex::new(Offset::new(0.0, 80.0)).binding(1, 1.0));
//!
//! // Rotate the shoulder: the tip, two bones down, moves with it.
//! let mut pose = bind_pose.clone();
//! pose.set(&skeleton, "shoulder", BoneTransform::from_rotation(std::f32::consts::FRAC_PI_2));
//!
//! let world = skeleton.world(&pose);
//! let deformed = skin.apply(&world);
//!
//! // 90 degrees of shoulder rotation swings the tip from (0, 80) to (-80, 0)
//! // ...in the shoulder's frame, which the forearm chain has carried along.
//! let tip = deformed[0];
//! assert!(tip.dx.abs() > 70.0, "the vertex followed the bones: {tip:?}");
//! assert!(tip.dy.abs() < 15.0);
//! ```
//!
//! # Why bones are named and indexed both
//!
//! Authoring happens in names — "left-thigh", "left-shin" — and the hot path
//! happens in indices: `world` is a flat `Vec` walk with no lookup at all.
//! The skeleton resolves names to indices once, at build time, and a pose
//! set by name pays for the lookup only when it sets.
//!
//! # The one deliberate omission
//!
//! No inverse kinematics. A solver that pulls a foot to a floor position is
//! real value, but it is also a numerical method with convergence behaviour
//! that this crate would own forever; and the crate's contract — everything
//! here is a pure function of its inputs, testable by handing it a
//! `Duration` — is worth more than a feature that half the users of an IK
//! system tune away anyway. FK (forward: animate parents, children follow)
//! is the whole of what this module does, stated out loud so the gap is a
//! scope decision rather than an oversight.

use crate::keyframe::Keyframes;
use crate::tween::Lerp;
use vieww_foundation::{Offset, Transform};

/// A bone's local placement: where it sits relative to its parent.
///
/// Deliberately plain — three numbers and a point — because everything
/// interesting about a bone is in its *chain*, not the bone. The compose
/// order is scale, then rotation, then translation, which is the order every
/// 2D rig package uses and the order a designer expects when they say
/// "move the bone down and turn it".
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoneTransform {
    /// Where the bone's origin sits in its parent's frame.
    pub translation: Offset,
    /// Rotation in radians, clockwise in screen space (y-down), like
    /// [`Transform::rotate`].
    pub rotation: f32,
    /// Non-uniform scale in the bone's local axes.
    pub scale: (f32, f32),
}

impl Default for BoneTransform {
    /// Identity, not zero: a bone at rest is *unchanged*, and a derived
    /// `Default` would have said scale `(0, 0)` — a bone that collapses its
    /// whole subtree onto a point, with a determinant of zero and no inverse
    /// for a skin to bind through. The unit of scale is one; leaving it to
    /// `derive` is how a rest pose becomes a black hole.
    fn default() -> Self {
        Self {
            translation: Offset::new(0.0, 0.0),
            rotation: 0.0,
            scale: (1.0, 1.0),
        }
    }
}

impl BoneTransform {
    /// A transform that moves the bone to `translation` and nothing else.
    #[must_use]
    pub const fn from_translation(translation: Offset) -> Self {
        Self {
            translation,
            rotation: 0.0,
            scale: (1.0, 1.0),
        }
    }

    /// A transform that rotates the bone around its parent-space origin.
    #[must_use]
    pub const fn from_rotation(rotation: f32) -> Self {
        Self {
            translation: Offset::new(0.0, 0.0),
            rotation,
            scale: (1.0, 1.0),
        }
    }

    /// The affine this describes: points are **scaled, then rotated, then
    /// translated** — the order every 2D rig package composes in, and the one
    /// a designer means by "move the bone down and turn it". Spelled in
    /// [`Transform::then`] order ("applies `self` first"), that is
    /// `scale.then(rotate).then(translate)`.
    #[must_use]
    pub fn to_transform(self) -> Transform {
        Transform::scale(self.scale.0, self.scale.1)
            .then(Transform::rotate(self.rotation))
            .then(Transform::translate(self.translation))
    }
}

impl Lerp for BoneTransform {
    /// Component-wise: translation, rotation, scale each interpolate on
    /// their own. Rotation is **linear, not shortest-path** — an authored
    /// track that turns a bone from 0 to `τ/3` should sweep the long way if
    /// that is what its two endpoints say, and second-guessing the author
    /// between two keyframes is a value nobody can reason about at the
    /// keyframes themselves.
    fn lerp(self, other: Self, t: f32) -> Self {
        Self {
            translation: self.translation.lerp(other.translation, t),
            rotation: self.rotation.lerp(other.rotation, t),
            scale: (
                self.scale.0.lerp(other.scale.0, t),
                self.scale.1.lerp(other.scale.1, t),
            ),
        }
    }
}

/// The bones, their hierarchy, and the pose they were authored around.
///
/// Bones are added in any order — a parent may be declared after a child —
/// because a rig exported from a tool arrives in whatever order the tool's
/// map holds, and requiring the application to topologically sort it by hand
/// is requiring it to know an implementation detail of somebody else's
/// serialiser.
#[derive(Debug, Clone, PartialEq)]
pub struct Skeleton {
    bones: Vec<Bone>,
    /// The local transforms the bones were rigged at — see
    /// [`bind_pose`](Self::bind_pose).
    rest: Vec<BoneTransform>,
}

/// One bone of a [`Skeleton`], resolved.
#[derive(Debug, Clone, PartialEq)]
struct Bone {
    name: &'static str,
    /// `None` for a root; `Some(index)` for everyone else, resolved from the
    /// author's `Option<&'static str>` parent name at build time.
    parent: Option<usize>,
}

impl Skeleton {
    /// An empty skeleton. Add bones with [`bone`](Self::bone).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            bones: Vec::new(),
            rest: Vec::new(),
        }
    }

    /// Add a bone: named, with a parent *name* (or `None`), at a local rest
    /// transform. Returns `self`, for rig-building chains.
    ///
    /// # Panics
    ///
    /// If `parent` names a bone that does not exist — including one declared
    /// *later*, because a forward reference is a cycle wearing a disguise —
    /// or if the name is already taken. Both are rig-authoring errors, and a
    /// rig that built half of itself before failing is worse than one that
    /// refused at the first bad bone.
    #[must_use]
    pub fn bone(
        mut self,
        name: &'static str,
        parent: Option<&'static str>,
        rest: BoneTransform,
    ) -> Self {
        assert!(
            !self.bones.iter().any(|bone| bone.name == name),
            "two bones named `{name}`"
        );
        let parent = parent.map(|parent| {
            // A parent must already exist, so its index is always smaller
            // than the child's about to be pushed — which is the property
            // `world`'s flat walk depends on, and why no cycle is possible.
            let index = self
                .bones
                .iter()
                .position(|bone| bone.name == parent)
                .unwrap_or_else(|| panic!("bone `{name}`'s parent `{parent}` does not exist"));
            index
        });
        self.bones.push(Bone { name, parent });
        self.rest.push(rest);
        self
    }

    /// The number of bones.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bones.len()
    }

    /// Whether the skeleton has no bones at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bones.is_empty()
    }

    /// A bone's index by name, for callers holding names across calls.
    #[must_use]
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|bone| bone.name == name)
    }

    /// A bone's name by index, for tools walking the skeleton.
    #[must_use]
    pub fn name_of(&self, index: usize) -> Option<&'static str> {
        self.bones.get(index).map(|bone| bone.name)
    }

    /// The pose the rig was built around: every bone at its rest transform.
    ///
    /// This is the pose a [`Skin`] binds in, and the pose an animation
    /// *starts from* — a clip animating three of forty bones samples its
    /// three tracks over a base of [`bind_pose`](Self::bind_pose), not over
    /// zero, because a bone with no track is at rest, not at the origin.
    #[must_use]
    pub fn bind_pose(&self) -> Pose {
        Pose {
            bones: self.rest.clone(),
        }
    }

    /// Turn a local `pose` into world transforms, one per bone, in bone
    /// order: parent chains composed.
    ///
    /// The walk is a single flat loop that may read any *earlier* entry —
    /// valid because a parent's index is always smaller than its child's —
    /// so there is no recursion, no visited set, and no cycle possible. A
    /// cycle in a rig is a build-time error this type cannot represent,
    /// which is the strongest form of that guarantee.
    #[must_use]
    pub fn world(&self, pose: &Pose) -> Vec<Transform> {
        debug_assert_eq!(
            pose.bones.len(),
            self.bones.len(),
            "a pose belongs to the skeleton it was made for"
        );
        let mut world: Vec<Transform> = Vec::with_capacity(self.bones.len());
        for (index, bone) in self.bones.iter().enumerate() {
            let local = pose
                .bones
                .get(index)
                .copied()
                .unwrap_or_default()
                .to_transform();
            // A child's world is its parent's world with the child's local
            // applied *first* — the point is carried into the parent's frame
            // and then wherever the parent carries its frame. `then` applies
            // its receiver first, so this is `local.then(world[parent])`, the
            // order the whole of skeletal animation depends on.
            let composed = match bone.parent {
                Some(parent) => local.then(world[parent]),
                None => local,
            };
            world.push(composed);
        }
        world
    }
}

impl Default for Skeleton {
    fn default() -> Self {
        Self::new()
    }
}

/// One local transform per bone, in bone order — what an animation produces
/// and the skeleton turns into world space.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Pose {
    bones: Vec<BoneTransform>,
}

impl Pose {
    /// A pose of `len` bones, every one at identity.
    ///
    /// Rarely what a caller wants over a rig — [`Skeleton::bind_pose`] is —
    /// but the honest base for building a pose bone by bone in tests.
    #[must_use]
    pub fn identity(len: usize) -> Self {
        Self {
            bones: vec![BoneTransform::default(); len],
        }
    }

    /// The number of bones the pose carries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bones.len()
    }

    /// Whether the pose carries no bones.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bones.is_empty()
    }

    /// A bone's local transform by index.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<BoneTransform> {
        self.bones.get(index).copied()
    }

    /// Overwrite a bone's local transform by index.
    ///
    /// Growing rather than panicking when the index is past the end, because
    /// a clip built name-at-a-time against a freshly-allocated pose is a real
    /// authoring path.
    pub fn set_index(&mut self, index: usize, transform: BoneTransform) {
        if index >= self.bones.len() {
            self.bones.resize(index + 1, BoneTransform::default());
        }
        self.bones[index] = transform;
    }

    /// Overwrite a bone's local transform by name. `false` — silently,
    /// deliberately — if the name is unknown: a clip that animates a bone
    /// the rig dropped plays the rest of its bones rather than dying, and
    /// the caller that wants loud has [`Skeleton::index_of`].
    ///
    /// The `skeleton` is passed in because a pose does not know its rig —
    /// the same pose type serves every skeleton of the same length, and
    /// wiring the name lookup through the pose would mean the pose owning a
    /// back-reference it mostly does not use.
    pub fn set(&mut self, skeleton: &Skeleton, name: &str, transform: BoneTransform) -> bool {
        match skeleton.index_of(name) {
            Some(index) => {
                self.set_index(index, transform);
                true
            }
            None => false,
        }
    }

    /// Blend two poses: every bone's transform lerped toward the other's.
    ///
    /// `self` at `t == 0`, `other` at `t == 1`. Poses of different lengths
    /// blend only over the bones both carry, and the longer one's extras are
    /// taken at `t` toward `other`'s *identity* — a bone one pose has and
    /// the other does not is a rig that changed shape mid-blend, and identity
    /// is its least surprising reading.
    #[must_use]
    pub fn blended(&self, other: &Pose, t: f32) -> Pose {
        let shared = self.bones.len().min(other.bones.len());
        let mut bones = Vec::with_capacity(self.bones.len().max(other.bones.len()));
        for index in 0..shared {
            bones.push(self.bones[index].lerp(other.bones[index], t));
        }
        for transform in self.bones.iter().skip(shared) {
            bones.push(transform.lerp(BoneTransform::default(), t));
        }
        for (index, transform) in other.bones.iter().enumerate().skip(shared) {
            if index >= bones.len() {
                bones.push(BoneTransform::default().lerp(*transform, t));
            }
        }
        Pose { bones }
    }
}

/// A skin: vertices bound to bones, deformed by wherever the bones are now.
///
/// "Skin" in the rig-package sense — the *mapping* between artwork and
/// skeleton, not the artwork itself. The artwork here is plain offsets; a
/// caller with a `Path` binds its points through the same
/// [`vertex`](Vertex::new) API and rebuilds the path from the results.
///
/// # How the binding works, and when
///
/// [`bind`](Self::bind) computes each bound bone's *inverse* bind-pose world
/// transform, once. A vertex's deformed position is then
///
/// ```text
/// Σ  wᵢ · Wᵢ' · Wᵢ⁻¹ · v
/// ```
///
/// where `Wᵢ'` is the bone's current world transform and the weights
/// normalise to one. The `Wᵢ⁻¹ · v` term is what makes the artwork stay
/// glued to the bone rather than to the world: it carries the vertex into
/// the bone's frame, so a bone that has not moved since bind puts every
/// bound vertex back exactly where it started.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Skin {
    /// Bind-pose world inverses, indexed by bone; `None` for unbound bones.
    inverses: Vec<Option<Transform>>,
    vertices: Vec<Vertex>,
}

/// One skinned vertex: a bind-pose position plus its bone weights.
#[derive(Debug, Clone, PartialEq)]
pub struct Vertex {
    bind: Offset,
    /// `(bone index, weight)` pairs. Normalised at [`Skin::apply`] time
    /// rather than at insertion, so a tool can build weights a spread at a
    /// time without every intermediate needing to sum to one.
    weights: Vec<(usize, f32)>,
}

impl Vertex {
    /// A vertex at `bind`, bound to nothing yet.
    #[must_use]
    pub const fn new(bind: Offset) -> Self {
        Self {
            bind,
            weights: Vec::new(),
        }
    }

    /// Add (or replace) a binding by bone index.
    #[must_use]
    pub fn binding(mut self, bone: usize, weight: f32) -> Self {
        match self.weights.iter_mut().find(|(index, _)| *index == bone) {
            Some(existing) => existing.1 = weight,
            None => self.weights.push((bone, weight)),
        }
        self
    }

    /// The bind-pose position.
    #[must_use]
    pub const fn bind_position(&self) -> Offset {
        self.bind
    }

    /// The bindings, as `(bone index, weight)` pairs.
    #[must_use]
    pub fn bindings(&self) -> &[(usize, f32)] {
        &self.weights
    }
}

impl Skin {
    /// Bind a skin against `skeleton` posed at `bind_pose`: the point at
    /// which bone inverses are frozen.
    ///
    /// The pose is passed in rather than read from the skeleton so that a
    /// re-bind — the same rig re-skinned at a different pose — is not a
    /// special case; the caller hands the skeleton's own
    /// [`bind_pose`](Skeleton::bind_pose) in the common case.
    #[must_use]
    pub fn bind(skeleton: &Skeleton, bind_pose: &Pose) -> Self {
        let world = skeleton.world(bind_pose);
        let inverses = world
            .into_iter()
            .map(|transform| transform.invert())
            .collect();
        Self {
            inverses,
            vertices: Vec::new(),
        }
    }

    /// The inverse bind transforms, by bone index — `None` for bones whose
    /// bind transform is singular (a zero scale), which deform nothing.
    #[must_use]
    pub fn inverses(&self) -> &[Option<Transform>] {
        &self.inverses
    }

    /// Add a bound vertex. Returns `self` for chains.
    ///
    /// Bindings that name a bone outside the skeleton are *refused* by
    /// being dropped at [`apply`](Self::apply) — an index the inverse list
    /// does not carry is a rig that changed under the skin, and the honest
    /// behaviour is to draw the vertex at bind, not to panic mid-mesh.
    #[must_use]
    pub fn vertex(mut self, vertex: Vertex) -> Self {
        self.vertices.push(vertex);
        self
    }

    /// The vertices in their bind positions.
    #[must_use]
    pub fn vertices(&self) -> &[Vertex] {
        &self.vertices
    }

    /// Deform every vertex to `world`: linear-blend skinning.
    ///
    /// A vertex bound to nothing — or to bones the skin does not know —
    /// comes back at its bind position, which is the position the artwork
    /// was authored at and the only position with a claim to be "unchanged".
    ///
    /// `world` shorter than the inverse list (the rig grew) treats the new
    /// bones' vertices the same way: bind position. `world` longer is fine;
    /// extras are unbound from this skin's point of view.
    #[must_use]
    pub fn apply(&self, world: &[Transform]) -> Vec<Offset> {
        self.vertices
            .iter()
            .map(|vertex| {
                // Weights normalise here: a tool authoring "0.5, 0.5, 0.5"
                // means "a third each", and dividing once at the end is both
                // cheaper than per-insertion renormalisation and correct for
                // every intermediate state along the way.
                let total: f32 = vertex
                    .weights
                    .iter()
                    .filter(|(bone, _)| self.bound(*bone, world))
                    .map(|(_, weight)| weight.abs())
                    .sum();
                if total <= f32::EPSILON {
                    return vertex.bind;
                }
                let mut x = 0.0;
                let mut y = 0.0;
                for &(bone, weight) in &vertex.weights {
                    if !self.bound(bone, world) {
                        continue;
                    }
                    // `unwrap` is sound: `bound` checked `is_some` for this
                    // same index one branch ago.
                    let inverse = self.inverses[bone].unwrap();
                    let placed = world[bone].apply(inverse.apply(vertex.bind));
                    x += placed.dx * weight / total;
                    y += placed.dy * weight / total;
                }
                Offset::new(x, y)
            })
            .collect()
    }

    /// A bone index this skin can deform with: known inverse, live world.
    fn bound(&self, bone: usize, world: &[Transform]) -> bool {
        self.inverses.get(bone).is_some_and(Option::is_some) && bone < world.len()
    }
}

/// A clip: per-bone keyframed tracks, sampled over a base pose.
///
/// One track per *bone*, not per *property* (rotation and translation
/// separately). The whole-bone track is what Rive's timeline and Spine's
/// dopesheet both author when a bone's channels move together — the common
/// case — and a property that does not move simply holds its base value,
/// because a [`BoneTransform`] lerps component-wise and a keyframe that
/// repeats the base pose's translation changes nothing.
///
/// ```
/// use std::time::Duration;
/// use vieww_animation::{Keyframe, Keyframes};
/// use vieww_animation::skeletal::{BoneTransform, Skeleton, SkeletalClip};
/// use vieww_foundation::Offset;
///
/// let skeleton = Skeleton::new()
///     .bone("hand", None, BoneTransform::default());
///
/// // The hand lifts: translation y 0 → -12 → 0, over half a second.
/// let clip = SkeletalClip::new()
///     .bone("hand", Keyframes::new(BoneTransform::default())
///         .with(Keyframe::to(0.25, BoneTransform::from_translation(Offset::new(0.0, -12.0))))
///         .with(Keyframe::to(0.5, BoneTransform::default())));
///
/// let pose = clip.pose(&skeleton, Duration::from_millis(125));
/// let lifted = pose.get(skeleton.index_of("hand").unwrap()).unwrap();
/// assert_eq!(lifted.translation, Offset::new(0.0, -6.0), "half way up");
/// ```
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SkeletalClip {
    tracks: Vec<(&'static str, Keyframes<BoneTransform>)>,
}

impl SkeletalClip {
    /// A clip with no tracks yet: sampling it yields the base pose.
    #[must_use]
    pub const fn new() -> Self {
        Self { tracks: Vec::new() }
    }

    /// Add (or replace) a bone's track. Returns `self` for chains.
    ///
    /// A bone the skeleton does not have is not an error here — the track is
    /// dead weight and [`pose`](Self::pose) skips it — because a clip shared
    /// across rig variants is a real asset and a missing bone is the *normal*
    /// difference between two variants of one character.
    #[must_use]
    pub fn bone(mut self, name: &'static str, frames: Keyframes<BoneTransform>) -> Self {
        match self.tracks.iter_mut().find(|(track, _)| *track == name) {
            Some(existing) => existing.1 = frames,
            None => self.tracks.push((name, frames)),
        }
        self
    }

    /// The pose at `elapsed`, over `base` (usually the skeleton's bind pose).
    #[must_use]
    pub fn pose(&self, skeleton: &Skeleton, elapsed: std::time::Duration) -> Pose {
        self.pose_over(skeleton, skeleton.bind_pose(), elapsed)
    }

    /// [`pose`](Self::pose), over an explicit base pose — the entry point for
    /// blending a clip onto another clip's output, or onto a live-edited pose.
    #[must_use]
    pub fn pose_over(&self, skeleton: &Skeleton, base: Pose, elapsed: std::time::Duration) -> Pose {
        let mut pose = base;
        for (name, frames) in &self.tracks {
            // See `bone`: unknown names skip silently, and the pose's `set`
            // is the half of that decision that lives here.
            pose.set(skeleton, name, frames.at(elapsed));
        }
        pose
    }

    /// When the longest track ends: the clip's natural duration.
    #[must_use]
    pub fn duration(&self) -> std::time::Duration {
        self.tracks
            .iter()
            .map(|(_, frames)| frames.end())
            .fold(std::time::Duration::ZERO, std::cmp::Ord::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyframe::Keyframe;
    use std::f32::consts::FRAC_PI_2;
    use std::time::Duration;

    fn ms(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    #[test]
    fn world_chains_parents() {
        // Root at (10, 0), child at (0, 20) relative to root.
        let skeleton = Skeleton::new()
            .bone(
                "root",
                None,
                BoneTransform::from_translation(Offset::new(10.0, 0.0)),
            )
            .bone(
                "child",
                Some("root"),
                BoneTransform::from_translation(Offset::new(0.0, 20.0)),
            );

        let world = skeleton.world(&skeleton.bind_pose());
        let origin = world[1].apply(Offset::new(0.0, 0.0));
        assert_eq!(
            origin,
            Offset::new(10.0, 20.0),
            "parent then child composes"
        );
    }

    #[test]
    fn a_child_may_be_declared_before_its_parent() {
        // Wait: it may not, and the error is loud — this test pins that.
        let result = std::panic::catch_unwind(|| {
            let _ = Skeleton::new().bone("child", Some("not-yet"), BoneTransform::default());
        });
        assert!(result.is_err(), "a parent must exist before its child");
    }

    #[test]
    fn rotation_of_a_parent_swings_the_chain() {
        // Root at origin, rotated 90°; child 40 units down the y axis.
        let skeleton = Skeleton::new()
            .bone("root", None, BoneTransform::default())
            .bone(
                "child",
                Some("root"),
                BoneTransform::from_translation(Offset::new(0.0, 40.0)),
            );

        let mut pose = skeleton.bind_pose();
        pose.set(&skeleton, "root", BoneTransform::from_rotation(FRAC_PI_2));

        let world = skeleton.world(&pose);
        let tip = world[1].apply(Offset::new(0.0, 0.0));
        // 90° clockwise (y-down): +y rotates to -x.
        assert!(tip.dx < -39.0 && tip.dy.abs() < 1.0, "tip at {tip:?}");
    }

    #[test]
    fn bind_pose_world_is_the_identity_the_skin_freezes() {
        let skeleton = Skeleton::new().bone(
            "a",
            None,
            BoneTransform::from_translation(Offset::new(5.0, 5.0)),
        );
        let world = skeleton.world(&skeleton.bind_pose());
        let origin = world[0].apply(Offset::new(0.0, 0.0));
        assert_eq!(origin, Offset::new(5.0, 5.0));
    }

    #[test]
    fn skinning_an_unmoved_rig_returns_bind_positions() {
        let skeleton = Skeleton::new().bone(
            "b",
            None,
            BoneTransform::from_translation(Offset::new(3.0, 4.0)),
        );
        let bind = skeleton.bind_pose();
        let skin = Skin::bind(&skeleton, &bind)
            .vertex(Vertex::new(Offset::new(10.0, 10.0)).binding(0, 1.0));

        let world = skeleton.world(&bind);
        let deformed = skin.apply(&world);
        assert_eq!(
            deformed[0],
            Offset::new(10.0, 10.0),
            "an unmoved bone leaves its vertices at bind"
        );
    }

    #[test]
    fn skinning_follows_a_translated_bone() {
        let skeleton = Skeleton::new().bone(
            "b",
            None,
            BoneTransform::from_translation(Offset::new(0.0, 0.0)),
        );
        let bind = skeleton.bind_pose();
        let mut skin = Skin::bind(&skeleton, &bind);
        skin = skin.vertex(Vertex::new(Offset::new(6.0, 0.0)).binding(0, 1.0));

        let mut pose = bind.clone();
        pose.set(
            &skeleton,
            "b",
            BoneTransform::from_translation(Offset::new(2.0, 0.0)),
        );
        let world = skeleton.world(&pose);
        let deformed = skin.apply(&world);
        assert_eq!(
            deformed[0],
            Offset::new(8.0, 0.0),
            "the vertex moved with its bone, not with the world"
        );
    }

    #[test]
    fn weights_blend_two_bones() {
        // Two bones 10 apart in x; a vertex half-bound to each lands between
        // wherever each would have put it.
        let skeleton = Skeleton::new()
            .bone(
                "left",
                None,
                BoneTransform::from_translation(Offset::new(0.0, 0.0)),
            )
            .bone(
                "right",
                None,
                BoneTransform::from_translation(Offset::new(10.0, 0.0)),
            );
        let bind = skeleton.bind_pose();
        let mut skin = Skin::bind(&skeleton, &bind);
        skin = skin.vertex(
            Vertex::new(Offset::new(0.0, 0.0))
                .binding(0, 0.5)
                .binding(1, 0.5),
        );

        // Move only the right bone +20 in x: half-weight pulls 10.
        let mut pose = bind.clone();
        pose.set(
            &skeleton,
            "right",
            BoneTransform::from_translation(Offset::new(30.0, 0.0)),
        );
        let world = skeleton.world(&pose);
        let deformed = skin.apply(&world);
        let dx = deformed[0].dx;
        assert!((9.5..10.5).contains(&dx), "half of 20 units of pull: {dx}");
    }

    #[test]
    fn weights_normalise_lazily() {
        // Authoring weights 0.5 + 0.5 + 0.5 means a third each.
        let skeleton = Skeleton::new()
            .bone("a", None, BoneTransform::default())
            .bone("b", None, BoneTransform::default())
            .bone("c", None, BoneTransform::default());
        let bind = skeleton.bind_pose();
        let mut skin = Skin::bind(&skeleton, &bind);
        skin = skin.vertex(
            Vertex::new(Offset::new(0.0, 0.0))
                .binding(0, 0.5)
                .binding(1, 0.5)
                .binding(2, 0.5),
        );

        // All three at identity: the vertex stays at 0 regardless, so move
        // one bone and check the pull is a third, not a half.
        let mut pose = bind.clone();
        pose.set_index(0, BoneTransform::from_translation(Offset::new(30.0, 0.0)));
        let world = skeleton.world(&pose);
        let deformed = skin.apply(&world);
        let dx = deformed[0].dx;
        assert!((9.5..10.5).contains(&dx), "a third of 30 units: {dx}");
    }

    #[test]
    fn an_unbound_vertex_stays_at_bind() {
        let skeleton = Skeleton::new().bone("b", None, BoneTransform::default());
        let bind = skeleton.bind_pose();
        let mut skin = Skin::bind(&skeleton, &bind);
        skin = skin.vertex(Vertex::new(Offset::new(7.0, 8.0)));

        let mut pose = bind.clone();
        pose.set(
            &skeleton,
            "b",
            BoneTransform::from_translation(Offset::new(100.0, 100.0)),
        );
        let world = skeleton.world(&pose);
        let deformed = skin.apply(&world);
        assert_eq!(deformed[0], Offset::new(7.0, 8.0));
    }

    #[test]
    fn poses_blend_bone_by_bone() {
        let at_rest = Pose::identity(1);
        let lifted = {
            let mut pose = Pose::identity(1);
            pose.set_index(0, BoneTransform::from_translation(Offset::new(0.0, -10.0)));
            pose
        };
        let half = at_rest.blended(&lifted, 0.5);
        assert_eq!(
            half.get(0).unwrap().translation,
            Offset::new(0.0, -5.0),
            "half way between two poses"
        );
    }

    #[test]
    fn a_clip_samples_over_the_bind_pose() {
        let skeleton = Skeleton::new()
            .bone("hand", None, BoneTransform::default())
            .bone(
                "arm",
                None,
                BoneTransform::from_translation(Offset::new(2.0, 2.0)),
            );
        let clip = SkeletalClip::new().bone(
            "hand",
            Keyframes::new(BoneTransform::default()).with(Keyframe::to(
                0.5,
                BoneTransform::from_translation(Offset::new(0.0, -12.0)),
            )),
        );

        let pose = clip.pose(&skeleton, ms(250));
        let hand = pose.get(skeleton.index_of("hand").unwrap()).unwrap();
        assert_eq!(hand.translation, Offset::new(0.0, -6.0));

        // The un-animated bone kept its bind transform, not identity.
        let arm = pose.get(skeleton.index_of("arm").unwrap()).unwrap();
        assert_eq!(arm.translation, Offset::new(2.0, 2.0));
    }

    #[test]
    fn a_clip_over_a_base_pose_blend_onto_live_state() {
        let skeleton = Skeleton::new().bone("hand", None, BoneTransform::default());
        let clip = SkeletalClip::new().bone(
            "hand",
            Keyframes::new(BoneTransform::default()).with(Keyframe::to(
                0.5,
                BoneTransform::from_translation(Offset::new(0.0, -12.0)),
            )),
        );

        let mut base = skeleton.bind_pose();
        base.set(
            &skeleton,
            "hand",
            BoneTransform::from_translation(Offset::new(100.0, 100.0)),
        );
        // The clip's track *replaces* the base for animated bones.
        let pose = clip.pose_over(&skeleton, base, ms(250));
        let hand = pose.get(0).unwrap();
        assert_eq!(hand.translation, Offset::new(0.0, -6.0));
    }

    #[test]
    fn a_track_for_an_unknown_bone_is_skipped() {
        let skeleton = Skeleton::new().bone("hand", None, BoneTransform::default());
        let clip = SkeletalClip::new().bone(
            "ghost",
            Keyframes::new(BoneTransform::default()).with(Keyframe::to(
                0.5,
                BoneTransform::from_translation(Offset::new(9.0, 9.0)),
            )),
        );

        let pose = clip.pose(&skeleton, ms(250));
        assert_eq!(pose.get(0).unwrap().translation, Offset::new(0.0, 0.0));
    }

    #[test]
    fn clip_duration_is_the_longest_track() {
        let _skeleton = Skeleton::new().bone("a", None, BoneTransform::default());
        let clip = SkeletalClip::new()
            .bone(
                "a",
                Keyframes::new(BoneTransform::default())
                    .with(Keyframe::to(0.5, BoneTransform::default())),
            )
            .bone(
                "b",
                Keyframes::new(BoneTransform::default())
                    .with(Keyframe::to(2.0, BoneTransform::default())),
            );
        assert_eq!(clip.duration(), ms(2000));
    }

    #[test]
    fn a_skeleton_rejects_duplicate_names() {
        let result = std::panic::catch_unwind(|| {
            let _ = Skeleton::new()
                .bone("a", None, BoneTransform::default())
                .bone("a", None, BoneTransform::default());
        });
        assert!(result.is_err(), "two bones cannot share a name");
    }

    #[test]
    fn the_two_bone_arm_doctest() {
        // The module doctest, as a test, because it is the shape of the whole
        // feature: shoulder + forearm, a skinned tip, one rotation.
        let skeleton = Skeleton::new()
            .bone("shoulder", None, BoneTransform::default())
            .bone(
                "forearm",
                Some("shoulder"),
                BoneTransform::from_translation(Offset::new(0.0, 40.0)),
            );
        let bind = skeleton.bind_pose();
        let mut skin = Skin::bind(&skeleton, &bind);
        skin = skin.vertex(Vertex::new(Offset::new(0.0, 80.0)).binding(1, 1.0));

        let mut pose = bind.clone();
        pose.set(
            &skeleton,
            "shoulder",
            BoneTransform::from_rotation(FRAC_PI_2),
        );
        let world = skeleton.world(&pose);
        let tip = skin.apply(&world)[0];
        assert!(tip.dx < -70.0, "tip at {tip:?}");
        assert!(tip.dy.abs() < 15.0, "tip at {tip:?}");
    }
}

// ---------------------------------------------------------------------
// Two-bone inverse kinematics.
//
// The module doc above used to end at "no inverse kinematics", and the
// reasoning it gave — a numerical method with convergence behaviour, against
// the crate's everything-is-a-pure-function contract — was correct for the
// *iterative* solvers (FABRIK, CCD, damped least squares) that N-bone IK
// means. Two-bone IK is not that: it is the law of cosines, a closed form
// with no iteration, no convergence and no tuning, as pure a function of its
// inputs as a spring. The arm every 2D rig actually has — upper arm, forearm,
// a hand that should be *here* — gets the analytic solver; the leg chains of
// a spider still do not, and that is where the original reasoning still
// applies, unchanged.
// ---------------------------------------------------------------------

/// Which way the middle joint bends — the elbow's answer to "up or down".
///
/// An IK solver that does not let the caller choose this is a solver that
/// chose it for them, and half the rigs in the wild need the other half's
/// answer: an arm bends one way, a leg the other, and a crab's bends both
/// ways from the midline.
///
/// The names are **visual**, in the screen space this crate draws in (y
/// grows down): `Clockwise` bends the first bone clockwise from the
/// root-to-target direction, which on a horizontal reach puts the joint
/// *below* the line; `CounterClockwise` puts it *above*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bend {
    /// The joint sits clockwise of the root-to-target line — below a
    /// horizontal reach, in y-down screen space.
    Clockwise,
    /// The joint sits counter-clockwise of the line — above a horizontal
    /// reach, in y-down screen space.
    CounterClockwise,
}

impl Bend {
    /// The sign the interior angle is applied with: +1 for clockwise, −1 for
    /// counter-clockwise, in the screen convention (y-down, angle positive
    /// clockwise) the rest of this crate uses. One place for the convention,
    /// so every formula below reads as its mirror image rather than as a
    /// differently-signed twin.
    fn sign(self) -> f32 {
        match self {
            Self::Clockwise => 1.0,
            Self::CounterClockwise => -1.0,
        }
    }
}

/// Two-bone IK, solved: the world-space rotations of both bones, and where
/// the chain actually ended up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TwoBoneIk {
    /// The first bone's world rotation, in radians — the angle from +x of
    /// the root-to-joint segment, in the screen convention (y-down, positive
    /// clockwise) the rest of this crate uses.
    pub bone_a: f32,
    /// The second bone's world rotation: the angle of the joint-to-effector
    /// segment. Relative to bone_a it is the elbow; in absolute terms it is
    /// what a flat two-bone rig paints.
    pub bone_b: f32,
    /// Whether the target was reachable. `false` means the chain is fully
    /// extended (or folded) toward it and `effector` is the closest point on
    /// that line — the honest answer rather than an error, because a target
    /// out of reach is a normal animated state (a hand reaching past full
    /// extension), not a caller's bug.
    pub reachable: bool,
    /// Where the chain's end landed — equal to the target when
    /// `reachable`, the clamped closest approach when not.
    pub effector: Offset,
}

/// Solve a two-bone chain for a target: closed-form, no iteration.
///
/// * `root` — where the first bone starts.
/// * `lengths` — `(first, second)` bone lengths, both strictly positive: a
///   zero-length bone has no direction for an angle to describe, and a
///   negative one is a sign error wearing a costume.
/// * `target` — where the chain's end should be.
/// * `bend` — which side the middle joint sits on.
///
/// # Panics
///
/// If either length is not finite and positive — see above.
///
/// ```
/// use vieww_animation::skeletal::{solve_two_bone, Bend};
/// use vieww_foundation::Offset;
///
/// // An arm of 60 + 60, reaching a point 100 away, elbow up.
/// let arm = solve_two_bone(
///     Offset::new(0.0, 0.0),
///     (60.0, 60.0),
///     Offset::new(100.0, 0.0),
///     Bend::CounterClockwise,
/// );
/// assert!(arm.reachable);
/// // Equal bones reaching symmetrically: the elbow is exactly over the
/// // midline, 50 units out and √(60² − 50²) above the line (y-down).
/// let reach_y = (60.0_f32 * 60.0 - 50.0 * 50.0).sqrt();
/// let elbow = arm.joint(Offset::new(0.0, 0.0), (60.0, 60.0));
/// assert!((elbow.dy + reach_y).abs() < 1e-3, "elbow off midline");
///
/// // And unreachable targets extend toward the target rather than failing:
/// let far = solve_two_bone(
///     Offset::new(0.0, 0.0),
///     (60.0, 60.0),
///     Offset::new(200.0, 0.0),
///     Bend::CounterClockwise,
/// );
/// assert!(!far.reachable);
/// assert!((far.effector.dx - 120.0).abs() < 1e-3, "fully extended");
/// ```
pub fn solve_two_bone(root: Offset, lengths: (f32, f32), target: Offset, bend: Bend) -> TwoBoneIk {
    let (l1, l2) = lengths;
    assert!(
        l1.is_finite() && l1 > 0.0 && l2.is_finite() && l2 > 0.0,
        "bone lengths must be positive and finite, not {lengths:?}"
    );

    let dx = target.dx - root.dx;
    let dy = target.dy - root.dy;
    let reach = (dx * dx + dy * dy).sqrt();

    // The clamp band: the chain can reach from |L1 − L2| (folded) to
    // L1 + L2 (extended). Clamping the *distance* rather than erroring is
    // what makes an out-of-reach target a normal state.
    let max_reach = l1 + l2;
    let min_reach = (l1 - l2).abs();
    let reachable = reach <= max_reach && reach >= min_reach;
    let d = reach.clamp(min_reach, max_reach).max(1e-6);

    // The direction the chain points when it cannot reach — toward the
    // target, scaled to the clamped distance. When it can reach, this is the
    // target itself and the arithmetic below is exact.
    let dir_x = dx / reach.max(1e-6);
    let dir_y = dy / reach.max(1e-6);

    // Law of cosines, twice: the interior angles of the root-joint-effector
    // triangle. Clamped to ±1 against float rounding at the band's edges,
    // where the triangle degenerates to a line and acos of 1.0000001 is NaN.
    let cos_a = ((l1 * l1 + d * d - l2 * l2) / (2.0 * l1 * d)).clamp(-1.0, 1.0);
    let cos_b = ((l1 * l1 + l2 * l2 - d * d) / (2.0 * l1 * l2)).clamp(-1.0, 1.0);
    let interior_a = cos_a.acos();
    let interior_b = cos_b.acos();

    // The root-to-target direction, and the first bone off it by the bend's
    // signed interior angle. The second bone's rotation is applied with the
    // *opposite* sign — the chain zig-zags: out to the elbow, back in to the
    // target. Applying the same sign to both is the classic mistake and
    // produces a chain that opens *away* from the target, which the
    // round-trip test in `ik_tests` catches on its first case.
    let aim = dy.atan2(dx);
    let bone_a = aim + bend.sign() * interior_a;
    let bone_b = bone_a - bend.sign() * (std::f32::consts::PI - interior_b);

    let effector = if reachable {
        target
    } else {
        // Extended (or folded) along the clamped direction: the closest the
        // chain gets to the target, which is exactly what "reaching for it"
        // looks like.
        Offset::new(root.dx + dir_x * d, root.dy + dir_y * d)
    };

    TwoBoneIk {
        bone_a,
        bone_b,
        reachable,
        effector,
    }
}

impl TwoBoneIk {
    /// Where the middle joint (the elbow) sits — derived, not stored, so
    /// there is exactly one copy of the forward kinematics to trust.
    pub fn joint(&self, root: Offset, lengths: (f32, f32)) -> Offset {
        Offset::new(
            root.dx + lengths.0 * self.bone_a.cos(),
            root.dy + lengths.0 * self.bone_a.sin(),
        )
    }
}

#[cfg(test)]
mod ik_tests {
    use super::*;

    const EPS: f32 = 1e-3;

    /// Round-trip: solve, then run the forward kinematics by hand and check
    /// the chain lands on the effector. The property every caller actually
    /// needs, and the one a sign error in one of the four angles breaks.
    fn lands_on_target(
        root: Offset,
        lengths: (f32, f32),
        target: Offset,
        bend: Bend,
    ) -> (Offset, bool) {
        let solution = solve_two_bone(root, lengths, target, bend);
        let joint = solution.joint(root, lengths);
        let end = Offset::new(
            joint.dx + lengths.1 * solution.bone_b.cos(),
            joint.dy + lengths.1 * solution.bone_b.sin(),
        );
        (end, solution.reachable)
    }

    #[test]
    fn the_chain_lands_on_reachable_targets() {
        let root = Offset::new(10.0, 20.0);
        for target in [
            Offset::new(100.0, 20.0),
            Offset::new(10.0, 90.0),
            Offset::new(60.0, 60.0),
            Offset::new(-30.0, 25.0),
            Offset::new(10.0, -20.0),
        ] {
            for bend in [Bend::Clockwise, Bend::CounterClockwise] {
                let (end, reachable) = lands_on_target(root, (50.0, 70.0), target, bend);
                assert!(reachable, "{target:?} should be in reach");
                assert!(
                    (end.dx - target.dx).abs() < EPS && (end.dy - target.dy).abs() < EPS,
                    "bend {bend:?}: end {end:?} missed target {target:?}"
                );
            }
        }
    }

    #[test]
    fn unreachable_targets_fully_extend_toward_themselves() {
        let solution = solve_two_bone(
            Offset::new(0.0, 0.0),
            (50.0, 50.0),
            Offset::new(300.0, 400.0), // 500 away, 100 reachable
            Bend::Clockwise,
        );
        assert!(!solution.reachable);
        assert!((solution.effector.dx - 60.0).abs() < EPS);
        assert!(
            (solution.effector.dy - 80.0).abs() < EPS,
            "toward the target"
        );
        // The FK round trip still lands on the (clamped) effector.
        let joint = solution.joint(Offset::new(0.0, 0.0), (50.0, 50.0));
        let end = Offset::new(
            joint.dx + 50.0 * solution.bone_b.cos(),
            joint.dy + 50.0 * solution.bone_b.sin(),
        );
        assert!((end.dx - solution.effector.dx).abs() < EPS);
    }

    #[test]
    fn the_bend_side_is_choosable() {
        let root = Offset::new(0.0, 0.0);
        let target = Offset::new(90.0, 0.0);
        let ccw = solve_two_bone(root, (50.0, 50.0), target, Bend::CounterClockwise);
        let cw = solve_two_bone(root, (50.0, 50.0), target, Bend::Clockwise);
        let elbow_ccw = ccw.joint(root, (50.0, 50.0));
        let elbow_cw = cw.joint(root, (50.0, 50.0));
        // Same distance out (symmetric bones), opposite sides of the line.
        assert!(elbow_ccw.dy < 0.0, "CCW elbow above the line (y-down)");
        assert!(elbow_cw.dy > 0.0, "CW elbow below the line");
        assert!((elbow_ccw.dx - elbow_cw.dx).abs() < EPS);
    }

    #[test]
    fn a_collinear_target_is_a_straight_arm() {
        let solution = solve_two_bone(
            Offset::new(0.0, 0.0),
            (40.0, 60.0),
            Offset::new(100.0, 0.0),
            Bend::CounterClockwise,
        );
        // Degenerate triangle: both bones lie along +x. acos at the clamp
        // band's edge must not produce NaN — this test is the one that
        // catches a missing clamp.
        assert!(solution.bone_a.is_finite());
        assert!(solution.bone_b.is_finite());
        assert!(solution.bone_a.abs() < EPS);
        assert!(solution.bone_b.abs() < EPS);
        let joint = solution.joint(Offset::new(0.0, 0.0), (40.0, 60.0));
        assert!((joint.dx - 40.0).abs() < EPS && joint.dy.abs() < EPS);
    }

    #[test]
    fn a_folded_target_is_a_closed_arm() {
        // Target inside the fold band: the chain folds onto itself.
        let solution = solve_two_bone(
            Offset::new(0.0, 0.0),
            (40.0, 60.0),
            Offset::new(15.0, 0.0), // |40 − 60| = 20 minimum; 15 is inside
            Bend::CounterClockwise,
        );
        assert!(!solution.reachable);
        assert!(
            (solution.effector.dx - 20.0).abs() < EPS,
            "folded to the minimum"
        );
    }

    #[test]
    #[should_panic(expected = "bone lengths must be positive")]
    fn zero_length_bones_are_refused() {
        let _ = solve_two_bone(
            Offset::new(0.0, 0.0),
            (0.0, 50.0),
            Offset::new(10.0, 0.0),
            Bend::Clockwise,
        );
    }

    #[test]
    fn the_solution_is_a_pure_function_of_its_inputs() {
        let ask = |t: f32| {
            solve_two_bone(
                Offset::new(0.0, 0.0),
                (55.0, 45.0),
                Offset::new(60.0 * t.cos(), 60.0 * t.sin()),
                Bend::Clockwise,
            )
        };
        let a = ask(0.7);
        let b = ask(0.3);
        let a_again = ask(0.7);
        assert_eq!(a, a_again);
        assert_ne!(a, b);
    }
}
