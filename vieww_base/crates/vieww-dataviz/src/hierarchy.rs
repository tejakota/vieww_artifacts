//! Hierarchy layouts — `d3-hierarchy`: stratify a table into a tree, sum
//! values, and lay it out as a **treemap** (squarified), a **tidy tree**
//! (Reingold–Tilford in Buchheim's linear-time form, exactly as `d3.tree`)
//! or a **partition** (icicle / sunburst).

use vieww_foundation::{Offset, Rect};

/// One node.
#[derive(Debug, Clone, PartialEq)]
pub struct HNode {
    pub id: String,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    /// Own value; after [`Hierarchy::sum`], the subtree total.
    pub value: f64,
    pub depth: usize,
    /// Longest path to a leaf.
    pub height: usize,
}

/// A rooted tree; node 0 is the root.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Hierarchy {
    pub nodes: Vec<HNode>,
}

impl Hierarchy {
    /// Build from `(id, parent id, value)` rows — `d3.stratify()`. Exactly
    /// one row must have no parent.
    ///
    /// # Errors
    ///
    /// No root, several roots, a missing parent, or a cycle.
    pub fn stratify(rows: &[(&str, Option<&str>, f64)]) -> Result<Self, String> {
        let roots: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.1.is_none())
            .map(|(i, _)| i)
            .collect();
        if roots.len() != 1 {
            return Err(format!("expected one root, found {}", roots.len()));
        }
        // Root first, then rows in order.
        let mut order = vec![roots[0]];
        order.extend((0..rows.len()).filter(|i| *i != roots[0]));
        let index: std::collections::HashMap<&str, usize> = order
            .iter()
            .enumerate()
            .map(|(n, &i)| (rows[i].0, n))
            .collect();
        let mut nodes: Vec<HNode> = order
            .iter()
            .map(|&i| HNode {
                id: rows[i].0.to_owned(),
                parent: None,
                children: Vec::new(),
                value: rows[i].2,
                depth: 0,
                height: 0,
            })
            .collect();
        for (n, &i) in order.iter().enumerate() {
            if let Some(p) = rows[i].1 {
                let pi = *index.get(p).ok_or_else(|| format!("missing parent {p}"))?;
                nodes[n].parent = Some(pi);
                nodes[pi].children.push(n);
            }
        }
        let mut h = Self { nodes };
        // Depths by BFS; a node never reached is in a cycle.
        let mut seen = vec![false; h.nodes.len()];
        let mut queue = std::collections::VecDeque::from([0usize]);
        seen[0] = true;
        while let Some(i) = queue.pop_front() {
            for c in h.nodes[i].children.clone() {
                if seen[c] {
                    return Err("cycle".into());
                }
                seen[c] = true;
                h.nodes[c].depth = h.nodes[i].depth + 1;
                queue.push_back(c);
            }
        }
        if seen.iter().any(|s| !s) {
            return Err("a cycle detached some nodes from the root".into());
        }
        for i in h.post_order() {
            h.nodes[i].height = h.nodes[i]
                .children
                .iter()
                .map(|&c| h.nodes[c].height + 1)
                .max()
                .unwrap_or(0);
        }
        Ok(h)
    }

    /// Children before parents.
    #[must_use]
    pub fn post_order(&self) -> Vec<usize> {
        let mut out = Vec::with_capacity(self.nodes.len());
        let mut stack = vec![(0usize, false)];
        while let Some((i, done)) = stack.pop() {
            if done {
                out.push(i);
            } else {
                stack.push((i, true));
                for &c in self.nodes[i].children.iter().rev() {
                    stack.push((c, false));
                }
            }
        }
        out
    }

    /// Replace each value with its subtree's total — `node.sum()`.
    pub fn sum(&mut self) {
        for i in self.post_order() {
            let kids: f64 = self.nodes[i]
                .children
                .iter()
                .map(|&c| self.nodes[c].value)
                .sum();
            self.nodes[i].value += kids;
        }
    }

    /// Sort each node's children, largest value first (`node.sort`).
    pub fn sort_by_value(&mut self) {
        for i in 0..self.nodes.len() {
            let mut kids = self.nodes[i].children.clone();
            kids.sort_by(|&a, &b| self.nodes[b].value.total_cmp(&self.nodes[a].value));
            self.nodes[i].children = kids;
        }
    }

    #[must_use]
    pub fn leaves(&self) -> Vec<usize> {
        (0..self.nodes.len())
            .filter(|&i| self.nodes[i].children.is_empty())
            .collect()
    }
}

