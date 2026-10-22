//! Typed edges (ADR-027): a hand-off with a contract, checked before use.
//!
//! In a Wright blueprint an edge is a promise: `Fetcher` gives
//! `stories.json` to `Summarizer`. Untyped, that promise can be broken in
//! silence — a JSON object where an array was expected, a PDF where text
//! was promised — and the failure surfaces two agents later, wrapped in
//! a model's apology. A typed edge binds a named output to a named input
//! with a shape, so a broken hand-off fails at PLAN time (the names don't
//! match) or at FIRE time (the bytes don't match the shape), in plain
//! words, before any agent reads them.
//!
//! ```kdl
//! edge from="Fetcher" to="Summarizer" out="stories.json" in="stories.json" schema="json-array"
//! ```
//!
//! Five shapes, no more. They are chosen so a person can READ them in the
//! Confirm Gate and a machine can CHECK them in one pass over the bytes:
//! [`Shape::Any`], [`Shape::Text`], [`Shape::JsonObject`],
//! [`Shape::JsonArray`], [`Shape::Csv`]. One file, one shape; unions and
//! regexes would be the beginning of a type system, and this is not
//! that — it is a contract you can say out loud.

use crate::wire::json::{self, Value};

/// The shape an artifact is promised to have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Shape {
    /// No promise: the edge carries data, no questions asked.
    Any,
    /// Decodes as UTF-8 text, no NUL bytes, at least one character.
    Text,
    /// Parses as a JSON object — the workhorse of agent hand-offs.
    JsonObject,
    /// Parses as a JSON array — "a list of things", the digest shape.
    JsonArray,
    /// Comma-separated values: UTF-8 text with a consistent field count
    /// (at least two fields) across its non-empty lines.
    Csv,
}

impl Shape {
    /// All the shapes, in Confirm-Gate display order.
    pub const ALL: [Shape; 5] = [
        Shape::Any,
        Shape::Text,
        Shape::JsonObject,
        Shape::JsonArray,
        Shape::Csv,
    ];

    pub fn parse(s: &str) -> Option<Shape> {
        match s {
            "any" => Some(Shape::Any),
            "text" => Some(Shape::Text),
            "json-object" => Some(Shape::JsonObject),
            "json-array" => Some(Shape::JsonArray),
            "csv" => Some(Shape::Csv),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Shape::Any => "any",
            Shape::Text => "text",
            Shape::JsonObject => "json-object",
            Shape::JsonArray => "json-array",
            Shape::Csv => "csv",
        }
    }

    /// One line a person reads in the plan: what this shape promises.
    pub fn promise(&self) -> &'static str {
        match self {
            Shape::Any => "any bytes",
            Shape::Text => "human-readable text",
            Shape::JsonObject => "a JSON object",
            Shape::JsonArray => "a JSON array",
            Shape::Csv => "rows of comma-separated fields, the same count in every row",
        }
    }

    /// Do the bytes keep the promise? Pure: no I/O, no panics on
    /// malformed input — an honest false is the answer.
    pub fn check_bytes(&self, bytes: &[u8]) -> Result<(), String> {
        match self {
            Shape::Any => Ok(()),
            Shape::Text => {
                if bytes.is_empty() {
                    return Err("the file is empty".into());
                }
                if bytes.contains(&0) {
                    return Err("it contains NUL bytes, so it is not text".into());
                }
                match std::str::from_utf8(bytes) {
                    Ok(_) => Ok(()),
                    Err(e) => Err(format!("it is not valid UTF-8 ({e})")),
                }
            }
            Shape::JsonObject => {
                let text = std::str::from_utf8(bytes)
                    .map_err(|e| format!("it is not valid UTF-8 ({e})"))?;
                match json::parse(text) {
                    Ok(Value::Obj(_)) => Ok(()),
                    Ok(_) => Err("it is JSON, but not an object".into()),
                    Err(e) => Err(format!("it does not parse as JSON ({e})")),
                }
            }
            Shape::JsonArray => {
                let text = std::str::from_utf8(bytes)
                    .map_err(|e| format!("it is not valid UTF-8 ({e})"))?;
                match json::parse(text) {
                    Ok(Value::Arr(_)) => Ok(()),
                    Ok(_) => Err("it is JSON, but not an array".into()),
                    Err(e) => Err(format!("it does not parse as JSON ({e})")),
                }
            }
            Shape::Csv => {
                let text = std::str::from_utf8(bytes)
                    .map_err(|e| format!("it is not valid UTF-8 ({e})"))?;
                let mut counts: Vec<usize> = Vec::new();
                for line in text.lines() {
                    let t = line.trim();
                    if t.is_empty() {
                        continue;
                    }
                    counts.push(t.matches(',').count() + 1);
                }
                let consistent = counts.iter().all(|c| *c == counts[0]);
                if counts.is_empty() {
                    return Err("the file has no rows".into());
                }
                if !consistent {
                    return Err("the rows have different numbers of fields".into());
                }
                if counts[0] < 2 {
                    return Err("every row has a single field — that is a list, not a table".into());
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_parse_and_print() {
        for s in Shape::ALL {
            assert_eq!(Shape::parse(s.as_str()), Some(s));
            assert!(
                !s.promise().is_empty(),
                "{} needs a readable promise",
                s.as_str()
            );
        }
        assert_eq!(Shape::parse("yaml"), None);
    }

    #[test]
    fn text_shape_is_honest_about_what_it_rejects() {
        assert!(Shape::Text.check_bytes(b"hello, plan").is_ok());
        assert!(Shape::Text.check_bytes(b"").is_err());
        assert!(Shape::Text.check_bytes(b"ok\x00binary").is_err());
        assert!(Shape::Text.check_bytes(&[0xff, 0xfe]).is_err());
    }

    #[test]
    fn json_shapes_check_the_actual_value() {
        assert!(Shape::JsonObject.check_bytes(br#"{"a":1}"#).is_ok());
        assert!(Shape::JsonObject.check_bytes(br#"[1,2]"#).is_err());
        assert!(Shape::JsonArray.check_bytes(br#"[1,2]"#).is_ok());
        assert!(Shape::JsonArray.check_bytes(br#"{"a":1}"#).is_err());
        assert!(Shape::JsonArray.check_bytes(b"not json").is_err());
        // trailing whitespace is fine; agents are not parsers.
        assert!(Shape::JsonObject.check_bytes(b"{\"a\": 1}\n").is_ok());
    }

    #[test]
    fn csv_shape_demands_consistent_rows() {
        assert!(Shape::Csv.check_bytes(b"a,b,c\n1,2,3\n4,5,6\n").is_ok());
        assert!(
            Shape::Csv.check_bytes(b"a,b,c\n1,2\n").is_err(),
            "ragged rows"
        );
        assert!(
            Shape::Csv
                .check_bytes(b"just one column\nanother line\n")
                .is_err(),
            "single-field rows are not a table"
        );
        assert!(
            Shape::Csv.check_bytes(b"a,b\n\n1,2\n").is_ok(),
            "blank lines are ignored"
        );
        assert!(Shape::Csv.check_bytes(b"").is_err());
    }

    #[test]
    fn any_shape_accepts_everything() {
        assert!(Shape::Any.check_bytes(b"").is_ok());
        assert!(Shape::Any.check_bytes(&[0u8; 7]).is_ok());
    }
}
