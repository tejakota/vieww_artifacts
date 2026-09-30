//! Particle forces, animation retargeting with root motion, and
//! off-thread worklets — Unity's Particle System / VFX Graph forces,
//! Mecanim's Humanoid retargeting and root motion, and Reanimated's UI-
//! thread `useSharedValue` + `withSpring`, photographed.
//!
//! 1. A fountain: gravity, turbulence (Perlin curl), drag and a bouncing
//!    floor with friction — `ParticleSystem` simulated from t = 0 each frame.
//! 2. An attractor and a vortex pulling a ring emitter into a galaxy.
//! 3. One walk clip authored on a short rig, retargeted with
//!    `retarget(..., BoneMap::by_name)` onto a tall rig with different
//!    proportions; the tall rig is moved by `root_motion` while its pose is
//!    `strip_root`-ed in place.
//! 4. `with_spring` / `with_timing` worklets writing `SharedValue`s, and a
//!    live `UiThread` ticking them off the main thread (frame counter in
//!    the caption).

use std::f32::consts::{FRAC_PI_2, TAU};
use std::sync::Arc;
use std::time::Duration;

use feature_harness::draw::{grid, page, painted, xywh, DIM, HUES, INK};
use vieww_animation::particle_system::{Collision, EmitterShape, Force, ParticleSystem};
use vieww_animation::retarget::{retarget, root_motion, strip_root, BoneMap};
use vieww_animation::shared::{with_spring, with_timing, SharedValue, UiThread};
use vieww_animation::skeletal::{BoneTransform, Pose, SkeletalClip, Skeleton};
use vieww_animation::{Curve, Keyframe, Keyframes};
use vieww_foundation::{Color, Offset, Size, Sketchbook};

const SPAN: f32 = 4.0;

fn fountain(t: f32) -> ParticleSystem {
    let mut p = ParticleSystem::new(EmitterShape::Point(Offset::new(110.0, 170.0)), 140.0, 11)
        .force(Force::Turbulence { strength: 90.0, scale: 0.02, speed: 0.6 })
        .force(Force::Drag(0.4));
    p.speed = (230.0, 300.0);
    p.spread = 0.35;
    p.lifetime = (1.6, 2.4);
    p.forces[0] = Force::Gravity(Offset::new(0.0, 220.0));
    p.collision = Some(Collision::Bounce { floor: 175.0, restitution: 0.45, friction: 0.2 });
    p.size = (3.5, 1.0);
    p.color = (Color::rgb(120, 200, 255), Color::rgba(90, 120, 255, 0));
    p.run(t, 1.0 / 60.0);
    p
}

fn galaxy(t: f32) -> ParticleSystem {
    let c = Offset::new(110.0, 95.0);
    let mut p = ParticleSystem::new(EmitterShape::Circle { center: c, radius: 85.0, edge: true }, 220.0, 5)
        .forces(vec![Force::Attractor { at: c, strength: 900.0, radius: 120.0 }, Force::Vortex { at: c, strength: 7.0 }, Force::Drag(0.6)]);
    p.speed = (5.0, 20.0);
    p.lifetime = (2.0, 3.0);
    p.size = (2.5, 0.5);
    p.color = (Color::rgb(255, 200, 120), Color::rgba(240, 100, 200, 0));
    p.run(t + 1.0, 1.0 / 60.0);
    p
}

fn draw_particles(g: &mut Sketchbook, p: &ParticleSystem) {
    for q in p.particles() {
        g.circle(q.position, q.size.max(0.5), q.color);
    }
}

fn bt(x: f32, y: f32) -> BoneTransform {
    BoneTransform::from_translation(Offset::new(x, y))
}

fn rig(scale: f32, leg: f32) -> Skeleton {
    Skeleton::new()
        .bone("hips", None, bt(0.0, 0.0))
        .bone("spine", Some("hips"), bt(0.0, -30.0 * scale))
        .bone("head", Some("spine"), bt(0.0, -14.0 * scale))
        .bone("arm", Some("spine"), bt(0.0, 0.0))
        .bone("hand", Some("arm"), bt(0.0, 26.0 * scale))
        .bone("thigh.l", Some("hips"), bt(0.0, 0.0))
        .bone("foot.l", Some("thigh.l"), bt(0.0, 30.0 * leg))
        .bone("thigh.r", Some("hips"), bt(0.0, 0.0))
        .bone("foot.r", Some("thigh.r"), bt(0.0, 30.0 * leg))
}

fn rot(t: f32, r: f32) -> Keyframe<BoneTransform> {
    Keyframe::to(t, BoneTransform { rotation: r, ..BoneTransform::default() })
}

