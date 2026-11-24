import json
import unittest
from pathlib import Path

LEXICON = json.loads((Path(__file__).resolve().parent.parent / 'data/lexicon.json').read_text())['entries']

SEGMENTS = {
    'ɑː': ('A:', 'aa'), 'iː': ('i:', 'iy'), 'uː': ('u:', 'uw'), 'eɪ': ('eI', 'ey'), 'oʊ': ('oU', 'ow'), 'aʊ': ('aU', 'aw'),
    'ɪ': ('I', 'ih'), 'ʊ': ('U', 'uh'), 'ɛ': ('E', 'eh'), 'ɒ': ('0', 'aa'),
    'dʒ': ('dZ', 'jh'), 'ʃ': ('S', 'sh'), 'ŋ': ('N', 'ng'), 'ɡ': ('g', 'g'), 'j': ('j', 'y'), 'h': ('h', 'hh'),
    **{c: (c, c) for c in 'bdfklmnrstvwz'},
}


def segments(ipa):
    out = []
    while ipa:
        seg = next(s for s in sorted(SEGMENTS, key=len, reverse=True) if ipa.startswith(s))
        out.append(seg)
        ipa = ipa[len(seg):]
    return out


class LexiconTest(unittest.TestCase):
    def test_every_transcription_spells_the_ipa(self):
        for e in LEXICON:
            with self.subTest(word=e['word']):
                segs = segments(e['ipa'])
                self.assertEqual(e['espeak_phonemes'], ''.join(SEGMENTS[s][0] for s in segs))
                self.assertEqual(e['flite_phones'], [SEGMENTS[s][1] for s in segs])
                self.assertEqual(e['piper_phonemes'], list(e['ipa'].replace('ɡ', 'g')))

    def test_every_dragon_word_has_one_entry(self):
        from thuum.table import load_table
        words = {w for phrase in load_table().values() for w in phrase.words}
        self.assertEqual(sorted(e['word'] for e in LEXICON), sorted(words))
