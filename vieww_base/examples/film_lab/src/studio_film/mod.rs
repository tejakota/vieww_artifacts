//! studio_film — **THE GAP, THE ENGINE, THE STUDIO** — the viewwstudio
//! product film, rendered by vieww.
//!
//! Five movements, fourteen scenes, 136 s at 30 fps, one canvas
//! (1920×1080 logical). The emotional arc is the brief's own:
//!
//! | movement | scenes | feeling |
//! |---|---|---|
//! | I · THE NEED | Z01–Z03 | curiosity → need — why another UI/UX framework at all |
//! | II · THE ENGINE | Z04–Z06 | relief begins — vieww, the engine under the studio |
//! | III · THE STUDIO | Z07–Z10 | relief, paid — the *actual* viewwstudio doing real work |
//! | IV · THE PROOF | Z11 | trust — real metrics, every number a receipt |
//! | V · THE RELEASE | Z12–Z14 | resolution — the mark, the name, *beta release available today* |
//!
//! Two scenes' kinds, the house rule: `Pure` scenes are functions of the
//! frame's [`Ctx`]; `Studio` scenes ride on the **actual `viewwstudio`
//! app**, mounted once on the film's own driver and driven by this
//! film's session script ([`script`]).
//!
//! **Exactly one scene is `Studio`, by the brief.** Z07 shows the real
//! application — its first frames are the app and nothing else, no matte,
//! no overlay, no camera move — because that is what the product looks
//! like and the audience is owed a look at it. The three scenes that
//! follow *explain* it, and an explanation wants a diagram: they are the
//! film's own drawing of the shell ([`shell`]), which can explode,
//! re-frame and re-tint on a beat. Same rasterizer, same palette, same
//! proportions; a different register. Every frame of every scene —
//! including the studio's own pixels — is rasterised by vieww's native
//! CPU renderer. Nothing here consults a wall clock: a frame is a pure
//! function of its index.

pub mod filmkit;
pub mod m1_need;
pub mod m2_engine;
pub mod m3_studio;
pub mod m4_proof;
pub mod m5_release;
pub mod master;
pub mod script;
pub mod shell;

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

/// This film's frame rate. The house films run 60; this one runs 30 —
/// the motion grammar is decelerate-heavy (every arrival is an
/// ease-out), which reads identically at 30, and the census that
/// certifies it is twice as cheap to walk.
pub const FPS: f32 = 30.0;

/// The studio's accent, quoted from `apps/viewwstudio/src/theme.rs` so
/// the end card and the app agree by construction.
pub const BRAND_NEAR: Color = Color::rgb(0x7E, 0x5C, 0xE8);
pub const BRAND_FAR: Color = Color::rgb(0xB4, 0x91, 0xFF);

/// The four stacks the first movement indicts — the reason a fifth
/// answer has to exist.
pub const PORTS: [&str; 4] = ["web · react", "android", "ios", "desktop · qt"];

