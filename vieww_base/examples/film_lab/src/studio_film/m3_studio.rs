//! Movement III · THE STUDIO — the actual `viewwstudio`, doing real
//! work, with this film's voice annotating it.
//!
//! The studio's pixels come from the mounted app (`script::mount`);
//! these scenes contribute **only the overlay**: the two-tone wordmark,
//! the brackets, the callout threads, the exploded cards, the tap
//! rings, the receipts. The honesty rule holds — nothing here grades
//! the app, and every chip describes what the session is actually
//! doing at that moment (see `super::script`).
//!
//! The v2 grammar, per the brief:
//!
//! * **Z07's first frame is the exact studio** — no plate, no scrim,
//!   no rail, the camera at the identity. From the next beat the
//!   annotations arrive: the wordmark (with `studio` in the brand's
//!   purple), a light sweep, corner brackets, callout threads.
//! * The three feature scenes lift what the session is doing out of
//!   the frame — exploded cards, a flowing compile pipeline, 3D
//!   device slabs — while the app keeps working underneath.

use vieww_foundation::{Color, Offset, Path, Rect, Size, Sketchbook, TextAlign};
use vieww_widget::prelude::*;

use crate::film_lib::{clamp01, ease_out_cubic, ease_out_expo};
use crate::product_film as pf;
use crate::product_film::script::{TAP_ADD_ONE, TAP_LIVE_ROW};
use super::filmkit as fk;
use super::{
    ACCENT, ACCENT_DEEP, BREAK_RED, BRAND_NEAR, CANVAS, ENGINE, INK, LEDGER, MUTED, SYN_MACRO,
    SYN_STRING, SYN_TYPE, W,
};

/// Where the studio's regions sit, in film coordinates — hit-tested
/// once against this session's layout (the tap anchor is the proof)
/// and used by every callout, thread and bracket in the act.
mod region {
    use vieww_foundation::Offset;
    /// The explorer's tree — left rail.
    pub const SIDEBAR: Offset = Offset::new(130.0, 470.0);
    /// The editor's code — centre stage.
    pub const EDITOR: Offset = Offset::new(700.0, 430.0);
    /// The preview device — right pane (the tap anchor lives here).
    pub const PREVIEW: Offset = Offset::new(1585.0, 450.0);
    /// The wordmark's home, over the top band.
    pub const WORDMARK: Offset = Offset::new(960.0, 372.0);
}

/// The studio act's headline — one line, Geist, centered the way the
/// house centres: a full-width box and `TextAlign::Center`, so the
/// words land where the box says and never where a flex wants them.
fn headline(text: &str, sub: &str, ctx: &pf::Ctx) -> vieww_widget::WidgetNode {
    let a = ease_out_expo(pf::clamp01((ctx.t - 0.04) / 0.10));
    if a <= 0.01 {
        return Stack::new().into();
    }
    Stack::new()
        .push(
            Positioned::new()
                .left(0.0)
                .top(0.0)
                .width(pf::W)
                .height(52.0)
                .child(
                    Text::new(text.to_string())
                        .style(pf::geist(40.0).bold().letter_spacing(2.4).color(pf::alpha(INK, 0.97 * a)))
                        .align(TextAlign::Center),
                ),
        )
        .push(
            Positioned::new()
                .left(0.0)
                .top(58.0)
                .width(pf::W)
                .height(26.0)
                .child(
                    Text::new(sub.to_string())
                        .style(pf::geist_mono(15.0).letter_spacing(2.0).color(pf::alpha(ENGINE, 0.9 * a)))
                        .align(TextAlign::Center),
                ),
        )
        .into()
}

/// The headline plate — centered, on the band the studio scenes share.
fn headline_plate(ctx: &pf::Ctx, text: &str, sub: &str) -> vieww_widget::WidgetNode {
    pf::chrome(Stack::new().push(
        Positioned::new()
            .left(0.0)
            .top(pf::BAND_H + 40.0 + (1.0 - ease_out_expo(pf::clamp01((ctx.t - 0.04) / 0.10))) * 14.0)
            .width(pf::W)
            .height(120.0)
            .child(headline(text, sub, ctx)),
    ).into())
}

