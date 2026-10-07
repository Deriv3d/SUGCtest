# Lessons log

A running record of what worked and what didn't on this port, kept because SUGC is a trial
run for a much bigger PS3 port (Uncharted 2). The compiled, forward-looking version is
[NEXT_GAME_PROMPT.md](NEXT_GAME_PROMPT.md); this file is the raw log it is built from.

Rules for this file: our own words only, no game data, bytes, dumps or decompiled code.
Newest entries go at the bottom of each section. Each entry says what happened, what we
learned, and how to apply it. Tag each one **[carries]** if it should apply to any PS3
port, **[SUGC]** if it is specific to this game, or **[check]** if we expect it to differ
on the next game and it must be re-verified there.

## Scope

- **2026-10-06:** Ez narrowed the trial to **Sonic 1 end to end**. Its purpose is to test
  whether rewriting an RPCS3 game natively in Rust works at all, not to port the whole
  collection. Uncharted 2 will start in a separate project with none of this context, so
  NEXT_GAME_PROMPT.md must stand on its own.

- **2026-10-07: the trial ended.** By then: every SUGC disc file was classified; the ROM
  archive, texture container, string tables and music had round-tripping Rust parsers; a
  desktop asset viewer decoded all 394 assets; the 68000 and Z80 CPUs passed public test
  vectors; and a native prototype booted Sonic 1 from the owner's own disc to the title
  screen and demo (no sound yet). Ez judged that enough proof that the approach works.
  The Uncharted 2 prompt was finalised (NEXT_GAME_PROMPT.md, setup in NEXT_GAME_SETUP.md).
- **2026-10-07: Uncharted 2 approach changed.** Ez chose a feature-by-feature recreation
  in hand-written Rust on Bevy, loading the game's real assets from the owner's disc, with
  "feels basically the same" as the bar and a sideloaded iPhone app with a controller as
  the end goal. Static recompilation (ps3recomp) was considered and not chosen.

## Worked

### Lab and workflow

- **Split cloud repo from on-device lab** (2026-10-06) [carries]. The repo holds only our
  code and docs; everything game-derived lives in `C:\SUGC-LAB` on the owner's PC, and a
  Remote Control session does the analysis there. Findings come back as counts, hashes,
  pass/fail and prose. This kept the legal line clean from day one with no clean-up later.
- **publish-check in CI and as a pre-commit hook from the first commit** [carries]. It
  refuses ISO/SELF/PKG/SFO, PPU/SPU ELFs, ROM headers, unknown binaries, decompiler-style
  names and key-like hex. Its tests use synthetic files only. Cheap to build first, and
  every later step can trust that the repo is clean.
- **Portable tool installs under the lab folder** [carries]. Java 21, Ghidra 12.1.3,
  RenderDoc and later Rust went under `C:\SUGC-LAB\tools` with no system-wide installs and
  no PATH edits. Nothing to undo on the owner's machine.
- **Separate RPCS3 config for lab runs** [carries]. The owner's own RPCS3 settings were
  never changed. Scripted boots use a copy.
- **Claims ledger with confirmed / negative / unknown / hypothesis** (from REA) [carries].
  Keeping "unknown" apart from "negative" stopped us from reporting "no SPU Ghidra module
  exists" when we had only failed to find one (C-004).
- **Long, self-contained briefs for the on-device session** [carries]. A brief that lists
  hard rules, inputs, numbered steps, where to write the report, and "on failure try one
  alternative, record why, move on" ran unattended and came back with a clean per-step
  report. Night 1 finished all eight steps in under an hour.
- **Batch the asks to the owner up front** [carries]. One message listing every path and
  account needed (disc image, RPCS3 folder, lab folder, tool status, GitHub repo) got
  everything answered in one reply.

### Tooling results
- **Public test vectors as a lab-only checker** (2026-10-06) [carries]. Ez agreed to use
  public CPU instruction test suites, kept in the lab and never committed. The 68000 passed
  821,970 of 821,973 and the Z80 all 1,000,000, cycle counts included. That caught CPU bugs
  long before a game ran.
- **A game-agnostic viewer crate plus a per-game disc source** [carries]. The egui asset
  browser is reusable as is; only the source that reads the disc is game-specific.
- **Owner checks by eye and ear** [carries]. Ez confirmed decoded textures and music, which
  automated checks couldn't settle.


- **RPCS3 CLI** [carries, version-dependent]. RPCS3 0.0.43 `--decrypt` turns the disc
  executable into an ELF reproducibly (same SHA-256 twice), and `--no-gui` boots the game.
  No GUI clicks were needed for decryption.
- **Ghidra 12.1.3 on PS3 PPU code** [check]. Language `PowerPC:BE:64:64-32addr` plus
  clienthax/Ps3GhidraScripts named all 250 imports from NIDs, found the entry point, and
  200 of 200 sampled functions decompiled without errors. Only one bad instruction, and it
  looked like data. The documented `r2` cspec tweak made no measurable difference. The
  warned-about Cell VMX `lvlx/lvrx` decode failures did not show up in this game.
