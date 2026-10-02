//! keynote_z — **THE SPARK**, the viewwstudio release film, sixth build.
//!
//! Every frame rendered by **vieww itself** — headless `FrameDriver` +
//! `NativeRenderer` at 1920×1080 · 60 fps, raw RGBA streamed into ffmpeg.
//! Nothing added in post; the closing sting is the contract: *this film
//! was rendered with vieww.*
//!
//! **The medium's own claim, upgraded from v5:** the studio scenes are
//! not a mock. Act III mounts the **actual `viewwstudio` app** — the
//! same `Shell`, the same `Studio` state, the same element runtime, the
//! same compile pipeline — on the film's own `FrameDriver`, and drives
//! it with a frame-indexed script through the real input pipeline
//! (`keynote_z/script.rs`). When the film types, the studio's editor
//! types. When the film taps, the studio's demo navigates. When the
//! film presses Render, `rustc` runs. Tier B of the plan's honesty
//! ladder, made literal.
//!
//! **The palette is the brand's**: every colour the film's own chrome
//! draws is taken verbatim from `apps/viewwsite/src/lib.rs` — the
//! product page's palette (GROUND `#0F0D0B` · INK `#F8F4F2` · ACCENT
//! `#B491FF` · ACCENT_DEEP `#7E5CE8`, the syntax ramp), and the film's
//! typography is the site's own Geist. The studio renders in its own
//! shipped theme (its chrome, its fonts) exactly as the product does.
//!
//! **The story — one spark of light becomes a product.** A dark room, a
//! blinking caret, the distance between a thought and a screen. A point
//! of light is born, and that spark never leaves the film — the
//! wordmark's bloom, the signal's pulse, the preview's first paint, the
//! tap's ripple, the phone's glow, the end card's mark — before
//! collapsing back into the caret for the loop.
//!
//! | act | movement | scenes | the job |
//! |-----|----------|--------|---------|
//! | I · THE SPARK | 起 | S01–S04 | the blank canvas · the old world's wait · the break · **the spark** — one point of light says *this is vieww* |
//! | II · THE MACHINE | 承 | S05–S10 | the descent into the engine: 36 crates · the signal apart from the tree · its own rasterizer · damage in pixels · the 60 fps cadence |
//! | III · THE STUDIO | 転 | S11–S19 | **viewwstudio, the real app**: it opens · live preview paints as you type · damage overlays · the tap · say→rust, compiled for real · the screens on device frames · the token editor · export & devices · devtools on itself |
//! | IV · THE SHIP | 結 | S20–S24 | real apps on real devices · the green ledger · the pull-back — the studio is one buffer on 36 crates · the end card audits itself · the loop |
//!
//! **The house rules, inherited verbatim:**
//! 1. No number on screen is typed by a human — every figure is emitted by
//!    the pipeline it describes, or quoted from this repository's receipts.
//! 2. No effect is added in post — visible ⇒ the product rendered it.
//! 3. The cut works muted — every beat is captioned by construction.
//! 4. Richness escalates deliberately, and every escalation is a demo.
//! 5. The spark is the continuity: scenes cut, the light persists.
//!
//! ```console
//! cargo run --release -p film_lab -- censusz     # pass 1 — the audit
//! cargo run --release -p film_lab -- masterz     # pass 2 — the MP4 + sheets
//! cargo run --release -p film_lab -- kz:spark    # one scene, 16 frames
//! ```

pub(crate) mod master;
pub(crate) mod s01_the_blank;
pub(crate) mod s02_the_wait;
pub(crate) mod s03_the_break;
pub(crate) mod s04_the_spark;
pub(crate) mod s05_the_descent;
pub(crate) mod s06_the_crates;
pub(crate) mod s07_the_signal;
pub(crate) mod s08_the_rasterizer;
pub(crate) mod s09_the_damage;
pub(crate) mod s10_the_cadence;
pub(crate) mod s11_studio_opens;
pub(crate) mod s12_first_paint;
pub(crate) mod s13_live_compose;
pub(crate) mod s14_the_tap;
pub(crate) mod s15_say_to_rust;
pub(crate) mod s16_the_screens;
pub(crate) mod s17_the_tokens;
pub(crate) mod s18_cross_build;
pub(crate) mod s19_devtools_mirror;
pub(crate) mod s20_the_apps;
pub(crate) mod s21_the_ledger;
pub(crate) mod s22_the_pullback;
pub(crate) mod s23_the_endcard;
pub(crate) mod s24_the_loop;
pub(crate) mod script;

pub(crate) use vieww_foundation::{
    Color, Gradient, Offset, Rect, Size, Sketchbook, TextAlign, TextStyle,
};
pub(crate) use vieww_widget::prelude::*;
pub(crate) use vieww_widget::{Opacity, PaintWith, Painting};

pub(crate) use crate::film_lib::{ease_out_cubic, Rng};

/// The studio's Teal accent ramp (the token editor's other face) —
/// quoted from `apps/viewwstudio/src/theme.rs`'s TEAL.
pub(crate) const TEAL_SWATCH: Color = Color::rgb(0x0E, 0x81, 0x74);
pub(crate) const TEAL_SWATCH_2: Color = Color::rgb(0x2F, 0xBF, 0xAE);

// ── The master format ───────────────────────────────────────────────────────

/// Master width, px — 1080p.
pub(crate) const W: f32 = 1920.0;
/// Master height, px.
pub(crate) const H: f32 = 1080.0;
/// The master canvas.
pub(crate) const CANVAS: Size = Size::new(W, H);
/// Master cadence — the product's own.
pub(crate) const FPS: f32 = 60.0;

// ── The palette — the viewwsite's own, verbatim ─────────────────────────────
//
// Every constant below is quoted from `apps/viewwsite/src/lib.rs` (the
// product page — the brand's source of truth) or the studio's brand ramp
// (`apps/viewwstudio/src/theme.rs`: PURPLE, which is the site's ACCENT pair
// exactly). The film's grammar keeps the house helper *names* so the
// scenes read the same; the values are the brand's.

/// The page's ground — warm near-black. The film's BG.
pub(crate) const GROUND: Color = Color::rgb(0x0F, 0x0D, 0x0B);
/// Deeper than the ground — the floor of the frame.
pub(crate) const BG_DEEP: Color = Color::rgb(0x0A, 0x09, 0x08);
/// The site's raised panel.
pub(crate) const SURFACE: Color = Color::rgb(0x1B, 0x17, 0x15);
/// The site's second raise.
pub(crate) const SURFACE_2: Color = Color::rgb(0x2B, 0x25, 0x21);
/// The site's hairline.
pub(crate) const LINE: Color = Color::rgb(0x2A, 0x28, 0x26);
/// The site's ink — warm off-white.
pub(crate) const INK: Color = Color::rgb(0xF8, 0xF4, 0xF2);
/// The site's second ink.
pub(crate) const MUTED: Color = Color::rgb(0xA6, 0x9C, 0x95);
/// The site's third ink.
pub(crate) const FAINT: Color = Color::rgb(0x78, 0x71, 0x6C);

