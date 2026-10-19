//! The AIFF decoder (`codec::decode`'s `Aiff` arm).

#![no_main]

mod common;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(d) = vieww_audio::codec::aiff::decode(data) {
        common::check(&d);
    }
});
