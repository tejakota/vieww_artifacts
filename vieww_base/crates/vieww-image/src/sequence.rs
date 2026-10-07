//! Animated images: decoding a GIF's frames and stepping through them by
//! elapsed time.
//!
//! # Real decode, by vieww's own GIF decoder
//!
//! [`crate::codec::gif`] does the LZW, the palettes and — critically — the
//! compositing of each frame against the ones before it according to its
//! disposal method, so what [`AnimatedImage::decode_gif`] receives is
//! already a full, canvas-sized RGBA8 frame. This module turns that frame
//! sequence into [`vieww_foundation::Image`]s and answers "which frame is
//! showing at time T", which is what an animated widget needs every tick.

use std::fmt;
use std::time::Duration;

use vieww_foundation::Image;

/// One decoded frame: its pixels and how long it stays on screen.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub image: Image,
    pub delay: Duration,
}

/// A decoded animated image: its frames, in playback order.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimatedImage {
    frames: Vec<Frame>,
}

/// Why a byte sequence could not be decoded as an animated image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SequenceError {
    /// The decoder rejected the bytes.
    Decode(String),
    /// It decoded, but produced zero frames — not a usable animation.
    NoFrames,
}

impl fmt::Display for SequenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(message) => write!(f, "decoding an animated image: {message}"),
            Self::NoFrames => write!(f, "decoded an animated image with zero frames"),
        }
    }
}

impl std::error::Error for SequenceError {}

impl AnimatedImage {
    /// Decode an animated GIF from its encoded bytes.
    ///
    /// # Errors
    ///
    /// [`SequenceError::Decode`] if the bytes are not a GIF this build's
    /// decoder can read; [`SequenceError::NoFrames`] if they decode to an
    /// animation with no frames at all (a technically valid but useless
    /// GIF — one with a global palette and no image blocks).
    pub fn decode_gif(bytes: &[u8]) -> Result<Self, SequenceError> {
        let gif = crate::codec::gif::decode(bytes)
            .map_err(|error| SequenceError::Decode(error.to_string()))?;
        if gif.frames.is_empty() {
            return Err(SequenceError::NoFrames);
        }
        let (width, height) = (gif.width, gif.height);
        let frames = gif
            .frames
            .into_iter()
            .map(|frame| Frame {
                image: Image::from_rgba8(frame.rgba, width, height),
                // Centiseconds, exactly.
                delay: Duration::from_millis(u64::from(frame.delay_cs) * 10),
            })
            .collect();

        Ok(Self { frames })
    }

    /// The frames, in playback order.
    #[must_use]
    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    /// How many frames the animation has. Always at least one: construction
    /// fails with [`SequenceError::NoFrames`] otherwise.
    #[must_use]
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// The total time for one full loop: the sum of every frame's delay.
    #[must_use]
    pub fn total_duration(&self) -> Duration {
        self.frames.iter().map(|frame| frame.delay).sum()
    }

