//! The agent loop: one task, many turns, every tool call through the gate.
//!
//! ```text
//! build prompt (contract + inputs)
//! loop:
//!     check: cancelled? past the wall clock? out of turns? out of budget?
//!     call the model (streaming; retries; fallback model)
//!     record tokens and cost (actual, from the provider's usage)
//!     no tool calls?  -> done: check the output contract, return
//!     for each tool call:
//!         gate.check -> Allow(permit) -> tools::run(permit, ...)
//!                    -> Ask           -> a human decides (Approver)
//!                    -> Deny          -> the model is told why
//!     append the results, compact old tool output if the context is large
//! ```
//!
//! Every step lands on the event spine as it happens, so the Event River
//! (and a crash) always shows exactly how far an agent got.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use wright_core::blueprint::Blueprint;
use wright_core::compiler::{SystemKind, Task, TaskGraph, TaskKind};
use wright_core::events::EventKind;
use wright_core::palette;
use wright_core::wire::json::{self, Value};
use wright_runtime::config::Config;
use wright_runtime::gate::{BudgetMeter, Gate, Verdict};
use wright_runtime::local;
use wright_runtime::models::{self, Block, CompletionRequest, Endpoint, Message, Role, StopReason, ToolDef};
use wright_runtime::net::NetConfig;
use wright_runtime::secrets;
use wright_runtime::store::Store;
use wright_tools::{ToolCtx, ToolError};

use crate::approve::{Approver, Ask};
use crate::spine::Spine;

/// Everything shared by the agents of one firing.
pub struct FiringEnv {
    pub bp: Blueprint,
    pub graph: TaskGraph,
    pub gate: Gate,
    /// ONE meter for the whole firing.
    pub budget: Mutex<BudgetMeter>,
    /// Model turns left for the whole firing (guardrails.max-turns).
    pub turns_left: AtomicI64,
    pub spine: Spine,
    pub config: Config,
    pub net: NetConfig,
    pub store: Store,
    pub tools: ToolCtx,
    pub approver: Box<dyn Approver>,
    pub approval_seq: AtomicU32,
    pub deadline: Instant,
    pub cancel: AtomicBool,
    /// Final messages by task name (the Reviewer reads the Testwright's).
    pub reports: Mutex<BTreeMap<String, String>>,
    /// Files written this firing, by task name.
    pub written: Mutex<BTreeMap<String, Vec<String>>>,
}

/// A provider client, the model id it serves, and its resolved key.
type ResolvedEndpoint = (Box<dyn models::Provider>, String, Option<secrets::SecretValue>);

/// What one agent run produced.
#[derive(Debug, Clone, Default)]
pub struct AgentResult {
    pub final_text: String,
    pub cost_micro_usd: u64,
    pub turns: u32,
}

/// Output-token ceiling per model turn (part of every cache key).
pub const MAX_TOKENS: u32 = 8192;

/// Characters of conversation above which old tool output is elided.
const COMPACT_AT_CHARS: usize = 300_000;
/// Inline at most this much of each input artifact into the prompt.
const INLINE_INPUT: usize = 48 * 1024;

fn short(s: &str, n: usize) -> String {
    let one_line: String = s.chars().map(|c| if c == '\n' { ' ' } else { c }).collect();
    if one_line.chars().count() <= n {
        one_line
    } else {
        format!("{}...", one_line.chars().take(n).collect::<String>())
    }
}

fn usd(micro: u64) -> String {
    if micro < 10_000 {
        format!("${:.4}", micro as f64 / 1e6)
    } else {
        format!("${:.2}", micro as f64 / 1e6)
    }
}

