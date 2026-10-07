//! Skinned and morphing meshes — Three.js' `SkinnedMesh` + `morphTargetInfluences`,
//! Blender's Armature modifier and shape keys, glTF's `skins` and `targets`.
//!
//! A [`SkinnedMesh`] is a bind-pose [`Mesh`], up to four joint influences per
//! vertex, one inverse bind matrix per joint, the scene nodes that *are* the
//! joints, and any number of morph targets. Each frame the renderer asks
//! [`deform`] for the posed mesh:
//!
//! 1. **morph** — `p = base + Σ wᵢ · Δpᵢ` (and normals likewise), the shape keys;
//! 2. **skin** — `p' = Σ wⱼ · Jⱼ · p` with the joint matrix
//!    `Jⱼ = meshWorld⁻¹ · jointWorldⱼ · inverseBindⱼ` (glTF §3.7.3).
//!
//! [`Skinning::Linear`] is classic linear-blend skinning; [`Skinning::DualQuaternion`]
//! blends rigid transforms as dual quaternions (Kavan et al. 2007), which keeps
//! volume at twisting joints where LBS collapses into the "candy-wrapper".
//! The test suite measures that collapse.
//!
//! Skinning is a pure function of (mesh, joint matrices, weights) and runs on
//! the CPU like the rest of this crate's reference renderer.

use std::sync::Arc;

use vieww_mesh::gltf::MorphTarget;
use vieww_mesh::Mesh;

use crate::math::{Mat4, Quat, Vec3};
use crate::scene::NodeId;

/// Which blend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Skinning {
    #[default]
    Linear,
    DualQuaternion,
}

/// A mesh bound to joints, with optional morph targets.
#[derive(Debug, Clone, PartialEq)]
pub struct SkinnedMesh {
    pub base: Mesh,
    pub joints: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
    /// The scene nodes driving each joint.
    pub bones: Vec<NodeId>,
    pub inverse_bind: Vec<Mat4>,
    pub targets: Vec<MorphTarget>,
    pub skinning: Skinning,
}

impl SkinnedMesh {
    /// A morph-only mesh (no joints).
    #[must_use]
    pub fn morphing(base: Mesh, targets: Vec<MorphTarget>) -> Self {
        Self {
            base,
            joints: Vec::new(),
            weights: Vec::new(),
            bones: Vec::new(),
            inverse_bind: Vec::new(),
            targets,
            skinning: Skinning::Linear,
        }
    }

    /// A skinned mesh; `joints`/`weights` per vertex, one bind matrix per bone.
    #[must_use]
    pub fn skinned(
        base: Mesh,
        joints: Vec<[u16; 4]>,
        weights: Vec<[f32; 4]>,
        bones: Vec<NodeId>,
        inverse_bind: Vec<Mat4>,
    ) -> Self {
        Self {
            base,
            joints,
            weights,
            bones,
            inverse_bind,
            targets: Vec::new(),
            skinning: Skinning::Linear,
        }
    }

    #[must_use]
    pub fn with_skinning(mut self, s: Skinning) -> Self {
        self.skinning = s;
        self
    }

    #[must_use]
    pub fn with_targets(mut self, t: Vec<MorphTarget>) -> Self {
        self.targets = t;
        self
    }

    /// Whether any joint is bound.
    #[must_use]
    pub fn is_skinned(&self) -> bool {
        !self.bones.is_empty() && self.joints.len() == self.base.positions.len()
    }

    /// Shared handle for scene content.
    #[must_use]
    pub fn shared(self) -> Arc<Self> {
        Arc::new(self)
    }
}

/// Apply the morph targets: `base + Σ w·Δ`.
#[must_use]
pub fn morph(m: &SkinnedMesh, weights: &[f32]) -> Mesh {
    let mut out = m.base.clone();
    for (t, &w) in m.targets.iter().zip(weights) {
        if w == 0.0 {
            continue;
        }
        for (p, d) in out.positions.iter_mut().zip(&t.positions) {
            for k in 0..3 {
                p[k] += w * d[k];
            }
        }
        if out.has_normals() {
            for (n, d) in out.normals.iter_mut().zip(&t.normals) {
                for k in 0..3 {
                    n[k] += w * d[k];
                }
            }
        }
    }
    out
}

