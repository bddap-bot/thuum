# Progress

This checklist mirrors the project's tracking issue. The acceptance bars are in [Design](design.md#acceptance).

- [x] 1. Foundation: design, README, AGENTS.md and this checklist.
- [x] 2. Recognizer and corpus: the dragon-only finite table, the pronunciation lexicon and the synthetic corpus tools.
- [x] 3. Game support: Skyrim VR 1.4.15 and Skyrim Special Edition 1.7.104.
- [x] 4. Embedded plugin and flat in-game proof: one DLL with a static CPU runtime and the classifier built in; load, imports, memory-buffer inference, in-process WASAPI capture and reconnect on native Windows and Proton; hold-to-shout firing through the engine's voice path at every strength, with cooldowns, menus, focus, controller coexistence and restart checked.
- [x] 5. Cut: an early headset streaming transport check; the stream and the headset's microphone are checked within VR integration (9) and headset gameplay acceptance (11).
- [x] 6. One-file package: the mod-manager archive with `thuum.dll` and `thuum.esp`, licence audit and notices, clean-prefix install and removal.
- [x] 7. Cut: integration with a large mod list.
- [x] 8. Framerate-only graphics experiment: DLSS super resolution and fixed foveated rendering, each measured against the vanilla stack; neither raised framerate.
- [ ] 9. VR integration: the Skyrim VR Shout binding is the hold; controller coexistence, shared capture, latency and GPU load checked on a real headset stream.
- [x] 10. Cut: gameplay-host setup is outside this project.
- [ ] 11. Headset gameplay acceptance: the accuracy bar, the no-hold zero-firing hour, all strengths, the 100-hold run and release-to-firing p95 below 500 ms, sustained play, restart and reconnect, on a real headset.
- [ ] 12. Nexus publication: the free release with pinned archive and hash, redistribution permissions, compatibility, notices and clean-install evidence, published and its download verified.
- [x] 13. Cut: a mod-list inclusion pitch.
- [ ] 14. Optional: the same plugin on a standalone ARM headset running SteamOS, through FEX and Proton.
- [ ] 15. Acoustic quality: a closed-set hold classifier that meets the accuracy bar on held-out microphone recordings, CPU-only on the recognizer thread inside the DLL, within the 500 ms budget, trained on synthetic speech only, with fresh evaluation recordings covering every prefix.

- [ ] Goal: dragon shouts triggered by speaking their words, reliably, in Skyrim VR played on a standalone headset streaming from a PC.
