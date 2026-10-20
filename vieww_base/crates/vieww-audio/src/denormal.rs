//! Flush-to-zero for the threads that render audio in real time.
//!
//! A feedback structure decaying to silence — a biquad's state, a reverb's
//! delay lines, an envelope's tail — passes through *subnormal* floats on
//! its way to zero. On x86 those take a microcode assist that costs one to
//! two orders of magnitude per operation: a reverb tail can turn a 1 ms
//! block into a 20 ms one, at the moment the music has gone quiet. The
//! standard remedy is the CPU's flush-to-zero mode on the threads doing
//! real-time work, which is what [`FlushDenormals`] scopes:
//!
//! * x86 / x86-64 (SSE): MXCSR's FTZ (results) and DAZ (inputs) bits.
//! * aarch64: FPCR's FZ bit.
//! * anything else (wasm32, 32-bit ARM): no-op — [`FlushDenormals::active`]
//!   reports `false`, and nothing is changed.
//!
//! The device backends hold one around every callback (`backend::isolate_*`),
//! restoring the thread's previous mode on the way out — the thread belongs to
//! the audio system, and its other users never see the change. The render pool
//! ([`crate::parallel`]) mirrors its *caller's* mode onto its workers for each
//! block, so every thread of one render flushes or none does: an offline render
//! (caller not flushing) stays bit-identical across thread counts, and a
//! real-time one flushes on every thread.
//!
//! Nothing audible changes: a subnormal is below 10⁻³⁸, more than 700 dB
//! under full scale.

/// The flush mode of the current thread, restored on drop.
#[derive(Debug)]
#[must_use = "the mode is restored when the guard drops"]
pub struct FlushDenormals {
    saved: Option<u64>,
}

impl FlushDenormals {
    /// Turn flush-to-zero on for this thread until the guard drops.
    pub fn enable() -> Self {
        Self::set(true)
    }

    /// Set flush-to-zero `on` or off for this thread until the guard drops.
    pub fn set(on: bool) -> Self {
        let saved = imp::read();
        if let Some(prev) = saved {
            let next = if on {
                prev | imp::BITS
            } else {
                prev & !imp::BITS
            };
            if next != prev {
                imp::write(next);
            }
        }
        Self { saved }
    }

    /// Whether this target has a flush mode at all.
    #[must_use]
    pub const fn supported() -> bool {
        imp::SUPPORTED
    }

    /// Whether the current thread is flushing subnormals now.
    #[must_use]
    pub fn active() -> bool {
        imp::read().is_some_and(|v| v & imp::BITS == imp::BITS)
    }
}

impl Drop for FlushDenormals {
    fn drop(&mut self) {
        if let Some(prev) = self.saved {
            if imp::read() != Some(prev) {
                imp::write(prev);
            }
        }
    }
}

#[cfg(any(
    target_arch = "x86_64",
    all(target_arch = "x86", target_feature = "sse")
))]
mod imp {
    /// MXCSR: FTZ (bit 15) flushes results, DAZ (bit 6) treats inputs as zero.
    pub(super) const BITS: u64 = (1 << 15) | (1 << 6);
    pub(super) const SUPPORTED: bool = true;

    pub(super) fn read() -> Option<u64> {
        let mut v: u32 = 0;
        // SAFETY: `stmxcsr` stores the 32-bit MXCSR to the given address,
        // which is a live, aligned local; it has no other effect.
        unsafe {
            core::arch::asm!("stmxcsr [{}]", in(reg) &raw mut v, options(nostack, preserves_flags));
        }
        Some(u64::from(v))
    }

    pub(super) fn write(v: u64) {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "MXCSR is 32 bits; BITS fit"
        )]
        let v = v as u32;
        // SAFETY: `ldmxcsr` loads a value read from MXCSR with only the FTZ
        // and DAZ bits changed — both defined, neither able to fault.
        unsafe {
            core::arch::asm!("ldmxcsr [{}]", in(reg) &raw const v, options(nostack, readonly, preserves_flags));
        }
    }
}

#[cfg(target_arch = "aarch64")]
mod imp {
    /// FPCR.FZ (bit 24).
    pub(super) const BITS: u64 = 1 << 24;
    pub(super) const SUPPORTED: bool = true;

    pub(super) fn read() -> Option<u64> {
        let v: u64;
        // SAFETY: reading FPCR is unprivileged and side-effect free.
        unsafe {
            core::arch::asm!("mrs {}, fpcr", out(reg) v, options(nomem, nostack, preserves_flags));
        }
        Some(v)
    }

    pub(super) fn write(v: u64) {
        // SAFETY: writes back a value read from FPCR with only FZ changed.
        unsafe {
            core::arch::asm!("msr fpcr, {}", in(reg) v, options(nomem, nostack, preserves_flags));
        }
    }
}

#[cfg(not(any(
    target_arch = "x86_64",
    all(target_arch = "x86", target_feature = "sse"),
    target_arch = "aarch64"
)))]
mod imp {
    pub(super) const BITS: u64 = 0;
    pub(super) const SUPPORTED: bool = false;

    pub(super) fn read() -> Option<u64> {
        None
    }

    pub(super) fn write(_: u64) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::hint::black_box;

    /// Half the smallest normal: a subnormal result when not flushing.
    fn tiny_product() -> f32 {
        black_box(f32::MIN_POSITIVE) * black_box(0.5)
    }

    #[test]
    fn the_guard_flushes_subnormals_and_restores_the_thread_on_drop() {
        if !FlushDenormals::supported() {
            return;
        }
        let before = FlushDenormals::active();
        {
            let _off = FlushDenormals::set(false);
            assert!(tiny_product() > 0.0, "subnormals exist when not flushing");
            {
                let _on = FlushDenormals::enable();
                assert!(FlushDenormals::active());
                assert_eq!(tiny_product(), 0.0, "flushed to zero");
            }
            assert!(
                !FlushDenormals::active(),
                "the inner guard restored the outer mode"
            );
            assert!(tiny_product() > 0.0);
        }
        assert_eq!(FlushDenormals::active(), before, "the thread is as it was");
    }
}
