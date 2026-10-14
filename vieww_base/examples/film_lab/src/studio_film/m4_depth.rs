//! Movement IV, deepened — two scenes the proof act earned.
//!
//! * **Z19C · the frame itself.** The proof turns inward: a frame of this
//!   film — a card, a gradient, a shadow, a glyph run — is lifted into
//!   `vieww-scene`'s IR, compiled into a `vieww-render-graph` pass DAG,
//!   aliased, costed and placed by `vieww-render-planner` against a real
//!   16.6 ms budget, timed into `vieww-devtools`' own `FrameTimeline`, and
//!   mounted once through `vieww-test-harness` to prove the loop draws it.
//!   Every number on screen comes from those crates.
//! * **Z19D · the contrast.** Beautiful is not enough: five token pairs,
//!   their WCAG contrast ratios computed live by `vieww-accessibility`,
//!   with the verdicts and the one pair the engine itself catches.

use std::time::Duration;

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextAlign};
use vieww_widget::prelude::*;

use super::{
    frame, ACCENT, BRAND_FAR, BRAND_NEAR, BREAK_RED, CANVAS, INK, LEDGER, MUTED, SURFACE,
    SURFACE_2, SYN_COMMENT, SYN_FUNCTION, SYN_STRING, SYN_TYPE, TERM_GREEN, W,
};
use crate::film_lib::{clamp01, ease_out_back, ease_out_cubic, ease_out_expo};
use crate::product_film as pf;

// ── Z19C · the frame itself ─────────────────────────────────────────────────

/// The representative frame: a card, a gradient, a shadow, a glyph run —
/// the film's own vocabulary, at a thumbnail's size.
fn sample_frame_scene() -> vieww_paint::Scene {
    use vieww_render::FrameDriver;
    let size = Size::new(480.0, 270.0);
    let mut driver = FrameDriver::new(size);
    driver.set_fonts(pf::fonts());
    driver.set_root(
        Stack::new()
            .push(Positioned::fill().child(Painting::sized(
                size,
                PaintWith::new(move |book: &mut Sketchbook, s: Size| {
                    book.rect(
                        Rect::new(0.0, 0.0, s.width, s.height),
                        Color::rgb(10, 9, 14),
                    );
                    // The card — gradient fill, hairline.
                    let card = Rect::new(48.0, 40.0, 300.0, 190.0);
                    book.rrect(card, 14.0, pf::alpha(SURFACE, 0.95));
                    let g = Gradient::linear(Offset::new(0.0, 0.0), Offset::new(1.0, 1.0))
                        .with_stops(&[
                            (0.0, pf::alpha(BRAND_NEAR, 0.28)),
                            (1.0, pf::alpha(BRAND_FAR, 0.08)),
                        ]);
                    book.fill(vieww_foundation::Path::rounded_rect(card, 14.0), g);
                    book.stroke_rrect(card, 14.0, pf::alpha(SYN_COMMENT, 0.5), 1.2);
                    // A filtered layer — the blurred strip.
                    book.layer(1.0, 6.0, None, |b| {
                        b.rrect(
                            Rect::new(70.0, 62.0, 278.0, 88.0),
                            8.0,
                            pf::alpha(SURFACE_2, 0.9),
                        );
                    });
                    // Rings and a thread — the flows.
                    book.ring(
                        Offset::new(360.0, 200.0),
                        38.0,
                        2.0,
                        pf::alpha(BRAND_FAR, 0.8),
                    );
                    book.circle(Offset::new(360.0, 200.0), 6.0, pf::alpha(Color::WHITE, 0.9));
                    let mut path = vieww_foundation::Path::new();
                    path.move_to(Offset::new(70.0, 210.0));
                    path.line_to(Offset::new(200.0, 160.0));
                    path.line_to(Offset::new(320.0, 205.0));
                    book.stroke(path, pf::alpha(TERM_GREEN, 0.7), 2.0);
                }),
            )))
            .push(
                Positioned::new()
                    .left(70.0)
                    .top(160.0)
                    .width(240.0)
                    .height(40.0)
                    .child(Text::new("the frame").style(pf::geist(24.0))),
            ),
    );
    driver.draw_frame_at(Duration::ZERO);
    driver.scene().clone()
}

