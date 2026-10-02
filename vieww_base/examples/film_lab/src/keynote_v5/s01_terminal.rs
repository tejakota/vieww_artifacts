//! S01 · THE TERMINAL — Act I opens. 0:00–0:10.
//!
//! The world as sold: a terminal, a build command, and the flood of
//! dependencies that follows — a 40 MB engine "for breakfast", a thousand
//! lines of glue, a platform framework, and one warning nobody reads: the
//! rendering tree was truncated. Silently.
//!
//! The whole scene runs at the old world's cadence — 24-in-60 judder,
//! grain, a slightly crushed ground — because the poverty of this image is
//! itself the argument, and it is *rendered*, not filmed (the aurora plate's
//! degradation grammar). The hard cut into Act II's 60 fps is the first
//! escalation.
//!
//! The numbers are the reference script's own: 40 MB engine, 1,847 lines of
//! C++ bindings, 41.7 MB before the first pixel — quoted from the keynote
//! script's problem statement (the story's source document).

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, PaintWith, Painting};

use crate::film_lib::{
    alpha, clamp01, ease_out_cubic, mix, tint, xywh, AMBER, BG_DEEP, INK, MUTED, RED,
};

use super::{grain, mono_tracked, Ctx, TERM_GREEN};

/// The terminal window's rect.
const TERM: Rect = Rect::new(360.0, 200.0, 1200.0, 700.0);

