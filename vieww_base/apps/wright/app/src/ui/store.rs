//! What the window remembers between launches, as plain files in
//! `~/.wright/`:
//!
//! ```text
//!  ui-open          the sessions in the row, in order (one folder per line)
//!  ui-saved         sessions closed into Saved, most recent first
//!  ui-theme         dark | light
//!  ui-projects-dir  where new sessions create their folders (~/Wright by default)
//! ```
//!
//! Lists hold folders, not copies: a session IS its project folder, and
//! its history already lives in the run store. A folder that no longer
//! exists drops out of the list the next time it is read.

use std::path::{Path, PathBuf};

use wright_runtime::config::wright_home;

pub fn file(name: &str) -> PathBuf {
    let dir = wright_home();
    let _ = std::fs::create_dir_all(&dir);
    dir.join(name)
}

pub fn read_list(name: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for l in std::fs::read_to_string(file(name)).unwrap_or_default().lines() {
        let p = PathBuf::from(l.trim());
        if !l.trim().is_empty() && p.is_dir() && !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

pub fn write_list(name: &str, paths: &[PathBuf]) {
    let text: String = paths.iter().map(|p| format!("{}\n", p.display())).collect();
    let _ = wright_runtime::store::write_atomic(&file(name), text.as_bytes());
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_else(|_| ".".into()))
}

pub fn projects_dir() -> PathBuf {
    match std::fs::read_to_string(file("ui-projects-dir")) {
        Ok(s) if !s.trim().is_empty() => PathBuf::from(s.trim()),
        _ => home().join("Wright"),
    }
}

pub fn set_projects_dir(dir: &Path) {
    let _ = std::fs::write(file("ui-projects-dir"), format!("{}\n", dir.display()));
}

/// A folder name from the first words of a description:
/// "Every Monday, read the CSVs..." -> "every-monday-read-the".
pub fn slug(text: &str) -> String {
    let words: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(4)
        .map(|w| w.to_lowercase())
        .collect();
    if words.is_empty() {
        "project".into()
    } else {
        words.join("-")
    }
}

/// Create a fresh, empty project folder for a new session.
pub fn new_project_dir(text: &str) -> Result<PathBuf, String> {
    let root = projects_dir();
    let base = slug(text);
    for n in 1..1000 {
        let name = if n == 1 { base.clone() } else { format!("{base}-{n}") };
        let dir = root.join(&name);
        if !dir.exists() {
            std::fs::create_dir_all(&dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
            return Ok(dir);
        }
    }
    Err(format!("{} already has 999 projects called '{base}'", root.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_description_becomes_a_folder_name() {
        assert_eq!(slug("Every Monday, read the CSVs in ~/sales"), "every-monday-read-the");
        assert_eq!(slug("  !!  "), "project");
    }
}
