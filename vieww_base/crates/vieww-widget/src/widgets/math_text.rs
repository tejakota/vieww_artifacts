//! Mathematical notation as a first-class drawable — the Manim
//! `Tex`/`MathTex` capability (and the reason a UI framework's text stack
//! stops at prose), for the formula a settings screen wants to explain, the
//! equation a chart wants to label itself with, the unit a physics readout
//! carries.
//!
//! # What this is, honestly
//!
//! A **TeX subset**, not TeX. The commands it parses, it renders properly:
//!
//! | syntax | renders |
//! |---|---|
//! | `x^2`, `x^{n+1}` | superscript |
//! | `a_i`, `a_{i-1}` | subscript |
//! | `\frac{a}{b}` | a stacked fraction with a rule |
//! | `\sqrt{x}`, `\sqrt[3]{x}` | a radical with an overline |
//! | `\alpha` … `\omega` | the Greek alphabet |
//! | `\sum \int \pi \times \cdot \le \ge \ne \approx \infty \pm \to \partial \nabla` | the common operators |
//!
//! What it does not parse, it does not pretend to: matrices, alignment
//! environments, macro definition, font switching — each is a typesetting
//! engine's worth of work, and "an honest subset beats code that looks like
//! it works and doesn't" is this workspace's rule. An unknown command
//! renders as its own text (`\foo` stays visible) rather than vanishing,
//! because notation that silently disappears is worse than notation that
//! looks wrong — a reader can ask about `\foo`; they cannot ask about
//! nothing.
//!
//! # Why composed widgets and not one painted canvas
//!
//! Every glyph here is a real [`Text`] run: real font, real shaping, real
//! theme colour, selected-and-copyable (or whatever the platform does with
//! text runs) — not an approximation painted by a rasteriser that would
//! have to re-implement all of those to look right. The structural
//! decorations — a fraction's rule, a radical's overline — are real layout
//! boxes (`DecoratedBox` stretched by the column), so they are exactly as
//! wide as the thing they decorate, whatever the font, whatever the size.
//!
//! # The layout maths
//!
//! Scripts (super and sub) are 0.6× the base size, raised 0.4× and dropped
//! 0.25× respectively — the ratios Knuth tuned for TeX's `\scriptstyle`,
//! which forty years of reading has not improved on. Fractions are 0.75×
//! per level, recursively, with the rule 1.5 logical px whatever the level:
//! a fraction of fractions shrinks its *text*, never its rule, because a
//! thin rule on a small fraction is a typo and a thick one is a bar.

use vieww_foundation::{Color, TextStyle};

use crate::prelude::*;
use crate::widgets::decorated_box::DecoratedBox;
use vieww_foundation::BoxDecoration;

/// One parsed piece of a formula.
#[derive(Debug, Clone, PartialEq)]
enum Atom {
    /// A literal run of text, symbols already resolved to their glyphs.
    Text(String),
    /// `^{...}` — raised script.
    Sup(Box<Atom>),
    /// `_{...}` — dropped script.
    Sub(Box<Atom>),
    /// `\frac{num}{den}` — stacked, with a rule between.
    Frac(Box<Atom>, Box<Atom>),
    /// `\sqrt[index]{radicand}` — radical sign plus an overline.
    Sqrt {
        index: Option<Box<Atom>>,
        radicand: Box<Atom>,
    },
    /// A horizontal row, from a braced group or the top level.
    Row(Vec<Atom>),
}

/// The symbol table: every command with a single-glyph answer.
///
/// Sorted by name only in the source's head — lookup is a linear scan of a
/// 40-entry table at *parse* time, once per build, which no profile will
/// ever find.
const SYMBOLS: &[(&str, &str)] = &[
    ("alpha", "α"),
    ("beta", "β"),
    ("gamma", "γ"),
    ("delta", "δ"),
    ("epsilon", "ε"),
    ("zeta", "ζ"),
    ("eta", "η"),
    ("theta", "θ"),
    ("iota", "ι"),
    ("kappa", "κ"),
    ("lambda", "λ"),
    ("mu", "μ"),
    ("nu", "ν"),
    ("xi", "ξ"),
    ("pi", "π"),
    ("rho", "ρ"),
    ("sigma", "σ"),
    ("tau", "τ"),
    ("phi", "φ"),
    ("chi", "χ"),
    ("psi", "ψ"),
    ("omega", "ω"),
    ("Gamma", "Γ"),
    ("Delta", "Δ"),
    ("Theta", "Θ"),
    ("Lambda", "Λ"),
    ("Pi", "Π"),
    ("Sigma", "Σ"),
    ("Phi", "Φ"),
    ("Psi", "Ψ"),
    ("Omega", "Ω"),
    ("sum", "∑"),
    ("prod", "∏"),
    ("int", "∫"),
    ("times", "×"),
    ("cdot", "·"),
    ("le", "≤"),
    ("ge", "≥"),
    ("ne", "≠"),
    ("approx", "≈"),
    ("infty", "∞"),
    ("pm", "±"),
    ("to", "→"),
    ("partial", "∂"),
    ("nabla", "∇"),
    ("in", "∈"),
    ("forall", "∀"),
    ("exists", "∃"),
];

