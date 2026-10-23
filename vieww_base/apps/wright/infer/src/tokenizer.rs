//! The Qwen tokenizer: byte-level BPE, read from the model file.
//!
//! ```text
//!  text -> special tokens split out   (<|im_start|>, <tool_call>, ...)
//!       -> pre-tokenize (Qwen's regex: words, single digits, punctuation, spaces)
//!       -> each piece: bytes -> printable characters (GPT-2's byte map)
//!       -> BPE: merge the lowest-ranked adjacent pair until none applies
//!       -> token ids
//! ```
//!
//! Matches llama.cpp's `qwen2` tokenizer token for token (tested against
//! its published test cases in `tests/tokenizer.rs`). User-defined tokens
//! such as `<think>` are always split out; control tokens such as
//! `<|im_end|>` only when the caller asks — text a person typed can never
//! smuggle in a control token.

use std::collections::HashMap;

use crate::gguf::{Gguf, Meta};
use crate::unicode::{is_letter, is_number};

const TYPE_CONTROL: i64 = 3;
const TYPE_USER_DEFINED: i64 = 4;

pub struct Tokenizer {
    tokens: Vec<String>,
    ids: HashMap<String, u32>,
    /// (left, right) -> rank (lower merges first).
    ranks: HashMap<(String, String), u32>,
    /// Special strings, longest first, with whether they are control tokens.
    specials: Vec<(String, u32, bool)>,
    byte_to_char: [char; 256],
    char_to_byte: HashMap<char, u8>,
    pub eos: u32,
}

/// GPT-2's map from bytes to printable characters (so every byte has a
/// visible stand-in in the vocabulary).
fn byte_map() -> [char; 256] {
    let mut map = ['\0'; 256];
    let mut next = 256u32;
    for b in 0..=255u8 {
        let printable =
            (b'!'..=b'~').contains(&b) || (0xA1..=0xAC).contains(&b) || (0xAE..=0xFF).contains(&b);
        map[b as usize] = if printable {
            char::from(b)
        } else {
            let c = char::from_u32(next).unwrap_or('?');
            next += 1;
            c
        };
    }
    map
}

