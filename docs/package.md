# Package

`python3 -m thuum package <dir>` builds the mod-manager archive `<dir>/thuum-<version>.zip` and prints its size and SHA-256, the DLL's imports and exports, and every member's size and SHA-256. It needs `nix-shell`; packaging publishes nothing.

The command:

- fetches the pinned hold classifier named in `data/hold-classifier.json` into `.scratch/` when it is not already there, from the URL the pin names, and `build.rs` checks its size and SHA-256;
- builds `thuum.dll` for `x86_64-pc-windows-gnu` in release mode with `--locked`, clearing `RUSTFLAGS` and any `RUSTC_WRAPPER`, and remaps the Cargo home, target and source paths so none reach the binary;
- writes `thuum.esp` with `cargo run --bin thuum-esp`;
- refuses a DLL that imports anything beyond Windows system libraries (advapi32, bcrypt, bcryptprimitives, combase, kernel32, msvcrt, ntdll, ole32, oleaut32, propsys, rpcrt4, shell32, userenv, ws2_32 and the api-ms-win sets), exports anything but `SKSEPlugin_Load`, `SKSEPlugin_Query` and `SKSEPlugin_Version`, or contains a machine path (under a home, temporary, system or Nix store directory);
- zips the members in sorted order with fixed timestamps and permissions, so the same tree and toolchain give a byte-identical archive whatever the checkout path. A second checkout at another path, built from scratch into its own target directory, reproduces the archive.

A new classifier only changes the pin; the command is rerun unchanged.

## Layout

The archive's root holds what goes into the game's `Data` folder, the layout mod managers install as is:

| Member | What |
| --- | --- |
| `SKSE/Plugins/thuum.dll` | the plugin: recognizer, hold classifier and tract runtime, all embedded |
| `thuum.esp` | the ESL-flagged plugin with the voice power Thuum |
| `SKSE/Plugins/thuum/LICENSE.txt` | thuum's license (MIT) |
| `SKSE/Plugins/thuum/NOTICE.txt` | third-party notices and license texts |

The DLL is the only file that runs. The esp is a separate file because the game loads records only from plugin files, and the two text files carry the notices the licenses require. Nothing is downloaded, started or configured at run time, and nothing needs calibrating.

## Install and removal

Install SKSE (or SKSE VR), extract the archive into `Data` (or install it with a mod manager), and enable `thuum.esp` in the load order. With the files present but `thuum.esp` not enabled, the plugin logs `thuum.esp is not loaded` and disables itself without opening the microphone. With it enabled, the log shows the shouts resolved, the voice power's form ID, the input sink registered, the shout handler hooked and capture open, and Thuum is listed under Powers.

The plugin starts no process and writes nothing outside the archive's files except its log, `My Games/Skyrim Special Edition/SKSE/thuum.log` (or `Skyrim VR`). Removing the four files and the load-order entry returns the game to vanilla; the log is left behind.

Capture uses the system's default input. Under Proton a default source with no available port (an onboard input with nothing plugged in) is skipped by the audio server, so the stream has no target and every open fails; capture retries every 500 ms and opens by itself as soon as a usable source becomes the default, with no restart.

## Licenses

`NOTICE` at the repository root is the archive's `NOTICE.txt`, and [license-audit.md](license-audit.md) lists every component in the DLL and what trained the classifier, with its license and source. `tests/test_notice.py` fails when `plugin/Cargo.lock` gains or changes a crate that the notice does not list, until the crate is added to the notice or marked as not linked. No game asset ships: the DLL holds shout names, dragon words and form identifiers, and `thuum.esp` refers to records in `Skyrim.esm` by identifier.
