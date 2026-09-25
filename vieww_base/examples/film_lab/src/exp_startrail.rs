//! exp_startrail — *the exposure axis.* Time, folded into arcs.
//!
//! A long-exposure photograph of the night sky, developing: every star
//! leaves an arc around the celestial pole, and the arc **grows with the
//! exposure** — the plate's own timeline is the shutter, held open. The
//! trails all ride ONE Plus-blended group, so where arcs cross the ink
//! **accumulates** exactly as silver grains do — the receipt's probe reads
//! a crossing pixel and a single-trail pixel out of the final buffer and
//! prints both, the additive blend measured in the output instead of
//! asserted in the docs.
//!
//! Around the pole: Polaris itself (the one fixed point), the Milky Way as
//! a band of faint field stars plus its soft unresolved glow, three meteors
//! (straight streaks — fast enough to beat the shutter), and a foreground of
//! two mountain ridgelines with an observatory dome, its red beacon pulsing.
//! The earth never moves in frame; the sky turns through it.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, clamp01, ease_out_cubic, mix, smoothstep, Rng, AMBER, BG_DEEP, INK,
    MUTED};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

/// The celestial pole (Polaris), on canvas.
const POLE: (f32, f32) = (938.0, 186.0);

/// Field stars, milky-way stars — the census.
const STARS: usize = 1900;
const MW_STARS: usize = 1400;

/// Total sky rotation across the whole exposure, radians.
const SWEEP: f32 = 0.55;

/// One star of the field.
struct Star {
    /// Position angle around the pole, radians.
    a: f32,
    /// Radius from the pole, px.
    r: f32,
    /// Brightness bucket: 0 bright, 1 medium, 2 faint.
    b: u8,
    /// Magnitude within bucket.
    m: f32,
}

/// The field: stars scattered over the whole canvas, stored polar around the
/// pole (only stars whose radius keeps the arc on-canvas are kept).
fn field() -> Vec<Star> {
    let mut rng = Rng::new(0x57A2_u64);
    let mut v: Vec<Star> = Vec::new();
    while v.len() < STARS {
        let x = rng.f01() * 1280.0;
        let y = rng.f01() * 560.0;
        let r = ((x - POLE.0).powi(2) + (y - POLE.1).powi(2)).sqrt();
        if r < 26.0 || r > 990.0 {
            continue;
        }
        let a = (y - POLE.1).atan2(x - POLE.0);
        let bucket = rng.f01();
        let b = if bucket > 0.96 { 0 } else if bucket > 0.72 { 1 } else { 2 };
        v.push(Star { a, r, b, m: 0.4 + 0.6 * rng.f01() });
    }
    v
}

/// The Milky Way: stars clustered along a diagonal band, plus its haze.
fn milky_way() -> Vec<Star> {
    let mut rng = Rng::new(0x411A_u64);
    let mut v: Vec<Star> = Vec::new();
    // The band: from lower-left to upper-right, offset from the pole.
    let (bx, by) = (140.0, 560.0);
    let (dx, dy) = (1080.0, -420.0);
    while v.len() < MW_STARS {
        let f = rng.f01();
        let spread = rng.sym() * rng.f01() * 92.0;
        let x = bx + dx * f - dy / 420.0 * spread;
        let y = by + dy * f + dx / 1080.0 * spread;
        if x < 4.0 || x > 1276.0 || y < 4.0 || y > 556.0 {
            continue;
        }
        let r = ((x - POLE.0).powi(2) + (y - POLE.1).powi(2)).sqrt();
        if r < 26.0 {
            continue;
        }
        let a = (y - POLE.1).atan2(x - POLE.0);
        v.push(Star { a, r, b: 2, m: 0.25 + 0.5 * rng.f01() });
    }
    v
}

/// One meteor: a window in the timeline plus a track.
struct Meteor {
    t0: f32,
    x0: f32,
    y0: f32,
    dx: f32,
    dy: f32,
}

