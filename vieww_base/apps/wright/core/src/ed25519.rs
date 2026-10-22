//! Ed25519 (RFC 8032), a reference port in safe Rust.
//!
//! Why hand-rolled: Wright signs run receipts with it, receipts are the
//! product's core promise, and the doctrine (ADR-012 crypto boundary)
//! allows a reference port in `wright-core` only when it is pinned
//! byte-for-byte to the standard's own test vectors — plus, in our case,
//! cross-checked against OpenSSL-generated vectors from this machine.
//! No crates, no unsafe, no data-dependent indexing.
//!
//! Shape (the same one the RFC's reference C code uses):
//!
//! * the field GF(2^255-19) as five 51-bit limbs;
//! * every "magic" constant is COMPUTED from its definition, never typed
//!   from memory: `d` is `-121665/121666`, the base point's `x` is
//!   recovered from `y = 4/5`, `sqrt(-1)` is `2^((p-1)/4)`. The group
//!   order `L` is the one literal — it is the definition, not a
//!   derivation;
//! * scalars are handled mod `L` with plain limb long division — signing
//!   is rare, so clarity beats a Barrett engine;
//! * the scalar ladder uses a masked cswap, so the instruction flow does
//!   not depend on the exponent bits (a shape, not a proof).
//!
//! What the tests pin: RFC 8032 §7.1 vectors byte-for-byte, three
//! OpenSSL-generated sign pairs byte-for-byte, tamper rejection, the
//! RFC's canonicality rules (S >= L, non-canonical y), and every derived
//! constant (d, sqrt(-1), the base point encoding).

use std::sync::OnceLock;

use crate::sha512::{sha512, Sha512};

// ── the field GF(2^255 - 19) ─────────────────────────────────────

/// A field element: five limbs, radix 2^51, little-endian. Between
/// operations limbs may be a few bits over 2^51; [`fe_carry`] brings
/// them back, and [`fe_tobytes`] fully reduces.
type Fe = [u64; 5];

const MASK51: u64 = (1 << 51) - 1;

fn fe_zero() -> Fe {
    [0; 5]
}

fn fe_one() -> Fe {
    [1, 0, 0, 0, 0]
}

fn fe_small(v: u64) -> Fe {
    [v & MASK51, v >> 51, 0, 0, 0]
}

fn fe_add(a: &Fe, b: &Fe) -> Fe {
    let mut r = [0u64; 5];
    for i in 0..5 {
        r[i] = a[i] + b[i];
    }
    fe_carry(&mut r);
    r
}

/// a - b: add 2p first, so no limb ever goes negative.
fn fe_sub(a: &Fe, b: &Fe) -> Fe {
    // 2p = (2^52-38) + (2^52-2)·2^51 + (2^52-2)·2^102 + ...
    const TWO_P: Fe = [
        (1 << 52) - 38,
        (1 << 52) - 2,
        (1 << 52) - 2,
        (1 << 52) - 2,
        (1 << 52) - 2,
    ];
    let mut r = [0u64; 5];
    for i in 0..5 {
        let av = a[i] + TWO_P[i];
        if av >= b[i] {
            r[i] = av - b[i];
        } else {
            r[i] = av.wrapping_sub(b[i]); // wraps to av - b + 2^64, in range
        }
    }
    fe_carry(&mut r);
    r
}

/// Propagate carries so every limb is < 2^51 (limb 2 may carry +1 once
/// more; callers that need exact canonicality use [`fe_tobytes`]).
fn fe_carry(r: &mut Fe) {
    let mut c = r[0] >> 51;
    r[0] &= MASK51;
    r[1] += c;
    c = r[1] >> 51;
    r[1] &= MASK51;
    r[2] += c;
    c = r[2] >> 51;
    r[2] &= MASK51;
    r[3] += c;
    c = r[3] >> 51;
    r[3] &= MASK51;
    r[4] += c;
    c = r[4] >> 51;
    r[4] &= MASK51;
    // 2^255 == 19 (mod p): the top carry folds back as 19.
    r[0] += c * 19;
    c = r[0] >> 51;
    r[0] &= MASK51;
    r[1] += c;
    c = r[1] >> 51;
    r[1] &= MASK51;
    r[2] += c;
}

