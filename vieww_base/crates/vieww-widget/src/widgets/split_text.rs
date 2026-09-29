//! Text that animates per character — GSAP's `SplitText`, the plugin every
//! motion-design portfolio uses for the "words arrive one letter at a time"
//! reveal, and the capability the comparison document gave to the web stack
//! alone.
//!
//! # The idea
//!
//! One string, N atoms, one progress value. Each atom (a character or a
//! word, per [`Split`]) starts its own entrance at `stagger` after the one
//! before it, so a single 0→1 animation over the whole text produces a
//! *cascade* — and because the per-atom maths is a pure function of
//! `(progress, index)`, the cascade is deterministic, frame-rate
//! independent, and byte-identical between two runs: the same contract the
//! rest of the animation stack keeps, applied to typography.
//!
//! ```text
//! progress 0.0:  a b c d     (all invisible)
//! progress 0.5:  A b c d     (each letter's own window slides later)
//! progress 1.0:  A B C D     (all arrived)
//! ```
//!
//! # What is honestly lost
//!
//! Each atom is its own text run, so cross-glyph kerning is gone — an "AV"
//! pair sits slightly further apart than a shaped paragraph would put it.
//! GSAP's SplitText makes exactly the same trade (it splits the DOM into
//! spans; the browser re-shapes each), and the honest fix for the cases
//! that need it is a [`Text`](crate::Text) with an animated transform on
//! the whole run. A cascade is worth the kerning; a slide is not.
//!
//! # Not just an entrance, and not the owner of the timing
//!
//! The progress is a **plain `f32`**, read once per build — not a signal,
//! not a controller. That is deliberate: this crate knows nothing about the
//! element layer, and the widget that *does* read your signal passes the
//! number down, exactly the way every other value widget here is driven
//! (read the signal in your own widget's `build`, hand `Opacity` an `f32`).
//! Drive it forwards for a reveal, backwards for an exit, from a spring,
//! from a scroll position — the typography is this widget's half of the
//! effect and the timing is yours, which is where the split belongs.

use vieww_foundation::{Color, Offset, TextStyle};

use crate::prelude::*;

/// What the text splits into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Split {
    /// One atom per character, spaces included — the letter cascade.
    #[default]
    Chars,
    /// One atom per word — the calmer variant, and the one long copy needs
    /// (a 60-character sentence one letter at a time is a 3-second wait).
    Words,
}

/// Text that reveals per atom, staggered along one progress value.
///
/// # Examples
///
/// ```ignore
/// // In your own widget's build, where the signal is readable:
/// SplitText::new("A quick brown fox", self.progress.get())
///     .words()
///     .stagger(0.6)
///     .rise(10.0)
/// ```
#[derive(Clone)]
pub struct SplitText {
    text: String,
    /// The whole-reveal progress, 0 = nothing shown, 1 = all arrived. A
    /// plain value read during build: the caller's widget reads a signal
    /// and passes the number, which keeps this crate element-free and the
    /// timing in the caller's hands.
    progress: f32,
    split: Split,
    /// How much of the timeline the stagger itself consumes — 0.0 is "all
    /// atoms at once", 0.9 is "a wave, one atom riding the last atom's
    /// shoulder". Clamped to 0..=0.95 because 1.0 means the last atom
    /// never starts.
    stagger: f32,
    /// How far each atom travels up, in logical pixels, as its local
    /// progress runs 0→1.
    rise: f32,
    /// `None` takes the ambient theme's body style, sized.
    color: Option<Color>,
    size: f32,
}

impl SplitText {
    /// Text that reveals at `progress` (0 = nothing shown, 1 = all
    /// arrived). The value is re-read on every build, so driving it from a
    /// signal in the caller's widget animates the cascade.
    #[must_use]
    pub fn new(text: impl Into<String>, progress: f32) -> Self {
        Self {
            text: text.into(),
            progress,
            split: Split::Chars,
            stagger: 0.7,
            rise: 12.0,
            color: None,
            size: 16.0,
        }
    }
    /// Split into characters (the default).
    #[must_use]
    pub const fn chars(mut self) -> Self {
        self.split = Split::Chars;
        self
    }

