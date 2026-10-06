# FPG archive (`flog_u.fpg`, `flog_c.fpg`)

The two `.fpg` files in `USRDIR` are flat, hash-addressed archives of zlib-compressed files.
`flog_u.fpg` (147 entries) holds the bundled console and arcade ROM images, box/screen art
and sound effects. `flog_c.fpg` (12 entries) holds a few UI images, text and nine small
binaries of one shared kind.

Status: **understood**. Our parser in `crates/sugc-formats` reads, rebuilds and round-trips
both archives byte for byte (see "Verification").

## Layout

All integers are **little-endian** u32. That's unusual for a PS3 title, and the game must
byte-swap them in memory (see "Open questions").

| Offset | Size | Field |
| ---: | ---: | --- |
| 0x0000 | 4 | magic: the ASCII characters `3`,`0`,`G`,`F` |
| 0x0004 | 4 | entry count *N* |
| 0x0008 | 0x7F8 | zero |
| 0x0800 | 16 × *N* | entry table |
| align up to 0x800 | | entry data, in table order |

Entry table record, 16 bytes:

| Offset | Field |
| ---: | --- |
| +0x0 | name hash (see below) |
| +0x4 | absolute file offset of the stored data, a multiple of 0x800 |
| +0x8 | stored size: the zlib stream length rounded **up** to a multiple of 0x800 |
| +0xC | uncompressed size |

Rules seen on both real archives, with no exceptions:
- **Data area.** It starts at the first 0x800 boundary at or after the end of the table: 0x1800 for 147 entries, 0x1000 for 12.
- **Order and padding.** Entries are stored back to back in table order. Each one is padded with zero bytes up to the next 0x800 boundary, and the stored-size field counts that padding. The file ends exactly at the end of the last padded entry.
- **Compression.** Every entry is zlib-compressed, even when that makes it larger: 60 of 147 entries in `flog_u` have stored size ≥ uncompressed size once padding is counted. Every stream uses the default-level zlib header.
  - Recompressing each decompressed entry with stock zlib at level 6 reproduces the original stream byte for byte (159 of 159 across both archives).
  - The game links zlib 1.2.3 and calls the one-shot `uncompress`.
- **Table order.** Entries aren't sorted by hash, and lookup is a linear scan. The archive stores no file names, only hashes.

## Name hash

`hash(name) = CRC-32(upper(name with '\' replaced by '/'))`
- CRC-32 is the standard reflected CRC: polynomial 0xEDB88320, initial value and final XOR 0xFFFFFFFF, the same as zlib's `crc32`.
- `upper` is ASCII upper-casing.
- The game passes only the **file name**: the virtual-file layer strips any directory before asking the archive.

Evidence: hashing strings taken from the executable named 69 of 147 entries in `flog_u` and 3 of 12 in `flog_c`. Name recovery for the rest needs names from other data files.

## What's inside

Classified from each entry's decompressed contents (no content copied here):

**`flog_u.fpg`, 147 entries:**

| Kind | Count | How identified |
| --- | ---: | --- |
| Mega Drive / Genesis ROM images | 40 | console header at 0x100; all named `<game>.68K` |
| Other `.68K` images without a console header | 6 | named `.68K`, binary |
| Master System / Game Gear ROM images | 2 | `TMR SEGA` header; named `.SMS` |
| Arcade board ROM images (`.ROM`, `.PROM`) | 16 | named; 14 `.ROM` + 2 `.PROM` |
| Z80 program images | 9 | 5 named `.Z80`/`.Z81`, 4 unnamed with the same Z80 start-up pattern |
| PNG images, 320×176 | 49 | PNG signature (box/screen art), names not yet recovered |
| WAV sound effects, PCM 16-bit mono 22,050 Hz | 17 | RIFF/WAVE |
| Other unnamed binaries | 8 | 6 zero-led and 2 others, not yet identified |

The brief's "105 non-ROM entries" (147 − 40 − 2) break down as: 6 + 16 + 9 arcade/aux program images, 49 PNG, 17 WAV and 8 unidentified. Strictly, 31 of those 105 are still ROM-like code images for the arcade and extra boards.

**`flog_c.fpg`, 12 entries:**
- 2 PNG images (320×176)
- 1 text file (named `.SR`)
- 9 localized UI string tables, one per language/region variant ([strings.md](strings.md)). An earlier guess that they were fonts was wrong.

Of the 40 Genesis images, **38 pass their internal header checksum and 2 don't**. The header's ROM-end field matches the image size in all 40. Two failures is normal for some retail cartridges; it doesn't by itself mean they were patched.

## How the game loads and uses it

Addresses are in the decrypted executable as loaded by Ghidra 12.1.3.

