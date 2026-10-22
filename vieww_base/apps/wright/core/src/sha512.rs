//! SHA-512, from FIPS 180-4, in safe Rust.
//!
//! Why it is here: Ed25519 (RFC 8032) hashes with SHA-512, and Wright
//! signs receipts with Ed25519 ([`crate::ed25519`]). We do not pull a
//! crate for it: this port is small, branch-free on secrets, and pinned
//! byte-for-byte to the NIST test vectors below. That is the ADR-012
//! crypto boundary — reference ports live in `wright-core` only when
//! they carry the standard's own vectors.
//!
//! The round constants are not typed in by hand (transcription from
//! memory is exactly how hash ports die). They are *computed*, because
//! FIPS 180-4 §4.2.2 defines them: `K[i]` is the first 64 bits of the
//! fractional part of the cube root of the i-th prime, and the IV is the
//! same recipe with square roots. Integer arithmetic reproduces them
//! exactly; the vectors below prove it did.

use std::sync::OnceLock;

/// The first `n` primes, by trial division. Small and boring on purpose.
fn primes(n: usize) -> Vec<u64> {
    let mut out = Vec::with_capacity(n);
    let mut c = 2u64;
    while out.len() < n {
        if (2..)
            .take_while(|d| d * d <= c)
            .all(|d| !c.is_multiple_of(d))
        {
            out.push(c);
        }
        c += 1;
    }
    out
}

/// `c^2` for a 128-bit `c`, as four little-endian limbs. Same schoolbook
/// shape as [`cube128`]'s first half: every product fits a u128.
fn square128(c: u128) -> [u64; 4] {
    let b = c as u64;
    let a = (c >> 64) as u64; // a few bits at most, for our searches
    let b2 = (b as u128) * (b as u128);
    let ab = (a as u128) * (b as u128) * 2;
    let a2 = (a as u128) * (a as u128);
    let mut out = [0u64; 4];
    out[0] = b2 as u64;
    let acc = (out[1] as u128) + (b2 >> 64) + (ab & (u64::MAX as u128));
    out[1] = acc as u64;
    let acc = (acc >> 64) + (ab >> 64) + a2;
    out[2] = acc as u64;
    out[3] = (acc >> 64) as u64; // zero by construction
    out
}