/// The brand accent (the site's ACCENT, the studio's dark_far).
pub(crate) const ACCENT: Color = Color::rgb(0xB4, 0x91, 0xFF);
/// The brand accent's deep end (the site's ACCENT_DEEP, the studio's
/// dark_near).
pub(crate) const ACCENT_DEEP: Color = Color::rgb(0x7E, 0x5C, 0xE8);
/// The site's violet wash.
pub(crate) const WASH: Color = Color::rgb(0x22, 0x1C, 0x33);

/// The site's syntax ramp — the film's engine/technical register.
pub(crate) const SYN_KEYWORD: Color = Color::rgb(0xEF, 0xA3, 0xFF);
pub(crate) const SYN_TYPE: Color = Color::rgb(0x48, 0xD7, 0xFE);
pub(crate) const SYN_STRING: Color = Color::rgb(0x59, 0xD3, 0x8C);
pub(crate) const SYN_NUMBER: Color = Color::rgb(0xFF, 0x8F, 0x9A);
pub(crate) const SYN_COMMENT: Color = Color::rgb(0x85, 0x7F, 0x7A);
pub(crate) const SYN_MACRO: Color = Color::rgb(0xFE, 0xB2, 0x63);
pub(crate) const SYN_FUNCTION: Color = Color::rgb(0xFF, 0xBB, 0x6D);
pub(crate) const SYN_PUNCT: Color = Color::rgb(0xA3, 0x9D, 0x98);

// The house grammar's aliases — same names the helpers grew up with,
// the brand's values.
pub(crate) const VIOLET: Color = ACCENT;
pub(crate) const VIOLET_SOFT: Color = Color::rgb(0xC7, 0xAF, 0xFF);
pub(crate) const VIOLET_DEEP: Color = ACCENT_DEEP;
pub(crate) const CYAN: Color = SYN_TYPE;
pub(crate) const CYAN_SOFT: Color = Color::rgb(0x8F, 0xE7, 0xFE);
pub(crate) const MINT: Color = SYN_STRING;
pub(crate) const AMBER: Color = SYN_FUNCTION;
pub(crate) const RED: Color = SYN_NUMBER;
pub(crate) const MAGENTA: Color = SYN_KEYWORD;

/// The damage/danger accent (act I's register).
pub(crate) const C_DAMAGE: Color = RED;
/// Act I's terminal green — the *old world's* promise color (not the
/// brand's: the old world gets its own palette, and it is not ours).
pub(crate) const TERM_GREEN: Color = Color::rgb(63, 185, 80);
/// Act II's engine register.
pub(crate) const ENGINE: Color = CYAN;
/// The receipts' green (act IV's ledger).
pub(crate) const LEDGER: Color = MINT;
/// The spark's own color — the light that never leaves.
pub(crate) const SPARK_C: Color = ACCENT;

// ── The scene registry ──────────────────────────────────────────────────────

/// How a scene renders.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A pure widget tree — a function of the frame's ctx (the film's
    /// own graphics).
    Pure,
    /// The **actual studio**: the harness mounts `Shell { studio }` on
    /// the film's driver and applies the session script up to this
    /// frame; the scene's `build` returns only the overlay chrome
    /// (captions, chips, the film's own voice) that rides on top.
    Studio,
}

/// What a scene may read: its own clock, the film clock, the ladder, and
/// the census pass's measured numbers (zeros in pass 1 — a scene must
/// render sanely without them; only the numbers' *display* differs).
pub(crate) struct Ctx<'a> {
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
pub(crate) struct SceneDef {
    /// "S01" — the scene id.
    pub id: &'static str,
    /// "the_blank" — the scene name.
    pub name: &'static str,
    /// Film seconds this scene spans. Act sums: I 9+10+7+11 = 37 ·
    /// II 8+10+9+11+8+6 = 52 · III 8+12+11+8+12+10+8+11+8 = 88 ·
    /// IV 10+9+8+11+6 = 44 → 221 s = 3:41.
    pub seconds: f32,
    /// How the harness renders this scene.
    pub kind: Kind,
    /// The scene — a pure function of its ctx (the overlay tree for
    /// studio scenes).
    pub build: fn(&Ctx) -> WidgetNode,
}

impl SceneDef {
    /// Frames this scene emits at master cadence — derived, never typed.
    pub(crate) fn frames(&self) -> usize {
        (self.seconds * FPS).round() as usize
    }
}

