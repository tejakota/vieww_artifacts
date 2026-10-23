//! ALSA through the kernel directly — the backend that needs no server and
//! no library at all (see [`device`](crate::device) for the ioctls).
//!
//! A stream thread renders a block, converts it to `S16_LE` with the SIMD
//! kernels and writes it; the kernel's blocking write paces the thread.
//! Capture is the mirror image. Hardware access is exclusive: on a desktop
//! with a sound server running, the server holds the device and this
//! backend reports it busy — which is why it comes last in the order.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use super::{
    isolate_capture, isolate_render, Backend, BackendError, BlockInfo, CaptureFn, Counters,
    DeviceInfo, DeviceStream, RenderFn, StreamConfig, StreamImpl,
};
use crate::device::{AlsaInput, AlsaOutput};

/// The ALSA backend.
#[derive(Debug, Clone, Copy, Default)]
pub struct Alsa {
    /// Card number.
    pub card: u32,
    /// Device number on the card.
    pub device: u32,
}

struct AlsaStream {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl StreamImpl for AlsaStream {
    fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            t.join().ok();
        }
    }
}

fn unavailable_or_refused(e: String) -> BackendError {
    if e.contains("No such file") || e.contains("os error 2") {
        BackendError::Unavailable(e)
    } else {
        BackendError::Refused(e)
    }
}

/// The PCM devices in `/proc/asound/pcm`'s format — one line per device:
/// `00-00: ALC892 Analog : ALC892 Analog : playback 1 : capture 1`.
/// Channel counts are not in the file (opening tells); `outputs` is `None`
/// (unknown) for a playback device and `Some(0)` for capture-only.
fn parse_pcm(text: &str) -> Vec<DeviceInfo> {
    text.lines()
        .filter_map(|line| {
            let (ids, rest) = line.split_once(':')?;
            let (card, dev) = ids.trim().split_once('-')?;
            let (card, dev): (u32, u32) = (card.parse().ok()?, dev.parse().ok()?);
            let fields: Vec<&str> = rest.split(" : ").map(str::trim).collect();
            let name = fields.first().copied().unwrap_or("").trim().to_owned();
            let has = |w: &str| fields.iter().any(|f| f.starts_with(w));
            Some(DeviceInfo {
                id: format!("hw:{card},{dev}"),
                name: if name.is_empty() {
                    format!("card {card} device {dev}")
                } else {
                    name
                },
                outputs: if has("playback") { None } else { Some(0) },
                inputs: if has("capture") { None } else { Some(0) },
                is_default: card == 0 && dev == 0,
            })
        })
        .collect()
}

/// `hw:C,D` (or `plughw:C,D`, or `C,D`) as card and device numbers.
fn parse_hw(id: &str) -> Option<(u32, u32)> {
    let s = id.trim_start_matches("plughw:").trim_start_matches("hw:");
    let (c, d) = s.split_once(',').unwrap_or((s, "0"));
    Some((c.trim().parse().ok()?, d.trim().parse().ok()?))
}

