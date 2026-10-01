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

use vieww_foundation::{Color, Offset, Rect, Size, Transform};
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
    /// The camera this frame is seen through.
    ///
    /// A scene reads it to place things *in depth*: the camera moves the
    /// whole world uniformly, which is flat by construction, so anything
    /// that should sit behind or in front of the subject has to displace
    /// itself against the move. That is what `parallax` is for.
    pub cam: Cam,
}

impl Ctx<'_> {
    /// How far a layer at `depth` should displace against the camera.
    ///
    /// `depth` 0 is the subject plane — it moves exactly with the camera
    /// and gets no displacement. Positive depth is *behind* it and lags;
    /// 1.0 lags completely, which is to say it ignores the camera's pan
    /// and behaves like a painted backdrop. Negative depth is in front
    /// and leads, which is what foreground dust wants.
    ///
    /// This compensates the camera's **pan** only, not its zoom: the zoom
    /// is shared by every layer, because a translation cannot express a
    /// difference of scale and pretending otherwise would put the layers
    /// out of register. A layer that also wants to recede under a push
    /// scales itself.
    pub fn parallax(&self, depth: f32) -> Offset {
        let c = self.cam;
        Offset::new(
            (c.at.dx - W * 0.5) * depth,
            (c.at.dy - H * 0.5) * depth,
        )
    }
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
            // Asked of the compiler that built this binary (build.rs),
            // never typed: the bench identity is a receipt like every
            // other number the film puts on screen.
            env!("FILM_RUSTC_VERSION"),
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
pub const CERT_CRATES: usize = 49;
pub const CERT_FRAMES_STEADY: u64 = 60;
pub const CERT_ALLOCS_STEADY: u64 = 0;

// ── The 49 crates — the engine scene's manifest, quoted ─────────────────────

/// The workspace's 49 crates, verbatim from `vieww_base/Cargo.toml`
/// (the `crates/` members; the two `apps/` are the products built on them,
/// not the engine). B02 and the receipts scene count this list at runtime;
/// the count on screen is the list's own length. The thirteen additions
/// over the first cut — audio, video, mesh, 3d, canvas, dataviz, graph,
/// game, collab, lottie, network, physics, embed — are the capabilities
/// Z10B shows running live.
pub const CRATES: [&str; 49] = [
    "vieww-foundation",
    "vieww-widget",
    "vieww-element",
    "vieww-paint",
    "vieww-hal",
    "vieww-text",
    "vieww-animation",
    "vieww-audio",
    "vieww-video",
    "vieww-mesh",
    "vieww-3d",
    "vieww-canvas",
    "vieww-dataviz",
    "vieww-graph",
    "vieww-game",
    "vieww-collab",
    "vieww-lottie",
    "vieww-network",
    "vieww-physics",
    "vieww-embed",
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
    stars_deep(book, w, h, seed, n, t, base, Offset::ZERO);
}

/// The star field, in depth.
///
/// A camera that moves everything by the same amount is a camera looking
/// at a painted flat, which is what the film's ground and dust were: one
/// plane, uniformly transformed. Giving the field three strata and letting
/// each lag the camera's pan by a different amount is what turns a move
/// into a *space* — the near dust slides past, the far stars barely
/// answer, and the subject sits between them. It costs one offset per
/// star and it is the difference between a zoom and a dolly.
///
/// `pan` is the camera displacement to work against — `Ctx::parallax(1.0)`.
#[allow(clippy::too_many_arguments)]
pub fn stars_deep(
    book: &mut Sketchbook,
    w: f32,
    h: f32,
    seed: u64,
    n: usize,
    t: f32,
    base: f32,
    pan: Offset,
) {
    let mut rng = Rng::new(seed);
    for _ in 0..n {
        let x = rng.f01() * w;
        let y = rng.f01() * h;
        let r = 0.4 + rng.f01() * 0.9;
        let tw = 0.5 + 0.5 * (t * 3.2 + rng.f01() * 9.0).sin();
        // Three strata, keyed off the star's own size so the near ones
        // (larger, brighter) are the ones that slide.
        let depth = if r > 1.05 {
            -0.22
        } else if r > 0.75 {
            0.35
        } else {
            0.78
        };
        book.circle(
            Offset::new(x + pan.dx * depth, y + pan.dy * depth),
            r,
            alpha(Color::WHITE, base * (0.4 + 0.6 * tw)),
        );
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

// ── The band — the film's own strip, reserved at the top ────────────────────
//
// The narration used to live in the bottom third, and it shared that third
// with two other things that were laid out independently of it: the
// distance readout, centred at `W * 0.5`, and (in the studio act) the
// app's own bottom panel tab bar. Nothing reserved a lane, so nothing had
// to agree, and the result was a caption with a hole punched through it in
// most of the film — "every number on screen is a receipt — counted,
// n▉▉▉▉▉▉▉" being the one that hurt, since that is the thesis.
//
// So the film takes a strip and keeps it. The band is film chrome only:
// the rail, the movement label, and the caption stack. Nothing else may be
// laid out inside it, and the caption never leaves it. The distance
// readout stays at the bottom, alone, where it is now the only thing —
// two instruments, two ends of the frame, and no arithmetic between them.
//
// Reading order comes with it. The old stack put the newest line *above*
// the older one, and cross-faded the new line in at a lower opacity than
// the line it was replacing, so the eye was pulled to the stale line and
// then had to read upward. Here the older line sits higher and dimmer and
// the newest line sits at the bottom of the band at full weight — the
// subtitle convention, which is the convention because it is the one that
// reads.

/// The reserved band's height. Nothing outside `mod.rs` lays out inside it.
pub const BAND_H: f32 = 280.0;
/// The older caption line — higher in the band, and dimmer.
pub const CAP_Y0: f32 = 98.0;
/// The newest caption line — the bottom of the band, at full weight.
pub const CAP_Y1: f32 = 152.0;
/// The movement label's baseline inside the band.
pub const ACT_Y: f32 = 40.0;

/// Map a scene's legacy bottom-third caption `y` into the band.
///
/// The scenes call `caption` with one of two literals — `1002.0` for the
/// line that arrives first and `966.0` for the line that replaces it (plus
/// a lone `1006.0` on the end card). Rather than touch forty-one call
/// sites and risk transposing a pair, the remap happens here, and it
/// **flips** the order on the way: the first line is the older line, so in
/// the band it belongs on top.
fn band_slot(legacy_y: f32) -> f32 {
    if legacy_y >= 990.0 {
        CAP_Y0
    } else {
        CAP_Y1
    }
}

/// The band's scrim — a plate under the film's own chrome.
///
/// In the pure scenes this is barely visible over the ground. In the
/// studio act it is doing real work: the app's menu bar and tab strip live
/// at the very top of the frame, and the movement label used to be drawn
/// straight through the breadcrumb beneath them — "M O V E M E N T  I I I"
/// and "product_film_workspace › live.rs" overprinted into an unreadable
/// smear for the whole hundred and four seconds. The scrim holds the two
/// apart. It stops short of opaque on purpose: the app is still faintly
/// there under the film's voice, which is the honest picture.
pub fn band_scrim(strength: f32) -> WidgetNode {
    if strength <= 0.01 {
        return Stack::new().into();
    }
    chrome(
    Positioned::new()
        .left(0.0)
        .top(0.0)
        .width(W)
        .height(BAND_H + 30.0)
        .child(Painting::sized(
            Size::new(W, BAND_H + 30.0),
            // The band's own height, not the size handed in: chrome is
            // composited inside a full-canvas box, so painting to `s`
            // stretched this gradient over all 1080 rows and dropped the
            // whole frame to a third of its luminance.
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                book.rect(
                    xywh(0.0, 0.0, W, BAND_H + 30.0),
                    Gradient::vertical().with_dither().with_stops(&[
                        (0.0, alpha(BG_DEEP, 0.94 * strength)),
                        (0.62, alpha(BG_DEEP, 0.90 * strength)),
                        (1.0, alpha(BG_DEEP, 0.0)),
                    ]),
                );
            }),
        ))
        .into(),
    )
}

