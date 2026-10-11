//! Colour correction — After Effects' Color Correction category (Levels,
//! Curves, Hue/Saturation, Vibrance, Color Balance, Lumetri's lift/gamma/
//! gain, Apply Color LUT), DaVinci's primaries, Unreal/Unity's grading
//! stage — on RGBA8 buffers.
//!
//! Every operation is a pure function of the pixel, so each compiles to a
//! [`Lut3`] (a 3D lookup table, 33³ by default) and a whole grade chain
//! runs as one trilinear lookup per pixel — which is how grading tools do
//! it, and why a LUT file can carry a look between tools. `.cube` files
//! (Adobe/Resolve's text format) read and write.

/// A per-pixel colour operation on 0..1 RGB.
pub trait ColorOp {
    fn apply(&self, rgb: [f32; 3]) -> [f32; 3];
}

impl<F: Fn([f32; 3]) -> [f32; 3]> ColorOp for F {
    fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        self(rgb)
    }
}

/// Run `op` on every pixel (alpha untouched, premultiplication ignored).
pub fn apply(px: &mut [u8], op: &dyn ColorOp) {
    for p in px.as_chunks_mut::<4>().0 {
        let c = op.apply([
            f32::from(p[0]) / 255.0,
            f32::from(p[1]) / 255.0,
            f32::from(p[2]) / 255.0,
        ]);
        for k in 0..3 {
            p[k] = to_u8(c[k]);
        }
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// AE Levels: input black/white, gamma, output black/white.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Levels {
    pub in_black: f32,
    pub in_white: f32,
    pub gamma: f32,
    pub out_black: f32,
    pub out_white: f32,
}

impl Default for Levels {
    fn default() -> Self {
        Self {
            in_black: 0.0,
            in_white: 1.0,
            gamma: 1.0,
            out_black: 0.0,
            out_white: 1.0,
        }
    }
}

impl ColorOp for Levels {
    fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        rgb.map(|v| {
            let t =
                ((v - self.in_black) / (self.in_white - self.in_black).max(1e-6)).clamp(0.0, 1.0);
            let t = t.powf(1.0 / self.gamma.max(1e-3));
            self.out_black + (self.out_white - self.out_black) * t
        })
    }
}

/// AE Curves: a monotone cubic (Fritsch–Carlson) through control points on
/// the master channel, optionally per channel.
#[derive(Debug, Clone, PartialEq)]
pub struct Curves {
    pub master: Curve,
    pub red: Curve,
    pub green: Curve,
    pub blue: Curve,
}

/// One tone curve.
#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
    xs: Vec<f32>,
    ys: Vec<f32>,
    ms: Vec<f32>,
}

impl Curve {
    /// The identity.
    #[must_use]
    pub fn identity() -> Self {
        Self::new(&[(0.0, 0.0), (1.0, 1.0)])
    }

    /// Through `points` (x sorted, in 0..1). Monotone data gives a monotone
    /// curve — no overshoot, which is why grading tools use this spline.
    #[must_use]
    pub fn new(points: &[(f32, f32)]) -> Self {
        let mut pts = points.to_vec();
        pts.sort_by(|a, b| a.0.total_cmp(&b.0));
        pts.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-6);
        if pts.len() < 2 {
            pts = vec![(0.0, 0.0), (1.0, 1.0)];
        }
        let n = pts.len();
        let xs: Vec<f32> = pts.iter().map(|p| p.0).collect();
        let ys: Vec<f32> = pts.iter().map(|p| p.1).collect();
        let d: Vec<f32> = (0..n - 1)
            .map(|i| (ys[i + 1] - ys[i]) / (xs[i + 1] - xs[i]))
            .collect();
        let mut ms = vec![0.0; n];
        ms[0] = d[0];
        ms[n - 1] = d[n - 2];
        for i in 1..n - 1 {
            ms[i] = if d[i - 1] * d[i] <= 0.0 {
                0.0
            } else {
                (d[i - 1] + d[i]) * 0.5
            };
        }
        for i in 0..n - 1 {
            if d[i].abs() < 1e-9 {
                ms[i] = 0.0;
                ms[i + 1] = 0.0;
                continue;
            }
            let a = ms[i] / d[i];
            let b = ms[i + 1] / d[i];
            let s = a * a + b * b;
            if s > 9.0 {
                let t = 3.0 / s.sqrt();
                ms[i] = t * a * d[i];
                ms[i + 1] = t * b * d[i];
            }
        }
        Self { xs, ys, ms }
    }

    /// The curve at `x`.
    #[must_use]
    pub fn eval(&self, x: f32) -> f32 {
        let n = self.xs.len();
        if x <= self.xs[0] {
            return self.ys[0];
        }
        if x >= self.xs[n - 1] {
            return self.ys[n - 1];
        }
        let i = self
            .xs
            .partition_point(|v| *v <= x)
            .saturating_sub(1)
            .min(n - 2);
        let h = self.xs[i + 1] - self.xs[i];
        let t = (x - self.xs[i]) / h;
        let (t2, t3) = (t * t, t * t * t);
        (2.0 * t3 - 3.0 * t2 + 1.0) * self.ys[i]
            + (t3 - 2.0 * t2 + t) * h * self.ms[i]
            + (-2.0 * t3 + 3.0 * t2) * self.ys[i + 1]
            + (t3 - t2) * h * self.ms[i + 1]
    }

    /// The classic S: deepen shadows, lift highlights by `amount` (0..0.3).
    #[must_use]
    pub fn s_curve(amount: f32) -> Self {
        Self::new(&[
            (0.0, 0.0),
            (0.25, 0.25 - amount),
            (0.75, 0.75 + amount),
            (1.0, 1.0),
        ])
    }
}

