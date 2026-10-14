//! Four runtimes in one page — a TouchDesigner-style cooking node graph
//! with a Blueprint event graph, a Unity/Godot-style ECS game loop, CRDT
//! collaboration, and Lottie playback — photographed.
//!
//! 1. `vieww-graph`: Time → LFO → Remap → Trail, Noise and an Expression
//!    summed by Math; pulled lazily (dirty flags; the cook counters show only
//!    what changed re-cooked). An `ExecGraph` (Event → ForLoop → Branch →
//!    Call) fires every second; its trace is the caption.
//! 2. `vieww-game`: a sun/planet/moon hierarchy spun by `Behaviour`s with
//!    transforms propagated parent→child, and a `Coroutine` that
//!    instantiates comet prefabs on a timer and despawns them.
//! 3. `vieww-collab`: two replicas type into the same text concurrently,
//!    exchange ops as JSON, and converge; LWW map, OR-set and counter too,
//!    with a presence cursor anchored to a character.
//! 4. `vieww-lottie`: a Bodymovin JSON (parented null, eased position
//!    keys, animated star, rounded rect, trim-path draw-on) played by the
//!    vector renderer.

use std::cell::RefCell;
use std::f32::consts::TAU;
use std::rc::Rc;

use feature_harness::draw::{grid, page, painted, plot, xywh, DIM, HUES, INK};
use vieww_collab::{anchor_at, Doc, Op, Presence};
use vieww_foundation::json::Json;
use vieww_foundation::{Offset, Size};
use vieww_game::{Behaviour, Coroutine, Ctx, Game, GlobalTransform, InputMap, Name, Transform2};
use vieww_graph::{
    Constant, ExecGraph, ExecNode, Expression, Graph, Lfo, Math, Noise, Remap, Time, Trail, Value,
};
use vieww_lottie::Composition;
use vieww_widget::prelude::*;

const SPAN: f32 = 6.0;

fn o(x: f32, y: f32) -> Offset {
    Offset::new(x, y)
}

// ---------------------------------------------------------------- graph

struct Net {
    g: Graph,
    exec: ExecGraph,
    out: vieww_graph::NodeId,
    trail: vieww_graph::NodeId,
    boxes: Vec<(vieww_graph::NodeId, Offset)>,
}

fn net() -> Net {
    let mut g = Graph::new();
    let time = g.add("time", Time);
    let lfo = g.add("lfo", Lfo);
    let remap = g.add("remap", Remap);
    let trail = g.add("trail", Trail::default());
    let noise = g.add("noise", Noise);
    let expr = g.add("expr", Expression::default());
    let k = g.add("gain", Constant);
    let add = g.add("add", Math::Add);
    let mul = g.add("mul", Math::Multiply);
    g.set_param(lfo, "frequency", Value::Float(0.5));
    g.set_param(remap, "to_lo", Value::Float(-1.0));
    g.set_param(expr, "expr", Value::Text("sin(a * 3) * 0.3".into()));
    g.set_param(k, "value", Value::Float(0.8));
    g.connect(time, "seconds", lfo, "time").ok();
    g.connect(lfo, "out", remap, "in").ok();
    g.connect(time, "seconds", noise, "x").ok();
    g.connect(time, "seconds", expr, "a").ok();
    g.connect(remap, "out", add, "a").ok();
    g.connect(expr, "out", add, "b").ok();
    g.connect(add, "out", mul, "a").ok();
    g.connect(k, "out", mul, "b").ok();
    g.connect(mul, "out", trail, "in").ok();
    let mut exec = ExecGraph::new();
    let ev = exec.add(ExecNode::Event("Tick".into()));
    let lp = exec.add(ExecNode::ForLoop {
        count: (k, "out".into()),
        index_var: "i".into(),
    });
    let br = exec.add(ExecNode::Branch {
        condition: (remap, "out".into()),
    });
    let hi = exec.add(ExecNode::Call("Flash".into()));
    let lo = exec.add(ExecNode::Call("Dim".into()));
    let set = exec.add(ExecNode::SetVariable {
        name: "level".into(),
        value: (mul, "out".into()),
    });
    exec.then(ev, 0, lp);
    exec.then(lp, 0, set);
    exec.then(lp, 1, br);
    exec.then(br, 0, hi);
    exec.then(br, 1, lo);
    let boxes = vec![
        (time, o(10.0, 20.0)),
        (lfo, o(70.0, 10.0)),
        (remap, o(130.0, 10.0)),
        (noise, o(70.0, 60.0)),
        (expr, o(70.0, 100.0)),
        (k, o(130.0, 130.0)),
        (add, o(130.0, 70.0)),
        (mul, o(190.0, 100.0)),
        (trail, o(190.0, 40.0)),
    ];
    Net {
        g,
        exec,
        out: mul,
        trail,
        boxes,
    }
}

