//! Shadow mode: the planner runs beside the real renderer, every frame, and
//! changes nothing.
//!
//! # Why this exists
//!
//! [`crate::plan_frame`] has been able to place every pass on the CPU, the GPU
//! or both since this crate was written, and no frame has ever been drawn
//! the way it said. The windowed path (`vieww-platform-winit`'s
//! `present_planned`) makes a different, simpler decision — the whole frame on
//! the GPU when `vieww_gpu::ScenePlan::is_complete`, otherwise the whole frame
//! on the CPU — and the planner's opinion of that frame is never even
//! computed. Which means nobody knows whether the planner's opinion is any
//! good: its throughputs ([`DeviceProfile`]) are round numbers, its 250,000-
//! pixel hybrid threshold is a guess, and making it authoritative today would
//! be putting a guess in charge of every frame.
//!
//! The audit's answer, and this module: **compute the plan, run the frame the
//! old way, and write down both.** [`ShadowPlanner::plan`] builds the render
//! graph for the frame and plans it; the caller renders exactly as before and
//! reports what actually ran and how long it took through
//! [`ShadowPlanner::observe`]; the result is a [`ShadowFrame`] — one JSON line
//! — saying what the planner would have done, what it predicted, and what the
//! real path measured.
//!
//! Two things fall out of a run of those lines:
//!
//! * **Calibration.** [`Calibration`] compares the heuristic's prediction for
//!   the executor that *actually ran* against the measured time, and gives the
//!   throughput the device profile would need for the heuristic to have been
//!   right ([`Calibration::calibrated`]). That is the "verify the planner's
//!   pixel-count, dispatch-overhead, CPU/GPU timing assumptions on actual
//!   devices" step, as a number per device rather than as a task.
//! * **Disagreement.** Every frame where the planner would not have done what
//!   the live path did is counted, with the planner's predicted saving. That
//!   is the evidence an authoritative rollout needs before it is switched on.
//!
//! # What shadow mode deliberately does not do
//!
//! * It does **not** feed measurements back into the cost model it predicts
//!   with. A self-correcting model would converge on whatever the live path
//!   does and report perfect agreement — the opposite of what a shadow run is
//!   for. Predictions always come from the uncorrected [`HeuristicCostModel`];
//!   correction is reported separately, by [`Calibration`].
//! * It does **not** claim per-pass actuals. The live CPU path rasterises the
//!   whole scene in one call and the GPU path submits one command buffer, so
//!   the only honest measurement is per frame. Per-pass predictions are
//!   recorded so that, when a backend can time passes, the comparison is
//!   ready; until then the frame is the unit.
//! * It is **not free.** Planning builds a scene graph and a render graph per
//!   frame and allocates to do it, so it must stay opt-in: the zero
//!   steady-state allocation gate applies to the frame path with shadow mode
//!   off.

use std::fmt::Write as _;

use vieww_paint::Scene;
use vieww_render_graph::PassKind;
use vieww_scene::SceneGraph;

use crate::capabilities::DeviceProfile;
use crate::cost::{CostModel, HeuristicCostModel};
use crate::frame_budget::FrameBudget;
use crate::planner::{plan_frame, work_pixels, FramePlan};
use crate::policy::Placement;

/// Which executor actually rendered a frame on the live path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Executor {
    Cpu,
    Gpu,
}

impl Executor {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
        }
    }
}

/// What the planner, taken as a whole, recommends for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recommendation {
    /// Every pass on the CPU.
    Cpu,
    /// Every pass on the GPU.
    Gpu,
    /// Some passes on each side, or at least one pass split — something the
    /// live path cannot do today, which is the point of the hybrid engine.
    Mixed,
}

impl Recommendation {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
            Self::Mixed => "mixed",
        }
    }

    /// Whether running the whole frame on `executor` is what this
    /// recommendation asked for. `Mixed` never agrees: no whole-frame
    /// executor does what it asked.
    #[must_use]
    pub const fn agrees_with(self, executor: Executor) -> bool {
        matches!(
            (self, executor),
            (Self::Cpu, Executor::Cpu) | (Self::Gpu, Executor::Gpu)
        )
    }
}

/// One pass of a planned frame, as the planner saw it.
#[derive(Debug, Clone, PartialEq)]
pub struct PassSummary {
    pub name: &'static str,
    pub kind: PassKind,
    pub pixels: f32,
    pub placement: Placement,
    pub cpu_ms: f32,
    pub gpu_ms: Option<f32>,
}

/// The planner's opinion of one frame, before it has been rendered.
#[derive(Debug, Clone, PartialEq)]
pub struct ShadowPlan {
    pub frame_plan: FramePlan,
    /// In execution order.
    pub passes: Vec<PassSummary>,
    /// Predicted time if every pass ran on the CPU — what the heuristic says
    /// the live CPU fallback costs.
    pub all_cpu_ms: f32,
    /// Predicted time if every pass ran on the GPU — what the heuristic says
    /// the live GPU path costs. `None` on a device profile with no GPU.
    pub all_gpu_ms: Option<f32>,
    /// Pixels of work, summed over passes ([`work_pixels`]): layers are
    /// bounds-sized and composites are charged their layer, so this counts
    /// real work, not passes × screens.
    pub pixels: f32,
}

