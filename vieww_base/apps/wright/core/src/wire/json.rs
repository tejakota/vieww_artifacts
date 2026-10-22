//! A JSON value model, parser, and serializer. Ours.
//!
//! Scope: what LLM provider APIs need — objects with stable order,
//! arrays, strings with escapes (including `\uXXXX` and surrogate
//! pairs), integers and floats, booleans, null. Not included: exact
//! float formatting edge cases beyond provider needs, streaming
//! incremental parse (the SSE layer handles chunking).
//!
//! Object keys preserve insertion order (providers' canonical forms
//! and our request bodies depend on it).
//!
//! Hostile input: the parser is recursive, so nesting is capped at
//! [`MAX_DEPTH`]. Without the cap, 200,000 `[` characters from a model
//! or a server overflowed the stack and aborted the whole process — the
//! UI included, since it shares the process. (Found in review; the
//! regression test is `deep_nesting_is_an_error_not_a_crash`.)

// ── value ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Value>),
    /// Insertion-ordered object.
    Obj(Vec<(String, Value)>),
}

impl Value {
    /// Get by key. Objects match by name; arrays match by numeric
    /// index as a string ("0", "1", ...) — enough for light pathing.
    /// Set a key in an Obj (append or replace); a no-op on other kinds.
    pub fn set(&mut self, key: &str, value: Value) {
        if let Value::Obj(fields) = self {
            for f in fields.iter_mut() {
                if f.0 == key {
                    f.1 = value;
                    return;
                }
            }
            fields.push((key.to_string(), value));
        }
    }

