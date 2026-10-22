//! `wright mcp install`: freeze an MCP server so its pin covers its code.
//!
//! `npx some-server` and `uvx some-server` fetch whatever the registry
//! serves at that moment; a pin of `npx` pins nothing that runs. This
//! installs ONE EXACT VERSION into `<wright home>/mcp/<name>/` — through
//! the machine's own npm or Python, nothing added to Wright — and pins the
//! whole installed tree (`mcp::tree_pin`). The config line then runs the
//! frozen copy; a call re-checks the tree and refuses anything changed.
//!
//! * `npm:<package>@<version>` — `npm install --prefix <dir> --ignore-scripts`
//!   (install scripts are other people's code running at install time;
//!   `--allow-scripts` lets them, on purpose). Runs the package's own bin.
//! * `pypi:<package>==<version>` — a private virtualenv,
//!   `pip install <package>==<version>`. Runs the console script.
//!
//! A version is required, and it must be exact: `latest`, ranges and
//! missing versions are refused, because "whatever comes" is the thing
//! being prevented. Installing replaces an earlier install of the same
//! name only after the new one installed and pinned cleanly.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::mcp::tree_pin;

/// Where a server comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Npm { package: String, version: String },
    Pypi { package: String, version: String },
}

impl Source {
    /// `npm:@scope/pkg@1.2.3` or `pypi:pkg==1.2.3`.
    pub fn parse(spec: &str) -> Result<Source, String> {
        let exact = |v: &str| -> Result<String, String> {
            let ok = !v.is_empty()
                && v.chars().next().is_some_and(|c| c.is_ascii_digit())
                && v.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'));
            if ok {
                Ok(v.to_string())
            } else {
                Err(format!("'{v}' is not an exact version (like 1.2.3) — `latest`, ranges and tags change under you, which is what pinning prevents"))
            }
        };
        if let Some(rest) = spec.strip_prefix("npm:") {
            // A scope starts with '@'; the version is after the LAST '@'.
            let at = rest.rfind('@').filter(|&i| i > 0).ok_or_else(|| {
                format!("npm:{rest} needs an exact version: npm:{rest}@<version>")
            })?;
            let (package, version) = (&rest[..at], &rest[at + 1..]);
            if package.is_empty() || package.contains(char::is_whitespace) {
                return Err(format!("'{package}' is not an npm package name"));
            }
            return Ok(Source::Npm {
                package: package.into(),
                version: exact(version)?,
            });
        }
        if let Some(rest) = spec.strip_prefix("pypi:") {
            let (package, version) = rest.split_once("==").ok_or_else(|| {
                format!("pypi:{rest} needs an exact version: pypi:{rest}==<version>")
            })?;
            if package.is_empty()
                || !package
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            {
                return Err(format!("'{package}' is not a PyPI package name"));
            }
            return Ok(Source::Pypi {
                package: package.into(),
                version: exact(version)?,
            });
        }
        Err(format!("'{spec}': say where the server comes from — npm:<package>@<version> or pypi:<package>==<version>"))
    }
}

/// What the config line needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub command: PathBuf,
    pub tree: PathBuf,
    pub hash: String,
}