/// The act's stage — the rectangle every feature annotation is drawn
/// inside, and the plate it clears for itself over the live app.
///
/// The v2 cut drew its cards, pipelines and slabs wherever they fit and
/// let the headline land on top of them; the contact sheets showed a
/// centred 40 px line sitting across a 3D device slab. The v3 rule is
/// four bands that never overlap:
///
/// | band | y | what lives there |
/// |---|---|---|
/// | title  | 146–306 | the act chip and the headline plate |
/// | stage  | 340–652 | the scene's one annotation graphic |
/// | beats  | 690–892 | the session's beat chips, left column |
/// | foot   | 912–1014 | the receipts row and the two captions |
pub const STAGE: Rect = Rect { left: 360.0, top: 340.0, right: 1560.0, bottom: 652.0 };

/// The beat column's origin — left of the stage, under it, clear of
/// both the headline and the receipts row.
const BEAT_X: f32 = 72.0;
const BEAT_Y: f32 = 690.0;
const BEAT_STEP: f32 = 42.0;

/// Clear the stage over the live app: a plate, then a single sheen as
/// it settles. Draw this before the scene's own annotation content.
fn stage(p: f32, color: Color, sec: f32, since: f32) -> vieww_widget::WidgetNode {
    if p <= 0.01 {
        return Stack::new().into();
    }
    let sheen = clamp01((since - 0.35) / 0.7);
    Positioned::fill()
        .child(Painting::sized(CANVAS, PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            fk::stage_plate(book, STAGE, p, color, 1.0);
            if p > 0.9 {
                fk::plate_sheen(book, STAGE, sheen, 1.0);
            }
        })))
        .into()
}

/// A beat chip — a mono chip that fires at one session beat, with its
/// own landing envelope.
fn beat_chip(text: &str, color: Color, x: f32, y: f32, since: f32) -> vieww_widget::WidgetNode {
    if since <= 0.0 {
        return Stack::new().into();
    }
    let a = ease_out_cubic(pf::clamp01(since / 0.24));
    let rise = (1.0 - a) * 10.0;
    pf::chrome(Stack::new().push(
        Positioned::new()
            .left(x)
            .top(y + rise)
            .width(pf::gmono_tw(14.0, text.chars().count(), 1.1) + pf::CHIP_PAD_X * 2.0 + 2.0)
            .height(30.0)
            .child(Opacity::new(a).child(pf::chip(text, 14.0, color))),
    ).into())
}

/// A callout label — a small mono name beside an anchor foot, with its
/// own landing rise. The studio act's lightest annotation.
fn callout_label(text: &str, at: Offset, dx: f32, dy: f32, p: f32, color: Color) -> vieww_widget::WidgetNode {
    if p <= 0.01 {
        return Stack::new().into();
    }
    let a = ease_out_cubic(p);
    let width = pf::gmono_tw(13.0, text.chars().count(), 1.2) + pf::CHIP_PAD_X * 2.0 + 2.0;
    let (lx, ly) = (at.dx + dx, at.dy + dy);
    pf::chrome(Stack::new().push(
        Positioned::new()
            .left(lx)
            .top(ly + (1.0 - a) * 8.0)
            .width(width)
            .height(26.0)
            .child(Opacity::new(a).child(pf::chip(text, 13.0, color))),
    ).into())
}

// ── Z07 · studio_opens ──────────────────────────────────────────────────────

