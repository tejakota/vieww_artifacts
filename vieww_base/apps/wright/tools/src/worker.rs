//! The worker tool (ADR-026): another coding agent, inside Wright's rules.
//!
//! A blueprint hands one step to an external agent — Claude Code, Codex,
//! Kimi Code, ZCode, whatever the machine has configured — and Wright
//! stays the manager: it keeps the contract, the tests and the receipt,
//! and the strongest hands do the typing.
//!
//! The isolation, in order of what actually matters:
//!
//! 1. **A copy, never the real project.** The worker sees a throwaway
//!    clone under the OS temp dir; the real project is never on its
//!    disk. Whatever it does to the copy, the worst case is a diff
//!    nobody applies.
//! 2. **No keys but its own.** The process gets a scrubbed environment
//!    (the same base the shell sandbox uses); Wright's store and the
//!    keychain are not reachable through the path it runs in. The ONE
//!    thing added is what the worker needs to sign in to its own model —
//!    the `env=` the machine's config gives that worker, resolved from
//!    the keychain at the moment it starts (e.g.
//!    `ANTHROPIC_API_KEY=keychain:wright/anthropic`).
//! 3. **A scope.** `worker-files "src/"` narrows what may change. When
//!    the diff steps outside the scope, NOTHING is applied — all or
//!    nothing, no partial takeovers.
//! 4. **A clock.** The card's timeout kills the run.
//! 5. On Linux with Landlock (kernel ≥ 5.13), the kernel sandbox wraps
//!    the run as `shell` is wrapped — with the network open, because a
//!    worker IS a model client: it talks to its vendor. That makes a
//!    worker step an outbound channel holding the project's private data
//!    and outside input at once — the Rule of Two's three legs — so a
//!    plan with a worker carries `approval "before-send"` and the gate
//!    asks before the copy is handed over. Where the kernel sandbox is
//!    unavailable, the run proceeds with 1-4 and SAYS SO in its report —
//!    the copy is the boundary the memo demands; the kernel wall is the
//!    extra one this Linux gives for free.
//!
//! The output goes through Wright's tests before anything ships: the
//! worker's diff lands in the workspace, and the Testwright and Reviewer
//! that close every firing judge it like any other work.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use wright_core::wire::json::Value;
use wright_runtime::config::WorkerEntry;
use wright_runtime::gate::Permit;
use wright_runtime::store::write_atomic;

use crate::paths;
use crate::sandbox;
use crate::{ToolCtx, ToolError, ToolOutput};

/// The largest project copy, in bytes, before the worker refuses.
/// (A worker on a 2 GB tree needs a different plan, not a bigger copy.)
const COPY_LIMIT: u64 = 256 * 1024 * 1024;

/// Directories never copied: they are the person's own machine state or
/// run state, not the project.
const SKIP: [&str; 3] = ["logs", ".wright", ".git"];

/// One change the worker made, relative to the project root.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Change {
    Edited(String),
    Added(String),
    Deleted(String),
}