impl ShadowPlan {
    #[must_use]
    pub fn recommendation(&self) -> Recommendation {
        let mut cpu = false;
        let mut gpu = false;
        for pass in &self.passes {
            match pass.placement {
                Placement::Cpu => cpu = true,
                Placement::Gpu => gpu = true,
                Placement::Hybrid(_) => return Recommendation::Mixed,
            }
        }
        match (cpu, gpu) {
            (_, false) => Recommendation::Cpu,
            (false, true) => Recommendation::Gpu,
            (true, true) => Recommendation::Mixed,
        }
    }

    /// What the heuristic predicted for the executor that actually ran.
    #[must_use]
    pub fn predicted_for(&self, executor: Executor) -> Option<f32> {
        match executor {
            Executor::Cpu => Some(self.all_cpu_ms),
            Executor::Gpu => self.all_gpu_ms,
        }
    }

    fn count(&self, want: fn(&Placement) -> bool) -> usize {
        self.passes.iter().filter(|p| want(&p.placement)).count()
    }
}

/// One shadow-mode frame: the plan, and what actually happened.
#[derive(Debug, Clone, PartialEq)]
pub struct ShadowFrame {
    pub frame: u64,
    /// Free text from the caller — a fixture name, a window title.
    pub label: String,
    pub width: u32,
    pub height: u32,
    pub plan: ShadowPlan,
    pub actual: Executor,
    pub actual_ms: f32,
    /// Why the live path took the executor it did, when that was not a free
    /// choice — `"plan incomplete: Text"`, `"no gpu"`. `None` when the live
    /// path simply took its preferred route.
    pub fallback: Option<String>,
}

impl ShadowFrame {
    #[must_use]
    pub fn agrees(&self) -> bool {
        self.plan.recommendation().agrees_with(self.actual)
    }

    /// Measured over predicted, for the executor that ran. Above 1.0 the
    /// heuristic is optimistic about this device; below, pessimistic.
    #[must_use]
    pub fn error_ratio(&self) -> Option<f32> {
        self.plan
            .predicted_for(self.actual)
            .filter(|p| *p > 0.0)
            .map(|p| self.actual_ms / p)
    }

    /// One line of JSON, no trailing newline. Hand-written: this crate takes
    /// no serialisation dependency for one record type.
    #[must_use]
    pub fn to_json_line(&self) -> String {
        let mut s = String::with_capacity(256 + 96 * self.plan.passes.len());
        let opt = |v: Option<f32>| v.map_or_else(|| "null".to_owned(), fmt_f32);
        let _ = write!(
            s,
            "{{\"frame\":{},\"label\":{},\"width\":{},\"height\":{},\
             \"actual\":\"{}\",\"actual_ms\":{},\"fallback\":{},\
             \"recommendation\":\"{}\",\"agrees\":{},\
             \"predicted_plan_ms\":{},\"predicted_all_cpu_ms\":{},\"predicted_all_gpu_ms\":{},\
             \"predicted_actual_ms\":{},\"error_ratio\":{},\"pixels\":{},\
             \"passes_cpu\":{},\"passes_gpu\":{},\"passes_hybrid\":{},\"passes\":[",
            self.frame,
            json_str(&self.label),
            self.width,
            self.height,
            self.actual.name(),
            fmt_f32(self.actual_ms),
            self.fallback
                .as_deref()
                .map_or_else(|| "null".to_owned(), json_str),
            self.plan.recommendation().name(),
            self.agrees(),
            fmt_f32(self.plan.frame_plan.predicted_ms),
            fmt_f32(self.plan.all_cpu_ms),
            opt(self.plan.all_gpu_ms),
            opt(self.plan.predicted_for(self.actual)),
            opt(self.error_ratio()),
            fmt_f32(self.plan.pixels),
            self.plan.count(|p| matches!(p, Placement::Cpu)),
            self.plan.count(|p| matches!(p, Placement::Gpu)),
            self.plan.count(|p| matches!(p, Placement::Hybrid(_))),
        );
        for (i, pass) in self.plan.passes.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            let placement = match pass.placement {
                Placement::Cpu => "\"cpu\"".to_owned(),
                Placement::Gpu => "\"gpu\"".to_owned(),
                Placement::Hybrid(split) => {
                    format!(
                        "{{\"hybrid_cpu_fraction\":{}}}",
                        fmt_f32(split.cpu_fraction)
                    )
                }
            };
            let _ = write!(
                s,
                "{{\"name\":\"{}\",\"kind\":\"{:?}\",\"pixels\":{},\"placement\":{},\
                 \"cpu_ms\":{},\"gpu_ms\":{}}}",
                pass.name,
                pass.kind,
                fmt_f32(pass.pixels),
                placement,
                fmt_f32(pass.cpu_ms),
                opt(pass.gpu_ms),
            );
        }
        s.push_str("]}");
        s
    }
}

