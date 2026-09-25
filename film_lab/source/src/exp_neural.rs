//! exp_neural — *the learning axis.* The machine that changes its mind.
//!
//! Two interleaved spirals — the dataset that embarrassed a generation of
//! linear models — and a small multilayer perceptron (2 → 16 → 16 → 1,
//! tanh) learning to tell them apart, **trained from a fixed seed and
//! replayed from scratch every frame** up to its own epoch count: the
//! network at t is a pure function of the seed and the schedule. Nothing
//! is carried between frames; determinism is the discipline, and the
//! cost of it shows up honestly in the build/raster split.
//!
//! What you watch is the **decision field** — a 128×72 lattice of forward
//! passes, each cell coloured by the network's live answer, violet vs
//! amber — sharpen from a haze into two winding territories. The data
//! points ride on top; the ones the net currently gets wrong wear a white
//! ring, so the mistakes are visible as they burn away.
//!
//! The receipt is the training log itself: the loss and accuracy measured
//! on the same points that are drawn, at the epochs the replay actually
//! completed — printed by the machine that felt them. The loss curve in
//! the corner is plotted from the same replay, milestone by milestone.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, clamp01, mix, tint, AMBER, CYAN, INK, MUTED, VIOLET,
    VIOLET_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

// ── The dataset ─────────────────────────────────────────────────────────────

/// Points per spiral.
const PTS: usize = 150;

/// One data point: position in [−1, 1]², label ±1.
struct Pt {
    x: f32,
    y: f32,
    label: f32,
}

#[must_use]
fn dataset() -> Vec<Pt> {
    let mut rng = crate::film_lib::Rng::new(0x51CA);
    let mut pts = Vec::with_capacity(2 * PTS);
    for class in [0.0_f32, 1.0] {
        for i in 0..PTS {
            let s = i as f32 / PTS as f32;
            // One and a half turns — interlocked, and learnable to 100%
            // by this architecture within the replay budget (measured:
            // the 2.25-turn version plateaus at 62% and never clicks).
            let ang = s * 1.5 * std::f32::consts::TAU + class * std::f32::consts::PI;
            let r = 0.08 + 0.87 * s;
            let jr = rng.f01() * 0.03;
            let ja = rng.f01() * 0.05;
            pts.push(Pt {
                x: (r + jr) * (ang + ja).cos(),
                y: (r + jr) * (ang + ja).sin(),
                label: if class == 0.0 { 1.0 } else { -1.0 },
            });
        }
    }
    pts
}

// ── The network ─────────────────────────────────────────────────────────────

/// Layer sizes.
const L0: usize = 2;
const L1: usize = 16;
const L2: usize = 16;
const L3: usize = 1;

/// Total epochs at t = 1 — past the click (measured: the plateau holds
/// until ≈1500, then the loss falls 0.65 → 0.17 → 0.004 within 300).
const EPOCHS: usize = 2400;

/// Mini-batch size — the batch noise is what escapes the saddle; the
/// full-batch run sat at loss = ln 2 for 2000 epochs and never moved.
const BATCH: usize = 20;

/// Learning rate (with classical momentum). 0.05 clicks; 0.1 does not
/// (measured — the optimiser's own receipt).
const LR: f32 = 0.05;
const MOMENTUM: f32 = 0.9;

/// The network's parameters (row-major weights, per-layer biases).
struct Net {
    w1: Vec<f32>, // L1×L0
    b1: Vec<f32>,
    w2: Vec<f32>, // L2×L1
    b2: Vec<f32>,
    w3: Vec<f32>, // L3×L2
    b3: Vec<f32>,
}

impl Net {
    fn seeded() -> Self {
        let mut rng = crate::film_lib::Rng::new(0x4E7);
        let mut init = |n: usize, scale: f32| -> Vec<f32> {
            (0..n).map(|_| rng.sym() * scale).collect()
        };
        Self {
            w1: init(L1 * L0, 0.7),
            b1: vec![0.0; L1],
            w2: init(L2 * L1, 0.25),
            b2: vec![0.0; L2],
            w3: init(L3 * L2, 0.25),
            b3: vec![0.0; L3],
        }
    }

