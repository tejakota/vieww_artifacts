//! Frequency analysis — the FFT behind every audio-reactive visual, from
//! TouchDesigner's audio CHOPs to Hydra's `a.fft()` to Notch's spectrum
//! scopes, and the "Audio Sync" row of the document's capability table that
//! vieww had no answer for.
//!
//! # What this adds to the crate
//!
//! Everything else here *produces* samples — a [`Tone`](crate::Tone), a
//! [`Mixer`](crate::Mixer). This module reads them back and asks *what
//! frequencies* they contain: a [`Spectrum`] is the loudness of each pitch
//! band in a buffer, which is exactly the input a spectrum analyser, a
//! VU bar, a waveform-reactive glow or a beat detector wants. Synthesis and
//! analysis in one crate is also the cheapest complete test: render a
//! [`Tone`](crate::Tone) at a known frequency, analyse it, and the peak bin *is* the
//! frequency — no fixture file, no golden sample, no trust.
//!
//! # The one rule
//!
//! Analysis is a pure function of the buffer: the same samples give the
//! same spectrum, on every machine, in any order. No state carries between
//! calls, so two bars driven by two calls with the same buffer move
//! identically — the determinism the rest of this crate promises, extended
//! from "the mix is reproducible" to "the *picture of the mix* is
//! reproducible".
//!
//! # The transform
//!
//! A radix-2 Cooley–Tukey FFT, iterative, in-place, with the bit-reversal
//! permutation first — the textbook algorithm, with no dependency and no
//! allocation beyond the output. Sizes must be powers of two (the radix-2
//! butterfly's requirement), and [`Spectrum::analyze`] picks the window size
//! itself: the largest power of two that fits the buffer, because asking
//! every caller to pad their own buffer is asking them to know this
//! module's internals.
//!
//! ```
//! use vieww_audio::{Envelope, Mixer, Tone, Waveform};
//! use vieww_audio::analysis::{Spectrum, Window};
//! use std::time::Duration;
//!
//! // A 440 Hz sine with a quarter-second release, rendered.
//! let tone = Tone::new(440.0, Waveform::Sine)
//!     .envelope(Envelope::attack_release(
//!         Duration::from_millis(5),
//!         Duration::from_millis(250),
//!     ));
//! let samples = Mixer::new(44_100).add(tone, 0.8).render();
//!
//! let spectrum = Spectrum::analyze(&samples, Window::Hann);
//!
//! // The loudest pitch in the buffer is the pitch that was rendered.
//! let peak_hz = spectrum.bin_hz(spectrum.peak_bin());
//! assert!((peak_hz - 440.0).abs() < 12.0, "peak at {peak_hz} Hz");
//! ```

use crate::Samples;

/// The window applied before the transform.
///
/// # Why a window at all
///
/// The FFT treats its input as one period of an infinitely repeating
/// signal. A real buffer does not loop — the sine at its end meets the
/// sine at its start with a discontinuity, and a discontinuity is, to the
/// transform, energy at *every* frequency: one true pitch turns into a
/// smear across the whole spectrum ("leakage"). A window that fades the
/// buffer's ends to zero makes the loop seamless and the spectrum honest —
/// the price is a little width on each peak, which is why [`None`] remains
/// available for the buffers that genuinely do loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Window {
    /// No window. Correct only for signals that are periodic in the buffer
    /// (a whole number of cycles), where it is also the *sharpest* answer —
    /// a calibration tone synthesised at an exact bin frequency.
    #[default]
    None,
    /// The Hann window, `0.5·(1 − cos(2πt/T))`. The default recommendation
    /// of every spectral-analysis text, and the one to reach for when the
    /// buffer is an arbitrary slice of an ongoing signal.
    Hann,
}

impl Window {
    /// The window's coefficient at position `i` of `n`.
    fn coefficient(self, i: usize, n: usize) -> f32 {
        match self {
            Self::None => 1.0,
            Self::Hann => {
                #[allow(clippy::cast_precision_loss)] // n ≤ buffer sizes; exact in f32
                let t = i as f32 / n as f32;
                0.5 * (1.0 - (std::f32::consts::TAU * t).cos())
            }
        }
    }
}

