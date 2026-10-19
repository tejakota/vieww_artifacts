//! `codec::decode` end to end: format sniffing, dispatch, and whichever
//! decoder claims the bytes. This is the entry the loader hands real
//! files, so it is the entry hostile bytes arrive at.

#![no_main]

mod common;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(d) = vieww_audio::codec::decode(data) {
        common::check(&d);
    }
});
