//! Schedules: cron expressions and time zones, from scratch.
//!
//! The scheduling chip says `cron="0 8 * * *" tz="Asia/Kolkata"`. To fire
//! that at the right instant we need two things the standard library does
//! not have, and the doctrine says we build:
//!
//! * [`Cron`] — the classic five-field cron language (minute, hour,
//!   day-of-month, month, day-of-week) with lists, ranges, steps and
//!   names, and the classic quirk: when BOTH day fields are restricted, a
//!   day matches if EITHER matches (that is what every cron has done
//!   since Vixie cron, so it is what users expect).
//! * [`TimeZone`] — a reader for the compiled IANA zone files (TZif,
//!   RFC 8536) that every Linux and macOS machine ships under
//!   `/usr/share/zoneinfo`, plus the POSIX TZ rule in the file's footer
//!   that covers every instant after the last listed transition.
//!
//! This module is pure: it takes bytes, not paths. The runtime reads the
//! zone file (`wright_runtime::tz`) and hands the bytes in.
//!
//! Times are Unix seconds (i64, UTC). "Local" seconds are Unix seconds
//! shifted by the zone offset — a convenient, honest representation of a
//! wall-clock reading.

// ── civil calendar ───────────────────────────────────────────────

/// Days since 1970-01-01 for a proleptic Gregorian date.
/// (Howard Hinnant's `days_from_civil`, public domain.)
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Inverse of [`days_from_civil`]: (year, month 1-12, day 1-31).
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Day of week for days-since-epoch: 0 = Sunday ... 6 = Saturday.
pub fn weekday(days: i64) -> u32 {
    (days + 4).rem_euclid(7) as u32 // 1970-01-01 was a Thursday
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
    }
}

/// Format Unix seconds (already shifted to local if you want local) as
/// `YYYY-MM-DD HH:MM`.
pub fn format_minute(secs: i64) -> String {
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", rem / 3600, (rem % 3600) / 60)
}

/// RFC 3339 UTC timestamp, e.g. `2026-10-20T13:44:09Z`.
pub fn rfc3339_utc(secs: i64) -> String {
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

// ── cron ─────────────────────────────────────────────────────────

/// A parsed five-field cron expression. Each field is a bit set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cron {
    minutes: u64, // bits 0..59
    hours: u32,   // bits 0..23
    dom: u32,     // bits 1..31
    months: u16,  // bits 1..12
    dow: u8,      // bits 0..6 (Sunday = 0)
    dom_star: bool,
    dow_star: bool,
}

const MONTH_NAMES: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
const DOW_NAMES: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

fn parse_value(s: &str, names: &[&str], name_base: u32) -> Result<u32, String> {
    if let Ok(n) = s.parse::<u32>() {
        return Ok(n);
    }
    let lower = s.to_ascii_lowercase();
    names
        .iter()
        .position(|n| *n == lower)
        .map(|i| i as u32 + name_base)
        .ok_or_else(|| format!("'{s}' is not a number or a known name"))
}

/// Parse one field into a bit set over [lo, hi].
fn parse_field(field: &str, lo: u32, hi: u32, names: &[&str], name_base: u32) -> Result<(u64, bool), String> {
    let mut bits: u64 = 0;
    let star = field == "*";
    for part in field.split(',') {
        if part.is_empty() {
            return Err("empty list item".into());
        }
        let (range, step) = match part.split_once('/') {
            Some((r, s)) => {
                let step: u32 = s.parse().map_err(|_| format!("bad step '{s}'"))?;
                if step == 0 {
                    return Err("a step of 0 never fires".into());
                }
                (r, step)
            }
            None => (part, 1),
        };
        let (start, end) = if range == "*" {
            (lo, hi)
        } else if let Some((a, b)) = range.split_once('-') {
            (parse_value(a, names, name_base)?, parse_value(b, names, name_base)?)
        } else {
            let v = parse_value(range, names, name_base)?;
            // "5/15" means "from 5 to the end, every 15".
            if part.contains('/') { (v, hi) } else { (v, v) }
        };
        if start < lo || end > hi || start > end {
            return Err(format!("'{part}' is outside {lo}-{hi}"));
        }
        let mut v = start;
        while v <= end {
            bits |= 1u64 << v;
            v += step;
        }
    }
    Ok((bits, star))
}

