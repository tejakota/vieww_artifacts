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
            "certification tests, green",
            SYN_TYPE,
        ),
        (
            format!("{}", pf::CERT_CRATES),
            "",
            "crates · one core, no forks",
            BRAND_NEAR,
        ),
        (
            format!("{}", pf::CERT_ALLOCS_STEADY),
            "",
            "allocations · 60 steady frames",
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
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(15, 13, 18)),
                    (0.55, BG_DEEP),
                    (1.0, Color::rgb(11, 10, 14)),
                ]),
            );
            if rail_a > 0.01 {
                let pts = fk::thread_pts(Offset::new(128.0, 90.0), Offset::new(168.0, 990.0), 110.0);
                fk::flow_along(book, &pts, rail_t, BRAND_FAR, rail_a, 1.4, 0.10);
            }
            pf::vignette(book, w, h, 0.52);
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
        let rect = Rect::new(x, 318.0, x + card_w, 318.0 + 196.0);
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
                    .top(342.0 + (1.0 - text_a) * 10.0)
                    .width(card_w)
                    .height(120.0)
                    .child(Opacity::new(text_a).child(
                        Text::new(format!("{shown}{unit}"))
                            .style(pf::geist(82.0).bold().letter_spacing(-1.5).color(pf::alpha(INK, 0.98)))
                            .align(TextAlign::Center),
                    )),
            );
            stack = stack.push(
                Positioned::new()
                    .left(x)
                    .top(452.0)
                    .width(card_w)
                    .height(28.0)
                    .child(Opacity::new(text_a).child(
                        Text::new(label.to_string())
                            .style(pf::geist_mono(17.0).letter_spacing(1.6).color(pf::alpha(color, 0.92)))
                            .align(TextAlign::Center),
                    )),
            );
            stack = stack.push(
                Positioned::new()
                    .left(x)
                    .top(480.0)
                    .width(card_w)
                    .height(22.0)
                    .child(Opacity::new(text_a * 0.8).child(
                        Text::new("audit".to_string())
                            .style(pf::geist_mono(12.0).letter_spacing(2.6).color(pf::alpha(MUTED, 0.7)))
                            .align(TextAlign::Center),
                    )),
            );
        }
    }

    // ── Register B · the frame budget ───────────────────────────────
    //
    // The gauge. Two measured frame times against the 60 fps allowance,
    // drawn at the same scale so the gap is a length and not a claim.
    let gauge_p = pf::clamp01((t - 0.28) / 0.14);
    if gauge_p > 0.01 {
        let gx0 = 470.0;
        let gx1 = 1560.0;
        let plate = Rect::new(360.0, 540.0, 1560.0, 540.0 + 226.0);
        let bars: [(&str, f32, f32); 2] = [
            ("p95 frame", pf::CERT_P95_MS, 0.34),
            ("worst frame", pf::CERT_WORST_MS, 0.42),
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
                    let y = 618.0 + k as f32 * 58.0;
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
                let ticks_a = pf::clamp01((t - 0.36) / 0.12);
                fk::gauge_ticks(book, gx0, gx1 - 150.0, 708.0, 12, ticks_a * ga);
                let bx = gx0 + (gx1 - 150.0 - gx0) * (BUDGET_MS / GAUGE_MS);
                let line_p = ease_out_expo(pf::clamp01((t - 0.31) / 0.16));
                if line_p > 0.01 {
                    book.rect(
                        Rect::new(bx - 1.0, 596.0, bx + 1.0, 596.0 + 112.0 * line_p),
                        pf::alpha(Color::WHITE, 0.38 * ga),
                    );
                }
            }),
        )));

        // The gauge's title and the bars' labels — widgets, so the type
        // is shaped by the same engine that shapes the app's.
        let title_a = ease_out_cubic(pf::clamp01((gauge_p - 0.4) / 0.6));
        stack = stack.push(
            Positioned::new().left(400.0).top(566.0).width(760.0).height(30.0).child(
                Opacity::new(title_a).child(
                    Text::new("the frame budget — 60 fps allows 16.7 ms".to_string())
                        .style(pf::geist_mono(17.0).letter_spacing(2.0).color(pf::alpha(ACCENT, 0.95))),
                ),
            ),
        );
        for (k, (label, value, t0)) in bars.iter().enumerate() {
            let p = pf::clamp01((t - t0) / 0.16);
            if p <= 0.01 {
                continue;
            }
            let y = 618.0 + k as f32 * 58.0;
            stack = stack.push(
                Positioned::new().left(400.0).top(y + 28.0).width(320.0).height(20.0).child(
                    Opacity::new(p).child(
                        Text::new(label.to_string())
                            .style(pf::geist_mono(15.0).letter_spacing(1.4).color(pf::alpha(MUTED, 0.9))),
                    ),
                ),
            );
            stack = stack.push(
                Positioned::new().left(1300.0).top(y - 8.0).width(250.0).height(48.0).child(
                    Opacity::new(p).child(
                        Text::new(format!("{:.1} ms", count_f(*value, (t - t0) / 0.20)))
                            .style(pf::geist(34.0).bold().color(pf::alpha(LEDGER, 0.98)))
                            .align(TextAlign::Right),
                    ),
                ),
            );
        }
        // The headroom — the scene's whole point, said once, in words.
        let head_a = pf::clamp01((t - 0.50) / 0.12);
        if head_a > 0.01 {
            stack = stack.push(
                Positioned::new().left(400.0).top(740.0).width(1160.0).height(26.0).child(
                    Opacity::new(head_a).child(
                        Text::new(format!(
                            "{:.1} ms of headroom on the worst frame the audit could find — and a {:.1} ms cold start.",
                            BUDGET_MS - pf::CERT_WORST_MS,
                            pf::CERT_STARTUP_MS
                        ))
                        .style(pf::geist_mono(16.0).letter_spacing(1.2).color(pf::alpha(LEDGER, 0.88))),
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
    let film_p = pf::clamp01((t - 0.62) / 0.12);
    if film_p > 0.01 && probe.frames > 0 {
        let plate = Rect::new(360.0, 786.0, 1560.0, 786.0 + 106.0);
        let sheen_p = pf::clamp01((t - 0.70) / 0.22);
        stack = stack.push(Positioned::fill().child(Painting::sized(
            CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                fk::stage_plate(book, plate, film_p, BRAND_NEAR, 1.0);
                fk::plate_sheen(book, plate, sheen_p, 1.0);
            }),
        )));
        let a = ease_out_cubic(pf::clamp01((film_p - 0.4) / 0.6));
        if a > 0.01 {
            let roll = pf::clamp01((t - 0.66) / 0.22);
            let frames = if roll >= 1.0 {
                probe.frames
            } else {
                pf::count_up(probe.frames, roll)
            };
            stack = stack.push(
                Positioned::new().left(400.0).top(804.0).width(760.0).height(32.0).child(
                    Opacity::new(a).child(
                        Text::new("and this film audited itself".to_string())
                            .style(pf::geist_mono(16.0).letter_spacing(2.2).color(pf::alpha(ACCENT, 0.95))),
                    ),
                ),
            );
            stack = stack.push(
                Positioned::new().left(400.0).top(838.0).width(900.0).height(28.0).child(
                    Opacity::new(a * 0.9).child(
                        Text::new(format!(
                            "every frame rendered by vieww · {:.0} ms median · {}",
                            probe.frame_ms, probe.bench
                        ))
                        .style(pf::geist_mono(13.0).letter_spacing(1.0).color(pf::alpha(MUTED, 0.85))),
                    ),
                ),
            );
            stack = stack.push(
                Positioned::new().left(1120.0).top(804.0).width(420.0).height(66.0).child(
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
    stack = stack.push(pf::caption(
        "every number a receipt — quoted from the repo's own audit.",
        1002.0,
        pf::clamp01((t - 0.06) / 0.10),
    ));
    stack = stack.push(pf::caption(
        "the last one is measured, not quoted — this film is its own benchmark.",
        966.0,
        pf::clamp01((t - 0.64) / 0.10),
    ));
    stack = stack.push(pf::chrome(super::progress_rail(ctx.abs)));
    let _ = (sec, ACCENT_DEEP);
    stack.into()
}
