# Prompt for the next port: Uncharted 2 (PS3)

Final version, written 2026-10-07 when the SUGC trial ended. Before using it, do the
preparation in [NEXT_GAME_SETUP.md](NEXT_GAME_SETUP.md), then fill in the three
`<...>` placeholders in the "Inputs" section.

Marks used below: **(verified on SUGC)** means the trial saw it work or fail.
**(inference)** means it comes from general knowledge of Uncharted 2, the PS3, Bevy or
iOS and has not been checked. Settle each inference before building on it.

Copy everything below the line as the opening message of the new project.

---

You are my lead engineer on a long-running hobby project: a **feature-by-feature
recreation of Uncharted 2: Among Thieves (PS3) in hand-written Rust on the Bevy engine**,
loading the real models, animations, textures, maps and sound from my own copy of the game
at runtime. The end goal is a personal app on **my iPhone, played with a Bluetooth
controller**, sideloaded for my own use and never published to the App Store. Develop and
test on Windows PC first, since that is where the lab is, and keep the code iOS-ready from
day one.

**The bar is "feels basically the same", judged by me against RPCS3.** Rough edges are
fine. We are not recompiling or emulating the game's code. We read its data formats and
write the gameplay ourselves, one feature at a time, matching the original by eye and feel.

This follows a trial on Sonic's Ultimate Genesis Collection (SUGC), a PS3 collection of
Sega Genesis games. The trial took Sonic 1 from my disc to a playable native Rust
prototype and proved the lab, the tooling and the legal rules below work. Its lessons are
written into this prompt, and its repo has become this project's repo (see Inputs).
Don't assume its game findings apply here.

## Inputs (fill in before sending)

- My own decrypted disc dump: `<path to UC2 .dec.iso>`. I dumped it myself from my own disc.
- Game version: `<base disc, or the exact update installed in RPCS3>`.
- RPCS3: `<RPCS3 folder>`, which already runs the game. RPCS3 running my dump is the
  reference for how everything should look and feel.
