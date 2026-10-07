//! Vector export: a [`Sketchbook`] as an SVG document or a PDF page.
//!
//! # The capability
//!
//! Fabric.js' `canvas.toSVG()`, Processing's `PGraphicsPDF`, Cairo's SVG and
//! PDF surfaces (Manim's `CairoRenderer`), After Effects' and Blender's vector
//! outputs: a drawing leaves the program as *vector* geometry rather than as
//! pixels, so it prints at any size and opens in an editor. A `Sketchbook` is
//! already a display list of paths and brushes, so export is a translation of
//! each [`Sketch`] into the target's vocabulary — never a rasterisation.
//!
//! # What each format can and cannot say
//!
//! | sketch feature | SVG | PDF |
//! |---|---|---|
//! | fills, strokes (caps, joins, miter, dashes) | exact | exact |
//! | linear / radial gradients | exact (`userSpaceOnUse`) | exact (axial / radial shadings, stitched stops) |
//! | sweep gradient | its representative colour | its representative colour |
//! | layer alpha | exact (`opacity`) | per-shape (the layer's alpha multiplied into each child's) |
//! | layer blend | CSS `mix-blend-mode` for the separable + non-separable modes | `/BM` for the same set |
//! | Porter–Duff modes (`SrcIn`, `Xor`, …) | drawn `Normal` | drawn `Normal` |
//! | layer blur, shadows | `feGaussianBlur` | drawn **unblurred** — PDF has no blur operator |
//! | clips, transforms | exact | exact |
//!
//! Every approximation above is a property of the *format*, and the table is
//! the whole list: nothing else is dropped.

use std::fmt::Write as _;

use crate::paint::{BlendMode, Gradient, GradientGeometry, StrokeCap, StrokeJoin, StrokeStyle};
use crate::sketch::{Brush, Sketch, Sketchbook};
use crate::{Color, Path, PathVerb, Rect, Size, Transform};

