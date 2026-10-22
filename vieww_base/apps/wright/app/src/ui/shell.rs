//! The frame around the pages: splash, top bar, the stage (the row of
//! pages, the overview, the engine layer), the status bar, and overlays
//! (Settings, gestures help, the toast).

use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use vieww::foundation::{KeyEvent, ScrollEvent};
use vieww::prelude::*;
use vieww::render::FrameDriver;

use super::kit::*;
use super::logo::{self, Mark};
use super::session::Tone;
use super::theme::{Mode, Pal};
use super::thread::tone_color;
use super::{engine, home, settings, EngineView, Ui};

/// How far apart the pages sit in the overview (at full size).
const OVERVIEW_GAP: f32 = 120.0;

pub fn register(driver: &mut FrameDriver) {
    driver.register::<Keys, RenderKeys>(|w| RenderKeys {
        handle: Rc::clone(&w.handle),
    });
}

pub fn root(ui: &Ui) -> WidgetNode {
    let handle: Rc<dyn Fn(&KeyEvent) -> bool> = {
        let ui = ui.clone();
        Rc::new(move |e: &KeyEvent| ui.handle_key(e))
    };
    let app = app(ui);
    let ui2 = ui.clone();
    let body: WidgetNode = LayoutBuilder::new(move |c: Constraints| {
        ui2.size.set(Size::new(c.max_width, c.max_height));
        let (ui3, app) = (ui2.clone(), app.clone());
        Live::new("root", move || frame(&ui3, &app)).into()
    })
    .into();
    Keys {
        handle,
        child: body,
    }
    .into()
}

/// The watermark's side: the mark, large, behind everything.
fn watermark_side(size: Size) -> f32 {
    (size.width.min(size.height) * 1.25).min(1500.0)
}

fn frame(ui: &Ui, app: &WidgetNode) -> WidgetNode {
    let p = ui.pal();
    let size = ui.size.get();
    let side = watermark_side(size);
    let ground = layer(Container::new().color(p.ground));
    if ui.splash.get() {
        let app = app.clone();
        return Animated::new(1.0)
            .from(0.0)
            .duration(Duration::from_millis(3200))
            .curve(Curve::Linear)
            .build(move |t| splash_frame(&p, side, t, &app))
            .into();
    }
    let mark = Mark {
        color: p.accent.with_alpha(p.watermark),
        progress: 1.0,
        soft: 1.0,
    };
    // A repaint boundary: the blurred mark is recorded once and reused, so
    // nothing animating above it pays for the blur again.
    let watermark = RepaintBoundary::new().child(Center::new().child(mark.widget(side)));
    stack(vec![ground, layer(watermark), layer(app.clone())])
}

/// The opening, as a function of time `t` (0..1 over 3.2 s):
///
/// ```text
///  0.00-0.34  the W draws itself
///  0.34-0.60  "wright" fades in and out beneath it
///  0.58-1.00  the W grows to the watermark's size, softens and pales,
///             the veil lifts, and the app rises in front of it
/// ```
fn splash_frame(p: &Pal, side: f32, t: f32, app: &WidgetNode) -> WidgetNode {
    let draw = ease(phase(t, 0.02, 0.34));
    let word = phase(t, 0.34, 0.44) * (1.0 - phase(t, 0.54, 0.60));
    let z = ease(phase(t, 0.58, 1.0));
    let alpha = mix(255.0, p.watermark as f32, z) as u8;
    let mark = Mark {
        color: p.accent.with_alpha(alpha),
        progress: draw,
        soft: z,
    };
    stack(vec![
        layer(Container::new().color(p.ground)),
        layer(fade(phase(t, 0.72, 1.0), app.clone())),
        layer(fade(1.0 - z, Container::new().color(p.ground))),
        layer(Center::new().child(mark.widget(mix(112.0, side, z)))),
        layer(
            Center::new().child(shift(
                0.0,
                92.0,
                fade(
                    word,
                    Text::new("wright")
                        .size(24.0)
                        .color(p.ink)
                        .weight(FontWeight::Bold),
                ),
            )),
        ),
    ])
}

