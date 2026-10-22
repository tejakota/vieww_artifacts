//! The built-in models: two small open models Wright runs itself, so a
//! project works with no API key, no account and no network after the
//! first download.
//!
//! ```text
//!  qwen3-1.7b    text + tool calls   Qwen3-1.7B, 8-bit         1.83 GB
//!  whisper-base  speech to text      Whisper base, 99 langs    148 MB
//! ```
//!
//! * Files live in `~/.wright/models/`, fetched on first use (or with
//!   `wright model pull`), resumable, and checked against the SHA-256
//!   pinned here before they are ever loaded. A file that does not match
//!   is deleted, never used.
//! * The inference itself is `wright-infer` (std-only, in this repo). The
//!   runtime does not depend on it: the app installs it as the
//!   [`LocalBackend`] at startup, and the runtime calls through the trait.
//! * In config the built-ins are an ordinary provider of kind `builtin`,
//!   last in the list: any configured provider for a model wins, and
//!   `builtin fallback=false` stops Wright from falling back to them.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use wright_core::sha256::Sha256;
use wright_core::wire::http::{parse_url, Request};

use crate::config::wright_home;
use crate::models::{CompletionRequest, CompletionResponse, Delta};
use crate::net::{self, NetConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Speech,
}

/// One built-in model: what it is and exactly which file it is.
#[derive(Debug, Clone, Copy)]
pub struct BuiltinModel {
    pub id: &'static str,
    pub kind: Kind,
    pub about: &'static str,
    pub file: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size: u64,
    pub license: &'static str,
}

/// The catalog. The hashes are the files' Git LFS object ids on Hugging
/// Face (`oid sha256:` of each file), pinned: a changed upstream file is
/// refused, not silently used.
pub const CATALOG: [BuiltinModel; 2] = [
    BuiltinModel {
        id: "qwen3-1.7b",
        kind: Kind::Text,
        about: "Qwen3 1.7B (8-bit): writes, plans and calls tools. Good for drafts, small edits and keeping a project moving without a key.",
        file: "Qwen3-1.7B-Q8_0.gguf",
        url: "https://huggingface.co/Qwen/Qwen3-1.7B-GGUF/resolve/main/Qwen3-1.7B-Q8_0.gguf",
        sha256: "061b54daade076b5d3362dac252678d17da8c68f07560be70818cace6590cb1a",
        size: 1_834_426_016,
        license: "Apache-2.0",
    },
    BuiltinModel {
        id: "whisper-base",
        kind: Kind::Speech,
        about: "Whisper base: speech to text in 99 languages, on this machine. Voice input never leaves it.",
        file: "ggml-base.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
        sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
        size: 147_951_465,
        license: "MIT",
    },
];

pub const TEXT_MODEL: &str = "qwen3-1.7b";
pub const SPEECH_MODEL: &str = "whisper-base";

pub fn find(id: &str) -> Option<&'static BuiltinModel> {
    CATALOG.iter().find(|m| m.id == id)
}

pub fn models_dir() -> PathBuf {
    wright_home().join("models")
}

impl BuiltinModel {
    pub fn path(&self) -> PathBuf {
        self.path_in(&models_dir())
    }

    fn path_in(&self, dir: &Path) -> PathBuf {
        dir.join(self.file)
    }

    fn part_in(&self, dir: &Path) -> PathBuf {
        dir.join(format!("{}.part", self.file))
    }

    fn marker_in(&self, dir: &Path) -> PathBuf {
        dir.join(format!("{}.sha256", self.file))
    }

    /// Downloaded and verified. (Verification happens once, at download;
    /// the marker records it next to a file of the right size.)
    pub fn installed(&self) -> bool {
        self.installed_in(&models_dir())
    }

    fn installed_in(&self, dir: &Path) -> bool {
        let size_ok = std::fs::metadata(self.path_in(dir))
            .map(|m| m.len() == self.size)
            .unwrap_or(false);
        size_ok
            && std::fs::read_to_string(self.marker_in(dir))
                .map(|h| h.trim() == self.sha256)
                .unwrap_or(false)
    }

    /// Bytes of a download in progress (to resume from).
    pub fn partial(&self) -> u64 {
        self.partial_in(&models_dir())
    }

    fn partial_in(&self, dir: &Path) -> u64 {
        std::fs::metadata(self.part_in(dir))
            .map(|m| m.len())
            .unwrap_or(0)
    }

    pub fn size_text(&self) -> String {
        human_bytes(self.size)
    }
}

pub fn human_bytes(n: u64) -> String {
    match n {
        n if n >= 1_000_000_000 => format!("{:.2} GB", n as f64 / 1e9),
        n if n >= 1_000_000 => format!("{:.0} MB", n as f64 / 1e6),
        n => format!("{:.0} KB", n as f64 / 1e3),
    }
}

