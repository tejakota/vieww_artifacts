"""A tiny random Whisper in whisper.cpp's ggml format, with the real mel
filters and vocabulary (taken from whisper.cpp's for-tests-ggml-base.bin),
to compare Wright's engine against whisper.cpp token for token.

    python3 make_tiny_whisper.py for-tests-ggml-base.bin out.bin f32|f16 seed [width heads layers]"""
import struct, sys
import numpy as np

src, out, kind, seed = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])
b = open(src, "rb").read()
at = 0
def take(n):
    global at
    s = b[at:at + n]; at += n; return s
def i32(): return struct.unpack("<i", take(4))[0]
assert struct.unpack("<I", take(4))[0] == 0x67676D6C
hp = [i32() for _ in range(11)]  # n_vocab, a_ctx, a_state, a_head, a_layer, t_ctx, t_state, t_head, t_layer, n_mels, ftype
n_mel, n_fft = i32(), i32()
filters = take(4 * n_mel * n_fft)
n_vocab = i32()
vocab = []
for _ in range(n_vocab):
    n = struct.unpack("<I", take(4))[0]; vocab.append(take(n))

V, A_CTX, T_CTX, MELS = hp[0], 1500, 448, hp[9]
# Width, heads, layers: tiny by default; `512 8 6` is base-sized (for timing).
S, H, L = (int(sys.argv[5]), int(sys.argv[6]), int(sys.argv[7])) if len(sys.argv) > 7 else (64, 4, 2)
f16 = kind == "f16"
rng = np.random.default_rng(seed)
o = open(out, "wb")
o.write(struct.pack("<I", 0x67676D6C))
o.write(struct.pack("<11i", V, A_CTX, S, H, L, T_CTX, S, H, L, MELS, 1 if f16 else 0))
o.write(struct.pack("<ii", n_mel, n_fft)); o.write(filters)
o.write(struct.pack("<i", n_vocab))
for t in vocab:
    o.write(struct.pack("<I", len(t))); o.write(t)
def tensor(name, a, half):
    a = np.asarray(a, dtype=np.float16 if half else np.float32)
    ne = list(reversed(a.shape))
    nm = name.encode()
    o.write(struct.pack("<iii", len(ne), len(nm), 1 if half else 0))
    o.write(struct.pack(f"<{len(ne)}i", *ne)); o.write(nm); o.write(a.tobytes())
def mat(name, rows, cols, scale): tensor(name, rng.standard_normal((rows, cols)) * scale, f16)
def vec(name, n, mean=0.0, scale=0.1): tensor(name, mean + scale * rng.standard_normal(n), False)
def pe(n, d):  # sinusoids, as Whisper's encoder uses
    t = np.arange(n)[:, None] / (10000 ** (np.arange(d // 2)[None, :] / (d // 2 - 1)))
    return np.concatenate([np.sin(t), np.cos(t)], axis=1)
tensor("encoder.positional_embedding", pe(A_CTX, S), False)
tensor("encoder.conv1.weight", rng.standard_normal((S, MELS, 3)) * 0.2, f16)
tensor("encoder.conv1.bias", 0.1 * rng.standard_normal((S, 1)), False)
tensor("encoder.conv2.weight", rng.standard_normal((S, S, 3)) * 0.1, f16)
tensor("encoder.conv2.bias", 0.1 * rng.standard_normal((S, 1)), False)
def block(p, cross):
    for part in (["attn", "cross_attn"] if cross else ["attn"]):
        vec(f"{p}.{part}_ln.weight", S, 1.0); vec(f"{p}.{part}_ln.bias", S)
        mat(f"{p}.{part}.query.weight", S, S, 0.3); vec(f"{p}.{part}.query.bias", S)
        mat(f"{p}.{part}.key.weight", S, S, 0.3)
        mat(f"{p}.{part}.value.weight", S, S, 0.3); vec(f"{p}.{part}.value.bias", S)
        mat(f"{p}.{part}.out.weight", S, S, 0.3); vec(f"{p}.{part}.out.bias", S)
    vec(f"{p}.mlp_ln.weight", S, 1.0); vec(f"{p}.mlp_ln.bias", S)
    mat(f"{p}.mlp.0.weight", 4 * S, S, 0.3); vec(f"{p}.mlp.0.bias", 4 * S)
    mat(f"{p}.mlp.2.weight", S, 4 * S, 0.15); vec(f"{p}.mlp.2.bias", S)
for i in range(L): block(f"encoder.blocks.{i}", False)
vec("encoder.ln_post.weight", S, 1.0); vec("encoder.ln_post.bias", S)
tensor("decoder.positional_embedding", 0.3 * rng.standard_normal((T_CTX, S)), False)
mat("decoder.token_embedding.weight", V, S, 1.0)
for i in range(L): block(f"decoder.blocks.{i}", True)
vec("decoder.ln.weight", S, 1.0); vec("decoder.ln.bias", S)
o.close()
print(out, hp)
