//! Movement IV · THE PROOF — real metrics, every number a receipt.
//!
//! One scene, fourteen seconds, **three registers** the camera falls
//! through in order. The v2 cut put seven 16 px rows in the middle of a
//! 1920×1080 frame, filled them in the first second and a half, then
//! held them for twelve more: a table, not a scene. This cut spends the
//! fourteen seconds:
//!
//! * **A · the engine's receipts** (0–4.5 s) — three hero cards, the
//!   numbers at the size the claim deserves, counted up to rest.
//! * **B · the frame budget** (4.0–9.5 s) — the one metric that decides
//!   whether a UI feels alive, drawn as a gauge against the 60 fps
//!   allowance. Both bars stop short of the line. That gap *is* the
//!   argument, and it is the scene's emotional beat: relief, measured.
//! * **C · this film audited itself** (9.0–14 s) — the census's own
//!   numbers, the only row in the film that is measured rather than
//!   quoted, with the bench identity under it.
//!
//! Nothing here is typed. `CERT_*` are the repository's certification
//! receipts; the last register is read from the manifest the census
//! wrote in pass 1.

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextAlign};
use vieww_widget::prelude::*;

use crate::film_lib::{ease_out_cubic, ease_out_expo};
use crate::product_film as pf;
use super::filmkit as fk;
use super::{ACCENT, ACCENT_DEEP, BG_DEEP, BRAND_FAR, BRAND_NEAR, CANVAS, INK, LEDGER, MUTED, SYN_TYPE, W};

/// The 60 fps allowance, in milliseconds — the line every frame-time
/// bar in register B is read against.
const BUDGET_MS: f32 = 16.7;

/// The gauge's full scale, in milliseconds. Chosen so the budget line
/// sits at 70% of the width: the bars have somewhere to fail to reach.
const GAUGE_MS: f32 = 24.0;

/// Register A's three hero cards — (value, unit, label, source colour).
fn hero_cards() -> [(String, &'static str, &'static str, Color); 3] {
    [
        (
            pf::group_commas(pf::CERT_TESTS),
            "",
            "automated tests, all passing",
            SYN_TYPE,
        ),
        (
            format!("{}", pf::CERT_CRATES),
            "",
            "building blocks, one engine",
            BRAND_NEAR,
        ),
        (
            format!("{}", pf::CERT_ALLOCS_STEADY),
            "",
            "memory hiccups while animating",
            LEDGER,
        ),
    ]
}

/// The rows the legacy ledger carried, kept as the scene's small print
/// so nothing the audit says is dropped from the film.
pub fn rows(probe: &pf::Probe) -> Vec<(String, String, &'static str)> {
    vec![
        ("certification tests".into(), pf::group_commas(pf::CERT_TESTS), "audit"),
        ("crates in the workspace".into(), format!("{}", pf::CERT_CRATES), "audit"),
        ("cold start".into(), format!("{:.1} ms", pf::CERT_STARTUP_MS), "audit"),
        ("p95 frame".into(), format!("{:.1} ms", pf::CERT_P95_MS), "audit"),
        ("worst frame".into(), format!("{:.1} ms", pf::CERT_WORST_MS), "audit"),
        (
            format!("steady-state allocs · {} frames", pf::CERT_FRAMES_STEADY),
            format!("{}", pf::CERT_ALLOCS_STEADY),
            "audit",
        ),
        (
            format!("this film · median {:.0} ms", probe.frame_ms),
            format!("{} frames", pf::group_commas(probe.frames)),
            "this film",
        ),
    ]
}

/// Count a decimal value up to rest — the measured numbers arrive the
/// way a meter settles, not the way a label appears.
fn count_f(target: f32, p: f32) -> f32 {
    target * ease_out_expo(p.clamp(0.0, 1.0))
}

