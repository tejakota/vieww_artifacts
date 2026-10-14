//! Channel layouts beyond stereo — quad, 5.1, 7.1 — and the up/down-mix
//! rules between them.
//!
//! [`Samples`](crate::Samples) stays mono-or-stereo: that is the shape UI
//! sound has, and its docs say why. Production audio routes surround beds,
//! and this module is where more channels live: [`Layout`] names the
//! speaker arrangement (in the WAV/SMPTE channel order every file and
//! device API agrees on), [`remix`] converts one frame between layouts, and
//! [`MultiSamples`] carries a buffer of any layout.
//!
//! The mixing rules are Web Audio's "speakers" interpretation verbatim for
//! mono, stereo, quad and 5.1 (the spec's tables), extended to 7.1 with
//! ITU-R BS.775 coefficients: side and back surrounds fold together at
//! −3 dB each into a 5.1 surround. Layouts the rules do not name fall back
//! to Web Audio's "discrete" rule — copy matching channels, zero or drop the
//! rest — which is what "I know what these channels are, don't mix them"
//! means.

use std::f32::consts::FRAC_1_SQRT_2 as H;

/// A speaker layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// One channel.
    Mono,
    /// L R.
    Stereo,
    /// L R SL SR.
    Quad,
    /// L R C LFE SL SR.
    Surround51,
    /// L R C LFE BL BR SL SR (WAVE_FORMAT_EXTENSIBLE order).
    Surround71,
    /// `n` unnamed channels.
    Discrete(usize),
}

impl Layout {
    /// The conventional layout for a channel count.
    #[must_use]
    pub const fn for_channels(n: usize) -> Self {
        match n {
            1 => Self::Mono,
            2 => Self::Stereo,
            4 => Self::Quad,
            6 => Self::Surround51,
            8 => Self::Surround71,
            n => Self::Discrete(n),
        }
    }

    /// Channel count.
    #[must_use]
    pub const fn channels(self) -> usize {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
            Self::Quad => 4,
            Self::Surround51 => 6,
            Self::Surround71 => 8,
            Self::Discrete(n) => n,
        }
    }

    /// Channel labels in order.
    #[must_use]
    pub fn labels(self) -> Vec<&'static str> {
        match self {
            Self::Mono => vec!["M"],
            Self::Stereo => vec!["L", "R"],
            Self::Quad => vec!["L", "R", "SL", "SR"],
            Self::Surround51 => vec!["L", "R", "C", "LFE", "SL", "SR"],
            Self::Surround71 => vec!["L", "R", "C", "LFE", "BL", "BR", "SL", "SR"],
            Self::Discrete(n) => vec!["?"; n],
        }
    }

    /// The WAVE_FORMAT_EXTENSIBLE channel mask for this layout.
    #[must_use]
    pub const fn wave_mask(self) -> u32 {
        match self {
            Self::Mono => 0x4,
            Self::Stereo => 0x3,
            Self::Quad => 0x33,
            Self::Surround51 => 0x60F,
            Self::Surround71 => 0x63F,
            Self::Discrete(_) => 0,
        }
    }
}

/// Convert one frame from `from` to `to`. `input.len()` must equal
/// `from.channels()` and `output.len()` must equal `to.channels()`.
pub fn remix(from: Layout, to: Layout, input: &[f32], output: &mut [f32]) {
    use Layout::{Mono, Quad, Stereo, Surround51 as S51, Surround71 as S71};
    output.fill(0.0);
    let i = input;
    match (from, to) {
        (a, b) if a == b => output.copy_from_slice(input),
        // Up-mixing.
        (Mono, Stereo) => {
            output[0] = i[0];
            output[1] = i[0];
        }
        (Mono, Quad) => {
            output[0] = i[0];
            output[1] = i[0];
        }
        (Mono, S51 | S71) => output[2] = i[0],
        (Stereo, Quad | S51 | S71) => {
            output[0] = i[0];
            output[1] = i[1];
        }
        (Quad, S51) => {
            output[0] = i[0];
            output[1] = i[1];
            output[4] = i[2];
            output[5] = i[3];
        }
        (Quad, S71) => {
            output[0] = i[0];
            output[1] = i[1];
            output[6] = i[2];
            output[7] = i[3];
        }
        (S51, S71) => {
            output[..4].copy_from_slice(&i[..4]);
            output[6] = i[4];
            output[7] = i[5];
        }
        // Down-mixing.
        (Stereo, Mono) => output[0] = 0.5 * (i[0] + i[1]),
        (Quad, Mono) => output[0] = 0.25 * (i[0] + i[1] + i[2] + i[3]),
        (S51, Mono) => output[0] = H * (i[0] + i[1]) + i[2] + 0.5 * (i[4] + i[5]),
        (Quad, Stereo) => {
            output[0] = 0.5 * (i[0] + i[2]);
            output[1] = 0.5 * (i[1] + i[3]);
        }
        (S51, Stereo) => {
            output[0] = i[0] + H * (i[2] + i[4]);
            output[1] = i[1] + H * (i[2] + i[5]);
        }
        (S51, Quad) => {
            output[0] = i[0] + H * i[2];
            output[1] = i[1] + H * i[2];
            output[2] = i[4];
            output[3] = i[5];
        }
        (S71, _) => {
            // Fold to 5.1 first (BS.775: side and back at −3 dB each).
            let fold = [i[0], i[1], i[2], i[3], H * (i[4] + i[6]), H * (i[5] + i[7])];
            remix(S51, to, &fold, output);
        }
        // Discrete: copy what lines up.
        _ => {
            let n = input.len().min(output.len());
            output[..n].copy_from_slice(&input[..n]);
        }
    }
}

