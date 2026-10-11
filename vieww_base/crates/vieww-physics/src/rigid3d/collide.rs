//! Narrowphase: shapes, poses, GJK distance, SAT with face clipping for
//! polyhedra, and analytic sphere/capsule/plane pairs. Every routine returns
//! world-space contacts whose normal points **from A to B**.

use std::sync::Arc;

use super::hull::Hull;
use super::math::{Q, V3};

/// A collision shape in body-local coordinates.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape3 {
    Sphere {
        radius: f32,
    },
    /// A segment along local y from −`half_height` to +`half_height`, swept
    /// by `radius`.
    Capsule {
        half_height: f32,
        radius: f32,
    },
    /// Any convex polyhedron (boxes are one).
    Convex {
        hull: Arc<Hull>,
    },
    /// The half-space `normal · x ≤ 0` in local space. Static bodies only.
    Plane {
        normal: V3,
    },
}

impl Shape3 {
    #[must_use]
    pub fn sphere(radius: f32) -> Self {
        Self::Sphere { radius }
    }
    #[must_use]
    pub fn cuboid(half: V3) -> Self {
        Self::Convex {
            hull: Arc::new(Hull::cuboid(half)),
        }
    }
    #[must_use]
    pub fn capsule(half_height: f32, radius: f32) -> Self {
        Self::Capsule {
            half_height,
            radius,
        }
    }
    /// The hull of `points`; `None` when they are flat.
    #[must_use]
    pub fn convex(points: &[V3]) -> Option<Self> {
        Hull::from_points(points).map(|h| Self::Convex { hull: Arc::new(h) })
    }
    #[must_use]
    pub fn plane(normal: V3) -> Self {
        Self::Plane {
            normal: normal.normalize(),
        }
    }

    /// Local bounding radius about the origin (∞ for planes).
    #[must_use]
    pub fn bounding_radius(&self) -> f32 {
        match self {
            Self::Sphere { radius } => *radius,
            Self::Capsule {
                half_height,
                radius,
            } => half_height + radius,
            Self::Convex { hull } => hull.verts.iter().map(|v| v.length()).fold(0.0, f32::max),
            Self::Plane { .. } => f32::INFINITY,
        }
    }
}

/// A rigid pose.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Iso {
    pub p: V3,
    pub q: Q,
}

impl Iso {
    #[must_use]
    pub fn new(p: V3, q: Q) -> Self {
        Self { p, q }
    }
    #[must_use]
    pub fn apply(&self, v: V3) -> V3 {
        self.q.rotate(v) + self.p
    }
    #[must_use]
    pub fn rotate(&self, v: V3) -> V3 {
        self.q.rotate(v)
    }
    #[must_use]
    pub fn inverse_apply(&self, v: V3) -> V3 {
        self.q.conjugate().rotate(v - self.p)
    }
}

/// One contact point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Contact {
    /// World point between the surfaces.
    pub point: V3,
    /// Unit normal from A to B.
    pub normal: V3,
    /// Penetration depth (> 0 overlapping).
    pub depth: f32,
}

fn world_hull(h: &Hull, x: &Iso) -> Hull {
    let mut w = h.clone();
    for v in &mut w.verts {
        *v = x.apply(*v);
    }
    for f in &mut w.faces {
        f.normal = x.rotate(f.normal);
        f.d = f.normal.dot(w.verts[f.verts[0]]);
    }
    w
}

fn segment(half: f32, x: &Iso) -> (V3, V3) {
    (
        x.apply(V3::new(0.0, -half, 0.0)),
        x.apply(V3::new(0.0, half, 0.0)),
    )
}

