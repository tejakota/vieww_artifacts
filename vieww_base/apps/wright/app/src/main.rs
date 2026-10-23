//! The Wright app: one binary, the whole product.
//!
//!   wright                      open the UI (when built with the `ui` feature)
//!   wright new [dir]            describe a project; confirm; get a blueprint
//!   wright plan [dir]           what will happen, in plain words, and what refires
//!   wright run [dir]            fire it (--yes, --headless, --no-cache, --quiet)
//!   wright resume <run> [dir]   continue a run that crashed or was stopped
//!   wright runs [dir]           run history
//!   wright replay <run> [dir]   reconstruct a past run from its log (--at N)
//!   wright verify [run|dir]      check a receipt: signature, log,
//!                                artifacts, inputs, blueprint (ADR-025);
//!                                --key <hex> trusts that signer for once
//!   wright receipt vc [run]      the receipt as a W3C Verifiable Credential
//!                                (eddsa-jcs-2022, did:key) — checkable
//!                                without Wright; `wright verify x.vc.json`
//!   wright trust ...             list | add <name> <key> | remove | me —
//!                                the signers this machine accepts
//!   wright explain [run|dir]     the one-screen story of a run
//!   wright spend [dir] [--csv]   cost per run, agent, model and tool
//!   wright handoff [dir]         the project as a PR folder: plan,
//!                                receipts, patch — for a human
//!   wright bench [dir]           time to a checked outcome, and
//!                                interventions per run (ADR-030)
//!   wright recipe ...            list | install | sign | verify — the
//!                                curated, signed gallery (ADR-028);
//!                                only pinned publishers install
//!   wright mcp install <name> npm:<pkg>@<ver> | pypi:<pkg>==<ver>
//!                                freeze an MCP server and pin every file
//!   wright mcp list              servers, and whether their pins hold
//!   wright mcp-hash <cmd|dir>    the BLAKE3 pin for a program or a folder
//!   wright approve [dir]        list / answer pending approvals
//!   wright diff [old] [new]     what an edit refires, and why
//!   wright export [dir]         the project as a .zip, plus its package: a Python
//!                               wheel, a Rust crate or a web build (--zip --wheel
//!                               --crate --web, --out DIR)
//!   wright schedule ...         list | approve [dir] (let it run unattended)
//!                               | revoke | pause | resume | remove | install
//!   wright fleet                everything scheduled: state, next run, last
//!                               run, receipt, approvals waiting on you
//!   wright stop ["why"]         the kill switch: nothing runs anywhere until
//!   wright start                ...this releases it
//!   wright daemon               fire schedules in the background
//!   wright secret set <ref>     put a key in the OS keychain
//!   wright model ...            the built-in models: list | pull [id] | rm <id>
//!                               | ask "<prompt>" | listen <file.wav>
//!   wright mock-provider        an offline model, for demos and CI
//!   wright where [dir]          where the project's history lives
//!   wright admin ...            the administrator's console: keygen | policy
//!                               init/sign/show/check | approve [dir] |
//!                               fleet-verify [folders] [--json]
//!   wright doctor               what works on this machine

fn main() {
    use wright::cmd;

    // Office machines first, before any thread exists: the config's `env`
    // lines (proxies, mirrors) and its extra certificate authorities
    // (`tls ca-file`) go into this process's environment, so every
    // connection Wright makes — and every program it starts — sees them.
    if let Ok(cfg) = wright_runtime::config::Config::load() {
        for problem in wright_runtime::config::apply_to_process(&cfg) {
            eprintln!("warning: {problem}");
        }
    }
    // The built-in models (qwen3-1.7b, whisper-base), for every command.
    wright_infer::install();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    let code = match args.first().map(String::as_str) {
        Some("new") => cmd::new(rest),
        Some("plan") => cmd::plan(rest),
        Some("run") => cmd::run(rest),
        Some("resume") => cmd::resume(rest),
        Some("runs") => cmd::runs(rest),
        Some("replay") => cmd::replay(rest),
        Some("approve") => cmd::approve(rest),
        Some("verify") => cmd::verify(rest),
        Some("explain") => cmd::explain(rest),
        Some("handoff") => cmd::handoff(rest),
        Some("bench") => cmd::bench(rest),
        Some("recipe") | Some("recipes") => cmd::recipe(rest),
        Some("mcp-hash") => cmd::mcp_hash(rest),
        Some("mcp") => cmd::mcp(rest),
        Some("trust") => cmd::trust(rest),
        Some("diff") => cmd::diff(rest),
        Some("export") => cmd::export(rest),
        Some("schedule") => cmd::schedule(rest),
        Some("daemon") => cmd::daemon(),
        Some("stop") => cmd::stop(rest),
        Some("start") => cmd::start(rest),
        Some("fleet") => cmd::fleet(rest),
        Some("spend") => cmd::spend(rest),
        Some("receipt") => cmd::receipt(rest),
        Some("admin") => cmd::admin(rest),
        Some("secret") => cmd::secret(rest),
        Some("model") | Some("models") => cmd::model(rest),
        Some("mock-provider") => cmd::mock_provider(rest),
        Some("where") => cmd::where_(rest),
        Some("doctor") => cmd::doctor(),
        Some("ui") => open_ui(rest),
        Some("--version") | Some("-V") => {
            println!("wright {}", env!("CARGO_PKG_VERSION"));
            0
        }
        Some("help") | Some("--help") | Some("-h") => {
            help();
            0
        }
        None => open_ui(rest),
        Some(other) => {
            eprintln!("error: unknown command '{other}'\n");
            help();
            2
        }
    };
    std::process::exit(code);
}

fn open_ui(args: &[String]) -> i32 {
    #[cfg(feature = "ui")]
    {
        wright::ui::main(args)
    }
    #[cfg(not(feature = "ui"))]
    {
        let _ = args;
        help();
        0
    }
}

fn help() {
    println!(
        "wright {} — describe it, it's wrought.\n",
        env!("CARGO_PKG_VERSION")
    );
    let doc = include_str!("main.rs");
    for l in doc.lines().skip(2).take_while(|l| l.starts_with("//!")) {
        println!("{}", l.trim_start_matches("//!"));
    }
}
