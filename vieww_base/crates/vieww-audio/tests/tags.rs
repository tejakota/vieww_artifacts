//! Tag metadata against an independent implementation.
//!
//! Readers: files ffmpeg tagged (`tests/fixtures/make.sh`) — ID3v2.4 with
//! ID3v1, ID3v2.3, FLAC and Ogg Vorbis/Opus comments, MP4 atoms, RIFF
//! `INFO`, AIFF with an `ID3 ` chunk. Writers: files tagged here, read back
//! by ffprobe (skipped where ffprobe is not installed).

use vieww_audio::codec::tags::{self, Tags};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

const TITLE: &str = "Vātāpi Gaṇapatim";

fn check(name: &str, custom: bool) {
    let t = tags::read(&fixture(name));
    assert_eq!(t.title(), Some(TITLE), "{name}: {t:?}");
    assert_eq!(t.artist(), Some("Ensemble"), "{name}");
    assert_eq!(t.album(), Some("Kritis"), "{name}");
    assert_eq!(t.date(), Some("2026"), "{name}");
    assert_eq!(t.genre(), Some("Carnatic"), "{name}");
    assert_eq!(t.track(), Some("3/8"), "{name}");
    assert_eq!(t.comment(), Some("tagged by ffmpeg"), "{name}");
    assert!(
        t.get("ENCODER").is_some_and(|e| e.starts_with("Lav")),
        "{name}: encoder"
    );
    if custom {
        assert_eq!(t.get("RAGA"), Some("Hamsadhwani"), "{name}: custom key");
    }
}

#[test]
fn id3v24_and_id3v1() {
    check("tags24.mp3", true);
    // ID3v1's copies (truncated to 30 bytes, Latin-1) are merged, not
    // duplicated, where they agree.
    let t = tags::read(&fixture("tags24.mp3"));
    assert_eq!(t.all("ARTIST").count(), 1);
}

#[test]
fn id3v23() {
    check("tags23.mp3", true);
}

#[test]
fn flac_vorbis_comments() {
    check("tags.flac", true);
}

#[test]
fn ogg_vorbis_comments() {
    check("tags.ogg", true);
}

#[test]
fn ogg_opus_tags() {
    check("tags.opus", true);
}

#[test]
fn mp4_atoms() {
    // ffmpeg writes custom keys to MP4 only with `use_metadata_tags`.
    check("tags.m4a", false);
}

#[test]
fn riff_info() {
    // RIFF INFO has no field for custom keys.
    check("tags.wav", false);
}

#[test]
fn aiff_id3_chunk() {
    check("tags.aiff", true);
}

#[test]
fn untagged_files_have_no_tags_beyond_the_encoder() {
    for f in ["source16.wav", "src24.aiff"] {
        let t = tags::read(&fixture(f));
        assert!(t.title().is_none(), "{f}: {t:?}");
    }
}

/// ffprobe's view of a file's tags, lower-cased keys.
fn ffprobe(bytes: &[u8], ext: &str) -> Option<Vec<(String, String)>> {
    let dir = std::env::temp_dir().join(format!("vieww-tags-{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    let p = dir.join(format!("t.{ext}"));
    std::fs::write(&p, bytes).ok()?;
    let out = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format_tags",
            "-of",
            "flat=s=_",
        ])
        .arg(&p)
        .output()
        .ok()?;
    let _ = std::fs::remove_dir_all(&dir);
    if !out.status.success() {
        return None;
    }
    Some(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| {
                let (k, v) = l.strip_prefix("format_tags_")?.split_once('=')?;
                Some((k.to_ascii_lowercase(), v.trim_matches('"').to_owned()))
            })
            .collect(),
    )
}

fn has(tags: &[(String, String)], k: &str, v: &str) -> bool {
    tags.iter().any(|(a, b)| a == k && b == v)
}

#[test]
fn ffprobe_reads_what_the_writers_write() {
    let samples = vieww_audio::Samples::stereo(vec![0.0; 4_800], 48_000);
    let wav = vieww_audio::write_wav(&samples).unwrap();
    let t = Tags::new()
        .with("TITLE", TITLE)
        .with("ARTIST", "Ensemble")
        .with("ALBUM", "Kritis")
        .with("DATE", "2026")
        .with("GENRE", "Carnatic")
        .with("TRACKNUMBER", "3")
        .with("COMMENT", "tagged by vieww")
        .with("RAGA", "Hamsadhwani");

    let tagged = tags::write_wav(&wav, &t).unwrap();
    let Some(probe) = ffprobe(&tagged, "wav") else {
        eprintln!("ffprobe not available — skipped");
        return;
    };
    for (k, v) in [
        ("title", TITLE),
        ("artist", "Ensemble"),
        ("album", "Kritis"),
        ("date", "2026"),
        ("genre", "Carnatic"),
        ("track", "3"),
        ("comment", "tagged by vieww"),
    ] {
        assert!(has(&probe, k, v), "wav {k}: {probe:?}");
    }

    let d = vieww_audio::codec::decode(&wav).unwrap();
    let flac = tags::write_flac(&vieww_audio::codec::flac::encode(&d, 16).unwrap(), &t).unwrap();
    let probe = ffprobe(&flac, "flac").unwrap();
    for (k, v) in [
        ("title", TITLE),
        ("artist", "Ensemble"),
        ("raga", "Hamsadhwani"),
        ("comment", "tagged by vieww"),
        ("track", "3"),
    ] {
        assert!(has(&probe, k, v), "flac {k}: {probe:?}");
    }

    let aiff = tags::write_aiff(&vieww_audio::codec::aiff::encode(&d, 16).unwrap(), &t).unwrap();
    let probe = ffprobe(&aiff, "aiff").unwrap();
    // ffprobe calls AIFF's `AUTH` "author".
    for (k, v) in [
        ("title", TITLE),
        ("author", "Ensemble"),
        ("comment", "tagged by vieww"),
    ] {
        assert!(has(&probe, k, v), "aiff {k}: {probe:?}");
    }
}
