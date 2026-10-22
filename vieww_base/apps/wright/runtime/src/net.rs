//! The network client: TCP, TLS, proxies, timeouts. One small file.
//!
//! Everything here composes the pure framing in `wright_core::wire::http`
//! with real sockets:
//!
//! * plain TCP for `http://` (local model gateways, the mock provider);
//! * OS-native TLS for `https://` (see `tls.rs`) — never a downgrade;
//!   where Wright has no native binding yet (Windows, macOS), the OS's own
//!   `curl` carries the request (see `curl.rs`), on the same TLS stack;
//! * corporate proxies: `HTTPS_PROXY` / `HTTP_PROXY` (CONNECT tunnels for
//!   https, absolute-form requests for http), `NO_PROXY` with exact
//!   hosts, domain suffixes, `*`, and IPv4 CIDR ranges — the shapes
//!   enterprise networks actually use;
//! * bounded everything: connect timeout, I/O timeout, head and body
//!   limits (from the wire layer).
//!
//! The streaming entry point ([`send_streaming`]) hands back a body
//! reader the provider layer feeds into the SSE parser as bytes arrive.

use std::io::{BufReader, Read, Write};
use std::net::{IpAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use wright_core::wire::http::{self, BodyReader, Head, Limits, Request, Response, Url};

use crate::curl::{self, CurlConn};
use crate::tls::TlsStream;

/// A connected stream: plain, TLS, or a running curl (whose output is the
/// raw response; the request went in when it started).
pub enum Conn {
    Tcp(TcpStream),
    Tls(Box<TlsStream>),
    Curl(Box<CurlConn>),
}

impl Read for Conn {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Conn::Tcp(s) => s.read(buf),
            Conn::Tls(s) => s.read(buf),
            Conn::Curl(s) => s.read(buf),
        }
    }
}

impl Write for Conn {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Conn::Tcp(s) => s.write(buf),
            Conn::Tls(s) => s.write(buf),
            Conn::Curl(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Conn::Tcp(s) => s.flush(),
            Conn::Tls(s) => s.flush(),
            Conn::Curl(s) => s.flush(),
        }
    }
}

/// A forward proxy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proxy {
    pub host: String,
    pub port: u16,
    /// `Proxy-Authorization` value, if the proxy URL carried credentials.
    /// Never logged.
    auth: Option<String>,
}

impl Proxy {
    /// Parse `http://[user:pass@]host:port`.
    pub fn parse(s: &str) -> Result<Proxy, String> {
        let rest = s
            .strip_prefix("http://")
            .or_else(|| s.strip_prefix("https://"))
            .unwrap_or(s);
        let rest = rest.trim_end_matches('/');
        let (auth, hostport) = match rest.rsplit_once('@') {
            Some((cred, hp)) => (Some(format!("Basic {}", base64(cred.as_bytes()))), hp),
            None => (None, rest),
        };
        let (host, port) = match hostport.rsplit_once(':') {
            Some((h, p)) => (
                h.trim_matches(['[', ']']).to_string(),
                p.parse().map_err(|_| format!("bad proxy port in '{s}'"))?,
            ),
            None => (hostport.to_string(), 80),
        };
        if host.is_empty() {
            return Err(format!("bad proxy '{s}'"));
        }
        Ok(Proxy { host, port, auth })
    }

    /// The `Proxy-Authorization` value (for handing to curl).
    pub fn auth(&self) -> Option<&str> {
        self.auth.as_deref()
    }
}

/// Standard base64 (RFC 4648) — for proxy Basic auth only.
fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Network settings for one client.
#[derive(Debug, Clone)]
pub struct NetConfig {
    pub connect_timeout: Duration,
    pub io_timeout: Duration,
    pub https_proxy: Option<Proxy>,
    pub http_proxy: Option<Proxy>,
    pub no_proxy: Vec<String>,
    pub limits: Limits,
}

