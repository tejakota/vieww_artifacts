//! exp_eclipse — *the narrative axis.* Totality.
//!
//! The first plate whose subject is a **documented sequence of light** —
//! every beat of a total solar eclipse, in order, on the clock the receipt
//! prints: first contact, the deepening crescent, crescent shadows under a
//! tree, the light draining (a full-sky palette shift carried by ONE
//! vertical gradient whose stops are re-lerped every frame), Baily's beads,
//! the diamond ring, totality — corona streamers through ONE Plus-blended
//! group, the 360° sunset, stars and planets out in daytime — then the
//! whole ladder run backwards. Nothing here is a still image decorated with
//! motion: the plate is the timeline itself.
//!
//! The crescent is honest geometry — the moon is a disc laid over the sun,
//! and the receipt's obscuration is the true circle-overlap area computed
//! from the same two radii and the same offset that drew the frame, not the
//! driver parameter. The beads and the diamond ring live on the limb the
//! overlap actually produces; the 360° sunset ring's alpha is the same
//! totality factor the sky was lerped by.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, clamp01, mix, shade, smoothstep, Rng, AMBER, BG_DEEP, FAINT, INK,
    MUTED, VIOLET_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 14.0;

// ── The clock of the eclipse ────────────────────────────────────────────────

const T_FIRST: f32 = 0.07;   // first contact
const T_TOTAL: f32 = 0.52;   // totality begins
const T_ENGEND: f32 = 0.76;  // totality ends (third contact)
const T_LAST: f32 = 0.97;    // last contact

/// The overlap-area obscuration of two equal discs at centre distance `d`.
/// This is the number the receipt prints — measured from the geometry the
/// frame actually drew, not the input parameter.
#[must_use]
fn obscuration_from(d: f32, r: f32) -> f32 {
    let x = clamp01(d / (2.0 * r)); // d / (r1 + r2)
    if x >= 1.0 {
        return 0.0;
    }
    // Standard lens area for two unit circles whose centres are 2x apart,
    // scaled by the disc area.
    let theta = 2.0 * (1.0 - x * x).sqrt().clamp(-1.0, 1.0).acos();
    let lens = theta - 0.5 * (2.0 * theta).sin();
    (lens / std::f32::consts::PI).min(1.0)
}

/// The moon's centre offset that produces the phase the timeline wants.
#[must_use]
fn moon_d(t: f32) -> f32 {
    let ing = smoothstep((t - T_FIRST) / (T_TOTAL - T_FIRST));
    let egr = smoothstep((t - T_ENGEND) / (T_LAST - T_ENGEND));
    let p = (ing * (1.0 - egr)).clamp(0.0, 1.0);
    // Offset eased the other way: fast bite early, slow final approach.
    let e = 1.0 - (1.0 - p).powi(3);
    let d0 = 2.0 * R_SUN + 26.0;
    d0 * (1.0 - e)
}

/// How deep into totality we are: 0 outside, 1 at maximum.
#[must_use]
fn totality(t: f32) -> f32 {
    let a = smoothstep((t - (T_TOTAL - 0.012)) / 0.03);
    let b = 1.0 - smoothstep((t - T_ENGEND) / 0.03);
    a * b
}

/// Scene light, 1 = day, 0 = deepest eclipse.
#[must_use]
fn daylight(t: f32) -> f32 {
    1.0 - 0.94 * totality(t) - 0.42 * (smoothstep((t - T_FIRST) / (T_TOTAL - T_FIRST))
        * (1.0 - smoothstep((t - T_ENGEND) / (T_LAST - T_ENGEND))))
        * (1.0 - totality(t))
}

// ── The scene ───────────────────────────────────────────────────────────────

const SUN: (f32, f32) = (640.0, 296.0);
const R_SUN: f32 = 104.0;

/// The sun's centre as an `Offset`.
fn sun_c() -> Offset {
    Offset::new(SUN.0, SUN.1)
}

/// Day sky stops (top, mid, horizon) — the palette the light lerps.
const SKY_DAY: [Color; 3] = [
    Color::rgb(96, 148, 200),
    Color::rgb(150, 186, 220),
    Color::rgb(214, 226, 230),
];
const SKY_DARK: [Color; 3] = [
    Color::rgb(8, 9, 18),
    Color::rgb(14, 15, 28),
    Color::rgb(30, 26, 44),
];

