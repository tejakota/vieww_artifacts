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
    DeviceStream, RenderFn, StreamConfig, StreamImpl,
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

impl Backend for Alsa {
    fn name(&self) -> &'static str {
        "alsa"
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
                let n = frames * usize::from(ch);
                let mut f = vec![0.0f32; n];
                let mut q = vec![0i16; n];
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
                    crate::simd::f32_to_i16(&f, &mut q);
                    if out.write(&q).is_err() {
                        c2.ended.store(true, Ordering::Release);
                        return;
                    }
                    done += frames as u64;
                    c2.block(frames);
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
