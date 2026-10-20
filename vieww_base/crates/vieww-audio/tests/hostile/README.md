# Hostile inputs

Each file here once made a decoder panic, allocate without bound, or "succeed"
with output no mixer should receive. Each was found by mutation fuzzing
(2026-10-20), minimised, and checked to fail on the code it was found in.
`tests/hostile_inputs.rs` runs all of them on every `cargo test`, on stable
Rust, so a regression is caught without the nightly toolchain the
`fuzz/` targets need.

The prefix names the entry point the file is fed to (`decode` is
`codec::decode`); the rest names what it used to do.

| file | entry | what it did before |
|---|---|---|
| `mp3-truncated-xing-flags.bin` | `codec::mp3::decode` | index out of bounds reading the Xing flags (`mp3.rs:899`) |
| `mp3-truncated-xing-frames.bin` | `codec::mp3::decode` | index out of bounds reading the Xing frame count (`mp3.rs:903`) |
| `mp3-runaway-global-gain.bin` | `codec::mp3::decode` | decoded "successfully" to samples in the millions |
| `aac_adts-fill-element-count-underflow.bin` | `codec::aac::decode_adts` | `usize` underflow in the fill element's escape count |
| `aac_adts-runaway-gain.bin` | `codec::aac::decode_adts` | decoded to samples far past full scale |
| `aac_mp4-huge-allocation.bin` | `codec::aac::decode_mp4` | allocation of many gigabytes from one box count |
| `flac-huge-allocation.bin` | `codec::flac::decode` | allocation of many gigabytes from STREAMINFO |
| `decode-flac-fixed-predictor-overflow.bin` | `codec::decode` | `i32` multiply overflow in the fixed predictor |
| `aiff-sample-rate-out-of-range.bin` | `codec::aiff::decode` | accepted a nonsensical sample rate |
| `aiff-channel-count-out-of-range.bin` | `codec::aiff::decode` | accepted a nonsensical channel count |

To add one: drop the (minimised) file here with the right prefix and a row
above. The test discovers files by name.
