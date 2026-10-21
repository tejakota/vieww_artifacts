"""A tiny random Qwen3 in GGUF, with the real Qwen vocabulary, to compare
Wright's engine against llama.cpp token for token."""
import sys, numpy as np, gguf
from gguf import GGUFReader, GGUFWriter, GGMLQuantizationType as Q

vocab_path, out, qtype, seed = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])
OS = float(sys.argv[5]) if len(sys.argv) > 5 else 0.1  # output-projection scale (0.6: strong attention)
r = GGUFReader(vocab_path)
def field(name):
    f = r.fields[name]
    return [bytes(f.parts[i]).decode() for i in f.data] if f.types[0] == gguf.GGUFValueType.ARRAY and f.types[-1] == gguf.GGUFValueType.STRING else [int(f.parts[i][0]) for i in f.data]
tokens = field("tokenizer.ggml.tokens"); types = field("tokenizer.ggml.token_type"); merges = field("tokenizer.ggml.merges")
V, D, H, KV, HD, FF, L = len(tokens), 64, 4, 2, 32, 128, 2
rng = np.random.default_rng(seed)
w = GGUFWriter(out, "qwen3")
w.add_block_count(L); w.add_context_length(4096); w.add_embedding_length(D); w.add_feed_forward_length(FF)
w.add_head_count(H); w.add_head_count_kv(KV); w.add_key_length(HD); w.add_value_length(HD)
w.add_rope_freq_base(1000000.0); w.add_layer_norm_rms_eps(1e-6); w.add_file_type(7 if qtype == "q8_0" else 0)
w.add_tokenizer_model("gpt2"); w.add_tokenizer_pre("qwen2"); w.add_token_list(tokens); w.add_token_types(types); w.add_token_merges(merges)
w.add_eos_token_id(tokens.index("<|endoftext|>")); w.add_add_bos_token(False)
def mat(name, rows, cols, scale):
    a = (rng.standard_normal((rows, cols)) * scale).astype(np.float32)
    if qtype == "q8_0":
        w.add_tensor(name, gguf.quants.quantize(a, Q.Q8_0), raw_dtype=Q.Q8_0)
    elif qtype == "f16":
        w.add_tensor(name, a.astype(np.float16))
    else:
        w.add_tensor(name, a)
def vec(name, n):
    w.add_tensor(name, (1.0 + 0.2 * rng.standard_normal(n)).astype(np.float32))
mat("token_embd.weight", V, D, 1.0)
for i in range(L):
    p = f"blk.{i}."
    vec(p + "attn_norm.weight", D)
    mat(p + "attn_q.weight", H * HD, D, 0.3); mat(p + "attn_k.weight", KV * HD, D, 0.3); mat(p + "attn_v.weight", KV * HD, D, 0.3)
    mat(p + "attn_output.weight", D, H * HD, OS)
    vec(p + "attn_q_norm.weight", HD); vec(p + "attn_k_norm.weight", HD)
    vec(p + "ffn_norm.weight", D)
    mat(p + "ffn_gate.weight", FF, D, 0.3); mat(p + "ffn_up.weight", FF, D, 0.3); mat(p + "ffn_down.weight", D, FF, OS)
vec("output_norm.weight", D)
w.write_header_to_file(); w.write_kv_data_to_file(); w.write_tensors_to_file(); w.close()
print(out, V)
