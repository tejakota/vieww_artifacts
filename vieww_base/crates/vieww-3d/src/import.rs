//! glTF into the scene graph — R3F's `useGLTF`, Three.js' `GLTFLoader`
//! result, Godot's scene import.
//!
//! [`import_gltf`] mirrors a [`Gltf`]'s node hierarchy into a [`Scene`]
//! (one child node per primitive, carrying a `Standard` material built from
//! the PBR factors) and returns, per glTF node, the scene node it became.
//! [`apply_animation`] samples one of the file's animations at a time and
//! writes the TRS overrides onto those nodes — the `AnimationMixer` step.

use std::sync::Arc;

use vieww_foundation::Color;
use vieww_mesh::gltf::Gltf;

use crate::geometry::bounding_sphere;
use crate::math::{Mat4, Quat, Vec3};
use crate::scene::{Content, Material, Node, NodeId, Rgb, Scene};

fn material(g: &Gltf, index: Option<usize>) -> Material {
    let Some(m) = index.and_then(|i| g.materials.get(i)) else {
        return Material::standard(Color::rgb(200, 200, 200), 0.0, 0.8);
    };
    let mut out = Material::standard(Color::WHITE, m.metallic, m.roughness);
    out.color = Rgb::new(m.base_color[0], m.base_color[1], m.base_color[2]);
    out.emissive = Rgb::new(m.emissive[0], m.emissive[1], m.emissive[2]);
    out.double_sided = m.double_sided;
    if m.blend {
        out.opacity = m.base_color[3];
    }
    out
}

/// Decompose a column-major matrix into TRS (no shear).
fn decompose(m: &[f32; 16]) -> (Vec3, Quat, Vec3) {
    let mat = Mat4::from_cols_array(m);
    let col = |c: usize| Vec3::new(mat.m[c][0], mat.m[c][1], mat.m[c][2]);
    let s = Vec3::new(col(0).length(), col(1).length(), col(2).length());
    let r = [col(0) / s.x.max(1e-12), col(1) / s.y.max(1e-12), col(2) / s.z.max(1e-12)];
    // Rotation matrix → quaternion (Shepperd).
    let (m00, m11, m22) = (r[0].x, r[1].y, r[2].z);
    let trace = m00 + m11 + m22;
    let q = if trace > 0.0 {
        let s4 = (trace + 1.0).sqrt() * 2.0;
        Quat::new((r[1].z - r[2].y) / s4, (r[2].x - r[0].z) / s4, (r[0].y - r[1].x) / s4, 0.25 * s4)
    } else if m00 > m11 && m00 > m22 {
        let s4 = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
        Quat::new(0.25 * s4, (r[1].x + r[0].y) / s4, (r[2].x + r[0].z) / s4, (r[1].z - r[2].y) / s4)
    } else if m11 > m22 {
        let s4 = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
        Quat::new((r[1].x + r[0].y) / s4, 0.25 * s4, (r[2].y + r[1].z) / s4, (r[2].x - r[0].z) / s4)
    } else {
        let s4 = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
        Quat::new((r[2].x + r[0].z) / s4, (r[2].y + r[1].z) / s4, 0.25 * s4, (r[0].y - r[1].x) / s4)
    };
    (mat.get_translation(), q.normalize(), s)
}

/// Add the default scene of `g` under `parent`; returns the scene node for
/// each glTF node (indexed like `g.nodes`).
pub fn import_gltf(scene: &mut Scene, g: &Gltf, parent: Option<NodeId>) -> Vec<Option<NodeId>> {
    let mut ids = vec![None; g.nodes.len()];
    let mut stack: Vec<(usize, Option<NodeId>)> = g.roots().into_iter().rev().map(|r| (r, parent)).collect();
    while let Some((i, par)) = stack.pop() {
        let n = &g.nodes[i];
        let (t, r, s) = match &n.matrix {
            Some(m) => decompose(m),
            None => (
                Vec3::from_array(n.translation),
                Quat::new(n.rotation[0], n.rotation[1], n.rotation[2], n.rotation[3]),
                Vec3::from_array(n.scale),
            ),
        };
        let name = if n.name.is_empty() { format!("node{i}") } else { n.name.clone() };
        let id = scene.add(Node::new(&name, Content::Empty).at(t).rotated(r).scaled(s), par);
        ids[i] = Some(id);
        if let Some(mesh) = n.mesh.and_then(|m| g.meshes.get(m)) {
            for (k, p) in mesh.primitives.iter().enumerate() {
                let bounds = bounding_sphere(&p.mesh);
                scene.add(
                    Node::new(
                        &format!("{name}/prim{k}"),
                        Content::Mesh {
                            mesh: Arc::new(p.mesh.clone()),
                            material: Arc::new(material(g, p.material)),
                            bounds,
                        },
                    ),
                    Some(id),
                );
            }
        }
        for &c in n.children.iter().rev() {
            stack.push((c, Some(id)));
        }
    }
    ids
}

/// Pose the imported nodes with animation `index` at `t` seconds.
pub fn apply_animation(scene: &mut Scene, g: &Gltf, ids: &[Option<NodeId>], index: usize, t: f32) {
    for (i, pose) in g.sample(index, t).into_iter().enumerate() {
        let Some(Some(id)) = ids.get(i) else { continue };
        if pose.translation.is_none() && pose.rotation.is_none() && pose.scale.is_none() {
            continue;
        }
        let node = scene.node_mut(*id);
        if let Some(v) = pose.translation {
            node.position = Vec3::from_array(v);
        }
        if let Some(q) = pose.rotation {
            node.rotation = Quat::new(q[0], q[1], q[2], q[3]);
        }
        if let Some(v) = pose.scale {
            node.scale = Vec3::from_array(v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::box_mesh;
    use vieww_mesh::gltf::{parse_gltf, write_gltf};

    #[test]
    fn a_written_gltf_imports_as_a_node_with_a_mesh_child() {
        let text = write_gltf(&box_mesh(1.0, 1.0, 1.0), "crate", [0.8, 0.2, 0.1, 1.0]);
        let g = parse_gltf(&text, &|_| None).unwrap();
        let mut s = Scene::new();
        let ids = import_gltf(&mut s, &g, None);
        let root = ids[0].unwrap();
        assert_eq!(s.node(root).name, "crate");
        assert_eq!(s.node(root).children().len(), 1);
        let child = s.node(s.node(root).children()[0]);
        match &child.content {
            Content::Mesh { mesh, material, .. } => {
                assert_eq!(mesh.triangles(), 12);
                assert!((material.color.r - 0.8).abs() < 1e-6);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn matrices_decompose_to_the_trs_they_were_made_from() {
        let t = Vec3::new(1.0, 2.0, 3.0);
        let r = Quat::from_euler(0.4, -0.3, 1.1);
        let sc = Vec3::new(2.0, 0.5, 1.5);
        let m = Mat4::compose(t, r, sc);
        let mut a = [0.0; 16];
        for c in 0..4 {
            for row in 0..4 {
                a[c * 4 + row] = m.m[c][row];
            }
        }
        let (t2, r2, s2) = decompose(&a);
        assert!((t2 - t).length() < 1e-4 && (s2 - sc).length() < 1e-4);
        let v = Vec3::new(0.3, -0.7, 0.2);
        assert!((r2.rotate(v) - r.rotate(v)).length() < 1e-4);
    }
}
