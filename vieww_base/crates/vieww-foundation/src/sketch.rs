//! A recorded drawing: paths, brushes, strokes, shadows and layers.
//!
//! # Why a display list and not a canvas
//!
//! `vieww-paint`'s [`Canvas`](../../vieww_paint/canvas/trait.Canvas.html) is the
//! real drawing surface, and it is exactly the wrong thing to hand a widget.
//! The widget layer is a cheap description that knows nothing about pixels —
//! `docs/DESIGN.md` §7 — and a `&mut dyn Canvas` in a widget's `build` would
//! point that dependency the wrong way and make every widget test need a
//! rasteriser.
//!
//! So a painter *records* instead. [`Sketchbook`] collects [`Sketch`] items, the
//! render object replays them onto the real canvas, and the recording itself is
//! an ordinary value: comparable, testable, and printable in a failure message.
//! The vocabulary is deliberately the renderer's own — a fill, a stroke, a
//! rounded-rectangle shadow, a layer — so replay is a translation and never an
//! emulation.
//!
//! # What this is for
//!
//! [`CustomPaint`](../../vieww_widget/struct.CustomPaint.html) draws rectangles,
//! circles and lines, which is enough for a bar chart and not enough for
//! anything with a curve in it. This is the escape hatch: arbitrary
//! [`Path`]s, [`Gradient`] brushes, strokes, blurred layers and clips, without
//! an application having to write a render object and register it.
//!
//! ```
//! use vieww_foundation::{Color, Gradient, Offset, Rect, Sketchbook};
//!
//! let mut book = Sketchbook::new();
//! book.rrect(
//!     Rect::new(0.0, 0.0, 64.0, 24.0),
//!     12.0,
//!     Gradient::horizontal().between(Color::rgb(90, 140, 255), Color::rgb(150, 100, 255)),
//! );
//! book.stroke_rrect(Rect::new(0.0, 0.0, 64.0, 24.0), 12.0, Color::WHITE, 1.0);
//! assert_eq!(book.len(), 2);
//! ```

use crate::color::Color;
use crate::geometry::{Offset, Rect};
use crate::paint::{BlendMode, Gradient, Shadow, StrokeStyle};
use crate::path::Path;
use crate::transform::Transform;

/// What a shape is filled or stroked with.
///
/// Two cases and no `Paint` struct, because those are the two the renderer
/// distinguishes: everything downstream either resolves to one colour or to a
/// gradient ramp, and a third case here would be a third case in every backend.
#[derive(Debug, Clone, PartialEq)]
pub enum Brush {
    Solid(Color),
    Gradient(Gradient),
}

impl Brush {
    /// Nothing would be drawn: a transparent colour, or a gradient whose stops
    /// are all transparent.
    #[must_use]
    pub fn is_invisible(&self) -> bool {
        match self {
            Self::Solid(color) => color.is_transparent(),
            Self::Gradient(gradient) => gradient.is_invisible(),
        }
    }
}

impl From<Color> for Brush {
    fn from(color: Color) -> Self {
        Self::Solid(color)
    }
}

impl From<Gradient> for Brush {
    fn from(gradient: Gradient) -> Self {
        Self::Gradient(gradient)
    }
}

