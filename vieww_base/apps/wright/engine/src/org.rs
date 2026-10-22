//! The administrator's side: organization policy applied to plans,
//! administrator approval of plans for unattended runs, and verification
//! of every receipt across the machine — the admin console's engine.
//! (`wright admin …` in the CLI; Settings > Organization in the window.)
//!
//! * [`check_plan`] — what in a blueprint the policy does not allow, one
//!   sentence each. Firing refuses a plan with any; the Interviewer is
//!   shown them, so it drafts within the rules.
//! * [`approve`] / [`approval`] — with `require unattended="admin"`, a
//!   plan runs unattended only once an administrator has signed ITS EXACT
//!   blueprint hash; a changed plan needs a new signature.
//! * [`fleet_verify`] — every run of every project this machine knows,
//!   through `wright verify`'s six checks.

use std::path::{Path, PathBuf};

use wright_core::blueprint::Blueprint;
use wright_core::events::now_ms;
use wright_core::wire::json::{self, Value};
use wright_core::{ed25519, hash, jcs};
use wright_runtime::config::Config;
use wright_runtime::policy::{self, Policy};
use wright_runtime::store::{write_atomic, Store};

/// Everything in `bp` that `p` does not allow. Empty = allowed.
pub fn check_plan(p: &Policy, bp: &Blueprint, config: &Config) -> Vec<String> {
    let mut out = Vec::new();
    let org = &p.org;
    let usd = |m: u64| format!("${:.2}", m as f64 / 1e6);
    if let Some(max) = p.max_per_run_micro_usd {
        if bp.guardrails.per_firing_micro_usd > max {
            out.push(format!(
                "{org} allows at most {} a run; this plan allows {}.",
                usd(max),
                usd(bp.guardrails.per_firing_micro_usd)
            ));
        }
    }
    if let Some(max) = p.max_per_day_micro_usd {
        if bp.guardrails.daily_micro_usd > max {
            out.push(format!(
                "{org} allows at most {} a day; this plan allows {}.",
                usd(max),
                usd(bp.guardrails.daily_micro_usd)
            ));
        }
    }
    for a in &bp.agents {
        for t in &a.tools {
            if p.forbid_tools.iter().any(|f| f == t) {
                out.push(format!(
                    "{} uses '{t}', which {org} does not allow.",
                    a.name
                ));
            }
        }
        if let (Some(w), Some(allowed)) = (&a.worker, &p.allow_workers) {
            if !allowed.contains(w) {
                out.push(format!(
                    "{} hands its work to '{w}', an external agent {org} has not allowed.",
                    a.name
                ));
            }
        }
        if let (Some(m), Some(allowed)) = (&a.mcp_server, &p.allow_mcp) {
            if !allowed.contains(m) {
                out.push(format!(
                    "{} talks to the MCP server '{m}', which {org} has not allowed.",
                    a.name
                ));
            }
        }
    }
    if let Some(allowed) = &p.allow_providers {
        let tiers = [
            &bp.models.cheap.prefer,
            &bp.models.cheap.fallback,
            &bp.models.quality.prefer,
            &bp.models.quality.fallback,
        ];
        for model in tiers.into_iter().filter(|m| !m.is_empty()) {
            let provider = config
                .provider_for(model)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "built-in".into());
            if !allowed.contains(&provider) {
                out.push(format!("The model '{model}' comes from '{provider}', which {org} has not allowed (allowed: {}).", allowed.join(", ")));
            }
        }
    }
    if p.require_tests && !bp.chips.tests {
        out.push(format!(
            "{org} requires every plan to be tested: turn tests on (chips {{ tests on }})."
        ));
    }
    out
}

/// The policy in force, applied to a plan: `Ok(None)` without a policy,
/// `Ok(Some)` when the plan is allowed, `Err` with every reason otherwise.
pub fn enforce(bp: &Blueprint, config: &Config) -> Result<Option<Policy>, String> {
    let Some(p) = policy::load()? else {
        return Ok(None);
    };
    let problems = check_plan(&p, bp, config);
    if problems.is_empty() {
        Ok(Some(p))
    } else {
        Err(format!(
            "{}'s policy does not allow this plan:\n  - {}",
            p.org,
            problems.join("\n  - ")
        ))
    }
}

// ── administrator approval ─────────────────────────────────────────

fn approval_file(store: &Store) -> PathBuf {
    store.root().join("org-approval.json")
}

fn approval_body(project: &str, blueprint: &str, by: &str, at_ms: u64) -> Value {
    Value::obj(vec![
        ("type", Value::str("wright.org-approval/1")),
        ("project", Value::str(project)),
        ("blueprint", Value::str(blueprint)),
        ("by", Value::str(by)),
        ("at_ms", Value::int(at_ms as i64)),
    ])
}

/// An administrator signs this exact plan for unattended runs.
pub fn approve(project: &Path, admin_seed: &[u8; 32], admin_name: &str) -> Result<String, String> {
    let bp = crate::load_blueprint(project)?;
    let blueprint = crate::unattended::blueprint_hash(project)?;
    let at = now_ms();
    let body = approval_body(&bp.meta.name, &blueprint, admin_name, at);
    let sig = ed25519::sign(admin_seed, jcs::canonicalize(&body)?.as_bytes());
    let mut v = body;
    v.set(
        "signature",
        Value::str(sig.iter().map(|b| format!("{b:02x}")).collect::<String>()),
    );
    let store = Store::open(project)?;
    write_atomic(&approval_file(&store), json::to_string(&v).as_bytes())?;
    Ok(blueprint)
}

