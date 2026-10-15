//! The deployment baseline: what a minimal vieww window costs to start, to
//! hold still, and to animate — measured from a real window on a real machine.
//!
//! ```console
//! cargo run --release -p vieww-platform-winit --example baseline -- [--gpu]
//!     [--settle S] [--idle S] [--animate S] [--out FILE]
//! ```
//!
//! # Why this exists
//!
//! The audit's P0 list has a row of numbers nobody had measured on a desktop:
//! cold start to first frame, resident memory, what an idle screen costs, and
//! energy per animated frame. `vieww-standard` measures start-up *in process
//! and offscreen* — a renderer and a driver to a first frame in memory — which
//! is the framework's share and not what a person waits for. This is the
//! other half: a real process, a real window, a real swapchain.
//!
//! # What it does
//!
//! 1. **Start.** The window opens with a small, realistic screen: a title,
//!    some body text, a card, and a band that will animate later. The first
//!    presented frame is timed from `main` and — when the launcher passes
//!    `VIEWW_BASELINE_LAUNCH_UNIX_NS`, the wall clock just before it started
//!    the process — from the launch, which includes the loader, dynamic
//!    linking and the binary's own page-in.
//! 2. **Settle** (default 1 s): fonts, the first layout and any second frame
//!    finish; nothing is measured.
//! 3. **Idle** (default 5 s): nothing changes. The contract is *zero raster
//!    work when nothing changes*, so this reports frames presented, pixels
//!    rasterised, process CPU time and — on Linux with RAPL readable — package
//!    energy. A healthy idle screen presents nothing at all.
//! 4. **Animate** (default 5 s): the band runs a repeating animation that the
//!    framework itself ticks every vsync — the same mechanism the demo's
//!    `Pulse` uses — so frame pacing is vieww's, not this harness's. Reports
//!    frames, fps, work median / p95 / worst from `FrameLog`, per-stage
//!    medians (`FrameStages`), CPU time and energy. Energy per animated frame
//!    is reported as **marginal**: animate energy minus idle power over the
//!    same duration, divided by frames.
//! 5. One line `__VIEWW_BASELINE__ {json}` on stdout (and the JSON in `--out`),
//!    then exit 0. `ci/hardware/linux.sh` and `ci/hardware/windows.ps1` run it
//!    several times and keep every report.
//!
//! # What the numbers are, and are not
//!
//! * **Memory** is the process's own: `VmRSS`/`VmHWM` on Linux, working set and
//!   peak working set on Windows. On Linux a desktop build links `libstd`
//!   dynamically (`.cargo/config.toml`), which RSS counts.
//! * **CPU time** is the whole process (every thread), user + system.
//! * **Energy** on Linux is RAPL's *package* counter — the whole CPU package,
//!   every process on it — so idle power is a machine baseline, and only the
//!   marginal per-frame figure is about vieww. RAPL is root-readable by default
//!   since 2020; the scripts say how to open it. On Windows and macOS energy is
//!   reported as `null`, never estimated.
//! * **Start-up** is warm-cache unless the launcher dropped the page cache
//!   first; the scripts say which.

#![cfg_attr(target_os = "android", allow(dead_code, unused_imports))]

use std::cell::RefCell;
use std::fmt::Write as _;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use vieww_animation::Tween;
use vieww_element::Animation;
use vieww_foundation::task::FrameWaker as _;
use vieww_foundation::{Color, EdgeInsets, Size};
use vieww_platform_winit::{App, FrameLog, Waker};
use vieww_widget::prelude::*;

#[cfg(target_os = "android")]
fn main() {
    eprintln!("the baseline is a desktop measurement; on a phone use ci/mobile/device-suite.sh");
}

const WINDOW: Size = Size {
    width: 800.0,
    height: 600.0,
};
const BG: Color = Color::rgb(12, 14, 19);
const INK: Color = Color::rgb(236, 240, 246);
const MUTED: Color = Color::rgb(150, 160, 178);
const CARD: Color = Color::rgb(28, 32, 42);

const STARTING: u8 = 0;
const SETTLING: u8 = 1;
const IDLE: u8 = 2;
const ANIMATING: u8 = 3;
const DONE: u8 = 4;

#[derive(Debug, Clone)]
struct Config {
    gpu: bool,
    settle: Duration,
    idle: Duration,
    animate: Duration,
    out: Option<std::path::PathBuf>,
}

