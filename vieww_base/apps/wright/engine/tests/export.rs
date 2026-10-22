//! Exports, checked by the tools that will read them: Python's `zipfile`
//! and pip for zips and wheels, cargo for crates, a real Vite build for
//! the web (only with `WRIGHT_TEST_NPM=1`; it downloads packages).

use std::path::{Path, PathBuf};
use std::process::Command;

use wright_core::blueprint::parse;
use wright_core::convention;
use wright_engine::export::{self, Kind};
use wright_runtime::store::Store;

fn have(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// A fresh project of `lang`: its skeleton and blueprint, under a private
/// WRIGHT_HOME. `verified` records a passing run (what the result screen
/// would show), so packages are allowed.
fn project(lang: &str, verified: bool) -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "wright-export-{lang}-{verified}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&base);
    std::env::set_var("WRIGHT_HOME", base.join("home"));
    let dir = base.join("projects").join(format!("{lang}-thing"));
    std::fs::create_dir_all(&dir).unwrap();
    let text = format!(
        "meta {{ name \"Export Check {lang}\" }}\nintent {{ summary \"Count words, export cleanly.\" confirmed \"v1\" }}\nproject {{ language \"{lang}\" }}\n\
agent Writer {{ goal \"write the code\" tools \"files-write\" permissions files-write=true }}\ndeliverable kind=\"code\" {{ check \"tests pass\" }}\n"
    );
    std::fs::write(dir.join("blueprint.kdl"), &text).unwrap();
    for f in convention::skeleton(&parse::blueprint(&text).unwrap()) {
        let p = dir.join(&f.path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, &f.contents).unwrap();
    }
    // Things an export must leave out.
    std::fs::write(dir.join("logs/main.log"), "{\"level\":\"info\"}\n").unwrap();
    std::fs::create_dir_all(dir.join("node_modules/left-pad")).unwrap();
    std::fs::write(dir.join("node_modules/left-pad/index.js"), "x").unwrap();
    if verified {
        let store = Store::open(&dir).unwrap();
        let run = store.new_run().unwrap().run();
        std::fs::write(
            store.run_dir(run).join("receipt.json"),
            "{\"ok\":true,\"headline\":\"done\"}",
        )
        .unwrap();
    }
    dir
}

fn python(code: &str) -> String {
    let out = Command::new("python3")
        .args(["-I", "-c", code])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "python failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn names(zip: &Path) -> Vec<String> {
    python(&format!(
        "import zipfile; print('\\n'.join(zipfile.ZipFile({:?}).namelist()))",
        zip.display().to_string()
    ))
    .lines()
    .map(String::from)
    .collect()
}

// One test, in order: the process environment (WRIGHT_HOME) is shared.
#[test]
fn exports_are_what_other_tools_expect() {
    if !have("python3") {
        eprintln!("python3 not installed; skipping");
        return;
    }

    // ── the source zip: always allowed, never the junk, same bytes twice ──
    let dir = project("python", false);
    let out = export::default_out(&dir);
    assert!(out.ends_with("Exports/python-thing"), "{}", out.display());
    let a = export::export(&dir, Kind::Zip, &out).unwrap();
    let b = export::export(&dir, Kind::Zip, &out).unwrap();
    assert_eq!(
        a.sha256, b.sha256,
        "the same project exports the same bytes"
    );
    let listed = names(&a.path);
    assert!(
        listed.contains(&"export-check-python/blueprint.kdl".to_string()),
        "{listed:?}"
    );
    assert!(listed.contains(&"export-check-python/src/export_check_python/__main__.py".to_string()));
    assert!(listed.contains(&"export-check-python/logs/.keep".to_string()));
    assert!(
        !listed
            .iter()
            .any(|n| n.contains("main.log") || n.contains("node_modules")),
        "logs and installs stay out: {listed:?}"
    );
    assert!(
        !dir.join("Exports").exists(),
        "nothing was written inside the project"
    );

    // ── packages need proof ──
    let e = export::export(&dir, Kind::Wheel, &out).unwrap_err();
    assert!(e.contains("not run yet"), "{e}");
    assert!(export::export(&dir, Kind::Crate, &out)
        .unwrap_err()
        .contains("not available"));

    // ── the wheel: pip installs it, and its command runs ──
    let dir = project("python", true);
    let wheel = export::export(&dir, Kind::Wheel, &export::default_out(&dir)).unwrap();
    assert!(
        wheel
            .path
            .ends_with("export_check_python-0.1.0-py3-none-any.whl"),
        "{}",
        wheel.path.display()
    );
    // RECORD's hashes are the files' hashes.
    python(&format!(
        "import zipfile,hashlib,base64\nz=zipfile.ZipFile({:?})\nfor line in z.read('export_check_python-0.1.0.dist-info/RECORD').decode().splitlines():\n    p,h,n=line.split(',')\n    if not h: continue\n    d=z.read(p)\n    assert h=='sha256='+base64.urlsafe_b64encode(hashlib.sha256(d).digest()).rstrip(b'=').decode(), p\n    assert int(n)==len(d), p\n",
        wheel.path.display().to_string()
    ));
    let pip_ok = Command::new("python3")
        .args(["-m", "pip", "--version"])
        .output()
        .is_ok_and(|o| o.status.success());
    if pip_ok {
        let target = dir.parent().unwrap().join("site");
        let out = Command::new("python3")
            .args([
                "-m",
                "pip",
                "install",
                "--quiet",
                "--no-deps",
                "--no-index",
                "--target",
            ])
            .arg(&target)
            .arg(&wheel.path)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "pip refused the wheel:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let logs = dir.parent().unwrap().join("logs");
        let ran = Command::new("python3")
            .args(["-m", "export_check_python"])
            .env("PYTHONPATH", &target)
            .env("WRIGHT_LOG_DIR", &logs)
            .output()
            .unwrap();
        assert!(
            ran.status.success(),
            "{}",
            String::from_utf8_lossy(&ran.stderr)
        );
        assert!(
            logs.join("main.log").exists(),
            "the installed package ran and logged"
        );
        assert!(
            target.join("bin/export-check-python").exists() || cfg!(windows),
            "the console script was installed"
        );
    }

    // ── the crate: cargo packages it ──
    if have("cargo") {
        let dir = project("rust", true);
        let c = export::export(&dir, Kind::Crate, &export::default_out(&dir)).unwrap();
        assert!(
            c.path.ends_with("export-check-rust-0.1.0.crate"),
            "{}",
            c.path.display()
        );
        assert!(c.bytes > 100);
        assert!(
            !dir.join("target/package").exists(),
            "the project's own target/ was not used"
        );
    }

    // ── the web build: only when asked (downloads packages) ──
    if std::env::var("WRIGHT_TEST_NPM").is_ok() && have("npm") {
        let dir = project("react", true);
        let w = export::export(&dir, Kind::Web, &export::default_out(&dir)).unwrap();
        let listed = names(&w.path);
        assert!(
            listed.contains(&"export-check-react/index.html".to_string()),
            "{listed:?}"
        );
        assert!(
            listed
                .iter()
                .any(|n| n.starts_with("export-check-react/assets/") && n.ends_with(".js")),
            "{listed:?}"
        );
    }
}