/// Closest points between segments `p1q1` and `p2q2` (Ericson 5.1.9):
/// `(s, t, c1, c2)`.
#[must_use]
pub fn closest_segments(p1: V3, q1: V3, p2: V3, q2: V3) -> (f32, f32, V3, V3) {
    let d1 = q1 - p1;
    let d2 = q2 - p2;
    let r = p1 - p2;
    let a = d1.dot(d1);
    let e = d2.dot(d2);
    let f = d2.dot(r);
    let (s, t);
    if a <= 1e-12 && e <= 1e-12 {
        return (0.0, 0.0, p1, p2);
    }
    if a <= 1e-12 {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = d1.dot(r);
        if e <= 1e-12 {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            let mut ss = if denom > 1e-12 {
                ((b * f - c * e) / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let mut tt = (b * ss + f) / e;
            if tt < 0.0 {
                tt = 0.0;
                ss = (-c / a).clamp(0.0, 1.0);
            } else if tt > 1.0 {
                tt = 1.0;
                ss = ((b - c) / a).clamp(0.0, 1.0);
            }
            s = ss;
            t = tt;
        }
    }
    (s, t, p1 + d1 * s, p2 + d2 * t)
}

/// Closest point on segment `ab` to `p`.
#[must_use]
pub fn closest_on_segment(p: V3, a: V3, b: V3) -> V3 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-12)).clamp(0.0, 1.0);
    a + ab * t
}

// ── GJK distance ──────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct Sv {
    w: V3,
    a: V3,
    b: V3,
}

/// Closest point to the origin on triangle abc: point and barycentrics.
fn closest_triangle(a: V3, b: V3, c: V3) -> (V3, [f32; 3]) {
    // Ericson 5.1.5 with p = origin.
    let ab = b - a;
    let ac = c - a;
    let ap = -a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return (a, [1.0, 0.0, 0.0]);
    }
    let bp = -b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return (b, [0.0, 1.0, 0.0]);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return (a + ab * v, [1.0 - v, v, 0.0]);
    }
    let cp = -c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return (c, [0.0, 0.0, 1.0]);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return (a + ac * w, [1.0 - w, 0.0, w]);
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return (b + (c - b) * w, [0.0, 1.0 - w, w]);
    }
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    (a + ab * v + ac * w, [1.0 - v - w, v, w])
}

type Weighted = Vec<(Sv, f32)>;

fn reduce(simplex: &[Sv]) -> Option<(V3, Weighted)> {
    match simplex.len() {
        1 => Some((simplex[0].w, vec![(simplex[0], 1.0)])),
        2 => {
            let (a, b) = (simplex[0].w, simplex[1].w);
            let ab = b - a;
            let t = ((-a).dot(ab) / ab.length_sq().max(1e-20)).clamp(0.0, 1.0);
            let mut keep = Vec::new();
            if t < 1.0 {
                keep.push((simplex[0], 1.0 - t));
            }
            if t > 0.0 {
                keep.push((simplex[1], t));
            }
            Some((a + ab * t, keep))
        }
        3 => {
            let (p, bc) = closest_triangle(simplex[0].w, simplex[1].w, simplex[2].w);
            let keep = (0..3)
                .filter(|i| bc[*i] > 0.0)
                .map(|i| (simplex[i], bc[i]))
                .collect();
            Some((p, keep))
        }
        _ => {
            let [a, b, c, d] = [simplex[0].w, simplex[1].w, simplex[2].w, simplex[3].w];
            let outside = |p: V3, q: V3, r: V3, s: V3| {
                let n = (q - p).cross(r - p);
                let so = n.dot(-p);
                let sd = n.dot(s - p);
                so * sd < 0.0
            };
            let faces = [(0, 1, 2, 3), (0, 2, 3, 1), (0, 3, 1, 2), (1, 3, 2, 0)];
            let pts = [a, b, c, d];
            let mut best: Option<(f32, V3, Weighted)> = None;
            let mut any = false;
            for (i, j, k, l) in faces {
                if !outside(pts[i], pts[j], pts[k], pts[l]) {
                    continue;
                }
                any = true;
                let (p, bc) = closest_triangle(pts[i], pts[j], pts[k]);
                let d2 = p.length_sq();
                if best.as_ref().is_none_or(|(bd, ..)| d2 < *bd) {
                    let keep = [i, j, k]
                        .iter()
                        .zip(bc)
                        .filter(|(_, w)| *w > 0.0)
                        .map(|(ix, w)| (simplex[*ix], w))
                        .collect();
                    best = Some((d2, p, keep));
                }
            }
            if !any {
                return None; // origin inside the tetrahedron
            }
            best.map(|(_, p, k)| (p, k))
        }
    }
}

