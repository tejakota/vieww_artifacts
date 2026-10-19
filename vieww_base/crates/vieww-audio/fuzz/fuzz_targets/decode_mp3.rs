//! The MP3 decoder (`codec::decode`'s `Mp3` arm), Xing tags and all.

#![no_main]

mod common;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(d) = vieww_audio::codec::mp3::decode(data) {
        common::check(&d);
    }
});
