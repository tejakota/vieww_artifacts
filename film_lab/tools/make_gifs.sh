#!/usr/bin/env bash
# make_gifs.sh — the third artifact, batch-made.
#
# Every film_lab render ships THREE receipts per experiment:
#   anim.gif    the motion receipt — loops inline in browsers and GitHub,
#               no codec needed, and at the lab's flat-colour vector
#               content measurably smaller than an equivalent mp4
#   sheet.png   the audit surface — motion cannot be watched by the
#               assistant, so the contact sheet stays the VLM door
#   metrics.txt the measured numbers — the gif= line is appended here
#
# House recipe (settled 2026-09-25 through the audit loop — see
# worklog "Round 6 — addendum" for the full ladder):
#   two-pass palette GIF, palettegen stats_mode=full (dark plates have
#   big static grounds — diff would starve the palette), paletteuse
#   dither=floyd_steinberg:diff_mode=rectangle, scale 640 wide, infinite
#   loop. floyd won the measured bake-off: smaller than bayer:5 on 34 of
#   36 plates AND error-diffusion is the anti-banding dither — the
#   aurora A/B audit flagged bayer:5's ordered crosshatch on smooth
#   gradients. One plate (aurora: intentional grain over smooth ramps)
#   exceeds the GIF 256-colour floor at every dither/width tried — its
#   audit surface of record stays the full-res sheet.
#
# usage: tools/make_gifs.sh <frames_root> [renders_dest]
#   <frames_root>   dir of <name>/frame_%03d.png (e.g. the harness out_root)
#   [renders_dest]  dir of <name>/metrics.txt to receive anim.gif + the
#                   gif= receipt line (e.g. the repo's film_lab/renders)
set -u

FRAMES_ROOT=${1:?usage: make_gifs.sh <frames_root> [renders_dest]}
RENDERS_DEST=${2:-}

fps_for() {
  case "$1" in
    hero)   echo 6 ;;   # 8 frames — a calmer loop
    hero4k) echo 2 ;;   # 2 frames — a slow A/B flip, not a blink
    *)      echo 12 ;;
  esac
}

total=0; count=0; failures=0
printf "%-12s %6s %4s %10s  %s\n" NAME FRAMES FPS BYTES DIMS
for dir in "$FRAMES_ROOT"/*/; do
  name=$(basename "$dir")
  [ -f "$dir/frame_000.png" ] || continue
  fps=$(fps_for "$name")
  n=$(ls "$dir"frame_*.png 2>/dev/null | wc -l)
  gif="$dir/anim.gif"
  ffmpeg -y -loglevel error -framerate "$fps" -i "$dir/frame_%03d.png" \
    -vf "scale=640:-1:flags=lanczos,split[a][b];[a]palettegen=stats_mode=full[p];[b][p]paletteuse=dither=floyd_steinberg:diff_mode=rectangle" \
    -loop 0 "$gif" || { echo "$name: ffmpeg FAILED"; failures=$((failures+1)); continue; }
  bytes=$(stat -c%s "$gif")
  dims=$(ffprobe -v error -select_streams v:0 -show_entries stream=width,height -of csv=p=0 "$gif")
  printf "%-12s %6s %4s %10s  %s\n" "$name" "$n" "$fps" "$bytes" "${dims/,/x}"
  total=$((total+bytes)); count=$((count+1))

  if [ -n "$RENDERS_DEST" ] && [ -d "$RENDERS_DEST/$name" ]; then
    cp "$gif" "$RENDERS_DEST/$name/anim.gif"
    # the measured receipt line — idempotent (re-runs replace, not append)
    m="$RENDERS_DEST/$name/metrics.txt"
    if [ -f "$m" ]; then
      grep -v '^gif=' "$m" > "$m.tmp" && mv "$m.tmp" "$m"
      printf 'gif=anim.gif,%s,%sfps,%s\n' "${dims/,/x}" "$fps" "$bytes" >> "$m"
    fi
  fi
done

echo "----------------------------------------------------------------"
echo "$count gifs · $total bytes total ($((total / 1024 / 1024)) MB) · $failures failure(s)"
[ "$failures" -eq 0 ] || exit 1
