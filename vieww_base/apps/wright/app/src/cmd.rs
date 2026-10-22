//! CLI commands. The CLI is a first-class door, not an escape hatch:
//! developers live here, CI runs here, and every UI action has a CLI twin.
//!
//! Output rules: ASCII only, no color (logs get piped), every number
//! human-readable, every error a sentence that says what to do next.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use wright_core::blueprint::{canon, diff as bpdiff, parse, validate, Blueprint};
use wright_core::compiler::{self, TaskKind};
use wright_core::confirm;
use wright_core::convention;
use wright_core::events::{Event, EventKind, RunId};
use wright_engine::approve::{Approver, AutoApprove, AutoDeny, StoreApprover, TerminalApprover};
use wright_engine::interview::{self, InterviewIo};
use wright_engine::{fire, FireOptions, TaskStatus, Trigger};
use wright_runtime::config::{wright_home, Config};
use wright_runtime::executor::Executor;
use wright_runtime::store::Store;

fn usd(micro: u64) -> String {
    if micro < 10_000 {
        format!("${:.4}", micro as f64 / 1e6)
    } else {
        format!("${:.2}", micro as f64 / 1e6)
    }
}

/// Split args into positionals and `--flags`.
fn split(args: &[String]) -> (Vec<&str>, Vec<&str>) {
    let mut pos = Vec::new();
    let mut flags = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if a.starts_with("--") {
            flags.push(a);
            // A flag that takes a value takes the next word with it, so
            // `--key <hex>` never reads as a project directory.
            if VALUE_FLAGS.contains(&a) {
                i += 1;
            }
        } else {
            pos.push(a);
        }
        i += 1;
    }
    (pos, flags)
}

/// Flags followed by a value (`--key <hex>`): `split` keeps the value out
/// of the positional arguments. Read the value with [`flag_values`].
const VALUE_FLAGS: [&str; 8] = [
    "--key", "--trust", "--runs", "--at", "--port", "--out", "--bin", "--name",
];

/// Every value given for a repeatable flag (`--key a --key b`).
fn flag_values<'a>(args: &'a [String], name: &str) -> Vec<&'a str> {
    args.iter()
        .enumerate()
        .filter(|(_, a)| a.as_str() == name)
        .filter_map(|(i, _)| args.get(i + 1).map(String::as_str))
        .collect()
}

fn flag_value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

/// A project directory from an argument that may be a dir or a .kdl file.
pub fn project_dir(arg: Option<&str>) -> PathBuf {
    let p = PathBuf::from(arg.unwrap_or("."));
    if p.extension().map(|e| e == "kdl").unwrap_or(false) {
        p.parent()
            .map(|d| {
                if d.as_os_str().is_empty() {
                    PathBuf::from(".")
                } else {
                    d.to_path_buf()
                }
            })
            .unwrap_or_else(|| ".".into())
    } else {
        p
    }
}

fn read_blueprint_file(path: &Path) -> Result<Blueprint, String> {
    let file = if path.is_dir() {
        path.join("blueprint.kdl")
    } else {
        path.to_path_buf()
    };
    let src = std::fs::read_to_string(&file)
        .map_err(|e| format!("cannot read {}: {e}", file.display()))?;
    parse::blueprint(&src).map_err(|e| format!("{}: {e}", file.display()))
}

fn fail(msg: impl std::fmt::Display) -> i32 {
    eprintln!("error: {msg}");
    1
}

/// Live event printer for the terminal.
pub fn print_event(e: &Event) {
    let who = e.agent.as_deref().unwrap_or("wright");
    let mark = match e.kind {
        EventKind::RunStarted => ">>",
        EventKind::RunFinished => "OK",
        EventKind::RunFailed => "!!",
        EventKind::AgentStarted => "..",
        EventKind::AgentFinished => "ok",
        EventKind::AgentFailed => "XX",
        EventKind::AgentRetrying => "~~",
        EventKind::TaskSkipped => "--",
        EventKind::ToolCalled => "  ",
        EventKind::ToolDenied => "no",
        EventKind::TokensRecorded => " $",
        EventKind::ModelText => " >",
        EventKind::ArtifactWritten => " +",
        EventKind::CacheHit => "==",
        EventKind::ApprovalRequested => "??",
        EventKind::ApprovalResolved => "?!",
        EventKind::Verdict => "VV",
        EventKind::ScheduleRegistered => "@@",
        EventKind::Note => " *",
    };
    println!("{:>4} {mark} {:<12} {}", e.seq, who, e.note);
}

// ── plan ─────────────────────────────────────────────────────────

/// `wright plan [dir|file]` — validate, compile, show what will happen.
pub fn plan(args: &[String]) -> i32 {
    let (pos, _) = split(args);
    let target = PathBuf::from(pos.first().copied().unwrap_or("."));
    let bp = match read_blueprint_file(&target) {
        Ok(bp) => bp,
        Err(e) => return fail(e),
    };
    if let Err(errs) = validate::validate(&bp) {
        eprintln!("error: the blueprint is not valid:");
        for e in errs {
            eprintln!("  - {e}");
        }
        return 1;
    }
    let graph = match compiler::compile(&bp) {
        Ok(g) => g,
        Err(e) => return fail(format!("compile failed: {e}")),
    };

    println!("WRIGHT PLAN — {}", bp.meta.name);
    println!(
        "  commit {}  ·  format {}  ·  confirmed {}",
        wright_core::hash::short(&canon::content_hash(&bp)),
        bp.meta.format,
        if bp.intent.confirmed.is_empty() {
            "(not yet)"
        } else {
            &bp.intent.confirmed
        }
    );
    println!();
    println!("WHAT WILL HAPPEN (generated from the blueprint, not by a model)");
    for l in confirm::summary(&bp, &graph) {
        println!("  {} {}", if l.attention { "!" } else { "-" }, l.text);
    }
    println!();
    println!(
        "FIRING ORDER ({} tasks in {} stages, up to {} at once)",
        graph.tasks.len(),
        graph.stages.len(),
        graph.max_parallelism()
    );
    let never = wright_engine::firing::never_cached_names(&bp);
    for (i, stage) in graph.stages.iter().enumerate() {
        let names: Vec<String> = stage
            .iter()
            .map(|&id| {
                let t = &graph.tasks[id];
                let mut n = t.name();
                if t.is_builtin() {
                    n.push_str(" (built-in)");
                } else if t.delivery {
                    n.push_str(" (delivers)");
                } else if never.contains(&t.name()) {
                    n.push_str(" (fresh)");
                }
                if let Some(m) = &t.model {
                    n.push_str(&format!(" [{}]", m.primary));
                }
                n
            })
            .collect();
        println!("  {:>2}. {}", i + 1, names.join("  |  "));
    }
    println!();
    // Refire preview against the last committed version, if any.
    let dir = project_dir(pos.first().copied());
    if let Ok(store) = Store::open(&dir) {
        if let Some(last) = store.blueprint_history().last() {
            if let Ok(prev) = store
                .blueprint_at(&last.hash)
                .and_then(|t| parse::blueprint(&t).map_err(|e| e.to_string()))
            {
                let d = bpdiff::diff(&prev, &bp);
                println!("REFIRE PREVIEW (vs. committed version {})", last.seq);
                if d.is_empty() {
                    println!("  No changes since the last firing.");
                } else {
                    println!("  {}", bpdiff::refire_plan(&d, &bp).sentence());
                }
                if !never.is_empty() {
                    println!(
                        "  Always fresh (live data or delivery): {}.",
                        never.into_iter().collect::<Vec<_>>().join(", ")
                    );
                }
            }
        }
    }
    0
}

