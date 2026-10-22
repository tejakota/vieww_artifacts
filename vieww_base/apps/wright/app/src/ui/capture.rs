//! The microphone, through vieww's audio backends (WASAPI, CoreAudio,
//! PipeWire / Pulse / ALSA / JACK — whichever the machine has).
//!
//! The audio thread only appends samples and updates a level meter; the
//! UI owns the [`Recording`] and dropping it closes the device. Turning the
//! samples into text is `wright_runtime::voice` (run on a worker thread).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use vieww::audio::backend::{open_input, BlockInfo, DeviceStream, StreamConfig};

pub struct Recording {
    _stream: DeviceStream,
    samples: Arc<Mutex<Vec<f32>>>,
    /// (channels, rate) as the device actually delivered them.
    format: Arc<Mutex<(usize, u32)>>,
    /// Recent peak, 0..1, as f32 bits.
    level: Arc<AtomicU32>,
}

impl Recording {
    /// Start listening on the default input device.
    ///
    /// # Errors
    /// No backend could open a microphone (each backend's reason, joined).
    pub fn start() -> Result<Recording, String> {
        let samples = Arc::new(Mutex::new(Vec::<f32>::new()));
        let format = Arc::new(Mutex::new((1usize, 48_000u32)));
        let level = Arc::new(AtomicU32::new(0));
        let limit = 48_000 * 2 * wright_runtime::voice::MAX_SECONDS;
        let (s, f, l) = (
            Arc::clone(&samples),
            Arc::clone(&format),
            Arc::clone(&level),
        );
        let stream = open_input(StreamConfig::default(), move || {
            let (s, f, l) = (Arc::clone(&s), Arc::clone(&f), Arc::clone(&l));
            Box::new(move |block: &[f32], info: &BlockInfo| {
                if let Ok(mut fm) = f.lock() {
                    *fm = (info.channels.max(1), info.rate);
                }
                let peak = block.iter().fold(0.0f32, |m, x| m.max(x.abs()));
                let old = f32::from_bits(l.load(Ordering::Relaxed));
                // Fast attack, slow release: a meter that reads as speech.
                l.store(peak.max(old * 0.92).to_bits(), Ordering::Relaxed);
                if let Ok(mut buf) = s.lock() {
                    if buf.len() < limit {
                        buf.extend_from_slice(block);
                    }
                }
            })
        })
        .map_err(|errors| {
            let why: Vec<String> = errors
                .iter()
                .map(|(name, e)| format!("{name}: {e}"))
                .collect();
            format!("no microphone could be opened ({})", why.join("; "))
        })?;
        Ok(Recording {
            _stream: stream,
            samples,
            format,
            level,
        })
    }

    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed)).clamp(0.0, 1.0)
    }

    /// Stop, and hand back 16 kHz mono speech.
    pub fn finish(self) -> Vec<f32> {
        let (channels, rate) = *self.format.lock().unwrap_or_else(|p| p.into_inner());
        let raw = std::mem::take(&mut *self.samples.lock().unwrap_or_else(|p| p.into_inner()));
        drop(self._stream);
        wright_runtime::voice::to_speech_rate(&raw, channels, rate)
    }
}
