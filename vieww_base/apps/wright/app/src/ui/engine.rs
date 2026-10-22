//! The engine, on request: the agents and tools of the current session,
//! live. Hidden by default; Space peeks, E (or the Engine button) keeps it.
//!
//! ```text
//!  step 1        step 2        step 3        (steps run left to right;
//!  [Shipwright]->[Fetcher]  -> [Summarizer]   cards in one step run
//!                [Logwright]                  at the same time)
//!                                  ┌ inspector: what it may touch,
//!  ticker: the last three events   │ its tools, its live log
//! ```
//!
//! Everything here is projected from the session's event list — the same
//! list `wright replay` reads — so what you see is what was recorded.

use vieww::prelude::*;
use wright_core::events::{Event, EventKind};

use super::kit::*;
use super::session::{Card, Progress, Session, State};
use super::theme::Pal;
use super::{EngineView, Ui};

const NODE_W_MAX: f32 = 196.0;
const NODE_W_MIN: f32 = 140.0;
const NODE_H: f32 = 112.0;
const GAP_X: f32 = 44.0;
const GAP_Y: f32 = 30.0;
const INSPECTOR_W: f32 = 330.0;

pub fn layer(ui: &Ui) -> WidgetNode {
    let ui = ui.clone();
    Live::new("engine", move || {
        let p = ui.pal();
        let Some(s) = ui.current_session() else {
            return frame(&ui, &p, "Engine", String::new(), false, Center::new().child(para("No agents here yet. Describe what should exist, and Wright drafts the workforce for you to approve.", 15.0, p.muted)).into());
        };
        let Some(plan) = s.plan.get() else {
            return frame(&ui, &p, &s.title(), String::new(), false, Center::new().child(para("The agents appear once the plan is drafted.", 15.0, p.muted)).into());
        };
        let events = s.events.get();
        let progress = Progress::of(&events);
        let live = s.running.get();
        let run_no = events.first().map(|e| e.run).unwrap_or_default();
        let meta = if events.is_empty() {
            "Not run yet".to_string()
        } else {
            format!(
                "Run {run_no} · ${:.4} of ${:.2} · {:.1}k tokens in · {:.1}k out",
                progress.spent as f64 / 1e6,
                plan.per_run,
                progress.tokens.0 as f64 / 1000.0,
                progress.tokens.1 as f64 / 1000.0
            )
        };
        let selected = ui.selected.get();
        let graph: WidgetNode = {
            let (ui, s, plan) = (ui.clone(), s.clone(), plan.clone());
            LayoutBuilder::new(move |c: Constraints| {
                let (ui, s, plan) = (ui.clone(), s.clone(), plan.clone());
                let width = c.max_width;
                Live::new("engine-graph", move || graph(&ui, &s, &plan, width)).into()
            })
            .into()
        };
        let mut body = vec![grow(scroll_area(&ui, &ui.engine_scroll, pad(EdgeInsets::only(0.0, 6.0, 0.0, 6.0), graph)))];
        if let Some(name) = selected {
            if let Some(card) = plan.cards().find(|c| c.name == name) {
                body.push(SizedBox::width(INSPECTOR_W).child(inspector(&ui, &p, &s, card, &events, &progress)).into());
            }
        }
        let content = stretch(14.0, vec![
            grow(Flex::row().cross_axis_alignment(CrossAxisAlignment::Start).spacing(18.0).children(body)),
            ticker(&p, &events, &progress),
        ]);
        frame(&ui, &p, &s.title(), meta, live, content)
    })
    .into()
}

