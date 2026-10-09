//! Flow diagrams — `d3-sankey` and `d3-chord`.
//!
//! * [`sankey`] lays nodes in columns by their longest path from a source,
//!   sizes each node by `max(in, out)`, stacks columns to fill the height
//!   with padding, then relaxes node positions toward the weighted centre
//!   of their neighbours (the iterative refinement d3-sankey does),
//!   resolving collisions after each pass; links get ribbon paths.
//! * [`chord`] turns a square flow matrix into angular groups and ribbons:
//!   each group's arc is proportional to its row sum; each ribbon joins the
//!   sub-arc of `i → j` to the sub-arc of `j → i`.

use vieww_foundation::{Offset, Path};

/// A positioned Sankey node.
#[derive(Debug, Clone, PartialEq)]
pub struct SankeyNode {
    pub column: usize,
    pub x0: f32,
    pub x1: f32,
    pub y0: f32,
    pub y1: f32,
    pub value: f32,
}

/// A positioned Sankey link.
#[derive(Debug, Clone, PartialEq)]
pub struct SankeyLink {
    pub source: usize,
    pub target: usize,
    pub value: f32,
    /// Ribbon width.
    pub width: f32,
    /// Ribbon centre where it leaves the source / enters the target.
    pub y0: f32,
    pub y1: f32,
}

impl SankeyLink {
    /// The ribbon as a closed path (cubic, horizontal tangents).
    #[must_use]
    pub fn path(&self, nodes: &[SankeyNode]) -> Path {
        let (x0, x1) = (nodes[self.source].x1, nodes[self.target].x0);
        let xm = (x0 + x1) * 0.5;
        let h = self.width * 0.5;
        let mut p = Path::new();
        p.move_to(Offset::new(x0, self.y0 - h));
        p.cubic_to(
            Offset::new(xm, self.y0 - h),
            Offset::new(xm, self.y1 - h),
            Offset::new(x1, self.y1 - h),
        );
        p.line_to(Offset::new(x1, self.y1 + h));
        p.cubic_to(
            Offset::new(xm, self.y1 + h),
            Offset::new(xm, self.y0 + h),
            Offset::new(x0, self.y0 + h),
        );
        p.close();
        p
    }
}

