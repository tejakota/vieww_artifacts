//! Timelines three ways — a GSAP timeline with position parameters,
//! labels and stagger; a Motion Canvas / Manim generator flow compiled to
//! the same timeline; and Blender F-curves with Bézier handles and
//! modifiers — photographed.
//!
//! Left: eight tiles staggered from the centre (`Sequence::stagger`,
//! `StaggerFrom::Center`), then a `"<"`-positioned colour tween and a
//! labelled finale, with every channel's value drawn as a heat strip under
//! a moving playhead. Middle: `Flow::chain`/`all`/`any`/`wait_until` driving
//! a ball and a bar, compiled with `Flow::compile`. Right: four F-curves —
//! auto-clamped, cycles-with-offset, noise, stepped — evaluated at the
//! playhead.

use feature_harness::draw::{grid, page, painted, plot, polyline, xywh, DIM, HUES, INK};
use vieww_animation::fcurve::{CycleMode, FCurve, Modifier};
use vieww_animation::flow::{Flow, TimeEvents};
use vieww_animation::sequence::{Sequence, StaggerFrom};
use vieww_animation::Curve;
use vieww_foundation::{Offset, Size};

const SPAN: f32 = 4.0;

fn gsap() -> Sequence {
    let names: Vec<String> = (0..8).map(|i| format!("tile{i}")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut s = Sequence::new();
    s.stagger(
        &refs,
        0.0,
        1.0,
        0.6,
        0.12,
        StaggerFrom::Center,
        Curve::BACK_OUT,
        0.0,
    );
    s.label("lift", ">");
    s.from_to("hue", 0.0, 1.0, 1.0, Curve::EASE_IN_OUT, "<");
    s.to("spin", 1.0, 1.2, Curve::ELASTIC_OUT, "lift+=0.3");
    s
}

fn flow() -> Sequence {
    let f = Flow::chain([
        Flow::tween("x", 1.0, 1.0, Curve::EASE_IN_OUT),
        Flow::all([
            Flow::tween("y", 1.0, 0.8, Curve::BOUNCE_OUT),
            Flow::tween("bar", 1.0, 1.4, Curve::Linear),
        ]),
        Flow::wait_until("drop"),
        Flow::any([
            Flow::tween("x", 0.0, 0.6, Curve::EASE_IN),
            Flow::tween("y", 0.0, 1.5, Curve::EASE_OUT),
        ]),
    ]);
    let mut ev = TimeEvents::new();
    ev.set("drop", 3.0);
    f.compile(&ev)
}

fn curves() -> Vec<(&'static str, FCurve)> {
    let mut a = FCurve::new();
    a.insert(0.0, 0.0)
        .insert(1.0, 1.0)
        .insert(2.0, 0.2)
        .insert(3.0, 0.8);
    let mut b = FCurve::new();
    b.insert(0.0, 0.0).insert(0.5, 0.4).insert(1.0, 0.25);
    b.modifier(Modifier::Cycles {
        before: None,
        after: Some(CycleMode::RepeatWithOffset),
    });
    let mut c = FCurve::new();
    c.insert(0.0, 0.5).insert(4.0, 0.5);
    c.modifier(Modifier::Noise {
        scale: 0.3,
        strength: 0.6,
        phase: 0.0,
        seed: 3,
    });
    let mut d = FCurve::new();
    d.insert(0.0, 0.0).insert(4.0, 1.0);
    d.modifier(Modifier::Stepped {
        step: 0.5,
        offset: 0.0,
    });
    vec![
        ("auto-clamped", a),
        ("cycles +offset", b),
        ("noise", c),
        ("stepped", d),
    ]
}

fn strip(
    g: &mut vieww_foundation::Sketchbook,
    seq: &Sequence,
    channels: &[&str],
    at: Offset,
    w: f32,
    t: f32,
    total: f32,
) {
    for (row, ch) in channels.iter().enumerate() {
        let y = at.dy + row as f32 * 10.0;
        for k in 0..60 {
            let tt = k as f32 / 60.0 * total;
            let v = seq
                .value_at(ch, tt)
                .and_then(|v| v.scalar())
                .unwrap_or(0.0)
                .clamp(0.0, 1.2);
            let x = at.dx + k as f32 / 60.0 * w;
            g.rect(
                xywh(x, y, w / 60.0 + 0.5, 8.0),
                HUES[row % 6].with_alpha((40.0 + v * 180.0).min(255.0) as u8),
            );
        }
    }
    let px = at.dx + (t / total).min(1.0) * w;
    g.line(
        Offset::new(px, at.dy - 4.0),
        Offset::new(px, at.dy + channels.len() as f32 * 10.0 + 2.0),
        INK,
        1.5,
    );
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    feature_harness::launch("82 — timelines", Size::new(980.0, 480.0), |d| {
        let view = feature_harness::clocked(d, SPAN, |t| {
            let s = gsap();
            let total = s.total_duration();
            let lift = s.label_time("lift").unwrap_or(0.0);
            let tt = t / SPAN * total;
            let a = painted(
                "GSAP timeline",
                &format!("stagger from center · label lift @ {lift:.2}s · total {total:.2}s"),
                Size::new(290.0, 300.0),
                move |g, _| {
                    let hue = s
                        .value_at("hue", tt)
                        .and_then(|v| v.scalar())
                        .unwrap_or(0.0);
                    let spin = s
                        .value_at("spin", tt)
                        .and_then(|v| v.scalar())
                        .unwrap_or(0.0);
                    for i in 0..8 {
                        let v = s
                            .value_at(&format!("tile{i}"), tt)
                            .and_then(|v| v.scalar())
                            .unwrap_or(0.0);
                        let x = 14.0 + i as f32 * 34.0;
                        let h = 20.0 + v * 110.0;
                        let c = HUES[0].lerp_oklab(HUES[1], hue);
                        let r = xywh(x, 150.0 - h, 26.0, h);
                        let cx = Offset::new(x + 13.0, 150.0 - h / 2.0);
                        g.transformed(
                            vieww_foundation::Transform::rotate_around(
                                cx,
                                spin * 0.3 * if i % 2 == 0 { 1.0 } else { -1.0 },
                            ),
                            |g| {
                                g.rrect(r, 5.0, c);
                            },
                        );
                    }
                    let ch: Vec<String> = (0..8)
                        .map(|i| format!("tile{i}"))
                        .chain(["hue".to_owned(), "spin".to_owned()])
                        .collect();
                    let refs: Vec<&str> = ch.iter().map(String::as_str).collect();
                    strip(g, &s, &refs, Offset::new(10.0, 180.0), 270.0, tt, total);
                },
            );
            let f = flow();
            let ftotal = f.total_duration();
            let ft = t / SPAN * ftotal;
            let b = painted(
                "generator flow → timeline",
                &format!("chain · all · wait_until(drop@3s) · any — {ftotal:.2}s"),
                Size::new(290.0, 300.0),
                move |g, _| {
                    let x = f.value_at("x", ft).and_then(|v| v.scalar()).unwrap_or(0.0);
                    let y = f.value_at("y", ft).and_then(|v| v.scalar()).unwrap_or(0.0);
                    let bar = f
                        .value_at("bar", ft)
                        .and_then(|v| v.scalar())
                        .unwrap_or(0.0);
                    g.stroke(
                        polyline(&[
                            Offset::new(20.0, 30.0),
                            Offset::new(20.0, 150.0),
                            Offset::new(270.0, 150.0),
                        ]),
                        DIM,
                        1.0,
                    );
                    g.circle(
                        Offset::new(30.0 + x * 220.0, 140.0 - y * 100.0),
                        12.0,
                        HUES[2],
                    );
                    g.rrect(xywh(20.0, 158.0, 250.0 * bar, 8.0), 4.0, HUES[3]);
                    strip(
                        g,
                        &f,
                        &["x", "y", "bar"],
                        Offset::new(10.0, 185.0),
                        270.0,
                        ft,
                        ftotal,
                    );
                },
            );
            let c = painted(
                "F-curves + modifiers",
                "Blender graph editor: handles, cycles, noise, stepped",
                Size::new(290.0, 300.0),
                move |g, _| {
                    for (i, (_, fc)) in curves().iter().enumerate() {
                        let y0 = 8.0 + i as f32 * 72.0;
                        let vals: Vec<f32> = (0..=120)
                            .map(|k| fc.evaluate(k as f32 / 120.0 * 4.0))
                            .collect();
                        let (lo, hi) = (-0.2, 1.4);
                        g.rect(xywh(10.0, y0, 270.0, 62.0), DIM.with_alpha(20));
                        g.stroke(
                            plot(&vals, Offset::new(10.0, y0), Size::new(270.0, 62.0), lo, hi),
                            HUES[i],
                            2.0,
                        );
                        for k in fc.keys() {
                            let kx = 10.0 + k.time / 4.0 * 270.0;
                            if kx <= 280.0 {
                                g.circle(
                                    Offset::new(kx, y0 + 62.0 - (k.value - lo) / (hi - lo) * 62.0),
                                    3.0,
                                    INK,
                                );
                            }
                        }
                        let v = fc.evaluate(t);
                        g.circle(
                            Offset::new(
                                10.0 + t / 4.0 * 270.0,
                                y0 + 62.0 - (v - lo) / (hi - lo) * 62.0,
                            ),
                            5.0,
                            HUES[i],
                        );
                    }
                },
            );
            page(
                "82 · timelines",
                "vieww-animation: sequence (GSAP), flow (Motion Canvas/Manim), fcurve (Blender)",
                grid(3, vec![a, b, c]),
            )
        });
        feature_harness::set_page(d, view);
    })
}
