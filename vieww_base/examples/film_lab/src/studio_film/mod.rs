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
pub mod kit;
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

/// The film's scene list — **five minutes, twenty-two scenes**.
///
/// Seconds are the budget; frames derive. The v3 cut ran 2:16 and had to
/// state each argument once and move on; five minutes is room to *show*
/// them — the drift between ports, the bridge tax, the engine's layers,
/// the text stack, the shell's own anatomy, the inspector, the ship step,
/// and a proof movement that can afford both the gauge and the crates.
///
/// | movement | scenes | seconds |
/// |---|---|---|
/// | I · THE NEED | Z01–Z05 | 63 |
/// | II · THE ENGINE | Z06–Z10 | 63 |
/// | III · THE STUDIO | Z11–Z17 | 100 |
/// | IV · THE PROOF | Z18–Z19 | 32 |
/// | V · THE RELEASE | Z20–Z22 | 42 |
pub fn scenes() -> Vec<SceneDef> {
    vec![
        // ── Movement I · THE NEED ───────────────────────────────────
        SceneDef { id: "Z01", name: "the_same_picture",  seconds: 12.0, kind: Kind::Pure,   build: m1_need::the_same_picture },
        SceneDef { id: "Z02", name: "the_tolls",         seconds: 12.0, kind: Kind::Pure,   build: m1_need::the_tolls },
        SceneDef { id: "Z03", name: "the_drift",         seconds: 11.0, kind: Kind::Pure,   build: m1_need::the_drift },
        SceneDef { id: "Z04", name: "the_bridge",        seconds: 12.0, kind: Kind::Pure,   build: m1_need::the_bridge },
        SceneDef { id: "Z05", name: "the_question",      seconds: 16.0, kind: Kind::Pure,   build: m1_need::the_question },
        // ── Movement II · THE ENGINE ────────────────────────────────
        SceneDef { id: "Z06", name: "the_engine",        seconds: 15.0, kind: Kind::Pure,   build: m2_engine::the_engine },
        SceneDef { id: "Z07", name: "the_layers",        seconds: 13.0, kind: Kind::Pure,   build: m2_engine::the_layers },
        SceneDef { id: "Z08", name: "the_pipeline",      seconds: 12.0, kind: Kind::Pure,   build: m2_engine::the_pipeline },
        SceneDef { id: "Z09", name: "the_motion",        seconds: 12.0, kind: Kind::Pure,   build: m2_engine::the_motion },
        SceneDef { id: "Z10", name: "the_type",          seconds: 11.0, kind: Kind::Pure,   build: m2_engine::the_type },
        // ── Movement III · THE STUDIO ───────────────────────────────
        // Exactly one Studio scene: the product, shown. The six that
        // follow explain it, and an explanation is a drawing.
        SceneDef { id: "Z11", name: "studio_opens",      seconds: 16.0, kind: Kind::Studio, build: m3_studio::studio_opens },
        SceneDef { id: "Z12", name: "the_shell",         seconds: 13.0, kind: Kind::Pure,   build: m3_studio::the_shell },
        SceneDef { id: "Z13", name: "live_compose",      seconds: 15.0, kind: Kind::Pure,   build: m3_studio::live_compose },
        SceneDef { id: "Z14", name: "say_to_rust",       seconds: 16.0, kind: Kind::Pure,   build: m3_studio::say_to_rust },
        SceneDef { id: "Z15", name: "ships_everywhere",  seconds: 14.0, kind: Kind::Pure,   build: m3_studio::ships_everywhere },
        SceneDef { id: "Z16", name: "the_inspector",     seconds: 13.0, kind: Kind::Pure,   build: m3_studio::the_inspector },
        SceneDef { id: "Z17", name: "build_and_ship",    seconds: 13.0, kind: Kind::Pure,   build: m3_studio::build_and_ship },
        // ── Movement IV · THE PROOF ─────────────────────────────────
        SceneDef { id: "Z18", name: "the_ledger",        seconds: 19.0, kind: Kind::Pure,   build: m4_proof::the_ledger },
        SceneDef { id: "Z19", name: "the_receipts",      seconds: 13.0, kind: Kind::Pure,   build: m4_proof::the_receipts },
        // ── Movement V · THE RELEASE ────────────────────────────────
        SceneDef { id: "Z20", name: "the_pullback",      seconds: 10.0, kind: Kind::Pure,   build: m5_release::the_pullback },
        SceneDef { id: "Z21", name: "the_endcard",       seconds: 18.0, kind: Kind::Pure,   build: m5_release::the_endcard },
        SceneDef { id: "Z22", name: "the_hold",          seconds: 14.0, kind: Kind::Pure,   build: m5_release::the_hold },
    ]
}

