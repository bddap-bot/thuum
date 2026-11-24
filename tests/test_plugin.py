import json
import struct
import tempfile
import unittest
import zlib
from pathlib import Path

import numpy as np

from thuum.hold_classifier import closed_set
from thuum.gamedata import load_forms, shout_words
from thuum.table import load_table

ROOT = Path(__file__).resolve().parent.parent
CLASSIFIER_FIXTURE = ROOT / "plugin" / "tests" / "classifier-fixture.json"


class PluginContractTests(unittest.TestCase):
    def test_classifier_fixture_matches_the_python_decision(self):
        fixture = json.loads(CLASSIFIER_FIXTURE.read_text())
        phrases = load_table()
        scores = closed_set(np.array(fixture["random_logp"]), phrases, fixture["word_bonus"])
        self.assertTrue(np.allclose([scores[k] for k in phrases], fixture["random_scores"], atol=1e-9))
        self.assertEqual(len(fixture["random_scores"]), len(phrases))
        self.assertEqual({name: clip["decision"] for name, clip in fixture["clips"].items()},
                         {"fus.wav": "unrelenting_force:1", "fus-ro-dah.wav": "unrelenting_force:3"})
        for clip in fixture["clips"].values():
            scores = dict(zip(phrases, clip["scores"]))
            self.assertEqual(max(scores, key=scores.get), clip["decision"])

    def test_master_file_shout_records(self):
        def record(kind, form, fields, flags=0):
            body = b"".join(k + struct.pack("<H", len(v)) + v for k, v in fields)
            if flags & 0x40000:
                body = struct.pack("<I", len(body)) + zlib.compress(body)
            return kind + struct.pack("<IIIIHH", len(body), flags, form, 0, 44, 0) + body

        def snam(word):
            return (b"SNAM", struct.pack("<IIf", word, 0, 5.0))

        shout = record(b"SHOU", 0x00013E07, [(b"EDID", b"X\0"), snam(1), snam(2), snam(3)])
        packed = record(b"SHOU", 0x02000800, [snam(4), snam(5), snam(6)], flags=0x40000)
        group = b"GRUP" + struct.pack("<I", 24 + len(shout) + len(packed)) + b"SHOU" + bytes(12) + shout + packed
        data = record(b"TES4", 0, [(b"HEDR", bytes(12))]) + group
        Path(".scratch").mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=".scratch") as directory:
            (Path(directory) / "Test.esm").write_bytes(data)
            self.assertEqual(shout_words(directory, "Test.esm"), {0x013E07: [1, 2, 3], 0x000800: [4, 5, 6]})

    def test_game_forms_cover_table(self):
        self.assertEqual(set(load_forms()), {p.shout for p in load_table().values()})
