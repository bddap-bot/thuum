# Design

thuum fires Skyrim's dragon shouts by voice: hold the Shout button, speak the first one, two or all three words of a shout in the dragon tongue, and release. It targets Skyrim VR played on a standalone headset streaming from a PC, and also runs in Skyrim Special Edition, under Proton on Linux and natively on Windows.

It is one SKSE plugin DLL and one ESL-flagged plugin file, installed through a mod manager like any other mod. The recognizer runs inside the DLL on its own thread, with its model built in: no separate program, service, download or calibration.

## Trigger: hold to shout

While the game's Shout input is held, the recognizer thread keeps the microphone audio; on release it classifies that one utterance as the nearest of the 81 closed-set entries (every shout at one, two or three words) and fires that shout with that word count. Each Skyrim shout is one discrete event, so one hold gives one decision, and nothing fires without a hold. A pre-roll of about 300 ms buffered before the press keeps a first syllable spoken with the press. Every hold is classified and fires its nearest entry: holding the button already means the player is shouting, so no loudness or speech gate applies. Unlearned words, recovery and menus refuse the command; it is never downgraded.

The button is the game's own Shout input, whatever the player binds to it (RB on a pad, the keyboard key, the Skyrim VR controller binding). thuum replaces the button entry in the player Shout handler's vtable, with one address per runtime.

Voice control is its own equippable power. The lesser power Thuum comes from `thuum.esp`; a start-game-enabled quest gives it to the player, so it shows in the Magic menu and favorites like any power. While Thuum is equipped at the press, thuum takes the press and release itself: the hold listens, and its release fires the spoken shout at the spoken strength, after which the game re-equips Thuum. Thuum never casts anything itself. While anything else is equipped (a power, an ordinary shout or nothing), every press goes to the vanilla handler unchanged.

Not built: a loudness gate, so a shout fires only when it is actually yelled. As a hands-free trigger it would need false-activation calibration over hours of negative audio, which hold-to-shout does not.

## Recognizer

Dragon-tongue words only: English names and spoken controls are out. The finite table in `data/shouts.json` holds the 27 base-game, Dawnguard and Dragonborn shouts; `thuum.table` derives the 81 entries and rejects shared textual prefixes.

The classifier is a pretrained speech encoder, Moonshine base, with a linear CTC head of 82 classes: the blank and the 81 dragon words, each word one token. A hold is decided by the CTC log-likelihood of each entry's word sequence over the whole hold, plus a fixed bonus per word, and the argmax fires. Entries are prefixes of each other, so the word count is part of the same decision; the bonus counters CTC's bias toward dropping a weak final word. `data/hold-classifier.json` pins the classifier file, its hashes and its bonus, and `thuum.hold_classifier` defines the decision the plugin reproduces.

The classifier is trained only on synthetic speech from licence-cleared voices, augmented with cleared room responses and noise; no recording of a person trains its weights, and the word bonus is chosen on synthetic held-out voices only. [Training](training.md) has the corpus, recipe and how to reproduce the pin.

The plugin runs the classifier with [tract](https://github.com/sonos/tract), statically linked, from a memory buffer, CPU-only: framerate comes first and the GPU belongs to the game. A hold longer than 4 s keeps its last 4 s, which bounds classification time.

## Capture and firing

Capture is WASAPI on the system's default capture endpoint, in the game process; under Proton it reaches PipeWire or PulseAudio through Wine's audio driver. thuum follows the default endpoint as it changes and reopens after a device or audio-server restart. It never selects a device itself: a setup that makes an output's monitor the default capture source records the game's own audio.

Decisions go through a bounded queue to an input-event sink, which runs in the game's input-poll context, and fire through the engine's own voice path, so cooldowns, unlocks and game restrictions apply as they do to a spoken shout on a pad. Details and measurements: [Game plugin](plugin.md).

## Graphics

Framerate is the top graphics priority, above image quality. thuum adds no GPU work. Two framerate levers were measured on Skyrim VR under Proton and neither raised framerate: DLSS super resolution through the Community Shaders VR line ran 34–67% below the vanilla frame rate, because its main-thread cost exceeded the whole vanilla frame interval; fixed foveated rendering through a Variable Rate Shading shim cannot engage, because dxvk-nvapi implements no D3D11 shading-rate calls.

## Acceptance

- Every playable base-game and included-expansion dragon shout and valid prefix.
- At least 96% exact shout-and-tier closed-set top-1 accuracy overall and 95% per acoustic-condition and strength group, on held-out microphone recordings cut as a button hold cuts them, with at least five takes per prefix and condition, including game audio and dialogue playing during the hold. Synthetic results are labelled synthetic and do not count toward this bar. All per-word confusions are listed.
- With no hold, nothing fires: an hour of mixed game audio, silence and conversation with no press produces zero firings.
- Release-to-firing p95 below 500 ms for each strength and condition under gameplay and streaming load.
- A 100-hold in-game run with no wrong shout or tier, duplicate or missed fire, plus cooldown, unlearned-word, menu, focus, controller, save/load, restart and microphone-reconnect checks.
- Game restrictions preserved: an unavailable tier is refused, never downgraded.

[Progress](progress.md) tracks which of these are met.

## Distribution

thuum is free, under the MIT licence, with every third-party notice in [NOTICE](../NOTICE) and the audit behind it in [License audit](license-audit.md). The archive holds `thuum.dll` and `thuum.esp`; the DLL contains the plugin, its runtime and the classifier weights. It contains no game files, audio, textures or records, and no Bethesda marks or logos.