/// The frequency content of one buffer: one magnitude per bin, half as many
/// bins as the analysed window, because a real signal's spectrum is
/// symmetric and the upper half says nothing new.
#[derive(Debug, Clone, PartialEq)]
pub struct Spectrum {
    /// The magnitude of each bin, linear (not dB), length `window / 2`.
    ///
    /// Linear because this crate's consumers are visual: a bar chart wants
    /// a linear height, and converting to dB is one `log` at the call site
    /// for the few that want it.
    magnitudes: Vec<f32>,
    /// The sample rate the buffer was recorded at — the thing that turns a
    /// bin *index* into a bin *pitch*.
    rate: u32,
    /// The window size the analysis used, in samples.
    window: usize,
}

impl Spectrum {
    /// Analyse a buffer's frequencies.
    ///
    /// The analysed window is the largest power of two that fits the
    /// buffer (see the module docs for why the caller does not choose it),
    /// downmixed to mono first — a stereo buffer's left and right are two
    /// performances of the same material, and the *spectrum* of a stereo
    /// mix is the spectrum of their sum.
    ///
    /// An empty or one-sample buffer has no frequencies to report and
    /// returns an empty spectrum; `bin_hz` on it never runs, because there
    /// is no bin to ask about.
    pub fn analyze(samples: &Samples, window: Window) -> Self {
        if samples.frames() < 2 {
            return Self {
                magnitudes: Vec::new(),
                rate: samples.rate,
                window: 0,
            };
        }
        // The largest power of two that fits, found by halving rather than
        // by floating-point log — `log2` and `pow2` round-tripping through
        // f32 is a place two implementations disagree by one bin.
        let mut size = 1;
        while size * 2 <= samples.frames() {
            size *= 2;
        }

        // Downmix to mono: the mean of the channels, per frame.
        let channels = samples.channels.max(1) as usize;
        let mut re = vec![0.0f32; size];
        for (i, slot) in re.iter_mut().enumerate() {
            let frame = &samples.data[i * channels..(i + 1) * channels];
            *slot = frame.iter().sum::<f32>() / channels as f32;
        }

        // Window, then transform. The imaginary part starts at zero: the
        // input is real, and the FFT fills it in as it goes.
        for (i, slot) in re.iter_mut().enumerate() {
            *slot *= window.coefficient(i, size);
        }
        let mut im = vec![0.0f32; size];
        fft(&mut re, &mut im);

        // Magnitudes of the meaningful half. The 1/n normalisation turns
        // the FFT's raw sums (which grow with n) into amplitudes a caller
        // can compare across window sizes.
        let half = size / 2;
        let mut magnitudes = Vec::with_capacity(half);
        for i in 0..half {
            // bin 0 is DC — its mirror is itself; every other bin has a
            // conjugate partner above Nyquist, so ×2 restores the energy
            // the half-spectrum dropped. Except DC and Nyquist themselves,
            // which have no partner; the distinction is one multiplication
            // nobody's bar chart can see, and the honest simple answer is
            // to leave them un-doubled.
            let scale = if i == 0 { 1.0 } else { 2.0 };
            magnitudes.push(scale * (re[i] * re[i] + im[i] * im[i]).sqrt() / size as f32);
        }

        Self {
            magnitudes,
            rate: samples.rate,
            window: size,
        }
    }

    /// The magnitudes, one per bin, linear — half as many as the window.
    pub fn magnitudes(&self) -> &[f32] {
        &self.magnitudes
    }

    /// The window size the analysis used.
    pub fn window(&self) -> usize {
        self.window
    }

    /// The pitch of bin `i`, in Hz.
    ///
    /// # Panics
    ///
    /// If `i` is past the last bin — an out-of-range bin has no pitch, and
    /// a caller indexing a loop by a stale `window` should find out now
    /// rather than draw a bar at a frequency that does not exist.
    pub fn bin_hz(&self, i: usize) -> f32 {
        assert!(
            i < self.magnitudes.len(),
            "bin {i} of {} does not exist",
            self.magnitudes.len()
        );
        i as f32 * as_f32(self.rate) / self.window as f32
    }

