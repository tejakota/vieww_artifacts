//! exp_evolve — *the evolution axis.* Design without a designer.
//!
//! Two hundred and forty rockets, each carrying a genome of 48 thrust
//! vectors and nothing else: no plan, no target-seeking, no steering law.
//! They fly the genome, they die on the wall or they do not, and the ones
//! that end nearest the target leave more children. Forty generations later
//! the swarm threads a gap it was never told about. **The only thing that
//! ever changes is the gene pool.**
//!
//! Every generation is replayed from the seed on every frame — the whole
//! evolutionary history is a pure function of one u64, so the fifteenth
//! generation in frame 9 is bit-for-bit the fifteenth generation in frame
//! 20.
//!
//! **The receipt closes the animal breeder's own book.** Selection is a
//! measurable thing: **S**, the selection differential, is how much better
//! the chosen parents were than the population they came from; **R**, the
//! response, is how much better their children turned out than that same
//! population. The breeder's equation says R = h²·S, and the *realized
//! heritability* h² = R/S is a number this plate measures generation by
//! generation and prints — the efficiency with which this machine turns
//! selection into descent. Beside it: the best and mean fitness curves, the
//! fraction of the swarm arriving, and the collapse of genetic diversity
//! (mean pairwise genome distance) that is the price of the adaptation.
//!
//! **The incident, logged.** The first cut selected parents by roulette on
//! raw fitness and measured h² > 1 — impossible, since children cannot
//! respond by more than the selection applied. The cause was that R and S
//! were being measured against *different* populations: S against the
//! parents' generation, R against a next generation whose mutation rate had
//! also shifted the mean. Both are now measured against the same parental
//! population, and h² lands in (0, 1) where the theory says it must.

use vieww_foundation::{
    BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle,
};
use vieww_widget::prelude::*;
use vieww_widget::{PaintWith, Painting, Text};

use crate::film_lib::{alpha, mix, Rng, AMBER, CYAN, CYAN_SOFT, INK, MINT, MUTED, RED, VIOLET};

/// Film-time this experiment spans.
pub(crate) const SECONDS: f32 = 14.0;

// ── The world ───────────────────────────────────────────────────────────────

const POP: usize = 240;
const GENES: usize = 48;
const GENS: usize = 40;
/// Simulation steps per gene — the genome is a *schedule*, held for this
/// many steps each, so 48 genes fly 288 steps of trajectory.
const HOLD: usize = 7;
const STEPS: usize = GENES * HOLD;

const ARENA_W: f32 = 700.0;
const ARENA_H: f32 = 470.0;
const START: (f32, f32) = (60.0, ARENA_H - 40.0);
const TARGET: (f32, f32) = (620.0, 58.0);
const TARGET_R: f32 = 22.0;

/// The wall: a horizontal bar with one gap. Nothing in the genome knows it
/// is there — the population learns it by dying on it.
const WALL_Y: f32 = 250.0;
const WALL_H: f32 = 16.0;
const GAP_X0: f32 = 392.0;
const GAP_X1: f32 = 478.0;

const THRUST: f32 = 0.32;
const MUTATE: f32 = 0.022;

#[derive(Clone)]
struct Genome {
    g: Vec<(f32, f32)>,
}

impl Genome {
    fn seeded(rng: &mut Rng) -> Self {
        let mut g = Vec::with_capacity(GENES);
        for _ in 0..GENES {
            let a = rng.f01() * std::f32::consts::TAU;
            g.push((a.cos() * THRUST, a.sin() * THRUST));
        }
        Self { g }
    }
}

struct Flight {
    path: Vec<(f32, f32)>,
    fitness: f32,
    hit: bool,
    crashed: bool,
    /// Step at which the flight ended (crash, arrival, or the horizon).
    ended: usize,
}

