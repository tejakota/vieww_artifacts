//! studio — **the actual viewwstudio, redrawn.**
//!
//! Not a mock-up of an IDE: this is `apps/viewwstudio`'s own shell, region for
//! region, colour for colour, metric for metric, rebuilt as a pure function of
//! film-time so the film can render it headless at sixty frames a second.
//!
//! Everything here is read out of the product's source rather than designed
//! for the film:
//!
//! | what | value | where it comes from |
//! |---|---|---|
//! | the shell | **cards**: rounded surfaces with a hairline, a sheen, a rim and an elevation shadow, separated by a gutter | `ui/chrome.rs::card` |
//! | card corner · gutter | `8.0` · `6.0` | `chrome::CARD_CORNER`, `chrome::CARD_GAP` |
//! | title bar · activity bar · status bar | `36` high · `48` wide · `24` high | `ui/title_bar.rs`, `ui/activity_bar.rs`, `ui/status_bar.rs` |
//! | sidebar | `180..=480`, header `34`, row `22` | `state::MIN_SIDEBAR`/`MAX_SIDEBAR`, `ui/sidebar.rs` |
//! | tab strip · breadcrumb · minimap | `35` · `26` · `64` | `ui/editor.rs` |
//! | bottom panel | `176`, tab strip `32` | `state::PANEL_HEIGHT`, `ui/panel.rs` |
//! | chrome colours | `#0E0E0E · #181818 · #1A1A1A · #1F1F1F · #262626 · #2F2F2F`, line `#2B2B2B` | `theme::StudioTheme::dark` |
//! | gutter ink | `#82898F`, active `#CCCCCC` | same |
//! | accent | `#7E5CE8 → #B491FF`, wash `0x3E` | `theme::PURPLE`, which is `theme::ACCENT` |
//! | syntax | keyword `#569CD6` · function `#DCDCAA` · type `#4EC9B0` · string `#CE9178` · number `#B5CEA8` · comment `#6A9955` · macro `#C586C0` · punctuation `#D4D4D4` · attribute `#9CDCFE` | `theme::Syntax` |
//! | activity views | explorer · search · snippets · problems · inspector · learn · docs · toolchain · source · export · tokens · settings | `ui/activity_bar.rs::view_icon` |
//!
//! The shell's own arrangement is the product's too: a column of
//! `[title bar · workspace · status bar]`; the workspace a row of
//! `[activity bar · sidebar · main column]`; the main column
//! `[[editor · preview] · bottom panel]`. That is `ui/mod.rs`'s `Body`,
//! `Workspace` and `MainColumn`, verbatim.
//!
//! **Why it is built this way.** A frame is a pure function of film-time, so
//! the studio is a *value*: [`Shell`] is what the IDE looks like at this
//! instant, [`Layout`] derives every rectangle from the window, and the draw
//! is split in two so a scene can paint its own preview content *between* the
//! chrome's back and front:
//!
//! ```text
//! back(book)  →  the scene's preview painting  →  front(book)  →  text(nodes)
//! ```
//!
//! Text is never faked with rectangles and never placed by guessing an
//! advance: a code line is one [`RichText`] whose spans carry the colours, so
//! the shaper decides where each token sits, and the caret is the last span of
//! the line — which makes it land exactly at the end of whatever has been
//! typed, at any size, on any bench.
//!
//! The window also stops short of the frame's floor. The band below it
//! ([`CAPTION_BAND`]) belongs to the film, not the product: the caption and
//! the act card live there, so the film never prints its own voice over the
//! chrome it is arguing for.

use vieww_foundation::{
    BlendMode, Color, Gradient, Offset, Path, Rect, Shadow, Size, Sketchbook, TextAlign, TextStyle,
};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, PaintWith, Painting, RichText, Span};

use crate::kit::{clamp01, ease_out_cubic, meter, smoothstep, tint, xywh, Type, H, W};

// ── The product's palette ───────────────────────────────────────────────────
//
// `StudioTheme::dark()`, copied value for value. The studio does not paint
// its editor in the brand's marketing colours, and neither does the film.

/// `#0E0E0E` — the ground every region floats on.
pub const WINDOW: Color = Color::rgb(0x0E, 0x0E, 0x0E);
/// `#181818` — activity bar and status bar: the frame around everything.
pub const CHROME_0: Color = Color::rgb(0x18, 0x18, 0x18);
/// `#1A1A1A` — sidebar, preview pane and bottom panel.
pub const CHROME_1: Color = Color::rgb(0x1A, 0x1A, 0x1A);
/// `#1F1F1F` — the editor surface.
pub const CHROME_2: Color = Color::rgb(0x1F, 0x1F, 0x1F);
/// `#262626` — raised: the active tab, a popover, the command palette.
pub const CHROME_3: Color = Color::rgb(0x26, 0x26, 0x26);
/// `#2F2F2F` — hover.
pub const CHROME_4: Color = Color::rgb(0x2F, 0x2F, 0x2F);
/// `#2B2B2B` — the hairline around every card.
pub const LINE: Color = Color::rgb(0x2B, 0x2B, 0x2B);
/// `#82898F` — line numbers at rest. Lifted one step off GitHub's muted grey
/// so a line number clears 4.5:1, because a line number is *read*.
pub const GUTTER: Color = Color::rgb(0x82, 0x89, 0x8F);
/// `#CCCCCC` — the caret line's number.
pub const GUTTER_ACTIVE: Color = Color::rgb(0xCC, 0xCC, 0xCC);
/// `#CCA700` — the warning.
pub const WARNING: Color = Color::rgb(0xCC, 0xA7, 0x00);
/// `#FFB4AB` — the error, from the dark `ColorScheme`.
pub const ERROR: Color = Color::rgb(0xFF, 0xB4, 0xAB);
/// `#E4E1E6` — body text on a chrome surface.
pub const ON_SURFACE: Color = Color::rgb(0xE4, 0xE1, 0xE6);

/// `#7E5CE8` — the accent's near stop. White text sits on this.
pub const ACCENT: Color = Color::rgb(0x7E, 0x5C, 0xE8);
/// `#B491FF` — the accent's far stop, and most of what the eye sees.
pub const ACCENT_SOFT: Color = Color::rgb(0xB4, 0x91, 0xFF);
/// The wash behind a selected thing: the near stop at `0x3E`.
pub const ACCENT_WASH: Color = Color::rgba(0x7E, 0x5C, 0xE8, 0x3E);

/// One colour per token class the highlighter emits — `theme::Syntax`.
pub mod syn {
    use vieww_foundation::Color;
    pub const KEYWORD: Color = Color::rgb(0x56, 0x9C, 0xD6);
    pub const FUNCTION: Color = Color::rgb(0xDC, 0xDC, 0xAA);
    pub const TYPE_NAME: Color = Color::rgb(0x4E, 0xC9, 0xB0);
    pub const STRING: Color = Color::rgb(0xCE, 0x91, 0x78);
    pub const NUMBER: Color = Color::rgb(0xB5, 0xCE, 0xA8);
    pub const COMMENT: Color = Color::rgb(0x6A, 0x99, 0x55);
    pub const MACRO_NAME: Color = Color::rgb(0xC5, 0x86, 0xC0);
    pub const PUNCTUATION: Color = Color::rgb(0xD4, 0xD4, 0xD4);
    pub const ATTRIBUTE: Color = Color::rgb(0x9C, 0xDC, 0xFE);
}

/// `c` at `a` of its own alpha.
#[must_use]
pub fn alpha(c: Color, a: f32) -> Color {
    Color::rgba(c.r, c.g, c.b, (a * 255.0).clamp(0.0, 255.0) as u8)
}

/// The sheen: one veil of caught light at the top of a surface, fading out by
/// a third of the way down. Nine of 255 — anything legible as a ramp on a
/// flat panel reads as a mistake.
#[must_use]
pub fn sheen(base: Color) -> Gradient {
    Gradient::vertical().with_dither().with_stops(&[
        (0.0, Color::rgba(255, 255, 255, 9).over(base)),
        (0.34, base),
        (1.0, base),
    ])
}

/// The deep sheen: lit from the top *and* shaded at the bottom, for the tall
/// regions where a sheen alone runs out of ramp.
#[must_use]
pub fn deep_sheen(base: Color) -> Gradient {
    Gradient::vertical().with_dither().with_stops(&[
        (0.0, Color::rgba(255, 255, 255, 14).over(base)),
        (0.28, base),
        (1.0, Color::rgba(0, 0, 0, 46).over(base)),
    ])
}

/// The rim: one pixel of caught light along a raised surface's top edge.
#[must_use]
pub fn rim(base: Color) -> Color {
    Color::rgba(255, 255, 255, 20).over(base)
}

/// The elevation shadow, by level — the product's own curve.
#[must_use]
pub fn elevation(level: u8) -> Shadow {
    let level = f32::from(level.clamp(1, 4));
    let a = (46.0 + level * 30.0).min(190.0);
    Shadow::new(
        Color::rgba(0, 0, 0, a as u8),
        Offset::new(0.0, level * 1.6),
        level * level * 1.6 + 2.0,
    )
}

// ── Geometry ────────────────────────────────────────────────────────────────

/// The radius every region of the shell is cut to — `chrome::CARD_CORNER`.
pub const CARD_CORNER: f32 = 8.0;
/// The gutter between two regions, and the window's own inset —
/// `chrome::CARD_GAP`. It is also the width of a divider, on purpose: the
/// drag handle *is* the gutter, so a split costs no pixels of its own.
pub const CARD_GAP: f32 = 6.0;