pub fn the_ledger(ctx: &pf::Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let probe = ctx.probe;

    let mut stack = Stack::new();

    // ── The room ────────────────────────────────────────────────────
    //
    // The audit's register: flat, dark, exact, with one flowing rail
    // down the left edge — receipts arriving for as long as the audit
    // speaks — and a hairline grid that only exists where numbers are.
    let rail_a = 0.55 * pf::clamp01(t / 0.16);
    let rail_t = t * 8.0;
    super::frame::ground(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            let _ = (w, h);
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(15, 13, 18)),
                    (0.55, BG_DEEP),
                    (1.0, Color::rgb(11, 10, 14)),
                ]),
            );
            pf::vignette(book, w, h, 0.52);
        }),
    )));
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            let _ = (w, h, &s);

            let _ = (rail_a, rail_t);

        }),
    )));

    // ── Register A · the engine's receipts ──────────────────────────
    //
    // Three hero cards. Each clears its own plate, then puts one number
    // on it at the size the claim deserves — 96 px, not 28.
    let cards = hero_cards();
    let card_w = 476.0;
    let gap = 34.0;
    let x0 = (W - (card_w * 3.0 + gap * 2.0)) * 0.5;
    for (i, (value, unit, label, color)) in cards.iter().enumerate() {
        let t0 = 0.04 + i as f32 * 0.055;
        let p = pf::clamp01((t - t0) / 0.13);
        if p <= 0.01 {
            continue;
        }
        let x = x0 + i as f32 * (card_w + gap);
        let rect = Rect::new(x, 300.0, x + card_w, 300.0 + 212.0);
        let color = *color;
        // The card's own sheen, one pass, staggered behind the landing.
        let sheen_p = pf::clamp01((t - t0 - 0.09) / 0.22);
        stack = stack.push(Positioned::fill().child(Painting::sized(
            CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                fk::stage_plate(book, rect, p, color, 1.0);
                if p > 0.85 {
                    fk::plate_sheen(book, rect, sheen_p, 1.0);
                }
            }),
        )));

        // The numeral — counted up, landing on the receipt's real value.
        let roll = pf::clamp01((t - t0 - 0.02) / 0.22);
        let shown = if roll >= 1.0 {
            value.clone()
        } else {
            let target: u64 = value.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0);
            pf::group_commas(pf::count_up(target, roll))
        };
        let text_a = ease_out_cubic(pf::clamp01((p - 0.35) / 0.65));
        if text_a > 0.01 {
            stack = stack.push(
                Positioned::new()
                    .left(x)
                    .top(326.0 + (1.0 - text_a) * 10.0)
                    .width(card_w)
                    .height(120.0)
                    .child(Opacity::new(text_a).child(
                        Text::new(format!("{shown}{unit}"))
                            .style(pf::geist(88.0).bold().letter_spacing(-1.5).color(pf::alpha(INK, 0.98)))
                            .align(TextAlign::Center),
                    )),
            );
            stack = stack.push(
                Positioned::new()
                    .left(x)
                    .top(438.0)
                    .width(card_w)
                    .height(32.0)
                    .child(Opacity::new(text_a).child(
                        Text::new(label.to_string())
                            .style(pf::geist_mono(21.0).letter_spacing(1.0).color(pf::alpha(color, 0.95)))
                            .align(TextAlign::Center),
                    )),
            );
            stack = stack.push(
                Positioned::new()
                    .left(x)
                    .top(474.0)
                    .width(card_w)
                    .height(24.0)
                    .child(Opacity::new(text_a * 0.8).child(
                        Text::new("audit".to_string())
                            .style(pf::geist_mono(15.0).letter_spacing(2.4).color(pf::alpha(MUTED, 0.8)))
                            .align(TextAlign::Center),
                    )),
            );
        }
    }

    // ── Register B · the frame budget ───────────────────────────────
    //
    // The gauge. Two measured frame times against the 60 fps allowance,
    // drawn at the same scale so the gap is a length and not a claim.
    let gauge_p = pf::clamp01((t - 0.22) / 0.11);
    if gauge_p > 0.01 {
        let gx0 = 330.0;
        let gx1 = 1700.0;
        let plate = Rect::new(212.0, 542.0, 1708.0, 542.0 + 256.0);
        let bars: [(&str, f32, f32); 2] = [
            ("typical frame", pf::CERT_P95_MS, 0.30),
            ("slowest frame", pf::CERT_WORST_MS, 0.37),
        ];
        let ga = gauge_p;
        stack = stack.push(Positioned::fill().child(Painting::sized(
            CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                fk::stage_plate(book, plate, gauge_p, ACCENT, 1.0);
                if gauge_p < 0.999 {
                    return;
                }
                for (k, (_, value, t0)) in bars.iter().enumerate() {
                    let p = pf::clamp01((t - t0) / 0.16);
                    if p <= 0.01 {
                        continue;
                    }
                    let y = 630.0 + k as f32 * 64.0;
                    fk::budget_bar(
                        book,
                        Rect::new(gx0, y, gx1 - 150.0, y + 26.0),
                        *value,
                        BUDGET_MS,
                        GAUGE_MS,
                        p,
                        LEDGER,
                        pf::BREAK_RED,
                        ga,
                    );
                }
                // The scale under the bars, and the budget's own line
                // carried the full height of the gauge.
                let ticks_a = pf::clamp01((t - 0.29) / 0.10);
                fk::gauge_ticks(book, gx0, gx1 - 150.0, 728.0, 12, ticks_a * ga);
                let bx = gx0 + (gx1 - 150.0 - gx0) * (BUDGET_MS / GAUGE_MS);
                let line_p = ease_out_expo(pf::clamp01((t - 0.25) / 0.12));
                if line_p > 0.01 {
                    book.rect(
                        Rect::new(bx - 1.0, 606.0, bx + 1.0, 606.0 + 122.0 * line_p),
                        pf::alpha(Color::WHITE, 0.38 * ga),
                    );
                }
            }),
        )));

        // The gauge's title and the bars' labels — widgets, so the type
        // is shaped by the same engine that shapes the app's.
        let title_a = ease_out_cubic(pf::clamp01((gauge_p - 0.4) / 0.6));
        stack = stack.push(
            Positioned::new().left(252.0).top(564.0).width(1000.0).height(34.0).child(
                Opacity::new(title_a).child(
                    Text::new("time allowed for one smooth frame: 16.7 ms".to_string())
                        .style(pf::geist_mono(22.0).letter_spacing(1.4).color(pf::alpha(ACCENT, 0.97))),
                ),
            ),
        );
        let gx0_l = gx0;
        for (k, (label, value, t0)) in bars.iter().enumerate() {
            let p = pf::clamp01((t - t0) / 0.16);
            if p <= 0.01 {
                continue;
            }
            let y = 630.0 + k as f32 * 64.0;
            stack = stack.push(
                Positioned::new().left(gx0_l).top(y + 28.0).width(320.0).height(26.0).child(
                    Opacity::new(p).child(
                        Text::new(label.to_string())
                            .style(pf::geist_mono(19.0).letter_spacing(1.0).color(pf::alpha(MUTED, 0.92))),
                    ),
                ),
            );
            stack = stack.push(
                Positioned::new().left(1440.0).top(y - 8.0).width(250.0).height(48.0).child(
                    Opacity::new(p).child(
                        Text::new(format!("{:.1} ms", count_f(*value, (t - t0) / 0.20)))
                            .style(pf::geist(34.0).bold().color(pf::alpha(LEDGER, 0.98)))
                            .align(TextAlign::Right),
                    ),
                ),
            );
        }
        // The headroom — the scene's whole point, said once, in words.
        let head_a = pf::clamp01((t - 0.44) / 0.10);
        if head_a > 0.01 {
            stack = stack.push(
                Positioned::new().left(252.0).top(760.0).width(1440.0).height(30.0).child(
                    Opacity::new(head_a).child(
                        Text::new(format!(
                            "even the slowest frame finishes {:.1} ms early — and the app starts in {:.1} ms.",
                            BUDGET_MS - pf::CERT_WORST_MS,
                            pf::CERT_STARTUP_MS
                        ))
                        .style(pf::geist_mono(20.0).letter_spacing(0.6).color(pf::alpha(LEDGER, 0.92))),
                    ),
                ),
            );
        }
    }

    // ── Register C · this film audited itself ───────────────────────
    //
    // The only measured row in the film. It arrives last, in the film's
    // own accent, on its own plate — the audit turning to look at the
    // thing that has been showing it to you.
    let film_p = pf::clamp01((t - 0.58) / 0.10);
    if film_p > 0.01 && probe.frames > 0 {
        let plate = Rect::new(212.0, 824.0, 1708.0, 824.0 + 116.0);
        let sheen_p = pf::clamp01((t - 0.66) / 0.18);
        stack = stack.push(Positioned::fill().child(Painting::sized(
            CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                fk::stage_plate(book, plate, film_p, BRAND_NEAR, 1.0);
                fk::plate_sheen(book, plate, sheen_p, 1.0);
            }),
        )));
        let a = ease_out_cubic(pf::clamp01((film_p - 0.4) / 0.6));
        if a > 0.01 {
            let roll = pf::clamp01((t - 0.62) / 0.18);
            let frames = if roll >= 1.0 {
                probe.frames
            } else {
                pf::count_up(probe.frames, roll)
            };
            stack = stack.push(
                Positioned::new().left(252.0).top(842.0).width(900.0).height(34.0).child(
                    Opacity::new(a).child(
                        Text::new("and this film measured itself".to_string())
                            .style(pf::geist_mono(21.0).letter_spacing(1.4).color(pf::alpha(ACCENT, 0.97))),
                    ),
                ),
            );
            stack = stack.push(
                Positioned::new().left(252.0).top(882.0).width(1000.0).height(30.0).child(
                    Opacity::new(a * 0.9).child(
                        Text::new(format!(
                            "every frame drawn by vieww itself · about {:.0} ms each, on an ordinary CPU",
                            probe.frame_ms
                        ))
                        .style(pf::geist_mono(17.0).letter_spacing(0.4).color(pf::alpha(MUTED, 0.9))),
                    ),
                ),
            );
            stack = stack.push(
                Positioned::new().left(1268.0).top(846.0).width(420.0).height(66.0).child(
                    Opacity::new(a).child(
                        Text::new(format!("{} frames", pf::group_commas(frames)))
                            .style(pf::geist(48.0).bold().letter_spacing(-0.5).color(pf::alpha(INK, 0.98)))
                            .align(TextAlign::Right),
                    ),
                ),
            );
        }
    }

    // ── The film's voice ────────────────────────────────────────────
    stack = stack.push(super::frame::caption(
        "Every number here is real.",
        1002.0,
        pf::clamp01((t - 0.06) / 0.10),
    ));
    stack = stack.push(super::frame::caption(
        "The last one was measured while making this very film.",
        966.0,
        pf::clamp01((t - 0.60) / 0.09),
    ));
    let _ = (sec, ACCENT_DEEP);
    stack.into()
}

