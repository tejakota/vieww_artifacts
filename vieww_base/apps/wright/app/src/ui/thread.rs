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

pub fn page(ui: &Ui, s: &Session) -> WidgetNode {
    let content: WidgetNode = {
        let (ui, s) = (ui.clone(), s.clone());
        Live::new("thread", move || body(&ui, &s)).into()
    };
    let scroller = scroll_area(
        ui,
        &s.scroll,
        pad(
            EdgeInsets::only(24.0, 28.0, 24.0, 150.0),
            chat_column(content),
        ),
    );
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
            let hint = if s.plan.get().is_some() {
                "Ask for a change, e.g. make it weekly"
            } else {
                "Describe what should exist"
            };
            // The reply's own state drives the button: send when idle,
            // stop while the words are on their way. Read here, inside
            // the Live region, so the flip is the frame it happens in.
            let busy = s.pending.get().is_some();
            let stop: Rc<dyn Fn()> = {
                let s = s.clone();
                Rc::new(move || s.stop_reply())
            };
            Container::new()
                .color(p.ground.with_alpha(235))
                .padding(EdgeInsets::only(24.0, 10.0, 24.0, 16.0))
                .child(chat_column(composer(
                    &ui,
                    &s.composer,
                    send,
                    hint,
                    false,
                    busy,
                    stop,
                )))
                .into()
        })
        .into()
    };
    // The jump-to-latest chip: a thread that has scrolled up while a run
    // or a reply pours in is a thread worth a one-tap way back. It is
    // its own Live region on the two scroll signals, so it appears the
    // moment the end is more than a screen away and leaves the moment it
    // is not — the same threshold the eye would call "far from home".
    let jump_node: WidgetNode = {
        let (ui, s) = (ui.clone(), s.clone());
        Live::new("thread-jump", move || {
            let p = ui.pal();
            let (view, content) = s.scroll.extent.get();
            let away = content - view - s.scroll.at();
            if away <= 480.0 {
                return SizedBox::shrink().into();
            }
            let s2 = s.clone();
            let chip = Pressable::sensed(move |sense: Sense| {
                Container::new()
                    .color(p.surface.lerp(p.line_strong, 0.4 * sense.hover))
                    .radius(corner::PILL)
                    .border(Border::new(p.line, 1.0))
                    .shadow(Shadow::new(
                        Color::rgba(0, 0, 0, 50),
                        Offset::new(0.0, 4.0),
                        12.0,
                    ))
                    .padding(EdgeInsets::symmetric(14.0, 7.0))
                    .child(row(
                        7.0,
                        vec![
                            Glyph::Down.draw(13.0, p.accent_ink),
                            t("Jump to the latest", 12.5, p.ink),
                        ],
                    ))
                    .into()
            })
            .on_tap(move || s2.scroll.to_end());
            Align::new(Alignment::BOTTOM_CENTER).child(chip).into()
        })
        .into()
    };
    // The receipt overlay: its own Live region, so opening it rebuilds
    // the mounted tree — a plain `if` at this level is evaluated once at
    // mount and the overlay would never rise.
    let receipt_node: WidgetNode = {
        let (ui, s) = (ui.clone(), s.clone());
        Live::new("thread-receipt", move || {
            if s.receipt_open.get() {
                layer(receipt_overlay(&ui.pal(), &s))
            } else {
                SizedBox::shrink().into()
            }
        })
        .into()
    };
    stack(vec![
        layer(scroller),
        // The chip floats over the thread, above where the composer
        // rises, so it never covers the words it offers to return to.
        Positioned::new()
            .left(0.0)
            .right(0.0)
            .bottom(86.0)
            .height(40.0)
            .child(jump_node)
            .into(),
        Positioned::new()
            .left(0.0)
            .right(0.0)
            .bottom(0.0)
            .child(composer_node)
            .into(),
        receipt_node,
    ])
}