/// A PNG with stored (uncompressed) zlib blocks — enough to embed an image
/// in an SVG data URI without a codec dependency in foundation.
fn png_stored(img: &crate::Image) -> Vec<u8> {
    fn crc(bytes: &[u8]) -> u32 {
        let mut c = 0xFFFF_FFFFu32;
        for &b in bytes {
            c ^= u32::from(b);
            for _ in 0..8 {
                c = if c & 1 == 1 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        c ^ 0xFFFF_FFFF
    }
    let chunk = |out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]| {
        #[allow(clippy::cast_possible_truncation)]
        out.extend((data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend(data);
        out.extend(&body);
        out.extend(crc(&body).to_be_bytes());
    };
    let (w, h) = (img.width(), img.height());
    let mut raw = Vec::with_capacity((w as usize * 4 + 1) * h as usize);
    for row in img.pixels().chunks((w as usize * 4).max(1)) {
        raw.push(0);
        raw.extend(row);
    }
    let mut z = vec![0x78, 0x01];
    let parts: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, p) in parts.iter().enumerate() {
        z.push(u8::from(i + 1 == parts.len()));
        #[allow(clippy::cast_possible_truncation)]
        let l = p.len() as u16;
        z.extend(l.to_le_bytes());
        z.extend((!l).to_le_bytes());
        z.extend(*p);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + u32::from(x)) % 65_521;
        b = (b + a) % 65_521;
    }
    z.extend(((b << 16) | a).to_be_bytes());
    let mut out = vec![137, 80, 78, 71, 13, 10, 26, 10];
    let mut ihdr = Vec::new();
    ihdr.extend(w.to_be_bytes());
    ihdr.extend(h.to_be_bytes());
    ihdr.extend([8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn base64(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = u32::from(c[0]) << 16 | u32::from(*c.get(1).unwrap_or(&0)) << 8 | u32::from(*c.get(2).unwrap_or(&0));
        for k in 0..4 {
            if k <= c.len() {
                s.push(A[((n >> (18 - 6 * k)) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

fn css_blend(mode: BlendMode) -> Option<&'static str> {
    Some(match mode {
        BlendMode::Multiply => "multiply",
        BlendMode::Screen => "screen",
        BlendMode::Overlay => "overlay",
        BlendMode::Darken => "darken",
        BlendMode::Lighten => "lighten",
        BlendMode::ColorDodge => "color-dodge",
        BlendMode::ColorBurn => "color-burn",
        BlendMode::HardLight => "hard-light",
        BlendMode::SoftLight => "soft-light",
        BlendMode::Difference => "difference",
        BlendMode::Exclusion => "exclusion",
        BlendMode::Hue => "hue",
        BlendMode::Saturation => "saturation",
        BlendMode::Color => "color",
        BlendMode::Luminosity => "luminosity",
        _ => return None,
    })
}

fn pdf_blend(mode: BlendMode) -> Option<&'static str> {
    Some(match mode {
        BlendMode::Multiply => "Multiply",
        BlendMode::Screen => "Screen",
        BlendMode::Overlay => "Overlay",
        BlendMode::Darken => "Darken",
        BlendMode::Lighten => "Lighten",
        BlendMode::ColorDodge => "ColorDodge",
        BlendMode::ColorBurn => "ColorBurn",
        BlendMode::HardLight => "HardLight",
        BlendMode::SoftLight => "SoftLight",
        BlendMode::Difference => "Difference",
        BlendMode::Exclusion => "Exclusion",
        BlendMode::Hue => "Hue",
        BlendMode::Saturation => "Saturation",
        BlendMode::Color => "Color",
        BlendMode::Luminosity => "Luminosity",
        _ => return None,
    })
}

fn n(v: f32) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    if r == 0.0 {
        "0".into()
    } else {
        format!("{r}")
    }
}

fn hex(c: Color) -> String {
    let c = c.to_srgb();
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

fn alpha(c: Color) -> f32 {
    f32::from(c.a) / 255.0
}

/// A path's bounds as a gradient resolves them.
fn bounds_of(path: &Path) -> Rect {
    path.bounds()
}

impl Sketchbook {
    /// This drawing as a standalone SVG 1.1 document of `size`.
    ///
    /// ```
    /// use vieww_foundation::{Color, Rect, Size, Sketchbook};
    ///
    /// let mut book = Sketchbook::new();
    /// book.rect(Rect::new(0.0, 0.0, 10.0, 10.0), Color::RED);
    /// let svg = book.to_svg(Size::new(10.0, 10.0));
    /// assert!(svg.starts_with("<svg"));
    /// assert!(svg.contains("fill=\"#ff0000\""));
    /// ```
    #[must_use]
    pub fn to_svg(&self, size: Size) -> String {
        let mut w = SvgWriter::default();
        let mut body = String::new();
        for item in self.items() {
            w.sketch(item, &mut body, 1);
        }
        let mut out = String::new();
        let _ = writeln!(
            out,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">",
            n(size.width),
            n(size.height),
            n(size.width),
            n(size.height)
        );
        if !w.defs.is_empty() {
            let _ = writeln!(out, "  <defs>\n{}  </defs>", w.defs);
        }
        out.push_str(&body);
        out.push_str("</svg>\n");
        out
    }

    /// This drawing as a single-page PDF 1.4 file of `size` points.
    ///
    /// The file is uncompressed and self-contained: every object is written
    /// in plain text with a correct cross-reference table, so it opens in any
    /// reader and diffs as text.
    #[must_use]
    pub fn to_pdf(&self, size: Size) -> Vec<u8> {
        let mut w = PdfWriter::default();
        let mut content = String::new();
        // PDF's origin is bottom-left; flip once so every coordinate below is
        // the sketch's own Y-down value.
        let _ = writeln!(content, "1 0 0 -1 0 {} cm", n(size.height));
        for item in self.items() {
            w.sketch(item, &mut content, 1.0);
        }
        w.finish(size, &content)
    }
}

#[derive(Default)]
struct SvgWriter {
    defs: String,
    next_id: usize,
}

impl SvgWriter {
    fn id(&mut self, prefix: &str) -> String {
        self.next_id += 1;
        format!("{prefix}{}", self.next_id)
    }

    fn paint(&mut self, brush: &Brush, bounds: Rect) -> (String, f32) {
        match brush {
            Brush::Solid(c) => (hex(*c), alpha(*c)),
            Brush::Gradient(g) => self.gradient(g, bounds),
        }
    }

    fn gradient(&mut self, g: &Gradient, bounds: Rect) -> (String, f32) {
        let stops = g
            .stops()
            .iter()
            .map(|s| {
                format!(
                    "      <stop offset=\"{}\" stop-color=\"{}\" stop-opacity=\"{}\"/>\n",
                    n(s.offset),
                    hex(s.color),
                    n(alpha(s.color))
                )
            })
            .collect::<String>();
        let (a, b) = g.resolve(bounds);
        let id = self.id("g");
        match g.geometry {
            GradientGeometry::Linear { .. } => {
                let _ = write!(
                    self.defs,
                    "    <linearGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\">\n{stops}    </linearGradient>\n",
                    n(a.dx), n(a.dy), n(b.dx), n(b.dy)
                );
            }
            GradientGeometry::Radial { .. } => {
                let r = (b.dx - a.dx).abs();
                let _ = write!(
                    self.defs,
                    "    <radialGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" cx=\"{}\" cy=\"{}\" r=\"{}\">\n{stops}    </radialGradient>\n",
                    n(a.dx), n(a.dy), n(r)
                );
            }
            GradientGeometry::Sweep { .. } => {
                let c = g.representative();
                return (hex(c), alpha(c));
            }
        }
        (format!("url(#{id})"), 1.0)
    }

    fn sketch(&mut self, item: &Sketch, out: &mut String, depth: usize) {
        let pad = "  ".repeat(depth);
        match item {
            Sketch::Fill { path, brush } => {
                let (fill, a) = self.paint(brush, bounds_of(path));
                let _ = write!(
                    out,
                    "{pad}<path d=\"{}\" fill=\"{fill}\"",
                    path.to_svg_data()
                );
                if a < 1.0 {
                    let _ = write!(out, " fill-opacity=\"{}\"", n(a));
                }
                out.push_str("/>\n");
            }
            Sketch::Stroke {
                path,
                brush,
                width,
                style,
            } => {
                let (stroke, a) = self.paint(brush, bounds_of(path));
                let _ = write!(
                    out,
                    "{pad}<path d=\"{}\" fill=\"none\" stroke=\"{stroke}\" stroke-width=\"{}\"",
                    path.to_svg_data(),
                    n(*width)
                );
                if a < 1.0 {
                    let _ = write!(out, " stroke-opacity=\"{}\"", n(a));
                }
                svg_stroke_style(out, style);
                out.push_str("/>\n");
            }
            Sketch::Shadow {
                rect,
                radius,
                shadow,
            } => {
                if shadow.is_inset {
                    return;
                }
                let id = self.id("s");
                let _ = writeln!(
                    self.defs,
                    "    <filter id=\"{id}\" x=\"-50%\" y=\"-50%\" width=\"200%\" height=\"200%\"><feGaussianBlur stdDeviation=\"{}\"/></filter>",
                    n(shadow.std_dev())
                );
                let r = rect.inflate(shadow.spread).translate(shadow.offset);
                let path = Path::rounded_rect(r, *radius);
                let _ = writeln!(
                    out,
                    "{pad}<path d=\"{}\" fill=\"{}\" fill-opacity=\"{}\" filter=\"url(#{id})\"/>",
                    path.to_svg_data(),
                    hex(shadow.color),
                    n(alpha(shadow.color))
                );
            }
            Sketch::Layer {
                alpha,
                blur,
                blend,
                clip,
                children,
            } => {
                let _ = write!(out, "{pad}<g");
                if *alpha < 1.0 {
                    let _ = write!(out, " opacity=\"{}\"", n(*alpha));
                }
                if let Some(mode) = css_blend(*blend) {
                    let _ = write!(out, " style=\"mix-blend-mode:{mode}\"");
                }
                if let Some(clip) = clip {
                    let id = self.id("c");
                    let _ = writeln!(
                        self.defs,
                        "    <clipPath id=\"{id}\"><path d=\"{}\"/></clipPath>",
                        clip.to_svg_data()
                    );
                    let _ = write!(out, " clip-path=\"url(#{id})\"");
                }
                if *blur > 0.0 {
                    let id = self.id("b");
                    let _ = writeln!(
                        self.defs,
                        "    <filter id=\"{id}\" x=\"-50%\" y=\"-50%\" width=\"200%\" height=\"200%\"><feGaussianBlur stdDeviation=\"{}\"/></filter>",
                        n(*blur)
                    );
                    let _ = write!(out, " filter=\"url(#{id})\"");
                }
                out.push_str(">\n");
                for child in children {
                    self.sketch(child, out, depth + 1);
                }
                let _ = writeln!(out, "{pad}</g>");
            }
            Sketch::Image { rect, image } => {
                let _ = writeln!(
                    out,
                    "{pad}<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\" href=\"data:image/png;base64,{}\"/>",
                    n(rect.left),
                    n(rect.top),
                    n(rect.width()),
                    n(rect.height()),
                    base64(&png_stored(image))
                );
            }
            Sketch::Transformed {
                transform,
                children,
            } => {
                let t = transform;
                let _ = writeln!(
                    out,
                    "{pad}<g transform=\"matrix({} {} {} {} {} {})\">",
                    n(t.a),
                    n(t.b),
                    n(t.c),
                    n(t.d),
                    n(t.tx),
                    n(t.ty)
                );
                for child in children {
                    self.sketch(child, out, depth + 1);
                }
                let _ = writeln!(out, "{pad}</g>");
            }
        }
    }
}

fn svg_stroke_style(out: &mut String, style: &StrokeStyle) {
    match style.cap {
        StrokeCap::Butt => {}
        StrokeCap::Round => out.push_str(" stroke-linecap=\"round\""),
        StrokeCap::Square => out.push_str(" stroke-linecap=\"square\""),
    }
    match style.join {
        StrokeJoin::Miter => {
            let _ = write!(
                out,
                " stroke-miterlimit=\"{}\"",
                n(style.effective_miter_limit())
            );
        }
        StrokeJoin::Round => out.push_str(" stroke-linejoin=\"round\""),
        StrokeJoin::Bevel => out.push_str(" stroke-linejoin=\"bevel\""),
    }
    if let Some(dash) = &style.dash {
        if !dash.is_solid() {
            let list: Vec<String> = dash.pattern.iter().map(|v| n(*v)).collect();
            let _ = write!(
                out,
                " stroke-dasharray=\"{}\" stroke-dashoffset=\"{}\"",
                list.join(" "),
                n(dash.offset)
            );
        }
    }
}

#[derive(Default)]
struct PdfWriter {
    /// Extra indirect objects (shadings, functions), each already rendered.
    objects: Vec<String>,
    /// `/GSn` → dictionary body.
    states: Vec<String>,
    /// `/Shn` → object number (1-based index into `objects`, offset later).
    shadings: Vec<usize>,
    blend: Vec<Option<&'static str>>,
}

fn pdf_path(path: &Path, out: &mut String) {
    for verb in path.verbs() {
        match *verb {
            PathVerb::MoveTo(p) => {
                let _ = writeln!(out, "{} {} m", n(p.dx), n(p.dy));
            }
            PathVerb::LineTo(p) => {
                let _ = writeln!(out, "{} {} l", n(p.dx), n(p.dy));
            }
            PathVerb::CubicTo(a, b, p) => {
                let _ = writeln!(
                    out,
                    "{} {} {} {} {} {} c",
                    n(a.dx),
                    n(a.dy),
                    n(b.dx),
                    n(b.dy),
                    n(p.dx),
                    n(p.dy)
                );
            }
            PathVerb::Close => out.push_str("h\n"),
        }
    }
}

fn rgb(c: Color) -> String {
    let c = c.to_srgb();
    format!(
        "{} {} {}",
        n(f32::from(c.r) / 255.0),
        n(f32::from(c.g) / 255.0),
        n(f32::from(c.b) / 255.0)
    )
}

impl PdfWriter {
    /// A graphics state for fill+stroke alpha `a` (and the active blend).
    fn state(&mut self, a: f32, blend: Option<&'static str>) -> String {
        let bm = blend.map(|b| format!(" /BM /{b}")).unwrap_or_default();
        let body = format!("<< /ca {} /CA {}{bm} >>", n(a), n(a));
        if let Some(i) = self.states.iter().position(|s| *s == body) {
            return format!("/GS{i}");
        }
        self.states.push(body);
        self.blend.push(blend);
        format!("/GS{}", self.states.len() - 1)
    }

    fn shading(&mut self, g: &Gradient, bounds: Rect) -> Option<String> {
        let stops = g.stops();
        if stops.is_empty() {
            return None;
        }
        let (a, b) = g.resolve(bounds);
        // One exponential (Type 2) function per stop pair, stitched (Type 3).
        let mut funcs = Vec::new();
        for pair in stops.windows(2) {
            funcs.push(format!(
                "<< /FunctionType 2 /Domain [0 1] /C0 [{}] /C1 [{}] /N 1 >>",
                rgb(pair[0].color),
                rgb(pair[1].color)
            ));
        }
        let function = if funcs.is_empty() {
            format!(
                "<< /FunctionType 2 /Domain [0 1] /C0 [{}] /C1 [{}] /N 1 >>",
                rgb(stops[0].color),
                rgb(stops[0].color)
            )
        } else if funcs.len() == 1 {
            funcs.remove(0)
        } else {
            let bounds_list: Vec<String> = stops[1..stops.len() - 1]
                .iter()
                .map(|s| n(s.offset))
                .collect();
            let encode: Vec<&str> = funcs.iter().map(|_| "0 1").collect();
            format!(
                "<< /FunctionType 3 /Domain [{} {}] /Functions [{}] /Bounds [{}] /Encode [{}] >>",
                n(stops[0].offset),
                n(stops[stops.len() - 1].offset),
                funcs.join(" "),
                bounds_list.join(" "),
                encode.join(" ")
            )
        };
        let dict = match g.geometry {
            GradientGeometry::Linear { .. } => format!(
                "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [{} {} {} {}] /Function {function} /Extend [true true] >>",
                n(a.dx), n(a.dy), n(b.dx), n(b.dy)
            ),
            GradientGeometry::Radial { .. } => format!(
                "<< /ShadingType 3 /ColorSpace /DeviceRGB /Coords [{} {} 0 {} {} {}] /Function {function} /Extend [true true] >>",
                n(a.dx), n(a.dy), n(a.dx), n(a.dy), n((b.dx - a.dx).abs())
            ),
            GradientGeometry::Sweep { .. } => return None,
        };
        self.objects.push(dict);
        self.shadings.push(self.objects.len());
        Some(format!("/Sh{}", self.shadings.len() - 1))
    }

    fn fill(&mut self, path: &Path, brush: &Brush, alpha_mul: f32, out: &mut String) {
        match brush {
            Brush::Solid(c) => {
                let gs = self.state(alpha(*c) * alpha_mul, None);
                let _ = writeln!(out, "q {gs} gs {} rg", rgb(*c));
                pdf_path(path, out);
                out.push_str("f Q\n");
            }
            Brush::Gradient(g) => {
                if let Some(sh) = self.shading(g, path.bounds()) {
                    let avg = g.stops().iter().map(|s| alpha(s.color)).sum::<f32>()
                        / g.stops().len().max(1) as f32;
                    let gs = self.state(avg * alpha_mul, None);
                    let _ = writeln!(out, "q {gs} gs");
                    pdf_path(path, out);
                    let _ = writeln!(out, "W n {sh} sh Q");
                } else {
                    self.fill(path, &Brush::Solid(g.representative()), alpha_mul, out);
                }
            }
        }
    }

    fn sketch(&mut self, item: &Sketch, out: &mut String, alpha_mul: f32) {
        match item {
            Sketch::Fill { path, brush } => self.fill(path, brush, alpha_mul, out),
            Sketch::Stroke {
                path,
                brush,
                width,
                style,
            } => {
                // A gradient stroke is the stroke's outline filled with the
                // gradient: PDF strokes take only a colour.
                if let Brush::Gradient(_) = brush {
                    let outline = path.stroke_outline(*width);
                    self.fill(&outline, brush, alpha_mul, out);
                    return;
                }
                let Brush::Solid(c) = brush else { return };
                let gs = self.state(alpha(*c) * alpha_mul, None);
                let cap = match style.cap {
                    StrokeCap::Butt => 0,
                    StrokeCap::Round => 1,
                    StrokeCap::Square => 2,
                };
                let join = match style.join {
                    StrokeJoin::Miter => 0,
                    StrokeJoin::Round => 1,
                    StrokeJoin::Bevel => 2,
                };
                let _ = write!(
                    out,
                    "q {gs} gs {} RG {} w {cap} J {join} j {} M",
                    rgb(*c),
                    n(*width),
                    n(style.effective_miter_limit())
                );
                if let Some(dash) = style.dash.as_ref().filter(|d| !d.is_solid()) {
                    let list: Vec<String> = dash.pattern.iter().map(|v| n(*v)).collect();
                    let _ = write!(out, " [{}] {} d", list.join(" "), n(dash.offset));
                }
                out.push('\n');
                pdf_path(path, out);
                out.push_str("S Q\n");
            }
            Sketch::Shadow {
                rect,
                radius,
                shadow,
            } => {
                if shadow.is_inset {
                    return;
                }
                let r = rect.inflate(shadow.spread).translate(shadow.offset);
                self.fill(
                    &Path::rounded_rect(r, *radius),
                    &Brush::Solid(shadow.color),
                    alpha_mul,
                    out,
                );
            }
            Sketch::Layer {
                alpha,
                blend,
                clip,
                children,
                ..
            } => {
                out.push_str("q\n");
                if let Some(mode) = pdf_blend(*blend) {
                    let gs = self.state(1.0, Some(mode));
                    let _ = writeln!(out, "{gs} gs");
                }
                if let Some(clip) = clip {
                    pdf_path(clip, out);
                    out.push_str("W n\n");
                }
                for child in children {
                    self.sketch(child, out, alpha_mul * alpha);
                }
                out.push_str("Q\n");
            }
            Sketch::Image { rect, image } => {
                // An inline image (no resources needed), ASCII-hex encoded so
                // the content stream stays text. PDF inline images carry no
                // soft mask, so alpha is flattened onto white here.
                let (w, h) = (image.width(), image.height());
                let _ = writeln!(
                    out,
                    "q {} 0 0 {} {} {} cm BI /W {w} /H {h} /CS /RGB /BPC 8 /F /AHx ID",
                    n(rect.width()),
                    n(-rect.height()),
                    n(rect.left),
                    n(rect.bottom)
                );
                for p in image.pixels().as_chunks::<4>().0 {
                    let a = u32::from(p[3]);
                    for c in &p[..3] {
                        let v = (u32::from(*c) * a + 255 * (255 - a)) / 255;
                        let _ = write!(out, "{v:02x}");
                    }
                }
                out.push_str(">\nEI Q\n");
            }
            Sketch::Transformed {
                transform,
                children,
            } => {
                let Transform { a, b, c, d, tx, ty } = *transform;
                let _ = writeln!(
                    out,
                    "q {} {} {} {} {} {} cm",
                    n(a),
                    n(b),
                    n(c),
                    n(d),
                    n(tx),
                    n(ty)
                );
                for child in children {
                    self.sketch(child, out, alpha_mul);
                }
                out.push_str("Q\n");
            }
        }
    }

    fn finish(self, size: Size, content: &str) -> Vec<u8> {
        // Object layout: 1 catalog, 2 pages, 3 page, 4 content, 5.. extras.
        let extra_base = 5;
        let mut resources = String::from("<< ");
        if !self.states.is_empty() {
            resources.push_str("/ExtGState << ");
            for (i, s) in self.states.iter().enumerate() {
                let _ = write!(resources, "/GS{i} {s} ");
            }
            resources.push_str(">> ");
        }
        if !self.shadings.is_empty() {
            resources.push_str("/Shading << ");
            for (i, obj) in self.shadings.iter().enumerate() {
                let _ = write!(resources, "/Sh{i} {} 0 R ", extra_base + obj - 1);
            }
            resources.push_str(">> ");
        }
        resources.push_str(">>");

        let mut objects = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources {resources} /Contents 4 0 R >>",
                n(size.width),
                n(size.height)
            ),
            format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        ];
        objects.extend(self.objects);

        let mut out = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
        let mut offsets = Vec::new();
        for (i, body) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
        }
        let xref = out.len();
        let mut table = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
        for off in offsets {
            let _ = writeln!(table, "{off:010} 00000 n ");
        }
        let _ = write!(
            table,
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        );
        out.extend_from_slice(table.as_bytes());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Offset, Shadow};

    fn sample() -> Sketchbook {
        let mut book = Sketchbook::new();
        book.rect(Rect::new(0.0, 0.0, 100.0, 50.0), Color::rgb(10, 20, 30));
        book.fill(
            Path::rect(Rect::new(10.0, 10.0, 90.0, 40.0)),
            Gradient::horizontal().with_stops(&[
                (0.0, Color::RED),
                (0.5, Color::GREEN),
                (1.0, Color::BLUE),
            ]),
        );
        book.stroke_styled(
            Path::arc(Offset::new(50.0, 25.0), 20.0, 0.0, std::f32::consts::TAU),
            Color::WHITE.with_alpha(128),
            3.0,
            StrokeStyle::rounded(),
        );
        book.layer(
            0.5,
            2.0,
            Some(Path::rect(Rect::new(0.0, 0.0, 50.0, 50.0))),
            |g| {
                g.circle(Offset::new(25.0, 25.0), 10.0, Color::BLUE);
            },
        );
        book.shadow(
            Rect::new(20.0, 20.0, 40.0, 40.0),
            4.0,
            Shadow {
                color: Color::BLACK.with_alpha(100),
                offset: Offset::new(2.0, 2.0),
                blur: 6.0,
                spread: 0.0,
                is_inset: false,
            },
        );
        book
    }

    #[test]
    fn svg_carries_every_feature() {
        let svg = sample().to_svg(Size::new(100.0, 50.0));
        assert!(svg.contains("<linearGradient"));
        assert_eq!(svg.matches("<stop").count(), 3);
        assert!(svg.contains("stroke-linecap=\"round\""));
        assert!(svg.contains("stroke-opacity=\"0.502\""));
        assert!(svg.contains("clip-path=\"url(#"));
        assert!(svg.contains("opacity=\"0.5\""));
        assert!(svg.contains("feGaussianBlur"));
        assert!(svg.trim_end().ends_with("</svg>"));
    }

    #[test]
    fn pdf_is_structurally_valid() {
        let pdf = sample().to_pdf(Size::new(100.0, 50.0));
        // The binary comment on line two is the only non-ASCII; blank it so
        // byte offsets and string offsets agree.
        let text: String = pdf
            .iter()
            .map(|&b| if b < 128 { b as char } else { '?' })
            .collect();
        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.trim_end().ends_with("%%EOF"));
        // The xref offsets point at the objects they claim to.
        let xref_at: usize = text
            .rsplit("startxref\n")
            .next()
            .and_then(|s| s.lines().next())
            .and_then(|s| s.parse().ok())
            .unwrap();
        assert!(text[xref_at..].starts_with("xref"));
        for (i, line) in text[xref_at..].lines().skip(3).take(4).enumerate() {
            let off: usize = line[..10].parse().unwrap();
            assert!(
                text[off..].starts_with(&format!("{} 0 obj", i + 1)),
                "object {}",
                i + 1
            );
        }
        assert!(text.contains("/ShadingType 2"));
        assert!(
            text.contains("/FunctionType 3"),
            "three stops stitch two functions"
        );
        assert!(text.contains(" re") || text.contains(" m\n"));
        // Content stream length is exact.
        let len: usize = text
            .split("/Length ")
            .nth(1)
            .unwrap()
            .split(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let start = text.find("stream\n").unwrap() + 7;
        let end = text.find("endstream").unwrap();
        assert_eq!(end - start, len);
    }
}
