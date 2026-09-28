//! product_film — **THE DISTANCE**, the viewwstudio product film.
//!
//! A five-minute film about the distance between a thought and a screen —
//! and the engine that collapses it. Every frame rendered by **vieww
//! itself**: headless `FrameDriver` + `NativeRenderer`, 1920×1080 logical
//! at 60 fps, raw RGBA streamed into ffmpeg. Nothing added in post.
//!
//! # The medium's own claim
//!
//! The studio act (movement III) is **not a mock**: the film mounts the
//! actual `viewwstudio` app — the real `Shell`, the real `Studio` state,
//! the real element runtime, the real `rustc` compile pipeline — on the
//! film's own `FrameDriver`, and drives it with a frame-indexed script
//! through the real input pipeline (`product_film/script.rs`). When the
//! film types, the studio's editor types. When the film taps, the studio's
//! demo navigates. When the film presses Render, `rustc` runs.
//!
//! # The story — one number, falling
//!
//! Two points: a caret (a thought) and a screen (a person waiting to see
//! it). Between them, a measured distance in milliseconds. The film is the
//! story of that number collapsing:
//!
//! ```text
//! 45,000 ms   the old world — edit, wait, rebuild, wait, ship, drift
//!  1,800 ms   the engine — three trees, one renderer, all the way down
//!    120 ms   the frame budget — damage, not repaint; signal, not tree
//!     ~30 ms  the studio — edit a line, see the picture change
//!     16 ms   one frame
//!      0 ms   the release — the caret touches the screen
//! ```
//!
//! The emotional arc the film carries: **curiosity** (how far is a thought
//! from a screen?) → **need** (the four pains of the old world: the wait,
//! the tolls, the jank, the drift) → **relief** (the engine that owns its
//! own pixels, and the studio built on it).
//!
//! # The palette is the brand's
//!
//! Every colour the film's chrome draws is taken verbatim from
//! `apps/viewwsite/src/lib.rs` — the product page's palette (GROUND
//! `#0F0D0B` · INK `#F8F4F2` · ACCENT `#B491FF` · ACCENT_DEEP `#7E5CE8`,
//! the syntax ramp) — and the mark on the end card is the studio's own
//! code-drawn logo (`viewwstudio::ui::brand`), used, not copied. The
//! studio renders in its own shipped theme exactly as the product does.
//!
//! # SCALE_FACTOR
//!
//! The master rasterises the *same* 1920×1080 logical frame at any device
//! resolution through `Scene::scaled(SCALE_FACTOR)` — glyph outlines
//! scan-converted at device resolution, never magnified. `SCALE_FACTOR=1`
//! is the 1080p base; `SCALE_FACTOR=2` renders 4K; `SCALE_FACTOR=4`
//! renders 8K. The FrameDriver, the layout, the script's tap coordinates
//! and the studio session are all in logical points and never change with
//! scale.

use vieww_foundation::{Color, Offset, Rect, Size};
pub use vieww_widget::prelude::*;
pub use vieww_widget::{Opacity, Painting, PaintWith};

pub use crate::film_lib::Rng;

// ── The canvas ──────────────────────────────────────────────────────────────

pub const W: f32 = 1920.0;
pub const H: f32 = 1080.0;
pub const CANVAS: Size = Size::new(W, H);
pub const FPS: f32 = 60.0;

// ── The palette — the brand's, quoted ───────────────────────────────────────

/// The page's ground `oklch(0.16 0.006 60)`.
pub const GROUND: Color = Color::rgb(0x0F, 0x0D, 0x0B);
pub const BG_DEEP: Color = Color::rgb(0x0A, 0x09, 0x08);
pub const SURFACE: Color = Color::rgb(0x1B, 0x17, 0x15);
pub const SURFACE_2: Color = Color::rgb(0x2B, 0x25, 0x21);
pub const LINE: Color = Color::rgb(0x2A, 0x28, 0x26);
pub const INK: Color = Color::rgb(0xF8, 0xF4, 0xF2);
pub const MUTED: Color = Color::rgb(0xA6, 0x9C, 0x95);
pub const FAINT: Color = Color::rgb(0x78, 0x71, 0x6C);

/// The accent on a dark ground — the studio's own purple, far stop.
pub const ACCENT: Color = Color::rgb(0xB4, 0x91, 0xFF);
/// The accent a button is filled with — the deep stop.
pub const ACCENT_DEEP: Color = Color::rgb(0x7E, 0x5C, 0xE8);
/// `bg-primary/10` — the wash an icon square sits on.
pub const WASH: Color = Color::rgb(0x22, 0x1C, 0x33);

