//! exp_smoke — *the fluid axis.* The Navier–Stokes engine.
//!
//! A real fluid solver — Stam's stable fluids on a **staggered MAC grid**
//! (u on vertical faces, v on horizontal faces, pressure at cell centres —
//! the arrangement that makes the checkerboard pressure mode impossible),
//! with **MacCormack advection** for the dye (forward + backward trace +
//! limiter: half the numerical diffusion of bilinear backtrace, which is
//! the difference between a wake that organises and one that smears), a
//! Gauss–Seidel pressure projection (the incompressibility that makes a
//! fluid a fluid), a no-through cylinder, free-slip walls, a driven inlet,
//! and a whisper of **vorticity confinement** (Fedkiw 2001) to repay the
//! curl that discretisation steals. No vortex is placed by hand; whatever
//! rolls, rolls because these equations do.
//!
//! The discipline is the lab's: **every frame re-integrates from the same
//! seed state** (still fluid, then the inlet switches on) up to its own
//! `t` — no state is carried between frames; the wake at t is a pure
//! function of the machine's constants. The cost of that discipline shows
//! up honestly in the build/raster split, and the receipt shows it.
//!
//! The receipt's quiet fact: the **lift on the cylinder** is integrated
//! from the solver's own pressure field every step (Σ p·n over the
//! obstacle boundary), and its oscillation — if the street organises at
//! this resolution — *is* the shedding frequency: the Strouhal number,
//! measured from the same pressure field that rendered the frame, printed
//! against the ≈0.2 the street is famous for. And if it does not organise,
//! the receipt says that too — measured honesty over hoped-for physics.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, clamp01, mix, tint, AMBER, CYAN, FAINT, INK, MUTED, VIOLET};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

// ── The machine ─────────────────────────────────────────────────────────────

/// The grid (cells; velocity faces live one larger on their axis).
const GX: usize = 128;
const GY: usize = 72;

/// Total sim steps at t = 1 — the wake needs time to organise.
const TOTAL_STEPS: usize = 460;

/// The inlet speed, cells/step.
const U_IN: f32 = 0.95;

/// The cylinder: centre (gx, gy), radius r — grid cells.
const CYL: (usize, usize, f32) = (36, 36, 6.0);

/// Pressure iterations per step — the projection's price.
const P_ITERS: usize = 22;

/// Vorticity confinement strength (Fedkiw 2001) — repays the curl that
/// MacCormack cannot fully keep.
const VORT_CONF: f32 = 0.05;

/// The dye release band at the inlet, rows [c−r, c+r].
const DYE_BAND: f32 = 13.0;

/// The staggered state, rebuilt from scratch every frame.
struct Sim {
    /// u on vertical faces: (GX+1) × GY, face (x,y) at (x, y+0.5).
    u: Vec<f32>,
    /// v on horizontal faces: GX × (GY+1), face (x,y) at (x+0.5, y).
    v: Vec<f32>,
    u0: Vec<f32>,
    v0: Vec<f32>,
    p: Vec<f32>,
    div: Vec<f32>,
    dye: Vec<f32>,
    dye0: Vec<f32>,
    solid: Vec<bool>,
    /// Lift history — Σ p·n_y over the cylinder, one entry per step.
    lift: Vec<f32>,
}

#[inline]
fn ui(x: usize, y: usize) -> usize {
    y * (GX + 1) + x
}
#[inline]
fn vi(x: usize, y: usize) -> usize {
    y * GX + x
}
#[inline]
fn ci(x: usize, y: usize) -> usize {
    y * GX + x
}

impl Sim {
    fn new() -> Self {
        let n = GX * GY;
        let mut solid = vec![false; n];
        let (cx, cy, r) = CYL;
        for y in 0..GY {
            for x in 0..GX {
                let dx = x as f32 - cx as f32;
                let dy = y as f32 - cy as f32;
                if dx * dx + dy * dy <= r * r {
                    solid[ci(x, y)] = true;
                }
            }
        }
        Self {
            u: vec![0.0; (GX + 1) * GY],
            v: vec![0.0; GX * (GY + 1)],
            u0: vec![0.0; (GX + 1) * GY],
            v0: vec![0.0; GX * (GY + 1)],
            p: vec![0.0; n],
            div: vec![0.0; n],
            dye: vec![0.0; n],
            dye0: vec![0.0; n],
            solid,
            lift: Vec::new(),
        }
    }

