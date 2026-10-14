//! Cloth — position-based dynamics on a particle grid.
//!
//! Unity's Cloth component, Unreal's Chaos Cloth, Blender's cloth modifier
//! (the capability matrix's "cloth" beside ragdoll and fluid): a sheet of
//! particles joined by distance constraints, integrated with Verlet (velocity
//! is implicit in the last two positions, which is what makes it
//! unconditionally stable for stiff constraints) and relaxed by
//! Gauss–Seidel projection — Jakobsen's *Advanced Character Physics*
//! (2001), the method behind a generation of game cloth.
//!
//! Features: pinned particles, gravity and gusting wind, damping,
//! structural and shear constraints, collision with circles, a particle you
//! can grab and drag, and **tearing** — a constraint stretched past
//! `tear_ratio` of its rest length breaks for good.

use vieww_foundation::Offset;

/// One particle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClothParticle {
    pub position: Offset,
    previous: Offset,
    pub pinned: bool,
}

/// A distance constraint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Link {
    pub a: usize,
    pub b: usize,
    pub rest: f32,
    pub broken: bool,
}

/// The sheet.
#[derive(Debug, Clone)]
pub struct Cloth {
    pub particles: Vec<ClothParticle>,
    pub links: Vec<Link>,
    pub gravity: Offset,
    pub wind: Offset,
    /// Fraction of velocity kept per step (0.99 is light air drag).
    pub damping: f32,
    pub iterations: u32,
    /// Stretch ratio past which a link breaks; `None` never tears.
    pub tear_ratio: Option<f32>,
    /// Circles particles cannot enter.
    pub colliders: Vec<(Offset, f32)>,
    grabbed: Option<(usize, Offset)>,
    time: f32,
    pub columns: usize,
}

impl Cloth {
    /// A `cols × rows` sheet with its top-left at `origin`, `spacing`
    /// apart, top-row particles pinned where `column % pin_every == 0`
    /// (0: none; a huge value pins only the top-left corner, a flag).
    #[must_use]
    pub fn grid(origin: Offset, cols: usize, rows: usize, spacing: f32, pin_every: usize) -> Self {
        let mut particles = Vec::with_capacity(cols * rows);
        for r in 0..rows {
            for c in 0..cols {
                #[allow(clippy::cast_precision_loss)]
                let p = Offset::new(
                    origin.dx + c as f32 * spacing,
                    origin.dy + r as f32 * spacing,
                );
                particles.push(ClothParticle {
                    position: p,
                    previous: p,
                    pinned: r == 0 && pin_every > 0 && c % pin_every == 0,
                });
            }
        }
        let mut links = Vec::new();
        let idx = |c: usize, r: usize| r * cols + c;
        let diag = spacing * std::f32::consts::SQRT_2;
        for r in 0..rows {
            for c in 0..cols {
                if c + 1 < cols {
                    links.push(Link {
                        a: idx(c, r),
                        b: idx(c + 1, r),
                        rest: spacing,
                        broken: false,
                    });
                }
                if r + 1 < rows {
                    links.push(Link {
                        a: idx(c, r),
                        b: idx(c, r + 1),
                        rest: spacing,
                        broken: false,
                    });
                }
                if c + 1 < cols && r + 1 < rows {
                    links.push(Link {
                        a: idx(c, r),
                        b: idx(c + 1, r + 1),
                        rest: diag,
                        broken: false,
                    });
                    links.push(Link {
                        a: idx(c + 1, r),
                        b: idx(c, r + 1),
                        rest: diag,
                        broken: false,
                    });
                }
            }
        }
        Self {
            particles,
            links,
            gravity: Offset::new(0.0, 900.0),
            wind: Offset::ZERO,
            damping: 0.99,
            iterations: 12,
            tear_ratio: None,
            colliders: Vec::new(),
            grabbed: None,
            time: 0.0,
            columns: cols,
        }
    }

    /// Hold particle `i` at `at` (the pointer).
    pub fn grab(&mut self, i: usize, at: Offset) {
        self.grabbed = Some((i, at));
    }

    pub fn release(&mut self) {
        self.grabbed = None;
    }

    /// Intact links.
    #[must_use]
    pub fn intact(&self) -> usize {
        self.links.iter().filter(|l| !l.broken).count()
    }

