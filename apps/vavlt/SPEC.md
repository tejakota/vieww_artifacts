# Vault — Core Specification

Revision 2 · 2026-08-06 · supersedes the initial brainstorm

A local-first, cross-platform file vault that reclaims phone storage by re-encoding
photos and video. Two user-facing tiers: **Move** (fully reversible) and **Deep Move**
(irreversible, visually lossless). Nothing is read, transformed, or deleted without
explicit, scoped, revocable consent. No network.

---

## 1. Non-negotiable invariants

1. **No ambient authority.** The engine receives OS-issued file descriptors, never
   paths. It cannot enumerate or open anything it wasn't handed.
2. **Consent is three separate grants** — Access, Transform, Delete-original. Never bundled.
3. **Manifest-hash binding.** The engine refuses to run if the plan drifted from what
   the user was shown.
4. **Verify before delete.** Lossless → BLAKE3 equality. Lossy → decode and score.
   Failure means fall back to lossless, never ship a bad encode.
5. **Write-ahead ordering.** encode → verify → commit → *only then* is the original
   eligible for deletion. Crash at any point leaves originals intact.
6. **14-day safety net** on Deep Move. "Restore everything" is a real button.
7. **Append-only hash-chained audit log**, user-visible and exportable. Skips and
   failures logged as loudly as successes.
8. **Reported savings are measured, net, and quality-qualified.** Net of vault
   overhead. A lossy ratio with no perceptual score is never quoted as achievable.
9. **A model may inform a decision. It may never produce stored bytes.**

---

## 2. Stack

| Layer | Choice | Status |
|---|---|---|
| Language | Rust, everywhere | settled |
| UI | **vieww** | one tree for all three targets; renders on a real Android phone and a real iPhone |
| Android host | `android-activity` (NativeActivity, via winit) + ~15-line Java shim | shim **confirmed still required** |
| iOS host | winit's `UIApplicationMain`; `objc2` for the picker | picker written, **not yet run on a device** |
| Index | SQLite via `rusqlite` (WAL) | — |
| Network | **None. No `INTERNET` permission declared.** | — |

### UI licensing — no longer a decision

Superseded. Slint's royalty-free licence permitted proprietary mobile apps only
if the app *disclosed* its use of Slint, which is why Open Decision #1 existed
and why Settings → About carried a required attribution line.

**vieww is Apache-2.0.** That asks for a notice, not a visible credit, so the
line in Settings → About is now a credit this app chooses to give. There is no
licence blocker and no commercial tier to weigh.

What replaced the licensing question is a maturity one, and it is a real cost:
vieww is 0.0.1, nothing is published, and every crate may break in any release.
The offsetting fact is that the interface is now **one** tree rather than two,
so the surface exposed to that churn is a third of what the Slint build's was.

---

## 3. Tier model

| | **Move** | **Deep Move** | **Deep Move+** *(not in v1)* |
|---|---|---|---|
| Reversible | ✅ byte-exact | ❌ | ❌ |
| Visible loss | none | none at normal viewing | yes on close inspection |
| Photo policy | Lossless | SSIMULACRA2 ≥ 90 | SSIMULACRA2 ≥ 82 |
| Video policy | Passthrough | VMAF ≥ 95 | VMAF ≥ 90, 1080p cap |
| Retention | n/a | 14 days | 14 days |

Implemented in `crates/core/src/tier.rs`. A tier is **policy only** — one pipeline
executes all of them.

---

## 4. Compression matrix

| Asset | Move | Deep Move | Deep Move+ |
|---|---|---|---|
| **JPEG** | Lepton, bit-exact | → **JXL** (or AVIF), SSIMULACRA2 ≥ 90 | butteraugli d=2.0 |
| **PNG** | oxipng -o4 | → AVIF/JXL lossy | + 12 MP cap |
| **HEIC** | store as-is | store as-is | mild requant |
| **H.264 MP4** | store as-is | → HW **HEVC**, VMAF ≥ 95 | VMAF ≥ 90 + 1080p cap |
| **HEVC MP4** | store as-is | keep or CRF+2 | VMAF ≥ 90 |
| **DNG / ProRAW** | zstd container | **untouched** | **untouched** |
| **Live / Motion Photo** | lossless both halves | still only, video intact | still only |
| **Audio in video** | untouched | **Opus 64 kbps** | Opus 48 kbps |
| **Non-media** | zstd-19 | same | same |
| **All** | **whole-file** BLAKE3 dedup | same | same |

