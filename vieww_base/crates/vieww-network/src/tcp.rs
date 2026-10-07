//! Real transports on `std::net` — no runtime, no external crate.
//!
//! * [`fetch`] speaks HTTP/1.1 over a `TcpStream`: request line, headers
//!   (`Host`, `Content-Length`, `Connection: close` added), body; the
//!   response's status line, headers and body by `Content-Length`,
//!   `Transfer-Encoding: chunked`, or read-to-close; redirects (301/302/
//!   303/307/308) followed up to a limit. [`TcpClient`] wraps it as an
//!   [`HttpClient`] whose [`Task`] runs on a worker thread.
//! * [`WebSocket`] is RFC 6455: the opening handshake (a random key, the
//!   `Sec-WebSocket-Accept` check by SHA-1 + base64 — both implemented
//!   here), masked client frames, 7/16/64-bit lengths, text/binary/ping/
//!   pong/close opcodes, fragmented messages reassembled, pings answered.
//!   [`WebSocket::accept`] is the server side, so two vieww programs can
//!   talk to each other (and the tests run without a third party).
//!
//! **TLS is not implemented**: an `https`/`wss` URL is refused by name
//! ([`NetError::Connection`] saying so). A from-scratch TLS 1.3 stack is a
//! project of its own; the platform's TLS plugs in behind [`HttpClient`].

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use vieww_foundation::task::{FrameWaker, NoWaker, Task, Threads};

use crate::{HttpClient, HttpRequest, HttpResponse, Method, NetError, Scheme, Url};

fn conn_err(e: impl std::fmt::Display) -> NetError {
    NetError::Connection(e.to_string())
}

fn connect(url: &Url, timeout: Duration) -> Result<TcpStream, NetError> {
    if url.scheme() == Scheme::Https {
        return Err(NetError::Connection(
            "TLS is not implemented in vieww's own transport; use a platform client for https".into(),
        ));
    }
    let addr = (url.host().as_str(), url.port())
        .to_socket_addrs()
        .map_err(conn_err)?
        .next()
        .ok_or_else(|| NetError::Connection(format!("{} did not resolve", url.host())))?;
    let s = TcpStream::connect_timeout(&addr, timeout).map_err(conn_err)?;
    s.set_read_timeout(Some(timeout)).map_err(conn_err)?;
    s.set_write_timeout(Some(timeout)).map_err(conn_err)?;
    Ok(s)
}

fn target(url: &Url) -> String {
    match url.query() {
        Some(q) => format!("{}?{q}", url.path()),
        None => url.path().clone(),
    }
}

fn read_headers(r: &mut impl BufRead) -> Result<(String, Vec<(String, String)>), NetError> {
    let mut status = String::new();
    r.read_line(&mut status).map_err(conn_err)?;
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).map_err(conn_err)? == 0 {
            break;
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_owned(), v.trim().to_owned()));
        }
    }
    Ok((status.trim_end().to_owned(), headers))
}

fn header<'a>(h: &'a [(String, String)], name: &str) -> Option<&'a str> {
    h.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