fn app(ui: &Ui) -> WidgetNode {
    let column: WidgetNode = Flex::column()
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .children(vec![
            top_bar(ui),
            stop_banner(ui),
            grow(stage(ui)),
            status_bar(ui),
        ])
        .into();
    let overlays: WidgetNode = {
        let ui = ui.clone();
        Live::new("overlays", move || overlays(&ui)).into()
    };
    stack(vec![layer(column), layer(overlays)])
}

// ── top bar ──────────────────────────────────────────────────────

/// The bar under the window's title bar: the page title, the session dots,
/// the view toggles. The dots own the bar's exact middle — the title and the
/// toggles sit on symmetric flanks either side of them, so the pager reads
/// from the window's centre line. The brand is not on the bar — the name and
/// the mark live on the title bar itself, one strip up (the window's icon and
/// title text, beside the minimise and close buttons), and a second "wright"
/// directly under that read as a stutter. `pub(crate)` for the test that
/// holds this bar to it.
pub(crate) fn top_bar(ui: &Ui) -> WidgetNode {
    let ui = ui.clone();
    Live::new("top-bar", move || {
        let p = ui.pal();
        let sessions = ui.sessions.get();
        let page = ui.page.get();
        let title = match page {
            0 => "Home".to_string(),
            n if n <= sessions.len() => sessions[n - 1].title(),
            _ => "New session".to_string(),
        };
        let dots: Vec<WidgetNode> = (0..sessions.len() + 2)
            .map(|i| {
                let go = ui.clone();
                let on = i == page;
                let fresh = i == sessions.len() + 1;
                let dot = Container::new()
                    .width(if on { 20.0 } else { 7.0 })
                    .height(7.0)
                    .radius(3.5)
                    .color(if on {
                        p.accent
                    } else if fresh {
                        Color::rgba(0, 0, 0, 0)
                    } else {
                        p.line_strong
                    })
                    .border(Border::new(
                        if fresh && !on {
                            p.line_strong
                        } else {
                            Color::rgba(0, 0, 0, 0)
                        },
                        1.5,
                    ));
                tap(
                    Container::new()
                        .padding(EdgeInsets::symmetric(3.0, 8.0))
                        .child(dot),
                    move || {
                        go.set_overview(false);
                        go.go(i);
                    },
                )
            })
            .collect();
        let live = ui.current_session().map(|s| s.live_agents()).unwrap_or(0);
        let engine_on = ui.engine.get() != EngineView::Hidden;
        let overview_on = ui.overview.get();
        let (a, b2, c, d) = (ui.clone(), ui.clone(), ui.clone(), ui.clone());
        Container::new()
            .padding(EdgeInsets::symmetric(16.0, 10.0))
            .child(row(
                0.0,
                vec![
                    // Equal flanks: whatever the title weighs on the left and
                    // the toggles on the right, the dots keep the middle.
                    grow(
                        SizedBox::width(300.0).child(
                            Align::new(Alignment::CENTER_LEFT).child(line(title, 13.5, p.ink)),
                        ),
                    ),
                    row(0.0, dots),
                    grow(Align::new(Alignment::CENTER_RIGHT).child(row(
                        12.0,
                        vec![
                            tool_button(
                                &p,
                                Glyph::Sessions,
                                "All sessions",
                                None,
                                overview_on,
                                move || a.set_overview(!a.overview.get()),
                            ),
                            tool_button(
                                &p,
                                Glyph::Engine,
                                "Engine",
                                if live > 0 {
                                    Some(format!("{live} live"))
                                } else {
                                    None
                                },
                                engine_on,
                                move || {
                                    b2.set_engine(if b2.engine.get() == EngineView::Hidden {
                                        EngineView::Open
                                    } else {
                                        EngineView::Hidden
                                    })
                                },
                            ),
                            icon_button(&p, Glyph::Gear, "Settings", false, move || {
                                c.open_settings()
                            }),
                            icon_button(
                                &p,
                                Glyph::Help,
                                "Gestures and shortcuts",
                                false,
                                move || d.help.set(!d.help.get()),
                            ),
                        ],
                    ))),
                ],
            ))
            .into()
    })
    .into()
}

