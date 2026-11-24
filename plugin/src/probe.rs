use crate::classifier::Classifier;
use crate::hold::{CHUNK, SIGNALS, clock_us};
use crate::table::Table;
use crate::worker::Worker;
use serde_json::json;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn inference() -> serde_json::Value {
    let table = Table::embedded();
    let classifier = Classifier::embedded(&table).unwrap();
    let names: Vec<&str> = table.phrases.iter().map(|p| p.id.as_str()).collect();
    let holds: Vec<serde_json::Value> = (1..=4)
        .map(|seconds| {
            let pcm: Vec<i16> = (0..seconds * 16_000 / CHUNK).flat_map(crate::tone::chunk).collect();
            let mut times = vec![];
            let mut decision = 0;
            for _ in 0..5 {
                let t = Instant::now();
                decision = classifier.decide(&pcm).unwrap();
                times.push(t.elapsed().as_secs_f64() * 1000.0);
            }
            times.sort_by(f64::total_cmp);
            json!({"seconds": seconds, "median_ms": times[2], "max_ms": times[4], "decision": names[decision]})
        })
        .collect();
    json!({"holds": holds})
}

fn capture(seconds: u32, hold: bool) -> serde_json::Value {
    let holds = Arc::new(Mutex::new(vec![]));
    let sink = holds.clone();
    let table = Table::embedded();
    let names: Vec<String> = table.phrases.iter().map(|p| p.id.clone()).collect();
    let worker = Worker::spawn(move |h| sink.lock().unwrap().push((clock_us(), h)));
    let mut seconds_log = vec![];
    for _ in 0..seconds {
        let owned = hold;
        if owned {
            SIGNALS.press();
        }
        std::thread::sleep(Duration::from_secs(1));
        if owned {
            SIGNALS.release(false);
        }
        let waited = Instant::now();
        while owned && holds.lock().unwrap().is_empty() && waited.elapsed() < Duration::from_millis(900) {
            std::thread::sleep(Duration::from_millis(2));
        }
        let decided: Vec<_> = holds
            .lock()
            .unwrap()
            .drain(..)
            .map(|(at, h)| {
                json!({"phrase": names[h.phrase()].clone(), "chunks": h.chunks, "peak_rms": h.peak_rms,
                   "release_to_decision_ms": at.saturating_sub(h.released_us) as f64 / 1000.0})
            })
            .collect();
        let c = &worker.counters;
        seconds_log.push(json!({
            "opens": c.capture.opens.load(Ordering::Relaxed),
            "samples": c.capture.samples.load(Ordering::Relaxed),
            "errors": c.capture.errors.load(Ordering::Relaxed),
            "stalls": c.capture.stalls.load(Ordering::Relaxed),
            "last_error": format!("{:#010x}", c.capture.last_error.load(Ordering::Relaxed)),
            "peak_rms": c.peak_rms.swap(0, Ordering::Relaxed),
            "owned": owned,
            "holds": decided,
        }));
    }
    let errors = worker.counters.recognizer_errors.load(Ordering::Relaxed);
    worker.join();
    json!({"per_second": seconds_log, "recognizer_errors": errors})
}

#[unsafe(no_mangle)]
pub extern "C" fn thuum_probe(seconds: u32, hold: u32, out: *mut u8, capacity: usize) -> usize {
    std::panic::catch_unwind(|| probe(seconds, hold != 0, out, capacity)).unwrap_or(0)
}

fn probe(seconds: u32, hold: bool, out: *mut u8, capacity: usize) -> usize {
    let capture = capture(seconds, hold);
    let report = json!({"inference": inference(), "capture": capture}).to_string();
    let n = report.len().min(capacity);
    unsafe { std::ptr::copy_nonoverlapping(report.as_ptr(), out, n) };
    report.len()
}
