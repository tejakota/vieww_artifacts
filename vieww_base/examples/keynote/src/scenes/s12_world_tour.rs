//! **S12 · THE WORLD TOUR** — one session, three surfaces.
//!
//! The counter the session has been writing leaves the studio and appears on
//! three machines at once: a native desktop window, a browser running the
//! real DOM backend, and a 2019 Android phone. They are not three
//! screenshots — they are three renderers showing **one program**, and the
//! film proves it the only way that counts.
//!
//! A hand taps the phone. The phone's counter turns to **7**, and in the same
//! frame the desktop window and the browser turn to 7 as well. That is the
//! session's last touch, the top of the ladder that started at 1 in S06, and
//! the reason the witness counter exists at all.
//!
//! Beside the phone, the only numbers in the film that came from another
//! machine: **59.3 fps · 4.09 ms median**, on a Redmi Note 7 Pro (2019,
//! Adreno 612), from `ci/mobile/device-suite.sh`'s artifacts. They are
//! labelled as quoted, because they were not measured by this render — and
//! that distinction is the ledger's whole point.
//!
//! iOS is not here. The codebase's own docs dispute it, so the film does not
//! claim it.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Rect, Size, Sketchbook};
use vieww_widget::prelude::*;

use crate::film::Ctx;
use crate::kit::{
    alpha, bump, caption, clamp01, dust, ease_out_cubic, grain, ground, horizon, mix, painter, seg,
    smoothstep, spring, vignette, xywh, Type, AMBER, CYAN, CYAN_SOFT, H, INK, INK_SOFT, MINT,
    MUTED, VIOLET, VIOLET_SOFT, W,
};
use crate::studio::{self, compose, App};

/// The device-suite figures, quoted verbatim from the CI artifacts.
const DEVICE: &str = "Redmi Note 7 Pro · 2019 · Adreno 612";
const FPS: &str = "59.3 fps";
const FRAME_MS: &str = "4.09 ms";

/// The beats, in scene-seconds.
const DESKTOP_T: f32 = 0.60;
const BROWSER_T: f32 = 2.30;
const PHONE_T: f32 = 4.00;
/// The sixth touch — on the desktop.
const TAP6_T: f32 = 9.60;
/// **The seventh** — on the phone, and every surface feels it.
const TAP7_T: f32 = 17.76;
const BENCH_T: f32 = 12.40;