/// `floor(2^64 * frac(sqrt(p)))` — the IV recipe: the first 64 bits of
/// the FRACTIONAL part of the square root (FIPS 180-4 §4.2.3). The
/// scaled root `floor(sqrt(p) * 2^64)` can exceed 64 bits, so the search
/// runs in u128 between the integer part's power and the next one —
/// the same shape as [`frac_cbrt64`].
fn frac_sqrt64(p: u64) -> u64 {
    let whole = integer_sqrt(p as u128); // the integer part, small
    let target = [0u64, 0, p, 0]; // p * 2^128, little-endian
    let mut lo = whole << 64;
    let mut hi = (whole + 1) << 64; // inclusive; too-big candidates fail
    while lo < hi {
        let mid = lo + (hi - lo + 1).div_ceil(2);
        if limbs_le(&square128(mid), &target) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    (lo - (whole << 64)) as u64
}

/// Integer cube root of a small p (<= ~409): the largest k with k^3 <= p.
fn icbrt_small(p: u64) -> u64 {
    let mut k = 0u64;
    while (k + 1).pow(3) <= p {
        k += 1;
    }
    k
}

/// `c^3` for a 128-bit `c`, as four little-endian u64 limbs (c < 2^67
/// here, so c^3 < 2^201 fits comfortably). Schoolbook in base 2^64,
/// every product a u128 — no u64 overflow anywhere.
fn cube128(c: u128) -> [u64; 4] {
    let lo = c as u64;
    let hi = (c >> 64) as u64; // at most a few bits for our primes
                               // c^2 as three limbs: lo^2 + 2·hi·lo·2^64 + hi^2·2^128.
    let lo2 = (lo as u128) * (lo as u128);
    let hilo = (hi as u128) * (lo as u128) * 2;
    let hi2 = (hi as u128) * (hi as u128);
    let mut r = [0u64; 3];
    r[0] = lo2 as u64;
    r[1] = (lo2 >> 64) as u64;
    let t = (r[1] as u128) + (hilo & u64::MAX as u128);
    r[1] = t as u64;
    r[2] = ((t >> 64) + (hilo >> 64) + hi2) as u64;
    debug_assert!(((t >> 64) + (hilo >> 64) + hi2) >> 64 == 0);
    // c^3 = c^2 · c = r·lo + (r·hi)·2^64.
    let mut out = [0u64; 4];
    let mut carry = 0u128;
    for i in 0..3 {
        let prod = (r[i] as u128) * (lo as u128) + carry;
        out[i] = prod as u64;
        carry = prod >> 64;
    }
    out[3] = carry as u64;
    let mut carry = 0u128;
    for i in 0..3 {
        let prod = (r[i] as u128) * (hi as u128) + carry + (out[i + 1] as u128);
        out[i + 1] = prod as u64;
        carry = prod >> 64;
    }
    debug_assert!(carry == 0, "c^3 must fit in four limbs");
    out
}

/// Little-endian limb compare: a <= b.
fn limbs_le(a: &[u64; 4], b: &[u64; 4]) -> bool {
    for i in (0..4).rev() {
        if a[i] != b[i] {
            return a[i] < b[i];
        }
    }
    true
}

/// `floor(2^64 * frac(cbrt(p)))` — the K recipe: the first 64 bits of the
/// FRACTIONAL part of the cube root (FIPS 180-4 §4.2.2). The true scaled
/// root `floor(cbrt(p) * 2^64)` is a 65+ bit number, so the search runs in
/// u128 between the integer part's power and the next one, and only the
/// fractional bits are returned.
fn frac_cbrt64(p: u64) -> u64 {
    let k = icbrt_small(p) as u128; // integer part of cbrt(p)
    let target = [0u64, 0, 0, p]; // p * 2^192, little-endian
    let mut lo = k << 64; // cbrt(p) >= k exactly
    let mut hi = (k + 1) << 64; // inclusive upper bound; a too-big
                                // candidate simply fails the cube test, perfect cubes included.
    while lo < hi {
        let mid = lo + (hi - lo + 1).div_ceil(2);
        if limbs_le(&cube128(mid), &target) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    (lo - (k << 64)) as u64
}

/// `isqrt` for u128 without the (1.84+) std method, so the code builds
/// on older toolchains too: Newton's method, converges from above.
///
/// The seed is `(x + 1) / 2` — the FLOOR of the half-plus-one, on
/// purpose. `div_ceil` here changes the answer: for n = 2 the seed
/// becomes 2, the loop never runs, and `integer_sqrt(2)` returns 2
/// instead of 1, which poisons every constant derived from
/// `frac_sqrt64` (the IV). Newton's floor is the correct start.
#[allow(clippy::manual_div_ceil)]
fn integer_sqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// The 80 FIPS round constants, computed once and cached.
fn k_table() -> &'static [u64; 80] {
    static K: OnceLock<[u64; 80]> = OnceLock::new();
    K.get_or_init(|| {
        let mut k = [0u64; 80];
        for (i, p) in primes(80).into_iter().enumerate() {
            k[i] = frac_cbrt64(p);
        }
        k
    })
}

/// The SHA-512 IV: fractional square roots of the first 8 primes.
fn iv() -> [u64; 8] {
    static IV: OnceLock<[u64; 8]> = OnceLock::new();
    *IV.get_or_init(|| {
        let mut iv = [0u64; 8];
        for (i, p) in primes(8).into_iter().enumerate() {
            iv[i] = frac_sqrt64(p);
        }
        iv
    })
}

/// A SHA-512 hasher. Clone mid-stream if you need a Merkle tree.
#[derive(Clone)]
pub struct Sha512 {
    state: [u64; 8],
    buf: [u8; 128],
    filled: usize,
    total: u64,
}

impl Default for Sha512 {
    fn default() -> Self {
        Sha512::new()
    }
}

impl Sha512 {
    pub fn new() -> Sha512 {
        Sha512 {
            state: iv(),
            buf: [0; 128],
            filled: 0,
            total: 0,
        }
    }

    fn compress(&mut self, block: &[u8]) {
        let k = k_table();
        let mut w = [0u64; 80];
        for (i, chunk) in block.as_chunks::<8>().0.iter().enumerate() {
            w[i] = u64::from_be_bytes(*chunk);
        }
        for i in 16..80 {
            let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
            let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for i in 0..80 {
            let big_e = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
            let ch = g ^ (e & (f ^ g));
            let t1 = h
                .wrapping_add(big_e)
                .wrapping_add(ch)
                .wrapping_add(k[i])
                .wrapping_add(w[i]);
            let big_a = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = big_a.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        let s = &mut self.state;
        s[0] = s[0].wrapping_add(a);
        s[1] = s[1].wrapping_add(b);
        s[2] = s[2].wrapping_add(c);
        s[3] = s[3].wrapping_add(d);
        s[4] = s[4].wrapping_add(e);
        s[5] = s[5].wrapping_add(f);
        s[6] = s[6].wrapping_add(g);
        s[7] = s[7].wrapping_add(h);
    }

    /// Feed more bytes.
    pub fn update(&mut self, mut data: &[u8]) {
        self.total = self.total.wrapping_add(data.len() as u64);
        if self.filled > 0 {
            let take = (128 - self.filled).min(data.len());
            self.buf[self.filled..self.filled + take].copy_from_slice(&data[..take]);
            self.filled += take;
            data = &data[take..];
            if self.filled == 128 {
                let block = self.buf;
                self.compress(&block);
                self.filled = 0;
            }
        }
        while data.len() >= 128 {
            let (block, rest) = data.split_at(128);
            self.compress(block);
            data = rest;
        }
        if !data.is_empty() {
            self.buf[..data.len()].copy_from_slice(data);
            self.filled = data.len();
        }
    }

    /// Pad (0x80, zeros to 112 mod 128, the 128-BIT big-endian bit length),
    /// compress the last block, and return the 64-byte digest. The length
    /// field is 128 bits, most-significant first: bytes 112..120 carry its
    /// high half (zero for any message this side of zettabytes), bytes
    /// 120..128 the low half. Getting that order backwards still hashes
    /// the empty message correctly — the FIPS vectors below exist so it
    /// cannot survive anything longer.
    pub fn finalize(mut self) -> Vec<u8> {
        let bit_len = self.total.wrapping_mul(8); // counted BEFORE padding
        self.update(&[0x80]);
        while self.filled != 112 {
            self.update(&[0]);
        }
        self.buf[112..120].fill(0);
        self.buf[120..128].copy_from_slice(&bit_len.to_be_bytes());
        let block = self.buf;
        self.compress(&block);
        let mut out = Vec::with_capacity(64);
        for word in &self.state {
            out.extend_from_slice(&word.to_be_bytes());
        }
        out
    }
}

/// SHA-512 of all parts, concatenated. The one-shot form callers use.
pub fn sha512(parts: &[&[u8]]) -> Vec<u8> {
    let mut h = Sha512::new();
    for p in parts {
        h.update(p);
    }
    h.finalize()
}

/// SHA-512 as lowercase hex.
pub fn sha512_hex(parts: &[&[u8]]) -> String {
    hex(&sha512(parts))
}

/// Lowercase hex.
pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    // Every expected digest below was produced by the system's OpenSSL
    // (`printf %s ... | openssl dgst -sha512`) on this machine; the first
    // three also appear verbatim in the FIPS 180-4 example set. A port
    // that disagrees with any of them is wrong, full stop.

    #[test]
    fn nist_fips_180_4_examples_byte_for_byte() {
        assert_eq!(
            sha512_hex(&[b"abc"]),
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a\
             2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
                .replace(' ', "")
        );
        assert_eq!(
            sha512_hex(&[b""]),
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce\
             47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
                .replace(' ', "")
        );
        // The 448-bit two-block message from the FIPS example set.
        assert_eq!(
            sha512_hex(&[b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu"]),
            "8e959b75dae313da8cf4f72814fc143f8f7779c6eb9f7fa17299aeadb6889018\
             501d289e4900f7e4331b99dec4b5433ac7d329eeb6dd26545e96e55b874be909"
                .replace(' ', "")
        );
    }

    #[test]
    fn one_million_a_characters_streamed_and_one_shot() {
        // The classic million-'a' vector — streamed in 1000-byte pieces,
        // then one shot. Both must equal OpenSSL's digest exactly.
        let expected = "e718483d0ce769644e2e42c7bc15b4638e1f98b13b2044285632a803afa973e\
                        bde0ff244877ea60a4cb0432ce577c31beb009c5c2c49aa2e4eadb217ad8cc09b"
            .replace(' ', "");
        let chunk = "a".repeat(1000);
        let mut h = Sha512::new();
        for _ in 0..1000 {
            h.update(chunk.as_bytes());
        }
        let streamed = hex(&h.finalize());
        let one_shot = sha512_hex(&[&"a".repeat(1_000_000).into_bytes()]);
        assert_eq!(streamed, one_shot, "streaming must not change the digest");
        assert_eq!(one_shot, expected);
    }

    #[test]
    fn every_seam_gives_one_answer() {
        for n in [
            0usize, 1, 63, 111, 112, 113, 127, 128, 129, 191, 255, 256, 257,
        ] {
            let data = vec![0xa5u8; n];
            let whole = sha512(&[&data]);
            let split = n / 3;
            let (a, b) = data.split_at(split);
            assert_eq!(whole, sha512(&[a, b]), "seam at {split} of {n}");
            let mut h = Sha512::new();
            h.update(a);
            h.update(b);
            assert_eq!(whole, h.finalize(), "streamed seam at {split} of {n}");
        }
    }

    #[test]
    fn constants_match_the_fips_recipe() {
        // The derivation is the definition; check the first two full rows
        // (the most-reproduced values in any SHA-2 implementation) plus
        // the IV's first and last words.
        let expected_first_rows: [u64; 16] = [
            0x428a2f98d728ae22,
            0x7137449123ef65cd,
            0xb5c0fbcfec4d3b2f,
            0xe9b5dba58189dbbc,
            0x3956c25bf348b538,
            0x59f111f1b605d019,
            0x923f82a4af194f9b,
            0xab1c5ed5da6d8118,
            0xd807aa98a3030242,
            0x12835b0145706fbe,
            0x243185be4ee4b28c,
            0x550c7dc3d5ffb4e2,
            0x72be5d74f27b896f,
            0x80deb1fe3b1696b1,
            0x9bdc06a725c71235,
            0xc19bf174cf692694,
        ];
        for (i, want) in expected_first_rows.iter().enumerate() {
            assert_eq!(k_table()[i], *want, "K[{i}] disagrees with the FIPS table");
        }
        assert_eq!(iv()[0], 0x6a09e667f3bcc908);
        assert_eq!(iv()[7], 0x5be0cd19137e2179);
    }

    #[test]
    fn integer_helpers_are_exact() {
        assert_eq!(integer_sqrt(0), 0);
        assert_eq!(integer_sqrt(1), 1);
        assert_eq!(integer_sqrt(1u128 << 100), 1u128 << 50);
        assert_eq!(integer_sqrt((1u128 << 100) - 1), (1u128 << 50) - 1);
        // cbrt recipe: 8 is exactly 2, so the fractional part is zero.
        assert_eq!(frac_cbrt64(8), 0);
        assert_eq!(icbrt_small(8), 2);
        assert_eq!(icbrt_small(7), 1);
        // 2 has no integer cube root, so frac(cbrt(2)) * 2^64 is K[0].
        assert_eq!(frac_cbrt64(2), 0x428a2f98d728ae22);
        // And the IV recipe: 4 is a perfect square (frac 0); the famous
        // first two IV words ARE frac(sqrt(2)) and frac(sqrt(3)).
        assert_eq!(frac_sqrt64(4), 0);
        assert_eq!(frac_sqrt64(2), 0x6a09e667f3bcc908);
        assert_eq!(frac_sqrt64(3), 0xbb67ae8584caa73b);
        assert_eq!(primes(80).last(), Some(&409));
        assert_eq!(primes(10), vec![2, 3, 5, 7, 11, 13, 17, 19, 23, 29]);
    }
}
