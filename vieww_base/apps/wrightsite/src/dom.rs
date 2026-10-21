//! The product page as a vieww tree, rendered to DOM.
//!
//! # What the browser does that the tree used to
//!
//! The canvas half (`src/lib.rs`) builds a scrollbar, measures every section
//! so the nav can land on it, runs fade-and-lift arithmetic against the scroll
//! offset, and tracks hover through a `Pressable`. All four are things a
//! browser already does. Here the document scrolls, `#fragment` links land,
//! CSS reveals on scroll, and `:hover` is a selector. What is left is the
//! page.
//!
//! The widgets are the same widgets — a tree that also rasterises, which is
//! what `examples/render.rs` still does with the canvas half. [`Styled`]
//! carries the declarations no rasteriser can honour, [`Tag`] names the
//! element a box should be, and [`Canvas`] is the seam where a real vieww
//! application is painted into the page by vieww's own renderer: the live
//! session below, its firing computed, its gate waiting.

use vieww::foundation::{Color, EdgeInsets, FontFamily, TextAlign, TextStyle};
use vieww::prelude::*;
use vieww_platform_web_dom::{Canvas, Styled, Tag};

use crate::{highlight, url_for, Os, ASSETS};

// ─── the palette ──────────────────────────────────────────────────────────
//
// `apps/wright/app/src/ui/theme.rs`'s `DARK` and `LIGHT` pals, by role. The
// names are the app's own, restated here rather than imported for the same
// reason the canvas half restates them: a binary crate is not a dependency,
// and a page that drifted from the product it sells would be advertising a
// theme nobody ships.

/// `DARK.ground`.
const GROUND: Color = Color::rgb(0x1F, 0x1F, 0x1F);
/// `DARK.ink`, lifted two points: page ink, brighter than the window's own.
const INK: Color = Color::rgb(0xE8, 0xE8, 0xE8);
/// `DARK.muted`.
const INK_2: Color = Color::rgb(0x9D, 0x9D, 0x9D);
const INK_3: Color = Color::rgb(0x6E, 0x6E, 0x6E);
/// `DARK.accent`.
const ACCENT: Color = Color::rgb(0x00, 0x78, 0xD4);
/// `DARK.accent_ink` — the accent as text on the dark ground.
const ACCENT_INK: Color = Color::rgb(0x4D, 0xAA, 0xFC);
/// `DARK.good` — the live dot; a stage that finished.
const LIVE: Color = Color::rgb(0x89, 0xD1, 0x85);

/// The palette as CSS custom properties, for
/// `vieww_platform_web_dom::set_color_tokens`: every text run and box in
/// these colours is written as `var(--name)`, and `index.html`'s
/// `prefers-color-scheme: light` block redefines them. The tree stays
/// literal — it still rasterises, dark, for `examples/render.rs`.
pub const COLOR_TOKENS: &[(Color, &str)] = &[
    (GROUND, "ground"),
    (INK, "ink"),
    (INK_2, "ink-2"),
    (INK_3, "ink-3"),
    (ACCENT_INK, "accent"),
    (LIVE, "live"),
];

// ─── the small vocabulary this page is written in ─────────────────────────

fn sans(size: f32, color: Color) -> TextStyle {
    TextStyle::new(size)
        .family(FontFamily::Named("Geist"))
        .color(color)
        .line_height(1.55)
}

fn mono(size: f32, color: Color) -> TextStyle {
    TextStyle::new(size)
        .family(FontFamily::Named("Geist Mono"))
        .color(color)
        .line_height(1.5)
}

fn text(content: &str, style: TextStyle) -> WidgetNode {
    Text::new(content.to_string()).style(style).into()
}

/// A heading, in the semantic element it deserves.
fn heading(level: &'static str, content: &str, size: f32, align: TextAlign) -> WidgetNode {
    Tag::new(
        level,
        Text::new(content.to_string())
            .style(
                sans(size, INK)
                    .weight(FontWeight::Bold)
                    .line_height(1.07)
                    .letter_spacing(size * -0.023),
            )
            .align(align),
    )
    .into()
}

/// The column every section is measured against — `max-w-7xl`.
fn column(child: impl Into<WidgetNode>) -> WidgetNode {
    Styled::css(
        "width:100%;max-width:1280px;margin:0 auto;padding-left:20px;padding-right:20px",
        child,
    )
    .into()
}

/// Fades and lifts as it comes into view — `animation-timeline: view()`: the
/// browser drives it off the scroll position with no observer, no listener
/// and no per-frame work in this application at all.
fn reveal(child: impl Into<WidgetNode>) -> WidgetNode {
    Styled::class("wr-reveal", child).into()
}

fn spacer(height: f32) -> WidgetNode {
    SizedBox::height(height).into()
}

fn col(spacing: f32, children: Vec<WidgetNode>) -> Flex {
    Flex::column()
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .spacing(spacing)
        .children(children)
}

fn row(spacing: f32, children: Vec<WidgetNode>) -> Flex {
    Flex::row()
        .main_axis_size(MainAxisSize::Min)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .spacing(spacing)
        .children(children)
}

/// A section: the vertical rhythm, and an `id` for the nav to land on.
fn section(id: &'static str, children: Vec<WidgetNode>) -> WidgetNode {
    Tag::new(
        "section",
        Styled::class("wr-section", column(col(0.0, children))),
    )
    .id(id)
    .into()
}

/// The pill above a heading.
fn eyebrow(label: &str) -> WidgetNode {
    Styled::css(
        "display:inline-flex;align-items:center;gap:7px;border-radius:999px;\
         border:1px solid rgba(77,170,252,0.28);background:rgba(77,170,252,0.10);\
         padding:5px 12px;width:fit-content;margin:0 auto",
        row(
            7.0,
            children![
                Container::new().color(ACCENT_INK).radius(999.0).size(5.0, 5.0),
                text(
                    label,
                    mono(10.5, ACCENT_INK)
                        .letter_spacing(1.6)
                        .weight(FontWeight::Medium),
                ),
            ]
            .into_iter()
            .collect(),
        ),
    )
    .into()
}

