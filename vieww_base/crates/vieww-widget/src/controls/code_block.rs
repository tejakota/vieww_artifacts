//! Syntax-highlighted code, and code that animates from one version to the
//! next — Motion Canvas' `Code` node with `edit`/diff transitions, Manim's
//! `Code` + `TransformMatchingLines`, and every docs site's code block
//! (§2.13, §2.16).
//!
//! [`highlight`] is a small, dependency-free lexer (strings with escapes,
//! line and block comments, numbers, keywords, types by capitalisation,
//! call sites by a following `(`) with keyword tables for Rust,
//! JavaScript/TypeScript, Python and JSON. [`diff_lines`] is an LCS line
//! diff. [`CodeBlock`] renders highlighted lines with an optional gutter
//! and highlighted line range; [`CodeMorph`] renders the transition between
//! two sources at progress `t`: kept lines slide to their new row, deleted
//! lines fade and collapse, inserted lines fade in where they land.

use std::fmt;

use vieww_foundation::{Color, EdgeInsets, FontWeight, Key, TextStyle};

use crate::{widget_node_from, BuildContext, Container, Opacity, Positioned, RichText, Span, Stack, Widget, WidgetKind, WidgetNode};

/// Source language for [`highlight`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    Rust,
    JavaScript,
    Python,
    Json,
    Plain,
}

impl Language {
    fn keywords(self) -> &'static [&'static str] {
        match self {
            Self::Rust => &[
                "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut",
                "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where", "while",
            ],
            Self::JavaScript => &[
                "async", "await", "break", "case", "catch", "class", "const", "continue", "default", "delete", "do", "else", "export", "extends", "false", "finally", "for", "from", "function", "if",
                "import", "in", "instanceof", "interface", "let", "new", "null", "of", "return", "static", "super", "switch", "this", "throw", "true", "try", "type", "typeof", "undefined", "var",
                "void", "while", "yield",
            ],
            Self::Python => &[
                "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if",
                "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "self", "try", "while", "with", "yield",
            ],
            Self::Json => &["true", "false", "null"],
            Self::Plain => &[],
        }
    }

    fn line_comment(self) -> Option<&'static str> {
        match self {
            Self::Rust | Self::JavaScript => Some("//"),
            Self::Python => Some("#"),
            _ => None,
        }
    }
}

/// A token's role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Plain,
    Keyword,
    Type,
    Function,
    String,
    Number,
    Comment,
    Punctuation,
    /// A JSON object key.
    Property,
}

/// A run of text with a role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
}

/// Highlight `src` into lines of tokens. Block comments and strings may
/// span lines.
#[must_use]
pub fn highlight(src: &str, lang: Language) -> Vec<Vec<Token>> {
    let chars: Vec<char> = src.chars().collect();
    let mut lines: Vec<Vec<Token>> = vec![Vec::new()];
    let push = |lines: &mut Vec<Vec<Token>>, kind: TokenKind, text: &str| {
        for (i, part) in text.split('\n').enumerate() {
            if i > 0 {
                lines.push(Vec::new());
            }
            if part.is_empty() {
                continue;
            }
            let line = lines.last_mut().expect("never empty");
            match line.last_mut() {
                Some(t) if t.kind == kind => t.text.push_str(part),
                _ => line.push(Token { kind, text: part.to_owned() }),
            }
        }
    };
    let kw = lang.keywords();
    let lc: Vec<char> = lang.line_comment().map(|s| s.chars().collect()).unwrap_or_default();
    let starts = |i: usize, pat: &[char]| !pat.is_empty() && chars.get(i..i + pat.len()) == Some(pat);
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if lang == Language::Plain {
            push(&mut lines, TokenKind::Plain, &c.to_string());
            i += 1;
            continue;
        }
        if starts(i, &lc) {
            let end = chars[i..].iter().position(|&c| c == '\n').map_or(chars.len(), |p| i + p);
            push(&mut lines, TokenKind::Comment, &chars[i..end].iter().collect::<String>());
            i = end;
            continue;
        }
        if matches!(lang, Language::Rust | Language::JavaScript) && starts(i, &['/', '*']) {
            let end = (i + 2..chars.len().saturating_sub(1)).find(|&j| chars[j] == '*' && chars[j + 1] == '/').map_or(chars.len(), |j| j + 2);
            push(&mut lines, TokenKind::Comment, &chars[i..end].iter().collect::<String>());
            i = end;
            continue;
        }
        let is_quote = c == '"' || (c == '\'' && lang != Language::Rust) || (c == '`' && lang == Language::JavaScript);
        if is_quote {
            let mut j = i + 1;
            while j < chars.len() && chars[j] != c {
                if chars[j] == '\\' {
                    j += 1;
                }
                j += 1;
            }
            let end = (j + 1).min(chars.len());
            let text: String = chars[i..end].iter().collect();
            // A JSON string followed by ':' is a key.
            let next = chars[end..].iter().find(|c| !c.is_whitespace());
            let kind = if lang == Language::Json && next == Some(&':') { TokenKind::Property } else { TokenKind::String };
            push(&mut lines, kind, &text);
            i = end;
            continue;
        }
        if c.is_ascii_digit() || (c == '-' && lang == Language::Json && chars.get(i + 1).is_some_and(char::is_ascii_digit)) {
            let mut j = i + 1;
            while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '.' || chars[j] == '_') {
                j += 1;
            }
            push(&mut lines, TokenKind::Number, &chars[i..j].iter().collect::<String>());
            i = j;
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let mut j = i + 1;
            while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                j += 1;
            }
            // Rust macros: `println!`.
            if lang == Language::Rust && chars.get(j) == Some(&'!') && chars.get(j + 1) != Some(&'=') {
                j += 1;
            }
            let word: String = chars[i..j].iter().collect();
            let next = chars[j..].iter().find(|c| **c != ' ');
            let kind = if kw.contains(&word.as_str()) {
                TokenKind::Keyword
            } else if next == Some(&'(') || word.ends_with('!') {
                TokenKind::Function
            } else if word.chars().next().is_some_and(char::is_uppercase) {
                TokenKind::Type
            } else {
                TokenKind::Plain
            };
            push(&mut lines, kind, &word);
            i = j;
            continue;
        }
        let kind = if c.is_whitespace() { TokenKind::Plain } else { TokenKind::Punctuation };
        push(&mut lines, kind, &c.to_string());
        i += 1;
    }
    lines
}

