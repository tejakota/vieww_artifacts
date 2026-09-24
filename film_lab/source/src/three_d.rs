//! three_d — a software 3D pipeline drawn through vieww's own Sketchbook.
//!
//! The widget layer's `Transform` is 2D affine, deliberately (see
//! `vieww-foundation/src/transform.rs`: "Perspective would need the full 3x3
//! and is deliberately out of scope"). The film's reveal needs depth anyway —
//! so instead of fighting the framework, this module does what the paint
//! layer is *for*: it projects vertices itself and emits ordinary `Path`s.
//! The renderer's 4x supersampling, gradients and anti-aliased strokes do
//! the rest. Every 3D pixel in the film is still the framework's own raster.
//!
//! Shading strategy (ratified for round one): **flat + per-face gradient** —
//! each face fills with a two-stop ramp from its lit color to its shaded
//! color, plus depth fog toward the horizon. Cheap, deterministic, and it
//! reads as depth instantly.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook};

use crate::film_lib::{alpha, clamp01, mix, scaled};

// ── Vector math ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0, z: 0.0 };

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }

    pub fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }

    pub fn scale(self, k: f32) -> Self {
        Self::new(self.x * k, self.y * k, self.z * k)
    }

    pub fn dot(self, o: Self) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn cross(self, o: Self) -> Self {
        Self::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    pub fn len(self) -> f32 {
        self.dot(self).sqrt()
    }

    pub fn norm(self) -> Self {
        let l = self.len();
        if l < 1e-9 { Self::ZERO } else { self.scale(1.0 / l) }
    }

    pub fn lerp(self, o: Self, t: f32) -> Self {
        Self::new(
            self.x + (o.x - self.x) * t,
            self.y + (o.y - self.y) * t,
            self.z + (o.z - self.z) * t,
        )
    }

    /// Rotate around the Y (vertical) axis by `a` radians.
    pub fn rot_y(self, a: f32) -> Self {
        let (s, c) = a.sin_cos();
        Self::new(self.x * c + self.z * s, self.y, -self.x * s + self.z * c)
    }

    /// Rotate around the X axis by `a` radians.
    pub fn rot_x(self, a: f32) -> Self {
        let (s, c) = a.sin_cos();
        Self::new(self.x, self.y * c - self.z * s, self.y * s + self.z * c)
    }
}

// ── Camera ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    /// Vertical field of view, radians.
    pub fov: f32,
}

impl Camera {
    /// Project a world point to canvas space. `None` if behind the near plane.
    /// Returns the point, the forward depth (for painter sorting + fog), and
    /// the perspective scale factor (for size cues).
    pub fn project(&self, p: Vec3, canvas: Size) -> Option<(Offset, f32, f32)> {
        let forward = self.target.sub(self.eye).norm();
        let world_up = Vec3::new(0.0, 1.0, 0.0);
        let right = forward.cross(world_up).norm();
        let up = right.cross(forward).norm();

        let d = p.sub(self.eye);
        let depth = d.dot(forward);
        if depth < 0.1 {
            return None;
        }
        let tan_half = (self.fov * 0.5).tan();
        let aspect = canvas.width / canvas.height;
        let x = d.dot(right) / (depth * tan_half * aspect);
        let y = d.dot(up) / (depth * tan_half);

        let sx = (x * 0.5 + 0.5) * canvas.width;
        let sy = (0.5 - y * 0.5) * canvas.height;
        Some((Offset::new(sx, sy), depth, 1.0 / depth))
    }
}

// ── Mesh ────────────────────────────────────────────────────────────────────

/// A quad-indexed mesh. Quads project more cheaply than triangles and the
/// rasterizer's path fill doesn't care either way.
#[derive(Debug, Clone)]
pub struct Mesh {
    pub verts: Vec<Vec3>,
    /// Quad vertex indices, counter-clockwise seen from the front.
    pub quads: Vec<[usize; 4]>,
    /// Per-quad base color.
    pub colors: Vec<Color>,
}

