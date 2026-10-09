//! Movement III, deepened — three scenes the studio act earned.
//!
//! * **Z13C · the arena.** Every touch is understood: a synthetic finger
//!   drives the real `vieww-gestures` recognisers through their arena — a
//!   tap tried, a drag won, a release thrown — and the shapes it moves live
//!   on a `vieww-canvas` retained stage with the Transformer's own handles.
//!   The fling that follows is `VelocityTracker` → `Fling`, the framework's
//!   own pipeline, and the settle time is measured from the curve.
//! * **Z13D · the designer.** Motion from the design team, played natively:
//!   a hand-authored Bodymovin composition parsed and rendered by
//!   `vieww-lottie` into this film's own vector display list, beside the
//!   asset pipeline that ships with it — `vieww-image`'s sprite sheet, mip
//!   chain and atlas, decoded by `vieww-asset`'s own codec.
//! * **Z16B · together.** The studio, when the work is shared: two authors,
//!   one document, no lock — `vieww-collab`'s RGA text, typed
//!   concurrently, merged both ways, presence cursors and all. The receipt
//!   compares the snapshots for real.
//!
//! The arena's stage is the film's one piece of cross-frame state: a
//! thread-local that resets the moment the frame index goes backwards, so
//! a resume replays the same pixels (the same guard the studio act's own
//! session driver obeys). Everything else here is a pure function of the
//! frame's index — the collab session is replayed from zero every frame.

use std::cell::RefCell;
use std::time::Duration;

use vieww_foundation::{
    BlendMode, Color, Offset, Path, PointerEvent, PointerId, Rect, Sketch, Sketchbook, TextAlign,
    Transform,
};
use vieww_widget::prelude::*;

use super::{
    filmkit, frame, ACCENT, BRAND_FAR, BRAND_NEAR, CANVAS, FPS, GROUND, INK, LEDGER, MUTED,
    SURFACE, SURFACE_2, SYN_COMMENT, SYN_FUNCTION, SYN_STRING, SYN_TYPE, TERM_GREEN,
};
use crate::film_lib::{clamp01, ease_out_expo};
use crate::product_film as pf;

// ── Z13C · the arena ────────────────────────────────────────────────────────

/// Where the stage card sits on the frame, and the stage's own size.
const CARD: Rect = Rect {
    left: 232.0,
    top: 296.0,
    right: 792.0,
    bottom: 916.0,
};
const STAGE_W: f32 = 520.0;
const STAGE_H: f32 = 580.0;

/// The finger's script — every event, with its time. Deterministic by
/// construction: the film types it, the recognisers read it.
fn finger_events() -> Vec<(f32, PointerEvent)> {
    let id = PointerId(1);
    let mut out = Vec::new();
    // Phase 1 — the tap try: down, a still hold, up.
    let tap_at = Offset::new(170.0, 140.0);
    out.push((
        1.0,
        PointerEvent::down(id, tap_at, Duration::from_secs_f32(1.0)),
    ));
    out.push((
        2.2,
        PointerEvent::up(id, tap_at, Duration::from_secs_f32(2.2)),
    ));
    // Phase 2 — the drag: down on the star, an S-curve of moves, up.
    let start = Offset::new(390.0, 300.0);
    let end = Offset::new(150.0, 430.0);
    out.push((
        3.2,
        PointerEvent::down(id, start, Duration::from_secs_f32(3.2)),
    ));
    let moves = 12;
    for k in 1..=moves {
        let u = k as f32 / moves as f32;
        let ts = 3.5 + u * 2.5;
        let bend = (u * std::f32::consts::PI).sin() * 90.0;
        let p = Offset::new(
            start.dx + (end.dx - start.dx) * u,
            start.dy + (end.dy - start.dy) * u - bend * (1.0 - u),
        );
        out.push((
            ts,
            PointerEvent::moved(id, p, p, Duration::from_secs_f32(ts)),
        ));
    }
    out.push((6.2, PointerEvent::up(id, end, Duration::from_secs_f32(6.2))));
    out
}

/// The finger's position at `sec` (for the comet), or where it last was.
fn finger_at(sec: f32) -> Option<Offset> {
    let mut pos = None;
    for (t, e) in &finger_events() {
        if *t <= sec {
            pos = Some(e.position);
        }
    }
    pos
}

/// The release velocity, measured by the framework's own tracker from the
/// finger's own event stream.
fn fling_velocity() -> Offset {
    let mut tracker = vieww_gestures::VelocityTracker::new();
    for (t, e) in finger_events() {
        if (3.2..=6.2).contains(&t) {
            tracker.add(Duration::from_secs_f32(t), e.position);
        }
    }
    tracker.velocity()
}

/// When the fling settles — measured from the curve, not asserted.
fn fling_settle_s() -> f32 {
    let v = fling_velocity().dx.min(0.0);
    let fling = vieww_gestures::Fling::new(0.0, v, 0.035);
    let mut t = 0.0_f32;
    while t < 6.0 {
        if fling.velocity_at(Duration::from_secs_f32(t)).abs() < 5.0 {
            return t;
        }
        t += 1.0 / 120.0;
    }
    6.0
}

/// The stage, kept across frames — the film's one retained surface.
struct ArenaState {
    stage: vieww_canvas::Stage,
    layer: vieww_canvas::NodeId,
    star: vieww_canvas::NodeId,
    timeline: vieww_canvas::tween::Timeline,
    built_at: f32,
}

thread_local! {
    static ARENA: RefCell<Option<ArenaState>> = const { RefCell::new(None) };
}

