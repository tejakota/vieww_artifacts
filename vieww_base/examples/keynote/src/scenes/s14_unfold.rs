//! **S14 · THE UNFOLD** — three trees, and the signal apart from all three.
//!
//! The last twist, and the only one that is purely architectural. The counter
//! the session has been tapping is pulled apart in space into the three trees
//! vieww is actually made of, each one a real layer at a real depth:
//!
//! - **Widget** — what the UI *is*, described fresh every frame. Blue-cyan,
//!   nearest the camera, and the only one that is thrown away and rebuilt.
//! - **Element** — *identity*, which is what survives a rebuild. Green, the
//!   middle plane, with lines tying each node to its widget above and its
//!   render object below.
//! - **Render** — *geometry*: layout in, size up, and the boxes that get
//!   painted. Amber, the far plane, and the only one with real rectangles.
//!
//! Then the move the scene exists for: the **signal** lifts out and floats
//! *beside* all three, wired to the two element nodes that read it. It is not
//! in the widget tree, not in the element tree, not in the render tree. It is
//! the thing the trees are hung from — which is exactly why the counter
//! survived every cut, every error and a change of language.
//!
//! The witness counter, at 7, rides on the signal. The film's own continuity
//! and the framework's own architecture turn out to be the same picture.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook};
use vieww_widget::prelude::*;

use crate::film::Ctx;
use crate::kit::{
    alpha, bump, caption, clamp01, dust, ease_out_cubic, grain, ground, mix, painter, seg,
    smoothstep, spring, vignette, Type, AMBER, CYAN, CYAN_SOFT, H, INK, INK_SOFT, MINT, MUTED,
    VIOLET, VIOLET_SOFT, W,
};
use crate::solid::{v3, Cam, V3};
use crate::studio::compose;

/// One node of the counter's tree, in all three layers at once: its depth in
/// the tree, its lateral slot, and the name each layer gives it.
struct Node {
    depth: usize,
    slot: f32,
    widget: &'static str,
    element: &'static str,
    render: &'static str,
    /// Does this node read the signal?
    reads: bool,
}

const NODES: &[Node] = &[
    Node { depth: 0, slot: 0.0,  widget: "Column",  element: "ColumnElement", render: "RenderFlex",  reads: false },
    Node { depth: 1, slot: -1.5, widget: "Dial",    element: "DialElement",   render: "RenderArc",   reads: true },
    Node { depth: 1, slot: -0.5, widget: "Text",    element: "TextElement",   render: "RenderText",  reads: true },
    Node { depth: 1, slot: 0.5,  widget: "Text",    element: "TextElement",   render: "RenderText",  reads: false },
    Node { depth: 1, slot: 1.5,  widget: "Button",  element: "ButtonElement", render: "RenderBox",   reads: false },
];

/// The beats, in scene-seconds.
const SPLIT_T: f32 = 1.10;
const SPLIT_SPAN: f32 = 2.80;
const LABEL_T: f32 = 4.40;
const SIGNAL_T: f32 = 8.20;
const CLAIM_T: f32 = 11.60;

/// The three planes' depths, once split.
const Z_WIDGET: f32 = -1.7;
const Z_ELEMENT: f32 = 0.0;
const Z_RENDER: f32 = 1.7;

