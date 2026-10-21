//! A session's page: one conversation, top to bottom.
//!
//! ```text
//!  your request                         (bubble, right)
//!  the plan        -> Approve & run     (before the first run)
//!  the run         steps, live          (while running, and after)
//!  approvals       drag right / left    (when something needs you)
//!  the result      proof, then actions  (after a run)
//!  the conversation                     (changes you asked for)
//!  composer        + · text · mic · send
//! ```

use std::rc::Rc;

use vieww::foundation::DragDetails;
use vieww::prelude::*;

use super::kit::*;
use super::session::{Composer, Progress, Session, State, Tone, Voice};
use super::theme::Pal;
use super::{EngineView, Ui};

const COLUMN: f32 = 760.0;

pub fn page(ui: &Ui, s: &Session) -> WidgetNode {
    let content: WidgetNode = {
        let (ui, s) = (ui.clone(), s.clone());
        Live::new("thread", move || body(&ui, &s)).into()
    };
    let scroller = scroll_area(ui, &s.scroll, pad(EdgeInsets::only(24.0, 28.0, 24.0, 150.0), reading(COLUMN, content)));
    let composer_node: WidgetNode = {
        let (ui, s) = (ui.clone(), s.clone());
        Live::new("thread-composer", move || {
            let p = ui.pal();
            let send: Rc<dyn Fn()> = {
                let s = s.clone();
                Rc::new(move || {
                    if s.send() {
                        // Show the reply as it arrives.
                        s.scroll.to_end();
                    }
                })
            };
            let hint = if s.plan.get().is_some() { "Ask for a change, e.g. make it weekly" } else { "Describe what should exist" };
            Container::new()
                .color(p.ground.with_alpha(235))
                .padding(EdgeInsets::only(24.0, 10.0, 24.0, 16.0))
                .child(reading(COLUMN, composer(&ui, &s.composer, send, hint, false)))
                .into()
        })
        .into()
    };
    stack(vec![layer(scroller), Positioned::new().left(0.0).right(0.0).bottom(0.0).child(composer_node).into()])
}

fn body(ui: &Ui, s: &Session) -> WidgetNode {
    let p = ui.pal();
    let mut items: Vec<WidgetNode> = Vec::new();
    let plan = s.plan.get();
    let chat = s.chat.get();
    let request = plan.as_ref().map(|pl| pl.request.clone()).filter(|r| !r.is_empty()).or_else(|| chat.iter().find(|(u, _)| *u).map(|(_, t)| t.clone()));

    match &request {
        Some(r) => items.push(bubble(&p, r)),
        None => items.push(column(10.0, vec![
            Text::new("What should exist?").size(34.0).color(p.ink).weight(FontWeight::Bold).into(),
            para("Describe the outcome in your own words. Wright drafts a plan, shows what it will touch and cost, and runs nothing until you approve.", 15.0, p.muted),
        ])),
    }

    // Drafting: the interview's questions and answers, then its summary.
    if plan.is_none() {
        for (user, text) in chat.iter().skip(if request.is_some() { 1 } else { 0 }) {
            items.push(if *user { bubble(&p, text) } else { para(text.clone(), 15.0, p.ink) });
        }
        if let Some(lines) = s.confirm.get() {
            items.push(confirm_card(&p, s, &lines));
        }
    }

    let running = s.running.get();
    let result = s.result.get();
    let events = s.events.get();
    if let Some(pl) = &plan {
        if !running && result.is_none() && events.is_empty() {
            items.push(plan_card(ui, &p, s, pl));
        } else {
            items.push(run_card(ui, &p, pl, &events, running));
        }
    }
    for (run, n, agent, reason) in s.approvals.get() {
        items.push(approval(&p, s, run, n, &agent, &reason));
    }
    if let (Some(r), false) = (&result, running) {
        items.push(result_card(ui, &p, s, r));
    }
    // After a plan exists, the conversation is about changes.
    if plan.is_some() {
        for (user, text) in chat.iter() {
            items.push(if *user { bubble(&p, text) } else { para(text.clone(), 15.0, p.ink) });
        }
        if let Some(lines) = s.confirm.get() {
            items.push(confirm_card(&p, s, &lines));
        }
    }
    let notice = s.notice.get();
    if !notice.is_empty() {
        items.push(t(notice, 13.0, p.wait));
    }
    stretch(16.0, items)
}

fn bubble(p: &Pal, text: &str) -> WidgetNode {
    Align::new(Alignment::CENTER_RIGHT)
        .child(
            Constrained::new(Constraints::new(0.0, 560.0, 0.0, f32::INFINITY))
                .child(Container::new().color(p.bubble).radius(20.0).padding(EdgeInsets::symmetric(16.0, 11.0)).child(para(text.to_string(), 15.0, p.ink))),
        )
        .into()
}