fn fmt_f32(v: f32) -> String {
    if v.is_finite() {
        // Four decimals: a microsecond-and-a-bit at millisecond scale, and
        // enough for a fraction.
        let s = format!("{v:.4}");
        let s = s.trim_end_matches('0').trim_end_matches('.');
        if s.is_empty() || s == "-" {
            "0".to_owned()
        } else {
            s.to_owned()
        }
    } else {
        "null".to_owned()
    }
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Running comparison of predicted and measured frame time, per executor.
///
/// Two readings live here and both are kept because they answer different
/// questions. The **ratio of sums** (`cpu_error_ratio`) is "how wrong is
/// the model in total over this run" — the number `calibrated` needs, and
/// the one a device profile correction wants. The **per-frame ratio
/// distribution** (`rmse_of_ratio`) is "how consistent is that wrongness"
/// — a model whose error is a steady 8.4× is fully correctable (multiply
/// back and the residual is noise), while one whose error swings 1×–20×
/// frame to frame cannot be trusted at any scale. The authoritative gate
/// ("ready") is stated on the second, because acting on a frame is
/// exactly where consistency, not the total, is what protects you.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Calibration {
    cpu_predicted: f64,
    cpu_actual: f64,
    cpu_frames: u64,
    cpu_ratio_sum: f64,
    cpu_ratio_sq_sum: f64,
    gpu_predicted: f64,
    gpu_actual: f64,
    gpu_overhead: f64,
    gpu_frames: u64,
    gpu_ratio_sum: f64,
    gpu_ratio_sq_sum: f64,
    agreeing_frames: u64,
}