// The syntax ramp — the film's secondary register, quoted from the page.
pub const SYN_KEYWORD: Color = Color::rgb(0xEF, 0xA3, 0xFF);
pub const SYN_TYPE: Color = Color::rgb(0x48, 0xD7, 0xFE);
pub const SYN_STRING: Color = Color::rgb(0x59, 0xD3, 0x8C);
pub const SYN_NUMBER: Color = Color::rgb(0xFF, 0x8F, 0x9A);
pub const SYN_COMMENT: Color = Color::rgb(0x85, 0x7F, 0x7A);
pub const SYN_MACRO: Color = Color::rgb(0xFE, 0xB2, 0x63);
pub const SYN_FUNCTION: Color = Color::rgb(0xFF, 0xBB, 0x6D);
pub const SYN_PUNCT: Color = Color::rgb(0xA3, 0x9D, 0x98);

// The film's own role colours, all derived from the two ramps above.
/// The old world's terminal green — the register of the pain act.
pub const TERM_GREEN: Color = Color::rgb(63, 185, 80);
/// The engine's cyan — the machine act.
pub const ENGINE: Color = SYN_TYPE;
/// The receipts' mint — the ledger act.
pub const LEDGER: Color = SYN_STRING;
/// The break's red — the moment the old world fails.
pub const BREAK_RED: Color = SYN_NUMBER;
/// The mark's ground — `viewwstudio::ui::brand::GROUND`, quoted.
pub const MARK_GROUND: Color = Color::rgb(0x14, 0x16, 0x1A);
/// The mark's editor panel — `viewwstudio::ui::brand::PANEL`, quoted.
pub const MARK_PANEL: Color = Color::rgb(0x46, 0x4E, 0x5E);

// ── The scene registry ──────────────────────────────────────────────────────

/// How a scene renders.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A pure widget tree — a function of the frame's ctx.
    Pure,
    /// The **actual studio**: the harness mounts `Shell { studio }` on the
    /// film's driver and applies the session script up to this frame; the
    /// scene's `build` returns only the overlay chrome that rides on top.
    Studio,
}

/// What a scene may read: its own clock, the film clock, the tap ladder,
/// and the census pass's measured numbers (zeros until the census ran).
pub struct Ctx<'a> {
    /// `t` in `[0, 1)` within the scene.
    pub t: f32,
    /// Seconds into the scene.
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
    /// "P01" — the scene id.
    pub id: &'static str,
    /// "the_two_points" — the scene name.
    pub name: &'static str,
    /// Film seconds this scene spans. Movement sums: P 12 · I 58 ·
    /// II 58 · III 104 · IV+V 68 → 300 s = 5:00.
    pub seconds: f32,
    /// How the harness renders this scene.
    pub kind: Kind,
    /// The scene — a pure function of its ctx.
    pub build: fn(&Ctx) -> WidgetNode,
}

impl SceneDef {
    /// Frames this scene emits at master cadence — derived, never typed.
    pub fn frames(&self) -> usize {
        (self.seconds * FPS).round() as usize
    }
}

