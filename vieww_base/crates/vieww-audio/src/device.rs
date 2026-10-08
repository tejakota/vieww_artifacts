//! Audio out to real hardware, with no audio library underneath.
//!
//! [`AlsaOutput`] talks to the Linux kernel's ALSA PCM driver directly —
//! `open("/dev/snd/pcmC{card}D{device}p")`, then the kernel's own ioctls:
//! `HW_PARAMS` with a hand-laid-out `struct snd_pcm_hw_params` (access
//! RW-interleaved, `S16_LE`, channels, rate, period and buffer sizes),
//! `PREPARE`, `WRITEI_FRAMES` per block and `DRAIN` at the end; an `-EPIPE`
//! underrun is recovered by re-`PREPARE`. The only foreign function is the
//! C library's `ioctl`, which `std` already links: this is the same
//! kernel interface `libasound` itself wraps.
//!
//! [`AlsaPlayer`] puts it behind the crate's [`AudioPlayer`] seam: `play`
//! renders the [`Sound`], spawns a thread that streams it, and `stop` ends
//! the stream at the next block.
//!
//! [`WavSink`] is the device-less output: a sound written to a WAV file,
//! for render pipelines and CI.
//!
//! Scope, honestly: verified to compile and to fail cleanly here (the build
//! machine has no sound card, so `open` reports that); the struct layout
//! follows `include/uapi/sound/asound.h` for 64-bit Linux. macOS
//! CoreAudio, Windows WASAPI, Android AAudio and the web's AudioContext are
//! each a backend of their own behind the same trait.

use std::path::Path;

use vieww_foundation::service::ServiceError;

use crate::player::{AudioPlayer, PlaybackHandle, Sound};
use crate::{Mixer, Samples};

/// Render any [`Sound`] to samples at `rate`.
#[must_use]
pub fn render_sound(sound: &Sound, rate: u32) -> Samples {
    match sound {
        Sound::Buffer(s) => s.clone(),
        Sound::Tone(t) => Mixer::new(rate).add(*t, 1.0).render(),
    }
}

