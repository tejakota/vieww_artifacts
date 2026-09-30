//! Rigid bodies with rotation, joints, a ragdoll, a character controller,
//! cloth and a fluid — Box2D/Rapier/Matter.js, Unity's CharacterController,
//! Verlet cloth and Clavet SPH, photographed.
//!
//! 1. A box pyramid, then a CCD "bullet" fired through it at t = 1 s.
//! 2. Joints: a revolute chain, a motor-driven windmill, a spring
//!    (soft distance joint) and a capsule ragdoll with limited hinges
//!    tumbling down steps.
//! 3. `CharacterController::move_and_slide` walking a slope and a step,
//!    jumping on a timer; a `raycast` from its feet shows the ground.
//! 4. Cloth pinned every fourth column, gusting wind, a circle collider,
//!    tearing past 2.2× stretch.
//! 5. A dam break in a tank — double-density relaxation SPH.
//!
//! Each simulation steps forward with the clip's clock (and resets when it
//! loops), so the frames are one continuous run.

use std::cell::RefCell;
use std::rc::Rc;

use feature_harness::draw::{circle, grid, page, painted, polyline, xywh, DIM, HUES, INK};
use vieww_foundation::{Color, Offset, Size, Sketchbook};
use vieww_physics::cloth::Cloth;
use vieww_physics::fluid::{Fluid, FluidParams};
use vieww_physics::rigid::{BodyKind, CharacterController, Collider, JointKind, RigidBody, RigidJoint, RigidWorld};

const SPAN: f32 = 6.0;
const DT: f32 = 1.0 / 60.0;

fn v(x: f32, y: f32) -> Offset {
    Offset::new(x, y)
}

fn ground(w: &mut RigidWorld, width: f32, y: f32) {
    w.add(RigidBody::fixed(Collider::rect(width, 20.0), v(width / 2.0, y + 10.0)));
}

fn pyramid() -> (RigidWorld, usize) {
    let mut w = RigidWorld::new(v(0.0, 500.0));
    ground(&mut w, 240.0, 180.0);
    let s = 18.0;
    for row in 0..6 {
        for i in 0..(6 - row) {
            let x = 120.0 - (6 - row) as f32 * s / 2.0 + i as f32 * s + s / 2.0;
            let y = 180.0 - s / 2.0 - row as f32 * s;
            w.add(RigidBody::dynamic(Collider::rect(s - 1.0, s - 1.0), v(x, y), 1.0));
        }
    }
    let mut bullet = RigidBody::dynamic(Collider::circle(4.0), v(-40.0, 150.0), 8.0);
    bullet.bullet = true;
    bullet.gravity_scale = 0.0;
    bullet.kind = BodyKind::Kinematic;
    let b = w.add(bullet);
    (w, b)
}

fn joints() -> RigidWorld {
    let mut w = RigidWorld::new(v(0.0, 500.0));
    let anchor = w.add(RigidBody::fixed(Collider::circle(3.0), v(40.0, 20.0)));
    // Chain.
    let mut prev = anchor;
    for i in 0..6 {
        let p = v(40.0 + (i + 1) as f32 * 14.0, 20.0);
        let link = w.add(RigidBody::dynamic(Collider::capsule(10.0, 3.0), p - v(7.0, 0.0), 1.0));
        let j = RigidJoint::at_anchor(&w, prev, link, p - v(14.0, 0.0), JointKind::Revolute { limits: None, motor: None });
        w.add_joint(j);
        prev = link;
    }
    // Windmill with a motor.
    let hub = w.add(RigidBody::fixed(Collider::circle(2.0), v(190.0, 50.0)));
    let blade = w.add(RigidBody::dynamic(Collider::rect(70.0, 8.0), v(190.0, 50.0), 1.0));
    let j = RigidJoint::at_anchor(&w, hub, blade, v(190.0, 50.0), JointKind::Revolute { limits: None, motor: Some((3.0, 1e7)) });
    w.add_joint(j);
    // Spring.
    let top = w.add(RigidBody::fixed(Collider::circle(2.0), v(150.0, 10.0)));
    let bob = w.add(RigidBody::dynamic(Collider::circle(8.0), v(150.0, 70.0), 1.0));
    let mut sj = RigidJoint::between(&w, top, v(150.0, 10.0), bob, v(150.0, 70.0), 1.5, 0.1);
    if let JointKind::Distance { length, .. } = &mut sj.kind {
        *length = 40.0;
    }
    w.add_joint(sj);
    // Steps and a ragdoll.
    for (i, x) in [60.0, 120.0, 180.0].iter().enumerate() {
        w.add(RigidBody::fixed(Collider::rect(60.0, 20.0 + i as f32 * 0.0), v(*x, 150.0 + i as f32 * 20.0)));
    }
    ground(&mut w, 240.0, 190.0);
    let at = v(55.0, 90.0);
    let torso = w.add(RigidBody::dynamic(Collider::capsule(22.0, 5.0), at, 1.0).angle(std::f32::consts::FRAC_PI_2));
    let head = w.add(RigidBody::dynamic(Collider::circle(6.0), at - v(0.0, 22.0), 1.0));
    let hinge = |w: &mut RigidWorld, a: usize, b: usize, p: Offset, lim: (f32, f32)| {
        let j = RigidJoint::at_anchor(w, a, b, p, JointKind::Revolute { limits: Some(lim), motor: None });
        w.add_joint(j);
    };
    hinge(&mut w, torso, head, at - v(0.0, 15.0), (-0.5, 0.5));
    for dx in [-1.0f32, 1.0] {
        let arm = w.add(RigidBody::dynamic(Collider::capsule(16.0, 3.0), at + v(dx * 12.0, -8.0), 1.0));
        hinge(&mut w, torso, arm, at + v(dx * 4.0, -8.0), (-1.5, 1.5));
        let leg = w.add(RigidBody::dynamic(Collider::capsule(18.0, 3.5), at + v(dx * 4.0, 22.0), 1.0).angle(std::f32::consts::FRAC_PI_2));
        hinge(&mut w, torso, leg, at + v(dx * 4.0, 12.0), (-0.8, 0.8));
    }
    w.bodies[torso].velocity = v(60.0, 0.0);
    w
}

