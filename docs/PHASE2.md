# Phase 2: Sonic the Hedgehog end to end

Goal: play Sonic the Hedgehog (Genesis) natively on PC from the user's own disc. It's the
trial run for rewriting an RPCS3-era game in Rust. SUGC's PS3 code is a frontend around a Genesis emulator,
so "the game" here is our own Genesis implementation fed the ROM from the user's disc archive.

## Steps

1. **68000 CPU** (`crates/m68k`, game-agnostic). Interpreter written from the published
   programmer's reference.
2. **Z80 CPU** (`crates/z80`, game-agnostic), checked the same way.
3. **VDP**: planes A/B and window, sprites, scrolling (H/V), palette (CRAM), VSRAM, DMA (68k→VRAM,
   fill, copy), H/V interrupts, H/V counters.
4. **Sound**: YM2612 FM and SN76489 PSG, mixed to 48 kHz like the PS3 build (C-019).
5. **Machine and frontend**: memory map as built by the PS3 code (fpg.md, step 3), controller I/O
   with the owner's key map, window, audio output. The ROM is loaded from `flog_u.fpg` with
   `sugc-formats`.
6. **Per-game hooks**: what Sonic's break-slot callback at 0x1C5DC does (C-016), reproduced as a
   PC-address callback.
7. **Verification against RPCS3**: frame and audio comparisons at fixed points (savestate or
   frame-number triggered), plus the GDB memory dumps already used for C-013.

## Verification log

- **68000** (2026-10-06): run against the public SingleStepTests 680x0 v1 vectors. They are kept in the lab, not in
  this repo; run with `SUGC_M68K_TESTS=<dir> cargo test -p m68k --release -- --ignored`.
  - 1,000,060 single-instruction cases across 125 files.
  - **821,970 of 821,973** pass when address-error cases are excluded. The 3 failures are 2 cases
    whose expected register state looks corrupt (ASL.b #2 rewriting the whole register) and 1
    memory-source DIVU by zero.
  - Address-error exception frames (178,087 cases) aren't modelled exactly yet; 332 pass. A
    correctly running game never takes address errors, so this is deferred.
  - Behaviours set from the vectors rather than the manual:
    - ASR by more than the operand width leaves C and X clear.
    - DIVU/DIVS overflow leaves N and Z unchanged.
  - Timing is a per-access estimate for now. Cycle-exact counts come later if the VDP needs them.
- **Z80** (2026-10-06): run against the public SingleStepTests z80 v1 vectors (kept in the lab; run with
  `SUGC_Z80_TESTS=<dir> cargo test -p z80 --release -- --ignored`).
  - **1,000,000 of 1,000,000** cases pass, including undocumented flag bits, MEMPTR/WZ, the Q latch
    behind SCF/CCF, and the block-repeat flag quirks.
  - **T-state counts match in all 1,000,000.** That matters because the Genesis sound driver times
    its sample playback in Z80 cycles.
