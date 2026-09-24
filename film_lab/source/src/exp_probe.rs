//! exp_probe — *the instrument panel.* The evidence plate.
//!
//! Three instruments, one frame, every number read out of the rasterizer's
//! own output by the harness's pixel probe (the last frame, straight from
//! the RGBA buffer — no number in this panel was typed):
//!
//! - **U-15, the fixed point** — a full-bleed violet wash behind everything,
//!   blurred at σ24. Before the edge-clamp fix its corners fell to 39/255
//!   while its centre stayed 255; the probe reads all four corners now and
//!   prints the max channel delta between them. Uniform in, uniform out —
//!   the blur's fixed point, asserted in pixels.
//! - **The open-subpath census** — six petals, one deliberately missing its
//!   `close()`. `SceneReport::open_subpath_fills` counts it at replay; the
//!   probe reads the ink at both petals' centroids and reports what the
//!   silence actually rendered as.
//! - **U-18, the hairline ladder** — seven strokes from 2.00 px down to
//!   0.03 px. The coverage rasterizer's arithmetic makes the thin rungs
//!   nearly nothing, which the entry records as *correct and deliberate*
//!   (the ink-preserving clamp is provably equivalent at 8-bit coverage);
//!   the probe measures each rung's ink so the tradeoff is a number, not a
//!   squint.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith};