/// Send `req` and read the whole response (blocking), following up to
/// `redirects` redirects.
///
/// # Errors
/// A connection failure, `https`, or a malformed response.
pub fn fetch(req: &HttpRequest, timeout: Duration, redirects: u32) -> Result<HttpResponse, NetError> {
    let mut req = req.clone();
    for _ in 0..=redirects {
        let s = connect(&req.url, timeout)?;
        let mut w = s.try_clone().map_err(conn_err)?;
        let host = if req.url.port() == req.url.scheme().default_port() {
            req.url.host().clone()
        } else {
            format!("{}:{}", req.url.host(), req.url.port())
        };
        let mut head = format!("{} {} HTTP/1.1\r\nHost: {host}\r\n", req.method, target(&req.url));
        for (k, v) in &req.headers {
            head.push_str(&format!("{k}: {v}\r\n"));
        }
        if !req.body.is_empty() || matches!(req.method, Method::Post | Method::Put | Method::Patch) {
            head.push_str(&format!("Content-Length: {}\r\n", req.body.len()));
        }
        head.push_str("Connection: close\r\nUser-Agent: vieww\r\n\r\n");
        w.write_all(head.as_bytes()).map_err(conn_err)?;
        w.write_all(&req.body).map_err(conn_err)?;
        w.flush().map_err(conn_err)?;
        let mut r = BufReader::new(s);
        let (status_line, headers) = read_headers(&mut r)?;
        let status: u16 = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|c| c.parse().ok())
            .ok_or_else(|| NetError::Connection(format!("bad status line: {status_line:?}")))?;
        let mut body = Vec::new();
        if req.method != Method::Head && !(100..200).contains(&status) && status != 204 && status != 304 {
            if header(&headers, "transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked")) {
                loop {
                    let mut size = String::new();
                    r.read_line(&mut size).map_err(conn_err)?;
                    let n = usize::from_str_radix(size.trim().split(';').next().unwrap_or("0"), 16)
                        .map_err(|_| NetError::Connection("bad chunk size".into()))?;
                    if n == 0 {
                        let mut trailer = String::new();
                        while r.read_line(&mut trailer).map_err(conn_err)? > 2 {
                            trailer.clear();
                        }
                        break;
                    }
                    let mut chunk = vec![0; n];
                    r.read_exact(&mut chunk).map_err(conn_err)?;
                    body.extend(chunk);
                    let mut crlf = [0; 2];
                    r.read_exact(&mut crlf).map_err(conn_err)?;
                }
            } else if let Some(n) = header(&headers, "content-length").and_then(|v| v.parse::<usize>().ok()) {
                body = vec![0; n];
                r.read_exact(&mut body).map_err(conn_err)?;
            } else {
                r.read_to_end(&mut body).map_err(conn_err)?;
            }
        }
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            if let Some(loc) = header(&headers, "location") {
                let next = if loc.contains("://") {
                    Url::parse(loc)?
                } else {
                    Url::parse(&format!("{}://{}:{}{}", req.url.scheme().as_str(), req.url.host(), req.url.port(), loc))?
                };
                req.url = next;
                if status == 303 {
                    req.method = Method::Get;
                    req.body.clear();
                }
                continue;
            }
        }
        return Ok(HttpResponse { status, headers, body });
    }
    Err(NetError::Connection("too many redirects".into()))
}

/// An [`HttpClient`] over [`fetch`], each request on a worker thread.
#[derive(Clone)]
pub struct TcpClient {
    pub timeout: Duration,
    pub redirects: u32,
    waker: Arc<dyn FrameWaker>,
}

impl std::fmt::Debug for TcpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TcpClient").field("timeout", &self.timeout).finish_non_exhaustive()
    }
}

impl Default for TcpClient {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(10),
            redirects: 5,
            waker: Arc::new(NoWaker),
        }
    }
}

impl TcpClient {
    /// Wake frames through `waker` when responses land.
    #[must_use]
    pub fn with_waker(mut self, waker: Arc<dyn FrameWaker>) -> Self {
        self.waker = waker;
        self
    }
}

impl HttpClient for TcpClient {
    fn send(&self, request: HttpRequest) -> Task<HttpResponse, NetError> {
        let (t, n) = (self.timeout, self.redirects);
        Task::spawn(&Threads, self.waker.clone(), move || fetch(&request, t, n))
    }
}

// ───────────────────────────── WebSocket ─────────────────────────────