/// Colours per token kind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CodeTheme {
    pub background: Color,
    pub plain: Color,
    pub keyword: Color,
    pub ty: Color,
    pub function: Color,
    pub string: Color,
    pub number: Color,
    pub comment: Color,
    pub punctuation: Color,
    pub property: Color,
    pub gutter: Color,
    pub highlight: Color,
}

impl CodeTheme {
    /// A dark theme (One Dark-like).
    pub const DARK: Self = Self {
        background: Color::rgb(30, 33, 40),
        plain: Color::rgb(210, 215, 225),
        keyword: Color::rgb(198, 120, 221),
        ty: Color::rgb(229, 192, 123),
        function: Color::rgb(97, 175, 239),
        string: Color::rgb(152, 195, 121),
        number: Color::rgb(209, 154, 102),
        comment: Color::rgb(110, 118, 135),
        punctuation: Color::rgb(160, 168, 185),
        property: Color::rgb(224, 108, 117),
        gutter: Color::rgb(85, 92, 108),
        highlight: Color::rgba(97, 175, 239, 40),
    };

    /// A light theme (GitHub-like).
    pub const LIGHT: Self = Self {
        background: Color::rgb(248, 249, 251),
        plain: Color::rgb(36, 41, 47),
        keyword: Color::rgb(207, 34, 46),
        ty: Color::rgb(149, 56, 0),
        function: Color::rgb(130, 80, 223),
        string: Color::rgb(10, 48, 105),
        number: Color::rgb(5, 80, 174),
        comment: Color::rgb(110, 119, 129),
        punctuation: Color::rgb(87, 96, 106),
        property: Color::rgb(5, 80, 174),
        gutter: Color::rgb(160, 166, 174),
        highlight: Color::rgba(255, 213, 79, 70),
    };

    #[must_use]
    pub const fn color(&self, k: TokenKind) -> Color {
        match k {
            TokenKind::Plain => self.plain,
            TokenKind::Keyword => self.keyword,
            TokenKind::Type => self.ty,
            TokenKind::Function => self.function,
            TokenKind::String => self.string,
            TokenKind::Number => self.number,
            TokenKind::Comment => self.comment,
            TokenKind::Punctuation => self.punctuation,
            TokenKind::Property => self.property,
        }
    }
}

/// One line-diff operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineOp {
    /// Line `old` becomes line `new`, unchanged.
    Keep { old: usize, new: usize },
    Delete { old: usize },
    Insert { new: usize },
}

