//! Picking the next token from the logits.
//!
//! ```text
//!  logits -> keep the k most likely -> softmax(logits / temperature)
//!         -> keep the smallest set reaching probability p -> draw one
//! ```
//!
//! Qwen3's published settings for non-thinking use are the defaults:
//! temperature 0.7, top-p 0.8, top-k 20. Temperature 0 is greedy (the
//! single most likely token), which tests use to be reproducible.

#[derive(Debug, Clone, Copy)]
pub struct Sampler {
    pub temperature: f32,
    pub top_k: usize,
    pub top_p: f32,
    state: u64,
}

impl Sampler {
    pub fn new(temperature: f32, top_k: usize, top_p: f32, seed: u64) -> Sampler {
        Sampler {
            temperature,
            top_k: top_k.max(1),
            top_p,
            state: seed | 1,
        }
    }

    pub fn greedy() -> Sampler {
        Sampler::new(0.0, 1, 1.0, 1)
    }

    /// Qwen3's recommended non-thinking settings, seeded from the clock.
    pub fn qwen() -> Sampler {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(7);
        Sampler::new(0.7, 20, 0.8, seed)
    }

    /// Uniform in [0, 1) (xorshift64*).
    fn uniform(&mut self) -> f32 {
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        let x = self.state.wrapping_mul(0x2545_f491_4f6c_dd1d);
        (x >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn sample(&mut self, logits: &[f32]) -> u32 {
        let argmax = || {
            logits
                .iter()
                .enumerate()
                .fold(
                    (0, f32::NEG_INFINITY),
                    |b, (i, v)| if *v > b.1 { (i, *v) } else { b },
                )
                .0 as u32
        };
        if self.temperature <= 0.0 || self.top_k == 1 {
            return argmax();
        }
        // The k best, most likely first.
        let mut idx: Vec<u32> = (0..logits.len() as u32).collect();
        let k = self.top_k.min(idx.len());
        idx.select_nth_unstable_by(k - 1, |a, b| {
            logits[*b as usize].total_cmp(&logits[*a as usize])
        });
        idx.truncate(k);
        idx.sort_by(|a, b| logits[*b as usize].total_cmp(&logits[*a as usize]));
        let max = logits[idx[0] as usize];
        let mut probs: Vec<f32> = idx
            .iter()
            .map(|i| ((logits[*i as usize] - max) / self.temperature).exp())
            .collect();
        let sum: f32 = probs.iter().sum();
        probs.iter_mut().for_each(|p| *p /= sum);
        // Nucleus: the smallest prefix reaching top_p.
        let mut cum = 0.0;
        let mut keep = probs.len();
        for (i, p) in probs.iter().enumerate() {
            cum += p;
            if cum >= self.top_p {
                keep = i + 1;
                break;
            }
        }
        let total: f32 = probs[..keep].iter().sum();
        let mut r = self.uniform() * total;
        for i in 0..keep {
            r -= probs[i];
            if r <= 0.0 {
                return idx[i];
            }
        }
        idx[keep - 1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greedy_takes_the_best_and_top_k_one_is_greedy() {
        let logits = [0.1, 3.0, -1.0, 2.9];
        assert_eq!(Sampler::greedy().sample(&logits), 1);
        assert_eq!(Sampler::new(1.0, 1, 1.0, 5).sample(&logits), 1);
    }

    #[test]
    fn sampling_stays_inside_the_nucleus() {
        // Two strong candidates, the rest negligible: top-p 0.9 keeps two.
        let mut logits = vec![-20.0f32; 100];
        logits[7] = 5.0;
        logits[42] = 4.8;
        let mut s = Sampler::new(0.7, 20, 0.9, 99);
        let mut seen = [0usize; 100];
        for _ in 0..500 {
            seen[s.sample(&logits) as usize] += 1;
        }
        assert_eq!(seen[7] + seen[42], 500);
        assert!(
            seen[7] > 0 && seen[42] > 0,
            "both are drawn: {} / {}",
            seen[7],
            seen[42]
        );
    }
}