    /// The loudest bin's index.
    ///
    /// An empty spectrum (an empty buffer) has no loudest bin, and `None`
    /// says so — a caller drawing a peak marker skips it rather than
    /// inventing a bin zero that has no meaning.
    pub fn peak_bin(&self) -> usize {
        let mut best = 0usize;
        let mut best_mag = f32::NEG_INFINITY;
        for (i, magnitude) in self.magnitudes.iter().enumerate() {
            if *magnitude > best_mag {
                best_mag = *magnitude;
                best = i;
            }
        }
        best
    }

    /// The total magnitude of the bins whose pitch falls in `lo..=hi` Hz —
    /// the "how much bass is in this" question, and the one a bar chart of
    /// K bands answers by calling it K times with K ranges.
    ///
    /// Bins outside the spectrum's range (above Nyquist, below DC) simply
    /// contribute nothing, which makes a band past the top of the spectrum
    /// zero rather than an error: a 5-band bar chart asked to run on a
    /// 8 kHz buffer has a top band that is legitimately silent.
    pub fn band_energy(&self, lo_hz: f32, hi_hz: f32) -> f32 {
        if self.magnitudes.is_empty() {
            return 0.0;
        }
        let mut total = 0.0;
        for i in 0..self.magnitudes.len() {
            let hz = self.bin_hz(i);
            if hz >= lo_hz && hz <= hi_hz {
                total += self.magnitudes[i];
            }
        }
        total
    }
}

/// The radix-2 Cooley–Tukey FFT, in place, real and imaginary parts passed
/// as separate slices of equal, power-of-two length.
///
/// The classic three stages: bit-reverse the order, then butterflies of
/// doubling width. Every twiddle factor is recomputed per butterfly — a
/// table would be faster, and a table is also a cache the caller cannot
/// see; for the window sizes a UI analyses (≤ 4096 is typical, 8192 is
/// generous) the recomputation is a rounding error against the transform
/// itself.
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    debug_assert_eq!(n, im.len());
    debug_assert!(n.is_power_of_two(), "radix-2 needs a power of two");
    if n <= 1 {
        return;
    }

    // Bit-reversal permutation: the butterfly stage reads its inputs at a
    // stride that halves each pass, which is the *reversed* bit order of
    // the natural one, and permuting once up front means every pass can
    // read its neighbours contiguously.
    let bits = n.trailing_zeros();
    for i in 0..n {
        let j = i.reverse_bits() >> (usize::BITS - bits);
        if j > i {
            re.swap(i, j);
            im.swap(i, j);
        }
    }

    // Butterflies: width 2, 4, 8, ... n. At width w, pairs `i` and `i + w/2`
    // combine through the twiddle e^{-2πi·k/w}.
    let mut width = 2;
    while width <= n {
        let half = width / 2;
        let angle = -std::f32::consts::TAU / width as f32;
        for start in (0..n).step_by(width) {
            for k in 0..half {
                let twiddle_re = (angle * k as f32).cos();
                let twiddle_im = (angle * k as f32).sin();
                let a = start + k;
                let b = a + half;
                // (re[b] + i·im[b]) × (twiddle_re + i·twiddle_im)
                let product_re = re[b] * twiddle_re - im[b] * twiddle_im;
                let product_im = re[b] * twiddle_im + im[b] * twiddle_re;
                re[b] = re[a] - product_re;
                im[b] = im[a] - product_im;
                re[a] += product_re;
                im[a] += product_im;
            }
        }
        width *= 2;
    }
}