pub const TITLEBAR_H: f32 = 36.0;
pub const RAIL_W: f32 = 48.0;
pub const STATUS_H: f32 = 24.0;
pub const SIDEBAR_W: f32 = 268.0;
pub const SIDEBAR_HEADER: f32 = 34.0;
pub const SIDEBAR_ROW: f32 = 22.0;
pub const TAB_STRIP: f32 = 35.0;
pub const BREADCRUMB: f32 = 26.0;
pub const MINIMAP_W: f32 = 64.0;
pub const PANEL_H: f32 = 176.0;
pub const PANEL_TABS: f32 = 32.0;
/// The gutter column's width. The product sizes it to the digit count; the
/// film's buffers are two digits, and this is that width.
pub const GUTTER_W: f32 = 52.0;

/// The band below the window that belongs to the film, not the product.
pub const CAPTION_BAND: f32 = 126.0;

/// Code metrics. `CODE_LINE_RATIO` is the product's `20.0 / 13.0`, so the
/// leading follows the size the way the editor's does.
pub const CODE_SIZE: f32 = 19.0;
pub const CODE_LINE_RATIO: f32 = 20.0 / 13.0;
pub const CODE_LEAD: f32 = CODE_SIZE * CODE_LINE_RATIO;
pub const CODE_PAD_X: f32 = 16.0;
pub const CODE_PAD_Y: f32 = 10.0;

/// Every rectangle of the studio, derived from the window. Nothing in a scene
/// positions a region by hand.
#[derive(Clone, Copy)]
pub struct Layout {
    /// The whole application surface — `chrome.window`, padded by the gutter.
    pub window: Rect,
    pub titlebar: Rect,
    pub rail: Rect,
    pub sidebar: Rect,
    /// The editor card: tab strip, breadcrumb, gutter, code, minimap.
    pub editor: Rect,
    pub tabs: Rect,
    pub breadcrumb: Rect,
    pub gutter: Rect,
    pub code: Rect,
    pub minimap: Rect,
    /// The preview card.
    pub preview: Rect,
    pub preview_bar: Rect,
    pub preview_body: Rect,
    /// The bottom panel card.
    pub panel: Rect,
    pub panel_tabs: Rect,
    pub panel_body: Rect,
    pub status: Rect,
    pub panel_open: bool,
}

impl Layout {
    /// The studio filling the frame at `inset`, with the preview taking
    /// `preview_frac` of the main column and the bottom panel open or shut.
    #[must_use]
    pub fn new(inset: f32, preview_frac: f32, panel_open: bool) -> Self {
        Self::within(
            Rect::new(inset, 40.0, W - inset, H - CAPTION_BAND),
            preview_frac,
            panel_open,
        )
    }

    /// The studio inside an arbitrary rectangle — how S11 shows the IDE
    /// inside its own preview.
    #[must_use]
    pub fn within(window: Rect, preview_frac: f32, panel_open: bool) -> Self {
        let g = CARD_GAP;
        let inner = Rect::new(
            window.left + g,
            window.top + g,
            window.right - g,
            window.bottom - g,
        );
        let titlebar = Rect::new(inner.left, inner.top, inner.right, inner.top + TITLEBAR_H);
        let status = Rect::new(inner.left, inner.bottom - STATUS_H, inner.right, inner.bottom);
        let work_top = titlebar.bottom + g;
        let work_bottom = status.top - g;

        let rail = Rect::new(inner.left, work_top, inner.left + RAIL_W, work_bottom);
        let sidebar = Rect::new(rail.right + g, work_top, rail.right + g + SIDEBAR_W, work_bottom);
        let main_l = sidebar.right + g;

        // The main column: panes over the bottom panel.
        let panel_top = if panel_open { work_bottom - PANEL_H } else { work_bottom - PANEL_TABS };
        let panel = Rect::new(main_l, panel_top, inner.right, work_bottom);
        let panel_tabs = Rect::new(panel.left, panel.top, panel.right, panel.top + PANEL_TABS);
        let panel_body = Rect::new(panel.left, panel_tabs.bottom, panel.right, panel.bottom);
        let panes_bottom = panel.top - g;

        let preview_w = (inner.right - main_l) * preview_frac;
        let preview = Rect::new(inner.right - preview_w, work_top, inner.right, panes_bottom);
        let editor = Rect::new(main_l, work_top, preview.left - g, panes_bottom);

        let tabs = Rect::new(editor.left, editor.top, editor.right, editor.top + TAB_STRIP);
        let breadcrumb = Rect::new(editor.left, tabs.bottom, editor.right, tabs.bottom + BREADCRUMB);
        let gutter = Rect::new(editor.left, breadcrumb.bottom, editor.left + GUTTER_W, editor.bottom);
        let minimap = Rect::new(editor.right - MINIMAP_W, breadcrumb.bottom, editor.right, editor.bottom);
        let code = Rect::new(gutter.right, breadcrumb.bottom, minimap.left, editor.bottom);

        let preview_bar = Rect::new(preview.left, preview.top, preview.right, preview.top + TAB_STRIP);
        let preview_body = Rect::new(preview.left, preview_bar.bottom, preview.right, preview.bottom);

        Self {
            window,
            titlebar,
            rail,
            sidebar,
            editor,
            tabs,
            breadcrumb,
            gutter,
            code,
            minimap,
            preview,
            preview_bar,
            preview_body,
            panel,
            panel_tabs,
            panel_body,
            status,
            panel_open,
        }
    }

    /// The top of code line `i` (0-based).
    #[must_use]
    pub fn line_y(&self, i: usize) -> f32 {
        self.code.top + CODE_PAD_Y + i as f32 * CODE_LEAD
    }

    /// How many code lines fit.
    #[must_use]
    pub fn visible_lines(&self) -> usize {
        ((self.code.height() - CODE_PAD_Y * 2.0) / CODE_LEAD).floor().max(1.0) as usize
    }
}

/// Draw one region the way `chrome::card` does: elevation, sheen over the
/// base, a hairline, and one pixel of rim along the top edge.
pub fn card(book: &mut Sketchbook, r: Rect, base: Color, deep: bool, a: f32) {
    if a <= 0.01 || r.width() <= 1.0 || r.height() <= 1.0 {
        return;
    }
    book.shadow(r, CARD_CORNER, elevation(2));
    book.rrect(r, CARD_CORNER, if deep { deep_sheen(base) } else { sheen(base) });
    book.stroke_rrect(r, CARD_CORNER, alpha(LINE, a), 1.0);
    book.rrect(
        xywh(r.left + CARD_CORNER, r.top + 1.0, (r.width() - CARD_CORNER * 2.0).max(0.0), 1.0),
        0.5,
        alpha(rim(base), a),
    );
}

// ── Syntax ──────────────────────────────────────────────────────────────────

/// A token class, one per role in the product's `Syntax` struct.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tok {
    Kw,
    Fun,
    Ty,
    Str,
    Num,
    Comment,
    Macro,
    Punct,
    Attr,
    Ident,
}

impl Tok {
    #[must_use]
    pub const fn color(self) -> Color {
        match self {
            Self::Kw => syn::KEYWORD,
            Self::Fun => syn::FUNCTION,
            Self::Ty => syn::TYPE_NAME,
            Self::Str => syn::STRING,
            Self::Num => syn::NUMBER,
            Self::Comment => syn::COMMENT,
            Self::Macro => syn::MACRO_NAME,
            Self::Attr => syn::ATTRIBUTE,
            Self::Punct | Self::Ident => syn::PUNCTUATION,
        }
    }
}

/// One source line: a list of coloured pieces.
#[derive(Clone, Default)]
pub struct Line {
    pub parts: Vec<(String, Tok)>,
}

impl Line {
    #[must_use]
    pub fn plain(text: &str, tok: Tok) -> Self {
        Self { parts: vec![(text.to_string(), tok)] }
    }

    #[must_use]
    pub fn text(&self) -> String {
        self.parts.iter().map(|(s, _)| s.as_str()).collect()
    }

    #[must_use]
    pub fn chars(&self) -> usize {
        self.parts.iter().map(|(s, _)| s.chars().count()).sum()
    }

    /// The line truncated to `n` characters — how a line types itself on.
    #[must_use]
    pub fn truncated(&self, n: usize) -> Self {
        let mut left = n;
        let mut parts = Vec::new();
        for (s, t) in &self.parts {
            let c = s.chars().count();
            if left == 0 {
                break;
            }
            if c <= left {
                parts.push((s.clone(), *t));
                left -= c;
            } else {
                parts.push((s.chars().take(left).collect::<String>(), *t));
                left = 0;
            }
        }
        Self { parts }
    }
}

