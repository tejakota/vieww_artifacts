//! Movement II, deepened — three scenes the engine act earned.
//!
//! * **Z10E · the shutter.** The engine composites its own cinema: the same
//!   frame function, rendered once sharp and once through a real shutter —
//!   `vieww-video`'s motion blur, samples averaged in linear light. The
//!   angle sweeps live, and the receipt is measured from the pixels.
//! * **Z10F · the cook.** The engine's dataflow heart: a `vieww-graph`
//!   operator network — Time → LFO → × → Noise → Remap → Trail — cooked
//!   lazily by dirty flags. The constant cooks **once**; the clock cooks
//!   every frame; the Trail remembers the last sixty-four samples. The
//!   counters on screen are the graph's own.
//! * **Z10G · the swarm.** Systems that behave: a `vieww-game` ECS world
//!   stepped on the fixed loop, agents routed by A* through a maze whose
//!   wall opens under them mid-scene. The receipt is the loop's own stats.
//!
//! Every scene keeps the film's contract: a frame is a pure function of its
//! index. The graph and the world are *replayed* from zero each frame —
//! state that accumulates across frames is reconstructed, never remembered
//! by the process, so a resume renders the same pixels.

use vieww_foundation::{Color, Offset, Rect, Size, Sketchbook, TextAlign};
use vieww_widget::prelude::*;

use crate::film_lib::{clamp01, ease_in_out, ease_out_cubic, ease_out_expo};
use crate::product_film as pf;
use super::{
    frame, filmkit, ACCENT, BRAND_FAR, BRAND_NEAR, CANVAS, FPS, GROUND, INK, LEDGER, MUTED,
    SURFACE, SURFACE_2, SYN_COMMENT, SYN_FUNCTION, SYN_MACRO, SYN_NUMBER, SYN_STRING, SYN_TYPE,
    TERM_GREEN, W,
};

// ── Z10E · the shutter ──────────────────────────────────────────────────────

/// The sub-render the shutter averages: one subject, one time.
///
/// A comet on a fast lissajous through a field of dashes and three thin
/// rings — motion fast enough that a 180° shutter smears visibly at 60 fps.
/// Rendered by the film's own rasterizer at panel resolution, pure in `t`.
fn shutter_subject(t: f64, w: u32, h: u32) -> vieww_foundation::Image {
    use std::time::Duration;
    use vieww_paint::native::NativeRenderer;
    use vieww_render::FrameDriver;

    let size = Size::new(w as f32, h as f32);
    let mut driver = FrameDriver::new(size);
    driver.set_root(Positioned::fill().child(Painting::sized(
        size,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            book.rect(Rect::new(0.0, 0.0, w, h), Color::rgb(8, 7, 12));
            let tf = t as f32;
            // The dash field — a static reference the motion reads against.
            let mut rng = crate::film_lib::Rng::new(0x5EED);
            for _ in 0..150 {
                let x = rng.f01() * w;
                let y = rng.f01() * h;
                let a = 0.05 + 0.09 * rng.f01();
                book.line(
                    Offset::new(x, y),
                    Offset::new(x + 26.0, y - 18.0),
                    pf::alpha(SYN_COMMENT, a),
                    1.2,
                );
            }
            // Three thin rings, drifting.
            for (k, r0) in [90.0_f32, 150.0, 214.0].iter().enumerate() {
                let c = Offset::new(w * 0.5, h * 0.5);
                let r = r0 + 6.0 * (tf * 0.7 + k as f32 * 2.1).sin();
                book.ring(
                    c,
                    r,
                    1.4,
                    pf::alpha([BRAND_FAR, SYN_TYPE, TERM_GREEN][k], 0.35),
                );
            }
            // The comet — a fast lissajous with a bright core and a tail.
            let (cx, cy) = (w * 0.5, h * 0.5);
            let p = |tt: f64| {
                let a = tt * 3.1;
                Offset::new(
                    cx + (w * 0.40) * (a as f32).sin(),
                    cy + (h * 0.30) * ((a * 1.5) as f32).cos(),
                )
            };
            let head = p(t);
            for i in (1..=18).rev() {
                let q = p(t - f64::from(i) * 0.012);
                let fade = 1.0 - i as f32 / 20.0;
                book.line(q, head, pf::alpha(Color::WHITE, 0.20 * fade), 3.0 * fade);
            }
            book.circle(head, 7.0, pf::alpha(Color::WHITE, 0.96));
            book.ring(head, 13.0, 1.6, pf::alpha(BRAND_FAR, 0.8));
            pf::vignette(book, w, h, 0.55);
        }),
    )));
    driver.draw_frame_at(Duration::from_secs_f64(t.max(0.0)));
    let mut renderer = NativeRenderer::new();
    let (pixels, _) = renderer
        .render_to_pixels(driver.scene(), w, h, Color::rgb(8, 7, 12))
        .expect("shutter subject rasterised");
    vieww_foundation::Image::from_rgba8(pixels.data().to_vec(), w, h)
}

