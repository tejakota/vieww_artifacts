//! Primitive generators — Three.js' `BoxGeometry`, `SphereGeometry`,
//! `PlaneGeometry`, `CylinderGeometry`, `ConeGeometry`, `TorusGeometry`.
//!
//! Every generator returns a [`Mesh`] with normals and UVs, wound
//! counter-clockwise when seen from outside (the front face, as in
//! Three.js), so the renderer's back-face culling needs no per-primitive
//! knowledge.

use std::f32::consts::{PI, TAU};

use vieww_mesh::Mesh;

use crate::math::Vec3;

fn push(mesh: &mut Mesh, p: Vec3, n: Vec3, uv: [f32; 2]) -> u32 {
    #[allow(clippy::cast_possible_truncation)]
    let i = mesh.positions.len() as u32;
    mesh.positions.push(p.to_array());
    mesh.normals.push(n.to_array());
    mesh.uvs.push(uv);
    i
}

/// A box centred on the origin, `w × h × d`, with a hard normal per face.
#[must_use]
pub fn box_mesh(w: f32, h: f32, d: f32) -> Mesh {
    let mut m = Mesh::default();
    let (x, y, z) = (w / 2.0, h / 2.0, d / 2.0);
    // (normal, u axis, v axis) for each face; corners built from them.
    let faces = [
        (Vec3::X, Vec3::new(0.0, 0.0, -1.0), Vec3::Y),
        (-Vec3::X, Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::X, Vec3::new(0.0, 0.0, -1.0)),
        (-Vec3::Y, Vec3::X, Vec3::Z),
        (Vec3::Z, Vec3::X, Vec3::Y),
        (-Vec3::Z, -Vec3::X, Vec3::Y),
    ];
    let half = Vec3::new(x, y, z);
    for (n, u, v) in faces {
        let c = n.mul_elem(half);
        let su = u.mul_elem(half).length();
        let sv = v.mul_elem(half).length();
        let corner = |a: f32, b: f32| c + u * (a * su) + v * (b * sv);
        let i0 = push(&mut m, corner(-1.0, -1.0), n, [0.0, 1.0]);
        let i1 = push(&mut m, corner(1.0, -1.0), n, [1.0, 1.0]);
        let i2 = push(&mut m, corner(1.0, 1.0), n, [1.0, 0.0]);
        let i3 = push(&mut m, corner(-1.0, 1.0), n, [0.0, 0.0]);
        m.indices.extend_from_slice(&[i0, i1, i2, i0, i2, i3]);
    }
    m
}

/// A UV sphere of `radius` with `segments` around and `rings` top to
/// bottom.
#[must_use]
pub fn sphere(radius: f32, segments: u32, rings: u32) -> Mesh {
    let mut m = Mesh::default();
    let (segments, rings) = (segments.max(3), rings.max(2));
    for r in 0..=rings {
        #[allow(clippy::cast_precision_loss)]
        let v = r as f32 / rings as f32;
        let phi = v * PI;
        for s in 0..=segments {
            #[allow(clippy::cast_precision_loss)]
            let u = s as f32 / segments as f32;
            let theta = u * TAU;
            let n = Vec3::new(-theta.cos() * phi.sin(), phi.cos(), theta.sin() * phi.sin());
            push(&mut m, n * radius, n, [u, v]);
        }
    }
    let row = segments + 1;
    for r in 0..rings {
        for s in 0..segments {
            let a = r * row + s;
            let b = a + row;
            if r != 0 {
                m.indices.extend_from_slice(&[a, b, a + 1]);
            }
            if r != rings - 1 {
                m.indices.extend_from_slice(&[a + 1, b, b + 1]);
            }
        }
    }
    m
}

