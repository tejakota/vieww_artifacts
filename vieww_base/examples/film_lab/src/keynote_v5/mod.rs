//! keynote_v5 — **the viewwstudio release film**, the fifth build and the
//! return to the native medium: every frame rendered by vieww itself,
//! headless, through `FrameDriver` + `NativeRenderer`, streamed raw into
//! ffmpeg. Nothing in a product shot is faked; nothing is added in post.
//!
//! **The story** — the release keynote's four acts, viewwstudio the hero,
//! vieww the foundation it stands on:
//!
//! | act | movement | scenes | the job |
//! |-----|----------|--------|---------|
//! | I · THE LIE | 起 | S01–S04 | the world as sold: heavy engines, silent truncation, rust locked out — until one point of light says *this is vieww* |
//! | II · THE FOUNDATION | 承 | S05–S09 | the engine beneath: 36 crates · its own rasterizer · the sub-pixel floor · one-click toolchains · the ceilings |
//! | III · THE STUDIO | 転 | S10–S17 | **viewwstudio**: first paint in say · per-keystroke compose · the descent to rust · 2,400 boids in a layout sheet · the fourier canvas · the cross-build to apk + ipa · devtools on the studio itself |
//! | IV · THE LEDGER | 結 | S18–S22 | receipts over claims: the green ledger · the 90-plate wall · built-on-vieww · the end card audits itself · the loop |
//!
//! **The house rules, inherited verbatim** (the culture, not the style):
//! 1. No number on screen is typed by a human — every figure is emitted by
//!    the pipeline it describes, or quoted from this repository's own
//!    receipts with the source named in the scene's docs.
//! 2. No effect is added in post — visible ⇒ the product rendered it.
//! 3. The cut works muted — every beat is captioned by construction.
//! 4. Richness escalates deliberately, and every escalation is a demo.
//!
//! ```console
//! cargo run --release -p film_lab -- census5     # pass 1 — the audit
//! cargo run --release -p film_lab -- master5     # pass 2 — the MP4 + sheets
//! cargo run --release -p film_lab -- k5:swarm    # one scene, 16 frames
//! ```

pub mod master;
pub mod studio;
pub mod s01_terminal;
pub mod s02_truncation;
pub mod s03_lockout;
pub mod s04_reveal;
pub mod s05_crates;
pub mod s06_rasterizer;
pub mod s07_subpixel;
pub mod s08_toolchains;
pub mod s09_ceilings;
pub mod s10_opens;
pub mod s11_first_paint;
pub mod s12_compose;
pub mod s13_descent;
pub mod s14_swarm;
pub mod s15_fourier;
pub mod s16_crossbuild;
pub mod s17_mirror;
pub mod s18_ledger;
pub mod s19_plates;
pub mod s20_built_on;
pub mod s21_endcard;
pub mod s22_loop;

use vieww_foundation::{
    Color, FontWeight, Gradient, Offset, Rect, Size, Sketchbook, TextAlign, TextStyle,
};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};

use crate::film_lib::{
    alpha, ease_out_cubic, mix, spring_out, tint, xywh, AMBER, BG, BG_DEEP, CYAN, CYAN_SOFT,
    FAINT, INK, MAGENTA, MINT, MUTED, RED, Rng, VIOLET, VIOLET_DEEP, VIOLET_SOFT,
};

// ── The master format ───────────────────────────────────────────────────────

/// Master width, px — 1080p.
pub const W: f32 = 1920.0;
/// Master height, px.
pub const H: f32 = 1080.0;
/// The master canvas.
pub const CANVAS: Size = Size::new(W, H);
/// Master cadence — the product's own.
pub const FPS: f32 = 60.0;

// ── The palette — inherited, plus the act registers ─────────────────────────

/// The damage/danger accent (act I's red).
pub const C_DAMAGE: Color = RED;
/// Act I's terminal green — the old world's promise color.
pub const TERM_GREEN: Color = Color::rgb(63, 185, 80);
/// Act II's engine blue — technical, cool.
pub const ENGINE: Color = CYAN;
/// The receipts' green (act IV's ledger).
pub const LEDGER: Color = MINT;

