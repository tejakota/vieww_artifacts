//! The first page (Home) and the last (a new session).
//!
//! ```text
//!  [Home] [session] [session] ... [new session]
//!    ^ what should exist?           ^ swipe past the last one to get here
//!      your sessions, Saved           and describe the next thing
//! ```

use std::rc::Rc;

use vieww::prelude::*;

use super::kit::*;
use super::thread::{composer, tone_color};
use super::Ui;

const STARTERS: [(&str, &str, &str); 3] = [
    ("Daily digest", "Fetch, summarize, email on a schedule", "Every morning at 8, fetch the top Hacker News stories, summarize them, and email me the digest."),
    ("A tested CLI tool", "Code, tests, logging and CI in place", "A command line tool that counts words in files, with tests, logging and CI."),
    ("Weekly report", "From a folder of CSV files", "Every Monday, read the CSV files in my sales folder and email me a one-page summary."),
];

pub fn home(ui: &Ui) -> WidgetNode {
    let ui2 = ui.clone();
    let ui = ui.clone();
    let content: WidgetNode = Live::new("home", move || {
        let p = ui.pal();
        let send: Rc<dyn Fn()> = {
            let ui = ui.clone();
            Rc::new(move || ui.start_session_from(&ui.home))
        };
        let starters: Vec<WidgetNode> = STARTERS
            .iter()
            .map(|(title, sub, text)| {
                let prompt = ui.home.prompt.clone();
                let (title, sub, text) = (title.to_string(), sub.to_string(), text.to_string());
                grow(Pressable::sensed(move |s: Sense| {
                    Container::new()
                        .color(p.surface.lerp(p.surface2, s.hover))
                        .radius(corner::CARD)
                        .border(Border::new(if s.hover > 0.0 { p.line_strong } else { p.line }, 1.0))
                        .padding(EdgeInsets::all(14.0))
                        .child(column(3.0, vec![b(title.clone(), 14.0, p.ink), t(sub.clone(), 12.5, p.muted)]))
                        .into()
                })
                .on_tap(move || prompt.set(TextEditingValue::new(text.clone()))))
            })
            .collect();

        let sessions = ui.sessions.get();
        let cards: Vec<WidgetNode> = sessions
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let (status, tone) = s.status();
                let share = s.done_share();
                let title = s.title();
                let go = ui.clone();
                let color = tone_color(&p, tone);
                Pressable::sensed(move |st: Sense| {
                    Container::new()
                        .width(330.0)
                        .color(p.surface)
                        .radius(corner::CARD)
                        .border(Border::new(if st.hover > 0.0 { p.line_strong } else { p.line }, 1.0))
                        .padding(EdgeInsets::all(16.0))
                        .child(stretch(6.0, vec![
                            line(title.clone(), 15.0, p.ink),
                            line(status.clone(), 12.5, color),
                            Container::new().height(4.0).radius(2.0).color(p.surface2).alignment(Alignment::CENTER_LEFT).child(
                                Container::new().width(298.0 * share).height(4.0).radius(2.0).color(color),
                            ).into(),
                        ]))
                        .into()
                })
                .on_tap(move || go.go(i + 1))
                .into()
            })
            .collect();

        let mut items = vec![
            SizedBox::height(28.0).into(),
            Text::new("What should exist?").size(46.0).color(p.ink).weight(FontWeight::Bold).into(),
            para("Describe the outcome in your own words. Wright drafts a plan, shows what it will touch and what it costs, and runs nothing until you approve.", 16.0, p.muted),
            composer(&ui, &ui.home, send, "Every Monday, read the CSVs in my sales folder and email me a one-page summary", true),
            row(10.0, starters),
        ];
        if !cards.is_empty() {
            items.push(row(10.0, vec![b("SESSIONS", 11.5, p.muted), t("Swipe sideways with two fingers to move between them", 12.5, p.muted)]));
            items.push(wrap_rows(cards, 2));
        }
        let saved = ui.saved.get();
        if !saved.is_empty() {
            items.push(row(10.0, vec![b("SAVED", 11.5, p.muted), t("Closed sessions keep their history and schedules. Open one to bring it back.", 12.5, p.muted)]));
            items.push(saved_shelf(&ui, &saved));
        }
        stretch(18.0, items)
    })
    .into();
    scroll_area(&ui2, &ui2.home_scroll, pad(EdgeInsets::only(32.0, 12.0, 32.0, 60.0), reading(720.0, content)))
}

/// Cards in rows of `per`.
fn wrap_rows(items: Vec<WidgetNode>, per: usize) -> WidgetNode {
    let mut rows = Vec::new();
    let mut it = items.into_iter().peekable();
    while it.peek().is_some() {
        rows.push(row(12.0, it.by_ref().take(per).collect()));
    }
    column(12.0, rows)
}

/// Saved sessions as chips: tap to reopen.
pub fn saved_shelf(ui: &Ui, saved: &[std::path::PathBuf]) -> WidgetNode {
    let p = ui.pal();
    let chips: Vec<WidgetNode> = saved
        .iter()
        .take(8)
        .map(|path| {
            let name = path.file_name().map(|n| n.to_string_lossy().replace('-', " ")).unwrap_or_default();
            let (u, path) = (ui.clone(), path.clone());
            Pressable::sensed(move |s: Sense| {
                Container::new()
                    .color(p.surface.lerp(p.surface2, s.hover))
                    .radius(16.0)
                    .border(Border::new(p.line_strong, 1.0))
                    .padding(EdgeInsets::only(10.0, 5.0, 12.0, 5.0))
                    .child(row(6.0, vec![Glyph::Restore.draw(14.0, p.accent_ink), t(name.clone(), 12.5, p.ink)]))
                    .into()
            })
            .on_tap(move || u.reopen(&path))
            .into()
        })
        .collect();
    wrap_rows(chips, 4)
}

/// The page past the last session: start something new.
pub fn fresh(ui: &Ui) -> WidgetNode {
    let ui = ui.clone();
    Live::new("fresh", move || {
        let p = ui.pal();
        let pull = (-ui.drag.get() / 160.0).clamp(0.0, 1.0);
        let send: Rc<dyn Fn()> = {
            let ui = ui.clone();
            Rc::new(move || ui.start_session_from(&ui.fresh))
        };
        let ring = Container::new()
            .width(72.0 + 10.0 * pull)
            .height(72.0 + 10.0 * pull)
            .radius(41.0)
            .color(p.ground.lerp(p.accent_soft, pull))
            .border(Border::new(p.line_strong.lerp(p.accent, pull), 1.5))
            .alignment(Alignment::CENTER)
            .child(Glyph::Plus.draw(26.0, p.muted.lerp(p.accent_ink, pull)));
        pad(
            EdgeInsets::only(32.0, 60.0, 32.0, 40.0),
            reading(680.0, stretch(18.0, vec![
                Center::new().child(ring).into(),
                Center::new().child(Text::new("A new session").size(30.0).color(p.ink).weight(FontWeight::Bold)).into(),
                Center::new().child(t("Describe what should exist. It gets its own folder, its own plan and its own history.", 14.0, p.muted)).into(),
                composer(&ui, &ui.fresh, send, "Describe what should exist", true),
            ])),
        )
    })
    .into()
}