/// Mean |ΔL| between two images, as a fraction of full scale.
fn luma_delta(a: &vieww_foundation::Image, b: &vieww_foundation::Image) -> f32 {
    let (pa, pb) = (a.pixels(), b.pixels());
    if pa.len() != pb.len() || pa.is_empty() {
        return 0.0;
    }
    let mut sum = 0.0_f64;
    for (x, y) in pa.chunks_exact(4).zip(pb.chunks_exact(4)) {
        let la = 0.2126 * f32::from(x[0]) + 0.7152 * f32::from(x[1]) + 0.0722 * f32::from(x[2]);
        let lb = 0.2126 * f32::from(y[0]) + 0.7152 * f32::from(y[1]) + 0.0722 * f32::from(y[2]);
        sum += f64::from((la - lb).abs());
    }
    (sum / (pa.len() / 4) as f64 / 255.0) as f32
}

/// Z10E — the shutter: sharp beside motion-blurred, the angle sweeping live.
pub fn the_shutter(ctx: &pf::Ctx) -> WidgetNode {
    use vieww_video::motion_blur::{accumulate, Shutter};

    let sec = ctx.sec;
    let t_stars = ctx.t;
    let mut stack = Stack::new();

    frame::ground(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            book.rect(Rect::new(0.0, 0.0, s.width, s.height), Color::rgb(7, 6, 10));
            pf::stars(book, s.width, s.height, 0x51E7, 60, t_stars, 0.06);
            pf::vignette(book, s.width, s.height, 0.6);
        }),
    )));

    // The panels — 768×432 each, the subject rendered per panel.
    let (pw, ph) = (768.0_f32, 432.0);
    let (py, gap) = (292.0, 48.0);
    let left = Rect::new(168.0, py, 168.0 + pw, py + ph);
    let right = Rect::new(168.0 + pw + gap, py, 168.0 + 2.0 * pw + gap, py + ph);

    // The shutter angle: closed until 2.2s, sweeping to 360° by 8s, then
    // parking at the house standard of 180° by 10.6s.
    let open = ease_in_out(clamp01((sec - 2.2) / 4.6));
    let park = ease_in_out(clamp01((sec - 9.2) / 1.4));
    let angle = 360.0 * open * (1.0 - park) + 180.0 * park;
    let samples: u32 = 8;

    // Sharp is one render; blurred is the shutter's average. The subject's
    // own clock runs at the film's rate so the smear is honest.
    let t = f64::from(sec) + 6.0;
    let frame_s = 1.0 / f64::from(FPS);
    let sharp = shutter_subject(t, pw as u32, ph as u32);
    let blurred = if angle > 0.5 {
        accumulate(
            |u| shutter_subject(u, pw as u32, ph as u32),
            t,
            frame_s,
            Shutter { angle: f64::from(angle), phase: -0.5, samples },
        )
    } else {
        sharp.clone()
    };
    let delta = luma_delta(&sharp, &blurred);

    let img_a = clamp01(sec / 0.7);
    stack = stack.push(
        Positioned::new().left(left.left).top(py).width(pw).height(ph).child(
            Opacity::new(img_a).child(Clip::rounded(16.0).child(
                vieww_widget::Image::new(sharp).label("one frame, one sample — sharp"),
            )),
        ),
    );
    stack = stack.push(
        Positioned::new().left(right.left).top(py).width(pw).height(ph).child(
            Opacity::new(img_a).child(Clip::rounded(16.0).child(
                vieww_widget::Image::new(blurred).label("the same frame, through the shutter"),
            )),
        ),
    );

    // The labels under each panel, and the receipt under the blur.
    let lab = |x: f32, text: &str, accent: bool| {
        frame::label(
            x,
            py + ph + 18.0,
            pw,
            30.0,
            text.to_string(),
            pf::geist(21.0).color(if accent {
                pf::alpha(ACCENT, 0.95)
            } else {
                pf::alpha(INK, 0.95)
            }),
            TextAlign::Center,
            img_a,
        )
    };
    stack = stack.push(lab(left.left, "sharp — one sample", false));
    stack = stack.push(lab(right.left, "the shutter — 8 samples, averaged", true));
    if sec > 3.4 {
        let receipt = format!(
            "vieww-video · motion_blur · angle {}° · {} samples/frame · mean ΔL {:.1}/255",
            angle.round() as i32,
            samples,
            delta * 255.0,
        );
        stack = stack.push(frame::label(
            right.left,
            py + ph + 52.0,
            pw,
            26.0,
            receipt,
            pf::geist_mono(17.0).color(pf::alpha(LEDGER, 0.95)),
            TextAlign::Center,
            clamp01((sec - 3.4) / 0.6),
        ));
    }

    // The angle gauge — an arc that opens with the shutter, sweeping under
    // the right panel like a camera's dial.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            let c = Offset::new(right.left + pw * 0.5, py + ph + 96.0);
            let r = 34.0;
            book.ring(c, r, 2.0, pf::alpha(SYN_COMMENT, 0.4));
            let sweep = 0.75 * angle / 360.0; // 270° of dial for 360° of shutter
            if sweep > 0.002 {
                let mut path = vieww_foundation::Path::new();
                let a0 = 0.75 * std::f32::consts::TAU; // the dial starts at 10 o'clock
                let steps = (sweep * 96.0).ceil().max(1.0) as usize;
                for i in 0..=steps {
                    let a = a0 - sweep * std::f32::consts::TAU * (i as f32 / steps as f32);
                    let p = Offset::new(c.dx + r * a.cos(), c.dy + r * a.sin());
                    if i == 0 {
                        path.move_to(p);
                    } else {
                        path.line_to(p);
                    }
                }
                book.stroke(path, pf::alpha(ACCENT, 0.9), 2.6);
            }
            book.circle(c, 3.0, pf::alpha(INK, 0.9));
        }),
    )));

    stack = stack.push(frame::caption(
        "The engine composites its own cinema.",
        1002.0,
        clamp01((sec - 0.2) / 0.5),
    ));
    stack = stack.push(frame::caption(
        "Motion blur from the same frame function — no plugins, no post house.",
        966.0,
        clamp01((sec - 4.6) / 0.6),
    ));
    stack = stack.push(frame::receipts(
        &[
            ("vieww-video · motion blur", ACCENT),
            ("linear-light average", SYN_TYPE),
            ("the angle is live", LEDGER),
        ],
        0.0,
        0.0,
        clamp01((sec - 6.2) / 0.6),
    ));
    frame::boxed(Rect::new(left.left, py, right.right, py + ph + 120.0));
    stack.into()
}

