//! buffer — the source the session actually writes.
//!
//! One program, in two languages. The film's session starts in `say` (three
//! lines, first paint), grows to the full component, and then *descends* to
//! the Rust the same buffer becomes. Keeping both texts here, beside each
//! other, is what lets S08 transliterate line by line instead of cutting to
//! a different file: the two lists are the same program at two altitudes,
//! and the mapping between them is an index.
//!
//! Nothing here is decoration. The `say` lines are the ones S06 types, the
//! error S07 makes is a real one (a name that is not in scope), and the Rust
//! is what the codegen's shape actually is: a widget function returning a
//! tree, with the signal read during build.

use crate::studio::{lex, Line};

/// The three lines that bring the preview alive — S06's whole content.
pub const FIRST_PAINT: &[&str] = &[
    "view counter {",
    "    text \"0\"",
    "}",
];

/// The component as S07 finishes it: a signal, a dial, a button, a spring.
pub const SAY: &[&str] = &[
    "view counter {",
    "    state taps = 0",
    "",
    "    column {",
    "        dial value: taps, of: 7",
    "        text \"{taps}\" size: 64",
    "        text \"taps\" dim: true",
    "",
    "        button \"count\" {",
    "            on tap { taps = taps + 1 }",
    "        }",
    "    }",
    "}",
];

/// The mistake S07 makes on purpose — `tap` where `taps` was meant. A name
/// that is not in scope is the smallest honest error: it is caught, it is
/// named, and the rest of the tree keeps painting.
pub const BROKEN_LINE: usize = 4;
pub const BROKEN_TEXT: &str = "        dial value: tap, of: 7";
pub const FIXED_TEXT: &str = "        dial value: taps, of: 7";
pub const DIAGNOSTIC: &str = "cannot find `tap` in this scope — did you mean `taps`?";

/// The same program, one altitude down. S08 walks the buffer from the list
/// above to this one, line by line, with the preview never restarting.
pub const RUST: &[&str] = &[
    "pub fn counter() -> impl Widget {",
    "    let taps = runtime().signal(0u32);",
    "",
    "    Column::new()",
    "        .push(Dial::new(taps.get()).of(7))",
    "        .push(Text::new(format!(\"{}\", taps.get())).size(64.0))",
    "        .push(Text::new(\"taps\").dim())",
    "",
    "        .push(Button::new(\"count\").on_tap(move |_| {",
    "            taps.set(taps.get() + 1);",
    "        }))",
    "}",
    "",
];

/// The lexed `say` buffer, with an optional broken line and an optional
/// character limit for the type-on.
#[must_use]
pub fn say_buffer(lines: usize, chars_on_last: Option<usize>, broken: bool) -> Vec<Line> {
    let mut out = Vec::new();
    for (i, raw) in SAY.iter().enumerate() {
        if i >= lines {
            break;
        }
        let text = if broken && i == BROKEN_LINE { BROKEN_TEXT } else { *raw };
        let line = lex(text, false);
        if i + 1 == lines {
            if let Some(n) = chars_on_last {
                out.push(line.truncated(n));
                continue;
            }
        }
        out.push(line);
    }
    out
}

/// The three-line buffer, typed on by total character count across the
/// whole thing — which is how a person types, and how S06 shows it.
#[must_use]
pub fn first_paint_buffer(chars: usize) -> Vec<Line> {
    let mut left = chars;
    let mut out = Vec::new();
    for raw in FIRST_PAINT {
        let n = raw.chars().count();
        if left == 0 {
            break;
        }
        let line = lex(raw, false);
        if n <= left {
            out.push(line);
            // A newline costs a character, which is why the pauses between
            // lines feel right without anyone tuning them.
            left = left.saturating_sub(n + 1);
        } else {
            out.push(line.truncated(left));
            left = 0;
        }
    }
    out
}

/// Total characters in the three-line buffer, newlines included.
#[must_use]
pub fn first_paint_len() -> usize {
    FIRST_PAINT.iter().map(|l| l.chars().count() + 1).sum()
}

/// The descent: the buffer with the first `converted` lines in Rust and the
/// rest still in `say`. The two lists are aligned by index, so the walk is
/// one cursor moving down the file.
#[must_use]
pub fn descent_buffer(converted: usize, mid: Option<(usize, f32)>) -> Vec<Line> {
    let n = SAY.len().max(RUST.len());
    let mut out = Vec::new();
    for i in 0..n {
        let say = SAY.get(i).copied().unwrap_or("");
        let rust = RUST.get(i).copied().unwrap_or("");
        if i < converted {
            out.push(lex(rust, true));
        } else if let Some((line, p)) = mid {
            if i == line {
                // The line being rewritten: the old text shrinking away and
                // the new one arriving, as one truncation each. The eye
                // reads it as a single line changing, which is what it is.
                let src = if p < 0.5 { say } else { rust };
                let frac = if p < 0.5 { 1.0 - p * 2.0 } else { (p - 0.5) * 2.0 };
                let keep = (src.chars().count() as f32 * frac).round() as usize;
                out.push(lex(src, p >= 0.5).truncated(keep));
            } else {
                out.push(lex(say, false));
            }
        } else {
            out.push(lex(say, false));
        }
    }
    out
}
