//! Rotoscoping — After Effects' Roto Brush (§2.15 L7), Nuke's RotoPaint,
//! the "select subject" every compositor offers.
//!
//! Roto Brush is a learned system; this module is the classical algorithm
//! it grew out of, implemented in full and named honestly:
//!
//! * **GrabCut** (Rother, Kolmogorov & Blake 2004): a [`Trimap`] (sure
//!   foreground, sure background, unknown — from a rectangle or from user
//!   strokes) seeds two **Gaussian mixture models** of colour (k-means
//!   initialised, full 3×3 covariance); every unknown pixel's data cost is
//!   its negative log-likelihood under each model, neighbouring pixels pay
//!   a contrast-sensitive smoothness cost (`γ·exp(−β‖Δc‖²)`, β from the
//!   image's mean contrast); a **minimum cut** of that graph (Dinic's
//!   max-flow) labels the pixels; models refit, repeat.
//! * **Propagation** ([`propagate`]): the previous frame's mask is carried
//!   forward by dense optical flow, eroded into a sure-inside core and
//!   dilated into a sure-outside band, and re-cut on the new frame — the
//!   temporal half of Roto Brush.
//!
//! What is not claimed: learned priors (hair, motion-blurred edges), and
//! matting of semi-transparent boundaries beyond a feathered edge.

use vieww_foundation::Image;

use crate::cv::{horn_schunck, Plane};

/// Per-pixel constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label {
    Background,
    Foreground,
    /// Let the cut decide.
    Unknown,
}

/// A labelled constraint map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trimap {
    pub w: usize,
    pub h: usize,
    pub labels: Vec<Label>,
}

impl Trimap {
    /// Everything outside `rect` is background; inside is unknown — the
    /// GrabCut rectangle.
    #[must_use]
    pub fn rect(w: usize, h: usize, x0: usize, y0: usize, x1: usize, y1: usize) -> Self {
        let mut labels = vec![Label::Background; w * h];
        for y in y0.min(h)..y1.min(h) {
            for x in x0.min(w)..x1.min(w) {
                labels[y * w + x] = Label::Unknown;
            }
        }
        Self { w, h, labels }
    }

    /// Paint a disc of `label` (a brush stroke).
    pub fn stroke(&mut self, cx: f32, cy: f32, radius: f32, label: Label) {
        for y in 0..self.h {
            for x in 0..self.w {
                #[allow(clippy::cast_precision_loss)]
                let d = (x as f32 - cx).powi(2) + (y as f32 - cy).powi(2);
                if d <= radius * radius {
                    self.labels[y * self.w + x] = label;
                }
            }
        }
    }
}

/// A Gaussian mixture over RGB.
#[derive(Debug, Clone)]
struct Gmm {
    weight: Vec<f64>,
    mean: Vec<[f64; 3]>,
    /// Inverse covariance and log-determinant per component.
    inv: Vec<[[f64; 3]; 3]>,
    logdet: Vec<f64>,
}

fn inv3(m: &[[f64; 3]; 3]) -> ([[f64; 3]; 3], f64) {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    let d = det.max(1e-12);
    let inv = [
        [
            (m[1][1] * m[2][2] - m[1][2] * m[2][1]) / d,
            (m[0][2] * m[2][1] - m[0][1] * m[2][2]) / d,
            (m[0][1] * m[1][2] - m[0][2] * m[1][1]) / d,
        ],
        [
            (m[1][2] * m[2][0] - m[1][0] * m[2][2]) / d,
            (m[0][0] * m[2][2] - m[0][2] * m[2][0]) / d,
            (m[0][2] * m[1][0] - m[0][0] * m[1][2]) / d,
        ],
        [
            (m[1][0] * m[2][1] - m[1][1] * m[2][0]) / d,
            (m[0][1] * m[2][0] - m[0][0] * m[2][1]) / d,
            (m[0][0] * m[1][1] - m[0][1] * m[1][0]) / d,
        ],
    ];
    (inv, d.ln())
}