impl Default for NetConfig {
    fn default() -> Self {
        NetConfig {
            connect_timeout: Duration::from_secs(10),
            io_timeout: Duration::from_secs(120),
            https_proxy: None,
            http_proxy: None,
            no_proxy: Vec::new(),
            limits: Limits::default(),
        }
    }
}

fn env_any(names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|n| std::env::var(n).ok())
        .filter(|v| !v.trim().is_empty())
}

impl NetConfig {
    /// Read proxy settings from the standard environment variables.
    pub fn from_env() -> Self {
        NetConfig {
            https_proxy: env_any(&["HTTPS_PROXY", "https_proxy"])
                .and_then(|p| Proxy::parse(&p).ok()),
            http_proxy: env_any(&["HTTP_PROXY", "http_proxy"]).and_then(|p| Proxy::parse(&p).ok()),
            no_proxy: env_any(&["NO_PROXY", "no_proxy"])
                .map(|v| {
                    v.split(',')
                        .map(|s| s.trim().to_ascii_lowercase())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            ..NetConfig::default()
        }
    }

    /// The proxy to use for `url`, if any.
    pub fn proxy_for(&self, url: &Url) -> Option<&Proxy> {
        if self.bypasses(&url.host) {
            return None;
        }
        if url.is_tls() {
            self.https_proxy.as_ref()
        } else {
            self.http_proxy.as_ref()
        }
    }

    /// NO_PROXY matching: `*`, exact host, `.suffix` / `*.suffix`,
    /// bare domain (matches subdomains too), IPv4 CIDR.
    pub fn bypasses(&self, host: &str) -> bool {
        let host = host.to_ascii_lowercase();
        let ip: Option<IpAddr> = host.parse().ok();
        self.no_proxy.iter().any(|rule| {
            if rule == "*" || *rule == host {
                return true;
            }
            if let Some((net, bits)) = rule.split_once('/') {
                if let (Some(IpAddr::V4(h)), Ok(IpAddr::V4(n)), Ok(bits)) =
                    (ip, net.parse::<IpAddr>(), bits.parse::<u32>())
                {
                    if bits <= 32 {
                        let mask = if bits == 0 {
                            0
                        } else {
                            u32::MAX << (32 - bits)
                        };
                        return u32::from(h) & mask == u32::from(n) & mask;
                    }
                }
                return false;
            }
            let suffix = rule.trim_start_matches('*').trim_start_matches('.');
            !suffix.is_empty()
                && ip.is_none()
                && (host == suffix || host.ends_with(&format!(".{suffix}")))
        })
    }
}

fn tcp_connect(host: &str, port: u16, cfg: &NetConfig) -> Result<TcpStream, String> {
    let addrs: Vec<_> = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("cannot resolve {host}: {e}"))?
        .collect();
    let mut last = format!("no addresses for {host}");
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, cfg.connect_timeout) {
            Ok(s) => {
                s.set_read_timeout(Some(cfg.io_timeout)).ok();
                s.set_write_timeout(Some(cfg.io_timeout)).ok();
                s.set_nodelay(true).ok();
                return Ok(s);
            }
            Err(e) => last = format!("connect {host}:{port}: {e}"),
        }
    }
    Err(last)
}

