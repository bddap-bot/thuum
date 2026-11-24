import argparse
import hashlib
import json
import wave
from pathlib import Path

import numpy as np

from .table import load_table

PAD, GAP, LEAD, TRAIL, QUIET = 1600, 640, 6400, 16000, 3200


def write_wav(path, pcm):
    with wave.open(str(path), "wb") as stream:
        stream.setparams((1, 2, 16000, 0, "NONE", "not compressed"))
        stream.writeframes(pcm.tobytes())


def read_wav(path):
    with wave.open(str(path)) as stream:
        if stream.getparams()[:3] != (1, 2, 16000):
            raise ValueError("audio must be 16 kHz mono 16-bit")
        return np.frombuffer(stream.readframes(stream.getnframes()), "<i2").copy()


def holds(yardstick):
    for row in json.loads((Path(yardstick) / "yardstick.json").read_text())["rows"]:
        if row["phrase"]:
            yield row, read_wav(Path(yardstick) / f"{row['id']}.wav")


def read_take(root, take):
    pcm = read_wav(Path(root) / (take["take"] + ".wav"))
    if hashlib.sha256(pcm.tobytes()).hexdigest() != take["sha256_pcm"]:
        raise ValueError("take PCM checksum mismatch")
    return pcm


def quiet_spans(take):
    edges = [(i["words"][0][0], i["words"][-1][1]) for i in take["items"]]
    bounds = [0] + [v for s, e in edges for v in (s, e)] + [take["samples"]]
    spans = []
    for a, b in zip(bounds[::2], bounds[1::2]):
        a, b = a + (QUIET if a else 0), b - (QUIET if b < take["samples"] else 0)
        if b > a:
            spans.append([a, b])
    return spans


def plan(labels, phrases):
    rows = []
    for take in labels["takes"]:
        def add(name, spans, phrase, negative):
            rows.append(dict(id=f"{take['take']}-{name}", take=take["take"], speaker=take["speaker"], style=take["style"],
                             phrase=phrase, negative_class=negative, spans=spans))

        for item in take["items"]:
            edges = [item["words"][0][0] - PAD] + [w[1] for w in item["words"][:-1]] + [item["words"][-1][1] + PAD]
            words, shout = [[a, b] for a, b in zip(edges, edges[1:])], item["shout"]
            if item.get("deviation"):
                add(f"{shout}-spoken", [[words[0][0], words[-1][1]]], None, "wrong_word")
                continue
            if item["spoken"] != list(phrases[f"{shout}:3"].words):
                raise ValueError("spoken words differ from the table without a recorded deviation")
            for tier in (1, 2, 3):
                add(f"{shout}-{tier}", [[words[0][0], words[tier - 1][1]]], f"{shout}:{tier}", None)
            add(f"{shout}-wrong", [list(w) for w in reversed(words)], None, "wrong_order")
            add(f"{shout}-continuation", [[words[0][0], words[-1][1]], list(words[0])], None, "continuation")
            add(f"{shout}-tail", [[words[1][0], words[-1][1]]], None, "dragon_nonprefix")
        add("pauses", quiet_spans(take), None, "own_pauses")
    return rows


def bed(pcm, take):
    return np.concatenate([pcm[a:b] for a, b in quiet_spans(take)])


def noise(quiet, identity, count):
    start = int(hashlib.sha256(identity.encode()).hexdigest(), 16) % len(quiet)
    return np.resize(np.roll(quiet, -start), count)


def render(pcm, quiet, row):
    if row["negative_class"] == "own_pauses":
        content = np.concatenate([pcm[a:b] for a, b in row["spans"]])
        return content, len(content)
    content = np.concatenate([v for k, (a, b) in enumerate(row["spans"]) for v in ([noise(quiet, row["id"] + str(k), GAP)] if k else []) + [pcm[a:b]]])
    lead, trail = noise(quiet, row["id"] + "lead", LEAD), noise(quiet, row["id"] + "trail", TRAIL)
    return np.concatenate([lead, content, trail]), LEAD + len(content)


def build(takes_dir, output=None):
    labels_path = Path(takes_dir) / "labels.json"
    labels = json.loads(labels_path.read_text())
    if labels["schema"] != 1:
        raise ValueError("unsupported labels schema")
    rows = plan(labels, load_table())
    if output:
        Path(output).mkdir(parents=True)
    audio = {t["take"]: read_take(takes_dir, t) for t in labels["takes"]}
    quiet = {t["take"]: bed(audio[t["take"]], t) for t in labels["takes"]}
    for row in rows:
        pcm, end = render(audio[row["take"]], quiet[row["take"]], row)
        row["speech_end_sample"], row["samples"], row["sha256_pcm"] = end, len(pcm), hashlib.sha256(pcm.tobytes()).hexdigest()
        if output:
            write_wav(Path(output) / (row["id"] + ".wav"), pcm)
    result = dict(schema=1, labels_sha256=hashlib.sha256(labels_path.read_bytes()).hexdigest(),
                  deviations=[dict(take=t["take"], shout=i["shout"], spoken=i["spoken"], note=i["deviation"])
                              for t in labels["takes"] for i in t["items"] if i.get("deviation")],
                  rows=rows)
    if output:
        (Path(output) / "yardstick.json").write_text(json.dumps(result, indent=1) + "\n")
    return result


def main():
    parser = argparse.ArgumentParser(description="microphone yardstick: one clip per labelled hold, rendered from labelled takes")
    parser.add_argument("takes", help="directory holding labels.json and <take>.wav for each take it labels")
    parser.add_argument("output", help="new directory for yardstick.json and one WAV per row")
    args = parser.parse_args()
    rows = build(args.takes, args.output)["rows"]
    print(json.dumps(dict(rows=len(rows), positives=sum(r["phrase"] is not None for r in rows),
                          takes=sorted({r["take"] for r in rows}))))


if __name__ == "__main__":
    main()
