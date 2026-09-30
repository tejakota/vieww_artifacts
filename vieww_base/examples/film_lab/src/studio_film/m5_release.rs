//! Movement V · THE RELEASE — the pullback, the end card, the hold.
//!
//! The studio recedes into the dark, the mark reveals itself panel by
//! panel (the studio's own `brand::revealed`, driven here by tweens),
//! and the film says the one thing it exists to say: **beta release
//! available today** — in the studio's purple, set in the studio's
//! type, rendered by the studio's engine.

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextAlign};
use vieww_widget::prelude::*;

use crate::film_lib::{clamp01, ease_in_out, ease_out_cubic, ease_out_expo, spring_out};
use crate::product_film as pf;
use super::filmkit as fk;
use super::{
    ACCENT, ACCENT_DEEP, BG_DEEP, BRAND_FAR, BRAND_NEAR, CANVAS, H, INK, MUTED, SYN_TYPE, W,
};

/// The repository — the call to action.
const REPO: &str = "github.com/tejakota/vieww_artifacts";

/// The release line — the one the film exists to say.
const RELEASE: &str = "beta release available today";

// ── Z12 · the_pullback ──────────────────────────────────────────────────────

pub fn the_pullback(ctx: &pf::Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;

    let mut stack = Stack::new();

    // The room — stars arrive as the studio recedes: the film's first
    // deep breath since the studio act began. Painted with overscan
    // margin, because this scene's camera is the one move allowed to
    // leave the canvas — a real dolly-out, past zoom 1.0.
    const MARGIN: f32 = 260.0;
    super::frame::ground(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            let _ = (w, h);
            fk::room(
                book,
                w,
                h,
                MARGIN,
                Color::rgb(10, 9, 13),
                BG_DEEP,
                Color::rgb(8, 8, 12),
            );
            let star_a = clamp01((t - 0.10) / 0.35);
            if star_a > 0.01 {
                fk::stars_wide(book, w, h, MARGIN, 0xC0DE, 110, t, 0.10 * star_a);
            }
            // A 3D floor opens beneath the studio as the camera pulls
            // back — the same perspective grid the question stood on.
            let floor_p = ease_in_out(clamp01(t / 0.9));
            if floor_p > 0.01 {
                let cam3 = crate::three_d::Camera {
                    eye: crate::three_d::Vec3::new(0.0, 60.0, -120.0 - 420.0 * floor_p),
                    target: crate::three_d::Vec3::new(0.0, 170.0, 420.0),
                    fov: 0.72,
                };
                fk::grid_floor_lines(book, &cam3, s, 1700.0, 30.0, 2100.0, 180.0, BRAND_NEAR, 0.75 * floor_p, 120.0, 1500.0);
            }
            pf::vignette(book, w, h, 0.55);
        }),
    )));
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            let _ = (w, h, &s);


        }),
    )));

    // The studio, receding — a panel that carries the act's shape: a
    // sidebar, code lines, a preview. It shrinks toward the dark, tilts
    // back in true perspective as it goes, and gives its light back to
    // the stars.
    let panel_p = ease_in_out(clamp01(t / 0.9));
    // The real studio — the same live application the act showed — as
    // one plate that shrinks into the dark and gives its light back.
    let k = 1.0 - 0.60 * panel_p;
    let alpha = 1.0 - 0.85 * panel_p;
    if alpha > 0.02 {
        let (w, h) = (1920.0 * k, 1080.0 * k);
        super::frame::plate(super::frame::Plate {
            src: super::layout::APP,
            dst: Rect::new(960.0 - w * 0.5, 540.0 - h * 0.5, 960.0 + w * 0.5, 540.0 + h * 0.5),
            alpha,
            radius: 14.0,
            snap: None,
            card: true,
        });
    }

    // The light gives itself back — thin threads converging on the spot
    // the studio left, riders flowing inward, the room absorbing it.
    if panel_p > 0.05 {
        let rays = 7;
        stack = stack.push(Positioned::fill().child(Painting::sized(
            CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                let centre = Offset::new(W * 0.5, H * 0.5);
                for k in 0..rays {
                    let ang = k as f32 * std::f32::consts::TAU / rays as f32 + 0.4;
                    let from = Offset::new(
                        centre.dx + 980.0 * ang.cos(),
                        centre.dy + 700.0 * ang.sin(),
                    );
                    let pts = fk::thread_pts(from, centre, 0.0);
                    let a = panel_p * 0.8;
                    fk::grow_stroke(book, &pts, clamp01(panel_p * 1.4 - k as f32 * 0.05), BRAND_FAR, 1.1, 0.35 * a);
                    fk::rider(book, &pts, (t * 0.30 + k as f32 * 0.14) % 1.0, BRAND_NEAR, 2.2, a);
                }
            }),
        )));
    }

    // The words — what the recede means.
    stack = stack.push(super::frame::caption("The studio steps back. The engine stays.", 1002.0, clamp01((t - 0.55) / 0.14)));
    let _ = sec;
    stack.into()
}

