//! Approvals: how a paused tool call reaches a human and comes back.
//!
//! The gate decides THAT a call needs a human (approval policy + a tool
//! that touches the world). An [`Approver`] decides HOW to ask:
//!
//! * [`AutoDeny`] — headless and scheduled runs with nobody watching.
//!   Fail closed: an unanswered send is a send that does not happen.
//! * [`AutoApprove`] — `wright run --yes`, for people who said so.
//! * [`StoreApprover`] — posts the request to the project store and
//!   waits for any UI or CLI (`wright approve`) to answer, up to a
//!   timeout, then denies. This is how the desktop UI and a phone-sized
//!   remote answer the same request.
//! * [`TerminalApprover`] — asks on this terminal, y/N.

use std::io::{BufRead, Write};
use std::time::{Duration, Instant};

use wright_core::events::RunId;
use wright_runtime::store::Store;

/// One request for a human decision.
#[derive(Debug, Clone)]
pub struct Ask {
    pub run: RunId,
    /// 1-based request number within the run.
    pub n: u32,
    pub agent: String,
    pub tool: String,
    pub reason: String,
    /// What the agent wants to send (shortened), so the human can judge.
    pub preview: String,
}

pub trait Approver: Send + Sync {
    /// Return true to approve. Blocks until decided.
    fn decide(&self, ask: &Ask) -> bool;
    fn name(&self) -> &'static str;
}

pub struct AutoDeny;
impl Approver for AutoDeny {
    fn decide(&self, _: &Ask) -> bool {
        false
    }
    fn name(&self) -> &'static str {
        "auto-deny (nobody watching)"
    }
}

pub struct AutoApprove;
impl Approver for AutoApprove {
    fn decide(&self, _: &Ask) -> bool {
        true
    }
    fn name(&self) -> &'static str {
        "auto-approve (--yes)"
    }
}

pub struct StoreApprover {
    pub store: Store,
    pub timeout: Duration,
}

impl Approver for StoreApprover {
    fn decide(&self, ask: &Ask) -> bool {
        let reason = format!("{}\n{}", ask.reason, ask.preview);
        if self
            .store
            .post_approval(ask.run, ask.n, &ask.agent, &reason)
            .is_err()
        {
            return false;
        }
        let deadline = Instant::now() + self.timeout;
        while Instant::now() < deadline {
            if let Some(answer) = self.store.approval_answer(ask.run, ask.n) {
                return answer;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        let _ = self.store.answer_approval(ask.run, ask.n, false);
        false
    }
    fn name(&self) -> &'static str {
        "store (UI / wright approve)"
    }
}

pub struct TerminalApprover;
impl Approver for TerminalApprover {
    fn decide(&self, ask: &Ask) -> bool {
        let mut err = std::io::stderr();
        let _ = writeln!(err, "\n  APPROVAL NEEDED  {}", ask.reason);
        if !ask.preview.is_empty() {
            for l in ask.preview.lines().take(12) {
                let _ = writeln!(err, "    | {l}");
            }
        }
        let _ = write!(err, "  Allow it? [y/N] ");
        let _ = err.flush();
        let mut line = String::new();
        if std::io::stdin().lock().read_line(&mut line).is_err() {
            return false;
        }
        matches!(line.trim(), "y" | "Y" | "yes")
    }
    fn name(&self) -> &'static str {
        "terminal"
    }
}