/// The twenty-three scenes, in cut order.
pub fn scenes() -> Vec<SceneDef> {
    vec![
        // ── Prologue · THE QUESTION ──────────────────────────────────────
        SceneDef { id: "P01", name: "the_two_points", seconds: 12.0, kind: Kind::Pure, build: s01_the_two_points::build },
        // ── Movement I · THE FAR — the old world's four pains ────────────
        SceneDef { id: "A01", name: "the_wait", seconds: 14.0, kind: Kind::Pure, build: a01_the_wait::build },
        SceneDef { id: "A02", name: "the_tolls", seconds: 14.0, kind: Kind::Pure, build: a02_the_tolls::build },
        SceneDef { id: "A03", name: "the_jank", seconds: 14.0, kind: Kind::Pure, build: a03_the_jank::build },
        SceneDef { id: "A04", name: "the_drift", seconds: 16.0, kind: Kind::Pure, build: a04_the_drift::build },
        // ── Movement II · THE ENGINE — what vieww is ─────────────────────
        SceneDef { id: "B01", name: "three_trees", seconds: 14.0, kind: Kind::Pure, build: b01_three_trees::build },
        SceneDef { id: "B02", name: "the_crates", seconds: 14.0, kind: Kind::Pure, build: b02_the_crates::build },
        SceneDef { id: "B03", name: "the_rasterizer", seconds: 14.0, kind: Kind::Pure, build: b03_the_rasterizer::build },
        SceneDef { id: "B04", name: "the_budget", seconds: 16.0, kind: Kind::Pure, build: b04_the_budget::build },
        // ── Movement III · THE STUDIO — the real app, driven live ────────
        SceneDef { id: "C01", name: "studio_opens", seconds: 10.0, kind: Kind::Studio, build: c01_studio_opens::build },
        SceneDef { id: "C02", name: "first_paint", seconds: 12.0, kind: Kind::Studio, build: c02_first_paint::build },
        SceneDef { id: "C03", name: "live_compose", seconds: 12.0, kind: Kind::Studio, build: c03_live_compose::build },
        SceneDef { id: "C04", name: "the_tap", seconds: 8.0, kind: Kind::Studio, build: c04_the_tap::build },
        SceneDef { id: "C05", name: "say_to_rust", seconds: 16.0, kind: Kind::Studio, build: c05_say_to_rust::build },
        SceneDef { id: "C06", name: "state_carries", seconds: 12.0, kind: Kind::Studio, build: c06_state_carries::build },
        SceneDef { id: "C07", name: "three_frames", seconds: 12.0, kind: Kind::Studio, build: c07_three_frames::build },
        SceneDef { id: "C08", name: "the_tokens", seconds: 10.0, kind: Kind::Studio, build: c08_the_tokens::build },
        SceneDef { id: "C09", name: "build_ships", seconds: 12.0, kind: Kind::Studio, build: c09_build_ships::build },
        // ── Movement IV · THE PROOF — the ledger, the pull-back ──────────
        SceneDef { id: "D01", name: "the_ledger", seconds: 14.0, kind: Kind::Pure, build: d01_the_ledger::build },
        SceneDef { id: "D02", name: "the_pullback", seconds: 12.0, kind: Kind::Pure, build: d02_the_pullback::build },
        // ── Movement V · ZERO — the release ──────────────────────────────
        SceneDef { id: "D03", name: "zero_distance", seconds: 14.0, kind: Kind::Pure, build: d03_zero_distance::build },
        SceneDef { id: "D04", name: "the_endcard", seconds: 18.0, kind: Kind::Pure, build: d04_the_endcard::build },
        SceneDef { id: "D05", name: "the_hold", seconds: 10.0, kind: Kind::Pure, build: d05_the_hold::build },
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

// ── The distance — the film's one number ────────────────────────────────────

/// The distance story, as the readout tells it. Each entry is (film
/// seconds, milliseconds) — the readout interpolates between them on a
/// log scale, because the story is a collapse, and a linear metre would
/// spend all five minutes in the old world's thousands.
///
/// The values are the film's narration, anchored to real ones where the
/// film has them: ~32 ms is near the *measured* edit-to-pixels latency of
/// the real studio at master resolution (the census's `alive_seconds`,
/// printed beside the narration wherever it is quoted); 16 ms is one
/// frame at 60 fps — the floor.
pub fn distance_ms(abs: f32) -> f32 {
    // (film seconds, ms) — the collapse's key frames.
    const STORY: &[(f32, f32)] = &[
        (0.0, 45_000.0),   // the question is asked
        (58.0, 45_000.0),  // the whole pain act holds it
        (72.0, 45_000.0),  // the tolls still stand
        (86.0, 8_400.0),   // three trees land
        (100.0, 2_600.0),  // the rasterizer
        (114.0, 620.0),    // the budget holds
        (128.0, 120.0),    // the studio opens
        (140.0, 32.0),     // first paint — near the real measure
        (232.0, 32.0),     // the whole studio act
        (258.0, 16.0),     // one frame
        (274.0, 16.0),     // held
        (300.0, 1.0),      // zero — the caret touches the screen
    ];
    let abs = abs.clamp(0.0, total_seconds());
    let mut ms = STORY[0].1;
    for pair in STORY.windows(2) {
        let (t0, m0) = pair[0];
        let (t1, m1) = pair[1];
        if abs >= t0 && abs <= t1 {
            let f = if t1 > t0 { (abs - t0) / (t1 - t0) } else { 0.0 };
            // Log-space interpolation — the collapse reads as a collapse.
            let l0 = m0.max(1.0).log10();
            let l1 = m1.max(1.0).log10();
            ms = 10.0f32.powf(l0 + (l1 - l0) * f);
            break;
        }
        if abs > t1 {
            ms = m1;
        }
    }
    ms
}

/// The distance readout's spelling — comma-grouped, `ms` suffixed.
pub fn distance_text(ms: f32) -> String {
    if ms >= 1000.0 {
        format!("{} ms", group_commas(ms.round() as u64))
    } else if ms >= 100.0 {
        format!("{:.0} ms", ms)
    } else {
        format!("{:.1} ms", ms.max(0.1))
    }
}

// ── The tap ladder — one number per human touch, never reset ────────────────

/// The seven touches, in absolute film seconds — exactly the script's
/// action times (`script.rs` is the source of truth): the live preview
/// opens (C02, `AcceptLive`) · the compose edit (C03) · the demo row's
/// tap fires (C04, `PointerUp`) · Render fires (C05) · *Add one* is
/// tapped (C05) · the build view opens (C09) · the devices land (C09).
pub fn taps() -> Vec<f32> {
    let at = |i: usize, frac: f32| scene_start(i) + scenes()[i].seconds * frac;
    vec![
        at(10, 2.20 / 12.0), // 1 · AcceptLive at 140.2
        at(11, 4.00 / 12.0), // 2 · the compose edit at 154.0
        at(12, 4.50 / 8.0),  // 3 · the row tap fires at 166.5
        at(13, 6.20 / 16.0), // 4 · Render fires at 176.2
        at(13, 8.62 / 16.0), // 5 · Add one at 178.62
        at(16, 3.30 / 12.0), // 6 · the build view at 223.3
        at(16, 8.80 / 12.0), // 7 · the devices land at 228.8
    ]
}

/// The witness counter at absolute time `abs`.
pub fn ladder_at(abs: f32) -> u32 {
    taps().iter().filter(|&&tap| abs >= tap).count() as u32
}

/// A short pulse envelope right after the most recent tap.
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

/// The session clock — mm:ss of the session, which is the film.
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
    /// The measured edit→visible latency of the real studio at master
    /// resolution, seconds — the film's own distance receipt.
    pub alive_seconds: f32,
    /// The median sampled build+raster frame time at 1080p.
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
            "{} · {} · rust {} · {} embedded faces",
            std::env::consts::OS,
            std::env::consts::ARCH,
            "1.98.1",
            fonts,
        )
    }
}

