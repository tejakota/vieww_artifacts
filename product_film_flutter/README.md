# viewwstudio product film (Flutter render)

The ~4:42 product film for **vieww Studio**, reproduced 1:1 from the app's
real UI (splash, editor, device-framed preview, receipts) and rendered
headlessly to MP4 with Flutter's own rasteriser.

- 22 scenes, five movements — question · old world · studio · engine · proof
- 1920x1080 @ 60 fps, silent, 16,921 frames (282.0 s)
- Every number on screen carries a repo citation (the receipt tags)
- `deliverables/` holds the finished cut and the 4x4 stride sheet

## Render

```sh
flutter test test/render_film_test.dart          # the whole film, one pass
FILM_SCENE=s09  ...                              # one scene, for review
FILM_T0=48 FILM_T1=89 ...                        # a range chunk, half-open
FILM_FPS=30 / FILM_CRF=18 / FILM_OUT=path ...    # capture controls
```

The harness pipes every frame as raw RGBA straight into ffmpeg — no PNGs on
disk, no wall clock, no dropped frames. The film is a pure function of
absolute time (`FilmClock.t`), so chunks concat seam-free:

```sh
scripts/render_chunks.sh all && scripts/render_chunks.sh concat
```

## Layout

| path | role |
| --- | --- |
| `lib/theme.dart` | the app's exact palette, every constant source-cited |
| `lib/motion/vieww_motion.dart` | vieww's curves/springs, ported |
| `lib/film.dart` | one clock, one pure function of time |
| `lib/script.dart` | the scene table (durations, cuts) |
| `lib/widgets/` | primitives, brand mark, studio chrome, device frames |
| `lib/scenes/` | s01..s22, five movements |
| `test/render_film_test.dart` | the render harness |
