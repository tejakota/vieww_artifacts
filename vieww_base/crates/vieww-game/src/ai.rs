//! Game AI and level structure — the pieces every engine in the comparison
//! ships beside its scene graph (Unity's NavMesh and Tilemap, Unreal's
//! Behavior Trees, Godot's TileMap and NavigationServer, Pygame/Arcade's
//! tile maps).
//!
//! * [`TileMap`] — a layered grid of tile ids with solidity, Wang-blob
//!   **autotiling** (the 4-bit neighbour mask every editor computes),
//!   point/rect collision queries and a swept-AABB `move_and_collide`.
//! * [`astar`] — A* on a grid (4- or 8-connected, no corner cutting) with
//!   an octile heuristic.
//! * [`NavMesh`] — convex polygons (triangles) with shared-edge adjacency,
//!   A* over polygon centroids and the **simple stupid funnel algorithm**
//!   (Mononen) to pull the corridor taut into the shortest path.
//! * [`Bt`] — behaviour trees: sequence, selector, parallel, inverter,
//!   repeater, cooldown and leaf conditions/actions over a blackboard, with
//!   `Running` carried across ticks.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

/// A layered tile grid.
#[derive(Debug, Clone, PartialEq)]
pub struct TileMap {
    pub width: usize,
    pub height: usize,
    pub tile_size: f32,
    /// Layers of tile ids (0 = empty).
    pub layers: Vec<Vec<u16>>,
    /// Which ids are solid.
    pub solid: Vec<bool>,
}

impl TileMap {
    #[must_use]
    pub fn new(width: usize, height: usize, tile_size: f32) -> Self {
        Self {
            width,
            height,
            tile_size,
            layers: vec![vec![0; width * height]],
            solid: vec![false, true],
        }
    }

    /// Parse ASCII art: `#` → tile 1 (solid), anything else empty.
    #[must_use]
    pub fn from_ascii(rows: &[&str], tile_size: f32) -> Self {
        let h = rows.len();
        let w = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        let mut m = Self::new(w, h, tile_size);
        for (y, r) in rows.iter().enumerate() {
            for (x, c) in r.bytes().enumerate() {
                if c == b'#' {
                    m.layers[0][y * w + x] = 1;
                }
            }
        }
        m
    }

