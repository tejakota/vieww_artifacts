//! Content hashing for blueprint commits, cache keys and the artifact store.
//!
//! The hash is BLAKE3 (256-bit), ported from the public-domain reference
//! implementation (github.com/BLAKE3-team/BLAKE3, `reference_impl.rs`,
//! CC0 / Apache-2.0). We own the code; we do not take the crate. That is
//! the one-dependency doctrine applied to hashing (ADR-002, AGENTS.md
//! rule 3: hashing ports a public-domain reference, it is never invented).
//!
//! The port is the *reference* shape on purpose: one chunk state, one CV
//! stack, no SIMD. It is not the fastest BLAKE3 in the world, but it is
//! the one a sleepy engineer can check line by line against the spec,
//! and the tests below pin it to vectors produced by the official
//! implementation.
//!
//! Every hash in the system is a lowercase hex string. Strings debug well,
//! diff well, and name files in the content-addressed store directly.

const OUT_LEN: usize = 32;
const BLOCK_LEN: usize = 64;
const CHUNK_LEN: usize = 1024;

const CHUNK_START: u32 = 1 << 0;
const CHUNK_END: u32 = 1 << 1;
const PARENT: u32 = 1 << 2;
const ROOT: u32 = 1 << 3;

/// The IV is the SHA-256 IV, as in the spec.
const IV: [u32; 8] = [
    0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A, 0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19,
];

const MSG_PERMUTATION: [usize; 16] = [2, 6, 3, 10, 7, 0, 4, 13, 1, 11, 12, 5, 9, 14, 15, 8];

/// The quarter-round mixing function.
#[allow(clippy::too_many_arguments)]
fn g(state: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize, mx: u32, my: u32) {
    state[a] = state[a].wrapping_add(state[b]).wrapping_add(mx);
    state[d] = (state[d] ^ state[a]).rotate_right(16);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] = (state[b] ^ state[c]).rotate_right(12);
    state[a] = state[a].wrapping_add(state[b]).wrapping_add(my);
    state[d] = (state[d] ^ state[a]).rotate_right(8);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] = (state[b] ^ state[c]).rotate_right(7);
}

fn round(state: &mut [u32; 16], m: &[u32; 16]) {
    // Mix the columns.
    g(state, 0, 4, 8, 12, m[0], m[1]);
    g(state, 1, 5, 9, 13, m[2], m[3]);
    g(state, 2, 6, 10, 14, m[4], m[5]);
    g(state, 3, 7, 11, 15, m[6], m[7]);
    // Mix the diagonals.
    g(state, 0, 5, 10, 15, m[8], m[9]);
    g(state, 1, 6, 11, 12, m[10], m[11]);
    g(state, 2, 7, 8, 13, m[12], m[13]);
    g(state, 3, 4, 9, 14, m[14], m[15]);
}

fn permute(m: &mut [u32; 16]) {
    let mut permuted = [0; 16];
    for i in 0..16 {
        permuted[i] = m[MSG_PERMUTATION[i]];
    }
    *m = permuted;
}

fn compress(
    chaining_value: &[u32; 8],
    block_words: &[u32; 16],
    counter: u64,
    block_len: u32,
    flags: u32,
) -> [u32; 16] {
    let mut state = [
        chaining_value[0],
        chaining_value[1],
        chaining_value[2],
        chaining_value[3],
        chaining_value[4],
        chaining_value[5],
        chaining_value[6],
        chaining_value[7],
        IV[0],
        IV[1],
        IV[2],
        IV[3],
        counter as u32,
        (counter >> 32) as u32,
        block_len,
        flags,
    ];
    let mut block = *block_words;
    // Seven rounds, permuting the message between them (not after the last).
    for r in 0..7 {
        round(&mut state, &block);
        if r < 6 {
            permute(&mut block);
        }
    }
    for i in 0..8 {
        state[i] ^= state[i + 8];
        state[i + 8] ^= chaining_value[i];
    }
    state
}

fn first_8_words(compression_output: [u32; 16]) -> [u32; 8] {
    let mut out = [0u32; 8];
    out.copy_from_slice(&compression_output[0..8]);
    out
}

fn words_from_le_bytes(bytes: &[u8], words: &mut [u32]) {
    for (word, chunk) in words.iter_mut().zip(bytes.as_chunks::<4>().0) {
        *word = u32::from_le_bytes(*chunk);
    }
}

/// The state just before the last compression of a node: either finished
/// into a chaining value, or (at the root) into output bytes.
struct Output {
    input_chaining_value: [u32; 8],
    block_words: [u32; 16],
    counter: u64,
    block_len: u32,
    flags: u32,
}

impl Output {
    fn chaining_value(&self) -> [u32; 8] {
        first_8_words(compress(
            &self.input_chaining_value,
            &self.block_words,
            self.counter,
            self.block_len,
            self.flags,
        ))
    }

