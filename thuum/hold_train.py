import argparse, glob, json, math, os, random, time
import numpy as np
import torch
import torch.nn as nn
import torch.nn.functional as F
from pathlib import Path
from scipy.signal import fftconvolve

from .hold_classifier import closed_set, report, targets, vocabulary
from .hold_corpus import cleared
from .human import holds
from .table import load_table
from .tempo import TRAIL, stretch

MODEL = 'UsefulSensors/moonshine-base'
MAX_HOLD = 4 * 16000
TABLE = load_table()
LABELS = list(TABLE)
WORDS = vocabulary(TABLE)
WID = {w: i + 1 for i, w in enumerate(WORDS)}
BONUSES = (0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0)


def out_len(n):
    n = (n - 127) // 64 + 1
    n = (n - 7) // 3 + 1
    return (n - 3) // 2 + 1


class Net(nn.Module):
    def __init__(self, name):
        super().__init__()
        from transformers import MoonshineModel
        self.encoder = MoonshineModel.from_pretrained(name).encoder
        self.head = nn.Linear(self.encoder.config.hidden_size, len(WORDS) + 1)

    def forward(self, x, lengths):
        mask = (torch.arange(x.shape[1], device=x.device)[None] < lengths[:, None]).long()
        h = self.encoder(x, attention_mask=mask).last_hidden_state
        return F.log_softmax(self.head(h), -1), torch.clamp(out_len(lengths), max=h.shape[1])


def corpus(dirs, split):
    for f in sorted(f for d in dirs for f in glob.glob(str(Path(d) / '*.npz'))):
        d = np.load(f)
        m = json.loads(str(d['meta']))
        if not cleared(m['voice']):
            raise ValueError(f'{f}: voice {m["voice"]} is not licence-cleared')
        if m['split'] == split:
            yield f, d, m


def load_corpus(dirs, split):
    clips, targets, voices = [], [], []
    for f, d, m in corpus(dirs, split):
        a, o = d['audio'], d['offsets']
        for i, c in enumerate(m['clips']):
            clips.append(a[o[i]:o[i + 1]]); targets.append([WID[w] for w in c['words']]); voices.append(m['voice'])
    return clips, targets, voices


def pack_corpus(dirs, split, path):
    spans, targets, voices, size = [], [], [], 0
    for f, d, m in corpus(dirs, split):
        o = d['offsets']
        for i, c in enumerate(m['clips']):
            if o[i + 1] - o[i] <= MAX_HOLD:
                spans.append((f, int(o[i]), int(o[i + 1]), size)); targets.append([WID[w] for w in c['words']]); voices.append(m['voice'])
                size += int(o[i + 1] - o[i])
    audio = np.lib.format.open_memmap(path, mode='w+', dtype=np.int16, shape=(size,))
    by_file = {}
    for f, s, e, at in spans:
        by_file.setdefault(f, []).append((s, e, at))
    for f, parts in by_file.items():
        a = np.load(f)['audio']
        for s, e, at in parts:
            audio[at:at + e - s] = a[s:e]
    audio.flush()
    del audio
    return np.load(path, mmap_mode='r'), [(at, at + e - s) for _, s, e, at in spans], targets, voices


