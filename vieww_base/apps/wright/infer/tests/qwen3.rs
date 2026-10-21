//! The text model against llama.cpp, on tiny random Qwen3 models with the
//! real vocabulary (`scripts/models/make_tiny_qwen3.py` makes them).
//!
//! `WRIGHT_TINY_DIR` holds `tiny-{q8_0,f16,f32}.gguf` and, for each,
//! `tiny-*.expect`: llama.cpp's greedy continuation (`llama-simple -n 24`)
//! of the prompt on its first line. `strong-q8_0.gguf` has strong attention
//! and MLP outputs (so a mistake there would show) and `strong-q8_0.logits`
//! holds a token sequence, then llama.cpp's top two logits after each
//! token (`position top1 logit1 top2 logit2`).

use std::path::PathBuf;

use wright_infer::llm::Llm;
use wright_infer::qwen3::Model;
use wright_infer::sample::Sampler;

#[test]
fn greedy_text_matches_llama_cpp() {
    let Ok(dir) = std::env::var("WRIGHT_TINY_DIR") else {
        eprintln!("WRIGHT_TINY_DIR not set; skipping");
        return;
    };
    for q in ["q8_0", "f16", "f32"] {
        let dir = PathBuf::from(&dir);
        let file = format!("tiny-{q}");
        let expect = std::fs::read_to_string(dir.join(format!("{file}.expect"))).unwrap();
        let (prompt, want) = expect.split_once('\n').unwrap();
        let llm = Llm::new(Model::load(&dir.join(format!("{file}.gguf"))).unwrap(), "tiny");
        let ids = llm.model.tokenizer.encode(prompt, false);
        let (got, n, _) = llm.generate(&ids, 24, Sampler::greedy(), &mut |_| {}).unwrap();
        assert_eq!(n, 24);
        assert_eq!(got, want.trim_end_matches('\n'), "{q}");

        // The cache: the same prompt again recomputes only its last token
        // and gives the same answer; a longer one extends it.
        let (again, _, _) = llm.generate(&ids, 24, Sampler::greedy(), &mut |_| {}).unwrap();
        assert_eq!(again, got, "{q}: a cached prefix changes nothing");
    }
}

/// Logits, position by position. Two engines quantizing the same way still
/// round differently, so: within 1.5% of llama.cpp's best logit, and the
/// same best token wherever llama.cpp's choice is clear (margin > 0.5).
/// Near-ties may go either way; with random weights there are a few.
#[test]
fn logits_match_llama_cpp_with_strong_attention() {
    let Ok(dir) = std::env::var("WRIGHT_TINY_DIR") else {
        eprintln!("WRIGHT_TINY_DIR not set; skipping");
        return;
    };
    let dir = PathBuf::from(dir);
    let text = std::fs::read_to_string(dir.join("strong-q8_0.logits")).unwrap();
    let mut lines = text.lines();
    let ids: Vec<u32> = lines.next().unwrap().split(' ').map(|s| s.parse().unwrap()).collect();
    let model = Model::load(&dir.join("strong-q8_0.gguf")).unwrap();
    let mut cache = model.cache(4096);
    let mut clear = 0;
    for (line, tok) in lines.zip(&ids) {
        let f: Vec<f32> = line.split(' ').map(|s| s.parse().unwrap()).collect();
        let (pos, best, best_logit, margin) = (f[0], f[1] as usize, f[2], f[2] - f[4]);
        let l = model.forward(&mut cache, &[*tok]).unwrap();
        let mine = l.iter().enumerate().fold((0, f32::MIN), |b, (i, v)| if *v > b.1 { (i, *v) } else { b });
        assert!((mine.1 - best_logit).abs() < 0.015 * best_logit.abs(), "position {pos}: {} vs {best_logit}", mine.1);
        if margin > 0.5 {
            assert_eq!(mine.0, best, "position {pos}: llama.cpp is sure (margin {margin})");
            clear += 1;
        }
    }
    assert!(clear > ids.len() / 2, "most positions are clear-cut");

    // The batched prompt (prefill) gives the same last logits as one by one.
    let mut batched = model.cache(4096);
    let all = model.forward(&mut batched, &ids).unwrap();
    let mut single = model.cache(4096);
    let mut last = Vec::new();
    for t in &ids {
        last = model.forward(&mut single, &[*t]).unwrap();
    }
    let worst = all.iter().zip(&last).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
    assert!(worst < 1e-3, "batched and one-by-one differ by {worst}");
}
