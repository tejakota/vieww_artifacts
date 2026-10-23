//! Audio out to real hardware, with no audio library underneath.
//!
//! [`AlsaOutput`] talks to the Linux kernel's ALSA PCM driver directly —
//! `open("/dev/snd/pcmC{card}D{device}p")`, then the kernel's own ioctls:
//! `HW_PARAMS` with a hand-laid-out `struct snd_pcm_hw_params` (access
//! RW-interleaved, a sample format, channels, rate, period and buffer
//! sizes), `PREPARE`, `WRITEI_FRAMES` per block and `DRAIN` at the end; an `-EPIPE`
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
//! [`AlsaInput`] is the capture side (`READI`, overrun recovery).
//!
//! # Formats and rates
//!
//! [`AlsaOutput::open_with_period`] and [`AlsaInput::open_with_period`]
//! negotiate: `HW_REFINE` asks the driver which of
//! [`PcmFormat::PREFERENCE`] (32-bit float first, 16-bit last) it takes at
//! which rate, trying the requested rate first and then the standard rates
//! nearest it ([`rate_candidates`]). The stream is opened in the best
//! format the device has — a 24-bit DAC is fed 24 bits, not a dithered
//! 16 — and [`AlsaOutput::rate`] says the rate it runs at, which the
//! [`backend`](crate::backend) converts to with the sinc resampler when it
//! differs from the one asked for. The plain `open` constructors keep the
//! old contract: `S16_LE` at exactly the rate given, or refusal.
//!
//! Scope, honestly: verified to compile and to fail cleanly on machines
//! without a sound card; the struct layout follows
//! `include/uapi/sound/asound.h` for 64-bit Linux. Streaming with
//! callbacks, and every other platform's audio system — PipeWire, JACK,
//! PulseAudio, Core Audio, WASAPI, AAudio and Web Audio — is in
//! [`backend`](crate::backend).

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

/// A PCM sample encoding a device takes, little-endian — negotiated by
/// the ALSA backend and by WASAPI exclusive mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PcmFormat {
    /// 32-bit IEEE float (`FLOAT_LE`).
    F32,
    /// 32-bit signed integer (`S32_LE`).
    S32,
    /// 24-bit signed in the low bytes of a 32-bit word (`S24_LE`).
    S24,
    /// 24-bit signed, packed in 3 bytes (`S24_3LE`) — common on USB.
    S24Packed,
    /// 16-bit signed (`S16_LE`).
    S16,
}

impl PcmFormat {
    /// Best first: what negotiation tries, in order.
    pub const PREFERENCE: [Self; 5] = [Self::F32, Self::S32, Self::S24, Self::S24Packed, Self::S16];

    /// The kernel's `SNDRV_PCM_FORMAT_*` number.
    #[must_use]
    pub const fn kernel_id(self) -> u32 {
        match self {
            Self::S16 => 2,
            Self::S24 => 6,
            Self::S32 => 10,
            Self::F32 => 14,
            Self::S24Packed => 32,
        }
    }

    /// Bytes one sample occupies.
    #[must_use]
    pub const fn bytes(self) -> usize {
        match self {
            Self::S16 => 2,
            Self::S24Packed => 3,
            Self::F32 | Self::S32 | Self::S24 => 4,
        }
    }

    /// Significant bits.
    #[must_use]
    pub const fn bits(self) -> u32 {
        match self {
            Self::S16 => 16,
            Self::S24 | Self::S24Packed => 24,
            Self::S32 => 32,
            Self::F32 => 24, // the mantissa
        }
    }
}

/// Rates to try for a request of `rate` Hz: the request itself, then the
/// standard rates by distance from it (ties to the higher, which keeps the
/// signal's band).
#[must_use]
pub fn rate_candidates(rate: u32) -> Vec<u32> {
    const STANDARD: [u32; 13] = [
        8_000, 11_025, 16_000, 22_050, 32_000, 44_100, 48_000, 64_000, 88_200, 96_000, 176_400,
        192_000, 384_000,
    ];
    let mut v: Vec<u32> = STANDARD.iter().copied().filter(|&r| r != rate).collect();
    v.sort_by_key(|&r| (r.abs_diff(rate), std::cmp::Reverse(r)));
    v.insert(0, rate);
    v
}