impl Cron {
    /// Parse `minute hour day-of-month month day-of-week`.
    /// Also accepts `@hourly`, `@daily`, `@weekly`, `@monthly`, `@yearly`.
    pub fn parse(expr: &str) -> Result<Cron, String> {
        let expanded = match expr.trim() {
            "@hourly" => "0 * * * *",
            "@daily" | "@midnight" => "0 0 * * *",
            "@weekly" => "0 0 * * 0",
            "@monthly" => "0 0 1 * *",
            "@yearly" | "@annually" => "0 0 1 1 *",
            other => other,
        };
        let fields: Vec<&str> = expanded.split_whitespace().collect();
        if fields.len() != 5 {
            return Err(format!("expected 5 fields (minute hour day month weekday), found {}", fields.len()));
        }
        let (minutes, _) = parse_field(fields[0], 0, 59, &[], 0).map_err(|e| format!("minute: {e}"))?;
        let (hours, _) = parse_field(fields[1], 0, 23, &[], 0).map_err(|e| format!("hour: {e}"))?;
        let (dom, dom_star) = parse_field(fields[2], 1, 31, &[], 0).map_err(|e| format!("day of month: {e}"))?;
        let (months, _) = parse_field(fields[3], 1, 12, &MONTH_NAMES, 1).map_err(|e| format!("month: {e}"))?;
        let (dow_raw, dow_star) = parse_field(fields[4], 0, 7, &DOW_NAMES, 0).map_err(|e| format!("weekday: {e}"))?;
        // 7 is also Sunday.
        let dow = ((dow_raw & 0x7f) | ((dow_raw >> 7) & 1)) as u8;
        Ok(Cron {
            minutes,
            hours: hours as u32,
            dom: dom as u32,
            months: months as u16,
            dow,
            dom_star,
            dow_star,
        })
    }

    fn day_matches(&self, y: i64, m: u32, d: u32) -> bool {
        if self.months & (1 << m) == 0 {
            return false;
        }
        let wd = weekday(days_from_civil(y, m, d));
        let dom_ok = self.dom & (1 << d) != 0;
        let dow_ok = self.dow & (1 << wd) != 0;
        match (self.dom_star, self.dow_star) {
            (true, true) => true,
            (true, false) => dow_ok,
            (false, true) => dom_ok,
            (false, false) => dom_ok || dow_ok, // the classic cron OR
        }
    }

    /// The first firing strictly after `after_utc`, in zone `tz`.
    /// Searches up to five years ahead (Feb 29 schedules need four).
    /// Wall-clock times that do not exist (a DST gap) are skipped;
    /// times that happen twice (a DST overlap) fire once, the first time.
    pub fn next_after(&self, after_utc: i64, tz: &TimeZone) -> Option<i64> {
        let local_now = after_utc + tz.offset_at(after_utc) as i64;
        let start_day = local_now.div_euclid(86400);
        for day in start_day..start_day + 366 * 5 + 2 {
            let (y, m, d) = civil_from_days(day);
            if d > days_in_month(y, m) || !self.day_matches(y, m, d) {
                continue;
            }
            for h in 0..24u32 {
                if self.hours & (1 << h) == 0 {
                    continue;
                }
                for min in 0..60u32 {
                    if self.minutes & (1 << min) == 0 {
                        continue;
                    }
                    let local = day * 86400 + (h as i64) * 3600 + (min as i64) * 60;
                    if let Some(utc) = tz.local_to_utc(local) {
                        if utc > after_utc {
                            return Some(utc);
                        }
                    }
                }
            }
        }
        None
    }

    /// A plain-language reading for the Confirm Gate, when the
    /// expression is one of the common shapes; None otherwise.
    pub fn describe(expr: &str) -> Option<String> {
        let f: Vec<&str> = expr.split_whitespace().collect();
        if f.len() != 5 {
            return None;
        }
        let (min, hour) = (f[0].parse::<u32>().ok(), f[1].parse::<u32>().ok());
        let at = match (min, hour) {
            (Some(mi), Some(h)) => format!("{h:02}:{mi:02}"),
            _ => return None,
        };
        match (f[2], f[3], f[4]) {
            ("*", "*", "*") => Some(format!("every day at {at}")),
            ("*", "*", "1-5") => Some(format!("every weekday at {at}")),
            ("*", "*", d) => {
                let n: usize = d.parse().ok()?;
                let names = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
                Some(format!("every {} at {at}", names.get(n)?))
            }
            (d, "*", "*") => Some(format!("on day {d} of every month at {at}")),
            _ => None,
        }
    }
}

