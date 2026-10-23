//! The kill switch, the fleet, and time to trust — end to end, through the
//! real engine and the mock provider.
//!
//! One test, in order, in its own binary: the kill switch is process-wide
//! by design (a file under WRIGHT_HOME), so nothing else may share this
//! home while it is pulled.

use std::path::PathBuf;

use wright_engine::mock::MockProvider;
use wright_engine::scheduler::{self, State};
use wright_engine::{fire, fleet, unattended, FireOptions, Trigger};
use wright_runtime::config::Config;
use wright_runtime::killswitch;

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("wright-fleet-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn opts(cfg: &Config) -> FireOptions {
    FireOptions {
        trigger: Trigger::Manual,
        approver: Box::new(wright_engine::approve::AutoApprove),
        listeners: Vec::new(),
        config: Some(cfg.clone()),
        allow_private_network: true,
        resume: None,
        no_cache: true,
    }
}

#[test]
fn stop_everything_then_earn_trust_then_change_the_plan() {
    let home = dir("home");
    std::env::set_var("WRIGHT_HOME", &home);
    std::env::set_var("WRIGHT_EPHEMERAL_SIGNING", "1");
    let mock = MockProvider::start(0).unwrap();
    let cfg = Config::parse(
        &format!(
            "provider \"mock\" kind=\"openai\" base=\"{}\" models=\"mock-*\"\nprice model=\"mock-*\" in=0 out=0\n",
            mock.base_url()
        ),
        "fleet test",
    )
    .unwrap();
    let project = dir("project");
    let blueprint = "meta { name \"Morning note\" }\n\
intent { summary \"a short note every morning\" confirmed \"v1\" }\n\
agent Writer { goal \"write a short note\" out \"note.md\" }\n\
chips { scheduling cron=\"0 8 * * *\" tz=\"UTC\" }\n\
models { tier cheap prefer=\"mock-1\" }\n\
deliverable kind=\"report\" { check \"a note exists\" }\n";
    std::fs::write(project.join("blueprint.kdl"), blueprint).unwrap();
    let project = project.canonicalize().unwrap();

    // ── the kill switch: nothing starts ──────────────────────────
    killswitch::engage("testing the switch", "test").unwrap();
    let err = fire(&project, opts(&cfg)).expect_err("a stopped Wright fires nothing");
    assert!(
        err.contains("Wright is stopped") && err.contains("testing the switch"),
        "{err}"
    );
    assert!(fleet::report(&fleet::rows()).starts_with("STOPPED"));
    killswitch::release().unwrap();

    // ── the first run registers the schedule, which WAITS ───────
    let first = fire(&project, opts(&cfg)).unwrap();
    assert!(first.ok, "{first:?}");
    let entry = scheduler::entry_for(&project).expect("the Operator registered the schedule");
    assert_eq!(
        entry.state,
        State::Awaiting,
        "a new schedule waits for the person's go-ahead"
    );
    assert!(scheduler::hold_reason(&entry).unwrap().contains("go-ahead"));

    // The fleet lists it, its last run, and that its receipt verifies.
    let rows = fleet::rows();
    let row = rows
        .iter()
        .find(|r| r.dir == project)
        .expect("in the fleet");
    assert_eq!(row.name, "Morning note");
    let last = row.last.as_ref().expect("the last run is shown");
    assert!(last.ok && last.verified, "{last:?}");
    assert!(row.held.is_some(), "not approved yet");

    // ── time to trust: approve after seeing it pass ─────────────
    let a = unattended::approve(&project, "test").unwrap();
    assert_eq!(a.road.runs, 1);
    assert_eq!(a.road.passed, 1);
    assert!(a.at_ms >= a.road.started_ms);
    assert!(unattended::describe(&a).starts_with("trusted to run unattended after"));
    let line = wright_engine::bench::trust_line(&project);
    assert!(
        line.contains("to the go-ahead to run unattended")
            && line.contains("1 run(s) before it (1 passed)"),
        "{line}"
    );
    let entry = scheduler::entry_for(&project).unwrap();
    assert_eq!(entry.state, State::Active);
    assert!(
        scheduler::hold_reason(&entry).is_none(),
        "approved: it will fire"
    );

    // Pause holds it; the kill switch holds everything.
    scheduler::set_state(&project, State::Paused).unwrap();
    assert_eq!(
        scheduler::hold_reason(&scheduler::entry_for(&project).unwrap()).as_deref(),
        Some("paused")
    );
    scheduler::set_state(&project, State::Active).unwrap();
    killswitch::engage("", "test").unwrap();
    assert!(
        scheduler::hold_reason(&scheduler::entry_for(&project).unwrap())
            .unwrap()
            .contains("stopped")
    );
    killswitch::release().unwrap();

    // ── an approval is for one plan ─────────────────────────────
    std::fs::write(
        project.join("blueprint.kdl"),
        blueprint.replace("a short note", "a longer note"),
    )
    .unwrap();
    let why = scheduler::hold_reason(&scheduler::entry_for(&project).unwrap())
        .expect("a changed plan waits");
    assert!(why.contains("plan changed"), "{why}");
    let again = unattended::approve(&project, "test").unwrap();
    assert_eq!(again.road.changes, 1, "the change is on the road");

    // A go-ahead before anything has passed is refused.
    let fresh = dir("fresh");
    std::fs::write(fresh.join("blueprint.kdl"), blueprint).unwrap();
    let err = unattended::approve(&fresh, "test").unwrap_err();
    assert!(err.contains("pass its checks first"), "{err}");

    std::env::remove_var("WRIGHT_HOME");
}
