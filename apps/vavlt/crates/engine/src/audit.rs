//! The append-only, hash-chained log (SPEC §1.7).
//!
//! Each entry commits to the one before it, so a removed entry is *detectable*
//! rather than merely missing. Skips and failures are recorded as prominently
//! as successes — a log that only records what went well is marketing.
//!
//! Deliberately hand-rolled JSON lines rather than `serde_json`: the schema is
//! five flat string fields, the reader has to be tolerant of a truncated final
//! line after a crash, and a strict parser would throw away the rest of a
//! user's log over one bad byte. That is the one place tolerance is the right
//! default, and it is worth not depending on a derive to get it.

use std::path::{Path, PathBuf};

use anyhow::Result;

/// One event, and its link in the chain.
#[derive(Debug, Clone)]
pub struct Entry {
    /// Unix seconds, UTC.
    pub at: u64,
    /// `grant` · `run` · `skip` · `fail` · `restore`. A closed set, because the
    /// Activity screen colours the rule beside each entry from it.
    pub kind: &'static str,
    pub title: String,
    pub detail: String,
    /// BLAKE3 over the previous hash and this entry's own fields, truncated to
    /// 24 hex characters — enough to detect tampering, short enough to show.
    pub hash: String,
}

/// The log, loaded from disk and appended to in place.
#[derive(Debug)]
pub struct Audit {
    path: PathBuf,
    prev: String,
    pub entries: Vec<Entry>,
}

impl Audit {
    /// Open — or start — the log in `dir`.
    ///
    /// Never fails: an unreadable log is an empty one plus a warning, because
    /// the alternative is an app that will not launch over a file it only
    /// writes to.
    pub fn open(dir: &Path) -> Self {
        let mut this = Self {
            path: dir.join("audit.jsonl"),
            prev: "genesis".into(),
            entries: Vec::new(),
        };
        this.load();
        log::info!("audit: {} entries at {}", this.entries.len(), this.path.display());
        this
    }

    fn load(&mut self) {
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return;
        };
        for line in text.lines() {
            // Deliberately tolerant: a corrupt line must not cost the user the
            // rest of their log.
            let Some(entry) = parse_entry(line) else {
                log::warn!("audit: skipping an unparseable line");
                continue;
            };
            self.prev = entry.hash.clone();
            self.entries.push(entry);
        }
    }

    /// Append one entry, chained to the last.
    pub fn record(
        &mut self,
        kind: &'static str,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) {
        let title = title.into();
        let detail = detail.into();
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let mut h = blake3::Hasher::new();
        h.update(self.prev.as_bytes());
        h.update(kind.as_bytes());
        h.update(title.as_bytes());
        h.update(detail.as_bytes());
        h.update(&at.to_le_bytes());
        let hash = h.finalize().to_hex()[..24].to_string();

        let entry = Entry {
            at,
            kind,
            title,
            detail,
            hash: hash.clone(),
        };
        self.append(&entry);
        self.prev = hash;
        self.entries.push(entry);
    }

    fn append(&self, entry: &Entry) {
        use std::io::Write;

        let line = format!(
            "{{\"at\":{},\"kind\":\"{}\",\"title\":\"{}\",\"detail\":\"{}\",\"hash\":\"{}\"}}\n",
            entry.at,
            entry.kind,
            escape(&entry.title),
            escape(&entry.detail),
            entry.hash
        );

        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            Ok(mut file) => {
                if let Err(error) = file.write_all(line.as_bytes()) {
                    log::error!("audit: could not append: {error:#}");
                }
            }
            Err(error) => log::error!("audit: could not open {}: {error:#}", self.path.display()),
        }
    }

    /// Copy the log somewhere the user can get at it.
    ///
    /// Export exists because the log does not survive uninstalling the app —
    /// which is deliberate, and which the Settings screen says out loud.
    pub fn export(&self) -> Result<PathBuf> {
        let out = self.path.with_file_name("audit-export.jsonl");
        std::fs::copy(&self.path, &out)?;
        log::info!("audit: exported to {}", out.display());
        Ok(out)
    }
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
}

fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let start = line.find(&format!("\"{key}\":\""))? + key.len() + 4;
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn parse_entry(line: &str) -> Option<Entry> {
    let at = line
        .find("\"at\":")
        .map(|i| &line[i + 5..])
        .and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    // Mapped back to a `&'static str` rather than kept as an owned string: the
    // set is closed, and anything outside it is a log line this build did not
    // write.
    let kind = match field(line, "kind")? {
        "grant" => "grant",
        "skip" => "skip",
        "fail" => "fail",
        "restore" => "restore",
        _ => "run",
    };

    Some(Entry {
        at,
        kind,
        title: field(line, "title")?.to_string(),
        detail: field(line, "detail").unwrap_or("").to_string(),
        hash: field(line, "hash").unwrap_or("").to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The chain is the whole point, so this is the test that matters: two
    /// entries, and the second's hash depends on the first.
    #[test]
    fn each_entry_commits_to_the_one_before_it() {
        let dir = std::env::temp_dir().join(format!("vavlt-audit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");

        let mut audit = Audit::open(&dir);
        audit.record("grant", "first", "");
        let first = audit.entries[0].hash.clone();
        audit.record("run", "second", "");
        let second = audit.entries[1].hash.clone();

        // Same fields, different predecessor: a different hash. If `prev` were
        // not folded in, these two runs would produce the same digest.
        let mut fresh = Audit::open(&dir.join("elsewhere"));
        std::fs::create_dir_all(dir.join("elsewhere")).ok();
        fresh.record("run", "second", "");

        assert_ne!(first, second, "two entries must not share a hash");
        assert_ne!(
            fresh.entries[0].hash, second,
            "the same entry after a different history must hash differently"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A truncated final line — the shape a crash mid-append leaves — must cost
    /// that line and nothing else.
    #[test]
    fn a_corrupt_line_does_not_cost_the_rest_of_the_log() {
        let dir = std::env::temp_dir().join(format!("vavlt-audit-torn-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(
            dir.join("audit.jsonl"),
            "{\"at\":1,\"kind\":\"run\",\"title\":\"kept\",\"detail\":\"\",\"hash\":\"aa\"}\n\
             {\"at\":2,\"kind\":\"run\",\"tit\n",
        )
        .expect("write");

        let audit = Audit::open(&dir);
        assert_eq!(audit.entries.len(), 1, "the good line survives");
        assert_eq!(audit.entries[0].title, "kept");

        std::fs::remove_dir_all(&dir).ok();
    }
}