// ── time zones ───────────────────────────────────────────────────

/// One rule of a POSIX TZ string: when DST starts or ends.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TransitionRule {
    /// Mm.w.d — month, week 1-5 (5 = last), weekday 0-6.
    MonthWeekDay { month: u32, week: u32, day: u32, time: i64 },
    /// Jn — Julian day 1-365, Feb 29 never counted.
    Julian1 { day: u32, time: i64 },
    /// n — zero-based day of year 0-365, Feb 29 counted.
    Julian0 { day: u32, time: i64 },
}

impl TransitionRule {
    /// Local-standard-time seconds of this transition in year `y`
    /// (seconds since the epoch, as a wall reading in the offset that
    /// applies just before the transition).
    fn local_secs_in_year(&self, y: i64) -> i64 {
        match *self {
            TransitionRule::MonthWeekDay { month, week, day, time } => {
                let first = days_from_civil(y, month, 1);
                let first_wd = weekday(first);
                let mut d = 1 + (day + 7 - first_wd) % 7; // first matching weekday
                d += (week - 1) * 7;
                let dim = days_in_month(y, month);
                while d > dim {
                    d -= 7; // week 5 means "last"
                }
                days_from_civil(y, month, d) * 86400 + time
            }
            TransitionRule::Julian1 { day, time } => {
                let mut doy = day as i64 - 1; // 0-based, ignoring Feb 29
                if is_leap(y) && day >= 60 {
                    doy += 1;
                }
                (days_from_civil(y, 1, 1) + doy) * 86400 + time
            }
            TransitionRule::Julian0 { day, time } => (days_from_civil(y, 1, 1) + day as i64) * 86400 + time,
        }
    }
}

/// A parsed POSIX TZ string such as `IST-5:30` or `CET-1CEST,M3.5.0,M10.5.0/3`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PosixTz {
    std_offset: i32, // seconds EAST of UTC (note: POSIX strings use WEST)
    dst: Option<(i32, TransitionRule, TransitionRule)>,
}

struct Cursor<'a> {
    s: &'a [u8],
    i: usize,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }
    fn name(&mut self) -> Result<(), String> {
        if self.peek() == Some(b'<') {
            while let Some(c) = self.peek() {
                self.i += 1;
                if c == b'>' {
                    return Ok(());
                }
            }
            return Err("unterminated <name>".into());
        }
        let start = self.i;
        while matches!(self.peek(), Some(c) if c.is_ascii_alphabetic()) {
            self.i += 1;
        }
        if self.i - start < 3 {
            return Err("zone abbreviation too short".into());
        }
        Ok(())
    }
    fn num(&mut self) -> Result<i64, String> {
        let start = self.i;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.i += 1;
        }
        std::str::from_utf8(&self.s[start..self.i])
            .ok()
            .and_then(|t| t.parse().ok())
            .ok_or_else(|| "expected a number".to_string())
    }
    /// [+-]hh[:mm[:ss]] in seconds.
    fn time(&mut self) -> Result<i64, String> {
        let sign = match self.peek() {
            Some(b'-') => {
                self.i += 1;
                -1
            }
            Some(b'+') => {
                self.i += 1;
                1
            }
            _ => 1,
        };
        let mut secs = self.num()? * 3600;
        if self.peek() == Some(b':') {
            self.i += 1;
            secs += self.num()? * 60;
            if self.peek() == Some(b':') {
                self.i += 1;
                secs += self.num()?;
            }
        }
        Ok(sign * secs)
    }
    fn rule(&mut self) -> Result<TransitionRule, String> {
        let r = match self.peek() {
            Some(b'M') => {
                self.i += 1;
                let month = self.num()? as u32;
                self.expect(b'.')?;
                let week = self.num()? as u32;
                self.expect(b'.')?;
                let day = self.num()? as u32;
                if !(1..=12).contains(&month) || !(1..=5).contains(&week) || day > 6 {
                    return Err("bad M rule".into());
                }
                TransitionRule::MonthWeekDay { month, week, day, time: 7200 }
            }
            Some(b'J') => {
                self.i += 1;
                TransitionRule::Julian1 { day: self.num()? as u32, time: 7200 }
            }
            _ => TransitionRule::Julian0 { day: self.num()? as u32, time: 7200 },
        };
        let time = if self.peek() == Some(b'/') {
            self.i += 1;
            self.time()?
        } else {
            7200
        };
        Ok(match r {
            TransitionRule::MonthWeekDay { month, week, day, .. } => TransitionRule::MonthWeekDay { month, week, day, time },
            TransitionRule::Julian1 { day, .. } => TransitionRule::Julian1 { day, time },
            TransitionRule::Julian0 { day, .. } => TransitionRule::Julian0 { day, time },
        })
    }
    fn expect(&mut self, c: u8) -> Result<(), String> {
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            Err(format!("expected '{}'", c as char))
        }
    }
}

