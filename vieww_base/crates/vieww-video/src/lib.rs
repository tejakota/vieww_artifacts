//! The video layer — frames, and the clock that walks them.
//!
//! # What this crate is, honestly
//!
//! **Sources, a player, and a frame clock.** A [`VideoSource`] is "frame *n*
//! is these pixels"; [`FrameSequence`] is the in-memory one a decoder
//! produces; [`GeneratedVideo`] paints deterministic test patterns on
//! demand; [`VideoPlayer`] is the state machine of playback — play, pause,
//! seek, rate, loop — advanced by frame deltas and sampled into a frame.
//!
//! What it is *not* is a codec. No H.264, no VP9, no container parsing:
//! those are platform work (Android's `MediaCodec`, the browser's
//! `<video>`), and the seam for them is exactly this crate's trait — a
//! platform crate implements [`VideoSource`] over its decoder and the
//! player, the seeking, the clock and the widget story work unchanged. That
//! division is the same one the framework draws everywhere else: the
//! *system* part is platform-shaped, and the *policy* part is
//! deterministic and testable, which is the part worth owning.
//!
//! # The one rule
//!
//! **Playback is a pure function of accumulated time.** [`VideoPlayer`]
//! never reads a clock; a frame loop hands it deltas via
//! [`advance`](VideoPlayer::advance) and reads
//! [`frame`](VideoPlayer::frame) — the same contract as the animation
//! crate, for the same reasons: a dropped frame skips ahead instead of
//! stretching, a test hands it a `Duration` instead of sleeping, and the
//! same input always produces the same frame, byte for byte.
//!
//! ```
//! use std::time::Duration;
//! use vieww_video::{GeneratedVideo, Pattern, VideoPlayer};
//!
//! // A 64x36, 10 fps, one-second test pattern.
//! let video = GeneratedVideo::new(64, 36, 10, Pattern::Sweep)
//!     .frames(10);
//! let mut player = VideoPlayer::over(&video);
//! player.play();
//!
//! // Half a second in is frame 5.
//! player.advance(Duration::from_millis(500));
//! assert_eq!(player.frame_index(), Some(5));
//!
//! // And it is the same frame every time.
//! let mut other = VideoPlayer::over(&video);
//! other.play();
//! for _ in 0..5 {
//!     other.advance(Duration::from_millis(100));
//! }
//! assert_eq!(other.frame_index(), Some(5));
//!
//! // Past the end without looping: the last frame holds.
//! player.advance(Duration::from_secs(10));
//! assert_eq!(player.frame_index(), Some(9));
//! assert!(!player.is_playing(), "and playback is over");
//! ```
//!
//! # Why frames, not timestamps
//!
//! A video is *n* frames at a fixed rate, and "which frame is at 3.7 s" is
//! arithmetic — `floor(3.7 * fps)` — not a query against a container's
//! timestamp table. Container timestamps exist because encoders write
//! frames out of order and at variable spacing; the moment video is
//! *decoded*, none of that survives, and pretending it does makes every
//! caller index with `as_frame_index` anyway.

use std::time::Duration;

use vieww_foundation::Image;

pub use vieww_foundation as foundation;

pub mod comp;
pub mod export;
pub mod matte;
pub mod track;

/// A frame's pixels, decoded and drawable.
///
/// `Image` from foundation, under a name that says what it is here. The
/// alias exists so this crate's docs read "frame" rather than "image" — the
/// pixel type is the same one the `Image` *widget* already draws, which is
/// the whole point: a video frame goes to screen through the still-image
/// path, and there is no second one to keep in agreement.
pub type Frame = Image;

/// Anything that can answer "frame *n*".
///
/// The contract is deliberately narrow — dimensions, rate, length, one
/// lookup — because it is the seam a platform decoder plugs into, and a
/// seam that demanded `send + Sync` or `async` would demand them of a
/// generated test pattern too. A real decoder's latency is hidden inside
/// its implementation (decode-ahead, a cache) rather than wired into the
/// trait.
pub trait VideoSource {
    /// Pixel dimensions of every frame. One pair for the whole video: a
    /// source that changed size mid-stream would be a source a layout
    /// engine cannot answer a question about.
    fn dimensions(&self) -> (u32, u32);