> Ratio figures deliberately removed from this table. See §8 — the priors did not
> survive first contact with a real corpus, and the harness now owns those numbers.

### Codec choice changes from revision 1

**JPEG XL becomes the primary lossy photo codec, not AVIF.** JXL encodes
*significantly* faster than AVIF at comparable density — at effort 6 it is
comparable to mozjpeg with trellis, fast enough to encode on the fly, whereas AVIF
effectively requires offline encoding. On a phone processing thousands of photos
under a thermal budget, encode speed dominates. AVIF is retained only for assets
that may leave the vault, where decoder support is broader.

**HEVC remains the video default; AV1 is flagship-only.** AV1 hardware *encode*
arrived on Tensor G3 (first SoC, 4K60) and Snapdragon 8 Gen 3 (late 2023, not at
4K60). AV1 *decode* is widespread (Snapdragon 8 Gen 1+, Dimensity 9000+,
Exynos 2200+) but decode does not help us. HEVC hardware encode is universal on
both platforms. **Ship HEVC; probe for AV1 encode at runtime and prefer it when
present.**

---

## 5. Toolchain

| Purpose | Crate / API | Verified |
|---|---|---|
| Format sniffing | `infer`, `mp4parse` | ✅ in harness |
| JPEG lossless | **`lepton_jpeg` 0.5.8** (Microsoft, active — June 2026) | ✅ roundtrip proven |
| PNG lossless | `oxipng` 10.x | ✅ in harness |
| General lossless | `zstd` | ✅ in harness |
| JPEG XL | `jpegxl-rs` → libjxl (FFI) | pending |
| AVIF encode | `ravif` / `rav1e` | ✅ builds (needs `nasm` for asm) |
| **Video, iOS** | VideoToolbox `VTCompressionSession` — C API | pending |
| **Video, Android** | NDK `AMediaCodec` — C API | pending |
| Video, desktop harness | `ffmpeg` CLI (x265) | ✅ in harness |
| Image quality gate | `ssimulacra2` | pending |
| Video quality gate | `libvmaf` | wired; local ffmpeg lacks it |
| Hashing / dedup | `blake3` | ✅ in harness |
| Crypto | `chacha20poly1305`, `argon2` | pending |
| ML | `tract` (classifiers), `candle` (embeddings), `hnsw_rs` | pending |