/// The first (rate, format) pair `supports` accepts, rates outermost (the
/// requested rate in any format beats a conversion), formats best first
/// within `allowed`.
pub fn negotiate(
    rate: u32,
    allowed: &[PcmFormat],
    mut supports: impl FnMut(PcmFormat, u32) -> bool,
) -> Option<(PcmFormat, u32)> {
    rate_candidates(rate).into_iter().find_map(|r| {
        PcmFormat::PREFERENCE
            .iter()
            .copied()
            .filter(|f| allowed.contains(f))
            .find(|&f| supports(f, r))
            .map(|f| (f, r))
    })
}

/// Encode float samples to `fmt` into `dst` (which must hold
/// `src.len() * fmt.bytes()` bytes). Integer formats are clamped to full
/// scale; 16- and 24-bit ones get TPDF dither from `dither` first, so the
/// word-length reduction is noise, not distortion.
///
/// # Panics
/// If `dst` is too short.
pub fn encode_samples(
    src: &[f32],
    fmt: PcmFormat,
    dst: &mut [u8],
    dither: &mut crate::wav::Dither,
) {
    let w = fmt.bytes();
    assert!(dst.len() >= src.len() * w, "destination too short");
    #[expect(
        clippy::cast_possible_truncation,
        reason = "clamped to the integer range"
    )]
    let int = |v: f32, full: f64| {
        (f64::from(v).clamp(-1.0, 1.0) * full)
            .round()
            .clamp(-full - 1.0, full) as i32
    };
    match fmt {
        PcmFormat::F32 => {
            for (v, d) in src.iter().zip(dst.as_chunks_mut::<4>().0) {
                *d = v.clamp(-1.0, 1.0).to_le_bytes();
            }
        }
        PcmFormat::S32 => {
            for (v, d) in src.iter().zip(dst.as_chunks_mut::<4>().0) {
                *d = int(*v, f64::from(i32::MAX)).to_le_bytes();
            }
        }
        PcmFormat::S24 | PcmFormat::S24Packed => {
            let full = f64::from((1 << 23) - 1);
            #[expect(clippy::cast_possible_truncation, reason = "an LSB")]
            let lsb = (1.0 / full) as f32;
            for (v, d) in src.iter().zip(dst.chunks_exact_mut(w)) {
                let x = int(v + dither.tpdf() * lsb, full).to_le_bytes();
                d.copy_from_slice(&x[..w]);
                if w == 4 {
                    // Sign-extend into the unused top byte.
                    d[3] = if x[2] & 0x80 == 0 { 0 } else { 0xFF };
                }
            }
        }
        PcmFormat::S16 => {
            let full = f64::from(i16::MAX);
            #[expect(clippy::cast_possible_truncation, reason = "an LSB")]
            let lsb = (1.0 / full) as f32;
            for (v, d) in src.iter().zip(dst.as_chunks_mut::<2>().0) {
                #[expect(clippy::cast_possible_truncation, reason = "within i16 by the clamp")]
                let x = int(v + dither.tpdf() * lsb, full) as i16;
                *d = x.to_le_bytes();
            }
        }
    }
}

/// Decode `fmt` bytes to float samples (`dst.len()` samples are read).
///
/// # Panics
/// If `src` is too short.
pub fn decode_samples(src: &[u8], fmt: PcmFormat, dst: &mut [f32]) {
    let w = fmt.bytes();
    assert!(src.len() >= dst.len() * w, "source too short");
    #[expect(clippy::cast_possible_truncation, reason = "to f32")]
    let scale = |x: i32, full: f64| (f64::from(x) / full) as f32;
    for (d, b) in dst.iter_mut().zip(src.chunks_exact(w)) {
        *d = match fmt {
            PcmFormat::F32 => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            PcmFormat::S32 => scale(
                i32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                2_147_483_648.0,
            ),
            PcmFormat::S24 | PcmFormat::S24Packed => {
                // Sign-extend from bit 23 by shifting through the top byte.
                scale(i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8, 8_388_608.0)
            }
            PcmFormat::S16 => scale(i32::from(i16::from_le_bytes([b[0], b[1]])), 32_768.0),
        };
    }
}

