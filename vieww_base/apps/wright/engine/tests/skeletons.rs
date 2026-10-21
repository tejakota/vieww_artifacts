//! Every language's skeleton must pass its own tests on a fresh machine
//! with nothing installed beyond the language's standard toolchain.
//! (This test found the v0.2-dev Python template bug: a Rust string
//! continuation had eaten the class's indentation.)

use wright_core::blueprint::parse;
use wright_core::convention;

fn check(lang: &str, tool: &str) {
    if std::process::Command::new(tool).arg("--version").output().is_err() {
        eprintln!("{tool} not installed; skipping {lang}");
        return;
    }
    let bp = parse::blueprint(&format!(
        "meta {{ name \"Skeleton Check {lang}\" }}\nintent {{ summary \"s\" confirmed \"v1\" }}\nproject {{ language \"{lang}\" }}\n"
    ))
    .unwrap();
    let dir = std::env::temp_dir().join(format!("wright-skel-{lang}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for f in convention::skeleton(&bp) {
        let p = dir.join(&f.path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, &f.contents).unwrap();
    }
    for cmd in [convention::test_cmd(&bp), convention::run_cmd(&bp)] {
        let out = std::process::Command::new("sh").arg("-c").arg(&cmd).current_dir(&dir).output().unwrap();
        assert!(
            out.status.success(),
            "{lang}: `{cmd}` failed\n{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    assert!(dir.join("logs/main.log").exists(), "{lang}: running it wrote a structured log line");
}

#[test]
fn python_skeleton_runs_and_tests_green() {
    check("python", "python3");
}

#[test]
fn node_skeleton_runs_and_tests_green() {
    check("node", "node");
}

#[test]
fn shell_skeleton_runs_and_tests_green() {
    check("shell", "sh");
}

#[test]
fn rust_skeleton_runs_and_tests_green() {
    check("rust", "cargo");
}

/// React: the logic tests need nothing installed (rule 1 holds). The
/// app itself needs npm; its build is checked only when asked
/// (`WRIGHT_TEST_NPM=1`), because it downloads packages.
#[test]
fn react_skeleton_tests_green_without_installs() {
    if std::process::Command::new("node").arg("--version").output().is_err() {
        eprintln!("node not installed; skipping react");
        return;
    }
    let bp = parse::blueprint("meta { name \"Skeleton Check react\" }\nintent { summary \"s\" confirmed \"v1\" }\nproject { language \"react\" }\n").unwrap();
    let dir = std::env::temp_dir().join(format!("wright-skel-react-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for f in convention::skeleton(&bp) {
        let p = dir.join(&f.path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, &f.contents).unwrap();
    }
    let run = |cmd: &str| {
        let out = std::process::Command::new("sh").arg("-c").arg(cmd).current_dir(&dir).output().unwrap();
        assert!(out.status.success(), "react: `{cmd}` failed\n{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    };
    run(&convention::test_cmd(&bp));
    assert!(!dir.join("node_modules").exists(), "the tests ran with nothing installed");
    if std::env::var("WRIGHT_TEST_NPM").is_ok() {
        run("npm install --no-audit --no-fund && npm run build");
        assert!(dir.join("dist/index.html").exists(), "vite built the app");
    }
}