`lepton_jpeg` API note: `encode_lepton`/`decode_lepton` take a 4th
`&dyn LeptonThreadPool` argument, and the writer must implement `Write + StreamPosition`
(blanket-impl'd for `Seek`, so use `Cursor<Vec<u8>>`, not `Vec<u8>`).

---

## 6. Consent architecture

```rust
pub struct Grant {
    id:         GrantId,
    scope:      Vec<AssetId>,     // exactly what was picked — no globs
    permits:    Permits,          // READ | TRANSFORM | DELETE_ORIGINAL
    tier:       Option<Tier>,
    manifest:   ManifestHash,     // binds to the plan shown on screen
    expires_at: SystemTime,
}

/// Only constructible from a grant. No Deref, no into_inner.
pub struct Consented<T> { inner: T, grant: GrantId }

/// The sole ingress. The fd comes from the OS picker — the fd *is* the capability.
pub fn ingest(fd: RawFd, meta: AssetMeta, g: &Grant) -> Result<Consented<Asset>>;
pub fn execute(work: Consented<Manifest>, cancel: Arc<AtomicBool>) -> Result<Report>;
```

Access comes exclusively from **out-of-process OS pickers** — `PHPickerViewController`
(iOS) and Photo Picker / `ACTION_PICK_IMAGES` (Android). **Zero media permissions
declared on either platform.** Deletion routes through `PHPhotoLibrary.performChanges`
/ `MediaStore.createDeleteRequest()`, so the OS renders the confirmation.

### The Android Java shim — confirmed necessary

`android-activity` still does **not** surface `onActivityResult`
([rust-mobile/android-activity#174](https://github.com/rust-mobile/android-activity/issues/174)).
JNI can *call* any Java method but cannot *define* one, and `NativeActivity`/
`GameActivity` extend plain `Activity`, so `ComponentActivity.registerForActivityResult`
is unavailable. The shim is ~15 lines of **Java** (no Kotlin toolchain):

```java
public class VaultActivity extends GameActivity {
    @Override protected void onActivityResult(int req, int res, Intent data) {
        super.onActivityResult(req, res, data);
        nativeOnActivityResult(req, res, data);   // → Rust
    }
    private static native void nativeOnActivityResult(int req, int res, Intent data);
}
```

Two flows use it: the photo picker, and the system delete confirmation. Launching
does not need it — `startActivityForResult` is called directly over JNI.
(`jni-min-helper` is worth evaluating as a convenience layer.)

iOS needs no equivalent: the Objective-C runtime allows runtime class creation, so
`objc2::declare_class!` defines the `PHPickerViewControllerDelegate` from Rust.

---

## 7. Storage

- **Whole-file BLAKE3 dedup for media.** ⚠️ *Changed from revision 1.* Content-defined
  chunking (FastCDC) finds shared byte ranges; compressed media has none. On a
  photo/video library CDC yields almost nothing beyond whole-file hashing while
  costing CPU and ~4M index rows per 64 GB. **FastCDC is retained only for the
  document bucket.**
- **Near-duplicate detection carries the media redundancy win** (bursts, re-saves,
  edits) — see §9.
- **Crypto:** Argon2id over the passphrase → vault key; XChaCha20-Poly1305 per chunk,
  nonce derived from chunk hash + vault key. Single-user local vault, so no
  cross-user confirmation-of-file exposure.
- **Seekable encryption is a requirement, not a nice-to-have.** In-vault video
  playback needs random access, so use fixed-size plaintext framing with per-frame
  nonces behind a `Read + Seek` decrypting reader.
- **Provenance per asset** — tier, encoder, quality score. Surfaced as a badge;
  enables "restore all Move items" independently.

### vavlt overhead is part of the savings math

Thumbnails (256 px, ~15 KB) and comparison-viewer previews (1024 px, ~80 KB) cost
roughly **1.2 GB on a 12,800-item library**. Per invariant 8, reported savings are
**net of this**. Implemented in `estimate.rs`; shown as a negative column in
`vault scan`.

---

## 8. Measurement over assumption

Every ratio in `crates/core/src/estimate.rs::RATIOS` is a published-benchmark prior,
in one place, explicitly labelled. `vault measure` replaces them with numbers from a
real corpus.

**First run already contradicted two priors** (23 desktop wallpapers + a synthetic
1080p clip — *not* a phone library):

| Codec | Prior | Measured | Reading |
|---|---|---|---|
| JPEG / lepton | 21% | **13.5–14.9%** | corpus JPEGs already well optimised; camera JPEGs should land higher |
| PNG / oxipng -o4 | 18% | **0.1–0.6%** | these PNGs are pre-optimised; phone screenshots will differ |
| JPEG / zstd | ~0% | **0.2%** | ✅ confirms generic compression is useless on media |
| PNG / zstd | — | **0.3–1.0%** | ✅ same |
| H.264 / zstd | ~0% | **0.0%** | ✅ same |
| H.264 → HEVC crf28 | 44% | 64.7% *(unqualified)* | synthetic 12 Mbps source, over-provisioned; **no VMAF gate ran** |

**Conclusion: the priors are not transferable, in either direction.** The harness must
be run against a real exported camera roll before any number reaches a UI mock. That
is the single highest-value next action.

---

## 9. Machine learning — scope

**Rule: a model may inform a decision; it may never produce stored bytes.** Every model
output is a hint that a deterministic verifier checks. A wrong model costs speed,
never a photo.

| Use | Payoff | Runtime |
|---|---|---|
| Quality/complexity predictor → starting CRF | ~40% faster Deep Move (fewer search encodes) | `tract` |
| Content classifier (screenshot/doc/meme vs. memories) | better routing | `tract` |
| **Near-duplicate detection** | **15–30% of a real library** | `candle` + `hnsw_rs` |
| Best-of-burst scoring | picks the keeper | `tract` |

Model sizes: MobileNetV3-small int8 ≈ 2.5 MB; embedding model ≈ 8–15 MB. All offline —
no network permission needed.

**Near-duplicate detection may be the better v1 headline than Deep Move**: it is the
largest non-destructive win and lands entirely inside the reversible tier. It requires
its own per-group consent — near-duplicates are *distinct files*, never auto-deleted.

### Explicitly rejected

- **Neural codecs in the storage path.** Decode must be bit-deterministic across every
  device and OS version, forever. Float divergence between CoreML/NNAPI/future silicon
  is a data-loss vector. JPEG AI reached International Standard (Part 1 IS targeted
  April 2026) and claims ~28.5% over VVC intra, but the only published device tests ran
  on a Snapdragon phone with an ML accelerator, and behaviour without an NPU is
  unknown. **Watch; do not build on.**
- **Super-resolution "restoration" on export.** Fabricates detail that was never in the
  photo and presents invented pixels as the user's memory. The clearest possible
  violation of invariant 8.
- **Denoise/enhance on import.** The vavlt stores what the camera saw.

---

## 10. Workspace

```
real-vault/
  crates/
    core/      classify · estimate · tier · report      ← no platform deps
    codecs/    lepton · oxipng · zstd · avif · video    ← one Codec trait
    cli/       scan · measure                            ← desktop harness
  platform/    (next) android: JNI + AMediaCodec + shim/Vault.java
               (next) ios:     objc2 + VideoToolbox
  ui/          (next) Slint
```

Built and running: `crates/core`, `crates/codecs`, `crates/cli`.

---

## 11. Build order

1. ✅ **`cli/` measurement harness** — promoted to first. Pure Rust, no encoders, no UI.
2. **Run it on a real exported camera roll.** Replaces §8's priors with facts.
3. **iOS spike** — app launches, picker opens, fd reaches Rust. De-risks Slint on iOS.
4. **Lossless path** — Lepton, oxipng, whole-file dedup, CAS, BLAKE3 verify → ships **Move**.
5. **Near-duplicate detection** — own consent flow; largest non-destructive win.
6. **Photo lossy** — JXL primary, SSIMULACRA2 gate with CRF search.
7. **Video lossy** — VideoToolbox first, then AMediaCodec, behind `HwEncoder`.
8. **Consent UI, audit log, retention window, comparison viewer.**

---

## 12. Open decisions

| # | Question | Status |
|---|---|---|
| 1 | Slint license for closed-source | ✅ **closed** — royalty-free + attribution |
| 2 | Slint iOS maturity | ⏳ gate on step 3 spike; Android is production-ready |
| 3 | Full-library scan offered? | **No** — picker-only for v1 |
| 4 | Ship Deep Move+ in v1? | **No** — two tiers |
| 5 | Audit log survives uninstall? | **No** — export-only |
| 6 | Per-asset override | demotion only ("always Move") |
| 7 | **VMAF sampling strategy** | ⚠️ **open** — full VMAF ≈ doubles video cost. Sample every Nth frame, keyframes only, or SSIMULACRA2 on sampled frames? |
| 8 | **Per-scene encoding** | ⚠️ **open** — Netflix-style per-shot CRF is worth 10–20% over fixed CRF and pairs with the ML predictor. Largest un-taken video win. |

---

## Sources

- [Slint on iOS — NLnet](https://nlnet.nl/project/SlintiOS/)
- [Slint mobile & web platforms](https://deepwiki.com/slint-ui/slint/9.3-mobile-and-web-platforms)
- [Slint pricing](https://slint.dev/pricing) · [LICENSE.md](https://github.com/slint-ui/slint/blob/master/LICENSE.md)
- [android-activity #174 — onActivityResult](https://github.com/rust-mobile/android-activity/issues/174)
- [microsoft/lepton_jpeg_rust](https://github.com/microsoft/lepton_jpeg_rust) · [docs.rs](https://docs.rs/lepton_jpeg)
- [Tensor G3 first SoC with AV1 encode](https://wccftech.com/google-tensor-g3-first-smartphone-soc-to-support-av1-encoding-4k-60fps/)
- [AV1 device compatibility list](https://gist.github.com/nekotrix/82158fc48f7edb0db8338779a997dad0)
- [JPEG XL and the Pareto front — Cloudinary](https://cloudinary.com/blog/jpeg-xl-and-the-pareto-front) · [JPEG XL vs AVIF](https://blog.openreplay.com/jpeg-xl-vs-avif/)
- [JPEG AI becomes an International Standard](https://jpeg.org/items/20250219_press.html) · [JPEG AI is coming](https://streaminglearningcenter.com/codecs/jpeg-ai-is-coming-what-you-need-to-know.html)


## vavlt branding

- Launcher/app mark is a white `V` with a hollow square at bottom-right on a near-black rounded field.
- Startup animation uses a 5x5 checkerboard of letters. Non-target letters black out first; the five target letters `v a v l t` then move into one horizontal wordmark and fade into the home screen.
- The startup animation is deliberately short (1.45s) and runs as a paint-time animation so it does not add ongoing work to the photo grid.