#[cfg(target_os = "linux")]
mod alsa {
    use super::{negotiate, PcmFormat};
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
    const HW_REFINE: u64 = ioc(3, 0x10, std::mem::size_of::<HwParams>() as u64);
    const HW_PARAMS: u64 = ioc(3, 0x11, std::mem::size_of::<HwParams>() as u64);
    const PREPARE: u64 = ioc(0, 0x40, 0);
    const DRAIN: u64 = ioc(0, 0x44, 0);
    const WRITEI: u64 = ioc(1, 0x50, std::mem::size_of::<Xferi>() as u64);
    const READI: u64 = ioc(2, 0x51, std::mem::size_of::<Xferi>() as u64);
    /// `SNDRV_PCM_IOCTL_DELAY`: frames between the application pointer
    /// and the DAC, as an `snd_pcm_sframes_t` (a C `long`).
    const DELAY: u64 = ioc(2, 0x21, std::mem::size_of::<std::ffi::c_long>() as u64);

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
    const EPIPE: i32 = 32;

    pub(super) struct Pcm {
        file: File,
        pub channels: u32,
        pub format: PcmFormat,
        pub rate: u32,
    }

    fn errno() -> i32 {
        std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
    }

    /// A parameter set: RW-interleaved `fmt` at exactly `rate`, `channels`,
    /// periods bracketing `period`. Sample and frame widths are left open:
    /// the kernel derives them from the format.
    fn params(channels: u32, period: u32, fmt: PcmFormat, rate: u32) -> HwParams {
        let full = Interval {
            min: 0,
            max: u32::MAX,
            flags: 0,
        };
        let mut p = HwParams {
            flags: 0,
            masks: [[u32::MAX; MASK_WORDS]; 3],
            mres: [[0; MASK_WORDS]; 5],
            intervals: [full; 12],
            ires: [Interval {
                min: 0,
                max: 0,
                flags: 0,
            }; 9],
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
        set_mask(&mut p.masks[FORMAT], fmt.kernel_id());
        set_mask(&mut p.masks[SUBFORMAT], 0);
        let exact = |v: u32| Interval {
            min: v,
            max: v,
            flags: 0b100,
        };
        let bits = u32::try_from(fmt.bytes() * 8).unwrap_or(32);
        p.intervals[I_SAMPLE_BITS] = exact(bits);
        p.intervals[I_FRAME_BITS] = exact(bits * channels);
        p.intervals[I_CHANNELS] = exact(channels);
        p.intervals[I_RATE] = exact(rate);
        // The period bracket follows the request rather than ignoring it:
        // a 64-frame low-latency stream used to be paced by 512-frame
        // device periods no matter what it asked for. The kernel fixates a
        // value inside the bracket — the request itself down and four times
        // the request up is slack enough for most hardware while keeping
        // the granted period near what was asked for. The buffer follows
        // from period × periods, so it brackets the bracket.
        let pmin = period.max(32);
        let pmax = period.saturating_mul(4).clamp(pmin, 16_384);
        p.intervals[I_PERIOD_SIZE] = Interval {
            min: pmin,
            max: pmax,
            flags: 0b100,
        };
        p.intervals[I_PERIODS] = Interval {
            min: 2,
            max: 8,
            flags: 0b100,
        };
        p.intervals[I_BUFFER_SIZE] = Interval {
            min: pmin.saturating_mul(2),
            max: pmax.saturating_mul(8),
            flags: 0b100,
        };
        p
    }

    impl Pcm {
        /// `S16_LE` at exactly `rate`, or refusal (the original contract).
        pub(super) fn open(
            card: u32,
            device: u32,
            rate: u32,
            channels: u32,
        ) -> Result<Self, String> {
            Self::open_dir(
                card,
                device,
                rate,
                channels,
                false,
                512,
                &[PcmFormat::S16],
                true,
            )
        }

        /// Open for playback, or (`capture`) for recording, in the best of
        /// `allowed` the device takes — at `rate` if it can, else (unless
        /// `exact_rate`) at the nearest standard rate it can.
        ///
        /// `period` is the frame count the caller wants the device to pace
        /// itself by; the range offered to the hardware brackets it.
        #[expect(clippy::too_many_arguments, reason = "the kernel's parameter set")]
        pub(super) fn open_dir(
            card: u32,
            device: u32,
            rate: u32,
            channels: u32,
            capture: bool,
            period: u32,
            allowed: &[PcmFormat],
            exact_rate: bool,
        ) -> Result<Self, String> {
            let path = format!(
                "/dev/snd/pcmC{card}D{device}{}",
                if capture { 'c' } else { 'p' }
            );
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .map_err(|e| format!("{path}: {e}"))?;
            let fd = file.as_raw_fd();
            // Ask, without committing: HW_REFINE narrows a parameter set to
            // what the driver can do and fails if nothing is left.
            let supports = |fmt: PcmFormat, r: u32| {
                if exact_rate && r != rate {
                    return false;
                }
                let mut p = params(channels, period, fmt, r);
                // SAFETY: `p` is a correctly sized snd_pcm_hw_params that
                // outlives the call; the request number encodes its size.
                unsafe { ioctl(fd, HW_REFINE, std::ptr::addr_of_mut!(p)) >= 0 }
            };
            let (format, granted) = negotiate(rate, allowed, supports).ok_or_else(|| {
                format!(
                    "{path}: no format of {allowed:?} at {}{rate} Hz × {channels} ch",
                    if exact_rate { "" } else { "or near " }
                )
            })?;
            let mut p = params(channels, period, format, granted);
            // SAFETY: as above.
            if unsafe { ioctl(fd, HW_PARAMS, std::ptr::addr_of_mut!(p)) } < 0 {
                return Err(format!("HW_PARAMS refused (errno {})", errno()));
            }
            // SAFETY: PREPARE takes no argument.
            if unsafe { ioctl(fd, PREPARE) } < 0 {
                return Err(format!("PREPARE refused (errno {})", errno()));
            }
            Ok(Self {
                file,
                channels,
                format,
                rate: granted,
            })
        }

        fn frame_bytes(&self) -> usize {
            self.format.bytes() * self.channels as usize
        }

        /// Write interleaved frames in the PCM's format, recovering from
        /// underruns.
        pub(super) fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
            let fd = self.file.as_raw_fd();
            let fb = self.frame_bytes();
            let mut off = 0;
            while off + fb <= bytes.len() {
                let mut x = Xferi {
                    result: 0,
                    buf: bytes[off..].as_ptr().cast(),
                    frames: ((bytes.len() - off) / fb) as u64,
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
                off += usize::try_from(x.result).unwrap_or(0) * fb;
            }
            Ok(())
        }

        /// Read interleaved frames in the PCM's format, recovering from
        /// overruns (the first read starts the capture stream).
        pub(super) fn read_bytes(&mut self, bytes: &mut [u8]) -> Result<(), String> {
            let fd = self.file.as_raw_fd();
            let fb = self.frame_bytes();
            let mut off = 0;
            while off + fb <= bytes.len() {
                let mut x = Xferi {
                    result: 0,
                    buf: bytes[off..].as_mut_ptr().cast_const().cast(),
                    frames: ((bytes.len() - off) / fb) as u64,
                };
                // SAFETY: `buf` points at room for the remaining frames for
                // the duration of the call; `x` is a valid snd_xferi.
                let r = unsafe { ioctl(fd, READI, std::ptr::addr_of_mut!(x)) };
                if r < 0 {
                    if errno() == EPIPE {
                        // SAFETY: PREPARE takes no argument.
                        unsafe { ioctl(fd, PREPARE) };
                        continue;
                    }
                    return Err(format!("READI failed (errno {})", errno()));
                }
                off += usize::try_from(x.result).unwrap_or(0) * fb;
            }
            Ok(())
        }

        /// Frames queued between what was just written and the DAC — the
        /// device's own latency figure.
        pub(super) fn delay(&self) -> Option<u64> {
            let mut d: std::ffi::c_long = 0;
            // SAFETY: an open PCM and a `long` out-pointer.
            let r = unsafe { ioctl(self.file.as_raw_fd(), DELAY, std::ptr::addr_of_mut!(d)) };
            (r >= 0).then(|| u64::try_from(d).unwrap_or(0))
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
            assert_eq!(HW_REFINE, 0xC260_4110);
            assert_eq!(WRITEI, 0x4018_4150);
            assert_eq!(READI, 0x8018_4151);
            assert_eq!(PREPARE, 0x4140);
            // _IOR('A', 0x21, long): 0x80084121 on 64-bit.
            #[cfg(target_pointer_width = "64")]
            assert_eq!(DELAY, 0x8008_4121);
        }
    }
}

/// A PCM stream to an ALSA device.
pub struct AlsaOutput {
    #[cfg(target_os = "linux")]
    pcm: alsa::Pcm,
    #[cfg(target_os = "linux")]
    bytes: Vec<u8>,
    #[cfg(target_os = "linux")]
    dither: crate::wav::Dither,
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
            alsa::Pcm::open(card, device, rate, 2).map(Self::from_pcm)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (card, device, rate);
            Err("ALSA exists only on Linux".into())
        }
    }

    #[cfg(target_os = "linux")]
    fn from_pcm(pcm: alsa::Pcm) -> Self {
        Self {
            pcm,
            bytes: Vec::new(),
            dither: crate::wav::Dither::new(0x5EED_A15A),
        }
    }

    /// Open card/device for `channels` channels, paced by device periods
    /// near `period` frames — the spelling the streaming backend uses,
    /// where the period is the block the render loop runs at — in the best
    /// [`PcmFormat`] the device takes, at `rate` or else the nearest
    /// standard rate it runs at ([`rate`](Self::rate) says which).
    ///
    /// # Errors
    /// As [`open`](Self::open).
    pub fn open_with_period(
        card: u32,
        device: u32,
        rate: u32,
        channels: u32,
        period: u32,
    ) -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        {
            alsa::Pcm::open_dir(
                card,
                device,
                rate,
                channels,
                false,
                period,
                &PcmFormat::PREFERENCE,
                false,
            )
            .map(Self::from_pcm)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (card, device, rate, channels, period);
            Err("ALSA exists only on Linux".into())
        }
    }

    /// As [`open_with_period`](Self::open_with_period), but only in one of
    /// `formats` (best first, as [`PcmFormat::PREFERENCE`] orders them) —
    /// to pin a format, as the loopback tests do for each in turn.
    ///
    /// # Errors
    /// As [`open`](Self::open), including when the device takes none of
    /// `formats`.
    pub fn open_with_formats(
        card: u32,
        device: u32,
        rate: u32,
        channels: u32,
        period: u32,
        formats: &[PcmFormat],
    ) -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        {
            alsa::Pcm::open_dir(card, device, rate, channels, false, period, formats, false)
                .map(Self::from_pcm)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (card, device, rate, channels, period, formats);
            Err("ALSA exists only on Linux".into())
        }
    }

    /// Open card/device for `rate` Hz, `channels` channels, `S16_LE`.
    ///
    /// # Errors
    /// As [`open`](Self::open).
    pub fn open_channels(card: u32, device: u32, rate: u32, channels: u32) -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        {
            alsa::Pcm::open(card, device, rate, channels).map(Self::from_pcm)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (card, device, rate, channels);
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
            if self.pcm.format == PcmFormat::S16 {
                self.bytes.clear();
                self.bytes
                    .extend(frames.iter().flat_map(|v| v.to_le_bytes()));
                let bytes = std::mem::take(&mut self.bytes);
                let r = self.pcm.write_bytes(&bytes);
                self.bytes = bytes;
                r
            } else {
                let f: Vec<f32> = frames.iter().map(|&v| f32::from(v) / 32_768.0).collect();
                self.write_f32(&f)
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = frames;
            Err("ALSA exists only on Linux".into())
        }
    }

    /// Queue interleaved float frames, encoded to the device's format
    /// (dithered where it is 16 or 24 bits). Allocation-free once the
    /// scratch buffer has grown to the block size.
    ///
    /// # Errors
    /// The device failed mid-stream.
    pub fn write_f32(&mut self, frames: &[f32]) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        {
            let need = frames.len() * self.pcm.format.bytes();
            if self.bytes.len() < need {
                self.bytes.resize(need, 0);
            }
            encode_samples(
                frames,
                self.pcm.format,
                &mut self.bytes[..need],
                &mut self.dither,
            );
            let bytes = std::mem::take(&mut self.bytes);
            let r = self.pcm.write_bytes(&bytes[..need]);
            self.bytes = bytes;
            r
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = frames;
            Err("ALSA exists only on Linux".into())
        }
    }

    /// The sample format the device was opened in.
    #[must_use]
    pub fn format(&self) -> PcmFormat {
        #[cfg(target_os = "linux")]
        {
            self.pcm.format
        }
        #[cfg(not(target_os = "linux"))]
        {
            PcmFormat::S16
        }
    }

    /// The rate the device runs at (the one asked for unless it had to
    /// fall back to another).
    #[must_use]
    pub fn rate(&self) -> u32 {
        #[cfg(target_os = "linux")]
        {
            self.pcm.rate
        }
        #[cfg(not(target_os = "linux"))]
        {
            0
        }
    }

    /// Wait for queued frames to play out.
    pub fn drain(&mut self) {
        #[cfg(target_os = "linux")]
        self.pcm.drain();
    }

    /// Frames between the last write and the DAC, as the kernel reports
    /// them (`None` if it will not say).
    #[must_use]
    pub fn delay(&self) -> Option<u64> {
        #[cfg(target_os = "linux")]
        {
            self.pcm.delay()
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
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

/// A PCM capture stream from an ALSA device.
pub struct AlsaInput {
    #[cfg(target_os = "linux")]
    pcm: alsa::Pcm,
    #[cfg(target_os = "linux")]
    bytes: Vec<u8>,
}

impl std::fmt::Debug for AlsaInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AlsaInput")
    }
}

