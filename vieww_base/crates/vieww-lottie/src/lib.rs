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
//! | styles: `gf` gradient fill, `gs` gradient stroke (linear and radial, colour + opacity stops) | yes |
//! | layer masks (`masksProperties`): add, subtract, intersect, difference, inverted, opacity | yes, as path booleans into the layer's clip |
//! | track mattes (`tt` 1–4, matte layer `td`) | alpha and inverted alpha exact; luma mattes use the matte's alpha |
//! | image layers (`ty` 2) with embedded `data:` assets | yes, decoded by `vieww-image` |
//! | text layers (`ty` 5): document, fill, size, tracking, justification, and **text animators** (`t.a` range selectors) | yes — as [`TextRun`]s the host shapes with `vieww-text` ([`Composition::text_runs`]) |
//! | expressions (`x`): `wiggle`, `loopOut`/`loopIn`, `time`, `value`, `linear`/`ease`, maths | yes, by `vieww-animation::expr` (Bodymovin's `$bm_rt` wrapper is understood) |
//! | repeaters (`rp`): copies, offset, transform, start/end opacity | yes |
//! | merge paths (`mm`): merge, add, subtract, intersect, exclude | yes, by `vieww-foundation::path_ops` |
//! | layer effects (`ef`): Gaussian blur, drop shadow, fill | yes |
//!
//! Anything else (3D layers, layer styles beyond the drop shadow, text on a
//! path) still plays without that part, and [`Composition::unsupported`]
//! lists what was skipped.

use std::collections::BTreeMap;

use std::sync::Arc;

use vieww_animation::expr::{Expr, Scope, Val};
use vieww_foundation::json::Json;
use vieww_foundation::{
    BlendMode, Brush, Color, Gradient, Image, Offset, Path, PathOp, Rect, Sketch, Sketchbook,
    StrokeStyle, Transform,
};

thread_local! {
    /// The frame rate of the document being parsed (expressions run in
    /// seconds, properties are keyed in frames).
    static PARSE_FR: std::cell::Cell<f32> = const { std::cell::Cell::new(30.0) };
    static PARSE_SKIPPED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

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
    /// An expression over an underlying property; the frame rate converts
    /// frames to the expression's seconds.
    Expression(Box<Prop>, Arc<Expr>, f32),
}

