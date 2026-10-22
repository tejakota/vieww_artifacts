//! HTTPS through the operating system's own `curl`, where Wright has no
//! native TLS binding yet (Windows, macOS — see `tls.rs`).
//!
//! ```text
//!  Request ──config on stdin──▶ curl -q --config -  ──TLS (Schannel / the macOS stack)──▶ server
//!          ◀──raw HTTP response on stdout (--include --raw)──
//! ```
//!
//! * Every Windows 10 (1803+) and macOS ships curl, linked against the
//!   OS's TLS and trust store, so this is still the platform's crypto.
//! * The whole request — URL, headers (API keys included), body — goes to
//!   curl as a config file on **stdin**. Nothing but `-q --config -` is on
//!   the command line, where other processes could read it.
//! * `--raw` keeps the response exactly as sent (chunked framing and
//!   all), so the same head parser and body reader as a direct connection
//!   read it: one code path for streaming, limits and errors.
//! * Wright decides proxying (its `NO_PROXY` rules) and tells curl
//!   explicitly; curl's own environment handling and `~/.curlrc` are off.
//!
//! A failure curl reports (`curl: (60) SSL certificate problem`) surfaces
//! as the read error; a transfer cut short is an error, never a short body
//! taken as complete.

use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStderr, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use wright_core::wire::http::Request;

use crate::net::NetConfig;

/// The curl to run: `WRIGHT_CURL` if set, else the OS's own copy by its
/// full path (not whatever `curl` comes first on `PATH`).
pub fn binary() -> PathBuf {
    if let Some(p) = std::env::var_os("WRIGHT_CURL").filter(|p| !p.is_empty()) {
        return PathBuf::from(p);
    }
    if cfg!(windows) {
        let root = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        root.join("System32").join("curl.exe")
    } else if cfg!(target_os = "macos") {
        PathBuf::from("/usr/bin/curl")
    } else {
        PathBuf::from("curl")
    }
}

