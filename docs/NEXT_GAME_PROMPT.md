# Prompt for the next port: Uncharted 2 (PS3)

Final version, written 2026-10-07 when the SUGC trial ended. Before using it, do the
preparation in [NEXT_GAME_SETUP.md](NEXT_GAME_SETUP.md), then fill in the five
`<...>` placeholders in the "Inputs" section.

Marks used below: **(verified on SUGC)** means the trial saw it work or fail.
**(inference)** means it comes from general knowledge of Uncharted 2 and the PS3 and has
not been checked on this game. Phase 0 settles every inference before anything is built
on it.

Copy everything below the line as the opening message of the new project.

---

You are my lead engineer on a long-running project: a native rewrite of **Uncharted 2:
Among Thieves (PS3)** in Rust, built from game files I own. The first target is Windows
PC. I may later move it onto the **Bevy** engine and/or ship a **mobile** build (Android
first, then iOS). Design for that from day one, but don't build those yet.

This follows a trial run on Sonic's Ultimate Genesis Collection (SUGC), a PS3 collection
of Sega Genesis games. The trial asked whether rewriting an RPCS3-era game natively in Rust
works. It took Sonic 1 from disc to a playable native prototype, and I'm satisfied it does.
Everything you need from the trial is written into this prompt. Its repo,
https://github.com/Deriv3d/SUGCtest, is reference you can copy from (list below). Don't
assume its game findings apply here.

## Inputs (fill in before sending)

- My own decrypted disc dump: `<path to UC2 .dec.iso>`. I dumped it myself from my own disc.
- Game version: `<base disc, or the exact update installed in RPCS3>`.
- RPCS3: `<RPCS3 folder>`, which already runs the game. RPCS3 running my dump is the
  ground-truth oracle.
- Lab folder: `<e.g. C:\UC2-LAB>`, empty except for `tools\` and `scripts\` copied over
  from the trial. Portable Java 21, Ghidra 12.1.3, RenderDoc, Rust and a MinGW-w64
  toolchain are already in `tools\`. Check them before installing anything.
- GitHub repo for this port: `<https://github.com/<me>/<repo>>`, private and empty, already
  connected to this project.

## Ground rules (verified on SUGC; keep as is)

- The repo must never contain game assets, keys, the executable, extracted or decompiled
  game code, copied middleware, or any data from the disc. Users supply their own disc,
  and a local pipeline extracts what is needed at build or run time. Tell users to dump
  their own disc; never link to or suggest downloading game files.
- From the first commit, add `publish-check` (copy `tools/publish-check` from SUGCtest)
  running in CI and as a pre-commit hook. It refuses ISO/SELF/PKG/SFO files, PPU and SPU
  ELFs, unknown binaries outside an allowlist, Ghidra/IDA auto-names (`FUN_xxxxxx` and the
  like), decompiler headers, key-like hex, and any file or aligned 4 KiB block whose hash
  matches a file in the lab extraction (`--game-dir`). Its tests use synthetic files only.
  Extend it for this game's container formats once you know them.
- Run CI's exact commands (`cargo fmt --check`, `clippy -D warnings`, tests, publish-check)
  locally before every push, on the same Rust version CI uses, and read CI after pushing.
  On SUGC, CI was red on main for four commits before anyone noticed.
- Single-player and offline only. Uncharted 2's multiplayer and any online service are out
  of scope. Don't bypass DRM or anti-cheat.
- Our code is clean-room: written from public documentation, our own analysis notes and
  our own tests. Never paste decompiled output into the repo; describe behaviour in our own
  words and cite Ghidra addresses.
- Public test vectors (for example CPU instruction test suites) may be used as checkers if
  they stay in the lab and never enter the repo. On SUGC this caught CPU bugs early.

## The trial's tooling, and what to reuse

Reuse from https://github.com/Deriv3d/SUGCtest:
- `tools/publish-check/` and `lab-scripts/pre-commit` (the hook).
- `.github/workflows/ci.yml` (publish-check job, then fmt, clippy and tests).
- `knowledge/README.md` (claims ledger and field-note rules) and `docs/LESSONS.md`.
- `crates/asset-browser/`: a game-agnostic egui asset viewer (images, audio, string tables)
  that each game plugs a disc source into.
- `crates/sugc-lab/` as a pattern for a lab CLI (`roundtrip`, `extract`) that refuses to
  write inside the repo.
- The capture scripts are not in the repo. They are in the lab's `scripts\` folder on my PC
  (PowerShell): launch, frame, audio and memory. Each writes a `manifest.json` with the
  RPCS3 version, settings, script and waits.