/// Build the arena's stage: five shapes, one layer, one star to drag.
fn build_arena() -> ArenaState {
    use vieww_canvas::{Attrs, Shape, Stage};
    let mut stage = Stage::new(STAGE_W, STAGE_H);
    let layer = stage.add_layer();
    let mk = |x: f32, y: f32, fill: Option<Color>| Attrs {
        x,
        y,
        fill,
        ..Attrs::default()
    };
    stage.add_shape(
        layer,
        mk(96.0, 120.0, Some(SURFACE_2)),
        Shape::Rect {
            width: 132.0,
            height: 96.0,
            corner: 12.0,
        },
    );
    stage.add_shape(
        layer,
        mk(210.0, 240.0, Some(GROUND)),
        Shape::Circle { radius: 34.0 },
    );
    let star = stage.add_shape(
        layer,
        mk(390.0, 300.0, Some(BRAND_NEAR)),
        Shape::Star {
            points: 5,
            inner: 22.0,
            outer: 48.0,
        },
    );
    let mut hex = mk(180.0, 470.0, None);
    hex.stroke = Some(SYN_TYPE);
    hex.stroke_width = 2.0;
    stage.add_shape(
        layer,
        hex,
        Shape::RegularPolygon {
            sides: 6,
            radius: 40.0,
        },
    );
    let mut arrow = mk(260.0, 545.0, None);
    arrow.stroke = Some(MUTED);
    arrow.stroke_width = 2.0;
    stage.add_shape(
        layer,
        arrow,
        Shape::Arrow {
            points: vec![Offset::new(40.0, 0.0), Offset::new(440.0, 0.0)],
            head: 14.0,
        },
    );
    // The settle flourish: a yoyo on the star once the drag lets go. The
    // timeline is stateful by design — it locks the tween's start to the
    // first frame it sees, which is why the stage persists across frames.
    let mut timeline = vieww_canvas::tween::Timeline::new();
    timeline.to(vieww_canvas::tween::NodeTween::new(
        star,
        vieww_canvas::tween::To::new().scale(1.12).opacity(0.82),
        Duration::from_secs_f32(0.5),
    )
    .curve(vieww_animation::Curve::EASE_IN_OUT)
    .repeat(3, true)
    .delay(Duration::from_secs_f32(6.6)));
    ArenaState {
        stage,
        layer,
        star,
        timeline,
        built_at: -1.0,
    }
}

/// Advance the arena to `sec`: drag the star, run the tween, scroll the
/// fling. Replays from zero if the frame index regresses.
fn arena_advance(sec: f32) {
    ARENA.with(|cell| {
        let mut guard = cell.borrow_mut();
        let reset = guard.as_ref().is_none_or(|s| sec + 1.0 < s.built_at);
        if reset {
            *guard = Some(build_arena());
        }
        let st = guard.as_mut().expect("arena built");
        if (sec - st.built_at).abs() < 0.5 / FPS {
            return; // already advanced to this frame
        }
        st.built_at = sec;
        // The drag: while the finger is down on the star, it follows.
        if (3.2..=6.2).contains(&sec) {
            if let Some(p) = finger_at(sec) {
                st.stage.attrs_mut(st.star).x = p.dx;
                st.stage.attrs_mut(st.star).y = p.dy;
            }
        }
        // The fling: after the release, the layer scrolls by the decay.
        if sec > 6.2 {
            let v = fling_velocity().dx.min(0.0);
            let fling = vieww_gestures::Fling::new(0.0, v, 0.035);
            let drift = fling.position(Duration::from_secs_f32((sec - 6.2).max(0.0)));
            st.stage.attrs_mut(st.layer).x = drift;
        }
        // The tween's yoyo settles over the transformed star.
        st.timeline
            .update(&mut st.stage, Duration::from_secs_f32(sec));
    });
}