/// Bodymovin wraps AE expressions as `var $bm_rt; $bm_rt = …;`.
fn unwrap_bodymovin(src: &str) -> String {
    let mut body = String::new();
    for stmt in src.split(';') {
        let s = stmt.trim();
        if s.is_empty() || s.starts_with("var $bm_rt") {
            continue;
        }
        let s = s.strip_prefix("$bm_rt =").or_else(|| s.strip_prefix("$bm_rt=")).unwrap_or(s);
        body = s.trim().to_owned();
    }
    body.replace("$bm_sum", "add").replace("$bm_mul", "mul")
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
        if let Some(src) = j.get("x").and_then(Json::as_str) {
            let mut base = j.clone();
            if let Json::Object(m) = &mut base {
                m.retain(|(k, _)| k != "x");
            }
            let inner = Self::parse(Some(&base), default);
            match Expr::parse(&unwrap_bodymovin(src)) {
                Ok(e) => return Self::Expression(Box::new(inner), Arc::new(e), PARSE_FR.with(std::cell::Cell::get)),
                Err(_) => {
                    PARSE_SKIPPED.with(|v| v.borrow_mut().push("expression".into()));
                    return inner;
                }
            }
        }
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

    fn key_range(&self) -> (f32, f32) {
        match self {
            Self::Animated(k) => (k[0].t, k[k.len() - 1].t),
            Self::Expression(inner, ..) => inner.key_range(),
            Self::Static(_) => (0.0, 0.0),
        }
    }

    fn at(&self, frame: f32) -> Vec<f32> {
        match self {
            Self::Static(v) => v.clone(),
            Self::Expression(inner, e, fr) => {
                let fr = fr.max(1e-6);
                let curve = |t: f32| Val::Vec(inner.at(t * fr));
                let (a, b) = inner.key_range();
                let scope = Scope::at(frame / fr)
                    .with_value(Val::Vec(inner.at(frame)))
                    .with_curve(&curve, a / fr, b / fr);
                match e.eval(&scope) {
                    Ok(Val::Num(n)) => vec![n],
                    Ok(Val::Vec(v)) => v,
                    _ => inner.at(frame),
                }
            }
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
    GradientFill {
        grad: Grad,
        opacity: Prop,
    },
    GradientStroke {
        grad: Grad,
        opacity: Prop,
        width: Prop,
    },
    Repeater {
        copies: Prop,
        offset: Prop,
        transform: Xform,
        start_opacity: Prop,
        end_opacity: Prop,
    },
    Merge {
        mode: i64,
    },
}

/// A gradient style's geometry and stops.
#[derive(Debug, Clone, PartialEq)]
struct Grad {
    radial: bool,
    start: Prop,
    end: Prop,
    count: usize,
    stops: Prop,
}

impl Grad {
    fn parse(s: &Json) -> Self {
        let g = s.get("g");
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Self {
            radial: s.get("t").and_then(Json::as_f64) == Some(2.0),
            start: Prop::parse(s.get("s"), &[0.0, 0.0]),
            end: Prop::parse(s.get("e"), &[100.0, 0.0]),
            count: g.and_then(|g| g.get("p")).and_then(Json::as_f64).unwrap_or(2.0) as usize,
            stops: Prop::parse(g.and_then(|g| g.get("k")), &[0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]),
        }
    }

    /// The brush for a path with `bounds`, faded by `opacity`.
    fn brush(&self, f: f32, bounds: Rect, opacity: f32) -> Brush {
        let raw = self.stops.at(f);
        let n = self.count.max(1);
        let alpha_at = |t: f32| -> f32 {
            // Opacity stops follow the colour stops as (offset, alpha) pairs.
            let extra = &raw[(n * 4).min(raw.len())..];
            if extra.len() < 4 {
                return 1.0;
            }
            let pairs: Vec<(f32, f32)> = extra.chunks_exact(2).map(|c| (c[0], c[1])).collect();
            if t <= pairs[0].0 {
                return pairs[0].1;
            }
            for w in pairs.windows(2) {
                if t <= w[1].0 {
                    let u = (t - w[0].0) / (w[1].0 - w[0].0).max(1e-6);
                    return w[0].1 + (w[1].1 - w[0].1) * u;
                }
            }
            pairs[pairs.len() - 1].1
        };
        let mut stops = Vec::new();
        for c in raw.chunks_exact(4).take(n) {
            let a = alpha_at(c[0]) * opacity;
            stops.push((c[0], color_of(&[c[1], c[2], c[3], a], 1.0)));
        }
        let (w, h) = (bounds.width().max(1e-6), bounds.height().max(1e-6));
        let unit = |p: Offset| Offset::new((p.dx - bounds.left) / w, (p.dy - bounds.top) / h);
        let (a, b) = (self.start.vec2(f), self.end.vec2(f));
        let g = if self.radial {
            let r = ((b.dx - a.dx).powi(2) + (b.dy - a.dy).powi(2)).sqrt();
            Gradient::radial(unit(a), r / w.max(h))
        } else {
            Gradient::linear(unit(a), unit(b))
        };
        Brush::Gradient(g.with_stops_resampled(&stops))
    }
}

/// Fill `path` with a gradient laid out in absolute shape space: the
/// gradient's box is a square around the path (so radial stays circular),
/// clipped to the path.
fn gradient_fill(path: &Path, grad: &Grad, f: f32, opacity: f32) -> Sketch {
    let b = path.bounds();
    let side = b.width().max(b.height()).max(1e-3);
    let sq = Rect::new(b.left, b.top, b.left + side, b.top + side);
    Sketch::Layer {
        alpha: 1.0,
        blur: 0.0,
        blend: BlendMode::Normal,
        clip: Some(path.clone()),
        children: vec![Sketch::Fill {
            path: Path::rect(sq),
            brush: grad.brush(f, sq, opacity),
        }],
    }
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
            "gf" => Item::GradientFill {
                grad: Grad::parse(s),
                opacity: Prop::parse(s.get("o"), &[100.0]),
            },
            "gs" => Item::GradientStroke {
                grad: Grad::parse(s),
                opacity: Prop::parse(s.get("o"), &[100.0]),
                width: Prop::parse(s.get("w"), &[1.0]),
            },
            "rp" => {
                let tr = s.get("tr");
                Item::Repeater {
                    copies: Prop::parse(s.get("c"), &[1.0]),
                    offset: Prop::parse(s.get("o"), &[0.0]),
                    transform: Xform::parse(tr),
                    start_opacity: Prop::parse(tr.and_then(|t| t.get("so")), &[100.0]),
                    end_opacity: Prop::parse(tr.and_then(|t| t.get("eo")), &[100.0]),
                }
            }
            #[allow(clippy::cast_possible_truncation)]
            "mm" => Item::Merge {
                mode: s.get("mm").and_then(Json::as_f64).unwrap_or(1.0) as i64,
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
    Image(String),
    Text(TextLayer),
}

/// A text layer's keyframed document and animators.
#[derive(Debug, Clone, PartialEq)]
struct TextLayer {
    /// (frame, document) — documents switch, they do not interpolate.
    docs: Vec<(f32, TextDocument)>,
    animators: Vec<(LottieSelector, Xform, Option<Prop>, Prop)>,
}

/// One text document state.
#[derive(Debug, Clone, PartialEq)]
pub struct TextDocument {
    pub text: String,
    pub font: String,
    pub size: f32,
    pub color: Color,
    /// 0 left, 1 right, 2 centre.
    pub justify: u8,
    /// Thousandths of an em, as AE.
    pub tracking: f32,
    pub line_height: f32,
}

#[derive(Debug, Clone, PartialEq)]
struct LottieSelector {
    start: Prop,
    end: Prop,
    offset: Prop,
    based_on: i64,
    shape: i64,
}

/// A text run for the host to shape: the document, its world transform,
/// and one resolved animator style per character.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub layer: String,
    pub document: TextDocument,
    pub transform: Transform,
    pub opacity: f32,
    pub styles: Vec<vieww_animation::text_animator::UnitStyle>,
}

/// A layer mask.
#[derive(Debug, Clone, PartialEq)]
struct Mask {
    mode: char,
    path: BezierProp,
    inverted: bool,
    opacity: Prop,
}

/// A layer effect.
#[derive(Debug, Clone, PartialEq)]
enum Effect {
    Blur(Prop),
    DropShadow { color: Prop, opacity: Prop, direction: Prop, distance: Prop, softness: Prop },
    Fill { color: Prop, opacity: Prop },
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
    masks: Vec<Mask>,
    /// Track matte mode (`tt`): 1 alpha, 2 inverted alpha, 3 luma, 4 inverted luma.
    matte: Option<i64>,
    /// This layer is a matte (`td`) and is not drawn itself.
    is_matte: bool,
    effects: Vec<Effect>,
}

fn effect_param<'a>(e: &'a Json, i: usize) -> Option<&'a Json> {
    e.get("ef")?.as_array()?.get(i)?.get("v")
}

