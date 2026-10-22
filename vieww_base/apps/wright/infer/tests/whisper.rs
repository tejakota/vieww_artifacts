//! Whisper against whisper.cpp, on tiny random models in whisper.cpp's
//! own file format with the real mel filters and vocabulary
//! (`scripts/models/make_tiny_whisper.py` makes them).
//!
//! `WRIGHT_TINY_DIR` holds `whisper-tiny-{f32,f16}.bin`, `jfk.wav` (from
//! whisper.cpp's samples) and `whisper-tiny-*.logits`: a token sequence,
//! then whisper.cpp's language guess (`lang id p id p id p`), then its
//! top two logits after each token (`scripts/models/whisper_logits.cpp`).

use std::path::PathBuf;

use wright_infer::audio::wav_to_samples;
use wright_infer::whisper::{Whisper, LANGUAGES};

#[test]
fn logits_and_language_match_whisper_cpp() {
    let Ok(dir) = std::env::var("WRIGHT_TINY_DIR") else {
        eprintln!("WRIGHT_TINY_DIR not set; skipping");
        return;
    };
    let dir = PathBuf::from(dir);
    let samples = wav_to_samples(&std::fs::read(dir.join("jfk.wav")).unwrap()).unwrap();
    assert_eq!(samples.len(), 176_000, "11 s at 16 kHz");
    // Weights as stored, both files: token for token. Then the half-float
    // file as Wright runs it by default (8-bit blocks): the same language,
    // logits within a few percent (with random 64-wide matrices 8 bits
    // moves near-ties; the real-model CI job checks real transcripts).
    for (kind, quantize, tolerance, p_tolerance, argmax) in [
        ("f32", false, 0.002, 0.002, true),
        ("f16", false, 0.005, 0.01, true),
        ("f16", true, 0.04, 0.06, false),
    ] {
        let text =
            std::fs::read_to_string(dir.join(format!("whisper-tiny-{kind}.logits"))).unwrap();
        let mut lines = text.lines();
        let tokens: Vec<u32> = lines
            .next()
            .unwrap()
            .split(' ')
            .map(|s| s.parse().unwrap())
            .collect();
        let lang: Vec<f32> = lines
            .next()
            .unwrap()
            .split(' ')
            .skip(1)
            .map(|s| s.parse().unwrap())
            .collect();
        let w = Whisper::parse(
            &std::fs::read(dir.join(format!("whisper-tiny-{kind}.bin"))).unwrap(),
            quantize,
        )
        .unwrap();
        let (mel, n_len) = w.mel(&samples);
        let heard = w.encode(&mel, n_len, 0);
        let guess = w.detect_language(&heard).unwrap();
        assert_eq!(guess[0].0, LANGUAGES[lang[0] as usize], "{kind}: language");
        assert!(
            (guess[0].1 - lang[1]).abs() < p_tolerance,
            "{kind}: p(language) {} vs {}",
            guess[0].1,
            lang[1]
        );
        let mut st = w.start();
        let mut clear = 0;
        for (line, tok) in lines.zip(&tokens) {
            let f: Vec<f32> = line.split(' ').map(|s| s.parse().unwrap()).collect();
            let (pos, best, best_logit, margin) = (f[0], f[1] as usize, f[2], f[2] - f[4]);
            let l = w.step(&heard, &mut st, *tok).unwrap();
            let mine = l.iter().enumerate().fold(
                (0, f32::MIN),
                |b, (i, v)| if *v > b.1 { (i, *v) } else { b },
            );
            assert!(
                (mine.1 - best_logit).abs() < tolerance * best_logit.abs(),
                "{kind} (quantized: {quantize}) position {pos}: {} vs {best_logit}",
                mine.1
            );
            if argmax && margin > 0.3 {
                assert_eq!(
                    mine.0, best,
                    "{kind} position {pos}: whisper.cpp is sure (margin {margin})"
                );
                clear += 1;
            }
        }
        assert!(
            !argmax || clear > tokens.len() / 2,
            "{kind}: most positions are clear-cut ({clear})"
        );
        // And end to end: it writes something, in the language it heard.
        let t = w.transcribe(&samples, None).unwrap();
        assert_eq!(t.language, LANGUAGES[lang[0] as usize]);
        assert!(!t.tokens.is_empty());
    }
}
