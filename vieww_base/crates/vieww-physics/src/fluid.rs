//! Particle fluid — Clavet, Beaudoin & Poulin's viscoelastic SPH (2005).
//!
//! The fluids the capability matrix lists under physics simulation
//! (Unreal's Niagara fluids, Blender's Mantaflow, Houdini FLIP) are, in a
//! 2D real-time setting, almost always this method: particles with
//! **double density relaxation** (a pressure term plus a *near*-pressure
//! term that keeps particles from clumping, which is what gives the method
//! its surface tension and its stability), pairwise **viscosity** impulses,
//! and prediction–relaxation integration (velocities are *derived* from the
//! corrected positions, so the step is stable at game time steps where
//! explicit SPH would explode). A uniform grid hash keeps neighbour search
//! linear.
//!
//! The `step` is deterministic for a given particle order: neighbours are
//! visited in grid order and nothing depends on hash iteration order.

use std::collections::HashMap;

use vieww_foundation::{Offset, Rect};

/// Tuning knobs, with Clavet's names.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FluidParams {
    /// Interaction radius `h`.
    pub radius: f32,
    /// Rest density `ρ0` (in kernel units).
    pub rest_density: f32,
    /// Pressure stiffness `k`.
    pub stiffness: f32,
    /// Near-pressure stiffness `k_near`.
    pub near_stiffness: f32,
    /// Linear and quadratic viscosity `σ`, `β`.
    pub viscosity: (f32, f32),
    pub gravity: Offset,
    /// The container.
    pub bounds: Rect,
}

impl Default for FluidParams {
    fn default() -> Self {
        Self {
            radius: 16.0,
            rest_density: 1.6,
            // Per-frame constants of 1.0 and 3.0 (Clavet's paper uses 0.004
            // and 0.01 in its own units) rescaled for a clock in seconds at
            // 60 Hz — the displacement is dt²·k, so ×3600. Chosen by a sweep
            // for pixel-scale particles under 600 px/s² gravity: the tank
            // settles at its incompressible height with no clumping.
            stiffness: 3600.0,
            near_stiffness: 3.0 * 3600.0,
            viscosity: (0.0, 0.02),
            gravity: Offset::new(0.0, 600.0),
            bounds: Rect::new(0.0, 0.0, 400.0, 300.0),
        }
    }
}

/// The particle system.
#[derive(Debug, Clone)]
pub struct Fluid {
    pub params: FluidParams,
    pub positions: Vec<Offset>,
    pub velocities: Vec<Offset>,
    densities: Vec<f32>,
}

impl Fluid {
    #[must_use]
    pub fn new(params: FluidParams) -> Self {
        Self {
            params,
            positions: Vec::new(),
            velocities: Vec::new(),
            densities: Vec::new(),
        }
    }