fn fe_mul(a: &Fe, b: &Fe) -> Fe {
    let (a0, a1, a2, a3, a4) = (
        a[0] as u128,
        a[1] as u128,
        a[2] as u128,
        a[3] as u128,
        a[4] as u128,
    );
    let (b0, b1, b2, b3, b4) = (
        b[0] as u128,
        b[1] as u128,
        b[2] as u128,
        b[3] as u128,
        b[4] as u128,
    );
    let b1_19 = 19 * b1;
    let b2_19 = 19 * b2;
    let b3_19 = 19 * b3;
    let b4_19 = 19 * b4;
    // Schoolbook with the 2^255 = 19 fold built into the top products.
    let mut t0 = a0 * b0 + a1 * b4_19 + a2 * b3_19 + a3 * b2_19 + a4 * b1_19;
    let mut t1 = a0 * b1 + a1 * b0 + a2 * b4_19 + a3 * b3_19 + a4 * b2_19;
    let mut t2 = a0 * b2 + a1 * b1 + a2 * b0 + a3 * b4_19 + a4 * b3_19;
    let mut t3 = a0 * b3 + a1 * b2 + a2 * b1 + a3 * b0 + a4 * b4_19;
    let mut t4 = a0 * b4 + a1 * b3 + a2 * b2 + a3 * b1 + a4 * b0;
    let m = MASK51 as u128;
    let c = t0 >> 51;
    t0 &= m;
    t1 += c;
    let c = t1 >> 51;
    t1 &= m;
    t2 += c;
    let c = t2 >> 51;
    t2 &= m;
    t3 += c;
    let c = t3 >> 51;
    t3 &= m;
    t4 += c;
    let mut r = [t0 as u64, t1 as u64, t2 as u64, t3 as u64, (t4 & m) as u64];
    r[0] += (t4 >> 51) as u64 * 19;
    fe_carry(&mut r);
    r
}

fn fe_sq(a: &Fe) -> Fe {
    fe_mul(a, a)
}

/// Swap a and b when bit == 1, with a masked (branchless-shape) select.
fn fe_cswap(bit: u64, a: &mut Fe, b: &mut Fe) {
    let mask = bit.wrapping_neg(); // 0 or all-ones
    for i in 0..5 {
        let t = mask & (a[i] ^ b[i]);
        a[i] ^= t;
        b[i] ^= t;
    }
}

/// a^e for a little-endian byte exponent (square-and-multiply, MSB first).
fn fe_pow(a: &Fe, e: &[u8]) -> Fe {
    let mut r = fe_one();
    for byte_index in (0..e.len()).rev() {
        let byte = e[byte_index];
        for bit in (0..8).rev() {
            r = fe_sq(&r);
            if (byte >> bit) & 1 == 1 {
                r = fe_mul(&r, a);
            }
        }
    }
    r
}

/// 1/a. p is prime, so a^(p-2) does it; p-2 = 2^255 - 21 (LE bytes).
fn fe_inv(a: &Fe) -> Fe {
    const P_MINUS_2: [u8; 32] = [
        0xeb, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0x7f,
    ];
    fe_pow(a, &P_MINUS_2)
}

/// sqrt(-1) = 2^((p-1)/4), with (p-1)/4 = 2^253 - 5 (LE bytes).
fn fe_sqrt_m1() -> Fe {
    static I: OnceLock<Fe> = OnceLock::new();
    *I.get_or_init(|| {
        const EXP: [u8; 32] = [
            0xfb, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0x1f,
        ];
        fe_pow(&fe_small(2), &EXP)
    })
}

/// Little-endian 32 bytes in. The sign bit (bit 255) is not part of any
/// limb, so callers mask it before or after — both work.
fn fe_frombytes(bytes: &[u8; 32]) -> Fe {
    let load8 = |b: &[u8], off: usize| -> u64 {
        let mut v = [0u8; 8];
        v.copy_from_slice(&b[off..off + 8]);
        u64::from_le_bytes(v)
    };
    let mut r = fe_zero();
    r[0] = load8(bytes, 0) & MASK51;
    r[1] = (load8(bytes, 6) >> 3) & MASK51;
    r[2] = (load8(bytes, 12) >> 6) & MASK51;
    r[3] = (load8(bytes, 19) >> 1) & MASK51;
    r[4] = (load8(bytes, 24) >> 12) & MASK51;
    r
}