/// Install `source` as the server `name` under `root` (normally
/// `<wright home>/mcp`). `bin` picks one of several executables a
/// package offers; `allow_scripts` lets npm install scripts run.
pub fn install(
    name: &str,
    source: &Source,
    bin: Option<&str>,
    allow_scripts: bool,
    root: &Path,
) -> Result<Installed, String> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        return Err(format!(
            "'{name}' is not a server name (letters, digits, - and _)"
        ));
    }
    let final_dir = root.join(name);
    let staging = root.join(format!(".{name}.installing-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| format!("{}: {e}", staging.display()))?;
    let result = (|| -> Result<PathBuf, String> {
        match source {
            Source::Npm { package, version } => {
                let mut cmd = Command::new(if cfg!(windows) { "npm.cmd" } else { "npm" });
                cmd.args(["install", "--prefix"]).arg(&staging).args([
                    "--no-audit",
                    "--no-fund",
                    "--save-exact",
                ]);
                if !allow_scripts {
                    cmd.arg("--ignore-scripts");
                }
                cmd.arg(format!("{package}@{version}"));
                run(cmd, "npm")?;
                let rel = npm_bin(&staging, package, bin)?;
                Ok(rel)
            }
            Source::Pypi { package, version } => {
                let venv = staging.join("venv");
                let mut mk = Command::new(if cfg!(windows) { "python" } else { "python3" });
                mk.args(["-m", "venv"]).arg(&venv);
                run(mk, "python -m venv")?;
                let scripts = if cfg!(windows) {
                    venv.join("Scripts")
                } else {
                    venv.join("bin")
                };
                let mut pip = Command::new(scripts.join(if cfg!(windows) {
                    "python.exe"
                } else {
                    "python"
                }));
                pip.args([
                    "-m",
                    "pip",
                    "install",
                    "--disable-pip-version-check",
                    "--no-input",
                ])
                .arg(format!("{package}=={version}"));
                run(pip, "pip")?;
                let exe = bin.unwrap_or(package);
                let path = scripts.join(if cfg!(windows) {
                    format!("{exe}.exe")
                } else {
                    exe.to_string()
                });
                if !path.exists() {
                    return Err(format!(
                        "{package} installed, but it has no `{exe}` program; name the one it has with --bin (its programs are in {})",
                        scripts.display()
                    ));
                }
                Ok(path.strip_prefix(&staging).unwrap_or(&path).to_path_buf())
            }
        }
    })();
    let rel = match result {
        Ok(r) => r,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(e);
        }
    };
    // Swap in the new install whole, then pin it where it will run from
    // (venv files name their own folder, so the pin is taken in place).
    let _ = std::fs::remove_dir_all(&final_dir);
    std::fs::rename(&staging, &final_dir)
        .map_err(|e| format!("move the install into {}: {e}", final_dir.display()))?;
    if matches!(source, Source::Pypi { .. }) {
        // A virtualenv records its own path; re-point it after the move.
        let cfg = final_dir.join("venv").join("pyvenv.cfg");
        if let Ok(text) = std::fs::read_to_string(&cfg) {
            let fixed = text.replace(
                &staging.to_string_lossy().to_string(),
                &final_dir.to_string_lossy(),
            );
            let _ = std::fs::write(&cfg, fixed);
        }
        rewrite_shebangs(&final_dir.join("venv").join("bin"), &staging, &final_dir);
    }
    let hash = tree_pin(&final_dir)?;
    Ok(Installed {
        command: final_dir.join(rel),
        tree: final_dir,
        hash,
    })
}

