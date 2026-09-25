//! exp_turing — *the morphogenesis axis.* The chemistry that draws.
//!
//! Alan Turing's 1952 answer to how a uniform embryo becomes a patterned
//! one: two reacting chemicals, one diffusing faster than the other, is
//! enough to break symmetry. This plate runs the **Gray–Scott** system —
//! the canonical morphogenesis model — on a 128×72 lattice, five-point
//! Laplacian, fixed dt, **re-simulated from the same seed every frame up
//! to its own t** (the replay discipline: a pattern at t is a pure
//! function of the seed and the schedule, nothing carried between frames).
//!
//! The machine walks a parameter tour: four named regimes of the (F, k)
//! plane — mitosis (spots that divide), coral (solitons that negotiate
//! spacing), labyrinth (stripes that eat the plane), and decay (the
//! pattern dies back to uniform). Between regimes the parameters ease
//! smoothly, and the pattern *reorganises* — dissolve, wander, re-lattice:
//! morphogenesis, watched at laboratory speed.
//!
//! The receipt's instrument: the **pattern wavelength, measured from the
//! final field** — the autocorrelation of the v concentration along the
//! mid-row, first positive peak after zero lag. Turing's claim is that
//! the wavelength is set by the diffusion constants, not by the seed; the
//! number is read off the field that drew the frame.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, clamp01, mix, smoothstep, tint, AMBER, CYAN, FAINT, INK, MUTED,
    VIOLET};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

// ── The chemistry ───────────────────────────────────────────────────────────

/// The lattice.
const GX: usize = 128;
const GY: usize = 72;

/// Diffusion rates (v is the faster — Turing's asymmetry, the whole point).
const DU: f32 = 0.160;
const DV: f32 = 0.080;

/// Integration dt and the replay budget at t = 1.
const DT: f32 = 1.0;
const TOTAL_STEPS: usize = 4400;

/// The regime tour: (name, F, k) — dwell points on the Gray–Scott map,
/// each one a canonical, named region of the standard parameter plane.
const REGIMES: [(&str, f32, f32); 5] = [
    ("mitosis", 0.0367, 0.0649),
    ("coral", 0.0545, 0.0620),
    ("labyrinth", 0.0290, 0.0570),
    ("worms", 0.0780, 0.0610),
    ("decay", 0.0900, 0.0590),
];

/// The (F, k) schedule at sim-time fraction s: dwell 70% per regime, ease
/// 30% between neighbours (smoothstep), so the pattern has time to be a
/// regime and then time to become the next one.
#[must_use]
fn schedule(s: f32) -> (&'static str, f32, f32) {
    let n = REGIMES.len() as f32;
    let x = clamp01(s) * (n - 1.0);
    let i = x.floor() as usize;
    let f = x - i as f32;
    let (blended, live_i) = if f < 0.70 || i + 1 >= REGIMES.len() {
        (false, i)
    } else {
        (true, i)
    };
    if !blended {
        let (name, ff, k) = REGIMES[live_i];
        (name, ff, k)
    } else {
        let w = smoothstep((f - 0.70) / 0.30);
        let (_, f0, k0) = REGIMES[i];
        let (name1, f1, k1) = REGIMES[i + 1];
        // Name carries the source regime until the blend is half done.
        let name = if w < 0.5 { REGIMES[i].0 } else { name1 };
        (name, f0 * (1.0 - w) + f1 * w, k0 * (1.0 - w) + k1 * w)
    }
}

/// The Gray–Scott state, re-seeded and re-run every frame.
struct Field {
    u: Vec<f32>,
    v: Vec<f32>,
    u0: Vec<f32>,
    v0: Vec<f32>,
}

impl Field {
    fn seeded() -> Self {
        let n = GX * GY;
        let mut u = vec![1.0; n];
        let mut v = vec![0.0; n];
        // The seed: three u-holes with v — the symmetry that breaks.
        let mut rng = crate::film_lib::Rng::new(0x70C5);
        let blobs = [(0.30, 0.42), (0.52, 0.60), (0.68, 0.35)];
        for &(bx, by) in blobs.iter() {
            let cx = (bx * GX as f32) as usize;
            let cy = (by * GY as f32) as usize;
            for dy in -3i32..=3 {
                for dx in -3i32..=3 {
                    let x = (cx as i32 + dx).clamp(0, GX as i32 - 1) as usize;
                    let y = (cy as i32 + dy).clamp(0, GY as i32 - 1) as usize;
                    let i = y * GX + x;
                    u[i] = 0.35 + rng.f01() * 0.15;
                    v[i] = 0.30 + rng.f01() * 0.15;
                }
            }
        }
        Self {
            u,
            v,
            u0: vec![0.0; n],
            v0: vec![0.0; n],
        }
    }

