//! A live-coded video synthesiser — Hydra's language (§2 Creative Coding),
//! TouchDesigner's TOP chains, a shader toy.
//!
//! Programs are written the way Hydra writes them — a **source** followed by
//! chained **transforms**, any argument a number or a nested chain:
//!
//! ```text
//! osc(20, 0.1, 0.8).kaleid(5).rotate(0.3, 0.1).modulate(noise(3), 0.2).out()
//! ```
//!
//! [`Synth::parse`] compiles the text to a tree; [`Synth::render`] evaluates
//! it per pixel at a time `t` into an RGBA image. Coordinates run `0..1`
//! across the frame (aspect-corrected about the centre), time in seconds —
//! the same space and the same functions as Hydra, so a sketch carries over:
//!
//! | kind | functions |
//! |---|---|
//! | sources | `osc(freq, sync, offset)`, `noise(scale, speed)`, `voronoi(scale, speed, blend)`, `shape(sides, radius, smooth)`, `gradient(speed)`, `solid(r, g, b)` |
//! | geometry | `rotate(angle, speed)`, `scale(amount)`, `pixelate(x, y)`, `repeat(x, y)`, `kaleid(n)`, `scroll(x, y, sx, sy)` |
//! | colour | `color(r, g, b)`, `invert()`, `contrast(a)`, `brightness(a)`, `saturate(a)`, `luma(threshold, tolerance)`, `posterize(bins)`, `hue(shift)` |
//! | blending | `add(src, a)`, `mult(src, a)`, `diff(src)`, `blend(src, a)`, `mask(src)` |
//! | modulation | `modulate(src, a)`, `modulateRotate(src, a)`, `modulateScale(src, a)` |
//!
//! `.out()` is accepted and ignored (there is one output). Unknown names and
//! wrong arities are parse errors with a byte position, so a live-coding
//! editor can underline them.

use vieww_foundation::Image;

/// A parse failure at byte `at`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SynthError {
    pub at: usize,
    pub message: String,
}

impl std::fmt::Display for SynthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at byte {}", self.message, self.at)
    }
}

impl std::error::Error for SynthError {}

#[derive(Debug, Clone, PartialEq)]
enum Arg {
    Num(f32),
    Chain(Box<Node>),
}

#[derive(Debug, Clone, PartialEq)]
struct Call {
    name: String,
    args: Vec<Arg>,
}

#[derive(Debug, Clone, PartialEq)]
struct Node {
    source: Call,
    chain: Vec<Call>,
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && (self.s[self.i] as char).is_whitespace() {
            self.i += 1;
        }
    }
    fn err<T>(&self, m: &str) -> Result<T, SynthError> {
        Err(SynthError {
            at: self.i,
            message: m.to_owned(),
        })
    }
    fn ident(&mut self) -> Result<String, SynthError> {
        self.ws();
        let st = self.i;
        while self.i < self.s.len() && (self.s[self.i].is_ascii_alphanumeric() || self.s[self.i] == b'_') {
            self.i += 1;
        }
        if st == self.i {
            return self.err("expected a function name");
        }
        Ok(String::from_utf8_lossy(&self.s[st..self.i]).into_owned())
    }
    fn expect(&mut self, c: u8) -> Result<(), SynthError> {
        self.ws();
        if self.s.get(self.i) == Some(&c) {
            self.i += 1;
            Ok(())
        } else {
            self.err(&format!("expected '{}'", c as char))
        }
    }
    fn call(&mut self) -> Result<Call, SynthError> {
        let name = self.ident()?;
        self.expect(b'(')?;
        let mut args = Vec::new();
        loop {
            self.ws();
            if self.s.get(self.i) == Some(&b')') {
                self.i += 1;
                break;
            }
            let c = self.s.get(self.i).copied().unwrap_or(0);
            if c.is_ascii_digit() || c == b'-' || c == b'.' {
                let st = self.i;
                self.i += 1;
                while self.i < self.s.len() && (self.s[self.i].is_ascii_digit() || self.s[self.i] == b'.' || self.s[self.i] == b'e') {
                    self.i += 1;
                }
                let t = std::str::from_utf8(&self.s[st..self.i]).unwrap_or("");
                match t.parse::<f32>() {
                    Ok(v) => args.push(Arg::Num(v)),
                    Err(_) => return self.err("bad number"),
                }
            } else {
                args.push(Arg::Chain(Box::new(self.node()?)));
            }
            self.ws();
            match self.s.get(self.i) {
                Some(b',') => self.i += 1,
                Some(b')') => {}
                _ => return self.err("expected ',' or ')'"),
            }
        }
        Ok(Call { name, args })
    }
    fn node(&mut self) -> Result<Node, SynthError> {
        let at = self.i;
        let source = self.call()?;
        if !SOURCES.iter().any(|(n, _)| *n == source.name) {
            return Err(SynthError {
                at,
                message: format!("'{}' is not a source", source.name),
            });
        }
        let mut chain = Vec::new();
        loop {
            self.ws();
            if self.s.get(self.i) != Some(&b'.') {
                break;
            }
            self.i += 1;
            let at = self.i;
            let c = self.call()?;
            if c.name == "out" {
                continue;
            }
            if !TRANSFORMS.iter().any(|(n, _)| *n == c.name) {
                return Err(SynthError {
                    at,
                    message: format!("unknown transform '{}'", c.name),
                });
            }
            chain.push(c);
        }
        Ok(node_checked(Node { source, chain }, at)?)
    }
}

