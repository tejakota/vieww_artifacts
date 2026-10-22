//! Wright's own configuration: which model providers exist, where they
//! live, how to authenticate, and what tokens cost.
//!
//! Blueprints name model IDs through tiers ("cheap" -> "deepseek-v4-flash");
//! this file maps a model ID to an endpoint. It lives OUTSIDE projects
//! (`~/.wright/config.kdl`, or `$WRIGHT_HOME/config.kdl`) because the
//! same machine serves many projects, and because keys are machine
//! business, not project business. It uses the same KDL dialect as the
//! blueprint, parsed by the same parser:
//!
//! ```kdl
//! provider "anthropic" kind="anthropic" base="https://api.anthropic.com" key="keychain:wright/anthropic" models="claude-*"
//! price model="claude-opus-*" in=15 out=75 cached=1.5
//! interview model="claude-sonnet-5-5"
//! voice model="whisper-base"
//! builtin fallback=true
//! worker "claude-code" command="claude" args="-p" timeout=900
//! mcp "weather" command="uvx" args="mcp-server-weather,--local" hash="blake3:..." timeout=60
//! tls ca-file="/etc/ssl/certs/corp-root.pem"
//! env "HTTPS_PROXY" value="http://proxy.corp.example:3128"
//! env "PIP_INDEX_URL" value="https://pypi.corp.example/simple"
//! trust "Wright gallery" key="<64 hex chars: the publisher's Ed25519 public key>"
//! ```
//!
//! **Office machines.** `tls ca-file` names a PEM file of extra
//! certificate authorities (a corporate root that re-signs HTTPS); it is
//! trusted IN ADDITION to the system store, never instead of it, and
//! `WRIGHT_CA_FILE` overrides it for one process. `env` lines set
//! environment variables for Wright and everything it starts (proxies,
//! package mirrors, the CA variables other tools read). A variable whose
//! name says it is a secret (`*KEY*`, `*TOKEN*`, `*SECRET*`,
//! `*PASSWORD*`, `*AUTH*`, `*CREDENTIAL*`) must be a `keychain:`
//! reference, like every other secret here. [`apply_to_process`] puts
//! them in place at startup.
//!
//! **Trust.** `trust` lines pin the Ed25519 keys whose signatures this
//! machine accepts: recipe publishers (ADR-028) and receipt signers
//! (ADR-025). A signature by a key nobody pinned proves only that the
//! bytes were not changed after signing, not WHO signed them, so
//! `wright recipe install` and `wright verify` refuse it.
//!
//! Workers (ADR-026) are the external coding agents a blueprint can
//! hand a step to: `claude-code`, `codex`, `kimi-code`, `zcode` are
//! known by default; the command line each runs is configured here,
//! per machine. MCP servers (ADR-029) are pinned by the BLAKE3 of the
//! executable they run — an unpinned server is refused at call time.
//!
//! The built-in models (`qwen3-1.7b`, `whisper-base`; see `local.rs`) are
//! a provider of kind `builtin`, last in the list. With `builtin
//! fallback=true` (the default) a model with no provider or no key falls
//! back to them instead of failing.
//!
//! Model patterns are comma-separated globs where `*` matches anything.
//! The first provider whose pattern matches wins. Prices are USD per
//! million tokens; unknown models are priced conservatively (see
//! [`Config::price_of`]) so budgets fail safe, never open.
//!
//! Without a config file, [`Config::builtin`] knows the public endpoints
//! of the major providers. Keys are always `keychain:` references.

use std::path::PathBuf;

use wright_core::blueprint::parse::{parse_doc, KdlValue};

use crate::models::{Anthropic, Builtin, OpenAiCompatible, Provider};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderKind {
    Anthropic,
    OpenAi,
    /// Wright's own built-in models, run in-process.
    Builtin,
}

#[derive(Debug, Clone)]
pub struct ProviderEntry {
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
    /// `keychain:` reference, or None for keyless endpoints (local, mock).
    pub key: Option<String>,
    pub models: Vec<String>,
}

impl ProviderEntry {
    pub fn matches(&self, model: &str) -> bool {
        self.models.iter().any(|p| glob(p, model))
    }

    pub fn client(&self) -> Box<dyn Provider> {
        match self.kind {
            ProviderKind::Anthropic => Box::new(Anthropic {
                base_url: self.base_url.clone(),
            }),
            ProviderKind::OpenAi => Box::new(OpenAiCompatible {
                base_url: self.base_url.clone(),
            }),
            ProviderKind::Builtin => Box::new(Builtin),
        }
    }
}

/// USD per million tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Price {
    pub input: f64,
    pub output: f64,
    pub cached_input: f64,
}

