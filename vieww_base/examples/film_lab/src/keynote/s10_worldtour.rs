//! S10 · WORLD TOUR — the session leaves the desk. 1:51–2:05.
//!
//! The buffer leaves: the studio pulls back into the dark and the session
//! line **forks ×3** (E-22) — to a native window, to a browser's DOM, to
//! the phone in hand. Desktop tap → **6**; phone tap → **7** — and every
//! surface updates at once (K2): the same session, the same counter, the
//! same spring, on three platforms simultaneously.
//!
//! Caption: *"one session · every surface."* Shō closes on the witness at
//! 7 — on three platforms at once.

use vieww_foundation::{
    Color, FontWeight, Offset, Size, Sketchbook, TextAlign, TextStyle, Transform,
};
use vieww_widget::prelude::*;
use vieww_widget::Transformed;
use vieww_widget::{Opacity, PaintWith, Painting};

use crate::film_lib::{
    alpha, clamp01, ease_in_out, ease_out_back, tint, xywh, INK, MUTED, VIOLET, VIOLET_SOFT,
};

use super::{backdrop, caption, Ctx};

/// Scale about a point — the globe plate's helper, verbatim.
fn scale_about(cx: f32, cy: f32, s: f32) -> Transform {
    Transform::translate(Offset::new(cx * (1.0 - s), cy * (1.0 - s))).then(Transform::scale(s, s))
}

/// A device's anchor — where the fork lands.
const DESKTOP: Offset = Offset::new(390.0, 350.0);
const BROWSER: Offset = Offset::new(1544.0, 320.0);
const PHONE: Offset = Offset::new(956.0, 900.0);
/// Where the fork rises from — the pulled-back studio's preview.
const SOURCE: Offset = Offset::new(956.0, 470.0);

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;

    // The pull-back: the studio shrinks into the dark, 1.0 → 0.56.
    let back = ease_in_out(clamp01(t / 0.20));
    let k = 1.0 - 0.44 * back;

    // The studio scene continues underneath — live, not paused: the same
    // session chip, the same counter. It is one of the surfaces now.
    let spec = super::studio::Spec {
        code: super::studio::Code::Say {
            typed: 1.0,
            blink: ctx.sec,
        },
        app: {
            let mut app = super::studio::App::new(ctx.ladder, super::tap_pulse(abs), abs);
            app.alive = 1.0;
            app.dial = 1.0;
            app.spring = 1.0;
            app
        },
        session_line: 1.0,
        ..Default::default()
    };
    let small_studio = super::studio::studio(abs, ctx.ladder, spec);

    let mut stack = Stack::new()
        .push(Positioned::fill().child(backdrop(t, 0x70A, 90)))
        .push(
            Positioned::fill()
                .child(Transformed::new(scale_about(960.0, 540.0, k)).child(small_studio)),
        );

    // The devices arrive — 0.20 to 0.52, staggered, sprung.
    let dev_in = |t0: f32| ease_out_back(clamp01((t - t0) / 0.22));
    let d_desktop = dev_in(0.22);
    let d_browser = dev_in(0.30);
    let d_phone = dev_in(0.38);

    if d_desktop > 0.0 {
        stack = stack.push(device_desktop(d_desktop, ctx.ladder, abs));
    }
    if d_browser > 0.0 {
        stack = stack.push(device_browser(d_browser, ctx.ladder, abs));
    }
    if d_phone > 0.0 {
        stack = stack.push(device_phone(d_phone, ctx.ladder, abs));
    }

    // The fork — three progressive strokes from the source to each device
    // (E-22), lit as the devices land. On tap 7 (K2), all three pulse.
    let fork_progress = clamp01((t - 0.24) / 0.28);
    let k2_pulse = super::tap_pulse(abs) * if ctx.ladder >= 7 { 1.0 } else { 0.0 };
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            let targets = [(DESKTOP, d_desktop), (BROWSER, d_browser), (PHONE, d_phone)];
            for (idx, &(target, arrive)) in targets.iter().enumerate() {
                if arrive <= 0.0 {
                    continue;
                }
                // The path: a soft arc from source to device.
                let mid = Offset::new(
                    (SOURCE.dx + target.dx) * 0.5,
                    SOURCE.dy.min(target.dy) - 90.0,
                );
                let mut p = vieww_foundation::Path::new();
                p.move_to(SOURCE);
                for i in 1..=24 {
                    let u = i as f32 / 24.0;
                    let x = (1.0 - u) * (1.0 - u) * SOURCE.dx
                        + 2.0 * (1.0 - u) * u * mid.dx
                        + u * u * target.dx;
                    let y = (1.0 - u) * (1.0 - u) * SOURCE.dy
                        + 2.0 * (1.0 - u) * u * mid.dy
                        + u * u * target.dy;
                    p.line_to(Offset::new(x, y));
                }
                // The ghost of the whole fork.
                book.stroke(p.clone(), alpha(Color::WHITE, 0.05), 1.0);
                // The progressive stroke — the fork reaching, one
                // branch at a time (E-22).
                let stage = fork_progress * 3.0;
                let which = stage.floor() as i32;
                let frac = (stage - stage.floor()).clamp(0.0, 1.0);
                let drawn = if which > idx as i32 {
                    1.0
                } else if which == idx as i32 {
                    frac
                } else {
                    0.0
                };
                if drawn > 0.0 {
                    book.stroke_styled(
                        p,
                        alpha(VIOLET_SOFT, 0.9),
                        2.0,
                        vieww_foundation::StrokeStyle::default()
                            .dash(vieww_foundation::Dash::new(vec![2600.0 * drawn, 2600.0])),
                    );
                }
                // The landing node.
                if arrive > 0.9 {
                    let lit7 = k2_pulse;
                    book.circle(target, 5.0 + 3.0 * lit7, tint(VIOLET_SOFT, 0.2));
                    if lit7 > 0.0 {
                        book.ring(
                            target,
                            12.0 + 26.0 * (1.0 - lit7),
                            2.0,
                            alpha(VIOLET, lit7 * 0.8),
                        );
                    }
                }
            }
        }),
    )));

    // K2's caption + the closing line.
    stack = stack.push(caption(
        "one session · every surface",
        1000.0,
        clamp01((t - 0.86) / 0.08),
    ));

    stack.into()
}

