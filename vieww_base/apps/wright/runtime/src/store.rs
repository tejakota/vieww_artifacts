//! The project store: everything Wright remembers about a project, on
//! disk, OUTSIDE the project, under `~/.wright/projects/<name>-<id>/`.
//!
//! Why outside (v0.2 decision): agents may run sandboxed shell commands
//! that can write anywhere inside the project, and the kernel sandbox
//! (Landlock) cannot carve a read-only hole out of a writable tree. If
//! the store lived in `<project>/.wright/`, an agent could forge a cache
//! entry or rewrite the event log. Outside, it is simply invisible to
//! them. `wright where` prints the path for any project.
//!
//! ```text
//! ~/.wright/projects/<name>-<id>/
//!   runs/<run>/events.jsonl   the event spine, one JSON line per event, fsynced
//!   runs/<run>/receipt.json   the run receipt (the Operator writes it)
//!   objects/<ab>/<hash>       content-addressed artifacts (BLAKE3), git's layout
//!   cache/<key>.json          task cache entries (key -> artifact hashes)
//!   blueprints/<hash>.kdl     every committed blueprint version
//!   blueprints/log            one line per commit: seq hash time message
//!   approvals/<run>-<n>.req   a pending approval (any UI or CLI may answer)
//!   approvals/<run>-<n>.res   the answer: "approve" or "deny"
//!   spend.jsonl               one line per finished run: day, run, cost
//! ```
//!
//! Durability rules: an event is appended and `fsync`ed before the
//! action it describes is considered done, so a crash leaves a log that
//! says exactly how far the firing got (resume reads it). Every other
//! file is written to a temp name and renamed into place — readers see
//! the old file or the new one, never half of one.
//!
//! Local-first: no server, no account. Delete the store directory and
//! the project is exactly as it was before Wright ran, minus its memory.

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use wright_core::events::{Event, EventKind, EventLog, RunId};
use wright_core::hash;
use wright_core::wire::json::{self, Value};

use crate::cache::CacheEntry;

/// Top-level directories that are state, not workspace.
const NOT_WORKSPACE_TOP: &[&str] = &[".wright", ".git", "logs", "artifacts"];
/// Build output, skipped at any depth.
const NOT_WORKSPACE_ANY: &[&str] = &["target", "node_modules", "__pycache__", ".venv"];

/// Handle to one project's store.
#[derive(Debug, Clone)]
pub struct Store {
    pub project_dir: PathBuf,
    root: PathBuf,
}

fn io(context: &str, path: &Path, e: std::io::Error) -> String {
    format!("{context} {}: {e}", path.display())
}

/// Write a file atomically: temp file, fsync, rename.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| io("create", parent, e))?;
    }
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    {
        let mut f = File::create(&tmp).map_err(|e| io("create", &tmp, e))?;
        f.write_all(bytes).map_err(|e| io("write", &tmp, e))?;
        f.sync_all().map_err(|e| io("sync", &tmp, e))?;
    }
    fs::rename(&tmp, path).map_err(|e| io("rename", path, e))
}