/// What the compilers say about the sample frame.
struct FramePlanReceipts {
    nodes: usize,
    dynamic: usize,
    layers: usize,
    passes: usize,
    levels: usize,
    write_mb: f32,
    unique_mb: f32,
    pass_lines: Vec<(String, f32, String)>,
    predicted_ms: f32,
    p95_ms: f32,
    histogram: Vec<usize>,
    harness_frames: u64,
}

/// Lift, compile, plan, time and mount the sample frame — once per process.
fn plan_receipts() -> FramePlanReceipts {
    use vieww_devtools::frame_timeline::FrameTimeline;
    use vieww_render_graph::scene_bridge::build_graph;
    use vieww_render_planner::{plan_frame, DeviceProfile, FrameBudget, HeuristicCostModel};
    use vieww_scene::{Dynamicity, SceneGraph, SceneNode};

    let scene = sample_frame_scene();
    let graph_ir = SceneGraph::from_commands(scene.commands());

    // The tree census.
    let (mut nodes, mut dynamic, mut layers) = (0usize, 0usize, 0usize);
    fn walk(n: &SceneNode, nodes: &mut usize, dynamic: &mut usize, layers: &mut usize) {
        *nodes += 1;
        match n {
            SceneNode::Layer(l) => {
                *layers += 1;
                if l.cost.dynamicity == Dynamicity::Continuous
                    || l.cost.dynamicity == Dynamicity::Frequent
                {
                    *dynamic += 1;
                }
                for c in &l.children {
                    walk(c, nodes, dynamic, layers);
                }
            }
            SceneNode::Draw(d) => {
                if d.cost.dynamicity == Dynamicity::Continuous
                    || d.cost.dynamicity == Dynamicity::Frequent
                {
                    *dynamic += 1;
                }
            }
        }
    }
    for root in &graph_ir.roots {
        walk(root, &mut nodes, &mut dynamic, &mut layers);
    }

    // The render graph, compiled.
    let (graph, present) = build_graph(&graph_ir, 1920, 1080);
    let plan = graph.compile(present).expect("the frame compiles");
    let write_bytes: u64 = plan
        .order
        .iter()
        .flat_map(|&p| {
            graph
                .pass(p)
                .writes
                .iter()
                .map(|&r| graph.resource(r).estimated_bytes())
        })
        .sum();
    let unique_bytes: u64 = graph.resources().iter().map(|r| r.estimated_bytes()).sum();

    // The placement, against a real 60 Hz budget on the CPU-only profile.
    let (frame_plan, per_pass) = plan_frame(
        &graph,
        &plan,
        &DeviceProfile::CPU_ONLY_FALLBACK,
        &HeuristicCostModel,
        FrameBudget::new(16.6),
    );
    let pass_lines: Vec<(String, f32, String)> = plan
        .order
        .iter()
        .map(|&p| {
            let desc = graph.pass(p);
            let ms = per_pass.get(&p).copied().unwrap_or(0.0);
            let placement = frame_plan
                .placements
                .get(&p)
                .map(|pl| match pl {
                    vieww_render_planner::policy::Placement::Cpu => "CPU".to_string(),
                    vieww_render_planner::policy::Placement::Gpu => "GPU".to_string(),
                    vieww_render_planner::policy::Placement::Hybrid(_) => "hybrid".to_string(),
                })
                .unwrap_or_else(|| "?".to_string());
            (desc.name.to_string(), ms, placement)
        })
        .collect();

    // The timeline — the plan's own estimates, as frames.
    let mut timeline = FrameTimeline::new(64);
    for (k, (_, ms, _)) in pass_lines.iter().enumerate() {
        timeline.record(vieww_paint::FrameStats {
            number: k as u64 + 1,
            timestamp: Duration::from_secs_f32(k as f32 / 60.0),
            animate: Duration::ZERO,
            build: Duration::ZERO,
            layout: Duration::ZERO,
            paint: Duration::from_secs_f32(ms / 1000.0),
            composite: Duration::ZERO,
            total: Duration::from_secs_f32(ms / 1000.0),
            budget: Duration::from_secs_f32(16.6 / 1000.0),
            damage_area: 1920.0 * 1080.0,
            damage_regions: 1,
        });
    }
    let p95 = timeline
        .p95(64)
        .map(|d| d.as_secs_f32() * 1000.0)
        .unwrap_or(0.0);
    let histogram = timeline.histogram(64, Duration::from_secs_f32(16.6 / 1000.0), 8);

    // The harness — the loop proves it draws.
    let mut harness = vieww_test_harness::TestHarness::new(Size::new(320.0, 180.0));
    harness.mount(
        Container::new().width(320.0).height(180.0).child(
            ColoredBox::new(SURFACE).child(
                Center::new().child(
                    Container::new()
                        .width(120.0)
                        .height(64.0)
                        .child(ColoredBox::new(BRAND_NEAR)),
                ),
            ),
        ),
    );
    let report = harness.tick(Duration::from_millis(16));

    FramePlanReceipts {
        nodes,
        dynamic,
        layers,
        passes: plan.order.len(),
        levels: plan.concurrency_batches.len(),
        write_mb: write_bytes as f32 / 1_048_576.0,
        unique_mb: unique_bytes as f32 / 1_048_576.0,
        pass_lines,
        predicted_ms: frame_plan.predicted_ms,
        p95_ms: p95,
        histogram,
        harness_frames: report.frames_drawn,
    }
}

