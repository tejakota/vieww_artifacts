//! Organization policy: the rules an IT team sets for every Wright on its
//! machines, signed by an administrator, enforced at plan time and at the
//! gate.
//!
//! Where it lives — the trust root is the operating system's own file
//! permissions, because a rule the person can edit is not a rule:
//!
//! | OS | folder (writable by administrators only) |
//! |---|---|
//! | Linux | `/etc/wright/` |
//! | macOS | `/Library/Application Support/Wright/` |
//! | Windows | `%ProgramData%\Wright\` |
//!
//! Two files: `admins.kdl` lists the administrators' public keys
//! (`admin "IT security" key="<64 hex or did:key>"`), and `policy.kdl` is
//! the policy, ending in a `signature` line made by one of them
//! (`wright admin policy sign`). `WRIGHT_POLICY_DIR` points somewhere else
//! — honoured only when the system folder holds no policy, so it is a way
//! to try a policy, never a way around one.
//!
//! Failing closed: a policy that is present but unsigned, signed by
//! someone not in `admins.kdl`, or edited after signing stops every firing
//! with a sentence saying so. No policy at all means no organization rules.
//!
//! ```kdl
//! policy "Acme IT" version=4
//! budget max-per-run="2.00" max-per-day="10.00"
//! tools forbid="shell,browser"
//! network allow="*.acme.com,api.anthropic.com,hn.algolia.com"
//! providers allow="anthropic,built-in"
//! workers allow="claude-code"
//! mcp allow="time"
//! require tests=true unattended="admin"
//! signature "…128 hex…" by="IT security"
//! ```

use std::path::{Path, PathBuf};

use wright_core::blueprint::parse::{parse_doc, KdlValue};
use wright_core::ed25519;

/// The rules in force.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Policy {
    pub org: String,
    pub version: u64,
    pub max_per_run_micro_usd: Option<u64>,
    pub max_per_day_micro_usd: Option<u64>,
    pub forbid_tools: Vec<String>,
    /// Hosts agents may reach with http tools (`*.example.com` globs).
    pub allow_hosts: Option<Vec<String>>,
    /// Model providers plans may use (config provider names).
    pub allow_providers: Option<Vec<String>>,
    pub allow_workers: Option<Vec<String>>,
    pub allow_mcp: Option<Vec<String>>,
    /// Every plan runs Testwright and the Reviewer.
    pub require_tests: bool,
    /// Unattended runs need an administrator's signed approval of the plan.
    pub require_admin_approval: bool,
    /// Who signed it (a name from admins.kdl), and where it came from.
    pub signed_by: String,
    pub source: PathBuf,
}

/// One administrator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Admin {
    pub name: String,
    pub key: String,
}