/// The studio's ghost — sidebar, code, preview — drawn at `scale`
/// about the canvas centre, its details dimming as it recedes.
fn draw_studio_ghost(book: &mut Sketchbook, scale: f32, p: f32) {
    let w = 980.0 * scale;
    let h = 620.0 * scale;
    let x0 = (W - w) * 0.5;
    let y0 = (1080.0 - h) * 0.5;
    let body = pf::xywh(x0, y0, w, h);
    book.shadow(body, 24.0 * scale, vieww_foundation::Shadow::new(pf::alpha(Color::BLACK, 0.5), Offset::new(0.0, 12.0 * scale), 40.0));
    book.rrect(body, 14.0 * scale, pf::alpha(viewwstudio_chrome(), 0.96));
    book.stroke_rrect(body, 14.0 * scale, pf::alpha(Color::WHITE, 0.05), 1.0);

    // The sidebar.
    let side_w = 190.0 * scale;
    book.rrect(pf::xywh(x0, y0, side_w, h), 14.0 * scale, pf::alpha(viewwstudio_chrome_1(), 0.9));
    for row in 0..7 {
        let rw = (60.0 + (row as f32 * 37.0) % 90.0) * scale;
        book.rrect(
            pf::xywh(x0 + 16.0 * scale, y0 + (26.0 + row as f32 * 30.0) * scale, rw, 9.0 * scale),
            4.0 * scale,
            pf::alpha(pf::FAINT, 0.5),
        );
    }

    // The editor — code lines, one of them the accent (the edit).
    let code_x = x0 + side_w + 22.0 * scale;
    for row in 0..9 {
        let cw = (110.0 + (row as f32 * 63.0) % 300.0) * scale;
        let accent_row = row == 4;
        book.rrect(
            pf::xywh(code_x, y0 + (24.0 + row as f32 * 34.0) * scale, cw, 10.0 * scale),
            5.0 * scale,
            pf::alpha(if accent_row { ACCENT } else { pf::MUTED }, if accent_row { 0.7 - 0.4 * p } else { 0.35 }),
        );
    }

    // The preview — the device frame with the truth card inside,
    // small. The picture from Movement I, come home.
    let pv_w = 300.0 * scale;
    let pv = pf::xywh(x0 + w - pv_w - 24.0 * scale, y0 + 22.0 * scale, pv_w, h - 44.0 * scale);
    book.rrect(pv, 12.0 * scale, pf::alpha(pf::MARK_GROUND, 0.95));
    let card = pf::xywh(pv.left + 26.0 * scale, pv.top + 26.0 * scale, pv.width() - 52.0 * scale, (pv.height() - 52.0 * scale).min(pv.width() * 1.3));
    book.rrect(card, 10.0 * scale, pf::alpha(pf::SURFACE, 0.9));
    book.rrect(
        pf::xywh(card.left, card.top, card.width(), 44.0 * scale),
        10.0 * scale,
        Gradient::vertical().with_dither().with_stops(&[(0.0, BRAND_FAR), (1.0, BRAND_NEAR)]),
    );
    for row in 0..3 {
        book.rrect(
            pf::xywh(card.left + 12.0 * scale, card.top + (58.0 + row as f32 * 26.0) * scale, card.width() - 24.0 * scale, 14.0 * scale),
            6.0 * scale,
            pf::alpha(pf::SURFACE_2, 0.9),
        );
    }
}

