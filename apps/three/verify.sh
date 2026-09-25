#!/usr/bin/env bash
set -euo pipefail

command -v cargo >/dev/null || { echo "error: cargo is required" >&2; exit 127; }
command -v rustfmt >/dev/null || { echo "error: rustfmt is required" >&2; exit 127; }

cargo fmt --all -- --check
cargo test
cargo test -p three-social -p three-app

if [[ -d ../vieww-develop/crates/vieww ]]; then
  cargo test --workspace
  echo "Vieww integration present: full workspace tests passed."
else
  echo "Vieww checkout not found at ../vieww-develop; skipped Vieww-dependent workspace tests."
  echo "Place Vieww there, then run: cargo test --workspace"
fi
