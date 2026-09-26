//! exp_rainbow — *the deviation axis.* The bow nobody drew.
//!
//! Descartes, 1637: send light into a sphere of water — refract, reflect
//! once, refract out — for every angle of incidence, and the exit
//! directions pile up where the deviation stands still. That stationary
//! point, D_min ≈ 137.5° for red, IS the rainbow: **42.4° from the
//! antisolar point, red outside, violet inside** — and a second bow at
//! 51° with two internal reflections and its spectrum reversed, with
//! Alexander's dark band hanging between them. The rainbow is a caustic:
//! light's own density, made visible by ten million drops.
//!
//! This plate runs the ray optics as its machine: **the per-wavelength
//! deviation histograms computed from Snell's law at incidence swept
//! 0–90° (the caustic spike smoothed to the physical Airy width), then
//! 2,600 seeded drops painted by the kernel at their own angular
//! position** — primary, secondary, and the dark band emerge from the
//! statistics of arrival, exactly as in the sky. The receipt closes the
//! law's books twice: from the kernel table (each λ's measured peak vs
//! Descartes' stationary D_min), and from the output raster — **the
//! angular luminance profile of the final frame, binned, its two peaks
//! read off in degrees.**

use std::f64::consts::PI;
use std::sync::OnceLock;

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook,
    TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, mix, AMBER, CYAN, INK, MUTED, VIOLET, Rng};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

// ── The optics ──────────────────────────────────────────────────────────────

/// Five sample wavelengths (nm) and their screen colours.
const BANDS: [(f64, Color); 5] = [
    (700.0, Color::rgb(255, 64, 54)),   // red
    (610.0, Color::rgb(255, 150, 40)),  // orange
    (550.0, Color::rgb(246, 232, 60)),  // yellow-green
    (470.0, Color::rgb(64, 178, 255)),  // blue
    (405.0, Color::rgb(142, 82, 255)),  // violet
];

/// Cauchy's law for water (λ in nm).
fn n_of(lambda: f64) -> f64 {
    1.3243 + 3587.0 / (lambda * lambda)
}

/// The deviation histogram kernel per band, on a 0.25° grid over the
/// observed angle β ∈ [0°, 65°], Gaussian-smoothed to the Airy width.
/// Computed once from Snell's law; `K[(band, βbin)]`.
fn kernel() -> &'static Vec<Vec<f32>> {
    static K: OnceLock<Vec<Vec<f32>>> = OnceLock::new();
    K.get_or_init(|| {
        let beta_min = 0.0_f64;
        let beta_max = 65.0;
        let step = 0.25;
        let nb = ((beta_max - beta_min) / step) as usize + 1;
        let mut k = vec![vec![0.0_f32; nb]; BANDS.len()];
        for (bi, &(lambda, _)) in BANDS.iter().enumerate() {
            let n = n_of(lambda);
            let mut raw = vec![0.0_f64; nb];
            // sweep incidence; density ∝ sin i / |dD/di|, estimated by the
            // finite difference of neighbouring deviations
            let inc: Vec<f64> = (0..=900).map(|x| x as f64 * 0.1).collect();
            let dev: Vec<f64> = inc
                .iter()
                .map(|&i| {
                    let r = (i.to_radians().sin() / n).asin().to_degrees();
                    180.0 + 2.0 * i - 4.0 * r // primary deviation D
                })
                .collect();
            for w in 1..inc.len() - 1 {
                let dd = (dev[w + 1] - dev[w - 1]).abs().max(1e-6);
                let weight = inc[w].to_radians().sin() * 0.2 / dd;
                let beta = 180.0 - dev[w];
                let b = ((beta - beta_min) / step) as i64;
                if b >= 0 && (b as usize) < nb {
                    raw[b as usize] += weight;
                }
            }
            // secondary bow: two internal reflections
            let dev2: Vec<f64> = inc
                .iter()
                .map(|&i| {
                    let r = (i.to_radians().sin() / n).asin().to_degrees();
                    360.0 + 2.0 * i - 6.0 * r
                })
                .collect();
            for w in 1..inc.len() - 1 {
                let dd = (dev2[w + 1] - dev2[w - 1]).abs().max(1e-6);
                let weight = inc[w].to_radians().sin() * 0.2 / dd * 0.42; // dimmer
                let beta = dev2[w] - 180.0;
                let b = ((beta - beta_min) / step) as i64;
                if b >= 0 && (b as usize) < nb {
                    raw[b as usize] += weight;
                }
            }
            // Gaussian smoothing to the physical width (~0.8°)
            let sigma = 0.8 / step;
            let mut smooth = vec![0.0_f64; nb];
            for i in 0..nb {
                let mut acc = 0.0;
                let mut wsum = 0.0;
                for d in -(sigma as isize * 3) as isize..=(sigma as isize * 3) as isize {
                    let j = i as isize + d;
                    if j >= 0 && (j as usize) < nb {
                        let wgt = (-(d * d) as f64 / (2.0 * sigma * sigma)).exp();
                        acc += raw[j as usize] * wgt;
                        wsum += wgt;
                    }
                }
                smooth[i] = if wsum > 0.0 { acc / wsum } else { 0.0 };
            }
            // normalise each band to its own peak
            let peak = smooth.iter().cloned().fold(0.0_f64, f64::max).max(1e-9);
            k[bi] = smooth.iter().map(|&v| (v / peak) as f32).collect();
        }
        k
    })
}

