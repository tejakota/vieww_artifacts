//! Byte counts and timestamps, as the user reads them.
//!
//! Formatting lives in the engine rather than the UI because the audit log
//! writes the same strings into its detail lines, and a figure that reads one
//! way on screen and another way in the exported log is a figure nobody can
//! reconcile.

/// `710 KB`, `1.4 MB`, `2.31 GB`.
///
/// One decimal at MB and two at GB, because the difference between 2.3 and 2.4
/// GB is 100 MB and worth seeing, while the difference between 710 and 710.4 KB
/// is noise.
#[must_use]
pub fn fmt_bytes(b: u64) -> String {
    const KB: f64 = 1024.0;
    let b = b as f64;
    if b < KB {
        format!("{b:.0} B")
    } else if b < KB * KB {
        format!("{:.0} KB", b / KB)
    } else if b < KB * KB * KB {
        format!("{:.1} MB", b / (KB * KB))
    } else {
        format!("{:.2} GB", b / (KB * KB * KB))
    }
}

/// `YYYY-MM-DD HH:MM`, UTC.
///
/// No `chrono`: one date format does not justify a dependency in a crate that
/// ships to a phone. The civil-date arithmetic is Howard Hinnant's
/// `civil_from_days`, which is exact for every date this app can produce.
#[must_use]
pub fn fmt_time(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let tod = secs % 86_400;

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}",
        tod / 3600,
        (tod % 3600) / 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_change_at_the_right_thresholds() {
        assert_eq!(fmt_bytes(0), "0 B");
        assert_eq!(fmt_bytes(1023), "1023 B");
        assert_eq!(fmt_bytes(1024), "1 KB");
        assert_eq!(fmt_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(fmt_bytes(1024 * 1024 * 1024), "1.00 GB");
    }

    /// The epoch, and a date past the 2000 leap-year rule that catches naive
    /// implementations.
    #[test]
    fn dates_are_civil_and_utc() {
        assert_eq!(fmt_time(0), "1970-01-01 00:00");
        assert_eq!(fmt_time(951_782_400), "2000-02-29 00:00");
        assert_eq!(fmt_time(1_755_388_800), "2025-08-17 00:00");
    }
}
