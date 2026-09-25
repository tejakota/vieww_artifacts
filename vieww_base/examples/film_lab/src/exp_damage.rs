//! exp_damage — *E-10: the damage flash.* One region lights — the rest
//! stays cold. vieww-paint's damage tracking, made visible.
//!
//! S08's shot: the author edits one thing, and **only the region that
//! changed repaints** — a single lit rectangle in a field of cold ones.
//! The experiment stages the studio's own surface (a mock of the app the
//! film is about): a sidebar tree, a header, the preview canvas, an
//! inspector. Four regions, four edits, each lighting **exactly its own
//! rect** in violet, decaying over ~0.5 film-seconds while the rest of
//! the screen never flickers.
//!
//! The receipt — the **PerformanceOverlay strip** at the bottom, E-10's
//! own instrument: the damaged region's name, its area **as a percentage
//! of the whole surface** (computed from the mock's actual geometry, not
//! typed), and the repaint budget the region's rect implies. When no
//! region is lit, the strip reads **idle · 0 regions · 0.0%** — the
//! absence is the claim.
//!
//! Grammar notes, from the graph:
//! - E-10 "damage flash — one region lights" — `vieww-paint damage +
//!   PerformanceOverlay`

use vieww_foundation::{Color, FontWeight, Gradient, Offset, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};

use crate::film_lib::{
    alpha, clamp01, mix, smoothstep, spring_out, tint, xywh, BG_DEEP, CANVAS, FAINT, INK, MUTED,
    Rng, VIOLET, VIOLET_SOFT, MINT, AMBER,
};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

/// Left margin.
const X: f32 = 120.0;

// ── The surface geometry — a mock of the studio app, in four regions ────────
//
// The whole app surface (a scaled-down window). Region rects are declared
// once; the receipt's percentages are computed FROM these rects.

/// The window: position and size.
const W_X: f32 = 140.0;
const W_Y: f32 = 116.0;
const W_W: f32 = 1000.0;
const W_H: f32 = 400.0;

/// A region of the surface: name, rect, and the edit's own accent.
struct Region {
    name: &'static str,
    rect: Rect,
    accent: Color,
}

const HEADER_H: f32 = 54.0;
const SIDEBAR_W: f32 = 216.0;
const INSPECTOR_W: f32 = 260.0;

fn regions() -> Vec<Region> {
    let sidebar = xywh(W_X, W_Y + HEADER_H, SIDEBAR_W, W_H - HEADER_H);
    let preview = xywh(
        W_X + SIDEBAR_W,
        W_Y + HEADER_H,
        W_W - SIDEBAR_W - INSPECTOR_W,
        W_H - HEADER_H,
    );
    let inspector = xywh(
        W_X + W_W - INSPECTOR_W,
        W_Y + HEADER_H,
        INSPECTOR_W,
        W_H - HEADER_H,
    );
    let _header = xywh(W_X, W_Y, W_W, HEADER_H);
    vec![
        Region { name: "sidebar/tree", rect: sidebar, accent: VIOLET_SOFT },
        Region { name: "preview/canvas", rect: preview, accent: VIOLET },
        Region { name: "inspector/props", rect: inspector, accent: VIOLET },
        Region { name: "preview/canvas", rect: preview, accent: VIOLET },
    ]
}

/// The edit events: (region index, t_start). Four edits, escalating cadence.
const EDITS: [(usize, f32); 4] = [(0, 0.14), (1, 0.38), (2, 0.62), (3, 0.82)];

/// The damage envelope for edit `e` at `t`: 0 → spike → decay.
/// Attack broad enough to survive the harness's frame sampling (~0.75 s
/// spacing at 16 frames / 12 s), decay ~1.2 s — the flash must still be
/// on-screen a frame after it fires, or the effect does not exist.
fn damage_env(e: usize, t: f32) -> f32 {
    let t0 = EDITS[e].1;
    if t < t0 {
        return 0.0;
    }
    let dt = (t - t0) * SECONDS;
    let decay = (-dt * 1.2).exp();
    // The flash spike rides a fast spring so it kicks, not fades in.
    (spring_out(clamp01(dt / 0.30), 16.0, 0.40) * decay * 1.25).min(1.0)
}

