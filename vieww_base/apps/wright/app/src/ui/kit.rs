//! Small building blocks every screen uses: text, buttons, cards, the
//! stroke icons, a reactive region ([`Live`]) and the keyboard layer
//! ([`Keys`]).
//!
//! Rule of the house: a screen function reads like a description of the
//! screen. Anything that would make it read like layout arithmetic lives
//! here instead.

use std::rc::Rc;
use std::time::Duration;

use vieww::foundation::KeyEvent;
use vieww::prelude::*;
use vieww::render::{LayoutCtx, RenderObject};
use vieww::widget::BuildContext as Ctx;

use super::theme::Pal;

// ── text ─────────────────────────────────────────────────────────

pub fn t(s: impl Into<String>, size: f32, color: Color) -> WidgetNode {
    Text::new(s).size(size).color(color).into()
}

pub fn b(s: impl Into<String>, size: f32, color: Color) -> WidgetNode {
    Text::new(s)
        .size(size)
        .color(color)
        .weight(FontWeight::Bold)
        .into()
}

/// One line that ends in an ellipsis instead of overflowing.
pub fn line(s: impl Into<String>, size: f32, color: Color) -> WidgetNode {
    Text::new(s)
        .size(size)
        .color(color)
        .max_lines(1)
        .overflow(vieww::text::TextOverflow::Ellipsis)
        .into()
}

/// Takes the rest of its row, one line, ellipsis.
pub fn fill(s: impl Into<String>, size: f32, color: Color) -> WidgetNode {
    grow(line(s, size, color))
}

/// Wrapping paragraph text.
pub fn para(s: impl Into<String>, size: f32, color: Color) -> WidgetNode {
    Text::new(s).size(size).color(color).into()
}

pub fn mono(s: impl Into<String>, size: f32, color: Color) -> WidgetNode {
    Text::new(s)
        .style(
            TextStyle::new(size)
                .family(vieww::foundation::FontFamily::Monospace)
                .color(color),
        )
        .max_lines(1)
        .overflow(vieww::text::TextOverflow::Ellipsis)
        .into()
}

// ── layout ───────────────────────────────────────────────────────

pub fn column(spacing: f32, items: Vec<WidgetNode>) -> WidgetNode {
    Flex::column()
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .spacing(spacing)
        .children(items)
        .into()
}

/// A column whose children are as wide as it is.
pub fn stretch(spacing: f32, items: Vec<WidgetNode>) -> WidgetNode {
    Flex::column()
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .spacing(spacing)
        .children(items)
        .into()
}

pub fn row(spacing: f32, items: Vec<WidgetNode>) -> WidgetNode {
    Flex::row()
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .spacing(spacing)
        .children(items)
        .into()
}

pub fn grow(child: impl Into<WidgetNode>) -> WidgetNode {
    Flexible::expanded(1).child(child).into()
}

pub fn gap() -> WidgetNode {
    grow(SizedBox::new())
}

pub fn sized(w: f32, h: f32, child: impl Into<WidgetNode>) -> WidgetNode {
    SizedBox::from_size(Size::new(w, h)).child(child).into()
}

pub fn pad(insets: EdgeInsets, child: impl Into<WidgetNode>) -> WidgetNode {
    Container::new().padding(insets).child(child).into()
}

/// Centre `child` horizontally at most `max` wide (the reading column).
pub fn reading(max: f32, child: impl Into<WidgetNode>) -> WidgetNode {
    Align::new(Alignment::TOP_CENTER)
        .child(Constrained::new(Constraints::new(0.0, max, 0.0, f32::INFINITY)).child(child))
        .into()
}

/// How much of the room the chat takes: a chat app's share of the pane,
/// not a reading column's.
const CHAT_FILL: f32 = 0.86;
/// The ceiling on that share: past it a line of prose runs too long to
/// follow, however wide the window grows.
const CHAT_MAX: f32 = 1160.0;