fn confirm_card(p: &Pal, s: &Session, lines: &[wright_core::confirm::Line]) -> WidgetNode {
    let (yes, no) = (s.clone(), s.clone());
    let mut rows = vec![
        b("This is exactly what will exist", 17.0, p.ink),
        t("Written from the blueprint by Wright, not by a model.", 12.5, p.muted),
    ];
    rows.extend(lines.iter().take(16).map(|l| plan_line(p, l)));
    rows.push(row(10.0, vec![
        button(p, "Build this", Look::Primary, true, move || yes.answer_confirm(true)),
        button(p, "Change something", Look::Secondary, true, move || no.answer_confirm(false)),
    ]));
    card(p, column(8.0, rows))
}

fn plan_line(p: &Pal, l: &wright_core::confirm::Line) -> WidgetNode {
    let mark = if l.attention { pill("needs care", p.wait, p.wait_soft) } else { Container::new().width(6.0).height(6.0).radius(3.0).color(p.line_strong).into() };
    row(10.0, vec![sized(78.0, 20.0, Align::new(Alignment::CENTER_LEFT).child(mark)), grow(para(l.text.clone(), 14.0, p.ink))])
}

fn plan_card(ui: &Ui, p: &Pal, s: &Session, pl: &super::session::PlanView) -> WidgetNode {
    let (go, change, engine) = (s.clone(), s.clone(), ui.clone());
    let steps: Vec<WidgetNode> = pl
        .stages
        .iter()
        .enumerate()
        .map(|(i, stage)| {
            let names: Vec<String> = stage.iter().map(|c| c.name.clone()).collect();
            let role = stage.iter().map(|c| c.role).find(|r| *r != "agent").unwrap_or("agent");
            let chip = match role {
                "delivers" => pill("asks you first", p.wait, p.wait_soft),
                "live data" => pill("uses the internet", p.wait, p.wait_soft),
                "built-in" => pill("built in", p.muted, p.surface2),
                "helper" => pill("added by Wright", p.muted, p.surface2),
                _ => pill("sandboxed", p.good, p.good_soft),
            };
            row(12.0, vec![
                Container::new().width(24.0).height(24.0).radius(12.0).color(p.accent_soft).alignment(Alignment::CENTER).child(t(format!("{}", i + 1), 12.0, p.accent_ink)).into(),
                fill(names.join("  ·  "), 14.5, p.ink),
                chip,
            ])
        })
        .collect();
    let what: Vec<WidgetNode> = pl.lines.iter().filter(|l| l.attention).take(6).map(|l| plan_line(p, l)).collect();
    let mut items = vec![
        row(10.0, vec![b(pl.name.clone(), 18.0, p.ink), pill(format!("blueprint {}", pl.commit), p.muted, p.surface2), gap()]),
        t(format!("{} steps · never more than ${:.2} a run, ${:.2} a day", pl.stages.len(), pl.per_run, pl.per_day), 13.0, p.muted),
        stretch(8.0, steps),
    ];
    if !what.is_empty() {
        items.push(divider(p));
        items.push(t("Worth a look before you approve", 12.5, p.muted));
        items.extend(what);
    }
    items.push(row(10.0, vec![
        button(p, "Approve & run", Look::Primary, true, move || go.start_run()),
        button(p, "Change something", Look::Secondary, true, move || change.composer.prompt.set(TextEditingValue::new(""))),
        gap(),
        button(p, "See the agents", Look::Quiet, true, move || engine.set_engine(EngineView::Open)),
    ]));
    card(p, stretch(12.0, items))
}

fn state_icon(p: &Pal, st: State, clock: f32) -> WidgetNode {
    match st {
        State::Working => Glyph::spinner(18.0, p.accent_ink, clock * 0.9),
        State::Done => Glyph::Done.draw(18.0, p.good),
        State::Cached => Glyph::Cached.draw(18.0, p.muted),
        State::Waiting => Glyph::Waiting.draw(18.0, p.wait),
        State::Skipped => Glyph::Skipped.draw(18.0, p.muted),
        State::Failed => Glyph::Failed.draw(18.0, p.bad),
        State::Queued => Glyph::Queued.draw(18.0, p.line_strong),
    }
}

