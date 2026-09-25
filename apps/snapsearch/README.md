# SnapSearch

Find a photo by describing it, or by handing it one. A real application:
it reads your actual photo folder, indexes it in the background, searches
it by embedding similarity, and opens what it finds full screen.

Built on `vieww` (expected as a sibling directory, `../vieww`).

```console
cargo run                       # reads ~/Pictures, or $SNAPSEARCH_PHOTOS
SNAPSEARCH_PHOTOS=~/Photos cargo run
cargo test                      # 38 tests (47 with --features clip)
```

## What it actually does

**Reads real photos.** On start it scans `$SNAPSEARCH_PHOTOS`, or the
platform's pictures folder — `~/Pictures`, `/sdcard/DCIM` on Android —
four directories deep, skipping hidden files, up to 2000 photos. Each one
is read, decoded and downscaled to a 256px thumbnail through
`vieww_asset::decode_sized`. Files dropped onto the window are added the
same way, via `FrameDriver::take_dropped_files()`.

If no photos are found it opens on a small generated set and the header
says `SAMPLE SET · NO PHOTOS FOUND`. It never presents a gradient as a
photograph.

**Indexes off the frame thread.** Decoding a JPEG is milliseconds;
embedding one through CLIP is tens of milliseconds. A thousand photos is
therefore a minute of work, and on the UI thread that is a minute of frozen
application. The whole job — read, decode, downscale, embed — goes to
`vieww_foundation::task::Spawn`, and the UI thread only drains finished
photos, twelve a frame. The grid fills in while you use it, and the header
counts down `INDEXING · 412 LEFT`.

This is why `Embedder` is `Send + Sync`: the model runs on the worker.

**Remembers what it embedded.** Vectors are cached through
`vieww_foundation::Storage`, keyed on path + size + mtime + encoder name.
A second launch reads them back instead of re-running the model. Without
this, CLIP over a real library would re-embed everything on every start.
The encoder is in the key because one encoder's vectors are meaningless to
another, and serving one to the other produces confident nonsense.

**Opens a photo full screen.** Tapping a result opens it, kicks off a
full-resolution decode on a worker, and swaps it in when it lands — the
thumbnail is shown scaled until then, so there is something to look at
immediately. A decode that finishes after you have moved on is discarded
by identity check rather than appearing over the wrong picture.

## The embedding model

Search is a joint embedding space: images and text both encoded to vectors,
matched by cosine similarity, nearest-neighbour lookup for results. That
machinery is shared. Two encoders plug into it.

| | **local appearance** (default) | **CLIP ViT-B/32** (`--features clip`) |
|---|---|---|
| encodes | colour, structure, composition | learned image/text *meaning* |
| "a sunset over water" | warm-topped and blue photos — which is usually right | photos that depict one |
| "my dog on the porch" | little of use | photos with a dog in them |
| needs | nothing | ~600MB of weights |
| speed | instant | tens of ms per photo |

### The local encoder is a real descriptor

72 dimensions of actual content-based image retrieval, read off real pixels:
a 4×4 grid of mean colour, a 12-bin saturation-weighted hue histogram, an
8-bin edge-orientation histogram from a Sobel pass over luma, and four
global statistics including Hasler–Süsstrunk colourfulness. Features are
centred before normalising — uncentred, every image sits in the positive
orthant and the index ranks by brightness and nothing else.

The edge histogram is what makes it respond to *structure*: a beach and a
plain blue wall are the same colour and a very different picture.

Its threshold is measured, not guessed. Across the sample library the
correct subject scores 0.84–1.00 and the first wrong answer 0.24–0.88, so
0.80 keeps every right answer. It does not cut everything wrong — a plate
of curry scores 0.88 against "sunset", because to an encoder that reads
colour and edges a plate of curry genuinely looks like a sunset. That is
the documented limit of this backend.

### CLIP

```console
./scripts/fetch-clip.sh
export SNAPSEARCH_CLIP_DIR="$PWD/models/clip-vit-base-patch32"
cargo run --features clip
```

Real `candle-transformers` model, both towers, correct CLIP preprocessing
(its own channel statistics — not ImageNet's, which degrades similarity
silently), 512-dim L2-normalised vectors into the same index. Tokenizer in
either published layout: `tokenizer.json`, or `vocab.json` + `merges.txt`.
Everything on-device; no photo and no query leaves the machine. If the
weights are missing it says so on stderr and falls back rather than
silently returning worse results, and the search sheet always names the
live encoder.

