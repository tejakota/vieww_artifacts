#!/usr/bin/env bash
# Place the three workflows from this directory into .github/workflows/ —
# see README.md for why they are here and what authority the follow-up
# `git push` needs.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"
mkdir -p ../workflows
# With arguments, place just those (e.g. `restore.sh hardware.yml`); without,
# the original three.
files=("$@")
[[ ${#files[@]} -eq 0 ]] && files=(audio-backends.yml audio-plugins.yml framework-checks.yml)
for f in "${files[@]}"; do
    [[ -f "$f" ]] || { echo "missing $f" >&2; exit 1; }
    cp "$f" "../workflows/$f"
    rm "$f"
done

echo "placed ${#files[@]} workflow(s) into .github/workflows/: ${files[*]}"
echo "next: git add .github/workflows && git commit -m 'ci: place the three workflows' && git push"
echo "(the push needs a token with workflow scope, or the web UI)"
