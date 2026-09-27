//! keynote — the 3:00 viewwstudio release film, built as sixteen scenes and
//! a two-pass master harness, both rendered through vieww's own rasterizer.
//!
//! The film is the production blueprint `KEYNOTE_FILM_PLAN.md` (plan-2.0)
//! made executable: four explicit kishōtenketsu movements, the twist ladder
//! K0–K5, the witness counter 1–7, and the meta-move intact — *the film's
//! motion graphics layer is rendered by vieww itself*, headless, fixed
//! clock, re-renderable to the byte on a given bench.
//!
//! ```console
//! cargo run --release -p film_lab -- census     # pass 1 — the audit + probes
//! cargo run --release -p film_lab -- master     # pass 2 — the MP4 + sheets
//! cargo run --release -p film_lab -- kn:s04     # one scene, 16 frames, sheet
//! ```
//!
//! **The two passes.** The film audits itself (S15's manifest bars), and a
//! self-referential receipt needs a prior measurement: **pass 1 (census)**
//! walks all 10,800 frames, counting the command stream each frame emits
//! (shapes, glyph runs, layers, blurred layers, strokes) plus sampled
//! raster timings at full master resolution, and writes the house-format
//! `manifest.txt`; **pass 2 (master)** renders the film with those numbers
//! in hand — S04's "alive in N seconds", S09's ms readout and S15's bars
//! all read the file, never a typed constant. Bars carry the census pass's
//! counts; the master's own receipt is printed at the end of the run.
//!
//! **The clock discipline** (inherited): scenes are pure functions of `t` in
//! `[0, 1]`; the harness owns wall-clock mapping; RNG is seeded per scene;
//! two runs render byte-identical frames given the same bench (the round-11
//! finding — the host's fonts shape the glyphs, so the manifest names the
//! bench that rendered it).
//!
//! **Numbers on screen are emitted, never typed** — the two incidents
//! (4320→10,800, 2550→2551) are the culture. Every figure in this film is
//! derived: frames from seconds × 60, the ladder from tap times, the
//! manifest from the census, N and ms from measured medians, the damage
//! pixel count from the widget's own geometry.

pub mod master;
pub mod studio;
pub mod s01_wait;
pub mod s02_question;
pub mod s03_title;
pub mod s04_first_paint;
pub mod s05_descent;
pub mod s06_compose;
pub mod s07_receipt;
pub mod s08_state;
pub mod s09_damage;
pub mod s10_worldtour;
pub mod s11_mirror;
pub mod s12_receipts;
pub mod s13_unfold;
pub mod s14_foldback;
pub mod s15_endcard;
pub mod s16_loop;

use vieww_foundation::{
    Color, FontWeight, Gradient, Offset, Rect, Size, Sketchbook, TextAlign, TextStyle,
};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};

use crate::film_lib::{
    alpha, ease_in_out, ease_out_cubic, held_24_in_60, mix, spring_out, tint, xywh, BG, BG_DEEP,
    CYAN, CYAN_SOFT, FAINT, INK, MAGENTA, MUTED, RED, Rng, VIOLET, VIOLET_DEEP, VIOLET_SOFT,
};

// ── The master format ───────────────────────────────────────────────────────

/// Master width, px — 1080p per §6.1.
pub const W: f32 = 1920.0;
/// Master height, px.
pub const H: f32 = 1080.0;
/// The master canvas.
pub const CANVAS: Size = Size::new(W, H);
/// Master cadence — the product's own (rule 9).
pub const FPS: f32 = 60.0;

// ── The reveal palette — the E-20 layer colors, inherited verbatim ─────────

pub const C_DESC: Color = Color::rgb(88, 166, 255); // description  #58a6ff
pub const C_IDENT: Color = Color::rgb(63, 185, 80); // identity     #3fb950
pub const C_GEOM: Color = Color::rgb(255, 166, 87); // geometry     #ffa657
pub const C_SIGNAL: Color = Color::rgb(188, 140, 255); // signal    #bc8cff
pub const C_DAMAGE: Color = RED; // damage                              #f85149

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
    /// "S01" — the plan's node id.
    pub id: &'static str,
    /// "the_wait" — the plan's name.
    pub name: &'static str,
    /// Film seconds this scene spans. The act sums close: 40 + 85 + 30 + 25.
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

