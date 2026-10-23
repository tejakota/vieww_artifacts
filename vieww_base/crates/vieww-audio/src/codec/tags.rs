//! Tag metadata — title, artist, album and the rest — read from every
//! container the crate decodes, and written to the ones it encodes.
//!
//! One vocabulary: [`Tags`] holds `(KEY, value)` pairs under Vorbis-comment
//! names (`TITLE`, `ARTIST`, `ALBUM`, `ALBUMARTIST`, `DATE`, `GENRE`,
//! `TRACKNUMBER`, `DISCNUMBER`, `COMMENT`, `COMPOSER`, `COPYRIGHT`,
//! `ENCODER`), whatever the container called them, so code that reads a
//! title does not care whether it came from an ID3 `TIT2` frame, a RIFF
//! `INAM` chunk or an MP4 `©nam` atom. Keys are case-insensitive; a key may
//! repeat (two artists).
//!
//! | Container | Read | Write |
//! |---|---|---|
//! | MP3 | ID3v2.2/2.3/2.4 (unsynchronisation, extended headers, UTF-16/8), ID3v1/1.1 | — |
//! | FLAC | `VORBIS_COMMENT` block | [`write_flac`] |
//! | Ogg | Vorbis comment header, `OpusTags` | — |
//! | WAV | `LIST`/`INFO` chunk, embedded `id3 ` chunk | [`write_wav`] |
//! | AIFF | `NAME`/`AUTH`/`ANNO`/`(c) ` chunks, embedded `ID3 ` chunk | [`write_aiff`] |
//! | MP4/M4A | `moov/udta/meta/ilst` atoms, freeform `----` | — |
//!
//! Readers never fail: a malformed or truncated tag yields what could be
//! read before the damage (the audio decoders are separate and stricter).
//! Writers replace any tags already in the file and leave the audio bytes
//! untouched. The readers are checked against files ffmpeg tagged, and
//! the writers' output against ffprobe — see `tests/tags.rs`.

use super::{sniff, CodecError, Format};

/// Tag metadata: ordered `(KEY, value)` pairs, keys upper-case.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tags {
    entries: Vec<(String, String)>,
}

impl Tags {
    /// No tags.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Builder form of [`set`](Self::set).
    #[must_use]
    pub fn with(mut self, key: &str, value: &str) -> Self {
        self.set(key, value);
        self
    }

    /// Set `key` to `value`, replacing any values it had.
    pub fn set(&mut self, key: &str, value: &str) {
        let k = key.to_ascii_uppercase();
        self.entries.retain(|(e, _)| *e != k);
        self.entries.push((k, value.to_owned()));
    }

    /// Add a value for `key`, keeping the ones it has.
    pub fn push(&mut self, key: &str, value: &str) {
        self.entries
            .push((key.to_ascii_uppercase(), value.to_owned()));
    }

    /// Add `value` for `key` unless that exact pair is already present
    /// (readers use this when a file carries the same tag twice, as ID3v2
    /// plus ID3v1 or `LIST` plus `id3 `).
    fn merge(&mut self, key: &str, value: &str) {
        let k = key.to_ascii_uppercase();
        let v = value.trim_end_matches('\0').trim();
        if v.is_empty() || self.entries.iter().any(|(e, x)| *e == k && x == v) {
            return;
        }
        self.entries.push((k, v.to_owned()));
    }

