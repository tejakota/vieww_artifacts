//! The oracle every decode target shares.
//!
//! A decoder is allowed to reject hostile bytes (`Err`); it is not allowed
//! to accept them and poison the pipeline. So a decode that returns `Ok`
//! must have a sane shape and be finite everywhere — the invariants the
//! hostile-input hardening in the decoders exists to uphold, checked on
//! every input the fuzzer generates. (Sample *range* is deliberately not
//! asserted: integer formats are bounded by construction, MP3 synthesis
//! can overshoot ±1 legitimately, and float WAV is a documented
//! passthrough — only runaway magnitude, 32 × full scale, is a bug.)

/// Check a successful decode.
pub fn check(d: &vieww_audio::codec::Decoded) {
    // The bounds every decoder enforces on its way out
    // (`codec::Decoded::validated`) — one definition, not a second guess.
    assert!(
        vieww_audio::codec::RATE_RANGE.contains(&d.rate),
        "sample rate out of range: {}",
        d.rate
    );
    assert!(
        d.channels >= 1 && d.channels <= vieww_audio::codec::MAX_CHANNELS,
        "channel count out of range: {}",
        d.channels
    );
    // The decoders cap total samples (the FLAC pre-allocation bound among
    // them); a "decode" larger than that is an allocation bug, not audio.
    assert!(
        d.frames() <= 1 << 24,
        "unreasonable frame count: {}",
        d.frames()
    );
    let mut peak = 0.0f32;
    for v in &d.data {
        assert!(v.is_finite(), "non-finite sample in the output");
        peak = peak.max(v.abs());
    }
    // Decoders conceal any block past `RUNAWAY` (+18 dBFS), so nothing past
    // it may survive a decode.
    assert!(
        peak <= vieww_audio::codec::RUNAWAY,
        "runaway magnitude in the output: {peak}"
    );
}
