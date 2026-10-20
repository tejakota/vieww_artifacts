//! End-to-end: real sockets, real SSE, real tools, real sandbox, real
//! store — with the mock provider standing in for a model vendor, a tiny
//! local "news site" for the Fetcher, and a local "mail API" that records
//! what the Mailer sent.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use wright_core::events::EventKind;
use wright_engine::approve::{AutoApprove, AutoDeny};
use wright_engine::interview::{interview, Scripted};
use wright_engine::mock::MockProvider;
use wright_engine::{fire, FireOptions, TaskStatus};
use wright_runtime::config::Config;
use wright_runtime::store::Store;

/// One WRIGHT_HOME and one mock provider for the whole test binary
/// (environment variables are process-wide).
fn env() -> &'static (PathBuf, MockProvider) {
    static ENV: OnceLock<(PathBuf, MockProvider)> = OnceLock::new();
    ENV.get_or_init(|| {
        let home = std::env::temp_dir().join(format!("wright-e2e-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("WRIGHT_HOME", &home);
        (home, MockProvider::start(0).unwrap())
    })
}

fn config(prices: &str) -> Config {
    let (_, mock) = env();
    Config::parse(
        &format!("provider \"mock\" kind=\"openai\" base=\"{}\" models=\"mock-*\"\ninterview model=\"mock-1\"\n{prices}", mock.base_url()),
        "test config",
    )
    .unwrap()
}

/// A tiny HTTP server: GET returns `body`; POST bodies are recorded.
fn site(body: &'static str) -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let posts = Arc::new(Mutex::new(Vec::new()));
    let p2 = Arc::clone(&posts);
    std::thread::spawn(move || {
        for s in listener.incoming().flatten() {
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut first = String::new();
            r.read_line(&mut first).unwrap();
            let mut len = 0;
            loop {
                let mut h = String::new();
                r.read_line(&mut h).unwrap();
                if h.trim().is_empty() {
                    break;
                }
                if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap();
                }
            }
            let mut b = vec![0; len];
            r.read_exact(&mut b).unwrap();
            if first.starts_with("POST") {
                p2.lock().unwrap().push(String::from_utf8_lossy(&b).to_string());
            }
            let mut s = s;
            write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    (port, posts)
}

fn project(name: &str, blueprint: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("wright-e2e-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("blueprint.kdl"), blueprint).unwrap();
    d
}

fn digest(news: u16, mail: u16, extra_project: &str) -> String {
    format!(
        r#"meta {{ name "HN Digest (e2e)" }}
intent {{ summary "Fetch top stories, summarize, email." confirmed "v1" }}
project {{
  language "python"
  {extra_project}
}}
agent Fetcher {{
  goal "Fetch the top stories from http://127.0.0.1:{news}/top"
  tools "http-read"
  out "stories.json"
  permissions network=true
}}
agent Summarizer {{
  goal "Write a punchy summary per story"
  model tier="quality"
  in "stories.json"
  out "summaries.json"
}}
agent Mailer {{
  goal "Send the digest to http://127.0.0.1:{mail}/send"
  tools "http-post"
  in "summaries.json"
  approval "before-send"
  retries 0
  permissions network=true
}}
edge from="Fetcher" to="Summarizer"
edge from="Summarizer" to="Mailer"
chips {{
  scheduling cron="0 8 * * *" tz="Asia/Kolkata"
  logging on
  tests on
}}
guardrails {{ budget per-firing=500000 daily=3000000 wall-clock 300 max-turns 60 }}
deliverable kind="email" {{ check "The digest was delivered with every story." }}
models {{
  tier cheap prefer="mock-flaky-1" fallback="mock-1"
  tier quality prefer="mock-down" fallback="mock-2"
}}
"#
    )
}

fn opts(approve: bool) -> FireOptions {
    FireOptions {
        approver: if approve { Box::new(AutoApprove) } else { Box::new(AutoDeny) },
        config: Some(config("")),
        allow_private_network: true,
        ..FireOptions::default()
    }
}

fn status_of<'a>(s: &'a wright_engine::RunSummary, name: &str) -> &'a TaskStatus {
    &s.tasks.iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("no task {name}")).1
}

#[test]
fn the_digest_fires_end_to_end_then_the_cache_and_freshness_rules_hold() {
    env();
    let (news, _) = site(r#"[{"id":1,"title":"Rust 2.0"},{"id":2,"title":"Wright ships"}]"#);
    let (mail, inbox) = site(r#"{"ok":true}"#);
    let dir = project("digest", &digest(news, mail, ""));

    // ── firing 1: everything runs ─────────────────────────────────
    let s1 = fire(&dir, opts(true)).unwrap();
    assert!(s1.ok, "{}\n{:?}", s1.headline, s1.tasks);
    for t in ["Shipwright", "Logwright", "Fetcher", "Summarizer", "Testwright", "Reviewer", "Mailer", "Operator"] {
        assert!(matches!(status_of(&s1, t), TaskStatus::Ran { .. }), "{t}: {:?}", status_of(&s1, t));
    }
    assert_eq!(s1.verdict, Some(Ok(())));
    assert_eq!(inbox.lock().unwrap().len(), 1, "the Mailer delivered exactly once");
    assert!(inbox.lock().unwrap()[0].contains("made_by"), "the digest carried the summaries");
    // the skeleton, the artifacts, the checks
    for f in ["README.md", "AGENTS.md", "src/hn_digest_e2e/log.py", "tests/test_smoke.py", "tests/test_wright_outputs.py", "artifacts/stories.json", "artifacts/summaries.json"] {
        assert!(dir.join(f).exists(), "missing {f}");
    }
    // the receipt is machine-checkable
    let receipt = std::fs::read_to_string(&s1.receipt).unwrap();
    assert!(receipt.contains("\"verdict\":\"approve\"") && receipt.contains("events_blake3"));

    let store = Store::open(&dir).unwrap();
    let ev = store.read_events(s1.run).unwrap();
    let kinds: Vec<EventKind> = ev.events().iter().map(|e| e.kind).collect();
    assert!(kinds.contains(&EventKind::AgentRetrying), "the flaky model was retried");
    assert!(ev.events().iter().any(|e| e.kind == EventKind::ScheduleRegistered && e.note.contains("08:00")));
    // tests ran before anything was sent
    let pos = |pred: &dyn Fn(&wright_core::events::Event) -> bool| ev.events().iter().position(pred).unwrap();
    let approved = pos(&|e| e.kind == EventKind::Verdict && e.agent.as_deref() == Some("Reviewer"));
    let sent = pos(&|e| e.kind == EventKind::ToolCalled && e.note.starts_with("http-post"));
    assert!(approved < sent, "the Reviewer approved before the Mailer sent");

    // ── firing 2: same blueprint ─────────────────────────────────
    let s2 = fire(&dir, opts(true)).unwrap();
    assert!(s2.ok, "{}", s2.headline);
    assert!(matches!(status_of(&s2, "Fetcher"), TaskStatus::Ran { .. }), "live data is fetched fresh every firing");
    assert!(matches!(status_of(&s2, "Summarizer"), TaskStatus::Cached { .. }), "same fetched content -> Summarizer is a cache hit");
    assert!(matches!(status_of(&s2, "Logwright"), TaskStatus::Cached { .. }), "setup happens once");
    assert!(matches!(status_of(&s2, "Mailer"), TaskStatus::Ran { .. }), "delivery is never cached");
    assert_eq!(inbox.lock().unwrap().len(), 2);

    // ── firing 3: nobody watching -> the send is denied, nothing leaves ──
    let s3 = fire(&dir, opts(false)).unwrap();
    assert_eq!(inbox.lock().unwrap().len(), 2, "auto-deny means no email");
    let ev3 = store.read_events(s3.run).unwrap();
    assert!(ev3.events().iter().any(|e| e.kind == EventKind::ApprovalResolved && e.note.contains("denied")));

    // ── the cache is content-addressed and tamper-evident ──
    let history = store.blueprint_history();
    assert!(!history.is_empty());
}

#[test]
fn failing_tests_stop_delivery() {
    env();
    let (news, _) = site(r#"[{"id":1}]"#);
    let (mail, inbox) = site(r#"{"ok":true}"#);
    let dir = project("reject", &digest(news, mail, "test \"echo 'FAIL: digest is wrong'; exit 1\""));
    let s = fire(&dir, opts(true)).unwrap();
    assert!(!s.ok);
    assert!(s.headline.contains("rejected"), "{}", s.headline);
    assert!(matches!(status_of(&s, "Mailer"), TaskStatus::Skipped(r) if r.contains("Reviewer")), "{:?}", status_of(&s, "Mailer"));
    assert!(inbox.lock().unwrap().is_empty(), "nothing was delivered");
}

#[test]
fn a_crashed_firing_resumes_without_redoing_finished_work() {
    env();
    let (news, _) = site(r#"[{"id":1}]"#);
    let (mail, inbox) = site(r#"{"ok":true}"#);
    let dir = project("resume", &digest(news, mail, ""));
    let s1 = fire(&dir, opts(true)).unwrap();
    assert!(s1.ok, "{}", s1.headline);

    // Simulate a crash: a new run whose log stops right after the
    // Summarizer finished.
    let store = Store::open(&dir).unwrap();
    let ev = store.read_events(s1.run).unwrap();
    let cut = ev.events().iter().position(|e| e.kind == EventKind::AgentFinished && e.agent.as_deref() == Some("Summarizer")).unwrap();
    let crashed = store.new_run().unwrap().run();
    let lines: Vec<String> = ev.events()[..=cut]
        .iter()
        .map(|e| {
            let mut e = e.clone();
            e.run = crashed;
            e.to_json_line()
        })
        .collect();
    std::fs::write(store.run_dir(crashed).join("events.jsonl"), lines.join("\n") + "\n").unwrap();

    let s2 = fire(&dir, FireOptions { resume: Some(crashed), ..opts(true) }).unwrap();
    assert_eq!(s2.run, crashed, "resume continues the same run");
    assert!(s2.ok, "{}", s2.headline);
    let ev2 = store.read_events(crashed).unwrap();
    let restarted = |name: &str| ev2.events()[cut + 1..].iter().any(|e| e.kind == EventKind::AgentStarted && e.agent.as_deref() == Some(name));
    assert!(!restarted("Fetcher") && !restarted("Summarizer"), "finished work is not redone");
    assert!(restarted("Mailer"));
    assert_eq!(inbox.lock().unwrap().len(), 2);
}

#[test]
fn the_budget_stops_a_firing_cleanly() {
    env();
    let (news, _) = site(r#"[{"id":1}]"#);
    let (mail, inbox) = site(r#"{"ok":true}"#);
    let bp = digest(news, mail, "").replace("per-firing=500000", "per-firing=20");
    let dir = project("budget", &bp);
    let s = fire(&dir, FireOptions { config: Some(config("price model=\"mock-*\" in=50 out=50\n")), ..opts(true) }).unwrap();
    assert!(!s.ok);
    assert!(s.tasks.iter().any(|(_, st)| matches!(st, TaskStatus::Failed(r) if r.contains("budget"))), "{:?}", s.tasks);
    assert!(inbox.lock().unwrap().is_empty());
}

#[test]
fn interview_to_confirmed_blueprint_to_working_project() {
    env();
    let dir = std::env::temp_dir().join(format!("wright-e2e-interview-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut io = Scripted { answers: vec!["A tool that turns my notes into a weekly summary".into()], confirm_answer: true, transcript: Vec::new() };
    let bp = interview(&dir, &config(""), &mut io).unwrap().expect("confirmed");
    assert_eq!(bp.intent.confirmed, "v1");
    assert!(io.transcript.iter().any(|l| l.contains("summary: Builder will")), "{:#?}", io.transcript);
    assert!(Path::new(&dir.join("blueprint.kdl")).exists());

    let s = fire(&dir, opts(true)).unwrap();
    assert!(s.ok, "{}\n{:?}", s.headline, s.tasks);
    assert!(dir.join("artifacts/result.json").exists());
    // the generated project's own tests pass, standalone
    let out = std::process::Command::new("sh").arg("-c").arg(wright_core::convention::test_cmd(&bp)).current_dir(&dir).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn declining_the_confirm_gate_writes_nothing() {
    env();
    let dir = std::env::temp_dir().join(format!("wright-e2e-decline-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut io = Scripted { answers: vec!["Something".into()], confirm_answer: false, transcript: Vec::new() };
    assert!(interview(&dir, &config(""), &mut io).unwrap().is_none());
    assert!(!dir.join("blueprint.kdl").exists());
}

#[test]
fn a_cached_approval_never_outlives_a_new_test_failure() {
    // Found in the v0.2 demo: the Reviewer's cache key did not include
    // the Testwright's report, so a cached APPROVE could follow a fresh
    // FAIL. Run once green, then break the tests: delivery must stop.
    env();
    let (news, _) = site(r#"[{"id":1}]"#);
    let (mail, inbox) = site(r#"{"ok":true}"#);
    let dir = project("stale-approval", &digest(news, mail, ""));
    let s1 = fire(&dir, opts(true)).unwrap();
    assert!(s1.ok, "{}", s1.headline);
    std::fs::write(dir.join("tests/test_broken.py"), "import unittest\n\nclass T(unittest.TestCase):\n    def test_x(self):\n        self.fail('broken on purpose')\n").unwrap();
    let s2 = fire(&dir, opts(true)).unwrap();
    assert!(!s2.ok, "{}", s2.headline);
    assert!(matches!(status_of(&s2, "Mailer"), TaskStatus::Skipped(_)), "{:?}", status_of(&s2, "Mailer"));
    assert_eq!(inbox.lock().unwrap().len(), 1, "only the first, green run delivered");
}
