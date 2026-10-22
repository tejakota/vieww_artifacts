//! The public benchmark (ADR-030): time to a checked outcome, and how
//! much human attention it took.
//!
//! The memo's ask, verbatim: "Time to a checked outcome and human
//! interventions per recipe, run on every release, for Wright and for
//! the agents it hosts." What that means concretely, and what this
//! module measures:
//!
//! * **Time to a checked outcome**: wall-clock from `RunStarted` to a
//!   receipt whose verdict is `approve` — not "the model answered", the
//!   OUTCOME CHECKED. A run that fails counts as no outcome, honestly.
//! * **Human interventions**: every `ApprovalRequested` the run raised —
//!   each one is a human who had to stop and decide. Zero is the number
//!   an unattended run is allowed to have.
//!
//! The harness is deliberately built from the machine's own parts: it
//! fires the engine's real `fire()` (provider, gate, sandbox, checks,
//! receipt — the whole spine), reads the receipt and the event log back,
//! and reports. `wright bench` runs it over one project, N times, and
//! prints the table; the release gate (docs/BENCHMARK.md) records the
//! same numbers on a machine that can run the sandbox fully.
//!
//! * **Time to trust** — the number that matters most for a person who
//!   does not write code: from the first version of the plan to the
//!   moment they approved it to run unattended (`crate::unattended`),
//!   with the runs, failures, approvals and plan changes it took. A
//!   checked outcome is a step on that road; the go-ahead is the end.
//!
//! Benchmarks that lie are worse than no benchmarks, so two rules hold:
//! the tool never reports a median it cannot back with every sample, and
//! every reported run's receipt remains in the store for re-verification
//! (`wright verify`), because a benchmark number nobody can audit is
//! marketing, not measurement.

use std::path::Path;

use wright_core::events::EventKind;
use wright_runtime::store::Store;

use crate::firing::{fire, FireOptions, Trigger};

/// One measured run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sample {
    pub run: u64,
    /// Milliseconds from RunStarted to a checked outcome.
    pub outcome_ms: Option<u64>,
    /// True when the Reviewer approved.
    pub checked: bool,
    /// Approval requests a human had to answer.
    pub interventions: u64,
}

impl Sample {
    /// The one-line record: what the table prints.
    pub fn line(&self) -> String {
        match (self.checked, self.outcome_ms) {
            (true, Some(ms)) => format!(
                "run {}: checked outcome in {:.1}s, {} intervention(s)",
                self.run,
                ms as f64 / 1000.0,
                self.interventions
            ),
            (true, None) => format!("run {}: approved but the clock is missing", self.run),
            (false, _) => format!(
                "run {}: NO checked outcome ({} intervention(s)) — an honest failure, counted",
                self.run, self.interventions
            ),
        }
    }
}

/// The whole measurement over a project: the per-run samples, and the
/// summary a release notes can quote.
#[derive(Debug, Clone)]
pub struct Report {
    pub samples: Vec<Sample>,
    /// Median time over the runs that reached a checked outcome.
    pub median_outcome_ms: Option<u64>,
    /// Runs that ended approved / total runs.
    pub checked_share: f64,
    /// Total interventions across all runs.
    pub interventions: u64,
    /// Time to trust, in words: the recorded road to the go-ahead, or the
    /// road so far when there is none yet.
    pub trust: String,
}

impl Report {
    pub fn table(&self) -> String {
        let mut out = String::from("time to a checked outcome — human interventions per run\n");
        for s in &self.samples {
            out.push_str(&format!("  {}\n", s.line()));
        }
        match self.median_outcome_ms {
            Some(ms) => out.push_str(&format!(
                "\nmedian time to a checked outcome: {:.1}s over {} of {} run(s); {} intervention(s) in total",
                ms as f64 / 1000.0,
                self.samples.iter().filter(|s| s.checked).count(),
                self.samples.len(),
                self.interventions
            )),
            None => out.push_str(&format!("\nno run reached a checked outcome ({} run(s) measured)", self.samples.len())),
        }
        out.push_str("\n\ntime to trust: ");
        out.push_str(&self.trust);
        out.push('\n');
        out
    }
}