fn frame(
    ui: &Ui,
    p: &Pal,
    title: &str,
    meta: String,
    live: bool,
    content: WidgetNode,
) -> WidgetNode {
    let close = ui.clone();
    let peek = ui.engine.get() == EngineView::Peek;
    let mut head = vec![b(title.to_string(), 20.0, p.ink)];
    if live {
        let u = ui.clone();
        head.push(
            Live::new("live-badge", move || {
                let p = u.pal();
                let pulse = (u.clock.get() * 3.0).sin() * 0.5 + 0.5;
                row(
                    6.0,
                    vec![
                        Container::new()
                            .width(7.0)
                            .height(7.0)
                            .radius(3.5)
                            .color(p.accent_ink.with_alpha((120.0 + 135.0 * pulse) as u8))
                            .into(),
                        b("LIVE", 11.0, p.accent_ink),
                    ],
                )
            })
            .into(),
        );
    }
    head.push(t(meta, 12.5, p.muted));
    head.push(gap());
    if peek {
        head.push(t(
            "Let go of Space to hide · E keeps it open",
            12.0,
            p.muted,
        ));
    } else {
        head.push(tool_button(
            p,
            Glyph::Close,
            "Hide",
            None,
            false,
            move || close.set_engine(EngineView::Hidden),
        ));
    }
    pad(
        EdgeInsets::only(28.0, 14.0, 28.0, 18.0),
        stretch(16.0, vec![row(12.0, head), grow(content)]),
    )
}

/// Where each card goes: steps left to right, wrapping into rows when the
/// width runs out.
fn layout(stages: &[Vec<Card>], width: f32) -> (Vec<(f32, f32)>, f32, f32) {
    let width = width * 0.98;
    let n = stages.len().max(1) as f32;
    let fit = ((width + GAP_X) / (NODE_W_MIN + GAP_X)).floor().max(1.0);
    let per_row = n.min(fit);
    let node_w = ((width - GAP_X * (per_row - 1.0)) / per_row).clamp(NODE_W_MIN, NODE_W_MAX);
    let mut pos = Vec::new();
    let mut y_row = 0.0;
    let mut row_height: f32 = 0.0;
    for (i, stage) in stages.iter().enumerate() {
        let col = (i as f32) % per_row;
        if i > 0 && col == 0.0 {
            y_row += row_height + GAP_Y * 1.6;
            row_height = 0.0;
        }
        let x = col * (node_w + GAP_X);
        for (j, _) in stage.iter().enumerate() {
            pos.push((x, y_row + j as f32 * (NODE_H + GAP_Y)));
        }
        row_height = row_height.max(stage.len() as f32 * (NODE_H + GAP_Y) - GAP_Y);
    }
    (pos, node_w, y_row + row_height)
}

fn graph(ui: &Ui, s: &Session, plan: &super::session::PlanView, width: f32) -> WidgetNode {
    let p = ui.pal();
    let events = s.events.get();
    let progress = Progress::of(&events);
    let (pos, node_w, height) = layout(&plan.stages, width);
    let clock = ui.clock.get();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    // Edges: every card in a step feeds every card in the next.
    let mut edges = Vec::new();
    let starts: Vec<usize> = plan
        .stages
        .iter()
        .scan(0, |acc, st| {
            let s0 = *acc;
            *acc += st.len();
            Some(s0)
        })
        .collect();
    for (i, stage) in plan.stages.iter().enumerate() {
        if i + 1 < plan.stages.len() {
            for a in 0..stage.len() {
                for b2 in 0..plan.stages[i + 1].len() {
                    let from = pos[starts[i] + a];
                    let to = pos[starts[i + 1] + b2];
                    let target = progress.state(&plan.stages[i + 1][b2].name);
                    edges.push(Edge {
                        from,
                        to,
                        node_w,
                        state: target,
                    });
                }
            }
        }
    }
    let mut layers = vec![at(
        0.0,
        0.0,
        width,
        height,
        CustomPaint::sized(
            Size::new(width, height),
            Edges {
                edges,
                line: p.line_strong,
                flow: p.accent_ink,
                clock,
            },
        ),
    )];
    let selected = ui.selected.get();
    for (idx, card) in plan.cards().enumerate() {
        let (x, y) = pos[idx];
        let state = progress.state(&card.name);
        let hot = recent_tools(&events, &card.name, now_ms);
        layers.push(at(
            x,
            y,
            node_w,
            NODE_H,
            node(
                ui,
                &p,
                card,
                state,
                &hot,
                selected.as_deref() == Some(card.name.as_str()),
                node_w,
            ),
        ));
    }
    sized(width, height, Stack::new().children(layers))
}