/// Mathematical notation, typeset from a TeX subset.
///
/// # Examples
///
/// ```ignore
/// MathText::new("x^2 + y^2 = z^2")
/// MathText::new("\\frac{\\partial u}{\\partial t} = \\nabla^2 u")
/// MathText::new("\\sum_{i=0}^{n} i = \\frac{n(n+1)}{2}")
/// ```
#[derive(Debug, Clone)]
pub struct MathText {
    source: String,
    /// `None` takes the ambient theme's body colour.
    color: Option<Color>,
    /// The base font size; everything else is a ratio of it.
    size: f32,
}

impl MathText {
    /// Notation from a TeX-subset string.
    #[must_use]
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            color: None,
            size: 16.0,
        }
    }

    /// Set the base colour, overriding the theme.
    #[must_use]
    pub const fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Set the base font size in logical pixels.
    #[must_use]
    pub const fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
}

impl Widget for MathText {
    fn debug_name(&self) -> &'static str {
        "MathText"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, ctx: &BuildContext) -> WidgetNode {
        let theme = ThemeData::of(ctx);
        let style = TextStyle {
            size: self.size,
            color: self.color.unwrap_or(theme.colors.on_surface),
            ..theme.text.body
        };
        let atom = parse(&self.source);
        let rendered = render(&atom, &style, theme.colors.on_surface);
        Semantics::new()
            .label(format!("math: {}", plain_text(&atom)))
            .child(rendered)
            .into()
    }
}

crate::widget_node_from!(MathText);

// ---------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------

/// Parse the TeX subset into an [`Atom`] tree.
///
/// Errors are not thrown — an unknown command stays as text, an unclosed
/// brace takes the rest of the string — because a *visible* wrong formula
/// is debuggable and a thrown one is just missing, and notation is
/// documentation, not input to a compiler.
fn parse(source: &str) -> Atom {
    let chars: Vec<char> = source.chars().collect();
    let mut at = 0;
    let row = parse_row(&chars, &mut at, None);
    Atom::Row(row)
}

/// Parse atoms until `stop` (or the end), consuming the stopper.
fn parse_row(chars: &[char], at: &mut usize, stop: Option<char>) -> Vec<Atom> {
    let mut out = Vec::new();
    let mut run = String::new();
    while *at < chars.len() {
        let c = chars[*at];
        if Some(c) == stop {
            *at += 1;
            break;
        }
        match c {
            '\\' => {
                flush_run(&mut run, &mut out);
                *at += 1;
                let name = parse_command_name(chars, at);
                out.push(parse_command(&name, chars, at));
            }
            '{' => {
                flush_run(&mut run, &mut out);
                *at += 1;
                let inner = parse_row(chars, at, Some('}'));
                out.push(Atom::Row(inner));
            }
            '}' => {
                // A stray close brace: not ours to consume (a caller above
                // may be looking for it), so stop here without eating it.
                break;
            }
            '^' => {
                flush_run(&mut run, &mut out);
                *at += 1;
                out.push(Atom::Sup(Box::new(parse_script_target(chars, at))));
            }
            '_' => {
                flush_run(&mut run, &mut out);
                *at += 1;
                out.push(Atom::Sub(Box::new(parse_script_target(chars, at))));
            }
            _ => {
                run.push(c);
                *at += 1;
            }
        }
    }
    flush_run(&mut run, &mut out);
    out
}

/// A `^`/`_` target: a braced group or a single character (so `x^2` and
/// `x^{n+1}` both work, the way TeX spells both).
fn parse_script_target(chars: &[char], at: &mut usize) -> Atom {
    if *at < chars.len() && chars[*at] == '{' {
        *at += 1;
        return Atom::Row(parse_row(chars, at, Some('}')));
    }
    if *at < chars.len() {
        let c = chars[*at];
        *at += 1;
        return Atom::Text(c.to_string());
    }
    Atom::Text(String::new())
}

/// A command name: alphabetic run after the backslash.
fn parse_command_name(chars: &[char], at: &mut usize) -> String {
    let mut name = String::new();
    while *at < chars.len() && chars[*at].is_ascii_alphabetic() {
        name.push(chars[*at]);
        *at += 1;
    }
    name
}

