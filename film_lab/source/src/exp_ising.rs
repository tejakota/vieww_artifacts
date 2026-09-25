//! exp_ising — *the criticality axis.* The order that arrives all at once.
//!
//! The Ising model: a lattice of spins that want to agree with their
//! neighbours, fighting against temperature. It is the simplest machine
//! that has a **phase transition** — and this plate cools through it live:
//! Metropolis dynamics annealed from a fixed seed, **the whole cooling
//! history replayed from scratch every frame** (no state between frames —
//! the lattice at t is a pure function of the schedule and the seed).
//!
//! Watch the domains: hot chaos → islands negotiating → the magnet
//! crystallising as T crosses criticality. The receipt carries the
//! magnetisation curve **M(T) measured at every stage of the same replay
//! that drew the frame**, the crossing where order sets in, and Onsager's
//! exact T_c = 2.269 for the infinite square lattice — the theory this
//! little finite machine is reaching toward. The energy per spin is
//! measured from the same lattice; the M(T) scatter in the corner is the
//! curve, plotted by the machine as it climbs.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, clamp01, mix, tint, AMBER, CYAN, INK, MUTED,
    VIOLET_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

// ── The model ───────────────────────────────────────────────────────────────

/// The lattice (spins: +1 up, −1 down).
const GX: usize = 120;
const GY: usize = 68;

/// Coupling J (energy units where kB = 1).
const J: f32 = 1.0;

/// The anneal schedule: T starts hot, cools through criticality, lands
/// cold. The cold end stops at 0.5 — cold enough to order, warm enough
/// that domain walls still move, so the lattice actually equilibrates
/// inside the sweep budget (the first cut ran to T = 0.14 with 90
/// sweeps/stage and froze at M = 0.16: the receipt printed a magnet that
/// never magnetised).
const T_HOT: f32 = 3.0;
const T_COLD: f32 = 0.5;

/// Temperature stages — each stage equilibrates before the next.
const STAGES: usize = 14;

/// Metropolis sweeps per stage — 240 lets domains coarsen fully.
const SWEEPS: usize = 240;

/// One replay: the seed lattice, annealed through `stage_count` stages.
/// Returns the lattice, the T of each completed stage, and M at each.
#[must_use]
fn anneal(stages_done: usize) -> (Vec<i8>, Vec<(f32, f32)>) {
    assert!(stages_done >= 1);
    // Seed: a hot scramble — deterministic coin per cell.
    let mut rng = crate::film_lib::Rng::new(0x1514);
    let mut spin: Vec<i8> = (0..GX * GY).map(|_| if rng.f01() < 0.5 { 1 } else { -1 }).collect();

    let temp_at = |s: usize| -> f32 {
        let f = s as f32 / (STAGES - 1) as f32;
        // Ease the cooling so the critical region gets more wall time.
        T_HOT - (T_HOT - T_COLD) * (f * f * (3.0 - 2.0 * f))
    };

    let mut curve = Vec::new();
    for stage in 0..stages_done {
        let temp = temp_at(stage);
        for _ in 0..SWEEPS {
            // One Metropolis sweep: visit every cell once (site order is
            // the array order — fixed, so the replay is exact).
            for i in 0..GX * GY {
                let x = i % GX;
                let y = i / GX;
                let s = spin[i];
                let xl = spin[y * GX + if x == 0 { GX - 1 } else { x - 1 }];
                let xr = spin[y * GX + if x == GX - 1 { 0 } else { x + 1 }];
                let yd = spin[if y == 0 { GY - 1 } else { y - 1 } * GX + x];
                let yu = spin[if y == GY - 1 { 0 } else { y + 1 } * GX + x];
                let field = xl + xr + yd + yu;
                let de = 2.0 * J * s as f32 * field as f32;
                if de <= 0.0 || rng.f01() < (-de / temp).exp() {
                    spin[i] = -s;
                }
            }
        }
        // M at this stage, measured from the lattice itself.
        let m: f32 = spin.iter().map(|&s| s as f32).sum::<f32>() / (GX * GY) as f32;
        curve.push((temp, m.abs()));
    }
    (spin, curve)
}