    /// Frames per second.
    fn frame_rate(&self) -> u32;

    /// How many frames the source holds.
    fn frame_count(&self) -> usize;

    /// The frame at `index`, or `None` past the end.
    ///
    /// `None` rather than a panic because "past the end" is a normal state
    /// of a player whose clock ran long, not a caller's bug.
    fn frame_at(&self, index: usize) -> Option<Frame>;

    /// How long the whole video plays.
    ///
    /// A default because it is arithmetic on the two things every source
    /// already declares, and making each implementation re-derive
    /// "frames / rate" is how one of them gets it subtly wrong.
    fn duration(&self) -> Duration {
        let seconds = self.frame_count() as f64 / f64::from(self.frame_rate());
        Duration::from_secs_f64(seconds.max(0.0))
    }
}

/// An in-memory video: the frames a decoder produced, held as pixels.
///
/// This is the honest shape of *decoded* video — an array of full frames —
/// and it is the type a platform crate's [`VideoSource`] implementation
/// feeds from its decode-ahead buffer. Small, because nothing here streams:
/// a two-hour film does not belong in memory, and a UI's clip does.
///
/// ```
/// use vieww_video::{FrameSequence, VideoSource};
/// use vieww_foundation::Image;
///
/// // Three 2x1 frames: red, green, blue.
/// let frames: Vec<Image> = [(255, 0, 0), (0, 255, 0), (0, 0, 255)]
///     .into_iter()
///     .map(|(r, g, b)| Image::from_rgba8(vec![r, g, b, 255, r, g, b, 255], 2, 1))
///     .collect();
///
/// let video = FrameSequence::new(frames, 3);
/// assert_eq!(video.frame_count(), 3);
/// assert_eq!(video.duration().as_millis(), 1000);
/// assert!(video.frame_at(2).is_some());
/// assert!(video.frame_at(3).is_none(), "past the end is None, not a panic");
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct FrameSequence {
    frames: Vec<Frame>,
    rate: u32,
}

impl FrameSequence {
    /// A video from decoded frames at `frame_rate`.
    ///
    /// # Panics
    ///
    /// If `frames` is empty or `frame_rate` is zero: a video with no frames
    /// has no duration, no first frame, and no answer to `dimensions` —
    /// states this type refuses to represent rather than making every
    /// caller handle. An empty video is the caller's "nothing to play",
    /// which belongs before construction, in the player.
    #[must_use]
    pub fn new(frames: Vec<Frame>, frame_rate: u32) -> Self {
        assert!(!frames.is_empty(), "a video has at least one frame");
        assert!(frame_rate > 0, "a video has a frame rate above zero");
        Self { frames, rate: frame_rate }
    }
}

impl VideoSource for FrameSequence {
    fn dimensions(&self) -> (u32, u32) {
        let first = &self.frames[0];
        (first.width(), first.height())
    }

    fn frame_rate(&self) -> u32 {
        self.rate
    }

    fn frame_count(&self) -> usize {
        self.frames.len()
    }

    fn frame_at(&self, index: usize) -> Option<Frame> {
        self.frames.get(index).cloned()
    }
}

/// A pattern [`GeneratedVideo`] can paint.
///
/// Test patterns rather than "a colour": a video whose every frame is one
/// flat colour cannot prove a player is advancing, because frame 3 and
/// frame 4 look identical. Each pattern here makes the frame index *legible
/// in the pixels* — a test can assert on them, and a demo screen shows
/// motion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pattern {
    /// A vertical bar sweeping left to right, wrapping: the bar's x is a
    /// function of the frame index, so "which frame" is readable from any
    /// pixel row. The default, because it is the one that reads at a
    /// glance.
    #[default]
    Sweep,
    /// A horizontal gradient whose phase advances per frame: smooth motion,
    /// no hard edges — for testing resampling or interpolation between
    /// frames.
    Drift,
    /// Frame counter encoded as brightness: frame *n* is grey `n * 255 /
    /// frames`, so the frame index is legible as a single number from any
    /// pixel.
    Counter,
}