    fn root_output_bytes(&self, out: &mut [u8]) {
        for (output_block_counter, out_block) in out.chunks_mut(2 * OUT_LEN).enumerate() {
            let words = compress(
                &self.input_chaining_value,
                &self.block_words,
                output_block_counter as u64,
                self.block_len,
                self.flags | ROOT,
            );
            for (word, out_word) in words.iter().zip(out_block.chunks_mut(4)) {
                out_word.copy_from_slice(&word.to_le_bytes()[..out_word.len()]);
            }
        }
    }
}

struct ChunkState {
    chaining_value: [u32; 8],
    chunk_counter: u64,
    block: [u8; BLOCK_LEN],
    block_len: u8,
    blocks_compressed: u8,
    flags: u32,
}

impl ChunkState {
    fn new(key_words: [u32; 8], chunk_counter: u64, flags: u32) -> Self {
        ChunkState {
            chaining_value: key_words,
            chunk_counter,
            block: [0; BLOCK_LEN],
            block_len: 0,
            blocks_compressed: 0,
            flags,
        }
    }

    fn len(&self) -> usize {
        BLOCK_LEN * self.blocks_compressed as usize + self.block_len as usize
    }

    fn start_flag(&self) -> u32 {
        if self.blocks_compressed == 0 {
            CHUNK_START
        } else {
            0
        }
    }

    fn update(&mut self, mut input: &[u8]) {
        while !input.is_empty() {
            // If the block buffer is full, compress it and clear it. More
            // input is coming, so this compression is not CHUNK_END.
            if self.block_len as usize == BLOCK_LEN {
                let mut block_words = [0; 16];
                words_from_le_bytes(&self.block, &mut block_words);
                self.chaining_value = first_8_words(compress(
                    &self.chaining_value,
                    &block_words,
                    self.chunk_counter,
                    BLOCK_LEN as u32,
                    self.flags | self.start_flag(),
                ));
                self.blocks_compressed += 1;
                self.block = [0; BLOCK_LEN];
                self.block_len = 0;
            }

            let want = BLOCK_LEN - self.block_len as usize;
            let take = want.min(input.len());
            self.block[self.block_len as usize..][..take].copy_from_slice(&input[..take]);
            self.block_len += take as u8;
            input = &input[take..];
        }
    }

    fn output(&self) -> Output {
        let mut block_words = [0; 16];
        words_from_le_bytes(&self.block, &mut block_words);
        Output {
            input_chaining_value: self.chaining_value,
            block_words,
            counter: self.chunk_counter,
            block_len: self.block_len as u32,
            flags: self.flags | self.start_flag() | CHUNK_END,
        }
    }
}

fn parent_output(left: [u32; 8], right: [u32; 8], key_words: [u32; 8], flags: u32) -> Output {
    let mut block_words = [0; 16];
    block_words[..8].copy_from_slice(&left);
    block_words[8..].copy_from_slice(&right);
    Output {
        input_chaining_value: key_words,
        block_words,
        counter: 0,                  // always 0 for parent nodes
        block_len: BLOCK_LEN as u32, // always BLOCK_LEN for parent nodes
        flags: PARENT | flags,
    }
}

fn parent_cv(left: [u32; 8], right: [u32; 8], key_words: [u32; 8], flags: u32) -> [u32; 8] {
    parent_output(left, right, key_words, flags).chaining_value()
}

/// An incremental BLAKE3 hasher (unkeyed mode). Feed bytes with
/// [`Hasher::update`], read the 32-byte digest with [`Hasher::finalize`].
pub struct Hasher {
    chunk_state: ChunkState,
    key_words: [u32; 8],
    cv_stack: Vec<[u32; 8]>,
    flags: u32,
}

impl Default for Hasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Hasher {
    pub fn new() -> Self {
        Hasher {
            chunk_state: ChunkState::new(IV, 0, 0),
            key_words: IV,
            cv_stack: Vec::new(),
            flags: 0,
        }
    }

    // Section 5.1.2 of the BLAKE3 spec: the number of 1-bits in the total
    // chunk count is the number of CVs that stay on the stack; merge the
    // rest pairwise as completed subtrees.
    fn add_chunk_chaining_value(&mut self, mut new_cv: [u32; 8], mut total_chunks: u64) {
        while total_chunks & 1 == 0 {
            let left = self.cv_stack.pop().expect("cv stack holds a left sibling");
            new_cv = parent_cv(left, new_cv, self.key_words, self.flags);
            total_chunks >>= 1;
        }
        self.cv_stack.push(new_cv);
    }

    /// Absorb more input. Call as many times as you like.
    pub fn update(&mut self, mut input: &[u8]) -> &mut Self {
        while !input.is_empty() {
            // A full chunk, with more input coming: finalize it into the tree.
            if self.chunk_state.len() == CHUNK_LEN {
                let chunk_cv = self.chunk_state.output().chaining_value();
                let total_chunks = self.chunk_state.chunk_counter + 1;
                self.add_chunk_chaining_value(chunk_cv, total_chunks);
                self.chunk_state = ChunkState::new(self.key_words, total_chunks, self.flags);
            }
            let want = CHUNK_LEN - self.chunk_state.len();
            let take = want.min(input.len());
            self.chunk_state.update(&input[..take]);
            input = &input[take..];
        }
        self
    }