/// One external coding agent a blueprint can hand a step to (ADR-026).
/// The defaults carry the four the memo names; the machine's config can
/// override the command line (where the binary lives, what flags it
/// takes) without touching a blueprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerEntry {
    /// Blueprint-facing id: `worker "claude-code"`.
    pub name: String,
    /// The executable to run, resolved through PATH.
    pub command: String,
    /// Static args, comma-separated in config (`args="-p,--json"`),
    /// passed before the prompt.
    pub args: Vec<String>,
    /// Wall-clock seconds before the run is killed (default 900).
    pub timeout_secs: u64,
    /// Environment the worker gets on top of the scrubbed base — what it
    /// needs to sign in to its own model (`env="ANTHROPIC_API_KEY=keychain:wright/anthropic"`).
    /// Values are `keychain:` references or plain non-secret text; they
    /// are resolved only at the moment the worker starts.
    pub env: Vec<(String, String)>,
}

/// One pinned signing key (`trust "name" key="hex"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedKey {
    /// Who it belongs to, as the person wrote it.
    pub name: String,
    /// 64 lower-case hex characters: an Ed25519 public key.
    pub key: String,
}

/// Names that say "this value is a secret": they must be keychain refs.
pub fn looks_secret(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    ["KEY", "TOKEN", "SECRET", "PASSWORD", "PASSWD", "AUTH", "CREDENTIAL"]
        .iter()
        .any(|w| n.contains(w))
}

/// A proxy URL with a password in it (`http://user:pass@host`) is a
/// secret too, whatever its variable is called.
fn carries_password(value: &str) -> bool {
    let rest = value.split_once("://").map(|(_, r)| r).unwrap_or(value);
    let authority = rest.split('/').next().unwrap_or("");
    authority.contains('@') && authority.split('@').next().is_some_and(|u| u.contains(':'))
}

/// Environment variable names: letters, digits, underscores, not
/// starting with a digit (the portable POSIX set).
pub fn valid_env_name(name: &str) -> bool {
    !name.is_empty()
        && !name.as_bytes()[0].is_ascii_digit()
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Check one `NAME=value` pair against the secrets rule. `Err` is a
/// sentence for the person.
pub fn check_env_pair(name: &str, value: &str) -> Result<(), String> {
    if !valid_env_name(name) {
        return Err(format!("'{name}' is not a valid environment variable name (letters, digits and _ only)"));
    }
    if value.starts_with("keychain:") {
        return Ok(());
    }
    if looks_secret(name) || carries_password(value) {
        return Err(format!(
            "{name} holds a secret, so it must be a keychain: reference (store it with `wright secret set keychain:wright/{}`), never the value itself",
            name.to_ascii_lowercase()
        ));
    }
    Ok(())
}

/// One hash-pinned MCP server (ADR-029).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpEntry {
    /// Blueprint-facing id: `mcp server="weather"`.
    pub name: String,
    /// The executable that IS the server.
    pub command: String,
    /// Static args, comma-separated in config.
    pub args: Vec<String>,
    /// The pin: `blake3:<hex>` of the executable file. A server whose
    /// binary no longer hashes to this is refused, loudly.
    pub hash: String,
    /// Wall-clock seconds per tool call (default 60).
    pub timeout_secs: u64,
}

impl Price {
    /// Cost in micro-USD. (USD per million tokens == micro-USD per token.)
    pub fn cost_micro(&self, input: u64, cached: u64, output: u64) -> u64 {
        let fresh = input.saturating_sub(cached);
        (fresh as f64 * self.input
            + cached as f64 * self.cached_input
            + output as f64 * self.output)
            .ceil() as u64
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub providers: Vec<ProviderEntry>,
    pub prices: Vec<(String, Price)>,
    /// The model the Interviewer uses (`interview model="..."`).
    pub interview_model: String,
    /// The speech-to-text model voice input uses (`voice model="..."`).
    /// Served by whichever OpenAI-compatible provider matches it, at
    /// `<base>/audio/transcriptions`, or by the built-in Whisper.
    pub voice_model: String,
    /// External coding agents blueprints can hand steps to (ADR-026).
    pub workers: Vec<WorkerEntry>,
    /// Hash-pinned MCP servers (ADR-029).
    pub mcp_servers: Vec<McpEntry>,
    /// Fall back to the built-in models when a model has no provider or
    /// no key (`builtin fallback=false` turns it off). `None`: not set in
    /// this file (the default, on, applies).
    pub builtin_fallback: Option<bool>,
    /// Extra certificate authorities, a PEM file (`tls ca-file="..."`).
    pub ca_file: Option<String>,
    /// Environment variables for Wright and what it starts (`env "NAME" value="..."`).
    pub env: Vec<(String, String)>,
    /// Signing keys this machine accepts (`trust "name" key="hex"`).
    pub trusted: Vec<TrustedKey>,
    pub source: String,
}

/// `*`-only glob match.
pub fn glob(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let mut rest = text;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            match rest.strip_prefix(part) {
                Some(r) => rest = r,
                None => return false,
            }
        } else if i == parts.len() - 1 {
            return rest.ends_with(part);
        } else {
            match rest.find(part) {
                Some(at) => rest = &rest[at + part.len()..],
                None => return false,
            }
        }
    }
    true
}