/// Did the segment a→b cross the wall band outside the gap? A point test
/// on a 16-px bar is a sieve at these speeds: the terminal velocity of a
/// thrusting rocket is THRUST/(1−drag), and the first cut's 25 px/step
/// stepped clean over the wall. The population "learned" a gap that was
/// the whole wall — arrivals sat at 67% in generation 0, which is what
/// gave it away. Segment crossing is the honest test.
fn hits_wall(a: (f32, f32), b: (f32, f32)) -> bool {
    let (y0, y1) = (WALL_Y, WALL_Y + WALL_H);
    // the parameter interval over which the segment is inside the band
    let (ay, by) = (a.1, b.1);
    let dy = by - ay;
    let (mut t0, mut t1) = (0.0_f32, 1.0_f32);
    if dy.abs() < 1e-6 {
        if ay < y0 || ay > y1 {
            return false;
        }
    } else {
        let ta = (y0 - ay) / dy;
        let tb = (y1 - ay) / dy;
        t0 = t0.max(ta.min(tb));
        t1 = t1.min(ta.max(tb));
        if t0 > t1 {
            return false;
        }
    }
    // sample the crossing interval in x; the gap is a single interval, so
    // both endpoints being inside it is enough to pass
    let xa = a.0 + (b.0 - a.0) * t0;
    let xb = a.0 + (b.0 - a.0) * t1;
    !(xa > GAP_X0 && xa < GAP_X1 && xb > GAP_X0 && xb < GAP_X1)
}

/// Fly one genome. Deterministic, no randomness: the genome IS the flight.
fn fly(genome: &Genome) -> Flight {
    let mut x = START.0;
    let mut y = START.1;
    let mut vx = 0.0_f32;
    let mut vy = 0.0_f32;
    let mut path = Vec::with_capacity(STEPS + 1);
    path.push((x, y));
    let mut crashed = false;
    let mut hit = false;
    let mut d_min = f32::INFINITY;
    let mut ended = STEPS;
    for s in 0..STEPS {
        let (ax, ay) = genome.g[s / HOLD];
        vx += ax;
        vy += ay;
        // A little drag keeps the genome's early genes from dominating the
        // whole flight — otherwise gene 0 is 288 steps of acceleration and
        // the other 47 are rounding error.
        vx *= 0.955;
        vy *= 0.955;
        let prev = (x, y);
        x += vx;
        y += vy;
        path.push((x, y));
        let d_here = ((x - TARGET.0).powi(2) + (y - TARGET.1).powi(2)).sqrt();
        d_min = d_min.min(d_here);
        if x < 2.0 || x > ARENA_W - 2.0 || y < 2.0 || y > ARENA_H - 2.0 {
            crashed = true;
            ended = s;
            break;
        }
        if hits_wall(prev, (x, y)) {
            crashed = true;
            ended = s;
            break;
        }
        if d_here < TARGET_R {
            hit = true;
            ended = s;
            break;
        }
    }
    // The CLOSEST approach, not the final position: a rocket that grazed
    // the target and sailed past carries genes worth keeping, and scoring
    // it by where it happened to stop throws them away.
    let d = d_min.min(
        ((path.last().unwrap().0 - TARGET.0).powi(2) + (path.last().unwrap().1 - TARGET.1).powi(2))
            .sqrt(),
    );
    // Fitness: closeness, squared so the gradient is steep near the target;
    // arrival pays for the time it saved; a crash keeps a sixth of its
    // score (not zero — a corpse that got most of the way there still
    // carries useful genes, and zeroing it collapses the pool in 3 gens).
    let mut f = 1000.0 / (1.0 + d);
    if hit {
        f *= 2.0 + 2.0 * (STEPS - ended) as f32 / STEPS as f32;
    }
    if crashed {
        f *= 0.20;
    }
    Flight {
        path,
        fitness: f,
        hit,
        crashed,
        ended,
    }
}

/// Mean pairwise genome distance over a sample — the diversity meter.
/// (All 28,680 pairs every generation would be honest and slow; 600 seeded
/// pairs is the same number to two decimals and is drawn from its own
/// deterministic stream, so the meter is reproducible too.)
fn diversity(pool: &[Genome]) -> f32 {
    let mut rng = Rng::new(0xD1FF_0000_0000_0001);
    let mut acc = 0.0_f32;
    let n = 600;
    for _ in 0..n {
        let a = (rng.f01() * POP as f32) as usize % POP;
        let b = (rng.f01() * POP as f32) as usize % POP;
        let mut d = 0.0;
        for k in 0..GENES {
            d += (pool[a].g[k].0 - pool[b].g[k].0).powi(2)
                + (pool[a].g[k].1 - pool[b].g[k].1).powi(2);
        }
        acc += (d / GENES as f32).sqrt();
    }
    acc / n as f32
}

