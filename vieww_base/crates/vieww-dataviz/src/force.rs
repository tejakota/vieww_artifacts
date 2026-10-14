//! Force-directed layout — `d3-force`: velocity Verlet with alpha cooling,
//! many-body repulsion by **Barnes–Hut** (a quadtree, θ = 0.9), springy
//! links, centring, collision and positioning forces.
//!
//! Nodes start on D3's phyllotaxis spiral, so a simulation is
//! deterministic without a seed; `tick` is D3's tick, one alpha step.

use vieww_foundation::Offset;

/// A simulated node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FNode {
    pub position: Offset,
    pub velocity: Offset,
    /// Pinned position (D3's `fx`/`fy`).
    pub fixed: Option<Offset>,
    pub radius: f32,
}

/// A spring between two nodes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Link {
    pub source: usize,
    pub target: usize,
    pub distance: f32,
    /// `None`: D3's default `1 / min(degree(source), degree(target))`.
    pub strength: Option<f32>,
}

/// The simulation.
#[derive(Debug, Clone)]
pub struct Simulation {
    pub nodes: Vec<FNode>,
    pub links: Vec<Link>,
    pub alpha: f32,
    pub alpha_min: f32,
    pub alpha_decay: f32,
    pub alpha_target: f32,
    pub velocity_decay: f32,
    /// Many-body strength (negative repels).
    pub charge: f32,
    pub theta: f32,
    pub center: Option<Offset>,
    /// Collision strength (0 disables).
    pub collide: f32,
    /// Pull toward x / y positions (`forceX`/`forceY`) with a strength.
    pub pull_x: Option<(f32, f32)>,
    pub pull_y: Option<(f32, f32)>,
    /// Quadtree cells visited in the last tick (the Barnes–Hut receipt).
    pub last_visits: usize,
}

struct Quad {
    lo: Offset,
    size: f32,
    mass: f32,
    center: Offset,
    kids: [Option<Box<Quad>>; 4],
    /// The body (or, past the depth limit, coincident bodies) in a leaf.
    points: Vec<usize>,
}

impl Quad {
    fn new(lo: Offset, size: f32) -> Self {
        Self {
            lo,
            size,
            mass: 0.0,
            center: Offset::ZERO,
            kids: [None, None, None, None],
            points: Vec::new(),
        }
    }

    fn is_leaf(&self) -> bool {
        self.kids.iter().all(Option::is_none)
    }

    fn place(&mut self, j: usize, pt: Offset, depth: u32, all: &[Offset]) {
        let half = self.size / 2.0;
        let ix = usize::from(pt.dx >= self.lo.dx + half);
        let iy = usize::from(pt.dy >= self.lo.dy + half);
        #[allow(clippy::cast_precision_loss)]
        let lo = Offset::new(self.lo.dx + half * ix as f32, self.lo.dy + half * iy as f32);
        self.kids[iy * 2 + ix]
            .get_or_insert_with(|| Box::new(Self::new(lo, half)))
            .insert(j, pt, depth + 1, all);
    }

    fn insert(&mut self, i: usize, p: Offset, depth: u32, all: &[Offset]) {
        let total = self.mass + 1.0;
        self.center = Offset::new(
            (self.center.dx * self.mass + p.dx) / total,
            (self.center.dy * self.mass + p.dy) / total,
        );
        self.mass = total;
        if self.is_leaf() && (self.points.is_empty() || depth > 32) {
            // An empty leaf takes the body; past the depth limit coincident
            // bodies share one leaf.
            self.points.push(i);
            return;
        }
        for old in std::mem::take(&mut self.points) {
            self.place(old, all[old], depth, all);
        }
        self.place(i, p, depth, all);
    }
}