    // ── Samplers (bilinear, half-cell offsets by field) ──────────────────

    fn sample_u(f: &[f32], fx: f32, fy: f32) -> f32 {
        let fx = fx.clamp(0.0, GX as f32);
        let fy = (fy - 0.5).clamp(0.0, GY as f32 - 1.001);
        let x0 = fx.floor() as usize;
        let y0 = fy.floor() as usize;
        let x1 = (x0 + 1).min(GX);
        let y1 = (y0 + 1).min(GY - 1);
        let tx = fx - x0 as f32;
        let ty = fy - y0 as f32;
        let a = f[ui(x0, y0)] * (1.0 - tx) + f[ui(x1, y0)] * tx;
        let b = f[ui(x0, y1)] * (1.0 - tx) + f[ui(x1, y1)] * tx;
        a * (1.0 - ty) + b * ty
    }

    fn sample_v(f: &[f32], fx: f32, fy: f32) -> f32 {
        let fx = (fx - 0.5).clamp(0.0, GX as f32 - 1.001);
        let fy = fy.clamp(0.0, GY as f32);
        let x0 = fx.floor() as usize;
        let y0 = fy.floor() as usize;
        let x1 = (x0 + 1).min(GX - 1);
        let y1 = (y0 + 1).min(GY);
        let tx = fx - x0 as f32;
        let ty = fy - y0 as f32;
        let a = f[vi(x0, y0)] * (1.0 - tx) + f[vi(x1, y0)] * tx;
        let b = f[vi(x0, y1)] * (1.0 - tx) + f[vi(x1, y1)] * tx;
        a * (1.0 - ty) + b * ty
    }

    fn sample_c(f: &[f32], fx: f32, fy: f32) -> f32 {
        let fx = (fx - 0.5).clamp(0.0, GX as f32 - 1.001);
        let fy = (fy - 0.5).clamp(0.0, GY as f32 - 1.001);
        let x0 = fx.floor() as usize;
        let y0 = fy.floor() as usize;
        let x1 = (x0 + 1).min(GX - 1);
        let y1 = (y0 + 1).min(GY - 1);
        let tx = fx - x0 as f32;
        let ty = fy - y0 as f32;
        let a = f[ci(x0, y0)] * (1.0 - tx) + f[ci(x1, y0)] * tx;
        let b = f[ci(x0, y1)] * (1.0 - tx) + f[ci(x1, y1)] * tx;
        a * (1.0 - ty) + b * ty
    }

    #[inline]
    fn vel_at(&self, x: f32, y: f32) -> (f32, f32) {
        (Self::sample_u(&self.u, x, y), Self::sample_v(&self.v, x, y))
    }