// ── run / resume ─────────────────────────────────────────────────

fn approver_for(flags: &[&str], dir: &Path) -> Box<dyn Approver> {
    if flags.contains(&"--yes") {
        Box::new(AutoApprove)
    } else if flags.contains(&"--headless") {
        match Store::open(dir) {
            Ok(store) => Box::new(StoreApprover {
                store,
                timeout: std::time::Duration::from_secs(600),
            }),
            Err(_) => Box::new(AutoDeny),
        }
    } else if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        Box::new(TerminalApprover)
    } else {
        Box::new(AutoDeny)
    }
}

fn report(summary: &wright_engine::RunSummary) {
    println!();
    println!("RUN {} — {}", summary.run, summary.headline);
    for (name, s) in &summary.tasks {
        let detail = match s {
            TaskStatus::Ran { cost_micro_usd } => format!("ran       {}", usd(*cost_micro_usd)),
            TaskStatus::Cached { saved_micro_usd } => {
                format!("cached    saved {}", usd(*saved_micro_usd))
            }
            TaskStatus::Skipped(r) => format!("skipped   {r}"),
            TaskStatus::Failed(r) => format!("FAILED    {r}"),
        };
        println!("  {name:<12} {detail}");
    }
    println!("  receipt: {}", summary.receipt.display());
}

// ── trust: verify, explain, handoff, bench, recipes, mcp-hash ─────

/// `wright verify [run|receipt|dir]` — five checks, one verdict (ADR-025).
pub fn verify(args: &[String]) -> i32 {
    let (pos, _) = split(args);
    // Keys the person checked with the signer themselves, for this call.
    let pins: Vec<String> = flag_values(args, "--key")
        .into_iter()
        .map(String::from)
        .collect();
    let target = pos.first().copied().unwrap_or(".");
    // A standard credential (receipt.vc.json)? Its proof and its issuer.
    if target.ends_with(".vc.json") && Path::new(target).is_file() {
        return match wright_engine::credential::verify_file(Path::new(target), &pins) {
            Ok(c) => {
                println!("[OK] proof: eddsa-jcs-2022 over the canonical credential verifies");
                println!("[OK] issuer: {} — {}", c.issuer, c.who);
                println!("\nverified: run {} as its issuer signed it. (For the artifacts and the log, run `wright verify {}` on the project.)", c.run, c.run);
                0
            }
            Err(e) => {
                println!("[FAILED] {e}");
                println!("\nNOT verified.");
                1
            }
        };
    }
    // A receipt file? Verify it in place.
    if target.ends_with(".json") && Path::new(target).is_file() {
        let receipt = PathBuf::from(target);
        // Receipts live in Wright's store, outside the project: verifying
        // one needs the project dir as the second argument.
        let project = pos.get(1).copied().unwrap_or(".");
        let v =
            wright_engine::verify::receipt_file_pinned(&PathBuf::from(project), &receipt, &pins);
        print!("{}", v.report());
        return if v.ok { 0 } else { 1 };
    }
    // A run number? Verify that run of this (or the named) project.
    if let Ok(run) = target.parse::<u64>() {
        let dir = project_dir(pos.get(1).copied());
        match wright_engine::verify::project_run_pinned(&dir, Some(run), &pins) {
            Ok(v) => {
                print!("{}", v.report());
                if v.ok {
                    0
                } else {
                    1
                }
            }
            Err(e) => fail(e),
        }
    } else {
        let dir = project_dir(Some(target));
        match wright_engine::verify::project_run_pinned(&dir, None, &pins) {
            Ok(v) => {
                print!("{}", v.report());
                if v.ok {
                    0
                } else {
                    1
                }
            }
            Err(e) => fail(e),
        }
    }
}

/// `wright explain [run|dir]` — the one-screen story of a run.
pub fn explain(args: &[String]) -> i32 {
    let (pos, _) = split(args);
    let target = pos.first().copied().unwrap_or(".");
    if let Ok(run) = target.parse::<u64>() {
        let dir = project_dir(pos.get(1).copied());
        let store = match Store::open(&dir) {
            Ok(s) => s,
            Err(e) => return fail(e),
        };
        match wright_engine::explain::story_of_run(&store, run) {
            Ok(text) => {
                print!("{text}");
                0
            }
            Err(e) => fail(e),
        }
    } else {
        let dir = project_dir(Some(target));
        match wright_engine::explain::latest(&dir) {
            Ok(text) => {
                print!("{text}");
                0
            }
            Err(e) => fail(e),
        }
    }
}

/// `wright handoff [dir]` — the project as a PR folder for a human.
pub fn handoff(args: &[String]) -> i32 {
    let (pos, _) = split(args);
    let dir = project_dir(pos.first().copied());
    match wright_engine::handoff::handoff(&dir, None) {
        Ok(out) => {
            println!("handed off to {}", out.display());
            println!("HANDOFF.md says what this is; receipts/ proves what ran; project.patch is the work.");
            0
        }
        Err(e) => fail(e),
    }
}

/// `wright bench [dir] [--runs N]` — time to a checked outcome (ADR-030).
pub fn bench(args: &[String]) -> i32 {
    let (pos, _) = split(args);
    let dir = project_dir(pos.first().copied());
    let runs: u32 = flag_value(args, "--runs")
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    println!("firing {runs} time(s) and measuring (each run's receipt stays, verify it):\n");
    match wright_engine::bench::run(&dir, runs) {
        Ok(r) => {
            print!("{}", r.table());
            0
        }
        Err(e) => fail(e),
    }
}

/// `wright recipe list|install|sign|verify` — the curated, signed gallery (ADR-028).
pub fn recipe(args: &[String]) -> i32 {
    let (pos, _) = split(args);
    let Some(cmd) = pos.first().copied() else {
        println!("usage: wright recipe list | install <file.recipe.kdl> [target-dir] [--trust <publisher key>] | sign <file.recipe.kdl> | verify <file.recipe.kdl> [--trust <publisher key>]");
        return 2;
    };
    let pins: Vec<String> = flag_values(args, "--trust")
        .into_iter()
        .map(String::from)
        .collect();
    match cmd {
        "list" => {
            let (listed, problems) = wright_engine::recipe::list();
            if listed.is_empty() {
                println!(
                    "no recipes in the gallery (looked in ./recipes and {}).",
                    wright_home().join("recipes").display()
                );
            }
            for l in &listed {
                let signed = if l.recipe.signature.is_some() {
                    "signed"
                } else {
                    "UNSIGNED"
                };
                println!(
                    "[{signed}] {} — {} (v{}) by {}",
                    l.recipe.id, l.recipe.description, l.recipe.version, l.recipe.publisher
                );
                println!("        {}", l.path.display());
            }
            for p in &problems {
                println!("? {p}");
            }
            0
        }
        "verify" => {
            let Some(file) = pos.get(1).copied() else {
                return fail("usage: wright recipe verify <file.recipe.kdl>");
            };
            let r = match std::fs::read_to_string(file)
                .map_err(|e| e.to_string())
                .and_then(|t| wright_core::recipe::Recipe::parse(&t))
            {
                Ok(r) => r,
                Err(e) => return fail(e),
            };
            match r
                .verify()
                .and_then(|_| wright_engine::recipe::publisher_trusted(&r, &pins))
            {
                Ok(who) => {
                    println!("OK: recipe '{}' verifies: its signature covers its exact meaning, and the key is {who}.", r.id);
                    0
                }
                Err(e) => fail(e),
            }
        }
        "install" => {
            let Some(file) = pos.get(1).copied() else {
                return fail("usage: wright recipe install <file.recipe.kdl> [target-dir]");
            };
            let target = PathBuf::from(pos.get(2).copied().unwrap_or("."));
            match wright_engine::recipe::install(&PathBuf::from(file), &target, &pins) {
                Ok(dir) => {
                    println!("installed '{}' into {}", file, dir.display());
                    println!("run it: wright run {}", dir.display());
                    0
                }
                Err(e) => fail(e),
            }
        }
        "sign" => {
            let Some(file) = pos.get(1).copied() else {
                return fail("usage: wright recipe sign <file.recipe.kdl>");
            };
            match wright_engine::recipe::sign(&PathBuf::from(file)) {
                Ok(()) => {
                    println!("signed {} with this machine's publishing key.", file);
                    if let Ok(k) = wright_runtime::signing::SigningKey::resolve() {
                        println!(
                            "people who install it pin this public key: {}",
                            k.public_hex()
                        );
                        println!("  wright trust add \"<your name>\" {}", k.public_hex());
                    }
                    0
                }
                Err(e) => fail(e),
            }
        }
        other => fail(format!(
            "unknown recipe command '{other}' (list | install | sign | verify)"
        )),
    }
}

