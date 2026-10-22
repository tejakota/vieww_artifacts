//! `wright explain` — the one-screen story of a run (the memo's
//! "explain any run in one screen").
//!
//! "What happened while I slept" is the question a scheduled run has to
//! answer before a person trusts it. The event spine already knows
//! everything; this module only has to TELL it, in the order a person
//! asks:
//!
//! ```text
//! The plan: what this project is, and what the plan promised
//! What ran: each task, one line — what it did, what it cost
//! What it sent: every world-touching call, with the outcome
//! What it made: artifacts, with hashes short enough to say aloud
//! The verdict: the Reviewer's word, and each acceptance sentence
//! The numbers: cost, cache savings, turns — from the receipt
//! ```
//!
//! The default view of a scheduled run (ADR: the memo's item 5) reads
//! exactly this text: `wright explain` is what the daemon shows.

use std::path::Path;

use wright_core::events::{Event, EventKind};
use wright_core::hash;
use wright_core::wire::json::{self, Value};
use wright_runtime::store::Store;

/// The story of one run, as text. One screen, no scrolling needed for a
/// normal firing; every line is a sentence, not a code fragment.
pub fn story_of_run(store: &Store, run: u64) -> Result<String, String> {
    let log = store.read_events(run)?;
    let receipt_path = store.run_dir(run).join("receipt.json");
    let receipt = std::fs::read_to_string(&receipt_path)
        .ok()
        .and_then(|t| json::parse(&t).ok());
    let events = log.events();
    let mut out = String::new();

    // ── the plan ────────────────────────────────────────────────
    let project = receipt
        .as_ref()
        .and_then(|r| r.str_of("project").map(String::from))
        .unwrap_or_else(|| "the project".into());
    let started = events
        .iter()
        .find(|e| e.kind == EventKind::RunStarted)
        .map(|e| {
            format!(
                "started {} (trigger: {})",
                short_time(e.at_ms),
                e.note.trim()
            )
        })
        .unwrap_or_else(|| "started at an unknown moment".into());
    out.push_str(&format!("What happened in run {run} of {project}\n\n"));
    out.push_str(&format!("THE PLAN  {}\n", started));
    if let Some(r) = &receipt {
        out.push_str(&format!(
            "THE PROMISE  {}\n",
            r.str_of("headline").unwrap_or("(no headline recorded)")
        ));
    }
    // The intent, from the committed blueprint, when it is still around.
    if let Some(commit) = receipt.as_ref().and_then(|r| r.str_of("blueprint")) {
        if let Ok(text) = store.blueprint_at(commit) {
            if let Ok(bp) = wright_core::blueprint::parse::blueprint(&text) {
                if !bp.intent.summary.is_empty() {
                    out.push_str(&format!("IN YOUR WORDS  {}\n", bp.intent.summary));
                }
            }
        }
    }

    // ── what ran ───────────────────────────────────────────────
    out.push_str("\nWHAT RAN\n");
    for e in events {
        match e.kind {
            EventKind::AgentStarted => {
                out.push_str(&format!("  ▶ {}  {}\n", who(e), first_sentence(&e.note)))
            }
            EventKind::AgentFinished => {
                out.push_str(&format!("  ● {}  {}\n", who(e), first_sentence(&e.note)))
            }
            EventKind::AgentFailed => {
                out.push_str(&format!("  ✕ {}  {}\n", who(e), first_sentence(&e.note)))
            }
            EventKind::TaskSkipped => {
                out.push_str(&format!("  - {}  {}\n", who(e), first_sentence(&e.note)))
            }
            EventKind::CacheHit => {
                out.push_str(&format!("  = {}  {}\n", who(e), first_sentence(&e.note)))
            }
            _ => {}
        }
    }

    // ── what it sent ────────────────────────────────────────────
    let sends: Vec<&Event> = events
        .iter()
        .filter(|e| {
            e.kind == EventKind::ToolCalled
                && (e.note.contains("http-post") || e.note.contains("mcp-call"))
        })
        .collect();
    out.push_str("\nWHAT IT SENT\n");
    if sends.is_empty() {
        out.push_str("  nothing left this machine\n");
    } else {
        for e in sends {
            out.push_str(&format!("  → {}  {}\n", who(e), first_sentence(&e.note)));
        }
    }
    let asks: Vec<&Event> = events
        .iter()
        .filter(|e| e.kind == EventKind::ApprovalRequested)
        .collect();
    let answers: Vec<&Event> = events
        .iter()
        .filter(|e| e.kind == EventKind::ApprovalResolved)
        .collect();
    if !asks.is_empty() {
        out.push_str(&format!(
            "  ({} approval(s) were requested, {} answered)\n",
            asks.len(),
            answers.len()
        ));
    }

    // ── what it made ───────────────────────────────────────────
    out.push_str("\nWHAT IT MADE\n");
    if let Some(r) = &receipt {
        let artifacts: Vec<Value> = r
            .get("artifacts")
            .and_then(|a| a.as_arr().map(|x| x.to_vec()))
            .unwrap_or_default();
        if artifacts.is_empty() {
            out.push_str("  no artifact files\n");
        } else {
            for a in artifacts {
                out.push_str(&format!(
                    "  {}  {}\n",
                    a.str_of("name").unwrap_or("?"),
                    a.str_of("blake3").map(hash::short).unwrap_or("no hash"),
                ));
            }
        }
    }

    // ── the verdict ─────────────────────────────────────────────
    out.push_str("\nTHE VERDICT\n");
    if let Some(r) = &receipt {
        match r.get("verdict").and_then(|v| v.as_str()) {
            Some("approve") => out.push_str("  the Reviewer approved the work\n"),
            Some(v) => out.push_str(&format!("  {}\n", v)),
            None => out.push_str("  no verdict (the checks did not run or did not finish)\n"),
        }
        let accepts: Vec<Value> = r
            .get("accepts")
            .and_then(|a| a.as_arr().map(|x| x.to_vec()))
            .unwrap_or_default();
        for a in &accepts {
            let n = a.get("n").and_then(|n| n.as_u64()).unwrap_or(0);
            let sentence = a
                .str_of("sentence")
                .unwrap_or("(the sentence was not recorded)");
            let ok = a.get("ok").and_then(|o| o.as_bool()).unwrap_or(false);
            out.push_str(&format!(
                "  promise {n}: {} — {}\n",
                first_sentence(sentence),
                if ok { "kept" } else { "BROKEN" }
            ));
        }
    } else {
        out.push_str("  no receipt was written for this run\n");
    }

    // ── the numbers ─────────────────────────────────────────────
    out.push_str("\nTHE NUMBERS\n");
    if let Some(r) = &receipt {
        let cost = r
            .get("cost_micro_usd")
            .and_then(|c| c.as_u64())
            .unwrap_or(0);
        let saved = r
            .get("saved_micro_usd")
            .and_then(|c| c.as_u64())
            .unwrap_or(0);
        let tasks: Vec<Value> = r
            .get("tasks")
            .and_then(|t| t.as_arr().map(|x| x.to_vec()))
            .unwrap_or_default();
        let ran = tasks
            .iter()
            .filter(|t| t.get("status").and_then(|s| s.as_str()) == Some("ran"))
            .count();
        let cached = tasks
            .iter()
            .filter(|t| t.get("status").and_then(|s| s.as_str()) == Some("cached"))
            .count();
        out.push_str(&format!(
            "  {} ran, {} served from cache, cost ${:.4}, saved ${:.4}\n",
            ran,
            cached,
            cost as f64 / 1e6,
            saved as f64 / 1e6
        ));
        out.push_str(&format!(
            "  wright {}, receipt signed: {}\n",
            r.str_of("wright").unwrap_or("?"),
            r.get("signature").is_some()
        ));
    }
    // Where the money went — from the log, so older receipts get it too.
    let spend = crate::spend::attribute(events);
    if !spend.agents.is_empty() {
        out.push_str("\nWHERE THE MONEY WENT\n");
        for l in spend.lines() {
            out.push_str(&format!("  {l}\n"));
        }
    }

    Ok(out)
}

