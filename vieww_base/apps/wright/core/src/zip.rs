//! A zip writer: enough of PKWARE's APPNOTE for archives every tool opens
//! (Explorer, Finder, `unzip`, Python's `zipfile`, pip).
//!
//! ```text
//!  [local header + bytes] x N   [central directory entry] x N   [end record]
//! ```
//!
//! Files are *stored* (method 0, no compression): a wheel or a project
//! export is mostly text that the download already compresses, and a
//! deflate encoder is a lot of code for a few kilobytes. Every timestamp is
//! 1980-01-01, so the same files always make the same bytes — an export is
//! reproducible, and its hash means something.
//!
//! Limits: no ZIP64, so under 4 GiB and 65,535 entries; refused, not
//! truncated, past either.

/// CRC-32 (IEEE 802.3, reflected, polynomial 0xEDB88320), as zip requires.
pub fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, slot) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 == 1 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *slot = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc = table[((crc ^ u32::from(b)) & 0xFF) as usize] ^ (crc >> 8);
    }
    !crc
}

/// Builds a zip in memory, in the order entries are added.
#[derive(Default)]
pub struct ZipWriter {
    out: Vec<u8>,
    central: Vec<u8>,
    count: u16,
}

/// DOS date for 1980-01-01, time 00:00 — the earliest a zip can say.
const DOS_DATE: u16 = (1 << 5) | 1;
const DOS_TIME: u16 = 0;

impl ZipWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a file at `name` (forward slashes; no leading slash, no `..`).
    pub fn add(&mut self, name: &str, data: &[u8]) -> Result<(), String> {
        if name.is_empty() || name.starts_with('/') || name.split('/').any(|p| p == ".." || p.is_empty()) || name.contains('\\') {
            return Err(format!("not a safe archive path: {name:?}"));
        }
        if self.count == u16::MAX {
            return Err("too many files for a zip without ZIP64 (65,535)".into());
        }
        let offset = u32::try_from(self.out.len()).map_err(|_| "archive larger than 4 GiB".to_string())?;
        let size = u32::try_from(data.len()).map_err(|_| format!("{name} is larger than 4 GiB"))?;
        let crc = crc32(data);
        let name_bytes = name.as_bytes();
        let name_len = u16::try_from(name_bytes.len()).map_err(|_| format!("path too long: {name}"))?;
        // UTF-8 names (general-purpose flag bit 11), stored, version 2.0.
        const FLAGS: u16 = 1 << 11;
        let local = [
            &0x0403_4b50u32.to_le_bytes()[..],
            &20u16.to_le_bytes(),
            &FLAGS.to_le_bytes(),
            &0u16.to_le_bytes(), // method: stored
            &DOS_TIME.to_le_bytes(),
            &DOS_DATE.to_le_bytes(),
            &crc.to_le_bytes(),
            &size.to_le_bytes(),
            &size.to_le_bytes(),
            &name_len.to_le_bytes(),
            &0u16.to_le_bytes(), // extra length
        ]
        .concat();
        self.out.extend_from_slice(&local);
        self.out.extend_from_slice(name_bytes);
        self.out.extend_from_slice(data);
        let central = [
            &0x0201_4b50u32.to_le_bytes()[..],
            &(3u16 << 8 | 20).to_le_bytes(), // made by: Unix, 2.0 (keeps the mode below meaningful)
            &20u16.to_le_bytes(),
            &FLAGS.to_le_bytes(),
            &0u16.to_le_bytes(),
            &DOS_TIME.to_le_bytes(),
            &DOS_DATE.to_le_bytes(),
            &crc.to_le_bytes(),
            &size.to_le_bytes(),
            &size.to_le_bytes(),
            &name_len.to_le_bytes(),
            &0u16.to_le_bytes(), // extra
            &0u16.to_le_bytes(), // comment
            &0u16.to_le_bytes(), // disk
            &0u16.to_le_bytes(), // internal attributes
            &(0o100_644u32 << 16).to_le_bytes(), // external: a regular file, rw-r--r--
            &offset.to_le_bytes(),
        ]
        .concat();
        self.central.extend_from_slice(&central);
        self.central.extend_from_slice(name_bytes);
        self.count += 1;
        Ok(())
    }

    /// The finished archive.
    pub fn finish(mut self) -> Result<Vec<u8>, String> {
        let dir_offset = u32::try_from(self.out.len()).map_err(|_| "archive larger than 4 GiB".to_string())?;
        let dir_size = u32::try_from(self.central.len()).map_err(|_| "directory larger than 4 GiB".to_string())?;
        self.out.extend_from_slice(&self.central);
        let end = [
            &0x0605_4b50u32.to_le_bytes()[..],
            &0u16.to_le_bytes(),
            &0u16.to_le_bytes(),
            &self.count.to_le_bytes(),
            &self.count.to_le_bytes(),
            &dir_size.to_le_bytes(),
            &dir_offset.to_le_bytes(),
            &0u16.to_le_bytes(),
        ]
        .concat();
        self.out.extend_from_slice(&end);
        Ok(self.out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_check_value() {
        // The standard check value for CRC-32/ISO-HDLC.
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn the_same_files_make_the_same_bytes() {
        let make = || {
            let mut z = ZipWriter::new();
            z.add("a/hello.txt", b"hello").unwrap();
            z.add("a/b/empty", b"").unwrap();
            z.finish().unwrap()
        };
        let a = make();
        assert_eq!(a, make());
        assert_eq!(&a[0..4], b"PK\x03\x04");
        assert_eq!(&a[a.len() - 22..a.len() - 18], b"PK\x05\x06", "ends with the end-of-directory record");
    }

    #[test]
    fn unsafe_paths_are_refused() {
        let mut z = ZipWriter::new();
        for bad in ["", "/etc/passwd", "../x", "a/../../x", "a//b", "a\\b"] {
            assert!(z.add(bad, b"").is_err(), "{bad:?}");
        }
    }
}
