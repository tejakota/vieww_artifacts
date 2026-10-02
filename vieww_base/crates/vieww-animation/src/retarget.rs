//! Retargeting and root motion — Unity's Avatar layer and root-motion layer
//! (§2.7 L5 and L7), in the 2D bone model of [`skeletal`](crate::skeletal).
//!
//! # Retargeting
//!
//! Unity converts a humanoid clip to *muscle space* — rotations relative to
//! the rest pose, normalised — so one walk plays on a tall rig and a short
//! one. The 2D equivalent is exactly that decomposition: a pose is taken as
//! **rotation deltas from rest** (which transfer unchanged, because an elbow
//! bending 40° is an elbow bending 40° on any arm) plus **translation deltas
//! from rest scaled by the bone-length ratio** (a hip that bobs 10 px on a
//! 100 px-leg rig bobs 15 px on a 150 px one). Bones are matched by a
//! [`BoneMap`] of source name → target name, so rigs from different tools
//! with different naming line up.
//!
//! # Root motion
//!
//! A walk clip authored "in place" slides; one authored moving forward runs
//! off its own origin. Root motion is the middle road: the clip moves its
//! root bone, the engine *extracts* that movement each frame, strips it from
//! the pose, and applies it to the character's world position instead —
//! so collisions and gameplay own where the character is, and the feet
//! still plant. [`root_motion`] is the per-frame delta, looping-aware (a
//! loop boundary adds one whole cycle's displacement, not a teleport back);
//! [`strip_root`] is the in-place pose.
//!
//! # Channels
//!
//! [`pose_to_channels`] / [`channels_to_pose`] carry a pose into the
//! [`Animator`](crate::animator::Animator)'s channel maps and back, so layer
//! masks can be written in bone names ("arm_l." masks the left arm).

use std::time::Duration;

use vieww_foundation::Offset;

use crate::animator::Channels;
use crate::skeletal::{BoneTransform, Pose, SkeletalClip, Skeleton};

/// Every bone's transform as `"<bone>.tx"`, `.ty`, `.rot`, `.sx`, `.sy`.
#[must_use]
pub fn pose_to_channels(skeleton: &Skeleton, pose: &Pose) -> Channels {
    let mut out = Channels::new();
    for i in 0..skeleton.len() {
        let (Some(name), Some(t)) = (skeleton.name_of(i), pose.get(i)) else {
            continue;
        };
        out.insert(format!("{name}.tx"), t.translation.dx);
        out.insert(format!("{name}.ty"), t.translation.dy);
        out.insert(format!("{name}.rot"), t.rotation);
        out.insert(format!("{name}.sx"), t.scale.0);
        out.insert(format!("{name}.sy"), t.scale.1);
    }
    out
}

/// Apply whatever channels are present over `base`.
#[must_use]
pub fn channels_to_pose(skeleton: &Skeleton, channels: &Channels, base: &Pose) -> Pose {
    let mut pose = base.clone();
    for i in 0..skeleton.len() {
        let Some(name) = skeleton.name_of(i) else {
            continue;
        };
        let mut t = pose.get(i).unwrap_or_default();
        let get = |c: &str| channels.get(&format!("{name}.{c}")).copied();
        if let Some(v) = get("tx") {
            t.translation.dx = v;
        }
        if let Some(v) = get("ty") {
            t.translation.dy = v;
        }
        if let Some(v) = get("rot") {
            t.rotation = v;
        }
        if let Some(v) = get("sx") {
            t.scale.0 = v;
        }
        if let Some(v) = get("sy") {
            t.scale.1 = v;
        }
        pose.set_index(i, t);
    }
    pose
}

/// Source bone name → target bone name.
#[derive(Debug, Clone, Default)]
pub struct BoneMap {
    pairs: Vec<(String, String)>,
}

impl BoneMap {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Map `source` onto `target`.
    #[must_use]
    pub fn map(mut self, source: &str, target: &str) -> Self {
        self.pairs.push((source.to_owned(), target.to_owned()));
        self
    }