/// The film's scene list. Seconds are the budget; frames derive.
pub fn scenes() -> Vec<SceneDef> {
    vec![
        // ── Movement I · THE NEED ───────────────────────────────────
        SceneDef { id: "Z01", name: "the_same_picture", seconds: 10.0, kind: Kind::Pure,   build: m1_need::the_same_picture },
        SceneDef { id: "Z02", name: "the_tolls",        seconds: 10.0, kind: Kind::Pure,   build: m1_need::the_tolls },
        SceneDef { id: "Z03", name: "the_question",     seconds: 8.0,  kind: Kind::Pure,   build: m1_need::the_question },
        // ── Movement II · THE ENGINE ────────────────────────────────
        SceneDef { id: "Z04", name: "the_engine",       seconds: 10.0, kind: Kind::Pure,   build: m2_engine::the_engine },
        SceneDef { id: "Z05", name: "the_pipeline",     seconds: 9.0,  kind: Kind::Pure,   build: m2_engine::the_pipeline },
        SceneDef { id: "Z06", name: "the_motion",       seconds: 9.0,  kind: Kind::Pure,   build: m2_engine::the_motion },
        // ── Movement III · THE STUDIO (the real app) ────────────────
        SceneDef { id: "Z07", name: "studio_opens",     seconds: 10.0, kind: Kind::Studio, build: m3_studio::studio_opens },
        SceneDef { id: "Z08", name: "live_compose",     seconds: 11.0, kind: Kind::Pure,   build: m3_studio::live_compose },
        SceneDef { id: "Z09", name: "say_to_rust",      seconds: 12.0, kind: Kind::Pure,   build: m3_studio::say_to_rust },
        SceneDef { id: "Z10", name: "ships_everywhere", seconds: 9.0,  kind: Kind::Pure,   build: m3_studio::ships_everywhere },
        // ── Movement IV · THE PROOF ─────────────────────────────────
        SceneDef { id: "Z11", name: "the_ledger",       seconds: 14.0, kind: Kind::Pure,   build: m4_proof::the_ledger },
        // ── Movement V · THE RELEASE ────────────────────────────────
        SceneDef { id: "Z12", name: "the_pullback",     seconds: 7.0,  kind: Kind::Pure,   build: m5_release::the_pullback },
        SceneDef { id: "Z13", name: "the_endcard",      seconds: 11.0, kind: Kind::Pure,   build: m5_release::the_endcard },
        SceneDef { id: "Z14", name: "the_hold",         seconds: 6.0,  kind: Kind::Pure,   build: m5_release::the_hold },
    ]
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

/// The session's taps before `abs` — this film's witness ladder, counted
/// from this film's own session, never typed.
pub fn ladder_at(abs: f32) -> u32 {
    script::session()
        .iter()
        .filter(|(at, a)| matches!(a, pf::script::Action::PointerUp(_)) && *at <= abs)
        .count() as u32
}

/// The session's tap times — the overlay's pulse reads them.
pub fn taps() -> Vec<f32> {
    script::session()
        .iter()
        .filter(|(_, a)| matches!(a, pf::script::Action::PointerUp(_)))
        .map(|(at, _)| *at)
        .collect()
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

// ── This film's own chrome ──────────────────────────────────────────────────
//
// The captions and act chips are the product film's (`pf::caption`,
// `pf::act_chip`) so the two films speak with one voice. What this film
// adds is its own progress rail (pinned to *this* film's scene list)
// and its own studio overlay plate.

// ── The frame's three bands ─────────────────────────────────────────────────
//
// The film has a header, a body and a footer, and only one of the three
// is allowed to move.
//
// * **Header** — `0 .. HEADER_H`. The act chip, the two caption lines,
//   and (in the studio act) the headline plate. Type only.
// * **Body** — `HEADER_H .. FOOTER_Y`. *Every* animated thing in the
//   film: every card, thread, slab, gauge, mark and diagram.
// * **Footer** — `FOOTER_Y .. H`. The receipts row, the witness chip and
//   the progress rail. Type only.
//
// This is enforced rather than observed. [`frame_matte`] paints the two
// bands opaque on the chrome layer — under the chrome's own type, over
// everything the world drew — so a scene that strays out of the body is
// simply cut off at the band, the way a matte cuts a frame. No scene can
// quietly reclaim the header by drawing higher.

/// Where the body begins. Above it: the film's voice, nothing else.
pub const HEADER_H: f32 = 300.0;

/// Where the body ends. Below it: the receipts and the rail, nothing else.
pub const FOOTER_Y: f32 = 900.0;

/// The matte's strength for a scene at `sec` into it.
///
/// One exemption, and it is the brief's: **Z07's first beat is the exact
/// studio** — the whole app, the whole frame, nothing of the film's over
/// it. The matte arrives with the rest of the film's voice at 0.40 s, on
/// the same envelope, so the moment the audience stops looking at the
/// product and starts being shown it is a single move.
pub fn matte_alpha(id: &str, sec: f32) -> f32 {
    if id == "Z07" {
        pf::clamp01((sec - 0.40) / 0.55)
    } else {
        1.0
    }
}

/// The matte — the two opaque bands that make the header and footer the
/// film's own, and the body the only place anything may move.
///
/// `a` fades the whole matte. It is 0 for Z07's opening beat, whose
/// contract is the *exact* studio, full frame, unmatted; everywhere else
/// it is 1 and the bands are absolute.
pub fn frame_matte(a: f32) -> vieww_widget::WidgetNode {
    use vieww_foundation::{Gradient, Size, Sketchbook};
    use vieww_widget::prelude::*;
    if a <= 0.005 {
        return Stack::new().into();
    }
    Positioned::fill()
        .child(Painting::sized(
            pf::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                let ground = pf::alpha(pf::BG_DEEP, a);
                book.rect(pf::xywh(0.0, 0.0, pf::W, HEADER_H), ground);
                book.rect(pf::xywh(0.0, FOOTER_Y, pf::W, pf::H - FOOTER_Y), ground);
                // The seams — a hairline where each band meets the body,
                // so the frame reads as composed rather than cropped, and
                // a short accent tick on the left of each.
                // The header's seam feathers downward into the body; the
                // footer's feathers upward. Both are 28 px, so a card
                // that ends near a band does not end on a hard line.
                book.rect(pf::xywh(0.0, HEADER_H - 0.5, pf::W, 1.0), pf::alpha(Color::WHITE, 0.07 * a));
                book.rect(
                    pf::xywh(0.0, HEADER_H, pf::W, 28.0),
                    Gradient::vertical().with_dither().with_stops(&[
                        (0.0, pf::alpha(pf::BG_DEEP, 0.60 * a)),
                        (1.0, pf::alpha(pf::BG_DEEP, 0.0)),
                    ]),
                );
                book.rect(pf::xywh(0.0, FOOTER_Y - 0.5, pf::W, 1.0), pf::alpha(Color::WHITE, 0.07 * a));
                book.rect(
                    pf::xywh(0.0, FOOTER_Y - 28.0, pf::W, 28.0),
                    Gradient::vertical().with_dither().with_stops(&[
                        (0.0, pf::alpha(pf::BG_DEEP, 0.0)),
                        (1.0, pf::alpha(pf::BG_DEEP, 0.60 * a)),
                    ]),
                );
                for y in [HEADER_H, FOOTER_Y] {
                    book.rect(pf::xywh(72.0, y - 1.0, 108.0, 2.0), pf::alpha(pf::ACCENT, 0.30 * a));
                }
            }),
        ))
        .into()
}