fn body(ui: &Ui, s: &Session) -> WidgetNode {
    let p = ui.pal();
    let mut items: Vec<WidgetNode> = Vec::new();
    let plan = s.plan.get();
    let chat = s.chat.get();
    let request = plan
        .as_ref()
        .map(|pl| pl.request.clone())
        .filter(|r| !r.is_empty())
        .or_else(|| chat.iter().find(|(u, _)| *u).map(|(_, t)| t.clone()));

    match &request {
        Some(r) => items.push(bubble(&p, r)),
        None => items.push(column(10.0, vec![
            Text::new("What should exist?").size(34.0).color(p.ink).weight(FontWeight::Bold).into(),
            para("Describe the outcome in your own words. Wright drafts a plan, shows what it will touch and cost, and runs nothing until you approve.", 15.0, p.muted),
        ])),
    }

    // Drafting: the interview's questions and answers (kept, so a session
    // reopened from Saved shows how its plan came about), then its summary.
    let drafted = if plan.is_none() {
        chat.len()
    } else {
        s.drafted.get().min(chat.len())
    };
    let skip_request = usize::from(
        chat.first()
            .is_some_and(|(u, t)| *u && Some(t) == request.as_ref()),
    );
    for (user, text) in chat[..drafted].iter().skip(skip_request) {
        items.push(if *user {
            bubble(&p, text)
        } else {
            reply(&p, text)
        });
    }
    if plan.is_none() {
        if let Some(lines) = s.confirm.get() {
            items.push(confirm_card(&p, s, &lines));
        }
        if let Some(live) = pending(ui, s) {
            items.push(live);
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
        for (user, text) in chat[drafted..].iter() {
            items.push(if *user {
                bubble(&p, text)
            } else {
                reply(&p, text)
            });
        }
        if let Some(lines) = s.confirm.get() {
            items.push(confirm_card(&p, s, &lines));
        }
        if let Some(live) = pending(ui, s) {
            items.push(live);
        }
    }
    let notice = s.notice.get();
    if !notice.is_empty() {
        items.push(copyable(notice, 13.0, p.wait));
    }
    stretch(16.0, items)
}

/// A reply on its way: the words so far, rendered as they will land
/// (markdown and all, the caret riding the last word), and under them a
/// spinner with what Wright is doing and for how long — so the screen
/// is never still while something is happening.
fn pending(ui: &Ui, s: &Session) -> Option<WidgetNode> {
    let now = s.pending.get()?;
    let mut rows: Vec<WidgetNode> = Vec::new();
    if !now.text.trim().is_empty() {
        // The half-written reply previews exactly as the finished one
        // will: same renderer, same size — plus the caret, so the eye
        // knows it is still being written. Structure appears as it
        // becomes unambiguous (a heading once its line ends, a code
        // block once its fence closes), which is how every editor that
        // renders markdown live does it.
        let mut body = now.text.clone();
        body.push('▌');
        rows.push(markdown(&ui.pal(), body));
    }
    let status: WidgetNode = {
        let (u, since, what) = (ui.clone(), now.since, now.what.clone());
        Live::new("reply-status", move || {
            let p = u.pal();
            let secs = since.elapsed().as_secs();
            let mut parts = vec![
                Glyph::spinner(16.0, p.accent_ink, u.clock.get()),
                t(format!("{what}…"), 13.0, p.muted),
            ];
            if secs >= 2 {
                parts.push(t(format!("{secs}s"), 12.0, p.muted));
            }
            row(8.0, parts)
        })
        .into()
    };
    rows.push(status);
    Some(column(8.0, rows))
}

/// Wright's reply: rendered markdown — the smaller models answer in
/// markdown whether or not anyone asked — and a Copy for all of it,
/// source included.
fn reply(p: &Pal, text: &str) -> WidgetNode {
    column(
        4.0,
        vec![
            markdown(p, text.to_string()),
            copy_icon(p.muted, text.to_string()),
        ],
    )
}

fn bubble(p: &Pal, text: &str) -> WidgetNode {
    Align::new(Alignment::CENTER_RIGHT)
        .child(
            // The cap is a chat app's share of the column (~68% at a
            // default window): room for a long message, while the reply
            // across from it still owns its half of the conversation.
            Constrained::new(Constraints::new(0.0, 720.0, 0.0, f32::INFINITY)).child(
                Container::new()
                    .color(p.bubble)
                    .radius(20.0)
                    .padding(EdgeInsets::symmetric(16.0, 11.0))
                    .child(copyable(text.to_string(), 15.0, p.ink)),
            ),
        )
        .into()
}

fn confirm_card(p: &Pal, s: &Session, lines: &[wright_core::confirm::Line]) -> WidgetNode {
    let (yes, no) = (s.clone(), s.clone());
    let mut rows = vec![
        b("This is exactly what will exist", 17.0, p.ink),
        t(
            "Written from the blueprint by Wright, not by a model.",
            12.5,
            p.muted,
        ),
    ];
    rows.extend(lines.iter().take(16).map(|l| plan_line(p, l)));
    // The card is a glance, not the whole contract: when the summary runs
    // past sixteen lines, say so — every line is one Copy away above.
    if lines.len() > 16 {
        rows.push(t(
            format!(
                "… and {} more line{} — all of them on the Copy button",
                lines.len() - 16,
                if lines.len() - 16 == 1 { "" } else { "s" }
            ),
            12.5,
            p.muted,
        ));
    }
    let all: String = lines
        .iter()
        .map(|l| l.text.trim().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    rows.push(row(
        10.0,
        vec![
            button(p, "Build this", Look::Primary, true, move || {
                yes.answer_confirm(true)
            }),
            button(p, "Change something", Look::Secondary, true, move || {
                no.answer_confirm(false)
            }),
            gap(),
            copy_icon(p.muted, all),
        ],
    ));
    card(p, column(8.0, rows))
}

fn plan_line(p: &Pal, l: &wright_core::confirm::Line) -> WidgetNode {
    // The mark sits in a column at least 84 px wide so the sentences line
    // up — never in a fixed box: a fixed 78×20 slot is what pushed "needs
    // care" onto two lines and out of its pill.
    let mark = if l.attention {
        pill("needs care", p.wait, p.wait_soft)
    } else {
        Container::new()
            .width(6.0)
            .height(6.0)
            .radius(3.0)
            .color(p.line_strong)
            .into()
    };
    Flex::row()
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .spacing(10.0)
        .children(vec![
            pad(
                EdgeInsets::only(0.0, if l.attention { 1.0 } else { 7.0 }, 0.0, 0.0),
                at_least(84.0, mark),
            ),
            // The CLI indents sub-points with spaces; here the mark column
            // already says it, and the spaces would push the first line
            // right of the lines it wraps onto.
            grow(copyable(l.text.trim().to_string(), 14.0, p.ink)),
        ])
        .into()
}

fn plan_card(ui: &Ui, p: &Pal, s: &Session, pl: &super::session::PlanView) -> WidgetNode {
    let (go, change, engine) = (s.clone(), s.clone(), ui.clone());
    let steps: Vec<WidgetNode> = pl
        .stages
        .iter()
        .enumerate()
        .map(|(i, stage)| {
            let names: Vec<String> = stage.iter().map(|c| c.name.clone()).collect();
            let role = stage
                .iter()
                .map(|c| c.role)
                .find(|r| *r != "agent")
                .unwrap_or("agent");
            let chip = match role {
                "delivers" => pill("asks you first", p.wait, p.wait_soft),
                "live data" => pill("uses the internet", p.wait, p.wait_soft),
                "built-in" => pill("built in", p.muted, p.surface2),
                "helper" => pill("added by Wright", p.muted, p.surface2),
                _ => pill("sandboxed", p.good, p.good_soft),
            };
            row(
                12.0,
                vec![
                    Container::new()
                        .width(24.0)
                        .height(24.0)
                        .radius(12.0)
                        .color(p.accent_soft)
                        .alignment(Alignment::CENTER)
                        .child(t(format!("{}", i + 1), 12.0, p.accent_ink))
                        .into(),
                    fill(names.join("  ·  "), 14.5, p.ink),
                    chip,
                ],
            )
        })
        .collect();
    let what: Vec<WidgetNode> = pl
        .lines
        .iter()
        .filter(|l| l.attention)
        .take(6)
        .map(|l| plan_line(p, l))
        .collect();
    let mut items = vec![
        row(
            10.0,
            vec![
                b(pl.name.clone(), 18.0, p.ink),
                pill(format!("blueprint {}", pl.commit), p.muted, p.surface2),
                gap(),
            ],
        ),
        t(
            format!(
                "{} steps · never more than ${:.2} a run, ${:.2} a day",
                pl.stages.len(),
                pl.per_run,
                pl.per_day
            ),
            13.0,
            p.muted,
        ),
        stretch(8.0, steps),
    ];
    if !what.is_empty() {
        items.push(divider(p));
        items.push(t("Worth a look before you approve", 12.5, p.muted));
        items.extend(what);
    }
    items.push(row(
        10.0,
        vec![
            button(p, "Approve & run", Look::Primary, true, move || {
                go.start_run()
            }),
            button(p, "Change something", Look::Secondary, true, move || {
                change.composer.prompt.set(TextEditingValue::new(""))
            }),
            gap(),
            button(p, "See the agents", Look::Quiet, true, move || {
                engine.set_engine(EngineView::Open)
            }),
            button(p, "Agents & tools", Look::Quiet, true, {
                let u = ui.clone();
                move || u.open_agents_and_tools()
            }),
        ],
    ));
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
        if states.iter().all(|s| *s == State::Skipped) {
            State::Skipped
        } else {
            State::Done
        }
    } else if states.iter().any(|s| s.settled()) {
        State::Working
    } else {
        State::Queued
    }
}

fn run_card(
    ui: &Ui,
    p: &Pal,
    pl: &super::session::PlanView,
    events: &[wright_core::events::Event],
    running: bool,
) -> WidgetNode {
    let progress = Progress::of(events);
    let engine = ui.clone();
    let rows: Vec<WidgetNode> = pl
        .stages
        .iter()
        .map(|stage| {
            let names: Vec<String> = stage.iter().map(|c| c.name.clone()).collect();
            let st = step_state(&progress, &names);
            let secs: f64 = names
                .iter()
                .map(|n| progress.seconds(n))
                .fold(0.0, f64::max);
            let meta = match st {
                // A step that took no measurable time says so by saying
                // nothing: "0.0s" eight times down a card is noise the
                // eye has to skip every time it scans.
                State::Done if secs >= 0.05 => format!("{secs:.1}s"),
                State::Done => String::new(),
                State::Queued => String::new(),
                other => other.label().to_string(),
            };
            let note = names
                .iter()
                .filter_map(|n| progress.agents.get(n))
                .map(|(_, note)| note.clone())
                .next_back()
                .unwrap_or_default();
            let ui2 = ui.clone();
            let icon: WidgetNode = Live::new("step-icon", move || {
                let p = ui2.pal();
                state_icon(
                    &p,
                    st,
                    if st == State::Working {
                        ui2.clock.get()
                    } else {
                        0.0
                    },
                )
            })
            .into();
            let label_color = if st == State::Queued { p.muted } else { p.ink };
            row(
                12.0,
                vec![
                    icon,
                    grow(column(
                        2.0,
                        vec![
                            b(names.join("  ·  "), 14.5, label_color),
                            if note.is_empty() || st == State::Queued {
                                SizedBox::shrink().into()
                            } else {
                                line(note, 12.0, p.muted)
                            },
                        ],
                    )),
                    if meta.is_empty() {
                        SizedBox::shrink().into()
                    } else {
                        mono(
                            meta,
                            12.5,
                            if st == State::Waiting {
                                p.wait
                            } else {
                                p.muted
                            },
                        )
                    },
                ],
            )
        })
        .collect();
    let run_no = events.first().map(|e| e.run).unwrap_or_default();
    card(
        p,
        stretch(
            10.0,
            vec![
                row(
                    10.0,
                    vec![
                        b(
                            if running {
                                format!("Run {run_no} · running")
                            } else {
                                format!("Run {run_no}")
                            },
                            16.0,
                            p.ink,
                        ),
                        t(
                            format!("${:.4} so far", progress.spent as f64 / 1e6),
                            12.5,
                            p.muted,
                        ),
                        gap(),
                        button(p, "Watch the agents", Look::Quiet, true, move || {
                            engine.set_engine(EngineView::Open)
                        }),
                    ],
                ),
                stretch(12.0, rows),
            ],
        ),
    )
}

/// An approval: buttons, or drag the card — right allows, left refuses.
fn approval(
    p: &Pal,
    s: &Session,
    run: wright_core::events::RunId,
    n: u32,
    agent: &str,
    reason: &str,
) -> WidgetNode {
    const DECIDE: f32 = 120.0;
    let drag = s.card_drag.get();
    let (y, no) = (s.clone(), s.clone());
    let card_body = tinted(
        p.surface,
        p.wait,
        stretch(
            10.0,
            vec![
                b(
                    format!("{agent} is asking before it touches the world"),
                    16.0,
                    p.ink,
                ),
                Container::new()
                    .color(p.surface2)
                    .radius(corner::FIELD)
                    .padding(EdgeInsets::all(12.0))
                    .child(para(reason.to_string(), 13.5, p.ink))
                    .into(),
                row(
                    10.0,
                    vec![
                        button(p, "Allow", Look::Primary, true, move || {
                            y.answer_approval(run, n, true)
                        }),
                        button(p, "Don't allow", Look::Secondary, true, move || {
                            no.answer_approval(run, n, false)
                        }),
                        gap(),
                        t(
                            "or drag this card: right allows, left refuses",
                            12.0,
                            p.muted,
                        ),
                    ],
                ),
            ],
        ),
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
        .child(row(
            0.0,
            vec![
                fade(allow, b("Allow", 15.0, p.good)),
                gap(),
                fade(refuse, b("Don't allow", 15.0, p.muted)),
            ],
        ));
    Stack::new().children(vec![layer(under), dragged]).into()
}

fn result_card(ui: &Ui, p: &Pal, s: &Session, r: &super::session::ResultView) -> WidgetNode {
    let (again, engine) = (s.clone(), ui.clone());
    // Exports are newest-first. Reveal the directory containing the latest
    // successful artifact, not the source project; before the first export,
    // keep the familiar project-folder behaviour.
    let folder = folder_to_open(&s.project, &s.exports.get());
    let (icon, title) = if r.ok {
        (Glyph::Done.draw(22.0, p.good), "Delivered and verified")
    } else {
        (
            Glyph::Failed.draw(22.0, p.bad),
            "Stopped — nothing unverified was delivered",
        )
    };
    let mut chips = vec![];
    if !r.tests.is_empty() {
        // The one chip whose words vary: it gives way (ellipsis) before
        // the row runs out of the card.
        chips.push(
            Flexible::new(1)
                .fit(FlexFit::Loose)
                .child(pill(first_line(&r.tests, 48), p.good, p.good_soft))
                .into(),
        );
    }
    chips.push(pill(
        format!("Review: {}", r.verdict),
        if r.ok { p.good } else { p.bad },
        if r.ok { p.good_soft } else { p.surface2 },
    ));
    chips.push(pill("Receipt saved", p.muted, p.surface2));
    let made: Vec<WidgetNode> = r
        .artifacts
        .iter()
        .take(6)
        .map(|(name, size)| {
            row(
                10.0,
                vec![
                    Glyph::File.draw(15.0, p.muted),
                    fill(name.clone(), 13.5, p.ink),
                    t(human_size(*size), 12.0, p.muted),
                ],
            )
        })
        .collect();
    // More than six artifacts and the list is a wall; the rest are in
    // the folder "Open the folder" reaches.
    let more_made = r.artifacts.len().saturating_sub(6);
    let mut items = vec![
        row(
            12.0,
            vec![
                icon,
                b(title, 18.0, p.ink),
                gap(),
                t(format!("run {} · ${:.4}", r.run, r.cost), 12.5, p.muted),
            ],
        ),
        copyable(r.headline.clone(), 13.5, p.muted),
        row(8.0, chips),
    ];
    if !made.is_empty() {
        let mut made_items = made;
        if more_made > 0 {
            made_items.push(t(
                format!("… and {more_made} more in the folder"),
                12.5,
                p.muted,
            ));
        }
        items.push(stretch(6.0, made_items));
    }
    // A scheduled plan waits for the person's go-ahead to run unattended.
    // After a passing run is exactly when to offer it.
    if let Some(entry) = wright_engine::scheduler::entry_for(&s.project) {
        let held = wright_engine::scheduler::hold_reason(&entry);
        let trust = wright_engine::unattended::current(&s.project).ok();
        let (go, notice) = (s.project.clone(), s.notice.clone());
        let mut line_items =
            vec![Glyph::Waiting.draw(15.0, if held.is_none() { p.good } else { p.wait })];
        line_items.push(grow(match (&held, &trust) {
            (None, Some(a)) => copyable(wright_engine::unattended::describe(a), 13.0, p.good),
            (Some(why), _) => copyable(
                format!("On its schedule ({} {}): {why}.", entry.cron, entry.tz),
                13.0,
                p.muted,
            ),
            (None, None) => copyable("Runs on its schedule.".to_string(), 13.0, p.muted),
        }));
        if trust.is_none() && r.ok {
            line_items.push(button(
                p,
                "Let it run unattended",
                Look::Primary,
                true,
                move || {
                    notice.set(match wright_engine::unattended::approve(&go, "window") {
                        Ok(a) => wright_engine::unattended::describe(&a),
                        Err(e) => format!("Not yet: {e}"),
                    });
                },
            ));
        }
        items.push(row(10.0, line_items));
    }
    let menu = s.export_menu.clone();
    let receipt = s.receipt_open.clone();
    // Two rows, two registers: what to do with the work, and how to
    // trust it. One row of six buttons did not fit the reading column
    // and overflowed by its last word.
    items.push(row(
        10.0,
        vec![
            button(p, "Run again", Look::Secondary, true, move || {
                again.start_run()
            }),
            button(
                p,
                "Export",
                if s.export_menu.get() {
                    Look::Primary
                } else {
                    Look::Secondary
                },
                s.exporting.get().is_none(),
                move || menu.set(!menu.get()),
            ),
            {
                let (target, notice) = (folder.clone(), s.notice.clone());
                button(p, "Open the folder", Look::Secondary, true, move || {
                    match super::picker::reveal(&target) {
                        Ok(()) => notice.set(String::new()),
                        Err(e) => notice.set(format!("Could not open {}: {e}", target.display())),
                    }
                })
            },
        ],
    ));
    items.push(row(
        10.0,
        vec![
            button(
                p,
                "See the receipt",
                Look::Secondary,
                s.receipt.get().is_some(),
                move || receipt.set(true),
            ),
            button(p, "How it was built", Look::Quiet, true, move || {
                engine.set_engine(EngineView::Open)
            }),
        ],
    ));
    if s.export_menu.get() {
        items.push(export_menu(p, s, r.ok));
    }
    items.extend(export_status(ui, p, s));
    card(p, stretch(12.0, items))
}

/// The receipt, over the page: every check in `wright verify`'s own
/// words, the verdict, and the report as one copyable block — the CLI's
/// evidence inside the window, so trusting a run never means opening a
/// terminal. Esc backs out (the window's key handler closes anything
/// open), and so does the scrim.
fn receipt_overlay(p: &Pal, s: &Session) -> WidgetNode {
    let Some(r) = s.receipt.get() else {
        return SizedBox::shrink().into();
    };
    let close = s.receipt_open.clone();
    let report = r.report.clone();
    let mut items = vec![
        row(
            10.0,
            vec![
                if r.ok {
                    Glyph::Done.draw(22.0, p.good)
                } else {
                    Glyph::Failed.draw(22.0, p.bad)
                },
                b(format!("The receipt · run {}", r.run), 18.0, p.ink),
                gap(),
                t(
                    if r.ok { "verified" } else { "NOT verified" },
                    13.0,
                    if r.ok { p.good } else { p.bad },
                ),
            ],
        ),
        t(
            "Every check over the signed receipt, in verify's own words:",
            13.0,
            p.muted,
        ),
    ];
    for (name, ok, note) in &r.checks {
        items.push(row(
            10.0,
            vec![
                if *ok {
                    Glyph::Done.draw(15.0, p.good)
                } else {
                    Glyph::Failed.draw(15.0, p.bad)
                },
                grow(column(
                    2.0,
                    vec![
                        line(name.clone(), 13.5, p.ink),
                        if note.is_empty() {
                            SizedBox::shrink().into()
                        } else {
                            copyable(note.clone(), 12.5, p.muted)
                        },
                    ],
                )),
            ],
        ));
    }
    if let Some(verdict) = r.report.lines().rev().find(|l| !l.trim().is_empty()) {
        items.push(copyable(
            verdict.to_string(),
            13.0,
            if r.ok { p.good } else { p.bad },
        ));
    }
    items.push(row(
        10.0,
        vec![
            button(p, "Copy the report", Look::Secondary, true, move || {
                super::kit::copy_text(&report);
            }),
            gap(),
            button(p, "Done", Look::Primary, true, move || close.set(false)),
        ],
    ));
    let scrim_close = s.receipt_open.clone();
    stack(vec![
        layer(tap(
            Container::new().color(Color::rgba(0, 0, 0, 110)),
            move || scrim_close.set(false),
        )),
        layer(Center::new().child(sized(620.0, 660.0, card(p, stretch(12.0, items))))),
    ])
}

/// The folder action follows the newest successful export. `exports` is
/// newest-first; a failed attempt does not hide the last artifact that exists.
pub(super) fn folder_to_open(
    project: &std::path::Path,
    exports: &[Result<wright_engine::export::Exported, String>],
) -> std::path::PathBuf {
    exports
        .iter()
        .find_map(|r| {
            r.as_ref()
                .ok()
                .and_then(|e| e.path.parent().map(std::path::Path::to_path_buf))
        })
        .unwrap_or_else(|| project.to_path_buf())
}

/// What this project can be exported as. Packages are offered only with
/// the proof: a run that passed its checks and review.
fn export_menu(p: &Pal, s: &Session, verified: bool) -> WidgetNode {
    use wright_engine::export::Kind;
    let language = s.plan.get().map(|pl| pl.language).unwrap_or_default();
    let mut kinds = vec![Kind::Zip];
    match language.as_str() {
        "python" => kinds.push(Kind::Wheel),
        "rust" => kinds.push(Kind::Crate),
        "react" => kinds.push(Kind::Web),
        _ => {}
    }
    let rows: Vec<WidgetNode> = kinds
        .into_iter()
        .map(|kind| {
            let what = match kind {
                Kind::Zip => "The source, without logs, caches or builds",
                Kind::Wheel => "A .whl that pip installs, with its command",
                Kind::Crate => "A .crate from cargo package",
                Kind::Web => "npm install + npm run build, then dist/ as a .zip",
            };
            let allowed = verified || !kind.needs_proof();
            let why = if allowed {
                what.to_string()
            } else {
                format!("{what} — after a run passes its checks")
            };
            let go = s.clone();
            let p = *p;
            let item = Pressable::sensed(move |st: Sense| {
                Container::new()
                    .color(if allowed {
                        p.surface.lerp(p.accent_soft, st.hover)
                    } else {
                        p.surface
                    })
                    .radius(corner::SMALL)
                    .padding(EdgeInsets::symmetric(12.0, 8.0))
                    .child(row(
                        10.0,
                        vec![
                            Glyph::File.draw(15.0, if allowed { p.ink } else { p.muted }),
                            column(
                                1.0,
                                vec![
                                    t(kind.label(), 13.5, if allowed { p.ink } else { p.muted }),
                                    t(why.clone(), 12.0, p.muted),
                                ],
                            ),
                        ],
                    ))
                    .into()
            });
            if allowed {
                item.on_tap(move || go.export(kind)).into()
            } else {
                item.into()
            }
        })
        .collect();
    Container::new()
        .color(p.surface)
        .radius(corner::CARD)
        .border(Border::new(p.line_strong, 1.0))
        .padding(EdgeInsets::all(6.0))
        .child(stretch(2.0, rows))
        .into()
}

/// The export in progress, then each result: where it went, or why not.
fn export_status(ui: &Ui, p: &Pal, s: &Session) -> Vec<WidgetNode> {
    let mut out = Vec::new();
    if let Some(kind) = s.exporting.get() {
        let u = ui.clone();
        let spin: WidgetNode = Live::new("export-spin", move || {
            Glyph::spinner(16.0, u.pal().accent_ink, u.clock.get())
        })
        .into();
        out.push(row(
            8.0,
            vec![
                spin,
                t(
                    format!("Making the {}…", kind.label().to_lowercase()),
                    13.0,
                    p.muted,
                ),
            ],
        ));
    }
    for r in s.exports.get() {
        out.push(match r {
            Ok(e) => {
                let dir = e
                    .path
                    .parent()
                    .map(std::path::Path::to_path_buf)
                    .unwrap_or_default();
                let name = e
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                row(
                    10.0,
                    vec![
                        Glyph::Done.draw(16.0, p.good),
                        fill(format!("{name} · {}", human_size(e.bytes)), 13.0, p.ink),
                        {
                            let notice = s.notice.clone();
                            button(p, "Show in folder", Look::Quiet, true, move || {
                                match super::picker::reveal(&dir) {
                                    Ok(()) => notice.set(String::new()),
                                    Err(e) => {
                                        notice.set(format!("Could not open {}: {e}", dir.display()))
                                    }
                                }
                            })
                        },
                    ],
                )
            }
            Err(why) => row(
                10.0,
                vec![
                    Glyph::Failed.draw(16.0, p.bad),
                    grow(copyable(why, 12.5, p.bad)),
                ],
            ),
        });
    }
    out
}

fn first_line(s: &str, n: usize) -> String {
    let l = s
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    if l.chars().count() > n {
        format!("{}…", l.chars().take(n).collect::<String>())
    } else {
        l.to_string()
    }
}

pub fn human_size(n: u64) -> String {
    match n {
        0..=1023 => format!("{n} B"),
        1024..=1_048_575 => format!("{:.1} KB", n as f64 / 1024.0),
        _ => format!("{:.1} MB", n as f64 / 1_048_576.0),
    }
}

// ── the composer ─────────────────────────────────────────────────

/// Where you type: `+` (files, folders), the text, the mic, and send —
/// which becomes stop while a reply is on its way. The same circle in
/// the same place does the opposite thing at the opposite moment: the
/// button you know is the button that interrupts, with no hunt for a
/// separate control while words pour in.
pub fn composer(
    ui: &Ui,
    c: &Composer,
    send: Rc<dyn Fn()>,
    placeholder: &str,
    big: bool,
    busy: bool,
    stop: Rc<dyn Fn()>,
) -> WidgetNode {
    let p = ui.pal();
    let mut items: Vec<WidgetNode> = Vec::new();

    if c.menu.get() {
        let (u1, u2, c1, c2, close) =
            (ui.clone(), ui.clone(), c.clone(), c.clone(), c.menu.clone());
        items.push(
            Container::new()
                .color(p.surface)
                .radius(corner::CARD)
                .border(Border::new(p.line_strong, 1.0))
                .padding(EdgeInsets::all(6.0))
                .child(column(
                    2.0,
                    vec![
                        menu_item(&p, Glyph::File, "Add files…", move || u1.pick(&c1, false)),
                        menu_item(&p, Glyph::Folder, "Add a folder…", move || {
                            u2.pick(&c2, true)
                        }),
                        menu_item(&p, Glyph::Close, "Close", move || close.set(false)),
                    ],
                ))
                .into(),
        );
    }

    let files = c.files.get();
    if !files.is_empty() {
        items.push(row(
            8.0,
            files.iter().map(|f| file_chip(&p, c, f)).collect(),
        ));
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
    let plus = icon_button(
        &p,
        Glyph::Plus,
        "Add files or a folder",
        c.menu.get(),
        move || menu.set(!menu.get()),
    );
    let mic: WidgetNode = {
        let (ui, c) = (ui.clone(), c.clone());
        Live::new("mic", move || {
            let p = ui.pal();
            match c.voice.get() {
                Voice::Listening => {
                    let level = ui.mic_level.get();
                    let (u, cc) = (ui.clone(), c.clone());
                    row(
                        8.0,
                        vec![
                            meter(&p, level, ui.clock.get()),
                            icon_button(&p, Glyph::Stop, "Stop and transcribe", true, move || {
                                u.stop_voice(&cc)
                            }),
                        ],
                    )
                }
                Voice::Transcribing => row(
                    6.0,
                    vec![
                        Glyph::spinner(16.0, p.accent_ink, ui.clock.get()),
                        t("Writing it down…", 12.0, p.muted),
                    ],
                ),
                _ => {
                    let (u, cc) = (ui.clone(), c.clone());
                    icon_button(
                        &p,
                        Glyph::Mic,
                        "Speak instead of typing",
                        false,
                        move || u.start_voice(&cc),
                    )
                }
            }
        })
        .into()
    };
    let go = send.clone();
    let send_btn: WidgetNode = Semantics::new()
        .label(if busy { "Stop" } else { "Send" }.to_string())
        .child(
            Pressable::sensed(move |s: Sense| {
                Container::new()
                    .width(34.0)
                    .height(34.0)
                    .radius(17.0)
                    // While the reply streams, the circle breathes with
                    // the work: pressed-dark on touch, a hair quieter
                    // than the send blue so the state reads at a glance.
                    // Send and stop are the same action surface in two states;
                    // keep the same blue so the control remains easy to find.
                    .color(p.accent.lerp(Color::hex(0x000000), 0.15 * s.press))
                    .alignment(Alignment::CENTER)
                    .child(if busy {
                        Glyph::Stop.draw(13.0, Color::WHITE)
                    } else {
                        Glyph::Send.draw(16.0, p.on_accent)
                    })
                    .into()
            })
            .on_tap(move || {
                if busy {
                    stop();
                } else {
                    go();
                }
            }),
        )
        .into();
    items.push(
        Container::new()
            .color(p.surface)
            .radius(if big { 26.0 } else { 24.0 })
            .border(Border::new(p.line_strong, 1.0))
            .shadow(Shadow::new(
                Color::rgba(0, 0, 0, 40),
                Offset::new(0.0, 6.0),
                20.0,
            ))
            .padding(EdgeInsets::only(
                8.0,
                if big { 12.0 } else { 6.0 },
                6.0,
                if big { 12.0 } else { 6.0 },
            ))
            .child(row(
                8.0,
                vec![
                    plus,
                    grow(
                        Container::new()
                            .padding(EdgeInsets::symmetric(4.0, 6.0))
                            .child(field),
                    ),
                    mic,
                    send_btn,
                ],
            ))
            .into(),
    );
    if let Voice::Failed(why) = c.voice.get() {
        items.push(copyable(why, 12.0, p.wait));
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
            .child(row(
                10.0,
                vec![glyph.draw(15.0, p.ink), t(label.clone(), 13.5, p.ink)],
            ))
            .into()
    })
    .on_tap(on_tap)
    .into()
}

fn file_chip(p: &Pal, c: &Composer, f: &std::path::Path) -> WidgetNode {
    let name = f
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| f.display().to_string());
    let glyph = if f.is_dir() {
        Glyph::Folder
    } else {
        Glyph::File
    };
    let (files, path) = (c.files.clone(), f.to_path_buf());
    Container::new()
        .color(p.surface2)
        .radius(14.0)
        .border(Border::new(p.line, 1.0))
        .padding(EdgeInsets::only(10.0, 4.0, 4.0, 4.0))
        .child(row(
            6.0,
            vec![
                glyph.draw(14.0, p.muted),
                t(name, 12.5, p.ink),
                tap(
                    Container::new()
                        .width(20.0)
                        .height(20.0)
                        .alignment(Alignment::CENTER)
                        .child(Glyph::Close.draw(12.0, p.muted)),
                    move || files.update(|v| v.retain(|x| x != &path)),
                ),
            ],
        ))
        .into()
}

/// Five bars that follow the voice.
pub(crate) fn meter(p: &Pal, level: f32, clock: f32) -> WidgetNode {
    let bars: Vec<WidgetNode> = (0..5)
        .map(|i| {
            let wobble = ((clock * 9.0 + i as f32 * 1.3).sin() * 0.5 + 0.5) * 0.35 + 0.65;
            let h = 4.0 + 18.0 * (level * 3.0).min(1.0) * wobble * [0.6, 0.85, 1.0, 0.85, 0.6][i];
            Container::new()
                .width(3.0)
                .height(h)
                .radius(1.5)
                .color(p.accent_ink)
                .into()
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
