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
    pub const SIDEBAR: Offset = Offset::new(130.0, 460.0);
    /// The editor's code — centre stage.
    pub const EDITOR: Offset = Offset::new(700.0, 430.0);
    /// The preview device — right pane (the tap anchor lives here).
    pub const PREVIEW: Offset = Offset::new(1585.0, 450.0);
    /// The wordmark's home, over the top band.
    pub const WORDMARK: Offset = Offset::new(960.0, 214.0);
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
                .top(pf::BAND_H + 26.0 + rise)
                .width(W)
                .height(64.0)
                .child(Opacity::new(mark_a).child(fk::wordmark(42.0, 1.0))),
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
                    Rect::new(36.0, 20.0, W - 36.0, 1010.0),
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
    beat_chip("open: counter.say", SYN_TYPE, 360.0, 330.0, abs - 56.4);
    beat_chip("panel closes — the editor breathes", MUTED, 360.0, 372.0, abs - 60.0);
    beat_chip("panel returns — the tree is the map", MUTED, 360.0, 414.0, abs - 64.0);

    // The receipts.
    pf::chrome(pf::chip_row(
        &[
            ("the real Shell", ACCENT),
            ("the real editor", SYN_TYPE),
            ("the real preview", SYN_STRING),
        ],
        360.0,
        924.0,
        clamp01((t - 0.45) / 0.20),
    ));

    pf::caption("one shell: editor, preview, build, ship.", 1002.0, clamp01((sec - 0.9) / 0.12));
    pf::caption("every pixel below this line is vieww's.", 966.0, clamp01((sec - 1.3) / 0.12));
    stack.into()
}

// ── Z08 · live_compose ──────────────────────────────────────────────────────

/// A mono string typed to `frac` of its length — chars, never bytes.
fn typed(text: &str, frac: f32) -> String {
    let chars: Vec<char> = text.chars().collect();
    let n = ((chars.len() as f32 * frac.clamp(0.0, 1.0)).round()) as usize;
    chars[..n.min(chars.len())].iter().collect()
}

