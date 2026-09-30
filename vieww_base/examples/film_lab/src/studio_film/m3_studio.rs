//! Movement III · THE STUDIO — the actual `viewwstudio`, and nothing that
//! pretends to be it.
//!
//! The v4 review: *the studio frame is not good — why is it blacked out at
//! the top? Place it in the centre, and the scenes after it should match
//! the layout of the original studio: the text, the placement, everything.*
//! And of the device slide: *these screens should be taken from how they
//! appear in the studio, not some funky animation.*
//!
//! So every scene of this act rides the **real application**, mounted once
//! on the film's own driver and driven by [`super::script`]:
//!
//! * **Z11** shows it whole, centred in the body, the frame's own header
//!   and footer around it — no matte, no scrim, no wordmark over the code.
//! * **Z12–Z17** quote it. A *plate* is a rectangle of the studio's own
//!   draw list re-composited somewhere else in the frame (see
//!   [`frame::Plate`]): the same glyph runs and paths, re-rasterised at
//!   the size the scene needs. The shell is taken apart by moving its real
//!   panes; the edit is magnified from the real editor and the real
//!   preview; the platforms and the devices are the preview pane captured
//!   in each state (`script::snapshots`). The film draws only what the
//!   application cannot: the labels, the threads, the build log.
//!
//! Coordinates: Z11 works in the application's own 1920×1080 space (the
//! master fits it into the body). Z12–Z17 work in **screen** space inside
//! [`BODY`] — their content box is the body itself, so the fit is the
//! identity and a number here is a pixel on screen.

use vieww_foundation::{Color, Offset, Rect, Size, Sketchbook, TextAlign};
use vieww_widget::prelude::*;

use crate::film_lib::{clamp01, ease_in_out, ease_out_cubic, ease_out_expo};
use crate::product_film as pf;
use super::filmkit as fk;
use super::frame::{self, Plate, BODY};
use super::kit::{self, Ico};
use super::{ACCENT, BREAK_RED, BRAND_NEAR, CANVAS, INK, LEDGER, MUTED, SYN_MACRO, SYN_STRING, SYN_TYPE};

// ── The application's own geometry ──────────────────────────────────────────
//
// Read off the running studio (`film_lab sfprobe`), in its 1920×1080 space.

pub mod app {
    use vieww_foundation::Rect;
    const fn r(l: f32, t: f32, rt: f32, b: f32) -> Rect {
        Rect { left: l, top: t, right: rt, bottom: b }
    }
    pub const FULL: Rect = r(0.0, 0.0, 1920.0, 1080.0);
    pub const TITLE: Rect = r(0.0, 0.0, 1920.0, 47.0);
    pub const ACTIVITY: Rect = r(0.0, 47.0, 60.0, 1046.0);
    pub const EXPLORER: Rect = r(60.0, 47.0, 316.0, 1046.0);
    pub const EDITOR: Rect = r(316.0, 47.0, 1450.0, 829.0);
    pub const PREVIEW: Rect = r(1450.0, 47.0, 1920.0, 829.0);
    pub const PANEL: Rect = r(316.0, 829.0, 1920.0, 1046.0);
    pub const STATUS: Rect = r(0.0, 1046.0, 1920.0, 1080.0);
    /// The live.rs tab in the editor's tab strip.
    pub const TAB_LIVE: Rect = r(640.0, 52.0, 746.0, 86.0);
    /// The live preview's caution dialog.
    pub const DIALOG: Rect = r(730.0, 96.0, 1190.0, 340.0);
    /// The simulated iPhone in the preview pane (bezel and badge included).
    pub const PHONE: Rect = r(1546.0, 122.0, 1818.0, 702.0);
    /// The phone's screen.
    pub const PHONE_SCREEN: Rect = r(1563.0, 148.0, 1804.0, 690.0);
    /// The editor's code, lines 10–17 of live.rs (`screen "Home"`).
    pub const LIVE_HOME: Rect = r(318.0, 302.0, 1150.0, 464.0);
    /// The editor's code, counter.say lines 1–15.
    pub const SAY_CODE: Rect = r(318.0, 118.0, 1030.0, 422.0);
    /// The preview pane's stage (below its two tool rows).
    pub const STAGE: Rect = r(1452.0, 116.0, 1914.0, 716.0);
    /// The Devices tab's list.
    pub const DEVICES_LIST: Rect = r(1452.0, 50.0, 1914.0, 470.0);
    /// The panel's tab strip (its tabs, not its trailing buttons).
    pub const PANEL_TABS: Rect = r(318.0, 832.0, 1010.0, 866.0);
    /// Line `n` (1-based) of the editor: its text's vertical centre.
    pub fn line_y(n: u32) -> f32 {
        132.0 + (n as f32 - 1.0) * 20.0
    }
}

/// The whole application fitted into the body — Z12's closed shell.
fn app_scale() -> f32 {
    BODY.height() / 1080.0
}

/// An application rectangle, placed where the whole-app fit puts it.
fn a2s(r: Rect) -> Rect {
    let s = app_scale();
    let ox = (BODY.left + BODY.right) * 0.5 - 960.0 * s;
    let oy = BODY.top;
    Rect::new(ox + r.left * s, oy + r.top * s, ox + r.right * s, oy + r.bottom * s)
}

/// A plate of the live studio.
fn plate(src: Rect, dst: Rect, alpha: f32, radius: f32, card: bool) {
    frame::plate(Plate { src, dst, alpha, radius, snap: None, card });
}

/// A plate of a named snapshot.
fn snap_plate(key: &'static str, src: Rect, dst: Rect, alpha: f32, radius: f32) {
    frame::plate(Plate { src, dst, alpha, radius, snap: Some(key), card: true });
}

