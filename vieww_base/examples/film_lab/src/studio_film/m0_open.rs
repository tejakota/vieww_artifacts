//! Z00 · the hush — the cold open.
//!
//! The film does not start on content. It starts on a dark room and one
//! point of light, and gives the audience a few seconds to settle before
//! anything asks to be read. Three lines arrive slowly, in plain words, and
//! the last one names who the film is for — everyone who designs apps,
//! builds them, or simply uses them. The point of light splits into four
//! as the second line lands: the four platforms the next scene is about.

use vieww_foundation::{Color, Offset, Rect, Size, Sketchbook, TextAlign};
use vieww_widget::prelude::*;

use crate::film_lib::{clamp01, ease_in_out, ease_out_cubic};
use crate::product_film as pf;
use super::{ACCENT, BRAND_FAR, CANVAS, INK, MUTED};

/// Where the light sits, in scene (= screen) coordinates.
const LIGHT: Offset = Offset::new(960.0, 420.0);

/// A slow fade: `from`..`from + len` seconds, eased.
fn slow(sec: f32, from: f32, len: f32) -> f32 {
    ease_in_out(clamp01((sec - from) / len))
}

pub fn the_hush(ctx: &pf::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let t = ctx.t;
    let mut stack = Stack::new();

    // The room — near black, a few stars that arrive late. The cold open
    // once carried a soft violet wash gathering behind the light; it is
    // gone with the rest of the radial glows (see `mod.rs`) — the hush is
    // now *drawn*: darkness, stars, and one crisp point of light.
    super::frame::ground(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            book.rect(Rect::new(0.0, 0.0, w, h), Color::rgb(6, 6, 9));
            pf::stars(book, w, h, 0x0B1E, 40, t, 0.05 * slow(sec, 3.0, 4.0));
            pf::vignette(book, w, h, 0.65);
        }),
    )));

    // The light — appears, breathes, then splits into four. A point of
    // light with an edge: a white core and a thin ring, bright by being
    // crisp rather than by being blurred — a drawn star, not a bloom.
    let appear = slow(sec, 0.8, 1.8);
    let split = ease_out_cubic(clamp01((sec - 5.4) / 2.2));
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            if appear <= 0.01 {
                return;
            }
            let breath = 1.0 + 0.08 * (sec * 1.1).sin();
            let spread = [(-1.5, 0.0), (-0.5, 0.0), (0.5, 0.0), (1.5, 0.0)];
            if split < 0.02 {
                book.ring(LIGHT, 46.0 * breath, 1.6, pf::alpha(BRAND_FAR, 0.60 * appear));
                book.ring(LIGHT, 58.0 * breath, 1.0, pf::alpha(BRAND_FAR, 0.28 * appear));
                book.circle(LIGHT, 7.0 * appear, pf::alpha(Color::WHITE, 0.95 * appear));
                return;
            }
            // Four lights leave the one, a hairline still tying each back.
            for (k, (dx, _)) in spread.iter().enumerate() {
                let x = LIGHT.dx + dx * 170.0 * split;
                let p = Offset::new(x, LIGHT.dy);
                book.line(LIGHT, p, pf::alpha(BRAND_FAR, 0.22 * split), 1.0);
                let col = [super::SYN_TYPE, BRAND_FAR, super::TERM_GREEN, super::SYN_MACRO][k];
                let tint = pf::mix(BRAND_FAR, col, split);
                book.ring(p, 30.0 * breath, 1.4, pf::alpha(tint, 0.55 * appear));
                book.circle(p, 6.0, pf::alpha(Color::WHITE, 0.92));
            }
            book.circle(LIGHT, 3.0, pf::alpha(Color::WHITE, 0.5 * (1.0 - split) + 0.2));
        }),
    )));

    // Three lines, each given time to be read.
    let lines: [(&str, f32, f32, f32, bool); 3] = [
        ("Every app you use began as one idea.", 2.2, 560.0, 58.0, false),
        ("Then it was built again — for every phone, tablet and computer.", 5.6, 648.0, 34.0, false),
        ("A film for everyone who designs apps, builds them, or simply uses them.", 8.4, 752.0, 28.0, true),
    ];
    for (text, at, y, size, accent) in lines {
        let a = slow(sec, at, 1.6);
        if a <= 0.01 {
            continue;
        }
        let rise = (1.0 - a) * 14.0;
        let style = if accent {
            pf::geist(size).letter_spacing(0.4).color(pf::alpha(ACCENT, 0.95))
        } else if size > 40.0 {
            pf::geist(size).color(pf::alpha(INK, 0.97))
        } else {
            pf::geist(size).color(pf::alpha(MUTED, 0.98))
        };
        stack = stack.push(super::frame::label(
            160.0, y + rise, 1600.0, size * 1.6,
            text.to_string(),
            style,
            TextAlign::Center, a,
        ));
    }
    stack.into()
}