/// The state of a step (several agents at once): the most urgent wins.
fn step_state(progress: &Progress, names: &[String]) -> State {
    let states: Vec<State> = names.iter().map(|n| progress.state(n)).collect();
    for s in [State::Failed, State::Waiting, State::Working] {
        if states.contains(&s) {
            return s;
        }
    }
    if states.iter().all(|s| *s == State::Cached) {
        State::Cached
    } else if states.iter().all(|s| s.settled()) {
        if states.iter().all(|s| *s == State::Skipped) { State::Skipped } else { State::Done }
    } else if states.iter().any(|s| s.settled()) {
        State::Working
    } else {
        State::Queued
    }
}

fn run_card(ui: &Ui, p: &Pal, pl: &super::session::PlanView, events: &[wright_core::events::Event], running: bool) -> WidgetNode {
    let progress = Progress::of(events);
    let engine = ui.clone();
    let rows: Vec<WidgetNode> = pl
        .stages
        .iter()
        .map(|stage| {
            let names: Vec<String> = stage.iter().map(|c| c.name.clone()).collect();
            let st = step_state(&progress, &names);
            let secs: f64 = names.iter().map(|n| progress.seconds(n)).fold(0.0, f64::max);
            let meta = match st {
                State::Done => format!("{secs:.1}s"),
                State::Queued => String::new(),
                other => other.label().to_string(),
            };
            let note = names.iter().filter_map(|n| progress.agents.get(n)).map(|(_, note)| note.clone()).next_back().unwrap_or_default();
            let ui2 = ui.clone();
            let icon: WidgetNode = Live::new("step-icon", move || {
                let p = ui2.pal();
                state_icon(&p, st, if st == State::Working { ui2.clock.get() } else { 0.0 })
            })
            .into();
            let label_color = if st == State::Queued { p.muted } else { p.ink };
            row(12.0, vec![
                icon,
                grow(column(2.0, vec![line(names.join("  ·  "), 14.5, label_color), if note.is_empty() || st == State::Queued { SizedBox::shrink().into() } else { line(note, 12.0, p.muted) }])),
                t(meta, 12.5, if st == State::Waiting { p.wait } else { p.muted }),
            ])
        })
        .collect();
    let run_no = events.first().map(|e| e.run).unwrap_or_default();
    card(
        p,
        stretch(10.0, vec![
            row(10.0, vec![
                b(if running { format!("Run {run_no} · running") } else { format!("Run {run_no}") }, 16.0, p.ink),
                t(format!("${:.4} so far", progress.spent as f64 / 1e6), 12.5, p.muted),
                gap(),
                button(p, "Watch the agents", Look::Quiet, true, move || engine.set_engine(EngineView::Open)),
            ]),
            stretch(12.0, rows),
        ]),
    )
}

/// An approval: buttons, or drag the card — right allows, left refuses.
fn approval(p: &Pal, s: &Session, run: wright_core::events::RunId, n: u32, agent: &str, reason: &str) -> WidgetNode {
    const DECIDE: f32 = 120.0;
    let drag = s.card_drag.get();
    let (y, no) = (s.clone(), s.clone());
    let card_body = tinted(
        p.surface,
        p.wait,
        stretch(10.0, vec![
            b(format!("{agent} is asking before it touches the world"), 16.0, p.ink),
            Container::new().color(p.surface2).radius(corner::FIELD).padding(EdgeInsets::all(12.0)).child(para(reason.to_string(), 13.5, p.ink)).into(),
            row(10.0, vec![
                button(p, "Allow", Look::Primary, true, move || y.answer_approval(run, n, true)),
                button(p, "Don't allow", Look::Secondary, true, move || no.answer_approval(run, n, false)),
                gap(),
                t("or drag this card: right allows, left refuses", 12.0, p.muted),
            ]),
        ]),
    );
    let (d1, d2, s2) = (s.card_drag.clone(), s.card_drag.clone(), s.clone());
    let dragged: WidgetNode = GestureDetector::new()
        .on_drag_update(move |d: DragDetails| d1.set(d1.get() + d.delta.dx))
        .on_drag_end(move |_| {
            let x = d2.get();
            if x > DECIDE {
                s2.answer_approval(run, n, true);
            } else if x < -DECIDE {
                s2.answer_approval(run, n, false);
            } else {
                d2.set(0.0);
            }
        })
        .child(shift(drag, 0.0, card_body))
        .into();
    let allow = (drag / DECIDE).clamp(0.0, 1.0);
    let refuse = (-drag / DECIDE).clamp(0.0, 1.0);
    let under = Container::new()
        .color(if drag >= 0.0 { p.good_soft } else { p.surface2 })
        .radius(corner::CARD)
        .padding(EdgeInsets::symmetric(24.0, 0.0))
        .child(row(0.0, vec![fade(allow, b("Allow", 15.0, p.good)), gap(), fade(refuse, b("Don't allow", 15.0, p.muted))]));
    Stack::new().children(vec![layer(under), dragged]).into()
}