/// The latest run's story (what the daemon's default view shows).
pub fn latest(project_dir: &Path) -> Result<String, String> {
    let store = Store::open(project_dir)?;
    let run = store
        .runs()
        .last()
        .copied()
        .ok_or("this project has no runs to explain")?;
    story_of_run(&store, run)
}

fn who(e: &Event) -> &str {
    e.agent.as_deref().unwrap_or("Wright")
}

fn first_sentence(s: &str) -> String {
    let t = s.trim();
    if t.is_empty() {
        return String::new();
    }
    match t.find(['.', '\n']) {
        Some(i) if i > 3 => t[..=i].trim_end().to_string(),
        _ => t.lines().next().unwrap_or("").trim_end().to_string(),
    }
}

fn short_time(ms: u64) -> String {
    let days = (ms / 86_400_000) as i64;
    let (y, m, d) = wright_core::schedule::civil_from_days(days);
    let secs = (ms / 1000) % 86_400;
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02} UTC",
        secs / 3600,
        (secs % 3600) / 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_sentences_are_sentences() {
        assert_eq!(first_sentence("One line only"), "One line only");
        assert_eq!(first_sentence("Two lines\nsecond line"), "Two lines");
        assert_eq!(first_sentence("It worked. Then more."), "It worked.");
        assert_eq!(first_sentence(""), "");
    }
}
