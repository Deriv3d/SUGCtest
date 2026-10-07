# Before starting the Uncharted 2 port

A checklist for the owner, written 2026-10-07 when the SUGC trial ended. Do these, then
paste [NEXT_GAME_PROMPT.md](NEXT_GAME_PROMPT.md) (below its line) into the new project.

## 1. Trial cleanup (done 2026-10-07, in the trial's Phase 0 thread)

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
2. **Disk space** on the drive holding `C:\rustREWRITE`: the extracted game, Ghidra
   project and captures will be far larger than SUGC's. 150 GB free is a safe start (an
   estimate).
3. **Rename the repo** to `UC2RewriteRust` in its GitHub settings, if not done yet.
4. **A new Claude project** for the port. Connect GitHub and add https://github.com/Deriv3d/UC2RewriteRust to it,
   allow Remote Control for `C:\rustREWRITE` when asked, and keep the PC from sleeping
   during overnight runs.
5. **Fill in the prompt's three `<...>` placeholders** (dump path, game version, RPCS3
   folder) and paste everything below its line as the first message.
