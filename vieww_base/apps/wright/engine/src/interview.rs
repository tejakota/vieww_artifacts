//! DECLARE: the interview that turns words into a confirmed blueprint.
//!
//! The Interviewer is a model with exactly one tool, `propose_blueprint`.
//! Wright — not the model — validates every proposal (parse, validate,
//! compile) and sends the errors back in plain words until it is valid.
//! Then the Confirm Gate shows the person the summary GENERATED FROM THE
//! BLUEPRINT BY CODE (`wright_core::confirm`), and only a "yes" writes
//! `blueprint.kdl`. A model can propose; it cannot describe permissions,
//! and it cannot confirm.
//!
//! The conversation is driven through [`InterviewIo`], so the terminal,
//! the UI and tests all run the same loop.

use std::path::Path;

use wright_core::blueprint::{canon, parse, validate, Blueprint};
use wright_core::compiler;
use wright_core::confirm::{self, Line};
use wright_core::palette::PALETTE;
use wright_runtime::config::{Config, ProviderEntry};
use wright_runtime::local;
use wright_runtime::models::{self, Block, CompletionRequest, Endpoint, Message, Role, ToolDef};
use wright_runtime::net::NetConfig;
use wright_runtime::secrets::{self, SecretValue};
use wright_runtime::store::{write_atomic, Store};

/// How the interview talks to a person.
pub trait InterviewIo {
    /// Show the Interviewer's words.
    fn say(&mut self, text: &str);
    /// Ask the person for their next message. None = they left.
    fn ask(&mut self) -> Option<String>;
    /// Show the Confirm Gate summary; true = confirmed.
    fn confirm(&mut self, lines: &[Line]) -> bool;
    /// What the Interviewer is doing while you wait ("Thinking", "Checking
    /// the draft plan", a first-use model download). Shown until the next
    /// `say`; nothing by default.
    fn working(&mut self, _what: &str) {}
    /// The Interviewer's words so far, while it is still writing them (the
    /// whole text each time, not the new part). Nothing by default.
    fn partial(&mut self, _so_far: &str) {}
    /// The conversation so far, when this interview picks one up (a session
    /// reopened from Saved): `(you, text)`, oldest first. The next `ask` is
    /// what comes after it.
    fn earlier(&mut self) -> Vec<(bool, String)> {
        Vec::new()
    }
}

const FORMAT_REFERENCE: &str = r#"
BLUEPRINT FORMAT (KDL dialect; one node per line; strings in double quotes)

meta { name "Project name" format 1 }
intent { summary "One sentence in the person's words." confirmed "" }
project { language "python" }            // python | node | react | rust | shell | none
agent Fetcher {                          // one card; a name, then fields:
  goal "One sentence: what this agent does."
  model tier="cheap"                     // cheap | quality
  tools "http-read"                      // one line per tool, from the palette below
  in "input.json"                        // artifacts it reads (made by an upstream agent)
  out "output.json"                      // artifacts it writes (plain file names)
  approval "before-send"                 // only for agents that send/post
  permissions network=true               // files-read files-write network shell spawn; all default false
}
edge from="Fetcher" to="Summarizer"      // Summarizer runs after Fetcher and receives its outputs
chips { scheduling cron="0 8 * * *" tz="Asia/Kolkata" logging on tests on }
guardrails { budget per-firing=500000 daily=3000000 wall-clock 300 max-turns 40 }   // budgets in micro-USD
deliverable kind="email" {
  check "What must be true when it is done."
  accept "One promise the person can read and a test can check."   // 1-3 lines; each becomes a check the Reviewer enforces
}
models { tier cheap prefer="model-id" fallback="model-id" tier quality prefer="model-id" fallback="model-id" }

edge from="Fetcher" to="Summarizer" out="stories.json" in="stories.json" schema="json-array"   // a typed hand-off (json-object | json-array | text | csv | any)

Rules: every tool's capability must be granted in permissions; every `in` must be produced
by an agent connected by edges; secrets are chips { secrets name="keychain:path" } — never values.
Write `accept` lines for what the person cares about: they read them as promises before anything runs.
An agent that holds private data, untrusted input AND an outbound channel needs `approval "before-send"`.
"#;

fn system_prompt(models_hint: &str, config: &Config) -> String {
    let spec = wright_agents::spec(compiler::SystemKind::Interviewer);
    let mut tools = String::from("\nTOOL PALETTE\n");
    for t in PALETTE.iter().filter(|t| {
        t.name != "write-output" && t.name != "worker" && t.name != "mcp-call" && t.implemented
    }) {
        tools.push_str(&format!("- {}: {}\n", t.name, t.summary));
    }
    format!(
        "{}\n{FORMAT_REFERENCE}{tools}{}\nUse these model ids in the models section unless told otherwise: {models_hint}.\nAsk at most one short question at a time. If the request is already clear, propose immediately.",
        spec.prompt,
        hands_on_this_machine(config)
    )
}

