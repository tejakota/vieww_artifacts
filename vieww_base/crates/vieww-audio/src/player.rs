//! The playback seam: sound as a service.
//!
//! [`vieww_foundation::service`]'s rule is that a third party must be able to
//! add a service without editing foundation — and the first-party proof is
//! right here: **the [`AudioPlayer`] trait lives in this crate, not in
//! foundation's `capability` module**, and a platform crate registers an
//! implementation into the same `Services` registry every other capability
//! uses. Nothing in foundation knows audio exists.
//!
//! ```
//! use std::rc::Rc;
//! use vieww_audio::{NoAudio, Sound, Tone, Waveform};
//! use vieww_foundation::service::{ServiceError, Services};
//!
//! // An application that registers nothing gets `None` — the honest default.
//! let mut services = Services::new();
//! assert!(services.get::<dyn vieww_audio::AudioPlayer>().is_none());
//!
//! // Registering `NoAudio` is a statement: this build has an audio-shaped
//! // hole in it on purpose, and every call says so.
//! services.provide::<dyn vieww_audio::AudioPlayer>(Rc::new(NoAudio));
//! let player = services.get::<dyn vieww_audio::AudioPlayer>().unwrap();
//! let refusal = player.play(&Sound::tone(Tone::new(440.0, Waveform::Sine)));
//! assert!(matches!(refusal, Err(ServiceError::Unsupported { .. })));
//! ```
//!
//! # Why the trait is shaped like this
//!
//! * **`play` returns a handle, not a promise.** A UI sound is fire-and-forget
//!   with one escape hatch — "stop it" — and a handle is exactly the smallest
//!   type that carries both. No position query, no volume ramp, no finished
//!   callback: a click needs none of them, and a trait that required a
//!   platform to implement them all would be a trait desktop could satisfy
//!   and a web backend could not.
//! * **`Sound` is an enum, not a file path.** A path is somebody's asset
//!   pipeline; a `Tone` or a decoded [`Samples`] is *sound*, in the hand,
//!   and the same value can come from a file, a synth, or a test fixture.
//! * **The implementations here play nothing.** [`NoAudio`] refuses;
//!   [`RecordingPlayer`] writes down what it was asked. The one that makes
//!   noise is a platform crate's job, because making noise is what
//!   platforms disagree about.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use vieww_foundation::service::ServiceError;

use crate::{Samples, Tone};

/// What can be asked to make a noise.
///
/// An enum rather than a trait object because the set is closed by design:
/// a tone and a buffer are the two things this crate can produce, and an
/// open-ended "any sound source" would be a streaming protocol — a real
/// feature, and one a UI framework's *clicks* do not need.
#[derive(Debug, Clone)]
pub enum Sound {
    /// A synthetic tone.
    Tone(Tone),
    /// A pre-rendered buffer, decoded from WAV or rendered by a [`Mixer`].
    ///
    /// [`Mixer`]: crate::Mixer
    Buffer(Samples),
}

impl Sound {
    /// A sound from a tone.
    #[must_use]
    pub const fn tone(tone: Tone) -> Self {
        Self::Tone(tone)
    }

    /// A sound from samples.
    #[must_use]
    pub const fn buffer(samples: Samples) -> Self {
        Self::Buffer(samples)
    }

    /// How long the sound plays, envelope tail included.
    #[must_use]
    pub fn duration(&self) -> Duration {
        match self {
            Self::Tone(tone) => tone.hold + tone.envelope.attack + tone.envelope.release,
            Self::Buffer(samples) => samples.duration(),
        }
    }
}

/// A playing sound's identity, for stopping it.
///
/// Opaque by design: an integer a platform hands back, meaningful only to
/// the implementation that minted it. A `Copy` handle, because "stop the
/// sound this handle names" is a *UI-unmount* operation and a handle that
/// had to be kept alive to keep the sound playing would make unmounting a
/// widget silently stop its sound — the opposite of what an unmount wants
/// only sometimes, which is the worst kind of rule to leave implicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlaybackHandle(pub u64);

/// Sound out of a device, as a service.
///
/// Implementations: [`NoAudio`] (refuses), [`RecordingPlayer`] (records), a
/// platform crate (plays). See the module docs for why the trait is this
/// small.
pub trait AudioPlayer: 'static {
    /// Start `sound`. The handle is how it is stopped later.
    ///
    /// # Errors
    ///
    /// [`ServiceError::Unsupported`] when the platform has no audio at all.
    /// A *playback* failure — device busy, buffer underrun — is not this
    /// trait's vocabulary: the trait's contract is fire-and-forget, and a
    /// caller that needs to know a sound failed is a caller with a sync
    /// problem this shape cannot express.
    fn play(&self, sound: &Sound) -> Result<PlaybackHandle, ServiceError>;

    /// Stop the sound `handle` names. Stopping a sound that already finished
    /// is fine — "stop" is a request about the future, and a sound that has
    /// no future stops trivially.
    ///
    /// # Errors
    ///
    /// [`ServiceError::Unsupported`] when the platform has no audio at all.
    fn stop(&self, handle: PlaybackHandle) -> Result<(), ServiceError>;
}

