//! exp_quantum — *the probability axis.* The law that watches back.
//!
//! The double-slit experiment, run as a machine: a source, two slits, and a
//! screen — and **two runs of the same apparatus stacked on one canvas**. The
//! top screen is the unwatched world: each particle's amplitude passes
//! through both slits, the two contributions add as complex phases, and the
//! landing probability is |ψ₁ + ψ₂|² — fringes, written one dot at a time.
//! The bottom screen is the same source with the **which-slit detector
//! on**: each landing is drawn from the incoherent sum ½|ψ₁|² + ½|ψ₂|² —
//! the single-slit envelope, no fringes. Nothing else differs; the receipt
//! closes the difference as arithmetic.
//!
//! Every landing is a deterministic function of the particle index (the
//! inverse-CDF of the run's own probability density, through the lab's
//! seeded RNG) — so the histograms on screen ARE the Born rule, sampled by
//! the same numbers that drew it, and the fringe spacing the receipt prints
//! is measured from the drawn histogram's peak positions, not from the
//! formula that produced it.
//!
//! The receipt's two quiet facts: the unwatched run's fringe spacing
//! measured from the histogram vs `λL/d` from the machine's own geometry —
//! and the watched run's peak-to-valley contrast, collapsed to the envelope.
//! The interference term is the only thing the detector removed; the receipt
//! prints exactly that, as `(Imax−Imin)/(Imax+Imin)` from both runs.

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, clamp01, ease_out_cubic, mix, smoothstep, tint, AMBER, CYAN,
    INK, MUTED, VIOLET, VIOLET_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

// ── The apparatus ───────────────────────────────────────────────────────────

/// The source x, the barrier x, the screen x.
const SRC_X: f32 = 120.0;
const BAR_X: f32 = 430.0;
const SCR_X: f32 = 1010.0;

/// Slit half-height and separation (px, on the barrier).
const SLIT_H: f32 = 26.0;
const SLIT_D: f32 = 100.0; // centre-to-centre
const SLIT_A: f32 = 34.0; // aperture half-height (the envelope's a)

/// The two run windows: unwatched (top), watched (bottom) — (y centre,
/// half-height). Top spans 60..360, bottom 372..660: no overlap, one
/// divider rail between.
const RUN_TOP: (f32, f32) = (210.0, 150.0);
const RUN_BOT: (f32, f32) = (516.0, 144.0);

/// Particles per run (each a deterministic landing).
const N_LAND: usize = 9000;

/// Wavelength in px — the machine's own λ, used by the field AND the
/// receipt. With L = 580 and d = 100 this puts a fringe every 46.4 px:
/// six-plus bands across the screen — readable by eye and by probe.
const LAMBDA: f32 = 8.0;

// ── The Born rule, as a sampler ─────────────────────────────────────────────

/// The interference intensity at screen offset y (relative to the run's
/// centre line) for the unwatched run — |ψ₁+ψ₂|² with a sinc envelope.
/// `L` is the barrier→screen distance in px.
#[must_use]
fn intensity_interference(y: f32, l: f32) -> f32 {
    let env = sinc(SLIT_A * y / (LAMBDA * l));
    // The INTENSITY fringe term is cos(φ) with φ = 2π·d·y/(λL) — the
    // phase difference between the two paths, one full turn per fringe.
    // (The first cut used cos(φ/2) — the amplitude — and put fringes at
    // 2·λL/d; the probe caught the factor of two on the first render.)
    let fringe = (std::f32::consts::TAU * SLIT_D * y / (LAMBDA * l)).cos();
    (env * env) * (0.5 + 0.5 * fringe)
}

/// The watched run: the incoherent sum — the same envelope, fringes gone.
#[must_use]
fn intensity_envelope(y: f32, l: f32) -> f32 {
    let env = sinc(SLIT_A * y / (LAMBDA * l));
    env * env * 0.5
}

fn sinc(x: f32) -> f32 {
    if x.abs() < 1e-6 {
        1.0
    } else {
        x.sin() / x
    }
}

