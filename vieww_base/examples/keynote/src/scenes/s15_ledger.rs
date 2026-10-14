//! **S15 · THE GREEN LEDGER** — 結. Receipts instead of adjectives.
//!
//! Act IV opens by paying for everything the film has claimed. A wall of test
//! results fills in, cell by cell, green as they pass — and then the wall
//! turns out to be a *count*, printed underneath: the suites, the plates, the
//! open regressions.
//!
//! The honesty column is the point. Beside the green, in the same type at the
//! same size, the film prints what it does **not** claim: iOS, disputed by
//! the codebase's own docs and excluded; the Android figures, quoted from CI
//! rather than measured here; sound, deferred; and the fact that determinism
//! is per-bench and this bench is named. A ledger that only lists credits is
//! not a ledger.
//!
//! Every figure on the left is from this repository's own artifacts; every
//! figure the film measured of itself carries the bench line.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Rect, Size, Sketchbook};
use vieww_widget::prelude::*;

use crate::film::Ctx;
use crate::kit::{
    act_card, alpha, caption, clamp01, dust, ease_out_cubic, grain, ground, horizon, mix, painter,
    seg, smoothstep, thousands, vignette, xywh, Rng, Type, AMBER, CYAN, H, INK, INK_SOFT, MINT,
    MUTED, RED, VIOLET, VIOLET_SOFT, W,
};
use crate::studio::compose;

/// The suites, quoted from the repository's own README.
const SUITES: &[(&str, u32)] = &[("framework", 518), ("studio", 237), ("integration", 13)];
/// The element-lab plates, and the open regressions.
const PLATES: u32 = 90;
const OPEN_REGRESSIONS: u32 = 0;

/// What the film declines to claim. The same type, the same size, the same
/// weight as the green — that is the whole design of this scene.
const NOT_CLAIMED: &[(&str, &str)] = &[
    ("ios", "disputed by the codebase's own docs · excluded"),
    ("59.3 fps · 4.09 ms", "quoted from CI artifacts · not measured by this render"),
    ("sound", "deferred · the cut works muted by construction"),
    ("byte-identical", "per bench · this render names its own"),
];

const FILL_T0: f32 = 0.80;
const FILL_SPAN: f32 = 5.20;
const COUNT_T: f32 = 6.60;
const HONEST_T: f32 = 10.60;