const SOURCES: [(&str, usize); 6] = [
    ("osc", 3),
    ("noise", 2),
    ("voronoi", 3),
    ("shape", 3),
    ("gradient", 1),
    ("solid", 3),
];

const TRANSFORMS: [(&str, usize); 24] = [
    ("rotate", 2),
    ("scale", 1),
    ("pixelate", 2),
    ("repeat", 2),
    ("kaleid", 1),
    ("scroll", 4),
    ("color", 3),
    ("invert", 0),
    ("contrast", 1),
    ("brightness", 1),
    ("saturate", 1),
    ("luma", 2),
    ("posterize", 1),
    ("hue", 1),
    ("add", 2),
    ("mult", 2),
    ("diff", 1),
    ("blend", 2),
    ("mask", 1),
    ("modulate", 2),
    ("modulateRotate", 2),
    ("modulateScale", 2),
    ("thresh", 1),
    ("shift", 3),
];

fn node_checked(n: Node, at: usize) -> Result<Node, SynthError> {
    let max = |name: &str, table: &[(&str, usize)]| table.iter().find(|(k, _)| *k == name).map_or(0, |(_, a)| *a);
    if n.source.args.len() > max(&n.source.name, &SOURCES) {
        return Err(SynthError {
            at,
            message: format!("too many arguments to '{}'", n.source.name),
        });
    }
    for c in &n.chain {
        if c.args.len() > max(&c.name, &TRANSFORMS) {
            return Err(SynthError {
                at,
                message: format!("too many arguments to '{}'", c.name),
            });
        }
    }
    Ok(n)
}

/// A compiled program.
#[derive(Debug, Clone, PartialEq)]
pub struct Synth {
    root: Node,
}

type Rgba = [f32; 4];

fn hash2(x: f32, y: f32) -> f32 {
    let v = (x * 127.1 + y * 311.7).sin() * 43_758.547;
    v - v.floor()
}

fn vnoise(x: f32, y: f32, z: f32) -> f32 {
    let (ix, iy) = (x.floor(), y.floor());
    let (fx, fy) = (x - ix, y - iy);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let h = |a: f32, b: f32| hash2(a + z * 17.0, b - z * 13.0);
    let a = h(ix, iy) + (h(ix + 1.0, iy) - h(ix, iy)) * s(fx);
    let b = h(ix, iy + 1.0) + (h(ix + 1.0, iy + 1.0) - h(ix, iy + 1.0)) * s(fx);
    (a + (b - a) * s(fy)) * 2.0 - 1.0
}

impl Synth {
    /// Compile a program.
    ///
    /// # Errors
    /// Syntax errors, unknown functions and too many arguments, with positions.
    pub fn parse(text: &str) -> Result<Self, SynthError> {
        let mut p = Parser { s: text.as_bytes(), i: 0 };
        let root = p.node()?;
        p.ws();
        if p.i != p.s.len() {
            return p.err("unexpected text after the program");
        }
        Ok(Self { root })
    }

    fn arg(args: &[Arg], i: usize, d: f32, uv: (f32, f32), t: f32) -> f32 {
        match args.get(i) {
            Some(Arg::Num(v)) => *v,
            Some(Arg::Chain(n)) => Self::eval(n, uv, t)[0],
            None => d,
        }
    }

    fn src(args: &[Arg], i: usize, uv: (f32, f32), t: f32) -> Rgba {
        match args.get(i) {
            Some(Arg::Chain(n)) => Self::eval(n, uv, t),
            Some(Arg::Num(v)) => [*v, *v, *v, 1.0],
            None => [0.0, 0.0, 0.0, 1.0],
        }
    }