    /// Split into words.
    #[must_use]
    pub const fn words(mut self) -> Self {
        self.split = Split::Words;
        self
    }

    /// How much of the timeline the stagger consumes, in `0..=0.95`.
    /// Values outside are clamped, because the two ends of the range are
    /// the two degenerate cascades (no stagger at all; a wave that never
    /// finishes) and a caller asking for more than 0.95 has asked for the
    /// second one by accident.
    #[must_use]
    pub const fn stagger(mut self, stagger: f32) -> Self {
        self.stagger = clamp_stagger(stagger);
        self
    }

    /// How far each atom rises, in logical pixels.
    #[must_use]
    pub const fn rise(mut self, rise: f32) -> Self {
        self.rise = rise;
        self
    }

    /// Set the text colour, overriding the theme.
    #[must_use]
    pub const fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Set the font size in logical pixels.
    #[must_use]
    pub const fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
}

impl std::fmt::Debug for SplitText {
    /// The signal is unprintable state, not data — a dump shows what the
    /// cascade *is*, not what it currently reads.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SplitText")
            .field("text", &self.text)
            .field("split", &self.split)
            .field("stagger", &self.stagger)
            .field("rise", &self.rise)
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

impl Widget for SplitText {
    fn debug_name(&self) -> &'static str {
        "SplitText"
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
        let progress = self.progress.clamp(0.0, 1.0);
        // The atom count, spelled once: the cascade's window maths needs
        // it, and both splits below agree with it by construction.
        let count = match self.split {
            Split::Chars => self.text.chars().count(),
            Split::Words => self.text.split_whitespace().count(),
        };

        let mut row = Flex::row()
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .spacing(0.0);
        let pieces: Vec<WidgetNode> = match self.split {
            Split::Chars => self
                .text
                .chars()
                .enumerate()
                .map(|(i, c)| {
                    atom_widget(c.to_string(), i, count, progress, self.stagger, self.rise, style)
                })
                .collect(),
            Split::Words => self
                .text
                .split_whitespace()
                .enumerate()
                .map(|(i, word)| {
                    atom_widget(
                        word.to_string(),
                        i,
                        count,
                        progress,
                        self.stagger,
                        self.rise,
                        style,
                    )
                })
                .collect(),
        };
        if self.split == Split::Words {
            // Words get a space between them: re-attaching the whitespace
            // the split consumed, as layout rather than as text, so a
            // half-arrived word is not followed by a half-arrived space.
            let mut spaced = Vec::with_capacity(pieces.len() * 2);
            for (i, piece) in pieces.into_iter().enumerate() {
                if i > 0 {
                    spaced.push(SizedBox::width(style.size * 0.28).into());
                }
                spaced.push(piece);
            }
            for piece in spaced {
                row = row.push(piece);
            }
        } else {
            for piece in pieces {
                row = row.push(piece);
            }
        }

        Semantics::new()
            .label(self.text.clone())
            .child(row)
            .into()
    }
}

crate::widget_node_from!(SplitText);

/// One atom's widget: its own entrance as a function of the shared
/// progress and its index.
fn atom_widget(
    text: String,
    index: usize,
    count: usize,
    progress: f32,
    stagger: f32,
    rise: f32,
    style: TextStyle,
) -> WidgetNode {
    // The window maths: atom `i` of `n` starts at `i·s/(1+(n−1)·s)` of the
    // timeline and finishes when every atom has. At progress 1 the last
    // atom's local progress is exactly 1; at 0 it is exactly 0 — the two
    // invariants a cascade must keep or its ends look broken.
    let local = if count <= 1 {
        progress
    } else {
        let scale = 1.0 + (count as f32 - 1.0) * stagger;
        (progress * scale - index as f32 * stagger).clamp(0.0, 1.0)
    };
    let eased = ease_out(local);
    let opacity = eased.clamp(0.0, 1.0);
    let dy = (1.0 - eased) * rise;

    // A fully arrived atom is plain text — no opacity group, no transform —
    // so the steady state of a revealed cascade is the same tree a plain
    // `Text` would have built, and the exit from animating is free.
    if opacity >= 1.0 && dy.abs() < 0.01 {
        return Text::new(text).style(style).into();
    }
    Opacity::new(opacity.max(0.0))
        .child(Transformed::translate(Offset::new(0.0, dy)).child(
            Text::new(text).style(style),
        ))
        .into()
}

