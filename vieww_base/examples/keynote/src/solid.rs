//! solid — the film's own little 3D: vectors, a camera, quad meshes, and a
//! painter's-algorithm draw.
//!
//! There is no depth buffer here and no GPU. A mesh is a list of quads; the
//! draw sorts them back-to-front by their centre's camera depth, shades each
//! by a Lambert term against one light, and fills it as an ordinary path.
//! That is the whole trick, and it is enough for the three set-pieces that
//! need volume: the crate constellation, the unfolding trees, and the
//! device tour's slabs.
//!
//! Two lessons from the bench are baked in rather than rediscovered:
//! - **Abutting quads do not meet.** Two paths sharing an edge are each
//!   anti-aliased against what is already there, so a hairline of ground
//!   survives between them and a shaded grid reads as a wireframe nobody
//!   asked for. Every quad is drawn `bleed` past its own outline.
//! - **Blur is priced per layer.** Nothing here wraps a quad in its own
//!   layer; a whole mesh's glow is one group, blurred once.

use vieww_foundation::{BlendMode, Brush, Color, Offset, Path, Size, Sketchbook};

use crate::kit::{alpha, mix, scaled};

// ── Vectors ─────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Default)]
pub struct V3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[must_use]
pub const fn v3(x: f32, y: f32, z: f32) -> V3 {
    V3 { x, y, z }
}

