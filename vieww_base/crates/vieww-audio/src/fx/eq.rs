//! Equalisation: parametric (with steep Butterworth cuts), graphic (ISO
//! octave and third-octave), dynamic, and linear-phase.
//!
//! * [`ParametricEq`] — any number of bands, each a peak, shelf, notch,
//!   band-pass, or a low/high cut at 12–48 dB/oct (cascaded biquads with
//!   Butterworth pole Qs, so a "24 dB" cut is a true fourth-order
//!   Butterworth, flat to its corner). [`ParametricEq::response`] gives the
//!   exact curve for drawing an EQ display.
//! * [`GraphicEq`] — fixed ISO 266 centres with constant-Q peaking bands.
//! * [`DynamicBand`] — a band whose gain moves with the level *in that
//!   band*: de-essing, taming a resonance only when it rings.
//! * [`LinearPhaseEq`] — any [`ParametricEq`]'s magnitude response with
//!   zero phase shift, designed as a symmetric FIR by frequency sampling and
//!   run through the partitioned convolver. Mastering engineers pay latency
//!   for it because a boost no longer smears transients; the latency is
//!   reported.

use super::filter::{Svf, SvfMode};
use super::fir::{kaiser, Convolver};
use crate::dsp::{Biquad, FilterKind, Processor};
use crate::fft::{Complex, Plan};

/// A parametric band type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BandKind {
    /// Bell boost/cut.
    Peak,
    /// Low shelf.
    LowShelf,
    /// High shelf.
    HighShelf,
    /// Notch.
    Notch,
    /// Band-pass.
    BandPass,
    /// High-pass ("low cut") at 12, 24, 36 or 48 dB/oct.
    LowCut(u8),
    /// Low-pass ("high cut") at 12, 24, 36 or 48 dB/oct.
    HighCut(u8),
}

/// One parametric band.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    /// Type.
    pub kind: BandKind,
    /// Centre or corner in Hz.
    pub freq: f32,
    /// Gain in dB (peaks and shelves).
    pub gain_db: f32,
    /// Quality (ignored by cuts, which are Butterworth).
    pub q: f32,
    /// Bypass.
    pub enabled: bool,
}

impl Band {
    /// A bell.
    #[must_use]
    pub const fn peak(freq: f32, gain_db: f32, q: f32) -> Self {
        Self {
            kind: BandKind::Peak,
            freq,
            gain_db,
            q,
            enabled: true,
        }
    }

    /// A cut or shelf or notch with default Q.
    #[must_use]
    pub const fn of(kind: BandKind, freq: f32, gain_db: f32) -> Self {
        Self {
            kind,
            freq,
            gain_db,
            q: std::f32::consts::FRAC_1_SQRT_2,
            enabled: true,
        }
    }

    fn sections(&self, rate: f32) -> Vec<Biquad> {
        if !self.enabled {
            return Vec::new();
        }
        let butter = |slope: u8| -> Vec<f32> {
            let order = usize::from((slope / 6).clamp(2, 8) / 2 * 2);
            #[expect(clippy::cast_precision_loss, reason = "orders up to 8")]
            (1..=order / 2)
                .map(|k| {
                    let th = (2 * k - 1) as f32 * std::f32::consts::PI / (2 * order) as f32;
                    1.0 / (2.0 * th.sin())
                })
                .collect()
        };
        match self.kind {
            BandKind::Peak => vec![Biquad::new(
                FilterKind::Peaking(self.gain_db),
                self.freq,
                self.q,
                rate,
            )],
            BandKind::LowShelf => vec![Biquad::new(
                FilterKind::LowShelf(self.gain_db),
                self.freq,
                self.q,
                rate,
            )],
            BandKind::HighShelf => vec![Biquad::new(
                FilterKind::HighShelf(self.gain_db),
                self.freq,
                self.q,
                rate,
            )],
            BandKind::Notch => vec![Biquad::new(FilterKind::Notch, self.freq, self.q, rate)],
            BandKind::BandPass => vec![Biquad::new(FilterKind::BandPass, self.freq, self.q, rate)],
            BandKind::LowCut(s) => butter(s)
                .into_iter()
                .map(|q| Biquad::new(FilterKind::HighPass, self.freq, q, rate))
                .collect(),
            BandKind::HighCut(s) => butter(s)
                .into_iter()
                .map(|q| Biquad::new(FilterKind::LowPass, self.freq, q, rate))
                .collect(),
        }
    }
}