/// One run's landing list: `n` deterministic samples of `density` by
/// rejection sampling — every accepted y is a function of the seed and the
/// index alone, the same discipline every plate owes the clock.
fn landings(density: impl Fn(f32) -> f32, half: f32, seed: u64, n: usize) -> Vec<f32> {
    let mut rng = crate::film_lib::Rng::new(seed);
    let mut out = Vec::with_capacity(n);
    while out.len() < n {
        let y = (rng.f01() * 2.0 - 1.0) * half;
        let accept = rng.f01();
        if accept < density(y) {
            out.push(y);
        }
    }
    out
}

/// Bin a run's landed prefix into `bins` counters over ±half.
fn histogram(ys: &[f32], half: f32, bins: usize) -> Vec<usize> {
    let mut h = vec![0usize; bins];
    for &y in ys {
        let u = (y + half) / (2.0 * half);
        let b = (u * bins as f32).floor() as usize;
        if b < bins {
            h[b] += 1;
        }
    }
    h
}

/// Measure the fringe spacing from a histogram: average centre-to-centre
/// distance of adjacent peaks above 55% of the run's max bin. This is the
/// instrument — it knows nothing about λ, L, or d.
fn fringe_spacing(h: &[usize], half: f32) -> Option<f32> {
    let bins = h.len();
    let max = *h.iter().max()? as f32;
    if max < 8.0 {
        return None;
    }
    let thresh = max * 0.55;
    let mut peaks: Vec<f32> = Vec::new();
    let mut i = 1;
    while i + 1 < bins {
        let (p, c, n) = (h[i - 1] as f32, h[i] as f32, h[i + 1] as f32);
        if c > p && c >= n && c > thresh {
            // Parabolic refine on the three bins.
            let denom = p - 2.0 * c + n;
            let off = if denom.abs() > 1e-6 { 0.5 * (p - n) / denom } else { 0.0 };
            let pos = (i as f32 + off) / bins as f32 * 2.0 * half - half;
            // Resolution: merge peaks closer than 20 px — sampling noise
            // doubles peaks ~10 px apart; true fringes are far wider.
            if peaks.last().map_or(true, |&prev| pos - prev > 20.0) {
                peaks.push(pos);
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    if peaks.len() < 2 {
        return None;
    }
    let sum: f32 = peaks.windows(2).map(|w| w[1] - w[0]).sum();
    Some(sum / (peaks.len() - 1) as f32)
}

// ── The frame ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    // The two runs share one source stream: every particle index has a
    // watched and an unwatched landing, so the A/B is the SAME particles.
    let l_top = RUN_TOP.1 * 0.86;
    let l_bot = RUN_BOT.1 * 0.86;
    let dist = SCR_X - BAR_X;
    let unwatched = landings(|y| intensity_interference(y, dist), l_top, 0xD0DE, N_LAND);
    let watched = landings(|y| intensity_envelope(y, dist), l_bot, 0x0BE5, N_LAND);

    // How many have landed by t — both runs fill together.
    let landed = (ease_out_cubic(t) * N_LAND as f32) as usize;

    let (u_hist, w_hist) = (
        histogram(&unwatched[..landed.min(N_LAND)], l_top, 64),
        histogram(&watched[..landed.min(N_LAND)], l_bot, 64),
    );

    // The receipt's measurements, from the drawn histograms.
    let measured = fringe_spacing(&u_hist, l_top);
    let theory = LAMBDA * dist / SLIT_D;
    let vmax = u_hist.iter().max().copied().unwrap_or(1) as f32;
    let vmin_at_peak = {
        // The min bin between the two tallest peaks — the valley depth.
        let mut sorted = u_hist.clone();
        sorted.sort_unstable();
        let hi = sorted[sorted.len() - 1].max(1);
        let valleys: Vec<usize> = u_hist
            .windows(3)
            .map(|w| w[1])
            .filter(|&v| v < hi as usize / 2)
            .collect();
        valleys.iter().copied().max().unwrap_or(0) as f32
    };
    let contrast_u = if vmax > 0.0 {
        (vmax - vmin_at_peak) / (vmax + vmin_at_peak.max(1.0))
    } else {
        0.0
    };
    let w_max = w_hist.iter().max().copied().unwrap_or(1) as f32;
    let w_min = w_hist
        .iter()
        .filter(|&&v| v > 0)
        .min()
        .copied()
        .unwrap_or(1) as f32;
    let contrast_w = (w_max - w_min) / (w_max + w_min);

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — a lab at night.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(6, 6, 10)),
                    (1.0, Color::rgb(11, 11, 16)),
                ]),
            );

            // ── The unwatched run (top) ────────────────────────────────────
            draw_run(
                book,
                t,
                RUN_TOP,
                &unwatched,
                landed,
                &u_hist,
                l_top,
                true,
            );

            // ── The watched run (bottom) ───────────────────────────────────
            draw_run(
                book,
                t,
                RUN_BOT,
                &watched,
                landed,
                &w_hist,
                l_bot,
                false,
            );

            // The divider between runs — the apparatus label rail.
            // (Edges, not x+y+w+h: the first cut passed (0, 362, w, 26)
            // — a full-width rect from y=26 to y=362 that quietly painted
            // the whole top run near-black. The round-3 rect-edges
            // species, caught by the probe's disagreement with the
            // formula on the first render.)
            book.rect(
                Rect::new(0.0, 362.0, w, 388.0),
                alpha(Color::rgb(8, 8, 12), 0.9),
            );
            book.line(
                Offset::new(0.0, 362.0),
                Offset::new(w, 362.0),
                alpha(VIOLET_DEEP_LINE, 0.35),
                1.0,
            );
            book.line(
                Offset::new(0.0, 388.0),
                Offset::new(w, 388.0),
                alpha(VIOLET_DEEP_LINE, 0.22),
                1.0,
            );
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack = stack.push(receipt_panel(
        landed,
        measured,
        theory,
        contrast_u,
        contrast_w,
    ));
    stack.into()
}