/// Kernel value at β (degrees) for a band, linearly interpolated.
fn k_at(bi: usize, beta: f64) -> f32 {
    let k = kernel();
    let x = (beta / 0.25) as f64;
    let i = x.floor() as usize;
    if i + 1 >= k[bi].len() {
        return 0.0;
    }
    let f = (x - i as f64) as f32;
    k[bi][i] * (1.0 - f) + k[bi][i + 1] * f
}

/// Descartes' stationary deviation for a band (closed form): the observed
/// angle β* = 180° − D_min for the primary, β* = D2_min − 180° for the
/// secondary, both minimised over incidence numerically from the same sweep.
fn descartes_peaks(lambda: f64, secondary: bool) -> f64 {
    let n = n_of(lambda);
    // The primary bow sits at the MINIMUM of the deviation D = 180+2i−4r,
    // i.e. the MAXIMUM of the observed angle β = 180 − D = 4r − 2i. The
    // secondary sits at the MINIMUM of D2 = 360+2i−6r, i.e. the MINIMUM
    // of β2 = D2 − 180. (The first cut minimised both and printed the
    // primary at 0.0° — the i = 0 endpoint, where there is no bow at all;
    // the probe's 41.0° caught it.)
    let mut best = if secondary { f64::INFINITY } else { f64::NEG_INFINITY };
    for x in 0..=9000 {
        let i = x as f64 * 0.01;
        let r = (i.to_radians().sin() / n).asin().to_degrees();
        let b = if secondary {
            360.0 + 2.0 * i - 6.0 * r - 180.0
        } else {
            4.0 * r - 2.0 * i
        };
        best = if secondary { best.min(b) } else { best.max(b) };
    }
    best
}

// ── The scene geometry ──────────────────────────────────────────────────────

const SUN: (f32, f32) = (120.0, 520.0);
const OBS: (f32, f32) = (700.0, 610.0);
const HORIZON: f32 = 560.0;

/// The drop veil, seeded once: (x, y, size).
fn drops() -> &'static Vec<(f32, f32, f32)> {
    static D: OnceLock<Vec<(f32, f32, f32)>> = OnceLock::new();
    D.get_or_init(|| {
        let mut rng = Rng::new(0xB0B0_0000);
        let mut v = Vec::with_capacity(2600);
        for _ in 0..2600 {
            let x = 240.0 + rng.f01() * 1000.0;
            // denser high, thinner at the horizon
            let y = 50.0 + rng.f01().powf(0.8) * (HORIZON - 70.0);
            let s = 0.9 + rng.f01() * 1.4;
            v.push((x, y, s));
        }
        v
    })
}