// ── The three surfaces ──────────────────────────────────────────────────────

/// The mini counter every device shows — number, spring, button hint.
fn mini_counter(book: &mut Sketchbook, cx: f32, cy: f32, ladder: u32, abs: f32, scale: f32) {
    let _ = abs;
    let _num = ladder;
    // The number — the session's witness, same on every surface.
    let r = 34.0 * scale;
    book.circle(Offset::new(cx, cy), r, alpha(Color::rgb(18, 18, 24), 0.95));
    book.stroke(
        super::circle_path(cx, cy, r, 36),
        alpha(VIOLET_SOFT, 0.5),
        1.6,
    );
}

/// The desktop window — a native frame with the counter app inside.
fn device_desktop(arrive: f32, ladder: u32, abs: f32) -> WidgetNode {
    let s = arrive;
    let w = 420.0;
    let h = 300.0;
    let x = DESKTOP.dx - w * 0.5;
    let y = DESKTOP.dy - h * 0.5 - 40.0;

    let mut stack = Stack::new().push(Positioned::new().left(x).top(y).width(w).height(h).child(
        Opacity::new(s.clamp(0.0, 1.0)).child(Painting::sized(
            Size::new(w, h),
            PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
                book.rrect(
                    xywh(0.0, 0.0, w, h),
                    12.0,
                    alpha(Color::rgb(15, 15, 20), 0.97),
                );
                book.stroke_rrect(xywh(0.0, 0.0, w, h), 12.0, alpha(Color::WHITE, 0.10), 1.0);
                book.rect(
                    xywh(1.0, 1.0, w - 2.0, 30.0),
                    alpha(Color::rgb(20, 20, 26), 0.95),
                );
                for (i, c) in [
                    Color::rgb(255, 95, 86),
                    Color::rgb(255, 189, 46),
                    Color::rgb(39, 201, 63),
                ]
                .iter()
                .enumerate()
                {
                    book.circle(Offset::new(20.0 + i as f32 * 16.0, 15.0), 4.0, *c);
                }
                // The mini app — a column with the counter.
                mini_counter(book, w * 0.5, 130.0, ladder, abs, 1.0);
                // The button hint.
                book.rrect(
                    xywh(w * 0.5 - 62.0, 196.0, 124.0, 34.0),
                    8.0,
                    alpha(VIOLET, 0.18),
                );
                book.stroke_rrect(
                    xywh(w * 0.5 - 62.0, 196.0, 124.0, 34.0),
                    8.0,
                    alpha(VIOLET_SOFT, 0.5),
                    1.2,
                );
            }),
        )),
    ));
    stack = stack.push(
        Positioned::new()
            .left(x + w * 0.5 - 40.0)
            .top(y + 108.0)
            .width(80.0)
            .height(52.0)
            .child(
                Text::new(ladder.to_string())
                    .style(
                        TextStyle::new(40.0)
                            .monospace()
                            .weight(FontWeight::Medium)
                            .color(INK),
                    )
                    .align(TextAlign::Center),
            ),
    );
    stack = stack.push(
        Positioned::new()
            .left(x)
            .top(y + h + 10.0)
            .width(w)
            .height(22.0)
            .child(
                Text::new("native window · vieww")
                    .style(
                        TextStyle::new(13.0)
                            .monospace()
                            .letter_spacing(1.6)
                            .color(alpha(MUTED, 0.8)),
                    )
                    .align(TextAlign::Center),
            ),
    );
    stack.into()
}