    /// The key's value, mutable, if this is an Obj holding it.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        if let Value::Obj(fields) = self {
            fields.iter_mut().find(|f| f.0 == key).map(|f| &mut f.1)
        } else {
            None
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            Value::Arr(items) => key.parse::<usize>().ok().and_then(|i| items.get(i)),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Num(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::Num(n) if *n >= 0.0 => Some(*n as u64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_arr(&self) -> Option<&[Value]> {
        match self {
            Value::Arr(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_obj(&self) -> Option<&[(String, Value)]> {
        match self {
            Value::Obj(p) => Some(p),
            _ => None,
        }
    }

    /// String field of an object, or None.
    pub fn str_of(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(|v| v.as_str())
    }

    /// Build an object from (key, value) pairs.
    pub fn obj<K: Into<String>>(pairs: Vec<(K, Value)>) -> Value {
        Value::Obj(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    /// Build a string value.
    pub fn str(s: impl Into<String>) -> Value {
        Value::Str(s.into())
    }

    /// Build a number value from an integer.
    pub fn int(n: i64) -> Value {
        Value::Num(n as f64)
    }

    /// Convenience: `value.get("a").and_then(|v| v.get("b"))`.
    pub fn path(&self, key: &str, rest: &[&str]) -> Option<&Value> {
        let mut cur = self.get(key)?;
        for k in rest {
            cur = cur.get(k)?;
        }
        Some(cur)
    }
}

// ── parse ────────────────────────────────────────────────────────

/// Parse a JSON document. Errors are human strings, not typed enums;
/// they go into logs and provider error messages.
pub fn parse(src: &str) -> Result<Value, String> {
    let mut p = Parser {
        b: src.as_bytes(),
        i: 0,
        depth: 0,
    };
    p.skip_ws();
    let v = p.value()?;
    p.skip_ws();
    if p.i != p.b.len() {
        return Err(format!("trailing data at byte {}", p.i));
    }
    Ok(v)
}

/// Maximum nesting of arrays/objects. Real provider payloads nest a
/// handful of levels; 128 is generous and keeps recursion shallow.
pub const MAX_DEPTH: usize = 128;

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn skip_ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn expect(&mut self, c: u8) -> Result<(), String> {
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            Err(format!("expected '{}' at byte {}", c as char, self.i))
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        match self.peek() {
            Some(b'{') | Some(b'[') => {
                self.depth += 1;
                if self.depth > MAX_DEPTH {
                    return Err(format!(
                        "JSON nested deeper than {MAX_DEPTH} levels at byte {}",
                        self.i
                    ));
                }
                let v = if self.peek() == Some(b'{') {
                    self.object()
                } else {
                    self.array()
                };
                self.depth -= 1;
                v
            }
            Some(b'"') => Ok(Value::Str(self.string()?)),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'n') => self.literal("null", Value::Null),
            Some(c) if c == b'-' || c.is_ascii_digit() => self.number(),
            Some(c) => Err(format!("unexpected '{}' at byte {}", c as char, self.i)),
            None => Err("unexpected end of input".into()),
        }
    }

    fn literal(&mut self, word: &str, v: Value) -> Result<Value, String> {
        if self.b[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            Ok(v)
        } else {
            Err(format!("invalid literal at byte {}", self.i))
        }
    }

    fn object(&mut self) -> Result<Value, String> {
        self.expect(b'{')?;
        let mut pairs = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(Value::Obj(pairs));
        }
        loop {
            self.skip_ws();
            let key = self.string()?;
            self.skip_ws();
            self.expect(b':')?;
            self.skip_ws();
            let val = self.value()?;
            pairs.push((key, val));
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.i += 1;
                }
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Value::Obj(pairs));
                }
                _ => return Err(format!("expected ',' or '}}' at byte {}", self.i)),
            }
        }
    }

    fn array(&mut self) -> Result<Value, String> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(Value::Arr(items));
        }
        loop {
            self.skip_ws();
            items.push(self.value()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.i += 1;
                }
                Some(b']') => {
                    self.i += 1;
                    return Ok(Value::Arr(items));
                }
                _ => return Err(format!("expected ',' or ']' at byte {}", self.i)),
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut s = String::new();
        loop {
            let c = self.peek().ok_or("unterminated string")?;
            match c {
                b'"' => {
                    self.i += 1;
                    return Ok(s);
                }
                b'\\' => {
                    self.i += 1;
                    let esc = self.peek().ok_or("unterminated escape")?;
                    match esc {
                        b'"' => s.push('"'),
                        b'\\' => s.push('\\'),
                        b'/' => s.push('/'),
                        b'b' => s.push('\u{0008}'),
                        b'f' => s.push('\u{000c}'),
                        b'n' => s.push('\n'),
                        b'r' => s.push('\r'),
                        b't' => s.push('\t'),
                        b'u' => {
                            self.i += 1;
                            let cp = self.hex4()?;
                            let ch = if (0xD800..0xDC00).contains(&cp) {
                                // high surrogate: expect a low surrogate next
                                if self.b[self.i..].starts_with(b"\\u") {
                                    self.i += 2;
                                    let low = self.hex4()?;
                                    if !(0xDC00..0xE000).contains(&low) {
                                        return Err("invalid low surrogate".into());
                                    }
                                    let combined = 0x10000 + ((cp - 0xD800) << 10) + (low - 0xDC00);
                                    char::from_u32(combined).ok_or("invalid surrogate pair")?
                                } else {
                                    '\u{FFFD}' // lone surrogate: replace, be lenient
                                }
                            } else {
                                char::from_u32(cp).unwrap_or('\u{FFFD}')
                            };
                            s.push(ch);
                            continue; // hex4 already advanced past its digits
                        }
                        other => return Err(format!("unknown escape '\\{}'", other as char)),
                    }
                    self.i += 1;
                }
                _ => {
                    // Consume one UTF-8 code point, byte-for-byte.
                    let len = utf8_len(c);
                    let end = (self.i + len).min(self.b.len());
                    s.push_str(&String::from_utf8_lossy(&self.b[self.i..end]));
                    self.i = end;
                }
            }
        }
    }

    /// Read 4 hex digits (already positioned after `\u`).
    fn hex4(&mut self) -> Result<u32, String> {
        if self.i + 4 > self.b.len() {
            return Err("truncated \\u escape".into());
        }
        let text = std::str::from_utf8(&self.b[self.i..self.i + 4])
            .map_err(|_| "bad \\u escape".to_string())?;
        let v = u32::from_str_radix(text, 16).map_err(|_| "bad \\u escape".to_string())?;
        self.i += 4;
        Ok(v)
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        while self.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            self.i += 1;
        }
        if self.peek() == Some(b'.') {
            self.i += 1;
            while self.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                self.i += 1;
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            self.i += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.i += 1;
            }
            while self.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                self.i += 1;
            }
        }
        let text =
            std::str::from_utf8(&self.b[start..self.i]).map_err(|_| "bad number".to_string())?;
        text.parse::<f64>()
            .map(Value::Num)
            .map_err(|_| format!("invalid number '{text}'"))
    }
}

