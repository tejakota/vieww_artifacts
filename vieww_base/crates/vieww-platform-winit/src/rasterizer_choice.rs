//! Choosing the CPU or the GPU rasterizer by measuring both on this device.
//!
//! # Why this exists
//!
//! Which rasterizer is faster is a fact about the phone, and Device Farm
//! (2026-10-17) showed it going both ways in one run:
//!
//! | phone | CPU path | GPU path | budget | right choice |
//! |---|---|---|---|---|
//! | LG Stylo 5 | 36.2 ms | 40.1 ms | 16.7 | CPU (both miss; GPU is slower and +57 MB) |
//! | Galaxy A36 | 13.9 ms | 6.8 ms | 8.3 | **GPU** (only GPU passes) |
//! | Galaxy S26 Ultra | 5.6 ms | 2.2 ms | 8.3 | GPU (CPU passes with no room; worst 21 ms) |
//! | iPhone 13 | 7.5 ms | 16.2 ms | 16.7 | CPU |
//! | iPhone 15 Plus | 5.8 ms | 16.3 ms | 16.7 | CPU |
//! | iPhone 17 Pro Max | 4.1 ms | 16.2 ms | 8.3 | **CPU** (only CPU passes) |
//!
//! `App::prefer_gpu` is a constant the application has to pick before it has
//! seen any of these phones, so whichever it picks is wrong on two of them.
//! [`App::auto_rasterizer`](crate::App::auto_rasterizer) measures instead.
//!
//! # How, and why across launches
//!
//! The two rasterizers do not share a way to the screen: the CPU path presents
//! straight to the compositor and never loads a GPU driver, which is most of
//! why it is the lighter one (77–85 MB against 101–171 MB PSS on the three
//! Android phones). Measuring the CPU frame *through Vulkan* to compare with
//! the GPU would be measuring a third path — and a misleading one: on the
//! Qualcomm phones the CPU raster is 1.6× faster when Vulkan is loaded than
//! when it is not (S26: 3.3 against 5.3 ms).
//!
//! So each launch measures the path it actually runs, and the result is kept
//! in a small file:
//!
//! 1. **First launch:** the CPU path. If its median frame fits in
//!    [`COMFORTABLE`] of the budget, that is the answer — the GPU is never
//!    loaded on this device, which is the lightest outcome there is.
//! 2. Otherwise **the next launch** runs the GPU path and measures it. The GPU
//!    is kept if it is clearly faster ([`CLEARLY_FASTER`]), or if it is the
//!    only one of the two inside the budget; otherwise a tie goes to the CPU,
//!    which costs less memory.
//! 3. Every launch after that uses the answer without measuring.
//!
//! The file is keyed by the app's version, an optional build id
//! (`VIEWW_BUILD_ID` at compile time), the OS and kernel release and the
//! surface size, so a new build, an OS update or another screen size measures
//! again. `VIEWW_RENDERER_RECALIBRATE=1` forces it.
//!
//! # What this is not
//!
//! It chooses one rasterizer per window for the whole frame. The per-pass
//! hybrid planner (`vieww_render_planner`) is still in shadow: its cost model
//! is uncalibrated, and the GPU path cannot draw a mixed frame yet.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

/// The CPU path is kept without trying the GPU when its median frame fits in
/// this share of the budget: 60%, so a 120 Hz phone's 8.3 ms budget keeps the
/// CPU up to 5 ms, and a 60 Hz phone's up to 10 ms.
pub const COMFORTABLE: f64 = 0.60;

/// The GPU is kept only when its median frame is below this share of the CPU
/// path's: 15% faster, so noise between two launches does not load a GPU
/// driver for nothing.
pub const CLEARLY_FASTER: f64 = 0.85;

/// Frames skipped before measuring: the first frames of a launch compile,
/// warm caches and lay out for the first time.
const WARM_UP: u32 = 20;

/// Frames measured. Forty is enough for a stable median and short enough to
/// finish inside three seconds on a 30 fps phone.
const SAMPLES: usize = 40;

/// Which rasterizer draws a window's frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rasterizer {
    Cpu,
    Gpu,
}

impl Rasterizer {
    const fn name(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
        }
    }
}

/// What this launch does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Launch {
    /// The answer is known.
    Use(Rasterizer),
    /// Measure the CPU path (first launch).
    MeasureCpu,
    /// Measure the GPU path, against the CPU median an earlier launch measured.
    MeasureGpu { cpu: Duration },
}