/// An interleaved buffer of any layout.
#[derive(Debug, Clone, PartialEq)]
pub struct MultiSamples {
    /// Interleaved frames.
    pub data: Vec<f32>,
    /// The layout.
    pub layout: Layout,
    /// Sample rate.
    pub rate: u32,
}

impl MultiSamples {
    /// Frame count.
    #[must_use]
    pub fn frames(&self) -> usize {
        self.data.len() / self.layout.channels().max(1)
    }

    /// Convert to another layout.
    #[must_use]
    pub fn remixed(&self, to: Layout) -> Self {
        let ci = self.layout.channels();
        let co = to.channels();
        let mut data = vec![0.0; self.frames() * co];
        for (src, dst) in self.data.chunks_exact(ci).zip(data.chunks_exact_mut(co)) {
            remix(self.layout, to, src, dst);
        }
        Self {
            data,
            layout: to,
            rate: self.rate,
        }
    }

    /// Down/up-mix to a stereo [`Samples`](crate::Samples).
    #[must_use]
    pub fn to_stereo(&self) -> crate::Samples {
        crate::Samples::stereo(self.remixed(Layout::Stereo).data, self.rate)
    }

    /// From a mono or stereo [`Samples`](crate::Samples).
    #[must_use]
    pub fn from_samples(s: &crate::Samples) -> Self {
        Self {
            data: s.data.clone(),
            layout: Layout::for_channels(usize::from(s.channels)),
            rate: s.rate,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mix(from: Layout, to: Layout, input: &[f32]) -> Vec<f32> {
        let mut out = vec![0.0; to.channels()];
        remix(from, to, input, &mut out);
        out
    }

    #[test]
    fn web_audio_downmix_tables() {
        assert_eq!(mix(Layout::Stereo, Layout::Mono, &[1.0, 0.5]), vec![0.75]);
        assert_eq!(
            mix(Layout::Quad, Layout::Mono, &[1.0, 1.0, 1.0, 1.0]),
            vec![1.0]
        );
        let m = mix(
            Layout::Surround51,
            Layout::Mono,
            &[1.0, 1.0, 1.0, 9.0, 1.0, 1.0],
        );
        assert!((m[0] - (2.0 * H + 1.0 + 1.0)).abs() < 1e-6, "LFE dropped");
        let s = mix(
            Layout::Surround51,
            Layout::Stereo,
            &[0.0, 0.0, 1.0, 0.0, 0.0, 0.0],
        );
        assert!(
            (s[0] - H).abs() < 1e-6 && (s[1] - H).abs() < 1e-6,
            "centre at −3 dB both sides"
        );
        let q = mix(Layout::Quad, Layout::Stereo, &[1.0, 0.0, 1.0, 0.0]);
        assert_eq!(q, vec![1.0, 0.0]);
    }

    #[test]
    fn upmix_places_mono_in_the_centre() {
        let o = mix(Layout::Mono, Layout::Surround51, &[0.7]);
        assert_eq!(o, vec![0.0, 0.0, 0.7, 0.0, 0.0, 0.0]);
        let o = mix(Layout::Mono, Layout::Stereo, &[0.7]);
        assert_eq!(o, vec![0.7, 0.7]);
    }

    #[test]
    fn seven_one_folds_surrounds() {
        let o = mix(
            Layout::Surround71,
            Layout::Surround51,
            &[0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0],
        );
        assert!((o[4] - 2.0 * H).abs() < 1e-6 && o[5] == 0.0);
        let st = mix(
            Layout::Surround71,
            Layout::Stereo,
            &[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        );
        assert_eq!(st, vec![1.0, 0.0]);
    }

    #[test]
    fn up_then_down_round_trips_stereo() {
        for to in [Layout::Surround51, Layout::Surround71] {
            let up = mix(Layout::Stereo, to, &[0.3, -0.4]);
            let back = mix(to, Layout::Stereo, &up);
            assert!(
                (back[0] - 0.3).abs() < 1e-6 && (back[1] + 0.4).abs() < 1e-6,
                "{to:?}"
            );
        }
        // Quad is the exception, by the spec: its downmix averages front and
        // surround (`0.5·(L + SL)`), so stereo placed only in the fronts
        // comes back at half level.
        let up = mix(Layout::Stereo, Layout::Quad, &[0.3, -0.4]);
        let back = mix(Layout::Quad, Layout::Stereo, &up);
        assert!((back[0] - 0.15).abs() < 1e-6 && (back[1] + 0.2).abs() < 1e-6);
    }

    #[test]
    fn discrete_copies_what_lines_up() {
        let o = mix(Layout::Discrete(3), Layout::Discrete(5), &[1.0, 2.0, 3.0]);
        assert_eq!(o, vec![1.0, 2.0, 3.0, 0.0, 0.0]);
    }

    #[test]
    fn buffers_remix() {
        let b = MultiSamples {
            data: vec![0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0],
            layout: Layout::Surround51,
            rate: 48_000,
        };
        let s = b.to_stereo();
        assert_eq!(s.frames(), 2);
        assert!((s.data[0] - H).abs() < 1e-6);
        assert_eq!(Layout::Surround71.wave_mask(), 0x63F);
    }
}
