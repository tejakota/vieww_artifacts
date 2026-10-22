//! The event spine: one append-only, deterministically-ordered log
//! per run, and everything else is a projection of it.
//!
//! The live UI projects it forward. The time-travel scrubber projects
//! it backward. Forks clone a prefix under a new run id. Resume after a
//! crash reads it to learn which tasks already finished.
//!
//! Wire form: one JSON object per line (`events.jsonl`), written and
//! fsynced by `wright_runtime::store`. This module owns the shape and
//! the (de)serialization so every reader — CLI, UI, replay — agrees.
//!
//! Determinism note: `at_ms` is recorded for humans; ordering and
//! replay use `seq` only. Same events, same projection, same pixels.

use crate::wire::json::{self, Value};

/// Run identifier. Monotonic per project; forking allocates a new one.
pub type RunId = u64;

/// Every kind of thing that can happen during a firing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    RunStarted,
    RunFinished,
    RunFailed,
    AgentStarted,
    AgentFinished,
    AgentFailed,
    AgentRetrying,
    /// A task did not run (its delivery gate refused, or a dependency failed).
    TaskSkipped,
    ToolCalled,
    ToolDenied,
    TokensRecorded,
    /// What the model said (a short excerpt; full text lives in the store).
    ModelText,
    ArtifactWritten,
    CacheHit,
    ApprovalRequested,
    ApprovalResolved,
    /// The Reviewer's verdict on the firing's work.
    Verdict,
    ScheduleRegistered,
    /// Anything else worth a line in the Event River.
    Note,
}

impl EventKind {
    pub const ALL: [EventKind; 19] = [
        EventKind::RunStarted,
        EventKind::RunFinished,
        EventKind::RunFailed,
        EventKind::AgentStarted,
        EventKind::AgentFinished,
        EventKind::AgentFailed,
        EventKind::AgentRetrying,
        EventKind::TaskSkipped,
        EventKind::ToolCalled,
        EventKind::ToolDenied,
        EventKind::TokensRecorded,
        EventKind::ModelText,
        EventKind::ArtifactWritten,
        EventKind::CacheHit,
        EventKind::ApprovalRequested,
        EventKind::ApprovalResolved,
        EventKind::Verdict,
        EventKind::ScheduleRegistered,
        EventKind::Note,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            EventKind::RunStarted => "run-started",
            EventKind::RunFinished => "run-finished",
            EventKind::RunFailed => "run-failed",
            EventKind::AgentStarted => "agent-started",
            EventKind::AgentFinished => "agent-finished",
            EventKind::AgentFailed => "agent-failed",
            EventKind::AgentRetrying => "agent-retrying",
            EventKind::TaskSkipped => "task-skipped",
            EventKind::ToolCalled => "tool-called",
            EventKind::ToolDenied => "tool-denied",
            EventKind::TokensRecorded => "tokens-recorded",
            EventKind::ModelText => "model-text",
            EventKind::ArtifactWritten => "artifact-written",
            EventKind::CacheHit => "cache-hit",
            EventKind::ApprovalRequested => "approval-requested",
            EventKind::ApprovalResolved => "approval-resolved",
            EventKind::Verdict => "verdict",
            EventKind::ScheduleRegistered => "schedule-registered",
            EventKind::Note => "note",
        }
    }

    pub fn parse(s: &str) -> Option<EventKind> {
        EventKind::ALL.iter().copied().find(|k| k.as_str() == s)
    }
}

/// One event. `note` is plain language — it feeds the Event River and
/// the "Why?" tooltips verbatim. `data` carries structured facts
/// (token counts, hashes, cost) as a JSON object string, or "".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// 1-based sequence within the run. The scrubber's coordinate.
    pub seq: u64,
    pub run: RunId,
    /// Wall-clock milliseconds since the Unix epoch (for humans only).
    pub at_ms: u64,
    pub kind: EventKind,
    /// Which agent card this belongs to, if any.
    pub agent: Option<String>,
    pub note: String,
    pub data: String,
}

impl Event {
    /// One JSON line (no trailing newline).
    pub fn to_json_line(&self) -> String {
        let mut pairs = vec![
            ("seq", Value::int(self.seq as i64)),
            ("run", Value::int(self.run as i64)),
            ("at_ms", Value::int(self.at_ms as i64)),
            ("kind", Value::str(self.kind.as_str())),
        ];
        if let Some(a) = &self.agent {
            pairs.push(("agent", Value::str(a.clone())));
        }
        pairs.push(("note", Value::str(self.note.clone())));
        if !self.data.is_empty() {
            let data = json::parse(&self.data).unwrap_or_else(|_| Value::str(self.data.clone()));
            pairs.push(("data", data));
        }
        json::to_string(&Value::obj(pairs))
    }