/// Tools this agent called in the last ~1.2 seconds (they light up).
fn recent_tools(events: &[Event], agent: &str, now_ms: u64) -> Vec<String> {
    events
        .iter()
        .rev()
        .take(40)
        .filter(|e| {
            e.kind == EventKind::ToolCalled
                && e.agent.as_deref() == Some(agent)
                && now_ms.saturating_sub(e.at_ms) < 1200
        })
        .filter_map(|e| e.note.split_whitespace().next().map(String::from))
        .collect()
}

fn node(
    ui: &Ui,
    p: &Pal,
    card: &Card,
    state: State,
    hot: &[String],
    selected: bool,
    width: f32,
) -> WidgetNode {
    let p = *p;
    let (pill_fg, pill_bg) = match state {
        State::Working => (p.accent_ink, p.accent_soft),
        State::Waiting => (p.wait, p.wait_soft),
        State::Done => (p.good, p.good_soft),
        State::Failed => (p.bad, p.surface2),
        _ => (p.muted, p.surface2),
    };
    let edge = match state {
        State::Working => p.accent,
        State::Waiting => p.wait,
        _ if selected => p.accent_ink,
        _ => p.line,
    };
    // As many tool chips as fit the card's width (about 6.6 px a letter
    // plus padding), then "+n".
    let room = width - 22.0;
    let mut used = 0.0;
    let mut shown: Vec<&String> = Vec::new();
    for tool in &card.tools {
        let wide = tool.chars().count() as f32 * 6.6 + 16.0;
        if used + wide > room - 26.0 {
            break;
        }
        used += wide + 4.0;
        shown.push(tool);
    }
    let rest = card.tools.len() - shown.len();
    let mut chips: Vec<WidgetNode> = shown
        .into_iter()
        .map(|tool| {
            let on = hot.iter().any(|h| h == tool);
            Container::new()
                .color(if on { p.accent } else { p.surface2 })
                .radius(corner::CHIP)
                .border(Border::new(if on { p.accent } else { p.line }, 1.0))
                .padding(EdgeInsets::symmetric(6.0, 1.0))
                .child(mono(
                    tool.clone(),
                    10.5,
                    if on { p.on_accent } else { p.muted },
                ))
                .into()
        })
        .collect();
    if rest > 0 {
        chips.push(t(format!("+{rest}"), 10.5, p.muted));
    }
    // A queued card is quieter — its words fade, its surface does not: a
    // see-through card let the page behind the engine view read through
    // it, word over word.
    let faded = if state == State::Queued { 0.6 } else { 1.0 };
    let body = Container::new()
        .color(p.surface)
        .radius(corner::CARD)
        .border(Border::new(
            edge,
            if matches!(state, State::Working | State::Waiting) || selected {
                2.0
            } else {
                1.0
            },
        ))
        .padding(EdgeInsets::all(11.0))
        .child(fade(
            faded,
            stretch(
                4.0,
                vec![
                    row(
                        6.0,
                        vec![
                            fill(card.name.clone(), 13.5, p.ink),
                            pill(state.label(), pill_fg, pill_bg),
                        ],
                    ),
                    Text::new(format!(
                        "{} · {}",
                        card.role,
                        if card.model.is_empty() {
                            "no model"
                        } else {
                            &card.model
                        }
                    ))
                    .size(11.5)
                    .color(p.muted)
                    .max_lines(2)
                    .overflow(vieww::text::TextOverflow::Ellipsis)
                    .into(),
                    gap(),
                    row(4.0, chips),
                ],
            ),
        ));
    let (sel, name) = (ui.selected.clone(), card.name.clone());
    tap(body, move || {
        sel.set(if sel.get().as_deref() == Some(name.as_str()) {
            None
        } else {
            Some(name.clone())
        })
    })
}

