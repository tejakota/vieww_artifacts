//! # wright-tools
//!
//! Tool implementations. Every entry point takes a [`Permit`] — proof,
//! checked by the compiler, that the permission gate allowed this exact
//! call. There is no other way in.
//!
//! | Tool          | What it does                                  | Guard                         |
//! |---------------|-----------------------------------------------|-------------------------------|
//! | write-output  | write one declared `out` artifact             | name must be declared         |
//! | files-read    | read a project file (64 KiB pages)            | [`paths::resolve`]            |
//! | files-list    | list a project directory                      | [`paths::resolve`]            |
//! | files-write   | create/overwrite a project file (≤ 1 MiB)     | [`paths::resolve`]            |
//! | edit          | replace one exact unique snippet              | [`paths::resolve`]            |
//! | apply-patch   | apply a unified diff, all-or-nothing          | [`paths::resolve`]            |
//! | git           | status / diff / log / add / commit            | [`sandbox`] + flag allowlist  |
//! | run-tests     | the project's test command                    | [`sandbox`]                   |
//! | shell         | any command                                   | [`sandbox`] (net only if granted) |
//! | http-read     | GET a public URL (≤ 1 MiB, text)              | no private/metadata addresses |
//! | http-post     | POST to a public URL                          | same; approval-eligible       |
//! | browser       | (not implemented: refuses loudly)             |                               |

pub mod mcp;
pub mod mcp_install;
pub mod patch;
pub mod paths;
pub mod sandbox;
pub mod worker;

use std::io::Read;
use std::net::{IpAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;

use wright_core::blueprint::{PermissionSet, SecretRef};
use wright_core::convention;
use wright_core::wire::http::{parse_url, Request};
use wright_core::wire::json::Value;
pub use wright_runtime::config as config_shim;
use wright_runtime::gate::Permit;
use wright_runtime::net::{self, NetConfig};
use wright_runtime::secrets;

/// Everything a tool needs to run. Built once per task by the engine.
#[derive(Debug, Clone)]
pub struct ToolCtx {
    /// Canonical project directory.
    pub project: PathBuf,
    /// The calling task's declared outputs (for `write-output`).
    pub outputs: Vec<String>,
    /// Effective test command (blueprint's, else the convention's).
    pub test_cmd: String,
    /// The blueprint's secret references (for `http-post` auth).
    pub secrets: Vec<SecretRef>,
    pub net: NetConfig,
    /// Allow http tools to reach private / loopback addresses. Off by
    /// default (SSRF guard); local development and tests turn it on.
    pub allow_private_network: bool,
    /// Extra writable dirs for the sandbox (e.g. CARGO_HOME).
    pub sandbox_extra_rw: Vec<PathBuf>,
    /// The worker this task hands its step to, if any (ADR-026).
    pub worker: Option<crate::config_shim::WorkerEntry>,
    /// The name the card used, for error sentences when unconfigured.
    pub worker_name: Option<String>,
    /// The files the card allows a worker to change (empty = all).
    pub worker_files: Vec<String>,
    /// Wall-clock seconds for the worker run.
    pub worker_timeout_secs: u64,
    /// The hash-pinned MCP server this card may call (ADR-029).
    pub mcp_server: Option<crate::config_shim::McpEntry>,
    /// The server tools this card may call (empty = none).
    pub mcp_allow: Vec<String>,
    /// The organization's network allow-list (`network allow=...` in its
    /// policy): http tools reach only these hosts. None = no such rule.
    pub allowed_hosts: Option<Vec<String>>,
}

impl ToolCtx {
    pub fn new(project: &Path) -> Result<ToolCtx, String> {
        Ok(ToolCtx {
            project: project
                .canonicalize()
                .map_err(|e| format!("project dir {}: {e}", project.display()))?,
            outputs: Vec::new(),
            test_cmd: String::new(),
            secrets: Vec::new(),
            net: NetConfig::from_env(),
            allow_private_network: std::env::var("WRIGHT_ALLOW_PRIVATE_NETWORK")
                .map(|v| v == "1")
                .unwrap_or(false),
            sandbox_extra_rw: Vec::new(),
            worker: None,
            worker_name: None,
            worker_files: Vec::new(),
            worker_timeout_secs: 0,
            mcp_server: None,
            mcp_allow: Vec::new(),
            allowed_hosts: None,
        })
    }

    pub fn artifacts_dir(&self) -> PathBuf {
        self.project.join("artifacts")
    }
}

/// What a tool call produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutput {
    /// Text the model sees.
    pub text: String,
    /// Project-relative files this call wrote (for ArtifactWritten events).
    pub wrote: Vec<String>,
}