/// The highlighter. Two languages: `say`, the studio's fast loop, and Rust,
/// what it becomes. One pass, and the classes are the product's own.
#[must_use]
pub fn lex(src: &str, rust: bool) -> Line {
    const SAY_KW: &[&str] = &[
        "view", "state", "when", "else", "for", "in", "let", "text", "column", "row", "stack",
        "button", "on", "tap", "show", "hide", "each", "if", "canvas", "spring", "animate", "dial",
    ];
    const RUST_KW: &[&str] = &[
        "fn", "let", "mut", "pub", "use", "impl", "struct", "enum", "for", "in", "if", "else",
        "match", "return", "move", "self", "Self", "const", "static", "crate", "mod", "as",
        "where", "type", "dyn", "true", "false",
    ];
    let kws = if rust { RUST_KW } else { SAY_KW };

    fn push(parts: &mut Vec<(String, Tok)>, s: String, t: Tok) {
        if s.is_empty() {
            return;
        }
        if let Some((prev, pt)) = parts.last_mut() {
            if *pt == t {
                prev.push_str(&s);
                return;
            }
        }
        parts.push((s, t));
    }

    let mut parts: Vec<(String, Tok)> = Vec::new();
    let b: Vec<char> = src.chars().collect();
    let mut i = 0usize;

    while i < b.len() {
        let c = b[i];
        if c == '/' && i + 1 < b.len() && b[i + 1] == '/' {
            push(&mut parts, b[i..].iter().collect(), Tok::Comment);
            break;
        }
        if c == '#' && !rust {
            push(&mut parts, b[i..].iter().collect(), Tok::Comment);
            break;
        }
        if c == '#' && rust && i + 1 < b.len() && (b[i + 1] == '[' || b[i + 1] == '!') {
            let mut j = i;
            while j < b.len() && b[j] != ']' {
                j += 1;
            }
            let j = (j + 1).min(b.len());
            push(&mut parts, b[i..j].iter().collect(), Tok::Attr);
            i = j;
            continue;
        }
        if c == '"' {
            let mut j = i + 1;
            while j < b.len() && b[j] != '"' {
                j += 1;
            }
            let j = (j + 1).min(b.len());
            push(&mut parts, b[i..j].iter().collect(), Tok::Str);
            i = j;
            continue;
        }
        if c.is_ascii_digit() {
            let mut j = i;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == '.' || b[j] == '_') {
                j += 1;
            }
            push(&mut parts, b[i..j].iter().collect(), Tok::Num);
            i = j;
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let mut j = i;
            while j < b.len() && (b[j].is_alphanumeric() || b[j] == '_') {
                j += 1;
            }
            let word: String = b[i..j].iter().collect();
            let next = b.get(j).copied().unwrap_or(' ');
            let tok = if kws.contains(&word.as_str()) {
                Tok::Kw
            } else if next == '!' {
                Tok::Macro
            } else if next == '(' {
                Tok::Fun
            } else if word.chars().next().is_some_and(char::is_uppercase) {
                Tok::Ty
            } else {
                Tok::Ident
            };
            push(&mut parts, word, tok);
            i = j;
            continue;
        }
        if c == ' ' {
            let mut j = i;
            while j < b.len() && b[j] == ' ' {
                j += 1;
            }
            push(&mut parts, b[i..j].iter().collect(), Tok::Punct);
            i = j;
            continue;
        }
        push(&mut parts, c.to_string(), Tok::Punct);
        i += 1;
    }
    Line { parts }
}

/// Lex a whole buffer.
#[must_use]
pub fn lex_all(src: &[&str], rust: bool) -> Vec<Line> {
    src.iter().map(|l| lex(l, rust)).collect()
}

// ── The shell's state ───────────────────────────────────────────────────────

#[derive(Clone)]
pub struct FileRow {
    pub name: String,
    pub depth: usize,
    pub kind: FileKind,
    pub active: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Folder,
    FolderOpen,
    Say,
    Rust,
    Toml,
    Asset,
}

impl FileKind {
    #[must_use]
    pub const fn color(self) -> Color {
        match self {
            Self::Folder | Self::FolderOpen => GUTTER,
            Self::Say => ACCENT_SOFT,
            Self::Rust => syn::MACRO_NAME,
            Self::Toml => syn::TYPE_NAME,
            Self::Asset => syn::ATTRIBUTE,
        }
    }
    #[must_use]
    pub const fn glyph(self) -> &'static str {
        match self {
            Self::Folder => "▸",
            Self::FolderOpen => "▾",
            Self::Say => "◆",
            Self::Rust => "⬢",
            Self::Toml => "▤",
            Self::Asset => "▩",
        }
    }
}

/// A diagnostic pinned to a code line.
#[derive(Clone)]
pub struct Diag {
    pub line: usize,
    pub message: String,
    pub severity: Sev,
    /// `0..=1` — the squiggle drawing itself in.
    pub reveal: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sev {
    Error,
    Warn,
}

impl Sev {
    #[must_use]
    pub const fn color(self) -> Color {
        match self {
            Self::Error => ERROR,
            Self::Warn => WARNING,
        }
    }
}

/// The command palette, mid-invocation.
#[derive(Clone)]
pub struct Palette {
    pub query: String,
    pub rows: Vec<(String, String)>,
    pub selected: usize,
    pub open: f32,
}

/// The inspector — the studio's own devtools view.
#[derive(Clone)]
pub struct Inspector {
    pub open: f32,
    pub tree: Vec<(usize, String, bool)>,
    pub facts: Vec<(String, String)>,
    /// Damage rectangles, in preview-body coordinates `0..=1`.
    pub damage: Vec<(Rect, f32)>,
    /// The selected element's box, in preview-body coordinates `0..=1`.
    pub selection: Option<Rect>,
}

/// The bottom panel's content: which tab, and what it is printing.
#[derive(Clone)]
pub struct Panel {
    /// Tab labels, and which is active.
    pub tabs: Vec<String>,
    pub active: usize,
    pub lines: Vec<(String, Color)>,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            tabs: vec!["Problems".into(), "Output".into(), "Terminal".into(), "Builds".into()],
            active: 1,
            lines: Vec::new(),
        }
    }
}