1. **Load at boot.**
   - A 10-state loader driven from 0x00054ce8 / 0x00054ae8 reads each archive whole into memory with asynchronous reads (0x000d1758, `cellFsAioRead`). `flog_u` is read first, then `flog_c`.
   - A stage counter (global 0x00241640) gates use: `flog_u` lookups are enabled from stage 3 (0x00054160), `flog_c` from stage 8 (0x00054140).
   - The resident archive pointers are globals 0x00241610 and 0x00241628. Init and free are at 0x000546b0.
2. **Lookup.**
   - The virtual-file `Open` (0x000f2030) strips the path to its file name, then calls the archive lookup at 0x00054430: once for `flog_u`, and if that misses, once for `flog_c`.
   - The lookup lower-cases the name and turns `\` into `/` (0x000552b8), hashes it (0x000554a0, the upper-case CRC-32 above), and scans the table.
   - On a hit it allocates the uncompressed size, then inflates with zlib `uncompress` (wrapper 0x00083878, zlib 1.2.3).
   - If the caller passes in a buffer that's too small, it logs an error and fails.
   - The inflated buffer becomes the file object's in-memory backing; reads (0x000f1d78) are then plain copies.
   - On a miss, `Open` falls back to the disc through the generic `cellFs` layer (0x00056b70).
3. **ROM hand-off to the emulator.**
   - Each bundled Genesis game has a small setup routine; there are 40 of them, at 0x0012f2a8 … 0x0014c430.
   - Each one calls the shared machine-builder at 0x0014dd40 with up to three ROM names, one per region. A region global (0x0024143c, read by 0x000fa3c8) picks one, falling back to whichever names exist.
   - The builder formats `<name>.68K` and loads it through 0x000fa998, a virtual-file "read whole file" helper (open, size, read, close). That returns the inflated buffer and its size.
   - The buffer is mapped **unchanged** as cartridge ROM:
     - 68000 address space: mirrored up to 4 MiB in pages of the ROM's size.
     - 64 KiB work RAM: mirrored over 0xE00000–0xFFFFFF.
     - Z80 window: 0xA00000–0xA0FFFF.
     - I/O: 0xA10000.
     - VDP ports: 0xC00000.
   - A second copy of the ROM backs a second memory map. The Z80 gets its 8 KiB RAM mirrored, plus a 32 KiB banked window into the ROM.
   - For images larger than 0x204000 bytes, a slice from that offset is copied into a 64 KiB side buffer. For images up to 2 MiB, 0x200000–0x203FFF is mapped to that buffer: a save/backup-RAM style window.
   - Audio is set up with a 7,670,453 Hz master clock (NTSC 68000) and 48 kHz output.
   - **We found no header rewrite or byte patch** between inflate and mapping in this path.
4. **Per-game hooks (break slots).**
   - After the machine is built, 39 of the 40 per-game routines register one or more PPU callbacks at fixed 68000 addresses through 0x0015cd80. Example: the Sonic the Hedgehog routine at 0x0013a770 registers one at 0x01C5DC. Seven other routines also call it, including 0x00142810, 0x00143d20, 0x00148900 and 0x001414d8 (the second `.68K` users).
   - The core has 48 such slots.
   - Registering a slot saves the original opcode and its decoder entry, then writes the 16-bit word 0x4848 at that address in the core's **instruction-fetch copy** of the ROM. Removing the slot restores the original word.
   - When the 68000 fetches that word, the core calls the PPU callback and then runs the saved original instruction.
   - The data copy the 68000 reads from is never modified.
   - What the callbacks do (trophies, saves, frontend events) is not analysed yet.
   - A port can reproduce the hooks as PC-address callbacks in our own core without touching ROM bytes.
5. **Other users of `%s.68K`.** Two more routines build `.68K` names: 0x00193860 and 0x0019a530. They're probably the lock-on cartridge or extra-content paths. They aren't analysed yet.

## Verification
- **Round trip.** `sugc-lab roundtrip` parses each archive, rebuilds it from the parsed entries and byte-compares it with the original. Results are recorded in `docs/STATUS.md`.
- **ROM in guest memory, 2026-10-06.** RPCS3 with the lab config, Sonic the Hedgehog started from the menu, guest heap dumped over GDB (94 MiB).
  - The 512 KiB image appears twice. One copy is **byte-identical** to our extracted entry.
  - The other is identical except for **one 16-bit word at ROM offset 0x1C5DC**: the break-slot opcode described above, at exactly the address the Sonic routine registers.
  - So the ROM bytes come straight from the archive. The only runtime change is the core's hook marker.

## Open questions
- **Byte order.** Where exactly the little-endian table is byte-swapped after load. The lookup reads native big-endian words, so a swap must happen in the loader states. Status: hypothesis.
- **Unrecovered names.** The names of the 78 unnamed `flog_u` entries and 9 unnamed `flog_c` entries.
- **Unidentified entries.** What the 8 unidentified binaries in `flog_u` are.