/// Interleaved stereo `i16` from samples (mono duplicated).
#[must_use]
pub fn to_i16_stereo(s: &Samples) -> Vec<i16> {
    let q = |v: f32| {
        #[allow(clippy::cast_possible_truncation)]
        let x = (v.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        x
    };
    if s.channels == 2 {
        s.data.iter().map(|&v| q(v)).collect()
    } else {
        s.data.iter().flat_map(|&v| [q(v), q(v)]).collect()
    }
}

/// Writes sounds to WAV files — the output with no device.
#[derive(Debug, Clone)]
pub struct WavSink {
    pub dir: std::path::PathBuf,
}

impl WavSink {
    /// # Errors
    /// File I/O.
    pub fn write(&self, name: &str, sound: &Sound) -> std::io::Result<std::path::PathBuf> {
        std::fs::create_dir_all(&self.dir)?;
        let p = self.dir.join(format!("{name}.wav"));
        let bytes = crate::write_wav(&render_sound(sound, 48_000))
            .map_err(|e| std::io::Error::other(format!("{e:?}")))?;
        std::fs::write(&p, bytes)?;
        Ok(p)
    }
}

#[cfg(target_os = "linux")]
mod alsa {
    use std::fs::{File, OpenOptions};
    use std::os::fd::AsRawFd;

    unsafe extern "C" {
        fn ioctl(fd: i32, request: u64, ...) -> i32;
    }

    const MASK_WORDS: usize = 8; // SNDRV_MASK_MAX 256 / 32

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Interval {
        min: u32,
        max: u32,
        /// openmin:1 openmax:1 integer:1 empty:1
        flags: u32,
    }

    /// `struct snd_pcm_hw_params` (64-bit layout, 608 bytes).
    #[repr(C)]
    struct HwParams {
        flags: u32,
        masks: [[u32; MASK_WORDS]; 3],
        mres: [[u32; MASK_WORDS]; 5],
        intervals: [Interval; 12],
        ires: [Interval; 9],
        rmask: u32,
        cmask: u32,
        info: u32,
        msbits: u32,
        rate_num: u32,
        rate_den: u32,
        fifo_size: u64,
        reserved: [u8; 64],
    }

    /// `struct snd_xferi`.
    #[repr(C)]
    struct Xferi {
        result: i64,
        buf: *const std::ffi::c_void,
        frames: u64,
    }

    const fn ioc(dir: u64, nr: u64, size: u64) -> u64 {
        (dir << 30) | (size << 16) | (0x41 << 8) | nr
    }
    const HW_PARAMS: u64 = ioc(3, 0x11, std::mem::size_of::<HwParams>() as u64);
    const PREPARE: u64 = ioc(0, 0x40, 0);
    const DRAIN: u64 = ioc(0, 0x44, 0);
    const WRITEI: u64 = ioc(1, 0x50, std::mem::size_of::<Xferi>() as u64);

    // Parameter indices.
    const ACCESS: usize = 0;
    const FORMAT: usize = 1;
    const SUBFORMAT: usize = 2;
    const I_SAMPLE_BITS: usize = 0;
    const I_FRAME_BITS: usize = 1;
    const I_CHANNELS: usize = 2;
    const I_RATE: usize = 3;
    const I_PERIOD_SIZE: usize = 5;
    const I_PERIODS: usize = 7;
    const I_BUFFER_SIZE: usize = 9;
    const ACCESS_RW_INTERLEAVED: u32 = 3;
    const FORMAT_S16_LE: u32 = 2;
    const EPIPE: i32 = 32;

    pub(super) struct Pcm {
        file: File,
        pub channels: u32,
    }

    fn errno() -> i32 {
        std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
    }

    impl Pcm {
        pub(super) fn open(card: u32, device: u32, rate: u32, channels: u32) -> Result<Self, String> {
            let path = format!("/dev/snd/pcmC{card}D{device}p");
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .map_err(|e| format!("{path}: {e}"))?;
            let full = Interval { min: 0, max: u32::MAX, flags: 0 };
            let mut p = HwParams {
                flags: 0,
                masks: [[u32::MAX; MASK_WORDS]; 3],
                mres: [[0; MASK_WORDS]; 5],
                intervals: [full; 12],
                ires: [Interval { min: 0, max: 0, flags: 0 }; 9],
                rmask: u32::MAX,
                cmask: 0,
                info: 0,
                msbits: 0,
                rate_num: 0,
                rate_den: 0,
                fifo_size: 0,
                reserved: [0; 64],
            };
            let set_mask = |m: &mut [u32; MASK_WORDS], bit: u32| {
                *m = [0; MASK_WORDS];
                m[(bit / 32) as usize] = 1 << (bit % 32);
            };
            set_mask(&mut p.masks[ACCESS], ACCESS_RW_INTERLEAVED);
            set_mask(&mut p.masks[FORMAT], FORMAT_S16_LE);
            set_mask(&mut p.masks[SUBFORMAT], 0);
            let exact = |v: u32| Interval { min: v, max: v, flags: 0b100 };
            p.intervals[I_SAMPLE_BITS] = exact(16);
            p.intervals[I_FRAME_BITS] = exact(16 * channels);
            p.intervals[I_CHANNELS] = exact(channels);
            p.intervals[I_RATE] = exact(rate);
            p.intervals[I_PERIOD_SIZE] = Interval { min: 512, max: 2048, flags: 0b100 };
            p.intervals[I_PERIODS] = Interval { min: 2, max: 8, flags: 0b100 };
            p.intervals[I_BUFFER_SIZE] = Interval { min: 2048, max: 16_384, flags: 0b100 };
            let fd = file.as_raw_fd();
            // SAFETY: `p` is a correctly sized and laid-out snd_pcm_hw_params
            // that outlives the call; the request number encodes its size.
            if unsafe { ioctl(fd, HW_PARAMS, std::ptr::addr_of_mut!(p)) } < 0 {
                return Err(format!("HW_PARAMS refused (errno {})", errno()));
            }
            // SAFETY: PREPARE takes no argument.
            if unsafe { ioctl(fd, PREPARE) } < 0 {
                return Err(format!("PREPARE refused (errno {})", errno()));
            }
            Ok(Self { file, channels })
        }

        /// Write interleaved frames, recovering from underruns.
        pub(super) fn write(&mut self, frames: &[i16]) -> Result<(), String> {
            let fd = self.file.as_raw_fd();
            let ch = self.channels as usize;
            let mut off = 0;
            while off < frames.len() {
                let mut x = Xferi {
                    result: 0,
                    buf: frames[off..].as_ptr().cast(),
                    frames: ((frames.len() - off) / ch) as u64,
                };
                // SAFETY: `buf` points at `frames` remaining interleaved
                // frames that live for the call; `x` is a valid snd_xferi.
                let r = unsafe { ioctl(fd, WRITEI, std::ptr::addr_of_mut!(x)) };
                if r < 0 {
                    if errno() == EPIPE {
                        // SAFETY: as above.
                        unsafe { ioctl(fd, PREPARE) };
                        continue;
                    }
                    return Err(format!("WRITEI failed (errno {})", errno()));
                }
                off += usize::try_from(x.result).unwrap_or(0) * ch;
            }
            Ok(())
        }

        pub(super) fn drain(&mut self) {
            // SAFETY: DRAIN takes no argument.
            unsafe { ioctl(self.file.as_raw_fd(), DRAIN) };
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn the_kernel_structs_have_the_kernel_sizes() {
            assert_eq!(std::mem::size_of::<HwParams>(), 608);
            assert_eq!(std::mem::size_of::<Xferi>(), 24);
            assert_eq!(HW_PARAMS, 0xC260_4111);
            assert_eq!(WRITEI, 0x4018_4150);
            assert_eq!(PREPARE, 0x4140);
        }
    }
}

/// A PCM stream to an ALSA device.
pub struct AlsaOutput {
    #[cfg(target_os = "linux")]
    pcm: alsa::Pcm,
}

impl std::fmt::Debug for AlsaOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AlsaOutput")
    }
}