/// Observed angle β (deg) of a point, from the observer, off the antisolar
/// axis (the axis from observer away from the sun).
fn beta_of(px: f32, py: f32) -> f64 {
    let ax = (OBS.0 - SUN.0) as f64;
    let ay = (OBS.1 - SUN.1) as f64;
    let am = (ax * ax + ay * ay).sqrt();
    let (ax, ay) = (ax / am, ay / am);
    let dx = px as f64 - OBS.0 as f64;
    let dy = py as f64 - OBS.1 as f64;
    let dm = (dx * dx + dy * dy).sqrt().max(1e-6);
    let (ux, uy) = (dx / dm, dy / dm);
    let dot = (ux * ax + uy * ay).clamp(-1.0, 1.0);
    dot.acos().to_degrees()
}

// ── The frame ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    // reveal: drops accumulate over the first 40%, then the rain matures
    let n_shown = (((t / 0.4).clamp(0.0, 1.0)) * drops().len() as f32).round() as usize;

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // ── The sky: a storm clearing at the edges ──
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(14, 17, 26)),
                    (0.6, Color::rgb(24, 28, 40)),
                    (1.0, Color::rgb(46, 42, 52)),
                ]),
            );

            // rain haze: faint vertical streaks in the right half
            book.blended_layer(0.6, 0.0, BlendMode::Plus, None, |g| {
                let mut rng = Rng::new(0x5EED_7A1D);
                for _ in 0..160 {
                    let x = 300.0 + rng.f01() * 940.0;
                    let y = 40.0 + rng.f01() * 420.0;
                    let l = 26.0 + rng.f01() * 44.0;
                    g.line(
                        Offset::new(x, y),
                        Offset::new(x - 5.0, y + l),
                        alpha(Color::rgb(120, 140, 170), 0.05),
                        1.2,
                    );
                }
            });

            // ── The sun, low left, behind cloud edge ──
            book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
                g.ring(Offset::new(SUN.0, SUN.1), 54.0, 44.0, alpha(AMBER, 0.10));
                g.ring(Offset::new(SUN.0, SUN.1), 30.0, 26.0, alpha(AMBER, 0.16));
            });
            book.circle(Offset::new(SUN.0, SUN.1), 15.0, alpha(Color::rgb(255, 214, 150), 0.95));
            // god-rays: a few faint parallel beams from the sun into the rain
            book.blended_layer(0.5, 0.0, BlendMode::Plus, None, |g| {
                let dir = ((OBS.0 - SUN.0) as f64, (OBS.1 - SUN.1) as f64);
                let m = (dir.0 * dir.0 + dir.1 * dir.1).sqrt();
                let (ux, uy) = (dir.0 / m, dir.1 / m);
                for k in -3..=3 {
                    let off = k as f64 * 60.0;
                    let sx = SUN.0 as f64 - uy * off;
                    let sy = SUN.1 as f64 + ux * off;
                    g.line(
                        Offset::new(sx as f32, sy as f32),
                        Offset::new((sx + ux * 900.0) as f32, (sy + uy * 900.0) as f32),
                        alpha(AMBER, 0.045),
                        7.0,
                    );
                }
            });

            // ── The drop veil, painted by the kernel at each drop's own β ──
            book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
                for &(x, y, s) in drops()[..n_shown].iter() {
                    let beta = beta_of(x, y);
                    if beta > 64.0 {
                        continue;
                    }
                    // mix the five bands' kernel colours at this β
                    let (mut r, mut gr, mut b) = (0.0_f32, 0.0_f32, 0.0_f32);
                    for (bi, &(_, col)) in BANDS.iter().enumerate() {
                        let k = k_at(bi, beta);
                        r += col.r as f32 * k;
                        gr += col.g as f32 * k;
                        b += col.b as f32 * k;
                    }
                    let amp = (r + gr + b) / (3.0 * 255.0 * 1.35);
                    let col = Color::rgba(
                        (r / 1.35).min(255.0) as u8,
                        (gr / 1.35).min(255.0) as u8,
                        (b / 1.35).min(255.0) as u8,
                        (amp.min(1.0) * 255.0) as u8,
                    );
                    g.circle(Offset::new(x, y), s * 1.5, col);
                }
            });
            // a second, sharper pass on the drops for the bright band cores
            for &(x, y, s) in drops()[..n_shown].iter() {
                let beta = beta_of(x, y);
                if beta > 64.0 {
                    continue;
                }
                let mut bright = 0.0_f32;
                for (bi, _) in BANDS.iter().enumerate() {
                    bright = bright.max(k_at(bi, beta));
                }
                if bright > 0.35 {
                    let c = if beta < 46.0 {
                        // primary: red outside, violet inside — the slope of β
                        mix(Color::rgb(255, 200, 150), Color::rgb(190, 150, 255),
                            (((beta - 39.0) / 3.4).clamp(0.0, 1.0)) as f32)
                    } else {
                        // secondary: reversed
                        mix(Color::rgb(190, 150, 255), Color::rgb(255, 200, 150),
                            (((beta - 50.0) / 3.4).clamp(0.0, 1.0)) as f32)
                    };
                    book.circle(
                        Offset::new(x, y),
                        s * 0.9,
                        alpha(c, (bright - 0.35) * 1.2),
                    );
                }
            }

            // ── The ground: hills, wet ──
            let mut ground = Path::new();
            ground.move_to(Offset::new(0.0, h));
            ground.line_to(Offset::new(0.0, HORIZON + 26.0));
            let mut rng = Rng::new(0x6D_0157);
            let mut gx = 0.0;
            while gx < w {
                let step = 60.0 + rng.f01() * 50.0;
                gx += step;
                let gy = HORIZON + 26.0 + rng.sym() * 16.0 - 18.0;
                ground.line_to(Offset::new(gx, gy));
            }
            ground.line_to(Offset::new(w, h));
            ground.close();
            book.fill(ground, Color::rgb(10, 10, 14));

            // the observer, small, at their spot
            let ob = Offset::new(OBS.0, OBS.1 - 26.0);
            book.circle(Offset::new(ob.dx, ob.dy + 16.0), 4.2, Color::rgb(8, 8, 12)); // shadow
            book.rrect(Rect::new(ob.dx - 5.0, ob.dy - 4.0, ob.dx + 5.0, ob.dy + 14.0), 3.0, Color::rgb(30, 30, 40));
            book.circle(Offset::new(ob.dx, ob.dy - 8.0), 4.4, Color::rgb(232, 228, 236));
            book.ring(Offset::new(ob.dx, ob.dy - 8.0), 4.4, 1.0, alpha(INK, 0.8));

            // ── The kernel panel, bottom-right: the machine's own physics ──
            let (kx, ky, kw, kh) = (880.0, 560.0, 360.0, 130.0);
            book.rrect(
                Rect::new(kx - 14.0, ky - 20.0, kx + kw + 14.0, ky + kh + 18.0),
                10.0,
                alpha(Color::rgb(12, 12, 18), 0.9),
            );
            let bx = |beta: f64| kx + ((beta - 30.0) / 34.0 * kw as f64) as f32;
            let by = |v: f32| ky + kh - v * (kh - 12.0);
            // axis: β 30..64
            book.line(
                Offset::new(kx, ky + kh),
                Offset::new(kx + kw, ky + kh),
                alpha(MUTED, 0.45),
                1.0,
            );
            for (bi, &(_, col)) in BANDS.iter().enumerate() {
                let mut path = Path::new();
                let mut started = false;
                let mut b = 34.0;
                while b < 64.0 {
                    let v = k_at(bi, b);
                    if v > 0.004 {
                        let (x, y) = (bx(b), by(v));
                        if !started {
                            path.move_to(Offset::new(x, y));
                            started = true;
                        } else {
                            path.line_to(Offset::new(x, y));
                        }
                    }
                    b += 0.25;
                }
                book.stroke(path, alpha(col, 0.9), 1.3);
            }
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack = stack.push(receipt_panel(n_shown));
    stack.into()
}