/// The twenty-four scenes, in cut order.
pub(crate) fn scenes() -> Vec<SceneDef> {
    vec![
        // ── Act I · THE SPARK ────────────────────────────────────────────
        SceneDef {
            id: "S01",
            name: "the_blank",
            seconds: 9.0,
            kind: Kind::Pure,
            build: s01_the_blank::build,
        },
        SceneDef {
            id: "S02",
            name: "the_wait",
            seconds: 10.0,
            kind: Kind::Pure,
            build: s02_the_wait::build,
        },
        SceneDef {
            id: "S03",
            name: "the_break",
            seconds: 7.0,
            kind: Kind::Pure,
            build: s03_the_break::build,
        },
        SceneDef {
            id: "S04",
            name: "the_spark",
            seconds: 11.0,
            kind: Kind::Pure,
            build: s04_the_spark::build,
        },
        // ── Act II · THE MACHINE ─────────────────────────────────────────
        SceneDef {
            id: "S05",
            name: "the_descent",
            seconds: 8.0,
            kind: Kind::Pure,
            build: s05_the_descent::build,
        },
        SceneDef {
            id: "S06",
            name: "the_crates",
            seconds: 10.0,
            kind: Kind::Pure,
            build: s06_the_crates::build,
        },
        SceneDef {
            id: "S07",
            name: "the_signal",
            seconds: 9.0,
            kind: Kind::Pure,
            build: s07_the_signal::build,
        },
        SceneDef {
            id: "S08",
            name: "the_rasterizer",
            seconds: 11.0,
            kind: Kind::Pure,
            build: s08_the_rasterizer::build,
        },
        SceneDef {
            id: "S09",
            name: "the_damage",
            seconds: 8.0,
            kind: Kind::Pure,
            build: s09_the_damage::build,
        },
        SceneDef {
            id: "S10",
            name: "the_cadence",
            seconds: 6.0,
            kind: Kind::Pure,
            build: s10_the_cadence::build,
        },
        // ── Act III · THE STUDIO — the real app, driven by the script ────
        SceneDef {
            id: "S11",
            name: "studio_opens",
            seconds: 8.0,
            kind: Kind::Studio,
            build: s11_studio_opens::build,
        },
        SceneDef {
            id: "S12",
            name: "first_paint",
            seconds: 12.0,
            kind: Kind::Studio,
            build: s12_first_paint::build,
        },
        SceneDef {
            id: "S13",
            name: "live_compose",
            seconds: 11.0,
            kind: Kind::Studio,
            build: s13_live_compose::build,
        },
        SceneDef {
            id: "S14",
            name: "the_tap",
            seconds: 8.0,
            kind: Kind::Studio,
            build: s14_the_tap::build,
        },
        SceneDef {
            id: "S15",
            name: "say_to_rust",
            seconds: 12.0,
            kind: Kind::Studio,
            build: s15_say_to_rust::build,
        },
        SceneDef {
            id: "S16",
            name: "the_screens",
            seconds: 10.0,
            kind: Kind::Studio,
            build: s16_the_screens::build,
        },
        SceneDef {
            id: "S17",
            name: "the_tokens",
            seconds: 8.0,
            kind: Kind::Studio,
            build: s17_the_tokens::build,
        },
        SceneDef {
            id: "S18",
            name: "cross_build",
            seconds: 11.0,
            kind: Kind::Studio,
            build: s18_cross_build::build,
        },
        SceneDef {
            id: "S19",
            name: "devtools_mirror",
            seconds: 8.0,
            kind: Kind::Studio,
            build: s19_devtools_mirror::build,
        },
        // ── Act IV · THE SHIP ────────────────────────────────────────────
        SceneDef {
            id: "S20",
            name: "the_apps",
            seconds: 10.0,
            kind: Kind::Pure,
            build: s20_the_apps::build,
        },
        SceneDef {
            id: "S21",
            name: "the_ledger",
            seconds: 9.0,
            kind: Kind::Pure,
            build: s21_the_ledger::build,
        },
        SceneDef {
            id: "S22",
            name: "the_pullback",
            seconds: 8.0,
            kind: Kind::Pure,
            build: s22_the_pullback::build,
        },
        SceneDef {
            id: "S23",
            name: "the_endcard",
            seconds: 11.0,
            kind: Kind::Pure,
            build: s23_the_endcard::build,
        },
        SceneDef {
            id: "S24",
            name: "the_loop",
            seconds: 6.0,
            kind: Kind::Pure,
            build: s24_the_loop::build,
        },
    ]
}

/// Absolute film seconds where scene `index` starts.
pub(crate) fn scene_start(index: usize) -> f32 {
    scenes().iter().take(index).map(|s| s.seconds).sum()
}

/// The film's total frame count — the sum of the scene table, emitted.
pub(crate) fn total_frames() -> usize {
    scenes().iter().map(|s| s.frames()).sum()
}

/// The film's total seconds — derived, never typed.
pub(crate) fn total_seconds() -> f32 {
    scenes().iter().map(|s| s.seconds).sum()
}

// ── The witness ladder — one number per human touch, never reset ───────────

/// The eight touches, in absolute film seconds — **exactly the script's
/// action times** (`script.rs` is the source of truth; the fractions
/// below are derived from them so the ladder and the session cannot
/// drift): the live preview opens (S12, `AcceptLive`) · the first live
/// keystroke (S12) · the compose edit (S13) · the demo row's tap fires
/// (S14, `PointerUp`) · Render fires (S15) · *Add one* is tapped (S15) ·
/// the build view opens (S18) · the devices land (S18). The counter is
/// the witness; the camera changes, it doesn't.
pub(crate) fn taps() -> Vec<f32> {
    let at = |i: usize, frac: f32| scene_start(i) + scenes()[i].seconds * frac;
    vec![
        at(11, 6.20 / 12.0), // 1 · AcceptLive at 103.2
        at(11, 9.40 / 12.0), // 2 · the first live keystroke at 106.4
        at(12, 5.00 / 11.0), // 3 · the compose edit at 114.0
        at(13, 4.50 / 8.0),  // 4 · the row tap fires at 124.5
        at(14, 6.20 / 12.0), // 5 · Render fires at 134.2
        at(14, 8.62 / 12.0), // 6 · Add one at 136.62
        at(17, 3.30 / 11.0), // 7 · the build view at 161.3
        at(17, 8.80 / 11.0), // 8 · the devices land at 166.8
    ]
}

/// The witness counter at absolute time `abs`.
pub(crate) fn ladder_at(abs: f32) -> u32 {
    taps().iter().filter(|&&tap| abs >= tap).count() as u32
}

