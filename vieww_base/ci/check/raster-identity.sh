#!/usr/bin/env bash
#
# Does this change move any pixel the CPU rasterizer draws?
#
#   ci/check/raster-identity.sh [BASE]      # BASE defaults to origin/main
#
# Renders the fixture gallery (`examples/fixtures`: every reference screen
# and animation) at BASE and at the working tree, and compares the two sets
# of images byte for byte. Exits 0 when they are identical, 1 when any image
# differs or exists on one side only, listing them.
#
# This is the gate behind docs/FIDELITY.md's first rule: **the CPU rasterizer
# is byte-identical across changes by default.** A speed-up — SIMD, threads,
# a cheaper blur — must leave every byte where it was. A change that is meant
# to move pixels says so in its commit message with a `Pixels-Changed:`
# trailer naming what moved and why; with `--allow-changed` this script then
# reports the differences without failing, so the trailer, not the script,
# carries the decision.
#
# BASE is built in a temporary git worktree with a target directory of its
# own, deleted afterwards. Sharing one is faster and wrong: cargo reuses a
# build script compiled for one checkout in the other, with whatever paths it
# baked in.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
repo="$(git -C "$root" rev-parse --show-toplevel)"
# The workspace's path inside the repository (vieww_base/ in this one).
inside="$(realpath --relative-to="$repo" "$root")"

base="origin/main"
allow=0
for arg in "$@"; do
    case "$arg" in
        --allow-changed) allow=1 ;;
        -h|--help) sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) base="$arg" ;;
    esac
done

work="$(mktemp -d)"
cleanup() {
    git -C "$repo" worktree remove --force "$work/base" > /dev/null 2>&1 || true
    rm -rf "$work"
}
trap cleanup EXIT

render() { # $1 = workspace dir, $2 = target dir, $3 = output dir
    mkdir -p "$3"
    (cd "$1" && cargo run -q --release -p fixtures --target-dir "$2" -- "$3" > "$3.log" 2>&1) || {
        echo "raster-identity: the gallery failed to render in $1 (see $3.log)" >&2
        tail -20 "$3.log" >&2
        exit 2
    }
}

echo "raster-identity: rendering the gallery at $base"
git -C "$repo" worktree add -q --detach "$work/base" "$base" > /dev/null 2>&1
target="${CARGO_TARGET_DIR:-$root/target}"
render "$work/base/$inside" "$work/target-base" "$work/out-base"

echo "raster-identity: rendering the gallery at the working tree"
render "$root" "$target" "$work/out-head"

changed=0
while IFS= read -r file; do
    rel="${file#"$work/out-base/"}"
    if [ ! -f "$work/out-head/$rel" ]; then
        echo "  only at $base: $rel"; changed=1
    elif ! cmp -s "$file" "$work/out-head/$rel"; then
        echo "  differs: $rel"; changed=1
    fi
done < <(find "$work/out-base" -type f | sort)
while IFS= read -r file; do
    rel="${file#"$work/out-head/"}"
    [ -f "$work/out-base/$rel" ] || { echo "  only in the working tree: $rel"; changed=1; }
done < <(find "$work/out-head" -type f | sort)

count="$(find "$work/out-head" -type f | wc -l | tr -d ' ')"
if [ "$changed" = 0 ]; then
    echo "raster-identity: all $count files identical to $base"
    exit 0
fi
if [ "$allow" = 1 ]; then
    echo "raster-identity: images differ from $base (allowed: the commit carries Pixels-Changed:)"
    exit 0
fi
echo "raster-identity: images differ from $base. If that is intended, say what moved in a" >&2
echo "  'Pixels-Changed:' commit trailer and rerun with --allow-changed (docs/FIDELITY.md)." >&2
exit 1
