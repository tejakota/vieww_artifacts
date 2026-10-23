//! The process sandbox for `shell`, `git` and `run-tests`.
//!
//! Linux (v0.2): every command runs in a child process that, before it
//! executes anything, locks itself down with the kernel's own tools:
//!
//! * **Landlock** (kernel ≥ 5.13): the filesystem becomes read-only and
//!   executable only where the toolchain lives (`/usr`, `/bin`, `/lib`,
//!   `/etc`, `/opt`, the directories on `PATH` and their parents), and
//!   writable ONLY inside the project (plus a private scratch HOME/TMPDIR
//!   that is deleted afterwards). Everything else — `~/.ssh`, other
//!   projects, Wright's own store — is invisible.
//! * **No network** unless the agent holds the network permission: a
//!   fresh network namespace (no interfaces), and Landlock's TCP rules
//!   (ABI ≥ 4) as a second wall. If neither is available the command is
//!   REFUSED — the sandbox fails closed, never open.
//! * `no_new_privs`, its own process group (killed as a group on
//!   timeout), core dumps off, a file-size limit, and a scrubbed
//!   environment (no inherited secrets, no API keys).
//!
//! Windows uses WSL 2 + Bubblewrap (`sandbox_windows.rs`): the project is
//! mounted as the only writable source tree, Windows drives and user homes
//! are hidden, and commands still fail closed if the isolation runtime is not
//! installed. macOS (Seatbelt) remains fail-closed until its backend exists.
//!
//! This is one of the files AGENTS.md allows `unsafe` in: the FFI and the
//! `pre_exec` hook. Every block carries a SAFETY comment.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// What a sandboxed command may do.
#[derive(Debug, Clone)]
pub struct Policy {
    /// Working directory, and the one writable tree.
    pub workdir: PathBuf,
    pub allow_network: bool,
    pub timeout: Duration,
    /// Extra writable directories (e.g. CARGO_HOME for Rust projects).
    pub extra_rw: Vec<PathBuf>,
    /// Extra read-only directories.
    pub extra_ro: Vec<PathBuf>,
    /// Environment on top of the scrubbed base.
    pub env: Vec<(String, String)>,
}

impl Policy {
    pub fn new(workdir: &Path) -> Policy {
        Policy {
            workdir: workdir.to_path_buf(),
            allow_network: false,
            timeout: Duration::from_secs(120),
            extra_rw: Vec::new(),
            extra_ro: Vec::new(),
            env: Vec::new(),
        }
    }
}

/// The result of one sandboxed command.
#[derive(Debug, Clone)]
pub struct RunOutput {
    /// Exit code, or None if killed by a signal / timeout.
    pub code: Option<i32>,
    /// stdout and stderr interleaved, tail-truncated to `MAX_OUTPUT`.
    pub output: String,
    pub timed_out: bool,
    pub truncated: bool,
}

impl RunOutput {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }

    /// The text an agent sees.
    pub fn report(&self) -> String {
        let status = match (self.timed_out, self.code) {
            (true, _) => "TIMED OUT (killed)".to_string(),
            (false, Some(c)) => format!("exit code {c}"),
            (false, None) => "killed by a signal".to_string(),
        };
        let trunc = if self.truncated {
            "[... earlier output truncated ...]\n"
        } else {
            ""
        };
        format!("{status}\n{trunc}{}", self.output)
    }
}

/// Output kept per command (the tail; the end is where errors are).
pub const MAX_OUTPUT: usize = 32 * 1024;

/// What the sandbox can do on this machine, for `wright doctor`.
pub fn capabilities() -> Result<String, String> {
    #[cfg(target_os = "linux")]
    {
        let abi = linux::landlock_abi();
        if abi <= 0 {
            return Err("Landlock is not available (kernel 5.13+ with Landlock enabled is required); shell, git and run-tests will refuse".into());
        }
        Ok(format!(
            "Landlock ABI {abi}{}",
            if abi >= 4 { ", network rules" } else { "" }
        ))
    }
    #[cfg(target_os = "windows")]
    {
        windows::capabilities()
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Err("the process sandbox is not available on this platform; shell, git and run-tests will refuse".into())
    }
}

