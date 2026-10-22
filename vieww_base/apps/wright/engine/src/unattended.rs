//! Approval for unattended runs — and the time it took to earn it.
//!
//! The moment that matters for a person who does not write code is not
//! "the agent answered" or even "the checks passed once". It is when they
//! say: *this may now run without me*. That is a deliberate act here
//! (`wright schedule approve`, or "Let it run unattended" in the window),
//! and Wright measures the road to it — **time to trust**: from the first
//! version of the plan to the go-ahead, with every run, failure, approval
//! asked and change of plan on the way. `wright bench` reports it.
//!
//! Rules:
//!
//! * The go-ahead needs at least one run that passed its checks — you
//!   approve something you have seen work, not a promise.
//! * It is for ONE plan: the approval records the blueprint's content
//!   hash. When the plan changes, the schedule waits for a new go-ahead
//!   (the scheduler asks [`current`] before every unattended firing).
//! * It lives in the project's store, outside the project folder, so no
//!   agent can grant it to itself.

use std::path::Path;

use wright_core::blueprint::canon;
use wright_core::events::{now_ms, EventKind};
use wright_core::hash;
use wright_core::wire::json::{self, Value};
use wright_runtime::store::{write_atomic, Store};

use crate::scheduler::{self, State};

/// The road to trust, as numbers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Road {
    /// When the plan first existed (its first committed version).
    pub started_ms: u64,
    /// Runs fired, in total and how many passed.
    pub runs: u32,
    pub passed: u32,
    /// Times a run stopped to ask a person, and how many were refused.
    pub asked: u32,
    pub refused: u32,
    /// Versions of the plan after the first: changes the person asked for.
    pub changes: u32,
}

/// A go-ahead to run unattended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Approval {
    pub at_ms: u64,
    /// Content hash of the blueprint that was approved.
    pub blueprint: String,
    pub by: String,
    /// The road as it stood at the moment of approval.
    pub road: Road,
}

impl Approval {
    /// Time from the first plan to the go-ahead, in milliseconds.
    pub fn time_to_trust_ms(&self) -> u64 {
        self.at_ms.saturating_sub(self.road.started_ms)
    }
}

fn file(store: &Store) -> std::path::PathBuf {
    store.root().join("unattended.json")
}

/// The content hash of the project's blueprint as it is on disk now.
pub fn blueprint_hash(project: &Path) -> Result<String, String> {
    let bp = crate::load_blueprint(project)?;
    Ok(hash::blake3_hex(canon::canonical(&bp).as_bytes()))
}

/// The road so far, read from the store: runs, receipts, asks, changes.
pub fn road(project: &Path) -> Result<Road, String> {
    let store = Store::open(project)?;
    let history = store.blueprint_history();
    let mut distinct: Vec<String> = Vec::new();
    for c in &history {
        if !distinct.contains(&c.hash) {
            distinct.push(c.hash.clone());
        }
    }
    // The plan on disk counts too: an edit not fired yet is still a change.
    if let Ok(now) = blueprint_hash(project) {
        if !distinct.contains(&now) {
            distinct.push(now);
        }
    }
    let mut r = Road {
        started_ms: history.first().map(|c| c.at_ms).unwrap_or(0),
        changes: distinct.len().saturating_sub(1) as u32,
        ..Road::default()
    };
    for run in store.runs() {
        r.runs += 1;
        let receipt =
            std::fs::read_to_string(store.run_dir(run).join("receipt.json")).unwrap_or_default();
        if json::parse(&receipt)
            .ok()
            .and_then(|v| v.get("ok").and_then(|b| b.as_bool()))
            == Some(true)
        {
            r.passed += 1;
        }
        if let Ok(log) = store.read_events(run) {
            for e in log.events() {
                match e.kind {
                    EventKind::ApprovalRequested => r.asked += 1,
                    EventKind::ApprovalResolved if e.note.ends_with("denied") => r.refused += 1,
                    _ => {}
                }
                if r.started_ms == 0 || (e.at_ms > 0 && e.at_ms < r.started_ms) {
                    r.started_ms = e.at_ms;
                }
            }
        }
    }
    Ok(r)
}

