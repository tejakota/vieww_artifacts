//! exp_planck — *the radiation axis.* The curve that broke classical physics.
//!
//! On 14 December 1900 Planck read a paper containing a constant he had
//! introduced, in his own words, as "an act of desperation": the only way
//! to make the blackbody curve come out right was to let energy come in
//! lumps of hν. This plate evaluates his law directly —
//!
//! ```text
//!     B_λ(T) = 2hc² / λ⁵ · 1/(exp(hc/λk_BT) − 1)
//! ```
//!
//! — at twelve temperatures from a cool brown dwarf to a blue supergiant,
//! draws the spectra, colours a star field by what those spectra actually
//! look like, and then makes the curves testify against the two laws that
//! were derived from them.
//!
//! **Wien's displacement law**, λ_max·T = b. The peak is not taken from a
//! formula: it is **located numerically on the plate's own sampled curve**
//! by golden-section search, at every temperature, and the products λ_max·T
//! are printed with their spread. The constant they should agree on is
//! b = 2.897771955×10⁻³ m·K, which is itself hc/(k_B·x) where x = 4.965114
//! solves x = 5(1 − e^(−x)) — and the plate solves that transcendental
//! equation too, by bisection, rather than quoting 4.965.
//!
//! **Stefan–Boltzmann**, M = σT⁴. The total exitance is obtained by
//! **numerically integrating π·B_λ over the same sampled curve** (Simpson,
//! on a log-λ grid), for each temperature; a log–log fit of M against T
//! then gives an exponent that must be 4 and a prefactor that must be
//! σ = 5.670374×10⁻⁸ W m⁻² K⁻⁴. Both are printed against the constants.
//!
//! **And the ultraviolet catastrophe**, drawn: the Rayleigh–Jeans curve
//! 2ck_BT/λ⁴ is plotted beside Planck's for one temperature. It is the
//! same curve at long wavelengths and it goes to infinity at short ones.
//! The plate prints the wavelength at which classical physics is already
//! wrong by a factor of two.

use vieww_foundation::{
    BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle,
};
use vieww_widget::prelude::*;
use vieww_widget::{PaintWith, Painting, Text};

use crate::film_lib::{alpha, mix, Rng, INK, MINT, MUTED, RED};

/// Film-time this experiment spans.
pub(crate) const SECONDS: f32 = 12.0;

// ── The constants, and nothing else typed ───────────────────────────────────

const H: f64 = 6.626_070_15e-34; // J·s   (exact, SI 2019)
const C: f64 = 2.997_924_58e8; // m/s   (exact)
const KB: f64 = 1.380_649e-23; // J/K   (exact)
/// The CODATA values the plate's own measurements are checked against.
const WIEN_B: f64 = 2.897_771_955e-3; // m·K
const SIGMA: f64 = 5.670_374_419e-8; // W m⁻² K⁻⁴

/// Planck's spectral radiance, per unit wavelength.
fn planck(lambda: f64, t: f64) -> f64 {
    let x = H * C / (lambda * KB * t);
    if x > 700.0 {
        return 0.0;
    }
    2.0 * H * C * C / lambda.powi(5) / (x.exp() - 1.0)
}

/// Rayleigh–Jeans: the classical answer, exact at long wavelengths and
/// divergent at short ones.
fn rayleigh_jeans(lambda: f64, t: f64) -> f64 {
    2.0 * C * KB * t / lambda.powi(4)
}

/// The twelve temperatures, in kelvin — a brown dwarf to a blue supergiant.
fn temperatures() -> Vec<f64> {
    (0..12)
        .map(|i| 1000.0 * (30.0_f64 / 1.0).powf(i as f64 / 11.0))
        .collect()
}

