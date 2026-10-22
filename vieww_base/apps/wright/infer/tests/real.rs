//! The real built-in models, end to end. Off unless `WRIGHT_REAL_MODELS=1`
//! (they are 2 GB; the `Wright - built-in models` workflow downloads them
//! with `wright model pull` first, from `~/.wright/models` or
//! `$WRIGHT_HOME/models`).
//!
//! * Qwen3-1.7B: greedy text agrees with llama.cpp's (`WRIGHT_REAL_QWEN_EXPECT`:
//!   a file with the prompt, a newline, then `llama-simple -n 32` output),
//!   it calls a tool when asked to, and it answers a sum.
//! * Whisper base: JFK's inaugural line, in English, detected as English,
//!   matching whisper.cpp's transcript (`WRIGHT_REAL_WHISPER_EXPECT`) word
//!   for word or nearly (8-bit weights here, half floats there).

use std::time::Instant;

use wright_core::wire::json;
use wright_infer::audio::wav_to_samples;
use wright_infer::llm::Llm;
use wright_infer::qwen3::Model;
use wright_infer::sample::Sampler;
use wright_infer::whisper::Whisper;
use wright_runtime::local;
use wright_runtime::models::{Block, CompletionRequest, Message, ToolDef};

fn on() -> bool {
    let yes = std::env::var("WRIGHT_REAL_MODELS").is_ok_and(|v| v == "1");
    if !yes {
        eprintln!("WRIGHT_REAL_MODELS=1 not set; skipping");
    }
    yes
}

fn words(s: &str) -> Vec<String> {
    s.split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

#[test]
fn qwen3_agrees_with_llama_cpp_and_calls_tools() {
    if !on() {
        return;
    }
    let m = local::find(local::TEXT_MODEL).unwrap();
    assert!(m.installed(), "run `wright model pull {}` first", m.id);
    let t = Instant::now();
    let llm = Llm::new(Model::load(&m.path()).unwrap(), m.id);
    eprintln!("loaded in {:.1}s", t.elapsed().as_secs_f32());

    if let Ok(path) = std::env::var("WRIGHT_REAL_QWEN_EXPECT") {
        let expect = std::fs::read_to_string(path).unwrap();
        let (prompt, want) = expect.split_once('\n').unwrap();
        let ids = llm.model.tokenizer.encode(prompt, false);
        let t = Instant::now();
        let (got, n, _) = llm
            .generate(&ids, 32, Sampler::greedy(), &mut |_| {})
            .unwrap();
        eprintln!(
            "32 tokens in {:.1}s\n  llama.cpp: {want:?}\n  wright:    {got:?}",
            t.elapsed().as_secs_f32()
        );
        assert_eq!(n, 32);
        // A real model's choices are rarely near-ties: the first dozen
        // words must be the same.
        let (a, b) = (words(want), words(&got));
        let k = 12.min(a.len()).min(b.len());
        assert_eq!(a[..k], b[..k], "greedy text diverges from llama.cpp early");
    }

    // A tool call, in Qwen3's own format, parsed back into a ToolUse.
    let mut req = CompletionRequest::new(m.id);
    req.system = "You are a careful assistant working in a project folder. Use the tools to look at files; never guess their contents.".into();
    req.tools.push(ToolDef { name: "files-read".into(), description: "Read a text file from the project folder.".into(), schema: r#"{"type":"object","properties":{"path":{"type":"string","description":"Path relative to the project"}},"required":["path"]}"#.into() });
    req.messages
        .push(Message::user_text("What does notes.txt say?"));
    req.max_tokens = 200;
    let t = Instant::now();
    let r = llm.complete(&req, Sampler::greedy(), &mut |_| {}).unwrap();
    eprintln!(
        "tool turn in {:.1}s: {:?}",
        t.elapsed().as_secs_f32(),
        r.blocks
    );
    let call = r.tool_uses();
    assert_eq!(call.len(), 1, "one tool call: {:?}", r.blocks);
    assert_eq!(call[0].1, "files-read");
    assert!(
        call[0]
            .2
            .str_of("path")
            .is_some_and(|p| p.ends_with("notes.txt")),
        "{}",
        json::to_string(call[0].2)
    );

    // The result goes back in; the next turn reuses the cached prompt and
    // answers from it.
    let id = call[0].0.to_string();
    req.messages.push(Message {
        role: wright_runtime::models::Role::Assistant,
        blocks: r.blocks.clone(),
    });
    req.messages.push(Message {
        role: wright_runtime::models::Role::User,
        blocks: vec![Block::ToolResult {
            id,
            content: "Ship the release on Friday at 10:00.".into(),
            is_error: false,
        }],
    });
    let t = Instant::now();
    let r = llm.complete(&req, Sampler::greedy(), &mut |_| {}).unwrap();
    eprintln!(
        "answer turn in {:.1}s: {}",
        t.elapsed().as_secs_f32(),
        r.text()
    );
    assert!(r.tool_uses().is_empty());
    assert!(r.text().contains("Friday"), "{}", r.text());

    let mut req = CompletionRequest::new(m.id);
    req.messages.push(Message::user_text(
        "What is 17 + 25? Reply with just the number.",
    ));
    req.max_tokens = 16;
    let r = llm.complete(&req, Sampler::greedy(), &mut |_| {}).unwrap();
    assert_eq!(r.text().trim(), "42");
}

#[test]
fn whisper_hears_jfk() {
    if !on() {
        return;
    }
    let m = local::find(local::SPEECH_MODEL).unwrap();
    assert!(m.installed(), "run `wright model pull {}` first", m.id);
    let wav = std::env::var("WRIGHT_REAL_JFK")
        .expect("WRIGHT_REAL_JFK: path to whisper.cpp's samples/jfk.wav");
    let samples = wav_to_samples(&std::fs::read(wav).unwrap()).unwrap();
    let w = Whisper::load(&m.path()).unwrap();
    let t = Instant::now();
    let out = w.transcribe(&samples, None).unwrap();
    eprintln!(
        "11 s of speech in {:.1}s: [{}] {}",
        t.elapsed().as_secs_f32(),
        out.language,
        out.text
    );
    assert_eq!(out.language, "en");
    let got = words(&out.text).join(" ");
    assert!(
        got.contains("ask not what your country can do for you"),
        "{got}"
    );
    if let Ok(path) = std::env::var("WRIGHT_REAL_WHISPER_EXPECT") {
        let want = words(&std::fs::read_to_string(path).unwrap());
        let mine = words(&out.text);
        let same = want.iter().zip(&mine).filter(|(a, b)| a == b).count();
        eprintln!("whisper.cpp: {}", want.join(" "));
        assert!(
            same * 10 >= want.len().max(mine.len()) * 9,
            "fewer than 90% of words agree with whisper.cpp"
        );
    }
}
