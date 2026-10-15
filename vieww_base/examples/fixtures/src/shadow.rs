//! The hybrid planner in shadow mode over the gallery: `--shadow FILE`.
//!
//! Every fixture is planned by `vieww_render_planner::ShadowPlanner` as well as
//! rendered, and one JSON line per measured frame is written to `FILE` — what
//! the planner would have placed where, what it predicted, what the renderer
//! actually took. At the end the planner's calibration for this machine is
//! printed (and added to the receipt when `--receipt` is also given).
//!
//! The planner plans as if on `DeviceProfile::DESKTOP_DISCRETE` by default,
//! because the reference machine the budgets are written against is a laptop
//! that has a GPU; `VIEWW_SHADOW_PROFILE=mobile|cpu|desktop` picks another.
//! The CPU calibration is meaningful under any profile — every gallery frame
//! is a CPU frame — and the GPU calibration only exists where `--census`
//! actually drove a GPU.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use vieww_paint::Scene;
use vieww_render_planner::{DeviceProfile, Executor, ShadowPlanner};

use crate::receipt::Json;

#[derive(Debug)]
pub(crate) struct ShadowLog {
    path: PathBuf,
    profile_name: &'static str,
    planner: ShadowPlanner,
    lines: Vec<String>,
}

impl ShadowLog {
    pub(crate) fn new(path: &Path) -> Self {
        let (profile_name, profile) = match std::env::var("VIEWW_SHADOW_PROFILE").as_deref() {
            Ok("mobile") => ("MOBILE_TILE_BASED", DeviceProfile::MOBILE_TILE_BASED),
            Ok("cpu") => ("CPU_ONLY_FALLBACK", DeviceProfile::CPU_ONLY_FALLBACK),
            _ => ("DESKTOP_DISCRETE", DeviceProfile::DESKTOP_DISCRETE),
        };
        Self {
            path: path.to_owned(),
            profile_name,
            planner: ShadowPlanner::new(profile),
            lines: Vec::new(),
        }
    }

    /// Plan `scene`, then record that `executor` actually drew it in
    /// `actual_ms`.
    pub(crate) fn record(
        &mut self,
        label: &str,
        scene: &Scene,
        (width, height): (u32, u32),
        executor: Executor,
        actual_ms: f64,
        fallback: Option<String>,
    ) {
        let Some(plan) = self.planner.plan(scene, width, height) else {
            eprintln!("shadow   : {label}: render graph did not compile; no line written");
            return;
        };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a frame time in ms fits an f32"
        )]
        let actual = actual_ms as f32;
        let frame = self
            .planner
            .observe(plan, label, (width, height), executor, actual, fallback);
        self.lines.push(frame.to_json_line());
    }

    /// Write the lines, print the calibration, and return it for a receipt.
    pub(crate) fn finish(self) -> Json {
        if let Some(parent) = self.path.parent().filter(|p| !p.as_os_str().is_empty()) {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::File::create(&self.path) {
            Ok(mut file) => {
                for line in &self.lines {
                    let _ = writeln!(file, "{line}");
                }
                println!(
                    "shadow   : {} frames planned -> {}",
                    self.lines.len(),
                    self.path.display()
                );
            }
            Err(error) => eprintln!(
                "shadow   : FAILED to write {} ({error})",
                self.path.display()
            ),
        }

        let device = *self.planner.device();
        let cal = *self.planner.calibration();
        let fixed = cal.calibrated(&device);
        println!(
            "shadow   : planned as {}; planner agreed with the executor that ran on {} of {} frames",
            self.profile_name,
            cal.agreeing_frames(),
            cal.frames()
        );
        if let Some(r) = cal.cpu_error_ratio() {
            println!(
                "shadow   : cpu measured/predicted {r:.2}x -> cpu_fill_mpix_per_sec {:.1} (profile says {:.1})",
                fixed.cpu_fill_mpix_per_sec, device.cpu_fill_mpix_per_sec
            );
        }
        if let Some(r) = cal.gpu_error_ratio() {
            println!(
                "shadow   : gpu measured/predicted {r:.2}x -> gpu_fill_mpix_per_sec {:.1} (profile says {:.1}; dispatch overhead held at {:.3} ms, not measured)",
                fixed.gpu_fill_mpix_per_sec,
                device.gpu_fill_mpix_per_sec,
                device.gpu_dispatch_overhead_ms
            );
        }

        let opt = |v: Option<f64>| v.map_or(Json::Null, Json::Num);
        #[expect(
            clippy::cast_precision_loss,
            reason = "frame counts are far below 2^52"
        )]
        let count = |n: u64| Json::Num(n as f64);
        Json::obj([
            ("log", Json::str(self.path.display().to_string())),
            ("planned_as", Json::str(self.profile_name)),
            ("frames", count(cal.frames())),
            ("agreeing_frames", count(cal.agreeing_frames())),
            ("cpu_measured_over_predicted", opt(cal.cpu_error_ratio())),
            ("gpu_measured_over_predicted", opt(cal.gpu_error_ratio())),
            (
                "profile_cpu_fill_mpix_per_sec",
                Json::Num(f64::from(device.cpu_fill_mpix_per_sec)),
            ),
            (
                "calibrated_cpu_fill_mpix_per_sec",
                Json::Num(f64::from(fixed.cpu_fill_mpix_per_sec)),
            ),
            (
                "profile_gpu_fill_mpix_per_sec",
                Json::Num(f64::from(device.gpu_fill_mpix_per_sec)),
            ),
            (
                "calibrated_gpu_fill_mpix_per_sec",
                Json::Num(f64::from(fixed.gpu_fill_mpix_per_sec)),
            ),
        ])
    }
}
