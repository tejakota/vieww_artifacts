//! Turning 3D frames into drawable 2D geometry: shaded, depth-sorted triangles.

use crate::projection::{Projector, ScreenPoint};
use crate::{DEFAULT_LIGHT, Vec3};
use three_core::{MeshFrame, PointFrame};

/// How a mesh is turned into 2D drawing data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshShading {
    /// Direction *toward* the light, in world space, normalized at use.
    pub light: Vec3,
    /// Brightness added to every face regardless of orientation (0..1).
    pub ambient: f32,
    /// How much the face's angle to the light contributes (0..1).
    ///
    /// With `ambient + diffuse <= 1`, brightness stays within 0..1 for any
    /// normal; the default (0.35 + 0.65) lets a directly-lit face reach full
    /// brightness and a perpendicular one drop to ambient.
    pub diffuse: f32,
    /// Drop triangles whose winding says they face away from the camera.
    ///
    /// Off by default: reconstruction output does not guarantee a consistent
    /// winding order, and culling on untrusted winding makes surfaces
    /// disappear rather than look wrong — the worse failure of the two on a
    /// social feed. Turn it on when the mesh is known well-formed.
    pub cull_backfaces: bool,
}

impl Default for MeshShading {
    fn default() -> Self {
        Self {
            light: DEFAULT_LIGHT,
            ambient: 0.35,
            diffuse: 0.65,
            cull_backfaces: false,
        }
    }
}

/// One triangle, projected to the viewport and lit.
///
/// `depth` is the mean of the vertex depths, which is what the triangles are
/// sorted by — the painter's algorithm. `brightness` is 0..1, ready to scale a
/// base colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadedTriangle {
    pub a: ScreenPoint,
    pub b: ScreenPoint,
    pub c: ScreenPoint,
    pub depth: f32,
    pub brightness: f32,
}

/// One mesh edge, projected, for wireframe drawing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WireframeEdge {
    pub from: ScreenPoint,
    pub to: ScreenPoint,
    pub depth: f32,
}

/// Project and shade a mesh frame.
///
/// Triangles with any vertex behind the near plane are dropped whole rather
/// than clipped: a viewer that orbits into its subject loses the triangles it
/// passes rather than showing stretched geometry. Honest, cheap, and the right
/// trade for a reference viewer — a production renderer would clip polygons
/// against the near plane instead.
///
/// The returned triangles are sorted **far to near**, so recording them in
/// order into a back-to-front canvas produces correct occlusion for opaque
/// geometry.
pub fn shaded_triangles(
    mesh: &MeshFrame,
    projector: &Projector,
    shading: &MeshShading,
) -> Vec<ShadedTriangle> {
    let mut triangles = Vec::with_capacity(mesh.indices.len() / 3);
    let lens_near = projector.near();
    let light = normalize_or_default(shading.light);

    for triangle in mesh.indices.as_chunks::<3>().0 {
        let (i0, i1, i2) = (triangle[0] as usize, triangle[1] as usize, triangle[2] as usize);
        if i0 >= mesh.vertices.len() || i1 >= mesh.vertices.len() || i2 >= mesh.vertices.len() {
            // `Capture3D::validate` rejects this; being quiet about it here
            // keeps a half-valid frame from panicking a viewer.
            continue;
        }
        let v0 = mesh.vertices[i0];
        let v1 = mesh.vertices[i1];
        let v2 = mesh.vertices[i2];

        let a = projector.project(v0);
        let b = projector.project(v1);
        let c = projector.project(v2);
        // Behind the camera, or crossing the near plane: drop (see doc).
        if !a.in_front || !b.in_front || !c.in_front
            || a.depth < lens_near || b.depth < lens_near || c.depth < lens_near
        {
            continue;
        }

        if shading.cull_backfaces && is_back_facing(&a, &b, &c) {
            continue;
        }

        let normal = normal(v0, v1, v2);
        // |dot| lights both sides identically: a single-sided capture scanned
        // from underneath should not render black from above.
        let lambert = dot_abs(normal, light);
        let brightness = (shading.ambient + shading.diffuse * lambert).clamp(0.0, 1.0);

        triangles.push(ShadedTriangle {
            a,
            b,
            c,
            depth: (a.depth + b.depth + c.depth) / 3.0,
            brightness,
        });
    }

    triangles.sort_by(|x, y| y.depth.partial_cmp(&x.depth).unwrap_or(std::cmp::Ordering::Equal));
    triangles
}

