//! After Effects-style compositions (§2.15 L1/L2/L5): a timed stack of
//! layers rendered to a frame on the CPU.
//!
//! A [`Composition`] has a size, a frame rate, a duration and layers, bottom
//! first. Each [`Layer`] has:
//!
//! - a **source** — a solid, a still, a [`VideoSource`], a closure of time,
//!   or a nested composition (a *precomp*);
//! - an **in/out point** and a **start offset** (sliding footage in time),
//!   and an optional **time remap** curve (freeze, reverse, slow-mo);
//! - a **transform** — anchor, position, scale, rotation, opacity — each a
//!   [`Prop`] that can be static, keyframed (with AE's easy-ease) or an
//!   expression closure, and an optional **parent** whose transform it
//!   inherits (the AE parenting chain; opacity is *not* inherited, as in AE);
//! - a **blend mode**, an optional **track matte** (the layer above it, as
//!   in AE, by alpha or luma, straight or inverted), and **effects**
//!   (closures `Image → Image` applied in layer space before transform);
//! - or it is an **adjustment layer**, whose effects are applied to
//!   everything composited beneath it, inside its in/out span.
//!
//! The same [`Composition`] is also a [`VideoSource`], so a comp plays in
//! a [`VideoPlayer`](crate::VideoPlayer) and exports through any
//! [`FrameSink`](crate::export::FrameSink).

use std::fmt;
use std::sync::Arc;

use vieww_foundation::{Color, Image, Offset, Transform};

use crate::matte::MatteMode;
use crate::{Frame, VideoSource};

/// A property over time.
#[derive(Clone)]
pub enum Prop {
    Static(f32),
    /// `(time seconds, value)` keys, eased in and out (AE's F9).
    Keys(Vec<(f32, f32)>),
    /// Linear keys.
    Linear(Vec<(f32, f32)>),
    /// An expression: comp time → value.
    Expr(Arc<dyn Fn(f32) -> f32 + Send + Sync>),
}

impl fmt::Debug for Prop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Static(v) => write!(f, "Static({v})"),
            Self::Keys(k) => write!(f, "Keys({k:?})"),
            Self::Linear(k) => write!(f, "Linear({k:?})"),
            Self::Expr(_) => f.write_str("Expr(..)"),
        }
    }
}

impl From<f32> for Prop {
    fn from(v: f32) -> Self {
        Self::Static(v)
    }
}

impl Prop {
    /// Eased keys.
    #[must_use]
    pub fn keys(k: &[(f32, f32)]) -> Self {
        Self::Keys(k.to_vec())
    }

    /// An expression.
    pub fn expr(f: impl Fn(f32) -> f32 + Send + Sync + 'static) -> Self {
        Self::Expr(Arc::new(f))
    }

    /// Value at time `t`.
    #[must_use]
    pub fn at(&self, t: f32) -> f32 {
        let interp = |k: &[(f32, f32)], ease: bool| -> f32 {
            match k {
                [] => 0.0,
                [only] => only.1,
                _ => {
                    if t <= k[0].0 {
                        return k[0].1;
                    }
                    for w in k.windows(2) {
                        let (a, b) = (w[0], w[1]);
                        if t <= b.0 {
                            let mut u = ((t - a.0) / (b.0 - a.0).max(1e-6)).clamp(0.0, 1.0);
                            if ease {
                                u = u * u * (3.0 - 2.0 * u);
                            }
                            return a.1 + (b.1 - a.1) * u;
                        }
                    }
                    k[k.len() - 1].1
                }
            }
        };
        match self {
            Self::Static(v) => *v,
            Self::Keys(k) => interp(k, true),
            Self::Linear(k) => interp(k, false),
            Self::Expr(f) => f(t),
        }
    }
}

/// What a layer shows.
#[derive(Clone)]
pub enum Source {
    Solid {
        color: Color,
        width: u32,
        height: u32,
    },
    Still(Image),
    /// Footage; sampled at the layer's (remapped) local time.
    Footage(Arc<dyn VideoSource + Send + Sync>),
    /// Generated: local time → image.
    Generator(Arc<dyn Fn(f32) -> Image + Send + Sync>),
    /// A nested composition, rendered at the layer's local time.
    Precomp(Arc<Composition>),
    /// Nothing (a null object, for parenting; or an adjustment layer).
    Null,
}

impl fmt::Debug for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Solid {
                color,
                width,
                height,
            } => write!(f, "Solid({color:?}, {width}x{height})"),
            Self::Still(i) => write!(f, "Still({}x{})", i.width(), i.height()),
            Self::Footage(_) => f.write_str("Footage(..)"),
            Self::Generator(_) => f.write_str("Generator(..)"),
            Self::Precomp(c) => write!(f, "Precomp({:?})", c.name),
            Self::Null => f.write_str("Null"),
        }
    }
}

/// Per-pixel blend of a layer onto what is beneath it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Blend {
    #[default]
    Normal,
    Add,
    Multiply,
    Screen,
    Overlay,
    Difference,
    Darken,
    Lighten,
}