/// One star in the eclipse sky.
struct Star {
    x: f32,
    y: f32,
    mag: f32,
    bright: bool,
}

fn stars() -> Vec<Star> {
    let mut rng = Rng::new(0x6311_u64);
    let mut v = Vec::new();
    while v.len() < 210 {
        let x = 30.0 + rng.f01() * 1220.0;
        let y = 26.0 + rng.f01() * 400.0;
        // Keep stars out of the sun's immediate disc.
        if ((x - SUN.0).powi(2) + (y - SUN.1).powi(2)).sqrt() < R_SUN + 40.0 {
            continue;
        }
        let r = rng.f01();
        v.push(Star { x, y, mag: 0.25 + 0.75 * r * r, bright: r > 0.90 });
    }
    v
}

pub fn frame(t: f32) -> WidgetNode {
    let stars = stars();
    let obs = obscuration_from(moon_d(t), R_SUN);
    let light = daylight(t);
    let tot = totality(t);
    // The bead/diamond windows, measured where they live: on the limb.
    let bead = ((1.0 - obs - 0.9945) / 0.004).clamp(0.0, 1.0).min(1.0)
        * smoothstep((t - 0.30) / 0.15)
        * (1.0 - smoothstep((t - 0.86) / 0.09));
    let diamond = ((1.0 - obs - 0.9988) / 0.0012).clamp(0.0, 1.0)
        * smoothstep((t - 0.34) / 0.15)
        * (1.0 - smoothstep((t - 0.87) / 0.09));
    // How many stars are visible enough to draw this frame (the census).
    let star_vis = stars.iter().filter(|s| (1.0 - light) * s.mag > 0.06).count();

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // ── The sky — one vertical gradient, stops re-lerped by light. ──
            let top = mix(SKY_DARK[0], SKY_DAY[0], light);
            let mid = mix(SKY_DARK[1], SKY_DAY[1], light);
            let hor = mix(SKY_DARK[2], SKY_DAY[2], light);
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, top),
                    (0.62, mid),
                    (1.0, hor),
                ]),
            );

            // The 360° sunset — the horizon ring only totality makes: the
            // whole horizon glows at once because every distant azimuth is
                // catching twilight. Its alpha is `totality` itself.
            if tot > 0.001 {
                book.rect(
                    Rect::new(0.0, 528.0, w, 620.0),
                    Gradient::vertical().with_dither().with_stops(&[
                        (0.0, alpha(shade(AMBER, 0.25), 0.0)),
                        (0.55, alpha(mix(AMBER, Color::rgb(255, 120, 60), 0.5), 0.42 * tot)),
                        (1.0, alpha(Color::rgb(210, 90, 50), 0.0)),
                    ]),
                );
            }

            // ── Stars and the two planets totality reveals. ──
            let star_gate = (1.0 - light).max(0.0);
            if star_gate > 0.02 {
                for s in &stars {
                    let a = star_gate * s.mag;
                    if a <= 0.06 {
                        continue;
                    }
                    book.circle(
                        Offset::new(s.x, s.y),
                        if s.bright { 2.1 } else { 1.15 },
                        alpha(Color::rgb(226, 232, 244), a),
                    );
                    if s.bright {
                        book.line(
                            Offset::new(s.x - 5.0, s.y),
                            Offset::new(s.x + 5.0, s.y),
                            alpha(Color::rgb(226, 232, 244), a * 0.35),
                            0.7,
                        );
                    }
                }
                // Venus (below-right of the sun) and Jupiter (above-left) —
                // out in daylight, exactly as they are during totality.
                book.circle(
                    Offset::new(SUN.0 + 218.0, SUN.1 + 128.0),
                    3.0,
                    alpha(Color::rgb(248, 236, 200), star_gate * 0.95),
                );
                book.circle(
                    Offset::new(SUN.0 - 262.0, SUN.1 - 118.0),
                    3.2,
                    alpha(Color::rgb(240, 226, 196), star_gate * 0.9),
                );
            }

            // ── The sun / the eclipsed pair. ──
            // Photosphere (drawn full, the moon eats it next frame-locally).
            book.circle(sun_c(), R_SUN, Color::rgb(255, 252, 244));

            // The moon: a disc of sky — its fill is the sky colour AT ITS
            // CENTRE, so it moves across the gradient honestly.
            let moon_c = Offset::new(SUN.0 + moon_d(t) * 0.86, SUN.1 - moon_d(t) * 0.51);
            let sky_at_moon = mix(mid, top, clamp01((moon_c.dy - 0.62 * h) / (0.62 * h)));
            book.circle(moon_c, R_SUN, shade(sky_at_moon, 0.22));

            // ── Corona — streamers through ONE Plus-blended group. ──
            if tot > 0.001 {
                let mut rng = Rng::new(0xC0FFEE_u64);
                book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                    // The inner corona: a soft disc hugging the limb.
                    g.circle(
                        sun_c(),
                        R_SUN + 26.0,
                        Gradient::radial_fill().with_dither().with_stops(&[
                            (0.0, alpha(Color::rgb(255, 252, 240), 0.85)),
                            (0.35, alpha(Color::rgb(232, 236, 255), 0.30)),
                            (1.0, alpha(Color::WHITE, 0.0)),
                        ]),
                    );
                    // Streamers: long wedge petals, breathing. Each is
                    // drawn as three stacked quads of falling alpha —
                    // gradient geometry is local-box space (the U-23-era
                    // lesson), so the falloff is built as geometry.
                    for k in 0..26 {
                        let a = rng.f01() * std::f32::consts::TAU;
                        let breathe = 0.75 + 0.25 * (t * 6.2832 * 0.7 + a * 3.0).sin();
                        let len = (110.0 + rng.f01() * 190.0) * breathe;
                        let half = 0.028 + rng.f01() * 0.05;
                        let warm = rng.f01() > 0.55;
                        let base = if warm {
                            Color::rgb(255, 244, 224)
                        } else {
                            Color::rgb(228, 234, 255)
                        };
                        let r0 = R_SUN - 2.0;
                        // Three depth bands: 0..0.4, 0.4..0.75, 0.75..1 of len.
                        for (f0, f1, dim) in
                            [(0.0_f32, 0.40_f32, 0.32), (0.40, 0.75, 0.13), (0.75, 1.0, 0.045)]
                        {
                            let taper = |fr: f32| 1.0 - fr * 0.66; // narrower outward
                            let w0 = half * taper(f0);
                            let w1 = half * taper(f1);
                            let ra = r0 + len * f0;
                            let rb = r0 + len * f1;
                            let mut p = Path::new();
                            p.move_to(Offset::new(
                                SUN.0 + (a - w0).cos() * ra,
                                SUN.1 + (a - w0).sin() * ra,
                            ));
                            p.line_to(Offset::new(
                                SUN.0 + (a - w1).cos() * rb,
                                SUN.1 + (a - w1).sin() * rb,
                            ));
                            p.line_to(Offset::new(
                                SUN.0 + (a + w1).cos() * rb,
                                SUN.1 + (a + w1).sin() * rb,
                            ));
                            p.line_to(Offset::new(
                                SUN.0 + (a + w0).cos() * ra,
                                SUN.1 + (a + w0).sin() * ra,
                            ));
                            p.close();
                            g.fill(p, alpha(base, dim));
                        }
                    }
                    // Three prominences on the limb — the chromosphere's
                    // rose-pink tongues, at fixed limb angles.
                    for (pa, pl) in [(0.8_f32, 9.0_f32), (2.9, 12.0), (4.6, 7.0)] {
                        g.circle(
                            Offset::new(
                                SUN.0 + pa.cos() * (R_SUN + 3.0),
                                SUN.1 + pa.sin() * (R_SUN + 3.0),
                            ),
                            pl,
                            alpha(Color::rgb(255, 120, 130), 0.8),
                        );
                    }
                });
            }

            // ── Baily's beads and the diamond ring. ──
            if bead > 0.003 {
                let mut rng = Rng::new(0xB41D_u64);
                // Beads sit on the SUN's limb where the moon's edge isn't:
                // the trailing limb during ingress, leading during egress.
                let ingress_side = t < 0.5;
                let base_a = if ingress_side { -2.62_f32 } else { 0.52 };
                for k in 0..6 {
                    let a = base_a + (k as f32 - 2.5) * 0.14;
                    let sz = 3.0 + rng.f01() * 3.4;
                    book.blended_layer(
                        1.0,
                        0.0,
                        vieww_foundation::BlendMode::Plus,
                        None,
                        |g| {
                            g.circle(
                                Offset::new(
                                    SUN.0 + a.cos() * R_SUN,
                                    SUN.1 + a.sin() * R_SUN,
                                ),
                                sz,
                                alpha(Color::rgb(255, 250, 235), 0.95 * bead),
                            );
                        },
                    );
                }
            }
            if diamond > 0.003 {
                // The diamond: one bead outliving the others, flaring.
                let da = if t < 0.5 { 0.55 } else { 0.55 + std::f32::consts::PI };
                let dc = Offset::new(SUN.0 + da.cos() * R_SUN, SUN.1 + da.sin() * R_SUN);
                book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                    g.circle(
                        dc,
                        34.0 * diamond,
                        Gradient::radial_fill().with_dither().with_stops(&[
                            (0.0, alpha(Color::WHITE, 0.98 * diamond)),
                            (0.3, alpha(Color::rgb(255, 250, 230), 0.55 * diamond)),
                            (1.0, alpha(Color::WHITE, 0.0)),
                        ]),
                    );
                    // The flare: two crossed spikes.
                    for rot in [0.0_f32, std::f32::consts::FRAC_PI_2] {
                        let (s, c) = rot.sin_cos();
                        let mut p = Path::new();
                        p.move_to(Offset::new(dc.dx - c * 92.0, dc.dy - s * 92.0));
                        p.line_to(Offset::new(dc.dx + s * 5.0, dc.dy - c * 5.0));
                        p.line_to(Offset::new(dc.dx + c * 92.0, dc.dy + s * 92.0));
                        p.line_to(Offset::new(dc.dx - s * 5.0, dc.dy + c * 5.0));
                        p.close();
                        g.fill(p, alpha(Color::rgb(255, 251, 238), 0.85 * diamond));
                    }
                });
            }

            // ── The ground: hills, a tree, a lake — and the crescents. ──
            let g_day = mix(Color::rgb(28, 34, 30), Color::rgb(96, 122, 92), light);
            let g_deep = shade(g_day, 0.45);
            let horizon = 620.0;
            let mut hill = Path::new();
            hill.move_to(Offset::new(0.0, horizon + 8.0));
            hill.line_to(Offset::new(0.0, 636.0));
            hill.line_to(Offset::new(240.0, 622.0));
            hill.line_to(Offset::new(520.0, 648.0));
            hill.line_to(Offset::new(860.0, 616.0));
            hill.line_to(Offset::new(1100.0, 640.0));
            hill.line_to(Offset::new(1280.0, 626.0));
            hill.line_to(Offset::new(1280.0, 720.0));
            hill.line_to(Offset::new(0.0, 720.0));
            hill.close();
            book.fill(hill, g_deep);

            // The lake: the horizon band the sky and (at totality) the 360°
            // sunset reflect into.
            let lake_top = mix(g_day, hor, 0.30);
            book.rect(
                Rect::new(0.0, 648.0, w, 720.0),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, mix(lake_top, Color::BLACK, 0.1)),
                    (1.0, shade(lake_top, 0.55)),
                ]),
            );

            // The tree — the crescent-shadow projector. Its gaps cast the
            // little crescents on the ground during the partial phase.
            let tree_x = 238.0;
            book.rect(Rect::new(tree_x - 4.0, 528.0, tree_x + 4.0, 636.0), shade(g_deep, 0.4));
            let mut crown = Path::new();
            crown.move_to(Offset::new(tree_x, 470.0));
            crown.line_to(Offset::new(tree_x + 62.0, 522.0));
            crown.line_to(Offset::new(tree_x + 34.0, 552.0));
            crown.line_to(Offset::new(tree_x - 34.0, 552.0));
            crown.line_to(Offset::new(tree_x - 62.0, 522.0));
            crown.close();
            book.fill(crown, shade(g_deep, 0.5));

            // Crescent shadows: the partial phase's signature. Light through
            // any small gap projects the crescent sun. Drawn when the phase
            // is partial enough, fading near totality.
            let partial = (obs > 0.18 && obs < 0.985) as u8 as f32;
            if partial > 0.5 {
                let mut rng = Rng::new(0xC5E5_u64);
                for k in 0..16 {
                    let x = 340.0 + rng.f01() * 760.0;
                    let y = 656.0 + rng.f01() * 44.0;
                    let s = 4.5 + rng.f01() * 5.5;
                    // The crescent: a light patch with the moon's bite
                    // taken out — same two discs, 1:30 scale.
                    let d = moon_d(t) / (2.0 * R_SUN + 26.0) * 2.4 * s;
                    let light_col = mix(
                        alpha(mix(g_day, Color::rgb(220, 226, 214), 0.5), 0.5),
                        alpha(Color::rgb(220, 226, 214), 0.55),
                        light,
                    );
                    book.circle(Offset::new(x, y), s, light_col);
                    book.circle(Offset::new(x + d * 0.86, y - d * 0.51), s, shade(g_deep, 0.2));
                }
            }
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack.push(receipt_panel(t, obs, light, star_vis, bead, diamond)).into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(
    t: f32,
    obs: f32,
    light: f32,
    star_vis: usize,
    bead: f32,
    diamond: f32,
) -> WidgetNode {
    let phase = if totality(t) > 0.6 {
        "TOTALITY — corona + 360° sunset"
    } else if diamond > 0.05 {
        "DIAMOND RING"
    } else if bead > 0.05 {
        "BAILY'S BEADS"
    } else if obs > 0.02 {
        "PARTIAL PHASE"
    } else if t > 0.5 {
        "AFTER LAST CONTACT"
    } else {
        "BEFORE FIRST CONTACT"
    };
    let lines = [
        "ECLIPSE · THE NARRATIVE AXIS · TOTALITY".to_string(),
        format!("phase: {phase}"),
        format!(
            "obscuration {obs:.1}% (lens area, computed) · moon offset {:.1} px",
            moon_d(t)
        ),
        format!(
            "scene light {light:.2} · stars visible {star_vis}/210 · planets 2"
        ),
        format!(
            "corona: 26 streamers · 1 Plus layer · beads α {bead:.2} · diamond α {diamond:.2}"
        ),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 40.0;

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

    // The instrument: the phase clock — a gauge whose needle is the
    // obscuration, drawn from the same numbers the receipt prints.
    let clock = Painting::sized(
        Size::new(150.0, 84.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            let c = Offset::new(75.0, 46.0);
            book.rrect(
                Rect::new(0.0, 0.0, 150.0, 84.0),
                8.0,
                alpha(Color::rgb(14, 14, 20), 0.88),
            );
            book.stroke_rrect(
                Rect::new(0.0, 0.0, 150.0, 84.0),
                8.0,
                alpha(Color::WHITE, 0.08),
                1.0,
            );
            book.arc(
                c,
                26.0,
                6.0,
                std::f32::consts::PI,
                std::f32::consts::PI * (obs.clamp(0.0, 1.0)),
                alpha(
                    if totality(t) > 0.5 {
                        VIOLET_SOFT
                    } else if obs > 0.02 {
                        AMBER
                    } else {
                        FAINT
                    },
                    0.85,
                ),
            );
            book.line(
                c,
                Offset::new(
                    75.0 + (std::f32::consts::PI * (1.0 + obs.clamp(0.0, 1.0))).cos() * 20.0,
                    46.0 + (std::f32::consts::PI * (1.0 + obs.clamp(0.0, 1.0))).sin() * 20.0,
                ),
                alpha(INK, 0.8),
                1.6,
            );
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(P_X)
            .top(P_Y + 88.0)
            .width(150.0)
            .height(84.0)
            .child(clock),
    );

    stack.into()
}