/// Open a CONNECT tunnel through `proxy` to `url`'s authority.
fn tunnel(proxy: &Proxy, url: &Url, cfg: &NetConfig) -> Result<TcpStream, String> {
    let mut s = tcp_connect(&proxy.host, proxy.port, cfg)?;
    let mut req = format!("CONNECT {a} HTTP/1.1\r\nHost: {a}\r\n", a = url.authority());
    if let Some(auth) = &proxy.auth {
        req.push_str(&format!("Proxy-Authorization: {auth}\r\n"));
    }
    req.push_str("\r\n");
    s.write_all(req.as_bytes())
        .map_err(|e| format!("proxy CONNECT: {e}"))?;
    // Read the proxy's answer byte-by-byte up to the blank line, so not
    // one byte of the tunnelled TLS stream is swallowed by a buffer.
    let mut head = Vec::new();
    let mut b = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() > 16 * 1024 {
            return Err("proxy CONNECT response too large".into());
        }
        match s.read(&mut b) {
            Ok(0) => return Err("proxy closed the connection during CONNECT".into()),
            Ok(_) => head.push(b[0]),
            Err(e) => return Err(format!("proxy CONNECT: {e}")),
        }
    }
    let status_line = String::from_utf8_lossy(&head);
    let status = status_line.split_whitespace().nth(1).unwrap_or("");
    if status != "200" {
        let first = status_line.lines().next().unwrap_or("").to_string();
        return Err(format!(
            "the proxy refused the connection to {} ({first})",
            url.authority()
        ));
    }
    Ok(s)
}

/// Connect to `url`, through a proxy if configured. Returns the stream
/// and whether requests must use absolute-form (plain http via proxy).
pub fn connect(url: &Url, cfg: &NetConfig) -> Result<(Conn, bool), String> {
    match (url.is_tls(), cfg.proxy_for(url)) {
        (true, Some(p)) => {
            let tcp = tunnel(p, url, cfg)?;
            Ok((
                Conn::Tls(Box::new(TlsStream::connect(tcp, &url.host)?)),
                false,
            ))
        }
        (true, None) => {
            let tcp = tcp_connect(&url.host, url.port, cfg)?;
            Ok((
                Conn::Tls(Box::new(TlsStream::connect(tcp, &url.host)?)),
                false,
            ))
        }
        (false, Some(p)) => Ok((Conn::Tcp(tcp_connect(&p.host, p.port, cfg)?), true)),
        (false, None) => Ok((Conn::Tcp(tcp_connect(&url.host, url.port, cfg)?), false)),
    }
}

fn write_request(conn: &mut Conn, req: &Request, absolute: bool) -> Result<(), String> {
    if absolute {
        let mut r = req.clone();
        r.url.path = req.url.to_string();
        r.encode(conn).map_err(|e| format!("send: {e}"))
    } else {
        req.encode(conn).map_err(|e| format!("send: {e}"))
    }
}

/// How `https://` is carried on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// Wright's own binding to the OS TLS library.
    Native,
    /// The OS's `curl`.
    Curl,
}

/// Native TLS where it is wired and loads; the OS's curl otherwise.
/// `WRIGHT_TLS=curl` or `=native` forces one (for tests, or a machine
/// whose OpenSSL is broken).
pub fn https_transport() -> Transport {
    match std::env::var("WRIGHT_TLS").as_deref() {
        Ok("curl") => Transport::Curl,
        Ok("native") => Transport::Native,
        _ if TlsStream::backend().is_ok() => Transport::Native,
        _ => Transport::Curl,
    }
}

/// Connect and send `req`; the response is read from what comes back.
fn open(req: &Request, cfg: &NetConfig) -> Result<Conn, String> {
    if req.url.is_tls() && https_transport() == Transport::Curl {
        return curl::start(req, cfg).map(|c| Conn::Curl(Box::new(c)));
    }
    let (mut conn, absolute) = connect(&req.url, cfg)?;
    write_request(&mut conn, req, absolute)?;
    Ok(conn)
}

/// Send a request and read the whole response.
pub fn send(req: &Request, cfg: &NetConfig) -> Result<Response, String> {
    let mut conn = open(req, cfg)?;
    http::decode_response_with(&mut conn, &cfg.limits)
}

/// A response whose body is still arriving.
pub struct Streaming {
    pub head: Head,
    pub body: BodyReader<BufReader<Conn>>,
}

