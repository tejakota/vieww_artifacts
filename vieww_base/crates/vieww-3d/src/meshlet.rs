//! Meshlets and a cluster-LOD chain — the shape of Unreal's Nanite
//! (virtualised geometry, §2.17 L5), meshoptimizer's `buildMeshlets`, and
//! the mesh-shader pipelines that consume them.
//!
//! # What Nanite does, and what this does
//!
//! Nanite splits geometry into small clusters, keeps a hierarchy of
//! simplified versions, and every frame chooses — *per cluster* — the
//! coarsest version whose error is below a pixel, then culls clusters that
//! cannot be seen. Triangle count follows *screen* resolution rather than
//! asset resolution.
//!
//! This module does the same work on the CPU reference path, honestly
//! scoped:
//!
//! * [`build_meshlets`] — greedy, adjacency-ordered clustering into
//!   meshlets of ≤ 64 vertices / ≤ 124 triangles, each with a bounding
//!   sphere and a **normal cone** for backface culling of the whole cluster;
//! * [`ClusterLods`] — a chain of simplified levels (vertex clustering at
//!   doubling cell sizes), each with its geometric error;
//! * [`ClusterLods::select`] — the coarsest level whose projected error is
//!   under the pixel threshold for this camera and distance;
//! * [`ClusterLods::gather`] — frustum + cone culling per meshlet and the
//!   assembled visible mesh, with counts for the render stats.
//!
//! The LOD is chosen per *object* rather than stitched per cluster (no
//! seam-free DAG), which is the documented difference from Nanite.

use std::collections::{HashMap, VecDeque};

use vieww_mesh::modifiers::Modifier;
use vieww_mesh::Mesh;

use crate::math::{Mat4, Vec3};

/// Meshoptimizer's recommended limits.
pub const MAX_VERTICES: usize = 64;
pub const MAX_TRIANGLES: usize = 124;

/// One cluster of triangles.
#[derive(Debug, Clone, PartialEq)]
pub struct Meshlet {
    /// Triangle indices into the level's mesh (three per triangle).
    pub indices: Vec<u32>,
    pub center: Vec3,
    pub radius: f32,
    /// Average face normal (unit), the cone's axis.
    pub cone_axis: Vec3,
    /// `sin` of the cone's half-angle; ≥ 1 means "never cull".
    pub cone_cutoff: f32,
}

impl Meshlet {
    #[must_use]
    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }
}

fn tri_normal(m: &Mesh, t: [u32; 3]) -> Vec3 {
    let p = t.map(|i| Vec3::from_array(m.positions[i as usize]));
    (p[1] - p[0]).cross(p[2] - p[0]).normalize()
}

/// Cluster `mesh` into meshlets.
#[must_use]
pub fn build_meshlets(mesh: &Mesh) -> Vec<Meshlet> {
    let tris: Vec<[u32; 3]> = mesh.indices.as_chunks::<3>().0.to_vec();
    let n = tris.len();
    // Edge → triangles adjacency.
    let mut edge: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (i, t) in tris.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            edge.entry((a.min(b), a.max(b))).or_default().push(i);
        }
    }
    let mut used = vec![false; n];
    let mut out = Vec::new();
    let mut seed = 0;
    while seed < n {
        if used[seed] {
            seed += 1;
            continue;
        }
        let mut verts: Vec<u32> = Vec::new();
        let mut idx: Vec<u32> = Vec::new();
        let mut queue = VecDeque::from([seed]);
        while let Some(t) = queue.pop_front() {
            if used[t] {
                continue;
            }
            let tri = tris[t];
            let new = tri.iter().filter(|v| !verts.contains(v)).count();
            if verts.len() + new > MAX_VERTICES || idx.len() / 3 + 1 > MAX_TRIANGLES {
                continue;
            }
            used[t] = true;
            for v in tri {
                if !verts.contains(&v) {
                    verts.push(v);
                }
            }
            idx.extend(tri);
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                for &nb in &edge[&(a.min(b), a.max(b))] {
                    if !used[nb] {
                        queue.push_back(nb);
                    }
                }
            }
        }
        if idx.is_empty() {
            // A lone triangle that did not fit (impossible with these limits).
            used[seed] = true;
            continue;
        }
        out.push(finish(mesh, idx));
    }
    out
}

