//! The fleet: everything this machine runs unattended, on one screen.
//!
//! One row per scheduled project — what it is, when it runs next, whether
//! it will (active, paused, waiting for a go-ahead, held by the kill
//! switch or by a changed plan), whether it is firing right now, what its
//! last run did and cost, whether that run's receipt still verifies, and
//! whether a run is waiting on a person. `wright fleet` prints it; the
//! window's Fleet panel draws it. Read-only: the actions (pause, resume,
//! approve, stop everything) live in `scheduler`, `unattended` and the
//! kill switch.

use std::path::{Path, PathBuf};

use wright_core::events::now_ms;
use wright_core::wire::json;
use wright_runtime::store::Store;

use crate::scheduler::{self, State};

/// The last run of a project, as its receipt tells it.
#[derive(Debug, Clone, PartialEq)]
pub struct LastRun {
    pub run: u64,
    pub ok: bool,
    pub headline: String,
    pub finished_ms: u64,
    pub cost_micro_usd: u64,
    /// The receipt's own verification (signature, log, artifacts...).
    pub verified: bool,
}

/// One project in the fleet.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub name: String,
    pub dir: PathBuf,
    pub cron: String,
    pub tz: String,
    /// "2026-10-23 08:00", or why it never fires.
    pub next: String,
    pub state: State,
    /// Why it will not fire next time, in words (None: it will).
    pub held: Option<String>,
    pub firing_now: bool,
    pub waiting_on_you: usize,
    pub last: Option<LastRun>,
    /// "trusted to run unattended after ..." when approved.
    pub trust: Option<String>,
}

/// Every scheduled project, in registry order.
pub fn rows() -> Vec<Row> {
    scheduler::load().into_iter().map(|e| row(&e)).collect()
}

fn row(e: &scheduler::Entry) -> Row {
    let dir = PathBuf::from(&e.project);
    let now = (now_ms() / 1000) as i64;
    let next = scheduler::next_fire(&e.cron, &e.tz, now)
        .map(|t| scheduler::local_string(t, &e.tz))
        .unwrap_or_else(|err| err);
    let name = crate::load_blueprint(&dir)
        .map(|bp| bp.meta.name)
        .unwrap_or_else(|_| dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
    let store = Store::open(&dir).ok();
    let firing_now = store.as_ref().is_some_and(|s| lock_alive(&s.root().join("firing.lock")));
    let waiting_on_you = store.as_ref().map(|s| s.pending_approvals().len()).unwrap_or(0);
    let last = store.as_ref().and_then(|s| last_run(&dir, s));
    let trust = crate::unattended::current(&dir).ok().map(|a| crate::unattended::describe(&a));
    Row {
        name,
        cron: e.cron.clone(),
        tz: e.tz.clone(),
        next,
        state: e.state,
        held: scheduler::hold_reason(e),
        firing_now,
        waiting_on_you,
        last,
        trust,
        dir,
    }
}

fn last_run(dir: &Path, store: &Store) -> Option<LastRun> {
    let run = *store.runs().last()?;
    let text = std::fs::read_to_string(store.run_dir(run).join("receipt.json")).ok()?;
    let v = json::parse(&text).ok()?;
    let verified = crate::verify::project_run(dir, Some(run)).map(|v| v.ok).unwrap_or(false);
    Some(LastRun {
        run,
        ok: v.get("ok").and_then(|b| b.as_bool()).unwrap_or(false),
        headline: v.str_of("headline").unwrap_or("").to_string(),
        finished_ms: v.get("finished_ms").and_then(|n| n.as_u64()).unwrap_or(0),
        cost_micro_usd: v.get("cost_micro_usd").and_then(|n| n.as_u64()).unwrap_or(0),
        verified,
    })
}

/// A firing lock whose process is still alive.
fn lock_alive(p: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(p) else {
        return false;
    };
    let pid: u32 = text.trim().parse().unwrap_or(0);
    pid != 0 && (cfg!(not(target_os = "linux")) || Path::new(&format!("/proc/{pid}")).exists())
}

/// The fleet as plain text — what `wright fleet` prints.
pub fn report(rows: &[Row]) -> String {
    let mut out = String::new();
    match wright_runtime::killswitch::current() {
        Some(s) => out.push_str(&format!(
            "STOPPED — {} (since {}). Nothing runs until `wright start`.\n\n",
            s.reason,
            wright_core::schedule::rfc3339_utc((s.at_ms / 1000) as i64)
        )),
        None => out.push_str("Running. `wright stop \"why\"` stops everything at once.\n\n"),
    }
    if rows.is_empty() {
        out.push_str("Nothing is scheduled. A plan with a schedule registers itself on its first run.\n");
        return out;
    }
    for r in rows {
        let state = if r.firing_now {
            "FIRING NOW".to_string()
        } else {
            match &r.held {
                None => format!("active, next {}", r.next),
                Some(why) => format!("{} — {why}", r.state.as_str()),
            }
        };
        out.push_str(&format!("{}  [{} {}]\n  {}\n  {}\n", r.name, r.cron, r.tz, r.dir.display(), state));
        match &r.last {
            Some(l) => out.push_str(&format!(
                "  last: run {} {} — {} — ${:.4} — receipt {}\n",
                l.run,
                if l.ok { "passed" } else { "FAILED" },
                l.headline,
                l.cost_micro_usd as f64 / 1e6,
                if l.verified { "verifies" } else { "DOES NOT VERIFY" }
            )),
            None => out.push_str("  last: never run\n"),
        }
        if r.waiting_on_you > 0 {
            out.push_str(&format!("  {} approval(s) waiting on you: wright approve {}\n", r.waiting_on_you, r.dir.display()));
        }
        if let Some(t) = &r.trust {
            out.push_str(&format!("  {t}\n"));
        }
        out.push('\n');
    }
    out
}