/// Z13C — the arena: a finger, its recognisers, and the physics of letting go.
pub(crate) fn the_arena(ctx: &pf::Ctx) -> WidgetNode {
    use vieww_canvas::Transformer;
    use vieww_gestures::{recognize, DragRecognizer, GestureDispatcher, Recognized, TapRecognizer};

    let sec = ctx.sec;
    let t_stars = ctx.t;
    arena_advance(sec);
    let mut stack = Stack::new();

    frame::ground(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            book.rect(Rect::new(0.0, 0.0, s.width, s.height), Color::rgb(7, 6, 10));
            pf::stars(book, s.width, s.height, 0xA11E, 40, t_stars, 0.05);
            pf::vignette(book, s.width, s.height, 0.55);
        }),
    )));

    // Recognise, from a fresh dispatcher, over the events so far — the log
    // is the recognisers' own words.
    let events: Vec<PointerEvent> = finger_events()
        .into_iter()
        .filter(|(t, _)| *t <= sec)
        .map(|(_, e)| e)
        .collect();
    let last_event_s = finger_events()
        .into_iter()
        .rfind(|(t, _)| *t <= sec)
        .map(|(t, _)| t);
    let mut dispatcher = GestureDispatcher::new();
    dispatcher.add(TapRecognizer::new());
    dispatcher.add(DragRecognizer::new());
    let heard = recognize(&mut dispatcher, &events);

    // The stage card, its contents clipped to the card.
    let card_a = clamp01(sec / 0.7);
    let stage_items = ARENA.with(|c| c.borrow().as_ref().map(|s| s.stage.render().0));
    let transformer_book = ARENA.with(|c| {
        c.borrow()
            .as_ref()
            .filter(|_| sec > 6.6 && sec < 9.6)
            .map(|s| Transformer::new(s.star).draw(&s.stage))
    });
    let finger_now = finger_at(sec);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            if card_a <= 0.01 {
                return;
            }
            book.rrect(CARD, 18.0, pf::alpha(SURFACE, 0.9 * card_a));
            book.stroke_rrect(CARD, 18.0, pf::alpha(SYN_COMMENT, 0.3 * card_a), 1.4);
            // The stage, translated into the card and clipped to it.
            if let Some(items) = &stage_items {
                let clip = Path::rounded_rect(CARD, 18.0);
                book.push(Sketch::Layer {
                    alpha: card_a,
                    blur: 0.0,
                    blend: BlendMode::Normal,
                    clip: Some(clip),
                    children: vec![Sketch::Transformed {
                        transform: Transform::translate(Offset::new(
                            CARD.left + 14.0,
                            CARD.top + 14.0,
                        )),
                        children: items.clone().into_items(),
                    }],
                });
            }
            // The transformer's own handles, over everything.
            if let Some(tb) = &transformer_book {
                for item in tb.clone().into_items() {
                    book.push(Sketch::Transformed {
                        transform: Transform::translate(Offset::new(
                            CARD.left + 14.0,
                            CARD.top + 14.0,
                        )),
                        children: vec![item],
                    });
                }
            }
            // The finger — a comet over the surface, wherever it is.
            if let Some(p) = finger_now {
                let live = last_event_s.is_some_and(|t| sec - t < 0.4);
                if live {
                    let at = Offset::new(CARD.left + 14.0 + p.dx, CARD.top + 14.0 + p.dy);
                    book.ring(at, 22.0, 2.0, pf::alpha(Color::WHITE, 0.7));
                    book.circle(at, 6.0, pf::alpha(Color::WHITE, 0.9));
                }
            }
        }),
    )));

    // The log — the recognisers' own words, arriving as they happen.
    let log_x = 880.0;
    let log_in = clamp01((sec - 0.9) / 0.5);
    if log_in > 0.01 && !heard.is_empty() {
        let mut lines: Vec<(String, Color)> = Vec::new();
        let mut updates = 0usize;
        let flush = |lines: &mut Vec<(String, Color)>, updates: usize| {
            if updates > 0 {
                lines.push((
                    format!("DragUpdate ×{updates} — the shape follows"),
                    SYN_TYPE,
                ));
            }
        };
        for r in &heard {
            match r {
                Recognized::TapDown(_) => {
                    lines.push(("TapDown — the press registers".to_string(), SYN_TYPE))
                }
                Recognized::Tap(_) => {
                    lines.push(("Tap — clean press and release".to_string(), TERM_GREEN))
                }
                Recognized::TapCancel => {
                    lines.push(("TapCancel — the tap bows out".to_string(), ACCENT))
                }
                Recognized::DragStart(_) => {
                    lines.push(("DragStart — the arena awards the drag".to_string(), ACCENT))
                }
                Recognized::DragUpdate(_) => updates += 1,
                Recognized::DragEnd(_) => {
                    flush(&mut lines, updates);
                    updates = 0;
                    lines.push(("DragEnd — released with velocity".to_string(), SYN_STRING));
                }
                _ => {}
            }
        }
        flush(&mut lines, updates);
        for (k, (text, col)) in lines.iter().take(7).enumerate() {
            stack = stack.push(frame::label(
                log_x,
                300.0 + k as f32 * 40.0,
                940.0,
                30.0,
                text.clone(),
                pf::geist(19.0).color(pf::alpha(*col, 0.95)),
                TextAlign::Left,
                clamp01((log_in - 0.06 * k as f32) / 0.3),
            ));
        }
    }

    // The arena's verdict, once the drag has won.
    if sec > 3.9 {
        let verdict = if sec < 6.2 {
            "the arena: drag wins — the tap cancels"
        } else {
            "released — velocity measured from the last moves"
        };
        stack = stack.push(frame::label(
            log_x,
            590.0,
            940.0,
            30.0,
            verdict.to_string(),
            pf::geist(21.0).color(pf::alpha(ACCENT, 0.95)),
            TextAlign::Left,
            clamp01((sec - 3.9) / 0.4),
        ));
    }

    // The fling curve — the framework's own decay, drawn live, with the
    // settle marked where the curve dies.
    if sec > 6.4 {
        let v = fling_velocity();
        let settle = fling_settle_s();
        let (x0, x1, y) = (log_x, log_x + 620.0, 780.0);
        let fling = vieww_gestures::Fling::new(0.0, v.dx.min(0.0), 0.035);
        stack = stack.push(Positioned::fill().child(Painting::sized(
            CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                let span = 4.0;
                let mut path = Path::new();
                let steps = 160;
                for i in 0..=steps {
                    let t = span * i as f32 / steps as f32;
                    let vel = fling.velocity_at(Duration::from_secs_f32(t));
                    let p = Offset::new(x0 + (x1 - x0) * t / span, y - (vel.abs() / 1400.0) * 96.0);
                    if i == 0 {
                        path.move_to(p);
                    } else {
                        path.line_to(p);
                    }
                }
                book.stroke(path, pf::alpha(TERM_GREEN, 0.8), 1.8);
                let sx = x0 + (x1 - x0) * settle.min(span) / span;
                book.line(
                    Offset::new(sx, y + 12.0),
                    Offset::new(sx, y - 96.0),
                    pf::alpha(ACCENT, 0.6),
                    1.2,
                );
                book.circle(Offset::new(sx, y), 4.0, pf::alpha(ACCENT, 0.9));
                book.line(
                    Offset::new(x0, y),
                    Offset::new(x1, y),
                    pf::alpha(SYN_COMMENT, 0.3),
                    1.0,
                );
            }),
        )));
        stack = stack.push(frame::label(
            log_x,
            810.0,
            940.0,
            24.0,
            format!(
                "velocity {} px/s at release · settles in {:.2} s — measured from the curve",
                v.dx.abs().round() as i32,
                settle
            ),
            pf::geist_mono(16.0).color(pf::alpha(LEDGER, 0.95)),
            TextAlign::Left,
            clamp01((sec - 6.6) / 0.5),
        ));
    }

    stack = stack.push(frame::caption(
        "Every touch is understood.",
        1002.0,
        clamp01((sec - 0.2) / 0.5),
    ));
    stack = stack.push(frame::caption(
        "Tap and drag compete in an arena — then physics carries the release.",
        966.0,
        clamp01((sec - 4.6) / 0.6),
    ));
    let nodes = ARENA.with(|c| c.borrow().as_ref().map(|s| s.stage.len()).unwrap_or(0));
    let node_line = format!("vieww-canvas · {} nodes retained", nodes);
    stack = stack.push(frame::receipts(
        &[
            ("vieww-gestures · the arena", ACCENT),
            (node_line.as_str(), SYN_TYPE),
            ("transformer handles, live", LEDGER),
        ],
        0.0,
        0.0,
        clamp01((sec - 7.6) / 0.6),
    ));
    frame::boxed(Rect::new(CARD.left, CARD.top, CARD.right, CARD.bottom));
    stack.into()
}

