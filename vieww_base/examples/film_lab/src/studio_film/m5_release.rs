//! Movement V · THE RELEASE — the pullback, the end card, the hold.
//!
//! The studio recedes into the dark, the mark reveals itself panel by
//! panel (the studio's own `brand::revealed`, driven here by tweens),
//! and the film says the one thing it exists to say: **beta release
//! available today** — in the studio's purple, set in the studio's
//! type, rendered by the studio's engine.

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextAlign};
use vieww_widget::prelude::*;
use vieww_widget::{RichText, Span};

use super::filmkit as fk;
use super::{
    ACCENT, ACCENT_DEEP, BG_DEEP, BRAND_FAR, BRAND_NEAR, CANVAS, MUTED, SYN_TYPE, TERM_GREEN, W,
};
use crate::film_lib::{clamp01, ease_in_out, ease_out_cubic, ease_out_expo, spring_out};
use crate::product_film as pf;

/// The repository — the call to action.
const REPO: &str = "github.com/tejakota/vieww_artifacts";

/// The release line — the one the film exists to say.
const RELEASE: &str = "beta release available today";

// ── Z12 · the_pullback ──────────────────────────────────────────────────────

/// Where the end card's mark sits on screen — the rect `the_endcard`
/// places it at, in *its* world coordinates.
fn endcard_mark() -> Rect {
    Rect::new((W - 184.0) * 0.5, 332.0, (W - 184.0) * 0.5 + 184.0, 516.0)
}

/// The end card's mark, translated into **this** scene's world — the
/// anchor the studio collapses onto. [`frame::fit_of`] is the master's
/// own fit arithmetic, so the anchor is where the incoming scene's fit
/// will actually put the mark on screen; handing the studio's collapse
/// to that exact rect is what makes the cut read as a *transform* rather
/// than as one thing fading out and another fading in.
fn mark_anchor() -> Rect {
    let (s_t, o_t) = super::frame::fit_of("Z20");
    let (s_e, o_e) = super::frame::fit_of("Z21");
    let m = endcard_mark();
    // The mark's rect on screen, through the end card's fit…
    let scr = Rect::new(
        m.left * s_e + o_e.dx,
        m.top * s_e + o_e.dy,
        m.right * s_e + o_e.dx,
        m.bottom * s_e + o_e.dy,
    );
    // …and back through this scene's, into this scene's world.
    Rect::new(
        (scr.left - o_t.dx) / s_t,
        (scr.top - o_t.dy) / s_t,
        (scr.right - o_t.dx) / s_t,
        (scr.bottom - o_t.dy) / s_t,
    )
}