/// Energy per spin, measured from a lattice.
#[must_use]
fn energy_per_spin(spin: &[i8]) -> f32 {
    let mut e = 0.0_f32;
    for y in 0..GY {
        for x in 0..GX {
            let s = spin[y * GX + x] as f32;
            let r = spin[y * GX + if x == GX - 1 { 0 } else { x + 1 }] as f32;
            let d = spin[if y == GY - 1 { 0 } else { y + 1 } * GX + x] as f32;
            e -= J * s * (r + d);
        }
    }
    e / (GX * GY) as f32
}

// ── The frame ───────────────────────────────────────────────────────────────

/// Onsager's exact critical temperature (square lattice, kB = J = 1).
const T_C: f32 = 2.269;

pub fn frame(t: f32) -> WidgetNode {
    let stages_done = ((clamp01(t) * STAGES as f32).ceil() as usize).clamp(1, STAGES);
    let (spin, curve) = anneal(stages_done);
    let temp = curve.last().map(|&(t_, _)| t_).unwrap_or(T_HOT);
    let m_now = curve.last().map(|&(_, m)| m).unwrap_or(0.0);
    let e_now = energy_per_spin(&spin);
    // Where order sets in: the hottest stage with M > 0.5.
    let t_order = curve
        .iter()
        .filter(|&&(_, m)| m > 0.5)
        .map(|&(t_, _)| t_)
        .fold(f32::MIN, f32::max);

    // The render window.
    const X0: f32 = 200.0;
    const Y0: f32 = 88.0;
    const WD: f32 = 896.0;
    const HT: f32 = 524.0;
    let cw = WD / GX as f32;
    let ch = HT / GY as f32;

    let curve_draw = curve.clone();
    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — the cryostat's room.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(6, 6, 10)),
                    (1.0, Color::rgb(10, 10, 14)),
                ]),
            );

            // The sample frame.
            book.rrect(
                Rect::new(X0 - 12.0, Y0 - 12.0, X0 + WD + 12.0, Y0 + HT + 12.0),
                10.0,
                alpha(Color::rgb(24, 24, 32), 0.95),
            );

            // ── The lattice: one rect per spin ─────────────────────────
            // Up = warm white-violet, down = deep blue-dark. As T falls
            // the domains negotiate — the story is in the blobs.
            for y in 0..GY {
                for x in 0..GX {
                    let s = spin[y * GX + x];
                    let col = if s > 0 {
                        mix(VIOLET_SOFT, Color::rgb(246, 242, 255), 0.35)
                    } else {
                        Color::rgb(16, 20, 40)
                    };
                    book.rect(
                        Rect::new(
                            X0 + x as f32 * cw,
                            Y0 + y as f32 * ch,
                            X0 + (x + 1) as f32 * cw + 0.4,
                            Y0 + (y + 1) as f32 * ch + 0.4,
                        ),
                        col,
                    );
                }
            }

            // The domain walls, implied: a faint stroke where neighbours
            // disagree — sampled on the cell grid, every 2nd row/col.
            for y in (1..GY).step_by(2) {
                for x in (1..GX).step_by(2) {
                    let c = spin[y * GX + x];
                    let r = spin[y * GX + ((x + 1).min(GX - 1))];
                    let d = spin[(y + 1).min(GY - 1) * GX + x];
                    if c != r || c != d {
                        book.rect(
                            Rect::new(
                                X0 + x as f32 * cw - 0.4,
                                Y0 + y as f32 * ch - 0.4,
                                X0 + (x + 1) as f32 * cw + 0.4,
                                Y0 + (y + 1) as f32 * ch + 0.4,
                            ),
                            alpha(Color::rgb(90, 60, 140), 0.30),
                        );
                    }
                }
            }

            // ── The thermometer — T with Onsager's line on it ──────────
            const T_X: f32 = 72.0;
            const T_Y: f32 = 150.0;
            const T_H: f32 = 380.0;
            book.rrect(
                Rect::new(T_X - 18.0, T_Y - 30.0, T_X + 34.0, T_Y + T_H + 26.0),
                10.0,
                alpha(Color::rgb(12, 12, 17), 0.92),
            );
            // The tube.
            book.rrect(Rect::new(T_X - 4.0, T_Y, T_X + 4.0, T_Y + T_H), 4.0, Color::rgb(26, 28, 36));
            let tmap = |temp_: f32| T_Y + T_H * ((temp_ - T_COLD) / (T_HOT - T_COLD)).clamp(0.0, 1.0);
            // The mercury.
            let merc_y = tmap(temp);
            book.rrect(Rect::new(T_X - 3.0, merc_y, T_X + 3.0, T_Y + T_H), 3.0, tint(AMBER, 0.3));
            book.circle(Offset::new(T_X, T_Y + T_H), 9.0, alpha(tint(AMBER, 0.3), 0.95));
            // Onsager's mark.
            let tc_y = tmap(T_C);
            book.line(
                Offset::new(T_X - 14.0, tc_y),
                Offset::new(T_X + 16.0, tc_y),
                alpha(CYAN, 0.9),
                1.6,
            );
            // The current-T rider.
            book.line(
                Offset::new(T_X - 12.0, merc_y),
                Offset::new(T_X + 14.0, merc_y),
                alpha(tint(AMBER, 0.5), 0.95),
                2.4,
            );

            // ── The M(T) scatter — the curve, plotted by the machine ──
            const S_X: f32 = 940.0;
            const S_Y: f32 = 150.0;
            const S_W: f32 = 260.0;
            const S_H: f32 = 190.0;
            book.rrect(
                Rect::new(S_X - 16.0, S_Y - 30.0, S_X + S_W + 16.0, S_Y + S_H + 16.0),
                10.0,
                alpha(Color::rgb(12, 12, 17), 0.92),
            );
            let smap = |temp_: f32, m: f32| Offset::new(
                S_X + S_W * ((temp_ - T_COLD) / (T_HOT - T_COLD)).clamp(0.0, 1.0),
                S_Y + S_H * (1.0 - m.clamp(0.0, 1.0)),
            );
            // T_c line, vertical.
            let tc_x = S_X + S_W * ((T_C - T_COLD) / (T_HOT - T_COLD)).clamp(0.0, 1.0);
            book.line(
                Offset::new(tc_x, S_Y),
                Offset::new(tc_x, S_Y + S_H),
                alpha(CYAN, 0.5),
                1.2,
            );
            // The measured curve — the stages completed so far.
            let mut path = Path::new();
            for (i, &(temp_, m)) in curve_draw.iter().enumerate() {
                let o = smap(temp_, m);
                if i == 0 {
                    path.move_to(o);
                } else {
                    path.line_to(o);
                }
            }
            book.stroke(path, alpha(tint(AMBER, 0.2), 0.95), 1.8);
            for &(temp_, m) in curve_draw.iter() {
                let o = smap(temp_, m);
                book.circle(o, 2.2, alpha(tint(AMBER, 0.35), 0.95));
            }
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack = stack.push(receipt_panel(
        stages_done,
        temp,
        m_now,
        e_now,
        t_order,
        curve.len(),
    ));
    stack.into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(
    stages: usize,
    temp: f32,
    m: f32,
    e: f32,
    t_order: f32,
    points: usize,
) -> WidgetNode {
    let lines = [
        "ISING · THE CRITICALITY AXIS · THE MAGNET, ANNEALED".to_string(),
        format!(
            "{}×{} torus · Metropolis {} sweeps/stage · stage {}/{} replayed this frame",
            GX, GY, SWEEPS, stages, STAGES
        ),
        format!(
            "T = {temp:.3} · M = {m:.3} (measured) · E/spin = {e:.3} (measured)"
        ),
        format!(
            "order sets in (M > 0.5) at T ≈ {t_order:.2} · Onsager's exact T_c = {T_C:.3} — finite-lattice, quenched, honest"
        ),
        format!("M(T) curve: {points} stages plotted live from the same replay"),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 42.0;

    let mut stack = Stack::new();
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 16.0)
                .width(820.0)
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