/// `src` scaled by `s` with its top-left at `at`.
fn place(src: Rect, at: Offset, s: f32) -> Rect {
    Rect::new(at.dx, at.dy, at.dx + src.width() * s, at.dy + src.height() * s)
}

/// `src` scaled by `s` and centred on `c`.
fn centred(src: Rect, c: Offset, s: f32) -> Rect {
    let (w, h) = (src.width() * s, src.height() * s);
    Rect::new(c.dx - w * 0.5, c.dy - h * 0.5, c.dx + w * 0.5, c.dy + h * 0.5)
}

fn lerp_rect(a: Rect, b: Rect, t: f32) -> Rect {
    let l = |x: f32, y: f32| x + (y - x) * t;
    Rect::new(l(a.left, b.left), l(a.top, b.top), l(a.right, b.right), l(a.bottom, b.bottom))
}

/// A painting over the whole canvas.
fn paint(f: impl Fn(&mut Sketchbook) + Send + Sync + 'static) -> WidgetNode {
    Positioned::fill()
        .child(Painting::sized(CANVAS, PaintWith::new(move |book: &mut Sketchbook, _s: Size| f(book))))
        .into()
}

/// The scene's narration: the header's subtitle follows the beats — the
/// line for the beat just passed fades out as the new one fades in.
fn beats(sec: f32, list: &[(f32, &str)]) {
    let cur = list.iter().rposition(|(at, _)| sec >= *at);
    let Some(i) = cur else { return };
    let since = sec - list[i].0;
    let a = clamp01(since / 0.45);
    if i > 0 && a < 1.0 {
        frame::caption(list[i - 1].1, 1002.0, 1.0);
    }
    frame::caption(list[i].1, 966.0, if i == 0 { clamp01((sec - 0.1) / 0.4) } else { a });
}

/// A name tag: a dark rounded label with a coloured edge — the act's one
/// annotation, the same everywhere.
fn tag(x: f32, y: f32, text: &str, size: f32, color: Color, a: f32) -> WidgetNode {
    let w = frame::chip_w(text, size) + 8.0;
    frame::chip_at(x, y, w, size * 1.9, text.to_string(), size, color, a)
}

/// A two-line tag: the region's name, and what it is — on one dark card,
/// so the description never sits bare on top of the application's text.
fn card_tag(x: f32, y: f32, name: &str, what: &str, color: Color, a: f32) -> WidgetNode {
    if a <= 0.01 {
        return Stack::new().into();
    }
    let w = frame::chip_w(name, 24.0).max(frame::chip_w(what, 17.0)) + 12.0;
    let h = 82.0;
    let r = Rect::new(x, y, x + w, y + h);
    Stack::new()
        .push(paint(move |book| {
            book.shadow(Rect::new(r.left, r.top + 4.0, r.right, r.bottom + 4.0), 10.0,
                vieww_foundation::Shadow::new(pf::alpha(Color::BLACK, 0.5 * a), Offset::new(0.0, 6.0), 18.0));
            book.rrect(r, 10.0, pf::alpha(Color::rgb(0x14, 0x12, 0x19), 0.96 * a));
            book.stroke_rrect(r, 10.0, pf::alpha(color, 0.55 * a), 1.2);
        }))
        .push(frame::label(x + 16.0, y + 8.0, w - 24.0, 36.0, name.to_string(),
            pf::geist_mono(24.0).color(pf::alpha(color, 0.98)), TextAlign::Left, a))
        .push(frame::label(x + 16.0, y + 44.0, w - 24.0, 28.0, what.to_string(),
            pf::geist_mono(17.0).color(pf::alpha(INK, 0.88)), TextAlign::Left, a))
        .into()
}

/// A focus ring over a rectangle: an accent outline, doubled by a dimmer
/// hairline further out.
fn ring(book: &mut Sketchbook, r: Rect, radius: f32, color: Color, a: f32, width: f32) {
    if a <= 0.01 {
        return;
    }
    // Two crisp strokes — the halo this once drew with a blur is gone
    // with the film's radial glows; a second hairline, further out and
    // dimmer, is the whole of the "soft" register now.
    book.stroke_rrect(r.inflate(3.0), radius, pf::alpha(color, 0.30 * a), width);
    book.stroke_rrect(r, radius, pf::alpha(color, 0.95 * a), width);
}

// ── Z11 · studio_opens ──────────────────────────────────────────────────────

/// **The studio, whole.** The real application, centred in the body with
/// the film's own header above and footer below — the same surface, no
/// band of black. A focus ring follows what the session does: the tab
/// that opens, the live preview's dialog, the phone the sketch lands on,
/// and then the four regions of the window, one at a time.
pub fn studio_opens(ctx: &pf::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    frame::headline("This is viewwstudio.", clamp01((sec - 0.1) / 0.5));
    beats(sec, &[
        (0.0, "The real app — every pixel on screen is drawn by vieww."),
        (1.6, "Open a file: a quick sketch of your app's screens."),
        (3.6, "Turn on live preview…"),
        (6.4, "…and your app appears on a simulated iPhone."),
        (9.8, "Files, code, preview and tools — all in one window."),
    ]);

    // The focus timeline, in the application's space.
    let keys: [(f32, f32, Rect, &str); 8] = [
        (1.6, 3.4, app::TAB_LIVE, ""),
        (3.6, 6.2, app::DIALOG, ""),
        (6.4, 9.6, app::PHONE, ""),
        (9.8, 11.2, app::EXPLORER, "explorer"),
        (11.2, 12.6, app::EDITOR, "editor"),
        (12.6, 14.0, app::PREVIEW, "preview"),
        (14.0, 15.4, app::PANEL, "panel"),
        (15.4, 99.0, app::FULL, ""),
    ];
    let cur = keys.iter().position(|(a, b, _, _)| sec >= *a && sec < *b);
    let mut stack = Stack::new();
    if let Some(i) = cur {
        let (from, _, r, label) = keys[i];
        let since = sec - from;
        let prev = if i > 0 && keys[i - 1].1 >= from - 0.3 && r != app::FULL { Some(keys[i - 1].2) } else { None };
        let m = ease_in_out(clamp01(since / 0.45));
        let rr = match prev {
            Some(p) => lerp_rect(p, r, m),
            // The last key lets the previous ring go rather than drawing
            // one around the whole window.
            None if r == app::FULL => keys[i - 1].2,
            None => r,
        };
        let a = if prev.is_some() { 1.0 } else { ease_out_cubic(clamp01(since / 0.3)) };
        let a = if r == app::FULL { 1.0 - clamp01(since / 0.4) } else { a };
        stack = stack.push(paint(move |book| ring(book, rr.inflate(3.0), 8.0, ACCENT, a, 3.0)));
        if !label.is_empty() {
            let la = ease_out_cubic(clamp01((since - 0.3) / 0.3));
            stack = stack.push(tag(r.left + 16.0, r.top + 14.0, label, 30.0, ACCENT, la));
        }
    }

    (frame::receipts(&[("the real app", ACCENT), ("real code", SYN_TYPE), ("live preview", SYN_STRING)], 0.0, 0.0, clamp01((sec - 1.0) / 0.6)));
    stack.into()
}

