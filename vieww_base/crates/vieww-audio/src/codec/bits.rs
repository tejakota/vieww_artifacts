//! Bit readers and writers shared by the codecs.
//!
//! FLAC, MP3 and AAC pack fields most-significant-bit first ([`MsbReader`],
//! [`MsbWriter`]); Vorbis packs them least-significant-bit first
//! ([`LsbReader`]). Every read is bounds-checked and reports running off the
//! end as `None` — a truncated or hostile file must fail, never panic.

/// MSB-first bit reader.
#[derive(Debug, Clone)]
pub struct MsbReader<'a> {
    data: &'a [u8],
    pos: usize, // bit position
}

impl<'a> MsbReader<'a> {
    /// A reader at bit 0.
    #[must_use]
    pub const fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Current bit position.
    #[must_use]
    pub const fn position(&self) -> usize {
        self.pos
    }

    /// Bits left.
    #[must_use]
    pub const fn remaining(&self) -> usize {
        (self.data.len() * 8).saturating_sub(self.pos)
    }

    /// Seek to a bit position.
    pub const fn seek(&mut self, bit: usize) {
        self.pos = bit;
    }

    /// Skip to the next byte boundary.
    pub const fn align(&mut self) {
        self.pos = self.pos.div_ceil(8) * 8;
    }

    /// Read `n` ≤ 32 bits.
    pub fn read(&mut self, n: u32) -> Option<u32> {
        if n == 0 {
            return Some(0);
        }
        if n > 32 || self.remaining() < n as usize {
            return None;
        }
        let mut v: u64 = 0;
        let mut need = n;
        while need > 0 {
            let byte = self.data[self.pos / 8];
            let bit_off = (self.pos % 8) as u32;
            let avail = 8 - bit_off;
            let take = avail.min(need);
            let shift = avail - take;
            let bits = (u32::from(byte) >> shift) & ((1 << take) - 1);
            v = (v << take) | u64::from(bits);
            need -= take;
            self.pos += take as usize;
        }
        u32::try_from(v).ok()
    }

    /// Read one bit as a bool.
    pub fn bit(&mut self) -> Option<bool> {
        self.read(1).map(|b| b == 1)
    }

    /// Read `n` bits as a two's-complement signed value.
    pub fn read_signed(&mut self, n: u32) -> Option<i32> {
        let v = self.read(n)?;
        if n == 0 {
            return Some(0);
        }
        let shift = 32 - n;
        Some((v.cast_signed() << shift) >> shift)
    }

    /// Peek `n` bits without consuming.
    pub fn peek(&self, n: u32) -> Option<u32> {
        let mut c = self.clone();
        c.read(n)
    }

    /// Count zero bits up to the next one (consumed), FLAC's unary code.
    pub fn unary(&mut self) -> Option<u32> {
        let mut n = 0;
        while !self.bit()? {
            n += 1;
        }
        Some(n)
    }

    /// Skip `n` bits.
    pub fn skip(&mut self, n: usize) -> Option<()> {
        if self.remaining() < n {
            return None;
        }
        self.pos += n;
        Some(())
    }
}

/// LSB-first bit reader (Vorbis).
#[derive(Debug, Clone)]
pub struct LsbReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> LsbReader<'a> {
    /// A reader at bit 0.
    #[must_use]
    pub const fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Bits left.
    #[must_use]
    pub const fn remaining(&self) -> usize {
        (self.data.len() * 8).saturating_sub(self.pos)
    }

    /// Read `n` ≤ 32 bits, least significant first.
    pub fn read(&mut self, n: u32) -> Option<u32> {
        if n == 0 {
            return Some(0);
        }
        if n > 32 || self.remaining() < n as usize {
            return None;
        }
        let mut v: u64 = 0;
        let mut got = 0;
        while got < n {
            let byte = self.data[self.pos / 8];
            let bit_off = (self.pos % 8) as u32;
            let take = (8 - bit_off).min(n - got);
            let bits = (u32::from(byte) >> bit_off) & ((1 << take) - 1);
            v |= u64::from(bits) << got;
            got += take;
            self.pos += take as usize;
        }
        u32::try_from(v).ok()
    }

    /// One bit.
    pub fn bit(&mut self) -> Option<bool> {
        self.read(1).map(|b| b == 1)
    }
}

