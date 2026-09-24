# vieww_artifacts

Artifacts for the vieww repository.

- `film_lab/` — the element experiments: **25 rendered plates** (rounds 2–5) with source, contact sheets, metrics receipts, the pixel-probe evidence plate, and the premium-UI reference run. Round 5 added the motion-persistence, settle-typography, dolly-vertigo, flow-field and instrument-panel plates.
- `vieww_base/` — the vieww framework source itself (the full Rust workspace: 36 crates, examples, docs, CI). **All updates to vieww land here.** The `todo-upgrades.md` items are fixed in this tree — each entry's status and the change that resolved it are annotated in place. Round 5 landed `guard_filtered_layers_below` (U-06), `SceneReport::open_subpath_fills` + `Path::open_subpaths` (U-20's counter), and the full-turn `Path::arc` closing (U-22 — the census's first catch).
- `todo-upgrades.md` — the bench's upgrade notes, annotated with resolutions. U-01 through U-22; the remaining OPEN item (U-18, the hairline policy) now carries the probe plate's measured rung-ink ladder.
