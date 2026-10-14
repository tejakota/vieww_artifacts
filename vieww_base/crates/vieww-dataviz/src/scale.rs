//! Scales — `d3-scale`: map a data domain onto a visual range.

/// D3's tick step: a power of ten times 1, 2 or 5, near `span / count`.
#[must_use]
pub fn tick_step(start: f64, stop: f64, count: usize) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    let step0 = (stop - start).abs() / count.max(1) as f64;
    if step0 <= 0.0 || !step0.is_finite() {
        return 0.0;
    }
    let power = step0.log10().floor();
    let error = step0 / 10f64.powf(power);
    let factor = if error >= 50f64.sqrt() {
        10.0
    } else if error >= 10f64.sqrt() {
        5.0
    } else if error >= 2f64.sqrt() {
        2.0
    } else {
        1.0
    };
    let step = factor * 10f64.powf(power);
    if stop < start {
        -step
    } else {
        step
    }
}

/// Round tick values covering `[start, stop]` — `d3.ticks`.
#[must_use]
pub fn ticks(start: f64, stop: f64, count: usize) -> Vec<f64> {
    let (lo, hi, rev) = if start <= stop {
        (start, stop, false)
    } else {
        (stop, start, true)
    };
    let step = tick_step(lo, hi, count);
    if step == 0.0 || !step.is_finite() {
        return vec![lo];
    }
    // D3's trick against float dust: for fractional steps, divide by the
    // (integral) inverse step instead of multiplying by the step, so 0.3 is
    // 3 / 10 rather than 3 × 0.1 = 0.30000000000000004.
    let inverse = if step < 1.0 {
        Some((1.0 / step).round())
    } else {
        None
    };
    let (i0, i1) = match inverse {
        Some(inc) => ((lo * inc).ceil(), (hi * inc).floor()),
        None => ((lo / step).ceil(), (hi / step).floor()),
    };
    #[allow(clippy::cast_possible_truncation)]
    let count = ((i1 - i0) as i64).max(-1);
    let mut out: Vec<f64> = (0..=count)
        .map(|k| {
            #[allow(clippy::cast_precision_loss)]
            let i = i0 + k as f64;
            match inverse {
                Some(inc) => i / inc,
                None => i * step,
            }
        })
        .collect();
    if rev {
        out.reverse();
    }
    out
}

/// A continuous numeric scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Transform {
    Linear,
    /// Logarithmic in `base`; the domain must not cross zero.
    Log(f64),
    /// Power `exponent` (0.5 is `scaleSqrt`), sign-preserving.
    Pow(f64),
}

impl Transform {
    fn forward(self, x: f64) -> f64 {
        match self {
            Self::Linear => x,
            Self::Log(_) => x.abs().ln() * x.signum(),
            Self::Pow(e) => x.abs().powf(e) * x.signum(),
        }
    }
    fn inverse(self, y: f64) -> f64 {
        match self {
            Self::Linear => y,
            Self::Log(_) => (y.abs()).exp() * y.signum(),
            Self::Pow(e) => y.abs().powf(1.0 / e) * y.signum(),
        }
    }
}

/// `scaleLinear`, `scaleLog`, `scalePow`, `scaleSqrt`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Continuous {
    pub domain: (f64, f64),
    pub range: (f64, f64),
    pub clamp: bool,
    pub transform: Transform,
}

impl Continuous {
    #[must_use]
    pub const fn linear(domain: (f64, f64), range: (f64, f64)) -> Self {
        Self {
            domain,
            range,
            clamp: false,
            transform: Transform::Linear,
        }
    }

    #[must_use]
    pub const fn log(domain: (f64, f64), range: (f64, f64)) -> Self {
        Self {
            domain,
            range,
            clamp: false,
            transform: Transform::Log(10.0),
        }
    }

    #[must_use]
    pub const fn pow(domain: (f64, f64), range: (f64, f64), exponent: f64) -> Self {
        Self {
            domain,
            range,
            clamp: false,
            transform: Transform::Pow(exponent),
        }
    }

    #[must_use]
    pub const fn sqrt(domain: (f64, f64), range: (f64, f64)) -> Self {
        Self::pow(domain, range, 0.5)
    }

    #[must_use]
    pub const fn clamped(mut self) -> Self {
        self.clamp = true;
        self
    }

    /// Data value → visual value.
    #[must_use]
    pub fn map(&self, x: f64) -> f64 {
        let t = self.transform;
        let (d0, d1) = (t.forward(self.domain.0), t.forward(self.domain.1));
        let mut u = if (d1 - d0).abs() < f64::EPSILON {
            0.5
        } else {
            (t.forward(x) - d0) / (d1 - d0)
        };
        if self.clamp {
            u = u.clamp(0.0, 1.0);
        }
        self.range.0 + (self.range.1 - self.range.0) * u
    }

