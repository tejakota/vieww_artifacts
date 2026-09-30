//! studio_film — **THE GAP, THE ENGINE, THE STUDIO** — the viewwstudio
//! product film, rendered by vieww.
//!
//! Five movements, twenty-three scenes, 318 s at 60 fps, one canvas
//! (1920×1080 logical). The emotional arc is the brief's own:
//!
//! | movement | scenes | feeling |
//! |---|---|---|
//! | I · THE NEED | Z00–Z05 | curiosity → need — why another UI/UX framework at all |
//! | II · THE ENGINE | Z06–Z10B | relief begins — vieww, the engine under the studio |
//! | III · THE STUDIO | Z11–Z17 | relief, paid — the *actual* viewwstudio doing real work |
//! | IV · THE PROOF | Z18–Z19 | trust — real metrics, every number a receipt |
//! | V · THE RELEASE | Z20–Z22 | resolution — the mark, the name, *beta release available today* |
//!
//! Two scenes' kinds, the house rule: `Pure` scenes are functions of the
//! frame's [`Ctx`]; `Studio` scenes ride on the **actual `viewwstudio`
//! app**, mounted once on the film's own driver and driven by this
//! film's session script ([`script`]).
//!
//! **Z11 opens the studio act on the real application, whole.** Its
//! first frames are the app and nothing else — no matte, no overlay, no
//! camera move — because that is what the product looks like and the
//! audience is owed a look at it. The six scenes that follow *explain*
//! it, and an explanation wants a diagram: they are the film's own
//! drawing of the shell, quoting the app as plates of its own draw list
//! — magnified, taken apart, or captured in a state the scene needs.
//! Same rasterizer, same palette, same proportions; a different
//! register. Every frame of every scene — including the studio's own
//! pixels — is rasterised by vieww's native CPU renderer. Nothing here
//! consults a wall clock: a frame is a pure function of its index.
//!
//! # The audience bookend
//!
//! The cold open names everyone the film is for — *everyone who designs
//! apps, builds them, or simply uses them* — and the end card hands each
//! of them their takeaway in one line before the repository asks
//! anything of anyone: designers get the studio (movement III), builders
//! get the engine and the say-to-Rust descent (movements II–III), users
//! get the everywhere-one-design release (movements I and V), and the
//! proof movement answers the people who sign. Three audiences, one
//! sentence, said twice.
//!
//! # The light rule — no radial glows
//!
//! **This film draws no radial glows.** Not a style call — a pipeline
//! one. The rasterizer is innocent: its gradients are evaluated per
//! pixel in `f32` with a ±0.5/255 ordered dither (`vieww-paint`'s
//! `DeviceRamp`), so a rendered PNG of even the softest halo is smooth
//! *in 8 bits*. What breaks it is the master's encode: `libx264` at
//! `yuv420p` quantises a soft luminance ramp that spans two or three
//! 8-bit levels over a near-black ground into visible steps — the
//! macroblock banding that read as a "glitch". The old glows were drawn
//! at alphas of 0.07–0.16 over `BG_DEEP`, exactly the content H.264
//! spends its fewest bits on. The fix is both halves: the film now draws
//! light the *vector* way (solid cores, crisp rings, strokes, the
//! shadow's own blur), and the encoder gets `aq-mode=3` so the dark
//! ramps that remain keep their bits. Shadows, the linear room
//! gradients and the photographic vignette stay — they are not radial
//! light, and they do not band.
//!
//! # The dissolve grammar
//!
//! Every cut is a frame dissolve, one grammar for the whole film (see
//! `master`): the previous scene's final frame cross-dissolves into the
//! new scene's live frame — no dip to dark, no wipes, no sliding lines
//! — held slightly longer where a movement gives way to the next. Where
//! the incoming world is a carded window (the studio act), the window
//! grows out of the previous scene's content box while its corners
//! curve in: the rectangle becoming the curved rectangle. The need
//! stays plain — abrupt cuts would be flattery the wait does not
//! deserve.

pub mod filmkit;
pub mod m0_open;
pub mod m1_need;
pub mod m2_engine;
pub mod m2_beyond;
pub mod m3_studio;
pub mod m4_proof;
pub mod m5_release;
pub mod master;
pub mod probe;
pub mod frame;
pub mod layout;
pub mod space;
pub mod models;
pub mod script;
pub mod kit;

use crate::product_film as pf;
use crate::product_film::{Ctx, Kind, SceneDef};
use vieww_foundation::Color;

// The house palette and the certification receipts, re-exported so this
// film's scene modules can speak the same names the product film does.
pub use crate::product_film::{
    ACCENT, ACCENT_DEEP, BG_DEEP, BREAK_RED, CANVAS, ENGINE, FAINT, GROUND, H, INK, LEDGER, LINE,
    MARK_GROUND, MARK_PANEL, MUTED, SURFACE, SURFACE_2, SYN_COMMENT, SYN_FUNCTION, SYN_KEYWORD,
    SYN_MACRO, SYN_NUMBER, SYN_PUNCT, SYN_STRING, SYN_TYPE, TERM_GREEN, W, WASH, CERT_ALLOCS_STEADY,
    CERT_CRATES, CERT_FRAMES_STEADY, CERT_P95_MS, CERT_STARTUP_MS, CERT_TESTS, CERT_VULKAN_TESTS,
    CERT_WORST_MS,
};

