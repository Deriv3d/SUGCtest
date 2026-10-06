# Claims

| ID | Claim | Status | Evidence / settling check |
| --- | --- | --- | --- |
| C-001 | The game is a PPU frontend (menus, museum, saves, input, filters) wrapping an embedded Genesis emulator running bundled ROM images. | hypothesis | Owner's description. Settle in Phase 0/1: disc file inventory, EBOOT strings and imports, RPCS3 boot log. |
| C-002 | Ghidra has a big-endian PowerPC64 language with 32-bit addressing suitable for PS3 PPU code. | confirmed (tooling) | `PowerPC:BE:64:64-32addr` is defined in Ghidra's `Processors/PowerPC/data/languages/ppc.ldefs` (master, checked 2026-10-06). Not yet tried on this game's ELF. |
| C-003 | Ghidra decompiles Cell-specific VMX load/store instructions (lvlx/lvrx family). | negative (per third party) | Ps3GhidraScripts README states these are unsupported and can break decompilation. To re-check on 12.1.3 against our ELF. |
| C-004 | A maintained Ghidra processor module for the Cell SPU exists. | unknown | Web and GitHub searches on 2026-10-06 found none. Not yet a negative: searches were not exhaustive. |
| C-005 | The game uses the SPU (directly or via SPURS). | unknown | Settle in Phase 0: embedded SPU ELF scan (e_machine 0x17) plus RPCS3 runtime log of SPU thread groups. |