/// The studio's window chrome, spelled from the theme itself.
fn viewwstudio_chrome() -> Color {
    viewwstudio::StudioTheme::dark().window
}

fn viewwstudio_chrome_1() -> Color {
    viewwstudio::StudioTheme::dark().chrome_1
}

// ── Z13 · the_endcard ───────────────────────────────────────────────────────

pub fn the_endcard(ctx: &pf::Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let probe = ctx.probe;

    let mut stack = Stack::new();

    // The ground — the calmest register in the film.
    super::frame::ground(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            let _ = (w, h);
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(8, 8, 11)),
                    (0.65, BG_DEEP),
                    (1.0, Color::rgb(10, 9, 14)),
                ]),
            );
            pf::stars(book, w, h, 0xE2D1, 90, t, 0.09);
            pf::vignette(book, w, h, 0.5);
        }),
    )));
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            let _ = (w, h, &s);



        }),
    )));

    // The mark — the studio's own, revealing panel by panel: the
    // editor panel first, the preview panel over it, the ground
    // always whole. `revealed` is the brand's own door. Behind it, a
    // 3D halo — a ring in real perspective, riders orbiting it.
    let ring_a = clamp01((t - 0.02) / 0.4) * 0.9;
    if ring_a > 0.01 {
        stack = stack.push(Positioned::fill().child(Painting::sized(
            CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, s: Size| {
                fk::ring3(book, s, Offset::new(W * 0.5, 424.0), 100.0, sec, ring_a);
            }),
        )));
    }
    let editor_a = ease_out_expo(clamp01((t - 0.04) / 0.12));
    let preview_a = spring_out(clamp01((t - 0.10) / 0.16), 9.0, 0.62);
    if editor_a > 0.01 {
        stack = stack.push(Positioned::new().left((W - 184.0) * 0.5).top(332.0).width(184.0).height(184.0).child(
            pf::brand_mark(184.0, editor_a, preview_a.clamp(0.0, 1.0)),
        ));
        // The sting — one glow breathing behind the mark.
        stack = stack.push(Positioned::fill().child(Painting::sized(CANVAS, PaintWith::new(
            move |book: &mut Sketchbook, _s: Size| {
                let glow_p = clamp01((t - 0.42) / 0.14);
                if glow_p > 0.01 {
                    let pulse = 0.8 + 0.2 * (sec * 2.0).sin();
                    book.layer(glow_p, 44.0, None, |b| {
                        b.circle(Offset::new(W * 0.5, 424.0), 178.0 * pulse, pf::alpha(BRAND_NEAR, 0.11 * glow_p));
                    });
                }
            },
        ))));
    }

    // The wordmark — the studio's name in the brand's type, two-tone
    // by demand: `vieww` in ink, `studio` in the brand's purple.
    let name_a = ease_out_expo(clamp01((t - 0.12) / 0.14));
    if name_a > 0.01 {
        let rise = (1.0 - name_a) * 16.0;
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(544.0 + rise)
                .width(W)
                .height(148.0)
                .child(Opacity::new(name_a).child(fk::wordmark(104.0, 1.0))),
        );
    }

    // The underline — the brand's own ramp, growing to its width, fed
    // by two ribbons flowing in from the frame's edges.
    let line_a = clamp01((t - 0.22) / 0.12);
    let ribbons_a = clamp01((t - 0.30) / 0.4);
    if ribbons_a > 0.01 {
        let (la, sec_v) = (ribbons_a, sec);
        stack = stack.push(Positioned::fill().child(Painting::sized(
            CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                for (from, bend, phase) in [
                    (Offset::new(W * 0.06, 940.0), -140.0, 0.0),
                    (Offset::new(W * 0.94, 940.0), 140.0, 0.5),
                ] {
                    let pts = fk::thread_pts(from, Offset::new(W * 0.5, 694.0), bend);
                    fk::ribbon(book, &pts, 2.0, sec_v + phase, BRAND_FAR, 0.55 * la);
                    fk::rider(book, &pts, (sec_v * 0.22 + phase) % 1.0, BRAND_NEAR, 2.4, la);
                }
            }),
        )));
    }
    if line_a > 0.01 {
        let target_w = 900.0;
        let uw = target_w * ease_out_expo(line_a);
        stack = stack.push(Positioned::new().left((W - uw) * 0.5).top(692.0).width(uw).height(5.0).child(
            Painting::sized(Size::new(uw.max(1.0), 5.0), PaintWith::new(
                move |book: &mut Sketchbook, s: Size| {
                    book.rect(
                        Rect::new(0.0, 0.0, s.width, s.height),
                        Gradient::horizontal().with_dither().with_stops(&[
                            (0.0, pf::alpha(ACCENT_DEEP, 0.0)),
                            (0.18, ACCENT_DEEP),
                            (0.5, ACCENT),
                            (0.82, ACCENT_DEEP),
                            (1.0, pf::alpha(ACCENT_DEEP, 0.0)),
                        ]),
                    );
                },
            )),
        ));
    }

    // The release line — the film's whole argument, typed on.
    stack = stack.push(pf::type_on(
        RELEASE,
        pf::TypeAt::CenteredOn(W as i32 / 2),
        740.0,
        pf::geist(48.0).bold().letter_spacing(3.0).color(pf::alpha(ACCENT, 1.0)),
        clamp01((t - 0.26) / 0.20),
        sec,
    ));

    // The repository — where the beta lives.
    let repo_a = clamp01((t - 0.52) / 0.12);
    if repo_a > 0.01 {
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(824.0 + (1.0 - ease_out_cubic(repo_a)) * 10.0)
                .width(W)
                .height(34.0)
                .child(Opacity::new(repo_a).child(
                    Text::new(REPO.to_string())
                        .style(pf::geist_mono(23.0).letter_spacing(1.6).color(pf::alpha(MUTED, 0.97)))
                        .align(TextAlign::Center),
                )),
        );
    }

    // The film's own contract — its receipts, counted by its census.
    //
    // Two full-width `Positioned` blocks with `TextAlign::Center`, not a
    // `Flex::column`: the column sized each child to its own intrinsic
    // width and then laid them out on its cross axis, which put a centred
    // end card's last two lines hard against the right margin.
    let contract_a = clamp01((t - 0.62) / 0.12);
    if contract_a > 0.01 && probe.frames > 0 {
        stack = stack.push(
            Positioned::new().left(0.0).top(876.0).width(W).height(32.0).child(
                Opacity::new(contract_a).child(
                    Text::new(format!(
                        "every frame rendered by vieww — {} frames · {:.0} ms median",
                        pf::group_commas(probe.frames),
                        probe.frame_ms
                    ))
                    .style(pf::geist_mono(20.0).letter_spacing(1.0).color(pf::alpha(SYN_TYPE, 0.92)))
                    .align(TextAlign::Center),
                ),
            ),
        );
        stack = stack.push(
            Positioned::new().left(0.0).top(912.0).width(W).height(30.0).child(
                Opacity::new(contract_a * 0.9).child(
                    Text::new(probe.bench.clone())
                        .style(pf::geist_mono(17.0).letter_spacing(0.8).color(pf::alpha(MUTED, 0.85)))
                        .align(TextAlign::Center),
                ),
            ),
        );
    }
    stack.into()
}

