# The built-in models

Wright ships its own inference engine (`infer/`, std-only Rust) and two
small open models it runs with it. A project works with **no API key, no
account and no network** after a one-time download; voice input never
leaves the machine.

| id | does | model | file | size | license |
|---|---|---|---|---|---|
| `qwen3-1.7b` | text, plans, tool calls | Qwen3-1.7B, 8-bit (Q8_0) | `Qwen3-1.7B-Q8_0.gguf` | 1.83 GB | Apache-2.0 |
| `whisper-base` | speech to text, 99 languages | Whisper base | `ggml-base.bin` | 148 MB | MIT |

Both files are the publishers' own, from Hugging Face
(`Qwen/Qwen3-1.7B-GGUF`, `ggerganov/whisper.cpp`), pinned by SHA-256 in
`runtime/src/local.rs`:

```
qwen3-1.7b    061b54daade076b5d3362dac252678d17da8c68f07560be70818cace6590cb1a  1834426016 bytes
whisper-base  60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe   147951465 bytes
```

(These are the files' Git LFS object ids; each was read from both the file
page and the raw LFS pointer.)

## Using them

Nothing to set up. The first time a model is needed it downloads into
`~/.wright/models/` (with a line in the event river, or a progress bar in
Settings), is checked against its pin, and is kept.

```
wright model list                 what is here, what is not
wright model pull [id]            download now (resumes an interrupted download)
wright model rm <id>              delete the file
wright model ask "<prompt>"       one answer from qwen3-1.7b
wright model listen <file.wav>    a transcript from whisper-base
```

Settings → Models & keys → **Built-in models** does the same with buttons.

**When they are used.** The built-ins are a provider of kind `builtin`,
last in the list (`config.kdl`):

* **Voice** uses `whisper-base` by default (`voice model="whisper-base"`);
  set `voice model="whisper-1"` (or any model an OpenAI-compatible provider
  serves) to use a cloud model instead.