/// Run the worker the permit names. `input.prompt` is the full
/// instruction: Wright adds scope and rules around it, so a blueprint's
/// goal sentence is all the model-side needs.
pub fn run(permit: &Permit, input: &Value, ctx: &ToolCtx) -> Result<ToolOutput, ToolError> {
    let agent = permit.agent().to_string();
    let Some(entry) = &ctx.worker else {
        return Err(ToolError::Failed(format!(
            "'{agent}' names the worker '{}', but no worker with that name is configured on this machine. Add a `worker \"...\"` line to config.kdl (ADR-026).",
            ctx.worker_name.clone().unwrap_or_default()
        )));
    };
    let prompt = input
        .str_of("prompt")
        .ok_or_else(|| ToolError::BadInput("the worker tool needs a prompt string".into()))?
        .to_string();

    // Everything that can fail before anything is copied, fails first:
    // the program must exist, and the keys it signs in with must be there.
    let program = resolve_command(&entry.command).map_err(|e| {
        ToolError::Failed(format!(
            "the worker '{}' runs `{}`, which {e}. Install it, or point `worker \"{}\" command=\"...\"` in config.kdl at it.",
            entry.name, entry.command, entry.name
        ))
    })?;
    let worker_env = resolve_env(entry)?;

    // 1. The copy: private to this user (0700) and removed afterwards,
    // whatever happens — it holds the whole project.
    let copy_dir = std::env::temp_dir().join(format!(
        "wright-worker-{}-{}",
        std::process::id(),
        now_nanos()
    ));
    private_dir(&copy_dir).map_err(ToolError::Failed)?;
    let _cleanup = RemoveOnDrop(copy_dir.clone());
    copy_tree(&ctx.project, &copy_dir).map_err(ToolError::Failed)?;

    // The worker's brief: the prompt, the scope, and the rules of the
    // house, written where the worker starts.
    let brief = format!(
        "# {agent}: do this work\n\n{prompt}\n\n\
         You are running inside a THROWAWAY COPY of a Wright project. Wright will diff this copy \
         against the original and apply your changes whole or not at all. You may change {}. \
         Do not commit, do not create lock files, do not touch anything outside this copy.\n",
        if ctx.worker_files.is_empty() {
            "the whole copy".to_string()
        } else {
            format!("only: {}", ctx.worker_files.join(", "))
        }
    );
    std::fs::write(copy_dir.join("WORKER.md"), &brief)
        .map_err(|e| ToolError::Failed(format!("write WORKER.md: {e}")))?;

    // 2. The run: kernel sandbox when the kernel has one, else the
    // honest degraded path (still copy + scrubbed env + clock).
    let timeout = Duration::from_secs(ctx.worker_timeout_secs.max(1));
    let started = Instant::now();
    let (stdout, isolation) =
        run_external(entry, &program, &worker_env, &prompt, &copy_dir, timeout)?;
    let took = started.elapsed();

    // 3. The diff: copy vs original.
    let changes = diff_trees(&ctx.project, &copy_dir).map_err(ToolError::Failed)?;
    if changes.is_empty() {
        return Ok(ToolOutput {
            text: format!(
                "the worker {} ran for {:.1}s and changed nothing. Its last words: {}",
                entry.name,
                took.as_secs_f64(),
                tail(&stdout, 400)
            ),
            wrote: Vec::new(),
        });
    }

    // 4. The scope: every change inside `worker_files`, or nothing.
    if !ctx.worker_files.is_empty() {
        let mut outside: Vec<String> = Vec::new();
        for c in &changes {
            let path = match c {
                Change::Edited(p) | Change::Added(p) | Change::Deleted(p) => p,
            };
            if !in_scope(&ctx.worker_files, path) {
                outside.push(path.clone());
            }
        }
        if !outside.is_empty() {
            return Err(ToolError::Failed(format!(
                "the worker stepped outside the scope the card gave it: it changed {} (allowed: {}). \
                 Nothing was applied — all or nothing, no partial takeovers (ADR-026).",
                outside.join(", "),
                ctx.worker_files.join(", ")
            )));
        }
    }

    // 5. Apply: whole or not at all (this loop only runs when everything
    // is in scope; each file lands atomically).
    let mut applied: Vec<String> = Vec::new();
    for c in &changes {
        match c {
            Change::Edited(rel) | Change::Added(rel) => {
                // Read back only a regular file: something the worker left
                // running could have swapped it for a link since the diff,
                // and a link would read a file from outside the copy.
                let src = copy_dir.join(rel);
                match std::fs::symlink_metadata(&src) {
                    Ok(m) if m.file_type().is_file() => {}
                    _ => {
                        return Err(ToolError::Failed(format!(
                            "{rel} changed into something other than a file while the worker's changes were being applied. Nothing more was applied."
                        )))
                    }
                }
                let bytes = std::fs::read(&src)
                    .map_err(|e| ToolError::Failed(format!("read back {rel}: {e}")))?;
                let full = ctx.project.join(rel);
                if let Some(parent) = full.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                write_atomic(&full, &bytes)
                    .map_err(|e| ToolError::Failed(format!("apply {rel}: {e}")))?;
                applied.push(rel.clone());
            }
            Change::Deleted(rel) => {
                let _ = std::fs::remove_file(ctx.project.join(rel));
                applied.push(format!("-{rel}"));
            }
        }
    }

    let listed: Vec<String> = applied
        .iter()
        .take(6)
        .map(|p| p.trim_start_matches('-').to_string())
        .collect();
    let report = format!(
        "{} ran for {:.1}s ({}): {} file(s) applied{}\nLast words: {}",
        entry.name,
        took.as_secs_f64(),
        isolation,
        applied.len(),
        if applied.len() > 6 {
            format!(" (starting with {})", listed.join(", "))
        } else {
            format!(": {}", listed.join(", "))
        },
        tail(&stdout, 400),
    );
    Ok(ToolOutput {
        text: report,
        wrote: applied,
    })
}

