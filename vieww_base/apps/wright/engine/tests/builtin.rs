//! With no provider key anywhere, a project still runs: on the built-in
//! model, with a line in the event river saying so. A stand-in backend
//! plays the built-in engine (the real one is `wright-infer`, tested
//! against llama.cpp there).

use std::sync::Mutex;

use wright_core::events::EventKind;
use wright_engine::{fire, FireOptions, TaskStatus};
use wright_runtime::config::Config;
use wright_runtime::local::{self, LocalBackend};
use wright_runtime::models::{Block, CompletionRequest, CompletionResponse, Delta, StopReason, Usage};
use wright_runtime::store::Store;

struct Stand {
    asked: Mutex<Vec<String>>,
}

impl LocalBackend for Stand {
    fn complete(&self, req: &CompletionRequest, on_delta: &mut dyn FnMut(&Delta)) -> Result<CompletionResponse, String> {
        self.asked.lock().unwrap().push(req.model.clone());
        on_delta(&Delta::Status("Loading the built-in model (stand-in)".into()));
        Ok(CompletionResponse { blocks: vec![Block::Text("Done: the note is written.".into())], usage: Usage { input_tokens: 100, output_tokens: 8, cached_input_tokens: 0 }, stop: StopReason::EndTurn, model: req.model.clone() })
    }
    fn transcribe(&self, _: &str, _: &[u8]) -> Result<String, String> {
        Ok("hello".into())
    }
}

static STAND: Stand = Stand { asked: Mutex::new(Vec::new()) };

struct Forward;
impl LocalBackend for Forward {
    fn complete(&self, req: &CompletionRequest, on_delta: &mut dyn FnMut(&Delta)) -> Result<CompletionResponse, String> {
        STAND.complete(req, on_delta)
    }
    fn transcribe(&self, m: &str, w: &[u8]) -> Result<String, String> {
        STAND.transcribe(m, w)
    }
}

#[test]
fn no_key_falls_back_to_the_builtin_model_and_says_so() {
    let home = std::env::temp_dir().join(format!("wright-builtin-home-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    std::env::set_var("WRIGHT_HOME", &home);
    local::install(Box::new(Forward));

    let dir = std::env::temp_dir().join(format!("wright-builtin-proj-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("blueprint.kdl"),
        "meta { name \"Offline\" }\nintent { summary \"write a note\" confirmed \"v1\" }\n\
agent Writer { goal \"Write a short note\" }\n\
deliverable kind=\"message\" { check \"a note exists\" }\n\
models { tier cheap prefer=\"claude-haiku-5-5\" fallback=\"\" tier quality prefer=\"claude-sonnet-5-5\" fallback=\"\" }\n",
    )
    .unwrap();
    // The built-in defaults only: Anthropic with a keychain reference that
    // is not set on this machine.
    let opts = FireOptions { config: Some(Config::parse("", "test").unwrap()), ..FireOptions::default() };
    let s = fire(&dir, opts).unwrap();
    let writer = &s.tasks.iter().find(|(n, _)| n == "Writer").unwrap().1;
    assert!(matches!(writer, TaskStatus::Ran { .. }), "{writer:?} ({})", s.headline);
    assert!(STAND.asked.lock().unwrap().iter().all(|m| m == local::TEXT_MODEL));

    let store = Store::open(&dir).unwrap();
    let ev = store.read_events(s.run).unwrap();
    let notes: Vec<&str> = ev.events().iter().filter(|e| e.kind == EventKind::Note && e.agent.as_deref() == Some("Writer")).map(|e| e.note.as_str()).collect();
    assert!(notes.iter().any(|n| n.contains("claude-haiku-5-5") && n.contains("Using the built-in model qwen3-1.7b")), "{notes:?}");
    assert!(notes.iter().any(|n| n.contains("stand-in")), "status lines reach the river: {notes:?}");
    let tokens = ev.events().iter().find(|e| e.kind == EventKind::TokensRecorded && e.agent.as_deref() == Some("Writer")).unwrap();
    assert!(tokens.note.contains("qwen3-1.7b") && tokens.note.contains("$0.00"), "free, and visibly the built-in: {}", tokens.note);

    // Turned off, the same project fails with the real problem instead.
    let off = Config::parse("builtin fallback=false\n", "test").unwrap();
    let s = fire(&dir, FireOptions { config: Some(off), no_cache: true, ..FireOptions::default() }).unwrap();
    assert!(!s.ok);
    let writer = &s.tasks.iter().find(|(n, _)| n == "Writer").unwrap().1;
    assert!(!matches!(writer, TaskStatus::Ran { .. }), "{writer:?}");
}
