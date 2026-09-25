//! Format sniffing. Never trusts file extensions.
//!
//! On device this is fed by `PHAsset` / `MediaStore` metadata; here it reads a
//! small header prefix so the harness can run over an ordinary folder.

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Routing bucket. The compression matrix is keyed on this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssetClass {
    Jpeg,
    Png,
    Heic,
    /// MP4/MOV carrying H.264 — the one class with large transcode headroom.
    VideoH264,
    /// MP4/MOV carrying HEVC — already efficient.
    VideoHevc,
    VideoAv1,
    VideoOther,
    /// DNG / ProRAW / camera raw. Never re-encoded, in any tier.
    Raw,
    AudioLossless,
    AudioLossy,
    Document,
    Other,
}

impl AssetClass {
    pub fn label(self) -> &'static str {
        match self {
            AssetClass::Jpeg => "JPEG",
            AssetClass::Png => "PNG",
            AssetClass::Heic => "HEIC",
            AssetClass::VideoH264 => "Video H.264",
            AssetClass::VideoHevc => "Video HEVC",
            AssetClass::VideoAv1 => "Video AV1",
            AssetClass::VideoOther => "Video other",
            AssetClass::Raw => "RAW/DNG",
            AssetClass::AudioLossless => "Audio lossless",
            AssetClass::AudioLossy => "Audio lossy",
            AssetClass::Document => "Document",
            AssetClass::Other => "Other",
        }
    }

    pub fn is_video(self) -> bool {
        matches!(
            self,
            AssetClass::VideoH264
                | AssetClass::VideoHevc
                | AssetClass::VideoAv1
                | AssetClass::VideoOther
        )
    }

    pub fn is_photo(self) -> bool {
        matches!(self, AssetClass::Jpeg | AssetClass::Png | AssetClass::Heic)
    }

    /// Assets the engine must never re-encode, regardless of tier.
    pub fn is_protected(self) -> bool {
        matches!(self, AssetClass::Raw)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetMeta {
    pub path: String,
    pub bytes: u64,
    pub class: AssetClass,
}

/// How much of the file header we inspect. Enough for `ftyp` + the first
/// `moov`/`stsd` fourcc in practice, without reading whole videos.
const SNIFF_LEN: usize = 256 * 1024;

pub fn classify_path(path: &Path) -> anyhow::Result<AssetMeta> {
    let bytes = std::fs::metadata(path)?.len();
    let mut f = File::open(path)?;
    let mut head = vec![0u8; SNIFF_LEN.min(bytes as usize)];
    let n = f.read(&mut head)?;
    head.truncate(n);

    Ok(AssetMeta {
        path: path.display().to_string(),
        bytes,
        class: classify_bytes(&head, path),
    })
}

/// Classify from a file name alone — no bytes read, no file opened.
///
/// This is the *pre-consent* path. On device the tier screen has to describe a
/// selection before the user has granted access to it, so all it has is what
/// the OS picker already showed them: a display name and a size. The guess is
/// replaced by [`classify_bytes`] the moment a descriptor exists, and the two
/// disagreeing is a fact worth logging rather than papering over — an extension
/// is a claim, and this engine does not act on claims.
pub fn classify_name(name: &str) -> AssetClass {
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "jpg" | "jpeg" | "jpe" | "jfif" => AssetClass::Jpeg,
        "png" => AssetClass::Png,
        "heic" | "heif" | "avif" => AssetClass::Heic,
        "mp4" | "mov" | "m4v" | "mkv" | "webm" => AssetClass::VideoOther,
        "dng" | "cr2" | "cr3" | "nef" | "arw" | "raf" | "orf" | "rw2" => AssetClass::Raw,
        "flac" | "wav" | "aiff" => AssetClass::AudioLossless,
        "mp3" | "m4a" | "aac" | "ogg" | "opus" => AssetClass::AudioLossy,
        "txt" | "md" | "json" | "csv" | "xml" | "log" | "html" | "pdf" => AssetClass::Document,
        _ => AssetClass::Other,
    }
}

pub fn classify_bytes(head: &[u8], path: &Path) -> AssetClass {
    let kind = infer::get(head);
    let mime = kind.map(|k| k.mime_type()).unwrap_or("");

    match mime {
        "image/jpeg" => return AssetClass::Jpeg,
        "image/png" => return AssetClass::Png,
        "image/heif" | "image/heic" | "image/avif" => return AssetClass::Heic,
        "image/x-canon-cr2" | "image/x-nikon-nef" | "image/tiff" => return AssetClass::Raw,
        "audio/x-flac" | "audio/x-wav" | "audio/wav" => return AssetClass::AudioLossless,
        "audio/mpeg" | "audio/m4a" | "audio/aac" | "audio/ogg" => return AssetClass::AudioLossy,
        "video/mp4" | "video/quicktime" | "video/x-matroska" | "video/webm" => {
            return classify_video(head)
        }
        _ => {}
    }

    // DNG is a TIFF variant that `infer` may not label; fall back to extension
    // for the raw family only, since misrouting a RAW is the costly error.
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "dng" | "cr2" | "cr3" | "nef" | "arw" | "raf" | "orf" | "rw2" => AssetClass::Raw,
        "txt" | "md" | "json" | "csv" | "xml" | "log" | "html" | "pdf" => AssetClass::Document,
        "" => AssetClass::Other,
        _ => {
            if head.iter().take(8192).all(|b| *b == b'\n' || *b == b'\r' || *b == b'\t' || (*b >= 0x20 && *b < 0x7f)) {
                AssetClass::Document
            } else {
                AssetClass::Other
            }
        }
    }
}

/// Crude sample-entry sniff: look for the codec fourcc in the header region.
///
/// A production build parses the `moov`/`trak`/`stsd` box tree properly (see
/// `mp4parse`); this is adequate for corpus measurement because the sample
/// entry lives in `moov`, which sits at the head of a faststart file and at the
/// tail otherwise — we accept the tail miss and report it as `VideoOther`.
fn classify_video(head: &[u8]) -> AssetClass {
    let has = |tag: &[u8]| head.windows(4).any(|w| w == tag);
    if has(b"hvc1") || has(b"hev1") {
        AssetClass::VideoHevc
    } else if has(b"avc1") || has(b"avc3") {
        AssetClass::VideoH264
    } else if has(b"av01") {
        AssetClass::VideoAv1
    } else {
        AssetClass::VideoOther
    }
}