    #[allow(clippy::too_many_lines)]
    fn eval(n: &Node, uv: (f32, f32), t: f32) -> Rgba {
        // Geometry transforms rewrite the coordinate before the source;
        // colour transforms rewrite the colour after. Hydra applies the
        // chain left to right, so coordinates are transformed in reverse.
        let mut st = uv;
        for c in n.chain.iter().rev() {
            let a = |i, d| Self::arg(&c.args, i, d, uv, t);
            match c.name.as_str() {
                "rotate" => {
                    let ang = a(0, 10.0) + a(1, 0.0) * t;
                    let (s, co) = ang.sin_cos();
                    let (x, y) = (st.0 - 0.5, st.1 - 0.5);
                    st = (co * x - s * y + 0.5, s * x + co * y + 0.5);
                }
                "scale" => {
                    let k = a(0, 1.5).max(1e-4);
                    st = ((st.0 - 0.5) / k + 0.5, (st.1 - 0.5) / k + 0.5);
                }
                "pixelate" => {
                    let (px, py) = (a(0, 20.0).max(1.0), a(1, 20.0).max(1.0));
                    st = ((st.0 * px).floor() / px + 0.5 / px, (st.1 * py).floor() / py + 0.5 / py);
                }
                "repeat" => {
                    st = ((st.0 * a(0, 3.0)).fract(), (st.1 * a(1, 3.0)).fract());
                }
                "kaleid" => {
                    let k = a(0, 4.0).max(1.0);
                    let (x, y) = (st.0 - 0.5, st.1 - 0.5);
                    let mut ang = y.atan2(x);
                    let r = (x * x + y * y).sqrt();
                    let seg = std::f32::consts::TAU / k;
                    ang = ang.rem_euclid(seg);
                    ang = (ang - seg * 0.5).abs();
                    st = (r * ang.cos() + 0.5, r * ang.sin() + 0.5);
                }
                "scroll" => {
                    st = (st.0 + a(0, 0.5) + a(2, 0.0) * t, st.1 + a(1, 0.5) + a(3, 0.0) * t);
                }
                "modulate" => {
                    let m = Self::src(&c.args, 0, uv, t);
                    let k = a(1, 0.1);
                    st = (st.0 + m[0] * k, st.1 + m[1] * k);
                }
                "modulateRotate" => {
                    let m = Self::src(&c.args, 0, uv, t);
                    let ang = m[0] * a(1, 1.0);
                    let (s, co) = ang.sin_cos();
                    let (x, y) = (st.0 - 0.5, st.1 - 0.5);
                    st = (co * x - s * y + 0.5, s * x + co * y + 0.5);
                }
                "modulateScale" => {
                    let m = Self::src(&c.args, 0, uv, t);
                    let k = (1.0 + m[0] * a(1, 1.0)).max(1e-3);
                    st = ((st.0 - 0.5) / k + 0.5, (st.1 - 0.5) / k + 0.5);
                }
                _ => {}
            }
        }
        let s = &n.source;
        let a = |i, d| Self::arg(&s.args, i, d, uv, t);
        let mut c: Rgba = match s.name.as_str() {
            "osc" => {
                // Hydra's definition, verbatim in Rust.
                let (f, sync, off) = (a(0, 60.0), a(1, 0.1), a(2, 0.0));
                let f = if f.abs() < 1e-6 { 1e-6 } else { f };
                let wave = |p: f32| (p * f).sin() * 0.5 + 0.5;
                [
                    wave(st.0 - off / f + t * sync),
                    wave(st.0 + t * sync),
                    wave(st.0 + off / f + t * sync),
                    1.0,
                ]
            }
            "noise" => {
                let v = vnoise(st.0 * a(0, 10.0), st.1 * a(0, 10.0), t * a(1, 0.1)) * 0.5 + 0.5;
                [v, v, v, 1.0]
            }
            "voronoi" => {
                let (sc, sp, bl) = (a(0, 5.0), a(1, 0.3), a(2, 0.3));
                let (x, y) = (st.0 * sc, st.1 * sc);
                let (ix, iy) = (x.floor(), y.floor());
                let mut best = 9.0f32;
                let mut cell = 0.0;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        #[allow(clippy::cast_precision_loss)]
                        let (cx, cy) = (ix + dx as f32, iy + dy as f32);
                        let px = cx + 0.5 + 0.4 * (t * sp + hash2(cx, cy) * 6.28).sin();
                        let py = cy + 0.5 + 0.4 * (t * sp + hash2(cy, cx) * 6.28).cos();
                        let d = ((x - px).powi(2) + (y - py).powi(2)).sqrt();
                        if d < best {
                            best = d;
                            cell = hash2(cx, cy);
                        }
                    }
                }
                let v = best * (1.0 - bl) + cell * bl;
                [v, v, v, 1.0]
            }
            "shape" => {
                let (sides, r, sm) = (a(0, 3.0).max(3.0), a(1, 0.3), a(2, 0.01).max(1e-4));
                let (x, y) = (st.0 * 2.0 - 1.0, st.1 * 2.0 - 1.0);
                let ang = x.atan2(y) + std::f32::consts::PI;
                let seg = std::f32::consts::TAU / sides;
                let d = ((ang / seg + 0.5).floor() * seg - ang).cos() * (x * x + y * y).sqrt();
                let v = 1.0 - ((d - r) / sm).clamp(0.0, 1.0);
                [v, v, v, 1.0]
            }
            "gradient" => {
                let sp = a(0, 0.0);
                [st.0, st.1, (0.5 + 0.5 * (t * sp).sin()).clamp(0.0, 1.0), 1.0]
            }
            "solid" => [a(0, 0.0), a(1, 0.0), a(2, 0.0), 1.0],
            _ => [0.0, 0.0, 0.0, 1.0],
        };
        for k in &n.chain {
            let a = |i, d| Self::arg(&k.args, i, d, uv, t);
            match k.name.as_str() {
                "color" => {
                    c = [c[0] * a(0, 1.0), c[1] * a(1, 1.0), c[2] * a(2, 1.0), c[3]];
                }
                "invert" => c = [1.0 - c[0], 1.0 - c[1], 1.0 - c[2], c[3]],
                "contrast" => {
                    let k = a(0, 1.6);
                    c = [(c[0] - 0.5) * k + 0.5, (c[1] - 0.5) * k + 0.5, (c[2] - 0.5) * k + 0.5, c[3]];
                }
                "brightness" => {
                    let k = a(0, 0.4);
                    c = [c[0] + k, c[1] + k, c[2] + k, c[3]];
                }
                "saturate" => {
                    let k = a(0, 2.0);
                    let l = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
                    c = [l + (c[0] - l) * k, l + (c[1] - l) * k, l + (c[2] - l) * k, c[3]];
                }
                "luma" => {
                    let l = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
                    let (th, tol) = (a(0, 0.5), a(1, 0.1).max(1e-4));
                    let alpha = ((l - th) / tol + 0.5).clamp(0.0, 1.0);
                    c = [c[0] * alpha, c[1] * alpha, c[2] * alpha, alpha];
                }
                "thresh" => {
                    let l = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
                    let v = if l > a(0, 0.5) { 1.0 } else { 0.0 };
                    c = [v, v, v, c[3]];
                }
                "posterize" => {
                    let b = a(0, 3.0).max(1.0);
                    c = [(c[0] * b).floor() / b, (c[1] * b).floor() / b, (c[2] * b).floor() / b, c[3]];
                }
                "hue" | "shift" => {
                    let h = a(0, 0.4) * std::f32::consts::TAU;
                    let (s, co) = h.sin_cos();
                    // Rotation about the grey axis.
                    let k = (1.0 - co) / 3.0;
                    let sq = s / 3f32.sqrt();
                    let m = [[co + k, k - sq, k + sq], [k + sq, co + k, k - sq], [k - sq, k + sq, co + k]];
                    c = [
                        m[0][0] * c[0] + m[0][1] * c[1] + m[0][2] * c[2],
                        m[1][0] * c[0] + m[1][1] * c[1] + m[1][2] * c[2],
                        m[2][0] * c[0] + m[2][1] * c[1] + m[2][2] * c[2],
                        c[3],
                    ];
                }
                "add" => {
                    let o = Self::src(&k.args, 0, uv, t);
                    let w = a(1, 1.0);
                    c = [c[0] + o[0] * w, c[1] + o[1] * w, c[2] + o[2] * w, c[3]];
                }
                "mult" => {
                    let o = Self::src(&k.args, 0, uv, t);
                    let w = a(1, 1.0);
                    c = [0, 1, 2, 3].map(|i| c[i] * (1.0 - w) + c[i] * o[i] * w);
                }
                "diff" => {
                    let o = Self::src(&k.args, 0, uv, t);
                    c = [(c[0] - o[0]).abs(), (c[1] - o[1]).abs(), (c[2] - o[2]).abs(), c[3]];
                }
                "blend" => {
                    let o = Self::src(&k.args, 0, uv, t);
                    let w = a(1, 0.5);
                    c = [0, 1, 2, 3].map(|i| c[i] + (o[i] - c[i]) * w);
                }
                "mask" => {
                    let o = Self::src(&k.args, 0, uv, t);
                    let l = 0.2126 * o[0] + 0.7152 * o[1] + 0.0722 * o[2];
                    c = [c[0] * l, c[1] * l, c[2] * l, c[3] * l];
                }
                _ => {}
            }
        }
        c
    }

    /// Render a frame at `t` seconds.
    #[must_use]
    pub fn render(&self, width: u32, height: u32, t: f32) -> Image {
        let (w, h) = (width as usize, height as usize);
        let mut px = Vec::with_capacity(w * h * 4);
        #[allow(clippy::cast_precision_loss)]
        let aspect = w as f32 / h.max(1) as f32;
        for y in 0..h {
            for x in 0..w {
                #[allow(clippy::cast_precision_loss)]
                let uv = (
                    ((x as f32 + 0.5) / w as f32 - 0.5) * aspect + 0.5,
                    (y as f32 + 0.5) / h as f32,
                );
                let c = Self::eval(&self.root, uv, t);
                for v in c {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    px.push((v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
                }
            }
        }
        Image::from_rgba8(px, width, height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hydra_programs_parse_and_render() {
        for p in [
            "osc(20, 0.1, 0.8).out()",
            "osc(10).kaleid(5).rotate(0.3, 0.1).modulate(noise(3), 0.2).out()",
            "voronoi(8, 0.5, 0.2).color(1, 0.4, 0.8).add(shape(4, 0.3, 0.01), 0.5)",
            "shape(6).repeat(3, 3).scroll(0.1, 0, 0.2, 0).invert()",
            "gradient(1).posterize(4).hue(0.25).saturate(1.5).contrast(1.2)",
            "solid(1, 0, 0).blend(solid(0, 0, 1), 0.5).mask(shape(3, 0.4))",
            "noise(4).thresh(0.5).diff(osc(5)).pixelate(16, 16).luma(0.3, 0.2)",
            "osc(30).modulateRotate(noise(2), 2).modulateScale(osc(4), 0.5).brightness(0.1).mult(noise(5), 0.5)",
        ] {
            let s = Synth::parse(p).unwrap_or_else(|e| panic!("{p}: {e}"));
            let img = s.render(32, 18, 1.5);
            assert_eq!((img.width(), img.height()), (32, 18));
        }
    }

    #[test]
    fn time_animates_and_rendering_is_deterministic() {
        let s = Synth::parse("osc(10, 0.2).rotate(0, 0.5)").unwrap();
        let (a, b) = (s.render(24, 24, 0.0), s.render(24, 24, 1.0));
        assert_ne!(a.pixels(), b.pixels());
        assert_eq!(s.render(24, 24, 1.0).pixels(), b.pixels());
    }

    #[test]
    fn solid_and_blend_are_exact() {
        let img = Synth::parse("solid(1, 0, 0).blend(solid(0, 0, 1), 0.5)").unwrap().render(2, 2, 0.0);
        assert_eq!(&img.pixels()[..4], &[128, 0, 128, 255]);
        let inv = Synth::parse("solid(0.2, 0.4, 0.6).invert()").unwrap().render(1, 1, 0.0);
        assert_eq!(&inv.pixels()[..3], &[204, 153, 102]);
    }

    #[test]
    fn kaleid_is_mirror_symmetric() {
        let img = Synth::parse("gradient().kaleid(4)").unwrap().render(32, 32, 0.0);
        let p = |x: usize, y: usize| img.pixels()[(y * 32 + x) * 4];
        assert_eq!(p(5, 16), p(26, 16), "left-right mirror");
    }

    #[test]
    fn errors_say_where() {
        let e = Synth::parse("osc(10).wobble(3)").unwrap_err();
        assert!(e.message.contains("wobble"));
        assert_eq!(e.at, 8);
        assert!(Synth::parse("rotate(1)").unwrap_err().message.contains("not a source"));
        assert!(Synth::parse("osc(1, 2, 3, 4)").unwrap_err().message.contains("too many"));
        assert!(Synth::parse("osc(1").is_err());
        assert!(Synth::parse("osc(1) junk").is_err());
    }
}