/// npm: the package's own executable, through node_modules/.bin.
fn npm_bin(prefix: &Path, package: &str, bin: Option<&str>) -> Result<PathBuf, String> {
    let manifest = prefix
        .join("node_modules")
        .join(package)
        .join("package.json");
    let text =
        std::fs::read_to_string(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let v = wright_core::wire::json::parse(&text)
        .map_err(|e| format!("{}: {e}", manifest.display()))?;
    let names: Vec<String> = match v.get("bin") {
        Some(wright_core::wire::json::Value::Str(_)) => {
            vec![package.rsplit('/').next().unwrap_or(package).to_string()]
        }
        Some(wright_core::wire::json::Value::Obj(fields)) => {
            fields.iter().map(|(k, _)| k.clone()).collect()
        }
        _ => Vec::new(),
    };
    let chosen = match (bin, names.as_slice()) {
        (Some(b), _) if names.iter().any(|n| n == b) => b.to_string(),
        (Some(b), _) => {
            return Err(format!(
                "{package} has no program called '{b}' (it has: {})",
                names.join(", ")
            ))
        }
        (None, [one]) => one.clone(),
        (None, []) => {
            return Err(format!(
                "{package} declares no program to run; it is not a runnable MCP server"
            ))
        }
        (None, many) => {
            return Err(format!(
                "{package} has several programs ({}); pick one with --bin",
                many.join(", ")
            ))
        }
    };
    let rel = PathBuf::from("node_modules")
        .join(".bin")
        .join(if cfg!(windows) {
            format!("{chosen}.cmd")
        } else {
            chosen
        });
    if !prefix.join(&rel).exists() {
        return Err(format!("npm did not create {}", rel.display()));
    }
    Ok(rel)
}

/// Console scripts in a moved virtualenv still name the old interpreter.
fn rewrite_shebangs(bin: &Path, from: &Path, to: &Path) {
    let (from, to) = (
        from.to_string_lossy().to_string(),
        to.to_string_lossy().to_string(),
    );
    let Ok(entries) = std::fs::read_dir(bin) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if std::fs::symlink_metadata(&p)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(true)
        {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&p) {
            if text.starts_with("#!") && text.contains(&from) {
                let _ = std::fs::write(&p, text.replace(&from, &to));
            }
        }
    }
}

fn run(mut cmd: Command, what: &str) -> Result<(), String> {
    let out = cmd
        .output()
        .map_err(|e| format!("could not run {what}: {e} — is it installed?"))?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let tail: String = err
        .lines()
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    Err(format!("{what} failed:\n{tail}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_exact_versions_are_installed() {
        assert_eq!(
            Source::parse("npm:@modelcontextprotocol/server-everything@2025.1.14").unwrap(),
            Source::Npm {
                package: "@modelcontextprotocol/server-everything".into(),
                version: "2025.1.14".into()
            }
        );
        assert_eq!(
            Source::parse("pypi:mcp-server-time==0.6.2").unwrap(),
            Source::Pypi {
                package: "mcp-server-time".into(),
                version: "0.6.2".into()
            }
        );
        for bad in [
            "npm:server-x",
            "npm:server-x@latest",
            "npm:server-x@^1.2.0",
            "pypi:mcp-server-time",
            "pypi:x>=1.0",
            "server-x",
        ] {
            assert!(Source::parse(bad).is_err(), "{bad} must be refused");
        }
    }

    #[test]
    fn a_tree_pin_sees_every_file_but_not_bytecode() {
        let d = std::env::temp_dir().join(format!("wright-tree-pin-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("lib/pkg")).unwrap();
        std::fs::write(d.join("lib/pkg/server.js"), "console.log(1)").unwrap();
        std::fs::write(d.join("run"), "#!/bin/sh\n").unwrap();
        let a = tree_pin(&d).unwrap();
        assert!(a.starts_with("tree-blake3:"));
        // Python writes bytecode as it runs: not part of the server.
        std::fs::create_dir_all(d.join("lib/pkg/__pycache__")).unwrap();
        std::fs::write(d.join("lib/pkg/__pycache__/server.cpython-312.pyc"), "x").unwrap();
        assert_eq!(tree_pin(&d).unwrap(), a);
        // One byte anywhere, a new file, or a renamed one breaks the pin.
        std::fs::write(d.join("lib/pkg/server.js"), "console.log(2)").unwrap();
        assert_ne!(tree_pin(&d).unwrap(), a);
        std::fs::write(d.join("lib/pkg/server.js"), "console.log(1)").unwrap();
        assert_eq!(tree_pin(&d).unwrap(), a);
        std::fs::write(d.join("lib/extra.js"), "").unwrap();
        assert_ne!(tree_pin(&d).unwrap(), a);
        std::fs::remove_file(d.join("lib/extra.js")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(d.join("run"), std::fs::Permissions::from_mode(0o755))
                .unwrap();
            assert_ne!(
                tree_pin(&d).unwrap(),
                a,
                "making a file executable changes what can run"
            );
        }
    }
}