// ── Z22 · the_hold ──────────────────────────────────────────────────────────

pub fn the_hold(ctx: &pf::Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let probe = ctx.probe;

    let mut stack = Stack::new();

    // The same ground, breathing.
    super::frame::ground(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            let _ = (w, h);
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(8, 8, 11)),
                    (0.65, BG_DEEP),
                    (1.0, Color::rgb(10, 9, 14)),
                ]),
            );
            pf::stars(book, w, h, 0xE2D1, 90, t + 11.0, 0.09);
            pf::vignette(book, w, h, 0.5);
        }),
    )));
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            let _ = (w, h, &s);


            let pulse = 1.0 + 0.08 * (sec * 1.2).sin();
            book.layer(0.8, 44.0, None, |b| {
                b.circle(Offset::new(w * 0.5, 424.0), 178.0 * pulse, pf::alpha(BRAND_NEAR, 0.11));
            });

        }),
    )));

    // The mark, whole; the name; the release line; the repo. Nothing
    // arrives — everything is already here. The halo keeps orbiting.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            fk::ring3(book, s, Offset::new(W * 0.5, 424.0), 100.0, sec + 18.0, 0.9);
        }),
    )));
    stack = stack.push(Positioned::new().left((W - 184.0) * 0.5).top(332.0).width(184.0).height(184.0).child(
        pf::brand_mark(184.0, 1.0, 1.0),
    ));
    stack = stack.push(
        Positioned::new()
            .left(0.0)
            .top(544.0)
            .width(W)
            .height(148.0)
            .child(fk::wordmark(104.0, 1.0)),
    );
    stack = stack.push(Positioned::new().left((W - 900.0) * 0.5).top(692.0).width(900.0).height(5.0).child(
        Painting::sized(Size::new(900.0, 5.0), PaintWith::new(
            move |book: &mut Sketchbook, s: Size| {
                book.rect(
                    Rect::new(0.0, 0.0, s.width, s.height),
                    Gradient::horizontal().with_dither().with_stops(&[
                        (0.0, pf::alpha(ACCENT_DEEP, 0.0)),
                        (0.18, ACCENT_DEEP),
                        (0.5, ACCENT),
                        (0.82, ACCENT_DEEP),
                        (1.0, pf::alpha(ACCENT_DEEP, 0.0)),
                    ]),
                );
            },
        )),
    ));
    stack = stack.push(
        Positioned::new()
            .left(0.0)
            .top(740.0)
            .width(W)
            .height(66.0)
            .child(
                Text::new(RELEASE.to_string())
                    .style(pf::geist(48.0).bold().letter_spacing(3.0).color(pf::alpha(ACCENT, 1.0)))
                    .align(TextAlign::Center),
            ),
    );
    stack = stack.push(
        Positioned::new()
            .left(0.0)
            .top(824.0)
            .width(W)
            .height(34.0)
            .child(
                Text::new(REPO.to_string())
                    .style(pf::geist_mono(23.0).letter_spacing(1.6).color(pf::alpha(MUTED, 0.97)))
                    .align(TextAlign::Center),
            ),
    );
    if probe.frames > 0 {
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(876.0)
                .width(W)
                .height(32.0)
                .child(
                    Text::new(format!(
                        "every frame rendered by vieww — {} frames · {:.0} ms median",
                        pf::group_commas(probe.frames),
                        probe.frame_ms
                    ))
                    .style(pf::geist_mono(20.0).letter_spacing(1.0).color(pf::alpha(SYN_TYPE, 0.92)))
                    .align(TextAlign::Center),
                ),
        );
        stack = stack.push(
            Positioned::new().left(0.0).top(912.0).width(W).height(30.0).child(
                Text::new(probe.bench.clone())
                    .style(pf::geist_mono(17.0).letter_spacing(0.8).color(pf::alpha(MUTED, 0.85)))
                    .align(TextAlign::Center),
            ),
        );
    }

    // The last thing the film does is stop — a fade to black that
    // holds long enough to feel like a promise.
    let fade = clamp01((sec - 10.4) / 2.6);
    if fade > 0.01 {
        stack = stack.push(Positioned::fill().child(Painting::sized(CANVAS, PaintWith::new(
            move |book: &mut Sketchbook, s: Size| {
                book.rect(Rect::new(0.0, 0.0, s.width, s.height), pf::alpha(Color::BLACK, fade));
            },
        ))));
    }

    stack.into()
}
