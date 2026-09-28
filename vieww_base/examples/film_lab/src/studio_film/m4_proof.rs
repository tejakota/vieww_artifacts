//! Movement IV · THE PROOF — real metrics, every number a receipt.
//!
//! One scene, fourteen seconds. The ledger counts up the repo's own
//! certification receipts — quoted, never invented — and closes with
//! the film's own audit: the frames this very film spent, counted by
//! its own census, median frame time measured, not typed.

use vieww_foundation::{Color, Gradient, Rect, Size, Sketchbook, TextAlign};
use vieww_widget::prelude::*;

use crate::film_lib::ease_out_expo;
use crate::product_film as pf;
use super::filmkit as fk;
use super::{ACCENT, BG_DEEP, BRAND_FAR, CANVAS, INK, MUTED, SYN_COMMENT};

/// The ledger's rows — (label, value, source). The last row is the
/// film's own audit, filled from the census manifest at build time.
pub fn rows(probe: &pf::Probe) -> Vec<(String, String, &'static str)> {
    vec![
        (
            "certification tests".into(),
            format!("{}", pf::group_commas(pf::CERT_TESTS)),
            "audit",
        ),
        (
            "crates in the workspace".into(),
            format!("{}", pf::CERT_CRATES),
            "audit",
        ),
        (
            "cold start".into(),
            format!("{:.1} ms", pf::CERT_STARTUP_MS),
            "audit",
        ),
        (
            "p95 frame".into(),
            format!("{:.1} ms", pf::CERT_P95_MS),
            "audit",
        ),
        (
            "worst frame".into(),
            format!("{:.1} ms", pf::CERT_WORST_MS),
            "audit",
        ),
        (
            format!("steady-state allocs · {} frames", pf::CERT_FRAMES_STEADY),
            format!("{}", pf::CERT_ALLOCS_STEADY),
            "audit",
        ),
        (
            format!("this film · median {:.0} ms — counted by its own census", probe.frame_ms),
            format!("{} frames", pf::group_commas(probe.frames)),
            "this film",
        ),
    ]
}

/// A countable value? The numeric rows roll their digits to rest.
fn countable(value: &str) -> Option<u64> {
    let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
    if value.contains("ms") || value.contains("frames ·") {
        None // measured values read as-is; the census owns them
    } else {
        digits.parse().ok()
    }
}

pub fn the_ledger(ctx: &pf::Ctx) -> WidgetNode {
    let t = ctx.t;
    let probe = ctx.probe;

    let mut stack = Stack::new();

    // The room — the audit's register: flat, dark, exact. A single
    // flowing rail runs down its left edge: the receipts arriving,
    // continuously, for as long as the audit speaks.
    let rail_a = 0.5 * pf::clamp01(t / 0.2);
    let rail_t = t * 8.0;
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(14, 13, 12)),
                    (0.6, BG_DEEP),
                    (1.0, Color::rgb(12, 11, 10)),
                ]),
            );
            if rail_a > 0.01 {
                let pts = fk::thread_pts(
                    vieww_foundation::Offset::new(180.0, 130.0),
                    vieww_foundation::Offset::new(230.0, 940.0),
                    120.0,
                );
                fk::flow_along(book, &pts, rail_t, BRAND_FAR, rail_a, 1.4, 0.10);
            }
            pf::vignette(book, w, h, 0.5);
        }),
    )));

    // The ledger's rule — a vertical accent line that grows with the
    // rows, the audit's spine.
    let rule_p = ease_out_expo(pf::clamp01(t / 0.25));
    if rule_p > 0.01 {
        stack = stack.push(Positioned::new().left(420.0).top(270.0).width(4.0).height(470.0 * rule_p).child(
            Painting::sized(Size::new(4.0, (470.0 * rule_p) as u32 as f32), PaintWith::new(
                move |book: &mut Sketchbook, s: Size| {
                    book.rect(
                        Rect::new(0.0, 0.0, s.width, s.height),
                        Gradient::vertical().with_dither().with_stops(&[
                            (0.0, ACCENT),
                            (1.0, pf::ACCENT_DEEP),
                        ]),
                    );
                },
            )),
        ));
    }

    // The rows — each arrives on its own beat; values roll to rest.
    let table = rows(probe);
    for (i, (label, value, source)) in table.iter().enumerate() {
        let t0 = 0.10 + i as f32 * 0.075;
        let p = ease_out_expo(pf::clamp01((t - t0) / 0.16));
        if p <= 0.01 {
            continue;
        }
        let y = 270.0 + i as f32 * 66.0;
        let slide = (1.0 - p) * 16.0;

        // The separator this row sits on.
        stack = stack.push(Positioned::new().left(460.0).top(y + 58.0).width(1000.0).height(1.0).child(
            Opacity::new(0.5 * p).child(Painting::sized(Size::new(1000.0, 1.0), PaintWith::new(
                move |book: &mut Sketchbook, _s: Size| {
                    book.rect(Rect::new(0.0, 0.0, 1000.0, 1.0), pf::alpha(Color::WHITE, 0.07));
                },
            ))),
        ));

        // The label.
        stack = stack.push(
            Positioned::new()
                .left(460.0 + slide)
                .top(y + 14.0)
                .width(560.0)
                .height(34.0)
                .child(Opacity::new(p).child(
                    Text::new(label.clone())
                        .style(pf::geist_mono(16.0).letter_spacing(1.1).color(pf::alpha(MUTED, 0.95))),
                )),
        );

        // The value — counted where it can be, measured where it must.
        let value_a = if let Some(target) = countable(value) {
            let roll = pf::clamp01((t - t0 - 0.06) / 0.24);
            if roll >= 1.0 {
                value.clone()
            } else {
                pf::group_commas(pf::count_up(target, roll))
            }
        } else {
            value.clone()
        };
        stack = stack.push(
            Positioned::new()
                .left(1050.0 + slide)
                .top(y + 8.0)
                .width(280.0)
                .height(44.0)
                .child(Opacity::new(p).child(
                    Text::new(value_a)
                        .style(pf::geist(28.0).bold().color(pf::alpha(INK, 0.97)))
                        .align(TextAlign::Right),
                )),
        );

        // The source chip — the receipt's provenance, on the record.
        let (chip_text, chip_color) = match *source {
            "audit" => ("audit", SYN_COMMENT),
            _ => ("this film", ACCENT),
        };
        stack = stack.push(
            Positioned::new()
                .left(1350.0 + slide)
                .top(y + 18.0)
                .width(110.0)
                .height(26.0)
                .child(Opacity::new(p).child(
                    Text::new(chip_text.to_string())
                        .style(pf::geist_mono(12.0).letter_spacing(1.4).color(pf::alpha(chip_color, 0.95))),
                )),
        );
    }


    // The film's voice.
    stack = stack.push(pf::caption("every number a receipt — quoted from the repo's own audit.", 1002.0, pf::clamp01((t - 0.12) / 0.12)));
    stack = stack.push(pf::caption("the last row is measured, not quoted — this film audited itself.", 966.0, pf::clamp01((t - 0.70) / 0.12)));
    stack = stack.push(pf::chrome(super::progress_rail(ctx.abs)));
    stack.into()
}
