//! The audio layer — waveforms, and the seam a speaker plugs into.
//!
//! # What this crate is, honestly
//!
//! **Synthesis, mixing, WAV and the service seam.** Sine waves, envelopes, a
//! mixer that sums sources into one buffer, a RIFF/WAVE reader and writer,
//! and an [`AudioPlayer`] trait a platform crate registers so sound can come
//! out of a device. The comparison table that listed "no audio crate" as a
//! gap is answered by exactly that, and by nothing it does not say: there is
//! no MP3/OGG decoder, no 3D spatialisation, no streaming — because a
//! decoder is a codec (a whole discipline), and this layer's job is to be
//! the *shape* audio has in a UI framework: a click on a button, a chime on
//! a notification, a tone under an animation.
//!
//! # The rule that shapes everything here
//!
//! **Every type is a function of time, and time arrives as a sample rate.**
//! A [`Tone`] is a phase function; an [`Envelope`] is an amplitude function;
//! a [`Mixer`] sums them at an index; nothing reads a clock and nothing
//! blocks. The consequence is the same one the animation crate draws from
//! the same rule: to test any of this you hand it a `Duration` or an index,
//! and the result is byte-for-byte reproducible — which for audio is not a
//! nicety but the *definition* of a deterministic mixdown.
//!
//! ```
//! use vieww_audio::{Envelope, Mixer, Tone, Waveform};
//! use std::time::Duration;
//!
//! // A UI "click": 2 kHz sine, 5 ms attack, 40 ms release.
//! let click = Tone::new(2_000.0, Waveform::Sine)
//!     .envelope(Envelope::attack_release(
//!         Duration::from_millis(5),
//!         Duration::from_millis(40),
//!     ));
//!
//! let mixer = Mixer::new(44_100).add(click, 0.5);
//! let rendered = mixer.render();
//!
//! assert_eq!(rendered.channels, 1);
//! assert_eq!(rendered.rate, 44_100);
//! // Loudest somewhere near the attack's end, silent by the release's end.
//! assert!(rendered.peak() > 0.2);
//! assert!(rendered.amplitude_at(Duration::from_millis(49)) < 0.01);
//! ```
//!
//! # Layering
//!
//! This crate depends on [`vieww_foundation`] for one thing:
//! [`ServiceError`](vieww_foundation::service::ServiceError), the vocabulary
//! the [`service`](vieww_foundation::service) seam already speaks. The
//! [`AudioPlayer`] trait here is defined by *this* crate rather than by
//! foundation's `capability` module on purpose — the seam's rule is that a
//! third party can add a service without editing foundation, and an audio
//! layer added by this very repository is the first proof that the rule
//! works for a real sense-and-control capability and not just key-value
//! stores. A platform crate (`vieww-platform-winit` on desktop, the JNI
//! bridge on Android) provides the implementation and registers it.
//!
//! # The unavailable story
//!
//! [`NoAudio`] is the implementation that says no: every call returns
//! [`ServiceError::Unsupported`](vieww_foundation::service::ServiceError::Unsupported).
//! That is the capability module's convention — a device-shaped capability
//! cannot be faked into correctness, and a silent success would ship an
//! application whose sounds simply never play. [`RecordingPlayer`] is the
//! *test* implementation: it records every command, plays nothing, and
//! exists so a widget's wiring — "the press called `play`, the unmount
//! called `stop`" — can be asserted without a sound card.

use std::time::Duration;

/// `u32` has no `Into<f32>`, and the sample-rate arithmetic here is
/// `rate as f32` at every site it touches. One helper, one cast annotation,
/// rather than the same `#[expect]` on eight functions.
#[expect(clippy::cast_precision_loss, reason = "a 192 kHz rate is exact in f32")]
const fn as_f32(rate: u32) -> f32 {
    rate as f32
}

pub mod analysis;
pub mod device;
pub mod dsp;
pub mod midi;
pub mod player;
pub mod spatial;
pub mod wav;

pub use player::{AudioPlayer, NoAudio, PlaybackHandle, RecordedCommand, RecordingPlayer, Sound};
pub use wav::{read_wav, write_wav, PcmError, PcmSamples};

pub use vieww_foundation as foundation;

