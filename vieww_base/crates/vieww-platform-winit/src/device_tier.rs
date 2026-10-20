//! What kind of device this is, and what that implies for quality.
//!
//! # Why this exists
//!
//! Device Farm (2026-10-17) put the same build on phones whose right answers
//! are opposite: the LG Stylo 5 draws its best frame on the CPU rasterizer at
//! 36.2 ms against a 16.7 ms budget, the Galaxy S26 Ultra on the GPU at 2.2 ms
//! against 8.3 (`rasterizer_choice`'s table). [`App::auto_rasterizer`](crate::
//! App::auto_rasterizer) measures the rasterizer per device; every other
//! quality-shaped decision — how many pixels a frame fills, which effects are
//! worth their fill rate — has had no device-shaped input yet. This module is
//! that input: a cheap, cached probe ([`device_profile`]) and a three-tier
//! assignment ([`DeviceTier`]) a quality profile resolves against
//! (`crate::quality`).
//!
//! # What is probed, and what is deliberately not
//!
//! Total RAM (`/proc/meminfo`), how many cores the process may use
//! (`std::thread::available_parallelism`), and on Android the SoC name and
//! API level, read from the system properties in-process rather than by
//! spawning `getprop`. The GPU is *not* probed: asking Vulkan for an adapter
//! name would load the driver, and the CPU-present path exists precisely so
//! frames can be drawn without ever paying for one (77–85 MB PSS against
//! 101–171 MB with it — Device Farm, 2026-10-17). The GPU's own name is
//! recorded from whichever path actually ran.
//!
//! `/proc/meminfo` has been readable by an app's SELinux domain on every farm
//! phone so far, but `/proc/uptime` was not (see `startup`'s `process_age`),
//! so the probe keeps whatever answered and the tier policy treats an
//! unanswered question conservatively rather than guessing a number.
//!
//! # The tier policy, and why these numbers
//!
//! | phone | RAM | tier | measured |
//! |---|---|---|---|
//! | LG Stylo 5 | 2 GB | Low | CPU 36.2 ms, GPU 40.1 ms — both miss the budget |
//! | Galaxy A36 | 6 GB | Medium | CPU 13.9 ms, GPU 6.8 ms — only the GPU passes |
//! | Galaxy A73 5G | 8 GB | Medium | CPU 22.8 ms, GPU 6.9 ms at a 120 Hz budget — only the GPU passes |
//! | Galaxy S20 | 8 GB | Medium | CPU 9–11 ms |
//! | Galaxy S26 Ultra | 12 GB | High | CPU 5.6 ms, GPU 2.2 ms |
//! | iPhone 13 / 15 Plus | 4–6 GB | Medium | CPU 5.8–7.5 ms passes |
//!
//! RAM is the axis that separates them (2 / 6–8 / 12 GB); core count is the
//! tie-breaker for small devices whose RAM is unreadable. `VIEWW_DEVICE_TIER`
//! (`low`, `medium`, `high`) overrides the assignment, for A/B runs that want
//! one phone to take another's profile.

use std::sync::OnceLock;

/// The coarseness at which quality decisions are made. Deliberately three
/// levels: finer bands would want measurements the farm has not taken yet,
/// and a tier nobody can explain the boundaries of is worse than a coarse one
/// everybody can.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DeviceTier {
    /// A phone where the CPU raster can miss the budget outright: the LG
    /// Stylo 5 (2 GB, Adreno 506). Quality is the cheapest there is, and the
    /// question is whether the GPU path can be made to pass at all.
    Low,
    /// The middle of the farm: the Galaxy A36 (6 GB), A73 (8 GB), S20 (8 GB),
    /// iPhone 13 (4 GB).
    /// Measurements decide the rasterizer; effects may run if they pay.
    Medium,
    /// Where every path passes with room: the S26 Ultra (12 GB) — more than
    /// 8 GB of RAM.
    /// Full resolution, effects on.
    High,
}