// ── Z13D · the designer ─────────────────────────────────────────────────────

/// The designer's own file, as a parsed composition — read once per process.
fn lottie_comp() -> vieww_lottie::Composition {
    static S: std::sync::OnceLock<Option<vieww_lottie::Composition>> = std::sync::OnceLock::new();
    S.get_or_init(|| vieww_lottie::Composition::parse(include_str!("designer_mark.json")).ok())
        .clone()
        .expect("the designer's composition parses")
}

/// The sprite sheet the asset pipeline ships — built once by the film's own
/// rasterizer: sixteen marks, four tints × four rotations.
fn sprite_sheet() -> vieww_foundation::Image {
    use std::sync::OnceLock;
    use vieww_paint::native::NativeRenderer;
    use vieww_render::FrameDriver;
    static S: OnceLock<vieww_foundation::Image> = OnceLock::new();
    S.get_or_init(|| {
        let size = Size::new(512.0, 512.0);
        let mut driver = FrameDriver::new(size);
        driver.set_root(Positioned::fill().child(Painting::sized(
            size,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                book.rect(Rect::new(0.0, 0.0, 512.0, 512.0), Color::rgb(16, 15, 22));
                let cols = [BRAND_NEAR, BRAND_FAR, SYN_TYPE, TERM_GREEN];
                for r in 0..4 {
                    for c in 0..4 {
                        let k = r * 4 + c;
                        let cx = 64.0 + c as f32 * 128.0;
                        let cy = 64.0 + r as f32 * 128.0;
                        let tint = cols[k % 4];
                        let rot = k as f32 * 22.5_f32.to_radians();
                        let pts: Vec<Offset> = (0..4)
                            .map(|i| {
                                let a = rot
                                    + i as f32 * std::f32::consts::FRAC_PI_2
                                    + std::f32::consts::FRAC_PI_4;
                                Offset::new(cx + 38.0 * a.cos(), cy + 38.0 * a.sin())
                            })
                            .collect();
                        let mut path = Path::new();
                        path.move_to(pts[0]);
                        for p in &pts[1..] {
                            path.line_to(*p);
                        }
                        path.line_to(pts[0]);
                        book.fill(path.clone(), pf::alpha(tint, 0.25));
                        book.stroke(path, pf::alpha(tint, 0.95), 5.0);
                        book.circle(Offset::new(cx, cy), 7.0, pf::alpha(Color::WHITE, 0.9));
                    }
                }
            }),
        )));
        driver.draw_frame_at(Duration::ZERO);
        let mut renderer = NativeRenderer::new();
        let (pixels, _) = renderer
            .render_to_pixels(driver.scene(), 512, 512, Color::rgb(16, 15, 22))
            .expect("the sprite sheet rasterises");
        vieww_foundation::Image::from_rgba8(pixels.data().to_vec(), 512, 512)
    })
    .clone()
}

/// The sheet, PNG-encoded then decoded by `vieww-asset`'s own codec — the
/// round trip the receipt reports.
fn asset_roundtrip() -> (usize, u32, u32) {
    use image::codecs::png::PngEncoder;
    use image::{ExtendedColorType, ImageEncoder};
    static S: std::sync::OnceLock<(usize, u32, u32)> = std::sync::OnceLock::new();
    *S.get_or_init(|| {
        let sheet = sprite_sheet();
        let mut png = Vec::new();
        // The encode half is the `image` crate's; the decode half is
        // vieww's own — that is the point.
        let _ = PngEncoder::new(&mut png).write_image(
            sheet.pixels(),
            sheet.width(),
            sheet.height(),
            ExtendedColorType::Rgba8,
        );
        let (img, _fmt) = vieww_asset::decode(&png).expect("vieww-asset decodes its own PNG");
        (png.len(), img.width(), img.height())
    })
}

