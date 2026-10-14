//! **S11 · THE MIRROR** — 転. The inspector, pointed at the inspector.
//!
//! Act III's first twist, and the one the whole session has been quietly
//! setting up. The devtools that just lit up a damage rect inside the preview
//! are turned around: the tree in the panel is no longer the counter's, it is
//! **the studio's own** — `Shell · Body · Workspace · MainColumn · EditorGroup`
//! — and the damage rectangles now land on the IDE's own chrome. The caret
//! blinks, and one small rectangle around the caret lights up. The tab strip
//! is hovered, and one small rectangle around the tab lights up.
//!
//! The sentence is four words long and it recontextualises the previous two
//! minutes: **viewwstudio is a vieww app.** Everything the film has shown the
//! product doing, the product has been doing to itself the entire time — the
//! chrome is a widget tree, the panels are elements, the repaints are damage
//! rects, and the 60 fps in the status bar was always its own.
//!
//! The scene ends by pulling the camera back through the window, which is the
//! move S13 completes.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Rect, Size, Sketchbook};
use vieww_widget::prelude::*;

use crate::film::Ctx;
use crate::kit::{
    act_card, alpha, bump, caption, clamp01, ease_out_cubic, grain, ground, painter, seg,
    smoothstep, vignette, xywh, Type, CYAN, CYAN_SOFT, H, INK, INK_SOFT, MINT, MUTED, RED,
    VIOLET_SOFT, W,
};
use crate::studio::{
    self, compose, project_tree, status_line, App, Inspector, Layout, Shell,
};

/// The beats, in scene-seconds.
const TURN_T: f32 = 1.80;
const CLAIM_T: f32 = 8.20;
const PULL_T: f32 = 12.60;