/// One command, resolved. `\frac` and `\sqrt` consume their arguments;
/// symbols resolve to their glyph; anything else stays visible.
fn parse_command(name: &str, chars: &[char], at: &mut usize) -> Atom {
    match name {
        "frac" => {
            let num = parse_braced(chars, at);
            let den = parse_braced(chars, at);
            Atom::Frac(Box::new(num), Box::new(den))
        }
        "sqrt" => {
            // An optional index, in square brackets — TeX's own syntax.
            let index = if *at < chars.len() && chars[*at] == '[' {
                *at += 1;
                let mut text = String::new();
                while *at < chars.len() && chars[*at] != ']' {
                    text.push(chars[*at]);
                    *at += 1;
                }
                if *at < chars.len() {
                    *at += 1;
                }
                Some(Box::new(Atom::Text(text)))
            } else {
                None
            };
            let radicand = parse_braced(chars, at);
            Atom::Sqrt {
                index,
                radicand: Box::new(radicand),
            }
        }
        _ => match SYMBOLS.iter().find(|(symbol, _)| *symbol == name) {
            Some((_, glyph)) => Atom::Text((*glyph).to_string()),
            // Unknown command: kept as literal text, visible, per the
            // module docs. The backslash is re-attached so the reader sees
            // *which* command was unknown.
            None => Atom::Text(format!("\\{name}")),
        },
    }
}

/// A `{...}` argument, consuming the braces. An argument that is not
/// braced (a caller writing `\sqrt x`) takes one character, the way TeX's
/// single-token arguments behave.
fn parse_braced(chars: &[char], at: &mut usize) -> Atom {
    if *at < chars.len() && chars[*at] == '{' {
        *at += 1;
        return Atom::Row(parse_row(chars, at, Some('}')));
    }
    parse_script_target(chars, at)
}

/// Flush the literal-run accumulator into an atom.
fn flush_run(run: &mut String, out: &mut Vec<Atom>) {
    if !run.is_empty() {
        out.push(Atom::Text(std::mem::take(run)));
    }
}

/// The plain-text reading of an atom tree — what a screen reader is told.
fn plain_text(atom: &Atom) -> String {
    match atom {
        Atom::Text(text) => text.clone(),
        Atom::Sup(inner) => format!("^({})", plain_text(inner)),
        Atom::Sub(inner) => format!("_({})", plain_text(inner)),
        Atom::Frac(num, den) => {
            format!("({})/({})", plain_text(num), plain_text(den))
        }
        Atom::Sqrt { index, radicand } => match index {
            Some(index) => format!("root_{}({})", plain_text(index), plain_text(radicand)),
            None => format!("sqrt({})", plain_text(radicand)),
        },
        Atom::Row(atoms) => {
            let joined: String = atoms.iter().map(plain_text).collect();
            joined
        }
    }
}

// ---------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------

/// How much smaller a script is, relative to its base.
const SCRIPT_SCALE: f32 = 0.6;
/// How far a superscript is raised, in base-size units.
const SUPER_RISE: f32 = 0.4;
/// How far a subscript is dropped, in base-size units.
const SUB_DROP: f32 = 0.25;
/// How much smaller each fraction level is.
const FRAC_SCALE: f32 = 0.75;
/// The fraction rule and radical overline thickness, in logical pixels —
/// constant, deliberately: see the module docs.
const RULE: f32 = 1.5;

/// Render an atom tree at `style`.
fn render(atom: &Atom, style: &TextStyle, rule_color: Color) -> WidgetNode {
    match atom {
        Atom::Text(text) => Text::new(text.clone()).style(*style).into(),
        Atom::Row(atoms) => {
            if atoms.is_empty() {
                return SizedBox::width(0.0).into();
            }
            let mut row = Flex::row().cross_axis_alignment(CrossAxisAlignment::Start);
            for child in atoms {
                row = row.push(render(child, style, rule_color));
            }
            row.into()
        }
        Atom::Sup(inner) => {
            let script = script_style(*style, SCRIPT_SCALE);
            // Raised: translated up by the rise, so the row's height is set
            // by the *base* glyphs and the script rides above them.
            Transformed::translate(vieww_foundation::Offset::new(0.0, -style.size * SUPER_RISE))
                .child(render(inner, &script, rule_color))
                .into()
        }
        Atom::Sub(inner) => {
            let script = script_style(*style, SCRIPT_SCALE);
            Transformed::translate(vieww_foundation::Offset::new(0.0, style.size * SUB_DROP))
                .child(render(inner, &script, rule_color))
                .into()
        }
        Atom::Frac(num, den) => {
            let part = script_style(*style, FRAC_SCALE);
            // Stacked with a rule between, all stretched to the widest
            // part — the rule is a DecoratedBox filling the column's width,
            // exactly as wide as the fraction it divides.
            Flex::column()
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .children(children![
                    render(num, &part, rule_color),
                    SizedBox::height(2.0),
                    DecoratedBox::new(BoxDecoration::filled(rule_color))
                        .child(SizedBox::height(RULE)),
                    SizedBox::height(2.0),
                    render(den, &part, rule_color),
                ])
                .into()
        }
        Atom::Sqrt { index, radicand } => {
            // The radical sign at full size (a hair larger, so its height
            // covers the rule), the index in script size dropped to its
            // baseline corner, and the radicand in a column whose first
            // child is the overline.
            let sign_column = Flex::column().cross_axis_alignment(CrossAxisAlignment::Start);
            let sign: WidgetNode = Text::new("√".to_string())
                .style(TextStyle {
                    size: style.size * 1.1,
                    ..*style
                })
                .into();
            let sign_column = match index {
                Some(index) => {
                    let script = script_style(*style, SCRIPT_SCALE);
                    sign_column
                        .push(
                            // The index sits above the sign's baseline
                            // corner: raised, not stacked on top, so a tall
                            // radicand does not push the index out of the
                            // row.
                            Transformed::translate(vieww_foundation::Offset::new(
                                0.0,
                                -style.size * 0.1,
                            ))
                            .child(render(index, &script, rule_color)),
                        )
                        .push(sign)
                }
                None => sign_column.push(sign),
            };

            let part = script_style(*style, FRAC_SCALE);
            let overlined = Flex::column()
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .children(children![
                    DecoratedBox::new(BoxDecoration::filled(rule_color))
                        .child(SizedBox::height(RULE)),
                    SizedBox::height(1.0),
                    render(radicand, &part, rule_color),
                ]);

            Flex::row()
                .cross_axis_alignment(CrossAxisAlignment::End)
                .children(children![sign_column, overlined,])
                .into()
        }
    }
}