fn walk() -> SkeletalClip {
    let swing = |a: f32| Keyframes::new(BoneTransform { rotation: a, ..BoneTransform::default() }).with(rot(0.5, -a)).with(rot(1.0, a));
    SkeletalClip::new()
        .bone("thigh.l", swing(0.5))
        .bone("thigh.r", swing(-0.5))
        .bone("arm", swing(-0.6))
        .bone("hips", Keyframes::new(bt(0.0, 0.0)).with(Keyframe::to(0.25, bt(15.0, -3.0))).with(Keyframe::to(0.5, bt(30.0, 0.0))).with(Keyframe::to(0.75, bt(45.0, -3.0))).with(Keyframe::to(1.0, bt(60.0, 0.0))))
}

fn draw_rig(g: &mut Sketchbook, sk: &Skeleton, pose: &Pose, at: Offset, c: Color) {
    let world = sk.world(pose);
    let pts: Vec<Offset> = world.iter().map(|w| w.apply(Offset::ZERO) + at).collect();
    let pairs = [("hips", "spine"), ("spine", "head"), ("arm", "hand"), ("thigh.l", "foot.l"), ("thigh.r", "foot.r")];
    for (a, b) in pairs {
        if let (Some(i), Some(j)) = (sk.index_of(a), sk.index_of(b)) {
            g.line(pts[i], pts[j], c, 4.0);
        }
    }
    if let Some(h) = sk.index_of("head") {
        g.circle(pts[h], 7.0, c);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // A real off-main-thread animation loop, running for the life of the app.
    let ui = Arc::new(UiThread::start(Duration::from_micros(16_667)));
    let live = SharedValue::new(0.0);
    ui.run(with_spring(&live, 1.0, 120.0, 6.0));
    feature_harness::launch("84 — particles & retarget", Size::new(1080.0, 330.0), move |d| {
        let ui = ui.clone();
        let live = live.clone();
        let view = feature_harness::clocked(d, SPAN, move |t| {
            let f = fountain(t);
            let n = f.len();
            let p1 = painted("fountain", &format!("gravity · turbulence · drag · bounce floor — {n} live"), Size::new(220.0, 190.0), move |g, _| {
                g.rect(xywh(0.0, 175.0, 220.0, 15.0), DIM.with_alpha(50));
                draw_particles(g, &f);
            });
            let gx = galaxy(t);
            let p2 = painted("attractor + vortex", &format!("ring emitter pulled into a spiral — {} live", gx.len()), Size::new(220.0, 190.0), move |g, _| draw_particles(g, &gx));

            let (short, tall) = (rig(1.0, 1.0), rig(1.5, 1.8));
            let clip = walk();
            let cyc = (t / 1.0).fract();
            let src = clip.pose(&short, Duration::from_secs_f32(cyc));
            let mapped = retarget(&short, &src, &tall, &BoneMap::by_name(&short, &tall));
            let inplace = strip_root(&tall, &mapped, "hips");
            let (dx, _) = root_motion(&clip, &short, "hips", 0.0, t, true);
            let p3 = painted("retarget + root motion", &format!("short rig → tall rig by name · root travelled {:.0}px", dx.dx), Size::new(260.0, 190.0), move |g, _| {
                draw_rig(g, &short, &src, Offset::new(40.0, 140.0), HUES[0]);
                let x = 110.0 + (dx.dx * 0.5).rem_euclid(140.0);
                draw_rig(g, &tall, &inplace, Offset::new(x, 116.0), HUES[3]);
                g.line(Offset::new(0.0, 170.0), Offset::new(260.0, 170.0), DIM, 1.0);
            });

            let frames = ui.frames();
            let now = live.get();
            let p4 = painted("worklets · shared values", &format!("UiThread frames {frames} · live spring {now:.3}"), Size::new(200.0, 190.0), move |g, _| {
                let specs: [(&str, Box<dyn Fn(&SharedValue) -> vieww_animation::shared::Worklet>); 3] = [
                    ("spring k120 c6", Box::new(|v| with_spring(v, 1.0, 120.0, 6.0))),
                    ("spring k300 c30", Box::new(|v| with_spring(v, 1.0, 300.0, 30.0))),
                    ("timing 0.8s", Box::new(|v| with_timing(v, 1.0, Duration::from_millis(800), Curve::EASE_IN_OUT))),
                ];
                for (i, (_, mk)) in specs.iter().enumerate() {
                    let y = 20.0 + i as f32 * 55.0;
                    let v = SharedValue::new(0.0);
                    let mut w = mk(&v);
                    w(Duration::from_secs_f32((t * 0.6).min(2.0)));
                    g.rrect(xywh(10.0, y, 180.0, 10.0), 5.0, DIM.with_alpha(50));
                    g.circle(Offset::new(10.0 + v.get() * 170.0, y + 5.0), 9.0, HUES[i]);
                }
                let spin = now * TAU - FRAC_PI_2;
                g.line(Offset::new(100.0, 176.0), Offset::new(100.0 + spin.cos() * 12.0, 176.0 + spin.sin() * 12.0), INK, 2.0);
            });
            page("84 · particles, retargeting, worklets", "vieww-animation: particle_system, retarget, shared", grid(4, vec![p1, p2, p3, p4]))
        });
        feature_harness::set_page(d, view);
    })
}
