//! SIMD kernels for the inner loops, chosen at run time.
//!
//! Every kernel exists three ways: a portable scalar version (the
//! reference, written with independent accumulators so the compiler can
//! vectorise it for whatever it targets), an x86-64 version (SSE2 — part
//! of the x86-64 baseline, so always present — and AVX when the CPU
//! reports it), and an AArch64 NEON version (part of the AArch64
//! baseline). [`level`] reports which one runs; the tests run every
//! available path against the scalar one on every remainder length, so
//! a path that drifts from the reference fails here, not in a mix.
//!
//! The kernels are what the audio paths spend their time in: FIR dot
//! products, mixing (`dst += src · gain`), gain, peak and energy scans,
//! sample-format conversion and stereo (de)interleaving.

/// The instruction set the kernels use on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Portable code.
    Scalar,
    /// x86-64 SSE2.
    Sse2,
    /// x86-64 AVX.
    Avx,
    /// AArch64 NEON.
    Neon,
}

/// The level in use.
#[must_use]
pub fn level() -> Level {
    #[cfg(target_arch = "x86_64")]
    {
        if std::arch::is_x86_feature_detected!("avx") {
            Level::Avx
        } else {
            Level::Sse2
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        Level::Neon
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        Level::Scalar
    }
}

/// `Σ a[i]·b[i]` over the shorter length.
#[must_use]
pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    let (a, b) = (&a[..n], &b[..n]);
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: AVX was detected; SSE2 is the x86-64 baseline.
        if std::arch::is_x86_feature_detected!("avx") {
            return unsafe { x86::dot_avx(a, b) };
        }
        unsafe { x86::dot_sse2(a, b) }
    }
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: NEON is the AArch64 baseline.
        unsafe { neon::dot(a, b) }
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        scalar::dot(a, b)
    }
}

/// `dst[i] += src[i]·gain` over the shorter length.
pub fn add_scaled(dst: &mut [f32], src: &[f32], gain: f32) {
    let n = dst.len().min(src.len());
    let (dst, src) = (&mut dst[..n], &src[..n]);
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: as in `dot`.
        if std::arch::is_x86_feature_detected!("avx") {
            return unsafe { x86::add_scaled_avx(dst, src, gain) };
        }
        unsafe { x86::add_scaled_sse2(dst, src, gain) }
    }
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: as in `dot`.
        unsafe { neon::add_scaled(dst, src, gain) }
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        scalar::add_scaled(dst, src, gain);
    }
}

/// `buf[i] *= gain`.
pub fn scale(buf: &mut [f32], gain: f32) {
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: as in `dot`.
        if std::arch::is_x86_feature_detected!("avx") {
            return unsafe { x86::scale_avx(buf, gain) };
        }
        unsafe { x86::scale_sse2(buf, gain) }
    }
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: as in `dot`.
        unsafe { neon::scale(buf, gain) }
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        scalar::scale(buf, gain);
    }
}

/// The largest `|x|` (0 for an empty slice; NaNs ignored).
#[must_use]
pub fn peak(x: &[f32]) -> f32 {
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: SSE2 is the x86-64 baseline.
        unsafe { x86::peak_sse2(x) }
    }
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: as in `dot`.
        unsafe { neon::peak(x) }
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        scalar::peak(x)
    }
}

/// `Σ x[i]²`, accumulated in f64 per block for accuracy.
#[must_use]
pub fn sum_squares(x: &[f32]) -> f64 {
    // Blocks of 4096 in f32 lanes, summed in f64: error stays at f32
    // rounding of a 4096-term sum, not of the whole buffer.
    x.chunks(4_096).map(|c| f64::from(dot(c, c))).sum()
}

/// Float → 16-bit PCM: clamp to ±1, ×32767, round to nearest (ties to
/// even), NaN → 0.
pub fn f32_to_i16(src: &[f32], dst: &mut [i16]) {
    let n = src.len().min(dst.len());
    let (src, dst) = (&src[..n], &mut dst[..n]);
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: SSE2 is the x86-64 baseline.
        unsafe { x86::f32_to_i16_sse2(src, dst) }
    }
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: as in `dot`.
        unsafe { neon::f32_to_i16(src, dst) }
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        scalar::f32_to_i16(src, dst);
    }
}