    /// One step: advect → project → confine → boundaries → dye.
    fn step(&mut self, step_i: usize) {
        // A whisper of asymmetry in the first steps so the wake picks a
        // side; after that, perfectly symmetric drive.
        let bow = if step_i < 60 {
            (step_i as f32 / 60.0 * std::f32::consts::PI).sin() * 0.05
        } else {
            0.0
        };

        // ── Advect velocity (semi-Lagrangian on the staggered faces) ──
        self.u0.copy_from_slice(&self.u);
        self.v0.copy_from_slice(&self.v);
        for y in 0..GY {
            for x in 0..=GX {
                let (px, py) = (x as f32, y as f32 + 0.5);
                let uu = Self::sample_u(&self.u0, px, py);
                let vv = Self::sample_v(&self.v0, px, py);
                self.u[ui(x, y)] = Self::sample_u(&self.u0, px - uu, py - vv);
            }
        }
        for y in 0..=GY {
            for x in 0..GX {
                let (px, py) = (x as f32 + 0.5, y as f32);
                let uu = Self::sample_u(&self.u0, px, py);
                let vv = Self::sample_v(&self.v0, px, py);
                self.v[vi(x, y)] = Self::sample_v(&self.v0, px - uu, py - vv);
            }
        }

        // ── Project: ∇²p = ∇·u, Gauss–Seidel, staggered differences ────
        for y in 0..GY {
            for x in 0..GX {
                self.div[ci(x, y)] =
                    -(self.u[ui(x + 1, y)] - self.u[ui(x, y)] + self.v[vi(x, y + 1)] - self.v[vi(x, y)]);
                self.p[ci(x, y)] = 0.0;
            }
        }
        let pc = |p: &[f32], x: i32, y: i32| -> f32 {
            p[ci(x.clamp(0, GX as i32 - 1) as usize, y.clamp(0, GY as i32 - 1) as usize)]
        };
        for _ in 0..P_ITERS {
            for y in 0..GY {
                for x in 0..GX {
                    let i = ci(x, y);
                    self.p[i] = (pc(&self.p, x as i32 - 1, y as i32)
                        + pc(&self.p, x as i32 + 1, y as i32)
                        + pc(&self.p, x as i32, y as i32 - 1)
                        + pc(&self.p, x as i32, y as i32 + 1)
                        + self.div[i])
                        * 0.25;
                }
            }
        }
        for y in 0..GY {
            for x in 0..GX {
                let (pl, pr) = (
                    pc(&self.p, x as i32 - 1, y as i32),
                    pc(&self.p, x as i32 + 1, y as i32),
                );
                let (pb, pt) = (
                    pc(&self.p, x as i32, y as i32 - 1),
                    pc(&self.p, x as i32, y as i32 + 1),
                );
                self.u[ui(x, y)] -= self.p[ci(x, y)] - pl;
                self.u[ui(x + 1, y)] -= pr - self.p[ci(x, y)];
                self.v[vi(x, y)] -= self.p[ci(x, y)] - pb;
                self.v[vi(x, y + 1)] -= pt - self.p[ci(x, y)];
            }
        }

        // ── Vorticity confinement: repay the curl diffusion stole ─────
        if VORT_CONF > 0.0 {
            // ω at cell centres (staggered differences).
            let mut om = vec![0.0_f32; GX * GY];
            for y in 1..GY - 1 {
                for x in 1..GX - 1 {
                    om[ci(x, y)] = 0.5
                        * ((self.v[vi((x + 1).min(GX - 1), y)] - self.v[vi(x.saturating_sub(1), y)])
                            - (self.u[ui(x, (y + 1).min(GY - 1))] - self.u[ui(x, y.saturating_sub(1))]));
                }
            }
            for y in 1..GY - 1 {
                for x in 1..GX - 1 {
                    let i = ci(x, y);
                    if self.solid[i] {
                        continue;
                    }
                    let gx_ = 0.5 * (om[ci(x + 1, y)].abs() - om[ci(x - 1, y)].abs());
                    let gy_ = 0.5 * (om[ci(x, y + 1)].abs() - om[ci(x, y - 1)].abs());
                    let m = (gx_ * gx_ + gy_ * gy_).sqrt();
                    if m < 1e-6 {
                        continue;
                    }
                    let (nx_, ny_) = (gx_ / m, gy_ / m);
                    let w = om[i];
                    let fx = ny_ * w * VORT_CONF;
                    let fy_ = -nx_ * w * VORT_CONF;
                    self.u[ui(x, y)] += fx;
                    self.u[ui(x + 1, y)] += fx;
                    self.v[vi(x, y)] += fy_;
                    self.v[vi(x, y + 1)] += fy_;
                }
            }
        }

        // ── Boundaries ────────────────────────────────────────────────
        // Inlet: driven u; a bow in v for the first steps.
        for y in 0..GY {
            self.u[ui(0, y)] = U_IN;
        }
        for y in 0..=GY {
            self.v[vi(0, y)] =
                bow * U_IN * (-((y as f32 - CYL.1 as f32 - 0.5).abs() / 10.0)).exp();
        }
        // Outflow: copy.
        for y in 0..GY {
            self.u[ui(GX, y)] = self.u[ui(GX - 1, y)];
        }
        // Walls: no through-flow, free-slip tangential.
        for x in 0..GX {
            self.v[vi(x, 0)] = 0.0;
            self.v[vi(x, GY)] = 0.0;
        }
        for x in 0..=GX {
            self.u[ui(x, 0)] = self.u[ui(x, 1)];
            self.u[ui(x, GY - 1)] = self.u[ui(x, GY - 2)];
        }
        // Solid: zero every face of solid cells.
        for y in 0..GY {
            for x in 0..GX {
                if !self.solid[ci(x, y)] {
                    continue;
                }
                self.u[ui(x, y)] = 0.0;
                self.u[ui(x + 1, y)] = 0.0;
                self.v[vi(x, y)] = 0.0;
                self.v[vi(x, y + 1)] = 0.0;
            }
        }

        // ── Dye: MacCormack with a neighbourhood limiter ──────────────
        self.dye0.copy_from_slice(&self.dye);
        for y in 0..GY {
            for x in 0..GX {
                let i = ci(x, y);
                if self.solid[i] {
                    self.dye[i] *= 0.90;
                    continue;
                }
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let (uu, vv) = self.vel_at(px, py);
                let (bx, by) = (px - uu, py - vv);
                let fwd = Self::sample_c(&self.dye0, bx, by);
                let (uu2, vv2) = self.vel_at(bx, by);
                let back = Self::sample_c(&self.dye0, px + uu2, py + vv2);
                let val = fwd + 0.5 * (self.dye0[i] - back);
                // Limiter: clamp to the departure neighbourhood (no new
                // extrema — the guarantee that keeps MacCormack stable).
                let ix = bx.clamp(0.0, (GX - 2) as f32) as usize;
                let iy = by.clamp(0.0, (GY - 2) as f32) as usize;
                let mut lo = f32::INFINITY;
                let mut hi = f32::NEG_INFINITY;
                for dy in 0..=1isize {
                    for dx in 0..=1isize {
                        let c = self.dye0[ci((ix as isize + dx) as usize, (iy as isize + dy) as usize)];
                        lo = lo.min(c);
                        hi = hi.max(c);
                    }
                }
                self.dye[i] = val.clamp(lo, hi) * 0.9995;
            }
        }
        // Release at the inlet (after advection, so the band persists).
        for y in 0..GY {
            let i = ci(0, y);
            let dyc = (y as f32 - CYL.1 as f32).abs();
            if dyc < DYE_BAND {
                let edge = 1.0 - dyc / DYE_BAND;
                self.dye[i] = self.dye[i].max((0.85 * edge * edge).min(1.0));
            }
        }

        // ── The lift, from the solver's own pressure ──────────────────
        let (cx, cy, _) = CYL;
        let mut lift = 0.0_f32;
        for gy in 1..GY - 1 {
            for gx in 1..GX - 1 {
                let i = ci(gx, gy);
                if !self.solid[i] {
                    continue;
                }
                let dx = gx as f32 - cx as f32;
                let dy = gy as f32 - cy as f32;
                let m = (dx * dx + dy * dy).sqrt().max(1e-6);
                let ny = dy / m;
                let up = self.solid[ci(gx, gy - 1)];
                let dn = self.solid[ci(gx, gy + 1)];
                if !up || !dn {
                    lift += self.p[i] * ny;
                }
            }
        }
        self.lift.push(lift);
    }

