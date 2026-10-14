//! **S02 · THE COST** — the wait, integrated.
//!
//! One build is an annoyance; the scene's job is the *sum*. A calendar of a
//! working year fills in, minute by minute, as a field of marks — one mark
//! per rebuild — and the marks accumulate into a shape nobody looks at on
//! purpose. Beside it, three dials spin up: rebuilds, minutes, and the one
//! number that hurts, *days*.
//!
//! The arithmetic is stated on screen and done in this file, so the figure
//! is a consequence of two assumptions the audience can see and argue with —
//! rebuilds per day and seconds per rebuild — rather than a claim. That is
//! the honest form of a cost slide: show the multiplication.
//!
//! Still degraded, still 24-in-60. The last beat drains the marks away and
//! leaves one line of type on black, which is the door into S03.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Rect, Size, Sketchbook};
use vieww_widget::prelude::*;

use crate::film::Ctx;
use crate::kit::{
    alpha, caption, clamp01, drained, ease_out_cubic, grain, held_24, mix, painter, scanlines, seg,
    smoothstep, thousands, vignette, xywh, Rng, Type, AMBER, FAINT, H, INK, INK_SOFT, MUTED, RED,
    VIOLET, VOID, W,
};
use crate::studio::compose;

/// The two assumptions, printed on screen beside the result.
const REBUILDS_PER_DAY: u32 = 42;
const SECONDS_PER_REBUILD: u32 = 38;
const WORKING_DAYS: u32 = 230;

const DRAIN: f32 = 0.74;

fn dim(c: Color) -> Color {
    drained(c, DRAIN)
}

/// The marks' grid: a year of working days, forty-two marks each.
const COLS: usize = 46;
const ROWS: usize = 5;