// ── The scene registry ──────────────────────────────────────────────────────

/// What a scene may read: its own clock, the film clock, the ladder, and
/// the census pass's measured numbers (zeros in pass 1 — a scene must
/// render sanely without them; only the numbers' *display* differs).
pub struct Ctx<'a> {
    /// `t` in `[0, 1)` within the scene.
    pub t: f32,
    /// Seconds into the scene (`t * seconds`).
    pub sec: f32,
    /// Absolute film seconds — the session clock's source.
    pub abs: f32,
    /// The witness counter right now — one per human touch, never reset.
    pub ladder: u32,
    /// The census pass's receipts (zeros until the census has run).
    pub probe: &'a Probe,
}

/// One scene in the film.
pub struct SceneDef {
    /// "S01" — the scene id.
    pub id: &'static str,
    /// "the_terminal" — the scene name.
    pub name: &'static str,
    /// Film seconds this scene spans. Act sums: I 10+10+8+11 = 39 ·
    /// II 11+12+9+10+7 = 49 · III 10+11+11+8+12+10+12+9 = 83 ·
    /// IV 11+11+10+11+5 = 48 → 219 s = 3:39.
    pub seconds: f32,
    /// The scene — a pure function of its ctx.
    pub build: fn(&Ctx) -> WidgetNode,
}

impl SceneDef {
    /// Frames this scene emits at master cadence — derived, never typed.
    pub fn frames(&self) -> usize {
        (self.seconds * FPS).round() as usize
    }
}

/// The twenty-two scenes, in cut order.
pub fn scenes() -> Vec<SceneDef> {
    vec![
        // ── Act I · THE LIE ───────────────────────────────────────────────
        SceneDef { id: "S01", name: "the_terminal", seconds: 10.0, build: s01_terminal::build },
        SceneDef { id: "S02", name: "silent_truncation", seconds: 10.0, build: s02_truncation::build },
        SceneDef { id: "S03", name: "rust_lockout", seconds: 8.0, build: s03_lockout::build },
        SceneDef { id: "S04", name: "the_reveal", seconds: 11.0, build: s04_reveal::build },
        // ── Act II · THE FOUNDATION ───────────────────────────────────────
        SceneDef { id: "S05", name: "thirty_six_crates", seconds: 11.0, build: s05_crates::build },
        SceneDef { id: "S06", name: "own_rasterizer", seconds: 12.0, build: s06_rasterizer::build },
        SceneDef { id: "S07", name: "subpixel_floor", seconds: 9.0, build: s07_subpixel::build },
        SceneDef { id: "S08", name: "one_click", seconds: 10.0, build: s08_toolchains::build },
        SceneDef { id: "S09", name: "the_ceilings", seconds: 7.0, build: s09_ceilings::build },
        // ── Act III · THE STUDIO ──────────────────────────────────────────
        SceneDef { id: "S10", name: "studio_opens", seconds: 10.0, build: s10_opens::build },
        SceneDef { id: "S11", name: "first_paint", seconds: 11.0, build: s11_first_paint::build },
        SceneDef { id: "S12", name: "live_compose", seconds: 11.0, build: s12_compose::build },
        SceneDef { id: "S13", name: "the_descent", seconds: 8.0, build: s13_descent::build },
        SceneDef { id: "S14", name: "the_swarm", seconds: 12.0, build: s14_swarm::build },
        SceneDef { id: "S15", name: "fourier_canvas", seconds: 10.0, build: s15_fourier::build },
        SceneDef { id: "S16", name: "cross_build", seconds: 12.0, build: s16_crossbuild::build },
        SceneDef { id: "S17", name: "devtools_mirror", seconds: 9.0, build: s17_mirror::build },
        // ── Act IV · THE LEDGER ───────────────────────────────────────────
        SceneDef { id: "S18", name: "green_ledger", seconds: 11.0, build: s18_ledger::build },
        SceneDef { id: "S19", name: "ninety_plates", seconds: 11.0, build: s19_plates::build },
        SceneDef { id: "S20", name: "built_on_vieww", seconds: 10.0, build: s20_built_on::build },
        SceneDef { id: "S21", name: "the_end_card", seconds: 11.0, build: s21_endcard::build },
        SceneDef { id: "S22", name: "the_loop", seconds: 5.0, build: s22_loop::build },
    ]
}

