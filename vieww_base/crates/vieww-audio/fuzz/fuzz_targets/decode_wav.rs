//! The WAV reader (`codec::decode`'s `Wav` arm) on unconstrained bytes —
//! the sniffer never routes non-RIFF input here, so this target is what
//! reaches its header parsing deeply.
//!
//! Sample *values* are not checked: float WAV is a documented passthrough,
//! so out-of-range (even non-finite) sample values are the file's own. The
//! shape must still be sane — a rate, channel count and frame accounting
//! the rest of the pipeline can survive.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok((m, _)) = vieww_audio::wav::read_wav_multi(data) {
        assert!(m.rate >= 1 && m.rate <= 768_000, "rate {}", m.rate);
        let ch = m.layout.channels();
        assert!(ch >= 1 && ch <= 32, "channels {ch}");
        // The reader caps allocation by the declared frame count; the
        // returned data must not exceed it.
        assert!(
            m.data.len() / ch.max(1) <= 1 << 24,
            "unreasonable frame count: {}",
            m.data.len() / ch.max(1)
        );
    }
});
