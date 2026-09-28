//! **S03 · THE QUESTION** — the film's one sentence.
//!
//! Black. Silence. The degradation grammar stops here: no scanlines, no
//! drain, no judder — **the clock changes to smooth 60 on this cut**, and
//! that change is the first thing the film gives the audience, before it
//! gives them a product name.
//!
//! The question types itself, word by word, in the film's own voice, and
//! then one word of it — *see* — lifts out of the line and becomes the axis
//! of the frame: the word is stroked in violet, a horizontal rule draws
//! under it, and everything else steps back. A single word carrying the
//! argument is worth more than a bullet list of them.
//!
//! The last two seconds are a held black with one line of light, which is
//! the door the wordmark drops through.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Rect, Size, Sketchbook};
use vieww_widget::prelude::*;

use crate::film::Ctx;
use crate::kit::{
    alpha, bump, caption, clamp01, dust, ease_out_cubic, ease_out_expo, grain, mix, noise1, painter,
    seg, smootherstep, smoothstep, vignette, xywh, Type, CYAN, H, INK, INK_SOFT, MUTED, VIOLET,
    VIOLET_SOFT, VOID, W,
};
use crate::studio::compose;

/// The question, in the words it is asked in. Each entry is one word and the
/// beat it arrives on, in scene-seconds — the rhythm is the writing.
const WORDS: &[(&str, f32)] = &[
    ("how", 1.05),
    ("long", 1.32),
    ("should", 1.66),
    ("it", 1.92),
    ("take", 2.18),
    ("to", 2.52),
    ("see", 2.86),
    ("what", 3.42),
    ("you", 3.66),
    ("built?", 3.94),
];

/// Which word lifts. Index into `WORDS` — *see*.
const HERO: usize = 6;