impl Launch {
    /// The rasterizer this launch draws with.
    pub(crate) const fn rasterizer(self) -> Rasterizer {
        match self {
            Self::Use(rasterizer) => rasterizer,
            Self::MeasureCpu => Rasterizer::Cpu,
            Self::MeasureGpu { .. } => Rasterizer::Gpu,
        }
    }
}

/// What a finished measurement decided.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Next {
    /// Settled.
    Final(Rasterizer),
    /// The CPU is not comfortable; try the GPU next launch.
    TryGpu,
}

/// After the CPU path's median: keep it, or try the GPU next time.
pub(crate) fn after_cpu(cpu: Duration, budget: Duration) -> Next {
    if cpu.as_secs_f64() <= budget.as_secs_f64() * COMFORTABLE {
        Next::Final(Rasterizer::Cpu)
    } else {
        Next::TryGpu
    }
}

/// After the GPU path's median, given the CPU's, the frame budget and the
/// share of measured frames the GPU actually drew (a frame the GPU plan
/// cannot express falls back to the CPU, and a GPU that draws few of the
/// frames is not an option).
///
/// Two ways the GPU wins. It is [`CLEARLY_FASTER`]; or **only it fits the
/// budget** — the CPU misses and the GPU does not. The second rule exists
/// because the margin alone chose wrong on the iPhone 15 Pro in Device Farm
/// run f91d07c (2026-10-19): CPU 8.77 ms, GPU 8.19 ms, budget 8.33 ms. The
/// GPU was only 7% faster, so the margin kept the CPU — the one path that
/// misses a 120 Hz frame — and the `auto` pass then failed at 11.1 ms. The
/// margin is there so launch-to-launch noise does not load a GPU driver for
/// nothing; it is not a reason to prefer a path that misses the deadline.
pub(crate) fn after_gpu(
    cpu: Duration,
    gpu: Duration,
    gpu_share: f64,
    budget: Duration,
) -> Rasterizer {
    if gpu_share < 0.9 {
        return Rasterizer::Cpu;
    }
    let only_gpu_fits = gpu <= budget && cpu > budget;
    if only_gpu_fits || gpu.as_secs_f64() < cpu.as_secs_f64() * CLEARLY_FASTER {
        Rasterizer::Gpu
    } else {
        Rasterizer::Cpu
    }
}

/// The calibration file: where, and under which key.
#[derive(Debug, Clone)]
pub(crate) struct Calibration {
    file: PathBuf,
    key: String,
}

/// What the file said, or would say.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Record {
    launch: Launch,
    cpu: Option<Duration>,
    gpu: Option<Duration>,
}

impl Calibration {
    pub(crate) fn new(file: PathBuf, width: u32, height: u32) -> Self {
        Self {
            file,
            key: key(width, height),
        }
    }

    /// What this launch should do. Never fails: an unreadable or foreign file
    /// is a first launch.
    pub(crate) fn launch(&self) -> Launch {
        if std::env::var_os("VIEWW_RENDERER_RECALIBRATE").is_some_and(|v| v == "1") {
            let _ = std::fs::remove_file(&self.file);
            return Launch::MeasureCpu;
        }
        std::fs::read_to_string(&self.file)
            .ok()
            .and_then(|text| parse(&text, &self.key))
            .map_or(Launch::MeasureCpu, |record| record.launch)
    }

    /// Record a settled answer without a measurement — the GPU could not be
    /// opened at all.
    pub(crate) fn settle(&self, rasterizer: Rasterizer) {
        let record = Record {
            launch: Launch::Use(rasterizer),
            cpu: None,
            gpu: None,
        };
        self.store(record);
        announce(&format!("{rasterizer} (the gpu could not be opened)"));
    }

    /// Write what the next launch should do. A write that fails costs a
    /// re-measurement next launch, nothing more.
    fn store(&self, record: Record) {
        if let Some(dir) = self.file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let temporary = self.file.with_extension("tmp");
        if std::fs::write(&temporary, render(&self.key, record)).is_ok() {
            let _ = std::fs::rename(&temporary, &self.file);
        }
    }
}