    /// The five-point Laplacian with reflecting edges.
    fn lap(f: &[f32], x: usize, y: usize) -> f32 {
        let l = f[y * GX + x.saturating_sub(1)];
        let r = f[y * GX + (x + 1).min(GX - 1)];
        let d = f[y.saturating_sub(1) * GX + x];
        let u_ = f[(y + 1).min(GY - 1) * GX + x];
        let c = f[y * GX + x];
        l + r + d + u_ - 4.0 * c
    }

    fn step(&mut self, feed: f32, kill: f32) {
        self.u0.copy_from_slice(&self.u);
        self.v0.copy_from_slice(&self.v);
        for y in 0..GY {
            for x in 0..GX {
                let i = y * GX + x;
                let u = self.u0[i];
                let v = self.v0[i];
                let uvv = u * v * v;
                let du = DU * Self::lap(&self.u0, x, y) - uvv + feed * (1.0 - u);
                let dv = DV * Self::lap(&self.v0, x, y) + uvv - (feed + kill) * v;
                self.u[i] = (u + DT * du).clamp(0.0, 1.0);
                self.v[i] = (v + DT * dv).clamp(0.0, 1.0);
            }
        }
    }
}

/// Replay the chemistry to film-fraction t. Returns the field, the number
/// of steps, and the schedule's live regime.
#[must_use]
fn replay(t: f32) -> (Field, usize, &'static str, f32, f32) {
    let steps = (TOTAL_STEPS as f32 * clamp01(t)) as usize;
    let mut f = Field::seeded();
    let mut live = REGIMES[0].0;
    let (mut feed, mut kill) = (REGIMES[0].1, REGIMES[0].2);
    for k in 0..steps {
        let s = (k as f32 + 1.0) / TOTAL_STEPS as f32;
        let (name, f_, k_) = schedule(s);
        live = name;
        feed = f_;
        kill = k_;
        f.step(f_, k_);
    }
    (f, steps, live, feed, kill)
}

/// The pattern wavelength, measured: autocorrelation of v along the
/// mid-row, first strong positive peak after zero lag. Cells, then px.
#[must_use]
fn wavelength(field: &Field) -> Option<f32> {
    let y = GY / 2;
    let row: Vec<f32> = (0..GX).map(|x| field.v[y * GX + x]).collect();
    let mean = row.iter().sum::<f32>() / row.len() as f32;
    let centred: Vec<f32> = row.iter().map(|&v| v - mean).collect();
    let energy: f32 = centred.iter().map(|c| c * c).sum();
    if energy < 1e-4 {
        return None; // uniform — no pattern to measure
    }
    // Correlate to lag 48; first positive peak with value > 0.25.
    for lag in 6..48 {
        let mut acc = 0.0_f32;
        for x in 0..GX - lag {
            acc += centred[x] * centred[x + lag];
        }
        let c = acc / energy;
        if c > 0.25 {
            // Confirm it's a local peak (check lag−1, lag+1 coarsely).
            let mut acc_p = 0.0_f32;
            for x in 0..GX - lag - 1 {
                acc_p += centred[x] * centred[x + lag + 1];
            }
            let cp = acc_p / energy;
            if cp < c {
                return Some(lag as f32);
            }
        }
    }
    None
}