/// A mono or interleaved-stereo buffer of samples, the crate's common
/// currency.
///
/// Every path in this crate ends in one of these: a [`Tone`] rendered, a
/// [`Mixer`] summed, a [`wav`] file decoded. Samples are `f32` in `-1..=1`
/// per channel — the convention every audio API in wide use settles on —
/// and `channels` says how to walk `data`: mono is one value per frame,
/// stereo is two, and nothing above two is supported (see the type's
/// `channels` field docs for why).
#[derive(Debug, Clone, PartialEq)]
pub struct Samples {
    /// The interleaved samples: `[L, R, L, R, ...]` for stereo.
    pub data: Vec<f32>,
    /// 1 for mono, 2 for stereo.
    ///
    /// Not `usize` unbounded: a 5.1 layout is six mono buffers interleaved
    /// in an order two standards disagree about, and a type that pretended
    /// to carry it would be lying about what a UI framework's audio does.
    /// Mono and stereo cover every sound a UI makes.
    pub channels: u8,
    /// Samples per second, per channel.
    pub rate: u32,
}

impl Samples {
    /// An empty buffer at `rate`, mono.
    #[must_use]
    pub const fn empty(rate: u32) -> Self {
        Self {
            data: Vec::new(),
            channels: 1,
            rate,
        }
    }

    /// A mono buffer from raw samples.
    #[must_use]
    pub fn mono(data: Vec<f32>, rate: u32) -> Self {
        Self {
            data,
            channels: 1,
            rate,
        }
    }

    /// A stereo buffer from interleaved `[L, R, ...]` samples.
    ///
    /// # Panics
    ///
    /// If `data` has an odd length — half an `[L, R]` frame is a buffer
    /// whose last frame reads past the end, and that is a caller bug worth
    /// a panic rather than a silent one-frame error.
    #[must_use]
    pub fn stereo(data: Vec<f32>, rate: u32) -> Self {
        assert!(
            data.len().is_multiple_of(2),
            "a stereo buffer is whole [L, R] frames; {} samples is {} and a half",
            data.len(),
            data.len() / 2
        );
        Self {
            data,
            channels: 2,
            rate,
        }
    }

    /// The number of frames (a frame is one sample per channel).
    #[must_use]
    pub fn frames(&self) -> usize {
        self.data.len() / self.channels as usize
    }

    /// How long the buffer sounds for.
    #[must_use]
    pub fn duration(&self) -> Duration {
        Duration::from_secs_f32(self.frames() as f32 / as_f32(self.rate))
    }

    /// The loudest sample, in `0..=1` per channel — the number a "clipping
    /// yet?" check reads.
    #[must_use]
    pub fn peak(&self) -> f32 {
        self.data
            .iter()
            .map(|sample| sample.abs())
            .fold(0.0, f32::max)
    }

    /// The amplitude at `at`: the peak of the frame there, or `0.0` past the
    /// end.
    ///
    /// A silence check on a rendered envelope is the assertion every
    /// envelope test wants to make, and doing it by duration rather than by
    /// index keeps the test honest about *when* rather than *where*.
    #[must_use]
    pub fn amplitude_at(&self, at: Duration) -> f32 {
        let frame = at.as_secs_f32() * as_f32(self.rate);
        let frame = frame as usize;
        if frame >= self.frames() {
            return 0.0;
        }
        let channels = self.channels as usize;
        self.data[frame * channels..(frame + 1) * channels]
            .iter()
            .map(|sample| sample.abs())
            .fold(0.0, f32::max)
    }

    /// Sum `other` into `self`, frame by frame, saturating at ±1.
    ///
    /// The addition a mixer does, exposed on the buffer itself for callers
    /// layering pre-rendered material. `other` plays from `at` seconds in
    /// (rounded to the nearest frame), so a sound layered at 200 ms has 200
    /// ms of the first buffer alone before the second joins. Nothing is
    /// time-stretched: buffers at different sample rates are refused, since
    /// resampling is a whole filter design and "silently wrong pitch" is
    /// what pretending otherwise buys.
    ///
    /// # Errors
    ///
    /// [`MixError::RateMismatch`] if the rates differ.
    pub fn overdub(&mut self, other: &Samples, at: Duration) -> Result<(), MixError> {
        if self.channels != other.channels {
            return Err(MixError::ChannelMismatch);
        }
        if self.rate != other.rate {
            return Err(MixError::RateMismatch);
        }
        let channels = self.channels as usize;
        let start = (at.as_secs_f32() * as_f32(self.rate)).round() as usize;
        let end = start + other.frames();
        if end > self.frames() {
            self.data.resize(end * channels, 0.0);
        }
        for (frame, sample) in other.data.iter().enumerate() {
            let index = start * channels + frame;
            self.data[index] = (self.data[index] + sample).clamp(-1.0, 1.0);
        }
        Ok(())
    }
}