pub fn frame(ctx: &Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let frame_i = ctx.frame;

    let turn = smoothstep(seg(sec, TURN_T, TURN_T + 0.9));
    let claim = smoothstep(seg(sec, CLAIM_T, CLAIM_T + 0.9)) * (1.0 - smoothstep(seg(sec, PULL_T + 1.4, PULL_T + 2.2)));
    let pull = ease_out_cubic(seg(sec, PULL_T, PULL_T + 3.0));

    let l = Layout::new(64.0, 0.36, true);

    // The studio's own element tree — the names are the product's modules.
    let tree = vec![
        (0, "Shell".to_string(), false),
        (1, "Theme".to_string(), false),
        (2, "Body".to_string(), false),
        (3, "TitleBar".to_string(), false),
        (3, "Workspace".to_string(), false),
        (4, "ActivityBar".to_string(), false),
        (4, "Sidebar".to_string(), false),
        (4, "MainColumn".to_string(), true),
        (5, "EditorGroup".to_string(), false),
    ];

    // The damage rectangles the studio is producing about itself. They are in
    // *window* coordinates here, not the preview's — which is the whole
    // difference between S10 and this scene.
    let mut damage: Vec<(Rect, f32)> = Vec::new();
    let unit = |r: Rect| -> Rect {
        Rect::new(
            (r.left - l.window.left) / l.window.width(),
            (r.top - l.window.top) / l.window.height(),
            (r.right - l.window.left) / l.window.width(),
            (r.bottom - l.window.top) / l.window.height(),
        )
    };
    // The caret's own repaint — a two-character box that blinks at the
    // caret's rate, because a blinking caret *is* a repaint.
    if turn > 0.4 {
        let cl = 3usize;
        let cy = l.line_y(cl);
        let blink = if crate::kit::caret_on(ctx.abs, false) { 1.0 } else { 0.25 };
        damage.push((
            unit(xywh(l.code.left + 190.0, cy - 2.0, 26.0, studio::CODE_LEAD)),
            turn * blink,
        ));
    }
    // The status bar's clock — one region, once a second.
    if turn > 0.5 {
        let tick = bump((ctx.abs).fract(), 0.0, 0.35);
        if tick > 0.02 {
            damage.push((unit(xywh(l.status.right - 150.0, l.status.top + 3.0, 132.0, 18.0)), turn * tick));
        }
    }
    // The tab strip, as the pointer crosses it.
    if sec > 4.4 && sec < 7.2 {
        let p = seg(sec, 4.4, 7.2);
        let x = l.tabs.left + 6.0 + p * 220.0;
        damage.push((unit(xywh(x, l.tabs.top + 3.0, 132.0, studio::TAB_STRIP - 3.0)), 0.9));
    }
    // The activity rail's selected entry, when the view changes.
    if sec > 2.6 && sec < 3.6 {
        damage.push((unit(xywh(l.rail.left + 4.0, l.rail.top + 26.0 + 4.0 * 36.0 - 14.0, 40.0, 28.0)), 0.9));
    }

    let inspector = Inspector {
        open: turn,
        tree,
        facts: vec![
            ("target".into(), "viewwstudio".into()),
            ("widgets".into(), "the chrome you are looking at".into()),
            ("renderer".into(), "vieww · native".into()),
            (
                "frame".into(),
                if ctx.probe.is_measured() {
                    format!("{:.2} ms", ctx.probe.frame_ms)
                } else {
                    "—".into()
                },
            ),
        ],
        // The overlay is drawn over the *window* by this scene, not by the
        // shell's own preview-body overlay — so the inspector carries none.
        damage: Vec::new(),
        selection: None,
    };

    let sh = Shell {
        files: project_tree("main.rs"),
        tabs: vec![
            ("main.rs".into(), studio::FileKind::Rust, true, false),
            ("shell.rs".into(), studio::FileKind::Rust, false, false),
        ],
        crumbs: vec!["viewwstudio".into(), "ui".into(), "mod.rs".into(), "Shell".into()],
        code: studio::lex_all(SHELL_SRC, true),
        caret: Some(3),
        caret_on: crate::kit::caret_on(ctx.abs, false),
        status: status_line(
            "desktop · native",
            "60 fps",
            &[if turn > 0.5 { ("inspecting: viewwstudio", CYAN) } else { ("inspecting: preview", MUTED) }],
        ),
        preview_label: "Preview · self".into(),
        preview_chip: "mirror".into(),
        panel: studio::Panel {
            active: 1,
            lines: vec![
                ("  inspector  target switched → the studio itself".to_string(), CYAN),
                ("  tree       Shell · Body · Workspace · MainColumn".to_string(), MUTED),
                ("  damage     the chrome's own rects, this frame".to_string(), MUTED),
            ],
            ..studio::Panel::default()
        },
        inspector: Some(inspector),
        witness: Some(ctx.spine.witness),
        elapsed: Some(crate::kit::clock_mmss(ctx.spine.elapsed)),
        view: 4,
        ..Shell::default()
    };
    let shell_paint = sh.clone();

    let app = App {
        value: ctx.spine.witness,
        alive: 1.0 - turn * 0.55,
        spring: 0.5 + 0.35 * (ctx.abs * 1.1).sin(),
        ..App::default()
    };
    let app_paint = App { ..app };
    let card = Rect::new(
        l.preview_body.left + 30.0,
        l.preview_body.top + 22.0,
        l.preview_body.right - 30.0,
        l.preview_body.bottom - 30.0,
    );

    let dmg = damage.clone();
    let bg = painter(move |book: &mut Sketchbook, size: Size| {
        ground(book, size, sec, 0.9);

        // The pull-back: the whole studio recedes, which is the camera
        // beginning the move S13 finishes.
        let k = 1.0 - 0.14 * pull;
        book.transformed(
            vieww_foundation::Transform::scale_around(
                Offset::new(size.width * 0.5, size.height * 0.46),
                k,
                k,
            ),
            |g| {
                studio::back(g, &l, &shell_paint);
                g.rrect(card, 8.0, alpha(Color::rgb(0x16, 0x16, 0x18), app_paint.alive));
                studio::app(g, card, &app_paint);
                studio::front(g, &l, &shell_paint);

                // **The mirror's own overlay.** The damage rectangles are
                // drawn over the window, in the window's coordinates —
                // because the thing being inspected is the window.
                for (r, strength) in &dmg {
                    let rr = Rect::new(
                        l.window.left + r.left * l.window.width(),
                        l.window.top + r.top * l.window.height(),
                        l.window.left + r.right * l.window.width(),
                        l.window.top + r.bottom * l.window.height(),
                    );
                    g.stroke_rrect(rr, 3.0, alpha(RED, 0.92 * strength), 2.0);
                    g.rrect(rr, 3.0, alpha(RED, 0.10 * strength));
                }

                // The element boxes: every region of the shell outlined at
                // once, faint, the way a devtools tree-hover looks.
                if turn > 0.3 {
                    let a = 0.22 * turn * (1.0 - claim * 0.4);
                    for r in [l.titlebar, l.rail, l.sidebar, l.editor, l.preview, l.panel, l.status] {
                        g.stroke_rrect(r, studio::CARD_CORNER, alpha(CYAN_SOFT, a), 1.0);
                    }
                    // The selected node — MainColumn — gets the brackets.
                    let main = Rect::new(l.editor.left, l.editor.top, l.preview.right, l.panel.bottom);
                    crate::kit::brackets(g, main, 22.0, alpha(CYAN_SOFT, 0.85 * turn), 2.0);
                }
            },
        );

        // The claim's own light: as the sentence lands, the window is lit
        // from the cyan side — the framework's colour, arriving under the
        // product's.
        if claim > 0.01 {
            book.blended_layer(1.0, 90.0, BlendMode::Plus, None, |g| {
                g.rect(
                    xywh(0.0, size.height * 0.5, size.width, size.height * 0.5),
                    Gradient::vertical().with_dither().with_stops(&[
                        (0.0, alpha(CYAN, 0.0)),
                        (1.0, alpha(CYAN, 0.055 * claim)),
                    ]),
                );
            });
        }

        grain(book, size, frame_i, 0.012, 300);
        vignette(book, size, 0.9);
    });

    // The chrome's glyphs travel with the pull-back.
    let k = 1.0 - 0.14 * pull;
    let mut chrome_text = Stack::new();
    for n in studio::text(&l, &sh) {
        chrome_text = chrome_text.push(Positioned::fill().child(n));
    }
    for n in studio::app_text(card, &app) {
        chrome_text = chrome_text.push(Positioned::fill().child(n));
    }
    let mut nodes: Vec<WidgetNode> = vec![vieww_widget::Transformed::new(
        vieww_foundation::Transform::scale_around(Offset::new(W * 0.5, H * 0.46), k, k),
    )
    .child(chrome_text)
    .into()];

    // The four words.
    if claim > 0.004 {
        nodes.push(
            Type::new("viewwstudio is a vieww app")
                .size(52.0)
                .light()
                .track(0.6)
                .color(alpha(INK, 0.98 * claim))
                .center()
                .banner(H * 0.42)
                .into(),
        );
        nodes.push(
            Type::new("the chrome is a widget tree · the panels are elements · the repaints are damage rects")
                .mono()
                .size(15.0)
                .track(2.0)
                .color(alpha(CYAN, 0.9 * claim))
                .center()
                .banner(H * 0.42 + 76.0)
                .into(),
        );
        // The scrim the sentence needs, painted over the window only.
        nodes.insert(
            1,
            crate::kit::painter(move |book: &mut Sketchbook, _s: Size| {
                book.rrect(l.window, studio::CARD_CORNER + 4.0, alpha(Color::rgb(6, 5, 5), 0.74 * claim));
            }),
        );
        // Re-issue the words above their own scrim.
        nodes.push(
            Type::new("viewwstudio is a vieww app")
                .size(52.0)
                .light()
                .track(0.6)
                .color(alpha(INK, 0.98 * claim))
                .center()
                .banner(H * 0.42)
                .into(),
        );
        nodes.push(
            Type::new("the chrome is a widget tree · the panels are elements · the repaints are damage rects")
                .mono()
                .size(15.0)
                .track(2.0)
                .color(alpha(CYAN, 0.9 * claim))
                .center()
                .banner(H * 0.42 + 76.0)
                .into(),
        );
    }

    nodes.push(act_card("ACT III", "THE TWISTS", "転", smoothstep(seg(sec, 0.4, 1.4)) * (1.0 - smoothstep(seg(sec, 5.6, 6.6)))));
    nodes.push(caption(
        if turn > 0.5 { "the inspector, pointed at the inspector" } else { "overlays still on" },
        smoothstep(seg(sec, 0.6, 1.4)) * (1.0 - smoothstep(seg(sec, 15.0, 15.9))),
    ));
    let _ = (INK_SOFT, MINT, MUTED, VIOLET_SOFT, clamp01);
    compose(bg, nodes)
}

/// The shell's own source, shown in the editor while the inspector points at
/// it — the studio reading the file that draws the studio.
const SHELL_SRC: &[&str] = &[
    "impl Widget for Shell {",
    "    fn build(&self, ctx: &BuildContext) -> WidgetNode {",
    "        let (theme, chrome) = self.studio.themed();",
    "        Theme::new(theme).child(Inherited::new(",
    "            chrome,",
    "            Overlay::new().child(Stack::new().children(children![",
    "                Body { studio: self.studio.clone() },",
    "                Palette { studio: self.studio.clone() },",
    "            ])),",
    "        ))",
    "    }",
    "}",
];
