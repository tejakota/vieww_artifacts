//! The aarch64 half of the two compositing inner loops, as explicit NEON —
//! the mirror of this file's parent module's `sse2` half, under the same
//! contract: **byte-identical to the scalar loops** in `simd.rs`, which stay
//! here as the reference and the fallback.
//!
//! # Why these two, and why this mirror
//!
//! The parent module's doc records the callgrind evidence: `composite_flat`
//! and `composite_layer` are the two loops a full repaint spends its frame
//! in. The SSE2 half made them fast on x86-64; the phones this renderer
//! actually targets — the Device Farm's LG Stylo 5 and A36, every Apple
//! device — are aarch64, where the scalar fallback was still running. NEON
//! is part of the aarch64 baseline the way SSE2 is part of the x86-64 one:
//! every aarch64 CPU has it, so there is no runtime detection and nothing
//! to negotiate.
//!
//! # Why it is the same bytes
//!
//! Lane for lane, the NEON sequence is the scalar loop's sequence, never
//! fused: `scaled` is a per-channel multiply (`vmulq_f32`), `over` is
//! `src + dst * (1.0 - src.a)` with the multiply-by-one left out as an IEEE
//! identity, and every scalar branch becomes a compare-then-select in the
//! scalar loop's order of precedence (`skip`, then `copy`, then `over`).
//! The compares match the scalar ones exactly: `vcleq_f32` and `vceqq_f32`
//! answer false for a `NaN` where the scalar `<=` and `==` do, and
//! `vbslq_f32` is precisely `(mask & a) | (!mask & b)` — the SSE2 `select`'s
//! definition, lane for lane. A skipped pixel is written back with the
//! value it was loaded with, which is the same bits as not writing it.
//!
//! The one thing that can differ is *which* `NaN` a `NaN` comes out as —
//! the parent module's doc already holds the argument for why nothing can
//! observe that (a `NaN` stays `NaN` through every later blend and becomes
//! byte 0 on the way out either way), and this half inherits it.
//!
//! # Status, stated plainly
//!
//! This half is **written and type-checked, not yet run**. It type-checks
//! for `aarch64-apple-ios` and `aarch64-linux-android` in
//! `ci/check/cross-check.sh` — that is the gate that keeps it compiling —
//! but no ARM machine has executed it yet. The referee already exists:
//! `simd_rows_match_scalar_bit_for_bit` in the parent module dispatches
//! through the same entry points, so the first Android-emulator or
//! iOS-simulator run of that test is the ARM half's bit-for-bit verdict.
//! Until then this module is labelled what it is, exactly as the platform
//! bridges were.

use core::arch::aarch64::{
    float32x4_t, uint32x4_t, vaddq_f32, vandq_u32, vbslq_f32, vceqq_f32, vcleq_f32,
    vdupq_laneq_f32, vdupq_n_f32, vdupq_n_u32, vld1q_f32, vmulq_f32, vst1q_f32, vsubq_f32,
};

use super::Premul;

#[inline]
#[target_feature(enable = "neon")]
fn load(p: &Premul) -> float32x4_t {
    // SAFETY: `Premul` is `repr(C)`, four `f32`s: 16 readable bytes.
    unsafe { vld1q_f32(std::ptr::from_ref(p).cast::<f32>()) }
}

#[inline]
#[target_feature(enable = "neon")]
fn store(p: &mut Premul, v: float32x4_t) {
    // SAFETY: as `load`, 16 writable bytes.
    unsafe { vst1q_f32(std::ptr::from_mut(p).cast::<f32>(), v) }
}

/// `if mask { a } else { b }`, per lane — `vbslq_f32` *is* the SSE2
/// `select`: `(mask & a) | (!mask & b)`, with the all-ones/all-zeros masks
/// the NEON compares produce (`uint32x4_t`, unlike the SSE2 compares'
/// `__m128` — the mask type is native here, no reinterpreting).
#[inline]
#[target_feature(enable = "neon")]
fn select(mask: uint32x4_t, a: float32x4_t, b: float32x4_t) -> float32x4_t {
    vbslq_f32(mask, a, b)
}

/// The conjunction of two compare masks — SSE2's `_mm_and_ps` on two
/// all-ones/all-zeros words, which is a bitwise and of the two masks.
#[inline]
#[target_feature(enable = "neon")]
fn both(a: uint32x4_t, b: uint32x4_t) -> uint32x4_t {
    vandq_u32(a, b)
}

