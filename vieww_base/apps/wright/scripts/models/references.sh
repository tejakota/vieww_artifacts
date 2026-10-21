#!/usr/bin/env bash
# Build the two reference implementations Wright's built-in engine is
# checked against: llama.cpp (Qwen3, the tokenizer) and whisper.cpp
# (Whisper), at pinned commits.
#
#   scripts/models/references.sh <dir>     -> <dir>/llama.cpp, <dir>/whisper.cpp
set -euo pipefail
dir="${1:?usage: references.sh <dir>}"
LLAMA_COMMIT=339f3242c761f16c27e5ffcde1d35dcc31933a55   # 2026-10-20
WHISPER_COMMIT=231638a0717b7822784297e5980965ffcec05810 # 2026-10-19
mkdir -p "$dir"
cd "$dir"
fetch() { # repo commit name
  if [ ! -d "$3/.git" ]; then git init -q "$3"; git -C "$3" remote add origin "$1"; fi
  git -C "$3" fetch -q --depth 1 origin "$2"
  git -C "$3" checkout -q FETCH_HEAD
}
fetch https://github.com/ggml-org/llama.cpp "$LLAMA_COMMIT" llama.cpp
fetch https://github.com/ggml-org/whisper.cpp "$WHISPER_COMMIT" whisper.cpp
jobs="$(nproc 2>/dev/null || sysctl -n hw.ncpu)"
cmake -S llama.cpp -B llama.cpp/build -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=ON -DLLAMA_CURL=OFF -DLLAMA_BUILD_SERVER=OFF -DGGML_NATIVE=ON >/dev/null
cmake --build llama.cpp/build -j "$jobs" --target llama-simple llama-tokenize >/dev/null
cmake -S whisper.cpp -B whisper.cpp/build -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=ON -DGGML_NATIVE=ON >/dev/null
cmake --build whisper.cpp/build -j "$jobs" --target whisper-cli >/dev/null
echo "references built in $dir"