/// This film's frame rate. v4 ran at 30; v5 runs at 60, because the
/// review asked for smooth animation and every move in the film is a
/// function of seconds — doubling the frame rate doubles the samples of
/// the same curves and changes nothing else.
pub const FPS: f32 = 60.0;

/// The scene whose live-edit window the census times (the alive receipt).
pub const ALIVE_SCENE: &str = "Z13";

/// The studio's accent, quoted from `apps/viewwstudio/src/theme.rs` so
/// the end card and the app agree by construction.
pub const BRAND_NEAR: Color = Color::rgb(0x7E, 0x5C, 0xE8);
pub const BRAND_FAR: Color = Color::rgb(0xB4, 0x91, 0xFF);

/// The four stacks the first movement indicts — the reason a fifth
/// answer has to exist.
pub const PORTS: [&str; 4] = ["web · react", "android", "ios", "desktop · qt"];

/// The film's scene list — **five minutes eighteen, twenty-four scenes**.
///
/// Seconds are the budget; frames derive.
///
/// | movement | scenes | seconds |
/// |---|---|---|
/// | I · THE NEED | Z00–Z05 | 71 (Z00 is the 11 s cold open) |
/// | II · THE ENGINE | Z06–Z10B | 77 |
/// | III · THE STUDIO | Z11–Z17 | 100 |
/// | IV · THE PROOF | Z18–Z19 | 32 |
/// | V · THE RELEASE | Z20–Z22 | 38 |
///
/// The engine movement ends on Z10B, four more things the engine
/// draws — a 3D model (vieww-mesh), physics (vieww-physics), a
/// character on bones (vieww-animation) and charts (vieww-widget) —
/// each running live.
pub fn scenes() -> Vec<SceneDef> {
    vec![
        // ── Movement I · THE NEED ───────────────────────────────────
        SceneDef { id: "Z00", name: "the_hush",          seconds: 11.0, kind: Kind::Pure,   build: m0_open::the_hush },
        SceneDef { id: "Z01", name: "the_same_picture",  seconds: 12.0, kind: Kind::Pure,   build: m1_need::the_same_picture },
        SceneDef { id: "Z02", name: "the_tolls",         seconds: 12.0, kind: Kind::Pure,   build: m1_need::the_tolls },
        SceneDef { id: "Z03", name: "the_drift",         seconds: 11.0, kind: Kind::Pure,   build: m1_need::the_drift },
        SceneDef { id: "Z04", name: "the_bridge",        seconds: 12.0, kind: Kind::Pure,   build: m1_need::the_bridge },
        SceneDef { id: "Z05", name: "the_question",      seconds: 13.0, kind: Kind::Pure,   build: m1_need::the_question },
        // ── Movement II · THE ENGINE ────────────────────────────────
        SceneDef { id: "Z06", name: "the_engine",        seconds: 15.0, kind: Kind::Pure,   build: m2_engine::the_engine },
        SceneDef { id: "Z07", name: "the_layers",        seconds: 13.0, kind: Kind::Pure,   build: m2_engine::the_layers },
        SceneDef { id: "Z08", name: "the_pipeline",      seconds: 12.0, kind: Kind::Pure,   build: m2_engine::the_pipeline },
        SceneDef { id: "Z09", name: "the_motion",        seconds: 12.0, kind: Kind::Pure,   build: m2_engine::the_motion },
        SceneDef { id: "Z10", name: "the_type",          seconds: 11.0, kind: Kind::Pure,   build: m2_engine::the_type },
        SceneDef { id: "Z10B", name: "beyond_ui",        seconds: 14.0, kind: Kind::Pure,   build: m2_beyond::beyond_ui },
        // ── Movement III · THE STUDIO ───────────────────────────────
        // Exactly one Studio scene: the product, shown. The six that
        // follow explain it, and an explanation is a drawing.
        SceneDef { id: "Z11", name: "studio_opens",      seconds: 16.0, kind: Kind::Studio, build: m3_studio::studio_opens },
        SceneDef { id: "Z12", name: "the_shell",         seconds: 13.0, kind: Kind::Studio, build: m3_studio::the_shell },
        SceneDef { id: "Z13", name: "live_compose",      seconds: 15.0, kind: Kind::Studio, build: m3_studio::live_compose },
        SceneDef { id: "Z14", name: "say_to_rust",       seconds: 16.0, kind: Kind::Studio, build: m3_studio::say_to_rust },
        SceneDef { id: "Z15", name: "ships_everywhere",  seconds: 14.0, kind: Kind::Studio, build: m3_studio::ships_everywhere },
        SceneDef { id: "Z16", name: "the_devices",       seconds: 13.0, kind: Kind::Studio, build: m3_studio::the_devices },
        SceneDef { id: "Z17", name: "build_and_ship",    seconds: 13.0, kind: Kind::Studio, build: m3_studio::build_and_ship },
        // ── Movement IV · THE PROOF ─────────────────────────────────
        SceneDef { id: "Z18", name: "the_ledger",        seconds: 19.0, kind: Kind::Pure,   build: m4_proof::the_ledger },
        SceneDef { id: "Z19", name: "the_receipts",      seconds: 13.0, kind: Kind::Pure,   build: m4_proof::the_receipts },
        // ── Movement V · THE RELEASE ────────────────────────────────
        SceneDef { id: "Z20", name: "the_pullback",      seconds: 10.0, kind: Kind::Studio,   build: m5_release::the_pullback },
        SceneDef { id: "Z21", name: "the_endcard",       seconds: 18.0, kind: Kind::Pure,   build: m5_release::the_endcard },
        SceneDef { id: "Z22", name: "the_hold",          seconds: 10.0, kind: Kind::Pure,   build: m5_release::the_hold },
    ]
}