// ── Z12 · the_shell ─────────────────────────────────────────────────────────

/// The shell's regions: (rect, name, what it is, colour).
const REGIONS: [(Rect, &str, &str, Color); 7] = [
    (app::TITLE, "title bar", "menus and search", MUTED),
    (app::ACTIVITY, "tools", "every tool, one rail", SYN_TYPE),
    (app::EXPLORER, "files", "your project", ACCENT),
    (app::EDITOR, "code", "where you write", BRAND_NEAR),
    (app::PREVIEW, "preview", "your app, running", LEDGER),
    (app::PANEL, "panel", "messages and builds", SYN_MACRO),
    (app::STATUS, "status bar", "where you are", MUTED),
];

/// Where a region sits when the shell is `e` of the way apart, in the
/// application's space: pushed out from the window's centre, shrunk about
/// its own, and the whole drawing drawn in so it never leaves the body.
fn exploded(r: Rect, e: f32) -> Rect {
    let c = Offset::new((r.left + r.right) * 0.5, (r.top + r.bottom) * 0.5);
    let big = Offset::new(960.0, 540.0);
    let push = 0.055 * e;
    let nc = Offset::new(c.dx + (c.dx - big.dx) * push, c.dy + (c.dy - big.dy) * push);
    // Thin bars keep their height; panes shrink a little about their centre.
    let f = 1.0 - 0.06 * e;
    let (w, h) = (r.width() * f, r.height() * (if r.height() < 60.0 { 1.0 } else { f }));
    let g = 1.0 - 0.035 * e;
    let gc = Offset::new(big.dx + (nc.dx - big.dx) * g, big.dy + (nc.dy - big.dy) * g);
    Rect::new(gc.dx - w * g * 0.5, gc.dy - h * g * 0.5, gc.dx + w * g * 0.5, gc.dy + h * g * 0.5)
}

/// **One shell, taken apart.** The real window's seven regions separate —
/// each is the application's own pixels for that region, moved — name
/// themselves, and close again.
pub fn the_shell(ctx: &pf::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    frame::boxed(BODY);
    frame::headline("Everything in one window", clamp01((sec - 0.1) / 0.5));
    beats(sec, &[
        (0.0, "Design, code, preview and ship — without switching tools."),
        (4.0, "Every part of this window is itself built with vieww."),
        (9.6, "Nothing else to install."),
    ]);
    let e = {
        let a = clamp01((sec - 1.0) / 1.6);
        let b = clamp01((sec - 10.2) / 1.6);
        ease_in_out(a) * (1.0 - ease_in_out(b))
    };
    let mut stack = Stack::new();
    for (i, (r, name, what, color)) in REGIONS.iter().enumerate() {
        let dst = a2s(exploded(*r, e));
        plate(*r, dst, 1.0, if e > 0.02 { 8.0 * e } else { 0.0 }, e > 0.02);
        let la = ease_out_cubic(clamp01((sec - 2.6 - i as f32 * 0.22) / 0.35)) * (1.0 - clamp01((sec - 9.6) / 0.4));
        if la <= 0.01 {
            continue;
        }
        let (name, what, color) = (*name, *what, *color);
        let big = dst.height() > 120.0 && dst.width() > 150.0;
        if big {
            // Inside the pane, top-left, on its own dark card.
            let x = dst.left + 12.0;
            let y = dst.top + 12.0;
            stack = stack.push(paint(move |book| ring(book, dst, 8.0, color, la * 0.9, 2.0)));
            stack = stack.push(card_tag(x, y, name, what, color, la));
        } else if dst.height() <= 60.0 {
            // A bar: its tag sits on the bar.
            let w = frame::chip_w(name, 18.0) + 8.0;
            let cx = dst.left + dst.width() * 0.62;
            stack = stack.push(paint(move |book| ring(book, dst, 6.0, color, la * 0.9, 2.0)));
            stack = stack.push(frame::chip_at(cx - w * 0.5, (dst.top + dst.bottom) * 0.5 - 17.0, w, 34.0, name.to_string(), 18.0, color, la));
        } else {
            // The activity rail: its tag sits outside, to its left.
            let w = frame::chip_w(name, 20.0) + 8.0;
            stack = stack.push(paint(move |book| ring(book, dst, 6.0, color, la * 0.9, 2.0)));
            stack = stack.push(frame::chip_at(dst.left - 14.0 - w, dst.top + dst.height() * 0.30, w, 38.0, name.to_string(), 20.0, color, la));
        }
    }
    (frame::receipts(&[("one window", ACCENT), ("no terminal", SYN_TYPE), ("nothing else to install", LEDGER)], 0.0, 0.0, clamp01((sec - 3.0) / 0.6)));
    stack.into()
}