// ── the process environment (office machines) ──────────────────────

/// Names of the variables Wright itself set from `env` lines, so a
/// later change in Settings can replace them — while a variable the
/// person exported in their own shell is never overwritten.
static SET_BY_WRIGHT: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// The extra-CA variable every TLS path reads (`runtime/src/tls.rs`,
/// `runtime/src/curl.rs`). Set it yourself to override `tls ca-file`.
pub const CA_ENV: &str = "WRIGHT_CA_FILE";

/// The extra certificate authorities for this process, if any.
pub fn ca_file() -> Option<PathBuf> {
    std::env::var_os(CA_ENV)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Check a CA file before anyone relies on it: it must exist and hold
/// at least one PEM certificate. Ok(number of certificates).
pub fn check_ca_file(path: &std::path::Path) -> Result<usize, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes);
    let n = text.matches("-----BEGIN CERTIFICATE-----").count();
    if n == 0 {
        return Err(format!(
            "{} holds no PEM certificate (a file of '-----BEGIN CERTIFICATE-----' blocks). A .cer/.der file converts with: openssl x509 -inform der -in corp.cer -out corp.pem",
            path.display()
        ));
    }
    Ok(n)
}

/// Put the config's `env` lines and `tls ca-file` into this process's
/// environment, so Wright and every program it starts see them. Called
/// first thing in `main`, before any thread exists, and again when
/// Settings saves. A variable the person already exported wins over the
/// config (the shell is the more specific place). Returns one sentence
/// per entry that could not be applied (a keychain entry that is
/// missing, a CA file that is not there).
pub fn apply_to_process(cfg: &Config) -> Vec<String> {
    let mut problems = Vec::new();
    let mut ours = SET_BY_WRIGHT.lock().unwrap_or_else(|p| p.into_inner());
    // Forget values Wright set earlier that the config no longer has.
    let wanted: Vec<&str> = cfg
        .env
        .iter()
        .map(|(k, _)| k.as_str())
        .chain(cfg.ca_file.iter().map(|_| CA_ENV))
        .collect();
    ours.retain(|k| {
        if wanted.contains(&k.as_str()) {
            true
        } else {
            std::env::remove_var(k);
            false
        }
    });
    let set = |k: &str, v: &str, ours: &mut Vec<String>| {
        let mine = ours.iter().any(|x| x == k);
        if std::env::var_os(k).is_some() && !mine {
            return; // the person's own environment wins
        }
        std::env::set_var(k, v);
        if !mine {
            ours.push(k.to_string());
        }
    };
    for (k, v) in &cfg.env {
        if v.starts_with("keychain:") {
            match crate::secrets::resolve(v) {
                Ok(secret) => set(k, secret.expose(), &mut ours),
                Err(e) => problems.push(format!("{k} was not set: {e}")),
            }
        } else {
            set(k, v, &mut ours);
        }
    }
    if let Some(f) = &cfg.ca_file {
        let path = expand_home(f);
        match check_ca_file(&path) {
            Ok(_) => set(CA_ENV, &path.to_string_lossy(), &mut ours),
            Err(e) => problems.push(format!("the extra certificate authorities were not added: {e}")),
        }
    }
    problems
}

/// `~/x` -> the home directory's `x`; anything else unchanged.
pub fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        if let Ok(h) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
            return PathBuf::from(h).join(rest);
        }
    }
    PathBuf::from(path)
}

/// Where Wright keeps machine-wide state.
pub fn wright_home() -> PathBuf {
    if let Ok(h) = std::env::var("WRIGHT_HOME") {
        return PathBuf::from(h);
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".wright")
}