    /// Every bone name present in both skeletons, mapped to itself.
    #[must_use]
    pub fn by_name(source: &Skeleton, target: &Skeleton) -> Self {
        let mut m = Self::new();
        for i in 0..source.len() {
            if let Some(n) = source.name_of(i) {
                if target.index_of(n).is_some() {
                    m = m.map(n, n);
                }
            }
        }
        m
    }
}

/// Play `pose` (on `source`) onto `target` — rotation deltas copied, scale
/// ratios copied, translation deltas scaled by rest-length ratio. Target
/// bones with no mapping keep their rest pose.
#[must_use]
pub fn retarget(source: &Skeleton, pose: &Pose, target: &Skeleton, map: &BoneMap) -> Pose {
    let src_rest = source.bind_pose();
    let dst_rest = target.bind_pose();
    let mut out = dst_rest.clone();
    for (s, d) in &map.pairs {
        let (Some(si), Some(di)) = (source.index_of(s), target.index_of(d)) else {
            continue;
        };
        let (Some(sr), Some(sp), Some(dr)) = (src_rest.get(si), pose.get(si), dst_rest.get(di))
        else {
            continue;
        };
        let src_len = sr.translation.distance();
        let dst_len = dr.translation.distance();
        let ratio = if src_len > 1e-6 {
            dst_len / src_len
        } else {
            1.0
        };
        let delta = sp.translation - sr.translation;
        let ratio_scale = |a: f32, b: f32| if b.abs() > 1e-6 { a / b } else { 1.0 };
        out.set_index(
            di,
            BoneTransform {
                translation: dr.translation + delta.scale(ratio),
                rotation: dr.rotation + (sp.rotation - sr.rotation),
                scale: (
                    dr.scale.0 * ratio_scale(sp.scale.0, sr.scale.0),
                    dr.scale.1 * ratio_scale(sp.scale.1, sr.scale.1),
                ),
            },
        );
    }
    out
}

/// The root bone's displacement and rotation change from clip time `from`
/// to `to` (seconds). With `looping`, crossing the clip's end adds whole
/// cycles of displacement.
#[must_use]
pub fn root_motion(
    clip: &SkeletalClip,
    skeleton: &Skeleton,
    root: &str,
    from: f32,
    to: f32,
    looping: bool,
) -> (Offset, f32) {
    let Some(ri) = skeleton.index_of(root) else {
        return (Offset::ZERO, 0.0);
    };
    let len = clip.duration().as_secs_f32();
    let at = |t: f32| {
        clip.pose(
            skeleton,
            Duration::from_secs_f32(t.clamp(0.0, len.max(0.0))),
        )
        .get(ri)
        .unwrap_or_default()
    };
    if !looping || len <= 0.0 {
        let (a, b) = (at(from), at(to));
        return (b.translation - a.translation, b.rotation - a.rotation);
    }
    let cycle = at(len);
    let start = at(0.0);
    let per_cycle = (
        cycle.translation - start.translation,
        cycle.rotation - start.rotation,
    );
    let pos = |t: f32| {
        let n = (t / len).floor();
        let local = at(t - n * len);
        (
            local.translation + per_cycle.0.scale(n),
            local.rotation + per_cycle.1 * n,
        )
    };
    let (a, b) = (pos(from), pos(to));
    (b.0 - a.0, b.1 - a.1)
}