// ---------------------------------------------------------------- game

struct Spin(f32);

impl Behaviour for Spin {
    fn update(&mut self, ctx: &mut Ctx<'_>, dt: f32) {
        if let Some(t) = ctx.world.get_mut::<Transform2>(ctx.entity) {
            t.rotation += self.0 * dt;
        }
    }
}

struct Drift(Offset);

impl Behaviour for Drift {
    fn update(&mut self, ctx: &mut Ctx<'_>, dt: f32) {
        if let Some(t) = ctx.world.get_mut::<Transform2>(ctx.entity) {
            t.position = t.position + self.0.scale(dt);
            if t.position.dx > 260.0 {
                ctx.commands.despawn(ctx.entity);
            }
        }
    }
}

fn game() -> Game {
    let mut g = Game::new(InputMap::new());
    let w = &mut g.world;
    let sun = w.spawn();
    w.insert(sun, Transform2::at(o(120.0, 95.0)));
    w.insert(sun, Name("sun".into()));
    let mut bodies = vec![];
    for (i, (r, speed)) in [(40.0, 1.3), (70.0, 0.7)].iter().enumerate() {
        let pivot = w.spawn();
        w.insert(pivot, Transform2::default());
        w.set_parent(pivot, Some(sun));
        let planet = w.spawn();
        w.insert(planet, Transform2::at(o(*r, 0.0)));
        w.insert(planet, Name(format!("planet{i}")));
        w.set_parent(planet, Some(pivot));
        bodies.push((pivot, *speed));
        let moon_pivot = w.spawn();
        w.insert(moon_pivot, Transform2::default());
        w.set_parent(moon_pivot, Some(planet));
        let moon = w.spawn();
        w.insert(moon, Transform2::at(o(14.0, 0.0)));
        w.insert(moon, Name(format!("moon{i}")));
        w.set_parent(moon, Some(moon_pivot));
        bodies.push((moon_pivot, 4.0));
    }
    for (e, s) in bodies {
        g.attach(e, Spin(s));
    }
    g.register_prefab("comet", |w, at| {
        let e = w.spawn();
        w.insert(e, Transform2::at(at));
        w.insert(e, Name("comet".into()));
        (
            e,
            Some(Box::new(Drift(o(90.0, 18.0))) as Box<dyn Behaviour>),
        )
    });
    let mut c = Coroutine::new();
    for k in 0..8 {
        c = c
            .wait(0.6)
            .then(move |_, cmd| cmd.instantiate("comet", o(0.0, 20.0 + (k % 4) as f32 * 40.0)));
    }
    g.start_coroutine(c);
    g
}

// ---------------------------------------------------------------- collab

fn exchange(a: &mut Doc, b: &mut Doc) -> usize {
    // Ops travel as JSON, as they would over a socket.
    let wire: Vec<String> = a
        .ops_since(b.clock())
        .iter()
        .map(|op| op.to_json().to_string())
        .collect();
    for s in &wire {
        let op = Op::from_json(&Json::parse(s).expect("json")).expect("op");
        b.apply(&op);
    }
    let back: Vec<String> = b
        .ops_since(a.clock())
        .iter()
        .map(|op| op.to_json().to_string())
        .collect();
    for s in &back {
        a.apply(&Op::from_json(&Json::parse(s).expect("json")).expect("op"));
    }
    wire.len() + back.len()
}

type Cursors = Vec<(u32, String, usize)>;

