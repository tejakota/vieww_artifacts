//! The canvas layer — Konva.js and Fabric.js, on vieww's vector stack.
//!
//! # What the document's Konva table (§2.4) asks for, and where it is
//!
//! | Konva layer | here |
//! |---|---|
//! | L2 Stage / container, L4 node tree with `x`, `y`, `rotation`, `scaleX/Y`, `offset`, groups | [`Stage`], [`Node`], [`Attrs`] |
//! | L5 shapes — `Rect`, `Circle`, `Ellipse`, `Line`, `Path`, `Star`, `RegularPolygon`, `Arrow`, `Text` | [`Shape`] |
//! | L6 hit graph — pixel-perfect event detection | [`Stage::hit`]: **exact** geometric hit-testing (`Path::contains` / `stroke_contains` in each node's local frame), front to back, honouring `listening` and `visible` — no hidden colour-keyed canvas needed, and no colour-collision limit |
//! | events with **bubbling** and `cancelBubble` | [`Stage::on`], [`Stage::dispatch`] |
//! | `draggable` | [`Stage::pointer_down`] / `pointer_move` / `pointer_up` |
//! | `Transformer` — resize / rotate handles | [`Transformer`] |
//! | L8 filters on cached nodes (`Blur`, `Grayscale`, `Brighten`, `Invert`, `Sepia`) | [`Filter`] |
//! | serialisation (`toJSON`, `Node.create`) | [`Stage::to_json`], [`Stage::from_json`] |
//! | z-order (`moveToTop`, `zIndex`), `find('#id')`, `find('.name')` | [`Stage::move_to_top`], [`Stage::find`] |
//! | selection / marquee | [`Stage::nodes_in_rect`] |
//!
//! A stage renders to a [`Sketchbook`] (so it is drawn by the same
//! rasteriser as everything else) plus a list of [`TextItem`]s for the text
//! shapes; [`CanvasView`] puts both in the widget tree.

pub mod tween;

use std::collections::BTreeMap;
use std::fmt;

use vieww_foundation::json::Json;
use vieww_foundation::{
    Brush, Color, FillRule, Offset, Path, Rect, Sketch, Sketchbook, StrokeStyle, Transform,
};

mod view;
pub use view::CanvasView;

/// A node's identity within its stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub usize);

/// What a shape draws, in its own local frame.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Rect {
        width: f32,
        height: f32,
        corner: f32,
    },
    Circle {
        radius: f32,
    },
    Ellipse {
        rx: f32,
        ry: f32,
    },
    /// A polyline; `closed` joins the ends and makes it fillable.
    Line {
        points: Vec<Offset>,
        closed: bool,
    },
    Path(Path),
    Star {
        points: u32,
        inner: f32,
        outer: f32,
    },
    RegularPolygon {
        sides: u32,
        radius: f32,
    },
    /// A line with an arrowhead at the end.
    Arrow {
        points: Vec<Offset>,
        head: f32,
    },
    /// Text drawn by the widget layer at the node's transform.
    Text {
        text: String,
        size: f32,
    },
}

impl Shape {
    /// The outline as a path (text: its approximate box).
    #[must_use]
    pub fn path(&self) -> Path {
        match self {
            Self::Rect {
                width,
                height,
                corner,
            } => {
                let r = Rect::new(0.0, 0.0, *width, *height);
                if *corner > 0.0 {
                    Path::rounded_rect(r, *corner)
                } else {
                    Path::rect(r)
                }
            }
            Self::Circle { radius } => Path::arc(Offset::ZERO, *radius, 0.0, std::f32::consts::TAU),
            Self::Ellipse { rx, ry } => Path::arc(Offset::ZERO, 1.0, 0.0, std::f32::consts::TAU)
                .transformed(Transform::scale(*rx, *ry)),
            Self::Line { points, closed } => {
                let mut p = Path::new();
                if let Some(first) = points.first() {
                    p.move_to(*first);
                    for q in &points[1..] {
                        p.line_to(*q);
                    }
                    if *closed {
                        p.close();
                    }
                }
                p
            }
            Self::Path(p) => p.clone(),
            Self::Star {
                points,
                inner,
                outer,
            } => star(*points, *inner, *outer),
            Self::RegularPolygon { sides, radius } => star(*sides, *radius, *radius),
            Self::Arrow { points, head } => {
                let mut p = Self::Line {
                    points: points.clone(),
                    closed: false,
                }
                .path();
                if points.len() >= 2 {
                    let (a, b) = (points[points.len() - 2], points[points.len() - 1]);
                    let d = b - a;
                    let len = d.distance().max(1e-6);
                    let u = d.scale(1.0 / len);
                    let n = Offset::new(-u.dy, u.dx);
                    let base = b - u.scale(*head);
                    p.move_to(base + n.scale(head * 0.5));
                    p.line_to(b);
                    p.line_to(base - n.scale(head * 0.5));
                }
                p
            }
            Self::Text { text, size } => {
                #[allow(clippy::cast_precision_loss)]
                let w = text.chars().count() as f32 * size * 0.55;
                Path::rect(Rect::new(0.0, 0.0, w, size * 1.25))
            }
        }
    }

    /// Whether the shape is a stroke-only kind (open line, arrow).
    #[must_use]
    pub const fn is_open(&self) -> bool {
        matches!(self, Self::Line { closed: false, .. } | Self::Arrow { .. })
    }

    const fn kind_name(&self) -> &'static str {
        match self {
            Self::Rect { .. } => "Rect",
            Self::Circle { .. } => "Circle",
            Self::Ellipse { .. } => "Ellipse",
            Self::Line { .. } => "Line",
            Self::Path(_) => "Path",
            Self::Star { .. } => "Star",
            Self::RegularPolygon { .. } => "RegularPolygon",
            Self::Arrow { .. } => "Arrow",
            Self::Text { .. } => "Text",
        }
    }
}

fn star(n: u32, inner: f32, outer: f32) -> Path {
    let mut p = Path::new();
    let count = if (inner - outer).abs() < 1e-6 {
        n
    } else {
        n * 2
    };
    for i in 0..count.max(3) {
        #[allow(clippy::cast_precision_loss)]
        let a =
            -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::TAU / count.max(3) as f32;
        let r = if count == n || i % 2 == 0 {
            outer
        } else {
            inner
        };
        let q = Offset::new(a.cos() * r, a.sin() * r);
        if i == 0 {
            p.move_to(q);
        } else {
            p.line_to(q);
        }
    }
    p.close();
    p
}

/// A pixel filter applied to a cached node — Konva's L8.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Filter {
    Blur(f32),
    Grayscale,
    /// −1..1.
    Brighten(f32),
    Invert,
    Sepia,
}