/// Eyebrow, heading, lead — the block every section opens with.
fn section_head(label: &str, title: &str, lead: &str) -> WidgetNode {
    reveal(
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .spacing(0.0)
            .children(children![
                eyebrow(label),
                spacer(18.0),
                Styled::css(
                    "max-width:768px;margin:0 auto",
                    heading("h2", title, 42.0, TextAlign::Center)
                )
                .with("wr-h2"),
                spacer(16.0),
                Styled::css(
                    "max-width:672px;margin:0 auto",
                    Text::new(lead.to_string())
                        .style(sans(17.5, INK_2).line_height(1.62))
                        .align(TextAlign::Center)
                )
                .with("wr-lead"),
            ]),
    )
}

// ─── the mark ─────────────────────────────────────────────────────────────

/// The W, as a file the CSS can put anywhere the DOM walk cannot.
///
/// The DOM backend has no `Painting`, and a mark that only existed on the
/// rasterised half would leave the served page logo-less. The data URI below
/// is the app's own geometry — `ui/logo.rs`'s five corners, one stroke, round
/// caps and joins — with the colour fixed per variant, because a data URI
/// cannot ask the page for `currentColor`.
fn w_svg(stroke: &str) -> String {
    format!(
        "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' \
         viewBox='0 0 18 13'%3E%3Cpath d='M0 0 L4 13 L9 3 L14 13 L18 0' \
         fill='none' stroke='{stroke}' stroke-width='2' \
         stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E"
    )
}

/// The mark on its tile, the way the icon and the window's top corner carry
/// it: the blue squircle, radius 30% of the side, the W at 62% of it.
fn mark(side: f32) -> WidgetNode {
    let radius = side * 0.3;
    let inner = side * 0.62;
    Styled::empty(format!(
        "width:{side}px;height:{side}px;border-radius:{radius}px;background:{accent} \
         url(\"{svg}\") center/{inner}px {inner}px no-repeat",
        accent = css_hex(ACCENT),
        svg = w_svg("%23FFFFFF"),
    ))
    .with("wr-mark")
    .into()
}

/// The mark alone, in whatever colour the caller wants, no tile — the
/// feature cards' corner glyph and the closing call's mark.
fn mark_bare(side: f32, color: Color) -> WidgetNode {
    let stroke = format!("%23{:02X}{:02X}{:02X}", color.r, color.g, color.b);
    Styled::empty(format!(
        "width:{side}px;height:{side}px;background:url(\"{svg}\") center/contain no-repeat",
        svg = w_svg(&stroke),
    ))
    .into()
}

/// One of the palette as `#RRGGBB`, for the inline declarations above.
fn css_hex(color: Color) -> String {
    format!("#{:02X}{:02X}{:02X}", color.r, color.g, color.b)
}

// ─── controls ─────────────────────────────────────────────────────────────

fn button(label: &str, href: &str, filled: bool) -> WidgetNode {
    let css = if filled {
        "display:inline-flex;align-items:center;justify-content:center;gap:8px;\
         border-radius:999px;background:linear-gradient(180deg,#2B93E9,#0078D4);\
         color:#FFFFFF;padding:11px 22px;text-decoration:none;font-weight:600;\
         box-shadow:0 8px 24px rgba(0,120,212,0.35),inset 0 1px 0 rgba(255,255,255,0.35);\
         white-space:nowrap"
    } else {
        "display:inline-flex;align-items:center;justify-content:center;gap:8px;\
         border-radius:999px;border:1px solid rgba(var(--glass),0.16);\
         background:rgba(var(--glass),0.04);padding:11px 22px;text-decoration:none;\
         backdrop-filter:blur(8px);white-space:nowrap"
    };
    Tag::link(
        href.to_string(),
        Styled::css(
            css,
            text(
                label,
                sans(14.5, if filled { Color::WHITE } else { INK })
                    .weight(FontWeight::Medium),
            ),
        )
        .with("wr-btn"),
    )
    .into()
}

// ─── the hero ─────────────────────────────────────────────────────────────

fn hero(host: Option<Os>) -> WidgetNode {
    let primary = match host {
        Some(Os::MacOs) => "Download for macOS",
        Some(Os::Windows) => "Download for Windows",
        _ => "Download for Linux",
    };
    Tag::new(
        "header",
        Styled::class(
            "wr-hero",
            Stack::new().children(children![
                // **The glow, and only the glow** — the lit ground is the
                // whole backdrop. Inline rather than in the class so the DOM
                // walk sees `position:absolute` and does not hand it a
                // `grid-area`.
                Styled::empty("position:absolute;inset:0;pointer-events:none")
                    .with("wr-hero-glow"),
                column(
                    Flex::column()
                        .cross_axis_alignment(CrossAxisAlignment::Center)
                        .spacing(0.0)
                        .children(children![
                            Styled::class("wr-rise wr-d0 wr-markglow", mark(84.0)),
                            spacer(24.0),
                            Styled::class("wr-rise wr-d1", status_pill()),
                            spacer(30.0),
                            Styled::css(
                                "max-width:980px;margin:0 auto",
                                Flex::column()
                                    .cross_axis_alignment(CrossAxisAlignment::Center)
                                    .spacing(2.0)
                                    .children(children![
                                        Styled::class(
                                            "wr-h1 wr-rise wr-d2",
                                            heading(
                                                "h1",
                                                "Describe the outcome.",
                                                68.0,
                                                TextAlign::Center,
                                            ),
                                        ),
                                        // The second sentence in a
                                        // white-to-accent gradient clipped to
                                        // the glyphs — a declaration no
                                        // rasteriser primitive expresses.
                                        Styled::class(
                                            "wr-h1 wr-gradient-text wr-rise wr-d3",
                                            heading(
                                                "div",
                                                "Agents wright it.",
                                                68.0,
                                                TextAlign::Center,
                                            ),
                                        ),
                                    ]),
                            ),
                            spacer(26.0),
                            Styled::class(
                                "wr-rise wr-d4",
                                Styled::css(
                                    "max-width:660px;margin:0 auto",
                                    Text::new(
                                        "You describe what should exist. Wright drafts a typed \
                                         blueprint, shows you — in plain words generated from that \
                                         blueprint, never by a model — what will run, what each \
                                         step may touch, what it may cost and how done is checked. \
                                         You approve; a workforce of agents builds it in a sandbox, \
                                         tests it, and delivers it."
                                            .to_string(),
                                    )
                                    .style(sans(19.0, INK_2).line_height(1.6))
                                    .align(TextAlign::Center),
                                )
                                .with("wr-lead-hero"),
                            ),
                            spacer(36.0),
                            Styled::class(
                                "wr-cta wr-rise wr-d5",
                                Flex::row()
                                    .main_axis_size(MainAxisSize::Min)
                                    .main_axis_alignment(MainAxisAlignment::Center)
                                    .spacing(12.0)
                                    .children(children![
                                        button(primary, "#download", true),
                                        button("See how it works", "#flow", false),
                                    ]),
                            ),
                            spacer(18.0),
                            Styled::class(
                                "wr-rise wr-d5",
                                Text::new(
                                    "MIT · zero external crates, headless · one dependency for the window: vieww"
                                )
                                .style(mono(11.5, INK_3))
                                .align(TextAlign::Center),
                            ),
                            spacer(52.0),
                            Styled::class("wr-rise wr-d6", stat_strip()),
                        ]),
                ),
            ]),
        ),
    )
    .id("top")
    .into()
}