/// A script/secondary style derived from a base one.
fn script_style(base: TextStyle, scale: f32) -> TextStyle {
    TextStyle {
        size: base.size * scale,
        ..base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_round_trips_simple_math() {
        let atom = parse("x^2 + y");
        assert_eq!(plain_text(&atom), "x^(2) + y");
    }

    #[test]
    fn subscripts_parse() {
        let atom = parse("a_{i-1}");
        assert_eq!(plain_text(&atom), "a_(i-1)");
        let single = parse("a_i");
        assert_eq!(plain_text(&single), "a_(i)");
    }

    #[test]
    fn fractions_nest() {
        let atom = parse("\\frac{a}{\\frac{b}{c}}");
        assert_eq!(plain_text(&atom), "(a)/((b)/(c))");
    }

    #[test]
    fn radicals_with_and_without_indices() {
        assert_eq!(plain_text(&parse("\\sqrt{x}")), "sqrt(x)");
        assert_eq!(plain_text(&parse("\\sqrt[3]{x+1}")), "root_3(x+1)");
    }

    #[test]
    fn greek_resolves_to_glyphs() {
        assert_eq!(plain_text(&parse("\\alpha + \\beta")), "α + β");
        assert_eq!(plain_text(&parse("\\Gamma")), "Γ");
    }

    #[test]
    fn operators_resolve() {
        for (source, want) in [
            ("\\sum", "∑"),
            ("\\int", "∫"),
            ("\\pi", "π"),
            ("\\times", "×"),
            ("\\le", "≤"),
            ("\\infty", "∞"),
            ("\\nabla", "∇"),
        ] {
            assert_eq!(plain_text(&parse(source)), want, "{source}");
        }
    }

    #[test]
    fn an_unknown_command_stays_visible() {
        // The module's honesty rule: `\foo` renders as text, not as nothing.
        assert_eq!(plain_text(&parse("\\foo + 1")), "\\foo + 1");
    }

    #[test]
    fn braced_groups_are_rows() {
        let atom = parse("x^{n+1}");
        assert_eq!(plain_text(&atom), "x^(n+1)");
    }

    #[test]
    fn the_gauss_sum_parses_end_to_end() {
        // The classic — every feature at once.
        let atom = parse("\\sum_{i=0}^{n} i = \\frac{n(n+1)}{2}");
        assert_eq!(plain_text(&atom), "∑_(i=0)^(n) i = (n(n+1))/(2)");
    }

    #[test]
    fn an_unclosed_brace_eats_the_rest_gracefully() {
        let atom = parse("x^{2");
        // The row is still parsed; only the closing brace is missing.
        assert_eq!(plain_text(&atom), "x^(2)");
    }

    #[test]
    fn empty_source_is_an_empty_row() {
        assert_eq!(plain_text(&parse("")), "");
    }

    #[test]
    fn spaces_survive_into_the_reading() {
        assert_eq!(plain_text(&parse("a + b")), "a + b");
    }

    #[test]
    fn plain_text_is_what_the_semantics_label_reads() {
        // The contract between the renderer and the screen reader: no
        // glyph, no layout, just the words. `x^2` says "x to the two".
        assert_eq!(plain_text(&parse("x^2")), "x^(2)");
    }
}