    #[must_use]
    pub fn get(&self, layer: usize, x: i64, y: i64) -> u16 {
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
            return 0;
        }
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        self.layers.get(layer).map_or(0, |l| l[y as usize * self.width + x as usize])
    }

    pub fn set(&mut self, layer: usize, x: usize, y: usize, id: u16) {
        while self.layers.len() <= layer {
            self.layers.push(vec![0; self.width * self.height]);
        }
        if x < self.width && y < self.height {
            self.layers[layer][y * self.width + x] = id;
        }
    }

    /// Solid on any layer; outside the map counts as solid.
    #[must_use]
    pub fn is_solid(&self, x: i64, y: i64) -> bool {
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
            return true;
        }
        (0..self.layers.len()).any(|l| {
            let id = usize::from(self.get(l, x, y));
            id != 0 && self.solid.get(id).copied().unwrap_or(false)
        })
    }

    /// The 4-bit Wang mask of same-id neighbours (N=1, E=2, S=4, W=8) —
    /// index into a 16-tile autotile sheet.
    #[must_use]
    pub fn autotile(&self, layer: usize, x: i64, y: i64) -> u8 {
        let id = self.get(layer, x, y);
        if id == 0 {
            return 0;
        }
        let same = |dx: i64, dy: i64| u8::from(self.get(layer, x + dx, y + dy) == id);
        same(0, -1) | same(1, 0) << 1 | same(0, 1) << 2 | same(-1, 0) << 3
    }

    /// Does the world-space box overlap a solid tile?
    #[must_use]
    pub fn box_hits(&self, x: f32, y: f32, w: f32, h: f32) -> bool {
        let t = self.tile_size;
        #[allow(clippy::cast_possible_truncation)]
        let (x0, y0, x1, y1) = (
            (x / t).floor() as i64,
            (y / t).floor() as i64,
            ((x + w) / t - 1e-4).floor() as i64,
            ((y + h) / t - 1e-4).floor() as i64,
        );
        (y0..=y1).any(|ty| (x0..=x1).any(|tx| self.is_solid(tx, ty)))
    }

    /// Move a box by `(dx, dy)` one axis at a time, stopping flush against
    /// solid tiles. Returns the new position and which axes were blocked.
    #[must_use]
    pub fn move_and_collide(&self, pos: (f32, f32), size: (f32, f32), d: (f32, f32)) -> ((f32, f32), (bool, bool)) {
        let t = self.tile_size;
        let mut p = pos;
        let mut hit = (false, false);
        // X then Y, in sub-steps no larger than a tile so nothing tunnels.
        for axis in 0..2 {
            let total = if axis == 0 { d.0 } else { d.1 };
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let n = (total.abs() / (t * 0.5)).ceil().max(1.0) as usize;
            #[allow(clippy::cast_precision_loss)]
            let step = total / n as f32;
            for _ in 0..n {
                let q = if axis == 0 { (p.0 + step, p.1) } else { (p.0, p.1 + step) };
                if self.box_hits(q.0, q.1, size.0, size.1) {
                    // Snap flush to the tile edge.
                    if axis == 0 {
                        p.0 = if step > 0.0 { ((p.0 + size.0) / t).ceil() * t - size.0 } else { (p.0 / t).floor() * t };
                        hit.0 = true;
                    } else {
                        p.1 = if step > 0.0 { ((p.1 + size.1) / t).ceil() * t - size.1 } else { (p.1 / t).floor() * t };
                        hit.1 = true;
                    }
                    break;
                }
                p = q;
            }
        }
        (p, hit)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Open(f32, usize);
impl Eq for Open {}
impl Ord for Open {
    fn cmp(&self, o: &Self) -> Ordering {
        o.0.total_cmp(&self.0).then_with(|| o.1.cmp(&self.1))
    }
}
impl PartialOrd for Open {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

/// A* on `map`'s solidity from `start` to `goal` (tile coordinates).
#[must_use]
pub fn astar(map: &TileMap, start: (usize, usize), goal: (usize, usize), diagonal: bool) -> Option<Vec<(usize, usize)>> {
    let w = map.width;
    let idx = |p: (usize, usize)| p.1 * w + p.0;
    #[allow(clippy::cast_possible_wrap)]
    let solid = |x: usize, y: usize| map.is_solid(x as i64, y as i64);
    if solid(start.0, start.1) || solid(goal.0, goal.1) {
        return None;
    }
    let h = |p: (usize, usize)| {
        #[allow(clippy::cast_precision_loss)]
        let (dx, dy) = ((p.0 as f32 - goal.0 as f32).abs(), (p.1 as f32 - goal.1 as f32).abs());
        if diagonal {
            dx.max(dy) + (std::f32::consts::SQRT_2 - 1.0) * dx.min(dy)
        } else {
            dx + dy
        }
    };
    let mut g = vec![f32::INFINITY; w * map.height];
    let mut came = vec![usize::MAX; w * map.height];
    let mut open = BinaryHeap::new();
    g[idx(start)] = 0.0;
    open.push(Open(h(start), idx(start)));
    while let Some(Open(_, cur)) = open.pop() {
        let (cx, cy) = (cur % w, cur / w);
        if (cx, cy) == goal {
            let mut path = vec![goal];
            let mut c = cur;
            while came[c] != usize::MAX {
                c = came[c];
                path.push((c % w, c / w));
            }
            path.reverse();
            return Some(path);
        }
        for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
            if !diagonal && dx != 0 && dy != 0 {
                continue;
            }
            #[allow(clippy::cast_possible_wrap)]
            let (nx, ny) = (cx as i64 + dx, cy as i64 + dy);
            if map.is_solid(nx, ny) {
                continue;
            }
            #[allow(clippy::cast_possible_wrap)]
            if dx != 0 && dy != 0 && (map.is_solid(cx as i64 + dx, cy as i64) || map.is_solid(cx as i64, cy as i64 + dy)) {
                continue; // no corner cutting
            }
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            let n = (nx as usize, ny as usize);
            let cost = if dx != 0 && dy != 0 { std::f32::consts::SQRT_2 } else { 1.0 };
            let ng = g[cur] + cost;
            if ng < g[idx(n)] {
                g[idx(n)] = ng;
                came[idx(n)] = cur;
                open.push(Open(ng + h(n), idx(n)));
            }
        }
    }
    None
}

/// A navigation mesh of triangles.
#[derive(Debug, Clone, PartialEq)]
pub struct NavMesh {
    pub vertices: Vec<(f32, f32)>,
    pub triangles: Vec<[usize; 3]>,
    /// For each triangle edge `k` (from vertex k to k+1), the neighbour across it.
    neighbours: Vec<[Option<usize>; 3]>,
}

fn cross(o: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
}

impl NavMesh {
    /// Build from triangles (counter-clockwise).
    #[must_use]
    pub fn new(vertices: Vec<(f32, f32)>, triangles: Vec<[usize; 3]>) -> Self {
        let mut edge: HashMap<(usize, usize), (usize, usize)> = HashMap::new();
        let mut neighbours = vec![[None; 3]; triangles.len()];
        for (ti, t) in triangles.iter().enumerate() {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                if let Some(&(oj, ok)) = edge.get(&(b, a)) {
                    neighbours[ti][k] = Some(oj);
                    neighbours[oj][ok] = Some(ti);
                } else {
                    edge.insert((a, b), (ti, k));
                }
            }
        }
        Self {
            vertices,
            triangles,
            neighbours,
        }
    }

    /// The triangle containing `p`.
    #[must_use]
    pub fn locate(&self, p: (f32, f32)) -> Option<usize> {
        self.triangles.iter().position(|t| {
            let [a, b, c] = t.map(|i| self.vertices[i]);
            cross(a, b, p) >= -1e-5 && cross(b, c, p) >= -1e-5 && cross(c, a, p) >= -1e-5
        })
    }

    fn centroid(&self, t: usize) -> (f32, f32) {
        let [a, b, c] = self.triangles[t].map(|i| self.vertices[i]);
        ((a.0 + b.0 + c.0) / 3.0, (a.1 + b.1 + c.1) / 3.0)
    }

    /// Shortest path from `start` to `goal` inside the mesh.
    #[must_use]
    pub fn find_path(&self, start: (f32, f32), goal: (f32, f32)) -> Option<Vec<(f32, f32)>> {
        let (s, g) = (self.locate(start)?, self.locate(goal)?);
        // A* over triangles.
        let n = self.triangles.len();
        let mut dist = vec![f32::INFINITY; n];
        let mut came = vec![usize::MAX; n];
        let mut open = BinaryHeap::new();
        dist[s] = 0.0;
        open.push(Open(0.0, s));
        let d = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).hypot(a.1 - b.1);
        while let Some(Open(_, cur)) = open.pop() {
            if cur == g {
                break;
            }
            for nb in self.neighbours[cur].iter().flatten() {
                let nd = dist[cur] + d(self.centroid(cur), self.centroid(*nb));
                if nd < dist[*nb] {
                    dist[*nb] = nd;
                    came[*nb] = cur;
                    open.push(Open(nd + d(self.centroid(*nb), goal), *nb));
                }
            }
        }
        if s != g && came[g] == usize::MAX {
            return None;
        }
        let mut corridor = vec![g];
        let mut c = g;
        while c != s {
            c = came[c];
            corridor.push(c);
        }
        corridor.reverse();
        // Portals: the shared edge between consecutive triangles, as (left, right)
        // seen walking forward.
        let mut portals = vec![(start, start)];
        for w in corridor.windows(2) {
            let t = self.triangles[w[0]];
            let k = (0..3).find(|&k| self.neighbours[w[0]][k] == Some(w[1])).expect("adjacent");
            let (a, b) = (self.vertices[t[k]], self.vertices[t[(k + 1) % 3]]);
            // Triangles are CCW, so walking out through edge a→b, `b` is on the left.
            portals.push((b, a));
        }
        portals.push((goal, goal));
        Some(funnel(&portals))
    }
}