/// A human-sized summary of a tool input (no file contents in events).
fn input_summary(tool: &str, input: &Value) -> String {
    match tool {
        "write-output" => format!("{} ({} bytes)", input.str_of("name").unwrap_or("?"), input.str_of("content").map(str::len).unwrap_or(0)),
        "files-write" => format!("{} ({} bytes)", input.str_of("path").unwrap_or("?"), input.str_of("content").map(str::len).unwrap_or(0)),
        "files-read" | "files-list" | "edit" => input.str_of("path").unwrap_or(".").to_string(),
        "apply-patch" => format!("{} line patch", input.str_of("patch").map(|p| p.lines().count()).unwrap_or(0)),
        "shell" => short(input.str_of("command").unwrap_or(""), 120),
        "http-read" | "http-post" => input.str_of("url").unwrap_or("?").to_string(),
        "git" => input.get("args").map(json::to_string).unwrap_or_default(),
        _ => short(&json::to_string(input), 120),
    }
}

impl FiringEnv {
    fn task_name(task: &Task) -> String {
        task.name()
    }

    /// The system prompt and first user message for a task.
    fn prompts(&self, task: &Task) -> (String, String) {
        let bp = &self.bp;
        let mut user = String::new();
        let system = match &task.kind {
            TaskKind::Project(name) => {
                let agent = bp.agent(name).expect("compiled from this blueprint");
                for input in &agent.inputs {
                    let path = self.tools.artifacts_dir().join(input);
                    match std::fs::read(&path) {
                        Ok(bytes) => {
                            let text = String::from_utf8_lossy(&bytes);
                            let shown: String = text.chars().take(INLINE_INPUT).collect();
                            user.push_str(&format!("=== input: {input} ({} bytes) ===\n{shown}\n", bytes.len()));
                            if shown.len() < text.len() {
                                user.push_str("[... truncated; read the rest with files-read if you have it ...]\n");
                            }
                        }
                        Err(_) => user.push_str(&format!("=== input: {input} === (MISSING — the upstream agent did not produce it)\n")),
                    }
                }
                user.push_str("\nDo your job now.");
                wright_agents::project_agent_prompt(bp, agent)
            }
            TaskKind::System(kind) => {
                let spec = wright_agents::spec(*kind);
                user.push_str(&format!("Project: {}\nIntent: {}\n", bp.meta.name, bp.intent.summary));
                user.push_str(&format!("Language: {}\n", bp.project.language));
                let test = wright_core::convention::test_cmd(bp);
                if !test.is_empty() {
                    user.push_str(&format!("Test command: {test}\n"));
                }
                user.push_str("\nDefinition of done (the deliverables):\n");
                for d in &bp.deliverables {
                    user.push_str(&format!("- [{}] {}\n", d.kind, d.check));
                }
                user.push_str("\nArtifacts produced so far:\n");
                let mut any = false;
                if let Ok(rd) = std::fs::read_dir(self.tools.artifacts_dir()) {
                    let mut names: Vec<_> = rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| !n.starts_with('.')).collect();
                    names.sort();
                    for n in names {
                        any = true;
                        let text = std::fs::read_to_string(self.tools.artifacts_dir().join(&n)).unwrap_or_default();
                        user.push_str(&format!("=== artifacts/{n} ({} bytes) ===\n{}\n", text.len(), text.chars().take(4096).collect::<String>()));
                    }
                }
                if !any {
                    user.push_str("(none)\n");
                }
                let written = self.written.lock().unwrap_or_else(|p| p.into_inner());
                if !written.is_empty() {
                    user.push_str("\nFiles changed in this firing:\n");
                    for (who, files) in written.iter() {
                        user.push_str(&format!("- {who}: {}\n", files.join(", ")));
                    }
                }
                drop(written);
                if *kind == SystemKind::Reviewer {
                    if let Some(t) = self.reports.lock().unwrap_or_else(|p| p.into_inner()).get("Testwright") {
                        user.push_str(&format!("\nThe Testwright's report:\n{t}\n"));
                    }
                }
                user.push_str("\nDo your job now.");
                format!(
                    "{}\n\nYou are working inside a Wright project that follows the universal project convention (src/, tests/, logs/, artifacts/; see AGENTS.md in the project root). Work only with your tools. Nobody is watching: do not ask questions.",
                    spec.prompt
                )
            }
        };
        (system, user)
    }

    fn endpoints_for(&self, task: &Task) -> Result<Vec<ResolvedEndpoint>, String> {
        let resolved = task.model.as_ref().ok_or("built-in tasks do not call models")?;
        let mut out = Vec::new();
        let mut problems = Vec::new();
        for model in [&resolved.primary, &resolved.fallback] {
            if model.is_empty() {
                continue;
            }
            let Some(entry) = self.config.provider_for(model) else {
                problems.push(format!("no provider is configured for model '{model}' (add one to {})", self.config.source));
                continue;
            };
            let key = match &entry.key {
                None => None,
                Some(r) => match secrets::resolve(r) {
                    Ok(v) => Some(v),
                    Err(e) => {
                        problems.push(format!("model '{model}' ({}): {e}", entry.name));
                        continue;
                    }
                },
            };
            out.push((entry.client(), model.clone(), key));
        }
        // The built-in model, last: it keeps a project moving when no
        // provider or key is set up, and answers if every cloud endpoint
        // fails mid-run. `builtin fallback=false` turns this off.
        if self.config.falls_back_to_builtin() && local::available() {
            if let Some(entry) = self.config.builtin_text() {
                if !out.iter().any(|(_, m, _)| m == local::TEXT_MODEL) {
                    if out.is_empty() {
                        let name = Self::task_name(task);
                        self.spine.note(EventKind::Note, Some(&name), format!("{}. Using the built-in model {} (on this machine, free) instead.", problems.join("; "), local::TEXT_MODEL));
                    }
                    out.push((entry.client(), local::TEXT_MODEL.to_string(), None));
                }
            }
        }
        if out.is_empty() {
            return Err(problems.join("; "));
        }
        Ok(out)
    }

    /// Run one model-backed task to completion.
    pub fn run_agent(&self, task: &Task) -> Result<AgentResult, String> {
        let name = Self::task_name(task);
        let me = Some(name.as_str());
        let endpoints = self.endpoints_for(task)?;
        let (system, first_user) = self.prompts(task);

        let mut tool_ctx = self.tools.clone();
        if let TaskKind::Project(n) = &task.kind {
            tool_ctx.outputs = self.bp.agent(n).map(|a| a.outputs.clone()).unwrap_or_default();
        }
        let tool_defs: Vec<ToolDef> = task
            .tools
            .iter()
            .filter_map(|t| palette::find(t))
            .filter(|t| t.implemented)
            .map(|t| ToolDef { name: t.name.into(), description: t.summary.into(), schema: t.schema.into() })
            .collect();

        let mut req = CompletionRequest::new(endpoints[0].1.clone());
        req.system = system;
        req.tools = tool_defs;
        req.messages.push(Message::user_text(first_user));
        req.max_tokens = MAX_TOKENS;

        let mut result = AgentResult::default();
        loop {
            if self.cancel.load(Ordering::SeqCst) {
                return Err("cancelled".into());
            }
            if Instant::now() >= self.deadline {
                return Err(format!("the firing's wall clock ({}s) ran out", self.bp.guardrails.wall_clock_secs));
            }
            if self.turns_left.fetch_sub(1, Ordering::SeqCst) <= 0 {
                return Err(format!("the firing used all {} model turns (guardrails.max-turns)", self.bp.guardrails.max_turns));
            }
            // Estimate before spending: prompt chars / 4 at the input price.
            let chars: usize = req.system.len() + req.messages.iter().map(Message::text_len).sum::<usize>();
            let price = self.config.price_of(&endpoints[0].1);
            let est = price.cost_micro((chars / 4) as u64, 0, 512);
            {
                let budget = self.budget.lock().unwrap_or_else(|p| p.into_inner());
                if !budget.can_afford(est) {
                    return Err(format!(
                        "budget exhausted: {} of {} spent, next turn would cost about {}",
                        usd(budget.spent_micro_usd),
                        usd(budget.cap_micro_usd),
                        usd(est)
                    ));
                }
            }

            let eps: Vec<Endpoint<'_>> = endpoints
                .iter()
                .map(|(p, m, k)| Endpoint { model: m.clone(), provider: p.as_ref(), api_key: k.as_ref().map(|v| v.expose().to_string()) })
                .collect();
            let spine = &self.spine;
            // Status lines (the built-in model downloading on first use)
            // are worth a line in the river; streamed text is not.
            let mut on_delta = |d: &models::Delta| {
                if let models::Delta::Status(s) = d {
                    spine.note(EventKind::Note, me, s.clone());
                }
            };
            let mut on_retry = |msg: &str| spine.note(EventKind::AgentRetrying, me, msg.to_string());
            let resp = models::call(&eps, &req, &self.net, 3, &mut on_delta, &mut on_retry)?;
            result.turns += 1;

            let price = self.config.price_of(&resp.model);
            let cost = price.cost_micro(resp.usage.input_tokens, resp.usage.cached_input_tokens, resp.usage.output_tokens);
            result.cost_micro_usd += cost;
            self.budget.lock().unwrap_or_else(|p| p.into_inner()).record(cost);
            self.spine.emit(
                EventKind::TokensRecorded,
                me,
                format!(
                    "{} tokens in ({} cached), {} out on {} — {}",
                    resp.usage.input_tokens, resp.usage.cached_input_tokens, resp.usage.output_tokens, resp.model, usd(cost)
                ),
                json::to_string(&Value::obj(vec![
                    ("in", Value::int(resp.usage.input_tokens as i64)),
                    ("cached", Value::int(resp.usage.cached_input_tokens as i64)),
                    ("out", Value::int(resp.usage.output_tokens as i64)),
                    ("model", Value::str(resp.model.clone())),
                    ("cost_micro_usd", Value::int(cost as i64)),
                ])),
            )?;
            let text = resp.text();
            if !text.trim().is_empty() {
                self.spine.emit(EventKind::ModelText, me, short(&text, 280), String::new())?;
            }
            req.messages.push(Message { role: Role::Assistant, blocks: resp.blocks.clone() });

            let uses = resp.tool_uses();
            if uses.is_empty() {
                if resp.stop == StopReason::MaxTokens {
                    return Err("the model hit its output limit before finishing".into());
                }
                result.final_text = text;
                return Ok(result);
            }

            let mut results = Vec::new();
            for (id, tool, input) in uses {
                let content = self.one_tool_call(task, &name, tool, input, &tool_ctx);
                let (content, is_error) = match content {
                    Ok(t) => (t, false),
                    Err(t) => (t, true),
                };
                results.push(Block::ToolResult { id: id.to_string(), content, is_error });
            }
            req.messages.push(Message { role: Role::User, blocks: results });
            compact(&mut req.messages);
        }
    }

    /// One tool call through the gate. Ok(text) or Err(text) for the model.
    fn one_tool_call(&self, task: &Task, name: &str, tool: &str, input: &Value, ctx: &ToolCtx) -> Result<String, String> {
        let me = Some(name);
        let summary = input_summary(tool, input);
        let verdict = {
            let mut budget = self.budget.lock().unwrap_or_else(|p| p.into_inner());
            self.gate.check(name, tool, &mut budget, 0)
        };
        let permit = match verdict {
            Verdict::Allow(p) => p,
            Verdict::Deny(why) => {
                self.spine.note(EventKind::ToolDenied, me, format!("{tool} {summary}: {why}"));
                return Err(format!("DENIED by the permission gate: {why}"));
            }
            Verdict::Ask(pending) => {
                let n = self.approval_seq.fetch_add(1, Ordering::SeqCst) + 1;
                let preview = match tool {
                    "http-post" => format!("POST {}\n{}", input.str_of("url").unwrap_or("?"), input.str_of("body").unwrap_or("").chars().take(1200).collect::<String>()),
                    _ => short(&json::to_string(input), 600),
                };
                let ask = Ask { run: self.spine.run(), n, agent: name.into(), tool: tool.into(), reason: pending.reason.clone(), preview };
                self.spine.note(EventKind::ApprovalRequested, me, format!("{} (request {n}, asking via {})", pending.reason, self.approver.name()));
                let approved = self.approver.decide(&ask);
                self.spine.note(EventKind::ApprovalResolved, me, if approved { format!("request {n} approved") } else { format!("request {n} denied") });
                if !approved {
                    return Err("DENIED: a human declined this action. Do not retry it; finish without it and say so.".into());
                }
                let mut budget = self.budget.lock().unwrap_or_else(|p| p.into_inner());
                match self.gate.approve(pending, &mut budget) {
                    Ok(p) => p,
                    Err(e) => return Err(format!("DENIED: {e}")),
                }
            }
        };
        let _ = task;
        let started = Instant::now();
        let out = wright_tools::run(permit, input, ctx);
        let ms = started.elapsed().as_millis();
        match out {
            Ok(o) => {
                self.spine.note(EventKind::ToolCalled, me, format!("{tool} {summary} — ok ({ms} ms)"));
                if !o.wrote.is_empty() {
                    for f in &o.wrote {
                        let h = std::fs::read(ctx.project.join(f)).map(|b| wright_core::hash::blake3_hex(&b)).unwrap_or_default();
                        let _ = self.spine.emit(
                            EventKind::ArtifactWritten,
                            me,
                            f.clone(),
                            json::to_string(&Value::obj(vec![("path", Value::str(f.clone())), ("hash", Value::str(h))])),
                        );
                    }
                    self.written.lock().unwrap_or_else(|p| p.into_inner()).entry(name.to_string()).or_default().extend(o.wrote.iter().cloned());
                }
                Ok(o.text)
            }
            Err(e) => {
                let msg = e.to_string();
                self.spine.note(EventKind::ToolCalled, me, format!("{tool} {summary} — error: {}", short(&msg, 160)));
                if matches!(e, ToolError::NotImplemented(_)) {
                    Err(format!("{msg}. Use another tool or finish without it."))
                } else {
                    Err(msg)
                }
            }
        }
    }
}