/// The external agents and MCP servers this machine actually has — so the
/// Interviewer can offer them, and never invents one that is not there.
pub fn hands_on_this_machine(config: &Config) -> String {
    let workers: Vec<String> = config
        .workers
        .iter()
        .filter_map(|w| {
            wright_tools::worker::resolve_command(&w.command)
                .ok()
                .map(|p| format!("- {} (runs {})", w.name, p.display()))
        })
        .collect();
    let mut out = String::from("\nWORKERS ON THIS MACHINE (external coding agents)\n");
    if workers.is_empty() {
        out.push_str("None are installed. Never write a `worker` line.\n");
    } else {
        out.push_str(&workers.join("\n"));
        out.push_str(
            "\nWhen one step is substantial coding inside the project — write a program, change code, make failing tests pass — \
hand that step to a worker instead of a plain agent:\n\
agent Coder {\n  goal \"One sentence: the code to write and what done looks like.\"\n  worker \"<name from the list>\"\n  worker-files \"src/\"              // the folders it may change; as narrow as works\n  out \"report.md\"                  // optional\n  approval \"before-send\"            // required: a worker sends the project copy to its own vendor\n  permissions files-read=true files-write=true spawn=true network=true\n}\n\
Its work is still checked by Wright's tests and Reviewer. Use a worker only for coding; a plain agent does the rest. \
Say in your summary that the step goes to that worker, so the person knows.\n",
        );
    }
    if !config.mcp_servers.is_empty() {
        out.push_str("\nMCP SERVERS ON THIS MACHINE (pinned)\n");
        for m in &config.mcp_servers {
            out.push_str(&format!("- {}\n", m.name));
        }
        out.push_str(
            "An agent may use one with `mcp server=\"<name>\" allow=\"<tool>,<tool>\"` — list only the tools it needs; \
its answers count as untrusted input.\n",
        );
    }
    out
}

fn propose_tool() -> ToolDef {
    ToolDef {
        name: "propose_blueprint".into(),
        description: "Propose the complete blueprint (KDL text). Returns VALID, or the problems to fix.".into(),
        schema: r#"{"type":"object","properties":{"kdl":{"type":"string","description":"The complete blueprint"}},"required":["kdl"]}"#.into(),
    }
}

/// Check a proposal: Ok(blueprint) or the problems, in words.
pub fn check_proposal(kdl: &str) -> Result<Blueprint, String> {
    let bp = parse::blueprint(kdl).map_err(|e| format!("It does not parse: {e}"))?;
    validate::validate(&bp).map_err(|errs| format!("It is not valid:\n- {}", errs.join("\n- ")))?;
    compiler::compile(&bp).map_err(|e| format!("It does not compile: {e}"))?;
    Ok(bp)
}

/// [`check_proposal`], and the organization's policy: a plan the policy
/// would refuse at firing is sent back now, so the Interviewer drafts
/// within the rules instead of the person finding out later.
pub fn check_proposal_with(kdl: &str, config: &Config) -> Result<Blueprint, String> {
    let bp = check_proposal(kdl)?;
    crate::org::enforce(&bp, config).map_err(|e| format!("It breaks the rules: {e}"))?;
    Ok(bp)
}

/// The Interviewer's model, provider and key: the configured one, or the
/// built-in model when that one has no provider or no key (and fallback
/// is on), with a word to the person about it.
fn interview_endpoint<'c>(
    config: &'c Config,
    io: &mut dyn InterviewIo,
) -> Result<(String, &'c ProviderEntry, Option<SecretValue>), String> {
    let model = config.interview_model.clone();
    let problem = match config.provider_for(&model) {
        None => format!(
            "no provider is configured for the interview model '{model}' ({})",
            config.source
        ),
        Some(entry) => match &entry.key {
            None => return Ok((model, entry, None)),
            Some(r) => match secrets::resolve(r) {
                Ok(k) => return Ok((model, entry, Some(k))),
                Err(e) => format!("the interview model '{model}' needs a key: {e}"),
            },
        },
    };
    match config.builtin_text() {
        Some(entry) if config.falls_back_to_builtin() && local::available() => {
            io.say(&format!("({problem}. Using the built-in model {} on this machine instead; add a key in Settings for a stronger interviewer.)", local::TEXT_MODEL));
            Ok((local::TEXT_MODEL.to_string(), entry, None))
        }
        _ => Err(problem),
    }
}