/// Project a mesh's unique edges for wireframe drawing.
///
/// Edges shared by two triangles appear once. Sorted far to near, like the
/// filled triangles, so a mixed fill-and-line drawing composites correctly.
pub fn wireframe_edges(mesh: &MeshFrame, projector: &Projector) -> Vec<WireframeEdge> {
    let mut seen = std::collections::HashSet::with_capacity(mesh.indices.len());
    let lens_near = projector.near();
    let mut edges = Vec::new();

    for triangle in mesh.indices.as_chunks::<3>().0 {
        let (i0, i1, i2) = (triangle[0], triangle[1], triangle[2]);
        for (from, to) in [(i0, i1), (i1, i2), (i2, i0)] {
            let key = (from.min(to), from.max(to));
            if !seen.insert(key) {
                continue;
            }
            let (from, to) = (from as usize, to as usize);
            if from >= mesh.vertices.len() || to >= mesh.vertices.len() {
                continue;
            }
            let p = projector.project(mesh.vertices[from]);
            let q = projector.project(mesh.vertices[to]);
            if !p.in_front || !q.in_front || p.depth < lens_near || q.depth < lens_near {
                continue;
            }
            edges.push(WireframeEdge {
                from: p,
                to: q,
                depth: (p.depth + q.depth) / 2.0,
            });
        }
    }

    edges.sort_by(|x, y| y.depth.partial_cmp(&x.depth).unwrap_or(std::cmp::Ordering::Equal));
    edges
}

/// Project a point-cloud frame, dropping points behind the camera.
///
/// Returned in input order; points carry their own depth so a painter can
/// scale radius or alpha with distance.
pub fn projected_points(frame: &PointFrame, projector: &Projector) -> Vec<ScreenPoint> {
    frame
        .points
        .iter()
        .map(|&p| projector.project(p))
        .filter(|p| p.in_front && p.on_screen)
        .collect()
}

/// The shoelace value of the screen-space triangle, halved.
///
/// Because the viewport's Y axis points **down**, a triangle that reads
/// counter-clockwise *on screen* has a **negative** value here, and one that
/// reads clockwise on screen is positive. [`is_back_facing`](fn.is_back_facing)
/// builds on this, and tests assert on it directly because the sign is the
/// one fact winding-dependent culling depends on.
pub fn signed_area(a: &ScreenPoint, b: &ScreenPoint, c: &ScreenPoint) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

/// Whether a projected triangle's winding says it faces away.
///
/// A triangle wound counter-clockwise as seen from the eye — the "CCW is
/// front" convention most mesh exporters use — projects to a **negative**
/// signed area in top-left coordinates (Y down), so a positive area means the
/// face is turned away. Note this is only sound when the mesh has consistent
/// winding; see [`MeshShading::cull_backfaces`].
fn is_back_facing(a: &ScreenPoint, b: &ScreenPoint, c: &ScreenPoint) -> bool {
    signed_area(a, b, c) > 0.0
}

fn normal(v0: Vec3, v1: Vec3, v2: Vec3) -> Vec3 {
    let e1 = Vec3::new(v1.x - v0.x, v1.y - v0.y, v1.z - v0.z);
    let e2 = Vec3::new(v2.x - v0.x, v2.y - v0.y, v2.z - v0.z);
    let n = Vec3::new(
        e1.y * e2.z - e1.z * e2.y,
        e1.z * e2.x - e1.x * e2.z,
        e1.x * e2.y - e1.y * e2.x,
    );
    let length = (n.x * n.x + n.y * n.y + n.z * n.z).sqrt();
    if length > 1e-9 {
        Vec3::new(n.x / length, n.y / length, n.z / length)
    } else {
        Vec3::new(0.0, 0.0, 0.0)
    }
}

fn dot_abs(a: Vec3, b: Vec3) -> f32 {
    (a.x * b.x + a.y * b.y + a.z * b.z).abs()
}