/// Z13D — the designer: Lottie played natively, beside the asset pipeline.
pub(crate) fn the_designer(ctx: &pf::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let comp = lottie_comp();
    let mut stack = Stack::new();

    frame::ground(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            book.rect(Rect::new(0.0, 0.0, s.width, s.height), Color::rgb(7, 6, 10));
            pf::vignette(book, s.width, s.height, 0.55);
        }),
    )));

    let appear = ease_out_expo(clamp01(sec / 1.4));

    // ── Left panel: the file itself. ────────────────────────────────────
    let file_card = Rect::new(168.0, 292.0, 728.0, 780.0);
    let (png_bytes, dec_w, dec_h) = asset_roundtrip();
    let json_len = include_str!("designer_mark.json").len();
    let layers = comp.layer_names().len();
    let unsupported: usize = comp.unsupported.values().sum();
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            if appear <= 0.01 {
                return;
            }
            book.rrect(file_card, 16.0, pf::alpha(SURFACE, 0.85 * appear));
            book.stroke_rrect(file_card, 16.0, pf::alpha(SYN_COMMENT, 0.3 * appear), 1.2);
            let head = Rect::new(
                file_card.left,
                file_card.top,
                file_card.right,
                file_card.top + 44.0,
            );
            book.rrect(head, 14.0, pf::alpha(SURFACE_2, 0.9 * appear));
            for (k, c) in [ACCENT, SYN_STRING, SYN_COMMENT].iter().enumerate() {
                book.circle(
                    Offset::new(
                        file_card.left + 22.0 + k as f32 * 20.0,
                        file_card.top + 22.0,
                    ),
                    5.0,
                    pf::alpha(*c, 0.8 * appear),
                );
            }
        }),
    )));
    stack = stack.push(frame::label(
        file_card.left + 76.0,
        file_card.top + 10.0,
        560.0,
        26.0,
        "mark.json — the designer's After Effects export".to_string(),
        pf::geist_mono(16.0).color(pf::alpha(MUTED, 0.95)),
        TextAlign::Left,
        appear,
    ));
    // A few lines of the actual JSON, dimmed — the file is the receipt.
    let json_lines = [
        "{ \"v\": \"5.9.0\", \"fr\": 60, \"w\": 300,",
        "  \"layers\": [ { \"ty\": 4, \"nm\": \"ring\",",
        "    \"shapes\": [ { \"ty\": \"el\",",
        "      \"s\": { \"a\": 0, \"k\": [176, 176] } },",
        "      { \"ty\": \"st\", \"w\": { \"a\": 0, \"k\": 7 } },",
        "      { \"ty\": \"tm\", \"e\": { \"a\": 1,",
        "        \"k\": [{ \"t\": 0, \"s\": [0] },",
        "               { \"t\": 55, \"s\": [100] }] } }",
        "  ] } ] }",
    ];
    for (k, line) in json_lines.iter().enumerate() {
        stack = stack.push(frame::label(
            file_card.left + 24.0,
            file_card.top + 68.0 + k as f32 * 25.0,
            file_card.width() - 48.0,
            22.0,
            line.to_string(),
            pf::geist_mono(14.0).color(pf::alpha(SYN_COMMENT, 0.85)),
            TextAlign::Left,
            clamp01((appear - 0.04 * k as f32) / 0.4),
        ));
    }
    stack = stack.push(frame::label(
        file_card.left + 24.0,
        file_card.bottom - 118.0,
        file_card.width() - 48.0,
        24.0,
        format!(
            "{} bytes · {} layers · {} skipped features",
            json_len, layers, unsupported
        ),
        pf::geist_mono(15.0).color(pf::alpha(LEDGER, 0.95)),
        TextAlign::Left,
        clamp01((sec - 1.6) / 0.5),
    ));
    stack = stack.push(frame::label(
        file_card.left + 24.0,
        file_card.bottom - 84.0,
        file_card.width() - 48.0,
        24.0,
        format!(
            "the PNG beside it: {} bytes → {}×{} by vieww-asset",
            png_bytes, dec_w, dec_h
        ),
        pf::geist_mono(15.0).color(pf::alpha(LEDGER, 0.95)),
        TextAlign::Left,
        clamp01((sec - 5.4) / 0.5),
    ));

    // ── Centre card: the composition, playing. ─────────────────────────
    let play_card = Rect::new(790.0, 292.0, 1250.0, 812.0);
    let fit = Rect::new(
        play_card.left + 30.0,
        play_card.top + 76.0,
        play_card.right - 30.0,
        play_card.bottom - 30.0,
    );
    let comp_frame = comp.frame_at(sec);
    let played = comp.render(comp_frame, fit);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            if appear <= 0.01 {
                return;
            }
            book.rrect(play_card, 18.0, pf::alpha(SURFACE, 0.92 * appear));
            book.stroke_rrect(play_card, 18.0, pf::alpha(SYN_COMMENT, 0.3 * appear), 1.4);
        }),
    )));
    stack = stack.push(frame::label(
        play_card.left + 30.0,
        play_card.top + 18.0,
        play_card.width() - 60.0,
        28.0,
        "played by vieww-lottie — every frame, the vectors".to_string(),
        pf::geist(19.0).color(pf::alpha(ACCENT, 0.95)),
        TextAlign::Left,
        appear,
    ));
    // The composition itself — its items ride the film's own book.
    let comp_items = played.into_items();
    let comp_a = clamp01((sec - 0.8) / 0.8);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            if comp_a <= 0.01 {
                return;
            }
            book.push(Sketch::Layer {
                alpha: comp_a,
                blur: 0.0,
                blend: BlendMode::Normal,
                clip: None,
                children: comp_items.clone(),
            });
        }),
    )));

    // ── Right panel: the asset pipeline. ───────────────────────────────
    let pipe_card = Rect::new(1312.0, 292.0, 1752.0, 812.0);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            if appear <= 0.01 {
                return;
            }
            book.rrect(pipe_card, 18.0, pf::alpha(SURFACE, 0.9 * appear));
            book.stroke_rrect(pipe_card, 18.0, pf::alpha(SYN_COMMENT, 0.3 * appear), 1.4);
        }),
    )));
    stack = stack.push(frame::label(
        pipe_card.left + 30.0,
        pipe_card.top + 18.0,
        pipe_card.width() - 60.0,
        28.0,
        "the asset pipeline — vieww-image".to_string(),
        pf::geist(19.0).color(pf::alpha(SYN_FUNCTION, 0.95)),
        TextAlign::Left,
        appear,
    ));

    // The sprite sheet, its clip cycling at 12 fps.
    let sheet = sprite_sheet();
    let frame_idx = (sec * 12.0).floor() as usize % 16;
    let clip_a = clamp01((sec - 1.2) / 0.6);
    if let Some(tile) =
        vieww_image::sprite::SpriteSheet::grid(sheet.clone(), 4, 4).frame_image(frame_idx)
    {
        let tile_pos = Rect::new(
            pipe_card.left + 30.0,
            pipe_card.top + 62.0,
            pipe_card.left + 230.0,
            pipe_card.top + 262.0,
        );
        stack = stack.push(Positioned::fill().child(Painting::sized(
            CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                if clip_a <= 0.01 {
                    return;
                }
                book.push(Sketch::Layer {
                    alpha: clip_a,
                    blur: 0.0,
                    blend: BlendMode::Normal,
                    clip: None,
                    children: vec![Sketch::Image {
                        rect: tile_pos,
                        image: tile.clone(),
                    }],
                });
                book.stroke_rrect(tile_pos, 12.0, pf::alpha(SYN_COMMENT, 0.4 * clip_a), 1.2);
            }),
        )));
        stack = stack.push(frame::label(
            pipe_card.left + 250.0,
            pipe_card.top + 120.0,
            200.0,
            84.0,
            format!("sprite {}/16\n12 fps · one atlas", frame_idx + 1),
            pf::geist_mono(15.0).color(pf::alpha(MUTED, 0.95)),
            TextAlign::Left,
            clip_a,
        ));
    }

    // The mip ladder — the same sheet, halved and halved again.
    let mips = vieww_image::mipmap::generate_mip_chain(&sheet);
    let mip_a = clamp01((sec - 3.6) / 0.8);
    if mip_a > 0.01 {
        let mut x = pipe_card.left + 30.0;
        let base_y = pipe_card.top + 330.0;
        for (k, m) in mips.iter().take(5).enumerate() {
            let h = 150.0 / (1u32 << k) as f32;
            let w = h;
            let rect = Rect::new(x, base_y + 150.0 - h, x + w, base_y + 150.0);
            let img = m.clone();
            let label = format!("{}", m.width());
            let level_a = clamp01((mip_a - 0.08 * k as f32) / 0.5);
            stack = stack.push(Positioned::fill().child(Painting::sized(
                CANVAS,
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    if level_a <= 0.01 {
                        return;
                    }
                    book.push(Sketch::Layer {
                        alpha: level_a,
                        blur: 0.0,
                        blend: BlendMode::Normal,
                        clip: None,
                        children: vec![Sketch::Image {
                            rect,
                            image: img.clone(),
                        }],
                    });
                    book.stroke_rrect(rect, 4.0, pf::alpha(SYN_COMMENT, 0.35 * level_a), 1.0);
                }),
            )));
            stack = stack.push(frame::label(
                x - 20.0,
                base_y + 158.0,
                w + 40.0,
                20.0,
                label,
                pf::geist_mono(13.0).color(pf::alpha(MUTED, 0.9)),
                TextAlign::Center,
                level_a,
            ));
            x += w + 34.0;
        }
        stack = stack.push(frame::label(
            pipe_card.left + 30.0,
            pipe_card.top + 296.0,
            pipe_card.width() - 60.0,
            24.0,
            "the mip chain — each level a quarter of the bits".to_string(),
            pf::geist(17.0).color(pf::alpha(INK, 0.9)),
            TextAlign::Left,
            mip_a,
        ));
    }

    // The atlas receipt — packed for real.
    let pack_in = clamp01((sec - 6.2) / 0.6);
    if pack_in > 0.01 {
        let packer = vieww_image::atlas::AtlasPacker::new(512, 512);
        let reqs: Vec<(u32, u32)> = (0..16).map(|_| (128, 128)).collect();
        let packed = packer.pack(&reqs);
        let fit_all = packed.placements.iter().all(|p| p.is_some());
        stack = stack.push(frame::label(
            pipe_card.left + 30.0,
            pipe_card.bottom - 96.0,
            pipe_card.width() - 60.0,
            24.0,
            format!(
                "16 tiles → one 512×512 atlas · {}",
                if fit_all {
                    "all packed, 0 rejected"
                } else {
                    "some rejected"
                }
            ),
            pf::geist_mono(15.0).color(pf::alpha(LEDGER, 0.95)),
            TextAlign::Left,
            pack_in,
        ));
    }

    stack = stack.push(frame::caption(
        "Motion from the design team, played natively.",
        1002.0,
        clamp01((sec - 0.2) / 0.5),
    ));
    stack = stack.push(frame::caption(
        "After Effects to vectors — no video, no plugin, no re-implementation.",
        966.0,
        clamp01((sec - 5.2) / 0.6),
    ));
    stack = stack.push(frame::receipts(
        &[
            ("vieww-lottie · 4 layers", ACCENT),
            ("vieww-image · atlas + mips", SYN_TYPE),
            ("vieww-asset · its own codec", LEDGER),
        ],
        0.0,
        0.0,
        clamp01((sec - 7.4) / 0.6),
    ));
    frame::boxed(Rect::new(
        play_card.left,
        play_card.top,
        play_card.right,
        play_card.bottom,
    ));
    stack.into()
}

