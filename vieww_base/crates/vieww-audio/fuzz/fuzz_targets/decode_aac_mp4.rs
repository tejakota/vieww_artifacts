//! The AAC decoder's MP4 (packaged) path: `stsz`/`stco`/`co64` boxes and
//! the fixed-size declarations their validation has to refuse.

#![no_main]

mod common;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(d) = vieww_audio::codec::aac::decode_mp4(data) {
        common::check(&d);
    }
});