impl V3 {
    #[must_use]
    pub fn add(self, o: Self) -> Self {
        v3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
    #[must_use]
    pub fn sub(self, o: Self) -> Self {
        v3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
    #[must_use]
    pub fn mul(self, k: f32) -> Self {
        v3(self.x * k, self.y * k, self.z * k)
    }
    #[must_use]
    pub fn dot(self, o: Self) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    #[must_use]
    pub fn cross(self, o: Self) -> Self {
        v3(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    #[must_use]
    pub fn len(self) -> f32 {
        self.dot(self).sqrt()
    }
    #[must_use]
    pub fn norm(self) -> Self {
        let l = self.len();
        if l < 1e-6 { v3(0.0, 0.0, 0.0) } else { self.mul(1.0 / l) }
    }
    #[must_use]
    pub fn lerp(self, o: Self, t: f32) -> Self {
        self.add(o.sub(self).mul(t))
    }
    #[must_use]
    pub fn rot_y(self, a: f32) -> Self {
        let (s, c) = a.sin_cos();
        v3(self.x * c + self.z * s, self.y, -self.x * s + self.z * c)
    }
    #[must_use]
    pub fn rot_x(self, a: f32) -> Self {
        let (s, c) = a.sin_cos();
        v3(self.x, self.y * c - self.z * s, self.y * s + self.z * c)
    }
    #[must_use]
    pub fn rot_z(self, a: f32) -> Self {
        let (s, c) = a.sin_cos();
        v3(self.x * c - self.y * s, self.x * s + self.y * c, self.z)
    }
}

// ── The camera ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub struct Cam {
    pub eye: V3,
    pub yaw: f32,
    pub pitch: f32,
    /// Focal length in pixels at the canvas's own scale.
    pub focal: f32,
}

impl Default for Cam {
    fn default() -> Self {
        Self {
            eye: v3(0.0, 0.0, -9.0),
            yaw: 0.0,
            pitch: 0.0,
            focal: 1150.0,
        }
    }
}

impl Cam {
    /// Project a world point. `None` behind the eye plane. Returns the screen
    /// point, the camera-space depth, and the perspective scale — the last is
    /// what makes a dot's radius and a stroke's width honest.
    #[must_use]
    pub fn project(&self, p: V3, canvas: Size) -> Option<(Offset, f32, f32)> {
        let q = p.sub(self.eye).rot_y(-self.yaw).rot_x(-self.pitch);
        if q.z <= 0.12 {
            return None;
        }
        let k = self.focal / q.z;
        Some((
            Offset::new(canvas.width * 0.5 + q.x * k, canvas.height * 0.5 - q.y * k),
            q.z,
            k / self.focal,
        ))
    }

    /// Pixels per unit of "design size" at a projected point's depth.
    ///
    /// `project` hands back `k = 1/z`: a ratio, not a scale. A caller that
    /// wants a glyph or a radius to shrink with distance wants `k * focal`,
    /// and dividing by the focal length instead — which is the shape the
    /// mistake takes — makes everything collapse to its clamp. This is that
    /// arithmetic, written once, normalised so `1.0` is "the size you would
    /// have written for a flat 2D scene at the film's usual depth".
    #[must_use]
    pub fn px(&self, k: f32) -> f32 {
        k * self.focal / 100.0
    }

    /// Depth only — the sort key, without the projection's cost.
    #[must_use]
    pub fn depth(&self, p: V3) -> f32 {
        p.sub(self.eye).rot_y(-self.yaw).rot_x(-self.pitch).z
    }
}

// ── Meshes ──────────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct Mesh {
    pub pts: Vec<V3>,
    pub quads: Vec<([usize; 4], Color)>,
}

impl Mesh {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn point(&mut self, p: V3) -> usize {
        self.pts.push(p);
        self.pts.len() - 1
    }

    pub fn quad(&mut self, a: usize, b: usize, c: usize, d: usize, color: Color) {
        self.quads.push(([a, b, c, d], color));
    }

    #[must_use]
    pub fn normal(&self, q: &[usize; 4]) -> V3 {
        let (a, b, c) = (self.pts[q[0]], self.pts[q[1]], self.pts[q[2]]);
        b.sub(a).cross(c.sub(a)).norm()
    }

    #[must_use]
    pub fn center(&self, q: &[usize; 4]) -> V3 {
        q.iter()
            .fold(v3(0.0, 0.0, 0.0), |acc, i| acc.add(self.pts[*i]))
            .mul(0.25)
    }

    /// Move every point through a map — how the unfold animates: one mesh,
    /// re-posed per frame, never rebuilt from scratch mid-draw.
    pub fn warp(&mut self, f: impl Fn(V3) -> V3) {
        for p in &mut self.pts {
            *p = f(*p);
        }
    }
}

/// An axis-aligned box centred at `c` with half-extents `h`, six faces, each
/// tinted from `color` so the volume reads without a light.
#[must_use]
pub fn box_mesh(c: V3, h: V3, color: Color) -> Mesh {
    let mut m = Mesh::new();
    let s = [-1.0f32, 1.0];
    let mut idx = [[0usize; 2]; 4];
    let mut ids = Vec::with_capacity(8);
    for sx in s {
        for sy in s {
            for sz in s {
                ids.push(m.point(v3(c.x + sx * h.x, c.y + sy * h.y, c.z + sz * h.z)));
            }
        }
    }
    let _ = &mut idx;
    // Index order: bit 2 = x, bit 1 = y, bit 0 = z.
    let p = |x: usize, y: usize, z: usize| ids[x * 4 + y * 2 + z];
    // -z, +z, -y, +y, -x, +x
    m.quad(p(0, 0, 0), p(1, 0, 0), p(1, 1, 0), p(0, 1, 0), color);
    m.quad(p(0, 0, 1), p(0, 1, 1), p(1, 1, 1), p(1, 0, 1), color);
    m.quad(p(0, 0, 0), p(0, 0, 1), p(1, 0, 1), p(1, 0, 0), color);
    m.quad(p(0, 1, 0), p(1, 1, 0), p(1, 1, 1), p(0, 1, 1), color);
    m.quad(p(0, 0, 0), p(0, 1, 0), p(0, 1, 1), p(0, 0, 1), color);
    m.quad(p(1, 0, 0), p(1, 0, 1), p(1, 1, 1), p(1, 1, 0), color);
    m
}

/// A flat slab in the x/y plane, subdivided — the "panel floating in space"
/// the crate wall and the device tour are both made of.
#[must_use]
pub fn slab(c: V3, w: f32, h: f32, cols: usize, rows: usize, color: Color) -> Mesh {
    let mut m = Mesh::new();
    let mut grid = Vec::with_capacity((cols + 1) * (rows + 1));
    for r in 0..=rows {
        for col in 0..=cols {
            let u = col as f32 / cols as f32;
            let v = r as f32 / rows as f32;
            grid.push(m.point(v3(c.x + (u - 0.5) * w, c.y + (0.5 - v) * h, c.z)));
        }
    }
    let at = |col: usize, r: usize| grid[r * (cols + 1) + col];
    for r in 0..rows {
        for col in 0..cols {
            m.quad(at(col, r), at(col + 1, r), at(col + 1, r + 1), at(col, r + 1), color);
        }
    }
    m
}

// ── Style and draw ──────────────────────────────────────────────────────────

pub struct Style {
    /// Direction *toward* the light.
    pub light: V3,
    pub ambient: f32,
    /// Wireframe on top of the fills.
    pub wire: Option<(Color, f32)>,
    /// A rim colour mixed in where the face turns away — the film's edge
    /// light, which is what stops dark volumes reading as holes.
    pub rim: Option<(Color, f32)>,
    /// Fraction of a quad's own size each fill is bled past its outline, to
    /// close the conflation seam.
    pub bleed: f32,
    /// Global opacity.
    pub alpha: f32,
    /// Cull back faces. Off for open shells (the trees' sheets).
    pub cull: bool,
    /// Fog: faces fade toward this colour with depth, between the two
    /// distances given.
    pub fog: Option<(Color, f32, f32)>,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            light: v3(-0.4, 0.8, -0.55).norm(),
            ambient: 0.34,
            wire: None,
            rim: None,
            bleed: 0.006,
            alpha: 1.0,
            cull: true,
            fog: None,
        }
    }
}