/// The studio act opens. **The first 0.4 s is the exact studio** — no
/// overlay of any kind, the camera at the identity. Then the film's
/// voice arrives: the two-tone wordmark, a light sweep, corner
/// brackets around the whole shell, and callout threads naming the
/// three regions while the session opens the counter behind them.
pub fn studio_opens(ctx: &pf::Ctx) -> vieww_widget::WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let abs = ctx.abs;

    // The pristine beat. Nothing but the product.
    if sec < 0.40 {
        return Stack::new().into();
    }

    let plate_a = clamp01((sec - 0.40) / 0.5);
    let mut stack = super::studio_plate_faded(ctx, "MOVEMENT III", "THE STUDIO", plate_a);

    // The wordmark — `vieww` in ink, `studio` in purple, one shape.
    let mark_a = clamp01((sec - 0.55) / 0.30);
    if mark_a > 0.01 {
        let rise = (1.0 - ease_out_expo(mark_a)) * 12.0;
        pf::chrome(Stack::new().push(
            Positioned::new()
                .left(0.0)
                .top(336.0 + rise)
                .width(W)
                .height(72.0)
                .child(Opacity::new(mark_a).child(fk::wordmark(50.0, 1.0))),
        ).into());
    }

    // The sweep — one bar of light crossing the shell as the name lands.
    // A world-space painting, so the camera rides with it.
    let sweep_p = clamp01((sec - 0.62) / 0.85);
    if sweep_p > 0.001 && sweep_p < 0.999 {
        stack = stack.push(Positioned::fill().child(
            Painting::sized(CANVAS, PaintWith::new(move |book: &mut Sketchbook, s: Size| {
                fk::light_sweep(book, s.width, s.height, sweep_p, 1.0);
            })),
        ));
    }

    // Corner brackets around the whole shell — the frame announcing
    // itself as the subject. Drawn to their own progress.
    let bracket_p = clamp01((sec - 0.70) / 0.7);
    if bracket_p > 0.01 {
        stack = stack.push(Positioned::fill().child(
            Painting::sized(CANVAS, PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                fk::corner_brackets(
                    book,
                    Rect::new(64.0, 322.0, W - 64.0, 878.0),
                    bracket_p,
                    ACCENT,
                    0.85,
                    52.0,
                );
            })),
        ));
    }

    // The three regions — anchor feet bloom, stems rise to labels, and
    // flowing threads tie them to the wordmark. Staggered, left to right.
    let regions: [(Offset, &str, Color, f32, f32); 3] = [
        (region::SIDEBAR, "explore the tree", SYN_TYPE, -336.0, -20.0),
        (region::EDITOR, "edit real code", ACCENT, -128.0, 46.0),
        (region::PREVIEW, "see it live", SYN_STRING, 24.0, 46.0),
    ];
    for (i, (at, label, color, dx, dy)) in regions.iter().enumerate() {
        let foot_p = clamp01((sec - 0.95 - i as f32 * 0.18) / 0.35);
        if foot_p <= 0.01 {
            continue;
        }
        let at = *at;
        let color = *color;
        let phase = sec + i as f32 * 1.7;
        stack = stack.push(Positioned::fill().child(
            Painting::sized(CANVAS, PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                fk::anchor_foot(book, at, foot_p, color, 1.0);
                // The thread from the wordmark to the region — flowing
                // once the foot has landed.
                let thread_a = clamp01((foot_p - 0.5) * 2.0);
                if thread_a > 0.01 {
                    let bend = match i {
                        0 => 96.0,
                        1 => 132.0,
                        _ => -120.0,
                    };
                    fk::flow_thread(book, region::WORDMARK, at, bend, phase, color, thread_a * 0.9, 1.6);
                }
            })),
        ));
        pf::chrome(callout_label(label, at, *dx, *dy, foot_p, color));
    }

    // The beats, exactly as the session plays them.
    beat_chip("open: counter.say", SYN_TYPE, BEAT_X, BEAT_Y, abs - 56.4);
    beat_chip("panel closes — the editor breathes", MUTED, BEAT_X, BEAT_Y + BEAT_STEP, abs - 60.0);
    beat_chip("panel returns — the tree is the map", MUTED, BEAT_X, BEAT_Y + BEAT_STEP * 2.0, abs - 64.0);

    // The receipts.
    pf::chrome(pf::chip_row(
        &[
            ("the real Shell", ACCENT),
            ("the real editor", SYN_TYPE),
            ("the real preview", SYN_STRING),
        ],
        BEAT_X,
        924.0,
        clamp01((t - 0.45) / 0.20),
    ));

    pf::caption("one shell: editor, preview, build, ship.", 1002.0, clamp01((sec - 0.9) / 0.12));
    pf::caption("every pixel below this line is vieww's.", 966.0, clamp01((sec - 1.3) / 0.12));
    stack.into()
}

// ── The explanation scenes ──────────────────────────────────────────────────
//
// Z07 was the product. These three are the explanation, and they are
// drawings: the film's own [`shell`], which can be exploded, re-framed and
// recoloured on a beat in a way a running application cannot. Every pixel
// of them still comes out of vieww's rasterizer — they are not a different
// medium, only a different register.

use super::shell::{self as sh, Kind as DevKind, Line, Preview};

/// The program the explanation scenes show — the same counter/inbox the
/// real session opens in Z07, drawn as tokens so the shape of the code is
/// legible at 1920×1080 without asking the audience to read it.
const PROGRAM: [Line; 11] = [
    Line(0.0, &[(58.0, 0), (96.0, 1)]),
    Line(1.0, &[(64.0, 6), (118.0, 2)]),
    Line(1.0, &[(52.0, 6), (72.0, 3)]),
    Line(0.0, &[]),
    Line(0.0, &[(58.0, 0), (84.0, 1), (40.0, 6)]),
    Line(1.0, &[(64.0, 6), (150.0, 2)]),
    Line(1.0, &[(46.0, 6), (196.0, 2)]),
    Line(1.0, &[(62.0, 5), (110.0, 2)]),
    Line(0.0, &[]),
    Line(0.0, &[(150.0, 4)]),
    Line(0.0, &[(58.0, 0), (128.0, 1)]),
];

