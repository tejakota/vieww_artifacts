//! Destruction — Voronoi fracturing, the Unreal Chaos Destruction / Blender
//! Cell Fracture / Houdini Voronoi Fracture capability (§2.17 L4:
//! "Chaos Physics … destruction").
//!
//! A convex body is cut into **Voronoi cells**: each seed point owns the part
//! of the body nearer to it than to any other seed, which is the body
//! clipped by one bisector half-space per neighbouring seed. Cells are
//! convex, tile the original exactly (their volumes sum to the body's), and
//! become bodies of their own. Seeds cluster around the impact point, so the
//! pieces are small where the blow landed and large far from it — the look
//! of real fracture.
//!
//! [`shatter`] does it inside a [`World3`]: the body is replaced by its
//! pieces, each inheriting the parent's linear and angular velocity at its
//! own centroid plus an outward impulse that falls off with distance from
//! the impact. [`fracture_polygon`] is the same construction in 2D for the
//! planar engine.

use std::sync::Arc;

use super::hull::Hull;
use super::math::V3;
use super::{Body3, BodyKind, Shape3, World3};

/// A small deterministic generator (splitmix64).
#[derive(Debug, Clone)]
pub struct SeedRng(u64);

impl SeedRng {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self(seed)
    }
    /// Uniform in 0..1.
    pub fn next_f32(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        #[allow(clippy::cast_precision_loss)]
        let v = (z >> 40) as f32 / (1u64 << 24) as f32;
        v
    }
}

/// `count` seed points inside `hull`. With an `impact`, seeds concentrate
/// around it (density falling with distance); otherwise uniform.
#[must_use]
pub fn seeds(hull: &Hull, count: usize, impact: Option<V3>, seed: u64) -> Vec<V3> {
    let mut lo = V3::splat(f32::INFINITY);
    let mut hi = V3::splat(f32::NEG_INFINITY);
    for v in &hull.verts {
        lo = lo.min(*v);
        hi = hi.max(*v);
    }
    let size = (hi - lo).length();
    let mut rng = SeedRng::new(seed);
    let mut out = Vec::with_capacity(count);
    let mut guard = 0;
    while out.len() < count && guard < count * 200 {
        guard += 1;
        let p = match impact {
            Some(c) => {
                // Gaussian-ish cloud around the impact, radius ~ size/3.
                let r = size * 0.35 * rng.next_f32().powf(0.7);
                let d = V3::new(rng.next_f32() * 2.0 - 1.0, rng.next_f32() * 2.0 - 1.0, rng.next_f32() * 2.0 - 1.0)
                    .normalize();
                c + d * r
            }
            None => V3::new(
                lo.x + (hi.x - lo.x) * rng.next_f32(),
                lo.y + (hi.y - lo.y) * rng.next_f32(),
                lo.z + (hi.z - lo.z) * rng.next_f32(),
            ),
        };
        if hull.contains(p) && !out.iter().any(|q: &V3| (*q - p).length() < size * 0.01) {
            out.push(p);
        }
    }
    out
}

/// The Voronoi cells of `seeds` inside `hull` (cells that vanish are
/// omitted). Their volumes sum to the hull's.
#[must_use]
pub fn voronoi_cells(hull: &Hull, seeds: &[V3]) -> Vec<Hull> {
    let mut out = Vec::new();
    for (i, s) in seeds.iter().enumerate() {
        let mut cell = Some(hull.clone());
        // Nearest neighbours first: they cut the most, so later clips are
        // cheaper.
        let mut others: Vec<&V3> = seeds.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, p)| p).collect();
        others.sort_by(|a, b| (**a - *s).length_sq().total_cmp(&(**b - *s).length_sq()));
        for o in others {
            let Some(c) = cell else { break };
            let n = (*o - *s).normalize();
            let mid = (*o + *s) * 0.5;
            cell = c.clip(n, n.dot(mid));
        }
        if let Some(c) = cell {
            if c.mass_props().volume > 1e-9 {
                out.push(c);
            }
        }
    }
    out
}