/// While the kill switch is pulled, a band across the window says so —
/// whoever pulled it, from wherever — with the one button that lifts it.
fn stop_banner(ui: &Ui) -> WidgetNode {
    let ui = ui.clone();
    Live::new("stop-banner", move || {
        let Some(reason) = ui.stopped.get() else {
            return SizedBox::shrink().into();
        };
        let p = ui.pal();
        let go = ui.clone();
        Container::new()
            .color(p.bad.with_alpha(40))
            .border(Border::new(p.bad, 1.0))
            .padding(EdgeInsets::symmetric(18.0, 8.0))
            .child(row(12.0, vec![
                Glyph::Stop.draw(16.0, p.bad),
                b("Everything is stopped", 13.5, p.ink),
                fill(format!("{reason} · nothing runs, sends or fetches on this computer until you start again"), 13.0, p.muted),
                button(&p, "Start again", Look::Primary, true, move || go.start_again()),
            ]))
            .into()
    })
    .into()
}

// ── status bar ───────────────────────────────────────────────────

fn status_bar(ui: &Ui) -> WidgetNode {
    let ui = ui.clone();
    Live::new("status-bar", move || {
        let p = ui.pal();
        let (where_, status, tone, spent) = match ui.current_session() {
            Some(s) => {
                let (st, tone) = s.status();
                (s.title(), st, tone, Some(s.spent_today()))
            }
            None if ui.page.get() == 0 => (
                "Home".into(),
                format!(
                    "{} · {} saved",
                    plural(ui.sessions.get().len(), "session"),
                    ui.saved.get().len()
                ),
                Tone::Quiet,
                None,
            ),
            None => (
                "New session".into(),
                "Describe what should exist".into(),
                Tone::Quiet,
                None,
            ),
        };
        let dot: WidgetNode = {
            let u = ui.clone();
            Live::new("status-dot", move || {
                let p = u.pal();
                let pulse = if tone == Tone::Live {
                    (u.clock.get() * 3.2).sin() * 0.5 + 0.5
                } else {
                    1.0
                };
                Container::new()
                    .width(7.0)
                    .height(7.0)
                    .radius(3.5)
                    .color(tone_color(&p, tone).with_alpha((110.0 + 145.0 * pulse) as u8))
                    .into()
            })
            .into()
        };
        let mode = ui.mode.get();
        let (flip, gear) = (ui.clone(), ui.clone());
        let mut items = vec![
            Container::new()
                .color(p.accent)
                .padding(EdgeInsets::symmetric(12.0, 0.0))
                .child(Center::new().child(row(
                    3.0,
                    vec![
                        Mark::solid(p.on_accent).widget(13.0),
                        b("wright", 12.0, p.on_accent),
                    ],
                )))
                .into(),
            line(where_, 12.0, p.ink),
            dot,
            fill(status, 12.0, p.muted),
        ];
        if let Some(micro) = spent {
            items.push(t(
                format!("${:.4} today", micro as f64 / 1e6),
                12.0,
                p.muted,
            ));
        }
        if ui.stopped.get().is_none() {
            let stop = ui.clone();
            items.push(status_button(
                &p,
                Glyph::Stop,
                "Stop everything",
                move || stop.stop_everything("stopped from the window"),
            ));
        }
        items.push(status_button(
            &p,
            Glyph::Theme,
            if mode == Mode::Dark { "Dark" } else { "Light" },
            move || flip.set_look(flip.mode.get().flip(), flip.accent.get()),
        ));
        items.push(status_button(&p, Glyph::Gear, "Settings", move || {
            gear.open_settings()
        }));
        Container::new()
            .height(26.0)
            .color(p.status)
            .border(Border::new(p.line, 1.0))
            .child(
                Flex::row()
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .spacing(12.0)
                    .children(
                        items
                            .into_iter()
                            .map(|w| Align::new(Alignment::CENTER_LEFT).child(w).into())
                            .collect::<Vec<WidgetNode>>(),
                    ),
            )
            .into()
    })
    .into()
}