impl Blend {
    fn mix(self, s: f32, d: f32) -> f32 {
        match self {
            Self::Normal => s,
            Self::Add => (s + d).min(1.0),
            Self::Multiply => s * d,
            Self::Screen => 1.0 - (1.0 - s) * (1.0 - d),
            Self::Overlay => {
                if d < 0.5 {
                    2.0 * s * d
                } else {
                    1.0 - 2.0 * (1.0 - s) * (1.0 - d)
                }
            }
            Self::Difference => (s - d).abs(),
            Self::Darken => s.min(d),
            Self::Lighten => s.max(d),
        }
    }
}

/// An effect: an image-to-image function, given the comp time.
pub type Effect = Arc<dyn Fn(&Image, f32) -> Image + Send + Sync>;

/// The layer transform, AE's "Transform" group.
#[derive(Debug, Clone)]
pub struct LayerTransform {
    pub anchor_x: Prop,
    pub anchor_y: Prop,
    pub x: Prop,
    pub y: Prop,
    pub scale_x: Prop,
    pub scale_y: Prop,
    /// Degrees, clockwise (screen space).
    pub rotation: Prop,
    /// 0..1.
    pub opacity: Prop,
}

impl Default for LayerTransform {
    fn default() -> Self {
        Self {
            anchor_x: 0.0.into(),
            anchor_y: 0.0.into(),
            x: 0.0.into(),
            y: 0.0.into(),
            scale_x: 1.0.into(),
            scale_y: 1.0.into(),
            rotation: 0.0.into(),
            opacity: 1.0.into(),
        }
    }
}

impl LayerTransform {
    /// Layer-space → parent-space at time `t`.
    #[must_use]
    pub fn matrix(&self, t: f32) -> Transform {
        Transform::translate(Offset::new(-self.anchor_x.at(t), -self.anchor_y.at(t)))
            .then(Transform::scale(self.scale_x.at(t), self.scale_y.at(t)))
            .then(Transform::rotate(self.rotation.at(t).to_radians()))
            .then(Transform::translate(Offset::new(
                self.x.at(t),
                self.y.at(t),
            )))
    }
}

/// One layer of a [`Composition`].
#[derive(Clone)]
pub struct Layer {
    pub name: String,
    pub source: Source,
    /// Visible from `in_point` (comp seconds) …
    pub in_point: f32,
    /// … until `out_point`.
    pub out_point: f32,
    /// Comp time at which the source's time 0 plays.
    pub start: f32,
    /// Optional remap: comp-time → source-time (overrides `start`).
    pub time_remap: Option<Prop>,
    pub transform: LayerTransform,
    /// Index of the parent layer in the comp.
    pub parent: Option<usize>,
    pub blend: Blend,
    /// Use the layer directly above (index + 1) as this layer's matte; that
    /// matte layer is then not drawn itself, as in AE.
    pub track_matte: Option<MatteMode>,
    pub effects: Vec<Effect>,
    pub adjustment: bool,
    pub visible: bool,
    /// AE's 3D switch: when set and the comp has a [`CompCamera`], the
    /// layer is a plane in 3D, projected through the camera and lit by the
    /// comp's [`CompLight`]s.
    pub three_d: Option<Layer3D>,
}

/// A 3D layer's extra transform (AE's Z position, X/Y rotation and
/// Material Options). Coordinates follow AE: x right, y down, z into the
/// screen; the layer's 2D transform supplies x, y, anchor, scale and Z
/// rotation.
#[derive(Debug, Clone)]
pub struct Layer3D {
    pub z: Prop,
    /// Degrees.
    pub rotation_x: Prop,
    /// Degrees.
    pub rotation_y: Prop,
    pub accepts_lights: bool,
    /// Diffuse response, 0..1.
    pub diffuse: f32,
    /// Light the layer reflects regardless of lights.
    pub ambient: f32,
}

impl Default for Layer3D {
    fn default() -> Self {
        Self {
            z: 0.0.into(),
            rotation_x: 0.0.into(),
            rotation_y: 0.0.into(),
            accepts_lights: true,
            diffuse: 0.5,
            ambient: 1.0,
        }
    }
}

/// AE's camera layer: a two-node camera (position + point of interest) and
/// a zoom (the distance at which one comp pixel is one screen pixel).
#[derive(Debug, Clone)]
pub struct CompCamera {
    pub x: Prop,
    pub y: Prop,
    pub z: Prop,
    pub poi_x: Prop,
    pub poi_y: Prop,
    pub poi_z: Prop,
    pub zoom: Prop,
}

impl CompCamera {
    /// AE's default camera for a `w × h` comp: centred, looking down +z
    /// from `-zoom`, so a 3D layer at z = 0 renders exactly as 2D.
    #[must_use]
    pub fn default_for(w: u32, h: u32, zoom: f32) -> Self {
        #[allow(clippy::cast_precision_loss)]
        let (cx, cy) = (w as f32 * 0.5, h as f32 * 0.5);
        Self {
            x: cx.into(),
            y: cy.into(),
            z: (-zoom).into(),
            poi_x: cx.into(),
            poi_y: cy.into(),
            poi_z: 0.0.into(),
            zoom: zoom.into(),
        }
    }
}