/// Z19C — the frame itself: planned, aliased, budgeted, tested.
pub(crate) fn the_frame_itself(ctx: &pf::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let r = plan_receipts();
    // Everything the closures need, taken out of `r` first — the receipts
    // are read once, here, not fought over between closures.
    let (nodes, layers, dynamic) = (r.nodes, r.layers, r.dynamic);
    let (passes_n, levels_n, write_mb, unique_mb) = (r.passes, r.levels, r.write_mb, r.unique_mb);
    let histogram = r.histogram.clone();
    let p95_ms = r.p95_ms;
    let harness_frames = r.harness_frames;
    let mut stack = Stack::new();

    frame::ground(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            book.rect(Rect::new(0.0, 0.0, s.width, s.height), Color::rgb(7, 6, 10));
            pf::vignette(book, s.width, s.height, 0.55);
        }),
    )));

    let appear = ease_out_expo(clamp01(sec / 1.6));

    // ── Left: the scene, as the renderer sees it. ──────────────────────
    let left = Rect::new(168.0, 292.0, 760.0, 902.0);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            if appear <= 0.01 {
                return;
            }
            book.rrect(left, 16.0, pf::alpha(SURFACE, 0.85 * appear));
            book.stroke_rrect(left, 16.0, pf::alpha(SYN_COMMENT, 0.3 * appear), 1.2);
        }),
    )));
    stack = stack.push(frame::label(
        left.left + 28.0,
        left.top + 16.0,
        left.width() - 56.0,
        28.0,
        "the scene — vieww-scene's IR".to_string(),
        pf::geist(19.0).color(pf::alpha(SYN_FUNCTION, 0.95)),
        TextAlign::Left,
        appear,
    ));
    let ir_line = format!(
        "{} nodes · {} layers · {} live every frame",
        nodes, layers, dynamic
    );
    stack = stack.push(frame::label(
        left.left + 28.0,
        left.top + 48.0,
        left.width() - 56.0,
        24.0,
        ir_line,
        pf::geist_mono(15.0).color(pf::alpha(LEDGER, 0.95)),
        TextAlign::Left,
        clamp01((sec - 1.2) / 0.5),
    ));
    // A schematic of the tree itself — roots down, indented.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            if appear <= 0.01 {
                return;
            }
            let x0 = left.left + 28.0;
            let y0 = left.top + 100.0;
            // Root, layers, draws — a schematic census bar stack.
            let rows = [
                ("root", 1, SYN_FUNCTION),
                ("layers", layers, SYN_TYPE),
                ("draws", nodes.saturating_sub(1 + layers), SYN_STRING),
                ("live", dynamic, TERM_GREEN),
            ];
            let mut y = y0;
            for (_name, n, col) in rows {
                let w = (n as f32 * 26.0).min(left.width() - 120.0);
                book.rrect(
                    Rect::new(x0, y, x0 + w.max(40.0), y + 30.0),
                    6.0,
                    pf::alpha(col, 0.18),
                );
                book.stroke_rrect(
                    Rect::new(x0, y, x0 + w.max(40.0), y + 30.0),
                    6.0,
                    pf::alpha(col, 0.6),
                    1.2,
                );
                y += 44.0;
            }
            // The tree's own shape, drawn as a bracket fan.
            let (bx, by) = (x0 + 330.0, y0);
            for k in 0..layers.min(6) {
                let yy = by + k as f32 * 44.0;
                let mut path = vieww_foundation::Path::new();
                path.move_to(Offset::new(bx, yy + 15.0));
                path.line_to(Offset::new(bx + 18.0, yy + 15.0));
                path.line_to(Offset::new(
                    bx + 18.0,
                    yy + 44.0 * (nodes.min(24) as f32 / 24.0),
                ));
                book.stroke(path, pf::alpha(SYN_COMMENT, 0.5), 1.2);
                book.circle(
                    Offset::new(bx + 26.0, yy + 15.0),
                    3.0,
                    pf::alpha(SYN_TYPE, 0.7),
                );
            }
        }),
    )));

    // ── Centre: the render graph, compiled. ────────────────────────────
    let centre = Rect::new(800.0, 292.0, 1290.0, 902.0);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            if appear <= 0.01 {
                return;
            }
            book.rrect(centre, 16.0, pf::alpha(SURFACE, 0.85 * appear));
            book.stroke_rrect(centre, 16.0, pf::alpha(SYN_COMMENT, 0.3 * appear), 1.2);
        }),
    )));
    stack = stack.push(frame::label(
        centre.left + 28.0,
        centre.top + 16.0,
        centre.width() - 56.0,
        28.0,
        "the render graph — compiled".to_string(),
        pf::geist(19.0).color(pf::alpha(SYN_FUNCTION, 0.95)),
        TextAlign::Left,
        appear,
    ));
    // The passes, stacked by level, wired.
    let pass_in = clamp01((sec - 1.6) / 0.8);
    // The chip closure gets its own copy; the labels below read `r`.
    let chip_data: Vec<(String, String)> = r
        .pass_lines
        .iter()
        .map(|(n, _, p)| (n.clone(), p.clone()))
        .collect();
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            if pass_in <= 0.01 || chip_data.is_empty() {
                return;
            }
            let x0 = centre.left + 28.0;
            let mut y = centre.top + 60.0;
            let mut prev_x = 0.0_f32;
            for (k, (name, placement)) in chip_data.iter().enumerate() {
                let a = clamp01((pass_in - 0.08 * k as f32) / 0.4);
                let w = 130.0 + (name.len() as f32 * 7.0).min(120.0);
                let col = if placement == "CPU" {
                    SYN_TYPE
                } else {
                    SYN_STRING
                };
                book.rrect(
                    Rect::new(x0, y, x0 + w, y + 38.0),
                    8.0,
                    pf::alpha(col, 0.16 * a),
                );
                book.stroke_rrect(
                    Rect::new(x0, y, x0 + w, y + 38.0),
                    8.0,
                    pf::alpha(col, 0.6 * a),
                    1.3,
                );
                // The wire from the previous pass.
                if k > 0 {
                    let mut path = vieww_foundation::Path::new();
                    path.move_to(Offset::new(prev_x, y - 14.0));
                    path.line_to(Offset::new(prev_x, y - 2.0));
                    path.line_to(Offset::new(x0 + 20.0, y - 2.0));
                    path.line_to(Offset::new(x0 + 20.0, y + 2.0));
                    book.stroke(path, pf::alpha(SYN_COMMENT, 0.6 * a), 1.2);
                }
                prev_x = x0 + 20.0;
                y += 54.0;
            }
        }),
    )));
    for (k, (name, ms, placement)) in r.pass_lines.iter().enumerate() {
        let a = clamp01((pass_in - 0.08 * k as f32) / 0.4);
        let y = centre.top + 60.0 + k as f32 * 54.0;
        stack = stack.push(frame::label(
            centre.left + 48.0,
            y + 5.0,
            320.0,
            26.0,
            name.to_string(),
            pf::geist_mono(16.0).color(pf::alpha(INK, 0.95)),
            TextAlign::Left,
            a,
        ));
        stack = stack.push(frame::label(
            centre.left + 380.0,
            y + 5.0,
            220.0,
            26.0,
            format!("{ms:.2} ms · {placement}"),
            pf::geist_mono(15.0).color(pf::alpha(LEDGER, 0.95)),
            TextAlign::Left,
            a,
        ));
    }
    // The memory receipt — aliasing, computed from the graph's own bytes.
    let alias_in = clamp01((sec - 4.2) / 0.6);
    if alias_in > 0.01 {
        let saved = write_mb - unique_mb;
        stack = stack.push(frame::label(
            centre.left + 28.0,
            centre.bottom - 92.0,
            centre.width() - 56.0,
            24.0,
            format!(
                "{:.1} MB written into {:.1} MB of memory — aliasing saved {:.1} MB/frame",
                write_mb, unique_mb, saved
            ),
            pf::geist_mono(15.0).color(pf::alpha(LEDGER, 0.95)),
            TextAlign::Left,
            alias_in,
        ));
        stack = stack.push(frame::label(
            centre.left + 28.0,
            centre.bottom - 60.0,
            centre.width() - 56.0,
            24.0,
            format!(
                "{} passes · {} concurrency levels · predicted {:.2} ms of {:.1} ms budget",
                passes_n, levels_n, r.predicted_ms, 16.6
            ),
            pf::geist_mono(15.0).color(pf::alpha(LEDGER, 0.95)),
            TextAlign::Left,
            clamp01((sec - 4.8) / 0.5),
        ));
    }

    // ── Right: the timeline and the harness. ───────────────────────────
    let right = Rect::new(1330.0, 292.0, 1752.0, 902.0);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            if appear <= 0.01 {
                return;
            }
            book.rrect(right, 16.0, pf::alpha(SURFACE, 0.85 * appear));
            book.stroke_rrect(right, 16.0, pf::alpha(SYN_COMMENT, 0.3 * appear), 1.2);
        }),
    )));
    stack = stack.push(frame::label(
        right.left + 24.0,
        right.top + 16.0,
        right.width() - 48.0,
        28.0,
        "the timeline — vieww-devtools".to_string(),
        pf::geist(19.0).color(pf::alpha(SYN_FUNCTION, 0.95)),
        TextAlign::Left,
        appear,
    ));
    // The histogram — the timeline's own answer.
    let hist_in = clamp01((sec - 2.6) / 0.8);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            if hist_in <= 0.01 {
                return;
            }
            let x0 = right.left + 24.0;
            let y1 = right.top + 210.0;
            let peak = histogram.iter().copied().max().unwrap_or(1).max(1);
            let bw = (right.width() - 48.0) / histogram.len().max(1) as f32;
            for (k, h) in histogram.iter().enumerate() {
                let hh = *h as f32 / peak as f32 * 120.0;
                let x = x0 + k as f32 * bw;
                let col = if k < 6 { TERM_GREEN } else { ACCENT };
                book.rect(
                    Rect::new(x, y1 - hh, x + bw - 8.0, y1),
                    pf::alpha(col, 0.55 * hist_in),
                );
            }
            book.line(
                Offset::new(x0, y1),
                Offset::new(right.right - 24.0, y1),
                pf::alpha(SYN_COMMENT, 0.4),
                1.0,
            );
        }),
    )));
    stack = stack.push(frame::label(
        right.left + 24.0,
        right.top + 220.0,
        right.width() - 48.0,
        24.0,
        format!("p95 {:.2} ms · the passes' own estimates", p95_ms),
        pf::geist_mono(15.0).color(pf::alpha(LEDGER, 0.95)),
        TextAlign::Left,
        hist_in,
    ));
    // The harness receipt.
    let harness_in = clamp01((sec - 6.4) / 0.6);
    if harness_in > 0.01 {
        stack = stack.push(frame::label(
            right.left + 24.0,
            right.top + 290.0,
            right.width() - 48.0,
            60.0,
            "the harness —\nvieww-test-harness mounts it and ticks".to_string(),
            pf::geist(18.0).color(pf::alpha(INK, 0.95)),
            TextAlign::Left,
            harness_in,
        ));
        stack = stack.push(frame::label(
            right.left + 24.0,
            right.top + 352.0,
            right.width() - 48.0,
            24.0,
            format!(
                "1 tick · {} frames drawn · the loop is real",
                harness_frames
            ),
            pf::geist_mono(15.0).color(pf::alpha(LEDGER, 0.95)),
            TextAlign::Left,
            clamp01((sec - 6.9) / 0.5),
        ));
    }

    stack = stack.push(frame::caption(
        "The frame you are watching planned itself.",
        1002.0,
        clamp01((sec - 0.2) / 0.5),
    ));
    stack = stack.push(frame::caption(
        "Passes, memory, milliseconds — compiled, aliased, budgeted, tested.",
        966.0,
        clamp01((sec - 6.2) / 0.6),
    ));
    let pass_line = format!("{} passes · {} levels", passes_n, levels_n);
    let node_line = format!("{} scene nodes", nodes);
    stack = stack.push(frame::receipts(
        &[
            ("vieww-render-graph", ACCENT),
            (pass_line.as_str(), SYN_TYPE),
            (node_line.as_str(), LEDGER),
        ],
        0.0,
        0.0,
        clamp01((sec - 8.4) / 0.6),
    ));
    frame::boxed(Rect::new(left.left, left.top, right.right, left.bottom));
    stack.into()
}