fn status_pill() -> WidgetNode {
    Styled::css(
        "display:inline-flex;align-items:center;gap:9px;border-radius:999px;\
         border:1px solid rgba(var(--glass),0.12);background:var(--pill);\
         backdrop-filter:blur(10px);padding:6px 14px;margin:0 auto;width:fit-content;\
         max-width:100%",
        row(
            9.0,
            children![
                Styled::class(
                    "wr-live",
                    Container::new().color(LIVE).radius(999.0).size(7.0, 7.0),
                ),
                text("v0.5.1", mono(11.5, INK)),
                text("·", mono(11.5, INK_3)),
                text("Built-in models. No key, no account.", mono(11.5, INK_2)),
            ]
            .into_iter()
            .collect(),
        ),
    )
    .into()
}

/// Four measured numbers a person deciding whether to download wants first.
fn stat_strip() -> WidgetNode {
    let cell = |value: &str, note: &str| -> WidgetNode {
        Styled::class(
            "wr-stat",
            Flex::column()
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .spacing(5.0)
                .children(children![
                    Styled::class(
                        "wr-stat-v",
                        text(value, mono(25.0, INK).weight(FontWeight::Bold)),
                    ),
                    Styled::class(
                        "wr-stat-n",
                        Text::new(note.to_string())
                            .style(sans(11.5, INK_3))
                            .align(TextAlign::Center),
                    ),
                ]),
        )
        .into()
    };
    Styled::css(
        "max-width:800px;margin:0 auto;border:1px solid rgba(var(--glass),0.10);\
         border-radius:16px;overflow:hidden;background:rgba(var(--glass),0.08);\
         box-shadow:0 20px 50px var(--shadow-deep)",
        Styled::class(
            "wr-stats",
            Flex::row().children(children![
                cell("236+", "tests passing, clippy clean"),
                cell("0", "external crates, headless"),
                cell("1", "dependency for the window"),
                cell("3", "platforms with installers"),
            ]),
        ),
    )
    .into()
}

// ─── the flow ─────────────────────────────────────────────────────────────