impl Calibration {
    fn record(&mut self, frame: &ShadowFrame, device: &DeviceProfile) {
        let Some(predicted) = frame.plan.predicted_for(frame.actual) else {
            return;
        };
        if predicted > 0.0 && frame.actual_ms.is_finite() {
            let ratio = f64::from(frame.actual_ms) / f64::from(predicted);
            if ratio.is_finite() {
                match frame.actual {
                    Executor::Cpu => {
                        self.cpu_ratio_sum += ratio;
                        self.cpu_ratio_sq_sum += ratio * ratio;
                    }
                    Executor::Gpu => {
                        self.gpu_ratio_sum += ratio;
                        self.gpu_ratio_sq_sum += ratio * ratio;
                    }
                }
            }
        }
        match frame.actual {
            Executor::Cpu => {
                self.cpu_predicted += f64::from(predicted);
                self.cpu_actual += f64::from(frame.actual_ms);
                self.cpu_frames += 1;
            }
            Executor::Gpu => {
                self.gpu_predicted += f64::from(predicted);
                self.gpu_actual += f64::from(frame.actual_ms);
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "a frame has a handful of passes"
                )]
                let passes = frame.plan.passes.len() as f64;
                self.gpu_overhead += f64::from(device.gpu_dispatch_overhead_ms) * passes;
                self.gpu_frames += 1;
            }
        }
        if frame.agrees() {
            self.agreeing_frames += 1;
        }
    }

    #[must_use]
    pub const fn frames(&self) -> u64 {
        self.cpu_frames + self.gpu_frames
    }

    #[must_use]
    pub const fn cpu_frames(&self) -> u64 {
        self.cpu_frames
    }

    #[must_use]
    pub const fn gpu_frames(&self) -> u64 {
        self.gpu_frames
    }

    #[must_use]
    pub const fn agreeing_frames(&self) -> u64 {
        self.agreeing_frames
    }

    /// Total measured CPU time over total predicted, across every CPU frame.
    /// A ratio of sums, not a mean of ratios, so a tiny frame with a large
    /// relative error cannot dominate.
    #[must_use]
    pub fn cpu_error_ratio(&self) -> Option<f64> {
        (self.cpu_frames > 0 && self.cpu_predicted > 0.0)
            .then(|| self.cpu_actual / self.cpu_predicted)
    }

    #[must_use]
    pub fn gpu_error_ratio(&self) -> Option<f64> {
        (self.gpu_frames > 0 && self.gpu_predicted > 0.0)
            .then(|| self.gpu_actual / self.gpu_predicted)
    }

    /// Root-mean-square of the per-frame error ratios (measured over
    /// predicted), for one executor — the *consistency* reading this
    /// struct's doc describes. `None` until that executor has frames with
    /// a positive prediction; identical to zero when every frame measured
    /// exactly what the model said.
    #[must_use]
    pub fn rmse_of_ratio(&self, executor: Executor) -> Option<f64> {
        let (frames, sum, sq_sum) = match executor {
            Executor::Cpu => (self.cpu_frames, self.cpu_ratio_sum, self.cpu_ratio_sq_sum),
            Executor::Gpu => (self.gpu_frames, self.gpu_ratio_sum, self.gpu_ratio_sq_sum),
        };
        if frames == 0 {
            return None;
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "frame counts are far below f64's exact range"
        )]
        let n = frames as f64;
        // mean((r - 1)^2) = mean(r^2) - 2*mean(r) + 1, clamped against the
        // negative side the algebra allows and floating-point rounding can
        // produce when every ratio is exactly 1.
        let mean_sq = sq_sum / n;
        let mean = sum / n;
        Some((mean_sq - 2.0 * mean + 1.0).max(0.0).sqrt())
    }

    /// Whether this executor's calibrated reading is good enough to act
    /// on: at least [`AUTHORITATIVE_MIN_FRAMES`] frames whose error, as a
    /// root-mean-square of per-frame ratios, stays within
    /// [`AUTHORITATIVE_MAX_RMSE`]. This is the advisory's gate ("RMSE < 5%
    /// and samples > 1000"), and it is deliberately about the frames the
    /// planner *made its predictions with* — see [`ShadowPlanner`] for the
    /// machine that feeds it.
    #[must_use]
    pub fn ready(&self, executor: Executor) -> bool {
        let frames = match executor {
            Executor::Cpu => self.cpu_frames,
            Executor::Gpu => self.gpu_frames,
        };
        match self.rmse_of_ratio(executor) {
            Some(rmse) => frames >= AUTHORITATIVE_MIN_FRAMES && rmse <= AUTHORITATIVE_MAX_RMSE,
            None => false,
        }
    }

    /// `device` with its fill throughputs corrected so the heuristic would
    /// have predicted the measured totals.
    ///
    /// The CPU side is exact for the heuristic's linear model. The GPU side
    /// holds the dispatch overhead fixed and attributes the whole remaining
    /// error to fill rate, which is an approximation: separating the two
    /// needs frames of different sizes on the same device, and the overhead
    /// is reported unchanged so nobody mistakes it for a measured value.
    /// A side with no frames, or whose measured time does not exceed its
    /// assumed overhead, keeps the profile's original number.
    #[must_use]
    pub fn calibrated(&self, device: &DeviceProfile) -> DeviceProfile {
        let mut out = *device;
        if let Some(ratio) = self.cpu_error_ratio().filter(|r| *r > 0.0) {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a throughput in Mpix/s fits an f32"
            )]
            let fill = (f64::from(device.cpu_fill_mpix_per_sec) / ratio) as f32;
            out.cpu_fill_mpix_per_sec = fill;
        }
        let gpu_fill_pred = self.gpu_predicted - self.gpu_overhead;
        let gpu_fill_actual = self.gpu_actual - self.gpu_overhead;
        if self.gpu_frames > 0 && gpu_fill_pred > 0.0 && gpu_fill_actual > 0.0 {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a throughput in Mpix/s fits an f32"
            )]
            let fill =
                (f64::from(device.gpu_fill_mpix_per_sec) * gpu_fill_pred / gpu_fill_actual) as f32;
            out.gpu_fill_mpix_per_sec = fill;
        }
        out
    }
}

/// The gate an authoritative rollout has to clear, as numbers rather than
/// as an argument: the model the planner would act on must predict at
/// least this many frames, with a per-frame relative-error RMSE no worse
/// than this. The advisory's figures ("RMSE < 5% and samples > 1000"),
/// adopted verbatim — the same warmup also feeds the raw-model
/// `calibrated()` correction, so the total observation budget before a
/// frame can be acted on is twice this.
pub const AUTHORITATIVE_MIN_FRAMES: u64 = 1_000;
pub const AUTHORITATIVE_MAX_RMSE: f64 = 0.05;

/// How far the shadow planner has been allowed to go.
///
/// The machine advances in one direction only, and every step is earned
/// by data rather than by confidence:
///
/// 1. **`Shadow`** — plan, log, change nothing. The state the planner has
///    lived in since 2026-10-15, and the state it stays in unless the
///    embedding application explicitly opts in
///    (`ShadowPlanner::with_authoritative`, `VIEWW_PLANNER_AUTHORITATIVE=1`
///    on the windowed path).
/// 2. **`ShadowCalibrated`** — after [`AUTHORITATIVE_MIN_FRAMES`] observed
///    frames, the profile is replaced by `Calibration::calibrated`'s
///    correction (the Kaby Lake receipts measured the raw model at 8.4×
///    optimistic on the GPU; the correction is exactly that kind of
///    number) and a **fresh** gate `Calibration` starts filling. Still
///    changing nothing: these frames measure how good the *corrected*
///    model is, which is the question actuation actually depends on.
/// 3. **`Authoritative`** — the gate has cleared on at least one
///    executor. `authoritative_choice` may now hand the live path a
///    whole-frame executor decision, and still only for frames whose
///    recommending executor's gate reading is `ready` at that moment —
///    `Mixed` frames and unready executors fall back to the live path's
///    own `ScenePlan::is_complete` gate, which stays the per-frame
///    fallback forever.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlannerMode {
    /// Planning and logging, never acting.
    #[default]
    Shadow,
    /// Planning with the calibrated profile, logging, still never acting.
    ShadowCalibrated,
    /// The gate cleared; `authoritative_choice` may act within its own
    /// per-frame readiness checks.
    Authoritative,
}

