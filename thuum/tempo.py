import argparse, json, subprocess
from collections import Counter
from pathlib import Path

import numpy as np

from .hold_classifier import closed_set, report
from .human import LEAD, holds
from .table import load_table

ROOT = Path(__file__).resolve().parent.parent
TRAIL = 4800
FACTORS = (1.0, 1.25, 1.5, 1.75)


def stretch(pcm, factor):
    if factor == 1.0:
        return pcm
    out = subprocess.run(['sox', '-q', '-t', 'raw', '-r', '16000', '-e', 'signed', '-b', '16', '-c', '1', '-',
                          '-t', 'raw', '-', 'tempo', '-s', str(factor)], input=pcm.astype('<i2').tobytes(),
                         capture_output=True, check=True).stdout
    return np.frombuffer(out, '<i2')


def faster(pcm, speech_end, factor):
    return np.concatenate([pcm[:LEAD], stretch(pcm[LEAD:speech_end], factor), pcm[speech_end:speech_end + TRAIL]])


class Model:
    def __init__(self, path, threads):
        import onnxruntime as ort
        options = ort.SessionOptions()
        options.intra_op_num_threads = threads
        self.session = ort.InferenceSession(str(path), options, providers=['CPUExecutionProvider'])

    def __call__(self, pcm):
        x = pcm.astype(np.float32)
        x = x * (0.5 / max(np.abs(x).max(), 1.0))
        return self.session.run(None, {'pcm': x[None]})[0]


def curve(model, audio, factors, phrases, word_bonus):
    labels = list(phrases)
    rows = {f: [] for f in factors}
    for r, pcm in holds(audio):
        for f in factors:
            scores = closed_set(model(faster(pcm, r['speech_end_sample'], f)), phrases)
            rows[f].append(dict(id=r['id'], phrase=r['phrase'], style=r['style'], scores=list(scores.values())))
    out = {}
    for f, rs in rows.items():
        groups = {style: report([x for x in rs if x['style'] == style], labels, word_bonus) for style in sorted({x['style'] for x in rs})}
        tiers = np.array([int(k.split(':')[1]) for k in labels])
        missed = Counter(x['phrase'].split(':')[0] for x in rs if labels[int(np.argmax(np.asarray(x['scores']) + word_bonus * tiers))] != x['phrase'])
        out[str(f)] = dict(all=report(rs, labels, word_bonus),
                           by_style={k: {n: v for n, v in g.items() if n != 'confusions'} for k, g in groups.items()},
                           misses_by_shout=dict(sorted(missed.items())))
    return out


def main():
    ap = argparse.ArgumentParser(description='hold classifier accuracy on a microphone yardstick with speech time-compressed, pitch kept; hold lead and tail unchanged')
    ap.add_argument('model'); ap.add_argument('audio', help='a yardstick directory written by thuum.human'); ap.add_argument('output')
    ap.add_argument('--factors', type=float, nargs='+', default=FACTORS); ap.add_argument('--threads', type=int, default=4)
    ap.add_argument('--word-bonus', type=float, help='per-word bonus; defaults to the pinned classifier\'s')
    a = ap.parse_args()
    pin = json.loads((ROOT / 'data/hold-classifier.json').read_text())
    bonus = pin['word_bonus'] if a.word_bonus is None else a.word_bonus
    result = dict(model=Path(a.model).name, word_bonus=bonus, stretch='sox tempo -s (WSOLA, pitch kept) on the speech only',
                  curve=curve(Model(a.model, a.threads), a.audio, a.factors, load_table(), bonus))
    Path(a.output).write_text(json.dumps(result, indent=1) + '\n')
    for f, c in result['curve'].items():
        print(f, 'all', c['all']['exact'], '/', c['all']['holds'], 'by_style', {k: (g['exact'], g['holds']) for k, g in c['by_style'].items()},
              'by_word_count', c['all']['by_word_count'], flush=True)


if __name__ == '__main__':
    main()
