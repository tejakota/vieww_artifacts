//! Effects — filters, reverbs, modulation effects, EQ, distortion and
//! dynamics. Every effect is a [`Processor`](crate::dsp::Processor) (mono,
//! one per channel) or a [`Stereo`] processor, so any of them drops into a
//! [`Chain`](crate::dsp::Chain) or a node of the [`graph`](crate::graph).

pub mod filter;

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