fn terrain() -> RigidWorld {
    let mut w = RigidWorld::new(v(0.0, 0.0));
    ground(&mut w, 240.0, 170.0);
    w.add(RigidBody::fixed(Collider::polygon(&[v(-40.0, 0.0), v(40.0, -30.0), v(40.0, 0.0)], 0.0), v(110.0, 170.0)));
    w.add(RigidBody::fixed(Collider::rect(60.0, 30.0), v(180.0, 155.0)));
    w
}

fn draw_body(g: &mut Sketchbook, b: &RigidBody, c: Color) {
    match &b.collider {
        Collider::Circle { radius, .. } => {
            g.circle(b.position, *radius, c);
            let tip = b.world_point(v(*radius, 0.0));
            g.line(b.position, tip, Color::rgba(0, 0, 0, 120), 1.0);
        }
        Collider::Polygon { radius, .. } => {
            let pts = b.world_vertices();
            if pts.len() == 2 {
                g.line(pts[0], pts[1], c, radius * 2.0);
                g.circle(pts[0], *radius, c);
                g.circle(pts[1], *radius, c);
            } else {
                let mut p = polyline(&pts);
                p.close();
                g.fill(p, c);
            }
        }
    }
}

fn draw_world(g: &mut Sketchbook, w: &RigidWorld) {
    for (i, b) in w.bodies.iter().enumerate() {
        let c = match b.kind {
            BodyKind::Static => DIM,
            BodyKind::Kinematic => HUES[1],
            BodyKind::Dynamic if b.sleeping => HUES[0].with_alpha(140),
            BodyKind::Dynamic => HUES[i % 6],
        };
        draw_body(g, b, c);
    }
    for j in &w.joints {
        let (a, b) = (w.bodies[j.a].world_point(j.local_a), w.bodies[j.b].world_point(j.local_b));
        if matches!(j.kind, JointKind::Distance { .. }) {
            g.line(a, b, INK.with_alpha(160), 1.0);
        }
        g.circle(b, 1.8, INK);
    }
}

struct Sims {
    time: f32,
    pyramid: (RigidWorld, usize),
    joints: RigidWorld,
    terrain: RigidWorld,
    walker: CharacterController,
    cloth: Cloth,
    fluid: Fluid,
}

impl Sims {
    fn new() -> Self {
        let mut cloth = Cloth::grid(v(20.0, 12.0), 22, 16, 9.0, 4);
        cloth.tear_ratio = Some(2.2);
        cloth.colliders.push((v(120.0, 130.0), 26.0));
        let mut fluid = Fluid::new(FluidParams { bounds: xywh(0.0, 0.0, 240.0, 190.0), ..FluidParams::default() });
        fluid.fill(xywh(4.0, 60.0, 80.0, 126.0), 7.0);
        Self {
            time: 0.0,
            pyramid: pyramid(),
            joints: joints(),
            terrain: terrain(),
            walker: CharacterController::new(v(20.0, 120.0), 10.0, 7.0),
            cloth,
            fluid,
        }
    }