impl Tokenizer {
    pub fn from_gguf(g: &Gguf) -> Result<Tokenizer, String> {
        let model = g
            .get("tokenizer.ggml.model")
            .and_then(Meta::as_str)
            .unwrap_or("");
        if model != "gpt2" {
            return Err(format!(
                "tokenizer '{model}' is not supported (byte-level BPE, 'gpt2', only)"
            ));
        }
        let tokens: Vec<String> = g
            .get("tokenizer.ggml.tokens")
            .and_then(Meta::as_array)
            .ok_or("the model file has no tokenizer.ggml.tokens")?
            .iter()
            .map(|m| m.as_str().unwrap_or("").to_string())
            .collect();
        let types: Vec<i64> = g
            .get("tokenizer.ggml.token_type")
            .and_then(Meta::as_array)
            .map(|a| a.iter().map(|m| m.as_int().unwrap_or(1)).collect())
            .unwrap_or_default();
        let merges = g
            .get("tokenizer.ggml.merges")
            .and_then(Meta::as_array)
            .ok_or("the model file has no tokenizer.ggml.merges")?;
        let mut ranks = HashMap::with_capacity(merges.len());
        for (i, m) in merges.iter().enumerate() {
            if let Some((a, b)) = m.as_str().and_then(|s| s.split_once(' ')) {
                ranks.insert((a.to_string(), b.to_string()), i as u32);
            }
        }
        let ids: HashMap<String, u32> = tokens
            .iter()
            .enumerate()
            .map(|(i, t)| (t.clone(), i as u32))
            .collect();
        let mut specials: Vec<(String, u32, bool)> = types
            .iter()
            .enumerate()
            .filter(|(_, t)| **t == TYPE_CONTROL || **t == TYPE_USER_DEFINED)
            .filter(|(i, _)| !tokens[*i].is_empty())
            .map(|(i, t)| (tokens[i].clone(), i as u32, *t == TYPE_CONTROL))
            .collect();
        specials.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then(a.1.cmp(&b.1)));
        let byte_to_char = byte_map();
        let char_to_byte = byte_to_char
            .iter()
            .enumerate()
            .map(|(b, c)| (*c, b as u8))
            .collect();
        let eos = g
            .get("tokenizer.ggml.eos_token_id")
            .and_then(Meta::as_int)
            .unwrap_or(0) as u32;
        Ok(Tokenizer {
            tokens,
            ids,
            ranks,
            specials,
            byte_to_char,
            char_to_byte,
            eos,
        })
    }

    pub fn vocab_size(&self) -> usize {
        self.tokens.len()
    }

    /// The id of an exact token string (e.g. `<|im_end|>`).
    pub fn id(&self, token: &str) -> Option<u32> {
        self.ids.get(token).copied()
    }

    /// Text to ids. `control`: whether control tokens in `text` count.
    pub fn encode(&self, text: &str, control: bool) -> Vec<u32> {
        let mut out = Vec::new();
        let mut rest = text;
        'outer: while !rest.is_empty() {
            // The earliest special token in what is left (longest at a tie).
            let mut best: Option<(usize, &str, u32)> = None;
            for (s, id, is_control) in &self.specials {
                if *is_control && !control {
                    continue;
                }
                if let Some(at) = rest.find(s.as_str()) {
                    if best.is_none_or(|(b, bs, _)| at < b || (at == b && s.len() > bs.len())) {
                        best = Some((at, s.as_str(), *id));
                    }
                }
            }
            match best {
                Some((at, s, id)) => {
                    self.encode_plain(&rest[..at], &mut out);
                    out.push(id);
                    rest = &rest[at + s.len()..];
                }
                None => {
                    self.encode_plain(rest, &mut out);
                    break 'outer;
                }
            }
        }
        out
    }

    fn encode_plain(&self, text: &str, out: &mut Vec<u32>) {
        for piece in pretokenize(text) {
            let mapped: String = piece
                .bytes()
                .map(|b| self.byte_to_char[b as usize])
                .collect();
            if let Some(id) = self.ids.get(&mapped) {
                out.push(*id);
                continue;
            }
            for sym in self.bpe(&mapped) {
                match self.ids.get(&sym) {
                    Some(id) => out.push(*id),
                    // A symbol with no token: fall back to its single
                    // characters (every byte has one in a byte-level vocab).
                    None => out.extend(
                        sym.chars()
                            .filter_map(|c| self.ids.get(&c.to_string()).copied()),
                    ),
                }
            }
        }
    }

    /// Merge adjacent symbols by rank until no merge applies.
    fn bpe(&self, word: &str) -> Vec<String> {
        let mut syms: Vec<String> = word.chars().map(|c| c.to_string()).collect();
        while syms.len() > 1 {
            let mut best: Option<(u32, usize)> = None;
            for i in 0..syms.len() - 1 {
                if let Some(r) = self.ranks.get(&(syms[i].clone(), syms[i + 1].clone())) {
                    if best.is_none_or(|(br, _)| *r < br) {
                        best = Some((*r, i));
                    }
                }
            }
            let Some((_, i)) = best else { break };
            let right = syms.remove(i + 1);
            syms[i].push_str(&right);
        }
        syms
    }

    /// One token's bytes (a token can be part of a UTF-8 character).
    pub fn token_bytes(&self, id: u32) -> Vec<u8> {
        let Some(t) = self.tokens.get(id as usize) else {
            return Vec::new();
        };
        if self.specials.iter().any(|(_, sid, _)| *sid == id) {
            return t.as_bytes().to_vec();
        }
        t.chars()
            .map(|c| self.char_to_byte.get(&c).copied().unwrap_or(b'?'))
            .collect()
    }

    pub fn decode(&self, ids: &[u32]) -> String {
        let bytes: Vec<u8> = ids.iter().flat_map(|i| self.token_bytes(*i)).collect();
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

/// Qwen's pre-tokenizer, as a hand-written matcher for its regex:
///
/// ```text
///  '[sS]|'[tT]|'[rR][eE]|'[vV][eE]|'[mM]|'[lL][lL]|'[dD]   contractions
///  [^\r\n\p{L}\p{N}]?\p{L}+                                 a word, with one leading non-letter
///  \p{N}                                                    one digit
///   ?[^\s\p{L}\p{N}]+[\r\n]*                                punctuation (one leading space)
///  \s*[\r\n]+                                               whitespace ending in newlines
///  \s+(?!\S)                                                spaces not before a word
///  \s+                                                      any other spaces
/// ```
///
/// Alternatives are tried in order at each position, first match wins —
/// the same semantics as the regex.
pub fn pretokenize(text: &str) -> Vec<&str> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let n = chars.len();
    let at = |i: usize| chars.get(i).map(|c| c.1);
    let byte = |i: usize| if i < n { chars[i].0 } else { text.len() };
    let is_nl = |c: char| c == '\r' || c == '\n';
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        let c = chars[i].1;
        let len = (|| {
            // 1. contractions
            if c == '\'' {
                if let Some(c1) = at(i + 1) {
                    let c1 = c1.to_ascii_lowercase();
                    let c2 = at(i + 2).map(|x| x.to_ascii_lowercase());
                    match (c1, c2) {
                        ('r', Some('e')) | ('v', Some('e')) | ('l', Some('l')) => return 3,
                        ('s' | 't' | 'm' | 'd', _) => return 2,
                        _ => {}
                    }
                }
            }
            // 2. [^\r\n\p{L}\p{N}]?\p{L}+
            let starts_word = |j: usize| at(j).is_some_and(is_letter);
            let letters_from = |j: usize| (j..n).take_while(|&k| is_letter(chars[k].1)).count();
            if !is_nl(c) && !is_letter(c) && !is_number(c) && starts_word(i + 1) {
                return 1 + letters_from(i + 1);
            }
            if is_letter(c) {
                return letters_from(i);
            }
            // 3. \p{N}
            if is_number(c) {
                return 1;
            }
            // 4.  ?[^\s\p{L}\p{N}]+[\r\n]*
            let punct = |x: char| !x.is_whitespace() && !is_letter(x) && !is_number(x);
            let start = if c == ' ' && at(i + 1).is_some_and(punct) {
                i + 1
            } else {
                i
            };
            if at(start).is_some_and(punct) {
                let mut j = start;
                while at(j).is_some_and(punct) {
                    j += 1;
                }
                while at(j).is_some_and(is_nl) {
                    j += 1;
                }
                return j - i;
            }
            // Whitespace: the run from i.
            let run = (i..n).take_while(|&k| chars[k].1.is_whitespace()).count();
            if run == 0 {
                return 1; // unreachable for valid categories; never stall
            }
            // 5. \s*[\r\n]+ : up to and including the last newline in the run.
            if let Some(last_nl) = (i..i + run).rev().find(|&k| is_nl(chars[k].1)) {
                return last_nl + 1 - i;
            }
            // 6. \s+(?!\S) : all but the last space when a non-space follows.
            if i + run < n {
                if run > 1 {
                    return run - 1;
                }
                // 7. \s+ (a single space before a word stands alone)
                return run;
            }
            run
        })();
        out.push(&text[byte(i)..byte(i + len)]);
        i += len;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pieces_follow_the_qwen_regex() {
        assert_eq!(pretokenize("Hello world"), ["Hello", " world"]);
        assert_eq!(
            pretokenize("I'm 2024!"),
            ["I", "'m", " ", "2", "0", "2", "4", "!"]
        );
        assert_eq!(pretokenize("a  b"), ["a", " ", " b"]);
        assert_eq!(pretokenize("a\n\n b"), ["a", "\n\n", " b"]);
        assert_eq!(pretokenize("  "), ["  "]);
        assert_eq!(pretokenize("x ..."), ["x", " ..."]);
        assert_eq!(
            pretokenize("मैं"),
            ["म", "ैं"],
            "combining marks are not letters: they group like punctuation"
        );
    }
}
