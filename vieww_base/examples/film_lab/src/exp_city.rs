//! exp_city — *ten thousand windows.*
//!
//! A procedural night city, one hash function deep: a downtown of towers
//! on a street grid, every tower's height from three octaves of hashed
//! value noise, every window's light from its own coordinate hash — so
//! the lit fraction is measured, not chosen, and the flicker re-renders to
//! the byte. The tolerance axis is **3D quad density**: every tower face
//! and every lit window is a projected, depth-sorted, gradient-free quad
//! — the whole city renders as plain `Path` fills through the framework's
//! own raster.
//!
//! - **BLACKOUT** (t 0–0.12): the city in the dark, streets only.
//! - **THE WAVE** (t 0.10–0.34): the lights arrive from the centre out —
//!   each window flips on at its own radius threshold, flickering once.
//! - **DUSK** (t 0.3–0.6): the full glow; traffic begins on the grid.
//! - **NIGHT** (t 0.6–1.0): steady shimmer, the camera drifting; the moon
//!   rides above the haze; the lit fraction settles — and is printed.
//!
//! Receipts: towers, windows, windows lit (counted this frame), traffic.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith};

use crate::film_lib::{
    alpha, clamp01, ease_in_out, ease_out_cubic, mix, tint, FAINT, MUTED, Rng, VIOLET,
    CYAN, CYAN_SOFT, AMBER, BG_DEEP,
};
use crate::three_d::{Camera, Vec3};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 9.0;

/// City grid: GRID × GRID blocks.
const GRID: usize = 13;

/// Block pitch (block + street), world units.
const PITCH: f32 = 5.0;

/// Hash a grid coordinate to a deterministic noise value.
fn vnoise(x: i32, y: i32, seed: u32) -> f32 {
    let h = (x as u32)
        .wrapping_mul(0x27D4EB2D)
         ^ ((y as u32).wrapping_mul(0x165667B1))
         ^ (seed.wrapping_mul(0x9E3779B1))
        .rotate_left(13);
    (h >> 8) as f32 / ((1u32 << 24) as f32)
}

/// Smoothed value noise by bilinear interpolation of the hashed lattice.
fn smooth_noise(x: f32, y: f32, seed: u32) -> f32 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sy = fy * fy * (3.0 - 2.0 * fy);
    let n00 = vnoise(x0, y0, seed);
    let n10 = vnoise(x0 + 1, y0, seed);
    let n01 = vnoise(x0, y0 + 1, seed);
    let n11 = vnoise(x0 + 1, y0 + 1, seed);
    let a = n00 + (n10 - n00) * sx;
    let b = n01 + (n11 - n01) * sx;
    a + (b - a) * sy
}

/// Tower height at block (bx, bz) — 3 octaves, downtown-weighted.
fn tower_height(bx: i32, bz: i32) -> f32 {
    let fx = bx as f32 * 0.33;
    let fz = bz as f32 * 0.33;
    let mut h = smooth_noise(fx, fz, 0xC17C) * 0.5
        + smooth_noise(fx * 2.1, fz * 2.1, 0x77) * 0.3
        + smooth_noise(fx * 4.3, fz * 4.3, 0x3E) * 0.2;
    // The downtown falloff: taller near the centre.
    let cx = (bx - GRID as i32 / 2) as f32 / GRID as f32;
    let cz = (bz - GRID as i32 / 2) as f32 / GRID as f32;
    let dist = (cx * cx + cz * cz).sqrt();
    h *= 1.0 - dist * 0.75;
    1.2 + h * 16.0
}

/// One window's lit state — pure hash of its whole address.
/// Returns (lit, warm) where warm picks the colour cast.
fn window_lit(bx: i32, bz: i32, face: u8, floor: i32, col: i32, t: f32, wave: f32) -> (bool, f32) {
    // The arrival wave: windows inside `wave` radius may be on.
    let cx = (bx - GRID as i32 / 2) as f32;
    let cz = (bz - GRID as i32 / 2) as f32;
    let dist = (cx * cx + cz * cz).sqrt() / (GRID as f32 * 0.5);
    if dist > wave {
        return (false, 0.0);
    }
    let h = (bx as u32)
        .wrapping_mul(0x27D4EB2D)
         ^ ((bz as u32).wrapping_mul(0x165667B1))
         ^ ((face as u32).wrapping_mul(0x9E3779B1))
         ^ ((floor as u32).wrapping_mul(0x85EBCA6B))
         ^ ((col as u32).wrapping_mul(0xC2B2AE35))
        .rotate_left(11);
    let on_bias = (h >> 9) as f32 / ((1u32 << 23) as f32); // 0..1
    // Flicker: a slow square-ish wobble per window.
    let phase = ((h >> 3) as f32) * 0.001;
    let wobble = (t * 0.55 + phase).sin();
    let lit = on_bias > 0.42 && wobble > -0.35;
    let warm = ((h >> 16) as f32) / ((1u32 << 16) as f32);
    (lit, warm)
}

