use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};

fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let pin: serde_json::Value =
        serde_json::from_str(&fs::read_to_string("../data/hold-classifier.json").unwrap()).unwrap();
    let path = PathBuf::from(env::var("THUUM_CLASSIFIER").expect("THUUM_CLASSIFIER names the pinned hold classifier"));
    let data = fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert_eq!(
        format!("{:x}", Sha256::digest(&data)),
        pin["sha256"].as_str().unwrap(),
        "hold classifier checksum"
    );
    assert_eq!(
        data.len() as u64,
        pin["bytes"].as_u64().unwrap(),
        "hold classifier size"
    );
    fs::write(out.join("classifier.onnx"), data).unwrap();
    println!("cargo:rustc-env=THUUM_WORD_BONUS={}", pin["word_bonus"].as_f64().unwrap());
    println!("cargo:rustc-env=THUUM_MIN_SAMPLES={}", pin["min_samples"].as_u64().unwrap());
    for path in [
        "../data/hold-classifier.json",
        "../data/shouts.json",
        "../data/game-forms.json",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rerun-if-env-changed=THUUM_CLASSIFIER");
    println!("cargo:rerun-if-changed={}", path.display());
}
