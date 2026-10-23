//! Windows command isolation through WSL 2 + Bubblewrap.
//!
//! Wright never falls back to an unrestricted Windows process. On Windows,
//! shell/git/run-tests execute inside a Linux WSL 2 distribution, with
//! Bubblewrap mounting the project as the only writable source tree, hiding
//! Windows drives and user homes, scrubbing environment variables, isolating
//! the network unless granted, and applying process/file-size limits. If the
//! required isolation runtime is missing, the command is refused with setup
//! instructions rather than executed unsandboxed.

use super::{Policy, RunOutput, MAX_OUTPUT};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;
use std::thread;

use std::os::windows::process::CommandExt;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const SETUP_HINT: &str = "Wright needs an isolated WSL 2 runtime on Windows. Enable WSL 2 and install an Ubuntu distribution, then run `sudo apt update && sudo apt install bubblewrap util-linux coreutils` inside it. Run `wright doctor` again. Wright will not run agent commands without isolation.";
const PATH_HINT: &str = "The path could not be mapped into WSL. Keep the project on a local Windows drive (for example C:\\work\\project), ensure that drive is mounted in WSL, and retry.";

fn wsl() -> Command {
    let mut command = Command::new("wsl.exe");
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

fn output_text(output: &Output) -> String {
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.stderr.is_empty() {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    text.trim().to_string()
}

fn clipped(s: &str) -> String {
    if s.len() <= 1200 {
        return s.to_string();
    }
    let mut start = s.len() - 1200;
    while !s.is_char_boundary(start) {
        start += 1;
    }
    format!("[earlier output omitted]\n{}", &s[start..])
}

fn linux_command(args: &[&str]) -> Result<Output, String> {
    wsl()
        .args(args)
        .output()
        .map_err(|e| format!("could not start WSL (`wsl.exe`): {e}. {SETUP_HINT}"))
}

fn linux_dir_exists(path: &str) -> bool {
    linux_command(&["--exec", "test", "-d", path])
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn linux_home() -> Result<String, String> {
    let output = linux_command(&["--exec", "sh", "-c", "printf '%s' \"$HOME\""])?;
    let home = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() || !home.starts_with('/') || home.contains('\n') {
        return Err(format!(
            "could not determine the default WSL user's home directory. {}",
            SETUP_HINT
        ));
    }
    Ok(home)
}

fn probe_runtime() -> Result<String, String> {
    // WSL startup can be comparatively expensive. Probe namespaces once per
    // Wright process; every actual command still launches Bubblewrap with the
    // full policy, so a stale positive result cannot run a command unsandboxed.
    static PROBE: OnceLock<Result<String, String>> = OnceLock::new();
    PROBE.get_or_init(probe_runtime_uncached).clone()
}

fn probe_runtime_uncached() -> Result<String, String> {
    let output = linux_command(&[
        "--exec",
        "sh",
        "-c",
        "case \"$(uname -r)\" in *WSL2*|*microsoft-standard*) ;; *) echo 'the default distribution is not running on a WSL 2 kernel'; exit 41;; esac; command -v bwrap >/dev/null || { echo 'bubblewrap (bwrap) is missing'; exit 42; }; command -v timeout >/dev/null || { echo 'GNU timeout is missing'; exit 43; }; command -v prlimit >/dev/null || { echo 'prlimit is missing'; exit 44; }",
    ])?;
    if !output.status.success() {
        let detail = output_text(&output);
        return Err(if detail.is_empty() {
            SETUP_HINT.to_string()
        } else {
            format!("{detail}. {SETUP_HINT}")
        });
    }

    // Exercise the exact user/pid/network namespace operation that actual
    // commands require. A present executable is not proof namespaces work.
    let probe = linux_command(&[
        "--exec",
        "bwrap",
        "--die-with-parent",
        "--unshare-user",
        "--unshare-pid",
        "--unshare-ipc",
        "--unshare-uts",
        "--unshare-net",
        "--ro-bind",
        "/",
        "/",
        "--dev",
        "/dev",
        "--proc",
        "/proc",
        "--",
        "/bin/true",
    ])?;
    if !probe.status.success() {
        let detail = output_text(&probe);
        return Err(if detail.is_empty() {
            format!("Bubblewrap cannot create the required namespaces. {SETUP_HINT}")
        } else {
            format!(
                "Bubblewrap isolation probe failed: {}. {}",
                clipped(&detail),
                SETUP_HINT
            )
        });
    }
    Ok("WSL 2 + Bubblewrap (isolates Linux shell/git/test commands; native Windows worker CLIs remain a separate, explicitly reported fallback)".into())
}

/// Validate the Windows path in the host, then translate it with WSL's own
/// path mapper. We never interpolate host paths into a shell command.
fn wsl_path(path: &Path) -> Result<String, String> {
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("cannot resolve {}: {e}", path.display()))?;
    let raw = canonical.to_string_lossy();
    let raw = raw.strip_prefix(r"\\?\").unwrap_or(raw.as_ref());
    if raw.starts_with(r"UNC\") || raw.starts_with(r"\\") {
        return Err(format!("{}: {PATH_HINT}", path.display()));
    }
    let output = linux_command(&["--exec", "wslpath", "-a", "-u", raw])?;
    if !output.status.success() {
        let detail = output_text(&output);
        return Err(if detail.is_empty() {
            format!("{}: {PATH_HINT}", path.display())
        } else {
            format!("{}: {}. {PATH_HINT}", path.display(), clipped(&detail))
        });
    }
    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !result.starts_with('/') || result.contains('\n') || result.contains('\0') {
        return Err(format!("{}: {PATH_HINT}", path.display()));
    }
    Ok(result)
}

fn bind_path_args(
    args: &mut Vec<String>,
    path: &Path,
    target: &str,
    writable: bool,
) -> Result<(), String> {
    let linux_path = wsl_path(path)?;
    args.push("--dir".into());
    args.push(target.into());
    args.push(if writable { "--bind" } else { "--ro-bind" }.into());
    args.push(linux_path);
    args.push(target.into());
    Ok(())
}

fn add_existing_read_only_mount(args: &mut Vec<String>, home: &str, suffix: &str, target: &str) {
    let source = format!("{home}/{suffix}");
    if !linux_dir_exists(&source) {
        return;
    }
    args.push("--dir".into());
    args.push(target.into());
    args.push("--ro-bind".into());
    args.push(source);
    args.push(target.into());
}

fn add_existing_cache_mount(args: &mut Vec<String>, home: &str, suffix: &str, target: &str) {
    let source = format!("{home}/{suffix}");
    if !linux_dir_exists(&source) {
        return;
    }
    args.push("--dir".into());
    args.push(target.into());
    // Only the non-secret package cache subtrees are exposed, not ~/.cargo.
    args.push("--bind".into());
    args.push(source);
    args.push(target.into());
}

fn add_host_cache_mount(
    args: &mut Vec<String>,
    cargo_home: &Path,
    suffix: &str,
    target: &str,
) -> Result<(), String> {
    let source = cargo_home.join(suffix);
    if !source.is_dir() {
        return Ok(());
    }
    let source = wsl_path(&source)?;
    args.push("--dir".into());
    args.push(target.into());
    // Only the registry and git caches are passed here; never mount the
    // parent CARGO_HOME, where credentials.toml and config.toml may live.
    // These cache directories are intentionally writable so Cargo can create
    // its lock/metadata files while still being isolated from all other host
    // state.
    args.push("--bind".into());
    args.push(source);
    args.push(target.into());
    Ok(())
}

fn read_tail<R: Read>(mut reader: R, limit: usize) -> (Vec<u8>, bool) {
    let mut keep = Vec::new();
    let mut truncated = false;
    let mut buf = [0u8; 8192];
    loop {
        match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                keep.extend_from_slice(&buf[..n]);
                if keep.len() > limit {
                    let cut = keep.len() - limit;
                    keep.drain(..cut);
                    truncated = true;
                }
            }
        }
    }
    (keep, truncated)
}

pub fn capabilities() -> Result<String, String> {
    probe_runtime()
}

pub fn run(program: &str, args: &[&str], policy: &Policy) -> Result<RunOutput, String> {
    if program.is_empty() || program.contains('\0') || args.iter().any(|a| a.contains('\0')) {
        return Err("command contains an empty program or NUL byte".into());
    }
    probe_runtime()?;

    let workdir = policy
        .workdir
        .canonicalize()
        .map_err(|e| format!("workdir {}: {e}", policy.workdir.display()))?;
    if !workdir.is_dir() {
        return Err(format!("workdir {} is not a directory", workdir.display()));
    }
    let home = linux_home()?;

    // Bind every approved host path before masking /mnt and /home. This is
    // deliberate: once the namespace is prepared, the command can see the
    // project and declared writable/read-only roots only, not the Windows
    // user's other files or credentials.
    let mut sandbox = vec![
        "--die-with-parent".to_string(),
        "--unshare-user".to_string(),
        "--unshare-pid".to_string(),
        "--unshare-ipc".to_string(),
        "--unshare-uts".to_string(),
    ];
    if !policy.allow_network {
        sandbox.push("--unshare-net".into());
    }
    sandbox.extend(["--ro-bind".into(), "/".into(), "/".into()]);
    bind_path_args(&mut sandbox, &workdir, "/workspace", true)?;
    for (index, path) in policy.extra_rw.iter().enumerate() {
        // The current caller supplies CARGO_HOME here. Do not bind that whole
        // directory writable: credentials.toml/config.toml can live beside the
        // cache. Offline cache mounts below are read-only instead.
        if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(".cargo"))
        {
            continue;
        }
        bind_path_args(&mut sandbox, path, &format!("/wright-rw-{index}"), true)?;
    }
    for (index, path) in policy.extra_ro.iter().enumerate() {
        bind_path_args(&mut sandbox, path, &format!("/wright-ro-{index}"), false)?;
    }

    // Toolchains are read-only. Cargo's pre-existing package source/cache is
    // exposed read-only only in offline mode; credentials/config files in the
    // user's .cargo directory are never mounted.
    add_existing_read_only_mount(&mut sandbox, &home, ".rustup", "/wright-rustup");
    add_existing_read_only_mount(&mut sandbox, &home, ".cargo/bin", "/wright-cargo-bin");
    if !policy.allow_network {
        sandbox.push("--dir".into());
        sandbox.push("/wright-cargo-cache".into());
        // Prefer the host Cargo cache's source/archive directories, without
        // mounting its credentials or config. If no custom CARGO_HOME was
        // declared, use the WSL user's public crate cache instead.
        if let Some(cargo_home) = policy.extra_rw.iter().find(|path| {
            path.file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(".cargo"))
        }) {
            add_host_cache_mount(
                &mut sandbox,
                cargo_home,
                "registry",
                "/wright-cargo-cache/registry",
            )?;
            add_host_cache_mount(&mut sandbox, cargo_home, "git", "/wright-cargo-cache/git")?;
        } else {
            add_existing_cache_mount(
                &mut sandbox,
                &home,
                ".cargo/registry",
                "/wright-cargo-cache/registry",
            );
            add_existing_cache_mount(&mut sandbox, &home, ".cargo/git", "/wright-cargo-cache/git");
        }
    }

    // Hide other projects, mounted Windows drives, user home directories and
    // host temporary files. Scratch is private to this command and disappears
    // with the namespace. /mnt is masked only after approved paths are bound.
    sandbox.extend([
        "--tmpfs".into(),
        "/home".into(),
        "--tmpfs".into(),
        "/root".into(),
        "--tmpfs".into(),
        "/mnt".into(),
        "--tmpfs".into(),
        "/media".into(),
        "--tmpfs".into(),
        "/tmp".into(),
        "--tmpfs".into(),
        "/var/tmp".into(),
        "--dev".into(),
        "/dev".into(),
        "--proc".into(),
        "/proc".into(),
        "--chdir".into(),
        "/workspace".into(),
        "--dir".into(),
        "/tmp/wright-home".into(),
        "--dir".into(),
        "/tmp/wright-home/.cargo".into(),
        "--dir".into(),
        "/tmp/wright-tmp".into(),
        "--clearenv".into(),
        "--setenv".into(),
        "PATH".into(),
        "/wright-cargo-bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".into(),
        "--setenv".into(),
        "HOME".into(),
        "/tmp/wright-home".into(),
        "--setenv".into(),
        "TMPDIR".into(),
        "/tmp/wright-tmp".into(),
        "--setenv".into(),
        "CARGO_HOME".into(),
        "/tmp/wright-home/.cargo".into(),
        "--setenv".into(),
        "RUSTUP_HOME".into(),
        "/wright-rustup".into(),
        "--setenv".into(),
        "LANG".into(),
        "C.UTF-8".into(),
        "--setenv".into(),
        "NO_COLOR".into(),
        "1".into(),
        "--setenv".into(),
        "PYTHONDONTWRITEBYTECODE".into(),
        "1".into(),
    ]);
    if !policy.allow_network {
        sandbox.extend(["--setenv".into(), "CARGO_NET_OFFLINE".into(), "true".into()]);
    }
    for (key, value) in &policy.env {
        if key.is_empty() || key.contains('=') || key.contains('\0') || value.contains('\0') {
            return Err(format!(
                "invalid environment variable name/value for `{key}`"
            ));
        }
        // Do not permit caller-controlled environment variables to undo the
        // scrubbed home, tool path, or Cargo isolation policy.
        if matches!(
            key.as_str(),
            "HOME" | "PATH" | "TMPDIR" | "RUSTUP_HOME" | "CARGO_HOME"
        ) {
            return Err(format!("the sandbox owns `{key}`; it cannot be overridden"));
        }
        sandbox.extend(["--setenv".into(), key.clone(), value.clone()]);
    }

    // Make the approved offline caches discoverable beneath the fresh Cargo
    // home. Only registry/git caches are mounted; credentials and config are
    // not. A constant shell wrapper forwards the program as positional args.
    let wrapper = if !policy.allow_network {
        "mkdir -p \"$CARGO_HOME\"; for name in registry git; do if [ -d \"/wright-cargo-cache/$name\" ]; then ln -sfn \"/wright-cargo-cache/$name\" \"$CARGO_HOME/$name\"; fi; done; exec \"$@\""
    } else {
        "exec \"$@\""
    };
    sandbox.extend([
        "--".into(),
        "/bin/sh".into(),
        "-c".into(),
        wrapper.into(),
        "wright-sandbox".into(),
        program.into(),
    ]);
    sandbox.extend(args.iter().map(|arg| (*arg).to_string()));

    let timeout = format!("{}s", policy.timeout.as_secs().max(1));
    let mut command = wsl();
    command.args([
        "--exec",
        "timeout",
        "--kill-after=3s",
        "--signal=TERM",
        timeout.as_str(),
        "prlimit",
        "--fsize=536870912",
        "--core=0",
        "--",
        "bwrap",
    ]);
    command.args(&sandbox);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not start the isolated WSL command: {e}. {SETUP_HINT}"))?;
    let stdout = child.stdout.take().expect("piped stdout was requested");
    let stderr = child.stderr.take().expect("piped stderr was requested");
    // Drain both pipes concurrently and retain only their tails. A noisy
    // or malicious command cannot grow the Windows process's output buffer
    // without bound while timeout is supervising the sandbox in WSL.
    let out_thread = thread::spawn(move || read_tail(stdout, MAX_OUTPUT / 2));
    let err_thread = thread::spawn(move || read_tail(stderr, MAX_OUTPUT / 2));
    let status = child
        .wait()
        .map_err(|e| format!("could not wait for the isolated WSL command: {e}"))?;
    let (stdout, out_truncated) = out_thread.join().unwrap_or_default();
    let (stderr, err_truncated) = err_thread.join().unwrap_or_default();
    let code = status.code();
    let timed_out = matches!(code, Some(124) | Some(137));
    let mut combined = String::from_utf8_lossy(&stdout).into_owned();
    if !stderr.is_empty() {
        if !combined.is_empty() && !combined.ends_with('\n') {
            combined.push('\n');
        }
        combined.push_str(&String::from_utf8_lossy(&stderr));
    }
    let truncated = out_truncated || err_truncated || combined.len() > MAX_OUTPUT;
    let output = clipped_to_max(&combined);
    Ok(RunOutput {
        code,
        output,
        timed_out,
        truncated,
    })
}

fn clipped_to_max(s: &str) -> String {
    if s.len() <= MAX_OUTPUT {
        return s.to_string();
    }
    let mut start = s.len() - MAX_OUTPUT;
    while !s.is_char_boundary(start) {
        start += 1;
    }
    s[start..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn output_reader_keeps_the_tail_and_reports_truncation() {
        let (tail, truncated) = read_tail(Cursor::new(b"0123456789"), 4);
        assert_eq!(tail, b"6789");
        assert!(truncated);

        let (whole, truncated) = read_tail(Cursor::new(b"abc"), 4);
        assert_eq!(whole, b"abc");
        assert!(!truncated);
    }

    #[test]
    fn clipping_preserves_utf8_boundaries() {
        let input = "🙂".repeat(MAX_OUTPUT / 4 + 4);
        let clipped = clipped_to_max(&input);
        assert!(clipped.len() <= MAX_OUTPUT);
        assert!(clipped.is_char_boundary(0));
        assert!(clipped.is_char_boundary(clipped.len()));
        assert!(clipped.ends_with("🙂"));
    }
}
