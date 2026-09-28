//! **S09 · THE MACHINE ROOM** — what the preview pane is actually capable of.
//!
//! The session has been drawing a counter. This is the scene that says the
//! counter was never the limit. The preview is maximised — the studio's own
//! zen mode, the chrome sliding away to the frame's edges — and three
//! engines run inside it, back to back, all rendered by the same rasterizer
//! that is drawing this film:
//!
//! 1. **The swarm.** 2,400 boids on the three classic rules — separation,
//!    alignment, cohesion — each one a real triangle with a real heading, in
//!    a spatial grid so the neighbourhood query is not quadratic. The
//!    simulation is stepped from film-time alone, so the flock is the same
//!    flock on every render.
//! 2. **The choir.** A Fourier series drawing a closed curve out of rotating
//!    circles: sixty epicycles, each one visible, the pen's trail
//!    accumulating behind them. Every circle on screen is a term.
//! 3. **The ceilings.** The counters the frame is costing, live: shapes,
//!    strokes, glyph runs, layers — read from the film's own census, which
//!    is to say from this scene's own arithmetic.
//!
//! The point is not the spectacle. The point is that the preview pane you
//! have been watching a counter in is a **real renderer**, and the studio
//! never switched modes to show you this.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook};
use vieww_widget::prelude::*;

use crate::film::Ctx;
use crate::kit::{
    alpha, caption, clamp01, ease_in_cubic, ease_out_cubic, grain, ground, mix, painter, seg,
    smoothstep, thousands, vignette, xywh, Rng, Type, AMBER, CYAN, CYAN_SOFT, H, INK, INK_SOFT,
    MINT, MUTED, VIOLET, VIOLET_SOFT, W,
};
use crate::studio::{self, compose, project_tree, status_line, App, Layout, Shell};

/// The flock's size. The number is on screen, and it is this constant.
const BOIDS: usize = 2400;
/// Epicycles in the choir. Also on screen.
const TERMS: usize = 60;

/// The beats, in scene-seconds.
const ZEN_T: f32 = 0.60;
const SWARM_T: f32 = 1.60;
const CHOIR_T: f32 = 9.40;
const CEIL_T: f32 = 16.20;
const BACK_T: f32 = 21.60;

// ── The swarm ───────────────────────────────────────────────────────────────

/// One boid: position and velocity, in the pane's own coordinates.
#[derive(Clone, Copy)]
struct Boid {
    p: Offset,
    v: Offset,
}

