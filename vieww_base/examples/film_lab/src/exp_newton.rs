//! exp_newton — *the root axis.* Where a guess goes when it converges.
//!
//! Newton's method on a polynomial is the friendliest algorithm in
//! mathematics: stand anywhere, follow the tangent, land closer. Cayley
//! asked the obvious follow-up question in 1879 — *which* root do you land
//! on? — solved it for quadratics in a page, and then failed, publicly, for
//! eleven years on the cubic. The answer is this picture: the basins of a
//! cubic (and here, of z⁵ − 1 and every polynomial the film morphs it
//! through) are fractal, and the boundary between any two of them touches
//! **all** of them.
//!
//! Every pixel of the plate is one run of Newton's method: 57,600 runs per
//! frame, iterated to tolerance, coloured by which root it found and shaded
//! by how long it took.
//!
//! **The receipt closes three books.**
//! - **Quadratic convergence, fitted.** Newton doubles the correct digits
//!   each step near a simple root: |eₙ₊₁| = C·|eₙ|². The plate takes a
//!   seeded sample of orbits, takes their last few errors, and fits the
//!   exponent of |eₙ₊₁| against |eₙ| in log–log. The number that comes out
//!   should be 2, and the fitted constant C should be near |f''/2f'| at the
//!   root, which the plate also evaluates.
//! - **The Wada property.** For z^n − 1 with n ≥ 3, every boundary point
//!   is on the boundary of *all* n basins — an outrageous claim about a
//!   picture, and a countable one: the plate scans the raster it just drew
//!   and reports the fraction of boundary pixels whose 8-neighbourhood
//!   contains three or more different basins.
//! - **The basins' own shares**, measured by counting pixels, against the
//!   exact 1/n that symmetry demands of the disc at the centre.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{PaintWith, Painting, Text};

use crate::film_lib::{
    alpha, mix, shade, tint, Rng, AMBER, CYAN, CYAN_SOFT, INK, MINT, MUTED, RED, VIOLET,
    VIOLET_SOFT,
};

/// Film-time this experiment spans.
pub(crate) const SECONDS: f32 = 12.0;

// ── The field ───────────────────────────────────────────────────────────────

const GW: usize = 320;
const GH: usize = 180;
const DEGREE: usize = 5;
const MAX_ITER: usize = 40;
const TOL: f64 = 1e-10;

/// The view, in the complex plane.
const VIEW: (f64, f64, f64) = (0.0, 0.0, 1.85); // (cx, cy, half-height)

#[derive(Clone, Copy)]
struct C {
    re: f64,
    im: f64,
}

impl C {
    fn mul(self, o: C) -> C {
        C {
            re: self.re * o.re - self.im * o.im,
            im: self.re * o.im + self.im * o.re,
        }
    }
    fn sub(self, o: C) -> C {
        C {
            re: self.re - o.re,
            im: self.im - o.im,
        }
    }
    fn div(self, o: C) -> C {
        let d = o.re * o.re + o.im * o.im;
        if d == 0.0 {
            return C { re: 0.0, im: 0.0 };
        }
        C {
            re: (self.re * o.re + self.im * o.im) / d,
            im: (self.im * o.re - self.re * o.im) / d,
        }
    }
    fn abs(self) -> f64 {
        (self.re * self.re + self.im * self.im).sqrt()
    }
}

/// The polynomial the film morphs through: f(z) = z⁵ − 1 + a·z², with the
/// perturbation a running from 0 up and back. At a = 0 the picture is the
/// perfectly symmetric five-fold flower; as a grows the symmetry breaks and
/// the basins deform — which is the part that shows the basins are *not*
/// a property of the picture's prettiness but of the polynomial.
fn f_and_df(z: C, a: f64) -> (C, C) {
    // z^5
    let z2 = z.mul(z);
    let z3 = z2.mul(z);
    let z4 = z3.mul(z);
    let z5 = z4.mul(z);
    let f = C {
        re: z5.re - 1.0 + a * z2.re,
        im: z5.im + a * z2.im,
    };
    let df = C {
        re: 5.0 * z4.re + 2.0 * a * z.re,
        im: 5.0 * z4.im + 2.0 * a * z.im,
    };
    (f, df)
}