/// The GJK result: distance and the closest points on A and B, or `None`
/// if the shapes overlap.
#[must_use]
pub fn gjk_distance(
    support_a: &dyn Fn(V3) -> V3,
    support_b: &dyn Fn(V3) -> V3,
    initial: V3,
) -> Option<(f32, V3, V3)> {
    let sv = |d: V3| {
        let a = support_a(d);
        let b = support_b(-d);
        Sv { w: a - b, a, b }
    };
    let mut d = if initial.length_sq() > 1e-12 {
        -initial
    } else {
        V3::X
    };
    let mut simplex = vec![sv(d)];
    let mut last = f32::INFINITY;
    for _ in 0..64 {
        let (v, keep) = reduce(&simplex)?;
        let vv = v.length_sq();
        if vv < 1e-12 {
            return None;
        }
        let (mut pa, mut pb) = (V3::ZERO, V3::ZERO);
        for (s, w) in &keep {
            pa += s.a * *w;
            pb += s.b * *w;
        }
        d = -v;
        let w = sv(d);
        if vv - v.dot(w.w) <= 1e-6 * vv.max(1e-6) || vv >= last {
            return Some((vv.sqrt(), pa, pb));
        }
        last = vv;
        simplex = keep.into_iter().map(|(s, _)| s).collect();
        if simplex.iter().any(|s| (s.w - w.w).length_sq() < 1e-14) {
            return Some((vv.sqrt(), pa, pb));
        }
        simplex.push(w);
    }
    None
}

// ── Pairs ─────────────────────────────────────────────────────────────────

fn sphere_sphere(ca: V3, ra: f32, cb: V3, rb: f32) -> Vec<Contact> {
    let d = cb - ca;
    let dist = d.length();
    if dist >= ra + rb {
        return vec![];
    }
    let n = if dist > 1e-6 { d / dist } else { V3::Y };
    vec![Contact {
        point: ca + n * (ra - (ra + rb - dist) * 0.5),
        normal: n,
        depth: ra + rb - dist,
    }]
}

fn sphere_hull(c: V3, r: f32, h: &Hull) -> Vec<Contact> {
    // Deepest face separation.
    let (mut best_sep, mut best_face) = (f32::NEG_INFINITY, 0);
    for (i, f) in h.faces.iter().enumerate() {
        let s = f.normal.dot(c) - f.d;
        if s > best_sep {
            best_sep = s;
            best_face = i;
        }
    }
    if best_sep > r {
        return vec![];
    }
    if best_sep <= 0.0 {
        // Centre inside: push out along the least-penetrated face. Normal
        // A(sphere) → B(hull) is minus the face normal.
        let n = h.faces[best_face].normal;
        return vec![Contact {
            point: c - n * best_sep,
            normal: -n,
            depth: r - best_sep,
        }];
    }
    let hv = |d: V3| h.support(d);
    let pt = move |_d: V3| c;
    match gjk_distance(&pt, &hv, c - h.verts[0]) {
        Some((dist, _pa, pb)) if dist < r => {
            let n = (pb - c).normalize();
            vec![Contact {
                point: pb - n * ((r - dist) * 0.5),
                normal: n,
                depth: r - dist,
            }]
        }
        _ => vec![],
    }
}

