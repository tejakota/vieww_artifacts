//! exp_filterstack — *the compositor-depth axis.* The frost cathedral.
//!
//! Five receding glass arches, every one its own `Filtered` group, and a
//! rack focus driven by the subject: a lantern travels from the deep end
//! toward the camera, and each pane's σ is its **distance from the
//! lantern's depth** — the panes near the traveller stay sharp, the far
//! ones frost over. The focus pull is the frame's own depth field.
//!
//! Around it, the filter system's other doors, all in one scene: a **rose
//! window whose ring is a `Filtered` inside a `Filtered`** (blur of blur —
//! nesting, the compositor's recursion axis, with its outer σ sweeping
//! 24 → 0.5 across the plate, the extremes both ends), and a floating
//! caption pane on `with_backdrop()` wearing a **matrix chain**
//! (`sepia ∘ brightness ∘ tint` — three calls, one composed pass, the
//! promise the `Filtered` docs make).
//!
//! The receipt prints every pane's live σ and the filtered-layer count
//! against the U-06 guard (8 of 32 here — breadth spent on purpose,
//! the gauge visible so the next plate knows the budget it spent).

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook,
    TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Filtered, Painting, PaintWith, Text};

use crate::film_lib::{alpha, clamp01, ease_in_out, mix, BG_DEEP, FAINT, INK, MUTED, VIOLET,
    VIOLET_SOFT, AMBER, CYAN_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 11.0;

/// The cathedral's centre-line.
const CX: f32 = 640.0;

/// How many arch panes.
const ARCHES: usize = 5;

/// The rose window's centre + radius.
const ROSE: (f32, f32, f32) = (640.0, 300.0, 84.0);

// ── The frame ───────────────────────────────────────────────────────────────

/// The lantern's depth this frame: 0 = far (deep end), 1 = near (camera).
#[must_use]
fn lantern_depth(t: f32) -> f32 {
    ease_in_out(t)
}

/// A pane's σ: distance from the focal plane (the lantern), mapped to blur.
#[must_use]
fn pane_sigma(i: usize, t: f32) -> f32 {
    let d_i = i as f32 / (ARCHES - 1) as f32;
    0.6 + 11.0 * (d_i - lantern_depth(t)).abs()
}

pub fn frame(t: f32) -> WidgetNode {
    let focus = lantern_depth(t);

    // The board: the nave's stone, the altar's glow, the arches' geometry.
    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The stone dark.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(8, 8, 12)),
                    (0.6, BG_DEEP),
                    (1.0, Color::rgb(4, 4, 7)),
                ]),
            );

            // The altar glow — far down the nave, behind everything.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::radial(Offset::new(0.5, 0.42), 0.34).with_dither().with_stops(&[
                    (0.0, alpha(AMBER, 0.14)),
                    (0.55, alpha(VIOLET, 0.05)),
                    (1.0, alpha(VIOLET, 0.0)),
                ]),
            );

            // The floor — perspective lines toward the altar.
            for k in -6..=6 {
                let x = 640.0 + k as f32 * 128.0;
                book.line(
                    Offset::new(640.0 + k as f32 * 26.0, 430.0),
                    Offset::new(x, h),
                    alpha(FAINT, 0.13),
                    1.0,
                );
            }
            book.line(Offset::new(0.0, 430.0), Offset::new(w, 430.0), alpha(FAINT, 0.22), 1.2);

            // The arches' stone edges (crisp, outside the frosted groups) —
            // drawn later, on top, so the frost reads as glass not fog.
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));

    // ── The frosted panes, deepest first so nearer ones composite over ──
    for i in (0..ARCHES).rev() {
        let d_i = i as f32 / (ARCHES - 1) as f32;
        // Depth → geometry: the deep end is small and high, the near end
        // fills the frame.
        let scale = 0.30 + 0.70 * d_i;
        let aw = 220.0 + 1000.0 * scale;
        let ah = 250.0 + 440.0 * scale;
        let ax = CX - aw * 0.5;
        let ay = 408.0 - ah;
        let sigma = pane_sigma(i, t);

        // The pane's own content: a translucent glass wash + mullions,
        // the wash itself cut to the arch silhouette (uprights + half-turn
        // top) so no Clip widget is needed — the glass IS the arch.
        let pane = Painting::sized(
            Size::new(aw, ah),
            PaintWith::new(move |g: &mut Sketchbook, sz: Size| {
                let pw = sz.width;
                let ph = sz.height;
                // One subpath, the arch: uprights + a two-cubic semicircle
                // top (the circle-approximating κ — `append_arc` is the
                // rasterizer's private helper, so the pane builds its own).
                let kappa = 0.5523;
                let (acx, acy, ar) = (pw * 0.5, pw * 0.5, pw * 0.5);
                let mut arch = Path::new();
                arch.move_to(Offset::new(0.0, ph));
                arch.line_to(Offset::new(0.0, acy));
                arch.cubic_to(
                    Offset::new(acx - ar, acy - ar * kappa),
                    Offset::new(acx - ar * kappa, acy - ar),
                    Offset::new(acx, acy - ar),
                );
                arch.cubic_to(
                    Offset::new(acx + ar * kappa, acy - ar),
                    Offset::new(acx + ar, acy - ar * kappa),
                    Offset::new(acx + ar, acy),
                );
                arch.line_to(Offset::new(pw, ph));
                arch.close();
                g.fill(
                    arch,
                    Gradient::vertical().with_dither().with_stops(&[
                        (0.0, alpha(mix(VIOLET_SOFT, CYAN_SOFT, d_i), 0.16)),
                        (1.0, alpha(VIOLET_DEEPISH, 0.20)),
                    ]),
                );
                // Mullions: the leading that makes it a window.
                let n = 3 + i;
                for k in 1..n {
                    let x = pw * k as f32 / n as f32;
                    g.line(Offset::new(x, pw * 0.5), Offset::new(x, ph), alpha(INK, 0.10), 2.0);
                }
                g.line(Offset::new(0.0, ph * 0.55), Offset::new(pw, ph * 0.55), alpha(INK, 0.10), 2.0);
            }),
        );

        stack = stack.push(
            Positioned::new()
                .left(ax)
                .top(ay)
                .width(aw)
                .height(ah)
                .child(Filtered::new().with_blur(sigma).child(pane)),
        );
    }

    // The stone arch edges, over the frost.
    let edges = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            for i in (0..ARCHES).rev() {
                let d_i = i as f32 / (ARCHES - 1) as f32;
                let scale = 0.30 + 0.70 * d_i;
                let aw = 220.0 + 1000.0 * scale;
                let ah = 250.0 + 440.0 * scale;
                let ax = CX - aw * 0.5;
                let ay = 408.0 - ah;
                // The arch: two uprights + a semicircular top, one stroke —
                // the same two-cubic semicircle the pane built.
                let kappa = 0.5523;
                let (acx, acy, ar) = (CX, ay + aw * 0.5, aw * 0.5);
                let mut p = Path::new();
                p.move_to(Offset::new(ax, 408.0));
                p.line_to(Offset::new(ax, acy));
                p.cubic_to(
                    Offset::new(acx - ar, acy - ar * kappa),
                    Offset::new(acx - ar * kappa, acy - ar),
                    Offset::new(acx, acy - ar),
                );
                p.cubic_to(
                    Offset::new(acx + ar * kappa, acy - ar),
                    Offset::new(acx + ar, acy - ar * kappa),
                    Offset::new(acx + ar, acy),
                );
                p.line_to(Offset::new(ax + aw, 408.0));
                book.stroke(p, alpha(mix(FAINT, INK, 0.24 - d_i * 0.12), 0.55), 2.2 - d_i);
            }
        }),
    );
    stack = stack.push(Positioned::fill().child(edges));

    // ── THE ROSE WINDOW — nesting: a Filtered ring inside a Filtered ring,
    //    and the outer σ sweeps 24 → 0.5 (the extremes, both ends). ──
    let rose_sweep = 0.5 + 23.5 * (1.0 - t);
    let (rx, ry, rr) = ROSE;
    let rose_inner = Painting::sized(
        Size::new(rr * 2.0, rr * 2.0),
        PaintWith::new(move |g: &mut Sketchbook, _sz: Size| {
            // Petal spokes + a hot core.
            for k in 0..12 {
                let a = k as f32 * std::f32::consts::TAU / 12.0;
                g.line(
                    Offset::new(rr, rr),
                    Offset::new(rr + a.cos() * rr * 0.92, rr + a.sin() * rr * 0.92),
                    alpha(AMBER, 0.5),
                    2.0,
                );
            }
            g.circle(Offset::new(rr, rr), rr * 0.30, alpha(Color::rgb(255, 240, 210), 0.8));
            g.ring(Offset::new(rr, rr), rr * 0.62, 2.4, alpha(VIOLET_SOFT, 0.6));
        }),
    );
    let rose = Stack::new()
        .push(
            // Inner ring: a modest blur, nested INSIDE the outer.
            Positioned::new()
                .left(rx - rr)
                .top(ry - rr)
                .width(rr * 2.0)
                .height(rr * 2.0)
                .child(Filtered::new().with_blur(2.2).child(rose_inner)),
        );
    stack = stack.push(
        Positioned::new()
            .left(rx - rr)
            .top(ry - rr)
            .width(rr * 2.0)
            .height(rr * 2.0)
            .child(
                // Outer: the sweep σ, wrapping the nested inner — blur of blur.
                Filtered::new().with_blur(rose_sweep).child(rose),
            ),
    );

    // ── THE LANTERN — always crisp, travelling the depth field ──
    let ld = lantern_depth(t);
    let lx = 640.0 + (1.0 - ld) * -40.0 + (t * 9.4).sin() * 14.0;
    let ly = 300.0 + (1.0 - ld) * 60.0 + (t * 12.0).sin() * 6.0;
    let ls = 0.42 + 0.78 * ld;
    // The chain — a full-frame overlay stroke, so it hangs from the frame's
    // own top down to the lantern's ring (paintings clip to their bounds,
    // so the chain cannot ride inside the lantern's own 120×200 canvas).
    let chain = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |g: &mut Sketchbook, _sz: Size| {
            g.line(Offset::new(lx, 0.0), Offset::new(lx, ly - 100.0 * ls + 12.0), alpha(FAINT, 0.38), 1.1);
        }),
    );
    stack = stack.push(Positioned::fill().child(chain));
    let lantern = Painting::sized(
        Size::new(120.0 * ls, 200.0 * ls),
        PaintWith::new(move |g: &mut Sketchbook, sz: Size| {
            let w = sz.width;
            let h = sz.height;
            let cx = w * 0.5;
            // The body: a hexagonal lantern silhouette.
            let bw = w * 0.42;
            let bh = h * 0.52;
            let top = h * 0.24;
            let mut body = Path::new();
            body.move_to(Offset::new(cx - bw * 0.5, top + bh * 0.12));
            body.line_to(Offset::new(cx - bw * 0.5, top + bh * 0.88));
            body.line_to(Offset::new(cx, top + bh));
            body.line_to(Offset::new(cx + bw * 0.5, top + bh * 0.88));
            body.line_to(Offset::new(cx + bw * 0.5, top + bh * 0.12));
            body.close();
            g.fill(body, alpha(Color::rgb(20, 20, 28), 0.97));
            // The flame.
            g.circle(Offset::new(cx, top + bh * 0.5), w * 0.13,
                Gradient::radial_fill().with_stops(&[
                    (0.0, Color::rgb(255, 252, 236)),
                    (0.55, alpha(AMBER, 0.95)),
                    (1.0, alpha(AMBER, 0.0)),
                ]));
            // The cap + ring.
            g.fill(
                {
                    let mut p = Path::new();
                    p.move_to(Offset::new(cx - bw * 0.34, top + bh * 0.10));
                    p.line_to(Offset::new(cx, top - h * 0.02));
                    p.line_to(Offset::new(cx + bw * 0.34, top + bh * 0.10));
                    p.close();
                    p
                },
                alpha(Color::rgb(26, 26, 36), 1.0),
            );
            g.ring(Offset::new(cx, top - h * 0.04), w * 0.16, 1.4, alpha(FAINT, 0.8));
        }),
    );
    // The lantern's glow — one Plus group (blurred once, warm).
    let glow = Painting::sized(
        Size::new(260.0, 260.0),
        PaintWith::new(move |g: &mut Sketchbook, _sz: Size| {
            g.blended_layer(0.9, 9.0, BlendMode::Plus, None, |gl| {
                gl.circle(Offset::new(130.0, 130.0), 92.0, alpha(AMBER, 0.16));
                gl.circle(Offset::new(130.0, 130.0), 52.0, alpha(Color::rgb(255, 220, 160), 0.14));
            });
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(lx - 130.0 * ls)
            .top(ly - 130.0 * ls)
            .width(260.0 * ls)
            .height(260.0 * ls)
            .child(glow),
    );
    stack = stack.push(
        Positioned::new()
            .left(lx - 60.0 * ls)
            .top(ly - 100.0 * ls)
            .width(120.0 * ls)
            .height(200.0 * ls)
            .child(lantern),
    );

    // ── THE CAPTION PANE — backdrop blur + a matrix chain (3 → 1 pass) ──
    let caption = Filtered::new()
        .with_blur(5.0)
        .with_backdrop()
        .sepia()
        .brightness(0.92)
        .tint(VIOLET, 0.18)
        .child(
            Painting::sized(
                Size::new(360.0, 74.0),
                PaintWith::new(move |g: &mut Sketchbook, _sz: Size| {
                    g.rrect(Rect::new(0.0, 0.0, 360.0, 74.0), 12.0, alpha(Color::rgb(255, 250, 240), 0.05));
                    g.stroke_rrect(Rect::new(0.0, 0.0, 360.0, 74.0), 12.0, alpha(INK, 0.25), 1.0);
                }),
            ),
        );
    stack = stack.push(
        Positioned::new()
            .left(852.0)
            .top(560.0)
            .width(360.0)
            .height(74.0)
            .child(caption),
    );
    stack = stack.push(
        Positioned::new()
            .left(876.0)
            .top(584.0)
            .width(340.0)
            .height(30.0)
            .child(
                Text::new("backdrop σ5 · sepia∘bright∘tint = 1 matrix".to_string()).style(
                    TextStyle::new(13.0).monospace().color(alpha(INK, 0.85)),
                ),
            ),
    );

    stack.push(receipt_panel(t, rose_sweep)).into()
}

/// A stand-in colour the panes fade toward (kept out of film_lib — one
/// plate's private mixing, not a palette decision).
const VIOLET_DEEPISH: Color = Color::rgb(44, 30, 78);

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(t: f32, rose_sweep: f32) -> WidgetNode {
    let sigmas: Vec<String> = (0..ARCHES)
        .map(|i| format!("{:.1}", pane_sigma(i, t)))
        .collect();
    let lines = [
        "FILTERSTACK · THE COMPOSITOR-DEPTH AXIS".to_string(),
        format!("pane σ [{}] · focus = lantern depth", sigmas.join(" ")),
        format!("rose σ sweep {rose_sweep:.1} (24→0.5) · nesting 2"),
        format!("filtered groups 8/32 (guard) · matrix chain 3→1"),
        "5 panes + rose×2 + backdrop · Plus glow ×1".to_string(),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 486.0;

    let mut stack = Stack::new();
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 16.0)
                .width(430.0)
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

    // The instrument: the σ ladder, live — five rungs, the rung nearest
    // the focus lit (blur spent where the subject is NOT).
    let strip = Painting::sized(
        Size::new(400.0, 54.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            book.rrect(
                Rect::new(0.0, 0.0, 400.0, 54.0),
                8.0,
                alpha(Color::rgb(16, 16, 21), 0.88),
            );
            book.stroke_rrect(
                Rect::new(0.0, 0.0, 400.0, 54.0),
                8.0,
                alpha(Color::WHITE, 0.08),
                1.0,
            );
            let focus = lantern_depth(t);
            for i in 0..ARCHES {
                let x = 14.0 + i as f32 * ((400.0 - 28.0) / (ARCHES - 1) as f32);
                let sig = pane_sigma(i, t);
                let bar = (sig / 12.0).clamp(0.0, 1.0) * 34.0;
                let near = ((i as f32 / (ARCHES - 1) as f32) - focus).abs() < 0.34;
                book.rect(
                    Rect::new(x - 7.0, 44.0 - bar, x + 7.0, 44.0),
                    alpha(if near { AMBER } else { VIOLET_SOFT }, if near { 0.7 } else { 0.5 }),
                );
            }
            // The focus playhead.
            let fx = 14.0 + focus * (400.0 - 28.0);
            book.line(Offset::new(fx, 8.0), Offset::new(fx, 48.0), alpha(INK, 0.8), 1.2);
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(P_X)
            .top(P_Y + 84.0)
            .width(400.0)
            .height(54.0)
            .child(strip),
    );

    stack.into()
}