/// A video that paints its frames from the frame index.
///
/// Deterministic by construction — frame 5 is the same pixels today and in
/// a year — which makes it the source for tests *and* for demos: a player
/// wired to a `GeneratedVideo` in a gallery shows real playback behaviour
/// with no asset pipeline and no license question. See [`Pattern`] for why
/// the patterns are what they are.
///
/// ```
/// use vieww_video::{GeneratedVideo, Pattern, VideoSource};
///
/// let video = GeneratedVideo::new(32, 18, 4, Pattern::Counter).frames(4);
/// assert_eq!(video.frame_count(), 4);
/// assert_eq!(video.dimensions(), (32, 18));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedVideo {
    width: u32,
    height: u32,
    rate: u32,
    frames: usize,
    pattern: Pattern,
}

impl GeneratedVideo {
    /// A generated video of one frame. Extend with [`frames`](Self::frames).
    #[must_use]
    pub const fn new(width: u32, height: u32, frame_rate: u32, pattern: Pattern) -> Self {
        Self {
            width,
            height,
            rate: frame_rate,
            frames: 1,
            pattern,
        }
    }

    /// Set the frame count.
    #[must_use]
    pub const fn frames(mut self, frames: usize) -> Self {
        self.frames = frames;
        self
    }

    /// The pattern being painted.
    #[must_use]
    pub const fn pattern(&self) -> Pattern {
        self.pattern
    }
}

impl VideoSource for GeneratedVideo {
    fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn frame_rate(&self) -> u32 {
        self.rate
    }

    fn frame_count(&self) -> usize {
        self.frames
    }

    fn frame_at(&self, index: usize) -> Option<Frame> {
        if index >= self.frames {
            return None;
        }
        let pixels = self.width * self.height * 4;
        let mut data = Vec::with_capacity(pixels as usize);

        match self.pattern {
            Pattern::Sweep => {
                // The bar sits at x = index/frames of the width, wrapping.
                let band = (self.width / 8).max(1);
                let head =
                    (u64::from(self.width) * index as u64 / self.frames.max(1) as u64) as u32;
                let head = if head >= self.width { 0 } else { head };
                for y in 0..self.height {
                    for x in 0..self.width {
                        let in_band = (x + self.width - head) % self.width < band;
                        // A dim grid so the frame's extent is visible even
                        // where the bar is not.
                        let grid = x % 8 == 0 || y % 8 == 0;
                        let (r, g, b) = if in_band {
                            (255, 255, 255)
                        } else if grid {
                            (48, 48, 48)
                        } else {
                            (12, 12, 12)
                        };
                        data.extend_from_slice(&[r, g, b, 255]);
                    }
                }
            }
            Pattern::Drift => {
                // Phase advances a quarter of the gradient per frame.
                let phase = (index % 4) as f32 / 4.0;
                for _y in 0..self.height {
                    for x in 0..self.width {
                        #[expect(clippy::cast_precision_loss, reason = "a gradient coordinate is exact enough in f32")]
                        let t = (x as f32 / self.width.max(1) as f32 + phase).fract();
                        let r = (t * 255.0) as u8;
                        let g = ((1.0 - t) * 255.0) as u8;
                        data.extend_from_slice(&[r, g, 128, 255]);
                    }
                }
            }
            Pattern::Counter => {
                // Brightness steps once per frame: any pixel reads the index.
                let step = if self.frames > 1 {
                    index as f32 / (self.frames - 1) as f32
                } else {
                    0.0
                };
                let grey = (step * 255.0).round() as u8;
                data.extend(
                    std::iter::repeat_n([grey, grey, grey, 255], (self.width * self.height) as usize)
                        .flatten(),
                );
            }
        }

        Some(Frame::from_rgba8(data, self.width, self.height))
    }
}

