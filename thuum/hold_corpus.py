import argparse, hashlib, json, os, random, re, subprocess, tempfile, wave
from functools import lru_cache
from multiprocessing import Pool
from pathlib import Path

import numpy as np
import onnxruntime as ort

from .table import load_table

ROOT = Path(__file__).resolve().parent.parent
VOICES = Path(os.environ.get('THUUM_PIPER_VOICES', 'piper-voices'))
TABLE = load_table()
LABELS = list(TABLE)
LEX = {e['word']: e for e in json.loads((ROOT / 'data/lexicon.json').read_text())['entries']}
WORDS = sorted(LEX)
ESPEAK_DIALECTS = ['en-us', 'en-gb', 'en-gb-scotland', 'en-gb-x-rp', 'en-029', 'en-us-nyc', 'en-gb-x-gbclan', 'en-gb-x-gbcwmd']
ESPEAK_VARIANTS = ['', 'm1', 'm2', 'm3', 'm4', 'm5', 'm6', 'm7', 'f1', 'f2', 'f3', 'f4', 'klatt', 'klatt2', 'croak', 'grandpa']
FLITE = ['slt', 'rms', 'awb', 'kal', 'kal16']
PIPER = ['en_US-kristin-medium', 'en_US-john-medium', 'en_US-norman-medium', 'en_US-ljspeech-medium', 'en_US-libritts-high', 'en_GB-cori-medium',
         'nl_BE-rdh-medium', 'nl_NL-alex-medium', 'no_NO-nvcc-medium', 'sv_SE-nst-medium']
VITS = {'vctk': ('kakao-enterprise/vits-vctk', '31b531929b1a364750c819acccb41187a1c1603f', 109)}


def piper_models():
    return {m: json.loads((VOICES / (m + '.onnx.json')).read_text()).get('num_speakers', 1) for m in PIPER}


def cleared(vid):
    engine, voice = vid.split(':')[:2]
    return voice in {'piper': PIPER, 'vits': VITS, 'flite': FLITE, 'espeak': [d + ('+' + v if v else '') for d in ESPEAK_DIALECTS for v in ESPEAK_VARIANTS]}[engine]


def split_of(vid):
    h = int(hashlib.sha256(vid.encode()).hexdigest(), 16) % 100
    return 'heldout' if h < 6 else 'train'


def voices():
    out = [('espeak', d + ('+' + v if v else ''), None) for d in ESPEAK_DIALECTS for v in ESPEAK_VARIANTS]
    out += [('flite', v, None) for v in FLITE]
    for model, n in piper_models().items():
        out += [('piper', model, s) for s in range(n)]
    for name, (_, _, n) in VITS.items():
        out += [('vits', name, s) for s in range(n)]
    return out


@lru_cache(maxsize=8192)
def espeak_ipa(text, voice='en-us'):
    return subprocess.run(['espeak-ng', '-q', '--ipa', '-v', voice, text], capture_output=True, text=True, check=True).stdout.strip()


class Piper:
    def __init__(self, model):
        o = ort.SessionOptions(); o.intra_op_num_threads = 1; o.inter_op_num_threads = 1
        self.session = ort.InferenceSession(str(VOICES / (model + '.onnx')), o, providers=['CPUExecutionProvider'])
        self.config = json.loads((VOICES / (model + '.onnx.json')).read_text())
        self.map = self.config['phoneme_id_map']
        self.rate = self.config['audio']['sample_rate']
        self.multi = self.config.get('num_speakers', 1) > 1
        self.lang = self.config.get('espeak', {}).get('voice', 'en-us')

    def run(self, phonemes, speaker, slow, rng):
        ids = list(self.map['^']) + list(self.map['_'])
        for s in phonemes:
            if s in self.map:
                ids += list(self.map[s]) + list(self.map['_'])
        ids += list(self.map['$'])
        feeds = {'input': np.array([ids], np.int64), 'input_lengths': np.array([len(ids)], np.int64),
                 'scales': np.array([rng.uniform(0.4, 0.9), slow, rng.uniform(0.5, 1.0)], np.float32)}
        if self.multi:
            feeds['sid'] = np.array([speaker], np.int64)
        return self.session.run(None, feeds)[0].reshape(-1), self.rate