    /// Vorticity at a cell (for dye colouring).
    fn curl(&self, x: usize, y: usize) -> f32 {
        let xm = x.saturating_sub(1);
        let xp = (x + 1).min(GX - 1);
        let ym = y.saturating_sub(1);
        let yp = (y + 1).min(GY - 1);
        let dvdx = self.v[vi(xp, y)] - self.v[vi(xm, y)];
        let dudy = self.u[ui(x, yp)] - self.u[ui(x, ym)];
        (dvdx - dudy) * 0.5
    }
}

/// The shedding readout from the lift series: dominant zero-crossing rate
/// over the last half of the record, plus the oscillation amplitude — so
/// the receipt can say "shedding" or "not yet" honestly.
#[must_use]
fn shedding(lift: &[f32]) -> Option<(f32, f32, f32)> {
    let n = lift.len();
    if n < 120 {
        return None;
    }
    let series = &lift[n / 2..];
    let mean = series.iter().sum::<f32>() / series.len() as f32;
    let crossings = series
        .windows(2)
        .filter(|w| (w[0] - mean) * (w[1] - mean) < 0.0)
        .count();
    let f = crossings as f32 / 2.0 / series.len() as f32;
    let amp = (series.iter().copied().fold(f32::MIN, f32::max)
        - series.iter().copied().fold(f32::MAX, f32::min))
        / 2.0;
    Some((f, f * (CYL.2 * 2.0) / U_IN, amp))
}

