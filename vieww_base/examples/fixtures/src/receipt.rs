//! Machine-readable benchmark receipts.
//!
//! # Why a receipt and not the printed table
//!
//! The audit's first finding about this gallery's numbers was not that they
//! were wrong but that they could not be checked: "~20–22 ms on the reference
//! laptop" is a sentence, and a sentence does not say which laptop, which
//! compiler, which `target-cpu`, which commit, or whether the 22 was the best
//! of three or the worst. Two runs of the animation stress suite produced two
//! different p95s and nothing recorded enough to say why.
//!
//! A receipt records, with every run:
//!
//! * the **source** — commit, branch, and whether the tree was dirty;
//! * the **build** — the rustc that compiled *this binary*, profile,
//!   opt-level, target and the exact `RUSTFLAGS` (baked in by `build.rs`), plus
//!   which SIMD features the binary was compiled to assume;
//! * the **machine** — OS, kernel, CPU model, logical cores, memory, the
//!   frequency governor (a `powersave` governor alone moves these numbers by
//!   more than most optimisations do) and the SIMD features the CPU offers;
//! * the **GPU**, when one was used — name, type and backend as Vulkan
//!   reports them (`lavapipe` is a CPU and the receipt says so);
//! * the **workload** — warm-ups, timed runs, and every fixture's every raw
//!   timing, not just a summary, so a reader can compute their own statistic;
//! * the **outcome** — what was over budget, what overflowed, the verdict.
//!
//! The format is versioned (`schema`) and dependency-free JSON, so a dashboard
//! can read a year of these without this crate.

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The receipt format's version. Bump it when a field changes meaning; adding
/// a field is not a change of meaning.
pub(crate) const SCHEMA: &str = "vieww.bench-receipt/1";

/// A JSON value — the six kinds, and nothing a receipt does not need.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    /// Ordered, because a receipt read by a person should read top-down.
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub(crate) fn obj<const N: usize>(fields: [(&str, Json); N]) -> Self {
        Self::Obj(fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
    }

    pub(crate) fn str(s: impl Into<String>) -> Self {
        Self::Str(s.into())
    }

    pub(crate) fn opt_str(s: Option<String>) -> Self {
        s.map_or(Self::Null, Self::Str)
    }

    /// Milliseconds, to the microsecond — finer is noise on every machine
    /// this runs on, and coarser hides a 0.4 ms regression.
    pub(crate) fn ms(d: Duration) -> Self {
        Self::Num((d.as_secs_f64() * 1_000_000.0).round() / 1000.0)
    }

    pub(crate) fn ms_list(runs: &[Duration]) -> Self {
        Self::Arr(runs.iter().copied().map(Self::ms).collect())
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "counts in a receipt are far below 2^52"
    )]
    pub(crate) fn count(n: usize) -> Self {
        Self::Num(n as f64)
    }

    /// Pretty-printed with two-space indentation; arrays of plain numbers stay
    /// on one line so a fixture's raw runs read as one row.
    pub(crate) fn render(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, 0);
        out.push('\n');
        out
    }

    fn write(&self, out: &mut String, depth: usize) {
        let pad = |out: &mut String, d: usize| {
            for _ in 0..d {
                out.push_str("  ");
            }
        };
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Self::Num(n) => {
                if n.is_finite() {
                    if n.fract() == 0.0 && n.abs() < 1e15 {
                        let _ = write!(out, "{n:.0}");
                    } else {
                        let _ = write!(out, "{n}");
                    }
                } else {
                    // JSON has no NaN or infinity; null is the honest stand-in.
                    out.push_str("null");
                }
            }
            Self::Str(s) => write_escaped(out, s),
            Self::Arr(items) => {
                if items.is_empty() {
                    out.push_str("[]");
                } else if items.iter().all(|i| matches!(i, Self::Num(_) | Self::Null)) {
                    out.push('[');
                    for (i, item) in items.iter().enumerate() {
                        if i > 0 {
                            out.push_str(", ");
                        }
                        item.write(out, depth);
                    }
                    out.push(']');
                } else {
                    out.push_str("[\n");
                    for (i, item) in items.iter().enumerate() {
                        pad(out, depth + 1);
                        item.write(out, depth + 1);
                        if i + 1 < items.len() {
                            out.push(',');
                        }
                        out.push('\n');
                    }
                    pad(out, depth);
                    out.push(']');
                }
            }
            Self::Obj(fields) => {
                if fields.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push_str("{\n");
                for (i, (key, value)) in fields.iter().enumerate() {
                    pad(out, depth + 1);
                    write_escaped(out, key);
                    out.push_str(": ");
                    value.write(out, depth + 1);
                    if i + 1 < fields.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                pad(out, depth);
                out.push('}');
            }
        }
    }
}