/// 16-bit PCM → float (÷32768).
pub fn i16_to_f32(src: &[i16], dst: &mut [f32]) {
    let n = src.len().min(dst.len());
    let (src, dst) = (&src[..n], &mut dst[..n]);
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: SSE2 is the x86-64 baseline.
        unsafe { x86::i16_to_f32_sse2(src, dst) }
    }
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: as in `dot`.
        unsafe { neon::i16_to_f32(src, dst) }
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        scalar::i16_to_f32(src, dst);
    }
}

/// Planar left/right → interleaved stereo (`out.len() ≥ 2·frames`).
pub fn interleave(l: &[f32], r: &[f32], out: &mut [f32]) {
    let n = l.len().min(r.len()).min(out.len() / 2);
    let (l, r, out) = (&l[..n], &r[..n], &mut out[..n * 2]);
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: SSE2 is the x86-64 baseline.
        unsafe { x86::interleave_sse2(l, r, out) }
    }
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: as in `dot`.
        unsafe { neon::interleave(l, r, out) }
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        scalar::interleave(l, r, out);
    }
}

/// Interleaved stereo → planar left/right.
pub fn deinterleave(input: &[f32], l: &mut [f32], r: &mut [f32]) {
    let n = l.len().min(r.len()).min(input.len() / 2);
    let (input, l, r) = (&input[..n * 2], &mut l[..n], &mut r[..n]);
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: SSE2 is the x86-64 baseline.
        unsafe { x86::deinterleave_sse2(input, l, r) }
    }
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: as in `dot`.
        unsafe { neon::deinterleave(input, l, r) }
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        scalar::deinterleave(input, l, r);
    }
}

/// The portable reference implementations.
#[allow(
    dead_code,
    reason = "the reference on every target; the fallback on some"
)]
mod scalar {
    pub(super) fn dot(a: &[f32], b: &[f32]) -> f32 {
        let mut acc = [0.0f32; 8];
        for (x, y) in a.as_chunks::<8>().0.iter().zip(b.as_chunks::<8>().0) {
            for k in 0..8 {
                acc[k] += x[k] * y[k];
            }
        }
        let head = a.len() - a.len() % 8;
        let tail: f32 = a[head..].iter().zip(&b[head..]).map(|(x, y)| x * y).sum();
        acc.iter().sum::<f32>() + tail
    }

    pub(super) fn add_scaled(dst: &mut [f32], src: &[f32], gain: f32) {
        for (d, s) in dst.iter_mut().zip(src) {
            *d += s * gain;
        }
    }

    pub(super) fn scale(buf: &mut [f32], gain: f32) {
        for v in buf {
            *v *= gain;
        }
    }

    pub(super) fn peak(x: &[f32]) -> f32 {
        x.iter()
            .fold(0.0f32, |m, v| if v.abs() > m { v.abs() } else { m })
    }

    pub(super) fn one_to_i16(v: f32) -> i16 {
        let v = if v.is_nan() { 0.0 } else { v.clamp(-1.0, 1.0) };
        #[expect(clippy::cast_possible_truncation, reason = "clamped to ±32767")]
        let q = (v * 32_767.0).round_ties_even() as i16;
        q
    }

    pub(super) fn f32_to_i16(src: &[f32], dst: &mut [i16]) {
        for (d, s) in dst.iter_mut().zip(src) {
            *d = one_to_i16(*s);
        }
    }

    pub(super) fn i16_to_f32(src: &[i16], dst: &mut [f32]) {
        for (d, s) in dst.iter_mut().zip(src) {
            *d = f32::from(*s) / 32_768.0;
        }
    }

    pub(super) fn interleave(l: &[f32], r: &[f32], out: &mut [f32]) {
        for (i, (a, b)) in l.iter().zip(r).enumerate() {
            out[2 * i] = *a;
            out[2 * i + 1] = *b;
        }
    }

    pub(super) fn deinterleave(input: &[f32], l: &mut [f32], r: &mut [f32]) {
        for (i, f) in input.as_chunks::<2>().0.iter().enumerate() {
            l[i] = f[0];
            r[i] = f[1];
        }
    }
}