// ── Z13 · live_compose ──────────────────────────────────────────────────────

/// **Edit a line — see the picture change.** The real editor's `screen
/// "Home"` block and the real preview, magnified side by side. Two real
/// edits land in the buffer; what the planner repainted is marked on the
/// phone, and a thread ties each edited line to it.
pub fn live_compose(ctx: &pf::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    frame::boxed(BODY);
    frame::headline("Change a line — watch the app update", clamp01((sec - 0.1) / 0.5));
    beats(sec, &[
        (0.0, "Your code on the left, your app on the right."),
        (1.2, "Red shows exactly what gets redrawn."),
        (3.4, "Change the title — only the title redraws."),
        (7.6, "Change a row — only that row redraws."),
        (11.0, "No restart, no waiting — it updates as you type."),
    ]);

    // The editor's block, magnified — lines 10–17.
    let es = 1.5;
    let ed = place(app::LIVE_HOME, Offset::new(BODY.left + 24.0, BODY.top + 150.0), es);
    // The phone, magnified.
    let ps = 1.30;
    let pd = place(app::PHONE, Offset::new(BODY.right - 24.0 - app::PHONE.width() * ps, BODY.top + 18.0), ps);
    let in_e = ease_out_expo(clamp01((sec - 0.1) / 0.6));
    let in_p = ease_out_expo(clamp01((sec - 0.3) / 0.6));
    plate(app::LIVE_HOME, ed, in_e, 14.0, true);
    plate(app::PHONE, pd, in_p, 30.0, false);

    let mut stack = Stack::new();
    stack = stack.push(tag(ed.left, ed.top - 50.0, "your code", 20.0, SYN_TYPE, in_e));
    stack = stack.push(tag(pd.left - 330.0, ed.top - 50.0, "your app", 20.0, SYN_STRING, in_p));

    // The threads: edited line → what it repainted, in the magnified space.
    let map_e = |x: f32, y: f32| Offset::new(ed.left + (x - app::LIVE_HOME.left) * es, ed.top + (y - app::LIVE_HOME.top) * es);
    let map_p = |x: f32, y: f32| Offset::new(pd.left + (x - app::PHONE.left) * ps, pd.top + (y - app::PHONE.top) * ps);
    let edits: [(f32, Offset, Offset); 2] = [
        (3.4, map_e(620.0, app::line_y(12)), map_p(1600.0, 197.0)),
        (7.6, map_e(1110.0, app::line_y(15)), map_p(1700.0, 326.0)),
    ];
    for (k, (at, from, to)) in edits.iter().enumerate() {
        let p = clamp01((sec - at - 0.2) / 0.7);
        let fade = 1.0 - clamp01((sec - 13.2) / 0.6);
        if p <= 0.01 || fade <= 0.01 {
            continue;
        }
        let (from, to) = (*from, *to);
        let k_on = (k == 0 && sec < 7.6) || k == 1;
        let a = fade * if k_on { 1.0 } else { 0.35 };
        stack = stack.push(paint(move |book| {
            let pts = fk::thread_pts(from, to, -60.0);
            fk::grow_stroke(book, &pts, p, BREAK_RED, 2.0, 0.75 * a);
            if p >= 1.0 {
                fk::rider(book, &pts, (sec * 0.45) % 1.0, BREAK_RED, 4.0, 0.9 * a);
            }
            book.circle(from, 5.0, pf::alpha(BREAK_RED, 0.9 * a));
        }));
    }

    // What the planner repainted, on the phone: the title bar for the
    // first edit, the one row for the second.
    let damage: [(f32, Rect); 2] = [
        (3.4, Rect::new(1556.0, 180.0, 1808.0, 216.0)),
        (7.6, Rect::new(1556.0, 306.0, 1808.0, 346.0)),
    ];
    for (at, r) in damage {
        let since = sec - at;
        if !(0.0..3.4).contains(&since) {
            continue;
        }
        let a = ease_out_cubic(clamp01(since / 0.2)) * (1.0 - clamp01((since - 2.6) / 0.8));
        let d = Rect::new(pd.left + (r.left - app::PHONE.left) * ps, pd.top + (r.top - app::PHONE.top) * ps,
                          pd.left + (r.right - app::PHONE.left) * ps, pd.top + (r.bottom - app::PHONE.top) * ps);
        let pulse = 0.75 + 0.25 * (since * 7.0).sin().abs();
        stack = stack.push(paint(move |book| {
            book.rrect(d, 6.0, pf::alpha(BREAK_RED, 0.14 * a));
            book.stroke_rrect(d, 6.0, pf::alpha(BREAK_RED, 0.95 * a * pulse), 2.2);
        }));
    }

    // The damage report, under the editor.
    let rep = clamp01((sec - 4.4) / 0.6);
    let rows: [(&str, &str, Color); 3] = [
        ("areas redrawn per change", "1", BREAK_RED),
        ("parts rebuilt per change", "1", ACCENT),
        ("the rest of the screen", "untouched", LEDGER),
    ];
    for (i, (label, value, color)) in rows.iter().enumerate() {
        let p = ease_out_cubic(clamp01(rep * 1.6 - i as f32 * 0.2));
        if p <= 0.01 {
            continue;
        }
        let y = ed.bottom + 60.0 + i as f32 * 54.0;
        stack = stack.push(frame::label(ed.left, y, 560.0, 40.0, label.to_string(),
            pf::geist_mono(23.0).color(pf::alpha(MUTED, 0.95)), TextAlign::Left, p));
        stack = stack.push(frame::label(ed.left + 580.0, y, 320.0, 40.0, value.to_string(),
            pf::geist(30.0).bold().color(pf::alpha(*color, 0.98)), TextAlign::Left, p));
    }
    (frame::receipts(&[("no restart", ACCENT), ("no reload", SYN_TYPE), ("only what changed", LEDGER)], 0.0, 0.0, clamp01((sec - 11.0) / 0.6)));
    stack.into()
}

