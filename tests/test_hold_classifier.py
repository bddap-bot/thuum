import itertools
import unittest

import numpy as np

from thuum import hold_classifier as H
from thuum.table import load_table


def collapse(path, blank=0):
    out, prev = [], None
    for token in path:
        if token != prev and token != blank:
            out.append(token)
        prev = token
    return out


class HoldClassifierTest(unittest.TestCase):
    def test_every_word_is_one_token_and_entries_are_prefixes(self):
        phrases = load_table()
        self.assertEqual(len(H.vocabulary(phrases)), 81)
        tg = H.targets(phrases)
        for key, phrase in phrases.items():
            self.assertEqual(len(tg[key]), phrase.tier)
            if phrase.tier > 1:
                self.assertEqual(tg[key][:-1], tg[f"{phrase.shout}:{phrase.tier - 1}"])

    def test_ctc_matches_brute_force_alignment_sum(self):
        rng = np.random.default_rng(6237)
        for target, frames in (([1], 3), ([2, 2], 4), ([1, 2, 3], 5), ([3, 1], 4)):
            logits = rng.normal(size=(frames, 4))
            logp = logits - np.log(np.exp(logits).sum(1, keepdims=True))
            total = -np.inf
            for path in itertools.product(range(4), repeat=frames):
                if collapse(path) == target:
                    total = np.logaddexp(total, sum(logp[t, k] for t, k in enumerate(path)))
            self.assertAlmostEqual(H.ctc_log_likelihoods(logp, [target])[0], total, places=9)

    def test_batched_sequences_of_mixed_length_match_single_ones_and_too_short_holds_lose(self):
        rng = np.random.default_rng(1)
        logits = rng.normal(size=(6, 5))
        logp = logits - np.log(np.exp(logits).sum(1, keepdims=True))
        seqs = [[1], [2, 3], [4, 4, 1], [1, 2, 3, 4]]
        batched = H.ctc_log_likelihoods(logp, seqs)
        for s, value in zip(seqs, batched):
            self.assertAlmostEqual(value, H.ctc_log_likelihoods(logp, [s])[0], places=9)
        self.assertEqual(H.ctc_log_likelihoods(logp[:2], [[4, 4, 1]])[0], -np.inf)

    def test_decide_reads_the_emitted_words_and_bonus_restores_a_weak_last_word(self):
        phrases = load_table()
        tg = H.targets(phrases)
        words = tg["unrelenting_force:3"]
        logp = np.full((9, 82), np.log(1e-4))
        logp[:, 0] = np.log(0.99)
        for t, word in ((1, words[0]), (4, words[1])):
            logp[t] = np.log(1e-4); logp[t, word] = np.log(0.99)
        logp[7] = np.log(1e-4); logp[7, 0] = np.log(0.7); logp[7, words[2]] = np.log(0.29)
        self.assertEqual(H.decide(logp, phrases), "unrelenting_force:2")
        self.assertEqual(H.decide(logp, phrases, word_bonus=2.0), "unrelenting_force:3")


if __name__ == "__main__":
    unittest.main()