// ── The scene ───────────────────────────────────────────────────────────────

fn scene(book: &mut Sketchbook, canvas: Size, t: f32) {
    let w = canvas.width;
    let h = canvas.height;

    // Stage gates.
    let wave = clamp01((t - 0.10) / 0.24);
    let wave_r = wave * 1.15;
    let traffic_t = clamp01((t - 0.30) / 0.30);

    // The night sky.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::vertical()
            .with_dither()
            .with_stops(&[
                (0.0, Color::rgb(5, 6, 11)),
                (0.5, Color::rgb(10, 10, 18)),
                (1.0, Color::rgb(16, 13, 22)),
            ]),
    );

    // Stars.
    {
        let mut rng = Rng::new(0xC17C);
        for _ in 0..110 {
            let x = rng.f01() * w;
            let y = rng.f01() * h * 0.5;
            let tw = 0.5 + 0.5 * (t * 1.9 + rng.f01() * 7.0).sin();
            book.circle(Offset::new(x, y), 0.4 + rng.f01() * 0.7, alpha(Color::WHITE, 0.02 + 0.08 * tw));
        }
    }

    // The moon + its halo.
    {
        let mx = w * 0.78;
        let my = h * 0.18;
        book.layer(1.0, 22.0, None, |g| {
            g.circle(
                Offset::new(mx, my),
                64.0,
                Gradient::radial_fill().with_stops(&[
                    (0.0, alpha(mix(VIOLET, Color::WHITE, 0.5), 0.14)),
                    (1.0, alpha(VIOLET, 0.0)),
                ]),
            );
        });
        book.circle(
            Offset::new(mx, my),
            17.0,
            Gradient::radial_fill().with_stops(&[
                (0.0, tint(Color::WHITE, 0.0)),
                (0.8, Color::rgb(216, 214, 226)),
                (1.0, Color::rgb(160, 158, 178)),
            ]),
        );
        // A crescent bite.
        book.circle(
            Offset::new(mx + 7.0, my - 4.0),
            14.5,
            Color::rgb(10, 10, 18),
        );
    }

    // The camera: elevated 3/4, a slow drift.
    let az = 0.5 + (t * 0.10).sin() * 0.22;
    let cam = Camera {
        eye: Vec3::new(az.sin() * 54.0, 36.0, az.cos() * 54.0),
        target: Vec3::new(0.0, 1.0, 0.0),
        fov: 0.88,
    };

    // The haze glow of the whole city — under everything, over the sky.
    book.layer(1.0, 30.0, None, |g| {
        g.circle(
            Offset::new(w * 0.5, h * 0.66),
            w * 0.42,
            Gradient::radial_fill().with_stops(&[
                (0.0, alpha(mix(AMBER, VIOLET, 0.55), 0.05 + 0.09 * wave)),
                (1.0, alpha(VIOLET, 0.0)),
            ]),
        );
    });

    // ── The city: faces + windows, one draw list, one painter's sort ──────
    struct Face {
        pts: [Offset; 4],
        depth: f32,
        color: Color,
    }
    let mut faces: Vec<Face> = Vec::with_capacity(14_000);
    let mut lit_count = 0usize;
    let mut total_windows = 0usize;

    let half = GRID as i32 / 2;
    for bz in -half..=half {
        for bx in -half..=half {
            let x = bx as f32 * PITCH;
            let z = bz as f32 * PITCH;
            // Parks: one block in five, away from the centre.
            let park = vnoise(bx.wrapping_mul(31), bz.wrapping_mul(17), 0x9A2) < 0.16
                && (bx.abs() > 2 || bz.abs() > 2);
            let height = tower_height(bx, bz);
            let half_w = 1.55;

            // Tower body: the four walls + roof.
            let corners = [
                Vec3::new(x - half_w, 0.0, z - half_w),
                Vec3::new(x + half_w, 0.0, z - half_w),
                Vec3::new(x + half_w, 0.0, z + half_w),
                Vec3::new(x - half_w, 0.0, z + half_w),
            ];
            let top: Vec<Vec3> = corners.iter().map(|c| Vec3::new(c.x, height, c.z)).collect();

            // Face orientations with their shade tints.
            for (fi, (a, b)) in [
                (0usize, 1usize),
                (1, 2),
                (2, 3),
                (3, 0),
            ]
            .iter()
            .enumerate()
            {
                let _ = fi;
                let (ca, cb) = (corners[*a], corners[*b]);
                let (ta, tb) = (top[*a], top[*b]);
                let quad = [ca, cb, tb, ta];
                let Some((_, depth, _)) = project4(&quad, &cam, canvas) else {
                    continue;
                };
                // Back-face cull in screen space.
                let pts = match project4(&quad, &cam, canvas) {
                    Some((pts, _, _)) => pts,
                    None => continue,
                };
                let area = cross_area(&pts);
                if area >= 0.0 {
                    continue;
                }
                // Face shade by orientation + a per-tower tint.
                let tint_k = vnoise(bx.wrapping_mul(7), bz.wrapping_mul(13), 0x51);
                let base = if park {
                    Color::rgb(9, 14, 11)
                } else {
                    mix(Color::rgb(13, 14, 22), Color::rgb(22, 20, 32), tint_k)
                };
                let shade = match fi % 4 {
                    0 => 0.55,
                    1 => 1.0,
                    2 => 0.75,
                    _ => 0.38,
                };
                let col = mix(Color::rgb(4, 4, 7), base, shade);
                faces.push(Face { pts, depth, color: col });

                // Windows on this wall.
                if park {
                    continue;
                }
                let floors = ((height - 0.8) / 0.62).max(1.0) as i32;
                let cols = 5;
                // Window extents: a proper face-aligned patch, in the wall's
                // own (u, v) parameter space.
                let su = 0.30 / cols as f32; // half-width, wall units
                let sv = 0.26 / floors as f32; // half-height, wall units
                for fl in 0..floors {
                    for cl in 0..cols {
                        total_windows += 1;
                        let u = (cl as f32 + 0.5) / cols as f32;
                        let v = (fl as f32 + 0.75) / floors as f32;
                        // The window's own quad: bottom-left → bottom-right
                        // → top-right → top-left, on the wall's lattice.
                        let bl = lerp3(lerp3(ca, cb, u - su), lerp3(ta, tb, u - su), v - sv);
                        let br = lerp3(lerp3(ca, cb, u + su), lerp3(ta, tb, u + su), v - sv);
                        let tr = lerp3(lerp3(ca, cb, u + su), lerp3(ta, tb, u + su), v + sv);
                        let tl = lerp3(lerp3(ca, cb, u - su), lerp3(ta, tb, u - su), v + sv);
                        let wq = [bl, br, tr, tl];
                        let Some((wpts, wdepth, _)) = project4(&wq, &cam, canvas) else {
                            continue;
                        };
                        if cross_area(&wpts) >= 0.0 {
                            continue;
                        }
                        let (lit, warm) = window_lit(bx, bz, fi as u8, fl, cl, t, wave_r);
                        if lit {
                            lit_count += 1;
                            let col = if warm < 0.18 {
                                mix(CYAN_SOFT, Color::WHITE, 0.35)
                            } else {
                                mix(AMBER, Color::WHITE, warm * 0.45)
                            };
                            faces.push(Face {
                                pts: wpts,
                                depth: wdepth - 0.05,
                                color: alpha(col, 0.65 + warm * 0.3),
                            });
                        }
                    }
                }
            }

            // The roof.
            let top_arr = [top[0], top[1], top[2], top[3]];
            if let Some((pts, depth, _)) = project4(&top_arr, &cam, canvas) {
                if cross_area(&pts) < 0.0 {
                    faces.push(Face {
                        pts,
                        depth,
                        color: mix(Color::rgb(18, 18, 26), Color::rgb(30, 28, 40), 0.5),
                    });
                }
            }
        }
    }

    // Painter's sort, far first.
    faces.sort_by(|a, b| b.depth.partial_cmp(&a.depth).unwrap_or(std::cmp::Ordering::Equal));
    for f in &faces {
        let mut path = Path::new();
        path.move_to(f.pts[0]);
        for p in &f.pts[1..] {
            path.line_to(*p);
        }
        path.close();
        book.fill(path, f.color);
    }

    // ── The streets: faint amber grid lines + traffic ─────────────────────
    book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
        let half_f = half as f32 * PITCH;
        for i in -half..=half {
            let a = i as f32 * PITCH;
            let p1 = Vec3::new(a, 0.02, -half_f);
            let p2 = Vec3::new(a, 0.02, half_f);
            let q1 = Vec3::new(-half_f, 0.02, a);
            let q2 = Vec3::new(half_f, 0.02, a);
            for (s, e) in [(p1, p2), (q1, q2)] {
                if let (Some((pa, _, _)), Some((pb, _, _))) =
                    (cam.project(s, canvas), cam.project(e, canvas))
                {
                    g.line(pa, pb, alpha(AMBER, 0.05 + 0.06 * wave), 1.0);
                }
            }
        }
        // Traffic: dots riding the streets, closed form.
        if traffic_t > 0.0 {
            let mut rng = Rng::new(0x7A1);
            for i in 0..72 {
                let axis_x = rng.f01() < 0.5;
                let line = ((rng.f01() * GRID as f32).round() - half as f32) * PITCH;
                let speed = (0.55 + rng.f01() * 0.9) * if rng.f01() < 0.5 { 1.0 } else { -1.0 };
                let span = half as f32 * PITCH * 2.0;
                let pos = ((t * speed * 8.0 + rng.f01() * span) % span + span) % span - span * 0.5;
                let p = if axis_x {
                    Vec3::new(pos, 0.06, line)
                } else {
                    Vec3::new(line, 0.06, pos)
                };
                if let Some((pp, _, sc)) = cam.project(p, canvas) {
                    let r = (0.9 * sc * 900.0).max(0.7);
                    let head = speed > 0.0;
                    let col = if head { tint(AMBER, 0.5) } else { mix(REDISH, Color::WHITE, 0.3) };
                    g.circle(pp, r, alpha(col, 0.85 * traffic_t));
                }
            }
        }
    });

    // The vignette.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::radial(Offset::new(0.5, 0.52), 0.85)
            .with_dither()
            .with_stops(&[
                (0.6, alpha(Color::BLACK, 0.0)),
                (1.0, alpha(Color::BLACK, 0.55)),
            ]),
    );

}

