//! MD5 (RFC 1321) — needed because FLAC's STREAMINFO carries the MD5 of
//! the decoded audio, which makes every FLAC decode self-verifying.
//! Not for anything cryptographic.

const S: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9,
    14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15,
    21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

fn k(i: usize) -> u32 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "RFC 1321's definition of K"
    )]
    let v = ((i as f64 + 1.0).sin().abs() * 4_294_967_296.0) as u32;
    v
}

/// An incremental MD5.
#[derive(Debug, Clone)]
pub struct Md5 {
    state: [u32; 4],
    buf: Vec<u8>,
    len: u64,
    table: [u32; 64],
}

impl Default for Md5 {
    fn default() -> Self {
        Self {
            state: [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476],
            buf: Vec::with_capacity(64),
            len: 0,
            table: std::array::from_fn(k),
        }
    }
}

impl Md5 {
    /// Feed bytes.
    pub fn update(&mut self, data: &[u8]) {
        self.len += data.len() as u64;
        for &b in data {
            self.buf.push(b);
            if self.buf.len() == 64 {
                let block: [u8; 64] = self.buf[..].try_into().unwrap_or([0; 64]);
                self.block(&block);
                self.buf.clear();
            }
        }
    }

    fn block(&mut self, b: &[u8; 64]) {
        let m: [u32; 16] = std::array::from_fn(|i| {
            u32::from_le_bytes([b[4 * i], b[4 * i + 1], b[4 * i + 2], b[4 * i + 3]])
        });
        let [mut a, mut bb, mut c, mut d] = self.state;
        for (i, &s) in S.iter().enumerate() {
            let (f, g) = match i / 16 {
                0 => ((bb & c) | (!bb & d), i),
                1 => ((d & bb) | (!d & c), (5 * i + 1) % 16),
                2 => (bb ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (bb | !d), (7 * i) % 16),
            };
            let t = d;
            d = c;
            c = bb;
            bb = bb.wrapping_add(
                a.wrapping_add(f)
                    .wrapping_add(self.table[i])
                    .wrapping_add(m[g])
                    .rotate_left(s),
            );
            a = t;
        }
        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(bb);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
    }

    /// The digest.
    #[must_use]
    pub fn finish(mut self) -> [u8; 16] {
        let bits = self.len.wrapping_mul(8);
        let mut pad = vec![0x80u8];
        while (self.buf.len() + pad.len()) % 64 != 56 {
            pad.push(0);
        }
        pad.extend(bits.to_le_bytes());
        let saved = self.len;
        self.update(&pad);
        self.len = saved;
        let mut out = [0u8; 16];
        for (i, w) in self.state.iter().enumerate() {
            out[4 * i..4 * i + 4].copy_from_slice(&w.to_le_bytes());
        }
        out
    }
}

/// One-shot MD5.
#[must_use]
pub fn md5(data: &[u8]) -> [u8; 16] {
    let mut m = Md5::default();
    m.update(data);
    m.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(d: [u8; 16]) -> String {
        d.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn rfc_1321_test_suite() {
        assert_eq!(hex(md5(b"")), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(hex(md5(b"abc")), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            hex(md5(b"message digest")),
            "f96b697d7cb7938d525a2f31aaf161d0"
        );
        assert_eq!(
            hex(md5(
                b"12345678901234567890123456789012345678901234567890123456789012345678901234567890"
            )),
            "57edf4a22be3c955ac49da2e2107b67a"
        );
    }

    #[test]
    fn incremental_equals_one_shot() {
        let data: Vec<u8> = (0..1000u32).map(|i| (i * 7 % 251) as u8).collect();
        let mut m = Md5::default();
        for c in data.chunks(37) {
            m.update(c);
        }
        assert_eq!(m.finish(), md5(&data));
    }
}