/// `wright trust list | add <name> <key> | remove <key> | me` — the
/// signing keys this machine accepts on receipts and recipes.
pub fn trust(args: &[String]) -> i32 {
    let (pos, _) = split(args);
    let edit = |change: &dyn Fn(&mut Config) -> Result<(), String>| -> i32 {
        match Config::load_file().and_then(|mut c| {
            change(&mut c)?;
            c.save_file()
        }) {
            Ok(path) => {
                println!("saved {}", path.display());
                0
            }
            Err(e) => fail(e),
        }
    };
    match pos.first().copied() {
        None | Some("list") => {
            let cfg = Config::load().unwrap_or_else(|_| Config::builtin());
            if cfg.trusted.is_empty() {
                println!("no keys pinned. Receipts and recipes verify only against this machine's own key until you pin one:");
                println!("  wright trust add \"<who>\" <64 hex characters>");
            }
            for t in &cfg.trusted {
                println!("{}  {}", t.key, t.name);
            }
            if let Some(k) = wright_runtime::signing::SigningKey::existing() {
                println!("{}  this machine ({})", k.public_hex(), k.provenance_str());
            }
            0
        }
        Some("me") => match wright_runtime::signing::SigningKey::resolve() {
            Ok(k) => {
                println!("{}", k.public_hex());
                if let Some(pk) =
                    wright_runtime::signing::normalize_key(&k.public_hex()).and_then(|h| {
                        let mut b = [0u8; 32];
                        for (i, x) in b.iter_mut().enumerate() {
                            *x = u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).ok()?;
                        }
                        Some(b)
                    })
                {
                    println!("{}", wright_core::jcs::did_key(&pk));
                }
                0
            }
            Err(e) => fail(e),
        },
        Some("add") => {
            let (Some(name), Some(key)) = (pos.get(1).copied(), pos.get(2).copied()) else {
                return fail("usage: wright trust add <name> <64 hex characters>");
            };
            let Some(key) = wright_runtime::signing::normalize_key(key) else {
                return fail("a key is 64 hex characters or a did:key (an Ed25519 public key)");
            };
            let name = name.to_string();
            edit(&move |c: &mut Config| {
                c.trusted.retain(|t| t.key != key);
                c.trusted.push(wright_runtime::config::TrustedKey {
                    name: name.clone(),
                    key: key.clone(),
                });
                Ok(())
            })
        }
        Some("remove") => {
            let Some(key) = pos.get(1).copied() else {
                return fail("usage: wright trust remove <key or name>");
            };
            let k = key.to_ascii_lowercase();
            edit(&move |c: &mut Config| {
                let before = c.trusted.len();
                c.trusted.retain(|t| t.key != k && t.name != key);
                if c.trusted.len() == before {
                    return Err(format!("no pinned key matches '{key}'"));
                }
                Ok(())
            })
        }
        Some(other) => fail(format!(
            "unknown trust command '{other}' (list | add | remove | me)"
        )),
    }
}

/// `wright mcp install <name> npm:<pkg>@<ver> | pypi:<pkg>==<ver> [--bin x]
/// [--allow-scripts]` and `wright mcp list` — servers frozen at an exact
/// version and pinned whole (ADR-035).
pub fn mcp(args: &[String]) -> i32 {
    let (pos, flags) = split(args);
    match pos.first().copied() {
        Some("install") => {
            let (Some(name), Some(spec)) = (pos.get(1).copied(), pos.get(2).copied()) else {
                return fail("usage: wright mcp install <name> npm:<package>@<version> | pypi:<package>==<version> [--bin <program>] [--allow-scripts]");
            };
            let source = match wright_tools::mcp_install::Source::parse(spec) {
                Ok(s) => s,
                Err(e) => return fail(e),
            };
            let bin = flag_value(args, "--bin");
            println!(
                "installing {spec} into {} ...",
                wright_home().join("mcp").join(name).display()
            );
            let installed = match wright_tools::mcp_install::install(
                name,
                &source,
                bin,
                flags.contains(&"--allow-scripts"),
                &wright_home().join("mcp"),
            ) {
                Ok(i) => i,
                Err(e) => return fail(e),
            };
            let entry = wright_runtime::config::McpEntry {
                name: name.to_string(),
                command: installed.command.display().to_string(),
                args: Vec::new(),
                hash: installed.hash.clone(),
                timeout_secs: 60,
                tree: Some(installed.tree.display().to_string()),
            };
            match Config::load_file().and_then(|mut c| {
                c.mcp_servers.retain(|m| m.name != entry.name);
                c.mcp_servers.push(entry);
                c.save_file()
            }) {
                Ok(path) => {
                    println!("pinned {name} at {}", installed.hash);
                    println!("saved to {}. A card uses it with: mcp server=\"{name}\" allow=\"<tool>,<tool>\"", path.display());
                    0
                }
                Err(e) => fail(e),
            }
        }
        Some("list") | None => {
            let cfg = Config::load().unwrap_or_else(|_| Config::builtin());
            if cfg.mcp_servers.is_empty() {
                println!(
                    "no MCP servers. Add one: wright mcp install <name> npm:<package>@<version>"
                );
            }
            for m in &cfg.mcp_servers {
                let state = match &m.tree {
                    Some(t) => match wright_tools::mcp::tree_pin(Path::new(t)) {
                        Ok(p) if p == m.hash => "pinned, intact".to_string(),
                        Ok(_) => "CHANGED since it was pinned — calls are refused".to_string(),
                        Err(e) => format!("cannot check: {e}"),
                    },
                    None => match wright_tools::mcp::pin_for_command(&m.command) {
                        Ok(p) if p == m.hash => "program pinned, intact".to_string(),
                        Ok(_) => "CHANGED since it was pinned — calls are refused".to_string(),
                        Err(e) => format!("cannot check: {e}"),
                    },
                };
                println!("{}  {}  ({state})", m.name, m.command);
            }
            0
        }
        Some(other) => fail(format!("unknown mcp command '{other}' (install | list)")),
    }
}

/// `wright mcp-hash <command>` — the pin for config.kdl (ADR-029).
pub fn mcp_hash(args: &[String]) -> i32 {
    let (pos, _) = split(args);
    let Some(command) = pos.first().copied() else {
        return fail(
            "usage: wright mcp-hash <the program config.kdl runs | an installed server folder>",
        );
    };
    if Path::new(command).is_dir() {
        return match wright_tools::mcp::tree_pin(Path::new(command)) {
            Ok(pin) => {
                println!("{pin}");
                0
            }
            Err(e) => fail(e),
        };
    }
    match wright_tools::mcp::pin_for_command(command) {
        Ok(pin) => {
            println!("{pin}");
            println!("paste it into config.kdl as hash=\"{pin}\" on the mcp line");
            0
        }
        Err(e) => fail(e),
    }
}