impl PlannerMode {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Shadow => "shadow",
            Self::ShadowCalibrated => "shadow-calibrated",
            Self::Authoritative => "authoritative",
        }
    }
}

/// The planner, run in shadow.
#[derive(Debug)]
pub struct ShadowPlanner {
    device: DeviceProfile,
    model: HeuristicCostModel,
    next_frame: u64,
    calibration: Calibration,
    /// How far the mode machine has been allowed to go.
    mode: PlannerMode,
    /// Whether the embedding application opted in to the machine running
    /// at all. `false` pins `mode` at [`PlannerMode::Shadow`] forever,
    /// which is the behavior every existing caller gets.
    authoritative_enabled: bool,
    /// The profile predictions are currently made with — `device` in
    /// shadow, `calibration.calibrated(&device)` once promoted.
    planning_device: DeviceProfile,
    /// The fresh gate reading started at promotion, fed by frames whose
    /// predictions came from the calibrated profile.
    gate: Calibration,
}

impl ShadowPlanner {
    /// Plan as if running on `device`. The profile's throughputs are what the
    /// predictions are made with; [`Self::calibration`] reports how wrong
    /// they turned out to be.
    #[must_use]
    pub fn new(device: DeviceProfile) -> Self {
        Self {
            device,
            model: HeuristicCostModel,
            next_frame: 0,
            calibration: Calibration::default(),
            mode: PlannerMode::Shadow,
            authoritative_enabled: false,
            planning_device: device,
            gate: Calibration::default(),
        }
    }

    /// Like [`Self::new`], with the mode machine allowed to run: after
    /// enough observed frames it promotes to the calibrated profile and,
    /// once the gate clears, may hand the live path whole-frame executor
    /// decisions. Without this opt-in the planner stays in
    /// [`PlannerMode::Shadow`] no matter how much data arrives.
    #[must_use]
    pub fn with_authoritative(device: DeviceProfile) -> Self {
        let mut out = Self::new(device);
        out.authoritative_enabled = true;
        out
    }

    #[must_use]
    pub const fn device(&self) -> &DeviceProfile {
        &self.device
    }

    #[must_use]
    pub const fn calibration(&self) -> &Calibration {
        &self.calibration
    }

    /// The gate reading — predictions versus measurements *under the
    /// currently-planning profile*, filled since the last promotion.
    /// Empty and meaningless in [`PlannerMode::Shadow`].
    #[must_use]
    pub const fn gate(&self) -> &Calibration {
        &self.gate
    }

    #[must_use]
    pub const fn mode(&self) -> PlannerMode {
        self.mode
    }

    /// Plan one frame of `scene` at `width`×`height` device pixels.
    ///
    /// Predictions come from the profile the mode machine is currently
    /// running — the raw one in shadow, the calibrated one after promotion.
    ///
    /// `None` if the render graph does not compile. `build_graph` is not
    /// known to produce such a graph, but this runs inside a live frame loop
    /// as a diagnostic, and a diagnostic must never be the thing that takes a
    /// window down — a missing shadow line is the right failure.
    #[must_use]
    pub fn plan(&self, scene: &Scene, width: u32, height: u32) -> Option<ShadowPlan> {
        let scene_graph = SceneGraph::from_scene(scene);
        let (graph, screen) = vieww_render_graph::build_graph(&scene_graph, width, height);
        let exec = graph.compile(screen).ok()?;
        let budget = FrameBudget::new(self.planning_device.frame_deadline_ms());
        let (frame_plan, _per_pass) =
            plan_frame(&graph, &exec, &self.planning_device, &self.model, budget);

        let mut passes = Vec::with_capacity(exec.order.len());
        let mut all_cpu_ms = 0.0f32;
        let mut all_gpu_ms = self.planning_device.has_gpu().then_some(0.0f32);
        let mut pixels_total = 0.0f32;
        for &id in &exec.order {
            let pass = graph.pass(id);
            let pixels = work_pixels(&graph, pass);
            let estimate = self.model.estimate(pass, pixels, &self.planning_device);
            all_cpu_ms += estimate.cpu_ms;
            if let (Some(total), Some(gpu)) = (all_gpu_ms.as_mut(), estimate.gpu_ms) {
                *total += gpu;
            }
            pixels_total += pixels;
            passes.push(PassSummary {
                name: pass.name,
                kind: pass.kind,
                pixels,
                placement: frame_plan
                    .placements
                    .get(&id)
                    .copied()
                    .unwrap_or(Placement::Cpu),
                cpu_ms: estimate.cpu_ms,
                gpu_ms: estimate.gpu_ms,
            });
        }

        Some(ShadowPlan {
            frame_plan,
            passes,
            all_cpu_ms,
            all_gpu_ms,
            pixels: pixels_total,
        })
    }

