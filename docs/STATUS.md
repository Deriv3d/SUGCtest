# Status

Updated 2026-10-06. Current phase: **0 (Lab)**, partly done. Plan: [PHASE0.md](PHASE0.md).
Claims and evidence: [../knowledge/claims.md](../knowledge/claims.md).

## Works

- Repo scaffold, CI, pre-commit hook, `publish-check` (7 tests on synthetic data).
- Lab on the owner's PC (`C:\SUGC-LAB`): disc extracted and hashed (27 files), executable
  decrypted reproducibly with RPCS3 `--decrypt`, Java 21 + Ghidra 12.1.3 installed portably.
- ghidra-mcp headless server running with the analysed executable loaded. Ps3GhidraScripts
  builds for 12.1.3 and names all 250 imports. Decompilation looks healthy (claim C-003).
- RPCS3 boots the game with `--no-gui` and writes a usable log.

## Confirmed findings

- One PPU executable, no PRX modules on disc. Genesis and Master System/Game Gear ROMs
  live in one zlib-packed archive (C-001).
- The game uses the SPU: one embedded SPU program, SPURS, 5 SPU thread groups (C-005).

## Unknown

- What the SPU program does (audio mixing, filters, decompression, something else).
- The archive's full format, the non-ROM entries, and how ROMs reach the emulator.
- Which Ghidra SPU route to use (no maintained module found, C-004).

## Not done yet in Phase 0

- Capture scripts: RenderDoc frame, RPCS3 RSX capture, audio loopback, memory dump.
  RenderDoc is not installed yet.
- Verification: compare Ghidra's view of a few import stubs with RPCS3's PPU debugger;
  repeatability of the title-screen frame.

## Known tool quirks

- ghidra-mcp headless: the program has to be loaded through its API; the README's
  `list_functions` endpoint returned 404.