// ── Z16B · together ─────────────────────────────────────────────────────────

/// One beat of the collaboration script.
enum Beat {
    Type {
        at: f32,
        who: u32,
        pos: usize,
        text: String,
    },
    Del {
        at: f32,
        who: u32,
        pos: usize,
        len: usize,
    },
    Sync {
        at: f32,
    },
}

/// The whole session, as it happened — typed, deleted, synced. The base
/// sentence builds through two syncs; then the concurrent beat: ada
/// appends ", together." while grace turns the comma into a semicolon.
fn script() -> Vec<Beat> {
    vec![
        Beat::Type {
            at: 1.0,
            who: 1,
            pos: 0,
            text: "one ".to_string(),
        },
        Beat::Type {
            at: 1.9,
            who: 1,
            pos: 4,
            text: "design, ".to_string(),
        },
        Beat::Type {
            at: 2.8,
            who: 1,
            pos: 12,
            text: "every ".to_string(),
        },
        Beat::Sync { at: 4.0 },
        Beat::Type {
            at: 4.9,
            who: 2,
            pos: 18,
            text: "screen".to_string(),
        },
        Beat::Sync { at: 7.2 },
        Beat::Type {
            at: 7.9,
            who: 1,
            pos: 24,
            text: ", together.".to_string(),
        },
        Beat::Del {
            at: 8.6,
            who: 2,
            pos: 10,
            len: 1,
        },
        Beat::Type {
            at: 9.2,
            who: 2,
            pos: 10,
            text: ";".to_string(),
        },
        Beat::Sync { at: 10.4 },
    ]
}