fn capsule_hull(a: V3, b: V3, r: f32, h: &Hull) -> Vec<Contact> {
    let seg = move |d: V3| if (b - a).dot(d) > 0.0 { b } else { a };
    let hv = |d: V3| h.support(d);
    if let Some((dist, pa, pb)) = gjk_distance(&seg, &hv, (a + b) * 0.5 - h.verts[0]) {
        if dist >= r {
            return vec![];
        }
        let n = (pb - pa).normalize();
        let mut out = vec![Contact {
            point: pb - n * ((r - dist) * 0.5),
            normal: n,
            depth: r - dist,
        }];
        // Lying flat on a face: both caps touch.
        let axis = (b - a).normalize();
        if axis.dot(n).abs() < 0.15 {
            out.clear();
            for e in [a, b] {
                out.extend(sphere_hull(e, r, h));
            }
            if out.is_empty() {
                out.push(Contact {
                    point: pb - n * ((r - dist) * 0.5),
                    normal: n,
                    depth: r - dist,
                });
            }
        }
        return out;
    }
    // Core segment penetrates: least-penetrated face over both endpoints.
    let (mut best, mut face) = (f32::NEG_INFINITY, 0);
    for (i, f) in h.faces.iter().enumerate() {
        let s = (f.normal.dot(a) - f.d).min(f.normal.dot(b) - f.d);
        if s > best {
            best = s;
            face = i;
        }
    }
    let f = &h.faces[face];
    let mut out = Vec::new();
    for e in [a, b] {
        let s = f.normal.dot(e) - f.d;
        if s < r {
            out.push(Contact {
                point: e - f.normal * s,
                normal: -f.normal,
                depth: r - s,
            });
        }
    }
    out
}

fn capsule_capsule(a1: V3, b1: V3, r1: f32, a2: V3, b2: V3, r2: f32) -> Vec<Contact> {
    let d1 = (b1 - a1).normalize();
    let d2 = (b2 - a2).normalize();
    if d1.cross(d2).length() < 0.05 {
        // Parallel: clip one segment's projection onto the other.
        let mut out = Vec::new();
        for e in [a1, b1] {
            let q = closest_on_segment(e, a2, b2);
            out.extend(sphere_sphere(e, r1, q, r2));
        }
        if !out.is_empty() {
            return out;
        }
    }
    let (_, _, c1, c2) = closest_segments(a1, b1, a2, b2);
    sphere_sphere(c1, r1, c2, r2)
}

/// Project `h` onto axis `n`: (min, max).
fn project(h: &Hull, n: V3) -> (f32, f32) {
    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    for v in &h.verts {
        let d = n.dot(*v);
        lo = lo.min(d);
        hi = hi.max(d);
    }
    (lo, hi)
}

fn clip_polygon(poly: &[V3], n: V3, d: f32) -> Vec<V3> {
    let mut out = Vec::new();
    let m = poly.len();
    for i in 0..m {
        let (p, q) = (poly[i], poly[(i + 1) % m]);
        let (sp, sq) = (n.dot(p) - d, n.dot(q) - d);
        if sp <= 0.0 {
            out.push(p);
        }
        if (sp < 0.0 && sq > 0.0) || (sp > 0.0 && sq < 0.0) {
            out.push(p + (q - p) * (sp / (sp - sq)));
        }
    }
    out
}

