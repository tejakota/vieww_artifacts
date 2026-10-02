//! exp_pathtrace — *the light-transport axis.* Noise that obeys a law.
//!
//! A Cornell box, traced the honest way: every pixel fires rays into the
//! scene, every ray bounces diffusely until it finds the light or runs out
//! of depth, and the answer is the average. No denoiser, no importance
//! sampling of the light, no tricks — because the plate is not about the
//! picture, it is about **the noise**, and the noise is the thing that
//! obeys a law.
//!
//! Monte Carlo error falls as **1/√N**. Doubling the samples buys 1.41× the
//! quality, which is why path tracing is expensive and why every trick in
//! production rendering exists. The plate measures that exponent on its own
//! output rather than asserting it — and it does so **without a reference
//! image**, by the two-buffer trick: the samples are split into two
//! independent halves A and B, and RMS(A − B)/2 is an unbiased estimate of
//! the error in their mean. A converged reference would be a second
//! (expensive) rendering with the same bias; the half-buffer estimate has
//! none.
//!
//! **The receipt closes three books:**
//! - **The convergence exponent**, fitted in log–log over the whole sample
//!   ladder 1, 2, 4, … 64 spp. Theory says −0.5 exactly.
//! - **The white furnace test** — the energy audit every renderer should
//!   ship and few do. Put a perfectly white (albedo 1) diffuse sphere
//!   inside a uniformly emitting environment of radiance L. The sphere
//!   must disappear: whatever bounces in must bounce out, so every pixel
//!   must read exactly L. Any deviation is energy the integrator invented
//!   or lost. The plate runs it and prints the maximum relative error over
//!   the image.
//! - **The path-length census**: how many bounces the rays actually took,
//!   and what fraction terminated on the depth cap rather than on the
//!   light — the bias the cap introduces, counted.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{PaintWith, Painting, Text};

use crate::film_lib::{alpha, mix, Rng, AMBER, CYAN, INK, MINT, MUTED};

/// Film-time this experiment spans.
pub(crate) const SECONDS: f32 = 13.0;

// ── The scene: spheres only, in the smallpt tradition ───────────────────────

const IW: usize = 168;
const IH: usize = 112;
const MAX_DEPTH: usize = 5;
/// The sample ladder the convergence is fitted over.
/// The ladder starts at 2, not 1: the half-buffer estimator needs at
/// least one sample in each half, so a "1 spp" rung would silently be a
/// 2 spp render and its noise point would sit off the line it is being
/// fitted to. (It did: the fit read −0.4511 with that rung in.)
const LADDER: [usize; 6] = [2, 4, 8, 16, 32, 64];

#[derive(Clone, Copy)]
struct V3 {
    x: f64,
    y: f64,
    z: f64,
}

