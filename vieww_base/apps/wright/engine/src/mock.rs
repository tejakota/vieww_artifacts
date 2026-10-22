//! The mock provider: a real HTTP server that speaks the OpenAI-compatible
//! streaming protocol and plays every Wright role deterministically.
//!
//! Why it exists: end-to-end tests and demos must exercise the WHOLE
//! machine — sockets, SSE parsing, tool calling, the gate, the sandbox,
//! the cache, the receipt — without a network or an API key. The mock is
//! not a stub inside the engine; it is a separate server the engine
//! reaches over TCP exactly as it reaches DeepSeek or a local gateway.
//!
//! Start it with `wright mock-provider [--port N]`, point a provider at
//! it in `~/.wright/config.kdl` (`provider "mock" base="http://127.0.0.1:N/v1" models="*"`),
//! and every firing runs offline.
//!
//! Scripts, chosen by the system prompt:
//!
//! | Role          | What the mock does                                            |
//! |---------------|---------------------------------------------------------------|
//! | project agent | http-read a URL in its goal (if it has the tool), write every declared output, http-post to a URL in its goal (if it has the tool), finish |
//! | Testwright    | write tests/test_wright_outputs.py, run-tests, report PASS/FAIL from the exit code |
//! | Reviewer      | list files, APPROVE — or REJECT if the Testwright failed       |
//! | Logwright / Firewright | look around, report the convention already covers it |
//! | Interviewer   | propose a blueprint built from the person's request            |
//!
//! Failure modes, by model name: `mock-flaky*` answers 503 to every other
//! request (exercises retries); `mock-down*` always answers 500 (exercises
//! fallback to the tier's second model).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use wright_core::wire::json::{self, Value};

/// A running mock server.
pub struct MockProvider {
    pub port: u16,
    pub requests: Arc<AtomicUsize>,
}

impl MockProvider {
    /// Start on 127.0.0.1:`port` (0 = any free port). Runs until the process exits.
    pub fn start(port: u16) -> Result<MockProvider, String> {
        let listener =
            TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("mock provider: {e}"))?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let requests = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&requests);
        std::thread::Builder::new()
            .name("wright-mock-provider".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    let counter = Arc::clone(&counter);
                    std::thread::spawn(move || {
                        let n = counter.fetch_add(1, Ordering::SeqCst);
                        let _ = handle(stream, n);
                    });
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(MockProvider { port, requests })
    }

    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}/v1", self.port)
    }
}

fn handle(stream: TcpStream, n: usize) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut len = 0usize;
    loop {
        let mut h = String::new();
        reader.read_line(&mut h)?;
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                len = v.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0u8; len.min(64 << 20)];
    reader.read_exact(&mut body)?;
    let mut out = stream;
    // Speech-to-text (voice input): answer with how much audio arrived, so
    // a test can tell the recording reached the provider.
    if line
        .split_whitespace()
        .nth(1)
        .is_some_and(|path| path.ends_with("/audio/transcriptions"))
    {
        let text = format!("mock transcript of {} bytes", body.len());
        let answer = json::to_string(&Value::obj(vec![("text", Value::str(text))]));
        return write!(out, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}", answer.len());
    }
    let req = match json::parse(&String::from_utf8_lossy(&body)) {
        Ok(v) => v,
        Err(e) => return respond_status(&mut out, 400, &format!("bad json: {e}")),
    };
    let model = req.str_of("model").unwrap_or("mock").to_string();
    if model.contains("mock-down") {
        return respond_status(&mut out, 500, "mock: this model is down");
    }
    if model.contains("mock-flaky") && n.is_multiple_of(2) {
        return respond_status(&mut out, 503, "mock: overloaded, try again");
    }
    let (text, calls) = script(&req);
    let prompt_tokens = (body.len() / 4) as i64;
    let stream_mode = req.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);
    if stream_mode {
        respond_stream(&mut out, &model, &text, &calls, prompt_tokens)
    } else {
        respond_json(&mut out, &model, &text, &calls, prompt_tokens)
    }
}