fn filter_color(c: Color, filters: &[Filter]) -> Color {
    let mut rgb = [f32::from(c.r), f32::from(c.g), f32::from(c.b)];
    for f in filters {
        match *f {
            Filter::Grayscale => {
                let l = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
                rgb = [l; 3];
            }
            Filter::Brighten(b) => rgb = rgb.map(|v| v + b * 255.0),
            Filter::Invert => rgb = rgb.map(|v| 255.0 - v),
            Filter::Sepia => {
                let [r, g, b] = rgb;
                rgb = [
                    0.393 * r + 0.769 * g + 0.189 * b,
                    0.349 * r + 0.686 * g + 0.168 * b,
                    0.272 * r + 0.534 * g + 0.131 * b,
                ];
            }
            Filter::Blur(_) => {}
        }
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let q = |v: f32| v.clamp(0.0, 255.0).round() as u8;
    Color::rgba(q(rgb[0]), q(rgb[1]), q(rgb[2]), c.a)
}

/// A node's attributes — Konva's `attrs`.
#[derive(Debug, Clone, PartialEq)]
pub struct Attrs {
    /// The `#id` selector.
    pub id: String,
    /// Space-separated `.name` classes.
    pub name: String,
    pub x: f32,
    pub y: f32,
    /// Degrees, clockwise (Konva's unit).
    pub rotation: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    /// The local point that sits at `(x, y)` and that rotation turns around.
    pub offset: Offset,
    pub opacity: f32,
    pub visible: bool,
    /// Receives events (Konva's `listening`).
    pub listening: bool,
    pub draggable: bool,
    pub fill: Option<Color>,
    pub stroke: Option<Color>,
    pub stroke_width: f32,
    /// Extra width for hit-testing strokes (`hitStrokeWidth`).
    pub hit_stroke_width: f32,
    pub filters: Vec<Filter>,
}

impl Default for Attrs {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            offset: Offset::ZERO,
            opacity: 1.0,
            visible: true,
            listening: true,
            draggable: false,
            fill: None,
            stroke: None,
            stroke_width: 1.0,
            hit_stroke_width: 0.0,
            filters: Vec::new(),
        }
    }
}

impl Attrs {
    /// The local → parent transform.
    #[must_use]
    pub fn transform(&self) -> Transform {
        Transform::translate(Offset::new(-self.offset.dx, -self.offset.dy))
            .then(Transform::scale(self.scale_x, self.scale_y))
            .then(Transform::rotate(self.rotation.to_radians()))
            .then(Transform::translate(Offset::new(self.x, self.y)))
    }
}

/// A node: a group, a layer or a shape.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub attrs: Attrs,
    /// `None` for a group (or layer).
    pub shape: Option<Shape>,
    pub children: Vec<NodeId>,
    pub parent: Option<NodeId>,
    alive: bool,
}

/// What happened to a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EventKind {
    PointerDown,
    PointerMove,
    PointerUp,
    Click,
    DragStart,
    DragMove,
    DragEnd,
    PointerEnter,
    PointerLeave,
}

/// An event as a handler sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Event {
    pub kind: EventKind,
    /// The node the pointer hit.
    pub target: NodeId,
    /// The node whose handler is running (the target or an ancestor).
    pub current: NodeId,
    pub point: Offset,
    cancel: bool,
}

impl Event {
    /// Stop this event bubbling further up — Konva's `cancelBubble`.
    pub fn cancel_bubble(&mut self) {
        self.cancel = true;
    }
}

type Handler = Box<dyn FnMut(&mut Event)>;

/// Text to draw, placed by the widget layer.
#[derive(Debug, Clone, PartialEq)]
pub struct TextItem {
    pub text: String,
    pub size: f32,
    pub color: Color,
    pub transform: Transform,
    pub opacity: f32,
}

/// The root: layers of nodes.
pub struct Stage {
    pub width: f32,
    pub height: f32,
    nodes: Vec<Node>,
    layers: Vec<NodeId>,
    handlers: BTreeMap<(NodeId, EventKind), Vec<Handler>>,
    drag: Option<(NodeId, Offset)>,
    hover: Option<NodeId>,
    down: Option<(NodeId, Offset)>,
    /// Every event dispatched, in order: `(kind, target, handled-by)` —
    /// the audit trail a test (or devtools) reads.
    pub log: Vec<(EventKind, NodeId, NodeId)>,
}

impl fmt::Debug for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Stage")
            .field("size", &(self.width, self.height))
            .field("nodes", &self.len())
            .field("layers", &self.layers.len())
            .finish_non_exhaustive()
    }
}

