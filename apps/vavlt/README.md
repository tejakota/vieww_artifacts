# real-vault

Rust engine for a consent-first photo/video vault. See [SPEC.md](SPEC.md).

This repo currently contains the **desktop measurement harness** — the engine core
with no UI and no platform encoders, so codec ratios can be measured on a laptop
against a real photo folder before any mobile build exists.

## Build

**vieww must be checked out beside this repo** — it is a path dependency, not a
vendored copy, so `../vieww` has to exist. Full commands for Linux, Android and
iOS are in [BUILD.md](BUILD.md); the short version:

```bash
git clone https://github.com/teja/vieww ../vieww

cargo run --release -p vavlt-desktop ~/Pictures   # a window
cargo apk run -p vavlt-android --release          # a phone
cargo build --release -p vavlt-cli                # the measurement harness
```

The codec harness alone, with no UI and no GPU:

```bash
cargo build --release -p vavlt-cli                 # core codecs
cargo build --release -p vavlt-cli --features avif # + AVIF (needs `nasm` for SIMD)
```

`--features avif` builds `rav1e` with assembly disabled unless `nasm` is installed.
Ratios are unaffected; AVIF *timings* under that build are not representative.

Video measurement shells out to `ffmpeg` if present. A build with `libvmaf` is
needed for the quality gate — without it, lossy rows are reported as
`UNQUALIFIED` rather than being quoted as savings.

## Use

```bash
# Metadata only — no pixels read. Mirrors the on-device tier screen.
./target/release/vault scan ~/Pictures

# Run the real codecs. Lossless results are proved by decode-and-compare.
./target/release/vault measure ~/Pictures --limit 200 --json report.json

# Include the H.264 → HEVC path (software x265, slow) with a quality gate.
./target/release/vault measure ~/Pictures --video --crf 28 --vmaf
```

## Why both commands

`scan` uses the priors in `crates/core/src/estimate.rs::RATIOS`; `measure` produces
facts. **The gap between them is the point.** The priors are published-benchmark
averages and the first real run already contradicted two of them — see SPEC §8.

Run `measure` against an exported camera roll before trusting any number in a UI.

## Layout

| Crate | Contents |
|---|---|
| `core` | classification, estimation, tier policy, reporting. No platform deps. |
| `codecs` | one `Codec` trait; Lepton, oxipng, zstd, AVIF, ffmpeg video |
| `engine` | the run, the audit chain, thumbnails, and the `Source` port. No UI. |
| `app` | **every screen, in vieww.** No platform, no window, no picker. |
| `cli` | `scan` and `measure` |
| `desktop` | a window and a folder walk |
| `ios` | a bundle and a PHPicker |
| `android` | an activity and the picker shim |

## One interface, three hosts

**The UI is `crates/app` and nothing else.** Android, iPhone and the desktop
mount the same widget tree, hold the same state, and run the same rules; a host
is a `Platform` impl four methods wide — where its private storage is, how to
read the bytes behind a handle, how to open the system picker, and how to run
work off the UI thread. None of them can change what the app does.

That replaced two separate Slint UIs, about 3,200 lines between them, which
shared a colour file and nothing else. They had already diverged: the desktop
had a comparison viewer the phone did not, the phone had a consent flow the
desktop did not, and the two type scales were maintained by hand against each
other.

```bash
cargo run --release -p vavlt-desktop ~/Pictures   # a window
cargo apk run -p vavlt-android --release          # a phone
ci/ios-app.sh --device                            # an iPhone
```

The layout is one tree with two sets of numbers, chosen from the *surface
width* rather than from which crate you are in — so a desktop window dragged
narrow becomes the phone layout, and a tablet is a phone that happens to be
wide. `crates/app/src/theme.rs` is where both sets live.

## What it looks like, and how that is checked

`cargo test -p vavlt-app --test screenshots --release` renders **every screen
through the real GPU backend to a PNG** — phone and desktop, dark and light,
forty files into `target/screenshots/`. Not a mock: the fixtures deliver the
same `Message`s a picker and a codec worker send, so the manifest hash, the
estimate, the audit chain and the navigation in each image are produced by the
code that ships. A screenshot that looks right is evidence the flow is right.

It skips rather than fails on a machine with no graphics adapter, the same way
vieww's own pixel tests do.

Two defects were found by looking at those files and would not have been found
any other way: every centred label was offset twice and ran off the right edge,
and the audit log's colour-coded rules were laying out three pixels wide and
zero pixels tall, so they drew nothing at all.

Adwaita's own named colours, both themes, in `crates/app/src/theme.rs` — mapped
onto vieww's `ColorScheme` so the framework's own controls come out
Adwaita-coloured with nothing restyling them at the call site.

Adwaita specifies statics only, so **motion is where this app has a point of
view**: the rule is mass, not bounce. The depth stack settles on a deep
out-ease rather than a linear slide, and the tier control's selection travels
between blue and red so the consequence of switching tiers lands before the
label is read.

### Two limits of the framework, stated rather than worked around

vieww has **no font-family selection** — one embedded family, and `TextStyle`
carries no family field. The Slint build set every figure in `DejaVu Sans Mono`;
the figures here are separated by weight and tracking instead.
`theme::Numeric::style` is the single place to change when vieww grows
families.

The embedded font is **missing some glyphs**: the `fi` ligature renders as a
mangled outline ("files" comes out wrong), and `≥` does not render at all. Both
are visible in `target/screenshots/`. Neither is anything this repo can fix.

## Reporting rules

The harness enforces the spec's transparency invariants:

- Lossless codecs **prove** their roundtrip (decode + byte compare). No claim without proof.
- Lossy codecs without a perceptual score print `UNQUALIFIED` and are never
  presented as comparable to lossless savings.
- `oxipng` reports `roundtrip_ok: None` — it is pixel-lossless, not byte-lossless,
  so the original file cannot be reproduced from the optimised one.
- Dedup is counted exactly, not modelled. Near-duplicates are explicitly excluded
  until the embedding model lands.