/// Serialize fully reduced and canonical. The add-19 trick: `value + 19`
/// crosses 2^255 exactly when `value >= p`; when it does, the wrapped
/// limbs already equal `value - p` (because p = 2^255 - 19), and when it
/// does not, subtract the 19 back.
fn fe_tobytes(a: &Fe) -> [u8; 32] {
    let mut t = *a;
    fe_carry(&mut t);
    // u = t + 19, carried, WITHOUT folding the bit-255 carry.
    let mut u = t;
    u[0] += 19;
    let mut c = u[0] >> 51;
    u[0] &= MASK51;
    u[1] += c;
    c = u[1] >> 51;
    u[1] &= MASK51;
    u[2] += c;
    c = u[2] >> 51;
    u[2] &= MASK51;
    u[3] += c;
    c = u[3] >> 51;
    u[3] &= MASK51;
    u[4] += c;
    let folded = u[4] >> 51;
    u[4] &= MASK51;
    if folded == 0 {
        // value < p: undo the +19.
        let mut borrow = 19u64;
        for limb in u.iter_mut() {
            let (v, b) = limb.overflowing_sub(borrow);
            *limb = v;
            borrow = b as u64;
            if borrow == 0 {
                break;
            }
        }
    }
    // Pack five 51-bit words into 32 LE bytes.
    let v0 = u[0] | (u[1] << 51);
    let v1 = (u[1] >> 13) | (u[2] << 38);
    let v2 = (u[2] >> 26) | (u[3] << 25);
    let v3 = (u[3] >> 39) | (u[4] << 12);
    let mut out = [0u8; 32];
    for (i, v) in [v0, v1, v2, v3].iter().enumerate() {
        out[i * 8..(i + 1) * 8].copy_from_slice(&v.to_le_bytes());
    }
    out
}

/// True when a carried field element is < p (canonical). Same add-19
/// trick as [`fe_tobytes`], minus the subtraction.
fn fe_less_than_p(a: &Fe) -> bool {
    let mut u = *a;
    u[0] += 19;
    let mut c = u[0] >> 51;
    u[0] &= MASK51;
    u[1] += c;
    c = u[1] >> 51;
    u[1] &= MASK51;
    u[2] += c;
    c = u[2] >> 51;
    u[2] &= MASK51;
    u[3] += c;
    c = u[3] >> 51;
    u[3] &= MASK51;
    u[4] += c;
    u[4] >> 51 == 0
}

/// Compare two field elements for equality of their canonical values.
fn fe_eq_canon(a: &Fe, b: &Fe) -> bool {
    fe_tobytes(a) == fe_tobytes(b)
}

// ── the group: scalars mod L ────────────────────────────────────

/// The group order L = 2^252 + 27742317777372353535851937790883648493,
/// as four little-endian u64 limbs. The one typed constant: it is the
/// definition of the group, not a derived value.
const L: [u64; 4] = [
    0x5812631a5cf5d3ed,
    0x14def9dea2f79cd6,
    0x0000000000000000,
    0x1000000000000000,
];

/// True when x >= L (four-limb little-endian comparison).
fn geq_l(x: &[u64; 4]) -> bool {
    for i in (0..4).rev() {
        if x[i] != L[i] {
            return x[i] > L[i];
        }
    }
    true // equal counts as >=
}

/// Reduce a 512-bit little-endian byte string mod L. Bit-by-bit long
/// division: `rem = rem·2 + bit; if rem >= L: rem -= L`. rem < L < 2^253
/// before each shift, so nothing overflows. 512 rounds of tiny work.
fn sc_reduce512(bytes: &[u8; 64]) -> [u64; 4] {
    let mut rem = [0u64; 4];
    for byte_index in (0..64).rev() {
        let byte = bytes[byte_index];
        for bit in (0..8).rev() {
            let mut carry = ((byte >> bit) & 1) as u64;
            for limb in rem.iter_mut() {
                let new_carry = *limb >> 63;
                *limb = (*limb << 1) | carry;
                carry = new_carry;
            }
            if geq_l(&rem) {
                let mut borrow = 0u64;
                for i in 0..4 {
                    let (v, b) = rem[i].overflowing_sub(L[i].wrapping_add(borrow));
                    rem[i] = v;
                    borrow = b as u64;
                }
            }
        }
    }
    rem
}

