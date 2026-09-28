//! S16 · CROSS BUILD — the reference's "Step 4": onto real devices.
//! 2:30–2:42.
//!
//! The build button fires (tap 7); the pipeline rolls — crates roll up,
//! assets bundle, the packages sign — and the stage-build rule holds:
//! **under 15 seconds on stage**, because the studio's modularity keeps
//! the core pre-compiled (the plan's own optimization note). A phone and
//! a tablet descend; the `.apk` lands on the Android, the `.ipa` on the
//! iOS device; the counter runs on both — and the phone plays the swarm
//! at the device suite's own receipt: **59.3 fps · 4.09 ms median · on a
//! Redmi Note 7 Pro, 2019** (quoted from the device-suite CI artifacts,
//! printed without being touched).
//!
//! Tap 8 (the package lands, t≈0.80) fires here.

use vieww_foundation::{
    Color, Gradient, Offset, Rect, Size, Sketchbook, TextAlign, TextStyle, FontWeight,
};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};

use crate::film_lib::{alpha, clamp01, ease_in_out, ease_out_back, ease_out_cubic, mix, spring_out, tint, xywh, FAINT, INK, MUTED, Rng, VIOLET, VIOLET_SOFT, CYAN, CYAN_SOFT, MINT, AMBER};

use super::studio;
use super::{Ctx};

/// When the build fires and when the packages land (scene fractions).
const BUILD_T: f32 = 0.30;
const LAND_T: f32 = 0.80;