fn result_card(ui: &Ui, p: &Pal, s: &Session, r: &super::session::ResultView) -> WidgetNode {
    let (again, folder, engine) = (s.clone(), s.project.clone(), ui.clone());
    let (icon, title) = if r.ok { (Glyph::Done.draw(22.0, p.good), "Delivered and verified") } else { (Glyph::Failed.draw(22.0, p.bad), "Stopped — nothing unverified was delivered") };
    let mut chips = vec![];
    if !r.tests.is_empty() {
        chips.push(pill(first_line(&r.tests, 48), p.good, p.good_soft));
    }
    chips.push(pill(format!("Review: {}", r.verdict), if r.ok { p.good } else { p.bad }, if r.ok { p.good_soft } else { p.surface2 }));
    chips.push(pill("Receipt saved", p.muted, p.surface2));
    let made: Vec<WidgetNode> = r.artifacts.iter().take(6).map(|(name, size)| row(10.0, vec![Glyph::File.draw(15.0, p.muted), fill(name.clone(), 13.5, p.ink), t(human_size(*size), 12.0, p.muted)])).collect();
    let mut items = vec![
        row(12.0, vec![icon, b(title, 18.0, p.ink), gap(), t(format!("run {} · ${:.4}", r.run, r.cost), 12.5, p.muted)]),
        para(r.headline.clone(), 13.5, p.muted),
        row(8.0, chips),
    ];
    if !made.is_empty() {
        items.push(stretch(6.0, made));
    }
    items.push(row(10.0, vec![
        button(p, "Run again", Look::Secondary, true, move || again.start_run()),
        button(p, "Open the folder", Look::Secondary, true, move || super::picker::reveal(&folder)),
        gap(),
        button(p, "How it was built", Look::Quiet, true, move || engine.set_engine(EngineView::Open)),
    ]));
    card(p, stretch(12.0, items))
}

fn first_line(s: &str, n: usize) -> String {
    let l = s.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    if l.chars().count() > n { format!("{}…", l.chars().take(n).collect::<String>()) } else { l.to_string() }
}

pub fn human_size(n: u64) -> String {
    match n {
        0..=1023 => format!("{n} B"),
        1024..=1_048_575 => format!("{:.1} KB", n as f64 / 1024.0),
        _ => format!("{:.1} MB", n as f64 / 1_048_576.0),
    }
}

// ── the composer ─────────────────────────────────────────────────