impl DeviceTier {
    /// The tier's name, as the Device Farm summary prints it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

/// What the device said, as far as it said anything. The `Option`s are
/// honest: a question the OS would not answer stays unanswered, and the
/// caller decides what that is worth rather than this module guessing.
#[derive(Debug, Clone)]
pub struct DeviceFacts {
    /// The tier the policy resolved to (or `VIEWW_DEVICE_TIER` chose).
    pub tier: DeviceTier,
    /// `/proc/meminfo`'s `MemTotal`, in kB.
    pub memory_total_kb: Option<u64>,
    /// Cores the process may use, from `available_parallelism`.
    pub cores: Option<usize>,
    /// The SoC as the device names it: `ro.soc.model` (falling back to
    /// `ro.board.platform`) on Android, `/proc/cpuinfo`'s model on Linux.
    pub soc: Option<String>,
    /// `ro.build.version.sdk`, on Android only.
    pub api_level: Option<u32>,
}

impl DeviceFacts {
    /// The one-line form the on-device suite observes as `device`.
    #[must_use]
    pub fn status_line(&self) -> String {
        let mut line = format!("tier {}", self.tier.name());
        if let Some(kb) = self.memory_total_kb {
            #[expect(
                clippy::cast_precision_loss,
                reason = "GB to one decimal is the unit the summary prints"
            )]
            let gb = kb as f64 / 1024.0 / 1024.0;
            line += &format!(", {gb:.1} GB");
        } else {
            line += ", RAM unreadable";
        }
        if let Some(cores) = self.cores {
            line += &format!(", {cores} cores");
        }
        if let Some(soc) = &self.soc {
            line += &format!(", {soc}");
        }
        if let Some(api) = self.api_level {
            line += &format!(" (API {api})");
        }
        line
    }
}

/// RAM under which a device is [`DeviceTier::Low`]: the farm's Low phone has
/// 2 GB and the next one up has 4 GB, and 3 GB sits between them with room
/// for the 2.5 GB class in between.
const LOW_MAX_KB: u64 = 3 * 1024 * 1024;
/// RAM under which a device is [`DeviceTier::Medium`]: 8 GiB, so an 8 GB
/// phone — whose kernel reports 7.1–7.7 GiB of it — is Medium. It was 7 GiB
/// until Device Farm run 39382602099 (2026-10-20), which put the Galaxy A73
/// (8 GB, MemTotal 7,477,188 kB, a mid-range SM7325) in High: High cache
/// budgets and no resolution-scale eligibility, on the phone the audit's
/// "mid-tier phone" targets mean. The same move takes the S20 (8 GB) from
/// the bottom of High to the top of Medium; RAM cannot tell the two apart.
/// High now starts above 8 GB (the S26 Ultra's 12 GB reads 10.9 GiB).
const MEDIUM_MAX_KB: u64 = 8 * 1024 * 1024;

/// Which tier a device with these facts gets. Nothing here is a benchmark —
/// it is a policy about which measurements a device's quality defaults should
/// assume before the device has said anything else. `App::auto_rasterizer`
/// remains the authority on the rasterizer; this feeds everything else.
#[must_use]
pub fn resolve_tier(memory_total_kb: Option<u64>, cores: Option<usize>) -> DeviceTier {
    if let Some(kb) = memory_total_kb {
        if kb < LOW_MAX_KB || cores.is_some_and(|count| count <= 4) {
            return DeviceTier::Low;
        }
        if kb < MEDIUM_MAX_KB {
            return DeviceTier::Medium;
        }
        DeviceTier::High
    } else {
        // RAM unreadable. Cores alone cannot tell an LG Stylo 5 from an S20
        // (both 8), so the only honest moves are the small-device catch and
        // the conservative middle.
        if cores.is_some_and(|count| count <= 4) {
            DeviceTier::Low
        } else {
            DeviceTier::Medium
        }
    }
}

/// The device's profile, probed once and cached. The probe costs two file
/// reads and a couple of `sysconf`-class calls; on Android the properties are
/// read in-process (`__system_property_get`), never a `getprop` process.
#[must_use]
pub fn device_facts() -> &'static DeviceFacts {
    static PROFILE: OnceLock<DeviceFacts> = OnceLock::new();
    PROFILE.get_or_init(|| {
        let memory_total_kb = memory_total_kb();
        let cores = std::thread::available_parallelism().ok().map(usize::from);
        let soc = soc_name();
        let api_level = api_level();
        let tier = tier_override().unwrap_or_else(|| resolve_tier(memory_total_kb, cores));
        DeviceFacts {
            tier,
            memory_total_kb,
            cores,
            soc,
            api_level,
        }
    })
}

/// The device line the suite observes, `None` before the first probe (which
/// `device_facts` runs on demand, so in practice this is always `Some`).
#[must_use]
pub fn device_facts_status() -> Option<String> {
    Some(device_facts().status_line())
}

fn memory_total_kb() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    let line = text.lines().find(|line| line.starts_with("MemTotal:"))?;
    line["MemTotal:".len()..]
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

