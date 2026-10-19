//! Transient buffer reuse for `PushLayer`/`PopLayer` — the CPU-rasterizer
//! shape of "Renderer v2" pillar A's transient resource aliasing
//! (`docs/RENDERER-V2-NOTES.md`).
//!
//! `target.rs`'s own module doc has said since the vello migration that one
//! `Target` type serves "the root frame buffer and every offscreen layer
//! ... minus the pooling", tracked as follow-up. This is that follow-up:
//! every isolated-layer buffer used to come from a fresh `Target::new(w, h)`
//! on `PushLayer` and be dropped on `PopLayer`'s return trip; now
//! [`NativeRenderer`](super::NativeRenderer) keeps a small pool of
//! already-allocated buffers keyed by exact size and hands one back out
//! instead of allocating, whenever a layer of a size it has seen before
//! opens again.
//!
//! # Why exact-size keying, not a bucket/nearest-fit scheme
//!
//! A GPU transient-resource allocator buckets by size class because
//! resizing a GPU allocation mid-frame is expensive to avoid entirely. A
//! `Vec<Premul>` has no such asymmetry — reusing a too-big buffer still
//! means writing `width * height` new elements to clear it, the same cost
//! as a fresh `vec![TRANSPARENT; n]` allocation would pay to zero itself,
//! so nearest-fit buys nothing here that isn't already free. Exact-size
//! keying is what actually saves the allocation: same `PushLayer` bounds
//! next frame (the overwhelmingly common case — a widget's clip rectangle
//! does not usually change frame to frame) means the buffer clear is the
//! only work, no allocator call at all.
//!
//! # Why sibling reuse falls out for free
//!
//! Layers are pushed and popped as a strict stack
//! (`native/reference.rs`'s `stack: Vec<LayerFrame>`), so two sibling
//! `PushLayer` groups are never open at once — the first is always popped,
//! returning its buffer to the pool, before the second is pushed. A pool
//! keyed only by size, with no notion of "this frame" versus "last frame",
//! already reuses a same-size sibling's buffer immediately, and reuses a
//! *previous frame's* buffer just as well once the whole scene has been
//! walked once. Nothing about the pool needs to know which case it is in.
//!
//! # Why the idle set carries a byte budget, not just a count
//!
//! The pool's first version capped only the *count* of idle buffers
//! (`MAX_IDLE`), which is a cap on the wrong unit: thirty-two idle
//! full-window buffers at 1366x679 are 32 × 14.8 MB ≈ 460 MB of memory
//! doing nothing, while thirty-two idle 8x8 badge buffers are noise. What
//! the framework's memory targets (the "lightest" audit, PENDING §4.2)
//! actually bound is *bytes*, so the cap is bytes now
//! ([`DEFAULT_IDLE_BUDGET_BYTES`], overridable per renderer through
//! `VIEWW_POOL_IDLE_MB`), with the count cap kept as a backstop against
//! entry-count overhead rather than as the memory story. Eviction is
//! least-recently-*released* first — a `PushLayer` bounds that stopped
//! appearing is exactly the buffer least likely to be asked for again —
//! and one buffer bigger than the whole budget is still kept, matching
//! `native/residency.rs`'s "a budget is a target, not a hard cap": a
//! pathological one-off layer should not make every *other* acquire pay
//! for memory that was freed only to be re-allocated the moment it is
//! asked for again.
//!
//! This changed the old cap's behaviour deliberately: it used to *drop the
//! incoming buffer* once `MAX_IDLE` others were held, which froze the pool's
//! contents at whatever thirty-two sizes arrived first; it now *evicts the
//! least-recently-released buffer* to admit the new one, so the pool tracks
//! what the scene is doing now rather than what it did when it warmed up.
//! `PoolStats::evicted` counts that.

use super::color::Premul;
use super::target::Target;

/// A same-size buffer is reused after this many idle buffers are already
/// held, past which the marginal buffer is more likely to be a one-off (a
/// screen resize, a filter applied to an unusually large region) than
/// something worth holding onto — dropped instead of pooled. The byte
/// budget is the memory story; this is the bookkeeping backstop.
const MAX_IDLE: usize = 32;