/// A parametric equaliser.
#[derive(Debug, Clone)]
pub struct ParametricEq {
    bands: Vec<Band>,
    sections: Vec<Biquad>,
    rate: f32,
}

impl ParametricEq {
    /// An EQ over `bands`.
    #[must_use]
    pub fn new(bands: Vec<Band>, rate: f32) -> Self {
        let mut eq = Self {
            bands,
            sections: Vec::new(),
            rate,
        };
        eq.rebuild();
        eq
    }

    /// The bands.
    #[must_use]
    pub fn bands(&self) -> &[Band] {
        &self.bands
    }

    /// Replace a band (state of other bands is kept by rebuilding fresh —
    /// automate slowly, or use [`DynamicBand`] for fast movement).
    pub fn set_band(&mut self, i: usize, band: Band) {
        if let Some(b) = self.bands.get_mut(i) {
            *b = band;
            self.rebuild();
        }
    }

    fn rebuild(&mut self) {
        self.sections = self
            .bands
            .iter()
            .flat_map(|b| b.sections(self.rate))
            .collect();
    }

    /// The magnitude response at `hz` (linear).
    #[must_use]
    pub fn response(&self, hz: f32) -> f32 {
        self.sections
            .iter()
            .map(|s| s.magnitude(hz, self.rate))
            .product()
    }
}

impl Processor for ParametricEq {
    fn process(&mut self, x: f32) -> f32 {
        self.sections.iter_mut().fold(x, |v, s| s.process(v))
    }
}

/// ISO octave centres (10 bands).
pub const ISO_OCTAVE: [f32; 10] = [
    31.5, 63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16_000.0,
];

/// ISO third-octave centres (31 bands, 20 Hz – 20 kHz).
pub const ISO_THIRD: [f32; 31] = [
    20.0, 25.0, 31.5, 40.0, 50.0, 63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0, 315.0, 400.0,
    500.0, 630.0, 800.0, 1000.0, 1250.0, 1600.0, 2000.0, 2500.0, 3150.0, 4000.0, 5000.0, 6300.0,
    8000.0, 10_000.0, 12_500.0, 16_000.0, 20_000.0,
];

/// A graphic EQ: one constant-Q peaking band per ISO centre.
#[derive(Debug, Clone)]
pub struct GraphicEq {
    eq: ParametricEq,
}

impl GraphicEq {
    /// Octave (`third = false`, Q 1.41) or third-octave (Q 4.32) bands with
    /// these slider gains in dB (missing sliders are 0).
    #[must_use]
    pub fn new(third: bool, gains_db: &[f32], rate: f32) -> Self {
        let (centres, q): (&[f32], f32) = if third {
            (&ISO_THIRD, 4.32)
        } else {
            (&ISO_OCTAVE, std::f32::consts::SQRT_2)
        };
        let bands = centres
            .iter()
            .enumerate()
            .filter(|(_, f)| **f < rate * 0.45)
            .map(|(i, f)| Band::peak(*f, gains_db.get(i).copied().unwrap_or(0.0), q))
            .collect();
        Self {
            eq: ParametricEq::new(bands, rate),
        }
    }

    /// The response at `hz`.
    #[must_use]
    pub fn response(&self, hz: f32) -> f32 {
        self.eq.response(hz)
    }
}

impl Processor for GraphicEq {
    fn process(&mut self, x: f32) -> f32 {
        self.eq.process(x)
    }
}

/// A dynamic EQ band: a bell whose gain follows the level in its band.
///
/// The bell is built as `x + (g − 1)·BP(x)` with a unity-peak band-pass, so
/// the gain `g` can change every sample without recomputing coefficients.
/// Above `threshold`, `g` falls toward `max_cut_db` with `ratio` (downward,
/// a de-esser) — or rises toward a positive `max_cut_db` (upward).
#[derive(Debug, Clone)]
pub struct DynamicBand {
    bp: Svf,
    detect: Svf,
    /// Threshold in dBFS of the band signal.
    pub threshold: f32,
    /// Ratio (above threshold).
    pub ratio: f32,
    /// Maximum gain change in dB (negative cuts, positive boosts).
    pub max_db: f32,
    env: f32,
    attack: f32,
    release: f32,
    q: f32,
    /// The current applied gain in dB (for a meter).
    pub gain_db: f32,
}