/// Where a download is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Downloading,
    Checking,
}

#[derive(Debug, Clone, Copy)]
pub struct Progress {
    pub phase: Phase,
    pub done: u64,
    pub total: u64,
}

impl Progress {
    pub fn percent(&self) -> u32 {
        (self.done.min(self.total) * 100 / self.total.max(1)) as u32
    }
}

/// One download at a time per process; across processes, a lock file.
static PULLING: Mutex<()> = Mutex::new(());

struct DirLock(PathBuf);

impl DirLock {
    fn take(m: &BuiltinModel, dir: &Path) -> Result<DirLock, String> {
        let p = dir.join(format!("{}.lock", m.file));
        // A lock older than a day is from a Wright that died mid-download.
        if let Ok(meta) = std::fs::metadata(&p) {
            let age = meta
                .modified()
                .ok()
                .and_then(|t| SystemTime::now().duration_since(t).ok())
                .unwrap_or_default();
            if age > Duration::from_secs(24 * 3600) {
                let _ = std::fs::remove_file(&p);
            }
        }
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&p)
        {
            Ok(mut f) => {
                let _ = writeln!(f, "{}", std::process::id());
                Ok(DirLock(p))
            }
            Err(_) => Err(format!(
                "another Wright is downloading {} (remove {} if not)",
                m.id,
                p.display()
            )),
        }
    }
}

impl Drop for DirLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// The URL to fetch from: `WRIGHT_MODEL_MIRROR` (a base URL serving the
/// same file names — for networks that cannot reach Hugging Face) or the
/// catalog's.
fn source_url(m: &BuiltinModel) -> String {
    match std::env::var("WRIGHT_MODEL_MIRROR") {
        Ok(base) if !base.trim().is_empty() => {
            format!("{}/{}", base.trim().trim_end_matches('/'), m.file)
        }
        _ => m.url.to_string(),
    }
}

/// Download (or finish downloading) `m`, verify it, and return its path.
/// Already installed: returns at once.
pub fn pull(m: &BuiltinModel, on_progress: &mut dyn FnMut(Progress)) -> Result<PathBuf, String> {
    pull_into(m, &models_dir(), &source_url(m), on_progress)
}

