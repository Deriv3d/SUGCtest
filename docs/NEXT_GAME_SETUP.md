# Before starting the Uncharted 2 port

A checklist for the owner, written 2026-10-07 when the SUGC trial ended. Do these, then
paste [NEXT_GAME_PROMPT.md](NEXT_GAME_PROMPT.md) (below its line) into the new project.

## 1. Trial cleanup (requested 2026-10-07, carried out in the trial's Phase 0 thread)

Ez asked for all trial game data to be deleted, including the SUGC `.dec.iso` and the
lab's separate RPCS3 config copy, and for `C:\SUGC-LAB` to be renamed `C:\rustREWRITE`.
What stays there is `tools\` (portable Java 21, Ghidra 12.1.3, ghidra-mcp,
Ps3GhidraScripts, RenderDoc, Rust, MinGW-w64), `scripts\` (the capture scripts, which
were never committed anywhere else) and `reports\` (own-words lab reports). That folder
becomes the Uncharted 2 lab.

Ez is keeping the GitHub repo and renaming it from `SUGCtest` to `UC2RewriteRust`, so
it becomes the Uncharted 2 port's repo. It contains no game data.

## 2. Create for Uncharted 2

1. **Your own Uncharted 2 dump.** Dump your own disc to a `.dec.iso`, and add it to RPCS3.
   Check that it boots and plays there. Decide whether to use the base game or a specific
   update, and write that version down. The prompt asks for it.
2. **Disk space** on the drive holding `C:\rustREWRITE`: about 80 GB free is enough to
   start. Rough breakdown (estimates): dump about 20 GB, extracted files 20 to 40 GB,
   captures 10 to 30 GB, Rust builds and Ghidra 10 to 15 GB, so 60 to 100 GB in total.
3. **Rename the lab folder by hand** from `C:\SUGC-LAB` to `C:\rustREWRITE`, once the
   cleanup has finished and nothing (Remote Control, Ghidra, RPCS3) has it open. Then
   check it holds only `tools\`, `scripts\` and `reports\`, plus `src\` if you kept
   the code checkout.
4. **Rename the repo** on GitHub: open https://github.com/Deriv3d/SUGCtest, go to
   Settings, change the repository name to `UC2RewriteRust`, and click Rename.
5. **Point any clone on your PC at the new name.** In each folder where the repo is
   cloned (for example `C:\rustREWRITE\src\SUGCtest`, if it still exists), run:
   `git remote set-url origin https://github.com/Deriv3d/UC2RewriteRust.git`
   then `git fetch` to check it works. GitHub redirects the old name for now, but don't
   rely on it.
6. **A new Claude project** for the port. Connect GitHub and add https://github.com/Deriv3d/UC2RewriteRust to it,
   allow Remote Control for `C:\rustREWRITE` when asked, and keep the PC from sleeping
   during overnight runs.
7. **Fill in the prompt's three `<...>` placeholders** (dump path, game version, RPCS3
   folder) and paste everything below its line as the first message.

## 3. Later, for the iPhone build (milestone 7)

- **A Mac with Xcode.** iOS apps can only be built and signed on macOS. The new project
  will give you exact steps when it gets there.
- **An Apple account for sideloading.** A free account works, but the app must be
  re-signed from the Mac every 7 days. A paid Apple Developer account lasts a year.
- **A Bluetooth controller** paired to the iPhone (Xbox or PlayStation pads work with iOS).
- **Space on the iPhone** for the converted game data, roughly 20 GB (an estimate).

## 4. Play it on your phone in the meantime

You can stream the real game from your PC to your iPhone today, with no porting: run
Sunshine (https://github.com/LizardByte/Sunshine) on the PC with RPCS3, and Moonlight
(https://github.com/moonlight-stream/moonlight-ios) on the iPhone with your controller
paired to it. It needs the PC on and a good home Wi-Fi connection.
