//! Ray picking — Three.js' `Raycaster`, the layer R3F's pointer events are
//! built on (§2.3 L5: "NDC coordinates → `Raycaster.setFromCamera()` →
//! intersects meshes → fires callback").
//!
//! A [`Ray`] is made from a camera and a screen position; [`raycast`] tests
//! every visible mesh (bounding sphere first, then Möller–Trumbore per
//! triangle, in the mesh's world space) and returns every hit, nearest
//! first, with the node, the instance, the point, the interpolated normal
//! and UV. [`project`] is the inverse — a world point to pixels — for
//! overlaying 2D labels on 3D objects.

use crate::math::{Mat4, Vec3};
use crate::scene::{Camera, Content, NodeId, Scene};

/// A half-line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    pub origin: Vec3,
    /// Unit length.
    pub direction: Vec3,
}

impl Ray {
    #[must_use]
    pub fn new(origin: Vec3, direction: Vec3) -> Self {
        Self {
            origin,
            direction: direction.normalize(),
        }
    }

    /// Through normalised device coordinates (−1..1, y up).
    #[must_use]
    pub fn from_camera(camera: &Camera, aspect: f32, ndc_x: f32, ndc_y: f32) -> Self {
        let inv = (camera.projection(aspect) * camera.view())
            .inverse()
            .unwrap_or(Mat4::IDENTITY);
        let near = inv.transform_point(Vec3::new(ndc_x, ndc_y, -1.0));
        let far = inv.transform_point(Vec3::new(ndc_x, ndc_y, 1.0));
        match camera.projection {
            // Perspective rays start at the eye, as Three.js' do, so hit
            // distances are distances from the camera.
            crate::scene::Projection::Perspective { .. } => {
                Self::new(camera.position, far - camera.position)
            }
            crate::scene::Projection::Orthographic { .. } => Self::new(near, far - near),
        }
    }

    /// Through pixel `(x, y)` of a `width × height` view.
    #[must_use]
    pub fn from_screen(camera: &Camera, width: f32, height: f32, x: f32, y: f32) -> Self {
        Self::from_camera(
            camera,
            width / height,
            x / width * 2.0 - 1.0,
            1.0 - y / height * 2.0,
        )
    }

    #[must_use]
    pub fn at(&self, t: f32) -> Vec3 {
        self.origin + self.direction * t
    }
}

/// Möller–Trumbore: `(t, u, v)` of the hit, front and back faces alike.
#[must_use]
pub fn ray_triangle(ray: &Ray, a: Vec3, b: Vec3, c: Vec3) -> Option<(f32, f32, f32)> {
    let e1 = b - a;
    let e2 = c - a;
    let p = ray.direction.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let s = ray.origin - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = ray.direction.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 1e-6).then_some((t, u, v))
}

/// One intersection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub node: NodeId,
    /// For instanced meshes, which instance.
    pub instance: Option<usize>,
    pub distance: f32,
    pub point: Vec3,
    pub normal: Vec3,
    pub uv: [f32; 2],
    /// Index of the triangle within the mesh.
    pub triangle: usize,
}

/// Every hit along `ray`, nearest first.
pub fn raycast(scene: &mut Scene, ray: &Ray) -> Vec<Hit> {
    scene.update_world();
    let mut hits = Vec::new();
    for id in scene.traverse() {
        if !scene.effectively_visible(id) {
            continue;
        }
        let node = scene.node(id);
        let world = node.world();
        let mut test =
            |model: Mat4, mesh: &vieww_mesh::Mesh, bounds: (Vec3, f32), instance: Option<usize>| {
                let c = model.transform_point(bounds.0);
                let r = bounds.1 * model.max_scale();
                // Sphere rejection.
                let oc = ray.origin - c;
                let b = oc.dot(ray.direction);
                if b * b - (oc.dot(oc) - r * r) < 0.0 {
                    return;
                }
                let normal_m = model.inverse().map_or(model, |m| m.transpose());
                for (ti, t) in mesh.indices.as_chunks::<3>().0.iter().enumerate() {
                    let p = |i: u32| {
                        model.transform_point(Vec3::from_array(mesh.positions[i as usize]))
                    };
                    let (a, bb, cc) = (p(t[0]), p(t[1]), p(t[2]));
                    if let Some((dist, u, v)) = ray_triangle(ray, a, bb, cc) {
                        let w = 1.0 - u - v;
                        let normal = if mesh.has_normals() {
                            let n = |i: u32| Vec3::from_array(mesh.normals[i as usize]);
                            normal_m
                                .transform_vector(n(t[0]) * w + n(t[1]) * u + n(t[2]) * v)
                                .normalize()
                        } else {
                            (bb - a).cross(cc - a).normalize()
                        };
                        let uv = if mesh.has_uvs() {
                            let q = |i: u32| mesh.uvs[i as usize];
                            let (q0, q1, q2) = (q(t[0]), q(t[1]), q(t[2]));
                            [
                                q0[0] * w + q1[0] * u + q2[0] * v,
                                q0[1] * w + q1[1] * u + q2[1] * v,
                            ]
                        } else {
                            [0.0, 0.0]
                        };
                        hits.push(Hit {
                            node: id,
                            instance,
                            distance: dist,
                            point: ray.at(dist),
                            normal,
                            uv,
                            triangle: ti,
                        });
                    }
                }
            };
        match &node.content {
            Content::Mesh { mesh, bounds, .. } => test(world, mesh, *bounds, None),
            Content::Instanced {
                mesh,
                bounds,
                instances,
                ..
            } => {
                for (i, inst) in instances.iter().enumerate() {
                    test(world * *inst, mesh, *bounds, Some(i));
                }
            }
            Content::Lod { levels, bounds, .. } => {
                if let Some((_, m)) = levels.first() {
                    test(world, m, *bounds, None);
                }
            }
            Content::Clustered { lods, bounds, .. } => {
                if let Some(l) = lods.levels.first() {
                    test(world, &l.mesh, *bounds, None);
                }
            }
            Content::Skinned {
                skin,
                morph_weights,
                ..
            } => {
                let m = crate::skin::morph(skin, morph_weights);
                let b = crate::geometry::bounding_sphere(&m);
                test(world, &m, b, None);
            }
            Content::Empty | Content::Light(_) | Content::Points { .. } => {}
        }
    }
    hits.sort_by(|a, b| a.distance.total_cmp(&b.distance));
    // A ray through a shared edge hits both triangles at the same point;
    // that is one surface crossing, not two.
    hits.dedup_by(|b, a| {
        a.node == b.node && a.instance == b.instance && (a.distance - b.distance).abs() < 1e-5
    });
    hits
}