impl Config {
    /// Public endpoints of the major providers. Prices are list prices
    /// at the time of writing — override them in config.kdl. The four
    /// workers the memo names are preconfigured; a machine that has one
    /// of them installed needs no config at all.
    pub fn builtin() -> Config {
        let p = |name: &str, kind: ProviderKind, base: &str, models: &str| ProviderEntry {
            name: name.into(),
            kind,
            base_url: base.into(),
            key: Some(format!("keychain:wright/{name}")),
            models: models.split(',').map(String::from).collect(),
        };
        let price = |m: &str, i: f64, o: f64, c: f64| {
            (
                m.to_string(),
                Price {
                    input: i,
                    output: o,
                    cached_input: c,
                },
            )
        };
        let worker = |name: &str, command: &str, args: &str| WorkerEntry {
            name: name.into(),
            command: command.into(),
            args: if args.is_empty() {
                Vec::new()
            } else {
                args.split(',').map(|s| s.to_string()).collect()
            },
            timeout_secs: 900,
            env: Vec::new(),
        };
        Config {
            providers: vec![
                p(
                    "anthropic",
                    ProviderKind::Anthropic,
                    "https://api.anthropic.com",
                    "claude-*",
                ),
                p(
                    "openai",
                    ProviderKind::OpenAi,
                    "https://api.openai.com/v1",
                    "gpt-*,o1*,o3*,o4*",
                ),
                p(
                    "deepseek",
                    ProviderKind::OpenAi,
                    "https://api.deepseek.com/v1",
                    "deepseek-*",
                ),
                p(
                    "zai",
                    ProviderKind::OpenAi,
                    "https://api.z.ai/api/paas/v4",
                    "glm-*",
                ),
                p(
                    "moonshot",
                    ProviderKind::OpenAi,
                    "https://api.moonshot.ai/v1",
                    "kimi-*,moonshot-*",
                ),
                p("xai", ProviderKind::OpenAi, "https://api.x.ai/v1", "grok-*"),
                ProviderEntry {
                    name: "built-in".into(),
                    kind: ProviderKind::Builtin,
                    base_url: String::new(),
                    key: None,
                    models: crate::local::CATALOG
                        .iter()
                        .map(|m| m.id.to_string())
                        .collect(),
                },
            ],
            prices: vec![
                price("claude-opus-*", 15.0, 75.0, 1.5),
                price("claude-sonnet-*", 3.0, 15.0, 0.3),
                price("claude-haiku-*", 1.0, 5.0, 0.1),
                price("deepseek-*", 0.3, 1.2, 0.03),
                price("glm-*", 0.6, 2.2, 0.11),
                price("kimi-*", 0.6, 2.5, 0.15),
                price("mock-*", 0.0, 0.0, 0.0),
                price(crate::local::TEXT_MODEL, 0.0, 0.0, 0.0),
                price(crate::local::SPEECH_MODEL, 0.0, 0.0, 0.0),
            ],
            interview_model: "claude-sonnet-5-5".into(),
            voice_model: crate::local::SPEECH_MODEL.into(),
            workers: vec![
                worker("claude-code", "claude", "--print"),
                worker("codex", "codex", "exec"),
                worker("kimi-code", "kimi", "code"),
                worker("zcode", "zcode", "--headless"),
            ],
            mcp_servers: Vec::new(),
            builtin_fallback: Some(true),
            ca_file: None,
            env: Vec::new(),
            trusted: Vec::new(),
            source: "built-in defaults".into(),
        }
    }

    /// Load `<wright home>/config.kdl` if present, else the built-ins.
    pub fn load() -> Result<Config, String> {
        let path = wright_home().join("config.kdl");
        match std::fs::read_to_string(&path) {
            Ok(text) => Config::parse(&text, &path.display().to_string()),
            Err(_) => Ok(Config::builtin()),
        }
    }

    /// Parse config text. Entries in the file come FIRST (they win),
    /// the built-ins stay behind them as defaults.
    pub fn parse(text: &str, source: &str) -> Result<Config, String> {
        let mut cfg = Config::parse_file_only(text, source)?;
        let b = Config::builtin();
        cfg.providers.extend(b.providers);
        cfg.prices.extend(b.prices);
        if cfg.interview_model.is_empty() {
            cfg.interview_model = b.interview_model;
        }
        if cfg.voice_model.is_empty() {
            cfg.voice_model = b.voice_model;
        }
        if cfg.builtin_fallback.is_none() {
            cfg.builtin_fallback = b.builtin_fallback;
        }
        // The preconfigured workers stay available behind the file's own
        // (a file that configures one worker, or none, must not make the
        // other three disappear — it used to, the moment Settings saved
        // anything at all).
        for w in b.workers {
            if !cfg.workers.iter().any(|x| x.name == w.name) {
                cfg.workers.push(w);
            }
        }
        Ok(cfg)
    }