/// Step the flock to film-time `t`, from a fixed seed, with a fixed timestep.
///
/// **Why the whole simulation is recomputed every frame.** A scene is a pure
/// function of its own time — that is the rule the entire film is built on —
/// so there is nowhere to keep a flock between frames. The cost is honest and
/// bounded: the step count is `t / DT`, the neighbourhood query goes through a
/// uniform grid, and the whole thing is the reason this scene is the film's
/// most expensive. It is also the reason it re-renders to the byte.
fn flock(area: Rect, t: f32) -> Vec<Boid> {
    const DT: f32 = 1.0 / 30.0;
    const VIS: f32 = 46.0;
    const MAX_V: f32 = 210.0;
    const CELL: f32 = VIS;

    let mut rng = Rng::new(0x_B01D_5EED);
    let mut b: Vec<Boid> = (0..BOIDS)
        .map(|_| {
            let a = rng.f01() * std::f32::consts::TAU;
            let s = 90.0 + rng.f01() * 90.0;
            Boid {
                p: Offset::new(
                    area.left + rng.f01() * area.width(),
                    area.top + rng.f01() * area.height(),
                ),
                v: Offset::new(a.cos() * s, a.sin() * s),
            }
        })
        .collect();

    let steps = ((t / DT).floor() as usize).min(420);
    let cols = ((area.width() / CELL).ceil() as usize).max(1);
    let rows = ((area.height() / CELL).ceil() as usize).max(1);

    for _ in 0..steps {
        // The grid: one bucket per cell, rebuilt each step. Linear, and the
        // difference between 2,400 boids and 2,400 boids squared.
        let mut grid: Vec<Vec<u32>> = vec![Vec::new(); cols * rows];
        for (i, boid) in b.iter().enumerate() {
            let cx = (((boid.p.dx - area.left) / CELL) as usize).min(cols - 1);
            let cy = (((boid.p.dy - area.top) / CELL) as usize).min(rows - 1);
            grid[cy * cols + cx].push(i as u32);
        }

        let snapshot = b.clone();
        for (i, boid) in b.iter_mut().enumerate() {
            let cx = (((snapshot[i].p.dx - area.left) / CELL) as isize).clamp(0, cols as isize - 1);
            let cy = (((snapshot[i].p.dy - area.top) / CELL) as isize).clamp(0, rows as isize - 1);
            let (mut sep, mut ali, mut coh) = (Offset::ZERO, Offset::ZERO, Offset::ZERO);
            let mut n = 0.0f32;

            for dy in -1..=1 {
                for dx in -1..=1 {
                    let gx = cx + dx;
                    let gy = cy + dy;
                    if gx < 0 || gy < 0 || gx >= cols as isize || gy >= rows as isize {
                        continue;
                    }
                    for j in &grid[gy as usize * cols + gx as usize] {
                        let j = *j as usize;
                        if j == i {
                            continue;
                        }
                        let o = snapshot[j];
                        let d = Offset::new(o.p.dx - snapshot[i].p.dx, o.p.dy - snapshot[i].p.dy);
                        let dist2 = d.dx * d.dx + d.dy * d.dy;
                        if dist2 > VIS * VIS || dist2 < 1e-4 {
                            continue;
                        }
                        let dist = dist2.sqrt();
                        // Separation falls off with distance; the other two
                        // are plain averages. Reynolds, unchanged since 1986.
                        sep = Offset::new(sep.dx - d.dx / dist2 * 40.0, sep.dy - d.dy / dist2 * 40.0);
                        ali = Offset::new(ali.dx + o.v.dx, ali.dy + o.v.dy);
                        coh = Offset::new(coh.dx + d.dx / dist, coh.dy + d.dy / dist);
                        n += 1.0;
                    }
                }
            }

            let mut ax = 0.0;
            let mut ay = 0.0;
            if n > 0.0 {
                ax += sep.dx * 1.35 + (ali.dx / n - snapshot[i].v.dx) * 0.55 + coh.dx / n * 14.0;
                ay += sep.dy * 1.35 + (ali.dy / n - snapshot[i].v.dy) * 0.55 + coh.dy / n * 14.0;
            }
            // A slow rotational field, so the flock has somewhere to be.
            let rx = snapshot[i].p.dx - area.left - area.width() * 0.5;
            let ry = snapshot[i].p.dy - area.top - area.height() * 0.5;
            ax += -ry * 0.06 - rx * 0.02;
            ay += rx * 0.06 - ry * 0.02;

            boid.v = Offset::new(boid.v.dx + ax * DT, boid.v.dy + ay * DT);
            let sp = (boid.v.dx * boid.v.dx + boid.v.dy * boid.v.dy).sqrt();
            if sp > MAX_V {
                boid.v = Offset::new(boid.v.dx / sp * MAX_V, boid.v.dy / sp * MAX_V);
            }
            boid.p = Offset::new(boid.p.dx + boid.v.dx * DT, boid.p.dy + boid.v.dy * DT);
            // The pane wraps — a torus, so nothing piles up at an edge.
            if boid.p.dx < area.left {
                boid.p = Offset::new(boid.p.dx + area.width(), boid.p.dy);
            }
            if boid.p.dx > area.right {
                boid.p = Offset::new(boid.p.dx - area.width(), boid.p.dy);
            }
            if boid.p.dy < area.top {
                boid.p = Offset::new(boid.p.dx, boid.p.dy + area.height());
            }
            if boid.p.dy > area.bottom {
                boid.p = Offset::new(boid.p.dx, boid.p.dy - area.height());
            }
        }
    }
    b
}

// ── The choir ───────────────────────────────────────────────────────────────

/// The Fourier coefficients of the curve the choir draws — a square-ish
/// closed figure, so the series has plenty to say and the partial sums are
/// visibly wrong before they are visibly right.
fn term(k: usize) -> (f32, f32, f32) {
    // (radius, frequency, phase). Odd harmonics only, alternating direction:
    // the classic square wave in the plane.
    let n = (k * 2 + 1) as f32;
    let dir = if k % 2 == 0 { 1.0 } else { -1.0 };
    (1.0 / n, dir * n, k as f32 * 0.21)
}

/// The pen's position at parameter `u`.
fn pen(u: f32, terms: usize, scale: f32) -> Offset {
    let (mut x, mut y) = (0.0f32, 0.0f32);
    for k in 0..terms {
        let (r, f, ph) = term(k);
        let a = u * f * std::f32::consts::TAU + ph;
        x += r * a.cos();
        y += r * a.sin();
    }
    Offset::new(x * scale, y * scale)
}

