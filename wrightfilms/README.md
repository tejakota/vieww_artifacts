# wrightfilms — the Wright product film

**THE ASK, THE WORKFORCE, THE PROOF** — the keynote film for
[Wright](../vieww_base/apps/wright/), the agent harness where you
describe the outcome and agents wright it. Every frame rendered by
[vieww](../vieww_base/) itself, headless; movement III is the **actual
application**.

| Artifact | What it is |
|---|---|
| `wright_film.mp4` | The keynote cut — 1920×1080 · 60 fps · **2:19** · 8,340 frames, every one rasterised by vieww's own CPU renderer. **For YouTube.** |
| `wright_teaser.mp4` | The 67-second door — the film's own cold open, run, receipt, refire and end card, for the places a full film won't fit |
| `wright_twitter.mp4` | The 41-second cut for **X** — the ask, the words, the gate, the mark (1920×1080) |
| `wright_linkedin.mp4` | The 61-second cut for **LinkedIn** — the trust buyer's edit: the gate, the bounds, the receipt, the delivery, the refire (1920×1080) |
| `wright_instagram.mp4` | The 33-second square for **the Instagram feed** — question, words, the real window framed like a print, the mark (1080×1080) |
| `wright_reels.mp4` | The 33-second, natively composed **9:16 cut for Instagram Reels and YouTube Shorts** (1080×1920; not cropped from the master) |
| `sheets/` | One 4×4 contact sheet per scene per cut — the reviewable set (13 keynote + 4 Twitter + 6 LinkedIn + 4 Instagram + 4 Reels/Shorts) |
| `manifest.txt` | The census: frames, shapes, glyph runs, layers, median ms — every number derived, none typed |
| `samples.txt` | The census's raster samples: `scene, draw commands, milliseconds` |

Source: `vieww_base/examples/wright_film/` — `cargo run --release -p
wright_film -- census`, then `-- master` (the keynote + teaser), then
`-- twitter` / `-- linkedin` / `-- instagram` / `-- reels` for the cuts (see its
`main.rs`). The output root is `WRIGHT_FILM_OUT` (default
`wright_film_out/`); the render is resumable — a scene whose segment and
sheet exist is skipped.

## The five movements

| movement | scenes | the sentence it carries |
|---|---|---|
| I · THE ASK | W00–W03 | What should exist? The words become a typed blueprint you never write; nothing runs until you have read the Confirm Gate in plain words. |
| II · THE WORKFORCE | W04–W05 | You compose the three agents; the compiler wraps them in the five. Budgets, permissions, the gate — drawn. |
| III · THE RUN | W06–W08 | **The actual window.** The run card is a projection of a real firing's event spine; the Mailer's approval card is dragged right; the engine view opens live. |
| IV · THE PROOF | W09–W10 | The receipt — `wright verify`'s six checks in their own words, read from the store at film start, sealed Ed25519. |
| V · THE REFIRE | W11–W12 | "Make the summaries punchier" — three steps refire, the rest is cached, free. The end card holds. |

## The short cuts — one film, four doors

The social cuts are not re-renders of a smaller film; they are the
keynote's **own scene builders**, unretimed, on shorter shot lists — the
same frames the keynote holds, choosing what each platform stops for:

- **X (`wright_twitter.mp4`, 0:41)** — the autoplay feed: the question,
  the typed request, the gate, the mark. A feed stops for a question it
  can finish in its head.
- **LinkedIn (`wright_linkedin.mp4`, 1:01)** — the buyer's cut: every
  trust artifact end to end — the gate in plain words, the bounds drawn,
  the receipt's six checks, the delivered result in the real window, the
  refire economics, the mark.
- **Instagram (`wright_instagram.mp4`, 0:33)** — the feed's square. Four
  scenes written for the 1080×1080 frame: the question, the typed
  request, **the real window framed like a print** (the same mounted
  shell as the keynote's Wright act, placed for the square), and the
  mark centred.
- **Reels / YouTube Shorts (`wright_reels.mp4`, 0:33)** — a native
  1080×1920 portrait composition, not a crop. The opening mark and end card
  share the same horizontal centre; the actual application sits in a readable
  922×576 panel with dedicated title and receipt safe areas.

## The honesty rules

- **Movement III is not a mock.** The film mounts the real
  `wright::ui` shell (`Ui`, `shell::root`, the real signals) on its own
  `FrameDriver` and drives it with a frame-indexed script — the pattern
  `viewwstudio` set for the launch film's studio act. What you see is
  the product. The Instagram and Reels/Shorts cuts do the same, placed for their own canvases.
- **The run is real.** Before the first frame, the film fires the
  repository's own HN-digest example through `wright_engine::fire`
  against the engine's mock provider (a local sink stands in for the
  news site and the mail API). The spine the run card projects, and the
  receipt's own sentences, come from that firing's store — the same
  store the CLI reads. When the film says "delivered and verified", a
  machine can check it.
- **A frame is a pure function of its index.** Nothing consults a wall
  clock; the census can re-derive any of it. The dissolve grammar is
  one rule for the whole film: the previous scene's final frame
  cross-dissolves into the new scene's live frame, and the Wright act
  enters as a window growing out of the predecessor's content box, its
  corners curving in.
- **The palette is the product's.** Every colour the chrome draws is
  Wright's own (`apps/wright/app/src/ui/theme.rs`, VS Code Dark Modern
  by role): ground `#1F1F1F`, accent `#0078D4`, good `#89D185`, wait
  `#CCA700`, bad `#F14C4C`. The mark is the app's own W geometry,
  imported (`wright::ui::logo::W`), not redrawn — the film and the
  product cannot drift apart. The film's room sits one step darker than
  the app's ground so the product is always the brightest thing in its
  own film.
- **The mark is centred where a mark belongs.** The opening scene and
  the end card hold the one composition a brand owns — the mark alone
  on the frame's own axis, the wordmark under it — so the film ends the
  way it began. The social cuts' opening and closing scenes hold the
  same axis on their own canvases.
- **The light rule.** No radial glows (the launch film's banding
  lesson); light is drawn — solid cores, crisp rings, the bloom's own
  physics in the finishing pass, `aq-mode=3` on the encoder so dark
  ramps keep their bits.