/// Absolute film seconds where scene `index` starts.
pub fn scene_start(index: usize) -> f32 {
    scenes().iter().take(index).map(|s| s.seconds).sum()
}

/// The film's total frame count — the sum of the scene table, emitted.
pub fn total_frames() -> usize {
    scenes().iter().map(|s| s.frames()).sum()
}

/// The film's total seconds — derived, never typed.
pub fn total_seconds() -> f32 {
    scenes().iter().map(|s| s.seconds).sum()
}

// ── The witness ladder — one number per human touch, never reset ───────────

/// The eight touches, in absolute film seconds: the first paint breathes
/// (S11) · Add one is tapped (S11) · the heading edit (S12) · the spacing
/// edit (S12) · the descent (S13) · the swarm drops (S14) · build fired
/// (S16) · the package lands (S16). The counter is the witness; the camera
/// changes, it doesn't.
pub fn taps() -> Vec<f32> {
    let at = |i: usize, frac: f32| scene_start(i) + scenes()[i].seconds * frac;
    vec![
        at(10, 0.58), // 1 · first paint
        at(10, 0.86), // 2 · Add one
        at(11, 0.30), // 3 · heading edit
        at(11, 0.62), // 4 · spacing edit
        at(12, 0.45), // 5 · the descent
        at(13, 0.22), // 6 · the swarm drops
        at(15, 0.30), // 7 · build fired
        at(15, 0.80), // 8 · the package lands
    ]
}

/// The witness counter at absolute time `abs`.
pub fn ladder_at(abs: f32) -> u32 {
    taps().iter().filter(|&&tap| abs >= tap).count() as u32
}

/// A short pulse envelope right after the most recent tap — the bloom the
/// counter wears on every touch.
pub fn tap_pulse(abs: f32) -> f32 {
    let mut best = 0.0f32;
    for &tap in taps().iter() {
        if abs >= tap {
            let dt = (abs - tap).min(1.2);
            let e = (-(dt - 0.05).max(0.0) * 5.0).exp();
            best = best.max(e * (1.0 - dt / 1.2).max(0.0));
        }
    }
    best
}

/// The session chip's clock — mm:ss of the session, which is the film.
pub fn session_clock(abs: f32) -> String {
    let shown = abs.min(total_seconds());
    format!("{:02}:{:02}", (shown / 60.0) as u32, (shown % 60.0) as u32)
}

// ── The census receipts — measured in pass 1, printed in pass 2 ────────────

/// The film's own audit, plus the live probes. Every field is measured or
/// derived; none is typed by a human.
#[derive(Default, Clone)]
pub struct Probe {
    /// Total frames — derived (Σ seconds × 60).
    pub frames: u64,
    /// Fill + stroke commands across the whole film.
    pub shapes: u64,
    /// DrawGlyphs commands across the whole film.
    pub glyph_runs: u64,
    /// Glyph instances across the whole film.
    pub glyphs: u64,
    /// PushLayer commands across the whole film.
    pub layers: u64,
    /// PushLayer commands carrying a real filter — the blur economy.
    pub filtered: u64,
    /// StrokePath commands across the whole film.
    pub strokes: u64,
    /// DrawShadow commands across the whole film.
    pub shadows: u64,
    /// S11's N — median of the sampled edit→visible latencies, seconds.
    /// Measured at master resolution by the census on this bench.
    pub alive_seconds: f32,
    /// S14's frame ms — median sampled build+raster at 1080p.
    pub frame_ms: f32,
    /// The bench identity — per-bench determinism names its bench.
    pub bench: String,
}

