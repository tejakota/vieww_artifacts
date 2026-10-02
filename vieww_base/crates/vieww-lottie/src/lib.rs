//! Lottie — the Bodymovin JSON format After Effects exports, played by
//! vieww's own vector renderer.
//!
//! # Where the document puts it
//!
//! "**Lottie** works in Flutter too, but only for non-interactive playback of
//! designer-authored animations"; the Cross-Platform Interactive Runtime row
//! pairs it with Rive ("Lottie only plays back fixed After Effects
//! exports"). A framework that cannot play the files designers already hand
//! over leaves every motion spec to be re-implemented by hand; this crate
//! plays them.
//!
//! # What is supported
//!
//! | Lottie | support |
//! |---|---|
//! | composition `w`, `h`, `fr`, `ip`, `op` | yes |
//! | layers: shape (4), solid (1), null (3), precomp (0) with `refId` assets | yes; in/out points, `st` start offset, `parent` chains |
//! | layer transform `ks`: anchor, position, scale %, rotation °, opacity % | yes, animated |
//! | shapes: `gr` groups with `tr`, `rc` rect (+roundness), `el` ellipse, `sh` bezier path, `sr` star/polygon | yes |
//! | styles: `fl` fill, `st` stroke (width, opacity) | yes (solid colours) |
//! | modifiers: `tm` trim paths (start, end, offset) | yes, through `Path::trim_offset` |
//! | keyframes: `{a:1, k:[{t, s, i, o, h}]}` with per-keyframe cubic-bezier easing and hold | yes, including the older `e`-end-value form |
//!
//! Not supported, and ignored rather than guessed: gradients (`gf`/`gs`),
//! masks and mattes, text layers, image layers, expressions, repeaters,
//! merge paths and effects. A file using them still plays; those parts are
//! simply absent, and [`Composition::unsupported`] lists what was skipped.

use std::collections::BTreeMap;

use vieww_foundation::json::Json;
use vieww_foundation::{
    Brush, Color, Offset, Path, Rect, Sketch, Sketchbook, StrokeStyle, Transform,
};

/// Why a file did not load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LottieError(pub String);

impl std::fmt::Display for LottieError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "lottie: {}", self.0)
    }
}

impl std::error::Error for LottieError {}

/// Solve a CSS-style cubic bezier (control points (x1,y1), (x2,y2)) at x.
fn bezier_ease(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    let bez = |a: f32, b: f32, t: f32| {
        let u = 1.0 - t;
        3.0 * u * u * t * a + 3.0 * u * t * t * b + t * t * t
    };
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    let mut t = x;
    for _ in 0..30 {
        let xv = bez(x1, x2, t);
        if (xv - x).abs() < 1e-5 {
            break;
        }
        if xv < x {
            lo = t;
        } else {
            hi = t;
        }
        t = 0.5 * (lo + hi);
    }
    bez(y1, y2, t)
}

#[derive(Debug, Clone, PartialEq)]
struct Key {
    t: f32,
    s: Vec<f32>,
    /// Explicit end value (old exports), else the next key's `s`.
    e: Option<Vec<f32>>,
    hold: bool,
    ease: Option<(f32, f32, f32, f32)>,
}

/// A possibly animated property.
#[derive(Debug, Clone, PartialEq)]
enum Prop {
    Static(Vec<f32>),
    Animated(Vec<Key>),
}

fn nums(j: &Json) -> Vec<f32> {
    match j {
        Json::Number(n) => {
            #[allow(clippy::cast_possible_truncation)]
            let v = *n as f32;
            vec![v]
        }
        Json::Array(a) => a.iter().filter_map(Json::as_f32).collect(),
        _ => Vec::new(),
    }
}

fn first_ease(j: Option<&Json>, key: &str) -> Option<f32> {
    let v = j?.get(key)?;
    match v {
        Json::Array(a) => a.first().and_then(Json::as_f32),
        other => other.as_f32(),
    }
}