impl DynamicBand {
    /// A band at `freq` with quality `q`.
    #[must_use]
    pub fn new(freq: f32, q: f32, threshold: f32, ratio: f32, max_db: f32, rate: f32) -> Self {
        Self {
            bp: Svf::new(SvfMode::BandPass, freq, q, rate),
            detect: Svf::new(SvfMode::BandPass, freq, q, rate),
            threshold,
            ratio,
            max_db,
            env: 0.0,
            attack: (-1.0 / (0.002 * rate)).exp(),
            release: (-1.0 / (0.08 * rate)).exp(),
            q,
            gain_db: 0.0,
        }
    }
}

impl Processor for DynamicBand {
    fn process(&mut self, x: f32) -> f32 {
        // The SVF band output peaks at 1/k = Q; normalise to unity.
        let det = self.detect.process(x) / self.q;
        let a = det.abs();
        let c = if a > self.env {
            self.attack
        } else {
            self.release
        };
        self.env = a + c * (self.env - a);
        let lvl = 20.0 * self.env.max(1e-9).log10();
        let over = (lvl - self.threshold).max(0.0);
        let change = over * (1.0 - 1.0 / self.ratio.max(1.0));
        self.gain_db = if self.max_db < 0.0 {
            (-change).max(self.max_db)
        } else {
            change.min(self.max_db)
        };
        let g = 10f32.powf(self.gain_db / 20.0);
        let band = self.bp.process(x) / self.q;
        x + (g - 1.0) * band
    }
}

/// A linear-phase EQ: the magnitude response of a [`ParametricEq`] with no
/// phase shift, as a symmetric FIR.
#[derive(Debug, Clone)]
pub struct LinearPhaseEq {
    conv: Convolver,
    kernel_len: usize,
    kernel: Vec<f32>,
}

impl LinearPhaseEq {
    /// Design from `eq`'s response with `taps` (power of two; 4096 gives
    /// ~12 Hz resolution at 48 kHz) and a `block`-sample convolver.
    ///
    /// # Panics
    /// If `taps` or `block` is not a power of two.
    #[must_use]
    pub fn new(eq: &ParametricEq, taps: usize, block: usize) -> Self {
        assert!(taps.is_power_of_two(), "taps must be a power of two");
        let plan = Plan::new(taps);
        #[expect(clippy::cast_precision_loss, reason = "tap counts")]
        let mut spec: Vec<Complex> = (0..taps)
            .map(|k| {
                let kk = if k <= taps / 2 { k } else { taps - k };
                let hz = kk as f32 / taps as f32 * eq.rate;
                Complex::new(f64::from(eq.response(hz.max(0.0))), 0.0)
            })
            .collect();
        plan.inverse(&mut spec);
        // Zero-phase → centred (linear-phase), windowed.
        let w = kaiser(taps - 1, 6.0);
        #[expect(clippy::cast_possible_truncation, reason = "f32 audio")]
        let kernel: Vec<f32> = (0..taps - 1)
            .map(|i| {
                let src = (i + taps - (taps / 2 - 1)) % taps;
                (spec[src].re * w[i]) as f32
            })
            .collect();
        Self {
            conv: Convolver::new(&kernel, block),
            kernel_len: kernel.len(),
            kernel,
        }
    }

    /// Total latency in samples (kernel centre + convolver block).
    #[must_use]
    pub const fn latency(&self) -> usize {
        (self.kernel_len - 1) / 2 + self.conv.latency()
    }

    /// The kernel (symmetric).
    #[must_use]
    pub fn kernel(&self) -> &[f32] {
        &self.kernel
    }
}