/// (k * a + r) mod L, inputs and output as 32 LE bytes.
fn sc_muladd(r: &[u8; 32], k: &[u8; 32], a: &[u8; 32]) -> [u8; 32] {
    let load = |b: &[u8; 32], i: usize| -> u64 {
        let mut v = [0u8; 8];
        v.copy_from_slice(&b[i * 8..i * 8 + 8]);
        u64::from_le_bytes(v)
    };
    let (kl, al, rl) = (
        [load(k, 0), load(k, 1), load(k, 2), load(k, 3)],
        [load(a, 0), load(a, 1), load(a, 2), load(a, 3)],
        [load(r, 0), load(r, 1), load(r, 2), load(r, 3)],
    );
    // 4x4 -> 8 limbs, then add r into the low half.
    let mut prod = [0u128; 8];
    for i in 0..4 {
        let mut carry = 0u128;
        for j in 0..4 {
            let t = prod[i + j] + (kl[i] as u128) * (al[j] as u128) + carry;
            prod[i + j] = t & u64::MAX as u128;
            carry = t >> 64;
        }
        prod[i + 4] = carry;
    }
    let mut carry = 0u128;
    for i in 0..4 {
        let t = prod[i] + rl[i] as u128 + carry;
        prod[i] = t & u64::MAX as u128;
        carry = t >> 64;
    }
    prod[4] += carry;
    let mut bytes = [0u8; 64];
    for i in 0..8 {
        bytes[i * 8..i * 8 + 8].copy_from_slice(&(prod[i] as u64).to_le_bytes());
    }
    sc_to_bytes(&sc_reduce512(&bytes))
}

/// The four limbs of a scalar as 32 LE bytes.
fn sc_to_bytes(s: &[u64; 4]) -> [u8; 32] {
    let mut out = [0u8; 32];
    for i in 0..4 {
        out[i * 8..i * 8 + 8].copy_from_slice(&s[i].to_le_bytes());
    }
    out
}

/// True when the 32-LE-byte scalar is < L (verify needs strict <).
fn sc_less_than_l(a: &[u8; 32]) -> bool {
    let mut limbs = [0u64; 4];
    for i in 0..4 {
        let mut v = [0u8; 8];
        v.copy_from_slice(&a[i * 8..i * 8 + 8]);
        limbs[i] = u64::from_le_bytes(v);
    }
    !geq_l(&limbs)
}

// ── points ──────────────────────────────────────────────────────

/// d = -121665/121666 (mod p): the curve's twist constant.
fn curve_d() -> Fe {
    static D: OnceLock<Fe> = OnceLock::new();
    *D.get_or_init(|| {
        fe_sub(
            &fe_zero(),
            &fe_mul(&fe_small(121_665), &fe_inv(&fe_small(121_666))),
        )
    })
}

/// 2d, as the Edwards addition formula wants it.
fn curve_d2() -> Fe {
    static D2: OnceLock<Fe> = OnceLock::new();
    *D2.get_or_init(|| fe_add(&curve_d(), &curve_d()))
}

/// A point in extended twisted-Edwards coordinates (X:Y:Z:T):
/// x = X/Z, y = Y/Z, T = XY/Z.
#[derive(Clone, Copy)]
struct Pt {
    x: Fe,
    y: Fe,
    z: Fe,
    t: Fe,
}

fn pt_identity() -> Pt {
    Pt {
        x: fe_zero(),
        y: fe_one(),
        z: fe_one(),
        t: fe_zero(),
    }
}

/// Unified addition (RFC 8032 §5.1.4) — also correct for P + P.
fn pt_add(p: &Pt, q: &Pt) -> Pt {
    let d2 = curve_d2();
    let a = fe_mul(&fe_sub(&p.y, &p.x), &fe_sub(&q.y, &q.x));
    let b = fe_mul(&fe_add(&p.y, &p.x), &fe_add(&q.y, &q.x));
    let c = fe_mul(&fe_mul(&d2, &p.t), &q.t);
    let d = fe_mul(&fe_add(&p.z, &p.z), &q.z);
    let e = fe_sub(&b, &a);
    let f = fe_sub(&d, &c);
    let g = fe_add(&d, &c);
    let h = fe_add(&b, &a);
    Pt {
        x: fe_mul(&e, &f),
        y: fe_mul(&g, &h),
        z: fe_mul(&f, &g),
        t: fe_mul(&e, &h),
    }
}

fn pt_double(p: &Pt) -> Pt {
    pt_add(p, p)
}

/// The masked-cswap ladder. `s` is 32 LE bytes; all 256 bits are walked
/// (reduced scalars have zero top bits, which the ladder handles).
fn pt_scalarmult(s: &[u8; 32], base: &Pt) -> Pt {
    let mut r0 = pt_identity();
    let mut r1 = *base;
    let mut swap = 0u64;
    for byte_index in (0..32).rev() {
        let byte = s[byte_index];
        for bit in (0..8).rev() {
            let b = ((byte >> bit) & 1) as u64;
            swap ^= b;
            fe_cswap(swap, &mut r0.x, &mut r1.x);
            fe_cswap(swap, &mut r0.y, &mut r1.y);
            fe_cswap(swap, &mut r0.z, &mut r1.z);
            fe_cswap(swap, &mut r0.t, &mut r1.t);
            swap = b;
            r1 = pt_add(&r0, &r1);
            r0 = pt_double(&r0);
        }
    }
    fe_cswap(swap, &mut r0.x, &mut r1.x);
    fe_cswap(swap, &mut r0.y, &mut r1.y);
    fe_cswap(swap, &mut r0.z, &mut r1.z);
    fe_cswap(swap, &mut r0.t, &mut r1.t);
    r0
}