// ── Z14 · say_to_rust ───────────────────────────────────────────────────────

/// **`.say` → rust → cdylib → pixels.** counter.say, open in the real
/// editor, magnified. Its path through the real toolchain is drawn as six
/// stations; at the end of it, the counter arrives in the preview's own
/// phone — the real device frame, with the program the file describes.
pub fn say_to_rust(ctx: &pf::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    frame::boxed(BODY);
    frame::headline("Write it in plain English. Get a real app.", clamp01((sec - 0.1) / 0.5));
    beats(sec, &[
        (0.0, "This file describes a screen in everyday words."),
        (1.6, "The studio turns it into real, fast native code…"),
        (8.2, "…and runs it right in the preview."),
        (10.4, "Tap the button — it really counts."),
    ]);

    let cy = (BODY.top + BODY.bottom) * 0.5 + 20.0;
    let es = 1.34;
    let ed = place(app::SAY_CODE, Offset::new(BODY.left + 10.0, cy - app::SAY_CODE.height() * es * 0.5), es);
    let in_e = ease_out_expo(clamp01((sec - 0.4) / 0.6));
    plate(app::SAY_CODE, ed, in_e, 14.0, true);

    // The phone: the preview pane's own device, as the studio drew it
    // before anything was rendered into it (the snapshot the master takes
    // when it mounts the studio).
    let ps = 1.10;
    let pd = place(app::PHONE, Offset::new(BODY.right - 10.0 - app::PHONE.width() * ps, cy - app::PHONE.height() * ps * 0.5), ps);
    let in_p = ease_out_expo(clamp01((sec - 0.8) / 0.6));
    frame::plate(Plate { src: app::PHONE, dst: pd, alpha: in_p, radius: 30.0, snap: Some("blank"), card: false });
    let screen = Rect::new(
        pd.left + (app::PHONE_SCREEN.left - app::PHONE.left) * ps,
        pd.top + (app::PHONE_SCREEN.top - app::PHONE.top) * ps,
        pd.left + (app::PHONE_SCREEN.right - app::PHONE.left) * ps,
        pd.top + (app::PHONE_SCREEN.bottom - app::PHONE.top) * ps,
    );

    let mut stack = Stack::new();
    stack = stack.push(tag(ed.left, ed.top - 50.0, "counter.say — plain English", 20.0, SYN_TYPE, in_e));

    // The stations, in one column between the file and the phone.
    const STATIONS: [&str; 6] = ["your words", "Rust code", "compiled", "packaged", "loaded", "on screen"];
    let lit_at: [f32; 6] = [1.6, 3.0, 4.6, 5.8, 6.8, 7.8];
    let sw = 230.0;
    let sh = 62.0;
    let gap = 26.0;
    let col_x = (ed.right + pd.left) * 0.5;
    let col_top = cy - (6.0 * sh + 5.0 * gap) * 0.5;
    let centres: Vec<Offset> = (0..6).map(|i| Offset::new(col_x, col_top + sh * 0.5 + i as f32 * (sh + gap))).collect();
    let path_pts: Vec<Offset> = {
        let mut v = vec![Offset::new(ed.right, ed.top + 60.0)];
        v.extend(centres.iter().copied());
        v.push(Offset::new(pd.left + 30.0, (screen.top + screen.bottom) * 0.5));
        v
    };
    let carrier = clamp01((sec - 1.4) / 1.0);
    stack = stack.push(paint(move |book| {
        if carrier <= 0.01 {
            return;
        }
        let mut all: Vec<Offset> = Vec::new();
        for w in path_pts.windows(2) {
            all.extend(fk::thread_pts(w[0], w[1], 0.0));
        }
        fk::grow_stroke(book, &all, carrier, BRAND_NEAR, 2.0, 0.5);
        let u = clamp01((sec - 1.6) / 6.6);
        if u > 0.0 && u < 1.0 {
            fk::rider(book, &all, u, ACCENT, 5.0, 1.0);
        }
    }));
    for (i, name) in STATIONS.iter().enumerate() {
        let arrive = ease_out_cubic(clamp01((sec - lit_at[i] + 0.4) / 0.4));
        if arrive <= 0.01 {
            continue;
        }
        let lit = sec >= lit_at[i];
        let c = centres[i];
        let r = Rect::new(c.dx - sw * 0.5, c.dy - sh * 0.5, c.dx + sw * 0.5, c.dy + sh * 0.5);
        let glow = if lit { (1.0 - (sec - lit_at[i]) / 0.8).max(0.0) } else { 0.0 };
        stack = stack.push(paint(move |book| {
            // The lit pulse: a crisp expanding stroke around the station
            // the tap reached — a drawn ring, not a radial glow.
            if glow > 0.01 {
                let r2 = r.inflate(6.0 + 10.0 * glow);
                book.stroke_rrect(r2, 12.0 + 8.0 * glow, pf::alpha(ACCENT, 0.55 * glow), 1.5);
            }
            book.rrect(r, 12.0, pf::alpha(Color::rgb(0x18, 0x16, 0x1E), 0.97 * arrive));
            book.stroke_rrect(r, 12.0, pf::alpha(if lit { ACCENT } else { pf::FAINT }, (if lit { 0.9 } else { 0.5 }) * arrive), 1.5);
        }));
        stack = stack.push(frame::label(r.left, r.top, r.width(), r.height(), name.to_string(),
            pf::geist_mono(23.0).color(pf::alpha(if lit { INK } else { MUTED }, 0.97)), TextAlign::Center, arrive));
    }

    // The counter, in the preview's own phone: the studio's light app
    // style — the same type, blue and spacing its Inbox sketch uses.
    let presses: [f32; 2] = [10.8, 12.4];
    let count = presses.iter().filter(|p| sec >= **p).count();
    let press = presses.iter().map(|p| if sec >= *p { clamp01(1.0 - (sec - p) / 0.5) } else { 0.0 }).fold(0.0f32, f32::max);
    let app_a = ease_out_cubic(clamp01((sec - 8.2) / 0.5));
    if app_a > 0.01 {
        let s = screen;
        let blue = Color::rgb(0x1A, 0x4F, 0xE0);
        let u = ps;
        let btn = Rect::new(s.left + 14.0 * u, s.top + 150.0 * u, s.right - 14.0 * u, s.top + 184.0 * u);
        stack = stack.push(paint(move |book| {
            book.rect(Rect::new(s.left, s.top + 42.0 * u, s.right, s.bottom - 30.0 * u), pf::alpha(Color::WHITE, app_a));
            let k = 1.0 - 0.03 * press;
            let b = Rect::new(
                btn.left + btn.width() * (1.0 - k) * 0.5,
                btn.top + btn.height() * (1.0 - k) * 0.5,
                btn.right - btn.width() * (1.0 - k) * 0.5,
                btn.bottom - btn.height() * (1.0 - k) * 0.5,
            );
            book.rrect(b, 6.0 * u, pf::alpha(blue, app_a));
            if press > 0.02 {
                book.ring(Offset::new((b.left + b.right) * 0.5, (b.top + b.bottom) * 0.5), 60.0 + 80.0 * (1.0 - press), 2.0, pf::alpha(blue, 0.5 * press));
            }
        }));
        stack = stack.push(frame::label(s.left + 14.0 * u, s.top + 40.0 * u, s.width() - 28.0 * u, 40.0 * u, "Counter".to_string(),
            pf::geist(22.0 * u).bold().color(pf::alpha(Color::rgb(0x11, 0x11, 0x14), app_a)), TextAlign::Left, 1.0));
        stack = stack.push(frame::label(s.left + 14.0 * u, s.top + 92.0 * u, s.width() - 28.0 * u, 30.0 * u, format!("Tapped {count} times"),
            pf::geist(15.0 * u).color(pf::alpha(Color::rgb(0x33, 0x33, 0x3A), app_a)), TextAlign::Left, 1.0));
        stack = stack.push(frame::label(btn.left, btn.top, btn.width(), btn.height(), "Add one".to_string(),
            pf::geist(13.0 * u).color(pf::alpha(Color::WHITE, app_a)), TextAlign::Center, 1.0));
    }
    stack = stack.push(tag(pd.left, pd.top - 50.0, "the preview", 20.0, SYN_STRING, in_p));

    (frame::receipts(&[("plain English in", SYN_MACRO), ("native code out", SYN_TYPE), ("no browser inside", LEDGER)], 0.0, 0.0, clamp01((sec - 9.0) / 0.6)));
    stack.into()
}