pub(crate) fn the_pullback(ctx: &pf::Ctx) -> WidgetNode {
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
                fk::grid_floor_lines(
                    book,
                    &cam3,
                    s,
                    1700.0,
                    30.0,
                    2100.0,
                    180.0,
                    BRAND_NEAR,
                    0.75 * floor_p,
                    120.0,
                    1500.0,
                );
            }
            pf::vignette(book, w, h, 0.55);
        }),
    )));

    // ── The studio becomes the mark ───────────────────────────────────
    //
    // The window does not merely shrink and fade — it collapses onto the
    // exact rect where the end card's mark lives, its corners rounding to
    // the mark's own, while the mark itself cross-dissolves in over it,
    // panel by panel. What the audience sees across the cut is one
    // object turning into another: the product, becoming its brand.
    // Z21 then opens on the mark *already whole* — the handoff is the
    // dissolve, and the transform continues instead of restarting.
    let panel_p = ease_in_out(clamp01(t / 0.9));
    let anchor = mark_anchor();
    let side = anchor.width();
    let app_world = super::layout::APP;
    let lerp = |a: f32, b: f32, u: f32| a + (b - a) * u;
    let lerp_r = |a: Rect, b: Rect, u: f32| {
        Rect::new(
            lerp(a.left, b.left, u),
            lerp(a.top, b.top, u),
            lerp(a.right, b.right, u),
            lerp(a.bottom, b.bottom, u),
        )
    };
    // The mark's own geometry — `viewwstudio::ui::brand`'s panel
    // fractions (x, y, w, h of the side), restated here because the
    // module keeps them private; the final hand-off to the real
    // `brand::revealed` below is what proves they agree.
    let frac = |f: (f32, f32, f32, f32)| {
        Rect::new(
            anchor.left + side * f.0,
            anchor.top + side * f.1,
            anchor.left + side * (f.0 + f.2),
            anchor.top + side * (f.1 + f.3),
        )
    };
    let mark_editor = frac((0.16, 0.20, 0.40, 0.60));
    let mark_preview = frac((0.44, 0.32, 0.40, 0.48));

    // **1 · the window recedes** — whole, keeping its own proportions —
    // until it is a little wider than the mark, centred where the mark
    // will be.
    let shrink = ease_in_out(clamp01(t / 0.46));
    let (acx, acy) = (
        (anchor.left + anchor.right) * 0.5,
        (anchor.top + anchor.bottom) * 0.5,
    );
    let ww = side * 1.55;
    let wh = ww * app_world.height() / app_world.width();
    let small = Rect::new(
        acx - ww * 0.5,
        acy - wh * 0.5,
        acx + ww * 0.5,
        acy + wh * 0.5,
    );
    let window = lerp_r(app_world, small, shrink);
    // Where a region of the app sits inside the collapsing window — the
    // plate maps the app by the window's width, so the panes ride with it.
    let k = window.width() / app_world.width();
    let inside = |r: Rect| {
        Rect::new(
            window.left + r.left * k,
            window.top + r.top * k,
            window.left + r.right * k,
            window.top + r.bottom * k,
        )
    };

    // **2 · the two panes lift out of it and become the two panels.** The
    // studio *is* the mark's picture — an editor beside a preview — so
    // the editor pane reshapes into the mark's grey panel and the
    // preview pane (the phone, still live) into its violet one, each
    // travelling from where it sits in the window to where it sits in
    // the logo. The window behind them dims into the mark's ground.
    let split = ease_in_out(clamp01((t - 0.36) / 0.36));
    let ed_dst = lerp_r(inside(super::m3_studio::app::EDITOR), mark_editor, split);
    let pv_dst = lerp_r(inside(super::m3_studio::app::PREVIEW), mark_preview, split);
    // The panes take the panels' solid faces as they land.
    let solid = ease_in_out(clamp01((t - 0.62) / 0.14));
    let window_a = 1.0 - ease_in_out(clamp01((t - 0.38) / 0.20));
    let ground_a = ease_in_out(clamp01((t - 0.36) / 0.14));
    // The ground is the window's own rectangle squaring up into the
    // mark's rounded square while the panes leave it.
    let ground = lerp_r(window, anchor, split);

    super::frame::plate(super::frame::Plate {
        src: super::layout::APP,
        dst: window,
        alpha: window_a.max(0.0),
        radius: 14.0 * (1.0 - shrink) + side * 0.06 * shrink,
        snap: None,
        card: true,
        bare: false,
        morph: None,
        stretch: false,
    });
    // The mark's ground, under the panes (drawn beneath the plates), so
    // the panes travel *over* it the way the panels sit on it.
    if ground_a > 0.01 {
        let g = ground;
        super::frame::under(Positioned::fill().child(Painting::sized(
            CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                let r = lerp(side * 0.06, side * 0.22, split);
                book.rrect(
                    g,
                    r,
                    Gradient::vertical().between(
                        pf::alpha(Color::rgb(0x24, 0x27, 0x2E), ground_a),
                        pf::alpha(viewwstudio::ui::brand::GROUND, ground_a),
                    ),
                );
                book.fill(
                    vieww_foundation::Path::rounded_ring(g, r, 1.0),
                    pf::alpha(Color::WHITE, 0.10 * ground_a),
                );
            }),
        )));
    }
    let pane_a = 1.0 - clamp01((solid - 0.85) / 0.15);
    if t > 0.34 && pane_a > 0.0 {
        for (src, dst) in [
            (super::m3_studio::app::EDITOR, ed_dst),
            (super::m3_studio::app::PREVIEW, pv_dst),
        ] {
            super::frame::plate(super::frame::Plate {
                src,
                dst,
                alpha: pane_a,
                radius: lerp(4.0, side * 0.08, split),
                snap: None,
                card: split < 0.6,
                bare: false,
                morph: None,
                stretch: true,
            });
        }
    }
    // The panels' solid faces, over the landing panes.
    if solid > 0.01 {
        stack = stack.push(Positioned::fill().child(Painting::sized(
            CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                book.rrect(
                    ed_dst,
                    side * 0.08,
                    pf::alpha(viewwstudio::ui::brand::PANEL, solid),
                );
                book.rrect(
                    pv_dst,
                    side * 0.08,
                    Gradient::vertical().between(
                        pf::alpha(viewwstudio::ui::brand::ACCENT_FAR, solid),
                        pf::alpha(viewwstudio::ui::brand::ACCENT, solid),
                    ),
                );
            }),
        )));
    }

    // **3 · the hand-off.** The real `brand::revealed` takes over at the
    // very same rect, whole — what Z21 receives across the cut.
    let mark_a = ease_out_cubic(clamp01((t - 0.78) / 0.10));
    if mark_a > 0.01 {
        let at = Offset::new(anchor.left, anchor.top);
        stack = stack.push(
            Positioned::new()
                .left(at.dx)
                .top(at.dy)
                .width(side)
                .height(side)
                .child(Opacity::new(mark_a).child(pf::brand_mark(side, 1.0, 1.0))),
        );
    }

    // The light gives itself back — thin threads converging on the spot
    // the studio is becoming, riders flowing inward, the room absorbing
    // it. Their centre is the mark's own: the rays feed the thing that
    // is taking the studio's place.
    if panel_p > 0.05 {
        let rays = 7;
        let centre = Offset::new(
            (anchor.left + anchor.right) * 0.5,
            (anchor.top + anchor.bottom) * 0.5,
        );
        stack = stack.push(Positioned::fill().child(Painting::sized(
            CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                for k in 0..rays {
                    let ang = k as f32 * std::f32::consts::TAU / rays as f32 + 0.4;
                    let from =
                        Offset::new(centre.dx + 980.0 * ang.cos(), centre.dy + 700.0 * ang.sin());
                    let pts = fk::thread_pts(from, centre, 0.0);
                    let a = panel_p * 0.8;
                    fk::grow_stroke(
                        book,
                        &pts,
                        clamp01(panel_p * 1.4 - k as f32 * 0.05),
                        BRAND_FAR,
                        1.1,
                        0.35 * a,
                    );
                    fk::rider(
                        book,
                        &pts,
                        (t * 0.30 + k as f32 * 0.14) % 1.0,
                        BRAND_NEAR,
                        2.2,
                        a,
                    );
                }
            }),
        )));
    }

    // The words — what the recede means.
    stack = stack.push(super::frame::caption(
        "The studio steps back. The engine stays.",
        1002.0,
        clamp01((t - 0.55) / 0.14),
    ));
    let _ = sec;
    stack.into()
}