/// The chat column — the thread and the composer under it — at the
/// width a chat app uses: most of the window, not a fixed reading
/// measure floating in margins. A [`LayoutBuilder`] asks the pane how
/// much room it has and sizes the column from the answer, so the
/// conversation widens with the window and stops at the ceiling. No
/// breakpoints: the width is geometry, used exactly, which is the case
/// the builder rebuilds on every change by design.
pub fn chat_column(child: impl Into<WidgetNode>) -> WidgetNode {
    let child = child.into();
    LayoutBuilder::new(move |c: Constraints| {
        reading((c.max_width * CHAT_FILL).min(CHAT_MAX), child.clone())
    })
    .into()
}

pub fn fade(alpha: f32, child: impl Into<WidgetNode>) -> WidgetNode {
    Opacity::new(alpha).child(child).into()
}

pub fn shift(dx: f32, dy: f32, child: impl Into<WidgetNode>) -> WidgetNode {
    Transformed::translate(Offset::new(dx, dy))
        .child(child)
        .into()
}

pub fn at(left: f32, top: f32, w: f32, h: f32, child: impl Into<WidgetNode>) -> WidgetNode {
    Positioned::new()
        .left(left)
        .top(top)
        .width(w)
        .height(h)
        .child(child)
        .into()
}

pub fn layer(child: impl Into<WidgetNode>) -> WidgetNode {
    Positioned::fill().child(child).into()
}

pub fn stack(items: Vec<WidgetNode>) -> WidgetNode {
    Stack::new().fit(StackFit::Expand).children(items).into()
}

/// `child` with clicks passing through it (`ignore == true`).
pub fn inert(ignore: bool, child: impl Into<WidgetNode>) -> WidgetNode {
    IgnorePointer::new().ignoring(ignore).child(child).into()
}

/// Ease from `from` to `to` as `t` goes 0..1, clamped.
pub fn mix(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t.clamp(0.0, 1.0)
}

/// `t` remapped so `a..b` becomes `0..1` (clamped): one phase of a timeline.
pub fn phase(t: f32, a: f32, b: f32) -> f32 {
    ((t - a) / (b - a)).clamp(0.0, 1.0)
}