/// The state of a playback: playing or paused, where it is, how fast.
///
/// The clock advances by [`advance`](Self::advance) — deltas, not
/// timestamps — and every question ("which frame", "are we done") is
/// answered from that clock, so the same deltas always produce the same
/// playback. Looping is a policy on the clock, not a rewing side effect:
/// the position wraps and `loops` counts, which is how a caller shows
/// "played 3 times" without watching for the wrap itself.
///
/// ```
/// use std::time::Duration;
/// use vieww_video::{FrameSequence, VideoPlayer};
/// use vieww_foundation::Image;
///
/// let frames: Vec<Image> = (0..4)
///     .map(|_| Image::from_rgba8(vec![0, 0, 0, 255], 1, 1))
///     .collect();
/// let video = FrameSequence::new(frames, 2); // 2 fps: 2 seconds
///
/// let mut player = VideoPlayer::over(&video);
/// player.play().looping();
/// player.advance(Duration::from_millis(2500));
///
/// assert_eq!(player.frame_index(), Some(1), "wrapped to 0.5 s in");
/// assert_eq!(player.loops(), 1, "and counted the wrap");
/// ```
#[derive(Debug, Clone)]
pub struct VideoPlayer {
    /// Frames per second of the source, cached so the clock's arithmetic
    /// does not go through the trait on every advance.
    rate: f64,
    /// Seconds of playback elapsed, unwrapped — see `position`.
    elapsed: f64,
    /// Total seconds in the source, cached for the same reason.
    length: f64,
    /// The source's frame count, cached so "which frame" can say `None`
    /// without going back through the trait.
    count: usize,
    playing: bool,
    looped: bool,
    /// Times the position has wrapped past the end.
    wraps: u64,
    /// Playback speed: 1.0 real, 0.5 slow, 2.0 fast, negative rewinds.
    speed: f64,
}

impl VideoPlayer {
    /// A player at frame 0, paused, over `source`.
    #[must_use]
    pub fn over(source: &dyn VideoSource) -> Self {
        let rate = f64::from(source.frame_rate());
        Self {
            rate: rate.max(1.0),
            elapsed: 0.0,
            length: source.duration().as_secs_f64(),
            count: source.frame_count(),
            playing: false,
            looped: false,
            wraps: 0,
            speed: 1.0,
        }
    }

    /// Play from wherever the clock is. Unpausing is the same call.
    ///
    /// `&mut` rather than a consuming builder, because every control on a
    /// player happens *after* construction — a pause button, a loop toggle,
    /// a speed setting — and a builder that moves the player would hand
    /// each of those a player it has to give back. This is the
    /// `vieww_animation::AnimationController`
    /// spelling: mutate, chain in one statement, keep the binding.
    pub fn play(&mut self) -> &mut Self {
        self.playing = true;
        self
    }

    /// Pause. The clock holds its position; [`play`](Self::play) resumes
    /// from there rather than from zero, because "pause" that forgets where
    /// it was is a stop.
    pub fn pause(&mut self) -> &mut Self {
        self.playing = false;
        self
    }

    /// Loop when the end is reached. See [`loops`](Self::loops).
    pub fn looping(&mut self) -> &mut Self {
        self.looped = true;
        self
    }

    /// Set the playback speed. `1.0` is real time; `2.0` doubles the
    /// clock's advance; negative runs it backwards, which with looping
    /// makes a plausible "boomerang" and without it stops at frame 0.
    pub fn speed(&mut self, speed: f64) -> &mut Self {
        self.speed = speed;
        self
    }

    /// Whether the clock is running.
    #[must_use]
    pub const fn is_playing(&self) -> bool {
        self.playing
    }

    /// Whether the clock will wrap at the end.
    #[must_use]
    pub const fn is_looping(&self) -> bool {
        self.looped
    }

    /// Times playback has wrapped past the end. Zero when not looping.
    #[must_use]
    pub const fn loops(&self) -> u64 {
        self.wraps
    }