/// Which movement a scene belongs to — the one place the mapping lives,
/// so the act chip, the progress rail's major ticks and the film's own
/// documentation cannot disagree.
pub fn movement_of(id: &str) -> &'static str {
    match id {
        "Z00" | "Z01" | "Z02" | "Z03" | "Z04" | "Z05" => "MOVEMENT I",
        "Z06" | "Z07" | "Z08" | "Z09" | "Z10" | "Z10B" => "MOVEMENT II",
        "Z11" | "Z12" | "Z13" | "Z14" | "Z15" | "Z16" | "Z17" => "MOVEMENT III",
        "Z18" | "Z19" => "MOVEMENT IV",
        _ => "MOVEMENT V",
    }
}

/// The movement's name, for the act chip.
pub fn movement_name(id: &str) -> &'static str {
    match movement_of(id) {
        "MOVEMENT I" => "THE NEED",
        "MOVEMENT II" => "THE ENGINE",
        "MOVEMENT III" => "THE STUDIO",
        "MOVEMENT IV" => "THE PROOF",
        _ => "THE RELEASE",
    }
}

/// Start time of scene `index`, in film seconds.
pub fn scene_start(index: usize) -> f32 {
    scenes()[..index].iter().map(|s| s.seconds).sum()
}

/// The film's length, seconds.
pub fn total_seconds() -> f32 {
    scenes().iter().map(|s| s.seconds).sum()
}

/// The film's length, frames at [`FPS`].
pub fn total_frames() -> usize {
    (total_seconds() * FPS).round() as usize
}

/// A scene's frame count at this film's frame rate — `SceneDef::frames`
/// is pinned to the 60 fps house films, so this film derives its own.
pub fn frames_of(s: &SceneDef) -> usize {
    (s.seconds * FPS).round() as usize
}

/// The session's taps before `abs` — this cut's session presses nothing
/// with a pointer, so the ladder stays at zero (and the census says so).
pub fn ladder_at(_abs: f32) -> u32 {
    0
}

/// The session's tap times — none in this cut.
pub fn taps() -> Vec<f32> {
    Vec::new()
}

/// A bloom that decays after each of `times` — the tap pulse.
pub fn tap_pulse(abs: f32, times: &[f32]) -> f32 {
    times
        .iter()
        .filter_map(|at| {
            let since = abs - at;
            (0.0..1.4).contains(&since).then(|| 1.0 - since / 1.4)
        })
        .fold(0.0f32, f32::max)
}

// ── The frame ───────────────────────────────────────────────────────────────
//
// The bands, the fit, the header and the footer live in [`frame`]; the
// constants are re-exported here for the scenes that quote them.

pub use frame::{BODY, FOOTER_Y, HEADER_H};

/// The body's height — quoted by the scenes that fit themselves to it.
pub const BODY_H: f32 = FOOTER_Y - HEADER_H;

/// The device-resolution multiplier, as a constant.
///
/// The film is composed once, in **logical** pixels, on a 1920×1080
/// canvas. This is the single number that turns that composition into an
/// actual raster: `1.0` is 1920×1080, `2.0` is 4K UHD, `4.0` is 8K. It
/// scales the *scene*, not the frame — the finished command list is
/// re-rasterised at the output resolution, so type stays type. The
/// `SCALE_FACTOR` environment variable overrides it for a one-off render.
pub const SCALE_FACTOR: f32 = 1.0;

/// The one scene that opens on the real application, whole.
pub const STUDIO_SCENE: &str = "Z11";

/// The film's left margin in the scenes' own coordinates — where every
/// left-aligned thing starts.
pub const MARGIN: f32 = 84.0;

/// The indices at which a movement begins — derived from the scene list's
/// own movement column, never typed twice.
pub fn act_starts() -> Vec<usize> {
    let mut out = Vec::new();
    let mut last = "";
    for (i, s) in scenes().iter().enumerate() {
        let m = movement_of(s.id);
        if m != last {
            out.push(i);
            last = m;
        }
    }
    out
}
