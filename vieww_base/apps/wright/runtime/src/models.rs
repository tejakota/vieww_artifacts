//! Model providers: Anthropic Messages and OpenAI-compatible chat
//! completions (OpenAI, DeepSeek, GLM/Z.ai, Kimi/Moonshot, local
//! gateways, and Wright's own mock provider), with tool calling and
//! streaming.
//!
//! Shape (rig's best idea — one unified surface — implemented from scratch):
//!
//! * [`CompletionRequest`] / [`CompletionResponse`] — one vocabulary of
//!   messages made of [`Block`]s: text, tool use, tool result.
//! * [`Provider`] — pure functions: `build_request` (request -> HTTP) and
//!   a [`StreamDecoder`] (SSE events -> deltas -> final response). Pure
//!   means the tests below need no network: canned bytes in, values out.
//! * [`call`] — the one impure function: send, stream, retry on 429/5xx
//!   with backoff (honoring `retry-after`), fall back to the tier's second
//!   model after repeated failures.
//!
//! Secrets: the API key arrives as a resolved value at send time (from
//! `secrets.rs`), goes into one header, and is never stored, logged or
//! put in an event. `build_request` takes it as an argument precisely so
//! tests can prove where it goes.

use std::io::Read;
use std::time::Duration;

use wright_core::wire::http::{parse_url, Request};
use wright_core::wire::json::{self, Value};
use wright_core::wire::sse::{SseMessage, SseParser};

use crate::net::{self, NetConfig};

// ── vocabulary ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Text(String),
    /// The model asks to run a tool. `input` is a JSON object.
    ToolUse { id: String, name: String, input: Value },
    /// The harness answers a tool use.
    ToolResult { id: String, content: String, is_error: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub role: Role,
    pub blocks: Vec<Block>,
}

impl Message {
    pub fn user_text(t: impl Into<String>) -> Self {
        Message { role: Role::User, blocks: vec![Block::Text(t.into())] }
    }

    /// Total characters of text in this message (for compaction).
    pub fn text_len(&self) -> usize {
        self.blocks
            .iter()
            .map(|b| match b {
                Block::Text(t) => t.len(),
                Block::ToolUse { input, .. } => json::to_string(input).len(),
                Block::ToolResult { content, .. } => content.len(),
            })
            .sum()
    }
}

/// A tool as the model sees it.
#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    /// JSON schema of the input object.
    pub schema: String,
}

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub model: String,
    pub system: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDef>,
    pub max_tokens: u32,
    pub stream: bool,
}

