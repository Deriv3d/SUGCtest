# Status

Updated 2026-10-06. Current phase: **0 (Lab)**. Plan: [PHASE0.md](PHASE0.md).

## Works

- Repo scaffold: Cargo workspace, CI workflow, pre-commit hook.
- `publish-check`: rejects banned extensions, PS3 formats by magic (SELF, PKG, PSF/SFO,
  NPD, ISO 9660, PPU/SPU ELF), Genesis/SMD/SMS ROM headers, binaries outside the
  allowlist, decompiler auto-names, key-like hex, secrets, absolute user paths; with
  `--game-dir` also whole-file and 4 KiB block matches against the lab extraction. 7 unit
  tests pass on synthetic data.

## Unknown

- Whether the game is structured as the owner describes (claim C-001).
- Ghidra 12.1.3 behavior on this game's ELF; whether Ps3GhidraScripts builds for 12.1.3.
- SPU usage (C-005) and Ghidra SPU support (C-004).
- Which RPCS3 debug and capture features the owner's build has.

## Blocked

- Lab work: needs the owner's file paths and a Remote Control session on their PC.
- Pushing the repo: no GitHub account is connected to the project yet; repo lives in the
  project files until one is.
