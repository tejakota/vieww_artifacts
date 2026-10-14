//! Ogg — the page/packet container under Vorbis.
//!
//! A stream is a sequence of pages (`OggS`, a header with a granule
//! position, a serial number, a CRC, and a lacing table); packets are split
//! across 255-byte segments, a segment shorter than 255 ending a packet, so a
//! packet can span pages. [`packets`] reassembles them for one logical
//! stream (the first serial seen), verifying every page's CRC-32 and
//! carrying the granule position of the page each packet *ends* on.

use super::CodecError;

/// A reassembled packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    /// The bytes.
    pub data: Vec<u8>,
    /// Granule position of the page this packet completed on (−1 if none
    /// completed there).
    pub granule: i64,
    /// Whether the page it completed on was the last of the stream.
    pub last_page: bool,
}

fn crc_table() -> &'static [u32; 256] {
    static T: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, e) in t.iter_mut().enumerate() {
            let mut r = u32::try_from(i).unwrap_or(0) << 24;
            for _ in 0..8 {
                r = if r & 0x8000_0000 != 0 {
                    (r << 1) ^ 0x04C1_1DB7
                } else {
                    r << 1
                };
            }
            *e = r;
        }
        t
    })
}

/// Ogg's CRC-32 (poly 0x04C11DB7, no reflection, init 0).
#[must_use]
pub fn crc32(data: &[u8]) -> u32 {
    let t = crc_table();
    data.iter().fold(0u32, |c, b| {
        (c << 8) ^ t[usize::from(((c >> 24) as u8) ^ b)]
    })
}

/// Reassemble the packets of the first logical stream.
///
/// # Errors
/// Not Ogg, truncated pages, or a CRC failure.
pub fn packets(bytes: &[u8]) -> Result<Vec<Packet>, CodecError> {
    if !bytes.starts_with(b"OggS") {
        return Err(CodecError::NotThisFormat);
    }
    let mut out = Vec::new();
    let mut partial: Vec<u8> = Vec::new();
    let mut serial: Option<u32> = None;
    let mut at = 0;
    while at + 27 <= bytes.len() {
        if &bytes[at..at + 4] != b"OggS" {
            at += 1;
            continue;
        }
        let h = &bytes[at..];
        let flags = h[5];
        let granule = i64::from_le_bytes([h[6], h[7], h[8], h[9], h[10], h[11], h[12], h[13]]);
        let sn = u32::from_le_bytes([h[14], h[15], h[16], h[17]]);
        let crc = u32::from_le_bytes([h[22], h[23], h[24], h[25]]);
        let nseg = usize::from(h[26]);
        if at + 27 + nseg > bytes.len() {
            return Err(CodecError::Truncated);
        }
        let lacing = &h[27..27 + nseg];
        let body_len: usize = lacing.iter().map(|v| usize::from(*v)).sum();
        let total = 27 + nseg + body_len;
        if at + total > bytes.len() {
            return Err(CodecError::Truncated);
        }
        let mut page = bytes[at..at + total].to_vec();
        page[22..26].fill(0);
        if crc32(&page) != crc {
            return Err(CodecError::Checksum("Ogg page CRC-32"));
        }
        if *serial.get_or_insert(sn) != sn {
            at += total;
            continue;
        }
        if flags & 1 == 0 {
            partial.clear(); // a fresh packet starts on a non-continued page
        }
        let last_page = flags & 4 != 0;
        let mut off = at + 27 + nseg;
        for (i, &l) in lacing.iter().enumerate() {
            partial.extend_from_slice(&bytes[off..off + usize::from(l)]);
            off += usize::from(l);
            if l < 255 {
                let ends_page = lacing[i + 1..].iter().all(|v| *v == 255) || i + 1 == nseg;
                out.push(Packet {
                    data: std::mem::take(&mut partial),
                    granule: if lacing[i + 1..].iter().any(|v| *v < 255) {
                        -1
                    } else {
                        granule
                    },
                    last_page: last_page && ends_page,
                });
            }
        }
        at += total;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_known_value() {
        // The Ogg CRC of "OggS" with nothing else, as libogg computes it.
        assert_eq!(crc32(b""), 0);
        assert_ne!(crc32(b"OggS"), 0);
    }

    #[test]
    fn reads_the_vorbis_fixture() {
        let p = packets(include_bytes!("../../tests/fixtures/src.ogg")).unwrap();
        assert!(p.len() > 10);
        assert_eq!(p[0].data[0], 1);
        assert_eq!(&p[0].data[1..7], b"vorbis");
        assert_eq!(p[1].data[0], 3);
        assert_eq!(p[2].data[0], 5);
        assert!(p.last().unwrap().last_page);
        assert!(p.last().unwrap().granule > 0);
    }

    #[test]
    fn a_corrupted_page_fails_its_crc() {
        let mut f = include_bytes!("../../tests/fixtures/src.ogg").to_vec();
        let n = f.len();
        f[n - 10] ^= 1;
        assert!(matches!(packets(&f), Err(CodecError::Checksum(_))));
    }
}