fn status_button(p: &Pal, glyph: Glyph, label: &str, on_tap: impl Fn() + 'static) -> WidgetNode {
    let p = *p;
    let label = label.to_string();
    Pressable::sensed(move |s: Sense| {
        Container::new()
            .color(p.status.lerp(p.surface2, s.hover))
            .radius(corner::SMALL)
            .padding(EdgeInsets::symmetric(8.0, 3.0))
            .child(row(
                5.0,
                vec![glyph.draw(12.0, p.muted), t(label.clone(), 12.0, p.muted)],
            ))
            .into()
    })
    .on_tap(on_tap)
    .into()
}

// ── the stage ────────────────────────────────────────────────────

fn stage(ui: &Ui) -> WidgetNode {
    let ui = ui.clone();
    let wheel = ui.clone();
    let body: WidgetNode = LayoutBuilder::new(move |c: Constraints| {
        let ui = ui.clone();
        let (w, h) = (c.max_width, c.max_height);
        Live::new("stage", move || {
            let pages = ui.pages();
            let page = ui.page.get() as f32;
            let drag = ui.drag.get();
            let dragging = ui.dragging.get();
            let overview = if ui.overview.get() { 1.0 } else { 0.0 };
            let engine_on = if ui.engine.get() == EngineView::Hidden {
                0.0
            } else {
                1.0
            };
            let track: WidgetNode = {
                let ui = ui.clone();
                glide(overview, 520, false, move |o| {
                    let (ui, pages) = (ui.clone(), pages.clone());
                    glide(-page * w + drag, 460, dragging, move |x| {
                        track(&ui, &pages, w, h, o, x)
                    })
                })
            };
            let ui2 = ui.clone();
            glide(engine_on, 300, false, move |e| {
                // Behind the agents, the page goes soft: blurred and dimmed,
                // so the boxes read cleanly instead of word-over-word with the
                // conversation underneath. The blur grows with the fade-in.
                let page: WidgetNode = Transformed::new(Transform::scale_around(
                    Offset::new(w / 2.0, h / 2.0),
                    1.0 - 0.03 * e,
                    1.0 - 0.03 * e,
                ))
                .child(track.clone())
                .into();
                let page: WidgetNode = if e > 0.01 {
                    vieww::widget::Filtered::blur(10.0 * e).child(page).into()
                } else {
                    page
                };
                let mut layers = vec![layer(fade(1.0 - 0.88 * e, page))];
                if e > 0.01 {
                    layers.push(layer(fade(
                        e,
                        shift(0.0, 16.0 * (1.0 - e), engine::layer(&ui2)),
                    )));
                }
                Clip::rect().child(stack(layers)).into()
            })
        })
        .into()
    })
    .into();
    GestureDetector::new()
        .on_scroll(move |e: ScrollEvent| wheel.on_wheel(e))
        .child(body)
        .into()
}