    /// Parse one JSON line written by [`Event::to_json_line`].
    pub fn from_json_line(line: &str) -> Result<Event, String> {
        let v = json::parse(line)?;
        let num = |k: &str| {
            v.get(k)
                .and_then(|n| n.as_u64())
                .ok_or_else(|| format!("event is missing '{k}'"))
        };
        let kind_s = v.str_of("kind").ok_or("event is missing 'kind'")?;
        Ok(Event {
            seq: num("seq")?,
            run: num("run")?,
            at_ms: num("at_ms").unwrap_or(0),
            kind: EventKind::parse(kind_s)
                .ok_or_else(|| format!("unknown event kind '{kind_s}'"))?,
            agent: v.str_of("agent").map(String::from),
            note: v.str_of("note").unwrap_or("").to_string(),
            data: v.get("data").map(json::to_string).unwrap_or_default(),
        })
    }

    /// A field of `data`, parsed. Convenience for projections.
    pub fn data_value(&self) -> Option<Value> {
        if self.data.is_empty() {
            None
        } else {
            json::parse(&self.data).ok()
        }
    }
}

/// An append-only event log for one run (in memory).
#[derive(Debug, Clone)]
pub struct EventLog {
    run: RunId,
    events: Vec<Event>,
}

/// Milliseconds since the Unix epoch, now.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl EventLog {
    pub fn new(run: RunId) -> Self {
        EventLog {
            run,
            events: Vec::new(),
        }
    }

    /// Rebuild a log from stored events (resume, replay). Sequence
    /// numbers must be contiguous from 1; anything else is corruption.
    pub fn from_events(run: RunId, events: Vec<Event>) -> Result<Self, String> {
        for (i, e) in events.iter().enumerate() {
            if e.seq != i as u64 + 1 {
                return Err(format!(
                    "event log is not contiguous at position {} (seq {})",
                    i + 1,
                    e.seq
                ));
            }
        }
        Ok(EventLog { run, events })
    }

    pub fn run(&self) -> RunId {
        self.run
    }

    /// Append an event; returns a copy of it. The ONLY way in.
    pub fn append(
        &mut self,
        kind: EventKind,
        agent: Option<&str>,
        note: impl Into<String>,
    ) -> Event {
        self.append_with(kind, agent, note, String::new())
    }

    /// Append with structured data (a JSON object string).
    pub fn append_with(
        &mut self,
        kind: EventKind,
        agent: Option<&str>,
        note: impl Into<String>,
        data: String,
    ) -> Event {
        let e = Event {
            seq: self.events.len() as u64 + 1,
            run: self.run,
            at_ms: now_ms(),
            kind,
            agent: agent.map(String::from),
            note: note.into(),
            data,
        };
        self.events.push(e.clone());
        e
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// Events up to and including `seq` — a past snapshot of the run.
    pub fn snapshot_at(&self, seq: u64) -> EventLog {
        EventLog {
            run: self.run,
            events: self.events.iter().take(seq as usize).cloned().collect(),
        }
    }

    /// Fork: clone the prefix up to `seq` under a new run id, with a
    /// provenance event pointing back at the parent.
    pub fn fork_at(&self, seq: u64, new_run: RunId) -> EventLog {
        let mut forked = self.snapshot_at(seq);
        forked.run = new_run;
        for e in &mut forked.events {
            e.run = new_run;
        }
        forked.append(
            EventKind::RunStarted,
            None,
            format!("forked from run {} at event {}", self.run, seq),
        );
        forked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_numbers_are_contiguous() {
        let mut log = EventLog::new(1);
        log.append(EventKind::RunStarted, None, "firing 1");
        log.append(EventKind::AgentStarted, Some("Fetcher"), "go");
        log.append(EventKind::AgentFinished, Some("Fetcher"), "done");
        let seqs: Vec<u64> = log.events().iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![1, 2, 3]);
    }

    #[test]
    fn json_lines_round_trip() {
        let mut log = EventLog::new(4);
        log.append_with(
            EventKind::TokensRecorded,
            Some("Fetcher"),
            "1,200 tokens",
            r#"{"in":1000,"out":200}"#.into(),
        );
        log.append(EventKind::Note, None, "a \"quoted\"\nline");
        for e in log.events() {
            let back = Event::from_json_line(&e.to_json_line()).unwrap();
            assert_eq!(&back, e);
        }
        assert_eq!(
            log.events()[0]
                .data_value()
                .unwrap()
                .get("in")
                .unwrap()
                .as_u64(),
            Some(1000)
        );
    }

    #[test]
    fn corrupt_logs_are_refused() {
        let mut log = EventLog::new(1);
        log.append(EventKind::RunStarted, None, "a");
        let mut evs = log.events().to_vec();
        evs[0].seq = 2;
        assert!(EventLog::from_events(1, evs).is_err());
    }

    #[test]
    fn fork_carries_provenance() {
        let mut log = EventLog::new(3);
        log.append(EventKind::RunStarted, None, "a");
        log.append(EventKind::AgentStarted, Some("Mailer"), "b");
        let fork = log.fork_at(1, 9);
        assert_eq!(fork.run(), 9);
        assert_eq!(fork.len(), 2);
        assert!(fork.events().iter().all(|e| e.run == 9));
        assert!(fork
            .events()
            .last()
            .unwrap()
            .note
            .contains("forked from run 3"));
    }
}