impl Store {
    /// Where the store for `project_dir` lives: a readable name plus a
    /// short hash of the project's canonical path (two projects with the
    /// same folder name never collide).
    pub fn default_root(project_dir: &Path) -> PathBuf {
        let canon = project_dir
            .canonicalize()
            .unwrap_or_else(|_| project_dir.to_path_buf());
        let name: String = canon
            .file_name()
            .map(|n| {
                n.to_string_lossy()
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() || c == '-' {
                            c
                        } else {
                            '_'
                        }
                    })
                    .collect()
            })
            .unwrap_or_else(|| "project".into());
        let id = hash::blake3_hex(canon.to_string_lossy().as_bytes());
        crate::config::wright_home()
            .join("projects")
            .join(format!("{name}-{}", &id[..10]))
    }

    /// Open (creating if needed) the store for a project directory.
    pub fn open(project_dir: &Path) -> Result<Store, String> {
        Self::open_at(project_dir, &Self::default_root(project_dir))
    }

    /// Open a store at an explicit location (tests, custom layouts).
    pub fn open_at(project_dir: &Path, root: &Path) -> Result<Store, String> {
        let root = root.to_path_buf();
        for sub in ["runs", "objects", "cache", "blueprints", "approvals"] {
            fs::create_dir_all(root.join(sub)).map_err(|e| io("create", &root.join(sub), e))?;
        }
        Ok(Store {
            project_dir: project_dir.to_path_buf(),
            root,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    // ── runs ─────────────────────────────────────────────────────

    /// All run ids, ascending.
    pub fn runs(&self) -> Vec<RunId> {
        let mut ids: Vec<RunId> = fs::read_dir(self.root.join("runs"))
            .map(|rd| {
                rd.filter_map(|e| e.ok()?.file_name().to_str()?.parse().ok())
                    .collect()
            })
            .unwrap_or_default();
        ids.sort_unstable();
        ids
    }

    pub fn run_dir(&self, run: RunId) -> PathBuf {
        self.root.join("runs").join(run.to_string())
    }

    /// Start a new run: allocate the next id, create its event file.
    pub fn new_run(&self) -> Result<RunWriter, String> {
        let id = self.runs().last().copied().unwrap_or(0) + 1;
        let dir = self.run_dir(id);
        fs::create_dir_all(&dir).map_err(|e| io("create", &dir, e))?;
        RunWriter::open(&dir.join("events.jsonl"), EventLog::new(id))
    }

    /// Reopen a run to append more events (resume).
    pub fn reopen_run(&self, run: RunId) -> Result<RunWriter, String> {
        let log = self.read_events(run)?;
        RunWriter::open(&self.run_dir(run).join("events.jsonl"), log)
    }

    /// Read a run's events. A torn last line (a crash mid-write) is
    /// dropped; anything else malformed is an error.
    pub fn read_events(&self, run: RunId) -> Result<EventLog, String> {
        let path = self.run_dir(run).join("events.jsonl");
        let f = File::open(&path).map_err(|e| io("open", &path, e))?;
        let lines: Vec<String> = BufReader::new(f).lines().map_while(Result::ok).collect();
        let mut events = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            match Event::from_json_line(line) {
                Ok(e) => events.push(e),
                Err(_) if i == lines.len() - 1 => break, // torn tail
                Err(e) => return Err(format!("{}: line {}: {e}", path.display(), i + 1)),
            }
        }
        EventLog::from_events(run, events)
    }

    // ── objects ──────────────────────────────────────────────────

    fn object_path(&self, h: &str) -> PathBuf {
        self.root.join("objects").join(&h[..2.min(h.len())]).join(h)
    }

    /// Store bytes; returns their BLAKE3 hash. Idempotent.
    pub fn put_object(&self, bytes: &[u8]) -> Result<String, String> {
        let h = hash::blake3_hex(bytes);
        let p = self.object_path(&h);
        if !p.exists() {
            write_atomic(&p, bytes)?;
        }
        Ok(h)
    }

    pub fn get_object(&self, h: &str) -> Result<Vec<u8>, String> {
        if h.len() != 64 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!("'{h}' is not an object hash"));
        }
        let p = self.object_path(h);
        let bytes = fs::read(&p).map_err(|e| io("read object", &p, e))?;
        // Content addressing is also an integrity check: a tampered or
        // corrupted object is refused, never served.
        if hash::blake3_hex(&bytes) != h {
            return Err(format!(
                "object {} is corrupt (its content no longer matches its hash)",
                hash::short(h)
            ));
        }
        Ok(bytes)
    }

    // ── cache ────────────────────────────────────────────────────

    pub fn cache_get(&self, key: &str) -> Option<CacheEntry> {
        let text = fs::read_to_string(self.root.join("cache").join(format!("{key}.json"))).ok()?;
        let v = json::parse(&text).ok()?;
        let artifacts = v
            .get("artifacts")?
            .as_arr()?
            .iter()
            .filter_map(|a| Some((a.str_of("name")?.to_string(), a.str_of("hash")?.to_string())))
            .collect();
        Some(CacheEntry {
            key: key.to_string(),
            artifacts,
            cost_micro_usd: v.get("cost_micro_usd")?.as_u64()?,
            produced_by_run: v.get("run")?.as_u64()?,
        })
    }

    pub fn cache_put(&self, e: &CacheEntry) -> Result<(), String> {
        let v = Value::obj(vec![
            ("key", Value::str(e.key.clone())),
            (
                "artifacts",
                Value::Arr(
                    e.artifacts
                        .iter()
                        .map(|(n, h)| {
                            Value::obj(vec![
                                ("name", Value::str(n.clone())),
                                ("hash", Value::str(h.clone())),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("cost_micro_usd", Value::int(e.cost_micro_usd as i64)),
            ("run", Value::int(e.produced_by_run as i64)),
        ]);
        write_atomic(
            &self.root.join("cache").join(format!("{}.json", e.key)),
            json::to_string(&v).as_bytes(),
        )
    }

    // ── blueprint history ────────────────────────────────────────

    /// Commit a blueprint version (canonical text). Returns its hash.
    /// Re-committing identical text is a no-op.
    pub fn commit_blueprint(&self, canonical_text: &str, message: &str) -> Result<String, String> {
        let h = hash::blake3_hex(canonical_text.as_bytes());
        let p = self.root.join("blueprints").join(format!("{h}.kdl"));
        let history = self.blueprint_history();
        if history.last().map(|c| c.hash == h).unwrap_or(false) {
            return Ok(h);
        }
        if !p.exists() {
            write_atomic(&p, canonical_text.as_bytes())?;
        }
        let seq = history.len() + 1;
        let line = format!(
            "{seq}\t{h}\t{}\t{}\n",
            wright_core::events::now_ms(),
            message.replace(['\n', '\t'], " ")
        );
        let log = self.root.join("blueprints").join("log");
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log)
            .map_err(|e| io("open", &log, e))?;
        f.write_all(line.as_bytes())
            .map_err(|e| io("append", &log, e))?;
        f.sync_all().map_err(|e| io("sync", &log, e))?;
        Ok(h)
    }

    pub fn blueprint_history(&self) -> Vec<Commit> {
        let text = fs::read_to_string(self.root.join("blueprints").join("log")).unwrap_or_default();
        text.lines()
            .filter_map(|l| {
                let mut p = l.splitn(4, '\t');
                Some(Commit {
                    seq: p.next()?.parse().ok()?,
                    hash: p.next()?.to_string(),
                    at_ms: p.next()?.parse().ok()?,
                    message: p.next().unwrap_or("").to_string(),
                })
            })
            .collect()
    }

    pub fn blueprint_at(&self, hash: &str) -> Result<String, String> {
        let p = self.root.join("blueprints").join(format!("{hash}.kdl"));
        fs::read_to_string(&p).map_err(|e| io("read", &p, e))
    }

    // ── approvals (cross-process) ────────────────────────────────

    /// Post an approval request; any UI or CLI watching can answer it.
    pub fn post_approval(
        &self,
        run: RunId,
        n: u32,
        agent: &str,
        reason: &str,
    ) -> Result<PathBuf, String> {
        let p = self.root.join("approvals").join(format!("{run}-{n}.req"));
        let v = Value::obj(vec![
            ("run", Value::int(run as i64)),
            ("agent", Value::str(agent)),
            ("reason", Value::str(reason)),
        ]);
        write_atomic(&p, json::to_string(&v).as_bytes())?;
        Ok(p)
    }

    /// The answer to a posted request, if one has arrived.
    pub fn approval_answer(&self, run: RunId, n: u32) -> Option<bool> {
        let p = self.root.join("approvals").join(format!("{run}-{n}.res"));
        let t = fs::read_to_string(p).ok()?;
        match t.trim() {
            "approve" => Some(true),
            "deny" => Some(false),
            _ => None,
        }
    }

    /// Answer a pending request (used by `wright approve` and the UI).
    pub fn answer_approval(&self, run: RunId, n: u32, approve: bool) -> Result<(), String> {
        write_atomic(
            &self.root.join("approvals").join(format!("{run}-{n}.res")),
            if approve { b"approve" } else { b"deny" },
        )
    }

    /// Requests without an answer yet: (run, n, agent, reason).
    pub fn pending_approvals(&self) -> Vec<(RunId, u32, String, String)> {
        let dir = self.root.join("approvals");
        let mut out = Vec::new();
        for e in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let Some(stem) = name.strip_suffix(".req") else {
                continue;
            };
            let Some((r, n)) = stem.split_once('-') else {
                continue;
            };
            let (Ok(r), Ok(n)) = (r.parse::<RunId>(), n.parse::<u32>()) else {
                continue;
            };
            if self.approval_answer(r, n).is_some() {
                continue;
            }
            let v = fs::read_to_string(e.path())
                .ok()
                .and_then(|t| json::parse(&t).ok());
            let (agent, reason) = v
                .map(|v| {
                    (
                        v.str_of("agent").unwrap_or("").to_string(),
                        v.str_of("reason").unwrap_or("").to_string(),
                    )
                })
                .unwrap_or_default();
            out.push((r, n, agent, reason));
        }
        out.sort();
        out
    }

    // ── spend ────────────────────────────────────────────────────

    pub fn record_spend(&self, day: &str, run: RunId, micro_usd: u64) -> Result<(), String> {
        let p = self.root.join("spend.jsonl");
        let line = json::to_string(&Value::obj(vec![
            ("day", Value::str(day)),
            ("run", Value::int(run as i64)),
            ("micro_usd", Value::int(micro_usd as i64)),
        ]));
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&p)
            .map_err(|e| io("open", &p, e))?;
        writeln!(f, "{line}").map_err(|e| io("append", &p, e))?;
        f.sync_all().map_err(|e| io("sync", &p, e))
    }

    /// Total spend recorded for one UTC day (YYYY-MM-DD).
    pub fn spent_on(&self, day: &str) -> u64 {
        fs::read_to_string(self.root.join("spend.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| json::parse(l).ok())
            .filter(|v| v.str_of("day") == Some(day))
            .filter_map(|v| v.get("micro_usd").and_then(|n| n.as_u64()))
            .sum()
    }

    // ── workspace ────────────────────────────────────────────────

    /// BLAKE3 over every workspace file's path and contents, sorted.
    /// Skips `.wright`, `.git`, logs, artifacts and build output.
    pub fn workspace_hash(&self) -> String {
        let mut files = Vec::new();
        collect_files(&self.project_dir, &self.project_dir, &mut files);
        files.sort();
        let mut h = hash::Hasher::new();
        for rel in files {
            let bytes = fs::read(self.project_dir.join(&rel)).unwrap_or_default();
            h.update(&(rel.len() as u64).to_le_bytes());
            h.update(rel.as_bytes());
            h.update(&(bytes.len() as u64).to_le_bytes());
            h.update(&bytes);
        }
        h.finalize_hex()
    }
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_symlink() {
            continue; // never follow links out of the project
        }
        let path = e.path();
        if ft.is_dir() {
            if (dir == root && NOT_WORKSPACE_TOP.contains(&name.as_str()))
                || NOT_WORKSPACE_ANY.contains(&name.as_str())
            {
                continue;
            }
            collect_files(root, &path, out);
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// One committed blueprint version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub seq: usize,
    pub hash: String,
    pub at_ms: u64,
    pub message: String,
}

/// Appends events to a run's log, durably.
pub struct RunWriter {
    file: File,
    log: EventLog,
    path: PathBuf,
}

impl RunWriter {
    fn open(path: &Path, log: EventLog) -> Result<RunWriter, String> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| io("open", path, e))?;
        Ok(RunWriter {
            file,
            log,
            path: path.to_path_buf(),
        })
    }

    pub fn run(&self) -> RunId {
        self.log.run()
    }

    pub fn log(&self) -> &EventLog {
        &self.log
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append, write, fsync. The event exists once this returns Ok.
    pub fn append(
        &mut self,
        kind: EventKind,
        agent: Option<&str>,
        note: impl Into<String>,
        data: String,
    ) -> Result<Event, String> {
        let e = self.log.append_with(kind, agent, note, data);
        let line = e.to_json_line();
        self.file
            .write_all(line.as_bytes())
            .and_then(|_| self.file.write_all(b"\n"))
            .map_err(|er| io("append", &self.path, er))?;
        self.file
            .sync_data()
            .map_err(|er| io("sync", &self.path, er))?;
        Ok(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(dir: &Path) -> Store {
        Store::open_at(dir, &dir.join(".state")).unwrap()
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("wright-store-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn events_survive_reopen_and_torn_tails() {
        let dir = tmp("events");
        let store = st(&dir);
        let mut w = store.new_run().unwrap();
        assert_eq!(w.run(), 1);
        w.append(EventKind::RunStarted, None, "go", String::new())
            .unwrap();
        w.append(EventKind::AgentStarted, Some("A"), "a", r#"{"x":1}"#.into())
            .unwrap();
        drop(w);
        // Simulate a crash mid-write: a torn trailing line.
        let p = store.run_dir(1).join("events.jsonl");
        let mut f = OpenOptions::new().append(true).open(&p).unwrap();
        f.write_all(b"{\"seq\":3,\"ru").unwrap();
        let log = store.read_events(1).unwrap();
        assert_eq!(log.len(), 2);
        assert_eq!(log.events()[1].agent.as_deref(), Some("A"));
        assert_eq!(store.new_run().unwrap().run(), 2);
    }

    #[test]
    fn objects_are_content_addressed() {
        let store = st(&tmp("objects"));
        let h = store.put_object(b"hello").unwrap();
        assert_eq!(h, hash::blake3_hex(b"hello"));
        assert_eq!(store.get_object(&h).unwrap(), b"hello");
        assert!(store.get_object("../../etc/passwd").is_err());
    }

    #[test]
    fn cache_entries_round_trip() {
        let store = st(&tmp("cache"));
        let e = CacheEntry {
            key: "k".repeat(64),
            artifacts: vec![("a.json".into(), "h".repeat(64))],
            cost_micro_usd: 1234,
            produced_by_run: 7,
        };
        store.cache_put(&e).unwrap();
        assert_eq!(store.cache_get(&e.key), Some(e));
    }

    #[test]
    fn blueprint_commits_dedupe_and_list() {
        let store = st(&tmp("bp"));
        let h1 = store.commit_blueprint("meta {}\n", "first").unwrap();
        store.commit_blueprint("meta {}\n", "same again").unwrap();
        let h2 = store
            .commit_blueprint("meta { name \"x\" }\n", "second")
            .unwrap();
        let hist = store.blueprint_history();
        assert_eq!(hist.len(), 2);
        assert_eq!(
            (hist[0].hash.as_str(), hist[1].hash.as_str()),
            (h1.as_str(), h2.as_str())
        );
        assert_eq!(store.blueprint_at(&h1).unwrap(), "meta {}\n");
    }

    #[test]
    fn approvals_round_trip_across_handles() {
        let dir = tmp("approvals");
        let a = st(&dir);
        a.post_approval(3, 1, "Mailer", "wants to send").unwrap();
        let b = st(&dir); // another process, same project
        assert_eq!(b.pending_approvals().len(), 1);
        b.answer_approval(3, 1, true).unwrap();
        assert_eq!(a.approval_answer(3, 1), Some(true));
        assert!(a.pending_approvals().is_empty());
    }

    #[test]
    fn workspace_hash_ignores_state_and_follows_content() {
        let dir = tmp("ws");
        let store = st(&dir);
        fs::write(dir.join("a.py"), "x = 1").unwrap();
        let h1 = store.workspace_hash();
        fs::create_dir_all(dir.join("logs")).unwrap();
        fs::write(dir.join("logs/run.log"), "noise").unwrap();
        assert_eq!(h1, store.workspace_hash(), "logs are not the workspace");
        fs::write(dir.join("a.py"), "x = 2").unwrap();
        assert_ne!(h1, store.workspace_hash());
    }

    #[test]
    fn spend_is_summed_per_day() {
        let store = st(&tmp("spend"));
        store.record_spend("2026-10-20", 1, 100).unwrap();
        store.record_spend("2026-10-20", 2, 250).unwrap();
        store.record_spend("2026-10-21", 3, 999).unwrap();
        assert_eq!(store.spent_on("2026-10-20"), 350);
    }
}