/// AE's light layer kinds.
#[derive(Debug, Clone)]
pub enum CompLight {
    Ambient {
        intensity: f32,
    },
    /// From a point, falling off with distance past `falloff_start`.
    Point {
        x: Prop,
        y: Prop,
        z: Prop,
        intensity: f32,
        falloff_start: f32,
    },
    /// Parallel rays travelling along (dx, dy, dz).
    Parallel {
        dx: f32,
        dy: f32,
        dz: f32,
        intensity: f32,
    },
}

impl fmt::Debug for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Layer")
            .field("name", &self.name)
            .field("source", &self.source)
            .field("in_point", &self.in_point)
            .field("out_point", &self.out_point)
            .field("parent", &self.parent)
            .field("blend", &self.blend)
            .field("track_matte", &self.track_matte)
            .field("effects", &self.effects.len())
            .field("adjustment", &self.adjustment)
            .finish_non_exhaustive()
    }
}

impl Layer {
    /// A layer showing `source` for the whole comp.
    #[must_use]
    pub fn new(name: &str, source: Source) -> Self {
        Self {
            name: name.to_owned(),
            source,
            in_point: 0.0,
            out_point: f32::INFINITY,
            start: 0.0,
            time_remap: None,
            transform: LayerTransform::default(),
            parent: None,
            blend: Blend::Normal,
            track_matte: None,
            effects: Vec::new(),
            adjustment: false,
            visible: true,
            three_d: None,
        }
    }

    /// Make this a 3D layer.
    #[must_use]
    pub fn three_d(mut self, l: Layer3D) -> Self {
        self.three_d = Some(l);
        self
    }

    /// An adjustment layer applying `effects` to everything beneath.
    #[must_use]
    pub fn adjustment(name: &str, effects: Vec<Effect>) -> Self {
        Self {
            adjustment: true,
            effects,
            ..Self::new(name, Source::Null)
        }
    }

    #[must_use]
    pub fn span(mut self, in_point: f32, out_point: f32) -> Self {
        self.in_point = in_point;
        self.out_point = out_point;
        self
    }

    #[must_use]
    pub fn starting(mut self, start: f32) -> Self {
        self.start = start;
        self
    }

    #[must_use]
    pub fn remap(mut self, remap: Prop) -> Self {
        self.time_remap = Some(remap);
        self
    }

    #[must_use]
    pub fn at(mut self, x: impl Into<Prop>, y: impl Into<Prop>) -> Self {
        self.transform.x = x.into();
        self.transform.y = y.into();
        self
    }

    #[must_use]
    pub fn anchor(mut self, x: f32, y: f32) -> Self {
        self.transform.anchor_x = x.into();
        self.transform.anchor_y = y.into();
        self
    }

    #[must_use]
    pub fn scale(mut self, s: impl Into<Prop>) -> Self {
        let s = s.into();
        self.transform.scale_x = s.clone();
        self.transform.scale_y = s;
        self
    }

    #[must_use]
    pub fn rotation(mut self, r: impl Into<Prop>) -> Self {
        self.transform.rotation = r.into();
        self
    }

    #[must_use]
    pub fn opacity(mut self, o: impl Into<Prop>) -> Self {
        self.transform.opacity = o.into();
        self
    }

    #[must_use]
    pub const fn parent(mut self, index: usize) -> Self {
        self.parent = Some(index);
        self
    }

    #[must_use]
    pub const fn blend(mut self, b: Blend) -> Self {
        self.blend = b;
        self
    }

    #[must_use]
    pub const fn matte(mut self, m: MatteMode) -> Self {
        self.track_matte = Some(m);
        self
    }

    #[must_use]
    pub fn effect(mut self, e: impl Fn(&Image, f32) -> Image + Send + Sync + 'static) -> Self {
        self.effects.push(Arc::new(e));
        self
    }

    /// Is the layer live at comp time `t`?
    #[must_use]
    pub fn active(&self, t: f32) -> bool {
        self.visible && t >= self.in_point && t < self.out_point
    }

    /// Source time for comp time `t`.
    #[must_use]
    pub fn local_time(&self, t: f32) -> f32 {
        self.time_remap.as_ref().map_or(t - self.start, |r| r.at(t))
    }
}

/// A composition.
#[derive(Debug, Clone)]
pub struct Composition {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// Seconds.
    pub duration: f32,
    pub background: Color,
    /// Bottom first.
    pub layers: Vec<Layer>,
    /// The active camera; without one, 3D layers render as 2D.
    pub camera: Option<CompCamera>,
    pub lights: Vec<CompLight>,
}

impl Composition {
    #[must_use]
    pub fn new(name: &str, width: u32, height: u32, fps: u32, duration: f32) -> Self {
        Self {
            name: name.to_owned(),
            width,
            height,
            fps,
            duration,
            background: Color::rgba(0, 0, 0, 0),
            layers: Vec::new(),
            camera: None,
            lights: Vec::new(),
        }
    }