/// Where you type: `+` (files, folders), the text, the mic, send.
pub fn composer(ui: &Ui, c: &Composer, send: Rc<dyn Fn()>, placeholder: &str, big: bool) -> WidgetNode {
    let p = ui.pal();
    let mut items: Vec<WidgetNode> = Vec::new();

    if c.menu.get() {
        let (u1, u2, c1, c2, close) = (ui.clone(), ui.clone(), c.clone(), c.clone(), c.menu.clone());
        items.push(
            Container::new()
                .color(p.surface)
                .radius(corner::CARD)
                .border(Border::new(p.line_strong, 1.0))
                .padding(EdgeInsets::all(6.0))
                .child(column(2.0, vec![
                    menu_item(&p, Glyph::File, "Add files…", move || u1.pick(&c1, false)),
                    menu_item(&p, Glyph::Folder, "Add a folder…", move || u2.pick(&c2, true)),
                    menu_item(&p, Glyph::Close, "Close", move || close.set(false)),
                ]))
                .into(),
        );
    }

    let files = c.files.get();
    if !files.is_empty() {
        items.push(row(8.0, files.iter().map(|f| file_chip(&p, c, f)).collect()));
    }

    let prompt = c.prompt.clone();
    let submit = send.clone();
    let field = TextField::new(c.prompt.get())
        .show_cursor(ui.is_typing_in(placeholder))
        .placeholder(placeholder.to_string())
        .placeholder_color(p.muted)
        .size(if big { 16.0 } else { 15.0 })
        .color(p.ink)
        .cursor(p.accent_ink, 2.0)
        .selection_color(p.accent_soft)
        .on_changed(Rc::new(move |v: TextEditingValue| prompt.set(v)))
        .on_submit(Rc::new(move |_t: String| submit()));
    let menu = c.menu.clone();
    let plus = icon_button(&p, Glyph::Plus, "Add files or a folder", c.menu.get(), move || menu.set(!menu.get()));
    let mic: WidgetNode = {
        let (ui, c) = (ui.clone(), c.clone());
        Live::new("mic", move || {
            let p = ui.pal();
            match c.voice.get() {
                Voice::Listening => {
                    let level = ui.mic_level.get();
                    let (u, cc) = (ui.clone(), c.clone());
                    row(8.0, vec![meter(&p, level, ui.clock.get()), icon_button(&p, Glyph::Stop, "Stop and transcribe", true, move || u.stop_voice(&cc))])
                }
                Voice::Transcribing => row(6.0, vec![Glyph::spinner(16.0, p.accent_ink, ui.clock.get()), t("Writing it down…", 12.0, p.muted)]),
                _ => {
                    let (u, cc) = (ui.clone(), c.clone());
                    icon_button(&p, Glyph::Mic, "Speak instead of typing", false, move || u.start_voice(&cc))
                }
            }
        })
        .into()
    };
    let go = send.clone();
    let send_btn = Semantics::new()
        .label("Send".to_string())
        .child(
            Pressable::sensed(move |s: Sense| {
                Container::new()
                    .width(34.0)
                    .height(34.0)
                    .radius(17.0)
                    .color(p.accent.lerp(Color::hex(0x000000), 0.15 * s.press))
                    .alignment(Alignment::CENTER)
                    .child(Glyph::Send.draw(16.0, p.on_accent))
                    .into()
            })
            .on_tap(move || go()),
        )
        .into();
    items.push(
        Container::new()
            .color(p.surface)
            .radius(if big { 26.0 } else { 24.0 })
            .border(Border::new(p.line_strong, 1.0))
            .shadow(Shadow::new(Color::rgba(0, 0, 0, 40), Offset::new(0.0, 6.0), 20.0))
            .padding(EdgeInsets::only(8.0, if big { 12.0 } else { 6.0 }, 6.0, if big { 12.0 } else { 6.0 }))
            .child(row(8.0, vec![plus, grow(Container::new().padding(EdgeInsets::symmetric(4.0, 6.0)).child(field)), mic, send_btn]))
            .into(),
    );
    if let Voice::Failed(why) = c.voice.get() {
        items.push(t(why, 12.0, p.wait));
    }
    stretch(8.0, items)
}

fn menu_item(p: &Pal, glyph: Glyph, label: &str, on_tap: impl Fn() + 'static) -> WidgetNode {
    let p = *p;
    let label = label.to_string();
    Pressable::sensed(move |s: Sense| {
        Container::new()
            .color(p.surface.lerp(p.accent_soft, s.hover))
            .radius(corner::SMALL)
            .padding(EdgeInsets::symmetric(10.0, 7.0))
            .child(row(10.0, vec![glyph.draw(15.0, p.ink), t(label.clone(), 13.5, p.ink)]))
            .into()
    })
    .on_tap(on_tap)
    .into()
}

fn file_chip(p: &Pal, c: &Composer, f: &std::path::Path) -> WidgetNode {
    let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| f.display().to_string());
    let glyph = if f.is_dir() { Glyph::Folder } else { Glyph::File };
    let (files, path) = (c.files.clone(), f.to_path_buf());
    Container::new()
        .color(p.surface2)
        .radius(14.0)
        .border(Border::new(p.line, 1.0))
        .padding(EdgeInsets::only(10.0, 4.0, 4.0, 4.0))
        .child(row(6.0, vec![
            glyph.draw(14.0, p.muted),
            t(name, 12.5, p.ink),
            tap(Container::new().width(20.0).height(20.0).alignment(Alignment::CENTER).child(Glyph::Close.draw(12.0, p.muted)), move || files.update(|v| v.retain(|x| x != &path))),
        ]))
        .into()
}

/// Five bars that follow the voice.
fn meter(p: &Pal, level: f32, clock: f32) -> WidgetNode {
    let bars: Vec<WidgetNode> = (0..5)
        .map(|i| {
            let wobble = ((clock * 9.0 + i as f32 * 1.3).sin() * 0.5 + 0.5) * 0.35 + 0.65;
            let h = 4.0 + 18.0 * (level * 3.0).min(1.0) * wobble * [0.6, 0.85, 1.0, 0.85, 0.6][i];
            Container::new().width(3.0).height(h).radius(1.5).color(p.accent_ink).into()
        })
        .collect();
    sized(32.0, 24.0, row(3.0, bars))
}

/// For the home and new-session pages: the status line of each session.
pub fn tone_color(p: &Pal, tone: Tone) -> Color {
    match tone {
        Tone::Quiet => p.muted,
        Tone::Live => p.accent_ink,
        Tone::Wait => p.wait,
        Tone::Good => p.good,
        Tone::Bad => p.bad,
    }
}