/// Lay out a Sankey diagram of `n` nodes and `(source, target, value)` links
/// in `width × height`, with node width and vertical padding.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn sankey(
    n: usize,
    links: &[(usize, usize, f32)],
    width: f32,
    height: f32,
    node_width: f32,
    padding: f32,
) -> (Vec<SankeyNode>, Vec<SankeyLink>) {
    // Columns: longest path from any source (Kahn's order).
    let mut indeg = vec![0usize; n];
    for &(_, t, _) in links {
        indeg[t] += 1;
    }
    let mut col = vec![0usize; n];
    let mut queue: Vec<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    let mut deg = indeg.clone();
    while let Some(u) = queue.pop() {
        for &(s, t, _) in links {
            if s == u {
                col[t] = col[t].max(col[u] + 1);
                deg[t] -= 1;
                if deg[t] == 0 {
                    queue.push(t);
                }
            }
        }
    }
    // Sinks align right (d3's sankeyJustify).
    let max_col = col.iter().copied().max().unwrap_or(0);
    for (i, c) in col.iter_mut().enumerate() {
        if !links.iter().any(|&(s, _, _)| s == i) {
            *c = max_col;
        }
    }
    let value: Vec<f32> = (0..n)
        .map(|i| {
            let inn: f32 = links.iter().filter(|l| l.1 == i).map(|l| l.2).sum();
            let out: f32 = links.iter().filter(|l| l.0 == i).map(|l| l.2).sum();
            inn.max(out)
        })
        .collect();
    let columns: Vec<Vec<usize>> = (0..=max_col)
        .map(|c| (0..n).filter(|&i| col[i] == c).collect())
        .collect();
    // Vertical scale: the tightest column decides.
    let ky = columns
        .iter()
        .filter(|c| !c.is_empty())
        .map(|c| {
            #[allow(clippy::cast_precision_loss)]
            let pad = padding * (c.len() as f32 - 1.0);
            (height - pad).max(1.0) / c.iter().map(|&i| value[i]).sum::<f32>().max(1e-9)
        })
        .fold(f32::INFINITY, f32::min);
    #[allow(clippy::cast_precision_loss)]
    let dx = if max_col == 0 {
        0.0
    } else {
        (width - node_width) / max_col as f32
    };
    let mut nodes: Vec<SankeyNode> = (0..n)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let x0 = col[i] as f32 * dx;
            SankeyNode {
                column: col[i],
                x0,
                x1: x0 + node_width,
                y0: 0.0,
                y1: value[i] * ky,
                value: value[i],
            }
        })
        .collect();
    let resolve = |nodes: &mut Vec<SankeyNode>| {
        for c in &columns {
            let mut idx = c.clone();
            idx.sort_by(|&a, &b| nodes[a].y0.total_cmp(&nodes[b].y0));
            let mut y = 0.0f32;
            for &i in &idx {
                let h = nodes[i].y1 - nodes[i].y0;
                if nodes[i].y0 < y {
                    nodes[i].y0 = y;
                    nodes[i].y1 = y + h;
                }
                y = nodes[i].y1 + padding;
            }
            // Push back up from the bottom.
            let mut y = height;
            for &i in idx.iter().rev() {
                let h = nodes[i].y1 - nodes[i].y0;
                if nodes[i].y1 > y {
                    nodes[i].y1 = y;
                    nodes[i].y0 = y - h;
                }
                y = nodes[i].y0 - padding;
            }
        }
    };
    // Initial stacking.
    for c in &columns {
        let mut y = 0.0;
        for &i in c {
            let h = nodes[i].y1 - nodes[i].y0;
            nodes[i].y0 = y;
            nodes[i].y1 = y + h;
            y += h + padding;
        }
    }
    // Relaxation: move towards the value-weighted centre of neighbours.
    for pass in 0..6 {
        #[allow(clippy::cast_precision_loss)]
        let alpha = 0.99f32.powi(pass) * 0.6;
        for i in 0..n {
            let (mut wsum, mut ysum) = (0.0, 0.0);
            for &(s, t, v) in links {
                let other = if s == i {
                    t
                } else if t == i {
                    s
                } else {
                    continue;
                };
                ysum += v * (nodes[other].y0 + nodes[other].y1) * 0.5;
                wsum += v;
            }
            if wsum > 0.0 {
                let target = ysum / wsum;
                let cur = (nodes[i].y0 + nodes[i].y1) * 0.5;
                let d = (target - cur) * alpha;
                nodes[i].y0 += d;
                nodes[i].y1 += d;
            }
        }
        resolve(&mut nodes);
    }
    // Link offsets: stack within each node, ordered by the other end's y.
    let mut out_off = vec![0.0f32; n];
    let mut in_off = vec![0.0f32; n];
    let mut order: Vec<usize> = (0..links.len()).collect();
    order.sort_by(|&a, &b| {
        let ya = nodes[links[a].1].y0;
        let yb = nodes[links[b].1].y0;
        ya.total_cmp(&yb)
    });
    let mut result = vec![None; links.len()];
    for &k in &order {
        let (s, _, v) = links[k];
        let w = v * ky;
        let y0 = nodes[s].y0 + out_off[s] + w * 0.5;
        out_off[s] += w;
        result[k] = Some((w, y0));
    }
    order.sort_by(|&a, &b| nodes[links[a].0].y0.total_cmp(&nodes[links[b].0].y0));
    let mut ends = vec![0.0f32; links.len()];
    for &k in &order {
        let (_, t, v) = links[k];
        let w = v * ky;
        ends[k] = nodes[t].y0 + in_off[t] + w * 0.5;
        in_off[t] += w;
    }
    let out_links = links
        .iter()
        .enumerate()
        .map(|(k, &(s, t, v))| {
            let (w, y0) = result[k].expect("every link placed");
            SankeyLink {
                source: s,
                target: t,
                value: v,
                width: w,
                y0,
                y1: ends[k],
            }
        })
        .collect();
    (nodes, out_links)
}

/// One chord group (a row of the matrix).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChordGroup {
    pub index: usize,
    pub start: f32,
    pub end: f32,
    pub value: f32,
}

/// One ribbon: `source → target` sub-arcs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Chord {
    pub source: (usize, f32, f32),
    pub target: (usize, f32, f32),
}

