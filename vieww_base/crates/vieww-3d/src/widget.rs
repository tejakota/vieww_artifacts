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

use crate::raycast::{raycast, Hit, Ray};
use crate::render::{RenderStats, Renderer};
use crate::scene::{Camera, Scene};

/// R3F's `useFrame`: called with the scene before each render.
pub type FrameHook = Rc<dyn Fn(&mut Scene, &Camera)>;
/// R3F's `onPointerDown` on meshes: the nearest hit under a tap, if any.
pub type PickHandler = Rc<dyn Fn(Option<Hit>)>;

/// See the [module docs](self).
#[derive(Clone)]
pub struct Viewport3D {
    scene: Rc<RefCell<Scene>>,
    camera: Camera,
    renderer: Renderer,
    stats: Option<Rc<RefCell<RenderStats>>>,
    on_frame: Option<FrameHook>,
    on_pick: Option<PickHandler>,
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
            on_frame: None,
            on_pick: None,
        }
    }

    /// Run `f` on the scene before every render (`useFrame`).
    #[must_use]
    pub fn on_frame(mut self, f: impl Fn(&mut Scene, &Camera) + 'static) -> Self {
        self.on_frame = Some(Rc::new(f));
        self
    }

    /// Ray-pick under taps: `f` receives the nearest [`Hit`] (node,
    /// instance, point, normal, uv, triangle) or `None` for empty space.
    #[must_use]
    pub fn on_pick(mut self, f: impl Fn(Option<Hit>) + 'static) -> Self {
        self.on_pick = Some(Rc::new(f));
        self
    }

    /// The pick a tap at local `(x, y)` would produce — what `on_pick`
    /// delivers, callable directly for tests and keyboard picking.
    #[must_use]
    pub fn pick(&self, x: f32, y: f32) -> Option<Hit> {
        #[allow(clippy::cast_precision_loss)]
        let (w, h) = (self.renderer.width as f32, self.renderer.height as f32);
        let ray = Ray::from_screen(&self.camera, w, h, x, y);
        raycast(&mut self.scene.borrow_mut(), &ray).into_iter().next()
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
        if let Some(f) = &self.on_frame {
            f(&mut self.scene.borrow_mut(), &self.camera);
        }
        let (image, stats) = self
            .renderer
            .render(&mut self.scene.borrow_mut(), &self.camera);
        if let Some(s) = &self.stats {
            *s.borrow_mut() = stats;
        }
        #[allow(clippy::cast_precision_loss)]
        let (w, h) = (self.renderer.width as f32, self.renderer.height as f32);
        let img: WidgetNode = vieww_widget::Image::new(image)
            .width(w)
            .height(h)
            .label("3D viewport")
            .into();
        match &self.on_pick {
            None => img,
            Some(handler) => {
                let me = self.clone();
                let handler = handler.clone();
                vieww_widget::GestureDetector::new()
                    .on_tap(move |t| handler(me.pick(t.local.dx, t.local.dy)))
                    .child(img)
                    .into()
            }
        }
    }
}

widget_node_from!(Viewport3D);
