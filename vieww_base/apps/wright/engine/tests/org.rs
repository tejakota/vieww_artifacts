//! The admin console end to end: a signed organization policy refuses a
//! plan that breaks it, fences the network, holds unattended runs for an
//! administrator's approval, fails closed when tampered with — and the
//! fleet verification finds an edited receipt.
//!
//! Its own binary: the policy folder and WRIGHT_HOME are process-wide.

use std::path::PathBuf;

use wright_engine::mock::MockProvider;
use wright_engine::scheduler;
use wright_engine::{fire, org, unattended, FireOptions, Trigger};
use wright_runtime::config::Config;

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("wright-org-{tag}-{}", std::process::id()));
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

const ADMIN: [u8; 32] = [42u8; 32];

#[test]
fn a_signed_policy_governs_plans_network_and_unattended_runs() {
    if wright_runtime::policy::system_dir()
        .join("policy.kdl")
        .exists()
    {
        eprintln!("skipped: this machine has a real organization policy");
        return;
    }
    let home = dir("home");
    std::env::set_var("WRIGHT_HOME", &home);
    std::env::set_var("WRIGHT_EPHEMERAL_SIGNING", "1");
    let mock = MockProvider::start(0).unwrap();
    let cfg = Config::parse(
        &format!("provider \"mock\" kind=\"openai\" base=\"{}\" models=\"mock-*\"\nprice model=\"mock-*\" in=0 out=0\n", mock.base_url()),
        "org test",
    )
    .unwrap();

    // ── the policy, signed by the administrator ─────────────────
    let pdir = dir("policy");
    let admin_pk = wright_runtime::signing::to_hex(&wright_core::ed25519::public_key(&ADMIN));
    std::fs::write(
        pdir.join("admins.kdl"),
        format!("admin \"IT security\" key=\"{admin_pk}\"\n"),
    )
    .unwrap();
    let body = "policy \"Acme IT\" version=1\n\
budget max-per-run=\"0.50\"\n\
tools forbid=\"shell\"\n\
network allow=\"hn.algolia.com\"\n\
providers allow=\"mock\"\n\
require tests=true unattended=\"admin\"\n";
    let signed = wright_runtime::policy::sign(body, &ADMIN, "IT security").unwrap();
    std::fs::write(pdir.join("policy.kdl"), &signed).unwrap();
    std::env::set_var("WRIGHT_POLICY_DIR", &pdir);

    // ── a plan that breaks it does not fire, and says why ───────
    let project = dir("project");
    let bp = |budget: &str, tests: &str, extra: &str| {
        format!(
            "meta {{ name \"Morning note\" }}\n\
intent {{ summary \"a note\" confirmed \"v1\" }}\n\
agent Writer {{ goal \"write a short note\" out \"note.md\" }}\n{extra}\
chips {{ scheduling cron=\"0 8 * * *\" tz=\"UTC\"{tests} }}\n\
guardrails {{ budget per-firing={budget} daily=5000000 }}\n\
models {{ tier cheap prefer=\"mock-1\" fallback=\"\" tier quality prefer=\"mock-1\" fallback=\"\" }}\n\
deliverable kind=\"report\" {{ check \"a note exists\" }}\n"
        )
    };
    std::fs::write(project.join("blueprint.kdl"), bp("1000000", "", "")).unwrap();
    let project = project.canonicalize().unwrap();
    let err = fire(&project, opts(&cfg)).expect_err("refused");
    assert!(
        err.contains("Acme IT") && err.contains("$0.50") && err.contains("tested"),
        "{err}"
    );

    // Within the rules, it fires.
    std::fs::write(project.join("blueprint.kdl"), bp("400000", " tests on", "")).unwrap();
    let first = fire(&project, opts(&cfg)).unwrap();
    assert!(first.ok, "{first:?}");

    // ── the network is fenced at the tool, not by the prompt ────
    let fenced = dir("fenced");
    std::fs::write(
        fenced.join("blueprint.kdl"),
        bp(
            "400000",
            " tests on",
            "agent Fetcher { goal \"fetch the list from http://127.0.0.1:9/list\" tools \"http-read\" out \"list.json\" permissions network=true }\n",
        )
        .replace("chips { scheduling cron=\"0 8 * * *\" tz=\"UTC\" tests on }", "chips { tests on }"),
    )
    .unwrap();
    let run = fire(&fenced, opts(&cfg)).unwrap();
    let log = std::fs::read_to_string(run.receipt.with_file_name("events.jsonl")).unwrap();
    assert!(
        log.contains("not on your organization's network allow-list"),
        "the request was refused at the tool"
    );

    // ── unattended runs wait for the administrator ──────────────
    unattended::approve(&project, "test").unwrap();
    let why = scheduler::hold_reason(&scheduler::entry_for(&project).unwrap()).expect("held");
    assert!(why.contains("Acme IT's approval"), "{why}");
    org::approve(&project, &ADMIN, "IT security").unwrap();
    assert!(
        scheduler::hold_reason(&scheduler::entry_for(&project).unwrap()).is_none(),
        "approved by both"
    );
    // A forged approval (a key not in admins.kdl) is not one.
    org::approve(&project, &[7u8; 32], "IT security").unwrap();
    assert!(org::approval(&project)
        .unwrap_err()
        .contains("changed after it was signed"));
    org::approve(&project, &ADMIN, "IT security").unwrap();
    // A changed plan needs a new approval.
    let text = std::fs::read_to_string(project.join("blueprint.kdl")).unwrap();
    std::fs::write(
        project.join("blueprint.kdl"),
        text.replace("a short note", "a longer note"),
    )
    .unwrap();
    assert!(org::approval(&project)
        .unwrap_err()
        .contains("plan changed"));

    // ── fleet verification finds an edited receipt ──────────────
    let checks = org::fleet_verify(&[]);
    assert!(
        checks.iter().any(|c| c.project == project && c.ok),
        "{checks:?}"
    );
    let receipt = std::fs::read_to_string(&first.receipt).unwrap();
    std::fs::write(
        &first.receipt,
        receipt.replace("Morning note", "Mourning note"),
    )
    .unwrap();
    let checks = org::fleet_verify(&[]);
    let bad = checks
        .iter()
        .find(|c| c.project == project && c.run == first.run)
        .unwrap();
    assert!(!bad.ok);
    assert!(org::fleet_report(&checks, false).contains("do NOT"));
    assert!(org::fleet_report(&checks, true)
        .lines()
        .all(|l| l.starts_with('{')));

    // ── a loosened policy stops everything ──────────────────────
    std::fs::write(pdir.join("policy.kdl"), signed.replace("0.50", "50.00")).unwrap();
    let err = fire(&project, opts(&cfg)).expect_err("fail closed");
    assert!(err.contains("cannot be trusted"), "{err}");

    std::env::remove_var("WRIGHT_POLICY_DIR");
    std::env::remove_var("WRIGHT_HOME");
}