fn parse_effects(l: &Json, skipped: &mut BTreeMap<String, usize>) -> Vec<Effect> {
    let mut out = Vec::new();
    for e in l.get("ef").and_then(Json::as_array).unwrap_or(&[]) {
        if e.get("en").and_then(Json::as_f64) == Some(0.0) {
            continue;
        }
        #[allow(clippy::cast_possible_truncation)]
        match e.get("ty").and_then(Json::as_f64).unwrap_or(-1.0) as i64 {
            29 => out.push(Effect::Blur(Prop::parse(effect_param(e, 0), &[0.0]))),
            25 => out.push(Effect::DropShadow {
                color: Prop::parse(effect_param(e, 0), &[0.0, 0.0, 0.0, 1.0]),
                opacity: Prop::parse(effect_param(e, 1), &[128.0]),
                direction: Prop::parse(effect_param(e, 2), &[135.0]),
                distance: Prop::parse(effect_param(e, 3), &[5.0]),
                softness: Prop::parse(effect_param(e, 4), &[0.0]),
            }),
            21 => out.push(Effect::Fill {
                color: Prop::parse(effect_param(e, 2), &[1.0, 0.0, 0.0, 1.0]),
                opacity: Prop::parse(effect_param(e, 6), &[1.0]),
            }),
            other => *skipped.entry(format!("effect:{other}")).or_default() += 1,
        }
    }
    out
}

fn parse_text(l: &Json) -> TextLayer {
    let t = l.get("t");
    let mut docs = Vec::new();
    for k in t.and_then(|t| t.get("d")).and_then(|d| d.get("k")).and_then(Json::as_array).unwrap_or(&[]) {
        let Some(sd) = k.get("s") else { continue };
        let fc = sd.get("fc").map(nums).unwrap_or_else(|| vec![0.0, 0.0, 0.0]);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        docs.push((
            k.get("t").and_then(Json::as_f32).unwrap_or(0.0),
            TextDocument {
                text: sd.get("t").and_then(Json::as_str).unwrap_or("").replace('\r', "\n"),
                font: sd.get("f").and_then(Json::as_str).unwrap_or("").to_owned(),
                size: sd.get("s").and_then(Json::as_f32).unwrap_or(24.0),
                color: color_of(&fc, 1.0),
                justify: sd.get("j").and_then(Json::as_f64).unwrap_or(0.0) as u8,
                tracking: sd.get("tr").and_then(Json::as_f32).unwrap_or(0.0),
                line_height: sd.get("lh").and_then(Json::as_f32).unwrap_or(0.0),
            },
        ));
    }
    let mut animators = Vec::new();
    for a in t.and_then(|t| t.get("a")).and_then(Json::as_array).unwrap_or(&[]) {
        let sel = a.get("s");
        let g = |k: &str| sel.and_then(|s| s.get(k));
        #[allow(clippy::cast_possible_truncation)]
        let selector = LottieSelector {
            start: Prop::parse(g("s"), &[0.0]),
            end: Prop::parse(g("e"), &[100.0]),
            offset: Prop::parse(g("o"), &[0.0]),
            based_on: g("b").and_then(Json::as_f64).unwrap_or(1.0) as i64,
            shape: g("sh").and_then(Json::as_f64).unwrap_or(1.0) as i64,
        };
        let props = a.get("a");
        let fc = props.and_then(|p| p.get("fc")).map(|j| Prop::parse(Some(j), &[0.0, 0.0, 0.0, 1.0]));
        let tracking = Prop::parse(props.and_then(|p| p.get("t")), &[0.0]);
        // Animator transforms default to identity (opacity 100 = unchanged).
        animators.push((selector, Xform::parse(props), fc, tracking));
    }
    TextLayer { docs, animators }
}