impl Mesh {
    pub fn new() -> Self {
        Self {
            verts: Vec::new(),
            quads: Vec::new(),
            colors: Vec::new(),
        }
    }

    pub fn push_quad(&mut self, a: usize, b: usize, c: usize, d: usize, color: Color) {
        self.quads.push([a, b, c, d]);
        self.colors.push(color);
    }

    /// Face normal (Newell's method — robust for near-degenerate quads).
    pub fn quad_normal(&self, q: &[usize; 4]) -> Vec3 {
        let vs: [Vec3; 4] = q.map(|i| self.verts[i]);
        let mut n = Vec3::ZERO;
        for i in 0..4 {
            let cur = vs[i];
            let next = vs[(i + 1) % 4];
            n.x += (cur.y - next.y) * (cur.z + next.z);
            n.y += (cur.z - next.z) * (cur.x + next.x);
            n.z += (cur.x - next.x) * (cur.y + next.y);
        }
        n.norm()
    }

    pub fn quad_center(&self, q: &[usize; 4]) -> Vec3 {
        let c: Vec3 = q.iter().map(|&i| self.verts[i]).fold(Vec3::ZERO, Vec3::add);
        c.scale(0.25)
    }
}

// ── Mesh builders ───────────────────────────────────────────────────────────

/// A flat grid on the XZ plane, `nx × nz` cells, cell size `step`.
pub fn grid_mesh(nx: usize, nz: usize, step: f32, color: Color) -> Mesh {
    let mut m = Mesh::new();
    for z in 0..=nz {
        for x in 0..=nx {
            m.verts.push(Vec3::new(x as f32 * step, 0.0, z as f32 * step));
        }
    }
    let row = nx + 1;
    for z in 0..nz {
        for x in 0..nx {
            let a = z * row + x;
            let b = a + 1;
            let c = a + row + 1;
            let d = a + row;
            // Winding a→d→c→b: Newell normal points +y, so a floor seen
            // from above passes the screen-space back-face cull.
            m.push_quad(a, d, c, b, color);
        }
    }
    m
}

/// Displace verts' y by a height function.
pub fn displace_y(m: &mut Mesh, f: impl Fn(Vec3) -> f32) {
    for v in &mut m.verts {
        v.y = f(*v);
    }
}

/// A torus in the XZ plane, `nseg` around the tube, `nring` around the ring.
pub fn torus_mesh(big: f32, small: f32, nseg: usize, nring: usize, color: Color) -> Mesh {
    let mut m = Mesh::new();
    for ring in 0..nring {
        let theta = ring as f32 / nring as f32 * std::f32::consts::TAU;
        let (st, ct) = theta.sin_cos();
        for seg in 0..nseg {
            let phi = seg as f32 / nseg as f32 * std::f32::consts::TAU;
            let (sp, cp) = phi.sin_cos();
            let r = big + small * cp;
            m.verts.push(Vec3::new(r * ct, small * sp, r * st));
        }
    }
    for ring in 0..nring {
        for seg in 0..nseg {
            let a = ring * nseg + seg;
            let b = ring * nseg + (seg + 1) % nseg;
            let c = ((ring + 1) % nring) * nseg + (seg + 1) % nseg;
            let d = ((ring + 1) % nring) * nseg + seg;
            m.push_quad(a, b, c, d, color);
        }
    }
    m
}

