import numpy as np

from .table import load_table


def vocabulary(phrases):
    return sorted({word for phrase in phrases.values() for word in phrase.words})


def targets(phrases):
    index = {word: i + 1 for i, word in enumerate(vocabulary(phrases))}
    return {key: [index[word] for word in phrase.words] for key, phrase in phrases.items()}


def ctc_log_likelihoods(logp, sequences, blank=0):
    states = 2 * max(len(s) for s in sequences) + 1
    labels = np.full((len(sequences), states), blank)
    for i, s in enumerate(sequences):
        labels[i, 1:2 * len(s):2] = s
    ends = np.array([2 * len(s) for s in sequences])
    valid = np.arange(states)[None] <= ends[:, None]
    skip = np.zeros_like(labels, bool)
    skip[:, 2:] = (labels[:, 2:] != blank) & (labels[:, 2:] != labels[:, :-2])
    alpha = np.full(labels.shape, -np.inf)
    alpha[:, :2] = logp[0, labels[:, :2]]
    alpha[~valid] = -np.inf
    for t in range(1, len(logp)):
        stay = alpha
        step = np.concatenate([np.full((len(labels), 1), -np.inf), alpha[:, :-1]], 1)
        jump = np.concatenate([np.full((len(labels), 2), -np.inf), alpha[:, :-2]], 1)
        prev = np.logaddexp(stay, step)
        prev = np.where(skip, np.logaddexp(prev, jump), prev)
        alpha = np.where(valid, prev + logp[t, labels], -np.inf)
    rows = np.arange(len(labels))
    return np.logaddexp(alpha[rows, ends], alpha[rows, ends - 1])


def closed_set(logp, phrases=None, word_bonus=0.0):
    tg = targets(phrases or load_table())
    sequences = list(tg.values())
    scores = ctc_log_likelihoods(np.asarray(logp, np.float64), sequences) + word_bonus * np.array([len(s) for s in sequences])
    return dict(zip(tg, scores.tolist()))


def decide(logp, phrases=None, word_bonus=0.0):
    scores = closed_set(logp, phrases, word_bonus)
    return max(scores, key=scores.get)


def report(rows, labels, word_bonus):
    tiers = np.array([int(label.split(":")[1]) for label in labels])
    by_count = {int(t): [0, 0] for t in sorted(set(tiers))}
    out = dict(holds=0, exact=0, shout=0, word_count=0, by_word_count=by_count, confusions=[])
    for row in rows:
        guess = labels[int(np.argmax(np.asarray(row["scores"]) + word_bonus * tiers))]
        truth = row["phrase"]
        (ts, tt), (gs, gt) = truth.split(":"), guess.split(":")
        out["holds"] += 1
        out["exact"] += guess == truth
        out["shout"] += gs == ts
        out["word_count"] += gt == tt
        by_count[int(tt)][0] += guess == truth
        by_count[int(tt)][1] += 1
        if guess != truth:
            out["confusions"].append([truth, guess])
    out["confusions"].sort()
    return out