/// The browser — a tab, an address bar, the DOM outline of the same app.
fn device_browser(arrive: f32, ladder: u32, abs: f32) -> WidgetNode {
    let w = 440.0;
    let h = 330.0;
    let x = BROWSER.dx - w * 0.5;
    let y = BROWSER.dy - h * 0.5 - 20.0;

    let mut stack = Stack::new().push(Positioned::new().left(x).top(y).width(w).height(h).child(
        Opacity::new(arrive.clamp(0.0, 1.0)).child(Painting::sized(
            Size::new(w, h),
            PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
                book.rrect(
                    xywh(0.0, 0.0, w, h),
                    12.0,
                    alpha(Color::rgb(16, 16, 21), 0.97),
                );
                book.stroke_rrect(xywh(0.0, 0.0, w, h), 12.0, alpha(Color::WHITE, 0.10), 1.0);
                // The tab.
                book.rrect(
                    xywh(14.0, 10.0, 150.0, 26.0),
                    7.0,
                    alpha(Color::rgb(26, 26, 34), 0.95),
                );
                // The address bar.
                book.rrect(
                    xywh(14.0, 44.0, w - 28.0, 26.0),
                    13.0,
                    alpha(Color::rgb(24, 24, 31), 0.95),
                );
                // The DOM outline — boxes as the web sees them.
                let mut rng = crate::film_lib::Rng::new(0xB0A);
                book.stroke_rrect(
                    xywh(24.0, 84.0, w - 48.0, 130.0),
                    8.0,
                    alpha(VIOLET, 0.22),
                    1.0,
                );
                for i in 0..4 {
                    let bw = 90.0 + rng.f01() * 160.0;
                    book.stroke_rrect(
                        xywh(38.0, 98.0 + i as f32 * 27.0, bw, 18.0),
                        4.0,
                        alpha(Color::WHITE, 0.10),
                        1.0,
                    );
                }
                mini_counter(book, w * 0.5, 252.0, ladder, abs, 0.85);
            }),
        )),
    ));
    stack = stack.push(
        Positioned::new()
            .left(x + w * 0.5 - 40.0)
            .top(y + 232.0)
            .width(80.0)
            .height(48.0)
            .child(
                Text::new(ladder.to_string())
                    .style(
                        TextStyle::new(36.0)
                            .monospace()
                            .weight(FontWeight::Medium)
                            .color(INK),
                    )
                    .align(TextAlign::Center),
            ),
    );
    stack = stack.push(
        Positioned::new()
            .left(x)
            .top(y + h + 10.0)
            .width(w)
            .height(22.0)
            .child(
                Text::new("web · the dom itself")
                    .style(
                        TextStyle::new(13.0)
                            .monospace()
                            .letter_spacing(1.6)
                            .color(alpha(MUTED, 0.8)),
                    )
                    .align(TextAlign::Center),
            ),
    );
    stack.into()
}