pub fn frame(ctx: &Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let frame_i = ctx.frame;

    // The split: the three planes separate from one.
    let split = ease_out_cubic(seg(sec, SPLIT_T, SPLIT_T + SPLIT_SPAN));
    // The signal lifts out to the side.
    let lift = spring(seg(sec, SIGNAL_T, SIGNAL_T + 1.6), 10.0, 0.44).clamp(0.0, 1.2);
    let labels = smoothstep(seg(sec, LABEL_T, LABEL_T + 0.9));
    let claim = smoothstep(seg(sec, CLAIM_T, CLAIM_T + 1.0)) * (1.0 - smoothstep(seg(sec, 16.6, 17.6)));

    // **An orbit, not a pan.** Yawing a camera that sits on the axis swings
    // the whole scene sideways, and the further a plane is the further it
    // swings — which is how the three trees ended up off the right edge. The
    // camera is placed on a circle around the origin instead, so the tree's
    // root projects to the centre of frame at any angle.
    let theta = -0.30 * split - 0.04;
    let radius = 11.6f32;
    let cam = Cam {
        eye: v3(-radius * theta.sin(), 0.05, -radius * theta.cos()),
        yaw: theta,
        pitch: -0.05,
        focal: 1220.0,
    };
    let canvas = Size::new(W, H);

    // Where a node sits in a given plane. A free function of the split, so
    // the painter can own its copy rather than borrow this frame's.
    let pos = move |n: &Node, z: f32| -> V3 {
        v3(n.slot * 1.58, 1.30 - n.depth as f32 * 1.75, z * split)
    };
    // The signal's own place, off to the side and slightly forward.
    let sig = v3(4.1 + 0.7 * (1.0 - lift.min(1.0)), 0.55, -1.0 * split);

    let bg = painter(move |book: &mut Sketchbook, size: Size| {
        ground(book, size, sec, 1.0);
        dust(book, size, sec, 40, CYAN_SOFT, 0.55);

        // The three planes, drawn back to front so the near one occludes.
        let planes: [(f32, Color, f32); 3] = [
            (Z_RENDER, AMBER, 0.07),
            (Z_ELEMENT, MINT, 0.08),
            (Z_WIDGET, CYAN, 0.09),
        ];
        for (z, c, a) in planes {
            if split < 0.02 && z != Z_ELEMENT {
                continue;
            }
            // The plane's own card: a faint rectangle in space, so each tree
            // is visibly *on* something.
            let sheet = crate::solid::slab(v3(0.0, -0.20, z * split), 6.6, 4.0, 1, 1, alpha(c, 0.04));
            crate::solid::draw(
                book,
                &sheet,
                &cam,
                canvas,
                &crate::solid::Style {
                    ambient: 1.0,
                    cull: false,
                    alpha: a * split,
                    wire: Some((alpha(c, 0.30), 1.0)),
                    ..crate::solid::Style::default()
                },
            );
        }

        // The edges of each tree: parent to child, in the plane's colour.
        for (z, c) in [(Z_RENDER, AMBER), (Z_ELEMENT, MINT), (Z_WIDGET, CYAN)] {
            let root = pos(&NODES[0], z);
            for n in &NODES[1..] {
                crate::solid::line3(book, &[root, pos(n, z)], &cam, canvas, alpha(c, 0.42), 1.4);
            }
        }

        // The correspondence lines: the same node, tied through all three
        // layers. This is the picture that makes "three trees, one UI" a
        // fact rather than a slogan.
        if split > 0.05 {
            for n in NODES {
                crate::solid::line3(
                    book,
                    &[pos(n, Z_WIDGET), pos(n, Z_ELEMENT), pos(n, Z_RENDER)],
                    &cam,
                    canvas,
                    alpha(INK_SOFT, 0.16 * split),
                    1.0,
                );
            }
        }

        // The nodes. Widget nodes are diamonds, element nodes are circles,
        // render nodes are boxes — three shapes, because they are three
        // different kinds of thing.
        for n in NODES {
            // Render: a real rectangle, because that is what it holds.
            if let Some((p, _, k)) = cam.project(pos(n, Z_RENDER), canvas) {
                let w = 46.0 * cam.px(k);
                let h = 26.0 * cam.px(k);
                book.stroke_rrect(
                    Rect::new(p.dx - w, p.dy - h, p.dx + w, p.dy + h),
                    3.0,
                    alpha(AMBER, 0.85),
                    1.6,
                );
            }
            // Element: a ring — identity, which persists.
            if let Some((p, _, k)) = cam.project(pos(n, Z_ELEMENT), canvas) {
                book.ring(p, 13.0 * cam.px(k), 2.0, alpha(MINT, 0.9));
            }
            // Widget: a diamond — a description, thrown away every frame.
            if let Some((p, _, k)) = cam.project(pos(n, Z_WIDGET), canvas) {
                let r = 13.0 * cam.px(k);
                let mut path = Path::new();
                path.move_to(Offset::new(p.dx, p.dy - r));
                path.line_to(Offset::new(p.dx + r, p.dy));
                path.line_to(Offset::new(p.dx, p.dy + r));
                path.line_to(Offset::new(p.dx - r, p.dy));
                path.close();
                book.stroke(path, alpha(CYAN, 0.9), 1.8);
            }
        }

        // **The signal.** Outside all three planes, wired to the two element
        // nodes that read it. The wires pulse, because a signal that nothing
        // reads is just a variable.
        if lift > 0.02 {
            let a = lift.min(1.0);
            for n in NODES.iter().filter(|n| n.reads) {
                let to = pos(n, Z_ELEMENT);
                crate::solid::line3(book, &[sig, to], &cam, canvas, alpha(VIOLET, 0.5 * a), 1.6);
                // The pulse travelling down the wire — one dot per wire, on
                // the scene's own clock.
                let u = ((sec - SIGNAL_T) * 0.55).fract();
                let p = V3 {
                    x: sig.x + (to.x - sig.x) * u,
                    y: sig.y + (to.y - sig.y) * u,
                    z: sig.z + (to.z - sig.z) * u,
                };
                crate::solid::dot3(book, p, &cam, canvas, 5.0, alpha(VIOLET_SOFT, 0.9 * a));
            }
            if let Some((p, _, k)) = cam.project(sig, canvas) {
                let r = 34.0 * cam.px(k);
                book.blended_layer(1.0, r * 0.7, BlendMode::Plus, None, |g| {
                    g.circle(p, r * 1.5, alpha(VIOLET, 0.24 * a));
                });
                book.ring(p, r, 2.4, alpha(VIOLET_SOFT, 0.95 * a));
                book.circle(p, r * 0.34, alpha(VIOLET, 0.85 * a));
            }
        }

        // The claim's light.
        let flash = bump(seg(sec, CLAIM_T, CLAIM_T + 1.4), 0.0, 1.0);
        if flash > 0.01 {
            book.rect(
                Rect::new(0.0, 0.0, size.width, size.height),
                alpha(mix(VIOLET, Color::WHITE, 0.4), 0.03 * flash),
            );
        }

        grain(book, size, frame_i, 0.012, 300);
        vignette(book, size, 1.0);
    });

    let mut nodes: Vec<WidgetNode> = Vec::new();

    // The node names, one per layer, at the node's own projected position.
    if labels > 0.02 {
        for n in NODES {
            let rows: [(f32, &str, Color); 3] = [
                (Z_WIDGET, n.widget, CYAN),
                (Z_ELEMENT, n.element, MINT),
                (Z_RENDER, n.render, AMBER),
            ];
            for (z, name, c) in rows {
                let Some((p, _, k)) = cam.project(pos(n, z), canvas) else {
                    continue;
                };
                let size = (11.0 * cam.px(k)).clamp(7.5, 13.0);
                nodes.push(
                    Type::new(name)
                        .mono()
                        .size(size)
                        .color(alpha(c, 0.92 * labels))
                        .center()
                        .at(p.dx - 90.0, p.dy + 16.0 * cam.px(k))
                        .width(180.0)
                        .into(),
                );
            }
        }
    }

    // The three planes' names, and what each one is *for*. The film's whole
    // explanation of the architecture is these three lines.
    if split > 0.3 {
        let a = smoothstep(seg(split, 0.3, 0.9)) * (1.0 - claim * 0.35);
        let rows: [(&str, &str, Color); 3] = [
            ("widget", "what the UI is · rebuilt every frame", CYAN),
            ("element", "identity · what survives a rebuild", MINT),
            ("render", "geometry · constraints down, sizes up", AMBER),
        ];
        for (i, (name, what, c)) in rows.into_iter().enumerate() {
            let y = 128.0 + i as f32 * 72.0;
            nodes.push(
                Type::new(name)
                    .size(26.0)
                    .medium()
                    .track(0.6)
                    .color(alpha(c, 0.95 * a))
                    .at(112.0, y)
                    .width(320.0)
                    .into(),
            );
            nodes.push(
                Type::new(what)
                    .mono()
                    .size(12.5)
                    .track(1.4)
                    .color(alpha(MUTED, 0.9 * a))
                    .at(114.0, y + 34.0)
                    .width(460.0)
                    .into(),
            );
        }
    }

    // The signal's own label, and the counter riding on it.
    if lift > 0.1 {
        let a = lift.min(1.0);
        if let Some((p, _, _)) = cam.project(sig, canvas) {
            nodes.push(
                Type::new(format!("{}", ctx.spine.witness))
                    .size(34.0)
                    .bold()
                    .color(alpha(INK, 0.97 * a))
                    .center()
                    .at(p.dx - 100.0, p.dy - 22.0)
                    .width(200.0)
                    .into(),
            );
            nodes.push(
                Type::new("signal")
                    .mono()
                    .size(13.0)
                    .track(3.0)
                    .color(alpha(VIOLET_SOFT, 0.95 * a))
                    .center()
                    .at(p.dx - 140.0, p.dy + 52.0)
                    .width(280.0)
                    .into(),
            );
            nodes.push(
                Type::new("outside all three trees")
                    .mono()
                    .size(11.5)
                    .track(1.4)
                    .color(alpha(MUTED, 0.9 * a))
                    .center()
                    .at(p.dx - 180.0, p.dy + 76.0)
                    .width(360.0)
                    .into(),
            );
        }
    }

    if claim > 0.004 {
        nodes.push(
            Type::new("the counter never restarted because it was never in the tree")
                .size(36.0)
                .light()
                .color(alpha(INK, 0.97 * claim))
                .center()
                .banner(H * 0.845)
                .into(),
        );
        nodes.push(
            Type::new("reading a signal during build is the subscription · rebuild is pull-based and coalesced")
                .mono()
                .size(14.0)
                .track(2.0)
                .color(alpha(VIOLET_SOFT, 0.9 * claim))
                .center()
                .banner(H * 0.845 + 52.0)
                .into(),
        );
    }

    nodes.push(caption(
        "widget · element · render — and the signal apart from all three",
        smoothstep(seg(sec, 5.4, 6.4)) * (1.0 - smoothstep(seg(sec, 16.8, 17.7))),
    ));
    let _ = (clamp01, VIOLET, W);
    compose(bg, nodes)
}
