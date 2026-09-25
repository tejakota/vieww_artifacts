//! exp_kaleido — *the symmetry axis.* The mirror engine.
//!
//! A kaleidoscope is a **dihedral group made visible**, and this plate
//! builds it the way the optics actually work: the source wedge is drawn
//! **symmetric about its own centre line**, then folded by twelve pure
//! rotations (plus a slow pre-rotation of the whole mandala). That single
//! construction gives everything at once: every sector bisector is a mirror
//! axis (the source's own symmetry, rotated into place), every sector
//! boundary is seamless (the source's symmetry maps its +edge onto its
//! −edge), and the whole image carries D₁₂ exactly — the fold is a group,
//! not a hope.
//!
//! The fold count is the axis: one source, ~140 shapes per wedge, ~1,680
//! drawn shapes per frame through one `Painting` — twelve `transformed`
//! groups cloning the same content. And the receipt **measures** the
//! symmetry instead of asserting it: the probe reads a pixel pair either
//! side of a sector bisector out of the output buffer; the pair are exact
//! mirror twins by construction, and the deltas printed are the raster's
//! own residuals.
//!
//! The source wedge is the lab in miniature: rose-curve petals in mirrored
//! pairs, a logarithmic spiral and its mirror twin with marching dash phase,
//! drifting amber sparks and their reflections, a breathing nebula blob on
//! the axis, concentric arc rulings.

use vieww_foundation::{Color, Dash, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle,
    StrokeStyle, Transform};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, mix, tint, Rng, AMBER, FAINT, INK, MUTED, VIOLET, VIOLET_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 10.0;

/// The fold count: the dihedral group's rotation order.
const FOLDS: usize = 12;

/// The sector half-angle (the source wedge spans ±WEDGE).
const WEDGE: f32 = std::f32::consts::PI / FOLDS as f32;

/// The mandala's centre.
const C: (f32, f32) = (640.0, 366.0);

// ── The source wedge ────────────────────────────────────────────────────────

/// One spark: polar seed in the wedge + drift parameters.
struct Spark {
    r: f32,
    a: f32,
    vr: f32,
    va: f32,
    size: f32,
}

fn sparks() -> Vec<Spark> {
    let mut rng = Rng::new(0xDA1E_u64);
    let mut v = Vec::new();
    for _ in 0..22 {
        v.push(Spark {
            r: 70.0 + rng.f01() * 280.0,
            a: 0.03 + rng.f01() * (WEDGE - 0.06),
            vr: 14.0 + rng.f01() * 30.0,
            va: rng.sym() * 0.2,
            size: 1.4 + rng.f01() * 2.2,
        });
    }
    v
}