    fn advance_to(&mut self, t: f32) {
        if t < self.time {
            *self = Self::new();
        }
        while self.time + DT <= t {
            self.time += DT;
            let (w, b) = &mut self.pyramid;
            if self.time > 1.0 {
                w.bodies[*b].velocity = v(900.0, 0.0);
            }
            w.step(DT);
            self.joints.step(DT);
            let c = &mut self.walker;
            c.velocity.dx = if self.time.rem_euclid(4.0) < 2.8 { 70.0 } else { -70.0 };
            if c.on_floor {
                c.velocity.dy = 0.0;
                if (self.time * 60.0) as i32 % 90 == 45 {
                    c.velocity.dy = -260.0;
                }
            } else {
                c.velocity.dy += 700.0 * DT;
            }
            c.move_and_slide(&self.terrain, DT);
            self.cloth.wind = v(60.0 + 50.0 * (self.time * 1.7).sin(), 0.0);
            self.cloth.step(DT);
            self.fluid.step(DT);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    feature_harness::launch("86 — physics", Size::new(1370.0, 330.0), |d| {
        let sims = Rc::new(RefCell::new(Sims::new()));
        let view = feature_harness::clocked(d, SPAN, move |t| {
            sims.borrow_mut().advance_to(t);
            let s = sims.borrow();
            let pw = s.pyramid.0.clone();
            let st = pw.stats;
            let p1 = painted("stack + CCD bullet", &format!("{} contacts · {} sleeping · {} TOI", st.contact_points, st.sleeping, st.toi_events), Size::new(240.0, 200.0), move |g, _| draw_world(g, &pw));
            let jw = s.joints.clone();
            let p2 = painted("joints + ragdoll", "revolute chain · motor · spring · limited hinges", Size::new(240.0, 200.0), move |g, _| draw_world(g, &jw));
            let tw = s.terrain.clone();
            let c = s.walker.clone();
            let p3 = painted("character controller", &format!("move_and_slide · on_floor {} · on_wall {}", c.on_floor, c.on_wall), Size::new(240.0, 200.0), move |g, _| {
                draw_world(g, &tw);
                let foot = c.position + v(0.0, c.half_height + c.radius);
                if let Some(hit) = tw.raycast(c.position, v(0.0, 1.0), 200.0) {
                    g.line(c.position, hit.point, HUES[3], 1.0);
                    g.line(hit.point, hit.point + hit.normal.scale(14.0), HUES[2], 2.0);
                }
                g.line(c.position - v(0.0, c.half_height), c.position + v(0.0, c.half_height), HUES[4], c.radius * 2.0);
                g.circle(c.position - v(0.0, c.half_height), c.radius, HUES[4]);
                g.circle(c.position + v(0.0, c.half_height), c.radius, HUES[4]);
                let _ = foot;
            });
            let cl = s.cloth.clone();
            let intact = cl.intact();
            let p4 = painted("cloth", &format!("verlet · wind · collider · tearing — {intact} links intact"), Size::new(240.0, 200.0), move |g, _| {
                for (o, r) in &cl.colliders {
                    g.fill(circle(*o, *r), DIM.with_alpha(90));
                }
                for l in cl.links.iter().filter(|l| !l.broken) {
                    g.line(cl.particles[l.a].position, cl.particles[l.b].position, HUES[5], 1.0);
                }
                for p in cl.particles.iter().filter(|p| p.pinned) {
                    g.circle(p.position, 2.5, INK);
                }
            });
            let fl = s.fluid.clone();
            let p5 = painted("SPH fluid", &format!("dam break · {} particles", fl.len()), Size::new(240.0, 200.0), move |g, _| {
                let dens = fl.densities();
                for (i, p) in fl.positions.iter().enumerate() {
                    let d = dens.get(i).copied().unwrap_or(0.0);
                    let c = HUES[0].lerp_oklab(Color::rgb(230, 245, 255), ((d - 1.0) / 2.0).clamp(0.0, 1.0));
                    g.circle(*p, 4.0, c);
                }
                g.stroke(polyline(&[v(0.0, 0.0), v(0.0, 190.0), v(240.0, 190.0), v(240.0, 0.0)]), DIM, 2.0);
            });
            page("86 · physics", "vieww-physics: rigid (rotation, SAT, joints, CCD, raycast, sleeping, character), cloth, fluid", grid(5, vec![p1, p2, p3, p4, p5]))
        });
        feature_harness::set_page(d, view);
    })
}