/// The system folder for this OS.
pub fn system_dir() -> PathBuf {
    if cfg!(target_os = "windows") {
        PathBuf::from(std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".into()))
            .join("Wright")
    } else if cfg!(target_os = "macos") {
        PathBuf::from("/Library/Application Support/Wright")
    } else {
        PathBuf::from("/etc/wright")
    }
}

/// The folder whose policy is in force: the system folder when it holds
/// one, else `WRIGHT_POLICY_DIR` when set.
pub fn dir() -> Option<PathBuf> {
    let sys = system_dir();
    if sys.join("policy.kdl").exists() {
        return Some(sys);
    }
    std::env::var_os("WRIGHT_POLICY_DIR")
        .map(PathBuf::from)
        .filter(|d| d.join("policy.kdl").exists())
}

/// The policy in force: `Ok(None)` without one, `Err` (with the sentence
/// that stops everything) when one exists and cannot be trusted.
pub fn load() -> Result<Option<Policy>, String> {
    match dir() {
        None => Ok(None),
        Some(d) => load_from(&d).map(Some),
    }
}

/// Load and check the policy in one folder.
pub fn load_from(d: &Path) -> Result<Policy, String> {
    let refuse = |why: String| {
        format!("Your organization's policy ({}) cannot be trusted: {why}. Nothing runs until IT fixes it.", d.join("policy.kdl").display())
    };
    let text = std::fs::read_to_string(d.join("policy.kdl")).map_err(|e| refuse(e.to_string()))?;
    let admins = admins_in(d).map_err(refuse)?;
    let (body, sig, by) = split_signature(&text).map_err(refuse)?;
    let admin = admins
        .iter()
        .find(|a| a.name == by)
        .ok_or_else(|| refuse(format!("it is signed by '{by}', who is not in admins.kdl")))?;
    let key = hex32(&admin.key)
        .ok_or_else(|| refuse(format!("{}'s key in admins.kdl is malformed", admin.name)))?;
    let sig = hex64(&sig).ok_or_else(|| refuse("the signature is malformed".into()))?;
    if !ed25519::verify(&key, body.as_bytes(), &sig) {
        return Err(refuse(format!("it was changed after {by} signed it")));
    }
    let mut p = parse_rules(&body).map_err(refuse)?;
    p.signed_by = by;
    p.source = d.join("policy.kdl");
    Ok(p)
}

/// The administrators listed in a folder's `admins.kdl`.
pub fn admins_in(d: &Path) -> Result<Vec<Admin>, String> {
    let text = std::fs::read_to_string(d.join("admins.kdl"))
        .map_err(|_| "there is no admins.kdl beside it to say who may sign".to_string())?;
    let doc = parse_doc(&text).map_err(|e| format!("admins.kdl: {e}"))?;
    let mut out = Vec::new();
    for n in &doc {
        if n.name != "admin" {
            return Err(format!(
                "admins.kdl line {}: only `admin \"name\" key=\"...\"` lines belong here",
                n.line
            ));
        }
        let name = n
            .arg_str()
            .ok_or(format!("admins.kdl line {}: an admin needs a name", n.line))?
            .to_string();
        let key = n
            .prop_str("key")
            .and_then(crate::signing::normalize_key)
            .ok_or(format!(
                "admins.kdl line {}: '{name}' needs key=\"<64 hex or did:key>\"",
                n.line
            ))?;
        out.push(Admin { name, key });
    }
    if out.is_empty() {
        return Err("admins.kdl lists nobody".into());
    }
    Ok(out)
}

/// Normalise and split off the trailing `signature "hex" by="name"` line.
/// The signed body is every other line, trimmed of trailing space, LF
/// line ends — so a file copied through Windows still verifies.
pub fn split_signature(text: &str) -> Result<(String, String, String), String> {
    let mut body = Vec::new();
    let mut sig: Option<(String, String)> = None;
    for line in text.replace("\r\n", "\n").lines() {
        let t = line.trim_end();
        if t.trim_start().starts_with("signature ") {
            let doc = parse_doc(t.trim()).map_err(|e| format!("the signature line: {e}"))?;
            let n = doc.first().ok_or("an empty signature line")?;
            let value = n
                .arg_str()
                .ok_or("the signature line has no value")?
                .to_string();
            let by = n
                .prop_str("by")
                .ok_or("the signature line does not say by=\"who\"")?
                .to_string();
            sig = Some((value, by));
        } else {
            body.push(t.to_string());
        }
    }
    while body.last().is_some_and(|l| l.is_empty()) {
        body.pop();
    }
    let (value, by) = sig.ok_or("it is not signed (no `signature` line)")?;
    Ok((body.join("\n") + "\n", value, by))
}

/// Sign a policy text as `by` (replacing any signature it had).
pub fn sign(text: &str, seed: &[u8; 32], by: &str) -> Result<String, String> {
    let body = match split_signature(text) {
        Ok((b, _, _)) => b,
        Err(_) => {
            let mut lines: Vec<String> = text
                .replace("\r\n", "\n")
                .lines()
                .map(|l| l.trim_end().to_string())
                .collect();
            while lines.last().is_some_and(|l| l.is_empty()) {
                lines.pop();
            }
            lines.join("\n") + "\n"
        }
    };
    parse_rules(&body)?;
    let sig = ed25519::sign(seed, body.as_bytes());
    let hex: String = sig.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!(
        "{body}signature \"{hex}\" by=\"{}\"\n",
        by.replace('"', "")
    ))
}