#[cfg(target_arch = "x86_64")]
mod x86 {
    use super::scalar;
    use std::arch::x86_64::{
        __m128, __m256, _mm256_add_ps, _mm256_castps256_ps128, _mm256_extractf128_ps,
        _mm256_loadu_ps, _mm256_mul_ps, _mm256_set1_ps, _mm256_setzero_ps, _mm256_storeu_ps,
        _mm_add_ps, _mm_and_ps, _mm_castsi128_ps, _mm_cmpeq_ps, _mm_cvtepi32_ps, _mm_cvtps_epi32,
        _mm_cvtss_f32, _mm_loadu_ps, _mm_loadu_si128, _mm_max_ps, _mm_min_ps, _mm_movehl_ps,
        _mm_mul_ps, _mm_packs_epi32, _mm_set1_epi32, _mm_set1_ps, _mm_setzero_ps, _mm_shuffle_ps,
        _mm_srai_epi32, _mm_storeu_ps, _mm_storeu_si128, _mm_unpackhi_epi16, _mm_unpackhi_ps,
        _mm_unpacklo_epi16, _mm_unpacklo_ps,
    };

    #[inline]
    #[target_feature(enable = "sse2")]
    unsafe fn hsum(v: __m128) -> f32 {
        let hi = _mm_movehl_ps(v, v);
        let s = _mm_add_ps(v, hi);
        let s2 = _mm_shuffle_ps::<0b01>(s, s);
        _mm_cvtss_f32(_mm_add_ps(s, s2))
    }

    #[target_feature(enable = "sse2")]
    pub(super) unsafe fn dot_sse2(a: &[f32], b: &[f32]) -> f32 {
        let n = a.len();
        let (mut s0, mut s1) = (_mm_setzero_ps(), _mm_setzero_ps());
        let mut i = 0;
        while i + 8 <= n {
            s0 = _mm_add_ps(
                s0,
                _mm_mul_ps(
                    _mm_loadu_ps(a.as_ptr().add(i)),
                    _mm_loadu_ps(b.as_ptr().add(i)),
                ),
            );
            s1 = _mm_add_ps(
                s1,
                _mm_mul_ps(
                    _mm_loadu_ps(a.as_ptr().add(i + 4)),
                    _mm_loadu_ps(b.as_ptr().add(i + 4)),
                ),
            );
            i += 8;
        }
        hsum(_mm_add_ps(s0, s1)) + scalar::dot(&a[i..], &b[i..])
    }

    #[target_feature(enable = "avx")]
    pub(super) unsafe fn dot_avx(a: &[f32], b: &[f32]) -> f32 {
        let n = a.len();
        let (mut s0, mut s1): (__m256, __m256) = (_mm256_setzero_ps(), _mm256_setzero_ps());
        let mut i = 0;
        while i + 16 <= n {
            s0 = _mm256_add_ps(
                s0,
                _mm256_mul_ps(
                    _mm256_loadu_ps(a.as_ptr().add(i)),
                    _mm256_loadu_ps(b.as_ptr().add(i)),
                ),
            );
            s1 = _mm256_add_ps(
                s1,
                _mm256_mul_ps(
                    _mm256_loadu_ps(a.as_ptr().add(i + 8)),
                    _mm256_loadu_ps(b.as_ptr().add(i + 8)),
                ),
            );
            i += 16;
        }
        let s = _mm256_add_ps(s0, s1);
        let q = _mm_add_ps(_mm256_castps256_ps128(s), _mm256_extractf128_ps::<1>(s));
        hsum(q) + scalar::dot(&a[i..], &b[i..])
    }

    #[target_feature(enable = "sse2")]
    pub(super) unsafe fn add_scaled_sse2(dst: &mut [f32], src: &[f32], gain: f32) {
        let n = dst.len();
        let g = _mm_set1_ps(gain);
        let mut i = 0;
        while i + 4 <= n {
            let d = dst.as_mut_ptr().add(i);
            _mm_storeu_ps(
                d,
                _mm_add_ps(
                    _mm_loadu_ps(d),
                    _mm_mul_ps(_mm_loadu_ps(src.as_ptr().add(i)), g),
                ),
            );
            i += 4;
        }
        scalar::add_scaled(&mut dst[i..], &src[i..], gain);
    }

    #[target_feature(enable = "avx")]
    pub(super) unsafe fn add_scaled_avx(dst: &mut [f32], src: &[f32], gain: f32) {
        let n = dst.len();
        let g = _mm256_set1_ps(gain);
        let mut i = 0;
        while i + 8 <= n {
            let d = dst.as_mut_ptr().add(i);
            _mm256_storeu_ps(
                d,
                _mm256_add_ps(
                    _mm256_loadu_ps(d),
                    _mm256_mul_ps(_mm256_loadu_ps(src.as_ptr().add(i)), g),
                ),
            );
            i += 8;
        }
        scalar::add_scaled(&mut dst[i..], &src[i..], gain);
    }