impl PosixTz {
    fn parse(s: &str) -> Result<PosixTz, String> {
        let mut c = Cursor { s: s.as_bytes(), i: 0 };
        c.name()?;
        let std_offset = -(c.time()? as i32);
        if c.peek().is_none() {
            return Ok(PosixTz { std_offset, dst: None });
        }
        c.name()?;
        let dst_offset = if matches!(c.peek(), Some(b',') | None) {
            std_offset + 3600
        } else {
            -(c.time()? as i32)
        };
        c.expect(b',')?;
        let start = c.rule()?;
        c.expect(b',')?;
        let end = c.rule()?;
        Ok(PosixTz { std_offset, dst: Some((dst_offset, start, end)) })
    }

    fn offset_at(&self, utc: i64) -> i32 {
        let Some((dst_offset, start, end)) = &self.dst else {
            return self.std_offset;
        };
        let (y, _, _) = civil_from_days((utc + self.std_offset as i64).div_euclid(86400));
        // Transition instants in UTC: start is given in standard time,
        // end in daylight time.
        let start_utc = start.local_secs_in_year(y) - self.std_offset as i64;
        let end_utc = end.local_secs_in_year(y) - *dst_offset as i64;
        let in_dst = if start_utc < end_utc {
            utc >= start_utc && utc < end_utc // northern hemisphere
        } else {
            !(utc >= end_utc && utc < start_utc) // southern hemisphere
        };
        if in_dst {
            *dst_offset
        } else {
            self.std_offset
        }
    }
}

/// A time zone: offsets from UTC over time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeZone {
    /// IANA name, for display ("Asia/Kolkata").
    pub name: String,
    transitions: Vec<i64>,
    /// Offset (seconds east of UTC) in effect from transitions[i].
    offsets_after: Vec<i32>,
    /// Offset before the first transition.
    initial: i32,
    footer: Option<PosixTz>,
}

impl TimeZone {
    pub fn utc() -> TimeZone {
        TimeZone { name: "UTC".into(), transitions: Vec::new(), offsets_after: Vec::new(), initial: 0, footer: None }
    }

    /// A zone from a POSIX TZ rule string alone (tests, and zones the
    /// system does not ship).
    pub fn from_posix(name: &str, rule: &str) -> Result<TimeZone, String> {
        let p = PosixTz::parse(rule)?;
        Ok(TimeZone { name: name.into(), transitions: Vec::new(), offsets_after: Vec::new(), initial: p.std_offset, footer: Some(p) })
    }