/// The idle set's default byte budget: two full-window 1366x679 buffers
/// (14.8 MB each, the size `NativeRenderer::new`'s doc quotes for the root)
/// with room for a handful of smaller widget layers, and — deliberately —
/// not enough to hold even one idle 4K buffer (3840x2160 is 127 MB, which
/// under the 40 MB memory target PENDING §4.2 measures against is already
/// the whole budget spent three times over on a buffer that may never be
/// asked for again). A renderer that wants a different trade passes an
/// explicit budget or sets `VIEWW_POOL_IDLE_MB`.
pub(crate) const DEFAULT_IDLE_BUDGET_BYTES: usize = 32 * 1024 * 1024;

/// `VIEWW_POOL_IDLE_MB` / `VIEWW_BLUR_CACHE_MB` carry megabytes; this turns
/// one such value into bytes. Shared by the pool's and the blur cache's
/// environment overrides so the two budgets parse the same way — including
/// the same strictness (no units, no arithmetic, `0` allowed and meaning
/// "hold nothing back", which tests and memory-tight targets use).
pub(crate) fn mb_env_bytes(value: &str) -> Option<usize> {
    let mb: usize = value.trim().parse().ok()?;
    Some(mb.saturating_mul(1024 * 1024))
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PoolStats {
    /// `acquire` calls a same-size idle buffer satisfied, no allocation.
    pub reused: usize,
    /// `acquire` calls that had to allocate a fresh buffer.
    pub allocated: usize,
    /// `release` calls whose buffer was kept for later reuse.
    pub kept: usize,
    /// `release` calls whose buffer *displaced an older idle buffer*
    /// (least-recently-released first) because the pool was over its byte
    /// budget or `MAX_IDLE`. The first version of this pool dropped the
    /// incoming buffer in this situation instead, which pinned the pool to
    /// the sizes it warmed up with; see the module doc's budget section.
    pub evicted: usize,
}

/// A free list of already-allocated [`Target`] buffers, keyed by exact
/// `(width, height)`, held under a byte budget.
///
/// The idle vector is kept in least-recently-released-first order —
/// `release` pushes to the end, eviction removes from the front — so
/// "which buffer goes when the budget is exceeded" is `idle[0]`, no
/// timestamp bookkeeping. The same `Vec`-ordered-LRU trade
/// `native/residency.rs` documents applies here, with the same excuse:
/// the idle set is small by construction, and correct-by-construction
/// beats pointer-chasing for a cache whose whole job is being invisible.
#[derive(Debug)]
pub(crate) struct TargetPool {
    idle: Vec<Target>,
    idle_bytes: usize,
    budget: usize,
    stats: PoolStats,
}

impl TargetPool {
    pub(crate) fn new() -> Self {
        Self::with_budget(DEFAULT_IDLE_BUDGET_BYTES)
    }

    /// Like [`Self::new`], but with `VIEWW_POOL_IDLE_MB` overriding the
    /// budget when it is set and parses — the memory-tight targets' knob,
    /// read where the pool is constructed so every renderer in the process
    /// sees the same trade without each call site knowing about it.
    pub(crate) fn from_env() -> Self {
        match std::env::var("VIEWW_POOL_IDLE_MB")
            .ok()
            .as_deref()
            .and_then(mb_env_bytes)
        {
            Some(bytes) => Self::with_budget(bytes),
            None => Self::new(),
        }
    }

    pub(crate) fn with_budget(budget: usize) -> Self {
        Self {
            idle: Vec::new(),
            idle_bytes: 0,
            budget,
            stats: PoolStats::default(),
        }
    }

    /// A `width x height` buffer, cleared to fully transparent — either an
    /// idle buffer of exactly that size, reused, or a fresh allocation.
    pub(crate) fn acquire(&mut self, width: u32, height: u32) -> Target {
        if let Some(index) = self
            .idle
            .iter()
            .position(|t| t.width == width && t.height == height)
        {
            // `remove`, not `swap_remove`: the vector *is* the eviction
            // order, and swapping the last idle buffer into an arbitrary
            // position would quietly reshuffle which buffer counts as
            // least-recently-released. The vector is at most `MAX_IDLE`
            // long, so the shift is nothing.
            let mut target = self.idle.remove(index);
            self.idle_bytes -= buffer_bytes(&target);
            target.clear();
            self.stats.reused += 1;
            target
        } else {
            self.stats.allocated += 1;
            Target::new(width, height)
        }
    }

    /// Return a buffer for possible reuse by a later [`acquire`](Self::acquire)
    /// of the same size, evicting least-recently-released idle buffers while
    /// the idle set is over its byte budget or `MAX_IDLE`.
    ///
    /// The just-released buffer is always admitted, and the last remaining
    /// idle buffer is never evicted: a buffer larger than the whole budget
    /// (one full-window layer on a 4K screen, say) sits alone rather than
    /// triggering a free-everything-then-reallocate cycle it alone would
    /// pay for. See the module doc's budget section.
    pub(crate) fn release(&mut self, target: Target) {
        self.idle_bytes += buffer_bytes(&target);
        self.idle.push(target);
        self.stats.kept += 1;
        while self.idle.len() > 1 && (self.idle.len() > MAX_IDLE || self.idle_bytes > self.budget) {
            let victim = self.idle.remove(0);
            self.idle_bytes -= buffer_bytes(&victim);
            self.stats.evicted += 1;
        }
    }

    pub(crate) fn stats(&self) -> PoolStats {
        self.stats
    }

    /// Bytes currently sitting idle — the number the budget bounds, and the
    /// one memory figure this pool can state without a profiler attached.
    /// (Test-only callers today; the budget itself does not need it, but
    /// the budget's tests do, and the figure belongs on the type that
    /// owns the accounting.)
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn idle_bytes(&self) -> usize {
        self.idle_bytes
    }
}

/// A `Target`'s whole cost: `width * height` premultiplied-f32 pixels. The
/// pool stores nothing per buffer besides the buffer, so this is exact,
/// not an estimate.
fn buffer_bytes(target: &Target) -> usize {
    target.width as usize * target.height as usize * std::mem::size_of::<Premul>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_size_acquire_after_release_reuses_the_buffer() {
        let mut pool = TargetPool::new();
        let a = pool.acquire(10, 10);
        pool.release(a);
        let _b = pool.acquire(10, 10);
        assert_eq!(
            pool.stats(),
            PoolStats {
                reused: 1,
                allocated: 1,
                kept: 1,
                evicted: 0
            }
        );
    }

    #[test]
    fn different_size_acquire_does_not_reuse() {
        let mut pool = TargetPool::new();
        let a = pool.acquire(10, 10);
        pool.release(a);
        let _b = pool.acquire(20, 20);
        assert_eq!(
            pool.stats(),
            PoolStats {
                reused: 0,
                allocated: 2,
                kept: 1,
                evicted: 0
            }
        );
    }

    #[test]
    fn reused_buffer_comes_back_cleared() {
        use super::super::color::Premul;
        let mut pool = TargetPool::new();
        let mut a = pool.acquire(4, 4);
        a.set(
            1,
            1,
            Premul {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
        );
        pool.release(a);
        let b = pool.acquire(4, 4);
        assert_eq!(b.get(1, 1), Premul::TRANSPARENT);
    }

    #[test]
    fn releases_past_the_count_cap_evict_the_oldest_not_the_newcomer() {
        // The first version of this pool dropped the *incoming* buffer once
        // `MAX_IDLE` were held — the pool froze at whatever arrived first.
        // Now the newest buffer always lands and the least-recently-released
        // one leaves, so after `MAX_IDLE + 5` releases of distinct sizes the
        // five *first* sizes are the ones gone.
        let mut pool = TargetPool::new();
        let buffers: Vec<_> = (0..(MAX_IDLE + 5))
            .map(|i| pool.acquire(1, i as u32 + 1))
            .collect();
        for buffer in buffers {
            pool.release(buffer);
        }
        let stats = pool.stats();
        assert_eq!(stats.kept, MAX_IDLE + 5);
        assert_eq!(stats.evicted, 5);
        assert_eq!(pool.idle.len(), MAX_IDLE);
        // The survivors are the five newest sizes: sizes 6..=MAX_IDLE+5 of
        // 1..=MAX_IDLE+5. The oldest (1x1) must be gone; a 1x(MAX_IDLE+5)
        // survivor reuses without allocating.
        assert_eq!(pool.acquire(1, 1).width, 1);
        assert_eq!(pool.stats().allocated, MAX_IDLE + 6);
        let _ = pool.acquire(1, MAX_IDLE as u32 + 5);
        assert_eq!(pool.stats().reused, 1);
    }

    #[test]
    fn a_sibling_layer_reuses_a_popped_siblings_buffer() {
        // Mirrors the stack discipline `native/reference.rs` actually uses:
        // push, pop, push again — never two same-size buffers open at once.
        let mut pool = TargetPool::new();
        let first = pool.acquire(50, 50);
        pool.release(first);
        let second = pool.acquire(50, 50);
        pool.release(second);
        assert_eq!(
            pool.stats(),
            PoolStats {
                reused: 1,
                allocated: 1,
                kept: 2,
                evicted: 0
            }
        );
    }

    #[test]
    fn the_byte_budget_evicts_least_recently_released_first() {
        // Two distinct sizes, the first of which alone is the whole
        // budget: releasing the second must evict the *first*-released
        // (a), not refuse the newcomer. Same-size buffers would reuse
        // instead of accumulating, which is why the sizes differ. (A
        // budget of exactly the pair's total would never trip — "over"
        // is strict, and one of each is exactly the budget.)
        let px = std::mem::size_of::<Premul>();
        let budget = 10 * 10 * px;
        let mut pool = TargetPool::with_budget(budget);
        let a = pool.acquire(10, 10);
        pool.release(a);
        assert_eq!(pool.idle.len(), 1, "under budget, nothing evicted");
        let b = pool.acquire(10, 20);
        pool.release(b);
        assert_eq!(pool.stats().evicted, 1, "`a` was least-recently-released");
        assert_eq!(pool.idle.len(), 1);
        assert_eq!(pool.idle_bytes(), 10 * 20 * px, "exactly `b` remains");
        // Asking for `a`'s size now allocates (it was evicted); asking for
        // `b`'s reuses.
        let _ = pool.acquire(10, 10);
        assert_eq!(pool.stats().allocated, 3);
        let _ = pool.acquire(10, 20);
        assert_eq!(pool.stats().reused, 1, "`b` survived; `a` did not");
    }

    #[test]
    fn a_buffer_bigger_than_the_whole_budget_is_still_kept() {
        // The softness `native/residency.rs` documents: eviction never
        // empties the pool to make the numbers work, because a one-off
        // oversized layer would pay re-allocation on its very next use.
        let tiny = 1000;
        let mut pool = TargetPool::with_budget(tiny);
        let big = pool.acquire(100, 100); // 100*100*16 = 160_000 bytes
        pool.release(big);
        assert_eq!(pool.stats().kept, 1);
        assert_eq!(pool.stats().evicted, 0);
        assert_eq!(pool.idle.len(), 1);
        assert_eq!(pool.acquire(100, 100).width, 100);
        assert_eq!(pool.stats().reused, 1, "the oversized buffer survived");
    }

    #[test]
    fn idle_bytes_tracks_the_budgeted_figure_exactly() {
        let mut pool = TargetPool::with_budget(usize::MAX);
        let px = std::mem::size_of::<Premul>();
        let a = pool.acquire(10, 10);
        pool.release(a);
        assert_eq!(pool.idle_bytes(), 100 * px);
        let b = pool.acquire(4, 5);
        pool.release(b);
        assert_eq!(pool.idle_bytes(), (100 + 20) * px);
        let _ = pool.acquire(10, 10); // takes `a` back out of the idle set
        assert_eq!(pool.idle_bytes(), 20 * px);
    }

    #[test]
    fn the_env_override_parses_megabytes_strictly() {
        assert_eq!(mb_env_bytes("64"), Some(64 * 1024 * 1024));
        assert_eq!(mb_env_bytes(" 8 "), Some(8 * 1024 * 1024));
        assert_eq!(mb_env_bytes("0"), Some(0), "zero is a legitimate ask");
        assert_eq!(mb_env_bytes(""), None);
        assert_eq!(mb_env_bytes("1.5"), None, "no units, no fractions");
        assert_eq!(mb_env_bytes("16MB"), None, "the env var carries the unit");
        assert_eq!(mb_env_bytes("-4"), None);
    }
}