/// `d3.chord()` with padding between groups (radians).
#[must_use]
pub fn chord(matrix: &[Vec<f32>], pad: f32) -> (Vec<ChordGroup>, Vec<Chord>) {
    let n = matrix.len();
    let sums: Vec<f32> = matrix.iter().map(|r| r.iter().sum()).collect();
    let total: f32 = sums.iter().sum();
    #[allow(clippy::cast_precision_loss)]
    let k = (std::f32::consts::TAU - pad * n as f32).max(0.0) / total.max(1e-9);
    let mut groups = Vec::with_capacity(n);
    let mut sub = vec![vec![(0.0f32, 0.0f32); n]; n];
    let mut a = 0.0;
    for i in 0..n {
        let start = a;
        for j in 0..n {
            let s = a;
            a += matrix[i][j] * k;
            sub[i][j] = (s, a);
        }
        groups.push(ChordGroup {
            index: i,
            start,
            end: a,
            value: sums[i],
        });
        a += pad;
    }
    let mut chords = Vec::new();
    for i in 0..n {
        for j in i..n {
            if matrix[i][j] > 0.0 || matrix[j][i] > 0.0 {
                chords.push(Chord {
                    source: (i, sub[i][j].0, sub[i][j].1),
                    target: (j, sub[j][i].0, sub[j][i].1),
                });
            }
        }
    }
    (groups, chords)
}

/// A ribbon path at `radius` around `center` (quadratic through the centre).
#[must_use]
pub fn ribbon(c: &Chord, center: Offset, radius: f32) -> Path {
    let pt = |a: f32| {
        Offset::new(
            center.dx + radius * (a - std::f32::consts::FRAC_PI_2).cos(),
            center.dy + radius * (a - std::f32::consts::FRAC_PI_2).sin(),
        )
    };
    let mut p = Path::new();
    p.move_to(pt(c.source.1));
    p.extend(&arc_seg(center, radius, c.source.1, c.source.2));
    p.cubic_to(center, center, pt(c.target.1));
    p.extend(&arc_seg(center, radius, c.target.1, c.target.2));
    p.cubic_to(center, center, pt(c.source.1));
    p.close();
    p
}

fn arc_seg(center: Offset, r: f32, a0: f32, a1: f32) -> Path {
    let mut p = Path::new();
    let steps = 12;
    for i in 1..=steps {
        #[allow(clippy::cast_precision_loss)]
        let a = a0 + (a1 - a0) * i as f32 / steps as f32 - std::f32::consts::FRAC_PI_2;
        let q = Offset::new(center.dx + r * a.cos(), center.dy + r * a.sin());
        if i == 1 {
            p.move_to(q);
        } else {
            p.line_to(q);
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sankey_columns_values_and_conservation() {
        // 0,1 → 2 → 3,4
        let links = [(0, 2, 5.0), (1, 2, 3.0), (2, 3, 6.0), (2, 4, 2.0)];
        let (nodes, ls) = sankey(5, &links, 400.0, 200.0, 10.0, 8.0);
        assert_eq!(
            nodes.iter().map(|n| n.column).collect::<Vec<_>>(),
            vec![0, 0, 1, 2, 2]
        );
        assert_eq!(nodes[2].value, 8.0);
        for n in &nodes {
            assert!(n.y0 >= -1e-3 && n.y1 <= 200.0 + 1e-3, "{n:?}");
        }
        // Link widths sum to the node's height on both sides.
        let h2 = nodes[2].y1 - nodes[2].y0;
        let into: f32 = ls.iter().filter(|l| l.target == 2).map(|l| l.width).sum();
        let out: f32 = ls.iter().filter(|l| l.source == 2).map(|l| l.width).sum();
        assert!((into - h2).abs() < 1e-3 && (out - h2).abs() < 1e-3);
        // Nodes in a column do not overlap.
        assert!(nodes[0].y1 + 8.0 <= nodes[1].y0 + 1e-3 || nodes[1].y1 + 8.0 <= nodes[0].y0 + 1e-3);
        assert!(!ls[0].path(&nodes).is_empty());
    }

    #[test]
    fn chord_groups_are_proportional() {
        let m = vec![
            vec![0.0, 10.0, 5.0],
            vec![5.0, 0.0, 5.0],
            vec![10.0, 0.0, 0.0],
        ];
        let (g, c) = chord(&m, 0.05);
        let span = |x: &ChordGroup| x.end - x.start;
        assert!((span(&g[0]) / span(&g[1]) - 1.5).abs() < 1e-4);
        assert!(
            (g[2].end + 0.05 - std::f32::consts::TAU).abs() < 1e-4,
            "groups fill the circle"
        );
        assert_eq!(c.len(), 3);
        assert!(!ribbon(&c[0], Offset::new(0.0, 0.0), 100.0).is_empty());
    }
}