/// Tail-light red.
const REDISH: Color = Color::rgb(255, 90, 71);

/// Lerp between two 3D points.
fn lerp3(a: Vec3, b: Vec3, k: f32) -> Vec3 {
    a.lerp(b, k)
}

/// Project a 4-point quad; `None` if any corner falls behind the camera.
fn project4(
    quad: &[Vec3; 4],
    cam: &Camera,
    canvas: Size,
) -> Option<([Offset; 4], f32, f32)> {
    let mut pts = [Offset::new(0.0, 0.0); 4];
    let mut depth = 0.0;
    let mut scale = 0.0;
    for (k, p) in quad.iter().enumerate() {
        let (pt, d, s) = cam.project(*p, canvas)?;
        pts[k] = pt;
        depth += d;
        scale += s;
    }
    Some((pts, depth / 4.0, scale / 4.0))
}

/// Signed area of the screen quad — the back-face test.
fn cross_area(pts: &[Offset; 4]) -> f32 {
    let ax = pts[1].dx - pts[0].dx;
    let ay = pts[1].dy - pts[0].dy;
    let bx = pts[2].dx - pts[1].dx;
    let by = pts[2].dy - pts[1].dy;
    ax * by - ay * bx
}

/// The frame.
pub fn frame(t: f32) -> WidgetNode {
    let paint = Painting::sized(
        crate::film_lib::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            scene(book, size, t);
        }),
    );

    // The counts, recomputed pure — the same hash the painter ran.
    let wave = clamp01((t - 0.10) / 0.24);
    let wave_r = wave * 1.15;
    let mut total_windows = 0usize;
    let mut lit = 0usize;
    let half = GRID as i32 / 2;
    for bz in -half..=half {
        for bx in -half..=half {
            let height = tower_height(bx, bz);
            let floors = ((height - 0.8) / 0.62).max(1.0) as i32;
            for fi in 0..4usize {
                for fl in 0..floors {
                    for cl in 0..5 {
                        total_windows += 1;
                        if window_lit(bx, bz, fi as u8, fl, cl, t, wave_r).0 {
                            lit += 1;
                        }
                    }
                }
            }
        }
    }

    let strip = Stack::new()
        .push(
            Positioned::new()
                .left(24.0)
                .top(20.0)
                .width(520.0)
                .height(18.0)
                .child(
                    Text::new("THE CITY · ten thousand windows").style(
                        TextStyle::new(12.0)
                            .monospace()
                            .letter_spacing(2.2)
                            .color(alpha(FAINT, 0.95)),
                    ),
                ),
        )
        .push(
            Positioned::new()
                .left(24.0)
                .top(40.0)
                .width(720.0)
                .height(18.0)
                .child(
                    Text::new(format!(
                        "{} windows · {} lit ({:>4.1}%) · hash-addressed, byte-identical",
                        total_windows,
                        lit,
                        lit as f32 / total_windows.max(1) as f32 * 100.0
                    ))
                    .style(TextStyle::new(11.0).monospace().color(alpha(MUTED, 0.85))),
                ),
        );

    Stack::new()
        .push(Positioned::fill().child(paint))
        .push(strip)
        .into()
}