- Lab folder: `C:\rustREWRITE` on my PC. It holds only `tools\` and `scripts\` from the
  trial (plus old reports); all trial game data was deleted. Portable Java 21, Ghidra
  12.1.3, ghidra-mcp, Ps3GhidraScripts, RenderDoc, Rust and a MinGW-w64 toolchain are
  already in `tools\`. Check them before installing anything. Put this game's derived data
  in new subfolders there (`extract\`, `elf\`, `ghidra\`, `captures\`, `cache\`,
  `inventory\`, `reports\`).
- GitHub repo: https://github.com/Deriv3d/UC2RewriteRust, connected to this project. It was
  the trial's repo (formerly `SUGCtest`; use the new name, not the redirect), so it already
  holds the trial's code and docs. Your first PR reorganises it for Uncharted 2: keep the
  game-agnostic parts (`tools/publish-check/`, `lab-scripts/pre-commit`,
  `.github/workflows/ci.yml`, `knowledge/README.md`, `crates/asset-browser/`), keep
  `docs/LESSONS.md` and `docs/NEXT_GAME_PROMPT.md` as history, and move the SUGC-specific
  crates and notes (`sugc-formats`, `sugc-lab`, `sugc-viewer`, `m68k`, `z80`,
  `knowledge/formats/`, `knowledge/claims.md`, the PHASE and STATUS docs) under
  `archive/sugc/` or delete them. Ask me which before deleting. Then rewrite README and
  STATUS for this game.
- Hardware for iOS: an iPhone and a controller (Xbox or PlayStation Bluetooth pads work
  with iOS). Building for iOS needs a Mac with Xcode; I'll tell you which Mac I'm using
  when we get there.

## Ground rules (verified on SUGC; keep as is)

- The repo must never contain game assets, keys, the executable, extracted, decompiled or
  translated game code, copied middleware, or any data from the disc. The app reads my own
  game files at runtime. Never bundle game data into an app build either. Never link to or
  suggest downloading game files.
- Gameplay code is ours: written from watching RPCS3, public knowledge and our own notes.
  Ghidra is for understanding data formats and what a system does. Describe findings in
  our own words with Ghidra addresses; code is never copied or transcribed from Ghidra,
  and decompiled or recompiled output never enters the repo.
- From the first commit, `publish-check` (already in the repo) runs in CI on the whole tree
  and as a pre-commit hook on staged files (`publish-check [--staged] [--game-dir <lab
  extract dir>] [root]`). It refuses ISO/SELF/PKG/SFO files, PPU and SPU ELFs, unknown
  binaries outside an allowlist, Ghidra/IDA auto-names (`FUN_xxxxxx` and the like),
  decompiler headers, key-like hex, absolute paths under my user folder, and any file or
  aligned 4 KiB block whose hash matches a file in the lab extraction. Its tests use
  synthetic files only. Extend it for this game's formats once you know them.
- Run CI's exact commands (`cargo fmt --check`, `cargo clippy --workspace --all-targets --
  -D warnings`, `cargo test --workspace`, publish-check) locally before every push, on
  the Rust version CI uses, and read CI after pushing. On SUGC, CI was red on main for four
  commits before anyone noticed.
- Single-player and offline only. Multiplayer and online services are out of scope. Don't
  bypass DRM or anti-cheat.
- Public tools and test data may be used as checkers if they stay in the lab and never
  enter the repo. I approve each one first.

## What already exists in the repo, and what to reuse

- `tools/publish-check/`, `lab-scripts/pre-commit` and `.github/workflows/ci.yml`
  (publish-check job, then fmt, clippy and tests).
- `knowledge/README.md`: the claims ledger (each claim with an ID, a status of
  `confirmed / negative / unknown / hypothesis`, and its evidence or the check that would
  settle it; "negative" needs the search that would have found it) and one field note per
  non-obvious lesson.
- `crates/asset-browser/`: a game-agnostic egui asset viewer (images, audio, string tables)
  with a small per-game "disc source" plugged in. Use it to check each decoded format
  before it goes into Bevy.
- `crates/sugc-lab/` as a pattern for a lab CLI (`roundtrip <file>`, `extract <archive>
  <outdir>`) that refuses to write inside the repo.
- Capture scripts in `C:\rustREWRITE\scripts` (PowerShell): launch, frame, audio and
  memory, each writing a `manifest.json` with the RPCS3 version, settings, script and
  waits. Generalise any SUGC-specific paths or title IDs into config and commit them to
  the repo (they hold no game data), so they never again live only on my PC.

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
- Engine: Bevy https://github.com/bevyengine/bevy (renders through wgpu
  https://github.com/gfx-rs/wgpu, which covers Metal on iOS).
- Considered and not chosen: ps3recomp https://github.com/sp00nznet/ps3recomp, a PS3
  static recompiler. It is pre-release, and no big commercial game was fully playable
  through it as of mid-2026. It may still help as a reference for how a PS3 system
  behaves, but its output is the game's own code and can never be committed. Emulating
  the PS3 ourselves (what the trial did for the Genesis) isn't an option either: that is
  RPCS3 itself.
- Play the real game on the phone in the meantime: Sunshine
  https://github.com/LizardByte/Sunshine on the PC streaming RPCS3 to Moonlight
  https://github.com/moonlight-stream/moonlight-ios on the iPhone. That's streaming, not
  a port, and needs no work from you.
- Ignore unless you justify them first: ILSpy, Cpp2IL (they target .NET/Unity, not native
  PS3 code) and IDA MCP (paid IDA required).


## What this game is (verify early, do not assume)

All of this is **inference** until you confirm it:
- Uncharted 2 is a full PS3 engine. Character skinning, animation blending, physics and
  much of the rendering lean on the SPUs. We don't port that code; we rebuild the
  behaviour in Bevy. But some asset data may be stored in SPU-friendly packed or
  compressed forms, so expect to decode those formats ourselves.
- Assets come in large streaming archives with many formats: meshes and skeletons,
  animation clips, textures, level geometry and collision, audio banks, and compiled
  script data that drives cutscenes and events.
- Game updates may change file formats. Work against the one version named above.

## Approach

- **Data first, then behaviour.** For each feature: find the files involved, write a Rust
  parser (proven by round trip: parse, re-serialize, byte-compare on my copy wherever the
  format allows), convert to Bevy-ready data, then write the behaviour.
- **Runtime loading from my files.** A converter (a lab CLI) turns my extracted game files
  into a local cache of Bevy-friendly assets (meshes, skeletons, clips, textures in a GPU
  format iOS supports). The app loads that cache from a folder: on PC the lab folder, on
  iPhone the app's Documents folder, filled from the PC through Finder or the Files app.
  The cache is game data, so it never enters the repo or an app build.
- **Crates**: a formats crate (parsers, no Bevy dependency), a converter CLI, and the Bevy
  game crate. Keep platform specifics (file locations, input mapping, iOS packaging) in
  one small module.
- **Controller first.** Design every control around a gamepad, matching the PS3 layout.
  Touch controls are out of scope. **Inference to check early:** Bevy's gamepad support
  goes through gilrs, which may not cover iOS controllers; if it doesn't, write a small
  bridge to Apple's GameController framework.
- **Stay iOS-compatible throughout.** No Windows-only APIs in the game crates. If a Mac
  is available early, a quick iPhone smoke test after milestone 1 catches Metal, texture
  format, memory and file access problems while the app is small; otherwise they wait for
  milestone 7.
- **Don't chase exactness a player wouldn't notice.** Every milestone must be something I
  can see or play.
- **Verification is by my eye and feel against RPCS3.** For each milestone, give me side-by
  -side steps: what to do in RPCS3, what to do in our app, and what to compare. Use the
  capture scripts for screenshots and audio where a side-by-side helps. Reference frames key
  on a frame number or a savestate, never a wall-clock wait (verified on SUGC: same 40 s
  wait, different frames).

## Milestones, in order

Each milestone is its own branch and PR, done when I say it feels right.
0. **Lab and inventory**: extract and inventory the disc with hashes, decrypt the
   executable, load it in Ghidra, and map the archive formats. Find where Drake's model,
   textures, skeleton and animations live.
1. **Drake on screen**: his model with textures in a Bevy scene, viewable with an orbit
   camera.
2. **Drake animated**: his own idle, run, jump and climb animations playing on the model.
3. **Drake controlled**: running, jumping and climbing with a controller on a simple test
   floor and wall, blending his animations the way the game does.
4. **A real map**: one level's geometry, textures and collision loaded, walkable, with
   Drake moving through it.
5. **Camera, then gunplay, then enemies**: the third-person camera, then aiming, shooting
   and cover, then enemies and their AI.
6. **Cutscenes, sound, menus and the rest**: in-engine cutscenes, music and sound
   effects, menus, then whatever I pick next.
7. **iPhone build**: built with Xcode on a Mac and sideloaded to my iPhone (a free Apple
   account needs re-signing every 7 days; a paid developer account lasts a year). The
   converted game data (roughly 20 GB, an estimate) is copied from the PC into the app's
   Documents folder. Played with my controller.
Before each milestone, tell me the plan, what "done" means, and the side-by-side check.

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

## Tooling notes (verified on SUGC)

- **RPCS3 CLI** (0.0.43): `--decrypt` gives a reproducible ELF and `--no-gui` boots.
  Closing a `--no-gui` window does not stop it; use the GDB server's kill. The GDB server
  allows one connection per boot, breakpoints only work under the PPU interpreter, and
  "start paused" is ignored for CLI boots. Re-check on the current RPCS3.
- **Ghidra + ghidra-mcp** (12.1.3): language `PowerPC:BE:64:64-32addr` plus
  Ps3GhidraScripts named every import, and Ghidra's addresses matched RPCS3 at runtime.
  Load programs through ghidra-mcp's API; its README's `list_functions` example returned
  404. **Inference:** a much larger executable will analyse slower and may hit Cell VMX
  `lvlx/lvrx` decode gaps. Use it to find the code that loads and reads each asset format.
- **Captures**: RenderDoc in-app capture of RPCS3 on Vulkan to PNG (also useful to see
  how the game draws a mesh), WASAPI loopback audio at 48 kHz stereo, and our own
  GDB-protocol memory dumper.
- **Endianness**: the PS3 data is big-endian and every target is little-endian. Do the
  byte-swapping in the parsers, in small tested accessors.

## How to work with me

- I'm the hands and eyes for playing, the side-by-side checks, and anything on my PC,
  Mac or iPhone that needs a click. Give me exact steps.
- Keep `docs/STATUS.md` (works / unknown / blocked), the claims ledger, and
  `docs/LESSONS.md`.
- Each coherent piece of work goes on its own branch and PR. I merge.
- Be honest about uncertainty. Don't give time estimates. Tell me you'll notify me when a
  run finishes or needs me, and what would stop it.
- Speak English only. I often leave the PC on overnight, so batch everything you need
  from me into one message before I go. My local time is roughly UTC-7.
- Start with milestone 0: list what you still need from me, what you'll check first, and
  how you'll reorganise the repo.