**What I could and could not verify.** This sandbox blocks every model
host, so real CLIP weights were never loaded here. Rather than leave that
as "it compiles", the whole path is exercised by tests that *run*:

- a real forward pass through the real architecture — preprocessing →
  `[1,3,224,224]` → vision tower → projection → L2 norm → 512 dims;
- the text tower, with padding to CLIP's 77-token context;
- **loading a model directory off disk** — a real `model.safetensors` with
  CLIP's exact tensor names and shapes is written, then read back through
  the same `from_mmaped_safetensors` path a download goes through;
- different pictures producing different unit vectors, which rules out a
  preprocessing path that quietly discards the image.

The weights in that file are randomly initialised, so none of it says
anything about *match quality* — one test asserts exactly that, so the
others cannot be read as more than they are. The only thing between this
and real CLIP is the values in the file, and `scripts/fetch-clip.sh` gets
them.

(Zeroed weights were the first attempt and produce `NaN` everywhere:
LayerNorm divides by a zero variance. Zeros are not weights.)

## Built on the 2026-08-26 `vieww`

**Frosted glass** behind the search sheet and the detail view — `Filtered`,
blur as a σ (8.0 ≈ CSS `blur(24px)`) with a desaturation chained on, which
composes into the same matrix and stays one pass. **Staggered reveal** of
results via a `Timeline` attached once at startup so a completed search can
replay it. **Collapsing header** off one `ScrollTimeline`. **Motion from
theme tokens** on `Motion::expressive()` — spatial springs overshoot,
effects springs never do, and a test asserts it.

### A defect found on the way in

Wrapping the screen in that blur made **every photo disappear**.
`src/bin/filter_probe.rs` reduces it to stock widgets:

| subtree inside the `Filtered` | `filtered_layers` | result |
|---|---|---|
| plain content | 1 | filtered correctly |
| a `Scrollable` alone | **0** | content intact, no blur — filter dropped |
| plain content *and* a `Scrollable` | 1 | plain part filtered, **scrollable's content lost** |

A `Scrollable`'s viewport is a repaint boundary, and a boundary records into
a layer the filter's offscreen pass never reads. The third row is the
dangerous one: missing *content*, not a missing effect, and nothing reports
it. The app filters on each side of the boundary instead. Also:
`SceneReport::skipped_filters` — cited as the way to catch a dropped filter
— is GPU-only and does not compile against a CPU report.

The previous round's CPU glyph-transform bug is **fixed upstream**, with the
same one-line change, so that patch is gone from here.

## Verified, not assumed

`cargo run --bin screenshot --features cpu` renders the app through `vieww`'s
CPU backend — no display, no GPU — against real image files. Looking at
those pictures is what caught, across this and previous rounds: an
invisible reference picker (a horizontal `Scrollable` collapses to zero
height), a progress dialog pinned only horizontally, the FAB overlapping the
sheet's own Search button, an inverted flip formula, search results
reshuffling themselves mid-view, `"sunset over water"` returning pink
flowers (averaging a warm vector with a cold one lands on magenta), and a
detail caption hugging the right edge (`CrossAxisAlignment::Center` sizes
each line to its own text, leaving `TextAlign` no width to centre within).

Tests cover retrieval, not just plumbing — `"sunset"`, `"the ocean"`,
`"a walk in the forest"` and `"flowers"` must each rank their own subject
first, and the encoder only ever sees pixels, never the label it is judged
on — plus the real-file path end to end: scan, decode real PNGs on workers,
embed, and a cache round-trip proving a second run re-embeds nothing.

## Known limits

- **Android has no photo access.** `/sdcard/DCIM` needs
  `READ_MEDIA_IMAGES`, and `vieww` has no permission service, so on a phone
  the scan will usually come back empty and the app falls back to samples.
  The app builds and runs there; it just cannot see the camera roll yet.
- **Android has no embedding cache** — `FileStorage::for_android` needs the
  `AndroidApp` handle, which `build_root` does not receive.
- **iOS is unverified by the framework itself.** `vieww`'s own source says
  in three places that no frame has ever reached an iPhone.
- `task::Threads` is a thread per task — honest for a few hundred photos,
  wrong for a few thousand. `Spawn` is the seam for a real pool.
- Text search under the local encoder matches appearance, not subject.
  That is what CLIP is for.
- No `SharedElement` flight between grid and detail yet: the detail view
  exists now, so the remaining work is the capture-then-fly sequence.
