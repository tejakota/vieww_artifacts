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
use wright_runtime::config::Config;
use wright_runtime::models::{self, Block, CompletionRequest, Endpoint, Message, Role, ToolDef};
use wright_runtime::net::NetConfig;
use wright_runtime::secrets;
use wright_runtime::store::{write_atomic, Store};

/// How the interview talks to a person.
pub trait InterviewIo {
    /// Show the Interviewer's words.
    fn say(&mut self, text: &str);
    /// Ask the person for their next message. None = they left.
    fn ask(&mut self) -> Option<String>;
    /// Show the Confirm Gate summary; true = confirmed.
    fn confirm(&mut self, lines: &[Line]) -> bool;
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
deliverable kind="email" { check "What must be true when it is done." }
models { tier cheap prefer="model-id" fallback="model-id" tier quality prefer="model-id" fallback="model-id" }

Rules: every tool's capability must be granted in permissions; every `in` must be produced
by an agent connected by edges; secrets are chips { secrets name="keychain:path" } — never values.
"#;

fn system_prompt(models_hint: &str) -> String {
    let spec = wright_agents::spec(compiler::SystemKind::Interviewer);
    let mut tools = String::from("\nTOOL PALETTE\n");
    for t in PALETTE.iter().filter(|t| t.name != "write-output" && t.implemented) {
        tools.push_str(&format!("- {}: {}\n", t.name, t.summary));
    }
    format!("{}\n{FORMAT_REFERENCE}{tools}\nUse these model ids in the models section unless told otherwise: {models_hint}.\nAsk at most one short question at a time. If the request is already clear, propose immediately.", spec.prompt)
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

/// Run the interview. On confirmation, writes `blueprint.kdl` into
/// `project_dir` (and commits it to the store) and returns it.
pub fn interview(project_dir: &Path, config: &Config, io: &mut dyn InterviewIo) -> Result<Option<Blueprint>, String> {
    let model = config.interview_model.clone();
    let entry = config.provider_for(&model).ok_or_else(|| format!("no provider is configured for the interview model '{model}' ({})", config.source))?;
    let key = match &entry.key {
        Some(r) => Some(secrets::resolve(r)?),
        None => None,
    };
    let client = entry.client();
    let net = NetConfig::from_env();

    let first = match io.ask() {
        Some(t) if !t.trim().is_empty() => t,
        _ => return Ok(None),
    };
    let mut req = CompletionRequest::new(model.clone());
    req.system = system_prompt(&model);
    req.tools = vec![propose_tool()];
    req.messages.push(Message::user_text(first));

    let mut proposal: Option<Blueprint> = None;
    for _turn in 0..40 {
        let ep = [Endpoint { model: model.clone(), provider: client.as_ref(), api_key: key.as_ref().map(|k| k.expose().to_string()) }];
        let resp = models::call(&ep, &req, &net, 3, &mut |_| {}, &mut |_| {})?;
        req.messages.push(Message { role: Role::Assistant, blocks: resp.blocks.clone() });
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
                    match check_proposal(input.str_of("kdl").unwrap_or("")) {
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
                results.push(Block::ToolResult { id: id.to_string(), is_error: !content.starts_with("VALID"), content });
            }
            req.messages.push(Message { role: Role::User, blocks: results });
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
        bp.meta.created = wright_core::schedule::rfc3339_utc((wright_core::events::now_ms() / 1000) as i64);
    }
    let text = format!(
        "// blueprint.kdl — {}\n// Written by Wright after you confirmed it. The UI edits this file;\n// you may edit it by hand too (`wright plan` checks your edit).\n\n{}",
        bp.meta.name,
        canon::canonical(&bp)
    );
    write_atomic(&project_dir.join("blueprint.kdl"), text.as_bytes())?;
    store.commit_blueprint(&canon::canonical(&bp), &format!("confirmed {}", bp.intent.confirmed))?;
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
    fn the_reference_mentions_every_implemented_tool() {
        let p = system_prompt("m");
        for t in PALETTE.iter().filter(|t| t.implemented && t.name != "write-output") {
            assert!(p.contains(t.name), "{}", t.name);
        }
    }
}