/// Run the external agent. The prompt is the LAST argument — the same
/// convention `claude --print`, `codex exec` and friends expect — so
/// both isolation paths feed it identically. Kernel sandbox first when
/// the kernel has one (Landlock: the same wall `shell` runs behind);
/// without one, the honest degraded path: copy + scrubbed env + clock,
/// and the report says which isolation ran.
fn run_external(
    entry: &WorkerEntry,
    program: &Path,
    worker_env: &[(String, String)],
    prompt: &str,
    copy_dir: &Path,
    timeout: Duration,
) -> Result<(String, &'static str), ToolError> {
    let mut args: Vec<String> = entry.args.clone();
    args.push(prompt.to_string());
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let program_str = program.to_string_lossy().to_string();

    let sandbox_available = sandbox::capabilities().is_ok();
    // The Windows WSL backend deliberately runs POSIX tools, not Windows PE
    // executables. Passing a native .exe path into Bubblewrap would fail (or,
    // worse, tempt a future change to execute it outside the namespace). Keep
    // native workers on the existing workspace-copy fallback and report that
    // it has no OS-level sandbox; shell, git and run-tests use WSL isolation.
    let can_sandbox_worker = sandbox_available && !cfg!(windows);
    if can_sandbox_worker {
        let mut policy = sandbox::Policy::new(copy_dir);
        policy.timeout = timeout;
        // A worker is a model client: it must reach its vendor.
        policy.allow_network = true;
        // The program itself may live outside the toolchain roots (a
        // user-local npm or pipx install): its directory, and the
        // directory of whatever its link points at, become readable
        // and executable — nothing else of the home directory does.
        policy.extra_ro.extend(program_dirs(program));
        policy.env = worker_env.to_vec();
        match sandbox::run(&program_str, &arg_refs, &policy) {
            Ok(out) => {
                if out.success() && !out.timed_out {
                    return Ok((out.output, "kernel sandbox"));
                }
                if out.timed_out && wright_runtime::killswitch::is_stopped() {
                    return Err(ToolError::Failed(format!(
                        "the worker '{}' was stopped by the kill switch. Nothing was applied.",
                        entry.name
                    )));
                }
                if out.timed_out {
                    return Err(ToolError::Failed(format!(
                        "the worker '{}' ran past its {}s timeout and was killed. Nothing was applied.",
                        entry.name,
                        timeout.as_secs()
                    )));
                }
                return Err(worker_failed(entry, &out.output));
            }
            Err(e) => {
                return Err(ToolError::Failed(format!(
                    "the sandbox refused to run '{}': {e}",
                    entry.command
                )))
            }
        }
    }

    // No kernel sandbox on this machine: the copy is the boundary (the
    // worker only ever sees the clone), plus a scrubbed environment and
    // the card's clock. The report names the isolation honestly.
    let out = direct_run(entry, program, worker_env, &arg_refs, copy_dir, timeout)?;
    let isolation = if cfg!(windows) && sandbox_available {
        "native Windows worker (workspace copy only; no OS sandbox; WSL isolates POSIX tools)"
    } else {
        "workspace copy only (no OS sandbox on this machine)"
    };
    Ok((out, isolation))
}