/// A short pulse envelope right after the most recent tap — the bloom the
/// counter wears on every touch.
pub(crate) fn tap_pulse(abs: f32) -> f32 {
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
pub(crate) fn session_clock(abs: f32) -> String {
    let shown = abs.min(total_seconds());
    format!("{:02}:{:02}", (shown / 60.0) as u32, (shown % 60.0) as u32)
}

// ── The census receipts — measured in pass 1, printed in pass 2 ────────────

/// The film's own audit, plus the live probes. Every field is measured or
/// derived; none is typed by a human.
#[derive(Default, Clone)]
pub(crate) struct Probe {
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
    /// S12's N — median of the sampled edit→visible latencies, seconds:
    /// the measured cost of one `studio.edit` → next frame's raster at
    /// master resolution, this bench, through the real studio.
    pub alive_seconds: f32,
    /// The median sampled build+raster frame time at 1080p.
    pub frame_ms: f32,
    /// The bench identity — per-bench determinism names its bench.
    pub bench: String,
}

impl Probe {
    /// Parse the house-format `key=value` receipt the census wrote.
    pub(crate) fn load(path: &std::path::Path) -> Option<Probe> {
        let body = std::fs::read_to_string(path).ok()?;
        let mut p = Probe::default();
        for line in body.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
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
    /// No system font scan runs (the film's store is embedded + Geist,
    /// both in-tree), so glyph determinism is a property of the checkout
    /// — said out loud rather than implied.
    pub(crate) fn bench_identity(fonts: u32) -> String {
        format!(
            "{} · {} · rust {} · {} embedded faces",
            std::env::consts::OS,
            std::env::consts::ARCH,
            "1.98.1", // the pinned toolchain this checkout builds with
            fonts,
        )
    }
}

// ── The 36 crates — the architecture scene's own manifest, quoted ──────────

/// The workspace's 36 crates, verbatim from `vieww_base/Cargo.toml`
/// (the examples and apps excluded — they are guests, not the engine).
/// S06 counts this list at runtime; the count on screen is the list's
/// own length.
pub(crate) const CRATES: [&str; 36] = [
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

// ── The brand mark — the studio's own glyph, lifted so the film's ───────────
// graphics and the product agree on the logo.

/// The viewwstudio mark — a window with a viewport and a beam of light
/// crossing it: *view* + *light*, the product in one glyph. Drawn as
/// strokes so it inherits every color the film puts it in. (Verbatim
/// from `apps/viewwstudio/src/theme.rs`'s chrome — the shipped mark.)
pub(crate) fn draw_mark(book: &mut Sketchbook, cx: f32, cy: f32, s: f32, color: Color, a: f32) {
    let c = alpha(color, a);
    // The window.
    book.stroke_rrect(
        xywh(cx - s * 0.5, cy - s * 0.5, s, s),
        s * 0.22,
        c,
        s * 0.14,
    );
    // The viewport — the view.
    book.stroke_rrect(
        xywh(cx - s * 0.26, cy - s * 0.26, s * 0.52, s * 0.52),
        s * 0.10,
        alpha(color, a * 0.65),
        s * 0.10,
    );
    // The beam — light through the window, corner to corner.
    let mut beam = vieww_foundation::Path::new();
    beam.move_to(Offset::new(cx + s * 0.62, cy - s * 0.62));
    beam.line_to(Offset::new(cx - s * 0.10, cy - s * 0.10));
    book.stroke_styled(
        beam,
        alpha(color, a * 0.95),
        s * 0.13,
        vieww_foundation::StrokeStyle::rounded(),
    );
    book.circle(
        Offset::new(cx + s * 0.62, cy - s * 0.62),
        s * 0.11,
        alpha(color, a),
    );
}

// ── Color arithmetic (the film_lib spelling, local so the palette is ours) ─

pub(crate) fn alpha(c: Color, a: f32) -> Color {
    Color::rgba(c.r, c.g, c.b, (a * 255.0).clamp(0.0, 255.0) as u8)
}

pub(crate) fn mix(a: Color, b: Color, t: f32) -> Color {
    a.lerp(b, t.clamp(0.0, 1.0))
}

/// A rect from origin + size — the (x, y, w, h) habit.
pub(crate) fn xywh(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x, y, x + w.max(0.0), y + h.max(0.0))
}

/// Lighten toward white (a "tint").
pub(crate) fn tint(c: Color, t: f32) -> Color {
    mix(c, Color::WHITE, t)
}

pub(crate) fn clamp01(t: f32) -> f32 {
    t.clamp(0.0, 1.0)
}

pub(crate) fn spring_out(t: f32, omega: f32, zeta: f32) -> f32 {
    let t = clamp01(t);
    let decay = (-zeta * omega * t).exp();
    1.0 - decay * ((1.0 - zeta * zeta).sqrt() * omega * t).cos()
}

// ── The film's fonts — the site's Geist, behind the embedded faces ──────────

/// The site's Geist faces, included from the site's own assets — the
/// brand's typography, byte-for-byte what the product page ships.
const GEIST: &[u8] = include_bytes!("../../../../apps/viewwsite/assets/fonts/vw-Geist-Regular.ttf");
const GEIST_MEDIUM: &[u8] =
    include_bytes!("../../../../apps/viewwsite/assets/fonts/vw-Geist-Medium.ttf");
const GEIST_BOLD: &[u8] =
    include_bytes!("../../../../apps/viewwsite/assets/fonts/vw-Geist-Bold.ttf");
const GEIST_MONO: &[u8] =
    include_bytes!("../../../../apps/viewwsite/assets/fonts/vw-GeistMono-Regular.ttf");
const GEIST_MONO_MEDIUM: &[u8] =
    include_bytes!("../../../../apps/viewwsite/assets/fonts/vw-GeistMono-Medium.ttf");

/// The film's font store: the **embedded faces only** for the generic
/// families — which is exactly the store the real studio binary runs —
/// with the site's Geist registered behind them for the film's *own*
/// voice (the wordmark, the end card, the headings). The studio's chrome
/// and previews therefore render precisely as the shipped product does;
/// the film's graphics render in the brand's type. No system scan: the
/// store is a property of the checkout, and glyph determinism with it.
pub(crate) fn fonts() -> vieww_text::FontStore {
    let mut store = vieww_text::FontStore::embedded_only();
    store.load_font_data(GEIST.to_vec());
    store.load_font_data(GEIST_MEDIUM.to_vec());
    store.load_font_data(GEIST_BOLD.to_vec());
    store.load_font_data(GEIST_MONO.to_vec());
    store.load_font_data(GEIST_MONO_MEDIUM.to_vec());
    store
}

/// A text style in the brand's Geist (the film's display voice).
pub(crate) fn geist(size: f32) -> TextStyle {
    TextStyle::new(size).family(vieww_foundation::FontFamily::Named("Geist"))
}

/// A text style in the brand's Geist Mono (the film's instrument voice).
pub(crate) fn geist_mono(size: f32) -> TextStyle {
    TextStyle::new(size).family(vieww_foundation::FontFamily::Named("Geist Mono"))
}

// ── Shared composition helpers — the film's house look ─────────────────────

/// A soft radial glow blob, drawn as its own blurred layer.
pub(crate) fn glow(book: &mut Sketchbook, x: f32, y: f32, r: f32, color: Color, a: f32) {
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
pub(crate) fn stars(book: &mut Sketchbook, w: f32, h: f32, seed: u64, n: usize, t: f32, base: f32) {
    let mut rng = Rng::new(seed);
    for _ in 0..n {
        let x = rng.f01() * w;
        let y = rng.f01() * h;
        let r = 0.4 + rng.f01() * 0.9;
        let tw = 0.5 + 0.5 * (t * 3.2 + rng.f01() * 9.0).sin();
        book.circle(
            Offset::new(x, y),
            r,
            alpha(Color::WHITE, base * (0.4 + 0.6 * tw)),
        );
    }
}

/// A parallax star field — three depth bands that shear against a
/// virtual camera (the descent's and the pull-back's sky).
pub(crate) fn stars_parallax(
    book: &mut Sketchbook,
    w: f32,
    h: f32,
    seed: u64,
    n: usize,
    t: f32,
    base: f32,
    cam_x: f32,
    cam_y: f32,
) {
    let mut rng = Rng::new(seed);
    for _ in 0..n {
        let x = rng.f01() * w;
        let y = rng.f01() * h;
        let depth = 1.0 + rng.f01() * 4.0; // 1 = near, 5 = far
        let r = (1.3 - depth * 0.18).max(0.35) + rng.f01() * 0.6;
        let tw = 0.5 + 0.5 * (t * 3.0 + rng.f01() * 9.0).sin();
        let px = (x - cam_x / depth + w).rem_euclid(w);
        let py = (y - cam_y / depth + h).rem_euclid(h);
        let a = base * (0.35 + 0.65 * tw) / depth.sqrt();
        book.circle(Offset::new(px, py), r, alpha(Color::WHITE, a));
    }
}

/// The corner vignette.
pub(crate) fn vignette(book: &mut Sketchbook, w: f32, h: f32, strength: f32) {
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
pub(crate) fn ground(book: &mut Sketchbook, w: f32, h: f32) {
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
pub(crate) fn backdrop(t: f32, seed: u64, star_n: usize) -> WidgetNode {
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
pub(crate) fn mono(text: impl Into<String>, size: f32, color: Color) -> WidgetNode {
    Text::new(text)
        .style(TextStyle::new(size).monospace().color(color))
        .into()
}

/// A mono text node with letter tracking.
pub(crate) fn mono_tracked(
    text: impl Into<String>,
    size: f32,
    color: Color,
    tracking: f32,
) -> WidgetNode {
    Text::new(text)
        .style(
            TextStyle::new(size)
                .monospace()
                .letter_spacing(tracking)
                .color(color),
        )
        .into()
}

/// The film's caption — bottom-left, mono, tracked, with an accent tick.
/// `appear` in `[0, 1]` fades it in with a slight rise; every beat is
/// captioned by construction (the cut works muted).
pub(crate) fn caption(text: &str, y: f32, appear: f32) -> WidgetNode {
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
                .child(
                    Opacity::new(a).child(
                        Text::new(text)
                            .style(geist_mono(22.0).letter_spacing(1.8).color(alpha(INK, 0.92)))
                            .align(TextAlign::Left),
                    ),
                ),
        )
        .into()
}

/// A centered caption — key info stays center-frame.
pub(crate) fn caption_center(text: &str, y: f32, appear: f32) -> WidgetNode {
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
                .child(
                    Opacity::new(a).child(
                        Text::new(text)
                            .style(geist_mono(22.0).letter_spacing(1.8).color(alpha(INK, 0.92)))
                            .align(TextAlign::Center),
                    ),
                ),
        )
        .into()
}

/// A small chip — rounded label with a soft border, the receipts' container.
pub(crate) fn chip(text: impl Into<String>, size: f32, fg: Color) -> WidgetNode {
    Container::new()
        .color(alpha(SURFACE, 0.88))
        .radius(7.0)
        .border(vieww_foundation::Border::new(alpha(fg, 0.22), 1.0))
        .padding(vieww_foundation::EdgeInsets::symmetric(7.0, 11.0))
        .child(Text::new(text).style(geist_mono(size).letter_spacing(1.1).color(fg)))
        .into()
}

/// Comma-grouped integer — the receipts' spelling (10,800, not 10800).
pub(crate) fn group_commas(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    let bytes = s.as_bytes();
    for (i, c) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*c as char);
    }
    out
}

/// DejaVu Sans Mono's advance in ems — layout arithmetic for the mono
/// grid (the film's mono voice and the studio's editor metrics agree,
/// which the studio-adjacent scenes rely on).
pub(crate) const MONO_ADV: f32 = 0.60205;

/// A monospace run's pixel width.
pub(crate) fn mono_w(size: f32, chars: usize) -> f32 {
    size * MONO_ADV * chars as f32
}

/// Geist Mono's advance in ems — measured from the face itself at load
/// (the receipt is printed by the harness; this constant is the fallback
/// before the first measure).
pub(crate) const GEIST_MONO_ADV: f32 = 0.6035;

/// A Geist Mono run's pixel width.
pub(crate) fn gmono_w(size: f32, chars: usize) -> f32 {
    size * GEIST_MONO_ADV * chars as f32
}

/// Held 24-in-60 progress for a sub-window — the old world's cadence.
pub(crate) fn held_t(sec: f32, seconds: f32) -> f32 {
    crate::film_lib::held_24_in_60(sec) / seconds
}

/// The typing cadence — bursts and pauses, a keystroke rhythm.
pub(crate) fn ease_out_type(t: f32) -> f32 {
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
pub(crate) fn circle_path(cx: f32, cy: f32, r: f32, segs: usize) -> vieww_foundation::Path {
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
pub(crate) fn grain(book: &mut Sketchbook, w: f32, h: f32, frame_i: u64, strength: f32) {
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
            if bright {
                alpha(Color::WHITE, a)
            } else {
                alpha(Color::BLACK, a * 1.6)
            },
        );
    }
}

/// Slow dust motes — the wait's air.
pub(crate) fn dust(book: &mut Sketchbook, w: f32, h: f32, t: f32, seed: u64, strength: f32) {
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

// ── The act chip — the movement's name (I–IV · 起承転結) ────────────────────

pub(crate) fn act_chip(act: &str, name: &str, appear: f32) -> WidgetNode {
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
                .child(
                    Opacity::new(a).child(
                        Text::new(format!("ACT {act} — {name}"))
                            .style(
                                geist_mono(15.0)
                                    .letter_spacing(4.5)
                                    .color(alpha(MUTED, 0.85)),
                            )
                            .align(TextAlign::Left),
                    ),
                ),
        )
        .into()
}

/// The film's progress rail — a hairline at the very bottom, one tick per
/// scene, the playhead sliding: the session clock made visible.
pub(crate) fn progress_rail(abs: f32) -> WidgetNode {
    let total = total_seconds();
    let frac = (abs / total).clamp(0.0, 1.0);
    let starts: Vec<f32> = (0..scenes().len()).map(scene_start).collect();
    Painting::sized(
        Size::new(W, 26.0),
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            let x0 = 180.0;
            let x1 = W - 180.0;
            let y = 18.0;
            book.line(
                Offset::new(x0, y),
                Offset::new(x1, y),
                alpha(Color::WHITE, 0.07),
                1.0,
            );
            // One tick per scene boundary — derived, never typed.
            for (i, st) in starts.iter().enumerate() {
                let x = x0 + (x1 - x0) * (st / total);
                let major = i == 0 || i == 4 || i == 10 || i == 19; // the four movements
                book.line(
                    Offset::new(x, y - if major { 6.0 } else { 4.0 }),
                    Offset::new(x, y + if major { 6.0 } else { 4.0 }),
                    alpha(Color::WHITE, if major { 0.22 } else { 0.13 }),
                    1.0,
                );
            }
            // The playhead.
            let px = x0 + (x1 - x0) * frac;
            book.circle(Offset::new(px, y), 3.0, alpha(VIOLET_SOFT, 0.9));
            book.line(
                Offset::new(px, y),
                Offset::new(px, y + 10.0),
                alpha(VIOLET_SOFT, 0.55),
                1.2,
            );
        }),
    )
    .into()
}