fn inspector(
    ui: &Ui,
    p: &Pal,
    s: &Session,
    card: &Card,
    events: &[Event],
    progress: &Progress,
) -> WidgetNode {
    let state = progress.state(&card.name);
    let close = ui.selected.clone();
    let start = progress.started_ms;
    let log: Vec<WidgetNode> = events
        .iter()
        .filter(|e| {
            e.agent.as_deref() == Some(card.name.as_str()) && e.kind != EventKind::TokensRecorded
        })
        .rev()
        .take(14)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|e| {
            row(
                8.0,
                vec![
                    SizedBox::width(52.0)
                        .child(mono(
                            format!("+{:.1}s", e.at_ms.saturating_sub(start) as f64 / 1000.0),
                            11.0,
                            p.muted,
                        ))
                        .into(),
                    grow(copyable(e.note.clone(), 12.0, p.ink)),
                ],
            )
        })
        .collect();
    // The whole log, for pasting into a bug report or a message.
    let full_log: String = events
        .iter()
        .filter(|e| {
            e.agent.as_deref() == Some(card.name.as_str()) && e.kind != EventKind::TokensRecorded
        })
        .map(|e| {
            format!(
                "+{:.1}s  {}",
                e.at_ms.saturating_sub(start) as f64 / 1000.0,
                e.note
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let time = match state {
        State::Done | State::Failed => format!("{:.1}s", progress.seconds(&card.name)),
        State::Cached => "reused from cache, $0".into(),
        other => other.label().to_string(),
    };
    let _ = s;
    card_box(
        p,
        stretch(
            10.0,
            vec![
                row(
                    8.0,
                    vec![
                        fill(card.name.clone(), 16.0, p.ink),
                        icon_button(p, Glyph::Close, "Close", false, move || close.set(None)),
                    ],
                ),
                t(
                    format!(
                        "{} · {}",
                        card.role,
                        if card.model.is_empty() {
                            "no model"
                        } else {
                            &card.model
                        }
                    ),
                    12.5,
                    p.muted,
                ),
                kv(p, "State", state.label()),
                kv(p, "May touch", &card.touch),
                kv(
                    p,
                    "Tools",
                    &if card.tools.is_empty() {
                        "none".to_string()
                    } else {
                        card.tools.join(", ")
                    },
                ),
                kv(p, "Time", &time),
                row(
                    8.0,
                    vec![
                        t("Live log", 12.0, p.muted),
                        gap(),
                        if full_log.is_empty() {
                            SizedBox::shrink().into()
                        } else {
                            copy_icon(p.muted, full_log)
                        },
                    ],
                ),
                Container::new()
                    .color(p.surface2)
                    .radius(corner::FIELD)
                    .padding(EdgeInsets::all(10.0))
                    .child(if log.is_empty() {
                        t("Nothing yet. Lines appear here as it works.", 12.0, p.muted)
                    } else {
                        stretch(5.0, log)
                    })
                    .into(),
            ],
        ),
    )
}

fn card_box(p: &Pal, child: WidgetNode) -> WidgetNode {
    Container::new()
        .color(p.surface)
        .radius(corner::CARD)
        .border(Border::new(p.line, 1.0))
        .padding(EdgeInsets::all(16.0))
        .child(child)
        .into()
}

fn kv(p: &Pal, k: &str, v: &str) -> WidgetNode {
    row(
        8.0,
        vec![
            SizedBox::width(78.0)
                .child(line(k.to_string(), 12.5, p.muted))
                .into(),
            grow(copyable(v.to_string(), 12.5, p.ink)),
        ],
    )
}

fn ticker(p: &Pal, events: &[Event], progress: &Progress) -> WidgetNode {
    let last: Vec<&Event> = events
        .iter()
        .filter(|e| e.kind != EventKind::TokensRecorded)
        .rev()
        .take(3)
        .collect();
    let lines: Vec<WidgetNode> = last
        .iter()
        .rev()
        .enumerate()
        .map(|(i, e)| {
            let alpha = [0.45, 0.7, 1.0][i + 3 - last.len().min(3)];
            fade(
                alpha,
                row(
                    10.0,
                    vec![
                        SizedBox::width(56.0)
                            .child(mono(
                                format!(
                                    "+{:.1}s",
                                    e.at_ms.saturating_sub(progress.started_ms) as f64 / 1000.0
                                ),
                                11.5,
                                p.muted,
                            ))
                            .into(),
                        SizedBox::width(110.0)
                            .child(line(
                                e.agent.clone().unwrap_or_else(|| "wright".into()),
                                12.0,
                                p.ink,
                            ))
                            .into(),
                        fill(e.note.clone(), 12.0, p.muted),
                    ],
                ),
            )
        })
        .collect();
    sized(f32::INFINITY, 62.0, stretch(3.0, lines))
}

// ── edges ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
struct Edge {
    from: (f32, f32),
    to: (f32, f32),
    node_w: f32,
    state: State,
}

#[derive(Debug, Clone, PartialEq)]
struct Edges {
    edges: Vec<Edge>,
    line: Color,
    flow: Color,
    clock: f32,
}

impl Edges {
    /// An elbow from the right side of `a` to the left side of `b`, or —
    /// when `b` wrapped onto the next row — down, across and down.
    fn path(e: &Edge) -> Vec<Offset> {
        let (ax, ay) = e.from;
        let (bx, by) = e.to;
        let (amid, bmid) = (ay + NODE_H / 2.0, by + NODE_H / 2.0);
        if bx > ax {
            let x1 = ax + e.node_w;
            let mx = (x1 + bx) / 2.0;
            vec![
                Offset::new(x1, amid),
                Offset::new(mx, amid),
                Offset::new(mx, bmid),
                Offset::new(bx, bmid),
            ]
        } else {
            let y1 = ay + NODE_H;
            let my = by - GAP_Y * 0.8;
            vec![
                Offset::new(ax + e.node_w / 2.0, y1),
                Offset::new(ax + e.node_w / 2.0, my),
                Offset::new(bx + e.node_w / 2.0, my),
                Offset::new(bx + e.node_w / 2.0, by),
            ]
        }
    }
}

/// Marching dashes: a dash wherever `(distance + phase) mod PERIOD < DASH`,
/// so increasing `phase` moves them along the path.
fn dashes(out: &mut Vec<DrawInstruction>, pts: &[Offset], color: Color, phase: f32) {
    const DASH: f32 = 5.0;
    const PERIOD: f32 = 12.0;
    let mut base = 0.0;
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = ((b.dx - a.dx).powi(2) + (b.dy - a.dy).powi(2)).sqrt();
        if len < 0.01 {
            continue;
        }
        let at = |d: f32| {
            Offset::new(
                a.dx + (b.dx - a.dx) * d / len,
                a.dy + (b.dy - a.dy) * d / len,
            )
        };
        let mut start = ((base + phase) / PERIOD).floor() * PERIOD - phase;
        while start < base + len {
            let (s0, s1) = (start.max(base), (start + DASH).min(base + len));
            if s1 > s0 {
                out.push(DrawInstruction::DrawLine {
                    from: at(s0 - base),
                    to: at(s1 - base),
                    color,
                    width: 2.0,
                });
            }
            start += PERIOD;
        }
        base += len;
    }
}

impl CustomPainter for Edges {
    fn paint(&self, _size: Size) -> Vec<DrawInstruction> {
        let mut out = Vec::new();
        for e in &self.edges {
            let pts = Edges::path(e);
            match e.state {
                State::Working | State::Waiting => {
                    dashes(&mut out, &pts, self.flow, self.clock * 28.0)
                }
                State::Done | State::Cached => {
                    for w in pts.windows(2) {
                        out.push(DrawInstruction::DrawLine {
                            from: w[0],
                            to: w[1],
                            color: self.flow.with_alpha(130),
                            width: 1.6,
                        });
                    }
                }
                _ => {
                    for w in pts.windows(2) {
                        out.push(DrawInstruction::DrawLine {
                            from: w[0],
                            to: w[1],
                            color: self.line,
                            width: 1.4,
                        });
                    }
                }
            }
        }
        out
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn should_repaint(&self, previous: &dyn CustomPainter) -> bool {
        previous.as_any().downcast_ref::<Edges>() != Some(self)
    }
}