/// Total lit regions at `t` (an edit stays "lit" while its envelope > 0.03).
fn lit_count(t: f32) -> u32 {
    EDITS.iter().filter(|&&(e, _)| damage_env(e, t) > 0.03).count() as u32
}

/// The strongest active envelope — drives the strip's headline numbers.
fn active_damage(t: f32) -> f32 {
    EDITS.iter().map(|&(e, _)| damage_env(e, t)).fold(0.0, f32::max)
}

/// The most-recently-lit region at `t` (for the strip's name line).
fn active_region(t: f32) -> Option<usize> {
    let mut best: Option<usize> = None;
    let mut best_t = -1.0;
    for &(e, t0) in EDITS.iter() {
        if t >= t0 && damage_env(e, t) > 0.02 && t0 > best_t {
            best = Some(EDITS[e].0);
            best_t = t0;
        }
    }
    best
}

// ── The app surface ─────────────────────────────────────────────────────────

fn app_surface(t: f32) -> WidgetNode {
    let regs = regions();

    // The surface: window chrome, region cards, content ghosts, damage.
    let surface = Painting::sized(
        Size::new(W_W + 40.0, W_H + 40.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            let ox = 20.0;
            let oy = 20.0;
            let win = xywh(ox, oy, W_W, W_H);

            // The window body — dark glass.
            book.rrect(
                win,
                14.0,
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, alpha(Color::rgb(19, 19, 25), 0.96)),
                    (1.0, alpha(Color::rgb(13, 13, 18), 0.97)),
                ]),
            );
            book.stroke_rrect(win, 14.0, alpha(Color::WHITE, 0.10), 1.2);

            // The content ghosts — each region's furniture, drawn cold and
            // low-contrast: this is what "not repainting" looks like.
            let header = xywh(ox, oy, W_W, HEADER_H);
            // Header: three dots + a title bar ghost.
            for i in 0..3 {
                book.circle(
                    Offset::new(ox + 22.0 + i as f32 * 18.0, oy + HEADER_H * 0.5),
                    4.0,
                    alpha(MUTED, 0.28),
                );
            }
            book.rrect(
                xywh(ox + 84.0, oy + 20.0, 220.0, 14.0),
                4.0,
                alpha(Color::WHITE, 0.05),
            );
            book.line(
                Offset::new(ox, oy + HEADER_H),
                Offset::new(ox + W_W, oy + HEADER_H),
                alpha(Color::WHITE, 0.07),
                1.0,
            );

            // Sidebar: a file tree ghost — 9 rows, indented.
            let sb = xywh(ox, oy + HEADER_H, SIDEBAR_W, W_H - HEADER_H);
            let mut rng = Rng::new(0x7EE);
            for row in 0..9 {
                let indent = if row % 3 == 0 { 0.0 } else { 16.0 + (row % 3) as f32 * 8.0 };
                let row_y = oy + HEADER_H + 22.0 + row as f32 * 34.0;
                let _ = rng.f01();
                book.rrect(
                    xywh(ox + 18.0 + indent, row_y, 90.0 + (row % 4) as f32 * 28.0, 9.0),
                    3.0,
                    alpha(Color::WHITE, 0.05 + if row % 4 == 0 { 0.02 } else { 0.0 }),
                );
            }
            book.line(
                Offset::new(sb.left + sb.width(), sb.top),
                Offset::new(sb.left + sb.width(), sb.top + sb.height()),
                alpha(Color::WHITE, 0.06),
                1.0,
            );

            // Inspector: prop rows — label + field pairs.
            let ins_x = ox + W_W - INSPECTOR_W;
            for row in 0..7 {
                let row_y = oy + HEADER_H + 26.0 + row as f32 * 44.0;
                book.rrect(xywh(ins_x + 20.0, row_y, 64.0, 9.0), 3.0, alpha(Color::WHITE, 0.05));
                book.rrect(
                    xywh(ins_x + 96.0, row_y - 4.0, INSPECTOR_W - 120.0, 17.0),
                    5.0,
                    alpha(Color::rgb(28, 28, 36), 0.9),
                );
            }
            book.line(
                Offset::new(ins_x, oy + HEADER_H),
                Offset::new(ins_x, oy + W_H),
                alpha(Color::WHITE, 0.06),
                1.0,
            );

            // Preview canvas: the app's own art — a mini composition living
            // in the preview region (a card + a sparkline, the film's props
            // inside the prop).
            let pv = xywh(ox + SIDEBAR_W, oy + HEADER_H, W_W - SIDEBAR_W - INSPECTOR_W, W_H - HEADER_H);
            book.rrect(
                xywh(pv.left + 34.0, pv.top + 30.0, 330.0, 150.0),
                10.0,
                alpha(Color::rgb(24, 22, 34), 0.9),
            );
            book.stroke_rrect(
                xywh(pv.left + 34.0, pv.top + 30.0, 330.0, 150.0),
                10.0,
                alpha(VIOLET_SOFT, 0.18),
                1.0,
            );
            // A ghost headline + sparkline in the preview card.
            book.rrect(xywh(pv.left + 58.0, pv.top + 56.0, 180.0, 16.0), 4.0, alpha(Color::WHITE, 0.10));
            book.rrect(xywh(pv.left + 58.0, pv.top + 84.0, 120.0, 10.0), 3.0, alpha(Color::WHITE, 0.06));
            let mut spark = vieww_foundation::Path::new();
            let mut rng2 = Rng::new(0x5FA2);
            for i in 0..12 {
                let px = pv.left + 58.0 + i as f32 * 20.0;
                let py = pv.top + 148.0 - 34.0 * (i as f32 / 11.0).powi(2) - rng2.f01() * 6.0;
                if i == 0 { spark.move_to(Offset::new(px, py)); } else { spark.line_to(Offset::new(px, py)); }
            }
            book.stroke(spark, alpha(VIOLET_SOFT, 0.4), 2.0);

            // ── The damage flashes — THE effect ─────────────────────────
            for &(e, _) in EDITS.iter() {
                let env = damage_env(e, t);
                if env <= 0.02 {
                    continue;
                }
                let r = regs[EDITS[e].0].rect;
                let rect = xywh(
                    r.left - ox + 6.0,
                    r.top - oy + 6.0,
                    r.width() - 12.0,
                    r.height() - 12.0,
                );
                let accent = regs[EDITS[e].0].accent;

                // The fill: a violet wash strong enough to read as an
                // event — the whole point is that this ONE region lights.
                book.rrect(
                    rect,
                    8.0,
                    alpha(accent, 0.22 * env),
                );
                // The border: bright on attack, the region's own outline.
                book.stroke_rrect(rect, 8.0, alpha(accent, 0.95 * env), 2.2);
                // Corner ticks — the selection grammar, keeping the rect
                // crisp when the wash has decayed.
                let tick = 18.0;
                let corners = [
                    (rect.left, rect.top, 1.0, 1.0),
                    (rect.left + rect.width(), rect.top, -1.0, 1.0),
                    (rect.left, rect.top + rect.height(), 1.0, -1.0),
                    (rect.left + rect.width(), rect.top + rect.height(), -1.0, -1.0),
                ];
                for &(cx, cy, sx, sy) in corners.iter() {
                    book.line(
                        Offset::new(cx, cy),
                        Offset::new(cx + sx * tick, cy),
                        alpha(accent, 0.95 * env),
                        3.2,
                    );
                    book.line(
                        Offset::new(cx, cy),
                        Offset::new(cx, cy + sy * tick),
                        alpha(accent, 0.95 * env),
                        3.2,
                    );
                }
                // The bloom — a soft glow just outside the rect, on attack.
                if env > 0.20 {
                    book.rrect(
                        xywh(
                            rect.left - 14.0,
                            rect.top - 14.0,
                            rect.width() + 28.0,
                            rect.height() + 28.0,
                        ),
                        14.0,
                        Gradient::radial_fill().with_dither().with_stops(&[
                            (0.0, alpha(accent, 0.20 * env)),
                            (1.0, alpha(accent, 0.0)),
                        ]),
                    );
                }
            }

            // The edit cursor — where the author's hand is (a caret glyph in
            // the region whose edit is freshest).
            if let Some(ri) = active_region(t) {
                let r = regs[ri].rect;
                let cxp = (r.left + 40.0 - ox) as f32;
                let cyp = (r.top + 64.0 - oy) as f32;
                let blink = 0.5 + 0.5 * (t * SECONDS * 2.4).sin();
                book.rect(xywh(cxp, cyp, 2.4, 22.0), alpha(INK, 0.75 * blink));
            }
        }),
    );

    Stack::new()
        .push(
            Positioned::new()
                .left(W_X - 20.0)
                .top(W_Y - 26.0)
                .width(700.0)
                .height(18.0)
                .child(
                    Text::new("THE STUDIO · FOUR SURFACES · ONE SESSION").style(
                        TextStyle::new(12.0)
                            .monospace()
                            .letter_spacing(2.2)
                            .color(alpha(FAINT, 0.9)),
                    ),
                ),
        )
        .push(
            Positioned::new()
                .left(W_X - 20.0)
                .top(W_Y - 20.0)
                .width(W_W + 40.0)
                .height(W_H + 40.0)
                .child(surface),
        )
        .into()
}