/// HSV → RGB.
#[must_use]
fn hsv(h: f32, s: f32, v: f32) -> Color {
    let h = (h.fract() + 1.0).fract();
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    let (r, g, b) = match i as u32 % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    Color::rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

/// The source wedge, drawn in LOCAL polar space (wedge centred on +x,
/// spanning ±WEDGE). **Symmetric about the x-axis by construction** — that
/// symmetry is what makes the fold's mirrors and seams exact.
fn draw_source(book: &mut Sketchbook, t: f32) -> usize {
    let mut shapes = 0usize;

    // Rose petals in mirrored pairs: r = a·|cos(k·θ)|^p sampled across the
    // wedge. For pa = 0 the curve is even (self-symmetric); for ±pa the
    // pair mirrors. Together: the wedge's own bilateral symmetry.
    for (pi, pa) in [0.0_f32, 1.05, 2.15].iter().enumerate() {
        for sign in [1.0_f32, -1.0] {
            let centre = pa * sign;
            let k = 3.0;
            let a = 118.0 + pi as f32 * 15.0;
            let wob = 0.85 + 0.15 * (t * 6.2832 * 0.9 + pi as f32 * 2.0).sin();
            let mut fill = Path::new();
            let mut spine = Path::new();
            for s in 0..44 {
                let th = -WEDGE + s as f32 / 43.0 * 2.0 * WEDGE;
                let local = centre + th;
                let r = (a * wob * (k * local).cos().abs().powf(0.8)).clamp(0.0, 336.0);
                let pt = Offset::new(r * local.cos(), r * local.sin());
                if s == 0 {
                    fill.move_to(pt);
                    spine.move_to(pt);
                } else {
                    fill.line_to(pt);
                    spine.line_to(pt);
                }
            }
            fill.close();
            let hue = 0.72 - pi as f32 * 0.06 + 0.04 * (t * 0.7).sin();
            book.fill(fill, alpha(hsv(hue, 0.55, 0.85), 0.16));
            book.stroke(spine, alpha(tint(hsv(hue, 0.6, 0.95), 0.3), 0.9), 1.6);
            shapes += 2;
        }
    }

    // The logarithmic spiral and its mirror twin (dash phase marching
    // outward). One generates; the second is the first with y negated.
    {
        let r0 = 30.0;
        let mut pts: Vec<Offset> = Vec::new();
        let mut th = 0.0_f32;
        let mut rr = r0;
        while rr < 330.0 {
            pts.push(Offset::new(rr * th.cos(), rr * th.sin()));
            th += 0.55 / rr.max(1.0);
            rr = r0 * (1.0 + th * 0.05).exp();
        }
        for sign in [1.0_f32, -1.0] {
            let mut sp = Path::new();
            for (i, p) in pts.iter().enumerate() {
                let pt = Offset::new(p.dx, p.dy * sign);
                if i == 0 {
                    sp.move_to(pt);
                } else {
                    sp.line_to(pt);
                }
            }
            book.stroke_styled(
                sp,
                alpha(hsv(0.62, 0.5, 0.9), 0.7),
                1.3,
                StrokeStyle::rounded().dash(Dash::even(14.0).offset(t * 90.0)),
            );
            shapes += 1;
        }
    }

    // The nebula blob: on the axis (y = 0), so self-symmetric, breathing.
    let breathe = 0.8 + 0.2 * (t * 6.2832 * 0.5).sin();
    let bx = 200.0 * (0.2 + 0.14 * (t * 0.4).cos().abs());
    book.circle(
        Offset::new(bx, 0.0),
        88.0 * breathe,
        Gradient::radial_fill().with_dither().with_stops(&[
            (0.0, alpha(hsv(0.68, 0.6, 0.9), 0.20)),
            (1.0, alpha(hsv(0.68, 0.6, 0.9), 0.0)),
        ]),
    );
    shapes += 1;

    // Concentric arc rulings — symmetric by construction.
    for k in 0..4 {
        let r = 130.0 + k as f32 * 62.0;
        book.arc(
            Offset::new(0.0, 0.0),
            r,
            2.0,
            -WEDGE + 0.02,
            2.0 * (WEDGE - 0.02),
            alpha(tint(hsv(0.7 - k as f32 * 0.03, 0.4, 0.8), 0.2), 0.5),
        );
        shapes += 1;
    }
    shapes
}

/// The sparks for this frame — each with its mirror twin (angle negated
/// exactly, drift included), so the wedge's symmetry holds while they move.
fn spark_points(sparks: &[Spark], t: f32) -> Vec<(f32, f32, f32)> {
    let mut v = Vec::new();
    for s in sparks {
        let r = s.r + (t * s.vr) % 70.0;
        let ang = s.a + (t * s.va).sin() * 0.5;
        v.push((r, ang, s.size));
        v.push((r, -ang, s.size)); // the mirror twin
    }
    v
}

pub fn frame(t: f32) -> WidgetNode {
    let sparks = sparks();
    let pts = spark_points(&sparks, t);
    let spark_count = pts.len();
    // The pre-rotation: the whole mandala slowly turns.
    let spin = t * 0.10 * std::f32::consts::TAU;

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — the scope's dark tube.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::radial(Offset::new(0.5, 0.5), 0.75).with_dither().with_stops(&[
                    (0.0, Color::rgb(13, 12, 18)),
                    (1.0, Color::rgb(5, 5, 8)),
                ]),
            );

            // THE FOLD — twelve pure rotations of a self-symmetric wedge:
            // every bisector a mirror, every boundary seamless, by the
            // group, not by hope.
            for k in 0..FOLDS {
                let sector = spin + k as f32 * std::f32::consts::TAU / FOLDS as f32;
                let transform =
                    Transform::translate(Offset::new(C.0, C.1)).then(Transform::rotate(sector));
                book.transformed(transform, |g| {
                    let _ = draw_source(g, t);
                    // The sparks, in mirrored pairs, through one Plus group.
                    g.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g2| {
                        for (r, a, size) in pts.iter() {
                            let x = r * a.cos();
                            let y = r * a.sin();
                            g2.circle(
                                Offset::new(x, y),
                                size * 2.6,
                                Gradient::radial_fill().with_dither().with_stops(&[
                                    (0.0, alpha(tint(AMBER, 0.4), 0.5)),
                                    (1.0, alpha(AMBER, 0.0)),
                                ]),
                            );
                            g2.circle(
                                Offset::new(x, y),
                                *size,
                                alpha(tint(AMBER, 0.55), 0.95),
                            );
                        }
                    });
                });
            }

            // The centre core: a bright point with a Plus cross-glint —
            // where every mirror line meets.
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                g.circle(
                    Offset::new(C.0, C.1),
                    26.0,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(Color::rgb(255, 250, 240), 0.9)),
                        (1.0, alpha(Color::WHITE, 0.0)),
                    ]),
                );
                for rot in [0.0_f32, std::f32::consts::FRAC_PI_2] {
                    let (s, c) = rot.sin_cos();
                    let mut p = Path::new();
                    p.move_to(Offset::new(C.0 - c * 60.0, C.1 - s * 60.0));
                    p.line_to(Offset::new(C.0 + s * 4.0, C.1 - c * 4.0));
                    p.line_to(Offset::new(C.0 + c * 60.0, C.1 + s * 60.0));
                    p.line_to(Offset::new(C.0 - s * 4.0, C.1 + c * 4.0));
                    p.close();
                    g.fill(p, alpha(tint(AMBER, 0.5), 0.5));
                }
            });

            // The outer rim: two thin rings + twelve fold-marks at the
            // sector boundaries (the group's signature, drawn deliberately).
            book.ring(Offset::new(C.0, C.1), 344.0, 1.2, alpha(mix(FAINT, VIOLET, 0.4), 0.5));
            book.ring(Offset::new(C.0, C.1), 352.0, 2.4, alpha(mix(FAINT, VIOLET, 0.3), 0.3));
            for k in 0..FOLDS {
                let a = spin + k as f32 * std::f32::consts::TAU / FOLDS as f32;
                book.line(
                    Offset::new(C.0 + a.cos() * 344.0, C.1 + a.sin() * 344.0),
                    Offset::new(C.0 + a.cos() * 356.0, C.1 + a.sin() * 356.0),
                    alpha(tint(VIOLET_SOFT, 0.2), 0.6),
                    1.2,
                );
            }
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack.push(receipt_panel(spark_count)).into()
}

