//! Real MCP servers, installed by `wright mcp install`, called through
//! Wright's client: the protocol against the real thing, and the tree pin
//! against a real package.
//!
//! Skipped unless `WRIGHT_REAL_MCP_HOME` names a Wright home where
//! `wright mcp install time pypi:mcp-server-time==<ver>` has run.

use std::path::PathBuf;

use wright_core::wire::json;
use wright_runtime::config::Config;
use wright_runtime::gate::Permit;
use wright_tools::ToolCtx;

#[test]
fn an_installed_pypi_server_answers_and_a_changed_file_is_refused() {
    let Ok(home) = std::env::var("WRIGHT_REAL_MCP_HOME") else {
        eprintln!("skipped: set WRIGHT_REAL_MCP_HOME");
        return;
    };
    let text = std::fs::read_to_string(PathBuf::from(&home).join("config.kdl")).unwrap();
    let cfg = Config::parse_file_only(&text, "real").unwrap();
    let entry = cfg.mcp_for("time").expect("`wright mcp install time ...` ran").clone();
    let project = std::env::temp_dir().join(format!("wright-real-mcp-{}", std::process::id()));
    std::fs::create_dir_all(&project).unwrap();
    let mut ctx = ToolCtx::new(&project).unwrap();
    ctx.mcp_server = Some(entry.clone());
    ctx.mcp_allow = vec!["get_current_time".into()];
    let spec = wright_core::palette::find("mcp-call").unwrap();
    let grants = wright_core::blueprint::PermissionSet::none()
        .with(wright_core::blueprint::PermissionSet::SPAWN)
        .with(wright_core::blueprint::PermissionSet::NETWORK);
    let input = json::parse(r#"{"server":"time","tool":"get_current_time","args":{"timezone":"Asia/Kolkata"}}"#).unwrap();
    let out = wright_tools::run(Permit::for_tests("Clock", spec, grants), &input, &ctx).unwrap();
    assert!(out.text.contains("Asia/Kolkata"), "{}", out.text);

    // Change one file deep inside the installed package — the code `uvx`
    // would have fetched — and the call is refused before anything runs.
    let tree = PathBuf::from(entry.tree.as_ref().unwrap());
    let victim = walk(&tree).into_iter().find(|p| p.to_string_lossy().contains("mcp_server_time") && p.extension().is_some_and(|e| e == "py")).expect("the package's code");
    let original = std::fs::read(&victim).unwrap();
    std::fs::write(&victim, [original.as_slice(), b"\n# changed\n"].concat()).unwrap();
    let err = wright_tools::run(Permit::for_tests("Clock", spec, grants), &input, &ctx).unwrap_err();
    std::fs::write(&victim, &original).unwrap();
    assert!(err.to_string().contains("changed since it was pinned"), "{err}");
}

fn walk(d: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(d).unwrap().flatten() {
        let p = e.path();
        if std::fs::symlink_metadata(&p).unwrap().is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}