/// Find the program a worker runs: a path as given, or the first match
/// on this process's PATH (with the Windows executable extensions).
pub fn resolve_command(command: &str) -> Result<PathBuf, String> {
    let as_path = Path::new(command);
    if command.contains('/') || command.contains('\\') {
        return if as_path.is_file() {
            Ok(as_path.to_path_buf())
        } else {
            Err(format!("does not exist at {}", as_path.display()))
        };
    }
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".into())
            .split(';')
            .map(|e| e.to_string())
            .chain(std::iter::once(String::new()))
            .collect()
    } else {
        vec![String::new()]
    };
    let path = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        for ext in &exts {
            let candidate = dir.join(format!("{command}{ext}"));
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err("is not installed on this machine (not found on PATH)".into())
}

/// The directories a program needs to be readable for it to start: its
/// own, and — when it is a link (npm and pipx install links) — the
/// directory the link resolves into and that directory's parent (the
/// package root, where an npm package keeps its bundled files).
fn program_dirs(program: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(d) = program.parent() {
        dirs.push(d.to_path_buf());
    }
    if let Ok(real) = std::fs::canonicalize(program) {
        if let Some(d) = real.parent() {
            dirs.push(d.to_path_buf());
            if let Some(pkg) = d.parent() {
                if pkg.parent().is_some() && pkg != Path::new("/") {
                    dirs.push(pkg.to_path_buf());
                }
            }
        }
    }
    dirs.sort();
    dirs.dedup();
    dirs
}

/// The worker's own sign-in environment: `keychain:` values resolved now,
/// plain values as written. A missing key stops the step with the
/// command that fixes it.
fn resolve_env(entry: &WorkerEntry) -> Result<Vec<(String, String)>, ToolError> {
    let mut out = Vec::new();
    for (k, v) in &entry.env {
        if v.starts_with("keychain:") {
            let secret = wright_runtime::secrets::resolve(v).map_err(|e| {
                ToolError::Failed(format!(
                    "the worker '{}' signs in with {k}, which is not set up: {e}",
                    entry.name
                ))
            })?;
            out.push((k.clone(), secret.expose().to_string()));
        } else {
            out.push((k.clone(), v.clone()));
        }
    }
    Ok(out)
}

/// A directory only this user can enter.
fn private_dir(dir: &Path) -> Result<(), String> {
    #[cfg_attr(
        not(unix),
        expect(
            unused_mut,
            reason = "only the unix mode call below mutates the builder"
        )
    )]
    let mut b = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    b.create(dir).map_err(|e| format!("{}: {e}", dir.display()))
}