/// Replace convex body `id` in `world` with Voronoi pieces. Returns the new
/// bodies' indices (the original slot keeps the first piece, so other
/// indices stay valid).
///
/// The impact `point` and `impulse` are in world space; `pieces` is the
/// seed count.
pub fn shatter(world: &mut World3, id: usize, point: V3, impulse: V3, pieces: usize, seed: u64) -> Vec<usize> {
    let parent = world.bodies[id].clone();
    let Shape3::Convex { hull } = &parent.shape else {
        return vec![id];
    };
    let density = if parent.inv_mass > 0.0 {
        parent.mass() / hull.mass_props().volume.max(1e-9)
    } else {
        1000.0
    };
    let local_impact = parent.pose.inverse_apply(point);
    let cell_seeds = seeds(hull, pieces.max(2), Some(local_impact), seed);
    let cells = voronoi_cells(hull, &cell_seeds);
    let mut ids = Vec::new();
    let size = hull.verts.iter().map(|v| v.length()).fold(0.0, f32::max).max(1e-3);
    for (k, cell) in cells.into_iter().enumerate() {
        let mp = cell.mass_props();
        let centred = cell.translated(-mp.centroid);
        let mut body = Body3::dynamic(Shape3::Convex { hull: Arc::new(centred) }, density);
        body.pose.p = parent.pose.apply(mp.centroid);
        body.pose.q = parent.pose.q;
        body.lin_vel = parent.velocity_at(body.pose.p);
        body.ang_vel = parent.ang_vel;
        body.friction = parent.friction;
        body.restitution = parent.restitution;
        body.user = parent.user;
        if parent.kind != BodyKind::Dynamic {
            body.lin_vel = V3::ZERO;
        }
        // The blow: impulse shared by proximity, pushing outward.
        let d = body.pose.p - point;
        let falloff = (1.0 - d.length() / (size * 2.5)).max(0.0).powi(2);
        let outward = d.normalize() * impulse.length() * 0.35;
        let j = (impulse + outward) * (falloff / pieces.max(1) as f32 * 2.0);
        body.apply_impulse(j, body.pose.p + (point - body.pose.p) * 0.3);
        if k == 0 {
            world.bodies[id] = body;
            ids.push(id);
        } else {
            ids.push(world.add(body));
        }
    }
    ids
}

