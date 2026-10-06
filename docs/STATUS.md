# Status

Updated 2026-10-06. Current phase: **0 (Lab)**, all but one item done. Plan: [PHASE0.md](PHASE0.md).
Claims and evidence: [../knowledge/claims.md](../knowledge/claims.md).

## Works

- Repo scaffold, CI, pre-commit hook, `publish-check` (7 tests on synthetic data).
- Lab on the owner's PC (`C:\SUGC-LAB`): disc extracted and hashed, executable decrypted
  reproducibly with RPCS3 `--decrypt`, Java 21, Ghidra 12.1.3 and RenderDoc 1.46 installed
  portably. Lab runs use a separate RPCS3 config; the owner's config is untouched.
- ghidra-mcp headless server with the analysed executable; all 250 imports named.
- Lab scripts (on the owner's PC, `C:\SUGC-LAB\scripts`), each capture with a manifest:
  - launch: `--no-gui` boot, log collection, clean shutdown via RPCS3's GDB server.
  - frame: RenderDoc in-app API capture, exported to PNG (1280x720).
  - audio: WASAPI loopback, 48 kHz stereo, matches RPCS3's reported output.
  - memory: own GDB-protocol client dumps program segments and heap.
- Ghidra agrees with RPCS3 at runtime (C-008).

## Confirmed findings

- One PPU executable; ROMs in one zlib-packed archive (C-001).
- SPU: one SPURS taskset task owned by the game; FMV playback uses the system video
  library's own SPURS instance (C-005). Port impact: one small SPU task to reimplement,
  and an ordinary video decoder in place of the system one.
- Frames are not reproducible by wall-clock wait (C-009).

## Unknown

- What the game's SPU task does, and when it runs.
- The archive's full format, the non-ROM entries, and how ROMs reach the emulator.
- Ghidra SPU route (C-004).

## Blocked on the owner

- RSX capture must be made by hand from RPCS3's menu (C-010).

## Known tool quirks

- ghidra-mcp headless: load the program through its API; README `list_functions` 404s.
- RPCS3 `--no-gui`: closing the window does not stop it; use the GDB server's kill.
- RPCS3 GDB server: one connection per boot; breakpoints need the PPU interpreter;
  "start paused" is ignored for CLI boots, so the entry point can't be broken on.
- The owner's machine blocks synthetic keystrokes and closing processes it didn't start.