fn parse_masks(l: &Json) -> Vec<Mask> {
    l.get("masksProperties")
        .and_then(Json::as_array)
        .unwrap_or(&[])
        .iter()
        .map(|m| Mask {
            mode: m.get("mode").and_then(Json::as_str).and_then(|s| s.chars().next()).unwrap_or('a'),
            path: BezierProp::parse(m.get("pt")),
            inverted: m.get("inv").and_then(Json::as_bool).unwrap_or(false),
            opacity: Prop::parse(m.get("o"), &[100.0]),
        })
        .collect()
}

/// The combined mask clip in layer space, or `None` for "no clip".
fn mask_clip(masks: &[Mask], f: f32) -> Option<Path> {
    let big = Path::rect(Rect::new(-1.0e5, -1.0e5, 1.0e5, 1.0e5));
    let mut acc: Option<Path> = None;
    for m in masks {
        if m.mode == 'n' || m.opacity.scalar(f) <= 0.0 {
            continue;
        }
        let mut p = m.path.at(f).path();
        if m.inverted {
            p = big.boolean(&p, PathOp::Difference);
        }
        acc = Some(match (m.mode, acc) {
            ('s', None) => big.boolean(&p, PathOp::Difference),
            ('s', Some(a)) => a.boolean(&p, PathOp::Difference),
            ('i', None) => p,
            ('i', Some(a)) => a.boolean(&p, PathOp::Intersect),
            ('f', Some(a)) => a.boolean(&p, PathOp::Xor),
            (_, None) => p,
            (_, Some(a)) => a.boolean(&p, PathOp::Union),
        });
    }
    acc
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        })
    };
    let bytes: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=').collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut acc = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            acc |= val(*c)? << (18 - 6 * i);
        }
        #[allow(clippy::cast_possible_truncation)]
        let b = [(acc >> 16) as u8, (acc >> 8) as u8, acc as u8];
        out.extend_from_slice(&b[..chunk.len().saturating_sub(1)]);
    }
    Some(out)
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
                2 => LayerKind::Image(l.get("refId").and_then(Json::as_str).unwrap_or("").to_owned()),
                5 => LayerKind::Text(parse_text(l)),
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
                masks: parse_masks(l),
                matte: l.get("tt").and_then(Json::as_f64).map(|v| v as i64),
                is_matte: l.get("td").and_then(Json::as_f64).is_some_and(|v| v != 0.0),
                effects: parse_effects(l, skipped),
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
    images: BTreeMap<String, Image>,
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
        PARSE_FR.with(|c| c.set(doc.get("fr").and_then(Json::as_f32).unwrap_or(30.0)));
        PARSE_SKIPPED.with(|v| v.borrow_mut().clear());
        let mut assets = BTreeMap::new();
        let mut images = BTreeMap::new();
        for a in doc.get("assets").and_then(Json::as_array).unwrap_or(&[]) {
            if let (Some(id), Some(p)) = (a.get("id").and_then(Json::as_str), a.get("p").and_then(Json::as_str)) {
                if let Some(b64) = p.strip_prefix("data:").and_then(|r| r.split_once(";base64,")).map(|(_, d)| d) {
                    match base64_decode(b64).and_then(|bytes| vieww_image::codec::decode(&bytes).ok()) {
                        Some(d) => {
                            images.insert(id.to_owned(), d.into_image());
                        }
                        None => *unsupported.entry("image:undecodable".into()).or_default() += 1,
                    }
                } else if !p.is_empty() {
                    // External files are the host's to resolve.
                    *unsupported.entry("image:external".into()).or_default() += 1;
                }
            }
            if let (Some(id), Some(layers)) = (
                a.get("id").and_then(Json::as_str),
                a.get("layers").and_then(Json::as_array),
            ) {
                assets.insert(id.to_owned(), parse_layers(layers, &mut unsupported));
            }
        }
        let layers = parse_layers(
            doc.get("layers").and_then(Json::as_array).unwrap_or(&[]),
            &mut unsupported,
        );
        for k in PARSE_SKIPPED.with(|v| std::mem::take(&mut *v.borrow_mut())) {
            *unsupported.entry(k).or_default() += 1;
        }
        Ok(Self {
            width,
            height,
            frame_rate: doc.get("fr").and_then(Json::as_f32).unwrap_or(30.0),
            in_point: doc.get("ip").and_then(Json::as_f32).unwrap_or(0.0),
            out_point: doc.get("op").and_then(Json::as_f32).unwrap_or(60.0),
            layers,
            assets,
            images,
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

    /// One layer's content in its own space (no transform, no opacity).
    fn layer_content(&self, l: &Layer, frame: f32, depth: usize) -> Option<Vec<Sketch>> {
        let local = frame - l.st;
        Some(match &l.kind {
            LayerKind::Null | LayerKind::Text(_) => return None,
            LayerKind::Solid { color, w, h } => vec![Sketch::Fill {
                path: Path::rect(Rect::new(0.0, 0.0, *w, *h)),
                brush: Brush::Solid(*color),
            }],
            LayerKind::Shape(items) => render_items(items, local, &[]),
            LayerKind::Image(id) => {
                let img = self.images.get(id)?;
                #[allow(clippy::cast_precision_loss)]
                let r = Rect::new(0.0, 0.0, img.width() as f32, img.height() as f32);
                vec![Sketch::Image { rect: r, image: img.clone() }]
            }
            LayerKind::Precomp(id) => match (self.assets.get(id), depth < 8) {
                (Some(sub), true) => self.render_layers(sub, local, depth + 1),
                _ => return None,
            },
        })
    }

    /// A layer fully dressed: content, masks, effects, transform, opacity.
    fn dressed(&self, layers: &[Layer], i: usize, frame: f32, depth: usize) -> Option<Sketch> {
        let l = &layers[i];
        let mut content = self.layer_content(l, frame, depth)?;
        let local = frame - l.st;
        if let Some(clip) = mask_clip(&l.masks, local) {
            content = vec![Sketch::Layer {
                alpha: 1.0,
                blur: 0.0,
                blend: BlendMode::Normal,
                clip: Some(clip),
                children: content,
            }];
        }
        let mut blur = 0.0;
        for e in &l.effects {
            match e {
                Effect::Blur(b) => blur += b.scalar(local),
                Effect::Fill { color, opacity } => {
                    // Recolour everything drawn, keeping its alpha.
                    let c = color_of(&color.at(local), opacity.scalar(local));
                    content = vec![Sketch::Layer {
                        alpha: 1.0,
                        blur: 0.0,
                        blend: BlendMode::Normal,
                        clip: None,
                        children: vec![
                            Sketch::Layer { alpha: 1.0, blur: 0.0, blend: BlendMode::Normal, clip: None, children: content },
                            Sketch::Layer {
                                alpha: 1.0,
                                blur: 0.0,
                                blend: BlendMode::SrcAtop,
                                clip: None,
                                children: vec![Sketch::Fill { path: Path::rect(Rect::new(-1.0e4, -1.0e4, 1.0e4, 1.0e4)), brush: Brush::Solid(c) }],
                            },
                        ],
                    }];
                }
                Effect::DropShadow { color, opacity, direction, distance, softness } => {
                    let c = color_of(&color.at(local), opacity.scalar(local) / 255.0);
                    let a = (direction.scalar(local) - 90.0).to_radians();
                    let d = distance.scalar(local);
                    let shadow = Sketch::Layer {
                        alpha: 1.0,
                        blur: softness.scalar(local) * 0.5,
                        blend: BlendMode::Normal,
                        clip: None,
                        children: vec![Sketch::Transformed {
                            transform: Transform::translate(Offset::new(a.cos() * d, a.sin() * d)),
                            children: vec![
                                Sketch::Layer { alpha: 1.0, blur: 0.0, blend: BlendMode::Normal, clip: None, children: content.clone() },
                                Sketch::Layer {
                                    alpha: 1.0,
                                    blur: 0.0,
                                    blend: BlendMode::SrcIn,
                                    clip: None,
                                    children: vec![Sketch::Fill { path: Path::rect(Rect::new(-1.0e4, -1.0e4, 1.0e4, 1.0e4)), brush: Brush::Solid(c) }],
                                },
                            ],
                        }],
                    };
                    let mut both = vec![shadow];
                    both.extend(content);
                    content = both;
                }
            }
        }
        Some(Sketch::Layer {
            alpha: l.transform.opacity(frame),
            blur,
            blend: BlendMode::Normal,
            clip: None,
            children: vec![Sketch::Transformed {
                transform: Self::layer_matrix(layers, i, frame),
                children: content,
            }],
        })
    }

    fn render_layers(&self, layers: &[Layer], frame: f32, depth: usize) -> Vec<Sketch> {
        let mut out = Vec::new();
        // Lottie lists the top layer first; draw bottom-up.
        for i in (0..layers.len()).rev() {
            let l = &layers[i];
            if frame < l.ip || frame >= l.op || l.is_matte {
                continue;
            }
            let Some(mut sketch) = self.dressed(layers, i, frame, depth) else { continue };
            if let Some(mode) = l.matte {
                // The matte is the layer directly above.
                let matte = (i > 0)
                    .then(|| &layers[i - 1])
                    .filter(|m| frame >= m.ip && frame < m.op)
                    .and_then(|_| self.dressed(layers, i - 1, frame, depth));
                let blend = if mode == 2 || mode == 4 { BlendMode::DstOut } else { BlendMode::DstIn };
                let mut kids = vec![sketch];
                match matte {
                    Some(m) => kids.push(Sketch::Layer { alpha: 1.0, blur: 0.0, blend, clip: None, children: vec![m] }),
                    None if blend == BlendMode::DstIn => kids.clear(),
                    None => {}
                }
                sketch = Sketch::Layer { alpha: 1.0, blur: 0.0, blend: BlendMode::Normal, clip: None, children: kids };
            }
            out.push(sketch);
        }
        out
    }

    /// The text layers visible at `seconds`, with their documents, world
    /// transforms (composition space) and per-character animator styles —
    /// for the host to shape and draw with its text stack.
    #[must_use]
    pub fn text_runs(&self, seconds: f32) -> Vec<TextRun> {
        use vieww_animation::text_animator::{self as ta, BasedOn, RangeSelector, Shape, TextAnimator, AnimatorProps};
        let frame = self.frame_at(seconds);
        let mut out = Vec::new();
        for (i, l) in self.layers.iter().enumerate().rev() {
            let LayerKind::Text(t) = &l.kind else { continue };
            if frame < l.ip || frame >= l.op {
                continue;
            }
            let local = frame - l.st;
            let Some(doc) = t.docs.iter().rev().find(|(k, _)| *k <= local).or(t.docs.first()).map(|d| d.1.clone()) else { continue };
            let animators: Vec<TextAnimator> = t
                .animators
                .iter()
                .map(|(sel, xf, fc, tr)| {
                    let p = xf.position.vec2(local);
                    let sc = xf.scale.vec2(local);
                    TextAnimator {
                        props: AnimatorProps {
                            position: (p.dx, p.dy),
                            scale: sc.dx / 100.0,
                            rotation: xf.rotation.scalar(local),
                            opacity: xf.opacity.scalar(local) / 100.0,
                            tracking: tr.scalar(local),
                            fill: fc.as_ref().map(|c| color_of(&c.at(local), 1.0)),
                            blur: 0.0,
                        },
                        range: RangeSelector {
                            start: sel.start.scalar(local),
                            end: sel.end.scalar(local),
                            offset: sel.offset.scalar(local),
                            based_on: match sel.based_on {
                                2 => BasedOn::CharactersExcludingSpaces,
                                3 => BasedOn::Words,
                                4 => BasedOn::Lines,
                                _ => BasedOn::Characters,
                            },
                            shape: match sel.shape {
                                2 => Shape::RampUp,
                                3 => Shape::RampDown,
                                4 => Shape::Triangle,
                                5 => Shape::Round,
                                6 => Shape::Smooth,
                                _ => Shape::Square,
                            },
                            ..RangeSelector::default()
                        },
                        wiggly: None,
                    }
                })
                .collect();
            out.push(TextRun {
                layer: l.name.clone(),
                styles: ta::resolve(&doc.text, &animators, local / self.frame_rate.max(1e-6)),
                document: doc,
                transform: Self::layer_matrix(&self.layers, i, frame),
                opacity: l.transform.opacity(frame),
            });
        }
        out
    }
}

/// Render a group's items: collect its paths (and sub-groups'), then apply
/// each style in the group to them, bottom style first.
fn render_items(items: &[Item], f: f32, inherited_trim: &[(f32, f32, f32)]) -> Vec<Sketch> {
    // A repeater repeats everything above it in the list.
    if let Some(r) = items.iter().position(|it| matches!(it, Item::Repeater { .. })) {
        let Item::Repeater {
            copies,
            offset,
            transform,
            start_opacity,
            end_opacity,
        } = &items[r]
        else {
            unreachable!()
        };
        let mut out = render_items(&items[r + 1..], f, inherited_trim);
        let body = render_items(&items[..r], f, inherited_trim);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = copies.scalar(f).round().max(0.0) as usize;
        let off = offset.scalar(f);
        let a = transform.anchor.vec2(f);
        let p = transform.position.vec2(f);
        let sc = transform.scale.vec2(f);
        let rot = transform.rotation.scalar(f);
        let (so, eo) = (start_opacity.scalar(f) / 100.0, end_opacity.scalar(f) / 100.0);
        for i in 0..n {
            #[allow(clippy::cast_precision_loss)]
            let k = i as f32 + off;
            let m = Transform::translate(Offset::new(-a.dx, -a.dy))
                .then(Transform::scale((sc.dx / 100.0).powf(k), (sc.dy / 100.0).powf(k)))
                .then(Transform::rotate((rot * k).to_radians()))
                .then(Transform::translate(Offset::new(a.dx + (p.dx - a.dx) * k, a.dy + (p.dy - a.dy) * k)));
            #[allow(clippy::cast_precision_loss)]
            let t = if n > 1 { i as f32 / (n - 1) as f32 } else { 0.0 };
            out.push(Sketch::Layer {
                alpha: (so + (eo - so) * t).clamp(0.0, 1.0),
                blur: 0.0,
                blend: BlendMode::Normal,
                clip: None,
                children: vec![Sketch::Transformed {
                    transform: m,
                    children: body.clone(),
                }],
            });
        }
        return out;
    }
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
    // Merge paths combine everything collected so far into one path.
    if let Some(Item::Merge { mode }) = items.iter().find(|it| matches!(it, Item::Merge { .. })) {
        if paths.len() > 1 {
            let op = match mode {
                3 => Some(PathOp::Difference),
                4 => Some(PathOp::Intersect),
                5 => Some(PathOp::Xor),
                2 => Some(PathOp::Union),
                _ => None,
            };
            let merged = match op {
                Some(op) => paths[1..].iter().fold(paths[0].clone(), |acc, p| acc.boolean(p, op)),
                None => {
                    let mut all = Path::new();
                    for p in &paths {
                        all.extend(p);
                    }
                    all
                }
            };
            paths = vec![merged];
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
            Item::GradientFill { grad, opacity } => {
                for p in &trimmed {
                    styled.push(gradient_fill(p, grad, f, opacity.scalar(f) / 100.0));
                }
            }
            Item::GradientStroke {
                grad,
                opacity,
                width,
            } => {
                for p in &trimmed {
                    let outline = p.stroke_outline(width.scalar(f));
                    styled.push(gradient_fill(&outline, grad, f, opacity.scalar(f) / 100.0));
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
        assert_eq!(c.layer_names(), ["dot", "line", "bg", "text"]);
        // Gradient fills and text layers are understood now; a bare `gf`
        // with no geometry still parses (with defaults).
        assert!(c.unsupported.is_empty(), "{:?}", c.unsupported);
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

    fn flatten(s: &Sketch, out: &mut Vec<Sketch>) {
        match s {
            Sketch::Layer { children, .. } | Sketch::Transformed { children, .. } => {
                out.push(s.clone());
                children.iter().for_each(|c| flatten(c, out));
            }
            other => out.push(other.clone()),
        }
    }

    fn all(book: &Sketchbook) -> Vec<Sketch> {
        let mut v = Vec::new();
        book.items().iter().for_each(|s| flatten(s, &mut v));
        v
    }

    const RICH: &str = r##"{"w": 100, "h": 100, "fr": 30, "ip": 0, "op": 60,
      "assets": [{"id": "img", "w": 1, "h": 1, "e": 1,
        "p": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg=="}],
      "layers": [
        {"ty": 4, "nm": "matte", "ind": 1, "td": 1, "ks": {},
         "shapes": [{"ty": "rc", "p": {"a": 0, "k": [50, 50]}, "s": {"a": 0, "k": [40, 40]}, "r": {"a": 0, "k": 0}},
                    {"ty": "fl", "c": {"a": 0, "k": [1, 1, 1, 1]}, "o": {"a": 0, "k": 100}}]},
        {"ty": 4, "nm": "grad", "ind": 2, "tt": 1, "ks": {},
         "masksProperties": [{"mode": "a", "inv": false, "o": {"a": 0, "k": 100},
            "pt": {"a": 0, "k": {"i": [[0,0],[0,0],[0,0]], "o": [[0,0],[0,0],[0,0]], "v": [[0,0],[100,0],[0,100]], "c": true}}}],
         "ef": [{"ty": 29, "ef": [{"v": {"a": 0, "k": 4}}]}],
         "shapes": [
            {"ty": "rc", "p": {"a": 0, "k": [50, 50]}, "s": {"a": 0, "k": [100, 100]}, "r": {"a": 0, "k": 0}},
            {"ty": "gf", "t": 1, "s": {"a": 0, "k": [0, 0]}, "e": {"a": 0, "k": [100, 0]}, "o": {"a": 0, "k": 100},
             "g": {"p": 2, "k": {"a": 0, "k": [0, 1, 0, 0, 1, 0, 0, 1]}}}
         ]},
        {"ty": 4, "nm": "rep", "ind": 3, "ks": {},
         "shapes": [{"ty": "el", "p": {"a": 0, "k": [10, 10]}, "s": {"a": 0, "k": [5, 5]}},
                    {"ty": "fl", "c": {"a": 0, "k": [0, 0, 0, 1]}, "o": {"a": 0, "k": 100}},
                    {"ty": "rp", "c": {"a": 0, "k": 4}, "o": {"a": 0, "k": 0},
                     "tr": {"p": {"a": 0, "k": [10, 0]}, "a": {"a": 0, "k": [0, 0]}, "s": {"a": 0, "k": [100, 100]}, "r": {"a": 0, "k": 0},
                            "so": {"a": 0, "k": 100}, "eo": {"a": 0, "k": 25}}}]},
        {"ty": 4, "nm": "merge", "ind": 4, "ks": {"p": {"a": 0, "k": [0, 0], "x": "var $bm_rt;\n$bm_rt = [time * 30, 0];"}},
         "shapes": [{"ty": "rc", "p": {"a": 0, "k": [50, 50]}, "s": {"a": 0, "k": [40, 40]}, "r": {"a": 0, "k": 0}},
                    {"ty": "el", "p": {"a": 0, "k": [50, 50]}, "s": {"a": 0, "k": [20, 20]}},
                    {"ty": "mm", "mm": 3},
                    {"ty": "fl", "c": {"a": 0, "k": [0, 0, 1, 1]}, "o": {"a": 0, "k": 100}}]},
        {"ty": 2, "nm": "pic", "ind": 5, "refId": "img", "ks": {}},
        {"ty": 5, "nm": "title", "ind": 6, "ks": {},
         "t": {"d": {"k": [{"t": 0, "s": {"t": "hello", "s": 20, "f": "Inter", "fc": [1, 1, 1], "j": 2}}]},
               "a": [{"s": {"s": {"a": 0, "k": 0}, "e": {"a": 0, "k": 40}, "o": {"a": 0, "k": 0}, "b": 1, "sh": 1},
                      "a": {"o": {"a": 0, "k": 0}, "p": {"a": 0, "k": [0, 10]}}}]}}
      ]}"##;

    #[test]
    fn everything_in_a_rich_file_is_understood() {
        let c = Composition::parse(RICH).expect("parse");
        assert!(c.unsupported.is_empty(), "{:?}", c.unsupported);
        assert_eq!(c.images.len(), 1);
        let book = c.render(10.0, Rect::new(0.0, 0.0, 100.0, 100.0));
        let items = all(&book);
        // Matte composited with DstIn; the matte layer itself is not drawn
        // on its own.
        assert!(items.iter().any(|s| matches!(s, Sketch::Layer { blend: BlendMode::DstIn, .. })));
        // The mask became a clip; the blur effect a layer blur.
        assert!(items.iter().any(|s| matches!(s, Sketch::Layer { clip: Some(_), .. })));
        assert!(items.iter().any(|s| matches!(s, Sketch::Layer { blur, .. } if (*blur - 4.0).abs() < 1e-5)));
        // A gradient brush was produced.
        assert!(items.iter().any(|s| matches!(s, Sketch::Fill { brush: Brush::Gradient(_), .. })));
        // The image layer drew its decoded pixels.
        assert!(items.iter().any(|s| matches!(s, Sketch::Image { .. })));
    }

    #[test]
    fn repeaters_make_copies_fading_from_start_to_end_opacity() {
        let c = Composition::parse(RICH).expect("parse");
        let LayerKind::Shape(items) = &c.layers[2].kind else { panic!() };
        let out = render_items(items, 0.0, &[]);
        let alphas: Vec<f32> = out
            .iter()
            .filter_map(|s| if let Sketch::Layer { alpha, .. } = s { Some(*alpha) } else { None })
            .collect();
        assert_eq!(alphas.len(), 4);
        assert!((alphas[0] - 1.0).abs() < 1e-5 && (alphas[3] - 0.25).abs() < 1e-5);
    }

    #[test]
    fn merge_paths_subtract_and_expressions_drive_properties() {
        let c = Composition::parse(RICH).expect("parse");
        let LayerKind::Shape(items) = &c.layers[3].kind else { panic!() };
        let out = render_items(items, 0.0, &[]);
        let Sketch::Fill { path, .. } = &out[0] else { panic!("{out:?}") };
        assert!(!path.contains(Offset::new(50.0, 50.0), vieww_foundation::FillRule::NonZero));
        assert!(path.contains(Offset::new(33.0, 33.0), vieww_foundation::FillRule::NonZero));
        // `time * 30` at frame 15 (0.5 s) = 15.
        let m = Composition::layer_matrix(&c.layers, 3, 15.0);
        assert!((m.apply(Offset::ZERO).dx - 15.0).abs() < 1e-3);
    }

    #[test]
    fn text_layers_become_runs_with_animator_styles() {
        let c = Composition::parse(RICH).expect("parse");
        let runs = c.text_runs(0.0);
        assert_eq!(runs.len(), 1);
        let r = &runs[0];
        assert_eq!(r.document.text, "hello");
        assert_eq!(r.document.justify, 2);
        assert_eq!(r.styles.len(), 5);
        // The selector covers the first 40 % (two characters): hidden and lowered.
        assert!(r.styles[0].opacity < 0.01 && (r.styles[0].offset.1 - 10.0).abs() < 1e-4);
        assert!(r.styles[4].opacity > 0.99);
    }

    #[test]
    fn base64_decodes() {
        assert_eq!(base64_decode("aGVsbG8=").expect("b64"), b"hello");
        assert!(base64_decode("a$").is_none());
    }
}