// ── Z10F · the cook ─────────────────────────────────────────────────────────

/// One cooked sample of the operator network: the counters and the trail.
///
/// Replayed from zero — `n` pulls at `n` clock ticks — so the counters are
/// the graph's own and the frame stays a pure function of its index.
fn cook_replay(sec: f32) -> (Vec<(&'static str, u64)>, Vec<f32>, f32) {
    use vieww_graph::{Constant, Graph, Lfo, Math, Noise, Remap, Time, Trail, Value};

    let mut g = Graph::new();
    let t_id = g.add("time", Time);
    let lfo = g.add("lfo", Lfo);
    let mul = g.add("multiply", Math::Multiply);
    let konst = g.add("constant", Constant);
    let noise = g.add("noise", Noise);
    let remap = g.add("remap", Remap);
    let trail = g.add("trail", Trail::default());

    g.set_param(lfo, "frequency", Value::Float(0.9));
    g.set_param(lfo, "amplitude", Value::Float(1.0));
    g.set_param(konst, "value", Value::Float(0.8));
    g.set_param(noise, "seed", Value::Float(7.0));
    g.set_param(noise, "frequency", Value::Float(0.9));
    g.set_param(noise, "amplitude", Value::Float(0.55));
    g.set_param(remap, "from_lo", Value::Float(-1.3));
    g.set_param(remap, "from_hi", Value::Float(1.3));
    g.set_param(remap, "to_lo", Value::Float(0.0));
    g.set_param(remap, "to_hi", Value::Float(1.0));
    g.set_param(trail, "length", Value::Float(64.0));

    let _ = g.connect(t_id, "seconds", lfo, "time");
    let _ = g.connect(lfo, "out", mul, "a");
    let _ = g.connect(konst, "out", mul, "b");
    let _ = g.connect(mul, "out", noise, "x");
    let _ = g.connect(noise, "out", remap, "in");
    let _ = g.connect(remap, "out", trail, "in");

    let n = (sec * FPS).round().max(0.0) as usize + 1;
    let mut channel = Vec::new();
    let mut out = 0.0_f32;
    for k in 0..n {
        g.set_time(k as f32 / FPS);
        if let Ok(Value::Channel(c)) = g.pull(trail, "channel") {
            channel = c;
        }
        if let Ok(Value::Float(v)) = g.pull(remap, "out") {
            out = v;
        }
    }
    let names = ["time", "lfo", "multiply", "constant", "noise", "remap", "trail"];
    let ids = [t_id, lfo, mul, konst, noise, remap, trail];
    let cooks = names
        .iter()
        .zip(ids.iter())
        .map(|(name, id)| (*name, g.cooks(*id)))
        .collect();
    (cooks, channel, out)
}

/// The operator chips: title, sub-label, colour — const, because the
/// chip cards' paint closures capture them and must hold 'static data.
const COOK_LABELS: [(&str, &str, Color); 7] = [
    ("Time", "the clock ×1", SYN_TYPE),
    ("LFO", "0.9 Hz sine", TERM_GREEN),
    ("Math ×", "a × b", SYN_FUNCTION),
    ("Noise", "seeded perlin", SYN_MACRO),
    ("Remap", "−1.3…1.3 → 0…1", SYN_NUMBER),
    ("Trail", "64 samples", SYN_STRING),
    ("Constant", "0.8 — cooked once", ACCENT),
];
/// The chain's positions on the frame — const, same reason.
const COOK_CHAIN_POS: [(f32, f32); 7] = [
    (188.0, 386.0),
    (424.0, 386.0),
    (660.0, 386.0),
    (896.0, 386.0),
    (1132.0, 386.0),
    (1368.0, 386.0),
    (660.0, 534.0),
];
const COOK_NAMES: [&str; 7] = ["time", "lfo", "multiply", "noise", "remap", "trail", "constant"];

/// Z10F — the cook: the operator network, its counters, and its trail.
pub fn the_cook(ctx: &pf::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let (cooks, channel, out) = cook_replay(sec);
    let mut stack = Stack::new();

    frame::ground(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            book.rect(Rect::new(0.0, 0.0, s.width, s.height), Color::rgb(7, 6, 10));
            filmkit::room(
                book,
                s.width,
                s.height,
                90.0,
                Color::rgb(10, 9, 15),
                Color::rgb(8, 7, 12),
                Color::rgb(6, 6, 9),
            );
            pf::vignette(book, s.width, s.height, 0.55);
        }),
    )));

    // The node row — the chain, with the constant hanging below the
    // multiply: a satellite that never recooks.
    let appear = ease_out_expo(clamp01(sec / 1.6));
    let row_y = 386.0;
    let chip_w = 178.0;
    let chip_h = 66.0;
    let chain_pos = COOK_CHAIN_POS;
    let cook_names = COOK_NAMES;

    // The wires first, so the chips sit on top.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            let mut wire = |from: Offset, to: Offset, k: f32| {
                let pts = filmkit::thread_pts(from, to, 26.0);
                filmkit::grow_stroke(book, &pts, k, pf::alpha(SYN_COMMENT, 0.55), 1.6, 1.0);
            };
            let cx = |i: usize| Offset::new(chain_pos[i].0 + chip_w, chain_pos[i].1 + chip_h * 0.5);
            let nx = |i: usize| Offset::new(chain_pos[i].0, chain_pos[i].1 + chip_h * 0.5);
            // The constant's wire rises into the multiply's `b` input.
            wire(
                Offset::new(chain_pos[6].0 + chip_w * 0.5, chain_pos[6].1),
                Offset::new(chain_pos[2].0 + chip_w * 0.5, chain_pos[2].1 + chip_h),
                appear,
            );
            for w in 0..5 {
                let k = clamp01((appear - 0.08 * w as f32) / 0.6);
                wire(cx(w), nx(w + 1), k);
            }
            // A pulse riding the wires — the cook, happening now.
            if appear > 0.98 {
                let pulse = (sec * 1.6).fract();
                for w in 0..5 {
                    let pts = filmkit::thread_pts(cx(w), nx(w + 1), 26.0);
                    let p = filmkit::point_at(&pts, pulse);
                    book.circle(p, 3.2, pf::alpha(ACCENT, 0.85));
                }
            }
        }),
    )));

    for (k, ((title, sub, col), (x, y))) in COOK_LABELS.iter().zip(COOK_CHAIN_POS.iter()).enumerate() {
        let a = clamp01((appear - 0.07 * k as f32) / 0.5);
        if a <= 0.01 {
            continue;
        }
        let cooks_n = cooks
            .iter()
            .find(|(n, _)| *n == cook_names[k])
            .map(|(_, c)| *c)
            .unwrap_or(0);
        let mut card = Stack::new();
        card = card.push(Positioned::fill().child(Painting::sized(
            Size::new(chip_w, chip_h),
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                book.rrect(
                    Rect::new(0.0, 0.0, chip_w, chip_h),
                    10.0,
                    pf::alpha(SURFACE, 0.92 * a),
                );
                book.stroke_rrect(
                    Rect::new(0.0, 0.0, chip_w, chip_h),
                    10.0,
                    pf::alpha(*col, 0.55 * a),
                    1.4,
                );
            }),
        )));
        card = card.push(frame::label(
            8.0,
            8.0,
            chip_w - 16.0,
            26.0,
            title.to_string(),
            pf::geist(22.0).color(pf::alpha(*col, 0.98)),
            TextAlign::Left,
            a,
        ));
        card = card.push(frame::label(
            8.0,
            34.0,
            chip_w - 16.0,
            20.0,
            sub.to_string(),
            pf::geist_mono(15.0).color(pf::alpha(MUTED, 0.95)),
            TextAlign::Left,
            a,
        ));
        // The operator's own cook counter — the receipt, live.
        card = card.push(frame::label(
            8.0,
            52.0,
            chip_w - 16.0,
            18.0,
            format!("cooked ×{}", pf::group_commas(cooks_n)),
            pf::geist_mono(14.0).color(pf::alpha(LEDGER, 0.9)),
            TextAlign::Left,
            clamp01((sec - 2.2) / 0.6),
        ));
        stack = stack.push(
            Positioned::new()
                .left(x - (1.0 - a) * 24.0)
                .top(*y)
                .width(chip_w)
                .height(chip_h)
                .child(card),
        );
    }

    // The output, made visible: a breathing ring, a luminous bar, and the
    // trail — all driven by what the graph cooked this frame.
    let out_a = clamp01((sec - 1.8) / 0.8);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            if out_a <= 0.01 {
                return;
            }
            let c = Offset::new(1572.0, 812.0);
            let r = 62.0 + 26.0 * out;
            book.ring(c, r, 2.4, pf::alpha(pf::mix(BRAND_NEAR, BRAND_FAR, out), 0.85 * out_a));
            book.ring(c, r * 0.66, 1.2, pf::alpha(SYN_COMMENT, 0.4 * out_a));
            book.circle(c, 4.0, pf::alpha(INK, 0.9 * out_a));
            // The luminous bar — the value itself.
            let bar = Rect::new(188.0, 786.0, 1368.0, 812.0);
            book.rrect(bar, 8.0, pf::alpha(SURFACE_2, 0.8 * out_a));
            let fill = Rect::new(190.0, 788.0, 190.0 + 1176.0 * out.max(0.0), 810.0);
            if fill.width() > 2.0 {
                book.rrect(fill, 7.0, pf::alpha(ACCENT, 0.9 * out_a));
            }
            // The trail — the last sixty-four samples, remembered.
            if channel.len() > 1 {
                let (x0, x1, y) = (188.0, 1368.0, 730.0);
                let mut path = vieww_foundation::Path::new();
                let span = (channel.len() - 1).max(1);
                for (i, v) in channel.iter().enumerate() {
                    let p = Offset::new(
                        x0 + (x1 - x0) * i as f32 / span as f32,
                        y - 46.0 * v.clamp(0.0, 1.0),
                    );
                    if i == 0 {
                        path.move_to(p);
                    } else {
                        path.line_to(p);
                    }
                }
                book.stroke(path, pf::alpha(TERM_GREEN, 0.75 * out_a), 1.8);
            }
        }),
    )));
    stack = stack.push(frame::label(
        188.0,
        748.0,
        1180.0,
        22.0,
        "the trail — the last 64 cooked samples, remembered by the operator".to_string(),
        pf::geist_mono(15.0).color(pf::alpha(MUTED, 0.95)),
        TextAlign::Left,
        clamp01((sec - 2.4) / 0.6),
    ));

    stack = stack.push(frame::caption(
        "The engine's dataflow heart.",
        1002.0,
        clamp01((sec - 0.2) / 0.5),
    ));
    stack = stack.push(frame::caption(
        "Dirty flags cook the chain — the constant cooked once and stayed cooked.",
        966.0,
        clamp01((sec - 5.4) / 0.6),
    ));
    let get = |n: &str| cooks.iter().find(|(k, _)| *k == n).map(|(_, c)| *c).unwrap_or(0);
    let const_line = format!("constant cooked ×{}", pf::group_commas(get("constant")));
    let clock_line = format!("clock cooked ×{}", pf::group_commas(get("time")));
    stack = stack.push(frame::receipts(
        &[
            ("vieww-graph · 7 operators", ACCENT),
            (const_line.as_str(), SYN_TYPE),
            (clock_line.as_str(), LEDGER),
        ],
        0.0,
        0.0,
        clamp01((sec - 6.4) / 0.6),
    ));
    frame::boxed(Rect::new(160.0, row_y - 26.0, 1800.0, row_y + 240.0));
    stack.into()
}

