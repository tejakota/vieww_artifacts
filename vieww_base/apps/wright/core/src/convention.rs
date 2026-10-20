//! The universal project convention — the skeleton every Wright
//! project has, in every language. Spec: `docs/PROJECT-CONVENTION.md`.
//!
//! This is the seed idea made concrete: "replace the hurdle of project
//! setup (folder structure, logging, testing) with agents". The part of
//! setup that is the SAME every time is not an agent's job at all — it
//! is this module: deterministic, instant, free, identical everywhere.
//! Agents (Logwright, Testwright, Firewright, the user's own) then work
//! INSIDE a structure they did not have to invent, which is what makes
//! their output navigable by other agents and a sleepy engineer at 3AM.
//!
//! The shape (every language):
//!
//! ```text
//! <project>/
//!   blueprint.kdl      the source of truth (Wright writes it; humans may too)
//!   README.md          what this is, how to run and test it
//!   AGENTS.md          the conventions, for agents and humans working ON it
//!   .gitignore
//!   artifacts/         agent inputs and outputs (`in` / `out` lines)
//!   logs/              structured JSON-lines logs (git-ignored)
//!   src/ | bin/        code, per language
//!   tests/ | test/     tests, per language — runnable with no installs
//!   .wright/           run logs, cache, objects (git-ignored, local state)
//! ```
//!
//! Zero-install rule: the generated test command for every language uses
//! only that language's standard toolchain (python unittest, node --test,
//! cargo test, POSIX sh). A fresh machine can run the tests the moment
//! the skeleton lands.
//!
//! Shipwright writes a file only if it does not exist yet. It never
//! overwrites — agent and human edits are sacred, and re-running a firing
//! is idempotent.

use crate::blueprint::model::Blueprint;

/// Languages the convention has a skeleton for.
pub const LANGUAGES: &[&str] = &["python", "node", "rust", "shell", "none"];

/// One file of the skeleton.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkeletonFile {
    /// Project-relative path with forward slashes.
    pub path: String,
    pub contents: String,
    /// Mark executable (shell scripts).
    pub executable: bool,
}

fn file(path: impl Into<String>, contents: impl Into<String>) -> SkeletonFile {
    SkeletonFile { path: path.into(), contents: contents.into(), executable: false }
}

/// Artifact names are plain file names: no separators, no dot-dot, no
/// hidden files. They become paths under `artifacts/`, so this is a
/// security check as much as a style rule.
pub fn is_safe_artifact_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('.')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// A filesystem/package-safe slug of the project name ("HN Daily Digest"
/// -> "hn_daily_digest").
pub fn slug(name: &str) -> String {
    let mut s = String::new();
    let mut last_us = true;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c.to_ascii_lowercase());
            last_us = false;
        } else if !last_us {
            s.push('_');
            last_us = true;
        }
    }
    while s.ends_with('_') {
        s.pop();
    }
    if s.is_empty() || s.as_bytes()[0].is_ascii_digit() {
        s.insert_str(0, "project_");
    }
    s
}

/// The default test command for a language ("" for none).
pub fn default_test_cmd(language: &str) -> &'static str {
    match language {
        "python" => "PYTHONPATH=src python3 -m unittest discover -s tests -t . -q",
        "node" => "node --test",
        "rust" => "cargo test --quiet",
        "shell" => "sh tests/run.sh",
        _ => "",
    }
}

/// The default run command for a language ("" for none).
pub fn default_run_cmd(language: &str, slug: &str) -> String {
    match language {
        "python" => format!("PYTHONPATH=src python3 -m {slug}"),
        "node" => "node src/index.js".into(),
        "rust" => "cargo run --quiet".into(),
        "shell" => "sh bin/main.sh".into(),
        _ => String::new(),
    }
}

/// The effective test command: the blueprint's, else the convention's.
pub fn test_cmd(bp: &Blueprint) -> String {
    if bp.project.test_cmd.is_empty() {
        default_test_cmd(&bp.project.language).to_string()
    } else {
        bp.project.test_cmd.clone()
    }
}

