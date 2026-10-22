//! Trust, end to end: the memo's core promise, run for real.
//!
//! Everything here goes through the WHOLE machine — the mock provider
//! over real HTTP, the compiled graph, the gate, the cache, the receipt
//! — with one deliberate omission: no `shell`/`run-tests` call happens
//! (the project has no test command), because those wrap in the kernel
//! sandbox, which this CI kernel (5.10) does not have. The suite that
//! exercises them is `e2e.rs`, which fails on such kernels by design
//! (the sandbox fails closed); this file proves the TRUST machinery on
//! any machine.
//!
//! What a run here proves, in order:
//!
//! * a plan with typed edges and acceptance sentences fires and passes;
//! * the receipt is v2, signed, records inputs and promises;
//! * `verify` passes its five checks — and fails after one edited byte;
//! * `explain` tells the story in one screen;
//! * the Rule of Two asks (or refuses) as designed;
//! * a worker step runs an external stub and applies its diff whole.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::OnceLock;

use wright_engine::mock::MockProvider;
use wright_engine::{fire, verify as trust_verify, FireOptions, TaskStatus, Trigger};
use wright_runtime::config::Config;
use wright_runtime::store::Store;

fn home(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("wright-trust-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// One provider for the whole binary (env vars are process-wide);
/// the WRIGHT_HOME is set once and tests use their own project dirs.
fn mock_env() -> &'static (PathBuf, MockProvider) {
    static ENV: OnceLock<(PathBuf, MockProvider)> = OnceLock::new();
    ENV.get_or_init(|| {
        let h = std::env::temp_dir().join(format!("wright-trust-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&h);
        std::fs::create_dir_all(&h).unwrap();
        std::env::set_var("WRIGHT_HOME", &h);
        std::env::set_var("WRIGHT_EPHEMERAL_SIGNING", "1");
        (h, MockProvider::start(0).unwrap())
    })
}

fn config(prices: &str) -> Config {
    let (_, mock) = mock_env();
    Config::parse(
        &format!(
            "provider \"mock\" kind=\"openai\" base=\"{}\" models=\"mock-*\"\ninterview model=\"mock-1\"\nprice model=\"mock-*\" in=0 out=0\n{prices}",
            mock.base_url()
        ),
        "trust test config",
    )
    .unwrap()
}

/// A tiny HTTP site: GET returns `body`.
fn site(body: &'static str) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut s = stream;
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut line = String::new();
            loop {
                line.clear();
                if r.read_line(&mut line).unwrap() == 0 || line.trim().is_empty() {
                    break;
                }
            }
            let resp = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            let _ = s.write_all(resp.as_bytes());
        }
    });
    port
}

fn project(tag: &str, blueprint: &str) -> PathBuf {
    let dir = home(tag);
    std::fs::write(dir.join("blueprint.kdl"), blueprint).unwrap();
    dir
}

fn fire_once(dir: &std::path::Path, cfg: Config) -> wright_engine::RunSummary {
    fire(
        dir,
        FireOptions {
            trigger: Trigger::Manual,
            approver: Box::new(wright_engine::approve::AutoApprove),
            listeners: Vec::new(),
            config: Some(cfg),
            allow_private_network: true,
            resume: None,
            no_cache: false,
        },
    )
    .unwrap()
}

