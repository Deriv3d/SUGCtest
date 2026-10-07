# Before starting the Uncharted 2 port

A checklist for the owner, written 2026-10-07 when the SUGC trial ended. Do these, then
paste [NEXT_GAME_PROMPT.md](NEXT_GAME_PROMPT.md) (below its line) into the new project.

## 1. Clear the trial's game data

The exact file list, with sizes, is confirmed with the owner in the trial's Phase 0
thread before anything is deleted. This is the shape of it.

**Delete (derived from the SUGC disc):**
- `C:\SUGC-LAB\extract\` and `C:\SUGC-LAB\extract-audio\`: the extracted disc and decoded audio.
- `C:\SUGC-LAB\elf\`: the decrypted executable and any SPU images.
- `C:\SUGC-LAB\ghidra\`: the Ghidra project holding the analysed executable.
- `C:\SUGC-LAB\captures\` (including `rsx-manual\`): frames, audio, memory dumps, logs.
- `C:\SUGC-LAB\golden\` and `C:\SUGC-LAB\inventory\`.
- `C:\SUGC-LAB\src\SUGCtest\target\`: built binaries. The checkout itself holds only repo
  code, and it can go too.
- In the RPCS3 folder, the game's PPU/SPU caches and any leftover RSX capture in
  `captures\`. RPCS3 rebuilds caches on the next boot.

**Your call:**
- The SUGC `.dec.iso` in your Downloads\SUGC folder: your own dump. Delete it only if
  you won't replay the trial.
- The lab's copy of the RPCS3 config: harmless, and the UC2 lab needs a fresh copy anyway.
- The public CPU test vectors downloaded into the lab for the 68000/Z80 checks: not game
  data, and not needed for Uncharted 2.

**Keep:**
- `C:\SUGC-LAB\tools\`: portable Java 21, Ghidra 12.1.3, ghidra-mcp, Ps3GhidraScripts,
  RenderDoc, Rust, MinGW-w64. These carry straight over.
- `C:\SUGC-LAB\scripts\`: the capture scripts (launch, frame, audio, memory). They aren't
  in the repo, so this is the only copy.
- `C:\SUGC-LAB\reports\`: our own-words lab reports, useful as examples.
- The SUGCtest repo on GitHub: nothing game-derived is in it, and the new port copies from it.

## 2. Create for Uncharted 2

1. **Your own Uncharted 2 dump.** Dump your own disc to a `.dec.iso`, and add it to RPCS3.
   Check that it boots and plays there. Decide whether to use the base game or a specific
   update, and write that version down. The prompt asks for it.
2. **A new lab folder**, e.g. `C:\UC2-LAB`, on a drive with plenty of free space (the
   extracted game, Ghidra project and captures will be far larger than SUGC's; 150 GB free
   is a safe start, an estimate). Move or copy `C:\SUGC-LAB\tools\` and
   `C:\SUGC-LAB\scripts\` into it. Once those are moved, `C:\SUGC-LAB` can be deleted.
3. **A new private GitHub repo**, e.g. `UC2-port`, created empty. The new project can't
   create it for you.
4. **A new Claude project** for the port. Connect GitHub and add the new repo to it,
   allow Remote Control for the new lab folder when asked, and keep the PC from sleeping
   during overnight runs.
5. **Fill in the prompt's five `<...>` placeholders** (dump path, game version, RPCS3
   folder, lab folder, repo URL) and paste everything below its line as the first
   message.