fn pull_into(
    m: &BuiltinModel,
    dir: &Path,
    from: &str,
    on_progress: &mut dyn FnMut(Progress),
) -> Result<PathBuf, String> {
    let _one = PULLING.lock().unwrap_or_else(|p| p.into_inner());
    if m.installed_in(dir) {
        return Ok(m.path_in(dir));
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let _lock = DirLock::take(m, dir)?;
    let (part, path, marker) = (m.part_in(dir), m.path_in(dir), m.marker_in(dir));
    let mut cfg = NetConfig::from_env();
    cfg.limits.max_body = usize::try_from(m.size)
        .unwrap_or(usize::MAX)
        .saturating_add(1 << 20);
    cfg.io_timeout = Duration::from_secs(60);
    let mut have = m.partial_in(dir);
    if have > m.size {
        let _ = std::fs::remove_file(&part);
        have = 0;
    }
    let mut url = from.to_string();
    // Follow redirects ourselves (Hugging Face answers with one to its
    // CDN), a few hops, never from HTTPS down to plain HTTP.
    let mut stream = None;
    for _hop in 0..6 {
        let mut req = Request::new("GET", parse_url(&url)?)
            .header("User-Agent", concat!("wright/", env!("CARGO_PKG_VERSION")));
        if have > 0 {
            req = req.header("Range", &format!("bytes={have}-"));
        }
        let s =
            net::send_streaming(&req, &cfg).map_err(|e| format!("downloading {}: {e}", m.id))?;
        match s.head.status {
            301 | 302 | 303 | 307 | 308 => {
                let next = s
                    .head
                    .header("location")
                    .ok_or("a redirect without a location")?
                    .to_string();
                let here = parse_url(&url)?;
                if next.starts_with('/') {
                    url = format!(
                        "{}://{}{next}",
                        if here.is_tls() { "https" } else { "http" },
                        here.authority()
                    );
                } else {
                    if here.is_tls() && !next.starts_with("https://") {
                        return Err(format!(
                            "{} redirected from HTTPS to '{next}'; refusing",
                            m.id
                        ));
                    }
                    url = next;
                }
            }
            206 if have > 0 => {
                stream = Some(s);
                break;
            }
            200 => {
                have = 0; // the server ignored the range: start over
                stream = Some(s);
                break;
            }
            416 if have == m.size => break, // already complete; just check it
            status => {
                return Err(format!(
                    "downloading {}: HTTP {status} from {}",
                    m.id,
                    url.split('?').next().unwrap_or("")
                ))
            }
        }
    }
    if let Some(mut s) = stream {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(have > 0)
            .truncate(have == 0)
            .open(&part)
            .map_err(|e| format!("{}: {e}", part.display()))?;
        let mut buf = vec![0u8; 1 << 20];
        let mut last = 0u64;
        on_progress(Progress {
            phase: Phase::Downloading,
            done: have,
            total: m.size,
        });
        loop {
            let n = s
                .body
                .read(&mut buf)
                .map_err(|e| format!("downloading {}: {e} (run again to resume)", m.id))?;
            if n == 0 {
                break;
            }
            f.write_all(&buf[..n])
                .map_err(|e| format!("{}: {e}", part.display()))?;
            have += n as u64;
            if have > m.size {
                drop(f);
                let _ = std::fs::remove_file(&part);
                return Err(format!(
                    "{} is larger than the pinned file; refusing it",
                    m.id
                ));
            }
            if have - last >= 8 << 20 || have == m.size {
                last = have;
                on_progress(Progress {
                    phase: Phase::Downloading,
                    done: have,
                    total: m.size,
                });
            }
        }
        f.sync_all()
            .map_err(|e| format!("{}: {e}", part.display()))?;
    } else if have != m.size {
        return Err(format!("downloading {}: too many redirects", m.id));
    }
    if have != m.size {
        return Err(format!(
            "{}: got {} of {} (run again to resume)",
            m.id,
            human_bytes(have),
            m.size_text()
        ));
    }
    // Check every byte against the pinned hash before it may be used.
    let got = hash_file(&part, &mut |done| {
        on_progress(Progress {
            phase: Phase::Checking,
            done,
            total: m.size,
        })
    })?;
    if got != m.sha256 {
        let _ = std::fs::remove_file(&part);
        return Err(format!("{} failed its checksum (got {got}); the download was deleted. Try again, or check WRIGHT_MODEL_MIRROR", m.id));
    }
    std::fs::rename(&part, &path).map_err(|e| format!("{}: {e}", path.display()))?;
    std::fs::write(&marker, format!("{}\n", m.sha256))
        .map_err(|e| format!("{}: {e}", marker.display()))?;
    Ok(path)
}

fn hash_file(p: &Path, on_progress: &mut dyn FnMut(u64)) -> Result<String, String> {
    let mut f = std::fs::File::open(p).map_err(|e| format!("{}: {e}", p.display()))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 4 << 20];
    let mut done = 0u64;
    loop {
        let n = f
            .read(&mut buf)
            .map_err(|e| format!("{}: {e}", p.display()))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        done += n as u64;
        if done % (64 << 20) < n as u64 {
            on_progress(done);
        }
    }
    on_progress(done);
    Ok(h.finish_hex())
}

/// Delete a model's file (and any partial download).
pub fn remove(m: &BuiltinModel) -> Result<(), String> {
    remove_in(m, &models_dir())
}

fn remove_in(m: &BuiltinModel, dir: &Path) -> Result<(), String> {
    for p in [m.path_in(dir), m.part_in(dir), m.marker_in(dir)] {
        match std::fs::remove_file(&p) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{}: {e}", p.display())),
        }
    }
    Ok(())
}

// ── the backend the app installs ────────────────────────────────

/// What runs the built-in models (`wright-infer`, installed by the app).
pub trait LocalBackend: Send + Sync {
    /// A completion from `req.model`. `on_delta` gets text as it is
    /// written, and [`Delta::Status`] lines (a first-use download, model
    /// loading) worth showing.
    fn complete(
        &self,
        req: &CompletionRequest,
        on_delta: &mut dyn FnMut(&Delta),
    ) -> Result<CompletionResponse, String>;
    /// Speech (WAV bytes) to text.
    fn transcribe(&self, model: &str, wav: &[u8]) -> Result<String, String>;
}

static BACKEND: OnceLock<Box<dyn LocalBackend>> = OnceLock::new();

/// Install the backend (once per process; later calls are ignored).
pub fn install(b: Box<dyn LocalBackend>) {
    let _ = BACKEND.set(b);
}

pub fn backend() -> Option<&'static dyn LocalBackend> {
    BACKEND.get().map(|b| b.as_ref())
}

/// Is the built-in engine in this build? (A Wright built without it can
/// still use cloud models.)
pub fn available() -> bool {
    backend().is_some()
}

