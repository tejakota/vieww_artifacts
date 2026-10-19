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
    assert!(
        d.rate >= 1 && d.rate <= 768_000,
        "sample rate out of range: {}",
        d.rate
    );
    assert!(
        d.channels >= 1 && d.channels <= 32,
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
    assert!(peak <= 32.0, "runaway magnitude in the output: {peak}");
}
