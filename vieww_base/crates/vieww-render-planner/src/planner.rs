//! Top-level orchestration: turn a compiled [`ExecutionPlan`] into a
//! per-pass [`Placement`] for one specific device and frame budget.

use std::collections::HashMap;

use vieww_render_graph::{ExecutionPlan, Graph, PassDesc, PassId, PassKind};

use crate::capabilities::DeviceProfile;
use crate::cost::CostModel;
use crate::frame_budget::FrameBudget;
use crate::policy::{self, Placement};

/// The planner's output for one frame: a placement for every surviving
/// pass, plus the summary numbers `docs/RENDERER-V2-NOTES.md`'s benchmark
/// section and this workspace's own README example ask for directly
/// ("Frame 100: GPU = 82%, CPU = 18%").
#[derive(Debug, Clone, PartialEq)]
pub struct FramePlan {
    pub placements: HashMap<PassId, Placement>,
    pub predicted_ms: f32,
    pub deadline_ms: f32,
}

impl FramePlan {
    /// Fraction of total predicted work-time running on the GPU, weighted
    /// by each pass's own predicted duration — a pass placed `Hybrid`
    /// contributes proportionally to its split, not as a whole unit either
    /// way.
    #[must_use]
    pub fn gpu_share(&self, per_pass_ms: &HashMap<PassId, f32>) -> f32 {
        let total: f32 = per_pass_ms.values().sum();
        if total <= 0.0 {
            return 0.0;
        }
        let gpu_weighted: f32 = self
            .placements
            .iter()
            .map(|(id, placement)| {
                per_pass_ms.get(id).copied().unwrap_or(0.0) * placement.gpu_fraction()
            })
            .sum();
        gpu_weighted / total
    }

    #[must_use]
    pub fn missed_deadline(&self) -> bool {
        self.predicted_ms > self.deadline_ms
    }
}

/// How many pixels of work a pass does — the unit every [`CostModel`]
/// estimate is in.
///
/// For most passes that is the area of what it writes. A **composite** is the
/// exception, and the one that matters: `build_graph` gives each composite a
/// fresh output the size of its *parent* (the whole screen, for a root-level
/// layer), because that is what the graph's resources are — but blending a
/// layer into its parent touches only the layer's own pixels. Charging the
/// output instead made a screen of 384 small sibling layers (`fixtures`'
/// `00-layers-flat`) cost 384 full-screen passes: 182 megapixels and a
/// predicted 727 ms, for a frame the CPU draws in 6 ms. The first shadow-mode
/// run over the gallery found it (2026-10-15); see `shadow.rs`.
///
/// So a composite is charged the area of the smallest resource it reads — the
/// child layer, which `build_graph` sizes to the layer's bounds.
#[must_use]
pub fn work_pixels(graph: &Graph, pass: &PassDesc) -> f32 {
    let area = |r: &vieww_render_graph::ResourceId| {
        let desc = graph.resource(*r);
        #[expect(
            clippy::cast_precision_loss,
            reason = "texture dimensions are far below f32's exact range"
        )]
        let a = (desc.width as f32) * (desc.height.max(1) as f32);
        a
    };
    let written: f32 = pass.writes.iter().map(area).sum();
    if pass.kind == PassKind::Composite {
        if let Some(smallest_read) = pass.reads.iter().map(area).reduce(f32::min) {
            return smallest_read;
        }
    }
    written
}

