import hashlib
import json
import tempfile
import unittest
import wave
from pathlib import Path

import numpy as np

from thuum.human import GAP, LEAD, PAD, QUIET, TRAIL, build, holds, plan, quiet_spans, write_wav
from thuum.table import load_table

def take_audio(samples, spans, seed=3):
    rng = np.random.default_rng(seed)
    pcm = rng.normal(0, 30, samples)
    for a, b in spans:
        pcm[a:b] += np.sin(np.arange(b - a) * 0.07) * 9000
    return pcm.astype(np.int16)


def labels(pcm, items, take="shout-a"):
    return dict(schema=1, takes=[dict(take=take, speaker="s1", style="shout", samples=len(pcm),
                                      sha256_pcm=hashlib.sha256(pcm.tobytes()).hexdigest(), items=items)])


def item(shout, start, words=None, deviation=None):
    spoken = words or list(load_table()[f"{shout}:3"].words)
    found = dict(shout=shout, spoken=spoken, words=[[start, start + 4000], [start + 4000, start + 7000], [start + 7000, start + 12000]])
    if deviation:
        found["deviation"] = deviation
    return found


class HumanTests(unittest.TestCase):
    def setUp(self):
        self.table = load_table()

    def test_plan_rows_and_deviations(self):
        items = [item("unrelenting_force", 20000), item("drain_vitality", 60000, ["gaan", "lun", "haas"], "middle word swapped")]
        rows = {r["id"]: r for r in plan(labels(np.zeros(100000, np.int16), items), self.table)}
        self.assertEqual(set(rows), {"shout-a-unrelenting_force-1", "shout-a-unrelenting_force-2", "shout-a-unrelenting_force-3",
                                     "shout-a-unrelenting_force-wrong", "shout-a-unrelenting_force-continuation",
                                     "shout-a-unrelenting_force-tail", "shout-a-drain_vitality-spoken", "shout-a-pauses"})
        self.assertEqual(rows["shout-a-unrelenting_force-2"]["spans"], [[20000 - PAD, 27000]])
        self.assertEqual(rows["shout-a-unrelenting_force-wrong"]["spans"], [[27000, 32000 + PAD], [24000, 27000], [20000 - PAD, 24000]])
        self.assertEqual(rows["shout-a-unrelenting_force-continuation"]["spans"], [[20000 - PAD, 32000 + PAD], [20000 - PAD, 24000]])
        self.assertEqual(rows["shout-a-unrelenting_force-tail"]["spans"], [[24000, 32000 + PAD]])
        self.assertEqual(rows["shout-a-drain_vitality-spoken"]["negative_class"], "wrong_word")
        self.assertIsNone(rows["shout-a-drain_vitality-spoken"]["phrase"])

    def test_undeclared_deviation_is_refused(self):
        with self.assertRaises(ValueError):
            plan(labels(np.zeros(50000, np.int16), [item("unrelenting_force", 20000, ["fus", "dah", "ro"])]), self.table)

    def test_quiet_spans_keep_clear_of_speech(self):
        take = labels(np.zeros(100000, np.int16), [item("unrelenting_force", 20000), item("bend_will", 60000)])["takes"][0]
        self.assertEqual(quiet_spans(take), [[0, 20000 - QUIET], [32000 + QUIET, 60000 - QUIET], [72000 + QUIET, 100000]])
        speech = [s for r in plan(dict(schema=1, takes=[take]), self.table) if r["phrase"] for s in r["spans"]]
        self.assertFalse(any(a < d and c < b for a, b in quiet_spans(take) for c, d in speech))

    def test_build_renders_hashed_rows_from_real_noise(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            pcm = take_audio(100000, [(20000, 32000), (60000, 72000)])
            write_wav(root / "shout-a.wav", pcm)
            (root / "labels.json").write_text(json.dumps(labels(pcm, [item("unrelenting_force", 20000), item("bend_will", 60000)])))
            result = build(root, root / "out")
            self.assertEqual(json.loads((root / "out" / "yardstick.json").read_text()), result)
            self.assertEqual({r["id"] for r, _ in holds(root / "out")}, {r["id"] for r in result["rows"] if r["phrase"]})
            rows = {r["id"]: r for r in result["rows"]}
            self.assertEqual(len(rows), 13)
            for row in rows.values():
                with wave.open(str(root / "out" / (row["id"] + ".wav"))) as stream:
                    audio = np.frombuffer(stream.readframes(stream.getnframes()), "<i2")
                self.assertEqual(hashlib.sha256(audio.tobytes()).hexdigest(), row["sha256_pcm"])
                self.assertEqual(len(audio), row["samples"])
                self.assertNotIn("wav", row)
            three = rows["shout-a-bend_will-3"]
            self.assertEqual(three["samples"], LEAD + 12000 + 2 * PAD + TRAIL)
            self.assertEqual(three["speech_end_sample"], LEAD + 12000 + 2 * PAD)
            with wave.open(str(root / "out" / "shout-a-bend_will-3.wav")) as stream:
                audio = np.frombuffer(stream.readframes(stream.getnframes()), "<i2")
            self.assertTrue(np.array_equal(audio[LEAD:LEAD + 12000 + 2 * PAD], pcm[60000 - PAD:72000 + PAD]))
            self.assertLess(np.abs(audio[:LEAD].astype(int)).max(), 200)
            self.assertGreater(np.abs(audio[-TRAIL:].astype(int)).max(), 0)
            one = rows["shout-a-bend_will-1"]
            self.assertEqual(one["speech_end_sample"], LEAD + 4000 + PAD)
            with wave.open(str(root / "out" / "shout-a-bend_will-1.wav")) as stream:
                audio = np.frombuffer(stream.readframes(stream.getnframes()), "<i2")
            self.assertTrue(np.array_equal(audio[LEAD:LEAD + 4000 + PAD], pcm[60000 - PAD:64000]))
            wrong = rows["shout-a-bend_will-wrong"]
            self.assertEqual(wrong["samples"], LEAD + 12000 + 2 * PAD + 2 * GAP + TRAIL)
            self.assertEqual(rows["shout-a-pauses"]["samples"], sum(b - a for a, b in rows["shout-a-pauses"]["spans"]))
            self.assertEqual(result["deviations"], [])

    def test_checksum_mismatch_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            pcm = take_audio(50000, [(20000, 32000)])
            data = labels(pcm, [item("unrelenting_force", 20000)])
            write_wav(root / "shout-a.wav", pcm[::-1].copy())
            (root / "labels.json").write_text(json.dumps(data))
            with self.assertRaises(ValueError):
                build(root)


if __name__ == "__main__":
    unittest.main()