    /// Visual value → data value.
    #[must_use]
    pub fn invert(&self, y: f64) -> f64 {
        let t = self.transform;
        let (d0, d1) = (t.forward(self.domain.0), t.forward(self.domain.1));
        let u = if (self.range.1 - self.range.0).abs() < f64::EPSILON {
            0.0
        } else {
            (y - self.range.0) / (self.range.1 - self.range.0)
        };
        t.inverse(d0 + (d1 - d0) * u)
    }

    /// Extend the domain to round values (`scale.nice()`).
    #[must_use]
    pub fn nice(mut self, count: usize) -> Self {
        if let Transform::Log(base) = self.transform {
            let lo = base.powf(self.domain.0.log(base).floor());
            let hi = base.powf(self.domain.1.log(base).ceil());
            self.domain = (lo, hi);
            return self;
        }
        let (mut lo, mut hi) = self.domain;
        let flip = lo > hi;
        if flip {
            std::mem::swap(&mut lo, &mut hi);
        }
        for _ in 0..10 {
            let step = tick_step(lo, hi, count);
            if step == 0.0 {
                break;
            }
            let (nlo, nhi) = ((lo / step).floor() * step, (hi / step).ceil() * step);
            if (nlo, nhi) == (lo, hi) {
                break;
            }
            lo = nlo;
            hi = nhi;
        }
        self.domain = if flip { (hi, lo) } else { (lo, hi) };
        self
    }

    /// Tick values for an axis.
    #[must_use]
    pub fn ticks(&self, count: usize) -> Vec<f64> {
        if let Transform::Log(base) = self.transform {
            let (lo, hi) = (
                self.domain.0.min(self.domain.1),
                self.domain.0.max(self.domain.1),
            );
            let mut out = Vec::new();
            #[allow(clippy::cast_possible_truncation)]
            let (e0, e1) = (lo.log(base).floor() as i32, hi.log(base).ceil() as i32);
            for e in e0..=e1 {
                let p = base.powi(e);
                for k in 1..10 {
                    let v = p * f64::from(k);
                    if v >= lo * (1.0 - 1e-12)
                        && v <= hi * (1.0 + 1e-12)
                        && (e1 - e0 <= 2 || k == 1)
                    {
                        out.push(v);
                    }
                }
            }
            return out;
        }
        ticks(self.domain.0, self.domain.1, count)
    }
}

/// `scaleBand` — discrete categories to evenly spaced bands.
#[derive(Debug, Clone, PartialEq)]
pub struct Band {
    pub domain: Vec<String>,
    pub range: (f64, f64),
    pub padding_inner: f64,
    pub padding_outer: f64,
    pub align: f64,
}

impl Band {
    #[must_use]
    pub fn new(domain: impl IntoIterator<Item = impl Into<String>>, range: (f64, f64)) -> Self {
        Self {
            domain: domain.into_iter().map(Into::into).collect(),
            range,
            padding_inner: 0.0,
            padding_outer: 0.0,
            align: 0.5,
        }
    }

    #[must_use]
    pub const fn padding(mut self, p: f64) -> Self {
        self.padding_inner = p;
        self.padding_outer = p;
        self
    }

    #[allow(clippy::cast_precision_loss)]
    fn layout(&self) -> (f64, f64) {
        let n = self.domain.len() as f64;
        let (r0, r1) = self.range;
        let span = r1 - r0;
        let step = span / (n - self.padding_inner + self.padding_outer * 2.0).max(1.0);
        let start = r0 + (span - step * (n - self.padding_inner)) * self.align;
        (start, step)
    }

    /// Width of one band.
    #[must_use]
    pub fn bandwidth(&self) -> f64 {
        self.layout().1 * (1.0 - self.padding_inner)
    }

    /// Distance between band starts.
    #[must_use]
    pub fn step(&self) -> f64 {
        self.layout().1
    }

    /// The start of `key`'s band, or `None` for an unknown key.
    #[must_use]
    pub fn map(&self, key: &str) -> Option<f64> {
        let i = self.domain.iter().position(|k| k == key)?;
        let (start, step) = self.layout();
        #[allow(clippy::cast_precision_loss)]
        Some(start + step * i as f64)
    }
}

/// `scalePoint` — a band scale with zero-width bands.
#[must_use]
pub fn point_scale(
    domain: impl IntoIterator<Item = impl Into<String>>,
    range: (f64, f64),
    padding: f64,
) -> Band {
    let mut b = Band::new(domain, range);
    b.padding_inner = 1.0;
    b.padding_outer = padding;
    b
}

/// `scaleOrdinal` — categories to a cycled list of outputs.
#[derive(Debug, Clone, PartialEq)]
pub struct Ordinal<T: Clone> {
    domain: Vec<String>,
    range: Vec<T>,
}