// ── Z19 · the_receipts ──────────────────────────────────────────────────────

/// **Where the numbers come from.** The ledger gave the headline figures;
/// this scene gives their provenance — the 49 crates as 49 real tiles,
/// each named, filling in as the count runs, and the certification's own
/// breakdown beside them.
///
/// The crate names are `pf::CRATES`, quoted from the workspace manifest.
/// A film that says "49 crates" and then shows 49 named crates is making
/// a checkable claim; one that shows a number is making a decorative one.
/// (The grid grew with the workspace: six columns held the first 36, seven
/// hold the 49 — and the scene's arithmetic is driven by the list's own
/// length, so the next crate to land needs no edit here.)
/// Each crate's tile in the treemap, by manifest index — vieww-dataviz's
/// squarified layout over the per-crate line counts, a 3 px gutter
/// between tiles.
fn sized_by_source(area: Rect) -> std::sync::Arc<Vec<Rect>> {
    static S: std::sync::OnceLock<std::sync::Arc<Vec<Rect>>> = std::sync::OnceLock::new();
    S.get_or_init(|| {
        use vieww_dataviz::hierarchy::{treemap, Hierarchy};
        let lines = super::crate_lines();
        let mut rows: Vec<(&str, Option<&str>, f64)> = vec![("vieww_base", None, 0.0)];
        rows.extend(lines.iter().map(|(n, l)| (*n, Some("vieww_base"), (*l).max(1) as f64)));
        let mut h = Hierarchy::stratify(&rows).expect("the crate table stratifies");
        h.sum();
        h.sort_by_value();
        let rects = treemap(&h, area, 0.0);
        let mut out = vec![Rect::ZERO; lines.len()];
        for (ni, node) in h.nodes.iter().enumerate().skip(1) {
            if let Some(i) = lines.iter().position(|(n, _)| *n == node.id) {
                out[i] = rects[ni].inflate(-3.0);
            }
        }
        std::sync::Arc::new(out)
    })
    .clone()
}

