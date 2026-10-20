//! HTTP/1.1 client core: request encoding and response decoding over
//! an abstract transport.
//!
//! Every LLM REST API speaks HTTP/1.1; HTTP/2 is a deliberate scope cut.
//! This module does framing only — no sockets. The runtime supplies TCP
//! and TLS streams, which keeps core pure and this module testable with
//! byte slices.
//!
//! Hostile-server rules (all found in review of v0.1, all tested):
//!
//! * Nothing is allocated from a number the server sent. Bodies are read
//!   in bounded pieces against [`Limits::max_body`]; a `Content-Length:
//!   99999999999999` is an error, not an allocation abort.
//! * The response head is capped at [`Limits::max_head`] bytes.
//! * Header names and values are validated when a request is encoded: a
//!   CR or LF in a value (header injection) refuses to encode.
//! * Credentials in URLs (`https://user:pass@host`) are refused rather
//!   than silently stripped — secrets travel as references, never inline.
//!
//! Reads go through a `BufRead`, never one byte per `read` call: on a
//! real socket the v0.1 line reader cost one syscall per byte.

use std::io::{BufRead, BufReader, Read, Write};

// ── url ──────────────────────────────────────────────────────────

/// The small URL shape our providers and tools need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    pub scheme: String,
    /// Host without brackets (an IPv6 literal is stored bare).
    pub host: String,
    pub port: u16,
    /// Path including any query string; always starts with '/'.
    pub path: String,
}

impl Url {
    pub fn is_tls(&self) -> bool {
        self.scheme == "https"
    }

    fn default_port(&self) -> u16 {
        if self.is_tls() {
            443
        } else {
            80
        }
    }

    /// The `Host` header value: host, bracketed if IPv6, plus the port
    /// when it is not the scheme's default.
    pub fn host_header(&self) -> String {
        let host = if self.host.contains(':') { format!("[{}]", self.host) } else { self.host.clone() };
        if self.port == self.default_port() {
            host
        } else {
            format!("{host}:{}", self.port)
        }
    }

    /// `host:port` for CONNECT and logs.
    pub fn authority(&self) -> String {
        let host = if self.host.contains(':') { format!("[{}]", self.host) } else { self.host.clone() };
        format!("{host}:{}", self.port)
    }
}

impl std::fmt::Display for Url {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}://{}{}", self.scheme, self.host_header(), self.path)
    }
}

/// Parse `http(s)://host[:port][/path][?query]`. Fragments are dropped.
pub fn parse_url(s: &str) -> Result<Url, String> {
    if s.bytes().any(|b| b <= b' ' || b == 0x7f) {
        return Err("URL contains spaces or control characters".into());
    }
    let (scheme, rest) = s
        .split_once("://")
        .ok_or_else(|| format!("URL needs a scheme (https://...): {s}"))?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "https" && scheme != "http" {
        return Err(format!("unsupported scheme '{scheme}' (http or https only)"));
    }
    // The authority ends at the first '/', '?' or '#'.
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    if authority.contains('@') {
        return Err("credentials in URLs are refused; use a keychain secret instead".into());
    }
    let tail = tail.split('#').next().unwrap_or("");
    let path = if tail.is_empty() {
        "/".to_string()
    } else if tail.starts_with('?') {
        format!("/{tail}")
    } else {
        tail.to_string()
    };

    let default_port = if scheme == "https" { 443 } else { 80 };
    let (host, port) = if let Some(after) = authority.strip_prefix('[') {
        let (h, rest) = after.split_once(']').ok_or("unterminated IPv6 literal")?;
        let port = match rest.strip_prefix(':') {
            Some(p) => p.parse().map_err(|_| format!("invalid port in '{authority}'"))?,
            None if rest.is_empty() => default_port,
            None => return Err(format!("invalid authority '{authority}'")),
        };
        (h.to_string(), port)
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse().map_err(|_| format!("invalid port in '{authority}'"))?),
            None => (authority.to_string(), default_port),
        }
    };
    if host.is_empty() {
        return Err("URL has an empty host".into());
    }
    Ok(Url { scheme, host: host.to_ascii_lowercase(), port, path })
}

// ── limits ───────────────────────────────────────────────────────

/// Bounds on what a server may make us read.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Status line + headers, in bytes.
    pub max_head: usize,
    /// Decoded body, in bytes.
    pub max_body: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { max_head: 64 * 1024, max_body: 32 * 1024 * 1024 }
    }
}

