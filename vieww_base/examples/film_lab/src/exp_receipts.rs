//! exp_receipts — *S11, the receipts, as a props showcase.*
//!
//! E-13 + E-18 + E-19 in one shot: **the benchmark card assembles in 3D** —
//! three glass planes (title · body · footer) fly in on fanned perspective
//! quads, converging into one card — and then the card's *contents* are the
//! framework's own widget vocabulary, used as film props:
//!
//! - a `Chip` carrying the device ("Redmi Note 7 Pro")
//! - `LinearProgress` at `59.3/60` — **the value derived, not typed**
//! - a `LineChart` of the fps window (synthetic in the lab, probe-fed at
//!   master render — the graph's numbers rail)
//! - an `Icon` and a `Badge` counting the CI runs
//! - `Text` stats counting up: 59.3 fps · 4.09 ms (CI artifacts at master)
//!
//! Below the card, **E-19: the CI timeline draws itself as a path**, each
//! run blooming as a node with a check glyph — artifact-fed at master,
//! deterministic here.
//!
//! The 3D is the framework's own raster: assembly quads are projected
//! vertices (as in exp_mesh/exp_globe); settled card content is the plain
//! widget tree. The handoff between the moving painted plane and the crisp
//! widget panel is the card's own trick — motion is paint, information is
//! widgets.

use vieww_foundation::{
    Color, Dash, FontWeight, Gradient, Offset, Path, Rect, Size, Sketchbook, StrokeStyle,
    TextStyle,
};
use vieww_widget::prelude::*;
use vieww_widget::{Icon, LinearProgress, LineChart, Opacity, Painting, PaintWith, Theme, ThemeData};

use crate::film_lib::{
    alpha, clamp01, ease_out_expo, mix, spring_out, BG_DEEP, CANVAS, CANVAS_W, FAINT, INK, MUTED,
    Rng, VIOLET, VIOLET_SOFT,
};
use crate::three_d::{Camera, Vec3};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

// The receipts, sourced: 59.3/4.09 from ci/mobile/device-suite artifacts
// (the lab pins their targets; the master reads files). The progress value
// and the frame count are derived.
const FPS_MEDIAN: f32 = 59.3;
const MS_MEDIAN: f32 = 4.09;
const FPS_CAP: f32 = 60.0;
/// CI runs shown on the timeline — the node count, derived from the array.
const CI_RUNS: usize = 10;

/// The card's screen rect (left/top/right/bottom), where the planes converge.
const CARD: Rect = Rect::new(320.0, 118.0, 960.0, 462.0);
const TITLE_H: f32 = 58.0;
const PROGRESS_H: f32 = 44.0;

// ── The assembly: three planes, corner-lerped from 3D fan to flat card ─────

/// One assembly piece: target sub-rect of the card + its fan pose.
struct Piece {
    /// Sub-rect within CARD (screen space, where it lands).
    rect: Rect,
    /// Fan start: center, rotY, tilt, z-lift.
    fan: (Vec3, f32, f32),
    /// Landing stagger.
    delay: f32,
}

fn pieces() -> Vec<Piece> {
    vec![
        Piece {
            rect: Rect::new(CARD.left, CARD.top, CARD.right, CARD.top + TITLE_H),
            fan: (Vec3::new(-6.2, 3.6, 3.2), 0.7, 0.14),
            delay: 0.0,
        },
        Piece {
            rect: Rect::new(CARD.left, CARD.top + TITLE_H, CARD.right, CARD.bottom - PROGRESS_H),
            fan: (Vec3::new(0.0, 3.0, 1.6), 0.0, 0.0),
            delay: 0.07,
        },
        Piece {
            rect: Rect::new(CARD.left, CARD.bottom - PROGRESS_H, CARD.right, CARD.bottom),
            fan: (Vec3::new(6.2, 3.6, 3.2), -0.7, 0.14),
            delay: 0.14,
        },
    ]
}

/// The assembly camera — a stable vantage for the fan.
fn cam() -> Camera {
    Camera {
        eye: Vec3::new(0.0, 6.4, 17.5),
        target: Vec3::new(0.0, 3.0, 0.0),
        fov: 0.9,
    }
}

