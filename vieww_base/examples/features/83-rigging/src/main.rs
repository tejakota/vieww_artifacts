//! Character-animation plumbing — Unity Mecanim layers and blend trees,
//! Blender's NLA, constraints and drivers, After Effects expressions and
//! TouchDesigner CHOPs — photographed.
//!
//! 1. `Animator`: a base locomotion layer (idle → walk/run 1D blend tree
//!    on `speed`, with a jump trigger through Any State) and an additive
//!    "wave" layer masked to the arm. The stick figure is drawn from the
//!    animator's channel output; the caption names the current state.
//! 2. `Nla`: three tracks of strips (replace, add, multiply) with blend-in/
//!    out, evaluated to one channel and plotted under a playhead.
//! 3. `ConstraintStack`: a bone that tracks a moving target within a
//!    rotation limit, a follower kept at a fixed distance, and a marker
//!    riding a path with auto-orient.
//! 4. `Expr` / `Driver`: `wiggle`, `sin`, `smoothstep` expressions plotted,
//!    and a driver feeding one channel from another.
//! 5. CHOPs: a noisy step through Lag, the 1€ filter, an ADSR envelope and
//!    sample-and-hold.

use std::f32::consts::{PI, TAU};

use feature_harness::draw::{circle, grid, page, painted, plot, polyline, star, xywh, DIM, HUES, INK};
use vieww_animation::animator::{Animator, Channels, Clip, Condition, Layer, LayerMode, Motion, Param, Transition};
use vieww_animation::channels::{Chop, Envelope, Lag, OneEuro, SampleHold};
use vieww_animation::constraints::{Constraint, ConstraintStack, DistanceMode, Targets};
use vieww_animation::expr::{Driver, Expr, Scope};
use vieww_animation::nla::{Nla, Strip, StripBlend, Track};
use vieww_animation::skeletal::BoneTransform;
use vieww_animation::{Keyframe, Keyframes};
use vieww_foundation::{Offset, Path, Size, Sketchbook};

const SPAN: f32 = 6.0;

fn kf(points: &[(f32, f32)]) -> Keyframes<f32> {
    let mut k = Keyframes::new(points[0].1);
    for &(t, v) in &points[1..] {
        k = k.with(Keyframe::to(t, v));
    }
    k
}

fn gait(stride: f32, bob: f32, period: f32) -> Clip {
    let h = period / 2.0;
    Clip::new()
        .track("leg.l", kf(&[(0.0, stride), (h, -stride), (period, stride)]))
        .track("leg.r", kf(&[(0.0, -stride), (h, stride), (period, -stride)]))
        .track("body.y", kf(&[(0.0, 0.0), (h / 2.0, -bob), (h, 0.0), (h * 1.5, -bob), (period, 0.0)]))
        .track("arm", kf(&[(0.0, -stride * 0.8), (h, stride * 0.8), (period, -stride * 0.8)]))
}

fn animator() -> Animator {
    let idle = Clip::new().track("body.y", kf(&[(0.0, 0.0), (1.0, -2.0), (2.0, 0.0)])).track("leg.l", kf(&[(0.0, 0.1)])).track("leg.r", kf(&[(0.0, -0.1)])).track("arm", kf(&[(0.0, 0.2)]));
    let jump = Clip::new().track("body.y", kf(&[(0.0, 0.0), (0.3, -40.0), (0.6, 0.0)])).track("leg.l", kf(&[(0.0, 0.6)])).track("leg.r", kf(&[(0.0, -0.6)]));
    let base = Layer::new("base", "idle", Motion::Clip(idle))
        .state("move", Motion::Blend1D { parameter: "speed".into(), children: vec![(0.0, gait(0.35, 3.0, 1.0)), (1.0, gait(0.8, 8.0, 0.6))] })
        .state("jump", Motion::Clip(jump))
        .transition(Transition::new("idle", "move", 0.3).when(Condition::Greater("speed".into(), 0.05)))
        .transition(Transition::new("move", "idle", 0.3).when(Condition::Less("speed".into(), 0.05)))
        .transition(Transition::from_any("jump", 0.1).when(Condition::Trigger("jump".into())))
        .transition(Transition::new("jump", "move", 0.2).exit_time(1.0));
    let wave = Layer::new("wave", "wave", Motion::Clip(Clip::new().track("arm", kf(&[(0.0, -1.6), (0.25, -2.4), (0.5, -1.6)]))))
        .mode(LayerMode::Additive)
        .mask(&["arm"])
        .weight(0.0);
    Animator::new().layer(base).layer(wave).param("speed", Param::Float(0.0)).param("jump", Param::Trigger(false))
}

/// Run the animator from 0 to `t` with a scripted parameter track.
fn animate(t: f32) -> (Channels, String, f32) {
    let mut a = animator();
    let dt = 1.0 / 60.0;
    let mut now = 0.0;
    let mut jumped = false;
    let mut speed = 0.0;
    while now < t {
        speed = if now < 1.0 { 0.0 } else { ((now - 1.0) / 2.0).min(1.0) };
        a.set_float("speed", speed);
        if now >= 3.6 && !jumped {
            a.fire("jump");
            jumped = true;
        }
        a.set_layer_weight(1, if (4.5..6.0).contains(&now) { ((now - 4.5) * 3.0).min(1.0) } else { 0.0 });
        a.advance(dt);
        now += dt;
    }
    (a.output(), a.layers()[0].current().to_owned(), speed)
}