/// 2D Voronoi fracture of a convex polygon (counter-clockwise points).
#[must_use]
pub fn fracture_polygon(poly: &[(f32, f32)], seeds: &[(f32, f32)]) -> Vec<Vec<(f32, f32)>> {
    let clip = |p: &[(f32, f32)], n: (f32, f32), d: f32| -> Vec<(f32, f32)> {
        let mut out = Vec::new();
        for i in 0..p.len() {
            let (a, b) = (p[i], p[(i + 1) % p.len()]);
            let (sa, sb) = (n.0 * a.0 + n.1 * a.1 - d, n.0 * b.0 + n.1 * b.1 - d);
            if sa <= 0.0 {
                out.push(a);
            }
            if (sa < 0.0 && sb > 0.0) || (sa > 0.0 && sb < 0.0) {
                let t = sa / (sa - sb);
                out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
            }
        }
        out
    };
    let mut cells = Vec::new();
    for (i, s) in seeds.iter().enumerate() {
        let mut cell = poly.to_vec();
        for (j, o) in seeds.iter().enumerate() {
            if i == j || cell.len() < 3 {
                continue;
            }
            let n = (o.0 - s.0, o.1 - s.1);
            let mid = ((o.0 + s.0) * 0.5, (o.1 + s.1) * 0.5);
            cell = clip(&cell, n, n.0 * mid.0 + n.1 * mid.1);
        }
        if cell.len() >= 3 {
            cells.push(cell);
        }
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_tile_the_body_exactly() {
        let h = Hull::cuboid(V3::new(1.0, 0.5, 0.75));
        let s = seeds(&h, 14, None, 7);
        assert_eq!(s.len(), 14);
        let cells = voronoi_cells(&h, &s);
        assert_eq!(cells.len(), 14);
        let total: f32 = cells.iter().map(|c| c.mass_props().volume).sum();
        assert!((total - 3.0).abs() < 1e-3, "{total}");
        // Each seed lies in its own cell.
        for (c, p) in cells.iter().zip(&s) {
            assert!(c.contains(*p));
        }
    }

    #[test]
    fn impact_seeds_cluster_near_the_blow() {
        let h = Hull::cuboid(V3::splat(1.0));
        let hit = V3::new(0.9, 0.0, 0.0);
        let s = seeds(&h, 30, Some(hit), 3);
        // Seeds fall within ~0.35 × the body's diagonal of the blow.
        let near = s.iter().filter(|p| (**p - hit).length() < 1.25).count();
        let far = s.iter().filter(|p| (**p - hit).length() > 1.5).count();
        assert!(near > 25 && far == 0, "{near} {far}");
    }

    #[test]
    fn shattering_conserves_mass_and_momentum() {
        let mut w = World3::new();
        w.gravity = V3::ZERO;
        let id = w.add(
            Body3::dynamic(Shape3::cuboid(V3::splat(0.5)), 800.0).with_velocity(V3::new(1.0, 0.0, 0.0)),
        );
        let m0 = w.bodies[id].mass();
        let p0 = w.bodies[id].lin_vel * m0;
        let ids = shatter(&mut w, id, V3::new(0.5, 0.0, 0.0), V3::ZERO, 10, 11);
        assert!(ids.len() >= 8);
        let m: f32 = ids.iter().map(|i| w.bodies[*i].mass()).sum();
        assert!((m - m0).abs() / m0 < 1e-3, "{m} vs {m0}");
        let p: V3 = ids.iter().fold(V3::ZERO, |acc, i| acc + w.bodies[*i].lin_vel * w.bodies[*i].mass());
        assert!((p - p0).length() / p0.length() < 1e-3);
    }

    #[test]
    fn a_blow_sends_pieces_flying_and_they_settle_on_the_floor() {
        let mut w = World3::new();
        w.add(Body3::fixed(Shape3::plane(V3::Y)));
        let id = w.add(Body3::dynamic(Shape3::cuboid(V3::splat(0.5)), 800.0).at(V3::new(0.0, 0.5, 0.0)));
        let ids = shatter(&mut w, id, V3::new(-0.5, 0.6, 0.0), V3::new(800.0, 200.0, 0.0), 12, 5);
        let fastest = ids.iter().map(|i| w.bodies[*i].lin_vel.length()).fold(0.0, f32::max);
        assert!(fastest > 0.5, "{fastest}");
        for _ in 0..720 {
            w.step(1.0 / 120.0);
        }
        for i in &ids {
            let y = w.bodies[*i].pose.p.y;
            assert!(y > -0.01 && y < 1.0, "piece {i} at {y}");
        }
    }

    #[test]
    fn polygons_fracture_into_cells_whose_area_sums() {
        let square = [(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)];
        let cells = fracture_polygon(&square, &[(0.5, 0.5), (1.5, 0.5), (1.0, 1.6)]);
        assert_eq!(cells.len(), 3);
        let area = |p: &Vec<(f32, f32)>| {
            let mut a = 0.0;
            for i in 0..p.len() {
                let (x0, y0) = p[i];
                let (x1, y1) = p[(i + 1) % p.len()];
                a += x0 * y1 - x1 * y0;
            }
            a * 0.5
        };
        let total: f32 = cells.iter().map(area).sum();
        assert!((total - 4.0).abs() < 1e-4, "{total}");
    }
}
