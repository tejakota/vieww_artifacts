This folder consists of real apps that must be deployed in mobiles and
desktops (if supported).

## The four apps

| app | what it is | status |
|---|---|---|
| **3** (`three/`) | temporal 3D capture and the `.3` binary container, plus the social product around it — profiles, feeds, remixes. `three-vieww` owns media state + presentation prep for Vieww. | foundation + social core working; real camera / network adapters isolated behind traits |
| **vavlt** (`vavlt/`) | consent-first photo/video vault. One Vieww UI (`crates/app`) mounted by desktop, Android and iOS hosts; codec measurement harness with audit chain. | desktop measurement harness + full UI; see `SPEC.md` |
| **upxcale** (`upxcale/`) | photo upscaler — Lanczos-3 resampler + unsharp mask behind a real render flow (grid → picker → progress → compare). | 20 tests pass; every screen state renders through vieww's CPU backend |
| **SnapSearch** (`snapsearch/`) | find a photo by describing it, or by handing it one. Joint embedding space; local appearance encoder by default, CLIP ViT-B/32 behind `--features clip`. | 38 tests (47 with clip); retrieval tested, not just plumbing |

## renders/

Each app ships the three receipts, following the `film_lab/renders`
convention:

- `sheet.png` — the audit surface: every screen of the app's story on one
  dark plate, numbered and captioned.
- `anim.gif` — the motion receipt: the app's flow as a seamless loop
  (two-pass palette GIF, floyd-steinberg dither — the house recipe).
- `metrics.txt` — the measured numbers, including the `gif=` receipt line.

Where the material existed it was used directly: **upxcale**'s sheet and
GIF are built from the five real screen states in `upxcale/shots/`;
**vavlt**'s from the two design mockups in `vavlt/design/`. Where it did
not, it was reproduced from the source: **3**'s moment frames re-render
the `demo_capture()` spec from `crates/three-runtime` (9×9 sine mesh, 32
deterministic motes, orbiting camera) and the screens follow the social
app's own strings; **SnapSearch**'s sample tiles are the ten concepts
from `src/concept.rs` in their exact colours, and the screens carry the
app's real UI strings.

The archives these folders replaced were removed once extracted.