// ── The frame ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    let (field, steps, live, feed, kill) = replay(t);
    let lam = wavelength(&field);
    // Mean v — the census of how much pattern exists at all.
    let mean_v = field.v.iter().sum::<f32>() / field.v.len() as f32;

    // The render window.
    const X0: f32 = 176.0;
    const Y0: f32 = 92.0;
    const WD: f32 = 928.0;
    const HT: f32 = 522.0;
    let cw = WD / GX as f32;
    let ch = HT / GY as f32;

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — the dish's room.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(6, 6, 9)),
                    (1.0, Color::rgb(11, 11, 15)),
                ]),
            );

            // The dish: a petri plate rim.
            book.rrect(
                Rect::new(X0 - 14.0, Y0 - 14.0, X0 + WD + 14.0, Y0 + HT + 14.0),
                16.0,
                alpha(Color::rgb(30, 30, 38), 0.9),
            );

            // ── The field: v as colour, u as depth ─────────────────────
            // The ramp: dark ground → violet mid → amber ridges, the
            // pattern's phase readable at a glance.
            for gy in 0..GY {
                for gx in 0..GX {
                    let v = field.v[gy * GX + gx];
                    let u = field.u[gy * GX + gx];
                    if v < 0.012 && u > 0.98 {
                        // Pristine u-ground: near-black, drawn cheap.
                        book.rect(
                            Rect::new(
                                X0 + gx as f32 * cw,
                                Y0 + gy as f32 * ch,
                                X0 + (gx + 1) as f32 * cw + 0.4,
                                Y0 + (gy + 1) as f32 * ch + 0.4,
                            ),
                            Color::rgb(9, 9, 13),
                        );
                        continue;
                    }
                    let col = if v < 0.18 {
                        mix(Color::rgb(14, 12, 22), VIOLET_DEEPISH, v / 0.18)
                    } else if v < 0.34 {
                        mix(VIOLET_DEEPISH, VIOLET, (v - 0.18) / 0.16)
                    } else {
                        mix(VIOLET, tint(AMBER, 0.35), ((v - 0.34) / 0.28).min(1.0))
                    };
                    book.rect(
                        Rect::new(
                            X0 + gx as f32 * cw,
                            Y0 + gy as f32 * ch,
                            X0 + (gx + 1) as f32 * cw + 0.4,
                            Y0 + (gy + 1) as f32 * ch + 0.4,
                        ),
                        alpha(col, 0.92),
                    );
                }
            }

            // The rim's highlight.
            book.stroke_rrect(
                Rect::new(X0 - 14.0, Y0 - 14.0, X0 + WD + 14.0, Y0 + HT + 14.0),
                16.0,
                alpha(tint(CYAN, 0.15), 0.25),
                1.0,
            );

            // ── The (F, k) tour map — a small instrument at the left ───
            // The five dwell points on a tiny F-k plot, the current point
            // riding the path that the schedule actually walked.
            const M_X: f32 = 44.0;
            const M_Y: f32 = 180.0;
            const M_W: f32 = 104.0;
            const M_H: f32 = 150.0;
            book.rrect(
                Rect::new(M_X - 10.0, M_Y - 26.0, M_X + M_W + 10.0, M_Y + M_H + 12.0),
                8.0,
                alpha(Color::rgb(12, 12, 17), 0.92),
            );
            // Axes: F rightward [0.01, 0.09], k downward [0.050, 0.068].
            let map_pt = |f_: f32, k_: f32| -> Offset {
                Offset::new(
                    M_X + ((f_ - 0.010) / 0.080).clamp(0.0, 1.0) * M_W,
                    M_Y + ((k_ - 0.050) / 0.018).clamp(0.0, 1.0) * M_H,
                )
            };
            // The tour path.
            let mut path = Path::new();
            for (i, &(_, f_, k_)) in REGIMES.iter().enumerate() {
                let o = map_pt(f_, k_);
                if i == 0 {
                    path.move_to(o);
                } else {
                    path.line_to(o);
                }
            }
            book.stroke(path, alpha(FAINT, 0.7), 1.0);
            // The dwell points + labels.
            for &(name, f_, k_) in REGIMES.iter() {
                let o = map_pt(f_, k_);
                book.circle(o, 2.0, alpha(mix(MUTED, INK, 0.3), 0.8));
                let lit = name == live;
                if lit {
                    book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                        g.circle(
                            o,
                            10.0,
                            Gradient::radial_fill().with_dither().with_stops(&[
                                (0.0, alpha(tint(AMBER, 0.3), 0.6)),
                                (1.0, alpha(AMBER, 0.0)),
                            ]),
                        );
                    });
                }
            }
            // The live point.
            let live_pt = map_pt(feed, kill);
            book.circle(live_pt, 3.4, alpha(tint(AMBER, 0.4), 0.95));
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack = stack.push(receipt_panel(steps, live, feed, kill, lam, mean_v));
    stack.into()
}

/// A deep violet between VIOLET_DEEP and VIOLET for the ramp's mid.
const VIOLET_DEEPISH: Color = Color::rgb(94, 52, 168);

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(
    steps: usize,
    live: &str,
    feed: f32,
    kill: f32,
    lam: Option<f32>,
    mean_v: f32,
) -> WidgetNode {
    let lines = [
        "TURING · THE MORPHOGENESIS AXIS · GRAY–SCOTT".to_string(),
        format!(
            "du {DU} · dv {DV} (u diffuses {}× faster than v — Turing's asymmetry) · {} steps replayed this frame",
            DU / DV,
            steps
        ),
        format!(
            "regime: {live} · F = {:.4} · k = {:.4} · tour of {}",
            feed,
            kill,
            REGIMES.len()
        ),
        match lam {
            Some(l) => format!(
                "pattern wavelength: {:.0} cells ≈ {:.0} px, measured from the mid-row autocorrelation",
                l,
                l * (928.0 / GX as f32)
            ),
            None => "pattern wavelength: field uniform — between regimes, or decayed".to_string(),
        },
        format!("mean v = {mean_v:.3} — how much pattern exists, measured"),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 42.0;

    let mut stack = Stack::new();
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 16.0)
                .width(760.0)
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