/// Send a request and return as soon as the head arrives; read the
/// body incrementally from `Streaming::body`.
pub fn send_streaming(req: &Request, cfg: &NetConfig) -> Result<Streaming, String> {
    let conn = open(req, cfg)?;
    let mut br = BufReader::with_capacity(16 * 1024, conn);
    let head = http::read_head(&mut br, &cfg.limits)?;
    let body = BodyReader::new(br, &head, &cfg.limits)?;
    Ok(Streaming { head, body })
}

/// What carries HTTPS here, for `wright doctor`.
pub fn https_backend() -> Result<String, String> {
    match https_transport() {
        Transport::Native => TlsStream::backend(),
        Transport::Curl => curl::version().map(|v| format!("{v} (the system curl)")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use wright_core::wire::http::parse_url;

    #[test]
    fn base64_matches_rfc4648() {
        assert_eq!(base64(b"user:pass"), "dXNlcjpwYXNz");
        assert_eq!(base64(b"a"), "YQ==");
        assert_eq!(base64(b"ab"), "YWI=");
    }

    #[test]
    fn proxy_urls_parse_with_hidden_credentials() {
        let p = Proxy::parse("http://alice:s3cret@proxy.corp:3128").unwrap();
        assert_eq!((p.host.as_str(), p.port), ("proxy.corp", 3128));
        assert!(!format!("{:?}", p.host).contains("s3cret"));
        assert_eq!(p.auth.as_deref(), Some("Basic YWxpY2U6czNjcmV0"));
    }

    #[test]
    fn no_proxy_rules() {
        let cfg = NetConfig {
            no_proxy: vec![
                "localhost".into(),
                ".internal.corp".into(),
                "10.0.0.0/8".into(),
                "api.anthropic.com".into(),
            ],
            https_proxy: Some(Proxy::parse("http://p:1").unwrap()),
            ..NetConfig::default()
        };
        assert!(cfg.bypasses("localhost"));
        assert!(cfg.bypasses("git.internal.corp"));
        assert!(cfg.bypasses("10.1.2.3"));
        assert!(!cfg.bypasses("11.1.2.3"));
        assert!(cfg.bypasses("api.anthropic.com"));
        assert!(!cfg.bypasses("api.openai.com"));
        let u = parse_url("https://api.openai.com/").unwrap();
        assert!(cfg.proxy_for(&u).is_some());
    }

    #[test]
    fn plain_http_round_trip_against_a_local_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let n = s.read(&mut buf).unwrap();
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                .unwrap();
            req
        });
        let url = parse_url(&format!("http://127.0.0.1:{port}/ping")).unwrap();
        let resp = send(&Request::new("GET", url), &NetConfig::default()).unwrap();
        assert_eq!((resp.status, resp.body_text().as_str()), (200, "ok"));
        assert!(server.join().unwrap().starts_with("GET /ping HTTP/1.1"));
    }

    /// Live check through the real network: one host reached directly
    /// and one through this machine's proxy (if any). TLS verification
    /// is on; any genuine HTTP status proves the handshake, the tunnel
    /// and the framing. Ignored by default: `cargo test -- --ignored live_`
    /// (with `WRIGHT_TLS=curl` to check the curl transport the same way).
    #[test]
    #[ignore]
    fn live_https_direct_and_proxied() {
        let cfg = NetConfig::from_env();
        eprintln!("https via {:?}: {:?}", https_transport(), https_backend());
        for u in [
            "https://index.crates.io/config.json",
            "https://api.github.com/repos/tejakota/vieww_artifacts",
        ] {
            let url = parse_url(u).unwrap();
            let via = if cfg.proxy_for(&url).is_some() {
                "proxy"
            } else {
                "direct"
            };
            let req = Request::new("GET", url).header("User-Agent", "wright-live-test");
            let resp = send(&req, &cfg).unwrap_or_else(|e| panic!("{u}: {e}"));
            eprintln!(
                "{u} via {via}: HTTP {} ({} bytes)",
                resp.status,
                resp.body.len()
            );
            assert!((100..600).contains(&resp.status));
        }
    }
}