/// The line the live edit lands on, and the line under it.
const EDIT_LINE: usize = 5;
const EDIT_LINE_2: usize = 6;

/// A world-space painting over the whole canvas — the scenes' one door
/// into the Sketchbook, so every drawing rides the camera together.
fn paint(f: impl Fn(&mut Sketchbook) + Send + Sync + 'static) -> vieww_widget::WidgetNode {
    Positioned::fill()
        .child(Painting::sized(CANVAS, PaintWith::new(move |book: &mut Sketchbook, _s: Size| f(book))))
        .into()
}

// ── Z08 · live_compose ──────────────────────────────────────────────────────

/// **Edit a line, see the picture change.** The shell assembles, one code
/// line retypes itself, the planner's damage rectangle appears over the
/// exactly one region that changed, and the preview repaints inside it —
/// and nowhere else. The whole argument of an incremental renderer is that
/// last clause, and it is the one thing a screen recording cannot show,
/// because the pixels that *didn't* change look identical either way.
pub fn live_compose(ctx: &pf::Ctx) -> vieww_widget::WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let mut stack = Stack::new();
    stack = stack.push(room(t, 0xA17E));
    headline_plate(ctx, "edit a line — see the picture change", "one rectangle repaints, and it is the only one");

    let build = clamp01(sec / 0.85);
    let type_p = clamp01((sec - 2.2) / 1.5);
    let damage_p = clamp01((sec - 3.5) / 0.4) * (1.0 - clamp01((sec - 8.6) / 0.6));
    let repaint = clamp01((sec - 3.9) / 0.5);
    let row_edit = clamp01((sec - 5.6) / 1.3);
    let caret_on = (sec * 2.6).fract() < 0.55;

    // The damage rectangle, in the device's own coordinates: the app bar,
    // and later the first row — the two regions the two edits touch.
    let bar = Rect::new(sh::DEVICE.left + 10.0, sh::DEVICE.top + 26.0, sh::DEVICE.right - 10.0, sh::DEVICE.top + 88.0);
    let row0 = Rect::new(sh::DEVICE.left + 24.0, sh::DEVICE.top + 108.0, sh::DEVICE.right - 24.0, sh::DEVICE.top + 144.0);
    let dmg = if row_edit > 0.02 { row0 } else { bar };

    stack = stack.push(paint(move |book| {
        sh::frame(book, build, 1.0);
        sh::tree(book, clamp01(build * 1.3), 4, 1.0);
        sh::code(
            book,
            &PROGRAM,
            PROGRAM.len(),
            (type_p < 1.0 && type_p > 0.0).then_some((EDIT_LINE, type_p)),
            Some((if row_edit > 0.02 { EDIT_LINE_2 } else { EDIT_LINE }, clamp01((sec - 2.0) / 0.4))),
            caret_on,
            1.0,
        );
        sh::device(
            book,
            sh::DEVICE,
            DevKind::Phone,
            Preview::List { rows: 5, title_lit: repaint > 0.5, row_lit: row_edit > 0.5 },
            ACCENT,
            (damage_p > 0.01).then_some((dmg, damage_p * (0.6 + 0.4 * (sec * 6.0).sin().abs()))),
            clamp01((sec - 0.5) / 0.6),
            1.0,
        );
        // The thread: the edited line to the rectangle it repaints. This
        // is the scene's whole claim drawn as a path — the edit does not
        // reach the app, it reaches *one rectangle of it*.
        let thread = clamp01((sec - 3.2) / 0.5);
        if thread > 0.01 {
            let from = Offset::new(sh::CODE_X + 200.0, sh::CODE_Y0 + (if row_edit > 0.02 { EDIT_LINE_2 } else { EDIT_LINE }) as f32 * sh::CODE_DY + 5.0);
            let to = Offset::new(dmg.left + dmg.width() * 0.5, dmg.top + dmg.height() * 0.5);
            let pts = fk::thread_pts(from, to, -120.0);
            fk::grow_stroke(book, &pts, thread, BREAK_RED, 1.6, 0.55);
            fk::rider(book, &pts, (sec * 0.42) % 1.0, BREAK_RED, 3.0, thread * 0.9);
        }
    }));

    // The numbers the claim rests on — the damage report, in the body.
    let rep = clamp01((sec - 4.2) / 0.5);
    if rep > 0.01 {
        for (i, (label, value, color)) in [
            ("rectangles repainted", "1", BREAK_RED),
            ("subtrees rebuilt", "1", ACCENT),
            ("the other 99% of the frame", "untouched", LEDGER),
        ]
        .iter()
        .enumerate()
        {
            let p = ease_out_cubic(clamp01(rep * 1.4 - i as f32 * 0.18));
            if p <= 0.01 {
                continue;
            }
            let y = 700.0 + i as f32 * 42.0;
            stack = stack.push(Positioned::new().left(452.0).top(y + (1.0 - p) * 8.0).width(420.0).height(28.0).child(
                Opacity::new(p).child(Text::new(label.to_string())
                    .style(pf::geist_mono(18.0).letter_spacing(1.2).color(pf::alpha(MUTED, 0.9)))),
            ));
            stack = stack.push(Positioned::new().left(880.0).top(y + (1.0 - p) * 8.0).width(300.0).height(28.0).child(
                Opacity::new(p).child(Text::new(value.to_string())
                    .style(pf::geist_mono(20.0).letter_spacing(1.2).color(pf::alpha(*color, 0.96)))),
            ));
        }
    }

    pf::chrome(pf::chip_row(
        &[("no relaunch", ACCENT), ("no reload", SYN_TYPE), ("no full-screen repaint", LEDGER)],
        BEAT_X,
        924.0,
        clamp01((t - 0.55) / 0.20),
    ));
    pf::caption("no relaunch. no reload. the line lands while you watch.", 1002.0, clamp01((t - 0.10) / 0.12));
    pf::caption("what you see repaint itself is exactly what the planner repainted.", 966.0, clamp01((t - 0.58) / 0.12));
    stack.into()
}