/// Run `program args...` under `policy`.
pub fn run(program: &str, args: &[&str], policy: &Policy) -> Result<RunOutput, String> {
    #[cfg(target_os = "linux")]
    {
        linux::run(program, args, policy)
    }
    #[cfg(target_os = "windows")]
    {
        windows::run(program, args, policy)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = (program, args, policy);
        Err(
            "sandboxed commands are unavailable on this platform; refusing to run unsandboxed"
                .into(),
        )
    }
}

#[cfg(target_os = "windows")]
#[path = "sandbox_windows.rs"]
mod windows;

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::ffi::{c_int, c_long, c_ulong, c_void};
    use std::fs::OpenOptions;
    use std::io::Read;
    use std::os::fd::{AsRawFd, OwnedFd};
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::process::{CommandExt, ExitStatusExt};
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Instant;

    const SYS_LANDLOCK_CREATE_RULESET: c_long = 444;
    const SYS_LANDLOCK_ADD_RULE: c_long = 445;
    const SYS_LANDLOCK_RESTRICT_SELF: c_long = 446;
    const LANDLOCK_CREATE_RULESET_VERSION: u32 = 1;
    const LANDLOCK_RULE_PATH_BENEATH: c_int = 1;

    const EXECUTE: u64 = 1 << 0;
    const WRITE_FILE: u64 = 1 << 1;
    const READ_FILE: u64 = 1 << 2;
    const READ_DIR: u64 = 1 << 3;
    const NET_BIND_TCP: u64 = 1 << 0;
    const NET_CONNECT_TCP: u64 = 1 << 1;

    const PR_SET_NO_NEW_PRIVS: c_int = 38;
    const CLONE_NEWUSER: c_int = 0x1000_0000;
    const CLONE_NEWNET: c_int = 0x4000_0000;
    const RLIMIT_FSIZE: c_int = 1;
    const RLIMIT_CORE: c_int = 4;
    const SIGKILL: c_int = 9;
    const O_PATH: i32 = 0o10_000_000;

    #[repr(C)]
    struct RLimit {
        cur: u64,
        max: u64,
    }

    #[repr(C)]
    struct RulesetAttr {
        handled_access_fs: u64,
        handled_access_net: u64,
    }

    #[repr(C, packed)]
    struct PathBeneath {
        allowed_access: u64,
        parent_fd: i32,
    }

    extern "C" {
        fn syscall(num: c_long, ...) -> c_long;
        fn prctl(option: c_int, a2: c_ulong, a3: c_ulong, a4: c_ulong, a5: c_ulong) -> c_int;
        fn setpgid(pid: c_int, pgid: c_int) -> c_int;
        fn kill(pid: c_int, sig: c_int) -> c_int;
        fn unshare(flags: c_int) -> c_int;
        fn setrlimit(resource: c_int, rlim: *const RLimit) -> c_int;
        fn close(fd: c_int) -> c_int;
    }

    /// Landlock ABI version, or <= 0 if unavailable.
    pub fn landlock_abi() -> c_long {
        // SAFETY: the documented version query: null attr, size 0.
        unsafe {
            syscall(
                SYS_LANDLOCK_CREATE_RULESET,
                std::ptr::null::<c_void>(),
                0usize,
                LANDLOCK_CREATE_RULESET_VERSION,
            )
        }
    }

    fn all_fs_rights(abi: c_long) -> u64 {
        let mut bits = (1u64 << 13) - 1; // ABI 1: 13 rights
        if abi >= 2 {
            bits |= 1 << 13; // REFER
        }
        if abi >= 3 {
            bits |= 1 << 14; // TRUNCATE
        }
        if abi >= 5 {
            bits |= 1 << 15; // IOCTL_DEV
        }
        bits
    }

    fn open_path(p: &Path) -> Option<OwnedFd> {
        OpenOptions::new()
            .read(true)
            .custom_flags(O_PATH | 0o2_000_000 /* O_CLOEXEC */)
            .open(p)
            .ok()
            .map(OwnedFd::from)
    }

    static SCRATCH_SEQ: AtomicU64 = AtomicU64::new(0);

    fn read_only_roots(policy: &Policy) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = [
            "/usr",
            "/bin",
            "/sbin",
            "/lib",
            "/lib64",
            "/lib32",
            "/etc",
            "/opt",
            "/nix",
            "/snap",
            "/proc",
            "/sys/devices/system/cpu",
        ]
        .iter()
        .map(PathBuf::from)
        .collect();
        if let Ok(path) = std::env::var("PATH") {
            for d in path.split(':').filter(|d| d.starts_with('/')) {
                let d = PathBuf::from(d);
                if let Some(parent) = d.parent() {
                    if parent != Path::new("/") {
                        dirs.push(parent.to_path_buf());
                    }
                }
                dirs.push(d);
            }
        }
        // Rust toolchains live in the home directory.
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(&home).join(".rustup"));
        }
        dirs.extend(policy.extra_ro.iter().cloned());
        dirs
    }

    pub fn run(program: &str, args: &[&str], policy: &Policy) -> Result<RunOutput, String> {
        let abi = landlock_abi();
        if abi <= 0 {
            return Err(
                "Landlock is not available on this kernel; refusing to run commands unsandboxed"
                    .into(),
            );
        }
        let workdir = policy
            .workdir
            .canonicalize()
            .map_err(|e| format!("workdir: {e}"))?;

        // A private scratch HOME/TMPDIR, outside the project, deleted after.
        let scratch = std::env::temp_dir().join(format!(
            "wright-sbx-{}-{}",
            std::process::id(),
            SCRATCH_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(scratch.join("tmp")).map_err(|e| format!("scratch: {e}"))?;
        let _cleanup = Cleanup(scratch.clone());

        // Open every rule's directory BEFORE fork: the child may only make
        // raw syscalls (no allocation) between fork and exec.
        let fs_all = all_fs_rights(abi);
        let ro = EXECUTE | READ_FILE | READ_DIR;
        let mut rules: Vec<(u64, OwnedFd)> = Vec::new();
        for d in read_only_roots(policy) {
            if let Some(fd) = open_path(&d) {
                rules.push((ro, fd));
            }
        }
        if let Some(fd) = open_path(Path::new("/dev")) {
            rules.push((READ_FILE | WRITE_FILE | READ_DIR, fd));
        }
        for d in std::iter::once(&workdir)
            .chain(policy.extra_rw.iter())
            .chain(std::iter::once(&scratch))
        {
            match open_path(d) {
                Some(fd) => rules.push((fs_all, fd)),
                None => return Err(format!("cannot open {} for the sandbox", d.display())),
            }
        }
        let rule_specs: Vec<(u64, c_int)> =
            rules.iter().map(|(a, fd)| (*a, fd.as_raw_fd())).collect();
        let allow_network = policy.allow_network;
        let handled_net = if abi >= 4 && !allow_network {
            NET_BIND_TCP | NET_CONNECT_TCP
        } else {
            0
        };
        if !allow_network && abi < 4 {
            // We will rely on the network namespace alone; check it works
            // here, in the parent, so a failure is a clean refusal.
            // (Probed in a throwaway child via the same unshare call.)
            let ok = Command::new("/bin/true")
                .pre_exec_unshare_probe()
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !ok {
                return Err("cannot isolate the network on this kernel (no Landlock net rules, no network namespaces); refusing to run a command without network permission".into());
            }
        }

        let (mut reader, writer) = std::io::pipe().map_err(|e| format!("pipe: {e}"))?;
        let writer2 = writer.try_clone().map_err(|e| format!("pipe: {e}"))?;
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(&workdir)
            .env_clear()
            .env(
                "PATH",
                std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into()),
            )
            .env("HOME", &scratch)
            .env("TMPDIR", scratch.join("tmp"))
            .env("LANG", "C.UTF-8")
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env("NO_COLOR", "1")
            .envs(policy.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdin(Stdio::null())
            .stdout(writer)
            .stderr(writer2);
        if let Ok(rustup) = std::env::var("RUSTUP_HOME") {
            cmd.env("RUSTUP_HOME", rustup);
        } else if let Ok(home) = std::env::var("HOME") {
            cmd.env("RUSTUP_HOME", PathBuf::from(home).join(".rustup"));
        }

        // SAFETY: the closure runs in the forked child before exec. It
        // only performs raw syscalls on stack data and pre-opened fds
        // (no allocation, no locks), which is async-signal-safe.
        unsafe {
            cmd.pre_exec(move || {
                setpgid(0, 0);
                let fsize = RLimit {
                    cur: 512 << 20,
                    max: 512 << 20,
                };
                setrlimit(RLIMIT_FSIZE, &fsize);
                let core = RLimit { cur: 0, max: 0 };
                setrlimit(RLIMIT_CORE, &core);
                if !allow_network && unshare(CLONE_NEWUSER | CLONE_NEWNET) != 0 && handled_net == 0
                {
                    return Err(std::io::Error::other("network isolation failed"));
                }
                if prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                let attr = RulesetAttr {
                    handled_access_fs: fs_all,
                    handled_access_net: handled_net,
                };
                let size = if abi >= 4 {
                    std::mem::size_of::<RulesetAttr>()
                } else {
                    8
                };
                let ruleset = syscall(
                    SYS_LANDLOCK_CREATE_RULESET,
                    &attr as *const RulesetAttr,
                    size,
                    0u32,
                ) as c_int;
                if ruleset < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                for (access, fd) in rule_specs.iter() {
                    let rule = PathBeneath {
                        allowed_access: *access & fs_all,
                        parent_fd: *fd,
                    };
                    if syscall(
                        SYS_LANDLOCK_ADD_RULE,
                        ruleset,
                        LANDLOCK_RULE_PATH_BENEATH,
                        &rule as *const PathBeneath,
                        0u32,
                    ) != 0
                    {
                        // A directory whose type does not support every
                        // right (rare) is skipped rather than fatal.
                        continue;
                    }
                }
                if syscall(SYS_LANDLOCK_RESTRICT_SELF, ruleset, 0u32) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                close(ruleset);
                Ok(())
            });
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("could not start '{program}': {e}"))?;
        drop(cmd); // closes our copies of the pipe's write ends
        drop(rules);
        let pid = child.id() as c_int;

        let reader_thread = std::thread::spawn(move || {
            let mut keep: Vec<u8> = Vec::new();
            let mut truncated = false;
            let mut buf = [0u8; 8192];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 {
                    break;
                }
                keep.extend_from_slice(&buf[..n]);
                if keep.len() > MAX_OUTPUT * 2 {
                    let cut = keep.len() - MAX_OUTPUT;
                    keep.drain(..cut);
                    truncated = true;
                }
            }
            if keep.len() > MAX_OUTPUT {
                let cut = keep.len() - MAX_OUTPUT;
                keep.drain(..cut);
                truncated = true;
            }
            (String::from_utf8_lossy(&keep).into_owned(), truncated)
        });

        let deadline = Instant::now() + policy.timeout;
        let mut timed_out = false;
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break s,
                Ok(None) if wright_runtime::killswitch::is_stopped() => {
                    // The kill switch: the command goes now, not at its clock.
                    timed_out = true;
                    // SAFETY: plain kill(2) of our own child's process group.
                    unsafe { kill(-pid, SIGKILL) };
                    break child.wait().map_err(|e| e.to_string())?;
                }
                Ok(None) if Instant::now() >= deadline => {
                    timed_out = true;
                    // SAFETY: plain kill(2) of our own child's process group.
                    unsafe { kill(-pid, SIGKILL) };
                    break child.wait().map_err(|e| e.to_string())?;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(15)),
                Err(e) => return Err(format!("waiting for '{program}': {e}")),
            }
        };
        // Kill stragglers the command may have left in its group.
        // SAFETY: as above; ESRCH (no such group) is harmless.
        unsafe { kill(-pid, SIGKILL) };
        let (output, truncated) = reader_thread.join().unwrap_or_default();
        let _ = status.signal();
        Ok(RunOutput {
            code: status.code(),
            output,
            timed_out,
            truncated,
        })
    }

    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    trait ProbeExt {
        fn pre_exec_unshare_probe(&mut self) -> &mut Self;
    }
    impl ProbeExt for Command {
        fn pre_exec_unshare_probe(&mut self) -> &mut Self {
            // SAFETY: a raw syscall in the child, nothing else.
            unsafe {
                self.pre_exec(|| {
                    if unshare(CLONE_NEWUSER | CLONE_NEWNET) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                })
            }
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    fn project(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("wright-sbx-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d.canonicalize().unwrap()
    }

    fn sh(cmd: &str, policy: &Policy) -> RunOutput {
        run("/bin/sh", &["-c", cmd], policy).expect("sandbox available on the test machine")
    }

    #[test]
    fn commands_run_and_can_write_the_project() {
        let p = project("write");
        let out = sh("echo hello > made.txt && cat made.txt", &Policy::new(&p));
        assert!(out.success(), "{}", out.report());
        assert_eq!(out.output.trim(), "hello");
        assert!(p.join("made.txt").exists());
    }

    #[test]
    fn writes_outside_the_project_are_blocked() {
        let p = project("outside");
        let victim = std::env::temp_dir().join(format!("wright-victim-{}", std::process::id()));
        let out = sh(
            &format!("echo pwned > {}", victim.display()),
            &Policy::new(&p),
        );
        assert!(!out.success());
        assert!(!victim.exists(), "the sandbox let a write escape");
    }

    #[test]
    fn home_secrets_are_invisible() {
        let p = project("home");
        let secret_dir =
            std::env::temp_dir().join(format!("wright-fake-home-{}", std::process::id()));
        std::fs::create_dir_all(&secret_dir).unwrap();
        std::fs::write(secret_dir.join("id_rsa"), "PRIVATE").unwrap();
        let out = sh(
            &format!("cat {}/id_rsa", secret_dir.display()),
            &Policy::new(&p),
        );
        assert!(!out.output.contains("PRIVATE"), "{}", out.report());
    }

    #[test]
    fn environment_is_scrubbed() {
        std::env::set_var("WRIGHT_SECRET_SHOULD_NOT_LEAK", "leaked");
        let p = project("env");
        let out = sh("env", &Policy::new(&p));
        assert!(!out.output.contains("leaked"));
    }

    #[test]
    fn network_is_off_unless_granted() {
        let p = project("net");
        // Connect to a local listener: blocked without permission.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let probe = format!("python3 -c \"import socket; socket.create_connection(('127.0.0.1', {port}), timeout=2); print('CON' + 'NECTED')\"");
        let denied = sh(&probe, &Policy::new(&p));
        assert!(!denied.output.contains("CONNECTED"), "{}", denied.report());
        let mut allowed_policy = Policy::new(&p);
        allowed_policy.allow_network = true;
        let allowed = sh(&probe, &allowed_policy);
        assert!(allowed.output.contains("CONNECTED"), "{}", allowed.report());
    }

    #[test]
    fn timeouts_kill_the_whole_process_group() {
        let p = project("timeout");
        let mut pol = Policy::new(&p);
        pol.timeout = Duration::from_millis(300);
        let started = std::time::Instant::now();
        let out = sh("sleep 30 & sleep 30; echo never", &pol);
        assert!(out.timed_out);
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