/// The messages to start from: an earlier conversation (if any), then
/// `next` from you. Turns alternate and start with you, so neighbours from
/// the same side are joined and a conversation that opened with Wright gets
/// a short note in front.
fn resumed(earlier: &[(bool, String)], next: String) -> Vec<Message> {
    let mut turns: Vec<(bool, String)> = Vec::new();
    for (you, text) in earlier
        .iter()
        .map(|(y, t)| (*y, t.trim()))
        .filter(|(_, t)| !t.is_empty())
        .chain([(true, next.trim())])
    {
        match turns.last_mut() {
            Some((side, joined)) if *side == you => {
                joined.push_str("\n\n");
                joined.push_str(text);
            }
            _ => turns.push((you, text.to_string())),
        }
    }
    if turns.first().is_some_and(|(you, _)| !you) {
        turns.insert(
            0,
            (true, "(We are picking up an earlier conversation.)".into()),
        );
    }
    turns
        .into_iter()
        .map(|(you, text)| {
            if you {
                Message::user_text(text)
            } else {
                Message {
                    role: Role::Assistant,
                    blocks: vec![Block::Text(text)],
                }
            }
        })
        .collect()
}

/// Run the interview. On confirmation, writes `blueprint.kdl` into
/// `project_dir` (and commits it to the store) and returns it.
pub fn interview(
    project_dir: &Path,
    config: &Config,
    io: &mut dyn InterviewIo,
) -> Result<Option<Blueprint>, String> {
    let (model, entry, key) = interview_endpoint(config, io)?;
    let client = entry.client();
    let net = NetConfig::from_env();

    let earlier = io.earlier();
    let first = match io.ask() {
        Some(t) if !t.trim().is_empty() => t,
        _ => return Ok(None),
    };
    let mut req = CompletionRequest::new(model.clone());
    req.system = system_prompt(&model, config);
    req.tools = vec![propose_tool()];
    req.messages = resumed(&earlier, first);

    let mut proposal: Option<Blueprint> = None;
    for _turn in 0..40 {
        let ep = [Endpoint {
            model: model.clone(),
            provider: client.as_ref(),
            api_key: key.as_ref().map(|k| k.expose().to_string()),
        }];
        io.working("Thinking");
        let mut so_far = String::new();
        let resp = {
            let io = &mut *io;
            let mut on_delta = |d: &models::Delta| match d {
                models::Delta::Text(t) => {
                    so_far.push_str(t);
                    io.partial(so_far.trim_start());
                }
                models::Delta::ToolStarted(name) if name == "propose_blueprint" => {
                    io.working("Drafting the plan")
                }
                models::Delta::ToolStarted(_) => {}
                models::Delta::Status(s) => io.working(s),
            };
            let mut on_retry = |_: &str| {};
            models::call(&ep, &req, &net, 3, &mut on_delta, &mut on_retry)?
        };
        req.messages.push(Message {
            role: Role::Assistant,
            blocks: resp.blocks.clone(),
        });
        let text = resp.text();
        if !text.trim().is_empty() {
            io.say(text.trim());
        }
        let uses = resp.tool_uses();
        if !uses.is_empty() {
            let mut results = Vec::new();
            for (id, name, input) in uses {
                let content = if name != "propose_blueprint" {
                    format!("Unknown tool '{name}'. Use propose_blueprint.")
                } else {
                    io.working("Checking the draft plan");
                    match check_proposal_with(input.str_of("kdl").unwrap_or(""), config) {
                        Ok(bp) => {
                            proposal = Some(bp);
                            "VALID. Wright will now show the person the summary.".into()
                        }
                        Err(problems) => {
                            proposal = None;
                            problems
                        }
                    }
                };
                results.push(Block::ToolResult {
                    id: id.to_string(),
                    is_error: !content.starts_with("VALID"),
                    content,
                });
            }
            req.messages.push(Message {
                role: Role::User,
                blocks: results,
            });
            continue;
        }
        // The Interviewer finished a turn without a tool call.
        if let Some(bp) = proposal.take() {
            let graph = compiler::compile(&bp)?;
            let lines = confirm::summary(&bp, &graph);
            if io.confirm(&lines) {
                return write_confirmed(project_dir, bp).map(Some);
            }
            io.say("What should change?");
        }
        match io.ask() {
            Some(t) if !t.trim().is_empty() => req.messages.push(Message::user_text(t)),
            _ => return Ok(None),
        }
    }
    Err("the interview went on too long without a confirmed blueprint".into())
}