pub fn frame(ctx: &Ctx) -> WidgetNode {
    let held = held_24(ctx.sec);
    let t = ctx.t;
    let frame_i = ctx.frame;

    // The fill: the grid completes over the scene's first two thirds.
    let fill = ease_out_cubic(clamp01(held / 9.6));
    // The drain: the last beat takes it all away.
    let drain = smoothstep(seg(t, 0.80, 0.97));

    let total_marks = COLS * ROWS * 8;
    let shown = (total_marks as f32 * fill) as usize;

    let rebuilds = f64::from(REBUILDS_PER_DAY) * f64::from(WORKING_DAYS) * f64::from(fill);
    let minutes = rebuilds * f64::from(SECONDS_PER_REBUILD) / 60.0;
    let days = minutes / 60.0 / 8.0;

    let vis = smoothstep(seg(t, 0.0, 0.06));

    let bg = painter(move |book: &mut Sketchbook, size: Size| {
        let (w, h) = (size.width, size.height);
        book.rect(
            Rect::new(0.0, 0.0, w, h),
            Gradient::vertical().with_dither().with_stops(&[
                (0.0, Color::rgb(9, 10, 13)),
                (0.6, Color::rgb(6, 7, 9)),
                (1.0, VOID),
            ]),
        );

        // The field of marks. Each mark is one rebuild: a short vertical
        // tick, coloured by how long that particular build ran — the long
        // ones are the amber, and there are more of them than anyone
        // remembers.
        let field = xywh(W * 0.085, H * 0.20, W * 0.83, H * 0.42);
        let cw = field.width() / COLS as f32;
        let ch = field.height() / ROWS as f32;
        let mut rng = Rng::new(0xC0_57);
        let mut drawn = 0usize;
        for row in 0..ROWS {
            for col in 0..COLS {
                for k in 0..8 {
                    let long = rng.f01();
                    let jitter = rng.f01();
                    if drawn >= shown {
                        break;
                    }
                    drawn += 1;
                    let x = field.left + col as f32 * cw + 2.0 + (k as f32 / 8.0) * (cw - 5.0);
                    let y = field.top + row as f32 * ch + 6.0 + jitter * (ch - 26.0);
                    let hgt = 4.0 + long * long * 16.0;
                    let age = clamp01((drawn as f32) / shown.max(1) as f32);
                    let c = if long > 0.86 {
                        alpha(dim(AMBER), 0.55)
                    } else if long > 0.62 {
                        alpha(dim(MUTED), 0.42)
                    } else {
                        alpha(dim(FAINT), 0.30)
                    };
                    // The newest marks arrive brighter and settle.
                    let arrive = clamp01((age - 0.94) / 0.06);
                    book.rect(
                        xywh(x, y, 1.6, hgt),
                        alpha(mix(c, dim(INK), arrive * 0.6), (1.0 - drain) * vis * f32::from(c.a) / 255.0),
                    );
                }
            }
        }

        // The week separators, so the field reads as a calendar and not as
        // noise.
        for col in 0..=COLS {
            if col % 5 == 0 {
                let x = field.left + col as f32 * cw;
                book.rect(
                    xywh(x, field.top, 1.0, field.height()),
                    alpha(dim(FAINT), 0.10 * (1.0 - drain) * vis),
                );
            }
        }

        // Three dials, right of centre, counting. Each is an arc whose sweep
        // is its own quantity normalised — a picture of the multiplication.
        let dials = [
            (rebuilds / 10_000.0, dim(MUTED)),
            (minutes / 8_000.0, dim(AMBER)),
            (days / 24.0, dim(RED)),
        ];
        let dy = H * 0.735;
        for (i, (p, c)) in dials.into_iter().enumerate() {
            let cx = W * 0.30 + i as f32 * W * 0.20;
            let r = 54.0;
            book.ring(Offset::new(cx, dy), r, 5.0, alpha(dim(Color::rgb(28, 30, 36)), 0.9 * vis));
            crate::kit::arc_sweep(
                book,
                Offset::new(cx, dy),
                r,
                5.0,
                -std::f32::consts::FRAC_PI_2,
                (clamp01(p as f32) * std::f32::consts::TAU).max(0.001),
                alpha(c, 0.85 * (1.0 - drain * 0.6) * vis),
            );
        }

        // The drain: as the marks leave, one violet thread survives — the
        // first colour in the film that is not grey, and the promise of the
        // cut into S03.
        if drain > 0.02 {
            book.blended_layer(1.0, 40.0, BlendMode::Plus, None, |g| {
                g.rect(
                    xywh(0.0, H * 0.5 - 2.0, W * drain, 4.0),
                    Gradient::horizontal().with_dither().with_stops(&[
                        (0.0, alpha(VIOLET, 0.0)),
                        (0.7, alpha(VIOLET, 0.45 * drain)),
                        (1.0, alpha(Color::WHITE, 0.55 * drain)),
                    ]),
                );
            });
        }

        scanlines(book, size, held, 0.75 * (1.0 - drain) * vis);
        grain(book, size, frame_i, 0.05 * vis, 800);
        vignette(book, size, 1.1);
        if drain > 0.6 {
            book.rect(Rect::new(0.0, 0.0, w, h), alpha(Color::BLACK, smoothstep(seg(drain, 0.6, 1.0)) * 0.55));
        }
    });

    let label = |text: String, x: f32, y: f32, size: f32, c: Color, a: f32| -> WidgetNode {
        Type::new(text)
            .mono()
            .size(size)
            .track(1.6)
            .color(alpha(c, a))
            .center()
            .at(x - 160.0, y)
            .width(320.0)
            .into()
    };

    let dial_a = (1.0 - drain) * vis;
    let mut nodes: Vec<WidgetNode> = vec![
        // The assumptions, stated. The audience is invited to disagree with
        // the inputs rather than the output.
        Type::new(format!(
            "{REBUILDS_PER_DAY} rebuilds a day   ×   {SECONDS_PER_REBUILD} seconds each   ×   {WORKING_DAYS} days"
        ))
        .mono()
        .size(17.0)
        .track(2.2)
        .color(alpha(dim(MUTED), 0.85 * (1.0 - drain) * vis))
        .center()
        .banner(H * 0.115)
        .into(),
        label(thousands(rebuilds as u64), W * 0.30, H * 0.735 - 16.0, 30.0, dim(INK), 0.95 * dial_a),
        label("rebuilds".into(), W * 0.30, H * 0.735 + 62.0, 12.0, dim(MUTED), 0.85 * dial_a),
        label(thousands(minutes as u64), W * 0.50, H * 0.735 - 16.0, 30.0, dim(INK), 0.95 * dial_a),
        label("minutes".into(), W * 0.50, H * 0.735 + 62.0, 12.0, dim(MUTED), 0.85 * dial_a),
        label(format!("{days:.0}"), W * 0.70, H * 0.735 - 16.0, 30.0, dim(RED), 0.95 * dial_a),
        label("working days, gone".into(), W * 0.70, H * 0.735 + 62.0, 12.0, dim(AMBER), 0.9 * dial_a),
    ];

    // The turn: one line, alone, as everything else goes.
    let line_a = smoothstep(seg(t, 0.84, 0.94));
    nodes.push(
        Type::new("a year of waiting for something you already wrote")
            .size(40.0)
            .light()
            .track(0.6)
            .color(alpha(INK_SOFT, 0.95 * line_a))
            .center()
            .banner(H * 0.46)
            .into(),
    );

    nodes.push(caption("one mark per rebuild · the arithmetic is on screen", (1.0 - drain) * vis * 0.85));
    compose(bg, nodes)
}
