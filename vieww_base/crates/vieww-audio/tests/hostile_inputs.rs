//! Every file in `tests/hostile/` — inputs that once crashed a decoder,
//! hung it, made it allocate without bound, or decode to garbage — must now
//! either be refused or decode to something a mixer can take. Runs on
//! stable as part of `cargo test`; the `fuzz/` targets are the search, this
//! is what keeps what the search found fixed.

use std::path::Path;

use vieww_audio::codec::{self, Decoded, MAX_CHANNELS, RATE_RANGE, RUNAWAY};

/// The same oracle as `fuzz/fuzz_targets/common`: refusing is fine,
/// accepting garbage is not.
fn check(name: &str, d: &Decoded) {
    assert!(RATE_RANGE.contains(&d.rate), "{name}: rate {}", d.rate);
    assert!(
        (1..=MAX_CHANNELS).contains(&d.channels),
        "{name}: channels {}",
        d.channels
    );
    assert_eq!(
        d.data.len() % usize::from(d.channels),
        0,
        "{name}: samples not a whole number of frames"
    );
    for v in &d.data {
        assert!(v.is_finite() && v.abs() <= RUNAWAY, "{name}: sample {v}");
    }
}

#[test]
fn every_hostile_input_is_refused_or_decodes_to_sane_audio() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/hostile");
    let mut seen = 0;
    for entry in std::fs::read_dir(&dir).expect("tests/hostile") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let bytes = std::fs::read(&path).expect("read");
        // The entry point the file was found through…
        let direct = match name.split('-').next().unwrap() {
            "decode" => codec::decode(&bytes),
            "mp3" => codec::mp3::decode(&bytes),
            "flac" => codec::flac::decode(&bytes),
            "aiff" => codec::aiff::decode(&bytes),
            "aac_adts" => codec::aac::decode_adts(&bytes),
            "aac_mp4" => codec::aac::decode_mp4(&bytes),
            "vorbis" => codec::vorbis::decode(&bytes),
            other => panic!("{name}: unknown entry prefix {other:?}"),
        };
        if let Ok(d) = &direct {
            check(&name, d);
        }
        // …and the sniffing front door the loader uses, which must agree.
        if let Ok(d) = codec::decode(&bytes) {
            check(&name, &d);
        }
        seen += 1;
    }
    assert!(
        seen >= 10,
        "expected the hostile corpus, found {seen} files"
    );
}