fn ok(text: impl Into<String>) -> Result<ToolOutput, ToolError> {
    Ok(ToolOutput {
        text: text.into(),
        wrote: Vec::new(),
    })
}

/// A tool failure. The text goes back to the model as an error result,
/// so it is written to be acted on ("call again with offset=...").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolError {
    NotImplemented(&'static str),
    BadInput(String),
    Failed(String),
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolError::NotImplemented(name) => write!(f, "tool '{name}' is not implemented yet"),
            ToolError::BadInput(m) => write!(f, "bad input: {m}"),
            ToolError::Failed(m) => write!(f, "{m}"),
        }
    }
}

fn s<'a>(input: &'a Value, key: &str) -> Result<&'a str, ToolError> {
    input
        .str_of(key)
        .ok_or_else(|| ToolError::BadInput(format!("missing string field '{key}'")))
}

/// Run the tool named by `permit` with a JSON object `input`.
pub fn run(permit: Permit, input: &Value, ctx: &ToolCtx) -> Result<ToolOutput, ToolError> {
    match permit.tool().name {
        "write-output" => write_output(input, ctx),
        "files-read" => files_read(input, ctx),
        "files-list" => files_list(input, ctx),
        "files-write" => files_write(input, ctx),
        "edit" => edit(input, ctx),
        "apply-patch" => apply_patch(input, ctx),
        "git" => git(&permit, input, ctx),
        "run-tests" => run_tests(&permit, ctx),
        "shell" => shell(&permit, input, ctx),
        "http-read" => http_read(input, ctx),
        "http-post" => http_post(&permit, input, ctx),
        "worker" => worker::run(&permit, input, ctx),
        "mcp-call" => mcp::run(&permit, input, ctx),
        "browser" => Err(ToolError::NotImplemented("browser")),
        other => Err(ToolError::Failed(format!(
            "tool '{other}' has no implementation"
        ))),
    }
}

// ── files ────────────────────────────────────────────────────────

/// Maximum bytes per `files-read` page.
pub const READ_PAGE: usize = 64 * 1024;
/// Maximum bytes per write.
pub const WRITE_LIMIT: usize = 1024 * 1024;

fn resolve(ctx: &ToolCtx, rel: &str) -> Result<PathBuf, ToolError> {
    paths::resolve(&ctx.project, rel).map_err(ToolError::Failed)
}

fn write_file(ctx: &ToolCtx, rel: &str, content: &str) -> Result<PathBuf, ToolError> {
    if content.len() > WRITE_LIMIT {
        return Err(ToolError::BadInput(format!(
            "content is {} bytes; the limit is {WRITE_LIMIT}",
            content.len()
        )));
    }
    let full = resolve(ctx, rel)?;
    if full.is_dir() {
        return Err(ToolError::Failed(format!("'{rel}' is a directory")));
    }
    paths::create_parents(&ctx.project, &full).map_err(ToolError::Failed)?;
    wright_runtime::store::write_atomic(&full, content.as_bytes()).map_err(ToolError::Failed)?;
    Ok(full)
}

fn rel_of(ctx: &ToolCtx, full: &Path) -> String {
    full.strip_prefix(&ctx.project)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
}

fn write_output(input: &Value, ctx: &ToolCtx) -> Result<ToolOutput, ToolError> {
    let name = s(input, "name")?;
    if !ctx.outputs.iter().any(|o| o == name) {
        return Err(ToolError::BadInput(format!(
            "'{name}' is not one of your declared outputs ({})",
            if ctx.outputs.is_empty() {
                "you have none".to_string()
            } else {
                ctx.outputs.join(", ")
            }
        )));
    }
    let content = s(input, "content")?;
    let full = write_file(ctx, &format!("artifacts/{name}"), content)?;
    Ok(ToolOutput {
        text: format!("wrote artifacts/{name} ({} bytes)", content.len()),
        wrote: vec![rel_of(ctx, &full)],
    })
}