pub fn frame(ctx: &Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let frame_i = ctx.frame;

    let total: u32 = SUITES.iter().map(|(_, n)| *n).sum();
    let fill = ease_out_cubic(seg(sec, FILL_T0, FILL_T0 + FILL_SPAN));
    let shown = (f64::from(total) * f64::from(fill)) as u32;
    let count_a = smoothstep(seg(sec, COUNT_T, COUNT_T + 0.9));
    let honest = smoothstep(seg(sec, HONEST_T, HONEST_T + 1.0));
    let out = smoothstep(seg(ctx.t, 0.94, 1.0));

    // The wall: one cell per test, laid out in a grid that fits the column.
    let cols = 44usize;
    let rows = (total as usize).div_ceil(cols);
    let wall = xywh(112.0, 208.0, 880.0, rows as f32 * 15.0);

    let bg = painter(move |book: &mut Sketchbook, size: Size| {
        ground(book, size, sec, 1.0);
        horizon(book, size, MINT, 0.35);
        dust(book, size, sec, 30, MINT, 0.4);

        // The wall, cell by cell. Each cell lights as its test passes; the
        // order is seeded, so the fill looks like a suite running rather
        // than like a progress bar.
        let mut rng = Rng::new(0x_1E_D6_E2);
        let mut order: Vec<u32> = (0..total).collect();
        // A cheap deterministic shuffle — Fisher–Yates on the seeded stream.
        for i in (1..order.len()).rev() {
            let j = rng.below(i + 1);
            order.swap(i, j);
        }
        let cw = wall.width() / cols as f32;
        for (rank, idx) in order.iter().enumerate() {
            if rank as u32 >= shown {
                break;
            }
            let i = *idx as usize;
            let x = wall.left + (i % cols) as f32 * cw;
            let y = wall.top + (i / cols) as f32 * 15.0;
            // The newest cells arrive brighter and settle — the eye follows
            // the run rather than the rectangle.
            let age = clamp01((shown - rank as u32) as f32 / 90.0);
            let c = mix(Color::WHITE, MINT, 0.25 + 0.75 * age);
            book.rrect(xywh(x, y, cw - 3.0, 10.0), 2.0, alpha(c, 0.30 + 0.55 * age));
        }
        // The cells not yet run.
        for i in (shown as usize)..(total as usize) {
            let x = wall.left + (i % cols) as f32 * cw;
            let y = wall.top + (i / cols) as f32 * 15.0;
            book.rrect(xywh(x, y, cw - 3.0, 10.0), 2.0, alpha(Color::rgb(0x24, 0x22, 0x20), 0.9));
        }

        // The green glow under a finished wall — one group, once.
        if fill > 0.98 {
            let a = smoothstep(seg(fill, 0.98, 1.0));
            book.blended_layer(1.0, 60.0, BlendMode::Plus, None, |g| {
                g.rrect(
                    xywh(wall.left, wall.top, wall.width(), wall.height()),
                    12.0,
                    alpha(MINT, 0.08 * a),
                );
            });
        }

        // The honesty column's own rule — a vertical hairline separating the
        // two halves, because the film is making a point about symmetry.
        if honest > 0.02 {
            crate::kit::rule_v(book, 1088.0, 200.0, 560.0, alpha(INK_SOFT, 0.16 * honest));
        }

        grain(book, size, frame_i, 0.012, 300);
        vignette(book, size, 0.95);
        if out > 0.004 {
            book.rect(Rect::new(0.0, 0.0, size.width, size.height), alpha(Color::BLACK, out));
        }
    });

    let mut nodes: Vec<WidgetNode> = vec![
        Type::new("the ledger")
            .size(46.0)
            .light()
            .track(0.6)
            .color(alpha(INK, 0.97 * (1.0 - out)))
            .at(112.0, 96.0)
            .width(600.0)
            .into(),
        Type::new("everything this film claimed, and everything it did not")
            .mono()
            .size(14.0)
            .track(2.2)
            .color(alpha(MUTED, 0.9 * (1.0 - out)))
            .at(114.0, 152.0)
            .width(900.0)
            .into(),
    ];

    // The counts, under the wall.
    if count_a > 0.004 {
        let a = count_a * (1.0 - out);
        let y = wall.bottom + 44.0;
        let mut x = 112.0;
        for (name, n) in SUITES {
            nodes.push(
                Type::new(thousands(u64::from(*n)))
                    .size(44.0)
                    .medium()
                    .color(alpha(MINT, 0.97 * a))
                    .at(x, y)
                    .width(220.0)
                    .into(),
            );
            nodes.push(
                Type::new(*name)
                    .mono()
                    .size(12.5)
                    .track(2.2)
                    .color(alpha(MUTED, 0.9 * a))
                    .at(x + 2.0, y + 56.0)
                    .width(220.0)
                    .into(),
            );
            x += 200.0;
        }
        nodes.push(
            Type::new(format!("{PLATES}"))
                .size(44.0)
                .medium()
                .color(alpha(CYAN, 0.97 * a))
                .at(x, y)
                .width(220.0)
                .into(),
        );
        nodes.push(
            Type::new("element plates")
                .mono()
                .size(12.5)
                .track(2.2)
                .color(alpha(MUTED, 0.9 * a))
                .at(x + 2.0, y + 56.0)
                .width(220.0)
                .into(),
        );
        nodes.push(
            Type::new(format!("{OPEN_REGRESSIONS}"))
                .size(44.0)
                .medium()
                .color(alpha(if OPEN_REGRESSIONS == 0 { MINT } else { RED }, 0.97 * a))
                .at(x + 200.0, y)
                .width(220.0)
                .into(),
        );
        nodes.push(
            Type::new("open regressions")
                .mono()
                .size(12.5)
                .track(2.2)
                .color(alpha(MUTED, 0.9 * a))
                .at(x + 202.0, y + 56.0)
                .width(240.0)
                .into(),
        );
    }

    // The honesty column.
    if honest > 0.004 {
        let a = honest * (1.0 - out);
        nodes.push(
            Type::new("not claimed")
                .size(26.0)
                .medium()
                .track(0.4)
                .color(alpha(AMBER, 0.95 * a))
                .at(1128.0, 204.0)
                .width(520.0)
                .into(),
        );
        for (i, (what, why)) in NOT_CLAIMED.iter().enumerate() {
            let y = 262.0 + i as f32 * 96.0;
            let row = smoothstep(seg(sec, HONEST_T + 0.3 + i as f32 * 0.35, HONEST_T + 1.1 + i as f32 * 0.35));
            nodes.push(
                Type::new(*what)
                    .size(22.0)
                    .color(alpha(INK, 0.95 * a * row))
                    .at(1128.0, y)
                    .width(660.0)
                    .into(),
            );
            nodes.push(
                Type::new(*why)
                    .mono()
                    .size(12.5)
                    .track(1.4)
                    .color(alpha(MUTED, 0.9 * a * row))
                    .at(1130.0, y + 32.0)
                    .width(700.0)
                    .into(),
            );
        }
    }

    // The bench, named, because determinism is per-bench.
    let bench_a = smoothstep(seg(sec, 15.6, 16.6)) * (1.0 - out);
    if bench_a > 0.004 {
        let bench = if ctx.probe.bench.is_empty() {
            "bench not recorded — run the census".to_string()
        } else {
            format!("this render's bench · {}", ctx.probe.bench)
        };
        nodes.push(
            Type::new(bench)
                .mono()
                .size(13.0)
                .track(1.6)
                .color(alpha(VIOLET_SOFT, 0.9 * bench_a))
                .at(112.0, H - 210.0)
                .width(1200.0)
                .into(),
        );
    }

    nodes.push(act_card("ACT IV", "THE LEDGER", "結", smoothstep(seg(sec, 0.4, 1.4)) * (1.0 - smoothstep(seg(sec, 6.0, 7.0)))));
    nodes.push(caption(
        "quoted from this repository's own artifacts",
        smoothstep(seg(sec, 2.4, 3.4)) * (1.0 - smoothstep(seg(ctx.t, 0.90, 0.98))),
    ));
    let _ = (VIOLET, Gradient::vertical(), Offset::ZERO, ease_out_cubic);
    compose(bg, nodes)
}
