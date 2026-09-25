//! The `Painter` that records a `.3` frame into a Vieww `Sketchbook`.

use std::rc::Rc;

use three_core::Capture3D;
use three_vieww::{
    MeshShading, OrbitCamera, Perspective, Projector, Viewport, projected_points, shaded_triangles,
    wireframe_edges,
};
use vieww::foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook};
use vieww::widget::{Painter, ThemeData};

/// The colours the media surface draws with.
///
/// The mesh, wire and mote colours come from the theme — the same rule every
/// vieww chart follows: a viewer embedded in a differently-accented app picks
/// up that accent. The chamber behind the geometry is fixed and dark, like
/// every photo and video viewer's letterbox: a dark surround is what makes
/// shaded geometry readable, and following the theme here would mean a
/// light-theme app washing the capture out against near-white.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewPalette {
    /// Top of the chamber gradient.
    pub background_top: Color,
    /// Bottom of the chamber gradient.
    pub background_bottom: Color,
    /// The base colour of the mesh; per-triangle brightness scales it.
    pub mesh: Color,
    /// Wireframe line colour.
    pub wire: Color,
    /// Point-mote colour.
    pub points: Color,
}

impl ViewPalette {
    /// Resolve the palette from the theme the widget is built under.
    pub fn from_theme(theme: &ThemeData) -> Self {
        let primary = theme.colors.primary;
        Self {
            background_top: Color::rgb(23, 27, 36),
            background_bottom: Color::rgb(11, 13, 19),
            mesh: primary,
            wire: primary,
            // Motes are the primary lifted toward white so they read at
            // small radii against the dark chamber in either theme.
            points: primary.lerp(Color::WHITE, 0.55),
        }
    }
}

/// Multiply a colour by a 0..1 brightness, channel-wise.
///
/// Shading in sRGB-encoded space rather than linear: this is a flat-shaded
/// reference viewer, and what it wants is "darker blue", not a physically
/// correct falloff — the same choice CSS/design tools make for tints.
pub fn shade(color: Color, brightness: f32) -> Color {
    let brightness = brightness.clamp(0.0, 1.0);
    Color::rgba(
        (color.r as f32 * brightness).round().clamp(0.0, 255.0) as u8,
        (color.g as f32 * brightness).round().clamp(0.0, 255.0) as u8,
        (color.b as f32 * brightness).round().clamp(0.0, 255.0) as u8,
        color.a,
    )
}

/// Records one moment of a capture: shaded triangles or wireframe, plus the
/// point motes, over a dark chamber gradient.
///
/// The widget's build creates this with the current camera, frame indices
/// and options; the geometry is projected at paint time, when the laid-out
/// size is known, and [`should_repaint`](Painter::should_repaint) keeps an
/// unchanged frame from being re-projected.
///
/// Motes are drawn after the mesh rather than depth-interleaved with it:
/// they float above the surface, so they are almost always nearer the camera
/// than the geometry behind them, and a mixed sort would buy nothing for
/// decorative points. The mesh itself is drawn far-to-near (the order
/// [`shaded_triangles`] returns).
pub struct MeshPainter {
    /// The capture being drawn; shared, never cloned per rebuild.
    pub capture: Rc<Capture3D>,
    /// Which mesh frame, already resolved from playback time by the widget.
    pub mesh_index: Option<usize>,
    /// Which point frame, resolved the same way.
    pub points_index: Option<usize>,
    pub camera: OrbitCamera,
    pub wireframe: bool,
    pub show_points: bool,
    pub palette: ViewPalette,
    pub shading: MeshShading,
    pub lens: Perspective,
}

impl std::fmt::Debug for MeshPainter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MeshPainter")
            .field("capture", &self.capture.title)
            .field("mesh_index", &self.mesh_index)
            .field("points_index", &self.points_index)
            .field("camera", &self.camera)
            .field("wireframe", &self.wireframe)
            .field("show_points", &self.show_points)
            .field("palette", &self.palette)
            .finish()
    }
}

impl MeshPainter {
    /// Mote radius for a point at `depth` meters: 4 px at 3 m, nearer points
    /// larger, clamped so a mote is never invisible nor a blob.
    fn mote_radius(depth: f32) -> f32 {
        (12.0 / depth.max(0.1)).clamp(1.5, 6.0)
    }
}

impl Painter for MeshPainter {
    fn paint(&self, book: &mut Sketchbook, size: Size) {
        let (width, height) = (size.width, size.height);
        if width < 2.0 || height < 2.0 {
            // A degenerate box has no drawing to record; the gradient would
            // be an empty rect and every projected point would be (0, 0).
            return;
        }

        // The chamber.
        let chamber =
            Gradient::vertical().between(self.palette.background_top, self.palette.background_bottom);
        book.rect(Rect::new(0.0, 0.0, width, height), chamber);

        let viewport = Viewport::new(width, height);
        let projector = Projector::new(&self.camera, self.lens, viewport);

        // The mesh.
        if let Some(index) = self.mesh_index {
            if let Some(mesh) = self.capture.meshes.get(index) {
                if self.wireframe {
                    for edge in wireframe_edges(mesh, &projector) {
                        book.line(
                            Offset::new(edge.from.x, edge.from.y),
                            Offset::new(edge.to.x, edge.to.y),
                            self.palette.wire,
                            1.0,
                        );
                    }
                } else {
                    for triangle in shaded_triangles(mesh, &projector, &self.shading) {
                        let mut path = Path::new();
                        path.move_to(Offset::new(triangle.a.x, triangle.a.y))
                            .line_to(Offset::new(triangle.b.x, triangle.b.y))
                            .line_to(Offset::new(triangle.c.x, triangle.c.y))
                            .close();
                        book.fill(path, shade(self.palette.mesh, triangle.brightness));
                    }
                }
            }
        }

        // The motes.
        if self.show_points {
            if let Some(index) = self.points_index {
                if let Some(frame) = self.capture.points.get(index) {
                    for point in projected_points(frame, &projector) {
                        book.circle(
                            Offset::new(point.x, point.y),
                            Self::mote_radius(point.depth),
                            self.palette.points,
                        );
                    }
                }
            }
        }
    }

    fn should_repaint(&self, previous: &dyn Painter) -> bool {
        match previous.as_any().downcast_ref::<Self>() {
            Some(previous) => {
                !Rc::ptr_eq(&self.capture, &previous.capture)
                    || self.mesh_index != previous.mesh_index
                    || self.points_index != previous.points_index
                    || self.camera != previous.camera
                    || self.wireframe != previous.wireframe
                    || self.show_points != previous.show_points
                    || self.palette != previous.palette
                    || self.shading != previous.shading
                    || self.lens != previous.lens
            }
            // A painter of a different type is a different drawing.
            None => true,
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