fn files_read(input: &Value, ctx: &ToolCtx) -> Result<ToolOutput, ToolError> {
    let rel = s(input, "path")?;
    let offset = input.get("offset").and_then(|o| o.as_u64()).unwrap_or(0);
    let full = resolve(ctx, rel)?;
    let mut f =
        std::fs::File::open(&full).map_err(|e| ToolError::Failed(format!("read {rel}: {e}")))?;
    let size = f.metadata().map(|m| m.len()).unwrap_or(0);
    if offset > 0 {
        use std::io::Seek;
        f.seek(std::io::SeekFrom::Start(offset))
            .map_err(|e| ToolError::Failed(e.to_string()))?;
    }
    // Bounded read: never pull more than one page into memory.
    let mut buf = Vec::with_capacity(READ_PAGE.min(size as usize));
    f.take(READ_PAGE as u64)
        .read_to_end(&mut buf)
        .map_err(|e| ToolError::Failed(format!("read {rel}: {e}")))?;
    if buf.contains(&0) {
        return Err(ToolError::Failed(format!(
            "{rel} looks binary; files-read returns text only"
        )));
    }
    let mut text = String::from_utf8_lossy(&buf).into_owned();
    let end = offset + buf.len() as u64;
    if end < size {
        text.push_str(&format!(
            "\n[... {} more bytes; call again with offset={end} ...]",
            size - end
        ));
    }
    ok(text)
}

fn files_list(input: &Value, ctx: &ToolCtx) -> Result<ToolOutput, ToolError> {
    let rel = input.str_of("path").unwrap_or(".");
    let root = resolve(ctx, rel)?;
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let name = e.file_name().to_string_lossy().to_string();
            if matches!(
                name.as_str(),
                ".wright" | ".git" | "node_modules" | "target" | "__pycache__" | ".venv"
            ) {
                continue;
            }
            let Ok(ft) = e.file_type() else { continue };
            let relp = rel_of(ctx, &e.path());
            if ft.is_symlink() {
                out.push(format!("{relp} -> (link, not followed)"));
            } else if ft.is_dir() {
                out.push(format!("{relp}/"));
                stack.push(e.path());
            } else {
                out.push(format!(
                    "{relp} ({} bytes)",
                    e.metadata().map(|m| m.len()).unwrap_or(0)
                ));
            }
            if out.len() >= 500 {
                out.push("[... listing stopped at 500 entries; list a subdirectory ...]".into());
                return ok(out.join("\n"));
            }
        }
    }
    ok(if out.is_empty() {
        "(empty)".to_string()
    } else {
        out.join("\n")
    })
}

fn files_write(input: &Value, ctx: &ToolCtx) -> Result<ToolOutput, ToolError> {
    let rel = s(input, "path")?;
    let content = s(input, "content")?;
    let full = write_file(ctx, rel, content)?;
    Ok(ToolOutput {
        text: format!("wrote {rel} ({} bytes)", content.len()),
        wrote: vec![rel_of(ctx, &full)],
    })
}

fn edit(input: &Value, ctx: &ToolCtx) -> Result<ToolOutput, ToolError> {
    let rel = s(input, "path")?;
    let (old, new) = (s(input, "old")?, s(input, "new")?);
    if old.is_empty() {
        return Err(ToolError::BadInput("'old' must not be empty".into()));
    }
    let full = resolve(ctx, rel)?;
    let text = std::fs::read_to_string(&full)
        .map_err(|e| ToolError::Failed(format!("read {rel}: {e}")))?;
    match text.matches(old).count() {
        0 => Err(ToolError::Failed(format!("the 'old' text was not found in {rel}; read the file and copy it exactly"))),
        1 => {
            let full = write_file(ctx, rel, &text.replacen(old, new, 1))?;
            Ok(ToolOutput { text: format!("edited {rel}"), wrote: vec![rel_of(ctx, &full)] })
        }
        n => Err(ToolError::Failed(format!("the 'old' text occurs {n} times in {rel}; include more surrounding lines so it is unique"))),
    }
}

