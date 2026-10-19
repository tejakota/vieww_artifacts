//! The fuzz oracle, run against every known-good fixture: a harness that
//! fails on valid files is worse than no harness. (This is a normal test,
//! not a fuzz target — `cargo test` inside this crate runs it.)

#[path = "../fuzz_targets/common/mod.rs"]
mod common;

fn fixtures() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures")
}

#[test]
fn every_fixture_passes_the_oracle() {
    let mut checked = 0;
    let mut rejected = 0;
    let dir = fixtures();
    let names = std::fs::read_dir(&dir)
        .expect("fixtures directory")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| {
            !n.starts_with('.')
                && !n.contains("make")
                && !n.ends_with(".ref")
                && !n.starts_with("README")
        })
        .collect::<Vec<_>>();
    assert!(!names.is_empty(), "no fixtures found in {}", dir.display());
    for name in &names {
        let bytes = std::fs::read(dir.join(name)).expect(name);
        match vieww_audio::codec::decode(&bytes) {
            Ok(d) => {
                common::check(&d);
                checked += 1;
            }
            Err(_) => rejected += 1,
        }
    }
    // Every fixture is meant to decode — a rejection here means the
    // oracle's shape bounds (or a decoder) drifted.
    assert_eq!(rejected, 0, "a fixture stopped decoding");
    assert!(checked >= 10, "only {checked} fixtures checked");
}