/// One dependency line: text, its tone, and the film-second it lands.
fn dep_lines() -> Vec<(&'static str, Color, f32)> {
    let mut v = vec![
        ("$ create ui app --target mobile", alpha(INK, 0.95), 0.0),
        ("resolving dependencies…", alpha(MUTED, 0.85), 0.7),
        ("heavy-js-engine        40.2 MB", AMBER, 1.5),
        ("cxx-bindings       1,847 lines", AMBER, 2.3),
        ("bridge-runtime       (native)", alpha(MUTED, 0.9), 3.0),
        ("sys-framework         1.1 GB", AMBER, 3.7),
        ("battery-drain           yes", RED, 4.4),
    ];
    v.push((
        "warning: rendering tree truncated at depth 3 (silent)",
        RED,
        5.4,
    ));
    v.push(("linking…", alpha(MUTED, 0.85), 6.3));
    v.push(("41.7 MB before your first pixel", tint(RED, 0.25), 7.1));
    v
}

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let abs = ctx.abs;
    let frame_i = (abs * 60.0) as u64;

    let mut stack = Stack::new();

    // The ground — deeper, flatter, more tired than the film's own.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(7, 7, 9)),
                    (0.65, BG_DEEP),
                    (1.0, Color::rgb(8, 8, 10)),
                ]),
            );
            super::vignette(book, w, h, 0.62);
        }),
    )));

    // The terminal window — a tired card with a title bar.
    stack = stack.push(
        Positioned::new()
            .left(TERM.left)
            .top(TERM.top)
            .width(TERM.width())
            .height(TERM.height())
            .child(Painting::sized(
                Size::new(TERM.width(), TERM.height()),
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    // The shadow it sits in.
                    book.shadow(
                        xywh(0.0, 10.0, TERM.width(), TERM.height()),
                        14.0,
                        vieww_foundation::Shadow::new(
                            alpha(Color::BLACK, 0.55),
                            Offset::new(0.0, 22.0),
                            46.0,
                        ),
                    );
                    // The body — near-black, faintly green-washed: the old world.
                    book.rrect(
                        xywh(0.0, 0.0, TERM.width(), TERM.height()),
                        14.0,
                        alpha(Color::rgb(12, 14, 13), 0.97),
                    );
                    book.stroke_rrect(
                        xywh(0.0, 0.0, TERM.width(), TERM.height()),
                        14.0,
                        alpha(mix(TERM_GREEN, Color::BLACK, 0.55), 0.35),
                        1.2,
                    );
                    // The title bar.
                    book.rect(
                        xywh(1.0, 1.0, TERM.width() - 2.0, 44.0),
                        alpha(Color::rgb(17, 20, 18), 0.95),
                    );
                    let dots = [
                        Color::rgb(255, 95, 86),
                        Color::rgb(255, 189, 46),
                        Color::rgb(39, 201, 63),
                    ];
                    for (i, c) in dots.iter().enumerate() {
                        book.circle(Offset::new(28.0 + i as f32 * 24.0, 22.0), 6.0, *c);
                    }
                    book.rect(
                        xywh(1.0, 44.0, TERM.width() - 2.0, 1.0),
                        alpha(Color::WHITE, 0.05),
                    );
                    // Scanlines — the CRT's memory, 3px apart, barely there.
                    let mut y = 46.0;
                    while y < TERM.height() {
                        book.rect(
                            xywh(1.0, y, TERM.width() - 2.0, 1.0),
                            alpha(Color::BLACK, 0.10),
                        );
                        y += 3.0;
                    }
                }),
            )),
    );
    stack = stack.push(
        Positioned::new()
            .left(TERM.left + TERM.width() * 0.5 - 200.0)
            .top(TERM.top + 12.0)
            .width(400.0)
            .height(24.0)
            .child(
                Text::new("the old way — sh")
                    .style(
                        TextStyle::new(14.0)
                            .monospace()
                            .letter_spacing(2.0)
                            .color(alpha(MUTED, 0.8)),
                    )
                    .align(TextAlign::Center),
            ),
    );

    // The lines — typed and streamed at the held cadence.
    let lines = dep_lines();
    for (i, (text, color, at)) in lines.iter().enumerate() {
        let appear = ((sec - at) / 0.5).clamp(0.0, 1.0);
        if appear <= 0.0 {
            continue;
        }
        let chars_total = text.chars().count();
        let typed = (chars_total as f32 * clamp01(appear * 2.4)) as usize;
        let shown: String = text.chars().take(typed).collect();
        stack = stack.push(
            Positioned::new()
                .left(TERM.left + 44.0)
                .top(TERM.top + 78.0 + i as f32 * 46.0)
                .width(TERM.width() - 80.0)
                .height(30.0)
                .child(Opacity::new(1.0).child(mono_tracked(shown, 21.0, *color, 0.8))),
        );
    }

    // The blinking caret — 24-in-60, at the last line's end.
    let last_y = TERM.top + 78.0 + lines.len() as f32 * 46.0;
    let on = (sec * 2.2).fract() < 0.55;
    if on {
        stack = stack.push(
            Positioned::new()
                .left(TERM.left + 44.0)
                .top(last_y - 4.0)
                .width(13.0)
                .height(26.0)
                .child(Container::new().color(alpha(TERM_GREEN, 0.85)).radius(2.0)),
        );
    }

    // The size bar — filling toward 41.7 MB as the deps land.
    let bar_p = clamp01((sec - 1.5) / 5.5);
    if bar_p > 0.0 {
        let p = ease_out_cubic(bar_p);
        stack = stack.push(
            Positioned::new()
                .left(TERM.left + 44.0)
                .top(TERM.top + TERM.height() - 92.0)
                .width(TERM.width() - 88.0)
                .height(46.0)
                .child(Painting::sized(
                    Size::new(TERM.width() - 88.0, 46.0),
                    PaintWith::new(move |book: &mut Sketchbook, s: Size| {
                        let w = s.width - 190.0;
                        book.rrect(xywh(0.0, 12.0, w, 14.0), 7.0, alpha(Color::WHITE, 0.06));
                        let fill = w * p;
                        if fill > 2.0 {
                            book.rrect(
                                xywh(0.0, 12.0, fill, 14.0),
                                7.0,
                                Gradient::horizontal().with_dither().with_stops(&[
                                    (0.0, alpha(AMBER, 0.85)),
                                    (0.7, alpha(mix(AMBER, RED, 0.55), 0.9)),
                                    (1.0, alpha(tint(RED, 0.2), 0.95)),
                                ]),
                            );
                        }
                    }),
                )),
        );
        let mb = 41.7 * ease_out_cubic(bar_p);
        stack = stack.push(
            Positioned::new()
                .left(TERM.left + TERM.width() - 226.0)
                .top(TERM.top + TERM.height() - 84.0)
                .width(190.0)
                .height(28.0)
                .child(mono_tracked(
                    format!("{:5.1} MB", mb),
                    20.0,
                    tint(RED, 0.2),
                    1.0,
                )),
        );
    }

    // The battery — draining in the corner, green to red.
    let drain = clamp01((sec - 2.0) / 5.0);
    stack = stack.push(
        Positioned::new()
            .left(TERM.left + TERM.width() - 190.0)
            .top(TERM.top + 64.0)
            .width(150.0)
            .height(60.0)
            .child(Painting::sized(
                Size::new(150.0, 60.0),
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    let bw = 92.0;
                    let bh = 34.0;
                    let bx = 0.0;
                    let by = 12.0;
                    book.stroke_rrect(xywh(bx, by, bw, bh), 5.0, alpha(MUTED, 0.7), 2.0);
                    book.rrect(
                        xywh(bx + bw + 3.0, by + bh * 0.5 - 6.0, 7.0, 12.0),
                        2.0,
                        alpha(MUTED, 0.7),
                    );
                    let level = 1.0 - drain * 0.86;
                    let col = mix(TERM_GREEN, RED, clamp01(drain * 1.6));
                    let fw = (bw - 6.0) * level;
                    if fw > 2.0 {
                        book.rrect(xywh(bx + 3.0, by + 3.0, fw, bh - 6.0), 3.0, alpha(col, 0.9));
                    }
                }),
            )),
    );

    // Grain + dust — the degradation is rendered, on purpose.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            super::dust(book, w, h, t, 0x11A, 0.8);
            grain(book, w, h, frame_i, 1.5);
        }),
    )));

    // The act chip and caption.
    stack = stack.push(super::act_chip("I", "THE LIE", clamp01((sec - 0.4) / 0.5)));
    stack = stack.push(super::caption(
        "this is what shipping ui still looks like",
        1000.0,
        clamp01((sec - 1.2) / 0.5),
    ));

    stack.into()
}
