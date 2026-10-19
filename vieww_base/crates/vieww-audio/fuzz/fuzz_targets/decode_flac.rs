//! The FLAC decoder (`codec::decode`'s `Flac` arm). Its hostile-input
//! hardening — wasted-bits validation and the total-sample cap — is
//! exactly what this target hammers.

#![no_main]

mod common;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(d) = vieww_audio::codec::flac::decode(data) {
        common::check(&d);
    }
});