impl Gmm {
    /// Fit `k` components: k-means (seeded deterministically) then moments.
    fn fit(samples: &[[f64; 3]], k: usize) -> Self {
        let k = k.min(samples.len()).max(1);
        let mut centers: Vec<[f64; 3]> = (0..k).map(|i| samples[i * samples.len() / k]).collect();
        let mut assign = vec![0usize; samples.len()];
        for _ in 0..8 {
            for (a, s) in assign.iter_mut().zip(samples) {
                *a = (0..k)
                    .min_by(|&i, &j| d2(s, &centers[i]).total_cmp(&d2(s, &centers[j])))
                    .unwrap_or(0);
            }
            let mut sum = vec![[0.0; 3]; k];
            let mut n = vec![0usize; k];
            for (a, s) in assign.iter().zip(samples) {
                for c in 0..3 {
                    sum[*a][c] += s[c];
                }
                n[*a] += 1;
            }
            for i in 0..k {
                if n[i] > 0 {
                    #[allow(clippy::cast_precision_loss)]
                    let m = n[i] as f64;
                    centers[i] = sum[i].map(|v| v / m);
                }
            }
        }
        Self::from_assignment(samples, &assign, k)
    }

    fn from_assignment(samples: &[[f64; 3]], assign: &[usize], k: usize) -> Self {
        let mut g = Self {
            weight: vec![0.0; k],
            mean: vec![[0.0; 3]; k],
            inv: vec![[[0.0; 3]; 3]; k],
            logdet: vec![0.0; k],
        };
        let mut n = vec![0usize; k];
        for (a, s) in assign.iter().zip(samples) {
            for c in 0..3 {
                g.mean[*a][c] += s[c];
            }
            n[*a] += 1;
        }
        for i in 0..k {
            #[allow(clippy::cast_precision_loss)]
            let m = n[i].max(1) as f64;
            g.mean[i] = g.mean[i].map(|v| v / m);
        }
        let mut cov = vec![[[0.0; 3]; 3]; k];
        for (a, s) in assign.iter().zip(samples) {
            let d = [s[0] - g.mean[*a][0], s[1] - g.mean[*a][1], s[2] - g.mean[*a][2]];
            for r in 0..3 {
                for c in 0..3 {
                    cov[*a][r][c] += d[r] * d[c];
                }
            }
        }
        #[allow(clippy::cast_precision_loss)]
        let total = samples.len().max(1) as f64;
        for i in 0..k {
            #[allow(clippy::cast_precision_loss)]
            let m = n[i].max(1) as f64;
            for r in 0..3 {
                for c in 0..3 {
                    cov[i][r][c] /= m;
                }
                cov[i][r][r] += 1e-3; // regularise flat clusters
            }
            #[allow(clippy::cast_precision_loss)]
            {
                g.weight[i] = n[i] as f64 / total;
            }
            let (iv, ld) = inv3(&cov[i]);
            g.inv[i] = iv;
            g.logdet[i] = ld;
        }
        g
    }

    fn component_cost(&self, i: usize, s: &[f64; 3]) -> f64 {
        let d = [s[0] - self.mean[i][0], s[1] - self.mean[i][1], s[2] - self.mean[i][2]];
        let mut q = 0.0;
        for r in 0..3 {
            for c in 0..3 {
                q += d[r] * self.inv[i][r][c] * d[c];
            }
        }
        0.5 * q + 0.5 * self.logdet[i]
    }

    /// −log p(s).
    fn cost(&self, s: &[f64; 3]) -> f64 {
        let p: f64 = (0..self.weight.len())
            .filter(|&i| self.weight[i] > 0.0)
            .map(|i| self.weight[i] * (-self.component_cost(i, s)).exp())
            .sum();
        -(p.max(1e-300)).ln()
    }

    fn best_component(&self, s: &[f64; 3]) -> usize {
        (0..self.weight.len())
            .min_by(|&i, &j| self.component_cost(i, s).total_cmp(&self.component_cost(j, s)))
            .unwrap_or(0)
    }
}