/// One launch's measurement, fed a frame at a time.
#[derive(Debug)]
pub(crate) struct Measuring {
    calibration: Calibration,
    launch: Launch,
    seen: u32,
    samples: Vec<Duration>,
    gpu_drawn: usize,
}

impl Measuring {
    /// `None` when `launch` measures nothing.
    pub(crate) fn start(calibration: Calibration, launch: Launch) -> Option<Self> {
        match launch {
            Launch::Use(_) => None,
            Launch::MeasureCpu | Launch::MeasureGpu { .. } => Some(Self {
                calibration,
                launch,
                seen: 0,
                samples: Vec::with_capacity(SAMPLES),
                gpu_drawn: 0,
            }),
        }
    }

    /// One presented frame's work. Returns `true` once the measurement is
    /// finished and stored, after which the caller drops this.
    pub(crate) fn observe(&mut self, work: Duration, drawn_on_gpu: bool, budget: Duration) -> bool {
        self.seen += 1;
        if self.seen <= WARM_UP {
            return false;
        }
        self.samples.push(work);
        if drawn_on_gpu {
            self.gpu_drawn += 1;
        }
        if self.samples.len() < SAMPLES {
            return false;
        }
        self.samples.sort_unstable();
        let median = self.samples[self.samples.len() / 2];
        let record = match self.launch {
            Launch::MeasureCpu => match after_cpu(median, budget) {
                Next::Final(rasterizer) => Record {
                    launch: Launch::Use(rasterizer),
                    cpu: Some(median),
                    gpu: None,
                },
                Next::TryGpu => Record {
                    launch: Launch::MeasureGpu { cpu: median },
                    cpu: Some(median),
                    gpu: None,
                },
            },
            Launch::MeasureGpu { cpu } => {
                #[expect(clippy::cast_precision_loss, reason = "forty samples")]
                let share = self.gpu_drawn as f64 / self.samples.len() as f64;
                Record {
                    launch: Launch::Use(after_gpu(cpu, median, share, budget)),
                    cpu: Some(cpu),
                    gpu: Some(median),
                }
            }
            Launch::Use(_) => return true,
        };
        self.calibration.store(record);
        announce(&describe(record, budget, "measured"));
        true
    }
}

/// The calibration file's key: anything that, when it changes, makes an old
/// measurement worth nothing.
fn key(width: u32, height: u32) -> String {
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
    format!(
        "{} {} {}/{} {} {width}x{height}",
        env!("CARGO_PKG_VERSION"),
        option_env!("VIEWW_BUILD_ID").unwrap_or("-"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        kernel.trim(),
    )
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn render(key: &str, record: Record) -> String {
    let (state, cpu) = match record.launch {
        Launch::Use(rasterizer) => (rasterizer.name().to_string(), record.cpu),
        Launch::MeasureCpu => ("measure-cpu".to_string(), record.cpu),
        Launch::MeasureGpu { cpu } => ("measure-gpu".to_string(), Some(cpu)),
    };
    let mut text = format!("vieww-rasterizer 1\nkey={key}\nstate={state}\n");
    if let Some(cpu) = cpu {
        text.push_str(&format!("cpu_ms={:.3}\n", ms(cpu)));
    }
    if let Some(gpu) = record.gpu {
        text.push_str(&format!("gpu_ms={:.3}\n", ms(gpu)));
    }
    text
}

fn parse(text: &str, key: &str) -> Option<Record> {
    let mut lines = text.lines();
    if lines.next()? != "vieww-rasterizer 1" {
        return None;
    }
    let mut fields = std::collections::HashMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once('=') {
            fields.insert(name, value);
        }
    }
    if *fields.get("key")? != key {
        return None;
    }
    let duration = |name: &str| {
        fields
            .get(name)
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v >= 0.0)
            .map(|v| Duration::from_secs_f64(v / 1000.0))
    };
    let (cpu, gpu) = (duration("cpu_ms"), duration("gpu_ms"));
    let launch = match *fields.get("state")? {
        "cpu" => Launch::Use(Rasterizer::Cpu),
        "gpu" => Launch::Use(Rasterizer::Gpu),
        "measure-gpu" => Launch::MeasureGpu { cpu: cpu? },
        _ => return None,
    };
    Some(Record { launch, cpu, gpu })
}

