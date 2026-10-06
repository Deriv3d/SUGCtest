# Claims

| ID | Claim | Status | Evidence / settling check |
| --- | --- | --- | --- |
| C-001 | The game is a PPU frontend wrapping one embedded Genesis emulator that runs bundled ROM images. | confirmed (structure), details unknown | Lab night 1: the disc ships one executable and no PRX/SELF; one zlib-packed archive in USRDIR holds 147 entries, of which 40 carry a Mega Drive header and 2 a Master System/Game Gear header, at standard cartridge sizes. Executable strings name the CPUs and the emulator. Unknown: per-game patches, how the frontend hands ROMs to the core, what the other 105 entries are. |
| C-002 | Ghidra has a big-endian PowerPC64 language with 32-bit addressing suitable for PS3 PPU code. | confirmed (tooling) | `PowerPC:BE:64:64-32addr` is defined in Ghidra's `Processors/PowerPC/data/languages/ppc.ldefs` (master, checked 2026-10-06). Not yet tried on this game's ELF. |
| C-003 | Ghidra fails on Cell-specific VMX load/store instructions in this game. | negative | Lab night 1, Ghidra 12.1.3: 1 bad instruction in the whole executable, and it looks like data. 200 of a random 200 functions decompile without errors (16 with warnings, mostly unrecovered jump tables). |
| C-004 | A maintained Ghidra processor module for the Cell SPU exists. | unknown | Web and GitHub searches on 2026-10-06 found none. Not yet a negative: searches were not exhaustive. |
| C-005 | The game uses the SPU. | confirmed | One SPU ELF is embedded in the executable (none in other files). cellSpurs is imported (20 functions). The RPCS3 boot log shows 5 SPU thread groups and SPURS starting. Purpose of the SPU code: unknown. |
| C-006 | Ghidra 12.1.3 with Ps3GhidraScripts names every PPU import. | confirmed | Lab night 1: 250 of 250 imports named from NIDs across 17 Sony modules; entry point found; 5,956 functions. The r2 cspec tweak made no measurable difference. |
| C-007 | RPCS3 can decrypt the executable and boot the game without its GUI. | confirmed | RPCS3 0.0.43-20227: `--decrypt` gives the same SHA-256 on two runs; `--no-gui` boot ran about 90 s and appeared to render. |
