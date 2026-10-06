# sugc-port

A native PC port of Sonic's Ultimate Genesis Collection (PS3), written in Rust.

This repository contains **only our own code and documentation**. It contains no game
assets, no keys, no executable from the disc, no extracted or decompiled game code, no
copied middleware, and no Genesis / Mega Drive ROM images.

To use it you must dump **your own** disc of the game. At build or run time, a pipeline
that runs on your machine reads your own decrypted disc image and extracts what it needs.
We do not link to, and will not help anyone obtain, disc images or ROMs.

Single-player and offline only. Nothing here bypasses DRM, anti-cheat, or online services.

## Layout

| Path | What |
| --- | --- |
| `crates/` | Our Rust crates (formats, runtime, Genesis core, renderer, audio). Added phase by phase. |
| `tools/publish-check/` | CI and pre-commit lint that refuses game-derived files and anything that looks like a ROM, SELF/ELF, ISO, SFO or key. |
| `lab-scripts/` | Scripts for the local lab (RPCS3, Ghidra, RenderDoc captures). They read and write the lab folder, never the repo. |
| `docs/STATUS.md` | What works, what is unknown, what is blocked. |
| `docs/PHASE0.md` | The current phase plan, definition of done, and verification. |
| `knowledge/` | Field notes and evidence-backed claims. No game data, ROM data or decompiled code. |

## Contributing rule

Run `git config core.hooksPath lab-scripts` once. The pre-commit hook runs
`publish-check --staged`; CI runs it on the whole tree. Synthetic test data only: Genesis
test ROMs are generated from our own source at build time and are never committed.