fn collab(t: f32) -> (String, String, usize, String, Cursors) {
    let mut a = Doc::new(1);
    let mut b = Doc::new(2);
    a.insert(0, "Hello world");
    exchange(&mut a, &mut b);
    let steps = (t / SPAN * 12.0) as usize;
    let a_typing = " from Ada";
    let b_typing = "Dear ";
    let mut sent = 0;
    for k in 0..steps.min(9) {
        let at = a.text.value().chars().count();
        a.insert(at, &a_typing[k..=k]);
        if k < b_typing.len() {
            b.insert(k, &b_typing[k..=k]);
        }
        if k % 3 == 2 {
            sent += exchange(&mut a, &mut b);
        }
    }
    if steps >= 10 {
        a.set("title", Some(Json::String("draft".into())));
        b.set("title", Some(Json::String("final".into())));
        a.add("reviewed");
        b.count(3);
        a.count(2);
        sent += exchange(&mut a, &mut b);
    }
    let mut presence = Presence::default();
    let pos = b.text.value().find('w').unwrap_or(0);
    presence.update(2, 1, "Bo", anchor_at(&b.text, pos));
    let cursors = presence.positions(&a.text);
    let meta = format!(
        "title {} · set {:?} · counter {}",
        a.map.get("title").map_or("-".to_owned(), |j| j.to_string()),
        a.set.elements(),
        a.counter.value()
    );
    (a.text.value(), b.text.value(), sent, meta, cursors)
}

// ---------------------------------------------------------------- lottie

const LOTTIE: &str = r##"{
  "v": "5.7.0", "fr": 30, "ip": 0, "op": 90, "w": 240, "h": 170,
  "layers": [
    {"ty": 3, "nm": "rig", "ind": 1, "ip": 0, "op": 90,
     "ks": {"p": {"a": 0, "k": [120, 85]}, "r": {"a": 1, "k": [{"t": 0, "s": [0], "o": {"x": [0.4], "y": [0]}, "i": {"x": [0.6], "y": [1]}}, {"t": 90, "s": [360]}]}}},
    {"ty": 4, "nm": "star", "ind": 2, "parent": 1, "ip": 0, "op": 90,
     "ks": {"p": {"a": 0, "k": [55, 0]}, "s": {"a": 1, "k": [{"t": 0, "s": [60, 60]}, {"t": 45, "s": [120, 120]}, {"t": 90, "s": [60, 60]}]}},
     "shapes": [{"ty": "sr", "sy": 1, "pt": {"a": 0, "k": 5}, "p": {"a": 0, "k": [0, 0]}, "or": {"a": 0, "k": 20}, "ir": {"a": 0, "k": 9}, "r": {"a": 0, "k": 0}},
                {"ty": "fl", "c": {"a": 0, "k": [0.9, 0.75, 0.3, 1]}, "o": {"a": 0, "k": 100}}]},
    {"ty": 4, "nm": "ball", "ind": 3, "ip": 0, "op": 90,
     "ks": {"p": {"a": 1, "k": [{"t": 0, "s": [30, 30], "o": {"x": [0.5], "y": [0]}, "i": {"x": [1], "y": [1]}}, {"t": 22, "s": [60, 140], "o": {"x": [0], "y": [0]}, "i": {"x": [0.5], "y": [1]}}, {"t": 45, "s": [90, 60], "o": {"x": [0.5], "y": [0]}, "i": {"x": [1], "y": [1]}}, {"t": 67, "s": [120, 140]}, {"t": 90, "s": [30, 30]}]}},
     "shapes": [{"ty": "el", "p": {"a": 0, "k": [0, 0]}, "s": {"a": 0, "k": [22, 22]}}, {"ty": "fl", "c": {"a": 0, "k": [0.38, 0.69, 0.94, 1]}, "o": {"a": 0, "k": 100}}]},
    {"ty": 4, "nm": "card", "ind": 4, "ip": 0, "op": 90,
     "ks": {"p": {"a": 0, "k": [195, 40]}, "o": {"a": 1, "k": [{"t": 0, "s": [20]}, {"t": 45, "s": [100]}, {"t": 90, "s": [20]}]}},
     "shapes": [{"ty": "rc", "p": {"a": 0, "k": [0, 0]}, "s": {"a": 0, "k": [60, 40]}, "r": {"a": 0, "k": 10}}, {"ty": "fl", "c": {"a": 0, "k": [0.6, 0.76, 0.47, 1]}, "o": {"a": 0, "k": 100}}]},
    {"ty": 4, "nm": "scribble", "ind": 5, "ip": 0, "op": 90, "ks": {},
     "shapes": [{"ty": "sh", "ks": {"a": 0, "k": {"i": [[0,0],[-30,0],[-30,0],[0,0]], "o": [[30,0],[30,0],[30,0],[0,0]], "v": [[10,160],[80,120],[160,160],[230,120]], "c": false}}},
                {"ty": "st", "c": {"a": 0, "k": [0.94, 0.44, 0.47, 1]}, "o": {"a": 0, "k": 100}, "w": {"a": 0, "k": 4}},
                {"ty": "tm", "s": {"a": 0, "k": 0}, "e": {"a": 1, "k": [{"t": 0, "s": [0]}, {"t": 60, "s": [100]}]}, "o": {"a": 0, "k": 0}}]}
  ]
}"##;