impl Probe {
    /// Parse the house-format `key=value` receipt the census wrote.
    pub fn load(path: &std::path::Path) -> Option<Probe> {
        let body = std::fs::read_to_string(path).ok()?;
        let mut p = Probe::default();
        for line in body.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "frames" => p.frames = v.parse().ok()?,
                "shapes" => p.shapes = v.parse().ok()?,
                "glyph_runs" => p.glyph_runs = v.parse().ok()?,
                "glyphs" => p.glyphs = v.parse().ok()?,
                "layers" => p.layers = v.parse().ok()?,
                "filtered_layers" => p.filtered = v.parse().ok()?,
                "strokes" => p.strokes = v.parse().ok()?,
                "shadows" => p.shadows = v.parse().ok()?,
                "alive_seconds" => p.alive_seconds = v.parse().ok()?,
                "frame_ms" => p.frame_ms = v.parse().ok()?,
                "bench" => p.bench = v.to_string(),
                _ => {}
            }
        }
        Some(p)
    }

    /// The bench identity line, emitted from the environment itself.
    pub fn bench_identity(fonts: u32) -> String {
        format!(
            "{} · {} · rust {} · {} system fonts",
            std::env::consts::OS,
            std::env::consts::ARCH,
            "1.98.1", // the pinned toolchain this checkout builds with
            fonts,
        )
    }
}

// ── Shared composition helpers — the film's house look ─────────────────────

/// A soft radial glow blob, drawn as its own blurred layer.
pub fn glow(book: &mut Sketchbook, x: f32, y: f32, r: f32, color: Color, a: f32) {
    book.layer(1.0, r * 0.16, None, |g| {
        g.circle(
            Offset::new(x, y),
            r,
            Gradient::radial_fill().with_dither().with_stops(&[
                (0.0, alpha(color, a)),
                (0.55, alpha(color, a * 0.42)),
                (1.0, alpha(color, 0.0)),
            ]),
        );
    });
}

/// The standard star field — sparse, calm, deterministic.
pub fn stars(book: &mut Sketchbook, w: f32, h: f32, seed: u64, n: usize, t: f32, base: f32) {
    let mut rng = Rng::new(seed);
    for _ in 0..n {
        let x = rng.f01() * w;
        let y = rng.f01() * h;
        let r = 0.4 + rng.f01() * 0.9;
        let tw = 0.5 + 0.5 * (t * 3.2 + rng.f01() * 9.0).sin();
        book.circle(Offset::new(x, y), r, alpha(Color::WHITE, base * (0.4 + 0.6 * tw)));
    }
}

/// The corner vignette.
pub fn vignette(book: &mut Sketchbook, w: f32, h: f32, strength: f32) {
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::radial(Offset::new(0.5, 0.5), 0.80)
            .with_dither()
            .with_stops(&[
                (0.55, alpha(Color::BLACK, 0.0)),
                (1.0, alpha(Color::BLACK, strength)),
            ]),
    );
}

/// The standard ground gradient — deep, quiet, faintly violet at the floor.
pub fn ground(book: &mut Sketchbook, w: f32, h: f32) {
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::vertical().with_dither().with_stops(&[
            (0.0, Color::rgb(9, 9, 13)),
            (0.6, BG_DEEP),
            (1.0, Color::rgb(12, 11, 18)),
        ]),
    );
}

/// A generic "deep space" backdrop painting: ground + stars + vignette.
pub fn backdrop(t: f32, seed: u64, star_n: usize) -> WidgetNode {
    Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;
            ground(book, w, h);
            stars(book, w, h, seed, star_n, t, 0.10);
            vignette(book, w, h, 0.45);
        }),
    )
    .into()
}

/// A mono text node — the film's instrument voice.
pub fn mono(text: impl Into<String>, size: f32, color: Color) -> WidgetNode {
    Text::new(text)
        .style(TextStyle::new(size).monospace().color(color))
        .into()
}

/// A mono text node with letter tracking.
pub fn mono_tracked(text: impl Into<String>, size: f32, color: Color, tracking: f32) -> WidgetNode {
    Text::new(text)
        .style(
            TextStyle::new(size)
                .monospace()
                .letter_spacing(tracking)
                .color(color),
        )
        .into()
}