/// This film's progress rail — one tick per scene, the playhead sliding.
pub fn progress_rail(abs: f32) -> vieww_widget::WidgetNode {
    use vieww_foundation::{Offset, Size, Sketchbook};
    use vieww_widget::prelude::*;
    let list = scenes();
    let total = total_seconds();
    let starts: Vec<f32> = (0..list.len()).map(scene_start).collect();
    let acts = [0usize, 3, 6, 10, 11]; // movement boundaries
    let frac = (abs / total).clamp(0.0, 1.0);
    Painting::sized(
        Size::new(pf::W, 26.0),
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            let (x0, x1, y) = (180.0, pf::W - 180.0, 18.0);
            book.line(
                Offset::new(x0, y),
                Offset::new(x1, y),
                pf::alpha(Color::WHITE, 0.07),
                1.0,
            );
            for (i, st) in starts.iter().enumerate() {
                let x = x0 + (x1 - x0) * (st / total);
                let major = acts.contains(&i);
                book.line(
                    Offset::new(x, y - if major { 6.0 } else { 4.0 }),
                    Offset::new(x, y + if major { 6.0 } else { 4.0 }),
                    pf::alpha(Color::WHITE, if major { 0.22 } else { 0.13 }),
                    1.0,
                );
            }
            let px = x0 + (x1 - x0) * frac;
            book.circle(Offset::new(px, y), 3.0, pf::alpha(pf::ACCENT, 0.9));
            book.line(
                Offset::new(px, y),
                Offset::new(px, y + 10.0),
                pf::alpha(pf::ACCENT, 0.55),
                1.2,
            );
        }),
    )
    .into()
}
/// The studio act's overlay plate — scrim + act chip + witness + rail,
/// this film's voice over the real product's pixels. The studio itself
/// renders ungraded (the honesty rule): this layer only annotates.
pub fn studio_plate(ctx: &Ctx, movement: &str, name: &str) -> vieww_widget::prelude::Stack {
    studio_plate_faded(ctx, movement, name, 1.0)
}

