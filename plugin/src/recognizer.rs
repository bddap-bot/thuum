use crate::classifier::Classifier;
use crate::hold::{CHUNK, Closed, Hold, Holds};
use crate::table::Table;
use tract_onnx::prelude::TractResult;

pub struct Recognizer {
    classifier: Classifier,
    pub holds: Holds,
    pending: Vec<i16>,
    chunks: u64,
}

impl Recognizer {
    pub fn embedded(table: &Table) -> TractResult<Recognizer> {
        Ok(Recognizer {
            classifier: Classifier::embedded(table)?,
            holds: Holds::default(),
            pending: vec![],
            chunks: 0,
        })
    }

    pub fn feed(&mut self, pcm: &[i16], mut closed: impl FnMut(Closed)) {
        self.pending.extend_from_slice(pcm);
        while self.pending.len() >= CHUNK {
            let chunk: Vec<i16> = self.pending.drain(..CHUNK).collect();
            let rms = (chunk.iter().map(|&s| (s as f32).powi(2)).sum::<f32>() / CHUNK as f32).sqrt();
            self.chunks += 1;
            if let Some(hold) = self.holds.chunk(rms, &chunk) {
                closed(hold);
            }
        }
    }

    pub fn decide(&self, closed: Closed) -> TractResult<Hold> {
        let start = std::time::Instant::now();
        Ok(Hold {
            ranked: self.classifier.top_two(&closed.pcm)?,
            chunks: closed.chunks,
            peak_rms: closed.peak_rms,
            released_us: closed.released_us,
            classify_ms: start.elapsed().as_secs_f32() * 1000.0,
        })
    }

    pub fn chunks(&self) -> u64 {
        self.chunks
    }
}