    /// Project a comp-space 3D point through the camera at `t`:
    /// screen position and depth along the view axis.
    #[must_use]
    pub fn project(&self, p: [f32; 3], t: f32) -> Option<(Offset, f32)> {
        let cam = self.camera.as_ref()?;
        let eye = [cam.x.at(t), cam.y.at(t), cam.z.at(t)];
        let poi = [cam.poi_x.at(t), cam.poi_y.at(t), cam.poi_z.at(t)];
        let f = norm3(sub3(poi, eye));
        // AE's y is down, so "up" is −y.
        let r = norm3(cross3(f, [0.0, -1.0, 0.0]));
        let r = if dot3(r, r) < 1e-12 {
            [1.0, 0.0, 0.0]
        } else {
            r
        };
        let u = cross3(r, f);
        let d = sub3(p, eye);
        let (x, y, z) = (dot3(d, r), dot3(d, u), dot3(d, f));
        if z <= 1e-3 {
            return None;
        }
        let zoom = cam.zoom.at(t);
        #[allow(clippy::cast_precision_loss)]
        let (cx, cy) = (self.width as f32 * 0.5, self.height as f32 * 0.5);
        Some((Offset::new(cx + x * zoom / z, cy - y * zoom / z), z))
    }

    /// Layer pixel `(u, v)` → comp 3D point at `t` (a 3D layer's plane).
    fn layer_point_3d(&self, i: usize, u: f32, v: f32, t: f32) -> [f32; 3] {
        let layer = &self.layers[i];
        let l3 = layer.three_d.clone().unwrap_or_default();
        let tr = &layer.transform;
        let (ax, ay) = (tr.anchor_x.at(t), tr.anchor_y.at(t));
        let mut p = [
            (u - ax) * tr.scale_x.at(t),
            (v - ay) * tr.scale_y.at(t),
            0.0,
        ];
        p = rot_z(p, tr.rotation.at(t).to_radians());
        p = rot_y(p, l3.rotation_y.at(t).to_radians());
        p = rot_x(p, l3.rotation_x.at(t).to_radians());
        let mut q = [p[0] + tr.x.at(t), p[1] + tr.y.at(t), p[2] + l3.z.at(t)];
        // Parents contribute their 2D offset (enough for 3D null rigs).
        let mut par = layer.parent;
        let mut guard = 0;
        while let Some(pi) = par {
            let pl = &self.layers[pi];
            q[0] += pl.transform.x.at(t) - pl.transform.anchor_x.at(t);
            q[1] += pl.transform.y.at(t) - pl.transform.anchor_y.at(t);
            if let Some(p3) = &pl.three_d {
                q[2] += p3.z.at(t);
            }
            par = pl.parent;
            guard += 1;
            if guard > self.layers.len() {
                break;
            }
        }
        q
    }

    /// Light arriving at a 3D layer (multiplier on its colour).
    fn light_factor(&self, i: usize, t: f32, w: f32, h: f32) -> f32 {
        let Some(l3) = &self.layers[i].three_d else {
            return 1.0;
        };
        if !l3.accepts_lights || self.lights.is_empty() {
            return 1.0;
        }
        let c = self.layer_point_3d(i, w * 0.5, h * 0.5, t);
        let a = self.layer_point_3d(i, 0.0, 0.0, t);
        let b = self.layer_point_3d(i, w, 0.0, t);
        let d = self.layer_point_3d(i, 0.0, h, t);
        // Facing the camera is −z in AE; the normal is two-sided.
        let mut n = norm3(cross3(sub3(b, a), sub3(d, a)));
        if let Some(cam) = &self.camera {
            let eye = [cam.x.at(t), cam.y.at(t), cam.z.at(t)];
            if dot3(n, sub3(eye, c)) < 0.0 {
                n = [-n[0], -n[1], -n[2]];
            }
        }
        let mut total = 0.0;
        for l in &self.lights {
            total += match l {
                CompLight::Ambient { intensity } => intensity * l3.ambient,
                CompLight::Parallel {
                    dx,
                    dy,
                    dz,
                    intensity,
                } => {
                    let dir = norm3([-dx, -dy, -dz]);
                    intensity * l3.diffuse * dot3(n, dir).max(0.0)
                }
                CompLight::Point {
                    x,
                    y,
                    z,
                    intensity,
                    falloff_start,
                } => {
                    let lp = [x.at(t), y.at(t), z.at(t)];
                    let v = sub3(lp, c);
                    let dist = dot3(v, v).sqrt();
                    let fall = if dist > *falloff_start {
                        (falloff_start / dist).powi(2)
                    } else {
                        1.0
                    };
                    intensity * l3.diffuse * dot3(n, norm3(v)).max(0.0) * fall
                }
            };
        }
        total
    }

    #[must_use]
    pub const fn background(mut self, c: Color) -> Self {
        self.background = c;
        self
    }

    /// Add a layer on top; returns its index (for parenting).
    pub fn push(&mut self, layer: Layer) -> usize {
        self.layers.push(layer);
        self.layers.len() - 1
    }

