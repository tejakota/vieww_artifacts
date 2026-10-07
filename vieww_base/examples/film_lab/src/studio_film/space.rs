//! space — true perspective for flat drawings, without tearing.
//!
//! The v4 cut tilted its cards with `panel_3d`, which approximates a
//! perspective quad with **four affine quadrants**. Each quadrant is right
//! at its own corners and wrong everywhere else, so a straight line that
//! crossed the middle of a card came out kinked — the "broken in the
//! centre" the review saw on the device slabs and the layer planes.
//!
//! This module projects *geometry*, not a transform: every point of a
//! shape drawn on a [`Plane`] goes through the camera, and the shape is
//! filled as the polygon those points make. A straight edge stays straight
//! because both of its ends are projected exactly, and there is no seam to
//! show because there is only one polygon. Type is never projected — it
//! would have to be re-shaped per frame — so labels sit flat beside the
//! objects they name, placed from the same projection.

use vieww_foundation::{Color, Offset, Path, Rect, Size, Sketchbook};

use crate::product_film as pf;
use crate::three_d::{Camera, Vec3};

/// A camera and where its picture is centred in the scene.
#[derive(Clone, Copy)]
pub struct View {
    pub cam: Camera,
    /// The canvas the camera projects into (its aspect).
    pub canvas: Size,
    /// Where the projection's centre lands in scene coordinates.
    pub centre: Offset,
}

impl View {
    /// A world point in scene coordinates, with its depth and scale.
    pub fn project(&self, p: Vec3) -> Option<(Offset, f32, f32)> {
        let (o, depth, k) = self.cam.project(p, self.canvas)?;
        Some((
            Offset::new(
                o.dx + self.centre.dx - self.canvas.width * 0.5,
                o.dy + self.centre.dy - self.canvas.height * 0.5,
            ),
            depth,
            k,
        ))
    }

    pub fn pt(&self, p: Vec3) -> Option<Offset> {
        self.project(p).map(|(o, _, _)| o)
    }
}

/// A flat surface in the world: `origin + u·x + v·y` for plane coordinates
/// `(x, y)`. `u` and `v` are unit-length and carry the plane's orientation,
/// so a drawing made in plane coordinates is in world units.
#[derive(Clone, Copy)]
pub struct Plane {
    pub origin: Vec3,
    pub u: Vec3,
    pub v: Vec3,
}

impl Plane {
    /// A horizontal plane (a floor) at height `y`, centred on `(x, z)`,
    /// turned by `yaw` about the vertical. Plane `x` runs right, plane `y`
    /// runs *away* from the viewer — so a layout drawn top-to-bottom on
    /// paper lies near-to-far on the floor.
    pub fn floor(centre: Vec3, yaw: f32) -> Plane {
        let (s, c) = yaw.sin_cos();
        Plane {
            origin: centre,
            u: Vec3::new(c, 0.0, s),
            v: Vec3::new(s, 0.0, -c).scale(-1.0),
        }
    }

    /// An upright plane (a card standing up) centred at `centre`, facing
    /// the viewer, turned by `yaw` and tipped back by `pitch`.
    pub fn card(centre: Vec3, yaw: f32, pitch: f32) -> Plane {
        let u = Vec3::new(1.0, 0.0, 0.0).rot_y(yaw);
        let v = Vec3::new(0.0, -1.0, 0.0).rot_x(pitch).rot_y(yaw);
        Plane { origin: centre, u, v }
    }

    pub fn at(&self, x: f32, y: f32) -> Vec3 {
        self.origin.add(self.u.scale(x)).add(self.v.scale(y))
    }
}

/// A polygon on a plane, projected and filled.
pub fn fill(book: &mut Sketchbook, view: &View, plane: &Plane, pts: &[(f32, f32)], color: Color) {
    if let Some(path) = path_of(view, plane, pts, true) {
        book.fill(path, color);
    }
}

/// A polyline (or closed outline) on a plane, projected and stroked.
pub fn stroke(book: &mut Sketchbook, view: &View, plane: &Plane, pts: &[(f32, f32)], closed: bool, color: Color, width: f32) {
    if let Some(path) = path_of(view, plane, pts, closed) {
        book.stroke(path, color, width);
    }
}

fn path_of(view: &View, plane: &Plane, pts: &[(f32, f32)], closed: bool) -> Option<Path> {
    let mut path = Path::new();
    for (i, (x, y)) in pts.iter().enumerate() {
        let p = view.pt(plane.at(*x, *y))?;
        if i == 0 {
            path.move_to(p);
        } else {
            path.line_to(p);
        }
    }
    if closed {
        path.close();
    }
    Some(path)
}

/// The outline of a rounded rectangle in plane coordinates, as points.
pub fn rrect_pts(r: Rect, radius: f32) -> Vec<(f32, f32)> {
    let rad = radius.min(r.width() * 0.5).min(r.height() * 0.5).max(0.0);
    let mut out = Vec::with_capacity(40);
    let corners = [
        (r.right - rad, r.top + rad, -std::f32::consts::FRAC_PI_2),
        (r.right - rad, r.bottom - rad, 0.0),
        (r.left + rad, r.bottom - rad, std::f32::consts::FRAC_PI_2),
        (r.left + rad, r.top + rad, std::f32::consts::PI),
    ];
    for (cx, cy, a0) in corners {
        for k in 0..=8 {
            let a = a0 + k as f32 / 8.0 * std::f32::consts::FRAC_PI_2;
            out.push((cx + rad * a.cos(), cy + rad * a.sin()));
        }
    }
    out
}

/// A filled rounded rectangle on a plane.
pub fn rrect(book: &mut Sketchbook, view: &View, plane: &Plane, r: Rect, radius: f32, color: Color) {
    fill(book, view, plane, &rrect_pts(r, radius), color);
}

/// A rounded rectangle's outline on a plane.
pub fn rrect_stroke(book: &mut Sketchbook, view: &View, plane: &Plane, r: Rect, radius: f32, color: Color, width: f32) {
    stroke(book, view, plane, &rrect_pts(r, radius), true, color, width);
}

/// A disc on a plane (a projected circle — an ellipse on screen).
pub fn disc(book: &mut Sketchbook, view: &View, plane: &Plane, c: (f32, f32), r: f32, color: Color) {
    let pts: Vec<(f32, f32)> = (0..24)
        .map(|k| {
            let a = k as f32 / 24.0 * std::f32::consts::TAU;
            (c.0 + r * a.cos(), c.1 + r * a.sin())
        })
        .collect();
    fill(book, view, plane, &pts, color);
}

/// A point of light in space, sized by perspective: a solid core with a
/// crisp ring — drawn, not blurred. The soft halo this once had is gone
/// with the film's radial glows (see `mod.rs`).
pub fn glow_point(book: &mut Sketchbook, view: &View, p: Vec3, r: f32, color: Color, a: f32) {
    if let Some((o, _, k)) = view.project(p) {
        let s = (k * 900.0).clamp(0.3, 3.0);
        let a = a.clamp(0.0, 1.0);
        book.ring(o, r * 1.7 * s, 1.1, pf::alpha(color, 0.55 * a));
        book.circle(o, r * s, pf::alpha(Color::WHITE, 0.95 * a));
    }
}