/// What the collaboration looks like at `sec`: both documents, the
/// receipts, and each document's own presence cursors.
struct Session {
    a: String,
    b: String,
    ops: usize,
    merges: usize,
    equal: bool,
    carets_a: Vec<(u32, usize)>,
    carets_b: Vec<(u32, usize)>,
}

/// Replay both replicas to `sec` — from zero, every frame.
fn collab_replay(sec: f32) -> Session {
    use vieww_collab::{anchor_at, Doc, Presence};
    let mut a = Doc::new(1);
    let mut b = Doc::new(2);
    let mut ops = 0usize;
    let mut merges = 0usize;
    for beat in script() {
        match beat {
            Beat::Type { at, who, pos, text } if at <= sec => {
                let doc = if who == 1 { &mut a } else { &mut b };
                ops += doc.insert(pos, &text).len();
            }
            Beat::Del { at, who, pos, len } if at <= sec => {
                let doc = if who == 1 { &mut a } else { &mut b };
                ops += doc.delete(pos, len).len();
            }
            Beat::Sync { at } if at <= sec => {
                let (ab, ba) = (a.merge_from(&b), b.merge_from(&a));
                merges += ab.max(ba);
            }
            _ => {}
        }
    }
    let ta = a.text.value();
    let tb = b.text.value();
    let equal = a.snapshot() == b.snapshot();
    // Presence, honestly: each author's caret resolved against each
    // document in turn — the stamp lives wherever the text has it.
    let ada_at = anchor_at(&a.text, ta.len());
    let grace_at = anchor_at(&b.text, if tb.len() > 10 { 10 } else { tb.len() });
    let mut pres_a = Presence::default();
    pres_a.update(1, 1, "ada", ada_at);
    pres_a.update(2, 1, "grace", grace_at);
    let carets_a = pres_a
        .positions(&a.text)
        .into_iter()
        .map(|(r, _, p)| (r, p))
        .collect();
    let mut pres_b = Presence::default();
    pres_b.update(1, 1, "ada", ada_at);
    pres_b.update(2, 1, "grace", grace_at);
    let carets_b = pres_b
        .positions(&b.text)
        .into_iter()
        .map(|(r, _, p)| (r, p))
        .collect();
    Session {
        a: ta,
        b: tb,
        ops,
        merges,
        equal,
        carets_a,
        carets_b,
    }
}