class Aug:
    def __init__(self, seed, work, quick=0.0):
        self.rng, self.quick = np.random.default_rng(seed), quick
        self.rirs = np.load(Path(work) / 'rirs.npy', mmap_mode='r')
        self.noise = np.load(Path(work) / 'noise.npy', mmap_mode='r')

    def __call__(self, pcm):
        r = self.rng
        if r.random() < self.quick:
            pcm = stretch(pcm, r.uniform(1.0, 1.8))
        x = pcm.astype(np.float32) / 32768
        f = r.uniform(0.87, 1.15)
        n = int(len(x) / f)
        x = np.interp(np.linspace(0, len(x) - 1, n), np.arange(len(x)), x).astype(np.float32)
        if r.random() < 0.35:
            d = r.uniform(1.5, 6); x = np.tanh(x * d) / np.tanh(d)
        if r.random() < 0.7:
            h = np.asarray(self.rirs[r.integers(len(self.rirs))], np.float32)
            h = h[:int(len(h) * r.uniform(0.2, 1.0))]
            x = fftconvolve(x, h)[:len(x) + 3200].astype(np.float32)
        if r.random() < 0.4:
            k = int(r.choice([3, 5, 9])); x = np.convolve(x, np.ones(k) / k, mode='same').astype(np.float32)
        if r.random() < 0.3:
            x = np.append(x[0], x[1:] - r.uniform(0.5, 0.97) * x[:-1]).astype(np.float32)
        x = x / (np.abs(x).max() + 1e-6)
        rms = np.sqrt(np.mean(x[np.abs(x) > 0.05] ** 2)) if (np.abs(x) > 0.05).any() else 0.1
        lead, trail = int(16000 * r.uniform(0.1, 0.6)), int(16000 * r.uniform(0.15, 0.5))
        y = np.concatenate([np.zeros(lead, np.float32), x, np.zeros(trail, np.float32)])
        if r.random() < 0.85:
            s = r.integers(len(self.noise) - len(y)); nz = np.asarray(self.noise[s:s + len(y)], np.float32)
            y = y + nz / (np.sqrt(np.mean(nz ** 2)) + 1e-6) * rms / 10 ** (r.uniform(0, 30) / 20)
        y = y + r.normal(0, rms / 10 ** (r.uniform(25, 60) / 20), len(y)).astype(np.float32)
        return (y / (np.abs(y).max() + 1e-6) * r.uniform(0.05, 0.95)).astype(np.float32)


class Data(torch.utils.data.IterableDataset):
    def __init__(self, audio, spans, targets, batch, seed, work, quick):
        self.audio, self.spans, self.targets, self.batch, self.seed, self.work, self.quick = audio, spans, targets, batch, seed, work, quick

    def __iter__(self):
        w = torch.utils.data.get_worker_info()
        seed = self.seed * 1000 + (w.id if w else 0)
        aug, rng = Aug(seed, self.work, self.quick), random.Random(seed)
        while True:
            items = [(np.asarray(self.audio[slice(*self.spans[i])]), self.targets[i]) for i in (rng.randrange(len(self.spans)) for _ in range(self.batch))]
            xs = [aug(c) for c, _ in items]
            L = torch.tensor([len(x) for x in xs])
            X = torch.zeros(len(xs), int(L.max()))
            for i, x in enumerate(xs):
                X[i, :len(x)] = torch.from_numpy(x)
            T = [torch.tensor(t) for _, t in items]
            yield X, L, torch.cat(T), torch.tensor([len(t) for t in T])


def score(net, pcms):
    device = next(net.parameters()).device
    net.eval()
    out = []
    with torch.no_grad():
        for pcm in pcms:
            x = torch.from_numpy(pcm.astype(np.float32) / 32768)[None]
            x = (x / (x.abs().max() + 1e-6) * 0.5).to(device)
            lp, _ = net(x, torch.tensor([x.shape[1]], device=device))
            out.append(np.array(list(closed_set(lp[0].cpu().numpy(), TABLE).values())))
    net.train()
    return np.stack(out)


def yardstick(audio):
    for r, pcm in holds(audio):
        yield dict(id=r['id'], phrase=r['phrase'], set=r['take']), pcm[:r['speech_end_sample'] + TRAIL]


def synthetic_heldout(corpus, work, n=1200):
    clips, tg, _ = load_corpus([corpus], 'heldout')
    valid = {tuple(t): label for label, t in zip(LABELS, targets(TABLE).values())}
    idx = [i for i, t in enumerate(tg) if tuple(t) in valid]
    idx = sorted(np.random.default_rng(6237).choice(idx, min(n, len(idx)), replace=False))
    aug = Aug(6237, work)
    for k, i in enumerate(idx):
        yield dict(id=f'synthetic-{k}', phrase=valid[tuple(tg[i])], set='synthetic'), np.clip(aug(clips[i]) * 32767, -32768, 32767).astype(np.int16)