/// The sixteen scenes, in cut order. Act sums: I 13+7+6+14 = 40 ·
/// II 11+20+18+11+11+14 = 85 · III 8+8+14 = 30 · IV 4+13+8 = 25 → 180.
pub fn scenes() -> Vec<SceneDef> {
    vec![
        SceneDef { id: "S01", name: "the_wait", seconds: 13.0, build: s01_wait::build },
        SceneDef { id: "S02", name: "the_question", seconds: 7.0, build: s02_question::build },
        SceneDef { id: "S03", name: "the_title", seconds: 6.0, build: s03_title::build },
        SceneDef { id: "S04", name: "first_paint", seconds: 14.0, build: s04_first_paint::build },
        SceneDef { id: "S05", name: "the_descent", seconds: 11.0, build: s05_descent::build },
        SceneDef { id: "S06", name: "compose_live", seconds: 20.0, build: s06_compose::build },
        SceneDef { id: "S07", name: "the_receipt", seconds: 18.0, build: s07_receipt::build },
        SceneDef { id: "S08", name: "state_survives", seconds: 11.0, build: s08_state::build },
        SceneDef { id: "S09", name: "damage_in_pixels", seconds: 11.0, build: s09_damage::build },
        SceneDef { id: "S10", name: "world_tour", seconds: 14.0, build: s10_worldtour::build },
        SceneDef { id: "S11", name: "the_mirror", seconds: 8.0, build: s11_mirror::build },
        SceneDef { id: "S12", name: "the_receipts", seconds: 8.0, build: s12_receipts::build },
        SceneDef { id: "S13", name: "the_unfold", seconds: 14.0, build: s13_unfold::build },
        SceneDef { id: "S14", name: "the_fold_back", seconds: 4.0, build: s14_foldback::build },
        SceneDef { id: "S15", name: "the_end_card", seconds: 13.0, build: s15_endcard::build },
        SceneDef { id: "S16", name: "the_loop", seconds: 8.0, build: s16_loop::build },
    ]
}

/// Absolute film seconds where scene `index` starts.
pub fn scene_start(index: usize) -> f32 {
    scenes().iter().take(index).map(|s| s.seconds).sum()
}

/// The film's total frame count — the sum of the scene tables, emitted.
pub fn total_frames() -> usize {
    scenes().iter().map(|s| s.frames()).sum()
}

// ── The counter ladder — one number per human touch, never reset ───────────

/// The seven touches, in absolute film seconds: say (S04) · rust (S05) ·
/// composed (S06) · carried (S08) · the damage write (S09) · desktop (S10) ·
/// phone (S10). The counter is the witness; the camera changes, it doesn't.
pub const TAPS: [f32; 7] = [37.5, 48.5, 68.5, 97.5, 108.5, 119.0, 122.0];

/// The witness counter at absolute time `abs`.
pub fn ladder_at(abs: f32) -> u32 {
    TAPS.iter().filter(|&&tap| abs >= tap).count() as u32
}

/// A short pulse envelope right after the most recent tap — the bloom the
/// counter wears on every touch.
pub fn tap_pulse(abs: f32) -> f32 {
    let mut best = 0.0f32;
    for &tap in TAPS.iter() {
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
    let total: f32 = scenes().iter().map(|s| s.seconds).sum();
    let shown = abs.min(total);
    format!("{:02}:{:02}", (shown / 60.0) as u32, (shown % 60.0) as u32)
}

// ── The census receipts — measured in pass 1, printed in pass 2 ────────────

/// The film's own audit, plus the two live probes. Every field is measured
/// or derived; none is typed by a human.
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
    /// S04's N — median of three sampled edit→visible latencies, seconds.
    /// Measured at master resolution by the census on this bench.
    pub alive_seconds: f32,
    /// S09's frame ms — median sampled build+raster at 1080p.
    pub frame_ms: f32,
    /// The bench identity — per-bench determinism names its bench (ledger 12).
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

/// A centered caption — the reveal's F-beats keep key info center-frame
/// (the vertical-cutdown discipline).
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

/// Held 24-in-60 progress for a sub-window — the wait's cadence anywhere.
pub fn held_t(sec: f32, seconds: f32) -> f32 {
    held_24_in_60(sec) / seconds
}

/// The typing cadence — bursts and pauses, a keystroke rhythm (E-02).
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

/// Film grain — round specks, per-frame seed (aurora's grammar: round reads
/// as film, square as corruption).
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