/// The effective run command: the blueprint's, else the convention's.
pub fn run_cmd(bp: &Blueprint) -> String {
    if bp.project.run_cmd.is_empty() {
        default_run_cmd(&bp.project.language, &slug(&bp.meta.name))
    } else {
        bp.project.run_cmd.clone()
    }
}

/// The whole skeleton for a blueprint. Pure: same blueprint, same bytes.
pub fn skeleton(bp: &Blueprint) -> Vec<SkeletonFile> {
    let lang = bp.project.language.as_str();
    let slug = slug(&bp.meta.name);
    let mut files = vec![
        file("README.md", readme(bp)),
        file("AGENTS.md", agents_md(bp)),
        file(".gitignore", gitignore(lang)),
        file("artifacts/.keep", ""),
        file("logs/.keep", ""),
    ];
    match lang {
        "python" => files.extend(python(&slug, &bp.meta.name)),
        "node" => files.extend(node(&slug, &bp.meta.name)),
        "rust" => files.extend(rust(&slug, &bp.meta.name)),
        "shell" => files.extend(shell(&bp.meta.name)),
        _ => {}
    }
    if bp.chips.ci {
        files.push(file(".github/workflows/ci.yml", ci_yaml(bp)));
    }
    files
}

fn readme(bp: &Blueprint) -> String {
    let mut s = format!("# {}\n\n{}\n\n", bp.meta.name, bp.intent.summary);
    s.push_str("This project was set up by [Wright](https://github.com/tejakota/wright): the\n");
    s.push_str("structure, logging and tests follow the universal project convention, so\n");
    s.push_str("every Wright project looks the same inside. `blueprint.kdl` is the source\n");
    s.push_str("of truth for what the project does.\n\n");
    s.push_str("## Run it\n\n```sh\nwright run blueprint.kdl\n```\n\n");
    let run = run_cmd(bp);
    if !run.is_empty() {
        s.push_str(&format!("Or run the code directly:\n\n```sh\n{run}\n```\n\n"));
    }
    let test = test_cmd(bp);
    if !test.is_empty() {
        s.push_str(&format!("## Test it\n\n```sh\n{test}\n```\n\n"));
    }
    s.push_str("## The agents\n\n");
    for a in &bp.agents {
        s.push_str(&format!("- **{}** — {}\n", a.name, a.goal));
    }
    s.push_str("\n## Layout\n\n");
    s.push_str("| Path | What lives there |\n|---|---|\n");
    s.push_str("| `artifacts/` | What agents read and write (their `in` / `out` lines) |\n");
    s.push_str("| `logs/` | Structured JSON-lines logs, one file per component |\n");
    s.push_str("| `src/`, `bin/` | Code |\n| `tests/` | Tests, runnable with no installs |\n");
    s.push_str("| `.wright/` | Run history, cache and receipts (local, git-ignored) |\n");
    s
}

fn agents_md(bp: &Blueprint) -> String {
    format!(
        "# AGENTS.md — {name}\n\n\
This project follows the Wright universal project convention. Read this\n\
before changing anything; it is short on purpose.\n\n\
## Rules\n\n\
1. `blueprint.kdl` is the source of truth. If the code and the blueprint\n   disagree, the blueprint wins.\n\
2. Agent inputs and outputs live in `artifacts/`, named exactly as the\n   blueprint's `in` / `out` lines say.\n\
3. Log through the project's log helper (structured JSON lines into\n   `logs/`). Never print secrets; log names and hashes, not payloads.\n\
4. Every behavior has a test in `tests/`, and the test command needs no\n   installs: `{test}`.\n\
5. Small files, plain names, a comment at the top of every file saying\n   what it is for. Write for a sleepy engineer at 3AM.\n\
6. Never write outside this folder. (Wright keeps its own run history\n   outside it, in ~/.wright/projects/.)\n\n\
## What this project does\n\n{intent}\n",
        name = bp.meta.name,
        test = if test_cmd(bp).is_empty() { "(none yet)".to_string() } else { test_cmd(bp) },
        intent = bp.intent.summary,
    )
}