fn apply_patch(input: &Value, ctx: &ToolCtx) -> Result<ToolOutput, ToolError> {
    let fps = patch::parse(s(input, "patch")?).map_err(ToolError::BadInput)?;
    // Compute everything first; write only if every file applies.
    let mut writes: Vec<(String, Option<String>)> = Vec::new();
    for fp in &fps {
        let full = resolve(ctx, &fp.path)?;
        let old = if fp.create {
            String::new()
        } else {
            std::fs::read_to_string(&full)
                .map_err(|e| ToolError::Failed(format!("{}: {e}", fp.path)))?
        };
        if fp.create && full.exists() {
            return Err(ToolError::Failed(format!(
                "{} already exists; patch it instead of creating it",
                fp.path
            )));
        }
        if fp.delete {
            writes.push((fp.path.clone(), None));
        } else {
            writes.push((
                fp.path.clone(),
                Some(patch::apply_to(&old, fp).map_err(ToolError::Failed)?),
            ));
        }
    }
    let mut wrote = Vec::new();
    for (rel, content) in &writes {
        match content {
            Some(c) => {
                let full = write_file(ctx, rel, c)?;
                wrote.push(rel_of(ctx, &full));
            }
            None => {
                std::fs::remove_file(resolve(ctx, rel)?)
                    .map_err(|e| ToolError::Failed(format!("delete {rel}: {e}")))?;
                wrote.push(rel.clone());
            }
        }
    }
    Ok(ToolOutput {
        text: format!("patched {} file(s): {}", writes.len(), wrote.join(", ")),
        wrote,
    })
}

// ── processes ────────────────────────────────────────────────────

fn policy(permit: &Permit, ctx: &ToolCtx, timeout_secs: u64) -> sandbox::Policy {
    let mut p = sandbox::Policy::new(&ctx.project);
    p.allow_network = permit.grants().bits() & PermissionSet::NETWORK != 0;
    p.timeout = Duration::from_secs(timeout_secs.clamp(1, 900));
    p.extra_rw = ctx.sandbox_extra_rw.clone();
    p
}

fn git(permit: &Permit, input: &Value, ctx: &ToolCtx) -> Result<ToolOutput, ToolError> {
    let args: Vec<String> = input
        .get("args")
        .and_then(|a| a.as_arr())
        .ok_or_else(|| ToolError::BadInput("'args' must be an array of strings".into()))?
        .iter()
        .map(|v| {
            v.as_str()
                .map(String::from)
                .ok_or_else(|| ToolError::BadInput("every git arg must be a string".into()))
        })
        .collect::<Result<_, _>>()?;
    let sub = args.first().map(String::as_str).unwrap_or("");
    let allowed_flags: &[&str] = match sub {
        "status" => &["--short", "--porcelain", "-s"],
        "diff" => &[
            "--stat",
            "--cached",
            "--staged",
            "--name-only",
            "--no-color",
            "--",
        ],
        "log" => &[
            "--oneline",
            "--stat",
            "-n",
            "--no-color",
            "-1",
            "-5",
            "-10",
            "-20",
        ],
        "add" => &["-A", "--all", "--"],
        "commit" => &["-m", "--allow-empty"],
        _ => {
            return Err(ToolError::BadInput(
                "git subcommand must be one of: status, diff, log, add, commit".into(),
            ))
        }
    };
    let mut after_m = false;
    for a in &args[1..] {
        if after_m {
            after_m = false;
            continue;
        }
        if a.starts_with('-') {
            if !allowed_flags.contains(&a.as_str()) {
                return Err(ToolError::BadInput(format!(
                    "git flag '{a}' is not allowed (allowed for {sub}: {})",
                    allowed_flags.join(" ")
                )));
            }
            after_m = a == "-m" || a == "-n";
        } else if sub != "commit" {
            // A path argument: it must be a safe project path.
            paths::resolve(&ctx.project, a).map_err(ToolError::BadInput)?;
        }
    }
    let mut pol = policy(permit, ctx, 60);
    pol.allow_network = false; // git never needs the network here
    pol.env = vec![
        ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
        ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
        ("GIT_TERMINAL_PROMPT".into(), "0".into()),
        (
            "GIT_AUTHOR_NAME".into(),
            format!("Wright ({})", permit.agent()),
        ),
        ("GIT_AUTHOR_EMAIL".into(), "agents@wright.local".into()),
        (
            "GIT_COMMITTER_NAME".into(),
            format!("Wright ({})", permit.agent()),
        ),
        ("GIT_COMMITTER_EMAIL".into(), "agents@wright.local".into()),
    ];
    let mut full_args: Vec<&str> = vec!["-c", "safe.directory=*", "-c", "core.hooksPath=/dev/null"];
    full_args.extend(args.iter().map(String::as_str));
    let out = sandbox::run("git", &full_args, &pol).map_err(ToolError::Failed)?;
    if out.success() {
        Ok(ToolOutput {
            text: out.report(),
            wrote: if sub == "commit" || sub == "add" {
                vec![".git".into()]
            } else {
                Vec::new()
            },
        })
    } else {
        Err(ToolError::Failed(out.report()))
    }
}