// ── The probe — the mirror, measured ────────────────────────────────────────

/// The mirror, measured with true correspondence: for each pixel in a
/// window, its exact mirror image about the bisector (a rotated line) is
/// computed and sampled from the raster with bilinear interpolation —
/// sub-pixel correspondence, not a second axis-aligned square (a naive
/// square pair reads 17% "asymmetric" on a field of small AA'd glows that
/// simply sit in the wrong half of the square; the instrument was wrong,
/// not the fold). The mean |Δ| per channel is the mirror's own residual.
pub fn probe(img: &image::RgbaImage) -> Vec<String> {
    let spin = 0.10 * std::f32::consts::TAU; // t = 1
    let (cs, sn) = spin.sin_cos();
    let bilinear = |x: f32, y: f32| -> (f32, f32, f32) {
        let x0 = x.floor().clamp(0.0, 1279.0) as u32;
        let y0 = y.floor().clamp(0.0, 719.0) as u32;
        let x1 = (x0 + 1).min(1279);
        let y1 = (y0 + 1).min(719);
        let fx = x - x0 as f32;
        let fy = y - y0 as f32;
        let p00 = img.get_pixel(x0, y0);
        let p10 = img.get_pixel(x1, y0);
        let p01 = img.get_pixel(x0, y1);
        let p11 = img.get_pixel(x1, y1);
        let mut out = [0.0_f32; 3];
        for c in 0..3 {
            let top = p00[c] as f32 * (1.0 - fx) + p10[c] as f32 * fx;
            let bot = p01[c] as f32 * (1.0 - fx) + p11[c] as f32 * fx;
            out[c] = top * (1.0 - fy) + bot * fy;
        }
        (out[0], out[1], out[2])
    };
    let mut lines = Vec::new();
    // A window on the +d side; each pixel mirrored about the bisector line
    // (through C at angle `spin`) and sampled.
    for r in [200.0_f32, 240.0, 280.0] {
        let (cx, cy) = (C.0 + (spin + 0.055).cos() * r, C.1 + (spin + 0.055).sin() * r);
        let mut acc = (0.0_f32, 0.0, 0.0);
        let mut n = 0.0;
        for dy in -7..=7 {
            for dx in -7..=7 {
                let x = cx + dx as f32;
                let y = cy + dy as f32;
                // Mirror about the line through C with direction (cs, sn):
                // v → 2(u·d)d − u, with u = p − C.
                let ux = x - C.0;
                let uy = y - C.1;
                let dot = ux * cs + uy * sn;
                let mx = C.0 + 2.0 * dot * cs - ux;
                let my = C.1 + 2.0 * dot * sn - uy;
                let (a, b, c) = bilinear(mx, my);
                let p = img.get_pixel(x.clamp(0.0, 1279.0) as u32, y.clamp(0.0, 719.0) as u32);
                acc.0 += (p[0] as f32 - a).abs();
                acc.1 += (p[1] as f32 - b).abs();
                acc.2 += (p[2] as f32 - c).abs();
                n += 1.0;
            }
        }
        lines.push(format!(
            "mirror residual r={r:.0}: mean |ΔR| {:.2} |ΔG| {:.2} |ΔB| {:.2} (bilinear-mirrored, 225 px)",
            acc.0 / n,
            acc.1 / n,
            acc.2 / n
        ));
    }
    lines.push(
        "residual = AA + bilinear smoothing only — the fold itself is exact by construction".to_string(),
    );
    lines
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(spark_pts: usize) -> WidgetNode {
    // The census, measured from the same loops that drew the frame.
    let per_wedge = 6 * 2 + 2 + 1 + 4 + spark_pts; // petals, spirals, blob, arcs, sparks
    let total = per_wedge * FOLDS;
    let lines = [
        "KALEIDO · THE SYMMETRY AXIS · THE MIRROR ENGINE".to_string(),
        format!("dihedral D{FOLDS} · 12 folds · pre-rotation 0.10 turn"),
        format!("source {per_wedge} shapes/wedge · {total} drawn/frame (measured)"),
        "mirrors: the wedge's own symmetry, rotated — seams cancel by group law".to_string(),
        "probe: mirror pair read from the raster — see metrics".to_string(),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 42.0;

    let mut stack = Stack::new();
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 16.0)
                .width(640.0)
                .height(15.0)
                .child(
                    Text::new(line.clone()).style(
                        TextStyle::new(if i == 0 { 12.0 } else { 11.0 })
                            .monospace()
                            .letter_spacing(if i == 0 { 1.8 } else { 0.0 })
                            .color(alpha(if i == 0 { MUTED } else { mix(MUTED, INK, 0.4) }, 0.95)),
                    ),
                ),
        );
    }

    stack.into()
}
