//! `wright handoff` — the memo's item 7: "hand to a developer".
//!
//! Export already makes a zip, wheel, crate or web build when a run
//! passes. What was missing is the OTHER audience: a professional who
//! will take the project over, and needs exactly three things to start
//! working — the plan (what this is), the proof (receipts that say what
//! ran and that it was checked), and the work itself (the files, with
//! the story of who changed what).
//!
//! The handoff is a FOLDER, because that is what a pull request is:
//!
//! ```text
//! handoff-<date>-<run>/
//!   HANDOFF.md          one page: what this is, how it is checked, what
//!                       the last run did, and where everything lives
//!   blueprint.kdl       the exact committed plan the receipts name
//!   receipts/           every run's receipt.json, verifiable by anyone
//!   project.patch       git diff of the project vs its first commit
//!   tests/              copies of the checks Testwright wrote, if any
//! ```
//!
//! `HANDOFF.md` is generated from the store — receipts, events, verdicts
//! — never from a model, for the same reason the Confirm Gate is: a
//! person deciding whether to take over deserves typed facts.

use std::path::{Path, PathBuf};

use wright_core::hash;
use wright_runtime::store::Store;

/// Write the handoff folder for the latest (or a named) run. Returns
/// the folder path (for the CLI to print and open).
pub fn handoff(project_dir: &Path, run: Option<u64>) -> Result<PathBuf, String> {
    let store = Store::open(project_dir)?;
    let run = run
        .or_else(|| store.runs().last().copied())
        .ok_or("this project has no runs to hand off")?;
    let receipt = std::fs::read_to_string(store.run_dir(run).join("receipt.json"))
        .map_err(|e| format!("run {run} has no receipt: {e}"))?;

    let date = wright_engine_firing_today();
    let dir = project_dir.join(format!("handoff-{date}-run{run}"));
    let receipts = dir.join("receipts");
    std::fs::create_dir_all(&receipts).map_err(|e| format!("{}: {e}", receipts.display()))?;

    // The exact committed blueprint the receipt names.
    let commit = wright_core::wire::json::parse(&receipt)
        .ok()
        .and_then(|v| v.str_of("blueprint").map(String::from))
        .unwrap_or_default();
    if commit.is_empty() {
        return Err(
            "the receipt names no blueprint commit; cannot hand off a plan nobody can reproduce"
                .into(),
        );
    }
    let blueprint_text = store
        .blueprint_at(&commit)
        .map_err(|e| format!("the committed blueprint is missing from the store: {e}"))?;
    let bp = wright_core::blueprint::parse::blueprint(&blueprint_text)
        .map_err(|e| format!("the committed blueprint no longer parses: {e}"))?;
    std::fs::write(dir.join("blueprint.kdl"), &blueprint_text)
        .map_err(|e| format!("write the blueprint into the handoff: {e}"))?;

    // Every run's receipt: the professional judges the whole history.
    for r in store.runs() {
        let from = store.run_dir(r).join("receipt.json");
        if from.exists() {
            let to = receipts.join(format!("run-{r}.receipt.json"));
            std::fs::copy(&from, &to).map_err(|e| format!("copy run {r}'s receipt: {e}"))?;
        }
    }

    // The project's work as one patch: diff against the first commit
    // (the Shipwright's skeleton), so the patch IS "what the agents did".
    let patch = project_patch(project_dir);
    if let Some(p) = &patch {
        std::fs::write(dir.join("project.patch"), p)
            .map_err(|e| format!("write project.patch: {e}"))?;
    }

    // The checks, when the tests chip wrote any.
    copy_tests(project_dir, &dir);

    // The page itself.
    let md = handoff_md(&store, &bp, &receipt, run, &commit, patch.as_deref());
    let md_path = dir.join("HANDOFF.md");
    std::fs::write(&md_path, md).map_err(|e| format!("{}: {e}", md_path.display()))?;
    Ok(dir)
}

fn wright_engine_firing_today() -> String {
    crate::firing::today_utc()
}

/// `git diff` of the working tree against the FIRST commit — the delta
/// of everything the agents did since the skeleton. None when git is
/// missing or the project has no history (still useful, just quieter).
fn project_patch(project_dir: &Path) -> Option<String> {
    // No shell substitution in std: find the root commit first, then diff
    // against it. A project without git (or without history) hands off
    // without a patch, and says so on the page.
    let roots = std::process::Command::new("git")
        .args(["rev-list", "--max-parents=0", "HEAD"])
        .current_dir(project_dir)
        .output()
        .ok()?;
    if !roots.status.success() {
        return None;
    }
    let root = String::from_utf8_lossy(&roots.stdout)
        .lines()
        .next()?
        .trim()
        .to_string();
    if root.is_empty() {
        return None;
    }
    let out = std::process::Command::new("git")
        .args(["diff", "--no-color", &root])
        .current_dir(project_dir)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

fn copy_tests(project_dir: &Path, dir: &Path) {
    let from = project_dir.join("tests");
    if !from.is_dir() {
        return;
    }
    let to = dir.join("tests");
    let _ = copy_dir(&from, &to);
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)?.flatten() {
        let path = e.path();
        if e.file_type().map(|t| t.is_file()).unwrap_or(false) {
            let _ = std::fs::copy(&path, to.join(e.file_name()));
        }
    }
    Ok(())
}