// ── Z09 · say_to_rust ───────────────────────────────────────────────────────

/// **`.say` → rust → cdylib → pixels.** The editor's program travels: it
/// leaves the buffer as a packet, crosses six stations of the real
/// toolchain, and arrives in the device as a running counter that then
/// takes two presses. The pipeline is the scene's subject, so it owns the
/// middle of the body and the shell steps back to make room for it.
pub fn say_to_rust(ctx: &pf::Ctx) -> vieww_widget::WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let mut stack = Stack::new();
    stack = stack.push(room(t, 0x59A7));
    headline_plate(ctx, ".say → rust → cdylib → preview", "one file of english — a real widget tree");

    const STATIONS: [&str; 6] = [".say", "codegen", "rustc", "cdylib", "dlopen", "pixels"];
    let lit_at: [f32; 6] = [1.0, 2.0, 3.1, 4.0, 4.8, 5.6];
    let xs: [f32; 6] = [300.0, 588.0, 876.0, 1164.0, 1452.0, 1740.0];
    let y_line = 470.0;

    // The presses — the film's own, and labelled as the film's own.
    let presses: [f32; 2] = [8.0, 9.4];
    let count = presses.iter().filter(|p| sec >= **p).count() as u32;
    let press_env = presses
        .iter()
        .map(|p| clamp01(1.0 - (sec - p) / 0.45) * if sec >= *p { 1.0 } else { 0.0 })
        .fold(0.0f32, f32::max);

    // The thread through the stations, and the packet riding it.
    let mut pts: Vec<Offset> = Vec::new();
    for i in 0..6 {
        let node = Offset::new(xs[i], y_line + if i % 2 == 0 { -26.0 } else { 26.0 });
        if pts.is_empty() {
            pts.push(node);
        } else {
            let prev = Offset::new(xs[i - 1], y_line + if (i - 1) % 2 == 0 { -26.0 } else { 26.0 });
            pts.extend(fk::thread_pts(prev, node, if i % 2 == 1 { 40.0 } else { -40.0 }).into_iter().skip(1));
        }
    }

    let build = clamp01(sec / 0.7);
    stack = stack.push(paint(move |book| {
        // The shell, reduced to its two ends: the buffer the program
        // leaves from, and the device it arrives in.
        let editor = Rect::new(118.0, 326.0, 560.0, 690.0);
        let a = ease_out_expo(build);
        book.rrect(editor, 14.0, pf::alpha(Color::rgb(0x11, 0x0F, 0x14), 0.98 * a));
        book.stroke_rrect(editor, 14.0, pf::alpha(Color::WHITE, 0.07 * a), 1.0);
        for i in 0..7 {
            let w = [120.0f32, 168.0, 96.0, 184.0, 140.0, 108.0, 152.0][i];
            let rp = clamp01(build * 1.6 - i as f32 * 0.08);
            book.rrect(
                pf::xywh(editor.left + 28.0 + if i % 3 == 1 { 16.0 } else { 0.0 }, editor.top + 44.0 + i as f32 * 30.0, w * rp, 9.0),
                4.5,
                pf::alpha(sh::tok_color((i % 6) as u8), 0.72 * a),
            );
        }

        let carrier = clamp01((sec - 0.7) / 0.7);
        if carrier > 0.01 {
            fk::grow_stroke(book, &pts, carrier, BRAND_NEAR, 2.2, 0.55 * carrier);
        }
        let flow = clamp01((sec - 1.1) / 0.5);
        if flow > 0.01 {
            fk::flow_along(book, &pts, sec * 0.9, SYN_TYPE, 0.55 * flow, 1.8, 0.09);
            fk::rider(book, &pts, ((sec - 1.1) / 5.0).clamp(0.0, 1.0), ACCENT, 4.0, flow);
        }
        for i in 0..6 {
            let since = sec - lit_at[i];
            let arrive = clamp01((sec - lit_at[i] + 0.35) / 0.35);
            if arrive <= 0.01 {
                continue;
            }
            let lit = if since > 0.0 { (0.5 + 0.5 * (-since * 2.0).exp()).min(1.0) } else { 0.30 };
            let (nx, ny) = (xs[i], y_line + if i % 2 == 0 { -26.0 } else { 26.0 });
            if lit > 0.5 {
                pf::glow(book, nx, ny, 66.0, ACCENT, (lit - 0.5) * 0.5);
            }
            let c = pf::xywh(nx - 92.0, ny - 22.0, 184.0, 44.0);
            book.rrect(c, 12.0, pf::alpha(Color::rgb(0x14, 0x12, 0x19), 0.96 * arrive));
            book.stroke_rrect(c, 12.0, pf::alpha(if since > 0.0 { ACCENT } else { pf::FAINT }, (0.24 + 0.56 * lit) * arrive), 1.3);
            book.circle(Offset::new(nx - 70.0, ny), 3.6, pf::alpha(if since > 0.0 { ACCENT } else { pf::FAINT }, 0.9 * lit * arrive));
        }

        // The device, arriving with the last station — the pipeline's
        // output is a running program, so the program is what lands.
        let dev = Rect::new(1560.0, 560.0, 1800.0, 874.0);
        sh::device(
            book,
            dev,
            DevKind::Phone,
            Preview::Counter { count, pressed: press_env },
            ACCENT,
            None,
            clamp01((sec - 5.4) / 0.6),
            1.0,
        );
    }));

    // The station names — widgets, at the stations' own coordinates.
    for (i, name) in STATIONS.iter().enumerate() {
        let a = ease_out_cubic(clamp01((sec - lit_at[i] + 0.35) / 0.35));
        if a <= 0.01 {
            continue;
        }
        let ny = y_line + if i % 2 == 0 { -26.0 } else { 26.0 };
        stack = stack.push(
            Positioned::new().left(xs[i] - 56.0).top(ny - 12.0).width(150.0).height(26.0).child(
                Opacity::new(a).child(Text::new(name.to_string()).style(
                    pf::geist_mono(17.0).letter_spacing(1.2).color(pf::alpha(if sec > lit_at[i] { INK } else { MUTED }, 0.96)),
                )),
            ),
        );
    }

    // The counter's numeral — the one number in the act, set as type.
    let dev_a = clamp01((sec - 5.6) / 0.5);
    if dev_a > 0.01 {
        stack = stack.push(
            Positioned::new().left(1560.0).top(672.0).width(240.0).height(90.0).child(
                Opacity::new(dev_a).child(
                    Text::new(format!("{count}"))
                        .style(pf::geist(64.0).bold().color(pf::alpha(INK, 0.98)))
                        .align(TextAlign::Center),
                ),
            ),
        );
    }

    // The scene's closing line, in the body where it belongs.
    let close = ease_out_cubic(clamp01((sec - 6.2) / 0.5));
    if close > 0.01 {
        stack = stack.push(Positioned::new().left(300.0).top(760.0).width(1180.0).height(34.0).child(
            Opacity::new(close).child(Text::new(
                "english in, native widgets out — no bridge, no interpreter, no webview.".to_string(),
            )
            .style(pf::geist_mono(19.0).letter_spacing(1.2).color(pf::alpha(SYN_STRING, 0.92)))),
        ));
    }

    pf::chrome(pf::chip_row(
        &[("counter.say → counter.rs", SYN_MACRO), ("rustc, not a runtime", SYN_TYPE), ("the screen is the counter", LEDGER)],
        BEAT_X,
        924.0,
        clamp01((t - 0.60) / 0.20),
    ));
    pf::caption("the language compiles to the framework — no bridge, no interpreter.", 1002.0, clamp01((t - 0.12) / 0.12));
    pf::caption("and the counter counts, because the button is the tree.", 966.0, clamp01((t - 0.64) / 0.12));
    stack.into()
}