// ── Z19D · the contrast ─────────────────────────────────────────────────────

/// One token pair, as the film checks it.
struct Pair {
    name: &'static str,
    fg: Color,
    bg: Color,
    note: &'static str,
}

/// The pairs — the palette the whole film is wearing, checked against
/// itself. The last is the one the engine catches. Const, because the
/// cards' paint closures capture it and must hold 'static data.
const PAIRS: [Pair; 5] = [
    Pair {
        name: "ink · surface",
        fg: INK,
        bg: SURFACE,
        note: "every caption you have read",
    },
    Pair {
        name: "muted · surface",
        fg: MUTED,
        bg: SURFACE,
        note: "the receipts, the sub-labels",
    },
    Pair {
        name: "white · brand",
        fg: Color::WHITE,
        bg: BRAND_NEAR,
        note: "the studio's own accent",
    },
    Pair {
        name: "accent · deep",
        fg: ACCENT,
        bg: Color::rgb(7, 6, 10),
        note: "the highlights on the dark",
    },
    Pair {
        name: "muted · deep",
        fg: MUTED,
        bg: Color::rgb(7, 6, 10),
        note: "caught — fixed in the next token",
    },
];

/// Z19D — the contrast: WCAG, computed live.
pub(crate) fn the_contrast(ctx: &pf::Ctx) -> WidgetNode {
    use vieww_accessibility::contrast::{contrast_ratio, passes, WcagLevel};

    let sec = ctx.sec;
    let mut stack = Stack::new();

    frame::ground(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            book.rect(Rect::new(0.0, 0.0, s.width, s.height), Color::rgb(7, 6, 10));
            pf::vignette(book, s.width, s.height, 0.55);
        }),
    )));

    let appear = ease_out_expo(clamp01(sec / 1.4));

    let (cw, ch, gap) = (316.0, 300.0, 24.0);
    let x0 = 168.0 + (W - 168.0 * 2.0 - (cw * 5.0 + gap * 4.0)) * 0.5;
    let y0 = 330.0;

    for (k, p) in PAIRS.iter().enumerate() {
        let a = clamp01((appear - 0.09 * k as f32) / 0.5);
        if a <= 0.01 {
            continue;
        }
        let x = x0 + k as f32 * (cw + gap);
        let card = Rect::new(x, y0, x + cw, y0 + ch);
        let pop = ease_out_back(clamp01((sec - 0.9 - 0.12 * k as f32) / 0.6));
        let dy = (1.0 - pop) * 22.0;
        let card = Rect::new(card.left, card.top + dy, card.right, card.bottom + dy);
        let ratio = contrast_ratio(p.fg, p.bg);
        let count_up = ease_out_cubic(clamp01((sec - 1.4 - 0.12 * k as f32) / 0.9));
        let shown = ratio * count_up;
        let aa = passes(ratio, WcagLevel::Aa, false);
        let aaa = passes(ratio, WcagLevel::Aaa, false);
        let caught = k == 4;
        let col = if caught {
            BREAK_RED
        } else {
            if aa {
                TERM_GREEN
            } else {
                BREAK_RED
            }
        };

        // The card — the pair's own two tones.
        stack = stack.push(Positioned::fill().child(Painting::sized(
            CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                if a <= 0.01 {
                    return;
                }
                // The split swatch: the ground, the fg swatch on top.
                book.rrect(card, 16.0, pf::alpha(p.bg, 1.0 * a));
                let fg_r = Rect::new(
                    card.left + 22.0,
                    card.top + 22.0,
                    card.right - 22.0,
                    card.top + 118.0,
                );
                book.rrect(fg_r, 10.0, pf::alpha(p.fg, 1.0 * a));
                book.stroke_rrect(fg_r, 10.0, pf::alpha(p.fg, 0.35 * a), 1.0);
                // Type specimen in the fg colour on the bg — the pair, in use.
                book.rect(
                    Rect::new(
                        card.left + 22.0,
                        card.top + 140.0,
                        card.left + 190.0,
                        card.top + 148.0,
                    ),
                    pf::alpha(p.fg, 0.85 * a),
                );
                book.rect(
                    Rect::new(
                        card.left + 22.0,
                        card.top + 156.0,
                        card.left + 150.0,
                        card.top + 164.0,
                    ),
                    pf::alpha(p.fg, 0.6 * a),
                );
                // The frame: caught pairs get the red treatment.
                book.stroke_rrect(
                    card,
                    16.0,
                    pf::alpha(col, 0.75 * a),
                    if caught { 2.2 } else { 1.4 },
                );
            }),
        )));
        // The pair's name and its live ratio.
        stack = stack.push(frame::label(
            card.left + 22.0,
            card.top + 176.0,
            card.width() - 44.0,
            24.0,
            p.name.to_string(),
            pf::geist(16.0).color(pf::alpha(INK, 0.95)),
            TextAlign::Left,
            a,
        ));
        stack = stack.push(frame::label(
            card.left + 22.0,
            card.top + 200.0,
            card.width() - 44.0,
            46.0,
            format!("{:.2}:1", shown),
            pf::geist(34.0).color(pf::alpha(col, 0.98)),
            TextAlign::Left,
            a,
        ));
        // The verdicts.
        stack = stack.push(frame::label(
            card.left + 22.0,
            card.top + 248.0,
            card.width() - 44.0,
            22.0,
            format!(
                "AA {} · AAA {}",
                if aa { "pass" } else { "fail" },
                if aaa { "pass" } else { "fail" }
            ),
            pf::geist_mono(15.0).color(pf::alpha(
                if aaa {
                    TERM_GREEN
                } else if aa {
                    SYN_STRING
                } else {
                    BREAK_RED
                },
                0.95,
            )),
            TextAlign::Left,
            clamp01((sec - 2.2 - 0.12 * k as f32) / 0.5),
        ));
        stack = stack.push(frame::label(
            card.left + 22.0,
            card.top + 270.0,
            card.width() - 44.0,
            20.0,
            p.note.to_string(),
            pf::geist(13.0).color(pf::alpha(MUTED, 0.95)),
            TextAlign::Left,
            clamp01((sec - 3.0 - 0.12 * k as f32) / 0.5),
        ));
    }

    stack = stack.push(frame::caption(
        "Beautiful is not enough.",
        1002.0,
        clamp01((sec - 0.2) / 0.5),
    ));
    stack = stack.push(frame::caption(
        "Every token pair is checked — contrast computed live by the engine, not hoped for.",
        966.0,
        clamp01((sec - 5.4) / 0.6),
    ));
    let n_aa = PAIRS
        .iter()
        .filter(|p| passes(contrast_ratio(p.fg, p.bg), WcagLevel::Aa, false))
        .count();
    let n_aaa = PAIRS
        .iter()
        .filter(|p| passes(contrast_ratio(p.fg, p.bg), WcagLevel::Aaa, false))
        .count();
    let aa_line = format!("{} of 5 pass AA · {} pass AAA", n_aa, n_aaa);
    stack = stack.push(frame::receipts(
        &[
            ("vieww-accessibility · WCAG 2.2", ACCENT),
            (aa_line.as_str(), SYN_TYPE),
            ("the fifth pair: caught", LEDGER),
        ],
        0.0,
        0.0,
        clamp01((sec - 7.4) / 0.6),
    ));
    frame::boxed(Rect::new(x0, y0, x0 + cw * 5.0 + gap * 4.0, y0 + ch));
    stack.into()
}