fn d2(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// Dinic's max-flow on an adjacency-list graph.
struct Flow {
    head: Vec<usize>,
    to: Vec<usize>,
    cap: Vec<f64>,
    next: Vec<usize>,
}

impl Flow {
    const NONE: usize = usize::MAX;
    fn new(n: usize) -> Self {
        Self {
            head: vec![Self::NONE; n],
            to: Vec::new(),
            cap: Vec::new(),
            next: Vec::new(),
        }
    }
    fn edge(&mut self, a: usize, b: usize, c_ab: f64, c_ba: f64) {
        for (u, v, c) in [(a, b, c_ab), (b, a, c_ba)] {
            self.to.push(v);
            self.cap.push(c);
            self.next.push(self.head[u]);
            self.head[u] = self.to.len() - 1;
        }
    }
    /// Max flow; afterwards `reachable` marks the source side of the cut.
    fn run(&mut self, s: usize, t: usize) -> Vec<bool> {
        let n = self.head.len();
        loop {
            let mut level = vec![-1i32; n];
            level[s] = 0;
            let mut q = std::collections::VecDeque::from([s]);
            while let Some(u) = q.pop_front() {
                let mut e = self.head[u];
                while e != Self::NONE {
                    if self.cap[e] > 1e-12 && level[self.to[e]] < 0 {
                        level[self.to[e]] = level[u] + 1;
                        q.push_back(self.to[e]);
                    }
                    e = self.next[e];
                }
            }
            if level[t] < 0 {
                return level.iter().map(|&l| l >= 0).collect();
            }
            let mut it = self.head.clone();
            loop {
                // Iterative DFS for one blocking path.
                let mut stack: Vec<usize> = Vec::new(); // edges
                let mut u = s;
                let found = loop {
                    if u == t {
                        break true;
                    }
                    let mut advanced = false;
                    while it[u] != Self::NONE {
                        let e = it[u];
                        let v = self.to[e];
                        if self.cap[e] > 1e-12 && level[v] == level[u] + 1 {
                            stack.push(e);
                            u = v;
                            advanced = true;
                            break;
                        }
                        it[u] = self.next[e];
                    }
                    if !advanced {
                        if u == s {
                            break false;
                        }
                        let e = stack.pop().expect("a path edge");
                        u = self.to[e ^ 1];
                        it[u] = self.next[it[u]];
                    }
                };
                if !found {
                    break;
                }
                let f = stack.iter().map(|&e| self.cap[e]).fold(f64::INFINITY, f64::min);
                for &e in &stack {
                    self.cap[e] -= f;
                    self.cap[e ^ 1] += f;
                }
            }
        }
    }
}

/// A soft mask, `0..1` per pixel.
#[derive(Debug, Clone, PartialEq)]
pub struct Mask {
    pub w: usize,
    pub h: usize,
    pub alpha: Vec<f32>,
}

impl Mask {
    /// The mask as a white-on-black image.
    #[must_use]
    pub fn to_image(&self) -> Image {
        Plane {
            w: self.w,
            h: self.h,
            data: self.alpha.clone(),
        }
        .to_image()
    }

    /// Multiply `img`'s alpha by the mask.
    #[must_use]
    pub fn apply(&self, img: &Image) -> Image {
        let mut px = img.pixels().to_vec();
        for (p, a) in px.as_chunks_mut::<4>().0.iter_mut().zip(&self.alpha) {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                p[3] = (f32::from(p[3]) * a) as u8;
            }
        }
        Image::from_rgba8(px, img.width(), img.height())
    }

    /// Fraction of pixels inside.
    #[must_use]
    pub fn coverage(&self) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        let n = self.alpha.len().max(1) as f32;
        self.alpha.iter().sum::<f32>() / n
    }
}

fn colours(img: &Image) -> Vec<[f64; 3]> {
    img.pixels()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])])
        .collect()
}