/// Mononen's simple stupid funnel algorithm over `(left, right)` portals.
fn funnel(portals: &[((f32, f32), (f32, f32))]) -> Vec<(f32, f32)> {
    let mut path = vec![portals[0].0];
    let (mut apex, mut left, mut right) = (portals[0].0, portals[0].0, portals[0].1);
    let (mut li, mut ri) = (0usize, 0usize);
    let mut i = 1;
    while i < portals.len() {
        let (pl, pr) = portals[i];
        // Right side.
        if cross(apex, right, pr) >= 0.0 {
            if apex == right || cross(apex, left, pr) < 0.0 {
                right = pr;
                ri = i;
            } else {
                if path.last() != Some(&left) {
                    path.push(left);
                }
                apex = left;
                let ai = li;
                left = apex;
                right = apex;
                ri = ai;
                i = ai + 1;
                continue;
            }
        }
        // Left side.
        if cross(apex, left, pl) <= 0.0 {
            if apex == left || cross(apex, right, pl) > 0.0 {
                left = pl;
                li = i;
            } else {
                if path.last() != Some(&right) {
                    path.push(right);
                }
                apex = right;
                let ai = ri;
                left = apex;
                right = apex;
                li = ai;
                i = ai + 1;
                continue;
            }
        }
        i += 1;
    }
    let goal = portals[portals.len() - 1].0;
    if path.last() != Some(&goal) {
        path.push(goal);
    }
    path
}