// ── Z15 · ships_everywhere ──────────────────────────────────────────────────

/// The preview's device, per platform — measured from the studio's own
/// preview pane (`sfprobe`): the bezel, not the pane.
pub fn device_src(key: &str) -> Rect {
    match key {
        "android" | "android_dark" => Rect::new(1549.0, 122.0, 1814.0, 702.0),
        "desktop" | "desktop_dark" => Rect::new(1467.0, 277.0, 1898.0, 554.0),
        _ => app::PHONE,
    }
}

/// **One tree, every device.** The preview pane, captured on each of the
/// studio's three platforms — the same sketch, re-framed by the platform
/// the studio tells it it is on. Then the preview's Dark switch, all three
/// at once.
pub fn ships_everywhere(ctx: &pf::Ctx) -> WidgetNode {
    use viewwstudio::state::Platform;
    let sec = ctx.sec;
    frame::boxed(BODY);
    frame::headline("One design. Every device.", clamp01((sec - 0.1) / 0.5));
    beats(sec, &[
        (0.0, "The same app on iPhone, Android and desktop."),
        (4.0, "Each one fits its device — nothing rewritten."),
        (8.0, "Flip to dark mode — all three follow."),
    ]);
    let dark = ease_in_out(clamp01((sec - 8.0) / 0.9));
    let devs: [(&'static str, &'static str, &str, Platform, f32); 3] = [
        ("ios", "ios_dark", "iOS", Platform::Ios, 0.4),
        ("android", "android_dark", "Android", Platform::Android, 1.4),
        ("desktop", "desktop_dark", "Desktop", Platform::Desktop, 2.4),
    ];
    let mut stack = Stack::new();
    // Phones share a height; the window sits on the phones' centre line.
    let h_phone = 600.0;
    let top = BODY.top + 30.0;
    let widths: Vec<f32> = devs
        .iter()
        .map(|(k, _, _, p, _)| {
            let src = device_src(k);
            if *p == Platform::Desktop { 640.0 } else { src.width() * h_phone / src.height() }
        })
        .collect();
    let gap = 140.0;
    let total: f32 = widths.iter().sum::<f32>() + gap * 2.0;
    let mut x = (BODY.left + BODY.right - total) * 0.5;
    for (i, (light, darkk, name, plat, at)) in devs.into_iter().enumerate() {
        let src = device_src(light);
        let w = widths[i];
        let s = w / src.width();
        let h = src.height() * s;
        let y = if plat == Platform::Desktop { top + (h_phone - h) * 0.5 } else { top };
        let p = ease_out_expo(clamp01((sec - at) / 0.7));
        let rise = (1.0 - p) * 30.0;
        let dst = Rect::new(x, y + rise, x + w, y + h + rise);
        if dark < 0.999 {
            snap_plate(light, src, dst, p, 18.0);
        }
        if dark > 0.001 {
            snap_plate(darkk, src, dst, p * dark, 18.0);
        }
        let la = ease_out_cubic(clamp01((sec - at - 0.4) / 0.4));
        stack = stack.push(frame::label(x - 60.0, top + h_phone + 24.0, w + 120.0, 44.0, name.to_string(),
            pf::geist(32.0).bold().color(pf::alpha(INK, 0.97)), TextAlign::Center, la));
        stack = stack.push(frame::label(x - 80.0, top + h_phone + 70.0, w + 160.0, 30.0, plat.describe(),
            pf::geist_mono(20.0).color(pf::alpha(MUTED, 0.92)), TextAlign::Center, la));
        x += w + gap;
    }
    (frame::receipts(&[("3 platforms, 1 design", ACCENT), ("one switch, every device", LEDGER)], 0.0, 0.0, clamp01((sec - 4.0) / 0.6)));
    stack.into()
}

