//! Time-indexed animation tracks, building on ViewW's allocation-conscious
//! interpolation and curve types.

use std::time::Duration;

use vieww_animation::{Curve, Lerp};

use crate::{Quat, Transform3, Vec2, Vec3};

// These implementations connect the existing ViewW tween contract to spatial
// values without making the animation crate depend upward on the 3D layer.
impl Lerp for Vec2 {
    fn lerp(self, other: Self, t: f32) -> Self {
        Vec2::lerp(self, other, t)
    }
}
impl Lerp for Vec3 {
    fn lerp(self, other: Self, t: f32) -> Self {
        Vec3::lerp(self, other, t)
    }
}
impl Lerp for Quat {
    fn lerp(self, other: Self, t: f32) -> Self {
        self.slerp(other, t)
    }
}
impl Lerp for Transform3 {
    fn lerp(self, other: Self, t: f32) -> Self {
        self.interpolate(other, t)
    }
}

/// How a timeline behaves after its last keyframe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackMode {
    /// Hold the first/last value outside the authored range.
    #[default]
    Once,
    /// Restart at the first keyframe after reaching the end.
    Repeat,
    /// Alternate forwards and backwards without a discontinuity at the ends.
    PingPong,
}

/// A keyframe; `curve` eases the segment from this keyframe to the next.
#[derive(Debug, Clone)]
pub struct Keyframe<T> {
    pub at: Duration,
    pub value: T,
    pub curve: Curve,
}
impl<T> Keyframe<T> {
    #[must_use]
    pub fn new(at: Duration, value: T) -> Self {
        Self {
            at,
            value,
            curve: Curve::Linear,
        }
    }
    #[must_use]
    pub fn with_curve(mut self, curve: Curve) -> Self {
        self.curve = curve;
        self
    }
}

/// A stable, sorted keyframe track with no runtime clock access.
///
/// `sample` is a pure function of the supplied timestamp; this makes timelines
/// deterministic in unit tests, replays, scrubbing and frame-rate changes.
#[derive(Debug, Clone)]
pub struct AnimationTrack<T> {
    keyframes: Vec<Keyframe<T>>,
    playback: PlaybackMode,
}
impl<T: Lerp> Default for AnimationTrack<T> {
    fn default() -> Self {
        Self {
            keyframes: Vec::new(),
            playback: PlaybackMode::Once,
        }
    }
}
impl<T: Lerp> AnimationTrack<T> {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace a keyframe at its timestamp. Equal timestamps replace
    /// the previous value instead of creating a zero-duration segment.
    pub fn insert(&mut self, keyframe: Keyframe<T>) {
        match self
            .keyframes
            .binary_search_by_key(&keyframe.at, |item| item.at)
        {
            Ok(index) => self.keyframes[index] = keyframe,
            Err(index) => self.keyframes.insert(index, keyframe),
        }
    }

    #[must_use]
    pub fn playback(mut self, mode: PlaybackMode) -> Self {
        self.playback = mode;
        self
    }
    #[must_use]
    pub fn playback_mode(&self) -> PlaybackMode {
        self.playback
    }
    #[must_use]
    pub fn keyframes(&self) -> &[Keyframe<T>] {
        &self.keyframes
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keyframes.is_empty()
    }
    #[must_use]
    pub fn duration(&self) -> Option<Duration> {
        Some(
            self.keyframes
                .last()?
                .at
                .saturating_sub(self.keyframes[0].at),
        )
    }

    /// Sample a track at an absolute elapsed time.
    #[must_use]
    pub fn sample(&self, elapsed: Duration) -> Option<T> {
        let first = self.keyframes.first()?;
        if self.keyframes.len() == 1 {
            return Some(first.value.clone());
        }
        let last = self.keyframes.last().expect("at least two keyframes");
        let start = first.at;
        let span = last.at.saturating_sub(start);
        if span.is_zero() {
            return Some(last.value.clone());
        }
        let relative = elapsed.saturating_sub(start);
        let sample_time = match self.playback {
            PlaybackMode::Once => relative.min(span),
            PlaybackMode::Repeat => duration_from_nanos(relative.as_nanos() % span.as_nanos()),
            PlaybackMode::PingPong => {
                let double = span.as_nanos().saturating_mul(2);
                let phase = relative.as_nanos() % double;
                duration_from_nanos(if phase <= span.as_nanos() {
                    phase
                } else {
                    double - phase
                })
            }
        } + start;
        if sample_time <= first.at {
            return Some(first.value.clone());
        }
        if sample_time >= last.at {
            return Some(last.value.clone());
        }
        let upper = self
            .keyframes
            .partition_point(|keyframe| keyframe.at < sample_time);
        let left = &self.keyframes[upper - 1];
        let right = &self.keyframes[upper];
        let segment = right.at.saturating_sub(left.at);
        if segment.is_zero() {
            return Some(right.value.clone());
        }
        let numerator = sample_time.saturating_sub(left.at).as_secs_f64();
        let denominator = segment.as_secs_f64();
        let progress = left.curve.transform((numerator / denominator) as f32);
        if !progress.is_finite() {
            return None;
        }
        Some(left.value.clone().lerp(right.value.clone(), progress))
    }
}

fn duration_from_nanos(nanos: u128) -> Duration {
    let seconds = nanos / 1_000_000_000;
    let subsecond = (nanos % 1_000_000_000) as u32;
    Duration::new(seconds.min(u64::MAX as u128) as u64, subsecond)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_intermediate_values_and_replaces_duplicate_timestamps() {
        let mut track = AnimationTrack::new();
        track.insert(Keyframe::new(Duration::ZERO, 0.0_f32));
        track.insert(Keyframe::new(Duration::from_secs(1), 10.0));
        track.insert(Keyframe::new(Duration::from_secs(1), 20.0));
        assert_eq!(track.sample(Duration::from_millis(500)), Some(10.0));
        assert_eq!(track.sample(Duration::from_secs(1)), Some(20.0));
        assert_eq!(track.keyframes().len(), 2);
    }

    fn invalid_easing(_: f32) -> f32 {
        f32::NAN
    }

    #[test]
    fn non_finite_custom_easing_is_rejected() {
        let mut track = AnimationTrack::new();
        track.insert(
            Keyframe::new(Duration::ZERO, 0.0_f32).with_curve(Curve::custom(invalid_easing)),
        );
        track.insert(Keyframe::new(Duration::from_secs(1), 1.0));
        assert_eq!(track.sample(Duration::from_millis(500)), None);
    }

    #[test]
    fn ping_pong_reverses_without_a_clock_dependency() {
        let track = AnimationTrack::new().playback(PlaybackMode::PingPong);
        let mut track = track;
        track.insert(Keyframe::new(Duration::ZERO, 0.0_f32));
        track.insert(Keyframe::new(Duration::from_secs(2), 2.0));
        assert_eq!(track.sample(Duration::from_secs(1)), Some(1.0));
        assert_eq!(track.sample(Duration::from_secs(3)), Some(1.0));
        assert_eq!(track.sample(Duration::from_secs(4)), Some(0.0));
    }
}
