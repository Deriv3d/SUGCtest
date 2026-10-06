# Phase 0: Lab

Goal: a reproducible lab on the owner's Windows PC that turns their own `.dec.iso` into
analyzable files, opens the PPU executable in Ghidra 12.1.3 with ghidra-mcp, and captures
ground truth from RPCS3 on demand, while the repository holds only our own code.

## Where things live

- **Repo** (`sugc-port`): our code and docs only. Guarded by `publish-check` in CI and a
  pre-commit hook.
- **Lab** (owner's PC, outside the repo, e.g. `D:\sugc-lab\`): everything game-derived.
  Game-derived data never leaves the owner's machine; analysis runs there through a Remote
  Control session. Notes that come back to the repo describe findings in our own words.

```
D:\sugc-lab\
  extract\      files extracted from the .dec.iso (read-only after extraction)
  elf\          EBOOT decrypted to ELF by RPCS3, plus any PRX/SPU images found
  ghidra\       Ghidra 12.1.3 project(s)
  captures\     RenderDoc .rdc, RPCS3 RSX captures, audio, memory dumps, logs
  golden\       Phase 2 golden data (never committed)
  inventory\    file lists, sizes, SHA-256 hashes of extract\ and elf\
```

## Tooling checks, in order, each reported honestly

1. **RPCS3**: version, Vulkan renderer, game boots to the main menu from the `.dec.iso`.
   Find out (do not assume) which of these the installed build offers: debugger and memory
   viewer, memory dump, GDB server, RSX capture, SPU/PPU logging levels, command-line boot
   (`rpcs3.exe --no-gui <path>`), "Decrypt PS3 Binaries" utility.
2. **Extraction**: extract the ISO file tree to `extract\` (7-Zip or RPCS3). Record an
   inventory with hashes. Decrypt `EBOOT.BIN` (a SELF) to ELF with RPCS3's own utility,
   which uses the keys bundled in RPCS3 on the owner's machine; nothing key-related is
   copied anywhere else.
3. **Ghidra 12.1.3 + ghidra-mcp** (headless server where possible; Java 21, Python 3.10+).
   Load the ELF as `PowerPC:BE:64:64-32addr`. Try clienthax/Ps3GhidraScripts for NID import
   naming and syscalls (needs a build for 12.1.3, and its documented `r2` cspec tweak).
   Measure: imports named, entry point found, functions discovered, decompile success rate,
   count of instructions Ghidra cannot decode (expected: Cell VMX lvlx/lvrx family).
4. **SPU**: scan the ELF and every extracted file for embedded SPU ELFs (big-endian ELF,
   `e_machine` 0x17) and SPURS markers; at runtime, check RPCS3's log for SPU thread groups
   or SPURS. If SPU code exists, choose a disassembly route with evidence: a Ghidra SPU
   module if a working one turns up, RPCS3's SPU disassembler, or a small SLEIGH module we
   write from IBM's public SPU ISA documents.
5. **Captures**: scripts (PowerShell, wrapped later by a Rust `lab` CLI) that:
   launch RPCS3 on the game; take a RenderDoc frame capture (RPCS3 on Vulkan launched under
   RenderDoc); take an RPCS3 RSX capture; record audio via WASAPI loopback; dump guest
   memory; collect `RPCS3.log`. Every capture is written to `captures\` with a manifest
   (RPCS3 version, settings, timestamp, what the owner did).

## Definition of done

- Repo scaffold exists with `publish-check` passing in CI and refusing a planted synthetic
  ROM header, SELF, ISO, and decompiler-named source (unit tests).
- Lab inventory exists; decrypted ELF loads in Ghidra 12.1.3 through ghidra-mcp headless;
  `docs/STATUS.md` reports the measured numbers from step 3, good or bad.
- SPU usage is `confirmed` or `negative` in `knowledge/claims.md`, with evidence. `unknown`
  is acceptable only with the reason it could not be settled.
- Claim C-001 (compilation structure) is updated from evidence: what is confirmed, what is
  still unknown.
- Each capture script has run once and produced its artifact.

## Verification against RPCS3

- **Extraction**: decrypting the EBOOT twice gives the same SHA-256; the entry point and
  imported module list in Ghidra match what RPCS3's log reports when it loads the game.
- **Ghidra**: for a handful of import stubs and one known syscall wrapper, Ghidra's view
  agrees with the call targets RPCS3's PPU debugger shows when broken at that address.
- **Captures**: the RenderDoc framebuffer of the title screen matches an RPCS3 screenshot of
  the same frame; two runs with the same scripted wait reach the same screen (or the
  nondeterminism is measured and written down). Audio capture length and sample rate match
  what RPCS3 reports for its audio backend.
- The owner confirms by eye that captured frames show what the game showed.