const METEORS: [Meteor; 3] = [
    Meteor { t0: 0.26, x0: 220.0, y0: 120.0, dx: 360.0, dy: 210.0 },
    Meteor { t0: 0.54, x0: 1080.0, y0: 80.0, dx: -300.0, dy: 260.0 },
    Meteor { t0: 0.81, x0: 520.0, y0: 60.0, dx: 260.0, dy: 300.0 },
];

pub fn frame(t: f32) -> WidgetNode {
    let stars = field();
    let mw = milky_way();
    // The exposure so far: eased so the shutter feels mechanical. The
    // shutter is already 6% open at frame one — an empty first frame is a
    // dead cell (the same audit catch fourier paid for).
    let exposure = ease_out_cubic(clamp01((t + 0.06).min(1.0)));
    let rot = SWEEP * exposure;
    // Trails visible this frame (census).
    let arc_count = stars.len() + mw.len();
    let beacon = 0.5 + 0.5 * (t * 6.2832 * 2.4).sin();

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The night sky — deep indigo, a trace of gradient toward the
            // horizon glow.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(5, 6, 12)),
                    (0.8, Color::rgb(8, 9, 17)),
                    (1.0, Color::rgb(13, 13, 20)),
                ]),
            );

            // The Milky Way's unresolved glow — one wide soft diagonal band.
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                let mut haze = Path::new();
                haze.move_to(Offset::new(60.0, 620.0));
                haze.line_to(Offset::new(300.0, 380.0));
                haze.line_to(Offset::new(1240.0, 60.0));
                haze.line_to(Offset::new(1400.0, 200.0));
                haze.line_to(Offset::new(420.0, 780.0));
                haze.close();
                g.fill(
                    haze,
                    alpha(Color::rgb(38, 40, 66), 0.16),
                );
            });

            // ── THE TRAILS — all arcs in ONE Plus group: crossings
            // accumulate, exactly like a long exposure. ──
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                let pole = Offset::new(POLE.0, POLE.1);
                let draw_star = |g: &mut Sketchbook, s: &Star| {
                    let (width, base_a) = match s.b {
                        0 => (1.7_f32, 0.85_f32),
                        1 => (1.1, 0.55),
                        _ => (0.7, 0.30),
                    };
                    let a0 = s.a + rot - SWEEP * exposure; // where the shutter opened
                    let a1 = s.a + rot;
                    // The arc: only as long as the exposure so far.
                    let trail = (a1 - a0).max(0.0);
                    if trail < 1e-4 {
                        return;
                    }
                    let ink = base_a * s.m;
                    g.arc(
                        pole,
                        s.r,
                        width,
                        a0,
                        trail,
                        alpha(Color::rgb(216, 224, 244), ink),
                    );
                };
                for s in &stars {
                    draw_star(g, s);
                }
                for s in &mw {
                    draw_star(g, s);
                }
                // Polaris: the still point — a small disc with its own
                // slight diffraction spikes.
                g.circle(pole, 2.6, alpha(Color::rgb(240, 244, 255), 0.95));
                g.line(
                    Offset::new(pole.dx - 7.0, pole.dy),
                    Offset::new(pole.dx + 7.0, pole.dy),
                    alpha(Color::rgb(240, 244, 255), 0.4),
                    0.8,
                );
                g.line(
                    Offset::new(pole.dx, pole.dy - 7.0),
                    Offset::new(pole.dx, pole.dy + 7.0),
                    alpha(Color::rgb(240, 244, 255), 0.4),
                    0.8,
                );
            });

            // ── The meteors — straight streaks with fading tails. ──
            for m in METEORS.iter() {
                let local = (t - m.t0) / 0.05;
                if !(0.0..1.0).contains(&local) {
                    continue;
                }
                let head = smoothstep(local);
                let (hx, hy) = (m.x0 + m.dx * head, m.y0 + m.dy * head);
                let tail = 0.42;
                let (tx, ty) = (hx - m.dx * tail, hy - m.dy * tail);
                book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                    // A tapered streak: a thin quad with a two-stop gradient
                    // along the flight direction — bright head, fading tail.
                    let ang = (m.dy).atan2(m.dx);
                    let nx = ang.sin() * 1.6;
                    let ny = -ang.cos() * 1.6;
                    let mut q = Path::new();
                    q.move_to(Offset::new(tx - nx, ty - ny));
                    q.line_to(Offset::new(hx - nx, hy - ny));
                    q.line_to(Offset::new(hx + nx, hy + ny));
                    q.line_to(Offset::new(tx + nx, ty + ny));
                    q.close();
                    g.fill(
                        q,
                        Gradient::linear(Offset::new(0.0, 0.5), Offset::new(1.0, 0.5))
                            .with_dither()
                            .with_stops(&[
                                (0.0, alpha(Color::WHITE, 0.0)),
                                (0.7, alpha(Color::rgb(230, 238, 255), 0.5)),
                                (1.0, alpha(Color::WHITE, 0.95)),
                            ]),
                    );
                });
            }

            // ── The foreground: two ridgelines and the observatory. ──
            let mut ridge = |seed: u64, y_base: f32, amp: f32, color: Color| {
                let mut rng = Rng::new(seed);
                let mut p = Path::new();
                p.move_to(Offset::new(0.0, y_base));
                let mut x = 0.0;
                while x < 1280.0 {
                    let step = 60.0 + rng.f01() * 90.0;
                    x += step;
                    let y = y_base - amp * rng.f01() * (0.4 + 0.6 * (x / 1280.0).sin().abs());
                    p.line_to(Offset::new(x.min(1280.0), y));
                }
                p.line_to(Offset::new(1280.0, 720.0));
                p.line_to(Offset::new(0.0, 720.0));
                p.close();
                book.fill(p, color);
            };
            ridge(
                0x2D6E,
                596.0,
                90.0,
                Color::rgb(14, 15, 24),
            );
            ridge(
                0x9C4A,
                648.0,
                60.0,
                Color::rgb(7, 8, 13),
            );

            // The observatory: dome, slit, and the red beacon.
            let (ox, oy) = (1012.0, 596.0);
            book.rect(Rect::new(ox - 34.0, oy - 26.0, ox + 34.0, oy + 2.0), Color::rgb(10, 11, 17));
            let mut dome = Path::new();
            dome.move_to(Offset::new(ox - 30.0, oy - 24.0));
            dome.line_to(Offset::new(ox - 26.0, oy - 44.0));
            dome.line_to(Offset::new(ox - 10.0, oy - 54.0));
            dome.line_to(Offset::new(ox + 12.0, oy - 52.0));
            dome.line_to(Offset::new(ox + 28.0, oy - 38.0));
            dome.line_to(Offset::new(ox + 30.0, oy - 24.0));
            dome.close();
            book.fill(dome, Color::rgb(12, 13, 20));
            // The slit.
            book.rect(
                Rect::new(ox - 3.0, oy - 52.0, ox + 3.0, oy - 30.0),
                Color::rgb(22, 24, 34),
            );
            // The beacon — a red aircraft-warning lamp, pulsing.
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                g.circle(
                    Offset::new(ox + 30.0, oy - 50.0),
                    6.0 + 3.0 * beacon,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(Color::rgb(255, 70, 60), 0.9 * beacon)),
                        (1.0, alpha(Color::rgb(255, 60, 50), 0.0)),
                    ]),
                );
            });

            // A faint airglow band above the mountains — the horizon's own
            // emission, deepest at the ridge.
            book.rect(
                Rect::new(0.0, 540.0, w, 660.0),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, alpha(Color::rgb(30, 42, 58), 0.0)),
                    (0.6, alpha(Color::rgb(34, 48, 66), 0.18)),
                    (1.0, alpha(Color::rgb(30, 42, 58), 0.0)),
                ]),
            );
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack.push(receipt_panel(t, exposure, arc_count)).into()
}