/// The base point: y = 4/5, x recovered and chosen EVEN — the point the
/// RFC calls B. Its encoding's sign bit is 0, pinned by the tests.
fn base_point() -> Pt {
    static B: OnceLock<(Fe, Fe)> = OnceLock::new();
    let (x, y) = B.get_or_init(|| {
        let y = fe_mul(&fe_small(4), &fe_inv(&fe_small(5)));
        let x = recover_x(&y, 0).expect("y = 4/5 is on the curve by definition");
        (x, y)
    });
    Pt {
        x: *x,
        y: *y,
        z: fe_one(),
        t: fe_mul(x, y),
    }
}

/// Recover x from a curve y and a parity bit (RFC 8032 §5.1.3).
/// None when y is not a canonical field element, not on the curve, or
/// encodes x = 0 with sign = 1.
fn recover_x(y: &Fe, sign: u8) -> Option<Fe> {
    if !fe_less_than_p(y) {
        return None; // y >= p: non-canonical, reject (RFC §10)
    }
    // x^2 = (y^2 - 1) / (d y^2 + 1) = u / v
    let y2 = fe_sq(y);
    let u = fe_sub(&y2, &fe_one());
    let v = fe_add(&fe_mul(&curve_d(), &y2), &fe_one());
    // x = (u v^3) (u v^7)^((p-5)/8); fix up with sqrt(-1) if needed.
    let v3 = fe_mul(&v, &fe_sq(&v)); // v^3
    let v7 = fe_mul(&v3, &fe_sq(&fe_sq(&v))); // v^3 * v^4 = v^7
                                              // (p-5)/8 = 2^252 - 3 (LE bytes).
    const EXP: [u8; 32] = [
        0xfd, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0x0f,
    ];
    let mut x = fe_mul(&fe_mul(&u, &v3), &fe_pow(&fe_mul(&u, &v7), &EXP));
    let vxx = fe_mul(&v, &fe_sq(&x));
    if !fe_eq_canon(&vxx, &u) {
        x = fe_mul(&x, &fe_sqrt_m1());
        let vxx = fe_mul(&v, &fe_sq(&x));
        if !fe_eq_canon(&vxx, &u) {
            return None; // no square root: y is not on the curve
        }
    }
    if fe_eq_canon(&x, &fe_zero()) {
        if sign == 1 {
            return None; // x = 0 encodes only with sign 0
        }
    } else if (fe_tobytes(&x)[0] & 1) != sign {
        x = fe_sub(&fe_zero(), &x); // the other root: p - x
    }
    Some(x)
}

/// Encode a point: affine y and the parity bit of affine x.
fn pt_encode(p: &Pt) -> [u8; 32] {
    let zinv = fe_inv(&p.z);
    let x = fe_mul(&p.x, &zinv);
    let y = fe_mul(&p.y, &zinv);
    let mut out = fe_tobytes(&y);
    out[31] |= (fe_tobytes(&x)[0] & 1) << 7;
    out
}

/// Decode 32 bytes into a point, strictly (RFC 8032 §5.1.3).
fn pt_decode(bytes: &[u8; 32]) -> Option<Pt> {
    let mut ybytes = *bytes;
    let sign = (ybytes[31] >> 7) & 1;
    ybytes[31] &= 0x7f;
    let y = fe_frombytes(&ybytes);
    let x = recover_x(&y, sign)?;
    let t = fe_mul(&x, &y);
    Some(Pt {
        x,
        y,
        z: fe_one(),
        t,
    })
}

// ── the API ─────────────────────────────────────────────────────

/// A public key: the encoding of A = a·B.
pub type PublicKey = [u8; 32];
/// A signature: R || S.
pub type Signature = [u8; 64];
/// A 32-byte seed — what the OS keychain stores (the RFC's private key).
pub type SecretKey = [u8; 32];

fn clamp_scalar(a: &mut [u8; 32]) {
    a[0] &= 248;
    a[31] &= 127;
    a[31] |= 64;
}