/// The pose with the root bone's translation reset to its rest value —
/// "in place", ready for root motion to move the character instead.
#[must_use]
pub fn strip_root(skeleton: &Skeleton, pose: &Pose, root: &str) -> Pose {
    let mut out = pose.clone();
    if let (Some(i), Some(rest)) = (
        skeleton.index_of(root),
        skeleton
            .index_of(root)
            .and_then(|i| skeleton.bind_pose().get(i)),
    ) {
        if let Some(mut t) = out.get(i) {
            t.translation = rest.translation;
            out.set_index(i, t);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyframe::{Keyframe, Keyframes};
    use crate::Curve;

    fn leg(len: f32) -> Skeleton {
        Skeleton::new()
            .bone(
                "hip",
                None,
                BoneTransform::from_translation(Offset::new(0.0, 0.0)),
            )
            .bone(
                "knee",
                Some("hip"),
                BoneTransform::from_translation(Offset::new(0.0, len)),
            )
            .bone(
                "foot",
                Some("knee"),
                BoneTransform::from_translation(Offset::new(0.0, len)),
            )
    }

    #[test]
    fn channels_round_trip_a_pose() {
        let s = leg(50.0);
        let mut p = s.bind_pose();
        p.set(
            &s,
            "knee",
            BoneTransform {
                rotation: 0.7,
                ..BoneTransform::from_translation(Offset::new(1.0, 50.0))
            },
        );
        let c = pose_to_channels(&s, &p);
        assert_eq!(c["knee.rot"], 0.7);
        let back = channels_to_pose(&s, &c, &s.bind_pose());
        assert_eq!(back.get(1), p.get(1));
    }

    #[test]
    fn retarget_copies_rotations_and_scales_translations() {
        let short = leg(50.0);
        let tall = leg(100.0);
        let mut p = short.bind_pose();
        p.set(
            &short,
            "knee",
            BoneTransform {
                rotation: 0.4,
                ..BoneTransform::from_translation(Offset::new(10.0, 50.0))
            },
        );
        let out = retarget(&short, &p, &tall, &BoneMap::by_name(&short, &tall));
        let knee = out.get(1).unwrap();
        assert!((knee.rotation - 0.4).abs() < 1e-6);
        assert!(
            (knee.translation.dx - 20.0).abs() < 1e-4,
            "delta doubled with the bone"
        );
        assert!((knee.translation.dy - 100.0).abs() < 1e-4);
    }

    #[test]
    fn retarget_follows_a_name_map() {
        let a = Skeleton::new().bone(
            "Thigh_L",
            None,
            BoneTransform::from_translation(Offset::new(0.0, 10.0)),
        );
        let b = Skeleton::new().bone(
            "leg.l",
            None,
            BoneTransform::from_translation(Offset::new(0.0, 10.0)),
        );
        let mut p = a.bind_pose();
        p.set(
            &a,
            "Thigh_L",
            BoneTransform {
                rotation: 1.0,
                ..BoneTransform::from_translation(Offset::new(0.0, 10.0))
            },
        );
        let out = retarget(&a, &p, &b, &BoneMap::new().map("Thigh_L", "leg.l"));
        assert!((out.get(0).unwrap().rotation - 1.0).abs() < 1e-6);
    }

    fn walk() -> (Skeleton, SkeletalClip) {
        let s = leg(50.0);
        let clip = SkeletalClip::new().bone(
            "hip",
            Keyframes::new(BoneTransform::from_translation(Offset::ZERO)).with(
                Keyframe::to(1.0, BoneTransform::from_translation(Offset::new(40.0, 0.0)))
                    .curve(Curve::Linear),
            ),
        );
        (s, clip)
    }

    #[test]
    fn root_motion_accumulates_across_loops() {
        let (s, clip) = walk();
        let (d, _) = root_motion(&clip, &s, "hip", 0.25, 0.5, false);
        assert!((d.dx - 10.0).abs() < 1e-3);
        let (d, _) = root_motion(&clip, &s, "hip", 0.75, 1.25, true);
        assert!((d.dx - 20.0).abs() < 1e-3, "across the wrap: {d:?}");
        let (d, _) = root_motion(&clip, &s, "hip", 0.0, 3.0, true);
        assert!((d.dx - 120.0).abs() < 1e-3);
    }

    #[test]
    fn strip_root_puts_the_root_back_at_rest() {
        let (s, clip) = walk();
        let p = clip.pose(&s, Duration::from_secs_f32(0.5));
        assert!(p.get(0).unwrap().translation.dx > 0.0);
        assert_eq!(
            strip_root(&s, &p, "hip").get(0).unwrap().translation,
            Offset::ZERO
        );
    }
}