/// GrabCut segmentation of `img` under `trimap`.
///
/// # Panics
/// If the trimap's size differs from the image's.
#[must_use]
pub fn grabcut(img: &Image, trimap: &Trimap, iterations: usize) -> Mask {
    let (w, h) = (img.width() as usize, img.height() as usize);
    assert_eq!((w, h), (trimap.w, trimap.h), "trimap size");
    let px = colours(img);
    let n = w * h;
    // Initial labelling: unknown counts as foreground.
    let mut fg: Vec<bool> = trimap.labels.iter().map(|l| *l != Label::Background).collect();
    // β from mean squared neighbour contrast.
    let mut sum = 0.0;
    let mut cnt = 0.0;
    for y in 0..h {
        for x in 0..w {
            if x + 1 < w {
                sum += d2(&px[y * w + x], &px[y * w + x + 1]);
                cnt += 1.0;
            }
            if y + 1 < h {
                sum += d2(&px[y * w + x], &px[(y + 1) * w + x]);
                cnt += 1.0;
            }
        }
    }
    let beta = if sum > 0.0 { cnt / (2.0 * sum) } else { 0.0 };
    let gamma = 50.0;
    for _ in 0..iterations.max(1) {
        let (fs, bs): (Vec<[f64; 3]>, Vec<[f64; 3]>) = {
            let mut f = Vec::new();
            let mut b = Vec::new();
            for i in 0..n {
                if fg[i] {
                    f.push(px[i]);
                } else {
                    b.push(px[i]);
                }
            }
            (f, b)
        };
        if fs.is_empty() || bs.is_empty() {
            break;
        }
        let (gf, gb) = (Gmm::fit(&fs, 5), Gmm::fit(&bs, 5));
        // Refit once with each pixel assigned to its best component.
        let af: Vec<usize> = fs.iter().map(|s| gf.best_component(s)).collect();
        let ab: Vec<usize> = bs.iter().map(|s| gb.best_component(s)).collect();
        let (gf, gb) = (
            Gmm::from_assignment(&fs, &af, gf.weight.len()),
            Gmm::from_assignment(&bs, &ab, gb.weight.len()),
        );
        let (s, t) = (n, n + 1);
        let mut g = Flow::new(n + 2);
        let big = 1e9;
        for i in 0..n {
            let (cs, ct) = match trimap.labels[i] {
                Label::Foreground => (big, 0.0),
                Label::Background => (0.0, big),
                // Source capacity = cost of labelling background.
                Label::Unknown => (gb.cost(&px[i]), gf.cost(&px[i])),
            };
            g.edge(s, i, cs, 0.0);
            g.edge(i, t, ct, 0.0);
        }
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                for (j, diag) in [(i + 1, x + 1 < w), (i + w, y + 1 < h)] {
                    if diag {
                        let c = gamma * (-beta * d2(&px[i], &px[j])).exp();
                        g.edge(i, j, c, c);
                    }
                }
            }
        }
        let side = g.run(s, t);
        let changed = (0..n).filter(|&i| side[i] != fg[i]).count();
        fg.copy_from_slice(&side[..n]);
        if changed == 0 {
            break;
        }
    }
    // A 1-pixel feather on the boundary.
    let mut alpha: Vec<f32> = fg.iter().map(|&b| if b { 1.0 } else { 0.0 }).collect();
    let hard = alpha.clone();
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let i = y * w + x;
            let s: f32 = [i - 1, i + 1, i - w, i + w, i].iter().map(|&j| hard[j]).sum();
            alpha[i] = s / 5.0;
        }
    }
    Mask { w, h, alpha }
}