impl AlsaInput {
    /// Open card/device for capture at `rate` Hz, `channels` channels.
    ///
    /// # Errors
    /// No such device or refused parameters; on non-Linux targets, always.
    pub fn open(card: u32, device: u32, rate: u32, channels: u32) -> Result<Self, String> {
        Self::open_with_period(card, device, rate, channels, 512)
    }

    /// Open for capture, with device periods near `period` frames, in the
    /// best [`PcmFormat`] the device has, at `rate` or the nearest
    /// standard rate it runs at ([`rate`](Self::rate)).
    ///
    /// # Errors
    /// No such device or refused parameters; on non-Linux targets, always.
    pub fn open_with_period(
        card: u32,
        device: u32,
        rate: u32,
        channels: u32,
        period: u32,
    ) -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        {
            alsa::Pcm::open_dir(
                card,
                device,
                rate,
                channels,
                true,
                period,
                &PcmFormat::PREFERENCE,
                false,
            )
            .map(|pcm| Self {
                pcm,
                bytes: Vec::new(),
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (card, device, rate, channels, period);
            Err("ALSA exists only on Linux".into())
        }
    }

    /// As [`open_with_period`](Self::open_with_period), but only in one of
    /// `formats`.
    ///
    /// # Errors
    /// As [`open`](Self::open), including when the device takes none of
    /// `formats`.
    pub fn open_with_formats(
        card: u32,
        device: u32,
        rate: u32,
        channels: u32,
        period: u32,
        formats: &[PcmFormat],
    ) -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        {
            alsa::Pcm::open_dir(card, device, rate, channels, true, period, formats, false).map(
                |pcm| Self {
                    pcm,
                    bytes: Vec::new(),
                },
            )
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (card, device, rate, channels, period, formats);
            Err("ALSA exists only on Linux".into())
        }
    }