    /// The clock's position, in seconds, wrapped into the video's length.
    #[must_use]
    pub fn position(&self) -> Duration {
        Duration::from_secs_f64(self.position_seconds())
    }

    /// [`position`](Self::position) as seconds, for arithmetic callers.
    #[must_use]
    pub fn position_seconds(&self) -> f64 {
        if self.length <= 0.0 {
            return 0.0;
        }
        // A looping clock wraps; a finished one holds its endpoint. The
        // two disagree at exactly the boundary, which is the one place a
        // caller reads: a looping player at 2 s in a 2 s video is at 0, a
        // finished one is at the last frame.
        if self.looped {
            self.elapsed.rem_euclid(self.length)
        } else {
            self.elapsed.clamp(0.0, self.length)
        }
    }

    /// Jump the clock to `position` (seconds, clamped into the video).
    ///
    /// A seek while paused holds the new position — the same behaviour a
    /// scrubber wants — and one while playing simply continues from there.
    /// The wrap counter resets, because a seek is a decision that the
    /// playback history does not carry.
    pub fn seek(&mut self, position: Duration) -> &mut Self {
        self.elapsed = position.as_secs_f64().clamp(0.0, self.length.max(0.0));
        self.wraps = 0;
        self
    }

    /// Move the clock by `delta` of *real* time.
    ///
    /// A paused player ignores the delta — pause means the clock does not
    /// move, not that frames hold while time accrues to be paid back on
    /// resume.
    pub fn advance(&mut self, delta: Duration) -> &mut Self {
        if !self.playing {
            return self;
        }
        let before = self.elapsed;
        self.elapsed += delta.as_secs_f64() * self.speed;

        if self.length > 0.0 {
            // Count wraps for forward play crossing the end (repeatedly, for
            // a delta bigger than the whole video); backward play "wraps" to
            // the end and is counted the same way. Saturating, because the
            // two floors cannot invert but a guard that costs nothing is
            // cheaper than a proof read twice.
            if self.speed > 0.0 {
                self.wraps += ((self.elapsed / self.length).floor() as u64)
                    .saturating_sub((before / self.length).floor() as u64);
            } else if self.speed < 0.0 {
                self.wraps += ((before / self.length).ceil() as u64)
                    .saturating_sub((self.elapsed / self.length).ceil() as u64);
            }
            if self.looped {
                // `rem_euclid` wraps forward and backward: -0.1 s in a 2 s
                // video is 1.9 s, which is what a rewind-through-zero loop
                // means.
                self.elapsed = self.elapsed.rem_euclid(self.length);
            } else if self.elapsed > self.length || self.elapsed < 0.0 {
                // Not looping: the clock stops, exactly at the boundary, and
                // playback is over — a player whose clock ran on past the
                // end would report frames that do not exist.
                self.elapsed = self.elapsed.clamp(0.0, self.length);
                self.playing = false;
            }
        }
        self
    }

    /// Which frame the clock is on, or `None` if the source is empty.
    ///
    /// Frame 0 at the start and **the last frame at the end, exactly** — the
    /// boundaries are the two places a caller scrubs to and the two places
    /// an off-by-one hides, so the one at the end is the last frame the
    /// caller can see, not the one-past-the-end that a half-open position
    /// arithmetic produces.
    #[must_use]
    pub fn frame_index(&self) -> Option<usize> {
        if self.rate <= 0.0 || self.count == 0 {
            return None;
        }
        let index = (self.position_seconds() * self.rate).floor() as usize;
        Some(index.min(self.count - 1))
    }