/// A sentence for the log and the device suite.
fn describe(record: Record, budget: Duration, how: &str) -> String {
    let numbers = match (record.cpu, record.gpu) {
        (Some(cpu), Some(gpu)) => format!("cpu {:.2} ms, gpu {:.2} ms", ms(cpu), ms(gpu)),
        (Some(cpu), None) => format!("cpu {:.2} ms", ms(cpu)),
        _ => "no measurement".to_string(),
    };
    let what = match record.launch {
        Launch::Use(rasterizer) => rasterizer.name().to_string(),
        Launch::MeasureCpu => "cpu, measuring".to_string(),
        Launch::MeasureGpu { .. } => "cpu for now; the gpu is measured next launch".to_string(),
    };
    format!("{what} ({how}: {numbers}; budget {:.2} ms)", ms(budget))
}

static CHOICE: Mutex<Option<String>> = Mutex::new(None);

fn announce(text: &str) {
    log::info!("vieww: rasterizer {text}");
    if let Ok(mut choice) = CHOICE.lock() {
        *choice = Some(text.to_string());
    }
}

/// Note what a launch is doing, before any measurement finishes.
pub(crate) fn announce_launch(calibration: &Calibration, launch: Launch) {
    let text = match launch {
        Launch::Use(rasterizer) => {
            let record = std::fs::read_to_string(&calibration.file)
                .ok()
                .and_then(|text| parse(&text, &calibration.key));
            match record {
                Some(record) => describe_cached(rasterizer, record),
                None => rasterizer.name().to_string(),
            }
        }
        Launch::MeasureCpu => "cpu, measuring (first launch with this build here)".to_string(),
        Launch::MeasureGpu { cpu } => format!(
            "gpu, measuring (the cpu path measured {:.2} ms last launch)",
            ms(cpu)
        ),
    };
    announce(&text);
}

fn describe_cached(rasterizer: Rasterizer, record: Record) -> String {
    let numbers = match (record.cpu, record.gpu) {
        (Some(cpu), Some(gpu)) => format!("cpu {:.2} ms, gpu {:.2} ms", ms(cpu), ms(gpu)),
        (Some(cpu), None) => format!("cpu {:.2} ms", ms(cpu)),
        _ => "no numbers".to_string(),
    };
    format!("{} (calibrated earlier: {numbers})", rasterizer.name())
}

/// What `App::auto_rasterizer` chose for this process, and why — `None` for an
/// application that did not ask for it.
#[must_use]
pub fn rasterizer_choice() -> Option<String> {
    CHOICE.lock().ok()?.clone()
}

/// Where the calibration file lives, given the application's data directory.
pub(crate) fn file_in(dir: &Path) -> PathBuf {
    let stem = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "app".to_string());
    dir.join(format!("rasterizer-{stem}.txt"))
}