/// The whole IDE at one instant.
#[derive(Clone)]
pub struct Shell {
    pub title: String,
    pub subtitle: String,
    pub files: Vec<FileRow>,
    pub tabs: Vec<(String, FileKind, bool, bool)>,
    /// The breadcrumb above the code — the product shows the path and symbol.
    pub crumbs: Vec<String>,
    pub code: Vec<Line>,
    pub caret: Option<usize>,
    pub caret_on: bool,
    pub changed: Vec<usize>,
    pub diag: Option<Diag>,
    pub status: Vec<(String, Color)>,
    pub palette: Option<Palette>,
    pub inspector: Option<Inspector>,
    pub panel: Panel,
    pub preview_label: String,
    pub preview_chip: String,
    /// Which activity view is lit, `0..12`.
    pub view: usize,
    pub witness: Option<u32>,
    pub elapsed: Option<String>,
    /// `0..=1` — how assembled the window is.
    pub assembly: f32,
    pub focus: Option<FocusOn>,
    pub dim: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FocusOn {
    Code,
    Preview,
    Sidebar,
    Status,
}

impl Default for Shell {
    fn default() -> Self {
        Self {
            title: "counter — viewwstudio".into(),
            subtitle: "vieww".into(),
            files: Vec::new(),
            tabs: Vec::new(),
            crumbs: vec!["counter".into(), "src".into(), "counter.say".into()],
            code: Vec::new(),
            caret: None,
            caret_on: true,
            changed: Vec::new(),
            diag: None,
            status: Vec::new(),
            palette: None,
            inspector: None,
            panel: Panel::default(),
            preview_label: "Preview".into(),
            preview_chip: "100%".into(),
            view: 0,
            witness: None,
            elapsed: None,
            assembly: 1.0,
            focus: None,
            dim: 0.0,
        }
    }
}

/// The project tree the film's session works in.
#[must_use]
pub fn project_tree(active: &str) -> Vec<FileRow> {
    let rows: &[(&str, usize, FileKind)] = &[
        ("counter", 0, FileKind::FolderOpen),
        ("src", 1, FileKind::FolderOpen),
        ("main.say", 2, FileKind::Say),
        ("counter.say", 2, FileKind::Say),
        ("theme.say", 2, FileKind::Say),
        ("main.rs", 2, FileKind::Rust),
        ("assets", 1, FileKind::Folder),
        ("targets", 1, FileKind::FolderOpen),
        ("desktop", 2, FileKind::Asset),
        ("web", 2, FileKind::Asset),
        ("android", 2, FileKind::Asset),
        ("vieww.toml", 1, FileKind::Toml),
    ];
    rows.iter()
        .map(|(n, d, k)| FileRow {
            name: (*n).to_string(),
            depth: *d,
            kind: *k,
            active: *n == active,
        })
        .collect()
}

/// The status bar the session carries.
#[must_use]
pub fn status_line(target: &str, fps: &str, extra: &[(&str, Color)]) -> Vec<(String, Color)> {
    let mut v = vec![
        ("viewwstudio".to_string(), ACCENT_SOFT),
        (format!("target {target}"), GUTTER),
        (fps.to_string(), syn::STRING),
    ];
    for (s, c) in extra {
        v.push(((*s).to_string(), *c));
    }
    v
}

/// The build log the bottom panel usually shows.
#[must_use]
pub fn output_lines() -> Vec<(String, Color)> {
    vec![
        ("  preview   say → tree in 0.9 ms · no codegen".to_string(), GUTTER),
        ("  reload    counter.say · 1 view rebuilt".to_string(), syn::STRING),
        ("  renderer  native · 4× supersampling".to_string(), GUTTER),
    ]
}

// ── The draw: back ──────────────────────────────────────────────────────────

/// Everything behind the preview's content.
pub fn back(book: &mut Sketchbook, l: &Layout, sh: &Shell) {
    let a = clamp01(sh.assembly);
    if a <= 0.01 {
        return;
    }

    // The ground the cards float on.
    book.shadow(l.window, CARD_CORNER + 4.0, elevation(4));
    book.rrect(l.window, CARD_CORNER + 4.0, alpha(WINDOW, a));

    // ── The title bar ───────────────────────────────────────────────────
    card(book, l.titlebar, CHROME_0, false, a);
    for (i, c) in [
        Color::rgb(0xE0, 0x6C, 0x60),
        Color::rgb(0xE0, 0xA8, 0x4E),
        Color::rgb(0x5C, 0xB8, 0x60),
    ]
    .into_iter()
    .enumerate()
    {
        book.circle(
            Offset::new(l.titlebar.left + 20.0 + i as f32 * 16.0, l.titlebar.top + TITLEBAR_H * 0.5),
            5.0,
            alpha(c, 0.9),
        );
    }
    // The brand mark: the studio's own glyph, a preview panel inside a frame.
    let bx = l.titlebar.left + 84.0;
    let by = l.titlebar.top + TITLEBAR_H * 0.5;
    book.stroke_rrect(xywh(bx, by - 8.0, 22.0, 16.0), 3.0, alpha(ACCENT_SOFT, 0.8), 1.4);
    book.rrect(xywh(bx + 12.0, by - 5.0, 7.0, 10.0), 1.5, alpha(ACCENT, 0.9));

    // ── The activity bar ────────────────────────────────────────────────
    card(book, l.rail, CHROME_0, true, a);
    for i in 0..12 {
        let y = l.rail.top + 26.0 + i as f32 * 36.0;
        if y + 16.0 > l.rail.bottom - 8.0 {
            break;
        }
        let on = i == sh.view;
        if on {
            book.rrect(
                xywh(l.rail.left + 4.0, y - 14.0, RAIL_W - 8.0, 28.0),
                5.0,
                ACCENT_WASH,
            );
            book.rrect(xywh(l.rail.left + 1.0, y - 11.0, 2.5, 22.0), 1.2, ACCENT_SOFT);
        }
        let c = if on { alpha(ACCENT_SOFT, 0.95) } else { alpha(GUTTER, 0.62) };
        let cx = l.rail.left + RAIL_W * 0.5;
        view_icon(book, i, Offset::new(cx, y), c);
    }

    // ── The sidebar ─────────────────────────────────────────────────────
    card(book, l.sidebar, CHROME_1, true, a);
    book.rrect(
        xywh(l.sidebar.left + 1.0, l.sidebar.top + 1.0, l.sidebar.width() - 2.0, SIDEBAR_HEADER),
        CARD_CORNER - 1.0,
        alpha(Color::rgba(255, 255, 255, 6).over(CHROME_1), a),
    );
    for (i, f) in sh.files.iter().enumerate() {
        let y = l.sidebar.top + SIDEBAR_HEADER + 8.0 + i as f32 * SIDEBAR_ROW;
        if y + SIDEBAR_ROW > l.sidebar.bottom - 6.0 {
            break;
        }
        if f.active {
            book.rrect(
                xywh(l.sidebar.left + 4.0, y, l.sidebar.width() - 8.0, SIDEBAR_ROW),
                4.0,
                ACCENT_WASH,
            );
        }
    }

    // ── The editor card ─────────────────────────────────────────────────
    card(book, l.editor, CHROME_2, true, a);
    // The tab strip.
    let mut x = l.tabs.left + 4.0;
    for (name, _k, active, dirty) in &sh.tabs {
        let w = 12.0 * 2.0 + 13.0 + 7.0 * 2.0 + 17.0 + name.chars().count() as f32 * 6.9;
        if *active {
            book.rrect(xywh(x, l.tabs.top + 3.0, w, TAB_STRIP - 3.0), 6.0, alpha(CHROME_3, a));
            book.rrect(xywh(x + 1.0, l.tabs.top + 3.0, w - 2.0, 1.0), 0.5, alpha(rim(CHROME_3), a));
            book.rrect(xywh(x + 8.0, l.tabs.bottom - 2.0, w - 16.0, 2.0), 1.0, alpha(ACCENT_SOFT, 0.9));
        }
        let _ = dirty;
        x += w + 2.0;
    }
    book.rect(xywh(l.tabs.left + 1.0, l.tabs.bottom, l.tabs.width() - 2.0, 1.0), alpha(LINE, a));
    // The breadcrumb strip.
    book.rect(
        xywh(l.breadcrumb.left + 1.0, l.breadcrumb.top, l.breadcrumb.width() - 2.0, l.breadcrumb.height()),
        alpha(CHROME_2, a),
    );
    book.rect(
        xywh(l.breadcrumb.left + 1.0, l.breadcrumb.bottom, l.breadcrumb.width() - 2.0, 1.0),
        alpha(LINE, 0.7 * a),
    );

    // The caret line's wash, across gutter and code.
    if let Some(ci) = sh.caret {
        let y = l.line_y(ci) - 3.0;
        if y > l.code.top && y + CODE_LEAD < l.code.bottom {
            book.rect(
                xywh(l.gutter.left + 1.0, y, l.editor.width() - 2.0, CODE_LEAD),
                Color::rgba(255, 255, 255, 8),
            );
        }
    }
    // Changed-line marks, in the gutter's own margin.
    for ci in &sh.changed {
        let y = l.line_y(*ci) - 2.0;
        if y > l.code.top && y + CODE_LEAD < l.code.bottom {
            book.rrect(
                xywh(l.gutter.left + 3.0, y, 2.0, CODE_LEAD - 6.0),
                1.0,
                alpha(syn::STRING, 0.8),
            );
        }
    }

    // The minimap — one rect per line, width following the real line's length
    // and rows on the product's own scale.
    for (i, line) in sh.code.iter().enumerate() {
        let y = l.minimap.top + 8.0 + i as f32 * 4.0;
        if y > l.minimap.bottom - 6.0 {
            break;
        }
        let n = line.chars().min(70) as f32;
        if n < 1.0 {
            continue;
        }
        let indent = line.text().chars().take_while(|c| *c == ' ').count().min(16) as f32;
        book.rect(
            xywh(l.minimap.left + 8.0 + indent * 0.7, y, (n - indent).max(1.0) * 0.7, 2.0),
            alpha(ON_SURFACE, 0.26),
        );
    }
    book.rrect(
        xywh(
            l.minimap.left + 4.0,
            l.minimap.top + 4.0,
            l.minimap.width() - 8.0,
            (sh.code.len().max(1) as f32 * 4.0 + 8.0).min(l.minimap.height() - 10.0),
        ),
        3.0,
        Color::rgba(255, 255, 255, 8),
    );

    // ── The preview card ────────────────────────────────────────────────
    card(book, l.preview, CHROME_1, true, a);
    book.rect(
        xywh(l.preview_bar.left + 1.0, l.preview_bar.bottom, l.preview_bar.width() - 2.0, 1.0),
        alpha(LINE, a),
    );
    // The device toolbar: three form-factor chips and a live dot.
    let bx = l.preview_bar.left + 14.0;
    for i in 0..3 {
        let x = bx + i as f32 * 30.0;
        let cy = l.preview_bar.top + TAB_STRIP * 0.5;
        let c = if i == 0 { alpha(ACCENT_SOFT, 0.95) } else { alpha(GUTTER, 0.55) };
        match i {
            0 => {
                book.stroke_rrect(xywh(x, cy - 7.0, 20.0, 14.0), 2.0, c, 1.4);
            }
            1 => {
                book.stroke_rrect(xywh(x + 5.0, cy - 8.0, 11.0, 17.0), 2.5, c, 1.4);
            }
            _ => {
                book.stroke_rrect(xywh(x, cy - 7.0, 18.0, 14.0), 2.0, c, 1.4);
                book.rect(xywh(x + 2.0, cy - 3.0, 14.0, 1.0), c);
            }
        }
    }
    book.circle(
        Offset::new(l.preview_bar.right - 24.0, l.preview_bar.top + TAB_STRIP * 0.5),
        4.5,
        alpha(syn::STRING, 0.9),
    );

    // ── The bottom panel ────────────────────────────────────────────────
    card(book, l.panel, CHROME_1, false, a);
    let mut px = l.panel_tabs.left + 14.0;
    for (i, t) in sh.panel.tabs.iter().enumerate() {
        let w = t.chars().count() as f32 * 7.2 + 20.0;
        if i == sh.panel.active {
            book.rrect(
                xywh(px - 8.0, l.panel_tabs.top + 5.0, w, PANEL_TABS - 10.0),
                4.0,
                alpha(CHROME_3, a),
            );
        }
        px += w + 6.0;
    }
    book.rect(
        xywh(l.panel_tabs.left + 1.0, l.panel_tabs.bottom, l.panel_tabs.width() - 2.0, 1.0),
        alpha(LINE, 0.8 * a),
    );

    // ── The status bar ──────────────────────────────────────────────────
    card(book, l.status, CHROME_0, false, a);
}

/// One activity-bar icon, drawn as geometry. The twelve views, in the
/// product's own order: explorer, search, snippets, problems, inspector,
/// learn, docs, toolchain, source, export, tokens, settings.
fn view_icon(book: &mut Sketchbook, i: usize, at: Offset, c: Color) {
    let (x, y) = (at.dx, at.dy);
    match i {
        // folder
        0 => {
            book.stroke_rrect(xywh(x - 9.0, y - 6.0, 18.0, 13.0), 2.0, c, 1.4);
            book.rect(xywh(x - 9.0, y - 8.0, 8.0, 3.0), c);
        }
        // search
        1 => {
            book.ring(Offset::new(x - 1.0, y - 2.0), 6.0, 1.4, c);
            book.line(Offset::new(x + 3.5, y + 2.5), Offset::new(x + 8.0, y + 7.5), c, 1.6);
        }
        // code / snippets
        2 => {
            let mut p = Path::new();
            p.move_to(Offset::new(x - 2.0, y - 7.0));
            p.line_to(Offset::new(x - 8.0, y));
            p.line_to(Offset::new(x - 2.0, y + 7.0));
            book.stroke(p, c, 1.5);
            let mut q = Path::new();
            q.move_to(Offset::new(x + 2.0, y - 7.0));
            q.line_to(Offset::new(x + 8.0, y));
            q.line_to(Offset::new(x + 2.0, y + 7.0));
            book.stroke(q, c, 1.5);
        }
        // warning
        3 => {
            let mut p = Path::new();
            p.move_to(Offset::new(x, y - 8.0));
            p.line_to(Offset::new(x + 9.0, y + 7.0));
            p.line_to(Offset::new(x - 9.0, y + 7.0));
            p.close();
            book.stroke(p, c, 1.4);
            book.rect(xywh(x - 0.7, y - 3.0, 1.6, 6.0), c);
        }
        // dashboard / inspector
        4 => {
            book.stroke_rrect(xywh(x - 8.0, y - 8.0, 7.0, 7.0), 1.5, c, 1.3);
            book.stroke_rrect(xywh(x + 1.0, y - 8.0, 7.0, 11.0), 1.5, c, 1.3);
            book.stroke_rrect(xywh(x - 8.0, y + 1.0, 7.0, 7.0), 1.5, c, 1.3);
        }
        // lightbulb / learn
        5 => {
            book.ring(Offset::new(x, y - 2.0), 6.0, 1.4, c);
            book.rect(xywh(x - 3.0, y + 5.0, 6.0, 1.6), c);
            book.rect(xywh(x - 2.0, y + 8.0, 4.0, 1.4), c);
        }
        // book / docs
        6 => {
            book.stroke_rrect(xywh(x - 8.0, y - 7.0, 16.0, 14.0), 1.5, c, 1.4);
            book.rect(xywh(x - 0.7, y - 7.0, 1.4, 14.0), c);
        }
        // tune / toolchain
        7 => {
            for (k, dy) in [(-5.0, -5.0f32), (2.0, 0.0), (-2.0, 5.0)] {
                book.rect(xywh(x - 9.0, y + dy, 18.0, 1.4), c);
                book.circle(Offset::new(x + k, y + dy + 0.7), 2.6, c);
            }
        }
        // branch / source control
        8 => {
            book.circle(Offset::new(x - 5.0, y - 6.0), 2.8, c);
            book.circle(Offset::new(x + 6.0, y + 1.0), 2.8, c);
            book.circle(Offset::new(x - 5.0, y + 8.0), 2.8, c);
            book.line(Offset::new(x - 5.0, y - 6.0), Offset::new(x - 5.0, y + 8.0), c, 1.3);
            book.line(Offset::new(x - 5.0, y + 1.0), Offset::new(x + 6.0, y + 1.0), c, 1.3);
        }
        // export
        9 => {
            book.stroke_rrect(xywh(x - 8.0, y - 1.0, 16.0, 9.0), 1.5, c, 1.4);
            let mut p = Path::new();
            p.move_to(Offset::new(x, y - 9.0));
            p.line_to(Offset::new(x, y + 2.0));
            book.stroke(p, c, 1.6);
            let mut q = Path::new();
            q.move_to(Offset::new(x - 4.0, y - 5.0));
            q.line_to(Offset::new(x, y - 9.5));
            q.line_to(Offset::new(x + 4.0, y - 5.0));
            book.stroke(q, c, 1.5);
        }
        // swatch / tokens
        10 => {
            book.circle(Offset::new(x - 3.0, y - 3.0), 4.6, c);
            book.ring(Offset::new(x + 3.0, y + 3.0), 4.6, 1.4, c);
        }
        // gear / settings
        _ => {
            book.ring(Offset::new(x, y), 6.5, 1.4, c);
            book.circle(Offset::new(x, y), 2.0, c);
            for k in 0..6 {
                let a = k as f32 / 6.0 * std::f32::consts::TAU;
                let (s, co) = a.sin_cos();
                book.line(
                    Offset::new(x + s * 6.5, y - co * 6.5),
                    Offset::new(x + s * 9.0, y - co * 9.0),
                    c,
                    1.4,
                );
            }
        }
    }
}

// ── The draw: front ─────────────────────────────────────────────────────────

/// Everything over the preview's content.
pub fn front(book: &mut Sketchbook, l: &Layout, sh: &Shell) {
    let a = clamp01(sh.assembly);
    if a <= 0.01 {
        return;
    }

    // The diagnostic: a squiggle under the offending line, and a mark in the
    // gutter's margin.
    if let Some(d) = &sh.diag {
        let y = l.line_y(d.line) + CODE_SIZE + 2.0;
        let w = (l.code.width() - CODE_PAD_X * 2.0) * 0.38 * clamp01(d.reveal);
        if w > 2.0 && y < l.code.bottom {
            let mut p = Path::new();
            let x0 = l.code.left + CODE_PAD_X;
            p.move_to(Offset::new(x0, y));
            let mut x = x0;
            let mut up = true;
            while x < x0 + w {
                x += 3.0;
                p.line_to(Offset::new(x, if up { y - 2.2 } else { y + 2.2 }));
                up = !up;
            }
            book.stroke(p, alpha(d.severity.color(), 0.95), 1.5);
        }
        book.circle(
            Offset::new(l.gutter.left + 12.0, l.line_y(d.line) + CODE_SIZE * 0.5),
            4.0 * clamp01(d.reveal * 1.4),
            d.severity.color(),
        );
    }

    // The inspector, over the preview body.
    if let Some(ins) = &sh.inspector {
        let o = ease_out_cubic(ins.open);
        if o > 0.01 {
            let h = l.preview_body.height() * 0.48;
            let r = Rect::new(
                l.preview_body.left + 1.0,
                l.preview_body.bottom - h * o,
                l.preview_body.right - 1.0,
                l.preview_body.bottom - 1.0,
            );
            book.rect(xywh(r.left, r.top, r.width(), r.height()), alpha(CHROME_0, 0.98));
            book.rect(xywh(r.left, r.top, r.width(), 1.0), alpha(ACCENT, 0.55));
            book.rect(
                xywh(r.left + r.width() * 0.52, r.top + 30.0, 1.0, (r.height() - 38.0).max(0.0)),
                alpha(LINE, 0.9),
            );
            for (i, (_d, _n, sel)) in ins.tree.iter().enumerate() {
                if *sel {
                    let y = r.top + 36.0 + i as f32 * 22.0;
                    if y + 20.0 < r.bottom {
                        book.rrect(
                            xywh(r.left + 8.0, y - 3.0, r.width() * 0.52 - 18.0, 20.0),
                            4.0,
                            ACCENT_WASH,
                        );
                    }
                }
            }
        }

        // The damage rectangles — the product's own overlay, and the film's
        // most load-bearing one.
        for (rect, strength) in &ins.damage {
            let r = unit_to(*rect, l.preview_body);
            book.stroke_rrect(r, 3.0, alpha(ERROR, 0.95 * strength), 2.0);
            book.rrect(r, 3.0, alpha(ERROR, 0.12 * strength));
        }
        if let Some(sel) = ins.selection {
            let r = unit_to(sel, l.preview_body);
            book.rrect(r, 3.0, alpha(syn::ATTRIBUTE, 0.10));
            crate::kit::brackets(book, r, 16.0, alpha(syn::ATTRIBUTE, 0.95), 2.0);
        }
    }

    // The focus ring — the product draws it in the accent.
    if let Some(f) = sh.focus {
        let r = match f {
            FocusOn::Code => l.editor,
            FocusOn::Preview => l.preview,
            FocusOn::Sidebar => l.sidebar,
            FocusOn::Status => l.status,
        };
        book.stroke_rrect(
            Rect::new(r.left + 1.0, r.top + 1.0, r.right - 1.0, r.bottom - 1.0),
            CARD_CORNER - 1.0,
            alpha(ACCENT, 0.5),
            1.5,
        );
    }

    // The command palette: a raised card on `chrome_3`, over a scrim.
    if let Some(p) = &sh.palette {
        let o = ease_out_cubic(p.open);
        if o > 0.01 {
            book.rrect(l.window, CARD_CORNER + 4.0, alpha(Color::BLACK, 0.5 * o));
            let pw = 680.0;
            let ph = 56.0 + p.rows.len() as f32 * 40.0;
            let px = l.window.left + (l.window.width() - pw) * 0.5;
            let py = l.window.top + 76.0 - 26.0 * (1.0 - o);
            let r = xywh(px, py, pw, ph);
            book.shadow(r, CARD_CORNER, elevation(4));
            book.rrect(r, CARD_CORNER, sheen(CHROME_3));
            book.stroke_rrect(r, CARD_CORNER, alpha(LINE, 1.0), 1.0);
            book.rect(
                xywh(r.left + CARD_CORNER, r.top + 1.0, r.width() - CARD_CORNER * 2.0, 1.0),
                rim(CHROME_3),
            );
            book.rect(xywh(r.left + 1.0, r.top + 48.0, r.width() - 2.0, 1.0), alpha(LINE, 1.0));
            book.rrect(xywh(r.left + 14.0, r.top + 14.0, 2.5, 20.0), 1.2, ACCENT_SOFT);
            for i in 0..p.rows.len() {
                if i == p.selected {
                    book.rrect(
                        xywh(r.left + 6.0, r.top + 54.0 + i as f32 * 40.0, r.width() - 12.0, 36.0),
                        5.0,
                        ACCENT_WASH,
                    );
                }
            }
        }
    }

    if sh.dim > 0.004 {
        book.rrect(l.window, CARD_CORNER + 4.0, alpha(Color::BLACK, 0.72 * sh.dim));
    }
}

/// Map a `0..=1` rect onto a real one.
#[must_use]
pub fn unit_to(u: Rect, into: Rect) -> Rect {
    Rect::new(
        into.left + u.left * into.width(),
        into.top + u.top * into.height(),
        into.left + u.right * into.width(),
        into.top + u.bottom * into.height(),
    )
}

// ── The draw: text ──────────────────────────────────────────────────────────

/// Every glyph the studio shows.
#[must_use]
pub fn text(l: &Layout, sh: &Shell) -> Vec<WidgetNode> {
    let mut out: Vec<WidgetNode> = Vec::new();
    let a = clamp01(sh.assembly);
    if a <= 0.02 {
        return out;
    }

    // Title bar: the brand, the centred document title, the lineage.
    out.push(
        Type::new("viewwstudio")
            .size(13.0)
            .medium()
            .track(0.2)
            .color(alpha(ON_SURFACE, 0.95))
            .at(l.titlebar.left + 116.0, l.titlebar.top + 10.0)
            .width(200.0)
            .into(),
    );
    out.push(
        Type::new(&sh.title)
            .size(13.0)
            .color(alpha(ON_SURFACE, 0.78))
            .center()
            .at(l.titlebar.left, l.titlebar.top + 10.0)
            .width(l.titlebar.width())
            .into(),
    );
    out.push(
        Type::new(format!("built on {}", sh.subtitle))
            .mono()
            .size(11.0)
            .track(1.4)
            .color(alpha(syn::TYPE_NAME, 0.8))
            .right()
            .at(l.titlebar.right - 236.0, l.titlebar.top + 11.0)
            .width(220.0)
            .into(),
    );

    // Sidebar: the section header and the tree.
    out.push(
        Type::new("EXPLORER")
            .mono()
            .size(10.5)
            .track(2.4)
            .color(alpha(GUTTER, 0.9))
            .at(l.sidebar.left + 14.0, l.sidebar.top + 11.0)
            .width(180.0)
            .into(),
    );
    for (i, f) in sh.files.iter().enumerate() {
        let y = l.sidebar.top + SIDEBAR_HEADER + 8.0 + i as f32 * SIDEBAR_ROW;
        if y + SIDEBAR_ROW > l.sidebar.bottom - 6.0 {
            break;
        }
        let x = l.sidebar.left + 12.0 + f.depth as f32 * 14.0;
        out.push(
            Type::new(f.kind.glyph())
                .size(10.5)
                .color(alpha(f.kind.color(), 0.85))
                .at(x, y + 4.0)
                .width(18.0)
                .into(),
        );
        out.push(
            Type::new(&f.name)
                .size(13.0)
                .color(if f.active { Color::WHITE } else { alpha(ON_SURFACE, 0.78) })
                .at(x + 16.0, y + 2.0)
                .width(l.sidebar.right - x - 24.0)
                .into(),
        );
    }

    // Tabs.
    let mut x = l.tabs.left + 4.0;
    for (name, kind, active, dirty) in &sh.tabs {
        let w = 12.0 * 2.0 + 13.0 + 7.0 * 2.0 + 17.0 + name.chars().count() as f32 * 6.9;
        out.push(
            Type::new(kind.glyph())
                .size(10.0)
                .color(alpha(kind.color(), if *active { 0.95 } else { 0.5 }))
                .at(x + 12.0, l.tabs.top + 12.0)
                .width(14.0)
                .into(),
        );
        out.push(
            Type::new(name)
                .size(13.0)
                .color(if *active { Color::WHITE } else { alpha(ON_SURFACE, 0.62) })
                .at(x + 26.0, l.tabs.top + 10.0)
                .width((w - 44.0).max(20.0))
                .into(),
        );
        if *dirty {
            out.push(
                Type::new("●")
                    .size(9.0)
                    .color(alpha(ACCENT_SOFT, 0.9))
                    .at(x + w - 18.0, l.tabs.top + 12.0)
                    .width(12.0)
                    .into(),
            );
        }
        x += w + 2.0;
    }

    // Breadcrumb.
    let crumb = sh.crumbs.join("  ›  ");
    out.push(
        Type::new(crumb)
            .size(11.5)
            .color(alpha(GUTTER, 0.9))
            .at(l.breadcrumb.left + CODE_PAD_X, l.breadcrumb.top + 6.0)
            .width(l.breadcrumb.width() - CODE_PAD_X * 2.0)
            .into(),
    );

    // The gutter's numbers and the code itself.
    let code_style = TextStyle::new(CODE_SIZE).monospace().line_height(1.0).color(ON_SURFACE);
    for (i, line) in sh.code.iter().enumerate() {
        let y = l.line_y(i);
        if y + CODE_LEAD > l.code.bottom {
            break;
        }
        let on_caret = sh.caret == Some(i);
        out.push(
            Type::new(format!("{}", i + 1))
                .mono()
                .size(13.0)
                .color(if on_caret { GUTTER_ACTIVE } else { alpha(GUTTER, 0.9) })
                .right()
                .at(l.gutter.left + 6.0, y + 3.0)
                .width(l.gutter.width() - 20.0)
                .into(),
        );
        if line.parts.is_empty() && !(on_caret && sh.caret_on) {
            continue;
        }
        let mut spans: Vec<Span> = line
            .parts
            .iter()
            .map(|(s, t)| Span::new(s.clone()).color(t.color()))
            .collect();
        // The caret is the line's last span — so it lands exactly where the
        // typing stopped, with no advance arithmetic anywhere in the film.
        if on_caret && sh.caret_on {
            spans.push(Span::new("▌").color(alpha(ACCENT_SOFT, 0.95)));
        }
        out.push(
            Stack::new()
                .push(
                    Positioned::new()
                        .left(l.code.left + CODE_PAD_X)
                        .top(y)
                        .width(l.code.width() - CODE_PAD_X * 1.4)
                        .height(CODE_LEAD)
                        .child(RichText::new(spans).style(code_style).align(TextAlign::Start)),
                )
                .into(),
        );
    }

    // The diagnostic reads as an inline lens at the end of its own line,
    // where the editor puts it — never on the line below, which is somebody
    // else's code.
    if let Some(d) = &sh.diag {
        if d.reveal > 0.35 {
            out.push(
                Type::new(&d.message)
                    .mono()
                    .size(12.0)
                    .color(alpha(d.severity.color(), 0.92 * clamp01((d.reveal - 0.35) / 0.3)))
                    .right()
                    .at(l.code.left + l.code.width() * 0.46, l.line_y(d.line) + 3.0)
                    .width(l.code.width() * 0.50)
                    .into(),
            );
        }
    }

    // The preview bar.
    out.push(
        Type::new(&sh.preview_label)
            .size(12.5)
            .color(alpha(ON_SURFACE, 0.85))
            .center()
            .at(l.preview_bar.left, l.preview_bar.top + 10.0)
            .width(l.preview_bar.width())
            .into(),
    );
    out.push(
        Type::new(&sh.preview_chip)
            .mono()
            .size(11.0)
            .color(alpha(GUTTER, 0.9))
            .right()
            .at(l.preview_bar.right - 146.0, l.preview_bar.top + 11.0)
            .width(108.0)
            .into(),
    );

    // The bottom panel.
    let mut px = l.panel_tabs.left + 14.0;
    for (i, t) in sh.panel.tabs.iter().enumerate() {
        let w = t.chars().count() as f32 * 7.2 + 20.0;
        out.push(
            Type::new(t)
                .size(12.0)
                .color(if i == sh.panel.active {
                    Color::WHITE
                } else {
                    alpha(GUTTER, 0.85)
                })
                .at(px, l.panel_tabs.top + 9.0)
                .width(w)
                .into(),
        );
        px += w + 6.0;
    }
    if l.panel_open {
        for (i, (line, color)) in sh.panel.lines.iter().enumerate() {
            let y = l.panel_body.top + 10.0 + i as f32 * 21.0;
            if y + 18.0 > l.panel_body.bottom {
                break;
            }
            out.push(
                Type::new(line)
                    .mono()
                    .size(12.5)
                    .color(alpha(*color, 0.92))
                    .at(l.panel_body.left + 14.0, y)
                    .width(l.panel_body.width() - 28.0)
                    .into(),
            );
        }
    }

    // The status bar.
    let mut sx = l.status.left + 14.0;
    for (label, color) in &sh.status {
        out.push(
            Type::new(label)
                .mono()
                .size(11.5)
                .track(0.8)
                .color(alpha(*color, 0.95))
                .at(sx, l.status.top + 5.0)
                .width(400.0)
                .into(),
        );
        sx += label.chars().count() as f32 * 8.0 + 28.0;
    }
    if let Some(e) = &sh.elapsed {
        out.push(
            Type::new(format!("session {e}"))
                .mono()
                .size(11.5)
                .track(0.8)
                .color(alpha(GUTTER, 0.9))
                .right()
                .at(l.status.right - 220.0, l.status.top + 5.0)
                .width(206.0)
                .into(),
        );
    }

    // The inspector's rows.
    if let Some(ins) = &sh.inspector {
        let o = ease_out_cubic(ins.open);
        if o > 0.2 {
            let h = l.preview_body.height() * 0.48;
            let r = Rect::new(
                l.preview_body.left + 1.0,
                l.preview_body.bottom - h * o,
                l.preview_body.right - 1.0,
                l.preview_body.bottom - 1.0,
            );
            out.push(
                Type::new("INSPECTOR")
                    .mono()
                    .size(10.5)
                    .track(2.4)
                    .color(alpha(syn::ATTRIBUTE, 0.95))
                    .opacity(o)
                    .at(r.left + 14.0, r.top + 10.0)
                    .width(200.0)
                    .into(),
            );
            for (i, (depth, name, sel)) in ins.tree.iter().enumerate() {
                let y = r.top + 36.0 + i as f32 * 22.0;
                if y + 18.0 > r.bottom {
                    break;
                }
                out.push(
                    Type::new(name)
                        .mono()
                        .size(12.5)
                        .color(if *sel { syn::ATTRIBUTE } else { alpha(ON_SURFACE, 0.7) })
                        .opacity(o)
                        .at(r.left + 18.0 + *depth as f32 * 16.0, y)
                        .width(r.width() * 0.52 - 28.0)
                        .into(),
                );
            }
            for (i, (k, v)) in ins.facts.iter().enumerate() {
                let y = r.top + 36.0 + i as f32 * 24.0;
                if y + 18.0 > r.bottom {
                    break;
                }
                let fx = r.left + r.width() * 0.52 + 16.0;
                out.push(
                    Type::new(k)
                        .mono()
                        .size(12.0)
                        .color(alpha(GUTTER, 0.95))
                        .opacity(o)
                        .at(fx, y)
                        .width(180.0)
                        .into(),
                );
                out.push(
                    Type::new(v)
                        .mono()
                        .size(12.0)
                        .color(alpha(Color::WHITE, 0.95))
                        .opacity(o)
                        .at(fx + 186.0, y)
                        .width((r.right - fx - 196.0).max(40.0))
                        .into(),
                );
            }
        }
    }

    // The palette's query and rows.
    if let Some(p) = &sh.palette {
        let o = ease_out_cubic(p.open);
        if o > 0.15 {
            let pw = 680.0;
            let px = l.window.left + (l.window.width() - pw) * 0.5;
            let py = l.window.top + 76.0 - 26.0 * (1.0 - o);
            out.push(
                Type::new(&p.query)
                    .mono()
                    .size(16.0)
                    .color(Color::WHITE)
                    .opacity(o)
                    .at(px + 28.0, py + 15.0)
                    .width(pw - 52.0)
                    .into(),
            );
            for (i, (name, hint)) in p.rows.iter().enumerate() {
                let y = py + 54.0 + i as f32 * 40.0;
                out.push(
                    Type::new(name)
                        .size(14.5)
                        .color(if i == p.selected { Color::WHITE } else { alpha(ON_SURFACE, 0.72) })
                        .opacity(o)
                        .at(px + 22.0, y + 10.0)
                        .width(pw * 0.62)
                        .into(),
                );
                out.push(
                    Type::new(hint)
                        .mono()
                        .size(11.5)
                        .color(alpha(GUTTER, 0.9))
                        .opacity(o)
                        .right()
                        .at(px + pw * 0.64, y + 12.0)
                        .width(pw * 0.34 - 22.0)
                        .into(),
                );
            }
        }
    }

    if let Some(n) = sh.witness {
        out.push(witness_chip(l.preview_body, n, 1.0));
    }

    out
}

/// The counter chip — the one number the whole session hangs on. The film's
/// own instrument, drawn in the film's accent, sitting in the product's
/// preview because that is where the session is.
#[must_use]
pub fn witness_chip(body: Rect, n: u32, a: f32) -> WidgetNode {
    let x = body.right - 104.0;
    let y = body.bottom - 96.0;
    let plate = Painting::sized(
        Size::new(86.0, 80.0),
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            book.rrect(xywh(0.0, 0.0, 86.0, 80.0), 10.0, alpha(crate::kit::WASH, 0.94));
            book.stroke_rrect(xywh(0.0, 0.0, 86.0, 80.0), 10.0, alpha(ACCENT, 0.55), 1.0);
            book.blended_layer(1.0, 15.0, BlendMode::Plus, None, |g| {
                g.circle(Offset::new(43.0, 36.0), 30.0, alpha(ACCENT, 0.16));
            });
        }),
    );
    Stack::new()
        .push(
            Positioned::new()
                .left(x)
                .top(y)
                .width(86.0)
                .height(80.0)
                .child(Opacity::new(clamp01(a)).child(plate)),
        )
        .push(
            Type::new(format!("{n}"))
                .size(42.0)
                .bold()
                .color(crate::kit::INK)
                .opacity(a)
                .center()
                .at(x, y + 10.0)
                .width(86.0),
        )
        .push(
            Type::new("witness")
                .mono()
                .size(9.0)
                .track(1.6)
                .color(alpha(ACCENT_SOFT, 0.95))
                .opacity(a)
                .center()
                .at(x, y + 62.0)
                .width(86.0),
        )
        .into()
}