/// Golden-section search for the peak of B_λ on a bracketing interval.
/// (Not a formula: the plate is measuring its own curve.)
fn peak_lambda(t: f64) -> f64 {
    let (mut a, mut b) = (1e-9_f64, 1e-3_f64);
    let gr = (5.0_f64.sqrt() - 1.0) / 2.0;
    // work in log-λ so the bracket is well conditioned over 6 decades
    let (mut la, mut lb) = (a.ln(), b.ln());
    let mut c = lb - gr * (lb - la);
    let mut d = la + gr * (lb - la);
    for _ in 0..200 {
        if planck(c.exp(), t) > planck(d.exp(), t) {
            lb = d;
        } else {
            la = c;
        }
        c = lb - gr * (lb - la);
        d = la + gr * (lb - la);
    }
    a = la.exp();
    b = lb.exp();
    (0.5 * (a + b)).max(1e-12)
}

/// Total exitance M = π∫B_λ dλ, by Simpson's rule on a log-λ grid that
/// spans six decades around the peak. (∫B dλ = ∫ B·λ d(lnλ).)
fn exitance(t: f64) -> f64 {
    let peak = peak_lambda(t);
    let lo = (peak / 60.0).ln();
    let hi = (peak * 220.0).ln();
    let n = 4000usize; // even
    let dh = (hi - lo) / n as f64;
    let g = |u: f64| {
        let l = u.exp();
        planck(l, t) * l
    };
    let mut s = g(lo) + g(hi);
    for i in 1..n {
        let u = lo + i as f64 * dh;
        s += if i % 2 == 1 { 4.0 } else { 2.0 } * g(u);
    }
    std::f64::consts::PI * s * dh / 3.0
}