/// The film's caption — in the band, mono, tracked, with an accent tick.
/// `appear` in `[0, 1]` fades it in with a slight rise.
///
/// The `y` argument is a scene's legacy bottom-third coordinate and is
/// remapped through [`band_slot`]; see the band note above.
pub fn caption(text: &str, y: f32, appear: f32) -> WidgetNode {
    let a = crate::film_lib::ease_out_cubic(appear.clamp(0.0, 1.0));
    if a <= 0.01 {
        return Stack::new().into();
    }
    let older = y >= 990.0;
    let y = band_slot(y);
    // The older line is held back to 0.55 so the newest line is always the
    // brightest thing in the band — the opposite of what the stack used to
    // do, where the arriving line faded up *underneath* the one it was
    // replacing and spent its first half-second dimmer than stale text.
    let a = if older { a * 0.55 } else { a };
    let rise = (1.0 - a) * 14.0;
    chrome(
    Stack::new()
        .push(
            Positioned::new()
                .left(84.0)
                .top(y + rise)
                .width(12.0)
                .height(36.0)
                .child(Opacity::new(a).child(Painting::sized(
                    Size::new(12.0, 36.0),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        book.rrect(xywh(0.0, 0.0, 5.0, 36.0), 2.5, alpha(ACCENT, 0.9));
                    }),
                ))),
        )
        .push(
            Positioned::new()
                .left(112.0)
                .top(y + rise)
                .width(1724.0)
                .height(42.0)
                .child(Opacity::new(a).child(
                    Text::new(text)
                        .style(
                            geist_mono(28.0)
                                .letter_spacing(1.6)
                                .color(alpha(INK, 0.94)),
                        )
                        .align(TextAlign::Left),
                )),
        )
        .into(),
    )
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

/// A chip's horizontal padding. Named because `chip_row` has to measure
/// what `chip` draws, and the two had drifted: the row added a flat 26 px
/// for a box whose real horizontal inset is `2 × 11` plus a 1 px border a
/// side. One constant, one truth.
pub const CHIP_PAD_X: f32 = 11.0;