/// Which movement a scene belongs to — the one place the mapping lives,
/// so the act chip, the progress rail's major ticks and the film's own
/// documentation cannot disagree.
pub fn movement_of(id: &str) -> &'static str {
    match id {
        "Z01" | "Z02" | "Z03" | "Z04" | "Z05" => "MOVEMENT I",
        "Z06" | "Z07" | "Z08" | "Z09" | "Z10" => "MOVEMENT II",
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
pub const HEADER_H: f32 = 280.0;

/// Where the body ends. Below it: the receipts and the rail, nothing else.
///
/// The footer only ever holds one chip row and a 26 px rail, so 120 px is
/// all it needs; the 180 px the v3 cut gave it was body the scenes could
/// have used.
pub const FOOTER_Y: f32 = 960.0;

/// The body's height — quoted by the scenes that fit themselves to it.
pub const BODY_H: f32 = FOOTER_Y - HEADER_H;

/// The device-resolution multiplier, as a constant.
///
/// The film is composed once, in **logical** pixels, on a 1920×1080
/// canvas: every coordinate in every scene is a 1080p coordinate and
/// nothing in the film knows about output resolution. This is the single
/// number that turns that composition into an actual raster:
///
/// | value | output |
/// |---|---|
/// | `1.0` | 1920×1080 — the base |
/// | `1.5` | 2880×1620 |
/// | `2.0` | 3840×2160 — 4K UHD |
/// | `4.0` | 7680×4320 — 8K |
///
/// It scales the *scene*, not the frame: [`master::raster`] applies it to
/// the finished command list, so every shape, stroke and glyph is
/// re-rasterised at the output resolution rather than a 1080p bitmap
/// being enlarged. Type stays type, hairlines stay hairlines, and a 4K
/// master is a genuine 4K master.
///
/// The `SCALE_FACTOR` environment variable overrides it for a one-off
/// render without touching the source; see [`master::scale_factor`].
/// Render cost grows with the square, so a 4K pass is roughly four times
/// the wall clock of the base.
pub const SCALE_FACTOR: f32 = 1.0;

/// The matte's strength for a scene at `sec` into it.
///
/// One exemption, and it is the brief's: **the studio act's first beat is
/// the exact studio** — the whole app, the whole frame, nothing of the
/// film's over it. The matte arrives with the rest of the film's voice at
/// 0.40 s, on the same envelope, so the moment the audience stops looking
/// at the product and starts being shown it is a single move.
pub fn matte_alpha(id: &str, sec: f32) -> f32 {
    if id == STUDIO_SCENE {
        pf::clamp01((sec - 0.40) / 0.55)
    } else {
        1.0
    }
}

/// The one scene that mounts the real application.
pub const STUDIO_SCENE: &str = "Z11";

/// The matte — the two bands that make the header and footer the film's
/// own, and the body the only place anything may move.
///
/// **They are black.** The v3 cut painted them in the film's ground
/// colour, which is the same colour the scenes sit on, so the frame had
/// margins that nothing could see: the bands read as more room rather
/// than as the edge of the picture. True black separates them from every
/// scene's ground, which is what a matte is for, and it gives the film
/// the one register that is never anything else — so the eye learns in a
/// second that type lives there and pictures do not.
///
/// `a` fades the whole matte. It is 0 for the studio act's opening beat,
/// whose contract is the *exact* studio, full frame, unmatted; everywhere
/// else it is 1 and the bands are absolute.
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
                let black = pf::alpha(Color::BLACK, a);
                book.rect(pf::xywh(0.0, 0.0, pf::W, HEADER_H), black);
                book.rect(pf::xywh(0.0, FOOTER_Y, pf::W, pf::H - FOOTER_Y), black);
                // Each band closes on a hairline and a short feather into
                // the body, so a card that ends near a seam does not end
                // on a hard line — and an accent tick at the left margin
                // ties the two bands to the same grid the captions use.
                book.rect(pf::xywh(0.0, HEADER_H - 1.0, pf::W, 1.0), pf::alpha(Color::WHITE, 0.09 * a));
                book.rect(
                    pf::xywh(0.0, HEADER_H, pf::W, 30.0),
                    Gradient::vertical().with_dither().with_stops(&[
                        (0.0, pf::alpha(Color::BLACK, 0.72 * a)),
                        (1.0, pf::alpha(Color::BLACK, 0.0)),
                    ]),
                );
                book.rect(pf::xywh(0.0, FOOTER_Y, pf::W, 1.0), pf::alpha(Color::WHITE, 0.09 * a));
                book.rect(
                    pf::xywh(0.0, FOOTER_Y - 30.0, pf::W, 30.0),
                    Gradient::vertical().with_dither().with_stops(&[
                        (0.0, pf::alpha(Color::BLACK, 0.0)),
                        (1.0, pf::alpha(Color::BLACK, 0.72 * a)),
                    ]),
                );
                for y in [HEADER_H, FOOTER_Y] {
                    book.rect(pf::xywh(MARGIN, y - 1.5, 120.0, 3.0), pf::alpha(pf::ACCENT, 0.42 * a));
                }
            }),
        ))
        .into()
}