/// A count-up value — integer part eased, so digits roll to their rest.
pub(crate) fn count_up(target: u64, progress: f32) -> u64 {
    let e = ease_out_cubic(progress.clamp(0.0, 1.0));
    ((target as f32 * e).round()) as u64
}

// ── The studio-scene chrome — the film's voice over the real product ───────

/// The witness chip — top-right: the counter of human touches, blooming
/// on every tap. This is the film's own instrument (the session spine),
/// riding over the studio like a screencast's annotation layer.
pub(crate) fn witness_chip(ladder: u32, pulse: f32) -> WidgetNode {
    let bloom = 1.0 + pulse * 0.5;
    Stack::new()
        .push(
            Positioned::new()
                .left(W - 268.0)
                .top(976.0)
                .width(240.0)
                .height(38.0)
                .child(Painting::sized(
                    Size::new(240.0, 38.0),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        // The halo — the touch's bloom.
                        if pulse > 0.02 {
                            glow(book, 120.0, 19.0, 90.0, ACCENT, pulse * 0.30);
                        }
                        let _ = bloom;
                        // The body.
                        book.rrect(xywh(0.0, 2.0, 240.0, 34.0), 9.0, alpha(SURFACE_2, 0.92));
                        book.stroke_rrect(
                            xywh(0.0, 2.0, 240.0, 34.0),
                            9.0,
                            alpha(ACCENT, 0.30),
                            1.1,
                        );
                        // The witness dot.
                        book.circle(
                            Offset::new(22.0, 19.0),
                            5.0,
                            alpha(ACCENT, 0.55 + pulse * 0.45),
                        );
                        book.ring(
                            Offset::new(22.0, 19.0),
                            9.0 + pulse * 5.0,
                            1.2,
                            alpha(ACCENT, 0.5 * pulse),
                        );
                    }),
                )),
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