/// A `w × h` plane in XY facing +z, subdivided `sx × sy`.
#[must_use]
pub fn plane(w: f32, h: f32, sx: u32, sy: u32) -> Mesh {
    let mut m = Mesh::default();
    let (sx, sy) = (sx.max(1), sy.max(1));
    for j in 0..=sy {
        for i in 0..=sx {
            #[allow(clippy::cast_precision_loss)]
            let (u, v) = (i as f32 / sx as f32, j as f32 / sy as f32);
            push(&mut m, Vec3::new((u - 0.5) * w, (0.5 - v) * h, 0.0), Vec3::Z, [u, v]);
        }
    }
    let row = sx + 1;
    for j in 0..sy {
        for i in 0..sx {
            let a = j * row + i;
            let b = a + row;
            m.indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    m
}

/// A cylinder along y (a cone when `top` is 0), capped at both ends.
#[must_use]
pub fn cylinder(top: f32, bottom: f32, height: f32, segments: u32) -> Mesh {
    let mut m = Mesh::default();
    let segments = segments.max(3);
    let half = height / 2.0;
    let slope = (bottom - top) / height;
    for s in 0..=segments {
        #[allow(clippy::cast_precision_loss)]
        let u = s as f32 / segments as f32;
        let t = u * TAU;
        let (sin, cos) = t.sin_cos();
        let n = Vec3::new(sin, slope, cos).normalize();
        push(&mut m, Vec3::new(top * sin, half, top * cos), n, [u, 0.0]);
        push(&mut m, Vec3::new(bottom * sin, -half, bottom * cos), n, [u, 1.0]);
    }
    for s in 0..segments {
        let a = s * 2;
        m.indices.extend_from_slice(&[a, a + 1, a + 2, a + 2, a + 1, a + 3]);
    }
    for (y, r, n) in [(half, top, Vec3::Y), (-half, bottom, -Vec3::Y)] {
        if r <= 0.0 {
            continue;
        }
        let c = push(&mut m, Vec3::new(0.0, y, 0.0), n, [0.5, 0.5]);
        let first = c + 1;
        for s in 0..=segments {
            #[allow(clippy::cast_precision_loss)]
            let t = s as f32 / segments as f32 * TAU;
            let (sin, cos) = t.sin_cos();
            push(&mut m, Vec3::new(r * sin, y, r * cos), n, [0.5 + sin * 0.5, 0.5 + cos * 0.5]);
        }
        for s in 0..segments {
            if n.y > 0.0 {
                m.indices.extend_from_slice(&[c, first + s, first + s + 1]);
            } else {
                m.indices.extend_from_slice(&[c, first + s + 1, first + s]);
            }
        }
    }
    m
}

/// A cone of `radius` and `height` along y.
#[must_use]
pub fn cone(radius: f32, height: f32, segments: u32) -> Mesh {
    cylinder(0.0, radius, height, segments)
}

/// A torus in the XZ plane: ring `radius`, tube `tube`.
#[must_use]
pub fn torus(radius: f32, tube: f32, radial: u32, tubular: u32) -> Mesh {
    let mut m = Mesh::default();
    let (radial, tubular) = (radial.max(3), tubular.max(3));
    for j in 0..=radial {
        #[allow(clippy::cast_precision_loss)]
        let v = j as f32 / radial as f32 * TAU;
        for i in 0..=tubular {
            #[allow(clippy::cast_precision_loss)]
            let u = i as f32 / tubular as f32 * TAU;
            let center = Vec3::new(radius * u.cos(), 0.0, radius * u.sin());
            let p = Vec3::new(
                (radius + tube * v.cos()) * u.cos(),
                tube * v.sin(),
                (radius + tube * v.cos()) * u.sin(),
            );
            #[allow(clippy::cast_precision_loss)]
            let uv = [i as f32 / tubular as f32, j as f32 / radial as f32];
            push(&mut m, p, (p - center).normalize(), uv);
        }
    }
    let row = tubular + 1;
    for j in 0..radial {
        for i in 0..tubular {
            let a = j * row + i;
            let b = a + row;
            m.indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    m
}

/// Bounding sphere (centre, radius) of a mesh's positions.
#[must_use]
pub fn bounding_sphere(mesh: &Mesh) -> (Vec3, f32) {
    let Some((lo, hi)) = mesh.bounds() else { return (Vec3::ZERO, 0.0) };
    let c = (Vec3::from_array(lo) + Vec3::from_array(hi)) * 0.5;
    let r = mesh
        .positions
        .iter()
        .map(|p| (Vec3::from_array(*p) - c).length())
        .fold(0.0, f32::max);
    (c, r)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every triangle's winding agrees with its vertex normals (front faces
    /// point out).
    fn outward(m: &Mesh) -> bool {
        m.indices.chunks(3).all(|t| {
            let p = |i: u32| Vec3::from_array(m.positions[i as usize]);
            let n = |i: u32| Vec3::from_array(m.normals[i as usize]);
            let face = (p(t[1]) - p(t[0])).cross(p(t[2]) - p(t[0]));
            face.length() < 1e-9 || face.dot(n(t[0]) + n(t[1]) + n(t[2])) > 0.0
        })
    }

    #[test]
    fn every_primitive_winds_outward_and_is_well_formed() {
        for (name, m) in [
            ("box", box_mesh(1.0, 2.0, 3.0)),
            ("sphere", sphere(1.0, 16, 8)),
            ("plane", plane(2.0, 2.0, 2, 2)),
            ("cylinder", cylinder(1.0, 1.0, 2.0, 12)),
            ("cone", cone(1.0, 2.0, 12)),
            ("torus", torus(2.0, 0.5, 8, 16)),
        ] {
            assert!(m.has_normals() && m.has_uvs(), "{name}");
            assert_eq!(m.indices.len() % 3, 0, "{name}");
            assert!(m.indices.iter().all(|&i| (i as usize) < m.positions.len()), "{name}");
            assert!(outward(&m), "{name} winds inward somewhere");
        }
    }

    #[test]
    fn sizes_are_what_was_asked() {
        let (lo, hi) = box_mesh(1.0, 2.0, 3.0).bounds().unwrap();
        assert_eq!((hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]), (1.0, 2.0, 3.0));
        let (c, r) = bounding_sphere(&sphere(2.0, 24, 12));
        assert!(c.length() < 1e-4 && (r - 2.0).abs() < 1e-4);
        assert_eq!(box_mesh(1.0, 1.0, 1.0).triangles(), 12);
    }
}