/// Deletes the worker's copy when the step is over, on every path out.
struct RemoveOnDrop(PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The degraded path: direct process, scrubbed environment, hard clock.
/// No inherited environment, so no keys reach the worker but its own.
fn direct_run(
    entry: &WorkerEntry,
    program: &Path,
    worker_env: &[(String, String)],
    args: &[&str],
    copy_dir: &Path,
    timeout: Duration,
) -> Result<String, ToolError> {
    use std::process::{Command, Stdio};
    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(copy_dir)
        .env_clear()
        // PATH is not a secret, and a worker started by node or python
        // needs the interpreter it was installed with.
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", copy_dir)
        .env("TMPDIR", copy_dir)
        .envs(worker_env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        // Windows programs need these to start at all; neither is secret.
        for k in [
            "SystemRoot",
            "PATHEXT",
            "COMSPEC",
            "USERPROFILE",
            "APPDATA",
            "LOCALAPPDATA",
        ] {
            if let Some(v) = std::env::var_os(k) {
                cmd.env(k, v);
            }
        }
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().map_err(|e| {
        ToolError::Failed(format!(
            "run '{}' for the worker '{}': {e}",
            entry.command, entry.name
        ))
    })?;
    // Drain both pipes while the worker runs: a chatty agent writes far
    // more than a pipe holds, and a pipe nobody reads stalls the writer
    // until the clock kills it.
    let drain = |pipe: Option<Box<dyn std::io::Read + Send>>| {
        std::thread::spawn(move || {
            let mut keep: Vec<u8> = Vec::new();
            if let Some(mut r) = pipe {
                let mut buf = [0u8; 8192];
                while let Ok(n) = r.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    keep.extend_from_slice(&buf[..n]);
                    // Only the tail is ever shown; keep memory bounded.
                    if keep.len() > 256 * 1024 {
                        keep.drain(..keep.len() - 128 * 1024);
                    }
                }
            }
            String::from_utf8_lossy(&keep).into_owned()
        })
    };
    let out_thread = drain(
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
    );
    let err_thread = drain(
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
    );
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // Anything the worker left running in the background goes
                // too, before its files are read back.
                kill_group(&mut child);
                let output = out_thread.join().unwrap_or_default();
                let err = err_thread.join().unwrap_or_default();
                if status.success() {
                    return Ok(format!("{output}{err}"));
                }
                return Err(worker_failed(entry, &format!("{output}{err}")));
            }
            Ok(None) => {
                if wright_runtime::killswitch::is_stopped() {
                    kill_group(&mut child);
                    let _ = child.wait();
                    return Err(ToolError::Failed(format!(
                        "the worker '{}' was stopped by the kill switch. Nothing was applied.",
                        entry.name
                    )));
                }
                if start.elapsed() > timeout {
                    kill_group(&mut child);
                    let _ = child.wait();
                    return Err(ToolError::Failed(format!(
                        "the worker '{}' ran past its {}s timeout and was killed. Nothing was applied.",
                        entry.name,
                        timeout.as_secs()
                    )));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(ToolError::Failed(format!("wait for the worker: {e}"))),
        }
    }
}

/// Stop the worker and everything it started. On Unix the worker leads
/// its own process group, so the group goes (through the system `kill`:
/// this file stays free of unsafe); elsewhere the process itself.
fn kill_group(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("kill")
            .args(["-KILL", &format!("-{}", child.id())])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    let _ = child.kill();
}

fn worker_failed(entry: &WorkerEntry, output: &str) -> ToolError {
    ToolError::Failed(format!(
        "the worker '{}' exited with an error. Nothing was applied.\nIts last words: {}",
        entry.name,
        tail(output, 600)
    ))
}

/// True when `rel` (project-relative, `/`-separated) is inside one of the
/// card's `worker-files` scopes. A scope is a directory (trailing slash
/// optional) or an exact file.
fn in_scope(scopes: &[String], rel: &str) -> bool {
    scopes.iter().any(|s| {
        let s = s.trim_matches('/');
        if s.is_empty() {
            return true;
        }
        rel == s || rel.starts_with(&format!("{s}/"))
    })
}

/// Copy the project into a fresh temp dir. Skips run state, follows the
/// same file set the workspace hash sees (no symlinks out of the tree).
fn copy_tree(project: &Path, to: &Path) -> Result<u64, String> {
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    let mut total = 0u64;
    let mut stack = vec![project.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .flatten()
        {
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_symlink() {
                continue; // never follow links out of the project
            }
            if ft.is_dir() {
                if dir == project && SKIP.contains(&name.as_str()) {
                    continue;
                }
                let rel = path.strip_prefix(project).unwrap();
                let dest = to.join(rel);
                std::fs::create_dir_all(&dest).map_err(|e| format!("{}: {e}", dest.display()))?;
                stack.push(path);
            } else {
                let rel = path.strip_prefix(project).unwrap();
                let meta = entry
                    .metadata()
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                total += meta.len();
                if total > COPY_LIMIT {
                    let _ = std::fs::remove_dir_all(to);
                    return Err(format!(
                        "the project is too large to hand to a worker in one copy (over {} MB). Split the step or shrink the tree.",
                        COPY_LIMIT / (1024 * 1024)
                    ));
                }
                let dest = to.join(rel);
                if let Some(parent) = dest.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                std::fs::copy(&path, &dest).map_err(|e| format!("copy {}: {e}", rel.display()))?;
            }
        }
    }
    Ok(total)
}