External tools, with links:
- RPCS3: https://github.com/RPCS3/rpcs3
- Ghidra 12.1.3: https://github.com/NationalSecurityAgency/ghidra/releases
- ghidra-mcp (primary analysis interface, headless): https://github.com/bethington/ghidra-mcp
- Ps3GhidraScripts (NID import naming, syscalls): https://github.com/clienthax/Ps3GhidraScripts
- RenderDoc: https://github.com/baldurk/renderdoc
- Temurin JDK 21: https://github.com/adoptium/temurin21-binaries
- Reference only, don't run: REA (claims-with-evidence idea, pins Ghidra 12.1.2)
  https://github.com/morluto/rea; universal-modder (publish-check and field-note ideas)
  https://github.com/rehan-remade/universal-modder
- Rendering and engine: wgpu https://github.com/gfx-rs/wgpu, Bevy
  https://github.com/bevyengine/bevy
- Ignore unless you justify them first: ILSpy, Cpp2IL (they target .NET/Unity, not native
  PS3 code) and IDA MCP (paid IDA required).

## What this game is (verify in Phase 0, do not assume)

All of this is **inference** until you confirm it:
- Unlike SUGC, there is no embedded emulator to swap out. SUGC was a thin PS3 frontend
  around a Genesis emulator, so porting it mostly meant writing our own Genesis emulator.
  Uncharted 2 is a full PS3 engine, so the port is the whole game's code.
- It leans heavily on the SPUs (animation, physics, culling, audio and post-processing
  are commonly cited). SUGC had one small SPU task. Expect many SPU programs and jobs here,
  and expect the SPU work to be the largest part of the port.
- Gameplay likely runs partly through Naughty Dog's own compiled script data, not only
  C++. Find the script format and how the runtime executes it before planning PPU work.
- Expect large streaming archives and many asset formats, not SUGC's 27 files.
- Expect extra PRX code modules or overlays besides the main executable, each needing its
  own decryption and analysis.
- Game updates may replace the executable. Work against the one version named above so
  addresses stay stable.

## Architecture, given the Bevy and mobile goals

Recommended shape (my default; give me evidence before changing it):
- **Static recompilation, not hand rewriting, for the bulk of the PPU code.** Translate PPU
  functions to Rust (or to native code through Rust) so the game's own engine logic runs
  natively, with OS and library calls routed to our runtime. Hand-rewrite in idiomatic
  Rust only what must change: platform layer, renderer, audio, input, file I/O, and each
  SPU job. A from-scratch reimplementation of the gameplay on Bevy would be a new game,
  not a port; only consider it per subsystem, later, with evidence.
- **Layering**, so Bevy and mobile stay possible:
  - `core`: recompiled game code plus our runtime. Pure Rust, no windowing, no global
    state the host can't own, deterministic stepping from a savestate.
  - `platform` trait: file access to the user's extracted data, timing, input, audio out,
    and a GPU device. The first host is a plain winit + wgpu desktop app.
  - `render`: the RSX command stream translated to wgpu. wgpu covers Vulkan, Metal, DX12
    and GLES, which is also what Bevy renders through and what phones need. Shaders go
    through our own RSX shader translator to WGSL or SPIR-V.
  - Later hosts: a Bevy app that drives `core` as a system and draws `render`'s output as a
    texture or custom render node, and an Android/iOS shell. Bevy's ECS won't map onto the
    game's own object model, so Bevy hosts the game rather than re-expressing it.
- **Endianness**: the guest is big-endian and every target host is little-endian. Keep
  byte-swapping inside small, tested accessors, and isolate any `unsafe` behind small
  tested interfaces. Tell me when you add `unsafe`.
- **Mobile constraints**, all inference: ARM64 targets mean recompiled code must not
  assume x86. Memory budget matters on phones, the user's game data must be extracted on a
  PC and copied onto the device, touch controls need designing, and store distribution of
  an app that needs a user-supplied disc dump is uncertain. Treat mobile as a later
  vertical slice, not a phase-0 requirement.

## Lab setup (verified on SUGC)

- Two places: the repo in the cloud (our code and docs only) and the lab folder on my PC,
  where everything game-derived lives. Analysis runs there through a Remote Control
  session. Reports come back as counts, hashes, pass/fail and your own words.
- Install tools portably inside the lab. No system-wide installs or PATH edits. Building
  the egui viewer on Windows with the GNU Rust toolchain needed a full MinGW-w64 on PATH;
  a portable WinLibs build worked.
- Lab runs use a separate copy of the RPCS3 config, never mine.
- **Never close, focus, or send input to an RPCS3 I started.** Scripts track the PID they
  launched and close only that one, and they refuse to start while my own RPCS3 is open.
  On SUGC a script closed my running game. My machine also blocks synthetic keystrokes and
  closing programs the session didn't start, so those are hand steps for me, given as
  exact clicks.
- Before I leave the PC overnight, confirm the PC session is online and has picked up the
  brief. On SUGC a night was lost because a message queued while the PC was offline.
