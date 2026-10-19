//! The Vorbis decoder (`codec::decode`'s `Ogg` arm): codebook setup,
//! floors, residues — the parts of the format a hostile file lives in.

#![no_main]

mod common;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(d) = vieww_audio::codec::vorbis::decode(data) {
        common::check(&d);
    }
});