/// `wright run [dir] [--yes] [--headless] [--no-cache] [--quiet]`
/// Fire the project: plan, cost sentence, choice, firing, report.
pub fn run(args: &[String]) -> i32 {
    let (pos, flags) = split(args);
    let dir = project_dir(pos.first().copied());
    let quiet = flags.contains(&"--quiet");
    let yes = flags.contains(&"--yes") || flags.contains(&"--headless");

    // The spending promise (the memo's item 3): the estimate BEFORE the
    // run, as a plain sentence; the built-in model offered as a choice,
    // not hidden in a config file.
    let mut forced_config = None;
    match (load_for_cost(&dir), quiet) {
        (Ok((bp, graph, cfg)), false) => {
            let plan = wright_engine::cost::estimate(&bp, &graph, &cfg);
            println!("{}\n", plan.sentence);
            if !yes
                && plan.builtin_possible
                && plan.est_micro_usd > 0
                && std::io::IsTerminal::is_terminal(&std::io::stdin())
            {
                for (key, label) in wright_engine::cost::choices(&plan) {
                    println!("  [{key}] {label}");
                }
                print!("choice (1): ");
                let _ = std::io::Write::flush(&mut std::io::stdout());
                let mut line = String::new();
                let _ = std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut line);
                if line.trim() == "2" {
                    // Run everything on the built-in model: drop every
                    // non-builtin provider, and the fallback does the rest.
                    let mut builtin_only = cfg.clone();
                    builtin_only.providers.retain(|p| {
                        matches!(p.kind, wright_runtime::config::ProviderKind::Builtin)
                    });
                    forced_config = Some(builtin_only);
                    println!("running on the built-in model: free, slower, nothing leaves this machine.\n");
                }
            }
        }
        (Err(e), false) => return fail(e),
        _ => {}
    }

    let mut opts = FireOptions {
        trigger: Trigger::Manual,
        approver: approver_for(&flags, &dir),
        no_cache: flags.contains(&"--no-cache"),
        allow_private_network: flags.contains(&"--allow-private-network"),
        config: forced_config,
        ..FireOptions::default()
    };
    if !quiet {
        opts.listeners.push(Box::new(print_event));
    }
    match fire(&dir, opts) {
        Ok(s) => {
            report(&s);
            if s.ok {
                0
            } else {
                1
            }
        }
        Err(e) => fail(e),
    }
}

/// Read+validate the blueprint and compile it, for the cost estimate.
fn load_for_cost(
    dir: &Path,
) -> Result<(Blueprint, wright_core::compiler::TaskGraph, Config), String> {
    let bp = wright_engine::load_blueprint(dir)?;
    let graph = compiler::compile(&bp)?;
    Ok((bp, graph, Config::load()?))
}

/// `wright resume <run> [dir] [--yes]`
pub fn resume(args: &[String]) -> i32 {
    let (pos, flags) = split(args);
    let Some(run) = pos.first().and_then(|r| r.parse::<RunId>().ok()) else {
        return fail("usage: wright resume <run-number> [project-dir]");
    };
    let dir = project_dir(pos.get(1).copied());
    let opts = FireOptions {
        trigger: Trigger::Resume,
        resume: Some(run),
        approver: approver_for(&flags, &dir),
        listeners: vec![Box::new(print_event)],
        ..FireOptions::default()
    };
    match fire(&dir, opts) {
        Ok(s) => {
            report(&s);
            if s.ok {
                0
            } else {
                1
            }
        }
        Err(e) => fail(e),
    }
}

// ── history ──────────────────────────────────────────────────────