    /// Forward pass, exposing the hidden activations for backprop.
    /// The output is the FULL readout Σ w3[i]·h2[i] + b3 — the first cut
    /// read only w3[0]·h2[0] (a stray helper index), leaving the output
    /// connected to one hidden unit in sixteen: the net trained to loss
    /// = ln 2 and 0.6% accuracy and the receipt printed the failure.
    fn forward(&self, x: f32, y: f32, h1: &mut [f32], h2: &mut [f32]) -> f32 {
        for j in 0..L1 {
            let z = self.w1[j * L0] * x + self.w1[j * L0 + 1] * y + self.b1[j];
            h1[j] = z.tanh();
        }
        for j in 0..L2 {
            let mut z = self.b2[j];
            for i in 0..L1 {
                z += self.w2[j * L1 + i] * h1[i];
            }
            h2[j] = z.tanh();
        }
        let mut out = self.b3[0];
        for i in 0..L2 {
            out += self.w3[i] * h2[i];
        }
        out
    }

    /// One mini-batch epoch: a deterministic Fisher–Yates shuffle (seeded
    /// — the replay stays a pure function of the seed), then gradient
    /// steps per batch. Returns the mean logistic loss over the epoch.
    fn train_epoch(&mut self, pts: &[Pt], vw: &mut [f32], vb: &mut [f32], shuf: &mut crate::film_lib::Rng) -> f32 {
        let mut order: Vec<usize> = (0..pts.len()).collect();
        for i in (1..order.len()).rev() {
            let j = (shuf.f01() * (i + 1) as f32) as usize % (i + 1);
            order.swap(i, j);
        }
        let mut total = 0.0_f32;
        let mut batches = 0usize;
        for chunk in order.chunks(BATCH) {
            total += self.train_batch(pts, chunk, vw, vb);
            batches += 1;
        }
        total / batches.max(1) as f32
    }

    /// One mini-batch gradient step; returns the mean logistic loss.
    fn train_batch(&mut self, pts: &[Pt], idxs: &[usize], vw: &mut [f32], vb: &mut [f32]) -> f32 {
        // Gradients, zeroed per epoch (accumulated over the batch).
        let mut gw1 = vec![0.0_f32; L1 * L0];
        let mut gb1 = vec![0.0_f32; L1];
        let mut gw2 = vec![0.0_f32; L2 * L1];
        let mut gb2 = vec![0.0_f32; L2];
        let mut gw3 = vec![0.0_f32; L3 * L2];
        let mut gb3 = vec![0.0_f32; L3];
        let mut h1 = vec![0.0_f32; L1];
        let mut h2 = vec![0.0_f32; L2];
        let mut loss = 0.0_f32;

        for &pi_ in idxs {
            let p = &pts[pi_];
            let out = self.forward(p.x, p.y, &mut h1, &mut h2);
            // Logistic loss against label ±1, sigmoid margin form.
            let margin = out * p.label;
            loss += (1.0 + (-margin).exp()).ln();
            // dL/dout = −label · sigmoid(−margin)
            let dout = -p.label / (1.0 + margin.exp());
            // Output layer (linear): grad wrt w3, h2; delta wrt pre-act of
            // layer 3 is dout itself.
            let d3 = dout;
            for i in 0..L2 {
                gw3[i] += d3 * h2[i];
            }
            gb3[0] += d3;
            // Layer 2.
            let mut d2 = vec![0.0_f32; L2];
            for j in 0..L2 {
                d2[j] = d3 * self.w3[j] * (1.0 - h2[j] * h2[j]);
                for i in 0..L1 {
                    gw2[j * L1 + i] += d2[j] * h1[i];
                }
                gb2[j] += d2[j];
            }
            // Layer 1.
            let mut d1 = vec![0.0_f32; L1];
            for j in 0..L1 {
                let mut acc = 0.0;
                for k in 0..L2 {
                    acc += d2[k] * self.w2[k * L1 + j];
                }
                d1[j] = acc * (1.0 - h1[j] * h1[j]);
                gw1[j * L0] += d1[j] * p.x;
                gw1[j * L0 + 1] += d1[j] * p.y;
                gb1[j] += d1[j];
            }
        }

        let n = idxs.len().max(1) as f32;
        // Momentum update through the flattened view buffers.
        let apply = |g: &[f32], w: &mut Vec<f32>, v: &mut [f32]| {
            for i in 0..w.len() {
                v[i] = MOMENTUM * v[i] - LR * g.get(i).copied().unwrap_or(0.0) / n;
                w[i] += v[i];
            }
        };
        apply(&gw1, &mut self.w1, &mut vw[w1_slice()]);
        apply(&gb1, &mut self.b1, &mut vb[b1_slice()]);
        apply(&gw2, &mut self.w2, &mut vw[w2_slice()]);
        apply(&gb2, &mut self.b2, &mut vb[b2_slice()]);
        apply(&gw3, &mut self.w3, &mut vw[w3_slice()]);
        apply(&gb3, &mut self.b3, &mut vb[b3_slice()]);
        loss / n
    }

