//! Shared helpers for the engine's tests: one place for the temp
//! project builder, so test isolation is a rule, not folklore.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A fresh, unique project directory (the caller owns it). Uniqueness
/// comes from pid + counter, so parallel tests never collide and the
/// same test can build several projects.
pub fn project(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let d = std::env::temp_dir().join(format!("wright-eng-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Write a blueprint.kdl (assumed valid) into a project directory.
pub fn with_blueprint(dir: &Path, src: &str) -> PathBuf {
    std::fs::write(dir.join("blueprint.kdl"), src).unwrap();
    dir.to_path_buf()
}

/// Env vars are process-wide and the tests run in parallel: any test
/// that flips one holds this for its whole run. Without it, whichever
/// `set_var` landed last pointed every later `wright_home()` read —
/// the schedule registry, the config — at another test's home, and a
/// finished test's `remove_var` dropped the variable out from under a
/// running one.
pub fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|p| p.into_inner())
}