/// Draw a mesh, back to front. One pass, no layers — the caller groups if it
/// wants glow.
pub fn draw(book: &mut Sketchbook, m: &Mesh, cam: &Cam, canvas: Size, st: &Style) {
    let mut order: Vec<(f32, usize)> = m
        .quads
        .iter()
        .enumerate()
        .map(|(i, (q, _))| (cam.depth(m.center(q)), i))
        .collect();
    order.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    for (depth, qi) in order {
        if depth <= 0.12 {
            continue;
        }
        let (q, base) = &m.quads[qi];
        let n = m.normal(q);
        let to_eye = cam.eye.sub(m.center(q)).norm();
        let facing = n.dot(to_eye);
        if st.cull && facing <= 0.0 {
            continue;
        }
        let lambert = n.dot(st.light).abs();
        let mut c = scaled(*base, st.ambient + (1.0 - st.ambient) * lambert);
        if let Some((rim, k)) = st.rim {
            c = mix(c, rim, (1.0 - facing.abs()).powi(3) * k);
        }
        if let Some((fog, near, far)) = st.fog {
            c = mix(c, fog, crate::kit::clamp01((depth - near) / (far - near).max(1e-3)));
        }

        // The bleed: push each corner away from the quad's centre.
        let ctr = m.center(q);
        let mut path = Path::new();
        let mut first = true;
        let mut ok = true;
        for i in q {
            let p = m.pts[*i];
            let out = p.add(p.sub(ctr).mul(st.bleed * 2.0));
            match cam.project(out, canvas) {
                Some((s, _, _)) => {
                    if first {
                        path.move_to(s);
                        first = false;
                    } else {
                        path.line_to(s);
                    }
                }
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            continue;
        }
        path.close();
        book.fill(path.clone(), alpha(c, st.alpha * (f32::from(c.a) / 255.0)));
        if let Some((wc, ww)) = st.wire {
            book.stroke(path, alpha(wc, st.alpha), ww);
        }
    }
}

/// A polyline in space, as one stroked path with per-segment culling.
pub fn line3(
    book: &mut Sketchbook,
    pts: &[V3],
    cam: &Cam,
    canvas: Size,
    brush: impl Into<Brush>,
    width: f32,
) {
    let mut path = Path::new();
    let mut open = false;
    for p in pts {
        match cam.project(*p, canvas) {
            Some((s, _, _)) => {
                if open {
                    path.line_to(s);
                } else {
                    path.move_to(s);
                    open = true;
                }
            }
            None => open = false,
        }
    }
    if !path.is_empty() {
        book.stroke(path, brush, width);
    }
}

/// A dot in space whose radius respects perspective.
pub fn dot3(book: &mut Sketchbook, p: V3, cam: &Cam, canvas: Size, r: f32, color: Color) {
    if let Some((s, _, k)) = cam.project(p, canvas) {
        book.circle(s, (r * k).max(0.35), color);
    }
}

/// A glowing dot field, one additive group for the lot — the cheap way to
/// make a constellation without buying N offscreens.
pub fn spark_field(
    book: &mut Sketchbook,
    pts: &[(V3, Color, f32)],
    cam: &Cam,
    canvas: Size,
    blur: f32,
) {
    book.blended_layer(1.0, blur, BlendMode::Plus, None, |g| {
        for (p, c, r) in pts {
            dot3(g, *p, cam, canvas, *r, *c);
        }
    });
}

/// The world's floor: a receding grid, drawn with distance fade. Gives every
/// 3D set-piece a ground so the volumes are not floating in nothing.
pub fn floor(
    book: &mut Sketchbook,
    cam: &Cam,
    canvas: Size,
    y: f32,
    half: f32,
    step: f32,
    color: Color,
) {
    let mut x = -half;
    while x <= half + 1e-3 {
        let fade = 1.0 - (x.abs() / half).powi(2);
        line3(
            book,
            &[v3(x, y, -half), v3(x, y, half)],
            cam,
            canvas,
            alpha(color, 0.18 * fade),
            1.0,
        );
        x += step;
    }
    let mut z = -half;
    while z <= half + 1e-3 {
        let fade = 1.0 - (z.abs() / half).powi(2);
        line3(
            book,
            &[v3(-half, y, z), v3(half, y, z)],
            cam,
            canvas,
            alpha(color, 0.18 * fade),
            1.0,
        );
        z += step;
    }
}