/// A piece's quad corners at progress `p`: lerp from the projected 3D quad
/// to the piece's exact screen rect. Landing is pixel-exact by construction.
fn piece_corners(piece: &Piece, p: f32) -> [Offset; 4] {
    let (center, rot_y, tilt) = piece.fan;
    // The 3D quad matching the target rect's aspect.
    let w = 7.0 * (piece.rect.width() / CARD.width());
    let h = w * (piece.rect.height() / piece.rect.width());
    let right = Vec3::new(rot_y.cos(), 0.0, -rot_y.sin());
    let up = Vec3::new(0.0, tilt.cos(), tilt.sin());
    let c3 = |u: f32, v: f32| {
        center
            .add(right.scale(u * w * 0.5))
            .add(up.scale(v * h * 0.5))
    };
    let camera = cam();
    let proj = |u: f32, v: f32| {
        camera
            .project(c3(u, v), CANVAS)
            .map(|x| x.0)
            .unwrap_or(Offset::new(CANVAS_W * 0.5, CANVAS.height * 0.5))
    };
    let target = [
        Offset::new(piece.rect.left, piece.rect.top),
        Offset::new(piece.rect.right, piece.rect.top),
        Offset::new(piece.rect.right, piece.rect.bottom),
        Offset::new(piece.rect.left, piece.rect.bottom),
    ];
    let start = [proj(-0.5, -0.5), proj(0.5, -0.5), proj(0.5, 0.5), proj(-0.5, 0.5)];
    let mut out = [Offset::ZERO; 4];
    for i in 0..4 {
        out[i] = Offset::new(
            start[i].dx + (target[i].dx - start[i].dx) * p,
            start[i].dy + (target[i].dy - start[i].dy) * p,
        );
    }
    out
}

// ── The scene painter ───────────────────────────────────────────────────────