/// The audio player that is not there.
///
/// Every call returns [`ServiceError::Unsupported`]. Registering it is a
/// statement about the build — "this platform's audio is a known hole" —
/// which is a different thing from registering nothing, where `get` returns
/// `None` and the application knows it did no wiring at all.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoAudio;

impl AudioPlayer for NoAudio {
    fn play(&self, _sound: &Sound) -> Result<PlaybackHandle, ServiceError> {
        Err(ServiceError::unsupported("AudioPlayer"))
    }

    fn stop(&self, _handle: PlaybackHandle) -> Result<(), ServiceError> {
        Err(ServiceError::unsupported("AudioPlayer"))
    }
}

/// One recorded command, for asserting wiring without a speaker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordedCommand {
    /// `play`, with the sound's duration (the sound itself is not cloned —
    /// a test asserting "a click was played" wants to know *that* and *how
    /// long*, and comparing two buffers sample-by-sample is a
    /// mixing test wearing a wiring test's name).
    Play { duration: Duration },
    /// `stop`, with the handle.
    Stop(PlaybackHandle),
}

/// An [`AudioPlayer`] that writes down what it was asked and plays nothing.
///
/// The implementation a widget test registers: "the press played a sound and
/// the unmount stopped it" is an assertion about *wiring*, and this is the
/// double that lets a test make it. [`RefCell`] inside and [`Rc`] outside
/// because services are held as `Rc<dyn Trait>` and shared, and a test's
/// assertions read the log from a second handle to the same recorder.
#[derive(Debug, Clone, Default)]
pub struct RecordingPlayer {
    commands: Rc<RefCell<Vec<RecordedCommand>>>,
    next: Rc<RefCell<u64>>,
}

impl RecordingPlayer {
    /// A recorder with an empty log.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The commands so far.
    #[must_use]
    pub fn commands(&self) -> Vec<RecordedCommand> {
        self.commands.borrow().clone()
    }
}

impl AudioPlayer for RecordingPlayer {
    fn play(&self, sound: &Sound) -> Result<PlaybackHandle, ServiceError> {
        let handle = PlaybackHandle({
            let mut next = self.next.borrow_mut();
            *next += 1;
            *next
        });
        self.commands
            .borrow_mut()
            .push(RecordedCommand::Play {
                duration: sound.duration(),
            });
        Ok(handle)
    }

    fn stop(&self, handle: PlaybackHandle) -> Result<(), ServiceError> {
        self.commands
            .borrow_mut()
            .push(RecordedCommand::Stop(handle));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_audio_refuses_loudly() {
        let player = NoAudio;
        let refusal = player.play(&Sound::tone(Tone::new(440.0, crate::Waveform::Sine)));
        assert!(matches!(refusal, Err(ServiceError::Unsupported { .. })));
        let stop = player.stop(PlaybackHandle(0));
        assert!(matches!(stop, Err(ServiceError::Unsupported { .. })));
    }

    #[test]
    fn a_recording_player_records_the_wiring() {
        let player = RecordingPlayer::new();
        let handle = player
            .play(&Sound::tone(Tone::held(440.0, crate::Waveform::Sine, Duration::from_millis(100))))
            .expect("the recorder always accepts");

        player.stop(handle).expect("the recorder always accepts");

        assert_eq!(
            player.commands(),
            vec![
                RecordedCommand::Play {
                    duration: Duration::from_millis(100)
                },
                RecordedCommand::Stop(handle)
            ]
        );
    }

    #[test]
    fn handles_are_distinct_and_copy() {
        let player = RecordingPlayer::new();
        let first = player
            .play(&Sound::tone(Tone::new(440.0, crate::Waveform::Sine)))
            .unwrap();
        let second = player
            .play(&Sound::tone(Tone::new(880.0, crate::Waveform::Sine)))
            .unwrap();
        assert_ne!(first, second);

        // Copy: the handle survives being handed to `stop` by value.
        let moved = first;
        player.stop(moved).unwrap();
        player.stop(first).unwrap();
        assert_eq!(player.commands().len(), 4);
    }

    #[test]
    fn a_recorder_shared_between_handles_shares_its_log() {
        let recorder = Rc::new(RecordingPlayer::new());
        let as_service: Rc<dyn AudioPlayer> = recorder.clone();

        as_service
            .play(&Sound::tone(Tone::new(440.0, crate::Waveform::Sine)))
            .unwrap();

        // The second handle sees the first's command: one log, shared.
        assert_eq!(recorder.commands().len(), 1);
    }

    #[test]
    fn sound_durations_cover_both_kinds() {
        let tone = Sound::tone(
            Tone::held(440.0, crate::Waveform::Sine, Duration::from_millis(200)).envelope(
                crate::Envelope::attack_release(
                    Duration::from_millis(10),
                    Duration::from_millis(50),
                ),
            ),
        );
        assert_eq!(tone.duration(), Duration::from_millis(260));

        let buffer = Sound::buffer(Samples::mono(vec![0.0; 441], 44_100));
        assert_eq!(buffer.duration(), Duration::from_millis(10));
    }
}