// ── Z16 · the_devices ───────────────────────────────────────────────────────

/// **Every size, before it ships.** The Devices tab — the real list, the
/// real selection moving down it — beside the preview each choice
/// produces: a small phone, a tablet, a laptop, an Android phone.
pub fn the_devices(ctx: &pf::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    frame::boxed(BODY);
    frame::headline("Check every screen size", clamp01((sec - 0.1) / 0.5));
    let devs = super::script::z16_devices();
    beats(sec, &[
        (0.0, "Pick a device — see your app at that size."),
        (2.4, "A small phone — where layouts break first."),
        (5.0, "A tablet — does it adapt?"),
        (7.6, "A laptop — the same app in a desktop window."),
        (10.2, "An Android phone — and it behaves like Android."),
    ]);
    // The list, magnified, on the left.
    let ls = 1.55;
    let ld = place(app::DEVICES_LIST, Offset::new(BODY.left + 40.0, BODY.top + 70.0), ls);
    let in_l = ease_out_expo(clamp01((sec - 0.7) / 0.6));
    plate(app::DEVICES_LIST, ld, in_l, 14.0, true);

    // The preview for the current choice, crossfading from the last.
    let cur = devs.iter().rposition(|(at, _)| sec >= *at).unwrap_or(0);
    let since = sec - devs[cur].0;
    let ps = 1.22;
    let pd = centred(app::STAGE, Offset::new((ld.right + BODY.right) * 0.5 + 10.0, (BODY.top + BODY.bottom) * 0.5 + 20.0), ps);
    let fade = ease_in_out(clamp01(since / 0.5));
    if cur > 0 && fade < 0.999 {
        snap_plate(super::script::device_key(devs[cur - 1].1), app::STAGE, pd, 1.0 - fade, 18.0);
    }
    let a_now = if cur > 0 { fade } else { ease_out_expo(clamp01(sec / 0.6)) };
    snap_plate(super::script::device_key(devs[cur].1), app::STAGE, pd, a_now, 18.0);

    let mut stack = Stack::new();
    stack = stack.push(tag(ld.left, ld.top - 50.0, "device sizes", 20.0, SYN_TYPE, in_l));
    stack = stack.push(tag(pd.left, pd.top - 50.0, "your app at that size", 20.0, SYN_STRING, 1.0));
    (frame::receipts(&[("8 device sizes", ACCENT), ("portrait and landscape", SYN_TYPE), ("notches and edges simulated", LEDGER)], 0.0, 0.0, clamp01((sec - 3.0) / 0.6)));
    stack.into()
}

// ── Z17 · build_and_ship ────────────────────────────────────────────────────