fn flow() -> WidgetNode {
    let step = |number: &str, role: &str, title: &str, body: &str| -> WidgetNode {
        reveal(Styled::class(
            "wr-card wr-step",
            Container::new().padding(EdgeInsets::all(24.0)).child(col(
                10.0,
                children![
                    row(
                        10.0,
                        children![
                            Styled::css(
                                "width:26px;height:26px;border-radius:8px;\
                                 background:rgba(77,170,252,0.14);\
                                 border:1px solid rgba(77,170,252,0.30);\
                                 display:flex;align-items:center;justify-content:center",
                                text(number, mono(12.0, ACCENT_INK).weight(FontWeight::Bold)),
                            ),
                            text(
                                role,
                                mono(10.0, INK_3)
                                    .letter_spacing(1.5)
                                    .weight(FontWeight::Medium),
                            ),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                    text(
                        title,
                        sans(19.0, INK).weight(FontWeight::Bold).line_height(1.25),
                    ),
                    text(body, sans(14.0, INK_2)),
                ]
                .into_iter()
                .collect(),
            )),
        ))
    };
    section(
        "flow",
        vec![
            section_head(
                "THE FLOW",
                "Describe it. Confirm it in plain words. Fire it.",
                "Wright turns a person's intent into a working, repeatable project — and the \
                 one thing it will not do is run something you have not read and approved first.",
            ),
            spacer(52.0),
            Styled::class(
                "wr-grid3",
                Flex::column().spacing(16.0).children(children![
                    step(
                        "01",
                        "DESCRIBE",
                        "Say what should exist",
                        "The Interview condenses what you say — typed or spoken, transcribed on \
                         this machine by the built-in Whisper — into a typed blueprint: agents, \
                         edges, chips, guardrails, deliverables. One file is the whole project, \
                         and you never have to write it.",
                    ),
                    step(
                        "02",
                        "CONFIRM",
                        "Read the plan in plain words",
                        "The Confirm Gate shows exactly what will run — what each step may touch, \
                         what it may cost, how done is checked — in words generated from the \
                         blueprint by code, never by a model. Nothing executes until you approve, \
                         and nothing touches the world before Testwright and Reviewer pass.",
                    ),
                    step(
                        "03",
                        "FIRE",
                        "Agents build it in a sandbox",
                        "A workforce of agents builds the project inside a Landlock and \
                         network-namespace sandbox, tests it, has it reviewed, and delivers it. \
                         Every run is fsynced to a durable event log with receipts, and it \
                         resumes after a crash. Change one requirement later and only the \
                         affected work runs again.",
                    ),
                ]),
            )
            .into(),
        ],
    )
}

// ─── the window ───────────────────────────────────────────────────────────

/// The screenshots, in application chrome.
fn window_section() -> WidgetNode {
    let shot = |asset: &str, tag: &str, note: &str| -> WidgetNode {
        reveal(WidgetNode::from(Styled::class(
            "wr-card",
            col(
                0.0,
                children![
                    Styled::empty(format!(
                        "width:100%;aspect-ratio:16/10;background-image:url('{asset}');\
                         background-size:cover;background-position:top center;\
                         background-color:#141414"
                    )),
                    Styled::css(
                        "border-top:1px solid rgba(var(--glass),0.07);padding:15px 20px",
                        col(
                            3.0,
                            children![
                                text(tag, mono(10.0, ACCENT_INK).letter_spacing(1.5)),
                                text(note, sans(14.0, INK_2)),
                            ]
                            .into_iter()
                            .collect(),
                        ),
                    ),
                ]
                .into_iter()
                .collect(),
            ),
        )))
    };
    section(
        "window",
        vec![
            section_head(
                "THE WINDOW",
                "Sessions, swipe, a live engine, and your voice.",
                "The window is a vieww shell: every session in a row you swipe through, the \
                 engine live under Space, approvals as cards you answer with a tap or a drag, \
                 files you drop on the window, and a mic that transcribes on this machine with \
                 the built-in Whisper.",
            ),
            spacer(52.0),
            Styled::class(
                "wr-grid4",
                Flex::column().spacing(16.0).children(children![
                    shot("1-describe.png", "THE INTERVIEW", "Describe the outcome, in your words."),
                    shot("2-plan.png", "THE CONFIRM GATE", "The plan, generated from the blueprint."),
                    shot("3-run.png", "THE FIRING", "The event river, and the gate."),
                    shot("4-result.png", "THE RECEIPT", "Delivered, with its history."),
                ]),
            )
            .into(),
        ],
    )
}

// ─── the island ───────────────────────────────────────────────────────────

/// The island's CSS box. The widget tree inside is laid out against exactly
/// this, and the bezel around it is a `border-radius` on the wrapper.
const DEMO_W: f32 = 300.0;
const DEMO_H: f32 = 520.0;

/// The live demo: DOM around it, vieww's own rasteriser inside it.
fn demo() -> WidgetNode {
    section(
        "demo",
        vec![
            section_head(
                "LIVE DEMO",
                "This is not a screenshot.",
                "The window below is a vieww widget tree, running in your browser: a Signal for \
                 the gate, two Animations for the run, and the same stage glyphs the real window \
                 draws. It fired on its own when you arrived; it is holding at the gate. Answer \
                 it. Allow, and Mailer sends and the receipt is written. Refuse, and nothing \
                 leaves the machine.",
            ),
            spacer(48.0),
            reveal(Styled::block(
                "display:flex;justify-content:center",
                Styled::class(
                    "wr-frame",
                    Styled::css(
                        "border-radius:26px;overflow:hidden;background:#1F1F1F;max-width:100%",
                        Canvas::new("wr-demo", DEMO_W, DEMO_H, mount_demo),
                    ),
                ),
            )),
            spacer(18.0),
            Text::new(
                "A real vieww screen, running here — computed, not replayed. Answer the gate."
            )
            .style(mono(12.0, INK_3))
            .align(TextAlign::Center)
            .into(),
        ],
    )
}

/// Start a genuine vieww application on the canvas the DOM walk just
/// created: the same session screen the canvas half carries, its firing
/// started the same way.
#[cfg(target_arch = "wasm32")]
fn mount_demo(id: &str) {
    use vieww_platform_web::{WebApp, WebSurface};
    let Ok(surface) = WebSurface::by_id(id) else {
        return;
    };
    // The canvas carries the size the page laid out for it in CSS, not in its
    // `width`/`height` attributes — which default to 300x150 and are what
    // `by_id` starts from.
    let mut surface = surface;
    let _ = surface.resize(vieww::foundation::Size::new(DEMO_W, DEMO_H));
    let _ = WebApp::new(surface)
        .background(GROUND)
        .mount_with(|driver| {
            driver.set_fonts(crate::fonts());
            let runtime = driver.elements().runtime().clone();
            let (fire, deliver, gate) = crate::start_demo(&runtime, driver.tickers());
            driver.set_root(crate::session_screen(fire, deliver, gate));
        })
        .map(std::mem::forget);
}

#[cfg(not(target_arch = "wasm32"))]
fn mount_demo(_id: &str) {}

// ─── the firing ───────────────────────────────────────────────────────────

/// The eight stages of the example firing, as the window would show them.
fn firing() -> WidgetNode {
    let stages: &[(&str, &str, &str, &str)] = &[
        ("01", "Shipwright", "built-in", "lays the universal skeleton — no model, milliseconds"),
        ("02", "Logwright", "chip: logging", "logging, inside that skeleton"),
        ("03", "Fetcher", "your agent", "live data: always fresh"),
        ("04", "Summarizer", "your agent", "cached if its input did not change"),
        ("05", "Testwright", "chip: tests", "writes + runs checks"),
        ("06", "Reviewer", "", "APPROVE or REJECT — rejection stops delivery"),
        ("07", "Mailer", "", "touches the world: asks first, never cached, never retried"),
        ("08", "Operator", "built-in", "registers the schedule; the receipt is written"),
    ];
    let rows: Vec<WidgetNode> = stages
        .iter()
        .map(|(number, name, chip_text, note)| {
            let emphasis = *name == "Reviewer" || *name == "Mailer";
            let chip_node: WidgetNode = if chip_text.is_empty() {
                SizedBox::width(96.0).into()
            } else {
                Styled::css(
                    "border:1px solid rgba(var(--glass),0.14);border-radius:999px;\
                     padding:3px 10px;white-space:nowrap",
                    text(chip_text, mono(10.0, ACCENT_INK)),
                )
                .into()
            };
            reveal(Styled::class(
                if emphasis { "wr-stage wr-stage-hot" } else { "wr-stage" },
                Container::new().padding(EdgeInsets::symmetric(18.0, 13.0)).child(
                    Flex::row()
                        .main_axis_size(MainAxisSize::Min)
                        .cross_axis_alignment(CrossAxisAlignment::Center)
                        .spacing(14.0)
                        .children(children![
                            text(number, mono(11.0, INK_3)),
                            text(name, sans(14.5, INK).weight(FontWeight::Bold)),
                            Flexible::new(1).child(
                                Text::new(note.to_string())
                                    .style(sans(13.0, INK_2))
                                    .align(TextAlign::End),
                            ),
                            chip_node,
                        ]),
                ),
            ))
            .into()
        })
        .collect();
    section(
        "firing",
        vec![
            section_head(
                "THE FIRING",
                "Eight stages. Nothing touches the world before the checks pass.",
                "Your agents work inside the sandbox; Testwright writes and runs the checks; \
                 Reviewer's rejection stops delivery; and Mailer — the one step that touches the \
                 outside — asks first, never runs from cache, and is never retried.",
            ),
            spacer(44.0),
            reveal(Styled::css(
                "border:1px solid rgba(var(--glass),0.10);border-radius:16px;overflow:hidden;\
                 background:rgba(var(--glass),0.02)",
                Styled::class("wr-rows", col(0.0, rows)),
            )),
        ],
    )
}

// ─── code ─────────────────────────────────────────────────────────────────

/// One source sample, tokenised and coloured. `highlight` is the page's own
/// five-token lexer, shared with the canvas build and memoised there.
fn code_block(filename: &str, lang: &str, source: &[&str]) -> WidgetNode {
    let lines: Vec<WidgetNode> = source
        .iter()
        .enumerate()
        .map(|(index, line)| {
            row(
                0.0,
                children![
                    Styled::css(
                        "width:34px;flex:none;text-align:right;padding-right:16px;\
                         user-select:none;opacity:.45",
                        text(&(index + 1).to_string(), mono(12.5, INK_3)),
                    ),
                    Flexible::new(1).child(if line.is_empty() {
                        text(" ", mono(12.5, INK_2))
                    } else {
                        RichText::new(highlight(line, 12.5)).into()
                    }),
                ]
                .into_iter()
                .collect(),
            )
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .into()
        })
        .collect();

    Styled::class(
        "wr-code",
        col(
            0.0,
            children![
                Styled::css(
                    "border-bottom:1px solid rgba(var(--glass),0.07);\
                     background:rgba(var(--glass),0.03);padding:11px 14px",
                    Flex::row()
                        .main_axis_alignment(MainAxisAlignment::SpaceBetween)
                        .cross_axis_alignment(CrossAxisAlignment::Center)
                        .children(children![
                            row(
                                8.0,
                                children![
                                    dot(Color::rgb(0xFF, 0x5F, 0x57)),
                                    dot(Color::rgb(0xFE, 0xBC, 0x2E)),
                                    dot(Color::rgb(0x28, 0xC8, 0x40)),
                                    SizedBox::width(6.0),
                                    text(filename, mono(12.0, INK_2)),
                                ]
                                .into_iter()
                                .collect(),
                            ),
                            Styled::css(
                                "border:1px solid rgba(var(--glass),0.10);border-radius:6px;\
                                 padding:2px 8px",
                                text(lang, mono(10.0, INK_3).letter_spacing(1.0)),
                            ),
                        ]),
                ),
                Styled::css("padding:18px 16px;overflow-x:auto", col(3.0, lines)),
            ]
            .into_iter()
            .collect(),
        ),
    )
    .into()
}

fn dot(color: Color) -> WidgetNode {
    Container::new()
        .color(color)
        .radius(999.0)
        .size(11.0, 11.0)
        .into()
}

/// A terminal block: the `$` in the accent, the command in ink, the output
/// muted — and a Copy button that takes the commands with it. The one piece
/// of the page a reader is meant to take away, so it is a real `<button>`:
/// the keyboard reaches it, and Enter and Space press it.
fn terminal(title: &str, lines: &[&str]) -> WidgetNode {
    let rows: Vec<WidgetNode> = lines
        .iter()
        .map(|line| {
            if let Some(rest) = line.strip_prefix("$ ") {
                row(
                    8.0,
                    children![
                        text("$", mono(12.5, ACCENT_INK).weight(FontWeight::Bold)),
                        // In a `Flexible`, so the command is bounded and wraps
                        // rather than measuring intrinsically off the end of
                        // the block.
                        Flexible::new(1)
                            .child(Styled::class("wr-cmd", text(rest, mono(12.5, INK)))),
                    ]
                    .into_iter()
                    .collect(),
                )
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .into()
            } else {
                text(line, mono(12.5, if line.starts_with("  ") { INK_2 } else { INK_3 })).into()
            }
        })
        .collect();
    Styled::class(
        "wr-term",
        col(
            0.0,
            children![
                Styled::css(
                    "border-bottom:1px solid rgba(var(--glass),0.07);\
                     background:rgba(var(--glass),0.03);padding:10px 14px",
                    Flex::row()
                        .main_axis_alignment(MainAxisAlignment::SpaceBetween)
                        .cross_axis_alignment(CrossAxisAlignment::Center)
                        .children(children![
                            text(title, mono(11.5, INK_3)),
                            Tag::new(
                                "button",
                                Styled::class(
                                    "wr-copy",
                                    Styled::css(
                                        "border:1px solid rgba(var(--glass),0.12);border-radius:7px;\
                                         padding:3px 10px;cursor:pointer",
                                        text("Copy", mono(10.5, INK_2)),
                                    ),
                                ),
                            ),
                        ]),
                ),
                Styled::css("padding:16px", col(5.0, rows)),
            ]
            .into_iter()
            .collect(),
        ),
    )
    .into()
}

/// The HN Daily Digest blueprint, abridged to the lines that carry the idea —
/// the full file is `apps/wright/examples/hn-digest/blueprint.kdl`. Kept in
/// step with the canvas half's copy by living beside it in the same crate.
const KDL_SAMPLE: &[&str] = &[
    "// blueprint.kdl — one file is the whole project.",
    "meta { name \"HN Daily Digest\" format 1 }",
    "",
    "intent {",
    "    summary \"Daily digest of top HN stories,",
    "             summarized, emailed at 8am.\"",
    "    confirmed \"v7\"",
    "}",
    "",
    "agent Fetcher {",
    "    goal \"Fetch top 30 HN stories: id, title, url, points\"",
    "    model tier=\"cheap\"   tools \"http-read\"",
    "    out \"stories.json\"   permissions network=true",
    "}",
    "agent Summarizer {",
    "    goal \"Write a punchy two-sentence summary per story\"",
    "    in \"stories.json\"   out \"summaries.json\"",
    "}",
    "agent Mailer {",
    "    goal \"Compose and send the digest email\"",
    "    in \"summaries.json\" out \"receipt.eml\"",
    "    approval before-send   retries 0",
    "}",
    "",
    "edge from=\"Fetcher\" to=\"Summarizer\"",
    "edge from=\"Summarizer\" to=\"Mailer\"",
    "",
    "chips {",
    "    scheduling cron=\"0 8 * * *\" tz=\"Asia/Kolkata\"",
    "    secrets smtp=\"keychain:wright/hn-digest/smtp\"",
    "    tests on   logging on",
    "}",
    "",
    "guardrails {",
    "    budget per-firing=500000 daily=3000000  // micro-USD",
    "    wall-clock 300   max-turns 40",
    "}",
    "",
    "deliverable kind=\"email\" {",
    "    check \"Mail received 08:00-08:05 local, every story",
    "           present, no dead links\"",
    "}",
];

/// The Confirm Gate's own words for it — `core/src/confirm.rs`'s output shape,
/// which is generated from the model by code, never by a model.
const PLAN_SAMPLE: &[&str] = &[
    "$ wright plan .",
    "Wright will build \"HN Daily Digest\".",
    "In your words: Daily digest of top HN stories, summarized, emailed at 8am.",
    "",
    "Fetcher will: Fetch top 30 HN stories: id, title, url, points (writes stories.json).",
    "  Fetcher may use the network.",
    "  Fetcher reads live data, so it runs fresh every time.",
    "Summarizer will: Write a punchy two-sentence summary per story",
    "  (reads stories.json, writes summaries.json).",
    "  Summarizer is fully sandboxed: no files outside its outputs, no network, no programs.",
    "Mailer will: Compose and send the digest email (reads summaries.json, writes receipt.eml).",
    "  Mailer touches the outside world. It runs only after the checks pass, and is",
    "  never retried. It will ask you before each send.",
    "",
    "Before anything leaves your machine, Testwright writes and runs checks and",
    "Reviewer must approve the work.",
    "It runs every day at 8am (Asia/Kolkata), while Wright's background service is running.",
    "It spends at most $0.50 per run and $3.00 per day, and stops after 300 seconds",
    "or 40 model turns.",
    "Done means: Mail received 08:00-08:05 local, every story present, no dead links.",
];

fn blueprint() -> WidgetNode {
    section(
        "blueprint",
        vec![
            section_head(
                "THE BLUEPRINT",
                "One file is the whole project. The plan is read, not interpreted.",
                "The blueprint is typed and validated at the gate — agents, edges, chips, \
                 guardrails, deliverables — and diffable by structure rather than text. The plan \
                 beside it is generated from that model by code: the same file, in plain words, \
                 before anything runs.",
            ),
            spacer(44.0),
            reveal(Styled::class(
                "wr-grid2",
                Flex::column().spacing(16.0).children(children![
                    code_block("blueprint.kdl", "kdl", KDL_SAMPLE),
                    terminal(
                        "the confirm gate — generated from the blueprint, never by a model",
                        PLAN_SAMPLE,
                    ),
                ]),
            )),
        ],
    )
}

// ─── the cache ────────────────────────────────────────────────────────────

fn cache() -> WidgetNode {
    let diff = [
        "$ wright diff",
        "This change refires Summarizer + Mailer + Testwright + Reviewer.",
        "Fetcher is fresh every run anyway (live data or delivery).",
    ];
    section(
        "cache",
        vec![
            section_head(
                "THE CACHE",
                "Refire, not re-run.",
                "Change one line and only the affected work runs again. Every task's inputs are \
                 hashed into its cache key; a task whose inputs did not change is served from \
                 cache. The freshness rules are the product's own honesty: live-data steps and \
                 delivery steps are never served from cache, whatever the key says — which is \
                 what makes the 8am digest affordable: one Fetcher, and the rest is yesterday's \
                 work with today's stories.",
            ),
            spacer(44.0),
            reveal(Styled::block(
                "display:flex;justify-content:center",
                terminal("wright diff — what an edit refires, before anything runs", &diff),
            )),
        ],
    )
}

// ─── features ─────────────────────────────────────────────────────────────

fn card(tag: &str, title: &str, body: &str) -> WidgetNode {
    reveal(Styled::class(
        "wr-card",
        Container::new().padding(EdgeInsets::all(24.0)).child(col(
            9.0,
            children![
                mark_bare(22.0, ACCENT_INK),
                text(
                    tag,
                    mono(10.0, ACCENT_INK)
                        .letter_spacing(1.5)
                        .weight(FontWeight::Medium),
                ),
                text(
                    title,
                    sans(16.5, INK).weight(FontWeight::Bold).line_height(1.3),
                ),
                text(body, sans(14.0, INK_2)),
            ]
            .into_iter()
            .collect(),
        )),
    ))
}

fn features() -> WidgetNode {
    section(
        "features",
        vec![
            section_head(
                "WHAT IS IN IT",
                "An agent harness with the receipts built in.",
                "Six of the product's own claims, each one a feature the repository can point at.",
            ),
            spacer(44.0),
            Styled::class(
                "wr-grid2",
                Flex::column().spacing(16.0).children(children![
                    card(
                        "SANDBOX",
                        "Landlock and a network namespace",
                        "Tools run inside it on Linux — files outside a step's outputs, the \
                         network, other programs, all refused. On macOS and Windows the sandboxed \
                         tools refuse loudly rather than run unprotected, which is the honest \
                         failure.",
                    ),
                    card(
                        "MODELS",
                        "Built in. No key, no account",
                        "Qwen3-1.7B for text and tool calls, Whisper base for your voice, both \
                         run in-process by Wright itself. Or bring a provider: Anthropic, \
                         OpenAI-compatible, DeepSeek, GLM, Kimi, xAI, or a local server.",
                    ),
                    card(
                        "RECEIPTS",
                        "Durable runs, resume after a crash",
                        "Every run is a fsynced event log with BLAKE3 content-addressed objects. \
                         wright resume continues a run that crashed or was stopped; wright replay \
                         reconstructs a past run from its log, at any point, without re-executing \
                         anything.",
                    ),
                    card(
                        "GUARDRAILS",
                        "Enforced at compile time, never at 3AM",
                        "A budget per firing and per day in micro-USD, a wall clock, a turn \
                         limit — compiled into the plan, not checked after the money is spent. \
                         The Confirm Gate says what it will spend before you approve it.",
                    ),
                    card(
                        "SCHEDULES",
                        "cron, in your own time zone",
                        "Chips carry the schedule — cron plus an IANA zone, resolved through the \
                         OS's own TZif. A background daemon fires them, and the systemd or \
                         launchd unit installer puts it in the right place for the machine.",
                    ),
                    card(
                        "SECRETS",
                        "The OS keychain, by reference",
                        "security on macOS, secret-tool on Linux, environment variables for CI. \
                         The blueprint names a secret; the value never crosses a prompt, a log, \
                         or the config file. wright secret env prints the variable name to set.",
                    ),
                ]),
            )
            .into(),
        ],
    )
}

// ─── get started ──────────────────────────────────────────────────────────

fn get_started() -> WidgetNode {
    const QUICKSTART: &[&str] = &[
        "$ cargo build --release",
        "$ ./target/release/wright doctor",
        "$ ./target/release/wright model pull        # the built-in models, once",
        "$ ./target/release/wright new .             # describe it; confirm the plan",
        "$ ./target/release/wright run . --yes       # fire it; watch the event river",
        "$ ./target/release/wright run . --yes       # again: unchanged work is cached",
    ];
    section(
        "start",
        vec![
            section_head(
                "GET STARTED",
                "No key needed for the first project.",
                "The built-in models download on first use — 1.9 GB, once, resumable, checked \
                 against pinned SHA-256s — or ahead of time with wright model pull. With no key \
                 set up, a model with no provider falls back to the built-in qwen3-1.7b, and the \
                 event river says so.",
            ),
            spacer(44.0),
            reveal(Styled::block(
                "display:flex;justify-content:center",
                terminal("the README's own quick start", QUICKSTART),
            )),
        ],
    )
}

// ─── downloads ────────────────────────────────────────────────────────────

fn downloads(host: Option<Os>) -> WidgetNode {
    let asset_row = |label: &str, asset: &str| -> WidgetNode {
        Tag::link(
            url_for(asset),
            Styled::class(
                "wr-row",
                Container::new()
                    .padding(EdgeInsets::symmetric(20.0, 15.0))
                    .child(
                        Flex::row()
                            .main_axis_alignment(MainAxisAlignment::SpaceBetween)
                            .cross_axis_alignment(CrossAxisAlignment::Center)
                            .children(children![
                                col(
                                    3.0,
                                    children![text(label, sans(14.5, INK))]
                                        .into_iter()
                                        .collect(),
                                )
                                .cross_axis_alignment(CrossAxisAlignment::Start),
                                row(
                                    10.0,
                                    children![
                                        text("Pending", mono(11.5, INK_3)),
                                        Styled::css(
                                            "border:1px solid rgba(var(--glass),0.12);\
                                             border-radius:999px;padding:2px 9px",
                                            text("release", mono(10.0, ACCENT_INK)),
                                        ),
                                    ]
                                    .into_iter()
                                    .collect(),
                                ),
                            ]),
                    ),
            ),
        )
        .into()
    };
    let primary = match host {
        Some(Os::MacOs) => "Download for macOS",
        Some(Os::Windows) => "Download for Windows",
        _ => "Download for Linux",
    };
    let preferred = preferred_file(host);
    section(
        "download",
        vec![
            section_head(
                "DOWNLOAD",
                "One binary. Every platform its own installer.",
                "The headless binary has zero external crates; the window adds exactly one \
                 dependency — vieww. Everything Wright remembers lives in ~/.wright, and \
                 uninstalling never touches it or your projects.",
            ),
            spacer(38.0),
            Styled::block(
                "display:flex;justify-content:center",
                button(primary, &url_for(preferred), true),
            )
            .into(),
            spacer(36.0),
            reveal(Styled::css(
                "border:1px solid rgba(var(--glass),0.10);border-radius:16px;overflow:hidden;\
                 background:rgba(var(--glass),0.02)",
                Styled::class(
                    "wr-rows",
                    col(
                        0.0,
                        ASSETS
                            .iter()
                            .map(|asset| asset_row(asset.label, asset.file))
                            .collect(),
                    ),
                ),
            )),
        ],
    )
}

/// The build to put on the big button.
fn preferred_file(host: Option<Os>) -> &'static str {
    match host {
        Some(Os::MacOs) => "Wright-macos-universal.dmg",
        Some(Os::Windows) => "Wright-Setup-x64.exe",
        _ => "wright_amd64.deb",
    }
}

// ─── the closing call ─────────────────────────────────────────────────────

fn closing(host: Option<Os>) -> WidgetNode {
    let primary = match host {
        Some(Os::MacOs) => "Download for macOS",
        Some(Os::Windows) => "Download for Windows",
        _ => "Download for Linux",
    };
    Tag::new(
        "section",
        Styled::css(
            "padding-top:40px;padding-bottom:120px",
            column(reveal(Styled::class(
                "wr-closing",
                Flex::column()
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .spacing(0.0)
                    .children(children![
                        mark(44.0),
                        spacer(22.0),
                        heading("h2", "You write it. Agents wright it.", 36.0, TextAlign::Center),
                        Styled::class(
                            "wr-gradient-text",
                            heading("div", "The artifact is wrought.", 36.0, TextAlign::Center),
                        ),
                        spacer(14.0),
                        Styled::css(
                            "max-width:560px;margin:0 auto",
                            Text::new(
                                "Wright (Old English: maker) — MIT, one dependency for the \
                                 window, built by the same framework it runs on."
                            )
                            .style(sans(16.0, INK_2))
                            .align(TextAlign::Center),
                        ),
                        spacer(28.0),
                        Flex::row()
                            .main_axis_size(MainAxisSize::Min)
                            .main_axis_alignment(MainAxisAlignment::Center)
                            .spacing(12.0)
                            .children(children![
                                button(primary, &url_for(preferred_file(host)), true),
                                button("Read the plan first", "#flow", false),
                            ]),
                    ]),
            ))),
        ),
    )
    .into()
}

// ─── the nav ──────────────────────────────────────────────────────────────

/// Sticky, translucent, and blurred over whatever is behind it.
fn nav() -> WidgetNode {
    let link = |label: &str, href: &'static str| -> WidgetNode {
        Tag::link(
            href.to_string(),
            Styled::class(
                "wr-navlink",
                text(label, sans(14.0, INK_2).weight(FontWeight::Medium)),
            ),
        )
        .into()
    };
    Tag::new(
        "nav",
        Styled::class(
            "wr-nav",
            column(Styled::css(
                "height:64px",
                Flex::row()
                    .main_axis_alignment(MainAxisAlignment::SpaceBetween)
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .children(children![
                        Tag::link(
                            "#top",
                            Styled::css(
                                "text-decoration:none",
                                row(
                                    10.0,
                                    children![
                                        mark(22.0),
                                        text("Wright", sans(15.0, INK).weight(FontWeight::Bold)),
                                        Styled::css(
                                            "border:1px solid rgba(var(--glass),0.12);\
                                             border-radius:999px;padding:2px 8px",
                                            text("0.5.1", mono(10.5, INK_3)),
                                        ),
                                    ]
                                    .into_iter()
                                    .collect(),
                                ),
                            ),
                        ),
                        Styled::class(
                            "wr-navlinks",
                            row(
                                28.0,
                                children![
                                    link("Flow", "#flow"),
                                    link("Window", "#window"),
                                    link("Demo", "#demo"),
                                    link("Blueprint", "#blueprint"),
                                    link("Download", "#download"),
                                ]
                                .into_iter()
                                .collect(),
                            ),
                        ),
                        button("Download", "#download", true),
                    ]),
            )),
        ),
    )
    .into()
}

