use std::f64::consts::PI;

pub const RATE: u32 = 16_000;
const ZERO_CROSSINGS: f64 = 16.0;
const STEPS: f64 = 256.0;

pub struct Resampler {
    step: f64,
    half: usize,
    kernel: Vec<f32>,
    position: f64,
    history: Vec<f32>,
}

impl Resampler {
    pub fn new(input_rate: u32) -> Resampler {
        let ratio = (RATE as f64 / input_rate as f64).min(1.0);
        let cutoff = 0.45 * ratio;
        let half = (ZERO_CROSSINGS / (2.0 * cutoff)).ceil() as usize;
        let kernel = (0..=(half as f64 * STEPS) as usize)
            .map(|i| {
                let d = i as f64 / STEPS;
                let x = 2.0 * cutoff * d;
                let sinc = if x == 0.0 { 1.0 } else { (PI * x).sin() / (PI * x) };
                (2.0 * cutoff * sinc * (0.5 + 0.5 * (PI * d / half as f64).cos())) as f32
            })
            .collect();
        Resampler {
            step: input_rate as f64 / RATE as f64,
            half,
            kernel,
            position: half as f64,
            history: vec![0.0; half],
        }
    }

    pub fn push(&mut self, input: &[f32], out: &mut Vec<i16>) {
        self.history.extend_from_slice(input);
        let half = self.half as isize;
        while self.position + (self.half as f64) < self.history.len() as f64 {
            let centre = self.position.floor() as isize;
            let mut acc = 0.0f32;
            for k in centre - half + 1..=centre + half {
                let d = (self.position - k as f64).abs();
                if d < self.half as f64 {
                    acc += self.history[k as usize] * self.kernel[(d * STEPS).round() as usize];
                }
            }
            out.push((acc * 32768.0).round().clamp(-32768.0, 32767.0) as i16);
            self.position += self.step;
        }
        let drop = (self.position.floor() as usize).saturating_sub(self.half);
        self.history.drain(..drop);
        self.position -= drop as f64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, hz: f64, seconds: f64) -> Vec<f32> {
        (0..(rate as f64 * seconds) as usize)
            .map(|i| (0.5 * (2.0 * PI * hz * i as f64 / rate as f64).sin()) as f32)
            .collect()
    }

    fn rms(x: &[i16]) -> f64 {
        (x.iter().map(|&s| (s as f64 / 32768.0).powi(2)).sum::<f64>() / x.len() as f64).sqrt()
    }

    #[test]
    fn passes_speech_band_and_rejects_aliases() {
        for rate in [8_000, 16_000, 44_100, 48_000, 96_000] {
            let mut pass = vec![];
            let mut r = Resampler::new(rate);
            for block in tone(rate, 1000.0, 1.0).chunks(441) {
                r.push(block, &mut pass);
            }
            assert!(
                (pass.len() as i64 - 16_000).abs() < 200,
                "{rate}: {} samples",
                pass.len()
            );
            assert!(
                (rms(&pass[600..]) - 0.3536).abs() < 0.01,
                "{rate}: {}",
                rms(&pass[600..])
            );
            if rate > RATE {
                for alias in [9_000.0, 12_000.0] {
                    let mut out = vec![];
                    Resampler::new(rate).push(&tone(rate, alias, 1.0), &mut out);
                    assert!(rms(&out[600..]) < 0.01, "{rate}: {alias} Hz leaks {}", rms(&out[600..]));
                }
            }
        }
    }
}