/// What changed between the original and the copy: edits, additions,
/// deletions, as project-relative paths. WORKER.md is Wright's own file
/// and never counts.
fn diff_trees(project: &Path, copy: &Path) -> Result<Vec<Change>, String> {
    let mut out = Vec::new();
    let mut orig_files: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    collect_rel(project, project, &mut orig_files)?;
    let mut copy_files: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    collect_rel(copy, copy, &mut copy_files)?;

    for rel in &orig_files {
        if rel == "WORKER.md" {
            continue;
        }
        if !copy_files.contains(rel) {
            out.push(Change::Deleted(rel.clone()));
            continue;
        }
        let a = std::fs::read(project.join(rel)).unwrap_or_default();
        let b = std::fs::read(copy.join(rel)).unwrap_or_default();
        if a != b {
            out.push(Change::Edited(rel.clone()));
        }
    }
    for rel in &copy_files {
        if rel == "WORKER.md" {
            continue;
        }
        if !orig_files.contains(rel) {
            out.push(Change::Added(rel.clone()));
        }
    }
    Ok(out)
}

fn collect_rel(
    root: &Path,
    dir: &Path,
    out: &mut std::collections::BTreeSet<String>,
) -> Result<(), String> {
    for entry in std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            if dir == root && SKIP.contains(&name.as_str()) {
                continue;
            }
            collect_rel(root, &path, out)?;
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.insert(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

fn tail(s: &str, n: usize) -> String {
    let clean: String = s
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .collect();
    if clean.chars().count() <= n {
        clean.trim().to_string()
    } else {
        format!(
            "…{}",
            clean
                .chars()
                .skip(clean.chars().count() - n)
                .collect::<String>()
        )
        .trim()
        .to_string()
    }
}

fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

/// Path helper reuse: the worker never touches paths outside the copy.
#[allow(dead_code)]
fn guard_path(project: &Path, rel: &str) -> Result<PathBuf, ToolError> {
    paths::resolve(project, rel).map_err(ToolError::Failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolCtx;
    use wright_runtime::config::WorkerEntry;
    use wright_runtime::gate::Permit;

    fn tmp_project(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("wright-worker-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("src")).unwrap();
        d
    }

    /// A worker stub: a shell script that does exactly what the test
    /// wants, driven by the prompt on stdin (the real workers read the
    /// prompt the same way).
    fn stub(dir: &Path, script: &str) -> WorkerEntry {
        // The stub lives OUTSIDE the project: the worker copy must not
        // contain the test's own machinery, and the project must not
        // gain files it never asked for.
        let p = dir.parent().unwrap_or(&std::env::temp_dir()).join(format!(
            "wright-worker-stub-{}-{}.sh",
            std::process::id(),
            now_nanos()
        ));
        std::fs::write(&p, format!("#!/bin/sh\n{script}\n")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        WorkerEntry {
            name: "stub-agent".into(),
            command: p.to_string_lossy().to_string(),
            args: Vec::new(),
            timeout_secs: 30,
            env: Vec::new(),
        }
    }

    fn ctx_for(project: &Path, entry: WorkerEntry, files: &[&str]) -> ToolCtx {
        let mut ctx = ToolCtx::new(project).unwrap();
        ctx.worker = Some(entry);
        ctx.worker_files = files.iter().map(|s| s.to_string()).collect();
        ctx.worker_timeout_secs = 30;
        ctx
    }

    fn permit_for(project: &Path, agent: &str) -> Permit {
        let _ = project;
        let spec = wright_core::palette::find("worker").expect("worker is on the palette");
        let grants = wright_core::blueprint::PermissionSet::none()
            .with(wright_core::blueprint::PermissionSet::READ_FILES)
            .with(wright_core::blueprint::PermissionSet::WRITE_FILES)
            .with(wright_core::blueprint::PermissionSet::SPAWN);
        Permit::for_tests(agent, spec, grants)
    }

    fn input(prompt: &str) -> Value {
        let v = wright_core::wire::json::parse(&format!(
            "{{\"prompt\":\"{}\"}}",
            prompt.replace('\\', "\\\\").replace('"', "\\\"")
        ))
        .unwrap();
        v
    }

    #[test]
    fn a_worker_that_writes_a_file_gets_it_applied() {
        let project = tmp_project("apply");
        std::fs::write(project.join("src").join("main.py"), "print('old')\n").unwrap();
        let _entry = stub(&project, "echo done");
        // Simpler: the stub edits the existing file directly.
        let entry = stub(
            &project,
            "echo \"print('new')\" > src/main.py; echo worker-done; echo got: $1 >> /dev/null",
        );
        let ctx = ctx_for(&project, entry, &[]);
        let permit = permit_for(&project, "W");
        let out = run(&permit, &input("make it print new"), &ctx).unwrap();
        assert!(
            out.text.contains("worker-done"),
            "the report carries the worker's last words: {}",
            out.text
        );
        assert!(
            out.text.contains("kernel sandbox") || out.text.contains("workspace copy"),
            "the isolation is named: {}",
            out.text
        );
        assert_eq!(
            std::fs::read_to_string(project.join("src").join("main.py")).unwrap(),
            "print('new')\n"
        );
        assert_eq!(out.wrote, vec!["src/main.py".to_string()]);
    }

    #[test]
    fn a_change_outside_the_worker_scope_applies_nothing() {
        let project = tmp_project("scope");
        std::fs::write(project.join("src").join("a.py"), "one\n").unwrap();
        std::fs::write(project.join("secret.txt"), "keep me\n").unwrap();
        let entry = stub(&project, "echo one > src/a.py; echo touched > secret.txt");
        let ctx = ctx_for(&project, entry, &["src/"]);
        let permit = permit_for(&project, "W");
        let err = run(&permit, &input("fix both files"), &ctx).unwrap_err();
        assert!(err.to_string().contains("outside the scope"), "{}", err);
        assert!(err.to_string().contains("Nothing was applied"), "{}", err);
        // BOTH changes were rolled back — all or nothing.
        assert_eq!(
            std::fs::read_to_string(project.join("src").join("a.py")).unwrap(),
            "one\n"
        );
        assert_eq!(
            std::fs::read_to_string(project.join("secret.txt")).unwrap(),
            "keep me\n"
        );
    }

    #[test]
    fn deletions_in_scope_are_applied_and_reported() {
        let project = tmp_project("delete");
        std::fs::write(project.join("old.txt"), "bye\n").unwrap();
        let entry = stub(&project, "rm old.txt");
        let ctx = ctx_for(&project, entry, &[]);
        let permit = permit_for(&project, "W");
        let out = run(&permit, &input("remove the file"), &ctx).unwrap();
        assert!(
            !project.join("old.txt").exists(),
            "the deletion was applied"
        );
        assert!(
            out.wrote.iter().any(|w| w == "-old.txt"),
            "the report names the deletion: {:?}",
            out.wrote
        );
    }

    #[test]
    fn a_chatty_worker_is_not_stalled_by_its_own_output() {
        // Far more than a pipe holds (64 KiB): before the pipes were
        // drained while the worker ran, this hung until the clock.
        let project = tmp_project("chatty");
        let entry = stub(&project, "i=0; while [ $i -lt 4000 ]; do echo 'a long line of worker chatter, the kind agents print'; i=$((i+1)); done; echo done > out.txt");
        let started = std::time::Instant::now();
        let out = direct_run(
            &entry,
            Path::new(&entry.command),
            &[],
            &["go"],
            &project,
            Duration::from_secs(20),
        )
        .unwrap();
        assert!(
            started.elapsed().as_secs() < 10,
            "the worker stalled on a full pipe"
        );
        assert!(out.contains("chatter"));
        assert!(project.join("out.txt").exists());
    }

    #[test]
    fn the_worker_gets_its_own_sign_in_and_nothing_else() {
        let project = tmp_project("env");
        let mut entry = stub(
            &project,
            "echo \"K=$WORKER_TEST_TOKEN H=$WRIGHT_HOME\" > env.txt",
        );
        entry.env = vec![("WORKER_TEST_TOKEN".into(), "plain-for-the-test".into())];
        let env = resolve_env(&entry).unwrap();
        direct_run(
            &entry,
            Path::new(&entry.command),
            &env,
            &["go"],
            &project,
            Duration::from_secs(20),
        )
        .unwrap();
        let seen = std::fs::read_to_string(project.join("env.txt")).unwrap();
        assert!(seen.contains("K=plain-for-the-test"), "{seen}");
        assert!(
            seen.contains("H=\n") || seen.trim_end().ends_with("H="),
            "Wright's own environment must not leak: {seen}"
        );
    }

    #[test]
    fn a_missing_program_is_named_before_anything_is_copied() {
        assert!(resolve_command("sh").is_ok(), "sh is on PATH");
        let err = resolve_command("wright-no-such-agent-binary").unwrap_err();
        assert!(err.contains("not installed"), "{err}");
        let err = resolve_command("/no/such/dir/agent").unwrap_err();
        assert!(err.contains("does not exist"), "{err}");
    }

    #[test]
    fn a_slow_worker_is_killed_by_its_clock() {
        let project = tmp_project("timeout");
        let mut entry = stub(&project, "sleep 30");
        entry.timeout_secs = 1;
        let mut ctx = ctx_for(&project, entry, &[]);
        ctx.worker_timeout_secs = 1;
        let permit = permit_for(&project, "W");
        let started = std::time::Instant::now();
        let err = run(&permit, &input("take forever"), &ctx).unwrap_err();
        assert!(err.to_string().contains("timeout"), "{}", err);
        assert!(
            started.elapsed().as_secs() < 10,
            "the kill must not wait the full 30s"
        );
        assert_eq!(
            std::fs::read_dir(&project).unwrap().count(),
            1,
            "only src/ remains: nothing was applied"
        );
    }

    #[test]
    fn a_failing_worker_changes_nothing() {
        let project = tmp_project("fail");
        std::fs::write(project.join("x.txt"), "original\n").unwrap();
        let entry = stub(&project, "echo changed > x.txt; exit 3");
        let ctx = ctx_for(&project, entry, &[]);
        let permit = permit_for(&project, "W");
        let err = run(&permit, &input("fail loudly"), &ctx).unwrap_err();
        assert!(err.to_string().contains("exited with an error"), "{}", err);
        assert!(err.to_string().contains("Nothing was applied"), "{}", err);
        assert_eq!(
            std::fs::read_to_string(project.join("x.txt")).unwrap(),
            "original\n"
        );
    }

    #[test]
    fn the_real_project_never_gains_the_worker_copy() {
        let project = tmp_project("copy-clean");
        std::fs::write(project.join("a.txt"), "a\n").unwrap();
        let entry = stub(&project, "true");
        let ctx = ctx_for(&project, entry, &[]);
        let permit = permit_for(&project, "W");
        run(&permit, &input("do nothing"), &ctx).unwrap();
        assert!(
            !project.join("WORKER.md").exists(),
            "the brief never leaks into the project"
        );
        assert!(
            std::fs::read_dir(&project).unwrap().count() <= 2,
            "no stray files: {:?}",
            std::fs::read_dir(&project).unwrap().count()
        );
    }

    #[test]
    fn scope_matching_treats_dirs_and_files_fairly() {
        assert!(in_scope(&["src".to_string()], "src/main.py"));
        assert!(in_scope(&["src/".to_string()], "src/deep/nested/x.py"));
        assert!(in_scope(&["notes.md".to_string()], "notes.md"));
        assert!(!in_scope(&["src/".to_string()], "srcx.py"));
        assert!(!in_scope(&["src/".to_string()], "secret.txt"));
        assert!(in_scope(&["".to_string()], "anything/goes"));
    }
}
