//! An OBJ mesh, projected and rotating — the gap-closure pass's
//! `vieww-mesh` crate, photographed.
//!
//! A cube authored as OBJ text (the all-four-face-spellings test case,
//! in miniature), parsed, and drawn as a wireframe: every vertex rotated
//! by a time-varying yaw and pitch, projected with a painter's-eye
//! perspective, every edge drawn with depth-cued weight. Deterministic
//! rotation — the same instant is the same cube, forever.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use vieww_animation::Ticker;
use vieww_element::Signal;
use vieww_foundation::{Color, Offset, Size};
use vieww_mesh::parse_obj;
use vieww_widget::prelude::*;

const SPAN: f32 = 6.0;

/// A unit cube, authored in the four spellings OBJ parsers meet: plain
/// `v`, `v/vt`, `v//vn` and `v/vt/vn` — the parser's own test corpus
/// reused as a demo, so the example exercises what the crate promises.
const CUBE_OBJ: &str = "\
v -1 -1 -1
v 1 -1 -1
v 1 1 -1
v -1 1 -1
v -1 -1 1
v 1 -1 1
v 1 1 1
v -1 1 1
vt 0 0
vt 1 0
vt 1 1
vt 0 1
vn 0 0 1
vn 1 0 0
vn 0 1 0
f 1//3 2//3 3//3 4//3
f 5 6 7 8
f 1/1 5/1 6/1 2/1
f 2//2 6//2 7//2 3//2
f 3/3/1 7/3/1 8/3/1 4/3/1
f 4 8 5 1
";

#[derive(Debug)]
struct LoopClock {
    time: Signal<f32>,
    span: f32,
}

impl Ticker for LoopClock {
    fn tick(&mut self, now: Duration) -> bool {
        self.time.set(now.as_secs_f32().rem_euclid(self.span));
        true
    }

    fn is_animating(&self) -> bool {
        true
    }
}

#[derive(Debug)]
struct Cube {
    /// The clock's registration is *weak*: the frame driver keeps a
    /// `Weak` to it, so somebody must own it for the frames to advance.
    /// This field is that somebody — the tree holds the widget, the
    /// widget holds the clock, the clock keeps ticking. The same trick
    /// example 05's comment explains for `TimelinePlayer`.
    _clock: Rc<RefCell<LoopClock>>,
    time: Signal<f32>,
}

impl Widget for Cube {
    fn debug_name(&self) -> &'static str {
        "Cube"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let t = self.time.get();
        CustomPaint::sized(Size::new(420.0, 360.0), CubePainter { t }).into()
    }
}

widget_node_from!(Cube);

struct CubePainter {
    t: f32,
}

impl CustomPainter for CubePainter {
    fn paint(&self, size: Size) -> Vec<DrawInstruction> {
        let mesh = parse_obj(CUBE_OBJ).expect("the cube is valid OBJ");
        let centre = Offset::new(size.width / 2.0, size.height / 2.0);

        // Yaw and pitch, both functions of the clock — one spin, one wobble.
        let yaw = self.t * std::f32::consts::TAU / SPAN;
        let pitch = (self.t * 0.9).sin() * 0.5;
        let (cy, sy) = (yaw.cos(), yaw.sin());
        let (cp, sp) = (pitch.cos(), pitch.sin());

        let rotate = |p: &[f32; 3]| -> (f32, f32, f32) {
            // Yaw around y, then pitch around x.
            let x1 = p[0] * cy - p[2] * sy;
            let z1 = p[0] * sy + p[2] * cy;
            let y2 = p[1] * cp - z1 * sp;
            let z2 = p[1] * sp + z1 * cp;
            (x1, y2, z2)
        };

        // Project: a fixed eye distance, perspective divide, scale to the
        // canvas. The far side shrinks — the depth cue the rotation needs
        // to read as rotation.
        let eye = 5.0;
        let scale = size.width.min(size.height) * 0.28;
        let projected: Vec<Offset> = mesh
            .positions
            .iter()
            .map(|position| {
                let (x, y, z) = rotate(position);
                let perspective = eye / (eye - z);
                Offset::new(
                    centre.dx + x * perspective * scale,
                    centre.dy - y * perspective * scale,
                )
            })
            .collect();
        let depths: Vec<f32> = mesh
            .positions
            .iter()
            .map(|position| rotate(position).2)
            .collect();

        // The 12 edges of the cube, by hand — the OBJ's quads-to-triangles
        // split duplicates vertices for attribute seams, and the *edges*
        // are a property of the cube, not of how the file spelled it.
        const EDGES: &[(usize, usize)] = &[
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 0),
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 4),
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7),
        ];
        let mut out = Vec::with_capacity(EDGES.len() + 8);
        for &(a, b) in EDGES {
            // Nearer edge, heavier line: depth as weight, not as colour.
            let near = ((depths[a] + depths[b]) / 2.0 + 1.5) / 3.0;
            out.push(DrawInstruction::DrawLine {
                from: projected[a],
                to: projected[b],
                color: Color::rgb(58, 122, 246).with_alpha((90.0 + near * 165.0) as u8),
                width: 1.0 + near * 2.5,
            });
        }
        for point in &projected {
            out.push(DrawInstruction::FillCircle {
                center: *point,
                radius: 2.5,
                color: Color::rgb(24, 28, 40),
            });
        }
        out
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn should_repaint(&self, previous: &dyn CustomPainter) -> bool {
        match previous.as_any().downcast_ref::<Self>() {
            Some(prev) => self.t != prev.t,
            None => true,
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    feature_harness::launch("67 — mesh", Size::new(460.0, 420.0), |d| {
        let runtime = d.elements().runtime().clone();
        let time = runtime.signal(0.0_f32);
        let clock = Rc::new(RefCell::new(LoopClock {
            time: time.clone(),
            span: SPAN,
        }));
        d.tickers().add(&clock);
        feature_harness::set_page(
            d,
            Container::new()
                .color(Color::rgb(16, 18, 28))
                .padding(EdgeInsets::all(20.0))
                .child(Cube {
                    time,
                    _clock: clock,
                }),
        );
    })
}
