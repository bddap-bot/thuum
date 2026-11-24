use thuum::classifier::{Classifier, closed_set};
use thuum::hold::{CHUNK, TAIL_CHUNKS};
use thuum::recognizer::Recognizer;
use thuum::table::Table;

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("classifier-fixture.json")).unwrap()
}

fn floats(v: &serde_json::Value) -> Vec<f64> {
    v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()
}

fn wav(bytes: &[u8]) -> Vec<i16> {
    let data = bytes.windows(4).position(|w| w == b"data").unwrap() + 8;
    bytes[data..]
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect()
}

fn clips() -> [(&'static str, Vec<i16>); 2] {
    [
        ("fus.wav", wav(include_bytes!("fus.wav"))),
        ("fus-ro-dah.wav", wav(include_bytes!("fus-ro-dah.wav"))),
    ]
}

fn close(got: &[f64], want: &[f64], tolerance: f64) {
    assert_eq!(got.len(), want.len());
    for (a, b) in got.iter().zip(want) {
        assert!((a - b).abs() < tolerance, "{a} vs {b}");
    }
}

#[test]
fn closed_set_scores_match_the_python_decision() {
    let f = fixture();
    let logp: Vec<f32> = f["random_logp"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(floats)
        .map(|x| x as f32)
        .collect();
    let got = closed_set(&logp, &Table::embedded().targets(), f["word_bonus"].as_f64().unwrap());
    close(&got, &floats(&f["random_scores"]), 1e-4);
}

#[test]
fn embedded_classifier_matches_onnx_runtime_on_speech() {
    let f = fixture();
    let table = Table::embedded();
    let classifier = Classifier::embedded(&table).unwrap();
    for (name, pcm) in clips() {
        let want = &f["clips"][name];
        let logp = classifier.log_probabilities(&pcm).unwrap();
        assert_eq!(logp.len(), want["frames"].as_u64().unwrap() as usize * 82, "{name}");
        close(&classifier.scores(&pcm).unwrap(), &floats(&want["scores"]), 1e-3);
        let decision = &table.phrases[classifier.decide(&pcm).unwrap()].id;
        assert_eq!(decision, want["decision"].as_str().unwrap(), "{name}");
    }
}

#[test]
fn a_hold_shorter_than_the_model_minimum_still_decides() {
    let table = Table::embedded();
    let classifier = Classifier::embedded(&table).unwrap();
    let (_, pcm) = &clips()[0];
    assert!(classifier.decide(&pcm[..CHUNK]).is_ok());
}

#[test]
fn embedded_recognizer_decides_every_hold() {
    let table = Table::embedded();
    let mut recognizer = Recognizer::embedded(&table).unwrap();
    let (_, mut speech) = clips().into_iter().nth(1).unwrap();
    speech.resize(speech.len().div_ceil(CHUNK) * CHUNK, 0);
    let mut closed = vec![];
    recognizer.feed(&vec![0; CHUNK * 8], |h| closed.push(h));
    assert_eq!(recognizer.holds.press(), None);
    recognizer.feed(&speech, |h| closed.push(h));
    recognizer.holds.release(1);
    recognizer.feed(&vec![0; CHUNK * TAIL_CHUNKS as usize], |h| closed.push(h));
    recognizer.holds.press();
    recognizer.feed(&vec![0; CHUNK * 10], |h| closed.push(h));
    recognizer.holds.release(2);
    recognizer.feed(&vec![0; CHUNK * TAIL_CHUNKS as usize], |h| closed.push(h));
    assert_eq!(closed.len(), 2);
    let spoken = recognizer.decide(closed.remove(0)).unwrap();
    let silent = recognizer.decide(closed.remove(0)).unwrap();
    assert_eq!(table.phrases[spoken.phrase()].id, "unrelenting_force:3");
    let [best, runner_up] = spoken.ranked;
    assert!(best.1 >= runner_up.1 && best.0 != runner_up.0);
    assert!(spoken.peak_rms > 1000.0);
    assert_eq!((silent.chunks, silent.peak_rms), (16, 0.0));
    assert!(silent.phrase() < table.phrases.len());
}