/// The film's caption — bottom-left, mono, tracked, with a violet tick.
/// `appear` in `[0, 1]` fades it in with a slight rise; every beat is
/// captioned by construction (the cut works muted).
pub fn caption(text: &str, y: f32, appear: f32) -> WidgetNode {
    let a = ease_out_cubic(appear.clamp(0.0, 1.0));
    if a <= 0.01 {
        return Stack::new().into();
    }
    let rise = (1.0 - a) * 14.0;
    Stack::new()
        .push(
            Positioned::new()
                .left(180.0)
                .top(y + rise)
                .width(10.0)
                .height(28.0)
                .child(Opacity::new(a).child(Painting::sized(
                    Size::new(10.0, 28.0),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        book.rrect(xywh(0.0, 0.0, 4.0, 28.0), 2.0, alpha(VIOLET_SOFT, 0.9));
                    }),
                ))),
        )
        .push(
            Positioned::new()
                .left(202.0)
                .top(y + rise)
                .width(1560.0)
                .height(32.0)
                .child(Opacity::new(a).child(
                    Text::new(text)
                        .style(
                            TextStyle::new(22.0)
                                .monospace()
                                .letter_spacing(2.6)
                                .color(alpha(INK, 0.92)),
                        )
                        .align(TextAlign::Left),
                )),
        )
        .into()
}

/// A centered caption — key info stays center-frame.
pub fn caption_center(text: &str, y: f32, appear: f32) -> WidgetNode {
    let a = ease_out_cubic(appear.clamp(0.0, 1.0));
    if a <= 0.01 {
        return Stack::new().into();
    }
    let rise = (1.0 - a) * 12.0;
    Stack::new()
        .push(
            Positioned::new()
                .left(0.0)
                .top(y + rise)
                .width(W)
                .height(32.0)
                .child(Opacity::new(a).child(
                    Text::new(text)
                        .style(
                            TextStyle::new(22.0)
                                .monospace()
                                .letter_spacing(2.6)
                                .color(alpha(INK, 0.92)),
                        )
                        .align(TextAlign::Center),
                )),
        )
        .into()
}

/// A small chip — rounded label with a soft border, the receipts' container.
pub fn chip(text: impl Into<String>, size: f32, fg: Color) -> WidgetNode {
    Container::new()
        .color(alpha(Color::rgb(16, 16, 21), 0.85))
        .radius(7.0)
        .border(vieww_foundation::Border::new(alpha(fg, 0.22), 1.0))
        .padding(vieww_foundation::EdgeInsets::symmetric(7.0, 11.0))
        .child(Text::new(text).style(
            TextStyle::new(size).monospace().letter_spacing(1.2).color(fg),
        ))
        .into()
}

/// Comma-grouped integer — the receipts' spelling (10,800, not 10800).
pub fn group_commas(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    let bytes = s.as_bytes();
    for (i, c) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*c as char);
    }
    out
}

/// DejaVu Sans Mono's advance in ems — layout arithmetic for the mono grid.
pub const MONO_ADV: f32 = 0.60205;

/// A monospace run's pixel width.
pub fn mono_w(size: f32, chars: usize) -> f32 {
    size * MONO_ADV * chars as f32
}

/// Held 24-in-60 progress for a sub-window — the old world's cadence.
pub fn held_t(sec: f32, seconds: f32) -> f32 {
    crate::film_lib::held_24_in_60(sec) / seconds
}

/// The typing cadence — bursts and pauses, a keystroke rhythm.
pub fn ease_out_type(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let burst = |u: f32| 1.0 - (1.0 - u).powi(2);
    if t < 0.35 {
        burst(t / 0.35) * 0.34
    } else if t < 0.5 {
        0.34
    } else if t < 0.78 {
        0.34 + burst((t - 0.5) / 0.28) * 0.4
    } else if t < 0.86 {
        0.74
    } else {
        0.74 + burst((t - 0.86) / 0.14) * 0.26
    }
}

/// A circle as a closed polyline path.
pub fn circle_path(cx: f32, cy: f32, r: f32, segs: usize) -> vieww_foundation::Path {
    let mut p = vieww_foundation::Path::new();
    for i in 0..=segs {
        let a = i as f32 / segs as f32 * std::f32::consts::TAU;
        let pt = Offset::new(cx + a.cos() * r, cy + a.sin() * r);
        if i == 0 {
            p.move_to(pt);
        } else {
            p.line_to(pt);
        }
    }
    p.close();
    p
}

