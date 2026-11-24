use crate::capture::{self, Stats};
use crate::hold::{Hold, SIGNALS, Step, clock_us, steps};
use crate::recognizer::Recognizer;
use crate::table::Table;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

#[derive(Default)]
pub struct Counters {
    pub capture: Stats,
    pub chunks: AtomicU64,
    pub recognizer_errors: AtomicU64,
    pub peak_rms: AtomicU32,
    pub failure: Mutex<Option<String>>,
}

pub struct Worker {
    pub stop: Arc<AtomicBool>,
    pub counters: Arc<Counters>,
    handle: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn spawn(mut on_hold: impl FnMut(Hold) + Send + 'static) -> Worker {
        let stop = Arc::new(AtomicBool::new(false));
        let counters = Arc::new(Counters::default());
        let (s, c) = (stop.clone(), counters.clone());
        let handle = std::thread::Builder::new()
            .name("thuum".into())
            .spawn(move || {
                let table = Table::embedded();
                let mut recognizer = match Recognizer::embedded(&table) {
                    Ok(r) => r,
                    Err(e) => {
                        c.recognizer_errors.fetch_add(1, Ordering::Relaxed);
                        *c.failure.lock().unwrap() = Some(e.to_string());
                        return;
                    }
                };
                let mut seen = SIGNALS.observe().0;
                capture::run(&s, &c.capture, |pcm| {
                    if !pcm.is_empty() {
                        let rms = (pcm.iter().map(|&x| (x as f32).powi(2)).sum::<f32>() / pcm.len() as f32).sqrt();
                        c.peak_rms.fetch_max(rms as u32, Ordering::Relaxed);
                    }
                    let mut closed = vec![];
                    recognizer.feed(pcm, |h| closed.push(h));
                    let (state, cancelled, released_us) = SIGNALS.observe();
                    for step in steps(seen, state, cancelled) {
                        match step {
                            Step::Press => closed.extend(recognizer.holds.press()),
                            Step::Release => recognizer.holds.release(released_us),
                            Step::Cancel => recognizer.holds.cancel(),
                        }
                    }
                    seen = state;
                    closed.extend(recognizer.holds.overdue(clock_us()));
                    for hold in closed {
                        match recognizer.decide(hold) {
                            Ok(hold) => on_hold(hold),
                            Err(e) => {
                                c.recognizer_errors.fetch_add(1, Ordering::Relaxed);
                                *c.failure.lock().unwrap() = Some(e.to_string());
                            }
                        }
                    }
                    c.chunks.store(recognizer.chunks(), Ordering::Relaxed);
                });
            })
            .unwrap();
        Worker {
            stop,
            counters,
            handle: Some(handle),
        }
    }

    pub fn join(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
