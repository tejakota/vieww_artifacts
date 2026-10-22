//! A unified-diff applier — the `apply-patch` tool's engine.
//!
//! Accepts the format every model and every `git diff` produces:
//!
//! ```text
//! --- a/src/app.py
//! +++ b/src/app.py
//! @@ -3,4 +3,5 @@
//!  context line
//! -removed line
//! +added line
//! ```
//!
//! `--- /dev/null` creates a file; `+++ /dev/null` deletes one. Hunks are
//! matched by their context and removed lines, first at the stated line
//! number, then anywhere in the file (models get line numbers wrong far
//! more often than they get content wrong). If any hunk of any file
//! fails to match, NOTHING is written: [`apply`] computes every new file
//! in memory first and returns the full set of writes.

/// One file's change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePatch {
    /// Project-relative path ("a/" / "b/" prefixes stripped).
    pub path: String,
    pub create: bool,
    pub delete: bool,
    pub hunks: Vec<Hunk>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// 1-based starting line in the old file (a hint, not a contract).
    pub old_start: usize,
    /// Lines as (op, text): ' ' context, '-' remove, '+' add.
    pub lines: Vec<(char, String)>,
}

fn strip_prefix(p: &str) -> String {
    let p = p.split('\t').next().unwrap_or(p).trim();
    p.strip_prefix("a/")
        .or_else(|| p.strip_prefix("b/"))
        .unwrap_or(p)
        .to_string()
}

/// Parse a unified diff into per-file patches.
pub fn parse(text: &str) -> Result<Vec<FilePatch>, String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut files: Vec<FilePatch> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i];
        if let Some(old) = l.strip_prefix("--- ") {
            let new = lines
                .get(i + 1)
                .and_then(|n| n.strip_prefix("+++ "))
                .ok_or_else(|| format!("line {}: '---' without a following '+++'", i + 1))?;
            let create = old.trim().starts_with("/dev/null");
            let delete = new.trim().starts_with("/dev/null");
            let path = if delete {
                strip_prefix(old)
            } else {
                strip_prefix(new)
            };
            files.push(FilePatch {
                path,
                create,
                delete,
                hunks: Vec::new(),
            });
            i += 2;
            continue;
        }
        if l.starts_with("@@") {
            let file = files
                .last_mut()
                .ok_or_else(|| format!("line {}: hunk before any file header", i + 1))?;
            // @@ -a,b +c,d @@
            let old_start = l
                .split_whitespace()
                .nth(1)
                .and_then(|r| r.trim_start_matches('-').split(',').next())
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or(1);
            let mut hunk = Hunk {
                old_start,
                lines: Vec::new(),
            };
            i += 1;
            while i < lines.len() && !lines[i].starts_with("@@") && !lines[i].starts_with("--- ") {
                let hl = lines[i];
                if hl.starts_with('\\') {
                    i += 1; // "\ No newline at end of file"
                    continue;
                }
                let (op, rest) = match hl.chars().next() {
                    Some(c @ (' ' | '-' | '+')) => (c, &hl[1..]),
                    None => (' ', ""), // a blank context line whose space was trimmed
                    Some(_) => break,  // trailing prose after the diff
                };
                hunk.lines.push((op, rest.to_string()));
                i += 1;
            }
            file.hunks.push(hunk);
            continue;
        }
        i += 1; // diff --git / index lines / prose
    }
    if files.is_empty() {
        return Err("no '--- ' / '+++ ' file headers found; send a unified diff".into());
    }
    Ok(files)
}

fn find(haystack: &[String], needle: &[String], hint: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(hint.min(haystack.len()));
    }
    if needle.len() > haystack.len() {
        return None;
    }
    let fits = |at: usize| haystack[at..at + needle.len()] == *needle;
    let last = haystack.len() - needle.len();
    let hint = hint.min(last);
    if hint <= last && fits(hint) {
        return Some(hint);
    }
    // Search outward from the hint: nearest match wins.
    for d in 1..=last + 1 {
        if hint >= d && hint - d <= last && fits(hint - d) {
            return Some(hint - d);
        }
        if hint + d <= last && fits(hint + d) {
            return Some(hint + d);
        }
    }
    None
}