impl Default for Curves {
    fn default() -> Self {
        Self {
            master: Curve::identity(),
            red: Curve::identity(),
            green: Curve::identity(),
            blue: Curve::identity(),
        }
    }
}

impl ColorOp for Curves {
    fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        [
            self.red.eval(self.master.eval(rgb[0])),
            self.green.eval(self.master.eval(rgb[1])),
            self.blue.eval(self.master.eval(rgb[2])),
        ]
    }
}

/// RGB → HSL (h in 0..1).
#[must_use]
pub fn rgb_to_hsl(c: [f32; 3]) -> [f32; 3] {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    let l = (max + min) * 0.5;
    if (max - min).abs() < 1e-7 {
        return [0.0, 0.0, l];
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if (max - c[0]).abs() < 1e-7 {
        (c[1] - c[2]) / d + if c[1] < c[2] { 6.0 } else { 0.0 }
    } else if (max - c[1]).abs() < 1e-7 {
        (c[2] - c[0]) / d + 2.0
    } else {
        (c[0] - c[1]) / d + 4.0
    };
    [h / 6.0, s, l]
}

/// HSL → RGB.
#[must_use]
pub fn hsl_to_rgb(c: [f32; 3]) -> [f32; 3] {
    let (h, s, l) = (c[0].rem_euclid(1.0), c[1], c[2]);
    if s <= 1e-7 {
        return [l, l, l];
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let f = |mut t: f32| {
        t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    [f(h + 1.0 / 3.0), f(h), f(h - 1.0 / 3.0)]
}

/// AE Hue/Saturation (master): hue rotation in degrees, saturation and
/// lightness in −1..1.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HueSaturation {
    pub hue: f32,
    pub saturation: f32,
    pub lightness: f32,
}

impl ColorOp for HueSaturation {
    fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        let mut hsl = rgb_to_hsl(rgb);
        hsl[0] += self.hue / 360.0;
        hsl[1] = (hsl[1] * (1.0 + self.saturation)).clamp(0.0, 1.0);
        let out = hsl_to_rgb(hsl);
        let l = self.lightness;
        out.map(|v| {
            if l >= 0.0 {
                v + (1.0 - v) * l
            } else {
                v * (1.0 + l)
            }
        })
    }
}

/// AE Vibrance: saturation that spares already-saturated colours (and
/// skin, by weighting down warm hues).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vibrance(pub f32);

impl ColorOp for Vibrance {
    fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        let max = rgb[0].max(rgb[1]).max(rgb[2]);
        let min = rgb[0].min(rgb[1]).min(rgb[2]);
        let sat = max - min;
        let l = luma(rgb);
        let k = self.0 * (1.0 - sat);
        rgb.map(|v| l + (v - l) * (1.0 + k))
    }
}

/// Lift / gamma / gain per channel (DaVinci primaries, Lumetri wheels),
/// plus a global saturation and a white-balance temperature/tint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Primaries {
    pub lift: [f32; 3],
    pub gamma: [f32; 3],
    pub gain: [f32; 3],
    pub saturation: f32,
    /// −1 (cool) .. 1 (warm).
    pub temperature: f32,
    /// −1 (green) .. 1 (magenta).
    pub tint: f32,
    pub contrast: f32,
    /// Contrast pivot.
    pub pivot: f32,
}