fn hull_hull(a: &Hull, b: &Hull) -> Vec<Contact> {
    let ca = a.verts.iter().fold(V3::ZERO, |s, v| s + *v) / a.verts.len() as f32;
    let cb = b.verts.iter().fold(V3::ZERO, |s, v| s + *v) / b.verts.len() as f32;
    // (overlap, axis A→B, kind) kind: 0 face of A, 1 face of B, 2 edge.
    let mut best: (f32, V3, u8, usize, usize) = (f32::INFINITY, V3::Y, 0, 0, 0);
    let test =
        |axis: V3, kind: u8, i: usize, j: usize, best: &mut (f32, V3, u8, usize, usize)| -> bool {
            let l = axis.length();
            if l < 1e-6 {
                return true;
            }
            let mut n = axis / l;
            if n.dot(cb - ca) < 0.0 {
                n = -n;
            }
            let (alo, ahi) = project(a, n);
            let (blo, bhi) = project(b, n);
            let overlap = ahi.min(bhi) - alo.max(blo);
            if overlap <= 0.0 {
                return false;
            }
            // Prefer faces over edges by a small margin (stability).
            let bias = if kind == 2 { 0.98 } else { 1.0 };
            if overlap < best.0 * bias {
                *best = (overlap, n, kind, i, j);
            }
            true
        };
    for (i, f) in a.faces.iter().enumerate() {
        if !test(f.normal, 0, i, 0, &mut best) {
            return vec![];
        }
    }
    for (i, f) in b.faces.iter().enumerate() {
        if !test(f.normal, 1, i, 0, &mut best) {
            return vec![];
        }
    }
    for (i, ea) in a.edges.iter().enumerate() {
        let da = a.verts[ea.1] - a.verts[ea.0];
        for (j, eb) in b.edges.iter().enumerate() {
            let db = b.verts[eb.1] - b.verts[eb.0];
            let ax = da.cross(db);
            if ax.length_sq() < 1e-10 * da.length_sq() * db.length_sq() {
                continue;
            }
            if !test(ax, 2, i, j, &mut best) {
                return vec![];
            }
        }
    }
    let (depth, n, kind, i, j) = best;
    if kind == 2 {
        let (ea, eb) = (a.edges[i], b.edges[j]);
        let (_, _, c1, c2) =
            closest_segments(a.verts[ea.0], a.verts[ea.1], b.verts[eb.0], b.verts[eb.1]);
        return vec![Contact {
            point: (c1 + c2) * 0.5,
            normal: n,
            depth,
        }];
    }
    // Reference face on one hull, incident face on the other.
    let (rh, ih, flip) = if kind == 0 {
        (a, b, false)
    } else {
        (b, a, true)
    };
    let rn = if flip { -n } else { n };
    let rf = rh
        .faces
        .iter()
        .max_by(|x, y| x.normal.dot(rn).total_cmp(&y.normal.dot(rn)))
        .expect("faces");
    let inf = ih
        .faces
        .iter()
        .min_by(|x, y| x.normal.dot(rn).total_cmp(&y.normal.dot(rn)))
        .expect("faces");
    let mut poly: Vec<V3> = inf.verts.iter().map(|v| ih.verts[*v]).collect();
    let m = rf.verts.len();
    for k in 0..m {
        let p = rh.verts[rf.verts[k]];
        let q = rh.verts[rf.verts[(k + 1) % m]];
        let side_n = (q - p).cross(rf.normal).normalize();
        poly = clip_polygon(&poly, side_n, side_n.dot(p));
        if poly.is_empty() {
            break;
        }
    }
    let mut out = Vec::new();
    for p in poly {
        let s = rf.normal.dot(p) - rf.d;
        if s <= 0.0 {
            let on_ref = p - rf.normal * s;
            out.push(Contact {
                point: (p + on_ref) * 0.5,
                normal: n,
                depth: -s,
            });
        }
    }
    reduce_manifold(out)
}

/// Keep at most four points: the deepest, then the ones spanning the
/// largest area (Bullet's manifold reduction).
fn reduce_manifold(mut pts: Vec<Contact>) -> Vec<Contact> {
    if pts.len() <= 4 {
        return pts;
    }
    pts.sort_by(|a, b| b.depth.total_cmp(&a.depth));
    let mut keep = vec![pts[0]];
    let far = |keep: &[Contact], c: &Contact| {
        keep.iter()
            .map(|k| (k.point - c.point).length_sq())
            .fold(f32::INFINITY, f32::min)
    };
    while keep.len() < 4 {
        let next = pts
            .iter()
            .max_by(|a, b| far(&keep, a).total_cmp(&far(&keep, b)))
            .copied()
            .expect("points");
        if far(&keep, &next) < 1e-10 {
            break;
        }
        keep.push(next);
    }
    keep
}