    /// The first value of `key`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    /// Every value of `key`, in order.
    pub fn all<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.entries
            .iter()
            .filter(move |(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    /// Every pair, in order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// How many pairs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `TITLE`.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.get("TITLE")
    }

    /// `ARTIST`.
    #[must_use]
    pub fn artist(&self) -> Option<&str> {
        self.get("ARTIST")
    }

    /// `ALBUM`.
    #[must_use]
    pub fn album(&self) -> Option<&str> {
        self.get("ALBUM")
    }

    /// `DATE`.
    #[must_use]
    pub fn date(&self) -> Option<&str> {
        self.get("DATE")
    }

    /// `GENRE`.
    #[must_use]
    pub fn genre(&self) -> Option<&str> {
        self.get("GENRE")
    }

    /// `TRACKNUMBER`.
    #[must_use]
    pub fn track(&self) -> Option<&str> {
        self.get("TRACKNUMBER")
    }

    /// `COMMENT`, or else `DESCRIPTION` — the Vorbis-comment spec's name
    /// for it, which ffmpeg and others write.
    #[must_use]
    pub fn comment(&self) -> Option<&str> {
        self.get("COMMENT").or_else(|| self.get("DESCRIPTION"))
    }
}

/// The tags in a file of any supported container (empty if it has none or
/// the container is not recognised).
#[must_use]
pub fn read(bytes: &[u8]) -> Tags {
    let mut t = Tags::new();
    match sniff(bytes) {
        Some(Format::Mp3) => {
            id3v2(bytes, &mut t);
            id3v1(bytes, &mut t);
        }
        Some(Format::Adts) => id3v1(bytes, &mut t),
        Some(Format::Flac) => flac(bytes, &mut t),
        Some(Format::Ogg) => ogg(bytes, &mut t),
        Some(Format::Wav) => wav(bytes, &mut t),
        Some(Format::Aiff) => aiff(bytes, &mut t),
        Some(Format::Mp4) => mp4(bytes, &mut t),
        None => {}
    }
    t
}

// ---- helpers -------------------------------------------------------------

fn le32(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        b.get(i..i.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn be32(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        b.get(i..i.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| char::from(c)).collect()
}

/// Text with a trailing NUL (and anything after it) removed.
fn cstr(b: &[u8]) -> &[u8] {
    b.iter().position(|&c| c == 0).map_or(b, |n| &b[..n])
}

/// UTF-8 if it is, else Latin-1 (RIFF and AIFF text has no declared
/// encoding; both occur in the wild).
fn text8(b: &[u8]) -> String {
    let b = cstr(b);
    std::str::from_utf8(b).map_or_else(|_| latin1(b), str::to_owned)
}

fn utf16(b: &[u8], big: bool) -> String {
    let units: Vec<u16> = b
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| {
            if big {
                u16::from_be_bytes([c[0], c[1]])
            } else {
                u16::from_le_bytes([c[0], c[1]])
            }
        })
        .collect();
    String::from_utf16_lossy(&units)
}

/// Vorbis-comment key for a key-ish string from any container.
fn canonical(key: &str) -> String {
    let k = key.trim().to_ascii_uppercase();
    match k.as_str() {
        "YEAR" => "DATE".into(),
        "TRACK" => "TRACKNUMBER".into(),
        "DISC" => "DISCNUMBER".into(),
        "ALBUM ARTIST" | "ALBUM_ARTIST" => "ALBUMARTIST".into(),
        _ => k,
    }
}

// ---- Vorbis comments (FLAC, Ogg) ------------------------------------------

/// Parse a Vorbis comment structure (little-endian lengths).
fn vorbis_comments(b: &[u8], t: &mut Tags) {
    let Some(vendor) = le32(b, 0) else { return };
    let mut at = 4usize.saturating_add(vendor as usize);
    let Some(n) = le32(b, at) else { return };
    at += 4;
    for _ in 0..n {
        let Some(len) = le32(b, at) else { return };
        let start = at + 4;
        let Some(c) = start
            .checked_add(len as usize)
            .and_then(|end| b.get(start..end))
        else {
            return;
        };
        at = start + len as usize;
        let s = String::from_utf8_lossy(c);
        if let Some((k, v)) = s.split_once('=') {
            t.merge(&canonical(k), v);
        }
    }
}

fn vorbis_comment_block(t: &Tags) -> Vec<u8> {
    let vendor = concat!("vieww-audio ", env!("CARGO_PKG_VERSION"));
    let mut b = Vec::new();
    let len = |n: usize| u32::try_from(n).unwrap_or(u32::MAX).to_le_bytes();
    b.extend_from_slice(&len(vendor.len()));
    b.extend_from_slice(vendor.as_bytes());
    b.extend_from_slice(&len(t.len()));
    for (k, v) in t.iter() {
        let c = format!("{k}={v}");
        b.extend_from_slice(&len(c.len()));
        b.extend_from_slice(c.as_bytes());
    }
    b
}

fn flac(b: &[u8], t: &mut Tags) {
    let mut at = 4;
    while let Some(&h) = b.get(at) {
        let Some(len) = b.get(at + 1..at + 4) else {
            return;
        };
        let len = (usize::from(len[0]) << 16) | (usize::from(len[1]) << 8) | usize::from(len[2]);
        let Some(body) = b.get(at + 4..at + 4 + len) else {
            return;
        };
        // Only VORBIS_COMMENT (4) carries text; a non-standard ID3v2 tag
        // before `fLaC` is handled by the sniffer as MP3.
        if h & 0x7F == 4 {
            vorbis_comments(body, t);
        }
        if h & 0x80 != 0 {
            return;
        }
        at += 4 + len;
    }
}

fn ogg(b: &[u8], t: &mut Tags) {
    let Ok(packets) = super::ogg::packets(b) else {
        return;
    };
    let Some(p) = packets.get(1) else { return };
    let d = &p.data;
    if d.len() > 7 && d[0] == 3 && &d[1..7] == b"vorbis" {
        vorbis_comments(&d[7..], t);
    } else if d.starts_with(b"OpusTags") {
        vorbis_comments(&d[8..], t);
    }
}

/// Write `tags` into a FLAC file, replacing any `VORBIS_COMMENT` and
/// `PADDING` blocks; the audio frames and every other metadata block are
/// kept byte for byte.
///
/// # Errors
/// Not a FLAC stream, or its metadata blocks are malformed.
pub fn write_flac(file: &[u8], tags: &Tags) -> Result<Vec<u8>, CodecError> {
    if !file.starts_with(b"fLaC") {
        return Err(CodecError::NotThisFormat);
    }
    let mut blocks: Vec<(u8, &[u8])> = Vec::new();
    let mut at = 4;
    loop {
        let h = *file.get(at).ok_or(CodecError::Truncated)?;
        let l = file.get(at + 1..at + 4).ok_or(CodecError::Truncated)?;
        let len = (usize::from(l[0]) << 16) | (usize::from(l[1]) << 8) | usize::from(l[2]);
        let body = file
            .get(at + 4..at + 4 + len)
            .ok_or(CodecError::Truncated)?;
        let kind = h & 0x7F;
        if kind == 127 {
            return Err(CodecError::Malformed("FLAC metadata block type 127"));
        }
        if kind != 4 && kind != 1 {
            blocks.push((kind, body));
        }
        at += 4 + len;
        if h & 0x80 != 0 {
            break;
        }
    }
    if blocks.first().map(|b| b.0) != Some(0) {
        return Err(CodecError::Malformed("FLAC without STREAMINFO first"));
    }
    let comment = vorbis_comment_block(tags);
    if comment.len() >= 1 << 24 {
        return Err(CodecError::Unsupported("tags over 16 MiB"));
    }
    // STREAMINFO, the comment, then the rest (seek table, pictures, …).
    blocks.insert(1, (4, &comment));
    let mut out = Vec::with_capacity(file.len() + comment.len());
    out.extend_from_slice(b"fLaC");
    let last = blocks.len() - 1;
    for (i, (kind, body)) in blocks.iter().enumerate() {
        out.push(kind | if i == last { 0x80 } else { 0 });
        let n = u32::try_from(body.len()).unwrap_or(0);
        out.extend_from_slice(&n.to_be_bytes()[1..]);
        out.extend_from_slice(body);
    }
    out.extend_from_slice(&file[at..]);
    Ok(out)
}

// ---- ID3 -------------------------------------------------------------------

/// ID3v1 genres 0–79 (the original list plus Winamp's first additions,
/// as far as every reader agrees).
const GENRES: [&str; 80] = [
    "Blues",
    "Classic Rock",
    "Country",
    "Dance",
    "Disco",
    "Funk",
    "Grunge",
    "Hip-Hop",
    "Jazz",
    "Metal",
    "New Age",
    "Oldies",
    "Other",
    "Pop",
    "R&B",
    "Rap",
    "Reggae",
    "Rock",
    "Techno",
    "Industrial",
    "Alternative",
    "Ska",
    "Death Metal",
    "Pranks",
    "Soundtrack",
    "Euro-Techno",
    "Ambient",
    "Trip-Hop",
    "Vocal",
    "Jazz+Funk",
    "Fusion",
    "Trance",
    "Classical",
    "Instrumental",
    "Acid",
    "House",
    "Game",
    "Sound Clip",
    "Gospel",
    "Noise",
    "AlternRock",
    "Bass",
    "Soul",
    "Punk",
    "Space",
    "Meditative",
    "Instrumental Pop",
    "Instrumental Rock",
    "Ethnic",
    "Gothic",
    "Darkwave",
    "Techno-Industrial",
    "Electronic",
    "Pop-Folk",
    "Eurodance",
    "Dream",
    "Southern Rock",
    "Comedy",
    "Cult",
    "Gangsta",
    "Top 40",
    "Christian Rap",
    "Pop/Funk",
    "Jungle",
    "Native American",
    "Cabaret",
    "New Wave",
    "Psychadelic",
    "Rave",
    "Showtunes",
    "Trailer",
    "Lo-Fi",
    "Tribal",
    "Acid Punk",
    "Acid Jazz",
    "Polka",
    "Retro",
    "Musical",
    "Rock & Roll",
    "Hard Rock",
];

/// `(17)` or `17` → `Rock`; `(17)Rock` → `Rock`; anything else as is.
fn genre(s: &str) -> String {
    let inner = s.strip_prefix('(').and_then(|r| r.split_once(')'));
    let (num, rest) = match inner {
        Some((n, rest)) => (n, rest),
        None => (s, ""),
    };
    if !rest.is_empty() {
        return rest.to_owned();
    }
    num.parse::<usize>()
        .ok()
        .and_then(|i| GENRES.get(i))
        .map_or_else(|| s.to_owned(), |g| (*g).to_owned())
}

fn id3_key(id: &str) -> Option<String> {
    Some(
        match id {
            "TIT2" | "TT2" => "TITLE",
            "TPE1" | "TP1" => "ARTIST",
            "TALB" | "TAL" => "ALBUM",
            "TPE2" | "TP2" => "ALBUMARTIST",
            "TDRC" | "TYER" | "TYE" | "TDRL" => "DATE",
            "TCON" | "TCO" => "GENRE",
            "TRCK" | "TRK" => "TRACKNUMBER",
            "TPOS" | "TPA" => "DISCNUMBER",
            "TCOM" | "TCM" => "COMPOSER",
            "TCOP" | "TCR" => "COPYRIGHT",
            "TSSE" | "TSS" | "TENC" | "TEN" => "ENCODER",
            "COMM" | "COM" => "COMMENT",
            "TXXX" | "TXX" => return Some(String::new()), // keyed by its description
            other if other.starts_with('T') => other,
            _ => return None,
        }
        .to_owned(),
    )
}

/// Decode an ID3v2 text field: an encoding byte's worth of strings,
/// NUL-separated (2.4 multi-values).
fn id3_strings(enc: u8, b: &[u8]) -> Vec<String> {
    let s = match enc {
        0 => latin1(b),
        1 => match b {
            [0xFF, 0xFE, r @ ..] => utf16(r, false),
            [0xFE, 0xFF, r @ ..] => utf16(r, true),
            r => utf16(r, false),
        },
        2 => utf16(b, true),
        _ => String::from_utf8_lossy(b).into_owned(),
    };
    // Multi-values in 2.4; a BOM restated after each separator in some
    // writers' UTF-16.
    s.split('\0')
        .map(|v| v.trim_start_matches('\u{FEFF}').to_owned())
        .filter(|v| !v.is_empty())
        .collect()
}

/// Split an encoded field at its first terminator (1 byte for Latin-1 and
/// UTF-8, an aligned 2-byte zero for UTF-16): (before, after).
fn id3_split(enc: u8, b: &[u8]) -> (&[u8], &[u8]) {
    if enc == 1 || enc == 2 {
        let mut i = 0;
        while i + 1 < b.len() {
            if b[i] == 0 && b[i + 1] == 0 {
                return (&b[..i], &b[i + 2..]);
            }
            i += 2;
        }
        (b, &[])
    } else {
        b.iter()
            .position(|&c| c == 0)
            .map_or((b, &[][..]), |n| (&b[..n], &b[n + 1..]))
    }
}

/// Undo unsynchronisation: `FF 00` → `FF`.
fn unsync(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(b.len());
    let mut prev = 0u8;
    for &c in b {
        if !(prev == 0xFF && c == 0) {
            out.push(c);
        }
        prev = c;
    }
    out
}

fn synchsafe(b: &[u8]) -> Option<usize> {
    let b = b.get(..4)?;
    if b.iter().any(|&x| x & 0x80 != 0) {
        return None;
    }
    Some(
        (usize::from(b[0]) << 21)
            | (usize::from(b[1]) << 14)
            | (usize::from(b[2]) << 7)
            | usize::from(b[3]),
    )
}

/// The ID3v2 tag at the start of `b` (MP3s, and `id3 ` chunks in RIFF and
/// AIFF).
fn id3v2(b: &[u8], t: &mut Tags) {
    if b.len() < 10 || &b[..3] != b"ID3" {
        return;
    }
    let (ver, flags) = (b[3], b[5]);
    if !(2..=4).contains(&ver) {
        return;
    }
    let Some(size) = synchsafe(&b[6..10]) else {
        return;
    };
    let Some(body) = b
        .get(10..10usize.saturating_add(size))
        .or_else(|| b.get(10..))
    else {
        return;
    };
    // Whole-tag unsynchronisation (2.2/2.3; 2.4 marks it per frame).
    let owned;
    let mut body: &[u8] = if flags & 0x80 != 0 && ver < 4 {
        owned = unsync(body);
        &owned
    } else {
        body
    };
    if flags & 0x40 != 0 && ver >= 3 {
        // Extended header: 2.3's size excludes its own 4 bytes, 2.4's
        // (synchsafe) includes them.
        let skip = if ver == 3 {
            be32(body, 0).map(|n| n as usize + 4)
        } else {
            synchsafe(body)
        };
        let Some(rest) = skip.and_then(|s| body.get(s..)) else {
            return;
        };
        body = rest;
    }
    let (id_len, hdr) = if ver == 2 { (3, 6) } else { (4, 10) };
    let mut at = 0;
    while at + hdr <= body.len() {
        let id = &body[at..at + id_len];
        if id[0] == 0 {
            break; // padding
        }
        let len = match ver {
            2 => {
                (usize::from(body[at + 3]) << 16)
                    | (usize::from(body[at + 4]) << 8)
                    | usize::from(body[at + 5])
            }
            3 => be32(body, at + 4).map_or(0, |n| n as usize),
            // 2.4 sizes are synchsafe; some writers used plain ones anyway.
            _ => synchsafe(&body[at + 4..])
                .unwrap_or_else(|| be32(body, at + 4).map_or(0, |n| n as usize)),
        };
        let (fl_status, fl_format) = if ver == 2 {
            (0, 0)
        } else {
            (body[at + 8], body[at + 9])
        };
        let _ = fl_status;
        let start = at + hdr;
        let Some(raw) = start.checked_add(len).and_then(|e| body.get(start..e)) else {
            break;
        };
        at = start + len;
        let id = latin1(id);
        let Some(key) = id3_key(&id) else { continue };
        // Compressed or encrypted frames: skip (3: 0x80/0x40; 4: 0x08/0x04).
        let (compressed, encrypted, frame_unsync, data_len) = match ver {
            3 => (fl_format & 0x80 != 0, fl_format & 0x40 != 0, false, false),
            4 => (
                fl_format & 0x08 != 0,
                fl_format & 0x04 != 0,
                fl_format & 0x02 != 0,
                fl_format & 0x01 != 0,
            ),
            _ => (false, false, false, false),
        };
        if compressed || encrypted {
            continue;
        }
        let owned_frame;
        let mut data: &[u8] = if frame_unsync {
            owned_frame = unsync(raw);
            &owned_frame
        } else {
            raw
        };
        if data_len {
            data = data.get(4..).unwrap_or(&[]);
        }
        let Some((&enc, rest)) = data.split_first() else {
            continue;
        };
        if key == "COMMENT" {
            // Language (3 bytes), short description, text.
            let Some(rest) = rest.get(3..) else { continue };
            let (_desc, text) = id3_split(enc, rest);
            for v in id3_strings(enc, text) {
                t.merge("COMMENT", &v);
            }
        } else if key.is_empty() {
            let (desc, value) = id3_split(enc, rest);
            let desc = id3_strings(enc, desc).concat();
            for v in id3_strings(enc, value) {
                t.merge(&canonical(&desc), &v);
            }
        } else {
            for v in id3_strings(enc, rest) {
                let v = if key == "GENRE" { genre(&v) } else { v };
                t.merge(&key, &v);
            }
        }
    }
}

/// The 128-byte ID3v1 tag at the end of `b`, if any.
fn id3v1(b: &[u8], t: &mut Tags) {
    let Some(tag) = b.len().checked_sub(128).and_then(|s| b.get(s..)) else {
        return;
    };
    if &tag[..3] != b"TAG" {
        return;
    }
    let field = |r: std::ops::Range<usize>| latin1(cstr(&tag[r])).trim_end().to_owned();
    t.merge("TITLE", &field(3..33));
    t.merge("ARTIST", &field(33..63));
    t.merge("ALBUM", &field(63..93));
    t.merge("DATE", &field(93..97));
    if tag[125] == 0 && tag[126] != 0 {
        // ID3v1.1: a track number in the comment's last two bytes.
        t.merge("COMMENT", &field(97..125));
        t.merge("TRACKNUMBER", &tag[126].to_string());
    } else {
        t.merge("COMMENT", &field(97..127));
    }
    if let Some(g) = GENRES.get(usize::from(tag[127])) {
        t.merge("GENRE", g);
    }
}

// ---- RIFF / WAV -------------------------------------------------------------

const INFO: [(&[u8; 4], &str); 12] = [
    (b"INAM", "TITLE"),
    (b"IART", "ARTIST"),
    (b"IPRD", "ALBUM"),
    (b"ICRD", "DATE"),
    (b"IGNR", "GENRE"),
    (b"ICMT", "COMMENT"),
    (b"ITRK", "TRACKNUMBER"),
    (b"IPRT", "TRACKNUMBER"),
    (b"ICOP", "COPYRIGHT"),
    (b"ISFT", "ENCODER"),
    (b"IMUS", "COMPOSER"),
    (b"ISBJ", "SUBJECT"),
];

/// Chunks of a RIFF (little-endian) or IFF (big-endian) body:
/// (id, body, offset of the chunk header).
fn chunks(b: &[u8], big: bool) -> Vec<([u8; 4], &[u8], usize)> {
    let mut out = Vec::new();
    let mut at = 0;
    while at + 8 <= b.len() {
        let id = [b[at], b[at + 1], b[at + 2], b[at + 3]];
        let len = if big {
            be32(b, at + 4)
        } else {
            le32(b, at + 4)
        }
        .unwrap_or(0) as usize;
        let start = at + 8;
        // A truncated last chunk (common for `data` in streamed files)
        // still yields what is there.
        let end = start.saturating_add(len).min(b.len());
        out.push((id, &b[start..end], at));
        at = start.saturating_add(len).saturating_add(len & 1);
    }
    out
}

fn wav(b: &[u8], t: &mut Tags) {
    let Some(body) = b.get(12..) else { return };
    for (id, c, _) in chunks(body, false) {
        if &id == b"LIST" && c.starts_with(b"INFO") {
            for (sid, text, _) in chunks(&c[4..], false) {
                if let Some((_, key)) = INFO.iter().find(|(f, _)| **f == sid) {
                    t.merge(key, &text8(text));
                }
            }
        } else if id.eq_ignore_ascii_case(b"id3 ") {
            id3v2(c, t);
        }
    }
}

/// Write `tags` into a WAV file as a `LIST`/`INFO` chunk before `data`,
/// replacing any `LIST`/`INFO` already there. Keys `INFO` has no field for
/// (anything beyond the twelve in its table) cannot be stored and are left
/// out; FLAC stores every key.
///
/// # Errors
/// Not a RIFF/WAVE file, or no `data` chunk.
pub fn write_wav(file: &[u8], tags: &Tags) -> Result<Vec<u8>, CodecError> {
    if file.len() < 12 || &file[..4] != b"RIFF" || &file[8..12] != b"WAVE" {
        return Err(CodecError::NotThisFormat);
    }
    let body = &file[12..];
    let mut list = b"INFO".to_vec();
    for (k, v) in tags.iter() {
        // The first INFO id for a key (ITRK over IPRT).
        let Some((id, _)) = INFO.iter().find(|(_, key)| *key == k) else {
            continue;
        };
        let mut text = v.as_bytes().to_vec();
        text.push(0);
        list.extend_from_slice(*id);
        list.extend_from_slice(&u32::try_from(text.len()).unwrap_or(0).to_le_bytes());
        list.extend_from_slice(&text);
        if text.len() % 2 == 1 {
            list.push(0);
        }
    }
    let mut out = Vec::with_capacity(file.len() + list.len() + 8);
    out.extend_from_slice(&file[..12]);
    let mut wrote = false;
    let mut saw_data = false;
    for (id, c, at) in chunks(body, false) {
        if &id == b"LIST" && c.starts_with(b"INFO") {
            continue;
        }
        if &id == b"data" {
            saw_data = true;
            if !wrote && list.len() > 4 {
                out.extend_from_slice(b"LIST");
                out.extend_from_slice(&u32::try_from(list.len()).unwrap_or(0).to_le_bytes());
                out.extend_from_slice(&list);
            }
            wrote = true;
        }
        let end = (at + 8 + c.len() + (c.len() & 1)).min(body.len());
        out.extend_from_slice(&body[at..end]);
    }
    if !saw_data {
        return Err(CodecError::Malformed("WAV without a data chunk"));
    }
    let riff =
        u32::try_from(out.len() - 8).map_err(|_| CodecError::Unsupported("WAV over 4 GiB"))?;
    out[4..8].copy_from_slice(&riff.to_le_bytes());
    Ok(out)
}

// ---- AIFF -------------------------------------------------------------------

const AIFF_TEXT: [(&[u8; 4], &str); 4] = [
    (b"NAME", "TITLE"),
    (b"AUTH", "ARTIST"),
    (b"(c) ", "COPYRIGHT"),
    (b"ANNO", "COMMENT"),
];

fn aiff(b: &[u8], t: &mut Tags) {
    let Some(body) = b.get(12..) else { return };
    for (id, c, _) in chunks(body, true) {
        if let Some((_, key)) = AIFF_TEXT.iter().find(|(f, _)| **f == id) {
            t.merge(key, &text8(c));
        } else if id.eq_ignore_ascii_case(b"ID3 ") {
            id3v2(c, t);
        }
    }
}

/// Write `tags` into an AIFF/AIFF-C file as `NAME`, `AUTH`, `(c) ` and
/// `ANNO` chunks before `SSND`, replacing those already there. AIFF has
/// fields for title, artist, copyright and comments only; other keys are
/// left out.
///
/// # Errors
/// Not a FORM/AIFF file, or no `SSND` chunk.
pub fn write_aiff(file: &[u8], tags: &Tags) -> Result<Vec<u8>, CodecError> {
    if file.len() < 12
        || &file[..4] != b"FORM"
        || !(&file[8..12] == b"AIFF" || &file[8..12] == b"AIFC")
    {
        return Err(CodecError::NotThisFormat);
    }
    let body = &file[12..];
    let mut text = Vec::new();
    for (id, key) in AIFF_TEXT {
        for v in tags.all(key) {
            text.extend_from_slice(id);
            text.extend_from_slice(&u32::try_from(v.len()).unwrap_or(0).to_be_bytes());
            text.extend_from_slice(v.as_bytes());
            if v.len() % 2 == 1 {
                text.push(0);
            }
        }
    }
    let mut out = Vec::with_capacity(file.len() + text.len());
    out.extend_from_slice(&file[..12]);
    let mut saw = false;
    for (id, c, at) in chunks(body, true) {
        if AIFF_TEXT.iter().any(|(f, _)| **f == id) {
            continue;
        }
        if &id == b"SSND" && !saw {
            saw = true;
            out.extend_from_slice(&text);
        }
        let end = (at + 8 + c.len() + (c.len() & 1)).min(body.len());
        out.extend_from_slice(&body[at..end]);
    }
    if !saw {
        return Err(CodecError::Malformed("AIFF without an SSND chunk"));
    }
    let form =
        u32::try_from(out.len() - 8).map_err(|_| CodecError::Unsupported("AIFF over 4 GiB"))?;
    out[4..8].copy_from_slice(&form.to_be_bytes());
    Ok(out)
}

// ---- MP4 --------------------------------------------------------------------

fn mp4_key(atom: &[u8; 4]) -> Option<&'static str> {
    Some(match atom {
        b"\xA9nam" => "TITLE",
        b"\xA9ART" => "ARTIST",
        b"\xA9alb" => "ALBUM",
        b"aART" => "ALBUMARTIST",
        b"\xA9day" => "DATE",
        b"\xA9gen" | b"gnre" => "GENRE",
        b"trkn" => "TRACKNUMBER",
        b"disk" => "DISCNUMBER",
        b"\xA9cmt" => "COMMENT",
        b"\xA9wrt" => "COMPOSER",
        b"cprt" => "COPYRIGHT",
        b"\xA9too" => "ENCODER",
        b"desc" => "DESCRIPTION",
        b"\xA9lyr" => "LYRICS",
        _ => return None,
    })
}

fn mp4(b: &[u8], t: &mut Tags) {
    use super::mp4::{boxes, child};
    let Some(ilst) = child(b, b"moov")
        .and_then(|m| child(m, b"udta"))
        .and_then(|u| child(u, b"meta"))
        // `meta` is a full box: version and flags before its children.
        .and_then(|m| m.get(4..))
        .and_then(|m| child(m, b"ilst"))
    else {
        return;
    };
    for (atom, item) in boxes(ilst) {
        let parts = boxes(item);
        let key = if &atom == b"----" {
            // Freeform: `mean` (reverse-DNS owner), `name`, `data`.
            let Some(name) = parts
                .iter()
                .find(|(k, _)| k == b"name")
                .and_then(|(_, v)| v.get(4..))
            else {
                continue;
            };
            canonical(&String::from_utf8_lossy(name))
        } else if let Some(k) = mp4_key(&atom) {
            k.to_owned()
        } else {
            continue;
        };
        for (_, data) in parts.iter().filter(|(k, _)| k == b"data") {
            // Type indicator (8 bits version + 24 bits type), locale.
            let (Some(kind), Some(v)) = (be32(data, 0), data.get(8..)) else {
                continue;
            };
            let value = match (kind & 0x00FF_FFFF, atom) {
                // trkn/disk: 2 bytes reserved, number, total.
                (_, a) if &a == b"trkn" || &a == b"disk" => {
                    let n = v.get(2..4).map_or(0, |x| u16::from_be_bytes([x[0], x[1]]));
                    let of = v.get(4..6).map_or(0, |x| u16::from_be_bytes([x[0], x[1]]));
                    if n == 0 {
                        continue;
                    }
                    if of == 0 {
                        n.to_string()
                    } else {
                        format!("{n}/{of}")
                    }
                }
                // gnre: an ID3v1 genre number plus one.
                (_, a) if &a == b"gnre" => {
                    let g = v.get(..2).map_or(0, |x| u16::from_be_bytes([x[0], x[1]]));
                    match GENRES.get(usize::from(g).wrapping_sub(1)) {
                        Some(name) => (*name).to_owned(),
                        None => continue,
                    }
                }
                (1, _) | (0, _) => String::from_utf8_lossy(v).into_owned(),
                (2, _) => utf16(v, true),
                _ => continue, // images and binary
            };
            t.merge(&key, &value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_case_insensitive_and_repeatable() {
        let mut t = Tags::new().with("title", "Raga").with("Artist", "A");
        t.push("ARTIST", "B");
        assert_eq!(t.title(), Some("Raga"));
        assert_eq!(t.all("artist").collect::<Vec<_>>(), ["A", "B"]);
        t.set("artist", "C");
        assert_eq!(t.all("ARTIST").collect::<Vec<_>>(), ["C"]);
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn genres_decode_from_numbers() {
        assert_eq!(genre("(17)"), "Rock");
        assert_eq!(genre("17"), "Rock");
        assert_eq!(genre("(17)Rock & Roll"), "Rock & Roll");
        assert_eq!(genre("Carnatic"), "Carnatic");
        assert_eq!(genre("(999)"), "(999)");
    }

    /// A hand-built ID3v2.3 tag: UTF-16 with BOM, whole-tag
    /// unsynchronisation (an `FF` in the text), a comment, a TXXX.
    #[test]
    fn id3v23_with_unsynchronisation() {
        let mut frames = Vec::new();
        let mut frame = |id: &[u8; 4], data: &[u8]| {
            frames.extend_from_slice(id);
            frames.extend_from_slice(&u32::try_from(data.len()).unwrap().to_be_bytes());
            frames.extend_from_slice(&[0, 0]);
            frames.extend_from_slice(data);
        };
        // "ÿes" in UTF-16LE: FF 00 65 00 73 00 — contains FF 00.
        frame(
            b"TIT2",
            &[1, 0xFF, 0xFE, 0xFF, 0x00, 0x65, 0x00, 0x73, 0x00],
        );
        frame(b"TPE1", b"\x00Latin");
        frame(b"COMM", b"\x03engdesc\x00a comment");
        frame(b"TXXX", b"\x03RAGA\x00Kalyani");
        frame(b"TCON", b"\x00(17)");
        // Apply unsynchronisation: FF followed by 00 or ≥ E0 gets a 00.
        let mut synced = Vec::new();
        for (i, &c) in frames.iter().enumerate() {
            synced.push(c);
            if c == 0xFF && frames.get(i + 1).is_some_and(|&n| n == 0 || n >= 0xE0) {
                synced.push(0);
            }
        }
        let n = synced.len();
        let size = [
            (n >> 21) as u8 & 0x7F,
            (n >> 14) as u8 & 0x7F,
            (n >> 7) as u8 & 0x7F,
            n as u8 & 0x7F,
        ];
        let mut tag = b"ID3\x03\x00\x80".to_vec();
        tag.extend_from_slice(&size);
        tag.extend_from_slice(&synced);
        tag.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x00]); // an MPEG frame header
        let t = read(&tag);
        assert_eq!(t.title(), Some("ÿes"));
        assert_eq!(t.artist(), Some("Latin"));
        assert_eq!(t.comment(), Some("a comment"));
        assert_eq!(t.get("RAGA"), Some("Kalyani"));
        assert_eq!(t.genre(), Some("Rock"));
    }

    #[test]
    fn id3v1_1_at_the_end() {
        let mut f = vec![0xFF, 0xFB, 0x90, 0x00];
        f.resize(1_000, 0);
        let mut tag = [0u8; 128];
        tag[..3].copy_from_slice(b"TAG");
        tag[3..8].copy_from_slice(b"Title");
        tag[33..39].copy_from_slice(b"Artist");
        tag[93..97].copy_from_slice(b"1999");
        tag[97..104].copy_from_slice(b"Comment");
        tag[126] = 7;
        tag[127] = 32;
        f.extend_from_slice(&tag);
        let t = read(&f);
        assert_eq!(t.title(), Some("Title"));
        assert_eq!(t.date(), Some("1999"));
        assert_eq!(t.track(), Some("7"));
        assert_eq!(t.genre(), Some("Classical"));
        assert_eq!(t.comment(), Some("Comment"));
    }

    #[test]
    fn wav_tags_round_trip_and_replace() {
        let wav = crate::write_wav(&crate::Samples::mono(vec![0.1; 64], 48_000)).unwrap();
        let tags = Tags::new()
            .with("TITLE", "Vatapi")
            .with("ARTIST", "Ensemble")
            .with("DATE", "2026")
            .with("TRACKNUMBER", "3")
            .with("RAGA", "Hamsadhwani"); // no INFO field: left out
        let tagged = write_wav(&wav, &tags).unwrap();
        let back = read(&tagged);
        assert_eq!(back.title(), Some("Vatapi"));
        assert_eq!(back.track(), Some("3"));
        assert_eq!(back.get("RAGA"), None);
        // The audio is unchanged.
        assert_eq!(
            crate::read_wav(&tagged).unwrap().samples,
            crate::read_wav(&wav).unwrap().samples
        );
        // Re-tagging replaces, not appends.
        let again = write_wav(&tagged, &Tags::new().with("TITLE", "Other")).unwrap();
        let t = read(&again);
        assert_eq!(t.all("TITLE").collect::<Vec<_>>(), ["Other"]);
        assert_eq!(t.artist(), None);
        assert_eq!(
            u32::from_le_bytes(again[4..8].try_into().unwrap()) as usize,
            again.len() - 8
        );
    }

    #[test]
    fn flac_tags_round_trip_and_replace() {
        let d = super::super::Decoded {
            data: vec![0.25; 4_096],
            channels: 1,
            rate: 44_100,
        };
        let flac = super::super::flac::encode(&d, 16).unwrap();
        let tags = Tags::new()
            .with("TITLE", "Korvai")
            .with("RAGA", "Kalyani")
            .with("TALA", "Adi")
            .with("COMMENT", "ünïcödé");
        let tagged = write_flac(&flac, &tags).unwrap();
        let back = read(&tagged);
        assert_eq!(back, tags);
        assert_eq!(
            super::super::decode(&tagged).unwrap(),
            super::super::decode(&flac).unwrap()
        );
        let again = write_flac(&tagged, &Tags::new().with("TITLE", "x")).unwrap();
        assert_eq!(read(&again).len(), 1);
        assert_eq!(
            super::super::decode(&again).unwrap(),
            super::super::decode(&flac).unwrap()
        );
    }

    #[test]
    fn aiff_tags_round_trip() {
        let d = super::super::Decoded {
            data: vec![0.25; 512],
            channels: 2,
            rate: 48_000,
        };
        let aiff = super::super::aiff::encode(&d, 16).unwrap();
        let tagged = write_aiff(
            &aiff,
            &Tags::new().with("TITLE", "Odd").with("ARTIST", "Even"),
        )
        .unwrap();
        let t = read(&tagged);
        assert_eq!((t.title(), t.artist()), (Some("Odd"), Some("Even")));
        assert_eq!(
            super::super::decode(&tagged).unwrap(),
            super::super::decode(&aiff).unwrap()
        );
    }

    #[test]
    fn hostile_tags_never_panic() {
        let mut rng = crate::rng::Rng::new(0x7A65);
        let seeds: [&[u8]; 5] = [
            b"ID3\x04\x00\x40\x00\x00\x00\x7F",
            b"fLaC\x04\xFF\xFF\xFF",
            b"RIFF\xFF\xFF\xFF\xFFWAVELIST\xFF\xFF\xFF\x7FINFO",
            b"FORM\x00\x00\x00\x00AIFFNAME\xFF\xFF\xFF\xFF",
            b"\x00\x00\x00\x08ftypM4A \xFF\xFF\xFF\xFFmoov",
        ];
        for seed in seeds {
            for _ in 0..2_000 {
                let mut b = seed.to_vec();
                let extra = ((rng.next_u64() as u32) % 64) as usize;
                for _ in 0..extra {
                    b.push((rng.next_u64() as u32) as u8);
                }
                let i = ((rng.next_u64() as u32) as usize) % b.len();
                b[i] ^= (rng.next_u64() as u32) as u8;
                let _ = read(&b);
                let _ = write_wav(&b, &Tags::new().with("TITLE", "t"));
                let _ = write_flac(&b, &Tags::new().with("TITLE", "t"));
                let _ = write_aiff(&b, &Tags::new().with("TITLE", "t"));
            }
        }
    }
}