pub fn ease(t: f32) -> f32 {
    // Smoothstep: gentle in, gentle out.
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// ── surfaces ─────────────────────────────────────────────────────

/// Corner radii, Google-style: generous on surfaces, fully round on controls.
pub mod corner {
    /// Cards, panels, menus.
    pub const CARD: f32 = 16.0;
    /// Large surfaces: dialogs, the settings panel, overview cards.
    pub const PANEL: f32 = 22.0;
    /// Text inputs and boxes inside cards.
    pub const FIELD: f32 = 12.0;
    /// Small fills: rows in a menu, tabs, status-bar buttons.
    pub const SMALL: f32 = 10.0;
    /// Pill-shaped controls (buttons, the composer): half their height.
    pub const PILL: f32 = 18.0;
    /// Chips and badges, about 20 px tall.
    pub const CHIP: f32 = 11.0;
}

/// Rule of the house for every box: **nothing it holds is drawn outside
/// it.** Text that can be long is one line with an ellipsis (`line`,
/// `fill`, `pill`) or wraps inside its width (`para`); a slot is never a
/// fixed size around text; and cards clip as a last resort, so a row that
/// is wider than the card is cut at the card's edge, never painted over
/// its neighbours.
pub fn card(p: &Pal, child: impl Into<WidgetNode>) -> WidgetNode {
    Clip::rounded(corner::CARD)
        .child(
            Container::new()
                .color(p.surface)
                .radius(corner::CARD)
                .border(Border::new(p.line, 1.0))
                .padding(EdgeInsets::symmetric(20.0, 18.0))
                .child(child),
        )
        .into()
}

pub fn tinted(fill: Color, edge: Color, child: impl Into<WidgetNode>) -> WidgetNode {
    Clip::rounded(corner::CARD)
        .child(
            Container::new()
                .color(fill)
                .radius(corner::CARD)
                .border(Border::new(edge, 1.0))
                .padding(EdgeInsets::symmetric(20.0, 18.0))
                .child(child),
        )
        .into()
}

/// A chip: one line, never wrapping — a pill sizes to its words, and when
/// the room is smaller than the words it ends in an ellipsis instead of
/// growing a second line out of its rounded box.
pub fn pill(label: impl Into<String>, fg: Color, bg: Color) -> WidgetNode {
    Container::new()
        .color(bg)
        .radius(corner::CHIP)
        .padding(EdgeInsets::symmetric(10.0, 3.0))
        .child(line(label, 11.5, fg))
        .into()
}

/// A box at least `min` wide, as wide as its content otherwise — the
/// right way to line up a column of marks or labels (a fixed-size box
/// lets anything longer than the guess spill out of it).
pub fn at_least(min: f32, child: impl Into<WidgetNode>) -> WidgetNode {
    Constrained::new(Constraints::new(min, f32::INFINITY, 0.0, f32::INFINITY))
        .child(child)
        .into()
}

// ── copyable text ────────────────────────────────────────────────

/// Which text is selected (its hash) and where.
type Selected = Option<(u64, vieww::foundation::TextSelection)>;

thread_local! {
    /// The one text selection in the window (like a browser: selecting in
    /// one place clears the other), keyed by the selected text's hash.
    static SELECTION: std::cell::RefCell<Option<Signal<Selected>>> = const { std::cell::RefCell::new(None) };
    /// The pasteboard the window was given (the system one; a memory one
    /// headless), for the Copy buttons.
    static PASTEBOARD: std::cell::RefCell<Option<Rc<dyn vieww::foundation::Clipboard>>> = const { std::cell::RefCell::new(None) };
}

/// Set up copyable text for this thread's window.
pub fn init_copy(rt: &vieww::element::Runtime, clipboard: Rc<dyn vieww::foundation::Clipboard>) {
    SELECTION.with_borrow_mut(|s| *s = Some(rt.signal(None)));
    PASTEBOARD.with_borrow_mut(|c| *c = Some(clipboard));
}

/// Put `text` on the pasteboard. False when there is none.
pub fn copy_text(text: &str) -> bool {
    PASTEBOARD.with_borrow(|c| c.as_ref().is_some_and(|c| c.write_text(text).is_ok()))
}

fn text_key(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// Wrapping text a person can select with the pointer and copy (Ctrl/Cmd+C,
/// or Ctrl/Cmd+A then C). Use it for anything worth keeping — replies,
/// plans, errors, paths, results. Not inside something that is itself
/// tapped or dragged: the text takes the pointer.
pub fn copyable(s: impl Into<String>, size: f32, color: Color) -> WidgetNode {
    let text: String = s.into();
    let Some(sel) = SELECTION.with_borrow(|s| s.clone()) else {
        return para(text, size, color);
    };
    let key = text_key(&text);
    Live::new("copyable", move || {
        let mut value = TextEditingValue::new(text.clone());
        value.selection = match sel.get() {
            Some((k, range)) if k == key => range,
            _ => vieww::foundation::TextSelection::collapsed(0),
        };
        let (on_change, on_select, same) = (sel.clone(), sel.clone(), text.clone());
        TextField::new(value)
            .read_only()
            .show_cursor(false)
            .size(size)
            .color(color)
            .cursor(color, 0.0)
            .selection_color(color.with_alpha(60))
            .on_changed(Rc::new(move |v: TextEditingValue| {
                // Selection and caret moves only: the field is read-only.
                if v.text == same {
                    on_change.set(Some((key, v.selection)));
                }
            }))
            .on_selection(Rc::new(move |range: vieww::foundation::TextSelection| {
                on_select.set(Some((key, range)))
            }))
            .into()
    })
    .into()
}

/// One line of copyable monospace (a path, a hash, a command).
pub fn copyable_mono(s: impl Into<String>, size: f32, color: Color) -> WidgetNode {
    let text: String = s.into();
    row(
        6.0,
        vec![
            grow(mono(text.clone(), size, color)),
            copy_icon(color, text),
        ],
    )
}

// ── rendered markdown ────────────────────────────────────────────

/// A reply's words, rendered: **bold**, `code`, headings, lists, quotes
/// and fenced blocks — the framework's own [`Markdown`] widget, wearing
/// Wright's palette.
///
/// Smaller models answer in markdown whether or not they were asked to,
/// and a wall of `**asterisks**` where a sentence should be reads as a
/// machine talking at you. This renders what they meant. Plain prose
/// renders exactly as [`para`] would — same size, same ink — so nothing
/// changes for the models that already write plainly.
///
/// The framework's widget reads its colours from the theme it is built
/// under, and the app carries its own palette rather than a
/// [`ThemeData`](vieww_widget::ThemeData) — so the one place the two
/// meet, this function publishes the app's palette as the theme for the
/// markdown's subtree. Inside it, `on_surface` *is* `p.ink` and
/// `surface_variant` *is* `p.surface2`; the two systems cannot drift
/// because this is the only bridge.
pub fn markdown(p: &Pal, text: impl Into<String>) -> WidgetNode {
    let scheme = ColorScheme {
        primary: p.accent,
        on_primary: p.on_accent,
        surface: p.ground,
        on_surface: p.ink,
        surface_variant: p.surface2,
        on_surface_variant: p.muted,
        outline: p.line,
        error: p.bad,
        on_error: Color::WHITE,
        success: p.good,
        on_success: Color::BLACK,
    };
    // The body style the thread's replies already wore: 15 px, the
    // paragraph's own line height, in the theme's ink.
    let mut data = ThemeData::from_colors(scheme);
    data.text = Typography::scale(p.ink);
    data.text.body = TextStyle::new(15.0).color(p.ink).line_height(1.45);
    Theme::new(data).child(Markdown::new(text)).into()
}

/// A small "copy" affordance: puts `text` on the pasteboard.
pub fn copy_icon(color: Color, text: String) -> WidgetNode {
    Semantics::new()
        .label("Copy".to_string())
        .child(
            Pressable::sensed(move |s: Sense| {
                Container::new()
                    .padding(EdgeInsets::symmetric(6.0, 2.0))
                    .radius(corner::SMALL)
                    .color(color.with_alpha((s.hover * 30.0) as u8))
                    .child(line("Copy", 11.5, color))
                    .into()
            })
            .on_tap(move || {
                copy_text(&text);
            }),
        )
        .into()
}

pub fn divider(p: &Pal) -> WidgetNode {
    Container::new().height(1.0).color(p.line).into()
}

// ── buttons ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Primary,
    Secondary,
    Quiet,
}

/// A labelled button. `enabled == false` draws it faded and ignores taps.
pub fn button(
    p: &Pal,
    label: &str,
    look: Look,
    enabled: bool,
    on_tap: impl Fn() + 'static,
) -> WidgetNode {
    let p = *p;
    let label = label.to_string();
    let body = Pressable::sensed(move |s: Sense| {
        let (bg, fg, edge) = match look {
            Look::Primary => (p.accent, p.on_accent, p.accent),
            Look::Secondary => (p.surface2, p.ink, p.line_strong),
            Look::Quiet => (Color::rgba(0, 0, 0, 0), p.muted, Color::rgba(0, 0, 0, 0)),
        };
        let bg = if s.hover > 0.0 && look != Look::Primary {
            p.surface2.lerp(p.line_strong, 0.35 * s.hover)
        } else {
            bg
        };
        let bg = if look == Look::Primary {
            bg.lerp(Color::hex(0x000000), 0.12 * s.press)
        } else {
            bg
        };
        Container::new()
            .color(bg)
            .radius(corner::PILL)
            .border(Border::new(edge, 1.0))
            .padding(EdgeInsets::symmetric(18.0, 8.0))
            .child(t(label.clone(), 13.0, fg))
            .into()
    });
    let node: WidgetNode = if enabled {
        body.on_tap(on_tap).into()
    } else {
        body.into()
    };
    fade(if enabled { 1.0 } else { 0.45 }, inert(!enabled, node))
}

/// A square icon button with a tooltip-ish label for screen readers.
pub fn icon_button(
    p: &Pal,
    glyph: Glyph,
    label: &str,
    active: bool,
    on_tap: impl Fn() + 'static,
) -> WidgetNode {
    let p = *p;
    let label = label.to_string();
    Semantics::new()
        .label(label)
        .child(
            Pressable::sensed(move |s: Sense| {
                let (bg, fg) = if active {
                    (p.accent, p.on_accent)
                } else {
                    (
                        p.surface2
                            .lerp(p.line_strong, 0.4 * s.hover)
                            .with_alpha((s.hover * 255.0) as u8),
                        p.ink,
                    )
                };
                Container::new()
                    .width(32.0)
                    .height(32.0)
                    .radius(16.0)
                    .color(bg)
                    .alignment(Alignment::CENTER)
                    .child(glyph.draw(16.0, fg))
                    .into()
            })
            .on_tap(on_tap),
        )
        .into()
}

/// A labelled toolbar button: icon, word, optional badge.
pub fn tool_button(
    p: &Pal,
    glyph: Glyph,
    label: &str,
    badge: Option<String>,
    active: bool,
    on_tap: impl Fn() + 'static,
) -> WidgetNode {
    let p = *p;
    let label = label.to_string();
    Pressable::sensed(move |s: Sense| {
        let (bg, fg, edge) = if active {
            (p.accent, p.on_accent, p.accent)
        } else {
            (p.surface.lerp(p.surface2, s.hover), p.ink, p.line)
        };
        let mut items = vec![glyph.draw(15.0, fg), t(label.clone(), 12.5, fg)];
        if let Some(n) = &badge {
            items.push(
                Container::new()
                    .color(if active {
                        p.on_accent.with_alpha(60)
                    } else {
                        p.accent_soft
                    })
                    .radius(9.0)
                    .padding(EdgeInsets::symmetric(7.0, 1.0))
                    .child(t(
                        n.clone(),
                        10.5,
                        if active { p.on_accent } else { p.accent_ink },
                    ))
                    .into(),
            );
        }
        Container::new()
            .color(bg)
            .radius(16.0)
            .border(Border::new(edge, 1.0))
            .padding(EdgeInsets::symmetric(14.0, 7.0))
            .child(row(7.0, items))
            .into()
    })
    .on_tap(on_tap)
    .into()
}

/// A tappable region with no look of its own.
pub fn tap(
    child: impl Into<WidgetNode> + Clone + 'static,
    on_tap: impl Fn() + 'static,
) -> WidgetNode {
    let child: WidgetNode = child.into();
    Pressable::new(move |_| child.clone()).on_tap(on_tap).into()
}

// ── icons ────────────────────────────────────────────────────────

/// Stroke icons on a 24-unit grid, drawn (no font, no asset).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Plus,
    Close,
    Send,
    Mic,
    Stop,
    Gear,
    Sessions,
    Engine,
    Done,
    Waiting,
    Queued,
    Skipped,
    Failed,
    Cached,
    File,
    Folder,
    Theme,
    Help,
    Restore,
    Down,
}