/// Squarified treemap (Bruls, Huizing & van Wijk 2000) — `d3.treemap()`
/// with `treemapSquarify`. Values must be summed. Returns a rect per node.
#[must_use]
pub fn treemap(h: &Hierarchy, area: Rect, padding: f32) -> Vec<Rect> {
    let mut out = vec![Rect::ZERO; h.nodes.len()];
    if h.nodes.is_empty() {
        return out;
    }
    out[0] = area;
    let mut stack = vec![0usize];
    while let Some(i) = stack.pop() {
        let kids: Vec<usize> = h.nodes[i]
            .children
            .iter()
            .copied()
            .filter(|&c| h.nodes[c].value > 0.0)
            .collect();
        if kids.is_empty() {
            continue;
        }
        let r = out[i].inflate(-padding);
        if r.width() <= 0.0 || r.height() <= 0.0 {
            continue;
        }
        let mut sorted = kids.clone();
        sorted.sort_by(|&a, &b| h.nodes[b].value.total_cmp(&h.nodes[a].value));
        squarify(h, &sorted, r, &mut out);
        stack.extend(sorted);
    }
    out
}

#[allow(clippy::cast_possible_truncation)]
fn squarify(h: &Hierarchy, items: &[usize], rect: Rect, out: &mut [Rect]) {
    let total: f64 = items.iter().map(|&i| h.nodes[i].value).sum();
    if total <= 0.0 {
        return;
    }
    let scale = f64::from(rect.width()) * f64::from(rect.height()) / total;
    let mut remaining = rect;
    let mut i = 0;
    while i < items.len() {
        let short = f64::from(remaining.width().min(remaining.height()));
        // Grow a row while the worst aspect ratio improves.
        let mut row_end = i + 1;
        let worst = |a: usize, b: usize| {
            let areas: Vec<f64> = items[a..b]
                .iter()
                .map(|&k| h.nodes[k].value * scale)
                .collect();
            let s: f64 = areas.iter().sum();
            let (mx, mn) = areas
                .iter()
                .fold((0.0f64, f64::MAX), |(x, n), &v| (x.max(v), n.min(v)));
            (short * short * mx / (s * s)).max(s * s / (short * short * mn))
        };
        while row_end < items.len() && worst(i, row_end + 1) <= worst(i, row_end) {
            row_end += 1;
        }
        let row_sum: f64 = items[i..row_end]
            .iter()
            .map(|&k| h.nodes[k].value * scale)
            .sum();
        let horizontal = remaining.width() >= remaining.height();
        let thick = (row_sum / short) as f32;
        let mut cursor = 0.0f32;
        for &k in &items[i..row_end] {
            let len = ((h.nodes[k].value * scale) / (row_sum / short)) as f32;
            out[k] = if horizontal {
                Rect::new(
                    remaining.left,
                    remaining.top + cursor,
                    remaining.left + thick,
                    remaining.top + cursor + len,
                )
            } else {
                Rect::new(
                    remaining.left + cursor,
                    remaining.top,
                    remaining.left + cursor + len,
                    remaining.top + thick,
                )
            };
            cursor += len;
        }
        remaining = if horizontal {
            Rect::new(
                remaining.left + thick,
                remaining.top,
                remaining.right,
                remaining.bottom,
            )
        } else {
            Rect::new(
                remaining.left,
                remaining.top + thick,
                remaining.right,
                remaining.bottom,
            )
        };
        i = row_end;
    }
}

/// Icicle partition: each depth a band, each node's width proportional to
/// its value — `d3.partition()`. Values must be summed.
#[must_use]
pub fn partition(h: &Hierarchy, area: Rect) -> Vec<Rect> {
    let mut out = vec![Rect::ZERO; h.nodes.len()];
    if h.nodes.is_empty() {
        return out;
    }
    #[allow(clippy::cast_precision_loss)]
    let band = area.height() / (h.nodes[0].height + 1) as f32;
    out[0] = Rect::new(area.left, area.top, area.right, area.top + band);
    for i in 0..h.nodes.len() {
        let r = out[i];
        let total = h.nodes[i].value;
        let mut x = r.left;
        for &c in &h.nodes[i].children {
            #[allow(clippy::cast_possible_truncation)]
            let w = if total > 0.0 {
                (h.nodes[c].value / total) as f32 * r.width()
            } else {
                0.0
            };
            out[c] = Rect::new(x, r.bottom, x + w, r.bottom + band);
            x += w;
        }
    }
    out
}

