# Status

Updated 2026-10-06. Current phase: **1 (Data)**, plan: [PHASE1.md](PHASE1.md). Phase 0 done; the RSX capture was made by hand but RPCS3 0.0.43 cannot replay it (C-010).
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
- `crates/sugc-formats`: FPG archive parser and writer (no unsafe; tests on synthetic archives).
- `crates/sugc-formats`: SPR container parser and writer; GCM texture descriptors, `TEXL` texture
  streams, and DXT1/3/5 + ARGB8 (linear and swizzled) decoding to RGBA.
- `crates/sugc-lab`: `roundtrip <file>` (FPG or SPR), `extract <archive> <outdir>` and
  `spr-textures <spr> <outdir>` (both refuse to write inside the repo).
- Round trip on the owner's disc, 2026-10-06: `ui.spr` **PASS** (4 streams), `global_binary.spr`
  **PASS** (2 streams), both byte-identical. All 234 UI textures decode, including the G8B8 font atlas.
- Round trip on the owner's disc, 2026-10-06: `flog_u.fpg` **PASS** (147 entries,
  byte-identical, 147/147 inflate), `flog_c.fpg` **PASS** (12 entries, byte-identical,
  12/12 inflate).

## Confirmed findings

- One PPU executable; ROMs in one zlib-packed archive (C-001).
- SPU: one SPURS taskset task owned by the game; FMV playback uses the system video
  library's own SPURS instance (C-005). Port impact: one small SPU task to reimplement,
  and an ordinary video decoder in place of the system one.
- Frames are not reproducible by wall-clock wait (C-009).
- All 27 disc files classified: [../knowledge/formats/INVENTORY.md](../knowledge/formats/INVENTORY.md).
- FPG archive format and the archive-to-emulator path:
  [../knowledge/formats/fpg.md](../knowledge/formats/fpg.md) (C-011 to C-014).
- 38 of 40 bundled Genesis ROMs pass their internal header checksum (C-015).
- `flog_c.fpg`'s 9 binaries are localized string tables, not fonts (C-018).
- Music: one MSF file, PlayStation ADPCM; parser round-trips and decodes (C-019).
- ROM in RPCS3 guest memory is byte-identical to our extracted image (C-013). The core's only
  change is a hook marker in its instruction-fetch copy (break slots, C-016; 39 of 40 games).

## Unknown

- What the game's SPU task does, and when it runs.
- Names for 78 of 147 `flog_u` entries and 9 of 12 `flog_c` entries; 8 unidentified binaries.
- Font glyph metrics; names of the 9 string tables.
- `streams/*.spr` scene-graph record layout (compression layer and stream roles done: [../knowledge/formats/spr.md](../knowledge/formats/spr.md)), MP4 codec parameters.
- MSF decode needs an owner listening check (C-019).
- Where the archive's little-endian table is byte-swapped after load.
- What the per-game break-slot callbacks do (C-016).
- Ghidra SPU route (C-004).

## Blocked on the owner

- Nothing at the moment. (RSX captures must be made by hand from RPCS3's menu, C-010.)

## Known tool quirks

- ghidra-mcp headless: load the program through its API; README `list_functions` 404s.
- RPCS3 `--no-gui`: closing the window does not stop it; use the GDB server's kill.
- RPCS3 GDB server: one connection per boot; breakpoints need the PPU interpreter;
  "start paused" is ignored for CLI boots, so the entry point can't be broken on.
- The owner's machine blocks synthetic keystrokes and closing processes it didn't start.