/// Stamp, write and commit a confirmed blueprint.
pub fn write_confirmed(project_dir: &Path, mut bp: Blueprint) -> Result<Blueprint, String> {
    std::fs::create_dir_all(project_dir).map_err(|e| format!("{}: {e}", project_dir.display()))?;
    let store = Store::open(project_dir)?;
    let version = store.blueprint_history().len() + 1;
    bp.intent.confirmed = format!("v{version}");
    if bp.meta.created.is_empty() {
        bp.meta.created =
            wright_core::schedule::rfc3339_utc((wright_core::events::now_ms() / 1000) as i64);
    }
    let text = format!(
        "// blueprint.kdl — {}\n// Written by Wright after you confirmed it. The UI edits this file;\n// you may edit it by hand too (`wright plan` checks your edit).\n\n{}",
        bp.meta.name,
        canon::canonical(&bp)
    );
    write_atomic(&project_dir.join("blueprint.kdl"), text.as_bytes())?;
    store.commit_blueprint(
        &canon::canonical(&bp),
        &format!("confirmed {}", bp.intent.confirmed),
    )?;
    Ok(bp)
}

/// A scripted conversation, for tests and `wright new --from`.
pub struct Scripted {
    pub answers: Vec<String>,
    pub confirm_answer: bool,
    pub transcript: Vec<String>,
}

impl InterviewIo for Scripted {
    fn say(&mut self, text: &str) {
        self.transcript.push(format!("wright: {text}"));
    }
    fn ask(&mut self) -> Option<String> {
        if self.answers.is_empty() {
            None
        } else {
            let a = self.answers.remove(0);
            self.transcript.push(format!("you: {a}"));
            Some(a)
        }
    }
    fn confirm(&mut self, lines: &[Line]) -> bool {
        for l in lines {
            self.transcript.push(format!("summary: {}", l.text));
        }
        self.confirm_answer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proposals_are_checked_by_wright_not_trusted() {
        let bad = "meta { name \"x\" }\nintent { summary \"s\" confirmed \"\" }\nagent A { goal \"g\" tools \"http-read\" }\ndeliverable kind=\"k\" { check \"c\" }";
        let err = check_proposal(bad).unwrap_err();
        assert!(err.contains("not allowed to use the network"), "{err}");
        assert!(check_proposal("garbage {").unwrap_err().contains("parse"));
    }

    #[test]
    fn an_earlier_conversation_is_picked_up_in_turns() {
        let roles = |m: &[Message]| m.iter().map(|m| m.role == Role::User).collect::<Vec<_>>();
        // Fresh: just the message.
        let m = resumed(&[], "make a digest".into());
        assert_eq!(
            (roles(&m), m[0].blocks.clone()),
            (vec![true], vec![Block::Text("make a digest".into())])
        );
        // Wright spoke first (a fallback note), you twice in a row, blanks dropped.
        let earlier = vec![
            (false, "(using the built-in model)".to_string()),
            (true, "a digest".into()),
            (false, "Of what?".into()),
            (true, "my sales folder".into()),
            (true, "  ".into()),
        ];
        let m = resumed(&earlier, "weekly".into());
        assert_eq!(roles(&m), vec![true, false, true, false, true]);
        assert_eq!(
            m[4].blocks,
            vec![Block::Text("my sales folder\n\nweekly".into())]
        );
    }

    #[test]
    fn the_reference_mentions_every_implemented_tool() {
        let p = system_prompt("m", &Config::builtin());
        // `worker` and `mcp-call` are reached through their own card lines
        // (`worker "..."`, `mcp server=...`), offered only when this machine
        // has one — never as a bare tool name.
        for t in PALETTE
            .iter()
            .filter(|t| t.implemented && !["write-output", "worker", "mcp-call"].contains(&t.name))
        {
            assert!(p.contains(t.name), "{}", t.name);
        }
        assert!(p.contains("accept \""), "acceptance sentences are taught");
        assert!(p.contains("schema="), "typed edges are taught");
    }

    #[test]
    fn workers_are_offered_only_when_this_machine_has_one() {
        let mut cfg = Config::parse("", "t").unwrap();
        for w in cfg.workers.iter_mut() {
            w.command = "/no/such/agent".into();
        }
        let none = hands_on_this_machine(&cfg);
        assert!(none.contains("Never write a `worker` line"), "{none}");
        cfg.workers[0].command = "sh".into(); // on every PATH
        let one = hands_on_this_machine(&cfg);
        assert!(
            one.contains(&format!("- {} (runs", cfg.workers[0].name)),
            "{one}"
        );
        assert!(
            one.contains("approval \\\"before-send\\\"")
                || one.contains("approval \"before-send\""),
            "{one}"
        );
        assert!(
            !one.contains("MCP SERVERS"),
            "no MCP section without servers"
        );
    }
}
