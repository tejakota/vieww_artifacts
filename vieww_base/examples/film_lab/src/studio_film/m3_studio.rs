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

use crate::film_lib::{clamp01, ease_in_out, ease_out_cubic, ease_out_expo};
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
                .height(60.0)
                .child(
                    Text::new(text.to_string())
                        .style(pf::geist(46.0).bold().letter_spacing(1.6).color(pf::alpha(INK, 0.97 * a)))
                        .align(TextAlign::Center),
                ),
        )
        .push(
            Positioned::new()
                .left(0.0)
                .top(62.0)
                .width(pf::W)
                .height(30.0)
                .child(
                    Text::new(sub.to_string())
                        .style(pf::geist_mono(19.0).letter_spacing(2.4).color(pf::alpha(ENGINE, 0.92 * a)))
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
            .top(186.0 + (1.0 - ease_out_expo(pf::clamp01((ctx.t - 0.04) / 0.10))) * 12.0)
            .width(pf::W)
            .height(94.0)
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
pub const STAGE: Rect = Rect { left: 360.0, top: 316.0, right: 1560.0, bottom: 684.0 };

/// The beat column's origin — left of the stage, under it, clear of
/// both the headline and the receipts row.
const BEAT_X: f32 = 84.0;
const BEAT_Y: f32 = 726.0;
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
                .top(318.0 + rise)
                .width(W)
                .height(84.0)
                .child(Opacity::new(mark_a).child(fk::wordmark(58.0, 1.0))),
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
                    Rect::new(64.0, 300.0, W - 64.0, 940.0),
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
    beat_chip("open: counter.say", SYN_TYPE, BEAT_X, BEAT_Y, sec - 0.4);
    beat_chip("panel closes — the editor breathes", MUTED, BEAT_X, BEAT_Y + BEAT_STEP, sec - 3.6);
    beat_chip("panel returns — the tree is the map", MUTED, BEAT_X, BEAT_Y + BEAT_STEP * 2.0, sec - 6.2);
    beat_chip("live preview: accepted", SYN_STRING, BEAT_X, BEAT_Y + BEAT_STEP * 3.0, sec - 9.4);
    beat_chip("damage overlay: on — one rect, not the screen", BREAK_RED, BEAT_X, BEAT_Y + BEAT_STEP * 4.0, sec - 10.6);
    beat_chip("edit lands while you watch", ACCENT, BEAT_X, BEAT_Y + BEAT_STEP * 5.0, sec - 11.8);

    // The receipts.
    pf::chrome(pf::chip_row(
        &[
            ("the real Shell", ACCENT),
            ("the real editor", SYN_TYPE),
            ("the real preview", SYN_STRING),
        ],
        super::MARGIN,
        986.0,
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

    let build = clamp01(sec / 1.05);
    let type_p = clamp01((sec - 3.0) / 2.0);
    let damage_p = clamp01((sec - 4.8) / 0.5) * (1.0 - clamp01((sec - 12.4) / 0.7));
    let repaint = clamp01((sec - 5.3) / 0.6);
    let row_edit = clamp01((sec - 7.8) / 1.6);
    let caret_on = (sec * 2.6).fract() < 0.55;

    // The device: iOS, at the studio's own 393×852. Its height decides
    // its width, so the preview in this film is the shape the product
    // says a phone is — and the damage rectangle is placed against the
    // safe area the platform reports, not against a guess.
    let dev_h = 560.0;
    let (dev_w, _, _, dev_bezel) = Device::IOS.frame_for_height(dev_h);
    let dev_at = Offset::new(1548.0, 332.0);
    let dev_scale = dev_h / 852.0;
    let (ios_top, _) = Device::IOS.insets();
    let safe_top = dev_at.dy + dev_bezel + ios_top * dev_scale;
    let sx0 = dev_at.dx + dev_bezel;
    let sx1 = dev_at.dx + dev_w - dev_bezel;
    let bar = Rect::new(sx0, safe_top, sx1, safe_top + 56.0 * dev_scale);
    let row0 = Rect::new(sx0 + 12.0 * dev_scale, safe_top + 68.0 * dev_scale, sx1 - 12.0 * dev_scale, safe_top + 112.0 * dev_scale);
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
        kit::device(
            book,
            Device::IOS,
            dev_at,
            dev_h,
            Screen::List { rows: 5, title_lit: repaint > 0.5, lit_row: (row_edit > 0.5).then_some(0) },
            ACCENT,
            (damage_p > 0.01).then_some((dmg, damage_p * (0.6 + 0.4 * (sec * 6.0).sin().abs()))),
            clamp01((sec - 0.6) / 0.7),
            1.0,
        );
        // The thread: the edited line to the rectangle it repaints. This
        // is the scene's whole claim drawn as a path — the edit does not
        // reach the app, it reaches *one rectangle of it*.
        let thread = clamp01((sec - 4.4) / 0.6);
        if thread > 0.01 {
            let from = Offset::new(sh::CODE_X + 220.0, sh::CODE_Y0 + (if row_edit > 0.02 { EDIT_LINE_2 } else { EDIT_LINE }) as f32 * sh::CODE_DY + 5.0);
            let to = Offset::new(dmg.left + dmg.width() * 0.5, dmg.top + dmg.height() * 0.5);
            let pts = fk::thread_pts(from, to, -120.0);
            fk::grow_stroke(book, &pts, thread, BREAK_RED, 1.6, 0.55);
            fk::rider(book, &pts, (sec * 0.42) % 1.0, BREAK_RED, 3.0, thread * 0.9);
        }
    }));

    // The numbers the claim rests on — the damage report, in the body.
    let rep = clamp01((sec - 5.8) / 0.6);
    if rep > 0.01 {
        for (i, (label, value, color)) in [
            ("rectangles repainted", "1", BREAK_RED),
            ("subtrees rebuilt", "1", ACCENT),
            ("the other 99% of the frame", "untouched", LEDGER),
        ]
        .iter()
        .enumerate()
        {
            let p = ease_out_cubic(clamp01(rep * 1.4 - i as f32 * 0.16));
            if p <= 0.01 {
                continue;
            }
            let y = 776.0 + i as f32 * 46.0;
            stack = stack.push(Positioned::new().left(468.0).top(y + (1.0 - p) * 8.0).width(520.0).height(30.0).child(
                Opacity::new(p).child(Text::new(label.to_string())
                    .style(pf::geist_mono(18.0).letter_spacing(1.2).color(pf::alpha(MUTED, 0.9)))),
            ));
            stack = stack.push(Positioned::new().left(1020.0).top(y + (1.0 - p) * 8.0).width(330.0).height(30.0).child(
                Opacity::new(p).child(Text::new(value.to_string())
                    .style(pf::geist_mono(20.0).letter_spacing(1.2).color(pf::alpha(*color, 0.96)))),
            ));
        }
    }

    pf::chrome(pf::chip_row(
        &[("no relaunch", ACCENT), ("no reload", SYN_TYPE), ("no full-screen repaint", LEDGER)],
        super::MARGIN,
        986.0,
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
    let lit_at: [f32; 6] = [1.4, 2.8, 4.4, 5.6, 6.6, 7.6];
    let xs: [f32; 6] = [318.0, 604.0, 890.0, 1176.0, 1462.0, 1748.0];
    let y_line = 452.0;

    // The presses — the film's own, and labelled as the film's own.
    let presses: [f32; 2] = [10.4, 12.0];
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

    let build = clamp01(sec / 0.9);
    stack = stack.push(paint(move |book| {
        // The shell, reduced to its two ends: the buffer the program
        // leaves from, and the device it arrives in.
        let editor = Rect::new(110.0, 300.0, 580.0, 700.0);
        let a = ease_out_expo(build);
        book.rrect(editor, 14.0, pf::alpha(Color::rgb(0x11, 0x0F, 0x14), 0.98 * a));
        book.stroke_rrect(editor, 14.0, pf::alpha(Color::WHITE, 0.07 * a), 1.0);
        for i in 0..7 {
            let w = [120.0f32, 168.0, 96.0, 184.0, 140.0, 108.0, 152.0][i];
            let rp = clamp01(build * 1.6 - i as f32 * 0.08);
            book.rrect(
                pf::xywh(editor.left + 28.0 + if i % 3 == 1 { 16.0 } else { 0.0 }, editor.top + 56.0 + i as f32 * 38.0, w * rp, 9.0),
                4.5,
                pf::alpha(sh::tok_color((i % 6) as u8), 0.72 * a),
            );
        }

        let carrier = clamp01((sec - 0.9) / 0.9);
        if carrier > 0.01 {
            fk::grow_stroke(book, &pts, carrier, BRAND_NEAR, 2.2, 0.55 * carrier);
        }
        let flow = clamp01((sec - 1.4) / 0.6);
        if flow > 0.01 {
            fk::flow_along(book, &pts, sec * 0.9, SYN_TYPE, 0.55 * flow, 1.8, 0.09);
            fk::rider(book, &pts, ((sec - 1.4) / 6.4).clamp(0.0, 1.0), ACCENT, 4.0, flow);
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
        let dev_h = 372.0;
        let (dev_w, _, _, _) = Device::IOS.frame_for_height(dev_h);
        kit::device(
            book,
            Device::IOS,
            Offset::new(1690.0 - dev_w * 0.5, 566.0),
            dev_h,
            Screen::Counter { count, pressed: press_env },
            ACCENT,
            None,
            clamp01((sec - 7.4) / 0.7),
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
    let dev_a = clamp01((sec - 7.6) / 0.6);
    if dev_a > 0.01 {
        stack = stack.push(
            Positioned::new().left(1560.0).top(632.0).width(260.0).height(90.0).child(
                Opacity::new(dev_a).child(
                    Text::new(format!("{count}"))
                        .style(pf::geist(64.0).bold().color(pf::alpha(INK, 0.98)))
                        .align(TextAlign::Center),
                ),
            ),
        );
    }

    // The scene's closing line, in the body where it belongs.
    let close = ease_out_cubic(clamp01((sec - 13.2) / 0.6));
    if close > 0.01 {
        stack = stack.push(Positioned::new().left(320.0).top(820.0).width(1300.0).height(36.0).child(
            Opacity::new(close).child(Text::new(
                "english in, native widgets out — no bridge, no interpreter, no webview.".to_string(),
            )
            .style(pf::geist_mono(19.0).letter_spacing(1.2).color(pf::alpha(SYN_STRING, 0.92)))),
        ));
    }

    pf::chrome(pf::chip_row(
        &[("counter.say → counter.rs", SYN_MACRO), ("rustc, not a runtime", SYN_TYPE), ("the screen is the counter", LEDGER)],
        super::MARGIN,
        986.0,
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
    let teal_at = 8.0;
    let purple_at = 10.0;
    let accent = if sec >= purple_at {
        ACCENT
    } else if sec >= teal_at {
        Color::rgb(0x2F, 0xBF, 0xAE)
    } else {
        ACCENT
    };

    // The fleet, quoted: android is 412×915, ios 393×852, desktop
    // 1280×800 — the studio's own `Platform::screen`. Three devices that
    // are three different shapes, because they are.
    let slabs: [(f32, f32, Device, &str, f32); 3] = [
        (376.0, -0.24, Device::ANDROID, "android", 1.4),
        (760.0, 0.0, Device::IOS, "ios", 3.0),
        (1420.0, 0.22, Device::DESKTOP, "desktop", 4.6),
    ];

    // The one tree they all share — drawn once, above the fleet, with a
    // thread down to each device. The picture of the claim.
    let tree_p = clamp01((sec - 0.4) / 0.7);
    stack = stack.push(paint(move |book| {
        let root = Offset::new(960.0, 336.0);
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
            // One height for the phones, a shorter one for the window —
            // and the width follows from each platform's own screen, so
            // the three read as three devices and not three rectangles.
            let h = if kind == Device::DESKTOP { 372.0 } else { 478.0 };
            let (w, hh, _, _) = kind.frame_for_height(h);
            let cy = if kind == Device::DESKTOP { 452.0 } else { 388.0 };
            let at = Offset::new(x - w * 0.5, cy + (1.0 - rise) * 26.0);
            let r = Rect::new(at.dx, at.dy, at.dx + w, at.dy + hh);
            // Each device in true perspective — the fleet has depth, and
            // the depth is the framework's own projector, not a skew.
            pf::panel_3d(book, r, yaw, 0.09, 1250.0, rise, |b| {
                kit::device(
                    b,
                    kind,
                    at,
                    h,
                    Screen::List { rows: if kind == Device::DESKTOP { 4 } else { 5 }, title_lit: true, lit_row: None },
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
        stack = stack.push(Positioned::new().left(x - 200.0).top(922.0).width(400.0).height(26.0).child(
            Opacity::new(a * 0.8).child(Text::new(kind_of(name).describe())
                .style(pf::geist_mono(14.0).letter_spacing(1.2).color(pf::alpha(MUTED, 0.85)))
                .align(TextAlign::Center)),
        ));
        stack = stack.push(Positioned::new().left(x - 200.0).top(892.0).width(400.0).height(30.0).child(
            Opacity::new(a).child(Text::new(name.to_string())
                .style(pf::geist_mono(19.0).letter_spacing(3.0).color(pf::alpha(INK, 0.94)))
                .align(TextAlign::Center)),
        ));
    }
    let tok_a = ease_out_cubic(clamp01((sec - 6.6) / 0.6));
    if tok_a > 0.01 {
        let line = if sec >= purple_at {
            "tokens.accent = purple   —   all three, one value"
        } else if sec >= teal_at {
            "tokens.accent = teal   —   all three, one value"
        } else {
            "tokens.accent — one value, every device"
        };
        stack = stack.push(Positioned::new().left(0.0).top(300.0).width(W).height(32.0).child(
            Opacity::new(tok_a).child(Text::new(line.to_string())
                .style(pf::geist_mono(17.0).letter_spacing(1.6).color(pf::alpha(accent, 0.94)))
                .align(TextAlign::Center)),
        ));
    }

    pf::chrome(pf::chip_row(
        &[("3 platforms, 0 forks", ACCENT), ("1 line to a new brand", LEDGER)],
        super::MARGIN,
        986.0,
        clamp01((t - 0.66) / 0.20),
    ));
    pf::caption("the same widget tree — re-framed, not re-written.", 1002.0, clamp01((t - 0.12) / 0.12));
    pf::caption("a brand is a token file, and the studio edits tokens.", 966.0, clamp01((t - 0.58) / 0.12));
    stack.into()
}

/// The device a fleet name stands for — so the caption under each device
/// is the studio's own `Platform::describe`, not a second copy of the
/// numbers that could drift from it.
fn kind_of(name: &str) -> Device {
    match name {
        "android" => Device::ANDROID,
        "ios" => Device::IOS,
        _ => Device::DESKTOP,
    }
}

/// The one tree's anchor — the point every device's thread leaves from.
fn root_of() -> Offset {
    Offset::new(960.0, 356.0)
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

use super::kit::{self, Device, Ico, Screen};

// ── Z12 · the_shell ─────────────────────────────────────────────────────────

/// **One shell: editor, preview, build, ship.** Z11 showed the studio;
/// this scene takes it apart. The five regions separate, name themselves
/// with the studio's own icons, and close again — an exploded view, which
/// is the one drawing a running application cannot give you.
///
/// Every icon here is `viewwstudio::ui::icons`: the same `IconData` the
/// real activity bar asks for, fitted to its box, so the film's diagram
/// and the product's chrome cannot disagree about what a Preview icon
/// looks like.
pub fn the_shell(ctx: &pf::Ctx) -> vieww_widget::WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let mut stack = Stack::new();
    stack = stack.push(room(t, 0x5E11));
    headline_plate(ctx, "one shell — taken apart", "editor, preview, build, ship");

    /// (icon, name, what it is, colour, the fraction of width it owns)
    const REGIONS: [(Ico, &str, &str, Color); 5] = [
        (Ico::Dashboard, "activity", "every tool, one rail", SYN_TYPE),
        (Ico::Folder, "explorer", "the project, as a tree", ACCENT),
        (Ico::Code, "editor", "real code, real language server", BRAND_NEAR),
        (Ico::Preview, "preview", "the tree, running", LEDGER),
        (Ico::Warning, "panel", "problems, output, builds", SYN_MACRO),
    ];
    // The explode: closed → open → closed, so the shell is seen whole at
    // both ends and apart in the middle.
    let explode = {
        let a = clamp01((sec - 1.4) / 1.6);
        let b = clamp01((sec - 9.4) / 1.8);
        ease_in_out(a) * (1.0 - ease_in_out(b))
    };
    let build = clamp01(sec / 0.9);

    // The five columns' closed geometry, summing to the shell's width.
    const SHELL_X0: f32 = 130.0;
    const SHELL_X1: f32 = 1790.0;
    const SHARE: [f32; 5] = [0.055, 0.155, 0.40, 0.26, 0.13];
    const TOP: f32 = 302.0;
    const BOT: f32 = 878.0;

    stack = stack.push(paint(move |book| {
        let total = SHELL_X1 - SHELL_X0;
        let gap = 34.0 * explode;
        let widened = total - gap * 4.0;
        let mut x = SHELL_X0;
        for (i, (ico, _, _, color)) in REGIONS.iter().enumerate() {
            let p = ease_out_expo(clamp01(build * 1.5 - i as f32 * 0.08));
            if p <= 0.01 {
                continue;
            }
            let w = widened * SHARE[i];
            // Each pane lifts by its own amount so the exploded stack
            // reads as depth rather than as a row that grew gaps.
            let lift = explode * [0.0f32, 14.0, 30.0, 14.0, 0.0][i];
            let r = Rect::new(x, TOP - lift, x + w, BOT - lift);
            book.shadow(
                Rect::new(r.left, r.top + 8.0, r.right, r.bottom + 8.0),
                14.0,
                vieww_foundation::Shadow::new(pf::alpha(Color::BLACK, 0.5 * p), Offset::new(0.0, 12.0), 30.0),
            );
            book.rrect(r, 14.0, pf::alpha(Color::rgb(0x12, 0x10, 0x16), 0.985 * p));
            book.stroke_rrect(r, 14.0, pf::alpha(*color, (0.18 + 0.32 * explode) * p), 1.2);

            // The pane's own content, sketched at the pane's own scale.
            match i {
                0 => {
                    for k in 0..6 {
                        kit::icon_tile(
                            book,
                            [Ico::Folder, Ico::Search, Ico::Code, Ico::Warning, Ico::Dashboard, Ico::Gear][k],
                            Offset::new(r.left + r.width() * 0.5, r.top + 44.0 + k as f32 * 54.0),
                            38.0,
                            *color,
                            k == 0,
                            p,
                        );
                    }
                }
                1 => {
                    for k in 0..9 {
                        let y = r.top + 40.0 + k as f32 * 34.0;
                        let indent = if k % 3 == 0 { 0.0 } else { 18.0 };
                        kit::icon_at(
                            book,
                            if k % 3 == 0 { Ico::Folder } else { Ico::File },
                            Offset::new(r.left + 26.0 + indent, y),
                            17.0,
                            if k == 4 { *color } else { MUTED },
                            1.6,
                            p * if k == 4 { 1.0 } else { 0.7 },
                        );
                        book.rrect(
                            pf::xywh(r.left + 42.0 + indent, y - 4.0, (68.0 + ((k * 53) % 90) as f32).min(r.width() - 70.0 - indent), 8.0),
                            4.0,
                            pf::alpha(if k == 4 { INK } else { MUTED }, (if k == 4 { 0.9 } else { 0.4 }) * p),
                        );
                    }
                }
                2 => {
                    for k in 0..11 {
                        let y = r.top + 38.0 + k as f32 * 30.0;
                        book.rect(pf::xywh(r.left + 18.0, y + 4.0, 16.0, 2.0), pf::alpha(pf::FAINT, 0.35 * p));
                        let mut cx = r.left + 52.0 + if k % 4 == 1 { 22.0 } else { 0.0 };
                        for seg in 0..(2 + k % 3) {
                            let sw = 52.0 + ((k * 31 + seg * 17) % 120) as f32;
                            if cx + sw > r.right - 24.0 {
                                break;
                            }
                            book.rrect(pf::xywh(cx, y, sw, 9.0), 4.5,
                                       pf::alpha(super::shell::tok_color(((k + seg) % 6) as u8), 0.78 * p));
                            cx += sw + 9.0;
                        }
                    }
                }
                3 => {
                    kit::device(
                        book,
                        Device::IOS,
                        Offset::new(r.left + (r.width() - (BOT - TOP - 96.0) * 393.0 / 852.0) * 0.5, r.top + 48.0),
                        BOT - TOP - 96.0,
                        Screen::List { rows: 4, title_lit: true, lit_row: None },
                        ACCENT,
                        None,
                        p,
                        p,
                    );
                }
                _ => {
                    for k in 0..7 {
                        let y = r.top + 40.0 + k as f32 * 32.0;
                        kit::icon_at(
                            book,
                            [Ico::Check, Ico::Warning, Ico::Error, Ico::Play, Ico::Branch, Ico::Save, Ico::Export][k],
                            Offset::new(r.left + 24.0, y),
                            15.0,
                            if k == 0 { LEDGER } else { MUTED },
                            1.6,
                            0.85 * p,
                        );
                        book.rrect(pf::xywh(r.left + 40.0, y - 4.0, (r.width() - 62.0) * (0.5 + 0.07 * k as f32).min(1.0), 7.0), 3.5,
                                   pf::alpha(MUTED, 0.42 * p));
                    }
                }
            }

            // The pane's badge — the icon and its name, only once the
            // shell is apart, because a closed shell should read as one
            // object and not as five labelled ones.
            if explode > 0.04 {
                let by = r.bottom + 18.0;
                kit::icon_at(book, *ico, Offset::new(r.left + 22.0, by + 13.0), 20.0, *color, 1.8, explode * p);
            }
            x += w + gap;
        }
    }));

    // The names, under each pane, on one baseline.
    if explode > 0.05 {
        let total = SHELL_X1 - SHELL_X0;
        let gap = 34.0 * explode;
        let widened = total - gap * 4.0;
        let mut x = SHELL_X0;
        for (i, (_, name, what, color)) in REGIONS.iter().enumerate() {
            let w = widened * SHARE[i];
            let a = ease_out_cubic(clamp01(explode * 1.6 - i as f32 * 0.06));
            if a > 0.01 {
                stack = stack.push(Positioned::new().left(x + 40.0).top(892.0).width(w).height(30.0).child(
                    Opacity::new(a).child(Text::new((*name).to_string())
                        .style(pf::geist_mono(18.0).letter_spacing(2.0).color(pf::alpha(*color, 0.96)))),
                ));
                stack = stack.push(Positioned::new().left(x + 40.0).top(922.0).width(w + 40.0).height(26.0).child(
                    Opacity::new(a * 0.85).child(Text::new((*what).to_string())
                        .style(pf::geist_mono(14.0).letter_spacing(0.9).color(pf::alpha(MUTED, 0.85)))),
                ));
            }
            x += w + gap;
        }
    }

    pf::chrome(pf::chip_row(
        &[("one window", ACCENT), ("no terminal", SYN_TYPE), ("no second tool", LEDGER)],
        super::MARGIN,
        986.0,
        clamp01((t - 0.62) / 0.20),
    ));
    pf::caption("one shell: editor, preview, build, ship.", 1002.0, clamp01((t - 0.06) / 0.10));
    pf::caption("five regions, one window, and nothing else to install.", 966.0, clamp01((t - 0.50) / 0.10));
    stack.into()
}

// ── Z16 · the_inspector ─────────────────────────────────────────────────────

/// **The tree is inspectable, because the tree is real.** A pointer moves
/// over the running preview, and the inspector names what is under it —
/// widget, element, rect, and the cost of the last repaint — the way the
/// studio's own devtools do. Nothing here is a screenshot: the tree on
/// the left and the highlight on the right are the same data.
pub fn the_inspector(ctx: &pf::Ctx) -> vieww_widget::WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let mut stack = Stack::new();
    stack = stack.push(room(t, 0x19FE));
    headline_plate(ctx, "the tree is real, so the tree is inspectable", "hover a pixel, name the widget that made it");

    /// The rows of the inspected tree — (depth, name, kind).
    const TREE: [(f32, &str, u8); 9] = [
        (0.0, "Screen", 0),
        (1.0, "Scaffold", 0),
        (2.0, "AppBar", 1),
        (3.0, "Text", 2),
        (2.0, "ListView", 1),
        (3.0, "ListTile", 3),
        (4.0, "Icon", 2),
        (4.0, "Text", 2),
        (3.0, "ListTile", 3),
    ];
    /// Which row the pointer is over, per beat — the hover is scripted,
    /// and the highlight on the device follows the same index.
    const HOVER: [(f32, usize); 4] = [(2.4, 2), (5.0, 5), (7.6, 6), (10.0, 3)];
    let hovered = HOVER.iter().rev().find(|(at, _)| sec >= *at).map(|(_, i)| *i);
    let hover_since = HOVER.iter().rev().find(|(at, _)| sec >= *at).map(|(at, _)| sec - at).unwrap_or(0.0);

    let build = clamp01(sec / 0.8);
    let dev_h = 470.0;
    let dev_x = 1290.0;
    let dev_y = 350.0;

    stack = stack.push(paint(move |book| {
        // The inspector pane.
        let pane = Rect::new(130.0, 336.0, 1150.0, 872.0);
        let p = ease_out_expo(build);
        book.rrect(pane, 16.0, pf::alpha(Color::rgb(0x11, 0x0F, 0x15), 0.985 * p));
        book.stroke_rrect(pane, 16.0, pf::alpha(Color::WHITE, 0.07 * p), 1.1);
        kit::icon_at(book, Ico::Dashboard, Offset::new(pane.left + 30.0, pane.top + 28.0), 20.0, SYN_TYPE, 1.8, p);

        for (i, (depth, _, kind)) in TREE.iter().enumerate() {
            let rp = ease_out_cubic(clamp01(build * 1.6 - i as f32 * 0.06));
            if rp <= 0.01 {
                continue;
            }
            let y = pane.top + 78.0 + i as f32 * 50.0;
            let x = pane.left + 34.0 + depth * 34.0;
            let on = hovered == Some(i);
            if on {
                book.rrect(pf::xywh(pane.left + 14.0, y - 16.0, pane.width() - 28.0, 42.0), 9.0,
                           pf::alpha(ACCENT, 0.16 * rp));
                book.rect(pf::xywh(pane.left + 14.0, y - 16.0, 3.0, 42.0), pf::alpha(ACCENT, 0.9 * rp));
            }
            // The tree's guide rules — depth is drawn, not implied.
            for d in 0..(*depth as usize) {
                let gx = pane.left + 44.0 + d as f32 * 34.0;
                book.rect(pf::xywh(gx, y - 16.0, 1.0, 42.0), pf::alpha(Color::WHITE, 0.06 * rp));
            }
            kit::icon_at(
                book,
                match kind { 0 => Ico::Panel, 1 => Ico::Split, 2 => Ico::Code, _ => Ico::File },
                Offset::new(x + 10.0, y + 5.0),
                17.0,
                if on { ACCENT } else { MUTED },
                1.6,
                (if on { 1.0 } else { 0.7 }) * rp,
            );
        }

        // The device, and the highlight over the thing being hovered.
        let d = kit::device(
            book,
            Device::IOS,
            Offset::new(dev_x, dev_y),
            dev_h,
            Screen::List { rows: 4, title_lit: false, lit_row: hovered.and_then(|h| match h { 5 | 6 | 7 => Some(0), 8 => Some(1), _ => None }) },
            ACCENT,
            None,
            build,
            1.0,
        );
        if let Some(_h) = hovered {
            // The highlight rect — a real box over the region, with the
            // measuring rules a devtool draws.
            let bloom = clamp01(hover_since / 0.3);
            let hr = match hovered {
                Some(2 | 3) => pf::xywh(d.left + 14.0, d.top + 68.0, d.width() - 28.0, 44.0),
                Some(5 | 6 | 7) => pf::xywh(d.left + 20.0, d.top + 130.0, d.width() - 40.0, 40.0),
                _ => pf::xywh(d.left + 20.0, d.top + 178.0, d.width() - 40.0, 40.0),
            };
            book.rrect(hr, 6.0, pf::alpha(ACCENT, 0.16 * bloom));
            book.stroke_rrect(hr, 6.0, pf::alpha(ACCENT, 0.85 * bloom), 1.4);
            for (x0, y0, x1, y1) in [
                (d.left - 40.0, hr.top, hr.left, hr.top),
                (d.left - 40.0, hr.bottom, hr.left, hr.bottom),
            ] {
                book.line(Offset::new(x0, y0), Offset::new(x1, y1), pf::alpha(ACCENT, 0.35 * bloom), 1.0);
            }
            // The thread from the inspector row to the highlight.
            if let Some(i) = hovered {
                let from = Offset::new(1150.0, 336.0 + 78.0 + i as f32 * 50.0 + 5.0);
                let pts = fk::thread_pts(from, Offset::new(hr.left, hr.top + hr.height() * 0.5), -70.0);
                fk::grow_stroke(book, &pts, bloom, ACCENT, 1.4, 0.55);
                fk::rider(book, &pts, (sec * 0.4) % 1.0, ACCENT, 2.6, bloom * 0.9);
            }
        }
    }));

    // The tree's names, as type.
    for (i, (depth, name, _)) in TREE.iter().enumerate() {
        let rp = ease_out_cubic(clamp01(build * 1.6 - i as f32 * 0.06));
        if rp <= 0.01 {
            continue;
        }
        let y = 336.0 + 78.0 + i as f32 * 50.0 - 12.0;
        let on = hovered == Some(i);
        stack = stack.push(Positioned::new().left(130.0 + 56.0 + depth * 34.0).top(y).width(500.0).height(30.0).child(
            Opacity::new(rp).child(Text::new((*name).to_string())
                .style(pf::geist_mono(19.0).letter_spacing(0.8)
                    .color(pf::alpha(if on { INK } else { MUTED }, if on { 0.98 } else { 0.72 })))),
        ));
    }
    // The measured facts about the hovered node, right of the tree.
    if let Some(i) = hovered {
        let a = ease_out_cubic(clamp01(hover_since / 0.3));
        const FACTS: [(&str, &str); 3] = [("rect", "343 × 44 @ 24,68"), ("rebuilt", "0 times this frame"), ("paints", "1 rect, 1 glyph run")];
        for (k, (key, val)) in FACTS.iter().enumerate() {
            let y = 336.0 + 78.0 + i as f32 * 50.0 - 12.0 + k as f32 * 0.0;
            let _ = y;
            let yy = 700.0 + k as f32 * 44.0;
            stack = stack.push(Positioned::new().left(700.0).top(yy).width(200.0).height(28.0).child(
                Opacity::new(a).child(Text::new((*key).to_string())
                    .style(pf::geist_mono(15.0).letter_spacing(1.6).color(pf::alpha(pf::FAINT, 0.85)))),
            ));
            stack = stack.push(Positioned::new().left(860.0).top(yy).width(280.0).height(28.0).child(
                Opacity::new(a).child(Text::new((*val).to_string())
                    .style(pf::geist_mono(16.0).letter_spacing(0.8).color(pf::alpha(SYN_TYPE, 0.92)))),
            ));
        }
    }

    pf::chrome(pf::chip_row(
        &[("widget → rect, both ways", ACCENT), ("no source maps", SYN_TYPE), ("no devtools protocol", LEDGER)],
        super::MARGIN,
        986.0,
        clamp01((t - 0.58) / 0.20),
    ));
    pf::caption("hover a pixel; the studio names the widget that made it.", 1002.0, clamp01((t - 0.06) / 0.10));
    pf::caption("it can, because the tree that drew it is still there.", 966.0, clamp01((t - 0.56) / 0.10));
    stack.into()
}

// ── Z17 · build_and_ship ────────────────────────────────────────────────────

/// **And then it ships.** The last thing a studio has to do is stop being
/// a studio: one button, a real `cargo` build, three artifacts. Drawn as
/// a progress the film actually spends time on — because the honest claim
/// is not "instant", it is "one step, in the same window".
pub fn build_and_ship(ctx: &pf::Ctx) -> vieww_widget::WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let mut stack = Stack::new();
    stack = stack.push(room(t, 0x8B17));
    headline_plate(ctx, "one button, three artifacts", "the studio stops being a studio");

    /// (icon, stage, when it starts, when it ends)
    const STAGES: [(Ico, &str, f32, f32); 5] = [
        (Ico::Check, "resolve", 1.0, 2.0),
        (Ico::Code, "compile", 2.0, 5.2),
        (Ico::Shield, "test", 5.2, 6.8),
        (Ico::Tune, "optimise", 6.8, 8.0),
        (Ico::Export, "package", 8.0, 9.2),
    ];
    /// The build's own log — (kind, line). Kind 0 is a step, 1 is work,
    /// 2 is a result. The lines are the shapes a `cargo` build actually
    /// prints, in the order it prints them.
    const LOG: [(u8, &str); 22] = [
        (0, "resolving workspace · 36 crates"),
        (2, "lockfile up to date"),
        (0, "compiling vieww-foundation v0.1.0"),
        (1, "compiling vieww-paint v0.1.0"),
        (1, "compiling vieww-text v0.1.0"),
        (1, "compiling vieww-element v0.1.0"),
        (1, "compiling vieww-widget v0.1.0"),
        (1, "compiling vieww-render v0.1.0"),
        (1, "compiling viewwstudio v0.1.0"),
        (2, "finished `release` in 41.7s"),
        (0, "running 4,225 tests"),
        (1, "vieww-paint · 812 passed"),
        (1, "vieww-widget · 1,104 passed"),
        (1, "vulkan conformance · 37 passed"),
        (2, "test result: ok · 4,225 passed · 0 failed"),
        (0, "optimising · lto, strip, panic=abort"),
        (2, "binary 9.4 MB"),
        (0, "packaging android · aab + apk"),
        (0, "packaging ios · ipa"),
        (0, "packaging desktop · signed binary"),
        (2, "3 artifacts written to target/ship"),
        (2, "done"),
    ];

    /// The three artifacts, each with the device it runs on.
    const OUT: [(&str, &str, Device, f32); 3] = [
        ("app.apk", "android", Device::ANDROID, 9.6),
        ("App.ipa", "ios", Device::IOS, 10.1),
        ("studio.bin", "desktop", Device::DESKTOP, 10.6),
    ];

    let build = clamp01(sec / 0.7);
    stack = stack.push(paint(move |book| {
        // The build rail — one bar, five stages, filled as each runs.
        let x0 = 170.0;
        let x1 = 1200.0;
        let y = 400.0;
        let p = ease_out_expo(build);
        book.rrect(pf::xywh(x0, y, (x1 - x0) * p, 14.0), 7.0, pf::alpha(Color::WHITE, 0.055));
        let done = clamp01((sec - 1.0) / 8.2);
        if done > 0.001 {
            book.rrect(
                pf::xywh(x0, y, (x1 - x0) * done, 14.0),
                7.0,
                vieww_foundation::Gradient::horizontal().with_dither().with_stops(&[
                    (0.0, pf::alpha(ACCENT_DEEP, 0.9)),
                    (1.0, pf::alpha(ACCENT, 0.95)),
                ]),
            );
            book.layer(1.0, 12.0, None, |b| {
                b.circle(Offset::new(x0 + (x1 - x0) * done, y + 7.0), 16.0, pf::alpha(ACCENT, 0.32));
            });
        }
        for (k, (ico, _, from, to)) in STAGES.iter().enumerate() {
            let at = x0 + (x1 - x0) * ((from - 1.0) / 8.2);
            let running = sec >= *from && sec < *to;
            let finished = sec >= *to;
            let a = clamp01((sec - from + 0.6) / 0.4);
            if a <= 0.01 {
                continue;
            }
            let c = if finished { LEDGER } else if running { ACCENT } else { MUTED };
            kit::icon_at(book, if finished { Ico::Check } else { *ico }, Offset::new(at, y - 44.0), 22.0, c, 1.8, a);
            if running {
                book.ring(Offset::new(at, y - 44.0), 20.0 + 8.0 * ((sec * 3.0).sin() * 0.5 + 0.5), 1.2, pf::alpha(ACCENT, 0.4));
            }
            let _ = k;
        }

        // The build's own output, while it builds. Nine seconds of a bar
        // creeping across an empty frame is nine seconds of nothing; the
        // log is what is actually happening, and it fills the time it
        // takes because that is the honest length of a build.
        let log = Rect::new(170.0, 452.0, 1200.0, 836.0);
        let lp = ease_out_expo(clamp01((sec - 1.0) / 0.5));
        if lp > 0.01 {
            book.rrect(log, 14.0, pf::alpha(Color::rgb(0x0C, 0x0B, 0x10), 0.96 * lp));
            book.stroke_rrect(log, 14.0, pf::alpha(Color::WHITE, 0.06 * lp), 1.0);
            book.rect(pf::xywh(log.left, log.top, log.width(), 34.0), pf::alpha(Color::rgb(0x15, 0x13, 0x1B), lp));
            kit::icon_at(book, Ico::Code, Offset::new(log.left + 24.0, log.top + 17.0), 16.0, MUTED, 1.6, 0.8 * lp);
            // One line per 0.42 s, scrolling once the panel is full.
            let n = (((sec - 1.2).max(0.0) / 0.42) as usize).min(LOG.len());
            let visible = 9usize;
            let first = n.saturating_sub(visible);
            for (row, i) in (first..n).enumerate() {
                let (kind, _) = LOG[i];
                let y = log.top + 56.0 + row as f32 * 34.0;
                let fresh = clamp01(((sec - 1.2) / 0.42 - i as f32) / 0.5);
                let c = match kind { 0 => MUTED, 1 => SYN_TYPE, _ => LEDGER };
                kit::icon_at(
                    book,
                    match kind { 0 => Ico::ChevronRight, 1 => Ico::Code, _ => Ico::Check },
                    Offset::new(log.left + 26.0, y + 2.0),
                    13.0,
                    c,
                    1.5,
                    0.8 * fresh * lp,
                );
                let _ = y;
            }
        }

        // The three artifacts, each landing on its own device.
        for (i, (_, _, dev, at)) in OUT.iter().enumerate() {
            let ap = clamp01((sec - at) / 0.8);
            if ap <= 0.01 {
                continue;
            }
            // Three real shapes, sized so the window's 1280×800 does not
            // walk off the right edge: the phones share a height, the
            // window takes the one its own aspect can afford.
            let h = if *dev == Device::DESKTOP { 208.0 } else { 300.0 };
            let (w, _, _, _) = dev.frame_for_height(h);
            let cx = [1288.0f32, 1432.0, 1664.0][i];
            kit::device(
                book,
                *dev,
                Offset::new(cx - w * 0.5, 470.0 + (1.0 - ease_out_cubic(ap)) * 30.0),
                h,
                Screen::List { rows: 3, title_lit: true, lit_row: None },
                ACCENT,
                None,
                ap,
                ap,
            );
        }
    }));

    // The log's lines, as type — the painter above drew their icons, so
    // the two agree on a baseline by sharing the same arithmetic.
    let lp = clamp01((sec - 1.0) / 0.5);
    if lp > 0.01 {
        let n = (((sec - 1.2).max(0.0) / 0.42) as usize).min(LOG.len());
        let first = n.saturating_sub(9);
        for (row, i) in (first..n).enumerate() {
            let (kind, line) = LOG[i];
            let y = 452.0 + 56.0 + row as f32 * 34.0 - 12.0;
            let fresh = ease_out_cubic(clamp01(((sec - 1.2) / 0.42 - i as f32) / 0.5));
            let c = match kind { 0 => INK, 1 => MUTED, _ => LEDGER };
            stack = stack.push(Positioned::new().left(196.0).top(y).width(960.0).height(28.0).child(
                Opacity::new(fresh * lp).child(Text::new(line.to_string())
                    .style(pf::geist_mono(17.0).letter_spacing(0.5).color(pf::alpha(c, 0.92)))),
            ));
        }
    }

    // The stage names.
    for (_, name, from, to) in STAGES {
        let a = ease_out_cubic(clamp01((sec - from + 0.6) / 0.4));
        if a <= 0.01 {
            continue;
        }
        let at = 170.0 + (1200.0 - 170.0) * ((from - 1.0) / 8.2);
        let finished = sec >= to;
        stack = stack.push(Positioned::new().left(at - 90.0).top(330.0).width(180.0).height(26.0).child(
            Opacity::new(a).child(Text::new(name.to_string())
                .style(pf::geist_mono(16.0).letter_spacing(1.6)
                    .color(pf::alpha(if finished { LEDGER } else { INK }, 0.94)))
                .align(TextAlign::Center)),
        ));
    }
    // The artifacts' names, under their devices, on one baseline.
    for (i, (file, plat, _, at)) in OUT.iter().enumerate() {
        let ap = ease_out_cubic(clamp01((sec - at) / 0.8));
        if ap <= 0.01 {
            continue;
        }
        let cx = [1288.0f32, 1432.0, 1664.0][i];
        stack = stack.push(Positioned::new().left(cx - 170.0).top(796.0).width(340.0).height(30.0).child(
            Opacity::new(ap).child(Text::new((*file).to_string())
                .style(pf::geist_mono(17.0).letter_spacing(0.8).color(pf::alpha(INK, 0.96)))
                .align(TextAlign::Center)),
        ));
        stack = stack.push(Positioned::new().left(cx - 170.0).top(824.0).width(340.0).height(26.0).child(
            Opacity::new(ap * 0.85).child(Text::new((*plat).to_string())
                .style(pf::geist_mono(14.0).letter_spacing(2.6).color(pf::alpha(MUTED, 0.85)))
                .align(TextAlign::Center)),
        ));
    }
    // The one line that makes the claim honest.
    let close = ease_out_cubic(clamp01((sec - 11.0) / 0.6));
    if close > 0.01 {
        stack = stack.push(Positioned::new().left(1290.0).top(880.0).width(560.0).height(34.0).child(
            Opacity::new(close).child(Text::new("cargo, not a cloud".to_string())
                .style(pf::geist_mono(20.0).letter_spacing(1.6).color(pf::alpha(LEDGER, 0.95)))),
        ));
    }

    pf::chrome(pf::chip_row(
        &[("one command", ACCENT), ("three targets", SYN_TYPE), ("no CI required", LEDGER)],
        super::MARGIN,
        986.0,
        clamp01((t - 0.70) / 0.20),
    ));
    pf::caption("and then it ships — from the same window.", 1002.0, clamp01((t - 0.06) / 0.10));
    pf::caption("one tree, one toolchain, three real artifacts.", 966.0, clamp01((t - 0.70) / 0.10));
    stack.into()
}