pub fn the_receipts(ctx: &pf::Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let probe = ctx.probe;
    let mut stack = Stack::new();

    const COLS: usize = 7;
    const TILE_W: f32 = 174.0;
    const TILE_H: f32 = 62.0;
    const GAP: f32 = 14.0;
    const GRID_X: f32 = 96.0;
    const GRID_Y: f32 = 330.0;
    /// The grid's rows, derived — 49 tiles in 7 columns is 7 rows exactly,
    /// and a future crate re-decides this on its own.
    const GRID_ROWS: usize = (pf::CERT_CRATES + COLS - 1) / COLS;

    // **The grid becomes a treemap.** Once every crate has a name, the
    // tiles re-flow into a squarified treemap (vieww-dataviz's
    // `hierarchy::treemap`, d3's own algorithm) whose areas are each
    // crate's lines of Rust, counted from the checkout by this render.
    // Same tiles, same order, same names — now sized by what is in them.
    let grid_area = Rect::new(GRID_X, GRID_Y, GRID_X + COLS as f32 * (TILE_W + GAP) - GAP, GRID_Y + GRID_ROWS as f32 * (TILE_H + GAP) - GAP);
    let tm = sized_by_source(grid_area);
    let morph = crate::film_lib::ease_in_out(pf::clamp01((t - 0.60) / 0.13));
    let tile = {
        let tm = tm.clone();
        move |i: usize, p: f32| -> Rect {
            let (cx, cy) = (i % COLS, i / COLS);
            let x = GRID_X + cx as f32 * (TILE_W + GAP);
            let y = GRID_Y + cy as f32 * (TILE_H + GAP);
            let g = pf::xywh(x, y + (1.0 - p) * 8.0, TILE_W, TILE_H);
            let m = tm.get(i).copied().unwrap_or(g);
            let l = |a: f32, b: f32| a + (b - a) * morph;
            Rect::new(l(g.left, m.left), l(g.top, m.top), l(g.right, m.right), l(g.bottom, m.bottom))
        }
    };
    let tile2 = tile.clone();

    super::frame::ground(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            let _ = (w, h);
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(14, 13, 18)),
                    (0.55, BG_DEEP),
                    (1.0, Color::rgb(10, 9, 13)),
                ]),
            );
            pf::vignette(book, w, h, 0.5);
        }),
    )));
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            let _ = (w, h, &s);

            // The tiles, one per crate in the manifest's own order. Each
            // lands on its own beat, in reading order, so the grid fills
            // the way a count runs; the crates that joined after the first
            // cut wear the brand's violet rail and a corner pip.
            let names = super::workspace_crates();
            for (i, name) in names.iter().enumerate() {
                let fresh = super::NEW_CRATES.contains(name);
                let p = ease_out_expo(pf::clamp01((t - 0.08 - i as f32 * 0.012) / 0.14));
                if p <= 0.01 {
                    continue;
                }
                let r = tile2(i, p);
                let cr = 10.0 - 6.0 * morph;
                book.rrect(r, cr, pf::alpha(Color::rgb(0x14, 0x12, 0x1A), 0.96 * p));
                let mark = if fresh { pf::clamp01((t - 0.50) / 0.08) } else { 0.0 };
                book.stroke_rrect(r, cr, pf::alpha(pf::mix(ACCENT, BRAND_FAR, mark), (0.20 + 0.45 * mark) * p), 1.0 + 0.3 * mark);
                // A lit spine on the left of each tile — the tiles are a
                // list, and a list reads better with a rail.
                book.rect(pf::xywh(r.left, r.top + cr, 3.0, (r.height() - 2.0 * cr).max(0.0)), pf::alpha(pf::mix(BRAND_NEAR, BRAND_FAR, mark), (0.55 + 0.4 * mark) * p));
                if mark > 0.0 && r.width() > 26.0 && r.height() > 26.0 {
                    book.circle(Offset::new(r.right - 13.0, r.top + 13.0), 3.6 * mark, pf::alpha(BRAND_FAR, 0.95));
                }
            }

        }),
    )));

    // The crate names, as type, at the tiles' own coordinates. The tile is
    // narrower than the first cut's (seven columns where six were), so the
    // long names take the smaller size — the grid's density is the point.
    for (i, name) in super::workspace_crates().into_iter().enumerate() {
        let p = ease_out_cubic(pf::clamp01((t - 0.08 - i as f32 * 0.012) / 0.14));
        if p <= 0.01 {
            continue;
        }
        let r = tile(i, p);
        // The shared `vieww-` prefix is said once, above the grid; each
        // tile carries the part that tells the crates apart. In the
        // treemap a name stays only where its tile can hold it, and the
        // big ones add their own line count beneath.
        let short = name.strip_prefix("vieww-").unwrap_or(name);
        let size = if short.len() > 11 { 15.0 } else { 18.0 };
        let need_w = short.len() as f32 * size * 0.62 + 22.0;
        let fits = r.width() >= need_w && r.height() >= 26.0;
        // Names step aside while the tiles travel (they would cross each
        // other mid-flight) and come back on the ones that can hold them.
        let dip = 1.0 - (morph * (1.0 - morph) * 4.0).powf(0.6);
        let la = (if fits { 1.0 } else { 1.0 - pf::clamp01(morph * 4.0) }) * dip;
        let roomy = morph > 0.5 && r.height() > 64.0 && r.width() > 130.0;
        if la > 0.01 {
            let h = if roomy { 40.0 } else { r.height().min(TILE_H) };
            stack = stack.push(super::frame::label(r.left + 14.0, r.top, (r.width() - 20.0).max(10.0), h, short.to_string(),
                pf::geist_mono(size).color(pf::alpha(INK, 0.94)), TextAlign::Left, p * la));
        }
        if roomy {
            let lines = super::crate_lines().get(i).map_or(0, |c| c.1);
            stack = stack.push(super::frame::label(r.left + 14.0, r.top + 32.0, r.width() - 20.0, 24.0,
                format!("{} lines", pf::group_commas(lines)),
                pf::geist_mono(15.0).color(pf::alpha(MUTED, 0.95)), TextAlign::Left, pf::clamp01((morph - 0.5) * 2.0)));
        }
    }

    let header = if morph < 0.5 { "every part of vieww, by name" } else { "every part of vieww, sized by its own source" };
    let swap = 1.0 - (1.0 - ((morph - 0.5).abs() * 2.0)).powi(6);
    stack = stack.push(super::frame::label(GRID_X, GRID_Y - 44.0, 800.0, 32.0, header.to_string(),
        pf::geist_mono(20.0).letter_spacing(1.0).color(pf::alpha(MUTED, 0.92)), TextAlign::Left, pf::clamp01(t / 0.10) * swap));

    // The count, running with the grid.
    let filled = super::workspace_crates()
        .iter()
        .enumerate()
        .filter(|(i, _)| t > 0.08 + *i as f32 * 0.012)
        .count();
    let head_a = pf::clamp01(t / 0.10);
    if head_a > 0.01 {
        stack = stack.push(Positioned::new().left(1440.0).top(300.0).width(420.0).height(110.0).child(
            Opacity::new(head_a).child(Text::new(format!("{filled}"))
                .style(pf::geist(92.0).bold().letter_spacing(-2.0).color(pf::alpha(INK, 0.98)))),
        ));
        stack = stack.push(Positioned::new().left(1440.0).top(410.0).width(420.0).height(34.0).child(
            Opacity::new(head_a).child(Text::new("building blocks".to_string())
                .style(pf::geist_mono(23.0).letter_spacing(1.4).color(pf::alpha(ACCENT, 0.97)))),
        ));
    }

    // The certification's breakdown, under the count — the rows the
    // ledger's hero cards summarised.
    let (rust_lines, tree_tests) = super::tree_stats();
    let rows: [(String, String); 5] = [
        ("automated tests".into(), format!("{} passed · 0 failed", pf::group_commas(pf::CERT_TESTS))),
        ("graphics checks".into(), format!("{} on real GPU paths", pf::CERT_VULKAN_TESTS)),
        ("memory checks".into(), format!("{} allocations · {} animated frames", pf::CERT_ALLOCS_STEADY, pf::CERT_FRAMES_STEADY)),
        ("tests in the tree".into(), format!("{} #[test]s · counted live", pf::group_commas(tree_tests))),
        ("lines of Rust".into(), format!("{} in the engine · counted live", pf::group_commas(rust_lines))),
    ];
    for (i, (label, note)) in rows.iter().enumerate() {
        let p = ease_out_cubic(pf::clamp01((t - 0.42 - i as f32 * 0.04) / 0.16));
        if p <= 0.01 {
            continue;
        }
        let y = 470.0 + i as f32 * 78.0;
        stack = stack.push(Positioned::new().left(1440.0).top(y).width(420.0).height(32.0).child(
            Opacity::new(p).child(Text::new(label.clone())
                .style(pf::geist_mono(22.0).letter_spacing(0.6).color(pf::alpha(INK, 0.96)))),
        ));
        stack = stack.push(Positioned::new().left(1440.0).top(y + 34.0).width(470.0).height(28.0).child(
            Opacity::new(p * 0.85).child(Text::new(note.clone())
                .style(pf::geist_mono(18.0).letter_spacing(0.4).color(pf::alpha(MUTED, 0.92)))),
        ));
        stack = stack.push(Positioned::new().left(1440.0).top(y + 70.0).width(420.0).height(2.0).child(
            Opacity::new(p * 0.5).child(Painting::sized(Size::new(500.0, 2.0), PaintWith::new(
                move |book: &mut Sketchbook, _s: Size| {
                    book.rect(Rect::new(0.0, 0.0, 500.0, 1.0), pf::alpha(Color::WHITE, 0.08));
                },
            ))),
        ));
    }

    // The key to the violet tiles — the workspace's growth, named.
    let key_a = pf::clamp01((t - 0.52) / 0.08);
    if key_a > 0.01 {
        stack = stack.push(Positioned::new().left(GRID_X + COLS as f32 * (TILE_W + GAP) - 520.0).top(GRID_Y - 44.0).width(506.0).height(32.0).child(
            Opacity::new(key_a).child(Text::new(format!("● {} new since the first cut", super::NEW_CRATES.len()))
                .style(pf::geist_mono(20.0).letter_spacing(0.8).color(pf::alpha(BRAND_FAR, 0.97)))
                .align(TextAlign::Right)),
        ));
    }

    // The film's own line, last.
    let film_a = pf::clamp01((t - 0.80) / 0.10);
    if film_a > 0.01 && probe.frames > 0 {
        stack = stack.push(Positioned::new().left(GRID_X).top(GRID_Y + GRID_ROWS as f32 * (TILE_H + GAP) + 14.0).width(1760.0).height(32.0).child(
            Opacity::new(film_a).child(Text::new(format!(
                "and {} frames of this film, counted by its own census",
                pf::group_commas(probe.frames)
            ))
            .style(pf::geist_mono(21.0).letter_spacing(0.6).color(pf::alpha(LEDGER, 0.95)))),
        ));
    }

    stack = stack.push(super::frame::caption(&format!("{} building blocks — and here they are.", super::workspace_crates().len()), 1002.0, pf::clamp01((t - 0.05) / 0.10)));
    stack = stack.push(super::frame::caption("A number you can check beats a number you're told.", 966.0, pf::clamp01((t - 0.60) / 0.10)));
    let _ = sec;
    stack.into()
}