fn scene(book: &mut Sketchbook, t: f32) {
    let w = CANVAS_W;
    let h = CANVAS.height;

    // The ground.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::vertical().with_dither().with_stops(&[
            (0.0, Color::rgb(8, 8, 12)),
            (0.55, BG_DEEP),
            (1.0, Color::rgb(12, 10, 18)),
        ]),
    );

    // Stars.
    let mut rng = Rng::new(0x51_11);
    for _ in 0..42 {
        let x = rng.f01() * w;
        let y = rng.f01() * h;
        let r = 0.4 + rng.f01() * 0.8;
        let tw = 0.5 + 0.5 * (t * 3.0 + rng.f01() * 9.0).sin();
        book.circle(Offset::new(x, y), r, alpha(Color::WHITE, 0.03 + 0.07 * tw));
    }

    // A quiet horizon behind the card.
    book.layer(1.0, 40.0, None, |inner| {
        inner.circle(
            Offset::new(w * 0.5, h * 0.40),
            w * 0.30,
            Gradient::radial_fill().with_stops(&[
                (0.0, alpha(VIOLET, 0.10)),
                (1.0, alpha(VIOLET, 0.0)),
            ]),
        );
    });

    // ── The assembly ────────────────────────────────────────────────────────
    let asm_t = clamp01(t / 0.42);
    let mut settled_all = true;
    for piece in pieces() {
        let p = spring_out(clamp01((asm_t - piece.delay) / 0.5), 9.0, 0.66);
        if p <= 0.0 {
            settled_all = false;
            continue;
        }
        if p < 0.98 {
            settled_all = false;
        }
        let corners = piece_corners(&piece, p);
        // Motion smear: a fainter, slightly-larger echo of the piece a few
        // steps back — the trail speed leaves behind.
        if p > 0.05 && p < 0.96 {
            let trail = piece_corners(&piece, (p - 0.09).max(0.0));
            let mut tq = Path::new();
            tq.move_to(trail[0]);
            tq.line_to(trail[1]);
            tq.line_to(trail[2]);
            tq.line_to(trail[3]);
            tq.close();
            book.fill(tq, alpha(VIOLET, 0.05 * (1.0 - p)));
            book.stroke_rrect(
                Rect::new(trail[0].dx - 2.0, trail[0].dy - 2.0, trail[1].dx + 2.0, trail[2].dy + 2.0),
                10.0,
                alpha(VIOLET_SOFT, 0.10 * (1.0 - p)),
                1.6,
            );
        }
        let mut quad = Path::new();
        quad.move_to(corners[0]);
        quad.line_to(corners[1]);
        quad.line_to(corners[2]);
        quad.line_to(corners[3]);
        quad.close();
        // Glass: violet, brighter at the piece's floor, arriving with p —
        // brighter in flight than at rest, so the dark never swallows it.
        book.fill(
            quad.clone(),
            Gradient::vertical().with_stops(&[
                (0.0, alpha(mix(VIOLET, Color::WHITE, 0.4), 0.09 + 0.03 * p)),
                (1.0, alpha(VIOLET, 0.13 + 0.04 * p)),
            ]),
        );
        book.stroke(quad, alpha(VIOLET_SOFT, 0.32 + 0.5 * p), 1.2);
        // A motion shadow on the void's floor while flying.
        if p < 0.98 {
            let c = (corners[0].dx + corners[2].dx) * 0.5;
            let bottom = corners[2].dy.max(corners[3].dy);
            book.layer(1.0, 12.0, None, |inner| {
                inner.circle(
                    Offset::new(c, bottom + 60.0),
                    180.0 * (1.0 - p),
                    Gradient::radial_fill().with_stops(&[
                        (0.0, alpha(Color::BLACK, 0.35 * (1.0 - p))),
                        (1.0, alpha(Color::BLACK, 0.0)),
                    ]),
                );
            });
        }
    }

    // The settled card: one crisp panel with a real drop shadow.
    if settled_all || asm_t > 0.85 {
        let settle = clamp01((asm_t - 0.75) / 0.25);
        book.shadow(
            Rect::new(CARD.left, CARD.top + 8.0, CARD.right, CARD.bottom),
            18.0,
            vieww_foundation::Shadow {
                color: alpha(Color::BLACK, 0.55 * settle),
                offset: Offset::new(0.0, 14.0),
                blur: 26.0,
                spread: 0.0,
                is_inset: false,
            },
        );
        book.rrect(
            CARD,
            16.0,
            Gradient::vertical().with_stops(&[
                (0.0, alpha(mix(VIOLET, Color::WHITE, 0.45), 0.06 * settle)),
                (1.0, alpha(VIOLET, 0.11 * settle)),
            ]),
        );
        book.stroke_rrect(CARD, 16.0, alpha(VIOLET_SOFT, 0.5 * settle), 1.2);
        // A hairline top edge — the card's catch-light.
        book.rrect(
            Rect::new(CARD.left + 1.0, CARD.top + 1.0, CARD.right - 1.0, CARD.top + 3.0),
            1.0,
            alpha(Color::WHITE, 0.10 * settle),
        );
    }

    // ── E-19 · the CI timeline draws itself ─────────────────────────────────
    let ci_t = clamp01((t - 0.58) / 0.4);
    if ci_t > 0.0 {
        let draw = ease_out_expo(ci_t);
        let y0 = 566.0;
        let x0 = 96.0;
        let x1 = 1184.0;
        // The path: a run history — mostly level, one failure dip, recovered.
        let mut pts: Vec<Offset> = Vec::new();
        for i in 0..=80 {
            let s = i as f32 / 80.0;
            let x = x0 + (x1 - x0) * s;
            // One failure dip at ~55%, depth 18 — the timeline's story.
            let dip = ((s - 0.55) / 0.055).abs().min(1.0);
            let y = y0 - dip * dip * 20.0;
            pts.push(Offset::new(x, y));
        }
        let total: f32 = pts.windows(2).map(|q| (q[1].dx - q[0].dx).hypot(q[1].dy - q[0].dy)).sum();
        let mut path = Path::new();
        path.move_to(pts[0]);
        for q in &pts[1..] {
            path.line_to(*q);
        }
        let style = StrokeStyle::default().dash(Dash::new(vec![total * draw, total + 1.0]));
        book.stroke_styled(path, alpha(MUTED, 0.6), 1.4, style);

        // The run nodes bloom as the front passes.
        for i in 0..CI_RUNS {
            let nf = i as f32 / (CI_RUNS - 1) as f32;
            let node_t = clamp01((draw - nf) / 0.02);
            if node_t <= 0.0 {
                continue;
            }
            let s = nf;
            let dip = ((s - 0.55) / 0.055).abs().min(1.0);
            let node = Offset::new(x0 + (x1 - x0) * nf, y0 - dip * dip * 20.0);
            let failed = i + 1 == 6; // the dip's run — the timeline's honesty
            let color = if failed {
                crate::film_lib::RED
            } else {
                crate::film_lib::MINT
            };
            let spring = spring_out(node_t, 12.0, 0.55);
            book.ring(node, 5.0 + 12.0 * (1.0 - node_t), 1.3, alpha(color, (1.0 - node_t) * 0.5));
            book.circle(node, 3.8 + 0.8 * (spring - 1.0), color);
        }
    }

    // The vignette.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::radial(Offset::new(0.5, 0.5), 0.8).with_dither().with_stops(&[
            (0.6, alpha(Color::BLACK, 0.0)),
            (1.0, alpha(Color::BLACK, 0.42)),
        ]),
    );
}

