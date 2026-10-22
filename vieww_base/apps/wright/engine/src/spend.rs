//! Where the money went: cost attributed per agent, per model and per
//! tool, from a run's own event log.
//!
//! Budgets say how much a run *may* spend; this says what each part of it
//! *did* spend — model calls, tokens in, cached and out, the models used,
//! the tool calls (and how many failed), and what the cache saved. It is
//! written into the receipt (so it is signed with everything else) and
//! printed by `wright explain` and `wright spend`, which also exports a
//! CSV a finance team can reconcile against the provider's invoice.
//!
//! Built from the events, never from a model's account of itself: a
//! `TokensRecorded` event is the provider's own usage for one call.

use std::collections::BTreeMap;

use wright_core::events::{Event, EventKind};
use wright_core::wire::json::Value;

/// One agent's share of a run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentSpend {
    pub agent: String,
    pub model_calls: u64,
    pub tokens_in: u64,
    pub tokens_cached: u64,
    pub tokens_out: u64,
    pub cost_micro_usd: u64,
    /// What reusing a cached result saved instead.
    pub saved_micro_usd: u64,
    pub models: Vec<String>,
    /// Tool → (calls, failed calls).
    pub tools: BTreeMap<String, (u64, u64)>,
}

/// One model's share of a run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelSpend {
    pub model: String,
    pub calls: u64,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cost_micro_usd: u64,
}

/// The whole attribution.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Spend {
    pub agents: Vec<AgentSpend>,
    pub models: Vec<ModelSpend>,
    pub total_micro_usd: u64,
    pub saved_micro_usd: u64,
}

/// Attribute a run's cost from its events.
pub fn attribute(events: &[Event]) -> Spend {
    let mut agents: BTreeMap<String, AgentSpend> = BTreeMap::new();
    let mut models: BTreeMap<String, ModelSpend> = BTreeMap::new();
    fn get<'a>(agents: &'a mut BTreeMap<String, AgentSpend>, name: &str) -> &'a mut AgentSpend {
        agents.entry(name.to_string()).or_insert_with(|| AgentSpend {
            agent: name.to_string(),
            ..AgentSpend::default()
        })
    }
    let mut total = 0u64;
    let mut saved = 0u64;
    for e in events {
        let who = e.agent.clone().unwrap_or_else(|| "wright".into());
        let data = e.data_value();
        let n = |k: &str| data.as_ref().and_then(|v| v.get(k).and_then(|x| x.as_u64())).unwrap_or(0);
        match e.kind {
            EventKind::TokensRecorded => {
                let model = data.as_ref().and_then(|v| v.str_of("model").map(String::from)).unwrap_or_default();
                let cost = n("cost_micro_usd");
                total += cost;
                let a = get(&mut agents, &who);
                a.model_calls += 1;
                a.tokens_in += n("in");
                a.tokens_cached += n("cached");
                a.tokens_out += n("out");
                a.cost_micro_usd += cost;
                if !model.is_empty() && !a.models.contains(&model) {
                    a.models.push(model.clone());
                }
                let m = models.entry(model.clone()).or_insert_with(|| ModelSpend { model, ..ModelSpend::default() });
                m.calls += 1;
                m.tokens_in += n("in");
                m.tokens_out += n("out");
                m.cost_micro_usd += cost;
            }
            EventKind::ToolCalled => {
                // Structured since 1.0 review; older logs: the first word.
                let tool = data
                    .as_ref()
                    .and_then(|v| v.str_of("tool").map(String::from))
                    .unwrap_or_else(|| e.note.split_whitespace().next().unwrap_or("?").to_string());
                let ok = data
                    .as_ref()
                    .and_then(|v| v.get("ok").and_then(|b| b.as_bool()))
                    .unwrap_or(!e.note.contains("— error"));
                let slot = get(&mut agents, &who).tools.entry(tool).or_insert((0, 0));
                slot.0 += 1;
                if !ok {
                    slot.1 += 1;
                }
            }
            EventKind::CacheHit => {
                let s = n("saved_micro_usd");
                saved += s;
                get(&mut agents, &who).saved_micro_usd += s;
            }
            _ => {}
        }
    }
    Spend {
        agents: agents.into_values().collect(),
        models: models.into_values().filter(|m| !m.model.is_empty()).collect(),
        total_micro_usd: total,
        saved_micro_usd: saved,
    }
}

