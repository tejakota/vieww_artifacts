//! wrightfilms — **THE ASK, THE WORKFORCE, THE PROOF** — the Wright
//! product film, rendered by vieww.
//!
//! **The keynote cut.** Five movements, thirteen scenes, 2:19 at 60 fps,
//! one canvas (1920×1080 logical). Every frame is rasterised by vieww's
//! own CPU renderer — nothing here consults a wall clock: a frame is a
//! pure function of its index.
//!
//! | movement | scenes | feeling |
//! |---|---|---|
//! | I · THE ASK | W00, W01, W02, W03 | the question, the words, the blueprint, the gate — describe it |
//! | II · THE WORKFORCE | W04, W05 | the plan as a picture: agents in stages, the bounds they run in |
//! | III · THE RUN | W06, W07, W08, W10 | the *actual* wright window: the run, the approval, the engine, the delivery |
//! | IV · THE PROOF | W09, W10 | the receipt — verify's own words, six checks, the signature |
//! | V · THE REFIRE | W11, W12 | the moat: one change, only what changed runs; the end card |
//!
//! **The house rule this film holds above all:** movement III is not a
//! mock. The film mounts the real `wright::ui` shell — the same `Ui`,
//! the same `shell::root`, the same signals the window runs on — on this
//! film's own `FrameDriver` ([`wright_app`]), and drives it with a
//! frame-indexed script. The run it shows is a **real firing** against
//! the engine's own mock provider; the event spine the run card and the
//! engine view project is the spine that firing wrote; the receipt
//! movement quotes `wright verify`'s sentences for that very run, read
//! from the store at film start. When the film says "delivered and
//! verified", a machine can check it.
//!
//! # The palette is the product's
//!
//! Every colour the film's chrome draws is Wright's own palette
//! (`apps/wright/app/src/ui/theme.rs`, VS Code Dark Modern by role):
//! ground `#1F1F1F`, surface `#252526`, ink `#CCCCCC`, accent `#0078D4`,
//! good `#89D185`, wait `#CCA700`, bad `#F14C4C` — and the mark is the
//! app's own W geometry (`wright::ui::logo::W`), imported, not redrawn.
//! The film's *room* (what surrounds the window) sits one step darker
//! than the app's ground so the product is always the brightest thing
//! in its own film.
//!
//! # The dissolve grammar
//!
//! Every cut is a frame dissolve: the previous scene's final frame
//! cross-dissolves into the new scene's live frame — no dip to dark, no
//! wipes — held longer where a movement gives way to the next (see
//! `master`). The Wright act enters as a window growing out of the
//! previous scene's content box, corners curving in: the rectangle
//! becoming the rounded rectangle.

/// The film's master cadence.
pub(crate) const FPS: f32 = 60.0;
/// The film's canvas, logical pixels.
pub(crate) const W: f32 = 1920.0;
pub(crate) const H: f32 = 1080.0;

use vieww_foundation::Color;
use vieww_widget::WidgetNode;

use crate::{m0_ask, m1_plan, m2_run, m3_proof, m4_refire, wright_app};

// ── the palette: Wright's own, one step darker for the room ─────────────────

/// The room the window sits in — the app's ground, deepened.
pub(crate) const ROOM: Color = Color::rgb(0x14, 0x14, 0x17);
/// The app's surface (`#252526`), quoted.
pub(crate) const SURFACE: Color = Color::rgb(0x25, 0x25, 0x26);
/// The app's ink (`#CCCCCC`), quoted.
pub(crate) const INK: Color = Color::rgb(0xCC, 0xCC, 0xCC);
/// The app's muted (`#9D9D9D`), quoted.
pub(crate) const MUTED: Color = Color::rgb(0x9D, 0x9D, 0x9D);
/// The app's accent (`#0078D4`, VS Code blue), quoted — the mark, the
/// edges, the one colour that means "wright is doing this".
pub(crate) const ACCENT: Color = Color::rgb(0x00, 0x78, 0xD4);
/// The app's good (`#89D185`), quoted — done, verified, delivered.
pub(crate) const GOOD: Color = Color::rgb(0x89, 0xD1, 0x85);
/// The app's wait (`#CCA700`), quoted — asking, holding.
pub(crate) const WAIT: Color = Color::rgb(0xCC, 0xA7, 0x00);
/// The app's bad (`#F14C4C`), quoted.
pub(crate) const BAD: Color = Color::rgb(0xF1, 0x4C, 0x4C);
/// The app's bubble (`#04395E`), quoted — your words in the thread.
pub(crate) const BUBBLE: Color = Color::rgb(0x04, 0x39, 0x5E);
/// A quiet line, between the room and the surface.
pub(crate) const LINE: Color = Color::rgb(0x2D, 0x2D, 0x2D);

// ── the scene registry ──────────────────────────────────────────────────────

/// How a scene renders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A pure widget tree — a function of the frame's ctx.
    Pure,
    /// The **actual wright window**: the rig mounts `wright::ui` on the
    /// film's driver and applies the session script up to this frame;
    /// the scene's `build` returns only the overlay chrome on top.
    Wright,
}