/// **And then it ships.** The studio's own panel — its real tab strip —
/// with a build running in it: five stages, the log a `cargo` build
/// prints, and three artifacts, each shown on the preview the studio
/// draws for its platform.
pub fn build_and_ship(ctx: &pf::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    frame::boxed(BODY);
    frame::headline("One click to ship", clamp01((sec - 0.1) / 0.5));
    beats(sec, &[
        (0.0, "Build for every platform from the same window."),
        (1.0, "Check, build, test, optimise, package."),
        (9.4, "Three real apps — iPhone, Android and desktop."),
    ]);

    const STAGES: [(Ico, &str, f32, f32); 5] = [
        (Ico::Check, "check", 1.0, 2.0),
        (Ico::Code, "build", 2.0, 5.2),
        (Ico::Shield, "test", 5.2, 6.8),
        (Ico::Tune, "optimise", 6.8, 8.0),
        (Ico::Export, "package", 8.0, 9.2),
    ];
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

    // The panel: the real tab strip, and under it the panel's body in the
    // studio's own colours with the build's log. The body is drawn *under*
    // the plates, so the real tab strip sits on it.
    let theme = viewwstudio::StudioTheme::dark();
    let pw = 1000.0;
    let ps = 1.4;
    let panel = Rect::new(BODY.left + 30.0, BODY.top + 130.0, BODY.left + 30.0 + pw, BODY.bottom - 20.0);
    let tabs = Rect::new(panel.left + 2.0, panel.top + 2.0, panel.left + 2.0 + app::PANEL_TABS.width() * ps, panel.top + 2.0 + app::PANEL_TABS.height() * ps);
    let in_p = ease_out_expo(clamp01((sec - 0.2) / 0.6));
    let body_bg = theme.chrome_1;
    let mut stack = Stack::new();
    frame::under(paint(move |book| {
        book.shadow(Rect::new(panel.left, panel.top + 8.0, panel.right, panel.bottom + 8.0), 14.0,
            vieww_foundation::Shadow::new(pf::alpha(Color::BLACK, 0.5 * in_p), Offset::new(0.0, 12.0), 36.0));
        book.rrect(panel, 14.0, pf::alpha(body_bg, in_p));
        book.stroke_rrect(panel, 14.0, pf::alpha(Color::WHITE, 0.10 * in_p), 1.0);
    }));
    plate(app::PANEL_TABS, tabs, in_p, 0.0, false);

    // The stage rail, over the panel.
    let (rx0, rx1, ry) = (panel.left + 40.0, panel.right - 40.0, BODY.top + 76.0);
    let done = clamp01((sec - 1.0) / 8.2);
    stack = stack.push(paint(move |book| {
        book.rrect(pf::xywh(rx0, ry - 5.0, rx1 - rx0, 10.0), 5.0, pf::alpha(Color::WHITE, 0.06 * in_p));
        if done > 0.001 {
            book.rrect(pf::xywh(rx0, ry - 5.0, (rx1 - rx0) * done, 10.0), 5.0, pf::alpha(ACCENT, 0.9));
        }
        for (ico, _, from, to) in STAGES {
            let x = rx0 + (rx1 - rx0) * ((to - 1.0) / 8.2);
            let finished = sec >= to;
            let running = sec >= from && sec < to;
            let c = if finished { LEDGER } else if running { ACCENT } else { MUTED };
            book.circle(Offset::new(x, ry), 18.0, pf::alpha(Color::rgb(0x18, 0x16, 0x1E), in_p));
            book.ring(Offset::new(x, ry), 18.0, 1.5, pf::alpha(c, 0.9 * in_p));
            kit::icon_at(book, if finished { Ico::Check } else { ico }, Offset::new(x, ry), 18.0, c, 1.8, in_p);
        }
    }));
    for (_, name, _, to) in STAGES {
        let x = rx0 + (rx1 - rx0) * ((to - 1.0) / 8.2);
        stack = stack.push(frame::label(x - 90.0, ry - 60.0, 180.0, 30.0, name.to_string(),
            pf::geist_mono(19.0).color(pf::alpha(if sec >= to { LEDGER } else { INK }, 0.95)), TextAlign::Center, in_p));
    }

    // The log.
    let n = (((sec - 1.2).max(0.0) / 0.40) as usize).min(LOG.len());
    let row_h = 34.0;
    let visible = (((panel.bottom - tabs.bottom - 30.0) / row_h) as usize).max(1);
    let first = n.saturating_sub(visible);
    for (row, i) in (first..n).enumerate() {
        let (kind, line) = LOG[i];
        let y = tabs.bottom + 16.0 + row as f32 * row_h;
        let fresh = ease_out_cubic(clamp01(((sec - 1.2) / 0.40 - i as f32) / 0.5));
        let c = match kind { 0 => INK, 1 => MUTED, _ => LEDGER };
        let ico = match kind { 0 => Ico::ChevronRight, 1 => Ico::Code, _ => Ico::Check };
        let ic = match kind { 0 => MUTED, 1 => SYN_TYPE, _ => LEDGER };
        let (lx, ly) = (panel.left + 34.0, y + row_h * 0.5);
        stack = stack.push(paint(move |book| kit::icon_at(book, ico, Offset::new(lx, ly), 15.0, ic, 1.6, fresh)));
        stack = stack.push(frame::label(panel.left + 58.0, y, pw - 80.0, row_h, line.to_string(),
            pf::geist_mono(19.0).color(pf::alpha(c, 0.95)), TextAlign::Left, fresh));
    }

    // The three artifacts, each on the preview the studio draws for it.
    const OUT: [(&str, &str, &str, f32); 3] = [
        ("App.ipa", "ios", "iPhone", 9.4),
        ("app.apk", "android", "Android", 9.9),
        ("studio.bin", "desktop", "desktop", 10.4),
    ];
    let ax0 = panel.right + 60.0;
    let aw = BODY.right - 20.0 - ax0;
    for (i, (file, key, plat, at)) in OUT.iter().enumerate() {
        let p = ease_out_expo(clamp01((sec - at) / 0.7));
        if p <= 0.01 {
            continue;
        }
        let src = device_src(key);
        let (dst, lab_y) = if i < 2 {
            let h = 330.0;
            let s = h / src.height();
            let w = src.width() * s;
            let cx = ax0 + aw * (if i == 0 { 0.25 } else { 0.75 });
            (Rect::new(cx - w * 0.5, BODY.top + 20.0, cx + w * 0.5, BODY.top + 20.0 + h), BODY.top + 20.0 + h + 10.0)
        } else {
            let w = 420.0;
            let s = w / src.width();
            let h = src.height() * s;
            let cx = ax0 + aw * 0.5;
            let y = BODY.top + 450.0;
            (Rect::new(cx - w * 0.5, y, cx + w * 0.5, y + h), y + h + 10.0)
        };
        let dst = Rect::new(dst.left, dst.top + (1.0 - p) * 24.0, dst.right, dst.bottom + (1.0 - p) * 24.0);
        snap_plate(key, src, dst, p, 16.0);
        let cx = (dst.left + dst.right) * 0.5;
        stack = stack.push(frame::label(cx - 150.0, lab_y, 300.0, 32.0, file.to_string(),
            pf::geist_mono(22.0).color(pf::alpha(INK, 0.97)), TextAlign::Center, p));
        stack = stack.push(frame::label(cx - 150.0, lab_y + 32.0, 300.0, 26.0, plat.to_string(),
            pf::geist_mono(17.0).color(pf::alpha(MUTED, 0.9)), TextAlign::Center, p));
    }
    (frame::receipts(&[("one click", ACCENT), ("three platforms", SYN_TYPE), ("on your own machine", LEDGER)], 0.0, 0.0, clamp01((sec - 10.8) / 0.6)));
    stack.into()
}