/// SHA-1 (FIPS 180-4). Used only for the WebSocket handshake.
#[must_use]
pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476, 0xC3D2_E1F0];
    let mut msg = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend(bits.to_be_bytes());
    for block in msg.chunks_exact(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([block[4 * i], block[4 * i + 1], block[4 * i + 2], block[4 * i + 3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let t = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e]) {
            *x = x.wrapping_add(y);
        }
    }
    let mut out = [0u8; 20];
    for (i, v) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

/// Standard base64 with padding.
#[must_use]
pub fn base64(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = u32::from(c[0]) << 16 | u32::from(*c.get(1).unwrap_or(&0)) << 8 | u32::from(*c.get(2).unwrap_or(&0));
        for (k, shift) in [18, 12, 6, 0].iter().enumerate() {
            if k <= c.len() {
                s.push(A[((n >> shift) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

fn accept_key(key: &str) -> String {
    base64(&sha1(format!("{key}{GUID}").as_bytes()))
}

/// A message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Text(String),
    Binary(Vec<u8>),
    /// Close with a status code and reason.
    Close(u16, String),
}

/// An open WebSocket.
#[derive(Debug)]
pub struct WebSocket {
    stream: TcpStream,
    /// Clients mask what they send; servers do not.
    client: bool,
    rng: u64,
}

impl WebSocket {
    /// Connect to `ws://host:port/path` and perform the opening handshake.
    ///
    /// # Errors
    /// Connection failures, `wss`, a non-101 answer or a wrong accept key.
    pub fn connect(url: &str, timeout: Duration) -> Result<Self, NetError> {
        let http = url.replacen("ws://", "http://", 1).replacen("wss://", "https://", 1);
        let u = Url::parse(&http)?;
        let mut s = connect(&u, timeout)?;
        let mut rng = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0x1234_5678, |d| d.as_nanos() as u64)
            | 1;
        let mut key = [0u8; 16];
        for b in &mut key {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            *b = rng as u8;
        }
        let key = base64(&key);
        let req = format!(
            "GET {} HTTP/1.1\r\nHost: {}:{}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n",
            target(&u),
            u.host(),
            u.port()
        );
        s.write_all(req.as_bytes()).map_err(conn_err)?;
        let mut r = BufReader::new(s.try_clone().map_err(conn_err)?);
        let (status, headers) = read_headers(&mut r)?;
        if !status.contains(" 101") {
            return Err(NetError::Connection(format!("handshake refused: {status}")));
        }
        if header(&headers, "sec-websocket-accept") != Some(accept_key(&key).as_str()) {
            return Err(NetError::Connection("Sec-WebSocket-Accept mismatch".into()));
        }
        Ok(Self { stream: s, client: true, rng })
    }

    /// Server side: read a client's handshake from `stream` and answer 101.
    ///
    /// # Errors
    /// A request that is not a WebSocket upgrade.
    pub fn accept(stream: TcpStream) -> Result<Self, NetError> {
        let mut r = BufReader::new(stream.try_clone().map_err(conn_err)?);
        let (_, headers) = read_headers(&mut r)?;
        let key = header(&headers, "sec-websocket-key")
            .ok_or_else(|| NetError::Connection("not a WebSocket upgrade".into()))?
            .to_owned();
        let mut s = stream;
        let resp = format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
            accept_key(&key)
        );
        s.write_all(resp.as_bytes()).map_err(conn_err)?;
        Ok(Self { stream: s, client: false, rng: 0x9E37_79B9 })
    }

    fn write_frame(&mut self, opcode: u8, payload: &[u8]) -> Result<(), NetError> {
        let mut f = vec![0x80 | opcode];
        let mask_bit = if self.client { 0x80 } else { 0 };
        let n = payload.len();
        if n < 126 {
            #[allow(clippy::cast_possible_truncation)]
            f.push(mask_bit | n as u8);
        } else if n <= 0xFFFF {
            f.push(mask_bit | 126);
            #[allow(clippy::cast_possible_truncation)]
            f.extend((n as u16).to_be_bytes());
        } else {
            f.push(mask_bit | 127);
            f.extend((n as u64).to_be_bytes());
        }
        if self.client {
            self.rng ^= self.rng << 13;
            self.rng ^= self.rng >> 7;
            self.rng ^= self.rng << 17;
            #[allow(clippy::cast_possible_truncation)]
            let mask = (self.rng as u32).to_be_bytes();
            f.extend(mask);
            f.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        } else {
            f.extend(payload);
        }
        self.stream.write_all(&f).map_err(conn_err)
    }

    /// Send a message.
    ///
    /// # Errors
    /// The socket failed.
    pub fn send(&mut self, m: &Message) -> Result<(), NetError> {
        match m {
            Message::Text(t) => self.write_frame(0x1, t.as_bytes()),
            Message::Binary(b) => self.write_frame(0x2, b),
            Message::Close(code, why) => {
                let mut p = code.to_be_bytes().to_vec();
                p.extend(why.as_bytes());
                self.write_frame(0x8, &p)
            }
        }
    }

    fn read_frame(&mut self) -> Result<(bool, u8, Vec<u8>), NetError> {
        let mut h = [0u8; 2];
        self.stream.read_exact(&mut h).map_err(conn_err)?;
        let (fin, op, masked) = (h[0] & 0x80 != 0, h[0] & 0x0F, h[1] & 0x80 != 0);
        let mut n = u64::from(h[1] & 0x7F);
        if n == 126 {
            let mut b = [0u8; 2];
            self.stream.read_exact(&mut b).map_err(conn_err)?;
            n = u64::from(u16::from_be_bytes(b));
        } else if n == 127 {
            let mut b = [0u8; 8];
            self.stream.read_exact(&mut b).map_err(conn_err)?;
            n = u64::from_be_bytes(b);
        }
        let mut mask = [0u8; 4];
        if masked {
            self.stream.read_exact(&mut mask).map_err(conn_err)?;
        }
        let mut p = vec![0u8; usize::try_from(n).map_err(conn_err)?];
        self.stream.read_exact(&mut p).map_err(conn_err)?;
        if masked {
            for (i, b) in p.iter_mut().enumerate() {
                *b ^= mask[i % 4];
            }
        }
        Ok((fin, op, p))
    }

    /// Receive the next message (answering pings, reassembling fragments).
    ///
    /// # Errors
    /// The socket failed or a text message was not UTF-8.
    pub fn recv(&mut self) -> Result<Message, NetError> {
        let mut buf = Vec::new();
        let mut kind = 0u8;
        loop {
            let (fin, op, p) = self.read_frame()?;
            match op {
                0x9 => {
                    self.write_frame(0xA, &p)?;
                    continue;
                }
                0xA => continue,
                0x8 => {
                    let code = if p.len() >= 2 { u16::from_be_bytes([p[0], p[1]]) } else { 1005 };
                    let why = String::from_utf8_lossy(p.get(2..).unwrap_or(&[])).into_owned();
                    let _ = self.write_frame(0x8, &p);
                    return Ok(Message::Close(code, why));
                }
                0x0 => buf.extend(p),
                _ => {
                    kind = op;
                    buf = p;
                }
            }
            if fin {
                return Ok(if kind == 0x1 {
                    Message::Text(String::from_utf8(buf).map_err(|_| NetError::Connection("text frame is not UTF-8".into()))?)
                } else {
                    Message::Binary(buf)
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn sha1_and_base64_match_the_rfc_vectors() {
        let hex = |b: [u8; 20]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        assert_eq!(hex(sha1(b"abc")), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(hex(sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        // RFC 6455 §1.3's worked example.
        assert_eq!(accept_key("dGhlIHNhbXBsZSBub25jZQ=="), "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
        assert_eq!(hex(sha1(&[b'a'; 100])), "7f9000257a4918d7072655ea468540cdcbd42e0c", "two blocks");
    }

    fn serve(reply: &'static str) -> u16 {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for s in l.incoming().take(2) {
                let mut s = s.unwrap();
                let mut r = BufReader::new(s.try_clone().unwrap());
                let (line, headers) = read_headers(&mut r).unwrap();
                let n: usize = header(&headers, "content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
                let mut body = vec![0; n];
                r.read_exact(&mut body).unwrap();
                if line.starts_with("GET /old") {
                    s.write_all(b"HTTP/1.1 302 Found\r\nLocation: /new\r\nContent-Length: 0\r\n\r\n").unwrap();
                } else {
                    let msg = format!("{reply}|{line}|{}", String::from_utf8_lossy(&body));
                    let chunked = format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nX-T: y\r\n\r\n{:x}\r\n{msg}\r\n0\r\n\r\n", msg.len());
                    s.write_all(chunked.as_bytes()).unwrap();
                }
            }
        });
        port
    }

    #[test]
    fn http_over_tcp_with_chunked_body_and_redirect() {
        let port = serve("hello");
        let url = Url::parse(&format!("http://127.0.0.1:{port}/old?x=1")).unwrap();
        let r = fetch(&HttpRequest::get(url), Duration::from_secs(5), 3).unwrap();
        assert_eq!(r.status, 200);
        assert_eq!(r.header_value("x-t"), Some("y"));
        let body = String::from_utf8(r.body).unwrap();
        assert!(body.starts_with("hello|GET /new HTTP/1.1|"), "{body}");
    }

    #[test]
    fn the_client_runs_on_a_task() {
        let port = serve("posted");
        let url = Url::parse(&format!("http://127.0.0.1:{port}/p")).unwrap();
        let mut t = TcpClient::default().send(HttpRequest::post(url, b"payload".to_vec()));
        for _ in 0..500 {
            if t.poll() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let r = t.value().ready().expect("answered");
        assert!(String::from_utf8_lossy(&r.body).ends_with("|payload"));
    }

    #[test]
    fn https_is_refused_by_name() {
        let e = fetch(&HttpRequest::get(Url::parse("https://example.test/").unwrap()), Duration::from_secs(1), 0).unwrap_err();
        assert!(e.to_string().contains("TLS"));
    }

    #[test]
    fn websocket_echo_round_trip() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (s, _) = l.accept().unwrap();
            let mut ws = WebSocket::accept(s).unwrap();
            loop {
                match ws.recv().unwrap() {
                    Message::Close(..) => break,
                    m => ws.send(&m).unwrap(),
                }
            }
        });
        let mut ws = WebSocket::connect(&format!("ws://127.0.0.1:{port}/echo"), Duration::from_secs(5)).unwrap();
        ws.send(&Message::Text("héllo".into())).unwrap();
        assert_eq!(ws.recv().unwrap(), Message::Text("héllo".into()));
        let big = vec![7u8; 70_000];
        ws.send(&Message::Binary(big.clone())).unwrap();
        assert_eq!(ws.recv().unwrap(), Message::Binary(big), "64-bit length path");
        ws.send(&Message::Close(1000, "bye".into())).unwrap();
        assert_eq!(ws.recv().unwrap(), Message::Close(1000, "bye".into()));
    }
}