// ── The counter app — what the session is actually building ─────────────────

/// The preview's content: the little counter app the session writes. Drawn
/// into any rectangle, so the same app appears in the studio's preview, in a
/// browser and on a phone — which is what makes S12's tour a tour of one
/// program rather than three pictures.
pub struct App {
    /// The counter's value — the witness number itself.
    pub value: u32,
    /// `0..=1` — the app's own paint-in.
    pub alive: f32,
    /// The press ripple, `0..=1`, or `None` when nothing was tapped.
    pub press: Option<f32>,
    /// The dial's subtree failed and the boundary is standing in for it.
    pub broken: bool,
    /// The app's own accent, which S07 changes live.
    pub accent: Color,
    /// A spring-driven bar the app animates, so the preview is never still.
    pub spring: f32,
    /// Compact chrome for the phone.
    pub compact: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            value: 0,
            alive: 1.0,
            press: None,
            broken: false,
            accent: ACCENT_SOFT,
            spring: 0.0,
            compact: false,
        }
    }
}

/// Where the app's parts land inside `r` — one place, so the painting and the
/// glyphs cannot disagree.
#[must_use]
fn app_metrics(r: Rect) -> (f32, f32, f32, Rect, Rect) {
    let cx = r.left + r.width() * 0.5;
    let cy = r.top + r.height() * 0.40;
    let dial_r = (r.width().min(r.height()) * 0.19).max(22.0);
    let bar_w = r.width() * 0.46;
    let bar = xywh(cx - bar_w * 0.5, cy + dial_r + 30.0, bar_w, 8.0);
    let bw = (r.width() * 0.34).min(230.0);
    let btn = xywh(cx - bw * 0.5, bar.bottom + 26.0, bw, 48.0);
    (cx, cy, dial_r, bar, btn)
}

