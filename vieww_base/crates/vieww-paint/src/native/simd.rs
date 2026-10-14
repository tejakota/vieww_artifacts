//! The two compositing inner loops that dominate a full repaint, as explicit
//! SSE2 on x86-64 — **byte-identical** to the scalar loops they replace,
//! which stay here as the reference and the fallback.
//!
//! # Why these two
//!
//! `Target::composite_flat` (a solid colour through a coverage mask — every
//! fill, stroke and glyph) and `Target::composite_layer` (a resolved layer
//! onto its parent) are the two loops a callgrind profile of
//! `examples/fixtures`' over-budget screens puts at the top: `00-layers-nested`
//! spends about half its frame in them, `23-editor-glass` about a fifth. Both
//! run the normal-blend, gamma-space path almost exclusively, and that path is
//! what is vectorised here; every other blend mode and pipeline still goes
//! through `blend_with_pipeline`, unchanged.
//!
//! The scalar loops spent most of their instructions *around* the
//! arithmetic — iterator bounds checks, four scalar channels, and a chain of
//! per-pixel branches — not in it. A `Premul` is exactly one 128-bit
//! register, so here each pixel is one load, one multiply, the `over`
//! arithmetic, and the branches become lane selects.
//!
//! # Why it is the same bytes
//!
//! Every branch of the scalar loop becomes a mask, evaluated under the same
//! comparison (`<=`, `>=`, `==`, each false for `NaN` exactly as in scalar
//! code), and the nested selects apply them in the scalar loop's order of
//! precedence. Every arithmetic step is the scalar one in the same order and
//! never fused: `scaled` is a per-channel multiply; `over` is
//! `porter_duff(src, dst, 1.0, 1.0 - src.a)`, i.e. `src * 1.0 + dst * fb`,
//! and `src * 1.0` is an IEEE identity, so it is `src + dst * fb`. A skipped
//! pixel is written back with the value it was loaded with, which is the same
//! bits as not writing it.
//!
//! The one thing that can differ is which `NaN` a `NaN` comes out as. x86
//! propagates the first operand's `NaN`, and which operand is first is the
//! compiler's choice in the scalar loop as much as here (`src * k` and
//! `k * src` are the same multiply to LLVM), so a `NaN` channel may come out
//! with the other sign bit. Nothing can observe it: a `NaN` stays `NaN`
//! through every later blend and converts to byte 0 on the way out either
//! way. A `NaN` only reaches these loops from an already-invalid input in the
//! first place (infinite alpha, say).
//!
//! `simd_rows_match_scalar_bit_for_bit` below holds both functions to their
//! scalar reference over hostile and random inputs, compared as raw bits with
//! every `NaN` treated as one value.

use super::color::{over, Premul};

/// `composite_layer`'s normal-blend, gamma-space row: `src` (scaled by the
/// layer's group `alpha` unless it is exactly 1) over `dst`, pixel for pixel.
pub(crate) fn over_layer_row(dst: &mut [Premul], src: &[Premul], alpha: f32, opaque_copy: bool) {
    debug_assert_eq!(dst.len(), src.len());
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: SSE2 is part of the x86-64 baseline; see `sse2`'s doc.
        unsafe { sse2::over_layer_row(dst, src, alpha, opaque_copy) }
    }
    #[cfg(not(target_arch = "x86_64"))]
    over_layer_row_scalar(dst, src, alpha, opaque_copy);
}

/// [`over_layer_row`], leaving `src` fully transparent: each source pixel is
/// zeroed in the same pass that reads it. See
/// `Target::composite_layer_and_clear`.
pub(crate) fn over_layer_row_take(
    dst: &mut [Premul],
    src: &mut [Premul],
    alpha: f32,
    opaque_copy: bool,
) {
    debug_assert_eq!(dst.len(), src.len());
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: SSE2 is part of the x86-64 baseline; see `sse2`'s doc.
        unsafe { sse2::over_layer_row_take(dst, src, alpha, opaque_copy) }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        over_layer_row_scalar(dst, src, alpha, opaque_copy);
        src.fill(Premul::TRANSPARENT);
    }
}

/// `composite_flat`'s normal-blend, gamma-space row: `color` through the
/// coverage `mask` over `dst`.
pub(crate) fn over_flat_row(dst: &mut [Premul], mask: &[f32], color: Premul, replaces: bool) {
    debug_assert_eq!(dst.len(), mask.len());
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: SSE2 is part of the x86-64 baseline; see `sse2`'s doc.
        unsafe { sse2::over_flat_row(dst, mask, color, replaces) }
    }
    #[cfg(not(target_arch = "x86_64"))]
    over_flat_row_scalar(dst, mask, color, replaces);
}

/// The scalar loop `composite_layer` ran for the normal blend, verbatim.
#[cfg_attr(target_arch = "x86_64", allow(dead_code))]
pub(crate) fn over_layer_row_scalar(
    dst: &mut [Premul],
    src: &[Premul],
    alpha: f32,
    opaque_copy: bool,
) {
    for (px, &src) in dst.iter_mut().zip(src) {
        let src = if alpha == 1.0 { src } else { src.scaled(alpha) };
        if src.a <= 0.0 {
            continue;
        }
        if opaque_copy && src.a == 1.0 {
            *px = src;
            continue;
        }
        *px = over(src, *px);
    }
}

