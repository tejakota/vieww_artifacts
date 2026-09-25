//! exp_cymatics — *the frequency axis.* The sand that hears.
//!
//! A Chladni plate: sand scattered on a brass plate migrates to the nodal
//! lines of the vibrating surface — Ernst Chladni's 1787 demonstration that
//! sound has shape. The plate's field here is the textbook one,
//! `v = cos(nπx)cos(mπy) − cos(mπx)cos(nπy)`, and the grains do what sand
//! physically does: slide downhill on |v| — **7,000 grains, each running 26
//! annealed gradient-descent steps from its fixed seed, every frame, from
//! scratch** — no state, no memory, the same closed-form discipline every
//! plate in the lab owes the renderer.
//!
//! The frequency sweeps a ladder of modes — (1,2) → (3,1) → (2,3) → … — and
//! during each handoff the two fields blend, so the grains visibly *migrate*
//! between patterns: dissolve, walk, re-lattice. The Hz readout is computed
//! from the live mode pair through the plate's own constant; the settled
//! count is measured at the end of the descent, every frame.
//!
//! The receipt's quiet point: the grains are PALE because they sit where the
//! field is SILENT — the census prints how many found the quiet, and the
//! mean |v| they ended on, measured, not asserted.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, clamp01, mix, smoothstep, tint, AMBER, FAINT, INK, MUTED, VIOLET_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

// ── The plate ────────────────────────────────────────────────────────────────

/// The plate's window on the canvas: x0, y0, w, h.
const PLATE: (f32, f32, f32, f32) = (430.0, 92.0, 460.0, 536.0);

/// The mode ladder the frequency walks — (n, m) pairs.
const MODES: [(u32, u32); 8] = [
    (1, 2),
    (3, 1),
    (2, 3),
    (4, 2),
    (3, 4),
    (5, 3),
    (2, 5),
    (4, 4),
];

/// Grains: enough to make a texture of the nodes.
const GRAINS: usize = 7000;

/// Descent steps per frame — annealed coarse→fine inside.
const STEPS: usize = 26;

// ── The Chladni field ───────────────────────────────────────────────────────

/// The blended field at plate-local (u, v) in [0,1]², between modes A and B
/// by `s`. Returns a signed amplitude in roughly [-2, 2].
#[must_use]
fn field(u: f32, v: f32, a: (u32, u32), b: (u32, u32), s: f32) -> f32 {
    let pi = std::f32::consts::PI;
    let fa = (a.0 as f32 * pi * u).cos() * (a.1 as f32 * pi * v).cos()
        - (a.0 as f32 * pi * v).cos() * (a.1 as f32 * pi * u).cos();
    let fb = (b.0 as f32 * pi * u).cos() * (b.1 as f32 * pi * v).cos()
        - (b.0 as f32 * pi * v).cos() * (b.1 as f32 * pi * u).cos();
    fa * (1.0 - s) + fb * s
}

/// The mode schedule at t: which two modes, blended by how much.
#[must_use]
fn schedule(t: f32) -> ((u32, u32), (u32, u32), f32, f32) {
    let slots = MODES.len() as f32;
    let x = clamp01(t) * slots;
    let i = (x.floor() as usize).min(MODES.len() - 1);
    let j = (i + 1) % MODES.len();
    let f = x - i as f32;
    // Each slot: hold 78% on its mode, morph over the last 22%.
    let s = if f > 0.78 { smoothstep((f - 0.78) / 0.22) } else { 0.0 };
    (MODES[i], MODES[j], s, f)
}

// ── The grains ──────────────────────────────────────────────────────────────

/// One grain's seed (plate-local, in [0,1]²) and its descent result.
struct Grain {
    su: f32,
    sv: f32,
    u: f32,
    v: f32,
    /// |field| at the settled position — the quiet it found.
    quiet: f32,
    /// Distance walked this frame (pixels) — the "heat" of the search.
    walked: f32,
}