fn utf8_len(first: u8) -> usize {
    if first < 0x80 {
        1
    } else if first >> 5 == 0b110 {
        2
    } else if first >> 4 == 0b1110 {
        3
    } else if first >> 3 == 0b11110 {
        4
    } else {
        1
    }
}

// ── serialize ────────────────────────────────────────────────────

/// Compact serialization. Integers print without a decimal point.
pub fn to_string(v: &Value) -> String {
    let mut s = String::new();
    write_value(v, &mut s);
    s
}

fn write_value(v: &Value, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Num(n) if !n.is_finite() => out.push_str("null"),
        Value::Num(n) => {
            if n.fract() == 0.0 && n.abs() < 1e15 {
                out.push_str(&format!("{}", *n as i64));
            } else {
                out.push_str(&format!("{n}"));
            }
        }
        Value::Str(s) => write_string(s, out),
        Value::Arr(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(item, out);
            }
            out.push(']');
        }
        Value::Obj(pairs) => {
            out.push('{');
            for (i, (k, val)) in pairs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(k, out);
                out.push(':');
                write_value(val, out);
            }
            out.push('}');
        }
    }
}

fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let src = r#"{"a":[1,2.5,"x"],"b":{"c":true,"d":null},"e":"he said \"hi\"\n"}"#;
        let v = parse(src).unwrap();
        assert_eq!(
            v.get("b")
                .and_then(|b| b.get("c"))
                .and_then(|c| c.as_bool()),
            Some(true)
        );
        let out = to_string(&v);
        let v2 = parse(&out).unwrap();
        assert_eq!(v, v2);
    }

    #[test]
    fn unicode_escapes_and_surrogates() {
        let v = parse(r#""\u0041""#).unwrap();
        assert_eq!(v.as_str(), Some("A"));
        // U+1F600 as a surrogate pair \ud83d\ude00.
        let v = parse(r#""\ud83d\ude00""#).unwrap();
        assert_eq!(v.as_str(), Some("\u{1F600}"));
    }

    #[test]
    fn integers_print_without_decimals() {
        let v = Value::Obj(vec![("n".into(), Value::Num(42.0))]);
        assert_eq!(to_string(&v), r#"{"n":42}"#);
    }

    #[test]
    fn control_characters_are_escaped_on_output() {
        let v = Value::Str("a\u{0001}b".into());
        assert_eq!(to_string(&v), r#""a\u0001b""#);
    }

    #[test]
    fn trailing_data_is_an_error() {
        assert!(parse("{} extra").is_err());
    }

    #[test]
    fn object_order_is_preserved() {
        let v = parse(r#"{"z":1,"a":2}"#).unwrap();
        let keys: Vec<&str> = match &v {
            Value::Obj(pairs) => pairs.iter().map(|(k, _)| k.as_str()).collect(),
            _ => panic!("not an object"),
        };
        assert_eq!(keys, vec!["z", "a"]);
    }

    #[test]
    fn numeric_keys_index_arrays() {
        let v = parse(r#"{"choices":[{"message":{"content":"hi"}}]}"#).unwrap();
        let text = v
            .path("choices", &["0", "message", "content"])
            .and_then(|t| t.as_str());
        assert_eq!(text, Some("hi"));
    }

    #[test]
    fn deep_nesting_is_an_error_not_a_crash() {
        let deep = "[".repeat(200_000);
        let err = parse(&deep).unwrap_err();
        assert!(err.contains("nested deeper"), "{err}");
        let ok = format!("{}{}", "[".repeat(100), "]".repeat(100));
        assert!(parse(&ok).is_ok());
    }

    #[test]
    fn non_finite_numbers_serialize_as_null() {
        assert_eq!(to_string(&Value::Num(f64::NAN)), "null");
    }
}