/// LCS diff of two line lists (in old/new order).
#[must_use]
pub fn diff_lines(a: &[&str], b: &[&str]) -> Vec<LineOp> {
    let (n, m) = (a.len(), b.len());
    let mut l = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            l[i][j] = if a[i] == b[j] { l[i + 1][j + 1] + 1 } else { l[i + 1][j].max(l[i][j + 1]) };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut ops = Vec::new();
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            ops.push(LineOp::Keep { old: i, new: j });
            i += 1;
            j += 1;
        } else if j < m && (i == n || l[i][j + 1] >= l[i + 1][j]) {
            ops.push(LineOp::Insert { new: j });
            j += 1;
        } else {
            ops.push(LineOp::Delete { old: i });
            i += 1;
        }
    }
    ops
}

fn line_widget(tokens: &[Token], theme: &CodeTheme, style: TextStyle) -> RichText {
    RichText::new(tokens.iter().map(|t| {
        let s = Span::new(t.text.clone()).color(theme.color(t.kind));
        if t.kind == TokenKind::Keyword {
            s.weight(FontWeight::Bold)
        } else if t.kind == TokenKind::Comment {
            s.italic()
        } else {
            s
        }
    }))
    .style(style)
}

/// Shared presentation settings.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Look {
    theme: CodeTheme,
    size: f32,
    line_height: f32,
    gutter: bool,
    width: Option<f32>,
}

impl Look {
    const fn new() -> Self {
        Self {
            theme: CodeTheme::DARK,
            size: 14.0,
            line_height: 1.5,
            gutter: true,
            width: None,
        }
    }

    fn row(&self) -> f32 {
        self.size * self.line_height
    }

    fn gutter_width(&self, lines: usize) -> f32 {
        if self.gutter {
            #[allow(clippy::cast_precision_loss)]
            let digits = lines.max(1).to_string().len().max(2) as f32;
            digits * self.size * 0.62 + self.size
        } else {
            0.0
        }
    }

    fn style(&self) -> TextStyle {
        let mut s = TextStyle::new(self.size).monospace();
        s.color = self.theme.plain;
        s
    }

    fn width_for(&self, lines: &[&str]) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        let chars = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f32;
        self.width.unwrap_or(self.gutter_width(lines.len()) + chars * self.size * 0.62 + 32.0)
    }
}

/// A highlighted code block.
#[derive(Clone)]
pub struct CodeBlock {
    source: String,
    language: Language,
    look: Look,
    highlighted: Option<(usize, usize)>,
    key: Option<Key>,
}

impl fmt::Debug for CodeBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CodeBlock").field("language", &self.language).field("lines", &self.source.lines().count()).finish_non_exhaustive()
    }
}

macro_rules! look_setters {
    () => {
        #[must_use]
        pub const fn theme(mut self, theme: CodeTheme) -> Self {
            self.look.theme = theme;
            self
        }

        #[must_use]
        pub const fn font_size(mut self, size: f32) -> Self {
            self.look.size = size;
            self
        }

        #[must_use]
        pub const fn line_numbers(mut self, on: bool) -> Self {
            self.look.gutter = on;
            self
        }

        /// Fix the block's width (otherwise estimated from the longest line).
        #[must_use]
        pub const fn width(mut self, w: f32) -> Self {
            self.look.width = Some(w);
            self
        }

        #[must_use]
        pub fn key(mut self, key: impl Into<Key>) -> Self {
            self.key = Some(key.into());
            self
        }
    };
}

impl CodeBlock {
    #[must_use]
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            language: Language::Rust,
            look: Look::new(),
            highlighted: None,
            key: None,
        }
    }

    #[must_use]
    pub const fn language(mut self, l: Language) -> Self {
        self.language = l;
        self
    }

    /// Highlight lines `first..=last` (0-based).
    #[must_use]
    pub const fn highlight_lines(mut self, first: usize, last: usize) -> Self {
        self.highlighted = Some((first, last));
        self
    }

    look_setters!();
}

impl Widget for CodeBlock {
    fn debug_name(&self) -> &'static str {
        "CodeBlock"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn key(&self) -> Option<&Key> {
        self.key.as_ref()
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let raw: Vec<&str> = self.source.lines().collect();
        let tokens = highlight(&self.source, self.language);
        let look = self.look;
        #[allow(clippy::cast_precision_loss)]
        let rows: Vec<Row> = tokens.into_iter().enumerate().map(|(i, t)| Row { tokens: t, y: i as f32, number: Some(i + 1), alpha: 1.0 }).collect();
        #[allow(clippy::cast_precision_loss)]
        let band = self.highlighted.map(|(a, b)| (a as f32, b as f32));
        render(&rows, &look, raw.len(), look.width_for(&raw), band)
    }
}