/// The build's staged duration — the plan's "under 15 s on stage" rule.
const BUILD_SECONDS: f32 = 14.8;

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;
    let ladder = ctx.ladder;
    let sec = ctx.sec;

    // The build progress: 0 → 1 over BUILD_T..LAND_T.
    let build_p = ease_in_out(clamp01((t - BUILD_T) / (LAND_T - BUILD_T - 0.04)));
    let landed = clamp01((t - LAND_T) / 0.10);

    // The build timer — mm:ss.d, derived from the same progress the bar
    // animates by (the receipt the stage keeps).
    let build_secs = build_p * BUILD_SECONDS;

    // The studio, dimmed slightly under the build overlay.
    let spec = studio::Spec {
        code: studio::Code::Say { typed: 1.0, blink: ctx.sec },
        app: {
            let mut app = studio::App::new(1, super::tap_pulse(abs), abs);
            app.dial = 1.0;
            app.spring = 1.0;
            app
        },
        session_line: 1.0,
        ..Default::default()
    };

    let mut stack = Stack::new()
        .push(Positioned::fill().child(studio::studio(abs, ladder, spec)));

    // ── The build rail — the pipeline rolling, over the editor.
    let rail_a = clamp01((t - BUILD_T + 0.04) / 0.08);
    if rail_a > 0.0 {
        stack = stack.push(
            Positioned::new()
                .left(studio::ED_X0 + 30.0)
                .top(150.0)
                .width(studio::ED_X1 - studio::ED_X0 - 60.0)
                .height(330.0)
                .child(Opacity::new(rail_a).child(Painting::sized(
                    Size::new(studio::ED_X1 - studio::ED_X0 - 60.0, 330.0),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        book.rrect(xywh(0.0, 0.0, _s.width, 330.0), 16.0, alpha(Color::rgb(13, 14, 20), 0.96));
                        book.stroke_rrect(xywh(0.0, 0.0, _s.width, 330.0), 16.0, alpha(VIOLET_SOFT, 0.4), 1.3);
                        // The header.
                        book.rect(xywh(1.0, 1.0, _s.width - 2.0, 48.0), alpha(Color::rgb(16, 17, 24), 0.9));
                        book.rect(xywh(0.0, 48.0, _s.width, 1.0), alpha(Color::WHITE, 0.06));
                        // The rolling log — the crates rolling up.
                        let log = [
                            "→ rolling up 36 crates (core cached)",
                            "→ bundling assets · icons · fonts",
                            "→ cargo matrix · aarch64-linux-android",
                            "→ cargo matrix · aarch64-apple-ios",
                            "→ signing counter.apk (debug)",
                            "→ packing counter.ipa",
                        ];
                        for (i, line) in log.iter().enumerate() {
                            let t0 = i as f32 / log.len() as f32;
                            let lit = build_p >= t0;
                            if !lit {
                                continue;
                            }
                            let y = 74.0 + i as f32 * 34.0;
                            // A mono-width bar of light rects — the log line
                            // (text nodes render above; here the rail's
                            // texture).
                            let _ = line;
                            book.circle(Offset::new(28.0, y + 9.0), 3.0, alpha(MINT, 0.85));
                            book.rect(xywh(44.0, y + 3.0, 300.0, 1.6), alpha(Color::WHITE, 0.05));
                        }
                        // The progress bar.
                        let y = 292.0;
                        book.rrect(xywh(24.0, y, _s.width - 48.0, 10.0), 5.0, alpha(Color::WHITE, 0.07));
                        book.rrect(
                            xywh(24.0, y, (_s.width - 48.0) * build_p, 10.0),
                            5.0,
                            Gradient::horizontal().with_dither().with_stops(&[
                                (0.0, alpha(VIOLET, 0.9)),
                                (1.0, alpha(tint(VIOLET_SOFT, 0.2), 0.95)),
                            ]),
                        );
                    }),
                ))),
        );
        // The log lines as text.
        let log = [
            "rolling up 36 crates (core cached)",
            "bundling assets · icons · fonts",
            "cargo matrix · aarch64-linux-android",
            "cargo matrix · aarch64-apple-ios",
            "signing counter.apk (debug)",
            "packing counter.ipa",
        ];
        for (i, line) in log.iter().enumerate() {
            let t0 = i as f32 / log.len() as f32;
            if build_p < t0 {
                continue;
            }
            stack = stack.push(
                Positioned::new()
                    .left(studio::ED_X0 + 30.0 + 48.0)
                    .top(150.0 + 74.0 + i as f32 * 34.0 - 9.0)
                    .width(560.0)
                    .height(24.0)
                    .child(Opacity::new(rail_a).child(
                        Text::new(*line)
                            .style(TextStyle::new(16.0).monospace().color(alpha(INK, 0.88))),
                    )),
            );
        }
        // The timer.
        stack = stack.push(
            Positioned::new()
                .left(studio::ED_X0 + 30.0 + 24.0)
                .top(150.0 + 236.0)
                .width(300.0)
                .height(30.0)
                .child(Opacity::new(rail_a).child(
                    Text::new(format!("{:02}.{} s", build_secs as u32, ((build_secs % 1.0) * 10.0) as u32))
                        .style(TextStyle::new(24.0).monospace().color(alpha(INK, 0.92))),
                )),
        );
        stack = stack.push(
            Positioned::new()
                .left(studio::ED_X0 + 30.0 + 130.0)
                .top(150.0 + 240.0)
                .width(400.0)
                .height(26.0)
                .child(Opacity::new(rail_a).child(
                    Text::new("the staged build — under 15 s")
                        .style(TextStyle::new(14.5).monospace().color(alpha(MUTED, 0.85))),
                )),
        );
    }

    // ── The devices — a phone and a tablet descend as the build completes.
    let dev_in = ease_out_back(clamp01((t - (LAND_T - 0.14)) / 0.18));
    if dev_in > 0.0 {
        let rise = (1.0 - dev_in) * 160.0;
        // The phone (Android) — left, carrying the .apk.
        stack = stack.push(
            Positioned::new()
                .left(1130.0)
                .top(320.0 + rise)
                .width(324.0)
                .height(650.0)
                .child(device(300.0, 600.0, ".apk", MINT, landed, sec, true)),
        );
        // The tablet (iOS) — right, carrying the .ipa.
        stack = stack.push(
            Positioned::new()
                .left(1500.0)
                .top(340.0 + rise)
                .width(364.0)
                .height(530.0)
                .child(device(340.0, 480.0, ".ipa", CYAN_SOFT, landed, sec, false)),
        );
    }

    // The package flight — the .apk / .ipa chips arcing from the rail to
    // the devices at LAND_T.
    if t >= LAND_T - 0.04 && landed < 1.0 {
        let fly = ease_in_out(landed);
        for (i, (ext, tx, ty)) in [(".apk", 1280.0, 620.0), (".ipa", 1670.0, 580.0)].iter().enumerate() {
            let sx = studio::ED_X0 + 300.0;
            let sy = 400.0;
            let x = sx + (tx - sx) * fly;
            let y = sy + (ty - sy) * fly - (1.0 - (fly - 0.5).abs() * 2.0) * 90.0;
            stack = stack.push(
                Positioned::new()
                    .left(x - 40.0)
                    .top(y - 20.0)
                    .width(80.0)
                    .height(40.0)
                    .child(Opacity::new(1.0 - landed * 0.6).child(super::chip(
                        *ext,
                        16.0,
                        if i == 0 { tint(MINT, 0.1) } else { tint(CYAN_SOFT, 0.1) },
                    ))),
            );
        }
    }

    // The device receipt — the CI artifacts' own numbers, quoted.
    let receipt_a = clamp01((t - (LAND_T + 0.06)) / 0.14);
    if receipt_a > 0.0 {
        stack = stack.push(
            Positioned::new()
                .left(studio::PV_X0 + 40.0)
                .top(studio::TITLE_H + 60.0 + studio::PV_H - 640.0)
                .width(720.0)
                .height(40.0)
                .child(Opacity::new(receipt_a).child(super::chip(
                    "59.3 fps · 4.09 ms median · Redmi Note 7 Pro (2019) — the device suite's receipt",
                    13.5,
                    tint(MINT, 0.15),
                ))),
        );
    }

    stack = stack.push(super::caption(
        "one codebase. every device. one click.",
        1000.0,
        clamp01((t - 0.10) / 0.12),
    ));
    stack = stack.push(super::caption(
        "the .apk and .ipa, built from the same buffer you watched type",
        964.0,
        clamp01((t - (LAND_T + 0.04)) / 0.12),
    ));

    stack.into()
}