impl Config {
    fn from_args() -> Result<Self, String> {
        let mut config = Self {
            gpu: false,
            settle: Duration::from_secs(1),
            idle: Duration::from_secs(5),
            animate: Duration::from_secs(5),
            out: None,
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            let mut secs = |flag: &str| -> Result<Duration, String> {
                let raw = args.next().ok_or_else(|| format!("{flag} needs seconds"))?;
                raw.parse::<f64>()
                    .ok()
                    .filter(|s| s.is_finite() && *s >= 0.0)
                    .map(Duration::from_secs_f64)
                    .ok_or_else(|| format!("{flag} takes seconds, not {raw:?}"))
            };
            match arg.as_str() {
                "--gpu" => config.gpu = true,
                "--settle" => config.settle = secs("--settle")?,
                "--idle" => config.idle = secs("--idle")?,
                "--animate" => config.animate = secs("--animate")?,
                "--out" => {
                    config.out = Some(args.next().ok_or("--out needs a file")?.into());
                }
                other => return Err(format!("unknown argument {other}")),
            }
        }
        Ok(config)
    }
}

/// What the operating system says about this process at one moment.
#[derive(Debug, Clone, Copy, Default)]
struct OsSample {
    rss_bytes: Option<u64>,
    peak_rss_bytes: Option<u64>,
    cpu: Option<Duration>,
    /// Per RAPL package: (energy µJ, wrap range µJ).
    energy: Option<[(u64, u64); 4]>,
    packages: usize,
}

/// A labelled moment: the OS sample plus the frame counters at that time.
#[derive(Debug, Clone, Copy)]
struct Mark {
    at: Instant,
    frames: u64,
    rasterised: u64,
    gpu_frames: u64,
    os: OsSample,
}

#[derive(Debug)]
struct Shared {
    phase: AtomicU8,
    frames: AtomicU64,
    rasterised: AtomicU64,
    gpu_frames: AtomicU64,
    marks: Mutex<Vec<(&'static str, Mark)>>,
}

impl Shared {
    fn mark(&self, name: &'static str) {
        let mark = Mark {
            at: Instant::now(),
            frames: self.frames.load(Ordering::Acquire),
            rasterised: self.rasterised.load(Ordering::Acquire),
            gpu_frames: self.gpu_frames.load(Ordering::Acquire),
            os: os::sample(),
        };
        self.marks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((name, mark));
    }

    fn get(&self, name: &str) -> Option<Mark> {
        self.marks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, m)| *m)
    }
}

/// The phases, on their own thread: it only sleeps, flips the phase and wakes
/// the loop once per transition. It never paces frames.
fn drive_phases(shared: &Shared, waker: &Waker, config: &Config) {
    shared.phase.store(SETTLING, Ordering::Release);
    std::thread::sleep(config.settle);
    shared.mark("idle_start");
    shared.phase.store(IDLE, Ordering::Release);
    std::thread::sleep(config.idle);
    shared.mark("idle_end");
    shared.phase.store(ANIMATING, Ordering::Release);
    // One wake: `before_frame` starts the animation, and from then on the
    // framework's own ticker asks for every frame.
    waker.wake();
    std::thread::sleep(config.animate);
    shared.mark("animate_end");
    shared.phase.store(DONE, Ordering::Release);
    waker.wake();
}

/// The band the animated phase moves — `examples/shared/screen.rs`' `Pulse`,
/// behind a repaint boundary so a frame costs the band and not the screen.
#[derive(Debug)]
struct Band {
    pulse: Animation<f32>,
}

impl Widget for Band {
    fn debug_name(&self) -> &'static str {
        "Band"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a tween bounded to 0..=255"
        )]
        let step = self.pulse.value().clamp(0.0, 255.0) as u8;
        RepaintBoundary::new()
            .child(
                ColoredBox::new(Color::rgb(step, 90, 255 - step))
                    .child(SizedBox::from_size(Size::new(f32::INFINITY, 32.0))),
            )
            .into()
    }
}

widget_node_from!(Band);