fn gitignore(lang: &str) -> String {
    let mut s = String::from("# Wright local state and logs\n.wright/\nlogs/*\n!logs/.keep\n");
    match lang {
        "python" => s.push_str("__pycache__/\n*.pyc\n.venv/\n"),
        "node" => s.push_str("node_modules/\n"),
        "rust" => s.push_str("target/\n"),
        _ => {}
    }
    s
}

// Templates are raw strings at column 0 so the generated files are
// byte-for-byte what you read here. (A `\` line continuation in a
// normal Rust string eats the next line's indentation — fatal for
// Python. The generated project's own smoke test caught that.)

const PY_LOG: &str = r#""""Structured logging: one JSON object per line, into logs/<component>.log.

Usage:
    from .log import get_logger
    log = get_logger("fetcher")
    log.info("fetched", count=30)
"""
import json
import os
import sys
import time


class _Logger:
    def __init__(self, component):
        self.component = component
        self.dir = os.environ.get("WRIGHT_LOG_DIR", "logs")
        os.makedirs(self.dir, exist_ok=True)
        self.path = os.path.join(self.dir, component + ".log")

    def _write(self, level, event, fields):
        record = {"ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                  "level": level, "component": self.component, "event": event}
        record.update(fields)
        line = json.dumps(record, sort_keys=True)
        with open(self.path, "a", encoding="utf-8") as f:
            f.write(line + "\n")
        print(line, file=sys.stderr)

    def info(self, event, **fields):
        self._write("info", event, fields)

    def warn(self, event, **fields):
        self._write("warn", event, fields)

    def error(self, event, **fields):
        self._write("error", event, fields)


def get_logger(component):
    """A logger that writes structured lines for one component."""
    return _Logger(component)
"#;

const PY_MAIN: &str = r#""""Entry point: `python3 -m {slug}` (with PYTHONPATH=src)."""
from .log import get_logger


def main():
    log = get_logger("main")
    log.info("started", project={name:?})


if __name__ == "__main__":
    main()
"#;

const PY_SMOKE: &str = r#""""Smoke test: the package imports and its logger writes JSON lines. Replace with real tests."""
import os
import tempfile
import unittest

from {slug} import log


class SmokeTest(unittest.TestCase):
    def test_logger_writes_json_lines(self):
        with tempfile.TemporaryDirectory() as d:
            os.environ["WRIGHT_LOG_DIR"] = d
            try:
                log.get_logger("smoke").info("ok", n=1)
            finally:
                del os.environ["WRIGHT_LOG_DIR"]
            with open(os.path.join(d, "smoke.log"), encoding="utf-8") as f:
                self.assertIn('"event": "ok"', f.read())


if __name__ == "__main__":
    unittest.main()
"#;

fn python(slug: &str, name: &str) -> Vec<SkeletonFile> {
    vec![
        file(format!("src/{slug}/__init__.py"), format!("\"\"\"{name} — set up by Wright (universal project convention).\"\"\"\n")),
        file(format!("src/{slug}/log.py"), PY_LOG),
        file(format!("src/{slug}/__main__.py"), PY_MAIN.replace("{slug}", slug).replace("{name:?}", &format!("{name:?}"))),
        file("tests/__init__.py", ""),
        file("tests/test_smoke.py", PY_SMOKE.replace("{slug}", slug)),
    ]
}