def bonus_sweep(rows):
    return {str(b): report(rows, LABELS, b)['exact'] for b in BONUSES}


class Single(nn.Module):
    def __init__(self, net):
        super().__init__(); self.net = net
        e = net.encoder
        cos, sin = e.rotary_emb(torch.zeros(1, 1024, e.config.hidden_size), position_ids=torch.arange(1024)[None])
        self.register_buffer('cos', cos.detach()); self.register_buffer('sin', sin.detach())

    def forward(self, x):
        e = self.net.encoder
        h = e.groupnorm(torch.tanh(e.conv1(x[:, None])))
        h = F.gelu(e.conv3(F.gelu(e.conv2(h)))).permute(0, 2, 1)
        pe = (self.cos[:, :h.shape[1]], self.sin[:, :h.shape[1]])
        for layer in e.layers:
            h = layer(h, attention_mask=None, position_embeddings=pe)
        return F.log_softmax(self.net.head(e.layer_norm(h)), -1)[0]


def export(net, path):
    m = Single(net.eval())
    torch.onnx.export(m, (torch.randn(1, 32000) * 0.1,), path, input_names=['pcm'], output_names=['logp'],
                      dynamic_shapes=({1: torch.export.Dim('n', min=8000, max=16000 * 20)},), opset_version=18, dynamo=True, external_data=False)
    tidy(path)


def tidy(path):
    import onnx
    g = onnx.load(path)
    del g.graph.value_info[:]
    for o in g.graph.output:
        for d in o.type.tensor_type.shape.dim:
            if d.dim_param:
                d.dim_param = 't'
    # load_onnx reads only fx_node; keeping only it makes an export reproducible.
    for node in g.graph.node:
        keep = [m for m in node.metadata_props if m.key == 'pkg.torch.onnx.fx_node']
        del node.metadata_props[:]
        node.metadata_props.extend(keep)
    onnx.save(g, path)


def pack_noise(rirs_noises, work):
    D = Path(rirs_noises)
    rng = random.Random(6237)

    def mono16(f):
        import soundfile as sf
        x, sr = sf.read(f, dtype='float32', always_2d=True)
        x = x[:, 0]
        if sr != 16000:
            n = int(len(x) * 16000 / sr); x = np.interp(np.linspace(0, len(x) - 1, n), np.arange(len(x)), x).astype(np.float32)
        return x

    rirs = sorted(glob.glob(str(D / 'real_rirs_isotropic_noises/air_type*.wav')))
    sim = sorted(glob.glob(str(D / 'simulated_rirs/*/*/*.wav'))); rng.shuffle(sim)
    out = []
    for f in rirs + sim[:4000]:
        h = mono16(f); p = int(np.abs(h).argmax()); h = h[max(0, p - 40):p + 16000]
        h = h / (np.sqrt((h ** 2).sum()) + 1e-9); out.append(np.pad(h, (0, 16040 - len(h))))
    np.save(Path(work) / 'rirs.npy', np.stack(out).astype(np.float16))
    segs = []
    for f in sorted(glob.glob(str(D / 'pointsource_noises/*.wav'))):
        x = mono16(f)
        if len(x) > 8000 and np.abs(x).max() > 0:
            segs.append((x / (np.sqrt((x ** 2).mean()) + 1e-9) * 0.05).astype(np.float32))
    np.save(Path(work) / 'noise.npy', np.clip(np.concatenate(segs), -1, 1).astype(np.float16))