/// `u32`→`f32` for sample rates — always small enough to be exact, and the
/// same spelling the rest of this crate uses.
#[allow(clippy::cast_precision_loss)] // sample rates: exact in f32
fn as_f32(value: u32) -> f32 {
    value as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Envelope, Mixer, Tone, Waveform};
    use std::time::Duration;

    /// Render a sine of `hz` for `seconds` at 44.1 kHz — the synthesis side
    /// of the round-trip tests below. `held` rather than `new`: a
    /// `Tone::new` is a zero-hold pulse, which is silence almost
    /// everywhere, and a spectrum of silence finds nothing.
    fn sine(hz: f32, seconds: f32) -> Samples {
        let tone = Tone::held(hz, Waveform::Sine, Duration::from_secs_f32(seconds));
        let frames = (44_100.0 * seconds) as usize;
        let mut data = Vec::with_capacity(frames);
        for i in 0..frames {
            let t = Duration::from_secs_f32(i as f32 / 44_100.0);
            data.push(0.8 * tone.at(t));
        }
        Samples::mono(data, 44_100)
    }

    #[test]
    fn a_dc_signal_is_all_bin_zero() {
        // The purest possible check of the transform: a constant has one
        // frequency, zero, and the FFT's job is to put all of it there.
        let data = vec![0.5f32; 4096];
        let spectrum = Spectrum::analyze(&Samples::mono(data, 44_100), Window::None);
        assert!(spectrum.magnitudes()[0] > 0.4);
        for (i, magnitude) in spectrum.magnitudes().iter().enumerate() {
            if i > 0 {
                assert!(*magnitude < 1e-3, "bin {i} should be silent: {magnitude}");
            }
        }
    }

    #[test]
    fn a_sine_lands_in_its_own_bin() {
        // 100 cycles in a 2048-sample window at 44.1 kHz = 2151.17 Hz, but
        // a *bin-aligned* test is stronger: 44100/2048 × 100 = 2153.32...
        // pick the frequency that is bin 100 exactly.
        let rate = 44_100u32;
        let window = 2048usize;
        let bin = 100usize;
        let hz = bin as f32 * rate as f32 / window as f32;
        // A whole number of cycles → no leakage, no window needed.
        let data: Vec<f32> = (0..window)
            .map(|i| (std::f32::consts::TAU * hz * i as f32 / rate as f32).sin())
            .collect();
        let spectrum = Spectrum::analyze(&Samples::mono(data, rate), Window::None);
        assert_eq!(spectrum.peak_bin(), bin, "the sine's own bin");
        assert!((spectrum.bin_hz(bin) - hz).abs() < 1e-3);
        // And it is *the* peak by a wide margin.
        let peak = spectrum.magnitudes()[bin];
        for (i, magnitude) in spectrum.magnitudes().iter().enumerate() {
            if i != bin {
                assert!(*magnitude < peak * 0.01, "bin {i} at {magnitude} vs {peak}");
            }
        }
    }

    #[test]
    fn a_hanned_arbitrary_sine_is_still_found() {
        // 440 Hz does not sit on a bin of any power-of-two window at
        // 44.1 kHz — this is the *leakage* case, and the Hann window is
        // what keeps the peak findable anyway.
        let spectrum = Spectrum::analyze(&sine(440.0, 0.2), Window::Hann);
        let peak_hz = spectrum.bin_hz(spectrum.peak_bin());
        assert!(
            (peak_hz - 440.0).abs() < 12.0,
            "peak at {peak_hz}, not near 440"
        );
    }

    #[test]
    fn the_window_size_is_the_largest_power_of_two_that_fits() {
        // 3000 frames: 2048 fits, 4096 does not.
        let spectrum = Spectrum::analyze(&sine(440.0, 3000.0 / 44_100.0), Window::Hann);
        assert_eq!(spectrum.window(), 2048);
        assert_eq!(spectrum.magnitudes().len(), 1024);
    }

    #[test]
    fn nyquist_sits_just_above_the_last_bin() {
        // The kept bins run from DC to one below Nyquist (the Nyquist bin
        // itself is the spectrum's edge case and visuals never read it), so
        // the last bin is *near* 22 050 by construction: within two
        // spacings of it, at the window this test renders.
        let spectrum = Spectrum::analyze(&sine(440.0, 0.1), Window::Hann);
        let last = spectrum.magnitudes().len() - 1;
        let top = spectrum.bin_hz(last);
        let spacing = 44_100.0 / spectrum.window() as f32;
        assert!((22_050.0 - top) < spacing * 2.0, "top bin at {top}");
    }

    #[test]
    fn stereo_is_downmixed_not_left_only() {
        // The same sine in both channels — unattenuated — analyses
        // identically to the mono original: the downmix is the *mean*, so
        // duplicating a channel into both halves of a stereo frame leaves
        // the mean, and the spectrum, unchanged.
        let mono = sine(440.0, 0.1);
        let mut stereo_data = Vec::with_capacity(mono.data.len() * 2);
        for sample in &mono.data {
            stereo_data.push(*sample);
            stereo_data.push(*sample);
        }
        let stereo = Samples::stereo(stereo_data, 44_100);
        let a = Spectrum::analyze(&mono, Window::Hann);
        let b = Spectrum::analyze(&stereo, Window::Hann);
        for (x, y) in a.magnitudes().iter().zip(b.magnitudes().iter()) {
            assert!((x - y).abs() < 1e-4, "{x} vs {y}");
        }
    }

    #[test]
    fn the_band_containing_the_note_holds_the_energy() {
        let spectrum = Spectrum::analyze(&sine(440.0, 0.2), Window::Hann);
        let around = spectrum.band_energy(300.0, 600.0);
        let elsewhere = spectrum.band_energy(1000.0, 10_000.0);
        assert!(around > 0.1, "the band with the note has energy: {around}");
        assert!(
            around > elsewhere * 10.0,
            "the note's band dominates the rest: {around} vs {elsewhere}"
        );
        // A band beyond Nyquist is legitimately silent, not an error.
        assert_eq!(spectrum.band_energy(30_000.0, 40_000.0), 0.0);
    }

    #[test]
    fn an_empty_buffer_is_an_empty_spectrum() {
        let spectrum = Spectrum::analyze(&Samples::empty(44_100), Window::Hann);
        assert!(spectrum.magnitudes().is_empty());
        assert_eq!(spectrum.peak_bin(), 0);
        assert_eq!(spectrum.band_energy(0.0, 20_000.0), 0.0);
    }

    #[test]
    fn analysis_is_deterministic() {
        let samples = sine(1234.0, 0.05);
        let a = Spectrum::analyze(&samples, Window::Hann);
        let b = Spectrum::analyze(&samples, Window::Hann);
        assert_eq!(a, b);
    }

    #[test]
    fn two_different_notes_are_told_apart() {
        let low = Spectrum::analyze(&sine(220.0, 0.2), Window::Hann);
        let high = Spectrum::analyze(&sine(1760.0, 0.2), Window::Hann);
        let low_hz = low.bin_hz(low.peak_bin());
        let high_hz = high.bin_hz(high.peak_bin());
        assert!((low_hz - 220.0).abs() < 12.0);
        assert!((high_hz - 1760.0).abs() < 24.0);
        assert!(high_hz > low_hz * 4.0);
    }

    #[test]
    #[should_panic(expected = "does not exist")]
    fn an_out_of_range_bin_is_loud() {
        // 0.1 s at 44.1 kHz is 4410 frames → a 4096 window → 2048 bins,
        // so 2048 is the first bin that does not exist.
        let spectrum = Spectrum::analyze(&sine(440.0, 0.1), Window::Hann);
        let _ = spectrum.bin_hz(2048);
    }

    #[test]
    fn the_mixer_round_trip_finds_both_notes() {
        // The full-crate round trip: two tones mixed, and both *found in
        // the analysis* — the test that says synthesis and analysis agree.
        //
        // Not by `peak_bin`, which is a bin-alignment lottery (a note that
        // happens to sit on a bin concentrates its energy into one bucket
        // and always wins it), but by band energy: the band holding each
        // note must dominate a band holding neither.
        let hold = Duration::from_millis(300);
        let shape =
            || Envelope::attack_release(Duration::from_millis(5), Duration::from_millis(50));
        let chord = Mixer::new(44_100)
            .add(
                Tone::held(440.0, Waveform::Sine, hold).envelope(shape()),
                0.5,
            )
            .add(
                Tone::held(1318.0, Waveform::Sine, hold).envelope(shape()),
                0.5,
            );
        let rendered = chord.render();
        let spectrum = Spectrum::analyze(&rendered, Window::Hann);

        let note_a = spectrum.band_energy(420.0, 460.0);
        let note_b = spectrum.band_energy(1290.0, 1350.0);
        let quiet_a = spectrum.band_energy(600.0, 1200.0);
        let quiet_b = spectrum.band_energy(2000.0, 4000.0);
        assert!(note_a > quiet_a * 10.0, "440 Hz: {note_a} vs {quiet_a}");
        assert!(note_b > quiet_b * 10.0, "1318 Hz: {note_b} vs {quiet_b}");
        // And the two notes are the two loudest things in the buffer.
        assert!(note_a > 0.01 && note_b > 0.01);
    }
}
