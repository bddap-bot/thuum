# thuum

A Skyrim mod that fires a dragon shout when you hold the Shout button and speak its words in the dragon tongue. Hold the button, say the first one, two or all three words of a shout (Fus, Fus Ro, Fus Ro Dah), and let go: the words select the shout and its strength. It is aimed at Skyrim VR played on a standalone headset streaming from a PC, and runs in Skyrim Special Edition too.

**Status: early development.** The plugin works in game; its recognizer does not yet meet every accuracy target.

## Install

Requires [SKSE](https://skse.silverlock.org/) (SKSE VR for Skyrim VR); supported game versions are listed in [Release](docs/release.md#compatibility). Install the `thuum-<version>.zip` archive with a mod manager and enable `thuum.esp`. In game, equip the lesser power **Thuum** from the Magic menu; while it is equipped, holding the Shout button listens, and releasing it fires the shout you spoke, under the normal game rules (learned words, cooldowns). With anything else equipped the Shout button works as usual. thuum uses the system's default microphone and needs no setup or calibration. Its log is `My Games/<game>/SKSE/thuum.log`.

## Build

The plugin is Rust, cross-compiled to Windows from Linux with Nix; the Python package holds the training, evaluation and packaging tools.

Prerequisites: Linux with [Nix](https://nixos.org/download/) and a `nixpkgs` channel (`plugin/shell.nix` imports `<nixpkgs>` and provides rustup and the MinGW-w64 cross compiler; rustup installs the toolchain `plugin/rust-toolchain.toml` pins), Python 3.11 or later, and `sox` on `PATH` for `tests/test_tempo.py`. Training has its own requirements ([Training](docs/training.md)).

```console
pip install -r requirements-dev.txt
python3 -m unittest discover -s tests
python3 -m thuum package out
cd plugin && THUUM_CLASSIFIER=../.scratch/hold-classifier-accent-r2.onnx nix-shell shell.nix --run 'cargo test'
```

`package` downloads the pinned classifier from this repository's [release](https://github.com/bddap-bot/thuum/releases/tag/hold-classifier-accent-r2) and checks its SHA-256, builds `thuum.dll`, writes `thuum.esp` and zips both into `out/thuum-<version>.zip`; the plugin's tests then use the classifier it fetched into `.scratch/`. See [Package](docs/package.md) and [Game plugin](docs/plugin.md).

## Documentation

- [Design and acceptance](docs/design.md)
- [Progress](docs/progress.md)
- [Game plugin](docs/plugin.md)
- [Training the classifier](docs/training.md)
- [Package](docs/package.md)
- [License audit](docs/license-audit.md)
- [Release](docs/release.md)

## License

MIT; see [LICENSE](LICENSE). Third-party notices are in [NOTICE](NOTICE). thuum is not affiliated with Bethesda and ships no game assets.