struct Row {
    tokens: Vec<Token>,
    /// Row position in line units (fractional while morphing).
    y: f32,
    number: Option<usize>,
    alpha: f32,
}

fn render(rows: &[Row], look: &Look, count: usize, width: f32, band: Option<(f32, f32)>) -> WidgetNode {
    let row = look.row();
    let pad = 12.0;
    let gutter = look.gutter_width(count);
    let height = rows.iter().map(|r| r.y + 1.0).fold(0.0f32, f32::max).max(1.0) * row + pad * 2.0;
    let mut kids: Vec<WidgetNode> = Vec::new();
    if let Some((a, b)) = band {
        kids.push(
            Positioned::new()
                .left(0.0)
                .top(pad + a * row)
                .width(width)
                .height((b - a + 1.0) * row)
                .child(Container::new().color(look.theme.highlight))
                .into(),
        );
    }
    let base = look.style();
    for r in rows {
        if r.alpha <= 0.001 {
            continue;
        }
        let top = pad + r.y * row + (row - look.size * 1.2) / 2.0;
        if let (true, Some(n)) = (look.gutter, r.number) {
            let mut gs = base;
            gs.color = look.theme.gutter;
            kids.push(
                Positioned::new()
                    .left(8.0)
                    .top(top)
                    .child(Opacity::new(r.alpha).child(RichText::new([Span::new(format!("{n:>2}"))]).style(gs)))
                    .into(),
            );
        }
        kids.push(
            Positioned::new()
                .left(gutter + 8.0)
                .top(top)
                .child(Opacity::new(r.alpha).child(line_widget(&r.tokens, &look.theme, base)))
                .into(),
        );
    }
    Container::new()
        .color(look.theme.background)
        .radius(10.0)
        .size(width, height)
        .padding(EdgeInsets::all(0.0))
        .child(Stack::new().children(kids))
        .into()
}

widget_node_from!(CodeBlock);

/// The animated transition from one source to another at `t` in 0..1.
#[derive(Clone)]
pub struct CodeMorph {
    from: String,
    to: String,
    t: f32,
    language: Language,
    look: Look,
    key: Option<Key>,
}

impl fmt::Debug for CodeMorph {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CodeMorph").field("t", &self.t).finish_non_exhaustive()
    }
}

impl CodeMorph {
    #[must_use]
    pub fn new(from: impl Into<String>, to: impl Into<String>, t: f32) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            t: t.clamp(0.0, 1.0),
            language: Language::Rust,
            look: Look::new(),
            key: None,
        }
    }

    #[must_use]
    pub const fn language(mut self, l: Language) -> Self {
        self.language = l;
        self
    }

    look_setters!();

    /// The per-line layout at the current `t`: (text, y in rows, alpha).
    #[must_use]
    pub fn layout(&self) -> Vec<(String, f32, f32)> {
        self.rows().into_iter().map(|r| (r.tokens.iter().map(|t| t.text.as_str()).collect(), r.y, r.alpha)).collect()
    }

    fn rows(&self) -> Vec<Row> {
        let a: Vec<&str> = self.from.lines().collect();
        let b: Vec<&str> = self.to.lines().collect();
        let (ha, hb) = (highlight(&self.from, self.language), highlight(&self.to, self.language));
        let ease = |x: f32| x * x * (3.0 - 2.0 * x);
        // Phases: deletions fade 0..0.4, moves 0.2..0.8, insertions 0.6..1.
        let ph = |lo: f32, hi: f32| ease(((self.t - lo) / (hi - lo)).clamp(0.0, 1.0));
        let (fade_out, mv, fade_in) = (ph(0.0, 0.4), ph(0.2, 0.8), ph(0.6, 1.0));
        let ops = diff_lines(&a, &b);
        // A deleted line collapses: it sits where it was, and the rows below
        // close up as `mv` advances. Kept lines interpolate old → new row.
        let mut rows = Vec::new();
        let mut last_new = -1.0f32;
        for op in ops {
            match op {
                LineOp::Keep { old, new } => {
                    #[allow(clippy::cast_precision_loss)]
                    let y = old as f32 + (new as f32 - old as f32) * mv;
                    rows.push(Row { tokens: hb.get(new).cloned().unwrap_or_default(), y, number: Some(new + 1), alpha: 1.0 });
                    #[allow(clippy::cast_precision_loss)]
                    {
                        last_new = new as f32;
                    }
                }
                LineOp::Delete { old } => {
                    #[allow(clippy::cast_precision_loss)]
                    let y = old as f32 + (last_new + 0.5 - old as f32) * mv;
                    rows.push(Row { tokens: ha.get(old).cloned().unwrap_or_default(), y, number: None, alpha: 1.0 - fade_out });
                }
                LineOp::Insert { new } => {
                    #[allow(clippy::cast_precision_loss)]
                    let y = new as f32;
                    rows.push(Row { tokens: hb.get(new).cloned().unwrap_or_default(), y, number: Some(new + 1), alpha: fade_in });
                    #[allow(clippy::cast_precision_loss)]
                    {
                        last_new = new as f32;
                    }
                }
            }
        }
        rows
    }
}