/// Paint the counter app into `r`.
pub fn app(book: &mut Sketchbook, r: Rect, s: &App) {
    let a = clamp01(s.alive);
    if a <= 0.01 {
        return;
    }
    let sc = smoothstep(a);
    let (cx, cy, dial_r0, bar, btn0) = app_metrics(r);
    let dial_r = dial_r0 * (0.86 + 0.14 * sc);

    // The app's own ground — the previewed program's surface, not the pane's.
    book.rect(
        xywh(r.left, r.top, r.width(), r.height()),
        Gradient::vertical().with_dither().with_stops(&[
            (0.0, alpha(Color::rgb(0x16, 0x16, 0x18), a)),
            (1.0, alpha(Color::rgb(0x10, 0x10, 0x12), a)),
        ]),
    );

    if s.broken {
        // The boundary replaces **only the subtree that failed** — the dial,
        // whose value could not be resolved. Everything around it is painted
        // by the same frame, which is the entire point of the scene.
        let bw = dial_r0 * 2.5;
        let cardr = Rect::new(cx - bw * 0.5, cy - dial_r0 * 1.2, cx + bw * 0.5, cy + dial_r0 * 1.2);
        book.rrect(cardr, 8.0, alpha(Color::rgb(0x3A, 0x1B, 0x1C), 0.97));
        book.stroke_rrect(cardr, 8.0, alpha(ERROR, 0.75), 1.5);
        book.rrect(xywh(cardr.left, cardr.top, cardr.width(), 2.5), 1.2, ERROR);
        // The hatch — one clipped group for the lot, not one per stripe.
        book.layer(1.0, 0.0, Some(Path::rounded_rect(cardr, 8.0)), |g| {
            let mut x = cardr.left - cardr.height();
            while x < cardr.right {
                let mut p = Path::new();
                p.move_to(Offset::new(x, cardr.bottom));
                p.line_to(Offset::new(x + cardr.height(), cardr.top));
                g.stroke(p, alpha(ERROR, 0.12), 2.0);
                x += 14.0;
            }
        });
    } else {
        book.ring(Offset::new(cx, cy), dial_r, 8.0, alpha(Color::rgb(0x2C, 0x2C, 0x30), a));
        let p = clamp01(s.value as f32 / 7.0);
        if p > 0.001 {
            crate::kit::arc_sweep(
                book,
                Offset::new(cx, cy),
                dial_r,
                8.0,
                -std::f32::consts::FRAC_PI_2,
                p * std::f32::consts::TAU,
                Gradient::sweep(Offset::new(0.5, 0.5), 0.0, std::f32::consts::TAU)
                    .with_dither()
                    .with_stops(&[
                        (0.0, alpha(ACCENT, a)),
                        (0.6, alpha(s.accent, a)),
                        (1.0, alpha(syn::TYPE_NAME, a)),
                    ]),
            );
        }
        book.blended_layer(a, dial_r * 0.5, BlendMode::Plus, None, |g| {
            g.circle(Offset::new(cx, cy), dial_r * 0.95, alpha(s.accent, 0.13));
        });
    }

    if bar.bottom < r.bottom - 6.0 {
        meter(
            book,
            bar,
            s.spring,
            alpha(Color::rgb(0x28, 0x28, 0x2C), a),
            Gradient::horizontal().with_dither().with_stops(&[
                (0.0, alpha(ACCENT, a)),
                (1.0, alpha(s.accent, a)),
            ]),
        );
    }

    let btn = if s.compact {
        Rect::new(btn0.left, btn0.top, btn0.right, btn0.top + 42.0)
    } else {
        btn0
    };
    if btn.bottom < r.bottom - 6.0 {
        let pressed = s.press.is_some_and(|p| p < 0.35);
        let shift = if pressed { 1.5 } else { 0.0 };
        let b = Rect::new(btn.left, btn.top + shift, btn.right, btn.bottom + shift);
        // The product's own button gradient: the accent's near stop into its
        // far stop, which is the ramp `Brand` exists to keep consistent.
        book.rrect(
            b,
            b.height() * 0.5,
            Gradient::vertical().with_dither().with_stops(&[
                (0.0, alpha(tint(s.accent, 0.06), a)),
                (1.0, alpha(ACCENT, a)),
            ]),
        );
        if let Some(pr) = s.press {
            let rr = b.height() * 0.5 + (b.width() * 0.6) * ease_out_cubic(pr);
            book.layer(1.0 - pr, 0.0, Some(Path::rounded_rect(b, b.height() * 0.5)), |g| {
                g.circle(
                    Offset::new(b.left + b.width() * 0.5, b.top + b.height() * 0.5),
                    rr,
                    alpha(Color::WHITE, 0.22),
                );
            });
        }
    }
}