fn run_tests(permit: &Permit, ctx: &ToolCtx) -> Result<ToolOutput, ToolError> {
    if ctx.test_cmd.is_empty() {
        return Err(ToolError::Failed("this project has no test command (set project.language or project.test in the blueprint)".into()));
    }
    let out = sandbox::run("/bin/sh", &["-c", &ctx.test_cmd], &policy(permit, ctx, 300))
        .map_err(ToolError::Failed)?;
    // A failing test run is a RESULT, not a tool error: the agent must see it.
    ok(format!("$ {}\n{}", ctx.test_cmd, out.report()))
}

fn shell(permit: &Permit, input: &Value, ctx: &ToolCtx) -> Result<ToolOutput, ToolError> {
    let cmd = s(input, "command")?;
    let timeout = input
        .get("timeout_secs")
        .and_then(|t| t.as_u64())
        .unwrap_or(120);
    let out = sandbox::run("/bin/sh", &["-c", cmd], &policy(permit, ctx, timeout))
        .map_err(ToolError::Failed)?;
    ok(out.report())
}

// ── network ──────────────────────────────────────────────────────

/// Refuse addresses an agent should never reach by default: loopback,
/// private ranges, link-local (cloud metadata lives at 169.254.169.254).
fn check_public(host: &str, port: u16, ctx: &ToolCtx) -> Result<(), ToolError> {
    if let Some(allowed) = &ctx.allowed_hosts {
        if !wright_runtime::policy::host_allowed(allowed, host) {
            return Err(ToolError::Failed(format!(
                "{host} is not on your organization's network allow-list ({}); the request was not sent",
                if allowed.is_empty() { "nothing is allowed".into() } else { allowed.join(", ") }
            )));
        }
    }
    if ctx.allow_private_network {
        return Ok(());
    }
    let addrs: Vec<IpAddr> = (host, port)
        .to_socket_addrs()
        .map_err(|e| ToolError::Failed(format!("cannot resolve {host}: {e}")))?
        .map(|a| a.ip())
        .collect();
    for ip in addrs {
        let private = match ip {
            IpAddr::V4(v4) => {
                v4.is_loopback()
                    || v4.is_private()
                    || v4.is_link_local()
                    || v4.is_unspecified()
                    || v4.octets()[0] == 100 && (64..128).contains(&v4.octets()[1])
            }
            IpAddr::V6(v6) => {
                v6.is_loopback()
                    || v6.is_unspecified()
                    || (v6.segments()[0] & 0xfe00) == 0xfc00
                    || (v6.segments()[0] & 0xffc0) == 0xfe80
            }
        };
        if private {
            return Err(ToolError::Failed(format!("{host} resolves to a private or local address ({ip}); agents may only reach public addresses")));
        }
    }
    Ok(())
}

const HTTP_READ_LIMIT: usize = 1024 * 1024;

fn http_read(input: &Value, ctx: &ToolCtx) -> Result<ToolOutput, ToolError> {
    let url = parse_url(s(input, "url")?).map_err(ToolError::BadInput)?;
    check_public(&url.host, url.port, ctx)?;
    let mut cfg = ctx.net.clone();
    cfg.limits.max_body = HTTP_READ_LIMIT;
    cfg.io_timeout = Duration::from_secs(30);
    let req = Request::new("GET", url.clone())
        .header("User-Agent", "Wright/0.2 (+agent harness)")
        .header("Accept", "*/*");
    let resp = net::send(&req, &cfg).map_err(ToolError::Failed)?;
    let ctype = resp
        .header("content-type")
        .unwrap_or("")
        .to_ascii_lowercase();
    let textual = ctype.is_empty()
        || ctype.starts_with("text/")
        || ctype.contains("json")
        || ctype.contains("xml")
        || ctype.contains("javascript");
    if !textual || resp.body.contains(&0) {
        return Err(ToolError::Failed(format!(
            "{url} returned {ctype}, not text"
        )));
    }
    ok(format!("HTTP {}\n{}", resp.status, resp.body_text()))
}