fn screen(pulse: Animation<f32>) -> WidgetNode {
    let card: WidgetNode = Container::new()
        .color(CARD)
        .padding(EdgeInsets::all(18.0))
        .child(
            Flex::column()
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .spacing(8.0)
                .children(children![
                    Text::new("A card").color(INK).size(16.0).bold(),
                    Text::new(
                        "Short body copy in a padded container: the kind of thing \
                         every screen has, so start-up pays for real text shaping.",
                    )
                    .color(MUTED)
                    .size(13.0),
                ]),
        )
        .into();
    Container::new()
        .color(BG)
        .padding(EdgeInsets::all(28.0))
        .child(
            Flex::column()
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .spacing(18.0)
                .children(children![
                    Text::new("vieww baseline").color(INK).size(26.0).bold(),
                    Text::new("start, hold still, animate — measured from this window")
                        .color(MUTED)
                        .size(13.0),
                    card,
                    Band { pulse },
                ]),
        )
        .into()
}

#[cfg(not(target_os = "android"))]
fn main() {
    let main_started = Instant::now();
    let launch_ns: Option<u128> = std::env::var("VIEWW_BASELINE_LAUNCH_UNIX_NS")
        .ok()
        .and_then(|v| v.trim().parse().ok());
    let config = match Config::from_args() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("baseline: {error}");
            std::process::exit(2);
        }
    };

    let shared = Arc::new(Shared {
        phase: AtomicU8::new(STARTING),
        frames: AtomicU64::new(0),
        rasterised: AtomicU64::new(0),
        gpu_frames: AtomicU64::new(0),
        marks: Mutex::new(Vec::new()),
    });
    let pulse: Rc<RefCell<Option<Animation<f32>>>> = Rc::new(RefCell::new(None));

    let mut app = App::new().title("vieww — baseline").size(WINDOW);
    if config.gpu {
        app = app.prefer_gpu();
    }
    let waker = app.waker();

    // Starts the animation the first frame after the phase flips to
    // ANIMATING. Never stopped: frames keep coming until the report is out.
    let started = Rc::new(std::cell::Cell::new(false));
    let before = {
        let shared = Arc::clone(&shared);
        let pulse = Rc::clone(&pulse);
        let started = Rc::clone(&started);
        move |_driver: &mut vieww_render::FrameDriver| {
            if !started.get() && shared.phase.load(Ordering::Acquire) >= ANIMATING {
                if let Some(pulse) = pulse.borrow().as_ref() {
                    pulse.repeat(true, Duration::ZERO);
                    started.set(true);
                }
            }
        }
    };

    let mut first: Option<(Duration, Option<f64>)> = None;
    let on_frame = {
        let shared = Arc::clone(&shared);
        let config = config.clone();
        move |log: &FrameLog| {
            shared.frames.store(log.frames(), Ordering::Release);
            shared.rasterised.store(log.rasterised(), Ordering::Release);
            shared.gpu_frames.store(log.gpu_frames(), Ordering::Release);
            if first.is_none() {
                let since_main = main_started.elapsed();
                let since_launch = launch_ns.and_then(|launch| {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .ok()?
                        .as_nanos();
                    #[expect(
                        clippy::cast_precision_loss,
                        reason = "a start-up time in ns fits an f64"
                    )]
                    now.checked_sub(launch).map(|ns| ns as f64 / 1e6)
                });
                first = Some((since_main, since_launch));
                shared.mark("first_frame");
                let shared = Arc::clone(&shared);
                let waker = waker.clone();
                let config = config.clone();
                std::thread::spawn(move || drive_phases(&shared, &waker, &config));
                return;
            }
            if shared.phase.load(Ordering::Acquire) == DONE {
                let (since_main, since_launch) = first.unwrap_or_default();
                let json = report(&shared, log, &config, since_main, since_launch);
                println!("__VIEWW_BASELINE__ {json}");
                if let Some(path) = &config.out {
                    if let Err(error) = std::fs::write(path, format!("{json}\n")) {
                        eprintln!("baseline: could not write {}: {error}", path.display());
                    }
                }
                std::process::exit(0);
            }
        }
    };

    let result = app.before_frame(before).on_frame(on_frame).run(|driver| {
        let animation = driver.animation(Tween::new(0.0_f32, 255.0), Duration::from_millis(900));
        *pulse.borrow_mut() = Some(animation.clone());
        driver.set_root(screen(animation));
    });
    // Reaching here means the window closed before the run finished.
    match result {
        Ok(_) => {
            eprintln!("baseline: the window closed before the measurement finished");
            std::process::exit(3);
        }
        Err(error) => {
            eprintln!("baseline: vieww failed: {error}");
            std::process::exit(1);
        }
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn num(out: &mut String, key: &str, value: Option<f64>) {
    match value.filter(|v| v.is_finite()) {
        Some(v) => {
            let _ = write!(out, "\"{key}\":{},", (v * 1000.0).round() / 1000.0);
        }
        None => {
            let _ = write!(out, "\"{key}\":null,");
        }
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "counts and byte totals in a report are far below 2^52"
)]
fn int(out: &mut String, key: &str, value: Option<u64>) {
    num(out, key, value.map(|v| v as f64));
}

/// Joules between two samples, summed over packages, handling counter wrap.
fn energy_joules(from: &OsSample, to: &OsSample) -> Option<f64> {
    let (Some(a), Some(b)) = (from.energy, to.energy) else {
        return None;
    };
    let mut total_uj: u64 = 0;
    for i in 0..from.packages.min(to.packages) {
        let ((start, range), (end, _)) = (a[i], b[i]);
        total_uj += if end >= start {
            end - start
        } else {
            range.saturating_sub(start) + end
        };
    }
    #[expect(clippy::cast_precision_loss, reason = "µJ over seconds fits an f64")]
    Some(total_uj as f64 / 1e6)
}

fn cpu_ms(from: &OsSample, to: &OsSample) -> Option<f64> {
    Some(ms(to.cpu?.checked_sub(from.cpu?)?))
}

/// The JSON report. Hand-written: one record, no serialisation dependency.
fn report(
    shared: &Shared,
    log: &FrameLog,
    config: &Config,
    since_main: Duration,
    since_launch: Option<f64>,
) -> String {
    let first = shared.get("first_frame");
    let idle_start = shared.get("idle_start");
    let idle_end = shared.get("idle_end");
    let anim_end = shared.get("animate_end");
    let report = log.report();
    let mut work = log.work_samples();
    work.sort_unstable();
    let p95 = (!work.is_empty()).then(|| {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "a rank into a short window"
        )]
        let rank = ((work.len() as f64) * 0.95).ceil().max(1.0) as usize;
        work[rank.min(work.len()) - 1]
    });

    let mut s = String::with_capacity(2048);
    s.push('{');
    s.push_str("\"schema\":\"vieww.baseline/1\",");
    let _ = write!(
        s,
        "\"mode\":\"{}\",",
        if config.gpu { "gpu" } else { "cpu" }
    );
    let _ = write!(
        s,
        "\"os\":\"{}\",\"arch\":\"{}\",",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    num(
        &mut s,
        "window_logical_width",
        Some(f64::from(WINDOW.width)),
    );
    num(
        &mut s,
        "window_logical_height",
        Some(f64::from(WINDOW.height)),
    );
    num(&mut s, "main_to_first_frame_ms", Some(ms(since_main)));
    num(&mut s, "launch_to_first_frame_ms", since_launch);

    if let Some(f) = first {
        int(&mut s, "first_frame_rss_bytes", f.os.rss_bytes);
        num(&mut s, "first_frame_cpu_ms", f.os.cpu.map(ms));
    }

    // Idle: the contract is zero.
    let idle_secs = idle_start
        .zip(idle_end)
        .map(|(a, b)| b.at.duration_since(a.at).as_secs_f64());
    let idle_energy = idle_start
        .zip(idle_end)
        .and_then(|(a, b)| energy_joules(&a.os, &b.os));
    let idle_power = idle_energy.zip(idle_secs).map(|(j, t)| j / t.max(1e-9));
    num(&mut s, "idle_seconds", idle_secs);
    int(
        &mut s,
        "idle_frames",
        idle_start.zip(idle_end).map(|(a, b)| b.frames - a.frames),
    );
    int(
        &mut s,
        "idle_rasterised_px",
        idle_start
            .zip(idle_end)
            .map(|(a, b)| b.rasterised - a.rasterised),
    );
    let idle_cpu = idle_start
        .zip(idle_end)
        .and_then(|(a, b)| cpu_ms(&a.os, &b.os));
    num(&mut s, "idle_cpu_ms", idle_cpu);
    num(
        &mut s,
        "idle_cpu_percent",
        idle_cpu.zip(idle_secs).map(|(c, t)| c / 10.0 / t.max(1e-9)),
    );
    num(&mut s, "idle_package_energy_j", idle_energy);
    num(&mut s, "idle_package_power_w", idle_power);
    int(
        &mut s,
        "idle_end_rss_bytes",
        idle_end.and_then(|m| m.os.rss_bytes),
    );

    // Animate.
    let anim_secs = idle_end
        .zip(anim_end)
        .map(|(a, b)| b.at.duration_since(a.at).as_secs_f64());
    let anim_frames = idle_end.zip(anim_end).map(|(a, b)| b.frames - a.frames);
    let anim_energy = idle_end
        .zip(anim_end)
        .and_then(|(a, b)| energy_joules(&a.os, &b.os));
    let anim_cpu = idle_end
        .zip(anim_end)
        .and_then(|(a, b)| cpu_ms(&a.os, &b.os));
    num(&mut s, "animate_seconds", anim_secs);
    int(&mut s, "animate_frames", anim_frames);
    int(
        &mut s,
        "animate_gpu_frames",
        idle_end
            .zip(anim_end)
            .map(|(a, b)| b.gpu_frames - a.gpu_frames),
    );
    #[expect(clippy::cast_precision_loss, reason = "a frame count fits an f64")]
    num(
        &mut s,
        "animate_fps",
        anim_frames
            .zip(anim_secs)
            .map(|(f, t)| f as f64 / t.max(1e-9)),
    );
    num(&mut s, "animate_cpu_ms", anim_cpu);
    num(
        &mut s,
        "animate_cpu_percent",
        anim_cpu.zip(anim_secs).map(|(c, t)| c / 10.0 / t.max(1e-9)),
    );
    num(&mut s, "animate_package_energy_j", anim_energy);
    // Marginal: what animating cost over holding still for as long.
    #[expect(clippy::cast_precision_loss, reason = "a frame count fits an f64")]
    num(
        &mut s,
        "marginal_energy_per_frame_mj",
        anim_energy
            .zip(idle_power)
            .zip(anim_secs)
            .zip(anim_frames.filter(|f| *f > 0))
            .map(|(((e, p), t), f)| (e - p * t) * 1000.0 / f as f64),
    );
    int(
        &mut s,
        "animate_end_rss_bytes",
        anim_end.and_then(|m| m.os.rss_bytes),
    );
    int(
        &mut s,
        "peak_rss_bytes",
        anim_end.and_then(|m| m.os.peak_rss_bytes),
    );

    // From the frame log's recent window (the last 240 frames).
    num(&mut s, "work_median_ms", Some(ms(report.median_work)));
    num(&mut s, "work_p95_ms", p95.map(ms));
    num(&mut s, "work_worst_ms", Some(ms(report.worst_work)));
    num(&mut s, "budget_ms", Some(ms(report.budget)));
    int(&mut s, "over_budget_frames", Some(report.over_budget));
    int(&mut s, "work_window_frames", Some(work.len() as u64));
    let st = report.median_stages;
    s.push_str("\"stage_medians_ms\":{");
    let stages = [
        ("scale", st.scale),
        ("plan", st.plan),
        ("cpu_raster", st.cpu_raster),
        ("gpu_upload", st.gpu.upload),
        ("gpu_record", st.gpu.record),
        ("gpu_submit", st.gpu.submit),
        ("gpu_wait", st.gpu.wait),
        ("gpu_readback", st.gpu.readback),
        ("acquire", st.acquire),
        ("present", st.present),
    ];
    for (i, (name, d)) in stages.iter().enumerate() {
        let sep = if i + 1 < stages.len() { "," } else { "" };
        let _ = write!(s, "\"{name}\":{:.3}{sep}", ms(*d));
    }
    s.push_str("},");
    let _ = write!(
        s,
        "\"energy_source\":{}",
        if first.is_some_and(|f| f.os.energy.is_some()) {
            "\"RAPL package counters (whole CPU package, all processes)\""
        } else {
            "null"
        }
    );
    s.push('}');
    s
}