/// A small chip — rounded label with a soft border, the receipts' container.
pub fn chip(text: impl Into<String>, size: f32, fg: Color) -> WidgetNode {
    Container::new()
        .color(alpha(SURFACE, 0.88))
        .radius(7.0)
        .border(vieww_foundation::Border::new(alpha(fg, 0.22), 1.0))
        .padding(vieww_foundation::EdgeInsets::symmetric(7.0, CHIP_PAD_X))
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
        // Tracked width, not the bare advance. `chip()` sets
        // `letter_spacing(1.1)`, so measuring with `gmono_w` alone lost
        // `1.1 × chars` — about 21 px on "remount: new screen", which is
        // more than the 10 px gap the row leaves between chips. Every
        // receipt row in the studio act had its chips growing into their
        // neighbour; the wider the label, the worse the overlap.
        let w = gmono_tw(14.0, text.chars().count(), 1.1) + CHIP_PAD_X * 2.0 + 2.0;
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

/// A Geist Mono run's pixel width, **advance only** — no tracking.
///
/// Prefer [`gmono_tw`] for anything that has to line up with laid-out text.
/// This one stays because a few call sites genuinely want the bare grid
/// (a glyph cell, a column step), and because silently folding tracking in
/// here would move those.
pub fn gmono_w(size: f32, chars: usize) -> f32 {
    size * GEIST_MONO_ADV * chars as f32
}

// ── Two spaces: the world, and the film's own voice ─────────────────────────
//
// The camera revealed a distinction the film had never needed to make.
// Everything it draws was in one tree, so the camera — which moves the
// whole command list — moved the captions and the rail along with the
// scene. A push from 1.0 to 2.35 on the rasteriser scene dragged the
// narration off the left edge of frame and scaled it to twice its size.
//
// Which is correct behaviour for a camera and the wrong place for a
// caption. A film's furniture — subtitles, the movement label, the
// timeline, the distance readout — does not live in the world the camera
// is looking at. It is composited over the finished picture, in *screen*
// space, at a fixed size, and it does not move when the camera does.
//
// So the chrome helpers no longer return a node into the scene's tree.
// They register themselves here, and the master drains them into a second
// tree that is composited after the camera transform and never subject to
// it. No scene file changes: `caption` is still called exactly where it
// was, it simply lands in the other space.

thread_local! {
    static CHROME: std::cell::RefCell<Vec<WidgetNode>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Register a node in **screen space** — composited over the frame, after
/// the camera, at a fixed size.
pub fn chrome(node: WidgetNode) -> WidgetNode {
    CHROME.with(|c| c.borrow_mut().push(node));
    Stack::new().into()
}

/// Drain the frame's screen-space chrome into one tree.
///
/// The master calls this straight after building a scene, so what comes
/// back is exactly what this frame registered and the buffer is left
/// empty for the next one.
pub fn take_chrome() -> WidgetNode {
    let items = CHROME.with(|c| std::mem::take(&mut *c.borrow_mut()));
    let mut stack = Stack::new();
    for item in items {
        stack = stack.push(Positioned::fill().child(item));
    }
    stack.into()
}

/// Throw away anything registered but not drained — the census and the
/// calibration probe build frames they never composite.
pub fn clear_chrome() {
    CHROME.with(|c| c.borrow_mut().clear());
}

// ── Measuring what will actually be drawn ───────────────────────────────────
//
// `vieww-text`'s own test for this says it in one line: *"the caret walks
// the same advances the glyphs were placed with. Measuring one way and
// drawing another puts the caret between the wrong letters."* The film was
// doing exactly that — placing glyphs through the shaper and placing the
// caret with `size × 0.6035 × chars`, a constant. A constant cannot
// account for tracking (which the shaper folds into the advances, one gap
// per glyph), and on the end card it could not even account for the
// *typeface*, since that line is set in proportional Geist and was being
// measured with a monospace advance.
//
// So nothing here estimates any more. The film already builds its own
// `FontStore` — the embedded faces plus the site's Geist, no system scan,
// deterministic by checkout — and that store can lay a run out and be
// asked where the caret goes. The store is built once per thread and the
// answers are memoised, because a type-on asks the same question about the
// same prefix on every frame it is on screen.

thread_local! {
    static SHAPER: std::cell::RefCell<Option<vieww_text::FontStore>> =
        const { std::cell::RefCell::new(None) };
    static MEASURED: std::cell::RefCell<
        std::collections::HashMap<(u64, u32, u32), (f32, f32)>,
    > = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Where the shaper puts a caret `bytes` into `text`, and how wide the
/// whole run is — both in logical pixels, both measured.
pub fn measured_caret(text: &str, style: TextStyle, bytes: usize) -> (f32, f32) {
    use std::hash::{Hash, Hasher};
    let key = {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut h);
        style.size.to_bits().hash(&mut h);
        style.letter_spacing.to_bits().hash(&mut h);
        format!("{:?}", style.family).hash(&mut h);
        (h.finish(), bytes as u32, style.size.to_bits())
    };
    if let Some(hit) = MEASURED.with(|m| m.borrow().get(&key).copied()) {
        return hit;
    }
    let out = SHAPER.with(|s| {
        let mut slot = s.borrow_mut();
        let store = slot.get_or_insert_with(fonts);
        let spans = [vieww_text::TextSpan::new(text, style)];
        let p = vieww_text::Paragraph::layout(store, &spans, f32::INFINITY);
        let caret = p.cursor_rect(vieww_foundation::TextPosition::new(bytes.min(text.len())));
        (caret.left, p.size().width)
    });
    MEASURED.with(|m| {
        m.borrow_mut().insert(key, out);
    });
    out
}

/// A Geist Mono run's **tracked** pixel width — advance plus the
/// `letter_spacing` the style actually sets.
///
/// `gmono_w` alone under-measures every tracked run by `tracking × chars`,
/// and the film tracks everything: chips at 1.1, the distance readout at
/// 1.5, the type-on lines at 1.6–2.0. That missing term is the single
/// arithmetic error behind a whole family of visible defects — chips
/// overlapping their neighbour in every Movement III receipt row, the
/// distance readout wrapping and spilling "ms" out the bottom of its own
/// pill, and the type-on caret finishing short of the last glyph. One term,
/// a dozen symptoms.
///
/// The shaper folds tracking **into each glyph's advance** rather than
/// placing it between them — `vieww-text`'s own test is titled
/// "letter_spacing lands in the advances rather than beside them", and
/// asserts "four glyphs at four pixels each". So an `n`-character run
/// carries `n` units of tracking, not `n - 1`.
///
/// This is still an estimate and still assumes a monospace face. Anything
/// that has to land on a glyph — a caret above all — should use
/// [`measured_caret`] instead and ask the shaper.
pub fn gmono_tw(size: f32, chars: usize, tracking: f32) -> f32 {
    if chars == 0 {
        return 0.0;
    }
    gmono_w(size, chars) + tracking * chars as f32
}

// ── The type-on — one anchor for the text and the caret ─────────────────────

/// How a type-on line is placed horizontally.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TypeAt {
    /// Left edge pinned at `x`.
    Left(i32),
    /// The **finished** line centred on `x`. The line does not re-centre as
    /// it types: it is laid out where it will end up and fills in from the
    /// left, which is what a caret can follow.
    CenteredOn(i32),
}

/// A line that types itself, with a caret that is actually on the end of it.
///
/// This exists because the caret and the text had drifted apart in three
/// different ways at once, and each one was invisible in the source:
///
/// 1. **Anchor mismatch.** The text was `TextAlign::Center` inside a
///    full-width box, so the rendered string *re-centred on every
///    keystroke* — its left edge walking left by half a glyph per
///    character. The caret was pinned to `W*0.5 - gmono_w(full)*0.5`, a
///    fixed point derived from the *finished* string. The error is
///    `(gmono_w(typed) - gmono_w(full)) / 2`, which starts at about
///    −190 px on a 22-character line and only closes on the last
///    character. The caret began in empty space before the first letter,
///    walked *through* the word overprinting glyphs, and arrived late.
/// 2. **Missing tracking.** Even with the anchors agreed, `gmono_w`
///    ignored `letter_spacing`, so the caret finished short by
///    `tracking × chars` — 44 px on a 2.0-tracked line.
/// 3. **Free-running blink.** The blink was `(sec * 2.6).fract()`, a phase
///    of the scene clock rather than of the typing, so the caret could be
///    dark for the exact beats a character landed on.
///
/// Here the text and the caret are computed from **one** `x0` and **one**
/// tracked measurement, so they cannot disagree by construction, and the
/// caret is solid while a character is actually landing — a real
/// caret is solid while you type.
///
/// 4. **The caret outstays its welcome.** The caret used to blink on a
///    finished line for the rest of the scene — a caret that keeps
///    blinking after the typing is done reads as a stuck input, not a
///    live one. It now leaves the moment the line is whole: the last
///    character's landing masks the departure, and the line rests as
///    type. (`clock` stays in the signature — the blink phase it drove
///    is retired, and callers keep their call sites.)
pub fn type_on(
    full: &str,
    at: TypeAt,
    y: f32,
    style: TextStyle,
    progress: f32,
    clock: f32,
) -> WidgetNode {
    let size = style.size;
    let p = clamp01(progress);
    let total = full.chars().count();
    let typed_n = (total as f32 * crate::film_lib::ease_out_cubic(p)).round() as usize;
    if typed_n == 0 {
        return Stack::new().into();
    }
    let shown: String = full.chars().take(typed_n).collect();

    // One anchor, and the shaper's own arithmetic for both halves of it.
    // The line is laid out where the *finished* line will sit and fills in
    // from the left — a centred line that re-centres per keystroke is a
    // line no caret can follow, and that was the original fault.
    let typed_bytes = shown.len();
    let (caret_x, full_w) = measured_caret(full, style, typed_bytes);
    let x0 = match at {
        TypeAt::Left(x) => x as f32,
        TypeAt::CenteredOn(x) => x as f32 - full_w * 0.5,
    };

    let mut stack = Stack::new().push(
        Positioned::new()
            .left(x0)
            .top(y)
            .width(full_w + size)
            .height(size * 1.45)
            .child(
                Text::new(shown)
                    .style(style)
                    .align(TextAlign::Left),
            ),
    );

    // The caret sits one advance past the last glyph — solid while the line
    // is still landing, gone the moment it is whole. The last character's
    // arrival masks the departure, so the line settles as finished type
    // rather than as an input still waiting.
    let done = typed_n >= total;
    let _ = clock;
    if !done {
        stack = stack.push(
            Positioned::new()
                .left(x0 + caret_x)
                .top(y + size * 0.16)
                .width(size * 0.1 + 2.0)
                .height(size * 1.02)
                .child(
                    Container::new()
                        .color(alpha(style.color, 0.95))
                        .radius(1.5),
                ),
        );
    }
    stack.into()
}

/// A count-up value — integer part eased, so digits roll to their rest.
pub fn count_up(target: u64, progress: f32) -> u64 {
    let e = crate::film_lib::ease_out_cubic(progress.clamp(0.0, 1.0));
    ((target as f32 * e).round()) as u64
}

// ── The camera ──────────────────────────────────────────────────────────────
//
// The film had none. Every element arrived by opacity — eighty-six
// `Opacity` wrappers, one `translate`, no `Transform` at all — and then sat
// perfectly still until the cut took it. Measured as mean inter-frame
// difference the whole picture runs at about 0.03/255, which is to say
// eighteen thousand frames were rendered to deliver what is, kinetically, a
// sequence of stills. Nothing in it needed sixty frames a second, or
// twenty-four.
//
// The camera is the cheapest way to change that, because it does not
// require touching a single scene: the frame is already a command list, and
// `Scene::append` exists precisely to lift a recording into another
// coordinate space without replaying the paint pass. So the film gets a
// camera the way it already got `SCALE_FACTOR` — after compositing, over
// the whole frame, one transform.
//
// **The studio act keeps the camera off.** Movement III's claim is that
// those are the product's own pixels, and a scaled frame is resampled
// pixels. `Cam::STILL` there is a correctness constraint, not an oversight.
//
// Zoom stays at or above 1.0 throughout. Below it the frame would show more
// world than the canvas paints and the star field's edge would walk into
// shot — so a "pull back" here is written as a move *down to* 1.0 from a
// tight start, which reads the same and cannot reveal the seam.

/// A 2D camera over the finished frame: a zoom about a held world point.
#[derive(Clone, Copy, PartialEq)]
pub struct Cam {
    /// Magnification. 1.0 is the canvas, 1:1.
    pub zoom: f32,
    /// The world point pinned to the centre of frame.
    pub at: Offset,
}

impl Cam {
    /// The canvas, untouched — and the identity the studio act holds.
    pub const STILL: Cam = Cam {
        zoom: 1.0,
        at: Offset::new(W * 0.5, H * 0.5),
    };

    /// A zoom about the canvas centre.
    pub fn zoom(z: f32) -> Cam {
        Cam { zoom: z, ..Cam::STILL }
    }

    /// A zoom about an arbitrary held point.
    pub fn at(z: f32, x: f32, y: f32) -> Cam {
        Cam {
            zoom: z,
            at: Offset::new(x, y),
        }
    }

    /// True when this camera cannot change a pixel, so the master can skip
    /// the copy entirely.
    pub fn is_still(&self) -> bool {
        (self.zoom - 1.0).abs() < 1.0e-4
            && (self.at.dx - W * 0.5).abs() < 0.01
            && (self.at.dy - H * 0.5).abs() < 0.01
    }

    /// Nudge the camera by a screen-space offset — the shake's door in.
    pub fn nudged(self, dx: f32, dy: f32) -> Cam {
        Cam {
            at: Offset::new(self.at.dx + dx, self.at.dy + dy),
            ..self
        }
    }

    /// The transform that takes world space to screen space: the held
    /// point lands at frame centre, everything else scales about it.
    pub fn transform(&self) -> Transform {
        let z = self.zoom.max(0.05);
        Transform::new(
            z,
            0.0,
            0.0,
            z,
            W * 0.5 - self.at.dx * z,
            H * 0.5 - self.at.dy * z,
        )
    }
}

/// Interpolate two cameras. Eased by the caller — this is the straight line.
pub fn cam_lerp(a: Cam, b: Cam, t: f32) -> Cam {
    let t = clamp01(t);
    Cam {
        zoom: a.zoom + (b.zoom - a.zoom) * t,
        at: Offset::new(
            a.at.dx + (b.at.dx - a.at.dx) * t,
            a.at.dy + (b.at.dy - a.at.dy) * t,
        ),
    }
}

/// A decaying impulse — an event the camera felt, not a loop it is in.
///
/// `since` is seconds since the hit. The shake is deterministic (it is a
/// function of `since` alone, not of a running RNG) because the film's
/// whole contract is that a frame is a function of the checkout.
pub fn cam_shake(since: f32, amplitude: f32) -> (f32, f32) {
    if !(0.0..0.55).contains(&since) {
        return (0.0, 0.0);
    }
    let decay = (1.0 - since / 0.55).powf(2.2);
    let a = amplitude * decay;
    (
        a * (since * 62.0).sin(),
        a * 0.55 * (since * 48.0 + 1.7).sin(),
    )
}

/// The film's camera plan, in one place — because a camera plan is a
/// document, and reading it should not mean opening twenty-three files.
///
/// Every move is slow, motivated and single: one idea per scene. The
/// easings vary on purpose — a push that decelerates into its subject
/// reads differently from one that is still travelling when the cut
/// comes, and the film had exactly one easing curve doing eighty percent
/// of the work.
pub fn camera(id: &str, t: f32, sec: f32) -> Cam {
    use crate::film_lib::{ease_in_out, ease_out_cubic, ease_out_expo, smoothstep};
    match id {
        // The question, asked closer than it was posed.
        "P01" => cam_lerp(Cam::zoom(1.0), Cam::zoom(1.055), ease_in_out(t)),

        // The wait closes in on the terminal — the only move in the film
        // that is meant to feel slightly oppressive.
        "A01" => cam_lerp(
            Cam::zoom(1.0),
            Cam::at(1.075, W * 0.5, H * 0.56),
            smoothstep(t),
        ),

        // The tolls become a journey. The camera tracks the signal left to
        // right through the gates instead of watching a stack from across
        // the room — the metaphor is serial, so the shot is too.
        "A02" => {
            let travel = ease_in_out(clamp01((t - 0.14) / 0.72));
            cam_lerp(
                Cam::at(1.16, 700.0, 452.0),
                Cam::at(1.03, 1180.0, 452.0),
                travel,
            )
        }

        // Held — then hit, once, when the frame budget breaks. The jank is
        // an event; the camera should have felt it.
        "A03" => {
            let base = cam_lerp(Cam::zoom(1.02), Cam::zoom(1.06), ease_out_cubic(t));
            let (dx, dy) = cam_shake(sec - 8.6, 9.0);
            base.nudged(dx, dy)
        }

        // Pull back off the desktop to take in all three devices at once.
        "A04" => cam_lerp(
            Cam::at(1.22, 850.0, 460.0),
            Cam::at(1.0, W * 0.5, H * 0.5),
            ease_out_expo(clamp01((t - 0.08) / 0.62)),
        ),

        // Start on the first tree, widen as the set completes.
        "B01" => cam_lerp(
            Cam::at(1.30, 700.0, 500.0),
            Cam::zoom(1.0),
            ease_out_expo(clamp01((t - 0.05) / 0.70)),
        ),

        // A slow drift across the constellation, never arriving.
        "B02" => {
            let d = ease_in_out(t);
            cam_lerp(
                Cam::at(1.10, 900.0, 430.0),
                Cam::at(1.02, 1010.0, 480.0),
                d,
            )
        }

        // The film's one real push: from the glyph, down to a single
        // pixel's coverage. This is the shot the scene was always
        // describing and never showed.
        "B03" => {
            let p = ease_in_out(clamp01((t - 0.18) / 0.66));
            cam_lerp(Cam::zoom(1.0), Cam::at(2.35, 905.0, 300.0), p)
        }

        // Settle back to take the three panels as one argument.
        "B04" => cam_lerp(
            Cam::at(1.18, 640.0, 520.0),
            Cam::zoom(1.0),
            ease_out_expo(clamp01((t - 0.06) / 0.60)),
        ),

        // ── Movement III — the app's pixels are its own. ────────────────
        "C01" | "C02" | "C03" | "C04" | "C05" | "C06" | "C07" | "C08" | "C09" => Cam::STILL,

        // The receipts settle into place.
        "D01" => cam_lerp(Cam::zoom(1.05), Cam::zoom(1.0), ease_out_expo(t)),

        // The pullback, finally pulling back: start tight on the studio
        // card, widen until the constellation around it is the shot. The
        // scene has been called `the_pullback` all along.
        "D02" => cam_lerp(
            Cam::at(1.85, 960.0, 560.0),
            Cam::zoom(1.0),
            ease_out_expo(clamp01((t - 0.04) / 0.74)),
        ),

        // In, as the poles meet — the distance collapsing is the camera's
        // move as much as the geometry's.
        "D03" => {
            let p = ease_in_out(clamp01((t - 0.22) / 0.50));
            cam_lerp(Cam::zoom(1.0), Cam::at(1.42, 960.0, 440.0), p)
        }

        // A last, barely-there settle onto the mark.
        "D04" => cam_lerp(Cam::zoom(1.035), Cam::zoom(1.0), ease_out_expo(clamp01(t / 0.5))),

        _ => Cam::STILL,
    }
}

// ── The cut ─────────────────────────────────────────────────────────────────
//
// There were twenty-two cuts in the film and all twenty-two were the same
// cut: the outgoing scene was killed on a frame boundary with no exit ramp
// at all — sixteen of the twenty-three scene files contain no fade-out
// logic of any kind — and the incoming scene began from an empty frame
// with its elements on `(t - 0.3) / 0.5`-style appear delays. The join is
// therefore a third to two-thirds of a second of very nearly nothing, at
// every boundary, which adds up to something like eight to twelve seconds
// of the runtime. It does not read as rhythm. It reads as buffering.
//
// Two of those cuts were worse than the rest. Mean frame luminance runs
// 27–30 across the graphics acts and 49–54 across the studio act, so the
// join into `C01` is a **+22.5 jump in a single frame** and the join out
// of `C09` is −16. In a dark room that is the audience flinching, twice,
// and it is also why the two halves of the film do not look like the same
// piece of work.
//
// The fix is a shot, not a dissolve. Each scene now enters *moving* and
// leaves *accelerating*, through the camera, with a short luminance ramp
// riding along. That matters for the brief the film set itself: a
// cross-fade reads the same at 24 fps as at 60, but a fast camera move
// only resolves cleanly at a high frame rate. If the film is going to
// render eighteen thousand frames it should put something in them that
// needs eighteen thousand frames.
//
// `ease_in_*` on the way out is the point. The film had two `ease_in_out`
// calls against fifty `ease_out_cubic`, which means nothing in five
// minutes ever accelerated, which in turn is *why* nothing ever left:
// every element decelerated into place and then waited to be cut. Things
// that leave have to speed up first.

/// The handle either side of a cut.
const CUT_IN: f32 = 0.34;
const CUT_OUT: f32 = 0.26;

/// Acceleration — the curve the film did not have.
pub fn ease_in_cubic(t: f32) -> f32 {
    let t = clamp01(t);
    t * t * t
}

/// What the master needs to render one frame: where the camera is, and how
/// much of the frame is present.
#[derive(Clone, Copy)]
pub struct Shot {
    pub cam: Cam,
    /// Global frame opacity, and the luminance ramp across a cut. 1.0 is
    /// the scene as authored.
    pub alpha: f32,
}

/// The camera a scene is entered from and left towards.
///
/// Varied by position so the cut does not become a tic — the same handle
/// on every join is its own kind of monotony. Movement boundaries get a
/// bigger move than the joins inside a movement, because they *are* a
/// bigger change, and the two studio boundaries get the largest because
/// they also carry the luminance ramp.
fn cut_handles(index: usize, base: Cam) -> (Cam, Cam) {
    // The rail already knows where the movements break; the same list.
    let movement_break = matches!(index, 1 | 5 | 9 | 18 | 20);
    let into_studio = index == 9;
    let out_of_studio = index == 18;

    let (zin, lift) = if into_studio || out_of_studio {
        (1.085, 26.0)
    } else if movement_break {
        (1.055, 18.0)
    } else {
        (1.028, 10.0)
    };

    let enter = Cam {
        zoom: base.zoom * zin,
        at: Offset::new(base.at.dx, base.at.dy + lift),
    };
    let leave = Cam {
        zoom: base.zoom * (2.0 - zin).max(0.7),
        at: Offset::new(base.at.dx, base.at.dy - lift * 0.8),
    };
    (enter, leave)
}

/// The full shot for a frame: the scene's camera plan, with the cut's
/// handles composed on either end.
pub fn shot(id: &str, index: usize, seconds: f32, t: f32, sec: f32) -> Shot {
    use crate::film_lib::ease_out_expo;

    let base = camera(id, t, sec);
    let (enter, leave) = cut_handles(index, base);
    let first = index == 0;
    let last = id == "D05";

    // Entering — decelerating into the scene's own camera.
    if !first && sec < CUT_IN {
        let e = ease_out_expo(clamp01(sec / CUT_IN));
        return Shot {
            cam: cam_lerp(enter, base, e),
            alpha: clamp01(sec / (CUT_IN * 0.62)),
        };
    }

    // Leaving — accelerating out of it. The last scene is exempt: the film
    // should end on a held frame, not on a frame that is still travelling.
    let remaining = seconds - sec;
    if !last && remaining < CUT_OUT {
        let l = ease_in_cubic(clamp01(1.0 - remaining / CUT_OUT));
        return Shot {
            cam: cam_lerp(base, leave, l),
            alpha: 1.0 - ease_in_cubic(clamp01(1.0 - remaining / (CUT_OUT * 0.72))),
        };
    }

    Shot { cam: base, alpha: 1.0 }
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
    let line = format!("{label} · {text}");

    // The pill is sized from the **tracked** width of the line it has to
    // hold, and from one place. It used to be sized from `gmono_w` — the
    // untracked estimate — which came up short by `1.5 × chars`, so the
    // line no longer fitted the box it was given, wrapped, and put "ms" on
    // a second row that fell straight out the bottom of a pill painted at
    // a hard-coded 30 px. The readout the whole film hangs on spent most
    // of its runtime visibly broken.
    const SIZE: f32 = 15.0;
    const TRACK: f32 = 1.5;
    const PAD_L: f32 = 30.0;
    const PAD_R: f32 = 16.0;
    const PILL_H: f32 = 30.0;
    let text_w = gmono_tw(SIZE, line.chars().count(), TRACK);
    let w = (PAD_L + text_w + PAD_R).ceil();
    let x = (W * 0.5 - w * 0.5).round();

    chrome(
    Stack::new()
        .push(
            Positioned::new()
                .left(x)
                .top(1008.0)
                .width(w)
                .height(PILL_H)
                .child(Opacity::new(a).child(Painting::sized(
                    Size::new(w, PILL_H),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        book.rrect(xywh(0.0, 0.0, w, PILL_H), 9.0, alpha(SURFACE_2, 0.92));
                        book.stroke_rrect(
                            xywh(0.0, 0.0, w, PILL_H),
                            9.0,
                            alpha(ACCENT, 0.28),
                            1.1,
                        );
                        book.circle(Offset::new(16.0, PILL_H * 0.5), 3.5, alpha(ACCENT, 0.7));
                    }),
                ))),
        )
        .push(
            Positioned::new()
                .left(x + PAD_L)
                .top(1008.0 + (PILL_H - SIZE * 1.34) * 0.5)
                .width(text_w + SIZE)
                .height(SIZE * 1.34)
                .child(Opacity::new(a).child(
                    Text::new(line)
                        .style(
                            geist_mono(SIZE)
                                .letter_spacing(TRACK)
                                .color(alpha(INK, 0.9)),
                        )
                        .align(TextAlign::Left),
                )),
        )
        .into(),
    )
}