- **Ghidra agrees with RPCS3 at runtime** [carries]. Entry descriptor and 5 import stubs
  matched RPCS3 memory and log; live GDB breakpoints hit at exactly Ghidra's addresses. The
  code segment in guest memory was byte-identical to the decrypted ELF, so static and
  dynamic addresses can be used interchangeably (no relocation for the main executable).
- **ghidra-mcp headless** [carries]. Works with the analysed program, but the program must
  be loaded through its API; the README's `list_functions` example returned 404.
- **Captures** [carries]. RenderDoc in-app API capture of RPCS3 on Vulkan, exported to PNG
  at 1280x720; WASAPI loopback audio at 48 kHz stereo, matching RPCS3's reported backend;
  our own minimal GDB-protocol client dumping program segments and heap. Every capture
  writes a manifest (RPCS3 version, settings, script, waits).
- **Clean RPCS3 shutdown via the GDB server's kill** [carries]. Closing the window of a
  `--no-gui` boot does not stop RPCS3.
- **SPU audit by structure, not disassembly** [carries]. Scanning the executable and every
  disc file for embedded big-endian ELFs with SPU machine type, plus SPURS import names,
  plus RPCS3's log of SPU thread groups, was enough to classify SPU usage without any SPU
  disassembler: one game-owned SPURS taskset task, and a separate SPURS instance owned by
  the system video library for FMVs.

## Didn't work

- **Capture script closed the owner's own RPCS3** (2026-10-06, ~12:26 owner time)
  [carries]. The first capture script sent "close" to every running RPCS3, including one
  the owner was playing. Fix: scripts only close processes they launched themselves (track
  the PID), and refuse to launch while any RPCS3 the owner started is open. Ask the owner
  before any run that touches a program they might be using.
- **Can't close the owner's programs from the lab session** [carries]. The owner's machine
  blocks closing processes the session didn't start, and blocks synthetic keystrokes. Any
  step that needs that is a hand step for the owner, written as exact clicks.
- **RSX capture can't be scripted on RPCS3 0.0.43** (C-010) [check]. No CLI or config
  path; only the menu or a hotkey, and synthetic keys are blocked. It needed the owner to
  click Utilities > Create RSX Capture by hand.
- **Wall-clock waits don't give reproducible frames** (C-009) [carries]. Two runs with the
  same 40 s wait captured frames 2164 and 2171 because the title screen animates. Golden
  captures must key on an exact frame number or a savestate, never on time.
- **RPCS3 GDB server limits** [carries, version-dependent]. One connection per boot;
  breakpoints only work with the PPU interpreter (not the recompiler); "start paused" is
  ignored for CLI boots, so you cannot break on the entry point.
- **No maintained Ghidra SPU module found** (C-004, unknown) [carries]. Searches found
  none. For SUGC it didn't matter because the SPU work is tiny; it will matter a lot more
  on a game that leans on the SPUs.
- **Device link from a thread failed once** ("worker busy, gave up") and the PC session
  went offline between runs [carries]. Messages to the PC session queue and only run when
  it is online and awake. A "continue" sent at night reached the PC the next morning, so a
  whole night was lost. Before the owner leaves the machine overnight, confirm the session
  is online and has actually picked up the brief.
- **Timing questions** [carries]. The owner asked when to check back; we can't estimate run
  length. Better: promise a notification on completion or on any hand step, and say what
  would stop the run (PC sleeping, app closed).
- **Docs drifted from reality** [carries]. PHASE0.md still suggests `D:\sugc-lab` while
  the lab is at `C:\SUGC-LAB`. Write real paths into the plan as soon as the owner gives
  them.
- **CI was red on main from the first commit and nobody noticed** (found 2026-10-06)
  [carries]. The build job runs `cargo fmt --check` and `clippy -D warnings`, which the
  scaffold never ran locally, and four pushes to main went by without anyone reading CI.
  Run the same fmt/clippy/test commands as CI before every push, and check CI after it.
  CI also uses the latest stable Rust, which brought a new clippy lint the local
  toolchain didn't know; test with the same toolchain as CI or pin one in the repo.
- **Capture scripts lived only on the owner's PC** [carries]. They were never committed,
  so the lab's `scripts\` folder is the only copy. Commit lab scripts (they contain no
  game data) to the repo from the start.
- **RSX captures made with RPCS3 0.0.43 couldn't be replayed** by that build (C-010)
  [check].
- **Shared project folder is noexec** [carries]. Builds from `/mnt/project-files` fail
  unless `CARGO_TARGET_DIR` points outside it.

## Open questions this port should still answer

These are the things the next game needs most and SUGC hasn't tested yet:

- Static recompilation of PPU code to native (Phase 3) has not been attempted. Everything
  about it (tooling, how imports are stubbed, how big the runtime layer gets) is untested.
- Rewriting an SPU task on the CPU and diffing it against captured SPU inputs/outputs
  (Phase 4a) has not been attempted.
- RSX command-stream capture replay is still unchecked (the manual capture exists).
- How much of the RSX usage maps cleanly to wgpu/Vulkan is unknown.