def load_onnx(net, path):
    import onnx
    from onnx import numpy_helper
    g = onnx.load(path)
    init = {i.name: numpy_helper.to_array(i) for i in g.graph.initializer}
    params = dict(net.named_parameters())
    flat = {'p_net_' + k.replace('.', '_'): k for k in params}
    found = {k[4:]: v for k, v in init.items() if k.startswith('net.') and k[4:] in params}
    for node in g.graph.node:
        fx = next((m.value for m in node.metadata_props if m.key == 'pkg.torch.onnx.fx_node'), '')
        consts = [x for x in node.input if x in init and not x.startswith('net.')]
        if not consts or 'aten.linear' not in fx and 'aten.group_norm' not in fx:
            continue
        args = [a.strip(' %)') for a in fx.split('args = (')[1].split(')')[0].split(',')]
        if 'aten.linear' in fx and node.op_type == 'MatMul':
            found[flat[args[1]]] = init[consts[0]].T
        elif 'aten.group_norm' in fx and node.op_type in ('Mul', 'Add') and init[consts[0]].ndim == 2:
            found[flat[args[2 if node.op_type == 'Mul' else 3]]] = init[consts[0]][:, 0]
    missing = sorted(set(params) - set(found))
    if missing:
        raise ValueError(f'parameters not recovered from {path}: {missing}')
    with torch.no_grad():
        for k, v in found.items():
            params[k].copy_(torch.from_numpy(np.ascontiguousarray(v)).reshape(params[k].shape))


def load_init(net, path):
    if str(path).endswith('.onnx'):
        load_onnx(net, path)
    else:
        net.load_state_dict(torch.load(path, map_location='cpu'))


def save_checkpoint(path, **state):
    torch.save(state, path + '.tmp')
    os.replace(path + '.tmp', path)


def train(a):
    torch.manual_seed(6237); torch.set_num_threads(a.threads)
    audio, spans, targets, voices = pack_corpus(a.corpus, 'train', Path(a.work) / (Path(a.out).name + '.train-audio.npy'))
    print('train clips', len(spans), 'voices', len(set(voices)), flush=True)
    net = Net(a.model)
    if a.init:
        load_init(net, a.init)
    net = net.to(a.device)
    ema = torch.optim.swa_utils.AveragedModel(net, multi_avg_fn=torch.optim.swa_utils.get_ema_multi_avg_fn(a.ema)) if a.ema else None
    kept = ema.module if ema else net
    opt = torch.optim.AdamW([dict(params=net.encoder.parameters(), lr=a.lr), dict(params=net.head.parameters(), lr=a.lr * 5)], weight_decay=0.01)
    sched = torch.optim.lr_scheduler.LambdaLR(opt, lambda s: min(1, (s + 1) / 500) * 0.5 * (1 + math.cos(math.pi * min(1, s / a.steps))))
    dl = torch.utils.data.DataLoader(Data(audio, spans, targets, a.batch, 6237, a.work, a.quick), batch_size=None, num_workers=a.workers, prefetch_factor=4, multiprocessing_context='fork')
    held = list(synthetic_heldout(a.corpus[0], a.work, 400))
    start = 0
    if os.path.exists(a.out + '.ckpt'):
        ck = torch.load(a.out + '.ckpt', map_location=a.device)
        net.load_state_dict(ck['net']); opt.load_state_dict(ck['opt']); sched.load_state_dict(ck['sched']); start = ck['step']
        if ema:
            ema.load_state_dict(ck['ema'])
        print('resumed at', start, flush=True)
    t0, smooth = time.time(), None
    for step, (X, L, T, TL) in enumerate(dl, start):
        if step >= a.steps:
            break
        lp, fl = net(X.to(a.device), L.to(a.device))
        loss = F.ctc_loss(lp.transpose(0, 1).float().cpu(), T, fl.cpu(), TL, blank=0, zero_infinity=True)
        opt.zero_grad(); loss.backward(); nn.utils.clip_grad_norm_(net.parameters(), 1.0); opt.step(); sched.step()
        if ema:
            ema.update_parameters(net)
        smooth = loss.item() if smooth is None else 0.98 * smooth + 0.02 * loss.item()
        if step % 100 == 0:
            print(step, round(smooth, 3), round(time.time() - t0), flush=True)
        if (step + 1) % a.eval_every == 0 or step + 1 == a.steps:
            torch.save(kept.state_dict(), a.out)
            save_checkpoint(a.out + '.ckpt', net=net.state_dict(), opt=opt.state_dict(), sched=sched.state_dict(), step=step + 1, ema=ema and ema.state_dict())
            rows = [dict(m, scores=v) for (m, _), v in zip(held, score(kept, [p for _, p in held]))]
            print('synthetic held-out', step + 1, bonus_sweep(rows), flush=True)