    /// Advance one step.
    pub fn step(&mut self, dt: f32) {
        self.time += dt;
        let dt2 = dt * dt;
        let gust = 0.6 + 0.4 * (self.time * 1.7).sin() * (self.time * 0.63).cos();
        let cols = self.columns.max(1);
        for (i, p) in self.particles.iter_mut().enumerate() {
            if p.pinned {
                continue;
            }
            #[allow(clippy::cast_precision_loss)]
            let flutter = 0.8 + 0.4 * ((i % cols) as f32 * 0.37 + self.time * 3.1).sin();
            let acc = self.gravity + self.wind.scale(gust * flutter);
            let vel = (p.position - p.previous).scale(self.damping);
            p.previous = p.position;
            p.position = p.position + vel + acc.scale(dt2);
        }
        for _ in 0..self.iterations {
            for li in 0..self.links.len() {
                let l = self.links[li];
                if l.broken {
                    continue;
                }
                let (pa, pb) = (self.particles[l.a], self.particles[l.b]);
                let d = pb.position - pa.position;
                let len = d.distance();
                if len < 1e-6 {
                    continue;
                }
                if let Some(ratio) = self.tear_ratio {
                    if len > l.rest * ratio {
                        self.links[li].broken = true;
                        continue;
                    }
                }
                let wa = if pa.pinned { 0.0 } else { 1.0 };
                let wb = if pb.pinned { 0.0 } else { 1.0 };
                if wa + wb == 0.0 {
                    continue;
                }
                let corr = d.scale((len - l.rest) / len / (wa + wb));
                self.particles[l.a].position = pa.position + corr.scale(wa);
                self.particles[l.b].position = pb.position - corr.scale(wb);
            }
            for p in &mut self.particles {
                for (c, r) in &self.colliders {
                    let d = p.position - *c;
                    let len = d.distance();
                    if len < *r && len > 1e-6 {
                        p.position = *c + d.scale(r / len);
                    }
                }
            }
            if let Some((i, at)) = self.grabbed {
                if let Some(p) = self.particles.get_mut(i) {
                    p.position = at;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hanging_sheet_keeps_its_pins_and_barely_stretches() {
        let mut c = Cloth::grid(Offset::new(0.0, 0.0), 12, 10, 10.0, 3);
        for _ in 0..240 {
            c.step(1.0 / 60.0);
        }
        assert_eq!(c.particles[0].position, Offset::new(0.0, 0.0));
        let max_stretch = c
            .links
            .iter()
            .map(|l| (c.particles[l.b].position - c.particles[l.a].position).distance() / l.rest)
            .fold(0.0f32, f32::max);
        assert!(max_stretch < 1.25, "{max_stretch}");
        let bottom = c.particles[c.particles.len() - 1].position.dy;
        assert!(bottom > 85.0, "hangs down: {bottom}");
    }

    #[test]
    fn pulling_hard_tears_it() {
        let mut c = Cloth::grid(Offset::new(0.0, 0.0), 10, 8, 10.0, 1);
        c.tear_ratio = Some(1.6);
        let before = c.intact();
        let corner = c.particles.len() - 5;
        for k in 0..60 {
            #[allow(clippy::cast_precision_loss)]
            c.grab(corner, Offset::new(45.0, 70.0 + k as f32 * 8.0));
            c.step(1.0 / 60.0);
        }
        assert!(
            c.intact() < before,
            "{} of {} links left",
            c.intact(),
            before
        );
    }

    #[test]
    fn colliders_keep_particles_out_and_wind_pushes() {
        let mut c = Cloth::grid(Offset::new(0.0, 0.0), 10, 10, 10.0, 1);
        c.colliders.push((Offset::new(45.0, 80.0), 25.0));
        for _ in 0..200 {
            c.step(1.0 / 60.0);
        }
        for p in &c.particles {
            assert!((p.position - Offset::new(45.0, 80.0)).distance() >= 24.9);
        }
        // A flag on one pin streams out in the wind (compared with calm).
        let flag = |wind: f32| {
            let mut w = Cloth::grid(Offset::new(0.0, 0.0), 8, 8, 10.0, 1000);
            w.wind = Offset::new(wind, 0.0);
            for _ in 0..200 {
                w.step(1.0 / 60.0);
            }
            w.particles.last().unwrap().position
        };
        let (calm, last) = (flag(0.0), flag(1500.0));
        assert!(
            last.dx > calm.dx + 25.0,
            "blown sideways: {calm:?} → {last:?}"
        );
    }
}