fn write_escaped(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// The p-th percentile of `runs` by nearest rank. `None` for no runs.
///
/// Nearest rank rather than interpolation: every number in a receipt should be
/// one that a frame actually took.
pub(crate) fn percentile(runs: &[Duration], p: f64) -> Option<Duration> {
    if runs.is_empty() {
        return None;
    }
    let mut sorted = runs.to_vec();
    sorted.sort_unstable();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "a rank into a short list of runs"
    )]
    let rank = ((p / 100.0) * sorted.len() as f64).ceil().max(1.0) as usize;
    Some(sorted[rank.min(sorted.len()) - 1])
}

/// Everything about where this run happened that a reader needs to reproduce
/// it. `gpu` is filled by the caller that opened a device.
pub(crate) fn header(mode: &str, warmup: usize, runs: usize, gpu: Json) -> Vec<(String, Json)> {
    vec![
        ("schema".to_owned(), Json::str(SCHEMA)),
        ("tool".to_owned(), Json::str("fixtures")),
        ("mode".to_owned(), Json::str(mode)),
        ("recorded_utc".to_owned(), Json::str(utc_now())),
        ("source".to_owned(), source()),
        ("build".to_owned(), build()),
        ("machine".to_owned(), machine()),
        ("gpu".to_owned(), gpu),
        (
            "workload".to_owned(),
            Json::obj([
                ("warmup_frames", Json::count(warmup)),
                ("timed_runs", Json::count(runs)),
                (
                    "timer",
                    Json::str("std::time::Instant around one whole render, after warm-up"),
                ),
            ]),
        ),
    ]
}

/// Write `fields` as a receipt to `path`, and say where.
pub(crate) fn write(path: &Path, fields: Vec<(String, Json)>) {
    let text = Json::Obj(fields).render();
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::write(path, text) {
        Ok(()) => println!("receipt  : {}", path.display()),
        Err(error) => eprintln!("receipt  : FAILED to write {} ({error})", path.display()),
    }
}

fn source() -> Json {
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_owned())
    };
    let dirty = git(&["status", "--porcelain", "--untracked-files=no"]).map(|s| !s.is_empty());
    Json::obj([
        ("commit", Json::opt_str(git(&["rev-parse", "HEAD"]))),
        (
            "branch",
            Json::opt_str(git(&["rev-parse", "--abbrev-ref", "HEAD"])),
        ),
        ("dirty", dirty.map_or(Json::Null, Json::Bool)),
        (
            "note",
            Json::str("read from git in the working directory at run time"),
        ),
    ])
}

fn build() -> Json {
    let mut compiled: Vec<Json> = Vec::new();
    #[cfg(target_feature = "sse2")]
    compiled.push(Json::str("sse2"));
    #[cfg(target_feature = "sse4.2")]
    compiled.push(Json::str("sse4.2"));
    #[cfg(target_feature = "avx2")]
    compiled.push(Json::str("avx2"));
    #[cfg(target_feature = "fma")]
    compiled.push(Json::str("fma"));
    #[cfg(target_feature = "neon")]
    compiled.push(Json::str("neon"));
    Json::obj([
        ("rustc", Json::str(env!("FIXTURES_RUSTC_VERSION"))),
        ("profile", Json::str(env!("FIXTURES_PROFILE"))),
        ("opt_level", Json::str(env!("FIXTURES_OPT_LEVEL"))),
        ("target", Json::str(env!("FIXTURES_TARGET"))),
        ("rustflags", Json::str(env!("FIXTURES_RUSTFLAGS"))),
        ("debug_assertions", Json::Bool(cfg!(debug_assertions))),
        ("compiled_target_features", Json::Arr(compiled)),
    ])
}