impl Widget for CodeMorph {
    fn debug_name(&self) -> &'static str {
        "CodeMorph"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn key(&self) -> Option<&Key> {
        self.key.as_ref()
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let a: Vec<&str> = self.from.lines().collect();
        let b: Vec<&str> = self.to.lines().collect();
        let both: Vec<&str> = a.iter().chain(b.iter()).copied().collect();
        let look = self.look;
        let rows = self.rows();
        let width = look.width_for(&both);
        render(&rows, &look, a.len().max(b.len()), width, None)
    }
}

widget_node_from!(CodeMorph);

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str, lang: Language) -> Vec<(TokenKind, String)> {
        highlight(src, lang).into_iter().flatten().filter(|t| !t.text.trim().is_empty()).map(|t| (t.kind, t.text)).collect()
    }

    #[test]
    fn rust_tokens() {
        let k = kinds("pub fn main() { let x: Vec<u8> = vec![1, 2]; // hi\n}", Language::Rust);
        assert!(k.contains(&(TokenKind::Keyword, "pub".into())));
        assert!(k.contains(&(TokenKind::Function, "main".into())));
        assert!(k.contains(&(TokenKind::Type, "Vec".into())));
        assert!(k.contains(&(TokenKind::Function, "vec!".into())));
        assert!(k.contains(&(TokenKind::Number, "1".into())));
        assert!(k.contains(&(TokenKind::Comment, "// hi".into())));
        assert_eq!(highlight("a\nb\nc", Language::Rust).len(), 3);
    }

    #[test]
    fn strings_escape_and_block_comments_span_lines() {
        let k = kinds(r#"const s = "a \" b"; /* x
y */ f()"#, Language::JavaScript);
        assert!(k.contains(&(TokenKind::String, r#""a \" b""#.into())));
        let lines = highlight("/* x\ny */ f()", Language::JavaScript);
        assert_eq!(lines[1][0], Token { kind: TokenKind::Comment, text: "y */".into() });
    }

    #[test]
    fn json_keys_and_python_comments() {
        let k = kinds(r#"{"a": 1, "b": "c", "d": true}"#, Language::Json);
        assert_eq!(k[1], (TokenKind::Property, "\"a\"".into()));
        assert!(k.contains(&(TokenKind::String, "\"c\"".into())));
        assert!(k.contains(&(TokenKind::Keyword, "true".into())));
        let p = kinds("def f(x):  # doc\n    return None", Language::Python);
        assert!(p.contains(&(TokenKind::Comment, "# doc".into())));
        assert!(p.contains(&(TokenKind::Keyword, "None".into())));
    }

    #[test]
    fn line_diff_is_minimal() {
        let ops = diff_lines(&["a", "b", "c", "d"], &["a", "c", "x", "d"]);
        assert_eq!(
            ops,
            vec![
                LineOp::Keep { old: 0, new: 0 },
                LineOp::Delete { old: 1 },
                LineOp::Keep { old: 2, new: 1 },
                LineOp::Insert { new: 2 },
                LineOp::Keep { old: 3, new: 3 },
            ]
        );
    }

    #[test]
    fn morph_endpoints_match_the_sources() {
        let m0 = CodeMorph::new("a\nb\nc", "a\nc\nd", 0.0).layout();
        let visible0: Vec<_> = m0.iter().filter(|r| r.2 > 0.99).map(|r| (r.0.clone(), r.1)).collect();
        assert_eq!(visible0, vec![("a".into(), 0.0), ("b".into(), 1.0), ("c".into(), 2.0)]);
        let m1 = CodeMorph::new("a\nb\nc", "a\nc\nd", 1.0).layout();
        let visible1: Vec<_> = m1.iter().filter(|r| r.2 > 0.99).map(|r| (r.0.clone(), r.1)).collect();
        assert_eq!(visible1, vec![("a".into(), 0.0), ("c".into(), 1.0), ("d".into(), 2.0)]);
        assert!(m1.iter().any(|r| r.0 == "b" && r.2 < 0.01), "deleted line faded out");
    }
}