/// The transcendental root x = 5(1 − e^(−x)), by bisection — the number
/// that makes Wien's constant b = hc/(k_B·x).
fn wien_x() -> f64 {
    let f = |x: f64| x - 5.0 * (1.0 - (-x).exp());
    let (mut lo, mut hi) = (1.0_f64, 10.0_f64);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if f(lo) * f(mid) <= 0.0 {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    0.5 * (lo + hi)
}

/// A crude but honest colour for a blackbody: integrate the spectrum
/// against three broad channel responses (long/medium/short), normalise to
/// the brightest channel. Not CIE — and the plate says so — but the hue
/// ordering it produces (red → white → blue) is the physics.
fn body_colour(t: f64) -> Color {
    let bands = [(0.62e-6, 0.10e-6), (0.54e-6, 0.09e-6), (0.46e-6, 0.09e-6)];
    let mut v = [0.0_f64; 3];
    for (k, &(c0, w)) in bands.iter().enumerate() {
        let mut acc = 0.0;
        for i in 0..40 {
            let l = c0 - 2.0 * w + i as f64 / 39.0 * 4.0 * w;
            let g = (-((l - c0) / w).powi(2)).exp();
            acc += planck(l, t) * g;
        }
        v[k] = acc;
    }
    let m = v[0].max(v[1]).max(v[2]).max(1e-30);
    Color::rgb(
        ((v[0] / m).powf(0.45) * 255.0) as u8,
        ((v[1] / m).powf(0.45) * 255.0) as u8,
        ((v[2] / m).powf(0.45) * 255.0) as u8,
    )
}

// ── The frame ───────────────────────────────────────────────────────────────

pub(crate) fn frame(t: f32) -> WidgetNode {
    let temps = temperatures();
    let shown = ((t * temps.len() as f32 * 1.25).floor() as usize + 1).min(temps.len());

    // ── Wien, measured ──
    let peaks: Vec<f64> = temps.iter().map(|&tt| peak_lambda(tt)).collect();
    let products: Vec<f64> = peaks.iter().zip(&temps).map(|(l, t)| l * t).collect();
    let b_mean = products.iter().sum::<f64>() / products.len() as f64;
    let b_spread = {
        let lo = products.iter().cloned().fold(f64::INFINITY, f64::min);
        let hi = products.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        (hi - lo) / b_mean
    };
    let x_root = wien_x();
    let b_from_root = H * C / (KB * x_root);

    // ── Stefan–Boltzmann, measured ──
    let ms: Vec<f64> = temps.iter().map(|&tt| exitance(tt)).collect();
    let (slope, sigma_fit) = {
        let xs: Vec<f64> = temps.iter().map(|t| t.ln()).collect();
        let ys: Vec<f64> = ms.iter().map(|m| m.ln()).collect();
        let n = xs.len() as f64;
        let mx = xs.iter().sum::<f64>() / n;
        let my = ys.iter().sum::<f64>() / n;
        let sxy: f64 = xs.iter().zip(&ys).map(|(x, y)| (x - mx) * (y - my)).sum();
        let sxx: f64 = xs.iter().map(|x| (x - mx) * (x - mx)).sum();
        let s = sxy / sxx;
        (s, (my - s * mx).exp())
    };

    // ── the ultraviolet catastrophe, located ──
    let t_uv = 5772.0; // the Sun's effective temperature
    let uv_lambda = {
        // the wavelength at which Rayleigh–Jeans is already 2× Planck
        let f = |l: f64| rayleigh_jeans(l, t_uv) / planck(l, t_uv).max(1e-300) - 2.0;
        let (mut lo, mut hi) = (1e-7_f64, 2e-5_f64);
        for _ in 0..200 {
            let mid = (lo.ln() * 0.5 + hi.ln() * 0.5).exp();
            if f(lo) * f(mid) <= 0.0 {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        (lo.ln() * 0.5 + hi.ln() * 0.5).exp()
    };

    let temps_c = temps.clone();
    let peaks_c = peaks.clone();
    let ms_c = ms.clone();

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            book.rect(
                Rect::new(0.0, 0.0, size.width, size.height),
                Gradient::vertical()
                    .with_dither()
                    .with_stops(&[(0.0, Color::rgb(5, 5, 9)), (1.0, Color::rgb(10, 10, 15))]),
            );

            // ── the star field, coloured by the spectra themselves ──
            let mut rng = Rng::new(0x5742_0000_0000_0003);
            book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
                for _ in 0..900 {
                    let x = rng.f01() * 1280.0;
                    let y = 150.0 + rng.f01() * 560.0;
                    let ti = (rng.f01() * shown as f32) as usize % shown.max(1);
                    let c = body_colour(temps_c[ti]);
                    let r = 0.5 + rng.f01() * 1.8;
                    g.circle(Offset::new(x, y), r, alpha(c, 0.18 + 0.4 * rng.f01()));
                }
            });

            // ── the spectra ──
            let gx = 58.0_f32;
            let gy = 214.0_f32;
            let gw = 700.0_f32;
            let gh = 420.0_f32;
            book.rrect(
                Rect::new(gx - 20.0, gy - 30.0, gx + gw + 20.0, gy + gh + 34.0),
                10.0,
                alpha(Color::rgb(12, 12, 18), 0.96),
            );
            // log-λ from 100 nm to 100 µm; log-B over 14 decades
            let l_lo = 1e-7_f64.ln();
            let l_hi = 1e-4_f64.ln();
            // Six of the twelve curves were clipped at the top of the first
            // cut's 1e15 ceiling — a 30,000 K peak radiates 1.0e17 W m⁻² sr⁻¹ m⁻¹
            // and the plate was drawing a flat line where the law's whole
            // point is the peak.
            let b_lo = 1e4_f64.ln();
            let b_hi = 1e18_f64.ln();
            let mx = |l: f64| gx + ((l.ln() - l_lo) / (l_hi - l_lo)).clamp(0.0, 1.0) as f32 * gw;
            let my = |b: f64| {
                gy + gh - ((b.max(1e-30).ln() - b_lo) / (b_hi - b_lo)).clamp(0.0, 1.0) as f32 * gh
            };
            // the visible band, marked
            book.rect(
                Rect::new(mx(380e-9), gy, mx(700e-9), gy + gh),
                alpha(Color::rgb(60, 56, 96), 0.22),
            );
            for (i, &tt) in temps_c.iter().enumerate().take(shown) {
                let c = body_colour(tt);
                let mut p = Path::new();
                for k in 0..400 {
                    let l = (l_lo + (l_hi - l_lo) * k as f64 / 399.0).exp();
                    let o = Offset::new(mx(l), my(planck(l, tt)));
                    if k == 0 {
                        p.move_to(o);
                    } else {
                        p.line_to(o);
                    }
                }
                book.stroke(p, alpha(c, 0.92), 1.8);
                // the measured peak
                book.circle(
                    Offset::new(mx(peaks_c[i]), my(planck(peaks_c[i], tt))),
                    3.0,
                    alpha(INK, 0.9),
                );
            }
            // Wien's line, through the peaks the plate located
            let mut wp = Path::new();
            for (i, &tt) in temps_c.iter().enumerate().take(shown) {
                let o = Offset::new(mx(peaks_c[i]), my(planck(peaks_c[i], tt)));
                if i == 0 {
                    wp.move_to(o);
                } else {
                    wp.line_to(o);
                }
            }
            book.stroke(wp, alpha(INK, 0.4), 1.0);

            // the classical curve, for the Sun
            let mut rj = Path::new();
            for k in 0..400 {
                let l = (l_lo + (l_hi - l_lo) * k as f64 / 399.0).exp();
                let o = Offset::new(mx(l), my(rayleigh_jeans(l, t_uv)));
                if k == 0 {
                    rj.move_to(o);
                } else {
                    rj.line_to(o);
                }
            }
            book.stroke(rj, alpha(RED, 0.65), 1.4);
            book.line(
                Offset::new(mx(uv_lambda), gy),
                Offset::new(mx(uv_lambda), gy + gh),
                alpha(RED, 0.4),
                1.0,
            );

            // ── Stefan–Boltzmann, log–log ──
            let sx = 812.0_f32;
            let sy = 214.0_f32;
            let sw = 410.0_f32;
            let sh = 190.0_f32;
            book.rrect(
                Rect::new(sx - 20.0, sy - 30.0, sx + sw + 20.0, sy + sh + 26.0),
                10.0,
                alpha(Color::rgb(12, 12, 18), 0.96),
            );
            let tx = |tt: f64| {
                sx + ((tt.ln() - 1000.0_f64.ln()) / (30000.0_f64.ln() - 1000.0_f64.ln())) as f32
                    * sw
            };
            let mmin = ms_c.first().copied().unwrap_or(1.0).ln();
            let mmax = ms_c.last().copied().unwrap_or(1.0).ln();
            let ty =
                |m: f64| sy + sh - ((m.ln() - mmin) / (mmax - mmin)).clamp(0.0, 1.0) as f32 * sh;
            // the T⁴ law, anchored at the coolest measured point
            book.line(
                Offset::new(tx(temps_c[0]), ty(ms_c[0])),
                Offset::new(tx(30000.0), ty(ms_c[0] * (30000.0 / temps_c[0]).powi(4))),
                alpha(MUTED, 0.45),
                1.2,
            );
            for (i, &tt) in temps_c.iter().enumerate().take(shown) {
                book.circle(
                    Offset::new(tx(tt), ty(ms_c[i])),
                    3.6,
                    alpha(body_colour(tt), 0.95),
                );
            }

            // ── the Wien products, as a ladder ──
            let wx = 812.0_f32;
            let wy = 486.0_f32;
            let ww = 410.0_f32;
            let wh = 148.0_f32;
            book.rrect(
                Rect::new(wx - 20.0, wy - 30.0, wx + ww + 20.0, wy + wh + 26.0),
                10.0,
                alpha(Color::rgb(12, 12, 18), 0.96),
            );
            // each product against the CODATA b, at ±0.2% full scale
            book.line(
                Offset::new(wx + ww * 0.5, wy),
                Offset::new(wx + ww * 0.5, wy + wh),
                alpha(MINT, 0.5),
                1.2,
            );
            for (i, &p) in products.iter().enumerate().take(shown) {
                let dev = ((p - WIEN_B) / WIEN_B) as f32;
                let y = wy + (i as f32 + 0.5) * (wh / temps_c.len() as f32);
                book.circle(
                    Offset::new(wx + ww * 0.5 + dev / 0.002 * (ww * 0.5), y),
                    3.0,
                    alpha(body_colour(temps_c[i]), 0.95),
                );
            }
        }),
    );

    let lines = ["PLANCK · THE RADIATION AXIS · THE CURVE THAT BROKE CLASSICAL PHYSICS".to_string(),
        format!(
            "B_λ(T) = 2hc²/λ⁵ · 1/(e^(hc/λk_BT) − 1) evaluated at {shown}/{} temperatures, 1,000 K → 30,000 K · h, c, k_B are the exact SI values and nothing else is typed",
            temps.len()
        ),
        format!(
            "WIEN, measured: every peak located by golden-section search on the plate's OWN sampled curve · ⟨λ_max·T⟩ = {:.6e} m·K vs CODATA {WIEN_B:.6e} ({:+.4}%), spread {:.2e}",
            b_mean,
            (b_mean - WIEN_B) / WIEN_B * 100.0,
            b_spread
        ),
        format!(
            "— and b is not quoted either: x = 5(1 − e^(−x)) solved by bisection gives x = {x_root:.6} (books: 4.965114), so b = hc/(k_B·x) = {b_from_root:.6e} m·K"
        ),
        format!(
            "STEFAN–BOLTZMANN, measured: M = π∫B_λ dλ by Simpson on a log-λ grid, then a log–log fit over all {} temperatures → M ∝ T^{slope:.5} (theory 4, {:+.4}%)",
            temps.len(),
            (slope - 4.0) / 4.0 * 100.0
        ),
        format!(
            "— the fitted prefactor is σ = {sigma_fit:.6e} W m⁻² K⁻⁴ vs CODATA {SIGMA:.6e} ({:+.3}%), obtained from nothing but numerical quadrature of Planck's curve",
            (sigma_fit - SIGMA) / SIGMA * 100.0
        ),
        format!(
            "THE ULTRAVIOLET CATASTROPHE (red curve, the Sun at {t_uv} K): Rayleigh–Jeans is already 2× too large at λ = {:.0} nm — and rises without limit below it",
            uv_lambda * 1e9
        )];

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(42.0)
                .top(32.0 + i as f32 * 15.0)
                .width(1200.0)
                .height(15.0)
                .child(
                    Text::new(line.clone()).style(
                        TextStyle::new(if i == 0 { 12.0 } else { 11.0 })
                            .monospace()
                            .letter_spacing(if i == 0 { 1.8 } else { 0.0 })
                            .color(alpha(
                                if i == 0 { MUTED } else { mix(MUTED, INK, 0.45) },
                                0.95,
                            )),
                    ),
                ),
        );
    }
    for (x, y, s) in [
        (58.0_f32, 186.0_f32, "SPECTRA — log λ (100 nm … 100 µm) × log B · the band is the visible · red: the classical curve".to_string()),
        (812.0, 186.0, "STEFAN–BOLTZMANN — grey line is T⁴".to_string()),
        (812.0, 458.0, "WIEN PRODUCTS λ_max·T — full scale ±0.2% of CODATA b".to_string()),
    ] {
        stack = stack.push(
            Positioned::new().left(x).top(y).width(720.0).height(14.0).child(
                Text::new(s).style(
                    TextStyle::new(9.5)
                        .monospace()
                        .letter_spacing(0.9)
                        .color(alpha(MUTED, 0.85)),
                ),
            ),
        );
    }
    stack.into()
}