/// The public key for a seed.
pub fn public_key(seed: &SecretKey) -> PublicKey {
    let mut h = [0u8; 64];
    h.copy_from_slice(&sha512(&[seed]));
    let mut a = [0u8; 32];
    a.copy_from_slice(&h[..32]);
    clamp_scalar(&mut a);
    pt_encode(&pt_scalarmult(&a, &base_point()))
}

/// Sign a message with a seed (RFC 8032 §5.1.6). Returns R || S.
pub fn sign(seed: &SecretKey, msg: &[u8]) -> Signature {
    let mut h = [0u8; 64];
    h.copy_from_slice(&sha512(&[seed]));
    let mut a = [0u8; 32];
    let mut prefix = [0u8; 32];
    a.copy_from_slice(&h[..32]);
    prefix.copy_from_slice(&h[32..]);
    clamp_scalar(&mut a);
    let pk = pt_encode(&pt_scalarmult(&a, &base_point()));

    // r = SHA-512(prefix || M) mod L; R = r·B
    let mut rr = Sha512::new();
    rr.update(&prefix);
    rr.update(msg);
    let mut rb = [0u8; 64];
    rb.copy_from_slice(&rr.finalize());
    let r = sc_to_bytes(&sc_reduce512(&rb));
    let big_r = pt_encode(&pt_scalarmult(&r, &base_point()));

    // k = SHA-512(R || A || M) mod L; S = (r + k·a) mod L
    let mut kh = Sha512::new();
    kh.update(&big_r);
    kh.update(&pk);
    kh.update(msg);
    let mut kb = [0u8; 64];
    kb.copy_from_slice(&kh.finalize());
    let k = sc_to_bytes(&sc_reduce512(&kb));
    let s = sc_muladd(&r, &k, &a);

    let mut sig: Signature = [0; 64];
    sig[..32].copy_from_slice(&big_r);
    sig[32..].copy_from_slice(&s);
    sig
}