fn node(slug: &str, name: &str) -> Vec<SkeletonFile> {
    vec![
        file(
            "package.json",
            format!("{{\n  \"name\": \"{}\",\n  \"version\": \"0.1.0\",\n  \"private\": true,\n  \"type\": \"module\",\n  \"scripts\": {{ \"start\": \"node src/index.js\", \"test\": \"node --test\" }}\n}}\n", slug.replace('_', "-")),
        ),
        file(
            "src/log.js",
            "// Structured logging: one JSON object per line, into logs/<component>.log.\n\
import fs from 'node:fs';\nimport path from 'node:path';\n\n\
const LOG_DIR = process.env.WRIGHT_LOG_DIR || 'logs';\n\n\
export function getLogger(component) {\n  fs.mkdirSync(LOG_DIR, { recursive: true });\n  const file = path.join(LOG_DIR, `${component}.log`);\n  const write = (level, event, fields = {}) => {\n    const line = JSON.stringify({ ts: new Date().toISOString(), level, component, event, ...fields });\n    fs.appendFileSync(file, line + '\\n');\n    console.error(line);\n  };\n  return {\n    info: (e, f) => write('info', e, f),\n    warn: (e, f) => write('warn', e, f),\n    error: (e, f) => write('error', e, f),\n  };\n}\n",
        ),
        file(
            "src/index.js",
            format!("// Entry point: `node src/index.js`.\nimport {{ getLogger }} from './log.js';\n\nconst log = getLogger('main');\nlog.info('started', {{ project: {name:?} }});\n"),
        ),
        file(
            "test/smoke.test.js",
            "// Smoke test: the logger writes JSON lines. Replace with real tests.\n\
import { test } from 'node:test';\nimport assert from 'node:assert';\nimport fs from 'node:fs';\nimport os from 'node:os';\nimport path from 'node:path';\n\n\
test('logger writes json lines', async () => {\n  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'wright-'));\n  process.env.WRIGHT_LOG_DIR = dir;\n  const { getLogger } = await import('../src/log.js?' + Date.now());\n  getLogger('smoke').info('ok', { n: 1 });\n  assert.match(fs.readFileSync(path.join(dir, 'smoke.log'), 'utf8'), /\"event\":\"ok\"/);\n});\n",
        ),
    ]
}

fn rust(slug: &str, name: &str) -> Vec<SkeletonFile> {
    vec![
        file("Cargo.toml", format!("[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n", slug.replace('_', "-"))),
        file(
            "src/log.rs",
            "//! Structured logging: one JSON object per line, into logs/<component>.log.\n\
use std::io::Write;\n\n\
/// Append one structured line for `component`. `fields` are already-JSON values.\n\
pub fn info(component: &str, event: &str, fields: &[(&str, String)]) {\n    let dir = std::env::var(\"WRIGHT_LOG_DIR\").unwrap_or_else(|_| \"logs\".into());\n    let _ = std::fs::create_dir_all(&dir);\n    let mut line = format!(\"{{\\\"level\\\":\\\"info\\\",\\\"component\\\":{:?},\\\"event\\\":{:?}\", component, event);\n    for (k, v) in fields {\n        line.push_str(&format!(\",{:?}:{}\", k, v));\n    }\n    line.push('}');\n    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(format!(\"{dir}/{component}.log\")) {\n        let _ = writeln!(f, \"{line}\");\n    }\n    eprintln!(\"{line}\");\n}\n",
        ),
        file("src/main.rs", format!("//! Entry point: `cargo run`.\nmod log;\n\nfn main() {{\n    log::info(\"main\", \"started\", &[(\"project\", format!(\"{{:?}}\", {name:?}))]);\n}}\n")),
        file("tests/smoke.rs", "//! Smoke test. Replace with real tests.\n#[test]\nfn arithmetic_still_works() {\n    assert_eq!(2 + 2, 4);\n}\n"),
    ]
}