/// Run every grain's descent from its seed — deterministic, stateless.
#[must_use]
fn settle(t: f32) -> Vec<Grain> {
    let (a, b, s, _) = schedule(t);
    let mut rng = crate::film_lib::Rng::new(0xC0D0_u64);
    let mut grains = Vec::with_capacity(GRAINS);
    for _ in 0..GRAINS {
        let su = 0.02 + rng.f01() * 0.96;
        let sv = 0.02 + rng.f01() * 0.96;
        let (mut u, mut v) = (su, sv);
        let (mut walked, mut last) = (0.0_f32, 0.0_f32);
        // Annealed descent: coarse steps collapse toward the valley system,
        // fine steps land on the line. The jitter radius shrinks with it, so
        // the grains keep a speck of individual width instead of collapsing
        // to exact points — sand has a size.
        for k in 0..STEPS {
            let anneal = 1.0 - k as f32 / STEPS as f32;
            let step = 0.028 * anneal + 0.0035;
            let eps = 0.004;
            // Numeric gradient of |f|² (2·f·∇f), central differences.
            let fu = field(u + eps, v, a, b, s) - field(u - eps, v, a, b, s);
            let fv = field(u, v + eps, a, b, s) - field(u, v - eps, a, b, s);
            let f0 = field(u, v, a, b, s);
            let gu = 2.0 * f0 * fu;
            let gv = 2.0 * f0 * fv;
            let norm = (gu * gu + gv * gv).sqrt();
            if norm > 1e-9 {
                let jx = (rng.sym() * 0.14) * step;
                let jy = (rng.sym() * 0.14) * step;
                let du = -gu / norm * step + jx;
                let dv = -gv / norm * step + jy;
                u = (u + du).clamp(0.004, 0.996);
                v = (v + dv).clamp(0.004, 0.996);
                walked += (du * du + dv * dv).sqrt() * PLATE.2;
            }
            last = f0.abs();
        }
        grains.push(Grain { su, sv, u, v, quiet: last, walked });
    }
    grains
}