    /// The prediction sign-matched to a label.
    fn correct(&self, p: &Pt) -> bool {
        let mut h1 = vec![0.0_f32; L1];
        let mut h2 = vec![0.0_f32; L2];
        let out = self.forward(p.x, p.y, &mut h1, &mut h2);
        out * p.label > 0.0
    }
}

fn j0() -> usize {
    0
}
fn w1_slice() -> std::ops::Range<usize> {
    0..L1 * L0
}
fn b1_slice() -> std::ops::Range<usize> {
    L1 * L0..L1 * L0 + L1
}
fn w2_slice() -> std::ops::Range<usize> {
    L1 * L0 + L1..L1 * L0 + L1 + L2 * L1
}
fn b2_slice() -> std::ops::Range<usize> {
    L1 * L0 + L1 + L2 * L1..L1 * L0 + L1 + L2 * L1 + L2
}
fn w3_slice() -> std::ops::Range<usize> {
    L1 * L0 + L1 + L2 * L1 + L2..L1 * L0 + L1 + L2 * L1 + L2 + L3 * L2
}
fn b3_slice() -> std::ops::Range<usize> {
    L1 * L0 + L1 + L2 * L1 + L2 + L3 * L2..L1 * L0 + L1 + L2 * L1 + L2 + L3 * L2 + L3
}

/// Total velocity buffer length.
const VEL: usize = L1 * L0 + L1 + L2 * L1 + L2 + L3 * L2 + L3;

// ── The replay ──────────────────────────────────────────────────────────────

/// Replay training to epoch count `epochs`; returns (net, milestone losses
/// at every 30th epoch including 0, final loss, accuracy).
#[must_use]
fn replay(epochs: usize) -> (Net, Vec<(usize, f32)>, f32, f32) {
    let pts = dataset();
    let mut net = Net::seeded();
    let mut vw = vec![0.0_f32; VEL];
    let mut vb = vec![0.0_f32; VEL];
    // The shuffle stream: its own seed, drawn once per epoch in order —
    // deterministic, so the replay is byte-stable across frames.
    let mut shuf = crate::film_lib::Rng::new(0xBACC);
    let mut curve = Vec::new();
    let mut loss = 0.0_f32;
    for e in 0..epochs {
        loss = net.train_epoch(&pts, &mut vw, &mut vb, &mut shuf);
        if e % 150 == 0 || e + 1 == epochs {
            curve.push((e, loss));
        }
    }
    let correct = pts.iter().filter(|p| net.correct(p)).count();
    let acc = correct as f32 / pts.len() as f32;
    (net, curve, loss, acc)
}

// ── The frame ───────────────────────────────────────────────────────────────