impl AlsaOutput {
    /// Open card/device for `rate` Hz stereo `S16_LE`.
    ///
    /// # Errors
    /// No such device, or the device refused the parameters; on non-Linux
    /// targets, always.
    pub fn open(card: u32, device: u32, rate: u32) -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        {
            alsa::Pcm::open(card, device, rate, 2).map(|pcm| Self { pcm })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (card, device, rate);
            Err("ALSA exists only on Linux".into())
        }
    }

    /// Queue interleaved stereo frames (blocks while the device's buffer
    /// is full — the device's clock paces the caller).
    ///
    /// # Errors
    /// The device failed mid-stream.
    pub fn write(&mut self, frames: &[i16]) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        {
            self.pcm.write(frames)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = frames;
            Err("ALSA exists only on Linux".into())
        }
    }

    /// Wait for queued frames to play out.
    pub fn drain(&mut self) {
        #[cfg(target_os = "linux")]
        self.pcm.drain();
    }

    /// Stream samples and drain.
    ///
    /// # Errors
    /// The device failed mid-stream.
    pub fn play_blocking(&mut self, s: &Samples) -> Result<(), String> {
        for block in to_i16_stereo(s).chunks(4096) {
            self.write(block)?;
        }
        self.drain();
        Ok(())
    }
}

/// An [`AudioPlayer`] over ALSA: each `play` streams on its own thread.
#[derive(Debug, Default)]
pub struct AlsaPlayer {
    next: std::cell::Cell<u64>,
    stops: std::cell::RefCell<Vec<(u64, std::sync::Arc<std::sync::atomic::AtomicBool>)>>,
}

impl AudioPlayer for AlsaPlayer {
    fn play(&self, sound: &Sound) -> Result<PlaybackHandle, ServiceError> {
        // Probe the device first so a missing card is an honest refusal.
        let mut out = AlsaOutput::open(0, 0, 48_000).map_err(ServiceError::Failed)?;
        let samples = render_sound(sound, 48_000);
        let id = self.next.get() + 1;
        self.next.set(id);
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.stops.borrow_mut().push((id, stop.clone()));
        std::thread::spawn(move || {
            for block in to_i16_stereo(&samples).chunks(4096) {
                if stop.load(std::sync::atomic::Ordering::Relaxed) || out.write(block).is_err() {
                    return;
                }
            }
            out.drain();
        });
        Ok(PlaybackHandle(id))
    }

    fn stop(&self, handle: PlaybackHandle) -> Result<(), ServiceError> {
        if let Some((_, s)) = self.stops.borrow().iter().find(|(i, _)| *i == handle.0) {
            s.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        Ok(())
    }
}

/// Whether this machine has an ALSA playback device.
#[must_use]
pub fn has_alsa_device() -> bool {
    Path::new("/dev/snd/pcmC0D0p").exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Tone, Waveform};
    use std::time::Duration;

    #[test]
    fn sounds_render_and_quantise() {
        let s = render_sound(&Sound::Tone(Tone::held(440.0, Waveform::Sine, Duration::from_millis(50))), 48_000);
        let q = to_i16_stereo(&s);
        assert_eq!(q.len(), s.data.len() * 2);
        assert!(q.iter().any(|&v| v > 10_000));
    }

    #[test]
    fn wav_sink_writes_a_readable_file() {
        let dir = std::env::temp_dir().join(format!("vieww-wavsink-{}", std::process::id()));
        let p = WavSink { dir: dir.clone() }
            .write("beep", &Sound::Tone(Tone::held(880.0, Waveform::Square, Duration::from_millis(20))))
            .unwrap();
        let back = crate::read_wav(&std::fs::read(&p).unwrap()).unwrap();
        assert!(!back.samples.data.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_device_is_an_honest_error() {
        if has_alsa_device() {
            return; // a real card: nothing to assert about refusal
        }
        assert!(AlsaOutput::open(0, 0, 48_000).is_err());
        let p = AlsaPlayer::default();
        assert!(matches!(
            p.play(&Sound::Tone(Tone::held(440.0, Waveform::Sine, Duration::from_millis(5)))),
            Err(ServiceError::Failed(_))
        ));
    }
}
