# Disc file inventory

All 27 files on the game disc (BLUS30259), classified by leading magic, size and the code that
opens them. Code addresses are virtual addresses in the decrypted PPU executable as loaded by
Ghidra 12.1.3 (`PowerPC:BE:64:64-32addr`). Evidence lives in the lab folder (`ghidra\filerefs.txt`,
`inventory\files.csv`).

Status: **understood** = we can parse and rebuild it, or it is a standard system file the game
never opens itself. **partial** = container known, contents not yet decoded. **unknown** = not
yet analysed.

| Path | Size (bytes) | Format | Opened by | Status |
| --- | ---: | --- | --- | --- |
| `PS3_DISC.SFB` | 1,536 | PS3 disc descriptor (system) | firmware only | understood |
| `PS3_GAME/PARAM.SFO` | 1,040 | PS3 system parameter file (system) | firmware; game reads values via `cellGame` (0x00053548) | understood |
| `PS3_GAME/ICON0.PNG` | 17,994 | PNG (XMB icon) | firmware; name also referenced at 0x00040bc0 (save-data setup) | understood |
| `PS3_GAME/PIC0.PNG` | 63,681 | PNG (XMB overlay) | firmware only | understood |
| `PS3_GAME/PIC1.PNG` | 1,235,464 | PNG (XMB background) | firmware; name also referenced at 0x00040bc0 | understood |
| `PS3_GAME/PIC2.PNG` | 17,865 | PNG (XMB) | firmware only | understood |
| `PS3_GAME/PS3LOGO.DAT` | 5,120 | PNG signature, PS3 boot-logo data (system) | firmware only | understood |
| `PS3_GAME/SND0.AT3` | 1,684,392 | RIFF / ATRAC3 (XMB background audio) | firmware only | understood |
| `PS3_GAME/TROPDIR/NPWR00584_00/TROPHY.TRP` | 1,748,304 | Trophy pack (system) | `sceNpTrophy` library | understood |
| `PS3_GAME/USRDIR/EBOOT.BIN` | 2,277,872 | SELF executable | firmware loader | understood (decrypted in lab, never in repo) |
| `PS3_GAME/USRDIR/flog_u.fpg` | 40,404,992 | **FPG archive**, 147 entries ([fpg.md](fpg.md)) | archive loader state machine 0x00054ae8, async reads via 0x000d1758; lookups via 0x00054430 | understood |
| `PS3_GAME/USRDIR/flog_c.fpg` | 413,696 | **FPG archive**, 12 entries ([fpg.md](fpg.md)) | same as above | understood |
| `PS3_GAME/USRDIR/pad.file` | 293,601,280 | all zero bytes (disc layout padding) | nothing (no reference in the executable) | understood |
| `PS3_GAME/USRDIR/fmv/interview_one.mp4` | 92,450,758 | MP4 (`mp42` brand) | `fmv/%s.mp4` built at 0x000fd1c0 / 0x000fd290 / 0x000fd350, played through `cellSail` | partial |
| `PS3_GAME/USRDIR/fmv/interview_two.mp4` | 76,786,006 | MP4 (`mp42`) | as above | partial |
| `PS3_GAME/USRDIR/fmv/interview_three.mp4` | 35,189,893 | MP4 (`mp42`) | as above | partial |
| `PS3_GAME/USRDIR/fmv/interview_four.mp4` | 71,942,880 | MP4 (`mp42`) | as above | partial |
| `PS3_GAME/USRDIR/fmv/interview_five.mp4` | 86,680,882 | MP4 (`mp42`) | as above | partial |
| `PS3_GAME/USRDIR/fmv/interview_six.mp4` | 95,499,952 | MP4 (`mp42`) | as above | partial |
| `PS3_GAME/USRDIR/fmv/interview_seven.mp4` | 35,900,680 | MP4 (`mp42`) | as above | partial |
| `PS3_GAME/USRDIR/fmv/interview_eight.mp4` | 76,873,023 | MP4 (`mp42`) | as above | partial |
| `PS3_GAME/USRDIR/fmv/main0001.mp4` | 29,477,384 | MP4 (`mp42`) | as above | partial |
| `PS3_GAME/USRDIR/fmv/sgc2_attract.mp4` | 22,642,190 | MP4 (`mp42`) | as above | partial |
| `PS3_GAME/USRDIR/sounds/music/retro_dreams.msf` | 2,411,904 | Sony MSF stream (`MSFC` magic) | `sounds/music` at 0x000eabf8, `.msf` at 0x000ee918, then the generic file layer 0x00056b70 | partial |
| `PS3_GAME/USRDIR/streams/ui.spr` | 12,722,176 | SPR stream container ([spr.md](spr.md)): chunked zlib, 4 streams | `Streams\ui.spr` at 0x0003fa10; path builder 0x0003f7e0 | partial |
| `PS3_GAME/USRDIR/streams/global_binary.spr` | 200,704 | SPR stream container ([spr.md](spr.md)): chunked zlib, 2 streams | path builder 0x0003f7e0 (`%s%s.%s`) | partial |
| `PS3_UPDATE/PS3UPDAT.PUP` | 268,435,456 | PS3 system update package (system) | firmware only | understood |

## The game's file layer

- **Virtual file object.** Its open method is at 0x000f2030, with vtable read 0x000f1d78, size 0x000f1c28 and close 0x000f1f78.
  - Open strips the path down to its file name and asks the two resident FPG archives first (0x00054430).
  - It only falls back to opening the file on disc if the archives don't have it. That goes through the generic `cellFs` wrapper at 0x00056b70 / 0x000568c8.
  - So any game asset name may be served from `flog_u.fpg` or `flog_c.fpg` rather than the disc.
- **Raw `cellFs` callers.** `cellFsOpen` is called directly from only four places:
  - 0x00014338: firmware ATRAC/MP3 plugin loader
  - 0x000568c8: generic file layer
  - 0x00056b70: generic file layer
  - 0x000d1758: async archive streaming
- **Names the executable builds that aren't top-level disc files.** `fonts\%s.rf`, `fonts\%s.album%s`, `0B/SARC_%s.BND`, `0D/ic_%s.png`, `%s.68K`, `STR_*.TXT`. These are looked up in the archives first; see [fpg.md](fpg.md).

## Next unknowns, by size
1. `streams/ui.spr` (12.7 MB): UI container, next format to analyse.
2. `retro_dreams.msf`: MSF header layout and codec.
3. MP4 codec parameters. Decoding will use a standard decoder, not a port of `cellSail`.