/// Reading this process's memory, CPU time and the machine's energy counter,
/// with no dependency beyond `std` (and two `kernel32` calls on Windows).
mod os {
    use super::OsSample;

    pub(super) fn sample() -> OsSample {
        let mut sample = OsSample::default();
        imp::memory(&mut sample);
        sample.cpu = imp::cpu();
        energy(&mut sample);
        sample
    }

    /// RAPL's top-level package counters (`intel-rapl:N`, also used for AMD
    /// on Linux 5.11+). Unreadable without permission; then `None`.
    fn energy(sample: &mut OsSample) {
        if !cfg!(target_os = "linux") {
            return;
        }
        let mut values = [(0u64, 0u64); 4];
        let mut n = 0;
        for package in 0..4 {
            let base = format!("/sys/class/powercap/intel-rapl:{package}");
            let read = |file: &str| {
                std::fs::read_to_string(format!("{base}/{file}"))
                    .ok()
                    .and_then(|s| s.trim().parse::<u64>().ok())
            };
            match (read("energy_uj"), read("max_energy_range_uj")) {
                (Some(energy), Some(range)) => {
                    values[n] = (energy, range);
                    n += 1;
                }
                _ => break,
            }
        }
        if n > 0 {
            sample.energy = Some(values);
            sample.packages = n;
        }
    }