/// The phone — held, glowing, the session in a palm.
fn device_phone(arrive: f32, ladder: u32, abs: f32) -> WidgetNode {
    let w = 260.0;
    let h = 500.0;
    let x = PHONE.dx - w * 0.5;
    let y = PHONE.dy - h * 0.5;

    let pulse = super::tap_pulse(abs) * if ladder >= 7 { 1.0 } else { 0.0 };

    let mut stack = Stack::new().push(Positioned::new().left(x).top(y).width(w).height(h).child(
        Opacity::new(arrive.clamp(0.0, 1.0)).child(Painting::sized(
            Size::new(w, h),
            PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
                // The palm glow — the phone is held.
                if pulse > 0.0 {
                    super::glow(
                        book,
                        w * 0.5,
                        h * 0.5,
                        200.0 + 90.0 * pulse,
                        VIOLET,
                        0.35 * pulse,
                    );
                }
                // The body.
                book.rrect(
                    xywh(0.0, 0.0, w, h),
                    44.0,
                    alpha(Color::rgb(17, 17, 22), 0.98),
                );
                book.stroke_rrect(xywh(0.0, 0.0, w, h), 44.0, alpha(Color::WHITE, 0.14), 1.6);
                // The notch.
                book.rrect(
                    xywh(w * 0.5 - 46.0, 12.0, 92.0, 22.0),
                    11.0,
                    alpha(Color::rgb(8, 8, 10), 0.98),
                );
                // The app — heading, the counter, the button.
                book.rrect(
                    xywh(28.0, 92.0, w - 56.0, 300.0),
                    16.0,
                    alpha(Color::rgb(22, 22, 29), 0.9),
                );
                mini_counter(book, w * 0.5, 214.0, ladder, abs, 1.05);
                // The home bar.
                book.rrect(
                    xywh(w * 0.5 - 52.0, h - 22.0, 104.0, 5.0),
                    2.5,
                    alpha(Color::WHITE, 0.35),
                );
            }),
        )),
    ));
    stack = stack.push(
        Positioned::new()
            .left(x + w * 0.5 - 50.0)
            .top(y + 186.0)
            .width(100.0)
            .height(60.0)
            .child(
                Text::new(ladder.to_string())
                    .style(
                        TextStyle::new(48.0)
                            .monospace()
                            .weight(FontWeight::Medium)
                            .color(INK),
                    )
                    .align(TextAlign::Center),
            ),
    );
    stack = stack.push(
        Positioned::new()
            .left(x)
            .top(y + h + 12.0)
            .width(w)
            .height(22.0)
            .child(
                Text::new("the phone in hand")
                    .style(
                        TextStyle::new(13.0)
                            .monospace()
                            .letter_spacing(1.6)
                            .color(alpha(MUTED, 0.8)),
                    )
                    .align(TextAlign::Center),
            ),
    );
    stack.into()
}