pub fn frame(ctx: &Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let t = ctx.t;
    let frame_i = ctx.frame;

    // The lift: the hero word separates from the line, grows, and the rest
    // of the sentence recedes.
    let lift = smootherstep(seg(sec, 5.4, 7.0));
    // The rule under the hero word draws itself.
    let rule = ease_out_expo(seg(sec, 6.2, 7.6));
    // The close: everything but one thread of light leaves.
    let close = smoothstep(seg(sec, 9.4, 11.4));

    let bg = painter(move |book: &mut Sketchbook, size: Size| {
        let (w, h) = (size.width, size.height);
        // The deepest black in the film so far — the scene has nothing in it
        // but the sentence.
        book.rect(
            Rect::new(0.0, 0.0, w, h),
            Gradient::vertical().with_dither().with_stops(&[
                (0.0, Color::rgb(6, 6, 9)),
                (0.5, VOID),
                (1.0, Color::rgb(5, 5, 9)),
            ]),
        );

        // One slow violet breath behind the line — the first product colour
        // in the film, and it arrives before the product does.
        let breath = 0.5 + 0.5 * (sec * 0.42 + noise1(sec * 0.3, 0x0B1) * 0.6).sin();
        book.blended_layer(1.0, 90.0, BlendMode::Plus, None, |g| {
            g.circle(
                Offset::new(w * 0.5, h * (0.5 - lift * 0.06)),
                w * 0.30,
                Gradient::radial_fill().with_dither().with_stops(&[
                    (0.0, alpha(VIOLET, (0.055 + 0.03 * breath) * (1.0 - close))),
                    (1.0, alpha(VIOLET, 0.0)),
                ]),
            );
        });

        // The rule under the hero word, which is also the film's first
        // lineage gradient: violet into cyan, the studio resting on the
        // framework, said in a line before it is said in words.
        if rule > 0.01 {
            let rw = 420.0 * rule;
            let rx = w * 0.5 - rw * 0.5;
            let ry = h * 0.5 + 76.0;
            book.rrect(
                xywh(rx, ry, rw, 3.0),
                1.5,
                Gradient::horizontal().with_dither().with_stops(&[
                    (0.0, alpha(VIOLET, 0.0)),
                    (0.35, alpha(VIOLET, 0.9 * (1.0 - close))),
                    (1.0, alpha(CYAN, 0.75 * (1.0 - close))),
                ]),
            );
            book.blended_layer(1.0, 22.0, BlendMode::Plus, None, |g| {
                g.rrect(xywh(rx, ry - 2.0, rw, 7.0), 3.5, alpha(VIOLET, 0.22 * rule * (1.0 - close)));
            });
        }

        // The closing thread: as the words go, a single bright line survives
        // at the centre and narrows to a seam — the cut into S04 happens
        // *through* it.
        if close > 0.02 {
            let cw = w * (1.0 - smoothstep(close) * 0.86);
            let bright = bump(close, 0.0, 1.0);
            book.blended_layer(1.0, 26.0, BlendMode::Plus, None, |g| {
                g.rrect(
                    xywh(w * 0.5 - cw * 0.5, h * 0.5 - 1.5, cw, 3.0),
                    1.5,
                    Gradient::horizontal().with_dither().with_stops(&[
                        (0.0, alpha(VIOLET, 0.0)),
                        (0.5, alpha(mix(VIOLET_SOFT, Color::WHITE, 0.5), 0.85 * bright + 0.3 * close)),
                        (1.0, alpha(CYAN, 0.0)),
                    ]),
                );
            });
        }

        dust(book, size, sec, 30, VIOLET_SOFT, 0.5 * (1.0 - close));
        // The grain is *gone* from here on — Act I's texture ends with Act
        // I's clock. A faint one stays, because a perfectly clean black
        // reads as a missing frame.
        grain(book, size, frame_i, 0.012, 300);
        vignette(book, size, 0.95);
    });

    // The words. Each arrives on its own beat with a small rise; the hero
    // word, once lifted, is placed on its own line and scaled up.
    let mut nodes: Vec<WidgetNode> = Vec::new();

    // Layout the sentence by measured-enough proportions: the line is
    // centred, so each word is positioned by its cumulative share of the
    // string's own characters. The type is centred per-word, which keeps the
    // rhythm even if the face's advance differs from the bench's.
    let total_chars: f32 = WORDS.iter().map(|(w, _)| w.chars().count() as f32 + 1.0).sum();
    let line_w = 1180.0;
    let mut cursor = 0.0f32;
    let size_base = 52.0;

    for (i, (word, beat)) in WORDS.iter().enumerate() {
        let chars = word.chars().count() as f32 + 1.0;
        let share = chars / total_chars;
        let x0 = W * 0.5 - line_w * 0.5 + cursor * line_w;
        let wide = share * line_w;
        cursor += share;

        let arrive = ease_out_cubic(seg(sec, *beat, beat + 0.44));
        if arrive <= 0.004 {
            continue;
        }
        let is_hero = i == HERO;

        // The recede: non-hero words fade and drift down a little.
        let recede = if is_hero { 0.0 } else { lift };
        let a = arrive * (1.0 - recede * 0.78) * (1.0 - close);
        if a <= 0.004 {
            continue;
        }

        if is_hero {
            // The hero word is drawn twice: in place, fading out as it
            // lifts; and lifted, centred, growing. One word, two states, so
            // the move is legible rather than a jump cut.
            let stay = 1.0 - lift;
            if stay > 0.01 {
                nodes.push(
                    Type::new(*word)
                        .size(size_base)
                        .light()
                        .color(alpha(INK, 0.95 * arrive * stay))
                        .center()
                        .at(x0, H * 0.5 - 34.0 + (1.0 - arrive) * 16.0)
                        .width(wide)
                        .into(),
                );
            }
            if lift > 0.01 {
                let s = size_base * (1.0 + 1.45 * lift);
                nodes.push(
                    Type::new(*word)
                        .size(s)
                        .weight(vieww_foundation::FontWeight::Medium)
                        .track(2.0 * lift)
                        .color(alpha(mix(INK, VIOLET_SOFT, 0.22 * lift), 0.98 * lift * (1.0 - close)))
                        .center()
                        .banner(H * 0.5 - s * 0.62)
                        .into(),
                );
            }
        } else {
            nodes.push(
                Type::new(*word)
                    .size(size_base * (1.0 - recede * 0.22))
                    .light()
                    .color(alpha(mix(INK, MUTED, recede), 0.95 * a))
                    .center()
                    .at(x0, H * 0.5 - 34.0 + (1.0 - arrive) * 16.0 + recede * 26.0)
                    .width(wide)
                    .into(),
            );
        }
    }

    // The answer's shape, not the answer: under the rule, a unit that says
    // what the rest of the film will measure.
    let ans = smoothstep(seg(sec, 7.4, 8.6)) * (1.0 - close);
    nodes.push(
        Type::new("the honest answer is a number, and it is measured")
            .mono()
            .size(16.0)
            .track(2.6)
            .color(alpha(MUTED, 0.85 * ans))
            .center()
            .banner(H * 0.5 + 106.0)
            .into(),
    );

    nodes.push(caption(
        "the clock changes here · 60 frames in 60, from this cut on",
        smoothstep(seg(t, 0.06, 0.18)) * (1.0 - smoothstep(seg(t, 0.72, 0.84))) * 0.9,
    ));
    let _ = ctx.spine.witness;
    compose(bg, nodes)
}