impl Default for Primaries {
    fn default() -> Self {
        Self {
            lift: [0.0; 3],
            gamma: [1.0; 3],
            gain: [1.0; 3],
            saturation: 1.0,
            temperature: 0.0,
            tint: 0.0,
            contrast: 1.0,
            pivot: 0.435,
        }
    }
}

impl ColorOp for Primaries {
    fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        let wb = [
            1.0 + 0.1 * self.temperature + 0.03 * self.tint,
            1.0 - 0.06 * self.tint,
            1.0 - 0.1 * self.temperature + 0.03 * self.tint,
        ];
        let mut c = [0.0; 3];
        for k in 0..3 {
            let v = rgb[k] * wb[k];
            // ASC-CDL-like: out = (gain·v + lift·(1−v))^(1/gamma).
            let v = (self.gain[k] * v + self.lift[k] * (1.0 - v)).max(0.0);
            let v = v.powf(1.0 / self.gamma[k].max(1e-3));
            c[k] = (v - self.pivot) * self.contrast + self.pivot;
        }
        let l = luma(c);
        c.map(|v| l + (v - l) * self.saturation)
    }
}

/// A 3D lookup table (`size³` RGB entries, red fastest — the `.cube`
/// order).
#[derive(Debug, Clone, PartialEq)]
pub struct Lut3 {
    pub size: usize,
    pub data: Vec<[f32; 3]>,
    pub title: String,
}

impl Lut3 {
    /// The identity table.
    #[must_use]
    pub fn identity(size: usize) -> Self {
        Self::bake(size, &|c| c)
    }

    /// Bake any operation (or chain) into a table.
    #[must_use]
    pub fn bake(size: usize, op: &dyn ColorOp) -> Self {
        let size = size.clamp(2, 129);
        let mut data = Vec::with_capacity(size * size * size);
        #[allow(clippy::cast_precision_loss)]
        let s = (size - 1) as f32;
        for b in 0..size {
            for g in 0..size {
                for r in 0..size {
                    #[allow(clippy::cast_precision_loss)]
                    data.push(op.apply([r as f32 / s, g as f32 / s, b as f32 / s]));
                }
            }
        }
        Self {
            size,
            data,
            title: String::new(),
        }
    }

