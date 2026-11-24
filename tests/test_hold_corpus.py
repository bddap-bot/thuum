import hashlib
import json
import random
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from thuum import hold_corpus as C


class HoldCorpusTest(unittest.TestCase):
    def test_long_u_lengthens_the_short_u_in_every_engine(self):
        e = C.long_u(C.LEX['zun'])
        self.assertEqual(e['piper_phonemes'], ['z', 'u', 'ː', 'n'])
        self.assertEqual(e['espeak_phonemes'], 'zu:n')
        self.assertEqual(e['flite_phones'], ['z', 'uw', 'n'])

    def test_long_u_leaves_long_u_and_diphthongs_alone(self):
        for word in ('fus', 'gol', 'aus', 'yol'):
            self.assertEqual(C.long_u(C.LEX[word]), C.LEX[word])

    def test_default_sequences_are_stable(self):
        s = C.sequences(random.Random(1), 200)
        self.assertEqual(hashlib.sha256(json.dumps(s).encode()).hexdigest(),
                         'b8f0515deb6cb6662430f37193356d57a5be7a6b31fe06474e07b01a2258c1c2')
        self.assertTrue(all(len(s) == 1 for s in C.sequences(random.Random(1), 200, 1.0)))


class FastCorpusTest(unittest.TestCase):
    def run_job(self, fast):
        import numpy as np

        def synth(engine, voice, speaker, words, route, slow, rng, piper, tmp, oo=False):
            return np.sin(np.arange(int(4000 * slow * len(words))) * 0.3).astype(np.float32), 16000

        with tempfile.TemporaryDirectory() as out, patch.object(C, 'synth', synth):
            C.job(('espeak', 'en-us', None, 60, 0.0, 0.0, fast, out))
            [path] = Path(out).glob('*.npz')
            d = np.load(path)
            return json.loads(str(d['meta']))['clips'], np.diff(d['offsets']) / [len(c['words']) for c in json.loads(str(d['meta']))['clips']]

    def test_fast_sequences_are_flagged_and_shorter(self):
        slow, slow_len = self.run_job(0.0)
        fast, fast_len = self.run_job(1.0)
        self.assertFalse(any(c['fast'] for c in slow))
        self.assertTrue(all(c['fast'] for c in fast))
        self.assertLess(fast_len.mean(), 0.75 * slow_len.mean())


class VoiceLicenceTest(unittest.TestCase):
    def test_only_cleared_piper_models_are_voiced(self):
        with tempfile.TemporaryDirectory() as d:
            for m in C.PIPER + ['en_US-lessac-medium', 'en_US-libritts_r-medium']:
                (Path(d) / (m + '.onnx')).write_bytes(b'')
                (Path(d) / (m + '.onnx.json')).write_text(json.dumps({'num_speakers': 2}))
            with patch.object(C, 'VOICES', Path(d)):
                piper = {v for e, v, _ in C.voices() if e == 'piper'}
        self.assertEqual(piper, set(C.PIPER))

    def test_every_vctk_speaker_is_voiced(self):
        with tempfile.TemporaryDirectory() as d, patch.object(C, 'VOICES', Path(d)), patch.object(C, 'PIPER', []):
            self.assertEqual([s for e, v, s in C.voices() if e == 'vits'], list(range(109)))

    def test_ipa_route_spells_g_as_espeak_does(self):
        heard = []

        class Engine:
            def run(self, phonemes, speaker, slow, rng):
                heard.append(''.join(phonemes))
                return None, 16000

        for engine in ('piper', 'vits'):
            C.synth(engine, 'v', 0, ('gol',), 'ipa', 1.0, random.Random(1), Engine(), None)
        self.assertEqual(heard, ['ˈɡoʊl'] * 2)

    def test_corpus_voices_outside_the_cleared_sets_are_refused(self):
        for vid in ('piper:en_US-kristin-medium', 'piper:en_US-libritts-high:903', 'piper:sv_SE-nst-medium', 'vits:vctk:108', 'espeak:en-gb+m1', 'flite:slt'):
            self.assertTrue(C.cleared(vid), vid)
        for vid in ('piper:en_US-lessac-medium', 'piper:en_US-libritts_r-medium:3', 'piper:en_GB-vctk-medium:5', 'piper:de_DE-mls-medium:1',
                    'vits:ljs', 'espeak:en-gb+Annie', 'flite:rxr'):
            self.assertFalse(C.cleared(vid), vid)