    #[cfg(target_os = "linux")]
    mod imp {
        use std::time::Duration;

        use super::OsSample;

        pub(super) fn memory(sample: &mut OsSample) {
            let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
                return;
            };
            let kb = |key: &str| {
                status
                    .lines()
                    .find(|l| l.starts_with(key))
                    .and_then(|l| l.split_whitespace().nth(1))
                    .and_then(|v| v.parse::<u64>().ok())
                    .map(|v| v * 1024)
            };
            sample.rss_bytes = kb("VmRSS:");
            sample.peak_rss_bytes = kb("VmHWM:");
        }

        /// utime + stime from `/proc/self/stat`, in USER_HZ ticks (100/s on
        /// every Linux this targets).
        pub(super) fn cpu() -> Option<Duration> {
            let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
            let rest = &stat[stat.rfind(')')? + 1..];
            let fields: Vec<&str> = rest.split_whitespace().collect();
            let utime: u64 = fields.get(11)?.parse().ok()?;
            let stime: u64 = fields.get(12)?.parse().ok()?;
            Some(Duration::from_millis((utime + stime) * 10))
        }
    }

    #[cfg(windows)]
    mod imp {
        use std::time::Duration;

        use super::OsSample;

        #[repr(C)]
        #[derive(Default)]
        struct ProcessMemoryCounters {
            cb: u32,
            page_fault_count: u32,
            peak_working_set_size: usize,
            working_set_size: usize,
            quota_peak_paged_pool_usage: usize,
            quota_paged_pool_usage: usize,
            quota_peak_non_paged_pool_usage: usize,
            quota_non_paged_pool_usage: usize,
            pagefile_usage: usize,
            peak_pagefile_usage: usize,
        }

        #[repr(C)]
        #[derive(Default, Clone, Copy)]
        struct FileTime {
            low: u32,
            high: u32,
        }

        impl FileTime {
            fn hundred_ns(self) -> u64 {
                (u64::from(self.high) << 32) | u64::from(self.low)
            }
        }

        #[link(name = "kernel32")]
        extern "system" {
            fn GetCurrentProcess() -> isize;
            fn K32GetProcessMemoryInfo(
                process: isize,
                counters: *mut ProcessMemoryCounters,
                cb: u32,
            ) -> i32;
            fn GetProcessTimes(
                process: isize,
                creation: *mut FileTime,
                exit: *mut FileTime,
                kernel: *mut FileTime,
                user: *mut FileTime,
            ) -> i32;
        }

        pub(super) fn memory(sample: &mut OsSample) {
            let mut counters = ProcessMemoryCounters::default();
            #[expect(clippy::cast_possible_truncation, reason = "the struct is 80 bytes")]
            let cb = std::mem::size_of::<ProcessMemoryCounters>() as u32;
            counters.cb = cb;
            // SAFETY: the pseudo-handle from GetCurrentProcess is always valid
            // for the calling process, and `counters` is a correctly sized,
            // writable PROCESS_MEMORY_COUNTERS whose `cb` says so.
            let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, cb) };
            if ok != 0 {
                sample.rss_bytes = Some(counters.working_set_size as u64);
                sample.peak_rss_bytes = Some(counters.peak_working_set_size as u64);
            }
        }

        pub(super) fn cpu() -> Option<Duration> {
            let mut times = [FileTime::default(); 4];
            let [creation, exit, kernel, user] = &mut times;
            // SAFETY: as above for the handle; four valid, writable FILETIMEs.
            let ok = unsafe { GetProcessTimes(GetCurrentProcess(), creation, exit, kernel, user) };
            (ok != 0).then(|| {
                Duration::from_nanos((times[2].hundred_ns() + times[3].hundred_ns()) * 100)
            })
        }
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    mod imp {
        use std::time::Duration;

        use super::OsSample;

        pub(super) fn memory(_sample: &mut OsSample) {}

        pub(super) fn cpu() -> Option<Duration> {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(energy: &[(u64, u64)], cpu_ms: u64) -> OsSample {
        let mut values = [(0, 0); 4];
        values[..energy.len()].copy_from_slice(energy);
        OsSample {
            rss_bytes: Some(10 << 20),
            peak_rss_bytes: Some(12 << 20),
            cpu: Some(Duration::from_millis(cpu_ms)),
            energy: (!energy.is_empty()).then_some(values),
            packages: energy.len(),
        }
    }

    #[test]
    fn energy_sums_packages_and_survives_a_counter_wrap() {
        let a = sample(&[(1_000_000, 10_000_000), (5_000_000, 10_000_000)], 0);
        let b = sample(&[(3_000_000, 10_000_000), (1_000_000, 10_000_000)], 0);
        // Package 0: +2 J. Package 1 wrapped: (10 - 5) + 1 = 6 J.
        assert_eq!(energy_joules(&a, &b), Some(8.0));
        assert_eq!(energy_joules(&sample(&[], 0), &b), None);
    }

    #[test]
    fn this_process_can_read_its_own_memory_and_cpu_time() {
        let before = os::sample();
        if cfg!(any(target_os = "linux", windows)) {
            assert!(before.rss_bytes.is_some_and(|r| r > 0));
            assert!(before.peak_rss_bytes >= before.rss_bytes);
            // Burn a little CPU so the counter has something to move by.
            let mut x = 0u64;
            let start = Instant::now();
            while start.elapsed() < Duration::from_millis(60) {
                x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            }
            std::hint::black_box(x);
            let after = os::sample();
            assert!(after.cpu >= before.cpu);
        }
    }

    #[test]
    fn the_report_is_one_line_of_json_with_every_phase() {
        let shared = Shared {
            phase: AtomicU8::new(DONE),
            frames: AtomicU64::new(0),
            rasterised: AtomicU64::new(0),
            gpu_frames: AtomicU64::new(0),
            marks: Mutex::new(Vec::new()),
        };
        let t0 = Instant::now();
        let mark = |at_ms: u64, frames: u64, energy: u64, cpu: u64| Mark {
            at: t0 + Duration::from_millis(at_ms),
            frames,
            rasterised: frames * 1000,
            gpu_frames: 0,
            os: sample(&[(energy, u64::MAX)], cpu),
        };
        {
            let mut marks = shared.marks.lock().unwrap();
            marks.push(("first_frame", mark(0, 1, 0, 50)));
            marks.push(("idle_start", mark(1_000, 2, 1_000_000, 60)));
            // Idle: no frames, 5 J over 5 s = 1 W.
            marks.push(("idle_end", mark(6_000, 2, 6_000_000, 70)));
            // Animate: 300 frames, 11 J over 5 s; marginal (11 - 5) / 300 J.
            marks.push(("animate_end", mark(11_000, 302, 17_000_000, 570)));
        }
        let log = FrameLog::new(Duration::from_millis(16), 800 * 600);
        let config = Config {
            gpu: false,
            settle: Duration::from_secs(1),
            idle: Duration::from_secs(5),
            animate: Duration::from_secs(5),
            out: None,
        };
        let json = report(
            &shared,
            &log,
            &config,
            Duration::from_millis(80),
            Some(95.5),
        );
        println!("{json}");
        assert!(json.starts_with('{') && json.ends_with('}') && !json.contains('\n'));
        assert!(!json.contains(",}") && !json.contains("NaN") && !json.contains("inf"));
        for needle in [
            "\"idle_frames\":0,",
            "\"idle_rasterised_px\":0,",
            "\"idle_package_power_w\":1,",
            "\"animate_frames\":300,",
            "\"animate_fps\":60,",
            "\"marginal_energy_per_frame_mj\":20,",
            "\"launch_to_first_frame_ms\":95.5,",
            "\"animate_cpu_percent\":10,",
        ] {
            assert!(json.contains(needle), "{needle} missing from {json}");
        }
    }
}