pub fn complete(
    req: &CompletionRequest,
    on_delta: &mut dyn FnMut(&Delta),
) -> Result<CompletionResponse, String> {
    backend()
        .ok_or("this build of Wright has no built-in model engine")?
        .complete(req, on_delta)
}

pub fn transcribe(model: &str, wav: &[u8]) -> Result<String, String> {
    backend()
        .ok_or("this build of Wright has no built-in model engine")?
        .transcribe(model, wav)
}

/// A status line for a download in progress: "Downloading qwen3-1.7b
/// (first use): 42% of 1.83 GB".
pub fn progress_line(m: &BuiltinModel, p: Progress) -> String {
    match p.phase {
        Phase::Downloading => format!(
            "Downloading the built-in model {} (first use, once): {}% of {}",
            m.id,
            p.percent(),
            m.size_text()
        ),
        Phase::Checking => format!(
            "Checking {} against its pinned checksum: {}%",
            m.id,
            p.percent()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn the_catalog_is_well_formed() {
        for m in &CATALOG {
            assert_eq!(m.sha256.len(), 64);
            assert!(m
                .sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
            assert!(m.url.starts_with("https://") && m.url.ends_with(m.file));
            assert!(m.size > 1_000_000);
        }
        assert_eq!(find(TEXT_MODEL).unwrap().kind, Kind::Text);
        assert_eq!(find(SPEECH_MODEL).unwrap().kind, Kind::Speech);
        assert_eq!(human_bytes(1_834_426_016), "1.83 GB");
        assert_eq!(human_bytes(147_951_465), "148 MB");
    }

    /// A model served by a local server: a redirect, a cut-off first
    /// attempt, a resumed second one, the checksum, and a bad file refused.
    #[test]
    fn downloads_resume_verify_and_refuse_bad_files() {
        let home = std::env::temp_dir().join(format!("wright-local-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let content: Vec<u8> = (0..3_000_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let sha: &'static str = Box::leak(wright_core::sha256::hex(&content).into_boxed_str());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let served = content.clone();
        std::thread::spawn(move || {
            let mut cut_once = true;
            for s in listener.incoming().flatten() {
                let mut s = s;
                let mut buf = [0u8; 4096];
                let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                if req.starts_with("GET /redirect") {
                    let _ = s.write_all(
                        b"HTTP/1.1 302 Found\r\nLocation: /m.bin\r\nContent-Length: 0\r\n\r\n",
                    );
                    continue;
                }
                let from: usize = req
                    .lines()
                    .find_map(|l| l.strip_prefix("Range: bytes="))
                    .and_then(|r| r.trim_end_matches('-').parse().ok())
                    .unwrap_or(0);
                let body = &served[from..];
                let head = if from > 0 {
                    format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\n\r\n",
                        body.len()
                    )
                } else {
                    format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len())
                };
                let _ = s.write_all(head.as_bytes());
                if cut_once {
                    cut_once = false;
                    let _ = s.write_all(&body[..1_000_000]); // then hang up
                    continue;
                }
                let _ = s.write_all(body);
            }
        });
        let from = format!("http://127.0.0.1:{port}/redirect-to/m.bin");
        let m = BuiltinModel {
            id: "test-model",
            kind: Kind::Text,
            about: "",
            file: "m.bin",
            url: "https://example.invalid/m.bin",
            sha256: sha,
            size: content.len() as u64,
            license: "",
        };
        let first = pull_into(&m, &home, &from, &mut |_| {});
        assert!(first.is_err(), "the connection was cut: {first:?}");
        assert_eq!(
            m.partial_in(&home),
            1_000_000,
            "the first million bytes are kept"
        );
        let mut phases = Vec::new();
        let path = pull_into(&m, &home, &from, &mut |p| phases.push(p.phase)).unwrap();
        assert!(m.installed_in(&home));
        assert_eq!(std::fs::read(&path).unwrap(), content);
        assert!(phases.contains(&Phase::Downloading) && phases.contains(&Phase::Checking));
        // Installed: no network at all.
        assert!(
            pull_into(&m, &home, "http://127.0.0.1:1/never", &mut |_| panic!(
                "no progress for an installed model"
            ))
            .is_ok()
        );
        remove_in(&m, &home).unwrap();
        assert!(!m.installed_in(&home) && !path.exists());
        // A file that does not match its pin is deleted, not installed.
        let bad = BuiltinModel {
            sha256: "0000000000000000000000000000000000000000000000000000000000000000",
            ..m
        };
        let e = pull_into(&bad, &home, &from, &mut |_| {}).unwrap_err();
        assert!(e.contains("checksum"), "{e}");
        assert!(!bad.installed_in(&home) && bad.partial_in(&home) == 0);
        let _ = std::fs::remove_dir_all(&home);
    }
}