impl Backend for Alsa {
    fn name(&self) -> &'static str {
        "alsa"
    }

    fn devices(&self) -> Result<Vec<DeviceInfo>, BackendError> {
        match std::fs::read_to_string("/proc/asound/pcm") {
            Ok(text) => Ok(parse_pcm(&text)),
            Err(e) => Err(BackendError::Unavailable(format!("no ALSA ({e})"))),
        }
    }

    /// `device` is `hw:C,D` from [`devices`](Backend::devices).
    fn open_output_on(
        &self,
        device: &str,
        config: StreamConfig,
        render: RenderFn,
    ) -> Result<DeviceStream, BackendError> {
        let (card, device) = parse_hw(device)
            .ok_or_else(|| BackendError::Refused(format!("not an ALSA device id: {device:?}")))?;
        Self { card, device }.open_output(config, render)
    }

    fn open_output(
        &self,
        config: StreamConfig,
        render: RenderFn,
    ) -> Result<DeviceStream, BackendError> {
        let ch = config.channels.clamp(1, 32);
        // The device period brackets the requested block, so the pacing the
        // stream asked for is the pacing the hardware tries to give — a
        // 64-frame block is no longer silently paced by 512-frame periods.
        let mut out = AlsaOutput::open_with_period(
            self.card,
            self.device,
            config.rate,
            u32::from(ch),
            config.block.max(64),
        )
        .map_err(unavailable_or_refused)?;
        let counters = Arc::new(Counters::default());
        // A panicking render closure must not kill the pacing thread —
        // isolate it so the failure is one message, then silence and
        // `ended`, like every other backend.
        let mut render = isolate_render(render, &counters);
        let stop = Arc::new(AtomicBool::new(false));
        let (c2, s2) = (counters.clone(), stop.clone());
        let frames = config.block.max(64) as usize;
        let rate = config.rate;
        let thread = std::thread::Builder::new()
            .name("vieww-alsa".into())
            .spawn(move || {
                #[expect(clippy::cast_precision_loss, reason = "a block length")]
                let _rt = crate::rt::promote_audio_thread(std::time::Duration::from_secs_f64(
                    frames as f64 / f64::from(rate.max(1)),
                ));
                let n = frames * usize::from(ch);
                let mut f = vec![0.0f32; n];
                let mut q = vec![0i16; n];
                // The device takes 16-bit samples: dither the reduction
                // (`wav::Dither`), or quiet passages round into distortion.
                let mut dither = crate::wav::Dither::new(0x5EED_A15A);
                let lsb = crate::wav::SampleFormat::I16.lsb().unwrap_or(0.0);
                let mut done = 0u64;
                while !s2.load(Ordering::Acquire) {
                    f.fill(0.0);
                    #[expect(clippy::cast_precision_loss, reason = "stream time")]
                    render(
                        &mut f,
                        &BlockInfo {
                            frames,
                            channels: usize::from(ch),
                            rate,
                            time: done as f64 / f64::from(rate),
                        },
                    );
                    for v in &mut f {
                        *v += dither.tpdf() * lsb;
                    }
                    crate::simd::f32_to_i16(&f, &mut q);
                    if out.write(&q).is_err() {
                        c2.ended.store(true, Ordering::Release);
                        return;
                    }
                    done += frames as u64;
                    c2.block(frames);
                    // The kernel's own count of frames still ahead of the
                    // DAC, every 64 blocks.
                    if c2.callbacks.load(Ordering::Relaxed) % 64 == 1 {
                        if let Some(d) = out.delay() {
                            c2.set_latency_frames(d, rate);
                        }
                    }
                }
                out.drain();
            })
            .map_err(|e| BackendError::Refused(e.to_string()))?;
        Ok(DeviceStream::new(
            "alsa",
            StreamConfig {
                channels: ch,
                block: config.block.max(64),
                ..config
            },
            counters,
            Box::new(AlsaStream {
                stop,
                thread: Some(thread),
            }),
        ))
    }

    fn open_input(
        &self,
        config: StreamConfig,
        capture: CaptureFn,
    ) -> Result<DeviceStream, BackendError> {
        let ch = config.channels.clamp(1, 32);
        let mut input = AlsaInput::open_with_period(
            self.card,
            self.device,
            config.rate,
            u32::from(ch),
            config.block.max(64),
        )
        .map_err(unavailable_or_refused)?;
        let counters = Arc::new(Counters::default());
        // As in `open_output`: a panicking capture is caught, not a dead
        // pacing thread.
        let mut capture = isolate_capture(capture, &counters);
        let stop = Arc::new(AtomicBool::new(false));
        let (c2, s2) = (counters.clone(), stop.clone());
        let frames = config.block.max(64) as usize;
        let rate = config.rate;
        let thread = std::thread::Builder::new()
            .name("vieww-alsa-in".into())
            .spawn(move || {
                #[expect(clippy::cast_precision_loss, reason = "a block length")]
                let _rt = crate::rt::promote_audio_thread(std::time::Duration::from_secs_f64(
                    frames as f64 / f64::from(rate.max(1)),
                ));
                let n = frames * usize::from(ch);
                let mut q = vec![0i16; n];
                let mut f = vec![0.0f32; n];
                let mut done = 0u64;
                while !s2.load(Ordering::Acquire) {
                    if input.read(&mut q).is_err() {
                        c2.ended.store(true, Ordering::Release);
                        return;
                    }
                    crate::simd::i16_to_f32(&q, &mut f);
                    #[expect(clippy::cast_precision_loss, reason = "stream time")]
                    capture(
                        &f,
                        &BlockInfo {
                            frames,
                            channels: usize::from(ch),
                            rate,
                            time: done as f64 / f64::from(rate),
                        },
                    );
                    done += frames as u64;
                    c2.block(frames);
                }
            })
            .map_err(|e| BackendError::Refused(e.to_string()))?;
        Ok(DeviceStream::new(
            "alsa",
            StreamConfig {
                channels: ch,
                block: config.block.max(64),
                ..config
            },
            counters,
            Box::new(AlsaStream {
                stop,
                thread: Some(thread),
            }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pcm_list_parses() {
        let text = "00-00: ALC892 Analog : ALC892 Analog : playback 1 : capture 1
00-01: ALC892 Digital : ALC892 Digital : playback 1
00-02: ALC892 Alt Analog : ALC892 Alt Analog : capture 1
01-03: HDMI 0 : HDMI 0 : playback 1
garbage line
";
        let d = parse_pcm(text);
        assert_eq!(d.len(), 4);
        assert_eq!(d[0].id, "hw:0,0");
        assert_eq!(d[0].name, "ALC892 Analog");
        assert!(d[0].is_default && d[0].outputs.is_none() && d[0].inputs.is_none());
        assert_eq!(d[1].inputs, Some(0), "playback only");
        assert_eq!(d[2].outputs, Some(0), "capture only");
        assert_eq!(d[3].id, "hw:1,3");
        assert_eq!(parse_hw("hw:1,3"), Some((1, 3)));
        assert_eq!(parse_hw("plughw:2,0"), Some((2, 0)));
        assert_eq!(parse_hw("3"), Some((3, 0)));
        assert_eq!(parse_hw("default"), None);
    }

    #[test]
    fn a_named_device_that_is_absent_refuses() {
        let cfg = StreamConfig::default();
        assert!(Alsa::default()
            .open_output_on("hw:9,9", cfg, Box::new(|_, _| {}))
            .is_err());
        assert!(matches!(
            Alsa::default().open_output_on("speakers", cfg, Box::new(|_, _| {})),
            Err(BackendError::Refused(_))
        ));
    }

    #[test]
    fn a_missing_card_is_unavailable_not_a_crash() {
        let a = Alsa {
            card: 97,
            device: 3,
        };
        let e = a
            .open_output(StreamConfig::default(), Box::new(|_, _| {}))
            .unwrap_err();
        assert!(matches!(e, BackendError::Unavailable(_)), "{e}");
        let e = a
            .open_input(StreamConfig::default(), Box::new(|_, _| {}))
            .unwrap_err();
        assert!(matches!(e, BackendError::Unavailable(_)), "{e}");
    }
}