    /// Parse a compiled TZif file (RFC 8536), versions 1-4.
    pub fn from_tzif(name: &str, data: &[u8]) -> Result<TimeZone, String> {
        fn be32(b: &[u8], at: usize) -> Result<u32, String> {
            b.get(at..at + 4)
                .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
                .ok_or_else(|| "truncated zone file".to_string())
        }
        if data.len() < 44 || &data[0..4] != b"TZif" {
            return Err(format!("'{name}' is not a TZif zone file"));
        }
        let version = data[4];
        let counts = |base: usize| -> Result<[usize; 6], String> {
            let mut c = [0usize; 6];
            for (i, slot) in c.iter_mut().enumerate() {
                *slot = be32(data, base + 20 + i * 4)? as usize;
            }
            Ok(c) // isutcnt, isstdcnt, leapcnt, timecnt, typecnt, charcnt
        };
        let [isut, isstd, leap, timecnt, typecnt, charcnt] = counts(0)?;
        let v1_len = timecnt * 4 + timecnt + typecnt * 6 + charcnt + leap * 8 + isstd + isut;

        // v2+ files repeat the data with 64-bit times; prefer that block.
        let (base, time_size) = if version >= b'2' { (44 + v1_len, 8usize) } else { (0, 4usize) };
        let [isut, isstd, leap, timecnt, typecnt, charcnt] = counts(base)?;
        let mut at = base + 44;
        let mut transitions = Vec::with_capacity(timecnt);
        for i in 0..timecnt {
            let p = at + i * time_size;
            let t = if time_size == 8 {
                let s = data.get(p..p + 8).ok_or("truncated zone file")?;
                i64::from_be_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]])
            } else {
                be32(data, p)? as i32 as i64
            };
            transitions.push(t);
        }
        at += timecnt * time_size;
        let type_idx: Vec<usize> =
            data.get(at..at + timecnt).ok_or("truncated zone file")?.iter().map(|b| *b as usize).collect();
        at += timecnt;
        let mut utoffs = Vec::with_capacity(typecnt);
        let mut isdst = Vec::with_capacity(typecnt);
        for i in 0..typecnt {
            let p = at + i * 6;
            utoffs.push(be32(data, p)? as i32);
            isdst.push(*data.get(p + 4).ok_or("truncated zone file")? != 0);
        }
        at += typecnt * 6 + charcnt + leap * (time_size + 4) + isstd + isut;
        if typecnt == 0 {
            return Err("zone file has no local time types".into());
        }
        let offsets_after: Vec<i32> =
            type_idx.iter().map(|&i| utoffs.get(i).copied().unwrap_or(utoffs[0])).collect();
        // RFC 8536 3.2: the first non-DST type (or type 0) applies before
        // the first transition.
        let initial = (0..typecnt).find(|&i| !isdst[i]).map(|i| utoffs[i]).unwrap_or(utoffs[0]);

        let footer = if version >= b'2' {
            let rest = data.get(at..).unwrap_or(&[]);
            let text = String::from_utf8_lossy(rest);
            let rule = text.trim_matches('\n');
            if rule.is_empty() { None } else { PosixTz::parse(rule).ok() }
        } else {
            None
        };
        Ok(TimeZone { name: name.into(), transitions, offsets_after, initial, footer })
    }

    /// Offset from UTC (seconds east) in effect at `utc`.
    pub fn offset_at(&self, utc: i64) -> i32 {
        let past_last = self.transitions.last().map(|&last| utc >= last).unwrap_or(true);
        if past_last {
            if let Some(f) = &self.footer {
                return f.offset_at(utc);
            }
        }
        match self.transitions.binary_search(&utc) {
            Ok(i) => self.offsets_after[i],
            Err(0) => self.initial,
            Err(i) => self.offsets_after[i - 1],
        }
    }

    /// The UTC instant of a wall-clock reading, or None if that reading
    /// does not exist (a spring-forward gap). Ambiguous readings (the
    /// fall-back overlap) resolve to the EARLIER instant.
    pub fn local_to_utc(&self, local: i64) -> Option<i64> {
        // Candidate offsets: those in effect a day either side.
        let mut candidates = vec![self.offset_at(local - 86400), self.offset_at(local + 86400), self.offset_at(local)];
        candidates.sort_unstable();
        candidates.dedup();
        candidates
            .into_iter()
            .map(|off| local - off as i64)
            .filter(|&utc| utc + self.offset_at(utc) as i64 == local)
            .min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(y: i64, m: u32, d: u32, h: i64, mi: i64) -> i64 {
        days_from_civil(y, m, d) * 86400 + h * 3600 + mi * 60
    }

    #[test]
    fn civil_round_trip() {
        for days in [-1000, 0, 1, 365, 10_957, 20_000, 20_746] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(weekday(days_from_civil(2026, 10, 20)), 2); // a Tuesday
        assert_eq!(rfc3339_utc(utc(2026, 10, 20, 13, 44)), "2026-10-20T13:44:00Z");
    }

    #[test]
    fn cron_fields_parse() {
        let c = Cron::parse("*/15 8-10 * * mon-fri").unwrap();
        assert_eq!(c.minutes, (1 << 0) | (1 << 15) | (1 << 30) | (1 << 45));
        assert_eq!(c.hours, (1 << 8) | (1 << 9) | (1 << 10));
        assert_eq!(c.dow, 0b0111110);
        assert!(Cron::parse("0 8 * * 7").unwrap().dow & 1 == 1, "7 is Sunday");
        assert!(Cron::parse("60 * * * *").is_err());
        assert!(Cron::parse("* * *").is_err());
        assert!(Cron::parse("*/0 * * * *").is_err());
        assert_eq!(Cron::parse("@daily").unwrap(), Cron::parse("0 0 * * *").unwrap());
    }

    #[test]
    fn eight_am_in_kolkata_is_0230_utc() {
        let tz = TimeZone::from_posix("Asia/Kolkata", "IST-5:30").unwrap();
        let c = Cron::parse("0 8 * * *").unwrap();
        // 2026-10-20 10:00 UTC = 15:30 IST, so next is tomorrow 08:00 IST.
        let next = c.next_after(utc(2026, 10, 20, 10, 0), &tz).unwrap();
        assert_eq!(next, utc(2026, 10, 21, 2, 30));
        // From just before 08:00 IST today.
        let next = c.next_after(utc(2026, 10, 20, 2, 29), &tz).unwrap();
        assert_eq!(next, utc(2026, 10, 20, 2, 30));
    }

    #[test]
    fn new_york_dst_rules_apply() {
        let tz = TimeZone::from_posix("America/New_York", "EST5EDT,M3.2.0,M11.1.0").unwrap();
        // Summer: EDT = UTC-4. Winter: EST = UTC-5.
        assert_eq!(tz.offset_at(utc(2026, 7, 1, 12, 0)), -4 * 3600);
        assert_eq!(tz.offset_at(utc(2026, 12, 1, 12, 0)), -5 * 3600);
        // 2026 spring forward: Sunday March 8, 02:00 local -> 03:00.
        assert_eq!(tz.offset_at(utc(2026, 3, 8, 6, 59)), -5 * 3600);
        assert_eq!(tz.offset_at(utc(2026, 3, 8, 7, 0)), -4 * 3600);
        // 02:30 local does not exist that day.
        assert_eq!(tz.local_to_utc(utc(2026, 3, 8, 2, 30)), None);
        // A daily 02:30 job skips the gap and fires the next day.
        let c = Cron::parse("30 2 * * *").unwrap();
        let next = c.next_after(utc(2026, 3, 8, 0, 0), &tz).unwrap();
        assert_eq!(next, utc(2026, 3, 9, 6, 30));
        // Fall back (Nov 1 2026): 01:30 happens twice; we fire the first.
        assert_eq!(tz.local_to_utc(utc(2026, 11, 1, 1, 30)), Some(utc(2026, 11, 1, 5, 30)));
    }

    #[test]
    fn southern_hemisphere_dst() {
        // Australia/Sydney: AEST-10AEDT,M10.1.0,M4.1.0/3
        let tz = TimeZone::from_posix("Australia/Sydney", "AEST-10AEDT,M10.1.0,M4.1.0/3").unwrap();
        assert_eq!(tz.offset_at(utc(2026, 1, 15, 0, 0)), 11 * 3600);
        assert_eq!(tz.offset_at(utc(2026, 7, 15, 0, 0)), 10 * 3600);
    }

    #[test]
    fn dom_and_dow_are_ored_like_classic_cron() {
        // "the 13th, or any Friday"
        let c = Cron::parse("0 0 13 * 5").unwrap();
        let tz = TimeZone::utc();
        let next = c.next_after(utc(2026, 10, 20, 0, 0), &tz).unwrap();
        assert_eq!(next, utc(2026, 10, 23, 0, 0)); // Friday Oct 23 comes before Nov 13
    }

    #[test]
    fn leap_day_schedules_find_the_next_leap_year() {
        let c = Cron::parse("0 9 29 2 *").unwrap();
        let next = c.next_after(utc(2026, 10, 20, 0, 0), &TimeZone::utc()).unwrap();
        assert_eq!(next, utc(2028, 2, 29, 9, 0));
    }

    #[test]
    fn angle_bracket_names_parse() {
        let tz = TimeZone::from_posix("Asia/Kathmandu", "<+0545>-5:45").unwrap();
        assert_eq!(tz.offset_at(0), 5 * 3600 + 45 * 60);
    }

    #[test]
    fn describe_common_shapes() {
        assert_eq!(Cron::describe("0 8 * * *").as_deref(), Some("every day at 08:00"));
        assert_eq!(Cron::describe("30 9 * * 1").as_deref(), Some("every Monday at 09:30"));
        assert_eq!(Cron::describe("*/5 * * * *"), None);
    }
}
