//! The tokenizer against llama.cpp's published Qwen2/3 test cases: the
//! real vocabulary (`ggml-vocab-qwen2.gguf`, tokens and merges only) and,
//! for each input, the ids llama.cpp's reference tokenizer produced.
//!
//! The files live in llama.cpp's repository (`models/`); point
//! `WRIGHT_VOCAB_DIR` at that folder to run this. CI fetches them.

use std::path::PathBuf;

use wright_infer::gguf::Gguf;
use wright_infer::tokenizer::Tokenizer;

#[test]
fn matches_llama_cpp_on_its_qwen2_cases() {
    let Ok(dir) = std::env::var("WRIGHT_VOCAB_DIR") else {
        eprintln!("WRIGHT_VOCAB_DIR not set; skipping");
        return;
    };
    let dir = PathBuf::from(dir);
    let tok = Tokenizer::from_gguf(&Gguf::open(&dir.join("ggml-vocab-qwen2.gguf")).unwrap()).unwrap();
    let inputs = std::fs::read_to_string(dir.join("ggml-vocab-qwen2.gguf.inp")).unwrap();
    let outputs = std::fs::read_to_string(dir.join("ggml-vocab-qwen2.gguf.out")).unwrap();
    let cases: Vec<&str> = inputs.split("\n__ggml_vocab_test__\n").collect();
    let expected: Vec<&str> = outputs.lines().collect();
    let mut failures = Vec::new();
    for (i, (text, want)) in cases.iter().zip(&expected).enumerate() {
        let got: Vec<String> = tok.encode(text, false).iter().map(|t| t.to_string()).collect();
        let want: Vec<&str> = want.split_whitespace().collect();
        if got != want {
            failures.push(format!("case {i} {text:?}\n  want {want:?}\n  got  {got:?}"));
        } else {
            assert_eq!(tok.decode(&tok.encode(text, false)), *text, "case {i} round-trips");
        }
    }
    assert!(failures.is_empty(), "{} of {} cases differ:\n{}", failures.len(), expected.len(), failures.join("\n"));
}

/// Random text against llama.cpp's own tokenizer binary: every script and
/// whitespace shape the pre-tokenizer has a rule for, mixed (`--no-escape`:
/// the tool would otherwise turn `\\` into `\`). Needs
/// `WRIGHT_VOCAB_DIR` and `WRIGHT_LLAMA_TOKENIZE` (path to `llama-tokenize`).
#[test]
fn fuzz_against_llama_tokenize() {
    let (Ok(dir), Ok(bin)) = (std::env::var("WRIGHT_VOCAB_DIR"), std::env::var("WRIGHT_LLAMA_TOKENIZE")) else {
        eprintln!("WRIGHT_VOCAB_DIR / WRIGHT_LLAMA_TOKENIZE not set; skipping");
        return;
    };
    let vocab = PathBuf::from(dir).join("ggml-vocab-qwen2.gguf");
    let tok = Tokenizer::from_gguf(&Gguf::open(&vocab).unwrap()).unwrap();
    let pool: Vec<&str> = vec![
        "a", "Z", "q", "ü", "É", "ß", "ж", "Д", "中", "文", "字", "ह", "ि", "ं", "्", "क", "ا", "ل", "😀", "🦙", "1", "7", "٣", "½", " ", "  ", "\t", "\n", "\r\n", "\n\n", "'s", "'LL", "'re", ".", ",",
        "!", "?", "...", "-", "_", "(", ")", "{", "}", "\"", "#", "@", "€", "/", "\\", "`", "~", "=", "+", "<", ">", "Hello", " world", "fn", "x=1;", "café",
    ];
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    let mut rand = |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n as u64) as usize
    };
    let mut failures = Vec::new();
    for case in 0..300 {
        let len = 1 + rand(14);
        let text: String = (0..len).map(|_| pool[rand(pool.len())]).collect();
        let out = std::process::Command::new(&bin).arg("-m").arg(&vocab).args(["--ids", "--log-disable", "--no-escape", "-p", &text]).output().unwrap();
        let line = String::from_utf8_lossy(&out.stdout).lines().last().unwrap_or("").to_string();
        let want: Vec<u32> = line.trim_matches(|c| c == '[' || c == ']').split(", ").filter(|s| !s.is_empty()).map(|s| s.parse().unwrap()).collect();
        let got = tok.encode(&text, false);
        if got != want {
            failures.push(format!("case {case} {text:?}\n  want {want:?}\n  got  {got:?}"));
        }
    }
    assert!(failures.is_empty(), "{} of 300 differ:\n{}", failures.len(), failures.iter().take(10).cloned().collect::<Vec<_>>().join("\n"));
}

/// Control tokens are markup only when the caller says so: text that
/// merely contains `<|im_end|>` (a fetched web page, say) stays text.
#[test]
fn control_tokens_only_on_request() {
    let Ok(dir) = std::env::var("WRIGHT_VOCAB_DIR") else {
        eprintln!("WRIGHT_VOCAB_DIR not set; skipping");
        return;
    };
    let tok = Tokenizer::from_gguf(&Gguf::open(&PathBuf::from(dir).join("ggml-vocab-qwen2.gguf")).unwrap()).unwrap();
    let end = tok.id("<|im_end|>").expect("ChatML end token");
    assert_eq!(tok.encode("<|im_end|>", true), vec![end]);
    let as_text = tok.encode("<|im_end|>", false);
    assert!(as_text.len() > 1 && !as_text.contains(&end), "{as_text:?}");
    assert_eq!(tok.decode(&as_text), "<|im_end|>", "and it still reads the same");
}