// ── Z10G · the swarm ────────────────────────────────────────────────────────

/// The maze — `#` solid, `.` open, `s` the start, `g` the goal.
const MAZE_A: [&str; 8] = [
    "#############",
    "#s..#.......#",
    "#.#.#.###.#.#",
    "#.#...#.#...#",
    "#.#####.#.###",
    "#.....#.#..g#",
    "#.###.#.#.#.#",
    "#############",
];
/// The same maze with the wall at (4, 1) opened — the shortcut the swarm
/// takes the moment the world changes under it.
const MAZE_B: [&str; 8] = [
    "#############",
    "#s..........#",
    "#.#.#.###.#.#",
    "#.#...#.#...#",
    "#.#####.#.###",
    "#.....#.#..g#",
    "#.###.#.#.#.#",
    "#############",
];

const TILE: f32 = 64.0;
const X0: f32 = 232.0;
const Y0: f32 = 300.0;
const AGENTS: usize = 22;
/// The beat where the wall opens.
const WALL_AT: f32 = 6.5;

/// Walk a polyline by arc length (pixels).
fn along(pts: &[Offset], s: f32) -> Offset {
    if pts.is_empty() {
        return Offset::ZERO;
    }
    let total: f32 = pts.windows(2).map(|w| (w[1].dx - w[0].dx).hypot(w[1].dy - w[0].dy)).sum();
    let target = s.clamp(0.0, total);
    let mut acc = 0.0;
    for w in pts.windows(2) {
        let seg = (w[1].dx - w[0].dx).hypot(w[1].dy - w[0].dy);
        if acc + seg >= target {
            let u = if seg < 1.0e-6 { 0.0 } else { (target - acc) / seg };
            return Offset::new(w[0].dx + (w[1].dx - w[0].dx) * u, w[0].dy + (w[1].dy - w[0].dy) * u);
        }
        acc += seg;
    }
    *pts.last().unwrap()
}