/// The exploded edit card — the session's two edits, lifted out of the
/// editor and annotated: before, after, and the caret still moving.
/// A thread flows from the card to the preview device, because that is
/// the direction the pixels actually travel.
pub fn live_compose(ctx: &pf::Ctx) -> vieww_widget::WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let abs = ctx.abs;
    let mut stack = super::studio_plate(ctx, "MOVEMENT III", "THE STUDIO");

    headline_plate(ctx, "edit a line — see the picture change", "the preview is the tree, not a texture");

    // The exploded card — arrives once the damage overlay is on.
    let card_p = ease_out_expo(clamp01((abs - 70.8) / 0.6));
    if card_p > 0.01 {
        let title_frac = clamp01((abs - 73.0) / 1.3);
        let row_frac = clamp01((abs - 75.6) / 1.2);
        let caret_on = (sec * 2.6).fract() < 0.55;
        let caret_len = typed("title: “My own inbox", title_frac).chars().count();

        // The card's body.
        stack = stack.push(Positioned::new().left(430.0).top(196.0 + (1.0 - card_p) * 22.0).width(900.0).height(252.0).child(
            Opacity::new(card_p).child(Painting::sized(Size::new(900.0, 252.0), PaintWith::new(
                move |book: &mut Sketchbook, _s: Size| {
                    let body = pf::xywh(0.0, 0.0, 900.0, 252.0);
                    book.shadow(pf::xywh(0.0, 8.0, 900.0, 252.0), 18.0, vieww_foundation::Shadow::new(pf::alpha(Color::BLACK, 0.5), Offset::new(0.0, 10.0), 30.0));
                    book.rrect(body, 16.0, pf::alpha(pf::SURFACE, 0.97));
                    book.stroke_rrect(body, 16.0, pf::alpha(ACCENT, 0.35), 1.2);
                    // The header tick.
                    book.rrect(pf::xywh(24.0, 20.0, 8.0, 18.0), 3.0, pf::alpha(ACCENT, 0.95));
                    // The rows' pills — grey before, purple after.
                    let row_a: [f32; 3] = [1.0, clamp01((abs - 72.8) / 0.3), clamp01((abs - 75.4) / 0.3)];
                    for (i, ra) in row_a.iter().enumerate() {
                        if *ra <= 0.01 {
                            continue;
                        }
                        let y = 58.0 + i as f32 * 62.0;
                        book.rrect(pf::xywh(24.0, y, 852.0, 46.0), 10.0, pf::alpha(pf::SURFACE_2, 0.85 * ra));
                        book.rrect(
                            pf::xywh(38.0, y + 13.0, 52.0, 20.0),
                            6.0,
                            pf::alpha(if i == 0 { pf::FAINT } else { ACCENT_DEEP }, 0.55 * ra),
                        );
                    }
                    // The caret — right where the typing stops.
                    if caret_on && title_frac > 0.02 && title_frac < 1.0 {
                        let w = pf::gmono_tw(15.0, caret_len, 0.4);
                        book.rrect(pf::xywh(46.0 + w, 58.0 + 62.0 + 12.0, 2.5, 21.0), 1.2, pf::alpha(ACCENT, 0.95));
                    }
                },
            ))),
        ));
        // The header text.
        stack = stack.push(Positioned::new().left(430.0 + 44.0).top(196.0 + 16.0).width(500.0).height(22.0).child(
            Text::new("the edit — one line, live".to_string())
                .style(pf::geist_mono(13.0).letter_spacing(1.6).color(pf::alpha(ACCENT, 0.95 * card_p))),
        ));
        // The rows' text — widgets over the painting, same geometry.
        let rows: [(String, Color, f32); 3] = [
            ("title: “Inbox”".into(), MUTED, 1.0),
            (format!("title: “{}”", typed("My own inbox", title_frac)), ACCENT, clamp01((abs - 72.8) / 0.3)),
            (format!("row:   “{}”", typed("Shipped the beta today", row_frac)), ACCENT, clamp01((abs - 75.4) / 0.3)),
        ];
        for (i, (body_text, color, ra)) in rows.iter().enumerate() {
            if *ra <= 0.01 {
                continue;
            }
            let y = 196.0 + 58.0 + i as f32 * 62.0 + 13.0;
            stack = stack.push(Positioned::new().left(430.0 + 46.0).top(y).width(760.0).height(22.0).child(
                Opacity::new(*ra).child(
                    Text::new(body_text.clone())
                        .style(pf::geist_mono(15.0).letter_spacing(0.4).color(pf::alpha(*color, 0.96))),
                ),
            ));
        }
    }

    // The thread — card to preview, the direction the pixels travel.
    let thread_a = clamp01((abs - 73.4) / 0.4);
    if thread_a > 0.01 {
        stack = stack.push(Positioned::fill().child(
            Painting::sized(CANVAS, PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                fk::flow_thread(book, Offset::new(1330.0, 330.0), region::PREVIEW, 60.0, sec, SYN_STRING, thread_a, 1.8);
            })),
        ));
    }

    // The damage window — brackets around the editor while the overlay
    // is on, breathing with the same clock the app uses.
    if abs >= 71.0 && abs <= 76.8 {
        let blink = 0.55 + 0.45 * (sec * 6.0).sin();
        stack = stack.push(Positioned::fill().child(
            Painting::sized(CANVAS, PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                fk::corner_brackets(book, Rect::new(268.0, 330.0, 1210.0, 700.0), 1.0, BREAK_RED, 0.55 * blink, 40.0);
            })),
        ));
    }

    // The beats — the session's own times, one chip each.
    beat_chip("live preview: accepted", SYN_STRING, 360.0, 330.0, abs - 69.2);
    beat_chip("damage overlay: on", BREAK_RED, 360.0, 372.0, abs - 71.0);
    beat_chip("edit: title → “My own inbox”", ACCENT, 360.0, 414.0, abs - 73.0);
    beat_chip("edit: row → “Shipped the beta today”", ACCENT, 360.0, 456.0, abs - 75.6);
    beat_chip("damage overlay: off", MUTED, 360.0, 498.0, abs - 76.8);

    // The receipts — what one edit cost, as the app itself reported it.
    pf::chrome(pf::chip_row(
        &[
            ("damage: one rect", ACCENT),
            ("rebuild: one subtree", SYN_TYPE),
            ("budget: kept", LEDGER),
        ],
        360.0,
        924.0,
        clamp01((t - 0.55) / 0.20),
    ));

    pf::caption("no relaunch. no reload. the line lands while you watch.", 1002.0, clamp01((t - 0.16) / 0.12));
    pf::caption("what you see repaint itself is exactly what the planner repainted.", 966.0, clamp01((t - 0.60) / 0.12));
    stack.into()
}