/// The app's own glyphs — returned separately so the shaper places them.
#[must_use]
pub fn app_text(r: Rect, s: &App) -> Vec<WidgetNode> {
    let a = clamp01(s.alive);
    if a <= 0.02 {
        return Vec::new();
    }
    let (cx, cy, dial_r, _bar, btn0) = app_metrics(r);
    let big = (dial_r * 0.95).clamp(20.0, 82.0);
    let bw = btn0.width();
    let label_dy = if s.compact { 12.0 } else { 15.0 };

    let mut out: Vec<WidgetNode> = Vec::new();

    if s.broken {
        let cw = dial_r * 2.5;
        let cardr = Rect::new(cx - cw * 0.5, cy - dial_r * 1.2, cx + cw * 0.5, cy + dial_r * 1.2);
        out.push(
            Type::new("dial")
                .mono()
                .size(11.5)
                .track(2.0)
                .color(alpha(ERROR, 0.95 * a))
                .center()
                .at(cardr.left, cardr.top + 18.0)
                .width(cardr.width())
                .into(),
        );
        out.push(
            Type::new("boundary")
                .size(20.0)
                .medium()
                .color(alpha(Color::WHITE, 0.93 * a))
                .center()
                .at(cardr.left, cardr.top + cardr.height() * 0.42)
                .width(cardr.width())
                .into(),
        );
        out.push(
            Type::new("subtree held")
                .mono()
                .size(10.0)
                .track(1.4)
                .color(alpha(GUTTER, 0.95 * a))
                .center()
                .at(cardr.left, cardr.bottom - 22.0)
                .width(cardr.width())
                .into(),
        );
    } else {
        out.push(
            Type::new(format!("{}", s.value))
                .size(big)
                .bold()
                .color(alpha(Color::WHITE, a))
                .center()
                .at(cx - 150.0, cy - big * 0.66)
                .width(300.0)
                .into(),
        );
        out.push(
            Type::new("taps")
                .mono()
                .size(10.5)
                .track(3.0)
                .color(alpha(GUTTER, 0.95 * a))
                .center()
                .at(cx - 150.0, cy + big * 0.42)
                .width(300.0)
                .into(),
        );
    }

    // The button's label is a **sibling** of the dial, so it is on screen in
    // both states — which is what makes S07's sentence checkable rather than
    // asserted.
    if btn0.top + 48.0 < r.bottom - 6.0 {
        out.push(
            Type::new("count")
                .size(if s.compact { 14.0 } else { 16.0 })
                .medium()
                .color(alpha(Color::WHITE, 0.96 * a))
                .center()
                .at(cx - bw * 0.5, btn0.top + label_dy)
                .width(bw)
                .into(),
        );
    }
    out
}