impl Glyph {
    pub fn draw(self, side: f32, color: Color) -> WidgetNode {
        CustomPaint::sized(
            Size::new(side, side),
            GlyphPainter {
                glyph: self,
                color,
                turn: 0.0,
            },
        )
        .into()
    }
    /// A spinner arc, rotated by `turn` (0..1).
    pub fn spinner(side: f32, color: Color, turn: f32) -> WidgetNode {
        CustomPaint::sized(
            Size::new(side, side),
            GlyphPainter {
                glyph: Glyph::Queued,
                color,
                turn: turn + 10.0,
            },
        )
        .into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct GlyphPainter {
    glyph: Glyph,
    color: Color,
    /// >= 10 means "spinner at turn - 10".
    turn: f32,
}

fn arc(cx: f32, cy: f32, r: f32, from: f32, to: f32) -> Vec<(f32, f32)> {
    let n = 28;
    (0..=n)
        .map(|i| {
            let a = from + (to - from) * i as f32 / n as f32;
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

fn circle(r: f32) -> Vec<(f32, f32)> {
    arc(12.0, 12.0, r, 0.0, std::f32::consts::TAU)
}

impl GlyphPainter {
    fn strokes(&self) -> Vec<Vec<(f32, f32)>> {
        use std::f32::consts::{PI, TAU};
        if self.turn >= 10.0 {
            let start = (self.turn - 10.0) * TAU;
            return vec![arc(12.0, 12.0, 8.0, start, start + 1.5 * PI)];
        }
        match self.glyph {
            Glyph::Plus => vec![
                vec![(12.0, 5.0), (12.0, 19.0)],
                vec![(5.0, 12.0), (19.0, 12.0)],
            ],
            Glyph::Close => vec![
                vec![(6.0, 6.0), (18.0, 18.0)],
                vec![(18.0, 6.0), (6.0, 18.0)],
            ],
            Glyph::Send => vec![
                vec![(5.0, 12.0), (19.0, 12.0)],
                vec![(13.0, 6.0), (19.0, 12.0), (13.0, 18.0)],
            ],
            Glyph::Mic => {
                let mut capsule = arc(12.0, 7.0, 3.0, PI, TAU);
                capsule.extend(arc(12.0, 12.0, 3.0, 0.0, PI));
                capsule.push(capsule[0]);
                vec![
                    capsule,
                    arc(12.0, 12.0, 6.5, 0.15, PI - 0.15),
                    vec![(12.0, 18.5), (12.0, 21.0)],
                ]
            }
            Glyph::Stop => vec![vec![
                (7.0, 7.0),
                (17.0, 7.0),
                (17.0, 17.0),
                (7.0, 17.0),
                (7.0, 7.0),
            ]],
            Glyph::Down => vec![vec![(6.0, 9.0), (12.0, 15.0), (18.0, 9.0)]],
            Glyph::Gear => {
                let mut v = vec![circle(3.2), circle(7.0)];
                for k in 0..8 {
                    let a = k as f32 * TAU / 8.0;
                    v.push(vec![
                        (12.0 + 7.0 * a.cos(), 12.0 + 7.0 * a.sin()),
                        (12.0 + 9.5 * a.cos(), 12.0 + 9.5 * a.sin()),
                    ]);
                }
                v
            }
            Glyph::Sessions => vec![
                vec![
                    (3.0, 5.0),
                    (10.0, 5.0),
                    (10.0, 19.0),
                    (3.0, 19.0),
                    (3.0, 5.0),
                ],
                vec![
                    (14.0, 5.0),
                    (21.0, 5.0),
                    (21.0, 19.0),
                    (14.0, 19.0),
                    (14.0, 5.0),
                ],
            ],
            Glyph::Engine => vec![
                arc(5.0, 12.0, 2.5, 0.0, TAU),
                arc(19.0, 6.0, 2.5, 0.0, TAU),
                arc(19.0, 18.0, 2.5, 0.0, TAU),
                vec![(7.3, 11.0), (16.7, 7.0)],
                vec![(7.3, 13.0), (16.7, 17.0)],
            ],
            Glyph::Done => vec![circle(9.0), vec![(8.0, 12.5), (10.6, 15.0), (16.0, 9.5)]],
            Glyph::Waiting => vec![circle(9.0), vec![(12.0, 7.5), (12.0, 12.0), (15.0, 14.0)]],
            Glyph::Queued => vec![circle(9.0)],
            Glyph::Skipped => vec![circle(9.0), vec![(8.0, 12.0), (16.0, 12.0)]],
            Glyph::Failed => vec![
                circle(9.0),
                vec![(9.0, 9.0), (15.0, 15.0)],
                vec![(15.0, 9.0), (9.0, 15.0)],
            ],
            Glyph::Cached => vec![
                arc(12.0, 12.0, 8.0, -PI * 0.25, PI * 1.5),
                vec![(17.0, 2.5), (17.7, 6.4), (13.8, 6.9)],
            ],
            Glyph::File => vec![
                vec![
                    (6.0, 3.0),
                    (14.0, 3.0),
                    (18.0, 7.0),
                    (18.0, 21.0),
                    (6.0, 21.0),
                    (6.0, 3.0),
                ],
                vec![(14.0, 3.0), (14.0, 7.0), (18.0, 7.0)],
            ],
            Glyph::Folder => vec![vec![
                (3.0, 6.0),
                (9.0, 6.0),
                (11.0, 8.5),
                (21.0, 8.5),
                (21.0, 19.0),
                (3.0, 19.0),
                (3.0, 6.0),
            ]],
            Glyph::Theme => vec![circle(8.0), vec![(12.0, 4.0), (12.0, 20.0)]],
            Glyph::Help => vec![
                circle(9.0),
                arc(12.0, 9.8, 2.6, PI, TAU + 0.6),
                vec![(13.6, 11.7), (12.0, 13.4), (12.0, 14.4)],
                vec![(12.0, 17.0), (12.0, 17.3)],
            ],
            Glyph::Restore => vec![
                arc(12.0, 12.0, 8.0, PI * 0.75, PI * 2.6),
                vec![(3.5, 9.0), (6.3, 12.6), (9.6, 10.0)],
            ],
        }
    }
}

impl CustomPainter for GlyphPainter {
    fn paint(&self, size: Size) -> Vec<DrawInstruction> {
        let s = size.width.min(size.height) / 24.0;
        let w = 1.9 * s;
        let mut out = Vec::new();
        for path in self.strokes() {
            let pts: Vec<Offset> = path
                .iter()
                .map(|&(x, y)| Offset::new(x * s, y * s))
                .collect();
            for seg in pts.windows(2) {
                out.push(DrawInstruction::DrawLine {
                    from: seg[0],
                    to: seg[1],
                    color: self.color,
                    width: w,
                });
            }
            for p in &pts {
                out.push(DrawInstruction::FillCircle {
                    center: *p,
                    radius: w / 2.0,
                    color: self.color,
                });
            }
        }
        out
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn should_repaint(&self, previous: &dyn CustomPainter) -> bool {
        previous.as_any().downcast_ref::<GlyphPainter>() != Some(self)
    }
}

// ── reactive regions ─────────────────────────────────────────────

/// A region that rebuilds on its own.
///
/// Every signal read inside `f` subscribes **this** element, not the screen
/// around it — so a spinner reading the clock repaints a 16-pixel square,
/// not the window. Wrap anything that reads a fast-changing signal in one.
pub struct Live {
    name: &'static str,
    f: Rc<dyn Fn() -> WidgetNode>,
}

impl Live {
    pub fn new(name: &'static str, f: impl Fn() -> WidgetNode + 'static) -> Self {
        Live {
            name,
            f: Rc::new(f),
        }
    }
}

impl std::fmt::Debug for Live {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Live({})", self.name)
    }
}

impl Widget for Live {
    fn debug_name(&self) -> &'static str {
        self.name
    }
    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }
    fn build(&self, _ctx: &Ctx) -> WidgetNode {
        (self.f)()
    }
}
vieww::widget::widget_node_from!(Live);

/// Moves smoothly to `target`; `snap` jumps instead (a finger is dragging).
pub fn glide(
    target: f32,
    ms: u64,
    snap: bool,
    build: impl Fn(f32) -> WidgetNode + 'static,
) -> WidgetNode {
    Animated::new(target)
        .duration(Duration::from_millis(if snap { 0 } else { ms }))
        .curve(Curve::EASE_OUT)
        .build(build)
        .into()
}

// ── scrolling ────────────────────────────────────────────────────

/// Where a scroll area is, and how long its window and content are.
///
/// The offset may be set past the end (`to_end`): it is clamped where it is
/// drawn, so "stay at the bottom" survives content arriving later.
#[derive(Clone)]
pub struct ScrollState {
    pub offset: Signal<f32>,
    /// (window, content) lengths, reported by layout.
    pub extent: Signal<(f32, f32)>,
}

impl ScrollState {
    pub fn new(rt: &vieww::element::Runtime) -> ScrollState {
        ScrollState {
            offset: rt.signal(0.0),
            extent: rt.signal((0.0, 0.0)),
        }
    }
    fn max(&self) -> f32 {
        let (view, content) = self.extent.get();
        (content - view).max(0.0)
    }
    /// Where it really is (the stored offset may be past the end).
    pub fn at(&self) -> f32 {
        self.offset.get().clamp(0.0, self.max())
    }
    pub fn by(&self, dy: f32) {
        self.offset.set((self.at() + dy).clamp(0.0, self.max()));
    }
    pub fn to_end(&self) {
        self.offset.set(f32::MAX);
    }
}

/// A vertical scroll area: wheel, trackpad or drag moves it, and a thin
/// scroll bar on the right shows where you are and can be dragged — for
/// everyone without a trackpad.
pub fn scroll_area(ui: &super::Ui, state: &ScrollState, child: WidgetNode) -> WidgetNode {
    let (ui, state) = (ui.clone(), state.clone());
    Live::new("scroll-area", move || {
        let p = ui.pal();
        let (view, content) = state.extent.get();
        let at = state.at();
        let (wheel, ext) = (state.clone(), state.extent.clone());
        let body: WidgetNode = Scrollable::vertical(at)
            .on_drag(Rc::new(move |d: vieww::foundation::DragDetails| {
                wheel.by(-d.delta.dy)
            }))
            .on_extents(Rc::new(move |e: ScrollExtents| {
                if ext.get() != (e.viewport, e.content) {
                    ext.set((e.viewport, e.content));
                }
            }))
            .child(child.clone())
            .into();
        if content <= view + 1.0 || view <= 0.0 {
            return body;
        }
        // The thumb: as tall as the window is of the content, never tiny.
        let track = view - 8.0;
        let thumb = (track * view / content).clamp(36.0, track);
        let top = 4.0 + (track - thumb) * at / (content - view);
        let (drag, per_px) = (state.clone(), (content - view) / (track - thumb).max(1.0));
        let bar: WidgetNode = GestureDetector::new()
            .on_drag_update(move |d: vieww::foundation::DragDetails| drag.by(d.delta.dy * per_px))
            .child(Pressable::sensed(move |s: Sense| {
                Container::new()
                    .width(if s.hover > 0.0 || s.press > 0.0 {
                        8.0
                    } else {
                        6.0
                    })
                    .height(thumb)
                    .radius(4.0)
                    .color(p.ink.with_alpha(if s.press > 0.0 {
                        140
                    } else if s.hover > 0.0 {
                        110
                    } else {
                        60
                    }))
                    .into()
            }))
            .into();
        Stack::new()
            .fit(StackFit::Expand)
            .children(vec![
                body,
                Positioned::new().right(3.0).top(top).child(bar).into(),
            ])
            .into()
    })
    .into()
}

// ── keyboard ─────────────────────────────────────────────────────

/// Wraps the whole shell so keys that nothing inside wanted reach the
/// app's shortcuts. Keys bubble outward from focus; a text field takes
/// ordinary typing (Space included) before anything here sees it.
pub struct Keys {
    pub handle: Rc<dyn Fn(&KeyEvent) -> bool>,
    pub child: WidgetNode,
}

impl std::fmt::Debug for Keys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Keys")
    }
}

impl Widget for Keys {
    fn debug_name(&self) -> &'static str {
        "Keys"
    }
    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::RenderSingleChild(&self.child)
    }
}
vieww::widget::widget_node_from!(Keys);

pub struct RenderKeys {
    pub handle: Rc<dyn Fn(&KeyEvent) -> bool>,
}

impl std::fmt::Debug for RenderKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RenderKeys")
    }
}

impl RenderObject for RenderKeys {
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, constraints: Constraints) -> Size {
        let Some(&child) = ctx.children().first() else {
            return constraints.smallest();
        };
        let size = ctx.layout_child(child, constraints);
        ctx.place_child(child, Offset::ZERO);
        size
    }
    fn is_focusable(&self) -> bool {
        true
    }
    fn handle_key(&self, event: &KeyEvent) -> bool {
        (self.handle)(event)
    }
    fn layout_differs(&self, _new: &dyn RenderObject) -> bool {
        false
    }
    fn debug_name(&self) -> &'static str {
        "Keys"
    }
}
