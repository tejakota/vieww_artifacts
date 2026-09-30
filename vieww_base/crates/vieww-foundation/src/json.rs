//! JSON: a reader and a writer, for the formats other frameworks ship assets in.
//!
//! # Why the foundation grew one
//!
//! This workspace has a zero-serde rule (see `vieww-devtools` and
//! `vieww-test-harness::suite_report`), and until now every JSON it touched
//! was *written* by hand-rolled code or read line-by-line by the studio's own
//! 150-line reader. That held while JSON was a report format. It stopped
//! holding the moment the comparison document's capabilities were taken
//! seriously: **glTF** is JSON (Three.js' `GLTFLoader`, Godot's importer),
//! **Lottie** is JSON (the After Effects export every mobile toolkit plays),
//! a Konva stage serialises to JSON (`stage.toJSON()`), Rive's lesson —
//! "state machines as data" — needs *some* data format, and a collaborative
//! document has to cross a wire as something. Six readers of the same grammar
//! is how six subtly different JSON dialects happen, so there is one, here.
//!
//! # What it is
//!
//! A [`Json`] value tree, [`Json::parse`] (RFC 8259, including `\u` escapes
//! and surrogate pairs, with the byte offset of the first error), and
//! `Json::to_string` (via `Display`) / [`Json::pretty`] writers whose output parses back to
//! an equal value. Objects keep **source order** in a `Vec` — asset formats
//! have a handful of keys per object, a linear scan beats hashing there, and
//! a round-tripped file then diffs cleanly against its original.
//!
//! # What it deliberately is not
//!
//! No derive, no borrowing from the input, no streaming. The largest file any
//! caller here reads is a glTF scene description (the binary buffers travel
//! beside it, not in it), which is kilobytes.
//!
//! ```
//! use vieww_foundation::json::Json;
//!
//! let doc = Json::parse(r#"{"name": "cube", "size": [1, 2.5, -3e2], "on": true}"#).unwrap();
//! assert_eq!(doc.get("name").and_then(Json::as_str), Some("cube"));
//! assert_eq!(doc.get("size").and_then(|s| s.index(2)).and_then(Json::as_f64), Some(-300.0));
//! assert_eq!(Json::parse(&doc.to_string()).unwrap(), doc);
//! ```

use std::fmt::{self, Write as _};

/// A parsed JSON value.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Json {
    #[default]
    Null,
    Bool(bool),
    /// JSON has one number type; it is an `f64` here, as in JavaScript.
    Number(f64),
    String(String),
    Array(Vec<Json>),
    /// Fields in source order. Duplicate keys are kept; [`Json::get`] returns
    /// the **last**, which is what `JSON.parse` does.
    Object(Vec<(String, Json)>),
}