pub fn frame(ctx: &Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let frame_i = ctx.frame;

    let d_in = spring(seg(sec, DESKTOP_T, DESKTOP_T + 1.2), 13.0, 0.5);
    let b_in = spring(seg(sec, BROWSER_T, BROWSER_T + 1.2), 13.0, 0.5);
    let p_in = spring(seg(sec, PHONE_T, PHONE_T + 1.2), 13.0, 0.5);

    let tap6 = (TAP6_T..TAP6_T + 0.7).contains(&sec).then(|| seg(sec, TAP6_T, TAP6_T + 0.7));
    let tap7 = (TAP7_T..TAP7_T + 0.8).contains(&sec).then(|| seg(sec, TAP7_T, TAP7_T + 0.8));
    // The wave: the tap on the phone reaching the other two surfaces. It is
    // drawn as a travelling pulse along the link lines, so the propagation is
    // a thing you watch rather than a thing you are told.
    let wave = seg(sec, TAP7_T + 0.06, TAP7_T + 0.80);

    let bench = smoothstep(seg(sec, BENCH_T, BENCH_T + 0.9)) * (1.0 - smoothstep(seg(sec, 22.4, 23.2)));

    // The three stages. The desktop is the widest, the phone the tallest —
    // the shapes are the devices' own proportions, not three equal boxes.
    let base_y = H * 0.46;
    let desk = Rect::new(96.0, base_y - 200.0, 96.0 + 620.0, base_y + 190.0);
    let brow = Rect::new(766.0, base_y - 180.0, 766.0 + 560.0, base_y + 170.0);
    let phone = Rect::new(1420.0, base_y - 268.0, 1420.0 + 290.0, base_y + 316.0);

    let value = ctx.spine.witness;
    let app_for = |press: Option<f32>, compact: bool| App {
        value,
        alive: 1.0,
        press,
        compact,
        accent: VIOLET,
        spring: 0.5 + 0.35 * (ctx.abs * 1.1).sin(),
        ..App::default()
    };
    let a_desk = app_for(tap6, false);
    let a_brow = app_for(None, false);
    let a_phone = app_for(tap7, true);

    let bg = painter(move |book: &mut Sketchbook, size: Size| {
        ground(book, size, sec, 0.95);
        horizon(book, size, VIOLET, 0.8);
        dust(book, size, sec, 40, VIOLET_SOFT, 0.6);

        // The link lines: one session, drawn. They are laid before the
        // devices so the machines sit on top of their own wiring.
        let link = |g: &mut Sketchbook, from: Offset, to: Offset, a: f32| {
            if a <= 0.01 {
                return;
            }
            let mid = Offset::new((from.dx + to.dx) * 0.5, from.dy.max(to.dy) + 150.0);
            let mut p = vieww_foundation::Path::new();
            p.move_to(from);
            p.cubic_to(Offset::new(from.dx, mid.dy), Offset::new(to.dx, mid.dy), to);
            g.stroke(p, alpha(VIOLET, 0.20 * a), 1.4);
        };
        let d_anchor = Offset::new(desk.left + desk.width() * 0.5, desk.bottom);
        let b_anchor = Offset::new(brow.left + brow.width() * 0.5, brow.bottom);
        let p_anchor = Offset::new(phone.left + phone.width() * 0.5, phone.bottom);
        link(book, d_anchor, b_anchor, d_in.min(b_in));
        link(book, b_anchor, p_anchor, b_in.min(p_in));

        // The desktop.
        if d_in > 0.01 {
            let r = lift(desk, d_in);
            let body = studio::shell_desktop(book, r, VIOLET, 1.0);
            studio::app(book, body, &a_desk);
        }
        // The browser.
        if b_in > 0.01 {
            let r = lift(brow, b_in);
            let body = studio::shell_browser(book, r, 1.0);
            studio::app(book, body, &a_brow);
        }
        // The phone.
        if p_in > 0.01 {
            let r = lift(phone, p_in);
            let body = studio::shell_phone(book, r, 1.0);
            studio::app(book, body, &a_phone);
        }

        // The tap, and the wave it sends.
        if let Some(p) = tap6 {
            let cx = desk.left + desk.width() * 0.5;
            let cy = desk.top + 28.0 + (desk.height() - 28.0) * 0.40;
            let dial_r = ((desk.width()).min(desk.height() - 28.0) * 0.19).max(22.0);
            studio::touch(book, Offset::new(cx, cy + dial_r + 88.0), p, VIOLET_SOFT);
        }
        if let Some(p) = tap7 {
            let cx = phone.left + phone.width() * 0.5;
            let inner_h = phone.height() - 52.0;
            let cy = phone.top + 27.0 + inner_h * 0.40;
            let dial_r = (phone.width().min(inner_h) * 0.19).max(22.0);
            studio::touch(book, Offset::new(cx, cy + dial_r + 80.0), p, VIOLET_SOFT);
        }
        if wave > 0.001 && wave < 1.0 {
            // A pulse travelling right-to-left along the links.
            let e = ease_out_cubic(wave);
            let x = phone.left - (phone.left - desk.left) * e;
            book.blended_layer(1.0, 26.0, BlendMode::Plus, None, |g| {
                g.circle(
                    Offset::new(x, base_y + 250.0),
                    90.0 * (1.0 - e * 0.5),
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(CYAN_SOFT, 0.45 * (1.0 - e))),
                        (1.0, alpha(CYAN, 0.0)),
                    ]),
                );
            });
            // And the surfaces flash as it passes them.
            for (r, at) in [(desk, desk.left), (brow, brow.left), (phone, phone.left)] {
                let hit = 1.0 - ((x - at).abs() / 260.0).min(1.0);
                if hit > 0.02 {
                    book.rrect(r, 12.0, alpha(CYAN, 0.07 * hit));
                }
            }
        }

        // The seventh's own pulse through the whole frame.
        let seven = bump(seg(sec, TAP7_T + 0.1, TAP7_T + 1.4), 0.0, 1.0);
        if seven > 0.01 {
            book.rect(
                Rect::new(0.0, 0.0, size.width, size.height),
                alpha(mix(VIOLET, Color::WHITE, 0.4), 0.035 * seven),
            );
        }

        // The bench card beside the phone — the only figures in the film that
        // another machine produced, and the card says so.
        if bench > 0.01 {
            let card = xywh(brow.left + 30.0, brow.bottom + 104.0, 360.0, 132.0);
            book.shadow(card, 10.0, studio::elevation(2));
            book.rrect(card, 10.0, alpha(Color::rgb(0x18, 0x16, 0x14), 0.96 * bench));
            book.stroke_rrect(card, 10.0, alpha(AMBER, 0.45 * bench), 1.0);
            book.rrect(xywh(card.left, card.top, 3.0, card.height()), 1.5, alpha(AMBER, 0.9 * bench));
        }

        grain(book, size, frame_i, 0.012, 320);
        vignette(book, size, 0.95);
    });

    let mut nodes: Vec<WidgetNode> = Vec::new();

    // Each surface's own glyphs, and its label.
    let labels: [(Rect, f32, &str, &str, f32); 3] = [
        (desk, d_in, "native window", "vieww-platform-winit · Vulkan swapchain", 28.0),
        (brow, b_in, "the browser", "vieww-platform-web-dom · real DOM", 50.0),
        (phone, p_in, "android", "the same program · the same signal", 27.0),
    ];
    for (r, t, name, sub, bar) in labels {
        if t <= 0.02 {
            continue;
        }
        let lifted = lift(r, t);
        let body = Rect::new(lifted.left + 1.0, lifted.top + bar, lifted.right - 1.0, lifted.bottom - 1.0);
        let body = if std::ptr::eq(&r, &phone) { body } else { body };
        let app = if bar > 40.0 {
            App { value, alive: 1.0, spring: 0.5, ..App::default() }
        } else {
            App { value, alive: 1.0, spring: 0.5, compact: bar < 28.0, ..App::default() }
        };
        nodes.extend(studio::app_text(body, &app));
        nodes.push(
            Type::new(name)
                .size(21.0)
                .medium()
                .color(alpha(INK, 0.95 * t))
                .center()
                .at(lifted.left, lifted.bottom + 18.0)
                .width(lifted.width())
                .into(),
        );
        nodes.push(
            Type::new(sub)
                .mono()
                .size(12.0)
                .track(1.4)
                .color(alpha(MUTED, 0.85 * t))
                .center()
                .at(lifted.left - 60.0, lifted.bottom + 48.0)
                .width(lifted.width() + 120.0)
                .into(),
        );
    }

    // The bench card's text, with its provenance on the card.
    if bench > 0.01 {
        let card = xywh(brow.left + 30.0, brow.bottom + 104.0, 360.0, 132.0);
        nodes.push(
            Type::new(DEVICE)
                .mono()
                .size(11.5)
                .track(1.2)
                .color(alpha(AMBER, 0.95 * bench))
                .at(card.left + 18.0, card.top + 14.0)
                .width(card.width() - 34.0)
                .into(),
        );
        nodes.push(
            Type::new(FPS)
                .size(34.0)
                .medium()
                .color(alpha(INK, 0.97 * bench))
                .at(card.left + 18.0, card.top + 38.0)
                .width(180.0)
                .into(),
        );
        nodes.push(
            Type::new(FRAME_MS)
                .size(34.0)
                .medium()
                .color(alpha(INK, 0.97 * bench))
                .right()
                .at(card.right - 180.0, card.top + 38.0)
                .width(162.0)
                .into(),
        );
        nodes.push(
            Type::new("quoted from ci/mobile/device-suite.sh · not measured by this render")
                .mono()
                .size(10.0)
                .track(0.8)
                .color(alpha(MUTED, 0.9 * bench))
                .at(card.left + 18.0, card.bottom - 30.0)
                .width(card.width() - 34.0)
                .into(),
        );
    }

    // The counter, once, big, over the whole tour — the film's own witness,
    // not any one machine's.
    let chip_a = smoothstep(seg(sec, 1.4, 2.4));
    nodes.push(
        Type::new(format!("{}", ctx.spine.witness))
            .size(86.0)
            .bold()
            .color(alpha(INK, 0.95 * chip_a))
            .at(112.0, 96.0)
            .width(200.0)
            .into(),
    );
    nodes.push(
        Type::new(format!("witness · {}", crate::film::touch_label(ctx.spine.witness)))
            .mono()
            .size(13.0)
            .track(2.6)
            .color(alpha(VIOLET_SOFT, 0.9 * chip_a))
            .at(114.0, 188.0)
            .width(420.0)
            .into(),
    );
    nodes.push(
        Type::new("one session · three renderers · never restarted")
            .mono()
            .size(13.0)
            .track(2.2)
            .color(alpha(MUTED, 0.85 * chip_a))
            .at(114.0, 214.0)
            .width(520.0)
            .into(),
    );

    // The seventh's sentence.
    let seven_a = smoothstep(seg(sec, TAP7_T + 0.5, TAP7_T + 1.3)) * (1.0 - smoothstep(seg(sec, 23.0, 23.9)));
    if seven_a > 0.004 {
        nodes.push(
            Type::new("the phone was tapped · the desktop felt it")
                .size(38.0)
                .light()
                .color(alpha(INK, 0.97 * seven_a))
                .center()
                .banner(H * 0.845)
                .into(),
        );
    }

    nodes.push(caption(
        "ios is excluded · the codebase's own docs dispute it",
        smoothstep(seg(sec, 6.4, 7.4)) * (1.0 - smoothstep(seg(sec, 11.4, 12.2))) * 0.85,
    ));
    let _ = (INK_SOFT, MINT, W, clamp01);
    compose(bg, nodes)
}

/// A device's arrival: it rises and settles, rather than fading in — a fade
/// would be the film moving, and this is the machine arriving.
fn lift(r: Rect, t: f32) -> Rect {
    let dy = (1.0 - clamp01(t)) * 70.0;
    Rect::new(r.left, r.top + dy, r.right, r.bottom + dy)
}