fn http_post(permit: &Permit, input: &Value, ctx: &ToolCtx) -> Result<ToolOutput, ToolError> {
    let url = parse_url(s(input, "url")?).map_err(ToolError::BadInput)?;
    check_public(&url.host, url.port, ctx)?;
    let body = s(input, "body")?;
    let ctype = input.str_of("content_type").unwrap_or("application/json");
    let mut req = Request::new("POST", url.clone())
        .header("Content-Type", ctype)
        .header("User-Agent", "Wright/0.2");
    if let Some(name) = input.str_of("auth_secret") {
        // Secrets are per identity: an agent resolves only what its own
        // task-scoped identity was delegated, never the blueprint's set.
        if !permit.may_use_secret(name) {
            return Err(ToolError::Failed(format!(
                "'{}' may not use the secret '{name}': its identity was not given it (add `secret \"{name}\"` to its card)",
                permit.agent()
            )));
        }
        let r = ctx.secrets.iter().find(|s| s.name == name).ok_or_else(|| {
            ToolError::BadInput(format!("'{name}' is not a secret in this blueprint"))
        })?;
        let value = secrets::resolve(&r.reference).map_err(ToolError::Failed)?;
        req = req.header("Authorization", &format!("Bearer {}", value.expose()));
    }
    let req = req.body(body.as_bytes().to_vec());
    let mut cfg = ctx.net.clone();
    cfg.io_timeout = Duration::from_secs(30);
    let resp = net::send(&req, &cfg).map_err(ToolError::Failed)?;
    let text: String = resp.body_text().chars().take(4000).collect();
    if resp.status >= 400 {
        return Err(ToolError::Failed(format!(
            "POST {url} -> HTTP {}: {text}",
            resp.status
        )));
    }
    ok(format!("HTTP {}\n{text}", resp.status))
}