/// Fire the project `runs` times (default 3) through the real engine and
/// measure. The receipt of every measured run stays in the store.
pub fn run(project_dir: &Path, runs: u32) -> Result<Report, String> {
    let runs = runs.max(1);
    let mut samples = Vec::new();
    for i in 0..runs {
        let summary = fire(
            project_dir,
            FireOptions {
                trigger: Trigger::Bench,
                approver: Box::new(crate::approve::AutoApprove),
                listeners: Vec::new(),
                config: None,
                allow_private_network: false,
                resume: None,
                no_cache: i == 0,
            },
        )?;
        let store = Store::open(project_dir)?;
        let log = store.read_events(summary.run)?;
        let events = log.events();
        let started = events
            .iter()
            .find(|e| e.kind == EventKind::RunStarted)
            .map(|e| e.at_ms);
        let finished = events
            .iter()
            .find(|e| e.kind == EventKind::RunFinished)
            .map(|e| e.at_ms);
        let outcome_ms = match (started, finished, summary.ok) {
            (Some(s), Some(f), true) if f > s => Some(f - s),
            _ => None,
        };
        let interventions = events
            .iter()
            .filter(|e| e.kind == EventKind::ApprovalRequested)
            .count() as u64;
        samples.push(Sample {
            run: summary.run,
            outcome_ms,
            checked: summary.ok && summary.verdict.as_ref().map(|v| v.is_ok()).unwrap_or(false),
            interventions,
        });
    }
    let mut report = summarize(samples);
    report.trust = trust_line(project_dir);
    Ok(report)
}

/// The time-to-trust sentence for a project.
pub fn trust_line(project_dir: &Path) -> String {
    use crate::unattended;
    match unattended::recorded(project_dir) {
        Some(a) => {
            let r = &a.road;
            format!(
                "{} from the first plan to the go-ahead to run unattended — {} run(s) before it ({} passed), {} approval(s) asked ({} refused), {} change(s) to the plan{}",
                unattended::human_duration(a.time_to_trust_ms()),
                r.runs,
                r.passed,
                r.asked,
                r.refused,
                r.changes,
                match unattended::current(project_dir) {
                    Ok(_) => String::new(),
                    Err(why) => format!(" (now held: {why})"),
                }
            )
        }
        None => match unattended::road(project_dir) {
            Ok(r) => format!(
                "not trusted to run unattended yet — {} so far, {} run(s) ({} passed), {} approval(s) asked, {} change(s) to the plan",
                unattended::human_duration(wright_core::events::now_ms().saturating_sub(r.started_ms)),
                r.runs,
                r.passed,
                r.asked,
                r.changes
            ),
            Err(e) => format!("unknown ({e})"),
        },
    }
}

fn summarize(samples: Vec<Sample>) -> Report {
    let mut checked_times: Vec<u64> = samples
        .iter()
        .filter(|s| s.checked)
        .filter_map(|s| s.outcome_ms)
        .collect();
    checked_times.sort_unstable();
    // Lower median: with an even number of samples we report the
    // pessimistic half, because a benchmark that flatters itself is a
    // lie with decimal places.
    let median_outcome_ms = if checked_times.is_empty() {
        None
    } else {
        Some(checked_times[(checked_times.len() - 1) / 2])
    };
    let checked = samples.iter().filter(|s| s.checked).count();
    let interventions = samples.iter().map(|s| s.interventions).sum();
    let total = samples.len();
    Report {
        samples,
        median_outcome_ms,
        checked_share: if total == 0 {
            0.0
        } else {
            checked as f64 / total as f64
        },
        interventions,
        trust: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_tells_the_truth_about_failures() {
        let r = summarize(vec![
            Sample {
                run: 1,
                outcome_ms: Some(10_000),
                checked: true,
                interventions: 0,
            },
            Sample {
                run: 2,
                outcome_ms: Some(30_000),
                checked: true,
                interventions: 1,
            },
            Sample {
                run: 3,
                outcome_ms: None,
                checked: false,
                interventions: 2,
            },
        ]);
        assert_eq!(
            r.median_outcome_ms,
            Some(10_000),
            "median of 10k, 30k over the checked runs"
        );
        assert_eq!(r.interventions, 3);
        let table = r.table();
        assert!(
            table.contains("run 1: checked outcome in 10.0s, 0 intervention(s)"),
            "{table}"
        );
        assert!(table.contains("run 3: NO checked outcome"), "{table}");
        assert!(
            table.contains("median time to a checked outcome: 10.0s over 2 of 3"),
            "{table}"
        );
    }

    #[test]
    fn a_run_without_outcomes_says_so() {
        let r = summarize(vec![Sample {
            run: 1,
            outcome_ms: None,
            checked: false,
            interventions: 0,
        }]);
        assert!(r.median_outcome_ms.is_none());
        assert!(
            r.table().contains("no run reached a checked outcome"),
            "{}",
            r.table()
        );
    }
}