    /// Fill `rect` with particles `spacing` apart (a dam, a drop).
    pub fn fill(&mut self, rect: Rect, spacing: f32) {
        let mut y = rect.top;
        let mut row = 0;
        while y <= rect.bottom {
            // Offset alternate rows so the block is not a perfect lattice.
            let mut x = rect.left + if row % 2 == 1 { spacing / 2.0 } else { 0.0 };
            while x <= rect.right {
                self.positions.push(Offset::new(x, y));
                self.velocities.push(Offset::ZERO);
                x += spacing;
            }
            y += spacing;
            row += 1;
        }
        self.densities = vec![0.0; self.positions.len()];
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.positions.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    /// Each particle's density from the last step (for colouring).
    #[must_use]
    pub fn densities(&self) -> &[f32] {
        &self.densities
    }

    fn neighbours(&self) -> Vec<Vec<usize>> {
        let h = self.params.radius;
        let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        #[allow(clippy::cast_possible_truncation)]
        let cell = |p: Offset| ((p.dx / h).floor() as i32, (p.dy / h).floor() as i32);
        for (i, p) in self.positions.iter().enumerate() {
            grid.entry(cell(*p)).or_default().push(i);
        }
        self.positions
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let (cx, cy) = cell(*p);
                let mut out = Vec::new();
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if let Some(list) = grid.get(&(cx + dx, cy + dy)) {
                            for &j in list {
                                if j > i && (self.positions[j] - *p).distance_squared() < h * h {
                                    out.push(j);
                                }
                            }
                        }
                    }
                }
                out.sort_unstable();
                out
            })
            .collect()
    }

    /// Advance one step.
    pub fn step(&mut self, dt: f32) {
        let p = self.params;
        let h = p.radius;
        let n = self.positions.len();
        for v in &mut self.velocities {
            *v = *v + p.gravity.scale(dt);
        }
        let nb = self.neighbours();
        // Viscosity impulses.
        #[allow(clippy::needless_range_loop)]
        for i in 0..n {
            for &j in &nb[i] {
                let d = self.positions[j] - self.positions[i];
                let r = d.distance();
                if r < 1e-6 {
                    continue;
                }
                let q = r / h;
                let u = d.scale(1.0 / r);
                let inward = (self.velocities[i].dx - self.velocities[j].dx) * u.dx
                    + (self.velocities[i].dy - self.velocities[j].dy) * u.dy;
                if inward > 0.0 {
                    let imp =
                        dt * (1.0 - q) * (p.viscosity.0 * inward + p.viscosity.1 * inward * inward);
                    let iv = u.scale(imp / 2.0);
                    self.velocities[i] = self.velocities[i] - iv;
                    self.velocities[j] = self.velocities[j] + iv;
                }
            }
        }
        // Predict.
        let prev = self.positions.clone();
        for i in 0..n {
            self.positions[i] = self.positions[i] + self.velocities[i].scale(dt);
        }
        // Double density relaxation.
        let nb = self.neighbours();
        let mut density = vec![0.0f32; n];
        let mut near = vec![0.0f32; n];
        for i in 0..n {
            for &j in &nb[i] {
                let q = 1.0 - (self.positions[j] - self.positions[i]).distance() / h;
                if q > 0.0 {
                    density[i] += q * q;
                    density[j] += q * q;
                    near[i] += q * q * q;
                    near[j] += q * q * q;
                }
            }
        }
        let dt2 = dt * dt;
        for i in 0..n {
            let pressure = p.stiffness * (density[i] - p.rest_density);
            let near_pressure = p.near_stiffness * near[i];
            let mut dx = Offset::ZERO;
            for &j in &nb[i] {
                let d = self.positions[j] - self.positions[i];
                let r = d.distance();
                if r < 1e-6 {
                    continue;
                }
                let q = 1.0 - r / h;
                if q > 0.0 {
                    let pj = p.stiffness * (density[j] - p.rest_density);
                    let nj = p.near_stiffness * near[j];
                    let mag =
                        dt2 * ((pressure + pj) * 0.5 * q + (near_pressure + nj) * 0.5 * q * q);
                    let disp = d.scale(mag / r * 0.5);
                    self.positions[j] = self.positions[j] + disp;
                    dx = dx - disp;
                }
            }
            self.positions[i] = self.positions[i] + dx;
        }
        // Container.
        let b = p.bounds;
        for pos in &mut self.positions {
            pos.dx = pos.dx.clamp(b.left, b.right);
            pos.dy = pos.dy.clamp(b.top, b.bottom);
        }
        for ((v, p), q) in self.velocities.iter_mut().zip(&self.positions).zip(&prev) {
            *v = (*p - *q).scale(1.0 / dt);
        }
        self.densities = density;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dam() -> Fluid {
        let mut f = Fluid::new(FluidParams::default());
        f.fill(Rect::new(10.0, 60.0, 130.0, 280.0), 8.0);
        f
    }

    #[test]
    fn a_dam_breaks_spreads_and_stays_in_the_tank() {
        let mut f = dam();
        let start_right = f.positions.iter().map(|p| p.dx).fold(0.0f32, f32::max);
        for _ in 0..240 {
            f.step(1.0 / 60.0);
        }
        let b = f.params.bounds;
        assert!(f.positions.iter().all(|p| b.contains(*p)
            || (p.dx - b.right).abs() < 1e-3
            || (p.dy - b.bottom).abs() < 1e-3));
        let right = f.positions.iter().map(|p| p.dx).fold(0.0f32, f32::max);
        assert!(
            right > start_right + 100.0,
            "the water ran across the floor: {start_right} → {right}"
        );
        let top = f.positions.iter().map(|p| p.dy).fold(f32::MAX, f32::min);
        assert!(top > 150.0, "and the column fell: top {top}");
        assert!(f
            .velocities
            .iter()
            .all(|v| v.dx.is_finite() && v.dy.is_finite()));
    }

    #[test]
    fn particles_do_not_collapse_onto_each_other() {
        let mut f = dam();
        for _ in 0..300 {
            f.step(1.0 / 60.0);
        }
        // Nearest-neighbour distance per particle; wall clamping can stack a
        // transient pair, so the claim is about the bulk, not the worst pair.
        let nearest: Vec<f32> = (0..f.len())
            .map(|i| {
                (0..f.len())
                    .filter(|&j| j != i)
                    .map(|j| (f.positions[i] - f.positions[j]).distance())
                    .fold(f32::MAX, f32::min)
            })
            .collect();
        let crowded = nearest.iter().filter(|&&d| d < 2.0).count();
        assert!(
            crowded * 50 < f.len(),
            "near pressure keeps particles apart: {crowded} of {} crowded",
            f.len()
        );
    }

    #[test]
    fn it_is_deterministic() {
        let run = || {
            let mut f = dam();
            for _ in 0..60 {
                f.step(1.0 / 60.0);
            }
            f.positions
        };
        assert_eq!(run(), run());
    }
}