/// The data window.
const WIN: (f32, f32, f32, f32) = (326.0, 84.0, 620.0, 556.0); // x, y, w, h

pub fn frame(t: f32) -> WidgetNode {
    let epochs = ((clamp01(t) * EPOCHS as f32).round() as usize).max(1);
    let (net, curve, loss, acc) = replay(epochs);
    let pts = dataset();

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;
            let (wx, wy, ww, wh) = WIN;

            // The ground — the training room.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(6, 6, 10)),
                    (1.0, Color::rgb(11, 11, 15)),
                ]),
            );

            // ── The decision field — a 128×72 lattice of forward passes ──
            // Each cell asks the network live: violet says +1, amber −1,
            // and the brightness is the network's confidence.
            let (fx, fy, fw, fh) = (wx, wy, ww, wh);
            let cols = 128usize;
            let rows = 72usize;
            let mut h1 = vec![0.0_f32; L1];
            let mut h2 = vec![0.0_f32; L2];
            for ry in 0..rows {
                for cx in 0..cols {
                    let px = (cx as f32 + 0.5) / cols as f32 * 2.0 - 1.0;
                    let py = 1.0 - (ry as f32 + 0.5) / rows as f32 * 2.0;
                    let out = net.forward(px, py, &mut h1, &mut h2);
                    let conf = (out * 1.6).clamp(-1.0, 1.0);
                    let col = if conf >= 0.0 {
                        mix(Color::rgb(14, 11, 24), mix(VIOLET, VIOLET_SOFT, 0.4), conf)
                    } else {
                        mix(Color::rgb(20, 14, 9), mix(AMBER, tint(AMBER, 0.4), -conf), -conf)
                    };
                    let cw = fw / cols as f32;
                    let chh = fh / rows as f32;
                    book.rect(
                        Rect::new(
                            fx + cx as f32 * cw,
                            fy + ry as f32 * chh,
                            fx + (cx + 1) as f32 * cw + 0.5,
                            fy + (ry + 1) as f32 * chh + 0.5,
                        ),
                        col,
                    );
                }
            }

            // The field's frame.
            book.stroke_rrect(
                Rect::new(fx - 1.0, fy - 1.0, fx + fw + 1.0, fy + fh + 1.0),
                4.0,
                alpha(Color::WHITE, 0.10),
                1.0,
            );

            // ── The data — the two spirals, mistakes ringed ─────────────
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                for p in pts.iter() {
                    let x = fx + (p.x * 0.5 + 0.5) * fw;
                    let y = fy + (0.5 - p.y * 0.5) * fh;
                    let right = p.label > 0.0;
                    let col = if right {
                        tint(VIOLET_SOFT, 0.55)
                    } else {
                        tint(AMBER, 0.55)
                    };
                    g.circle(Offset::new(x, y), 3.0, alpha(col, 0.95));
                    if !net.correct(p) {
                        // The mistake ring — visible while it lasts.
                        g.ring(Offset::new(x, y), 6.0, 1.4, alpha(Color::WHITE, 0.85));
                    }
                }
            });

            // ── The loss curve — the replay's own log ───────────────────
            const L_X: f32 = 1000.0;
            const L_Y: f32 = 120.0;
            const L_W: f32 = 200.0;
            const L_H: f32 = 150.0;
            book.rrect(
                Rect::new(L_X - 16.0, L_Y - 30.0, L_X + L_W + 16.0, L_Y + L_H + 16.0),
                10.0,
                alpha(Color::rgb(12, 12, 17), 0.92),
            );
            if curve.len() > 1 {
                let max_l = curve.iter().map(|&(_, l)| l).fold(0.2_f32, f32::max);
                let mut path = Path::new();
                for (i, &(e, l)) in curve.iter().enumerate() {
                    let x = L_X + e as f32 / EPOCHS as f32 * L_W;
                    let y = L_Y + L_H * (1.0 - (l / max_l).clamp(0.0, 1.0));
                    if i == 0 {
                        path.move_to(Offset::new(x, y));
                    } else {
                        path.line_to(Offset::new(x, y));
                    }
                }
                book.stroke(path, alpha(tint(CYAN, 0.3), 0.95), 1.6);
                // The rider at the current epoch.
                if let Some(&(_, l)) = curve.last() {
                    let y = L_Y + L_H * (1.0 - (l / max_l).clamp(0.0, 1.0));
                    book.circle(Offset::new(L_X + L_W, y), 2.6, alpha(tint(CYAN, 0.4), 0.95));
                }
            }

            // ── The network diagram — the actual architecture ──────────
            const N_X: f32 = 76.0;
            const N_Y: f32 = 330.0;
            const N_H: f32 = 260.0;
            let layer_x = |l: usize| N_X + l as f32 * 74.0;
            let node_y = |l: usize, i: usize| -> f32 {
                let n = [L0, L1, L2, L3][l];
                N_Y + N_H * (i as f32 + 0.5) / n as f32
            };
            // Edges: sample the weight matrix — the strongest few per layer
            // pair, coloured by sign (violet +, amber −).
            let strongest = |w: &[f32], _rows: usize, _cols: usize, take: usize| -> Vec<usize> {
                let mut idx: Vec<usize> = (0..w.len()).collect();
                idx.sort_by(|&a, &b| w[b].abs().partial_cmp(&w[a].abs()).unwrap());
                idx.truncate(take);
                idx
            };
            for &e in strongest(&net.w2, L2, L1, 26).iter() {
                let (r, c) = (e / L1, e % L1);
                let positive = net.w2[e] >= 0.0;
                book.line(
                    Offset::new(layer_x(1), node_y(1, c)),
                    Offset::new(layer_x(2), node_y(2, r)),
                    alpha(if positive { VIOLET } else { AMBER }, 0.30),
                    1.0,
                );
            }
            for &e in strongest(&net.w3, L3, L2, 8).iter() {
                let (r, c) = (e / L2, e % L2);
                let positive = net.w3[e] >= 0.0;
                book.line(
                    Offset::new(layer_x(2), node_y(2, c)),
                    Offset::new(layer_x(3), node_y(3, r)),
                    alpha(if positive { VIOLET } else { AMBER }, 0.4),
                    1.2,
                );
            }
            for l in 0..4 {
                let n = [L0, L1, L2, L3][l];
                for i in 0..n {
                    book.circle(
                        Offset::new(layer_x(l), node_y(l, i)),
                        if l == 0 || l == 3 { 4.4 } else { 3.2 },
                        alpha(mix(MUTED, INK, 0.55), 0.95),
                    );
                }
            }
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack = stack.push(receipt_panel(epochs, loss, acc));
    stack.into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(epochs: usize, loss: f32, acc: f32) -> WidgetNode {
    let lines = [
        "NEURAL · THE LEARNING AXIS · TWO SPIRALS, ONE REPLAY".to_string(),
        format!(
            "MLP 2→{}→{}→1 · tanh · mini-batch {BATCH}, lr {LR}, momentum {MOMENTUM} · epoch {epochs}/{EPOCHS}",
            L1, L2
        ),
        format!(
            "loss {loss:.4} (logistic, measured on the drawn points) · accuracy {:.1}% (measured)",
            acc * 100.0
        ),
        "decision field: 128×72 forward passes per frame — the mind, changing".to_string(),
        "the plateau, then the click: the loss holds ≈ln2, then falls to ~0 as the spirals resolve".to_string(),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 42.0;

    let mut stack = Stack::new();
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 16.0)
                .width(860.0)
                .height(15.0)
                .child(
                    Text::new(line.clone()).style(
                        TextStyle::new(if i == 0 { 12.0 } else { 11.0 })
                            .monospace()
                            .letter_spacing(if i == 0 { 1.8 } else { 0.0 })
                            .color(alpha(if i == 0 { MUTED } else { mix(MUTED, INK, 0.4) }, 0.95)),
                    ),
                ),
        );
    }

    stack.into()
}