// ── The certification receipts — quoted from the repo's own audit ──────────

/// The 2026-09-15 certification run, quoted verbatim from
/// `vieww_base/docs/VIEWW-PHASE-STATUS.md` (2-core container, lavapipe,
/// rustc 1.95.0). The ledger scene counts these up; nothing here is
/// measured by the film, and it says so.
pub const CERT_STARTUP_MS: f32 = 27.5;
pub const CERT_P95_MS: f32 = 11.4;
pub const CERT_WORST_MS: f32 = 13.1;
pub const CERT_TESTS: u64 = 4_225;
pub const CERT_VULKAN_TESTS: u64 = 37;
pub const CERT_CRATES: usize = 36;
pub const CERT_FRAMES_STEADY: u64 = 60;
pub const CERT_ALLOCS_STEADY: u64 = 0;

// ── The 36 crates — the engine scene's manifest, quoted ─────────────────────

/// The workspace's 36 crates, verbatim from `vieww_base/Cargo.toml`.
/// B02 counts this list at runtime; the count on screen is the list's
/// own length.
pub const CRATES: [&str; 36] = [
    "vieww-foundation",
    "vieww-widget",
    "vieww-element",
    "vieww-paint",
    "vieww-hal",
    "vieww-text",
    "vieww-animation",
    "vieww-asset",
    "vieww-gestures",
    "vieww-render",
    "vieww-scene",
    "vieww-render-graph",
    "vieww-render-planner",
    "vieww-runtime",
    "vieww-gpu",
    "vieww-shaders",
    "vieww-reload",
    "vieww-platform",
    "vieww-platform-winit",
    "vieww-platform-web",
    "vieww-platform-web-dom",
    "vieww-hardware",
    "vieww-effects",
    "vieww-devtools",
    "vieww",
    "vieww-say-codegen",
    "vieww-test-harness",
    "vieww-image",
    "vieww-interaction",
    "vieww-scroll",
    "vieww-accessibility",
    "vieww-plugin",
    "vieww-plugin-macros",
    "vieww-widget-macros",
    "vieww-build",
    "vieww-cli",
];

// ── The mark — the studio's own logo, drawn by the studio's own code ─────────

/// The viewwstudio mark — the studio's own code-drawn logo, used
/// verbatim (`viewwstudio::ui::brand::revealed`) so the film's logo and
/// the product's cannot drift. `editor` and `preview` run `0..=1` and are
/// each a fade and a scale about the panel's own centre.
pub fn brand_mark(side: f32, editor: f32, preview: f32) -> WidgetNode {
    viewwstudio::ui::brand::revealed(side, editor, preview)
}

// ── Color arithmetic ─────────────────────────────────────────────────────────

pub fn alpha(c: Color, a: f32) -> Color {
    Color::rgba(c.r, c.g, c.b, (a * 255.0).clamp(0.0, 255.0) as u8)
}

pub fn mix(a: Color, b: Color, t: f32) -> Color {
    a.lerp(b, t.clamp(0.0, 1.0))
}

/// A rect from origin + size — the (x, y, w, h) habit.
pub fn xywh(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x, y, x + w.max(0.0), y + h.max(0.0))
}

/// Lighten toward white (a "tint").
pub fn tint(c: Color, t: f32) -> Color {
    mix(c, Color::WHITE, t)
}

pub fn clamp01(t: f32) -> f32 {
    t.clamp(0.0, 1.0)
}

/// An *analytic* underdamped spring settle — the scrub-safe shadow of the
/// real `SpringAnimation`, used wherever the film's clock (not a ticker)
/// drives the motion.
pub fn spring_out(t: f32, omega: f32, zeta: f32) -> f32 {
    let t = clamp01(t);
    let decay = (-zeta * omega * t).exp();
    1.0 - decay * ((1.0 - zeta * zeta).sqrt() * omega * t).cos()
}

// ── The film's fonts — the site's Geist, behind the embedded faces ───────────