// ── Z13 · the_endcard ───────────────────────────────────────────────────────

pub(crate) fn the_endcard(ctx: &pf::Ctx) -> WidgetNode {
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

    // The mark — **already whole.** The pullback ended with the studio
    // collapsed onto this exact rect, the mark cross-dissolved in over
    // it, panel by panel, and landed whole; the cut's dissolve hands it
    // over mid-breath. Re-running the panel reveal here would restart
    // the very transform the previous scene just completed — so the
    // mark arrives as one object, settling the last few percent of the
    // landing, and the scene spends its entrance budget on the name and
    // the promise instead. Behind it, the 3D halo — a ring in real
    // perspective, riders orbiting it — fades up around what the rays
    // were feeding a cut ago.
    let ring_a = clamp01((t - 0.02) / 0.4) * 0.9;
    if ring_a > 0.01 {
        stack = stack.push(Positioned::fill().child(Painting::sized(
            CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, s: Size| {
                fk::ring3(book, s, Offset::new(W * 0.5, 424.0), 100.0, sec, ring_a);
            }),
        )));
    }
    let mark_in = ease_out_expo(clamp01((sec - 0.05) / 0.15));
    if mark_in > 0.01 {
        // The landing's last breath: the mark settles from a hair under
        // its own size, about its own centre — the physical receipt of
        // the handoff, gone in half a second. (Timed on `sec`, the real
        // clock: this scene is 18 s and its `t` crawls.)
        let settle = spring_out(clamp01(sec / 0.55), 9.0, 0.62);
        let k = 0.94 + 0.06 * settle;
        let side = 184.0;
        let inset = Offset::new(side * (1.0 - k) * 0.5, side * (1.0 - k) * 0.5);
        stack = stack.push(
            Positioned::new()
                .left((W - side) * 0.5)
                .top(332.0)
                .width(side)
                .height(side)
                .child(
                    Opacity::new(mark_in).child(
                        Transformed::translate(inset)
                            .child(Transformed::scale(k, k).child(pf::brand_mark(side, 1.0, 1.0))),
                    ),
                ),
        );
        // (The sting's breathing glow behind the mark is gone with the
        // film's radial glows — the orbiting 3D ring above and the
        // underline below carry the release's altitude on their own.)
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
                    fk::rider(
                        book,
                        &pts,
                        (sec_v * 0.22 + phase) % 1.0,
                        BRAND_NEAR,
                        2.4,
                        la,
                    );
                }
            }),
        )));
    }
    if line_a > 0.01 {
        let target_w = 900.0;
        let uw = target_w * ease_out_expo(line_a);
        stack = stack.push(
            Positioned::new()
                .left((W - uw) * 0.5)
                .top(692.0)
                .width(uw)
                .height(5.0)
                .child(Painting::sized(
                    Size::new(uw.max(1.0), 5.0),
                    PaintWith::new(move |book: &mut Sketchbook, s: Size| {
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
                    }),
                )),
        );
    }

    // The release line — the film's whole argument, typed on.
    stack = stack.push(pf::type_on(
        RELEASE,
        pf::TypeAt::CenteredOn(W as i32 / 2),
        740.0,
        pf::geist(48.0)
            .bold()
            .letter_spacing(3.0)
            .color(pf::alpha(ACCENT, 1.0)),
        clamp01((t - 0.26) / 0.20),
        sec,
    ));

    // The audience row — the film's bookend. The cold open named everyone
    // the film is for ("designs apps, builds them, or simply uses them");
    // the end card hands each of them their takeaway in one line, before
    // the repository asks anything of anyone. Three audiences, three of
    // the film's own working colours, one sentence.
    let aud_a = clamp01((t - 0.44) / 0.14);
    if aud_a > 0.01 {
        let mono = pf::geist_mono(21.0).letter_spacing(1.4);
        let rise = (1.0 - ease_out_cubic(aud_a)) * 8.0;
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(808.0 + rise)
                .width(W)
                .height(34.0)
                .child(
                    Opacity::new(aud_a).child(
                        RichText::new(vec![
                            Span::new("for everyone who ").color(pf::alpha(MUTED, 0.95)),
                            Span::new("designs").color(pf::alpha(BRAND_NEAR, 0.98)),
                            Span::new(" it · ").color(pf::alpha(MUTED, 0.95)),
                            Span::new("builds").color(pf::alpha(SYN_TYPE, 0.98)),
                            Span::new(" it · ").color(pf::alpha(MUTED, 0.95)),
                            Span::new("uses").color(pf::alpha(TERM_GREEN, 0.98)),
                            Span::new(" it").color(pf::alpha(MUTED, 0.95)),
                        ])
                        .style(mono)
                        .align(TextAlign::Center),
                    ),
                ),
        );
    }

    // The repository — where the beta lives.
    let repo_a = clamp01((t - 0.52) / 0.12);
    if repo_a > 0.01 {
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(858.0 + (1.0 - ease_out_cubic(repo_a)) * 10.0)
                .width(W)
                .height(34.0)
                .child(
                    Opacity::new(repo_a).child(
                        Text::new(REPO.to_string())
                            .style(
                                pf::geist_mono(23.0)
                                    .letter_spacing(1.6)
                                    .color(pf::alpha(MUTED, 0.97)),
                            )
                            .align(TextAlign::Center),
                    ),
                ),
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
            Positioned::new().left(0.0).top(900.0).width(W).height(32.0).child(
                Opacity::new(contract_a).child(
                    Text::new(format!(
                        "every frame rendered by vieww — {} frames · {:.0} ms median · score synthesised by vieww-audio",
                        pf::group_commas(probe.frames),
                        probe.frame_ms
                    ))
                    .style(pf::geist_mono(20.0).letter_spacing(1.0).color(pf::alpha(SYN_TYPE, 0.92)))
                    .align(TextAlign::Center),
                ),
            ),
        );
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(936.0)
                .width(W)
                .height(30.0)
                .child(
                    Opacity::new(contract_a * 0.9).child(
                        Text::new(probe.bench.clone())
                            .style(
                                pf::geist_mono(17.0)
                                    .letter_spacing(0.8)
                                    .color(pf::alpha(MUTED, 0.85)),
                            )
                            .align(TextAlign::Center),
                    ),
                ),
        );
    }
    stack.into()
}