impl Prop {
    fn parse(j: Option<&Json>, default: &[f32]) -> Self {
        let Some(j) = j else {
            return Self::Static(default.to_vec());
        };
        let animated = j.get("a").and_then(Json::as_f64).unwrap_or(0.0) != 0.0;
        let k = j.get("k").unwrap_or(j);
        if animated {
            let keys = k
                .as_array()
                .unwrap_or(&[])
                .iter()
                .map(|kf| {
                    let o = kf.get("o");
                    let i = kf.get("i");
                    let ease = match (
                        first_ease(o, "x"),
                        first_ease(o, "y"),
                        first_ease(i, "x"),
                        first_ease(i, "y"),
                    ) {
                        (Some(a), Some(b), Some(c), Some(d)) => Some((a, b, c, d)),
                        _ => None,
                    };
                    Key {
                        t: kf.get("t").and_then(Json::as_f32).unwrap_or(0.0),
                        s: kf.get("s").map(nums).unwrap_or_default(),
                        e: kf.get("e").map(nums),
                        hold: kf.get("h").and_then(Json::as_f64).unwrap_or(0.0) != 0.0,
                        ease,
                    }
                })
                .collect::<Vec<_>>();
            if keys.is_empty() {
                Self::Static(default.to_vec())
            } else {
                Self::Animated(keys)
            }
        } else {
            let v = nums(k);
            Self::Static(if v.is_empty() { default.to_vec() } else { v })
        }
    }

    fn at(&self, frame: f32) -> Vec<f32> {
        match self {
            Self::Static(v) => v.clone(),
            Self::Animated(keys) => {
                if frame <= keys[0].t {
                    return keys[0].s.clone();
                }
                for w in keys.windows(2) {
                    let (a, b) = (&w[0], &w[1]);
                    if frame < b.t {
                        if a.hold {
                            return a.s.clone();
                        }
                        let end = a.e.clone().unwrap_or_else(|| b.s.clone());
                        let span = (b.t - a.t).max(1e-6);
                        let u = (frame - a.t) / span;
                        let e = a
                            .ease
                            .map_or(u, |(x1, y1, x2, y2)| bezier_ease(x1, y1, x2, y2, u));
                        return a
                            .s
                            .iter()
                            .zip(end.iter().chain(std::iter::repeat(&0.0)))
                            .map(|(p, q)| p + (q - p) * e)
                            .collect();
                    }
                }
                let last = &keys[keys.len() - 1];
                if last.s.is_empty() {
                    keys[keys.len() - 2].e.clone().unwrap_or_default()
                } else {
                    last.s.clone()
                }
            }
        }
    }

    fn scalar(&self, frame: f32) -> f32 {
        self.at(frame).first().copied().unwrap_or(0.0)
    }

    fn vec2(&self, frame: f32) -> Offset {
        let v = self.at(frame);
        Offset::new(
            v.first().copied().unwrap_or(0.0),
            v.get(1).copied().unwrap_or(0.0),
        )
    }
}

/// A transform (`ks` or a group's `tr`).
#[derive(Debug, Clone, PartialEq)]
struct Xform {
    anchor: Prop,
    position: Prop,
    scale: Prop,
    rotation: Prop,
    opacity: Prop,
}

impl Xform {
    fn parse(j: Option<&Json>) -> Self {
        let g = |k: &str| j.and_then(|j| j.get(k));
        Self {
            anchor: Prop::parse(g("a"), &[0.0, 0.0]),
            position: Prop::parse(g("p"), &[0.0, 0.0]),
            scale: Prop::parse(g("s"), &[100.0, 100.0]),
            rotation: Prop::parse(g("r"), &[0.0]),
            opacity: Prop::parse(g("o"), &[100.0]),
        }
    }

    fn matrix(&self, f: f32) -> Transform {
        let a = self.anchor.vec2(f);
        let p = self.position.vec2(f);
        let s = self.scale.vec2(f);
        Transform::translate(Offset::new(-a.dx, -a.dy))
            .then(Transform::scale(s.dx / 100.0, s.dy / 100.0))
            .then(Transform::rotate(self.rotation.scalar(f).to_radians()))
            .then(Transform::translate(p))
    }