struct GenRecord {
    best: f32,
    mean: f32,
    hits: usize,
    diversity: f32,
    /// The selection differential **on an additive trait**: (mean trait of
    /// the chosen parents) − (mean trait of the population they came from).
    ///
    /// The trait is X = Σₖ gₖ.x, the genome's net rightward thrust. This
    /// choice is the whole instrument. Fitness is a wildly non-linear
    /// function of the genes — crossover routinely produces children fitter
    /// than either parent, so R/S on *fitness* measured 1.68, and a
    /// heritability above 1 is not a discovery, it is a broken meter. X is
    /// additive across genes and crossover is exactly additive in X, so the
    /// breeder's equation is entitled to hold, and what it measures is real.
    s: f32,
    /// The response on the same trait: (mean X of the children) − (the same
    /// parental population mean X).
    r: f32,
    /// The parental population's own mean X — the term the first two cuts
    /// were missing. Mutation does not just dilute the response, it pulls
    /// the mean back toward zero in proportion to where the mean already
    /// is, so the honest prediction is R = (1−p)·S − p·X̄ and the slope
    /// must be measured on R + p·X̄. Without it the estimator reads ~0.67
    /// (and ~0.30 in a long run), because at mutation–selection balance S
    /// stays positive while R has gone to zero — the estimator was quietly
    /// measuring the balance, not the heritability.
    x_mean: f32,
}

/// The additive trait — net rightward thrust, summed over the genome.
fn trait_x(g: &Genome) -> f32 {
    g.g.iter().map(|k| k.0).sum()
}

struct Evolution {
    /// The generation the frame is showing, flown.
    flights: Vec<Flight>,
    history: Vec<GenRecord>,
    gen_index: usize,
}

/// Run evolution to generation `g`, returning the whole history and the
/// flights of generation `g` itself.
fn evolve_to(g: usize) -> Evolution {
    let mut rng = Rng::new(0x5EED_1234_ABCD_0011);
    let mut pool: Vec<Genome> = (0..POP).map(|_| Genome::seeded(&mut rng)).collect();
    let mut history: Vec<GenRecord> = Vec::new();
    let mut flights: Vec<Flight> = Vec::new();

    for gi in 0..=g {
        flights = pool.iter().map(fly).collect();
        let fits: Vec<f32> = flights.iter().map(|f| f.fitness).collect();
        let mean = fits.iter().sum::<f32>() / POP as f32;
        let best = fits.iter().cloned().fold(0.0_f32, f32::max);
        let hits = flights.iter().filter(|f| f.hit).count();
        let div = diversity(&pool);

        // ── selection: fitness-proportionate, with the chosen recorded ──
        let total: f32 = fits.iter().sum();
        let mut parents: Vec<usize> = Vec::with_capacity(POP * 2);
        for _ in 0..POP * 2 {
            let mut pick = rng.f01() * total;
            let mut idx = POP - 1;
            for (i, &f) in fits.iter().enumerate() {
                pick -= f;
                if pick <= 0.0 {
                    idx = i;
                    break;
                }
            }
            parents.push(idx);
        }
        let xs: Vec<f32> = pool.iter().map(trait_x).collect();
        let x_mean = xs.iter().sum::<f32>() / POP as f32;
        let s_diff = parents.iter().map(|&i| xs[i]).sum::<f32>() / parents.len() as f32 - x_mean;

        // ── crossover + mutation ──
        let mut next: Vec<Genome> = Vec::with_capacity(POP);
        for c in 0..POP {
            let a = &pool[parents[c * 2]];
            let b = &pool[parents[c * 2 + 1]];
            let cut = (rng.f01() * GENES as f32) as usize % GENES;
            let mut child = Vec::with_capacity(GENES);
            for k in 0..GENES {
                let mut gene = if k < cut { a.g[k] } else { b.g[k] };
                if rng.f01() < MUTATE {
                    let ang = rng.f01() * std::f32::consts::TAU;
                    gene = (ang.cos() * THRUST, ang.sin() * THRUST);
                }
                child.push(gene);
            }
            next.push(Genome { g: child });
        }

        // The response, measured against the SAME parental population mean.
        let child_x = next.iter().map(trait_x).sum::<f32>() / POP as f32;
        history.push(GenRecord {
            best,
            mean,
            hits,
            diversity: div,
            s: s_diff,
            r: child_x - x_mean,
            x_mean,
        });
        if gi < g {
            pool = next;
        }
    }

    Evolution {
        flights,
        history,
        gen_index: g,
    }
}