/// A polyline's total length.
fn poly_total(pts: &[Offset]) -> f32 {
    pts.windows(2).map(|w| (w[1].dx - w[0].dx).hypot(w[1].dy - w[0].dy)).sum()
}

/// Project a point onto a polyline, returning its arc length.
fn project_arc(pts: &[Offset], p: Offset) -> f32 {
    let mut best = 0.0_f32;
    let mut best_d = f32::INFINITY;
    let mut acc = 0.0;
    for w in pts.windows(2) {
        let seg = (w[1].dx - w[0].dx).hypot(w[1].dy - w[0].dy);
        let u = if seg < 1.0e-6 {
            0.0
        } else {
            (((p.dx - w[0].dx) * (w[1].dx - w[0].dx) + (p.dy - w[0].dy) * (w[1].dy - w[0].dy))
                / (seg * seg))
                .clamp(0.0, 1.0)
        };
        let q = Offset::new(w[0].dx + (w[1].dx - w[0].dx) * u, w[0].dy + (w[1].dy - w[0].dy) * u);
        let d = (p.dx - q.dx).hypot(p.dy - q.dy);
        if d < best_d {
            best_d = d;
            best = acc + seg * u;
        }
        acc += seg;
    }
    best
}

/// The A* path through a maze, as world-space points.
fn maze_path(maze: &[&str]) -> Vec<Offset> {
    use vieww_game::ai::{astar, TileMap};
    let map = TileMap::from_ascii(maze, TILE);
    astar(&map, (1, 1), (11, 5), false)
        .unwrap_or_default()
        .into_iter()
        .map(|(c, r)| Offset::new(X0 + c as f32 * TILE + TILE * 0.5, Y0 + r as f32 * TILE + TILE * 0.5))
        .collect()
}

