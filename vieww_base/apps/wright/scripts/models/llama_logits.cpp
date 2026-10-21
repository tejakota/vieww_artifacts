// Teacher-forced logits from llama.cpp: argv model, then token ids. Prints,
// for each position, the top-2 tokens and their logits.
#include "llama.h"
#include <cstdio>
#include <cstdlib>
#include <vector>
#include <algorithm>
int main(int argc, char ** argv) {
    llama_backend_init();
    auto mp = llama_model_default_params();
    llama_model * m = llama_model_load_from_file(argv[1], mp);
    auto cp = llama_context_default_params();
    cp.n_ctx = 512; cp.n_batch = 512;
    if (getenv("NOFA")) cp.flash_attn_type = LLAMA_FLASH_ATTN_TYPE_DISABLED;
    llama_context * ctx = llama_init_from_model(m, cp);
    std::vector<llama_token> toks;
    for (int i = 2; i < argc; i++) toks.push_back(atoi(argv[i]));
    llama_batch b = llama_batch_init(toks.size(), 0, 1);
    for (size_t i = 0; i < toks.size(); i++) { b.token[i] = toks[i]; b.pos[i] = i; b.n_seq_id[i] = 1; b.seq_id[i][0] = 0; b.logits[i] = 1; }
    b.n_tokens = toks.size();
    llama_decode(ctx, b);
    int nv = llama_vocab_n_tokens(llama_model_get_vocab(m));
    for (size_t i = 0; i < toks.size(); i++) {
        float * l = llama_get_logits_ith(ctx, i);
        std::vector<int> idx(nv); for (int j = 0; j < nv; j++) idx[j] = j;
        std::partial_sort(idx.begin(), idx.begin() + 2, idx.end(), [&](int a, int c){ return l[a] > l[c]; });
        printf("%zu %d %.5f %d %.5f\n", i, idx[0], l[idx[0]], idx[1], l[idx[1]]);
    }
}