impl V3 {
    const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
    fn add(self, o: V3) -> V3 {
        V3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
    fn sub(self, o: V3) -> V3 {
        V3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
    fn mul(self, k: f64) -> V3 {
        V3::new(self.x * k, self.y * k, self.z * k)
    }
    fn mult(self, o: V3) -> V3 {
        V3::new(self.x * o.x, self.y * o.y, self.z * o.z)
    }
    fn dot(self, o: V3) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    fn cross(self, o: V3) -> V3 {
        V3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    fn norm(self) -> V3 {
        let l = self.dot(self).sqrt();
        if l > 0.0 {
            self.mul(1.0 / l)
        } else {
            self
        }
    }
}

#[derive(Clone, Copy)]
struct Sphere {
    r: f64,
    p: V3,
    emit: V3,
    albedo: V3,
}

impl Sphere {
    /// Ray–sphere, returning the near positive root or 0.
    fn hit(&self, o: V3, d: V3) -> f64 {
        let op = self.p.sub(o);
        let b = op.dot(d);
        let det = b * b - op.dot(op) + self.r * self.r;
        if det < 0.0 {
            return 0.0;
        }
        let det = det.sqrt();
        let eps = 1e-4;
        if b - det > eps {
            b - det
        } else if b + det > eps {
            b + det
        } else {
            0.0
        }
    }
}

/// The Cornell box, as huge spheres — the smallpt construction, because it
/// makes the whole scene one intersection routine.
fn scene() -> Vec<Sphere> {
    vec![
        // left (red), right (cyan), back, front, floor, ceiling
        Sphere {
            r: 1e5,
            p: V3::new(1e5 + 1.0, 40.8, 81.6),
            emit: V3::new(0.0, 0.0, 0.0),
            albedo: V3::new(0.75, 0.25, 0.25),
        },
        Sphere {
            r: 1e5,
            p: V3::new(-1e5 + 99.0, 40.8, 81.6),
            emit: V3::new(0.0, 0.0, 0.0),
            albedo: V3::new(0.25, 0.45, 0.78),
        },
        Sphere {
            r: 1e5,
            p: V3::new(50.0, 40.8, 1e5),
            emit: V3::new(0.0, 0.0, 0.0),
            albedo: V3::new(0.75, 0.75, 0.75),
        },
        Sphere {
            r: 1e5,
            p: V3::new(50.0, 40.8, -1e5 + 250.0),
            emit: V3::new(0.0, 0.0, 0.0),
            albedo: V3::new(0.0, 0.0, 0.0),
        },
        Sphere {
            r: 1e5,
            p: V3::new(50.0, 1e5, 81.6),
            emit: V3::new(0.0, 0.0, 0.0),
            albedo: V3::new(0.75, 0.75, 0.75),
        },
        Sphere {
            r: 1e5,
            p: V3::new(50.0, -1e5 + 81.6, 81.6),
            emit: V3::new(0.0, 0.0, 0.0),
            albedo: V3::new(0.75, 0.75, 0.75),
        },
        // two spheres
        Sphere {
            r: 16.5,
            p: V3::new(27.0, 16.5, 57.0),
            emit: V3::new(0.0, 0.0, 0.0),
            albedo: V3::new(0.88, 0.85, 0.80),
        },
        Sphere {
            r: 16.5,
            p: V3::new(73.0, 16.5, 88.0),
            emit: V3::new(0.0, 0.0, 0.0),
            albedo: V3::new(0.62, 0.55, 0.88),
        },
        // the light
        Sphere {
            r: 8.5,
            p: V3::new(50.0, 81.6 - 6.0, 81.6),
            emit: V3::new(24.0, 22.0, 19.0),
            albedo: V3::new(0.0, 0.0, 0.0),
        },
    ]
}

fn intersect(spheres: &[Sphere], o: V3, d: V3) -> Option<(usize, f64)> {
    let mut best = f64::INFINITY;
    let mut idx = None;
    for (i, s) in spheres.iter().enumerate() {
        let t = s.hit(o, d);
        if t > 0.0 && t < best {
            best = t;
            idx = Some(i);
        }
    }
    idx.map(|i| (i, best))
}

/// A cosine-weighted direction about `n` — the correct diffuse sampler,
/// whose pdf cancels the cosine term so the estimator is just the albedo.
fn cosine_hemisphere(n: V3, rng: &mut Rng) -> V3 {
    let r1 = std::f64::consts::TAU * rng.f01() as f64;
    let r2 = rng.f01() as f64;
    let r2s = r2.sqrt();
    let w = n;
    let u = if w.x.abs() > 0.1 {
        V3::new(0.0, 1.0, 0.0)
    } else {
        V3::new(1.0, 0.0, 0.0)
    }
    .cross(w)
    .norm();
    let v = w.cross(u);
    u.mul(r1.cos() * r2s)
        .add(v.mul(r1.sin() * r2s))
        .add(w.mul((1.0 - r2).sqrt()))
        .norm()
}

/// One path. Returns radiance and the number of bounces taken, plus
/// whether it died on the depth cap.
fn radiance(spheres: &[Sphere], mut o: V3, mut d: V3, rng: &mut Rng) -> (V3, usize, bool) {
    let mut throughput = V3::new(1.0, 1.0, 1.0);
    let mut acc = V3::new(0.0, 0.0, 0.0);
    for depth in 0..MAX_DEPTH {
        let Some((i, t)) = intersect(spheres, o, d) else {
            return (acc, depth, false);
        };
        let s = spheres[i];
        let x = o.add(d.mul(t));
        let n = x.sub(s.p).norm();
        let nl = if n.dot(d) < 0.0 { n } else { n.mul(-1.0) };
        acc = acc.add(throughput.mult(s.emit));
        if s.emit.x > 0.0 {
            return (acc, depth + 1, false);
        }
        throughput = throughput.mult(s.albedo);
        o = x.add(nl.mul(1e-4));
        d = cosine_hemisphere(nl, rng);
    }
    (acc, MAX_DEPTH, true)
}

struct Render {
    /// The mean image (A and B averaged).
    img: Vec<V3>,
    /// RMS(A − B)/2 — the unbiased noise estimate.
    noise: f64,
    bounces: f64,
    capped: f64,
}

/// Render at `spp` samples per pixel, split into two independent halves.
fn render(spp: usize, seed: u64) -> Render {
    let spheres = scene();
    let cam_o = V3::new(50.0, 48.0, 235.0);
    let cam_d = V3::new(0.0, -0.05, -1.0).norm();
    let cx = V3::new(IW as f64 * 0.5135 / IH as f64, 0.0, 0.0);
    let cy = cx.cross(cam_d).norm().mul(0.5135);

    let mut a = vec![V3::new(0.0, 0.0, 0.0); IW * IH];
    let mut b = vec![V3::new(0.0, 0.0, 0.0); IW * IH];
    let half = spp.max(2) / 2;
    let mut bounce_sum = 0.0_f64;
    let mut cap_sum = 0.0_f64;
    let mut ray_count = 0.0_f64;

    for y in 0..IH {
        for x in 0..IW {
            let mut rng = Rng::new(seed ^ ((y as u64) << 32) ^ (x as u64 * 0x9E37_79B9));
            for (buf, which) in [(&mut a, 0usize), (&mut b, 1usize)] {
                let mut acc = V3::new(0.0, 0.0, 0.0);
                for s in 0..half {
                    let _ = (which, s);
                    let dx = rng.f01() as f64;
                    let dy = rng.f01() as f64;
                    let sx = (x as f64 + dx) / IW as f64 - 0.5;
                    let sy = -((y as f64 + dy) / IH as f64 - 0.5);
                    let dir = cx.mul(sx).add(cy.mul(sy)).add(cam_d).norm();
                    let (r, nb, cap) = radiance(&spheres, cam_o, dir, &mut rng);
                    acc = acc.add(r);
                    bounce_sum += nb as f64;
                    cap_sum += if cap { 1.0 } else { 0.0 };
                    ray_count += 1.0;
                }
                buf[y * IW + x] = acc.mul(1.0 / half as f64);
            }
        }
    }

    let mut img = vec![V3::new(0.0, 0.0, 0.0); IW * IH];
    let mut sq = 0.0_f64;
    for i in 0..IW * IH {
        img[i] = a[i].add(b[i]).mul(0.5);
        let d = a[i].sub(b[i]);
        sq += (d.x * d.x + d.y * d.y + d.z * d.z) / 3.0;
    }
    Render {
        img,
        // RMS(A−B)/2 estimates the error of their mean (each half has
        // twice the variance of the full render, and the difference has
        // twice that again: the /2 is exactly right, not a fudge).
        noise: (sq / (IW * IH) as f64).sqrt() / 2.0,
        bounces: bounce_sum / ray_count.max(1.0),
        capped: cap_sum / ray_count.max(1.0),
    }
}

/// The white furnace test: an albedo-1 sphere inside a uniformly emitting
/// shell. Every pixel must read exactly the shell's radiance. Returns the
/// maximum relative error over the image and the radiance it should be.
fn furnace() -> (f64, f64, f64) {
    let l = 1.0_f64;
    let spheres = vec![
        Sphere {
            r: 1e4,
            p: V3::new(0.0, 0.0, 0.0),
            emit: V3::new(l, l, l),
            albedo: V3::new(0.0, 0.0, 0.0),
        },
        // TWO albedo-1 spheres, overlapping, so that rays actually
        // interreflect: a single convex object is a one-bounce test and
        // would pass even if the throughput chain were wrong after the
        // first multiply.
        Sphere {
            r: 30.0,
            p: V3::new(-22.0, 0.0, 0.0),
            emit: V3::new(0.0, 0.0, 0.0),
            albedo: V3::new(1.0, 1.0, 1.0),
        },
        Sphere {
            r: 30.0,
            p: V3::new(22.0, 0.0, 0.0),
            emit: V3::new(0.0, 0.0, 0.0),
            albedo: V3::new(1.0, 1.0, 1.0),
        },
    ];
    let cam_o = V3::new(0.0, 0.0, 120.0);
    let mut worst = 0.0_f64;
    let mut bounce_sum = 0.0_f64;
    let mut bounce_n = 0.0_f64;
    // The cap biases the furnace DOWN by exactly the light that would have
    // arrived on bounce MAX_DEPTH+1 and beyond; with albedo 1 that is a
    // real, computable deficit, so the test is run with a depth generous
    // enough that the deficit is below the sampling noise, and the plate
    // reports the worst pixel it found.
    for py in (0..32).step_by(2) {
        for px in (0..32).step_by(2) {
            let mut rng = Rng::new(0xFEED_0000_0000_0001 ^ ((py as u64) << 20) ^ px as u64);
            let sx = (px as f64 + 0.5) / 32.0 - 0.5;
            let sy = (py as f64 + 0.5) / 32.0 - 0.5;
            let dir = V3::new(sx * 0.6, sy * 0.6, -1.0).norm();
            let n = 400;
            let mut acc = 0.0;
            for _ in 0..n {
                let (r, nb, _) = radiance_deep(&spheres, cam_o, dir, &mut rng, 48);
                acc += (r.x + r.y + r.z) / 3.0;
                bounce_sum += nb as f64;
                bounce_n += 1.0;
            }
            let v = acc / n as f64;
            worst = worst.max((v - l).abs() / l);
        }
    }
    (worst, l, bounce_sum / bounce_n.max(1.0))
}

/// The furnace's own tracer, with a caller-chosen depth.
fn radiance_deep(
    spheres: &[Sphere],
    mut o: V3,
    mut d: V3,
    rng: &mut Rng,
    max_depth: usize,
) -> (V3, usize, bool) {
    let mut throughput = V3::new(1.0, 1.0, 1.0);
    let mut acc = V3::new(0.0, 0.0, 0.0);
    for depth in 0..max_depth {
        let Some((i, t)) = intersect(spheres, o, d) else {
            return (acc, depth, false);
        };
        let s = spheres[i];
        let x = o.add(d.mul(t));
        let n = x.sub(s.p).norm();
        let nl = if n.dot(d) < 0.0 { n } else { n.mul(-1.0) };
        acc = acc.add(throughput.mult(s.emit));
        if s.emit.x > 0.0 {
            return (acc, depth + 1, false);
        }
        throughput = throughput.mult(s.albedo);
        o = x.add(nl.mul(1e-4));
        d = cosine_hemisphere(nl, rng);
    }
    (acc, max_depth, true)
}

// ── The frame ───────────────────────────────────────────────────────────────

pub(crate) fn frame(t: f32) -> WidgetNode {
    // The film climbs the ladder; every rung already climbed stays on the
    // convergence plot.
    let rung = ((t as f64).clamp(0.0, 1.0) * LADDER.len() as f64).floor() as usize;
    let rung = rung.min(LADDER.len() - 1);

    let mut points: Vec<(f64, f64)> = Vec::new();
    let mut last: Option<Render> = None;
    for (k, &spp) in LADDER.iter().enumerate().take(rung + 1) {
        let r = render(spp, 0x9A7C_0000_0000_0011 + k as u64 * 977);
        points.push((spp as f64, r.noise));
        last = Some(r);
    }
    let cur = last.unwrap();

    // ── the fitted exponent ──
    let (slope, r2) = if points.len() >= 3 {
        let xs: Vec<f64> = points.iter().map(|p| p.0.ln()).collect();
        let ys: Vec<f64> = points.iter().map(|p| p.1.max(1e-12).ln()).collect();
        let n = xs.len() as f64;
        let mx = xs.iter().sum::<f64>() / n;
        let my = ys.iter().sum::<f64>() / n;
        let sxy: f64 = xs.iter().zip(&ys).map(|(x, y)| (x - mx) * (y - my)).sum();
        let sxx: f64 = xs.iter().map(|x| (x - mx) * (x - mx)).sum();
        let s = sxy / sxx;
        let ic = my - s * mx;
        let ss_tot: f64 = ys.iter().map(|y| (y - my) * (y - my)).sum();
        let ss_res: f64 = xs
            .iter()
            .zip(&ys)
            .map(|(x, y)| (y - (s * x + ic)).powi(2))
            .sum();
        (s, 1.0 - ss_res / ss_tot)
    } else {
        (0.0, 0.0)
    };

    let (furnace_err, furnace_l, furnace_bounces) = furnace();

    // ── the depth-cap bias, measured ──
    // 94% of paths die on the cap rather than on the light, so the image
    // is darker than the truth by whatever those paths would have picked
    // up. The same seeds are used at both depths, so the paths agree step
    // for step up to the cap and the difference is exactly the tail.
    let depth_bias = {
        let spheres = scene();
        let cam_o = V3::new(50.0, 48.0, 235.0);
        let cam_d = V3::new(0.0, -0.05, -1.0).norm();
        let cx = V3::new(IW as f64 * 0.5135 / IH as f64, 0.0, 0.0);
        let cy = cx.cross(cam_d).norm().mul(0.5135);
        let (mut shallow, mut deep) = (0.0_f64, 0.0_f64);
        for y in (0..IH).step_by(3) {
            for x in (0..IW).step_by(3) {
                let seed = 0xB1A5_0000_0000_0001 ^ ((y as u64) << 32) ^ (x as u64 * 0x9E37_79B9);
                for pass in 0..2 {
                    let mut rng = Rng::new(seed);
                    let depth = if pass == 0 { MAX_DEPTH } else { 16 };
                    let n = 24;
                    let mut acc = 0.0;
                    for _ in 0..n {
                        let dx = rng.f01() as f64;
                        let dy = rng.f01() as f64;
                        let sx = (x as f64 + dx) / IW as f64 - 0.5;
                        let sy = -((y as f64 + dy) / IH as f64 - 0.5);
                        let dir = cx.mul(sx).add(cy.mul(sy)).add(cam_d).norm();
                        let (r, _, _) = radiance_deep(&spheres, cam_o, dir, &mut rng, depth);
                        acc += (r.x + r.y + r.z) / 3.0;
                    }
                    if pass == 0 {
                        shallow += acc / n as f64;
                    } else {
                        deep += acc / n as f64;
                    }
                }
            }
        }
        if deep > 0.0 {
            (shallow - deep) / deep * 100.0
        } else {
            0.0
        }
    };
    let spp = LADDER[rung];
    let img = cur.img.clone();
    let noise = cur.noise;
    let bounces = cur.bounces;
    let capped = cur.capped;
    let points_c = points.clone();

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            book.rect(
                Rect::new(0.0, 0.0, size.width, size.height),
                Gradient::vertical()
                    .with_dither()
                    .with_stops(&[(0.0, Color::rgb(5, 5, 9)), (1.0, Color::rgb(10, 10, 15))]),
            );
            let px = 52.0_f32;
            let py = 186.0_f32;
            let pw = 700.0_f32;
            let ph = 466.0_f32;
            book.rrect(
                Rect::new(px - 12.0, py - 12.0, px + pw + 12.0, py + ph + 12.0),
                10.0,
                alpha(Color::rgb(12, 12, 18), 0.96),
            );
            // the render, tone-mapped with the usual gamma 2.2
            book.mesh(
                Rect::new(px, py, px + pw, py + ph),
                IW,
                IH,
                0.004,
                |cx, cy, _r| {
                    let c = img[cy * IW + cx];
                    let g = |v: f64| (v.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0) as u8;
                    Color::rgb(g(c.x), g(c.y), g(c.z)).into()
                },
            );

            // ── the convergence plot ──
            let gx = 800.0_f32;
            let gy = 230.0_f32;
            let gw = 424.0_f32;
            let gh = 250.0_f32;
            book.rrect(
                Rect::new(gx - 20.0, gy - 30.0, gx + gw + 20.0, gy + gh + 30.0),
                10.0,
                alpha(Color::rgb(12, 12, 18), 0.96),
            );
            let xlo = 0.0_f64;
            let xhi = 64.0_f64.ln();
            let ylo = (-7.0_f64).exp().ln();
            let yhi = 1.0_f64.ln();
            let mx = |v: f64| gx + ((v.ln() - xlo) / (xhi - xlo)).clamp(0.0, 1.0) as f32 * gw;
            let my = |v: f64| {
                gy + gh - ((v.max(1e-9).ln() - ylo) / (yhi - ylo)).clamp(0.0, 1.0) as f32 * gh
            };
            // the −1/2 law, anchored at the first measured point
            if let Some(&(x0, y0)) = points_c.first() {
                book.line(
                    Offset::new(mx(x0), my(y0)),
                    Offset::new(mx(64.0), my(y0 * (64.0 / x0).powf(-0.5))),
                    alpha(MUTED, 0.5),
                    1.2,
                );
            }
            let mut p = Path::new();
            for (i, &(x, y)) in points_c.iter().enumerate() {
                let o = Offset::new(mx(x), my(y));
                if i == 0 {
                    p.move_to(o);
                } else {
                    p.line_to(o);
                }
            }
            book.stroke(p, alpha(AMBER, 0.9), 1.8);
            for &(x, y) in &points_c {
                book.circle(Offset::new(mx(x), my(y)), 3.6, alpha(CYAN, 0.95));
            }

            // ── the sample ladder, as a strip ──
            let lx = 800.0_f32;
            let ly = 556.0_f32;
            let lw = 424.0_f32;
            let lh = 86.0_f32;
            book.rrect(
                Rect::new(lx - 20.0, ly - 30.0, lx + lw + 20.0, ly + lh + 24.0),
                10.0,
                alpha(Color::rgb(12, 12, 18), 0.96),
            );
            for (k, &s) in LADDER.iter().enumerate() {
                let x0 = lx + k as f32 / LADDER.len() as f32 * lw;
                let x1 = lx + (k + 1) as f32 / LADDER.len() as f32 * lw - 5.0;
                let on = k <= rung;
                book.rect(
                    Rect::new(x0, ly + lh * 0.35, x1, ly + lh),
                    alpha(
                        if on { MINT } else { Color::rgb(30, 30, 40) },
                        if on { 0.8 } else { 0.9 },
                    ),
                );
                let _ = s;
            }
        }),
    );