/// The session chip — the film's clock, under the witness: mm:ss of the
/// session, which is the film.
pub(crate) fn session_chip(abs: f32, a: f32) -> WidgetNode {
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
                .child(
                    Opacity::new(a).child(
                        Text::new(format!("session · {}", session_clock(abs)))
                            .style(
                                geist_mono(13.0)
                                    .letter_spacing(1.6)
                                    .color(alpha(MUTED, 0.9)),
                            )
                            .align(TextAlign::Left),
                    ),
                ),
        )
        .into()
}

// ── THE SPARK — the film's signature light, and its supporting grammar ──────
//
// One point of light is the film's protagonist: born in S01's dark, it
// becomes the caret, the wordmark's bloom, the signal's pulse, the first
// paint, the tap's ripple, the device glow, the end card's mark — and
// collapses back into the caret for the loop. Everything below exists to
// keep that light alive, moving, and never twice the same shape.

/// **The spark** — the film's through-line of light. A hot core, a soft
/// halo, a breathing ring, and seven orbiting motes; `phase` is the light's
/// life (feed it film seconds and it never repeats a pose).
pub(crate) fn spark(
    book: &mut Sketchbook,
    cx: f32,
    cy: f32,
    r: f32,
    phase: f32,
    a: f32,
    color: Color,
) {
    let a = a.clamp(0.0, 1.0);
    if a <= 0.004 {
        return;
    }
    // The halo — one soft blurred layer, breathing with the phase.
    let breathe = 1.0 + 0.10 * (phase * 1.7).sin();
    book.layer(1.0, (r * 0.22).max(2.0), None, |g| {
        g.circle(
            Offset::new(cx, cy),
            r * 1.35 * breathe,
            Gradient::radial_fill().with_dither().with_stops(&[
                (0.0, alpha(color, a * 0.65)),
                (0.4, alpha(color, a * 0.28)),
                (1.0, alpha(color, 0.0)),
            ]),
        );
    });
    // The ring — a thin circle at the halo's waist, opacity breathing
    // counter to the halo so the light feels alive rather than pulsed.
    let ring_a = a * (0.30 + 0.22 * (phase * 1.7 + std::f32::consts::PI).sin());
    book.ring(
        Offset::new(cx, cy),
        r * 0.72,
        1.3,
        alpha(tint(color, 0.45), ring_a.max(0.0)),
    );
    // The core — white-hot, slightly larger than a point.
    let core_r = (r * 0.11).max(1.6);
    book.circle(Offset::new(cx, cy), core_r, alpha(tint(color, 0.82), a));
    book.circle(Offset::new(cx, cy), core_r * 0.55, alpha(Color::WHITE, a));
    // The motes — seven particles on eccentric orbits, each its own speed
    // and phase; the deterministic RNG seeds their personalities so the
    // swarm around the light re-renders to the byte.
    let mut rng = Rng::new(0x5B12 ^ ((r as u64) << 10));
    for i in 0..7 {
        let off = i as f32 / 7.0 * std::f32::consts::TAU;
        let speed = 0.5 + rng.f01() * 0.8;
        let ecc = 0.55 + rng.f01() * 0.5;
        let rr = r * (0.8 + rng.f01() * 0.6);
        let ang = off + phase * speed;
        let mx = cx + ang.cos() * rr;
        let my = cy + ang.sin() * rr * ecc;
        let mr = 0.9 + rng.f01() * 1.5;
        let ma = a * (0.25 + 0.55 * (phase * 2.0 + i as f32).sin().abs());
        book.circle(Offset::new(mx, my), mr, alpha(color, ma.max(0.0)));
    }
}

/// **God rays** — soft triangular shafts rotating slowly around a center,
/// the spark's arrival lighting the room. `n` rays, base angle drifting
/// with `t`, each breathing on its own harmonic.
pub(crate) fn light_rays(
    book: &mut Sketchbook,
    cx: f32,
    cy: f32,
    r0: f32,
    r1: f32,
    base: f32,
    t: f32,
    a: f32,
    color: Color,
) {
    let n = 9;
    book.layer(1.0, 7.0, None, |g| {
        for i in 0..n {
            let ang = base
                + i as f32 / n as f32 * std::f32::consts::TAU
                + (t * 0.30 + i as f32 * 1.7).sin() * 0.11;
            let half = 0.10 + 0.05 * (t * 0.4 + i as f32).sin().abs();
            let fade = a
                * (0.35
                    + 0.65
                        * (i as f32 / n as f32 * std::f32::consts::TAU + t * 0.5)
                            .cos()
                            .abs());
            let tip0 = Offset::new(cx + (ang - half).cos() * r1, cy + (ang - half).sin() * r1);
            let tip1 = Offset::new(cx + (ang + half).cos() * r1, cy + (ang + half).sin() * r1);
            let root = Offset::new(cx + ang.cos() * r0, cy + ang.sin() * r0);
            let mut p = vieww_foundation::Path::new();
            p.move_to(root);
            p.line_to(tip0);
            p.line_to(tip1);
            p.close();
            let grad = Gradient::linear(root, Offset::new(cx, cy))
                .with_dither()
                .with_stops(&[
                    (0.0, alpha(color, fade * 0.55)),
                    (0.85, alpha(color, fade * 0.10)),
                    (1.0, alpha(color, 0.0)),
                ]);
            g.fill(p, grad);
        }
    });
}