#[test]
fn a_checked_outcome_signs_its_receipt_and_verifies() {
    let (home_dir, _) = mock_env();
    let _ = &home_dir;
    let port = site("[{\"title\":\"a story\"}]");
    let dir = project(
        "checked",
        &format!(
            "meta {{ name \"digest-demo\" }}\n\
intent {{ summary \"a tiny digest\" confirmed \"v1\" }}\n\
agent Fetcher {{ goal \"fetch the list from http://127.0.0.1:{port}\" tools \"http-read\" out \"stories.json\" permissions network=true }}\n\
agent Summarizer {{ goal \"sum it up\" in \"stories.json\" out \"digest.md\" }}\n\
edge from=\"Fetcher\" to=\"Summarizer\" out=\"stories.json\" in=\"stories.json\" schema=\"json-object\"\n\
chips {{ tests on }}\n\
models {{ tier cheap prefer=\"mock-1\" }}
deliverable kind=\"report\" {{ check \"a digest exists\" accept \"the digest says a story\" }}\n"
        ),
    );
    let summary = fire_once(&dir, config(""));

    assert!(summary.ok, "the run must pass: {summary:?}");
    assert!(
        summary.verdict.as_ref().map(|v| v.is_ok()).unwrap_or(false),
        "the Reviewer approved"
    );
    // both artifacts exist, and the typed edge's promise was kept
    assert!(dir.join("artifacts/stories.json").exists());
    assert!(dir.join("artifacts/digest.md").exists());

    // the receipt is v2 and signed
    let receipt = std::fs::read_to_string(&summary.receipt).unwrap();
    assert!(receipt.contains("wright.run-receipt/2"), "{receipt}");
    assert!(receipt.contains("\"signature\""), "{receipt}");
    assert!(
        receipt.contains("\"accepts\""),
        "promises are recorded: {receipt}"
    );
    assert!(
        receipt.contains("\"inputs\""),
        "inputs are recorded: {receipt}"
    );
    assert!(
        receipt.contains("the digest says a story"),
        "the sentence itself is in the receipt"
    );

    // all five checks pass
    let v = trust_verify::project_run(&dir, Some(summary.run)).unwrap();
    assert!(v.ok, "{}", v.report());

    // one edited byte and the receipt fails verification
    let tampered = receipt.replace("digest-demo", "digest-DAMAGED");
    std::fs::write(&summary.receipt, &tampered).unwrap();
    let v = trust_verify::project_run(&dir, Some(summary.run)).unwrap();
    assert!(!v.ok, "tampering must be caught");
    assert!(
        v.report().contains("signature"),
        "the signature names it: {}",
        v.report()
    );

    // A forger edits the receipt and RE-SIGNS it with a fresh key of
    // their own: the bytes and the signature agree, but nobody pinned
    // that key — so the receipt still fails, and says why.
    use wright_core::wire::json::{self, Value};
    let parsed = json::parse(&tampered).unwrap();
    let Value::Obj(fields) = parsed else { panic!("a receipt is an object") };
    let body: Vec<(String, Value)> = fields.iter().filter(|(k, _)| k != "signature").cloned().collect();
    let bytes = json::to_string(&Value::Obj(body.clone()));
    let forger = [9u8; 32];
    let forger_pub: String = wright_core::ed25519::public_key(&forger).iter().map(|b| format!("{b:02x}")).collect();
    let sig: String = wright_core::ed25519::sign(&forger, bytes.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
    let mut resigned = body;
    resigned.push((
        "signature".into(),
        Value::obj(vec![
            ("public_key", Value::str(forger_pub.clone())),
            ("value", Value::str(sig)),
            ("provenance", Value::str("os-keychain")),
        ]),
    ));
    std::fs::write(&summary.receipt, json::to_string(&Value::Obj(resigned))).unwrap();
    let v = trust_verify::project_run(&dir, Some(summary.run)).unwrap();
    assert!(!v.ok, "a stranger's signature must not verify: {}", v.report());
    assert!(v.report().contains("not one this machine trusts"), "{}", v.report());
    // ...and pinning that key on purpose is a decision the verifier makes
    let v = trust_verify::project_run_pinned(&dir, Some(summary.run), std::slice::from_ref(&forger_pub)).unwrap();
    assert!(
        v.checks.iter().find(|c| c.name == "signature").is_some_and(|c| c.ok),
        "a pinned key's signature is accepted: {}",
        v.report()
    );

    // and the explain view tells the story
    let story = wright_engine::explain::latest(&dir).unwrap();
    assert!(story.contains("WHAT RAN"), "{story}");
    assert!(story.contains("promise 1"), "{story}");
    assert!(story.contains("kept"), "{story}");
}

#[test]
fn a_broken_handoff_stops_the_plan_before_the_consumer_runs() {
    let (home_dir, _) = mock_env();
    let _ = &home_dir;
    let port = site("not json at all");
    let dir = project(
        "edge",
        &format!(
            "meta {{ name \"edge-demo\" }}\n\
intent {{ summary \"a broken promise\" confirmed \"v1\" }}\n\
agent Fetcher {{ goal \"fetch from http://127.0.0.1:{port}\" tools \"http-read\" out \"stories.json\" permissions network=true }}\n\
agent Summarizer {{ goal \"sum it up\" in \"stories.json\" out \"digest.md\" }}\n\
edge from=\"Fetcher\" to=\"Summarizer\" out=\"stories.json\" in=\"stories.json\" schema=\"json-array\"\n\
models {{ tier cheap prefer=\"mock-1\" }}
deliverable kind=\"report\" {{ check \"a digest exists\" }}\n"
        ),
    );
    let summary = fire_once(&dir, config(""));
    assert!(
        !summary.ok,
        "the run must fail: the hand-off's promise was broken"
    );
    let names: Vec<(String, TaskStatus)> = summary.tasks;
    let sum = names.iter().find(|(n, _)| n == "Summarizer");
    assert!(sum.is_some());
    let (_, status) = sum.unwrap();
    let reason = match status {
        TaskStatus::Failed(r) => r.clone(),
        other => panic!("the Summarizer must fail with words, got {other:?}"),
    };
    assert!(reason.contains("promised to be"), "{reason}");
    assert!(reason.contains("JSON array"), "{reason}");
}

#[test]
fn a_worker_step_runs_an_external_agent_and_applies_whole() {
    let (home_dir, _) = mock_env();
    let _ = &home_dir;
    // The worker stub: a shell script that writes the plan's artifact,
    // reads its prompt from argv, and lives outside the project.
    let stub_dir = home("worker-stub");
    let stub = stub_dir.join("fake-claude.sh");
    std::fs::write(
        &stub,
        "#!/bin/sh\necho \"hello from the worker: $1\"\necho '# Summary' > artifacts/digest.md\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let pin = std::fs::read(&stub).unwrap();

    let dir = project(
        "worker",
        "meta { name \"worker-demo\" }\n\
intent { summary \"a worker does it\" confirmed \"v1\" }\n\
agent Coder { goal \"write the digest file\" worker \"fake-claude\" retries 1 out \"digest.md\" approval \"before-send\" permissions files-read=true files-write=true spawn=true network=true }\n\
models { tier cheap prefer=\"mock-1\" }
deliverable kind=\"report\" { check \"the file exists\" }\n",
    );
    // the project's artifacts dir must exist for the stub to write into
    std::fs::create_dir_all(dir.join("artifacts")).unwrap();
    let _ = pin;

    let mut cfg = config("");
    cfg.workers.push(wright_runtime::config::WorkerEntry {
        name: "fake-claude".into(),
        command: stub.to_string_lossy().to_string(),
        args: Vec::new(),
        timeout_secs: 30,
        env: Vec::new(),
    });
    let summary = fire_once(&dir, cfg);
    assert!(summary.ok, "the worker step must pass: {summary:?}");
    let text = std::fs::read_to_string(dir.join("artifacts").join("digest.md")).unwrap();
    assert!(text.contains("Summary"), "{text}");
    // and the receipt says the step ran (externally, at no model cost)
    let receipt = std::fs::read_to_string(&summary.receipt).unwrap();
    assert!(receipt.contains("Coder"), "{receipt}");
    assert!(receipt.contains("\"status\":\"ran\""), "{receipt}");
    // the copy the worker saw is gone: it held the whole project
    let leftovers: Vec<_> = std::fs::read_dir(std::env::temp_dir())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(&format!("wright-worker-{}-", std::process::id())))
        .collect();
    assert!(leftovers.is_empty(), "worker copies left behind: {leftovers:?}");
}

#[test]
fn a_worker_step_asks_first_and_a_refusal_applies_nothing() {
    let (home_dir, _) = mock_env();
    let _ = &home_dir;
    let stub_dir = home("worker-stub-refused");
    let stub = stub_dir.join("fake-codex.sh");
    std::fs::write(&stub, "#!/bin/sh\necho '# Summary' > artifacts/digest.md\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let dir = project(
        "worker-refused",
        "meta { name \"worker-refused\" }\n\
intent { summary \"a worker does it\" confirmed \"v1\" }\n\
agent Coder { goal \"write the digest file\" worker \"fake-codex\" out \"digest.md\" approval \"before-send\" permissions files-read=true files-write=true spawn=true network=true }\n\
models { tier cheap prefer=\"mock-1\" }
deliverable kind=\"report\" { check \"the file exists\" }\n",
    );
    std::fs::create_dir_all(dir.join("artifacts")).unwrap();
    let mut cfg = config("");
    cfg.workers.push(wright_runtime::config::WorkerEntry {
        name: "fake-codex".into(),
        command: stub.to_string_lossy().to_string(),
        args: Vec::new(),
        timeout_secs: 30,
        env: Vec::new(),
    });
    let summary = fire(
        &dir,
        FireOptions {
            trigger: Trigger::Manual,
            approver: Box::new(wright_engine::approve::AutoDeny),
            listeners: Vec::new(),
            config: Some(cfg),
            allow_private_network: true,
            resume: None,
            no_cache: false,
        },
    )
    .unwrap();
    assert!(!summary.ok, "a refused worker step must not pass: {summary:?}");
    assert!(summary.headline.contains("declined"), "{}", summary.headline);
    assert!(!dir.join("artifacts").join("digest.md").exists(), "nothing may be applied after a refusal");
}

#[test]
fn a_worker_plan_without_an_approval_is_refused_by_the_rule_of_two() {
    let bp = wright_core::blueprint::parse::blueprint(
        "meta { name \"w\" }\n\
intent { summary \"a worker does it\" confirmed \"v1\" }\n\
agent Coder { goal \"write it\" worker \"claude-code\" out \"x.md\" permissions files-read=true files-write=true spawn=true network=true }\n\
models { tier cheap prefer=\"mock-1\" }
deliverable kind=\"report\" { check \"x\" }\n",
    )
    .unwrap();
    let errs = wright_core::blueprint::validate::validate(&bp).unwrap_err();
    assert!(errs.iter().any(|e| e.contains("Coder") && e.contains("before-send")), "{errs:?}");
}

#[test]
fn the_rule_of_two_refuses_a_three_leg_plan_without_an_approval() {
    let (home_dir, _) = mock_env();
    let _ = &home_dir;
    let port = site("{}");
    let dir = project(
        "rule",
        &format!(
            "meta {{ name \"greedy-demo\" }}\n\
intent {{ summary \"too much\" confirmed \"v1\" }}\n\
agent Greedy {{ goal \"read, fetch and send from http://127.0.0.1:{port}\" tools \"files-read\" tools \"http-read\" tools \"http-post\" out \"o.json\" permissions network=true files-read=true }}\n\
models {{ tier cheap prefer=\"mock-1\" }}
deliverable kind=\"k\" {{ check \"c\" }}\n"
        ),
    );
    let err = fire(
        &dir,
        FireOptions {
            trigger: Trigger::Manual,
            approver: Box::new(wright_engine::approve::AutoApprove),
            listeners: Vec::new(),
            config: Some(config("")),
            allow_private_network: true,
            resume: None,
            no_cache: false,
        },
    )
    .unwrap_err();
    assert!(err.contains("all three legs"), "{err}");
    assert!(err.contains("before-send"), "{err}");
}

#[test]
fn the_store_keeps_every_receipt_verifiable_after_the_run() {
    let (home_dir, _) = mock_env();
    let _ = &home_dir;
    let port = site("[1,2,3]");
    let dir = project(
        "store",
        &format!(
            "meta {{ name \"store-demo\" }}\n\
intent {{ summary \"tiny\" confirmed \"v1\" }}\n\
agent Fetcher {{ goal \"fetch from http://127.0.0.1:{port}\" tools \"http-read\" out \"nums.json\" permissions network=true }}\n\
models {{ tier cheap prefer=\"mock-1\" }}
deliverable kind=\"report\" {{ check \"it exists\" }}\n"
        ),
    );
    let summary = fire_once(&dir, config(""));
    assert!(summary.ok);
    let store = Store::open(&dir).unwrap();
    assert_eq!(store.runs(), vec![summary.run]);
    let v = trust_verify::project_run(&dir, None).unwrap();
    assert!(v.ok, "{}", v.report());
}