/// One recorded drawing operation.
///
/// Coordinates are in the painter's own space, which the render object
/// translates to the widget's origin before replaying — so a painter draws as
/// if its box started at `(0, 0)` and never has to know where it landed.
#[derive(Debug, Clone, PartialEq)]
pub enum Sketch {
    /// Fill a path's interior.
    Fill { path: Path, brush: Brush },
    /// Stroke a path's outline. `width` is the full width, centred on the path.
    ///
    /// `style` carries the caps, joins and dashes;
    /// [`StrokeStyle::default()`](crate::StrokeStyle) is what a width-only
    /// stroke always drew, so a sketch written before the vocabulary existed
    /// renders identically.
    Stroke {
        path: Path,
        brush: Brush,
        width: f32,
        style: StrokeStyle,
    },
    /// A rounded-rectangle shadow, outer or inset.
    ///
    /// Rounded rectangles only, because that is the caster the renderer's
    /// `DrawShadow` takes. A shadow under an arbitrary path is a blurred layer
    /// containing a fill of that path.
    Shadow {
        rect: Rect,
        radius: f32,
        shadow: Shadow,
    },
    /// A group: composited as a unit, so `alpha` fades the whole thing rather
    /// than each shape in it, and `blur` softens what the group drew.
    ///
    /// `clip` confines the group to a path — the way to clip arbitrary vector
    /// content, and the reason a group exists at all. `blend` is how the
    /// resolved group combines with what is already behind it: the 22 modes
    /// the rasterizer honours are all reachable from a sketch layer, where
    /// before every one of them composited `Normal` (todo-upgrades U-01).
    Layer {
        alpha: f32,
        blur: f32,
        blend: BlendMode,
        clip: Option<Path>,
        children: Vec<Sketch>,
    },
    /// A group drawn through a transform. Rotation, scale and skew for content
    /// that is otherwise axis-aligned.
    Transformed {
        transform: Transform,
        children: Vec<Sketch>,
    },
}

impl Sketch {
    /// Nothing would appear on screen.
    ///
    /// Empty paths, transparent brushes, zero-width strokes and empty groups —
    /// checked so a render object can skip a whole subtree rather than pushing
    /// a layer for it.
    #[must_use]
    pub fn is_invisible(&self) -> bool {
        match self {
            Self::Fill { path, brush } => path.is_empty() || brush.is_invisible(),
            Self::Stroke {
                path, brush, width, ..
            } => path.is_empty() || brush.is_invisible() || *width <= 0.0,
            Self::Shadow { shadow, .. } => shadow.is_invisible(),
            Self::Layer {
                alpha, children, ..
            } => *alpha <= 0.0 || children.iter().all(Self::is_invisible),
            Self::Transformed { children, .. } => children.iter().all(Self::is_invisible),
        }
    }
}

/// Records a drawing.
///
/// The methods are the shapes worth having a name for; anything else is
/// [`fill`](Self::fill) or [`stroke`](Self::stroke) with a [`Path`] built by
/// hand or parsed from SVG path data with
/// [`parse_path_data`](crate::parse_path_data).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sketchbook {
    items: Vec<Sketch>,
}