    /// The full layer → comp matrix, walking the parent chain.
    #[must_use]
    pub fn world_matrix(&self, index: usize, t: f32) -> Transform {
        let mut m = self.layers[index].transform.matrix(t);
        let mut p = self.layers[index].parent;
        let mut guard = 0;
        while let Some(i) = p {
            m = m.then(self.layers[i].transform.matrix(t));
            p = self.layers[i].parent;
            guard += 1;
            if guard > self.layers.len() {
                break;
            }
        }
        m
    }

    fn source_image(&self, layer: &Layer, t: f32) -> Option<Image> {
        let lt = layer.local_time(t);
        match &layer.source {
            Source::Solid {
                color,
                width,
                height,
            } => {
                let px = [color.r, color.g, color.b, color.a];
                Some(Image::from_rgba8(
                    px.repeat((width * height) as usize),
                    *width,
                    *height,
                ))
            }
            Source::Still(i) => Some(i.clone()),
            Source::Footage(v) => {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let idx = (lt.max(0.0) * v.frame_rate() as f32).floor() as usize;
                v.frame_at(idx.min(v.frame_count().saturating_sub(1)))
            }
            Source::Generator(g) => Some(g(lt)),
            Source::Precomp(c) => Some(c.render(lt)),
            Source::Null => None,
        }
    }

    /// Render layer `i` (effects applied, transformed into comp space, with
    /// opacity folded into alpha).
    fn layer_pixels(&self, i: usize, t: f32) -> Option<Vec<f32>> {
        let layer = &self.layers[i];
        let mut img = self.source_image(layer, t)?;
        for e in &layer.effects {
            img = e(&img, t);
        }
        let (sw, sh) = (i64::from(img.width()), i64::from(img.height()));
        #[allow(clippy::cast_precision_loss)]
        let (swf, shf) = (sw as f32, sh as f32);
        // 2D layers invert an affine; 3D layers under a camera invert the
        // plane's homography (exact for a projected plane).
        let inv: Box<dyn Fn(Offset) -> Option<Offset>> =
            if layer.three_d.is_some() && self.camera.is_some() {
                let corners = [(0.0, 0.0), (swf, 0.0), (swf, shf), (0.0, shf)];
                let mut src = Vec::new();
                let mut dst = Vec::new();
                for (u, v) in corners {
                    let (p, _) = self.project(self.layer_point_3d(i, u, v, t), t)?;
                    src.push(Offset::new(u, v));
                    dst.push(p);
                }
                let hinv = crate::solve::homography(&src, &dst)?.inverse()?;
                Box::new(move |p| Some(hinv.apply(p)))
            } else {
                let m = self.world_matrix(i, t).invert()?;
                Box::new(move |p| Some(m.apply(p)))
            };
        let light = self.light_factor(i, t, swf, shf);
        let op = layer.transform.opacity.at(t).clamp(0.0, 1.0);
        let (w, h) = (self.width as usize, self.height as usize);
        let src = img.pixels();
        let mut out = vec![0.0f32; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                #[allow(clippy::cast_precision_loss)]
                let Some(p) = inv(Offset::new(x as f32 + 0.5, y as f32 + 0.5)) else {
                    continue;
                };
                let (fx, fy) = (p.dx - 0.5, p.dy - 0.5);
                let (x0, y0) = (fx.floor(), fy.floor());
                let (tx, ty) = (fx - x0, fy - y0);
                #[allow(clippy::cast_possible_truncation)]
                let (x0, y0) = (x0 as i64, y0 as i64);
                if x0 < -1 || y0 < -1 || x0 >= sw || y0 >= sh {
                    continue;
                }
                // Premultiplied bilinear.
                let mut acc = [0.0f32; 4];
                for (dx, dy, wt) in [
                    (0, 0, (1.0 - tx) * (1.0 - ty)),
                    (1, 0, tx * (1.0 - ty)),
                    (0, 1, (1.0 - tx) * ty),
                    (1, 1, tx * ty),
                ] {
                    let (sx, sy) = (x0 + dx, y0 + dy);
                    if sx < 0 || sy < 0 || sx >= sw || sy >= sh {
                        continue;
                    }
                    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                    let k = ((sy * sw + sx) * 4) as usize;
                    let a = f32::from(src[k + 3]) / 255.0 * wt;
                    acc[0] += f32::from(src[k]) / 255.0 * a;
                    acc[1] += f32::from(src[k + 1]) / 255.0 * a;
                    acc[2] += f32::from(src[k + 2]) / 255.0 * a;
                    acc[3] += a;
                }
                let o = (y * w + x) * 4;
                for c in 0..3 {
                    out[o + c] = acc[c] * op * light;
                }
                out[o + 3] = acc[3] * op;
            }
        }
        Some(out)
    }

    /// Render the comp at time `t` (seconds).
    #[must_use]
    pub fn render(&self, t: f32) -> Image {
        let (w, h) = (self.width as usize, self.height as usize);
        let bg = self.background;
        let ba = f32::from(bg.a) / 255.0;
        let mut acc: Vec<f32> = [
            f32::from(bg.r) / 255.0 * ba,
            f32::from(bg.g) / 255.0 * ba,
            f32::from(bg.b) / 255.0 * ba,
            ba,
        ]
        .repeat(w * h);
        // Layers used as a matte by the layer below them are not drawn.
        let is_matte = |i: usize| i > 0 && self.layers[i - 1].track_matte.is_some();
        // AE's 3D sort: within each contiguous run of 3D layers, farthest
        // first (by the layer centre's depth from the camera).
        let mut order: Vec<usize> = (0..self.layers.len()).collect();
        if self.camera.is_some() {
            let depth = |i: usize| {
                let l = &self.layers[i];
                let (w, h) = self.source_image(l, t).map_or((0.0, 0.0), |im| {
                    #[allow(clippy::cast_precision_loss)]
                    (im.width() as f32, im.height() as f32)
                });
                self.project(self.layer_point_3d(i, w * 0.5, h * 0.5, t), t)
                    .map_or(f32::INFINITY, |(_, z)| z)
            };
            let mut k = 0;
            while k < order.len() {
                if self.layers[k].three_d.is_none() {
                    k += 1;
                    continue;
                }
                let start = k;
                while k < order.len() && self.layers[k].three_d.is_some() {
                    k += 1;
                }
                let mut run: Vec<(usize, f32)> = (start..k).map(|i| (i, depth(i))).collect();
                run.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
                for (j, (i, _)) in run.into_iter().enumerate() {
                    order[start + j] = i;
                }
            }
        }
        for i in order {
            let layer = &self.layers[i];
            if !layer.active(t) || is_matte(i) {
                continue;
            }
            if layer.adjustment {
                let img = to_image(&acc, self.width, self.height);
                let mut fx = img.clone();
                for e in &layer.effects {
                    fx = e(&fx, t);
                }
                let op = layer.transform.opacity.at(t).clamp(0.0, 1.0);
                let f = from_image(&fx);
                for (a, b) in acc.iter_mut().zip(f) {
                    *a += (b - *a) * op;
                }
                continue;
            }
            let Some(mut px) = self.layer_pixels(i, t) else {
                continue;
            };
            if let Some(mode) = layer.track_matte {
                let matte = self
                    .layers
                    .get(i + 1)
                    .filter(|m| m.active(t))
                    .and_then(|_| self.layer_pixels(i + 1, t));
                for (k, p) in px.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    let m = matte.as_ref().map_or([0.0; 4], |m| {
                        [m[k * 4], m[k * 4 + 1], m[k * 4 + 2], m[k * 4 + 3]]
                    });
                    let luma = 0.2126 * m[0] + 0.7152 * m[1] + 0.0722 * m[2];
                    let f = match mode {
                        MatteMode::Alpha => m[3],
                        MatteMode::AlphaInverted => 1.0 - m[3],
                        MatteMode::Luma => luma,
                        MatteMode::LumaInverted => 1.0 - luma,
                    };
                    for c in p.iter_mut() {
                        *c *= f;
                    }
                }
            }
            for (d, s) in acc
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(px.as_chunks::<4>().0)
            {
                let sa = s[3];
                if sa <= 0.0 {
                    continue;
                }
                let da = d[3];
                for c in 0..3 {
                    // Separable blend (W3C compositing): straight colours in
                    // the blend function, premultiplied accumulation.
                    let sc = s[c] / sa;
                    let dc = if da > 0.0 { d[c] / da } else { 0.0 };
                    let mixed = layer.blend.mix(sc, dc);
                    d[c] = s[c] * (1.0 - da) + d[c] * (1.0 - sa) + sa * da * mixed;
                }
                d[3] = sa + da * (1.0 - sa);
            }
        }
        to_image(&acc, self.width, self.height)
    }

    /// Frame count at the comp rate.
    #[must_use]
    pub fn frames(&self) -> usize {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = (self.duration * self.fps as f32).round() as usize;
        n.max(1)
    }
}

fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm3(a: [f32; 3]) -> [f32; 3] {
    let l = dot3(a, a).sqrt();
    if l < 1e-12 {
        a
    } else {
        [a[0] / l, a[1] / l, a[2] / l]
    }
}
fn rot_x(p: [f32; 3], a: f32) -> [f32; 3] {
    let (s, c) = a.sin_cos();
    [p[0], p[1] * c - p[2] * s, p[1] * s + p[2] * c]
}
fn rot_y(p: [f32; 3], a: f32) -> [f32; 3] {
    let (s, c) = a.sin_cos();
    [p[0] * c + p[2] * s, p[1], -p[0] * s + p[2] * c]
}
fn rot_z(p: [f32; 3], a: f32) -> [f32; 3] {
    let (s, c) = a.sin_cos();
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c, p[2]]
}

fn to_image(acc: &[f32], w: u32, h: u32) -> Image {
    let mut out = Vec::with_capacity(acc.len());
    for p in acc.as_chunks::<4>().0 {
        let a = p[3].clamp(0.0, 1.0);
        for &c in &p[..3] {
            let v = if a > 0.0 { c / a } else { 0.0 };
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            out.push((v * 255.0).round().clamp(0.0, 255.0) as u8);
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        out.push((a * 255.0).round() as u8);
    }
    Image::from_rgba8(out, w, h)
}

fn from_image(img: &Image) -> Vec<f32> {
    img.pixels()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| {
            let a = f32::from(p[3]) / 255.0;
            [
                f32::from(p[0]) / 255.0 * a,
                f32::from(p[1]) / 255.0 * a,
                f32::from(p[2]) / 255.0 * a,
                a,
            ]
        })
        .collect()
}