fn finish(mesh: &Mesh, indices: Vec<u32>) -> Meshlet {
    let pts: Vec<Vec3> = indices
        .iter()
        .map(|&i| Vec3::from_array(mesh.positions[i as usize]))
        .collect();
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for p in &pts {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let center = (lo + hi) * 0.5;
    let radius = pts
        .iter()
        .map(|p| (*p - center).length())
        .fold(0.0, f32::max);
    let normals: Vec<Vec3> = indices
        .as_chunks::<3>()
        .0
        .iter()
        .map(|t| tri_normal(mesh, *t))
        .collect();
    let sum = normals.iter().fold(Vec3::ZERO, |a, n| a + *n);
    let (cone_axis, cone_cutoff) = if sum.length() < 1e-6 {
        (Vec3::Z, 2.0)
    } else {
        let axis = sum.normalize();
        let min_dot = normals.iter().map(|n| n.dot(axis)).fold(1.0f32, f32::min);
        if min_dot <= 0.0 {
            (axis, 2.0)
        } else {
            (axis, (1.0 - min_dot * min_dot).max(0.0).sqrt())
        }
    };
    Meshlet {
        indices,
        center,
        radius,
        cone_axis,
        cone_cutoff,
    }
}

/// `true` when every triangle of the meshlet faces away from `eye`
/// (meshoptimizer's apex-free cone test).
#[must_use]
pub fn cone_culled(m: &Meshlet, eye: Vec3) -> bool {
    if m.cone_cutoff >= 1.0 {
        return false;
    }
    let d = m.center - eye;
    d.dot(m.cone_axis) >= m.cone_cutoff * d.length() + m.radius
}

/// One simplification level.
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterLevel {
    pub mesh: Mesh,
    pub meshlets: Vec<Meshlet>,
    /// Geometric error in object units (0 for the source level).
    pub error: f32,
}

/// A source mesh and its simplified levels.
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterLods {
    pub levels: Vec<ClusterLevel>,
}

/// What [`ClusterLods::gather`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClusterStats {
    pub level: usize,
    pub clusters: usize,
    pub frustum_culled: usize,
    pub cone_culled: usize,
    pub triangles: usize,
}

impl ClusterLods {
    /// Build `levels` levels: the source, then vertex-clustered versions at
    /// cells of `diag / 2^(k+4)` … coarsening by 2× each step. Levels that
    /// stop reducing are dropped.
    #[must_use]
    pub fn build(mesh: &Mesh, levels: usize) -> Self {
        let diag = mesh
            .bounds()
            .map_or(1.0, |(lo, hi)| {
                (Vec3::from_array(hi) - Vec3::from_array(lo)).length()
            })
            .max(1e-6);
        let mut out = vec![ClusterLevel {
            mesh: mesh.clone(),
            meshlets: build_meshlets(mesh),
            error: 0.0,
        }];
        for k in 0..levels.saturating_sub(1) {
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            let cell = diag / 2f32.powi(5 - k as i32);
            let simplified = Modifier::Decimate { cell }.apply(mesh);
            let prev = out.last().map_or(usize::MAX, |l| l.mesh.triangles());
            if simplified.triangles() == 0 || simplified.triangles() >= prev {
                continue;
            }
            out.push(ClusterLevel {
                meshlets: build_meshlets(&simplified),
                mesh: simplified,
                error: cell,
            });
        }
        Self { levels: out }
    }

    /// The coarsest level whose error, projected at `distance` with vertical
    /// fov `fov` into a `height_px` tall image, is under `threshold_px`.
    #[must_use]
    pub fn select(
        &self,
        distance: f32,
        fov: f32,
        height_px: f32,
        scale: f32,
        threshold_px: f32,
    ) -> usize {
        let px_per_unit = height_px / (2.0 * distance.max(1e-4) * (fov * 0.5).tan());
        let mut best = 0;
        for (i, l) in self.levels.iter().enumerate() {
            if l.error * scale * px_per_unit <= threshold_px {
                best = i;
            }
        }
        best
    }

