//! Export: a project leaves Wright as something other tools take.
//!
//! ```text
//!  every project  -> <name>-<date>.zip      the source, without logs, caches, builds
//!  python         -> <name>-<v>-py3-none-any.whl   pip installs it
//!  rust           -> <name>-<v>.crate              cargo package
//!  react          -> <name>-<v>-web.zip            the built site (dist/), host anywhere
//! ```
//!
//! **Packages need proof.** A wheel, a crate or a web build is made only
//! from a project whose last run passed its checks and was approved by the
//! Reviewer — the same proof the result screen shows. The source zip is
//! always allowed: it is a copy, not a claim.
//!
//! **Exports never land inside the project.** They go to
//! `<projects folder>/Exports/<project>/` (or `--out`), so exporting does
//! not change the project, its cache keys, or the next export.
//!
//! The zip and the wheel are written here (std only, reproducible bytes);
//! the crate and the web build run the project's own tool chain (`cargo
//! package`, `npm run build`) on this machine, outside the sandbox, because
//! a person asked for them.

use std::path::{Path, PathBuf};
use std::process::Command;

use wright_core::blueprint::Blueprint;
use wright_core::convention;
use wright_core::sha256;
use wright_core::wire::json;
use wright_core::zip::ZipWriter;
use wright_runtime::store::{write_atomic, Store};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The project's source, as a zip.
    Zip,
    /// A Python wheel.
    Wheel,
    /// A Rust crate (`.crate`, what crates.io takes).
    Crate,
    /// A built web app (React), zipped.
    Web,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Zip, Kind::Wheel, Kind::Crate, Kind::Web];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Zip => "Project as .zip",
            Kind::Wheel => "Python wheel",
            Kind::Crate => "Rust crate",
            Kind::Web => "Web build",
        }
    }

    pub fn flag(self) -> &'static str {
        match self {
            Kind::Zip => "--zip",
            Kind::Wheel => "--wheel",
            Kind::Crate => "--crate",
            Kind::Web => "--web",
        }
    }

    /// Whether this export is a package (needs a verified run).
    pub fn needs_proof(self) -> bool {
        self != Kind::Zip
    }
}

/// The exports that make sense for this project: the zip, plus the one
/// package its language has.
pub fn available(bp: &Blueprint) -> Vec<Kind> {
    let mut out = vec![Kind::Zip];
    match bp.project.language.as_str() {
        "python" => out.push(Kind::Wheel),
        "rust" => out.push(Kind::Crate),
        "react" => out.push(Kind::Web),
        _ => {}
    }
    out
}