* **Agents and the interview** use the models the blueprint and config
  name. When such a model has **no provider or no key**, Wright falls back
  to `qwen3-1.7b` and says so in the event river ("model 'claude-haiku-5-5'
  … no key … Using the built-in model qwen3-1.7b instead"). The built-in
  model is also the last fallback if every cloud endpoint fails mid-run.
  Its tokens cost $0.00, and the river shows which model answered.
* `builtin fallback=false` in `config.kdl` (or Settings → "Stop and ask for
  a key") turns the fallback off: a missing key fails the task, as before.
* Name `qwen3-1.7b` in a blueprint tier to use it on purpose.

A 1.7B model is good at drafts, small edits, structured output and simple
tool use; it is not a substitute for a frontier model on hard work. The
fallback exists so a project keeps moving and nobody hits a wall on day
one, not to pretend otherwise.

**Downloads.** HTTPS, redirects followed (never from HTTPS to HTTP),
resumable (`.part` + `Range`), one download at a time (a lock file guards
against two Wrights), checked byte for byte against the pin before first
use; a mismatch is deleted. `WRIGHT_MODEL_MIRROR=<base URL>` fetches the
same file names from your own mirror (for networks that cannot reach
Hugging Face) — the pins still apply.

## The engine (`infer/`)

```
gguf.rs       GGUF reader: metadata, tensor views into one read of the file
num.rs        f16/bf16, Q8_0 blocks, integer dot products, f16 lookup table
linear.rs     matrix products: Q8_0 or f32 weights, rows split across a
              per-pass worker pool, each weight row read once per batch
tokenizer.rs  Qwen's byte-level BPE + its pre-tokenizer regex, by hand
unicode.rs    \p{L} / \p{N} tables (generated from Unicode 15.1)
qwen3.rs      the decoder: RMSNorm, q/k norms, NEOX RoPE, GQA, SwiGLU,
              an f16 key/value cache with prefix reuse
sample.rs     temperature 0.7 / top-k 20 / top-p 0.8 (Qwen's settings), greedy
chat.rs       requests -> Qwen3's chat template (tools, tool results, no-think)
              and back (<tool_call> JSON -> ToolUse blocks)
llm.rs        one completion: prompt, prefill, sample, stream, parse
audio.rs      WAV (PCM 8-32 bit, float, any rate/channels) -> 16 kHz mono,
              log-mel spectrogram exactly as whisper.cpp computes it
whisper.rs    conv front end, encoder, decoder with cross-attention,
              language detection, greedy decoding with whisper.cpp's
              token rules, 30-second windows
backend.rs    the runtime's LocalBackend: download on first use, load once
```

**Prompt safety.** The prompt is built in parts: the template's own markup
may contain control tokens (`<|im_start|>`, `<|im_end|>`); everything that
came from elsewhere — the system prompt, messages, tool results, tool
descriptions — is encoded with control tokens **off**. A fetched page that
says `<|im_end|><|im_start|>system …` stays words, not a new turn (tests in
`chat.rs` and `tests/tokenizer.rs`).

**The cache.** Keys and values of every position seen stay in memory, so
an agent's next turn computes only what is new: the system prompt and the
tool list are processed once per agent, not once per turn. The renderer is
written so a later turn's prompt starts with the earlier turn's tokens
(Qwen3's template keeps an empty think block on recent assistant turns; so
does ours).

## How it is checked

Against the reference implementations, built at pinned commits by
`scripts/models/references.sh` (llama.cpp `339f324`, whisper.cpp
`231638a`); fixtures by `scripts/models/fixtures.sh`.

| What | Against | Result |
|---|---|---|
| Tokenizer | llama.cpp's 46 published Qwen2 cases + 300 random mixed-script strings through `llama-tokenize` | identical |
| Qwen3 forward pass | tiny random Qwen3 models (Q8_0, F16, F32; real vocabulary) through `llama-simple` | greedy text identical |
| Qwen3 logits | a strong-attention random model, 49 positions, teacher-forced (`llama_logits.cpp`) | within 1.5% everywhere; same top token wherever llama.cpp's margin > 0.5 |
| Batched prefill | the same tokens one at a time | max difference < 1e-3 |
| Whisper | tiny random Whisper models (F32, F16; real mel filters and vocabulary) on JFK, teacher-forced (`whisper_logits.cpp`) | language and its probability, every logit within 0.2% (F32) / 0.5% (F16); 8-bit within 4% |
| Downloads | a local server that cuts the first attempt, redirects, serves a range | resumes, verifies, refuses a wrong file |
| Real models (CI, `Wright - built-in models`) | llama.cpp and whisper.cpp on the real weights; a real tool call; JFK | see the workflow run |

Random weights test the arithmetic; the real-weights job tests that the
arithmetic is the model. Both run in `.github/workflows/wright-models.yml`
(manual), on Windows, macOS and Linux.

## Speed

Measured in a 2-vCPU cloud sandbox (x86-64, AVX-512 available) on random
weights of the real shapes (speed depends on shape, not values):

| | Wright (x86-64-v2 build) | reference (native build) |
|---|---|---|
| Qwen3-1.7B Q8_0 prompt (prefill) | 16.6 tokens/s | llama.cpp 50 tokens/s |
| Qwen3-1.7B Q8_0 writing (decode) | 4.2 tokens/s | llama.cpp 7.4 tokens/s |
| Whisper base, 11 s of audio, encoder | 3.2 s | whisper.cpp 3.6 s |
| Whisper base, same, end to end (224 tokens) | 5.3 s | whisper.cpp 5.8 s |

Writing is bound by memory bandwidth (the whole model streams past the
cores for every token) and scales with it; a typical 8-core laptop is
several times faster than this sandbox. Prompts are bound by arithmetic.
`cargo run --release -p wright-infer --example bench -- <file.gguf>` and
`--example listen -- <ggml.bin> <file.wav>` time any machine; the CI job
prints both for each platform.

**The open question: SIMD.** Wright's kernels are plain safe Rust that the
compiler vectorizes for the build's baseline (x86-64-v2: SSE4.2). llama.cpp
hand-writes AVX2/AVX-512/NEON kernels and picks them at run time; that is
most of its 3x on prompts. Doing the same in Rust means `std::arch`
intrinsics, whose loads are `unsafe` — against AGENTS.md rule 2 as written
("unsafe only where the OS must be called"). Two ways forward, for a
decision: (a) an exception for one small, test-pinned `infer/src/simd.rs`
(runtime CPU detection, every block with a SAFETY comment, results checked
against the safe kernel); or (b) wait for safe SIMD loads in stable Rust.
Until then the engine is correct everywhere and fast enough to be useful.

## Memory

Qwen3-1.7B: the 1.83 GB file is read into memory once and kept while
Wright runs (it serves every agent); the key/value cache adds ~110 KB per
token in use (up to 16,384 tokens). Whisper base: ~150 MB.