fn plane_contacts(n: V3, d: f32, other: &Shape3, x: &Iso) -> Vec<Contact> {
    // Contacts with normal plane → other, i.e. A = plane.
    let mut out = Vec::new();
    let mut point = |p: V3, r: f32| {
        let s = n.dot(p) - d - r;
        if s < 0.0 {
            out.push(Contact {
                point: p - n * (r + s * 0.5),
                normal: n,
                depth: -s,
            });
        }
    };
    match other {
        Shape3::Sphere { radius } => point(x.p, *radius),
        Shape3::Capsule {
            half_height,
            radius,
        } => {
            let (a, b) = segment(*half_height, x);
            point(a, *radius);
            point(b, *radius);
        }
        Shape3::Convex { hull } => {
            for v in &hull.verts {
                point(x.apply(*v), 0.0);
            }
        }
        Shape3::Plane { .. } => {}
    }
    // The solid is n·x ≤ d and its normal points into free space, toward
    // the other body: +n is already "A (plane) → B".
    reduce_manifold(out)
}

/// Contacts between shape `a` at `xa` and `b` at `xb`, normal A → B.
#[must_use]
pub fn collide(a: &Shape3, xa: &Iso, b: &Shape3, xb: &Iso) -> Vec<Contact> {
    use Shape3::{Capsule, Convex, Plane, Sphere};
    let flip = |v: Vec<Contact>| -> Vec<Contact> {
        v.into_iter()
            .map(|c| Contact {
                normal: -c.normal,
                ..c
            })
            .collect()
    };
    match (a, b) {
        (Sphere { radius: ra }, Sphere { radius: rb }) => sphere_sphere(xa.p, *ra, xb.p, *rb),
        (
            Sphere { radius },
            Capsule {
                half_height,
                radius: rc,
            },
        ) => {
            let (p, q) = segment(*half_height, xb);
            sphere_sphere(xa.p, *radius, closest_on_segment(xa.p, p, q), *rc)
        }
        (Capsule { .. }, Sphere { .. }) => flip(collide(b, xb, a, xa)),
        (
            Capsule {
                half_height: h1,
                radius: r1,
            },
            Capsule {
                half_height: h2,
                radius: r2,
            },
        ) => {
            let (a1, b1) = segment(*h1, xa);
            let (a2, b2) = segment(*h2, xb);
            capsule_capsule(a1, b1, *r1, a2, b2, *r2)
        }
        (Sphere { radius }, Convex { hull }) => sphere_hull(xa.p, *radius, &world_hull(hull, xb)),
        (Convex { .. }, Sphere { .. }) => flip(collide(b, xb, a, xa)),
        (
            Capsule {
                half_height,
                radius,
            },
            Convex { hull },
        ) => {
            let (p, q) = segment(*half_height, xa);
            capsule_hull(p, q, *radius, &world_hull(hull, xb))
        }
        (Convex { .. }, Capsule { .. }) => flip(collide(b, xb, a, xa)),
        (Convex { hull: ha }, Convex { hull: hb }) => {
            hull_hull(&world_hull(ha, xa), &world_hull(hb, xb))
        }
        (Plane { normal }, other) => {
            let n = xa.rotate(*normal);
            plane_contacts(n, n.dot(xa.p), other, xb)
        }
        (_, Plane { .. }) => flip(collide(b, xb, a, xa)),
    }
}

