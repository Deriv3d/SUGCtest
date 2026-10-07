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

The SUGCtest GitHub repo contains no game data. Keeping it costs nothing and lets the new
project copy publish-check, CI and the viewer instead of rewriting them, but the prompt
works without it.

## 2. Create for Uncharted 2

1. **Your own Uncharted 2 dump.** Dump your own disc to a `.dec.iso`, and add it to RPCS3.
   Check that it boots and plays there. Decide whether to use the base game or a specific
   update, and write that version down. The prompt asks for it.
2. **Disk space** on the drive holding `C:\rustREWRITE`: the extracted game, Ghidra
   project and captures will be far larger than SUGC's. 150 GB free is a safe start (an
   estimate).
3. **A new private GitHub repo**, e.g. `UC2-port`, created empty. The new project can't
   create it for you.
4. **A new Claude project** for the port. Connect GitHub and add the new repo to it,
   allow Remote Control for `C:\rustREWRITE` when asked, and keep the PC from sleeping
   during overnight runs.
5. **Fill in the prompt's four `<...>` placeholders** (dump path, game version, RPCS3
   folder, repo URL) and paste everything below its line as the first message.