/// A behaviour tree's verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Success,
    Failure,
    Running,
}

/// The blackboard behaviour trees read and write.
pub type Blackboard = HashMap<String, f32>;

/// A leaf function.
pub type Leaf = fn(&mut Blackboard, f32) -> Status;

/// A behaviour tree node.
#[derive(Debug, Clone)]
pub enum Bt {
    /// Children in order until one fails; resumes at a `Running` child.
    Sequence(Vec<Bt>, usize),
    /// Children in order until one succeeds; resumes at a `Running` child.
    Selector(Vec<Bt>, usize),
    /// All children every tick: succeed when `need` succeed, fail when that
    /// becomes impossible.
    Parallel(Vec<Bt>, usize),
    Invert(Box<Bt>),
    /// Run the child `n` times (succeeding each time).
    Repeat(Box<Bt>, u32, u32),
    /// After the child succeeds, fail for `seconds`.
    Cooldown(Box<Bt>, f32, f32),
    Leaf(&'static str, Leaf),
}

impl Bt {
    #[must_use]
    pub fn sequence(children: Vec<Self>) -> Self {
        Self::Sequence(children, 0)
    }
    #[must_use]
    pub fn selector(children: Vec<Self>) -> Self {
        Self::Selector(children, 0)
    }
    #[must_use]
    pub fn leaf(name: &'static str, f: Leaf) -> Self {
        Self::Leaf(name, f)
    }

