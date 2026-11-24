import unittest

import numpy as np

from thuum import tempo
from thuum.human import LEAD


class TempoTest(unittest.TestCase):
    def test_only_the_speech_is_compressed(self):
        rng = np.random.default_rng(6237)
        speech = (np.sin(np.arange(24000) * 2 * np.pi * 180 / 16000) * 8000).astype(np.int16)
        pcm = np.concatenate([rng.integers(-50, 50, LEAD), speech, rng.integers(-50, 50, 9000)]).astype(np.int16)
        end = LEAD + len(speech)
        same = tempo.faster(pcm, end, 1.0)
        self.assertTrue(np.array_equal(same, pcm[:end + tempo.TRAIL]))
        fast = tempo.faster(pcm, end, 1.5)
        self.assertTrue(np.array_equal(fast[:LEAD], pcm[:LEAD]))
        self.assertTrue(np.array_equal(fast[-tempo.TRAIL:], pcm[end:end + tempo.TRAIL]))
        self.assertAlmostEqual(len(fast) - LEAD - tempo.TRAIL, len(speech) / 1.5, delta=320)


if __name__ == "__main__":
    unittest.main()