#[cfg(target_os = "android")]
fn property(name: &str) -> Option<String> {
    use std::ffi::{c_char, CStr, CString};
    // SAFETY: `name` outlives the call as a NUL-terminated C string, and
    // `__system_property_get` writes at most `PROP_VALUE_MAX` (92) bytes plus
    // a NUL into a buffer of exactly that size, on this stack frame.
    let name = CString::new(name).ok()?;
    let mut value = [0u8; 92];
    let written =
        unsafe { libc::__system_property_get(name.as_ptr(), value.as_mut_ptr().cast::<c_char>()) };
    if written <= 0 {
        return None;
    }
    // SAFETY: the call NUL-terminated what it wrote.
    let value = unsafe { CStr::from_ptr(value.as_ptr().cast::<c_char>()) };
    Some(value.to_string_lossy().trim().to_owned())
}

#[cfg(target_os = "android")]
fn soc_name() -> Option<String> {
    let model = property("ro.soc.model").filter(|value| !value.is_empty());
    let board = property("ro.board.platform").filter(|value| !value.is_empty());
    match (model, board) {
        (Some(model), Some(board)) if model != board => Some(format!("{model} ({board})")),
        (Some(model), _) => Some(model),
        (None, board) => board,
    }
}

#[cfg(target_os = "android")]
fn api_level() -> Option<u32> {
    property("ro.build.version.sdk")?.parse().ok()
}

#[cfg(not(target_os = "android"))]
fn soc_name() -> Option<String> {
    // Linux: x86 reports a model name; ARM SoCs mostly report nothing useful,
    // and an absent SoC is fine — the tier never depended on it.
    let text = std::fs::read_to_string("/proc/cpuinfo").ok()?;
    let line = text
        .lines()
        .find(|line| line.starts_with("model name") || line.starts_with("Hardware"))?;
    line.split(':').nth(1).map(str::trim).map(str::to_owned)
}

#[cfg(not(target_os = "android"))]
const fn api_level() -> Option<u32> {
    None
}

fn tier_override() -> Option<DeviceTier> {
    let requested = std::env::var("VIEWW_DEVICE_TIER").ok()?;
    match requested.trim().to_ascii_lowercase().as_str() {
        "low" => Some(DeviceTier::Low),
        "medium" => Some(DeviceTier::Medium),
        "high" => Some(DeviceTier::High),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_tier, DeviceTier};

    #[test]
    fn the_farm_phones_land_where_they_measured() {
        // LG Stylo 5: 2 GB, 8 cores — Low despite the core count.
        assert_eq!(
            resolve_tier(Some(1_913_472), Some(8)),
            DeviceTier::Low,
            "the LG's 36/40 ms both-miss run is what Low means"
        );
        // Galaxy A36: 6 GB, 8 cores — the middle.
        assert_eq!(resolve_tier(Some(5_883_264), Some(8)), DeviceTier::Medium);
        // Galaxy A73 5G: 8 GB, run 39382602099's MemTotal — Medium. Under the
        // old 7 GiB line it read High.
        assert_eq!(resolve_tier(Some(7_477_188), Some(8)), DeviceTier::Medium);
        // Galaxy S20: 8 GB — Medium too since that change (RAM is the only
        // signal, and the two phones report the same class of it).
        assert_eq!(resolve_tier(Some(7_700_000), Some(8)), DeviceTier::Medium);
        // Galaxy S26 Ultra: 12 GB (10.9 GiB reported) — High.
        assert_eq!(resolve_tier(Some(11_429_000), Some(8)), DeviceTier::High);
        // The line itself: just under 8 GiB is Medium, 8 GiB is High.
        assert_eq!(
            resolve_tier(Some(8 * 1024 * 1024 - 1), Some(8)),
            DeviceTier::Medium
        );
        assert_eq!(
            resolve_tier(Some(8 * 1024 * 1024), Some(8)),
            DeviceTier::High
        );
        // A 4-core phone is Low whatever its RAM says: nothing that small
        // has ever passed the GPU budget in this repository's runs.
        assert_eq!(resolve_tier(Some(8_000_000), Some(4)), DeviceTier::Low);
        // RAM unreadable: cores are the only signal left, and the honest
        // default for an 8-core unknown is the middle, not optimism.
        assert_eq!(resolve_tier(None, Some(8)), DeviceTier::Medium);
        assert_eq!(resolve_tier(None, Some(4)), DeviceTier::Low);
        assert_eq!(resolve_tier(None, None), DeviceTier::Medium);
    }
}