fn respond_status(out: &mut TcpStream, status: u16, msg: &str) -> std::io::Result<()> {
    let body = json::to_string(&Value::obj(vec![(
        "error",
        Value::obj(vec![("message", Value::str(msg))]),
    )]));
    write!(out, "HTTP/1.1 {status} Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
}

fn respond_json(
    out: &mut TcpStream,
    model: &str,
    text: &str,
    calls: &[(String, Value)],
    prompt: i64,
) -> std::io::Result<()> {
    let tool_calls: Vec<Value> = calls
        .iter()
        .enumerate()
        .map(|(i, (name, args))| {
            Value::obj(vec![
                ("id", Value::str(format!("call_{i}"))),
                ("type", Value::str("function")),
                (
                    "function",
                    Value::obj(vec![
                        ("name", Value::str(name.clone())),
                        ("arguments", Value::str(json::to_string(args))),
                    ]),
                ),
            ])
        })
        .collect();
    let mut msg = vec![
        ("role", Value::str("assistant")),
        ("content", Value::str(text)),
    ];
    if !tool_calls.is_empty() {
        msg.push(("tool_calls", Value::Arr(tool_calls)));
    }
    let body = json::to_string(&Value::obj(vec![
        ("model", Value::str(model)),
        (
            "choices",
            Value::Arr(vec![Value::obj(vec![
                ("message", Value::obj(msg)),
                (
                    "finish_reason",
                    Value::str(if calls.is_empty() {
                        "stop"
                    } else {
                        "tool_calls"
                    }),
                ),
            ])]),
        ),
        (
            "usage",
            Value::obj(vec![
                ("prompt_tokens", Value::int(prompt)),
                (
                    "completion_tokens",
                    Value::int((text.len() / 4 + 20) as i64),
                ),
            ]),
        ),
    ]));
    write!(out, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
}

fn respond_stream(
    out: &mut TcpStream,
    model: &str,
    text: &str,
    calls: &[(String, Value)],
    prompt: i64,
) -> std::io::Result<()> {
    // Chunked transfer + SSE, split into several small events so the
    // client's incremental parsing is genuinely exercised.
    write!(out, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n")?;
    let mut send = |data: String| -> std::io::Result<()> {
        let ev = format!("data: {data}\n\n");
        write!(out, "{:x}\r\n{ev}\r\n", ev.len())?;
        out.flush()
    };
    let mid = text
        .char_indices()
        .nth(text.chars().count() / 2)
        .map(|(i, _)| i)
        .unwrap_or(0);
    for piece in [&text[..mid], &text[mid..]] {
        if piece.is_empty() {
            continue;
        }
        send(json::to_string(&Value::obj(vec![
            ("model", Value::str(model)),
            (
                "choices",
                Value::Arr(vec![Value::obj(vec![(
                    "delta",
                    Value::obj(vec![("content", Value::str(piece))]),
                )])]),
            ),
        ])))?;
    }
    for (i, (name, args)) in calls.iter().enumerate() {
        let a = json::to_string(args);
        let cut = a
            .char_indices()
            .nth(a.chars().count() / 2)
            .map(|(i, _)| i)
            .unwrap_or(0);
        let (a1, a2) = a.split_at(cut);
        for (first, part) in [(true, a1), (false, a2)] {
            let mut f = vec![("arguments", Value::str(part))];
            if first {
                f.insert(0, ("name", Value::str(name.clone())));
            }
            let mut call = vec![("index", Value::int(i as i64)), ("function", Value::obj(f))];
            if first {
                call.insert(1, ("id", Value::str(format!("call_{i}"))));
            }
            send(json::to_string(&Value::obj(vec![(
                "choices",
                Value::Arr(vec![Value::obj(vec![(
                    "delta",
                    Value::obj(vec![("tool_calls", Value::Arr(vec![Value::obj(call)]))]),
                )])]),
            )])))?;
        }
    }
    send(json::to_string(&Value::obj(vec![(
        "choices",
        Value::Arr(vec![Value::obj(vec![
            ("delta", Value::Obj(vec![])),
            (
                "finish_reason",
                Value::str(if calls.is_empty() {
                    "stop"
                } else {
                    "tool_calls"
                }),
            ),
        ])]),
    )])))?;
    send(json::to_string(&Value::obj(vec![
        ("choices", Value::Arr(vec![])),
        (
            "usage",
            Value::obj(vec![
                ("prompt_tokens", Value::int(prompt)),
                (
                    "completion_tokens",
                    Value::int((text.len() / 4 + 20) as i64),
                ),
            ]),
        ),
    ])))?;
    send("[DONE]".into())?;
    write!(out, "0\r\n\r\n")?;
    out.flush()
}

// ── the scripts ──────────────────────────────────────────────────

struct Convo<'a> {
    system: String,
    first_user: String,
    tools: Vec<String>,
    /// Tool names called so far, in order.
    called: Vec<String>,
    /// Content of each tool result so far, in order.
    results: Vec<String>,
    req: &'a Value,
}

fn convo(req: &Value) -> Convo<'_> {
    let msgs = req.get("messages").and_then(|m| m.as_arr()).unwrap_or(&[]);
    let mut c = Convo {
        system: String::new(),
        first_user: String::new(),
        tools: Vec::new(),
        called: Vec::new(),
        results: Vec::new(),
        req,
    };
    for m in msgs {
        match m.str_of("role") {
            Some("system") => c.system = m.str_of("content").unwrap_or("").to_string(),
            Some("user") if c.first_user.is_empty() => {
                c.first_user = m.str_of("content").unwrap_or("").to_string()
            }
            Some("assistant") => {
                for tc in m.get("tool_calls").and_then(|t| t.as_arr()).unwrap_or(&[]) {
                    c.called.push(
                        tc.path("function", &["name"])
                            .and_then(|n| n.as_str())
                            .unwrap_or("")
                            .to_string(),
                    );
                }
            }
            Some("tool") => c
                .results
                .push(m.str_of("content").unwrap_or("").to_string()),
            _ => {}
        }
    }
    for t in req.get("tools").and_then(|t| t.as_arr()).unwrap_or(&[]) {
        if let Some(n) = t.path("function", &["name"]).and_then(|n| n.as_str()) {
            c.tools.push(n.to_string());
        }
    }
    c
}

fn args(pairs: Vec<(&str, Value)>) -> Value {
    Value::obj(pairs)
}

fn first_url(text: &str) -> Option<String> {
    let at = text.find("http://").or_else(|| text.find("https://"))?;
    Some(
        text[at..]
            .split(|c: char| c.is_whitespace() || c == '"' || c == ')')
            .next()?
            .trim_end_matches(['.', ',', ':', ';'])
            .to_string(),
    )
}

/// Decide the next assistant turn: (text, tool calls).
fn script(req: &Value) -> (String, Vec<(String, Value)>) {
    let c = convo(req);
    let sys = c.system.as_str();
    if sys.contains("You are the Interviewer") {
        return interviewer(&c);
    }
    if sys.contains("You are the Testwright") {
        return testwright(&c);
    }
    if sys.contains("You are the Reviewer") {
        return if c.called.is_empty() {
            (
                "Reading the project before deciding.".into(),
                vec![("files-list".into(), args(vec![]))],
            )
        } else if c.first_user.contains("TESTS: FAIL") {
            ("The Testwright reported failures, so nothing may be delivered.\nVERDICT: REJECT: the tests failed".into(), vec![])
        } else {
            ("I read the artifacts and the files changed this firing; they match the definition of done and the layout convention.\nVERDICT: APPROVE".into(), vec![])
        };
    }
    if sys.contains("You are the Logwright") || sys.contains("You are the Firewright") {
        return if c.called.is_empty() {
            (
                "Checking what the skeleton already provides.".into(),
                vec![("files-list".into(), args(vec![]))],
            )
        } else {
            ("The convention's skeleton already provides this (log helper / CI workflow); nothing to add.".into(), vec![])
        };
    }
    project_agent(&c)
}

fn project_agent(c: &Convo<'_>) -> (String, Vec<(String, Value)>) {
    let goal = c
        .system
        .lines()
        .find_map(|l| l.strip_prefix("Your goal: "))
        .unwrap_or("")
        .to_string();
    let outputs: Vec<String> = c
        .system
        .lines()
        .find_map(|l| l.strip_prefix("Your outputs: "))
        .map(|l| {
            l.split(". ")
                .next()
                .unwrap_or("")
                .split(", ")
                .map(|s| s.trim().trim_end_matches('.').to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let url = first_url(&goal);
    let has = |t: &str| c.tools.iter().any(|x| x == t);

    // 1. Fetch first, if this agent reads the web.
    if has("http-read") && url.is_some() && !c.called.iter().any(|n| n == "http-read") {
        return (
            format!("Fetching {}.", url.clone().unwrap_or_default()),
            vec![(
                "http-read".into(),
                args(vec![("url", Value::str(url.unwrap_or_default()))]),
            )],
        );
    }
    let fetched = c
        .called
        .iter()
        .zip(&c.results)
        .find(|(n, _)| *n == "http-read")
        .map(|(_, r)| r.clone())
        .unwrap_or_default();
    // 2. Write each declared output not yet written.
    let written = c.called.iter().filter(|n| *n == "write-output").count();
    if written < outputs.len() {
        let name = outputs[written].clone();
        let inputs_seen = c
            .first_user
            .lines()
            .filter(|l| l.starts_with("=== input:"))
            .count();
        let source = if !fetched.is_empty() {
            fetched.lines().skip(1).collect::<Vec<_>>().join("\n")
        } else {
            c.first_user
                .lines()
                .filter(|l| !l.starts_with("===") && !l.is_empty() && *l != "Do your job now.")
                .collect::<Vec<_>>()
                .join("\n")
        };
        let content = if name.ends_with(".json") {
            json::to_string(&args(vec![
                ("made_by", Value::str("mock")),
                ("goal", Value::str(goal.clone())),
                ("inputs_seen", Value::int(inputs_seen as i64)),
                (
                    "excerpt",
                    Value::str(source.chars().take(400).collect::<String>()),
                ),
            ]))
        } else {
            format!(
                "{goal}\n\n{}",
                source.chars().take(2000).collect::<String>()
            )
        };
        return (
            format!("Writing {name}."),
            vec![(
                "write-output".into(),
                args(vec![
                    ("name", Value::str(name)),
                    ("content", Value::str(content)),
                ]),
            )],
        );
    }
    // 3. Deliver, if this agent posts.
    if has("http-post") && url.is_some() && !c.called.iter().any(|n| n == "http-post") {
        let body = c
            .first_user
            .lines()
            .filter(|l| !l.starts_with("===") && *l != "Do your job now.")
            .collect::<Vec<_>>()
            .join("\n");
        return (
            "Sending it.".into(),
            vec![(
                "http-post".into(),
                args(vec![
                    ("url", Value::str(url.unwrap_or_default())),
                    ("body", Value::str(body)),
                    ("content_type", Value::str("text/plain")),
                ]),
            )],
        );
    }
    let denied = c.results.iter().any(|r| r.contains("DENIED"));
    (
        if denied {
            "Done, except for an action that was denied; I did not retry it.".into()
        } else {
            format!(
                "Done: {}.",
                if outputs.is_empty() {
                    "my job is complete".to_string()
                } else {
                    format!("wrote {}", outputs.join(", "))
                }
            )
        },
        vec![],
    )
}

fn testwright(c: &Convo<'_>) -> (String, Vec<(String, Value)>) {
    let has_cmd = c.first_user.contains("Test command:");
    if !has_cmd {
        let promises: Vec<String> = c
            .first_user
            .lines()
            .filter_map(|l| l.strip_prefix("- promise "))
            .map(|l| {
                l.split_once(':')
                    .map(|(_, s)| s.trim().to_string())
                    .unwrap_or_default()
            })
            .collect();
        let accept_lines: String = promises
            .iter()
            .enumerate()
            .map(|(i, _)| format!("ACCEPT {}: PASS\n", i + 1))
            .collect();
        return (format!("{accept_lines}There is no test command for this project; I checked that every artifact exists and is non-empty.\nTESTS: PASS"), vec![]);
    }
    match c.called.len() {
        0 => {
            let test = "\"\"\"Checks written by the Testwright: every declared artifact exists, is non-empty, and JSON parses.\"\"\"\n\
import json\nimport os\nimport unittest\n\n\n\
class WrightOutputs(unittest.TestCase):\n    def test_artifacts(self):\n        names = [n for n in os.listdir(\"artifacts\") if not n.startswith(\".\")]\n        self.assertTrue(names, \"no artifacts were produced\")\n        for n in names:\n            with open(os.path.join(\"artifacts\", n), encoding=\"utf-8\") as f:\n                text = f.read()\n            self.assertTrue(text.strip(), n + \" is empty\")\n            if n.endswith(\".json\"):\n                json.loads(text)\n\n\n\
if __name__ == \"__main__\":\n    unittest.main()\n";
            (
                "Writing checks for the artifacts.".into(),
                vec![(
                    "files-write".into(),
                    args(vec![
                        ("path", Value::str("tests/test_wright_outputs.py")),
                        ("content", Value::str(test)),
                    ]),
                )],
            )
        }
        1 => (
            "Running the tests.".into(),
            vec![("run-tests".into(), args(vec![]))],
        ),
        _ => {
            let last = c.results.last().cloned().unwrap_or_default();
            // The promises the person approved, if any (ADR-025): the
            // mock answers one ACCEPT line per numbered promise, then
            // the summary. Failing mode: if the first_user's text was
            // built with "fail" the mock exercises a broken promise.
            let promises: Vec<String> = c
                .first_user
                .lines()
                .filter_map(|l| l.strip_prefix("- promise "))
                .map(|l| {
                    l.split_once(':')
                        .map(|(_, s)| s.trim().to_string())
                        .unwrap_or_default()
                })
                .collect();
            let accept_lines: String = promises
                .iter()
                .enumerate()
                .map(|(i, _)| format!("ACCEPT {}: PASS\n", i + 1))
                .collect();
            if last.contains("exit code 0") {
                (
                    format!("{accept_lines}All checks passed.\nTESTS: PASS"),
                    vec![],
                )
            } else {
                let why = last
                    .lines()
                    .rev()
                    .find(|l| l.contains("Error") || l.contains("FAIL"))
                    .unwrap_or("non-zero exit")
                    .to_string();
                (
                    format!("{accept_lines}The checks failed.\nTESTS: FAIL: {why}"),
                    vec![],
                )
            }
        }
    }
}

fn interviewer(c: &Convo<'_>) -> (String, Vec<(String, Value)>) {
    let last = c.results.last().cloned().unwrap_or_default();
    if last.starts_with("VALID") {
        return (
            "I have drafted the blueprint. Wright will now show you exactly what it will do."
                .into(),
            vec![],
        );
    }
    if c.called.len() >= 3 {
        return (
            "I could not produce a valid blueprint; please describe the project differently."
                .into(),
            vec![],
        );
    }
    let wish: String = c
        .first_user
        .lines()
        .next()
        .unwrap_or("A small project")
        .chars()
        .take(160)
        .collect();
    let name: String = wish
        .split_whitespace()
        .take(4)
        .collect::<Vec<_>>()
        .join(" ");
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let kdl = format!(
        "meta {{\n  name \"{name}\"\n  format 1\n}}\nintent {{\n  summary \"{wish}\"\n  confirmed \"\"\n}}\nproject {{\n  language \"python\"\n}}\n\
agent Builder {{\n  goal \"{wish}\"\n  model tier=\"cheap\"\n  out \"result.json\"\n}}\n\
chips {{\n  logging on\n  tests on\n}}\nguardrails {{\n  budget per-firing=200000 daily=1000000\n  wall-clock 300\n  max-turns 40\n}}\n\
deliverable kind=\"artifact\" {{\n  check \"result.json exists and describes the result\"\n}}\n\
models {{\n  tier cheap prefer=\"{m}\" fallback=\"\"\n  tier quality prefer=\"{m}\" fallback=\"\"\n}}\n",
        name = esc(&name),
        wish = esc(&wish),
        m = c.req.str_of("model").unwrap_or("mock-1"),
    );
    (
        "Here is my proposal.".into(),
        vec![(
            "propose_blueprint".into(),
            args(vec![("kdl", Value::str(kdl))]),
        )],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use wright_runtime::models::{
        call_once, CompletionRequest, Message, OpenAiCompatible, ToolDef,
    };
    use wright_runtime::net::NetConfig;

    #[test]
    fn the_mock_streams_tool_calls_the_real_client_understands() {
        let mock = MockProvider::start(0).unwrap();
        let p = OpenAiCompatible {
            base_url: mock.base_url(),
        };
        let mut req = CompletionRequest::new("mock-1");
        req.system = "You are Fetcher, one agent in a Wright project.\n\nYour goal: Fetch https://example.com/top\n\nYour outputs: stories.json. Write each one".into();
        req.tools = vec![
            ToolDef {
                name: "http-read".into(),
                description: String::new(),
                schema: "{}".into(),
            },
            ToolDef {
                name: "write-output".into(),
                description: String::new(),
                schema: "{}".into(),
            },
        ];
        req.messages.push(Message::user_text("Do your job now."));
        let resp = call_once(&p, &req, None, &NetConfig::default(), &mut |_| {}).unwrap();
        let uses = resp.tool_uses();
        assert_eq!(uses[0].1, "http-read");
        assert_eq!(uses[0].2.str_of("url"), Some("https://example.com/top"));
        assert!(resp.usage.input_tokens > 0);
    }

    #[test]
    fn flaky_and_down_models_fail_the_way_they_say() {
        let mock = MockProvider::start(0).unwrap();
        let p = OpenAiCompatible {
            base_url: mock.base_url(),
        };
        let mut req = CompletionRequest::new("mock-down");
        req.messages.push(Message::user_text("hi"));
        let e = call_once(&p, &req, None, &NetConfig::default(), &mut |_| {}).unwrap_err();
        assert!(e.retryable && e.message.contains("500"));
    }
}
