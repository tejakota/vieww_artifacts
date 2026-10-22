//! The mark: one stroke, a W.
//!
//! ```text
//!   (3,6)            (21,6)        on a 24 x 24 grid, stroke 2,
//!      \    (12,9)    /            round caps and joins
//!       \   /    \   /
//!       (7,19)  (17,19)
//! ```
//!
//! Drawn, never loaded: the same painter makes the window icon (the title
//! bar's picture, beside the name — `shell::window_icon` renders it), the
//! splash (where the stroke draws itself), the pale background watermark
//! the splash settles into, and the app icon the installers ship
//! (`wright ui --icon <png>`). One definition, so they cannot drift apart.

use vieww::foundation::StrokeStyle;
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
    /// 0 = crisp; 1 = blurred and a little heavier, the way the watermark
    /// looks behind the app.
    pub soft: f32,
}

impl Mark {
    pub const fn solid(color: Color) -> Self {
        Mark { color, progress: 1.0, soft: 0.0 }
    }

    /// The mark, `side` pixels square.
    pub fn widget(self, side: f32) -> WidgetNode {
        Painting::sized(Size::new(side, side), self).into()
    }
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

impl Painter for Mark {
    /// One path, stroked with round caps and joins — so the joints are part
    /// of the stroke, not discs laid over it, and a transparent colour stays
    /// one even shade. The soft version is that stroke at a few widths,
    /// stacked thinly, which fades its edge the way a blur would.
    fn paint(&self, book: &mut Sketchbook, size: Size) {
        let side = size.width.min(size.height);
        let scale = side / 24.0;
        let origin = Offset::new((size.width - side) / 2.0, (size.height - side) / 2.0);
        let pts = visible(self.progress, scale, origin);
        if pts.len() < 2 || self.color.a == 0 {
            return;
        }
        let mut path = Path::new();
        path.move_to(pts[0]);
        for p in &pts[1..] {
            path.line_to(*p);
        }
        let width = STROKE * scale * (1.0 + 0.5 * self.soft);
        let (color, blur) = (self.color, side * 0.012 * self.soft);
        if blur > 0.5 {
            // The soft edge as a few translucent strokes, widest first,
            // instead of a blurred layer: the same falloff to the eye, and
            // path fills rather than a blur, the most expensive thing the CPU
            // renderer draws. The mark sits behind everything, so anything
            // scrolling or animating in front of it repaints it — with a blur
            // that cost 16 ms of a 25 ms scroll frame at 1x and ~90 ms at 2x.
            const STEPS: usize = 6;
            let reach = 3.0 * blur;
            let alpha = 1.0 - (1.0 - f32::from(color.a) / 255.0).powf(1.0 / STEPS as f32);
            for k in (0..STEPS).rev() {
                let w = width + 2.0 * reach * k as f32 / (STEPS - 1) as f32 - reach * 0.6;
                book.stroke_styled(path.clone(), color.with_alpha((alpha * 255.0).round() as u8), w.max(width * 0.5), StrokeStyle::rounded());
            }
        } else {
            book.stroke_styled(path, color, width, StrokeStyle::rounded());
        }
    }
    fn should_repaint(&self, previous: &dyn Painter) -> bool {
        previous.as_any().downcast_ref::<Mark>() != Some(self)
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
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
