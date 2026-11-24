# Training the hold classifier

The pinned classifier (`data/hold-classifier.json`) is a uniform weight soup of ten fine-tuning runs of Moonshine base, each trained only on synthetic speech. This page describes the corpus, the recipe and how to reproduce it. The sources and their licences are in [License audit](license-audit.md) and [NOTICE](../NOTICE).

## Dependencies

Beyond `requirements-dev.txt`: torch, transformers, onnx, onnxscript, scipy and soundfile; espeak-ng, flite and sox on the path; the ten cleared Piper voices from [rhasspy/piper-voices](https://huggingface.co/rhasspy/piper-voices) and their `.onnx.json` files in the directory named by `THUUM_PIPER_VOICES`; and [OpenSLR 28](https://www.openslr.org/28/) (`RIRS_NOISES`). The VITS VCTK model downloads from Hugging Face at its pinned revision.

## Corpus

`python3 -m thuum.hold_corpus <out>` synthesises dragon-word sequences with eSpeak NG (8 English dialects × 16 variants), flite (5 voices), the cleared Piper voices and VITS VCTK (109 speakers). 70% of the sequences are valid shout prefixes and 30% random dragon-word sequences of one to four words. Each uses the IPA route (phonemes from `data/lexicon.json`) or the text route (the spelling read as English) at random; some are synthesised word by word and joined with short gaps, the rest as one phrase. About 6% of voices are held out by a hash of the voice id.

Options: `--per-voice` sequences per voice, `--single` the fraction of one-word sequences, `--oo` the probability that the IPA route reads the short u as long, `--fast` the probability of fast speech, `--engines` the engines to voice. `thuum.hold_corpus` voices only its fixed list of cleared voices, and `thuum.hold_train` refuses a corpus file from any other voice.

`python3 -m thuum.hold_train noise <RIRS_NOISES> <work>` packs the simulated and Aachen room responses and the public-domain point-source noises. Training augments every draw with tempo and pitch changes, saturation, those room responses and noises at 0–30 dB SNR, filtering, and hold-like silence before and after.

## Recipe

Each run starts from `UsefulSensors/moonshine-base` and trains 10,000 steps of batch 32, warmup then cosine. `--quick p` time-compresses a draw by 1.0–1.8x with the pitch kept, with probability p; `--ema d` saves an exponential moving average of the weights.

| Run | Corpora | Options |
| --- | --- | --- |
| C1 | syn2, synfast, syn1 | |
| C3 | syn2, synfast, syn1 | `--ema 0.999` |
| N1 | syn2, synfast, syn1 without VCTK | |
| E1 | syn2, synfast, syn1 without VCTK | `--quick 0.3` |
| E2 | syn2, synfast, syn1 | `--quick 0.3` |
| E3 | syn2, synfast, syn1, vctk4 | `--quick 0.3` |
| E4 | syn2, synfast, syn1 without VCTK | `--quick 0.5` |
| E5 | syn2, synfast, syn1, vctk4 | `--quick 0.5` |
| E6 | syn2, synfast, vctk4 | `--quick 0.3` |
| E7 | syn2, synfast, syn1, vctk4 | `--quick 0.3 --lr 1.5e-4` |

The word bonus is chosen on a 1,200-clip sweep over the synthetic held-out voices only; no recording of a person chooses it. Single runs of one recipe differ noticeably, and the soup scores at or above its best member, which is why the pin is a soup.

```console
python3 -m thuum.hold_corpus syn2 --per-voice 80 --oo 0.5
python3 -m thuum.hold_corpus synfast --per-voice 80 --oo 0.5 --fast 1.0
python3 -m thuum.hold_corpus syn1 --per-voice 40 --single 1.0 --oo 0.5
python3 -m thuum.hold_corpus vctk4 --engines vits --per-voice 320 --oo 0.5 --fast 0.3
python3 -m thuum.hold_corpus syn2-novctk --engines espeak flite piper --per-voice 80 --oo 0.5
python3 -m thuum.hold_corpus synfast-novctk --engines espeak flite piper --per-voice 80 --oo 0.5 --fast 1.0
python3 -m thuum.hold_corpus syn1-novctk --engines espeak flite piper --per-voice 40 --single 1.0 --oo 0.5
python3 -m thuum.hold_train noise path/to/RIRS_NOISES work
python3 -m thuum.hold_train train c1.pt --work work --corpus syn2 synfast syn1 --model UsefulSensors/moonshine-base --steps 10000 --device cuda
python3 -m thuum.hold_train soup accent.pt c1.pt c3.pt n1.pt e1.pt e2.pt e3.pt e4.pt e5.pt e6.pt e7.pt
python3 -m thuum.hold_train export accent.pt hold-classifier.onnx --model UsefulSensors/moonshine-base
```

The other nine runs change `train`'s corpora and options as in the table. Regenerating a corpus is near-identical but not bit-identical, because a synthesis call that times out under load is skipped. `export` keeps only the node metadata `--init` reads back, so an export carries no machine paths and is the same on every machine; `tidy` applies the same step to an existing export.

## Measuring

`python3 -m thuum.hold_train scores <weights> <out.json> --corpus <corpus> --work <work>` scores the synthetic held-out voices. Acceptance is measured on held-out microphone recordings: `python3 -m thuum.human <takes> <yardstick>` renders one clip per labelled hold from a directory of takes (16 kHz mono 16-bit `<take>.wav`) and their `labels.json`, and `scores --yardstick <yardstick>` and `python3 -m thuum.tempo <onnx> <yardstick> <out.json>` (accuracy against speaking tempo) read the result.

`labels.json` has `"schema": 1` and a `takes` list. Each take names its WAV (`take`), a `speaker` and a `style` label, its `samples` count and the SHA-256 of its PCM (`sha256_pcm`), and lists `items` in table order, one per shout: `shout` (the table id), `spoken` (the three words heard) and `words` (each word's start and end sample, contiguous). An item whose `spoken` words differ from the table carries a `deviation` note and becomes a wrong-word negative. From each item `thuum.human` cuts the one-, two- and three-word holds and the wrong-order, continuation and tail negatives, and pads each with the take's own quiet.

On an evaluation set of 237 microphone recordings of holds (up to three per prefix), not distributed, the pinned classifier decides 230 exactly (97.0%); shouted one-word holds are at 48/52. At 1.75x speaking tempo it decides 226. The set informed which recipes went into the soup, so these figures are not an acceptance result; acceptance needs fresh recordings with five takes per prefix and condition ([Design](design.md#acceptance)). The misses are mostly short first words (od/lok, su/zul, iiz/mid) and two-word holds read as one word.