/// `wright runs [dir]` — the run history, newest last.
pub fn runs(args: &[String]) -> i32 {
    let (pos, _) = split(args);
    let dir = project_dir(pos.first().copied());
    let store = match Store::open(&dir) {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    let ids = store.runs();
    if ids.is_empty() {
        println!("No runs yet. Start one with: wright run {}", dir.display());
        return 0;
    }
    for id in ids {
        let Ok(log) = store.read_events(id) else {
            continue;
        };
        let first = log.events().first();
        let last = log.events().last();
        let when = first
            .map(|e| wright_core::schedule::rfc3339_utc((e.at_ms / 1000) as i64))
            .unwrap_or_default();
        let headline = match last {
            Some(e) if matches!(e.kind, EventKind::RunFinished | EventKind::RunFailed) => {
                e.note.clone()
            }
            Some(_) => "(did not finish — `wright resume` can continue it)".into(),
            None => "(empty)".into(),
        };
        println!("{id:>4}  {when}  {headline}");
    }
    0
}

/// `wright replay <run> [dir] [--at N]` — reconstruct a past run from its
/// log. Nothing is executed and nothing is sent: replay is a projection,
/// not a re-execution.
pub fn replay(args: &[String]) -> i32 {
    let (pos, _) = split(args);
    let Some(run) = pos.first().and_then(|r| r.parse::<RunId>().ok()) else {
        return fail("usage: wright replay <run-number> [project-dir] [--at <event>]");
    };
    let dir = project_dir(pos.get(1).copied());
    let store = match Store::open(&dir) {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    let log = match store.read_events(run) {
        Ok(l) => l,
        Err(e) => return fail(e),
    };
    let at = flag_value(args, "--at")
        .and_then(|v| v.parse().ok())
        .unwrap_or(log.len() as u64);
    let snap = log.snapshot_at(at);
    for e in snap.events() {
        print_event(e);
    }
    // The projection: state of every card at that moment.
    let mut cards: std::collections::BTreeMap<String, &str> = Default::default();
    let mut spent = 0u64;
    for e in snap.events() {
        if e.kind == EventKind::TokensRecorded {
            spent += e
                .data_value()
                .and_then(|v| v.get("cost_micro_usd").and_then(|c| c.as_u64()))
                .unwrap_or(0);
        }
        if let Some(a) = &e.agent {
            let st = match e.kind {
                EventKind::AgentStarted => "working",
                EventKind::AgentFinished => "done",
                EventKind::AgentFailed => "failed",
                EventKind::CacheHit => "cached",
                EventKind::TaskSkipped => "skipped",
                EventKind::ApprovalRequested => "waiting for approval",
                _ => continue,
            };
            cards.insert(a.clone(), st);
        }
    }
    println!();
    println!(
        "STATE AT EVENT {at} of {} (replayed from the log; nothing was re-run)",
        log.len()
    );
    for (card, st) in cards {
        println!("  {card:<12} {st}");
    }
    println!("  spent so far  {}", usd(spent));
    0
}

/// `wright approve [dir]` lists pending requests;
/// `wright approve <run> <n> yes|no [dir]` answers one.
pub fn approve(args: &[String]) -> i32 {
    let (pos, _) = split(args);
    if pos.len() >= 3 {
        let (Ok(run), Ok(n)) = (pos[0].parse::<RunId>(), pos[1].parse::<u32>()) else {
            return fail("usage: wright approve <run> <request> yes|no [project-dir]");
        };
        let yes = matches!(pos[2], "yes" | "y" | "approve");
        let dir = project_dir(pos.get(3).copied());
        return match Store::open(&dir).and_then(|s| s.answer_approval(run, n, yes)) {
            Ok(()) => {
                println!(
                    "request {run}-{n}: {}",
                    if yes { "approved" } else { "denied" }
                );
                0
            }
            Err(e) => fail(e),
        };
    }
    let dir = project_dir(pos.first().copied());
    let store = match Store::open(&dir) {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    let pending = store.pending_approvals();
    if pending.is_empty() {
        println!("Nothing is waiting for approval.");
    }
    for (run, n, agent, reason) in pending {
        println!("run {run} request {n} — {agent}");
        for l in reason.lines().take(8) {
            println!("    {l}");
        }
        println!(
            "  answer: wright approve {run} {n} yes|no {}",
            dir.display()
        );
    }
    0
}

/// `wright diff <old> <new>` or `wright diff [dir]` (vs. last committed).
pub fn diff(args: &[String]) -> i32 {
    let (pos, _) = split(args);
    let (old, new) = if pos.len() >= 2 {
        match (
            read_blueprint_file(Path::new(pos[0])),
            read_blueprint_file(Path::new(pos[1])),
        ) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => return fail(e),
        }
    } else {
        let dir = project_dir(pos.first().copied());
        let new = match read_blueprint_file(&dir) {
            Ok(b) => b,
            Err(e) => return fail(e),
        };
        let store = match Store::open(&dir) {
            Ok(s) => s,
            Err(e) => return fail(e),
        };
        let Some(last) = store.blueprint_history().last().cloned() else {
            println!("No committed version yet — the first firing runs everything.");
            return 0;
        };
        match store
            .blueprint_at(&last.hash)
            .and_then(|t| parse::blueprint(&t).map_err(|e| e.to_string()))
        {
            Ok(old) => (old, new),
            Err(e) => return fail(e),
        }
    };
    let d = bpdiff::diff(&old, &new);
    if d.is_empty() {
        println!("No changes.");
        return 0;
    }
    let plan = bpdiff::refire_plan(&d, &new);
    println!("{}", plan.sentence());
    for (who, why) in &plan.reasons {
        println!("  {who:<12} {why}");
    }
    0
}

// ── new (the interview) ──────────────────────────────────────────

struct TerminalInterview;

impl InterviewIo for TerminalInterview {
    fn say(&mut self, text: &str) {
        println!("\nwright> {text}");
    }
    fn ask(&mut self) -> Option<String> {
        print!("\nyou> ");
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        match std::io::stdin().lock().read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(line.trim().to_string()),
        }
    }
    fn confirm(&mut self, lines: &[confirm::Line]) -> bool {
        println!("\nCONFIRM — this is exactly what will exist (generated from the blueprint):");
        for l in lines {
            println!("  {} {}", if l.attention { "!" } else { "-" }, l.text);
        }
        print!("\nBuild this? [y/N] ");
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        let _ = std::io::stdin().lock().read_line(&mut line);
        matches!(line.trim(), "y" | "Y" | "yes")
    }
}

/// `wright new [dir]` — the interview, in the terminal.
pub fn new(args: &[String]) -> i32 {
    let (pos, _) = split(args);
    let dir = PathBuf::from(pos.first().copied().unwrap_or("."));
    if dir.join("blueprint.kdl").exists() {
        return fail(format!("{} already has a blueprint.kdl — use `wright plan` / `wright run`, or pick a new folder", dir.display()));
    }
    let config = match Config::load() {
        Ok(c) => c,
        Err(e) => return fail(e),
    };
    println!("wright> What should exist? (describe the outcome in your own words)");
    match interview::interview(&dir, &config, &mut TerminalInterview) {
        Ok(Some(bp)) => {
            println!(
                "\nWrote {}/blueprint.kdl ({}).",
                dir.display(),
                bp.intent.confirmed
            );
            println!("Next: wright run {}", dir.display());
            0
        }
        Ok(None) => {
            println!("\nNothing was written.");
            0
        }
        Err(e) => fail(e),
    }
}

// ── schedule / daemon ────────────────────────────────────────────

/// `wright schedule list | remove [dir] | install`
pub fn schedule(args: &[String]) -> i32 {
    let (pos, _) = split(args);
    match pos.first().copied().unwrap_or("list") {
        "list" => {
            let entries = wright_engine::scheduler::load();
            if entries.is_empty() {
                println!("No schedules. A firing of a blueprint with a scheduling chip registers one.");
            }
            for e in entries {
                let next = wright_engine::scheduler::next_fire(&e.cron, &e.tz, (wright_core::events::now_ms() / 1000) as i64)
                    .map(|t| wright_engine::scheduler::local_string(t, &e.tz))
                    .unwrap_or_else(|err| err);
                let state = wright_engine::scheduler::hold_reason(&e).unwrap_or_else(|| "active".into());
                println!("{}  [{} {}]  next: {next}  ({state})", e.project, e.cron, e.tz);
            }
            0
        }
        "remove" => {
            let dir = project_dir(pos.get(1).copied());
            let dir = dir.canonicalize().unwrap_or(dir);
            match wright_engine::scheduler::unregister(&dir) {
                Ok(true) => {
                    println!("Removed the schedule for {}.", dir.display());
                    0
                }
                Ok(false) => fail(format!("{} has no schedule", dir.display())),
                Err(e) => fail(e),
            }
        }
        "pause" | "resume" => {
            let dir = project_dir(pos.get(1).copied());
            let dir = dir.canonicalize().unwrap_or(dir);
            let state = if pos[0] == "pause" { wright_engine::scheduler::State::Paused } else { wright_engine::scheduler::State::Active };
            if state == wright_engine::scheduler::State::Active {
                if let Err(why) = wright_engine::unattended::current(&dir) {
                    return fail(format!("{why}. Give the go-ahead with `wright schedule approve {}`.", dir.display()));
                }
            }
            match wright_engine::scheduler::set_state(&dir, state) {
                Ok(true) => {
                    println!("{} {}.", if pos[0] == "pause" { "Paused" } else { "Resumed" }, dir.display());
                    0
                }
                Ok(false) => fail(format!("{} has no schedule", dir.display())),
                Err(e) => fail(e),
            }
        }
        "approve" => {
            let dir = project_dir(pos.get(1).copied());
            let dir = dir.canonicalize().unwrap_or(dir);
            match wright_engine::unattended::approve(&dir, "cli") {
                Ok(a) => {
                    println!("Approved to run unattended: {}.", dir.display());
                    println!("{}", wright_engine::unattended::describe(&a));
                    println!("If the plan changes, it waits for your go-ahead again.");
                    0
                }
                Err(e) => fail(e),
            }
        }
        "revoke" => {
            let dir = project_dir(pos.get(1).copied());
            let dir = dir.canonicalize().unwrap_or(dir);
            match wright_engine::unattended::revoke(&dir) {
                Ok(_) => {
                    println!("{} will not run unattended until you approve it again.", dir.display());
                    0
                }
                Err(e) => fail(e),
            }
        }
        "install" => {
            let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("wright"));
            match wright_engine::scheduler::install_unit(&exe) {
                Ok((path, cmd)) => {
                    println!("Wrote {}.", path.display());
                    println!("Turn it on (starts at login, fires schedules in the background):\n  {cmd}");
                    0
                }
                Err(e) => fail(e),
            }
        }
        other => fail(format!("unknown schedule command '{other}' (list | approve | revoke | pause | resume | remove | install)")),
    }
}