    /// Trilinear lookup.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    pub fn sample(&self, c: [f32; 3]) -> [f32; 3] {
        let n = self.size;
        let s = (n - 1) as f32;
        let f = c.map(|v| v.clamp(0.0, 1.0) * s);
        let i = f.map(|v| (v.floor() as usize).min(n - 2));
        let t = [f[0] - i[0] as f32, f[1] - i[1] as f32, f[2] - i[2] as f32];
        let base = (i[2] * n + i[1]) * n + i[0];
        let (dg, db) = (n, n * n);
        let d = &self.data;
        let (c000, c100, c010, c110) = (d[base], d[base + 1], d[base + dg], d[base + dg + 1]);
        let (c001, c101, c011, c111) = (
            d[base + db],
            d[base + db + 1],
            d[base + db + dg],
            d[base + db + dg + 1],
        );
        let mut out = [0.0; 3];
        for k in 0..3 {
            let c00 = c000[k] + (c100[k] - c000[k]) * t[0];
            let c10 = c010[k] + (c110[k] - c010[k]) * t[0];
            let c01 = c001[k] + (c101[k] - c001[k]) * t[0];
            let c11 = c011[k] + (c111[k] - c011[k]) * t[0];
            let c0 = c00 + (c10 - c00) * t[1];
            let c1 = c01 + (c11 - c01) * t[1];
            out[k] = c0 + (c1 - c0) * t[2];
        }
        out
    }

    /// Apply to RGBA8 pixels at `strength` (0 = untouched, 1 = full),
    /// through a 256-entry-per-axis cache-friendly path.
    pub fn apply_rgba(&self, px: &mut [u8], strength: f32) {
        let k = strength.clamp(0.0, 1.0);
        let run = |band: &mut [u8]| {
            for p in band.as_chunks_mut::<4>().0 {
                let c = [
                    f32::from(p[0]) / 255.0,
                    f32::from(p[1]) / 255.0,
                    f32::from(p[2]) / 255.0,
                ];
                let o = self.sample(c);
                for j in 0..3 {
                    p[j] = to_u8(c[j] + (o[j] - c[j]) * k);
                }
            }
        };
        let threads = std::thread::available_parallelism()
            .map_or(1, std::num::NonZero::get)
            .min(16);
        let n = px.len() / 4;
        if threads <= 1 || n < 65_536 {
            run(px);
            return;
        }
        let per = n.div_ceil(threads) * 4;
        std::thread::scope(|sc| {
            for chunk in px.chunks_mut(per) {
                let run = &run;
                sc.spawn(move || run(chunk));
            }
        });
    }

    /// Parse a `.cube` (LUT_3D_SIZE, optional DOMAIN_MIN/MAX, TITLE).
    ///
    /// # Errors
    /// A message naming the line.
    pub fn from_cube(text: &str) -> Result<Self, String> {
        let mut size = 0usize;
        let mut title = String::new();
        let mut lo = [0.0f32; 3];
        let mut hi = [1.0f32; 3];
        let mut data = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut it = line.split_whitespace();
            let key = it.next().unwrap_or("");
            match key {
                "TITLE" => title = line[5..].trim().trim_matches('"').to_owned(),
                "LUT_3D_SIZE" => {
                    size = it
                        .next()
                        .and_then(|v| v.parse().ok())
                        .ok_or(format!("cube line {}: bad LUT_3D_SIZE", n + 1))?;
                }
                "LUT_1D_SIZE" => return Err("cube: 1D LUTs are not 3D tables".into()),
                "DOMAIN_MIN" | "DOMAIN_MAX" => {
                    let v: Vec<f32> = it.filter_map(|x| x.parse().ok()).collect();
                    if v.len() != 3 {
                        return Err(format!("cube line {}: bad domain", n + 1));
                    }
                    if key == "DOMAIN_MIN" {
                        lo = [v[0], v[1], v[2]];
                    } else {
                        hi = [v[0], v[1], v[2]];
                    }
                }
                _ => {
                    let v: Vec<f32> = line
                        .split_whitespace()
                        .filter_map(|x| x.parse().ok())
                        .collect();
                    if v.len() != 3 {
                        return Err(format!("cube line {}: expected three numbers", n + 1));
                    }
                    data.push([
                        (v[0] - lo[0]) / (hi[0] - lo[0]),
                        (v[1] - lo[1]) / (hi[1] - lo[1]),
                        (v[2] - lo[2]) / (hi[2] - lo[2]),
                    ]);
                }
            }
        }
        if size < 2 || data.len() != size * size * size {
            return Err(format!("cube: {} entries for size {size}", data.len()));
        }
        Ok(Self { size, data, title })
    }

    /// Write as `.cube`.
    #[must_use]
    pub fn to_cube(&self) -> String {
        let mut s = String::new();
        if !self.title.is_empty() {
            s.push_str(&format!("TITLE \"{}\"\n", self.title));
        }
        s.push_str(&format!("LUT_3D_SIZE {}\n", self.size));
        for c in &self.data {
            s.push_str(&format!("{:.6} {:.6} {:.6}\n", c[0], c[1], c[2]));
        }
        s
    }
}

/// A chain of operations, applied in order.
pub struct Chain(pub Vec<Box<dyn ColorOp + Send + Sync>>);

impl std::fmt::Debug for Chain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Chain({} ops)", self.0.len())
    }
}

impl ColorOp for Chain {
    fn apply(&self, mut rgb: [f32; 3]) -> [f32; 3] {
        for op in &self.0 {
            rgb = op.apply(rgb);
        }
        rgb
    }
}

