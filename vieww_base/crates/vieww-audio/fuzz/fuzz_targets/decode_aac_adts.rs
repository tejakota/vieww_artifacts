//! The AAC decoder's ADTS (streamed) path — raw frames, no container.

#![no_main]

mod common;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(d) = vieww_audio::codec::aac::decode_adts(data) {
        common::check(&d);
    }
});