/// Film grain — round specks, per-frame seed (round reads as film).
pub fn grain(book: &mut Sketchbook, w: f32, h: f32, frame_i: u64, strength: f32) {
    let mut rng = Rng::new(0x6A1D ^ frame_i.wrapping_mul(0x9E37));
    let n = (240.0 * strength) as usize;
    for _ in 0..n {
        let x = rng.f01() * w;
        let y = rng.f01() * h;
        let bright = rng.f01() > 0.5;
        let a = 0.012 + rng.f01() * 0.03;
        book.circle(
            Offset::new(x, y),
            0.5 + rng.f01() * 0.7,
            if bright { alpha(Color::WHITE, a) } else { alpha(Color::BLACK, a * 1.6) },
        );
    }
}

/// Slow dust motes — the wait's air.
pub fn dust(book: &mut Sketchbook, w: f32, h: f32, t: f32, seed: u64, strength: f32) {
    let mut rng = Rng::new(seed);
    for _ in 0..64 {
        let bx = rng.f01() * w;
        let by = rng.f01() * h;
        let r = 0.8 + rng.f01() * 1.7;
        let drift = (t * 1.3 + rng.f01() * 7.0).sin() * 10.0;
        let fall = t * 12.0 * rng.f01();
        book.circle(
            Offset::new(bx + drift, (by + fall) % h),
            r,
            alpha(MUTED, 0.22 * strength),
        );
    }
}

// ── The v5 additions — the release film's own props ────────────────────────

/// The act chip — top-left, the movement's name (I–IV · 起承転結).
pub fn act_chip(act: &str, name: &str, appear: f32) -> WidgetNode {
    let a = ease_out_cubic(appear.clamp(0.0, 1.0));
    if a <= 0.01 {
        return Stack::new().into();
    }
    Stack::new()
        .push(
            Positioned::new()
                .left(178.0)
                .top(92.0)
                .width(700.0)
                .height(26.0)
                .child(Opacity::new(a).child(
                    Text::new(format!("ACT {act} — {name}"))
                        .style(
                            TextStyle::new(15.0)
                                .monospace()
                                .letter_spacing(4.5)
                                .color(alpha(MUTED, 0.85)),
                        )
                        .align(TextAlign::Left),
                )),
        )
        .into()
}

/// The film's progress rail — a hairline at the very bottom, one tick per
/// scene, the playhead sliding: the session clock made visible.
pub fn progress_rail(abs: f32) -> WidgetNode {
    let total = total_seconds();
    let frac = (abs / total).clamp(0.0, 1.0);
    let n_scenes = scenes().len() as f32;
    let starts: Vec<f32> = (0..scenes().len()).map(scene_start).collect();
    Painting::sized(Size::new(W, 26.0), PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
        let x0 = 180.0;
        let x1 = W - 180.0;
        let y = 18.0;
        book.line(Offset::new(x0, y), Offset::new(x1, y), alpha(Color::WHITE, 0.07), 1.0);
        // One tick per scene boundary — derived, never typed.
        for (i, st) in starts.iter().enumerate() {
            let x = x0 + (x1 - x0) * (st / total);
            let major = i % 4 == 0; // act boundaries — the four movements
            book.line(
                Offset::new(x, y - if major { 6.0 } else { 4.0 }),
                Offset::new(x, y + if major { 6.0 } else { 4.0 }),
                alpha(Color::WHITE, if major { 0.22 } else { 0.13 }),
                1.0,
            );
        }
        let _ = n_scenes;
        // The playhead.
        let px = x0 + (x1 - x0) * frac;
        book.circle(Offset::new(px, y), 3.0, alpha(VIOLET_SOFT, 0.9));
        book.line(Offset::new(px, y), Offset::new(px, y + 10.0), alpha(VIOLET_SOFT, 0.55), 1.2);
    }))
    .into()
}

/// A count-up value — integer part eased, so digits roll to their rest.
pub fn count_up(target: u64, progress: f32) -> u64 {
    let e = ease_out_cubic(progress.clamp(0.0, 1.0));
    ((target as f32 * e).round()) as u64
}