// ── request ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub url: Url,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

fn valid_token(s: &str) -> bool {
    !s.is_empty()
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

fn valid_value(s: &str) -> bool {
    s.bytes().all(|b| b == b'\t' || (b >= 0x20 && b != 0x7f))
}

impl Request {
    pub fn new(method: &str, url: Url) -> Self {
        Request { method: method.to_string(), url, headers: Vec::new(), body: Vec::new() }
    }

    /// Add a header. Validated at encode time (an invalid header makes
    /// `encode` fail, so a bad value can never reach the wire).
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    pub fn body(mut self, body: Vec<u8>) -> Self {
        self.body = body;
        self
    }

    pub fn json_body(mut self, body: String) -> Self {
        self.body = body.into_bytes();
        self
    }

    /// Encode onto a stream. Host, Content-Length and Connection are
    /// added. Fails without writing anything if any header is invalid.
    pub fn encode(&self, out: &mut dyn Write) -> std::io::Result<()> {
        let head = self.head_text().map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        out.write_all(head.as_bytes())?;
        out.write_all(&self.body)?;
        out.flush()
    }

    fn head_text(&self) -> Result<String, String> {
        if !valid_token(&self.method) {
            return Err(format!("invalid HTTP method '{}'", self.method));
        }
        let mut head = format!("{} {} HTTP/1.1\r\n", self.method, self.url.path);
        head.push_str(&format!("Host: {}\r\n", self.url.host_header()));
        for (k, v) in &self.headers {
            if !valid_token(k) {
                return Err(format!("invalid header name '{k}'"));
            }
            if !valid_value(v) {
                return Err(format!("header '{k}' contains a line break or control character (refused)"));
            }
            head.push_str(&format!("{k}: {v}\r\n"));
        }
        if !self.body.is_empty() || self.method == "POST" || self.method == "PUT" {
            head.push_str(&format!("Content-Length: {}\r\n", self.body.len()));
        }
        head.push_str("Connection: close\r\n\r\n");
        Ok(head)
    }

    /// The encoded head as text, for tests and logs. Secrets never get
    /// here: auth headers are added by the runtime at send time only.
    pub fn encode_head(&self) -> String {
        self.head_text().unwrap_or_else(|e| format!("<invalid request: {e}>"))
    }
}

// ── response ─────────────────────────────────────────────────────

/// Status line + headers. Header names are lowercased on purpose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub status: u16,
    pub headers: Vec<(String, String)>,
}

impl Head {
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers.iter().find(|(k, _)| *k == name).map(|(_, v)| v.as_str())
    }
}

/// A fully read response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers.iter().find(|(k, _)| *k == name).map(|(_, v)| v.as_str())
    }

    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// Read one CRLF/LF-terminated line, counting against `budget`.