/// The site's Geist faces, included from the site's own assets — the
/// brand's typography, byte-for-byte what the product page ships.
const GEIST: &[u8] = include_bytes!("../../../../apps/viewwsite/assets/fonts/vw-Geist-Regular.ttf");
const GEIST_MEDIUM: &[u8] = include_bytes!("../../../../apps/viewwsite/assets/fonts/vw-Geist-Medium.ttf");
const GEIST_BOLD: &[u8] = include_bytes!("../../../../apps/viewwsite/assets/fonts/vw-Geist-Bold.ttf");
const GEIST_MONO: &[u8] = include_bytes!("../../../../apps/viewwsite/assets/fonts/vw-GeistMono-Regular.ttf");
const GEIST_MONO_MEDIUM: &[u8] = include_bytes!("../../../../apps/viewwsite/assets/fonts/vw-GeistMono-Medium.ttf");

/// The film's font store: the embedded faces for the generic families —
/// exactly the store the real studio binary runs — with the site's Geist
/// registered behind them for the film's *own* voice. No system scan: the
/// store is a property of the checkout, and glyph determinism with it.
pub fn fonts() -> vieww_text::FontStore {
    let mut store = vieww_text::FontStore::embedded_only();
    store.load_font_data(GEIST.to_vec());
    store.load_font_data(GEIST_MEDIUM.to_vec());
    store.load_font_data(GEIST_BOLD.to_vec());
    store.load_font_data(GEIST_MONO.to_vec());
    store.load_font_data(GEIST_MONO_MEDIUM.to_vec());
    store
}

/// A text style in the brand's Geist (the film's display voice).
pub fn geist(size: f32) -> TextStyle {
    TextStyle::new(size).family(vieww_foundation::FontFamily::Named("Geist"))
}

/// A text style in the brand's Geist Mono (the film's instrument voice).
pub fn geist_mono(size: f32) -> TextStyle {
    TextStyle::new(size).family(vieww_foundation::FontFamily::Named("Geist Mono"))
}

// ── Shared composition helpers — the film's house look ───────────────────────