/// Why a document did not parse, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonError {
    /// Byte offset into the input at which the problem was found.
    pub offset: usize,
    /// What was wrong, in words.
    pub message: String,
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JSON error at byte {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for JsonError {}

impl Json {
    /// Parse a complete document. Trailing non-whitespace is an error.
    ///
    /// # Errors
    ///
    /// A [`JsonError`] naming the byte offset of the first problem.
    pub fn parse(text: &str) -> Result<Self, JsonError> {
        let mut parser = Parser {
            bytes: text.as_bytes(),
            at: 0,
            depth: 0,
        };
        parser.skip_ws();
        let value = parser.value()?;
        parser.skip_ws();
        if parser.at != parser.bytes.len() {
            return Err(parser.error("trailing characters after the document"));
        }
        Ok(value)
    }

    /// An object from `(key, value)` pairs.
    #[must_use]
    pub fn object<K: Into<String>>(fields: impl IntoIterator<Item = (K, Json)>) -> Self {
        Self::Object(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    /// An array of numbers — the commonest shape in every asset format here.
    #[must_use]
    pub fn numbers(values: impl IntoIterator<Item = f64>) -> Self {
        Self::Array(values.into_iter().map(Json::Number).collect())
    }

    /// The field `key` of an object; `None` for a missing key or a non-object.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Self::Object(fields) => fields.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Element `index` of an array.
    #[must_use]
    pub fn index(&self, index: usize) -> Option<&Json> {
        match self {
            Self::Array(items) => items.get(index),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        match *self {
            Self::Number(n) => Some(n),
            _ => None,
        }
    }

    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn as_f32(&self) -> Option<f32> {
        self.as_f64().map(|n| n as f32)
    }

    /// A non-negative integer that fits, or `None` — `-1`, `1.5` and `1e300`
    /// are all refused rather than silently becoming some other index.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn as_usize(&self) -> Option<usize> {
        match *self {
            Self::Number(n) if n >= 0.0 && n.fract() == 0.0 && n <= 9.0e15 => Some(n as usize),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match *self {
            Self::Bool(b) => Some(b),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Self::Array(items) => Some(items),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_object(&self) -> Option<&[(String, Json)]> {
        match self {
            Self::Object(fields) => Some(fields),
            _ => None,
        }
    }

    /// An array of numbers as `f32`s; `None` if any element is not a number.
    #[must_use]
    pub fn as_f32_vec(&self) -> Option<Vec<f32>> {
        self.as_array()?.iter().map(Json::as_f32).collect()
    }

    #[must_use]
    pub const fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Indented, two spaces, one field per line — for files a human diffs.
    #[must_use]
    pub fn pretty(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, Some(0));
        out
    }

    fn write(&self, out: &mut String, indent: Option<usize>) {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Self::Number(n) => write_number(out, *n),
            Self::String(s) => write_string(out, s),
            Self::Array(items) => {
                if items.is_empty() {
                    out.push_str("[]");
                    return;
                }
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    newline(out, indent.map(|d| d + 1));
                    item.write(out, indent.map(|d| d + 1));
                }
                newline(out, indent);
                out.push(']');
            }
            Self::Object(fields) => {
                if fields.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push('{');
                for (i, (key, value)) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    newline(out, indent.map(|d| d + 1));
                    write_string(out, key);
                    out.push(':');
                    if indent.is_some() {
                        out.push(' ');
                    }
                    value.write(out, indent.map(|d| d + 1));
                }
                newline(out, indent);
                out.push('}');
            }
        }
    }
}

/// Compact output: no whitespace at all.
impl fmt::Display for Json {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        self.write(&mut out, None);
        f.write_str(&out)
    }
}

impl From<f64> for Json {
    fn from(n: f64) -> Self {
        Self::Number(n)
    }
}
impl From<f32> for Json {
    fn from(n: f32) -> Self {
        Self::Number(f64::from(n))
    }
}
impl From<bool> for Json {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}
impl From<&str> for Json {
    fn from(s: &str) -> Self {
        Self::String(s.to_owned())
    }
}
impl From<String> for Json {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}
impl From<usize> for Json {
    #[allow(clippy::cast_precision_loss)]
    fn from(n: usize) -> Self {
        Self::Number(n as f64)
    }
}
impl From<Vec<Json>> for Json {
    fn from(items: Vec<Json>) -> Self {
        Self::Array(items)
    }
}

fn newline(out: &mut String, indent: Option<usize>) {
    if let Some(depth) = indent {
        out.push('\n');
        for _ in 0..depth {
            out.push_str("  ");
        }
    }
}

fn write_number(out: &mut String, n: f64) {
    if !n.is_finite() {
        // JSON has no NaN or infinity; `null` is what `JSON.stringify` writes.
        out.push_str("null");
    } else if n.fract() == 0.0 && n.abs() < 1e15 {
        let _ = write!(out, "{n:.0}");
    } else {
        // `{}` on f64 is the shortest string that round-trips.
        let _ = write!(out, "{n}");
    }
}

fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Nesting beyond this is refused rather than overflowing the stack.
const MAX_DEPTH: usize = 256;

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
    depth: usize,
}