/// The act chip — the movement's name.
pub fn act_chip(movement: &str, name: &str, appear: f32) -> WidgetNode {
    let a = crate::film_lib::ease_out_cubic(appear.clamp(0.0, 1.0));
    if a <= 0.01 {
        return Stack::new().into();
    }
    chrome(
    Stack::new()
        .push(
            Positioned::new()
                .left(112.0)
                .top(ACT_Y)
                .width(1200.0)
                .height(30.0)
                .child(Opacity::new(a).child(
                    Text::new(format!("{movement} — {name}"))
                        .style(
                            geist_mono(19.0)
                                .letter_spacing(6.0)
                                .color(alpha(MUTED, 0.88)),
                        )
                        .align(TextAlign::Left),
                )),
        )
        .into(),
    )
}

/// The film's progress rail — a hairline at the very bottom, one tick per
/// scene, the playhead sliding.
pub fn progress_rail(abs: f32) -> WidgetNode {
    let total = total_seconds();
    let frac = (abs / total).clamp(0.0, 1.0);
    let starts: Vec<f32> = (0..scenes().len()).map(scene_start).collect();
    chrome(
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
    .into(),
    )
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
        // The headline plate.
        //
        // Every studio scene drops its big line at y≈190, which is inside
        // the editor's code area in all nine of them — "one file, one
        // screen()", "edit a line, see the picture change", the say
        // program, the swatch row, all painted straight through lines 4
        // to 8 with nothing between them. Two runs of text at similar
        // weight over each other is not a composite, it is a collision,
        // and neither survived it.
        //
        // This is a scrim, not a fill: the file's comment header stays
        // faintly readable underneath, which keeps the honesty rule — the
        // app is still there and still drawing — while giving the film's
        // voice a surface to sit on. It is the least costly rectangle in
        // the frame to borrow, being the same eight lines of prose for
        // the whole act.
        .push(
            Positioned::new()
                .left(0.0)
                .top(BAND_H + 24.0)
                .width(W)
                .height(190.0)
                .child(Painting::sized(
                    Size::new(W, 190.0),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        book.rect(
                            xywh(0.0, 0.0, W, 190.0),
                            Gradient::vertical().with_dither().with_stops(&[
                                (0.0, alpha(BG_DEEP, 0.0)),
                                (0.22, alpha(BG_DEEP, 0.80)),
                                (0.78, alpha(BG_DEEP, 0.80)),
                                (1.0, alpha(BG_DEEP, 0.0)),
                            ]),
                        );
                    }),
                )),
        )
        .push(band_scrim(1.0))
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
    // The editor's empty lower half, not the preview pane's paragraph.
    //
    // Pinned at `W - 900` this row landed exactly on the preview's
    // description text — three chips and a paragraph rendered through each
    // other, in every studio scene — while the bottom third of the editor
    // sat empty for the whole act. The chips go where the app isn't.
    chip_row(chips, 360.0, 924.0, appear)
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
    dust_deep(book, w, h, t, seed, strength, Offset::ZERO);
}