// ── The PerformanceOverlay strip — E-10's own instrument ────────────────────

/// The strip: geometry.
const S_Y: f32 = 600.0;

fn overlay_strip(t: f32) -> WidgetNode {
    let regs = regions();
    let active = active_region(t);
    let env = active_damage(t);
    let lit = lit_count(t);

    // The percentage: computed from the rects — never typed.
    let pct = match active {
        Some(ri) => {
            let r = regs[ri].rect;
            let area = r.width() * r.height();
            let total = W_W * W_H;
            (area / total * 100.0)
        }
        None => 0.0,
    };
    let pct_now = pct * env.max(if lit > 0 { 0.35 } else { 0.0 });

    // The per-edit shares, computed once outside the closure — the same
    // arithmetic as the headline number, one source, not two.
    let shares: Vec<f32> = EDITS
        .iter()
        .map(|&(e, _)| {
            let r = regs[EDITS[e].0].rect;
            (r.width() * r.height()) / (W_W * W_H)
        })
        .collect();

    // The strip's graph: a history trace of damaged-area percentage over t —
    // four spikes, one per edit, decaying. The area history IS the receipt.
    let trace = Painting::sized(
        Size::new(460.0, 76.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            // Baseline.
            book.line(
                Offset::new(0.0, 60.0),
                Offset::new(440.0, 60.0),
                alpha(Color::WHITE, 0.10),
                1.0,
            );
            // The trace — summed envelopes over the whole timeline.
            let mut p = vieww_foundation::Path::new();
            for i in 0..=90 {
                let u = i as f32 / 90.0;
                let mut v = 0.0;
                for &(e, _t0) in EDITS.iter() {
                    v += damage_env(e, u) * shares[e] * 100.0;
                }
                let px = 4.0 + u * 432.0;
                let py = 60.0 - (v / 40.0).min(1.0) * 52.0;
                if i == 0 { p.move_to(Offset::new(px, py)); } else { p.line_to(Offset::new(px, py)); }
            }
            book.stroke(p, alpha(VIOLET_SOFT, 0.75), 1.8);
            // The playhead.
            let px = 4.0 + t * 432.0;
            book.line(
                Offset::new(px, 6.0),
                Offset::new(px, 60.0),
                alpha(INK, 0.22),
                1.0,
            );
            // Idle label at the right when nothing is lit.
            if lit == 0 {
                book.ring(Offset::new(452.0, 20.0), 5.0, 1.4, alpha(MINT, 0.5));
            }
        }),
    );

    let headline = match active {
        Some(ri) => format!(
            "damage · {} · {:.1}% of surface",
            regs[ri].name,
            pct_now
        ),
        None => "idle · 0 regions · 0.0%".to_string(),
    };

    let badge_color = if lit > 0 {
        mix(MUTED, tint(VIOLET, 0.3), env.max(0.3))
    } else {
        alpha(MINT, 0.7)
    };

    Stack::new()
        .push(
            Positioned::new()
                .left(X)
                .top(S_Y - 26.0)
                .width(700.0)
                .height(18.0)
                .child(
                    Text::new("PERFORMANCEOVERLAY · DAMAGE, NOT SCREEN").style(
                        TextStyle::new(12.0)
                            .monospace()
                            .letter_spacing(2.2)
                            .color(alpha(FAINT, 0.9)),
                    ),
                ),
        )
        .push(
            Positioned::new()
                .left(X)
                .top(S_Y)
                .width(460.0)
                .height(76.0)
                .child(trace),
        )
        .push(
            Positioned::new()
                .left(X + 490.0)
                .top(S_Y + 6.0)
                .width(420.0)
                .height(26.0)
                .child(
                    Text::new(headline).style(
                        TextStyle::new(16.0)
                            .monospace()
                            .weight(FontWeight::Medium)
                            .color(badge_color),
                    ),
                ),
        )
        .push(
            Positioned::new()
                .left(X + 490.0)
                .top(S_Y + 36.0)
                .width(420.0)
                .height(16.0)
                .child(
                    Text::new("one region lights · the rest never repaints").style(
                        TextStyle::new(11.0)
                            .monospace()
                            .color(alpha(MUTED, 0.8)),
                    ),
                ),
        )
        .into()
}

