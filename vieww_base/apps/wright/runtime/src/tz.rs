//! Loading time zones from the operating system's zone database.
//!
//! The parsing is pure and lives in `wright_core::schedule`; this module
//! only finds and reads the file. Search order: `$TZDIR`, then the
//! standard locations on Linux and macOS. A zone name is validated
//! before it touches the filesystem (no `..`, no absolute paths).

use std::path::PathBuf;

use wright_core::schedule::TimeZone;

const DIRS: &[&str] = &["/usr/share/zoneinfo", "/usr/lib/zoneinfo", "/usr/share/lib/zoneinfo", "/etc/zoneinfo"];

/// Load a zone by IANA name ("Asia/Kolkata"). "UTC" needs no file.
pub fn load(name: &str) -> Result<TimeZone, String> {
    if matches!(name, "UTC" | "Etc/UTC" | "Z" | "GMT" | "Etc/GMT") {
        return Ok(TimeZone::utc());
    }
    let valid = !name.is_empty()
        && !name.starts_with('/')
        && !name.split('/').any(|p| p == ".." || p.is_empty())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || "/_+-".contains(c));
    if !valid {
        return Err(format!("'{name}' is not a valid time zone name (like Asia/Kolkata)"));
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(d) = std::env::var("TZDIR") {
        dirs.push(PathBuf::from(d));
    }
    dirs.extend(DIRS.iter().map(PathBuf::from));
    for d in dirs {
        let p = d.join(name);
        if let Ok(bytes) = std::fs::read(&p) {
            return TimeZone::from_tzif(name, &bytes);
        }
    }
    Err(format!(
        "time zone '{name}' was not found (no zone database on this machine?) — install tzdata, or use tz=\"UTC\""
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wright_core::schedule::{days_from_civil, Cron};

    fn utc(y: i64, m: u32, d: u32, h: i64, mi: i64) -> i64 {
        days_from_civil(y, m, d) * 86400 + h * 3600 + mi * 60
    }

    #[test]
    fn real_zone_files_agree_with_the_rules() {
        let Ok(kolkata) = load("Asia/Kolkata") else {
            eprintln!("no zone database on this machine; skipping");
            return;
        };
        assert_eq!(kolkata.offset_at(utc(2026, 10, 20, 0, 0)), 5 * 3600 + 1800);
        let ny = load("America/New_York").unwrap();
        assert_eq!(ny.offset_at(utc(2026, 7, 1, 12, 0)), -4 * 3600);
        assert_eq!(ny.offset_at(utc(2026, 12, 1, 12, 0)), -5 * 3600);
        // Historic: 1990 had DST too (from the transition table, not the footer).
        assert_eq!(ny.offset_at(utc(1990, 7, 1, 12, 0)), -4 * 3600);
        let next = Cron::parse("0 8 * * *").unwrap().next_after(utc(2026, 10, 20, 10, 0), &kolkata).unwrap();
        assert_eq!(next, utc(2026, 10, 21, 2, 30));
    }

    #[test]
    fn hostile_names_never_touch_the_filesystem() {
        for bad in ["../../etc/passwd", "/etc/passwd", "Asia//Kolkata", "a b"] {
            assert!(load(bad).unwrap_err().contains("not a valid"), "{bad}");
        }
        assert_eq!(load("UTC").unwrap().offset_at(0), 0);
    }
}
