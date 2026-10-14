//! A Konva-style retained 2D stage and a D3-style data-visualisation
//! toolkit, photographed.
//!
//! 1. `vieww-canvas`: layers, groups and shapes; a scripted pointer drags
//!    the star (`pointer_down/move/up` → DragStart/Move/End events that
//!    bubble), hover hit-testing highlights whatever is under the cursor, a
//!    `Transformer` rotates and resizes the card by its anchors, and the
//!    stage is round-tripped through JSON.
//! 2. `vieww-dataviz` for the rest: band + linear scales with nice ticks
//!    and keyed enter/update/exit transitions (`Marks`); curves and a
//!    stacked area; a squarified treemap; a tidy tree; a Barnes–Hut force
//!    layout; marching-squares contours coloured with viridis; pie arcs.

use std::cell::Cell;
use std::f32::consts::TAU;
use std::rc::Rc;

use feature_harness::draw::{grid, page, painted, xywh, DIM, HUES, INK};
use vieww_canvas::{Anchor, Attrs, EventKind, Shape, Stage, Transformer};
use vieww_dataviz::color::viridis;
use vieww_dataviz::contour::isoline_path;
use vieww_dataviz::force::Simulation;
use vieww_dataviz::hierarchy::{tree, treemap, Hierarchy};
use vieww_dataviz::join::Marks;
use vieww_dataviz::scale::{Band, Continuous};
use vieww_dataviz::shape::{arc, area, line, pie, stack, Curve, StackOffset};
use vieww_foundation::{Color, Offset, Size};

const SPAN: f32 = 4.0;
const PW: f32 = 240.0;
const PH: f32 = 170.0;

fn o(x: f32, y: f32) -> Offset {
    Offset::new(x, y)
}