fn figure(g: &mut Sketchbook, ch: &Channels, at: Offset) {
    let v = |k: &str| ch.get(k).copied().unwrap_or(0.0);
    let hip = at + Offset::new(0.0, v("body.y"));
    let neck = hip + Offset::new(0.0, -50.0);
    for (k, c) in [("leg.l", HUES[0]), ("leg.r", HUES[1])] {
        let a = v(k);
        let foot = hip + Offset::new(a.sin() * 45.0, a.cos() * 45.0);
        g.line(hip, foot, c, 5.0);
    }
    g.line(hip, neck, INK, 6.0);
    let a = v("arm");
    let hand = neck + Offset::new(a.sin() * 38.0, a.cos() * 38.0);
    g.line(neck + Offset::new(0.0, 6.0), hand, HUES[3], 4.0);
    g.circle(neck + Offset::new(0.0, -14.0), 11.0, INK);
    g.line(Offset::new(at.dx - 90.0, at.dy + 46.0), Offset::new(at.dx + 90.0, at.dy + 46.0), DIM, 1.0);
}

fn nla() -> Nla {
    let base = Clip::new().track("v", kf(&[(0.0, 0.2), (2.0, 0.8), (4.0, 0.2)]));
    let bump = Clip::new().track("v", kf(&[(0.0, 0.0), (0.5, 0.3), (1.0, 0.0)]));
    let gain = Clip::new().track("v", kf(&[(0.0, 1.0), (1.0, 0.5)]));
    let mut s1 = Strip::new("base", base, 0.0);
    s1.repeat = 1.5;
    let mut s2 = Strip::new("bump", bump, 1.0);
    s2.blend = StripBlend::Add;
    s2.repeat = 3.0;
    s2.blend_in = 0.3;
    s2.blend_out = 0.3;
    let mut s3 = Strip::new("damp", gain, 4.0);
    s3.blend = StripBlend::Multiply;
    s3.blend_in = 0.5;
    Nla::new().track(Track::new("base").strip(s1)).track(Track::new("layer").strip(s2)).track(Track::new("mul").strip(s3))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    feature_harness::launch("83 — rigging", Size::new(980.0, 560.0), |d| {
        let view = feature_harness::clocked(d, SPAN, |t| {
            let (ch, state, speed) = animate(t);
            let p1 = painted("Animator · layers + blend tree", &format!("state {state} · speed {speed:.2} · +wave layer (additive, arm mask)"), Size::new(220.0, 180.0), move |g, _| {
                figure(g, &ch, Offset::new(110.0, 110.0));
            });

            let n = nla();
            let len = n.length();
            let p2 = painted("NLA · strips", &format!("replace · add(blend in/out) · multiply — {len:.1}s"), Size::new(220.0, 180.0), move |g, _| {
                let base = Channels::new();
                for (row, (s, e, c)) in [(0.0, 6.0, HUES[0]), (1.0, 4.0, HUES[2]), (4.0, 5.0, HUES[3])].iter().enumerate() {
                    let y = 10.0 + row as f32 * 14.0;
                    g.rrect(xywh(10.0 + s / 6.0 * 200.0, y, (e - s) / 6.0 * 200.0, 10.0), 3.0, c.with_alpha(200));
                }
                let vals: Vec<f32> = (0..=120).map(|k| n.evaluate(k as f32 / 20.0, &base).get("v").copied().unwrap_or(0.0)).collect();
                g.stroke(plot(&vals, Offset::new(10.0, 60.0), Size::new(200.0, 110.0), 0.0, 1.2), INK, 2.0);
                g.line(Offset::new(10.0 + t / 6.0 * 200.0, 6.0), Offset::new(10.0 + t / 6.0 * 200.0, 172.0), HUES[1], 1.5);
            });

            let p3 = painted("constraints", "track-to + limit rotation · limit distance · follow path", Size::new(220.0, 180.0), move |g, _| {
                let target = Offset::new(110.0 + 80.0 * (t * 1.3).cos(), 80.0 + 50.0 * (t * 1.9).sin());
                let mut targets = Targets::new();
                targets.insert("target".into(), BoneTransform { translation: target, ..BoneTransform::default() });
                let bone = ConstraintStack::new()
                    .with(Constraint::TrackTo { target: "target".into(), offset: 0.0 })
                    .with(Constraint::LimitRotation { min: -PI * 0.75, max: -PI * 0.1 })
                    .evaluate(BoneTransform { translation: Offset::new(110.0, 160.0), ..BoneTransform::default() }, &targets);
                let tip = bone.translation + Offset::new(bone.rotation.cos() * 60.0, bone.rotation.sin() * 60.0);
                g.fill(Path::arc_ring(Offset::new(110.0, 160.0), 64.0, 64.0, -PI * 0.75, PI * 0.65), DIM.with_alpha(40));
                g.line(bone.translation, tip, HUES[0], 6.0);
                let follower = ConstraintStack::new()
                    .with(Constraint::LimitDistance { target: "target".into(), distance: 40.0, mode: DistanceMode::OnSurface })
                    .evaluate(BoneTransform { translation: Offset::new(20.0, 20.0), ..BoneTransform::default() }, &targets);
                g.stroke(circle(target, 40.0), DIM, 1.0);
                g.circle(follower.translation, 6.0, HUES[2]);
                g.fill(star(target, 9.0, 4.0, 5, t), HUES[1]);
                let mut path = Path::new();
                path.move_to(Offset::new(10.0, 20.0)).cubic_to(Offset::new(80.0, -20.0), Offset::new(140.0, 60.0), Offset::new(210.0, 20.0));
                g.stroke(path.clone(), DIM, 1.0);
                let rider = ConstraintStack::new().with(Constraint::follow_path(&path, (t / SPAN * 2.0).fract(), true)).evaluate(BoneTransform::default(), &targets);
                g.transformed(vieww_foundation::Transform::rotate(rider.rotation).then(vieww_foundation::Transform::translate(rider.translation)), |g| {
                    g.rrect(xywh(-9.0, -5.0, 18.0, 10.0), 3.0, HUES[3]);
                });
            });

            let exprs = [
                ("wiggle(1.5, 0.35) + 0.5", HUES[0]),
                ("0.5 + 0.4 * sin(time * 3) * exp(-time * 0.3)", HUES[1]),
                ("smoothstep(1, 4, time)", HUES[2]),
            ];
            let p4 = painted("expressions + driver", "wiggle · damped sin · smoothstep · driver: x² from the sin", Size::new(220.0, 180.0), move |g, _| {
                for (e, c) in exprs {
                    let ex = Expr::parse(e).expect("expression parses");
                    let vals: Vec<f32> = (0..=120).map(|k| ex.eval_num(&Scope::at(k as f32 / 20.0)).unwrap_or(0.0)).collect();
                    g.stroke(plot(&vals, Offset::new(10.0, 10.0), Size::new(200.0, 160.0), -0.1, 1.1), c, 2.0);
                }
                let src = Expr::parse(exprs[1].0).expect("parses");
                let driver = Driver::new("x * x").expect("parses").input("x", "osc");
                let vals: Vec<f32> = (0..=120)
                    .map(|k| {
                        let tt = k as f32 / 20.0;
                        driver.evaluate(tt, |_| src.eval_num(&Scope::at(tt)).ok()).unwrap_or(0.0)
                    })
                    .collect();
                g.stroke(plot(&vals, Offset::new(10.0, 10.0), Size::new(200.0, 160.0), -0.1, 1.1).dashed(&[5.0, 3.0], 0.0), HUES[3], 1.5);
                g.line(Offset::new(10.0 + t / 6.0 * 200.0, 6.0), Offset::new(10.0 + t / 6.0 * 200.0, 172.0), INK, 1.0);
            });

            let p5 = painted("CHOPs", "noisy step → lag · 1€ · ADSR envelope · sample&hold", Size::new(440.0, 180.0), move |g, _| {
                let dt = 1.0 / 60.0;
                let mut seed = 7u32;
                let input: Vec<f32> = (0..360)
                    .map(|k| {
                        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                        let n = (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
                        let tt = k as f32 * dt;
                        let step = if (1.0..3.5).contains(&tt) { 0.8 } else { 0.1 };
                        step + n * 0.15 + 0.05 * (tt * TAU * 0.7).sin()
                    })
                    .collect();
                let run = |c: &mut dyn Chop| input.iter().map(|&x| c.process(x, dt)).collect::<Vec<f32>>();
                let lag = run(&mut Lag::new(0.3, 0.8));
                let euro = run(&mut OneEuro::new(1.0, 0.5));
                let env = run(&mut Envelope::new(0.5, 0.2, 0.3, 0.6, 0.8));
                let mut sh = SampleHold::default();
                let held: Vec<f32> = input
                    .iter()
                    .enumerate()
                    .map(|(k, &x)| {
                        sh.set_hold(k % 30 != 0);
                        sh.process(x, dt)
                    })
                    .collect();
                let area = Size::new(420.0, 160.0);
                let o = Offset::new(10.0, 10.0);
                g.stroke(plot(&input, o, area, -0.1, 1.1), DIM, 1.0);
                for (i, s) in [lag, euro, env, held].iter().enumerate() {
                    g.stroke(plot(s, o, area, -0.1, 1.1), HUES[i], 2.0);
                }
                let px = 10.0 + t / 6.0 * 420.0;
                g.stroke(polyline(&[Offset::new(px, 6.0), Offset::new(px, 172.0)]), INK, 1.0);
            });
            page("83 · rigging & procedural channels", "vieww-animation: animator, nla, constraints, expr, channels", grid(3, vec![p1, p2, p3, p4, p5]))
        });
        feature_harness::set_page(d, view);
    })
}
