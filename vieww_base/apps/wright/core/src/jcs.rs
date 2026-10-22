//! JSON Canonicalization Scheme (RFC 8785), and the two encodings a
//! standard proof needs around it: base58btc and `did:key` for Ed25519.
//!
//! JCS turns a JSON value into ONE byte string, so a signature over it can
//! be checked by any implementation in any language: object members sorted
//! by their UTF-16 code units, no whitespace, strings escaped exactly as
//! ECMAScript's `JSON.stringify` does, numbers written exactly as
//! ECMAScript's `Number.prototype.toString` does. That is what makes a
//! Wright receipt checkable WITHOUT Wright (the `eddsa-jcs-2022`
//! cryptosuite of W3C Data Integrity, see `wright-engine::credential`).
//!
//! std-only, like the rest of this crate.

use crate::wire::json::Value;

/// The canonical bytes of `v`. Fails only on numbers JSON cannot carry
/// (NaN, infinities).
pub fn canonicalize(v: &Value) -> Result<String, String> {
    let mut out = String::new();
    write(v, &mut out)?;
    Ok(out)
}

fn write(v: &Value, out: &mut String) -> Result<(), String> {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Num(n) => out.push_str(&number(*n)?),
        Value::Str(s) => string(s, out),
        Value::Arr(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write(item, out)?;
            }
            out.push(']');
        }
        Value::Obj(fields) => {
            let mut sorted: Vec<&(String, Value)> = fields.iter().collect();
            // RFC 8785 §3.2.3: by UTF-16 code units, not by bytes or chars.
            sorted.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
            for w in sorted.windows(2) {
                if w[0].0 == w[1].0 {
                    return Err(format!("duplicate member '{}' cannot be canonicalized", w[0].0));
                }
            }
            out.push('{');
            for (i, (k, val)) in sorted.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                string(k, out);
                out.push(':');
                write(val, out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

/// `JSON.stringify` for a string.
fn string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// `Number.prototype.toString` for a finite double (ECMA-262 §6.1.6.1.20):
/// the shortest digits that round-trip, then the ES layout rules.
pub fn number(x: f64) -> Result<String, String> {
    if !x.is_finite() {
        return Err("NaN and infinities are not JSON".into());
    }
    if x == 0.0 {
        return Ok("0".into()); // -0 too
    }
    let neg = x < 0.0;
    // Rust's `{:e}` is the shortest round-trip form: "d.ddde±x".
    let e_form = format!("{:e}", x.abs());
    let (mantissa, exp) = e_form.split_once('e').expect("{:e} always has an exponent");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let exp: i32 = exp.parse().expect("{:e} exponent is an integer");
    let k = digits.len() as i32;
    let n = exp + 1; // the decimal point sits after n digits
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let sign = if n > 0 { "+" } else { "-" };
        let tail = if k == 1 { String::new() } else { format!(".{}", &digits[1..]) };
        format!("{}{tail}e{sign}{}", &digits[..1], (n - 1).abs())
    };
    Ok(if neg { format!("-{body}") } else { body })
}

// ── base58btc and did:key ──────────────────────────────────────────

const ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/// Bitcoin's base58 (what multibase calls `base58btc`, prefix `z`).
pub fn base58_encode(bytes: &[u8]) -> String {
    let zeros = bytes.iter().take_while(|b| **b == 0).count();
    let mut digits: Vec<u8> = Vec::new(); // little-endian base-58 digits
    for &b in &bytes[zeros..] {
        let mut carry = b as u32;
        for d in digits.iter_mut() {
            carry += (*d as u32) << 8;
            *d = (carry % 58) as u8;
            carry /= 58;
        }
        while carry > 0 {
            digits.push((carry % 58) as u8);
            carry /= 58;
        }
    }
    let mut out = String::with_capacity(zeros + digits.len());
    out.extend(std::iter::repeat_n('1', zeros));
    out.extend(digits.iter().rev().map(|d| ALPHABET[*d as usize] as char));
    out
}

pub fn base58_decode(s: &str) -> Result<Vec<u8>, String> {
    let zeros = s.chars().take_while(|c| *c == '1').count();
    let mut bytes: Vec<u8> = Vec::new(); // little-endian base-256
    for c in s.chars().skip(zeros) {
        let v = ALPHABET
            .iter()
            .position(|a| *a as char == c)
            .ok_or_else(|| format!("'{c}' is not a base58 character"))? as u32;
        let mut carry = v;
        for b in bytes.iter_mut() {
            carry += (*b as u32) * 58;
            *b = (carry & 0xff) as u8;
            carry >>= 8;
        }
        while carry > 0 {
            bytes.push((carry & 0xff) as u8);
            carry >>= 8;
        }
    }
    let mut out = vec![0u8; zeros];
    out.extend(bytes.iter().rev());
    Ok(out)
}

/// The multicodec prefix of an Ed25519 public key (`ed25519-pub`, 0xed).
const ED25519_PUB: [u8; 2] = [0xed, 0x01];

/// `did:key:z6Mk…` for an Ed25519 public key.
pub fn did_key(public_key: &[u8; 32]) -> String {
    format!("did:key:{}", multibase_key(public_key))
}

/// The multibase form (`z6Mk…`) of an Ed25519 public key.
pub fn multibase_key(public_key: &[u8; 32]) -> String {
    let mut b = ED25519_PUB.to_vec();
    b.extend_from_slice(public_key);
    format!("z{}", base58_encode(&b))
}

/// The Ed25519 public key inside a `did:key` (with or without `#fragment`).
pub fn did_key_public(did: &str) -> Result<[u8; 32], String> {
    let id = did.split('#').next().unwrap_or(did);
    let mb = id
        .strip_prefix("did:key:z")
        .ok_or_else(|| format!("'{did}' is not a base58 did:key"))?;
    let raw = base58_decode(mb)?;
    if raw.len() != 34 || raw[..2] != ED25519_PUB {
        return Err(format!("'{did}' is not an Ed25519 did:key"));
    }
    let mut k = [0u8; 32];
    k.copy_from_slice(&raw[2..]);
    Ok(k)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::json;

    #[test]
    #[allow(clippy::excessive_precision)] // RFC 8785's own literal, on purpose
    fn rfc8785_numbers() {
        // RFC 8785 Appendix B and ECMA-262's own examples.
        for (x, want) in [
            (0.0, "0"),
            (-0.0, "0"),
            (1.0, "1"),
            (4.5, "4.5"),
            (0.002, "0.002"),
            (1e-7, "1e-7"),
            (1e21, "1e+21"),
            (1e20, "100000000000000000000"),
            (1e30, "1e+30"),
            (333333333.33333329, "333333333.3333333"),
            (9007199254740992.0, "9007199254740992"),
            (-1.5e-9, "-1.5e-9"),
            (5e-324, "5e-324"),
            (1.7976931348623157e308, "1.7976931348623157e+308"),
            (295147905179352830000.0, "295147905179352830000"),
            (0.000001, "0.000001"),
        ] {
            assert_eq!(number(x).unwrap(), want, "{x:e}");
        }
        assert!(number(f64::NAN).is_err());
    }

    #[test]
    fn rfc8785_sorting_and_strings() {
        // RFC 8785 §3.2.3's sorting example: UTF-16 code units, so the
        // emoji (a surrogate pair, 0xD83D…) sorts before U+FB33.
        let v = json::parse(r#"{"€":"Euro Sign","\r":"Carriage Return","דּ":"Hebrew Letter Dalet With Dagesh","1":"One","😀":"Emoji: Grinning Face","\u0080":"Control","ö":"Latin Small Letter O With Diaeresis"}"#).unwrap();
        let keys: Vec<String> = match json::parse(&canonicalize(&v).unwrap()).unwrap() {
            Value::Obj(f) => f.into_iter().map(|(k, _)| k).collect(),
            _ => unreachable!(),
        };
        assert_eq!(keys, vec!["\r", "1", "\u{80}", "\u{f6}", "\u{20ac}", "\u{1F600}", "\u{fb33}"]);
        // Escapes exactly as JSON.stringify; no whitespace anywhere.
        let v = json::parse(r#"{ "b": [1, true, null], "a": "x\"\\\n\u0001 é" }"#).unwrap();
        assert_eq!(canonicalize(&v).unwrap(), "{\"a\":\"x\\\"\\\\\\n\\u0001\u{2028}é\",\"b\":[1,true,null]}");
    }

    #[test]
    fn base58_and_did_key_round_trip() {
        // Bitcoin's own vectors.
        assert_eq!(base58_encode(b"hello world"), "StV1DL6CwTryKyV");
        assert_eq!(base58_encode(&[0, 0, 1]), "112");
        assert_eq!(base58_decode("StV1DL6CwTryKyV").unwrap(), b"hello world");
        // The did:key spec's Ed25519 example decodes to a 32-byte key and
        // encodes back to itself; every Ed25519 did:key starts z6Mk.
        let did = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
        let pk = did_key_public(did).unwrap();
        assert_eq!(did_key(&pk), did);
        assert!(did_key(&[7u8; 32]).starts_with("did:key:z6Mk"));
        assert!(did_key_public("did:key:zQ3s").is_err());
    }
}