    /// The frame's plan, plus the whole-frame executor the live path is
    /// being asked to run it on — `Some` only in
    /// [`PlannerMode::Authoritative`], only for a `Cpu` or `Gpu`
    /// recommendation (never `Mixed`: no live backend can execute a split
    /// frame), and only when that executor's gate reading is `ready` at
    /// this moment. A `Gpu` choice is deliberately never returned: the
    /// live path already prefers the GPU whenever its own
    /// `ScenePlan::is_complete` gate passes, so "acting" on it would be a
    /// no-op wearing a decision's clothes. The plan is returned either way
    /// so the caller's `observe` can record this exact prediction instead
    /// of planning the frame a second time.
    #[must_use]
    pub fn plan_for_frame(
        &self,
        scene: &Scene,
        width: u32,
        height: u32,
    ) -> (Option<Executor>, Option<ShadowPlan>) {
        let Some(plan) = self.plan(scene, width, height) else {
            return (None, None);
        };
        let act = if self.mode == PlannerMode::Authoritative
            && self.gate.ready(Executor::Cpu)
            && plan.recommendation() == Recommendation::Cpu
        {
            Some(Executor::Cpu)
        } else {
            None
        };
        (act, Some(plan))
    }

    /// Record what the live path actually did with a frame planned by
    /// [`Self::plan`] or [`Self::plan_for_frame`], fold it into the raw
    /// [`Self::calibration`] and — past promotion — into the
    /// [`Self::gate`], then run the mode machine's forward step.
    pub fn observe(
        &mut self,
        plan: ShadowPlan,
        label: impl Into<String>,
        (width, height): (u32, u32),
        actual: Executor,
        actual_ms: f32,
        fallback: Option<String>,
    ) -> ShadowFrame {
        let frame = ShadowFrame {
            frame: self.next_frame,
            label: label.into(),
            width,
            height,
            plan,
            actual,
            actual_ms,
            fallback,
        };
        self.next_frame += 1;
        self.calibration.record(&frame, &self.planning_device);
        if self.mode != PlannerMode::Shadow {
            self.gate.record(&frame, &self.planning_device);
        }
        self.advance();
        frame
    }