/// Carry `mask` from `prev` to `next` by optical flow and re-cut: the
/// warped mask, eroded by `band`, is sure foreground; dilated by `band`,
/// its outside is sure background; the band between is decided by GrabCut.
#[must_use]
pub fn propagate(prev: &Image, next: &Image, mask: &Mask, band: usize) -> Mask {
    let (w, h) = (mask.w, mask.h);
    let flow = horn_schunck(&Plane::luma(prev), &Plane::luma(next), 0.08, 40, 3);
    // Backward warp: a pixel of `next` came from (x − u, y − v) in `prev`.
    let src = Plane {
        w,
        h,
        data: mask.alpha.clone(),
    };
    let mut warped = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let (u, v) = flow.at(x, y);
            #[allow(clippy::cast_precision_loss)]
            {
                warped[y * w + x] = src.sample(x as f32 - u, y as f32 - v);
            }
        }
    }
    let inside: Vec<bool> = warped.iter().map(|&a| a > 0.5).collect();
    let near = |x: usize, y: usize, want: bool| -> bool {
        let r = band as isize;
        for dy in -r..=r {
            for dx in -r..=r {
                let (xx, yy) = (x as isize + dx, y as isize + dy);
                if xx < 0 || yy < 0 || xx >= w as isize || yy >= h as isize {
                    continue;
                }
                #[allow(clippy::cast_sign_loss)]
                if inside[yy as usize * w + xx as usize] == want {
                    return true;
                }
            }
        }
        false
    };
    let mut tri = Trimap {
        w,
        h,
        labels: vec![Label::Unknown; w * h],
    };
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            tri.labels[i] = if inside[i] && !near(x, y, false) {
                Label::Foreground
            } else if !inside[i] && !near(x, y, true) {
                Label::Background
            } else {
                Label::Unknown
            };
        }
    }
    grabcut(next, &tri, 3)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A noisy orange disc on a noisy blue-green background.
    fn scene(cx: f32, cy: f32) -> Image {
        let (w, h) = (48usize, 40usize);
        let mut px = Vec::with_capacity(w * h * 4);
        let mut s = 99u32;
        for y in 0..h {
            for x in 0..w {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                #[allow(clippy::cast_possible_truncation)]
                let n = (s % 24) as u8;
                #[allow(clippy::cast_precision_loss)]
                let inside = (x as f32 - cx).powi(2) + (y as f32 - cy).powi(2) < 100.0;
                if inside {
                    px.extend([220 + n / 2, 120 + n, 30 + n, 255]);
                } else {
                    px.extend([30 + n, 90 + n, 120 + n, 255]);
                }
            }
        }
        #[allow(clippy::cast_possible_truncation)]
        Image::from_rgba8(px, w as u32, h as u32)
    }

    fn truth(cx: f32, cy: f32) -> Vec<bool> {
        (0..48 * 40)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let (x, y) = ((i % 48) as f32, (i / 48) as f32);
                (x - cx).powi(2) + (y - cy).powi(2) < 100.0
            })
            .collect()
    }

    fn iou(m: &Mask, t: &[bool]) -> f32 {
        let (mut i, mut u) = (0.0f32, 0.0f32);
        for (a, &b) in m.alpha.iter().zip(t) {
            let a = *a > 0.5;
            if a && b {
                i += 1.0;
            }
            if a || b {
                u += 1.0;
            }
        }
        i / u.max(1.0)
    }

    #[test]
    fn grabcut_from_a_rectangle_finds_the_disc() {
        let img = scene(20.0, 18.0);
        let m = grabcut(&img, &Trimap::rect(48, 40, 6, 4, 36, 32), 4);
        let s = iou(&m, &truth(20.0, 18.0));
        assert!(s > 0.9, "IoU {s}");
    }

    #[test]
    fn strokes_are_respected() {
        let img = scene(20.0, 18.0);
        let mut t = Trimap::rect(48, 40, 6, 4, 36, 32);
        // Insist a background patch inside the disc is background.
        t.stroke(20.0, 18.0, 3.0, Label::Background);
        let m = grabcut(&img, &t, 3);
        assert!(m.alpha[18 * 48 + 20] < 0.5);
    }

    #[test]
    fn the_mask_follows_the_subject_to_the_next_frame() {
        let (a, b) = (scene(20.0, 18.0), scene(23.0, 19.0));
        let m0 = grabcut(&a, &Trimap::rect(48, 40, 6, 4, 36, 32), 4);
        let m1 = propagate(&a, &b, &m0, 3);
        let s = iou(&m1, &truth(23.0, 19.0));
        assert!(s > 0.85, "IoU {s}");
        assert!(m1.coverage() > 0.1);
        let cut = m1.apply(&b);
        assert_eq!(cut.pixels()[3], 0, "corner keyed out");
    }

    #[test]
    fn max_flow_on_a_tiny_graph() {
        // s→a 3, s→b 2, a→b 1, a→t 2, b→t 3: max flow 5, cut {s}.
        let mut g = Flow::new(4);
        let (s, a, b, t) = (0, 1, 2, 3);
        g.edge(s, a, 3.0, 0.0);
        g.edge(s, b, 2.0, 0.0);
        g.edge(a, b, 1.0, 0.0);
        g.edge(a, t, 2.0, 0.0);
        g.edge(b, t, 3.0, 0.0);
        let side = g.run(s, t);
        assert!(side[s] && !side[t]);
        let residual_out_of_s: f64 = [0usize, 2].iter().map(|&e| g.cap[e]).sum();
        assert!(residual_out_of_s.abs() < 1e-9, "both source edges saturated");
    }
}