/// Elide the content of old tool results once the conversation is big.
/// The two most recent exchanges are always kept whole.
fn compact(messages: &mut [Message]) {
    let total: usize = messages.iter().map(Message::text_len).sum();
    if total <= COMPACT_AT_CHARS {
        return;
    }
    let keep_from = messages.len().saturating_sub(4);
    for m in messages[1..keep_from].iter_mut() {
        for b in m.blocks.iter_mut() {
            if let Block::ToolResult { content, .. } = b {
                if content.len() > 400 {
                    *content = format!("[elided by Wright to save context: {} bytes; call the tool again if you need it]", content.len());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compaction_keeps_recent_turns_whole() {
        let big = "x".repeat(200_000);
        let mut msgs = vec![Message::user_text("start")];
        for i in 0..4 {
            msgs.push(Message { role: Role::Assistant, blocks: vec![Block::Text(format!("turn {i}"))] });
            msgs.push(Message { role: Role::User, blocks: vec![Block::ToolResult { id: format!("t{i}"), content: big.clone(), is_error: false }] });
        }
        compact(&mut msgs);
        let last = msgs.last().unwrap();
        assert!(matches!(&last.blocks[0], Block::ToolResult { content, .. } if content.len() == 200_000));
        assert!(matches!(&msgs[2].blocks[0], Block::ToolResult { content, .. } if content.starts_with("[elided")));
        assert!(msgs.iter().map(Message::text_len).sum::<usize>() < 500_000);
    }

    #[test]
    fn summaries_never_carry_file_contents() {
        let input = json::parse(r#"{"path":"a.txt","content":"SECRET-ISH CONTENT"}"#).unwrap();
        assert_eq!(input_summary("files-write", &input), "a.txt (18 bytes)");
    }
}