/// The swarm's per-frame state: agent positions plus the loop's receipts.
///
/// The world is replayed from zero every frame — `Game::frame(1/60)` called
/// `n` times — so the ECS receipts are the loop's own, and the frame stays
/// a pure function of its index.
fn swarm_replay(sec: f32) -> (Vec<(Offset, Vec<Offset>)>, usize, usize, usize) {
    use vieww_game::{Behaviour, Ctx, Game, InputMap, Name, Transform2};
    use std::rc::Rc;

    /// The agent behaviour: ride the corridor, re-route the moment the wall
    /// opens, remember a short trail.
    struct Follow {
        old: Rc<Vec<Offset>>,
        new: Rc<Vec<Offset>>,
        switched: bool,
        phase: f32,
        speed: f32,
    }
    #[derive(Clone)]
    struct Trail6(Vec<Offset>);
    impl Behaviour for Follow {
        fn update(&mut self, ctx: &mut Ctx<'_>, dt: f32) {
            if !self.switched && ctx.time >= WALL_AT {
                // Re-route: keep the world position, find it on the new
                // corridor, and continue from there — no restart.
                let pos = along(&self.old, self.phase);
                self.phase = project_arc(&self.new, pos);
                self.switched = true;
            }
            let path = if self.switched { &self.new } else { &self.old };
            let total = poly_total(path).max(1.0);
            self.phase = (self.phase + self.speed * dt).rem_euclid(total);
            let pos = along(path, self.phase);
            ctx.world.insert(ctx.entity, Transform2::at(pos));
            let mut trail = ctx
                .world
                .get::<Trail6>(ctx.entity)
                .cloned()
                .unwrap_or(Trail6(Vec::new()));
            trail.0.push(pos);
            if trail.0.len() > 7 {
                trail.0.remove(0);
            }
            ctx.world.insert(ctx.entity, trail);
        }
    }

    let old = Rc::new(maze_path(&MAZE_A));
    let new = Rc::new(maze_path(&MAZE_B));
    let mut game = Game::new(InputMap::new());
    let total_a = poly_total(&old).max(1.0);
    for k in 0..AGENTS {
        let e = game.world.spawn();
        let phase = k as f32 / AGENTS as f32 * total_a;
        game.world.insert(e, Transform2::at(along(&old, phase)));
        game.world.insert(e, Name(format!("agent-{k:02}")));
        game.attach(
            e,
            Follow {
                old: Rc::clone(&old),
                new: Rc::clone(&new),
                switched: false,
                phase,
                speed: 190.0 + 26.0 * (k % 5) as f32,
            },
        );
    }

    let n = (sec * FPS).round().max(0.0) as usize;
    let mut steps = 0usize;
    for _ in 0..n {
        let stats = game.frame(1.0 / FPS);
        steps += stats.fixed_steps;
    }

    let agents: Vec<(Offset, Vec<Offset>)> = game
        .world
        .query::<Transform2>()
        .into_iter()
        .map(|e| {
            let pos = game
                .world
                .get::<Transform2>(e)
                .map(|t| t.position)
                .unwrap_or(Offset::ZERO);
            let trail = game.world.get::<Trail6>(e).map(|t| t.0.clone()).unwrap_or_default();
            (pos, trail)
        })
        .collect();
    let searches = if sec >= WALL_AT { 2 } else { 1 };
    (agents, steps, new.len(), searches)
}

