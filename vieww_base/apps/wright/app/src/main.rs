//! The Wright app: one binary, the whole product.
//!
//!   wright                      open the UI (when built with the `ui` feature)
//!   wright new [dir]            describe a project; confirm; get a blueprint
//!   wright plan [dir]           what will happen, in plain words, and what refires
//!   wright run [dir]            fire it (--yes, --headless, --no-cache, --quiet)
//!   wright resume <run> [dir]   continue a run that crashed or was stopped
//!   wright runs [dir]           run history
//!   wright replay <run> [dir]   reconstruct a past run from its log (--at N)
//!   wright approve [dir]        list / answer pending approvals
//!   wright diff [old] [new]     what an edit refires, and why
//!   wright export [dir]         the project as a .zip, plus its package: a Python
//!                               wheel, a Rust crate or a web build (--zip --wheel
//!                               --crate --web, --out DIR)
//!   wright schedule ...         list | remove [dir] | install
//!   wright daemon               fire schedules in the background
//!   wright secret set <ref>     put a key in the OS keychain
//!   wright model ...            the built-in models: list | pull [id] | rm <id>
//!                               | ask "<prompt>" | listen <file.wav>
//!   wright mock-provider        an offline model, for demos and CI
//!   wright where [dir]          where the project's history lives
//!   wright doctor               what works on this machine

mod cmd;
#[cfg(feature = "ui")]
mod ui;

/// One line about the UI for `wright doctor`.
pub fn ui_status() -> String {
    #[cfg(feature = "ui")]
    {
        "vieww shell built in (`wright ui`)".into()
    }
    #[cfg(not(feature = "ui"))]
    {
        "not built in (build with --features ui)".into()
    }
}

fn main() {
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
        Some("diff") => cmd::diff(rest),
        Some("export") => cmd::export(rest),
        Some("schedule") => cmd::schedule(rest),
        Some("daemon") => cmd::daemon(),
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
        ui::main(args)
    }
    #[cfg(not(feature = "ui"))]
    {
        let _ = args;
        help();
        0
    }
}

fn help() {
    println!("wright {} — describe it, it's wrought.\n", env!("CARGO_PKG_VERSION"));
    let doc = include_str!("main.rs");
    for l in doc.lines().skip(2).take_while(|l| l.starts_with("//!")) {
        println!("{}", l.trim_start_matches("//!"));
    }
}