/// Clock, graph, game, last event-graph trace.
type Run = (f32, Net, Game, Vec<String>);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let comp = Rc::new(Composition::parse(LOTTIE).map_err(|e| e.0)?);
    feature_harness::launch(
        "88 — graph, game, collab, lottie",
        Size::new(1160.0, 330.0),
        move |d| {
            let comp = comp.clone();
            let state: Rc<RefCell<Run>> = Rc::new(RefCell::new((0.0, net(), game(), Vec::new())));
            let view = feature_harness::clocked(d, SPAN, move |t| {
                let mut st = state.borrow_mut();
                if t < st.0 {
                    *st = (0.0, net(), game(), Vec::new());
                }
                while st.0 + 1.0 / 60.0 <= t {
                    st.0 += 1.0 / 60.0;
                    let now = st.0;
                    let (_, n, g, trace) = &mut *st;
                    n.g.set_time(now);
                    let _ = n.g.pull(n.trail, "channel");
                    if (now * 60.0) as i32 % 60 == 0 {
                        if let Ok(tr) = n.exec.fire("Tick", &mut n.g) {
                            *trace = tr;
                        }
                    }
                    g.frame(1.0 / 60.0);
                }
                let (_, n, g, trace) = &mut *st;
                let channel = match n.g.pull(n.trail, "channel") {
                    Ok(Value::Channel(c)) => c,
                    _ => Vec::new(),
                };
                let level = n.g.pull(n.out, "out").map(|v| v.as_f32()).unwrap_or(0.0);
                let boxes: Vec<(String, Offset, u64)> = n
                    .boxes
                    .iter()
                    .map(|(id, p)| (n.g.name(*id).to_owned(), *p, n.g.cooks(*id)))
                    .collect();
                let wires: Vec<(Offset, Offset)> =
                    n.g.wires()
                        .iter()
                        .filter_map(|w| {
                            let a = n.boxes.iter().find(|b| b.0 == w.from)?.1;
                            let b = n.boxes.iter().find(|b| b.0 == w.to)?.1;
                            Some((a + o(44.0, 9.0), b + o(0.0, 9.0)))
                        })
                        .collect();
                let mut trace_s = trace.join(" → ");
                if trace_s.chars().count() > 34 {
                    trace_s = trace_s.chars().take(33).collect::<String>() + "…";
                }
                let labels: Vec<WidgetNode> = boxes
                    .iter()
                    .map(|(n, p, _)| {
                        Positioned::new()
                            .left(p.dx + 4.0)
                            .top(p.dy + 2.0)
                            .child(Text::new(n.clone()).size(9.0).color(INK))
                            .into()
                    })
                    .collect();
                let graph_paint =
                    feature_harness::draw::paint(Size::new(240.0, 190.0), move |gk, _| {
                        for (a, b) in &wires {
                            let mut p = vieww_foundation::Path::new();
                            p.move_to(*a)
                                .cubic_to(*a + o(20.0, 0.0), *b - o(20.0, 0.0), *b);
                            gk.stroke(p, DIM, 1.2);
                        }
                        for (i, (_, p, cooks)) in boxes.iter().enumerate() {
                            gk.rrect(
                                xywh(p.dx, p.dy, 44.0, 18.0),
                                4.0,
                                HUES[i % 6].with_alpha(200),
                            );
                            gk.rect(
                                xywh(
                                    p.dx + 2.0,
                                    p.dy + 14.0,
                                    (*cooks as f32 / 8.0).min(40.0),
                                    2.0,
                                ),
                                INK,
                            );
                        }
                        gk.stroke(
                            plot(&channel, o(10.0, 150.0), Size::new(220.0, 36.0), -1.5, 1.5),
                            HUES[1],
                            1.5,
                        );
                    });
                let mut layers: Vec<WidgetNode> = vec![graph_paint];
                layers.extend(labels);
                let p1 = feature_harness::draw::panel(
                    "node graph + event graph",
                    &format!(
                        "out {level:.2} · Tick: {}",
                        if trace_s.is_empty() {
                            "…".into()
                        } else {
                            trace_s
                        }
                    ),
                    Stack::new().children(layers),
                );

                let w = &g.world;
                let mut bodies: Vec<(String, Offset)> = Vec::new();
                for e in w.query::<GlobalTransform>() {
                    if let Some(Name(nm)) = w.get::<Name>(e) {
                        let gt = w.get::<GlobalTransform>(e).expect("queried").0;
                        bodies.push((nm.clone(), gt.apply(Offset::ZERO)));
                    }
                }
                let count = w.len();
                let p2 = painted(
                    "ECS + behaviours + coroutine",
                    &format!(
                        "{count} entities · frame {} · hierarchy propagated",
                        g.frame
                    ),
                    Size::new(240.0, 190.0),
                    move |gk, _| {
                        gk.stroke(
                            feature_harness::draw::circle(o(120.0, 95.0), 40.0),
                            DIM.with_alpha(60),
                            1.0,
                        );
                        gk.stroke(
                            feature_harness::draw::circle(o(120.0, 95.0), 70.0),
                            DIM.with_alpha(60),
                            1.0,
                        );
                        for (nm, p) in &bodies {
                            let (r, c) = match nm.as_str() {
                                "sun" => (14.0, HUES[3]),
                                "comet" => (3.0, INK),
                                n if n.starts_with("planet") => (7.0, HUES[0]),
                                _ => (3.0, HUES[5]),
                            };
                            gk.circle(*p, r, c);
                        }
                    },
                );
                drop(st);

                let (ta, tb, sent, meta, cursors) = collab(t);
                let same = ta == tb;
                let cursor = cursors
                    .first()
                    .map_or(String::new(), |(r, n, p)| format!("{n}#{r}@{p}"));
                let p3 = feature_harness::draw::panel(
                    "CRDT collaboration",
                    &format!(
                        "{sent} ops over JSON · {} · {cursor}",
                        if same {
                            "converged"
                        } else {
                            "diverged (pending sync)"
                        }
                    ),
                    Container::new().width(240.0).height(190.0).child(
                        Flex::column().spacing(6.0).children(children![
                            Text::new("replica A").size(10.0).color(DIM),
                            Text::new(ta.clone()).size(13.0).color(HUES[0]),
                            Text::new("replica B").size(10.0).color(DIM),
                            Text::new(tb.clone()).size(13.0).color(HUES[1]),
                            Text::new(meta).size(10.0).color(INK),
                        ]),
                    ),
                );

                let frame = comp.frame_at(t / SPAN * comp.duration());
                let comp2 = comp.clone();
                let p4 = painted(
                    "Lottie playback",
                    &format!(
                        "frame {frame:.0}/{:.0} · {} layers",
                        comp.duration() * 30.0,
                        comp.layer_names().len()
                    ),
                    Size::new(240.0, 190.0),
                    move |gk, _| {
                        gk.rect(xywh(0.0, 0.0, 240.0, 170.0), DIM.with_alpha(25));
                        for it in comp2.render(frame, xywh(0.0, 0.0, 240.0, 170.0)).items() {
                            gk.push(it.clone());
                        }
                    },
                );
                page(
                    "88 · node graph, ECS, CRDT, Lottie",
                    "vieww-graph · vieww-game · vieww-collab · vieww-lottie",
                    grid(4, vec![p1, p2, p3, p4]),
                )
            });
            feature_harness::set_page(d, view);
            let _ = TAU;
        },
    )
}