/// Ray against a shape: distance and world normal of the first hit.
#[must_use]
pub fn ray_shape(origin: V3, dir: V3, max_t: f32, s: &Shape3, x: &Iso) -> Option<(f32, V3)> {
    let ray_sphere = |c: V3, r: f32| -> Option<(f32, V3)> {
        let m = origin - c;
        let b = m.dot(dir);
        let cc = m.length_sq() - r * r;
        if cc > 0.0 && b > 0.0 {
            return None;
        }
        let disc = b * b - cc;
        if disc < 0.0 {
            return None;
        }
        let t = (-b - disc.sqrt()).max(0.0);
        (t <= max_t).then(|| (t, (origin + dir * t - c).normalize()))
    };
    match s {
        Shape3::Sphere { radius } => ray_sphere(x.p, *radius),
        Shape3::Capsule {
            half_height,
            radius,
        } => {
            // March: spheres at the caps and a cylinder test by sampling the
            // closest approach (exact for the caps, analytic for the body).
            let (a, b) = segment(*half_height, x);
            let mut best: Option<(f32, V3)> = None;
            for c in [a, b] {
                if let Some(h) = ray_sphere(c, *radius) {
                    if best.is_none_or(|bb| h.0 < bb.0) {
                        best = Some(h);
                    }
                }
            }
            // Infinite cylinder around ab.
            let axis = (b - a).normalize();
            let m = origin - a;
            let dd = dir - axis * dir.dot(axis);
            let mm = m - axis * m.dot(axis);
            let qa = dd.length_sq();
            if qa > 1e-12 {
                let qb = 2.0 * dd.dot(mm);
                let qc = mm.length_sq() - radius * radius;
                let disc = qb * qb - 4.0 * qa * qc;
                if disc >= 0.0 {
                    let t = (-qb - disc.sqrt()) / (2.0 * qa);
                    if t >= 0.0 && t <= max_t {
                        let p = origin + dir * t;
                        let h = (p - a).dot(axis);
                        if (0.0..=(2.0 * half_height)).contains(&h)
                            && best.is_none_or(|bb| t < bb.0)
                        {
                            best = Some((t, (p - (a + axis * h)).normalize()));
                        }
                    }
                }
            }
            best
        }
        Shape3::Convex { hull } => {
            let w = world_hull(hull, x);
            let (mut t0, mut t1) = (0.0f32, max_t);
            let mut n0 = V3::ZERO;
            for f in &w.faces {
                let denom = f.normal.dot(dir);
                let dist = f.d - f.normal.dot(origin);
                if denom.abs() < 1e-12 {
                    if dist < 0.0 {
                        return None;
                    }
                    continue;
                }
                let t = dist / denom;
                if denom < 0.0 {
                    if t > t0 {
                        t0 = t;
                        n0 = f.normal;
                    }
                } else if t < t1 {
                    t1 = t;
                }
                if t0 > t1 {
                    return None;
                }
            }
            (n0 != V3::ZERO).then_some((t0, n0))
        }
        Shape3::Plane { normal } => {
            let n = x.rotate(*normal);
            let denom = n.dot(dir);
            if denom >= -1e-12 {
                return None;
            }
            let t = (n.dot(x.p) - n.dot(origin)) / denom;
            (t >= 0.0 && t <= max_t).then_some((t, n))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(p: V3) -> Iso {
        Iso::new(p, Q::IDENTITY)
    }

    #[test]
    fn gjk_measures_the_gap_between_two_boxes() {
        let a = Hull::cuboid(V3::splat(0.5));
        let b = Hull::cuboid(V3::splat(0.5)).translated(V3::new(3.0, 0.2, 0.0));
        let sa = |d: V3| a.support(d);
        let sb = |d: V3| b.support(d);
        let (d, pa, pb) = gjk_distance(&sa, &sb, V3::X).expect("apart");
        assert!((d - 2.0).abs() < 1e-4, "{d}");
        assert!((pa.x - 0.5).abs() < 1e-4 && (pb.x - 2.5).abs() < 1e-4);
        let c = Hull::cuboid(V3::splat(0.5)).translated(V3::new(0.6, 0.0, 0.0));
        let sc = |d: V3| c.support(d);
        assert!(gjk_distance(&sa, &sc, V3::X).is_none());
    }

    #[test]
    fn a_box_resting_on_a_box_gets_a_four_point_face_manifold() {
        let s = Shape3::cuboid(V3::splat(0.5));
        let c = collide(&s, &at(V3::ZERO), &s, &at(V3::new(0.1, 0.98, 0.05)));
        assert_eq!(c.len(), 4);
        for k in &c {
            assert!((k.normal - V3::Y).length() < 1e-4, "{k:?}");
            assert!((k.depth - 0.02).abs() < 1e-3);
        }
    }

    #[test]
    fn rotated_edge_on_edge_contact_is_found() {
        let s = Shape3::cuboid(V3::splat(0.5));
        let qa = Q::from_axis_angle(V3::Z, std::f32::consts::FRAC_PI_4);
        let qb = Q::from_axis_angle(V3::X, std::f32::consts::FRAC_PI_4);
        let h = 0.5 * 2f32.sqrt();
        let c = collide(
            &s,
            &Iso::new(V3::ZERO, qa),
            &s,
            &Iso::new(V3::new(0.0, 2.0 * h - 0.05, 0.0), qb),
        );
        assert_eq!(c.len(), 1);
        assert!(c[0].normal.y > 0.99);
        assert!((c[0].depth - 0.05).abs() < 2e-3, "{:?}", c[0]);
    }

    #[test]
    fn spheres_capsules_and_planes_pair_up() {
        let ball = Shape3::sphere(0.5);
        let cap = Shape3::capsule(1.0, 0.25);
        let floor = Shape3::plane(V3::Y);
        let c = collide(&floor, &at(V3::ZERO), &ball, &at(V3::new(0.0, 0.4, 0.0)));
        assert_eq!(c.len(), 1);
        assert!((c[0].normal - V3::Y).length() < 1e-5 && (c[0].depth - 0.1).abs() < 1e-5);
        // A capsule lying on the floor touches with both caps.
        let lying = Iso::new(
            V3::new(0.0, 0.2, 0.0),
            Q::from_axis_angle(V3::Z, std::f32::consts::FRAC_PI_2),
        );
        let c = collide(&floor, &at(V3::ZERO), &cap, &lying);
        assert_eq!(c.len(), 2);
        // Sphere against capsule, side on.
        let c = collide(&ball, &at(V3::new(0.7, 0.3, 0.0)), &cap, &at(V3::ZERO));
        assert_eq!(c.len(), 1);
        assert!(c[0].normal.x < -0.99);
        // Sphere on a box, from above.
        let bx = Shape3::cuboid(V3::splat(1.0));
        let c = collide(&ball, &at(V3::new(0.2, 1.45, 0.0)), &bx, &at(V3::ZERO));
        assert_eq!(c.len(), 1);
        assert!(
            c[0].normal.y < -0.99 && (c[0].depth - 0.05).abs() < 1e-3,
            "{:?}",
            c
        );
        // Capsule lying on a box: two contacts.
        let c = collide(
            &cap,
            &Iso::new(V3::new(0.0, 1.2, 0.0), lying.q),
            &bx,
            &at(V3::ZERO),
        );
        assert_eq!(c.len(), 2, "{c:?}");
    }

    #[test]
    fn rays_hit_each_shape_kind() {
        let o = V3::new(0.0, 5.0, 0.0);
        let d = -V3::Y;
        let (t, n) = ray_shape(o, d, 100.0, &Shape3::sphere(1.0), &at(V3::ZERO)).expect("sphere");
        assert!((t - 4.0).abs() < 1e-4 && n.y > 0.99);
        let (t, _) =
            ray_shape(o, d, 100.0, &Shape3::cuboid(V3::splat(1.0)), &at(V3::ZERO)).expect("box");
        assert!((t - 4.0).abs() < 1e-4);
        let (t, _) =
            ray_shape(o, d, 100.0, &Shape3::capsule(1.0, 0.5), &at(V3::ZERO)).expect("cap");
        assert!((t - 3.5).abs() < 1e-4, "{t}");
        let (t, _) = ray_shape(o, d, 100.0, &Shape3::plane(V3::Y), &at(V3::ZERO)).expect("plane");
        assert!((t - 5.0).abs() < 1e-4);
        assert!(ray_shape(o, V3::Y, 100.0, &Shape3::sphere(1.0), &at(V3::ZERO)).is_none());
        // Side hit on the capsule body.
        let (t, n) = ray_shape(
            V3::new(5.0, 0.3, 0.0),
            -V3::X,
            100.0,
            &Shape3::capsule(1.0, 0.5),
            &at(V3::ZERO),
        )
        .expect("side");
        assert!((t - 4.5).abs() < 1e-4 && n.x > 0.99);
    }
}