fn read_line_bounded(r: &mut dyn BufRead, budget: &mut usize) -> Result<String, String> {
    let mut buf = Vec::new();
    loop {
        let available = r.fill_buf().map_err(|e| format!("read: {e}"))?;
        if available.is_empty() {
            if buf.is_empty() {
                return Err("connection closed before a complete response".into());
            }
            break;
        }
        let (take, done) = match available.iter().position(|&b| b == b'\n') {
            Some(i) => (i + 1, true),
            None => (available.len(), false),
        };
        if take > *budget {
            return Err("response head too large (refused)".into());
        }
        *budget -= take;
        buf.extend_from_slice(&available[..take]);
        r.consume(take);
        if done {
            break;
        }
    }
    while matches!(buf.last(), Some(b'\n') | Some(b'\r')) {
        buf.pop();
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// Read the status line and headers.
pub fn read_head(r: &mut dyn BufRead, limits: &Limits) -> Result<Head, String> {
    let mut budget = limits.max_head;
    let status_line = read_line_bounded(r, &mut budget)?;
    let mut parts = status_line.splitn(3, ' ');
    let version = parts.next().unwrap_or("");
    if !version.starts_with("HTTP/1.") {
        return Err(format!("bad status line: '{status_line}'"));
    }
    let status: u16 = parts
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("bad status line: '{status_line}'"))?;
    let mut headers = Vec::new();
    loop {
        let line = read_line_bounded(r, &mut budget)?;
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
    }
    Ok(Head { status, headers })
}

#[derive(Debug, Clone, Copy)]
enum Framing {
    Length(u64),
    Chunked,
    Eof,
}

/// A streaming, bounded body decoder. Implements `Read`, so it plugs
/// into the SSE parser chunk by chunk as bytes arrive.
pub struct BodyReader<R: BufRead> {
    inner: R,
    framing: Framing,
    /// Bytes left in the current chunk (chunked) or body (length).
    remaining: u64,
    done: bool,
    total: usize,
    max_body: usize,
}

impl<R: BufRead> BodyReader<R> {
    /// Decide framing from the head and wrap the stream.
    pub fn new(inner: R, head: &Head, limits: &Limits) -> Result<Self, String> {
        let chunked = head
            .header("transfer-encoding")
            .map(|v| v.to_ascii_lowercase().contains("chunked"))
            .unwrap_or(false);
        let framing = if chunked {
            Framing::Chunked
        } else if let Some(len) = head.header("content-length") {
            let n: u64 = len.trim().parse().map_err(|_| format!("bad Content-Length '{len}'"))?;
            if n > limits.max_body as u64 {
                return Err(format!("response body of {n} bytes exceeds the {}-byte limit (refused)", limits.max_body));
            }
            Framing::Length(n)
        } else if head.status == 204 || head.status == 304 || (100..200).contains(&head.status) {
            Framing::Length(0)
        } else {
            Framing::Eof
        };
        let remaining = match framing {
            Framing::Length(n) => n,
            _ => 0,
        };
        Ok(BodyReader { inner, framing, remaining, done: matches!(framing, Framing::Length(0)), total: 0, max_body: limits.max_body })
    }

    fn next_chunk_size(&mut self) -> std::io::Result<u64> {
        let mut budget = 1024usize;
        let line = read_line_bounded(&mut self.inner, &mut budget).map_err(std::io::Error::other)?;
        let size_hex = line.split(';').next().unwrap_or("").trim();
        u64::from_str_radix(size_hex, 16)
            .map_err(|_| std::io::Error::other(format!("bad chunk size '{size_hex}'")))
    }

    fn count(&mut self, n: usize) -> std::io::Result<()> {
        self.total += n;
        if self.total > self.max_body {
            return Err(std::io::Error::other(format!("response body exceeds the {}-byte limit (refused)", self.max_body)));
        }
        Ok(())
    }
}

impl<R: BufRead> Read for BodyReader<R> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.done || out.is_empty() {
            return Ok(0);
        }
        match self.framing {
            Framing::Eof => {
                let n = self.inner.read(out)?;
                if n == 0 {
                    self.done = true;
                }
                self.count(n)?;
                Ok(n)
            }
            Framing::Length(_) => {
                let want = (self.remaining.min(out.len() as u64)) as usize;
                let n = self.inner.read(&mut out[..want])?;
                if n == 0 {
                    return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "body truncated"));
                }
                self.remaining -= n as u64;
                if self.remaining == 0 {
                    self.done = true;
                }
                self.count(n)?;
                Ok(n)
            }
            Framing::Chunked => {
                if self.remaining == 0 {
                    let size = self.next_chunk_size()?;
                    if size == 0 {
                        // Trailers until a blank line.
                        let mut budget = 16 * 1024;
                        loop {
                            let l = read_line_bounded(&mut self.inner, &mut budget).map_err(std::io::Error::other)?;
                            if l.is_empty() {
                                break;
                            }
                        }
                        self.done = true;
                        return Ok(0);
                    }
                    if size > self.max_body as u64 {
                        return Err(std::io::Error::other("chunk larger than the body limit (refused)"));
                    }
                    self.remaining = size;
                }
                let want = (self.remaining.min(out.len() as u64)) as usize;
                let n = self.inner.read(&mut out[..want])?;
                if n == 0 {
                    return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "chunk truncated"));
                }
                self.remaining -= n as u64;
                if self.remaining == 0 {
                    let mut budget = 4;
                    read_line_bounded(&mut self.inner, &mut budget).map_err(std::io::Error::other)?; // CRLF after data
                }
                self.count(n)?;
                Ok(n)
            }
        }
    }
}

/// Read one full response (head + body) with the given limits.
pub fn decode_response_with(r: &mut dyn Read, limits: &Limits) -> Result<Response, String> {
    let mut br = BufReader::with_capacity(16 * 1024, r);
    let head = read_head(&mut br, limits)?;
    let mut body_reader = BodyReader::new(&mut br, &head, limits)?;
    let mut body = Vec::new();
    let mut buf = [0u8; 16 * 1024];
    loop {
        let n = body_reader.read(&mut buf).map_err(|e| format!("read body: {e}"))?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&buf[..n]);
    }
    Ok(Response { status: head.status, headers: head.headers, body })
}