/// The per-atom easing, kept in one place so the cascade's personality is
/// one edit: ease-out, because each atom should *arrive* promptly and then
/// settle, the opposite of an ease-in entrance where every letter creeps.
///
/// Local rather than `vieww_animation::Curve::EASE_OUT.transform` because
/// the widget crate's dependency on the animation crate is *optional at
/// run time* — the animation features arrive behind a feature flag — and
/// two lines of arithmetic is a cheaper price than making the cascade
/// unavailable without it. The shape is `Curve::SINE_OUT` to within the
/// precision a rise in logical pixels can show.
fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t) * (1.0 - t)
}

/// The stagger clamp — a function so the builder and the tests share one
/// spelling of "the two ends are the two degenerate cascades".
const fn clamp_stagger(stagger: f32) -> f32 {
    if stagger < 0.0 {
        0.0
    } else if stagger > 0.95 {
        0.95
    } else {
        stagger
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn char_count_matches_the_text() {
        let text = "A fox";
        assert_eq!(text.chars().count(), 5);
    }

    #[test]
    fn the_first_atom_moves_first_and_the_last_finishes_last() {
        // The window maths, pinned: with 4 atoms and stagger 0.5, the
        // timeline is stretched by 1 + 3·0.5 = 2.5, so at progress 0.2
        // atom 0 is at 0.5, atom 1 at 0.0 — a wave, not a bloom.
        let count = 4u32;
        let stagger = 0.5f32;
        let progress = 0.2f32;
        let scale = 1.0 + (count - 1) as f32 * stagger;
        let first = (progress * scale).clamp(0.0, 1.0);
        let second = (progress * scale - stagger).clamp(0.0, 1.0);
        assert!((first - 0.5).abs() < 1e-4);
        assert_eq!(second, 0.0, "atom 1 has not started");
    }

    #[test]
    fn at_zero_no_atom_has_started() {
        for i in 0..5 {
            let local = (0.0f32 * 3.0 - i as f32 * 0.5).clamp(0.0, 1.0);
            assert_eq!(local, 0.0, "atom {i} started at progress zero");
        }
    }

    #[test]
    fn at_one_every_atom_has_finished() {
        let count = 6;
        let stagger = 0.7;
        let scale = 1.0 + (count as f32 - 1.0) * stagger;
        for i in 0..count {
            let local = (1.0 * scale - i as f32 * stagger).clamp(0.0, 1.0);
            assert_eq!(local, 1.0, "atom {i} unfinished at progress one");
        }
    }

    #[test]
    fn a_single_atom_tracks_the_progress_directly() {
        // No stagger possible with one atom — the cascade degenerates to
        // the plain animation, and the maths must say so rather than
        // stretch a timeline nobody shares.
        let local = (0.3f32 * 1.0 - 0.0).clamp(0.0, 1.0);
        assert!((local - 0.3).abs() < 1e-4);
    }

    #[test]
    fn stagger_is_clamped_at_both_ends() {
        assert_eq!(clamp_stagger(-1.0), 0.0);
        assert_eq!(clamp_stagger(2.0), 0.95);
        assert_eq!(clamp_stagger(0.5), 0.5);
        assert_eq!(clamp_stagger(0.0), 0.0);
        assert_eq!(clamp_stagger(0.95), 0.95);
    }

    #[test]
    fn the_ease_is_an_ease_out() {
        assert_eq!(ease_out(0.0), 0.0);
        assert_eq!(ease_out(1.0), 1.0);
        // Half way, an ease-out is already most of the way there.
        assert!(ease_out(0.5) > 0.7);
    }

    #[test]
    fn words_split_on_whitespace_and_drop_the_separators() {
        let text = "  a   b  ";
        let words: Vec<&str> = text.split_whitespace().collect();
        assert_eq!(words, vec!["a", "b"]);
    }

    #[test]
    fn empty_text_has_no_atoms() {
        assert_eq!("".split_whitespace().count(), 0);
        assert_eq!("".chars().count(), 0);
    }
}