    fn opacity(&self, f: f32) -> f32 {
        (self.opacity.scalar(f) / 100.0).clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Item {
    Group {
        items: Vec<Item>,
        transform: Xform,
    },
    Rect {
        position: Prop,
        size: Prop,
        roundness: Prop,
    },
    Ellipse {
        position: Prop,
        size: Prop,
    },
    Bezier {
        path: BezierProp,
    },
    Star {
        star: bool,
        points: Prop,
        position: Prop,
        outer: Prop,
        inner: Prop,
        rotation: Prop,
    },
    Fill {
        color: Prop,
        opacity: Prop,
    },
    Stroke {
        color: Prop,
        opacity: Prop,
        width: Prop,
    },
    Trim {
        start: Prop,
        end: Prop,
        offset: Prop,
    },
}

#[derive(Debug, Clone, PartialEq)]
struct Bez {
    i: Vec<Offset>,
    o: Vec<Offset>,
    v: Vec<Offset>,
    closed: bool,
}

impl Bez {
    fn parse(j: &Json) -> Self {
        let pts = |k: &str| {
            j.get(k)
                .and_then(Json::as_array)
                .unwrap_or(&[])
                .iter()
                .map(|p| {
                    let v = nums(p);
                    Offset::new(
                        v.first().copied().unwrap_or(0.0),
                        v.get(1).copied().unwrap_or(0.0),
                    )
                })
                .collect::<Vec<_>>()
        };
        Self {
            i: pts("i"),
            o: pts("o"),
            v: pts("v"),
            closed: j.get("c").and_then(Json::as_bool).unwrap_or(false),
        }
    }

    fn lerp(&self, other: &Self, t: f32) -> Self {
        let l = |a: &[Offset], b: &[Offset]| {
            a.iter()
                .zip(b)
                .map(|(p, q)| Offset::new(p.dx + (q.dx - p.dx) * t, p.dy + (q.dy - p.dy) * t))
                .collect()
        };
        Self {
            i: l(&self.i, &other.i),
            o: l(&self.o, &other.o),
            v: l(&self.v, &other.v),
            closed: self.closed,
        }
    }

    fn path(&self) -> Path {
        let mut p = Path::new();
        let n = self.v.len();
        if n == 0 {
            return p;
        }
        p.move_to(self.v[0]);
        let seg = |p: &mut Path, a: usize, b: usize| {
            let c1 = self.v[a] + self.o.get(a).copied().unwrap_or(Offset::ZERO);
            let c2 = self.v[b] + self.i.get(b).copied().unwrap_or(Offset::ZERO);
            p.cubic_to(c1, c2, self.v[b]);
        };
        for k in 1..n {
            seg(&mut p, k - 1, k);
        }
        if self.closed {
            seg(&mut p, n - 1, 0);
            p.close();
        }
        p
    }
}

/// One shape keyframe: time, shape, easing, hold.
type ShapeKey = (f32, Bez, Option<(f32, f32, f32, f32)>, bool);

/// An animated bezier (`sh` shapes animate the whole vertex list).
#[derive(Debug, Clone, PartialEq)]
enum BezierProp {
    Static(Bez),
    Animated(Vec<ShapeKey>),
}

impl BezierProp {
    fn parse(j: Option<&Json>) -> Self {
        let Some(j) = j else {
            return Self::Static(Bez {
                i: vec![],
                o: vec![],
                v: vec![],
                closed: false,
            });
        };
        let animated = j.get("a").and_then(Json::as_f64).unwrap_or(0.0) != 0.0;
        let k = j.get("k").unwrap_or(j);
        if !animated {
            return Self::Static(Bez::parse(k));
        }
        let keys = k
            .as_array()
            .unwrap_or(&[])
            .iter()
            .filter_map(|kf| {
                let s = kf.get("s")?;
                let shape = s.index(0).unwrap_or(s);
                let ease = match (
                    first_ease(kf.get("o"), "x"),
                    first_ease(kf.get("o"), "y"),
                    first_ease(kf.get("i"), "x"),
                    first_ease(kf.get("i"), "y"),
                ) {
                    (Some(a), Some(b), Some(c), Some(d)) => Some((a, b, c, d)),
                    _ => None,
                };
                Some((
                    kf.get("t").and_then(Json::as_f32).unwrap_or(0.0),
                    Bez::parse(shape),
                    ease,
                    kf.get("h").and_then(Json::as_f64).unwrap_or(0.0) != 0.0,
                ))
            })
            .collect();
        Self::Animated(keys)
    }

    fn at(&self, f: f32) -> Bez {
        match self {
            Self::Static(b) => b.clone(),
            Self::Animated(keys) => {
                if keys.is_empty() {
                    return Bez {
                        i: vec![],
                        o: vec![],
                        v: vec![],
                        closed: false,
                    };
                }
                if f <= keys[0].0 {
                    return keys[0].1.clone();
                }
                for w in keys.windows(2) {
                    if f < w[1].0 {
                        if w[0].3 {
                            return w[0].1.clone();
                        }
                        let u = (f - w[0].0) / (w[1].0 - w[0].0).max(1e-6);
                        let e = w[0].2.map_or(u, |(a, b, c, d)| bezier_ease(a, b, c, d, u));
                        return w[0].1.lerp(&w[1].1, e);
                    }
                }
                keys[keys.len() - 1].1.clone()
            }
        }
    }
}

fn parse_items(list: &[Json], skipped: &mut BTreeMap<String, usize>) -> Vec<Item> {
    let mut out = Vec::new();
    for s in list {
        if s.get("hd").and_then(Json::as_bool) == Some(true) {
            continue;
        }
        let ty = s.get("ty").and_then(Json::as_str).unwrap_or("");
        let item = match ty {
            "gr" => {
                let it = s.get("it").and_then(Json::as_array).unwrap_or(&[]);
                let tr = it
                    .iter()
                    .find(|x| x.get("ty").and_then(Json::as_str) == Some("tr"));
                Item::Group {
                    items: parse_items(it, skipped),
                    transform: Xform::parse(tr),
                }
            }
            "rc" => Item::Rect {
                position: Prop::parse(s.get("p"), &[0.0, 0.0]),
                size: Prop::parse(s.get("s"), &[0.0, 0.0]),
                roundness: Prop::parse(s.get("r"), &[0.0]),
            },
            "el" => Item::Ellipse {
                position: Prop::parse(s.get("p"), &[0.0, 0.0]),
                size: Prop::parse(s.get("s"), &[0.0, 0.0]),
            },
            "sh" => Item::Bezier {
                path: BezierProp::parse(s.get("ks")),
            },
            "sr" => Item::Star {
                star: s.get("sy").and_then(Json::as_f64).unwrap_or(1.0) == 1.0,
                points: Prop::parse(s.get("pt"), &[5.0]),
                position: Prop::parse(s.get("p"), &[0.0, 0.0]),
                outer: Prop::parse(s.get("or"), &[10.0]),
                inner: Prop::parse(s.get("ir"), &[5.0]),
                rotation: Prop::parse(s.get("r"), &[0.0]),
            },
            "fl" => Item::Fill {
                color: Prop::parse(s.get("c"), &[0.0, 0.0, 0.0, 1.0]),
                opacity: Prop::parse(s.get("o"), &[100.0]),
            },
            "st" => Item::Stroke {
                color: Prop::parse(s.get("c"), &[0.0, 0.0, 0.0, 1.0]),
                opacity: Prop::parse(s.get("o"), &[100.0]),
                width: Prop::parse(s.get("w"), &[1.0]),
            },
            "tm" => Item::Trim {
                start: Prop::parse(s.get("s"), &[0.0]),
                end: Prop::parse(s.get("e"), &[100.0]),
                offset: Prop::parse(s.get("o"), &[0.0]),
            },
            "tr" => continue,
            other => {
                *skipped.entry(format!("shape:{other}")).or_default() += 1;
                continue;
            }
        };
        out.push(item);
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
enum LayerKind {
    Shape(Vec<Item>),
    Solid { color: Color, w: f32, h: f32 },
    Null,
    Precomp(String),
}

#[derive(Debug, Clone, PartialEq)]
struct Layer {
    index: Option<i64>,
    parent: Option<i64>,
    ip: f32,
    op: f32,
    st: f32,
    transform: Xform,
    kind: LayerKind,
    name: String,
}

#[allow(clippy::cast_possible_truncation)]
fn parse_layers(list: &[Json], skipped: &mut BTreeMap<String, usize>) -> Vec<Layer> {
    list.iter()
        .filter_map(|l| {
            let kind = match l.get("ty").and_then(Json::as_f64).unwrap_or(-1.0) as i64 {
                4 => LayerKind::Shape(parse_items(
                    l.get("shapes").and_then(Json::as_array).unwrap_or(&[]),
                    skipped,
                )),
                1 => {
                    let hex = l.get("sc").and_then(Json::as_str).unwrap_or("#000000");
                    let c = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0);
                    LayerKind::Solid {
                        color: Color::hex(c),
                        w: l.get("sw").and_then(Json::as_f32).unwrap_or(0.0),
                        h: l.get("sh").and_then(Json::as_f32).unwrap_or(0.0),
                    }
                }
                3 => LayerKind::Null,
                0 => LayerKind::Precomp(
                    l.get("refId")
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .to_owned(),
                ),
                other => {
                    *skipped.entry(format!("layer:{other}")).or_default() += 1;
                    return None;
                }
            };
            #[allow(clippy::cast_possible_truncation)]
            Some(Layer {
                index: l.get("ind").and_then(Json::as_f64).map(|v| v as i64),
                parent: l.get("parent").and_then(Json::as_f64).map(|v| v as i64),
                ip: l.get("ip").and_then(Json::as_f32).unwrap_or(0.0),
                op: l.get("op").and_then(Json::as_f32).unwrap_or(f32::MAX),
                st: l.get("st").and_then(Json::as_f32).unwrap_or(0.0),
                transform: Xform::parse(l.get("ks")),
                kind,
                name: l.get("nm").and_then(Json::as_str).unwrap_or("").to_owned(),
            })
        })
        .collect()
}

/// A loaded animation.
#[derive(Debug, Clone, PartialEq)]
pub struct Composition {
    pub width: f32,
    pub height: f32,
    pub frame_rate: f32,
    pub in_point: f32,
    pub out_point: f32,
    layers: Vec<Layer>,
    assets: BTreeMap<String, Vec<Layer>>,
    /// Features present in the file this player skipped, with counts.
    pub unsupported: BTreeMap<String, usize>,
}

fn color_of(v: &[f32], opacity: f32) -> Color {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let q = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    let a = v.get(3).copied().unwrap_or(1.0) * opacity;
    Color::rgba(
        q(v.first().copied().unwrap_or(0.0)),
        q(v.get(1).copied().unwrap_or(0.0)),
        q(v.get(2).copied().unwrap_or(0.0)),
        q(a),
    )
}

fn star_path(star: bool, points: f32, c: Offset, outer: f32, inner: f32, rot: f32) -> Path {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = points.round().max(3.0) as usize;
    let count = if star { n * 2 } else { n };
    let mut p = Path::new();
    for i in 0..count {
        #[allow(clippy::cast_precision_loss)]
        let a = (rot - 90.0).to_radians() + i as f32 * std::f32::consts::TAU / count as f32;
        let r = if star && i % 2 == 1 { inner } else { outer };
        let q = Offset::new(c.dx + a.cos() * r, c.dy + a.sin() * r);
        if i == 0 {
            p.move_to(q);
        } else {
            p.line_to(q);
        }
    }
    p.close();
    p
}

impl Composition {
    /// Parse a Bodymovin JSON document.
    ///
    /// # Errors
    ///
    /// Malformed JSON or a missing size.
    pub fn parse(text: &str) -> Result<Self, LottieError> {
        let doc = Json::parse(text).map_err(|e| LottieError(e.to_string()))?;
        let mut unsupported = BTreeMap::new();
        let width = doc
            .get("w")
            .and_then(Json::as_f32)
            .ok_or_else(|| LottieError("missing w".into()))?;
        let height = doc
            .get("h")
            .and_then(Json::as_f32)
            .ok_or_else(|| LottieError("missing h".into()))?;
        let mut assets = BTreeMap::new();
        for a in doc.get("assets").and_then(Json::as_array).unwrap_or(&[]) {
            if let (Some(id), Some(layers)) = (
                a.get("id").and_then(Json::as_str),
                a.get("layers").and_then(Json::as_array),
            ) {
                assets.insert(id.to_owned(), parse_layers(layers, &mut unsupported));
            }
        }
        Ok(Self {
            width,
            height,
            frame_rate: doc.get("fr").and_then(Json::as_f32).unwrap_or(30.0),
            in_point: doc.get("ip").and_then(Json::as_f32).unwrap_or(0.0),
            out_point: doc.get("op").and_then(Json::as_f32).unwrap_or(60.0),
            layers: parse_layers(
                doc.get("layers").and_then(Json::as_array).unwrap_or(&[]),
                &mut unsupported,
            ),
            assets,
            unsupported,
        })
    }

    /// Length in seconds.
    #[must_use]
    pub fn duration(&self) -> f32 {
        (self.out_point - self.in_point) / self.frame_rate.max(1e-6)
    }

    /// The frame shown at `seconds`, looping.
    #[must_use]
    pub fn frame_at(&self, seconds: f32) -> f32 {
        let span = (self.out_point - self.in_point).max(1e-6);
        self.in_point + (seconds * self.frame_rate).rem_euclid(span)
    }

    /// Layer names, top first.
    #[must_use]
    pub fn layer_names(&self) -> Vec<&str> {
        self.layers.iter().map(|l| l.name.as_str()).collect()
    }

    /// Draw frame `frame` into a sketchbook, fitted to `size` (the
    /// composition is scaled uniformly and centred, like `BoxFit::contain`).
    #[must_use]
    pub fn render(&self, frame: f32, fit: Rect) -> Sketchbook {
        let s = (fit.width() / self.width).min(fit.height() / self.height);
        let ox = fit.left + (fit.width() - self.width * s) / 2.0;
        let oy = fit.top + (fit.height() - self.height * s) / 2.0;
        let base = Transform::scale(s, s).then(Transform::translate(Offset::new(ox, oy)));
        let mut book = Sketchbook::new();
        let children = self.render_layers(&self.layers, frame, 0);
        book.push(Sketch::Layer {
            alpha: 1.0,
            blur: 0.0,
            blend: vieww_foundation::BlendMode::Normal,
            clip: Some(
                Path::rect(Rect::new(0.0, 0.0, self.width, self.height).translate(Offset::ZERO))
                    .transformed(base),
            ),
            children: vec![Sketch::Transformed {
                transform: base,
                children,
            }],
        });
        book
    }

    fn layer_matrix(layers: &[Layer], i: usize, f: f32) -> Transform {
        let mut m = layers[i].transform.matrix(f);
        let mut parent = layers[i].parent;
        let mut guard = 0;
        while let (Some(p), true) = (parent, guard < 64) {
            guard += 1;
            match layers.iter().find(|l| l.index == Some(p)) {
                Some(pl) => {
                    m = m.then(pl.transform.matrix(f));
                    parent = pl.parent;
                }
                None => break,
            }
        }
        m
    }

    fn render_layers(&self, layers: &[Layer], frame: f32, depth: usize) -> Vec<Sketch> {
        let mut out = Vec::new();
        // Lottie lists the top layer first; draw bottom-up.
        for i in (0..layers.len()).rev() {
            let l = &layers[i];
            if frame < l.ip || frame >= l.op {
                continue;
            }
            let local = frame - l.st;
            let m = Self::layer_matrix(layers, i, frame);
            let alpha = l.transform.opacity(frame);
            let content = match &l.kind {
                LayerKind::Null => continue,
                LayerKind::Solid { color, w, h } => vec![Sketch::Fill {
                    path: Path::rect(Rect::new(0.0, 0.0, *w, *h)),
                    brush: Brush::Solid(*color),
                }],
                LayerKind::Shape(items) => render_items(items, local, &[]),
                LayerKind::Precomp(id) => match (self.assets.get(id), depth < 8) {
                    (Some(sub), true) => self.render_layers(sub, local, depth + 1),
                    _ => continue,
                },
            };
            out.push(Sketch::Layer {
                alpha,
                blur: 0.0,
                blend: vieww_foundation::BlendMode::Normal,
                clip: None,
                children: vec![Sketch::Transformed {
                    transform: m,
                    children: content,
                }],
            });
        }
        out
    }
}

/// Render a group's items: collect its paths (and sub-groups'), then apply
/// each style in the group to them, bottom style first.
fn render_items(items: &[Item], f: f32, inherited_trim: &[(f32, f32, f32)]) -> Vec<Sketch> {
    let mut trims: Vec<(f32, f32, f32)> = inherited_trim.to_vec();
    for it in items {
        if let Item::Trim { start, end, offset } = it {
            trims.push((
                start.scalar(f) / 100.0,
                end.scalar(f) / 100.0,
                offset.scalar(f) / 360.0,
            ));
        }
    }
    let mut paths: Vec<Path> = Vec::new();
    let mut out = Vec::new();
    for it in items {
        match it {
            Item::Rect {
                position,
                size,
                roundness,
            } => {
                let c = position.vec2(f);
                let sz = size.vec2(f);
                let r = Rect::new(
                    c.dx - sz.dx / 2.0,
                    c.dy - sz.dy / 2.0,
                    c.dx + sz.dx / 2.0,
                    c.dy + sz.dy / 2.0,
                );
                let rr = roundness.scalar(f).min(sz.dx.min(sz.dy) / 2.0);
                paths.push(if rr > 0.0 {
                    Path::rounded_rect(r, rr)
                } else {
                    Path::rect(r)
                });
            }
            Item::Ellipse { position, size } => {
                let c = position.vec2(f);
                let sz = size.vec2(f);
                paths.push(
                    Path::arc(
                        Offset::ZERO,
                        1.0,
                        -std::f32::consts::FRAC_PI_2,
                        std::f32::consts::TAU,
                    )
                    .transformed(
                        Transform::scale(sz.dx / 2.0, sz.dy / 2.0).then(Transform::translate(c)),
                    ),
                );
            }
            Item::Bezier { path } => paths.push(path.at(f).path()),
            Item::Star {
                star,
                points,
                position,
                outer,
                inner,
                rotation,
            } => {
                paths.push(star_path(
                    *star,
                    points.scalar(f),
                    position.vec2(f),
                    outer.scalar(f),
                    inner.scalar(f),
                    rotation.scalar(f),
                ));
            }
            Item::Group {
                items: sub,
                transform,
            } => {
                let children = render_items(sub, f, &trims);
                out.push(Sketch::Layer {
                    alpha: transform.opacity(f),
                    blur: 0.0,
                    blend: vieww_foundation::BlendMode::Normal,
                    clip: None,
                    children: vec![Sketch::Transformed {
                        transform: transform.matrix(f),
                        children,
                    }],
                });
            }
            _ => {}
        }
    }
    let trimmed: Vec<Path> = paths
        .into_iter()
        .map(|p| {
            trims
                .iter()
                .fold(p, |acc, &(s, e, o)| acc.trim_offset(s.min(e), s.max(e), o))
        })
        .collect();
    // Styles: later items in the list sit *below* earlier ones in Lottie,
    // so draw them in reverse.
    let mut styled = Vec::new();
    for it in items.iter().rev() {
        match it {
            Item::Fill { color, opacity } => {
                let c = color_of(&color.at(f), opacity.scalar(f) / 100.0);
                for p in &trimmed {
                    styled.push(Sketch::Fill {
                        path: p.clone(),
                        brush: Brush::Solid(c),
                    });
                }
            }
            Item::Stroke {
                color,
                opacity,
                width,
            } => {
                let c = color_of(&color.at(f), opacity.scalar(f) / 100.0);
                for p in &trimmed {
                    styled.push(Sketch::Stroke {
                        path: p.clone(),
                        brush: Brush::Solid(c),
                        width: width.scalar(f),
                        style: StrokeStyle::rounded(),
                    });
                }
            }
            _ => {}
        }
    }
    // Sub-groups were listed first in `out`; in Lottie order an earlier
    // item is on top, so groups (usually listed before the styles) draw
    // after this group's own styled paths.
    styled.extend(out.into_iter().rev());
    styled
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"{
      "v": "5.7.0", "fr": 30, "ip": 0, "op": 60, "w": 200, "h": 100,
      "layers": [
        {"ty": 4, "nm": "dot", "ind": 1, "ip": 0, "op": 60, "st": 0,
         "ks": {"p": {"a": 1, "k": [{"t": 0, "s": [20, 50], "o": {"x": [0.42], "y": [0]}, "i": {"x": [0.58], "y": [1]}},
                                   {"t": 30, "s": [180, 50]}]},
                "o": {"a": 0, "k": 100}},
         "shapes": [{"ty": "gr", "it": [
            {"ty": "el", "p": {"a": 0, "k": [0, 0]}, "s": {"a": 0, "k": [20, 20]}},
            {"ty": "fl", "c": {"a": 0, "k": [1, 0, 0, 1]}, "o": {"a": 0, "k": 100}},
            {"ty": "tr", "p": {"a": 0, "k": [0, 0]}}
         ]}]},
        {"ty": 4, "nm": "line", "ind": 2, "ip": 0, "op": 60,
         "ks": {},
         "shapes": [
            {"ty": "sh", "ks": {"a": 0, "k": {"i": [[0,0],[0,0]], "o": [[0,0],[0,0]], "v": [[0,90],[200,90]], "c": false}}},
            {"ty": "st", "c": {"a": 0, "k": [0, 0, 1, 1]}, "o": {"a": 0, "k": 100}, "w": {"a": 0, "k": 4}},
            {"ty": "tm", "s": {"a": 0, "k": 0}, "e": {"a": 1, "k": [{"t": 0, "s": [0]}, {"t": 60, "s": [100]}]}, "o": {"a": 0, "k": 0}},
            {"ty": "gf"}
         ]},
        {"ty": 1, "nm": "bg", "ind": 3, "sc": "#eeeeee", "sw": 200, "sh": 100, "ip": 0, "op": 60, "ks": {}},
        {"ty": 5, "nm": "text"}
      ]
    }"##;

    #[test]
    fn parses_header_layers_and_reports_what_it_skips() {
        let c = Composition::parse(SAMPLE).unwrap();
        assert_eq!((c.width, c.height, c.frame_rate), (200.0, 100.0, 30.0));
        assert!((c.duration() - 2.0).abs() < 1e-6);
        assert_eq!(c.layer_names(), ["dot", "line", "bg"]);
        assert_eq!(c.unsupported.get("shape:gf"), Some(&1));
        assert_eq!(c.unsupported.get("layer:5"), Some(&1));
    }

    #[test]
    fn keyframes_ease_with_their_bezier_handles() {
        let c = Composition::parse(SAMPLE).unwrap();
        let LayerKind::Shape(_) = &c.layers[0].kind else {
            panic!()
        };
        let p = &c.layers[0].transform.position;
        assert_eq!(p.vec2(0.0), Offset::new(20.0, 50.0));
        assert_eq!(p.vec2(30.0), Offset::new(180.0, 50.0));
        let mid = p.vec2(15.0).dx;
        assert!((mid - 100.0).abs() < 1.0, "ease-in-out is symmetric: {mid}");
        let early = p.vec2(5.0).dx;
        assert!(
            early < 20.0 + 160.0 * (5.0 / 30.0),
            "eased start is slower than linear: {early}"
        );
    }

    #[test]
    fn rendering_draws_layers_bottom_up_with_trim() {
        let c = Composition::parse(SAMPLE).unwrap();
        let book = c.render(30.0, Rect::new(0.0, 0.0, 200.0, 100.0));
        let Sketch::Layer { children, .. } = &book.items()[0] else {
            panic!()
        };
        let Sketch::Transformed {
            children: layers, ..
        } = &children[0]
        else {
            panic!()
        };
        assert_eq!(layers.len(), 3, "bg, line, dot");
        // The trimmed line at frame 30 is half drawn.
        let mut stroke_len = None;
        fn find(s: &Sketch, out: &mut Option<f32>) {
            match s {
                Sketch::Stroke { path, .. } => *out = Some(path.length()),
                Sketch::Layer { children, .. } | Sketch::Transformed { children, .. } => {
                    children.iter().for_each(|c| find(c, out))
                }
                _ => {}
            }
        }
        find(&layers[1], &mut stroke_len);
        assert!((stroke_len.unwrap() - 100.0).abs() < 0.5, "{stroke_len:?}");
    }

    #[test]
    fn precomps_parents_and_holds() {
        let doc = r##"{"w": 100, "h": 100, "fr": 10, "ip": 0, "op": 20,
          "assets": [{"id": "comp_0", "layers": [
             {"ty": 1, "sc": "#ff0000", "sw": 10, "sh": 10, "ks": {"p": {"a": 0, "k": [5, 5]}}}]}],
          "layers": [
            {"ty": 3, "ind": 1, "ks": {"p": {"a": 1, "k": [{"t": 0, "s": [0, 0], "h": 1}, {"t": 10, "s": [50, 0]}]}}},
            {"ty": 0, "ind": 2, "parent": 1, "refId": "comp_0", "ks": {"p": {"a": 0, "k": [0, 40]}}}
          ]}"##;
        let c = Composition::parse(doc).unwrap();
        let m5 = Composition::layer_matrix(&c.layers, 1, 5.0);
        assert_eq!(
            m5.apply(Offset::ZERO),
            Offset::new(0.0, 40.0),
            "held until frame 10"
        );
        let m10 = Composition::layer_matrix(&c.layers, 1, 10.0);
        assert_eq!(
            m10.apply(Offset::ZERO),
            Offset::new(50.0, 40.0),
            "carried by its parent"
        );
        let book = c.render(12.0, Rect::new(0.0, 0.0, 100.0, 100.0));
        assert!(!book.is_empty());
    }

    #[test]
    fn frame_at_loops() {
        let c = Composition::parse(SAMPLE).unwrap();
        assert!((c.frame_at(0.5) - 15.0).abs() < 1e-4);
        assert!((c.frame_at(2.5) - 15.0).abs() < 1e-3);
    }
}