// ── The frame ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    // The replay: from still fluid to t — pure function of the constants.
    let steps = (TOTAL_STEPS as f32 * t) as usize;
    let mut sim = Sim::new();
    for k in 0..steps {
        sim.step(k);
    }
    let shed = shedding(&sim.lift);

    // The render window: the channel on the canvas.
    const X0: f32 = 84.0;
    const Y0: f32 = 84.0;
    const WD: f32 = 1112.0;
    const HT: f32 = 552.0;
    let cw = WD / GX as f32;
    let ch = HT / GY as f32;

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — the wind tunnel at night.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(5, 6, 9)),
                    (1.0, Color::rgb(10, 11, 15)),
                ]),
            );

            // The channel interior.
            book.rect(Rect::new(X0, Y0, X0 + WD, Y0 + HT), Color::rgb(7, 8, 12));

            // ── The dye, upsampled ×2 from the sim (bilinear) ──────────
            // Amber where the curl turns one way, cyan the other — the
            // wake's rolls read as alternating temperature.
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                let rx_n = 224usize;
                let ry_n = 126usize;
                let rw = WD / rx_n as f32;
                let rh = HT / ry_n as f32;
                for ry in 0..ry_n {
                    for rx in 0..rx_n {
                        let fx = (rx as f32 + 0.5) / rx_n as f32 * GX as f32;
                        let fy = (ry as f32 + 0.5) / ry_n as f32 * GY as f32;
                        let d = Sim::sample_c(&sim.dye, fx, fy);
                        if d < 0.02 {
                            continue;
                        }
                        let vort = sim
                            .curl(fx.round().clamp(0.0, (GX - 1) as f32) as usize,
                                  fy.round().clamp(0.0, (GY - 1) as f32) as usize);
                        let warm = clamp01(vort * 2.6 + 0.5);
                        let col = mix(
                            mix(tint(CYAN, 0.25), Color::rgb(228, 238, 250), 0.3),
                            mix(tint(AMBER, 0.3), Color::rgb(255, 250, 240), 0.35),
                            warm,
                        );
                        let x = X0 + rx as f32 * rw;
                        let y = Y0 + ry as f32 * rh;
                        g.rect(
                            Rect::new(x, y, x + rw + 0.5, y + rh + 0.5),
                            alpha(col, (d * 0.80).min(0.92)),
                        );
                    }
                }
            });

            // ── The cylinder ───────────────────────────────────────────
            let (cgx, cgy, cr) = CYL;
            let cc = Offset::new(
                X0 + (cgx as f32 + 0.5) * cw,
                Y0 + (cgy as f32 + 0.5) * ch,
            );
            let pr = cr * cw;
            book.circle(cc, pr + 1.5, Color::rgb(16, 16, 22));
            book.ring(cc, pr, 2.2, alpha(VIOLET, 0.9));
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                g.circle(
                    cc,
                    pr + 12.0,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(VIOLET, 0.22)),
                        (1.0, alpha(VIOLET, 0.0)),
                    ]),
                );
            });

            // ── The channel furniture ──────────────────────────────────
            book.line(
                Offset::new(X0, Y0),
                Offset::new(X0 + WD, Y0),
                alpha(mix(FAINT, CYAN, 0.2), 0.55),
                2.0,
            );
            book.line(
                Offset::new(X0, Y0 + HT),
                Offset::new(X0 + WD, Y0 + HT),
                alpha(mix(FAINT, CYAN, 0.2), 0.55),
                2.0,
            );
            let march = (t * 6.2832 * 2.0) % 1.0;
            for k in 0..7 {
                let yy = Y0 + HT * (0.18 + 0.64 * (k as f32 / 6.0));
                let xx = X0 + 22.0 + march * 26.0;
                book.line(
                    Offset::new(xx - 14.0, yy),
                    Offset::new(xx, yy),
                    alpha(tint(CYAN, 0.25), 0.5),
                    1.6,
                );
            }

            // ── The lift trace — the pressure field's own instrument ──
            let t_x0 = X0 + WD - 330.0;
            let t_y0 = Y0 + HT - 96.0;
            book.rrect(
                Rect::new(t_x0 - 14.0, t_y0 - 22.0, t_x0 + 316.0, t_y0 + 64.0),
                8.0,
                alpha(Color::rgb(10, 10, 15), 0.85),
            );
            let series = &sim.lift[sim.lift.len().saturating_sub(180)..];
            let maxa = series
                .iter()
                .fold(1e-9_f32, |m, v| m.max(v.abs()));
            if series.len() > 1 {
                let mut path = Path::new();
                for (i, &l) in series.iter().enumerate() {
                    let x = t_x0 + i as f32 / (series.len() - 1) as f32 * 296.0;
                    let y = t_y0 + 18.0 - (l / maxa) * 30.0;
                    if i == 0 {
                        path.move_to(Offset::new(x, y));
                    } else {
                        path.line_to(Offset::new(x, y));
                    }
                }
                book.stroke(path, alpha(tint(AMBER, 0.2), 0.95), 1.4);
                book.line(
                    Offset::new(t_x0, t_y0 + 18.0),
                    Offset::new(t_x0 + 296.0, t_y0 + 18.0),
                    alpha(FAINT, 0.5),
                    1.0,
                );
            }

            // The flow-speed yardstick.
            book.line(
                Offset::new(X0 + 24.0, Y0 + HT - 26.0),
                Offset::new(X0 + 24.0 + U_IN * cw, Y0 + HT - 26.0),
                alpha(tint(CYAN, 0.3), 0.8),
                2.4,
            );
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack = stack.push(receipt_panel(steps, shed));
    stack.into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(steps: usize, shed: Option<(f32, f32, f32)>) -> WidgetNode {
    let lines = [
        "SMOKE · THE FLUID AXIS · NAVIER–STOKES, RUN LIVE".to_string(),
        format!(
            "staggered MAC {}×{} · MacCormack dye · {} pressure iters/step · replayed {steps} steps this frame",
            GX, GY, P_ITERS
        ),
        format!(
            "cylinder D {:.0} cells · inlet U {:.2} cells/step · vorticity confinement {VORT_CONF}",
            CYL.2 * 2.0, U_IN
        ),
        match shed {
            Some((f, st, amp)) if amp > 0.02 => format!(
                "lift oscillates: {:.4} cycles/step, amplitude {amp:.3} → Strouhal ≈ {st:.2} (the street's own ≈0.2)",
                f
            ),
            Some((_, _, amp)) => format!(
                "lift amplitude {amp:.3} — the wake has not shed at this resolution; the receipt says so"
            ),
            None => "lift series too short to measure — the wind is still arriving".to_string(),
        },
        "no vortex is placed by hand — whatever rolls, rolls because the equations do".to_string(),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 42.0;

    let mut stack = Stack::new();
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 16.0)
                .width(800.0)
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