/// The roots of f, found by Newton from a ring of seeds and deduplicated —
/// the plate does not hard-code the five fifth-roots of unity, because at
/// a ≠ 0 they are not the roots any more.
fn roots_of(a: f64) -> Vec<C> {
    let mut out: Vec<C> = Vec::new();
    for k in 0..48 {
        let ang = k as f64 / 48.0 * std::f64::consts::TAU;
        let mut z = C {
            re: 1.25 * ang.cos(),
            im: 1.25 * ang.sin(),
        };
        for _ in 0..200 {
            let (f, df) = f_and_df(z, a);
            if df.abs() < 1e-14 {
                break;
            }
            let step = f.div(df);
            z = z.sub(step);
            if step.abs() < 1e-14 {
                break;
            }
        }
        if !z.re.is_finite() || !z.im.is_finite() {
            continue;
        }
        if !out.iter().any(|r| r.sub(z).abs() < 1e-6) {
            out.push(z);
        }
    }
    out
}

struct Field {
    /// Which root each pixel converged to (255 = none).
    basin: Vec<u8>,
    /// Iterations taken.
    iters: Vec<u8>,
    roots: Vec<C>,
}

fn compute(a: f64) -> Field {
    let roots = roots_of(a);
    let mut basin = vec![255u8; GW * GH];
    let mut iters = vec![0u8; GW * GH];
    let aspect = GW as f64 / GH as f64;
    for py in 0..GH {
        for px in 0..GW {
            let x = VIEW.0 + (px as f64 / (GW - 1) as f64 * 2.0 - 1.0) * VIEW.2 * aspect;
            let y = VIEW.1 + (py as f64 / (GH - 1) as f64 * 2.0 - 1.0) * VIEW.2;
            let mut z = C { re: x, im: y };
            let mut n = 0usize;
            for i in 0..MAX_ITER {
                let (f, df) = f_and_df(z, a);
                if df.abs() < 1e-16 {
                    break;
                }
                z = z.sub(f.div(df));
                n = i + 1;
                if f.abs() < TOL {
                    break;
                }
            }
            iters[py * GW + px] = n as u8;
            for (k, r) in roots.iter().enumerate() {
                if z.sub(*r).abs() < 1e-4 {
                    basin[py * GW + px] = k as u8;
                    break;
                }
            }
        }
    }
    Field {
        basin,
        iters,
        roots,
    }
}