/// Why two buffers could not be mixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixError {
    /// The sample rates differ; mixing them would resample, which this
    /// crate deliberately does not do.
    RateMismatch,
    /// The channel counts differ; downmixing is a policy decision the caller
    /// should make on purpose, not one a sum should make by accident.
    ChannelMismatch,
}

impl std::fmt::Display for MixError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RateMismatch => f.write_str("sample rates differ; resample first"),
            Self::ChannelMismatch => f.write_str("channel counts differ; downmix first"),
        }
    }
}

impl std::error::Error for MixError {}

/// The shape of a wave: what a [`Tone`] repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Waveform {
    /// The pure tone — a sine. Everything a UI plays starts here, because
    /// the alternatives carry harmonics that read as "buzzer" at UI
    /// durations.
    #[default]
    Sine,
    /// Odd harmonics, hollow-sounding — a woodwind-ish alert.
    Square,
    /// All harmonics falling off — bright, nasal, the "saw" of alarm clocks.
    Saw,
    /// The sine folded once — soft, like a triangle should be.
    Triangle,
}

impl Waveform {
    /// The wave's value at phase `turns` — whole turns, so `0.25` is a
    /// quarter of the way through one cycle, whatever the frequency.
    ///
    /// All four land in `-1..=1`. Only the sine starts at zero; the others
    /// begin at their own natural phase, which is why an un-enveloped tone
    /// of any shape clicks — and why the envelope's attack exists.
    #[must_use]
    pub fn at(self, turns: f32) -> f32 {
        let phase = (turns - turns.floor()) * std::f32::consts::TAU;
        match self {
            Self::Sine => phase.sin(),
            Self::Square => {
                if phase < std::f32::consts::PI {
                    1.0
                } else {
                    -1.0
                }
            }
            Self::Saw => 1.0 - 2.0 * (phase / std::f32::consts::TAU),
            Self::Triangle => {
                // Fold the saw: rise 0→1 over half a cycle, fall back.
                let unit = phase / std::f32::consts::TAU;
                1.0 - 4.0 * (unit - 0.5).abs()
            }
        }
    }
}

/// How a tone's loudness changes over its life.
///
/// Attack/release rather than a full ADSR, because the sustain level an ADSR
/// adds is a *note* concept — a sound held until released — and everything a
/// UI plays is a *pulse*: it starts and it dies, and what it does in between
/// is the hold at full. An envelope that models the middle of a note models
/// a note a UI does not have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Envelope {
    /// Ramp from 0 to 1 over this long. Short (1–10 ms): a step from silence
    /// to full is itself a click — an unshaped burst of every frequency the
    /// speaker can make, which is why "an un-enveloped tone" and "a click"
    /// are the same sentence.
    pub attack: Duration,
    /// Ramp from 1 to 0 over this long, starting at the end of the hold.
    pub release: Duration,
}

impl Envelope {
    /// An envelope with no ramps at all — a hard on and a hard off.
    ///
    /// Named because it is almost never what anyone wants, and spelling the
    /// default out loud is how the click it makes gets traced back here.
    pub const INSTANT: Self = Self {
        attack: Duration::ZERO,
        release: Duration::ZERO,
    };

    /// The classic pulse: rise over `attack`, hold, fall over `release`.
    #[must_use]
    pub const fn attack_release(attack: Duration, release: Duration) -> Self {
        Self { attack, release }
    }