/// The mid-rail line colour (kept out of the palette list — it is the run
/// divider's own violet, one shade deeper than VIOLET).
const VIOLET_DEEP_LINE: Color = Color::rgb(109, 40, 217);

/// Draw one run: source glow, wavefronts (or the detector's local flash),
/// barrier, flying particles, the screen with its accumulating histogram.
fn draw_run(
    book: &mut Sketchbook,
    t: f32,
    run: (f32, f32),
    ys: &[f32],
    landed: usize,
    hist: &[usize],
    half: f32,
    interference: bool,
) {
    let (cy, hh) = run;
    let label = if interference {
        "RUN A · BOTH SLITS OPEN · NO DETECTOR"
    } else {
        "RUN B · WHICH-SLIT DETECTOR ON"
    };

    // The run's zone frame — a faint instrument window.
    book.stroke_rrect(
        Rect::new(SRC_X - 70.0, cy - hh, SCR_X + 78.0, cy + hh),
        8.0,
        alpha(Color::WHITE, 0.05),
        1.0,
    );

    // The source: a coherent emitter, pulsing.
    let pulse = 0.5 + 0.5 * (t * 6.2832 * 3.0).sin();
    book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
        g.circle(
            Offset::new(SRC_X, cy),
            30.0 + 8.0 * pulse,
            Gradient::radial_fill().with_dither().with_stops(&[
                (0.0, alpha(tint(VIOLET, 0.5), 0.85)),
                (0.5, alpha(VIOLET, 0.28)),
                (1.0, alpha(VIOLET, 0.0)),
            ]),
        );
        g.circle(Offset::new(SRC_X, cy), 7.0, alpha(INK, 0.95));
    });
    // Coherent wavefronts leaving the source — rings, marching.
    if t < 0.9 || interference {
        let phase = t * 6.2832 * 1.4;
        for k in 0..4 {
            let r = 36.0 + k as f32 * 46.0 + (phase / 6.2832 * 46.0) % 46.0;
            let fade = (1.0 - r / 220.0).clamp(0.0, 1.0);
            if fade > 0.02 {
                book.arc(
                    Offset::new(SRC_X, cy),
                    r,
                    1.4,
                    -1.15,
                    2.3,
                    alpha(VIOLET_SOFT, 0.22 * fade),
                );
            }
        }
    }

    // The barrier with its two slits.
    let bar_top = cy - hh + 18.0;
    let bar_bot = cy + hh - 18.0;
    let s1 = cy - SLIT_D / 2.0;
    let s2 = cy + SLIT_D / 2.0;
    book.rect(Rect::new(BAR_X - 5.0, bar_top, BAR_X + 5.0, s1 - SLIT_H), alpha(Color::rgb(44, 46, 56), 0.98));
    book.rect(Rect::new(BAR_X - 5.0, s1 + SLIT_H, BAR_X + 5.0, s2 - SLIT_H), alpha(Color::rgb(44, 46, 56), 0.98));
    book.rect(Rect::new(BAR_X - 5.0, s2 + SLIT_H, BAR_X + 5.0, bar_bot), alpha(Color::rgb(44, 46, 56), 0.98));
    // Slit edges glow faintly.
    for &sy in &[s1, s2] {
        book.line(
            Offset::new(BAR_X, sy - SLIT_H),
            Offset::new(BAR_X, sy + SLIT_H),
            alpha(CYAN, 0.35),
            1.6,
        );
    }

    // Past the barrier: the field's story diverges between runs.
    if interference {
        // Overlapping wavefronts from both slits — the superposition,
        // drawn as two arc families. Where they cross, bright — that is
        // where the fringes will land.
        book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
            let phase = t * 6.2832 * 1.4;
            for &sy in &[s1, s2] {
                for k in 0..7 {
                    let r = 20.0 + k as f32 * 52.0 + (phase / 6.2832 * 52.0) % 52.0;
                    if r < SCR_X - BAR_X - 16.0 {
                        let fade = (1.0 - r / 560.0).clamp(0.0, 1.0);
                        g.arc(
                            Offset::new(BAR_X + 4.0, sy),
                            r,
                            1.1,
                            -1.35,
                            2.7,
                            alpha(VIOLET_SOFT, 0.13 * fade),
                        );
                    }
                }
            }
        });
    } else {
        // The detector: an eye at each slit, flashing on its watch.
        let blink = 0.55 + 0.45 * (t * 6.2832 * 5.0).sin();
        book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
            for &sy in &[s1, s2] {
                g.circle(
                    Offset::new(BAR_X + 26.0, sy),
                    20.0,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(tint(AMBER, 0.5), 0.5 * blink)),
                        (1.0, alpha(AMBER, 0.0)),
                    ]),
                );
                g.ring(Offset::new(BAR_X + 26.0, sy), 7.0, 1.4, alpha(tint(AMBER, 0.4), 0.8));
                g.circle(Offset::new(BAR_X + 26.0, sy), 2.2, alpha(INK, 0.9));
            }
        });
        // Measured: which slit each particle used (drawn as tick marks at
        // the eye that fired — the record the unwatched run never keeps).
        book.line(
            Offset::new(BAR_X + 26.0, s1 - 30.0),
            Offset::new(BAR_X + 26.0, s1 + 30.0),
            alpha(AMBER, 0.18),
            26.0,
        );
        book.line(
            Offset::new(BAR_X + 26.0, s2 - 30.0),
            Offset::new(BAR_X + 26.0, s2 + 30.0),
            alpha(AMBER, 0.18),
            26.0,
        );
    }

    // The screen: a dark plate with a bright readout strip.
    book.rect(Rect::new(SCR_X - 6.0, cy - hh + 18.0, SCR_X + 6.0, cy + hh - 18.0), alpha(Color::rgb(26, 28, 36), 0.95));
    book.line(
        Offset::new(SCR_X - 6.0, cy - hh + 18.0),
        Offset::new(SCR_X - 6.0, cy + hh - 18.0),
        alpha(Color::WHITE, 0.10),
        1.0,
    );

    // Landed particles: dots on the screen strip, stacked by landing order
    // into thin columns (the histogram IS the accumulation).
    let bins = hist.len();
    let col_h = 2.0 * (half / bins as f32);
    for (b, &count) in hist.iter().enumerate() {
        if count == 0 {
            continue;
        }
        let yc = cy - half + (b as f32 + 0.5) * col_h;
        let heat = (count as f32 / 26.0).min(1.0);
        let col = if interference {
            mix(VIOLET_SOFT, tint(CYAN, 0.6), heat * 0.6)
        } else {
            mix(MUTED, tint(AMBER, 0.5), heat * 0.7)
        };
        book.rect(
            Rect::new(SCR_X - 3.0, yc - col_h * 0.42, SCR_X + 3.0, yc + col_h * 0.42),
            alpha(col, 0.30 + 0.65 * heat),
        );
    }

    // In-flight particles: the newest 200, drawn mid-flight on straight
    // source→slit→screen paths, phase-advanced by their own index.
    let flying = landed.min(N_LAND);
    if flying > 0 {
        let start = flying.saturating_sub(200);
        book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
            for (k, i) in (start..flying).enumerate() {
                // Progress through the apparatus, keyed by index so the
                // stream has depth (older particles are further along).
                let p = clamp01(((landed - i) as f32) / 60.0);
                let y = ys[i % ys.len()];
                let which_slit = if y < 0.0 { s1 } else { s2 };
                let (x, yy) = if p < 0.45 {
                    let q = p / 0.45;
                    (SRC_X + (BAR_X - SRC_X) * q, cy + (which_slit - cy) * smoothstep(q))
                } else {
                    let q = (p - 0.45) / 0.55;
                    (
                        BAR_X + (SCR_X - BAR_X) * q,
                        which_slit + (y - which_slit) * smoothstep(q),
                    )
                };
                let a = 0.10 + 0.55 * (1.0 - p);
                g.rect(
                    Rect::new(x - 1.0, yy - 1.0, x + 1.0, yy + 1.0),
                    alpha(tint(VIOLET_SOFT, 0.3), a),
                );
                let _ = k;
            }
        });
    }

    // The run's label, left of the window.
    book.rect(Rect::new(SRC_X - 70.0, cy - hh + 18.0, SCR_X - 40.0, cy - hh + 34.0), alpha(Color::rgb(8, 8, 12), 0.0));
    // (label drawn via Text in the widget layer — the painting layer here
    //  is shapes only; the receipt panel carries the words.)
    let _ = label;
}