impl VideoSource for Composition {
    fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn frame_rate(&self) -> u32 {
        self.fps
    }

    fn frame_count(&self) -> usize {
        self.frames()
    }

    fn frame_at(&self, index: usize) -> Option<Frame> {
        #[allow(clippy::cast_precision_loss)]
        (index < self.frames()).then(|| self.render(index as f32 / self.fps as f32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(img: &Image, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * img.width() + x) * 4) as usize;
        let p = img.pixels();
        [p[i], p[i + 1], p[i + 2], p[i + 3]]
    }

    fn solid(c: Color, w: u32, h: u32) -> Source {
        Source::Solid {
            color: c,
            width: w,
            height: h,
        }
    }

    #[test]
    fn eased_keys_and_expressions() {
        let p = Prop::keys(&[(0.0, 0.0), (1.0, 10.0)]);
        assert_eq!(p.at(-1.0), 0.0);
        assert_eq!(p.at(0.5), 5.0);
        assert!(p.at(0.25) < 2.5, "eased: slow out of the first key");
        assert_eq!(p.at(2.0), 10.0);
        assert_eq!(Prop::expr(|t| t * 2.0).at(3.0), 6.0);
    }

    #[test]
    fn in_out_points_and_position() {
        let mut c = Composition::new("c", 20, 10, 10, 2.0).background(Color::rgb(0, 0, 0));
        c.push(
            Layer::new("red", solid(Color::rgb(255, 0, 0), 4, 4))
                .at(10.0, 2.0)
                .span(0.5, 1.5),
        );
        assert_eq!(px(&c.render(0.0), 11, 3), [0, 0, 0, 255]);
        assert_eq!(px(&c.render(1.0), 11, 3), [255, 0, 0, 255]);
        assert_eq!(px(&c.render(1.0), 9, 3), [0, 0, 0, 255]);
        assert_eq!(px(&c.render(1.6), 11, 3), [0, 0, 0, 255]);
    }

    #[test]
    fn parenting_inherits_transform() {
        let mut c = Composition::new("c", 40, 40, 10, 1.0);
        let null = c.push(Layer::new("null", Source::Null).at(20.0, 20.0).scale(2.0));
        c.push(
            Layer::new("child", solid(Color::rgb(0, 255, 0), 2, 2))
                .at(5.0, 0.0)
                .parent(null),
        );
        // child's (0..2) → ×2 → +(10,0)… then +(20,20): covers x 30..34, y 20..24.
        let img = c.render(0.0);
        assert_eq!(px(&img, 31, 21)[1], 255);
        assert_eq!(px(&img, 25, 21)[3], 0);
    }

    #[test]
    fn blend_modes() {
        let mut c = Composition::new("c", 2, 2, 10, 1.0).background(Color::rgb(128, 128, 128));
        c.push(Layer::new("m", solid(Color::rgb(128, 255, 0), 2, 2)).blend(Blend::Multiply));
        let p = px(&c.render(0.0), 0, 0);
        assert_eq!(p[1], 128);
        assert!((i32::from(p[0]) - 64).abs() <= 1);
        assert_eq!(p[2], 0);
        let mut s = Composition::new("s", 2, 2, 10, 1.0).background(Color::rgb(128, 128, 128));
        s.push(Layer::new("s", solid(Color::rgb(128, 0, 0), 2, 2)).blend(Blend::Screen));
        assert!((i32::from(px(&s.render(0.0), 0, 0)[0]) - 192).abs() <= 1);
    }

    #[test]
    fn track_matte_cuts_and_is_hidden() {
        let mut c = Composition::new("c", 10, 10, 10, 1.0).background(Color::rgb(0, 0, 0));
        c.push(Layer::new("fill", solid(Color::rgb(255, 0, 0), 10, 10)).matte(MatteMode::Alpha));
        c.push(Layer::new("shape", solid(Color::rgb(0, 0, 255), 4, 4)).at(3.0, 3.0));
        let img = c.render(0.0);
        assert_eq!(
            px(&img, 4, 4),
            [255, 0, 0, 255],
            "red inside the matte, matte itself invisible"
        );
        assert_eq!(px(&img, 0, 0), [0, 0, 0, 255]);
    }

    #[test]
    fn adjustment_layer_affects_only_beneath() {
        let invert = |img: &Image, _t: f32| {
            let p: Vec<u8> = img
                .pixels()
                .chunks(4)
                .flat_map(|p| [255 - p[0], 255 - p[1], 255 - p[2], p[3]])
                .collect();
            Image::from_rgba8(p, img.width(), img.height())
        };
        let mut c = Composition::new("c", 4, 1, 10, 1.0).background(Color::rgb(0, 0, 0));
        c.push(Layer::adjustment("inv", vec![Arc::new(invert)]));
        c.push(Layer::new("top", solid(Color::rgb(10, 10, 10), 2, 1)));
        let img = c.render(0.0);
        assert_eq!(px(&img, 0, 0)[0], 10, "above the adjustment: untouched");
        assert_eq!(px(&img, 3, 0)[0], 255, "beneath: inverted");
    }

    #[test]
    fn time_remap_and_precomp() {
        let gen = Source::Generator(Arc::new(|t: f32| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let v = (t * 100.0).clamp(0.0, 255.0) as u8;
            Image::from_rgba8(vec![v, 0, 0, 255], 1, 1)
        }));
        let mut inner = Composition::new("inner", 1, 1, 10, 3.0);
        inner.push(Layer::new("g", gen.clone()));
        let mut c = Composition::new("c", 3, 1, 10, 3.0);
        c.push(Layer::new("frozen", gen.clone()).remap(1.0.into()));
        c.push(
            Layer::new("reversed", gen)
                .at(1.0, 0.0)
                .remap(Prop::Linear(vec![(0.0, 2.0), (2.0, 0.0)])),
        );
        c.push(
            Layer::new("pre", Source::Precomp(Arc::new(inner)))
                .at(2.0, 0.0)
                .starting(0.5),
        );
        let img = c.render(1.5);
        assert_eq!(px(&img, 0, 0)[0], 100, "freeze frame at source 1s");
        assert_eq!(px(&img, 1, 0)[0], 50, "reversed: comp 1.5 → source 0.5");
        assert_eq!(px(&img, 2, 0)[0], 100, "precomp slid by 0.5s");
        assert_eq!(c.frame_count(), 30);
    }

