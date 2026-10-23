# Changelog — vieww-music

## Unreleased — 2026-10-23

### Added
- The crate: music as mathematics and code, from Carnatic to rock.
- `pitch`, `scala`: just intonation, cents, Scala `.scl`/`.kbm` files.
- `carnatic`: swaras and their 16 names, the 72 melakartas by the
  chakra rule, ragas with arohana/avarohana grammar and vakra paths,
  gamakas as pitch curves, the 35 suladi talas, korvai and mora
  arithmetic that lands on sam.
- `western`: scales and modes, chords and Roman numerals, voice leading
  by dynamic programming, Euclidean and other rhythms.
- `rock`: riffs, power chords, 12-bar forms with drums and bass.
- `score`, `generate`, `render`: scores in any tuning with MPE and
  gamaka bends, generators, offline rendering with EBU R128 loudness
  normalisation and an exact true-peak ceiling, WAV and FLAC output.
- `Score::tags`: rendered files carry title, raga, tala, genre and
  encoder metadata.
- Examples: `carnatic`, `chorale`, `rock`.