impl Processor for LinearPhaseEq {
    fn process(&mut self, x: f32) -> f32 {
        self.conv.process(x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn db(x: f32) -> f32 {
        20.0 * x.log10()
    }

    /// Steady-state gain by RMS ratio.
    fn measured(p: &mut impl Processor, hz: f32) -> f32 {
        let mut e_in = 0.0f64;
        let mut e_out = 0.0f64;
        for i in 0..96_000 {
            #[expect(clippy::cast_precision_loss, reason = "test")]
            let x = (2.0 * std::f32::consts::PI * hz * i as f32 / RATE).sin();
            let y = p.process(x);
            if i >= 72_000 {
                e_in += f64::from(x).powi(2);
                e_out += f64::from(y).powi(2);
            }
        }
        #[expect(clippy::cast_possible_truncation, reason = "test")]
        let g = (e_out / e_in).sqrt() as f32;
        g
    }

    #[test]
    fn peak_band_hits_its_gain_at_centre() {
        let mut eq = ParametricEq::new(vec![Band::peak(1000.0, 6.0, 1.0)], RATE);
        assert!((db(eq.response(1000.0)) - 6.0).abs() < 0.01);
        assert!((db(measured(&mut eq, 1000.0)) - 6.0).abs() < 0.1);
        assert!(db(eq.response(50.0)).abs() < 0.1);
    }

    #[test]
    fn butterworth_cuts_have_their_slopes() {
        for (slope, expect) in [(12, 12.0), (24, 24.0), (48, 48.0)] {
            let eq = ParametricEq::new(vec![Band::of(BandKind::LowCut(slope), 1000.0, 0.0)], RATE);
            assert!(
                (db(eq.response(1000.0)) + 3.01).abs() < 0.1,
                "{slope}: -3 dB corner"
            );
            let a = db(eq.response(125.0));
            let b = db(eq.response(250.0));
            assert!(((b - a) - expect).abs() < 1.0, "{slope}: {}", b - a);
            // Maximally flat: no bump above the corner.
            assert!(db(eq.response(4000.0)) < 0.01 && db(eq.response(4000.0)) > -0.1);
        }
    }

    #[test]
    fn graphic_eq_slider_boosts_its_band() {
        let mut g = vec![0.0; 10];
        g[5] = 9.0; // 1 kHz
        let eq = GraphicEq::new(false, &g, RATE);
        assert!((db(eq.response(1000.0)) - 9.0).abs() < 0.3);
        assert!(db(eq.response(125.0)).abs() < 0.5);
        let third = GraphicEq::new(true, &[0.0; 31], RATE);
        assert!(db(third.response(1234.0)).abs() < 1e-3);
    }

    #[test]
    fn dynamic_band_cuts_only_when_loud() {
        let loud = {
            let mut d = DynamicBand::new(6000.0, 2.0, -30.0, 4.0, -12.0, RATE);
            db(measured(&mut d, 6000.0))
        };
        let quiet = {
            let mut d = DynamicBand::new(6000.0, 2.0, -30.0, 4.0, -12.0, RATE);
            let mut peak = 0.0f32;
            for i in 0..96_000 {
                #[expect(clippy::cast_precision_loss, reason = "test")]
                let x = 0.001 * (2.0 * std::f32::consts::PI * 6000.0 * i as f32 / RATE).sin();
                let y = d.process(x);
                if i > 72_000 {
                    peak = peak.max(y.abs());
                }
            }
            db(peak / 0.001)
        };
        assert!(loud < -10.0, "loud cut: {loud}");
        assert!(quiet.abs() < 0.2, "quiet untouched: {quiet}");
        let off_band = {
            let mut d = DynamicBand::new(6000.0, 2.0, -30.0, 4.0, -12.0, RATE);
            db(measured(&mut d, 300.0))
        };
        assert!(off_band.abs() < 0.5, "{off_band}");
    }

    #[test]
    fn linear_phase_eq_is_symmetric_and_matches_magnitude() {
        let eq = ParametricEq::new(
            vec![
                Band::peak(1000.0, 6.0, 1.0),
                Band::of(BandKind::LowCut(24), 80.0, 0.0),
            ],
            RATE,
        );
        let mut lp = LinearPhaseEq::new(&eq, 4096, 256);
        let k = lp.kernel();
        let n = k.len();
        for i in 0..n / 2 {
            assert!((k[i] - k[n - 1 - i]).abs() < 1e-6, "symmetric at {i}");
        }
        for hz in [300.0, 1000.0, 5000.0] {
            let got = db(measured(&mut lp, hz));
            let want = db(eq.response(hz));
            assert!((got - want).abs() < 0.3, "{hz}: {got} vs {want}");
        }
        assert_eq!(lp.latency(), 2047 + 256);
    }
}
