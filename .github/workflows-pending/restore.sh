#!/usr/bin/env bash
# Place the three workflows from this directory into .github/workflows/ —
# see README.md for why they are here and what authority the follow-up
# `git push` needs.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"
mkdir -p ../workflows
for f in audio-backends.yml audio-plugins.yml framework-checks.yml; do
    [[ -f "$f" ]] || { echo "missing $f" >&2; exit 1; }
    cp "$f" "../workflows/$f"
    rm "$f"
done

echo "placed 3 workflows into .github/workflows/"
echo "next: git add .github/workflows && git commit -m 'ci: place the three workflows' && git push"
echo "(the push needs a token with workflow scope, or the web UI)"