impl<T: Clone> Ordinal<T> {
    #[must_use]
    pub const fn new(range: Vec<T>) -> Self {
        Self {
            domain: Vec::new(),
            range,
        }
    }

    /// The output for `key`; unseen keys are appended to the domain, as in
    /// D3's implicit domain.
    pub fn map(&mut self, key: &str) -> Option<T> {
        if self.range.is_empty() {
            return None;
        }
        let i = match self.domain.iter().position(|k| k == key) {
            Some(i) => i,
            None => {
                self.domain.push(key.to_owned());
                self.domain.len() - 1
            }
        };
        Some(self.range[i % self.range.len()].clone())
    }
}

/// Format a tick value the way D3's default formatter does for its step:
/// just enough decimals, SI suffixes for large values.
#[must_use]
pub fn format_tick(value: f64, step: f64) -> String {
    let a = value.abs();
    if a >= 1e9 && step >= 1e8 {
        return format!("{}G", trim(value / 1e9));
    }
    if a >= 1e6 && step >= 1e5 {
        return format!("{}M", trim(value / 1e6));
    }
    if a >= 1e4 && step >= 1e3 {
        return format!("{}k", trim(value / 1e3));
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let decimals = if step > 0.0 && step < 1.0 {
        (-step.log10()).ceil() as usize
    } else {
        0
    };
    format!("{value:.decimals$}")
}

fn trim(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_match_d3() {
        assert_eq!(
            ticks(0.0, 1.0, 10),
            vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0]
        );
        assert_eq!(ticks(0.0, 10.0, 5), vec![0.0, 2.0, 4.0, 6.0, 8.0, 10.0]);
        assert_eq!(ticks(-3.3, 7.7, 5), vec![-2.0, 0.0, 2.0, 4.0, 6.0]);
        assert_eq!(ticks(1.0, 0.0, 2), vec![1.0, 0.5, 0.0]);
    }

    #[test]
    fn linear_maps_inverts_clamps_and_nices() {
        let s = Continuous::linear((0.0, 100.0), (0.0, 500.0));
        assert_eq!(s.map(50.0), 250.0);
        assert_eq!(s.invert(250.0), 50.0);
        assert_eq!(s.map(200.0), 1000.0);
        assert_eq!(s.clamped().map(200.0), 500.0);
        let n = Continuous::linear((0.201, 0.996), (0.0, 1.0)).nice(10);
        assert_eq!(n.domain, (0.2, 1.0));
    }

    #[test]
    fn log_and_sqrt() {
        let l = Continuous::log((1.0, 1000.0), (0.0, 300.0));
        assert!((l.map(10.0) - 100.0).abs() < 1e-9);
        assert!((l.invert(200.0) - 100.0).abs() < 1e-9);
        let t = l.ticks(10);
        assert!(t.contains(&1.0) && t.contains(&10.0) && t.contains(&1000.0));
        let q = Continuous::sqrt((0.0, 100.0), (0.0, 10.0));
        assert!((q.map(25.0) - 5.0).abs() < 1e-9);
        let nl = Continuous::log((3.0, 700.0), (0.0, 1.0)).nice(10);
        assert_eq!(nl.domain, (1.0, 1000.0));
    }

    #[test]
    fn bands_and_points() {
        let b = Band::new(["a", "b", "c"], (0.0, 300.0)).padding(0.0);
        assert_eq!(b.map("b"), Some(100.0));
        assert_eq!(b.bandwidth(), 100.0);
        let pb = Band::new(["a", "b", "c", "d"], (0.0, 100.0)).padding(0.2);
        assert!((pb.step() - 100.0 / 4.2).abs() < 1e-9);
        assert!((pb.bandwidth() - pb.step() * 0.8).abs() < 1e-9);
        assert_eq!(b.map("z"), None);
        let p = point_scale(["x", "y", "z"], (0.0, 100.0), 0.0);
        assert_eq!((p.map("x"), p.map("z")), (Some(0.0), Some(100.0)));
        assert_eq!(p.bandwidth(), 0.0);
    }

    #[test]
    fn ordinal_cycles_and_remembers() {
        let mut o = Ordinal::new(vec!["red", "green"]);
        assert_eq!(o.map("a"), Some("red"));
        assert_eq!(o.map("b"), Some("green"));
        assert_eq!(o.map("c"), Some("red"));
        assert_eq!(o.map("a"), Some("red"));
    }

    #[test]
    fn tick_formats() {
        assert_eq!(format_tick(0.30000000000000004, 0.1), "0.3");
        assert_eq!(format_tick(20000.0, 5000.0), "20k");
        assert_eq!(format_tick(2_500_000.0, 500_000.0), "2.5M");
        assert_eq!(format_tick(7.0, 1.0), "7");
    }
}