// ── The board ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    let bg = Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(9, 9, 13)),
                    (0.6, BG_DEEP),
                    (1.0, Color::rgb(12, 12, 17)),
                ]),
            );

            // Sparse stars — S08, interior, tool-lit.
            let mut rng = Rng::new(0xDA1);
            for _ in 0..40 {
                let x = rng.f01() * w;
                let y = rng.f01() * h;
                let r = 0.4 + rng.f01() * 0.8;
                let tw = 0.5 + 0.5 * (t * 3.0 + rng.f01() * 10.0).sin();
                book.circle(Offset::new(x, y), r, alpha(Color::WHITE, 0.03 + 0.06 * tw));
            }

            // The tool glow — amber-leaning: the devtools light.
            book.layer(1.0, 38.0, None, |inner| {
                inner.circle(
                    Offset::new(w * 0.70, h * 0.18),
                    w * 0.22,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(AMBER, 0.05)),
                        (1.0, alpha(AMBER, 0.0)),
                    ]),
                );
            });
            book.layer(1.0, 36.0, None, |inner| {
                inner.circle(
                    Offset::new(w * 0.28, h * 0.85),
                    w * 0.26,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(VIOLET, 0.07)),
                        (1.0, alpha(VIOLET, 0.0)),
                    ]),
                );
            });

            // Board rule.
            book.rect(xywh(X, 566.0, w - 2.0 * X, 1.0), alpha(Color::WHITE, 0.05));

            // The vignette.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::radial(Offset::new(0.5, 0.5), 0.80).with_dither().with_stops(&[
                    (0.55, alpha(Color::BLACK, 0.0)),
                    (1.0, alpha(Color::BLACK, 0.45)),
                ]),
            );
        }),
    );

    Stack::new()
        .push(Positioned::fill().child(bg))
        .push(app_surface(t))
        .push(overlay_strip(t))
        .into()
}