    /// The frame showing at `elapsed` time into playback, looping forever.
    ///
    /// `elapsed` is wrapped modulo [`total_duration`](Self::total_duration),
    /// so this is correct for a widget that just keeps handing it the
    /// animation's total running time — there is no separate "did it loop"
    /// bookkeeping for a caller to get wrong.
    ///
    /// If every frame has a zero delay (a malformed but not rejected GIF —
    /// "as fast as possible" in the GIF spec, which this does not attempt to
    /// rate-limit into some assumed minimum) the total duration is zero and
    /// this always returns the last frame, consistently rather than
    /// panicking on the impossible modulo.
    #[must_use]
    pub fn frame_at(&self, elapsed: Duration) -> &Frame {
        let total = self.total_duration();
        let wrapped = if total.is_zero() {
            Duration::ZERO
        } else {
            let nanos = elapsed.as_nanos() % total.as_nanos();
            Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
        };

        let mut accumulated = Duration::ZERO;
        for frame in &self.frames {
            accumulated += frame.delay;
            if wrapped < accumulated {
                return frame;
            }
        }
        self.frames
            .last()
            .expect("constructed with at least one frame")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::gif::encode;

    /// A tiny synthetic 2-frame GIF: a 2x2 solid-red frame held for 30ms,
    /// then a 2x2 solid-blue frame held for 50ms — encoded at test time by
    /// vieww's own GIF encoder, so both halves of the codec are exercised.
    fn two_frame_gif() -> Vec<u8> {
        encode(
            2,
            2,
            &[([255, 0, 0, 255].repeat(4), 3), ([0, 0, 255, 255].repeat(4), 5)],
            0,
        )
    }

    #[test]
    fn decoding_reproduces_the_exact_frame_count_colors_and_delays() {
        let animated = AnimatedImage::decode_gif(&two_frame_gif()).expect("decode");
        assert_eq!(animated.frame_count(), 2);

        let frames = animated.frames();
        assert_eq!((frames[0].image.width(), frames[0].image.height()), (2, 2));
        assert_eq!(frames[0].image.pixels(), &[255, 0, 0, 255].repeat(4)[..]);
        // GIF delay units are 10ms; 30 and 50 are exact multiples so the
        // round trip loses nothing.
        assert_eq!(frames[0].delay, Duration::from_millis(30));

        assert_eq!(frames[1].image.pixels(), &[0, 0, 255, 255].repeat(4)[..]);
        assert_eq!(frames[1].delay, Duration::from_millis(50));

        assert_eq!(animated.total_duration(), Duration::from_millis(80));
    }

    #[test]
    fn frame_at_selects_by_elapsed_time_within_one_loop() {
        let animated = AnimatedImage::decode_gif(&two_frame_gif()).expect("decode");
        let red = [255, 0, 0, 255].repeat(4);
        let blue = [0, 0, 255, 255].repeat(4);

        assert_eq!(animated.frame_at(Duration::ZERO).image.pixels(), &red[..]);
        assert_eq!(
            animated.frame_at(Duration::from_millis(29)).image.pixels(),
            &red[..]
        );
        assert_eq!(
            animated.frame_at(Duration::from_millis(30)).image.pixels(),
            &blue[..]
        );
        assert_eq!(
            animated.frame_at(Duration::from_millis(79)).image.pixels(),
            &blue[..]
        );
    }

    #[test]
    fn frame_at_wraps_around_after_a_full_loop() {
        let animated = AnimatedImage::decode_gif(&two_frame_gif()).expect("decode");
        let red = [255, 0, 0, 255].repeat(4);

        // Total duration is 80ms; 80ms and 85ms elapsed are 0ms and 5ms into
        // the second loop, both still inside the first (red) frame.
        assert_eq!(
            animated.frame_at(Duration::from_millis(80)).image.pixels(),
            &red[..]
        );
        assert_eq!(
            animated.frame_at(Duration::from_millis(85)).image.pixels(),
            &red[..]
        );

        let blue = [0, 0, 255, 255].repeat(4);
        // 160 + 30 = 190ms is two full loops (160ms) plus 30ms — right at
        // the boundary into the blue frame of the third loop.
        assert_eq!(
            animated.frame_at(Duration::from_millis(190)).image.pixels(),
            &blue[..]
        );
    }

    #[test]
    fn bytes_that_are_not_a_gif_fail_rather_than_panicking() {
        let error = AnimatedImage::decode_gif(b"not a gif").unwrap_err();
        assert!(matches!(error, SequenceError::Decode(_)), "{error}");
    }

    #[test]
    fn a_single_frame_gif_decodes_to_one_frame_and_never_wraps() {
        let bytes = encode(1, 1, &[(vec![9, 9, 9, 255], 1)], 0);

        let animated = AnimatedImage::decode_gif(&bytes).expect("decode");
        assert_eq!(animated.frame_count(), 1);
        assert_eq!(
            animated.frame_at(Duration::from_secs(1_000)).image,
            animated.frames()[0].image
        );
    }
}