// ── Z10 · ships_everywhere ──────────────────────────────────────────────────

/// **One tree, every device.** The preview pane's single device divides
/// into three — android, ios, desktop — each re-framed rather than
/// re-written: same rows, same tree, different chrome and metrics. Then
/// one token changes and all three re-tint at once, because a brand here
/// is a value and not a fork.
pub fn ships_everywhere(ctx: &pf::Ctx) -> vieww_widget::WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let mut stack = Stack::new();
    stack = stack.push(room(t, 0x5417));
    headline_plate(ctx, "one tree. every device.", "platforms and tokens, not forks");

    // The token flip — teal, then the brand's purple, on all three at once.
    let teal_at = 5.6;
    let purple_at = 6.8;
    let accent = if sec >= purple_at {
        ACCENT
    } else if sec >= teal_at {
        Color::rgb(0x2F, 0xBF, 0xAE)
    } else {
        ACCENT
    };

    let slabs: [(f32, f32, DevKind, &str, f32); 3] = [
        (300.0, -0.24, DevKind::Phone, "android", 1.0),
        (860.0, 0.0, DevKind::Phone, "ios", 2.0),
        (1420.0, 0.24, DevKind::Window, "desktop", 3.0),
    ];

    // The one tree they all share — drawn once, above the fleet, with a
    // thread down to each device. The picture of the claim.
    let tree_p = clamp01((sec - 0.3) / 0.6);
    stack = stack.push(paint(move |book| {
        let root = Offset::new(960.0, 372.0);
        if tree_p > 0.01 {
            let a = ease_out_expo(tree_p);
            book.rrect(pf::xywh(root.dx - 150.0, root.dy - 26.0, 300.0, 52.0), 13.0,
                       pf::alpha(Color::rgb(0x14, 0x12, 0x19), 0.96 * a));
            book.stroke_rrect(pf::xywh(root.dx - 150.0, root.dy - 26.0, 300.0, 52.0), 13.0,
                              pf::alpha(BRAND_NEAR, 0.5 * a), 1.3);
            book.circle(Offset::new(root.dx - 126.0, root.dy), 4.0, pf::alpha(BRAND_NEAR, 0.95 * a));
        }
        for (x, yaw, kind, _name, at) in slabs {
            let rise_p = clamp01((sec - at) / 0.75);
            if rise_p <= 0.01 {
                continue;
            }
            let rise = ease_out_cubic(rise_p);
            let (w, h) = match kind {
                DevKind::Phone => (230.0, 344.0),
                DevKind::Window => (396.0, 262.0),
            };
            let cy = match kind {
                DevKind::Phone => 468.0,
                DevKind::Window => 508.0,
            };
            let r = Rect::new(x - w * 0.5, cy + (1.0 - rise) * 26.0, x + w * 0.5, cy + h + (1.0 - rise) * 26.0);
            // Each slab in true perspective — the fleet has depth, and
            // the depth is the framework's own projector, not a skew.
            pf::panel_3d(book, r, yaw, 0.09, 1250.0, rise, |b| {
                sh::device(
                    b,
                    r,
                    kind,
                    Preview::List { rows: if kind == DevKind::Window { 4 } else { 5 }, title_lit: true, row_lit: false },
                    accent,
                    None,
                    1.0,
                    1.0,
                );
            });
            // The thread from the one tree to this device.
            let thread = clamp01((rise_p - 0.4) * 2.2);
            if thread > 0.01 {
                let pts = fk::thread_pts(root_of(), Offset::new(x, cy + 6.0), if x < 900.0 { -140.0 } else if x > 1100.0 { 140.0 } else { 0.0 });
                fk::grow_stroke(book, &pts, thread, BRAND_NEAR, 1.5, 0.45 * thread);
                fk::rider(book, &pts, (sec * 0.3 + x * 0.0004) % 1.0, accent, 2.8, thread * 0.9);
            }
        }
    }));

    // The token flood — one wash, when the accent actually changes.
    let flood_p = clamp01((sec - purple_at) / 0.9);
    if flood_p > 0.001 && flood_p < 0.999 {
        stack = stack.push(paint(move |book| {
            fk::token_flood(book, W, 1080.0, flood_p, BRAND_NEAR);
        }));
    }

    // The fleet's names, and the token line under them.
    for (x, _, _, name, at) in slabs {
        let a = ease_out_cubic(clamp01((sec - at - 0.45) / 0.4));
        if a <= 0.01 {
            continue;
        }
        stack = stack.push(Positioned::new().left(x - 170.0).top(838.0).width(340.0).height(28.0).child(
            Opacity::new(a).child(Text::new(name.to_string())
                .style(pf::geist_mono(17.0).letter_spacing(3.0).color(pf::alpha(INK, 0.92)))
                .align(TextAlign::Center)),
        ));
    }
    let tok_a = ease_out_cubic(clamp01((sec - 4.6) / 0.5));
    if tok_a > 0.01 {
        let line = if sec >= purple_at {
            "tokens.accent = purple   —   all three, one value"
        } else if sec >= teal_at {
            "tokens.accent = teal   —   all three, one value"
        } else {
            "tokens.accent — one value, every device"
        };
        stack = stack.push(Positioned::new().left(0.0).top(334.0).width(W).height(30.0).child(
            Opacity::new(tok_a).child(Text::new(line.to_string())
                .style(pf::geist_mono(17.0).letter_spacing(1.6).color(pf::alpha(accent, 0.94)))
                .align(TextAlign::Center)),
        ));
    }

    pf::chrome(pf::chip_row(
        &[("3 platforms, 0 forks", ACCENT), ("1 line to a new brand", LEDGER)],
        BEAT_X,
        924.0,
        clamp01((t - 0.66) / 0.20),
    ));
    pf::caption("the same widget tree — re-framed, not re-written.", 1002.0, clamp01((t - 0.12) / 0.12));
    pf::caption("a brand is a token file, and the studio edits tokens.", 966.0, clamp01((t - 0.58) / 0.12));
    stack.into()
}