/// The convergence order, fitted from real orbits. For each of a seeded
/// sample of starting points, the orbit's last errors |zₙ − r| are
/// recorded; a least-squares line through (ln|eₙ|, ln|eₙ₊₁|) has slope 2
/// for a simple root and slope 1 for a multiple one.
fn convergence_fit(a: f64, roots: &[C]) -> (f64, f64, usize, Vec<(f64, f64)>) {
    let mut rng = Rng::new(0x0E47_0000_0000_0009);
    let mut xs: Vec<f64> = Vec::new();
    let mut ys: Vec<f64> = Vec::new();
    let mut samples = 0usize;
    for _ in 0..900 {
        let mut z = C {
            re: (rng.f01() as f64 * 2.0 - 1.0) * 2.4,
            im: (rng.f01() as f64 * 2.0 - 1.0) * 1.6,
        };
        let mut errs: Vec<f64> = Vec::new();
        let mut hit = None;
        for _ in 0..MAX_ITER {
            let (f, df) = f_and_df(z, a);
            if df.abs() < 1e-16 {
                break;
            }
            z = z.sub(f.div(df));
            if let Some((_, r)) = roots
                .iter()
                .enumerate()
                .min_by(|p, q| z.sub(*p.1).abs().partial_cmp(&z.sub(*q.1).abs()).unwrap())
            {
                errs.push(z.sub(*r).abs());
                hit = Some(*r);
            }
        }
        if hit.is_none() || errs.len() < 6 {
            continue;
        }
        // Use only the window where the error is small enough to be in the
        // quadratic regime and large enough not to be rounding noise. The
        // first cut used every step of the orbit, including the wandering
        // at the start, and fitted 1.2 — an honest number about the wrong
        // question.
        for w in errs.windows(2) {
            if w[0] < 1e-2 && w[0] > 1e-12 && w[1] > 1e-15 {
                xs.push(w[0].ln());
                ys.push(w[1].ln());
                samples += 1;
            }
        }
    }
    let pairs: Vec<(f64, f64)> = xs.iter().copied().zip(ys.iter().copied()).collect();
    if samples < 8 {
        return (0.0, 0.0, samples, pairs);
    }
    let n = xs.len() as f64;
    let mx = xs.iter().sum::<f64>() / n;
    let my = ys.iter().sum::<f64>() / n;
    let sxy: f64 = xs.iter().zip(&ys).map(|(x, y)| (x - mx) * (y - my)).sum();
    let sxx: f64 = xs.iter().map(|x| (x - mx) * (x - mx)).sum();
    let slope = sxy / sxx;
    let intercept = my - slope * mx;
    // r²
    let ss_tot: f64 = ys.iter().map(|y| (y - my) * (y - my)).sum();
    let ss_res: f64 = xs
        .iter()
        .zip(&ys)
        .map(|(x, y)| (y - (slope * x + intercept)).powi(2))
        .sum();
    let r2 = 1.0 - ss_res / ss_tot;
    let _ = r2;
    (slope, intercept.exp(), samples, pairs)
}

// ── The frame ───────────────────────────────────────────────────────────────

const FX: f32 = 40.0;
const FY: f32 = 166.0;
const FW: f32 = 880.0;
const FH: f32 = 495.0;