fn shell(name: &str) -> Vec<SkeletonFile> {
    let mut main = file(
        "bin/main.sh",
        format!("#!/bin/sh\n# Entry point: `sh bin/main.sh`.\nset -eu\n. \"$(dirname \"$0\")/../lib/log.sh\"\nlog_info main started \"{}\"\n", name.replace('"', "'")),
    );
    main.executable = true;
    let mut tests = file(
        "tests/run.sh",
        "#!/bin/sh\n# Runs every tests/test_*.sh; fails if any fails.\nset -eu\ncd \"$(dirname \"$0\")/..\"\nstatus=0\nfor t in tests/test_*.sh; do\n  if sh \"$t\"; then echo \"ok   $t\"; else echo \"FAIL $t\"; status=1; fi\ndone\nexit $status\n",
    );
    tests.executable = true;
    vec![
        main,
        file(
            "lib/log.sh",
            "# Structured logging for shell: one JSON line into logs/<component>.log.\n# Usage: log_info <component> <event> [message]\nlog_info() {\n  dir=\"${WRIGHT_LOG_DIR:-logs}\"; mkdir -p \"$dir\"\n  line=$(printf '{\"level\":\"info\",\"component\":\"%s\",\"event\":\"%s\",\"msg\":\"%s\"}' \"$1\" \"$2\" \"${3:-}\")\n  echo \"$line\" >> \"$dir/$1.log\"; echo \"$line\" >&2\n}\n",
        ),
        tests,
        file(
            "tests/test_smoke.sh",
            "#!/bin/sh\n# Smoke test: the logger writes a JSON line.\nset -eu\nd=$(mktemp -d)\nWRIGHT_LOG_DIR=\"$d\" sh -c '. lib/log.sh; log_info smoke ok' 2>/dev/null\ngrep -q '\"event\":\"ok\"' \"$d/smoke.log\"\n",
        ),
    ]
}

fn ci_yaml(bp: &Blueprint) -> String {
    let test = test_cmd(bp);
    let setup = match bp.project.language.as_str() {
        "python" => "      - uses: actions/setup-python@v5\n        with: { python-version: '3.12' }\n",
        "node" => "      - uses: actions/setup-node@v4\n        with: { node-version: '22' }\n",
        "rust" => "      - uses: dtolnay/rust-toolchain@stable\n",
        _ => "",
    };
    format!(
        "# CI for {name} — generated by Wright (Firewright may refine it).\nname: ci\non: [push, pull_request]\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n{setup}      - run: {test}\n",
        name = bp.meta.name,
        test = if test.is_empty() { "echo no tests yet".into() } else { test },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blueprint::parse;

    fn bp(lang: &str) -> Blueprint {
        let src = format!(
            "meta {{ name \"HN Daily Digest\" }}\nintent {{ summary \"s\" confirmed \"v1\" }}\nproject {{ language \"{lang}\" }}\nchips {{ ci on }}\n"
        );
        parse::blueprint(&src).unwrap()
    }

    #[test]
    fn slugs_are_package_safe() {
        assert_eq!(slug("HN Daily Digest"), "hn_daily_digest");
        assert_eq!(slug("  9 lives!! "), "project_9_lives");
        assert_eq!(slug(""), "project_");
    }

    #[test]
    fn every_language_gets_the_common_shape() {
        for lang in LANGUAGES {
            let files = skeleton(&bp(lang));
            let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
            for must in ["README.md", "AGENTS.md", ".gitignore", "artifacts/.keep", "logs/.keep", ".github/workflows/ci.yml"] {
                assert!(paths.contains(&must), "{lang} is missing {must}");
            }
        }
        let py = skeleton(&bp("python"));
        assert!(py.iter().any(|f| f.path == "src/hn_daily_digest/log.py"));
        assert!(py.iter().any(|f| f.path == "tests/test_smoke.py"));
    }

    #[test]
    fn skeleton_is_deterministic() {
        assert_eq!(skeleton(&bp("node")), skeleton(&bp("node")));
    }

    #[test]
    fn artifact_names_cannot_escape() {
        assert!(is_safe_artifact_name("stories.json"));
        for bad in ["../x", "a/b", ".env", "", "a b"] {
            assert!(!is_safe_artifact_name(bad), "{bad}");
        }
    }
}