/// MSB-first bit writer.
#[derive(Debug, Clone, Default)]
pub struct MsbWriter {
    /// The bytes written so far (the last may be partial).
    pub bytes: Vec<u8>,
    bits: u32,
}

impl MsbWriter {
    /// Write the low `n` bits of `v`.
    pub fn write(&mut self, v: u64, n: u32) {
        for i in (0..n).rev() {
            let bit = (v >> i) & 1;
            if self.bits.is_multiple_of(8) {
                self.bytes.push(0);
            }
            let last = self.bytes.len() - 1;
            self.bytes[last] |= u8::from(bit == 1) << (7 - self.bits % 8);
            self.bits += 1;
        }
    }

    /// Write a signed value in `n` bits (two's complement).
    pub fn write_signed(&mut self, v: i64, n: u32) {
        let mask = if n >= 64 { u64::MAX } else { (1u64 << n) - 1 };
        self.write(v.cast_unsigned() & mask, n);
    }

    /// Unary: `n` zeros then a one.
    pub fn unary(&mut self, n: u32) {
        for _ in 0..n {
            self.write(0, 1);
        }
        self.write(1, 1);
    }

    /// Pad with zeros to a byte boundary.
    pub fn align(&mut self) {
        while !self.bits.is_multiple_of(8) {
            self.write(0, 1);
        }
    }

    /// Bits written.
    #[must_use]
    pub const fn len_bits(&self) -> u32 {
        self.bits
    }
}

/// CRC-8 (poly 0x07) — FLAC frame headers.
#[must_use]
pub fn crc8(data: &[u8]) -> u8 {
    let mut c = 0u8;
    for b in data {
        c ^= b;
        for _ in 0..8 {
            c = if c & 0x80 != 0 {
                (c << 1) ^ 0x07
            } else {
                c << 1
            };
        }
    }
    c
}

/// CRC-16 (poly 0x8005, MSB-first, init 0) — FLAC frames.
#[must_use]
pub fn crc16(data: &[u8]) -> u16 {
    let mut c = 0u16;
    for b in data {
        c ^= u16::from(*b) << 8;
        for _ in 0..8 {
            c = if c & 0x8000 != 0 {
                (c << 1) ^ 0x8005
            } else {
                c << 1
            };
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msb_round_trip() {
        let mut w = MsbWriter::default();
        w.write(0b101, 3);
        w.write_signed(-5, 7);
        w.unary(4);
        w.write(0xABCD, 16);
        w.align();
        let mut r = MsbReader::new(&w.bytes);
        assert_eq!(r.read(3), Some(0b101));
        assert_eq!(r.read_signed(7), Some(-5));
        assert_eq!(r.unary(), Some(4));
        assert_eq!(r.read(16), Some(0xABCD));
    }

    #[test]
    fn lsb_order() {
        let d = [0b1010_0110u8, 0b0000_0001];
        let mut r = LsbReader::new(&d);
        assert_eq!(r.read(3), Some(0b110));
        assert_eq!(r.read(6), Some(0b1_10100));
    }

    #[test]
    fn running_off_the_end_is_none() {
        let mut r = MsbReader::new(&[0xFF]);
        assert_eq!(r.read(9), None);
        assert_eq!(r.read(8), Some(0xFF));
        assert_eq!(r.bit(), None);
    }

    #[test]
    fn crcs_match_known_values() {
        assert_eq!(crc8(b"123456789"), 0xF4);
        assert_eq!(crc16(b"123456789"), 0xFEE8);
    }
}