class Vits:
    def __init__(self, name):
        import torch
        from transformers import AutoTokenizer, VitsModel
        torch.set_num_threads(1)
        repo, revision, _ = VITS[name]
        self.model = VitsModel.from_pretrained(repo, revision=revision).eval()
        self.map = AutoTokenizer.from_pretrained(repo, revision=revision).get_vocab()
        self.rate = self.model.config.sampling_rate
        self.lang = 'en-us'

    def run(self, phonemes, speaker, slow, rng):
        import torch
        from transformers import set_seed
        set_seed(rng.randrange(2 ** 31))
        ids = [0]
        for s in phonemes:
            if s in self.map:
                ids += [self.map[s], 0]
        m = self.model
        m.noise_scale, m.noise_scale_duration, m.speaking_rate = rng.uniform(0.4, 0.9), rng.uniform(0.5, 1.0), 1 / slow
        with torch.no_grad():
            return m(input_ids=torch.tensor([ids]), speaker_id=speaker).waveform[0].numpy(), self.rate


def long_u(entry):
    ph = entry['piper_phonemes']
    return dict(entry, piper_phonemes=[q for i, p in enumerate(ph) for q in (['u', 'ː'] if p == 'ʊ' and (i == 0 or ph[i - 1] not in 'oa') else [p])],
                espeak_phonemes=re.sub(r'(?<![oa])U', 'u:', entry['espeak_phonemes']),
                flite_phones=['uw' if p == 'uh' else p for p in entry['flite_phones']])


def synth(engine, voice, speaker, words, route, slow, rng, piper, tmp, oo=False):
    lex, text = {w: long_u(LEX[w]) if oo else LEX[w] for w in words}, ' '.join(words)
    if engine in ('piper', 'vits'):
        if route == 'ipa':
            ph = ' '.join('ˈ' + ''.join(lex[w]['piper_phonemes']) for w in words).replace('g', 'ɡ')
        else:
            ph = espeak_ipa(text + '!' if rng.random() < 0.3 else text, piper.lang)
        return piper.run(list(ph), speaker, slow, rng)
    path = Path(tmp) / 'u.wav'
    if engine == 'espeak':
        inp = '[[' + ' '.join(lex[w]['espeak_phonemes'] for w in words) + ']]' if route == 'ipa' else text
        subprocess.run(['espeak-ng', '-v', voice, '-s', str(int(rng.randint(150, 200) / slow)), '-p', str(rng.randint(20, 80)),
                        '-a', '150', '-w', str(path), inp], check=True, timeout=30)
    else:
        if route == 'ipa':
            subprocess.run(['flite', '-voice', voice, '--setf', f'duration_stretch={slow:.2f}', '-p',
                            ' pau '.join(' '.join(lex[w]['flite_phones']) for w in words), '-o', str(path)], check=True, timeout=30)
        else:
            subprocess.run(['flite', '-voice', voice, '--setf', f'duration_stretch={slow:.2f}', '-t', text, '-o', str(path)], check=True, timeout=30)
    with wave.open(str(path)) as s:
        rate = s.getframerate()
        x = np.frombuffer(s.readframes(s.getnframes()), '<i2').astype(np.float32) / 32768
    return x, rate


def to16k(x, rate):
    n = int(round(len(x) * 16000 / rate))
    if n < 10:
        return np.zeros(0, np.float32)
    return np.interp(np.linspace(0, len(x) - 1, n), np.arange(len(x)), x).astype(np.float32)


def trim(x):
    a = np.flatnonzero(np.abs(x) > 0.01 * (np.abs(x).max() + 1e-9))
    return x[a[0]:a[-1] + 1] if len(a) > 400 else None


def sequences(rng, n, single=0.0):
    firsts = sorted({TABLE[label].words[0] for label in LABELS})
    out = []
    for _ in range(n):
        if single and rng.random() < single:
            out.append((rng.choice(firsts if rng.random() < 0.7 else WORDS),))
        elif rng.random() < 0.7:
            out.append(TABLE[rng.choice(LABELS)].words)
        else:
            out.append(tuple(rng.choice(WORDS) for _ in range(rng.choice([1, 2, 2, 3, 3, 4]))))
    return out