/// Every page at once, blended between the row (`o == 0`) and the
/// overview (`o == 1`); `x` is the row's offset.
///
/// The overview draws each page as a miniature card rather than shrinking
/// the live page: a vieww scroll view keeps its own layer, and layers do not
/// take a parent's scale yet, so a shrunk page would leave its scrolling
/// part drawn full size. The live row fades out as the cards fly in from it.
fn track(ui: &Ui, pages: &[WidgetNode], w: f32, h: f32, o: f32, x: f32) -> WidgetNode {
    let p = ui.pal();
    let n = pages.len();
    let current = ui.page.get();
    let sessions = ui.sessions.get();
    let mut layers = Vec::new();

    let live = (1.0 - o * 2.5).clamp(0.0, 1.0);
    if live > 0.0 {
        for (i, page) in pages.iter().enumerate() {
            let left = i as f32 * w + x;
            if left > w || left + w < 0.0 {
                continue; // off screen: nothing to draw
            }
            layers.push(at(
                left,
                0.0,
                w,
                h,
                fade(live, inert(o > 0.01 || i != current, page.clone())),
            ));
        }
    }
    if o <= 0.01 {
        return Stack::new().children(layers).into();
    }

    let total = n as f32 * w + (n as f32 - 1.0) * OVERVIEW_GAP;
    let s = (0.92 * w / total).min(0.30);
    let (tx, ty) = ((w - total * s) / 2.0, (h - h * s) / 2.0 - 30.0);
    for i in 0..n {
        // Each card flies from where its page is in the row to its place.
        let from = (i as f32 * w + x, 0.0, w, h);
        let to = (tx + i as f32 * (w + OVERVIEW_GAP) * s, ty, w * s, h * s);
        let k = ease(o);
        let (left, top, cw, ch) = (
            mix(from.0, to.0, k),
            mix(from.1, to.1, k),
            mix(from.2, to.2, k),
            mix(from.3, to.3, k),
        );
        let on = i == current;
        let card = Container::new()
            .color(p.ground)
            .radius(corner::PANEL)
            .border(Border::new(
                if on { p.accent } else { p.line_strong },
                if on { 2.5 } else { 1.0 },
            ))
            .child(Clip::rounded(corner::PANEL).child(mini(ui, &p, i, k > 0.85)));
        let go = ui.clone();
        // Sized explicitly: a tap target loosens its child's constraints,
        // and a card of only layered children would shrink to nothing.
        layers.push(at(
            left,
            top,
            cw,
            ch,
            fade(
                o.min(1.0),
                tap(sized(cw, ch, card), move || {
                    go.set_overview(false);
                    go.go(i);
                }),
            ),
        ));
        let title = match i {
            0 => "Home".to_string(),
            k if k <= sessions.len() => sessions[k - 1].title(),
            _ => "New session".to_string(),
        };
        let mut caption = vec![fill(title, 13.0, if on { p.ink } else { p.muted })];
        if i >= 1 && i <= sessions.len() {
            let close = ui.clone();
            caption.push(icon_button(
                &p,
                Glyph::Close,
                "Close into Saved",
                false,
                move || close.close_session(i - 1),
            ));
        }
        layers.push(at(
            left,
            top + ch + 8.0,
            cw,
            30.0,
            fade(phase(o, 0.6, 1.0), row(6.0, caption)),
        ));
    }
    let saved = ui.saved.get();
    if !saved.is_empty() {
        let shelf = row(
            12.0,
            vec![b("SAVED", 11.0, p.muted), home::saved_chips(ui, &saved)],
        );
        layers.push(at(
            0.0,
            h - 70.0,
            w,
            60.0,
            fade(phase(o, 0.6, 1.0), Center::new().child(shelf)),
        ));
    }
    layers.push(at(
        0.0,
        18.0,
        w,
        24.0,
        fade(
            phase(o, 0.6, 1.0),
            Center::new().child(t(
                "Click a session to open it · × closes it into Saved · Esc goes back",
                12.5,
                p.muted,
            )),
        ),
    ));
    Stack::new().children(layers).into()
}

/// A page in miniature: what it is and where it stands, at a glance.
fn mini(ui: &Ui, p: &Pal, i: usize, words: bool) -> WidgetNode {
    let sessions = ui.sessions.get();
    let mark = Mark {
        color: p.accent.with_alpha(p.watermark),
        progress: 1.0,
        soft: 0.6,
    };
    if !words {
        // In flight: just the mark, so nothing has to fit a moving box.
        return Center::new().child(mark.widget(150.0)).into();
    }
    let body: WidgetNode = match i {
        0 => column(
            8.0,
            vec![
                b("What should exist?", 16.0, p.ink),
                Container::new()
                    .height(26.0)
                    .color(p.surface)
                    .radius(13.0)
                    .border(Border::new(p.line_strong, 1.0))
                    .into(),
                t(
                    format!(
                        "{} · {} saved",
                        plural(sessions.len(), "session"),
                        ui.saved.get().len()
                    ),
                    11.0,
                    p.muted,
                ),
            ],
        ),
        k if k <= sessions.len() => {
            let s = &sessions[k - 1];
            let (status, tone) = s.status();
            let request = s.plan.get().map(|pl| pl.request).unwrap_or_else(|| {
                s.chat
                    .get()
                    .first()
                    .map(|c| c.1.clone())
                    .unwrap_or_default()
            });
            let share = s.done_share();
            column(
                8.0,
                vec![
                    line(s.title(), 15.0, p.ink),
                    Container::new()
                        .color(p.bubble)
                        .radius(corner::FIELD)
                        .padding(EdgeInsets::symmetric(9.0, 5.0))
                        .child(Text::new(request).size(10.5).color(p.ink).max_lines(2))
                        .into(),
                    line(status, 11.0, tone_color(p, tone)),
                    Container::new()
                        .height(3.0)
                        .radius(1.5)
                        .color(p.surface2)
                        .alignment(Alignment::CENTER_LEFT)
                        .child(
                            Container::new()
                                .width(160.0 * share)
                                .height(3.0)
                                .radius(1.5)
                                .color(tone_color(p, tone)),
                        )
                        .into(),
                ],
            )
        }
        _ => column(
            8.0,
            vec![
                Glyph::Plus.draw(22.0, p.muted),
                b("A new session", 14.0, p.ink),
                t("Describe what should exist", 11.0, p.muted),
            ],
        ),
    };
    // The mark sits behind the text as a non-layered child, so the card's
    // size comes from its own constraints (a stack of only layered children
    // can be laid out at zero size for a frame, and its text overflows).
    Stack::new()
        .fit(StackFit::Expand)
        .children(vec![
            Center::new().child(mark.widget(150.0)).into(),
            pad(
                EdgeInsets::all(14.0),
                Align::new(Alignment::TOP_LEFT).child(body),
            ),
        ])
        .into()
}