    /// The amplitude at `elapsed`, given the pulse is held until `hold_end`.
    ///
    /// `hold_end` arrives as a parameter rather than living in the struct
    /// because the envelope is the *shape* of the pulse and the caller knows
    /// when the pulse ends — a 40 ms release after a 10 ms tone and the same
    /// release after 4 s of ambience are one envelope.
    ///
    /// A hold shorter than the attack releases from the attack's peak rather
    /// than from full, so the curve is continuous however the two durations
    /// relate — an envelope that steps at its own boundary is the click it
    /// existed to prevent.
    #[must_use]
    pub fn at(&self, elapsed: Duration, hold_end: Duration) -> f32 {
        let t = elapsed.as_secs_f32();
        let attack = self.attack.as_secs_f32();
        let release = self.release.as_secs_f32();
        let end = hold_end.as_secs_f32();

        if attack > 0.0 && t < attack {
            // Rising.
            (t / attack).clamp(0.0, 1.0)
        } else if t <= end {
            1.0
        } else if release > 0.0 {
            // Falling — from the later of the hold's end and the attack's
            // end, so a short hold releases from wherever the rise got to.
            let release_from = end.max(attack);
            (1.0 - (t - release_from) / release).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// A frequency, a shape, an envelope and a hold: a whole sound, as data.
///
/// Rendering it is [`Mixer`]'s job; being one is this type's. The split is
/// the same one tweens and controllers draw in the animation crate —
/// describe separately from schedule.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tone {
    /// Frequency in hertz. Human hearing tops out around 20 kHz and UI
    /// sounds live far below; anything is accepted and the mixer renders it
    /// faithfully, which above the Nyquist rate of the output means an alias
    /// rather than a chirp — documented here because "faithful" is the only
    /// promise this type makes.
    pub frequency: f32,
    /// The wave's shape.
    pub waveform: Waveform,
    /// How long the tone is held at full before its release tail: the click
    /// of a button holds ~5 ms, an error chime ~200 ms.
    pub hold: Duration,
    /// The loudness shape. Defaults to [`Envelope::INSTANT`] — the honest
    /// default, because the click an unshaped tone makes should be traced
    /// to the place that chose not to shape it.
    pub envelope: Envelope,
}

impl Tone {
    /// A plain tone, held for `hold`.
    #[must_use]
    pub const fn held(frequency: f32, waveform: Waveform, hold: Duration) -> Self {
        Self {
            frequency,
            waveform,
            hold,
            envelope: Envelope::INSTANT,
        }
    }

    /// A pulse: hold 0, so the envelope's attack rises straight into its
    /// release — the shortest sound a UI makes.
    #[must_use]
    pub const fn new(frequency: f32, waveform: Waveform) -> Self {
        Self::held(frequency, waveform, Duration::ZERO)
    }

    /// Set the hold.
    #[must_use]
    pub const fn hold(mut self, hold: Duration) -> Self {
        self.hold = hold;
        self
    }

    /// Set the envelope.
    #[must_use]
    pub const fn envelope(mut self, envelope: Envelope) -> Self {
        self.envelope = envelope;
        self
    }

    /// The wave's amplitude at `elapsed`.
    #[must_use]
    pub fn at(&self, elapsed: Duration) -> f32 {
        self.envelope.at(elapsed, self.hold)
            * self.waveform.at(elapsed.as_secs_f32() * self.frequency)
    }
}

/// Sums sources into one buffer at one rate.
///
/// A mixer is the whole reason "render" exists as an operation: a UI's sound
/// at an instant is the *sum* of everything playing — the click, the chime
/// it overlapped, the ambience under both — and summing before the device
/// (rather than by it) is what keeps the sum testable and deterministic.
#[derive(Debug, Clone)]
pub struct Mixer {
    rate: u32,
    sources: Vec<Source>,
    duration: Duration,
}

/// One thing a [`Mixer`] is playing.
#[derive(Debug, Clone)]
struct Source {
    kind: SourceKind,
    /// Start offset into the mix.
    at: Duration,
    /// Linear gain in `0..=1`, applied at render.
    gain: f32,
}

/// What a source contributes.
#[derive(Debug, Clone)]
enum SourceKind {
    /// A pre-rendered buffer.
    Buffer(Samples),
    /// A tone, its hold and release included.
    Tone(Tone),
}

impl Mixer {
    /// A silent mixer at `rate`.
    #[must_use]
    pub const fn new(rate: u32) -> Self {
        Self {
            rate,
            sources: Vec::new(),
            duration: Duration::ZERO,
        }
    }

    /// The mixer's sample rate.
    #[must_use]
    pub const fn rate(&self) -> u32 {
        self.rate
    }

    /// Add a pre-rendered buffer, playing from `at`, at `gain`.
    ///
    /// # Errors
    ///
    /// [`MixError::RateMismatch`] — see [`Samples::overdub`]'s docs for why
    /// this crate refuses to resample by accident.
    pub fn add_buffer(
        &mut self,
        buffer: &Samples,
        at: Duration,
        gain: f32,
    ) -> Result<(), MixError> {
        if buffer.rate != self.rate {
            return Err(MixError::RateMismatch);
        }
        let ends = at + buffer.duration();
        self.duration = self.duration.max(ends);
        self.sources.push(Source {
            kind: SourceKind::Buffer(buffer.clone()),
            at,
            gain,
        });
        Ok(())
    }

    /// Add a tone at time zero, at `gain`. Returns `self` for chains — the
    /// one-shot spelling a mixer assembled at once reads best in.
    #[must_use]
    pub fn add(self, tone: Tone, gain: f32) -> Self {
        let mut mixer = self;
        mixer.tone_at(tone, Duration::ZERO, gain);
        mixer
    }

    /// Add a tone starting `at` seconds in, at `gain`.
    ///
    /// The source sounds for the tone's hold plus its release tail; a
    /// mixer's [`duration`](Self::duration) covers both, because a release
    /// tail cut off by a render that ended at the hold is a click at the
    /// end — the very thing the envelope exists to prevent.
    pub fn tone_at(&mut self, tone: Tone, at: Duration, gain: f32) -> &mut Self {
        // The sounding span is the later of hold and attack, plus the
        // release — the same arithmetic `Envelope::at` performs when a
        // hold shorter than its attack releases from the attack's peak.
        // Counting only `hold + release` here under-measured exactly that
        // case, and a mixer whose duration stops early cuts the tail it
        // just promised to keep click-free.
        let span = tone.hold.max(tone.envelope.attack) + tone.envelope.release;
        let ends = at + span;
        self.duration = self.duration.max(ends);
        self.sources.push(Source {
            kind: SourceKind::Tone(tone),
            at,
            gain,
        });
        self
    }

    /// When the last source finishes — how long [`render`](Self::render)
    /// will sound for. A mixer with nothing in it lasts zero.
    #[must_use]
    pub const fn duration(&self) -> Duration {
        self.duration
    }

    /// Render the mix: every source summed, sample by sample, saturating.
    ///
    /// The output is mono — every source's channels are summed to one —
    /// because the mixer's job is the *sum*, and stereo is a pan decision
    /// that belongs to whoever places sounds in a field, not to the sum.
    ///
    /// Each tone renders only its **own span** — its attack (when the rise
    /// outlasts the hold, the release falls from the attack's peak; see
    /// [`Envelope::at`]), its hold, and its release tail. Beyond that span
    /// the envelope is exactly zero, so the sample it would have summed is
    /// the identity — and rendering it anyway is quadratic work for linear
    /// sound. This was not theoretical: the launch film's score schedules
    /// ~1,300 tones across a 221 s timeline, and the tone loop as it stood
    /// walked every source to the mix's end — billions of envelope
    /// evaluations of exactly zero, minutes of wall for a mix that is
    /// seconds of audio. The span bound makes render linear in the sound
    /// actually scheduled.
    #[must_use]
    pub fn render(&self) -> Samples {
        let frames = (self.duration.as_secs_f32() * as_f32(self.rate)).ceil() as usize;
        let mut data = vec![0.0_f32; frames];

        for source in &self.sources {
            let start = (source.at.as_secs_f32() * as_f32(self.rate)).round() as usize;
            match &source.kind {
                SourceKind::Buffer(buffer) => {
                    for (index, sample) in buffer.data.iter().enumerate() {
                        // Stereo buffers downmix to the mean of each frame —
                        // the one downmix with no phase surprises.
                        let frame = index / buffer.channels as usize;
                        let out = start + frame;
                        if out < frames {
                            let mono = if buffer.channels == 2 {
                                let left = buffer.data[frame * 2];
                                let right = buffer.data[frame * 2 + 1];
                                (left + right) * 0.5
                            } else {
                                *sample
                            };
                            data[out] = (data[out] + mono * source.gain).clamp(-1.0, 1.0);
                        }
                    }
                }
                SourceKind::Tone(tone) => {
                    // The tone's own span: zero-amplitude beyond it, so the
                    // loop stops there instead of walking to the mix's end.
                    let span = tone.hold.max(tone.envelope.attack) + tone.envelope.release;
                    let len = (span.as_secs_f32() * as_f32(self.rate)).ceil() as usize;
                    let end = start.saturating_add(len).min(frames);
                    for (frame, out) in data.iter_mut().enumerate().take(end).skip(start) {
                        let elapsed = Duration::from_secs_f32(
                            as_f32(self.rate).recip() * (frame - start) as f32,
                        );
                        let value = tone.at(elapsed) * source.gain;
                        *out = (*out + value).clamp(-1.0, 1.0);
                    }
                }
            }
        }

        Samples::mono(data, self.rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    #[test]
    fn the_four_waveforms_live_in_range_and_start_at_zero() {
        for wave in [
            Waveform::Sine,
            Waveform::Square,
            Waveform::Saw,
            Waveform::Triangle,
        ] {
            assert!(
                (-1.0..=1.0).contains(&wave.at(0.0)),
                "{wave:?} starts inside the unit interval"
            );
            for turns in [0.1_f32, 0.25, 0.3, 0.5, 0.7, 0.9, 1.7] {
                let value = wave.at(turns);
                assert!(
                    (-1.0..=1.0).contains(&value),
                    "{wave:?} at {turns} left the unit interval: {value}"
                );
            }
        }
        // And the shapes are distinct where they must be.
        assert_eq!(Waveform::Sine.at(0.0), 0.0, "only the sine starts at zero");
        assert_eq!(Waveform::Square.at(0.25), 1.0);
        assert_eq!(Waveform::Square.at(0.75), -1.0);
        assert_eq!(Waveform::Saw.at(0.75), -0.5);
        assert_eq!(
            Waveform::Triangle.at(0.5),
            1.0,
            "the triangle peaks mid-cycle"
        );
        assert_eq!(Waveform::Triangle.at(0.0), -1.0, "and starts at its trough");
    }

    #[test]
    fn an_envelope_rises_holds_and_falls() {
        let envelope = Envelope::attack_release(ms(10), ms(40));

        assert_eq!(envelope.at(ms(0), ms(100)), 0.0);
        assert_eq!(envelope.at(ms(5), ms(100)), 0.5);
        assert_eq!(envelope.at(ms(10), ms(100)), 1.0);
        assert_eq!(envelope.at(ms(80), ms(100)), 1.0, "the hold");
        assert!(
            (envelope.at(ms(120), ms(100)) - 0.5).abs() < 0.001,
            "the release, half way"
        );
        assert_eq!(envelope.at(ms(140), ms(100)), 0.0, "the release, done");
    }

    #[test]
    fn a_short_hold_releases_from_the_rise_not_from_a_step() {
        // Hold 20 ms, attack 100 ms: the tone never reaches full, and the
        // release must fall from wherever the rise had got to — continuous.
        let envelope = Envelope::attack_release(ms(100), ms(100));

        let just_before = envelope.at(ms(60), ms(20));
        let just_after = envelope.at(ms(61), ms(20));
        assert!(
            (just_after - just_before).abs() < 0.05,
            "no step: {just_before} then {just_after}"
        );
        // Both are on the release now, which began at the attack's end.
        assert!(just_before > 0.5 && just_before < 1.0);
    }

    #[test]
    fn an_instant_envelope_is_a_hard_gate() {
        let envelope = Envelope::INSTANT;
        assert_eq!(envelope.at(ms(50), ms(100)), 1.0);
        assert_eq!(envelope.at(ms(101), ms(100)), 0.0);
    }

    #[test]
    fn samples_know_their_shape() {
        let mono = Samples::mono(vec![0.1; 100], 10_000);
        assert_eq!(mono.frames(), 100);
        assert_eq!(mono.duration(), ms(10));
        assert_eq!(mono.peak(), 0.1);

        let stereo = Samples::stereo((0..10).flat_map(|_| [0.5, -0.25]).collect(), 10_000);
        assert_eq!(stereo.frames(), 10);
        assert_eq!(stereo.peak(), 0.5);
        assert_eq!(stereo.amplitude_at(ms(0)), 0.5);
        assert_eq!(
            stereo.amplitude_at(Duration::from_secs(10)),
            0.0,
            "past the end"
        );
    }

    #[test]
    #[should_panic(expected = "whole [L, R] frames")]
    fn a_stereo_buffer_of_half_a_frame_is_refused() {
        let _ = Samples::stereo(vec![0.0; 3], 8_000);
    }

    #[test]
    fn overdubbing_layers_and_saturates() {
        let mut base = Samples::mono(vec![0.8; 100], 8_000);
        let top = Samples::mono(vec![0.8; 100], 8_000);
        base.overdub(&top, Duration::ZERO).unwrap();

        assert_eq!(base.data[0], 1.0, "0.8 + 0.8 saturates, it does not wrap");

        // And a late overdub grows the buffer rather than clipping short:
        // 50 ms at 8 kHz is 400 frames, plus the 100 the source brings.
        let mut base = Samples::mono(vec![0.0; 10], 8_000);
        base.overdub(&top, ms(50)).unwrap();
        assert_eq!(base.frames(), 400 + 100, "grown to hold the late arrival");
    }

    #[test]
    fn overdubbing_refuses_mismatched_buffers() {
        let mut base = Samples::mono(vec![0.0; 10], 8_000);
        let fast = Samples::mono(vec![0.0; 10], 16_000);
        assert_eq!(
            base.overdub(&fast, Duration::ZERO),
            Err(MixError::RateMismatch)
        );

        let stereo = Samples::stereo(vec![0.0; 10], 8_000);
        assert_eq!(
            base.overdub(&stereo, Duration::ZERO),
            Err(MixError::ChannelMismatch)
        );
    }

    #[test]
    fn mix_errors_say_what_to_do() {
        assert_eq!(
            MixError::RateMismatch.to_string(),
            "sample rates differ; resample first"
        );
        assert_eq!(
            MixError::ChannelMismatch.to_string(),
            "channel counts differ; downmix first"
        );
    }

    #[test]
    fn a_mixer_sums_two_tones() {
        // Two 440 Hz tones at gain 0.5: the sum is one tone at 1.0.
        let tone = Tone::held(440.0, Waveform::Sine, ms(100));
        let mixed = Mixer::new(44_100).add(tone, 0.5).add(tone, 0.5).render();

        // A pure sine's peak is the sum of the gains.
        assert!(
            (mixed.peak() - 1.0).abs() < 0.05,
            "summed to one tone, peak {}",
            mixed.peak()
        );
    }

    #[test]
    fn a_mixer_staggers_starts() {
        // Square waves (constant amplitude, so a single-frame read means
        // something), held long enough that the staggered source overlaps
        // the first rather than replacing it. 440 Hz at a 100 ms stagger is
        // exactly 44 cycles apart, so the two are phase-locked: the sum is
        // one louder, never a cancellation.
        let tone = Tone::held(440.0, Waveform::Square, ms(150));
        let mut mixer = Mixer::new(44_100);
        mixer.tone_at(tone, Duration::ZERO, 0.5);
        mixer.tone_at(tone, ms(100), 0.5);
        let mixed = mixer.render();

        let early = mixed.amplitude_at(ms(50));
        let late = mixed.amplitude_at(ms(125));
        assert!(
            (early - 0.5).abs() < 0.01,
            "one square at half gain, got {early}"
        );
        assert!(
            (late - 1.0).abs() < 0.01,
            "two phase-locked squares at half gain each, got {late}"
        );
        assert_eq!(mixer.duration(), ms(250));
    }

    #[test]
    fn a_mixer_downmixes_stereo_buffers_to_the_mean() {
        // Left +0.5, right -0.5: mono by the mean is zero.
        let buffer = Samples::stereo((0..100).flat_map(|_| [0.5, -0.5]).collect(), 8_000);
        let mut mixer = Mixer::new(8_000);
        mixer.add_buffer(&buffer, Duration::ZERO, 1.0).unwrap();

        let mixed = mixer.render();
        assert_eq!(mixed.channels, 1);
        assert!(mixed.peak() < 0.001, "the mean of +0.5 and -0.5 is silence");
    }

    #[test]
    fn a_mixer_refuses_a_foreign_rate() {
        let foreign = Samples::mono(vec![0.0; 10], 48_000);
        let mut mixer = Mixer::new(44_100);
        assert_eq!(
            mixer.add_buffer(&foreign, Duration::ZERO, 1.0),
            Err(MixError::RateMismatch)
        );
    }

    #[test]
    fn an_empty_mixer_renders_silence() {
        let mixer = Mixer::new(44_100);
        assert_eq!(mixer.duration(), Duration::ZERO);
        let rendered = mixer.render();
        assert!(rendered.data.is_empty());
        assert_eq!(rendered.rate, 44_100);
    }

    #[test]
    fn gain_zero_renders_silence() {
        let tone = Tone::held(440.0, Waveform::Sine, ms(100));
        let mixed = Mixer::new(44_100).add(tone, 0.0).render();
        assert!(mixed.peak() < 0.001, "gain zero is silence, not a whisper");
    }

    #[test]
    fn a_tone_knows_its_amplitude_at_any_instant() {
        // 4 Hz square: +1 through the first half of every cycle, so the wave
        // contributes a constant and the envelope is the number under test.
        let tone = Tone::held(4.0, Waveform::Square, ms(100))
            .envelope(Envelope::attack_release(ms(50), ms(50)));

        // Rising: half way through the 50 ms attack, wave full.
        assert_eq!(tone.at(ms(25)), 0.5);
        // Held: the wave is full and the envelope is 1.
        assert_eq!(tone.at(ms(75)), 1.0);
        // Released: hold ended at 100 ms, release takes 50 ms.
        assert_eq!(tone.at(ms(150)), 0.0);
    }

    #[test]
    fn a_short_tone_renders_only_its_own_span() {
        // The invariant the render loop now relies on: beyond a tone's
        // attack (when the rise outlasts the hold), hold and release, its
        // envelope is exactly zero — so the mix there is bit-identically
        // untouched, whatever else the timeline carries.
        //
        // Found by the launch film's score: ~1,300 tones across a 221 s
        // timeline made the old loop — every source walked to the mix's
        // end — minutes of wall for seconds of audio. The bound is a
        // performance claim; this test is its correctness receipt: a
        // click one second into a ten-second mix leaves every sample
        // past its tail exactly as it was.
        let rate = 8_000usize;
        let mut mixer = Mixer::new(rate as u32);
        mixer.tone_at(
            Tone::held(440.0, Waveform::Sine, ms(5))
                .envelope(Envelope::attack_release(ms(1), ms(40))),
            ms(1_000),
            0.5,
        );
        // A silent buffer that makes the mix long — the click is the only
        // sound, and it must not render across the nine seconds after it.
        mixer
            .add_buffer(
                &Samples::mono(vec![0.0; rate * 10], rate as u32),
                Duration::ZERO,
                0.0,
            )
            .expect("same rate");

        let out = mixer.render();
        assert_eq!(
            out.data.len(),
            rate * 10,
            "the mix lasts its longest source"
        );
        // The click sounds 1.000 s → 1.046 s (attack 1 ms, hold 5 ms,
        // release 40 ms) — samples 8 000 → 8 368 at this rate; two samples
        // of ceil slack, then silence — *exactly* silence, not
        // envelope-zero summed onto silence.
        let tail_end = 8_000 + ((46.0 / 1_000.0) * rate as f32).ceil() as usize + 2;
        for (i, &s) in out.data.iter().enumerate() {
            if i > tail_end {
                assert_eq!(
                    s, 0.0,
                    "sample {i} beyond the click's tail must be untouched"
                );
            }
        }
        // And the click itself is present where it should be.
        assert!(
            out.data[8_000..8_050].iter().any(|&s| s != 0.0),
            "the click's own span carries sound"
        );
    }

    #[test]
    fn a_tone_whose_attack_outlasts_its_hold_releases_from_the_attack_peak() {
        // The span arithmetic must cover the case `Envelope::at`
        // documents: a hold shorter than the attack releases from wherever
        // the rise got to — which lands *later* than hold + release. Both
        // `tone_at`'s duration accounting and the render loop's bound use
        // the same `max(hold, attack) + release`, so the tail survives
        // whole instead of being cut by a mix that ended early.
        //
        // 4 Hz square (+1 for the first 125 ms) so the wave contributes a
        // constant and the envelope is the number under test.
        let rate = 8_000usize;
        let mut mixer = Mixer::new(rate as u32);
        // Hold 0, attack 60 ms, release 20 ms: the release falls from
        // 60 ms to 80 ms — twenty milliseconds past hold + release.
        mixer.tone_at(
            Tone::held(4.0, Waveform::Square, Duration::ZERO)
                .envelope(Envelope::attack_release(ms(60), ms(20))),
            Duration::ZERO,
            1.0,
        );
        let out = mixer.render();
        // The mix lasts the whole sounding span: 80 ms, not 20.
        assert_eq!(
            out.data.len(),
            8_000 * 80 / 1_000,
            "the duration covers the attack's tail"
        );
        let at = |t_ms: usize| out.data[t_ms * 8_000 / 1_000];
        // Mid-attack: rising through 0.5.
        assert!(
            (at(30) - 0.5).abs() < 0.02,
            "mid-attack amplitude is ~0.5: {}",
            at(30)
        );
        // Mid-release (70 ms): the fall is at its half.
        assert!(
            (at(70) - 0.5).abs() < 0.02,
            "mid-release amplitude is ~0.5: {}",
            at(70)
        );
        // The tail is complete: nearly zero by 79 ms, and the last sample
        // the mix carries is the release's own end.
        assert!(at(79) < 0.10, "the release completes: {}", at(79));
    }
}