impl Sketchbook {
    #[must_use]
    pub const fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// What was recorded, in paint order.
    #[must_use]
    pub fn items(&self) -> &[Sketch] {
        &self.items
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Everything recorded so far, taken out.
    #[must_use]
    pub fn into_items(self) -> Vec<Sketch> {
        self.items
    }

    /// Record an item as given. The other methods are conveniences over this.
    pub fn push(&mut self, sketch: Sketch) -> &mut Self {
        self.items.push(sketch);
        self
    }

    pub fn fill(&mut self, path: Path, brush: impl Into<Brush>) -> &mut Self {
        self.push(Sketch::Fill {
            path,
            brush: brush.into(),
        })
    }

    pub fn stroke(&mut self, path: Path, brush: impl Into<Brush>, width: f32) -> &mut Self {
        self.stroke_styled(path, brush, width, StrokeStyle::default())
    }

    /// Stroke with caps, joins and dashes.
    ///
    /// A separate entry point rather than a fourth argument on
    /// [`stroke`](Self::stroke), because almost every call site wants the plain
    /// one and a `StrokeStyle::default()` at each of them would be noise that
    /// hides the two that do not.
    pub fn stroke_styled(
        &mut self,
        path: Path,
        brush: impl Into<Brush>,
        width: f32,
        style: StrokeStyle,
    ) -> &mut Self {
        self.push(Sketch::Stroke {
            path,
            brush: brush.into(),
            width,
            style,
        })
    }

    pub fn rect(&mut self, rect: Rect, brush: impl Into<Brush>) -> &mut Self {
        self.fill(Path::rect(rect), brush)
    }

    pub fn rrect(&mut self, rect: Rect, radius: f32, brush: impl Into<Brush>) -> &mut Self {
        self.fill(Path::rounded_rect(rect, radius), brush)
    }

    pub fn stroke_rrect(
        &mut self,
        rect: Rect,
        radius: f32,
        brush: impl Into<Brush>,
        width: f32,
    ) -> &mut Self {
        self.stroke(Path::rounded_rect(rect, radius), brush, width)
    }

    /// A filled disc.
    pub fn circle(&mut self, center: Offset, radius: f32, brush: impl Into<Brush>) -> &mut Self {
        self.fill(Path::arc(center, radius, 0.0, std::f32::consts::TAU), brush)
    }

    /// A circular outline, as a filled ring rather than a stroke, so its width
    /// is exact at any radius.
    pub fn ring(
        &mut self,
        center: Offset,
        radius: f32,
        width: f32,
        brush: impl Into<Brush>,
    ) -> &mut Self {
        self.fill(
            Path::arc_ring(center, radius, width, 0.0, std::f32::consts::TAU),
            brush,
        )
    }

    /// A slice of a ring: progress arcs, gauges, spinners. Angles in radians,
    /// zero at three o'clock, sweeping clockwise.
    pub fn arc(
        &mut self,
        center: Offset,
        radius: f32,
        width: f32,
        start: f32,
        sweep: f32,
        brush: impl Into<Brush>,
    ) -> &mut Self {
        self.fill(Path::arc_ring(center, radius, width, start, sweep), brush)
    }

    /// A straight line, as a stroke.
    pub fn line(
        &mut self,
        from: Offset,
        to: Offset,
        brush: impl Into<Brush>,
        width: f32,
    ) -> &mut Self {
        let mut path = Path::new();
        path.move_to(from).line_to(to);
        self.stroke(path, brush, width)
    }

    /// A rounded-rectangle shadow under whatever is drawn next.
    pub fn shadow(&mut self, rect: Rect, radius: f32, shadow: Shadow) -> &mut Self {
        self.push(Sketch::Shadow {
            rect,
            radius,
            shadow,
        })
    }

    /// A composited group. `build` draws into a fresh book, and the result is
    /// pushed as one [`Sketch::Layer`], composited `Normal`.
    ///
    /// # A blurred layer is priced per layer
    ///
    /// `blur > 0` costs one offscreen buffer and one kernel pass **per
    /// layer**. Wrapping each of eleven light shafts in its own blurred layer
    /// took a render from 15 ms to 140 ms; moving the blur up one level so the
    /// *group* shares a single layer brought it to 60 ms and looked better
    /// (todo-upgrades U-06). The expensive and the cheap version look
    /// identical at the call site, and the expensive one is the one you write
    /// first, because you are thinking about one shaft at a time — so the
    /// receipt is [`SceneReport::filtered_layers`]
    /// (vieww_paint::native::reference), which counts what you actually
    /// bought. Group early, blur once.
    ///
    /// # An invisible painter is not a free painter
    ///
    /// `alpha: 0.0` skips the *compositing*, not the recording: every `Sketch`
    /// item is still built and allocated before the replay can see the alpha
    /// (todo-upgrades U-19). A painter that fades to zero costs the same as
    /// one at full opacity; skip the work in `paint`, not with `alpha`.
    pub fn layer(
        &mut self,
        alpha: f32,
        blur: f32,
        clip: Option<Path>,
        build: impl FnOnce(&mut Self),
    ) -> &mut Self {
        self.blended_layer(alpha, blur, BlendMode::Normal, clip, build)
    }

    /// A composited group that combines with what is behind it using `blend`
    /// rather than always `Normal` (todo-upgrades U-01).
    ///
    /// This is how additive light — beams, bloom, glow, dust — stays *one*
    /// drawing: `BlendMode::Plus` on the layer, rather than N widgets each
    /// wrapped in `Opacity::new(1.0).blend(mode)`. The widget form still wins
    /// when a light source needs a name and an addressable node — keep both,
    /// and pick by whether anything needs to point at the light.
    ///
    /// Same pricing as [`layer`](Self::layer): one offscreen per blurred
    /// group, and a blend mode other than `Normal` is a compositing layer
    /// even at `blur: 0.0`.
    pub fn blended_layer(
        &mut self,
        alpha: f32,
        blur: f32,
        blend: BlendMode,
        clip: Option<Path>,
        build: impl FnOnce(&mut Self),
    ) -> &mut Self {
        let mut inner = Self::new();
        build(&mut inner);
        self.push(Sketch::Layer {
            alpha,
            blur,
            blend,
            clip,
            children: inner.into_items(),
        })
    }

    /// Fill a grid of abutting cells, each bled `bleed` (a fraction of cell
    /// size) into its neighbours (todo-upgrades U-05).
    ///
    /// # Why the bleed exists: the conflation artifact
    ///
    /// Two paths that share an edge do not meet. Each is anti-aliased against
    /// what is already in the target, so the shared edge receives two partial
    /// coverages and a hairline of background survives between them. One quad
    /// hides it; a 96×64 grid of shaded quads reads as a wireframe nobody
    /// asked for. This is inherent to coverage-based compositing — not a
    /// rasterizer bug — and it is invisible until it is catastrophic.
    ///
    /// The fix is to overlap every cell a fraction into its neighbours;
    /// `0.0035` is the value the ocean plate settled on, plus a fraction on
    /// the depth axis where the shading varies fastest. `cell` receives the
    /// **un-bled** rectangle, so shading decisions are made on the honest
    /// geometry, and the fill covers the bled one.
    pub fn mesh(
        &mut self,
        bounds: Rect,
        cols: usize,
        rows: usize,
        bleed: f32,
        mut cell: impl FnMut(usize, usize, Rect) -> Brush,
    ) -> &mut Self {
        debug_assert!(cols > 0 && rows > 0, "a mesh needs at least one cell");
        let (cw, ch) = (bounds.width() / cols as f32, bounds.height() / rows as f32);
        let (bx, by) = (bleed * cw, bleed * ch);
        for row in 0..rows {
            for col in 0..cols {
                let rect = Rect::new(
                    bounds.left + col as f32 * cw,
                    bounds.top + row as f32 * ch,
                    bounds.left + (col + 1) as f32 * cw,
                    bounds.top + (row + 1) as f32 * ch,
                );
                let brush = cell(col, row, rect);
                if !brush.is_invisible() {
                    // Bled independently per axis: a fraction of the cell's own
                    // width into its horizontal neighbours, of its height into
                    // the vertical ones.
                    let bled = Rect::new(
                        rect.left - bx,
                        rect.top - by,
                        rect.right + bx,
                        rect.bottom + by,
                    );
                    self.fill(Path::rect(bled), brush);
                }
            }
        }
        self
    }

    /// A drop shadow under an arbitrary path (todo-upgrades U-12).
    ///
    /// [`Sketch::Shadow`] is rounded rectangles only, because that is the
    /// caster the renderer's `DrawShadow` takes — a letterform is not a
    /// rounded rectangle. This performs the documented construction for any
    /// path: the path, translated by the shadow's offset, filled with the
    /// shadow's colour inside a blurred layer. One offscreen per shadow, so
    /// the cost is visible as one call rather than re-derived at every call
    /// site; at eighty shards or five letters it is fine, at a hundred it
    /// would not be.
    ///
    /// Outer shadows only — an inset shadow under a path is a clip plus this,
    /// and belongs to the caller who knows which side of the path is "inside".
    pub fn path_shadow(&mut self, path: Path, shadow: Shadow) -> &mut Self {
        let (offset, blur, color) = (shadow.offset, shadow.blur, shadow.color);
        self.layer(1.0, blur, None, |g| {
            g.transformed(Transform::translate(offset), |h| {
                h.fill(path, color);
            });
        })
    }

    /// The transform-outside/clip-inside pattern (todo-upgrades U-11): a
    /// window onto `source` that flies away with the window still attached.
    ///
    /// `transform` positions the window; `clip` — in the **pre-transform**
    /// space — is carried along with the contents, so the transform moves the
    /// clip *and* what it shows together. The same source drawing seen
    /// through many moving windows is the shape of every shatter, wipe,
    /// reveal and split-screen transition.
    ///
    /// A painter cannot read what has already been painted beneath it —
    /// [`Filtered::with_backdrop`](vieww_widget::Filtered) is the one thing in
    /// the tree that can — so each window redraws the source, clipped to the
    /// window's own outline: N windows means N redraws of the source, real
    /// but affordable (eighty shards of a fifteen-shape drawing: 67 ms).
    ///
    /// ```no_run
    /// # use vieww_foundation::{Offset, Path, Rect, Sketchbook, Transform};
    /// # let mut book = Sketchbook::new();
    /// # let shard = Path::rect(Rect::new(0.0, 0.0, 8.0, 8.0));
    /// # let flight = Transform::translate(Offset::new(40.0, 12.0));
    /// // one shard of a shatter: the artwork redrawn inside a flying window
    /// book.window(flight, shard, 1.0, |g| {
    ///     g.rect(Rect::new(0.0, 0.0, 200.0, 100.0), /* the artwork */
    ///            vieww_foundation::Color::RED);
    /// });
    /// ```
    pub fn window(
        &mut self,
        transform: Transform,
        clip: Path,
        alpha: f32,
        source: impl FnOnce(&mut Self),
    ) -> &mut Self {
        self.transformed(transform, |g| {
            g.layer(alpha, 0.0, Some(clip), source);
        })
    }

    /// A group drawn through `transform`.
    pub fn transformed(
        &mut self,
        transform: Transform,
        build: impl FnOnce(&mut Self),
    ) -> &mut Self {
        let mut inner = Self::new();
        build(&mut inner);
        self.push(Sketch::Transformed {
            transform,
            children: inner.into_items(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_solid_and_a_gradient_are_both_brushes() {
        let solid: Brush = Color::RED.into();
        let ramp: Brush = Gradient::vertical().between(Color::RED, Color::BLUE).into();
        assert!(!solid.is_invisible());
        assert!(!ramp.is_invisible());
        assert!(Brush::Solid(Color::TRANSPARENT).is_invisible());
    }

    #[test]
    fn the_conveniences_record_one_item_each() {
        let mut book = Sketchbook::new();
        book.rect(Rect::new(0.0, 0.0, 10.0, 10.0), Color::RED);
        book.rrect(Rect::new(0.0, 0.0, 10.0, 10.0), 2.0, Color::RED);
        book.circle(Offset::new(5.0, 5.0), 4.0, Color::RED);
        book.ring(Offset::new(5.0, 5.0), 4.0, 1.0, Color::RED);
        book.line(Offset::ZERO, Offset::new(10.0, 0.0), Color::RED, 1.0);
        assert_eq!(book.len(), 5);
    }

    #[test]
    fn a_layer_nests_what_it_built() {
        let mut book = Sketchbook::new();
        book.layer(0.5, 2.0, None, |inner| {
            inner.rect(Rect::new(0.0, 0.0, 4.0, 4.0), Color::RED);
            inner.rect(Rect::new(4.0, 0.0, 8.0, 4.0), Color::BLUE);
        });
        let Sketch::Layer {
            alpha,
            blur,
            children,
            ..
        } = &book.items()[0]
        else {
            panic!("the one item is the layer")
        };
        assert!((alpha - 0.5).abs() < f32::EPSILON);
        assert!((blur - 2.0).abs() < f32::EPSILON);
        assert_eq!(children.len(), 2);
    }

    /// Skipping an invisible subtree is what keeps a painter that fades to zero
    /// from costing a layer push per frame at the end of the fade.
    #[test]
    fn nothing_visible_is_reported_as_nothing() {
        assert!(Sketch::Fill {
            path: Path::new(),
            brush: Color::RED.into()
        }
        .is_invisible());
        assert!(Sketch::Stroke {
            path: Path::rect(Rect::new(0.0, 0.0, 4.0, 4.0)),
            brush: Color::RED.into(),
            width: 0.0,
            style: StrokeStyle::default()
        }
        .is_invisible());
        assert!(Sketch::Layer {
            alpha: 0.0,
            blur: 0.0,
            blend: BlendMode::Normal,
            clip: None,
            children: vec![Sketch::Fill {
                path: Path::rect(Rect::new(0.0, 0.0, 4.0, 4.0)),
                brush: Color::RED.into()
            }],
        }
        .is_invisible());
    }

    /// U-01: a blended layer records its mode, and `layer` stays `Normal`.
    #[test]
    fn a_blended_layer_records_its_mode() {
        let mut book = Sketchbook::new();
        book.blended_layer(1.0, 0.0, crate::BlendMode::Plus, None, |inner| {
            inner.circle(Offset::new(5.0, 5.0), 4.0, Color::WHITE);
        });
        let Sketch::Layer { blend, .. } = &book.items()[0] else {
            panic!("the one item is the layer")
        };
        assert_eq!(*blend, crate::BlendMode::Plus);

        book.layer(1.0, 0.0, None, |inner| {
            inner.rect(Rect::new(0.0, 0.0, 4.0, 4.0), Color::RED);
        });
        let Sketch::Layer { blend, .. } = &book.items()[1] else {
            panic!("the second item is the plain layer")
        };
        assert_eq!(*blend, crate::BlendMode::Normal);
    }

    /// U-05: every cell is bled into its neighbours, per axis, and the closure
    /// sees the honest un-bled rectangle.
    #[test]
    fn a_mesh_bleeds_every_cell() {
        let mut book = Sketchbook::new();
        let bounds = Rect::new(0.0, 0.0, 100.0, 50.0);
        book.mesh(bounds, 2, 2, 0.1, |_, _, rect| {
            // The honest cell: exactly half the bounds, no bleed.
            assert!((rect.width() - 50.0).abs() < 1e-4);
            assert!((rect.height() - 25.0).abs() < 1e-4);
            Color::RED.into()
        });
        assert_eq!(book.len(), 4);
        let Sketch::Fill { path, .. } = &book.items()[0] else {
            panic!("mesh fills are plain paths")
        };
        let b = path.bounds();
        // 0.1 of a 50-wide cell is 5 each side; of a 25-high cell, 2.5.
        assert!((b.left - (0.0 - 5.0)).abs() < 1e-4);
        assert!((b.top - (0.0 - 2.5)).abs() < 1e-4);
        assert!((b.right - (50.0 + 5.0)).abs() < 1e-4);
        assert!((b.bottom - (25.0 + 2.5)).abs() < 1e-4);
    }

    /// U-12: a path shadow is the documented construction — a blurred layer
    /// holding a translated fill of the path.
    #[test]
    fn a_path_shadow_is_a_blurred_translated_fill() {
        let mut book = Sketchbook::new();
        book.path_shadow(
            Path::rect(Rect::new(0.0, 0.0, 10.0, 10.0)),
            Shadow::new(Color::BLACK, Offset::new(0.0, 4.0), 8.0),
        );
        let Sketch::Layer { blur, children, .. } = &book.items()[0] else {
            panic!("the shadow is a layer")
        };
        assert!((blur - 8.0).abs() < f32::EPSILON);
        let Sketch::Transformed { transform, children } = &children[0] else {
            panic!("inside the layer, the fill flies")
        };
        assert!((transform.ty - 4.0).abs() < f32::EPSILON);
        assert_eq!(children.len(), 1);
    }

    /// U-11: a window is a transform *outside* a clipped layer, so the clip
    /// travels with the flight.
    #[test]
    fn a_window_carries_its_clip_through_the_transform() {
        let mut book = Sketchbook::new();
        let flight = Transform::translate(Offset::new(40.0, 0.0));
        book.window(
            flight,
            Path::rect(Rect::new(0.0, 0.0, 8.0, 8.0)),
            0.5,
            |inner| {
                inner.rect(Rect::new(0.0, 0.0, 200.0, 100.0), Color::RED);
            },
        );
        let Sketch::Transformed { children, .. } = &book.items()[0] else {
            panic!("the transform is outside")
        };
        let Sketch::Layer { alpha, clip, children, .. } = &children[0] else {
            panic!("the clip is inside")
        };
        assert!((alpha - 0.5).abs() < f32::EPSILON);
        assert!(clip.is_some());
        assert_eq!(children.len(), 1);
    }
}