// ── Z09 · say_to_rust ───────────────────────────────────────────────────────

/// The compile pipeline — six stations on one flowing S-thread across
/// the top of the frame, each glowing when the session reaches it,
/// packets riding the thread the whole way. The film's most literal
/// vector flow: the language literally flows into pixels.
pub fn say_to_rust(ctx: &pf::Ctx) -> vieww_widget::WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let abs = ctx.abs;
    let mut stack = super::studio_plate(ctx, "MOVEMENT III", "THE STUDIO");

    headline_plate(ctx, ".say → rust → cdylib → preview", "one file of english — a real widget tree");

    // The stations and the thread through them — a flowing S-thread in
    // the band between the editor's last lines and the receipts, where
    // the headline plate cannot bury it.
    const STATIONS: [&str; 6] = [".say", "codegen", "rustc", "cdylib", "dlopen", "pixels"];
    let xs: Vec<f32> = (0..6).map(|i| 250.0 + i as f32 * 290.0).collect();
    let ys: Vec<f32> = (0..6).map(|i| if i % 2 == 0 { 580.0 } else { 616.0 }).collect();

    // The combined thread polyline, and the beat each station lights.
    let mut pts: Vec<Offset> = Vec::new();
    for i in 0..6 {
        let node = Offset::new(xs[i], ys[i]);
        if pts.is_empty() {
            pts.push(node);
        } else {
            let bend = if i % 2 == 1 { 46.0 } else { -46.0 };
            let seg = fk::thread_pts(Offset::new(xs[i - 1], ys[i - 1]), node, bend);
            pts.extend(seg.into_iter().skip(1));
        }
    }
    let lit_at: [f32; 6] = [77.4, 78.0, 83.2, 83.35, 83.4, 84.2];

    let xs_p = xs.clone();
    let ys_p = ys.clone();
    stack = stack.push(Positioned::fill().child(
        Painting::sized(CANVAS, PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            // The carrier — drawn once the first station is near.
            let carrier = clamp01((abs - 76.6) / 0.5);
            if carrier > 0.01 {
                fk::grow_stroke(book, &pts, carrier, BRAND_NEAR, 2.0, 0.5 * carrier);
            }
            // The flow — dashes and riders once the thread is whole.
            let flow_a = clamp01((abs - 77.2) / 0.4);
            if flow_a > 0.01 {
                let total = pts.len();
                let n = 14;
                for k in 0..n {
                    let u = ((k as f32 / n as f32) + sec * 0.11) % 1.0;
                    let i0 = (u * (total - 1) as f32) as usize;
                    let i1 = (i0 + 1).min(total - 1);
                    book.line(pts[i0], pts[i1], pf::alpha(SYN_TYPE, 0.5 * flow_a), 1.6);
                }
                fk::rider(book, &pts, (sec * 0.13) % 1.0, ACCENT, 3.0, flow_a);
            }
            // The stations — chips glowing as the session reaches them.
            for i in 0..6 {
                let since = abs - lit_at[i];
                let lit = if since > 0.0 {
                    let flash = (-since * 2.2).exp();
                    (0.45 + 0.55 * flash).min(1.0)
                } else {
                    0.28
                };
                let (nx, ny) = (xs_p[i], ys_p[i]);
                if lit > 0.45 {
                    pf::glow(book, nx, ny, 60.0, ACCENT, (lit - 0.45) * 0.5);
                }
                let c = pf::xywh(nx - 62.0, ny - 20.0, 124.0, 40.0);
                book.rrect(c, 12.0, pf::alpha(pf::SURFACE, 0.96));
                book.stroke_rrect(c, 12.0, pf::alpha(if since > 0.0 { ACCENT } else { pf::FAINT }, 0.25 + 0.55 * lit), 1.3);
                book.circle(Offset::new(nx - 40.0, ny), 3.4, pf::alpha(if since > 0.0 { ACCENT } else { pf::FAINT }, 0.9 * lit));
            }
        })),
    ));

    // The stations' labels — widgets, at the same coordinates.
    for (i, name) in STATIONS.iter().enumerate() {
        let since = abs - lit_at[i];
        let a = ease_out_cubic(clamp01((abs - 76.8 - i as f32 * 0.06) / 0.3));
        if a <= 0.01 {
            continue;
        }
        stack = stack.push(
            Positioned::new()
                .left(xs[i] - 36.0)
                .top(ys[i] - 11.0)
                .width(110.0)
                .height(24.0)
                .child(Opacity::new(a).child(
                    Text::new(name.to_string())
                        .style(pf::geist_mono(14.0).letter_spacing(1.0).color(pf::alpha(
                            if since > 0.0 { INK } else { MUTED },
                            0.95,
                        ))),
                )),
        );
    }

    // The two real taps — the film's annotation of real input.
    for at in [85.6, 87.2] {
        let since = abs - at;
        if (0.0..=1.0).contains(&since) {
            stack = stack.push(pf::tap_ring_at(TAP_ADD_ONE, since));
        }
    }

    // The pipeline beats, one per stage of the real compile.
    beat_chip("say-codegen: parses the program", SYN_MACRO, 360.0, 372.0, abs - 78.0);
    beat_chip("rustc: compiles the generated rust", SYN_TYPE, 360.0, 414.0, abs - 83.2);
    beat_chip("dlopen: the preview loads the cdylib", pf::SYN_FUNCTION, 360.0, 456.0, abs - 83.4);
    beat_chip("pixels: the screen is the counter", SYN_STRING, 360.0, 498.0, abs - 84.2);

    // The receipts.
    pf::chrome(pf::chip_row(
        &[
            ("counter.say → counter.rs", SYN_MACRO),
            ("rustc: settled off-clock", MUTED),
            ("2 taps, real input", ACCENT),
        ],
        360.0,
        924.0,
        clamp01((t - 0.62) / 0.20),
    ));

    pf::caption("the language compiles to the framework — no bridge, no interpreter.", 1002.0, clamp01((t - 0.14) / 0.12));
    pf::caption("and the counter counts, because the taps are real.", 966.0, clamp01((t - 0.66) / 0.12));
    stack.into()
}