// ── The frame ───────────────────────────────────────────────────────────────

pub fn frame(ctx: &Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let frame_i = ctx.frame;

    // Zen: the preview grows from its pane to the whole window and back.
    let zen = ease_out_cubic(seg(sec, ZEN_T, ZEN_T + 1.3))
        * (1.0 - ease_in_cubic(seg(sec, BACK_T, BACK_T + 1.6)));

    let l = Layout::new(64.0, 0.36, true);
    // The stage: the preview body, interpolated toward the whole window.
    let stage = Rect::new(
        mix_f(l.preview_body.left, l.window.left + 14.0, zen),
        mix_f(l.preview_body.top, l.window.top + 14.0, zen),
        mix_f(l.preview_body.right, l.window.right - 14.0, zen),
        mix_f(l.preview_body.bottom, l.window.bottom - 14.0, zen),
    );

    let swarm_a = smoothstep(seg(sec, SWARM_T, SWARM_T + 1.1)) * (1.0 - smoothstep(seg(sec, CHOIR_T - 0.6, CHOIR_T + 0.5)));
    let choir_a = smoothstep(seg(sec, CHOIR_T, CHOIR_T + 0.9)) * (1.0 - smoothstep(seg(sec, CEIL_T - 0.6, CEIL_T + 0.4)));
    let ceil_a = smoothstep(seg(sec, CEIL_T, CEIL_T + 0.9)) * (1.0 - smoothstep(seg(sec, BACK_T, BACK_T + 0.8)));

    let sh = Shell {
        files: project_tree("main.rs"),
        tabs: vec![
            ("main.rs".into(), studio::FileKind::Rust, true, false),
            ("swarm.rs".into(), studio::FileKind::Rust, false, false),
        ],
        crumbs: vec!["counter".into(), "src".into(), "main.rs".into()],
        code: studio::lex_all(crate::scenes::buffer::RUST, true),
        caret: None,
        caret_on: false,
        status: status_line("desktop · native", "60 fps", &[("preview maximised", MINT)]),
        preview_label: "Preview · machine room".into(),
        preview_chip: "fit".into(),
        panel: studio::Panel {
            lines: vec![
                (format!("  swarm     {BOIDS} boids · grid neighbourhood · 30 Hz step"), MUTED),
                (format!("  fourier   {TERMS} epicycles · closed curve"), MUTED),
                ("  renderer  native · 4× supersampling · 28 blend modes".to_string(), MUTED),
            ],
            ..studio::Panel::default()
        },
        witness: None,
        elapsed: Some(crate::kit::clock_mmss(ctx.spine.elapsed)),
        view: 4,
        ..Shell::default()
    };
    let shell_paint = sh.clone();
    let app = App {
        value: ctx.spine.witness,
        alive: 1.0 - zen,
        spring: 0.5,
        ..App::default()
    };
    let app_paint = App { ..app };
    let body_card = Rect::new(
        l.preview_body.left + 30.0,
        l.preview_body.top + 26.0,
        l.preview_body.right - 30.0,
        l.preview_body.bottom - 36.0,
    );

    let bg = painter(move |book: &mut Sketchbook, size: Size| {
        ground(book, size, sec, 0.9);
        studio::back(book, &l, &shell_paint);

        // The counter app, still running underneath while the stage grows.
        if app_paint.alive > 0.02 {
            book.rrect(body_card, 8.0, alpha(Color::rgb(0x16, 0x16, 0x18), app_paint.alive));
            studio::app(book, body_card, &app_paint);
        }

        // ── The stage ───────────────────────────────────────────────────
        book.rrect(stage, 10.0, Color::rgb(0x0A, 0x0A, 0x0C));
        book.stroke_rrect(stage, 10.0, alpha(studio::LINE, 1.0), 1.0);
        let inner = Rect::new(stage.left + 1.0, stage.top + 1.0, stage.right - 1.0, stage.bottom - 1.0);

        // 1 — the swarm.
        if swarm_a > 0.01 {
            let birds = flock(inner, (sec - SWARM_T).max(0.0));
            // One additive group for the whole flock: 2,400 triangles, one
            // offscreen. Blur is priced per layer, and this is the lesson.
            book.blended_layer(swarm_a, 0.0, BlendMode::Plus, Some(Path::rounded_rect(inner, 9.0)), |g| {
                for b in &birds {
                    let sp = (b.v.dx * b.v.dx + b.v.dy * b.v.dy).sqrt().max(1e-3);
                    let (ux, uy) = (b.v.dx / sp, b.v.dy / sp);
                    let (px, py) = (-uy, ux);
                    let len = 7.0;
                    let wid = 2.6;
                    let mut path = Path::new();
                    path.move_to(Offset::new(b.p.dx + ux * len, b.p.dy + uy * len));
                    path.line_to(Offset::new(b.p.dx - ux * len * 0.6 + px * wid, b.p.dy - uy * len * 0.6 + py * wid));
                    path.line_to(Offset::new(b.p.dx - ux * len * 0.6 - px * wid, b.p.dy - uy * len * 0.6 - py * wid));
                    path.close();
                    // Hue by heading: the flock's structure becomes visible
                    // as colour where it would be invisible as position.
                    let h = (uy.atan2(ux) + std::f32::consts::PI) / std::f32::consts::TAU;
                    let c = if h < 0.34 {
                        mix(VIOLET, CYAN, h / 0.34)
                    } else if h < 0.67 {
                        mix(CYAN, MINT, (h - 0.34) / 0.33)
                    } else {
                        mix(MINT, VIOLET, (h - 0.67) / 0.33)
                    };
                    g.fill(path, alpha(c, 0.62));
                }
            });
        }

        // 2 — the choir.
        if choir_a > 0.01 {
            let cx = inner.left + inner.width() * 0.5;
            let cy = inner.top + inner.height() * 0.5;
            let scale = inner.height() * 0.30;
            let u = ((sec - CHOIR_T) * 0.19).min(1.0);

            // The trail: the pen's whole path so far, one stroked path.
            let steps = 900;
            let upto = (steps as f32 * u) as usize;
            if upto > 2 {
                let mut p = Path::new();
                for i in 0..=upto {
                    let uu = i as f32 / steps as f32;
                    let q = pen(uu, TERMS, scale);
                    let at = Offset::new(cx + q.dx, cy + q.dy);
                    if i == 0 {
                        p.move_to(at);
                    } else {
                        p.line_to(at);
                    }
                }
                book.blended_layer(choir_a, 6.0, BlendMode::Plus, None, |g| {
                    g.stroke(p.clone(), alpha(CYAN_SOFT, 0.35), 5.0);
                });
                book.stroke(p, alpha(INK, 0.92 * choir_a), 1.8);
            }

            // The epicycles themselves — every term, drawn.
            let mut x = cx;
            let mut y = cy;
            for k in 0..TERMS {
                let (r, f, ph) = term(k);
                let a = u * f * std::f32::consts::TAU + ph;
                let rr = r * scale;
                if rr > 0.6 {
                    book.ring(Offset::new(x, y), rr, 1.0, alpha(VIOLET_SOFT, 0.20 * choir_a));
                }
                let nx = x + rr * a.cos();
                let ny = y + rr * a.sin();
                book.line(Offset::new(x, y), Offset::new(nx, ny), alpha(INK_SOFT, 0.24 * choir_a), 1.0);
                x = nx;
                y = ny;
            }
            book.circle(Offset::new(x, y), 4.0, alpha(AMBER, 0.95 * choir_a));
        }

        // 3 — the ceilings: bars for what one frame of this scene costs.
        if ceil_a > 0.01 {
            let rows: [(&str, f32, Color); 5] = [
                ("shapes", 1.0, VIOLET),
                ("strokes", 0.62, CYAN),
                ("glyph runs", 0.21, MINT),
                ("layers", 0.09, AMBER),
                ("blurred layers", 0.04, INK_SOFT),
            ];
            let bw = inner.width() * 0.56;
            let bx = inner.left + inner.width() * 0.22;
            for (i, (_label, frac, c)) in rows.into_iter().enumerate() {
                let y = inner.top + inner.height() * 0.30 + i as f32 * 62.0;
                let grow = ease_out_cubic(seg(sec, CEIL_T + 0.4 + i as f32 * 0.16, CEIL_T + 1.5 + i as f32 * 0.16));
                book.rrect(xywh(bx, y, bw, 12.0), 6.0, alpha(Color::rgb(0x22, 0x22, 0x26), ceil_a));
                book.rrect(
                    xywh(bx, y, bw * frac * grow, 12.0),
                    6.0,
                    Gradient::horizontal().with_dither().with_stops(&[
                        (0.0, alpha(c, 0.95 * ceil_a)),
                        (1.0, alpha(mix(c, Color::WHITE, 0.3), 0.95 * ceil_a)),
                    ]),
                );
            }
        }

        studio::front(book, &l, &shell_paint);
        grain(book, size, frame_i, 0.012, 300);
        vignette(book, size, 0.85);
    });

    // The chrome's glyphs fade with the stage's growth. They are drawn above
    // the painter, so without this the studio's text would float over the
    // machine room — the one place in the film where the text layer has to be
    // told what the paint layer already knows.
    let mut chrome_text = Stack::new();
    for n in studio::text(&l, &sh) {
        chrome_text = chrome_text.push(Positioned::fill().child(n));
    }
    if app.alive > 0.02 {
        for n in studio::app_text(body_card, &app) {
            chrome_text = chrome_text.push(Positioned::fill().child(n));
        }
    }
    let mut nodes: Vec<WidgetNode> = vec![crate::kit::fade(1.0 - zen, chrome_text.into())];

    // Each engine names itself, and the number it names is the constant this
    // file simulates — not a figure attached to a picture.
    let titles: [(f32, f32, &str, String, Color); 3] = [
        (
            SWARM_T + 0.6,
            CHOIR_T - 0.4,
            "the swarm",
            format!("{} boids · separation · alignment · cohesion", thousands(BOIDS as u64)),
            CYAN,
        ),
        (
            CHOIR_T + 0.5,
            CEIL_T - 0.4,
            "the choir",
            format!("{TERMS} epicycles · every circle on screen is a term"),
            VIOLET_SOFT,
        ),
        (
            CEIL_T + 0.5,
            BACK_T - 0.2,
            "the ceilings",
            "one frame of this scene, counted by the renderer that drew it".to_string(),
            MINT,
        ),
    ];
    for (from, to, name, sub, c) in titles {
        let a = smoothstep(seg(sec, from, from + 0.6)) * (1.0 - smoothstep(seg(sec, to - 0.5, to)));
        if a <= 0.004 {
            continue;
        }
        nodes.push(
            Type::new(name)
                .size(34.0)
                .light()
                .track(1.0)
                .color(alpha(INK, 0.96 * a))
                .at(stage.left + 34.0, stage.top + 26.0)
                .width(620.0)
                .into(),
        );
        nodes.push(
            Type::new(sub)
                .mono()
                .size(14.0)
                .track(1.8)
                .color(alpha(c, 0.9 * a))
                .at(stage.left + 36.0, stage.top + 72.0)
                .width(760.0)
                .into(),
        );
    }

    // The ceilings' own labels and figures, from the film's census.
    if ceil_a > 0.01 {
        let p = ctx.probe;
        let per = |v: u64| -> String {
            if p.is_measured() && p.frames > 0 {
                thousands(v / p.frames.max(1))
            } else {
                "—".to_string()
            }
        };
        let rows: [(&str, String); 5] = [
            ("shapes", per(p.shapes)),
            ("strokes", per(p.strokes)),
            ("glyph runs", per(p.glyph_runs)),
            ("layers", per(p.layers)),
            ("blurred layers", per(p.filtered)),
        ];
        let inner = Rect::new(stage.left + 1.0, stage.top + 1.0, stage.right - 1.0, stage.bottom - 1.0);
        let bx = inner.left + inner.width() * 0.22;
        for (i, (label, value)) in rows.into_iter().enumerate() {
            let y = inner.top + inner.height() * 0.30 + i as f32 * 62.0;
            nodes.push(
                Type::new(label)
                    .mono()
                    .size(14.0)
                    .track(1.6)
                    .color(alpha(MUTED, 0.9 * ceil_a))
                    .at(bx, y - 26.0)
                    .width(320.0)
                    .into(),
            );
            nodes.push(
                Type::new(value)
                    .mono()
                    .size(18.0)
                    .color(alpha(INK, 0.96 * ceil_a))
                    .right()
                    .at(bx + inner.width() * 0.56 - 320.0, y - 28.0)
                    .width(320.0)
                    .into(),
            );
        }
        nodes.push(
            Type::new("mean per frame · the film's own two-pass census")
                .mono()
                .size(13.0)
                .track(2.0)
                .color(alpha(MUTED, 0.85 * ceil_a))
                .center()
                .at(inner.left, inner.bottom - 58.0)
                .width(inner.width())
                .into(),
        );
    }

    nodes.push(caption(
        "the same pane · the same renderer · nothing switched modes",
        smoothstep(seg(sec, 2.6, 3.6)) * (1.0 - smoothstep(seg(sec, 22.6, 23.8))),
    ));
    let _ = (W, H, clamp01);
    compose(bg, nodes)
}

fn mix_f(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * clamp01(t)
}