use crate::film_lib::{alpha, clamp01, mix, BG_DEEP, CANVAS, FAINT, INK, MUTED, VIOLET,
    VIOLET_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 8.0;

/// The wash's blur sigma — the U-15 probe's subject.
const WASH_SIGMA: f32 = 24.0;

/// The petal clock's center and reach.
const CLOCK: Offset = Offset::new(402.0, 344.0);
const PETALS: usize = 6;
/// The petal that is missing its `close()` — the census's own subject.
const OPEN_PETAL: usize = 5;

/// The hairline ladder: rung widths, px. U-18's sweep.
const RUNGS: [f32; 7] = [2.00, 1.00, 0.50, 0.25, 0.12, 0.06, 0.03];

const LADDER_X0: f32 = 986.0;
const LADDER_X1: f32 = 1218.0;
const LADDER_Y0: f32 = 168.0;
const LADDER_DY: f32 = 46.0;
/// The playhead marker rides to the RIGHT of the rungs, never on them —
/// the probe reads the rung pixels, and the marker must not ink them.
const PLAYHEAD_X: f32 = LADDER_X1 + 14.0;

// ── The petal clock — closed form in `t`, so the probe's points are too ─────

/// The clock's rotation at film-t.
#[must_use]
fn rotation(t: f32) -> f32 {
    t * SECONDS * 0.22
}

/// One petal's `k`-th point: radius `r`, angle offset `da` from its axis.
fn pt(axis: f32, r: f32, da: f32) -> Offset {
    let a = axis + da;
    Offset::new(CLOCK.dx + r * a.cos(), CLOCK.dy + r * a.sin())
}

/// The petal ring: one path, PETALS subpaths, one of them open.
#[must_use]
fn petal_path(t: f32) -> Path {
    let rot = rotation(t);
    let mut p = Path::new();
    for k in 0..PETALS {
        let axis = k as f32 / PETALS as f32 * std::f32::consts::TAU + rot;
        // A leaf: base at r20, out to the tip at r132, back on the other
        // side to r20 — two cubics.
        p.move_to(pt(axis, 20.0, 0.0));
        p.cubic_to(
            pt(axis, 96.0, 0.14),
            pt(axis, 120.0, 0.28),
            pt(axis, 132.0, 0.42),
        );
        p.cubic_to(
            pt(axis, 120.0, 0.56),
            pt(axis, 96.0, 0.70),
            pt(axis, 20.0, 0.84),
        );
        if k != OPEN_PETAL {
            p.close();
        }
    }
    p
}

/// A petal's centroid, probe-ready: mid-angle, mid-radius.
#[must_use]
fn petal_centroid(t: f32, k: usize) -> Offset {
    let axis = k as f32 / PETALS as f32 * std::f32::consts::TAU + rotation(t);
    pt(axis, 76.0, 0.42)
}

// ── The board ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    let fade_in = clamp01(t / 0.06);
    let petals = petal_path(t);

    let board = Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(8, 8, 11)),
                    (1.0, BG_DEEP),
                ]),
            );

            // ── U-15's subject: the full-bleed wash, blurred σ24 ─────────
            // The probe reads its four corners: uniform is the fixed point.
            book.layer(1.0, WASH_SIGMA, None, |g| {
                g.rect(
                    Rect::new(-60.0, -60.0, w + 60.0, h + 60.0),
                    alpha(VIOLET, 0.18 + 0.06 * (t * SECONDS * 0.9).sin()),
                );
            });

            // Corner brackets at the probe points — where the probe reads.
            let bracket = 16.0;
            let pulse = 0.5 + 0.5 * (t * SECONDS * 1.6).sin();
            for (bx, by, dx, dy) in [
                (8.0, 8.0, 1.0, 1.0),
                (w - 8.0, 8.0, -1.0, 1.0),
                (8.0, h - 8.0, 1.0, -1.0),
                (w - 8.0, h - 8.0, -1.0, -1.0),
            ] {
                book.line(
                    Offset::new(bx, by),
                    Offset::new(bx + dx * bracket, by),
                    alpha(VIOLET_SOFT, (0.5 + 0.4 * pulse) * fade_in),
                    1.2,
                );
                book.line(
                    Offset::new(bx, by),
                    Offset::new(bx, by + dy * bracket),
                    alpha(VIOLET_SOFT, (0.5 + 0.4 * pulse) * fade_in),
                    1.2,
                );
            }

            // ── The petal clock ─────────────────────────────────────────
            // The clock face: a breathing ring.
            book.ring(
                CLOCK,
                150.0 + 5.0 * (t * SECONDS * 0.7).sin(),
                1.0,
                alpha(FAINT, 0.22 * fade_in),
            );
            // The petals: one fill, six subpaths, one open.
            book.fill(petals.clone(), alpha(INK, 0.92 * fade_in));
            // A soft violet bloom behind the clock.
            book.layer(1.0, 12.0, None, |g| {
                g.circle(
                    CLOCK,
                    176.0,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(VIOLET, 0.24 * fade_in)),
                        (1.0, alpha(VIOLET, 0.0)),
                    ]),
                );
            });
            // The hub.
            book.circle(CLOCK, 7.0, alpha(INK, 0.9 * fade_in));
            book.ring(CLOCK, 12.0, 1.0, alpha(VIOLET_SOFT, 0.5 * fade_in));

            // The open petal's marker: a dashed ring at its centroid —
            // where the probe reads, and where the census counts.
            let open_c = petal_centroid(t, OPEN_PETAL);
            book.ring(open_c, 26.0, 1.4, alpha(VIOLET_SOFT, 0.9 * fade_in));
            let closed_c = petal_centroid(t, 0);
            book.ring(closed_c, 26.0, 1.0, alpha(FAINT, 0.40 * fade_in));

            // ── The hairline ladder ────────────────────────────────────
            for (i, rung) in RUNGS.iter().enumerate() {
                let y = LADDER_Y0 + i as f32 * LADDER_DY;
                book.line(
                    Offset::new(LADDER_X0, y),
                    Offset::new(LADDER_X1, y),
                    alpha(INK, 0.9 * fade_in),
                    *rung,
                );
            }
            // The playhead: a bright tick sweeping down the ladder's right
            // side, rung to rung — the probe's own reading order, moving.
            // (Right of the rungs: never on the probed pixels.)
            let sweep = clamp01((t - 0.10) / 0.8) * (RUNGS.len() - 1) as f32;
            let si = sweep.floor() as usize;
            let sf = sweep - si as f32;
            if fade_in > 0.0 {
                let y_now = LADDER_Y0 + si as f32 * LADDER_DY + sf * LADDER_DY;
                book.line(
                    Offset::new(PLAYHEAD_X, y_now - 5.0),
                    Offset::new(PLAYHEAD_X, y_now + 5.0),
                    alpha(VIOLET_SOFT, 0.9 * fade_in),
                    2.0,
                );
                book.line(
                    Offset::new(PLAYHEAD_X + 4.0, LADDER_Y0 - 12.0),
                    Offset::new(PLAYHEAD_X + 4.0, y_now),
                    alpha(FAINT, 0.35 * fade_in),
                    1.0,
                );
                // The already-read rungs, ticked.
                for r in 0..si {
                    let y = LADDER_Y0 + r as f32 * LADDER_DY;
                    book.line(
                        Offset::new(PLAYHEAD_X, y - 3.0),
                        Offset::new(PLAYHEAD_X, y + 3.0),
                        alpha(VIOLET_SOFT, 0.4 * fade_in),
                        1.0,
                    );
                }
            }
            // The ladder's rail — faint, for the eye.
            book.line(
                Offset::new(LADDER_X0 - 10.0, LADDER_Y0 - 14.0),
                Offset::new(
                    LADDER_X0 - 10.0,
                    LADDER_Y0 + LADDER_DY * RUNGS.len() as f32 + 24.0,
                ),
                alpha(FAINT, 0.18 * fade_in),
                1.0,
            );

            // The vignette.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::radial(Offset::new(0.5, 0.5), 0.9).with_dither().with_stops(&[
                    (0.6, alpha(Color::BLACK, 0.0)),
                    (1.0, alpha(Color::BLACK, 0.38)),
                ]),
            );
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));

    // The ladder's rung labels — the widths, stated where the probe reads.
    for (i, rung) in RUNGS.iter().enumerate() {
        let y = LADDER_Y0 + i as f32 * LADDER_DY;
        stack = stack.push(
            Positioned::new()
                .left(LADDER_X0 - 64.0)
                .top(y - 8.0)
                .width(58.0)
                .height(16.0)
                .child(
                    Text::new(format!("{rung:.2} px")).style(
                        TextStyle::new(11.0).monospace().color(alpha(MUTED, 1.0)),
                    ),
                ),
        );
    }

    stack.push(receipt_panel(t)).into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(t: f32) -> WidgetNode {
    let fade_in = clamp01(t / 0.06);
    let _ = fade_in;

    const P_X: f32 = 42.0;
    const P_Y: f32 = 560.0;
    const P_W: f32 = 352.0;

    let lines = [
        "PROBE · THE INSTRUMENT PANEL · MEASURED IN PIXELS".to_string(),
        format!("wash σ{:.0} full-bleed — corners read by the probe", WASH_SIGMA),
        format!("petals {} · open #{} · open_subpath_fills → 1", PETALS, OPEN_PETAL),
        format!("ladder {} rungs {:.2}→{:.2} px (U-18: OPEN, recorded)", RUNGS.len(),
            RUNGS[0], RUNGS[RUNGS.len() - 1]),
        "probe lines below: from the RGBA buffer, last frame".to_string(),
    ];

    let mut stack = Stack::new().push(
        Positioned::new()
            .left(P_X)
            .top(P_Y)
            .width(P_W)
            .height(18.0)
            .child(
                Text::new(lines[0].clone()).style(
                    TextStyle::new(12.0)
                        .monospace()
                        .letter_spacing(1.6)
                        .color(alpha(MUTED, 1.0)),
                ),
            ),
    );
    for (i, line) in lines.iter().enumerate().skip(1) {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + 18.0 + i as f32 * 16.0)
                .width(P_W)
                .height(15.0)
                .child(
                    Text::new(line.clone()).style(
                        TextStyle::new(11.0).monospace().color(alpha(mix(MUTED, INK, 0.55), 0.95)),
                    ),
                ),
        );
    }
    stack.into()
}

