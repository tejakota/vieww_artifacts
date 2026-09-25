#!/usr/bin/env bash
# Fetch CLIP ViT-B/32 so SnapSearch can search by meaning rather than by
# colour.
#
#   ./scripts/fetch-clip.sh [target-dir]     # default: ./models/clip-vit-base-patch32
#
# Then:
#   export SNAPSEARCH_CLIP_DIR="$PWD/models/clip-vit-base-patch32"
#   cargo run --features clip
#
# Roughly 600MB. Everything runs on-device afterwards — no photo and no
# query leaves the machine.
set -euo pipefail

DIR="${1:-models/clip-vit-base-patch32}"
REPO="https://huggingface.co/openai/clip-vit-base-patch32/resolve/main"

# `tokenizer.json` is preferred; `vocab.json` + `merges.txt` are the older
# layout and the app reads either. Both are fetched because the repository
# publishes both and neither is large.
FILES=(model.safetensors tokenizer.json vocab.json merges.txt)

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "error: this script needs '$1' on PATH" >&2
    exit 1
  }
}
need curl

mkdir -p "$DIR"

for file in "${FILES[@]}"; do
  target="$DIR/$file"
  if [ -s "$target" ]; then
    echo "have    $file"
    continue
  fi
  echo "fetch   $file"
  # `--fail` so an HTML error page is never written out as if it were a
  # model: a truncated or wrong-content safetensors fails much later and
  # much less clearly, inside the memory map.
  if ! curl --fail --location --progress-bar --output "$target.part" "$REPO/$file"; then
    rm -f "$target.part"
    # vocab/merges are optional when tokenizer.json arrived.
    if [ "$file" = "vocab.json" ] || [ "$file" = "merges.txt" ]; then
      echo "        (optional, skipping)"
      continue
    fi
    echo "error: could not fetch $file" >&2
    exit 1
  fi
  mv "$target.part" "$target"
done

if [ ! -s "$DIR/model.safetensors" ]; then
  echo "error: $DIR/model.safetensors is missing or empty" >&2
  exit 1
fi

if [ ! -s "$DIR/tokenizer.json" ] && { [ ! -s "$DIR/vocab.json" ] || [ ! -s "$DIR/merges.txt" ]; }; then
  echo "error: no usable tokenizer in $DIR" >&2
  exit 1
fi

cat <<EOF

Done. $DIR

  export SNAPSEARCH_CLIP_DIR="\$(cd "$DIR" && pwd)"
  cargo run --features clip

The search sheet will say "Matching on meaning · CLIP ViT-B/32" once it is
loading. If it still says "local appearance", the weights were not found and
the app fell back rather than failing — the reason is printed on stderr.
EOF