// ─── the footer ───────────────────────────────────────────────────────────

fn footer() -> WidgetNode {
    let group = |title: &str, links: Vec<(&str, String)>| -> WidgetNode {
        col(
            10.0,
            std::iter::once(text(
                title,
                mono(10.0, INK_3)
                    .letter_spacing(1.6)
                    .weight(FontWeight::Medium),
            ))
            .chain(links.into_iter().map(|(label, href)| {
                Tag::link(
                    href,
                    Styled::class("wr-foot", text(label, sans(14.0, INK_2))),
                )
                .into()
            }))
            .collect(),
        )
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .into()
    };
    Tag::new(
        "footer",
        Styled::css(
            "border-top:1px solid rgba(var(--glass),0.08);padding:56px 0 44px",
            column(col(
                40.0,
                children![
                    Styled::class(
                        "wr-footgrid",
                        Flex::row()
                            .cross_axis_alignment(CrossAxisAlignment::Start)
                            .children(children![
                                col(
                                    12.0,
                                    children![
                                        row(
                                            10.0,
                                            children![
                                                mark(24.0),
                                                text(
                                                    "Wright",
                                                    sans(15.0, INK).weight(FontWeight::Bold),
                                                ),
                                            ]
                                            .into_iter()
                                            .collect(),
                                        ),
                                        Styled::css(
                                            "max-width:260px",
                                            text(
                                                "Describe the outcome; agents build, check and \
                                                 deliver it.",
                                                sans(13.5, INK_3),
                                            ),
                                        ),
                                    ]
                                    .into_iter()
                                    .collect(),
                                )
                                .cross_axis_alignment(CrossAxisAlignment::Start),
                                group(
                                    "WRIGHT",
                                    vec![
                                        ("Download", "#download".into()),
                                        ("Get started", "#start".into()),
                                        ("Live demo", "#demo".into()),
                                    ],
                                ),
                                group(
                                    "HOW IT WORKS",
                                    vec![
                                        ("The flow", "#flow".into()),
                                        ("The firing", "#firing".into()),
                                        ("The blueprint", "#blueprint".into()),
                                        ("The cache", "#cache".into()),
                                    ],
                                ),
                                group(
                                    "PROJECT",
                                    vec![
                                        ("Source", format!("https://github.com/{}", crate::repo())),
                                        (
                                            "Releases",
                                            format!("https://github.com/{}/releases", crate::repo()),
                                        ),
                                        (
                                            "Issues",
                                            format!("https://github.com/{}/issues", crate::repo()),
                                        ),
                                    ],
                                ),
                            ]),
                    ),
                    Styled::css(
                        "border-top:1px solid rgba(var(--glass),0.06);padding-top:22px",
                        Styled::class(
                            "wr-footbottom",
                            Flex::row()
                                .main_axis_alignment(MainAxisAlignment::SpaceBetween)
                                .cross_axis_alignment(CrossAxisAlignment::Center)
                                .children(children![
                                    text("Wright 0.5.1 — MIT", mono(11.5, INK_3)),
                                    text(
                                        "Built with vieww. This page is a vieww widget tree, rendered to DOM.",
                                        mono(11.5, INK_3),
                                    ),
                                ]),
                        ),
                    ),
                ]
                .into_iter()
                .collect(),
            )),
        ),
    )
    .into()
}

// ─── the page ─────────────────────────────────────────────────────────────

fn divider() -> WidgetNode {
    Styled::empty(
        "max-width:1280px;margin:0 auto;width:100%;height:1px;\
         background:linear-gradient(90deg,transparent,rgba(var(--glass),0.09),transparent)",
    )
    .into()
}

/// The whole thing.
#[must_use]
pub fn page(host: Option<Os>) -> WidgetNode {
    Styled::css(
        "background:var(--ground);color:var(--ink);min-height:100vh",
        col(
            0.0,
            children![
                nav(),
                hero(host),
                Tag::new(
                    "main",
                    col(
                        0.0,
                        children![
                            flow(),
                            divider(),
                            window_section(),
                            divider(),
                            demo(),
                            divider(),
                            firing(),
                            divider(),
                            blueprint(),
                            divider(),
                            cache(),
                            divider(),
                            features(),
                            divider(),
                            get_started(),
                            divider(),
                            downloads(host),
                            closing(host),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                ),
                footer(),
            ]
            .into_iter()
            .collect(),
        ),
    )
    .into()
}