- Write each on-device brief as one self-contained message: hard rules, inputs, numbered
  steps, where the report goes, and "on failure, try one alternative, record why, move on".
- If you build from the cloud project's shared folder, set `CARGO_TARGET_DIR` outside it,
  because that folder is noexec.

## Tooling: what to expect

- **RPCS3 CLI** (verified on SUGC with 0.0.43): `--decrypt` gives a reproducible ELF and
  `--no-gui` boots. Closing a `--no-gui` window does not stop it; use the GDB server's
  kill. The GDB server allows one connection per boot, breakpoints only work under the PPU
  interpreter, and "start paused" is ignored for CLI boots. RSX capture had no CLI path,
  needed a manual menu click, and 0.0.43 could not replay the capture. Re-check all of this
  on the current RPCS3.
- **Ghidra + ghidra-mcp** (verified on SUGC with 12.1.3): language `PowerPC:BE:64:64-32addr`
  plus Ps3GhidraScripts named every import, and Ghidra's addresses matched RPCS3 at
  runtime (no relocation of the main executable). Load programs through ghidra-mcp's API;
  its README's `list_functions` example returned 404. **Inference:** a much larger,
  heavily optimised executable will analyse slower and will likely hit the Cell VMX
  `lvlx/lvrx` decode gaps that SUGC never did. Measure the bad-instruction count and the
  decompile rate on a random sample early.
- **SPU tooling is the main gap.** The trial found no maintained Ghidra SPU module
  (unknown, not proven absent). In Phase 0 settle the route with evidence: a working Ghidra
  module, RPCS3's SPU disassembler, or a SLEIGH module we write from IBM's public SPU ISA
  documents. Start by counting SPU programs (embedded big-endian ELFs with the SPU machine
  type, plus SPURS job and taskset images) and grouping them by role. On SUGC that worked
  without any disassembly.
- **Captures** (verified on SUGC): RenderDoc in-app capture of RPCS3 on Vulkan to PNG,
  WASAPI loopback audio at 48 kHz stereo, and our own GDB-protocol memory dumper.
- **Reference frames must key on an exact frame number or a savestate**, never a
  wall-clock wait (verified on SUGC: same 40 s wait, different frames). **Inference:**
  this matters more for a 3D game with streaming, physics and particles. Get deterministic
  replay (savestate plus recorded input) working before collecting golden data.

## Strategy: RPCS3 is the oracle

Work in phases, and finish each phase's verification against RPCS3 before the next.
Before each phase, tell me the plan, what "done" means, and how you'll verify it.

0. **Lab**: extract, decrypt, inventory with hashes, Ghidra, capture scripts. Settle the
   number of executables and modules, SPU program count and roles, the main archive
   formats, the script data format, and how well Ghidra handles this executable. Report
   honestly what works before building on it.
1. **Data**: archive and asset formats (streaming packages, textures, meshes, animation,
   audio, scripts). Rust parsers proven by round trip (parse, re-serialize, byte-compare)
   on my copy, and plug a disc source into the reused asset viewer. I check textures and
   audio by eye and ear.
2. **Ground truth**: deterministic captures from savestates with recorded input: frames,
   audio, RSX command streams, and SPU job inputs and outputs. Kept in the lab, never
   committed.
3. **PPU**: static recompilation with OS and library calls routed to our runtime.
   Decision gate first: with evidence, decide what is recompiled and what is
   hand-written, and report it to me. Milestone: main menu.
4. **SPU**: rewrite each SPU job in Rust on the CPU, one at a time, each diffed against
   captured inputs and outputs, then optimise. Order the jobs by what the main menu and
   the first level need. **Inference:** this is the largest phase.
5. **Renderer and audio**: RSX on wgpu, validated with image diffs against RPCS3; audio
   validated against loopback captures.
6. **Vertical slices**: main menu, then the first level on desktop, then the rest. Then a
   Bevy host and a mobile build as separate slices, if I still want them.

## How to work with me

- I'm the hands and eyes for playing, visual and listening checks, and any menu click RPCS3
  needs. Give me exact steps.
- Keep `docs/STATUS.md` (works / unknown / blocked) and a claims ledger with statuses
  `confirmed / negative / unknown / hypothesis`, where "unknown" is never merged into
  "negative". Also keep a `docs/LESSONS.md` log like the trial's.
- Each coherent piece of work goes on its own branch and PR. I merge.
- Be honest about uncertainty. Don't give time estimates. Tell me you'll notify me when a
  run finishes or needs me, and what would stop it.
- Speak English only. I often leave the PC on overnight, so batch everything you need
  from me into one message before I go. My local time is roughly UTC-7.
- Start with Phase 0: list what you still need from me, what you'll check about the
  tooling first, and the repo layout.