/// The pixel a world point lands on in a `width × height` view, and its
/// NDC depth; `None` behind the camera.
#[must_use]
pub fn project(camera: &Camera, width: f32, height: f32, point: Vec3) -> Option<(f32, f32, f32)> {
    let vp = camera.projection(width / height) * camera.view();
    let c = vp.mul_vec4([point.x, point.y, point.z, 1.0]);
    if c[3] <= 1e-6 {
        return None;
    }
    let (x, y, z) = (c[0] / c[3], c[1] / c[3], c[2] / c[3]);
    Some(((x * 0.5 + 0.5) * width, (0.5 - y * 0.5) * height, z))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{box_mesh, sphere};
    use crate::scene::{Material, Node};
    use vieww_foundation::Color;

    #[test]
    fn the_centre_ray_hits_the_front_face_first() {
        let mut s = Scene::new();
        let id = s.add(
            Node::new(
                "box",
                Content::mesh(box_mesh(2.0, 2.0, 2.0), Material::lambert(Color::WHITE)),
            ),
            None,
        );
        let cam = Camera::perspective(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, 0.8);
        let ray = Ray::from_screen(&cam, 100.0, 100.0, 50.0, 50.0);
        let hits = raycast(&mut s, &ray);
        assert_eq!(hits.len(), 2, "in the front, out the back");
        assert_eq!(hits[0].node, id);
        assert!((hits[0].distance - 4.0).abs() < 1e-3);
        assert!((hits[0].normal - Vec3::Z).length() < 1e-4);
    }

    #[test]
    fn picking_distinguishes_objects_and_instances() {
        let mut s = Scene::new();
        let inst = vec![
            Mat4::translation(Vec3::new(-2.0, 0.0, 0.0)),
            Mat4::translation(Vec3::new(2.0, 0.0, 0.0)),
        ];
        s.add(
            Node::new(
                "pair",
                Content::instanced(sphere(0.5, 16, 8), Material::lambert(Color::WHITE), inst),
            ),
            None,
        );
        let cam = Camera::perspective(Vec3::new(0.0, 0.0, 8.0), Vec3::ZERO, 0.8);
        let (x, y, _) = project(&cam, 200.0, 200.0, Vec3::new(2.0, 0.0, 0.0)).unwrap();
        let hits = raycast(&mut s, &Ray::from_screen(&cam, 200.0, 200.0, x, y));
        assert!(!hits.is_empty(), "projected ({x}, {y}) missed");
        assert_eq!(hits[0].instance, Some(1));
        let miss = raycast(&mut s, &Ray::from_screen(&cam, 200.0, 200.0, 100.0, 100.0));
        assert!(miss.is_empty());
    }

    #[test]
    fn project_and_ray_agree() {
        let cam = Camera::perspective(Vec3::new(3.0, 2.0, 6.0), Vec3::ZERO, 0.9);
        let p = Vec3::new(0.5, -0.25, 0.3);
        let (x, y, _) = project(&cam, 320.0, 200.0, p).unwrap();
        let ray = Ray::from_screen(&cam, 320.0, 200.0, x, y);
        // The point lies on the ray.
        let t = (p - ray.origin).dot(ray.direction);
        assert!((ray.at(t) - p).length() < 1e-3);
    }
}