/// A rigid transform as a unit dual quaternion `(real, dual)`.
#[derive(Debug, Clone, Copy)]
struct Dq {
    r: [f32; 4],
    d: [f32; 4],
}

fn qmul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let (ax, ay, az, aw) = (a[0], a[1], a[2], a[3]);
    let (bx, by, bz, bw) = (b[0], b[1], b[2], b[3]);
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

/// Rotation of a (rigid, possibly scaled) matrix as a quaternion.
fn mat_rotation(m: &Mat4) -> Quat {
    let col = |c: usize| Vec3::new(m.m[c][0], m.m[c][1], m.m[c][2]).normalize();
    let (c0, c1, c2) = (col(0), col(1), col(2));
    let trace = c0.x + c1.y + c2.z;
    if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        Quat::new((c1.z - c2.y) / s, (c2.x - c0.z) / s, (c0.y - c1.x) / s, 0.25 * s)
    } else if c0.x > c1.y && c0.x > c2.z {
        let s = (1.0 + c0.x - c1.y - c2.z).sqrt() * 2.0;
        Quat::new(0.25 * s, (c1.x + c0.y) / s, (c2.x + c0.z) / s, (c1.z - c2.y) / s)
    } else if c1.y > c2.z {
        let s = (1.0 + c1.y - c0.x - c2.z).sqrt() * 2.0;
        Quat::new((c1.x + c0.y) / s, 0.25 * s, (c2.y + c1.z) / s, (c2.x - c0.z) / s)
    } else {
        let s = (1.0 + c2.z - c0.x - c1.y).sqrt() * 2.0;
        Quat::new((c2.x + c0.z) / s, (c2.y + c1.z) / s, 0.25 * s, (c0.y - c1.x) / s)
    }
    .normalize()
}

impl Dq {
    fn from_mat(m: &Mat4) -> Self {
        let q = mat_rotation(m);
        let r = [q.x, q.y, q.z, q.w];
        let t = m.get_translation();
        let d = qmul([t.x, t.y, t.z, 0.0], r).map(|v| v * 0.5);
        Self { r, d }
    }

    fn transform(self, p: Vec3) -> Vec3 {
        let q = Quat::new(self.r[0], self.r[1], self.r[2], self.r[3]);
        let conj = [-self.r[0], -self.r[1], -self.r[2], self.r[3]];
        let t = qmul(self.d, conj).map(|v| v * 2.0);
        q.rotate(p) + Vec3::new(t[0], t[1], t[2])
    }

    fn rotate(self, v: Vec3) -> Vec3 {
        Quat::new(self.r[0], self.r[1], self.r[2], self.r[3]).rotate(v)
    }
}

/// The joint matrices for one frame: `meshWorld⁻¹ · jointWorld · inverseBind`.
#[must_use]
pub fn joint_matrices(mesh_world: &Mat4, joint_worlds: &[Mat4], inverse_bind: &[Mat4]) -> Vec<Mat4> {
    let inv = mesh_world.inverse().unwrap_or(Mat4::IDENTITY);
    joint_worlds
        .iter()
        .zip(inverse_bind)
        .map(|(j, ib)| inv * *j * *ib)
        .collect()
}

