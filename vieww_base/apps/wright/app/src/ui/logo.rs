//! The mark: one stroke, a W.
//!
//! ```text
//!   (3,6)            (21,6)        on a 24 x 24 grid, stroke 2,
//!      \    (12,9)    /            round caps and joins
//!       \   /    \   /
//!       (7,19)  (17,19)
//! ```
//!
//! Drawn, never loaded: the same painter makes the top-bar mark, the
//! splash (where the stroke draws itself), the pale background watermark
//! the splash settles into, and the app icon the installers ship
//! (`wright ui --icon <png>`). One definition, so they cannot drift apart.

use vieww::prelude::*;

/// The W's corners on the 24-unit grid.
pub const W: [(f32, f32); 5] = [(3.0, 6.0), (7.0, 19.0), (12.0, 9.0), (17.0, 19.0), (21.0, 6.0)];
/// Stroke width on the same grid.
pub const STROKE: f32 = 2.0;

/// How the mark is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mark {
    pub color: Color,
    /// How much of the stroke is drawn, 0..=1 (the splash animates this).
    pub progress: f32,
    /// 0 = crisp; 1 = soft and wide, the way the watermark looks. Painted
    /// as widening low-alpha passes, which reads as a blur and costs a few
    /// lines instead of a full-window filter.
    pub soft: f32,
}

impl Mark {
    pub const fn solid(color: Color) -> Self {
        Mark { color, progress: 1.0, soft: 0.0 }
    }

    /// The mark, `side` pixels square.
    pub fn widget(self, side: f32) -> WidgetNode {
        CustomPaint::sized(Size::new(side, side), self).into()
    }
}

fn scaled(c: Color, factor: f32) -> Color {
    c.with_alpha((c.a as f32 * factor.clamp(0.0, 1.0)) as u8)
}

/// The polyline up to `progress` of its length, in pixels.
fn visible(progress: f32, scale: f32, origin: Offset) -> Vec<Offset> {
    let pts: Vec<Offset> = W.iter().map(|&(x, y)| Offset::new(origin.dx + x * scale, origin.dy + y * scale)).collect();
    let lens: Vec<f32> = pts.windows(2).map(|w| ((w[1].dx - w[0].dx).powi(2) + (w[1].dy - w[0].dy).powi(2)).sqrt()).collect();
    let mut left = lens.iter().sum::<f32>() * progress.clamp(0.0, 1.0);
    let mut out = vec![pts[0]];
    for (i, len) in lens.iter().enumerate() {
        if left <= 0.0 {
            break;
        }
        let t = (left / len).min(1.0);
        out.push(Offset::new(pts[i].dx + (pts[i + 1].dx - pts[i].dx) * t, pts[i].dy + (pts[i + 1].dy - pts[i].dy) * t));
        left -= len;
    }
    out
}

/// A round-capped, round-joined polyline: lines, with a dot on every
/// corner and end.
fn stroke(out: &mut Vec<DrawInstruction>, pts: &[Offset], width: f32, color: Color) {
    if pts.len() < 2 || color.a == 0 {
        return;
    }
    for w in pts.windows(2) {
        out.push(DrawInstruction::DrawLine { from: w[0], to: w[1], color, width });
    }
    for p in pts {
        out.push(DrawInstruction::FillCircle { center: *p, radius: width / 2.0, color });
    }
}

impl CustomPainter for Mark {
    fn paint(&self, size: Size) -> Vec<DrawInstruction> {
        let side = size.width.min(size.height);
        let scale = side / 24.0;
        let origin = Offset::new((size.width - side) / 2.0, (size.height - side) / 2.0);
        let pts = visible(self.progress, scale, origin);
        let width = STROKE * scale;
        let mut out = Vec::new();
        if self.soft > 0.0 {
            // Widest and faintest first; each pass narrower and stronger.
            for k in (1..=6).rev() {
                let w = width * (1.0 + k as f32 * 0.9 * self.soft);
                stroke(&mut out, &pts, w, scaled(self.color, 0.22 / k as f32));
            }
        }
        stroke(&mut out, &pts, width, scaled(self.color, 1.0 - 0.55 * self.soft));
        out
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn should_repaint(&self, previous: &dyn CustomPainter) -> bool {
        previous.as_any().downcast_ref::<Mark>() != Some(self)
    }
}

/// The app icon: the mark on a rounded tile ("squircle"-ish, Google
/// style: corner radius ~30% of the side).
pub fn tile(side: f32, ground: Color, ink: Color) -> WidgetNode {
    Container::new()
        .width(side)
        .height(side)
        .color(ground)
        .radius(side * 0.3)
        .alignment(Alignment::CENTER)
        .child(Mark::solid(ink).widget(side * 0.62))
        .into()
}

/// The icon colours (not the theme's: an icon looks the same everywhere).
pub const ICON_GROUND: Color = Color::hex(0x0078D4);
pub const ICON_INK: Color = Color::hex(0xFFFFFF);
