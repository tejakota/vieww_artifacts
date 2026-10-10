//! Effects — filters, reverbs, modulation effects, EQ, distortion and
//! dynamics. Every effect is a [`Processor`](crate::dsp::Processor) (mono,
//! one per channel) or a [`Stereo`] processor, so any of them drops into a
//! [`Chain`](crate::dsp::Chain).
//!
//! | Module | Contents |
//! |---|---|
//! | [`filter`] | SVF, ladder, one-pole, all-passes, combs, delay lines |
//! | [`fir`] | Kaiser FIR design, direct FIR, partitioned FFT convolution |
//! | [`reverb`] | Freeverb, an 8-line FDN with RT60 specification, convolution, synthetic IRs |
//! | [`modfx`] | chorus, flanger, phaser, tremolo, auto-pan, vibrato, ring mod |
//! | [`eq`] | parametric (Butterworth cuts), graphic (ISO), dynamic, linear-phase |
//! | [`distortion`] | waveshaper (Web Audio curves, Chebyshev, fold), bitcrusher, saturation, exciter, oversampling |
//! | [`dynamics`] | sidechain/lookahead compressor and expander, brick-wall limiter, gate, transient designer, LR4 crossovers, multiband |

pub mod distortion;
pub mod dynamics;
pub mod eq;
pub mod filter;
pub mod fir;
pub mod modfx;
pub mod reverb;

/// A stereo-in, stereo-out effect.
pub trait Stereo {
    /// Process one frame.
    fn process_stereo(&mut self, l: f32, r: f32) -> (f32, f32);

    /// Process an interleaved stereo buffer in place.
    fn run_stereo(&mut self, buf: &mut [f32]) {
        for f in buf.chunks_exact_mut(2) {
            let (l, r) = self.process_stereo(f[0], f[1]);
            f[0] = l;
            f[1] = r;
        }
    }
}