/// **Aurora curtains** — two or three slow luminous ribbons across the
/// frame, heavily blurred, the calm register behind the film's big
/// moments. The ribbons drift and never repeat.
pub(crate) fn aurora(book: &mut Sketchbook, w: f32, h: f32, t: f32, seed: u64, a: f32) {
    let bands: [(Color, f32, f32, f32, f32); 3] = [
        // (color, y0, amplitude, speed, thickness)
        (ACCENT, 0.30, 54.0, 0.16, 130.0),
        (SYN_TYPE, 0.42, 70.0, 0.11, 170.0),
        (SYN_KEYWORD, 0.24, 44.0, 0.21, 100.0),
    ];
    let mut rng = Rng::new(seed);
    let mut seeds = [0.0f32; 3];
    for s in seeds.iter_mut() {
        *s = rng.f01() * 9.0;
    }
    book.layer(1.0, 30.0, None, |g| {
        for (bi, (color, y0f, amp, sp, thick)) in bands.iter().enumerate() {
            let y0 = h * y0f;
            let mut p = vieww_foundation::Path::new();
            let steps = 26;
            let mut pts: Vec<Offset> = Vec::with_capacity(steps + 1);
            for i in 0..=steps {
                let x = w * i as f32 / steps as f32;
                let u = i as f32 / steps as f32;
                let y = y0
                    + (u * 4.2 + t * sp + seeds[bi]).sin() * amp
                    + (u * 9.1 - t * sp * 1.7 + seeds[bi] * 2.0).sin() * amp * 0.35;
                pts.push(Offset::new(x, y));
            }
            p.move_to(pts[0]);
            for pt in pts.iter().skip(1) {
                p.line_to(*pt);
            }
            for pt in pts.iter().rev() {
                p.line_to(Offset::new(
                    pt.dx,
                    pt.dy + thick * (0.7 + 0.3 * (pt.dx / w).sin()),
                ));
            }
            p.close();
            let grad = Gradient::vertical().with_dither().with_stops(&[
                (0.0, alpha(*color, 0.0)),
                (0.45, alpha(*color, a * 0.30)),
                (1.0, alpha(*color, 0.0)),
            ]);
            g.fill(p, grad);
        }
    });
}

/// **Bokeh** — defocused discs drifting through the depth field, the
/// dark's warm air. Drawn inside one blurred layer so each disc is a
/// soft circle of light, not a circle with a soft edge.
pub(crate) fn bokeh(
    book: &mut Sketchbook,
    w: f32,
    h: f32,
    t: f32,
    seed: u64,
    n: usize,
    a: f32,
    color: Color,
) {
    let mut rng = Rng::new(seed);
    let mut disc = Vec::with_capacity(n);
    for _ in 0..n {
        disc.push((
            rng.f01() * w,
            rng.f01() * h,
            8.0 + rng.f01() * 34.0,
            0.15 + rng.f01() * 0.35, // drift speed
            0.4 + rng.f01() * 0.6,   // alpha scale
        ));
    }
    book.layer(1.0, 9.0, None, |g| {
        for (x, y, r, sp, ascale) in disc {
            let dx = (t * 12.0 * sp + x).rem_euclid(w);
            let dy = y - (t * 6.0 * sp).sin() * 24.0;
            let pulse = 0.6 + 0.4 * (t * 0.5 + sp * 9.0).sin();
            g.circle(Offset::new(dx, dy), r, alpha(color, a * ascale * pulse));
        }
    });
}

/// **A full-frame flash** — the match-cut breath between scenes; `a` is
/// its envelope. Never pure white: a hair of the accent rides in.
pub(crate) fn flash(book: &mut Sketchbook, w: f32, h: f32, a: f32, color: Color) {
    let a = a.clamp(0.0, 1.0);
    if a <= 0.004 {
        return;
    }
    book.rect(Rect::new(0.0, 0.0, w, h), alpha(tint(color, 0.6), a * 0.85));
}

/// **Scanline bands** — the old world's CRT register: horizontal bands
/// crawling down the frame, alternating darkness and a faint tint.
pub(crate) fn scanbands(book: &mut Sketchbook, w: f32, h: f32, t: f32, a: f32, color: Color) {
    let band_h = 90.0;
    let drift = (t * 130.0) % (band_h * 2.0);
    let mut y = -band_h * 2.0 + drift;
    let mut i = 0;
    while y < h {
        let fade = (1.0 - (y / h - 0.5).abs() * 1.4).clamp(0.0, 1.0);
        if i % 2 == 0 {
            book.rect(
                xywh(0.0, y, w, band_h * 0.55),
                alpha(Color::BLACK, a * 0.30 * fade),
            );
        } else {
            book.rect(
                xywh(0.0, y, w, band_h * 0.35),
                alpha(color, a * 0.05 * fade),
            );
        }
        y += band_h;
        i += 1;
    }
}

/// **A perspective grid plane** — one receding floor/wall plane of the
/// machine's architecture: `z` in `[0, 1]` is its depth (0 = at the
/// camera, 1 = at the vanishing point), and the camera's `dive` shifts
/// the plane toward the eye. The descent's geography.
pub(crate) fn grid_plane(
    book: &mut Sketchbook,
    w: f32,
    h: f32,
    z: f32,
    dive: f32,
    a: f32,
    color: Color,
) {
    let vpx = w * 0.5;
    let vpy = h * 0.42;
    let z_eff = (z - dive).rem_euclid(1.0);
    let scale = 1.0 - z_eff * 0.86;
    let a_eff = a * (0.25 + 0.75 * (1.0 - z_eff));
    let gw = w * 1.8 * scale;
    let gh = h * 1.4 * scale;
    let x0 = vpx - gw * 0.5;
    let y0 = vpy - gh * 0.5;
    let cols = 9;
    let rows = 6;
    let mut rng = Rng::new(0xC0DE ^ (z.to_bits() as u64));
    let jit = 0.9 + rng.f01() * 0.2;
    for i in 0..=cols {
        let x = x0 + gw * i as f32 / cols as f32 * jit;
        book.line(
            Offset::new(x, y0),
            Offset::new(vpx + (x - vpx) * 0.12, y0 + gh),
            alpha(color, a_eff * 0.34),
            1.0,
        );
    }
    for j in 0..=rows {
        let y = y0 + gh * j as f32 / rows as f32;
        let converge = 1.0 - (j as f32 / rows as f32) * 0.55;
        let half = gw * 0.5 * converge;
        book.line(
            Offset::new(vpx - half, y),
            Offset::new(vpx + half, y),
            alpha(color, a_eff * 0.30),
            1.0,
        );
    }
}