/// A filmic "teal and orange" look: cool shadows, warm highlights, an S
/// curve — the blockbuster grade, as a ready table.
#[must_use]
pub fn look_teal_orange(strength: f32) -> Lut3 {
    let s = strength.clamp(0.0, 1.0);
    let prim = Primaries {
        lift: [-0.02 * s, 0.005 * s, 0.03 * s],
        gain: [1.0 + 0.05 * s, 1.0 + 0.01 * s, 1.0 - 0.04 * s],
        saturation: 1.0 + 0.1 * s,
        ..Primaries::default()
    };
    let curve = Curves {
        master: Curve::s_curve(0.035 * s),
        ..Curves::default()
    };
    let mut lut = Lut3::bake(33, &Chain(vec![Box::new(prim), Box::new(curve)]));
    lut.title = "vieww teal/orange".into();
    lut
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 3], b: [f32; 3], tol: f32) -> bool {
        (0..3).all(|k| (a[k] - b[k]).abs() <= tol)
    }

    #[test]
    fn identity_operations_are_identities() {
        let c = [0.2, 0.55, 0.9];
        assert!(close(Levels::default().apply(c), c, 1e-6));
        assert!(close(Curves::default().apply(c), c, 1e-5));
        assert!(close(HueSaturation::default().apply(c), c, 1e-5));
        assert!(close(Primaries::default().apply(c), c, 1e-5));
        assert!(close(Lut3::identity(17).sample(c), c, 1e-5));
    }

    #[test]
    fn levels_remap_black_and_white_points() {
        let l = Levels {
            in_black: 0.2,
            in_white: 0.8,
            ..Levels::default()
        };
        assert!(close(l.apply([0.2, 0.5, 0.8]), [0.0, 0.5, 1.0], 1e-5));
    }

    #[test]
    fn monotone_points_give_a_monotone_curve_without_overshoot() {
        let c = Curve::new(&[(0.0, 0.0), (0.1, 0.4), (0.2, 0.42), (1.0, 1.0)]);
        let mut last = -1.0;
        for i in 0..=200 {
            #[allow(clippy::cast_precision_loss)]
            let y = c.eval(i as f32 / 200.0);
            assert!(y >= last - 1e-6, "{i}");
            assert!((0.0..=1.0).contains(&y));
            last = y;
        }
        assert!((c.eval(0.1) - 0.4).abs() < 1e-6);
    }

    #[test]
    fn hue_rotation_by_120_degrees_cycles_primaries() {
        let h = HueSaturation {
            hue: 120.0,
            ..HueSaturation::default()
        };
        assert!(close(h.apply([1.0, 0.0, 0.0]), [0.0, 1.0, 0.0], 1e-5));
        let desat = HueSaturation {
            saturation: -1.0,
            ..HueSaturation::default()
        }
        .apply([0.9, 0.1, 0.3]);
        assert!((desat[0] - desat[1]).abs() < 1e-5 && (desat[1] - desat[2]).abs() < 1e-5);
    }

    #[test]
    fn hsl_round_trips() {
        for c in [
            [0.1, 0.2, 0.3],
            [0.9, 0.5, 0.1],
            [0.4, 0.4, 0.4],
            [0.0, 1.0, 0.5],
        ] {
            assert!(close(hsl_to_rgb(rgb_to_hsl(c)), c, 1e-5), "{c:?}");
        }
    }

    #[test]
    fn vibrance_boosts_dull_colours_more_than_vivid_ones() {
        let v = Vibrance(0.5);
        let dull = [0.5, 0.45, 0.4];
        let vivid = [0.9, 0.1, 0.1];
        let spread = |c: [f32; 3]| c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2]);
        let gain_dull = spread(v.apply(dull)) / spread(dull);
        let gain_vivid = spread(v.apply(vivid)) / spread(vivid);
        assert!(gain_dull > gain_vivid);
    }

    #[test]
    fn a_baked_lut_reproduces_its_chain_and_round_trips_through_cube() {
        let chain = Chain(vec![
            Box::new(Levels {
                gamma: 1.4,
                ..Levels::default()
            }),
            Box::new(HueSaturation {
                hue: 15.0,
                saturation: 0.2,
                lightness: 0.0,
            }),
        ]);
        let lut = Lut3::bake(33, &chain);
        for c in [[0.1, 0.5, 0.9], [0.7, 0.2, 0.3], [0.33, 0.33, 0.66]] {
            assert!(close(lut.sample(c), chain.apply(c), 0.02), "{c:?}");
        }
        let text = lut.to_cube();
        let back = Lut3::from_cube(&text).expect("cube");
        assert_eq!(back.size, 33);
        assert!(close(
            back.sample([0.4, 0.6, 0.2]),
            lut.sample([0.4, 0.6, 0.2]),
            1e-5
        ));
        assert!(Lut3::from_cube("LUT_3D_SIZE 2\n0 0 0\n").is_err());
    }

    #[test]
    fn the_teal_orange_look_cools_shadows_and_warms_highlights() {
        let lut = look_teal_orange(1.0);
        let shadow = lut.sample([0.12, 0.12, 0.12]);
        let high = lut.sample([0.8, 0.8, 0.8]);
        assert!(shadow[2] > shadow[0], "{shadow:?}");
        assert!(high[0] > high[2], "{high:?}");
        let mut px = vec![128u8, 128, 128, 255];
        lut.apply_rgba(&mut px, 0.0);
        assert_eq!(px, vec![128, 128, 128, 255]);
    }
}