impl Parser<'_> {
    fn error(&self, message: &str) -> JsonError {
        JsonError {
            offset: self.at,
            message: message.to_owned(),
        }
    }

    fn skip_ws(&mut self) {
        while let Some(&b) = self.bytes.get(self.at) {
            if matches!(b, b' ' | b'\n' | b'\r' | b'\t') {
                self.at += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn expect_word(&mut self, word: &str, value: Json) -> Result<Json, JsonError> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(value)
        } else {
            Err(self.error("unexpected literal"))
        }
    }

    fn value(&mut self) -> Result<Json, JsonError> {
        match self.peek() {
            None => Err(self.error("unexpected end of input")),
            Some(b'n') => self.expect_word("null", Json::Null),
            Some(b't') => self.expect_word("true", Json::Bool(true)),
            Some(b'f') => self.expect_word("false", Json::Bool(false)),
            Some(b'"') => self.string().map(Json::String),
            Some(b'[') => self.array(),
            Some(b'{') => self.object(),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.error("unexpected character")),
        }
    }

    fn enter(&mut self) -> Result<(), JsonError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            Err(self.error("nesting too deep"))
        } else {
            Ok(())
        }
    }

    fn array(&mut self) -> Result<Json, JsonError> {
        self.enter()?;
        self.at += 1;
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.at += 1;
            self.depth -= 1;
            return Ok(Json::Array(items));
        }
        loop {
            self.skip_ws();
            items.push(self.value()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    break;
                }
                _ => return Err(self.error("expected ',' or ']'")),
            }
        }
        self.depth -= 1;
        Ok(Json::Array(items))
    }

    fn object(&mut self) -> Result<Json, JsonError> {
        self.enter()?;
        self.at += 1;
        let mut fields = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.at += 1;
            self.depth -= 1;
            return Ok(Json::Object(fields));
        }
        loop {
            self.skip_ws();
            if self.peek() != Some(b'"') {
                return Err(self.error("expected a string key"));
            }
            let key = self.string()?;
            self.skip_ws();
            if self.peek() != Some(b':') {
                return Err(self.error("expected ':'"));
            }
            self.at += 1;
            self.skip_ws();
            let value = self.value()?;
            fields.push((key, value));
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    break;
                }
                _ => return Err(self.error("expected ',' or '}'")),
            }
        }
        self.depth -= 1;
        Ok(Json::Object(fields))
    }

    fn number(&mut self) -> Result<Json, JsonError> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        let digits = |p: &mut Self| {
            let s = p.at;
            while matches!(p.peek(), Some(b'0'..=b'9')) {
                p.at += 1;
            }
            p.at - s
        };
        if self.peek() == Some(b'0') {
            self.at += 1;
        } else if digits(self) == 0 {
            return Err(self.error("expected a digit"));
        }
        if self.peek() == Some(b'.') {
            self.at += 1;
            if digits(self) == 0 {
                return Err(self.error("expected a digit after '.'"));
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if digits(self) == 0 {
                return Err(self.error("expected an exponent"));
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at])
            .map_err(|_| self.error("invalid number"))?;
        text.parse::<f64>()
            .map(Json::Number)
            .map_err(|_| self.error("invalid number"))
    }

    fn hex4(&mut self) -> Result<u32, JsonError> {
        let slice = self
            .bytes
            .get(self.at..self.at + 4)
            .ok_or_else(|| self.error("truncated \\u escape"))?;
        let text = std::str::from_utf8(slice).map_err(|_| self.error("bad \\u escape"))?;
        let value = u32::from_str_radix(text, 16).map_err(|_| self.error("bad \\u escape"))?;
        self.at += 4;
        Ok(value)
    }

    fn string(&mut self) -> Result<String, JsonError> {
        self.at += 1; // the opening quote
        let mut out = String::new();
        loop {
            let run_start = self.at;
            while let Some(b) = self.peek() {
                if b == b'"' || b == b'\\' || b < 0x20 {
                    break;
                }
                self.at += 1;
            }
            out.push_str(
                std::str::from_utf8(&self.bytes[run_start..self.at])
                    .map_err(|_| self.error("invalid UTF-8"))?,
            );
            match self.peek() {
                None => return Err(self.error("unterminated string")),
                Some(b'"') => {
                    self.at += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.at += 1;
                    let escape = self.peek().ok_or_else(|| self.error("truncated escape"))?;
                    self.at += 1;
                    match escape {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let high = self.hex4()?;
                            let code = if (0xD800..0xDC00).contains(&high) {
                                if !self.bytes[self.at..].starts_with(b"\\u") {
                                    return Err(self.error("lone high surrogate"));
                                }
                                self.at += 2;
                                let low = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&low) {
                                    return Err(self.error("invalid low surrogate"));
                                }
                                0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00)
                            } else {
                                high
                            };
                            out.push(
                                char::from_u32(code)
                                    .ok_or_else(|| self.error("invalid code point"))?,
                            );
                        }
                        _ => return Err(self.error("unknown escape")),
                    }
                }
                Some(_) => return Err(self.error("control character in string")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalars_parse() {
        assert_eq!(Json::parse("null").unwrap(), Json::Null);
        assert_eq!(Json::parse(" true ").unwrap(), Json::Bool(true));
        assert_eq!(Json::parse("-0.5e1").unwrap(), Json::Number(-5.0));
        assert_eq!(Json::parse("\"a\\u00e9\\n\"").unwrap(), Json::String("aé\n".into()));
    }

    #[test]
    fn surrogate_pairs_decode_to_one_char() {
        assert_eq!(
            Json::parse("\"\\ud83d\\ude00\"").unwrap(),
            Json::String("😀".into())
        );
        assert!(Json::parse("\"\\ud83d\"").is_err());
    }

    #[test]
    fn nested_documents_round_trip_compact_and_pretty() {
        let text = r#"{"a":[1,2,{"b":null,"c":"x\"y"}],"d":{},"e":[],"f":1.25}"#;
        let doc = Json::parse(text).unwrap();
        assert_eq!(doc.to_string(), text);
        assert_eq!(Json::parse(&doc.pretty()).unwrap(), doc);
    }

    #[test]
    fn errors_carry_offsets() {
        let e = Json::parse("[1, 2,, 3]").unwrap_err();
        assert_eq!(e.offset, 6);
        assert!(Json::parse("{\"a\" 1}").is_err());
        assert!(Json::parse("01").is_err());
        assert!(Json::parse("[1] x").is_err());
        assert!(Json::parse("\"unterminated").is_err());
    }

    #[test]
    fn depth_is_bounded() {
        let deep = "[".repeat(MAX_DEPTH + 1) + &"]".repeat(MAX_DEPTH + 1);
        assert!(Json::parse(&deep).is_err());
        let ok = "[".repeat(10) + &"]".repeat(10);
        assert!(Json::parse(&ok).is_ok());
    }

    #[test]
    fn accessors_refuse_the_wrong_shape() {
        let doc = Json::parse(r#"{"n": -1, "f": 1.5, "i": 7, "k": "v", "k": "w"}"#).unwrap();
        assert_eq!(doc.get("n").and_then(Json::as_usize), None);
        assert_eq!(doc.get("f").and_then(Json::as_usize), None);
        assert_eq!(doc.get("i").and_then(Json::as_usize), Some(7));
        assert_eq!(doc.get("k").and_then(Json::as_str), Some("w"), "last duplicate wins");
        assert_eq!(doc.get("missing"), None);
        assert_eq!(Json::Null.get("x"), None);
    }

    #[test]
    fn non_finite_numbers_write_null() {
        assert_eq!(Json::Number(f64::NAN).to_string(), "null");
        assert_eq!(Json::Number(3.0).to_string(), "3");
        assert_eq!(Json::Number(0.1).to_string(), "0.1");
    }
}