impl Simulation {
    /// Nodes on the phyllotaxis spiral, as `d3.forceSimulation` places them.
    #[must_use]
    pub fn new(count: usize) -> Self {
        let angle = std::f32::consts::PI * (3.0 - 5f32.sqrt());
        let nodes = (0..count)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let r = 10.0 * (0.5 + i as f32).sqrt();
                #[allow(clippy::cast_precision_loss)]
                let a = i as f32 * angle;
                FNode {
                    position: Offset::new(r * a.cos(), r * a.sin()),
                    velocity: Offset::ZERO,
                    fixed: None,
                    radius: 5.0,
                }
            })
            .collect();
        Self {
            nodes,
            links: Vec::new(),
            alpha: 1.0,
            alpha_min: 0.001,
            alpha_decay: 1.0 - 0.001f32.powf(1.0 / 300.0),
            alpha_target: 0.0,
            velocity_decay: 0.4,
            charge: -30.0,
            theta: 0.9,
            center: Some(Offset::ZERO),
            collide: 0.0,
            pull_x: None,
            pull_y: None,
            last_visits: 0,
        }
    }

    /// Add a link with D3's default strength.
    pub fn link(&mut self, source: usize, target: usize, distance: f32) {
        self.links.push(Link {
            source,
            target,
            distance,
            strength: None,
        });
    }

    /// Whether alpha has cooled below `alpha_min`.
    #[must_use]
    pub fn is_settled(&self) -> bool {
        self.alpha < self.alpha_min
    }

    fn build_tree(&self) -> Quad {
        let (mut lo, mut hi) = (
            Offset::new(f32::MAX, f32::MAX),
            Offset::new(f32::MIN, f32::MIN),
        );
        for n in &self.nodes {
            lo = Offset::new(lo.dx.min(n.position.dx), lo.dy.min(n.position.dy));
            hi = Offset::new(hi.dx.max(n.position.dx), hi.dy.max(n.position.dy));
        }
        let size = (hi.dx - lo.dx).max(hi.dy - lo.dy).max(1.0) * 1.001;
        let mut q = Quad::new(lo, size);
        let all: Vec<Offset> = self.nodes.iter().map(|n| n.position).collect();
        for (i, p) in all.iter().enumerate() {
            q.insert(i, *p, 0, &all);
        }
        q
    }

    fn many_body(&self, q: &Quad, i: usize, p: Offset, alpha: f32, visits: &mut usize) -> Offset {
        *visits += 1;
        let is_leaf = q.is_leaf();
        let own = if is_leaf {
            q.points.iter().filter(|&&j| j == i).count()
        } else {
            0
        };
        #[allow(clippy::cast_precision_loss)]
        let mass = q.mass - own as f32;
        if mass <= 0.0 {
            return Offset::ZERO;
        }
        let d = q.center - p;
        let l2 = d.distance_squared().max(1.0);
        if is_leaf || q.size * q.size / l2 < self.theta * self.theta {
            // Treat the cell as one body (D3: strength × alpha / l²).
            // A body never feels itself: a leaf holding `i` contributes the
            // others at the leaf's (shared) position.
            return d.scale(self.charge * alpha * mass / l2);
        }
        let mut f = Offset::ZERO;
        for k in q.kids.iter().flatten() {
            f = f + self.many_body(k, i, p, alpha, visits);
        }
        f
    }

    /// One tick: cool alpha, apply forces, integrate.
    pub fn tick(&mut self) {
        self.alpha += (self.alpha_target - self.alpha) * self.alpha_decay;
        let alpha = self.alpha;
        let n = self.nodes.len();
        // Links (D3 applies them to the predicted positions).
        let mut degree = vec![0usize; n];
        for l in &self.links {
            degree[l.source] += 1;
            degree[l.target] += 1;
        }
        for l in &self.links {
            let (s, t) = (self.nodes[l.source], self.nodes[l.target]);
            let mut d = (t.position + t.velocity) - (s.position + s.velocity);
            if d.distance() < 1e-6 {
                d = Offset::new(1e-3, 0.0);
            }
            let len = d.distance();
            #[allow(clippy::cast_precision_loss)]
            let strength = l
                .strength
                .unwrap_or(1.0 / degree[l.source].min(degree[l.target]).max(1) as f32);
            let k = (len - l.distance) / len * alpha * strength;
            #[allow(clippy::cast_precision_loss)]
            let bias = degree[l.source] as f32 / (degree[l.source] + degree[l.target]) as f32;
            let f = d.scale(k);
            self.nodes[l.target].velocity = self.nodes[l.target].velocity - f.scale(bias);
            self.nodes[l.source].velocity = self.nodes[l.source].velocity + f.scale(1.0 - bias);
        }
        // Many-body via Barnes–Hut.
        if self.charge != 0.0 && n > 1 {
            let tree = self.build_tree();
            let mut visits = 0;
            let forces: Vec<Offset> = (0..n)
                .map(|i| self.many_body(&tree, i, self.nodes[i].position, alpha, &mut visits))
                .collect();
            for (node, f) in self.nodes.iter_mut().zip(forces) {
                node.velocity = node.velocity + f;
            }
            self.last_visits = visits;
        }
        // Collision (pairwise; fine for the few hundred nodes a UI shows).
        if self.collide > 0.0 {
            for i in 0..n {
                for j in (i + 1)..n {
                    let (a, b) = (self.nodes[i], self.nodes[j]);
                    let pa = a.position + a.velocity;
                    let pb = b.position + b.velocity;
                    let d = pb - pa;
                    let r = a.radius + b.radius;
                    let l = d.distance();
                    if l < r && l > 1e-6 {
                        let push = d.scale((r - l) / l * self.collide * 0.5);
                        self.nodes[i].velocity = self.nodes[i].velocity - push;
                        self.nodes[j].velocity = self.nodes[j].velocity + push;
                    }
                }
            }
        }
        if let Some((x, s)) = self.pull_x {
            for node in &mut self.nodes {
                node.velocity.dx += (x - node.position.dx) * s * alpha;
            }
        }
        if let Some((y, s)) = self.pull_y {
            for node in &mut self.nodes {
                node.velocity.dy += (y - node.position.dy) * s * alpha;
            }
        }
        // Integrate.
        for node in &mut self.nodes {
            if let Some(f) = node.fixed {
                node.position = f;
                node.velocity = Offset::ZERO;
            } else {
                node.velocity = node.velocity.scale(1.0 - self.velocity_decay);
                node.position = node.position + node.velocity;
            }
        }
        // Centring (a translation of the whole, not a force).
        if let Some(c) = self.center {
            #[allow(clippy::cast_precision_loss)]
            let inv = 1.0 / n.max(1) as f32;
            let mean = self
                .nodes
                .iter()
                .fold(Offset::ZERO, |acc, nd| acc + nd.position)
                .scale(inv);
            let shift = c - mean;
            for node in &mut self.nodes {
                if node.fixed.is_none() {
                    node.position = node.position + shift;
                }
            }
        }
    }

    /// Tick until settled (at most `max` ticks); returns ticks run.
    pub fn run(&mut self, max: usize) -> usize {
        let mut t = 0;
        while !self.is_settled() && t < max {
            self.tick();
            t += 1;
        }
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chain_settles_to_its_link_lengths() {
        let mut s = Simulation::new(6);
        for i in 0..5 {
            s.link(i, i + 1, 40.0);
        }
        let ticks = s.run(1000);
        assert!(s.is_settled() && ticks < 400, "{ticks}");
        for l in &s.links {
            let d = (s.nodes[l.source].position - s.nodes[l.target].position).distance();
            assert!((d - 40.0).abs() < 12.0, "{d}");
        }
    }

    #[test]
    fn charge_spreads_nodes_and_centre_holds_the_mean() {
        let mut s = Simulation::new(30);
        let r0 = s
            .nodes
            .iter()
            .map(|n| n.position.distance())
            .fold(0.0f32, f32::max);
        s.run(500);
        let r1 = s
            .nodes
            .iter()
            .map(|n| n.position.distance())
            .fold(0.0f32, f32::max);
        assert!(r1 > r0, "repelled outward: {r0} → {r1}");
        let mean = s
            .nodes
            .iter()
            .fold(Offset::ZERO, |a, n| a + n.position)
            .scale(1.0 / 30.0);
        assert!(mean.distance() < 1e-3);
    }

    #[test]
    fn barnes_hut_visits_fewer_cells_than_pairs() {
        let mut s = Simulation::new(400);
        s.tick();
        assert!(s.last_visits < 400 * 399 / 4, "{} visits", s.last_visits);
    }

    #[test]
    fn collision_separates_and_fixed_nodes_stay() {
        let mut s = Simulation::new(20);
        s.charge = 0.0;
        s.collide = 1.0;
        for n in &mut s.nodes {
            n.radius = 12.0;
        }
        s.nodes[0].fixed = Some(Offset::new(0.0, 0.0));
        s.center = None;
        s.run(300);
        assert_eq!(s.nodes[0].position, Offset::ZERO);
        for i in 0..20 {
            for j in i + 1..20 {
                let d = (s.nodes[i].position - s.nodes[j].position).distance();
                assert!(d > 20.0, "{i},{j}: {d}");
            }
        }
    }

    #[test]
    fn deterministic() {
        let run = || {
            let mut s = Simulation::new(50);
            for i in 1..50 {
                s.link(i / 3, i, 30.0);
            }
            s.run(300);
            s.nodes.iter().map(|n| n.position).collect::<Vec<_>>()
        };
        assert_eq!(run(), run());
    }
}