    /// The 32-byte digest of everything absorbed so far.
    pub fn finalize(&self) -> [u8; OUT_LEN] {
        let mut output = self.chunk_state.output();
        let mut parent_nodes_remaining = self.cv_stack.len();
        while parent_nodes_remaining > 0 {
            parent_nodes_remaining -= 1;
            output = parent_output(
                self.cv_stack[parent_nodes_remaining],
                output.chaining_value(),
                self.key_words,
                self.flags,
            );
        }
        let mut out = [0u8; OUT_LEN];
        output.root_output_bytes(&mut out);
        out
    }

    /// The digest as lowercase hex.
    pub fn finalize_hex(&self) -> String {
        to_hex(&self.finalize())
    }
}

/// Lowercase hex of any byte string.
pub fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(DIGITS[(b >> 4) as usize] as char);
        s.push(DIGITS[(b & 0xf) as usize] as char);
    }
    s
}

/// BLAKE3 of one byte slice, as hex. The content-address of a file.
pub fn blake3_hex(data: &[u8]) -> String {
    Hasher::new().update(data).finalize_hex()
}

/// Hash several byte slices as one logical value, as hex.
///
/// Each part is length-prefixed before hashing, so `["ab", "c"]` and
/// `["a", "bc"]` hash differently. Use this for every composite key
/// (cache keys, commit ids) — never concatenate by hand.
pub fn hash_hex(parts: &[&[u8]]) -> String {
    let mut h = Hasher::new();
    for part in parts {
        h.update(&(part.len() as u64).to_le_bytes());
        h.update(part);
    }
    h.finalize_hex()
}

/// A short, human-sized prefix of a hash for logs and the CLI. Never use
/// it as an identity — it is for eyes only.
pub fn short(hash: &str) -> &str {
    &hash[..hash.len().min(12)]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Input pattern of the official test vectors: byte i is i % 251.
    fn pattern(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i % 251) as u8).collect()
    }

    // Vectors produced by the official BLAKE3 implementation (Python
    // `blake3` package, which wraps the upstream Rust crate).
    const VECTORS: &[(usize, &str)] = &[
        (
            0,
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262",
        ),
        (
            1,
            "2d3adedff11b61f14c886e35afa036736dcd87a74d27b5c1510225d0f592e213",
        ),
        (
            63,
            "e9bc37a594daad83be9470df7f7b3798297c3d834ce80ba85d6e207627b7db7b",
        ),
        (
            64,
            "4eed7141ea4a5cd4b788606bd23f46e212af9cacebacdc7d1f4c6dc7f2511b98",
        ),
        (
            65,
            "de1e5fa0be70df6d2be8fffd0e99ceaa8eb6e8c93a63f2d8d1c30ecb6b263dee",
        ),
        (
            1023,
            "10108970eeda3eb932baac1428c7a2163b0e924c9a9e25b35bba72b28f70bd11",
        ),
        (
            1024,
            "42214739f095a406f3fc83deb889744ac00df831c10daa55189b5d121c855af7",
        ),
        (
            1025,
            "d00278ae47eb27b34faecf67b4fe263f82d5412916c1ffd97c8cb7fb814b8444",
        ),
        (
            2048,
            "e776b6028c7cd22a4d0ba182a8bf62205d2ef576467e838ed6f2529b85fba24a",
        ),
        (
            2049,
            "5f4d72f40d7a5f82b15ca2b2e44b1de3c2ef86c426c95c1af0b6879522563030",
        ),
        (
            3072,
            "b98cb0ff3623be03326b373de6b9095218513e64f1ee2edd2525c7ad1e5cffd2",
        ),
        (
            3073,
            "7124b49501012f81cc7f11ca069ec9226cecb8a2c850cfe644e327d22d3e1cd3",
        ),
        (
            8192,
            "aae792484c8efe4f19e2ca7d371d8c467ffb10748d8a5a1ae579948f718a2a63",
        ),
        (
            8193,
            "bab6c09cb8ce8cf459261398d2e7aef35700bf488116ceb94a36d0f5f1b7bc3b",
        ),
        (
            31744,
            "62b6960e1a44bcc1eb1a611a8d6235b6b4b78f32e7abc4fb4c6cdcce94895c47",
        ),
        (
            102400,
            "bc3e3d41a1146b069abffad3c0d44860cf664390afce4d9661f7902e7943e085",
        ),
    ];

    #[test]
    fn matches_the_official_vectors() {
        for (n, want) in VECTORS {
            assert_eq!(blake3_hex(&pattern(*n)), *want, "length {n}");
        }
        assert_eq!(
            blake3_hex(b"abc"),
            "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85"
        );
    }

    #[test]
    fn incremental_equals_one_shot() {
        let data = pattern(5000);
        let mut h = Hasher::new();
        for piece in data.chunks(77) {
            h.update(piece);
        }
        assert_eq!(h.finalize_hex(), blake3_hex(&data));
    }

    #[test]
    fn length_prefixing_prevents_concat_ambiguity() {
        assert_ne!(hash_hex(&[b"ab", b"c"]), hash_hex(&[b"a", b"bc"]));
        assert_eq!(hash_hex(&[b"x"]).len(), 64);
    }
}