/// `<parent of the project>/Exports/<project folder name>`.
pub fn default_out(project: &Path) -> PathBuf {
    let name = project.file_name().map(|n| n.to_owned()).unwrap_or_else(|| "project".into());
    project.parent().unwrap_or(project).join("Exports").join(name)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Exported {
    pub kind: Kind,
    pub path: PathBuf,
    pub bytes: u64,
    pub sha256: String,
}

/// Why a package cannot be made yet, or `Ok` when the last run is the proof.
pub fn verified(project: &Path) -> Result<(), String> {
    let store = Store::open(project)?;
    let Some(run) = store.runs().last().copied() else {
        return Err("this project has not run yet — run it, and export once its checks pass".into());
    };
    let receipt = std::fs::read_to_string(store.run_dir(run).join("receipt.json")).map_err(|_| format!("run {run} has no receipt yet (still running, or it stopped)"))?;
    let ok = json::parse(&receipt).ok().and_then(|v| v.get("ok").and_then(|b| b.as_bool())).unwrap_or(false);
    if ok {
        Ok(())
    } else {
        Err(format!("the last run ({run}) did not pass its checks and review — fix that first, then export"))
    }
}

/// Make one export of `project` into `out`.
pub fn export(project: &Path, kind: Kind, out: &Path) -> Result<Exported, String> {
    let bp = crate::load_blueprint(project)?;
    if !available(&bp).contains(&kind) {
        return Err(format!("a {} is not available for a {} project", kind.label().to_lowercase(), bp.project.language));
    }
    if kind.needs_proof() {
        verified(project)?;
    }
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let (name, bytes) = match kind {
        Kind::Zip => source_zip(project, &bp)?,
        Kind::Wheel => wheel(project, &bp)?,
        Kind::Crate => crate_file(project)?,
        Kind::Web => web(project, &bp)?,
    };
    let path = out.join(name);
    write_atomic(&path, &bytes)?;
    Ok(Exported { kind, bytes: bytes.len() as u64, sha256: sha256::hex(&bytes), path })
}

// ── the files of a project ───────────────────────────────────────

/// Folders that are never part of what a project *is*.
const SKIP_DIRS: &[&str] = &[".git", ".wright", "target", "node_modules", "dist", "__pycache__", ".venv", "venv", ".pytest_cache", ".mypy_cache"];

/// Every regular file under `dir`, as (path relative to `dir` with `/`,
/// bytes), sorted so the same tree makes the same archive. Symbolic links
/// are skipped: an export copies the project, not what it points at.
fn files(dir: &Path, skip: &dyn Fn(&str, bool) -> bool) -> Result<Vec<(String, Vec<u8>)>, String> {
    fn walk(root: &Path, dir: &Path, skip: &dyn Fn(&str, bool) -> bool, out: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
        let mut entries: Vec<_> = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let meta = std::fs::symlink_metadata(e.path()).map_err(|err| err.to_string())?;
            let rel = e.path().strip_prefix(root).map_err(|err| err.to_string())?.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect::<Vec<_>>().join("/");
            if meta.file_type().is_symlink() || skip(&rel, meta.is_dir()) {
                continue;
            }
            if meta.is_dir() {
                walk(root, &e.path(), skip, out)?;
            } else if meta.is_file() {
                out.push((rel, std::fs::read(e.path()).map_err(|err| format!("{}: {err}", e.path().display()))?));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(dir, dir, skip, &mut out)?;
    Ok(out)
}

fn zip_of(prefix: &str, entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    let mut z = ZipWriter::new();
    for (path, data) in entries {
        z.add(&format!("{prefix}/{path}"), data)?;
    }
    z.finish()
}

fn today() -> String {
    crate::firing::today_utc()
}

// ── the four exports ─────────────────────────────────────────────

fn source_zip(project: &Path, bp: &Blueprint) -> Result<(String, Vec<u8>), String> {
    let skip = |rel: &str, is_dir: bool| {
        let name = rel.rsplit('/').next().unwrap_or(rel);
        (is_dir && SKIP_DIRS.contains(&name))
            || (rel.starts_with("logs/") && name != ".keep")
            || name.ends_with(".pyc")
    };
    let entries = files(project, &skip)?;
    let slug = convention::slug(&bp.meta.name).replace('_', "-");
    Ok((format!("{slug}-{}.zip", today()), zip_of(&slug, &entries)?))
}

/// A pure-Python wheel (PEP 427) of `src/<slug>/`, written directly: a
/// wheel is a zip with three metadata files, so no `pip install build`.
fn wheel(project: &Path, bp: &Blueprint) -> Result<(String, Vec<u8>), String> {
    let slug = convention::slug(&bp.meta.name);
    let pkg = project.join("src").join(&slug);
    if !pkg.is_dir() {
        return Err(format!("no Python package at src/{slug}/ — the convention puts the code there"));
    }
    let init = std::fs::read_to_string(pkg.join("__init__.py")).unwrap_or_default();
    let version = init
        .lines()
        .find_map(|l| l.trim().strip_prefix("__version__").map(|r| r.trim().trim_start_matches('=').trim().trim_matches(|c| c == '"' || c == '\'').to_string()))
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "0.1.0".into());
    let dist = format!("{slug}-{version}.dist-info");
    let skip = |rel: &str, is_dir: bool| (is_dir && rel.rsplit('/').next() == Some("__pycache__")) || rel.ends_with(".pyc");
    let mut entries: Vec<(String, Vec<u8>)> = files(&pkg, &skip)?.into_iter().map(|(p, d)| (format!("{slug}/{p}"), d)).collect();

    let summary = bp.intent.summary.lines().next().unwrap_or("").trim().to_string();
    let readme = std::fs::read_to_string(project.join("README.md")).unwrap_or_default();
    let metadata = format!(
        "Metadata-Version: 2.1\nName: {name}\nVersion: {version}\nSummary: {summary}\nRequires-Python: >=3.9\nDescription-Content-Type: text/markdown\n\n{readme}",
        name = slug.replace('_', "-"),
    );
    entries.push((format!("{dist}/METADATA"), metadata.into_bytes()));
    entries.push((format!("{dist}/WHEEL"), format!("Wheel-Version: 1.0\nGenerator: wright {}\nRoot-Is-Purelib: true\nTag: py3-none-any\n", env!("CARGO_PKG_VERSION")).into_bytes()));
    let main = std::fs::read_to_string(pkg.join("__main__.py")).unwrap_or_default();
    if main.contains("def main(") {
        entries.push((format!("{dist}/entry_points.txt"), format!("[console_scripts]\n{} = {slug}.__main__:main\n", slug.replace('_', "-")).into_bytes()));
    }
    // RECORD: every file with its SHA-256 and size; RECORD itself unhashed.
    let mut record = String::new();
    for (path, data) in &entries {
        record.push_str(&format!("{path},sha256={},{}\n", sha256::base64_url(&sha256::digest(data)), data.len()));
    }
    record.push_str(&format!("{dist}/RECORD,,\n"));
    entries.push((format!("{dist}/RECORD"), record.into_bytes()));

    let mut z = ZipWriter::new();
    for (path, data) in &entries {
        z.add(path, data)?;
    }
    Ok((format!("{slug}-{version}-py3-none-any.whl"), z.finish()?))
}

/// A command from the project's tool chain. On Windows, npm and friends
/// are `.cmd` scripts that only `cmd` can start.
fn tool(name: &str) -> Command {
    if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", name]);
        c
    } else {
        Command::new(name)
    }
}

fn run(mut cmd: Command, dir: &Path, what: &str) -> Result<(), String> {
    let out = cmd.current_dir(dir).output().map_err(|e| format!("could not start {what}: {e} — is it installed and on PATH?"))?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let tail: Vec<&str> = err.lines().rev().take(12).collect::<Vec<_>>().into_iter().rev().collect();
    Err(format!("{what} failed:\n{}", tail.join("\n")))
}

/// `cargo package`, into a scratch target folder (the project's own
/// `target/` is left alone).
fn crate_file(project: &Path) -> Result<(String, Vec<u8>), String> {
    let scratch = std::env::temp_dir().join(format!("wright-crate-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let mut cmd = tool("cargo");
    cmd.args(["package", "--allow-dirty", "--no-verify", "--quiet", "--target-dir"]).arg(&scratch);
    run(cmd, project, "cargo package")?;
    let found = std::fs::read_dir(scratch.join("package"))
        .map_err(|e| format!("cargo package wrote nothing: {e}"))?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x == "crate"))
        .ok_or("cargo package made no .crate file")?;
    let name = found.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let bytes = std::fs::read(&found).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&scratch);
    Ok((name, bytes))
}

/// `npm install` and `npm run build`, then `dist/` as a zip.
///
/// Install runs every time: it takes about a second when nothing changed,
/// and a `node_modules/` that merely exists can be partial or stale (an
/// interrupted install, a dependency an agent just added).
fn web(project: &Path, bp: &Blueprint) -> Result<(String, Vec<u8>), String> {
    let mut cmd = tool("npm");
    cmd.args(["install", "--no-audit", "--no-fund"]);
    run(cmd, project, "npm install")?;
    let mut cmd = tool("npm");
    cmd.args(["run", "build"]);
    run(cmd, project, "npm run build")?;
    let dist = project.join("dist");
    if !dist.join("index.html").is_file() {
        return Err("the build finished but dist/index.html is missing".into());
    }
    let version = std::fs::read_to_string(project.join("package.json"))
        .ok()
        .and_then(|t| json::parse(&t).ok())
        .and_then(|v| v.str_of("version").map(String::from))
        .unwrap_or_else(|| "0.1.0".into());
    let slug = convention::slug(&bp.meta.name).replace('_', "-");
    let entries = files(&dist, &|_, _| false)?;
    Ok((format!("{slug}-{version}-web.zip"), zip_of(&slug, &entries)?))
}
