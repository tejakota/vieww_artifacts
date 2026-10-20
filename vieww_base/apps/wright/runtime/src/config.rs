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
//! provider "deepseek" kind="openai" base="https://api.deepseek.com/v1" key="keychain:wright/deepseek" models="deepseek-*"
//! provider "local" kind="openai" base="http://127.0.0.1:7878/v1" models="*"
//! price model="claude-opus-*" in=15 out=75 cached=1.5
//! interview model="claude-sonnet-5-5"
//! ```
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

use crate::models::{Anthropic, OpenAiCompatible, Provider};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderKind {
    Anthropic,
    OpenAi,
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
            ProviderKind::Anthropic => Box::new(Anthropic { base_url: self.base_url.clone() }),
            ProviderKind::OpenAi => Box::new(OpenAiCompatible { base_url: self.base_url.clone() }),
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

impl Price {
    /// Cost in micro-USD. (USD per million tokens == micro-USD per token.)
    pub fn cost_micro(&self, input: u64, cached: u64, output: u64) -> u64 {
        let fresh = input.saturating_sub(cached);
        (fresh as f64 * self.input + cached as f64 * self.cached_input + output as f64 * self.output).ceil() as u64
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub providers: Vec<ProviderEntry>,
    pub prices: Vec<(String, Price)>,
    /// The model the Interviewer uses (`interview model="..."`).
    pub interview_model: String,
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

/// Where Wright keeps machine-wide state.
pub fn wright_home() -> PathBuf {
    if let Ok(h) = std::env::var("WRIGHT_HOME") {
        return PathBuf::from(h);
    }
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".wright")
}

impl Config {
    /// Public endpoints of the major providers. Prices are list prices
    /// at the time of writing — override them in config.kdl.
    pub fn builtin() -> Config {
        let p = |name: &str, kind: ProviderKind, base: &str, models: &str| ProviderEntry {
            name: name.into(),
            kind,
            base_url: base.into(),
            key: Some(format!("keychain:wright/{name}")),
            models: models.split(',').map(String::from).collect(),
        };
        let price = |m: &str, i: f64, o: f64, c: f64| (m.to_string(), Price { input: i, output: o, cached_input: c });
        Config {
            providers: vec![
                p("anthropic", ProviderKind::Anthropic, "https://api.anthropic.com", "claude-*"),
                p("openai", ProviderKind::OpenAi, "https://api.openai.com/v1", "gpt-*,o1*,o3*,o4*"),
                p("deepseek", ProviderKind::OpenAi, "https://api.deepseek.com/v1", "deepseek-*"),
                p("zai", ProviderKind::OpenAi, "https://api.z.ai/api/paas/v4", "glm-*"),
                p("moonshot", ProviderKind::OpenAi, "https://api.moonshot.ai/v1", "kimi-*,moonshot-*"),
                p("xai", ProviderKind::OpenAi, "https://api.x.ai/v1", "grok-*"),
            ],
            prices: vec![
                price("claude-opus-*", 15.0, 75.0, 1.5),
                price("claude-sonnet-*", 3.0, 15.0, 0.3),
                price("claude-haiku-*", 1.0, 5.0, 0.1),
                price("deepseek-*", 0.3, 1.2, 0.03),
                price("glm-*", 0.6, 2.2, 0.11),
                price("kimi-*", 0.6, 2.5, 0.15),
                price("mock-*", 0.0, 0.0, 0.0),
            ],
            interview_model: "claude-sonnet-5-5".into(),
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
        let doc = parse_doc(text).map_err(|e| format!("{source}: {e}"))?;
        let mut cfg = Config { providers: Vec::new(), prices: Vec::new(), interview_model: String::new(), source: source.into() };
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
                        Some(other) => return Err(format!("{source}: line {}: unknown provider kind '{other}' (anthropic | openai)", node.line)),
                    };
                    let key = s("key");
                    if let Some(k) = &key {
                        if !k.starts_with("keychain:") {
                            return Err(format!("{source}: line {}: provider keys must be keychain: references, never values", node.line));
                        }
                    }
                    cfg.providers.push(ProviderEntry {
                        name,
                        kind,
                        base_url: s("base").ok_or_else(|| format!("{source}: line {}: provider needs base=\"https://...\"", node.line))?,
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
                other => return Err(format!("{source}: line {}: unknown setting '{other}' (provider | price | interview)", node.line)),
            }
        }
        let b = Config::builtin();
        cfg.providers.extend(b.providers);
        cfg.prices.extend(b.prices);
        if cfg.interview_model.is_empty() {
            cfg.interview_model = b.interview_model;
        }
        Ok(cfg)
    }

    pub fn provider_for(&self, model: &str) -> Option<&ProviderEntry> {
        self.providers.iter().find(|p| p.matches(model))
    }

    /// Price of a model. Unknown models get a deliberately HIGH default
    /// ($15 in / $75 out) so a missing price can only stop a firing
    /// early, never let it overspend.
    pub fn price_of(&self, model: &str) -> Price {
        self.prices
            .iter()
            .find(|(p, _)| glob(p, model))
            .map(|(_, pr)| *pr)
            .unwrap_or(Price { input: 15.0, output: 75.0, cached_input: 15.0 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(cfg.provider_for("claude-opus-5-5").unwrap().name, "anthropic");
        assert_eq!(cfg.price_of("deepseek-v4-flash").input, 1.0);
        assert!(cfg.provider_for("local-thing").is_none());
    }

    #[test]
    fn keys_must_be_references() {
        let err = Config::parse("provider \"x\" base=\"https://x\" key=\"sk-live-123\"\n", "test").unwrap_err();
        assert!(err.contains("keychain"));
    }

    #[test]
    fn unknown_models_are_priced_high_so_budgets_fail_safe() {
        let p = Config::builtin().price_of("brand-new-model");
        assert!(p.output >= 75.0);
        assert_eq!(Config::builtin().price_of("claude-sonnet-5-5").cost_micro(1_000_000, 0, 0), 3_000_000);
    }
}