fn normalize_or_default(v: Vec3) -> Vec3 {
    let length = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    if length > 1e-9 {
        Vec3::new(v.x / length, v.y / length, v.z / length)
    } else {
        DEFAULT_LIGHT
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection::{Perspective, Viewport};
    use crate::OrbitCamera;
    use three_core::TimestampNs;

    /// Camera at (0, 0, 5), 90-degree lens, 200x100 viewport.
    fn projector() -> Projector {
        let camera = OrbitCamera {
            yaw: 0.0,
            pitch: 0.0,
            distance: 5.0,
            target: Vec3::new(0.0, 0.0, 0.0),
        };
        Projector::new(&camera, Perspective::default(), Viewport::new(200.0, 100.0))
    }

    fn mesh(vertices: Vec<Vec3>, indices: Vec<u32>) -> MeshFrame {
        MeshFrame { timestamp: TimestampNs(0), vertices, indices }
    }

    #[test]
    fn triangles_come_out_far_to_near() {
        // Two floor tiles, one at z=0 (farther, depth 5) and one at z=1
        // (nearer the camera, depth 4).
        let mesh = mesh(
            vec![
                Vec3::new(-1.0, -0.5, 0.0),
                Vec3::new(1.0, -0.5, 0.0),
                Vec3::new(0.0, -0.5, 0.0),
                Vec3::new(-1.0, -0.5, 1.0),
                Vec3::new(1.0, -0.5, 1.0),
                Vec3::new(0.0, -0.5, 1.0),
            ],
            vec![0, 1, 2, 3, 4, 5],
        );
        let triangles = shaded_triangles(&mesh, &projector(), &MeshShading::default());
        assert_eq!(triangles.len(), 2);
        assert!(
            triangles[0].depth > triangles[1].depth,
            "the farther tile is drawn first: {} vs {}",
            triangles[0].depth,
            triangles[1].depth
        );
    }

    #[test]
    fn a_behind_the_camera_triangle_is_dropped() {
        let mesh = mesh(
            vec![
                Vec3::new(0.0, 0.0, 10.0),
                Vec3::new(1.0, 0.0, 10.0),
                Vec3::new(0.0, 1.0, 10.0),
            ],
            vec![0, 1, 2],
        );
        assert!(shaded_triangles(&mesh, &projector(), &MeshShading::default()).is_empty());
    }

    #[test]
    fn brightness_covers_its_range_and_stays_inside_it() {
        let shading = MeshShading::default();
        // A floor (normal +Y) catches most of the default light...
        let floor = mesh(
            vec![
                Vec3::new(-1.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, -1.0),
            ],
            vec![0, 1, 2],
        );
        let lit = shaded_triangles(&floor, &projector(), &shading)[0].brightness;

        // ...while a wall facing away from it catches none.
        let wall = mesh(
            vec![
                Vec3::new(0.0, 0.0, -1.0),
                Vec3::new(1.0, 0.0, -1.0),
                Vec3::new(0.0, 1.0, -1.0),
            ],
            vec![0, 1, 2],
        );
        let dark = shaded_triangles(&wall, &projector(), &shading)[0].brightness;

        assert!(lit > dark, "a floor out-lights a wall facing away: {lit} vs {dark}");
        assert!(dark >= shading.ambient - 1e-5, "ambient is the floor: {dark}");
        assert!(lit <= 1.0 + 1e-5, "brightness never exceeds 1: {lit}");
    }

    #[test]
    fn backface_culling_is_opt_in_and_correct_for_ccw_front() {
        // A triangle wound counter-clockwise seen from +Z (the camera side).
        let front = mesh(
            vec![Vec3::new(-1.0, -1.0, 0.0), Vec3::new(1.0, -1.0, 0.0), Vec3::new(0.0, 1.0, 0.0)],
            vec![0, 1, 2],
        );
        // The same triangle, wound clockwise from the camera's side.
        let back = mesh(
            vec![Vec3::new(-1.0, -1.0, 0.0), Vec3::new(0.0, 1.0, 0.0), Vec3::new(1.0, -1.0, 0.0)],
            vec![0, 1, 2],
        );

        let mut shading = MeshShading { cull_backfaces: true, ..MeshShading::default() };
        let kept = shaded_triangles(&front, &projector(), &shading).len();
        let culled = shaded_triangles(&back, &projector(), &shading).len();
        assert_eq!((kept, culled), (1, 0), "CCW-from-camera is front, CW is back");

        shading.cull_backfaces = false;
        assert_eq!(shaded_triangles(&back, &projector(), &shading).len(), 1, "off = keep both");
    }

    #[test]
    fn shared_edges_appear_once_in_the_wireframe() {
        // Two triangles sharing the edge (1, 2).
        let mesh = mesh(
            vec![
                Vec3::new(-1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, -1.0, 0.0),
            ],
            vec![0, 1, 2, 1, 3, 2],
        );
        let edges = wireframe_edges(&mesh, &projector());
        assert_eq!(edges.len(), 5, "6 triangle-edge slots, one shared, 5 unique");
    }

    #[test]
    fn points_behind_or_off_screen_are_filtered() {
        let frame = PointFrame {
            timestamp: TimestampNs(0),
            points: vec![
                Vec3::new(0.0, 0.0, 0.0),     // center, visible
                Vec3::new(0.0, 0.0, 10.0),    // behind the camera
                Vec3::new(500.0, 0.0, 0.0),   // far outside the frustum
            ],
        };
        let points = projected_points(&frame, &projector());
        assert_eq!(points.len(), 1);
        assert!((points[0].x - 100.0).abs() < 1e-4);
    }

    #[test]
    fn out_of_range_indices_are_skipped_not_panicked() {
        let mesh = mesh(vec![Vec3::new(0.0, 0.0, 0.0)], vec![0, 1, 2]);
        assert!(shaded_triangles(&mesh, &projector(), &MeshShading::default()).is_empty());
    }
}