// ── Z10 · ships_everywhere ──────────────────────────────────────────────────

/// One tree, every device — three 3D slabs rise above the studio as
/// the session flips platforms, ribbon threads tying each to the real
/// preview pane, and the token flip floods the frame in purple. The
/// 2D studio below, the 3D fleet above: one picture, two depths.
pub fn ships_everywhere(ctx: &pf::Ctx) -> vieww_widget::WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let abs = ctx.abs;
    let mut stack = super::studio_plate(ctx, "MOVEMENT III", "THE STUDIO");

    headline_plate(ctx, "one tree. every device.", "platforms and tokens, not forks");

    // The fleet — three slabs, each tilted in true perspective, each
    // rising as its platform beat lands.
    let slabs: [(f32, f32, f32, &str, f32, Color, f32); 3] = [
        (430.0, 168.0, -0.20, "android", 89.6, SYN_TYPE, 0.10),
        (860.0, 148.0, 0.0, "ios", 91.2, BRAND_NEAR, 0.08),
        (1290.0, 168.0, 0.20, "desktop", 92.8, SYN_STRING, 0.10),
    ];
    for (x, y, yaw, name, lit_at, color, pitch) in slabs {
        let rise_p = clamp01((abs - lit_at) / 0.7);
        if rise_p <= 0.01 {
            continue;
        }
        let rise = ease_out_cubic(rise_p);
        stack = stack.push(Positioned::new().left(x).top(y).width(340.0).height(216.0).child(
            Opacity::new(rise).child(Painting::sized(Size::new(340.0, 216.0), PaintWith::new(
                move |book: &mut Sketchbook, _s: Size| {
                    let rect = Rect::new(0.0, 0.0, 340.0, 190.0);
                    let a = rise;
                    // The slab in perspective, drawn by the framework's
                    // own quadrant projector.
                    pf::panel_3d(book, rect, yaw, pitch, 980.0, a, |b| {
                        let body = pf::xywh(6.0, 6.0, 328.0, 178.0);
                        b.shadow(body, 14.0, vieww_foundation::Shadow::new(pf::alpha(Color::BLACK, 0.45), Offset::new(0.0, 8.0), 24.0));
                        b.rrect(body, 14.0, pf::alpha(pf::MARK_GROUND, 0.98));
                        b.stroke_rrect(body, 14.0, pf::alpha(color, 0.45), 1.2);
                        if name == "desktop" {
                            // A window — title bar, three dots, content.
                            b.rrect(pf::xywh(6.0, 6.0, 328.0, 30.0), 14.0, pf::alpha(pf::SURFACE_2, 0.9));
                            for d in 0..3 {
                                b.circle(Offset::new(26.0 + d as f32 * 16.0, 21.0), 4.0, pf::alpha(pf::FAINT, 0.9));
                            }
                            for row in 0..3 {
                                b.rrect(pf::xywh(26.0, 52.0 + row as f32 * 34.0, 200.0 - row as f32 * 40.0, 12.0), 6.0, pf::alpha(pf::MUTED, 0.4));
                            }
                            b.rrect(pf::xywh(26.0, 152.0, 120.0, 20.0), 8.0, pf::alpha(ACCENT, 0.75));
                        } else {
                            // A phone — rounded frame, notch, app rows.
                            let ph = pf::xywh(106.0, 14.0, 128.0, 162.0);
                            b.rrect(ph, 20.0, pf::alpha(pf::SURFACE, 0.95));
                            b.stroke_rrect(ph, 20.0, pf::alpha(Color::WHITE, 0.14), 1.1);
                            b.rrect(pf::xywh(146.0, 22.0, 48.0, 7.0), 3.5, pf::alpha(Color::BLACK, 0.5));
                            b.rrect(pf::xywh(120.0, 44.0, 100.0, 34.0), 8.0, pf::alpha(color, 0.55));
                            for row in 0..3 {
                                b.rrect(pf::xywh(120.0, 90.0 + row as f32 * 26.0, 100.0, 14.0), 5.0, pf::alpha(pf::SURFACE_2, 0.95));
                            }
                        }
                        // The tree thread inside — the same tree, every slab.
                        let mut tp = Path::new();
                        tp.move_to(Offset::new(24.0, 170.0));
                        tp.cubic_to(Offset::new(90.0, 120.0), Offset::new(230.0, 190.0), Offset::new(316.0, 120.0));
                        b.stroke(tp, pf::alpha(BRAND_NEAR, 0.4), 1.2);
                    });
                },
            ))),
        ));
        // The slab's name — a widget under the painting.
        stack = stack.push(
            Positioned::new()
                .left(x)
                .top(y + 192.0)
                .width(340.0)
                .height(22.0)
                .child(Opacity::new(rise).child(
                    Text::new(name.to_string())
                        .style(pf::geist_mono(14.0).letter_spacing(2.4).color(pf::alpha(INK, 0.9 * rise)))
                        .align(TextAlign::Center),
                )),
        );
        // The ribbon — preview to slab, flowing once the slab is up.
        let ribbon_a = clamp01((rise_p - 0.55) * 2.4);
        if ribbon_a > 0.01 {
            let from = region::PREVIEW;
            let to = Offset::new(x + 170.0, y + 180.0);
            let bend = if x < 800.0 { -140.0 } else if x > 1200.0 { 140.0 } else { -80.0 };
            stack = stack.push(Positioned::fill().child(
                Painting::sized(CANVAS, PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    let pts = fk::thread_pts(from, to, bend);
                    fk::ribbon(book, &pts, 2.4, sec, color, ribbon_a * 0.8);
                    fk::rider(book, &pts, (sec * 0.35) % 1.0, color, 2.6, ribbon_a);
                })),
            ));
        }
    }

    // The token flip — the flood, then the swatch, exactly as v1 drew it.
    let flood_p = clamp01((abs - 96.8) / 0.9);
    if flood_p > 0.001 && flood_p < 0.999 {
        stack = stack.push(Positioned::fill().child(
            Painting::sized(CANVAS, PaintWith::new(move |book: &mut Sketchbook, s: Size| {
                fk::token_flood(book, s.width, s.height, flood_p, BRAND_NEAR);
            })),
        ));
    }

    // The platform flips — three chips lighting as each lands.
    beat_chip("android — the frame re-chromes", SYN_TYPE, 360.0, 430.0, abs - 89.6);
    beat_chip("ios — same tree, new metrics", SYN_TYPE, 360.0, 472.0, abs - 91.2);
    beat_chip("desktop — same tree, a window", SYN_TYPE, 360.0, 514.0, abs - 92.8);
    beat_chip("tokens: the accent is data", LEDGER, 360.0, 556.0, abs - 94.4);
    beat_chip("teal → purple — the studio re-tints, live", ACCENT, 360.0, 598.0, abs - 95.6);

    // The accent swatch — the token flip, drawn as it happens.
    let teal_at = 95.6;
    let purple_at = 96.8;
    let swatch = if abs >= purple_at {
        ACCENT
    } else if abs >= teal_at {
        Color::rgb(0x2F, 0xBF, 0xAE)
    } else {
        Color::rgb(0x46, 0x4E, 0x5E)
    };
    let sw_a = ease_out_cubic(clamp01((t - 0.72) / 0.14));
    if sw_a > 0.01 {
        stack = stack.push(Positioned::new().left(1420.0).top(430.0).width(300.0).height(200.0).child(
            Painting::sized(Size::new(300.0, 200.0), PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                book.rrect(pf::xywh(40.0, 20.0, 220.0, 68.0), 16.0, pf::alpha(swatch, sw_a));
                book.stroke_rrect(pf::xywh(40.0, 20.0, 220.0, 68.0), 16.0, pf::alpha(Color::WHITE, 0.10 * sw_a), 1.0);
                book.rrect(pf::xywh(40.0, 108.0, 220.0, 12.0), 6.0, pf::alpha(swatch, 0.4 * sw_a));
                book.rrect(pf::xywh(40.0, 132.0, 150.0, 12.0), 6.0, pf::alpha(swatch, 0.25 * sw_a));
            })),
        ));
    }

    // The receipts.
    pf::chrome(pf::chip_row(
        &[
            ("3 platforms, 0 forks", ACCENT),
            ("tokens: 1 line to a new brand", LEDGER),
        ],
        360.0,
        924.0,
        clamp01((t - 0.70) / 0.20),
    ));

    pf::caption("the same widget tree — re-framed, not re-written.", 1002.0, clamp01((t - 0.14) / 0.12));
    pf::caption("a brand is a token file, and the studio edits tokens.", 966.0, clamp01((t - 0.62) / 0.12));
    stack.into()
}

/// Keep the lint honest about the one import the beats still carry.
#[allow(unused)]
fn _unused() {
    let _ = (TAP_LIVE_ROW,);
}