    /// Fill `frames` with interleaved samples (blocks until they arrive).
    ///
    /// # Errors
    /// The device failed mid-stream.
    pub fn read(&mut self, frames: &mut [i16]) -> Result<(), String> {
        let mut f = vec![0.0f32; frames.len()];
        self.read_f32(&mut f)?;
        #[expect(clippy::cast_possible_truncation, reason = "clamped to i16")]
        for (d, v) in frames.iter_mut().zip(&f) {
            *d = (v * 32_768.0).round().clamp(-32_768.0, 32_767.0) as i16;
        }
        Ok(())
    }

    /// Fill `frames` with interleaved float samples decoded from the
    /// device's format (blocks until they arrive). Allocation-free once
    /// the scratch buffer has grown to the block size.
    ///
    /// # Errors
    /// The device failed mid-stream.
    pub fn read_f32(&mut self, frames: &mut [f32]) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        {
            let need = frames.len() * self.pcm.format.bytes();
            if self.bytes.len() < need {
                self.bytes.resize(need, 0);
            }
            self.pcm.read_bytes(&mut self.bytes[..need])?;
            decode_samples(&self.bytes[..need], self.pcm.format, frames);
            Ok(())
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = frames;
            Err("ALSA exists only on Linux".into())
        }
    }

    /// The sample format the device was opened in.
    #[must_use]
    pub fn format(&self) -> PcmFormat {
        #[cfg(target_os = "linux")]
        {
            self.pcm.format
        }
        #[cfg(not(target_os = "linux"))]
        {
            PcmFormat::S16
        }
    }

    /// The rate the device runs at.
    #[must_use]
    pub fn rate(&self) -> u32 {
        #[cfg(target_os = "linux")]
        {
            self.pcm.rate
        }
        #[cfg(not(target_os = "linux"))]
        {
            0
        }
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
        let s = render_sound(
            &Sound::Tone(Tone::held(440.0, Waveform::Sine, Duration::from_millis(50))),
            48_000,
        );
        let q = to_i16_stereo(&s);
        assert_eq!(q.len(), s.data.len() * 2);
        assert!(q.iter().any(|&v| v > 10_000));
    }

    #[test]
    fn wav_sink_writes_a_readable_file() {
        let dir = std::env::temp_dir().join(format!("vieww-wavsink-{}", std::process::id()));
        let p = WavSink { dir: dir.clone() }
            .write(
                "beep",
                &Sound::Tone(Tone::held(
                    880.0,
                    Waveform::Square,
                    Duration::from_millis(20),
                )),
            )
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
            p.play(&Sound::Tone(Tone::held(
                440.0,
                Waveform::Sine,
                Duration::from_millis(5)
            ))),
            Err(ServiceError::Failed(_))
        ));
    }

    #[test]
    fn rates_are_tried_requested_first_then_nearest() {
        let c = rate_candidates(48_000);
        assert_eq!(&c[..4], &[48_000, 44_100, 64_000, 32_000]);
        let c = rate_candidates(47_000);
        assert_eq!(&c[..3], &[47_000, 48_000, 44_100]);
        // Equidistant: the higher one keeps the band.
        assert_eq!(rate_candidates(40_000)[1], 44_100);
        assert_eq!(rate_candidates(44_100).len(), 13);
    }

    #[test]
    fn negotiation_prefers_the_requested_rate_then_the_best_format() {
        use PcmFormat::{S24Packed, F32, S16, S32};
        // A USB DAC: S24_3LE or S16 at 44.1/48/96 kHz.
        let usb = |f: PcmFormat, r: u32| {
            matches!(f, S24Packed | S16) && [44_100, 48_000, 96_000].contains(&r)
        };
        assert_eq!(
            negotiate(48_000, &PcmFormat::PREFERENCE, usb),
            Some((S24Packed, 48_000))
        );
        // The requested rate in a lesser format beats a conversion.
        let odd = |f: PcmFormat, r: u32| (f == S16 && r == 48_000) || (f == F32 && r == 44_100);
        assert_eq!(
            negotiate(48_000, &PcmFormat::PREFERENCE, odd),
            Some((S16, 48_000))
        );
        // A 44.1 kHz-only codec asked for 48 kHz: the nearest it runs at.
        let cd = |f: PcmFormat, r: u32| f == S32 && r == 44_100;
        assert_eq!(
            negotiate(48_000, &PcmFormat::PREFERENCE, cd),
            Some((S32, 44_100))
        );
        // `allowed` restricts: the legacy S16-only open.
        assert_eq!(negotiate(48_000, &[S16], usb), Some((S16, 48_000)));
        assert_eq!(negotiate(48_000, &[F32], usb), None);
    }

    #[test]
    fn every_format_round_trips_within_its_resolution() {
        let src: Vec<f32> = (0..2_000)
            .map(|i| ((i as f32) * 0.0137).sin() * 0.9)
            .chain([0.0, 1.0, -1.0, 0.5, -0.5, 1e-7, -1e-7])
            .collect();
        for fmt in PcmFormat::PREFERENCE {
            let mut bytes = vec![0u8; src.len() * fmt.bytes()];
            let mut d = crate::wav::Dither::new(7);
            encode_samples(&src, fmt, &mut bytes, &mut d);
            let mut back = vec![0.0f32; src.len()];
            decode_samples(&bytes, fmt, &mut back);
            // Rounding plus TPDF (±1 LSB) for 16/24 bits.
            let tol = match fmt {
                PcmFormat::F32 => 0.0,
                PcmFormat::S32 => 2.0 / 2_147_483_648.0,
                _ => 2.5 / f32::powi(2.0, fmt.bits() as i32 - 1),
            };
            for (a, b) in src.iter().zip(&back) {
                assert!((a - b).abs() <= tol + 1e-7, "{fmt:?}: {a} → {b}");
            }
        }
    }

    #[test]
    fn integer_encodings_are_exact_little_endian_and_clamped() {
        let mut d = crate::wav::Dither::new(1);
        let mut b = [0u8; 8];
        encode_samples(&[1.5, -3.0], PcmFormat::S32, &mut b, &mut d);
        assert_eq!(i32::from_le_bytes(b[..4].try_into().unwrap()), i32::MAX);
        assert_eq!(i32::from_le_bytes(b[4..].try_into().unwrap()), -i32::MAX);
        // S24 in 32 bits: sign-extended top byte; packed: three bytes.
        encode_samples(&[-1.0, 1.0], PcmFormat::S24, &mut b, &mut d);
        assert_eq!(i32::from_le_bytes(b[..4].try_into().unwrap()), -8_388_607);
        assert_eq!(i32::from_le_bytes(b[4..].try_into().unwrap()), 8_388_607);
        let mut p = [0u8; 6];
        encode_samples(&[-1.0, 1.0], PcmFormat::S24Packed, &mut p, &mut d);
        assert_eq!(p, [0x01, 0x00, 0x80, 0xFF, 0xFF, 0x7F]);
        // F32: the bits of the (clamped) value.
        encode_samples(&[0.25, 2.0], PcmFormat::F32, &mut b, &mut d);
        assert_eq!(f32::from_le_bytes(b[..4].try_into().unwrap()), 0.25);
        assert_eq!(f32::from_le_bytes(b[4..].try_into().unwrap()), 1.0);
    }

    #[test]
    fn sixteen_bit_output_is_dithered_not_truncated() {
        // A −100 dBFS sine vanishes into silence when rounded to 16 bits;
        // dithered, it survives as a measurable correlation.
        let amp = 10f32.powf(-100.0 / 20.0);
        let n = 48_000;
        let src: Vec<f32> = (0..n)
            .map(|i| (i as f32 * 2.0 * std::f32::consts::PI * 1_000.0 / 48_000.0).sin() * amp)
            .collect();
        let mut bytes = vec![0u8; n * 2];
        encode_samples(
            &src,
            PcmFormat::S16,
            &mut bytes,
            &mut crate::wav::Dither::new(3),
        );
        let mut back = vec![0.0f32; n];
        decode_samples(&bytes, PcmFormat::S16, &mut back);
        let corr: f64 = src
            .iter()
            .zip(&back)
            .map(|(a, b)| f64::from(*a) * f64::from(*b))
            .sum();
        let energy: f64 = src.iter().map(|a| f64::from(*a).powi(2)).sum();
        let gain = corr / energy;
        assert!(
            (gain - 1.0).abs() < 0.35,
            "the sine survives dithering (gain {gain:.2})"
        );
    }
}