impl Stage {
    #[must_use]
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            nodes: Vec::new(),
            layers: Vec::new(),
            handlers: BTreeMap::new(),
            drag: None,
            hover: None,
            down: None,
            log: Vec::new(),
        }
    }

    fn push(&mut self, node: Node) -> NodeId {
        self.nodes.push(node);
        NodeId(self.nodes.len() - 1)
    }

    /// A new layer on top.
    pub fn add_layer(&mut self) -> NodeId {
        let id = self.push(Node {
            attrs: Attrs::default(),
            shape: None,
            children: Vec::new(),
            parent: None,
            alive: true,
        });
        self.layers.push(id);
        id
    }

    /// Add a group under `parent`.
    pub fn add_group(&mut self, parent: NodeId, attrs: Attrs) -> NodeId {
        self.add(parent, attrs, None)
    }

    /// Add a shape under `parent`.
    pub fn add_shape(&mut self, parent: NodeId, attrs: Attrs, shape: Shape) -> NodeId {
        self.add(parent, attrs, Some(shape))
    }

    fn add(&mut self, parent: NodeId, attrs: Attrs, shape: Option<Shape>) -> NodeId {
        let id = self.push(Node {
            attrs,
            shape,
            children: Vec::new(),
            parent: Some(parent),
            alive: true,
        });
        self.nodes[parent.0].children.push(id);
        id
    }

    /// Destroy a node and its subtree.
    pub fn destroy(&mut self, id: NodeId) {
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            stack.extend(self.nodes[n.0].children.clone());
            self.nodes[n.0].alive = false;
        }
        if let Some(p) = self.nodes[id.0].parent {
            self.nodes[p.0].children.retain(|c| *c != id);
        } else {
            self.layers.retain(|l| *l != id);
        }
    }

    #[must_use]
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.0]
    }

    pub fn attrs_mut(&mut self, id: NodeId) -> &mut Attrs {
        &mut self.nodes[id.0].attrs
    }

    /// Live nodes, excluding layers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes
            .iter()
            .filter(|n| n.alive && n.parent.is_some())
            .count()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[must_use]
    pub fn layers(&self) -> &[NodeId] {
        &self.layers
    }

    /// `#id` or `.name` selector (Konva's `find`), or a shape kind (`Rect`).
    #[must_use]
    pub fn find(&self, selector: &str) -> Vec<NodeId> {
        self.paint_order()
            .into_iter()
            .filter(|&id| {
                let n = &self.nodes[id.0];
                if let Some(id_sel) = selector.strip_prefix('#') {
                    n.attrs.id == id_sel
                } else if let Some(class) = selector.strip_prefix('.') {
                    n.attrs.name.split_whitespace().any(|c| c == class)
                } else {
                    n.shape.as_ref().is_some_and(|s| s.kind_name() == selector)
                }
            })
            .collect()
    }

    /// Move a node to the end of its parent's children (drawn last, on top).
    pub fn move_to_top(&mut self, id: NodeId) {
        if let Some(p) = self.nodes[id.0].parent {
            let kids = &mut self.nodes[p.0].children;
            kids.retain(|c| *c != id);
            kids.push(id);
        }
    }

    /// Set a node's index among its siblings.
    pub fn set_z_index(&mut self, id: NodeId, z: usize) {
        if let Some(p) = self.nodes[id.0].parent {
            let kids = &mut self.nodes[p.0].children;
            kids.retain(|c| *c != id);
            let z = z.min(kids.len());
            kids.insert(z, id);
        }
    }

    /// Every live node, parents before children, in paint order.
    #[must_use]
    pub fn paint_order(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = self.layers.iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            if !self.nodes[id.0].alive {
                continue;
            }
            out.push(id);
            stack.extend(self.nodes[id.0].children.iter().rev().copied());
        }
        out
    }

    /// Local → stage transform.
    #[must_use]
    pub fn absolute_transform(&self, id: NodeId) -> Transform {
        let mut chain = Vec::new();
        let mut cur = Some(id);
        while let Some(c) = cur {
            chain.push(c);
            cur = self.nodes[c.0].parent;
        }
        chain.iter().fold(Transform::IDENTITY, |acc, n| {
            acc.then(self.nodes[n.0].attrs.transform())
        })
    }

    fn effectively(&self, id: NodeId, f: impl Fn(&Attrs) -> bool) -> bool {
        let mut cur = Some(id);
        while let Some(c) = cur {
            if !f(&self.nodes[c.0].attrs) {
                return false;
            }
            cur = self.nodes[c.0].parent;
        }
        true
    }

    fn opacity(&self, id: NodeId) -> f32 {
        let mut o = 1.0;
        let mut cur = Some(id);
        while let Some(c) = cur {
            o *= self.nodes[c.0].attrs.opacity;
            cur = self.nodes[c.0].parent;
        }
        o
    }

    /// The node's box in stage coordinates (Konva's `getClientRect`).
    #[must_use]
    pub fn client_rect(&self, id: NodeId) -> Rect {
        let t = self.absolute_transform(id);
        let local = self.local_rect(id);
        t.apply_rect(local)
    }

    /// The node's box in its own frame (groups: union of children).
    #[must_use]
    pub fn local_rect(&self, id: NodeId) -> Rect {
        let n = &self.nodes[id.0];
        if let Some(s) = &n.shape {
            let b = s.path().bounds();
            let pad = if n.attrs.stroke.is_some() {
                n.attrs.stroke_width / 2.0
            } else {
                0.0
            };
            return b.inflate(pad);
        }
        let mut acc: Option<Rect> = None;
        for c in &n.children {
            let r = self.nodes[c.0]
                .attrs
                .transform()
                .apply_rect(self.local_rect(*c));
            acc = Some(acc.map_or(r, |a| a.union(r)));
        }
        acc.unwrap_or(Rect::ZERO)
    }

    /// The topmost listening, visible shape under `point` — the hit graph.
    #[must_use]
    pub fn hit(&self, point: Offset) -> Option<NodeId> {
        self.paint_order().into_iter().rev().find(|&id| {
            let n = &self.nodes[id.0];
            let Some(shape) = &n.shape else { return false };
            if !self.effectively(id, |a| a.visible && a.listening) {
                return false;
            }
            let Some(inv) = self.absolute_transform(id).invert() else {
                return false;
            };
            let local = inv.apply(point);
            let path = shape.path();
            let stroke_w = n
                .attrs
                .stroke
                .map_or(0.0, |_| n.attrs.stroke_width)
                .max(n.attrs.hit_stroke_width);
            let filled =
                !shape.is_open() && (n.attrs.fill.is_some() || matches!(shape, Shape::Text { .. }));
            (filled && path.contains(local, FillRule::NonZero))
                || (stroke_w > 0.0 && path.stroke_contains(local, stroke_w.max(1.0)))
        })
    }

    /// Every shape whose client rect lies inside `rect` (marquee select).
    #[must_use]
    pub fn nodes_in_rect(&self, rect: Rect) -> Vec<NodeId> {
        self.paint_order()
            .into_iter()
            .filter(|&id| {
                self.nodes[id.0].shape.is_some() && {
                    let r = self.client_rect(id);
                    r.left >= rect.left
                        && r.top >= rect.top
                        && r.right <= rect.right
                        && r.bottom <= rect.bottom
                }
            })
            .collect()
    }

    /// Listen for `kind` on `id` (Konva's `node.on('click', …)`).
    pub fn on(&mut self, id: NodeId, kind: EventKind, handler: impl FnMut(&mut Event) + 'static) {
        self.handlers
            .entry((id, kind))
            .or_default()
            .push(Box::new(handler));
    }

    /// Fire `kind` at `target` and bubble it to the root.
    pub fn dispatch(&mut self, kind: EventKind, target: NodeId, point: Offset) {
        let mut ev = Event {
            kind,
            target,
            current: target,
            point,
            cancel: false,
        };
        let mut cur = Some(target);
        while let Some(c) = cur {
            ev.current = c;
            if let Some(list) = self.handlers.get_mut(&(c, kind)) {
                self.log.push((kind, target, c));
                for h in list.iter_mut() {
                    h(&mut ev);
                }
            }
            if ev.cancel {
                break;
            }
            cur = self.nodes[c.0].parent;
        }
    }

    /// The draggable node a press on `id` picks up: `id` or its nearest
    /// draggable ancestor (a draggable group moves with its children).
    fn draggable_of(&self, id: NodeId) -> Option<NodeId> {
        let mut cur = Some(id);
        while let Some(c) = cur {
            if self.nodes[c.0].attrs.draggable {
                return Some(c);
            }
            cur = self.nodes[c.0].parent;
        }
        None
    }

    /// Pointer pressed at a stage point.
    pub fn pointer_down(&mut self, point: Offset) -> Option<NodeId> {
        let target = self.hit(point)?;
        self.dispatch(EventKind::PointerDown, target, point);
        self.down = Some((target, point));
        if let Some(d) = self.draggable_of(target) {
            self.drag = Some((d, point));
            self.dispatch(EventKind::DragStart, d, point);
        }
        Some(target)
    }

    /// Pointer moved: drags, and enter/leave for hover.
    pub fn pointer_move(&mut self, point: Offset) {
        if let Some((id, last)) = self.drag {
            // Move in the parent's frame so rotated/scaled parents drag
            // correctly.
            let parent_inv = self.nodes[id.0]
                .parent
                .map(|p| self.absolute_transform(p))
                .and_then(Transform::invert)
                .unwrap_or(Transform::IDENTITY);
            let a = parent_inv.apply(last);
            let b = parent_inv.apply(point);
            let at = &mut self.nodes[id.0].attrs;
            at.x += b.dx - a.dx;
            at.y += b.dy - a.dy;
            self.drag = Some((id, point));
            self.dispatch(EventKind::DragMove, id, point);
            return;
        }
        let over = self.hit(point);
        if over != self.hover {
            if let Some(old) = self.hover {
                self.dispatch(EventKind::PointerLeave, old, point);
            }
            if let Some(new) = over {
                self.dispatch(EventKind::PointerEnter, new, point);
            }
            self.hover = over;
        }
        if let Some(t) = over {
            self.dispatch(EventKind::PointerMove, t, point);
        }
    }

    /// Pointer released; a press and release on the same node is a click.
    pub fn pointer_up(&mut self, point: Offset) {
        if let Some((id, _)) = self.drag.take() {
            self.dispatch(EventKind::DragEnd, id, point);
        }
        if let Some((down, at)) = self.down.take() {
            self.dispatch(EventKind::PointerUp, down, point);
            if self.hit(point) == Some(down) && (point - at).distance() < 4.0 {
                self.dispatch(EventKind::Click, down, point);
            }
        }
    }

    /// Draw the stage.
    #[must_use]
    pub fn render(&self) -> (Sketchbook, Vec<TextItem>) {
        let mut book = Sketchbook::new();
        let mut texts = Vec::new();
        for id in self.paint_order() {
            let n = &self.nodes[id.0];
            let Some(shape) = &n.shape else { continue };
            if !self.effectively(id, |a| a.visible) {
                continue;
            }
            let t = self.absolute_transform(id);
            let opacity = self.opacity(id);
            let filters = &n.attrs.filters;
            let blur = filters
                .iter()
                .find_map(|f| {
                    if let Filter::Blur(s) = f {
                        Some(*s)
                    } else {
                        None
                    }
                })
                .unwrap_or(0.0);
            if let Shape::Text { text, size } = shape {
                texts.push(TextItem {
                    text: text.clone(),
                    size: *size,
                    color: filter_color(n.attrs.fill.unwrap_or(Color::BLACK), filters),
                    transform: t,
                    opacity,
                });
                continue;
            }
            let path = shape.path();
            let mut items = Vec::new();
            if let (Some(fill), false) = (n.attrs.fill, shape.is_open()) {
                items.push(Sketch::Fill {
                    path: path.clone(),
                    brush: Brush::Solid(filter_color(fill, filters)),
                });
            }
            if let Some(stroke) = n.attrs.stroke {
                items.push(Sketch::Stroke {
                    path,
                    brush: Brush::Solid(filter_color(stroke, filters)),
                    width: n.attrs.stroke_width,
                    style: StrokeStyle::rounded(),
                });
            }
            let group = Sketch::Transformed {
                transform: t,
                children: items,
            };
            if opacity < 1.0 || blur > 0.0 {
                book.push(Sketch::Layer {
                    alpha: opacity,
                    blur,
                    blend: vieww_foundation::BlendMode::Normal,
                    clip: None,
                    children: vec![group],
                });
            } else {
                book.push(group);
            }
        }
        (book, texts)
    }

    // ------------------------------------------------------------ JSON

    /// The whole stage as JSON — Konva's `stage.toJSON()`.
    #[must_use]
    pub fn to_json(&self) -> Json {
        let layers = self.layers.iter().map(|l| self.node_json(*l)).collect();
        Json::object([
            (
                "attrs",
                Json::object([
                    ("width", Json::from(self.width)),
                    ("height", Json::from(self.height)),
                ]),
            ),
            ("className", Json::from("Stage")),
            ("children", Json::Array(layers)),
        ])
    }

    fn node_json(&self, id: NodeId) -> Json {
        let n = &self.nodes[id.0];
        let a = &n.attrs;
        let d = Attrs::default();
        let mut attrs: Vec<(&str, Json)> = Vec::new();
        let color = |c: Color| Json::from(format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a));
        if !a.id.is_empty() {
            attrs.push(("id", Json::from(a.id.as_str())));
        }
        if !a.name.is_empty() {
            attrs.push(("name", Json::from(a.name.as_str())));
        }
        for (k, v, dv) in [
            ("x", a.x, d.x),
            ("y", a.y, d.y),
            ("rotation", a.rotation, d.rotation),
            ("scaleX", a.scale_x, d.scale_x),
            ("scaleY", a.scale_y, d.scale_y),
            ("offsetX", a.offset.dx, 0.0),
            ("offsetY", a.offset.dy, 0.0),
            ("opacity", a.opacity, d.opacity),
            ("strokeWidth", a.stroke_width, d.stroke_width),
            ("hitStrokeWidth", a.hit_stroke_width, d.hit_stroke_width),
        ] {
            if (v - dv).abs() > f32::EPSILON {
                attrs.push((k, Json::from(v)));
            }
        }
        if !a.visible {
            attrs.push(("visible", Json::Bool(false)));
        }
        if !a.listening {
            attrs.push(("listening", Json::Bool(false)));
        }
        if a.draggable {
            attrs.push(("draggable", Json::Bool(true)));
        }
        if let Some(c) = a.fill {
            attrs.push(("fill", color(c)));
        }
        if let Some(c) = a.stroke {
            attrs.push(("stroke", color(c)));
        }
        if !a.filters.is_empty() {
            let fs = a
                .filters
                .iter()
                .map(|f| match *f {
                    Filter::Blur(s) => Json::object([("Blur", Json::from(s))]),
                    Filter::Brighten(b) => Json::object([("Brighten", Json::from(b))]),
                    Filter::Grayscale => Json::from("Grayscale"),
                    Filter::Invert => Json::from("Invert"),
                    Filter::Sepia => Json::from("Sepia"),
                })
                .collect();
            attrs.push(("filters", Json::Array(fs)));
        }
        let class = match &n.shape {
            None if n.parent.is_none() => "Layer",
            None => "Group",
            Some(s) => {
                let pts = |v: &[Offset]| {
                    Json::numbers(v.iter().flat_map(|p| [f64::from(p.dx), f64::from(p.dy)]))
                };
                match s {
                    Shape::Rect {
                        width,
                        height,
                        corner,
                    } => {
                        attrs.push(("width", Json::from(*width)));
                        attrs.push(("height", Json::from(*height)));
                        attrs.push(("cornerRadius", Json::from(*corner)));
                    }
                    Shape::Circle { radius } => attrs.push(("radius", Json::from(*radius))),
                    Shape::Ellipse { rx, ry } => {
                        attrs.push(("radiusX", Json::from(*rx)));
                        attrs.push(("radiusY", Json::from(*ry)));
                    }
                    Shape::Line { points, closed } => {
                        attrs.push(("points", pts(points)));
                        attrs.push(("closed", Json::Bool(*closed)));
                    }
                    Shape::Path(p) => attrs.push(("data", Json::from(p.to_svg_data()))),
                    Shape::Star {
                        points,
                        inner,
                        outer,
                    } => {
                        attrs.push(("numPoints", Json::from(*points as usize)));
                        attrs.push(("innerRadius", Json::from(*inner)));
                        attrs.push(("outerRadius", Json::from(*outer)));
                    }
                    Shape::RegularPolygon { sides, radius } => {
                        attrs.push(("sides", Json::from(*sides as usize)));
                        attrs.push(("radius", Json::from(*radius)));
                    }
                    Shape::Arrow { points, head } => {
                        attrs.push(("points", pts(points)));
                        attrs.push(("pointerLength", Json::from(*head)));
                    }
                    Shape::Text { text, size } => {
                        attrs.push(("text", Json::from(text.as_str())));
                        attrs.push(("fontSize", Json::from(*size)));
                    }
                }
                s.kind_name()
            }
        };
        let mut fields = vec![
            ("attrs", Json::object(attrs)),
            ("className", Json::from(class)),
        ];
        if !n.children.is_empty() {
            fields.push((
                "children",
                Json::Array(n.children.iter().map(|c| self.node_json(*c)).collect()),
            ));
        }
        Json::object(fields)
    }

    /// Rebuild a stage from [`to_json`](Self::to_json) — Konva's
    /// `Node.create`.
    ///
    /// # Errors
    ///
    /// Unknown class names or malformed attributes.
    pub fn from_json(doc: &Json) -> Result<Self, String> {
        let attrs = doc.get("attrs").cloned().unwrap_or_default();
        let mut s = Self::new(
            attrs.get("width").and_then(Json::as_f32).unwrap_or(0.0),
            attrs.get("height").and_then(Json::as_f32).unwrap_or(0.0),
        );
        for layer in doc.get("children").and_then(Json::as_array).unwrap_or(&[]) {
            let id = s.add_layer();
            s.nodes[id.0].attrs = parse_attrs(layer.get("attrs").unwrap_or(&Json::Null))?;
            for c in layer
                .get("children")
                .and_then(Json::as_array)
                .unwrap_or(&[])
            {
                s.load_node(id, c)?;
            }
        }
        Ok(s)
    }

    fn load_node(&mut self, parent: NodeId, j: &Json) -> Result<(), String> {
        let a = j.get("attrs").cloned().unwrap_or_default();
        let attrs = parse_attrs(&a)?;
        let f = |k: &str| a.get(k).and_then(Json::as_f32).unwrap_or(0.0);
        let points = |k: &str| {
            a.get(k)
                .and_then(Json::as_f32_vec)
                .map(|v| {
                    v.as_chunks::<2>()
                        .0
                        .iter()
                        .map(|c| Offset::new(c[0], c[1]))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let shape = match j.get("className").and_then(Json::as_str).unwrap_or("Group") {
            "Group" => None,
            "Rect" => Some(Shape::Rect {
                width: f("width"),
                height: f("height"),
                corner: f("cornerRadius"),
            }),
            "Circle" => Some(Shape::Circle {
                radius: f("radius"),
            }),
            "Ellipse" => Some(Shape::Ellipse {
                rx: f("radiusX"),
                ry: f("radiusY"),
            }),
            "Line" => Some(Shape::Line {
                points: points("points"),
                closed: a.get("closed").and_then(Json::as_bool).unwrap_or(false),
            }),
            "Path" => Some(Shape::Path(
                vieww_foundation::parse_path_data(
                    a.get("data").and_then(Json::as_str).unwrap_or(""),
                )
                .map_err(|e| format!("{e:?}"))?,
            )),
            "Star" => Some(Shape::Star {
                points: f("numPoints") as u32,
                inner: f("innerRadius"),
                outer: f("outerRadius"),
            }),
            "RegularPolygon" => Some(Shape::RegularPolygon {
                sides: f("sides") as u32,
                radius: f("radius"),
            }),
            "Arrow" => Some(Shape::Arrow {
                points: points("points"),
                head: f("pointerLength"),
            }),
            "Text" => Some(Shape::Text {
                text: a
                    .get("text")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_owned(),
                size: f("fontSize"),
            }),
            other => return Err(format!("unknown className {other}")),
        };
        let id = self.add(parent, attrs, shape);
        for c in j.get("children").and_then(Json::as_array).unwrap_or(&[]) {
            self.load_node(id, c)?;
        }
        Ok(())
    }
}

fn parse_color(s: &str) -> Option<Color> {
    let h = s.strip_prefix('#')?;
    let p = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    match h.len() {
        6 => Some(Color::rgb(p(0)?, p(2)?, p(4)?)),
        8 => Some(Color::rgba(p(0)?, p(2)?, p(4)?, p(6)?)),
        _ => None,
    }
}

fn parse_attrs(a: &Json) -> Result<Attrs, String> {
    let d = Attrs::default();
    let f = |k: &str, dv: f32| a.get(k).and_then(Json::as_f32).unwrap_or(dv);
    let b = |k: &str, dv: bool| a.get(k).and_then(Json::as_bool).unwrap_or(dv);
    let mut filters = Vec::new();
    for x in a.get("filters").and_then(Json::as_array).unwrap_or(&[]) {
        filters.push(match (x.as_str(), x.get("Blur"), x.get("Brighten")) {
            (Some("Grayscale"), ..) => Filter::Grayscale,
            (Some("Invert"), ..) => Filter::Invert,
            (Some("Sepia"), ..) => Filter::Sepia,
            (_, Some(v), _) => Filter::Blur(v.as_f32().unwrap_or(0.0)),
            (_, _, Some(v)) => Filter::Brighten(v.as_f32().unwrap_or(0.0)),
            _ => return Err(format!("unknown filter {x}")),
        });
    }
    Ok(Attrs {
        id: a.get("id").and_then(Json::as_str).unwrap_or("").to_owned(),
        name: a
            .get("name")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_owned(),
        x: f("x", d.x),
        y: f("y", d.y),
        rotation: f("rotation", d.rotation),
        scale_x: f("scaleX", d.scale_x),
        scale_y: f("scaleY", d.scale_y),
        offset: Offset::new(f("offsetX", 0.0), f("offsetY", 0.0)),
        opacity: f("opacity", d.opacity),
        visible: b("visible", true),
        listening: b("listening", true),
        draggable: b("draggable", false),
        fill: a.get("fill").and_then(Json::as_str).and_then(parse_color),
        stroke: a.get("stroke").and_then(Json::as_str).and_then(parse_color),
        stroke_width: f("strokeWidth", d.stroke_width),
        hit_stroke_width: f("hitStrokeWidth", d.hit_stroke_width),
        filters,
    })
}

/// Which handle of a [`Transformer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Anchor {
    TopLeft,
    TopCenter,
    TopRight,
    MiddleLeft,
    MiddleRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
    Rotater,
}

impl Anchor {
    pub const ALL: [Self; 9] = [
        Self::TopLeft,
        Self::TopCenter,
        Self::TopRight,
        Self::MiddleLeft,
        Self::MiddleRight,
        Self::BottomLeft,
        Self::BottomCenter,
        Self::BottomRight,
        Self::Rotater,
    ];

    /// Position as fractions of the box (rotater sits above the top).
    const fn fraction(self) -> (f32, f32) {
        match self {
            Self::TopLeft => (0.0, 0.0),
            Self::TopCenter | Self::Rotater => (0.5, 0.0),
            Self::TopRight => (1.0, 0.0),
            Self::MiddleLeft => (0.0, 0.5),
            Self::MiddleRight => (1.0, 0.5),
            Self::BottomLeft => (0.0, 1.0),
            Self::BottomCenter => (0.5, 1.0),
            Self::BottomRight => (1.0, 1.0),
        }
    }
}

/// Resize and rotate handles around one node — Konva's `Transformer`.
#[derive(Debug, Clone, PartialEq)]
pub struct Transformer {
    pub node: NodeId,
    pub keep_ratio: bool,
    /// Rotation snaps (degrees) and how close counts.
    pub rotation_snaps: Vec<f32>,
    pub snap_tolerance: f32,
    pub anchor_size: f32,
    pub rotate_offset: f32,
    active: Option<(Anchor, Offset)>,
}

impl Transformer {
    #[must_use]
    pub const fn new(node: NodeId) -> Self {
        Self {
            node,
            keep_ratio: false,
            rotation_snaps: Vec::new(),
            snap_tolerance: 5.0,
            anchor_size: 10.0,
            rotate_offset: 30.0,
            active: None,
        }
    }

    /// Each anchor's stage position (the box follows the node's rotation).
    #[must_use]
    pub fn anchors(&self, stage: &Stage) -> Vec<(Anchor, Offset)> {
        let local = stage.local_rect(self.node);
        let t = stage.absolute_transform(self.node);
        let sy = stage.node(self.node).attrs.scale_y.abs().max(1e-6);
        Anchor::ALL
            .iter()
            .map(|&a| {
                let (fx, fy) = a.fraction();
                let mut p = Offset::new(
                    local.left + local.width() * fx,
                    local.top + local.height() * fy,
                );
                if a == Anchor::Rotater {
                    p.dy -= self.rotate_offset / sy;
                }
                (a, t.apply(p))
            })
            .collect()
    }

    /// The rotated outline (four stage points) to draw.
    #[must_use]
    pub fn outline(&self, stage: &Stage) -> [Offset; 4] {
        let local = stage.local_rect(self.node);
        let t = stage.absolute_transform(self.node);
        [
            t.apply(Offset::new(local.left, local.top)),
            t.apply(Offset::new(local.right, local.top)),
            t.apply(Offset::new(local.right, local.bottom)),
            t.apply(Offset::new(local.left, local.bottom)),
        ]
    }

    /// Which anchor is under `point`.
    #[must_use]
    pub fn anchor_at(&self, stage: &Stage, point: Offset) -> Option<Anchor> {
        self.anchors(stage)
            .into_iter()
            .find(|(_, p)| (*p - point).distance() <= self.anchor_size)
            .map(|(a, _)| a)
    }

    /// Start dragging whatever anchor is under `point`.
    pub fn begin(&mut self, stage: &Stage, point: Offset) -> Option<Anchor> {
        let a = self.anchor_at(stage, point)?;
        self.active = Some((a, point));
        Some(a)
    }

    pub fn end(&mut self) {
        self.active = None;
    }

    /// Continue the drag to `point`, updating the node's attributes.
    pub fn drag(&mut self, stage: &mut Stage, point: Offset) {
        let Some((anchor, _)) = self.active else {
            return;
        };
        let id = self.node;
        if anchor == Anchor::Rotater {
            let local = stage.local_rect(id);
            let t = stage.absolute_transform(id);
            let center = t.apply(Offset::new(
                local.left + local.width() / 2.0,
                local.top + local.height() / 2.0,
            ));
            let mut deg = (point.dy - center.dy)
                .atan2(point.dx - center.dx)
                .to_degrees()
                + 90.0;
            for s in &self.rotation_snaps {
                let diff = (deg - s + 180.0).rem_euclid(360.0) - 180.0;
                if diff.abs() <= self.snap_tolerance {
                    deg = *s;
                }
            }
            // Rotate about the centre: adjust x/y so the centre stays put.
            let old_t = t;
            stage.attrs_mut(id).rotation = deg;
            let new_t = stage.absolute_transform(id);
            let c_local = Offset::new(
                local.left + local.width() / 2.0,
                local.top + local.height() / 2.0,
            );
            let drift = new_t.apply(c_local) - old_t.apply(c_local);
            let at = stage.attrs_mut(id);
            at.x -= drift.dx;
            at.y -= drift.dy;
            return;
        }
        // Resize: work in the node's local frame (unscaled), keep the
        // opposite anchor fixed.
        let local = stage.local_rect(id);
        let t = stage.absolute_transform(id);
        let Some(inv) = t.invert() else { return };
        let p = inv.apply(point);
        let (fx, fy) = anchor.fraction();
        let fixed = Offset::new(
            local.left + local.width() * (1.0 - fx),
            local.top + local.height() * (1.0 - fy),
        );
        let fixed_stage = t.apply(fixed);
        let mut sx = if fx == 0.5 {
            1.0
        } else {
            (p.dx - fixed.dx) / (local.width() * (fx * 2.0 - 1.0)).max(1e-6)
        };
        let mut sy = if fy == 0.5 {
            1.0
        } else {
            (p.dy - fixed.dy) / (local.height() * (fy * 2.0 - 1.0)).max(1e-6)
        };
        if self.keep_ratio && fx != 0.5 && fy != 0.5 {
            let s = sx.max(sy);
            sx = s;
            sy = s;
        }
        sx = sx.max(0.05);
        sy = sy.max(0.05);
        let at = stage.attrs_mut(id);
        at.scale_x *= sx;
        at.scale_y *= sy;
        // Put the fixed anchor back where it was.
        let moved = stage.absolute_transform(id).apply(fixed) - fixed_stage;
        let at = stage.attrs_mut(id);
        at.x -= moved.dx;
        at.y -= moved.dy;
        self.active = Some((anchor, point));
    }

    /// The handles as a drawing: outline, anchor squares, rotater stem.
    #[must_use]
    pub fn draw(&self, stage: &Stage) -> Sketchbook {
        let mut book = Sketchbook::new();
        let blue = Color::rgb(0, 161, 255);
        let o = self.outline(stage);
        let mut p = Path::new();
        p.move_to(o[0])
            .line_to(o[1])
            .line_to(o[2])
            .line_to(o[3])
            .close();
        book.stroke(p, blue, 1.5);
        let anchors = self.anchors(stage);
        let top = anchors
            .iter()
            .find(|(a, _)| *a == Anchor::TopCenter)
            .map(|(_, p)| *p);
        let rot = anchors
            .iter()
            .find(|(a, _)| *a == Anchor::Rotater)
            .map(|(_, p)| *p);
        if let (Some(a), Some(b)) = (top, rot) {
            book.line(a, b, blue, 1.5);
        }
        let h = self.anchor_size / 2.0;
        for (a, pt) in anchors {
            if a == Anchor::Rotater {
                book.circle(pt, h, Color::WHITE);
                book.ring(pt, h, 1.5, blue);
            } else {
                let r = Rect::new(pt.dx - h, pt.dy - h, pt.dx + h, pt.dy + h);
                book.rect(r, Color::WHITE);
                book.stroke(Path::rect(r), blue, 1.5);
            }
        }
        book
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn filled(x: f32, y: f32, c: Color) -> Attrs {
        Attrs {
            x,
            y,
            fill: Some(c),
            ..Attrs::default()
        }
    }

    #[test]
    fn hit_testing_is_exact_and_front_to_back() {
        let mut s = Stage::new(400.0, 300.0);
        let l = s.add_layer();
        let back = s.add_shape(
            l,
            filled(0.0, 0.0, Color::RED),
            Shape::Rect {
                width: 200.0,
                height: 200.0,
                corner: 0.0,
            },
        );
        let circle = s.add_shape(
            l,
            filled(100.0, 100.0, Color::BLUE),
            Shape::Circle { radius: 50.0 },
        );
        assert_eq!(s.hit(Offset::new(100.0, 100.0)), Some(circle));
        // Inside the circle's bounding box but outside the curve: the rect.
        assert_eq!(s.hit(Offset::new(145.0, 145.0)), Some(back));
        assert_eq!(s.hit(Offset::new(300.0, 250.0)), None);
        s.attrs_mut(circle).listening = false;
        assert_eq!(s.hit(Offset::new(100.0, 100.0)), Some(back));
    }

    #[test]
    fn transforms_nest_through_groups_with_rotation_and_offset() {
        let mut s = Stage::new(400.0, 400.0);
        let l = s.add_layer();
        let g = s.add_group(
            l,
            Attrs {
                x: 200.0,
                y: 200.0,
                rotation: 90.0,
                ..Attrs::default()
            },
        );
        let r = s.add_shape(
            g,
            Attrs {
                offset: Offset::new(50.0, 10.0),
                ..filled(0.0, 0.0, Color::RED)
            },
            Shape::Rect {
                width: 100.0,
                height: 20.0,
                corner: 0.0,
            },
        );
        // Rotated 90°: the bar is now vertical through (200, 200).
        assert_eq!(s.hit(Offset::new(200.0, 240.0)), Some(r));
        assert_eq!(s.hit(Offset::new(240.0, 200.0)), None);
        let cr = s.client_rect(r);
        assert!((cr.width() - 20.0).abs() < 1e-3 && (cr.height() - 100.0).abs() < 1e-3);
    }

    #[test]
    fn open_lines_hit_on_their_stroke() {
        let mut s = Stage::new(200.0, 200.0);
        let l = s.add_layer();
        let line = s.add_shape(
            l,
            Attrs {
                stroke: Some(Color::BLACK),
                stroke_width: 2.0,
                hit_stroke_width: 12.0,
                ..Attrs::default()
            },
            Shape::Line {
                points: vec![Offset::new(10.0, 10.0), Offset::new(190.0, 10.0)],
                closed: false,
            },
        );
        assert_eq!(
            s.hit(Offset::new(100.0, 15.0)),
            Some(line),
            "within the hit stroke"
        );
        assert_eq!(s.hit(Offset::new(100.0, 30.0)), None);
    }

    #[test]
    fn events_bubble_and_can_be_cancelled() {
        let mut s = Stage::new(200.0, 200.0);
        let l = s.add_layer();
        let g = s.add_group(l, Attrs::default());
        let a = s.add_shape(
            g,
            filled(0.0, 0.0, Color::RED),
            Shape::Rect {
                width: 50.0,
                height: 50.0,
                corner: 0.0,
            },
        );
        let seen = Rc::new(RefCell::new(Vec::new()));
        for (id, tag) in [(a, "shape"), (g, "group"), (l, "layer")] {
            let seen = seen.clone();
            s.on(id, EventKind::Click, move |e| {
                seen.borrow_mut().push((tag, e.target, e.current))
            });
        }
        s.pointer_down(Offset::new(10.0, 10.0));
        s.pointer_up(Offset::new(11.0, 10.0));
        assert_eq!(
            seen.borrow().iter().map(|x| x.0).collect::<Vec<_>>(),
            ["shape", "group", "layer"]
        );
        assert!(seen.borrow().iter().all(|x| x.1 == a));
        seen.borrow_mut().clear();
        s.on(g, EventKind::Click, Event::cancel_bubble);
        s.pointer_down(Offset::new(10.0, 10.0));
        s.pointer_up(Offset::new(10.0, 10.0));
        assert_eq!(
            seen.borrow().iter().map(|x| x.0).collect::<Vec<_>>(),
            ["shape", "group"]
        );
    }

    #[test]
    fn draggable_nodes_follow_the_pointer_in_their_parents_frame() {
        let mut s = Stage::new(400.0, 400.0);
        let l = s.add_layer();
        let g = s.add_group(
            l,
            Attrs {
                scale_x: 2.0,
                scale_y: 2.0,
                ..Attrs::default()
            },
        );
        let r = s.add_shape(
            g,
            Attrs {
                draggable: true,
                ..filled(10.0, 10.0, Color::RED)
            },
            Shape::Rect {
                width: 20.0,
                height: 20.0,
                corner: 0.0,
            },
        );
        let moves = Rc::new(RefCell::new(0));
        let m = moves.clone();
        s.on(r, EventKind::DragMove, move |_| *m.borrow_mut() += 1);
        assert_eq!(s.pointer_down(Offset::new(30.0, 30.0)), Some(r));
        s.pointer_move(Offset::new(70.0, 50.0));
        s.pointer_up(Offset::new(70.0, 50.0));
        // 40 stage px in a 2× group is 20 local units.
        assert_eq!((s.node(r).attrs.x, s.node(r).attrs.y), (30.0, 20.0));
        assert_eq!(*moves.borrow(), 1);
    }

    #[test]
    fn hover_enter_and_leave() {
        let mut s = Stage::new(200.0, 200.0);
        let l = s.add_layer();
        let a = s.add_shape(
            l,
            filled(0.0, 0.0, Color::RED),
            Shape::Rect {
                width: 50.0,
                height: 50.0,
                corner: 0.0,
            },
        );
        s.on(a, EventKind::PointerEnter, |_| {});
        s.on(a, EventKind::PointerLeave, |_| {});
        s.pointer_move(Offset::new(10.0, 10.0));
        s.pointer_move(Offset::new(100.0, 100.0));
        let kinds: Vec<EventKind> = s.log.iter().map(|x| x.0).collect();
        assert_eq!(kinds, [EventKind::PointerEnter, EventKind::PointerLeave]);
    }

    #[test]
    fn transformer_resizes_from_the_opposite_corner_and_rotates_with_snaps() {
        let mut s = Stage::new(400.0, 400.0);
        let l = s.add_layer();
        let r = s.add_shape(
            l,
            filled(100.0, 100.0, Color::RED),
            Shape::Rect {
                width: 100.0,
                height: 50.0,
                corner: 0.0,
            },
        );
        let mut tr = Transformer::new(r);
        assert_eq!(
            tr.begin(&s, Offset::new(200.0, 150.0)),
            Some(Anchor::BottomRight)
        );
        tr.drag(&mut s, Offset::new(300.0, 250.0));
        tr.end();
        let cr = s.client_rect(r);
        assert!(
            (cr.left - 100.0).abs() < 1e-3 && (cr.top - 100.0).abs() < 1e-3,
            "top-left fixed: {cr:?}"
        );
        assert!((cr.width() - 200.0).abs() < 1e-3 && (cr.height() - 150.0).abs() < 1e-3);
        // Rotate by dragging the rotater to the right of the centre: 90°.
        tr.rotation_snaps = vec![0.0, 90.0, 180.0, 270.0];
        let rot_at = tr
            .anchors(&s)
            .into_iter()
            .find(|(a, _)| *a == Anchor::Rotater)
            .unwrap()
            .1;
        assert_eq!(tr.begin(&s, rot_at), Some(Anchor::Rotater));
        let centre = Offset::new(cr.left + cr.width() / 2.0, cr.top + cr.height() / 2.0);
        tr.drag(&mut s, Offset::new(centre.dx + 100.0, centre.dy + 3.0));
        assert!(
            (s.node(r).attrs.rotation - 90.0).abs() < 1e-3,
            "snapped: {}",
            s.node(r).attrs.rotation
        );
        let after = s.client_rect(r);
        let c2 = Offset::new(
            after.left + after.width() / 2.0,
            after.top + after.height() / 2.0,
        );
        assert!((c2 - centre).distance() < 1e-2, "rotated about its centre");
    }

    #[test]
    fn keep_ratio_scales_uniformly() {
        let mut s = Stage::new(400.0, 400.0);
        let l = s.add_layer();
        let r = s.add_shape(
            l,
            filled(0.0, 0.0, Color::RED),
            Shape::Rect {
                width: 100.0,
                height: 50.0,
                corner: 0.0,
            },
        );
        let mut tr = Transformer::new(r);
        tr.keep_ratio = true;
        tr.begin(&s, Offset::new(100.0, 50.0));
        tr.drag(&mut s, Offset::new(300.0, 60.0));
        let a = &s.node(r).attrs;
        assert!((a.scale_x - a.scale_y).abs() < 1e-5 && (a.scale_x - 3.0).abs() < 1e-3);
    }

    #[test]
    fn json_round_trips_the_stage() {
        let mut s = Stage::new(300.0, 200.0);
        let l = s.add_layer();
        let g = s.add_group(
            l,
            Attrs {
                id: "grp".into(),
                x: 5.0,
                rotation: 15.0,
                ..Attrs::default()
            },
        );
        s.add_shape(
            g,
            Attrs {
                name: "a b".into(),
                filters: vec![Filter::Blur(2.0), Filter::Grayscale],
                ..filled(1.0, 2.0, Color::rgb(10, 20, 30))
            },
            Shape::Star {
                points: 5,
                inner: 10.0,
                outer: 20.0,
            },
        );
        s.add_shape(
            l,
            Attrs {
                stroke: Some(Color::BLACK),
                ..Attrs::default()
            },
            Shape::Arrow {
                points: vec![Offset::ZERO, Offset::new(40.0, 5.0)],
                head: 8.0,
            },
        );
        s.add_shape(
            l,
            filled(0.0, 0.0, Color::BLUE),
            Shape::Text {
                text: "hi \"there\"".into(),
                size: 14.0,
            },
        );
        let text = s.to_json().pretty();
        let back = Stage::from_json(&Json::parse(&text).unwrap()).unwrap();
        assert_eq!(back.to_json(), s.to_json());
        assert_eq!(back.find("#grp").len(), 1);
        assert_eq!(back.find(".b").len(), 1);
        assert_eq!(back.find("Arrow").len(), 1);
    }

    #[test]
    fn z_order_and_marquee_and_destroy() {
        let mut s = Stage::new(300.0, 300.0);
        let l = s.add_layer();
        let a = s.add_shape(
            l,
            filled(0.0, 0.0, Color::RED),
            Shape::Rect {
                width: 50.0,
                height: 50.0,
                corner: 0.0,
            },
        );
        let b = s.add_shape(
            l,
            filled(10.0, 10.0, Color::BLUE),
            Shape::Rect {
                width: 50.0,
                height: 50.0,
                corner: 0.0,
            },
        );
        assert_eq!(s.hit(Offset::new(20.0, 20.0)), Some(b));
        s.move_to_top(a);
        assert_eq!(s.hit(Offset::new(20.0, 20.0)), Some(a));
        s.set_z_index(a, 0);
        assert_eq!(s.hit(Offset::new(20.0, 20.0)), Some(b));
        assert_eq!(s.nodes_in_rect(Rect::new(-1.0, -1.0, 55.0, 55.0)), vec![a]);
        s.destroy(b);
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn filters_change_colours_and_blur_becomes_a_layer() {
        assert_eq!(
            filter_color(Color::rgb(255, 0, 0), &[Filter::Grayscale]),
            Color::rgb(54, 54, 54)
        );
        assert_eq!(
            filter_color(Color::rgb(10, 20, 30), &[Filter::Invert]),
            Color::rgb(245, 235, 225)
        );
        let mut s = Stage::new(100.0, 100.0);
        let l = s.add_layer();
        s.add_shape(
            l,
            Attrs {
                filters: vec![Filter::Blur(3.0)],
                ..filled(0.0, 0.0, Color::RED)
            },
            Shape::Circle { radius: 10.0 },
        );
        let (book, _) = s.render();
        assert!(matches!(book.items()[0], Sketch::Layer { blur, .. } if blur == 3.0));
    }
}