/// [`studio_plate`], faded by `a` — Z07's first beat asks for the
/// *exact* studio on the first frame, so its plate arrives late.
pub fn studio_plate_faded(
    ctx: &Ctx,
    movement: &str,
    name: &str,
    a: f32,
) -> vieww_widget::prelude::Stack {
    use vieww_foundation::{Gradient, Size, Sketchbook};
    use vieww_widget::prelude::*;
    if a <= 0.01 {
        return Stack::new();
    }
    let abs = ctx.abs;
    let times = taps();
    let pulse = tap_pulse(abs, &times);
    let ladder = ctx.ladder;
    Stack::new()
        .push(Positioned::fill().child(
            Painting::sized(
                pf::CANVAS,
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    // The exposure. The studio's live preview renders a
                    // light-themed app, so the brightest thing in the
                    // studio act is a white rectangle in the right third
                    // — it out-shouted the film's own voice in the v2
                    // cut. A vignette is the cinematographer's answer:
                    // it changes no pixel's meaning, only the frame's
                    // falloff, exactly as a lens would. The app is still
                    // ungraded; the *shot* is exposed.
                    pf::vignette(book, pf::W, pf::H, 0.62 * a);
                    book.rect(
                        pf::xywh(0.0, pf::H - 150.0, pf::W, 150.0),
                        Gradient::vertical().with_dither().with_stops(&[
                            (0.0, pf::alpha(Color::BLACK, 0.0)),
                            (1.0, pf::alpha(Color::BLACK, 0.42 * a)),
                        ]),
                    );
                }),
            ),
        ))
        .push(pf::band_scrim(a))
        // The headline plate's scrim — the same borrow the product film
        // makes: the least costly rectangle in the frame, under the
        // act's big line, over the editor's first code lines.
        .push(Positioned::new().left(0.0).top(pf::BAND_H + 24.0).width(pf::W).height(190.0).child(
            Painting::sized(
                Size::new(pf::W, 190.0),
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    book.rect(
                        pf::xywh(0.0, 0.0, pf::W, 190.0),
                        Gradient::vertical().with_dither().with_stops(&[
                            (0.0, pf::alpha(pf::BG_DEEP, 0.0)),
                            (0.22, pf::alpha(pf::BG_DEEP, 0.80 * a)),
                            (0.78, pf::alpha(pf::BG_DEEP, 0.80 * a)),
                            (1.0, pf::alpha(pf::BG_DEEP, 0.0)),
                        ]),
                    );
                }),
            ),
        ))
        .push(pf::act_chip(movement, name, pf::clamp01((ctx.sec - 0.3) / 0.5) * a))
        .push(witness_chip(ladder, pulse))
        .push(pf::chrome(progress_rail(abs)))
}

/// The witness chip, bottom-right — this film's count of human touches,
/// blooming on every tap. Drawn here (not borrowed) because its pulse
/// reads this film's own session.
fn witness_chip(ladder: u32, pulse: f32) -> vieww_widget::WidgetNode {
    use vieww_foundation::{Offset, Size, Sketchbook};
    use vieww_widget::prelude::*;
    let mut stack = Stack::new();
    stack = stack.push(
        Positioned::new()
            .left(pf::W - 268.0)
            .top(976.0)
            .width(240.0)
            .height(38.0)
            .child(
                Painting::sized(
                    Size::new(240.0, 38.0),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        if pulse > 0.02 {
                            pf::glow(book, 120.0, 19.0, 90.0, pf::ACCENT, pulse * 0.30);
                        }
                        book.rrect(pf::xywh(0.0, 2.0, 240.0, 34.0), 9.0, pf::alpha(pf::SURFACE_2, 0.92));
                        book.stroke_rrect(
                            pf::xywh(0.0, 2.0, 240.0, 34.0),
                            9.0,
                            pf::alpha(pf::ACCENT, 0.30),
                            1.1,
                        );
                        book.circle(Offset::new(24.0, 19.0), 3.4, pf::alpha(pf::ACCENT, 0.9));
                    }),
                ),
            ),
    );
    stack = stack.push(
        Positioned::new()
            .left(pf::W - 268.0 + 40.0)
            .top(984.0)
            .width(180.0)
            .height(24.0)
            .child(
                Opacity::new(1.0).child(
                    Text::new(format!("{ladder} human touches"))
                        .style(pf::geist_mono(14.0).letter_spacing(1.2).color(pf::alpha(pf::INK, 0.9))),
                ),
            ),
    );
    stack.into()
}