/// A box centered at `center`, half-extents `h`.
pub fn box_mesh(center: Vec3, h: Vec3, color: Color) -> Mesh {
    let mut m = Mesh::new();
    let c = |dx: f32, dy: f32, dz: f32| center.add(Vec3::new(dx * h.x, dy * h.y, dz * h.z));
    // 8 corners, CCW seen from outside.
    let v: [Vec3; 8] = [
        c(-1.0, -1.0, 1.0), c(1.0, -1.0, 1.0), c(1.0, 1.0, 1.0), c(-1.0, 1.0, 1.0), // front z+
        c(1.0, -1.0, -1.0), c(-1.0, -1.0, -1.0), c(-1.0, 1.0, -1.0), c(1.0, 1.0, -1.0), // back z-
    ];
    m.verts = v.to_vec();
    let c7 = color;
    m.push_quad(0, 1, 2, 3, c7); // +z
    m.push_quad(4, 5, 6, 7, shade(color, 0.25)); // -z
    m.push_quad(1, 4, 7, 2, shade(color, 0.10)); // +x
    m.push_quad(5, 0, 3, 6, shade(color, 0.35)); // -x
    m.push_quad(3, 2, 7, 6, tint(color, 0.18)); // +y
    m.push_quad(5, 4, 1, 0, shade(color, 0.45)); // -y
    m
}

fn shade(c: Color, t: f32) -> Color {
    mix(c, Color::BLACK, t)
}

fn tint(c: Color, t: f32) -> Color {
    mix(c, Color::WHITE, t)
}

// ── Drawing ─────────────────────────────────────────────────────────────────

/// How a mesh renders.
#[derive(Debug, Clone, Copy)]
pub struct MeshStyle {
    /// Direction *toward* the light, normalized.
    pub light_dir: Vec3,
    /// Ambient floor added to the Lambert term.
    pub ambient: f32,
    /// Fog color and the depth range over which it takes over.
    pub fog_color: Color,
    pub fog_start: f32,
    pub fog_end: f32,
    /// Translucent edge stroke over each face (wireframe kiss). 0 = off.
    pub edge_alpha: f32,
    pub edge_width: f32,
    /// When set, faces fill flat (no per-face ramp) — for the "unlit" beats.
    pub flat: bool,
    /// Blinn-Phong specular strength (0 = off). The glint that sells form.
    pub specular: f32,
    /// Specular exponent — how tight the glint is.
    pub shininess: f32,
    /// Per-face ramp contrast: light lift at the top stop.
    pub ramp_light: f32,
    /// Per-face ramp contrast: dark pull at the bottom stop.
    pub ramp_dark: f32,
}

impl Default for MeshStyle {
    fn default() -> Self {
        Self {
            light_dir: Vec3::new(-0.45, 0.8, 0.35).norm(),
            ambient: 0.22,
            fog_color: Color::rgb(8, 8, 13),
            fog_start: 14.0,
            fog_end: 46.0,
            edge_alpha: 0.16,
            edge_width: 1.0,
            flat: false,
            specular: 0.0,
            shininess: 28.0,
            ramp_light: 0.14,
            ramp_dark: 0.38,
        }
    }
}

