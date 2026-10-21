// Teacher-forced logits from whisper.cpp, to check Wright's Whisper.
//   whisper_logits <model.bin> <samples.f32> <token ids...>
// Prints the language probabilities' top three ("lang id prob ..."), then
// for each token fed one at a time: position, top-2 token ids and logits.
#include "whisper.h"
#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <vector>
int main(int argc, char ** argv) {
    auto cp = whisper_context_default_params();
    cp.use_gpu = false;
    cp.flash_attn = false;
    whisper_context * ctx = whisper_init_from_file_with_params(argv[1], cp);
    FILE * f = fopen(argv[2], "rb");
    std::vector<float> pcm;
    float s;
    while (fread(&s, 4, 1, f) == 1) pcm.push_back(s);
    fclose(f);
    whisper_pcm_to_mel(ctx, pcm.data(), pcm.size(), 1);
    std::vector<float> probs(whisper_lang_max_id() + 1);
    whisper_lang_auto_detect(ctx, 0, 1, probs.data());
    std::vector<int> li(probs.size());
    for (size_t i = 0; i < li.size(); i++) li[i] = i;
    std::sort(li.begin(), li.end(), [&](int a, int b) { return probs[a] > probs[b]; });
    printf("lang %d %.6f %d %.6f %d %.6f\n", li[0], probs[li[0]], li[1], probs[li[1]], li[2], probs[li[2]]);
    whisper_encode(ctx, 0, 1);
    int nv = whisper_n_vocab(ctx);
    for (int i = 3; i < argc; i++) {
        whisper_token t = atoi(argv[i]);
        whisper_decode(ctx, &t, 1, i - 3, 1);
        const float * l = whisper_get_logits(ctx);
        int a = 0, b = -1;
        for (int j = 1; j < nv; j++) if (l[j] > l[a]) a = j;
        for (int j = 0; j < nv; j++) if (j != a && (b < 0 || l[j] > l[b])) b = j;
        printf("%d %d %.5f %d %.5f\n", i - 3, a, l[a], b, l[b]);
    }
}