/// The effective test command for a project (used by the engine).
pub fn test_cmd_for(bp: &wright_core::blueprint::Blueprint) -> String {
    convention::test_cmd(bp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wright_core::palette;
    use wright_core::wire::json;

    fn ctx(name: &str) -> ToolCtx {
        let d = std::env::temp_dir().join(format!("wright-tools-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let mut c = ToolCtx::new(&d).unwrap();
        c.outputs = vec!["out.json".into()];
        c
    }

    fn permit(tool: &str, bits: u32) -> Permit {
        Permit::for_tests("Agent", palette::find(tool).unwrap(), PermissionSet(bits))
    }

    fn call(tool: &str, input: &str, c: &ToolCtx) -> Result<ToolOutput, ToolError> {
        run(permit(tool, 0b11111), &json::parse(input).unwrap(), c)
    }

    #[test]
    fn write_output_is_limited_to_declared_names() {
        let c = ctx("output");
        let r = call("write-output", r#"{"name":"out.json","content":"{}"}"#, &c).unwrap();
        assert_eq!(r.wrote, vec!["artifacts/out.json"]);
        assert!(call("write-output", r#"{"name":"other.json","content":"x"}"#, &c).is_err());
        assert!(call("write-output", r#"{"name":"../escape","content":"x"}"#, &c).is_err());
    }

    #[test]
    fn read_write_edit_list() {
        let c = ctx("rw");
        call(
            "files-write",
            r#"{"path":"src/a.py","content":"x = 1\ny = 2\n"}"#,
            &c,
        )
        .unwrap();
        assert_eq!(
            call("files-read", r#"{"path":"src/a.py"}"#, &c)
                .unwrap()
                .text,
            "x = 1\ny = 2\n"
        );
        call(
            "edit",
            r#"{"path":"src/a.py","old":"y = 2","new":"y = 3"}"#,
            &c,
        )
        .unwrap();
        assert!(call("files-read", r#"{"path":"src/a.py"}"#, &c)
            .unwrap()
            .text
            .contains("y = 3"));
        assert!(
            call("edit", r#"{"path":"src/a.py","old":"nope","new":"x"}"#, &c)
                .unwrap_err()
                .to_string()
                .contains("not found")
        );
        assert!(call("files-list", r#"{}"#, &c)
            .unwrap()
            .text
            .contains("src/a.py"));
    }

    #[test]
    fn reads_are_paged_not_slurped() {
        let c = ctx("paged");
        let big = "a".repeat(READ_PAGE + 100);
        std::fs::write(c.project.join("big.txt"), &big).unwrap();
        let first = call("files-read", r#"{"path":"big.txt"}"#, &c)
            .unwrap()
            .text;
        assert!(first.contains(&format!("offset={READ_PAGE}")));
        let rest = call(
            "files-read",
            &format!(r#"{{"path":"big.txt","offset":{READ_PAGE}}}"#),
            &c,
        )
        .unwrap()
        .text;
        assert_eq!(rest.len(), 100);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_reads_are_refused() {
        let c = ctx("link");
        std::os::unix::fs::symlink("/etc/hostname", c.project.join("innocent.txt")).unwrap();
        assert!(call("files-read", r#"{"path":"innocent.txt"}"#, &c)
            .unwrap_err()
            .to_string()
            .contains("symbolic link"));
    }

    #[test]
    fn patches_apply_all_or_nothing() {
        let c = ctx("patch");
        call(
            "files-write",
            r#"{"path":"a.txt","content":"one\ntwo\n"}"#,
            &c,
        )
        .unwrap();
        let bad = "--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-one\n+ONE\n--- a/missing.txt\n+++ b/missing.txt\n@@ -1 +1 @@\n-x\n+y\n";
        let input = json::to_string(&Value::obj(vec![("patch", Value::str(bad))]));
        assert!(call("apply-patch", &input, &c).is_err());
        assert_eq!(
            std::fs::read_to_string(c.project.join("a.txt")).unwrap(),
            "one\ntwo\n",
            "nothing written"
        );
        let good = "--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-one\n+ONE\n";
        let input = json::to_string(&Value::obj(vec![("patch", Value::str(good))]));
        call("apply-patch", &input, &c).unwrap();
        assert_eq!(
            std::fs::read_to_string(c.project.join("a.txt")).unwrap(),
            "ONE\ntwo\n"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn git_runs_sandboxed_with_a_flag_allowlist() {
        let c = ctx("git");
        let init = sandbox::run("git", &["init", "-q"], &sandbox::Policy::new(&c.project)).unwrap();
        assert!(init.success(), "{}", init.report());
        call("files-write", r#"{"path":"a.txt","content":"x"}"#, &c).unwrap();
        call("git", r#"{"args":["add","-A"]}"#, &c).unwrap();
        let commit = call("git", r#"{"args":["commit","-m","first"]}"#, &c).unwrap();
        assert!(commit.text.contains("exit code 0"), "{}", commit.text);
        assert!(call("git", r#"{"args":["log","--oneline"]}"#, &c)
            .unwrap()
            .text
            .contains("first"));
        assert!(call("git", r#"{"args":["push"]}"#, &c).is_err());
        assert!(call("git", r#"{"args":["diff","--output=/tmp/x"]}"#, &c).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn run_tests_reports_failures_as_results() {
        let mut c = ctx("tests");
        c.test_cmd = "echo 'FAILED: 1 of 3'; exit 1".into();
        let r = call("run-tests", "{}", &c).unwrap();
        assert!(
            r.text.contains("exit code 1") && r.text.contains("FAILED: 1 of 3"),
            "{}",
            r.text
        );
    }

    #[test]
    fn private_addresses_are_refused_by_default() {
        let c = ctx("ssrf");
        for u in [
            "http://127.0.0.1:9/",
            "http://169.254.169.254/latest/meta-data/",
            "http://10.0.0.1/",
        ] {
            let input = json::to_string(&Value::obj(vec![("url", Value::str(u))]));
            let err = call("http-read", &input, &c).unwrap_err().to_string();
            assert!(err.contains("private or local"), "{u}: {err}");
        }
    }

    #[test]
    fn browser_refuses_loudly() {
        let c = ctx("browser");
        assert_eq!(
            call("browser", r#"{"url":"https://x"}"#, &c),
            Err(ToolError::NotImplemented("browser"))
        );
    }
}