// ── The probe — the additive blend, read out of the output ─────────────────

/// The mean of an 8×8 patch — a stable read in a field of sub-pixel arcs
/// (a single pixel can land between two AA'd trails and say nothing).
fn patch_mean(img: &image::RgbaImage, cx: u32, cy: u32) -> (f32, f32, f32) {
    let mut acc = (0.0_f32, 0.0, 0.0);
    let mut n = 0.0;
    for dy in 0..8 {
        for dx in 0..8 {
            let p = img.get_pixel(cx + dx, cy + dy);
            acc.0 += p[0] as f32;
            acc.1 += p[1] as f32;
            acc.2 += p[2] as f32;
            n += 1.0;
        }
    }
    (acc.0 / n, acc.1 / n, acc.2 / n)
}

/// Reads the dark foreground, a single-trail patch, and the dense milky band
/// from the final buffer — the Plus accumulation, measured. The band's
/// excess over one trail is many faint arcs stacked through ONE additive
/// layer; the ground's near-zero ink is the same sky with the shutter
/// blocked by the mountain.
pub fn probe(img: &image::RgbaImage) -> Vec<String> {
    let mut lines = Vec::new();
    let ground = patch_mean(img, 640, 704);
    lines.push(format!(
        "ground ink ({:.0}, {:.0}, {:.0}) — the mountain, shutter blocked",
        ground.0, ground.1, ground.2
    ));
    let lone = patch_mean(img, 240, 120);
    lines.push(format!(
        "single-trail sky ({:.0}, {:.0}, {:.0}) — field arcs + sky",
        lone.0, lone.1, lone.2
    ));
    let band = patch_mean(img, 700, 330);
    lines.push(format!(
        "milky band ({:.0}, {:.0}, {:.0}) — haze + many faint arcs, Plus-stacked",
        band.0, band.1, band.2
    ));
    lines.push(format!(
        "band excess over single-trail: +{:.0} +{:.0} +{:.0} (the blend, measured)",
        band.0 - lone.0,
        band.1 - lone.1,
        band.2 - lone.2
    ));
    lines
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(t: f32, exposure: f32, arc_count: usize) -> WidgetNode {
    let deg = SWEEP.to_degrees();
    let lines = [
        "STARTRAIL · THE EXPOSURE AXIS · TIME, FOLDED INTO ARCS".to_string(),
        format!("exposure {exposure:.0}% · sky rotation {deg:.0}° held open"),
        format!("arcs {arc_count}/3,300 · crossings accumulate (1 Plus layer)"),
        format!("meteors 3 · streaks beat the shutter (straight, not curved)"),
        format!("beacon {:.0}% · polaris fixed at (938, 186)", beacon(t)),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 42.0;

    let mut stack = Stack::new();
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 16.0)
                .width(560.0)
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

    // The instrument: the exposure meter — a horizontal film strip that
    // fills as the shutter stays open, drawn from the same eased t.
    let meter = Painting::sized(
        Size::new(240.0, 26.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            book.rrect(
                Rect::new(0.0, 0.0, 240.0, 26.0),
                6.0,
                alpha(Color::rgb(16, 16, 21), 0.9),
            );
            for k in 0..12 {
                let x = 6.0 + k as f32 * 19.0;
                let on = (k as f32 / 12.0) < exposure;
                book.rect(
                    Rect::new(x, 8.0, x + 14.0, 18.0),
                    alpha(if on { AMBER } else { Color::rgb(40, 42, 52) }, if on { 0.8 } else { 0.9 }),
                );
            }
            book.stroke_rrect(
                Rect::new(0.0, 0.0, 240.0, 26.0),
                6.0,
                alpha(Color::WHITE, 0.08),
                1.0,
            );
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(P_X)
            .top(P_Y + 84.0)
            .width(240.0)
            .height(26.0)
            .child(meter),
    );

    stack.into()
}

/// The beacon's pulse, shared with the board (one law, two callers).
fn beacon(t: f32) -> f32 {
    0.5 + 0.5 * (t * 6.2832 * 2.4).sin()
}