// ── overlays ─────────────────────────────────────────────────────

fn scrim(ui: &Ui, close: impl Fn() + 'static) -> WidgetNode {
    let _ = ui;
    layer(tap(
        Container::new().color(Color::rgba(0, 0, 0, 110)),
        close,
    ))
}

fn overlays(ui: &Ui) -> WidgetNode {
    let p = ui.pal();
    let size = ui.size.get();
    let mut layers: Vec<WidgetNode> = Vec::new();
    if ui.settings.get() {
        let close = ui.settings.clone();
        layers.push(scrim(ui, move || close.set(false)));
        layers.push(layer(Center::new().child(sized(
            (size.width - 80.0).min(1000.0),
            (size.height - 100.0).min(720.0),
            settings::page(ui),
        ))));
    }
    if ui.help.get() {
        let close = ui.help.clone();
        layers.push(scrim(ui, move || close.set(false)));
        layers.push(layer(Center::new().child(sized(
            520.0,
            470.0,
            help(ui, &p),
        ))));
    }
    if let Some(toast) = ui.toast.get() {
        let mut items = vec![t(toast.text.clone(), 13.0, p.ground)];
        if let Some(path) = toast.undo.clone() {
            let u = ui.clone();
            items.push(button(&p, "Undo", Look::Primary, true, move || {
                u.reopen(&path)
            }));
        }
        let close = ui.toast.clone();
        items.push(tap(Glyph::Close.draw(14.0, p.ground), move || {
            close.set(None)
        }));
        layers.push(
            Positioned::new()
                .left(0.0)
                .right(0.0)
                .bottom(44.0)
                .child(
                    Center::new().child(
                        Container::new()
                            .color(p.ink)
                            .radius(corner::PILL)
                            .padding(EdgeInsets::only(18.0, 6.0, 12.0, 6.0))
                            .child(row(12.0, items)),
                    ),
                )
                .into(),
        );
    }
    if layers.is_empty() {
        return inert(true, SizedBox::shrink());
    }
    stack(layers)
}

fn help(ui: &Ui, p: &Pal) -> WidgetNode {
    let close = ui.help.clone();
    let replay = ui.clone();
    let list = ui.clone();
    let entry = |k: &str, v: &str| -> WidgetNode {
        row(
            14.0,
            vec![
                SizedBox::width(150.0)
                    .child(b(k.to_string(), 13.5, p.ink))
                    .into(),
                grow(para(v.to_string(), 13.5, p.muted)),
            ],
        )
    };
    card(p, stretch(12.0, vec![
        b("Gestures and shortcuts", 18.0, p.ink),
        entry("Two-finger swipe", "Next or previous session. Arrow keys too."),
        entry("Swipe past the last", "Start a new session."),
        entry("Pinch (or O)", "Fingers together: every session side by side. Apart: back to the one you were on. × closes one into Saved."),
        entry("Hold Space", "Peek at the live agents and tools. E keeps them open."),
        entry("Drag an approval", "Right allows, left refuses. The buttons do the same."),
        entry("+ in the composer", "Attach files or a folder, or drop them on the window."),
        entry("Ctrl/Cmd + Shift + T", "Reopen the last closed session."),
        entry("Ctrl/Cmd + ,", "Settings. Esc backs out of anything."),
        entry("Agents & tools", "Wright's built-in agents and every tool a plan can grant, in Settings."),
        row(10.0, vec![
            button(p, "Agents & tools", Look::Secondary, true, move || list.open_agents_and_tools()),
            button(p, "Replay the opening", Look::Secondary, true, move || {
                replay.help.set(false);
                replay.splash.set(true);
            }),
            gap(),
            button(p, "Done", Look::Primary, true, move || close.set(false)),
        ]),
    ]))
}