    /// The frame the clock is on, as pixels, from `source`.
    ///
    /// `None` if the source has no frames — an empty video, which
    /// [`FrameSequence`] refuses and a platform decoder might still
    /// produce while it warms up.
    #[must_use]
    pub fn frame(&self, source: &dyn VideoSource) -> Option<Frame> {
        let index = self.frame_index()?;
        let count = source.frame_count();
        if count == 0 {
            return None;
        }
        source.frame_at(index.min(count - 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    fn flat(frames: usize, rate: u32) -> FrameSequence {
        let images: Vec<Frame> = (0..frames)
            .map(|_| Image::from_rgba8(vec![10, 20, 30, 255], 1, 1))
            .collect();
        FrameSequence::new(images, rate)
    }

    #[test]
    fn a_sequence_declares_itself() {
        let video = flat(6, 3);
        assert_eq!(video.dimensions(), (1, 1));
        assert_eq!(video.frame_rate(), 3);
        assert_eq!(video.frame_count(), 6);
        assert_eq!(video.duration(), ms(2000));
        assert!(video.frame_at(5).is_some());
        assert!(video.frame_at(6).is_none());
    }

    #[test]
    #[should_panic(expected = "at least one frame")]
    fn an_empty_sequence_is_refused() {
        let _ = FrameSequence::new(Vec::new(), 30);
    }

    #[test]
    #[should_panic(expected = "frame rate above zero")]
    fn a_zero_rate_is_refused() {
        let _ = FrameSequence::new(vec![Image::from_rgba8(vec![0; 4], 1, 1)], 0);
    }

    #[test]
    fn half_a_second_at_10fps_is_frame_5() {
        let video = GeneratedVideo::new(8, 8, 10, Pattern::Sweep).frames(10);
        let mut player = VideoPlayer::over(&video);
        player.play();

        player.advance(ms(500));
        assert_eq!(player.frame_index(), Some(5));
        assert_eq!(player.frame(&video).map(|frame| frame.width()), Some(8));
    }

    #[test]
    fn the_same_deltas_give_the_same_frame() {
        let video = flat(10, 10);
        let mut a = VideoPlayer::over(&video);
        let mut b = VideoPlayer::over(&video);
        a.play();
        b.play();

        a.advance(ms(373));
        for _ in 0..373 {
            b.advance(ms(1));
        }
        assert_eq!(a.frame_index(), b.frame_index());
        assert_eq!(a.position(), b.position());
    }

    #[test]
    fn a_paused_clock_does_not_accrue() {
        let video = flat(10, 10);
        let mut player = VideoPlayer::over(&video);
        player.play();

        player.advance(ms(200));
        player.pause();
        player.advance(ms(5000));
        assert_eq!(player.frame_index(), Some(2), "pause held position");

        player.play();
        assert_eq!(player.frame_index(), Some(2), "and resume continued from it");
        player.advance(ms(100));
        assert_eq!(player.frame_index(), Some(3));
    }

    #[test]
    fn without_looping_the_last_frame_holds() {
        let video = flat(4, 2); // 2 seconds
        let mut player = VideoPlayer::over(&video);
        player.play();

        player.advance(ms(2500));
        assert!(!player.is_playing(), "playback is over");
        assert_eq!(player.position(), ms(2000), "clamped at the end");
        assert_eq!(
            player.frame_index(),
            Some(3),
            "the last frame is what is on screen at the end"
        );

        // And it is the last frame's pixels, not `None`.
        assert_eq!(player.frame(&video).map(|frame| frame.height()), Some(1));
        assert!(video.frame_at(3).is_some());
    }

    #[test]
    fn looping_wraps_and_counts() {
        let video = flat(4, 2); // 2 seconds
        let mut player = VideoPlayer::over(&video);
        player.play().looping();

        player.advance(ms(2500));
        assert_eq!(player.loops(), 1);
        assert_eq!(player.position(), ms(500));
        assert!(player.is_playing(), "a loop never ends itself");

        // A delta bigger than the whole video wraps several times.
        player.advance(ms(6500));
        assert_eq!(player.loops(), 4, "0.5 s + 6.5 s = 7 s = three more wraps");
        assert_eq!(player.position(), ms(1000), "7 s in a 2 s loop is 1 s");
    }

    #[test]
    fn speed_two_plays_twice_as_fast() {
        let video = flat(10, 10);
        let mut normal = VideoPlayer::over(&video);
        let mut fast = VideoPlayer::over(&video);
        normal.play();
        fast.play().speed(2.0);

        normal.advance(ms(300));
        fast.advance(ms(300));
        assert_eq!(normal.frame_index(), Some(3));
        assert_eq!(fast.frame_index(), Some(6), "the clock doubled");
    }

    #[test]
    fn negative_speed_rewinds_and_wraps_backwards() {
        let video = flat(10, 10); // 1 second
        let mut player = VideoPlayer::over(&video);
        player.play().speed(-1.0).looping();

        player.seek(ms(800));
        player.advance(ms(500));
        assert!(player.is_playing(), "a rewind loop keeps going");
        // 0.8 - 0.5 = 0.3, no wrap yet.
        assert_eq!(player.position(), ms(300));
        assert_eq!(player.loops(), 0);

        player.advance(ms(600));
        // 0.3 - 0.6 = -0.3 → wrapped to 0.7, one backward wrap.
        assert_eq!(player.position(), ms(700));
        assert_eq!(player.loops(), 1);
    }

    #[test]
    fn without_looping_negative_speed_stops_at_zero() {
        let video = flat(10, 10);
        let mut player = VideoPlayer::over(&video);
        player.play().speed(-1.0);

        player.seek(ms(300));
        player.advance(ms(1000));
        assert!(!player.is_playing(), "the rewind stopped at the start");
        assert_eq!(player.position(), Duration::ZERO);
    }

    #[test]
    fn seek_resets_the_wrap_count_and_clamps() {
        let video = flat(4, 2);
        let mut player = VideoPlayer::over(&video);
        player.play().looping();

        player.advance(ms(5000));
        assert_eq!(player.loops(), 2);

        player.seek(ms(100));
        assert_eq!(player.loops(), 0);
        assert_eq!(player.position(), ms(100));
    }

    #[test]
    fn a_player_over_nothing_reports_none() {
        // A source with zero frames: legal for the trait, refused by
        // FrameSequence, and handled by the player as "nothing to show".
        struct Empty;
        impl VideoSource for Empty {
            fn dimensions(&self) -> (u32, u32) {
                (0, 0)
            }
            fn frame_rate(&self) -> u32 {
                1
            }
            fn frame_count(&self) -> usize {
                0
            }
            fn frame_at(&self, index: usize) -> Option<Frame> {
                let _ = index;
                None
            }
        }

        let empty = Empty;
        let mut player = VideoPlayer::over(&empty);
        player.play();
        player.advance(ms(100));
        assert_eq!(player.frame_index(), None);
        assert_eq!(player.frame(&empty), None);
        assert_eq!(player.position(), Duration::ZERO);
    }

    #[test]
    fn generated_patterns_make_the_index_legible() {
        let sweep = GeneratedVideo::new(16, 4, 1, Pattern::Sweep).frames(2);
        let a = sweep.frame_at(0).expect("frame 0");
        let b = sweep.frame_at(1).expect("frame 1");
        assert_ne!(a.pixels(), b.pixels(), "the bar moved");

        let counter = GeneratedVideo::new(2, 2, 1, Pattern::Counter).frames(3);
        let first = counter.frame_at(0).expect("frame 0").pixels().to_vec();
        let last = counter.frame_at(2).expect("frame 2").pixels().to_vec();
        assert_eq!(first[0], 0, "frame 0 is black");
        assert_eq!(last[0], 255, "the last frame is white");

        // Determinism: the same frame, twice, is the same bytes.
        let again = counter.frame_at(2).expect("frame 2, again");
        assert_eq!(last, again.pixels());
    }

    #[test]
    fn a_generated_video_declares_itself() {
        let video = GeneratedVideo::new(64, 36, 10, Pattern::Drift).frames(10);
        assert_eq!(video.dimensions(), (64, 36));
        assert_eq!(video.frame_rate(), 10);
        assert_eq!(video.frame_count(), 10);
        assert_eq!(video.duration(), Duration::from_secs(1));
        assert_eq!(video.pattern(), Pattern::Drift);
    }
}