    #[target_feature(enable = "sse2")]
    pub(super) unsafe fn scale_sse2(buf: &mut [f32], gain: f32) {
        let n = buf.len();
        let g = _mm_set1_ps(gain);
        let mut i = 0;
        while i + 4 <= n {
            let p = buf.as_mut_ptr().add(i);
            _mm_storeu_ps(p, _mm_mul_ps(_mm_loadu_ps(p), g));
            i += 4;
        }
        scalar::scale(&mut buf[i..], gain);
    }

    #[target_feature(enable = "avx")]
    pub(super) unsafe fn scale_avx(buf: &mut [f32], gain: f32) {
        let n = buf.len();
        let g = _mm256_set1_ps(gain);
        let mut i = 0;
        while i + 8 <= n {
            let p = buf.as_mut_ptr().add(i);
            _mm256_storeu_ps(p, _mm256_mul_ps(_mm256_loadu_ps(p), g));
            i += 8;
        }
        scalar::scale(&mut buf[i..], gain);
    }

    #[target_feature(enable = "sse2")]
    pub(super) unsafe fn peak_sse2(x: &[f32]) -> f32 {
        let n = x.len();
        let mask = _mm_castsi128_ps(_mm_set1_epi32(0x7FFF_FFFF));
        let mut m = _mm_setzero_ps();
        let mut i = 0;
        while i + 4 <= n {
            // maxps returns its second operand when either is NaN, so the
            // running maximum goes second and NaNs never replace it.
            let v = _mm_and_ps(_mm_loadu_ps(x.as_ptr().add(i)), mask);
            m = _mm_max_ps(v, m);
            i += 4;
        }
        let hi = _mm_movehl_ps(m, m);
        let m2 = _mm_max_ps(m, hi);
        let m3 = _mm_max_ps(m2, _mm_shuffle_ps::<0b01>(m2, m2));
        _mm_cvtss_f32(m3).max(scalar::peak(&x[i..]))
    }

    #[target_feature(enable = "sse2")]
    pub(super) unsafe fn f32_to_i16_sse2(src: &[f32], dst: &mut [i16]) {
        let n = src.len();
        let (lo, hi, k) = (_mm_set1_ps(-1.0), _mm_set1_ps(1.0), _mm_set1_ps(32_767.0));
        let conv = |v: __m128| {
            let v = _mm_and_ps(v, _mm_cmpeq_ps(v, v)); // NaN → 0
            _mm_cvtps_epi32(_mm_mul_ps(_mm_min_ps(_mm_max_ps(v, lo), hi), k))
        };
        let mut i = 0;
        while i + 8 <= n {
            let a = conv(_mm_loadu_ps(src.as_ptr().add(i)));
            let b = conv(_mm_loadu_ps(src.as_ptr().add(i + 4)));
            _mm_storeu_si128(dst.as_mut_ptr().add(i).cast(), _mm_packs_epi32(a, b));
            i += 8;
        }
        scalar::f32_to_i16(&src[i..], &mut dst[i..]);
    }

    #[target_feature(enable = "sse2")]
    pub(super) unsafe fn i16_to_f32_sse2(src: &[i16], dst: &mut [f32]) {
        let n = src.len();
        let k = _mm_set1_ps(1.0 / 32_768.0);
        let mut i = 0;
        while i + 8 <= n {
            let v = _mm_loadu_si128(src.as_ptr().add(i).cast());
            // Sign-extend by unpacking into the high halves, then shifting.
            let lo = _mm_srai_epi32::<16>(_mm_unpacklo_epi16(v, v));
            let hi = _mm_srai_epi32::<16>(_mm_unpackhi_epi16(v, v));
            _mm_storeu_ps(dst.as_mut_ptr().add(i), _mm_mul_ps(_mm_cvtepi32_ps(lo), k));
            _mm_storeu_ps(
                dst.as_mut_ptr().add(i + 4),
                _mm_mul_ps(_mm_cvtepi32_ps(hi), k),
            );
            i += 8;
        }
        scalar::i16_to_f32(&src[i..], &mut dst[i..]);
    }