fn handoff_md(
    store: &Store,
    bp: &wright_core::blueprint::Blueprint,
    receipt: &str,
    run: u64,
    commit: &str,
    patch: Option<&str>,
) -> String {
    let v = wright_core::wire::json::parse(receipt).unwrap_or(wright_core::wire::json::Value::Null);
    let mut out = String::new();
    out.push_str(&format!("# Handoff: {}\n\n", bp.meta.name));
    out.push_str(&format!(
        "You are taking over a project that an agent harness (Wright {}) built and ran.\n\
         This folder has the plan, the proof and the work; nothing here asks you to trust a chat log.\n\n",
        v.str_of("wright").unwrap_or("")
    ));
    out.push_str("## What this is\n\n");
    out.push_str(&format!("In the owner's words: {}\n\n", bp.intent.summary));
    out.push_str("## The plan\n\n");
    out.push_str(&format!("The exact blueprint the receipts name is `blueprint.kdl` in this folder (commit `{}`). It declares:\n\n",
        hash::short(commit)));
    for a in &bp.agents {
        out.push_str(&format!(
            "- **{}** — {} ({}{})\n",
            a.name,
            a.goal,
            if a.inputs.is_empty() {
                String::new()
            } else {
                format!("reads {}, ", a.inputs.join(", "))
            },
            if a.outputs.is_empty() {
                "writes nothing".to_string()
            } else {
                format!("writes {}", a.outputs.join(", "))
            }
        ));
    }
    for d in &bp.deliverables {
        out.push_str(&format!("- Done means: {}\n", d.check));
        for (i, s) in d.accept.iter().enumerate() {
            out.push_str(&format!("  - promise {}: {}\n", i + 1, s));
        }
    }
    out.push_str("\n## The proof\n\n");
    out.push_str(&format!(
        "The latest run ({run}) ended: {}\n\n",
        v.str_of("headline").unwrap_or("(no headline)")
    ));
    match v.get("verdict").and_then(|x| x.as_str()) {
        Some("approve") => {
            out.push_str("The Reviewer approved the work; every acceptance sentence passed.\n\n")
        }
        Some(other) => out.push_str(&format!("The verdict was: {other}\n\n")),
        None => out.push_str("No Reviewer verdict was recorded (the tests chip did not run).\n\n"),
    }
    out.push_str(&format!(
        "Every run's receipt is in `receipts/`. Check any of them without Wright:\n\
         the receipts are signed (Ed25519) over their exact bytes, and name the\n\
         event log's BLAKE3, every artifact's hash and the blueprint commit {}\n\n",
        hash::short(commit)
    ));
    out.push_str("## The work\n\n");
    match patch {
        Some(_) => out.push_str("`project.patch` is the full diff of the project against its first commit — every change the agents made, in reviewable form.\n\n"),
        None => out.push_str("The project has no git history, so there is no patch; the working tree in the original folder is the work.\n\n"),
    }
    out.push_str("## Where things run\n\n");
    out.push_str(&format!(
        "Project folder: {}\nWright's records: {}\n\n",
        store.project_dir.display(),
        store.root().display()
    ));
    out.push_str(&format!(
        "To re-run the checks: `cd {} && wright run --no-cache`\n",
        store.project_dir.display()
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("wright-handoff-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_project_with_a_run_hands_off_its_three_things() {
        let _guard = crate::test_support::env_lock();
        std::env::set_var("WRIGHT_HOME", tmp("home"));
        let dir = tmp("full");
        let store = Store::open(&dir).unwrap();
        // a minimal committed blueprint + a receipt for run 1
        let bp_text = "meta { name \"handoff-demo\" }\nintent { summary \"a digest\" confirmed \"v1\" }\nagent A { goal \"make it\" out \"out.txt\" }\ndeliverable kind=\"report\" { check \"it exists\" accept \"the file says hello\" }\n";
        let bp = wright_core::blueprint::parse::blueprint(bp_text).unwrap();
        let commit = store
            .commit_blueprint(
                &wright_core::blueprint::canon::canonical(&bp),
                "handoff test",
            )
            .unwrap();
        let run = 1;
        fs::create_dir_all(store.run_dir(run)).unwrap();
        let receipt = format!(
            "{{\"format\":\"wright.run-receipt/2\",\"run\":{run},\"blueprint\":\"{commit}\",\"headline\":\"done: 1 ran\",\"verdict\":\"approve\",\"wright\":\"1.0.0\",\"tasks\":[],\"artifacts\":[],\"inputs\":[]}}"
        );
        fs::write(store.run_dir(run).join("receipt.json"), &receipt).unwrap();
        drop(store);

        let out = handoff(&dir, Some(run)).unwrap();
        std::env::remove_var("WRIGHT_HOME");
        let md = fs::read_to_string(out.join("HANDOFF.md")).unwrap();
        assert!(md.contains("What this is"), "{md}");
        assert!(md.contains("In the owner's words: a digest"), "{md}");
        assert!(md.contains("promise 1: the file says hello"), "{md}");
        assert!(md.contains("The Reviewer approved the work"), "{md}");
        assert!(fs::read_to_string(out.join("blueprint.kdl"))
            .unwrap()
            .contains("handoff-demo"));
        assert!(out.join("receipts").join("run-1.receipt.json").exists());
    }

    #[test]
    fn a_project_without_runs_refuses_with_a_sentence() {
        let _guard = crate::test_support::env_lock();
        std::env::set_var("WRIGHT_HOME", tmp("home2"));
        let dir = tmp("empty");
        let err = handoff(&dir, None).unwrap_err();
        assert!(err.contains("no runs"), "{err}");
        std::env::remove_var("WRIGHT_HOME");
    }
}