    /// Exactly what the text says, without the built-in defaults behind
    /// it. This is what Settings edits and writes back: saving the merged
    /// config would copy every built-in into the user's file.
    pub fn parse_file_only(text: &str, source: &str) -> Result<Config, String> {
        let doc = parse_doc(text).map_err(|e| format!("{source}: {e}"))?;
        let mut cfg = Config {
            providers: Vec::new(),
            prices: Vec::new(),
            interview_model: String::new(),
            voice_model: String::new(),
            workers: Vec::new(),
            mcp_servers: Vec::new(),
            builtin_fallback: None,
            ca_file: None,
            env: Vec::new(),
            trusted: Vec::new(),
            source: source.into(),
        };
        for node in &doc {
            let s = |k: &str| node.prop_str(k).map(String::from);
            let num = |k: &str| -> Option<f64> {
                node.prop(k).and_then(|v| match v {
                    KdlValue::Int(i) => Some(*i as f64),
                    KdlValue::Str(t) => t.parse().ok(),
                    _ => None,
                })
            };
            match node.name.as_str() {
                "provider" => {
                    let name = node.arg_str().ok_or_else(|| format!("{source}: line {}: provider needs a name", node.line))?.to_string();
                    let kind = match s("kind").as_deref() {
                        Some("anthropic") => ProviderKind::Anthropic,
                        Some("openai") | None => ProviderKind::OpenAi,
                        Some("builtin") => ProviderKind::Builtin,
                        Some(other) => return Err(format!("{source}: line {}: unknown provider kind '{other}' (anthropic | openai | builtin)", node.line)),
                    };
                    let key = s("key");
                    if let Some(k) = &key {
                        if !k.starts_with("keychain:") {
                            return Err(format!("{source}: line {}: provider keys must be keychain: references, never values", node.line));
                        }
                    }
                    let base_url = match kind {
                        ProviderKind::Builtin => s("base").unwrap_or_default(),
                        _ => s("base").ok_or_else(|| format!("{source}: line {}: provider needs base=\"https://...\"", node.line))?,
                    };
                    cfg.providers.push(ProviderEntry {
                        name,
                        kind,
                        base_url,
                        key,
                        models: s("models").unwrap_or_else(|| "*".into()).split(',').map(|m| m.trim().to_string()).collect(),
                    });
                }
                "price" => {
                    let model = s("model").ok_or_else(|| format!("{source}: line {}: price needs model=\"...\"", node.line))?;
                    let input = num("in").unwrap_or(0.0);
                    let output = num("out").unwrap_or(0.0);
                    cfg.prices.push((model, Price { input, output, cached_input: num("cached").unwrap_or(input / 10.0) }));
                }
                "interview" => {
                    cfg.interview_model = s("model").ok_or_else(|| format!("{source}: line {}: interview needs model=\"...\"", node.line))?;
                }
                "voice" => {
                    cfg.voice_model = s("model").ok_or_else(|| format!("{source}: line {}: voice needs model=\"...\"", node.line))?;
                }
                "builtin" => {
                    cfg.builtin_fallback = Some(match node.prop("fallback") {
                        Some(KdlValue::Bool(b)) => *b,
                        Some(KdlValue::Str(t)) if t == "true" || t == "false" => t == "true",
                        _ => return Err(format!("{source}: line {}: builtin needs fallback=true or fallback=false", node.line)),
                    });
                }
                "worker" => {
                    let name = node.arg_str().ok_or_else(|| format!("{source}: line {}: worker needs a name (worker \"claude-code\" command=...)", node.line))?.to_string();
                    let command = s("command").ok_or_else(|| format!("{source}: line {}: worker '{name}' needs command=\"...\"", node.line))?;
                    let args = s("args").unwrap_or_default().split(',').map(|a| a.trim().to_string()).filter(|a| !a.is_empty()).collect();
                    let timeout_secs = num("timeout").unwrap_or(900.0).max(1.0) as u64;
                    if cfg.workers.iter().any(|w: &WorkerEntry| w.name == name) {
                        return Err(format!("{source}: line {}: two workers are both named '{name}'", node.line));
                    }
                    let mut env = Vec::new();
                    for pair in s("env").unwrap_or_default().split(',').map(str::trim).filter(|p| !p.is_empty()) {
                        let (k, v) = pair.split_once('=').ok_or_else(|| format!("{source}: line {}: worker '{name}' env entries are NAME=value, comma separated", node.line))?;
                        let (k, v) = (k.trim().to_string(), v.trim().to_string());
                        check_env_pair(&k, &v).map_err(|e| format!("{source}: line {}: worker '{name}': {e}", node.line))?;
                        env.push((k, v));
                    }
                    cfg.workers.push(WorkerEntry { name, command, args, timeout_secs, env });
                }
                "mcp" => {
                    let name = node.arg_str().ok_or_else(|| format!("{source}: line {}: mcp needs a name (mcp \"weather\" command=... hash=...)", node.line))?.to_string();
                    let command = s("command").ok_or_else(|| format!("{source}: line {}: mcp '{name}' needs command=\"...\"", node.line))?;
                    let args = s("args").unwrap_or_default().split(',').map(|a| a.trim().to_string()).filter(|a| !a.is_empty()).collect();
                    let hash = s("hash").unwrap_or_default();
                    if !hash.starts_with("blake3:") || hash.len() != 7 + 64 {
                        return Err(format!(
                            "{source}: line {}: mcp '{name}' needs the executable pinned: hash=\"blake3:<64 hex chars>\" (run `wright mcp-hash <command>` to compute it). Unpinned servers are not run (ADR-029).",
                            node.line
                        ));
                    }
                    let timeout_secs = num("timeout").unwrap_or(60.0).max(1.0) as u64;
                    if cfg.mcp_servers.iter().any(|m: &McpEntry| m.name == name) {
                        return Err(format!("{source}: line {}: two MCP servers are both named '{name}'", node.line));
                    }
                    cfg.mcp_servers.push(McpEntry { name, command, args, hash, timeout_secs });
                }
                "tls" => {
                    let file = s("ca-file").map(|f| f.trim().to_string()).filter(|f| !f.is_empty()).ok_or_else(|| format!("{source}: line {}: tls needs ca-file=\"/path/to/certificates.pem\"", node.line))?;
                    cfg.ca_file = Some(file);
                }
                "env" => {
                    let name = node.arg_str().ok_or_else(|| format!("{source}: line {}: env needs a name (env \"HTTPS_PROXY\" value=...)", node.line))?.to_string();
                    let value = s("value").ok_or_else(|| format!("{source}: line {}: env '{name}' needs value=\"...\"", node.line))?;
                    check_env_pair(&name, &value).map_err(|e| format!("{source}: line {}: {e}", node.line))?;
                    cfg.env.retain(|(k, _)| k != &name);
                    cfg.env.push((name, value));
                }
                "trust" => {
                    let name = node.arg_str().ok_or_else(|| format!("{source}: line {}: trust needs a name (trust \"Wright gallery\" key=...)", node.line))?.to_string();
                    let key = s("key").unwrap_or_default().trim().to_ascii_lowercase();
                    if key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
                        return Err(format!("{source}: line {}: trust '{name}' needs key=\"<64 hex characters>\" (an Ed25519 public key)", node.line));
                    }
                    cfg.trusted.retain(|t| t.key != key);
                    cfg.trusted.push(TrustedKey { name, key });
                }
                other => return Err(format!("{source}: line {}: unknown setting '{other}' (provider | price | interview | voice | builtin | worker | mcp | tls | env | trust)", node.line)),
            }
        }
        Ok(cfg)
    }

    /// `config.kdl` as written (no built-ins); empty when there is no file.
    pub fn load_file() -> Result<Config, String> {
        let path = wright_home().join("config.kdl");
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        Config::parse_file_only(&text, &path.display().to_string())
    }

    /// The settings as config.kdl text, one line per entry. Parses back to
    /// the same settings (tested below).
    pub fn to_kdl(&self) -> String {
        let q = |s: &str| format!("\"{}\"", wright_core::blueprint::canon::escape_string(s));
        let mut out = String::from("// Wright settings. Settings in the app rewrites this file; a copy of the\n// previous version is kept as config.kdl.bak.\n");
        for p in &self.providers {
            let kind = match p.kind {
                ProviderKind::Anthropic => "anthropic",
                ProviderKind::OpenAi => "openai",
                ProviderKind::Builtin => "builtin",
            };
            out.push_str(&format!(
                "provider {} kind={} base={}",
                q(&p.name),
                q(kind),
                q(&p.base_url)
            ));
            if let Some(k) = &p.key {
                out.push_str(&format!(" key={}", q(k)));
            }
            out.push_str(&format!(" models={}\n", q(&p.models.join(","))));
        }
        for (m, pr) in &self.prices {
            out.push_str(&format!(
                "price model={} in={} out={} cached={}\n",
                q(m),
                q(&pr.input.to_string()),
                q(&pr.output.to_string()),
                q(&pr.cached_input.to_string())
            ));
        }
        if !self.interview_model.is_empty() {
            out.push_str(&format!("interview model={}\n", q(&self.interview_model)));
        }
        if !self.voice_model.is_empty() {
            out.push_str(&format!("voice model={}\n", q(&self.voice_model)));
        }
        for w in &self.workers {
            out.push_str(&format!(
                "worker {} command={} args={} timeout={}",
                q(&w.name),
                q(&w.command),
                q(&w.args.join(",")),
                q(&w.timeout_secs.to_string())
            ));
            if !w.env.is_empty() {
                let pairs: Vec<String> = w.env.iter().map(|(k, v)| format!("{k}={v}")).collect();
                out.push_str(&format!(" env={}", q(&pairs.join(","))));
            }
            out.push('\n');
        }
        for m in &self.mcp_servers {
            out.push_str(&format!(
                "mcp {} command={} args={} hash={} timeout={}\n",
                q(&m.name),
                q(&m.command),
                q(&m.args.join(",")),
                q(&m.hash),
                q(&m.timeout_secs.to_string())
            ));
        }
        if let Some(on) = self.builtin_fallback {
            out.push_str(&format!("builtin fallback={on}\n"));
        }
        if let Some(f) = &self.ca_file {
            out.push_str(&format!("tls ca-file={}\n", q(f)));
        }
        for (k, v) in &self.env {
            out.push_str(&format!("env {} value={}\n", q(k), q(v)));
        }
        for t in &self.trusted {
            out.push_str(&format!("trust {} key={}\n", q(&t.name), q(&t.key)));
        }
        out
    }

    /// The name a pinned key was trusted under, if it is pinned.
    pub fn trusts(&self, public_hex: &str) -> Option<&str> {
        let k = public_hex.to_ascii_lowercase();
        self.trusted.iter().find(|t| t.key == k).map(|t| t.name.as_str())
    }

    /// The configured worker a blueprint names, or None with a plain
    /// sentence for the plan-time error.
    pub fn worker_for(&self, name: &str) -> Option<&WorkerEntry> {
        self.workers.iter().find(|w| w.name == name)
    }

    /// A configured MCP server by name.
    pub fn mcp_for(&self, name: &str) -> Option<&McpEntry> {
        self.mcp_servers.iter().find(|m| m.name == name)
    }

    /// Write [`to_kdl`](Self::to_kdl) to `~/.wright/config.kdl`, keeping the
    /// previous file as `config.kdl.bak`. Refuses settings that would not
    /// load back.
    pub fn save_file(&self) -> Result<std::path::PathBuf, String> {
        let text = self.to_kdl();
        Config::parse_file_only(&text, "settings")?;
        let dir = wright_home();
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = dir.join("config.kdl");
        if path.exists() {
            let _ = std::fs::copy(&path, dir.join("config.kdl.bak"));
        }
        crate::store::write_atomic(&path, text.as_bytes())?;
        Ok(path)
    }

    pub fn provider_for(&self, model: &str) -> Option<&ProviderEntry> {
        self.providers.iter().find(|p| p.matches(model))
    }

    /// Whether to fall back to the built-in models (default: yes).
    pub fn falls_back_to_builtin(&self) -> bool {
        self.builtin_fallback.unwrap_or(true)
    }

    /// The built-in text model as a provider entry, for falling back to.
    pub fn builtin_text(&self) -> Option<&ProviderEntry> {
        self.providers
            .iter()
            .find(|p| p.kind == ProviderKind::Builtin && p.matches(crate::local::TEXT_MODEL))
    }

    /// Price of a model. Unknown models get a deliberately HIGH default
    /// ($15 in / $75 out) so a missing price can only stop a firing
    /// early, never let it overspend.
    pub fn price_of(&self, model: &str) -> Price {
        self.prices
            .iter()
            .find(|(p, _)| glob(p, model))
            .map(|(_, pr)| *pr)
            .unwrap_or(Price {
                input: 15.0,
                output: 75.0,
                cached_input: 15.0,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_written_by_the_app_read_back_the_same() {
        let text = "provider \"local\" kind=\"openai\" base=\"http://127.0.0.1:11434/v1\" models=\"llama*,qwen*\"\n\
price model=\"llama*\" in=0 out=0\ninterview model=\"qwen3\"\nvoice model=\"whisper-large\"\n";
        let a = Config::parse_file_only(text, "t").unwrap();
        let b = Config::parse_file_only(&a.to_kdl(), "t").unwrap();
        assert_eq!(b.providers.len(), 1);
        assert_eq!(b.providers[0].models, vec!["llama*", "qwen*"]);
        assert_eq!(b.prices.len(), 1);
        assert_eq!(
            (b.interview_model.as_str(), b.voice_model.as_str()),
            ("qwen3", "whisper-large")
        );
        assert!(
            Config::parse_file_only("", "t")
                .unwrap()
                .providers
                .is_empty(),
            "no file means no entries, not the built-ins"
        );
        let off = Config::parse_file_only("builtin fallback=false\n", "t").unwrap();
        assert_eq!(
            Config::parse_file_only(&off.to_kdl(), "t")
                .unwrap()
                .builtin_fallback,
            Some(false)
        );
    }

    #[test]
    fn the_builtin_models_are_a_last_resort_provider() {
        let cfg = Config::builtin();
        assert_eq!(
            cfg.provider_for("qwen3-1.7b").unwrap().kind,
            ProviderKind::Builtin
        );
        assert_eq!(
            cfg.provider_for("whisper-base").unwrap().kind,
            ProviderKind::Builtin
        );
        assert_eq!(cfg.voice_model, "whisper-base", "voice is local by default");
        assert_eq!(
            cfg.price_of("qwen3-1.7b")
                .cost_micro(1_000_000, 0, 1_000_000),
            0
        );
        assert!(cfg.falls_back_to_builtin());
        // A user's own catch-all provider still wins over the built-ins.
        let mine = Config::parse(
            "provider \"gw\" base=\"http://127.0.0.1:1/v1\" models=\"*\"\n",
            "t",
        )
        .unwrap();
        assert_eq!(mine.provider_for("qwen3-1.7b").unwrap().name, "gw");
        assert!(Config::parse("builtin fallback=false\n", "t")
            .map(|c| !c.falls_back_to_builtin())
            .unwrap());
    }

    #[test]
    fn globs() {
        assert!(glob("claude-*", "claude-opus-5-5"));
        assert!(glob("*", "anything"));
        assert!(glob("a*c*e", "abcde"));
        assert!(!glob("gpt-*", "claude-x"));
        assert!(glob("exact", "exact") && !glob("exact", "exactly"));
    }

    #[test]
    fn file_entries_win_over_builtins() {
        let cfg = Config::parse(
            "provider \"local\" kind=\"openai\" base=\"http://127.0.0.1:7878/v1\" models=\"deepseek-*,mock-*\"\nprice model=\"deepseek-*\" in=1 out=2\n",
            "test",
        )
        .unwrap();
        assert_eq!(cfg.provider_for("deepseek-v4-flash").unwrap().name, "local");
        assert_eq!(
            cfg.provider_for("claude-opus-5-5").unwrap().name,
            "anthropic"
        );
        assert_eq!(cfg.price_of("deepseek-v4-flash").input, 1.0);
        assert!(cfg.provider_for("local-thing").is_none());
    }

    #[test]
    fn keys_must_be_references() {
        let err = Config::parse(
            "provider \"x\" base=\"https://x\" key=\"sk-live-123\"\n",
            "test",
        )
        .unwrap_err();
        assert!(err.contains("keychain"));
    }

    #[test]
    fn unknown_models_are_priced_high_so_budgets_fail_safe() {
        let p = Config::builtin().price_of("brand-new-model");
        assert!(p.output >= 75.0);
        assert_eq!(
            Config::builtin()
                .price_of("claude-sonnet-5-5")
                .cost_micro(1_000_000, 0, 0),
            3_000_000
        );
    }

    #[test]
    fn office_settings_round_trip_and_secrets_stay_references() {
        let key = "ab".repeat(32);
        let text = format!(
            "tls ca-file=\"/etc/ssl/corp.pem\"\n\
env \"HTTPS_PROXY\" value=\"http://proxy.corp:3128\"\n\
env \"NPM_TOKEN\" value=\"keychain:wright/npm\"\n\
trust \"Wright gallery\" key=\"{key}\"\n\
worker \"claude-code\" command=\"claude\" args=\"--print\" env=\"ANTHROPIC_API_KEY=keychain:wright/anthropic\"\n"
        );
        let cfg = Config::parse_file_only(&text, "t").unwrap();
        assert_eq!(cfg.ca_file.as_deref(), Some("/etc/ssl/corp.pem"));
        assert_eq!(cfg.env.len(), 2);
        assert_eq!(cfg.trusts(&key.to_uppercase()), Some("Wright gallery"));
        assert_eq!(cfg.workers[0].env, vec![("ANTHROPIC_API_KEY".to_string(), "keychain:wright/anthropic".to_string())]);
        let back = Config::parse_file_only(&cfg.to_kdl(), "t").unwrap();
        assert_eq!(back.ca_file, cfg.ca_file);
        assert_eq!(back.env, cfg.env);
        assert_eq!(back.trusted, cfg.trusted);
        assert_eq!(back.workers, cfg.workers);

        // A secret-looking variable with a literal value is refused...
        let err = Config::parse_file_only("env \"GITHUB_TOKEN\" value=\"ghp_abc\"\n", "t").unwrap_err();
        assert!(err.contains("keychain:"), "{err}");
        // ...so is a proxy URL carrying a password, whatever its name...
        let err = Config::parse_file_only("env \"HTTPS_PROXY\" value=\"http://me:hunter2@proxy:3128\"\n", "t").unwrap_err();
        assert!(err.contains("keychain:"), "{err}");
        // ...and so is a worker sign-in given as a value.
        let err = Config::parse_file_only("worker \"codex\" command=\"codex\" env=\"OPENAI_API_KEY=sk-123\"\n", "t").unwrap_err();
        assert!(err.contains("keychain:"), "{err}");
        assert!(Config::parse_file_only("env \"1BAD\" value=\"x\"\n", "t").is_err());
        assert!(Config::parse_file_only("trust \"x\" key=\"short\"\n", "t").is_err());
    }

    #[test]
    fn a_config_file_does_not_hide_the_preconfigured_workers() {
        let cfg = Config::parse("worker \"claude-code\" command=\"/opt/claude\"\n", "t").unwrap();
        assert_eq!(cfg.worker_for("claude-code").unwrap().command, "/opt/claude", "the file wins");
        for w in ["codex", "kimi-code", "zcode"] {
            assert!(cfg.worker_for(w).is_some(), "{w} is still there");
        }
        let empty = Config::parse("interview model=\"x\"\n", "t").unwrap();
        assert!(empty.worker_for("claude-code").is_some(), "any config file at all used to drop them");
    }

    #[test]
    fn the_ca_file_is_checked_before_anyone_relies_on_it() {
        let dir = std::env::temp_dir().join(format!("wright-ca-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let good = dir.join("corp.pem");
        std::fs::write(&good, "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----\n").unwrap();
        assert_eq!(check_ca_file(&good).unwrap(), 1);
        let der = dir.join("corp.cer");
        std::fs::write(&der, [0x30u8, 0x82, 0x01]).unwrap();
        assert!(check_ca_file(&der).unwrap_err().contains("openssl x509"));
        assert!(check_ca_file(&dir.join("missing.pem")).is_err());
        assert_eq!(expand_home("/abs/x.pem"), PathBuf::from("/abs/x.pem"));
    }

}
