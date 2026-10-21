#!/usr/bin/env bash
# Tiny random models in the real formats, and what the reference
# implementations make of them — the fixtures `cargo test -p wright-infer`
# compares against (WRIGHT_TINY_DIR, WRIGHT_VOCAB_DIR, WRIGHT_LLAMA_TOKENIZE).
#
#   scripts/models/fixtures.sh <references dir> <out dir>
#
# Needs python3 with `gguf` and `numpy` (pip install gguf numpy) and a C++
# compiler. The references come from references.sh.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
ref="$(cd "${1:?usage: fixtures.sh <references dir> <out dir>}" && pwd)"
mkdir -p "${2:?}"
out="$(cd "$2" && pwd)"
L="$ref/llama.cpp"; W="$ref/whisper.cpp"
export LD_LIBRARY_PATH="$L/build/bin:$W/build/bin:${LD_LIBRARY_PATH:-}"
export DYLD_LIBRARY_PATH="$LD_LIBRARY_PATH"
vocab="$L/models/ggml-vocab-qwen2.gguf"

# ── Qwen3 ──
for q in q8_0 f16 f32; do python3 -I "$here/make_tiny_qwen3.py" "$vocab" "$out/tiny-$q.gguf" "$q" 7 >/dev/null; done
python3 -I "$here/make_tiny_qwen3.py" "$vocab" "$out/strong-q8_0.gguf" q8_0 11 0.6 >/dev/null
P="Hello, my name is"
for q in q8_0 f16 f32; do
  got="$("$L/build/bin/llama-simple" -m "$out/tiny-$q.gguf" -n 24 "$P" 2>/dev/null)"
  printf '%s\n%s\n' "$P" "${got#"$P"}" > "$out/tiny-$q.expect"
done
c++ -O2 -std=c++17 "$here/llama_logits.cpp" -I"$L/include" -I"$L/ggml/include" -L"$L/build/bin" -lllama -lggml -lggml-base -o "$out/llama_logits"
# The strong model's token sequence: the tokenizer's ids for a long mixed prompt.
P2="The quick brown fox jumps over the lazy dog. 敏捷的棕色狐狸跳过了懒狗。 def f(x): return x*2 # 1234567890 — and then, the story continues with"
ids="$("$L/build/bin/llama-tokenize" -m "$vocab" -p "$P2" --ids --log-disable --no-escape | tail -1 | tr -d '[],')"
{ echo "$ids"; "$out/llama_logits" "$out/strong-q8_0.gguf" $ids 2>/dev/null; } > "$out/strong-q8_0.logits"

# ── Whisper ──
cp "$W/samples/jfk.wav" "$out/"
for k in f32 f16; do python3 -I "$here/make_tiny_whisper.py" "$W/models/for-tests-ggml-base.bin" "$out/whisper-tiny-$k.bin" "$k" 3 >/dev/null; done
c++ -O2 -std=c++17 "$here/whisper_logits.cpp" -I"$W/include" -I"$W/ggml/include" -L"$W/build/bin" -lwhisper -lggml -lggml-base -o "$out/whisper_logits"
python3 -I - "$out/jfk.wav" "$out/jfk.f32" <<'PY'
import struct, sys, array
b = open(sys.argv[1], "rb").read(); i = 12
while i < len(b):
    cid, n = b[i:i+4], struct.unpack("<I", b[i+4:i+8])[0]
    if cid == b"data":
        a = array.array("h"); a.frombytes(b[i+8:i+8+n])
        open(sys.argv[2], "wb").write(struct.pack(f"<{len(a)}f", *[x / 32768.0 for x in a]))
    i += 8 + n + (n & 1)
PY
TOK="50258 50259 50359 50363 400 1029 452 12 18 11 286 1203 1652 2302 1883 1203 2302 586 2603 13 50257"
for k in f32 f16; do
  { echo "$TOK"; "$out/whisper_logits" "$out/whisper-tiny-$k.bin" "$out/jfk.f32" $TOK 2>/dev/null; } > "$out/whisper-tiny-$k.logits"
done
echo "fixtures in $out"