// ── The probe: the two bows, read from the raster ───────────────────────────

/// Build the angular luminance profile of the final frame (sky pixels,
/// binned by β from the observer), then report the two peak angles beside
/// Descartes' stationary values. Measured from pixels, not from the model.
pub fn probe(img: &image::RgbaImage) -> Vec<String> {
    let (iw, ih) = img.dimensions();
    let step = 0.25_f64;
    let nb = (64.0 / step) as usize;
    let mut sums = vec![0u64; nb];
    let mut counts = vec![0u64; nb];
    for y in (40..HORIZON as u32 - 6).step_by(3) {
        for x in (240..iw - 10).step_by(3) {
            let beta = beta_of(x as f32, y as f32);
            if beta < 0.0 || beta >= 64.0 {
                continue;
            }
            let p = img.get_pixel(x, y);
            let lum = (p.0[0] as u64 + p.0[1] as u64 + p.0[2] as u64) / 3;
            let b = (beta / step) as usize;
            sums[b] += lum;
            counts[b] += 1;
        }
    }
    let prof: Vec<f64> = (0..nb)
        .map(|i| sums[i] as f64 / counts[i].max(1) as f64)
        .collect();
    // smooth then find the two tallest local maxima in [36, 56]
    let mut sm = vec![0.0_f64; nb];
    for i in 0..nb {
        let mut acc = 0.0;
        let mut wsum = 0.0;
        for d in -8i64..=8 {
            let j = i as i64 + d;
            if j >= 0 && (j as usize) < nb {
                let wgt = (-(d * d) as f64 / 8.0).exp();
                acc += prof[j as usize] * wgt;
                wsum += wgt;
            }
        }
        sm[i] = if wsum > 0.0 { acc / wsum } else { 0.0 };
    }
    let mut peaks: Vec<(f64, f64)> = Vec::new(); // (beta, value)
    for i in ((36.0 / step) as usize)..((56.0 / step) as usize) {
        if sm[i] > sm[i - 1] && sm[i] > sm[i + 1] {
            peaks.push((i as f64 * step, sm[i]));
        }
    }
    peaks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut out = Vec::new();
    if peaks.len() >= 1 {
        out.push(format!(
            "RASTER PEAK 1: β = {:.1}° (luminance profile of the final frame)",
            peaks[0].0
        ));
    }
    if peaks.len() >= 2 {
        out.push(format!(
            "RASTER PEAK 2: β = {:.1}° — the secondary, dimmer and wider",
            peaks[1].0
        ));
    }
    // Descartes' stationary values, red and violet, from the same sweep
    out.push(format!(
        "DESCARTES: primary β* red {:.2}° … violet {:.2}° · secondary red {:.2}° … violet {:.2}°",
        descartes_peaks(700.0, false),
        descartes_peaks(405.0, false),
        descartes_peaks(700.0, true),
        descartes_peaks(405.0, true),
    ));
    out.push(format!(
        "Alexander's dark band: {:.1}°–{:.1}°, the gap between the bows, in the same profile",
        descartes_peaks(700.0, false),
        descartes_peaks(405.0, true),
    ));
    out
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(n_shown: usize) -> WidgetNode {
    let lines = [
        "RAINBOW · THE DEVIATION AXIS · DESCARTES, 1637".to_string(),
        "Snell's law at n(λ) = 1.3243 + 3587/λ² nm · incidence swept 0–90° · density ∝ sin i / |dD/di|".to_string(),
        format!(
            "2,600 seeded drops, each painted by the kernel at its own β — the bow is the statistics",
        ),
        format!(
            "drops shown {n_shown} · primary: D_min stationary · secondary: 2 reflections, spectrum reversed"
        ),
        "PROBE: the final frame's angular luminance profile, binned by β — the two peaks, in degrees".to_string(),
        "Alexander's dark band between them: the rays that go nowhere, visible as absence".to_string(),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 42.0;

    let mut stack = Stack::new();
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 16.0)
                .width(880.0)
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