impl fmt::Display for Rasterizer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(value: f64) -> Duration {
        Duration::from_secs_f64(value / 1000.0)
    }

    const HZ60: f64 = 16.67;
    const HZ120: f64 = 8.33;

    /// The six phones of the 2026-10-17 Device Farm run, as the table in the
    /// module docs: (cpu median, gpu median, budget, expected).
    #[test]
    fn the_device_farm_phones_get_the_rasterizer_that_passes() {
        let phones = [
            ("LG Stylo 5", 36.2, 40.1, HZ60, Rasterizer::Cpu),
            ("Galaxy A36", 13.9, 6.8, HZ120, Rasterizer::Gpu),
            ("Galaxy S26 Ultra", 5.6, 2.2, HZ120, Rasterizer::Gpu),
            ("iPhone 13", 7.5, 16.2, HZ60, Rasterizer::Cpu),
            ("iPhone 15 Plus", 5.8, 16.3, HZ60, Rasterizer::Cpu),
            ("iPhone 17 Pro Max", 4.1, 16.2, HZ120, Rasterizer::Cpu),
            // Run f91d07c, 2026-10-19: the calibration's own numbers.
            (
                "iPhone 15 Pro (f91d07c)",
                8.77,
                8.19,
                HZ120,
                Rasterizer::Gpu,
            ),
            ("iPhone 17 (f91d07c)", 5.41, 8.09, HZ120, Rasterizer::Cpu),
            ("Pixel 5a (f91d07c)", 36.12, 8.44, HZ60, Rasterizer::Gpu),
            ("LG Stylo 5 (f91d07c)", 37.20, 40.91, HZ60, Rasterizer::Cpu),
            (
                "Galaxy S26 Ultra (f91d07c)",
                5.98,
                1.70,
                HZ120,
                Rasterizer::Gpu,
            ),
        ];
        for (phone, cpu, gpu, budget, expected) in phones {
            let chosen = match after_cpu(ms(cpu), ms(budget)) {
                Next::Final(rasterizer) => rasterizer,
                Next::TryGpu => after_gpu(ms(cpu), ms(gpu), 1.0, ms(budget)),
            };
            assert_eq!(chosen, expected, "{phone}");
        }
    }

    #[test]
    fn a_gpu_that_cannot_draw_most_frames_is_not_chosen() {
        assert_eq!(after_gpu(ms(20.0), ms(5.0), 0.5, ms(4.0)), Rasterizer::Cpu);
        // Not even when it is the only one inside the budget.
        assert_eq!(after_gpu(ms(20.0), ms(5.0), 0.5, ms(HZ60)), Rasterizer::Cpu);
    }

    #[test]
    fn a_near_tie_goes_to_the_lighter_cpu_path() {
        // Both miss the budget: the margin decides, and a tie stays on the CPU.
        assert_eq!(
            after_gpu(ms(20.0), ms(18.0), 1.0, ms(HZ60)),
            Rasterizer::Cpu
        );
        // Both fit: same.
        assert_eq!(after_gpu(ms(10.0), ms(9.0), 1.0, ms(HZ60)), Rasterizer::Cpu);
    }

    #[test]
    fn a_near_tie_goes_to_the_gpu_when_only_the_gpu_fits_the_budget() {
        assert_eq!(
            after_gpu(ms(8.77), ms(8.19), 1.0, ms(HZ120)),
            Rasterizer::Gpu
        );
        // And never the other way round: a GPU that misses is not preferred
        // to a CPU that fits.
        assert_eq!(after_gpu(ms(8.0), ms(8.5), 1.0, ms(HZ120)), Rasterizer::Cpu);
    }

    #[test]
    fn the_file_round_trips_and_a_foreign_key_is_a_first_launch() {
        let record = Record {
            launch: Launch::MeasureGpu { cpu: ms(13.9) },
            cpu: Some(ms(13.9)),
            gpu: None,
        };
        let text = render("k1", record);
        let back = parse(&text, "k1").expect("parses");
        assert!(matches!(back.launch, Launch::MeasureGpu { cpu } if (cpu.as_secs_f64() - 0.0139).abs() < 1e-6));
        assert_eq!(parse(&text, "k2"), None);
        assert_eq!(parse("garbage", "k1"), None);
    }

    #[test]
    fn two_launches_settle_a_phone_that_needs_the_gpu() {
        let dir = std::env::temp_dir().join(format!("vieww-raster-{}", std::process::id()));
        let file = dir.join("rasterizer-test.txt");
        let _ = std::fs::remove_file(&file);
        let budget = ms(HZ120);

        // Launch 1: the CPU path, too slow to be comfortable.
        let calibration = Calibration::new(file.clone(), 1080, 2340);
        let launch = calibration.launch();
        assert_eq!(launch, Launch::MeasureCpu);
        let mut measuring = Measuring::start(calibration, launch).expect("measures");
        let mut done = false;
        for _ in 0..(WARM_UP as usize + SAMPLES) {
            done = measuring.observe(ms(13.9), false, budget);
        }
        assert!(done);

        // Launch 2: the GPU path, clearly faster.
        let calibration = Calibration::new(file.clone(), 1080, 2340);
        let launch = calibration.launch();
        assert!(matches!(launch, Launch::MeasureGpu { .. }), "{launch:?}");
        assert_eq!(launch.rasterizer(), Rasterizer::Gpu);
        let mut measuring = Measuring::start(calibration, launch).expect("measures");
        for _ in 0..(WARM_UP as usize + SAMPLES) {
            done = measuring.observe(ms(6.8), true, budget);
        }
        assert!(done);

        // Launch 3 and after: settled, nothing measured.
        let calibration = Calibration::new(file.clone(), 1080, 2340);
        assert_eq!(calibration.launch(), Launch::Use(Rasterizer::Gpu));
        // Another screen size measures again.
        assert_eq!(Calibration::new(file.clone(), 720, 1600).launch(), Launch::MeasureCpu);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