/// The scalar loop `composite_flat` ran for the normal blend, verbatim.
#[cfg_attr(target_arch = "x86_64", allow(dead_code))]
pub(crate) fn over_flat_row_scalar(
    dst: &mut [Premul],
    mask: &[f32],
    color: Premul,
    replaces: bool,
) {
    for (px, &c) in dst.iter_mut().zip(mask) {
        if c <= 0.0 {
            continue;
        }
        if replaces && c >= 1.0 {
            *px = color;
            continue;
        }
        let src = color.scaled(c);
        if src.a == 1.0 {
            *px = src;
            continue;
        }
        *px = over(src, *px);
    }
}

/// # Why no runtime detection
///
/// SSE2 is part of the x86-64 baseline: every x86-64 CPU has it and rustc
/// enables it for every x86-64 target. The `target_feature` attribute is
/// there because Rust asks for it before treating the intrinsics as safe, not
/// because any machine could lack it.
#[cfg(target_arch = "x86_64")]
mod sse2 {
    use core::arch::x86_64::{
        __m128, _mm_add_ps, _mm_and_ps, _mm_andnot_ps, _mm_castsi128_ps, _mm_cmpeq_ps,
        _mm_cmple_ps, _mm_loadu_ps, _mm_mul_ps, _mm_or_ps, _mm_set1_epi32, _mm_set1_ps,
        _mm_setzero_ps, _mm_shuffle_ps, _mm_storeu_ps, _mm_sub_ps,
    };

    use super::Premul;

    #[inline]
    #[target_feature(enable = "sse2")]
    fn load(p: &Premul) -> __m128 {
        // SAFETY: `Premul` is `repr(C)`, four `f32`s: 16 readable bytes.
        unsafe { _mm_loadu_ps(std::ptr::from_ref(p).cast::<f32>()) }
    }

    #[inline]
    #[target_feature(enable = "sse2")]
    fn store(p: &mut Premul, v: __m128) {
        // SAFETY: as `load`, 16 writable bytes.
        unsafe { _mm_storeu_ps(std::ptr::from_mut(p).cast::<f32>(), v) }
    }

    /// `if mask { a } else { b }`, per lane.
    #[inline]
    #[target_feature(enable = "sse2")]
    fn select(mask: __m128, a: __m128, b: __m128) -> __m128 {
        _mm_or_ps(_mm_and_ps(mask, a), _mm_andnot_ps(mask, b))
    }

    /// A mask of all ones when `flag`, else all zeros.
    #[inline]
    #[target_feature(enable = "sse2")]
    fn flag(flag: bool) -> __m128 {
        _mm_castsi128_ps(_mm_set1_epi32(-i32::from(flag)))
    }

    /// `over(src, dst)`: `src * 1.0 + dst * (1.0 - src.a)`, where the
    /// multiply by one is an identity and is left out.
    #[inline]
    #[target_feature(enable = "sse2")]
    fn over(s: __m128, a: __m128, d: __m128) -> __m128 {
        _mm_add_ps(s, _mm_mul_ps(d, _mm_sub_ps(_mm_set1_ps(1.0), a)))
    }

    #[target_feature(enable = "sse2")]
    pub(super) fn over_layer_row(
        dst: &mut [Premul],
        src: &[Premul],
        alpha: f32,
        opaque_copy: bool,
    ) {
        // Two loops rather than a per-pixel test of `alpha == 1.0`: the scalar
        // path skips the multiply at exactly one, and `x * 1.0` is an identity
        // anyway, so either loop gives the same bits — this one just does not
        // spend the multiply.
        if alpha == 1.0 {
            layer_loop::<false>(dst, src, _mm_set1_ps(1.0), opaque_copy);
        } else {
            layer_loop::<true>(dst, src, _mm_set1_ps(alpha), opaque_copy);
        }
    }

    #[target_feature(enable = "sse2")]
    pub(super) fn over_layer_row_take(
        dst: &mut [Premul],
        src: &mut [Premul],
        alpha: f32,
        opaque_copy: bool,
    ) {
        let zero = _mm_setzero_ps();
        let one = _mm_set1_ps(1.0);
        let copy_ok = flag(opaque_copy);
        let k = _mm_set1_ps(alpha);
        let scale = alpha != 1.0;
        for (px, sp) in dst.iter_mut().zip(src) {
            let s = load(sp);
            store(sp, zero);
            let s = if scale { _mm_mul_ps(s, k) } else { s };
            let a = _mm_shuffle_ps::<0xFF>(s, s);
            let d = load(px);
            let skip = _mm_cmple_ps(a, zero);
            let copy = _mm_and_ps(_mm_cmpeq_ps(a, one), copy_ok);
            store(px, select(skip, d, select(copy, s, over(s, a, d))));
        }
    }