// ── headless ─────────────────────────────────────────────────────

fn png(
    driver: &mut FrameDriver,
    renderer: &mut vieww::paint::native::NativeRenderer,
    size: Size,
    bg: Color,
    path: &Path,
) -> Result<(), String> {
    let (bytes, _) = renderer
        .render_to_png(driver.scene(), size.width as u32, size.height as u32, bg)
        .map_err(|e| format!("render: {e}"))?;
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// The app icon the installers ship: the mark on a rounded tile.
/// `padded` leaves the margin macOS icons expect.
pub fn render_icon(path: &Path, side: u32, padded: bool) -> Result<(), String> {
    use vieww::paint::native::NativeRenderer;
    let size = Size::new(side as f32, side as f32);
    let mut driver = FrameDriver::new(size);
    let tile_side = if padded {
        side as f32 * 0.8
    } else {
        side as f32
    };
    driver.set_root(Center::new().child(logo::tile(tile_side, logo::ICON_GROUND, logo::ICON_INK)));
    driver.draw_frame_at(Duration::from_millis(16));
    png(
        &mut driver,
        &mut NativeRenderer::new(),
        size,
        Color::rgba(0, 0, 0, 0),
        path,
    )
}

/// The window's icon, in pixels the platform can draw beside the title:
/// the same tile the installers ship, rendered headlessly. This is the
/// brand's place on a desktop window — the title bar, where the minimise
/// and close buttons are — so the shell draws the mark nowhere else at the
/// top. Windows and X11 put it in that bar; macOS carries it to the Dock;
/// a compositor that takes neither has still the title text, "wright".
pub fn window_icon() -> Option<vieww_platform_winit::WindowIcon> {
    use vieww::paint::native::NativeRenderer;
    const SIDE: u32 = 32;
    let size = Size::new(SIDE as f32, SIDE as f32);
    let mut driver = FrameDriver::new(size);
    driver.set_root(Center::new().child(logo::tile(
        SIDE as f32,
        logo::ICON_GROUND,
        logo::ICON_INK,
    )));
    driver.draw_frame_at(Duration::from_millis(16));
    let (pixels, _) = NativeRenderer::new()
        .render_to_pixels(driver.scene(), SIDE, SIDE, Color::rgba(0, 0, 0, 0))
        .ok()?;
    Some(vieww_platform_winit::WindowIcon::new(
        SIDE,
        SIDE,
        pixels.data().to_vec(),
    ))
}

/// Every screen of the shell to PNG, headless. Uses `WRIGHT_HOME` like
/// everything else, so point it somewhere disposable.
pub fn snapshots(project: Option<&Path>, dir: &Path) -> Result<usize, String> {
    use vieww::paint::native::NativeRenderer;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let size = Size::new(1280.0, 800.0);
    let mut driver = FrameDriver::new(size);
    register(&mut driver);
    let ui = Ui::new(
        driver.elements().runtime(),
        project.map(Path::to_path_buf),
        Arc::new(AtomicBool::new(false)),
    );
    ui.go(0);
    driver.set_root(root(&ui));
    let mut renderer = NativeRenderer::new();
    // Layout that does not fit is a failure here, not a line in stderr.
    vieww::render::overflow::forget_reported();
    let mut clock = 0u64;
    let mut shot = |driver: &mut FrameDriver, name: &str, settle_ms: u64| -> Result<(), String> {
        for _ in 0..(settle_ms / 50).max(2) {
            clock += 50;
            driver.draw_frame_at(Duration::from_millis(clock));
        }
        png(
            driver,
            &mut renderer,
            size,
            ui.pal().ground,
            &dir.join(format!("{name}.png")),
        )
    };
    let mut n = 0;
    shot(&mut driver, "1-splash", 1700)?;
    n += 1;
    ui.splash.set(false);
    shot(&mut driver, "2-home", 600)?;
    n += 1;
    if !ui.sessions.get().is_empty() {
        ui.go(1);
        shot(&mut driver, "3-session", 900)?;
        ui.set_engine(EngineView::Open);
        shot(&mut driver, "4-engine", 900)?;
        ui.set_engine(EngineView::Hidden);
        n += 2;
    }
    ui.set_overview(true);
    shot(&mut driver, "5-overview", 900)?;
    n += 1;
    ui.set_overview(false);
    ui.open_settings();
    shot(&mut driver, "6-settings", 400)?;
    n += 1;
    // A built-in model mid-download, as Settings shows it.
    ui.form.pulls.set(
        [(
            wright_runtime::local::TEXT_MODEL.to_string(),
            super::settings::Pull::Working {
                line: "Downloading the built-in model qwen3-1.7b (first use, once): 42% of 1.83 GB"
                    .into(),
                percent: 42,
            },
        )]
        .into_iter()
        .collect(),
    );
    shot(&mut driver, "6-settings-download", 200)?;
    n += 1;
    ui.form.pulls.set(Default::default());
    ui.open_agents_and_tools();
    shot(&mut driver, "6-settings-agents", 400)?;
    n += 1;
    // Office machines: certificates and environment variables.
    ui.form.tab.set(super::settings::Tab::Network);
    ui.form.scroll.offset.set(0.0);
    shot(&mut driver, "6-settings-network", 400)?;
    ui.form.tab.set(super::settings::Tab::Fleet);
    shot(&mut driver, "6-settings-fleet", 400)?;
    ui.form.tab.set(super::settings::Tab::Organization);
    shot(&mut driver, "6-settings-organization", 400)?;
    n += 3;
    ui.form.tab.set(super::settings::Tab::Models);
    ui.settings.set(false);
    ui.mode.set(Mode::Light);
    shot(&mut driver, "7-light", 600)?;
    n += 1;
    if let Some(s) = ui.sessions.get().first().cloned() {
        ui.mode.set(Mode::Dark);
        ui.go(1);
        s.export_menu.set(true);
        s.scroll.to_end();
        shot(&mut driver, "8-export", 600)?;
        s.export_menu.set(false);
        // A reply on its way: the words so far and what Wright is doing.
        s.pending.set(Some(super::session::Pending {
            what: "Drafting the plan".into(),
            text: "Got it: a weekly summary of the sales folder. I'll draft a plan with one reader and one writer".into(),
            since: std::time::Instant::now() - std::time::Duration::from_secs(7),
        }));
        s.scroll.to_end();
        shot(&mut driver, "9-thinking", 600)?;
        s.pending.set(None);
        // The kill switch, as the window shows it (drawn, not pulled).
        ui.stopped.set(Some("a test of the stop banner".into()));
        shot(&mut driver, "10-stopped", 300)?;
        ui.stopped.set(None);
        n += 3;
        // The receipt, as the window proves a run: every check in
        // verify's words, over the session page.
        if s.receipt.get().is_some() {
            s.receipt_open.set(true);
            shot(&mut driver, "11-receipt", 500)?;
            s.receipt_open.set(false);
            n += 1;
        }
        // The shell's light side, seen on the pages that carry a run —
        // the theme is one flip, and the evidence should hold in both.
        ui.mode.set(Mode::Light);
        shot(&mut driver, "12-light-session", 600)?;
        ui.set_engine(EngineView::Open);
        shot(&mut driver, "13-light-engine", 700)?;
        ui.set_engine(EngineView::Hidden);
        ui.mode.set(Mode::Dark);
        n += 2;
    }
    match vieww::render::overflow::reported() {
        0 => Ok(n),
        k => Err(format!("{k} layout overflow(s) while rendering (each is printed above as 'vieww: ... overflowed')")),
    }
}

/// "1 session", "2 sessions".
fn plural(n: usize, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}