/// Read one full response with default limits.
pub fn decode_response(r: &mut dyn Read) -> Result<Response, String> {
    decode_response_with(r, &Limits::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn url_parsing() {
        let u = parse_url("https://api.openai.com/v1/chat/completions").unwrap();
        assert_eq!((u.host.as_str(), u.port, u.path.as_str()), ("api.openai.com", 443, "/v1/chat/completions"));
        let u = parse_url("http://localhost:8080").unwrap();
        assert_eq!((u.port, u.path.as_str()), (8080, "/"));
        let u = parse_url("https://example.com?q=1#frag").unwrap();
        assert_eq!((u.host.as_str(), u.path.as_str()), ("example.com", "/?q=1"));
        let u = parse_url("http://[::1]:7878/x").unwrap();
        assert_eq!((u.host.as_str(), u.port), ("::1", 7878));
        assert_eq!(u.host_header(), "[::1]:7878");
        assert!(parse_url("ftp://x").is_err());
        assert!(parse_url("https://user:pw@host/").unwrap_err().contains("credentials"));
        assert!(parse_url("https://a b/").is_err());
    }

    #[test]
    fn request_encoding() {
        let req = Request::new("POST", parse_url("https://api.openai.com/v1/chat/completions").unwrap())
            .header("Content-Type", "application/json")
            .json_body("{\"model\":\"x\"}".into());
        let head = req.encode_head();
        assert!(head.starts_with("POST /v1/chat/completions HTTP/1.1\r\n"));
        assert!(head.contains("Host: api.openai.com\r\n"));
        assert!(head.contains("Content-Length: 13\r\n"));
        let req = Request::new("GET", parse_url("http://127.0.0.1:7878/").unwrap());
        assert!(req.encode_head().contains("Host: 127.0.0.1:7878\r\n"));
    }

    #[test]
    fn header_injection_is_refused() {
        let req = Request::new("GET", parse_url("http://x/").unwrap()).header("X-Note", "a\r\nEvil: 1");
        let mut out = Vec::new();
        assert!(req.encode(&mut out).is_err());
        assert!(out.is_empty(), "nothing reaches the wire");
    }

    #[test]
    fn decodes_content_length_chunked_and_eof() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
        assert_eq!(decode_response(&mut Cursor::new(&raw[..])).unwrap().body, b"hello");
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n";
        assert_eq!(decode_response(&mut Cursor::new(&raw[..])).unwrap().body, b"hello world");
        let raw = b"HTTP/1.1 200 OK\r\n\r\nruns to the end";
        assert_eq!(decode_response(&mut Cursor::new(&raw[..])).unwrap().body, b"runs to the end");
    }

    #[test]
    fn hostile_sizes_are_refused_not_allocated() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 99999999999999\r\n\r\nx";
        assert!(decode_response(&mut Cursor::new(&raw[..])).unwrap_err().contains("exceeds"));
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nffffffffffff\r\nx";
        assert!(decode_response(&mut Cursor::new(&raw[..])).is_err());
        let mut huge = b"HTTP/1.1 200 OK\r\nX: ".to_vec();
        huge.extend(std::iter::repeat_n(b'a', 100_000));
        assert!(decode_response(&mut Cursor::new(&huge[..])).unwrap_err().contains("too large"));
        let small = Limits { max_head: 1024, max_body: 4 };
        let raw = b"HTTP/1.1 200 OK\r\n\r\nmore than four";
        assert!(decode_response_with(&mut Cursor::new(&raw[..]), &small).is_err());
    }

    #[test]
    fn bad_status_line_is_rejected() {
        assert!(decode_response(&mut Cursor::new(&b"garbage\r\n\r\n"[..])).is_err());
    }

    #[test]
    fn body_reader_streams_chunks() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n3\r\ndef\r\n0\r\n\r\n";
        let mut br = BufReader::new(Cursor::new(&raw[..]));
        let head = read_head(&mut br, &Limits::default()).unwrap();
        let mut body = BodyReader::new(br, &head, &Limits::default()).unwrap();
        let mut buf = [0u8; 2];
        let mut got = Vec::new();
        loop {
            let n = body.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            got.extend_from_slice(&buf[..n]);
        }
        assert_eq!(got, b"abcdef");
    }
}