/// Whether the plan as it is now carries a valid administrator approval
/// under the policy in force. `Ok(())` too when no policy asks for one.
pub fn approval(project: &Path) -> Result<(), String> {
    let Some(p) = policy::load()? else {
        return Ok(());
    };
    if !p.require_admin_approval {
        return Ok(());
    }
    let waiting = format!(
        "waiting for {}'s approval (an administrator runs `wright admin approve` on this project)",
        p.org
    );
    let store = Store::open(project)?;
    let Ok(text) = std::fs::read_to_string(approval_file(&store)) else {
        return Err(waiting);
    };
    let v = json::parse(&text).map_err(|_| waiting.clone())?;
    let by = v.str_of("by").unwrap_or("").to_string();
    let blueprint = v.str_of("blueprint").unwrap_or("").to_string();
    let now = crate::unattended::blueprint_hash(project)?;
    if blueprint != now {
        return Err(format!(
            "the plan changed since {by} approved it for {}; it needs a new approval",
            p.org
        ));
    }
    let dir = policy::dir().ok_or_else(|| waiting.clone())?;
    let admins = policy::admins_in(&dir)?;
    let admin = admins.iter().find(|a| a.name == by).ok_or_else(|| {
        format!("the approval is signed by '{by}', who is not an administrator in admins.kdl")
    })?;
    let key: [u8; 32] = (0..32)
        .map(|i| u8::from_str_radix(&admin.key[i * 2..i * 2 + 2], 16).ok())
        .collect::<Option<Vec<u8>>>()
        .and_then(|v| v.try_into().ok())
        .ok_or("an administrator key is malformed")?;
    let sig: [u8; 64] = (0..64)
        .map(|i| {
            v.str_of("signature")
                .and_then(|s| s.get(i * 2..i * 2 + 2))
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        })
        .collect::<Option<Vec<u8>>>()
        .and_then(|v| v.try_into().ok())
        .ok_or("the approval's signature is malformed")?;
    let body = approval_body(
        v.str_of("project").unwrap_or(""),
        &blueprint,
        &by,
        v.get("at_ms").and_then(|n| n.as_u64()).unwrap_or(0),
    );
    if !ed25519::verify(&key, jcs::canonicalize(&body)?.as_bytes(), &sig) {
        return Err("the administrator's approval was changed after it was signed".into());
    }
    Ok(())
}

// ── fleet-wide verification ────────────────────────────────────────

/// One run's verdict.
#[derive(Debug, Clone, PartialEq)]
pub struct RunCheck {
    pub project: PathBuf,
    pub run: u64,
    pub ok: bool,
    /// The first failing check, in words.
    pub problem: Option<String>,
}

/// Every project with a Wright store under `roots` (one level deep, plus
/// the roots themselves) and every scheduled project, each run verified.
pub fn fleet_verify(roots: &[PathBuf]) -> Vec<RunCheck> {
    let mut projects: Vec<PathBuf> = Vec::new();
    let mut add = |p: PathBuf| {
        if p.join("blueprint.kdl").is_file() && !projects.contains(&p) {
            projects.push(p);
        }
    };
    for e in crate::scheduler::load() {
        add(PathBuf::from(e.project));
    }
    for r in roots {
        add(r.clone());
        if let Ok(rd) = std::fs::read_dir(r) {
            for e in rd.flatten() {
                add(e.path());
            }
        }
    }
    let mut out = Vec::new();
    for p in projects {
        let Ok(store) = Store::open(&p) else { continue };
        for run in store.runs() {
            match crate::verify::project_run(&p, Some(run)) {
                Ok(v) => out.push(RunCheck {
                    project: p.clone(),
                    run,
                    ok: v.ok,
                    problem: v
                        .checks
                        .iter()
                        .find(|c| !c.ok)
                        .map(|c| format!("{}: {}", c.name, c.note)),
                }),
                Err(e) => out.push(RunCheck {
                    project: p.clone(),
                    run,
                    ok: false,
                    problem: Some(e),
                }),
            }
        }
    }
    out
}

/// The fleet verification as text, or as JSON lines for a SIEM.
pub fn fleet_report(checks: &[RunCheck], as_json: bool) -> String {
    if as_json {
        return checks
            .iter()
            .map(|c| {
                json::to_string(&Value::obj(vec![
                    ("project", Value::str(c.project.display().to_string())),
                    ("run", Value::int(c.run as i64)),
                    ("ok", Value::Bool(c.ok)),
                    (
                        "problem",
                        c.problem.clone().map(Value::str).unwrap_or(Value::Null),
                    ),
                ])) + "\n"
            })
            .collect();
    }
    let bad: Vec<&RunCheck> = checks.iter().filter(|c| !c.ok).collect();
    let projects: std::collections::BTreeSet<&PathBuf> =
        checks.iter().map(|c| &c.project).collect();
    let mut out = format!(
        "{} run(s) across {} project(s): {} verify, {} do NOT\n",
        checks.len(),
        projects.len(),
        checks.len() - bad.len(),
        bad.len()
    );
    for c in bad {
        out.push_str(&format!(
            "  FAILED {} run {}: {}\n",
            c.project.display(),
            c.run,
            c.problem.clone().unwrap_or_default()
        ));
    }
    out
}

/// A short, stable id for an administrator key (for display).
pub fn key_id(public_hex: &str) -> String {
    hash::short(public_hex).to_string()
}