    #[target_feature(enable = "sse2")]
    pub(super) unsafe fn interleave_sse2(l: &[f32], r: &[f32], out: &mut [f32]) {
        let n = l.len();
        let mut i = 0;
        while i + 4 <= n {
            let a = _mm_loadu_ps(l.as_ptr().add(i));
            let b = _mm_loadu_ps(r.as_ptr().add(i));
            _mm_storeu_ps(out.as_mut_ptr().add(2 * i), _mm_unpacklo_ps(a, b));
            _mm_storeu_ps(out.as_mut_ptr().add(2 * i + 4), _mm_unpackhi_ps(a, b));
            i += 4;
        }
        scalar::interleave(&l[i..], &r[i..], &mut out[2 * i..]);
    }

    #[target_feature(enable = "sse2")]
    pub(super) unsafe fn deinterleave_sse2(input: &[f32], l: &mut [f32], r: &mut [f32]) {
        let n = l.len();
        let mut i = 0;
        while i + 4 <= n {
            let a = _mm_loadu_ps(input.as_ptr().add(2 * i));
            let b = _mm_loadu_ps(input.as_ptr().add(2 * i + 4));
            _mm_storeu_ps(l.as_mut_ptr().add(i), _mm_shuffle_ps::<0b10_00_10_00>(a, b));
            _mm_storeu_ps(r.as_mut_ptr().add(i), _mm_shuffle_ps::<0b11_01_11_01>(a, b));
            i += 4;
        }
        scalar::deinterleave(&input[2 * i..], &mut l[i..], &mut r[i..]);
    }
}

#[cfg(target_arch = "aarch64")]
mod neon {
    use super::scalar;
    use std::arch::aarch64::{
        float32x4x2_t, vabsq_f32, vaddq_f32, vaddvq_f32, vcombine_s16, vcvtnq_s32_f32,
        vcvtq_f32_s32, vdupq_n_f32, vget_high_s16, vget_low_s16, vld1q_f32, vld1q_s16, vld2q_f32,
        vmaxnmq_f32, vmaxnmvq_f32, vmaxq_f32, vminq_f32, vmlaq_f32, vmovl_s16, vmulq_f32,
        vqmovn_s32, vst1q_f32, vst1q_s16, vst2q_f32,
    };

    #[target_feature(enable = "neon")]
    pub(super) unsafe fn dot(a: &[f32], b: &[f32]) -> f32 {
        let n = a.len();
        let (mut s0, mut s1) = (vdupq_n_f32(0.0), vdupq_n_f32(0.0));
        let mut i = 0;
        while i + 8 <= n {
            s0 = vmlaq_f32(
                s0,
                vld1q_f32(a.as_ptr().add(i)),
                vld1q_f32(b.as_ptr().add(i)),
            );
            s1 = vmlaq_f32(
                s1,
                vld1q_f32(a.as_ptr().add(i + 4)),
                vld1q_f32(b.as_ptr().add(i + 4)),
            );
            i += 8;
        }
        vaddvq_f32(vaddq_f32(s0, s1)) + scalar::dot(&a[i..], &b[i..])
    }

    #[target_feature(enable = "neon")]
    pub(super) unsafe fn add_scaled(dst: &mut [f32], src: &[f32], gain: f32) {
        let n = dst.len();
        let g = vdupq_n_f32(gain);
        let mut i = 0;
        while i + 4 <= n {
            let d = dst.as_mut_ptr().add(i);
            vst1q_f32(
                d,
                vmlaq_f32(vld1q_f32(d), vld1q_f32(src.as_ptr().add(i)), g),
            );
            i += 4;
        }
        scalar::add_scaled(&mut dst[i..], &src[i..], gain);
    }

    #[target_feature(enable = "neon")]
    pub(super) unsafe fn scale(buf: &mut [f32], gain: f32) {
        let n = buf.len();
        let g = vdupq_n_f32(gain);
        let mut i = 0;
        while i + 4 <= n {
            let p = buf.as_mut_ptr().add(i);
            vst1q_f32(p, vmulq_f32(vld1q_f32(p), g));
            i += 4;
        }
        scalar::scale(&mut buf[i..], gain);
    }