/// The one tree's anchor — the point every device's thread leaves from.
fn root_of() -> Offset {
    Offset::new(960.0, 398.0)
}

/// The explanation scenes' ground — the film's register, not the app's.
/// A flat dark room with the faintest grid, so the drawings above it have
/// something to sit on and the body band reads as a stage.
fn room(t: f32, seed: u64) -> vieww_widget::WidgetNode {
    Positioned::fill()
        .child(Painting::sized(CANVAS, PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                vieww_foundation::Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(13, 12, 16)),
                    (0.55, pf::BG_DEEP),
                    (1.0, Color::rgb(10, 9, 13)),
                ]),
            );
            // The stage grid — inside the body band only, and faint
            // enough to be felt rather than read.
            for k in 0..17 {
                let x = 118.0 + k as f32 * 105.25;
                book.rect(Rect::new(x - 0.5, 326.0, x + 0.5, 874.0), pf::alpha(Color::WHITE, 0.018));
            }
            pf::stars(book, w, h, seed, 40, t, 0.05);
            pf::vignette(book, w, h, 0.5);
        })))
        .into()
}

/// Keep the lint honest about the imports the act still carries.
#[allow(unused)]
fn _unused() {
    let _ = (TAP_LIVE_ROW, TAP_ADD_ONE, region::PREVIEW, STAGE, BEAT_Y, BEAT_STEP, stage, beat_chip, callout_label, typed);
}

/// A mono string typed to `frac` of its length — chars, never bytes.
fn typed(text: &str, frac: f32) -> String {
    let chars: Vec<char> = text.chars().collect();
    let n = ((chars.len() as f32 * frac.clamp(0.0, 1.0)).round()) as usize;
    chars[..n.min(chars.len())].iter().collect()
}