/// Z10G — the swarm: an ECS world, A* routing, a maze that changes.
pub fn the_swarm(ctx: &pf::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let t_stars = ctx.t;
    let (agents, steps, path_len, searches) = swarm_replay(sec);
    let agent_count = agents.len();
    let mut stack = Stack::new();

    frame::ground(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            book.rect(Rect::new(0.0, 0.0, s.width, s.height), Color::rgb(7, 6, 10));
            pf::stars(book, s.width, s.height, 0x5A17, 44, t_stars, 0.05);
            pf::vignette(book, s.width, s.height, 0.55);
        }),
    )));

    let appear = ease_out_cubic(clamp01(sec / 1.4));
    let wall_open = sec >= WALL_AT;
    let open_p = crate::film_lib::ease_out_back(clamp01((sec - WALL_AT) / 0.7));
    let path_draw = ease_out_cubic(clamp01((sec - 1.6) / 1.6));
    let path_redraw = ease_out_cubic(clamp01((sec - (WALL_AT + 0.4)) / 1.1));
    let old_path = maze_path(&MAZE_A);
    let new_path = maze_path(&MAZE_B);

    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            if appear <= 0.01 {
                return;
            }
            // The plate.
            let plate = Rect::new(
                X0 - 24.0,
                Y0 - 24.0,
                X0 + 13.0 * TILE + 24.0,
                Y0 + 8.0 * TILE + 24.0,
            );
            book.rrect(plate, 16.0, pf::alpha(SURFACE, 0.55 * appear));
            book.stroke_rrect(plate, 16.0, pf::alpha(SYN_COMMENT, 0.25 * appear), 1.2);

            // Walls and floor. The opened cell draws as floor with a ghost.
            let rows = if wall_open { &MAZE_B } else { &MAZE_A };
            for (r, row) in rows.iter().enumerate() {
                for (c, ch) in row.chars().enumerate() {
                    let cell = Rect::new(
                        X0 + c as f32 * TILE + 3.0,
                        Y0 + r as f32 * TILE + 3.0,
                        X0 + (c + 1) as f32 * TILE - 3.0,
                        Y0 + (r + 1) as f32 * TILE - 3.0,
                    );
                    let centre = Offset::new(cell.left + TILE * 0.42, cell.top + TILE * 0.42);
                    match ch {
                        '#' => {
                            book.rrect(cell, 7.0, pf::alpha(SURFACE_2, 0.95 * appear));
                        }
                        's' => {
                            book.rrect(cell, 7.0, pf::alpha(GROUND, 0.35 * appear));
                            book.ring(centre, 9.0, 2.0, pf::alpha(TERM_GREEN, 0.9 * appear));
                        }
                        'g' => {
                            book.rrect(cell, 7.0, pf::alpha(GROUND, 0.35 * appear));
                            book.ring(centre, 9.0, 2.0, pf::alpha(ACCENT, 0.9 * appear));
                        }
                        _ => {
                            book.rrect(cell, 7.0, pf::alpha(GROUND, 0.35 * appear));
                        }
                    }
                }
            }

            // The path threads — the old corridor fading, the new one
            // self-drawing the moment the wall opens.
            if !wall_open {
                filmkit::grow_stroke(
                    book,
                    &old_path,
                    path_draw,
                    pf::alpha(TERM_GREEN, 0.75),
                    2.0,
                    appear,
                );
            } else {
                filmkit::grow_stroke(
                    book,
                    &old_path,
                    1.0,
                    pf::alpha(SYN_COMMENT, 0.30 * (1.0 - 0.6 * open_p)),
                    2.0,
                    appear,
                );
                filmkit::grow_stroke(
                    book,
                    &new_path,
                    path_redraw,
                    pf::alpha(ACCENT, 0.85),
                    2.0,
                    appear,
                );
            }

            // The agents — glowing dots with their short trails.
            for (pos, trail) in &agents {
                for (i, q) in trail.iter().enumerate() {
                    let fade = i as f32 / trail.len().max(1) as f32;
                    book.circle(*q, 2.2, pf::alpha(BRAND_FAR, 0.35 * fade * appear));
                }
                book.circle(*pos, 5.4, pf::alpha(Color::WHITE, 0.9 * appear));
                book.ring(*pos, 9.0, 1.4, pf::alpha(BRAND_NEAR, 0.65 * appear));
            }

            // The opened wall's ghost — a lit hole where the wall was.
            if wall_open && open_p > 0.02 {
                let cell = Rect::new(
                    X0 + 4.0 * TILE + 3.0,
                    Y0 + TILE + 3.0,
                    X0 + 5.0 * TILE - 3.0,
                    Y0 + 2.0 * TILE - 3.0,
                );
                book.stroke_rrect(cell, 7.0, pf::alpha(ACCENT, 0.55 * open_p), 1.6);
                book.ring(
                    Offset::new(cell.left + TILE * 0.42, cell.top + TILE * 0.42),
                    12.0 * open_p,
                    1.8,
                    pf::alpha(ACCENT, 0.8 * open_p),
                );
            }
        }),
    )));

    // The receipt rail, right of the plate.
    let rail_x = X0 + 13.0 * TILE + 56.0;
    let receipt_in = clamp01((sec - 1.0) / 0.6);
    if receipt_in > 0.01 {
        let steps_line = format!(
            "{} entities · {} fixed steps",
            agent_count,
            pf::group_commas(steps as u64)
        );
        let astar_line = format!("A* · path {} cells · {} searches", path_len, searches);
        let lines: [(String, Color); 5] = [
            ("vieww-game · the ECS".to_string(), SYN_FUNCTION),
            (steps_line, SYN_TYPE),
            (astar_line, LEDGER),
            ("the maze changed at 6.5 s".to_string(), MUTED),
            ("the swarm re-routed — no restart".to_string(), TERM_GREEN),
        ];
        for (k, (text, col)) in lines.into_iter().enumerate() {
            let a = clamp01((receipt_in - 0.08 * k as f32) / 0.4);
            if a <= 0.01 {
                continue;
            }
            let y = Y0 + k as f32 * 46.0;
            let text_w = 120.0 + (text.len() as f32 * 10.0).min(480.0);
            stack = stack.push(frame::label(
                rail_x,
                y,
                W - rail_x - 180.0,
                34.0,
                text,
                pf::geist(20.0).color(pf::alpha(col, 0.95)),
                TextAlign::Left,
                a,
            ));
            let underline_col = col;
            stack = stack.push(Positioned::fill().child(Painting::sized(
                CANVAS,
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    book.rect(
                        Rect::new(rail_x - 12.0, y + 32.0, rail_x - 12.0 + text_w * a, y + 33.4),
                        pf::alpha(underline_col, 0.4 * a),
                    );
                }),
            )));
        }
    }

    stack = stack.push(frame::caption(
        "Systems that behave.",
        1002.0,
        clamp01((sec - 0.2) / 0.5),
    ));
    stack = stack.push(frame::caption(
        "A game-grade ECS on the fixed loop — routed by A*, re-routed the moment the world changed.",
        966.0,
        clamp01((sec - 5.0) / 0.6),
    ));
    stack = stack.push(frame::receipts(
        &[
            ("vieww-game · ECS + A*", ACCENT),
            ("22 entities, one fixed loop", SYN_TYPE),
            ("deterministic replay", LEDGER),
        ],
        0.0,
        0.0,
        clamp01((sec - 7.4) / 0.6),
    ));
    frame::boxed(Rect::new(
        X0 - 24.0,
        Y0 - 24.0,
        X0 + 13.0 * TILE + 24.0,
        Y0 + 8.0 * TILE + 24.0,
    ));
    stack.into()
}