impl CompletionRequest {
    pub fn new(model: impl Into<String>) -> Self {
        CompletionRequest { model: model.into(), system: String::new(), messages: Vec::new(), tools: Vec::new(), max_tokens: 4096, stream: true }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Tokens served from the provider's prompt cache (cheaper).
    pub cached_input_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    Other(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompletionResponse {
    /// The assistant's blocks: text and tool uses, in order.
    pub blocks: Vec<Block>,
    pub usage: Usage,
    pub stop: StopReason,
    /// The model that actually answered (after any fallback).
    pub model: String,
}

impl CompletionResponse {
    pub fn text(&self) -> String {
        self.blocks
            .iter()
            .filter_map(|b| if let Block::Text(t) = b { Some(t.as_str()) } else { None })
            .collect::<Vec<_>>()
            .join("")
    }

    pub fn tool_uses(&self) -> Vec<(&str, &str, &Value)> {
        self.blocks
            .iter()
            .filter_map(|b| match b {
                Block::ToolUse { id, name, input } => Some((id.as_str(), name.as_str(), input)),
                _ => None,
            })
            .collect()
    }
}

/// Incremental output while a response streams in.
#[derive(Debug, Clone, PartialEq)]
pub enum Delta {
    Text(String),
    ToolStarted(String),
}

// ── provider trait ───────────────────────────────────────────────

pub trait StreamDecoder: Send {
    /// Feed one SSE message; return any deltas it produced.
    fn on_message(&mut self, msg: &SseMessage) -> Result<Vec<Delta>, String>;
    /// The stream ended: assemble the final response.
    fn finish(self: Box<Self>) -> Result<CompletionResponse, String>;
}

pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    /// Build the HTTP request. Pure (the key is an argument).
    fn build_request(&self, req: &CompletionRequest, api_key: Option<&str>) -> Result<Request, String>;
    /// Parse a complete non-streaming body. Pure.
    fn parse_response(&self, body: &[u8]) -> Result<CompletionResponse, String>;
    /// A fresh decoder for one streaming response.
    fn decoder(&self) -> Box<dyn StreamDecoder>;
}

fn tool_input_value(text: &str) -> Value {
    if text.trim().is_empty() {
        Value::Obj(Vec::new())
    } else {
        json::parse(text).unwrap_or_else(|_| Value::obj(vec![("_unparsed", Value::str(text))]))
    }
}

// ── OpenAI-compatible ────────────────────────────────────────────

/// Any `/chat/completions` endpoint. `base_url` includes the version
/// segment: `https://api.openai.com/v1`, `https://api.deepseek.com/v1`,
/// `https://api.z.ai/api/paas/v4`, `https://api.moonshot.ai/v1`.
#[derive(Debug, Clone)]
pub struct OpenAiCompatible {
    pub base_url: String,
}

impl OpenAiCompatible {
    fn messages(req: &CompletionRequest) -> Vec<Value> {
        let mut out = Vec::new();
        if !req.system.is_empty() {
            out.push(Value::obj(vec![("role", Value::str("system")), ("content", Value::str(req.system.clone()))]));
        }
        for m in &req.messages {
            match m.role {
                Role::Assistant => {
                    let text: String = m.blocks.iter().filter_map(|b| if let Block::Text(t) = b { Some(t.as_str()) } else { None }).collect();
                    let calls: Vec<Value> = m
                        .blocks
                        .iter()
                        .filter_map(|b| match b {
                            Block::ToolUse { id, name, input } => Some(Value::obj(vec![
                                ("id", Value::str(id.clone())),
                                ("type", Value::str("function")),
                                ("function", Value::obj(vec![("name", Value::str(name.clone())), ("arguments", Value::str(json::to_string(input)))])),
                            ])),
                            _ => None,
                        })
                        .collect();
                    let mut pairs = vec![("role", Value::str("assistant")), ("content", if text.is_empty() { Value::Null } else { Value::str(text) })];
                    if !calls.is_empty() {
                        pairs.push(("tool_calls", Value::Arr(calls)));
                    }
                    out.push(Value::obj(pairs));
                }
                Role::User => {
                    // Tool results become role=tool messages; text stays user.
                    let mut text = String::new();
                    for b in &m.blocks {
                        match b {
                            Block::ToolResult { id, content, is_error } => {
                                let c = if *is_error { format!("ERROR: {content}") } else { content.clone() };
                                out.push(Value::obj(vec![("role", Value::str("tool")), ("tool_call_id", Value::str(id.clone())), ("content", Value::str(c))]));
                            }
                            Block::Text(t) => text.push_str(t),
                            Block::ToolUse { .. } => {}
                        }
                    }
                    if !text.is_empty() {
                        out.push(Value::obj(vec![("role", Value::str("user")), ("content", Value::str(text))]));
                    }
                }
            }
        }
        out
    }
}

impl Provider for OpenAiCompatible {
    fn name(&self) -> &str {
        "openai-compatible"
    }

    fn build_request(&self, req: &CompletionRequest, api_key: Option<&str>) -> Result<Request, String> {
        let mut body = vec![
            ("model", Value::str(req.model.clone())),
            ("messages", Value::Arr(Self::messages(req))),
            ("max_tokens", Value::int(req.max_tokens as i64)),
        ];
        if !req.tools.is_empty() {
            let tools: Vec<Value> = req
                .tools
                .iter()
                .map(|t| {
                    Value::obj(vec![
                        ("type", Value::str("function")),
                        (
                            "function",
                            Value::obj(vec![
                                ("name", Value::str(t.name.clone())),
                                ("description", Value::str(t.description.clone())),
                                ("parameters", json::parse(&t.schema).unwrap_or(Value::Obj(vec![]))),
                            ]),
                        ),
                    ])
                })
                .collect();
            body.push(("tools", Value::Arr(tools)));
        }
        if req.stream {
            body.push(("stream", Value::Bool(true)));
            body.push(("stream_options", Value::obj(vec![("include_usage", Value::Bool(true))])));
        }
        let url = parse_url(&format!("{}/chat/completions", self.base_url.trim_end_matches('/')))?;
        let mut r = Request::new("POST", url).header("Content-Type", "application/json").header("Accept", "text/event-stream, application/json");
        if let Some(k) = api_key {
            r = r.header("Authorization", &format!("Bearer {k}"));
        }
        Ok(r.json_body(json::to_string(&Value::obj(body))))
    }

    fn parse_response(&self, body: &[u8]) -> Result<CompletionResponse, String> {
        let v = json::parse(&String::from_utf8_lossy(body))?;
        let msg = v.path("choices", &["0", "message"]).ok_or("response has no choices[0].message")?;
        let mut blocks = Vec::new();
        if let Some(t) = msg.str_of("content") {
            if !t.is_empty() {
                blocks.push(Block::Text(t.to_string()));
            }
        }
        if let Some(calls) = msg.get("tool_calls").and_then(|c| c.as_arr()) {
            for c in calls {
                blocks.push(Block::ToolUse {
                    id: c.str_of("id").unwrap_or("call").to_string(),
                    name: c.path("function", &["name"]).and_then(|n| n.as_str()).unwrap_or("").to_string(),
                    input: tool_input_value(c.path("function", &["arguments"]).and_then(|a| a.as_str()).unwrap_or("")),
                });
            }
        }
        let finish = v.path("choices", &["0", "finish_reason"]).and_then(|f| f.as_str()).unwrap_or("");
        Ok(CompletionResponse {
            blocks,
            usage: openai_usage(v.get("usage")),
            stop: openai_stop(finish),
            model: v.str_of("model").unwrap_or("").to_string(),
        })
    }

    fn decoder(&self) -> Box<dyn StreamDecoder> {
        Box::new(OpenAiDecoder::default())
    }
}

fn openai_usage(u: Option<&Value>) -> Usage {
    let Some(u) = u else { return Usage::default() };
    Usage {
        input_tokens: u.get("prompt_tokens").and_then(|n| n.as_u64()).unwrap_or(0),
        output_tokens: u.get("completion_tokens").and_then(|n| n.as_u64()).unwrap_or(0),
        cached_input_tokens: u
            .path("prompt_tokens_details", &["cached_tokens"])
            .and_then(|n| n.as_u64())
            .or_else(|| u.get("prompt_cache_hit_tokens").and_then(|n| n.as_u64()))
            .unwrap_or(0),
    }
}

fn openai_stop(s: &str) -> StopReason {
    match s {
        "stop" => StopReason::EndTurn,
        "tool_calls" | "function_call" => StopReason::ToolUse,
        "length" => StopReason::MaxTokens,
        other => StopReason::Other(other.to_string()),
    }
}

#[derive(Default)]
struct OpenAiDecoder {
    text: String,
    /// (id, name, argument text) by tool-call index.
    calls: Vec<(String, String, String)>,
    usage: Usage,
    stop: Option<StopReason>,
    model: String,
}

impl StreamDecoder for OpenAiDecoder {
    fn on_message(&mut self, msg: &SseMessage) -> Result<Vec<Delta>, String> {
        if msg.data.trim() == "[DONE]" {
            return Ok(Vec::new());
        }
        let v = json::parse(&msg.data)?;
        if let Some(err) = v.get("error") {
            return Err(format!("provider error: {}", err.str_of("message").unwrap_or("unknown")));
        }
        if let Some(m) = v.str_of("model") {
            self.model = m.to_string();
        }
        if v.get("usage").map(|u| !matches!(u, Value::Null)).unwrap_or(false) {
            self.usage = openai_usage(v.get("usage"));
        }
        let mut out = Vec::new();
        if let Some(choice) = v.path("choices", &["0"]) {
            if let Some(delta) = choice.get("delta") {
                if let Some(t) = delta.str_of("content") {
                    if !t.is_empty() {
                        self.text.push_str(t);
                        out.push(Delta::Text(t.to_string()));
                    }
                }
                if let Some(calls) = delta.get("tool_calls").and_then(|c| c.as_arr()) {
                    for c in calls {
                        let idx = c.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                        while self.calls.len() <= idx {
                            self.calls.push((String::new(), String::new(), String::new()));
                        }
                        let slot = &mut self.calls[idx];
                        if let Some(id) = c.str_of("id") {
                            slot.0 = id.to_string();
                        }
                        if let Some(name) = c.path("function", &["name"]).and_then(|n| n.as_str()) {
                            if slot.1.is_empty() {
                                out.push(Delta::ToolStarted(name.to_string()));
                            }
                            slot.1.push_str(name);
                        }
                        if let Some(args) = c.path("function", &["arguments"]).and_then(|a| a.as_str()) {
                            slot.2.push_str(args);
                        }
                    }
                }
            }
            if let Some(f) = choice.str_of("finish_reason") {
                self.stop = Some(openai_stop(f));
            }
        }
        Ok(out)
    }

    fn finish(self: Box<Self>) -> Result<CompletionResponse, String> {
        let mut blocks = Vec::new();
        if !self.text.is_empty() {
            blocks.push(Block::Text(self.text));
        }
        for (i, (id, name, args)) in self.calls.into_iter().enumerate() {
            if name.is_empty() {
                continue;
            }
            blocks.push(Block::ToolUse { id: if id.is_empty() { format!("call_{i}") } else { id }, name, input: tool_input_value(&args) });
        }
        let stop = self.stop.ok_or("the stream ended before the model finished (no finish_reason)")?;
        Ok(CompletionResponse { blocks, usage: self.usage, stop, model: self.model })
    }
}

// ── Anthropic ────────────────────────────────────────────────────

/// Anthropic Messages API (`/v1/messages`).
#[derive(Debug, Clone)]
pub struct Anthropic {
    pub base_url: String,
}

impl Default for Anthropic {
    fn default() -> Self {
        Anthropic { base_url: "https://api.anthropic.com".into() }
    }
}

impl Provider for Anthropic {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn build_request(&self, req: &CompletionRequest, api_key: Option<&str>) -> Result<Request, String> {
        let messages: Vec<Value> = req
            .messages
            .iter()
            .map(|m| {
                let content: Vec<Value> = m
                    .blocks
                    .iter()
                    .map(|b| match b {
                        Block::Text(t) => Value::obj(vec![("type", Value::str("text")), ("text", Value::str(t.clone()))]),
                        Block::ToolUse { id, name, input } => Value::obj(vec![
                            ("type", Value::str("tool_use")),
                            ("id", Value::str(id.clone())),
                            ("name", Value::str(name.clone())),
                            ("input", input.clone()),
                        ]),
                        Block::ToolResult { id, content, is_error } => Value::obj(vec![
                            ("type", Value::str("tool_result")),
                            ("tool_use_id", Value::str(id.clone())),
                            ("content", Value::str(content.clone())),
                            ("is_error", Value::Bool(*is_error)),
                        ]),
                    })
                    .collect();
                Value::obj(vec![
                    ("role", Value::str(if m.role == Role::User { "user" } else { "assistant" })),
                    ("content", Value::Arr(content)),
                ])
            })
            .collect();
        let mut body = vec![("model", Value::str(req.model.clone())), ("max_tokens", Value::int(req.max_tokens as i64))];
        if !req.system.is_empty() {
            // The system prompt is stable across every turn of an agent,
            // so mark it cacheable: later turns pay the cached rate.
            body.push((
                "system",
                Value::Arr(vec![Value::obj(vec![
                    ("type", Value::str("text")),
                    ("text", Value::str(req.system.clone())),
                    ("cache_control", Value::obj(vec![("type", Value::str("ephemeral"))])),
                ])]),
            ));
        }
        body.push(("messages", Value::Arr(messages)));
        if !req.tools.is_empty() {
            let tools: Vec<Value> = req
                .tools
                .iter()
                .map(|t| {
                    Value::obj(vec![
                        ("name", Value::str(t.name.clone())),
                        ("description", Value::str(t.description.clone())),
                        ("input_schema", json::parse(&t.schema).unwrap_or(Value::Obj(vec![]))),
                    ])
                })
                .collect();
            body.push(("tools", Value::Arr(tools)));
        }
        if req.stream {
            body.push(("stream", Value::Bool(true)));
        }
        let url = parse_url(&format!("{}/v1/messages", self.base_url.trim_end_matches('/')))?;
        let mut r = Request::new("POST", url)
            .header("Content-Type", "application/json")
            .header("anthropic-version", "2023-06-01");
        if let Some(k) = api_key {
            r = r.header("x-api-key", k);
        }
        Ok(r.json_body(json::to_string(&Value::obj(body))))
    }

    fn parse_response(&self, body: &[u8]) -> Result<CompletionResponse, String> {
        let v = json::parse(&String::from_utf8_lossy(body))?;
        let mut blocks = Vec::new();
        for b in v.get("content").and_then(|c| c.as_arr()).ok_or("response has no content")? {
            match b.str_of("type") {
                Some("text") => blocks.push(Block::Text(b.str_of("text").unwrap_or("").to_string())),
                Some("tool_use") => blocks.push(Block::ToolUse {
                    id: b.str_of("id").unwrap_or("").to_string(),
                    name: b.str_of("name").unwrap_or("").to_string(),
                    input: b.get("input").cloned().unwrap_or(Value::Obj(vec![])),
                }),
                _ => {}
            }
        }
        let mut usage = Usage::default();
        anthropic_usage(v.get("usage"), &mut usage);
        Ok(CompletionResponse {
            blocks,
            usage,
            stop: anthropic_stop(v.str_of("stop_reason").unwrap_or("")),
            model: v.str_of("model").unwrap_or("").to_string(),
        })
    }

    fn decoder(&self) -> Box<dyn StreamDecoder> {
        Box::new(AnthropicDecoder::default())
    }
}

fn anthropic_usage(u: Option<&Value>, into: &mut Usage) {
    let Some(u) = u else { return };
    let n = |k: &str| u.get(k).and_then(|x| x.as_u64());
    if let Some(i) = n("input_tokens") {
        into.input_tokens = i + n("cache_creation_input_tokens").unwrap_or(0) + n("cache_read_input_tokens").unwrap_or(0);
        into.cached_input_tokens = n("cache_read_input_tokens").unwrap_or(0);
    }
    if let Some(o) = n("output_tokens") {
        into.output_tokens = o;
    }
}

fn anthropic_stop(s: &str) -> StopReason {
    match s {
        "end_turn" | "stop_sequence" => StopReason::EndTurn,
        "tool_use" => StopReason::ToolUse,
        "max_tokens" => StopReason::MaxTokens,
        other => StopReason::Other(other.to_string()),
    }
}

#[derive(Default)]
struct AnthropicDecoder {
    /// (kind, id, name, accumulated text or json) by content index.
    blocks: Vec<(String, String, String, String)>,
    usage: Usage,
    stop: Option<StopReason>,
    model: String,
}

impl StreamDecoder for AnthropicDecoder {
    fn on_message(&mut self, msg: &SseMessage) -> Result<Vec<Delta>, String> {
        let v = json::parse(&msg.data)?;
        let kind = v.str_of("type").or(msg.event.as_deref()).unwrap_or("");
        let mut out = Vec::new();
        match kind {
            "message_start" => {
                if let Some(m) = v.get("message") {
                    self.model = m.str_of("model").unwrap_or("").to_string();
                    anthropic_usage(m.get("usage"), &mut self.usage);
                }
            }
            "content_block_start" => {
                let idx = v.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                let cb = v.get("content_block").ok_or("content_block_start without a block")?;
                while self.blocks.len() <= idx {
                    self.blocks.push(Default::default());
                }
                let ty = cb.str_of("type").unwrap_or("").to_string();
                if ty == "tool_use" {
                    let name = cb.str_of("name").unwrap_or("").to_string();
                    out.push(Delta::ToolStarted(name.clone()));
                    self.blocks[idx] = (ty, cb.str_of("id").unwrap_or("").to_string(), name, String::new());
                } else {
                    self.blocks[idx] = (ty, String::new(), String::new(), cb.str_of("text").unwrap_or("").to_string());
                }
            }
            "content_block_delta" => {
                let idx = v.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                let d = v.get("delta").ok_or("delta event without a delta")?;
                if let Some(slot) = self.blocks.get_mut(idx) {
                    match d.str_of("type") {
                        Some("text_delta") => {
                            let t = d.str_of("text").unwrap_or("");
                            slot.3.push_str(t);
                            out.push(Delta::Text(t.to_string()));
                        }
                        Some("input_json_delta") => slot.3.push_str(d.str_of("partial_json").unwrap_or("")),
                        _ => {}
                    }
                }
            }
            "message_delta" => {
                if let Some(s) = v.path("delta", &["stop_reason"]).and_then(|s| s.as_str()) {
                    self.stop = Some(anthropic_stop(s));
                }
                anthropic_usage(v.get("usage"), &mut self.usage);
            }
            "error" => {
                return Err(format!(
                    "provider error: {}",
                    v.path("error", &["message"]).and_then(|m| m.as_str()).unwrap_or("unknown")
                ));
            }
            _ => {} // ping, content_block_stop, message_stop
        }
        Ok(out)
    }

    fn finish(self: Box<Self>) -> Result<CompletionResponse, String> {
        let blocks = self
            .blocks
            .into_iter()
            .filter_map(|(ty, id, name, text)| match ty.as_str() {
                "text" if !text.is_empty() => Some(Block::Text(text)),
                "tool_use" => Some(Block::ToolUse { id, name, input: tool_input_value(&text) }),
                _ => None,
            })
            .collect();
        let stop = self.stop.ok_or("the stream ended before the model finished (no stop_reason)")?;
        Ok(CompletionResponse { blocks, usage: self.usage, stop, model: self.model })
    }
}

// ── calling a model, for real ────────────────────────────────────

/// Why a call failed, and whether trying again could help.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallError {
    pub message: String,
    pub retryable: bool,
    /// Seconds the server asked us to wait, if it said.
    pub retry_after: Option<u64>,
}

fn error_message(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
    json::parse(&text)
        .ok()
        .and_then(|v| v.path("error", &["message"]).and_then(|m| m.as_str()).map(String::from))
        .unwrap_or_else(|| text.chars().take(300).collect())
}

/// One attempt: send, stream, decode. `on_delta` sees text as it arrives.
pub fn call_once(
    provider: &dyn Provider,
    req: &CompletionRequest,
    api_key: Option<&str>,
    net_cfg: &NetConfig,
    on_delta: &mut dyn FnMut(&Delta),
) -> Result<CompletionResponse, CallError> {
    let http_req = provider.build_request(req, api_key).map_err(|m| CallError { message: m, retryable: false, retry_after: None })?;
    let mut s = net::send_streaming(&http_req, net_cfg).map_err(|m| CallError { message: m, retryable: true, retry_after: None })?;
    if s.head.status != 200 {
        let mut body = Vec::new();
        let _ = s.body.by_ref().take(64 * 1024).read_to_end(&mut body);
        let status = s.head.status;
        return Err(CallError {
            message: format!("{} returned HTTP {status}: {}", provider.name(), error_message(&body)),
            retryable: status == 429 || status == 408 || status >= 500,
            retry_after: s.head.header("retry-after").and_then(|v| v.trim().parse().ok()),
        });
    }
    let is_sse = s.head.header("content-type").map(|c| c.contains("event-stream")).unwrap_or(false);
    if !is_sse {
        let mut body = Vec::new();
        s.body.read_to_end(&mut body).map_err(|e| CallError { message: e.to_string(), retryable: true, retry_after: None })?;
        let mut resp = provider.parse_response(&body).map_err(|m| CallError { message: m, retryable: false, retry_after: None })?;
        if resp.model.is_empty() {
            resp.model = req.model.clone();
        }
        return Ok(resp);
    }
    let mut decoder = provider.decoder();
    let mut parser = SseParser::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = s.body.read(&mut buf).map_err(|e| CallError { message: format!("stream interrupted: {e}"), retryable: true, retry_after: None })?;
        let msgs = if n == 0 { parser.finish() } else { parser.feed(&buf[..n]) };
        for m in &msgs {
            for d in decoder.on_message(m).map_err(|e| CallError { retryable: e.contains("overloaded"), message: e, retry_after: None })? {
                on_delta(&d);
            }
        }
        if n == 0 {
            break;
        }
    }
    let mut resp = decoder.finish().map_err(|m| CallError { message: m, retryable: true, retry_after: None })?;
    if resp.model.is_empty() {
        resp.model = req.model.clone();
    }
    Ok(resp)
}