/// Give the go-ahead for unattended runs of this exact plan.
pub fn approve(project: &Path, by: &str) -> Result<Approval, String> {
    let road = road(project)?;
    if road.passed == 0 {
        return Err(
            "run it once and let it pass its checks first — an unattended go-ahead is for something you have seen work"
                .into(),
        );
    }
    let a = Approval {
        at_ms: now_ms(),
        blueprint: blueprint_hash(project)?,
        by: by.to_string(),
        road,
    };
    let store = Store::open(project)?;
    let v = Value::obj(vec![
        ("at_ms", Value::int(a.at_ms as i64)),
        ("blueprint", Value::str(a.blueprint.clone())),
        ("by", Value::str(a.by.clone())),
        ("started_ms", Value::int(a.road.started_ms as i64)),
        ("runs", Value::int(a.road.runs as i64)),
        ("passed", Value::int(a.road.passed as i64)),
        ("asked", Value::int(a.road.asked as i64)),
        ("refused", Value::int(a.road.refused as i64)),
        ("changes", Value::int(a.road.changes as i64)),
    ]);
    write_atomic(&file(&store), json::to_string(&v).as_bytes())?;
    let _ = scheduler::set_state(project, State::Active);
    Ok(a)
}

/// Take the go-ahead back; the schedule waits again.
pub fn revoke(project: &Path) -> Result<bool, String> {
    let store = Store::open(project)?;
    let _ = scheduler::set_state(project, State::Awaiting);
    match std::fs::remove_file(file(&store)) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}

/// The recorded go-ahead, whether or not it still matches the plan.
pub fn recorded(project: &Path) -> Option<Approval> {
    let store = Store::open(project).ok()?;
    let v = json::parse(&std::fs::read_to_string(file(&store)).ok()?).ok()?;
    let n = |k: &str| v.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
    Some(Approval {
        at_ms: n("at_ms"),
        blueprint: v.str_of("blueprint")?.to_string(),
        by: v.str_of("by").unwrap_or("").to_string(),
        road: Road {
            started_ms: n("started_ms"),
            runs: n("runs") as u32,
            passed: n("passed") as u32,
            asked: n("asked") as u32,
            refused: n("refused") as u32,
            changes: n("changes") as u32,
        },
    })
}

/// The go-ahead in force for the plan as it is now, or why there is none.
pub fn current(project: &Path) -> Result<Approval, String> {
    let a = recorded(project).ok_or_else(|| "not approved to run unattended yet".to_string())?;
    let now = blueprint_hash(project)?;
    if now != a.blueprint {
        return Err(format!(
            "the plan changed since you approved it to run unattended (approved {}, now {}); approve it again",
            hash::short(&a.blueprint),
            hash::short(&now)
        ));
    }
    Ok(a)
}

/// "2h 14m", "3d 4h", "45s".
pub fn human_duration(ms: u64) -> String {
    let s = ms / 1000;
    match s {
        0..=59 => format!("{s}s"),
        60..=3599 => format!("{}m {}s", s / 60, s % 60),
        3600..=86_399 => format!("{}h {}m", s / 3600, (s % 3600) / 60),
        _ => format!("{}d {}h", s / 86_400, (s % 86_400) / 3600),
    }
}

/// One sentence about the road, for the CLI and the window.
pub fn describe(a: &Approval) -> String {
    format!(
        "trusted to run unattended after {} — {} run{} ({} passed), {} approval{} asked ({} refused), {} change{} to the plan",
        human_duration(a.time_to_trust_ms()),
        a.road.runs,
        if a.road.runs == 1 { "" } else { "s" },
        a.road.passed,
        a.road.asked,
        if a.road.asked == 1 { "" } else { "s" },
        a.road.refused,
        a.road.changes,
        if a.road.changes == 1 { "" } else { "s" },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_like_a_person_wrote_them() {
        assert_eq!(human_duration(45_000), "45s");
        assert_eq!(human_duration(134 * 60_000), "2h 14m");
        assert_eq!(human_duration(3 * 86_400_000 + 4 * 3_600_000), "3d 4h");
    }
}