/// Z16B — together: two authors, one truth.
pub(crate) fn together(ctx: &pf::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let s = collab_replay(sec);
    let mut stack = Stack::new();

    frame::ground(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s2: Size| {
            book.rect(
                Rect::new(0.0, 0.0, s2.width, s2.height),
                Color::rgb(7, 6, 10),
            );
            pf::vignette(book, s2.width, s2.height, 0.55);
        }),
    )));

    let appear = ease_out_expo(clamp01(sec / 1.2));

    // The two panels — one document, two views.
    let pa_rect = Rect::new(200.0, 300.0, 900.0, 838.0);
    let pb_rect = Rect::new(1020.0, 300.0, 1720.0, 838.0);
    let merged = sec >= 10.4;
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            if appear <= 0.01 {
                return;
            }
            for (r, col) in [(pa_rect, ACCENT), (pb_rect, TERM_GREEN)] {
                book.rrect(r, 18.0, pf::alpha(SURFACE, 0.9 * appear));
                book.stroke_rrect(
                    r,
                    18.0,
                    pf::alpha(col, if merged { 0.6 } else { 0.35 } * appear),
                    1.6,
                );
                let head = Rect::new(r.left, r.top, r.right, r.top + 46.0);
                book.rrect(head, 14.0, pf::alpha(SURFACE_2, 0.9 * appear));
            }
            // The wire between them — the sync, visible.
            let pts = filmkit::thread_pts(
                Offset::new(pa_rect.right, pa_rect.top + 160.0),
                Offset::new(pb_rect.left, pb_rect.top + 160.0),
                30.0,
            );
            let live = (4.0..4.7).contains(&sec)
                || (7.2..7.9).contains(&sec)
                || (10.4..11.4).contains(&sec);
            let alpha = if live { 0.85 } else { 0.22 };
            filmkit::grow_stroke(
                book,
                &pts,
                1.0,
                pf::alpha(SYN_COMMENT, alpha * appear),
                1.6,
                appear,
            );
            if live {
                // Ops in flight — dots crossing the wire.
                let base = if sec >= 10.4 {
                    10.4
                } else if sec >= 7.2 {
                    7.2
                } else {
                    4.0
                };
                let p = ((sec - base) * 2.2).fract();
                for k in 0..3 {
                    let q = filmkit::point_at(&pts, p - k as f32 * 0.08);
                    book.circle(
                        q,
                        4.0,
                        pf::alpha(if k == 0 { ACCENT } else { SYN_TYPE }, 0.8),
                    );
                }
            }
        }),
    )));

    // The panels' documents, with their presence carets.
    let panel_doc = |rect: Rect, name: &str, text: &str, carets: &[(u32, usize)], col: Color| {
        let mut s = Stack::new();
        s = s.push(frame::label(
            rect.left + 26.0,
            rect.top + 9.0,
            400.0,
            28.0,
            name.to_string(),
            pf::geist(18.0).color(pf::alpha(col, 0.95)),
            TextAlign::Left,
            appear,
        ));
        // The document — mono, so the carets sit exactly.
        let mono = 27.0_f32;
        let chars_per_line = 34usize;
        let mut lines: Vec<String> = Vec::new();
        let mut rest = text.to_string();
        while rest.len() > chars_per_line {
            let head: String = rest.chars().take(chars_per_line).collect();
            let cut = head.rfind(' ').unwrap_or(chars_per_line).max(1);
            let (x, y) = rest.split_at(cut);
            lines.push(x.to_string());
            rest = y.trim_start().to_string();
        }
        lines.push(rest);
        for (i, line) in lines.iter().enumerate() {
            s = s.push(frame::label(
                rect.left + 26.0,
                rect.top + 84.0 + i as f32 * 44.0,
                rect.width() - 52.0,
                36.0,
                line.clone(),
                pf::geist_mono(mono).color(pf::alpha(INK, 0.96)),
                TextAlign::Left,
                appear,
            ));
        }
        // The carets — presence cursors, placed by the char count, in the
        // presence engine's own colours.
        for (who, pos) in carets {
            let mut acc = 0usize;
            let mut caret_line = 0usize;
            let mut caret_col = *pos;
            for (i, l) in lines.iter().enumerate() {
                if *pos <= acc + l.len() {
                    caret_line = i;
                    caret_col = pos.saturating_sub(acc);
                    break;
                }
                acc += l.len() + 1;
            }
            let ccol = if *who == 1 { ACCENT } else { TERM_GREEN };
            let cx = rect.left + 26.0 + caret_col as f32 * mono * 0.62;
            let cy = rect.top + 84.0 + caret_line as f32 * 44.0;
            let blink = ((sec * 1.6).cos() * 0.5 + 0.5).max(0.25);
            s = s.push(Positioned::fill().child(Painting::sized(
                CANVAS,
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    book.rect(
                        Rect::new(cx, cy + 3.0, cx + 3.0, cy + 37.0),
                        pf::alpha(ccol, blink),
                    );
                }),
            )));
        }
        s
    };

    stack = stack.push(panel_doc(
        pa_rect,
        "ada · berlin",
        &s.a,
        &s.carets_a,
        ACCENT,
    ));
    stack = stack.push(panel_doc(
        pb_rect,
        "grace · lagos",
        &s.b,
        &s.carets_b,
        TERM_GREEN,
    ));

    // The sync banner, while a merge flies.
    if (4.0..5.2).contains(&sec) || (7.2..8.2).contains(&sec) || (10.4..11.6).contains(&sec) {
        let label = if sec >= 10.4 {
            "the merge — both ways, no conflicts to resolve"
        } else if sec >= 7.2 {
            "grace → ada · the ops flow"
        } else {
            "ada → grace · the ops flow"
        };
        stack = stack.push(frame::label(
            200.0,
            862.0,
            1520.0,
            30.0,
            label.to_string(),
            pf::geist(20.0).color(pf::alpha(SYN_STRING, 0.95)),
            TextAlign::Center,
            0.95,
        ));
    }

    stack = stack.push(frame::caption(
        "The studio, when the work is shared.",
        1002.0,
        clamp01((sec - 0.2) / 0.5),
    ));
    stack = stack.push(frame::caption(
        "Two authors, one truth — no locks, no merges to resolve by hand.",
        966.0,
        clamp01((sec - 5.4) / 0.6),
    ));
    let ops_line = format!("{} ops · {} applied on merge", s.ops, s.merges);
    let equal_line = format!(
        "snapshots equal: {}",
        if s.equal { "yes" } else { "not yet" }
    );
    stack = stack.push(frame::receipts(
        &[
            ("vieww-collab · RGA text", ACCENT),
            (ops_line.as_str(), SYN_TYPE),
            (equal_line.as_str(), LEDGER),
        ],
        0.0,
        0.0,
        clamp01((sec - 7.4) / 0.6),
    ));
    frame::boxed(Rect::new(
        pa_rect.left,
        pa_rect.top,
        pb_rect.right,
        pb_rect.bottom,
    ));
    stack.into()
}