/// The film's left margin — where every left-aligned thing starts, in the
/// bands and in the body alike.
pub const MARGIN: f32 = 84.0;

/// This film's progress rail — one tick per scene, the playhead sliding.
pub fn progress_rail(abs: f32) -> vieww_widget::WidgetNode {
    use vieww_foundation::{Offset, Size, Sketchbook};
    use vieww_widget::prelude::*;
    let list = scenes();
    let total = total_seconds();
    let starts: Vec<f32> = (0..list.len()).map(scene_start).collect();
    let acts = act_starts();
    let frac = (abs / total).clamp(0.0, 1.0);
    // Positioned in the footer band, not at the tree's origin: the origin
    // is the black header now, and a rail drawn there was a rail nobody
    // could read against the act chip.
    Positioned::new()
        .left(0.0)
        .top(pf::H - 46.0)
        .width(pf::W)
        .height(34.0)
        .child(Painting::sized(
            Size::new(pf::W, 34.0),
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                let (x0, x1, y) = (MARGIN, pf::W - MARGIN, 16.0);
                book.line(
                    Offset::new(x0, y),
                    Offset::new(x1, y),
                    pf::alpha(Color::WHITE, 0.09),
                    1.0,
                );
                for (i, st) in starts.iter().enumerate() {
                    let x = x0 + (x1 - x0) * (st / total);
                    let major = acts.contains(&i);
                    book.line(
                        Offset::new(x, y - if major { 7.0 } else { 4.0 }),
                        Offset::new(x, y + if major { 7.0 } else { 4.0 }),
                        pf::alpha(Color::WHITE, if major { 0.30 } else { 0.15 }),
                        1.0,
                    );
                }
                let px = x0 + (x1 - x0) * frac;
                // The travelled part of the rail is lit, so the playhead
                // reads as progress and not as a lone dot.
                book.line(Offset::new(x0, y), Offset::new(px, y), pf::alpha(pf::ACCENT, 0.42), 1.6);
                book.circle(Offset::new(px, y), 4.0, pf::alpha(pf::ACCENT, 0.95));
                book.circle(Offset::new(px, y), 8.0, pf::alpha(pf::ACCENT, 0.18));
            }),
        ))
        .into()
}

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