/// A device frame — the phone or tablet, its screen showing the counter
/// (and, on the phone, a whisper of the swarm), its package label below.
fn device(
    w: f32,
    h: f32,
    ext: &str,
    col: Color,
    landed: f32,
    sec: f32,
    is_phone: bool,
) -> WidgetNode {
    let ext_owned = ext.to_string();
    let body = Painting::sized(
        Size::new(w + 24.0, h + 24.0),
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            // The shadow.
            book.shadow(
                xywh(8.0, 14.0, w + 8.0, h + 8.0),
                26.0,
                vieww_foundation::Shadow::new(alpha(Color::BLACK, 0.5), Offset::new(0.0, 22.0), 52.0),
            );
            // The body.
            book.rrect(xywh(0.0, 0.0, w, h), 30.0, alpha(Color::rgb(18, 19, 25), 0.98));
            book.stroke_rrect(xywh(0.0, 0.0, w, h), 30.0, alpha(Color::WHITE, 0.16), 1.6);
            // The screen.
            let scr = xywh(10.0, 10.0, w - 20.0, h - 20.0);
            book.rrect(scr, 22.0, alpha(Color::rgb(10, 10, 14), 0.98));
            // The screen comes alive when the package lands.
            if landed > 0.0 {
                let a = clamp01(landed * 2.0);
                let cx = scr.left + scr.width() * 0.5;
                let cy = scr.top + scr.height() * 0.30;
                // The counter — the same witness number.
                let rr = 74.0 * if is_phone { 1.0 } else { 1.3 };
                book.circle(Offset::new(cx, cy), rr, alpha(Color::rgb(18, 18, 24), 0.95 * a));
                book.stroke(super::circle_path(cx, cy, rr, 40), alpha(col, 0.6 * a), 2.4);
                // The button hint.
                book.rrect(xywh(cx - 70.0, cy + rr + 60.0, 140.0, 40.0), 10.0, alpha(VIOLET, 0.22 * a));
                // The phone plays the swarm — a whisper of birds.
                if is_phone {
                    let mut rng = Rng::new(0x600D);
                    for _ in 0..46 {
                        let by = scr.top + scr.height() * 0.62 + rng.f01() * 70.0;
                        let drift = (sec * 40.0 + rng.f01() * 200.0).rem_euclid(scr.width() - 40.0);
                        book.circle(
                            Offset::new(scr.left + 20.0 + drift, by + (sec * 12.0 * rng.f01()).sin() * 4.0),
                            1.4,
                            alpha(INK, 0.5 * a),
                        );
                    }
                }
            }
        }),
    );
    Stack::new()
        .push(Positioned::new().left(0.0).top(0.0).width(w + 24.0).height(h + 24.0).child(body))
        .push(
            Positioned::new()
                .left(0.0)
                .top(h + 30.0)
                .width(w + 24.0)
                .height(26.0)
                .child(
                    Text::new(format!("{} — installed", ext_owned))
                        .style(TextStyle::new(15.0).monospace().letter_spacing(1.4).color(alpha(col, 0.95)))
                        .align(TextAlign::Center),
                ),
        )
        .into()
}