/// `wright spend [dir] [--csv]` — cost per run and agent; `--csv` prints
/// rows a finance team can reconcile against the provider's invoice.
pub fn spend(args: &[String]) -> i32 {
    let (pos, flags) = split(args);
    let dir = project_dir(pos.first().copied());
    let store = match Store::open(&dir) {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    let mut rows = Vec::new();
    for run in store.runs() {
        let Ok(log) = store.read_events(run) else {
            continue;
        };
        let finished = log.events().last().map(|e| e.at_ms).unwrap_or(0);
        rows.push((run, finished, wright_engine::spend::attribute(log.events())));
    }
    if flags.contains(&"--csv") {
        print!("{}", wright_engine::spend::csv(&rows));
        return 0;
    }
    if rows.is_empty() {
        println!("no runs yet");
    }
    let mut total = 0u64;
    for (run, _, s) in &rows {
        total += s.total_micro_usd;
        println!("run {run}: ${:.4}", s.total_micro_usd as f64 / 1e6);
        for l in s.lines() {
            println!("  {l}");
        }
    }
    println!("\nall runs: ${:.4}", total as f64 / 1e6);
    0
}

/// `wright receipt vc [run] [dir] [--out file]` — the run's receipt as a
/// W3C Verifiable Credential (eddsa-jcs-2022, did:key). Runs from before
/// credentials existed are issued now, from their signed receipt, but only
/// with the same key that signed it.
pub fn receipt(args: &[String]) -> i32 {
    let (pos, _) = split(args);
    if pos.first().copied() != Some("vc") {
        return fail("usage: wright receipt vc [run] [dir] [--out <file>]");
    }
    let (run, dir) = match pos.get(1).and_then(|r| r.parse::<u64>().ok()) {
        Some(r) => (Some(r), project_dir(pos.get(2).copied())),
        None => (None, project_dir(pos.get(1).copied())),
    };
    let store = match Store::open(&dir) {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    let Some(run) = run.or_else(|| store.runs().last().copied()) else {
        return fail("this project has no runs yet");
    };
    let run_dir = store.run_dir(run);
    let vc_path = run_dir.join("receipt.vc.json");
    let text = match std::fs::read_to_string(&vc_path) {
        Ok(t) => t,
        Err(_) => {
            let receipt = match std::fs::read_to_string(run_dir.join("receipt.json"))
                .map_err(|e| e.to_string())
                .and_then(|t| wright_core::wire::json::parse(&t))
            {
                Ok(r) => r,
                Err(e) => return fail(format!("run {run} has no readable receipt: {e}")),
            };
            let key = match wright_runtime::signing::SigningKey::resolve() {
                Ok(k) => k,
                Err(e) => return fail(e),
            };
            if receipt
                .get("signature")
                .and_then(|s| s.str_of("public_key"))
                != Some(key.public_hex().as_str())
            {
                return fail(format!(
                    "run {run} was signed by another key; only its signer can issue its credential"
                ));
            }
            if !wright_engine::verify::project_run(&dir, Some(run))
                .map(|v| v.ok)
                .unwrap_or(false)
            {
                return fail(format!("run {run}'s receipt does not verify; a credential for it would vouch for something false"));
            }
            match wright_engine::credential::issue(&receipt, &key) {
                Ok(vc) => {
                    let t = wright_core::wire::json::to_string(&vc);
                    let _ = wright_runtime::store::write_atomic(&vc_path, t.as_bytes());
                    t
                }
                Err(e) => return fail(e),
            }
        }
    };
    match flag_value(args, "--out") {
        Some(out) => match std::fs::write(out, &text) {
            Ok(()) => {
                println!("wrote {out} — anyone can check it with any eddsa-jcs-2022 verifier, or `wright verify {out}`");
                0
            }
            Err(e) => fail(format!("{out}: {e}")),
        },
        None => {
            println!("{text}");
            0
        }
    }
}

/// A starter organization policy (unsigned) for `wright admin policy init`.
const POLICY_TEMPLATE: &str = "policy \"Your organization\" version=1
budget max-per-run=\"2.00\" max-per-day=\"10.00\"
tools forbid=\"shell\"
network allow=\"api.anthropic.com,*.your-company.com\"
providers allow=\"anthropic,built-in\"
workers allow=\"claude-code\"
mcp allow=\"\"
require tests=true unattended=\"admin\"
";

/// `wright admin ...` — the administrator's console: keys, the signed
/// organization policy, approval of plans for unattended runs, and
/// verification of every receipt on the machine.
pub fn admin(args: &[String]) -> i32 {
    let (pos, flags) = split(args);
    let name = flag_value(args, "--name").unwrap_or("IT");
    let one = |i: usize| pos.get(i).copied();
    match (one(0), one(1)) {
        (Some("keygen"), _) => match wright_runtime::signing::new_admin_key() {
            Ok((seed, place)) => {
                let pk = wright_core::ed25519::public_key(&seed);
                let hex = wright_runtime::signing::to_hex(&pk);
                println!("A new administrator key is in {place}.");
                println!("Its public half goes in admins.kdl, in the policy folder ({}):", wright_runtime::policy::system_dir().display());
                println!("  admin \"{name}\" key=\"{hex}\"");
                println!("(as a did:key: {})", wright_core::jcs::did_key(&pk));
                0
            }
            Err(e) => fail(e),
        },
        (Some("policy"), Some("init")) => {
            let out = PathBuf::from(one(2).unwrap_or("policy.kdl"));
            if out.exists() {
                return fail(format!("{} already exists", out.display()));
            }
            match std::fs::write(&out, POLICY_TEMPLATE) {
                Ok(()) => {
                    println!("wrote {} — edit it, then `wright admin policy sign {} --name \"{name}\"`", out.display(), out.display());
                    0
                }
                Err(e) => fail(e.to_string()),
            }
        }
        (Some("policy"), Some("sign")) => {
            let Some(file) = one(2) else { return fail("usage: wright admin policy sign <policy.kdl> --name \"<admin name>\"") };
            let seed = match wright_runtime::signing::admin_key() {
                Ok(s) => s,
                Err(e) => return fail(e),
            };
            match std::fs::read_to_string(file).map_err(|e| e.to_string()).and_then(|t| wright_runtime::policy::sign(&t, &seed, name)) {
                Ok(signed) => match std::fs::write(file, signed) {
                    Ok(()) => {
                        println!("signed {file} as {name}. Put it beside admins.kdl in {}.", wright_runtime::policy::system_dir().display());
                        0
                    }
                    Err(e) => fail(e.to_string()),
                },
                Err(e) => fail(e),
            }
        }
        (Some("policy"), Some("show")) | (Some("policy"), None) => match wright_runtime::policy::load() {
            Ok(Some(p)) => {
                println!("{}", p.source.display());
                for l in p.sentences() {
                    println!("  {l}");
                }
                0
            }
            Ok(None) => {
                println!("No organization policy on this machine (looked in {}).", wright_runtime::policy::system_dir().display());
                0
            }
            Err(e) => fail(e),
        },
        (Some("policy"), Some("check")) => {
            let dir = project_dir(one(2));
            let bp = match wright_engine::load_blueprint(&dir) {
                Ok(b) => b,
                Err(e) => return fail(e),
            };
            let cfg = Config::load().unwrap_or_else(|_| Config::builtin());
            match wright_engine::org::enforce(&bp, &cfg) {
                Ok(None) => {
                    println!("No organization policy; nothing to check against.");
                    0
                }
                Ok(Some(p)) => {
                    println!("{}'s policy allows this plan.", p.org);
                    0
                }
                Err(e) => fail(e),
            }
        }
        (Some("approve"), _) => {
            let dir = project_dir(one(1));
            let dir = dir.canonicalize().unwrap_or(dir);
            let seed = match wright_runtime::signing::admin_key() {
                Ok(s) => s,
                Err(e) => return fail(e),
            };
            match wright_engine::org::approve(&dir, &seed, name) {
                Ok(h) => {
                    println!("{name} approved blueprint {} of {} for unattended runs.", wright_core::hash::short(&h), dir.display());
                    println!("A change to the plan needs a new approval.");
                    0
                }
                Err(e) => fail(e),
            }
        }
        (Some("fleet-verify"), _) => {
            let roots: Vec<PathBuf> = pos[1..].iter().map(PathBuf::from).collect();
            let checks = wright_engine::org::fleet_verify(&roots);
            print!("{}", wright_engine::org::fleet_report(&checks, flags.contains(&"--json")));
            if checks.iter().all(|c| c.ok) { 0 } else { 1 }
        }
        _ => fail(
            "usage: wright admin keygen [--name <you>]\n\
             \x20      wright admin policy init [file] | sign <file> --name <you> | show | check [dir]\n\
             \x20      wright admin approve [dir] --name <you>\n\
             \x20      wright admin fleet-verify [projects-folder ...] [--json]",
        ),
    }
}

/// `wright stop ["why"]` — the kill switch: nothing runs, sends or
/// fetches anywhere on this machine until `wright start`.
pub fn stop(args: &[String]) -> i32 {
    let (pos, _) = split(args);
    let reason = pos.join(" ");
    match wright_runtime::killswitch::engage(&reason, "cli") {
        Ok(s) => {
            println!("STOPPED: {}.", s.reason);
            println!("No firing starts; firings in flight stop at their next step; pending approvals are refused.");
            println!("Start again with: wright start");
            0
        }
        Err(e) => fail(e),
    }
}

/// `wright start` — release the kill switch.
pub fn start(_args: &[String]) -> i32 {
    match wright_runtime::killswitch::release() {
        Ok(true) => {
            println!("Started again. Schedules fire as before.");
            0
        }
        Ok(false) => {
            println!("Wright was not stopped.");
            0
        }
        Err(e) => fail(e),
    }
}

/// `wright fleet` — everything scheduled, its state and its last run.
pub fn fleet(_args: &[String]) -> i32 {
    print!(
        "{}",
        wright_engine::fleet::report(&wright_engine::fleet::rows())
    );
    0
}

/// `wright daemon` — fire schedules forever.
pub fn daemon() -> i32 {
    wright_engine::scheduler::daemon(
        |project| {
            let opts = FireOptions {
                trigger: Trigger::Schedule,
                approver: match Store::open(project) {
                    Ok(store) => Box::new(StoreApprover {
                        store,
                        timeout: std::time::Duration::from_secs(900),
                    }),
                    Err(_) => Box::new(AutoDeny),
                },
                ..FireOptions::default()
            };
            match fire(project, opts) {
                Ok(s) => println!("{}: run {} — {}", project.display(), s.run, s.headline),
                Err(e) => eprintln!("{}: {e}", project.display()),
            }
        },
        |msg| {
            println!(
                "{} {msg}",
                wright_core::schedule::rfc3339_utc((wright_core::events::now_ms() / 1000) as i64)
            )
        },
    )
}

/// `wright secret set <keychain:ref>` — read a value (hidden) and store it.
pub fn secret(args: &[String]) -> i32 {
    let (pos, _) = split(args);
    match (pos.first().copied(), pos.get(1).copied()) {
        (Some("set"), Some(reference)) => {
            if cfg!(target_os = "macos") {
                eprintln!("note: on macOS the value is passed to `security` as an argument (briefly visible to local processes).");
            }
            eprint!("value for {reference} (input is not echoed): ");
            let _ = std::process::Command::new("stty").arg("-echo").status();
            let mut v = String::new();
            let _ = std::io::stdin().lock().read_line(&mut v);
            let _ = std::process::Command::new("stty").arg("echo").status();
            eprintln!();
            match wright_runtime::secrets::store(reference, v.trim_end_matches(['\n', '\r'])) {
                Ok(()) => {
                    println!("Stored {reference} in your keychain.");
                    0
                }
                Err(e) => fail(e),
            }
        }
        (Some("env"), Some(reference)) => match wright_runtime::secrets::env_name(reference) {
            Ok(n) => {
                println!("{n}");
                0
            }
            Err(e) => fail(e),
        },
        _ => {
            fail("usage: wright secret set <keychain:path>   |   wright secret env <keychain:path>")
        }
    }
}

/// `wright mock-provider [--port N]` — the offline model, for demos and CI.
pub fn mock_provider(args: &[String]) -> i32 {
    let port = flag_value(args, "--port")
        .and_then(|p| p.parse().ok())
        .unwrap_or(7878);
    match wright_engine::mock::MockProvider::start(port) {
        Ok(m) => {
            println!("mock provider listening on {}", m.base_url());
            println!("add to {}:", wright_home().join("config.kdl").display());
            println!(
                "  provider \"mock\" kind=\"openai\" base=\"{}\" models=\"*\"",
                m.base_url()
            );
            loop {
                std::thread::park();
            }
        }
        Err(e) => fail(e),
    }
}

/// `wright where [dir]` — where Wright keeps this project's history.
pub fn where_(args: &[String]) -> i32 {
    let (pos, _) = split(args);
    let dir = project_dir(pos.first().copied());
    match Store::open(&dir) {
        Ok(s) => {
            println!("{}", s.root().display());
            0
        }
        Err(e) => fail(e),
    }
}

// ── export ───────────────────────────────────────────────────────

/// `wright export [dir] [--zip|--wheel|--crate|--web] [--out DIR]` — the
/// project as a zip, plus the package its language makes (a Python wheel,
/// a Rust crate, a built web app). With no kind given, every one that
/// applies. Packages need a last run that passed its checks and review.
pub fn export(args: &[String]) -> i32 {
    use wright_engine::export::{self, Kind};
    let out_arg = flag_value(args, "--out");
    let (pos, flags) = split(args);
    let pos: Vec<&str> = pos.into_iter().filter(|p| Some(*p) != out_arg).collect();
    let dir = project_dir(pos.first().copied());
    let dir = dir.canonicalize().unwrap_or(dir);
    let bp = match wright_engine::load_blueprint(&dir) {
        Ok(b) => b,
        Err(e) => return fail(e),
    };
    let asked: Vec<Kind> = Kind::ALL
        .into_iter()
        .filter(|k| flags.contains(&k.flag()))
        .collect();
    let kinds = if asked.is_empty() {
        export::available(&bp)
    } else {
        asked
    };
    let out = out_arg
        .map(PathBuf::from)
        .unwrap_or_else(|| export::default_out(&dir));
    let mut failed = 0;
    for kind in kinds {
        match export::export(&dir, kind, &out) {
            Ok(e) => println!(
                "{:<16} {}
{:<16} {} bytes · sha256 {}",
                kind.label(),
                e.path.display(),
                "",
                e.bytes,
                e.sha256
            ),
            Err(err) => {
                failed += 1;
                eprintln!("{:<16} not made: {err}", kind.label());
            }
        }
    }
    i32::from(failed > 0)
}

// ── model ────────────────────────────────────────────────────────

/// `wright model` — the built-in models.
pub fn model(args: &[String]) -> i32 {
    use std::io::Write as _;
    use wright_runtime::local::{self, Kind};
    let find = |id: &str| {
        local::find(id).ok_or_else(|| format!("no built-in model '{id}' (wright model list)"))
    };
    let pull = |m: &local::BuiltinModel| -> Result<(), String> {
        let mut shown = u32::MAX;
        local::pull(m, &mut |p| {
            if p.percent() != shown {
                shown = p.percent();
                print!("\r  {:<80}", local::progress_line(m, p));
                let _ = std::io::stdout().flush();
            }
        })?;
        println!(
            "\r  {:<80}",
            format!("{} ready: {}", m.id, m.path().display())
        );
        Ok(())
    };
    let result: Result<(), String> = match args.first().map(String::as_str) {
        None | Some("list") => {
            println!("Built-in models (run on this machine; no key, no account) — {}", local::models_dir().display());
            for m in &local::CATALOG {
                let state = if m.installed() { "ready".to_string() } else if m.partial() > 0 { format!("{} of {} downloaded", local::human_bytes(m.partial()), m.size_text()) } else { "not downloaded".into() };
                println!("  {:<14} {:<6} {:>8}  {:<20} {}", m.id, if m.kind == Kind::Text { "text" } else { "speech" }, m.size_text(), state, m.about);
            }
            println!("\nThey download on first use. `wright model pull` fetches them now; `builtin fallback=false` in config.kdl stops Wright using them.");
            Ok(())
        }
        Some("pull") => match args.get(1).map(String::as_str) {
            None | Some("all") => local::CATALOG.iter().try_for_each(pull),
            Some(id) => find(id).and_then(pull),
        },
        Some("rm") | Some("remove") => match args.get(1) {
            Some(id) => find(id).and_then(|m| local::remove(m).map(|_| println!("removed {}", m.id))),
            None => Err("usage: wright model rm <id>".into()),
        },
        Some("ask") => {
            let prompt = args[1..].join(" ");
            if prompt.trim().is_empty() {
                Err("usage: wright model ask \"<prompt>\"".into())
            } else {
                let mut req = wright_runtime::models::CompletionRequest::new(local::TEXT_MODEL);
                req.messages.push(wright_runtime::models::Message::user_text(prompt));
                req.max_tokens = 512;
                local::complete(&req, &mut |d| match d {
                    wright_runtime::models::Delta::Text(t) => {
                        print!("{t}");
                        let _ = std::io::stdout().flush();
                    }
                    wright_runtime::models::Delta::Status(s) => eprint!("\r  {s:<80}"),
                    _ => {}
                })
                .map(|r| println!("\n\n({} in, {} out, {})", r.usage.input_tokens, r.usage.output_tokens, r.model))
            }
        }
        Some("listen") => match args.get(1) {
            Some(file) => std::fs::read(file)
                .map_err(|e| format!("{file}: {e}"))
                .and_then(|wav| {
                    let m = find(local::SPEECH_MODEL)?;
                    if !m.installed() {
                        pull(m)?;
                    }
                    local::transcribe(local::SPEECH_MODEL, &wav)
                })
                .map(|text| println!("{text}")),
            None => Err("usage: wright model listen <file.wav>".into()),
        },
        Some(other) => Err(format!("unknown: wright model {other} (list | pull [id] | rm <id> | ask \"<prompt>\" | listen <file.wav>)")),
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("\nerror: {e}");
            1
        }
    }
}

// ── doctor ───────────────────────────────────────────────────────

const DOCTOR_BLUEPRINT: &str = "meta { name \"doctor\" }\nintent { summary \"self test\" confirmed \"v1\" }\nagent Probe { goal \"prove the machine compiles\" tools \"files-read\" permissions files-read=true }\ndeliverable kind=\"check\" { check \"doctor ran\" }";

/// `wright doctor` — what works on this machine, and what does not.
pub fn doctor() -> i32 {
    let mut failures = 0;
    let mut line = |name: &str, r: Result<String, String>| match r {
        Ok(s) => println!("  {name:<18} ok    {s}"),
        Err(e) => {
            println!("  {name:<18} NO    {e}");
            failures += 1;
        }
    };
    println!(
        "wright {} doctor — {} / {}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!();
    let home = wright_home();
    line(
        "wright home",
        std::fs::create_dir_all(&home)
            .map(|_| home.display().to_string())
            .map_err(|e| format!("{}: {e}", home.display())),
    );
    line(
        "blueprint machine",
        parse::blueprint(DOCTOR_BLUEPRINT)
            .map_err(|e| e.to_string())
            .and_then(|bp| {
                validate::validate(&bp)
                    .map_err(|e| e.join("; "))
                    .map(|_| bp)
            })
            .and_then(|bp| {
                compiler::compile(&bp)
                    .map(|g| format!("parse + validate + compile ({} tasks)", g.tasks.len()))
            }),
    );
    let ex = Executor::new();
    line(
        "executor",
        if ex.block_on(async { 41 + 1 }) == 42 {
            Ok("block_on, spawn_blocking".into())
        } else {
            Err("broken".into())
        },
    );
    line("https", wright_runtime::net::https_backend());
    line(
        "extra CAs",
        match wright_runtime::config::ca_file() {
            None => Ok("none (the system store only; add one with `tls ca-file=\"...\"` in config.kdl or WRIGHT_CA_FILE)".into()),
            Some(f) => wright_runtime::config::check_ca_file(&f).map(|n| format!("{} ({n} certificate{})", f.display(), if n == 1 { "" } else { "s" })),
        },
    );
    for (k, _) in Config::load().map(|c| c.env).unwrap_or_default() {
        let state = if std::env::var_os(&k).is_some() {
            "set"
        } else {
            "NOT set (see the warning above)"
        };
        println!("    env {k:<24} {state}");
    }
    for m in &wright_runtime::local::CATALOG {
        let state = if m.installed() {
            Ok(format!("{} ready ({}, {})", m.id, m.size_text(), m.license))
        } else if m.partial() > 0 {
            Ok(format!(
                "{}: {} of {} downloaded (wright model pull {} resumes)",
                m.id,
                wright_runtime::local::human_bytes(m.partial()),
                m.size_text(),
                m.id
            ))
        } else {
            Ok(format!(
                "{} downloads on first use ({}), or now: wright model pull {}",
                m.id,
                m.size_text(),
                m.id
            ))
        };
        line(
            if m.kind == wright_runtime::local::Kind::Text {
                "built-in text"
            } else {
                "built-in speech"
            },
            state,
        );
    }
    line("sandbox", wright_tools::sandbox::capabilities());
    line(
        "time zones",
        wright_runtime::tz::load("Asia/Kolkata").map(|_| "system zone database".into()),
    );
    let git = std::process::Command::new("git").arg("--version").output();
    line(
        "git",
        git.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .map_err(|_| "not installed (the git tool will refuse)".into()),
    );
    match Config::load() {
        Ok(cfg) => {
            line("config", Ok(cfg.source.clone()));
            for p in cfg.providers.iter().take(8) {
                let key = match &p.key {
                    None => "no key needed".to_string(),
                    Some(r) => match wright_runtime::secrets::resolve(r) {
                        Ok(_) => format!("key found ({r})"),
                        Err(_) => format!("no key yet: wright secret set {r}"),
                    },
                };
                println!("    provider {:<10} {:<36} {key}", p.name, p.base_url);
            }
        }
        Err(e) => line("config", Err(e)),
    }
    println!(
        "  tools              {} in the palette ({} implemented)",
        wright_core::palette::PALETTE.len(),
        wright_core::palette::PALETTE
            .iter()
            .filter(|t| t.implemented)
            .count()
    );
    println!(
        "  workforce          {} system agents (2 built-in, run as code)",
        wright_agents::roster().len()
    );
    println!("  ui                 {}", crate::ui_status());
    let _ = (TaskKind::Project(String::new()), convention::LANGUAGES);
    println!();
    if failures == 0 {
        println!("all checks passed.");
        0
    } else {
        println!("{failures} check(s) need attention (the rest of Wright still works).");
        1
    }
}