fn machine() -> Json {
    let read = |path: &str| {
        std::fs::read_to_string(path)
            .ok()
            .map(|s| s.trim().to_owned())
    };
    let cpu_model = read("/proc/cpuinfo")
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("model name") || l.starts_with("Hardware"))
                .and_then(|l| l.split_once(':'))
                .map(|(_, v)| v.trim().to_owned())
        })
        .or_else(|| command(&["sysctl", "-n", "machdep.cpu.brand_string"]))
        .or_else(|| std::env::var("PROCESSOR_IDENTIFIER").ok());
    let mem_total_kb = read("/proc/meminfo").and_then(|s| {
        s.lines()
            .find(|l| l.starts_with("MemTotal:"))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<f64>().ok())
    });
    let os = read("/etc/os-release")
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("PRETTY_NAME="))
                .map(|v| v.trim_matches('"').to_owned())
        })
        .or_else(|| command(&["sw_vers", "-productVersion"]).map(|v| format!("macOS {v}")))
        .unwrap_or_else(|| std::env::consts::OS.to_owned());

    Json::obj([
        ("os", Json::str(os)),
        ("kernel", Json::opt_str(read("/proc/sys/kernel/osrelease"))),
        ("arch", Json::str(std::env::consts::ARCH)),
        ("cpu_model", Json::opt_str(cpu_model)),
        (
            "logical_cpus",
            std::thread::available_parallelism().map_or(Json::Null, |n| Json::count(n.get())),
        ),
        ("mem_total_kb", mem_total_kb.map_or(Json::Null, Json::Num)),
        (
            "cpufreq_governor",
            Json::opt_str(read(
                "/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor",
            )),
        ),
        ("runtime_cpu_features", runtime_features()),
        (
            "virtualised",
            Json::opt_str(read("/sys/class/dmi/id/product_name")),
        ),
    ])
}

fn runtime_features() -> Json {
    #[cfg(target_arch = "x86_64")]
    {
        let mut found = Vec::new();
        if std::arch::is_x86_feature_detected!("sse4.2") {
            found.push(Json::str("sse4.2"));
        }
        if std::arch::is_x86_feature_detected!("avx2") {
            found.push(Json::str("avx2"));
        }
        if std::arch::is_x86_feature_detected!("fma") {
            found.push(Json::str("fma"));
        }
        if std::arch::is_x86_feature_detected!("avx512f") {
            found.push(Json::str("avx512f"));
        }
        Json::Arr(found)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        Json::Null
    }
}

fn command(args: &[&str]) -> Option<String> {
    let (program, rest) = args.split_first()?;
    Command::new(program)
        .args(rest)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

/// `YYYY-MM-DDTHH:MM:SSZ`, without a date crate.
fn utc_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format_utc(secs)
}

pub(crate) fn format_utc(secs: u64) -> String {
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Howard Hinnant's days-to-civil, for days since 1970-01-01 (non-negative).
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + u64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_escapes_and_nests() {
        let j = Json::obj([
            ("a", Json::str("q\"\\\n")),
            ("b", Json::Arr(vec![Json::Num(1.0), Json::Num(2.5)])),
            ("c", Json::Num(f64::NAN)),
            ("d", Json::obj([("e", Json::Bool(true))])),
        ]);
        assert_eq!(
            j.render(),
            "{\n  \"a\": \"q\\\"\\\\\\n\",\n  \"b\": [1, 2.5],\n  \"c\": null,\n  \"d\": {\n    \"e\": true\n  }\n}\n"
        );
    }

    #[test]
    fn percentiles_are_nearest_rank_and_always_a_real_sample() {
        let runs: Vec<Duration> = [5, 1, 4, 2, 3].map(Duration::from_millis).to_vec();
        assert_eq!(percentile(&runs, 50.0), Some(Duration::from_millis(3)));
        assert_eq!(percentile(&runs, 95.0), Some(Duration::from_millis(5)));
        assert_eq!(percentile(&runs, 0.0), Some(Duration::from_millis(1)));
        assert_eq!(percentile(&[], 50.0), None);
    }

    #[test]
    fn utc_formatting_matches_known_instants() {
        assert_eq!(format_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_utc(951_782_400), "2000-02-29T00:00:00Z");
        // The audit's own commit date.
        assert_eq!(format_utc(1_791_936_000), "2026-10-14T00:00:00Z");
    }

    #[test]
    fn milliseconds_are_kept_to_the_microsecond() {
        assert_eq!(
            Json::ms(Duration::from_nanos(21_234_567)),
            Json::Num(21.235)
        );
    }
}