/// Plan one frame.
///
/// `graph` and `plan` must be the [`Graph`]/[`ExecutionPlan`] pair produced
/// together (`plan` came from `graph.compile(..)`) — this function trusts
/// that pairing rather than re-deriving it.
pub fn plan_frame(
    graph: &Graph,
    exec_plan: &ExecutionPlan,
    device: &DeviceProfile,
    model: &dyn CostModel,
    mut budget: FrameBudget,
) -> (FramePlan, HashMap<PassId, f32>) {
    let mut placements = HashMap::new();
    let mut per_pass_ms = HashMap::new();

    for &pass_id in &exec_plan.order {
        let pass = graph.pass(pass_id);
        let pixels = work_pixels(graph, pass);

        let estimate = model.estimate(pass, pixels, device);
        let placement = policy::decide(pass, pixels, estimate, device, &budget);

        let ms = match placement {
            Placement::Cpu => estimate.cpu_ms,
            Placement::Gpu => estimate.gpu_ms.unwrap_or(estimate.cpu_ms),
            Placement::Hybrid(split) => {
                let gpu_ms = estimate.gpu_ms.unwrap_or(estimate.cpu_ms);
                // The two sides run concurrently, so the pass's wall time is
                // the slower of the two shares, not their sum.
                (split.cpu_fraction * estimate.cpu_ms).max(split.gpu_fraction() * gpu_ms)
            }
        };

        budget.spend(ms);
        placements.insert(pass_id, placement);
        per_pass_ms.insert(pass_id, ms);
    }

    let predicted_ms = per_pass_ms.values().sum();
    (
        FramePlan {
            placements,
            predicted_ms,
            deadline_ms: budget.deadline_ms,
        },
        per_pass_ms,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cost::HeuristicCostModel;
    use vieww_foundation::{Color, Rect};
    use vieww_paint::{Canvas, Paint, Scene};
    use vieww_scene::SceneGraph;

    #[test]
    fn a_cpu_only_device_places_every_pass_on_cpu() {
        let mut scene = Scene::new();
        scene.fill_rect(Rect::new(0.0, 0.0, 10.0, 10.0), Paint::solid(Color::RED));
        let scene_graph = SceneGraph::from_scene(&scene);
        let (graph, screen) = vieww_render_graph::build_graph(&scene_graph, 200, 200);
        let exec_plan = graph.compile(screen).unwrap();

        let (frame_plan, _) = plan_frame(
            &graph,
            &exec_plan,
            &DeviceProfile::CPU_ONLY_FALLBACK,
            &HeuristicCostModel,
            FrameBudget::new(16.0),
        );

        assert!(frame_plan
            .placements
            .values()
            .all(|p| matches!(p, Placement::Cpu)));
    }

    #[test]
    fn a_composite_is_charged_its_layer_not_the_screen() {
        let mut scene = Scene::new();
        for i in 0..8 {
            let x = i as f32 * 20.0;
            // `Rect::new` is left, top, right, bottom.
            let r = Rect::new(x, 0.0, x + 10.0, 10.0);
            scene.push_layer(r, 0.5, vieww_foundation::BlendMode::Normal);
            scene.fill_rect(r, Paint::solid(Color::RED));
            scene.pop_layer();
        }
        let scene_graph = SceneGraph::from_scene(&scene);
        let (graph, screen) = vieww_render_graph::build_graph(&scene_graph, 1000, 1000);
        let exec_plan = graph.compile(screen).unwrap();
        let composites: Vec<f32> = exec_plan
            .order
            .iter()
            .map(|id| graph.pass(*id))
            .filter(|p| p.kind == PassKind::Composite)
            .map(|p| work_pixels(&graph, p))
            .collect();
        assert_eq!(composites.len(), 8);
        // Each is a 10x10 layer, not a 1000x1000 screen.
        assert!(composites.iter().all(|px| *px <= 200.0), "{composites:?}");
    }

    #[test]
    fn a_heavy_blur_on_a_capable_device_lands_on_the_gpu() {
        let mut scene = Scene::new();
        scene.push_filtered_layer(
            Rect::new(0.0, 0.0, 1000.0, 1000.0),
            1.0,
            vieww_foundation::BlendMode::Normal,
            vieww_foundation::ImageFilter::blur(20.0),
        );
        scene.fill_rect(
            Rect::new(0.0, 0.0, 1000.0, 1000.0),
            Paint::solid(Color::RED),
        );
        scene.pop_layer();
        let scene_graph = SceneGraph::from_scene(&scene);
        let (graph, screen) = vieww_render_graph::build_graph(&scene_graph, 1000, 1000);
        let exec_plan = graph.compile(screen).unwrap();

        let (frame_plan, per_pass_ms) = plan_frame(
            &graph,
            &exec_plan,
            &DeviceProfile::DESKTOP_DISCRETE,
            &HeuristicCostModel,
            FrameBudget::new(16.0),
        );

        assert!(frame_plan.gpu_share(&per_pass_ms) > 0.0);
    }
}