    #[target_feature(enable = "neon")]
    pub(super) unsafe fn peak(x: &[f32]) -> f32 {
        let n = x.len();
        let mut m = vdupq_n_f32(0.0);
        let mut i = 0;
        while i + 4 <= n {
            // maxnm ignores NaN operands.
            m = vmaxnmq_f32(m, vabsq_f32(vld1q_f32(x.as_ptr().add(i))));
            i += 4;
        }
        vmaxnmvq_f32(m).max(scalar::peak(&x[i..]))
    }

    #[target_feature(enable = "neon")]
    pub(super) unsafe fn f32_to_i16(src: &[f32], dst: &mut [i16]) {
        let n = src.len();
        let (lo, hi, k) = (vdupq_n_f32(-1.0), vdupq_n_f32(1.0), vdupq_n_f32(32_767.0));
        let mut i = 0;
        while i + 8 <= n {
            // vmaxq/vminq propagate NaN; vcvtnq maps NaN to 0, as wanted.
            let a = vld1q_f32(src.as_ptr().add(i));
            let b = vld1q_f32(src.as_ptr().add(i + 4));
            let qa = vcvtnq_s32_f32(vmulq_f32(vminq_f32(vmaxq_f32(a, lo), hi), k));
            let qb = vcvtnq_s32_f32(vmulq_f32(vminq_f32(vmaxq_f32(b, lo), hi), k));
            vst1q_s16(
                dst.as_mut_ptr().add(i),
                vcombine_s16(vqmovn_s32(qa), vqmovn_s32(qb)),
            );
            i += 8;
        }
        scalar::f32_to_i16(&src[i..], &mut dst[i..]);
    }

    #[target_feature(enable = "neon")]
    pub(super) unsafe fn i16_to_f32(src: &[i16], dst: &mut [f32]) {
        let n = src.len();
        let k = vdupq_n_f32(1.0 / 32_768.0);
        let mut i = 0;
        while i + 8 <= n {
            let v = vld1q_s16(src.as_ptr().add(i));
            let lo = vcvtq_f32_s32(vmovl_s16(vget_low_s16(v)));
            let hi = vcvtq_f32_s32(vmovl_s16(vget_high_s16(v)));
            vst1q_f32(dst.as_mut_ptr().add(i), vmulq_f32(lo, k));
            vst1q_f32(dst.as_mut_ptr().add(i + 4), vmulq_f32(hi, k));
            i += 8;
        }
        scalar::i16_to_f32(&src[i..], &mut dst[i..]);
    }

    #[target_feature(enable = "neon")]
    pub(super) unsafe fn interleave(l: &[f32], r: &[f32], out: &mut [f32]) {
        let n = l.len();
        let mut i = 0;
        while i + 4 <= n {
            let v = float32x4x2_t(vld1q_f32(l.as_ptr().add(i)), vld1q_f32(r.as_ptr().add(i)));
            vst2q_f32(out.as_mut_ptr().add(2 * i), v);
            i += 4;
        }
        scalar::interleave(&l[i..], &r[i..], &mut out[2 * i..]);
    }