    /// Cull `level`'s meshlets against frustum `planes` (world space, inside
    /// `≥ 0`) and the eye, and assemble the survivors into one mesh.
    #[must_use]
    pub fn gather(
        &self,
        level: usize,
        model: &Mat4,
        eye: Vec3,
        planes: &[[f32; 4]; 6],
    ) -> (Mesh, ClusterStats) {
        let l = &self.levels[level.min(self.levels.len() - 1)];
        let mut stats = ClusterStats {
            level,
            clusters: l.meshlets.len(),
            ..ClusterStats::default()
        };
        let inv = model.inverse().unwrap_or(Mat4::IDENTITY);
        let eye_local = inv.transform_point(eye);
        let scale = model.max_scale();
        let mut out = Mesh {
            positions: l.mesh.positions.clone(),
            normals: l.mesh.normals.clone(),
            uvs: l.mesh.uvs.clone(),
            indices: Vec::new(),
        };
        for m in &l.meshlets {
            let c = model.transform_point(m.center);
            let r = m.radius * scale;
            if !planes
                .iter()
                .all(|p| p[0] * c.x + p[1] * c.y + p[2] * c.z + p[3] >= -r)
            {
                stats.frustum_culled += 1;
                continue;
            }
            if cone_culled(m, eye_local) {
                stats.cone_culled += 1;
                continue;
            }
            out.indices.extend_from_slice(&m.indices);
        }
        stats.triangles = out.indices.len() / 3;
        (out, stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::sphere;

    #[test]
    fn meshlets_cover_every_triangle_once_within_limits() {
        let m = sphere(1.0, 64, 32);
        let ms = build_meshlets(&m);
        let total: usize = ms.iter().map(Meshlet::triangles).sum();
        assert_eq!(total, m.triangles());
        for c in &ms {
            assert!(c.triangles() <= MAX_TRIANGLES);
            let mut v: Vec<u32> = c.indices.clone();
            v.sort_unstable();
            v.dedup();
            assert!(v.len() <= MAX_VERTICES);
            for i in &c.indices {
                let p = Vec3::from_array(m.positions[*i as usize]);
                assert!((p - c.center).length() <= c.radius + 1e-4);
            }
        }
        let mut seen: Vec<[u32; 3]> = ms
            .iter()
            .flat_map(|c| c.indices.as_chunks::<3>().0.to_vec())
            .collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), m.triangles(), "no triangle twice");
        assert!(ms.len() > 10);
    }

    #[test]
    fn the_far_side_of_a_sphere_is_cone_culled() {
        let m = sphere(1.0, 64, 32);
        let lods = ClusterLods::build(&m, 1);
        let eye = Vec3::new(0.0, 0.0, 5.0);
        let planes = [[0.0, 0.0, 0.0, 1.0]; 6]; // everything inside
        let (vis, st) = lods.gather(0, &Mat4::IDENTITY, eye, &planes);
        assert!(st.cone_culled > st.clusters / 5, "{st:?}");
        // Nothing visible was culled: every front-facing triangle survives.
        let front = m
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .filter(|t| {
                let p = Vec3::from_array(m.positions[t[0] as usize]);
                tri_normal(&m, **t).dot(eye - p) > 0.0
            })
            .count();
        assert!(vis.triangles() >= front);
    }

    #[test]
    fn lod_levels_shrink_and_selection_follows_distance() {
        let m = sphere(1.0, 96, 48);
        let lods = ClusterLods::build(&m, 4);
        assert!(lods.levels.len() >= 3);
        for w in lods.levels.windows(2) {
            assert!(w[1].mesh.triangles() < w[0].mesh.triangles());
            assert!(w[1].error > w[0].error);
        }
        let near = lods.select(2.0, 0.8, 1080.0, 1.0, 1.0);
        let far = lods.select(200.0, 0.8, 1080.0, 1.0, 1.0);
        assert_eq!(near, 0);
        assert!(far > near);
    }

    #[test]
    fn frustum_culls_clusters_outside() {
        let m = sphere(1.0, 32, 16);
        let lods = ClusterLods::build(&m, 1);
        // One plane keeping only x ≥ 0.5.
        let mut planes = [[0.0, 0.0, 0.0, 1.0]; 6];
        planes[0] = [1.0, 0.0, 0.0, -0.5];
        let (_, st) = lods.gather(0, &Mat4::IDENTITY, Vec3::new(10.0, 0.0, 0.0), &planes);
        assert!(st.frustum_culled > 0);
    }
}