/// A soft radial glow blob, drawn as its own blurred layer.
pub fn glow(book: &mut Sketchbook, x: f32, y: f32, r: f32, color: Color, a: f32) {
    book.layer(1.0, (r * 0.16).max(2.0), None, |g| {
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

/// The standard ground gradient — the site's warm ground, deepening at
/// the floor.
pub fn ground(book: &mut Sketchbook, w: f32, h: f32) {
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::vertical().with_dither().with_stops(&[
            (0.0, Color::rgb(0x12, 0x10, 0x0D)),
            (0.6, GROUND),
            (1.0, Color::rgb(0x0C, 0x0B, 0x09)),
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

/// The film's caption — bottom-left, mono, tracked, with an accent tick.
/// `appear` in `[0, 1]` fades it in with a slight rise.
pub fn caption(text: &str, y: f32, appear: f32) -> WidgetNode {
    let a = crate::film_lib::ease_out_cubic(appear.clamp(0.0, 1.0));
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
                        book.rrect(xywh(0.0, 0.0, 4.0, 28.0), 2.0, alpha(ACCENT, 0.9));
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
                            geist_mono(22.0)
                                .letter_spacing(1.8)
                                .color(alpha(INK, 0.92)),
                        )
                        .align(TextAlign::Left),
                )),
        )
        .into()
}

/// A centered caption — key info stays center-frame.
pub fn caption_center(text: &str, y: f32, appear: f32) -> WidgetNode {
    let a = crate::film_lib::ease_out_cubic(appear.clamp(0.0, 1.0));
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
                            geist_mono(22.0)
                                .letter_spacing(1.8)
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
        .color(alpha(SURFACE, 0.88))
        .radius(7.0)
        .border(vieww_foundation::Border::new(alpha(fg, 0.22), 1.0))
        .padding(vieww_foundation::EdgeInsets::symmetric(7.0, 11.0))
        .child(
            Text::new(text)
                .style(geist_mono(size).letter_spacing(1.1).color(fg)),
        )
        .into()
}

/// A receipt chip row — chips laid out left-to-right at a baseline,
/// fading in staggered.
pub fn chip_row(chips: &[(&str, Color)], x: f32, y: f32, appear: f32) -> WidgetNode {
    let a = crate::film_lib::ease_out_cubic(appear.clamp(0.0, 1.0));
    if a <= 0.01 {
        return Stack::new().into();
    }
    let mut stack = Stack::new();
    let mut cx = x;
    for (i, (text, color)) in chips.iter().enumerate() {
        let chip_a = ((a - i as f32 * 0.08) / 0.6).clamp(0.0, 1.0);
        if chip_a <= 0.01 {
            continue;
        }
        let w = gmono_w(14.0, text.len()) + 26.0;
        let rise = (1.0 - crate::film_lib::ease_out_cubic(chip_a)) * 10.0;
        stack = stack.push(
            Positioned::new()
                .left(cx)
                .top(y + rise)
                .width(w)
                .height(30.0)
                .child(Opacity::new(chip_a).child(chip(text.to_string(), 14.0, *color))),
        );
        cx += w + 10.0;
    }
    stack.into()
}

/// Comma-grouped integer — the receipts' spelling.
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

/// Geist Mono's advance in ems — layout arithmetic for the mono grid.
pub const GEIST_MONO_ADV: f32 = 0.6035;

/// A Geist Mono run's pixel width.
pub fn gmono_w(size: f32, chars: usize) -> f32 {
    size * GEIST_MONO_ADV * chars as f32
}

/// A count-up value — integer part eased, so digits roll to their rest.
pub fn count_up(target: u64, progress: f32) -> u64 {
    let e = crate::film_lib::ease_out_cubic(progress.clamp(0.0, 1.0));
    ((target as f32 * e).round()) as u64
}

// ── The gap — the film's spine, drawn where a scene needs it ────────────────

/// The gap line: a dashed axis from the caret pole (left) to the screen
/// pole (right), whose dashes crawl toward the screen. `close` in
/// `[0, 1]` moves the *right* endpoint toward the left (1 = the poles
/// touch). `sag` bends the line under the tolls' weight; `fray` (in
/// `[0, 1]`) breaks the dashes into disorder — the drift act's fraying.
pub fn gap_line(
    book: &mut Sketchbook,
    y: f32,
    x0: f32,
    x1: f32,
    close: f32,
    t: f32,
    sag: f32,
    fray: f32,
    color: Color,
    a: f32,
) {
    let a = a.clamp(0.0, 1.0);
    if a <= 0.01 {
        return;
    }
    let close = close.clamp(0.0, 1.0);
    let reach = x0 + (x1 - x0) * (1.0 - close);
    // The dashes: 26 segments, crawling toward the screen at a speed that
    // reads as data in transit, not as a marquee.
    let n = 26;
    let crawl = (t * 0.35).fract();
    for i in 0..n {
        let u = (i as f32 + crawl) / n as f32;
        if u > 1.0 {
            continue;
        }
        let x = x0 + (reach - x0) * u;
        // The sag — a parabola under the middle, deepening with `sag`.
        let sy = y + (u * (1.0 - u) * 4.0) * sag;
        // The fray — deterministic per-dash disorder past `fray`.
        let (jx, jy, ja) = if fray > 0.01 {
            let mut rng = Rng::new(0xD15C ^ (i as u64).wrapping_mul(0x9E37));
            let k = fray * (0.3 + rng.f01() * 0.7);
            (
                x + rng.sym() * 26.0 * k,
                sy + rng.sym() * 18.0 * k,
                a * (1.0 - k).max(0.0),
            )
        } else {
            (x, sy, a)
        };
        let dash_w = 14.0 * (1.0 - fray * 0.5);
        book.line(
            Offset::new(jx, jy),
            Offset::new(jx + dash_w, jy),
            alpha(color, ja * 0.6),
            2.0,
        );
    }
}

/// The left pole — the caret: a blinking terminal point with a dormant
/// glow (the thought, waiting to become a screen).
pub fn pole_caret(book: &mut Sketchbook, x: f32, y: f32, sec: f32, a: f32) {
    let a = a.clamp(0.0, 1.0);
    if a <= 0.01 {
        return;
    }
    let on = (sec * 2.2).fract() < 0.55;
    glow(book, x, y, 90.0, ACCENT, a * 0.16);
    book.ring(Offset::new(x, y), 13.0, 1.2, alpha(tint(ACCENT, 0.4), a * 0.7));
    if on {
        book.rrect(xywh(x - 2.5, y - 22.0, 5.0, 44.0), 2.0, alpha(tint(ACCENT, 0.4), a));
        book.rrect(xywh(x - 1.5, y - 18.0, 3.0, 36.0), 1.5, alpha(Color::WHITE, a * 0.85));
    }
}

/// The right pole — the screen: a device outline waiting to light up.
/// `lit` in `[0, 1]` is how awake it is; the outline is a rounded rect
/// with a home tick beneath it, and when lit, a soft inner glow.
pub fn pole_screen(book: &mut Sketchbook, x: f32, y: f32, lit: f32, a: f32, color: Color) {
    let a = a.clamp(0.0, 1.0);
    if a <= 0.01 {
        return;
    }
    let w = 108.0;
    let h = 76.0;
    let r = 10.0;
    let body = xywh(x - w * 0.5, y - h * 0.5, w, h);
    if lit > 0.02 {
        glow(book, x, y, 120.0, color, lit * 0.22 * a);
    }
    book.stroke_rrect(body, r, alpha(color, a * (0.45 + 0.5 * lit)), 2.0);
    // The screen's inner surface, waking with `lit`.
    if lit > 0.02 {
        book.rrect(
            xywh(x - w * 0.5 + 7.0, y - h * 0.5 + 7.0, w - 14.0, h - 14.0),
            5.0,
            alpha(color, lit * 0.16 * a),
        );
    }
    // The home tick.
    book.line(
        Offset::new(x - 9.0, y + h * 0.5 + 9.0),
        Offset::new(x, y + h * 0.5 + 9.0),
        alpha(color, a * 0.5),
        2.0,
    );
    book.line(
        Offset::new(x, y + h * 0.5 + 9.0),
        Offset::new(x + 9.0, y + h * 0.5 + 9.0),
        alpha(color, a * 0.5),
        2.0,
    );
}

/// The distance readout — the film's persistent instrument: a centered
/// chip at the bottom of the frame carrying the one number the film is
/// about.
pub fn distance_chip(abs: f32, a: f32) -> WidgetNode {
    if a <= 0.01 {
        return Stack::new().into();
    }
    let ms = distance_ms(abs);
    let text = distance_text(ms);
    let label = "the distance";
    let w = gmono_w(15.0, label.len() + text.len() + 3) + 36.0;
    Stack::new()
        .push(
            Positioned::new()
                .left(W * 0.5 - w * 0.5)
                .top(1008.0)
                .width(w)
                .height(34.0)
                .child(Opacity::new(a).child(Painting::sized(
                    Size::new(w, 34.0),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        book.rrect(xywh(0.0, 0.0, w, 30.0), 9.0, alpha(SURFACE_2, 0.92));
                        book.stroke_rrect(xywh(0.0, 0.0, w, 30.0), 9.0, alpha(ACCENT, 0.28), 1.1);
                        book.circle(Offset::new(16.0, 15.0), 3.5, alpha(ACCENT, 0.7));
                    }),
                ))),
        )
        .push(
            Positioned::new()
                .left(W * 0.5 - w * 0.5 + 30.0)
                .top(1014.0)
                .width(w - 36.0)
                .height(22.0)
                .child(
                    Text::new(format!("{label} · {text}"))
                        .style(geist_mono(15.0).letter_spacing(1.5).color(alpha(INK, 0.9)))
                        .align(TextAlign::Left),
                ),
        )
        .into()
}