pub(crate) fn frame(t: f32) -> WidgetNode {
    // the perturbation runs 0 → 0.9 → 0
    let a = 0.9 * (1.0 - (t as f64 * std::f64::consts::TAU).cos()) * 0.5;
    let field = compute(a);
    let n_roots = field.roots.len();

    // ── basin shares ──
    let mut shares = vec![0usize; n_roots.max(1)];
    let mut escaped = 0usize;
    for &b in &field.basin {
        if (b as usize) < n_roots {
            shares[b as usize] += 1;
        } else {
            escaped += 1;
        }
    }
    let total = (GW * GH) as f32;

    // ── the Wada count, read off the raster this frame drew ──
    let mut boundary = 0usize;
    let mut wada = 0usize;
    for y in 1..GH - 1 {
        for x in 1..GW - 1 {
            let me = field.basin[y * GW + x];
            let mut seen: Vec<u8> = vec![me];
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let b = field.basin[(y as i32 + dy) as usize * GW + (x as i32 + dx) as usize];
                    if !seen.contains(&b) {
                        seen.push(b);
                    }
                }
            }
            if seen.len() >= 2 {
                boundary += 1;
                if seen.len() >= 3 {
                    wada += 1;
                }
            }
        }
    }

    let (order, const_c, fit_n, pairs) = convergence_fit(a, &field.roots);
    // The theoretical constant at a root: |f''(r) / 2f'(r)|. For z⁵−1 at a
    // fifth root of unity that is |20r³ / 10r⁴| = 2/|r| = 2.
    let c_theory = {
        let r = field
            .roots
            .first()
            .copied()
            .unwrap_or(C { re: 1.0, im: 0.0 });
        let r2 = r.mul(r);
        let r3 = r2.mul(r);
        let r4 = r3.mul(r);
        let f2 = C {
            re: 20.0 * r3.re + 2.0 * a,
            im: 20.0 * r3.im,
        };
        let f1 = C {
            re: 5.0 * r4.re + 2.0 * a * r.re,
            im: 5.0 * r4.im + 2.0 * a * r.im,
        };
        f2.div(f1).abs() / 2.0
    };

    let mean_iters = field.iters.iter().map(|&i| i as f64).sum::<f64>() / total as f64;
    let basin = field.basin.clone();
    let iters = field.iters.clone();
    let roots = field.roots.clone();
    let shares_c = shares.clone();

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            book.rect(
                Rect::new(0.0, 0.0, size.width, size.height),
                Gradient::vertical()
                    .with_dither()
                    .with_stops(&[(0.0, Color::rgb(5, 5, 9)), (1.0, Color::rgb(10, 10, 15))]),
            );
            book.rrect(
                Rect::new(FX - 12.0, FY - 12.0, FX + FW + 12.0, FY + FH + 12.0),
                10.0,
                alpha(Color::rgb(12, 12, 18), 0.96),
            );

            let palette = [VIOLET, CYAN, AMBER, MINT, RED, VIOLET_SOFT, CYAN_SOFT];
            book.mesh(
                Rect::new(FX, FY, FX + FW, FY + FH),
                GW,
                GH,
                0.004,
                |cx, cy, _r| {
                    let i = cy * GW + cx;
                    let b = basin[i];
                    let it = iters[i] as f32;
                    // shade by iteration count: the "ripples" are level
                    // sets of how long Newton took, and they are the
                    // picture's real texture.
                    let k = (1.0 - (it / 22.0).clamp(0.0, 1.0)).powf(1.5);
                    if b as usize >= palette.len() && b == 255 {
                        return alpha(Color::rgb(8, 8, 12), 1.0).into();
                    }
                    let c = palette[b as usize % palette.len()];
                    // Keep the chroma: shade toward black for the slow
                    // pixels and tint only gently for the fast ones. The
                    // first cut mixed toward white at 0.55 and produced a
                    // pastel wash where the basins should be jewels.
                    alpha(mix(shade(c, 0.78), tint(c, 0.12), k), 1.0).into()
                },
            );

            // the roots themselves, marked on the picture they generated
            let aspect = GW as f64 / GH as f64;
            let sx = |re: f64| FX + ((re - VIEW.0) / (VIEW.2 * aspect) * 0.5 + 0.5) as f32 * FW;
            let sy = |im: f64| FY + ((im - VIEW.1) / VIEW.2 * 0.5 + 0.5) as f32 * FH;
            for (k, r) in roots.iter().enumerate() {
                book.ring(Offset::new(sx(r.re), sy(r.im)), 6.0, 1.8, alpha(INK, 0.9));
                let _ = k;
            }

            // ── the basin shares ──
            let bx = 952.0_f32;
            let by = 206.0_f32;
            let bw = 288.0_f32;
            let bh = 180.0_f32;
            book.rrect(
                Rect::new(bx - 18.0, by - 30.0, bx + bw + 18.0, by + bh + 22.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.95),
            );
            let n = shares_c.len().max(1);
            let ideal = 1.0 / n as f32;
            let scale = bw / (ideal * 2.2);
            for (k, &s) in shares_c.iter().enumerate() {
                let y = by + k as f32 * (bh / n as f32);
                let h = bh / n as f32 - 6.0;
                let frac = s as f32 / total;
                book.rect(
                    Rect::new(bx, y, bx + frac * scale, y + h),
                    alpha(palette[k % palette.len()], 0.85),
                );
            }
            book.line(
                Offset::new(bx + ideal * scale, by - 4.0),
                Offset::new(bx + ideal * scale, by + bh),
                alpha(INK, 0.55),
                1.2,
            );

            // ── the convergence ladder ──
            let cx0 = 952.0_f32;
            let cy0 = 468.0_f32;
            let cw = 288.0_f32;
            let chh = 172.0_f32;
            book.rrect(
                Rect::new(cx0 - 18.0, cy0 - 30.0, cx0 + cw + 18.0, cy0 + chh + 22.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.95),
            );
            // ln|e_{n+1}| vs ln|e_n|, and the fitted line
            let lo = -30.0_f64;
            let hi = 0.0_f64;
            let mx = |v: f64| cx0 + ((v - lo) / (hi - lo)).clamp(0.0, 1.0) as f32 * cw;
            let my = |v: f64| cy0 + chh - ((v - lo) / (hi - lo)).clamp(0.0, 1.0) as f32 * chh;
            // the identity (order 1) for reference
            book.line(
                Offset::new(mx(lo), my(lo)),
                Offset::new(mx(hi), my(hi)),
                alpha(MUTED, 0.40),
                1.0,
            );
            // the real error pairs — the fit has to answer to these
            for &(x, y) in pairs.iter().step_by(2) {
                if y < lo {
                    continue;
                }
                book.circle(Offset::new(mx(x), my(y)), 1.1, alpha(CYAN, 0.45));
            }
            // the fitted power law, drawn only where it is inside the box
            let x_lo = ((lo - const_c.ln()) / order).max(lo);
            let mut p = Path::new();
            p.move_to(Offset::new(mx(x_lo), my(order * x_lo + const_c.ln())));
            p.line_to(Offset::new(mx(hi), my(order * hi + const_c.ln())));
            book.stroke(p, alpha(AMBER, 0.95), 1.8);
        }),
    );

    let lines = ["NEWTON · THE ROOT AXIS · WHERE A GUESS GOES (CAYLEY'S QUESTION, 1879)".to_string(),
        format!(
            "f(z) = z⁵ − 1 + a·z², a = {a:.3} (animated) · {} × {} = {} starting guesses per frame, Newton to |f| < 1e−10 or {MAX_ITER} steps · mean {mean_iters:.2} iterations",
            GW, GH, GW * GH
        ),
        format!(
            "the {n_roots} roots are FOUND, not hard-coded (Newton from a ring of 48 seeds, deduplicated) — at a ≠ 0 the fifth roots of unity are not the roots any more"
        ),
        format!(
            "QUADRATIC CONVERGENCE, fitted from {fit_n} real error pairs in the window 1e−12 < |eₙ| < 1e−2: |eₙ₊₁| = C·|eₙ|^{order:.4} · theory says the exponent is exactly 2 ({:+.2}%)",
            (order - 2.0) / 2.0 * 100.0
        ),
        format!(
            "and the constant: fitted C = {const_c:.3} vs |f″(r)/2f′(r)| evaluated at the plate's own first root = {c_theory:.3} ({:+.1}%)",
            (const_c - c_theory) / c_theory.max(1e-9) * 100.0
        ),
        format!(
            "THE WADA PROPERTY, counted on the raster this frame drew: {boundary} boundary pixels, of which {wada} have three or more basins in their 8-neighbourhood = {:.1}%",
            wada as f32 / boundary.max(1) as f32 * 100.0
        ),
        format!(
            "BASIN SHARES by pixel count: {} · symmetry demands 1/{n_roots} = {:.4} of the view at a = 0 · {escaped} pixels reached no root at all",
            shares
                .iter()
                .map(|&s| format!("{:.4}", s as f32 / total))
                .collect::<Vec<_>>()
                .join(" "),
            1.0 / n_roots.max(1) as f32
        )];

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(42.0)
                .top(30.0 + i as f32 * 15.0)
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
        (
            952.0_f32,
            178.0_f32,
            "BASIN SHARES — the line is 1/n".to_string(),
        ),
        (
            952.0,
            440.0,
            "ln|eₙ₊₁| vs ln|eₙ| · grey: order 1 · amber: fit".to_string(),
        ),
    ] {
        stack = stack.push(
            Positioned::new()
                .left(x)
                .top(y)
                .width(320.0)
                .height(14.0)
                .child(
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