// ── The widget layer — the props ────────────────────────────────────────────

/// Deterministic fps samples for the chart — synthetic, seeded, honest.
fn fps_samples() -> Vec<f32> {
    let mut rng = Rng::new(0xF935);
    (0..26)
        .map(|_| 57.8 + rng.f01() * 3.4)
        .collect()
}

pub fn frame(t: f32) -> WidgetNode {
    let asm_t = clamp01(t / 0.42);

    // Each piece's content blooms as its plane lands.
    let bloom = |delay: f32| clamp01((asm_t - delay - 0.42) / 0.3);

    let title_bloom = bloom(0.0);
    let body_bloom = bloom(0.07);
    let foot_bloom = bloom(0.14);

    let stats_start = clamp01((t - 0.40) / 0.34);
    let fps_v = FPS_MEDIAN * spring_out(clamp01((t - 0.42) / 0.34), 12.0, 0.7);
    let ms_v = MS_MEDIAN * spring_out(clamp01((t - 0.52) / 0.34), 12.0, 0.7);

    // ── Title bar: label + the device chip ─────────────────────────────────
    let mut title = Stack::new();
    if title_bloom > 0.0 {
        title = title
            .push(
                Positioned::new()
                    .left(CARD.left + 22.0)
                    .top(CARD.top + 20.0)
                    .width(220.0)
                    .height(18.0)
                    .child(
                        Opacity::new(title_bloom).child(
                            Text::new("DEVICE SUITE").style(
                                TextStyle::new(13.0)
                                    .monospace()
                                    .letter_spacing(3.0)
                                    .color(alpha(MUTED, 0.9)),
                            ),
                        ),
                    ),
            )
            .push(
                Positioned::new()
                    .left(CARD.left + CARD.width() - 200.0)
                    .top(CARD.top + 13.0)
                    .width(180.0)
                    .height(32.0)
                    .child(Opacity::new(title_bloom).child(Chip::new("Redmi Note 7 Pro"))),
            );
    }

    // ── Body: the stats + the chart ────────────────────────────────────────
    let mut body = Stack::new();
    if body_bloom > 0.0 {
        let body_y = CARD.top + TITLE_H;
        // The fps median — counting up, Bold when settled.
        let fps_settled = fps_v >= FPS_MEDIAN * 0.999;
        body = body
            .push(
                Positioned::new()
                    .left(CARD.left + 22.0)
                    .top(body_y + 18.0)
                    .width(220.0)
                    .height(56.0)
                    .child(
                        Opacity::new(body_bloom).child(
                            Text::new(format!("{:.1}", fps_v)).style(
                                TextStyle::new(44.0)
                                    .monospace()
                                    .weight(if fps_settled { FontWeight::Bold } else { FontWeight::Medium })
                                    .color(if fps_settled { INK } else { alpha(MUTED, 0.9) }),
                            ),
                        ),
                    ),
            )
            .push(
                Positioned::new()
                    .left(CARD.left + 22.0)
                    .top(body_y + 74.0)
                    .width(240.0)
                    .height(16.0)
                    .child(
                        Opacity::new(body_bloom).child(
                            Text::new("FPS MEDIAN · 60 S WINDOW").style(
                                TextStyle::new(11.0)
                                    .monospace()
                                    .letter_spacing(2.0)
                                    .color(alpha(FAINT, 0.9)),
                            ),
                        ),
                    )
            )
            // The ms median — second stat.
            .push(
                Positioned::new()
                    .left(CARD.left + 22.0)
                    .top(body_y + 108.0)
                    .width(220.0)
                    .height(40.0)
                    .child(
                        Opacity::new(body_bloom).child(
                            Text::new(format!("{:.2}", ms_v)).style(
                                TextStyle::new(30.0).monospace().color(alpha(INK, 0.85)),
                            ),
                        ),
                    ),
            )
            .push(
                Positioned::new()
                    .left(CARD.left + 22.0)
                    .top(body_y + 148.0)
                    .width(240.0)
                    .height(16.0)
                    .child(
                        Opacity::new(body_bloom).child(
                            Text::new("MS MEDIAN · FRAME").style(
                                TextStyle::new(11.0)
                                    .monospace()
                                    .letter_spacing(2.0)
                                    .color(alpha(FAINT, 0.9)),
                            ),
                        ),
                    ),
            )
            // The chart — the framework's own, as a prop.
            .push(
                Positioned::new()
                    .left(CARD.left + 280.0)
                    .top(body_y + 12.0)
                    .width(340.0)
                    .height(160.0)
                    .child(
                        Opacity::new(body_bloom).child(
                            LineChart::new(fps_samples()).label("fps · window"),
                        ),
                    ),
            );
        let _ = stats_start;
    }

    // ── Footer: the progress bar (value derived) + the runs badge ──────────
    let mut footer = Stack::new();
    if foot_bloom > 0.0 {
        let foot_y = CARD.bottom - PROGRESS_H;
        footer = footer
            .push(
                Positioned::new()
                    .left(CARD.left + 22.0)
                    .top(foot_y + 8.0)
                    .width(CARD.width() - 190.0)
                    .height(18.0)
                    .child(
                        Opacity::new(foot_bloom).child(LinearProgress::new(FPS_MEDIAN / FPS_CAP)),
                    ),
            )
            .push(
                Positioned::new()
                    .left(CARD.left + 22.0)
                    .top(foot_y + 28.0)
                    .width(360.0)
                    .height(14.0)
                    .child(
                        Opacity::new(foot_bloom).child(
                            Text::new(format!("fps median / cap = {:.3}", FPS_MEDIAN / FPS_CAP)).style(
                                TextStyle::new(10.5)
                                    .monospace()
                                    .letter_spacing(1.5)
                                    .color(alpha(FAINT, 0.8)),
                            ),
                        ),
                    ),
            )
            .push(
                Positioned::new()
                    .left(CARD.left + CARD.width() - 150.0)
                    .top(foot_y + 6.0)
                    .width(130.0)
                    .height(30.0)
                    .child(
                        Opacity::new(foot_bloom).child(
                            Badge::new(Icon::new(icons::check()).size(15.0).color(alpha(INK, 0.85)))
                                .count(CI_RUNS as u32),
                        ),
                    ),
            );
    }

    // ── The CI caption — S11's own line, from the graph ───────────────────
    let cap_t = clamp01((t - 0.82) / 0.16);
    let caption: WidgetNode = if cap_t > 0.01 {
        Positioned::new()
            .left(CANVAS_W * 0.5 - 320.0)
            .top(684.0)
            .width(640.0)
            .height(22.0)
            .child(
                Opacity::new(cap_t).child(
                    Text::new("59.3 · 4.09 · Redmi Note 7 Pro — from CI artifacts")
                        .style(TextStyle::new(12.5).monospace().letter_spacing(1.5).color(alpha(MUTED, 0.95)))
                        .align(vieww_foundation::TextAlign::Center),
                ),
            )
            .into()
    } else {
        SizedBox::shrink().into()
    };

    // The CI timeline's label.
    let ci_label_t = clamp01((t - 0.58) / 0.2);
    let ci_label: WidgetNode = if ci_label_t > 0.01 {
        Positioned::new()
            .left(96.0)
            .top(522.0)
            .width(400.0)
            .height(16.0)
            .child(
                Opacity::new(ci_label_t).child(
                    Text::new("CI · MOBILE / DEVICE-SUITE · MAIN").style(
                        TextStyle::new(11.0)
                            .monospace()
                            .letter_spacing(2.5)
                            .color(alpha(FAINT, 0.9)),
                    ),
                ),
            )
            .into()
    } else {
        SizedBox::shrink().into()
    };

    let paint = Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            scene(book, t);
            let _ = size;
        }),
    );

    // The props render inside a dark theme — controls read the theme, and
    // the film's world is dark.
    let content: WidgetNode = Stack::new()
        .push(Positioned::fill().child(paint))
        .push(title)
        .push(body)
        .push(footer)
        .push(ci_label)
        .push(caption)
        .into();
    Theme::new(ThemeData::dark()).child(content).into()
}