impl Spend {
    /// The receipt's `spend` section.
    pub fn to_value(&self) -> Value {
        Value::obj(vec![
            ("total_micro_usd", Value::int(self.total_micro_usd as i64)),
            ("saved_micro_usd", Value::int(self.saved_micro_usd as i64)),
            (
                "agents",
                Value::Arr(
                    self.agents
                        .iter()
                        .map(|a| {
                            Value::obj(vec![
                                ("agent", Value::str(a.agent.clone())),
                                ("cost_micro_usd", Value::int(a.cost_micro_usd as i64)),
                                ("saved_micro_usd", Value::int(a.saved_micro_usd as i64)),
                                ("model_calls", Value::int(a.model_calls as i64)),
                                ("tokens_in", Value::int(a.tokens_in as i64)),
                                ("tokens_cached", Value::int(a.tokens_cached as i64)),
                                ("tokens_out", Value::int(a.tokens_out as i64)),
                                ("models", Value::Arr(a.models.iter().map(|m| Value::str(m.clone())).collect())),
                                (
                                    "tools",
                                    Value::Arr(
                                        a.tools
                                            .iter()
                                            .map(|(t, (c, f))| {
                                                Value::obj(vec![
                                                    ("tool", Value::str(t.clone())),
                                                    ("calls", Value::int(*c as i64)),
                                                    ("failed", Value::int(*f as i64)),
                                                ])
                                            })
                                            .collect(),
                                    ),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "models",
                Value::Arr(
                    self.models
                        .iter()
                        .map(|m| {
                            Value::obj(vec![
                                ("model", Value::str(m.model.clone())),
                                ("calls", Value::int(m.calls as i64)),
                                ("tokens_in", Value::int(m.tokens_in as i64)),
                                ("tokens_out", Value::int(m.tokens_out as i64)),
                                ("cost_micro_usd", Value::int(m.cost_micro_usd as i64)),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }

    /// Lines for `wright explain`: the costliest agent first.
    pub fn lines(&self) -> Vec<String> {
        let mut agents: Vec<&AgentSpend> = self.agents.iter().collect();
        agents.sort_by(|a, b| b.cost_micro_usd.cmp(&a.cost_micro_usd).then(a.agent.cmp(&b.agent)));
        agents
            .iter()
            .map(|a| {
                let tools: u64 = a.tools.values().map(|(c, _)| c).sum();
                let failed: u64 = a.tools.values().map(|(_, f)| f).sum();
                format!(
                    "{:<14} ${:.4}  {} model call(s), {} in / {} out tokens{}, {} tool call(s){}{}",
                    a.agent,
                    a.cost_micro_usd as f64 / 1e6,
                    a.model_calls,
                    a.tokens_in,
                    a.tokens_out,
                    if a.models.is_empty() { String::new() } else { format!(" on {}", a.models.join(", ")) },
                    tools,
                    if failed > 0 { format!(" ({failed} failed)") } else { String::new() },
                    if a.saved_micro_usd > 0 { format!(", cache saved ${:.4}", a.saved_micro_usd as f64 / 1e6) } else { String::new() },
                )
            })
            .collect()
    }
}

/// CSV rows (header first) over many runs: one row per run × agent.
/// Money in USD with six decimals, so sums reconcile to the micro-dollar.
pub fn csv(runs: &[(u64, u64, Spend)]) -> String {
    let mut out = String::from("run,finished_utc,agent,models,model_calls,tokens_in,tokens_cached,tokens_out,cost_usd,saved_usd,tool_calls,failed_tool_calls\n");
    for (run, finished_ms, s) in runs {
        for a in &s.agents {
            let calls: u64 = a.tools.values().map(|(c, _)| c).sum();
            let failed: u64 = a.tools.values().map(|(_, f)| f).sum();
            out.push_str(&format!(
                "{run},{},{},{},{},{},{},{},{:.6},{:.6},{calls},{failed}\n",
                wright_core::schedule::rfc3339_utc((finished_ms / 1000) as i64),
                quote(&a.agent),
                quote(&a.models.join(" ")),
                a.model_calls,
                a.tokens_in,
                a.tokens_cached,
                a.tokens_out,
                a.cost_micro_usd as f64 / 1e6,
                a.saved_micro_usd as f64 / 1e6,
            ));
        }
    }
    out
}

fn quote(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wright_core::events::{Event, EventKind};

    fn ev(kind: EventKind, agent: &str, note: &str, data: &str) -> Event {
        Event {
            seq: 0,
            run: 1,
            at_ms: 0,
            kind,
            agent: Some(agent.into()),
            note: note.into(),
            data: data.into(),
        }
    }

    #[test]
    fn cost_lands_on_the_agent_that_spent_it() {
        let events = vec![
            ev(EventKind::TokensRecorded, "Fetcher", "", r#"{"in":100,"cached":20,"out":10,"model":"m-1","cost_micro_usd":300}"#),
            ev(EventKind::TokensRecorded, "Fetcher", "", r#"{"in":50,"cached":0,"out":5,"model":"m-1","cost_micro_usd":150}"#),
            ev(EventKind::TokensRecorded, "Writer", "", r#"{"in":10,"cached":0,"out":90,"model":"m-2","cost_micro_usd":1000}"#),
            ev(EventKind::ToolCalled, "Fetcher", "http-read GET x — ok (3 ms)", r#"{"tool":"http-read","ok":true,"ms":3}"#),
            ev(EventKind::ToolCalled, "Fetcher", "http-read GET y — error: 404", ""),
            ev(EventKind::CacheHit, "Summarizer", "", r#"{"saved_micro_usd":700}"#),
        ];
        let s = attribute(&events);
        assert_eq!(s.total_micro_usd, 1450);
        assert_eq!(s.saved_micro_usd, 700);
        let f = s.agents.iter().find(|a| a.agent == "Fetcher").unwrap();
        assert_eq!((f.model_calls, f.tokens_in, f.tokens_cached, f.tokens_out, f.cost_micro_usd), (2, 150, 20, 15, 450));
        assert_eq!(f.tools.get("http-read"), Some(&(2, 1)), "an old-style error line still counts as failed");
        assert_eq!(s.models.iter().map(|m| m.cost_micro_usd).sum::<u64>(), 1450, "models add up to the total");
        assert!(s.lines()[0].starts_with("Writer"), "costliest first: {:?}", s.lines());
        let csv = csv(&[(1, 0, s)]);
        assert!(csv.lines().nth(1).unwrap().starts_with("1,1970-01-01T00:00:00Z,Fetcher,m-1,2,150,20,15,0.000450"), "{csv}");
    }
}
