//! Path operations — Paper.js/Skia path ops, After Effects Trim Paths,
//! SVG dashes, geometric hit testing and vector export, photographed.
//!
//! Top row: a circle and a rotating star combined by the four boolean ops
//! (union, intersect, difference, xor) computed on the CPU by
//! `Path::boolean`. Bottom row: a spiral drawn on with `trim` while an arrow
//! rides it with `PathMeasure::point_at_fraction` (auto-orient); marching
//! ants from `dashed` with an animated offset; a pentagram's even-odd vs
//! non-zero coverage sampled with `contains`; and the same drawing exported
//! with `Sketchbook::to_svg` / `to_pdf` (byte counts in the caption).

use std::f32::consts::TAU;

use feature_harness::draw::{circle, grid, page, painted, polygon, star, DIM, HUES, INK};
use vieww_foundation::{FillRule, Offset, Path, PathOp, Size, Sketchbook};

const W: f32 = 200.0;
const H: f32 = 150.0;

fn spiral() -> Path {
    let mut p = Path::new();
    for i in 0..=240 {
        let t = i as f32 / 240.0;
        let a = t * TAU * 3.0;
        let r = 8.0 + t * 58.0;
        let q = Offset::new(W / 2.0 + r * a.cos(), H / 2.0 + r * a.sin());
        if i == 0 {
            p.move_to(q);
        } else {
            p.line_to(q);
        }
    }
    p
}

fn exported_sizes() -> (usize, usize) {
    let mut b = Sketchbook::new();
    b.fill(circle(Offset::new(60.0, 60.0), 40.0).boolean(&star(Offset::new(90.0, 60.0), 45.0, 20.0, 5, 0.0), PathOp::Xor), HUES[0]);
    b.stroke(spiral(), HUES[1], 2.0);
    (b.to_svg(Size::new(W, H)).len(), b.to_pdf(Size::new(W, H)).len())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (svg, pdf) = exported_sizes();
    feature_harness::launch("81 — path ops", Size::new(960.0, 490.0), move |d| {
        let view = feature_harness::clocked(d, 4.0, move |t| {
            let rot = t / 4.0 * TAU / 5.0;
            let ops = [(PathOp::Union, "union"), (PathOp::Intersect, "intersect"), (PathOp::Difference, "difference"), (PathOp::Xor, "xor")];
            let mut items: Vec<_> = ops
                .iter()
                .enumerate()
                .map(|(i, &(op, name))| {
                    painted(&format!("boolean · {name}"), "circle ∘ rotating star, CPU path op", Size::new(W, H), move |g, _| {
                        let a = circle(Offset::new(80.0, 75.0), 48.0);
                        let b = star(Offset::new(122.0, 75.0), 60.0, 26.0, 5, rot);
                        g.stroke(a.clone(), DIM, 1.0);
                        g.stroke(b.clone(), DIM, 1.0);
                        g.fill(a.boolean(&b, op), HUES[i]);
                    })
                })
                .collect();
            let frac = (t / 3.0).min(1.0);
            items.push(painted("trim paths + ride", &format!("draw-on end = {:.0}% · arrow auto-orients", frac * 100.0), Size::new(W, H), move |g, _| {
                let s = spiral();
                g.stroke(s.clone(), DIM.with_alpha(40), 1.0);
                g.stroke(s.trim(0.0, frac), HUES[2], 3.0);
                if let Some(p) = s.measure().point_at_fraction(frac) {
                    let (c, sn) = (p.angle.cos(), p.angle.sin());
                    let tip = p.position + Offset::new(c * 10.0, sn * 10.0);
                    let l = p.position + Offset::new(-sn * 6.0 - c * 4.0, c * 6.0 - sn * 4.0);
                    let r = p.position + Offset::new(sn * 6.0 - c * 4.0, -c * 6.0 - sn * 4.0);
                    let mut tri = Path::new();
                    tri.move_to(tip).line_to(l).line_to(r).close();
                    g.fill(tri, INK);
                }
            }));
            items.push(painted("dashes", "dashed([14, 6, 3, 6], offset) — marching ants", Size::new(W, H), move |g, _| {
                let shape = polygon(Offset::new(W / 2.0, H / 2.0), 60.0, 6, 0.3);
                g.stroke(shape.dashed(&[14.0, 6.0, 3.0, 6.0], t * 40.0), HUES[3], 3.0);
                g.stroke(circle(Offset::new(W / 2.0, H / 2.0), 30.0).dashed(&[4.0, 4.0], -t * 20.0), HUES[5], 2.0);
            }));
            items.push(painted("hit test · even-odd | non-zero", "pentagram sampled with Path::contains", Size::new(W, H), |g, _| {
                for (k, rule) in [FillRule::EvenOdd, FillRule::NonZero].into_iter().enumerate() {
                    let cx = 50.0 + k as f32 * 100.0;
                    let mut p = Path::new();
                    for i in 0..5 {
                        let a = -TAU / 4.0 + i as f32 * TAU * 2.0 / 5.0;
                        let q = Offset::new(cx + 44.0 * a.cos(), 78.0 + 44.0 * a.sin());
                        if i == 0 {
                            p.move_to(q);
                        } else {
                            p.line_to(q);
                        }
                    }
                    p.close();
                    let mut y = 30.0;
                    while y < 126.0 {
                        let mut x = cx - 46.0;
                        while x < cx + 46.0 {
                            let hit = p.contains(Offset::new(x, y), rule);
                            g.circle(Offset::new(x, y), 1.8, if hit { HUES[1] } else { DIM.with_alpha(70) });
                            x += 5.0;
                        }
                        y += 5.0;
                    }
                    g.stroke(p, INK, 1.0);
                }
            }));
            items.push(painted("vector export", &format!("to_svg: {svg} bytes · to_pdf: {pdf} bytes"), Size::new(W, H), |g, _| {
                g.fill(circle(Offset::new(60.0, 60.0), 40.0).boolean(&star(Offset::new(90.0, 60.0), 45.0, 20.0, 5, 0.0), PathOp::Xor), HUES[0]);
                g.stroke(spiral(), HUES[1], 2.0);
            }));
            page("81 · path ops", "booleans, trim/measure, dashes, hit testing and SVG/PDF export — all CPU geometry in vieww-foundation", grid(4, items))
        });
        feature_harness::set_page(d, view);
    })
}