/// Tidy tree (Buchheim, Jünger & Leipert 2002) — `d3.tree()`: non-leaf
/// nodes centred over children, subtrees packed as close as the
/// `separation` allows, identical subtrees drawn identically. Returns a
/// point per node in `size` (x across, y down by depth).
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn tree(h: &Hierarchy, size: (f32, f32)) -> Vec<Offset> {
    let n = h.nodes.len();
    if n == 0 {
        return Vec::new();
    }
    let kids = |v: usize| &h.nodes[v].children;
    let parent = |v: usize| h.nodes[v].parent;
    let sibling_index: Vec<usize> = (0..n)
        .map(|v| parent(v).map_or(0, |p| kids(p).iter().position(|&c| c == v).unwrap_or(0)))
        .collect();
    let separation = |a: usize, b: usize| if parent(a) == parent(b) { 1.0 } else { 2.0 };
    let mut prelim = vec![0.0f64; n];
    let mut modv = vec![0.0f64; n];
    let mut change = vec![0.0f64; n];
    let mut shift = vec![0.0f64; n];
    let mut thread: Vec<Option<usize>> = vec![None; n];
    let mut ancestor: Vec<usize> = (0..n).collect();
    // d3's `parent.A`: the default ancestor for each parent's children.
    let mut default_ancestor: Vec<Option<usize>> = vec![None; n];

    let next_left = |v: usize, thread: &[Option<usize>]| kids(v).first().copied().or(thread[v]);
    let next_right = |v: usize, thread: &[Option<usize>]| kids(v).last().copied().or(thread[v]);
    let left_sibling = |v: usize| {
        let i = sibling_index[v];
        (i > 0).then(|| kids(parent(v).expect("sibling has a parent"))[i - 1])
    };
    let leftmost_sibling = |v: usize| parent(v).map(|p| kids(p)[0]);

    for v in h.post_order() {
        if kids(v).is_empty() {
            prelim[v] = left_sibling(v).map_or(0.0, |w| prelim[w] + separation(v, w));
        } else {
            // Execute shifts.
            let (mut s, mut c) = (0.0, 0.0);
            for &w in kids(v).iter().rev() {
                prelim[w] += s;
                modv[w] += s;
                c += change[w];
                s += shift[w] + c;
            }
            let first = kids(v)[0];
            let last = *kids(v).last().expect("non-empty");
            let mid = (prelim[first] + prelim[last]) / 2.0;
            match left_sibling(v) {
                Some(w) => {
                    prelim[v] = prelim[w] + separation(v, w);
                    modv[v] = prelim[v] - mid;
                }
                None => prelim[v] = mid,
            }
        }
        // Apportion: push v's subtree right of its left siblings'.
        let Some(par) = parent(v) else { continue };
        let mut default = default_ancestor[par].unwrap_or(kids(par)[0]);
        if let Some(w) = left_sibling(v) {
            let mut vip = v;
            let mut vop = v;
            let mut vim = w;
            let mut vom = leftmost_sibling(v).expect("has siblings");
            let (mut sip, mut sop, mut sim, mut som) = (modv[vip], modv[vop], modv[vim], modv[vom]);
            while let (Some(nim), Some(nip)) = (next_right(vim, &thread), next_left(vip, &thread)) {
                vim = nim;
                vip = nip;
                vom = next_left(vom, &thread).expect("contour");
                vop = next_right(vop, &thread).expect("contour");
                ancestor[vop] = v;
                let sh = prelim[vim] + sim - prelim[vip] - sip + separation(vim, vip);
                if sh > 0.0 {
                    let anc = if parent(ancestor[vim]) == parent(v) {
                        ancestor[vim]
                    } else {
                        default
                    };
                    // Move subtree.
                    #[allow(clippy::cast_precision_loss)]
                    let subtrees = (sibling_index[v] - sibling_index[anc]) as f64;
                    change[v] -= sh / subtrees;
                    shift[v] += sh;
                    change[anc] += sh / subtrees;
                    prelim[v] += sh;
                    modv[v] += sh;
                    sip += sh;
                    sop += sh;
                }
                sim += modv[vim];
                sip += modv[vip];
                som += modv[vom];
                sop += modv[vop];
            }
            if let (Some(t), None) = (next_right(vim, &thread), next_right(vop, &thread)) {
                thread[vop] = Some(t);
                modv[vop] += sim - sop;
            }
            if let (Some(t), None) = (next_left(vip, &thread), next_left(vom, &thread)) {
                thread[vom] = Some(t);
                modv[vom] += sip - som;
                default = v;
            }
        }
        default_ancestor[par] = Some(default);
    }
    // Second walk: absolute x = prelim + sum of ancestors' mod.
    let mut x = vec![0.0f64; n];
    let mut stack = vec![(0usize, 0.0f64)];
    while let Some((v, acc)) = stack.pop() {
        x[v] = prelim[v] + acc;
        for &c in kids(v) {
            stack.push((c, acc + modv[v]));
        }
    }
    let (lo, hi) = x
        .iter()
        .fold((f64::MAX, f64::MIN), |(a, b), &v| (a.min(v), b.max(v)));
    let depth = h.nodes[0].height.max(1);
    (0..n)
        .map(|v| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
            let px = if hi > lo {
                ((x[v] - lo) / (hi - lo)) as f32 * size.0
            } else {
                size.0 / 2.0
            };
            #[allow(clippy::cast_precision_loss)]
            let py = h.nodes[v].depth as f32 / depth as f32 * size.1;
            Offset::new(px, py)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Hierarchy {
        Hierarchy::stratify(&[
            ("root", None, 0.0),
            ("a", Some("root"), 0.0),
            ("b", Some("root"), 0.0),
            ("a1", Some("a"), 6.0),
            ("a2", Some("a"), 6.0),
            ("b1", Some("b"), 4.0),
            ("b2", Some("b"), 3.0),
            ("b3", Some("b"), 1.0),
        ])
        .unwrap()
    }

    #[test]
    fn stratify_and_sum() {
        let mut h = sample();
        assert_eq!(h.nodes[0].height, 2);
        h.sum();
        assert_eq!(h.nodes[0].value, 20.0);
        assert!(Hierarchy::stratify(&[("x", None, 0.0), ("y", None, 0.0)]).is_err());
        assert!(Hierarchy::stratify(&[("x", None, 0.0), ("y", Some("nope"), 0.0)]).is_err());
    }

    #[test]
    fn treemap_areas_are_proportional_and_tile_the_rect() {
        let mut h = sample();
        h.sum();
        let rects = treemap(&h, Rect::new(0.0, 0.0, 200.0, 100.0), 0.0);
        let total = 200.0 * 100.0;
        for &leaf in &h.leaves() {
            #[allow(clippy::cast_possible_truncation)]
            let expect = (h.nodes[leaf].value / 20.0) as f32 * total;
            assert!(
                (rects[leaf].area() - expect).abs() < 1.0,
                "{} {} vs {expect}",
                h.nodes[leaf].id,
                rects[leaf].area()
            );
        }
        // Leaves do not overlap.
        let leaves = h.leaves();
        for (i, &a) in leaves.iter().enumerate() {
            for &b in &leaves[i + 1..] {
                assert!(rects[a].intersect(rects[b]).area() < 1e-2);
            }
        }
    }

    #[test]
    fn partition_bands_by_depth() {
        let mut h = sample();
        h.sum();
        let p = partition(&h, Rect::new(0.0, 0.0, 100.0, 90.0));
        assert_eq!(p[0], Rect::new(0.0, 0.0, 100.0, 30.0));
        let a = h.nodes.iter().position(|n| n.id == "a").unwrap();
        assert!((p[a].width() - 60.0).abs() < 1e-3);
    }

    #[test]
    fn tidy_tree_centres_parents_and_never_overlaps() {
        let h = sample();
        let pts = tree(&h, (100.0, 100.0));
        for (i, n) in h.nodes.iter().enumerate() {
            if !n.children.is_empty() {
                let first = pts[n.children[0]].dx;
                let last = pts[*n.children.last().unwrap()].dx;
                assert!(
                    (pts[i].dx - (first + last) / 2.0).abs() < 1e-3,
                    "{} centred",
                    n.id
                );
            }
        }
        // Nodes at the same depth keep order and spacing.
        let leaves: Vec<f32> = h.leaves().iter().map(|&l| pts[l].dx).collect();
        assert!(leaves.windows(2).all(|w| w[1] > w[0]));
        assert_eq!(pts[0].dy, 0.0);
    }

    #[test]
    fn tidy_tree_handles_an_unbalanced_tree() {
        // A deep left subtree next to a shallow right one: apportion must
        // push the right subtree clear of the left one's contour.
        let h = Hierarchy::stratify(&[
            ("r", None, 0.0),
            ("l", Some("r"), 0.0),
            ("m", Some("r"), 0.0),
            ("x", Some("r"), 0.0),
            ("l1", Some("l"), 0.0),
            ("l2", Some("l"), 0.0),
            ("l11", Some("l1"), 0.0),
            ("l12", Some("l1"), 0.0),
            ("l21", Some("l2"), 0.0),
            ("l22", Some("l2"), 0.0),
            ("x1", Some("x"), 0.0),
            ("x11", Some("x1"), 0.0),
            ("x12", Some("x1"), 0.0),
        ])
        .unwrap();
        let pts = tree(&h, (300.0, 100.0));
        // Same-depth nodes are strictly ordered left to right by their
        // in-order position: no overlaps anywhere.
        for d in 0..=3 {
            let mut row: Vec<f32> = (0..h.nodes.len())
                .filter(|&i| h.nodes[i].depth == d)
                .map(|i| pts[i].dx)
                .collect();
            let sorted = {
                let mut s = row.clone();
                s.sort_by(f32::total_cmp);
                s
            };
            assert_eq!(row, sorted, "depth {d} keeps order");
            row.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
            assert_eq!(
                row.len(),
                sorted.len(),
                "depth {d}: no two nodes share a column"
            );
        }
    }
}