/// The dust, in depth — the near stratum of the film's air.
///
/// Dust reads as foreground, so it *leads* the camera rather than lagging
/// it: a negative depth. Against a push this is what makes the move feel
/// like the lens travelling through something rather than the picture
/// getting bigger.
pub fn dust_deep(
    book: &mut Sketchbook,
    w: f32,
    h: f32,
    t: f32,
    seed: u64,
    strength: f32,
    pan: Offset,
) {
    let mut rng = Rng::new(seed);
    for _ in 0..64 {
        let bx = rng.f01() * w;
        let by = rng.f01() * h;
        let r = 0.8 + rng.f01() * 1.7;
        let drift = (t * 1.3 + rng.f01() * 7.0).sin() * 10.0;
        let fall = t * 12.0 * rng.f01();
        let depth = -0.18 - rng.f01() * 0.26;
        book.circle(
            Offset::new(bx + drift + pan.dx * depth, (by + fall) % h + pan.dy * depth),
            r,
            alpha(MUTED, 0.22 * strength),
        );
    }
}

// ── Depth for a flat panel ──────────────────────────────────────────────────

/// Draw `source` as a panel standing in space, turned `yaw` radians about
/// its own vertical axis and pitched `pitch` about its horizontal one.
///
/// The film is a stack of rectangles seen square-on, which is the flattest
/// a picture can be. `vieww-foundation` ships `Transform3` — with a real
/// perspective divide, and a `project_rect` that hands back the quad a
/// tilted plane occupies — and an earlier film in this lab used it. This
/// one never did.
///
/// **Piecewise affine, 2×2.** A single affine fitted to the projected
/// corners maps the panel to a *parallelogram*: its verticals stay
/// vertical, the trapezoid exists only in the clip, and since the content
/// never reaches the clip's slanted edge the panel renders as a rotated
/// rectangle — displaced, but flat. (That was this function's first cut,
/// measured on the render: side edges vertical to 0.3 px.)
///
/// So the panel is drawn as four quadrants, each with its own affine
/// fitted to its own three projected corners. Perspective maps straight
/// lines to straight lines, so every cell edge lands exactly on the
/// true projected edge, adjacent cells agree exactly along their shared
/// seam (both affines pass through the same two corners, and both map
/// the straight seam to the straight line between them), and the union
/// silhouette is the true perspective quad. Only the interior carries
/// error — a quarter of the corner discrepancy at each cell's centre,
/// ~3 px on a 300 px panel at 26° — where nothing is anchored to
/// anything. The cost is the source drawn four times, clipped to
/// disjoint quadrants.
///
/// `focal` is in logical pixels: larger is a longer lens.
#[allow(clippy::too_many_arguments)]
pub fn panel_3d(
    book: &mut Sketchbook,
    rect: Rect,
    yaw: f32,
    pitch: f32,
    focal: f32,
    alpha_mul: f32,
    source: impl Fn(&mut Sketchbook),
) {
    use vieww_foundation::Transform3;

    if yaw.abs() < 1.0e-4 && pitch.abs() < 1.0e-4 {
        source(book);
        return;
    }

    let cx = (rect.left + rect.right) * 0.5;
    let cy = (rect.top + rect.bottom) * 0.5;

    // Turn about the panel's own centre, then project.
    let t3 = Transform3::translation(-cx, -cy, 0.0)
        .then(Transform3::rotation_y(yaw))
        .then(Transform3::rotation_x(pitch))
        .then(Transform3::translation(cx, cy, 0.0))
        .then(Transform3::perspective(focal));

    // A cell that cannot be projected (behind the camera) is skipped
    // rather than failing the panel; at the angles this function is
    // called with it does not happen.
    let a_mul = alpha_mul.clamp(0.0, 1.0);
    for (x0, x1) in [(rect.left, cx), (cx, rect.right)] {
        for (y0, y1) in [(rect.top, cy), (cy, rect.bottom)] {
            let Some(quad) = t3.project_rect(Rect::new(x0, y0, x1, y1)) else {
                continue;
            };
            let (Some(tl), Some(tr), Some(bl)) = (
                t3.project(Offset::new(x0, y0), 0.0),
                t3.project(Offset::new(x1, y0), 0.0),
                t3.project(Offset::new(x0, y1), 0.0),
            ) else {
                continue;
            };
            // The affine through the cell's own three corners: the top
            // edge gives the x basis, the left edge the y basis.
            let wsp = (x1 - x0).max(1.0);
            let hsp = (y1 - y0).max(1.0);
            let ax = (tr.dx - tl.dx) / wsp;
            let ay = (tr.dy - tl.dy) / wsp;
            let bx = (bl.dx - tl.dx) / hsp;
            let by = (bl.dy - tl.dy) / hsp;
            let fit = Transform::new(
                ax,
                ay,
                bx,
                by,
                tl.dx - (ax * x0 + bx * y0),
                tl.dy - (ay * x0 + by * y0),
            );

            // The clip is the *projected* cell quad, so it is already in
            // screen space and must sit outside the transform —
            // `Sketchbook::window` puts its clip inside, which would
            // project it a second time.
            book.layer(a_mul, 0.0, Some(quad), |g| {
                // Borrow, not move: the loop draws the source once per
                // quadrant, and an `Fn` source is shareable by design.
                g.transformed(fit, |b| source(b));
            });
        }
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