/// Is curl there and working? (For `wright doctor` and the transport choice.)
pub fn version() -> Result<String, String> {
    let out = Command::new(binary())
        .args(["-q", "--version"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{}: {e}", binary().display()))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let first = text
        .lines()
        .next()
        .filter(|l| l.starts_with("curl "))
        .ok_or("curl did not report a version")?;
    // "curl 8.5.0" and the TLS library it uses, of the long list it prints.
    let words: Vec<&str> = first.split_whitespace().collect();
    let tls = words.iter().find(|w| {
        ["SSL", "Schannel", "SecureTransport", "TLS"]
            .iter()
            .any(|k| w.contains(k))
    });
    Ok(match tls {
        Some(t) => format!("{} {}, {t}", words[0], words.get(1).unwrap_or(&"")),
        None => words.iter().take(2).copied().collect::<Vec<_>>().join(" "),
    })
}

/// A value in curl's config syntax: double-quoted, `\` `"` and control
/// characters escaped.
fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{b}' => out.push_str("\\v"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Headers curl writes itself from the request it makes.
fn curl_sets(name: &str) -> bool {
    ["host", "content-length", "connection", "transfer-encoding"]
        .contains(&name.to_ascii_lowercase().as_str())
}

/// Where the request body goes in the config: inline when it is text, a
/// private temporary file when it is binary (audio, say) — config files
/// cannot carry NUL bytes, and an inline value starting with `@` would be
/// read as a file name.
pub enum BodyArg {
    None,
    Inline(String),
    File(PathBuf),
}

/// The config text for `req` (pure: tests read it). `body` says how the
/// body is passed.
pub fn config(req: &Request, cfg: &NetConfig, body: &BodyArg) -> String {
    let mut c = String::new();
    let mut line = |k: &str, v: Option<&str>| {
        c.push_str(k);
        if let Some(v) = v {
            c.push_str(" = ");
            c.push_str(&quote(v));
        }
        c.push('\n');
    };
    line("url", Some(&req.url.to_string()));
    line("request", Some(&req.method));
    // Plain HTTP/1.1 out, the raw response back, nothing else on stdout.
    for flag in [
        "http1.1",
        "include",
        "raw",
        "no-buffer",
        "silent",
        "show-error",
        "globoff",
        "suppress-connect-headers",
    ] {
        line(flag, None);
    }
    line("proto", Some("=https,http"));
    for (k, v) in &req.headers {
        if !curl_sets(k) {
            line("header", Some(&format!("{k}: {v}")));
        }
    }
    line("header", Some("Expect:")); // no 100-continue round trip
    match body {
        BodyArg::None => {}
        BodyArg::Inline(text) => line("data-binary", Some(text)),
        BodyArg::File(p) => line("data-binary", Some(&format!("@{}", p.display()))),
    }
    line(
        "connect-timeout",
        Some(&cfg.connect_timeout.as_secs().max(1).to_string()),
    );
    // Abort when less than a byte a second arrives for the I/O timeout.
    line("speed-limit", Some("1"));
    line(
        "speed-time",
        Some(&cfg.io_timeout.as_secs().max(1).to_string()),
    );
    // Extra authorities (`tls ca-file` / WRIGHT_CA_FILE). The OS curl
    // treats this file as THE trust list for the request (Schannel and
    // Secure Transport have no "add to the system store" switch), so on
    // these platforms the file should be the office's full bundle — which
    // is what IT departments hand out. A root IT already installed in
    // the system store needs no file at all here.
    if let Some(ca) = crate::config::ca_file() {
        line("cacert", Some(&ca.to_string_lossy()));
    }
    match cfg.proxy_for(&req.url) {
        Some(p) => {
            line("proxy", Some(&format!("http://{}:{}", p.host, p.port)));
            if let Some(auth) = p.auth() {
                line(
                    "proxy-header",
                    Some(&format!("Proxy-Authorization: {auth}")),
                );
            }
        }
        None => line("noproxy", Some("*")),
    }
    c
}

/// A running curl: its stdout is the raw response.
pub struct CurlConn {
    child: Child,
    stdout: ChildStdout,
    stderr: Option<ChildStderr>,
    temp: Option<PathBuf>,
    finished: bool,
}

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn private_temp(body: &[u8]) -> io::Result<PathBuf> {
    let p = std::env::temp_dir().join(format!(
        "wright-body-{}-{}",
        std::process::id(),
        TEMP_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(&p)?.write_all(body)?;
    Ok(p)
}

/// Start curl for `req`. The response is read from the returned stream.
pub fn start(req: &Request, cfg: &NetConfig) -> Result<CurlConn, String> {
    // Same rule as a direct connection: an invalid header never leaves.
    req.encode(&mut io::sink()).map_err(|e| e.to_string())?;
    let text_body = std::str::from_utf8(&req.body)
        .ok()
        .filter(|t| !t.contains('\0') && !t.starts_with('@'));
    let (body, temp) = match (req.body.is_empty(), text_body) {
        (true, _) => (BodyArg::None, None),
        (false, Some(t)) => (BodyArg::Inline(t.to_string()), None),
        (false, None) => {
            let p = private_temp(&req.body)
                .map_err(|e| format!("temporary file for the request body: {e}"))?;
            (BodyArg::File(p.clone()), Some(p))
        }
    };
    let mut cmd = Command::new(binary());
    cmd.args(["-q", "--config", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flashing up
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("cannot run {} for HTTPS: {e}", binary().display()))?;
    let written = child
        .stdin
        .take()
        .map(|mut stdin| stdin.write_all(config(req, cfg, &body).as_bytes()));
    let stdout = child.stdout.take().ok_or("curl gave no output stream")?;
    let stderr = child.stderr.take();
    let mut conn = CurlConn {
        child,
        stdout,
        stderr,
        temp,
        finished: false,
    };
    if let Some(Err(e)) = written {
        return Err(conn.failure().unwrap_or_else(|| format!("curl: {e}")));
    }
    Ok(conn)
}

impl CurlConn {
    /// Wait for curl; its complaint if it failed.
    fn failure(&mut self) -> Option<String> {
        self.finished = true;
        let status = self.child.wait().ok()?;
        if status.success() {
            return None;
        }
        let mut err = String::new();
        if let Some(mut e) = self.stderr.take() {
            let _ = e.by_ref().take(4096).read_to_string(&mut err);
        }
        let err = err.trim();
        Some(if err.is_empty() {
            format!("curl failed ({status})")
        } else {
            err.to_string()
        })
    }
}

impl Read for CurlConn {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.finished {
            return Ok(0);
        }
        let n = self.stdout.read(buf)?;
        if n == 0 {
            if let Some(msg) = self.failure() {
                return Err(io::Error::other(msg));
            }
        }
        Ok(n)
    }
}

impl Write for CurlConn {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("the request went to curl at start"))
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for CurlConn {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        if let Some(p) = self.temp.take() {
            let _ = std::fs::remove_file(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use wright_core::wire::http::parse_url;

    use crate::net::{self, Proxy};

    fn have_curl() -> bool {
        version().is_ok()
    }

    #[test]
    fn config_quotes_and_keeps_secrets_off_the_command_line() {
        let url = parse_url("https://api.example.com/v1/chat?x=[1]").unwrap();
        let req = Request::new("POST", url)
            .header("Authorization", "Bearer sk-secret")
            .header("Content-Length", "99")
            .json_body(r#"{"a":"say \"hi\"\n"}"#.into());
        let cfg = NetConfig::default();
        let c = config(
            &req,
            &cfg,
            &BodyArg::Inline(String::from_utf8(req.body.clone()).unwrap()),
        );
        assert!(
            c.contains("url = \"https://api.example.com/v1/chat?x=[1]\"\n"),
            "{c}"
        );
        assert!(c.contains("header = \"Authorization: Bearer sk-secret\"\n"));
        assert!(!c.contains("Content-Length"), "curl writes its own");
        assert!(
            c.contains(r#"data-binary = "{\"a\":\"say \\\"hi\\\"\\n\"}""#),
            "{c}"
        );
        assert!(
            c.contains("noproxy = \"*\"\n"),
            "Wright decides proxying, not curl"
        );
        assert!(c.contains("globoff\n") && c.contains("raw\n") && c.contains("include\n"));
        // Proxy with credentials: a header in the config, not a URL.
        let cfg = NetConfig {
            https_proxy: Some(Proxy::parse("http://bob:pw@proxy.corp:3128").unwrap()),
            ..NetConfig::default()
        };
        let c = config(&req, &cfg, &BodyArg::None);
        assert!(c.contains("proxy = \"http://proxy.corp:3128\"\n"), "{c}");
        assert!(c.contains("proxy-header = \"Proxy-Authorization: Basic Ym9iOnB3\"\n"));
        assert!(!c.contains("bob:pw"));
    }

    /// The whole path against a local server: request in through the
    /// config, a chunked streaming answer out through the normal reader.
    #[test]
    fn round_trip_through_curl() {
        if !have_curl() {
            eprintln!("no curl here; skipping");
            return;
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for _ in 0..2 {
                let (mut s, _) = listener.accept().unwrap();
                let mut req = Vec::new();
                let mut buf = [0u8; 65536];
                // Read the head, then the body by Content-Length.
                loop {
                    let n = s.read(&mut buf).unwrap();
                    req.extend_from_slice(&buf[..n]);
                    if let Some(end) = req.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&req[..end]).to_ascii_lowercase();
                        let len: usize = head
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length: "))
                            .and_then(|v| v.trim().parse().ok())
                            .unwrap_or(0);
                        while req.len() < end + 4 + len {
                            let n = s.read(&mut buf).unwrap();
                            req.extend_from_slice(&buf[..n]);
                        }
                        break;
                    }
                }
                s.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n").unwrap();
                for part in ["data: one\n\n", "data: two\n\n"] {
                    s.write_all(format!("{:x}\r\n{part}\r\n", part.len()).as_bytes())
                        .unwrap();
                    s.flush().unwrap();
                }
                s.write_all(b"0\r\n\r\n").unwrap();
                seen.push(req);
            }
            seen
        });
        let url = parse_url(&format!("http://127.0.0.1:{port}/v1/x")).unwrap();
        // Text body, then binary (through a private temporary file).
        let text = Request::new("POST", url.clone())
            .header("Authorization", "Bearer sk-1")
            .json_body("{\"q\":1}".into());
        let binary = Request::new("POST", url)
            .header("Content-Type", "audio/wav")
            .body(vec![0, 159, 146, 150, b'@']);
        for req in [&text, &binary] {
            let conn = start(req, &NetConfig::default()).unwrap();
            let mut br = std::io::BufReader::new(net::Conn::Curl(Box::new(conn)));
            let head = wright_core::wire::http::read_head(&mut br, &Default::default()).unwrap();
            assert_eq!(head.status, 200);
            let mut body = String::new();
            wright_core::wire::http::BodyReader::new(br, &head, &Default::default())
                .unwrap()
                .read_to_string(&mut body)
                .unwrap();
            assert_eq!(
                body, "data: one\n\ndata: two\n\n",
                "chunked framing decoded by Wright's reader"
            );
        }
        let seen = server.join().unwrap();
        let first = String::from_utf8_lossy(&seen[0]);
        assert!(first.starts_with("POST /v1/x HTTP/1.1\r\n"), "{first}");
        assert!(
            first.contains("Authorization: Bearer sk-1\r\n")
                && first.ends_with("\r\n\r\n{\"q\":1}"),
            "{first}"
        );
        assert!(
            seen[1].ends_with(&[0, 159, 146, 150, b'@']),
            "binary body intact"
        );
        assert!(
            !std::fs::read_dir(std::env::temp_dir())
                .unwrap()
                .flatten()
                .any(|e| e
                    .file_name()
                    .to_string_lossy()
                    .starts_with(&format!("wright-body-{}-", std::process::id()))),
            "temporary body files are removed"
        );
    }

    #[test]
    fn curl_errors_surface() {
        if !have_curl() {
            return;
        }
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port(); // closed again at once
        let req = Request::new(
            "GET",
            parse_url(&format!("http://127.0.0.1:{port}/")).unwrap(),
        );
        let conn = start(&req, &NetConfig::default()).unwrap();
        let err = wright_core::wire::http::decode_response(&mut net::Conn::Curl(Box::new(conn)))
            .unwrap_err();
        assert!(err.contains("curl: (7)"), "{err}");
    }
}