    #[inline]
    #[target_feature(enable = "sse2")]
    fn layer_loop<const SCALE: bool>(
        dst: &mut [Premul],
        src: &[Premul],
        k: __m128,
        opaque_copy: bool,
    ) {
        let zero = _mm_setzero_ps();
        let one = _mm_set1_ps(1.0);
        let copy_ok = flag(opaque_copy);
        for (px, s) in dst.iter_mut().zip(src) {
            let s = if SCALE {
                _mm_mul_ps(load(s), k)
            } else {
                load(s)
            };
            let a = _mm_shuffle_ps::<0xFF>(s, s);
            let d = load(px);
            // Precedence as in the scalar loop: skip, then copy, then over.
            let skip = _mm_cmple_ps(a, zero);
            let copy = _mm_and_ps(_mm_cmpeq_ps(a, one), copy_ok);
            let out = select(skip, d, select(copy, s, over(s, a, d)));
            store(px, out);
        }
    }

    #[target_feature(enable = "sse2")]
    pub(super) fn over_flat_row(dst: &mut [Premul], mask: &[f32], color: Premul, replaces: bool) {
        let one = _mm_set1_ps(1.0);
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
            if c <= 0.0 {
                continue;
            }
            if replace_all && c >= 1.0 {
                *px = color;
                continue;
            }
            let cv = _mm_set1_ps(c);
            let s = _mm_mul_ps(col, cv);
            let a = _mm_shuffle_ps::<0xFF>(s, s);
            let d = load(px);
            // An opaque scaled source replaces; anything else is `over`.
            let copy = _mm_cmpeq_ps(a, one);
            store(px, select(copy, s, over(s, a, d)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Raw bits, except that every `NaN` is one `NaN` — see the module doc's
    /// note on `NaN` sign and payload.
    fn bits(row: &[Premul]) -> Vec<[u32; 4]> {
        let canon = |v: f32| {
            if v.is_nan() {
                f32::NAN.to_bits()
            } else {
                v.to_bits()
            }
        };
        row.iter()
            .map(|p| [canon(p.r), canon(p.g), canon(p.b), canon(p.a)])
            .collect()
    }

    const EDGES: [f32; 16] = [
        0.0,
        -0.0,
        1.0,
        0.5,
        0.97,
        1.0 / 255.0,
        0.999_999,
        1.000_001,
        1.5,
        -0.25,
        f32::MIN_POSITIVE / 8.0,
        f32::EPSILON,
        f32::MAX,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
    ];

    /// xorshift, so the sweep is deterministic and needs no dependency.
    struct Rng(u64);
    impl Rng {
        fn f(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            // Biased toward the values the branches test: 0, 1, and between.
            match self.0 % 8 {
                0 => 0.0,
                1 => 1.0,
                _ => (self.0 >> 40) as f32 / (1u64 << 24) as f32 * 1.3 - 0.15,
            }
        }
        fn premul(&mut self) -> Premul {
            let a = self.f();
            Premul {
                r: self.f() * a.abs(),
                g: self.f() * a.abs(),
                b: self.f() * a.abs(),
                a,
            }
        }
    }

    fn pixels(rng: &mut Rng, n: usize) -> Vec<Premul> {
        let mut out: Vec<Premul> = EDGES
            .iter()
            .flat_map(|&a| {
                EDGES.iter().map(move |&c| Premul {
                    r: c,
                    g: c * 0.5,
                    b: 1.0 - c,
                    a,
                })
            })
            .collect();
        out.extend((0..n).map(|_| rng.premul()));
        out
    }

    #[test]
    fn simd_rows_match_scalar_bit_for_bit() {
        let mut rng = Rng(0x2545_F491_4F6C_DD1D);
        let dst = pixels(&mut rng, 200_000);
        let src = pixels(&mut rng, 200_000);
        let mask: Vec<f32> = EDGES
            .iter()
            .copied()
            .cycle()
            .take(EDGES.len() * EDGES.len())
            .chain((0..200_000).map(|_| rng.f()))
            .collect();
        assert_eq!(mask.len(), dst.len());

        for alpha in [1.0, 0.97, 0.5, 0.0, 1.5, f32::NAN] {
            for opaque_copy in [true, false] {
                let (mut a, mut b) = (dst.clone(), dst.clone());
                over_layer_row(&mut a, &src, alpha, opaque_copy);
                over_layer_row_scalar(&mut b, &src, alpha, opaque_copy);
                assert!(
                    bits(&a) == bits(&b),
                    "layer row: alpha {alpha}, opaque_copy {opaque_copy}"
                );
            }
        }
        let colors = [
            Premul {
                r: 0.2,
                g: 0.4,
                b: 0.6,
                a: 1.0,
            },
            Premul {
                r: 0.1,
                g: 0.05,
                b: 0.0,
                a: 0.25,
            },
            Premul {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            },
            Premul {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.078,
            },
        ];
        for color in colors {
            for replaces in [true, false] {
                let (mut a, mut b) = (dst.clone(), dst.clone());
                over_flat_row(&mut a, &mask, color, replaces);
                over_flat_row_scalar(&mut b, &mask, color, replaces);
                assert!(
                    bits(&a) == bits(&b),
                    "flat row: {color:?}, replaces {replaces}"
                );
            }
        }
    }
}