fn list(v: Option<&str>) -> Vec<String> {
    v.unwrap_or("")
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn money(v: Option<&KdlValue>) -> Result<Option<u64>, String> {
    match v {
        None => Ok(None),
        Some(KdlValue::Int(i)) => Ok(Some((*i as u64) * 1_000_000)),
        Some(KdlValue::Str(s)) => s
            .trim_start_matches('$')
            .parse::<f64>()
            .map(|d| Some((d * 1e6).round() as u64))
            .map_err(|_| format!("'{s}' is not an amount of dollars")),
        Some(_) => Err("a budget is a number of dollars".into()),
    }
}

/// The rules of a policy body (no signature handling).
pub fn parse_rules(body: &str) -> Result<Policy, String> {
    let doc = parse_doc(body).map_err(|e| e.to_string())?;
    let mut p = Policy::default();
    for n in &doc {
        let flag = |k: &str| {
            matches!(n.prop(k), Some(KdlValue::Bool(true))) || n.prop_str(k) == Some("true")
        };
        match n.name.as_str() {
            "policy" => {
                p.org = n.arg_str().unwrap_or("your organization").to_string();
                p.version = match n.prop("version") {
                    Some(KdlValue::Int(i)) => *i as u64,
                    Some(KdlValue::Str(s)) => s.parse().unwrap_or(0),
                    _ => 0,
                };
            }
            "budget" => {
                p.max_per_run_micro_usd = money(n.prop("max-per-run"))?;
                p.max_per_day_micro_usd = money(n.prop("max-per-day"))?;
            }
            "tools" => p.forbid_tools = list(n.prop_str("forbid")),
            "network" => p.allow_hosts = Some(list(n.prop_str("allow"))),
            "providers" => p.allow_providers = Some(list(n.prop_str("allow"))),
            "workers" => p.allow_workers = Some(list(n.prop_str("allow"))),
            "mcp" => p.allow_mcp = Some(list(n.prop_str("allow"))),
            "require" => {
                p.require_tests |= flag("tests");
                p.require_admin_approval |= n.prop_str("unattended") == Some("admin");
            }
            other => return Err(format!("line {}: '{other}' is not a policy rule (policy | budget | tools | network | providers | workers | mcp | require)", n.line)),
        }
    }
    if p.org.is_empty() {
        return Err("a policy starts with `policy \"<organization>\" version=<n>`".into());
    }
    Ok(p)
}

/// `*.example.com` matches `api.example.com` (not `example.com`); a plain
/// name matches itself; `*` matches everything.
pub fn host_allowed(patterns: &[String], host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    patterns.iter().any(|p| {
        let p = p.to_ascii_lowercase();
        p == "*"
            || p == host
            || p.strip_prefix("*.")
                .is_some_and(|suffix| host.ends_with(&format!(".{suffix}")))
    })
}

impl Policy {
    /// The rules, one sentence each, for the window and `wright admin policy show`.
    pub fn sentences(&self) -> Vec<String> {
        let mut out = vec![format!(
            "{} policy, version {}, signed by {}.",
            self.org, self.version, self.signed_by
        )];
        let usd = |m: u64| format!("${:.2}", m as f64 / 1e6);
        if let Some(r) = self.max_per_run_micro_usd {
            out.push(format!("A run may spend at most {}.", usd(r)));
        }
        if let Some(d) = self.max_per_day_micro_usd {
            out.push(format!("A project may spend at most {} a day.", usd(d)));
        }
        if !self.forbid_tools.is_empty() {
            out.push(format!(
                "These tools are not allowed: {}.",
                self.forbid_tools.join(", ")
            ));
        }
        if let Some(h) = &self.allow_hosts {
            out.push(format!(
                "Agents may reach only: {}.",
                if h.is_empty() {
                    "nothing on the network".into()
                } else {
                    h.join(", ")
                }
            ));
        }
        if let Some(v) = &self.allow_providers {
            out.push(format!("Models come only from: {}.", v.join(", ")));
        }
        if let Some(v) = &self.allow_workers {
            out.push(format!(
                "External coding agents allowed: {}.",
                if v.is_empty() {
                    "none".into()
                } else {
                    v.join(", ")
                }
            ));
        }
        if let Some(v) = &self.allow_mcp {
            out.push(format!(
                "MCP servers allowed: {}.",
                if v.is_empty() {
                    "none".into()
                } else {
                    v.join(", ")
                }
            ));
        }
        if self.require_tests {
            out.push("Every plan is tested and reviewed before anything is delivered.".into());
        }
        if self.require_admin_approval {
            out.push("A plan runs unattended only after an administrator approves it.".into());
        }
        out
    }
}

fn hex32(h: &str) -> Option<[u8; 32]> {
    let v = (0..32)
        .map(|i| u8::from_str_radix(h.get(i * 2..i * 2 + 2)?, 16).ok())
        .collect::<Option<Vec<u8>>>()?;
    v.try_into().ok()
}

fn hex64(h: &str) -> Option<[u8; 64]> {
    if h.len() != 128 {
        return None;
    }
    let v = (0..64)
        .map(|i| u8::from_str_radix(h.get(i * 2..i * 2 + 2)?, 16).ok())
        .collect::<Option<Vec<u8>>>()?;
    v.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = "policy \"Acme IT\" version=4\nbudget max-per-run=\"2.00\" max-per-day=\"10\"\ntools forbid=\"shell\"\nnetwork allow=\"*.acme.com,hn.algolia.com\"\nproviders allow=\"anthropic,built-in\"\nrequire tests=true unattended=\"admin\"\n";

    fn folder(tag: &str, admin_seed: [u8; 32]) -> PathBuf {
        let d = std::env::temp_dir().join(format!("wright-policy-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let pk: String = ed25519::public_key(&admin_seed)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        std::fs::write(
            d.join("admins.kdl"),
            format!("admin \"IT security\" key=\"{pk}\"\n"),
        )
        .unwrap();
        d
    }

    #[test]
    fn a_signed_policy_loads_and_says_its_rules() {
        let d = folder("ok", [3u8; 32]);
        std::fs::write(
            d.join("policy.kdl"),
            sign(BODY, &[3u8; 32], "IT security").unwrap(),
        )
        .unwrap();
        let p = load_from(&d).unwrap();
        assert_eq!(p.org, "Acme IT");
        assert_eq!(p.max_per_run_micro_usd, Some(2_000_000));
        assert_eq!(p.max_per_day_micro_usd, Some(10_000_000));
        assert!(p.require_tests && p.require_admin_approval);
        assert!(p.sentences().iter().any(|s| s.contains("*.acme.com")));
        // Windows line ends do not break it.
        let crlf = std::fs::read_to_string(d.join("policy.kdl"))
            .unwrap()
            .replace('\n', "\r\n");
        std::fs::write(d.join("policy.kdl"), crlf).unwrap();
        assert!(load_from(&d).is_ok());
    }

    #[test]
    fn an_untrusted_policy_fails_closed() {
        let d = folder("bad", [3u8; 32]);
        // Unsigned.
        std::fs::write(d.join("policy.kdl"), BODY).unwrap();
        assert!(load_from(&d).unwrap_err().contains("not signed"));
        // Signed by someone not in admins.kdl.
        std::fs::write(
            d.join("policy.kdl"),
            sign(BODY, &[9u8; 32], "Mallory").unwrap(),
        )
        .unwrap();
        assert!(load_from(&d).unwrap_err().contains("not in admins.kdl"));
        // Signed by a listed name but the wrong key.
        std::fs::write(
            d.join("policy.kdl"),
            sign(BODY, &[9u8; 32], "IT security").unwrap(),
        )
        .unwrap();
        assert!(load_from(&d).unwrap_err().contains("changed after"));
        // Loosened after signing.
        let good = sign(BODY, &[3u8; 32], "IT security").unwrap();
        std::fs::write(
            d.join("policy.kdl"),
            good.replace("max-per-run=\"2.00\"", "max-per-run=\"200.00\""),
        )
        .unwrap();
        let err = load_from(&d).unwrap_err();
        assert!(
            err.contains("changed after") && err.contains("Nothing runs"),
            "{err}"
        );
        // No admins.kdl at all.
        std::fs::remove_file(d.join("admins.kdl")).unwrap();
        std::fs::write(d.join("policy.kdl"), good).unwrap();
        assert!(load_from(&d).unwrap_err().contains("admins.kdl"));
    }

    #[test]
    fn hosts_match_like_a_person_expects() {
        let p = vec!["*.acme.com".to_string(), "hn.algolia.com".to_string()];
        assert!(host_allowed(&p, "api.acme.com"));
        assert!(host_allowed(&p, "HN.algolia.com."));
        assert!(
            !host_allowed(&p, "acme.com"),
            "a wildcard is for subdomains"
        );
        assert!(!host_allowed(&p, "evil-acme.com"));
        assert!(!host_allowed(&p, "acme.com.evil.net"));
        assert!(host_allowed(&["*".into()], "anything.example"));
    }
}