/// Draw a mesh: project, back-face cull, painter-sort far→near, shade.
pub fn draw_mesh(book: &mut Sketchbook, mesh: &Mesh, cam: &Camera, canvas: Size, style: &MeshStyle) {
    // Project all verts once.
    let projected: Vec<Option<(Offset, f32, f32)>> =
        mesh.verts.iter().map(|v| cam.project(*v, canvas)).collect();

    // Build the draw list: (avg depth, quad idx, screen pts).
    let mut faces: Vec<(f32, usize, [Offset; 4])> = Vec::new();
    for (qi, q) in mesh.quads.iter().enumerate() {
        let mut pts = [Offset::new(0.0, 0.0); 4];
        let mut ok = true;
        let mut depth = 0.0;
        for (k, &vi) in q.iter().enumerate() {
            match projected[vi] {
                Some((pt, d, _)) => {
                    pts[k] = pt;
                    depth += d;
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
        let depth = depth / 4.0;

        // Back-face cull: screen-space cross product of the first edge pair.
        // (Offset has no arithmetic — compute components directly.)
        let ax = pts[1].dx - pts[0].dx;
        let ay = pts[1].dy - pts[0].dy;
        let bx = pts[2].dx - pts[1].dx;
        let by = pts[2].dy - pts[1].dy;
        let area = ax * by - ay * bx;
        if area >= 0.0 {
            continue; // clockwise on screen → facing away
        }

        faces.push((depth, qi, pts));
    }

    // Painter's algorithm: far first.
    faces.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let forward = cam.target.sub(cam.eye).norm();
    for (depth, qi, pts) in faces {
        let base = mesh.colors[qi];
        let normal = mesh.quad_normal(&mesh.quads[qi]);
        // Double-sided lighting: flip the normal if it faces away from the camera.
        let n = if normal.dot(forward) < 0.0 { normal.scale(-1.0) } else { normal };
        let lambert = (n.dot(style.light_dir)).max(0.0);
        let mut lit = scaled(base, style.ambient + (1.0 - style.ambient) * lambert);

        // Blinn-Phong glint: the half-vector between light and view, against
        // the face normal. One bright face beats a hundred evenly-lit ones.
        if style.specular > 0.0 && lambert > 0.0 {
            let center = mesh.quad_center(&mesh.quads[qi]);
            let view_dir = center.sub(cam.eye).norm();
            let half = style.light_dir.add(view_dir).norm();
            let spec = (n.dot(half)).max(0.0).powf(style.shininess) * style.specular;
            if spec > 0.0 {
                lit = mix(lit, Color::WHITE, spec.min(1.0));
            }
        }

        // Depth fog.
        let fog = clamp01((depth - style.fog_start) / (style.fog_end - style.fog_start));
        let near = mix(lit, style.fog_color, fog);

        let mut path = Path::new();
        path.move_to(pts[0]);
        for p in &pts[1..] {
            path.line_to(*p);
        }
        path.close();

        if style.flat {
            book.fill(path.clone(), near);
        } else {
            // Two-stop vertical ramp across the face's own bbox — the
            // "flat + gradient" shading strategy, resolved per face.
            let gradient = Gradient::vertical().with_stops(&[
                (0.0, mix(near, Color::WHITE, style.ramp_light)),
                (1.0, mix(near, Color::BLACK, style.ramp_dark)),
            ]);
            book.fill(path.clone(), gradient);
        }

        if style.edge_alpha > 0.0 {
            book.stroke(path, alpha(Color::WHITE, style.edge_alpha), style.edge_width);
        }
    }
}

/// Draw a 3D polyline (already a world-space path) projected to screen.
pub fn draw_polyline3(
    book: &mut Sketchbook,
    points: &[Vec3],
    cam: &Camera,
    canvas: Size,
    brush: impl Into<vieww_foundation::Brush>,
    width: f32,
) {
    let mut path = Path::new();
    let mut started = false;
    for p in points {
        if let Some((pt, _, _)) = cam.project(*p, canvas) {
            if started {
                path.line_to(pt);
            } else {
                path.move_to(pt);
                started = true;
            }
        }
    }
    if started {
        book.stroke(path, brush, width);
    }
}

/// A projected, filled disc in 3D (the drop, the sparkles).
pub fn draw_dot3(
    book: &mut Sketchbook,
    p: Vec3,
    radius_world: f32,
    cam: &Camera,
    canvas: Size,
    brush: impl Into<vieww_foundation::Brush>,
) {
    if let Some((pt, depth, scale)) = cam.project(p, canvas) {
        let r = (radius_world * scale * 900.0).max(0.5);
        let _ = depth;
        book.circle(pt, r, brush);
    }
}

/// The horizon glow band behind a 3D scene — one wide gradient rect.
pub fn horizon(book: &mut Sketchbook, canvas: Size, color: Color) {
    let rect = Rect::new(0.0, 0.0, canvas.width, canvas.height);
    let g = vieww_foundation::Gradient::radial(
        Offset::new(0.5, 0.62),
        0.9,
    )
    .with_stops(&[(0.0, alpha(color, 0.30)), (1.0, alpha(color, 0.0))]);
    book.rect(rect, g);
}