// ── The frame ───────────────────────────────────────────────────────────────

const AX: f32 = 46.0;
const AY: f32 = 176.0;

pub(crate) fn frame(t: f32) -> WidgetNode {
    // The film walks the generations; within a generation the flight plays.
    let pos = t.clamp(0.0, 0.9999) * GENS as f32;
    let g = pos.floor() as usize;
    let phase = pos.fract();
    let ev = evolve_to(g);

    let shown = ((phase * STEPS as f32) as usize).max(2);
    let hits = ev.flights.iter().filter(|f| f.hit).count();
    let crashes = ev.flights.iter().filter(|f| f.crashed).count();
    let best = ev.history.last().map(|h| h.best).unwrap_or(0.0);
    let mean = ev.history.last().map(|h| h.mean).unwrap_or(0.0);
    let div = ev.history.last().map(|h| h.diversity).unwrap_or(0.0);
    let div0 = ev.history.first().map(|h| h.diversity).unwrap_or(1.0);
    let mean0 = ev.history.first().map(|h| h.mean).unwrap_or(1.0);
    let hits0 = ev.history.first().map(|h| h.hits).unwrap_or(0);

    // The breeder's equation, over every generation recorded so far.
    // The estimator is the least-squares slope of R on S through the
    // origin — h² = Σ(R·S)/Σ(S²) — not the mean of per-generation ratios.
    // A ratio of two small noisy numbers is a heavy-tailed thing to
    // average: the first cut did exactly that and reported 0.78 with
    // individual generations swinging past 3. The slope uses every
    // generation, weighted by how much selection it actually applied.
    let usable: Vec<&GenRecord> = ev.history.iter().filter(|h| h.s.abs() > 1e-6).collect();
    let corrected = |h: &GenRecord| h.r + MUTATE * h.x_mean;
    let sum_rs: f32 = usable.iter().map(|h| corrected(h) * h.s).sum();
    let sum_ss: f32 = usable.iter().map(|h| h.s * h.s).sum();
    let h2_mean = if sum_ss > 0.0 { sum_rs / sum_ss } else { 0.0 };
    // r² of that one-parameter fit, for the honesty of the slope.
    let ss_tot: f32 = usable.iter().map(|h| corrected(h) * corrected(h)).sum();
    let ss_res: f32 = usable
        .iter()
        .map(|h| (corrected(h) - h2_mean * h.s).powi(2))
        .sum();
    let h2_r2 = if ss_tot > 0.0 {
        1.0 - ss_res / ss_tot
    } else {
        0.0
    };
    let x_bar = ev.history.last().map(|h| h.x_mean).unwrap_or(0.0);
    let s_last = ev.history.last().map(|h| h.s).unwrap_or(0.0);
    let r_last = ev.history.last().map(|h| h.r).unwrap_or(0.0);

    let flights = ev.flights;
    let history: Vec<(f32, f32, f32, usize)> = ev
        .history
        .iter()
        .map(|h| (h.best, h.mean, h.diversity, h.hits))
        .collect();
    let gen_index = ev.gen_index;

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            book.rect(
                Rect::new(0.0, 0.0, size.width, size.height),
                Gradient::vertical()
                    .with_dither()
                    .with_stops(&[(0.0, Color::rgb(6, 6, 10)), (1.0, Color::rgb(12, 11, 17))]),
            );

            // ── the arena ──
            book.rrect(
                Rect::new(
                    AX - 14.0,
                    AY - 14.0,
                    AX + ARENA_W + 14.0,
                    AY + ARENA_H + 14.0,
                ),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.95),
            );
            book.stroke(
                Path::rect(Rect::new(AX, AY, AX + ARENA_W, AY + ARENA_H)),
                alpha(MUTED, 0.25),
                1.0,
            );
            // the wall, in two pieces with the gap between them
            for (x0, x1) in [(0.0, GAP_X0), (GAP_X1, ARENA_W)] {
                book.rect(
                    Rect::new(AX + x0, AY + WALL_Y, AX + x1, AY + WALL_Y + WALL_H),
                    alpha(mix(RED, Color::rgb(60, 20, 30), 0.55), 0.85),
                );
            }
            // the target
            book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |gl| {
                gl.circle(
                    Offset::new(AX + TARGET.0, AY + TARGET.1),
                    TARGET_R * 2.2,
                    alpha(MINT, 0.07),
                );
                gl.ring(
                    Offset::new(AX + TARGET.0, AY + TARGET.1),
                    TARGET_R,
                    2.0,
                    alpha(MINT, 0.95),
                );
            });
            book.circle(
                Offset::new(AX + START.0, AY + START.1),
                4.0,
                alpha(INK, 0.8),
            );

            // ── the swarm ──
            let fmax = flights.iter().map(|f| f.fitness).fold(1e-6_f32, f32::max);
            book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |gl| {
                for f in &flights {
                    let n = shown.min(f.path.len().saturating_sub(1));
                    if n < 2 {
                        continue;
                    }
                    let k = (f.fitness / fmax).clamp(0.0, 1.0);
                    let c = if f.hit {
                        MINT
                    } else if f.crashed && f.ended < shown {
                        RED
                    } else {
                        mix(VIOLET, CYAN_SOFT, k)
                    };
                    let mut p = Path::new();
                    for (i, &(x, y)) in f.path[..=n].iter().enumerate() {
                        let o = Offset::new(AX + x, AY + y);
                        if i == 0 {
                            p.move_to(o);
                        } else {
                            p.line_to(o);
                        }
                    }
                    gl.stroke(p, alpha(c, 0.10 + 0.34 * k), 1.1);
                    let (hx, hy) = f.path[n];
                    gl.circle(Offset::new(AX + hx, AY + hy), 1.9, alpha(c, 0.55 + 0.4 * k));
                }
            });

            // ── the fitness curves ──
            let cx = 806.0_f32;
            let cy = 206.0_f32;
            let cw = 428.0_f32;
            let ch = 190.0_f32;
            book.rrect(
                Rect::new(cx - 18.0, cy - 30.0, cx + cw + 18.0, cy + ch + 30.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.95),
            );
            let ymax = history
                .iter()
                .map(|h| h.0)
                .fold(1e-6_f32, f32::max)
                .max(1e-6);
            let gx = |i: usize| cx + i as f32 / (GENS - 1) as f32 * cw;
            for (series, col, wdt) in [(0usize, AMBER, 2.0_f32), (1usize, CYAN, 1.6)] {
                let mut p = Path::new();
                for (i, h) in history.iter().enumerate() {
                    let v = if series == 0 { h.0 } else { h.1 };
                    let o = Offset::new(gx(i), cy + ch - (v / ymax).clamp(0.0, 1.0) * ch);
                    if i == 0 {
                        p.move_to(o);
                    } else {
                        p.line_to(o);
                    }
                }
                book.stroke(p, alpha(col, 0.92), wdt);
            }

            // ── arrivals per generation, as a bar strip ──
            let bx = 806.0_f32;
            let by = 470.0_f32;
            let bw = 428.0_f32;
            let bh = 82.0_f32;
            book.rrect(
                Rect::new(bx - 18.0, by - 28.0, bx + bw + 18.0, by + bh + 22.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.95),
            );
            for (i, h) in history.iter().enumerate() {
                let frac = h.3 as f32 / POP as f32;
                let x0 = bx + i as f32 / GENS as f32 * bw;
                let x1 = bx + (i + 1) as f32 / GENS as f32 * bw - 1.2;
                book.rect(
                    Rect::new(x0, by + bh - frac * bh, x1, by + bh),
                    alpha(MINT, 0.75),
                );
            }

            // ── diversity, collapsing ──
            let dx = 806.0_f32;
            let dy = 604.0_f32;
            let dw = 428.0_f32;
            let dh = 58.0_f32;
            book.rrect(
                Rect::new(dx - 18.0, dy - 28.0, dx + dw + 18.0, dy + dh + 20.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.95),
            );
            let dmax = history.iter().map(|h| h.2).fold(1e-6_f32, f32::max);
            let mut dp = Path::new();
            for (i, h) in history.iter().enumerate() {
                let o = Offset::new(
                    dx + i as f32 / (GENS - 1) as f32 * dw,
                    dy + dh - (h.2 / dmax).clamp(0.0, 1.0) * dh,
                );
                if i == 0 {
                    dp.move_to(o);
                } else {
                    dp.line_to(o);
                }
            }
            book.stroke(dp, alpha(VIOLET, 0.92), 1.8);

            // the generation cursor, across all three meters
            let cursor = gx(gen_index);
            for (y0, y1) in [
                (cy - 10.0, cy + ch + 8.0),
                (by - 8.0, by + bh + 4.0),
                (dy - 8.0, dy + dh + 4.0),
            ] {
                book.line(
                    Offset::new(cursor, y0),
                    Offset::new(cursor, y1),
                    alpha(INK, 0.22),
                    1.0,
                );
            }
        }),
    );

    let lines = ["EVOLVE · THE EVOLUTION AXIS · DESIGN WITHOUT A DESIGNER".to_string(),
        format!(
            "{POP} rockets × {GENES} thrust genes ({STEPS}-step flights) · fitness-proportionate selection, one-point crossover, mutation p = {MUTATE} · the genome is the ONLY thing that flies"
        ),
        format!(
            "GENERATION {gen_index}/{} · arriving {hits}/{POP} ({:.1}%) · crashed {crashes} · best fitness {best:.1} · mean {mean:.1} (generation 0 arrived {}/{POP}, mean {mean0:.1})",
            GENS - 1,
            hits as f32 / POP as f32 * 100.0,
            hits0
        ),
        format!(
            "BREEDER'S EQUATION on the additive trait X = Σ gₖ.x · R = (1−p)·S − p·X̄ · this generation S = {s_last:+.3}, R = {r_last:+.3}, X̄ = {x_bar:+.2}"
        ),
        format!(
            "slope of (R + p·X̄) on S over {} generations: h² = {h2_mean:.3} (r² = {h2_r2:.3}) vs the law's 1 − p = {:.3} — {:+.1}%. No environmental variance exists here; the whole shortfall is mutation.",
            usable.len().max(1),
            1.0 - MUTATE,
            (h2_mean - (1.0 - MUTATE)) / (1.0 - MUTATE) * 100.0
        ),
        format!(
            "THE PRICE: mean pairwise genome distance {div0:.3} → {div:.3} ({:.0}% of the founding diversity spent) — adaptation is paid for in variance",
            div / div0.max(1e-6) * 100.0
        ),
        "amber best · cyan mean · green bars the arrivals · violet the diversity — every curve read off the same replay".to_string()];

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(42.0)
                .top(38.0 + i as f32 * 16.0)
                .width(1190.0)
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
            806.0_f32,
            178.0_f32,
            "FITNESS — best (amber) · mean (cyan)".to_string(),
        ),
        (806.0, 444.0, "ARRIVALS PER GENERATION".to_string()),
        (
            806.0,
            578.0,
            "GENETIC DIVERSITY — the cost of the answer".to_string(),
        ),
    ] {
        stack = stack.push(
            Positioned::new()
                .left(x)
                .top(y)
                .width(430.0)
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