/// A model endpoint ready to call: provider + resolved key.
pub struct Endpoint<'a> {
    pub model: String,
    pub provider: &'a dyn Provider,
    pub api_key: Option<String>,
}

/// Call with retries and fallback. Up to `attempts` tries per endpoint
/// with exponential backoff (1s, 2s, 4s, capped by `retry-after`), then
/// the next endpoint (the tier's fallback model).
pub fn call(
    endpoints: &[Endpoint<'_>],
    base: &CompletionRequest,
    net_cfg: &NetConfig,
    attempts: u32,
    on_delta: &mut dyn FnMut(&Delta),
    on_retry: &mut dyn FnMut(&str),
) -> Result<CompletionResponse, String> {
    let mut last = String::from("no model endpoints configured");
    for ep in endpoints {
        let mut req = base.clone();
        req.model = ep.model.clone();
        for attempt in 0..attempts.max(1) {
            match call_once(ep.provider, &req, ep.api_key.as_deref(), net_cfg, on_delta) {
                Ok(r) => return Ok(r),
                Err(e) => {
                    last = e.message.clone();
                    if !e.retryable {
                        break;
                    }
                    if attempt + 1 < attempts {
                        let wait = e.retry_after.unwrap_or(1 << attempt).min(30);
                        on_retry(&format!("{} (retrying {} in {wait}s)", e.message, ep.model));
                        std::thread::sleep(Duration::from_secs(wait));
                    }
                }
            }
        }
        on_retry(&format!("giving up on {}: {last}", ep.model));
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_req() -> CompletionRequest {
        let mut r = CompletionRequest::new("m");
        r.system = "You are terse.".into();
        r.tools.push(ToolDef { name: "files-read".into(), description: "read".into(), schema: r#"{"type":"object","properties":{"path":{"type":"string"}}}"#.into() });
        r.messages.push(Message::user_text("read a.txt"));
        r.messages.push(Message {
            role: Role::Assistant,
            blocks: vec![Block::ToolUse { id: "t1".into(), name: "files-read".into(), input: json::parse(r#"{"path":"a.txt"}"#).unwrap() }],
        });
        r.messages.push(Message { role: Role::User, blocks: vec![Block::ToolResult { id: "t1".into(), content: "hello".into(), is_error: false }] });
        r
    }

    #[test]
    fn openai_request_carries_tools_and_results() {
        let p = OpenAiCompatible { base_url: "https://api.deepseek.com/v1".into() };
        let http = p.build_request(&tool_req(), Some("sk-test")).unwrap();
        assert_eq!(http.url.path, "/v1/chat/completions");
        let body = String::from_utf8(http.body.clone()).unwrap();
        assert!(body.contains(r#""role":"system","content":"You are terse.""#));
        assert!(body.contains(r#""tool_calls":[{"id":"t1","type":"function","function":{"name":"files-read","arguments":"{\"path\":\"a.txt\"}"}}]"#), "{body}");
        assert!(body.contains(r#""role":"tool","tool_call_id":"t1","content":"hello""#));
        assert!(body.contains(r#""parameters":{"type":"object""#));
        // The key goes in exactly one header and nowhere else.
        assert!(http.encode_head().contains("Authorization: Bearer sk-test"));
        assert!(!body.contains("sk-test"));
    }

    #[test]
    fn anthropic_request_shape() {
        let http = Anthropic::default().build_request(&tool_req(), Some("key")).unwrap();
        assert_eq!(http.url.path, "/v1/messages");
        let body = String::from_utf8(http.body.clone()).unwrap();
        assert!(body.contains(r#""cache_control":{"type":"ephemeral"}"#));
        assert!(body.contains(r#""type":"tool_use","id":"t1","name":"files-read","input":{"path":"a.txt"}"#));
        assert!(body.contains(r#""type":"tool_result","tool_use_id":"t1","content":"hello","is_error":false"#));
        assert!(body.contains(r#""input_schema":{"type":"object""#));
        assert!(http.encode_head().contains("x-api-key: key"));
    }

    fn feed(dec: &mut Box<dyn StreamDecoder>, raw: &str) -> Vec<Delta> {
        let mut p = SseParser::new();
        let mut out = Vec::new();
        for m in p.feed(raw.as_bytes()) {
            out.extend(dec.on_message(&m).unwrap());
        }
        out
    }

    #[test]
    fn openai_stream_assembles_text_and_tool_calls() {
        let raw = concat!(
            "data: {\"model\":\"deepseek-v4-flash\",\"choices\":[{\"delta\":{\"content\":\"Let me \"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"read.\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"files-read\",\"arguments\":\"{\\\"pa\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"th\\\":\\\"a.txt\\\"}\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":120,\"completion_tokens\":30}}\n\n",
            "data: [DONE]\n\n"
        );
        let mut dec = OpenAiCompatible { base_url: String::new() }.decoder();
        let deltas = feed(&mut dec, raw);
        assert_eq!(deltas[0], Delta::Text("Let me ".into()));
        assert!(deltas.contains(&Delta::ToolStarted("files-read".into())));
        let resp = dec.finish().unwrap();
        assert_eq!(resp.text(), "Let me read.");
        assert_eq!(resp.stop, StopReason::ToolUse);
        let uses = resp.tool_uses();
        assert_eq!(uses[0].1, "files-read");
        assert_eq!(uses[0].2.str_of("path"), Some("a.txt"));
        assert_eq!(resp.usage.input_tokens, 120);
        assert_eq!(resp.model, "deepseek-v4-flash");
    }

    #[test]
    fn anthropic_stream_assembles_text_and_tool_use() {
        let raw = concat!(
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"model\":\"claude-x\",\"usage\":{\"input_tokens\":50,\"cache_read_input_tokens\":40,\"output_tokens\":1}}}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Reading.\"}}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tu1\",\"name\":\"files-read\",\"input\":{}}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"path\\\": \\\"a\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\".txt\\\"}\"}}\n\n",
            "event: ping\ndata: {\"type\":\"ping\"}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":25}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
        );
        let mut dec = Anthropic::default().decoder();
        feed(&mut dec, raw);
        let resp = dec.finish().unwrap();
        assert_eq!(resp.text(), "Reading.");
        assert_eq!(resp.stop, StopReason::ToolUse);
        assert_eq!(resp.tool_uses()[0].2.str_of("path"), Some("a.txt"));
        assert_eq!(resp.usage, Usage { input_tokens: 90, output_tokens: 25, cached_input_tokens: 40 });
    }

    #[test]
    fn truncated_stream_is_an_error_not_a_silent_success() {
        let mut dec = Anthropic::default().decoder();
        feed(&mut dec, "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":1}}}\n\n");
        assert!(dec.finish().is_err());
    }

    #[test]
    fn non_streaming_bodies_parse() {
        let body = br#"{"model":"m","choices":[{"message":{"content":"pong","tool_calls":[{"id":"x","function":{"name":"edit","arguments":"{}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":12,"completion_tokens":3}}"#;
        let r = OpenAiCompatible { base_url: String::new() }.parse_response(body).unwrap();
        assert_eq!(r.text(), "pong");
        assert_eq!(r.tool_uses().len(), 1);
        let body = br#"{"content":[{"type":"text","text":"pong"}],"usage":{"input_tokens":12,"output_tokens":3},"stop_reason":"end_turn"}"#;
        let r = Anthropic::default().parse_response(body).unwrap();
        assert_eq!((r.text().as_str(), r.stop.clone()), ("pong", StopReason::EndTurn));
    }

    #[test]
    fn provider_error_messages_are_extracted() {
        assert_eq!(error_message(br#"{"error":{"type":"overloaded","message":"Overloaded"}}"#), "Overloaded");
        assert_eq!(error_message(b"plain text"), "plain text");
    }
}