// ── Device shells — the world tour's frames ─────────────────────────────────

/// A desktop window frame — the native surface.
pub fn shell_desktop(book: &mut Sketchbook, r: Rect, accent: Color, a: f32) -> Rect {
    book.shadow(r, 10.0, elevation(3));
    book.rrect(r, 10.0, sheen(CHROME_0));
    book.stroke_rrect(r, 10.0, alpha(LINE, a), 1.0);
    let bar = Rect::new(r.left, r.top, r.right, r.top + 28.0);
    for (i, c) in [
        Color::rgb(0xE0, 0x6C, 0x60),
        Color::rgb(0xE0, 0xA8, 0x4E),
        Color::rgb(0x5C, 0xB8, 0x60),
    ]
    .into_iter()
    .enumerate()
    {
        book.circle(Offset::new(r.left + 16.0 + i as f32 * 14.0, r.top + 14.0), 4.0, alpha(c, 0.85 * a));
    }
    book.rrect(xywh(r.left + r.width() * 0.5 - 36.0, r.top + 11.0, 72.0, 6.0), 3.0, alpha(accent, 0.35 * a));
    book.rect(xywh(bar.left + 1.0, bar.bottom, bar.width() - 2.0, 1.0), alpha(LINE, a));
    Rect::new(r.left + 1.0, bar.bottom, r.right - 1.0, r.bottom - 1.0)
}

/// A browser frame — the real-DOM surface.
pub fn shell_browser(book: &mut Sketchbook, r: Rect, a: f32) -> Rect {
    book.shadow(r, 10.0, elevation(3));
    book.rrect(r, 10.0, sheen(CHROME_0));
    book.stroke_rrect(r, 10.0, alpha(LINE, a), 1.0);
    let bar = Rect::new(r.left, r.top, r.right, r.top + 50.0);
    for i in 0..3 {
        book.circle(Offset::new(r.left + 16.0 + i as f32 * 14.0, r.top + 13.0), 4.0, alpha(GUTTER, 0.5 * a));
    }
    book.rrect(xywh(r.left + 64.0, r.top + 3.0, 164.0, 21.0), 6.0, alpha(CHROME_3, a));
    book.rrect(xywh(r.left + 14.0, r.top + 30.0, r.width() - 28.0, 15.0), 7.5, alpha(WINDOW, a));
    book.rect(xywh(bar.left + 1.0, bar.bottom, bar.width() - 2.0, 1.0), alpha(LINE, a));
    Rect::new(r.left + 1.0, bar.bottom, r.right - 1.0, r.bottom - 1.0)
}

/// A phone shell with a notch — the 2019 Android the receipts came from.
pub fn shell_phone(book: &mut Sketchbook, r: Rect, a: f32) -> Rect {
    book.shadow(r, 24.0, elevation(4));
    book.rrect(r, 24.0, alpha(Color::rgb(0x16, 0x16, 0x16), a));
    book.stroke_rrect(r, 24.0, alpha(Color::rgb(0x3A, 0x3A, 0x3A), 0.95 * a), 2.0);
    let screen = Rect::new(r.left + 7.0, r.top + 7.0, r.right - 7.0, r.bottom - 7.0);
    book.rrect(screen, 18.0, alpha(Color::rgb(0x0C, 0x0C, 0x0D), a));
    book.rrect(
        xywh(screen.left + screen.width() * 0.5 - 32.0, screen.top + 5.0, 64.0, 11.0),
        5.5,
        alpha(Color::BLACK, 0.92 * a),
    );
    book.rrect(
        xywh(screen.left + screen.width() * 0.5 - 38.0, screen.bottom - 11.0, 76.0, 4.0),
        2.0,
        alpha(Color::WHITE, 0.25 * a),
    );
    Rect::new(screen.left, screen.top + 20.0, screen.right, screen.bottom - 18.0)
}

/// A finger: the tap that lands on the phone and is felt on the desktop.
pub fn touch(book: &mut Sketchbook, at: Offset, p: f32, accent: Color) {
    if p <= 0.0 || p >= 1.0 {
        return;
    }
    let e = ease_out_cubic(p);
    book.ring(at, 12.0 + 54.0 * e, 2.5 * (1.0 - e) + 0.6, alpha(accent, 0.8 * (1.0 - e)));
    book.circle(at, 14.0 * (1.0 - e * 0.5), alpha(Color::WHITE, 0.5 * (1.0 - e)));
    book.blended_layer(1.0, 18.0, BlendMode::Plus, None, |g| {
        g.circle(at, 36.0, alpha(accent, 0.30 * (1.0 - e)));
    });
}

/// The lineage badge: **viewwstudio** over **built on vieww**.
#[must_use]
pub fn lineage_badge(x: f32, y: f32, scale: f32, a: f32) -> Vec<WidgetNode> {
    vec![
        Type::new("viewwstudio")
            .size(34.0 * scale)
            .medium()
            .track(1.0 * scale)
            .color(alpha(crate::kit::INK, a))
            .at(x, y)
            .width(600.0 * scale)
            .into(),
        Type::new("built on vieww")
            .mono()
            .size(13.0 * scale)
            .track(3.0 * scale)
            .color(alpha(crate::kit::CYAN, 0.85 * a))
            .at(x + 2.0, y + 44.0 * scale)
            .width(600.0 * scale)
            .into(),
    ]
}

/// Compose a scene's frame: one background painter, then layers on top.
#[must_use]
pub fn compose(bg: WidgetNode, nodes: Vec<WidgetNode>) -> WidgetNode {
    let mut s = Stack::new().push(Positioned::fill().child(bg));
    for n in nodes {
        s = s.push(Positioned::fill().child(n));
    }
    s.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_regions_tile_with_one_gutter_between_them() {
        let l = Layout::new(64.0, 0.34, true);
        assert!((l.sidebar.left - l.rail.right - CARD_GAP).abs() < 1e-3);
        assert!((l.editor.left - l.sidebar.right - CARD_GAP).abs() < 1e-3);
        assert!((l.preview.left - l.editor.right - CARD_GAP).abs() < 1e-3);
        assert!((l.panel.top - l.editor.bottom - CARD_GAP).abs() < 1e-3);
    }

    #[test]
    fn the_product_metrics_are_the_products() {
        let l = Layout::new(64.0, 0.34, true);
        assert!((l.titlebar.height() - 36.0).abs() < 1e-3);
        assert!((l.rail.width() - 48.0).abs() < 1e-3);
        assert!((l.status.height() - 24.0).abs() < 1e-3);
        assert!((l.panel.height() - PANEL_H).abs() < 1e-3);
        assert!((l.tabs.height() - 35.0).abs() < 1e-3);
    }

    #[test]
    fn the_window_leaves_the_caption_band_free() {
        let l = Layout::new(64.0, 0.34, true);
        assert!((H - l.window.bottom - CAPTION_BAND).abs() < 1e-3);
    }

    #[test]
    fn lines_tile_by_the_leading() {
        let l = Layout::new(64.0, 0.34, true);
        assert!((l.line_y(1) - l.line_y(0) - CODE_LEAD).abs() < 1e-3);
    }

    #[test]
    fn the_lexer_keeps_every_character() {
        let src = "view counter { text \"7\" }";
        assert_eq!(lex(src, false).text(), src);
    }

    #[test]
    fn a_truncated_line_is_a_prefix() {
        assert_eq!(lex("let count = 0", false).truncated(5).text(), "let c");
    }

    #[test]
    fn unit_rects_map_into_the_body() {
        let body = Rect::new(0.0, 0.0, 100.0, 200.0);
        let r = unit_to(Rect::new(0.5, 0.5, 1.0, 1.0), body);
        assert!((r.left - 50.0).abs() < 1e-3 && (r.bottom - 200.0).abs() < 1e-3);
    }

    #[test]
    fn the_button_sits_below_the_dial_in_both_states() {
        let r = Rect::new(0.0, 0.0, 600.0, 700.0);
        let (_cx, cy, dial_r, bar, btn) = app_metrics(r);
        assert!(bar.top > cy + dial_r);
        assert!(btn.top > bar.bottom);
    }
}
