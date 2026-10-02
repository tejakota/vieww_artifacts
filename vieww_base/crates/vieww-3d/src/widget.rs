//! `Viewport3D` — a 3D scene as a widget, R3F's `<Canvas>` (§2.3 L4).
//!
//! The widget owns nothing but a shared handle to a [`Scene`], a camera and
//! a renderer configuration; `build` renders and hands the pixels to an
//! [`Image`](vieww_widget::Image). So a 3D view composes with everything
//! else in the tree — padding, stacks, overlays of 2D labels positioned with
//! [`project`](crate::project) — and rebuilds whenever a signal it read in
//! its parent's `build` changes, exactly like any other widget.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use vieww_widget::{widget_node_from, BuildContext, Widget, WidgetKind, WidgetNode};

use crate::render::{RenderStats, Renderer};
use crate::scene::{Camera, Scene};

/// See the [module docs](self).
#[derive(Clone)]
pub struct Viewport3D {
    scene: Rc<RefCell<Scene>>,
    camera: Camera,
    renderer: Renderer,
    stats: Option<Rc<RefCell<RenderStats>>>,
}

impl fmt::Debug for Viewport3D {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Viewport3D")
            .field("nodes", &self.scene.borrow().len())
            .field("camera", &self.camera)
            .field("renderer", &self.renderer)
            .finish()
    }
}

impl Viewport3D {
    #[must_use]
    pub fn new(scene: Rc<RefCell<Scene>>, camera: Camera, renderer: Renderer) -> Self {
        Self {
            scene,
            camera,
            renderer,
            stats: None,
        }
    }

    /// Write each render's [`RenderStats`] here.
    #[must_use]
    pub fn report_to(mut self, stats: Rc<RefCell<RenderStats>>) -> Self {
        self.stats = Some(stats);
        self
    }
}

impl Widget for Viewport3D {
    fn debug_name(&self) -> &'static str {
        "Viewport3D"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let (image, stats) = self
            .renderer
            .render(&mut self.scene.borrow_mut(), &self.camera);
        if let Some(s) = &self.stats {
            *s.borrow_mut() = stats;
        }
        #[allow(clippy::cast_precision_loss)]
        let (w, h) = (self.renderer.width as f32, self.renderer.height as f32);
        vieww_widget::Image::new(image)
            .width(w)
            .height(h)
            .label("3D viewport")
            .into()
    }
}

widget_node_from!(Viewport3D);
