//! The kill switch: one switch that stops everything Wright does.
//!
//! A file, `<wright home>/STOP`, is the signal — because the things it must
//! stop live in different processes (the window, `wright run` in a
//! terminal, the background service firing schedules) and a file is the
//! one thing all of them already look at. While it exists:
//!
//! * no firing starts, whatever started it (a schedule, a click, the CLI);
//! * the permission gate denies every tool call — so a firing already in
//!   flight cannot send, write or fetch anything more, even mid-turn;
//! * agents stop at their next turn, sandboxed commands and workers are
//!   killed within a moment, and pending approvals are answered "no";
//! * the background service keeps running but fires nothing.
//!
//! `wright stop` / "Stop everything" engages it; `wright start` / "Start
//! again" releases it. Nothing is lost: runs that were stopped say so in
//! their event log and receipt, and resume picks up where they were.

use std::path::{Path, PathBuf};

use wright_core::events::now_ms;
use wright_core::wire::json::{self, Value};

use crate::config::wright_home;
use crate::store::write_atomic;

/// Why and when everything was stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stop {
    pub reason: String,
    pub at_ms: u64,
    /// Who pulled it: "cli", "window", "policy", or a person's words.
    pub by: String,
}

/// Where the switch lives.
pub fn path() -> PathBuf {
    wright_home().join("STOP")
}

/// Stop everything. Idempotent: a second stop keeps the first reason.
pub fn engage(reason: &str, by: &str) -> Result<Stop, String> {
    engage_at(&path(), reason, by)
}

fn engage_at(p: &Path, reason: &str, by: &str) -> Result<Stop, String> {
    if let Some(s) = current_at(p) {
        return Ok(s);
    }
    let stop = Stop {
        reason: if reason.trim().is_empty() {
            "stopped by hand".into()
        } else {
            reason.trim().to_string()
        },
        at_ms: now_ms(),
        by: by.to_string(),
    };
    let v = Value::obj(vec![
        ("reason", Value::str(stop.reason.clone())),
        ("at_ms", Value::int(stop.at_ms as i64)),
        ("by", Value::str(stop.by.clone())),
    ]);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    write_atomic(p, json::to_string(&v).as_bytes())?;
    Ok(stop)
}

/// Let Wright run again. `Ok(false)` when it was not stopped.
pub fn release() -> Result<bool, String> {
    release_at(&path())
}

fn release_at(p: &Path) -> Result<bool, String> {
    match std::fs::remove_file(p) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("{}: {e}", p.display())),
    }
}

/// The stop in force, if any. A switch file that cannot be parsed still
/// counts as a stop: failing safe is the point of the thing.
pub fn current() -> Option<Stop> {
    current_at(&path())
}

fn current_at(p: &Path) -> Option<Stop> {
    let text = match std::fs::read_to_string(p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(_) => String::new(),
    };
    let v = json::parse(&text).ok();
    Some(Stop {
        reason: v
            .as_ref()
            .and_then(|v| v.str_of("reason").map(String::from))
            .unwrap_or_else(|| "stopped (the switch file could not be read)".into()),
        at_ms: v
            .as_ref()
            .and_then(|v| v.get("at_ms").and_then(|n| n.as_u64()))
            .unwrap_or(0),
        by: v
            .as_ref()
            .and_then(|v| v.str_of("by").map(String::from))
            .unwrap_or_default(),
    })
}

/// Cheap enough to ask before every tool call: one `stat`.
pub fn is_stopped() -> bool {
    path().exists()
}

/// The sentence a refused action says.
pub fn refusal() -> Option<String> {
    current().map(|s| {
        format!(
            "Wright is stopped ({}). Nothing runs, sends or fetches until it is started again: `wright start`, or Start again in the window.",
            s.reason
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Through the path-taking halves: tests run in parallel, and a STOP
    // in a shared WRIGHT_HOME would stop every other test's gate.
    #[test]
    fn stop_and_start_round_trip_and_fail_safe() {
        let dir = std::env::temp_dir().join(format!("wright-kill-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let p = dir.join("STOP");
        assert!(current_at(&p).is_none());
        let s = engage_at(&p, "a bad digest went out", "cli").unwrap();
        assert!(p.exists());
        assert_eq!(
            engage_at(&p, "again", "cli").unwrap(),
            s,
            "the first reason stands"
        );
        assert_eq!(current_at(&p).unwrap().reason, "a bad digest went out");
        assert!(release_at(&p).unwrap());
        assert!(!release_at(&p).unwrap(), "releasing twice is harmless");
        // A garbled switch still stops.
        std::fs::write(&p, "not json").unwrap();
        assert!(current_at(&p).is_some());
    }
}