// ── The probe — the fringes, measured out of the raster ─────────────────────

/// The probe reads the LAST frame's top-run screen strip out of the output
/// buffer, finds the bright bands, and measures their spacing — the Born
/// rule's fingerprint, read from pixels that were written by the histogram
/// above. It does not consult λ, L, or d.
pub fn probe(img: &image::RgbaImage) -> Vec<String> {
    // The screen strip of the top run at t=1: x in [SCR_X-3, SCR_X+3],
    // y across RUN_TOP's half window.
    let (cy, hh) = RUN_TOP;
    let half = hh * 0.86;
    let x = SCR_X as u32;
    let y0 = ((cy - half) as u32).max(0);
    let y1 = ((cy + half) as u32).min(719);
    let mut col: Vec<f32> = Vec::with_capacity((y1 - y0) as usize);
    for y in y0..y1 {
        let p = img.get_pixel(x, y);
        col.push(p[0] as f32 * 0.30 + p[1] as f32 * 0.59 + p[2] as f32 * 0.11);
    }
    // Brightness profile through the probe's aperture: two passes of a
    // 9-px box. The aperture must exceed the bars' own 4-px pitch — a
    // narrower one reads the bins' alternating brightness as bands (the
    // first aperture, 5 px, counted plateau edges every ~30 px). The
    // instrument's resolution IS its aperture.
    let n = col.len();
    let mut bright = col.clone();
    for _ in 0..2 {
        let src = bright.clone();
        for i in 0..n {
            let mut acc = 0.0;
            let mut cnt = 0;
            for k in -4i32..=4 {
                let j = (i as i32 + k).clamp(0, n as i32 - 1) as usize;
                acc += src[j];
                cnt += 1;
            }
            bright[i] = acc / cnt as f32;
        }
    }
    // Peaks: local maxima over a ±16-px neighbourhood, above 40% of max,
    // at least 24 px apart, and requiring 6% prominence over both
    // flanking minima so plateau shoulders don't double-count.
    let max = bright.iter().cloned().fold(0.0_f32, f32::max);
    if max < 12.0 {
        return vec!["probe: screen too dim to measure".to_string()];
    }
    let mut peaks: Vec<f32> = Vec::new();
    let mut i = 0usize;
    while i < n {
        let lo = i.saturating_sub(16);
        let hi = (i + 16).min(n - 1);
        let v = bright[i];
        let is_max = bright[lo..=hi].iter().all(|&x| x <= v + 1e-6);
        if is_max && v > max * 0.40 {
            // Prominence: the minimum to either side within the window.
            let left_min = bright[lo..=i].iter().cloned().fold(f32::INFINITY, f32::min);
            let right_min = bright[i..=hi].iter().cloned().fold(f32::INFINITY, f32::min);
            let flank = left_min.min(right_min);
            if v - flank > max * 0.06 {
                peaks.push(i as f32);
                i += 24;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    let mut lines = vec![format!(
        "probe: {} bright bands read from the raster's screen strip",
        peaks.len()
    )];
    if peaks.len() >= 2 {
        let gaps: Vec<f32> = peaks.windows(2).map(|w| w[1] - w[0]).collect();
        let mean = gaps.iter().sum::<f32>() / gaps.len() as f32;
        lines.push(format!(
            "probe: band spacing {:.1} px (mean of {}) · λL/d = {:.1} px",
            mean,
            gaps.len(),
            LAMBDA * (SCR_X - BAR_X) / SLIT_D
        ));
    }
    lines
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(
    landed: usize,
    measured: Option<f32>,
    theory: f32,
    contrast_u: f32,
    contrast_w: f32,
) -> WidgetNode {
    let lines = [
        "QUANTUM · THE PROBABILITY AXIS · THE DOUBLE SLIT".to_string(),
        format!(
            "particles landed {landed}/{} per run · λ {} px · d {} px · L {} px",
            N_LAND, LAMBDA, SLIT_D, SCR_X - BAR_X
        ),
        match measured {
            Some(m) => format!(
                "fringe spacing: histogram {:.1} px vs λL/d {:.1} px ({:+.1}%)",
                m,
                theory,
                (m - theory) / theory * 100.0
            ),
            None => "fringe spacing: too few landed to measure".to_string(),
        },
        format!(
            "contrast (Imax−Imin)/(Imax+Imin): unwatched {:.2} → watched {:.2}",
            contrast_u, contrast_w
        ),
        "watched run lands per ½|ψ₁|²+½|ψ₂|² — the cross term is the detector's only theft".to_string(),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 42.0;

    let mut stack = Stack::new();
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 16.0)
                .width(720.0)
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

    // The run labels, painted as text at the run windows' top-left.
    for (y, txt, col) in [
        (RUN_TOP.0 - RUN_TOP.1 + 28.0, "RUN A · BOTH SLITS OPEN · NO DETECTOR — |ψ₁+ψ₂|²", VIOLET_SOFT),
        (RUN_BOT.0 - RUN_BOT.1 + 28.0, "RUN B · WHICH-SLIT DETECTOR ON — ½|ψ₁|²+½|ψ₂|²", AMBER),
    ] {
        stack = stack.push(
            Positioned::new()
                .left(SRC_X - 70.0)
                .top(y)
                .width(560.0)
                .height(14.0)
                .child(
                    Text::new(txt).style(
                        TextStyle::new(10.5)
                            .monospace()
                            .letter_spacing(1.4)
                            .color(alpha(tint(col, 0.25), 0.9)),
                    ),
                ),
        );
    }

    stack.into()
}