    /// Advance by `dt` seconds.
    pub fn tick(&mut self, bb: &mut Blackboard, dt: f32) -> Status {
        match self {
            Self::Leaf(_, f) => f(bb, dt),
            Self::Sequence(cs, at) => {
                while *at < cs.len() {
                    match cs[*at].tick(bb, dt) {
                        Status::Success => *at += 1,
                        Status::Running => return Status::Running,
                        Status::Failure => {
                            *at = 0;
                            return Status::Failure;
                        }
                    }
                }
                *at = 0;
                Status::Success
            }
            Self::Selector(cs, at) => {
                while *at < cs.len() {
                    match cs[*at].tick(bb, dt) {
                        Status::Failure => *at += 1,
                        Status::Running => return Status::Running,
                        Status::Success => {
                            *at = 0;
                            return Status::Success;
                        }
                    }
                }
                *at = 0;
                Status::Failure
            }
            Self::Parallel(cs, need) => {
                let results: Vec<Status> = cs.iter_mut().map(|c| c.tick(bb, dt)).collect();
                let ok = results.iter().filter(|s| **s == Status::Success).count();
                let fail = results.iter().filter(|s| **s == Status::Failure).count();
                if ok >= *need {
                    Status::Success
                } else if cs.len() - fail < *need {
                    Status::Failure
                } else {
                    Status::Running
                }
            }
            Self::Invert(c) => match c.tick(bb, dt) {
                Status::Success => Status::Failure,
                Status::Failure => Status::Success,
                Status::Running => Status::Running,
            },
            Self::Repeat(c, n, done) => {
                match c.tick(bb, dt) {
                    Status::Success => *done += 1,
                    Status::Failure => {
                        *done = 0;
                        return Status::Failure;
                    }
                    Status::Running => return Status::Running,
                }
                if *done >= *n {
                    *done = 0;
                    Status::Success
                } else {
                    Status::Running
                }
            }
            Self::Cooldown(c, secs, left) => {
                if *left > 0.0 {
                    *left = (*left - dt).max(0.0);
                    return Status::Failure;
                }
                let s = c.tick(bb, dt);
                if s == Status::Success {
                    *left = *secs;
                }
                s
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level() -> TileMap {
        TileMap::from_ascii(
            &[
                "##########",
                "#........#",
                "#.######.#",
                "#.#....#.#",
                "#.#.##.#.#",
                "#...#....#",
                "##########",
            ],
            16.0,
        )
    }

    #[test]
    fn astar_finds_the_shortest_route() {
        let m = level();
        let p = astar(&m, (1, 1), (3, 3), false).unwrap();
        assert_eq!(p.first(), Some(&(1, 1)));
        assert_eq!(p.last(), Some(&(3, 3)));
        assert_eq!(p.len(), 9, "{p:?}");
        for w in p.windows(2) {
            let d = w[0].0.abs_diff(w[1].0) + w[0].1.abs_diff(w[1].1);
            assert_eq!(d, 1);
            assert!(!m.is_solid(w[1].0 as i64, w[1].1 as i64));
        }
        assert!(astar(&m, (1, 1), (0, 0), true).is_none(), "goal in a wall");
        let diag = astar(&m, (8, 1), (5, 5), true).unwrap();
        assert!(diag.len() < 9);
    }

    #[test]
    fn autotile_masks_and_collision() {
        let m = level();
        assert_eq!(m.autotile(0, 0, 0), 0b0110, "top-left corner: E + S");
        assert_eq!(m.autotile(0, 3, 2), 0b1010, "a horizontal run: E + W");
        assert!(m.box_hits(0.0, 0.0, 4.0, 4.0));
        assert!(!m.box_hits(17.0, 17.0, 10.0, 10.0));
        // Walk right into the wall at x = 9 tiles.
        let ((x, y), hit) = m.move_and_collide((20.0, 20.0), (10.0, 10.0), (400.0, 0.0));
        assert!(hit.0 && !hit.1);
        assert!((x + 10.0 - 144.0).abs() < 1e-3, "flush against the wall: {x}");
        assert_eq!(y, 20.0);
    }

    #[test]
    fn navmesh_path_is_pulled_taut_around_a_corner() {
        // An L-shaped corridor: (0,0)-(10,0)-(10,10)-(8,10)-(8,2)-(0,2).
        let v = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (8.0, 10.0), (8.0, 2.0), (0.0, 2.0)];
        let t = vec![[0, 4, 5], [0, 1, 4], [1, 2, 4], [2, 3, 4]];
        let nav = NavMesh::new(v, t);
        let p = nav.find_path((1.0, 1.0), (9.0, 9.0)).unwrap();
        assert_eq!(p.first(), Some(&(1.0, 1.0)));
        assert_eq!(p.last(), Some(&(9.0, 9.0)));
        assert_eq!(p.len(), 3, "one turn, at the inner corner: {p:?}");
        assert_eq!(p[1], (8.0, 2.0));
        let straight = nav.find_path((1.0, 1.0), (7.0, 1.0)).unwrap();
        assert_eq!(straight.len(), 2);
        assert!(nav.find_path((1.0, 1.0), (5.0, 8.0)).is_none(), "off the mesh");
    }

    #[test]
    fn behaviour_trees_run_across_ticks() {
        fn far(bb: &mut Blackboard, _: f32) -> Status {
            if bb["dist"] > 0.0 { Status::Success } else { Status::Failure }
        }
        fn walk(bb: &mut Blackboard, dt: f32) -> Status {
            let d = bb.get_mut("dist").expect("dist");
            *d = (*d - 5.0 * dt).max(0.0);
            if *d > 0.0 { Status::Running } else { Status::Success }
        }
        fn attack(bb: &mut Blackboard, _: f32) -> Status {
            *bb.entry("hits".into()).or_insert(0.0) += 1.0;
            Status::Success
        }
        let mut tree = Bt::selector(vec![
            Bt::sequence(vec![Bt::leaf("far", far), Bt::leaf("walk", walk)]),
            Bt::Cooldown(Box::new(Bt::leaf("attack", attack)), 1.0, 0.0),
        ]);
        let mut bb: Blackboard = HashMap::from([("dist".into(), 2.0)]);
        let mut statuses = Vec::new();
        for _ in 0..10 {
            statuses.push(tree.tick(&mut bb, 0.25));
        }
        assert_eq!(statuses[0], Status::Running);
        assert_eq!(bb["dist"], 0.0);
        // Attack lands, then the cooldown blocks it for a second (4 ticks).
        assert_eq!(bb["hits"], 2.0, "{statuses:?}");
        let mut inv = Bt::Invert(Box::new(Bt::leaf("far", far)));
        assert_eq!(inv.tick(&mut bb, 0.1), Status::Success);
        let mut rep = Bt::Repeat(Box::new(Bt::leaf("attack", attack)), 3, 0);
        assert_eq!(rep.tick(&mut bb, 0.1), Status::Running);
        rep.tick(&mut bb, 0.1);
        assert_eq!(rep.tick(&mut bb, 0.1), Status::Success);
        let mut par = Bt::Parallel(vec![Bt::leaf("attack", attack), Bt::leaf("far", far)], 2);
        assert_eq!(par.tick(&mut bb, 0.1), Status::Failure);
    }
}