    /// The mode machine's single forward step, run after every observation.
    /// One direction only, and each transition requires its own data: the
    /// promotion needs the raw calibration to have something to correct
    /// with, and actuation needs the gate to have cleared.
    fn advance(&mut self) {
        if !self.authoritative_enabled {
            return;
        }
        match self.mode {
            PlannerMode::Shadow => {
                if self.calibration.frames() >= AUTHORITATIVE_MIN_FRAMES
                    && (self.calibration.cpu_error_ratio().is_some()
                        || self.calibration.gpu_error_ratio().is_some())
                {
                    self.planning_device = self.calibration.calibrated(&self.device);
                    self.gate = Calibration::default();
                    self.mode = PlannerMode::ShadowCalibrated;
                }
            }
            PlannerMode::ShadowCalibrated => {
                if self.gate.ready(Executor::Cpu) || self.gate.ready(Executor::Gpu) {
                    self.mode = PlannerMode::Authoritative;
                }
            }
            PlannerMode::Authoritative => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vieww_foundation::{BlendMode, Color, ImageFilter, Rect};
    use vieww_paint::{Canvas, Paint};

    fn flat() -> Scene {
        let mut scene = Scene::new();
        scene.fill_rect(Rect::new(0.0, 0.0, 100.0, 100.0), Paint::solid(Color::RED));
        scene
    }

    fn glass() -> Scene {
        let mut scene = Scene::new();
        scene.fill_rect(
            Rect::new(0.0, 0.0, 1000.0, 1000.0),
            Paint::solid(Color::BLUE),
        );
        scene.push_filtered_layer(
            Rect::new(0.0, 0.0, 1000.0, 1000.0),
            1.0,
            BlendMode::Normal,
            ImageFilter::blur(20.0),
        );
        scene.fill_rect(
            Rect::new(0.0, 0.0, 1000.0, 1000.0),
            Paint::solid(Color::RED),
        );
        scene.pop_layer();
        scene
    }

    #[test]
    fn a_cpu_only_profile_recommends_the_cpu_and_agrees_with_a_cpu_frame() {
        let mut shadow = ShadowPlanner::new(DeviceProfile::CPU_ONLY_FALLBACK);
        let plan = shadow.plan(&flat(), 200, 200).unwrap();
        assert_eq!(plan.recommendation(), Recommendation::Cpu);
        assert!(plan.all_gpu_ms.is_none());
        let frame = shadow.observe(plan, "flat", (200, 200), Executor::Cpu, 1.0, None);
        assert!(frame.agrees());
        assert_eq!(shadow.calibration().agreeing_frames(), 1);
    }

    #[test]
    fn the_plan_lists_every_pass_in_order_with_its_pixels() {
        let shadow = ShadowPlanner::new(DeviceProfile::DESKTOP_DISCRETE);
        let plan = shadow.plan(&glass(), 1000, 1000).unwrap();
        let names: Vec<_> = plan.passes.iter().map(|p| p.name).collect();
        assert_eq!(names.first(), Some(&"root_raster"));
        assert_eq!(names.last(), Some(&"present_blit"));
        assert!(names.contains(&"layer_filter"));
        let summed: f32 = plan.passes.iter().map(|p| p.pixels).sum();
        assert!((summed - plan.pixels).abs() < 1.0);
        assert!(plan.all_gpu_ms.unwrap() < plan.all_cpu_ms);
    }

    #[test]
    fn a_gpu_leaning_plan_disagrees_with_a_cpu_fallback_and_says_why() {
        let mut shadow = ShadowPlanner::new(DeviceProfile::DESKTOP_DISCRETE);
        let plan = shadow.plan(&glass(), 1000, 1000).unwrap();
        assert_ne!(plan.recommendation(), Recommendation::Cpu);
        let frame = shadow.observe(
            plan,
            "glass",
            (1000, 1000),
            Executor::Cpu,
            12.0,
            Some("plan incomplete: Blur".to_owned()),
        );
        assert!(!frame.agrees());
        let line = frame.to_json_line();
        assert!(line.starts_with("{\"frame\":0,\"label\":\"glass\""));
        assert!(line.contains("\"fallback\":\"plan incomplete: Blur\""));
        assert!(line.contains("\"agrees\":false"));
        assert!(line.ends_with("]}"));
        assert!(!line.contains('\n'));
    }

    #[test]
    fn calibration_recovers_the_throughput_that_would_have_been_right() {
        let device = DeviceProfile::CPU_ONLY_FALLBACK;
        let mut shadow = ShadowPlanner::new(device);
        for _ in 0..4 {
            let plan = shadow.plan(&flat(), 500, 500).unwrap();
            // This "device" is exactly twice as slow as the profile claims.
            let actual = plan.all_cpu_ms * 2.0;
            shadow.observe(plan, "flat", (500, 500), Executor::Cpu, actual, None);
        }
        let ratio = shadow.calibration().cpu_error_ratio().unwrap();
        assert!((ratio - 2.0).abs() < 1e-4, "{ratio}");
        let fixed = shadow.calibration().calibrated(&device);
        assert!((fixed.cpu_fill_mpix_per_sec - device.cpu_fill_mpix_per_sec / 2.0).abs() < 1e-3);

        // With the corrected profile, the same frame is predicted correctly.
        let replanned = ShadowPlanner::new(fixed).plan(&flat(), 500, 500).unwrap();
        let original = ShadowPlanner::new(device).plan(&flat(), 500, 500).unwrap();
        assert!((replanned.all_cpu_ms - original.all_cpu_ms * 2.0).abs() < 1e-3);
    }

    #[test]
    fn gpu_calibration_holds_overhead_fixed_and_ignores_impossible_frames() {
        let device = DeviceProfile::DESKTOP_DISCRETE;
        let mut shadow = ShadowPlanner::new(device);
        let plan = shadow.plan(&glass(), 1000, 1000).unwrap();
        // Faster than the assumed dispatch overhead alone: says nothing
        // about fill rate, so the fill rate is left alone.
        shadow.observe(plan, "g", (1000, 1000), Executor::Gpu, 0.0001, None);
        let fixed = shadow.calibration().calibrated(&device);
        assert_eq!(fixed.gpu_fill_mpix_per_sec, device.gpu_fill_mpix_per_sec);
        assert!(shadow.calibration().gpu_error_ratio().is_some());
    }

    #[test]
    fn rmse_is_the_spread_of_the_ratios_not_their_total() {
        // Two devices whose total error is identical — every frame 3× the
        // prediction — but one is a steady 3× and the other alternates 1×
        // and 5×. The ratio of sums cannot tell them apart; the RMSE can,
        // and the gate is stated on the RMSE for exactly that reason.
        let device = DeviceProfile::CPU_ONLY_FALLBACK;
        let mut steady = ShadowPlanner::new(device);
        let steady_plan = steady.plan(&flat(), 400, 400).unwrap();
        for _ in 0..10 {
            let observed = steady_plan.clone();
            steady.observe(
                observed,
                "flat",
                (400, 400),
                Executor::Cpu,
                steady_plan.all_cpu_ms * 3.0,
                None,
            );
        }
        let steady_rmse = steady.calibration().rmse_of_ratio(Executor::Cpu).unwrap();
        assert!((steady_rmse - 2.0).abs() < 1e-6, "{steady_rmse}");

        let mut wild = ShadowPlanner::new(device);
        let wild_plan = wild.plan(&flat(), 400, 400).unwrap();
        for i in 0..10 {
            let observed = wild_plan.clone();
            let factor = if i % 2 == 0 { 1.0 } else { 5.0 };
            wild.observe(
                observed,
                "flat",
                (400, 400),
                Executor::Cpu,
                wild_plan.all_cpu_ms * factor,
                None,
            );
        }
        let wild_ratio = wild.calibration().cpu_error_ratio().unwrap();
        let wild_rmse = wild.calibration().rmse_of_ratio(Executor::Cpu).unwrap();
        assert!(
            (wild_ratio - 3.0).abs() < 1e-4,
            "same total error: {wild_ratio}"
        );
        assert!(
            wild_rmse > steady_rmse + 0.5,
            "the swinging error must read worse: {wild_rmse} against {steady_rmse}"
        );
    }

    #[test]
    fn the_gate_needs_the_full_sample_window_even_at_perfect_accuracy() {
        let device = DeviceProfile::CPU_ONLY_FALLBACK;
        let mut shadow = ShadowPlanner::new(device);
        let plan = shadow.plan(&flat(), 400, 400).unwrap();
        for _ in 0..(AUTHORITATIVE_MIN_FRAMES - 1) {
            let observed = plan.clone();
            shadow.observe(
                observed,
                "flat",
                (400, 400),
                Executor::Cpu,
                plan.all_cpu_ms,
                None,
            );
        }
        assert!(
            !shadow.calibration().ready(Executor::Cpu),
            "one frame short of the window is not ready, whatever the RMSE"
        );
        let observed = plan.clone();
        shadow.observe(
            observed,
            "flat",
            (400, 400),
            Executor::Cpu,
            plan.all_cpu_ms,
            None,
        );
        assert!(
            shadow.calibration().ready(Executor::Cpu),
            "a model whose every frame measured exactly what it predicted, \
             over the full window, is ready by construction"
        );
    }

    #[test]
    fn without_the_opt_in_the_machine_never_leaves_shadow() {
        let mut shadow = ShadowPlanner::new(DeviceProfile::DESKTOP_DISCRETE);
        let plan = shadow.plan(&flat(), 400, 400).unwrap();
        for _ in 0..(2 * AUTHORITATIVE_MIN_FRAMES + 10) {
            let observed = plan.clone();
            shadow.observe(
                observed,
                "flat",
                (400, 400),
                Executor::Cpu,
                plan.all_cpu_ms,
                None,
            );
        }
        assert_eq!(shadow.mode(), PlannerMode::Shadow);
        assert_eq!(shadow.plan_for_frame(&flat(), 400, 400).0, None);
    }

    #[test]
    fn an_opted_in_planner_promotes_then_actuates_when_the_gate_clears() {
        // A tiny flat frame on a GPU-capable profile: the planner
        // recommends the CPU, because at 40x40 the GPU's dispatch
        // overhead alone costs more than the whole fill — the classic
        // CPU-affinity case, arrived at by arithmetic rather than by the
        // hint. The "device" then measures exactly what the model
        // predicts, so the raw calibration is a perfect correction and
        // the gate clears the moment its window is full. The frames after
        // that are the ones `plan_for_frame` may act on.
        let mut shadow = ShadowPlanner::with_authoritative(DeviceProfile::DESKTOP_DISCRETE);
        let plan = shadow.plan(&flat(), 40, 40).unwrap();
        assert_eq!(plan.recommendation(), Recommendation::Cpu, "test premise");
        let mut acted = 0;
        for _ in 0..(2 * AUTHORITATIVE_MIN_FRAMES + 10) {
            let (act, planned) = shadow.plan_for_frame(&flat(), 40, 40);
            let planned = planned.expect("a flat scene always compiles");
            // The live path measures whatever it measures; here it is
            // exactly the prediction, which is the best case the gate can
            // see. Read before the plan moves into `observe`.
            let predicted = planned.all_cpu_ms;
            shadow.observe(planned, "flat", (40, 40), Executor::Cpu, predicted, None);
            if act == Some(Executor::Cpu) {
                acted += 1;
            }
        }
        assert_eq!(shadow.mode(), PlannerMode::Authoritative);
        assert!(
            acted > 0,
            "an Authoritative planner with a ready CPU gate acts"
        );
    }

    #[test]
    fn json_numbers_are_compact_and_finite() {
        assert_eq!(fmt_f32(1.5), "1.5");
        assert_eq!(fmt_f32(2.0), "2");
        assert_eq!(fmt_f32(0.0), "0");
        assert_eq!(fmt_f32(f32::NAN), "null");
        assert_eq!(json_str("a\"b\n"), "\"a\\\"b\\u000a\"");
    }
}