/// Morph, then skin. `joint_mats` are from [`joint_matrices`]; with no joints
/// bound this is [`morph`] alone.
#[must_use]
pub fn deform(m: &SkinnedMesh, joint_mats: &[Mat4], morph_weights: &[f32]) -> Mesh {
    let mut out = morph(m, morph_weights);
    if !m.is_skinned() || joint_mats.is_empty() {
        return out;
    }
    let has_n = out.has_normals();
    match m.skinning {
        Skinning::Linear => {
            for i in 0..out.positions.len() {
                let (js, ws) = (m.joints[i], m.weights[i]);
                let p = Vec3::from_array(out.positions[i]);
                let mut acc = Vec3::ZERO;
                let mut nacc = Vec3::ZERO;
                let n = if has_n {
                    Vec3::from_array(out.normals[i])
                } else {
                    Vec3::ZERO
                };
                for k in 0..4 {
                    if ws[k] == 0.0 {
                        continue;
                    }
                    let jm = joint_mats.get(js[k] as usize).copied().unwrap_or(Mat4::IDENTITY);
                    acc += jm.transform_point(p) * ws[k];
                    if has_n {
                        nacc += jm.transform_vector(n) * ws[k];
                    }
                }
                out.positions[i] = [acc.x, acc.y, acc.z];
                if has_n {
                    let nn = nacc.normalize();
                    out.normals[i] = [nn.x, nn.y, nn.z];
                }
            }
        }
        Skinning::DualQuaternion => {
            let dqs: Vec<Dq> = joint_mats.iter().map(Dq::from_mat).collect();
            for i in 0..out.positions.len() {
                let (js, ws) = (m.joints[i], m.weights[i]);
                let pivot = dqs.get(js[0] as usize).map_or([0.0, 0.0, 0.0, 1.0], |d| d.r);
                let (mut r, mut d) = ([0.0f32; 4], [0.0f32; 4]);
                for k in 0..4 {
                    if ws[k] == 0.0 {
                        continue;
                    }
                    let Some(q) = dqs.get(js[k] as usize) else {
                        continue;
                    };
                    // Antipodality: blend in the pivot's hemisphere.
                    let dot: f32 = (0..4).map(|c| q.r[c] * pivot[c]).sum();
                    let w = if dot < 0.0 { -ws[k] } else { ws[k] };
                    for c in 0..4 {
                        r[c] += q.r[c] * w;
                        d[c] += q.d[c] * w;
                    }
                }
                let len = r.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-12);
                let dq = Dq {
                    r: r.map(|v| v / len),
                    d: d.map(|v| v / len),
                };
                let p = dq.transform(Vec3::from_array(out.positions[i]));
                out.positions[i] = [p.x, p.y, p.z];
                if has_n {
                    let n = dq.rotate(Vec3::from_array(out.normals[i])).normalize();
                    out.normals[i] = [n.x, n.y, n.z];
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tube along +x from 0 to 2, `rings` rings of `seg` vertices, bound to
    /// two joints at x = 0 and x = 1 with a linear weight ramp over [0.5, 1.5].
    fn tube(rings: usize, seg: usize) -> SkinnedMesh {
        let mut m = Mesh::default();
        let mut joints = Vec::new();
        let mut weights = Vec::new();
        for r in 0..rings {
            #[allow(clippy::cast_precision_loss)]
            let x = 2.0 * r as f32 / (rings - 1) as f32;
            for s in 0..seg {
                #[allow(clippy::cast_precision_loss)]
                let a = std::f32::consts::TAU * s as f32 / seg as f32;
                m.positions.push([x, a.cos() * 0.25, a.sin() * 0.25]);
                m.normals.push([0.0, a.cos(), a.sin()]);
                let w1 = ((x - 0.5) / 1.0).clamp(0.0, 1.0);
                joints.push([0, 1, 0, 0]);
                weights.push([1.0 - w1, w1, 0.0, 0.0]);
            }
        }
        let ib = vec![Mat4::IDENTITY, Mat4::translation(Vec3::new(-1.0, 0.0, 0.0))];
        SkinnedMesh::skinned(m, joints, weights, vec![NodeId(0), NodeId(1)], ib)
    }

    fn mid_radius(mesh: &Mesh, rings: usize, seg: usize) -> f32 {
        let r = rings / 2;
        let ring = &mesh.positions[r * seg..(r + 1) * seg];
        let c = ring.iter().fold([0.0f32; 3], |a, p| [a[0] + p[0], a[1] + p[1], a[2] + p[2]]);
        #[allow(clippy::cast_precision_loss)]
        let c = c.map(|v| v / seg as f32);
        #[allow(clippy::cast_precision_loss)]
        let avg = ring
            .iter()
            .map(|p| ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) + (p[2] - c[2]).powi(2)).sqrt())
            .sum::<f32>()
            / seg as f32;
        avg
    }

    #[test]
    fn bind_pose_is_the_identity() {
        let t = tube(9, 12);
        let worlds = [Mat4::IDENTITY, Mat4::translation(Vec3::new(1.0, 0.0, 0.0))];
        let jm = joint_matrices(&Mat4::IDENTITY, &worlds, &t.inverse_bind);
        for s in [Skinning::Linear, Skinning::DualQuaternion] {
            let out = deform(&t.clone().with_skinning(s), &jm, &[]);
            for (a, b) in out.positions.iter().zip(&t.base.positions) {
                for k in 0..3 {
                    assert!((a[k] - b[k]).abs() < 1e-4, "{s:?}");
                }
            }
        }
    }

    #[test]
    fn a_rigidly_moved_skeleton_moves_the_mesh_rigidly() {
        let t = tube(5, 8);
        let shift = Mat4::translation(Vec3::new(0.0, 3.0, 0.0));
        let worlds = [shift, shift * Mat4::translation(Vec3::new(1.0, 0.0, 0.0))];
        let jm = joint_matrices(&Mat4::IDENTITY, &worlds, &t.inverse_bind);
        let out = deform(&t, &jm, &[]);
        for (a, b) in out.positions.iter().zip(&t.base.positions) {
            assert!((a[1] - b[1] - 3.0).abs() < 1e-4);
        }
    }

    #[test]
    fn twisting_collapses_linear_blend_but_not_dual_quaternions() {
        let (rings, seg) = (9, 16);
        let t = tube(rings, seg);
        // Twist the second joint 170° about the tube's axis.
        let twist = Mat4::translation(Vec3::new(1.0, 0.0, 0.0))
            * Mat4::rotation(Quat::from_axis_angle(Vec3::X, 170f32.to_radians()));
        let jm = joint_matrices(&Mat4::IDENTITY, &[Mat4::IDENTITY, twist], &t.inverse_bind);
        let lbs = deform(&t, &jm, &[]);
        let dqs = deform(&t.clone().with_skinning(Skinning::DualQuaternion), &jm, &[]);
        let (rl, rd) = (mid_radius(&lbs, rings, seg), mid_radius(&dqs, rings, seg));
        assert!(rl < 0.1, "LBS candy-wrapper: radius {rl}");
        assert!((rd - 0.25).abs() < 0.01, "DQS keeps volume: radius {rd}");
    }

    #[test]
    fn morph_targets_add_weighted_deltas() {
        let mut base = Mesh::default();
        base.positions = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
        let up = MorphTarget {
            positions: vec![[0.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
            normals: Vec::new(),
        };
        let out_ = MorphTarget {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
            normals: Vec::new(),
        };
        let m = SkinnedMesh::morphing(base, vec![up, out_]);
        let r = morph(&m, &[0.5, 2.0]);
        assert_eq!(r.positions[0], [0.0, 0.5, 0.0]);
        assert_eq!(r.positions[1], [3.0, 0.5, 0.0]);
        assert_eq!(deform(&m, &[], &[0.0, 0.0]).positions, m.base.positions);
    }

    #[test]
    fn mesh_world_is_factored_out() {
        // A mesh node far from the origin: joints in world space, vertices
        // must come out in the mesh node's local space.
        let t = tube(3, 4);
        let mw = Mat4::translation(Vec3::new(10.0, 0.0, 0.0));
        let worlds = [mw, mw * Mat4::translation(Vec3::new(1.0, 0.0, 0.0))];
        let jm = joint_matrices(&mw, &worlds, &t.inverse_bind);
        let out = deform(&t, &jm, &[]);
        assert!((out.positions[0][0] - t.base.positions[0][0]).abs() < 1e-4);
    }
}