/// The act chip — the movement's name.
pub fn act_chip(movement: &str, name: &str, appear: f32) -> WidgetNode {
    let a = crate::film_lib::ease_out_cubic(appear.clamp(0.0, 1.0));
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
                    Text::new(format!("{movement} — {name}"))
                        .style(
                            geist_mono(15.0)
                                .letter_spacing(4.5)
                                .color(alpha(MUTED, 0.85)),
                        )
                        .align(TextAlign::Left),
                )),
        )
        .into()
}

/// The film's progress rail — a hairline at the very bottom, one tick per
/// scene, the playhead sliding.
pub fn progress_rail(abs: f32) -> WidgetNode {
    let total = total_seconds();
    let frac = (abs / total).clamp(0.0, 1.0);
    let starts: Vec<f32> = (0..scenes().len()).map(scene_start).collect();
    Painting::sized(Size::new(W, 26.0), PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
        let x0 = 180.0;
        let x1 = W - 180.0;
        let y = 18.0;
        book.line(Offset::new(x0, y), Offset::new(x1, y), alpha(Color::WHITE, 0.07), 1.0);
        for (i, st) in starts.iter().enumerate() {
            let x = x0 + (x1 - x0) * (st / total);
            // The five movements' boundaries.
            let major = i == 0 || i == 1 || i == 5 || i == 9 || i == 18 || i == 20;
            book.line(
                Offset::new(x, y - if major { 6.0 } else { 4.0 }),
                Offset::new(x, y + if major { 6.0 } else { 4.0 }),
                alpha(Color::WHITE, if major { 0.22 } else { 0.13 }),
                1.0,
            );
        }
        let px = x0 + (x1 - x0) * frac;
        book.circle(Offset::new(px, y), 3.0, alpha(ACCENT, 0.9));
        book.line(Offset::new(px, y), Offset::new(px, y + 10.0), alpha(ACCENT, 0.55), 1.2);
    }))
    .into()
}

// ── The studio scenes' chrome — the film's voice over the real product ───────
//
// The studio renders as itself, ungraded: no vignette, no grain, no flash
// over its pixels (the honesty rule — visible ⇒ the product rendered it).
// The film's own layer is purely annotative.