/// A mask of all ones when `flag`, else all zeros.
#[inline]
#[target_feature(enable = "neon")]
fn flag(flag: bool) -> uint32x4_t {
    vdupq_n_u32((-i32::from(flag)) as u32)
}

/// `over(src, dst)`: `src * 1.0 + dst * (1.0 - src.a)`, where the
/// multiply by one is an identity and is left out.
#[inline]
#[target_feature(enable = "neon")]
fn over(s: float32x4_t, a: float32x4_t, d: float32x4_t) -> float32x4_t {
    vaddq_f32(s, vmulq_f32(d, vsubq_f32(vdupq_n_f32(1.0), a)))
}

#[target_feature(enable = "neon")]
pub(super) fn over_layer_row(dst: &mut [Premul], src: &[Premul], alpha: f32, opaque_copy: bool) {
    // Two loops rather than a per-pixel test of `alpha == 1.0`: the scalar
    // path skips the multiply at exactly one, and `x * 1.0` is an identity
    // anyway, so either loop gives the same bits — this one just does not
    // spend the multiply.
    if alpha == 1.0 {
        layer_loop::<false>(dst, src, vdupq_n_f32(1.0), opaque_copy);
    } else {
        layer_loop::<true>(dst, src, vdupq_n_f32(alpha), opaque_copy);
    }
}

#[target_feature(enable = "neon")]
pub(super) fn over_layer_row_take(
    dst: &mut [Premul],
    src: &mut [Premul],
    alpha: f32,
    opaque_copy: bool,
) {
    let zero = vdupq_n_f32(0.0);
    let one = vdupq_n_f32(1.0);
    let copy_ok = flag(opaque_copy);
    let k = vdupq_n_f32(alpha);
    let scale = alpha != 1.0;
    for (px, sp) in dst.iter_mut().zip(src) {
        let s = load(sp);
        store(sp, zero);
        let s = if scale { vmulq_f32(s, k) } else { s };
        let a = vdupq_laneq_f32::<3>(s);
        let d = load(px);
        let skip = vcleq_f32(a, zero);
        let copy = both(vceqq_f32(a, one), copy_ok);
        store(px, select(skip, d, select(copy, s, over(s, a, d))));
    }
}

#[inline]
#[target_feature(enable = "neon")]
fn layer_loop<const SCALE: bool>(
    dst: &mut [Premul],
    src: &[Premul],
    k: float32x4_t,
    opaque_copy: bool,
) {
    let zero = vdupq_n_f32(0.0);
    let one = vdupq_n_f32(1.0);
    let copy_ok = flag(opaque_copy);
    for (px, s) in dst.iter_mut().zip(src) {
        let s = if SCALE {
            vmulq_f32(load(s), k)
        } else {
            load(s)
        };
        let a = vdupq_laneq_f32::<3>(s);
        let d = load(px);
        // Precedence as in the scalar loop: skip, then copy, then over.
        let skip = vcleq_f32(a, zero);
        let copy = both(vceqq_f32(a, one), copy_ok);
        let out = select(skip, d, select(copy, s, over(s, a, d)));
        store(px, out);
    }
}

#[target_feature(enable = "neon")]
pub(super) fn over_flat_row(dst: &mut [Premul], mask: &[f32], color: Premul, replaces: bool) {
    let one = vdupq_n_f32(1.0);
    let col = load(&color);
    let replace_all = replaces;
    for (px, &c) in dst.iter_mut().zip(mask) {
        // **The two cheap cases stay branches.** No coverage, and an
        // opaque colour at full coverage, are long runs — outside a shape,
        // and the interior of every solid fill — so the branches predict
        // almost perfectly and each costs a compare and at most a store.
        // Folding them into the selects below, as `layer_loop` does,
        // measured worse: `23-editor-glass` (mostly opaque panels) went
        // from 1,301 M to 1,431 M instructions, because every interior
        // pixel then paid the full blend to compute a value it discarded.
        // The measurement is the SSE2 half's, on x86; the branch profile it
        // describes is content-shaped, not instruction-set-shaped, so it
        // carries — and the ARM bit-equality test reads the same branches.
        if c <= 0.0 {
            continue;
        }
        if replace_all && c >= 1.0 {
            *px = color;
            continue;
        }
        let cv = vdupq_n_f32(c);
        let s = vmulq_f32(col, cv);
        let a = vdupq_laneq_f32::<3>(s);
        let d = load(px);
        // An opaque scaled source replaces; anything else is `over`.
        let copy = vceqq_f32(a, one);
        store(px, select(copy, s, over(s, a, d)));
    }
}