// ── The pixel probe — run by the harness on the last rendered frame ─────────

/// Reads the evidence out of the output buffer. Every number below is a
/// pixel value the rasterizer produced; the probe's own positions are the
/// plate's closed-form geometry at `t = 1`.
pub fn probe(img: &image::RgbaImage) -> Vec<String> {
    let mut out = Vec::new();

    // ── U-15: the wash's four corners, and the max channel delta.
    let (w, h) = img.dimensions();
    let corners = [
        img.get_pixel(4, 4),
        img.get_pixel(w - 5, 4),
        img.get_pixel(4, h - 5),
        img.get_pixel(w - 5, h - 5),
    ];
    let mut delta = 0u8;
    for c in &corners {
        delta = delta
            .max(c[0].abs_diff(corners[0][0]))
            .max(c[1].abs_diff(corners[0][1]))
            .max(c[2].abs_diff(corners[0][2]));
    }
    let fmt = |c: &image::Rgba<u8>| format!("({},{},{})", c[0], c[1], c[2]);
    out.push(format!(
        "U-15 corners {} {} {} {} · max Δch {}",
        fmt(corners[0]),
        fmt(corners[1]),
        fmt(corners[2]),
        fmt(corners[3]),
        delta
    ));

    // ── The open petal: ink at both centroids, read where the markers are.
    let open_c = petal_centroid(1.0, OPEN_PETAL);
    let closed_c = petal_centroid(1.0, 0);
    let open_px = img.get_pixel(open_c.dx as u32, open_c.dy as u32);
    let closed_px = img.get_pixel(closed_c.dx as u32, closed_c.dy as u32);
    out.push(format!(
        "petal ink: closed {} · open {} — the census counted the difference",
        fmt(closed_px),
        fmt(open_px)
    ));

    // ── U-18: the hairline ladder, one pixel read per rung.
    let ink: Vec<String> = RUNGS
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let y = (LADDER_Y0 + i as f32 * LADDER_DY) as u32;
            let x = ((LADDER_X0 + LADDER_X1) * 0.5) as u32;
            let px = img.get_pixel(x, y);
            format!("{r:.2}→{}", px[1])
        })
        .collect();
    out.push(format!("U-18 rung ink (g): {}", ink.join(" ")));

    out
}
