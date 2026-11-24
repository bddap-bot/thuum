use crate::table::Table;
use tract_onnx::prelude::*;

pub const BLANK: usize = 0;
const TOKENS: usize = 82;

type Plan = TypedRunnableModel<TypedModel>;

pub struct Classifier {
    plan: Plan,
    targets: Vec<Vec<usize>>,
    word_bonus: f64,
    min_samples: usize,
}

fn log_add(a: f64, b: f64) -> f64 {
    if a == f64::NEG_INFINITY {
        return b;
    }
    if b == f64::NEG_INFINITY {
        return a;
    }
    a.max(b) + (-(a - b).abs()).exp().ln_1p()
}

pub fn ctc_log_likelihood(logp: &[f32], target: &[usize]) -> f64 {
    let frames = logp.len() / TOKENS;
    let at = |t: usize, k: usize| logp[t * TOKENS + k] as f64;
    let labels: Vec<usize> = (0..2 * target.len() + 1)
        .map(|s| if s % 2 == 0 { BLANK } else { target[s / 2] })
        .collect();
    let mut alpha = vec![f64::NEG_INFINITY; labels.len()];
    if frames == 0 {
        return f64::NEG_INFINITY;
    }
    alpha[0] = at(0, labels[0]);
    alpha[1] = at(0, labels[1]);
    for t in 1..frames {
        let previous = alpha.clone();
        for s in 0..labels.len() {
            let mut sum = previous[s];
            if s >= 1 {
                sum = log_add(sum, previous[s - 1]);
            }
            if s >= 2 && labels[s] != BLANK && labels[s] != labels[s - 2] {
                sum = log_add(sum, previous[s - 2]);
            }
            alpha[s] = sum + at(t, labels[s]);
        }
    }
    let end = labels.len() - 1;
    log_add(alpha[end], alpha[end - 1])
}

pub fn closed_set(logp: &[f32], targets: &[Vec<usize>], word_bonus: f64) -> Vec<f64> {
    targets
        .iter()
        .map(|t| ctc_log_likelihood(logp, t) + word_bonus * t.len() as f64)
        .collect()
}

pub fn top_two(scores: &[f64]) -> [(usize, f64); 2] {
    let mut ranked: Vec<(usize, f64)> = scores.iter().copied().enumerate().collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    [ranked[0], ranked[1]]
}

impl Classifier {
    pub fn embedded(table: &Table) -> TractResult<Classifier> {
        let mut model = tract_onnx::onnx()
            .model_for_read(&mut &include_bytes!(concat!(env!("OUT_DIR"), "/classifier.onnx"))[..])?;
        let samples = model.symbols.sym("samples");
        model.set_input_fact(0, f32::fact(&[1.to_dim(), samples.to_dim()]).into())?;
        Ok(Classifier {
            plan: model.into_optimized()?.into_runnable()?,
            targets: table.targets(),
            word_bonus: env!("THUUM_WORD_BONUS").parse().unwrap(),
            min_samples: env!("THUUM_MIN_SAMPLES").parse().unwrap(),
        })
    }

    pub fn log_probabilities(&self, pcm: &[i16]) -> TractResult<Vec<f32>> {
        let peak = pcm.iter().map(|&s| (s as f32).abs()).fold(0.0, f32::max);
        let gain = if peak > 0.0 { 0.5 / peak } else { 0.0 };
        let mut input: Vec<f32> = pcm.iter().map(|&s| s as f32 * gain).collect();
        input.resize(input.len().max(self.min_samples), 0.0);
        let out = self
            .plan
            .run(tvec!(Tensor::from_shape(&[1, input.len()], &input)?.into()))?;
        let logp = out[0].as_slice::<f32>()?.to_vec();
        if logp.len() % TOKENS != 0 || !logp.iter().all(|x| x.is_finite()) {
            return Err(TractError::msg("unexpected classifier output"));
        }
        Ok(logp)
    }

    pub fn scores(&self, pcm: &[i16]) -> TractResult<Vec<f64>> {
        Ok(closed_set(
            &self.log_probabilities(pcm)?,
            &self.targets,
            self.word_bonus,
        ))
    }

    pub fn top_two(&self, pcm: &[i16]) -> TractResult<[(usize, f64); 2]> {
        Ok(top_two(&self.scores(pcm)?))
    }

    pub fn decide(&self, pcm: &[i16]) -> TractResult<usize> {
        Ok(self.top_two(pcm)?[0].0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collapse(path: &[usize]) -> Vec<usize> {
        let mut out = vec![];
        let mut previous = None;
        for &k in path {
            if Some(k) != previous && k != BLANK {
                out.push(k);
            }
            previous = Some(k);
        }
        out
    }

    #[test]
    fn ctc_matches_brute_force_alignment_sum() {
        let mut seed = 6202u64;
        let mut random = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as f64 / (1u64 << 31) as f64
        };
        for (target, frames) in [(vec![1], 3), (vec![2, 2], 4), (vec![1, 2, 3], 5), (vec![3, 1], 4)] {
            let mut logp = vec![f32::NEG_INFINITY; frames * TOKENS];
            for t in 0..frames {
                let row: Vec<f64> = (0..4).map(|_| random() * 4.0 - 2.0).collect();
                let norm = row.iter().map(|x| x.exp()).sum::<f64>().ln();
                for k in 0..4 {
                    logp[t * TOKENS + k] = (row[k] - norm) as f32;
                }
            }
            let mut total = f64::NEG_INFINITY;
            let mut path = vec![0usize; frames];
            for code in 0..4usize.pow(frames as u32) {
                let mut c = code;
                for p in path.iter_mut() {
                    *p = c % 4;
                    c /= 4;
                }
                if collapse(&path) == target {
                    total = log_add(
                        total,
                        path.iter().enumerate().map(|(t, &k)| logp[t * TOKENS + k] as f64).sum(),
                    );
                }
            }
            assert!((ctc_log_likelihood(&logp, &target) - total).abs() < 1e-9, "{target:?}");
        }
    }

    #[test]
    fn top_two_ranks_by_score_and_keeps_the_first_entry_on_a_tie() {
        assert_eq!(top_two(&[1.0, 3.0, 2.0]), [(1, 3.0), (2, 2.0)]);
        assert_eq!(top_two(&[5.0, 1.0, 5.0]), [(0, 5.0), (2, 5.0)]);
    }

    #[test]
    fn too_short_holds_cannot_hold_a_longer_entry() {
        let logp = vec![(1.0f32 / TOKENS as f32).ln(); 2 * TOKENS];
        assert_eq!(ctc_log_likelihood(&logp, &[4, 4, 1]), f64::NEG_INFINITY);
    }
}
