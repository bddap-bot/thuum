import json
import subprocess
import sys
import wave
from pathlib import Path

import numpy as np
import onnxruntime as ort

from thuum.hold_classifier import closed_set
from thuum.table import load_table

ROOT = Path(__file__).resolve().parent.parent
TESTS = ROOT / "plugin" / "tests"
CLIPS = {"fus.wav": "[[fu:s]]", "fus-ro-dah.wav": "[[fu:s ro:U dA:]]"}


def synthesize(path, phonemes):
    speech = subprocess.run(["espeak-ng", "-v", "en-us", "-s", "140", "--stdout", phonemes], capture_output=True, check=True).stdout
    subprocess.run(["sox", "-t", "wav", "-", "-r", "16000", "-c", "1", "-b", "16", str(path), "pad", "0.4", "0.3"], input=speech, check=True)


def main():
    pin = json.loads((ROOT / "data" / "hold-classifier.json").read_text())
    phrases = load_table()
    session = ort.InferenceSession(sys.argv[1], providers=["CPUExecutionProvider"])
    clips = {}
    for name, phonemes in CLIPS.items():
        synthesize(TESTS / name, phonemes)
        with wave.open(str(TESTS / name)) as w:
            pcm = np.frombuffer(w.readframes(w.getnframes()), "<i2").astype(np.float32)
        x = np.zeros(max(len(pcm), pin["min_samples"]), np.float32)
        x[: len(pcm)] = pcm * (0.5 / np.abs(pcm).max())
        logp = session.run(None, {session.get_inputs()[0].name: x[None]})[0].reshape(-1, 82)
        scores = closed_set(logp, phrases, pin["word_bonus"])
        clips[name] = {"frames": int(len(logp)), "scores": [scores[k] for k in phrases],
                       "decision": max(scores, key=scores.get)}
    rng = np.random.default_rng(6202)
    logits = rng.normal(size=(24, 82)) * 3
    logp = logits - np.log(np.exp(logits).sum(1, keepdims=True))
    scores = closed_set(logp, phrases, pin["word_bonus"])
    fixture = {"word_bonus": pin["word_bonus"], "clips": clips, "random_logp": logp.tolist(),
               "random_scores": [scores[k] for k in phrases]}
    (TESTS / "classifier-fixture.json").write_text(json.dumps(fixture, indent=1) + "\n")


main()