pub fn frame(t: f32) -> WidgetNode {
    let grains = settle(t);
    let (a, b, s, slot_f) = schedule(t);
    // The census, measured at the end of the descent.
    let settled = grains.iter().filter(|g| g.quiet < 0.02).count();
    let mean_quiet = grains.iter().map(|g| g.quiet).sum::<f32>() / grains.len() as f32;
    let live = if s > 0.5 { b } else { a };
    let hz = 180.0 * ((live.0 as f32).powi(2) + (live.1 as f32).powi(2)).sqrt();

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — a dark instrument room.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(7, 7, 10)),
                    (1.0, Color::rgb(12, 12, 16)),
                ]),
            );

            let (px, py, pw, ph) = PLATE;

            // The stand: a column and foot, brass-dark.
            book.rect(
                Rect::new(636.0, py + ph, 644.0, 700.0),
                mix(MUTED, Color::BLACK, 0.55),
            );
            book.rrect(
                Rect::new(500.0, 696.0, 780.0, 712.0),
                6.0,
                mix(MUTED, Color::BLACK, 0.45),
            );

            // The plate itself: a brass rectangle with a warm sheen.
            book.rrect(
                Rect::new(px - 10.0, py - 10.0, px + pw + 10.0, py + ph + 10.0),
                10.0,
                Gradient::linear(Offset::new(0.0, 0.0), Offset::new(1.0, 1.0))
                    .with_dither()
                    .with_stops(&[
                        (0.0, Color::rgb(84, 62, 30)),
                        (0.5, Color::rgb(110, 84, 44)),
                        (1.0, Color::rgb(66, 48, 24)),
                    ]),
            );
            book.stroke_rrect(
                Rect::new(px - 10.0, py - 10.0, px + pw + 10.0, py + ph + 10.0),
                10.0,
                alpha(tint(AMBER, 0.25), 0.35),
                1.2,
            );

            // The antinodal shimmer — where the field is LOUD, the plate
            // reads faintly hot. Sampled on a 23×27 grid, one rect each,
            // alpha from |field| — the negative of the pattern the grains
            // are about to draw, drawn first so the sand sits on top.
            for i in 0..23 {
                for j in 0..27 {
                    let u = (i as f32 + 0.5) / 23.0;
                    let v = (j as f32 + 0.5) / 27.0;
                    let loud = field(u, v, a, b, s).abs();
                    if loud > 0.55 {
                        let cw = pw / 23.0;
                        let ch = ph / 27.0;
                        book.rect(
                            Rect::new(px + u * pw - cw * 0.5, py + v * ph - ch * 0.5,
                                      px + u * pw + cw * 0.5, py + v * ph + ch * 0.5),
                            alpha(tint(AMBER, 0.1), 0.05 + 0.045 * (loud - 0.55)),
                        );
                    }
                }
            }

            // THE GRAINS — 7,000 rects, one each. Pale where quiet, dark
            // bronze where still churning: the settled lines read as light.
            for g in &grains {
                let x = px + g.u * pw;
                let y = py + g.v * ph;
                let calm = clamp01(1.0 - g.quiet / 0.35);
                let col = mix(
                    mix(Color::rgb(96, 74, 40), Color::rgb(238, 222, 186), calm),
                    Color::rgb(252, 244, 224),
                    calm * 0.5,
                );
                book.rect(
                    Rect::new(x - 1.1, y - 1.1, x + 1.1, y + 1.1),
                    alpha(col, 0.55 + 0.45 * calm),
                );
            }

            // ── The frequency dial — a gauge, its needle at f(m,n). ──
            let dc = Offset::new(180.0, 340.0);
            let needle = clamp01((hz - 400.0) / 1300.0);
            book.rrect(
                Rect::new(96.0, 240.0, 268.0, 448.0),
                10.0,
                alpha(Color::rgb(16, 16, 21), 0.92),
            );
            book.stroke_rrect(
                Rect::new(96.0, 240.0, 268.0, 448.0),
                10.0,
                alpha(Color::WHITE, 0.08),
                1.0,
            );
            // The Hz arc, with tick marks at each mode of the ladder.
            book.arc(
                dc,
                62.0,
                9.0,
                std::f32::consts::PI * 0.75,
                std::f32::consts::PI * 1.5 * 0.92,
                alpha(Color::rgb(40, 44, 52), 0.9),
            );
            // Ticks for each mode, drawn from the ladder itself.
            for nm in MODES.iter() {
                let f = 180.0 * ((nm.0 as f32).powi(2) + (nm.1 as f32).powi(2)).sqrt();
                let ang = std::f32::consts::PI * 0.75
                    + std::f32::consts::PI * 1.5 * 0.92 * clamp01((f - 400.0) / 1300.0);
                book.line(
                    Offset::new(dc.dx + ang.cos() * 56.0, dc.dy + ang.sin() * 56.0),
                    Offset::new(dc.dx + ang.cos() * 68.0, dc.dy + ang.sin() * 68.0),
                    alpha(FAINT, 0.8),
                    1.2,
                );
            }
            let na = std::f32::consts::PI * 0.75 + std::f32::consts::PI * 1.5 * 0.92 * needle;
            book.line(
                dc,
                Offset::new(dc.dx + na.cos() * 52.0, dc.dy + na.sin() * 52.0),
                alpha(tint(AMBER, 0.35), 0.95),
                2.4,
            );
            book.circle(dc, 5.0, alpha(INK, 0.9));

            // The driver's cove: a small exciter under the plate centre —
            // where the bow touches the glass. A pulsing ring, phase-locked
            // to the (computed) frequency: the visual metronome.
            let beat = 0.5 + 0.5 * (t * 6.2832 * 6.0).sin();
            book.circle(
                Offset::new(px + pw * 0.5, py + ph * 0.5),
                10.0 + 5.0 * beat,
                Gradient::radial_fill().with_dither().with_stops(&[
                    (0.0, alpha(tint(AMBER, 0.3), 0.5 * beat)),
                    (1.0, alpha(AMBER, 0.0)),
                ]),
            );
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack.push(receipt_panel(a, b, s, settled, mean_quiet, hz)).into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(
    a: (u32, u32),
    b: (u32, u32),
    s: f32,
    settled: usize,
    mean_quiet: f32,
    hz: f32,
) -> WidgetNode {
    let mode_line = if s > 0.02 {
        format!(
            "mode ({},{}) → ({},{}) · blend {:.0}%",
            a.0, a.1, b.0, b.1, s * 100.0
        )
    } else {
        format!("mode ({},{}) · steady", a.0, a.1)
    };
    let lines = [
        "CYMATICS · THE FREQUENCY AXIS · THE SAND THAT HEARS".to_string(),
        mode_line,
        format!("f(m,n) = 180·√(n²+m²) = {hz:.0} Hz · plate 460×536"),
        format!(
            "grains 7,000 · steps 26 (annealed) · settled {settled} · mean |v| {mean_quiet:.3}"
        ),
        "descent: closed-form per frame · no state, no memory".to_string(),
    ];

    const P_X: f32 = 96.0;
    const P_Y: f32 = 488.0;

    let mut stack = Stack::new();
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 16.0)
                .width(360.0)
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

    // The instrument: the settled fraction, as a bar that fills with the
    // census every frame — the receipt drawn as glass.
    let frac = settled as f32 / 7000.0;
    let bar = Painting::sized(
        Size::new(240.0, 26.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            book.rrect(
                Rect::new(0.0, 0.0, 240.0, 26.0),
                6.0,
                alpha(Color::rgb(16, 16, 21), 0.9),
            );
            book.rrect(
                Rect::new(3.0, 3.0, 3.0 + 234.0 * frac, 23.0),
                4.0,
                alpha(
                    if frac > 0.8 { VIOLET_SOFT } else { mix(AMBER, VIOLET_SOFT, 0.4) },
                    0.75,
                ),
            );
            book.stroke_rrect(
                Rect::new(0.0, 0.0, 240.0, 26.0),
                6.0,
                alpha(Color::WHITE, 0.08),
                1.0,
            );
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(P_X)
            .top(P_Y + 84.0)
            .width(240.0)
            .height(26.0)
            .child(bar),
    );

    stack.into()
}