    #[target_feature(enable = "neon")]
    pub(super) unsafe fn deinterleave(input: &[f32], l: &mut [f32], r: &mut [f32]) {
        let n = l.len();
        let mut i = 0;
        while i + 4 <= n {
            let v = vld2q_f32(input.as_ptr().add(2 * i));
            vst1q_f32(l.as_mut_ptr().add(i), v.0);
            vst1q_f32(r.as_mut_ptr().add(i), v.1);
            i += 4;
        }
        scalar::deinterleave(&input[2 * i..], &mut l[i..], &mut r[i..]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(n: usize, seed: u64) -> Vec<f32> {
        let mut r = crate::rng::Rng::new(seed);
        (0..n).map(|_| r.bipolar() * 1.5).collect()
    }

    fn close(a: f32, b: f32, n: usize) -> bool {
        (a - b).abs() <= 1e-5 * (n as f32).max(1.0)
    }

    #[test]
    fn every_kernel_matches_the_scalar_reference_on_every_remainder() {
        for n in 0..70 {
            let a = data(n, 1 + n as u64);
            let b = data(n, 100 + n as u64);
            assert!(close(dot(&a, &b), scalar::dot(&a, &b), n), "dot {n}");

            let mut d1 = b.clone();
            let mut d2 = b.clone();
            add_scaled(&mut d1, &a, 0.37);
            scalar::add_scaled(&mut d2, &a, 0.37);
            assert_eq!(d1, d2, "add_scaled {n}");

            let (mut s1, mut s2) = (a.clone(), a.clone());
            scale(&mut s1, -1.25);
            scalar::scale(&mut s2, -1.25);
            assert_eq!(s1, s2, "scale {n}");

            assert_eq!(peak(&a), scalar::peak(&a), "peak {n}");

            let (mut q1, mut q2) = (vec![0i16; n], vec![0i16; n]);
            f32_to_i16(&a, &mut q1);
            scalar::f32_to_i16(&a, &mut q2);
            assert_eq!(q1, q2, "f32_to_i16 {n}");

            let (mut f1, mut f2) = (vec![0.0; n], vec![0.0; n]);
            i16_to_f32(&q1, &mut f1);
            scalar::i16_to_f32(&q1, &mut f2);
            assert_eq!(f1, f2, "i16_to_f32 {n}");

            let (mut i1, mut i2) = (vec![0.0; 2 * n], vec![0.0; 2 * n]);
            interleave(&a, &b, &mut i1);
            scalar::interleave(&a, &b, &mut i2);
            assert_eq!(i1, i2, "interleave {n}");

            let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
            deinterleave(&i1, &mut l, &mut r);
            assert_eq!((l, r), (a.clone(), b.clone()), "deinterleave {n}");
        }
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn both_x86_paths_agree_with_scalar() {
        for n in [0, 1, 7, 15, 16, 17, 33, 255] {
            let a = data(n, 7);
            let b = data(n, 8);
            let want = scalar::dot(&a, &b);
            // SAFETY: SSE2 is baseline; AVX only when detected.
            assert!(close(unsafe { x86::dot_sse2(&a, &b) }, want, n));
            if std::arch::is_x86_feature_detected!("avx") {
                assert!(close(unsafe { x86::dot_avx(&a, &b) }, want, n));
                let (mut d1, mut d2) = (b.clone(), b.clone());
                unsafe { x86::add_scaled_avx(&mut d1, &a, 2.0) };
                scalar::add_scaled(&mut d2, &a, 2.0);
                assert_eq!(d1, d2);
                let (mut s1, mut s2) = (a.clone(), a.clone());
                unsafe { x86::scale_avx(&mut s1, 0.1) };
                scalar::scale(&mut s2, 0.1);
                assert_eq!(s1, s2);
            }
            let (mut d1, mut d2) = (b.clone(), b.clone());
            unsafe { x86::add_scaled_sse2(&mut d1, &a, 2.0) };
            scalar::add_scaled(&mut d2, &a, 2.0);
            assert_eq!(d1, d2);
            let (mut s1, mut s2) = (a.clone(), a.clone());
            unsafe { x86::scale_sse2(&mut s1, 0.1) };
            scalar::scale(&mut s2, 0.1);
            assert_eq!(s1, s2);
        }
        assert!(matches!(level(), Level::Sse2 | Level::Avx));
    }

    #[test]
    fn conversion_edge_cases() {
        let x = [
            0.0,
            1.0,
            -1.0,
            2.0,
            -7.0,
            f32::NAN,
            0.5 / 32_767.0,
            1.5 / 32_767.0,
            f32::INFINITY,
            f32::NEG_INFINITY,
            1e30,
            -0.0,
            0.25,
            -0.25,
            0.999,
            -0.999,
        ];
        let mut q = [0i16; 16];
        f32_to_i16(&x, &mut q);
        assert_eq!(
            q,
            [
                0, 32_767, -32_767, 32_767, -32_767, 0, 0, 2, 32_767, -32_767, 32_767, 0, 8_192,
                -8_192, 32_734, -32_734
            ]
        );
        let mut f = [0.0f32; 3];
        i16_to_f32(&[i16::MIN, 0, i16::MAX], &mut f);
        assert_eq!(f, [-1.0, 0.0, 32_767.0 / 32_768.0]);
    }

    #[test]
    fn peak_ignores_nan_and_sum_squares_is_accurate() {
        let mut v = vec![0.1f32; 37];
        v[5] = f32::NAN;
        v[30] = -0.9;
        assert_eq!(peak(&v), 0.9);
        assert_eq!(peak(&[]), 0.0);
        let ones = vec![1.0f32; 1_000_003];
        assert_eq!(sum_squares(&ones), 1_000_003.0);
    }
}