/// Apply one file's hunks to its old contents. Returns the new contents.
pub fn apply_to(old: &str, fp: &FilePatch) -> Result<String, String> {
    let had_trailing_newline = old.ends_with('\n') || old.is_empty();
    let mut cur: Vec<String> = old.lines().map(String::from).collect();
    let mut offset: isize = 0;
    for (n, h) in fp.hunks.iter().enumerate() {
        let before: Vec<String> = h
            .lines
            .iter()
            .filter(|(op, _)| *op != '+')
            .map(|(_, t)| t.clone())
            .collect();
        let after: Vec<String> = h
            .lines
            .iter()
            .filter(|(op, _)| *op != '-')
            .map(|(_, t)| t.clone())
            .collect();
        let hint = ((h.old_start as isize - 1) + offset).max(0) as usize;
        let at =
            find(&cur, &before, hint).ok_or_else(|| {
                format!(
                "{}: hunk {} does not match the file (its context/removed lines were not found){}",
                fp.path,
                n + 1,
                before.first().map(|l| format!("; first expected line: {l:?}")).unwrap_or_default()
            )
            })?;
        cur.splice(at..at + before.len(), after.iter().cloned());
        offset += after.len() as isize - before.len() as isize;
    }
    let mut out = cur.join("\n");
    if had_trailing_newline && !out.is_empty() {
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &str = "def add(a, b):\n    return a + b\n\n\ndef sub(a, b):\n    return a - b\n";

    #[test]
    fn modifies_with_context() {
        let patch = "--- a/src/m.py\n+++ b/src/m.py\n@@ -5,2 +5,3 @@\n def sub(a, b):\n-    return a - b\n+    \"\"\"Subtract.\"\"\"\n+    return a - b\n";
        let fps = parse(patch).unwrap();
        assert_eq!(fps[0].path, "src/m.py");
        let new = apply_to(OLD, &fps[0]).unwrap();
        assert!(new.contains("\"\"\"Subtract.\"\"\"\n    return a - b\n"));
        assert!(new.starts_with("def add"));
    }

    #[test]
    fn wrong_line_numbers_still_apply_by_content() {
        let patch = "--- a/m.py\n+++ b/m.py\n@@ -40,2 +40,2 @@\n def add(a, b):\n-    return a + b\n+    return b + a\n";
        let new = apply_to(OLD, &parse(patch).unwrap()[0]).unwrap();
        assert!(new.contains("return b + a"));
    }

    #[test]
    fn mismatch_is_an_error_with_a_reason() {
        let patch = "--- a/m.py\n+++ b/m.py\n@@ -1,1 +1,1 @@\n-nonexistent line\n+x\n";
        let err = apply_to(OLD, &parse(patch).unwrap()[0]).unwrap_err();
        assert!(err.contains("does not match"), "{err}");
    }

    #[test]
    fn create_and_delete() {
        let patch = "--- /dev/null\n+++ b/new.txt\n@@ -0,0 +1,2 @@\n+one\n+two\n--- a/old.txt\n+++ /dev/null\n@@ -1 +0,0 @@\n-gone\n";
        let fps = parse(patch).unwrap();
        assert!(fps[0].create && fps[0].path == "new.txt");
        assert_eq!(apply_to("", &fps[0]).unwrap(), "one\ntwo\n");
        assert!(fps[1].delete && fps[1].path == "old.txt");
    }

    #[test]
    fn multiple_hunks_track_offsets() {
        let old = "1\n2\n3\n4\n5\n6\n7\n8\n";
        let patch = "--- a/n\n+++ b/n\n@@ -2,1 +2,2 @@\n 2\n+2.5\n@@ -7,1 +8,1 @@\n-7\n+seven\n";
        assert_eq!(
            apply_to(old, &parse(patch).unwrap()[0]).unwrap(),
            "1\n2\n2.5\n3\n4\n5\n6\nseven\n8\n"
        );
    }
}