/// What a scene may read: its own clock, the film clock.
pub(crate) struct Ctx<'a> {
    /// `t` in `[0, 1)` within the scene. Every scene reads `sec`; `t`
    /// is the contract for the ones that want it.
    #[allow(dead_code)]
    pub t: f32,
    /// Seconds into the scene.
    pub sec: f32,
    /// The run the film fired: its event spine, its receipt in verify's
    /// words, its plan — the honest numbers every scene may quote.
    pub demo: &'a wright_app::Demo,
}

/// One scene in the film.
#[derive(Clone)]
pub(crate) struct SceneDef {
    /// "W00" — the scene id.
    pub id: &'static str,
    /// "the_question" — the scene name.
    pub name: &'static str,
    /// Film seconds this scene spans.
    pub seconds: f32,
    /// How the harness renders this scene.
    pub kind: Kind,
    /// The scene — a pure function of its ctx.
    pub build: fn(&Ctx) -> WidgetNode,
}

impl SceneDef {
    /// Frames this scene emits at master cadence — derived, never typed.
    pub(crate) fn frames(&self) -> usize {
        (self.seconds * FPS).round() as usize
    }
}

/// Which movement a scene belongs to — the chrome's label and the
/// dissolve's widening rule both read it.
pub(crate) fn movement_of(id: &str) -> usize {
    match id {
        "W00" | "W01" | "W02" | "W03" => 1,
        "W04" | "W05" => 2,
        "W06" | "W07" | "W08" => 3,
        "W09" | "W10" => 4,
        _ => 5,
    }
}

/// The movement titles, as the chrome shows them.
pub(crate) fn movement_title(m: usize) -> &'static str {
    match m {
        1 => "I · THE ASK",
        2 => "II · THE WORKFORCE",
        3 => "III · THE RUN",
        4 => "IV · THE PROOF",
        _ => "V · THE REFIRE",
    }
}

/// The thirteen scenes, in cut order. Movement sums:
/// I 42 · II 20 · III 35 · IV 21 · V 21 → 139 s = 2:19.
pub(crate) fn scenes() -> Vec<SceneDef> {
    vec![
        // ── Movement I · THE ASK ────────────────────────────────────
        SceneDef {
            id: "W00",
            name: "the_question",
            seconds: 10.0,
            kind: Kind::Pure,
            build: m0_ask::the_question,
        },
        SceneDef {
            id: "W01",
            name: "the_words",
            seconds: 11.0,
            kind: Kind::Pure,
            build: m0_ask::the_words,
        },
        SceneDef {
            id: "W02",
            name: "the_blueprint",
            seconds: 11.0,
            kind: Kind::Pure,
            build: m0_ask::the_blueprint,
        },
        SceneDef {
            id: "W03",
            name: "the_gate",
            seconds: 10.0,
            kind: Kind::Pure,
            build: m0_ask::the_gate,
        },
        // ── Movement II · THE WORKFORCE ─────────────────────────────
        SceneDef {
            id: "W04",
            name: "the_workforce",
            seconds: 11.0,
            kind: Kind::Pure,
            build: m1_plan::the_workforce,
        },
        SceneDef {
            id: "W05",
            name: "the_bounds",
            seconds: 9.0,
            kind: Kind::Pure,
            build: m1_plan::the_bounds,
        },
        // ── Movement III · THE RUN — the actual window ──────────────
        SceneDef {
            id: "W06",
            name: "the_window",
            seconds: 13.0,
            kind: Kind::Wright,
            build: m2_run::the_window,
        },
        SceneDef {
            id: "W07",
            name: "the_approval",
            seconds: 10.0,
            kind: Kind::Wright,
            build: m2_run::the_approval,
        },
        SceneDef {
            id: "W08",
            name: "the_engine",
            seconds: 12.0,
            kind: Kind::Wright,
            build: m2_run::the_engine,
        },
        // ── Movement IV · THE PROOF ─────────────────────────────────
        SceneDef {
            id: "W09",
            name: "the_receipt",
            seconds: 12.0,
            kind: Kind::Pure,
            build: m3_proof::the_receipt,
        },
        SceneDef {
            id: "W10",
            name: "the_delivered",
            seconds: 9.0,
            kind: Kind::Wright,
            build: m3_proof::the_delivered,
        },
        // ── Movement V · THE REFIRE ─────────────────────────────────
        SceneDef {
            id: "W11",
            name: "the_change",
            seconds: 11.0,
            kind: Kind::Pure,
            build: m4_refire::the_change,
        },
        SceneDef {
            id: "W12",
            name: "the_endcard",
            seconds: 10.0,
            kind: Kind::Pure,
            build: m4_refire::the_endcard,
        },
    ]
}

/// Where scene `i` starts, in whole frames from the top of the film —
/// the keynote's own spine. (`master::Film` computes the same for every
/// cut from its own list; this one stays for the film's own docs.)
#[allow(dead_code)]
pub(crate) fn scene_start(i: usize) -> usize {
    scenes()[..i].iter().map(SceneDef::frames).sum()
}

/// The film's whole length in frames — derived, never typed.
#[allow(dead_code)]
pub(crate) fn total_frames() -> usize {
    scenes().iter().map(SceneDef::frames).sum()
}

/// The seconds `e` eases 0→1 with, the house curve.
pub(crate) fn ease(sec: f32, over: f32) -> f32 {
    let x = (sec / over).clamp(0.0, 1.0);
    1.0 - (1.0 - x).powi(3)
}

/// `x` clamped to the unit interval.
pub(crate) fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}
