//! Path safety: the one function every file tool goes through.
//!
//! A tool-facing path is project-relative. [`resolve`] turns it into an
//! absolute path that is guaranteed to stay inside the project, or
//! refuses. The rules:
//!
//! * no absolute paths, no `..` components, no NUL;
//! * no symlinks anywhere along the way — not the file, not a parent.
//!   (v0.1 canonicalized only the parent, so `innocent.txt -> /etc/hostname`
//!   read straight out of the sandbox. Review found it; the regression
//!   test is `symlink_escape_is_refused`.)
//! * `.wright/` (Wright's own state) is never reachable by agents;
//!   `.git/` is reachable only through the `git` tool.
//!
//! Symlink checks walk the path component by component with
//! `symlink_metadata`, which does not follow links.

use std::path::{Component, Path, PathBuf};

/// Why a path was refused, in words for the Event River.
pub type PathError = String;

/// Resolve `rel` inside `project` (which must already be canonical).
pub fn resolve(project: &Path, rel: &str) -> Result<PathBuf, PathError> {
    if rel.contains('\0') {
        return Err("path contains a NUL byte".into());
    }
    let rel = rel.trim();
    let rel = if rel.is_empty() { "." } else { rel };
    let p = Path::new(rel);
    let mut out = project.to_path_buf();
    let mut first = true;
    for c in p.components() {
        match c {
            Component::CurDir => continue,
            Component::Normal(name) => {
                if first && (name == ".wright" || name == ".git") {
                    return Err(format!("'{rel}' is inside {} — not reachable by agents", name.to_string_lossy()));
                }
                first = false;
                out.push(name);
                if let Ok(meta) = std::fs::symlink_metadata(&out) {
                    if meta.file_type().is_symlink() {
                        return Err(format!("'{rel}' goes through a symbolic link (refused: links can point outside the project)"));
                    }
                }
            }
            Component::ParentDir => return Err(format!("'{rel}' uses '..' (paths must stay inside the project)")),
            Component::RootDir | Component::Prefix(_) => {
                return Err(format!("'{rel}' is absolute (paths are relative to the project)"));
            }
        }
    }
    debug_assert!(out.starts_with(project));
    Ok(out)
}

/// Create missing parent directories of `target`, refusing to create
/// anything through a symlink (each level re-checked as it appears).
pub fn create_parents(project: &Path, target: &Path) -> Result<(), PathError> {
    let parent = target.parent().ok_or("no parent directory")?;
    let rel = parent.strip_prefix(project).map_err(|_| "outside the project".to_string())?;
    let mut cur = project.to_path_buf();
    for c in rel.components() {
        cur.push(c);
        match std::fs::symlink_metadata(&cur) {
            Ok(m) if m.file_type().is_symlink() => return Err("a parent directory is a symbolic link (refused)".into()),
            Ok(m) if m.is_dir() => {}
            Ok(_) => return Err(format!("'{}' exists and is not a directory", cur.display())),
            Err(_) => std::fs::create_dir(&cur).map_err(|e| format!("create {}: {e}", cur.display()))?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("wright-paths-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d.canonicalize().unwrap()
    }

    #[test]
    fn plain_paths_resolve_inside() {
        let p = project("plain");
        assert_eq!(resolve(&p, "src/a.py").unwrap(), p.join("src/a.py"));
        assert_eq!(resolve(&p, "./x").unwrap(), p.join("x"));
        assert_eq!(resolve(&p, "").unwrap(), p);
    }

    #[test]
    fn escapes_are_refused() {
        let p = project("escape");
        for bad in ["../x", "a/../../x", "/etc/passwd", ".wright/cache/k.json", ".git/config", "a\0b"] {
            assert!(resolve(&p, bad).is_err(), "{bad} must be refused");
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_refused() {
        let p = project("symlink");
        std::os::unix::fs::symlink("/etc/hostname", p.join("innocent.txt")).unwrap();
        std::os::unix::fs::symlink("/etc", p.join("cfg")).unwrap();
        assert!(resolve(&p, "innocent.txt").unwrap_err().contains("symbolic link"));
        assert!(resolve(&p, "cfg/passwd").unwrap_err().contains("symbolic link"));
    }

    #[cfg(unix)]
    #[test]
    fn parents_are_created_but_never_through_links() {
        let p = project("parents");
        create_parents(&p, &p.join("a/b/c.txt")).unwrap();
        assert!(p.join("a/b").is_dir());
        std::os::unix::fs::symlink("/tmp", p.join("lnk")).unwrap();
        assert!(create_parents(&p, &p.join("lnk/x/y.txt")).is_err());
    }
}