// ── Z22 · the_hold ──────────────────────────────────────────────────────────

pub(crate) fn the_hold(ctx: &pf::Ctx) -> WidgetNode {
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
        PaintWith::new(move |_book: &mut Sketchbook, s: Size| {
            let _ = s;
            // (No glow behind the mark here either — the hold is the
            // end card at rest, and its rest is drawn, not bloomed.)
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
    stack = stack.push(
        Positioned::new()
            .left((W - 184.0) * 0.5)
            .top(332.0)
            .width(184.0)
            .height(184.0)
            .child(pf::brand_mark(184.0, 1.0, 1.0)),
    );
    stack = stack.push(
        Positioned::new()
            .left(0.0)
            .top(544.0)
            .width(W)
            .height(148.0)
            .child(fk::wordmark(104.0, 1.0)),
    );
    stack = stack.push(
        Positioned::new()
            .left((W - 900.0) * 0.5)
            .top(692.0)
            .width(900.0)
            .height(5.0)
            .child(Painting::sized(
                Size::new(900.0, 5.0),
                PaintWith::new(move |book: &mut Sketchbook, s: Size| {
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
                }),
            )),
    );
    stack = stack.push(
        Positioned::new()
            .left(0.0)
            .top(740.0)
            .width(W)
            .height(66.0)
            .child(
                Text::new(RELEASE.to_string())
                    .style(
                        pf::geist(48.0)
                            .bold()
                            .letter_spacing(3.0)
                            .color(pf::alpha(ACCENT, 1.0)),
                    )
                    .align(TextAlign::Center),
            ),
    );
    // The audience row, at rest — the same bookend the end card typed on.
    stack = stack.push(
        Positioned::new()
            .left(0.0)
            .top(808.0)
            .width(W)
            .height(34.0)
            .child(
                RichText::new(vec![
                    Span::new("for everyone who ").color(pf::alpha(MUTED, 0.95)),
                    Span::new("designs").color(pf::alpha(BRAND_NEAR, 0.98)),
                    Span::new(" it · ").color(pf::alpha(MUTED, 0.95)),
                    Span::new("builds").color(pf::alpha(SYN_TYPE, 0.98)),
                    Span::new(" it · ").color(pf::alpha(MUTED, 0.95)),
                    Span::new("uses").color(pf::alpha(TERM_GREEN, 0.98)),
                    Span::new(" it").color(pf::alpha(MUTED, 0.95)),
                ])
                .style(pf::geist_mono(21.0).letter_spacing(1.4))
                .align(TextAlign::Center),
            ),
    );
    stack = stack.push(
        Positioned::new()
            .left(0.0)
            .top(858.0)
            .width(W)
            .height(34.0)
            .child(
                Text::new(REPO.to_string())
                    .style(
                        pf::geist_mono(23.0)
                            .letter_spacing(1.6)
                            .color(pf::alpha(MUTED, 0.97)),
                    )
                    .align(TextAlign::Center),
            ),
    );
    if probe.frames > 0 {
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(900.0)
                .width(W)
                .height(32.0)
                .child(
                    Text::new(format!(
                        "every frame rendered by vieww — {} frames · {:.0} ms median · score synthesised by vieww-audio",
                        pf::group_commas(probe.frames),
                        probe.frame_ms
                    ))
                    .style(pf::geist_mono(20.0).letter_spacing(1.0).color(pf::alpha(SYN_TYPE, 0.92)))
                    .align(TextAlign::Center),
                ),
        );
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(936.0)
                .width(W)
                .height(30.0)
                .child(
                    Text::new(probe.bench.clone())
                        .style(
                            pf::geist_mono(17.0)
                                .letter_spacing(0.8)
                                .color(pf::alpha(MUTED, 0.85)),
                        )
                        .align(TextAlign::Center),
                ),
        );
    }

    // The last thing the film does is stop — a fade to black that
    // holds long enough to feel like a promise.
    let fade = clamp01((sec - 10.4) / 2.6);
    if fade > 0.01 {
        stack = stack.push(Positioned::fill().child(Painting::sized(
            CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, s: Size| {
                book.rect(
                    Rect::new(0.0, 0.0, s.width, s.height),
                    pf::alpha(Color::BLACK, fade),
                );
            }),
        )));
    }

    stack.into()
}