fn stage_at(t: f32) -> (Stage, Option<Transformer>, usize, usize, Offset) {
    let mut s = Stage::new(PW, PH);
    let layer = s.add_layer();
    let group = s.add_group(
        layer,
        Attrs {
            x: 10.0,
            y: 10.0,
            ..Attrs::default()
        },
    );
    s.add_shape(
        group,
        Attrs {
            fill: Some(Color::rgb(40, 46, 62)),
            ..Attrs::default()
        },
        Shape::Rect {
            width: 220.0,
            height: 150.0,
            corner: 8.0,
        },
    );
    let card = s.add_shape(
        layer,
        Attrs {
            x: 70.0,
            y: 90.0,
            offset: o(35.0, 22.0),
            fill: Some(HUES[0]),
            stroke: Some(INK),
            stroke_width: 1.5,
            name: "card".into(),
            ..Attrs::default()
        },
        Shape::Rect {
            width: 70.0,
            height: 44.0,
            corner: 6.0,
        },
    );
    let star = s.add_shape(
        layer,
        Attrs {
            x: 60.0,
            y: 45.0,
            fill: Some(HUES[3]),
            draggable: true,
            name: "star".into(),
            ..Attrs::default()
        },
        Shape::Star {
            points: 5,
            inner: 9.0,
            outer: 20.0,
        },
    );
    s.add_shape(
        layer,
        Attrs {
            x: 190.0,
            y: 120.0,
            fill: Some(HUES[2]),
            name: "hex".into(),
            ..Attrs::default()
        },
        Shape::RegularPolygon {
            sides: 6,
            radius: 18.0,
        },
    );
    s.add_shape(
        layer,
        Attrs {
            stroke: Some(HUES[1]),
            stroke_width: 2.5,
            ..Attrs::default()
        },
        Shape::Arrow {
            points: vec![o(20.0, 150.0), o(80.0, 130.0), o(120.0, 150.0)],
            head: 8.0,
        },
    );
    let drags = Rc::new(Cell::new(0));
    let d2 = drags.clone();
    s.on(layer, EventKind::DragMove, move |_| d2.set(d2.get() + 1));
    // Scripted pointer: drag the star along an arc, then hover the hex.
    let mut cursor = o(60.0, 45.0);
    let steps = (t * 30.0) as usize;
    let mut tr = Transformer::new(card);
    tr.rotation_snaps = vec![0.0, 45.0, 90.0];
    for k in 0..=steps {
        let tt = k as f32 / 30.0;
        if tt < 0.3 {
            if k == 0 {
                s.pointer_down(cursor);
            }
        } else if tt < 1.8 {
            let u = (tt - 0.3) / 1.5;
            cursor = o(60.0 + 120.0 * u, 45.0 + 25.0 * (u * TAU / 2.0).sin());
            s.pointer_move(cursor);
        } else if tt < 1.9 {
            s.pointer_up(cursor);
        } else if tt < 3.2 {
            // Transformer: grab the rotater and swing it.
            let anchors = tr.anchors(&s);
            if let Some((_, p)) = anchors.iter().find(|(a, _)| *a == Anchor::Rotater) {
                if tr.anchor_at(&s, *p).is_some() && k % 30 == 0 {
                    tr.begin(&s, *p);
                }
            }
            let u = (tt - 1.9) / 1.3;
            let c = s.client_rect(card);
            let centre = o((c.left + c.right) / 2.0, (c.top + c.bottom) / 2.0);
            let a = -TAU / 4.0 + u * 1.2;
            cursor = centre + o(a.cos() * 50.0, a.sin() * 50.0);
            tr.drag(&mut s, cursor);
        } else {
            tr.end();
            cursor = o(190.0, 120.0);
            s.pointer_move(cursor);
        }
    }
    let json = s.to_json().to_string().len();
    let _ = star;
    let n = drags.get();
    (s, Some(tr), n, json, cursor)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    feature_harness::launch("87 — canvas & dataviz", Size::new(1130.0, 530.0), |d| {
        let view = feature_harness::clocked(d, SPAN, |t| {
            let (stage, tr, drags, json, cursor) = stage_at(t);
            let hover = stage
                .hit(cursor)
                .map(|id| stage.node(id).attrs.name.clone())
                .unwrap_or_default();
            let p1 = painted(
                "canvas stage (Konva)",
                &format!("drag events {drags} · hit '{hover}' · json {json} B"),
                Size::new(PW, PH),
                move |g, _| {
                    let (book, _) = stage.render();
                    for it in book.items() {
                        g.push(it.clone());
                    }
                    if let Some(tr) = &tr {
                        for it in tr.draw(&stage).items() {
                            g.push(it.clone());
                        }
                    }
                    g.circle(cursor, 4.0, INK);
                },
            );

            // Bars with keyed transitions between datasets.
            let sets: [&[(&str, f32)]; 3] = [
                &[("a", 3.0), ("b", 7.0), ("c", 5.0), ("d", 2.0)],
                &[("b", 9.0), ("c", 4.0), ("d", 6.0), ("e", 8.0)],
                &[("a", 5.0), ("c", 7.0), ("e", 3.0)],
            ];
            let mut marks: Marks<String, f32> = Marks::new(0.6);
            let phase = (t / SPAN * 3.0) as usize;
            for (i, set) in sets.iter().enumerate().take(phase + 1) {
                let data: Vec<(String, f32)> =
                    set.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect();
                marks.data(&data, |_| 0.0, |_| 0.0);
                let dt = if i == phase {
                    t - i as f32 * SPAN / 3.0
                } else {
                    SPAN / 3.0
                };
                let mut left = dt;
                while left > 0.0 {
                    marks.advance(1.0 / 60.0);
                    left -= 1.0 / 60.0;
                }
            }
            let cur = marks.current();
            let p2 = painted(
                "scales + join (D3)",
                &format!(
                    "band × linear · nice ticks · {} marks (enter/update/exit)",
                    cur.len()
                ),
                Size::new(PW, PH),
                move |g, _| {
                    let keys: Vec<String> = cur.iter().map(|m| m.0.clone()).collect();
                    let x = Band::new(keys.clone(), (30.0, 230.0));
                    let y = Continuous::linear((0.0, 10.0), (150.0, 10.0)).nice(5);
                    for tk in y.ticks(5) {
                        let yy = y.map(tk) as f32;
                        g.line(o(28.0, yy), o(232.0, yy), DIM.with_alpha(60), 1.0);
                    }
                    for (i, (k, v, _)) in cur.iter().enumerate() {
                        let x0 = x.map(k).unwrap_or(0.0) as f32;
                        let top = y.map(f64::from(*v)) as f32;
                        g.rrect(
                            xywh(x0, top, x.bandwidth() as f32, 150.0 - top),
                            3.0,
                            HUES[i % 6],
                        );
                    }
                },
            );

            let series: Vec<Vec<f32>> = (0..3)
                .map(|s| {
                    (0..12)
                        .map(|i| {
                            1.0 + ((i as f32 * 0.6 + s as f32 * 2.0 + t).sin() + 1.0)
                                * (1.0 + s as f32 * 0.4)
                        })
                        .collect()
                })
                .collect();
            let p3 = painted(
                "shapes · curves + stack",
                "monotoneX · cardinal · basis · stacked area",
                Size::new(PW, PH),
                move |g, _| {
                    let st = stack(&series, StackOffset::None);
                    let xs = |i: usize| 10.0 + i as f32 * 20.0;
                    let ys = |v: f32| 160.0 - v * 7.0;
                    for (si, layer) in st.iter().enumerate() {
                        let top: Vec<Offset> = layer
                            .iter()
                            .enumerate()
                            .map(|(i, (_, hi))| o(xs(i), ys(*hi)))
                            .collect();
                        let base: Vec<Offset> = layer
                            .iter()
                            .enumerate()
                            .map(|(i, (lo, _))| o(xs(i), ys(*lo)))
                            .collect();
                        g.fill(
                            area(&top, &base, Curve::MonotoneX),
                            HUES[si].with_alpha(150),
                        );
                    }
                    let pts: Vec<Offset> = series[0]
                        .iter()
                        .enumerate()
                        .map(|(i, v)| o(xs(i), 95.0 - v * 14.0))
                        .collect();
                    for (c, col) in [
                        (Curve::Cardinal(0.0), INK),
                        (Curve::Basis, HUES[4]),
                        (Curve::Step, HUES[5]),
                    ] {
                        g.stroke(line(&pts, c), col, 1.5);
                    }
                },
            );

            let rows: Vec<(String, Option<String>, f64)> = {
                let mut r = vec![("root".to_owned(), None, 0.0)];
                for (i, g) in ["ui", "gfx", "anim", "io"].iter().enumerate() {
                    r.push(((*g).to_owned(), Some("root".to_owned()), 0.0));
                    for k in 0..(3 + i) {
                        let w = 1.0
                            + ((k * 7 + i * 3) % 5) as f64
                            + (t as f64 * 0.8 + k as f64).sin().abs() * 3.0;
                        r.push((format!("{g}{k}"), Some((*g).to_owned()), w));
                    }
                }
                r
            };
            let refs: Vec<(&str, Option<&str>, f64)> = rows
                .iter()
                .map(|(a, b, c)| (a.as_str(), b.as_deref(), *c))
                .collect();
            let mut h = Hierarchy::stratify(&refs).expect("valid tree");
            h.sum();
            h.sort_by_value();
            let h2 = h.clone();
            let p4 = painted(
                "treemap",
                "stratify → sum → squarify",
                Size::new(PW, PH),
                move |g, _| {
                    let rects = treemap(&h, xywh(0.0, 0.0, PW, PH), 2.0);
                    for (i, r) in rects.iter().enumerate() {
                        let n = &h.nodes[i];
                        if n.children.is_empty() {
                            let top = n.parent.map_or(0, |p| h.nodes[p].parent.map_or(p, |_| p));
                            g.rrect(*r, 2.0, HUES[top % 6].with_alpha(210));
                        }
                    }
                },
            );
            let p5 = painted(
                "tidy tree",
                "Reingold–Tilford / Buchheim",
                Size::new(PW, PH),
                move |g, _| {
                    let pos = tree(&h2, (PW - 20.0, PH - 20.0));
                    for (i, n) in h2.nodes.iter().enumerate() {
                        if let Some(p) = n.parent {
                            g.line(pos[p] + o(10.0, 10.0), pos[i] + o(10.0, 10.0), DIM, 1.0);
                        }
                    }
                    for (i, p) in pos.iter().enumerate() {
                        g.circle(*p + o(10.0, 10.0), 4.0, HUES[h2.nodes[i].depth % 6]);
                    }
                },
            );

            let mut sim = Simulation::new(30);
            for i in 1..30 {
                sim.link(i, (i - 1) / 3, 22.0);
            }
            for _ in 0..((t / SPAN * 200.0) as usize + 1) {
                sim.tick();
            }
            let p6 = painted(
                "force layout",
                &format!(
                    "links + many-body (Barnes–Hut) + centre · α {:.3}",
                    sim.alpha
                ),
                Size::new(PW, PH),
                move |g, _| {
                    // Fit the layout's bounds into the panel.
                    let (mut lo, mut hi) = (o(f32::MAX, f32::MAX), o(f32::MIN, f32::MIN));
                    for n in &sim.nodes {
                        lo = o(lo.dx.min(n.position.dx), lo.dy.min(n.position.dy));
                        hi = o(hi.dx.max(n.position.dx), hi.dy.max(n.position.dy));
                    }
                    let k = ((PW - 20.0) / (hi.dx - lo.dx).max(1.0))
                        .min((PH - 20.0) / (hi.dy - lo.dy).max(1.0))
                        .min(1.5);
                    let mid = (lo + hi).scale(0.5);
                    let at = |p: Offset| (p - mid).scale(k) + o(PW / 2.0, PH / 2.0);
                    for l in &sim.links {
                        g.line(
                            at(sim.nodes[l.source].position),
                            at(sim.nodes[l.target].position),
                            DIM,
                            1.0,
                        );
                    }
                    for (i, n) in sim.nodes.iter().enumerate() {
                        g.circle(at(n.position), 5.0, HUES[i % 6]);
                    }
                },
            );

            let (cols, rows_n) = (49usize, 35usize);
            let field: Vec<f32> = (0..rows_n)
                .flat_map(|y| (0..cols).map(move |x| (x, y)))
                .map(|(x, y)| {
                    let (fx, fy) = (x as f32 / 8.0, y as f32 / 8.0);
                    (fx + t * 0.5).sin() * (fy * 1.3).cos()
                        + 0.5 * ((fx - 3.0).powi(2) + (fy - 2.0).powi(2)).sqrt().cos()
                })
                .collect();
            let p7 = painted(
                "contours",
                "marching squares · viridis thresholds",
                Size::new(PW, PH),
                move |g, _| {
                    for k in 0..9 {
                        let th = -1.2 + k as f32 * 0.3;
                        g.stroke(
                            isoline_path(&field, cols, rows_n, th, 5.0, o(0.0, 0.0)),
                            viridis(k as f32 / 8.0),
                            1.6,
                        );
                    }
                },
            );

            let vals = [3.0, 5.0 + t, 2.0, 4.0, 6.0 - t * 0.8];
            let p8 = painted(
                "pie / arcs",
                "pie() + padded arcs, donut",
                Size::new(PW, PH),
                move |g, _| {
                    for s in pie(&vals, true, 0.0, TAU, 0.03) {
                        g.fill(
                            arc(o(PW / 2.0, PH / 2.0), 35.0, 75.0, s.start, s.end, 0.0),
                            HUES[s.index % 6],
                        );
                    }
                },
            );
            page(
                "87 · canvas stage & data visualisation",
                "vieww-canvas (Konva) · vieww-dataviz (D3)",
                grid(4, vec![p1, p2, p3, p4, p5, p6, p7, p8]),
            )
        });
        feature_harness::set_page(d, view);
    })
}