/// Verify (RFC 8032 §5.1.7, the cofactored form): 8SB = 8R + 8kA —
/// equivalently, since 8 distributes, 8·(SB) = 8·(R + kA). Strict about
/// every canonicality rule: S < L, A and R decode, y < p.
pub fn verify(pk: &PublicKey, msg: &[u8], sig: &Signature) -> bool {
    let mut big_r = [0u8; 32];
    let mut s = [0u8; 32];
    big_r.copy_from_slice(&sig[..32]);
    s.copy_from_slice(&sig[32..]);
    if !sc_less_than_l(&s) {
        return false; // S >= L: non-canonical, reject (RFC §10)
    }
    let Some(a_pt) = pt_decode(pk) else {
        return false;
    };
    let Some(r_pt) = pt_decode(&big_r) else {
        return false;
    };
    let mut kh = Sha512::new();
    kh.update(&big_r);
    kh.update(pk);
    kh.update(msg);
    let mut kb = [0u8; 64];
    kb.copy_from_slice(&kh.finalize());
    let k = sc_to_bytes(&sc_reduce512(&kb));

    // lhs = 8·(S·B), rhs = 8·(R + k·A); compare encodings.
    let mut lhs = pt_scalarmult(&s, &base_point());
    let mut rhs = pt_add(&r_pt, &pt_scalarmult(&k, &a_pt));
    for _ in 0..3 {
        lhs = pt_double(&lhs);
        rhs = pt_double(&rhs);
    }
    pt_encode(&lhs) == pt_encode(&rhs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unhex<const N: usize>(s: &str) -> [u8; N] {
        let clean: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        assert_eq!(clean.len(), N * 2, "bad vector length: {clean}");
        let mut out = [0u8; N];
        for i in 0..N {
            out[i] = u8::from_str_radix(&clean[i * 2..i * 2 + 2], 16).unwrap();
        }
        out
    }

    fn hex(bytes: &[u8]) -> String {
        crate::sha512::hex(bytes)
    }

    #[test]
    fn rfc8032_vector_1_empty_message() {
        let seed = unhex::<32>("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60");
        assert_eq!(
            hex(&public_key(&seed)),
            "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
            "public key: RFC 8032 §7.1 test 1"
        );
        let msg: &[u8] = b"";
        assert_eq!(
            hex(&sign(&seed, msg)),
            "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155\
             5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
            "signature: RFC 8032 §7.1 test 1, byte for byte"
        );
        assert!(verify(&public_key(&seed), msg, &sign(&seed, msg)));
    }

    #[test]
    fn rfc8032_vector_2_one_byte() {
        let seed = unhex::<32>("4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb");
        // The public key below is what OpenSSL derives from this seed on
        // this machine — and it is what the RFC prints. (An earlier draft
        // of this test trusted a misremembered constant; the machine won.)
        assert_eq!(
            hex(&public_key(&seed)),
            "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c",
            "public key: RFC 8032 §7.1 test 2"
        );
        let msg: &[u8] = b"\x72";
        assert_eq!(
            hex(&sign(&seed, msg)),
            "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da\
             085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00",
            "signature: RFC 8032 §7.1 test 2, byte for byte"
        );
        assert!(verify(&public_key(&seed), msg, &sign(&seed, msg)));
    }

    // Generated on this machine by OpenSSL 3.x (see the module comment):
    //   openssl genpkey -algorithm ED25519; pkeyutl -sign -rawin
    // The seed, the exact message bytes, and the 64-byte signature are
    // pinned here so the cross-check needs no openssl at test time.

    #[test]
    fn openssl_pair_1() {
        let seed = unhex::<32>("a8674e91c8edb36b0c7e01e033b1e498b72083e3fbaf33d596ed64629697d7fa");
        let msg = b"wright receipts cross vector".as_slice();
        let sig = unhex::<64>(
            "d1c63f7f8ff88f5f4526a2e1bbc2d956da059d825cded069d8195cb4af9d3df0\
             475ec5b5813f5d3edc63234a7b94cd351a215ee4e7e402abbd1c9a68a5e25e05",
        );
        assert_eq!(
            hex(&public_key(&seed)),
            "975b954a54e19b95b63713955771a041ab1ebff562896ab445097a5fad7f7d7f",
            "pub key"
        );
        assert_eq!(
            hex(&sign(&seed, msg)),
            hex(&sig),
            "must reproduce OpenSSL's signature byte for byte"
        );
        assert!(verify(&public_key(&seed), msg, &sig));
    }

    #[test]
    fn openssl_pair_2() {
        let seed = unhex::<32>("74e6ec9f386e2607b71c1e8ecbeb9da2abb10b09987ba682c1bf1db20aa93056");
        let msg = b"wright receipts cross vector".as_slice();
        let sig = unhex::<64>(
            "163f0dd7b886430d9eb84ee3de9b978c91133ca8e4226bffea44a1dc5b2f80fb\
             f8ec680f377ae7ad43b0b392baf9365e723eb9324311ac652efb76f0358efb00",
        );
        assert_eq!(
            hex(&public_key(&seed)),
            "6f1829f20001806e37ada6954f40e0f05e7bdb49c6778f3fd24af110933e4deb",
            "pub key"
        );
        assert_eq!(
            hex(&sign(&seed, msg)),
            hex(&sig),
            "must reproduce OpenSSL's signature byte for byte"
        );
        assert!(verify(&public_key(&seed), msg, &sig));
    }

    #[test]
    fn openssl_pair_3() {
        let seed = unhex::<32>("d9b0c7e5440abf9599f58d309f171a6d7b994f3609ffd913a9090d70bc0829ff");
        let msg = b"wright receipts cross vector".as_slice();
        let sig = unhex::<64>(
            "c81a6db1c565fb18a5ce2864ca13b3aabf99fc6a4c92ff26f26482c1ba29dd42\
             e197ebbf29efbf4f590be77f19e1e7df8fdd8462286f97fffa79d735dfe0ed02",
        );
        assert_eq!(
            hex(&public_key(&seed)),
            "fb01eb0b53ff3927626713cabe7d2027a6d98d183278e8b24bfcc4ffa293b483",
            "pub key"
        );
        assert_eq!(
            hex(&sign(&seed, msg)),
            hex(&sig),
            "must reproduce OpenSSL's signature byte for byte"
        );
        assert!(verify(&public_key(&seed), msg, &sig));
    }

    #[test]
    fn field_identities_and_reduction() {
        // 7 + (-7) == 0
        let a = fe_small(7);
        let neg = fe_sub(&fe_zero(), &a);
        assert_eq!(fe_tobytes(&fe_add(&a, &neg)), [0u8; 32]);
        // 7 · inv(7) == 1
        assert_eq!(fe_tobytes(&fe_mul(&a, &fe_inv(&a))), fe_tobytes(&fe_one()));
        // I^2 == -1
        let i = fe_sqrt_m1();
        assert_eq!(
            fe_tobytes(&fe_sq(&i)),
            fe_tobytes(&fe_sub(&fe_zero(), &fe_one()))
        );
        // p-1 is canonical; p itself is not (it reduces to 0)
        let p_minus_1 = fe_sub(&fe_zero(), &fe_one());
        assert!(fe_less_than_p(&p_minus_1));
        // fe_tobytes is fully reduced: ((p-1) + 1) == 0
        assert_eq!(fe_tobytes(&fe_add(&p_minus_1, &fe_one())), [0u8; 32]);
        // round-trip: serialize -> reparse -> serialize is a fixpoint
        let v = [12345u64, 67890, 111213, 141516, 171819];
        let once = fe_tobytes(&v);
        assert_eq!(fe_tobytes(&fe_frombytes(&once)), once);
        // the all-ones 255-bit value is >= p, so it is NOT canonical
        assert!(!fe_less_than_p(&fe_frombytes(&[0xff; 32])));
        // but p-1 is
        assert!(fe_less_than_p(&fe_frombytes(&fe_tobytes(&p_minus_1))));
    }

    #[test]
    fn base_point_pinned_by_its_standard_encoding() {
        // Computed from y = 4/5 + even x, then pinned to the well-known
        // encoding; if the derivation drifts, this fails.
        assert_eq!(
            hex(&pt_encode(&base_point())),
            "5866666666666666666666666666666666666666666666666666666666666666"
        );
        // And it is on the curve: -x^2 + y^2 = 1 + d x^2 y^2
        let b = base_point();
        let x2 = fe_sq(&b.x);
        let y2 = fe_sq(&b.y);
        let lhs = fe_sub(&y2, &x2); // -x^2 + y^2
        let rhs = fe_add(&fe_one(), &fe_mul(&curve_d(), &fe_mul(&x2, &y2)));
        assert!(fe_eq_canon(&lhs, &rhs), "B must satisfy the curve equation");
    }

    #[test]
    fn ladder_agrees_with_repeated_addition() {
        let b = base_point();
        let mut three = [0u8; 32];
        three[0] = 3;
        let via_ladder = pt_scalarmult(&three, &b);
        let via_add = pt_add(&pt_add(&b, &b), &b);
        assert_eq!(pt_encode(&via_ladder), pt_encode(&via_add));
        // 0·B is the identity: y = 1, x = 0, sign 0.
        assert_eq!(
            hex(&pt_encode(&pt_scalarmult(&[0u8; 32], &b))),
            "0100000000000000000000000000000000000000000000000000000000000000"
        );
    }

    #[test]
    fn tampered_message_signature_or_key_is_rejected() {
        let seed: SecretKey = [0x42u8; 32];
        let pk = public_key(&seed);
        let msg = b"the plan ran for 6 tasks and cost $0.04".as_slice();
        let sig = sign(&seed, msg);
        assert!(verify(&pk, msg, &sig));
        assert!(
            !verify(&pk, b"the plan ran for 7 tasks and cost $0.04", &sig),
            "changed message"
        );
        let mut bad = sig;
        bad[0] ^= 1;
        assert!(!verify(&pk, msg, &bad), "changed R");
        let mut bad = sig;
        bad[63] ^= 1;
        assert!(!verify(&pk, msg, &bad), "changed S");
        let other: SecretKey = [0x43u8; 32];
        assert!(!verify(&public_key(&other), msg, &sig), "wrong key");
    }

    #[test]
    fn non_canonical_inputs_are_rejected() {
        let seed: SecretKey = [7u8; 32];
        let pk = public_key(&seed);
        let msg = b"hello".as_slice();
        let sig = sign(&seed, msg);
        // S = L must be rejected.
        let mut forged = sig;
        forged[32..].copy_from_slice(&sc_to_bytes(&L));
        assert!(!verify(&pk, msg, &forged), "S = L is non-canonical");
        // A public key with y >= p must be rejected: 2^255-19 encoded
        // with the sign bit clear is p itself, non-canonical.
        let mut bad_pk = pk;
        bad_pk.copy_from_slice(&[
            0xed, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0x7f,
        ]);
        assert!(!verify(&bad_pk, msg, &sig), "y = p is non-canonical");
    }

    #[test]
    fn signing_is_deterministic_and_seed_sensitive() {
        let a = sign(&[9u8; 32], b"stable");
        assert_eq!(a, sign(&[9u8; 32], b"stable"));
        assert_ne!(a, sign(&[9u8; 32], b"unstable"));
        assert_ne!(a, sign(&[10u8; 32], b"stable"));
        assert_ne!(public_key(&[1u8; 32]), public_key(&[2u8; 32]));
    }
}