def job(args):
    engine, voice, speaker, per_voice, single, oo, fast, out = args
    vid = f'{engine}:{voice}' + ('' if speaker is None else f':{speaker}')
    variant = (f':{single}' if single else '') + (f':oo{oo}' if oo else '') + (f':fast{fast}' if fast else '')
    path = Path(out) / (hashlib.sha256((vid + variant).encode()).hexdigest()[:16] + '.npz')
    if path.exists():
        return vid, -1
    rng = random.Random(f'6237:{vid}' + variant)
    piper = Piper(voice) if engine == 'piper' else Vits(voice) if engine == 'vits' else None
    clips, meta = [], []
    with tempfile.TemporaryDirectory() as tmp:
        for words in sequences(rng, per_voice, single):
            route = rng.choice(['ipa', 'text'])
            separated = rng.random() < 0.6
            slow = rng.uniform(0.9, 1.7) if separated else rng.uniform(0.8, 1.4)
            quick = bool(fast) and rng.random() < fast
            if quick:
                slow = rng.uniform(0.5, 0.85)
            long = bool(oo) and rng.random() < oo and route == 'ipa' and any(long_u(LEX[w]) != LEX[w] for w in words)
            try:
                if separated:
                    parts, bounds, t = [], [], 0
                    for w in words:
                        x, rate = synth(engine, voice, speaker, (w,), route, slow, rng, piper, tmp, long)
                        x = trim(to16k(x, rate))
                        if x is None:
                            raise ValueError
                        gap = np.zeros(int(16000 * rng.uniform(0.0, 0.12 if quick else 0.45)), np.float32) if parts else np.zeros(0, np.float32)
                        parts += [gap, x / (np.abs(x).max() + 1e-6) * rng.uniform(0.5, 1.0)]
                        t += len(gap); bounds.append((t, t + len(x))); t += len(x)
                    x = np.concatenate(parts)
                else:
                    x, rate = synth(engine, voice, speaker, words, route, slow, rng, piper, tmp, long)
                    x = trim(to16k(x, rate))
                    if x is None:
                        raise ValueError
                    bounds = None
            except (subprocess.CalledProcessError, subprocess.TimeoutExpired, ValueError):
                continue
            x = x / (np.abs(x).max() + 1e-6)
            clips.append(np.clip(x * 32767, -32768, 32767).astype(np.int16))
            meta.append(dict(words=list(words), route=route, separated=separated, bounds=bounds, oo=long, fast=quick))
    offs = np.cumsum([0] + [len(c) for c in clips])
    np.savez(path, audio=np.concatenate(clips) if clips else np.zeros(0, np.int16), offsets=offs,
             meta=json.dumps(dict(voice=vid, split=split_of(vid), clips=meta)))
    return vid, len(clips)


def main():
    parser = argparse.ArgumentParser(description='synthetic dragon-word corpus for the hold classifier; development audio only')
    parser.add_argument('output')
    parser.add_argument('--per-voice', type=int, default=80)
    parser.add_argument('--procs', type=int, default=8)
    parser.add_argument('--single', type=float, default=0.0, help='fraction of one-word sequences, mostly first words of shouts')
    parser.add_argument('--oo', type=float, default=0.0, help='probability that an IPA-route sequence reads the short u as long, the dragon alphabet\'s value')
    parser.add_argument('--fast', type=float, default=0.0, help='probability that a sequence is spoken fast: length scale 0.5-0.85 and word gaps of at most 120 ms')
    parser.add_argument('--engines', nargs='+', default=['espeak', 'flite', 'piper', 'vits'], help='voice only these engines')
    args = parser.parse_args()
    Path(args.output).mkdir(parents=True, exist_ok=True)
    vs = [v for v in voices() if v[0] in args.engines]
    random.Random(6237).shuffle(vs)
    with Pool(args.procs) as p:
        for i, (vid, n) in enumerate(p.imap_unordered(job, [(e, v, s, args.per_voice, args.single, args.oo, args.fast, args.output) for e, v, s in vs])):
            print(json.dumps(dict(done=i + 1, of=len(vs), voice=vid, clips=n)), flush=True)


if __name__ == '__main__':
    main()