    #[test]
    fn a_3d_layer_at_z_zero_under_the_default_camera_renders_as_2d() {
        let solid = Source::Solid {
            color: Color::rgb(200, 40, 40),
            width: 20,
            height: 10,
        };
        let mut flat = Composition::new("flat", 64, 48, 30, 1.0);
        flat.push(
            Layer::new("s", solid.clone())
                .at(32.0, 24.0)
                .anchor(10.0, 5.0),
        );
        let mut deep = flat.clone();
        deep.layers[0].three_d = Some(Layer3D::default());
        deep.camera = Some(CompCamera::default_for(64, 48, 100.0));
        let (a, b) = (flat.render(0.0), deep.render(0.0));
        let diff = a
            .pixels()
            .iter()
            .zip(b.pixels())
            .filter(|(x, y)| x.abs_diff(**y) > 3)
            .count();
        assert!(diff < 20, "{diff}");
    }

    #[test]
    fn farther_layers_shrink_and_sort_behind_nearer_ones() {
        let red = Source::Solid {
            color: Color::rgb(255, 0, 0),
            width: 20,
            height: 20,
        };
        let blue = Source::Solid {
            color: Color::rgb(0, 0, 255),
            width: 20,
            height: 20,
        };
        let mut c = Composition::new("depth", 80, 60, 30, 1.0);
        c.camera = Some(CompCamera::default_for(80, 60, 80.0));
        // Pushed in the "wrong" order: the near red layer first.
        c.push(
            Layer::new("near", red)
                .at(40.0, 30.0)
                .anchor(10.0, 10.0)
                .three_d(Layer3D::default()),
        );
        c.push(
            Layer::new("far", blue)
                .at(40.0, 30.0)
                .anchor(10.0, 10.0)
                .three_d(Layer3D {
                    z: 80.0.into(),
                    ..Layer3D::default()
                }),
        );
        let img = c.render(0.0);
        let px = |x: u32, y: u32| {
            let k = ((y * 80 + x) * 4) as usize;
            [img.pixels()[k], img.pixels()[k + 1], img.pixels()[k + 2]]
        };
        assert!(px(40, 30)[0] > 200, "near layer must cover the far one");
        // The far layer is half size: no blue beyond 5 px from the centre.
        assert!(px(40, 30 + 12)[2] < 10);
    }

    #[test]
    fn a_layer_turned_away_from_a_parallel_light_darkens() {
        let white = Source::Solid {
            color: Color::rgb(255, 255, 255),
            width: 20,
            height: 20,
        };
        let mut c = Composition::new("lit", 60, 60, 30, 1.0);
        c.camera = Some(CompCamera::default_for(60, 60, 100.0));
        c.lights.push(CompLight::Parallel {
            dx: 0.0,
            dy: 0.0,
            dz: 1.0,
            intensity: 1.0,
        });
        c.push(
            Layer::new("w", white)
                .at(30.0, 30.0)
                .anchor(10.0, 10.0)
                .three_d(Layer3D {
                    diffuse: 1.0,
                    ambient: 0.0,
                    ..Layer3D::default()
                }),
        );
        let facing = c.render(0.0).pixels()[(30 * 60 + 30) * 4];
        c.layers[0].three_d = Some(Layer3D {
            rotation_y: 60.0.into(),
            diffuse: 1.0,
            ambient: 0.0,
            ..Layer3D::default()
        });
        let turned = c.render(0.0).pixels()[(30 * 60 + 30) * 4];
        assert!(facing > 240, "{facing}");
        assert!(turned < facing && turned > 100, "{turned}");
    }
}