/// The witness chip — bottom-right: the counter of human touches, blooming
/// on every tap.
pub fn witness_chip(ladder: u32, pulse: f32) -> WidgetNode {
    Stack::new()
        .push(
            Positioned::new()
                .left(W - 268.0)
                .top(976.0)
                .width(240.0)
                .height(38.0)
                .child(Painting::sized(Size::new(240.0, 38.0), PaintWith::new(
                    move |book: &mut Sketchbook, _s: Size| {
                        if pulse > 0.02 {
                            glow(book, 120.0, 19.0, 90.0, ACCENT, pulse * 0.30);
                        }
                        book.rrect(xywh(0.0, 2.0, 240.0, 34.0), 9.0, alpha(SURFACE_2, 0.92));
                        book.stroke_rrect(xywh(0.0, 2.0, 240.0, 34.0), 9.0, alpha(ACCENT, 0.30), 1.1);
                        book.circle(Offset::new(22.0, 19.0), 5.0, alpha(ACCENT, 0.55 + pulse * 0.45));
                        book.ring(Offset::new(22.0, 19.0), 9.0 + pulse * 5.0, 1.2, alpha(ACCENT, 0.5 * pulse));
                    },
                ))),
        )
        .push(
            Positioned::new()
                .left(W - 268.0 + 38.0)
                .top(976.0 + 9.0)
                .width(200.0)
                .height(24.0)
                .child(
                    Text::new(format!("witness · {}", ladder))
                        .style(geist_mono(14.0).letter_spacing(1.6).color(alpha(INK, 0.88)))
                        .align(TextAlign::Left),
                ),
        )
        .into()
}

/// The session chip — the film's clock, under the witness.
pub fn session_chip(abs: f32, a: f32) -> WidgetNode {
    if a <= 0.01 {
        return Stack::new().into();
    }
    Stack::new()
        .push(
            Positioned::new()
                .left(W - 268.0)
                .top(1016.0)
                .width(240.0)
                .height(28.0)
                .child(Opacity::new(a).child(
                    Text::new(format!("session · {}", session_clock(abs)))
                        .style(geist_mono(13.0).letter_spacing(1.6).color(alpha(MUTED, 0.9)))
                        .align(TextAlign::Left),
                )),
        )
        .into()
}

/// The studio scenes' shared chrome: the caption scrim, the act chip,
/// the witness, and the session clock.
pub fn studio_chrome(ctx: &Ctx) -> Stack {
    let abs = ctx.abs;
    Stack::new()
        .push(Positioned::fill().child(Painting::sized(
            CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                book.rect(
                    xywh(0.0, H - 150.0, W, 150.0),
                    Gradient::vertical().with_dither().with_stops(&[
                        (0.0, alpha(Color::BLACK, 0.0)),
                        (1.0, alpha(Color::BLACK, 0.42)),
                    ]),
                );
            }),
        )))
        .push(act_chip("MOVEMENT III", "THE STUDIO", clamp01((ctx.sec - 0.3) / 0.5)))
        .push(witness_chip(ctx.ladder, tap_pulse(ctx.abs)))
        .push(session_chip(abs, clamp01((ctx.sec - 1.2) / 0.6)))
}

/// The tap ring — the film's annotation of a real interaction.
pub fn tap_ring_at(at: Offset, since: f32) -> WidgetNode {
    if !(0.0..=1.0).contains(&since) {
        return Stack::new().into();
    }
    let r = 12.0 + since * 130.0;
    let a = (1.0 - since).max(0.0);
    let size = 360.0;
    Stack::new()
        .push(
            Positioned::new()
                .left(at.dx - size * 0.5)
                .top(at.dy - size * 0.5)
                .width(size)
                .height(size)
                .child(Painting::sized(
                    Size::new(size, size),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        let cx = size * 0.5;
                        let cy = size * 0.5;
                        book.ring(Offset::new(cx, cy), r, 2.6, alpha(ACCENT, a * 0.85));
                        book.ring(Offset::new(cx, cy), r * 0.6, 1.4, alpha(tint(ACCENT, 0.4), a * 0.6));
                        if a > 0.5 {
                            glow(book, cx, cy, 90.0, ACCENT, (a - 0.5) * 0.5);
                        }
                    }),
                )),
        )
        .into()
}

/// A receipt chip row pinned bottom-right.
pub fn receipt_row(chips: &[(&str, Color)], appear: f32) -> WidgetNode {
    chip_row(chips, W - 900.0, 936.0, appear)
}

// ── Film grain and dust — the film's air ─────────────────────────────────────

/// Film grain — round specks, per-frame seed.
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

/// A full-frame flash — the match-cut breath between scenes.
pub fn flash(book: &mut Sketchbook, w: f32, h: f32, a: f32, color: Color) {
    let a = a.clamp(0.0, 1.0);
    if a <= 0.004 {
        return;
    }
    book.rect(Rect::new(0.0, 0.0, w, h), alpha(tint(color, 0.6), a * 0.85));
}

// ── The scene modules ────────────────────────────────────────────────────────

pub mod master;
pub mod script;
mod s01_the_two_points;
mod a01_the_wait;
mod a02_the_tolls;
mod a03_the_jank;
mod a04_the_drift;
mod b01_three_trees;
mod b02_the_crates;
mod b03_the_rasterizer;
mod b04_the_budget;
mod c01_studio_opens;
mod c02_first_paint;
mod c03_live_compose;
mod c04_the_tap;
mod c05_say_to_rust;
mod c06_state_carries;
mod c07_three_frames;
mod c08_the_tokens;
mod c09_build_ships;
mod d01_the_ledger;
mod d02_the_pullback;
mod d03_zero_distance;
mod d04_the_endcard;
mod d05_the_hold;