def scores(a):
    torch.set_num_threads(a.threads)
    net = Net(a.model); load_init(net, a.weights); net.to(a.device)
    rows = []
    sources = ([synthetic_heldout(a.corpus, a.work)] if a.corpus else []) + ([yardstick(a.yardstick)] if a.yardstick else [])
    for source in sources:
        meta, pcms = zip(*source)
        for m, s in zip(meta, score(net, pcms)):
            rows.append(dict(m, scores=s.tolist()))
    Path(a.output).write_text(json.dumps(dict(labels=LABELS, rows=rows)))


def soup(out, members):
    sds = [torch.load(p, map_location='cpu') for p in members]
    torch.save({k: sum(sd[k].float() for sd in sds) / len(sds) for k in sds[0]}, out)


def main():
    ap = argparse.ArgumentParser(description='hold classifier: pretrained encoder with a dragon-word CTC head, trained on synthetic speech')
    sub = ap.add_subparsers(dest='command', required=True)
    n = sub.add_parser('noise'); n.add_argument('rirs_noises'); n.add_argument('work')
    t = sub.add_parser('train'); t.add_argument('out'); t.add_argument('--work', required=True); t.add_argument('--corpus', required=True, nargs='+', help='corpus directories; the first one supplies the synthetic held-out check')
    t.add_argument('--model', default=MODEL); t.add_argument('--init', help='weights to start from: a state dict, or an exported ONNX classifier of the same model')
    t.add_argument('--steps', type=int, default=4000); t.add_argument('--batch', type=int, default=32)
    t.add_argument('--lr', type=float, default=2e-4); t.add_argument('--threads', type=int, default=8)
    t.add_argument('--workers', type=int, default=4); t.add_argument('--eval-every', type=int, default=1000)
    t.add_argument('--device', default='cpu'); t.add_argument('--quick', type=float, default=0.0, help='probability that a draw is time-compressed 1.0-1.8x with pitch kept')
    t.add_argument('--ema', type=float, default=0.0, help='save an exponential moving average of the weights with this decay instead of the raw weights')
    u = sub.add_parser('soup', help='uniform weight average of runs fine-tuned from the same encoder'); u.add_argument('out'); u.add_argument('members', nargs='+')
    e = sub.add_parser('export'); e.add_argument('weights'); e.add_argument('output'); e.add_argument('--model', default=MODEL)
    d = sub.add_parser('tidy', help="keep only the node metadata --init reads, in place; weights and outputs are unchanged"); d.add_argument('onnx')
    s = sub.add_parser('scores'); s.add_argument('weights'); s.add_argument('output'); s.add_argument('--model', default=MODEL)
    s.add_argument('--corpus'); s.add_argument('--work'); s.add_argument('--yardstick', help='a yardstick directory written by thuum.human'); s.add_argument('--threads', type=int, default=8); s.add_argument('--device', default='cpu')
    a = ap.parse_args()
    if a.command == 'noise':
        pack_noise(a.rirs_noises, a.work)
    elif a.command == 'train':
        train(a)
    elif a.command == 'soup':
        soup(a.out, a.members)
    elif a.command == 'export':
        net = Net(a.model); net.load_state_dict(torch.load(a.weights, map_location='cpu')); export(net, a.output)
    elif a.command == 'tidy':
        tidy(a.onnx)
    else:
        scores(a)


if __name__ == '__main__':
    main()