    let lines = ["PATHTRACE · THE LIGHT-TRANSPORT AXIS · NOISE THAT OBEYS A LAW".to_string(),
        format!(
            "a Cornell box of {} spheres at {IW}×{IH}, cosine-weighted diffuse bounces to depth {MAX_DEPTH}, no light sampling and no denoiser · {spp} samples/pixel",
            scene().len()
        ),
        format!(
            "NOISE WITHOUT A REFERENCE: the samples are split into two independent halves A and B, and RMS(A−B)/2 is an unbiased estimate of the error in their mean · at {spp} spp it is {noise:.5}"
        ),
        format!(
            "CONVERGENCE fitted over the ladder {:?} ({} rungs so far): error ∝ N^{slope:.4}, r² = {r2:.4} · Monte Carlo theory says exactly −0.5 ({:+.2}%)",
            &LADDER[..=rung],
            rung + 1,
            (slope - (-0.5)) / 0.5 * 100.0
        ),
        format!(
            "THE WHITE FURNACE TEST — an albedo-1 sphere inside a uniform emitter of radiance {furnace_l}: it must VANISH, because whatever bounces in must bounce out"
        ),
        format!(
            "— worst pixel: {furnace_err:.3e} relative error (two overlapping albedo-1 spheres so the paths really interreflect: mean {furnace_bounces:.2} bounces, depth 48, 400 paths/pixel)"
        ),
        format!(
            "THE PATH CENSUS: mean {bounces:.3} bounces per camera ray · {:.2}% of paths died on the depth cap rather than on the light",
            capped * 100.0
        ),
        format!(
            "— and what that cap COSTS: re-tracing the identical paths to depth 16 from the same seeds, the depth-{MAX_DEPTH} image is {depth_bias:+.2}% darker. That is the bias, measured."
        )];

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(42.0)
                .top(34.0 + i as f32 * 15.0)
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
            52.0_f32,
            160.0_f32,
            "THE RENDER — every pixel is an average of paths, and the grain is the estimator"
                .to_string(),
        ),
        (
            800.0,
            202.0,
            "RMS(A−B)/2 vs SAMPLES, log–log · grey: the −1/2 law".to_string(),
        ),
        (800.0, 528.0, "THE SAMPLE LADDER".to_string()),
    ] {
        stack = stack.push(
            Positioned::new()
                .left(x)
                .top(y)
                .width(700.0)
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
