# Release

What a Nexus Mods upload of thuum needs, prepared and checked.

## Release of record

The upload is the archive `python3 -m thuum package` builds from the release commit, unchanged. The command prints its SHA-256 and each member's; record those here with the archive's MD5, which Nexus Mods indexes, when the release is built.

## Compatibility

| Game | Version | Script extender | Checked |
| --- | --- | --- | --- |
| Skyrim Special Edition (Steam) | 1.7.104 | SKSE 2.3.1 | under Proton: install into a new game folder and prefix, every tier, cooldowns, menus, controller, save/load, the 100-hold run, removal; native Windows 11: load and SKSE exports |
| Skyrim VR (Steam) | 1.4.15 | SKSE VR 2.0.12 | under Proton, a vanilla game with only SKSE VR and thuum, streamed by SteamVR with a virtual microphone: holds decided and fired through the keyboard Shout binding |

- Any other executable is refused: the plugin logs `unsupported executable` with the executable's link timestamp, by which it picks its engine addresses, and does nothing.
- No Address Library is needed.
- `thuum.esp` is ESL-flagged with `Skyrim.esm` as its only master. Skyrim VR has no light plugins of its own: it loads there in a full slot, and in a light slot with Skyrim VR ESL Support ([plugin.md](plugin.md)).
- The archive's root is the `Data` layout, which mod managers install as is. It was checked by extracting it into `Data`; installation through a mod manager is not yet shown.
- Not yet shown: a headset's own microphone and the motion-controller Shout binding, and gameplay acceptance on a headset ([progress.md](progress.md)).

## Redistribution

thuum is MIT, and the [license audit](license-audit.md) finds nothing that blocks redistributing the archive; its `NOTICE.txt` carries the required notices.

Nexus permissions, set at upload:

| Field | Setting |
| --- | --- |
| Upload to other sites | yes, with credit |
| Modification | yes |
| Conversion | yes |
| Asset use | yes |
| Commercial use | yes |
| Credit | thuum's `NOTICE.txt` |

## Wabbajack

A Wabbajack list fetches a Nexus file by game, mod and file ID and checks its hash. Its [troubleshooting guide](https://wiki.wabbajack.org/user_documentation/Troubleshooting%20FAQ.html) lists an archived, moderated or deleted file as unavailable, and a list needing one cannot install until it is rebuilt. So:

- Upload the archive as a MAIN file, unchanged.
- Never archive or delete a released file; a new version is a new file.
- No off-site download or manual step: the archive is the whole mod.

## Nexus page

Game: Skyrim Special Edition, the Nexus site that also hosts Skyrim VR mods. Category: Magic - Gameplay. Name: thuum. Requirements: SKSE64 or SKSE VR. AI tags: AI-Generated Content (the code) and AI Media (the page text), the categories of Nexus Mods' [generative-AI tagging rules](https://www.shacknews.com/article/150216/nexus-mods-gen-ai-tagging-moderation). No images.

Description:

> Shout in the dragon tongue to use your shouts.
>
> Equip the power Thuum, hold the Shout button, say one, two or three words of a shout (Fus, Fus Ro, Fus Ro Dah) and let go. thuum fires the shout you said at the strength you said. Cooldowns apply, and a word you have not learned is refused rather than downgraded. With anything else equipped the Shout button is untouched.
>
> One DLL with the recognizer built in, running on the CPU on its own thread. It recognizes speech only while you hold the button. Nothing to download, start or calibrate; it uses your default microphone.
>
> Supports the Steam releases of Skyrim Special Edition 1.7.104 and Skyrim VR 1.4.15; any other version is left alone. Early release: one-word shouts are the least reliable, and in VR the motion-controller binding and a headset's own microphone are not yet tested.
>
> Free, MIT licensed. Not affiliated with Bethesda.

## Upload

1. Build the archive, record its printed SHA-256s and MD5 in "Release of record", and upload it with these permissions, tags and page text.
2. Download it from its public page while signed out and compare its SHA-256.
3. Look it up by MD5 through the Nexus API (`GET https://api.nexusmods.com/v1/games/skyrimspecialedition/mods/md5_search/<md5>.json`); it must return the new mod and file IDs with the file in MAIN.
4. Record the page, the IDs and each check here.