/// **A receipt chip row** — the scene-level provenance line: chips laid
/// out left-to-right at a baseline, fading in staggered (the `Sequence`
/// discipline: annotations bloom in order, never simultaneously).
pub(crate) fn chip_row(chips: &[(&str, Color)], x: f32, y: f32, appear: f32) -> WidgetNode {
    let a = ease_out_cubic(appear.clamp(0.0, 1.0));
    if a <= 0.01 {
        return Stack::new().into();
    }
    let mut stack = Stack::new();
    let mut cx = x;
    for (i, (text, color)) in chips.iter().enumerate() {
        // Staggered arrival — each chip rides 0.08 behind the last.
        let chip_a = ((a - i as f32 * 0.08) / 0.6).clamp(0.0, 1.0);
        if chip_a <= 0.01 {
            continue;
        }
        let w = gmono_w(14.0, text.len()) + 26.0;
        let rise = (1.0 - ease_out_cubic(chip_a)) * 10.0;
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

/// **A damage rect** — the inspector's grammar: an outlined rect with
/// corner ticks, the film's most reused receipt.
pub(crate) fn damage_rect(book: &mut Sketchbook, r: Rect, a: f32, color: Color) {
    let a = a.clamp(0.0, 1.0);
    if a <= 0.01 {
        return;
    }
    let tick = 10.0f32.min(r.width() * 0.4).min(r.height() * 0.4).max(3.0);
    // The body — hairline outline.
    book.stroke_rrect(
        xywh(r.left, r.top, r.width(), r.height()),
        4.0,
        alpha(color, a * 0.85),
        1.6,
    );
    // Corner ticks — heavier than the body, the inspector's brackets.
    let corners = [
        (r.left, r.top, 1.0, 1.0),
        (r.right, r.top, -1.0, 1.0),
        (r.right, r.bottom, -1.0, -1.0),
        (r.left, r.bottom, 1.0, -1.0),
    ];
    for (x, y, sx, sy) in corners {
        book.line(
            Offset::new(x, y),
            Offset::new(x + sx * tick, y),
            alpha(color, a),
            3.0,
        );
        book.line(
            Offset::new(x, y),
            Offset::new(x, y + sy * tick),
            alpha(color, a),
            3.0,
        );
    }
}

// ── The studio scenes' chrome — the film's voice over the real product ─────
//
// The studio renders as itself, ungraded: no vignette, no grain, no
// flash over its pixels (the honesty rule — visible ⇒ the product
// rendered it). The film's own layer is purely annotative: the act
// chip, the witness, the session clock, a soft scrim behind the
// captions so they stay legible over the editor.

/// The studio scenes' shared chrome: the act chip (top-left), the
/// witness chip and the session clock (bottom-right), and the caption
/// scrim. The captions themselves are each scene's own.
pub(crate) fn studio_chrome(ctx: &Ctx) -> Stack {
    let ladder = ctx.ladder;
    let pulse = tap_pulse(ctx.abs);
    let abs = ctx.abs;
    Stack::new()
        // The caption scrim — a soft gradient behind the bottom band,
        // annotation infrastructure rather than grading.
        .push(Positioned::fill().child(Painting::sized(
            CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, s: Size| {
                let _ = s;
                book.rect(
                    xywh(0.0, H - 150.0, W, 150.0),
                    Gradient::vertical().with_dither().with_stops(&[
                        (0.0, alpha(Color::BLACK, 0.0)),
                        (1.0, alpha(Color::BLACK, 0.42)),
                    ]),
                );
            }),
        )))
        .push(act_chip(
            "III",
            "THE STUDIO",
            clamp01((ctx.sec - 0.3) / 0.5),
        ))
        // The witness — bottom-right.
        .push(witness_chip(ladder, pulse))
        .push(session_chip(abs, clamp01((ctx.sec - 1.2) / 0.6)))
}

/// The tap ring — the film's annotation of a real interaction: a bloom
/// ring expanding from the tap point, plus a small spark there. The
/// product's own ripple (if any) is beneath it.
pub(crate) fn tap_ring(_at: Offset, since: f32) -> WidgetNode {
    if since < 0.0 || since > 1.0 {
        return Stack::new().into();
    }
    let r = 12.0 + since * 130.0;
    let a = (1.0 - since).max(0.0);
    Painting::sized(
        Size::new(360.0, 360.0),
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            let cx = 180.0;
            let cy = 180.0;
            book.ring(Offset::new(cx, cy), r, 2.6, alpha(ACCENT, a * 0.85));
            book.ring(
                Offset::new(cx, cy),
                r * 0.6,
                1.4,
                alpha(tint(ACCENT, 0.4), a * 0.6),
            );
            if a > 0.5 {
                glow(book, cx, cy, 90.0, ACCENT, (a - 0.5) * 0.5);
                spark(book, cx, cy, 8.0, since * 6.0, a, ACCENT);
            }
        }),
    )
    .into()
}

/// A positioned tap ring at window coordinates.
pub(crate) fn tap_ring_at(at: Offset, since: f32) -> WidgetNode {
    let size = 360.0;
    let node = tap_ring(at, since);
    Stack::new()
        .push(
            Positioned::new()
                .left(at.dx - size * 0.5)
                .top(at.dy - size * 0.5)
                .width(size)
                .height(size)
                .child(node),
        )
        .into()
}

/// A receipt chip row pinned bottom-right (the studio scenes' provenance
/// line, mirroring the captions on the left).
pub(crate) fn receipt_row(chips: &[(&str, Color)], appear: f32) -> WidgetNode {
    chip_row(chips, W - 900.0, 936.0, appear)
}

/// **The session rail** — the film's own spine, drawn when a scene wants
/// the continuity visible without the studio: a horizontal line with
/// the ladder's ticks and a traveling pulse.
pub(crate) fn session_rail(book: &mut Sketchbook, x0: f32, x1: f32, y: f32, abs: f32, a: f32) {
    let a = a.clamp(0.0, 1.0);
    if a <= 0.01 {
        return;
    }
    book.line(
        Offset::new(x0, y),
        Offset::new(x1, y),
        alpha(Color::WHITE, 0.10 * a),
        1.0,
    );
    let total = total_seconds();
    let frac = (abs / total).clamp(0.0, 1.0);
    // The ladder's ticks — one per tap already landed.
    for tap in taps().iter() {
        let tx = x0 + (x1 - x0) * (tap / total);
        let lit = abs >= *tap;
        let bloom = if lit { tap_pulse(abs) } else { 0.0 };
        book.line(
            Offset::new(tx, y - 7.0),
            Offset::new(tx, y + 7.0),
            alpha(
                if lit { ACCENT } else { FAINT },
                if lit { a } else { a * 0.5 },
            ),
            if lit { 2.0 } else { 1.0 },
        );
        if lit {
            book.circle(
                Offset::new(tx, y),
                2.4 + bloom * 2.2,
                alpha(ACCENT, a * (0.5 + bloom * 0.5)),
            );
        }
    }
    // The playhead — the session's own position.
    let px = x0 + (x1 - x0) * frac;
    book.circle(Offset::new(px, y), 3.4, alpha(tint(ACCENT, 0.3), a));
}
